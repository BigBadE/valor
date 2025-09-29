#!/usr/bin/env pwsh

param(
    [string]
    $LogSpec = ''
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Invoke-BashStandards {
    param([string]$LogSpec)
    if ([string]::IsNullOrWhiteSpace($LogSpec)) {
        $localOutput = & bash -lc "./scripts/code_standards.sh" 2>&1 | Tee-Object -Variable LastRunOutput
    } else {
        $localOutput = & bash -lc "./scripts/code_standards.sh '$LogSpec'" 2>&1 | Tee-Object -Variable LastRunOutput
    }
    $script:LastRunOutput = $LastRunOutput
    return $LASTEXITCODE
}

function Clear-IncrementalState {
    param()
    Write-Host "[code_standards] Detected ICE. Performing targeted cache cleanup..." -ForegroundColor Yellow
    $root = Split-Path -Parent $MyInvocation.MyCommand.Path | Split-Path -Parent
    # Delete rustc ICE logs to avoid clutter
    Get-ChildItem -LiteralPath $root -Filter 'rustc-ice-*.txt' -File -ErrorAction SilentlyContinue | ForEach-Object {
        Write-Host "[code_standards] Deleting ICE log: $($_.FullName)" -ForegroundColor DarkYellow
        Remove-Item -LiteralPath $_.FullName -Force -ErrorAction SilentlyContinue
    }
    $targets = @(
        Join-Path $root 'target/debug/incremental',
        Join-Path $root 'target/debug/.fingerprint',
        Join-Path $root 'target/debug/build'
    )
    foreach ($p in $targets) {
        if (Test-Path -LiteralPath $p) {
            Write-Host "[code_standards] Removing: $p" -ForegroundColor DarkYellow
            Remove-Item -LiteralPath $p -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
}

try {
    # Normalize line endings in all shell scripts to avoid /usr/bin/env 'bash\r' errors
    $scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
    $shFiles = Get-ChildItem -Path (Join-Path $scriptDir '*.sh') -File -ErrorAction SilentlyContinue
    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    foreach ($f in $shFiles) {
        $text = Get-Content -LiteralPath $f.FullName -Raw -ErrorAction SilentlyContinue
        if ($null -ne $text) {
            $lf = $text -replace "`r", ""
            [System.IO.File]::WriteAllText($f.FullName, $lf, $utf8NoBom)
        }
    }

    # First attempt
    $code = Invoke-BashStandards -LogSpec $LogSpec
    if ($code -eq 0) { exit 0 }

    # Detect ICE heuristically from output or presence of rustc ICE log and retry after cleanup
    $root = Split-Path -Parent $MyInvocation.MyCommand.Path | Split-Path -Parent
    $hasIceLog = (Get-ChildItem -LiteralPath $root -Filter 'rustc-ice-*.txt' -File -ErrorAction SilentlyContinue | Measure-Object).Count -gt 0
    $outputText = ($script:LastRunOutput | Out-String)
    $hasIceText = $outputText -match 'compiler unexpectedly panicked' -or $outputText -match '\bICE\b' -or $outputText -match 'rustc-ice-'
    if ($hasIceLog -or $hasIceText) {
        Clear-IncrementalState
        $code = Invoke-BashStandards -LogSpec $LogSpec
    }
    exit $code
}
catch {
    Write-Host "[code_standards] FAILED to invoke bash: $($_.Exception.Message)" -ForegroundColor Red
    exit 1
}
