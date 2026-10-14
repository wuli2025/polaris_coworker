
ARCH=$(uname -m)
case "$ARCH" in
  arm64|aarch64) UARCH=aarch64 ;;
  x86_64) UARCH=x86_64 ;;
  *) UARCH=x86_64 ;;
esac
ASSET="uv-${UARCH}-apple-darwin.tar.gz"
VER='UV_VER'
REL="https://github.com/astral-sh/uv/releases/download/${VER}/${ASSET}"
TMP="$(mktemp -d)"
TARBALL="$TMP/$ASSET"
OK=0
# 自家 R2 打头 (发版时传上去、字节数与官方核对过), 公共代理与 GitHub 直连兜底。
# 每个源都校验 gzip 魔数 (1f8b) —— 代理挂掉时常回「200 + HTML 错误页」, 只看「文件非空」会把
# 错误页当安装包喂给 tar, 报一句看不懂的错。
for U in \
  "DEPS_BASE/$ASSET" \
  "https://ghfast.top/$REL" \
  "https://ghproxy.net/$REL" \
  "$REL" ; do
  echo "下载: $U"
  if curl -fsSL --retry 2 --retry-delay 2 "$U" -o "$TARBALL" && [ -s "$TARBALL" ]; then
    if [ "$(head -c 2 "$TARBALL" | od -An -tx1 | tr -d ' \n')" = "1f8b" ]; then OK=1; break; fi
    echo "  文件头不是 gzip —— 多半是代理回的错误页, 换下一个源..."
  else
    echo "  下载失败, 试下一个镜像..."
  fi
done
if [ "$OK" != "1" ]; then echo "uv 下载失败 (检查网络/代理后重试)。"; rm -rf "$TMP"; exit 1; fi
BIN="$HOME/.local/bin"
mkdir -p "$BIN"
tar -xzf "$TARBALL" -C "$BIN" --strip-components=1 || { echo "解压失败。"; rm -rf "$TMP"; exit 1; }
rm -rf "$TMP"
if [ ! -x "$BIN/uv" ]; then echo "uv 解压后未找到。"; exit 1; fi
# 国内镜像配置: 仅当用户尚无 uv.toml 时写入
CFG="$HOME/.config/uv/uv.toml"
if [ ! -f "$CFG" ]; then
  mkdir -p "$HOME/.config/uv"
  cat > "$CFG" <<'EOF'
# 由北极星写入: Python 解释器与依赖走国内镜像, 国内可达。删掉本文件即恢复 uv 默认源。
python-install-mirror = "https://gh-proxy.com/https://github.com/astral-sh/python-build-standalone/releases/download"
index-url = "https://pypi.tuna.tsinghua.edu.cn/simple"
EOF
  echo "已写入 uv 国内镜像配置: $CFG"
fi
# 把 ~/.local/bin 写进 shell 配置 (zsh 为 macOS 默认), 已存在则不重复
LINE="export PATH=\"$BIN:\$PATH\""
for RC in "$HOME/.zshrc" "$HOME/.zprofile" "$HOME/.bash_profile" "$HOME/.profile" ; do
  touch "$RC" 2>/dev/null || true
  grep -qF "$BIN" "$RC" 2>/dev/null || printf '\n# Added by Polaris (uv)\n%s\n' "$LINE" >> "$RC"
done
echo "uv 安装完成: $("$BIN/uv" --version)"
echo "已写入 ~/.zshrc 等; 本应用内 Claude 立即可用 (uv run)。"
