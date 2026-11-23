# LeeConnect: extend hosts takeover with signaling domains (uuyc.*) + verify task/cert state.
# Self-elevating. Usage: .\extend-takeover.ps1
$ErrorActionPreference = 'Continue'
$root = Split-Path -Parent $PSScriptRoot

$cur = [Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
if (-not $cur.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
  $ps = Join-Path $env:WINDIR 'System32\WindowsPowerShell\v1.0\powershell.exe'
  Start-Process $ps -Verb RunAs -ArgumentList @('-NoProfile','-ExecutionPolicy','Bypass','-File', "`"$PSCommandPath`"")
  exit
}

# 1) extend hosts block with signaling + feedback + ugc domains
$hostsPath = 'C:\Windows\System32\drivers\etc\hosts'
$text = [System.IO.File]::ReadAllText($hostsPath)
$extra = @(
  'uuyc.163.com', 'uuyc.webapp.163.com', 'uuyc-aide-customer.webapp.163.com',
  'feedback-web.webapp.163.com', 'nrd-ugc.fp.ps.netease.com', 'adl.netease.com', 'uu.163.com'
) | Where-Object { $text -notmatch [regex]::Escape($_) }
if ($extra) {
  $block = ($extra | ForEach-Object { "127.0.0.1 $_" }) -join "`r`n"
  [System.IO.File]::AppendAllText($hostsPath, "`r`n# LeeConnect mock-cloud ext`r`n" + $block + "`r`n")
  Write-Host ('[ext] hosts added: ' + ($extra -join ', '))
} else { Write-Host '[ext] hosts already extended' }
ipconfig /flushdns | Out-Null

# 2) regen cert with full SAN set (overwrite old), reimport trusted
$domains = @(
  'api.nrd.nie.163.com','api-ipv4.nrd.nie.163.com','api-dcdn.nrd.nie.163.com','api-event.nrd.nie.163.com',
  'fcount-api.webapp.163.com','sentry.netease.com','localhost',
  'uuyc.163.com','uuyc.webapp.163.com','uuyc-aide-customer.webapp.163.com',
  'feedback-web.webapp.163.com','nrd-ugc.fp.ps.netease.com','adl.netease.com','uu.163.com'
)
$old = Get-ChildItem Cert:\CurrentUser\My | Where-Object Subject -match 'api\.nrd\.nie'
if ($old) { Remove-Item "Cert:\CurrentUser\My\$($old.Thumbprint)" -Recurse -Force }
$cert = New-SelfSignedCertificate -DnsName $domains -CertStoreLocation 'Cert:\CurrentUser\My' `
  -NotAfter (Get-Date).AddYears(10) -KeyExportPolicy Exportable -KeyAlgorithm RSA -KeyLength 2048 -Hash SHA256
$pw = ConvertTo-SecureString -String 'leeconnect' -Force -AsPlainText
$pfx = Join-Path $root 'sandbox\mockcloud\mockcloud.pfx'
Export-PfxCertificate -Cert $cert -FilePath $pfx -Password $pw | Out-Null
$prev = Get-ChildItem Cert:\LocalMachine\Root | Where-Object Subject -match 'api\.nrd\.nie'
if ($prev) { Remove-Item "Cert:\LocalMachine\Root\$($prev.Thumbprint)" -Force }
Import-PfxCertificate -FilePath $pfx -CertStoreLocation Cert:\LocalMachine\Root -Password $pw | Out-Null
Write-Host ('[ext] cert regen: ' + $cert.Thumbprint + ' (SAN x' + $domains.Count + ')')

# 3) task state + restart with fresh cert
$info = Get-ScheduledTaskInfo -TaskName 'LeeConnectMockCloud' -ErrorAction SilentlyContinue
if ($info) { Write-Host ('[ext] task: last=' + $info.LastRunTime + ' result=' + $info.LastTaskResult) }
else { Write-Host '[ext] task MISSING -> re-register' }
if (-not $info) {
  $node = (Get-Command node.exe -EA SilentlyContinue).Source; if (-not $node) { $node = Join-Path ${env:ProgramFiles} 'nodejs\node.exe' }
  $a = New-ScheduledTaskAction -Execute $node -Argument "`"$root\tools\mock-cloud.js`"" -WorkingDirectory $root
  $t = New-ScheduledTaskTrigger -AtStartup
  $s = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -RestartCount 3 -RestartInterval (New-TimeSpan -Minutes 1)
  Register-ScheduledTask -TaskName 'LeeConnectMockCloud' -Action $a -Trigger $t -Settings $s -User 'SYSTEM' -RunLevel Highest | Out-Null
}
Stop-ScheduledTask -TaskName 'LeeConnectMockCloud' -EA SilentlyContinue
Start-Sleep 1
Start-ScheduledTask -TaskName 'LeeConnectMockCloud'
Start-Sleep 3
Write-Host ('[ext] 443 listeners: ' + (Get-NetTCPConnection -LocalPort 443 -State Listen -EA SilentlyContinue | Measure-Object).Count)

# 4) bounce UU to re-handshake through new SAN set
Stop-Process -Name GameViewer, crashpad_handler -Force -EA SilentlyContinue
Start-Sleep 2
Start-Process 'D:\Program Files\Netease\GameViewer\bin\GameViewer.exe' -WorkingDirectory 'D:\Program Files\Netease\GameViewer\bin'
Write-Host '[ext] UU bounced'
