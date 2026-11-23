# LeeConnect Phase 2 rollback: remove hosts block, uninstall root cert, restore config INIs.
# Self-elevating like apply. Usage: .\phase2-revert.ps1
$ErrorActionPreference = 'Continue'
$root = Split-Path -Parent $PSScriptRoot

$cur = [Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
if (-not $cur.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
  Write-Host '[revert] not elevated -> spawning UAC...'
  $ps = Join-Path $env:WINDIR 'System32\WindowsPowerShell\v1.0\powershell.exe'
  Start-Process $ps -Verb RunAs -ArgumentList @('-NoProfile','-ExecutionPolicy','Bypass','-File', "`"$PSCommandPath`"")
  exit
}

# 1) hosts block
$hostsPath = 'C:\Windows\System32\drivers\etc\hosts'
$text = [System.IO.File]::ReadAllText($hostsPath)
$marker = '# LeeConnect mock-cloud'
if ($text -match [regex]::Escape($marker)) {
  $lines = [System.IO.File]::ReadAllLines($hostsPath)
  $out = New-Object System.Collections.Generic.List[string]
  $skip = 0
  foreach ($l in $lines) {
    if ($l -eq $marker) { $skip = 6; continue }     # marker + 6 redirect lines
    if ($skip -gt 0) { $skip--; continue }
    $out.Add($l)
  }
  [System.IO.File]::WriteAllLines($hostsPath, $out)
  Write-Host '[revert] hosts block removed'
} else { Write-Host '[revert] hosts already clean' }
ipconfig /flushdns | Out-Null

# 2) root cert
$th = Get-ChildItem Cert:\LocalMachine\Root | Where-Object Subject -match 'api\.nrd\.nie\.163\.com'
if ($th) { Remove-Item "Cert:\LocalMachine\Root\$($th.Thumbprint)" -Recurse; Write-Host ('[revert] cert removed: ' + $th.Thumbprint) } else { Write-Host '[revert] cert not present' }

# 3) config INIs
$cfg = 'D:\Program Files\Netease\GameViewer\config\server'
Get-ChildItem $cfg -Filter *.ini.lee-bak -ErrorAction SilentlyContinue | ForEach-Object {
  $orig = $_.FullName -replace '\.lee-bak$'
  Move-Item $_.FullName $orig -Force
  Write-Host ('[revert] restored ' + (Split-Path $orig -Leaf))
}

Write-Host '[revert] done. Restart UU client to rejoin real cloud.'
