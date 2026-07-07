<#
.SYNOPSIS
    语料上库守卫 · govinfo→ingest→eval 一键套用

.DESCRIPTION
    把"下载/落盘原始语料 → 入寓言索引 → 跑评测确认没退化"三段式沉淀为单文件调用,
    配套 wiki/workflows/语料上库-SOP.md。典型场景:微信导出、EDGAR 财报、NAS 镜像等
    GB~TB 级语料,按本脚本一次跑完即可。

    调用 polaris-forge.exe 的三个子命令:
      1. fable status               # 看 files_total / pending_files / chunks_total / lex_files
      2. fable index --max-chunks=N  # 循环直到 pending_files ≤ 5(默认 40 轮)
      3. fable eval --set=... --mode={vector,hybrid,grep}

    任何阶段失败 / 召回跌破阈值,立即非零退出,把诊断信息打到 stderr。

.PARAMETER Roots
    语料根(可多个;逗号分隔)。仅在 §2 盘点/索引时作"应纳入"参考;真正的盘点是寓言自己从
    默认盘点根列表里扫,本参数用来在索引前 sanity check 一下"我打算入库的就是这几个目录"。

.PARAMETER EvalSet
    评测集 JSON 路径(默认 ~/Polaris/data/fable_eval.json,寓言约定路径)。
    不存在时直接退出(不自动生成 — 重建考卷是另一件事,见 SOP §5)。

.PARAMETER Modes
    要跑的评测模式,逗号分隔,默认 'vector,hybrid'。
    合法值:vector | hybrid | grep。hybrid 是实战命中率,vector 是语义命中下限,grep 是字面下限。
    至少要有 hybrid — 单独跑 vector/grep 只在你明确想看 baseline 时。

.PARAMETER TopK
    评测 top-k,默认 10。寓言内部 clamp 到 [1, 50]。

.PARAMETER MaxIters
    索引循环最大轮数,默认 40。每轮消化 5000 chunk,超时或 429 触发 8s 退避后继续。

.PARAMETER PendingStop
    索引收敛阈值,默认 5。pending_files ≤ 此值就停(允许个位数永远 pending:扩展名/超大/编码异常)。

.PARAMETER MinHybridRecall
    hybrid 模式 recall@10 的健康下限,默认 0.95。跌破即非零退出,不许发布。

.PARAMETER MinVectorRecall
    vector 模式 recall@10 的健康下限,默认 0.50。仅在 modes 含 vector 时生效。

.PARAMETER ChunkBudget
    每轮消化 chunk 数,默认 5000。撞嵌入限速(429)时降到 2000 比较稳。

.PARAMETER Exe
    polaris-forge.exe 路径,默认 D:\polaris\polaris-app\src-tauri\target\release\polaris-forge.exe。
    没编译出来的话用 -SkipIngest + -EvalSet 单独跑评测。

.PARAMETER SkipIngest
    跳过盘点+索引阶段,直接跑评测。用于"刚换语料想快速比对"的场景。

.PARAMETER SkipEval
    跳过评测阶段,只跑入库。用于"先全量入完再说"的场景。

.PARAMETER WhatIf
    Dry-run:只把要执行的命令打到 stdout,不真跑。便于在真跑前对一眼。

.PARAMETER LogDir
    中间日志/JSON 落盘目录,默认 _scratch 下三个固定文件名:
      _ingest_guard_status.json
      _ingest_guard_iter.json/.err
      _ingest_guard_eval_<mode>.json/.err
      _ingest_guard_summary.log

.EXAMPLE
    # 微信语料标准套法
    .\scripts\ingest-guard.ps1 -Roots 'D:\Archives\wechat-2026Q2' -EvalSet 'D:\_ragtest\wechat_eval.json' -MaxIters 60

.EXAMPLE
    # 只跑评测(刚换了别的语料想快速看 hybrid 召回有没有崩)
    .\scripts\ingest-guard.ps1 -SkipIngest -EvalSet 'D:\_ragtest\wechat_eval.json'

.EXAMPLE
    # Dry-run(只打印要跑的脚本,不执行)
    .\scripts\ingest-guard.ps1 -Roots 'D:\Archives\wechat-2026Q2' -WhatIf

.OUTPUTS
    stdout: 进度日志 + 最终 [summary] 行(recall@k / MRR / 是否通过阈值)
    stderr: 失败诊断
    exit 0: 三段全过 + hybrid recall@10 ≥ 阈值
    exit 1: 阶段失败 / 召回跌破阈值
    exit 2: 前置条件不齐(语料根不存在 / 评测集缺失 / 极简路径问题)

.NOTES
    配套文档:wiki/workflows/语料上库-SOP.md(必读,本脚本是它的执行壳)
    设计取舍:
      - 调 external exe 而不内嵌 rust — 单一事实源在 fable.db,exe 是门面;
      - PowerShell 而非 bash — Windows 主战场,且 PS 原生接 .NET 时间/JSON,无需 jq/awk;
      - 不自动重建评测集 — 重建是创造性劳动(挑关键词、查路径),脚本不能代替判断;
      - 阈值硬卡 — hybrid 跌破 0.95 不许放过,而不是"打 warning";SOP 已有判定准则,信任它。
#>

[CmdletBinding()]
param(
    [string[]]$Roots,
    [string]$EvalSet = (Join-Path $env:USERPROFILE 'Polaris\data\fable_eval.json'),
    [string]$Modes = 'vector,hybrid',
    [int]$TopK = 10,
    [int]$MaxIters = 40,
    [int]$PendingStop = 5,
    [double]$MinHybridRecall = 0.95,
    [double]$MinVectorRecall = 0.50,
    [int]$ChunkBudget = 5000,
    [string]$Exe = 'D:\polaris\polaris-app\src-tauri\target\release\polaris-forge.exe',
    [switch]$SkipIngest,
    [switch]$SkipEval,
    [switch]$WhatIf,
    [string]$LogDir = 'D:\polaris\polaris-app\_scratch'
)

$ErrorActionPreference = 'Stop'
$utf8 = [System.Text.Encoding]::UTF8

# ───────────────────────── 小工具 ─────────────────────────

function Log([string]$msg) {
    $line = "[$(Get-Date -Format 'HH:mm:ss')] $msg"
    Write-Host $line
    Add-Content -Path (Join-Path $LogDir '_ingest_guard_summary.log') -Value $line -Encoding utf8
}
function Fail([string]$msg, [int]$code = 1) {
    [Console]::Error.WriteLine("[FAIL] $msg")
    Add-Content -Path (Join-Path $LogDir '_ingest_guard_summary.log') -Value "[FAIL] $msg (exit=$code)" -Encoding utf8
    exit $code
}
function RunExe([string[]]$args, [string]$outFile = $null, [string]$errFile = $null) {
    if ($WhatIf) {
        Log "DRY-RUN: $Exe $($args -join ' ')"
        return @{ exit = 0; json = $null }
    }
    $stdoutTarget = if ($outFile) { $outFile } else { [Console]::OpenStandardOutput() }
    $stderrTarget = if ($errFile) { $errFile } else { [Console]::OpenStandardError() }
    $p = Start-Process -FilePath $Exe -ArgumentList $args `
        -NoNewWindow -PassThru -Wait `
        -RedirectStandardOutput $stdoutTarget -RedirectStandardError $stderrTarget
    $exit = $p.ExitCode
    if (-not $SkipEval -and $args -contains 'eval' -and $outFile -and (Test-Path $outFile)) {
        try { return @{ exit = $exit; json = (Get-Content $outFile -Raw | ConvertFrom-Json) } }
        catch { Fail "eval 输出解析失败 ($outFile): $($_.Exception.Message)" }
    }
    return @{ exit = $exit; json = $null }
}
function ReadStatus() {
    $tmp = Join-Path $LogDir '_ingest_guard_status.json'
    RunExe @('fable','status') -outFile $tmp
    if (-not (Test-Path $tmp)) { Fail "fable status 没产出 $tmp" }
    Get-Content $tmp -Raw | ConvertFrom-Json
}

# ───────────────────────── 前置条件 ─────────────────────────

if (-not (Test-Path $LogDir)) { New-Item -ItemType Directory -Force -Path $LogDir | Out-Null }

Log "════════ ingest-guard 启动 ════════"
Log "exepath : $Exe"
Log "roots   : $(if ($Roots) { $Roots -join ',' } else { '(默认寓言盘点根)' })"
Log "evalset : $EvalSet"
Log "modes   : $Modes   topK=$TopK   maxIters=$MaxIters   pendingStop=$PendingStop"
Log "thresh  : hybrid≥$MinHybridRecall  vector≥$MinVectorRecall   chunkBudget=$ChunkBudget"

if (-not $WhatIf -and -not (Test-Path $Exe)) {
    Fail "找不到 polaris-forge.exe: $Exe(先 cargo build --release,或传 -Exe 指向你的构建产物)" 2
}

# Roots sanity:列出来只是"我打算入库这几个目录",不等于强制盘点根,寓言盘点根是它自己定的
if ($Roots) {
    foreach ($r in $Roots) {
        if (-not (Test-Path $r)) { Fail "Roots 路径不存在: $r" 2 }
        Log "  root ok : $r ($( (Get-ChildItem $r -Recurse -File -ErrorAction SilentlyContinue | Measure-Object).Count ) files)"
    }
}

if (-not $SkipEval -and -not (Test-Path $EvalSet)) {
    Fail "评测集不存在: $EvalSet — 先按 SOP §5 生成(uv run _scratch/build_eval*.py),或传 -SkipEval" 2
}

# ───────────────────────── §2 盘点 + 索引循环 ─────────────────────────

if (-not $SkipIngest) {
    Log "─── §2 索引循环 (pending ≤ $PendingStop 才停,最多 $MaxIters 轮) ───"
    $st = ReadStatus
    Log "状态起点: files=$($st.files_total) pending=$($st.pending_files) chunks=$($st.chunks_total) lex=$($st.lex_files)"

    $iter = 0
    while ($iter -lt $MaxIters) {
        $iter++
        $st = ReadStatus
        if ($st.pending_files -le $PendingStop) {
            Log "✓ iter=$iter 收敛 (pending=$($st.pending_files) ≤ $PendingStop)"
            break
        }
        $itOut = Join-Path $LogDir '_ingest_guard_iter.json'
        $itErr = Join-Path $LogDir '_ingest_guard_iter.err'
        $sw = [Diagnostics.Stopwatch]::StartNew()
        $r = RunExe @('fable','index',"--max-chunks=$ChunkBudget") -outFile $itOut -errFile $itErr
        $sw.Stop()
        if ($r.exit -ne 0) {
            $tail = ''
            if (Test-Path $itErr) { $tail = (Get-Content $itErr -Tail 5 -ErrorAction SilentlyContinue) -join ' / ' }
            Log "  iter=$iter 退出码=$($r.exit) 退避 8s 后继续 — err尾: $tail"
            if (-not $WhatIf) { Start-Sleep 8 }
            continue
        }
        $st2 = ReadStatus
        $delta = $st2.chunks_total - $st.chunks_total
        Log "  iter=$iter exit=$($r.exit) +$delta chunks  wall=$([math]::Round($sw.Elapsed.TotalSeconds,1))s  pending=$($st2.pending_files)→$($st2.pending_files)"
    }

    if ($iter -ge $MaxIters) {
        $stEnd = ReadStatus
        if ($stEnd.pending_files -gt $PendingStop) {
            Fail "跑了 $MaxIters 轮仍 pending=$($stEnd.pending_files) > $PendingStop — 看 _ingest_guard_iter.err 诊断(可能是嵌入通道 / 扩展名黑名单)" 1
        }
    }
} else {
    Log "─── §2 SKIP(-SkipIngest) ───"
}

# ───────────────────────── §3 评测对照 ─────────────────────────

$summary = @()
$exitCode = 0

if (-not $SkipEval) {
    Log "─── §3 评测对照 (modes=$Modes topK=$TopK) ───"
    foreach ($mode in ($Modes -split ',' | ForEach-Object { $_.Trim() } | Where-Object { $_ })) {
        if ($mode -notin @('vector','hybrid','grep')) {
            Fail "非法 mode: $mode(只接受 vector | hybrid | grep)" 2
        }
        $eOut = Join-Path $LogDir "_ingest_guard_eval_$mode.json"
        $eErr = Join-Path $LogDir "_ingest_guard_eval_$mode.err"
        $sw = [Diagnostics.Stopwatch]::StartNew()
        $r = RunExe @('fable','eval',"--set=$EvalSet","--top=$TopK","--mode=$mode") -outFile $eOut -errFile $eErr
        $sw.Stop()
        if ($r.exit -ne 0) {
            $tail = ''
            if (Test-Path $eErr) { $tail = (Get-Content $eErr -Tail 5 -ErrorAction SilentlyContinue) -join ' / ' }
            Fail "$mode eval 退出码=$($r.exit) — err尾: $tail" 1
        }
        $rj = $r.json
        if (-not $rj) { Fail "$mode eval 没产出 JSON,看 $eErr" 1 }

        $line = "  $mode : exit=$($r.exit) eval=$($rj.evaluated)/$($rj.total_cases) recall@$($rj.k)=$([math]::Round($rj.recall_at_k*100,1))% MRR=$([math]::Round($rj.mrr,3)) wall=$([math]::Round($sw.Elapsed.TotalSeconds,1))s"
        Log $line
        $summary += [pscustomobject]@{
            mode = $mode
            exit = $r.exit
            evaluated = $rj.evaluated
            recall_at_k = $rj.recall_at_k
            mrr = $rj.mrr
            wall_sec = [math]::Round($sw.Elapsed.TotalSeconds, 1)
        }

        # 阈值硬卡
        if ($mode -eq 'hybrid' -and $rj.recall_at_k -lt $MinHybridRecall) {
            Fail "hybrid recall@$($rj.k)=$([math]::Round($rj.recall_at_k*100,1))% 跌破阈值 $($MinHybridRecall*100)% — 不许合并发布,查 RRF 融合 / 重排闸门 / 索引完整性" 1
        }
        if ($mode -eq 'vector' -and $rj.recall_at_k -lt $MinVectorRecall) {
            Fail "vector recall@$($rj.k)=$([math]::Round($rj.recall_at_k*100,1))% 跌破阈值 $($MinVectorRecall*100)% — 嵌入通道退化,先查 429 / 本地 ONNX 状态再回头看 hybrid" 1
        }
    }
} else {
    Log "─── §3 SKIP(-SkipEval) ───"
}

# ───────────────────────── 收口 ─────────────────────────

Log "════════ ingest-guard 结束 ════════"
if ($summary.Count -gt 0) {
    Log "[summary]"
    $summary | Format-Table -AutoSize | Out-String -Width 200 | ForEach-Object { Log $_ }
    $summaryPath = Join-Path $LogDir '_ingest_guard_summary.json'
    [System.IO.File]::WriteAllText($summaryPath, ($summary | ConvertTo-Json -Depth 4), $utf8)
    Log "summary: $summaryPath"
}
exit $exitCode
