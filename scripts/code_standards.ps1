#!/usr/bin/env pwsh

param(
    [string]
    $LogSpec = ''
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# Minimal wrapper: delegate entirely to bash script and propagate exit code.
if ([string]::IsNullOrWhiteSpace($LogSpec)) {
    & bash -lc "./scripts/code_standards.sh"
    exit $LASTEXITCODE
} else {
    & bash -lc "./scripts/code_standards.sh '$LogSpec'"
    exit $LASTEXITCODE
}
