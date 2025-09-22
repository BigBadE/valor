#!/usr/bin/env pwsh
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

try {
    Write-Host '[code_standards] Running cargo fmt' -ForegroundColor Cyan
    cargo fmt
    if ($LASTEXITCODE -ne 0) { throw "cargo fmt failed with exit code $LASTEXITCODE" }

    Write-Host '[code_standards] Running cargo clippy' -ForegroundColor Cyan
    cargo clippy --all-targets --all-features -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw "cargo clippy failed with exit code $LASTEXITCODE" }

    function Invoke-CargoTestWithRetry([string[]] $ArgsArr) {
        Write-Host ("[code_standards] cargo test " + ($ArgsArr -join ' ')) -ForegroundColor Cyan
        cargo test @ArgsArr
        if ($LASTEXITCODE -ne 0) {
            Write-Host '[code_standards] Test failed — attempting cargo clean and one retry (possible ICE)' -ForegroundColor Yellow
            cargo clean
            cargo test @ArgsArr
            if ($LASTEXITCODE -ne 0) {
                throw ("cargo test " + ($ArgsArr -join ' ') + " failed with exit code $LASTEXITCODE")
            }
        }
    }

    Write-Host '[code_standards] Running layouting test first for diagnostics...' -ForegroundColor Cyan
    Invoke-CargoTestWithRetry @('-p','valor','--test','layouter_chromium_compare','--','--nocapture')

    Write-Host '[code_standards] Running full cargo test suite...' -ForegroundColor Cyan
    Invoke-CargoTestWithRetry @('--all','--all-features')

    Write-Host '[code_standards] OK' -ForegroundColor Green
}
catch {
    Write-Host "[code_standards] FAILED: $($_.Exception.Message)" -ForegroundColor Red
    exit 1
}
