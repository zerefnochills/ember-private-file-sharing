#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

timeout_seconds="${EMBER_ANDROID_TIMEOUT_SECONDS:-1200}"
run_timed() {
  if command -v timeout >/dev/null 2>&1; then
    timeout "${timeout_seconds}s" "$@"
  else
    "$@"
  fi
}

if [[ -z "${ANDROID_HOME:-}" && -n "${LOCALAPPDATA:-}" && -d "${LOCALAPPDATA}/Android/Sdk" ]]; then
  if command -v cygpath >/dev/null 2>&1; then
    export ANDROID_HOME="$(cygpath -u "${LOCALAPPDATA}/Android/Sdk")"
  else
    export ANDROID_HOME="${LOCALAPPDATA}/Android/Sdk"
  fi
fi
export ANDROID_SDK_ROOT="${ANDROID_SDK_ROOT:-${ANDROID_HOME:-}}"
if [[ -z "${ANDROID_HOME:-}" || ! -d "$ANDROID_HOME" ]]; then
  echo "ANDROID_HOME must point to an installed Android SDK." >&2
  exit 2
fi

if [[ -d "$ANDROID_HOME/ndk/28.2.13676358" ]]; then
  export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-$ANDROID_HOME/ndk/28.2.13676358}"
  export ANDROID_NDK_ROOT="${ANDROID_NDK_ROOT:-$ANDROID_NDK_HOME}"
fi

echo "=== Android toolchain ==="
java -version
printf 'SDK: %s\n' "$ANDROID_HOME"
printf 'NDK: %s\n' "${ANDROID_NDK_HOME:-not set}"
cargo ndk --version

echo "=== Rust formatting ==="
run_timed cargo fmt --all --check

echo "=== Rust bindings build and tests ==="
run_timed cargo build -p ember-android-bindings
run_timed cargo test -p ember-android-bindings

echo "=== Build Android native libraries and Kotlin bindings ==="
run_timed cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 \
  -o android/app/src/main/jniLibs build --manifest-path android-bindings/Cargo.toml --release
run_timed cargo run --manifest-path android-bindings/Cargo.toml --features cli \
  --bin uniffi-bindgen -- generate \
  "target/aarch64-linux-android/release/libember_android_bindings.so" \
  --language kotlin --no-format --out-dir android/app/build/generated/uniffi/kotlin \
  --config android-bindings/uniffi.toml

echo "=== Android lint, unit tests, dependency check, and APK ==="
if [[ -n "${EMBER_GRADLE_BIN:-}" ]]; then
  gradle_command=("$EMBER_GRADLE_BIN")
else
  gradle_command=(android/gradlew)
fi
run_timed "${gradle_command[@]}" --no-daemon -p android lint testDebugUnitTest \
  :app:checkForbiddenDependencies assembleDebug

if ! command -v adb >/dev/null 2>&1; then
  echo "adb is required to run M9's device vector test." >&2
  exit 2
fi
if ! adb devices | awk 'NR > 1 && $2 == "device" { found = 1 } END { exit !found }'; then
  echo "No Android device/emulator is connected; M9's on-device test cannot run." >&2
  exit 2
fi
echo "=== On-device M9 test-vector screen ==="
run_timed "${gradle_command[@]}" --no-daemon -p android connectedDebugAndroidTest

apk="android/app/build/outputs/apk/debug/app-debug.apk"
if [[ ! -f "$apk" ]]; then
  echo "Expected debug APK was not produced: $apk" >&2
  exit 1
fi
apk_bytes="$(wc -c < "$apk" | tr -d ' ')"
printf 'APK size: %s bytes (%.2f MiB)\n' "$apk_bytes" "$(awk -v n="$apk_bytes" 'BEGIN { print n / 1048576 }')"

echo "=== Forbidden dependency source scan ==="
if grep -RniE 'com\.google\.(firebase|android\.gms|mlkit)|play-services|firebase[-:]|mlkit|NearbyConnections' \
  android/app/src/main; then
  echo "Forbidden Android dependency reference found." >&2
  exit 1
fi
if grep -nE '^[[:space:]]*(implementation|api|runtimeOnly|testImplementation|androidTestImplementation|debugImplementation)[[:space:]]*\([^)]*(com\.google\.(firebase|android\.gms|mlkit)|play-services|firebase[-:]|mlkit|NearbyConnections)' \
  android/app/build.gradle.kts; then
  echo "Forbidden Android dependency declaration found." >&2
  exit 1
fi

echo "Android gate passed."
