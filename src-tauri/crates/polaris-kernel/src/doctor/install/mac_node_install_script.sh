
VER=20.18.1
ARCH=$(uname -m)
case "$ARCH" in
  arm64|aarch64) NARCH=arm64 ;;
  x86_64) NARCH=x64 ;;
  *) NARCH=x64 ;;
esac
PKG="node-v${VER}-darwin-${NARCH}.tar.gz"
DEST="$HOME/.local"
NODE_DIR="$DEST/polaris-node"
TMP="$(mktemp -d)"
TARBALL="$TMP/$PKG"
echo "目标架构: $NARCH; Node 版本: v$VER"
OK=0
for U in \
  "https://cdn.npmmirror.com/binaries/node/v${VER}/${PKG}" \
  "https://npmmirror.com/mirrors/node/v${VER}/${PKG}" \
  "https://nodejs.org/dist/v${VER}/${PKG}" ; do
  echo "下载: $U"
  if curl -fsSL "$U" -o "$TARBALL" && [ -s "$TARBALL" ]; then OK=1; break; fi
  echo "  下载失败, 试下一个镜像..."
done
if [ "$OK" != "1" ]; then echo "Node.js 下载失败 (检查网络/代理后重试)。"; rm -rf "$TMP"; exit 1; fi
mkdir -p "$DEST"
rm -rf "$NODE_DIR"
mkdir -p "$NODE_DIR"
tar -xzf "$TARBALL" -C "$NODE_DIR" --strip-components=1 || { echo "解压失败。"; rm -rf "$TMP"; exit 1; }
rm -rf "$TMP"
BIN="$NODE_DIR/bin"
if [ ! -x "$BIN/node" ]; then echo "解压后未找到 node 可执行文件。"; exit 1; fi
# 把 node bin 写进 shell 配置 (zsh 为 macOS 默认; 同时照顾 bash/sh), 已存在则不重复
LINE="export PATH=\"$BIN:\$PATH\""
for RC in "$HOME/.zshrc" "$HOME/.zprofile" "$HOME/.bash_profile" "$HOME/.profile" ; do
  touch "$RC" 2>/dev/null || true
  grep -qF "$BIN" "$RC" 2>/dev/null || printf '\n# Added by Polaris (Node.js)\n%s\n' "$LINE" >> "$RC"
done
export PATH="$BIN:$PATH"
echo "Node.js 安装完成: node $("$BIN/node" -v), npm $("$BIN/npm" -v)"
echo "已写入 ~/.zshrc 等; 本次会话已即时生效。"
