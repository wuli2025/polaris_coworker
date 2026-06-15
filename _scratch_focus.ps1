$ErrorActionPreference='Stop'
$big='D:\_ragtest\corpus'; $focus='D:\_ragtest\focus'
if(Test-Path $focus){ Remove-Item $focus -Recurse -Force }
$utf8=[System.Text.Encoding]::UTF8
$dirCache=[System.Collections.Generic.HashSet[string]]::new()
function Ensure($d){ if($dirCache.Add($d)){ if(-not(Test-Path $d)){ New-Item -ItemType Directory -Force $d|Out-Null } } }
function W($rel,$c){ $f=Join-Path $focus $rel; Ensure(Split-Path $f -Parent); [IO.File]::WriteAllText($f,$c,$utf8) }
# copy needle md (preserve docs/needles/...) + needle srt (preserve media/videos/...)
Copy-Item (Join-Path $big 'docs\needles') (Join-Path $focus 'docs\needles') -Recurse -Force
Ensure (Join-Path $focus 'media\videos')
for($f=0;$f -lt 40;$f++){ $n=('clip_{0:D4}.srt' -f $f); Copy-Item (Join-Path $big "media\videos\$n") (Join-Path $focus "media\videos\$n") -Force }
# 2000 off-topic noise distractors
$rng=[Random]::new(99)
$lorem=@('系统','检索','向量','嵌入','倒排','索引','文件','知识库','语义','模型','缓存','并行','车道','融合','重排','盘点','视频','字幕','转写','分块','余弦','汉明','量化','召回','排序','性能','吞吐','延迟','磁盘','内存')
for($i=0;$i -lt 2000;$i++){
  $body="# 噪音文档 $i`n`n" + (((1..6)|%{ ($lorem|Get-Random -Count 8 -SetSeed $rng.Next()) -join '' }) -join '。') + '。'
  W ("noise/n_{0:D4}.md" -f $i) $body
}
"focus built: " + (Get-ChildItem $focus -Recurse -File).Count + " files"
# focus eval = original minus the 2 big-corpus-only special cases (huge_app.log / bottom.md)
$ev=Get-Content 'D:\_ragtest\fable_eval.json' -Raw|ConvertFrom-Json
$keep=$ev.cases | Where-Object { $_.expect[0] -notin @('huge_app.log','bottom.md') }
[IO.File]::WriteAllText('D:\_ragtest\fable_eval_focus.json', ([pscustomobject]@{cases=$keep}|ConvertTo-Json -Depth 6), $utf8)
"focus eval cases: " + $keep.Count
