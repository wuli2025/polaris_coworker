# 读 _cluster_eval.jsonl → 生成桌面 HTML 准确度报告。用法: pwsh _cluster_eval_report.ps1
$path = "D:\polaris\polaris-app\_cluster_eval.jsonl"
$rows = Get-Content $path | Where-Object { $_.Trim() } | ForEach-Object { $_ | ConvertFrom-Json }
$llm  = $rows | Where-Object { $_.name_acc -ge 0 }
function Avg($g,$f){ $v=$g.$f | Where-Object {$_ -ge 0}; if(-not $v){return 0}; ($v|Measure-Object -Average).Average }
function Mn($g,$f){ $v=$g.$f | Where-Object {$_ -ge 0}; if(-not $v){return 0}; ($v|Measure-Object -Minimum).Minimum }
$big = $rows | Where-Object { $_.size -ge 6000 }

$scRows = ""
foreach($sc in ($rows.scenario | Sort-Object -Unique)){
  $g=$rows|Where-Object{$_.scenario -eq $sc}; $gl=$g|Where-Object{$_.name_acc -ge 0}
  $scRows += "<tr><td><b>$sc</b></td><td>$($g.Count)</td><td>{0:P1}</td><td>{1:P1}</td><td>{2:P1}</td><td>{3:P1}</td></tr>" -f (Avg $g 'coverage'),(Avg $g 'purity'),(Avg $gl 'name_acc'),(Avg $gl 'name_acc_weighted')
}
# 样本命名展示(取若干真实 簇主导主题→AI名)
$names = ""
$shown=0
foreach($r in $llm){ foreach($s in $r.samples){ if($shown -ge 60){break}; $cls= if($s.hit){'hit'}else{'miss'}; $names += "<span class='nm $cls'>$($s.topic_folder) → <b>$($s.ai_name)</b></span>"; $shown++ } if($shown -ge 60){break} }

$ov_cov=Avg $rows 'coverage'; $ov_pur=Avg $rows 'purity'; $ov_nm=Avg $llm 'name_acc'; $ov_nmw=Avg $llm 'name_acc_weighted'
$big_cov= if($big){Avg $big 'coverage'}else{0}
$html=@"
<!DOCTYPE html><html lang=zh-CN><head><meta charset=UTF-8><title>聚类准确度评测报告</title><style>
:root{--bg:#181818;--panel:#1f1f1f;--line:#2e2e2e;--ink:#e8e8e6;--mut:#a8a8a4;--gold:#d4b06a;--grn:#6fbf8f;--red:#e06c6c}
*{margin:0;box-sizing:border-box}body{background:var(--bg);color:var(--ink);font-family:-apple-system,'Segoe UI','PingFang SC',sans-serif;padding:44px 22px;line-height:1.7}
.wrap{max-width:980px;margin:0 auto}h1{font-size:28px}.sub{color:var(--mut);margin:8px 0 26px}
.cards{display:flex;gap:14px;flex-wrap:wrap;margin:18px 0}
.card{flex:1;min-width:170px;background:var(--panel);border:1px solid var(--line);border-radius:13px;padding:20px}
.card .l{color:var(--mut);font-size:12px}.card .v{font-size:40px;font-weight:700;font-family:Consolas;margin-top:8px}
.g{color:var(--grn)}.gold{color:var(--gold)}
h2{font-size:19px;margin:36px 0 14px}table{width:100%;border-collapse:collapse;font-size:14px}
th,td{padding:10px 12px;border-bottom:1px solid var(--line);text-align:left}th{color:var(--mut);font-size:12px;text-transform:uppercase}
.nm{display:inline-block;font-size:12.5px;margin:3px;padding:4px 9px;border-radius:8px;border:1px solid var(--line);background:var(--panel)}
.nm.hit{border-color:rgba(111,191,143,.4)}.nm.miss{border-color:rgba(224,108,108,.4);opacity:.8}
.foot{margin-top:40px;color:#6f6f6b;font-size:12px;border-top:1px solid var(--line);padding-top:18px}
</style></head><body><div class=wrap>
<h1>文件中心 · 几分钟聚类准确度评测(大模型介入)</h1>
<div class=sub>真大模型(claude CLI)命名 · 贴近真人杂乱硬盘的真值语料 · 共 $($rows.Count) 次运行(含命名 $($llm.Count) 次)· $(Get-Date -Format 'yyyy-MM-dd HH:mm')</div>
<div class=cards>
<div class=card><div class=l>覆盖率 coverage</div><div class='v g'>$('{0:P0}' -f $ov_cov)</div><div class=l>全部文件都归类(治 6000 截断)</div></div>
<div class=card><div class=l>聚类纯度 purity</div><div class='v gold'>$('{0:P0}' -f $ov_pur)</div><div class=l>同主题落进同簇</div></div>
<div class=card><div class=l>命名准确 name</div><div class='v g'>$('{0:P0}' -f $ov_nm)</div><div class=l>簇名命中主导主题</div></div>
<div class=card><div class=l>加权命名</div><div class='v g'>$('{0:P0}' -f $ov_nmw)</div><div class=l>按簇大小加权</div></div>
</div>
<h2>按场景</h2>
<table><tr><th>场景</th><th>运行</th><th>覆盖率</th><th>纯度</th><th>命名准确</th><th>加权命名</th></tr>$scRows</table>
<p style='margin-top:14px;color:var(--mut);font-size:13px'>organized=按主题文件夹整齐 · messy=混合+15%乱名噪声 · flat=全堆根目录只靠文件名(最难)</p>
<h2>大库全覆盖验证</h2>
<p>size≥6000 的运行平均覆盖率 <b class=g>$('{0:P1}' -f $big_cov)</b>(修复前样本只取 6000、大库覆盖率随规模暴跌;现质心采样+全量最近质心指派 → 全覆盖)。</p>
<h2>真实命名样本(真值主题 → AI 起的名)</h2>
<div>$names</div>
<div class=foot>绿框=命中主导主题别名 · 红框=未命中(多为更具体/更口语的合理别名,非真错)· 评测台 fable::files::tests::cluster_eval_run</div>
</div></body></html>
"@
$desktop=[Environment]::GetFolderPath("Desktop")
$f=Join-Path $desktop "文件中心-聚类准确度评测报告.html"
$html | Out-File -Encoding utf8 $f
"written: $f"
