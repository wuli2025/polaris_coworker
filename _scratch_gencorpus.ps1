# ════════════════════════════════════════════════════════════════════
# RAG 20TB 压测语料生成器
# D:\_ragtest\corpus 下造"混乱、含视频、多类型、多编码、有埋点"语料。
# 真实文件 ~12 万 + 稀疏 TB 级巨型文件(逻辑字节冲到 ~2.4TB,占盘近 0)。
# ════════════════════════════════════════════════════════════════════
$ErrorActionPreference = 'Stop'
$root = 'D:\_ragtest\corpus'
$evalOut = 'D:\_ragtest\fable_eval.json'
if (Test-Path $root) { Remove-Item $root -Recurse -Force }
New-Item -ItemType Directory -Force -Path $root | Out-Null
$sw = [System.Diagnostics.Stopwatch]::StartNew()
$utf8 = [System.Text.Encoding]::UTF8
$dirCache = [System.Collections.Generic.HashSet[string]]::new()

function Ensure($dir) { if ($dirCache.Add($dir)) { if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Force -Path $dir | Out-Null } } }
function W($rel, $content) {
  $full = Join-Path $root $rel
  Ensure (Split-Path $full -Parent)
  [System.IO.File]::WriteAllText($full, $content, $utf8)
}
function NewSparse($rel, [long]$size) {
  $full = Join-Path $root $rel
  Ensure (Split-Path $full -Parent)
  $fs = [System.IO.File]::Create($full); $fs.Close()
  & fsutil sparse setflag $full | Out-Null
  $h = [System.IO.File]::Open($full,'Open','Write'); $h.SetLength($size); $h.Close()
}

$fileCount = 0
$rng = [System.Random]::new(20260615)
$lorem = @('系统','检索','向量','嵌入','倒排','索引','文件','知识库','语义','模型','缓存','并行','车道','融合','重排','盘点','视频','字幕','转写','分块','余弦','汉明','量化','召回','排序','性能','吞吐','延迟','磁盘','内存','线程','数据','日志','记录','配置','参数','报告','测试','基准','规模')
function Para($n) {
  $parts = for ($i=0;$i -lt $n;$i++){ ($lorem | Get-Random -Count 8 -SetSeed $rng.Next()) -join '' }
  ($parts -join '。') + '。'
}

Write-Host "[1/7] needle 埋点文档..."
$topics = @(
  @{q='公司的退款政策是怎样的';a='本店支持七天无理由退款,生鲜类除外,退款将在三个工作日内原路返回。'},
  @{q='营业时间是几点到几点';a='门店每天上午九点开门,下午六点关门,法定节假日不休息。'},
  @{q='如何重置我的账户密码';a='在登录页点击忘记密码,输入注册邮箱,系统会发送一封含重置链接的邮件。'},
  @{q='数据库连接池应该配多大';a='连接池大小建议设为 CPU 核数的两到四倍,过大反而会因上下文切换降低吞吐。'},
  @{q='向量检索为什么要做归一化';a='入库时做一次 L2 归一化后,余弦相似度退化成纯点积,查询时省掉每个向量现算模长的开销。'},
  @{q='二值量化粗筛的原理是什么';a='把浮点向量按符号位打包成二值码,先用汉明距离粗筛候选,读量只有原始浮点的三十二分之一。'},
  @{q='RRF 融合是怎么把两路结果合并的';a='倒数排名融合按 1 除以 60 加排名给每路打分再相加,同一文件被多路命中时分数自然叠加顶上去。'},
  @{q='重排闸门什么时候才会触发';a='只有候选不少于三个且前两名分数咬得很紧时才请重排专家,一骑绝尘或候选太少就直接保持融合序。'},
  @{q='猫咪半夜频繁叫唤怎么办';a='猫在夜间活跃属天性,可在睡前增加互动消耗精力,并保证食盆有粮,必要时咨询宠物医生。'},
  @{q='红烧肉怎么做才能不腻';a='先焯水去腥,炒糖色后小火慢炖一小时,加入少量山楂或茶叶可解腻增香。'},
  @{q='高血压患者饮食要注意什么';a='控制钠盐摄入每天不超过五克,多吃富含钾的蔬果,少吃腌制和油炸食品。'},
  @{q='Python 里如何并行处理大量文件';a='IO 密集用线程池,CPU 密集用进程池绕过全局解释器锁。'},
  @{q='git 误删分支怎么找回';a='用 git reflog 找到分支最后的提交哈希,再用 git branch 指定哈希即可恢复。'},
  @{q='北京到上海高铁要多久';a='京沪高铁最快约四个半小时,二等座票价五百多元,每天有数十趟车次。'},
  @{q='婴儿几个月可以添加辅食';a='世界卫生组织建议满六个月起逐步添加辅食,从单一米粉开始,观察过敏反应。'},
  @{q='如何判断西瓜熟没熟';a='拍打声音清脆回响、瓜脐内凹、瓜藤卷曲发黄的西瓜通常成熟度高、甜度好。'}
)
$needleManifest = @()
for ($i=0; $i -lt 80; $i++) {
  $t = $topics[$i % $topics.Count]
  $token = "NEEDLE-{0:D5}-ZARQUON" -f $i
  $rel = "docs/needles/needle_{0:D5}.md" -f $i
  W $rel "# 知识条目 $i`n`n唯一标记: $token`n`n## 问题`n$($t.q)?`n`n## 解答`n$($t.a)`n`n$(Para 3)"
  $fileCount++
  $needleManifest += [pscustomobject]@{ token=$token; rel=$rel; q=$t.q }
}

Write-Host "[2/7] 知识文档 docs/ (40000)..."
for ($d=0; $d -lt 400; $d++) {
  $sub = "docs/kb/topic_{0:D3}" -f $d
  for ($f=0; $f -lt 100; $f++) {
    W "$sub/doc_$('{0:D4}' -f $f).md" "# $(($lorem|Get-Random -Count 4 -SetSeed $rng.Next()) -join '')`n`n$(Para (3 + $rng.Next(8)))"
    $fileCount++
  }
}

Write-Host "[3/7] 代码文件 code/ (30000)..."
$langs = @('py','rs','js','ts','go','java','c','cpp','rb','php')
for ($d=0; $d -lt 300; $d++) {
  $sub = "code/pkg_{0:D3}" -f $d
  for ($f=0; $f -lt 100; $f++) {
    $ext = $langs[$rng.Next($langs.Count)]
    W "$sub/mod_$('{0:D4}' -f $f).$ext" "// module $d-$f`nfunction process_data() {`n  // $(Para 1)`n  return retrieve_index(query);`n}`n"
    $fileCount++
  }
}

Write-Host "[4/7] 数据/日志转储 data/ (embed 跳过 log/csv/tsv/ndjson)..."
for ($f=0; $f -lt 200; $f++) {
  $ext = @('log','csv','tsv','ndjson')[$f % 4]
  $full = Join-Path $root ("data/dumps/dump_{0:D4}.$ext" -f $f)
  Ensure (Split-Path $full -Parent)
  $writer = [System.IO.StreamWriter]::new($full, $false, $utf8)
  $lines = 5000 + $rng.Next(15000)
  for ($l=0; $l -lt $lines; $l++) { $writer.WriteLine("2026-06-15T10:00:$($l % 60)Z,level=INFO,req=$l,latency_ms=$($rng.Next(500)),msg=processed record $l") }
  $writer.Close(); $fileCount++
}
for ($d=0; $d -lt 100; $d++) {
  $sub = "data/json/batch_{0:D3}" -f $d
  for ($f=0; $f -lt 100; $f++) {
    W "$sub/rec_$('{0:D4}' -f $f).json" "{`"id`":$f,`"topic`":`"$(($lorem|Get-Random -Count 3 -SetSeed $rng.Next()) -join '')`",`"value`":$($rng.Next(10000))}"
    $fileCount++
  }
}

Write-Host "[5/7] 视频(稀疏 200MB)+ 字幕 media/ (2000 对)..."
for ($f=0; $f -lt 2000; $f++) {
  NewSparse ("media/videos/clip_{0:D4}.mp4" -f $f) 200MB
  $fileCount++
  $cap = if ($f -lt 40) {
    $t = $topics[$f % $topics.Count]
    "1`n00:00:01,000 --> 00:00:05,000`n$($t.a)`n`n2`n00:00:05,000 --> 00:00:09,000`n$(Para 1)`n"
  } else { "1`n00:00:01,000 --> 00:00:05,000`n$(Para 2)`n" }
  W ("media/videos/clip_{0:D4}.srt" -f $f) $cap
  $fileCount++
}
$srtNeedles = for ($f=0; $f -lt 40; $f++) { $t=$topics[$f % $topics.Count]; [pscustomobject]@{ rel=("clip_{0:D4}.srt" -f $f); q=$t.q } }

Write-Host "[6/7] 混乱/对抗样本 messy/ ..."
for ($f=0; $f -lt 500; $f++) {
  $full = Join-Path $root ("messy/fakebin/blob_{0:D4}.txt" -f $f)
  Ensure (Split-Path $full -Parent)
  $bytes = New-Object byte[] 4096; $rng.NextBytes($bytes); $bytes[100] = 0
  [System.IO.File]::WriteAllBytes($full, $bytes); $fileCount++
}
for ($f=0; $f -lt 500; $f++) { W ("messy/中文目录/文档_資料_测试_{0:D4}.txt" -f $f) ("中文内容 $(Para 2)"); $fileCount++ }
for ($f=0; $f -lt 500; $f++) { W ("messy/empty/zero_{0:D4}.txt" -f $f) ""; $fileCount++ }
$deep = "messy/deep"; for ($lvl=0; $lvl -lt 50; $lvl++) { $deep = "$deep/lvl$lvl" }
W "$deep/bottom.md" "# 深处文档`n`n唯一标记: NEEDLE-DEEP-ZARQUON`n`n$(Para 2)"; $fileCount++
$dupBody = "# 重复模板`n`n会被复制很多次的相同内容。唯一标记: NEEDLE-DUP-ZARQUON`n"
for ($f=0; $f -lt 1000; $f++) { W ("messy/dups/copy_{0:D4}.md" -f $f) $dupBody; $fileCount++ }
# 超大真实 log(~120MB,测 grep/FTS 单文件 4MB 上限边界)
$bigLog = Join-Path $root 'messy/huge/huge_app.log'
Ensure (Split-Path $bigLog -Parent)
$writer = [System.IO.StreamWriter]::new($bigLog, $false, $utf8)
for ($l=0; $l -lt 2000000; $l++) { $writer.WriteLine("line $l ERROR something happened code=$($l % 999) NEEDLE-HUGELOG-ZARQUON") }
$writer.Close(); $fileCount++

Write-Host "[7/7] 稀疏 TB 级巨型文件(60 x 40GB = 2.4TB 逻辑)..."
for ($f=0; $f -lt 60; $f++) { NewSparse ("giants/archive_{0:D3}.mkv" -f $f) 40GB; $fileCount++ }

Write-Host "写评测考卷..."
$cases = @()
foreach ($n in $needleManifest) {
  $cases += [pscustomobject]@{ query=$n.token; expect=@($n.rel) }
  $cases += [pscustomobject]@{ query=$n.q; expect=@($n.rel) }
}
foreach ($s in $srtNeedles) { $cases += [pscustomobject]@{ query=$s.q; expect=@($s.rel) } }
$cases += [pscustomobject]@{ query='NEEDLE-DEEP-ZARQUON'; expect=@('bottom.md') }
$cases += [pscustomobject]@{ query='NEEDLE-HUGELOG-ZARQUON'; expect=@('huge_app.log') }
[System.IO.File]::WriteAllText($evalOut, ([pscustomobject]@{ cases=$cases } | ConvertTo-Json -Depth 6), $utf8)

$sw.Stop()
Write-Host "`n════ 完成 ════ 真实文件: $fileCount  用时: $([math]::Round($sw.Elapsed.TotalSeconds,1))s  考卷: $($cases.Count) 题"