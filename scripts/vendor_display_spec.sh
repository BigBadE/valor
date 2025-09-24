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

# Require pandoc for high-quality HTML->Markdown conversion
PANDOC_BIN="${PANDOC:-}"
if [[ -z "${PANDOC_BIN}" ]]; then
  if command -v pandoc >/dev/null 2>&1; then
    PANDOC_BIN="pandoc"
  else
    # Try common Windows installation paths when running under Git Bash/WSL
    for p in \
      "/c/Program Files/Pandoc/pandoc.exe" \
      "/c/Program Files (x86)/Pandoc/pandoc.exe" \
      "/mnt/c/Program Files/Pandoc/pandoc.exe" \
      "/mnt/c/Program Files (x86)/Pandoc/pandoc.exe" \
      "/c/ProgramData/chocolatey/bin/pandoc.exe" \
      "/mnt/c/ProgramData/chocolatey/bin/pandoc.exe" \
      "/c/Users/${USERNAME}/AppData/Local/Pandoc/pandoc.exe" \
      "/mnt/c/Users/${USERNAME}/AppData/Local/Pandoc/pandoc.exe"; do
      if [[ -x "$p" ]]; then PANDOC_BIN="$p"; break; fi
    done
  fi
fi
if [[ -z "${PANDOC_BIN}" ]]; then
  echo "[vendor_display_spec] pandoc not found. Please install pandoc to continue." >&2
  exit 1
fi

# Extract <body> and remove scripts/styles
body="$(printf '%s' "${html}" | awk 'BEGIN{IGNORECASE=1} /<body/{p=1} p{print} /<\/body>/{exit}')"
body="$(printf '%s' "${body}" | sed -E 's/<script[\s\S]*?<\/script>//Ig')"
body="$(printf '%s' "${body}" | sed -E 's/<style[\s\S]*?<\/style>//Ig')"

# Slice from Chapter 2 (first H2 starting with "2" or "2.") to before Acknowledgements
begin_idx=$(perl -0777 -ne 'if (/<h2[^>]*>(\s*2[\.|\s][^<]*)<\/h2>/i) { print $-[0]; exit }' <<<"${body}" || true)
ack_idx=$(perl -0777 -ne 'if (/<h2[^>]*>\s*Acknowledg/i) { print $-[0]; exit }' <<<"${body}" || true)

if [[ -n "${begin_idx}" ]]; then
  body_slice="$(perl -0777 -pe 'BEGIN{ $b = $ENV{BEGIN_IDX}; $a = $ENV{ACK_IDX}; } END{}' 2>/dev/null <<<"")"
  export BEGIN_IDX="${begin_idx}"
  export ACK_IDX="${ack_idx}"
  body="$(perl -0777 -e '$/=undef; $s=<>; $b=$ENV{BEGIN_IDX}; $a=$ENV{ACK_IDX}; if($b eq q{}){print $s; exit}; if($a eq q{}){$a=length($s);} print substr($s,$b,$a-$b);' <<<"${body}")"
fi

# High-quality conversion to Markdown via pandoc
body_md="$(printf '%s' "${body}" | "${PANDOC_BIN}" -f html -t gfm --wrap=none)"

# Post-process tables: merge adjacent headers and drop stray '|' lines inside tables
:


# Reformat property definition blocks into Markdown tables
body_md="$(awk '
  # Globals used: g_key, g_rest
  function norm_key(line,    m,label,canon) {
    g_key=""; g_rest=""
    # Match plain, bold (**), or bracketed/linked keys, anywhere on the line
    if (match(line, /(\*\*)?\s*(\[)?(Name|Value|Initial|Applies to|Inherited|Percentages|Computed value|Canonical order|Animation type|Media)(:)?(\])?(\*\*)?/, m)) {
      label = m[3]
      canon = label ":"
      g_key = canon
      # Capture any remainder after the matched token on the same line as inline value
      rem = substr(line, RSTART + RLENGTH)
      # Trim leading punctuation/space
      gsub(/^\s+/, "", rem)
      g_rest = rem
      return canon
    }
    return ""
  }
  function flush_table(){
    if (in_prop && have_any) {
      # sanitize values
      for (k in props) {
        gsub(/\r?\n+/, "<br>", props[k])
        gsub(/\|/, "\\|", props[k])
      }
      print ""
      print "| Field | Value |"
      print "|---|---|"
      if (props["Name:"] != "")        print "| Name | " props["Name:"] " |"
      if (props["Value:"] != "")       print "| Value | " props["Value:"] " |"
      if (props["Initial:"] != "")     print "| Initial | " props["Initial:"] " |"
      if (props["Applies to:"] != "")  print "| Applies to | " props["Applies to:"] " |"
      if (props["Inherited:"] != "")   print "| Inherited | " props["Inherited:"] " |"
      if (props["Percentages:"] != "") print "| Percentages | " props["Percentages:"] " |"
      if (props["Computed value:"]!="")print "| Computed value | " props["Computed value:"] " |"
      if (props["Canonical order:"]!="")print "| Canonical order | " props["Canonical order:"] " |"
      if (props["Animation type:"]!="")print "| Animation type | " props["Animation type:"] " |"
      if (props["Media:"] != "")       print "| Media | " props["Media:"] " |"
      print ""
    }
    # reset
    delete props
    in_prop=0; expecting=0; last_key=""; have_any=0; seen_any_key=0
  }
  BEGIN{
    in_prop=0; expecting=0; last_key=""; have_any=0; seen_any_key=0
  }
  {
    line=$0
    # If a new section heading starts, flush any pending table first
    if (line ~ /^## +/ || line ~ /^### +/) {
      flush_table()
      print line
      next
    }
    # Known property keys (plain or bracketed/linked)
    k = norm_key(line)
    if (k != "") {
      in_prop=1
      last_key=k
      if (g_rest != "") {
        props[last_key] = g_rest
        have_any=1
        expecting=0
      } else {
        expecting=1
      }
      seen_any_key=1
      next
    }
    if (in_prop) {
      # If we just saw a key, capture the first meaningful line (skip blanks and stray table pipes)
      if (expecting) {
        if (line ~ /^\s*$/) { next }
        if (line ~ /^\s*\|\s*$/) { next }
        props[last_key]=line
        have_any=1
        expecting=0
        next
      }
      # While inside a property block and not expecting a new value,
      # accumulate continuation lines until next key or heading.
      k2 = norm_key(line)
      if (k2 != "") {
        # New key begins; move on and handle in next iteration
        in_prop=1
        last_key=k2
        if (g_rest != "") {
          props[last_key] = g_rest
          have_any=1
          expecting=0
        } else {
          expecting=1
        }
        next
      }
      if (line ~ /^## +/ || line ~ /^### +/) {
        # Reached a heading -> end of property block
        flush_table()
        print line
        next
      }
      # Otherwise, append the line to the current key value (including blanks)
      if (last_key != "") {
        if (line ~ /^\s*\|\s*$/) {
          next
        } else if (line ~ /^\s*$/) {
          props[last_key] = props[last_key] "\n"
        } else {
          props[last_key] = props[last_key] "\n" line
        }
      }
      next
    }
    # Default: passthrough
    print line
  }
' <<<"${body_md}")"

# Targeted fix: attach a trailing 'by computed value type' line to preceding Animation type row
body_md="$(awk '
  BEGIN{ have_anim=0; saved_row="" }
  {
    line=$0
    if (!have_anim) {
      if (match(line, /^\|[ ]*Animation type[ ]*\|([^|]*)\|[ ]*$/, m)) {
        saved_row=line
        have_anim=1
        next
      }
      print line
      next
    }
    # have_anim == 1
    t=line; gsub(/^ *| *$/, "", t)
    if (t == "") { next }
    if (t == "by computed value type") {
      # inject into the saved row before the closing |
      row=saved_row
      sub(/\|[ ]*$/, "<br>by computed value type |", row)
      print row
      have_anim=0; saved_row=""
      next
    }
    # different content; emit saved row and current line
    print saved_row
    print line
    have_anim=0; saved_row=""
  }
  END{ if(have_anim) print saved_row }
' <<<"${body_md}")"
# Post-process: merge narrative continuation lines into the preceding table row value
body_md="$(awk '
  BEGIN{ in_table=0; have_row=0; field=""; value=""; row_prefix="| " }
  function flush(){ if(have_row){ print row_prefix field " | " value " |"; have_row=0; field=""; value="" } }
  {
    line=$0
    if (!in_table) {
      if (line ~ /^\| Field \| Value \|$/) { print line; in_table=1; next }
      print line; next
    }
    # in_table
    if (line ~ /^\|---\|---\|$/) { print line; next }
    if (match(line, /^\|([^|]+)\|([^|]*)\|\s*$/, m)) {
      flush()
      field=m[1]; gsub(/^ *| *$/, "", field)
      value=m[2]; gsub(/^ *| *$/, "", value)
      have_row=1
      next
    }
    if (line ~ /^\|/) { flush(); print line; next }
    if (line ~ /^## +/ || line ~ /^### +/ || line ~ /^```/) { flush(); in_table=0; print line; next }
    # Non-table content; accumulate only for known short continuations
    if (have_row) {
      t=line; gsub(/^ *| *$/, "", t)
      if (t == "") { next }
      if (t ~ /^(specified integer|per grammar|by computed value type)$/) {
        add=t; gsub(/\|/, "\\|", add)
        if (value == "") value=add; else value=value "<br>" add
        next
      }
      # otherwise end the table and emit this line
      flush(); in_table=0; print line; next
    }
    # No current row: end table and print
    in_table=0; print line
  }
  END{ flush() }
' <<<"${body_md}")"

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

# Build new file by replacing content between markers
awk -v sm="$start_marker" -v em="$end_marker" -v payload="${body_md//\/\\}" '
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
