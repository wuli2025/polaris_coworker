
$ver = 'NODE_VER'
$arch = switch ($env:PROCESSOR_ARCHITECTURE) { 'ARM64' { 'arm64' } 'AMD64' { 'x64' } default { 'x86' } }
$zip = "node-v$ver-win-$arch.zip"
$dst = Join-Path $env:TEMP $zip
$urls = @(
  "DEPS_BASE/$zip",
  "https://cdn.npmmirror.com/binaries/node/v$ver/$zip",
  "https://npmmirror.com/mirrors/node/v$ver/$zip",
  "https://nodejs.org/dist/v$ver/$zip"
)
Get-PolarisFile -Urls $urls -Dest $dst -Magic 'ZIP_MAGIC' -MinBytes 15MB
if (-not $global:PolarisDlOk) { Write-Output 'Node.js 下载失败 (可检查网络 / 代理后重试)。'; exit 1 }
$dest = Join-Path $env:USERPROFILE '.local\polaris-node'
$stage = Join-Path $env:TEMP ('polaris-node-stage-' + [guid]::NewGuid().ToString('N'))
try {
  Expand-Archive -Path $dst -DestinationPath $stage -Force
} catch {
  Write-Output ('解压失败: ' + $_.Exception.Message); Remove-Item $dst -Force -ErrorAction SilentlyContinue; exit 1
}
Remove-Item $dst -Force -ErrorAction SilentlyContinue
# zip 内是 node-v<ver>-win-<arch>\ 一层, 提出来
$inner = Get-ChildItem $stage -Directory | Select-Object -First 1
if (-not $inner) { Write-Output '解压后没找到 Node 目录。'; exit 1 }
if (Test-Path $dest) { Remove-Item $dest -Recurse -Force -ErrorAction SilentlyContinue }
New-Item -ItemType Directory -Force -Path (Split-Path $dest -Parent) | Out-Null
Move-Item $inner.FullName $dest
Remove-Item $stage -Recurse -Force -ErrorAction SilentlyContinue
$node = Join-Path $dest 'node.exe'
if (-not (Test-Path $node)) { Write-Output '解压后未找到 node.exe。'; exit 1 }
Write-Output ('Node.js (便携版) 安装完成: ' + (& $node -v) + ' -> ' + $dest)
