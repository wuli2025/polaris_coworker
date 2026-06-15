$exe="D:\polaris\polaris-app\src-tauri\target\release\polaris-forge.exe"
$ev='D:\_ragtest\fable_eval_focus.json'
$log='D:\polaris\polaris-app\_scratch_eval_vh.log'
"start $(Get-Date -Format o)" | Set-Content $log
foreach($mode in @('vector','hybrid')){
  $sw=[Diagnostics.Stopwatch]::StartNew()
  & $exe fable eval --set=$ev --top=10 --mode=$mode 1>"D:\polaris\polaris-app\_scratch_eval_$mode.json" 2>"D:\polaris\polaris-app\_scratch_eval_$mode.err"
  $sw.Stop()
  $code=$LASTEXITCODE
  try{
    $r=Get-Content "D:\polaris\polaris-app\_scratch_eval_$mode.json" -Raw|ConvertFrom-Json
    $lit=$r.details|Where-Object{$_.query -like 'NEEDLE-*'}
    $sem=$r.details|Where-Object{$_.query -notlike 'NEEDLE-*'}
    $reranked = ($r.details | Where-Object { $_.PSObject.Properties.Name -contains 'top_paths' }).Count
    "$mode : exit=$code evaluated=$($r.evaluated) recall@10=$([math]::Round($r.recall_at_k*100,1))% MRR=$([math]::Round($r.mrr,3)) wall=$([math]::Round($sw.Elapsed.TotalSeconds,1))s litHit=$(($lit|?{$_.hit_rank}).Count)/$($lit.Count) semHit=$(($sem|?{$_.hit_rank}).Count)/$($sem.Count)" | Add-Content $log
  }catch{
    "$mode : exit=$code PARSE-FAIL err=$(Get-Content "D:\polaris\polaris-app\_scratch_eval_$mode.err" -Raw)" | Add-Content $log
  }
}
"done $(Get-Date -Format o)" | Add-Content $log
