param(
    [string]$SpecUrl = "https://www.w3.org/TR/css-display-3/",
    [string]$ModuleSpecPath = "crates/css/modules/display/spec.md",
    [string]$Year = "2025"
)

$ErrorActionPreference = "Stop"

$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$root = Resolve-Path (Join-Path $here "..")
$sh = Join-Path $root "scripts/vendor_display_spec.sh"

if (-not (Test-Path $sh)) {
    throw "Bash vendor script not found: $sh"
}

function Find-Bash {
    $gitBash = "C:\\Program Files\\Git\\bin\\bash.exe"
    if (Test-Path $gitBash) { return $gitBash }
    if (Get-Command bash -ErrorAction SilentlyContinue) { return "bash" }
    $wsl = "wsl.exe"
    if (Get-Command $wsl -ErrorAction SilentlyContinue) { return $wsl }
    return $null
}

$bashExe = Find-Bash
if (-not $bashExe) {
    throw "bash not found. Please install Git Bash or WSL to run the vendor script."
}

# Propagate pandoc path to bash via PANDOC env var if available
$pandocCmd = Get-Command pandoc -ErrorAction SilentlyContinue
if ($pandocCmd) {
    $env:PANDOC = $pandocCmd.Source
} else {
    try {
        $pandocWhere = & where.exe pandoc 2>$null | Select-Object -First 1
        if ($pandocWhere) { $env:PANDOC = $pandocWhere }
    } catch {}
}

if ($bashExe -like "*wsl.exe") {
    $pandocArg = ''
    if ($env:PANDOC) { $pandocArg = "export PANDOC='" + ($env:PANDOC) + "'; " }
    & $bashExe bash -lc ("${pandocArg}'${sh}' '${SpecUrl}' '${ModuleSpecPath}' '${Year}'")
} else {
    & $bashExe "${sh}" "$SpecUrl" "$ModuleSpecPath" "$Year"
}

Write-Host "[vendor_display_spec] Completed via bash script."
