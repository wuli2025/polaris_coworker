$exe="D:\polaris\polaris-app\src-tauri\target\release\polaris-forge.exe"
$db="$env:USERPROFILE\Polaris\data\fable.db"
$log="D:\polaris\polaris-app\_scratch_soak.log"
if(-not (Test-Path $log)){ "ts,iter,exit,chunks,lex,pending,db_mb,call_s,cum_s" | Set-Content $log }
$start=Get-Date
for($iter=1; $iter -le 400; $iter++){
  $st=& $exe fable status 2>$null|ConvertFrom-Json
  if($st.pending_files -le 5){ "$(Get-Date -Format o),$iter,DONE,$($st.chunks_total),$($st.lex_files),$($st.pending_files),,,$([math]::Round(((Get-Date)-$start).TotalSeconds))" | Add-Content $log; break }
  $c0=Get-Date
  & $exe fable index --max-chunks=600 1>$null 2>$null
  $code=$LASTEXITCODE
  $callS=[math]::Round(((Get-Date)-$c0).TotalSeconds,1)
  $st2=& $exe fable status 2>$null|ConvertFrom-Json
  $mb=[math]::Round((Get-Item $db -ErrorAction SilentlyContinue).Length/1MB,1)
  "$(Get-Date -Format o),$iter,$code,$($st2.chunks_total),$($st2.lex_files),$($st2.pending_files),$mb,$callS,$([math]::Round(((Get-Date)-$start).TotalSeconds))" | Add-Content $log
  if($code -ne 0){ Start-Sleep 8 }
}
