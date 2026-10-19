# syntax=docker/dockerfile:1
# ═══════════════════════════════════════════════════════════════
# Polaris: Node + Pi agentd by default; bundled Pi is the release backend.
# ── 镜像源开关（默认官方源；国内构建机传 --build-arg 切国内源加速）──
#   APT_MIRROR=mirrors.ustc.edu.cn
#   NPM_REGISTRY=https://registry.npmmirror.com
#   CARGO_SPARSE_INDEX=sparse+https://rsproxy.cn/index/
#   NODE_DIST_BASE=https://registry.npmmirror.com/-/binary/node

# ── stage 1: 前端 ──────────────────────────────────────────────
ARG NODE_VERSION=22.19.0
FROM node:${NODE_VERSION}-bookworm-slim AS web
ARG NPM_REGISTRY=
RUN if [ -n "$NPM_REGISTRY" ]; then npm config set -g registry "$NPM_REGISTRY"; fi
WORKDIR /build
COPY package.json package-lock.json ./
RUN npm ci
COPY index.html vite.config.ts tsconfig.json tsconfig.node.json ./
COPY src ./src
COPY public ./public
# build = vue-tsc --noEmit && vite build → dist/
RUN npm run build

# Production agent daemon; never run dependency lifecycle scripts.
FROM node:${NODE_VERSION}-bookworm-slim AS agent
ARG NPM_REGISTRY=
RUN apt-get update && apt-get install -y --no-install-recommends libxcb1 && rm -rf /var/lib/apt/lists/*
WORKDIR /build/agentd
COPY agentd/package.json agentd/package-lock.json ./
RUN if [ -n "$NPM_REGISTRY" ]; then npm config set -g registry "$NPM_REGISTRY"; fi
RUN npm ci --ignore-scripts --no-audit --no-fund
COPY agentd/tsconfig.json ./
COPY agentd/src ./src
COPY agentd/config ./config
COPY agentd/test ./test
COPY scripts/pack-agentd.mjs /build/scripts/pack-agentd.mjs
# Copy the whole helper directory: pack-agentd's imports change over time (a fixed file list broke the build once).
COPY scripts/runtime/ /build/scripts/runtime/
RUN node /build/scripts/pack-agentd.mjs /out/agentd

# ── stage 2: Rust 服务端 ───────────────────────────────────────
# 1.85 会被 lock 里 darling@0.23(要 1.88)/icu 2.x(要 1.86)拒编；本机 1.95 实测过,钉同版
FROM rust:1.95-bookworm AS server
ARG APT_MIRROR=
RUN if [ -n "$APT_MIRROR" ]; then sed -i "s|deb.debian.org|$APT_MIRROR|g" /etc/apt/sources.list.d/debian.sources; fi
# crates.io 稀疏索引镜像（如 rsproxy）。留空走官方。
ARG CARGO_SPARSE_INDEX=
RUN if [ -n "$CARGO_SPARSE_INDEX" ]; then printf '[source.crates-io]\nreplace-with = "mirror"\n\n[source.mirror]\nregistry = "%s"\n' "$CARGO_SPARSE_INDEX" > "$CARGO_HOME/config.toml"; fi
# openh264(source feature)/audiopus 等原生构建链；libssl-dev 备用
# Acquire::Retries:代理/镜像源偶发 502,重试扛过去(实测踩过)
RUN apt-get -o Acquire::Retries=5 update && apt-get -o Acquire::Retries=5 install -y --no-install-recommends \
        pkg-config libssl-dev cmake nasm clang \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /build
# 整个 src-tauri 进上下文（.dockerignore 已排除 src-tauri/target 等）。
# 注意本地 path 依赖：crates/polaris-core、crates/forge-codec、crates/polaris-cli
# 以及 include_dir!/include_str! 内嵌的 src/templates、assets/、voice-libs 等，
# 都在 src-tauri/ 之内，整目录 COPY 一次到位。
COPY src-tauri ./src-tauri
COPY agentd/package.json ./agentd/package.json
# ★ src-tauri 之外的内嵌资源:beam.rs 用 include_str!("../../docs/beam-guide.html")
#   把这份说明编进二进制。只 COPY src-tauri 会在 stage2 直接编译失败
#   (`couldn't read src/../../docs/beam-guide.html`)—— 2026-07-28 实测踩到。
#   新增跨出 src-tauri 的 include_* 时,这里要跟着加。
COPY docs/beam-guide.html ./docs/beam-guide.html
WORKDIR /build/src-tauri
# ★ 关键：polaris-server 的 [[bin]] 不在主包（tauri bundler 连坐问题，47d1e0c），
#   而在 workspace 成员 crates/polaris-cli；它依赖
#   polaris-app { default-features = false, features = ["server"] }，
#   故 -p polaris-cli 即等价于文档里的 --no-default-features --features server。
# 注意括号:|| true 只容忍 strip 失败,绝不能吞 cargo 的失败(踩过——binary not found 才炸)
# --features collab-net:让 polaris-server 起 iroh host_listen 暴露 NodeId(P2P 直连 + fsface 远程盘)。
RUN --mount=type=secret,id=POLARIS_BUNDLED_MINIMAX_KEY,env=POLARIS_BUNDLED_MINIMAX_KEY cargo build --release -p polaris-cli --bin polaris-server --features collab-net \
    && (strip target/release/polaris-server || true)

# ── stage 3: 运行层 ────────────────────────────────────────────
FROM node:${NODE_VERSION}-bookworm-slim
ARG POLARIS_BUILD_REVISION=local
ARG POLARIS_BUILD_VERSION=0.0.0
ARG APT_MIRROR=
RUN if [ -n "$APT_MIRROR" ]; then sed -i "s|deb.debian.org|$APT_MIRROR|g" /etc/apt/sources.list.d/debian.sources; fi
# bookworm 自带 git 2.39 ≥ 2.38（merge-tree 冲突试算可用）
RUN apt-get -o Acquire::Retries=5 update && apt-get -o Acquire::Retries=5 install -y --no-install-recommends \
        git git-lfs ca-certificates curl openssl tini libxcb1 \
    && rm -rf /var/lib/apt/lists/*
COPY --from=agent /out/agentd /app/runtime/agentd

# 非 root 运行；数据根 = $HOME/Polaris（server 用 ~/Polaris 当工作目录，
# collab.db 落 ~/Polaris/data/；claude 凭证落 ~/.claude）
# The node base image already owns UID 1000 ("node"); keep UID 1000 for existing data volumes.
RUN userdel -r node 2>/dev/null; useradd -m -u 1000 -s /bin/bash polaris \
    && mkdir -p /home/polaris/Polaris /home/polaris/.claude /home/polaris/.polaris/agentd /srv/web /app/resources \
    && printf '%s\n' "$POLARIS_BUILD_REVISION" > /app/polaris-build-revision \
    && printf '%s\n' "$POLARIS_BUILD_VERSION" > /app/polaris-build-version \
    && chown -R polaris:polaris /home/polaris /srv/web /app/resources

COPY --from=web        --chown=polaris:polaris /build/dist /srv/web
COPY --from=server     --chown=polaris:polaris /build/src-tauri/target/release/polaris-server /usr/local/bin/polaris-server
COPY --from=server     --chown=polaris:polaris /build/src-tauri/resources /app/resources
# 更新脚本只通过内网调用隔离的 Watchtower HTTP API；本镜像既不含 Docker CLI，
# 也永不挂宿主机 docker.sock（socket 仅属于可选 updater sidecar）。
# package.json 仅供旧的 update.sh --check 人工诊断；产品更新页读取 OCI config 的 build revision。
COPY --chown=polaris:polaris package.json /app/package.json
COPY --chmod=0755 docker/update.sh /usr/local/bin/update.sh

ENV POLARIS_AGENT_BACKEND=pi \
    POLARIS_AGENTD_NODE=/usr/local/bin/node \
    POLARIS_AGENTD_ENTRY=/app/runtime/agentd/dist/src/main.js \
    POLARIS_PORT=8080 \
    POLARIS_WEB_DIR=/srv/web \
    POLARIS_RESOURCE_DIR=/app/resources \
    POLARIS_VERSION_FILE=/app/package.json \
    POLARIS_BUILD_REVISION=${POLARIS_BUILD_REVISION} \
    POLARIS_BUILD_VERSION=${POLARIS_BUILD_VERSION} \
    POLARIS_IMAGE_REPO=ghcr.io/wuli2025/polaris_coworker \
    HOME=/home/polaris

USER polaris
WORKDIR /home/polaris
VOLUME ["/home/polaris/Polaris", "/home/polaris/.claude", "/home/polaris/.polaris/agentd"]
EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
    CMD curl -fsS http://localhost:8080/api/health || exit 1

ENTRYPOINT ["/usr/bin/tini", "--"]
CMD ["/usr/local/bin/polaris-server"]
