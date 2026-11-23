# LeeConnect mock-cloud cert: SAN self-signed cert for extracted API domains.
# Usage: .\gen-cert.ps1   -> produces sandbox\mockcloud\{key.pem,cert.pem,mockcloud.pfx}
# Then import mockcloud.pfx into LocalMachine Trusted Root (admin) and point hosts at 127.0.0.1.
$ErrorActionPreference = 'Stop'
$out = Join-Path $PSScriptRoot '..\sandbox\mockcloud'
New-Item -ItemType Directory -Force $out | Out-Null

$domains = @(
  'api.nrd.nie.163.com', 'api-ipv4.nrd.nie.163.com', 'api-dcdn.nrd.nie.163.com',
  'api-event.nrd.nie.163.com', 'fcount-api.webapp.163.com', 'sentry.netease.com', 'localhost'
)
$cert = New-SelfSignedCertificate -DnsName $domains -CertStoreLocation 'Cert:\CurrentUser\My' `
  -NotAfter (Get-Date).AddYears(10) -KeyExportPolicy Exportable -KeyAlgorithm RSA -KeyLength 2048 -Hash SHA256
Write-Host ('cert thumbprint: ' + $cert.Thumbprint)
Write-Host ('SAN: ' + ($domains -join ', '))

$pw = ConvertTo-SecureString -String 'leeconnect' -Force -AsPlainText
$pfx = Join-Path $out 'mockcloud.pfx'
Export-PfxCertificate -Cert $cert -FilePath $pfx -Password $pw | Out-Null
Write-Host ('pfx: ' + $pfx + '  (password: leeconnect)')

if (Get-Command openssl -ErrorAction SilentlyContinue) {
  openssl pkcs12 -in $pfx -passin pass:leeconnect -nodes -out (Join-Path $out 'bundle.pem') 2>&1 | Out-Null
  openssl rsa -in (Join-Path $out 'bundle.pem') -out (Join-Path $out 'key.pem') 2>&1 | Out-Null
  openssl x509 -in (Join-Path $out 'bundle.pem') -out (Join-Path $out 'cert.pem') 2>&1 | Out-Null
  Remove-Item (Join-Path $out 'bundle.pem') -Force
  Write-Host ('pem: key.pem + cert.pem ready -> run node tools/mock-cloud.js')
} else {
  Write-Host 'openssl missing: split key.pem/cert.pem from the PFX manually (pw: leeconnect) or use an https server that takes pfx'
}
Write-Host 'trust (admin, one line):'
Write-Host "  Import-PfxCertificate -FilePath '$pfx' -CertStoreLocation Cert:\LocalMachine\Root -Password (ConvertTo-SecureString -String 'leeconnect' -Force -AsPlainText)"
