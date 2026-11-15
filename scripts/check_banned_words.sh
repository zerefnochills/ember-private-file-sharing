#!/usr/bin/env bash
set -euo pipefail

echo "=== Scanning User-Facing Files for Banned Marketing Buzzwords ==="
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${ROOT_DIR}"

TARGET_FILES=(
    "README.md"
    "docs/distribution/store-listing.md"
    "docs/distribution/fdroid-metadata.yml"
    "tools/cli/src/main.rs"
    "tools/admin-cli/src/main.rs"
    "web-receiver/index.html"
)

BANNED_PATTERNS=(
    "military[- ]grade"
    "untraceable"
    "leaves no trace"
    "no logs"
)

VIOLATIONS=0

for file in "${TARGET_FILES[@]}"; do
    if [ -f "$file" ]; then
        for pat in "${BANNED_PATTERNS[@]}"; do
            if grep -Ei "\b${pat}\b" "$file" >/dev/null 2>&1; then
                echo "  [ERROR] Banned buzzword pattern '${pat}' found in ${file}"
                VIOLATIONS=$((VIOLATIONS + 1))
            fi
        done
    fi
done

if [ "$VIOLATIONS" -gt 0 ]; then
    echo "Banned words check failed with ${VIOLATIONS} violation(s)."
    exit 1
fi

echo "Banned words check passed cleanly. Zero prohibited marketing buzzwords found."
