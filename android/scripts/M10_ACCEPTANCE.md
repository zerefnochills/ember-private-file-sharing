# M10 Android acceptance scripts

These scripts use an API 34 emulator and the in-process relay test harness. Set `ANDROID_SERIAL` if more than one emulator is connected. They do not read `.env` files or contain credentials.

## Interoperability

Run `./android/scripts/m10-cli-to-android.sh` to generate a transient file, send it with the CLI to the host relay, and receive it in the app on the emulator. The run uses a runtime-generated passphrase, enables burn-after-view, checks the received SHA-256, and deletes its local test file. The link is held in process memory and passed directly to the Android instrumentation runner.

Run `python android/scripts/m10-android-to-cli.py` to send an Android file and text drop to the CLI. A local in-memory collector passes the two links to the host CLI without writing them to disk or logging request bodies. The file uses a runtime passphrase and burn-after-view; the text drop uses burn-after-view.

Add `--cli-only` to omit the web receiver attempt after the CLI checks. Without it, the script opens the third Android-generated text link in headless Chrome against the local server's embedded receiver and expects the page to decrypt it.

CLI-to-Android file interoperability passed with a passphrase-protected, burn-after-view drop. The CLI currently has no text-drop send command, so CLI-to-Android text interoperability remains open under request A-3 in `docs/cross-agent-requests.md`.

Android-to-CLI file and text interoperability passed: the CLI decrypted the passphrase-protected, burn-after-view file and the burn-after-view text, and the file digest and text bytes matched. In the web attempt, Chrome opened the link and loaded the embedded receiver page, but the page remained at `Initializing cryptography engine...`; no details or error view appeared and decryption did not start. Web receiver interoperability remains open under request A-4 in `docs/cross-agent-requests.md`.

## Weak network and retry

Run `./android/scripts/m10-weak-network.sh`. It sets the emulator to `hsdpa` speed and `hsdpa` delay (the emulator's 14.4 Mbit/s and 200 ms presets), sends and receives a 20 MiB file, and arms one transient HTTP 503 for upload and download. The Android client must retry both operations successfully. The script restores full speed and no delay on exit.

Verified on API 34: 20 MiB send and receive both completed after the injected transient upload and download failures; emulator settings were `speed=hsdpa` and `delay=hsdpa` (14.4 Mbit/s, 200 ms). The script restored full speed and no delay.

## Memory

Run `./android/scripts/m10-memory.sh`. It uses the same speed and delay presets while sending and receiving a 50 MiB file, samples `adb shell dumpsys meminfo com.ember.mobile` once per second, prints every PSS sample and the peak, and fails above `M10_MAX_PSS_KIB` (default 160000 KiB). Set that environment variable only when a different documented bound is intended. The script restores the emulator network on exit.

Verified on API 34: 50 MiB send and receive completed; 23 PSS samples peaked at 119547 KiB (116.75 MiB), below the 160000 KiB bound. The measured peak is consistent with bounded streaming rather than whole-file buffering.

## OWNER MANUAL CHECKS (physical phone required)

- Camera scanning: install the debug APK on a physical phone; open Receive; tap Scan QR code; grant camera permission; scan a valid Ember drop QR code; verify the link details appear; enter the passphrase if requested; open the drop and verify the decrypted content. Record the phone model and Android version.
- SAF picker and save: open Send; choose a real file through the Android document picker; verify its name and size in the send summary; send it through the relay. On a second receive flow, open a file drop, confirm burn-after-view when shown, choose Save in the Android Create Document dialog, select a folder and filename, save, and compare the saved bytes with the original. Record the phone model and Android version.
