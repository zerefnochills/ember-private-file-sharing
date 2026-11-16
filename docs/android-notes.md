# Android development notes

## CURRENT STATE

- Milestone: Android A11 drops-only v1 is complete. Do not build thread UI.
- Done: M9 and M10 are complete/tagged. M10 includes streamed text/file send and receive, protected links, bounded PoW, HTTPS relay retries, QR generation/scanning, share/paste link intake, burn confirmation, Compose progress, metadata stripping review, private temporary receive output, document pickers, and privacy flags. Receive verifies metadata and completes core `finish()` before exposing plaintext; failed transfers wipe temporary output. Metadata stripping remains capped at 8 MiB, with explicit pass-through above that size or for unsupported formats.
- Done: finish retry handles ambiguous lost responses for burn drops; `FLAG_SECURE` covers the app window; incoming intents without text clear prior receive state. Received drops have explicit-confirmation reporting through `/v1/reports` and device-local exact-drop blocking. Optional decrypted text and link key are sent only when selected and confirmed.
- Done: every Android string has English and Hindi values, including transfer progress, errors, metadata summaries, and report/block dialogs. A 280 dp Hindi wrapping instrumentation test passes.
- Done: TalkBack descriptions cover the burn option and camera preview; status updates are a polite live region; all buttons and the burn checkbox have at least 48 dp targets. API 34 instrumentation checks labels, target bounds, and 4.5:1 text/button contrast.
- Done: the default connected instrumentation task builds and starts the actual `ember-server` router through an Android-owned harness, runs burn-after-view receive plus tampered-data rejection, and stops the host process. The helper uses in-memory storage and zero PoW difficulty. HTTP is enabled only in the debug manifest and test-only client factory; the release client remains HTTPS-only.
- Checkpoints: report/block `60f1079`; English/Hindi `fd6f260`; accessibility `19c64d9`; default real-server test `3351381`.
- Integration: backend commits `96ff359` and `0846275` from local `main` are merged into `android-work` (merge commits `88f3b2c` and `6b06048`). The integration is not pushed.
- Final gate: `scripts/check-android.sh` passed on 2026-11-16: 13 Rust binding tests, 26 Android unit tests, 11 connected API 34 instrumentation tests with zero skipped, lint, all three native ABI builds, R8 release minification, and forbidden-dependency checks. The real-server burn/tamper test ran as part of the connected suite; its host server started and stopped automatically. No tests failed.
- Post-merge verification: `scripts/check.ps1` passed formatting, clippy, binary builds, workspace tests, and `cargo deny check` (`advisories`, `bans`, `licenses`, and `sources` all OK; cargo-deny emits duplicate-version and unused-license-allow warnings). The script then failed because the banned-words scan searched generated Android `.class`/`.dex` build outputs and matched `anonymous`; a clean checkout or scan exclusion is needed for that local ordering. The connected Android task did not finish: the `EmberM9_API34` emulator disappeared from adb during package cleanup (`emulator-5554` not found), so rerun it with a stable device.
- PR CI diagnosis at `ef30d99`: the Android dependency scan falsely matched its own forbidden-coordinate list in `app/build.gradle.kts`; the scan now checks Gradle dependency declarations separately. Backend path dependencies now declare explicit `0.0.1` versions and `cargo deny check` passes on the merged tree.
- Final APK sizes (unsigned): arm64-v8a debug 23,549,790 bytes (22.46 MiB), release 8,230,010 bytes (7.85 MiB); armeabi-v7a debug 23,204,300 bytes (22.13 MiB), release 7,884,520 bytes (7.52 MiB); x86_64 debug 23,582,559 bytes (22.49 MiB), release 8,262,779 bytes (7.88 MiB). `*.salive` remains ignored.
- Not done / manual QA: M11 thread UI is intentionally excluded from drops-only v1. Cross-process resume and low-memory device measurement were skipped. Physical camera scanning and SAF picker/save flows were not manually exercised. Resume remains limited to request retries and an in-memory active claim.
- Known issue: GitHub PR #1 still points to the old remote head `ef30d99`; the local merge and Android hygiene fix are not on the PR yet. The server CLI `--host`/`-h` collision is fixed in `96ff359`, and cargo-deny wildcard failures are fixed in `0846275`.
- Exact next step: rerun `scripts/check-android.sh` with the API 34 emulator stable, then update the PR branch only after the owner authorizes a push. Keep thread UI out of scope.

## 2026-11-15 — M9 completed

- Branch: `android-work`, based on committed snapshot `a924269`.
- JDK: Temurin 25.0.4.1 (build `25.0.4.1+1-LTS`).
- Android Gradle Plugin: 9.5.0.
- Gradle: 9.6.0, pinned by `android/gradle/wrapper/gradle-wrapper.properties` with SHA-256 `bbaeb2fef8710818cf0e261201dab964c572f92b942812df0c3620d62a529a01`.
- Kotlin Compose compiler plugin: 2.4.10; AGP 9 provides Kotlin support, so the app does not apply `org.jetbrains.kotlin.android`.
- Compose BOM: 2026.09.00.
- Android SDK: compile platform 37.0 (SDK also has 37.2); Build Tools 37.0.0.
- Android NDK: 28.2.13676358.
- `cargo-ndk`: 4.1.2.
- Rust Android targets: `aarch64-linux-android`, `armv7-linux-androideabi`, and `x86_64-linux-android`.
- Emulator acceptance device: API 34, x86_64 Google APIs image, `EmberM9_API34`.

### Changes

- Moved the UniFFI wrapper into the sibling `android-bindings/` crate; the sole root manifest change is its workspace member, with the corresponding generated lockfile rename.
- Added a Compose debug action that runs all 8 positive decrypt and 5 negative decrypt/link vectors through generated Rust bindings. It reports only the result count; plaintext is not displayed.
- Added instrumentation coverage for the vector screen and direct calls through the generated bindings.
- Added `scripts/check-android.sh`, an Android `.gitignore`, and wrapper files. The gate supports `EMBER_GRADLE_BIN` for a locally installed Gradle binary when wrapper distribution download is unavailable; the default remains the pinned wrapper.
- Removed tracked prebuilt legacy bridge `.so` libraries; the gate builds ignored `.so` output for all three supported ABIs.
- Updated the Gradle Kotlin compile task to include generated UniFFI Kotlin output under AGP 9.5. Android's Java source directory list alone did not feed that generated directory into Kotlin compilation.
- Removed a plaintext value from the debug screen result and ensure the vector runner wipes decrypted byte arrays even when an assertion fails.

### Validation

- `scripts/check-android.sh`: passed end to end, including `cargo fmt --all --check`, Android binding build/tests, all three `cargo ndk` ABIs, Kotlin binding generation, Gradle lint, Android unit tests, forbidden dependency checks, debug APK build, and connected emulator instrumentation tests.
- Rust UniFFI tests: 5 passed, 0 failed.
- Android unit tests: 7 passed, 0 failed.
- API 34 emulator tests: 2 passed, 0 failed; `debugScreenRunsEverySharedVectorThroughRustBindings` verified all 13 shared vectors.
- Debug APK: 19,765,201 bytes (18.85 MiB).
- The environment's default `bash` launcher points to WSL with no distribution. The gate was run with Git Bash and explicit Android SDK/NDK paths. Java could not download Gradle within its 10-second wrapper timeout, so the gate used the checksum-verified local Gradle 9.6.0 binary through `EMBER_GRADLE_BIN`.
- `android/build-native.ps1` accepts optional `EMBER_LLVM_MINGW_BIN` instead of embedding a developer-specific toolchain path.

### Next

Start M10 with the documented API only: inspect the existing server routes and protocol specification, then build the Android send flow in small checkpointed steps. Do not modify backend-owned files.
