$exe="D:\polaris\polaris-app\src-tauri\target\release\polaris-forge.exe"
$log="D:\polaris\polaris-app\_scratch_index_loop.log"
"start $(Get-Date -Format o)" | Set-Content $log
for($iter=1; $iter -le 40; $iter++){
  $st = & $exe fable status 2>$null | ConvertFrom-Json
  if($st.pending_files -le 5){ "all indexed (pending=$($st.pending_files)) at iter $iter" | Add-Content $log; break }
  & $exe fable index --max-chunks=5000 1>"D:\polaris\polaris-app\_scratch_iter.json" 2>"D:\polaris\polaris-app\_scratch_iter.err"
  $code=$LASTEXITCODE
  $st2 = & $exe fable status 2>$null | ConvertFrom-Json
  "iter $iter exit=$code chunks=$($st2.chunks_total) lex=$($st2.lex_files) pending=$($st2.pending_files)" | Add-Content $log
  if($code -ne 0){ Start-Sleep 8 }  # rate-limit backoff before resume
}
"done $(Get-Date -Format o)" | Add-Content $log
