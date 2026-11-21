#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ANDROID_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "Checking for forbidden Google dependencies in Android module..."

FORBIDDEN_PATTERN='com\.google\.(firebase|android\.gms|mlkit)|play-services|firebase|mlkit|NearbyConnections|analytics|crashlytics|sentry|bugsnag|appcenter|hockeyapp|rollbar|mixpanel|amplitude|segment\.com|adjust\.com|appsflyer|admob|adservices|applovin|unity-ads|ironsource|chartboost|adcolony|startapp'

# Search source and configuration files outside build/. The Gradle build file
# also contains the forbidden-coordinate list used by its own resolved-artifact
# check, so inspect only dependency declarations there to avoid self-matches.
if grep -rniE "$FORBIDDEN_PATTERN" "$ANDROID_DIR" \
    --exclude-dir=build \
    --exclude-dir=.gradle \
    --include="*.kt" \
    --include="*.java" \
    --include="*.xml" \
    --include="*.properties" 2>/dev/null; then
  echo "ERROR: Forbidden Google dependency found in Android sources!"
  exit 1
fi

GRADLE_DEPENDENCY_PATTERN='^[[:space:]]*(implementation|api|compileOnly|runtimeOnly|testImplementation|androidTestImplementation|debugImplementation|releaseImplementation|kapt|ksp)[[:space:]]*[(]'
if grep -rniE "$GRADLE_DEPENDENCY_PATTERN.*($FORBIDDEN_PATTERN)" "$ANDROID_DIR" \
    --exclude-dir=build \
    --exclude-dir=.gradle \
    --include="*.gradle.kts" \
    --include="*.gradle" 2>/dev/null; then
  echo "ERROR: Forbidden Google dependency declared in Gradle!"
  exit 1
fi

echo "Zero forbidden Google dependencies found in Android module."
exit 0
