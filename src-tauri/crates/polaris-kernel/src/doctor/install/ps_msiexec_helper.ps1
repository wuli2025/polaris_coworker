
$global:PolarisMsiCode = -1
function Install-PolarisMsi {
  param([string]$Path, [string]$ExtraArgs = '')
  foreach ($try in 1..3) {
    try {
      $a = '/i "' + $Path + '" /quiet /norestart'
      if ($ExtraArgs -ne '') { $a = $a + ' ' + $ExtraArgs }
      $p = Start-Process msiexec.exe -ArgumentList $a -Wait -PassThru -Verb RunAs
      $global:PolarisMsiCode = $p.ExitCode
    } catch {
      Write-Output ('安装启动失败 (可能未授予管理员权限): ' + $_.Exception.Message)
      $global:PolarisMsiCode = -1
      return
    }
    if ($global:PolarisMsiCode -ne 1618) { return }
    Write-Output ('另一个安装程序正在运行 (msiexec 1618), 等 15 秒重试 ({0}/3)...' -f $try)
    Start-Sleep -Seconds 15
  }
}
