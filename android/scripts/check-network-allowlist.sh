#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

if [[ -z "${EMBER_RELAY_HOST:-}" ]]; then
  echo "Set EMBER_RELAY_HOST to the configured HTTPS Ember relay hostname." >&2
  exit 2
fi
if [[ ! "$EMBER_RELAY_HOST" =~ ^[A-Za-z0-9.-]+$ ]]; then
  echo "EMBER_RELAY_HOST must be a hostname without scheme, port, path, or credentials." >&2
  exit 2
fi
if ! command -v adb >/dev/null 2>&1 || ! command -v python3 >/dev/null 2>&1; then
  echo "adb and python3 are required." >&2
  exit 2
fi
if ! adb devices | awk 'NR > 1 && $2 == "device" { found = 1 } END { exit !found }'; then
  echo "Start an Android emulator before running the network proof." >&2
  exit 2
fi

tmp_dir="$(mktemp -d)"
proxy_log="$tmp_dir/proxy.jsonl"
proxy_output="$tmp_dir/proxy.log"
proxy_port="${EMBER_PROXY_PORT:-8899}"
old_proxy="$(adb shell settings get global http_proxy | tr -d '\r')"
proxy_pid=""
proof_test="android/app/src/androidTest/java/com/ember/mobile/NetworkHostAllowlistInstrumentedTest.kt"
proof_test_template="android/scripts/network-proof/NetworkHostAllowlistInstrumentedTest.kt"
installed_proof_test=false
restore() {
  if [[ -n "$proxy_pid" ]]; then kill "$proxy_pid" 2>/dev/null || true; wait "$proxy_pid" 2>/dev/null || true; fi
  if [[ -z "$old_proxy" || "$old_proxy" == "null" || "$old_proxy" == ":0" ]]; then
    adb shell settings put global http_proxy :0 >/dev/null 2>&1 || true
  else
    adb shell settings put global http_proxy "$old_proxy" >/dev/null 2>&1 || true
  fi
  if [[ "$installed_proof_test" == true ]]; then rm -f "$proof_test"; fi
  rm -f "$proxy_log" "$proxy_output"
  rmdir "$tmp_dir" 2>/dev/null || true
}
trap restore EXIT

if [[ -e "$proof_test" ]]; then
  echo "A temporary network proof test is already present: $proof_test" >&2
  exit 1
fi
cp "$proof_test_template" "$proof_test"
installed_proof_test=true

python3 android/scripts/logging_proxy.py --expected "$EMBER_RELAY_HOST" --port "$proxy_port" \
  --log "$proxy_log" >"$proxy_output" 2>&1 &
proxy_pid=$!
for _ in $(seq 1 50); do
  if grep -q "Logging proxy listening" "$proxy_output"; then break; fi
  sleep 0.1
done
grep -q "Logging proxy listening" "$proxy_output" || { cat "$proxy_output" >&2; exit 1; }

android/gradlew --no-daemon -p android -PemberRelayHost="$EMBER_RELAY_HOST" \
  :app:installDebug :app:installDebugAndroidTest
adb shell settings put global http_proxy "10.0.2.2:$proxy_port"
adb shell pm clear com.ember.mobile >/dev/null
adb shell am force-stop com.ember.mobile
adb shell am start -n com.ember.mobile/.MainActivity >/dev/null
sleep 3
if [[ -s "$proxy_log" ]]; then
  echo "The app contacted a host before an explicit relay operation (possibly before terms acceptance)." >&2
  cat "$proxy_log" >&2
  exit 1
fi

instrumentation_output="$(adb shell am instrument -w -e class com.ember.mobile.NetworkHostAllowlistInstrumentedTest \
  com.ember.mobile.test/androidx.test.runner.AndroidJUnitRunner 2>&1)"
printf '%s\n' "$instrumentation_output"
grep -Eq 'OK \(1 test\)|INSTRUMENTATION_CODE: 0' <<<"$instrumentation_output" || {
  echo "Relay network probe instrumentation failed." >&2
  exit 1
}
python3 - "$proxy_log" "$EMBER_RELAY_HOST" <<'PY'
import json
import sys

log_path, expected_host = sys.argv[1:]
events = [json.loads(line) for line in open(log_path, encoding="utf-8") if line.strip()]
expected = expected_host.lower()
if not events:
    raise SystemExit("The logging proxy recorded no relay request.")
bad = [event for event in events if not event["allowed"] or event["host"] != expected]
if bad:
    raise SystemExit(f"Unexpected network destinations: {bad}")
print(f"Network allow-list passed: {len(events)} CONNECT request(s), host={expected} only.")
PY
