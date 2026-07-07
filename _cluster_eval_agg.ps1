# 聚合 _cluster_eval.jsonl → 按场景/总览的平均指标。用法: pwsh _cluster_eval_agg.ps1
$path = "D:\polaris\polaris-app\_cluster_eval.jsonl"
if (-not (Test-Path $path)) { "no results yet"; exit }
$rows = Get-Content $path | Where-Object { $_.Trim() } | ForEach-Object { $_ | ConvertFrom-Json }
$llm = $rows | Where-Object { $_.name_acc -ge 0 }
"总运行数: $($rows.Count) (含真大模型命名: $($llm.Count))"
"LLM 失败行: $(( $rows | Where-Object { $_.llm_err -ne '' }).Count)"
""
function Stat($g, $field) {
  $vals = $g.$field | Where-Object { $_ -ge 0 }
  if ($vals.Count -eq 0) { return "n/a" }
  $avg = ($vals | Measure-Object -Average).Average
  $min = ($vals | Measure-Object -Minimum).Minimum
  "{0:F3} (min {1:F3})" -f $avg, $min
}
"=== 按场景(覆盖率 / 纯度 / 命名准确度 / 加权命名) ==="
foreach ($sc in ($rows.scenario | Sort-Object -Unique)) {
  $g = $rows | Where-Object { $_.scenario -eq $sc }
  $gl = $g | Where-Object { $_.name_acc -ge 0 }
  "{0,-11} n={1,-3} cov={2}  pur={3}  name={4}  nameW={5}" -f `
    $sc, $g.Count, (Stat $g 'coverage'), (Stat $g 'purity'), (Stat $gl 'name_acc'), (Stat $gl 'name_acc_weighted')
}
""
"=== 总览 ==="
"覆盖率 coverage : $(Stat $rows 'coverage')"
"聚类纯度 purity : $(Stat $rows 'purity')"
"命名准确 name   : $(Stat $llm 'name_acc')"
"加权命名 nameW  : $(Stat $llm 'name_acc_weighted')"
$bigcov = ($rows | Where-Object { $_.size -ge 6000 })
if ($bigcov.Count) { "大库(>=6000)覆盖率: $(Stat $bigcov 'coverage')  ← 验证全覆盖修复" }
