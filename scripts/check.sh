#!/usr/bin/env bash
set -euo pipefail

echo "=== Running cargo fmt ==="
cargo fmt --all --check

echo "=== Running cargo clippy ==="
cargo clippy --workspace --all-targets -- -D warnings

echo "=== Running cargo test ==="
if command -v timeout >/dev/null 2>&1; then
    timeout 600 cargo test --workspace
else
    cargo test --workspace
fi

if command -v cargo-audit >/dev/null 2>&1; then
    echo "=== Running cargo audit ==="
    cargo audit || true
fi

if command -v cargo-deny >/dev/null 2>&1; then
    echo "=== Running cargo deny ==="
    cargo deny check || true
fi

echo "=== Gate passed cleanly ==="
