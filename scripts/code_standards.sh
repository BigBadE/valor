#!/usr/bin/env bash
set -euo pipefail

echo "[code_standards] Running cargo format"
cargo fmt

echo "[code_standards] Running cargo clippy"
cargo clippy --all-targets --all-features -- -D warnings

# Allow an optional logging spec as first argument; compose RUST_LOG so user entries override defaults.
BASE_LOG="warn,html5ever=warn,wgpu_hal=off,headless_chrome=warn"
USER_LOG_SPEC="${1-}"
if [ -z "${USER_LOG_SPEC}" ]; then
  export RUST_LOG="${BASE_LOG}"
else
  export RUST_LOG="${BASE_LOG},${USER_LOG_SPEC}"
fi

echo "[code_standards] Running cargo test..."
cargo test --all --all-features --test layouter_chromium_compare
cargo test --all --all-features --package valor

echo "[code_standards] OK"