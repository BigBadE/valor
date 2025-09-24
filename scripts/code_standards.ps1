#!/usr/bin/env pwsh

param(
    [string]
    $LogSpec = ''
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

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

    # Delegate to bash script, forwarding optional log spec argument.
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
