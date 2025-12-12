#!/usr/bin/env bash

set -euo pipefail

# Usage:
#   ./scripts/verify.sh [--cov] [--ui-only]
#
# Flags:
#   --cov       Run coverage testing (delegates to coverage.sh)
#   --ui-only   Run only UI tests (skip Rust)
#
# Environment variables:
#   RENDERER_CI   Set to skip clean step in CI environments

check_file_sizes() {
  local max_lines=500
  local violations=$(
          find crates -type f -name '*.rs' -print0 |
          xargs -0 awk -v max="$max_lines" '
            { count[FILENAME]++ }
            ENDFILE { if (count[FILENAME] > max) print FILENAME ": " count[FILENAME] " lines" }
          '
        )

  if [ -n "$violations" ]; then
    echo "ERROR: The following files exceed $max_lines lines:"
    echo "$violations"
    return 1
  fi

  return 0
}

check_allows() {
  # This project does not allow ANY #[allow] or #[expect] annotations
  # All warnings must be fixed, not silenced

  # Find Rust files that have allow, expect, or cfg_attr with these
  mapfile -t files < <(
    find crates src -type f -name '*.rs' -print0 2>/dev/null |
    xargs -0 grep -lE '#!?\[(allow|expect|cfg_attr.*(allow|expect))' 2>&1 | grep -v "No such file" || true
  )

  local violations=false

  for file in "${files[@]}"; do
    # Check for ANY allow or expect annotations
    result=$(awk '
      BEGIN { in_attr = 0; attr = "" }

      {
        # If currently collecting an attribute, continue
        if (in_attr) {
          attr = attr $0
          if (index($0, "]")) {
            if (attr ~ /(allow|expect)/) {
              print "DISALLOWED:::" FILENAME ":::" attr
            }
            in_attr = 0
            attr = ""
          }
          next
        }

        # Detect start of an attribute with allow or expect
        if ($0 ~ /#(!)?\[.*(allow|expect)/) {
          attr = $0
          if (index($0, ")]")) {
            if (attr ~ /(allow|expect)/) {
              print "DISALLOWED:::" FILENAME ":::" attr
            }
            attr = ""
          } else {
            in_attr = 1
          }
        }
      }

      END {
        if (in_attr && attr != "" && attr ~ /(allow|expect)/) {
          print "DISALLOWED:::" FILENAME ":::" attr
        }
      }
    ' "$file")

    if [ -n "$result" ]; then
      echo "ERROR: Found disallowed #[allow] or #[expect] annotation:"
      echo "$result" | awk -F':::' '{print "  File: " $2; print "  Annotation: " $3}'
      violations=true
    fi
  done

  if $violations; then
    echo ""
    echo "NO #[allow] or #[expect] annotations are permitted in this project."
    echo "All warnings must be fixed, not silenced."
    echo "Fix the code instead of using #[allow] or #[expect]."
    return 1
  fi

  return 0
}

RUN_COVERAGE=false

for arg in "$@"; do
  case "$arg" in
    --cov)
      RUN_COVERAGE=true
      ;;
    *)
      # ignore unknown args (forward compatibility)
      ;;
  esac
done

echo "================================================"
echo "Renderer Project Verification"
echo "================================================"

# Check file sizes
echo "[verify] Checking file sizes..."
check_file_sizes

# Check for disallowed allow/expect annotations
echo "[verify] Checking for disallowed #[allow] and #[expect] annotations..."
if ! check_allows; then
  echo "FATAL: check_allows failed" >&2
  exit 1
fi

# Format and lint Rust code
echo "[verify] Formatting Rust code..."
cargo fmt --all

echo "[verify] Running Clippy..."
cargo clippy --all-targets --workspace -- -D warnings

echo "[verify] Building Rust workspace..."
cargo build --workspace

# Delegate to coverage.sh if --cov is passed
if [ "$RUN_COVERAGE" = true ]; then
  exec "$(dirname "${BASH_SOURCE[0]}")/coverage.sh"
fi

# Run full workspace tests
echo "[verify] Running Rust tests..."
cargo nextest run --workspace

# Clean old artifacts if not in CI
if [ -z "${RENDERER_CI:-}" ]; then
  if command -v cargo-sweep &> /dev/null; then
    echo "[verify] Cleaning old build artifacts..."
    cargo sweep --time 7
  fi
fi

echo ""
echo "================================================"
echo "✅ All verification checks passed!"
echo "================================================"
