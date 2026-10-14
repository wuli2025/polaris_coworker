
$ErrorActionPreference = 'Continue'
$ProgressPreference = 'SilentlyContinue'
try { [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12 } catch {}

# 结果写 $global:PolarisDlOk 而不是用返回值 —— 函数里的 Write-Output 会混进返回值 (PowerShell 老坑)。
$global:PolarisDlOk = $false
function Get-PolarisFile {
  param([string[]]$Urls, [string]$Dest, [string]$Magic, [long]$MinBytes)
  $global:PolarisDlOk = $false
  foreach ($round in 1..2) {
    foreach ($u in $Urls) {
      try {
        Write-Output "下载: $u"
        if (Test-Path $Dest) { Remove-Item $Dest -Force -ErrorAction SilentlyContinue }
        Invoke-WebRequest -Uri $u -OutFile $Dest -UseBasicParsing -TimeoutSec 600
        if (-not (Test-Path $Dest)) { Write-Output '  下载后没见到文件'; continue }
        $len = (Get-Item $Dest).Length
        if ($len -lt $MinBytes) {
          Write-Output ('  体积不对: {0:N0} 字节, 至少该有 {1:N0} —— 多半是代理回的错误页' -f $len, $MinBytes)
          continue
        }
        $need = [int]($Magic.Length / 2)
        $head = New-Object byte[] $need
        $fs = [IO.File]::OpenRead($Dest)
        $read = $fs.Read($head, 0, $need)
        $fs.Close()
        if ($read -lt $need) { Write-Output '  文件头都读不满, 显然不对'; continue }
        $got = (($head | ForEach-Object { $_.ToString('x2') }) -join '')
        if ($got -ne $Magic) {
          Write-Output ('  文件头不对: {0} (应为 {1}) —— 多半是代理回的错误页' -f $got, $Magic)
          continue
        }
        Write-Output ('  校验通过: {0:N1} MB' -f ($len / 1MB))
        $global:PolarisDlOk = $true
        return
      } catch {
        Write-Output ('  下载失败: ' + $_.Exception.Message)
      }
    }
    if ($round -eq 1) { Write-Output '一轮镜像都没成功, 歇 2 秒再来一轮...'; Start-Sleep -Seconds 2 }
  }
}
