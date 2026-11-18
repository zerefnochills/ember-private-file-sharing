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
if [[ "$api" != 34 ]]; then
  echo "M10 CLI-to-Android interop requires an API 34 emulator; found API $api." >&2
  exit 2
fi

tmp_input="$(mktemp)"
relay_log="$(mktemp)"
relay_pid=""
cleanup() {
  if [[ -n "$relay_pid" ]] && kill -0 "$relay_pid" 2>/dev/null; then kill "$relay_pid"; wait "$relay_pid" || true; fi
  rm -f "$tmp_input" "$relay_log"
}
trap cleanup EXIT

expected_sha256="$(python - "$tmp_input" <<'PY'
import hashlib, os, sys
data = os.urandom(256 * 1024)
with open(sys.argv[1], "wb") as output:
    output.write(data)
print(hashlib.sha256(data).hexdigest())
PY
)"
passphrase="$(od -An -N24 -tx1 /dev/urandom | tr -d ' \n')"

cargo build --manifest-path android/test-relay-server/Cargo.toml --bin ember-android-test-server
relay_bin="target/debug/ember-android-test-server"
cli_bin="target/debug/ember"
if [[ -f "${relay_bin}.exe" ]]; then relay_bin="${relay_bin}.exe"; fi
if [[ -f "${cli_bin}.exe" ]]; then cli_bin="${cli_bin}.exe"; fi
"$relay_bin" >"$relay_log" 2>&1 &
relay_pid=$!
ready=0
for _ in $(seq 1 100); do
  if curl -fsS http://127.0.0.1:8080/v1/pow/challenge >/dev/null; then ready=1; break; fi
  sleep 0.1
done
if [[ "$ready" != 1 ]]; then
  echo "Local Ember integration relay did not start." >&2
  exit 1
fi

cargo build -p ember-cli --bin ember
cli_output="$("$cli_bin" send "$tmp_input" --relay http://127.0.0.1:8080 --passphrase "$passphrase" --burn --no-strip)"
share_link="$(printf '%s\n' "$cli_output" | sed -n 's/^Share link: //p' | tail -n 1)"
unset cli_output
if [[ -z "$share_link" ]]; then
  echo "CLI send did not produce a share link." >&2
  exit 1
fi

"$gradle_bin" --no-daemon -p android connectedDebugAndroidTest \
  -Pandroid.testInstrumentationRunnerArguments.class=com.ember.mobile.CliInteropInstrumentedTest \
  -Pandroid.testInstrumentationRunnerArguments.m10_link="$share_link" \
  -Pandroid.testInstrumentationRunnerArguments.m10_passphrase="$passphrase" \
  -Pandroid.testInstrumentationRunnerArguments.m10_sha256="$expected_sha256"
unset share_link passphrase
echo "CLI file drop received by Android: passphrase protection and burn-after-view verified."
