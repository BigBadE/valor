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

    Write-Host '[code_standards] Running cargo test...' -ForegroundColor Cyan
    cargo test --all --all-features
    if ($LASTEXITCODE -ne 0) { throw "cargo test failed with exit code $LASTEXITCODE" }

    Write-Host '[code_standards] OK' -ForegroundColor Green
}
catch {
    Write-Host "[code_standards] FAILED: $($_.Exception.Message)" -ForegroundColor Red
    exit 1
}
