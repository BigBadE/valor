#!/usr/bin/env bash
set -euo pipefail

SPEC_URL=${1:-"https://www.w3.org/TR/css-display-3/"}
MODULE_SPEC_PATH=${2:-"crates/css/modules/display/spec.md"}
YEAR=${3:-"2025"}

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
spec_file="${root_dir}/${MODULE_SPEC_PATH}"

if [[ ! -f "${spec_file}" ]]; then
  echo "[vendor_display_spec] Spec file not found: ${spec_file}" >&2
  exit 1
fi

# Fetch spec HTML
if command -v curl >/dev/null 2>&1; then
  html="$(curl -fsSL "${SPEC_URL}")"
elif command -v wget >/dev/null 2>&1; then
  html="$(wget -q -O - "${SPEC_URL}")"
else
  echo "[vendor_display_spec] Need curl or wget" >&2
  exit 1
fi

# Note: pandoc is not required. We embed raw HTML inside spec.md; Markdown passes it through.

# Extract <body> and remove scripts/styles
body="$(printf '%s' "${html}" | awk 'BEGIN{IGNORECASE=1} /<body/{p=1} p{print} /<\/body>/{exit}')"
body="$(printf '%s' "${body}" | sed -E 's/<script[\s\S]*?<\/script>//Ig')"
body="$(printf '%s' "${body}" | sed -E 's/<style[\s\S]*?<\/style>//Ig')"

# Slice from the first numbered chapter >= 2 to the first non-numbered H2 (structural-only)
begin_idx=$(perl -0777 -e '
  undef $/; $s=<>; my $fallback=-1;
  while ($s =~ m{<h2[^>]*>(.*?)</h2>}sig) {
    my $pos = $-[0];
    my $h = $1;
    if ($h =~ m{<span[^>]*class="[^"]*secno[^"]*"[^>]*>\s*([0-9]+)\.}i) {
      my $n = $1; if ($n >= 2) { print $pos; exit }
    }
    if ($fallback < 0 && $h =~ m{^\s*([0-9]+)\.[^<]}s) { my $n=$1; if ($n>=2){ $fallback=$pos } }
  }
  if ($fallback >= 0) { print $fallback }
' <<<"${body}" || true)
# Export BEGIN_IDX early for subsequent perl
export BEGIN_IDX="${begin_idx}"

ack_idx=$(perl -0777 -e '
  undef $/; $s=<>; my $b=$ENV{BEGIN_IDX};
  if ($b eq q{}) { exit 0 }
  $s = substr($s, $b);
  # Find the first H2 that appears to be Acknowledgments (by text or id)
  while ($s =~ m{<h2[^>]*>(.*?)</h2>}sig) {
    my $pos = $-[0] + $b;
    my $h = $1;
    if ($h =~ m{acknowledg}i) { print $pos; exit }
  }
' <<<"${body}" || true)

if [[ -n "${begin_idx}" ]]; then
  body_slice="$(perl -0777 -pe 'BEGIN{ $b = $ENV{BEGIN_IDX}; $a = $ENV{ACK_IDX}; } END{}' 2>/dev/null <<<"")"
  export ACK_IDX="${ack_idx}"
  body="$(perl -0777 -e '
    $/=undef; $s=<>; $b=$ENV{BEGIN_IDX}; $a=$ENV{ACK_IDX};
    if($b eq q{}){print $s; exit};
    if($a eq q{}){$a=length($s);} 
    if($a < $b){ $a = length($s); }
    print substr($s,$b,$a-$b);
  ' <<<"${body}")"
fi

# Fallback if slicing failed and produced nearly empty content
if [[ ${#body} -lt 2000 ]]; then
  echo "[vendor_display_spec] Slice produced too little content; using full spec body." >&2
  body="$(printf '%s' "${html}" | awk 'BEGIN{IGNORECASE=1} /<body/{p=1} p{print} /<\/body>/{exit}')"
fi

# Remove Glossary appendix (skip only Glossary; keep other appendices). We drop from the Glossary H2 to the next H2.
body="$(perl -0777 -e '
  undef $/; $s=<>;
  my $pos = 0; my $out = ""; my $changed = 0;
  while ($s =~ m{<h2[^>]*>(.*?)</h2>}sig) {
    my $h_start = $-[0];
    my $h_end   = $+[0];
    my $h = $1;
    if ($h =~ /glossary/i) {
      # find next H2 after this one
      my $rest = substr($s, $h_end);
      if ($rest =~ m{<h2[^>]*>}is) {
        my $next_h2_start = $-[0] + $h_end;
        $out .= substr($s, $pos, $h_start - $pos);
        $pos = $next_h2_start;
        $changed = 1;
        next;
      } else {
        # Glossary is the last H2; drop to end
        $out .= substr($s, $pos, $h_start - $pos);
        $pos = length($s);
        $changed = 1;
        last;
      }
    }
  }
  if ($changed) {
    print $out; if ($pos < length($s)) { print substr($s, $pos) }
  } else { print $s }
' <<<"${body}")"

# Wrap per-section spec content in <details> for H2/H3 sections, preserving the heading and optional status block
# Rules:
#  - For each <h2> or <h3>, keep the heading as-is.
#  - If the immediate following content starts with a <div data-valor-status="...">...</div>, keep that block outside.
#  - Wrap the remaining content until the next heading of same-or-higher level (<h2> for h2; <h2|h3> for h3) in a
#    <details class="valor-spec" data-level="2|3"> with a <summary>Show spec text</summary>.
#  - Do not nest or double-wrap.
payload_html="$(perl -0777 -e '
  undef $/; my $s = <>; my $out = ""; my $pos = 0;
  # Helper to find matching closing tag position for a simple <div> ... </div> immediately at start
  sub consume_status_div {
    my ($str) = @_; # expects $str to start at the beginning of the div
    if ($str =~ m{^\s*<div[^>]*data-valor-status[^>]*> }isx) {
      my $start = $-[0];
      # naive stack for nested divs inside status block
      my $i = $start; my $depth = 0; my $len = length($str);
      while ($i < $len) {
        if (substr($str,$i) =~ m{\G\s*<div\b}sigc) { $depth++; next }
        if (substr($str,$i) =~ m{\G\s*</div\s*>}sigc) { $depth--; if ($depth<=0) { return pos($str) } next }
        # advance by one char if no tag matched to avoid infinite loop
        $i++;
      }
    }
    return undef;
  }

  while ($s =~ m{<(h[23])\b([^>]*)>(.*?)</\1>}sig) {
    my $h_start = $-[0];
    my $h_end   = $+[0];
    my $h_tag   = $1; # h2 or h3
    my $h_attrs = $2;
    # Emit content before heading unchanged
    $out .= substr($s, $pos, $h_end - $pos); # include the heading itself
    my $scan_from = $h_end;
    # Determine the next boundary depending on level
    # H2 sections should exclude H3 subsections from their collapse region.
    # Therefore, H2 wraps until the next H2 or H3; H3 wraps until next H2 or H3 as usual.
    my $re_next = ($h_tag eq "h2") ? qr{<(?:h2|h3)\b}i : qr{<(?:h2|h3)\b}i;

    # Find next heading of same/higher level
    my $rest = substr($s, $scan_from);
    my $next_idx;
    if ($rest =~ $re_next) { $next_idx = $-[0] + $scan_from; } else { $next_idx = length($s); }

    # Extract the section body following the heading
    my $section = substr($s, $scan_from, $next_idx - $scan_from);

    # Peel off an optional status div at the start of the section
    my $status_len;
    if ($section =~ m{^\s*<div[^>]*data-valor-status}i) {
      # Use a balanced div matching routine (approximate)
      # This approximation assumes reasonably well-formed status block without stray </div>
      if ($section =~ m{^\s*(<div[^>]*data-valor-status[\s\S]*?</div>)\s*}i) {
        $status_len = length($1) + ($section =~ m{^\s*} ? length($&) : 0);
        $out .= substr($section, 0, $status_len);
        $section = substr($section, $status_len);
      }
    }

    # Inject a generated status block from mapping if available (based on heading id)
    my $id = undef;
    if ($h_attrs =~ /\bid\s*=\s*"([^"]+)"/i) { $id = $1 }
    if ($id) {
      my %blocks = ();
    }
    # The bash layer will substitute a token for known ids. We mark with a placeholder here.
    my $status_placeholder = ($id) ? "<!--__VALOR_STATUS:".$id."__-->" : "";
    if (length($status_placeholder)) { $out .= "\n".$status_placeholder."\n"; }

    # Trim leading whitespace before wrapping
    $section =~ s/^\s+//;
    $section =~ s/\s+$//;

    my $lvl = ($h_tag eq "h2") ? 2 : 3;
    $out .= "\n<details class=\"valor-spec\" data-level=\"$lvl\">\n  <summary>Show spec text</summary>\n\n" . $section . "\n\n</details>\n";

    $pos = $next_idx;
    pos($s) = $next_idx; # continue scanning from next heading
  }
  # Append any trailing content after the last processed section
  if ($pos < length($s)) { $out .= substr($s, $pos) }
  print $out;
' <<<"${body}")"

# Replace status placeholders with actual blocks from the parsed mapping
for id in "${!STATUS_BLOCKS[@]}"; do
  ph="<!--__VALOR_STATUS:${id}__-->"
  payload_html="${payload_html//${ph}/${STATUS_BLOCKS[$id]}}"
done

# Note: Per-chapter generation and placeholder/status injection have been removed.
# This script now only slices the spec and embeds raw HTML verbatim into spec.md.

# Replace content between markers
start_marker_ps1='<!-- BEGIN VERBATIM SPEC: DO NOT EDIT BELOW. This block is auto-generated by scripts/vendor_display_spec.ps1 -->'
start_marker_sh='<!-- BEGIN VERBATIM SPEC: DO NOT EDIT BELOW. This block is auto-generated by scripts/vendor_display_spec.sh -->'
end_marker='<!-- END VERBATIM SPEC: DO NOT EDIT ABOVE. -->'

# Determine which start marker is present (ps1 preferred)
start_marker="${start_marker_ps1}"
if ! awk -v m="$start_marker" 'index($0,m){found=1} END{exit(found?0:1)}' "${spec_file}"; then
  start_marker="${start_marker_sh}"
fi

# Validate presence of markers
if ! awk -v m="$start_marker" 'index($0,m){found=1} END{exit(found?0:1)}' "${spec_file}" || \
   ! awk -v m="$end_marker" 'index($0,m){found=1} END{exit(found?0:1)}' "${spec_file}"; then
  echo "[vendor_display_spec] Markers not found in ${MODULE_SPEC_PATH}" >&2
  exit 1
fi

# Build new file by replacing content between markers (embed HTML as-is)
awk -v sm="$start_marker" -v em="$end_marker" -v payload="${payload_html//\/\\}" '
  BEGIN{in_block=0}
  {
    if(!in_block){
      print
    }
    if(index($0,sm)){
      in_block=1
      print ""
      print payload
    }
    if(index($0,em)){
      in_block=0
      print
    }
  }
' "${spec_file}" > "${spec_file}.tmp"

mv "${spec_file}.tmp" "${spec_file}"

echo "[vendor_display_spec] Updated ${MODULE_SPEC_PATH} with verbatim spec content from ${SPEC_URL}."
