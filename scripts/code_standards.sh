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
    export RUST_LOG="${BASE_LOG}"
  else
    export RUST_LOG="${BASE_LOG},${USER_LOG_SPEC}"
  fi

  echo "[code_standards] Running cargo test..."
  cargo test --all --all-features --test layouter_chromium_compare
  cargo test --all --all-features --package valor
}

cleanup_after_ice() {
  echo "[code_standards] Detected potential compiler ICE. Cleaning incremental caches..."
  rm -rf "${ROOT_DIR}/target/debug/incremental" \
         "${ROOT_DIR}/target/debug/.fingerprint" \
         "${ROOT_DIR}/target/debug/build" || true
  # Prune rustc ICE logs to reduce clutter
  find "${ROOT_DIR}" -maxdepth 1 -type f -name 'rustc-ice-*.txt' -print -exec rm -f {} + 2>/dev/null || true
}

detect_ice() {
  # Return 0 if ICE likely, 1 otherwise
  if find "${ROOT_DIR}" -maxdepth 1 -type f -name 'rustc-ice-*.txt' | grep -q . 2>/dev/null; then
    return 0
  fi
  if grep -Eiq 'compiler unexpectedly panicked|\bICE\b|rustc-ice-' "${LOG_FILE}" 2>/dev/null; then
    return 0
  fi
  return 1
}

# First run, capturing output
mkdir -p "${ROOT_DIR}/target"
( set -e; run_all "${1-}" ) |& tee "${LOG_FILE}"
code=${PIPESTATUS[0]}

if [ ${code} -ne 0 ]; then
  if detect_ice; then
    cleanup_after_ice
    echo "[code_standards] Retrying after cleanup..."
    ( set -e; run_all "${1-}" ) |& tee "${LOG_FILE}"
    code=${PIPESTATUS[0]}
  fi
fi

if [ ${code} -eq 0 ]; then
  echo "[code_standards] OK"
else
  echo "[code_standards] FAILED with exit code ${code}. See ${LOG_FILE} for details."
fi
exit ${code}