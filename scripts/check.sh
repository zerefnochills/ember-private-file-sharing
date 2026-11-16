#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${SCRIPT_DIR}/.."

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
    cargo audit
fi

if command -v cargo-deny >/dev/null 2>&1; then
    echo "=== Running cargo deny ==="
    cargo deny check
fi

echo "=== Running license and dependency checks ==="
"${SCRIPT_DIR}/check_licenses.sh"

echo "=== Running banned marketing buzzwords checks ==="
"${SCRIPT_DIR}/check_banned_words.sh"

echo "=== Gate passed cleanly ==="
