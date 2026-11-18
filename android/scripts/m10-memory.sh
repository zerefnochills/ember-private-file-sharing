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

max_pss_kib="${M10_MAX_PSS_KIB:-160000}"
restore_network() {
  "$adb_bin" "${serial_args[@]}" emu network speed full >/dev/null 2>&1 || true
  "$adb_bin" "${serial_args[@]}" emu network delay none >/dev/null 2>&1 || true
}
trap restore_network EXIT

"$adb_bin" "${serial_args[@]}" emu network speed hsdpa >/dev/null
"$adb_bin" "${serial_args[@]}" emu network delay hsdpa >/dev/null
echo "Emulator network: speed=hsdpa (14.4 Mbit/s preset), delay=hsdpa (200 ms preset)."
echo "Sampling adb shell dumpsys meminfo com.ember.mobile once per second during a 50 MiB send and receive. Peak PSS limit: ${max_pss_kib} KiB."

"$gradle_bin" --no-daemon -p android connectedDebugAndroidTest \
  -Pandroid.testInstrumentationRunnerArguments.class=com.ember.mobile.M10TransferStressInstrumentedTest#fiftyMiBFileSendAndReceiveStaysStreaming &
gradle_pid=$!
peak_pss_kib=0
samples=0
while kill -0 "$gradle_pid" 2>/dev/null; do
  pss_kib="$("$adb_bin" "${serial_args[@]}" shell dumpsys meminfo com.ember.mobile 2>/dev/null \
    | tr -d '\r' | awk '/TOTAL PSS:/ {print $3; exit}')"
  if [[ "$pss_kib" =~ ^[0-9]+$ ]]; then
    samples=$((samples + 1))
    if (( pss_kib > peak_pss_kib )); then peak_pss_kib="$pss_kib"; fi
    printf 'PSS sample %d: %s KiB\n' "$samples" "$pss_kib"
  fi
  sleep 1
done
gradle_status=0
wait "$gradle_pid" || gradle_status=$?
if (( gradle_status != 0 )); then exit "$gradle_status"; fi
if (( samples == 0 )); then echo "No PSS samples were collected." >&2; exit 1; fi
printf 'Peak PSS during 50 MiB send/receive: %s KiB (%s MiB); samples=%s\n' \
  "$peak_pss_kib" "$(awk -v n="$peak_pss_kib" 'BEGIN {printf "%.2f", n/1024}')" "$samples"
if (( peak_pss_kib > max_pss_kib )); then
  echo "Peak PSS exceeded the configured bound; investigate whole-file buffering." >&2
  exit 1
fi
