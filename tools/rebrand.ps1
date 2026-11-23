# LeeConnect Phase 4 rebrand sweep: string-table level brand replacement on a build copy.
# Full RCData/ICON/PNG swap needs ResourceHacker; this covers embedded null-terminated strings + protocol scheme.
# Usage: .\rebrand.ps1 -Binary <build-copy> [-OldBrand UU] [-NewBrand LeeConnect] [-DryRun]
# In-place swaps only (file size constant). Longer names need null slack after the string, else skipped -> ResourceHacker.
param(
  [Parameter(Mandatory)][string]$Binary,
  [string]$OldBrand = 'UU',
  [string]$NewBrand = 'LeeConnect',
  [string]$OldProto = 'uu',
  [switch]$DryRun
)
$ErrorActionPreference = 'Stop'
if (-not (Test-Path $Binary)) { Write-Error "binary not found: $Binary"; exit 1 }
$full = (Resolve-Path $Binary).Path
if (-not (Test-Path "$full.pre-brand")) { Copy-Item $full "$full.pre-brand" }
$bytes = [System.IO.File]::ReadAllBytes($full)

function Replace-Bytes($hay, $needle, $repl) {
  # in-place swap. Longer repl: original tail shifts right into null slack; without slack -> skipped.
  $out = New-Object byte[] $hay.Length
  [Array]::Copy($hay, $out, $hay.Length)
  $count = 0; $skipped = 0
  for ($i = 0; $i -le ($hay.Length - $needle.Length); $i++) {
    $m = $true
    for ($j = 0; $j -lt $needle.Length; $j++) { if ($hay[$i + $j] -ne $needle[$j]) { $m = $false; break } }
    if (-not $m) { continue }
    $term = $i + $needle.Length
    while ($term -lt $hay.Length -and $hay[$term] -ne 0) { $term++ }   # this string's null terminator
    $tail = $term - ($i + $needle.Length)                              # original suffix after needle
    if ($repl.Length + $tail -lt ($hay.Length - $i)) {                 # room to fit within file
      if ($repl.Length -ge $needle.Length) {
        $slackOk = $true
        for ($k = $term; $k -le ($i + $repl.Length + $tail); $k++) { if ($hay[$k] -ne 0) { $slackOk = $false; break } }
        if (-not $slackOk) { $skipped++; continue }                    # would eat into next entry
        for ($k = ($tail - 1); $k -ge 0; $k--) { $out[$i + $repl.Length + $k] = $hay[$i + $needle.Length + $k] }
      }
      for ($k = 0; $k -lt $repl.Length; $k++) { $out[$i + $k] = $repl[$k] }
      $out[$i + $repl.Length + $tail] = 0
      $count++; $i += $needle.Length - 1                               # resume after original needle span
    } else { $skipped++ }
  }
  , @($out, $count, $skipped)
}

function Replace-Bytes16($hay, $needle16, $repl16) {
  # UTF-16LE variant: terminator = pair of null bytes; same in-place slack rules.
  $out = New-Object byte[] $hay.Length
  [Array]::Copy($hay, $out, $hay.Length)
  $count = 0; $skipped = 0
  $nl = $needle16.Length
  for ($i = 0; $i -le ($hay.Length - $nl); $i += 2) {
    $m = $true
    for ($j = 0; $j -lt $nl; $j++) { if ($hay[$i + $j] -ne $needle16[$j]) { $m = $false; break } }
    if (-not $m) { continue }
    $term = $i + $nl
    while (($term + 1) -lt $hay.Length -and -not ($hay[$term] -eq 0 -and $hay[$term + 1] -eq 0)) { $term += 2 }
    $tail = $term - ($i + $nl)
    if ($repl16.Length + $tail -ge ($hay.Length - $i)) { $skipped++; continue }
    if ($repl16.Length -ge $nl) {
      $slackOk = $true
      for ($k = $term; $k -le ($i + $repl16.Length + $tail); $k += 2) { if (-not ($hay[$k] -eq 0 -and $hay[$k + 1] -eq 0)) { $slackOk = $false; break } }
      if (-not $slackOk) { $skipped++; continue }
      for ($k = ($tail - 2); $k -ge 0; $k -= 2) { $out[$i + $repl16.Length + $k] = $hay[$i + $nl + $k] }
    }
    for ($k = 0; $k -lt $repl16.Length; $k++) { $out[$i + $k] = $repl16[$k] }
    $out[$i + $repl16.Length + $tail] = 0
    $count++
  }
  , @($out, $count, $skipped)
}

# 1) protocol scheme swap (uu:// -> leeconnect://)
$oldA = [System.Text.Encoding]::ASCII.GetBytes("${OldProto}://")
$newA = [System.Text.Encoding]::ASCII.GetBytes(($NewBrand.ToLower() + '://'))
$res = Replace-Bytes $bytes $oldA $newA
Write-Host ("[brand] scheme '{0}://' -> '{1}://' hits={2} skipped={3}" -f $OldProto, $NewBrand.ToLower(), $res[1], $res[2])
$bytes2 = $res[0]; $hits = $res[1]

# 2) brand word sweep (case variants, equal or shorter repl safe via tail shift)
$variants = @($OldBrand, $OldBrand.ToLower(), $OldBrand.ToUpper()) | Select-Object -Unique
foreach ($v in $variants) {
  $nb = [System.Text.Encoding]::ASCII.GetBytes($v)
  $nr = [System.Text.Encoding]::ASCII.GetBytes($NewBrand)
  $r2 = Replace-Bytes $bytes2 $nb $nr
  $bytes2 = $r2[0]; $hits += $r2[1]
  Write-Host ("[brand] word '{0}' -> '{1}' hits={2} skipped={3}" -f $v, $NewBrand, $r2[1], $r2[2])
}

# 3) wide (UTF-16LE) brand sweep - real binaries store display names as wchar
$cjkRemote = [string][char]0x8FDC + [char]0x7A0B                 # 'remote' CJK, built from code points (ASCII-only source)
foreach ($v in @($OldBrand, $OldBrand.ToLower(), $OldBrand.ToUpper(), ($OldBrand + $cjkRemote))) {
  $nb16 = [System.Text.Encoding]::Unicode.GetBytes($v)
  $nr16 = [System.Text.Encoding]::Unicode.GetBytes($NewBrand)
  $r3 = Replace-Bytes16 $bytes2 $nb16 $nr16
  $bytes2 = $r3[0]; $hits += $r3[1]
  if ($r3[1] -gt 0) { Write-Host ("[brand] wide '{0}' -> '{1}' hits={2}" -f $v, $NewBrand, $r3[1]) }
}

# residual audit on the new image
$latin1 = [System.Text.Encoding]::GetEncoding(28591).GetString($bytes2)
$strings = [regex]::Matches($latin1, '[\x20-\x7e]{6,}') | ForEach-Object Value | Select-Object -Unique
$residual = @($strings | Where-Object { $_ -match "(?i)$OldBrand" } | Select-Object -First 10)
Write-Host ("[brand] residual lines: " + $residual.Count)
$residual | ForEach-Object { Write-Host "   $_" }

if ($DryRun) { Write-Host '[brand] dry-run, file untouched'; exit 0 }
if ($hits -gt 0) { [System.IO.File]::WriteAllBytes($full, $bytes2); Write-Host "[brand] written ($hits swaps)" } else { Write-Host '[brand] 0 auto-hits, file untouched (ResourceHacker path)' }
if ($residual.Count -gt 0) { Write-Warning 'residuals remain (RCData/icons/short strings) - finish via ResourceHacker then re-run audit' }
