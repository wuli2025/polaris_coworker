
# ① 优先 winget (能拿最新 LTS, 自带配 PATH)
$wg = Get-Command winget -ErrorAction SilentlyContinue
if ($wg) {
  Write-Output '检测到 winget, 优先用它安装 Node.js LTS...'
  & winget install --id OpenJS.NodeJS.LTS -e --source winget --accept-package-agreements --accept-source-agreements
  if ($LASTEXITCODE -eq 0) { Write-Output 'Node.js (winget) 安装完成。'; exit 0 }
  Write-Output ('winget 安装未成功 (退出码 ' + $LASTEXITCODE + '), 改用直接下载 MSI...')
} else {
  Write-Output '未检测到 winget (Windows 10 常见), 改用直接下载官方 MSI...'
}
# ② 下载 Node LTS MSI -> %TEMP% -> msiexec 静默安装。自家 R2 打头, 后面依次 npmmirror、官方直连。
$ver = 'NODE_VER'
$arch = switch ($env:PROCESSOR_ARCHITECTURE) { 'ARM64' { 'arm64' } 'AMD64' { 'x64' } default { 'x86' } }
$msi = "node-v$ver-$arch.msi"
$dst = Join-Path $env:TEMP $msi
$urls = @(
  "DEPS_BASE/$msi",
  "https://cdn.npmmirror.com/binaries/node/v$ver/$msi",
  "https://npmmirror.com/mirrors/node/v$ver/$msi",
  "https://nodejs.org/dist/v$ver/$msi"
)
Get-PolarisFile -Urls $urls -Dest $dst -Magic 'MSI_MAGIC' -MinBytes 15MB
if (-not $global:PolarisDlOk) {
  Write-Output 'Node.js 安装包下载失败 (可检查网络 / 代理后重试)。'
  exit 1
}
Write-Output "安装中 (msiexec, 会弹一次 UAC 授权): $dst"
Install-PolarisMsi -Path $dst
Remove-Item $dst -ErrorAction SilentlyContinue
if ($global:PolarisMsiCode -ne 0) { Write-Output ('msiexec 退出码 ' + $global:PolarisMsiCode); exit 1 }
Write-Output 'Node.js 安装完成。'
