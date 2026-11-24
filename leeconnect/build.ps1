# LeeConnect build wrapper: clears the stale CC/CXX env (w64devkit gcc) that
# hijacks cc-rs for ring/openh264-sys2 on MSVC targets, then runs cargo.
# Usage: .\build.ps1 [cargo args...]   e.g. .\build.ps1 test --lib
param([Parameter(ValueFromRemainingArguments = $true)][string[]]$Args = @('build'))
$ErrorActionPreference = 'Continue'
Remove-Item Env:CC, Env:CXX -ErrorAction SilentlyContinue
$env:CARGO_HOME = 'E:\rust\.cargo'
$env:RUSTUP_HOME = 'E:\rust\.rustup'
$env:PATH = "E:\rust\.cargo\bin;$env:PATH"
# crates.io via sparse index; HTTP/2 multiplexing breaks under schannel here
$env:CARGO_REGISTRIES_CRATES_IO_PROTOCOL = 'sparse'
$env:CARGO_HTTP_MULTIPLEXING = 'false'
Set-Location $PSScriptRoot
& cargo @Args
exit $LASTEXITCODE
