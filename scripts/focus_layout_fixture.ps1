param(
  [Parameter(Mandatory=$true)][string]$Filter,
  [switch]$ClearCache
)

$env:LAYOUT_FIXTURE_FILTER = $Filter
$env:RUST_LOG = "layouter=debug,layouter::visual_formatting::vertical=debug"

if ($ClearCache) {
  $cacheDir = Join-Path -Path (Resolve-Path ".").Path -ChildPath "target/valor_layout_cache"
  if (Test-Path $cacheDir) {
    Write-Host "[focus] Clearing cache dir: $cacheDir"
    Remove-Item -Recurse -Force $cacheDir
  }
}

cargo test -p valor --test layouter_chromium_compare -- --nocapture
