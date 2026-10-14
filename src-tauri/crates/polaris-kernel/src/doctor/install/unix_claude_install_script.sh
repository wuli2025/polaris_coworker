
VER='CLAUDE_VER'
OS=$(uname -s)
ARCH=$(uname -m)
case "$ARCH" in
  arm64|aarch64) A=arm64 ;;
  x86_64|amd64)  A=x64 ;;
  *)             A=x64 ;;
esac
case "$OS" in
  Darwin) PLAT="darwin-${A}" ;;
  Linux)
    # musl(Alpine 等) 与 glibc 各有各的包, 拿错了解压出来也跑不起来
    if ldd --version 2>&1 | grep -qi musl; then PLAT="linux-${A}-musl"; else PLAT="linux-${A}"; fi ;;
  *) echo "不支持的系统: $OS"; exit 1 ;;
esac
PKG="claude-code-${PLAT}"
TGZ="${PKG}-${VER}.tgz"
BIN="$HOME/.local/bin"
mkdir -p "$BIN"
TMP="$(mktemp -d)"
# 多源, 失败域各不相同: R2(自托管) → npmmirror(阿里) → yarn(CF 镜像) → npmjs(官方)。
# 每个源**各自端到端自证**: 下载 → gzip 魔数 → 解压 → `--version` 真跑一次, 任一步不过就换下
# 一个源 (挡住「下坏但仍是 gzip」「架构不符」「包结构变了」)。下载与落盘分开。
URLS="DEPS_BASE/$TGZ https://registry.npmmirror.com/@anthropic-ai/${PKG}/-/${TGZ} https://registry.yarnpkg.com/@anthropic-ai/${PKG}/-/${TGZ} https://registry.npmjs.org/@anthropic-ai/${PKG}/-/${TGZ}"
VERIFIED=""
for ROUND in 1 2; do
  [ -n "$VERIFIED" ] && break
  for U in $URLS; do
    echo "下载: $U"
    TARBALL="$TMP/dl.tgz"
    if ! curl -fsSL --retry 2 --retry-delay 2 "$U" -o "$TARBALL"; then echo "  下载失败, 换下一个源..."; continue; fi
    if [ ! -s "$TARBALL" ]; then echo "  空文件, 换下一个源..."; continue; fi
    if [ "$(head -c 2 "$TARBALL" | od -An -tx1 | tr -d ' \n')" != "1f8b" ]; then echo "  非 gzip (多半是错误页), 换下一个源..."; continue; fi
    ST="$TMP/stage"; rm -rf "$ST"; mkdir -p "$ST"
    if ! tar -xzf "$TARBALL" -C "$ST" --strip-components=1 2>/dev/null; then echo "  解压失败, 换下一个源..."; continue; fi
    if [ ! -f "$ST/claude" ]; then echo "  解压后没有 claude, 换下一个源..."; continue; fi
    chmod +x "$ST/claude" 2>/dev/null || true
    if ! "$ST/claude" --version >/dev/null 2>&1; then echo "  claude 跑不起来 (下坏 / 架构不符), 换下一个源..."; continue; fi
    VERIFIED="$TMP/claude.verified"
    cp -f "$ST/claude" "$VERIFIED"; chmod +x "$VERIFIED"
    break
  done
  if [ -z "$VERIFIED" ] && [ "$ROUND" = "1" ]; then echo "一轮源都没成, 歇 2 秒再来一轮..."; sleep 2; fi
done
if [ -z "$VERIFIED" ]; then echo "Claude Code 所有源都失败 (检查网络/代理后重试)。"; rm -rf "$TMP"; exit 1; fi
# 同目录临时文件 + mv 原子替换 —— cp -f 会 open(O_TRUNC) 就地截断, 会让**正在运行**的旧
# claude 二进制崩(SIGBUS)。mv 是 unlink+rename, 旧进程照抱着已被 unlink 的 inode 跑完不受影响。
NEW="$BIN/.claude.new.$$"
cp -f "$VERIFIED" "$NEW" && chmod +x "$NEW" && mv -f "$NEW" "$BIN/claude" || { echo "落盘失败。"; rm -f "$NEW"; rm -rf "$TMP"; exit 1; }
rm -rf "$TMP"
# 把 ~/.local/bin 写进 shell 配置 (与 uv/node 一致), 已存在则不重复
LINE="export PATH=\"$BIN:\$PATH\""
for RC in "$HOME/.zshrc" "$HOME/.zprofile" "$HOME/.bash_profile" "$HOME/.profile" ; do
  touch "$RC" 2>/dev/null || true
  grep -qF "$BIN" "$RC" 2>/dev/null || printf '\n# Added by Polaris (Claude Code)\n%s\n' "$LINE" >> "$RC"
done
echo "Claude Code 安装完成: $("$BIN/claude" --version 2>/dev/null || echo "(已装到 $BIN/claude)")"
