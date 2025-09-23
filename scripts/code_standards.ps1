#!/usr/bin/env pwsh

param(
    [string]
    $LogSpec = ''
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

try {
    # Thin wrapper: delegate to bash script, forwarding optional log spec argument.
    if ([string]::IsNullOrWhiteSpace($LogSpec)) {
        & bash -lc "./scripts/code_standards.sh"
    } else {
        # Quote the argument for bash invocation
        & bash -lc "./scripts/code_standards.sh '$LogSpec'"
    }
    exit $LASTEXITCODE
}
catch {
    Write-Host "[code_standards] FAILED to invoke bash: $($_.Exception.Message)" -ForegroundColor Red
    exit 1
}
