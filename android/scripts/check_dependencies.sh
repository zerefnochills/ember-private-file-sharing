#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ANDROID_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "Checking for forbidden Google dependencies in Android module..."

FORBIDDEN_PATTERN='com\.google\.(firebase|android\.gms|mlkit)|play-services|firebase-|firebase:|mlkit|NearbyConnections'

# Search all gradle, kotlin, java, and xml files outside build/
if grep -rniE "$FORBIDDEN_PATTERN" "$ANDROID_DIR" \
    --exclude-dir=build \
    --exclude-dir=.gradle \
    --include="*.gradle.kts" \
    --include="*.gradle" \
    --include="*.kt" \
    --include="*.java" \
    --include="*.xml" \
    --include="*.properties" 2>/dev/null; then
  echo "ERROR: Forbidden Google dependency found in Android sources!"
  exit 1
fi

echo "Zero forbidden Google dependencies found in Android module."
exit 0
