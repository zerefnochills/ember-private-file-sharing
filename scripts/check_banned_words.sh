#!/usr/bin/env bash
set -euo pipefail

echo "=== Scanning User-Facing Files for Banned Marketing Buzzwords ==="
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${ROOT_DIR}"

TARGET_PATHS=(
    "README.md"
    "docs/distribution/store-listing.md"
    "docs/distribution/fdroid-metadata.yml"
    "tools/cli/src"
    "tools/admin-cli/src"
    "web-receiver/index.html"
)

if [ -d "web-receiver/src" ]; then
    TARGET_PATHS+=("web-receiver/src")
fi

if [ -d "android" ]; then
    TARGET_PATHS+=("android")
fi

BANNED_PATTERNS=(
    "anonymous"
    "untraceable"
    "military[ -]grade"
    "leaves no trace"
    "no logs"
)

VIOLATIONS=0

for path in "${TARGET_PATHS[@]}"; do
    if [ -e "$path" ]; then
        for pat in "${BANNED_PATTERNS[@]}"; do
            if grep -rniE "\b${pat}\b" "$path" \
                --exclude-dir=build \
                --exclude-dir=.gradle \
                --exclude-dir=.kotlin \
                --exclude-dir=target \
                --exclude='*.class' --exclude='*.dex' --exclude='*.jar' \
                --exclude='*.apk' --exclude='*.aab' 2>/dev/null; then
                echo "  [ERROR] Banned buzzword pattern '${pat}' found in ${path}"
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
