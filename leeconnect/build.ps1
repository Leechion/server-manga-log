# LeeConnect build wrapper.
# 1) clears the stale CC/CXX env (w64devkit gcc) that hijacks cc-rs for ring/openh264-sys2 on MSVC
# 2) auto-detects a working Rust toolchain (cargo home + rustup home pair) so it runs on any machine
# 3) forces the sparse crates.io index (HTTP/2 multiplexing breaks under schannel on Windows)
# Usage: .\build.ps1 [cargo args...]   e.g. .\build.ps1 test --lib
param([Parameter(ValueFromRemainingArguments = $true)][string[]]$Args = @('build'))
$ErrorActionPreference = 'Continue'
Remove-Item Env:CC, Env:CXX -ErrorAction SilentlyContinue

$env:CARGO_REGISTRIES_CRATES_IO_PROTOCOL = 'sparse'
$env:CARGO_HTTP_MULTIPLEXING = 'false'

# Candidate (CARGO_HOME, RUSTUP_HOME) pairs, tried in order. First that answers `cargo --version` wins.
$candidates = @(
    @{ cargo = 'E:\rust\cargo';  rustup = 'E:\rust\rustup' }   # dev machine A layout
    @{ cargo = 'E:\rust\.cargo'; rustup = 'E:\rust\.rustup' }  # dev machine B layout
)

function Test-RustHome($cargo, $rustup) {
    $pc = $env:CARGO_HOME; $pr = $env:RUSTUP_HOME; $pp = $env:PATH
    try {
        $env:CARGO_HOME = $cargo; $env:RUSTUP_HOME = $rustup
        $env:PATH = "$cargo\bin;$pp"
        $v = & cargo --version 2>&1
        return ($LASTEXITCODE -eq 0 -and "$v" -match 'cargo')
    } catch { return $false }
    finally { $env:CARGO_HOME = $pc; $env:RUSTUP_HOME = $pr; $env:PATH = $pp }
}

$chosen = $candidates | Where-Object { Test-RustHome $_.cargo $_.rustup } | Select-Object -First 1
if ($chosen) {
    $env:CARGO_HOME = $chosen.cargo; $env:RUSTUP_HOME = $chosen.rustup
    $env:PATH = "$($chosen.cargo)\bin;$env:PATH"
    Write-Host "[build] rust home: $($chosen.cargo) | rustup: $($chosen.rustup)"
} else {
    Write-Host '[build] no known E:\rust home validated; falling back to cargo on PATH'
}

Set-Location $PSScriptRoot
& cargo @Args
exit $LASTEXITCODE
