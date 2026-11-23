# LeeConnect Phase 4 patch replayer: applies ledger PATCH_BYTE onto a build copy.
# Usage: .\apply-patches.ps1 -Binary <build-copy> -Ledger client\patch\patches.json -PatchId PC-001 [-DryRun]
# Ledger offsets are file offsets (0x hex). First write leaves a .orig backup; original_bytes fingerprint guards wrong offsets. ASCII-only source.
param(
  [Parameter(Mandatory)][string]$Binary,
  [Parameter(Mandatory)][string]$Ledger,
  [Parameter(Mandatory)][string]$PatchId,
  [switch]$DryRun
)
$ErrorActionPreference = 'Stop'
$entry = (Get-Content -LiteralPath $Ledger -Raw -Encoding UTF8 | ConvertFrom-Json).patches | Where-Object id -eq $PatchId
if (-not $entry) { Write-Error "patch $PatchId not in ledger"; exit 1 }
if (-not (Test-Path $Binary)) { Write-Error "binary not found: $Binary"; exit 1 }
$full = (Resolve-Path $Binary).Path

# 1) OFFSET from ledger (placeholder => skip gracefully until Phase 1 fills it)
$offText = $null
if ($entry.target -match '0x([0-9a-fA-F]+)') { $offText = $Matches[1] }
elseif ($entry.offset -match '0x([0-9a-fA-F]+)') { $offText = $Matches[1] }
if (-not $offText) { Write-Host "[patch] $PatchId has no OFFSET yet (fill ledger after Ghidra/IDA locate); skip"; exit 0 }
$offset = [Convert]::ToUInt64($offText, 16)

# 2) PATCH_BYTE slot resolution
$pb = [string]$entry.patch_byte
if ($pb -match '^0x([0-9a-fA-F]{2})$') { $patchBytes = @([Convert]::ToByte($Matches[1], 16)) }
elseif ($pb -match '(?i)nop') { $n = 3; if ($pb -match 'x(\d+)') { $n = [int]$Matches[1] }; $patchBytes = @(); for ($j = 0; $j -lt $n; $j++) { $patchBytes += 0x90 } }
elseif ($pb -match '^([0-9a-fA-F]{2}( [0-9a-fA-F]{2})*)$') { $patchBytes = @(); foreach ($t in ($pb -split '\s+')) { $patchBytes += [Convert]::ToByte($t, 16) } }
else { $patchBytes = @(0xB0, 0x01, 0xC3) }   # mov al,1 / ret generic entitlement-true slot

# 3) read - fingerprint - patch - write
$bytes = [System.IO.File]::ReadAllBytes($full)
if ($offset + $patchBytes.Count -gt $bytes.Length) { Write-Error ("offset 0x{0:x} out of bounds (file={1})" -f $offset, $bytes.Length); exit 1 }
$before = @(); for ($j = 0; $j -lt $patchBytes.Count; $j++) { $before += $bytes[$offset + $j] }
function Fmt($a) { ($a | ForEach-Object { '{0:x2}' -f $_ }) -join ' ' }
Write-Host ("[patch] {0} @ file+0x{1:x}  before=[{2}]" -f $PatchId, $offset, (Fmt $before))

$ob = [string]$entry.original_bytes
if ($ob -and $ob -notmatch '(?i)^\[?(PENDING|READ)') {
  $exp = @(); foreach ($t in ($ob -split '\s+')) { $exp += [Convert]::ToByte($t, 16) }
  $diff = Compare-Object $exp $before -SyncWindow 0
  if ($diff) { Write-Error ("fingerprint mismatch (expected [{0}]) - wrong offset or sample version; abort" -f (Fmt $exp)); exit 1 }
  Write-Host '[patch] original_bytes fingerprint OK'
}
if ($DryRun) { Write-Host ("[patch] dry-run: would write [{0}]" -f (Fmt $patchBytes)); exit 0 }

if (-not (Test-Path "$full.orig")) { Copy-Item $full "$full.orig" }
for ($j = 0; $j -lt $patchBytes.Count; $j++) { $bytes[$offset + $j] = $patchBytes[$j] }
[System.IO.File]::WriteAllBytes($full, $bytes)
Write-Host ("[patch] applied {0} -> [{1}]  (rollback: restore .orig)" -f $PatchId, (Fmt $patchBytes))
