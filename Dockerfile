# ═══════════════════════════════════════════════════════════════
# Polaris 服务器镜像（多阶段构建）
#
# ⚠ 完整三段构建未在开发机跑过（stage2 的 Rust release 编译很慢）。首次构建若报错，见
#   DEPLOY-CLOUD.md「构建排错」一节。
#   已实测（2026-07-24，Docker 29.4.3）：**stage3 运行层的 Claude Code 安装段**在真
#   debian:bookworm-slim 里单独构建通过 —— 四源回落生效（R2 未传→被 gzip 魔数拦下→
#   落 npmmirror）、容器内 `claude --version` 正常、**非 root(uid 1000) 下同样正常**、
#   `ldd` 确认只依赖基础 glibc（librt/libc/libpthread/libdl/libm），无 libstdc++/libgcc 依赖。
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
RUN cargo build --release -p polaris-cli --bin polaris-server --features collab-net \
    && (strip target/release/polaris-server || true)

# ── Docker CLI：仅供显式启用的容器自更新调用 ─────────────────────
# 直接复用官方多架构 CLI，避免在运行层安装完整 docker.io（体积大且带 daemon）。
FROM docker:27.5.1-cli AS docker-cli

# ── stage 3: 运行层 ────────────────────────────────────────────
FROM debian:bookworm-slim
ARG APT_MIRROR=
RUN if [ -n "$APT_MIRROR" ]; then sed -i "s|deb.debian.org|$APT_MIRROR|g" /etc/apt/sources.list.d/debian.sources; fi
# bookworm 自带 git 2.39 ≥ 2.38（merge-tree 冲突试算可用）
RUN apt-get -o Acquire::Retries=5 update && apt-get -o Acquire::Retries=5 install -y --no-install-recommends \
        git git-lfs ca-certificates curl openssl tini \
    && rm -rf /var/lib/apt/lists/*
# Node 20：给 Claude 写的 JS/TS 脚本当运行时。
# ⚠ 它**已不是 claude CLI 的运行时** —— claude 自 2.1.198 起是原生二进制，不调 Node
#   （官方原话：the installed claude binary does not itself invoke Node）。
# 官方 dist 直下 tar.gz（不走 nodesource 脚本，免 gnupg/apt 源注入，
# NODE_DIST_BASE 可切 npmmirror 国内加速）。按 uname 选架构 → x64/arm64 都能构建。
ARG NODE_VERSION=20.18.1
ARG NODE_DIST_BASE=https://nodejs.org/dist
ARG NPM_REGISTRY=
RUN set -eux; \
    case "$(uname -m)" in \
      x86_64|amd64)  NARCH=x64 ;; \
      aarch64|arm64) NARCH=arm64 ;; \
      *) echo "不支持的架构: $(uname -m)"; exit 1 ;; \
    esac; \
    curl -fsSL "$NODE_DIST_BASE/v$NODE_VERSION/node-v$NODE_VERSION-linux-$NARCH.tar.gz" \
      | tar -xz -C /usr/local --strip-components=1; \
    if [ -n "$NPM_REGISTRY" ]; then npm config set -g registry "$NPM_REGISTRY"; fi; \
    node -v; npm -v

# ── Claude Code：直抓 npm 平台包 tgz，**不走 `npm i -g`** ──────────────────────
# 为什么换掉 `npm i -g @anthropic-ai/claude-code`（三个实打实的毛病）：
#   ① 它要拉 ~80MB 的 optionalDependency 原生包，构建期常卡死/半途而废 → 整个镜像构建挂掉；
#   ② 不锁版本（隐式 latest）→ 每次重建拿到的 claude 都不一样，镜像不可复现；
#   ③ 只有一个源，npm 那边一抖就没得救。
# 现在：锁版本 + 四源逐个自证（下载 → gzip 魔数 → 解压 → 真跑一次 --version），
# 任一步不过就换下一个源；四源失败域各不相同，任一家活着就装得上：
#   R2(自托管/CF) → npmmirror(阿里) → yarn(CF 镜像) → npmjs(官方)
# 与桌面端 doctor::install 完全同一套规矩 —— **升级 claude 版本要两边一起改**
#   (src-tauri/crates/polaris-kernel/src/doctor/install.rs 的 CLAUDE_VER)。
# 注：debian:bookworm-slim 是 glibc，故取 linux-<arch>（非 -musl 包）。
ARG CLAUDE_VERSION=2.1.218
ARG DEPS_BASE=https://llmwiki.cloud/downloads/deps
RUN set -eu; \
    case "$(uname -m)" in \
      x86_64|amd64)  CARCH=x64 ;; \
      aarch64|arm64) CARCH=arm64 ;; \
      *) echo "不支持的架构: $(uname -m)"; exit 1 ;; \
    esac; \
    PKG="claude-code-linux-${CARCH}"; TGZ="${PKG}-${CLAUDE_VERSION}.tgz"; OK=0; \
    for U in "$DEPS_BASE/$TGZ" \
             "https://registry.npmmirror.com/@anthropic-ai/${PKG}/-/${TGZ}" \
             "https://registry.yarnpkg.com/@anthropic-ai/${PKG}/-/${TGZ}" \
             "https://registry.npmjs.org/@anthropic-ai/${PKG}/-/${TGZ}"; do \
      echo "下载 Claude Code: $U"; \
      curl -fsSL --retry 2 --retry-delay 2 "$U" -o /tmp/cc.tgz || { echo "  下载失败，换下一个源"; continue; }; \
      [ -s /tmp/cc.tgz ] || { echo "  空文件，换下一个源"; continue; }; \
      [ "$(head -c 2 /tmp/cc.tgz | od -An -tx1 | tr -d ' \n')" = "1f8b" ] \
        || { echo "  非 gzip（多半是代理/CDN 回的错误页），换下一个源"; continue; }; \
      rm -rf /tmp/ccx; mkdir -p /tmp/ccx; \
      tar -xzf /tmp/cc.tgz -C /tmp/ccx --strip-components=1 \
        || { echo "  解压失败，换下一个源"; continue; }; \
      [ -f /tmp/ccx/claude ] || { echo "  解压后没有 claude（包结构变了），换下一个源"; continue; }; \
      chmod +x /tmp/ccx/claude; \
      /tmp/ccx/claude --version || { echo "  跑不起来（下坏/架构不符），换下一个源"; continue; }; \
      install -m 0755 /tmp/ccx/claude /usr/local/bin/claude; OK=1; break; \
    done; \
    rm -rf /tmp/cc.tgz /tmp/ccx; \
    [ "$OK" = "1" ] || { echo "Claude Code 所有源都失败，构建中止"; exit 1; }; \
    claude --version

# 非 root 运行；数据根 = $HOME/Polaris（server 用 ~/Polaris 当工作目录，
# collab.db 落 ~/Polaris/data/；claude 凭证落 ~/.claude）
RUN useradd -m -u 1000 -s /bin/bash polaris \
    && mkdir -p /home/polaris/Polaris /home/polaris/.claude /srv/web /app/resources \
    && chown -R polaris:polaris /home/polaris /srv/web /app/resources

COPY --from=web        --chown=polaris:polaris /build/dist /srv/web
COPY --from=server     --chown=polaris:polaris /build/src-tauri/target/release/polaris-server /usr/local/bin/polaris-server
COPY --from=server     --chown=polaris:polaris /build/src-tauri/resources /app/resources
COPY --from=docker-cli /usr/local/bin/docker /usr/local/bin/docker
# package.json 是容器当前版本的唯一来源；update.sh 的 --check 与网页更新页都读它。
COPY --chown=polaris:polaris package.json /app/package.json
COPY --chmod=0755 docker/update.sh /usr/local/bin/update.sh

ENV POLARIS_PORT=8080 \
    POLARIS_WEB_DIR=/srv/web \
    POLARIS_RESOURCE_DIR=/app/resources \
    POLARIS_VERSION_FILE=/app/package.json \
    POLARIS_IMAGE_REPO=ghcr.io/wuli2025/polaris_coworker \
    POLARIS_CONTAINER_NAME=polaris-web \
    HOME=/home/polaris

USER polaris
WORKDIR /home/polaris
VOLUME ["/home/polaris/Polaris", "/home/polaris/.claude"]
EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
    CMD curl -fsS http://localhost:8080/api/health || exit 1

ENTRYPOINT ["/usr/bin/tini", "--"]
CMD ["/usr/local/bin/polaris-server"]
