# LeeConnect Phase 2 one-shot: elevate via UAC once, then install cert + hosts + mock-cloud.
# Usage: right-click Run as PowerShell, OR just .\phase2-apply.ps1 (it self-elevates).
# Roll back everything with .\phase2-revert.ps1 (also self-elevating).
$ErrorActionPreference = 'Stop'
$ws = Split-Path -Parent $PSScriptRoot          # tools -> workspace root (E:\CODE\server-manga-log)
$root = $ws

# self-elevate
$cur = [Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
if (-not $cur.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
  Write-Host '[phase2] not elevated -> spawning UAC prompt...'
  $ps = Join-Path $env:WINDIR 'System32\WindowsPowerShell\v1.0\powershell.exe'
  Start-Process $ps -Verb RunAs -ArgumentList @('-NoProfile','-ExecutionPolicy','Bypass','-File', "`"$PSCommandPath`"")
  exit
}

# 1) trusted root (LocalMachine so SYSTEM service GameViewerService validates it too)
$pfx = Join-Path $root 'sandbox\mockcloud\mockcloud.pfx'
if (-not (Test-Path $pfx)) { Write-Error "missing $pfx -> run tools/gen-cert.ps1 first"; exit 1 }
$pw = ConvertTo-SecureString -String 'leeconnect' -Force -AsPlainText
$existing = Get-ChildItem Cert:\LocalMachine\Root | Where-Object Subject -match 'api\.nrd\.nie\.163\.com'
if ($existing) { Write-Host ('[phase2] cert already in LocalMachine\Root: ' + $existing.Thumbprint) }
else {
  $imported = Import-PfxCertificate -FilePath $pfx -CertStoreLocation Cert:\LocalMachine\Root -Password $pw
  Write-Host ('[phase2] cert installed: ' + $imported.Thumbprint)
  ($imported.Thumbprint) | Set-Content (Join-Path $root 'sandbox\mockcloud\installed-thumbprint.txt')
}

# 2) hosts redirect (idempotent, marker-guarded)
$hostsPath = 'C:\Windows\System32\drivers\etc\hosts'
$text = [System.IO.File]::ReadAllText($hostsPath)
$marker = '# LeeConnect mock-cloud'
if ($text -notmatch [regex]::Escape($marker)) {
  $lines = @(
    $marker
    '127.0.0.1 api.nrd.nie.163.com'
    '127.0.0.1 api-ipv4.nrd.nie.163.com'
    '127.0.0.1 api-dcdn.nrd.nie.163.com'
    '127.0.0.1 api-event.nrd.nie.163.com'
    '127.0.0.1 fcount-api.webapp.163.com'
    '127.0.0.1 sentry.netease.com'
  ) -join "`r`n"
  [System.IO.File]::AppendAllText($hostsPath, "`r`n" + $lines + "`r`n")
  Write-Host '[phase2] hosts: 6 domains -> 127.0.0.1'
} else { Write-Host '[phase2] hosts: already redirected' }
ipconfig /flushdns | Out-Null

# 3) mock-cloud on 443 in an elevated window (keeps running after this script exits)
$running = Get-NetTCPConnection -LocalPort 443 -State Listen -ErrorAction SilentlyContinue
if ($running) { Write-Host '[phase2] port 443 already listening -> skip spawn' }
else {
  $node = (Get-Command node.exe -ErrorAction SilentlyContinue).Source
  if (-not $node) { $node = Join-Path ${env:ProgramFiles} 'nodejs\node.exe' }
  $ps2 = Join-Path $env:WINDIR 'System32\WindowsPowerShell\v1.0\powershell.exe'
  Start-Process $ps2 -ArgumentList '-NoProfile','-WindowStyle','Normal','-Command',"`"$node '$root\tools\mock-cloud.js'`""
  Write-Host '[phase2] mock-cloud spawned on :443 (check its window)'
}

Write-Host ''
Write-Host '[phase2] DONE. Now: restart UU (exit tray -> relaunch GameViewer.exe), then watch:'
Write-Host '   - mock-cloud window logs: GET /api/v1/member/entitlements -> 200'
Write-Host '   - client member state flips to VIP (member_type=3)'
Write-Host '   - if TLS pinning blocks (no GET in mock log): run tools/frida attach path (RE-NOTES §2)'
