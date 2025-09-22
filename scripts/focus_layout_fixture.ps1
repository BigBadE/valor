param(
  [Parameter(Mandatory=$true)][string]$Filter
)

$env:LAYOUT_FIXTURE_FILTER = $Filter
$env:RUST_LOG = "layouter=debug,layouter::visual_formatting::vertical=debug"

cargo test -p valor --test layouter_chromium_compare -- --nocapture
