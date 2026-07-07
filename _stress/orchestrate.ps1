# 自包含抗压编排器：服务器作为本命令子进程存活整个测试周期，全程监控存活/内存。
# 顺序确定性阶段 -> 多客户端并发风暴(climax) -> serve_file OOM 受控验证。
$ErrorActionPreference = 'SilentlyContinue'
$cwd = (Get-Location).Path
$exe = (Resolve-Path "src-tauri\target\debug\polaris-server.exe").Path
$env:POLARIS_PORT = "8799"
$env:PORT = "8799"

Get-Process polaris-server -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep 1
"== launching server (child of this command) =="
$srv = Start-Process -FilePath $exe -PassThru -WindowStyle Hidden `
    -RedirectStandardOutput "$cwd\_stress_srv_out.log" -RedirectStandardError "$cwd\_stress_srv_err.log"
$srvId = $srv.Id

function Alive { $null -ne (Get-Process -Id $srvId -ErrorAction SilentlyContinue) }
function Mem { $x = Get-Process -Id $srvId -ErrorAction SilentlyContinue; if ($x) { [int]($x.WorkingSet64 / 1MB) } else { -1 } }

$ok = $false
for ($i = 0; $i -lt 30; $i++) {
    try { if ((Invoke-WebRequest "http://127.0.0.1:8799/api/health" -TimeoutSec 3).Content -eq 'ok') { $ok = $true; break } } catch {}
    Start-Sleep 1
}
"server pid=$srvId health_ok=$ok baseline_mem=$(Mem)MB"
if (-not $ok) { "FATAL: server never became healthy"; Get-Content _stress_srv_err.log -Tail 30; Stop-Process -Id $srvId -Force; exit 1 }

# ── 确定性对抗阶段 ──
foreach ($ph in 'fuzz', 'traversal', 'unicode', 'nested', 'bomb', 'conn') {
    if (-not (Alive)) { "!!! SERVER DEAD before phase $ph"; break }
    "===== PHASE $ph ====="
    (& python _stress\adversarial.py $ph 2>&1 | Out-String).Trim()
    "[after $ph] alive=$(Alive) mem=$(Mem)MB"
}

# ── CLIMAX：多客户端并发风暴（= 多 agent 同时打）──
if (Alive) {
    "===== CLIMAX: 7 concurrent adversarial clients (4x flood + 2x fuzz + 1x bomb) ====="
    $procs = @()
    $procs += Start-Process python -ArgumentList "_stress\adversarial.py", "flood", "45", "30" -PassThru -WindowStyle Hidden -RedirectStandardOutput "$cwd\_cl_f1.json" -RedirectStandardError "$cwd\_cl_f1.err"
    $procs += Start-Process python -ArgumentList "_stress\adversarial.py", "flood", "45", "30" -PassThru -WindowStyle Hidden -RedirectStandardOutput "$cwd\_cl_f2.json" -RedirectStandardError "$cwd\_cl_f2.err"
    $procs += Start-Process python -ArgumentList "_stress\adversarial.py", "flood", "45", "25" -PassThru -WindowStyle Hidden -RedirectStandardOutput "$cwd\_cl_f3.json" -RedirectStandardError "$cwd\_cl_f3.err"
    $procs += Start-Process python -ArgumentList "_stress\adversarial.py", "flood", "45", "25" -PassThru -WindowStyle Hidden -RedirectStandardOutput "$cwd\_cl_f4.json" -RedirectStandardError "$cwd\_cl_f4.err"
    $procs += Start-Process python -ArgumentList "_stress\adversarial.py", "fuzz" -PassThru -WindowStyle Hidden -RedirectStandardOutput "$cwd\_cl_z1.json" -RedirectStandardError "$cwd\_cl_z1.err"
    $procs += Start-Process python -ArgumentList "_stress\adversarial.py", "fuzz" -PassThru -WindowStyle Hidden -RedirectStandardOutput "$cwd\_cl_z2.json" -RedirectStandardError "$cwd\_cl_z2.err"
    $procs += Start-Process python -ArgumentList "_stress\adversarial.py", "bomb" -PassThru -WindowStyle Hidden -RedirectStandardOutput "$cwd\_cl_b1.json" -RedirectStandardError "$cwd\_cl_b1.err"
    $peak = 0; $died = -1
    for ($i = 0; $i -lt 30; $i++) {
        if (-not (Alive)) { $died = $i * 2; break }
        $m = Mem; if ($m -gt $peak) { $peak = $m }
        if (($procs | Where-Object { -not $_.HasExited }).Count -eq 0) { break }
        Start-Sleep 2
    }
    if ($died -ge 0) { "!!! SERVER DIED during climax at ~${died}s" } else { "climax survived. peak_mem=$peak MB alive=$(Alive)" }
    $procs | ForEach-Object { try { $_ | Wait-Process -Timeout 20 } catch {} ; if (-not $_.HasExited) { $_ | Stop-Process -Force } }
    "--- climax client results ---"
    foreach ($f in 'f1', 'f2', 'f3', 'f4', 'z1', 'z2', 'b1') { $c = Get-Content "$cwd\_cl_$f.json" -Raw; if ($c) { "$f`: " + $c.Trim() } }
}

# ── serve_file OOM 受控验证 ──
if (Alive) {
    "===== serve_file OOM validation ====="
    $big = Get-ChildItem "$env:USERPROFILE\Polaris\models" -Recurse -File -ErrorAction SilentlyContinue | Sort-Object Length -Descending | Select-Object -First 1
    if ($big) {
        $fmb = [int]($big.Length / 1MB)
        $before = Mem
        $client = Start-Process python -ArgumentList "_stress\adversarial.py", "servefile", $big.FullName, "6" -PassThru -WindowStyle Hidden -RedirectStandardOutput "$cwd\_cl_sf.json" -RedirectStandardError "$cwd\_cl_sf.err"
        $peak2 = $before
        for ($i = 0; $i -lt 60; $i++) {
            if (-not (Alive)) { break }
            $m = Mem; if ($m -gt $peak2) { $peak2 = $m }
            if ($client.HasExited) { Start-Sleep 1; $m = Mem; if ($m -gt $peak2) { $peak2 = $m }; break }
            Start-Sleep 1
        }
        try { $client | Wait-Process -Timeout 10 } catch {}; if (-not $client.HasExited) { $client | Stop-Process -Force }
        "serve_file: file=$fmb MB conc=6 before=$before MB peak=$peak2 MB delta=$($peak2-$before) MB alive_after=$(Alive)"
        "(unbounded read => delta ~ file*conc; bounded/streamed => delta small)"
        $sf = Get-Content "$cwd\_cl_sf.json" -Raw; if ($sf) { "client: " + $sf.Trim() }
    } else { "no large model file found; skipping serve_file test" }
}

# ── 收尾 ──
"===== FINAL ====="
"alive=$(Alive) final_mem=$(Mem)MB"
"--- server stderr tail (panics show here) ---"
Get-Content _stress_srv_err.log -Tail 25
Stop-Process -Id $srvId -Force -ErrorAction SilentlyContinue
"server stopped. done."
