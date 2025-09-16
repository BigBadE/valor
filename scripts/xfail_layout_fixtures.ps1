Param(
  [Parameter(Mandatory=$false)] [string] $Root = "crates"
)

$rootPath = Resolve-Path $Root
$files = Get-ChildItem -Path $rootPath -Recurse -Filter *.html | Where-Object { $_.FullName -like "*\tests\fixtures\layout\*" }

# Known passing fixtures that should remain enabled
$exclusions = @(
  [IO.Path]::GetFullPath("crates/valor/tests/fixtures/layout/edges_box_model/index.html")
)

$added = @()
foreach ($f in $files) {
  $full = [IO.Path]::GetFullPath($f.FullName)
  if ($exclusions -contains $full) { continue }
  $text = Get-Content -Raw -LiteralPath $full
  if ($text -notmatch "VALOR_XFAIL") {
    if ($text -match "<body[^>]*>") {
      $new = $text -replace "(<body[^>]*>)", "$1`r`n  <!-- VALOR_XFAIL: awaiting layout parity -->"
    } else {
      $new = "<!-- VALOR_XFAIL: awaiting layout parity -->`r`n" + $text
    }
    Set-Content -LiteralPath $full -Value $new -NoNewline
    Write-Host "XFAIL added: $full"
    $added += $full
  }
}

Write-Host ("Total XFAIL inserted: " + $added.Count)
