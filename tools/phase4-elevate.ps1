# LeeConnect Phase 4+5 elevation: trust signing cert, autostart mock-cloud, restart UU.
# Self-elevating. Usage: .\phase4-elevate.ps1
$ErrorActionPreference = 'Continue'
$root = Split-Path -Parent $PSScriptRoot

$cur = [Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
if (-not $cur.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
  Write-Host '[elev] not elevated -> UAC...'
  $ps = Join-Path $env:WINDIR 'System32\WindowsPowerShell\v1.0\powershell.exe'
  Start-Process $ps -Verb RunAs -ArgumentList @('-NoProfile','-ExecutionPolicy','Bypass','-File', "`"$PSCommandPath`"")
  exit
}

# 1) trust the code-signing cert (CurrentUser\My has it; copy to LocalMachine Root+Trust so Authenticode validates)
$sign = Get-ChildItem Cert:\CurrentUser\My | Where-Object Subject -match 'LeeConnect Code Signing' | Select-Object -First 1
if ($sign) {
  $tmp = Join-Path $env:TEMP 'leeconnect-signing.cer'
  [System.IO.File]::WriteAllBytes($tmp, $sign.Export('Cert'))
  Import-Certificate -FilePath $tmp -CertStoreLocation Cert:\LocalMachine\Root | Out-Null
  Import-Certificate -FilePath $tmp -CertStoreLocation Cert:\LocalMachine\TrustedPeople | Out-Null
  Remove-Item $tmp -Force
  Write-Host ('[elev] signing cert trusted: ' + $sign.Thumbprint)
} else { Write-Host '[elev] signing cert missing in CurrentUser\My' }

# 2) verify Authenticode status now
foreach ($f in 'LeeConnect.exe', 'LeeConnect-Host.exe') {
  $st = Get-AuthenticodeSignature (Join-Path $root "client\build\$f")
  Write-Host ("[elev] {0} -> {1}" -f $f, $st.Status)
}

# 3) autostart mock-cloud (SYSTEM, at boot + now, on :443)
$node = (Get-Command node.exe -ErrorAction SilentlyContinue).Source
if (-not $node) { $node = Join-Path ${env:ProgramFiles} 'nodejs\node.exe' }
$action = New-ScheduledTaskAction -Execute $node -Argument "`"$root\tools\mock-cloud.js`"" -WorkingDirectory $root
$trigger = New-ScheduledTaskTrigger -AtStartup
$settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -RestartCount 3 -RestartInterval (New-TimeSpan -Minutes 1)
Unregister-ScheduledTask -TaskName 'LeeConnectMockCloud' -Confirm:$false -ErrorAction SilentlyContinue
Register-ScheduledTask -TaskName 'LeeConnectMockCloud' -Action $action -Trigger $trigger -Settings $settings -User 'SYSTEM' -RunLevel Highest | Out-Null
Start-ScheduledTask -TaskName 'LeeConnectMockCloud'
Write-Host '[elev] LeeConnectMockCloud registered + started (SYSTEM, at boot)'

# 4) restart UU client so it re-pulls configures_with_version (now full-unlock table)
Stop-Process -Name GameViewer, crashpad_handler -Force -ErrorAction SilentlyContinue
Start-Sleep 2
Restart-Service GameViewerService -Force -ErrorAction SilentlyContinue
$gv = 'D:\Program Files\Netease\GameViewer'
Start-Process "$gv\bin\GameViewer.exe" -WorkingDirectory "$gv\bin"
Write-Host '[elev] UU restarted -> watch mock-cloud log for POST /api/v1/tool/configures_with_version'
Start-Sleep 12
Get-Content (Join-Path $root 'sandbox\mockcloud\phase2.log') -Tail 3 -ErrorAction SilentlyContinue | Out-Null
