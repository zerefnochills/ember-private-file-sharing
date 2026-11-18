#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"
adb_bin="${ADB:-adb}"
gradle_bin="${EMBER_GRADLE_BIN:-android/gradlew}"
serial_args=()
if [[ -n "${ANDROID_SERIAL:-}" ]]; then serial_args=(-s "$ANDROID_SERIAL"); fi
"$adb_bin" "${serial_args[@]}" get-state >/dev/null
api="$($adb_bin "${serial_args[@]}" shell getprop ro.build.version.sdk | tr -d '\r')"
if [[ "$api" != 34 ]]; then echo "Requires an API 34 emulator; found API $api." >&2; exit 2; fi

restore_network() {
  "$adb_bin" "${serial_args[@]}" emu network speed full >/dev/null 2>&1 || true
  "$adb_bin" "${serial_args[@]}" emu network delay none >/dev/null 2>&1 || true
}
trap restore_network EXIT

"$adb_bin" "${serial_args[@]}" emu network speed hsdpa >/dev/null
"$adb_bin" "${serial_args[@]}" emu network delay hsdpa >/dev/null
echo "Emulator network: speed=hsdpa (14.4 Mbit/s preset), delay=hsdpa (200 ms preset)."
echo "The instrumentation test sends and receives a 20 MiB file and injects one HTTP 503 on upload and download to exercise retry/backoff."
"$gradle_bin" --no-daemon -p android connectedDebugAndroidTest \
  -Pandroid.testInstrumentationRunnerArguments.class=com.ember.mobile.M10TransferStressInstrumentedTest#twentyMiBFileSendAndReceiveRetriesOnTransientRelayErrors
