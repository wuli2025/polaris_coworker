
$ver = 'CLAUDE_VER'
$arch = switch ($env:PROCESSOR_ARCHITECTURE) { 'ARM64' { 'arm64' } 'AMD64' { 'x64' } default { 'x64' } }
$pkg = "claude-code-win32-$arch"
$tgz = "$pkg-$ver.tgz"
$bin = Join-Path $env:USERPROFILE '.local\bin'
New-Item -ItemType Directory -Force -Path $bin | Out-Null
$exe = Join-Path $bin 'claude.exe'
# 清掉历次「旧 claude 运行中让路」遗留的 .old-*（那些进程多半早退出了，此刻能删就删）
Get-ChildItem (Join-Path $bin 'claude.exe.old-*') -ErrorAction SilentlyContinue | Remove-Item -Force -ErrorAction SilentlyContinue

# 多源, 失败域各不相同: R2(自托管 / CF) → npmmirror(阿里) → yarn(CF 镜像) → npmjs(官方)。
# 四家不同公司、不同网络路径, 任一活着就能装。R2 打头 = 不开梯子最快。
$urls = @(
  "DEPS_BASE/$tgz",
  "https://registry.npmmirror.com/@anthropic-ai/$pkg/-/$tgz",
  "https://registry.yarnpkg.com/@anthropic-ai/$pkg/-/$tgz",
  "https://registry.npmjs.org/@anthropic-ai/$pkg/-/$tgz"
)

# ── 阶段一: 从任一源拿到一个「下得下来、解得开、还真能跑」的 claude.exe ──
# 每个源**各自端到端自证**(下载→体积→gzip 魔数→解压→`--version` 真跑一次), 任一步不过就换下
# 一个源。比只校验哈希更强: 能挡住「下坏但恰好 >30MB 还是 gzip 头」「架构拿错」「包结构变了」,
# 且零维护(不用每版更新哈希)。下载与落盘分开 —— 占锁不该让人白下 80MB。
$verified = Join-Path $env:TEMP ('polaris-claude-verified-' + [guid]::NewGuid().ToString('N') + '.exe')
$ver_out = $null
$got = $false
foreach ($round in 1..2) {
  foreach ($u in $urls) {
    if ($got) { break }
    $dst = Join-Path $env:TEMP $tgz
    $stage = Join-Path $env:TEMP ('polaris-cc-stage-' + [guid]::NewGuid().ToString('N'))
    try {
      Write-Output "下载: $u"
      if (Test-Path $dst) { Remove-Item $dst -Force -ErrorAction SilentlyContinue }
      Invoke-WebRequest -Uri $u -OutFile $dst -UseBasicParsing -TimeoutSec 600
      $len = (Get-Item $dst).Length
      if ($len -lt 30MB) { Write-Output ('  体积不对 {0:N0} 字节 (多半是代理错误页), 换下一个源' -f $len); continue }
      $fs = [IO.File]::OpenRead($dst); $hb = New-Object byte[] 2; $null = $fs.Read($hb, 0, 2); $fs.Close()
      $magic = ($hb | ForEach-Object { $_.ToString('x2') }) -join ''
      if ($magic -ne 'GZ_MAGIC') { Write-Output ('  文件头 {0} 不是 gzip, 换下一个源' -f $magic); continue }
      # 解压到独立 stage (tar.exe = bsdtar, Win10 1803+ 自带; --strip-components=1 去掉 package\ 一层)
      New-Item -ItemType Directory -Force -Path $stage | Out-Null
      & tar.exe -xzf $dst -C $stage --strip-components=1
      $cand = Join-Path $stage 'claude.exe'
      if (-not (Test-Path $cand)) { Write-Output '  解压后没有 claude.exe (包坏了 / 结构变了), 换下一个源'; continue }
      # 真跑一次: 证明是能用的、架构对的二进制, 而不是坏包 / 错架构
      $out = & $cand --version 2>$null
      if ($LASTEXITCODE -ne 0 -or -not $out) { Write-Output '  解压出的 claude 跑不起来 (下坏 / 架构不符), 换下一个源'; continue }
      Copy-Item $cand $verified -Force
      $ver_out = $out
      $got = $true
    } catch {
      Write-Output ('  下载 / 校验失败: ' + $_.Exception.Message + ', 换下一个源')
    } finally {
      Remove-Item $dst -Force -ErrorAction SilentlyContinue
      Remove-Item $stage -Recurse -Force -ErrorAction SilentlyContinue
    }
  }
  if ($got) { break }
  if ($round -eq 1) { Write-Output '一轮源都没成, 歇 2 秒再来一轮...'; Start-Sleep -Seconds 2 }
}
if (-not $got) { Write-Output 'Claude Code 所有源都失败 (可检查网络 / 代理后重试)。'; exit 1 }

# ── 阶段二: 落盘 (只做一次; 与「重试下载源」分开, 避免因占锁白下 80MB) ──
$installed = $false
# 首选直接覆盖（全新装 / 旧文件没被占用都走这条）
try { Copy-Item $verified $exe -Force -ErrorAction Stop; $installed = $true }
catch {
  # 覆盖被拒 = 旧 claude 正在运行、镜像被锁。用**唯一后缀**改名让路（不与遗留 .old 撞）, 再覆盖。
  # (Windows 允许给「正在运行的镜像」改名, 这正是能原地更新的关键; 改名后 $exe 空出来即可写新的。)
  $old = "$exe.old-" + [guid]::NewGuid().ToString('N')
  try {
    Move-Item $exe $old -Force -ErrorAction Stop
    Copy-Item $verified $exe -Force -ErrorAction Stop
    $installed = $true
    Remove-Item $old -Force -ErrorAction SilentlyContinue  # 进程还活着会删不掉, 无妨, 下次再清
  } catch {
    Write-Output ('无法替换正在运行的 claude.exe: ' + $_.Exception.Message + ' — 请结束正在进行的对话后重试。')
  }
}
Remove-Item $verified -Force -ErrorAction SilentlyContinue
# 关键: 只有真装成功才算成功。绝不在「旧文件还在」时误报(否则用户以为装了新版, 其实是旧版)。
if (-not $installed) { exit 1 }
Write-Output ('Claude Code 安装完成: ' + $ver_out)
