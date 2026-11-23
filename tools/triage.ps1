# LeeConnect Phase 1 static triage: packer/framework, CHECK_FN candidate strings, domains.
# Usage: .\triage.ps1 -Sample ..\sandbox\uu\SAMPLE_UU.exe
# ASCII-only source (BOM-less scripts parsed as ANSI by some hosts). Byte-array impl, no .Position.
param(
  [Parameter(Mandatory)][string]$Sample,
  [string]$OutDir = "$PSScriptRoot\..\docs\triage"
)
$ErrorActionPreference = 'Stop'
if (-not (Test-Path $Sample)) { Write-Error "sample not found: $Sample"; exit 1 }
New-Item -ItemType Directory -Force $OutDir | Out-Null
$full = (Resolve-Path $Sample).Path
$bytes = [System.IO.File]::ReadAllBytes($full)
Write-Host "[triage] $full  $([math]::Round($bytes.Length/1MB,1)) MB"
Write-Host ("sha256: " + (Get-FileHash $full -Algorithm SHA256).Hash)

# 1) PE header summary
if ($bytes.Length -ge 0x40 -and $bytes[0] -eq 0x4D -and $bytes[1] -eq 0x5A) {
  $pe = [BitConverter]::ToInt32($bytes, 0x3C)
  if ($pe -ge 0 -and $pe + 24 -lt $bytes.Length -and [BitConverter]::ToInt32($bytes, $pe) -eq 0x4550) {
    $machine = [BitConverter]::ToUInt16($bytes, $pe + 4)
    $nsec    = [BitConverter]::ToUInt16($bytes, $pe + 6)
    $optSize = [BitConverter]::ToUInt16($bytes, $pe + 20)
    $chars   = [BitConverter]::ToUInt16($bytes, $pe + 22)
    $isDll   = ($chars -band 0x2000) -ne 0
    $magic   = [BitConverter]::ToUInt16($bytes, $pe + 24)
    Write-Host ("PE: machine=0x{0:x4} sections={1} dll={2} pe32+={3}" -f $machine, $nsec, $isDll, ($magic -eq 0x20b))
    $secOff = $pe + 24 + $optSize
    for ($i = 0; $i -lt $nsec -and ($secOff + $i * 40 + 40) -le $bytes.Length; $i++) {
      $o = $secOff + $i * 40
      $name = ([System.Text.Encoding]::ASCII.GetString($bytes, $o, 8) -replace '[^ -~]', '').PadRight(10)
      $vsize = [BitConverter]::ToUInt32($bytes, $o + 8)
      $vaddr = [BitConverter]::ToUInt32($bytes, $o + 12)
      $raw   = [BitConverter]::ToUInt32($bytes, $o + 16)
      Write-Host ("  {0} vsize={1,-9} vaddr=0x{2:x8} raw={3}" -f $name, $vsize, $vaddr, $raw)
    }
  } else { Write-Host 'PE header invalid (raw dump or not PE)' }
} else { Write-Host 'no MZ header (APK/resource/data -> mobile line)' }

# 2) strings via Latin1 regex (printable runs >= 6)
$latin1 = [System.Text.Encoding]::GetEncoding(28591).GetString($bytes)
$strings = [regex]::Matches($latin1, '[\x20-\x7e]{6,}') | ForEach-Object Value | Select-Object -Unique
Set-Content -LiteralPath "$OutDir\strings.txt" -Value $strings -Encoding UTF8
Write-Host ("strings>=6: " + @($strings).Count)

# 3) feature buckets (CHECK_FN candidates + countermeasures + protocol leads)
$buckets = [ordered]@{
  brand       = '(?i)\buu\b|netease'
  domains     = '(?i)[a-z0-9.-]+\.(netease\.com|163\.com|uu\.com|leidian\.com|nx\.net)\b'
  entitlement = '(?i)member|vip|privilege|entitlement|resolution|fps|gamepad|privacy.?screen|license'
  webrtc      = '(?i)webrtc|sdp|\bice\b|\bturn\b|stun|opus|x264|x265|openh264'
  antidebug   = '(?i)CheckRemoteDebuggerPresent|IsDebuggerPresent|NtGlobalFlag|BeingDebugged|OutputDebugString|vmware|vbox|virtualbox|qemu'
  crypto      = '(?i)ssl_|tls_|X509|CertOpenStore|CryptVerify|pinning|certificate'
  updater     = '(?i)update|upgrade|signature|Authenticode|WinVerify'
  protocol    = '(?i)[a-z]+://[a-z]+[?=/]'
}
foreach ($k in $buckets.Keys) {
  $hits = @($strings | Where-Object { $_ -match $buckets[$k] } | Select-Object -First 60)
  Set-Content -LiteralPath "$OutDir\hits-$k.txt" -Value $hits -Encoding UTF8   # explicit -Value truncates stale file even when empty
  Write-Host ("  [{0,-11}] {1} hits -> hits-{0}.txt" -f $k, $hits.Count)
}
Write-Host "[triage] done -> $((Resolve-Path $OutDir).Path)"
