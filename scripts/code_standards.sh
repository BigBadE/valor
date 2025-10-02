#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")"/.. && pwd -P)
LOG_FILE="${ROOT_DIR}/target/code_standards.log"

run_all() {
  echo "[code_standards] Running cargo format"
  cargo fmt

  echo "[code_standards] Running cargo clippy"
  cargo clippy --all-targets --all-features -- -D warnings

  # Allow an optional logging spec as first argument; compose RUST_LOG so user entries override defaults.
  local BASE_LOG="warn,html5ever=warn,wgpu_hal=off,headless_chrome=warn"
  local USER_LOG_SPEC="${1-}"
  if [ -z "${USER_LOG_SPEC}" ]; then
    local LOG_SPEC="${BASE_LOG}"
  else
    local LOG_SPEC="${BASE_LOG},${USER_LOG_SPEC}"
  fi
  export RUST_LOG=LOG_SPEC

  echo "[code_standards] Running cargo test with args ${LOG_SPEC}..."
  cargo test --all --all-features --test layouter_chromium_compare -- --nocapture
  cargo test --all --all-features --package valor -- --nocapture
}

cleanup_after_ice() {
  echo "[code_standards] Cleaning incremental caches after ICE..."
  
  # Clean incremental compilation artifacts
  if [ -d "${ROOT_DIR}/target/debug/incremental" ]; then
    echo "[code_standards]   Removing incremental cache..."
    rm -rf "${ROOT_DIR}/target/debug/incremental" || true
  fi
  
  if [ -d "${ROOT_DIR}/target/debug/.fingerprint" ]; then
    echo "[code_standards]   Removing fingerprint cache..."
    rm -rf "${ROOT_DIR}/target/debug/.fingerprint" || true
  fi
  
  if [ -d "${ROOT_DIR}/target/debug/build" ]; then
    echo "[code_standards]   Removing build cache..."
    rm -rf "${ROOT_DIR}/target/debug/build" || true
  fi
  
  # Remove ICE log files
  local ice_count
  ice_count=$(find "${ROOT_DIR}" -maxdepth 1 -type f -name 'rustc-ice-*.txt' 2>/dev/null | wc -l)
  if [ "${ice_count}" -gt 0 ]; then
    echo "[code_standards]   Removing ${ice_count} ICE log file(s)..."
    find "${ROOT_DIR}" -maxdepth 1 -type f -name 'rustc-ice-*.txt' -exec rm -f {} + 2>/dev/null || true
  fi
  
  echo "[code_standards] Cleanup complete"
}

detect_ice() {
  # Return 0 if ICE likely, 1 otherwise
  local ice_files
  ice_files=$(find "${ROOT_DIR}" -maxdepth 1 -type f -name 'rustc-ice-*.txt' 2>/dev/null | wc -l)
  if [ "${ice_files}" -gt 0 ]; then
    echo "[code_standards] Found ${ice_files} ICE file(s)"
    return 0
  fi
  if [ -f "${LOG_FILE}" ] && grep -Eiq 'compiler unexpectedly panicked|internal compiler error|rustc-ice-' "${LOG_FILE}" 2>/dev/null; then
    echo "[code_standards] Detected ICE in log file"
    return 0
  fi
  return 1
}

# First run, capturing output
mkdir -p "${ROOT_DIR}/target"
echo "[code_standards] Starting first run..."
set +e  # Temporarily disable exit on error to capture exit code
( set -e; run_all "${1-}" ) 2>&1 | tee "${LOG_FILE}"
code=${PIPESTATUS[0]}
set -e  # Re-enable exit on error
echo "[code_standards] First run completed with exit code: ${code}"

if [ ${code} -ne 0 ]; then
  echo "[code_standards] First run failed with exit code ${code}"
  echo "[code_standards] Checking for ICE..."
  if detect_ice; then
    echo "[code_standards] ICE detected! Running cleanup..."
    cleanup_after_ice
    echo "[code_standards] Retrying after ICE cleanup..."
    set +e
    ( set -e; run_all "${1-}" ) 2>&1 | tee "${LOG_FILE}"
    code=${PIPESTATUS[0]}
    set -e
    if [ ${code} -ne 0 ]; then
      echo "[code_standards] Retry also failed with exit code ${code}"
      # Check if retry also hit an ICE
      if detect_ice; then
        echo "[code_standards] Retry also hit an ICE - this is a persistent compiler bug"
      fi
    else
      echo "[code_standards] Retry succeeded!"
    fi
  else
    echo "[code_standards] No ICE detected, not retrying"
  fi
fi

if [ ${code} -eq 0 ]; then
  echo "[code_standards] ✓ OK"
else
  echo "[code_standards] ✗ FAILED with exit code ${code}. See ${LOG_FILE} for details."
fi
exit ${code}