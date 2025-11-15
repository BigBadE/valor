#!/usr/bin/env bash

set -euo pipefail

# Usage:
#   ./scripts/coverage.sh
#
# Environment variables:
#   RENDERER_CI   Set to skip clean step in CI environments
#
# This script runs coverage instrumentation and generates a coverage report
# for both Rust and UI code. Run verify.sh first to ensure code quality.

# Respect CARGO_TARGET_DIR if set externally, otherwise use default
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")"/.. && pwd)"
ROOT_DIR="${SCRIPT_DIR}"
TARGET_DIR="${CARGO_TARGET_DIR:-${ROOT_DIR}/target}"
LLVM_COV_DIR="${TARGET_DIR}/llvm-cov-target"
CARGO_PROFILE="${CARGO_PROFILE:-dev}"

echo "================================================"
echo "Renderer Coverage Report Generation"
echo "================================================"

# Clean any existing prof files and temp directories for a clean build
rm -rf coverage 2>/dev/null || true
rm -f "${LLVM_COV_DIR}"/*.prof* 2>/dev/null || true
rm -f "${LLVM_COV_DIR}"/*-profraw-list 2>/dev/null || true
rm -rf "${LLVM_COV_DIR}"/temp_lcov_* 2>/dev/null || true
mkdir -p coverage/rust
mkdir -p coverage/ui

# ============================================================================
# Rust Coverage
# ============================================================================
echo ""
echo "[coverage] Running Rust tests with coverage instrumentation..."

LLVM_PROFILE_FILE_NAME="renderer-%m.profraw" \
cargo llvm-cov \
  --no-report \
  --ignore-filename-regex "test_repositories|.cargo|.rustup" \
  --all-features \
  --workspace \
  --lib --tests \
  --no-fail-fast \
  --cargo-profile "${CARGO_PROFILE}" \
  test

echo "[coverage] Rust tests completed with exit code: $?"

# Check profraw files and disk usage
PROFRAW_COUNT=$(find "${LLVM_COV_DIR}" -name "*.profraw" 2>/dev/null | wc -l || echo "0")
PROFRAW_SIZE=$(find "${LLVM_COV_DIR}" -name "*.profraw" -exec du -ch {} + 2>/dev/null | tail -1 | awk '{print $1}' || echo "unknown")
LLVM_COV_SIZE=$(du -sh "${LLVM_COV_DIR}" 2>/dev/null | awk '{print $1}' || echo "unknown")
echo "[coverage] Found ${PROFRAW_COUNT} profraw files (${PROFRAW_SIZE}), llvm-cov-target total: ${LLVM_COV_SIZE}"

# Merge profraw files into the expected profdata location
PROJECT_NAME="renderer"
PROFDATA_FILE="${LLVM_COV_DIR}/${PROJECT_NAME}.profdata"
echo "[coverage] Merging profraw files into ${PROFDATA_FILE}..."
MERGE_START=$(date +%s)
llvm-profdata merge -sparse \
  -o "${PROFDATA_FILE}" \
  $(find "${LLVM_COV_DIR}" -name "*.profraw" 2>/dev/null)
MERGE_END=$(date +%s)
MERGE_TIME=$((MERGE_END - MERGE_START))
PROFDATA_SIZE=$(du -sh "${PROFDATA_FILE}" 2>/dev/null | awk '{print $1}' || echo "unknown")
echo "[coverage] Profraw files merged in ${MERGE_TIME}s (profdata size: ${PROFDATA_SIZE})"

# Delete the prof files that cargo llvm-cov creates
rm -f "${LLVM_COV_DIR}"/*.profraw 2>/dev/null
rm -f "${LLVM_COV_DIR}"/*-profraw-list 2>/dev/null

# Generate coverage reports using llvm-cov
echo "[coverage] Generating Rust coverage reports..."
REPORT_START=$(date +%s)

# Find all instrumented test binaries
BINARIES=()
while IFS= read -r binary; do
  BINARIES+=("$binary")
done < <(find "${LLVM_COV_DIR}/debug/deps" -type f -executable 2>/dev/null | grep -E "-(test|lib)" | sort)

# On Windows, look for .exe files instead
if [ "${#BINARIES[@]}" -eq 0 ]; then
  while IFS= read -r binary; do
    BINARIES+=("$binary")
  done < <(find "${LLVM_COV_DIR}/debug/deps" -type f -name "*.exe" 2>/dev/null | sort)
fi

echo "[coverage] Found ${#BINARIES[@]} instrumented binaries"

if [ "${#BINARIES[@]}" -eq 0 ]; then
  echo "[coverage] ERROR: No instrumented binaries found"
  exit 1
fi

# Ignore patterns matching cargo llvm-cov behavior
IGNORE_PATTERN="test_repositories|\.cargo|\.rustup|/tests/|\\\\rustc\\\\|\\\\target\\\\llvm-cov-target|\\\\cargo\\\\(registry|git)|\\\\rustup\\\\toolchains"

# Create temporary directory for parallel lcov generation
TEMP_COV_DIR="${LLVM_COV_DIR}/temp_lcov_$$"
mkdir -p "$TEMP_COV_DIR"

# Convert paths to Windows format if on MSYS/Windows (llvm-cov is a native Windows tool)
if [[ "$OSTYPE" == "msys" || "$OSTYPE" == "win32" ]]; then
  PROFDATA_FILE_NATIVE="$(cygpath -w "${PROFDATA_FILE}")"
else
  PROFDATA_FILE_NATIVE="${PROFDATA_FILE}"
fi

# Run llvm-cov export in parallel (one thread per binary)
echo "[coverage] Running llvm-cov in parallel (${#BINARIES[@]} threads)..."
PIDS=()
for i in "${!BINARIES[@]}"; do
  binary="${BINARIES[$i]}"
  # Convert binary path to Windows format if needed
  if [[ "$OSTYPE" == "msys" || "$OSTYPE" == "win32" ]]; then
    binary_native="$(cygpath -w "${binary}")"
  else
    binary_native="${binary}"
  fi
  output_file="${TEMP_COV_DIR}/cov_${i}.lcov"

  (
    error_file="${TEMP_COV_DIR}/error_${i}.txt"
    if llvm-cov export \
      -format=lcov \
      -instr-profile="${PROFDATA_FILE_NATIVE}" \
      -ignore-filename-regex="${IGNORE_PATTERN}" \
      "$binary_native" \
      > "$output_file" 2>"$error_file"; then
      # Success
      exit 0
    fi

    # Failed - check if it's due to missing coverage data (benign)
    if grep -q "no coverage data found" "$error_file" 2>/dev/null; then
      # Create empty file to avoid merge issues
      echo -n "" > "$output_file"
      exit 0
    fi

    # Real error
    echo "[coverage] ERROR: llvm-cov export failed for binary $i ($(basename "$binary")):" >&2
    cat "$error_file" >&2
    exit 1
  ) &
  PIDS+=($!)
done

# Wait for all parallel llvm-cov jobs and collect failures
FAILED=0
for pid in "${PIDS[@]}"; do
  if ! wait "$pid"; then
    FAILED=$((FAILED + 1))
  fi
done

if [ $FAILED -gt 0 ]; then
  echo "[coverage] ERROR: $FAILED llvm-cov export jobs failed" >&2
  exit 1
fi

echo "[coverage] Merging Rust coverage reports..."
# Merge all lcov files using simple concatenation
cat "$TEMP_COV_DIR"/*.lcov > coverage/rust/latest.info 2>/dev/null

rm -rf "$TEMP_COV_DIR"

# Generate HTML report for Rust coverage
if command -v grcov &> /dev/null; then
  echo "[coverage] Generating Rust HTML report..."
  if ! grcov coverage/rust/latest.info \
    -s "${ROOT_DIR}" \
    -t html \
    -o coverage/rust/html 2>&1; then
    echo "[coverage] ERROR: grcov failed to generate HTML report" >&2
    exit 1
  fi
else
  echo "[coverage] WARNING: grcov not installed, skipping HTML report generation"
  echo "[coverage] Install grcov with: cargo install grcov"
fi

REPORT_END=$(date +%s)
REPORT_TIME=$((REPORT_END - REPORT_START))
echo "[coverage] Rust coverage reports generated in ${REPORT_TIME}s"

# ============================================================================
# UI Coverage
# ============================================================================
echo ""
echo "[coverage] Running UI tests with coverage instrumentation..."

cd ui

# Check if node_modules exists
if [ ! -d "node_modules" ]; then
  echo "[coverage] Installing UI dependencies..."
  pnpm install --frozen-lockfile
fi

# Run coverage
if ! pnpm run test:coverage; then
  echo "[coverage] ERROR: UI coverage generation failed"
  exit 1
fi

# Move coverage to root coverage directory
if [ -d "coverage" ]; then
  cp -r coverage ../coverage/ui/
  echo "[coverage] UI coverage report copied to coverage/ui/"
fi

cd ..

# ============================================================================
# Summary
# ============================================================================
echo ""
echo "================================================"
echo "✅ Coverage reports generated successfully!"
echo "================================================"
echo ""
echo "Rust Coverage:"
echo "  - LCOV: coverage/rust/latest.info"
if [ -d "coverage/rust/html" ]; then
  echo "  - HTML: coverage/rust/html/index.html"
fi
echo ""
echo "UI Coverage:"
if [ -d "coverage/ui/html" ]; then
  echo "  - HTML: coverage/ui/html/index.html"
elif [ -d "coverage/ui" ]; then
  echo "  - Location: coverage/ui/"
fi
echo ""
