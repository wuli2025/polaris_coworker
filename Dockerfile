# ═══════════════════════════════════════════════════════════════
# Polaris 服务器镜像（多阶段构建）
#
# ⚠ 未在开发机实测构建（本机 WSL 无 docker）。首次构建若报错，见
#   DEPLOY-CLOUD.md「构建排错」一节。
#
# stage1  node:20        → 前端 dist/
# stage2  rust:1.95      → polaris-server 二进制（bin 住在 crates/polaris-cli，
#                          经依赖恒开 server feature，无需 tauri/webkit）
# stage3  debian:slim    → 运行层：git≥2.38 + node20 + claude CLI + 非 root
# ═══════════════════════════════════════════════════════════════

# ── 镜像源开关（默认官方源；国内构建机传 --build-arg 切国内源加速）──
#   APT_MIRROR=mirrors.ustc.edu.cn
#   NPM_REGISTRY=https://registry.npmmirror.com
#   CARGO_SPARSE_INDEX=sparse+https://rsproxy.cn/index/
#   NODE_DIST_BASE=https://registry.npmmirror.com/-/binary/node

# ── stage 1: 前端 ──────────────────────────────────────────────
FROM node:20-bookworm-slim AS web
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
WORKDIR /build/src-tauri
# ★ 关键：polaris-server 的 [[bin]] 不在主包（tauri bundler 连坐问题，47d1e0c），
#   而在 workspace 成员 crates/polaris-cli；它依赖
#   polaris-app { default-features = false, features = ["server"] }，
#   故 -p polaris-cli 即等价于文档里的 --no-default-features --features server。
# 注意括号:|| true 只容忍 strip 失败,绝不能吞 cargo 的失败(踩过——binary not found 才炸)
RUN cargo build --release -p polaris-cli --bin polaris-server \
    && (strip target/release/polaris-server || true)

# ── stage 3: 运行层 ────────────────────────────────────────────
FROM debian:bookworm-slim
ARG APT_MIRROR=
RUN if [ -n "$APT_MIRROR" ]; then sed -i "s|deb.debian.org|$APT_MIRROR|g" /etc/apt/sources.list.d/debian.sources; fi
# bookworm 自带 git 2.39 ≥ 2.38（merge-tree 冲突试算可用）
RUN apt-get -o Acquire::Retries=5 update && apt-get -o Acquire::Retries=5 install -y --no-install-recommends \
        git git-lfs ca-certificates curl openssl tini \
    && rm -rf /var/lib/apt/lists/*
# Node 20（claude CLI 运行时）：官方 dist 直下 tar.gz（不走 nodesource 脚本，
# 免 gnupg/apt 源注入，且 NODE_DIST_BASE 可切 npmmirror 国内加速）→ 全局装 claude
ARG NODE_VERSION=20.18.1
ARG NODE_DIST_BASE=https://nodejs.org/dist
ARG NPM_REGISTRY=
RUN curl -fsSL "$NODE_DIST_BASE/v$NODE_VERSION/node-v$NODE_VERSION-linux-x64.tar.gz" \
        | tar -xz -C /usr/local --strip-components=1 \
    && if [ -n "$NPM_REGISTRY" ]; then npm config set -g registry "$NPM_REGISTRY"; fi \
    && npm i -g @anthropic-ai/claude-code \
    && npm cache clean --force

# 非 root 运行；数据根 = $HOME/Polaris（server 用 ~/Polaris 当工作目录，
# collab.db 落 ~/Polaris/data/；claude 凭证落 ~/.claude）
RUN useradd -m -u 1000 -s /bin/bash polaris \
    && mkdir -p /home/polaris/Polaris /home/polaris/.claude /srv/web /app/resources \
    && chown -R polaris:polaris /home/polaris /srv/web /app/resources

COPY --from=web    --chown=polaris:polaris /build/dist /srv/web
COPY --from=server --chown=polaris:polaris /build/src-tauri/target/release/polaris-server /usr/local/bin/polaris-server
COPY --from=server --chown=polaris:polaris /build/src-tauri/resources /app/resources

ENV POLARIS_PORT=8080 \
    POLARIS_WEB_DIR=/srv/web \
    POLARIS_RESOURCE_DIR=/app/resources \
    HOME=/home/polaris

USER polaris
WORKDIR /home/polaris
VOLUME ["/home/polaris/Polaris", "/home/polaris/.claude"]
EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
    CMD curl -fsS http://localhost:8080/api/health || exit 1

ENTRYPOINT ["/usr/bin/tini", "--"]
CMD ["/usr/local/bin/polaris-server"]
