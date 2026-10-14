
$arch = switch ($env:PROCESSOR_ARCHITECTURE) { 'ARM64' { 'aarch64' } 'AMD64' { 'x86_64' } default { 'x86_64' } }
$ver = 'UV_VER'
$asset = "uv-$arch-pc-windows-msvc.zip"
$rel = "https://github.com/astral-sh/uv/releases/download/$ver/$asset"
$urls = @(
  "DEPS_BASE/$asset",
  "https://ghfast.top/$rel",
  "https://ghproxy.net/$rel",
  $rel
)
$dst = Join-Path $env:TEMP $asset
Get-PolarisFile -Urls $urls -Dest $dst -Magic 'ZIP_MAGIC' -MinBytes 5MB
if (-not $global:PolarisDlOk) { Write-Output 'uv 下载失败 (可检查网络 / 代理后重试)。'; exit 1 }
$bin = Join-Path $env:USERPROFILE '.local\bin'
New-Item -ItemType Directory -Force -Path $bin | Out-Null
try {
  Expand-Archive -Path $dst -DestinationPath $bin -Force
} catch {
  Write-Output ('解压失败: ' + $_.Exception.Message); Remove-Item $dst -ErrorAction SilentlyContinue; exit 1
}
Remove-Item $dst -ErrorAction SilentlyContinue
$uv = Join-Path $bin 'uv.exe'
if (-not (Test-Path $uv)) { Write-Output 'uv.exe 解压后未找到。'; exit 1 }
# 国内镜像配置: 仅当用户尚无 uv.toml 时写入, 尊重既有配置
$cfgDir = Join-Path $env:APPDATA 'uv'
$cfg = Join-Path $cfgDir 'uv.toml'
if (-not (Test-Path $cfg)) {
  New-Item -ItemType Directory -Force -Path $cfgDir | Out-Null
  $toml = @'
# 由北极星写入: Python 解释器与依赖走国内镜像, 国内可达。删掉本文件即恢复 uv 默认源。
python-install-mirror = "https://gh-proxy.com/https://github.com/astral-sh/python-build-standalone/releases/download"
index-url = "https://pypi.tuna.tsinghua.edu.cn/simple"
'@
  Set-Content -Path $cfg -Value $toml -Encoding UTF8
  Write-Output "已写入 uv 国内镜像配置: $cfg"
}
Write-Output ("uv 安装完成: " + (& $uv --version))
Write-Output "已装到 $bin —— 重启后终端可直接用; 本应用内 Claude 立即可用 (uv run)。"
