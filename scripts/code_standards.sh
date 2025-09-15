#!/usr/bin/env bash
set -euo pipefail

echo "[code_standards] Running cargo format"
cargo fmt

echo "[code_standards] Running cargo clippy"
cargo clippy --all-targets --all-features -- -D warnings

echo "[code_standards] Running cargo test..."
cargo test --all --all-features

echo "[code_standards] OK"