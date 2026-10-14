
# ① 优先 winget (能拿最新版, 自带配 PATH)
$wg = Get-Command winget -ErrorAction SilentlyContinue
if ($wg) {
  Write-Output '检测到 winget, 优先用它安装 PowerShell 7...'
  & winget install --id Microsoft.PowerShell -e --source winget --accept-package-agreements --accept-source-agreements
  if ($LASTEXITCODE -eq 0) { Write-Output 'PowerShell 7 (winget) 安装完成。'; exit 0 }
  Write-Output ('winget 安装未成功 (退出码 ' + $LASTEXITCODE + '), 改用直接下载 MSI...')
} else {
  Write-Output '未检测到 winget, 改用直接下载官方 MSI...'
}
# ② 下载 MSI -> %TEMP% -> msiexec 静默安装。**自家 R2 排第一** —— 公共 GitHub 代理时好时坏
#    (实测 gh-proxy.com 已 500, 故直接摘掉), 而 R2 的包是发版时自己传上去、字节数与官方逐一
#    核对过的, 出站还免费。后面仍留公共代理与 GitHub 直连兜底: R2/CF 万一挂了也装得上。
#    (arm64/x64 在 R2 里都有; 32 位 x86 没镜像 → R2 那跳 404, 自动落到后面的源, 不影响。)
$ver = 'PWSH_VER'
$arch = switch ($env:PROCESSOR_ARCHITECTURE) { 'ARM64' { 'arm64' } 'AMD64' { 'x64' } default { 'x86' } }
$msi = "PowerShell-$ver-win-$arch.msi"
$dst = Join-Path $env:TEMP $msi
$rel = "https://github.com/PowerShell/PowerShell/releases/download/v$ver/$msi"
$urls = @(
  "DEPS_BASE/$msi",
  "https://ghfast.top/$rel",
  "https://ghproxy.net/$rel",
  $rel
)
Get-PolarisFile -Urls $urls -Dest $dst -Magic 'MSI_MAGIC' -MinBytes 40MB
if (-not $global:PolarisDlOk) {
  Write-Output 'PowerShell 7 安装包下载失败 (可检查网络 / 代理后重试)。'
  exit 1
}
Write-Output "安装中 (msiexec, 会弹一次 UAC 授权): $dst"
Install-PolarisMsi -Path $dst -ExtraArgs 'ADD_PATH=1'
Remove-Item $dst -ErrorAction SilentlyContinue
if ($global:PolarisMsiCode -ne 0) { Write-Output ('msiexec 退出码 ' + $global:PolarisMsiCode); exit 1 }
Write-Output 'PowerShell 7 安装完成。'
