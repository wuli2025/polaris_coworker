# ═══════════════════════════════════════════════════════════════
# 协作·任务卡检查工作流(CI-lite)对抗性端到端测试
#
# 用法(仓库根目录):
#   1) cargo build --manifest-path src-tauri\Cargo.toml -p polaris-cli --bin polaris-server
#   2) pwsh _collabtest\checks-adversarial.ps1
# 16 个场景驱动真实 polaris-server(端口 18081 + 临时库),断言坏码能否溜进 main。
# 每次改 collab/checks.rs 或合并闸后重放本脚本。历史:2026-07-07 揪出 round 漂移、
# Windows 缺失工具误判、中文名 C-quote 绕过、3MB 密钥漏扫、超时绕过等真 bug。
# ═══════════════════════════════════════════════════════════════
# 任务卡检查工作流 —— 对抗性端到端测试
# 驱动真实 polaris-server,每个场景独立 git 仓,断言「坏代码能不能溜进 main」。
$ErrorActionPreference = "Stop"
$PORT = 18081
$B = "http://127.0.0.1:$PORT"
$root = "$env:TEMP\checks-adv-$PID"
$db = "$env:TEMP\checks-adv-$PID.db"
New-Item -ItemType Directory -Force $root | Out-Null

$results = New-Object System.Collections.Generic.List[object]
function Rec($name, $pass, $detail) {
  $results.Add([pscustomobject]@{ name = $name; pass = $pass; detail = $detail })
  $tag = if ($pass) { "PASS" } else { "FAIL" }
  Write-Host "[$tag] $name  ::  $detail"
}

# ── HTTP 助手 ──
$script:tok = $null
function Api($method, $path, $body) {
  $h = @{}
  if ($script:tok) { $h["Authorization"] = "Bearer $script:tok" }
  $p = @{ Method = $method; Uri = "$B$path"; Headers = $h }
  if ($null -ne $body) { $p.ContentType = "application/json"; $p.Body = ($body | ConvertTo-Json -Compress -Depth 6) }
  try {
    $r = Invoke-RestMethod @p
    return [pscustomobject]@{ ok = $true; data = $r; error = $null }
  } catch {
    $msg = $_.ErrorDetails.Message
    if (-not $msg) { $msg = $_.Exception.Message }
    # 错误体是 JSON({"error":"..."}),且非 ASCII 被转成 \uXXXX。解出 .error 字段还原中文,
    # 否则下游 -match "陈旧" 匹配的是转义串而非真字符,永远不中。
    try { $msg = ($msg | ConvertFrom-Json).error } catch {}
    return [pscustomobject]@{ ok = $false; data = $null; error = $msg }
  }
}

# ── git 助手(forward-slash 路径,免 JSON 转义) ──
function Git($repo, $argline) {
  $out = & git.exe -C $repo @argline 2>&1
  if ($LASTEXITCODE -ne 0) { throw "git $argline 失败: $out" }
}
function NewRepo($name, [scriptblock]$seedMain) {
  $r = "$root/$name"
  New-Item -ItemType Directory -Force $r | Out-Null
  Git $r @("init", "-b", "main")
  Git $r @("config", "user.email", "t@test.local")
  Git $r @("config", "user.name", "tester")
  & $seedMain $r
  Git $r @("add", "-A"); Git $r @("commit", "-m", "init")
  return $r
}
$FAKEKEY = "AKIA" + "ABCDEFGHIJKLMNOP"   # 拼接,免本仓自扫误报

# ── 一条完整流程:建项目→建卡→claim→在分支铺内容→submit→等检查→review→尝试合并 ──
# 返回 @{ checks=<checks data>; merge=<api result> }
function RunFlow($repo, $profile, [scriptblock]$branchContent, $doReviewPass = $true, [scriptblock]$afterCheck = $null) {
  $proj = (Api POST "/api/collab/projects" @{ name = "P"; repo = ($repo -replace '\\', '/') }).data
  if ($profile -ne "code") { (Api POST "/api/collab/checks/profile" @{ projectId = $proj.id; profile = $profile }) | Out-Null }
  $task = (Api POST "/api/collab/tasks" @{ projectId = $proj.id; title = "T"; body = "b"; scope = "src/"; criteria = "1.ok" }).data
  $card = (Api POST "/api/collab/task/claim" @{ taskId = $task.id }).data
  $branch = $card.branch
  Git $repo @("checkout", "-q", "-b", $branch)
  & $branchContent $repo
  Git $repo @("add", "-A"); Git $repo @("commit", "-q", "-m", "work")
  Git $repo @("checkout", "-q", "main")
  (Api POST "/api/collab/task/submit" @{ taskId = $task.id }) | Out-Null
  # 等检查跑完。★必须「连续两次都无 running 且条数一致」才算稳定 —— 各步骤是串行的,
  # 上一步完成到下一步 insert running 之间有个瞬间「全部 terminal」的空窗,单次判定会误以为
  # 跑完(实际 cargo check 还没开始),导致抢在编译中就 merge、被闸正确拦下 → 假失败。
  $checks = $null
  $stableCount = -1
  for ($i = 0; $i -lt 150; $i++) {
    Start-Sleep -Milliseconds 800
    $c = Api GET "/api/collab/checks?taskId=$($task.id)"
    if ($c.ok -and $c.data.runs.Count -gt 0) {
      $running = @($c.data.runs | Where-Object { $_.status -eq "running" })
      if ($running.Count -eq 0) {
        if ($stableCount -eq $c.data.runs.Count) { $checks = $c.data; break }
        $stableCount = $c.data.runs.Count
      } else {
        $stableCount = -1
      }
    }
  }
  if ($afterCheck) { & $afterCheck $repo $branch }
  if ($doReviewPass) { (Api POST "/api/collab/task/review" @{ taskId = $task.id; pass = $true; comments = @() }) | Out-Null }
  return [pscustomobject]@{ taskId = $task.id; branch = $branch; checks = $checks; card = $card }
}
function TryMerge($taskId, $force = $false) {
  if ($force) { return Api POST "/api/collab/merge/squash" @{ taskId = $taskId; force = $true } }
  return Api POST "/api/collab/merge/squash" @{ taskId = $taskId }
}
function StatusOf($checks, $stepName) {
  if (-not $checks) { return "NO-CHECKS" }
  ($checks.runs | Where-Object { $_.name -eq $stepName } | Select-Object -First 1).status
}

# ═══════════════ 启动服务 ═══════════════
$env:POLARIS_COLLAB_DB = $db
$env:POLARIS_PORT = "$PORT"
Get-Process polaris-server -ErrorAction SilentlyContinue | Where-Object { $_.Path -like "*polaris-app*" } | Stop-Process -Force -ErrorAction SilentlyContinue
$srv = Start-Process -FilePath (Join-Path $PSScriptRoot "..\src-tauri\target\debug\polaris-server.exe") -PassThru -WindowStyle Hidden
Start-Sleep 5
$boot = Invoke-RestMethod -Method Post -Uri "$B/api/collab/bootstrap" -ContentType "application/json" -Body '{"username":"boss","password":"pass1234","displayName":"Boss","deviceId":"n1"}'
$script:tok = $boot.token
Write-Host "server up, owner token len=$($script:tok.Length)`n"

try {
  # S1 干净分支(无 manifest)→ 应合并成功
  $r = RunFlow (NewRepo "s1" { param($p) Set-Content "$p/README.md" "hi" }) "code" { param($p) Set-Content "$p/note.txt" "clean change" }
  $m = TryMerge $r.taskId
  Rec "S1 干净分支应可合并" ($m.ok -and $m.data.card.state -eq "merged") "merge.ok=$($m.ok) err=$($m.error)"

  # S2 分支引入密钥 → 应被闸住
  $r = RunFlow (NewRepo "s2" { param($p) Set-Content "$p/README.md" "hi" }) "code" { param($p) Set-Content "$p/leak.py" "aws = $FAKEKEY" }
  $m = TryMerge $r.taskId
  Rec "S2 密钥应 fail 且合并被拒" ((StatusOf $r.checks "密钥扫描") -eq "fail" -and -not $m.ok) "scan=$(StatusOf $r.checks '密钥扫描') mergeOk=$($m.ok) err=$($m.error)"

  # S3 分支引入 60MB 大文件(code 档 50MB 上限)→ 应被闸住
  $r = RunFlow (NewRepo "s3" { param($p) Set-Content "$p/README.md" "hi" }) "code" { param($p) & fsutil file createnew "$p/blob.bin" 62914560 | Out-Null }
  $m = TryMerge $r.taskId
  Rec "S3 大文件(code)应 fail 且被拒" ((StatusOf $r.checks "大文件闸") -eq "fail" -and -not $m.ok) "big=$(StatusOf $r.checks '大文件闸') mergeOk=$($m.ok)"

  # S4 同样 60MB 但 creative 档(500MB 上限)→ 应通过并合并
  $r = RunFlow (NewRepo "s4" { param($p) Set-Content "$p/README.md" "hi" }) "creative" { param($p) & fsutil file createnew "$p/blob.bin" 62914560 | Out-Null }
  $m = TryMerge $r.taskId
  Rec "S4 大文件(creative)应 pass 且合并" ((StatusOf $r.checks "大文件闸") -eq "pass" -and $m.ok) "big=$(StatusOf $r.checks '大文件闸') mergeOk=$($m.ok) err=$($m.error)"

  # S5 增量语义:main 上已有密钥(历史遗留),分支干净 → 不该被历史密钥挡住
  $r = RunFlow (NewRepo "s5" { param($p) Set-Content "$p/README.md" "hi"; Set-Content "$p/legacy.py" "old = $FAKEKEY" }) "code" { param($p) Set-Content "$p/feature.txt" "clean" }
  $m = TryMerge $r.taskId
  Rec "S5 历史密钥不挡干净新卡(增量)" ((StatusOf $r.checks "密钥扫描") -eq "pass" -and $m.ok) "scan=$(StatusOf $r.checks '密钥扫描') mergeOk=$($m.ok) err=$($m.error)"

  # S6 编译错误的 Rust → cargo check 应 fail 且被拒
  $r = RunFlow (NewRepo "s6" { param($p)
      Set-Content "$p/Cargo.toml" "[package]`nname=`"t`"`nversion=`"0.1.0`"`nedition=`"2021`"`n[[bin]]`nname=`"t`"`npath=`"main.rs`""
      Set-Content "$p/main.rs" "fn main(){}"
    }) "code" { param($p) Set-Content "$p/main.rs" "fn main(){ let x: i32 = `"broken`"; }" }
  $m = TryMerge $r.taskId
  Rec "S6 编译错误应 fail 且被拒" ((StatusOf $r.checks "cargo check") -eq "fail" -and -not $m.ok) "cargo=$(StatusOf $r.checks 'cargo check') mergeOk=$($m.ok)"

  # S7 合法 Rust → cargo check 应 pass 且合并(证明不是「见 rust 就 fail」)
  $r = RunFlow (NewRepo "s7" { param($p)
      Set-Content "$p/Cargo.toml" "[package]`nname=`"t2`"`nversion=`"0.1.0`"`nedition=`"2021`"`n[[bin]]`nname=`"t2`"`npath=`"main.rs`""
      Set-Content "$p/main.rs" "fn main(){}"
    }) "code" { param($p) Set-Content "$p/main.rs" "fn main(){ println!(`"ok`"); }" }
  $m = TryMerge $r.taskId
  Rec "S7 合法 Rust 应 pass 且合并" ((StatusOf $r.checks "cargo check") -eq "pass" -and $m.ok) "cargo=$(StatusOf $r.checks 'cargo check') mergeOk=$($m.ok) err=$($m.error)"

  # S8 npm lint 失败 → 应被拒
  $r = RunFlow (NewRepo "s8" { param($p) Set-Content "$p/package.json" '{"name":"t","version":"1.0.0","scripts":{"lint":"exit 1"}}' }) "code" { param($p) Set-Content "$p/a.js" "// change" }
  $m = TryMerge $r.taskId
  Rec "S8 npm lint 失败应被拒" ((StatusOf $r.checks "npm run lint") -eq "fail" -and -not $m.ok) "lint=$(StatusOf $r.checks 'npm run lint') mergeOk=$($m.ok)"

  # S9 off 档 + 密钥 → 闸整体跳过,应合并(确认 off 真的放行)
  $r = RunFlow (NewRepo "s9" { param($p) Set-Content "$p/README.md" "hi" }) "off" { param($p) Set-Content "$p/leak.py" "aws = $FAKEKEY" }
  $m = TryMerge $r.taskId
  Rec "S9 off 档密钥也放行" ($m.ok) "runs=$(@($r.checks.runs).Count) mergeOk=$($m.ok) err=$($m.error)"

  # S10 ★陈旧检查绕过:干净分支→检查过→偷偷再推密钥提交→尝试合并→应被 SHA 比对拒
  $r = RunFlow (NewRepo "s10" { param($p) Set-Content "$p/README.md" "hi" }) "code" `
    { param($p) Set-Content "$p/ok.txt" "clean" } $true `
    { param($repo, $branch)
      # 检查已跑完(干净通过),现在偷偷往分支塞一个带密钥的新提交
      Git $repo @("checkout", "-q", $branch)
      Set-Content "$repo/sneak.py" "aws = $FAKEKEY"
      Git $repo @("add", "-A"); Git $repo @("commit", "-q", "-m", "sneak secret after checks")
      Git $repo @("checkout", "-q", "main")
    }
  $m = TryMerge $r.taskId
  $blockedStale = (-not $m.ok) -and ($m.error -match "陈旧|新提交")
  Rec "S10 ★检查后偷推密钥应被 SHA 拒" $blockedStale "checkScan=$(StatusOf $r.checks '密钥扫描') mergeOk=$($m.ok) err=$($m.error)"

  # S11 owner force 强推被闸住的密钥分支 → 应成功合并(逃生舱有效)
  $r = RunFlow (NewRepo "s11" { param($p) Set-Content "$p/README.md" "hi" }) "code" { param($p) Set-Content "$p/leak.py" "aws = $FAKEKEY" }
  $mBlock = TryMerge $r.taskId
  $mForce = TryMerge $r.taskId $true
  Rec "S11 owner force 可强推" ((-not $mBlock.ok) -and $mForce.ok) "block=$($mBlock.ok) force=$($mForce.ok) err=$($mForce.error)"

  # S12 ruff 缺失 → 应 skipped(不误伤),干净 py 项目应可合并
  $ruffAbsent = -not (Get-Command ruff -ErrorAction SilentlyContinue)
  if ($ruffAbsent) {
    $r = RunFlow (NewRepo "s12" { param($p) Set-Content "$p/pyproject.toml" "[project]`nname=`"t`"`nversion=`"0.1`"" }) "code" { param($p) Set-Content "$p/m.py" "x = 1" }
    $m = TryMerge $r.taskId
    Rec "S12 ruff 缺失应 skipped 不误伤" ((StatusOf $r.checks "ruff check") -eq "skipped" -and $m.ok) "ruff=$(StatusOf $r.checks 'ruff check') mergeOk=$($m.ok)"
  } else {
    Rec "S12 ruff 缺失场景(跳过:本机装了 ruff)" $true "ruff present, skipped scenario"
  }

  # S13 并发提交序列化不崩:同时 submit 两卡,两边都应跑完出完整结果
  $repoA = NewRepo "s13a" { param($p) Set-Content "$p/README.md" "hi" }
  $repoB = NewRepo "s13b" { param($p) Set-Content "$p/README.md" "hi" }
  $pa = (Api POST "/api/collab/projects" @{ name = "PA"; repo = ($repoA -replace '\\','/') }).data
  $pb = (Api POST "/api/collab/projects" @{ name = "PB"; repo = ($repoB -replace '\\','/') }).data
  $ta = (Api POST "/api/collab/tasks" @{ projectId=$pa.id; title="A"; body="b"; scope="s"; criteria="c" }).data
  $tb = (Api POST "/api/collab/tasks" @{ projectId=$pb.id; title="B"; body="b"; scope="s"; criteria="c" }).data
  $ca = (Api POST "/api/collab/task/claim" @{ taskId=$ta.id }).data
  $cb = (Api POST "/api/collab/task/claim" @{ taskId=$tb.id }).data
  foreach ($x in @(@($repoA,$ca.branch),@($repoB,$cb.branch))) {
    Git $x[0] @("checkout","-q","-b",$x[1]); Set-Content "$($x[0])/c.txt" "x"; Git $x[0] @("add","-A"); Git $x[0] @("commit","-q","-m","w"); Git $x[0] @("checkout","-q","main")
  }
  (Api POST "/api/collab/task/submit" @{ taskId=$ta.id }) | Out-Null
  (Api POST "/api/collab/task/submit" @{ taskId=$tb.id }) | Out-Null
  Start-Sleep 6
  $okA = @((Api GET "/api/collab/checks?taskId=$($ta.id)").data.runs | Where-Object { $_.status -eq 'running' }).Count -eq 0
  $okB = @((Api GET "/api/collab/checks?taskId=$($tb.id)").data.runs | Where-Object { $_.status -eq 'running' }).Count -eq 0
  Rec "S13 并发提交都跑完不卡" ($okA -and $okB) "A完成=$okA B完成=$okB"

  # S14 worktree 清理:临时 worktree 目录不残留
  Start-Sleep 2
  $leftover = @(Get-ChildItem "$env:TEMP" -Directory -Filter "polaris-check-*" -ErrorAction SilentlyContinue).Count
  Rec "S14 worktree 检完即清无残留" ($leftover -eq 0) "残留目录数=$leftover"

  # S15 ★中文文件名藏密钥(审计发现的 C-quote 绕过)→ -z 修复后应被抓到
  $r = RunFlow (NewRepo "s15" { param($p) Set-Content "$p/README.md" "hi" }) "code" { param($p) Set-Content "$p/秘密.txt" "aws = $FAKEKEY" -Encoding UTF8 }
  $m = TryMerge $r.taskId
  Rec "S15 ★中文名文件里的密钥应被抓" ((StatusOf $r.checks "密钥扫描") -eq "fail" -and -not $m.ok) "scan=$(StatusOf $r.checks '密钥扫描') mergeOk=$($m.ok)"

  # S16 ★3MB 文件藏密钥(旧 2MB 上限会跳过)→ 提高上限后应被抓到
  $r = RunFlow (NewRepo "s16" { param($p) Set-Content "$p/README.md" "hi" }) "code" { param($p)
      $pad = "a" * 3145728  # 3MB 填充,把密钥顶到 2MB 之外
      Set-Content "$p/config.json" "$pad`naws = $FAKEKEY"
    }
  $m = TryMerge $r.taskId
  Rec "S16 ★3MB 文件里的密钥应被抓" ((StatusOf $r.checks "密钥扫描") -eq "fail" -and -not $m.ok) "scan=$(StatusOf $r.checks '密钥扫描') mergeOk=$($m.ok)"

} finally {
  Stop-Process -Id $srv.Id -Force -ErrorAction SilentlyContinue
  Start-Sleep 1
  Remove-Item -Recurse -Force $root -ErrorAction SilentlyContinue
  Remove-Item -Force "$db*" -ErrorAction SilentlyContinue
  $env:POLARIS_COLLAB_DB = ""; $env:POLARIS_PORT = ""
}

Write-Host "`n═══════════════ 汇总 ═══════════════"
$fail = @($results | Where-Object { -not $_.pass })
Write-Host "总 $($results.Count) 项,通过 $($results.Count - $fail.Count),失败 $($fail.Count)"
if ($fail.Count) { Write-Host "`n失败项:"; $fail | ForEach-Object { Write-Host " ✗ $($_.name) :: $($_.detail)" } }

