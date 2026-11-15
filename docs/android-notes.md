# Android development notes

## CURRENT STATE

- Milestone in progress: none. M10 send and receive is complete; this is the a10-done handoff state.
- Done: M9 remains complete/tagged. M10 covers streamed text/file send and receive, protected links, bounded PoW, HTTPS relay retries, QR generation/scanning, share/paste link intake, burn confirmation, Compose progress, metadata stripping review, private temporary receive output, document pickers, and privacy flags. Core streaming and metadata stripping remain behind Rust-owned UniFFI wrappers. Receive verifies metadata and completes core `finish()` before exposing plaintext; failed transfers wipe temporary output. Metadata stripping remains capped at 8 MiB, with explicit pass-through above that size or for unsupported formats.
- Done: finish retries up to five attempts and treats an ambiguous lost response followed by 404/410 as success for a burn drop. Unit tests cover the lost-response case. `FLAG_SECURE` is set for the whole activity window, including link, QR, passphrase, and decrypted-content screens. Incoming intents without text set the incoming link to empty and clear prior receive state.
- Done: one API 34 emulator run sent and received a burn-after-view drop against the host `ember-server` at `10.0.2.2`, then corrupted a second real relay response and confirmed decryption rejected it while the blob stayed claimed and undeleted. The host server needed a temporary CLI-only build workaround because its `--host` short option conflicts with clap's `-h` help flag; the backend source was restored after the run.
- Validation: final `scripts/check-android.sh` passed on 2026-11-16: 13 Rust binding tests, 25 Android unit tests, 6 connected API 34 tests, lint, three native ABI builds, R8 release minification, and dependency checks. The optional real-server instrumentation case was skipped in the final gate because the test service had been stopped after the separate successful real-server run. A prior test-harness and APK-path issue was corrected before the final passing gate.
- Release APK sizes (unsigned): arm64-v8a 8,164,094 bytes (7.79 MiB); armeabi-v7a 7,818,604 bytes (7.46 MiB); x86_64 8,196,863 bytes (7.82 MiB). `*.salive` is ignored.
- Not done / manual QA: M11 has not started; cross-process transfer resume was skipped; low-memory device measurement was skipped. Physical camera scanning and SAF picker/save flows were not exercised manually. Current resume remains limited to request retries and an in-memory active claim.
- Known issue: backend-owned `server/src/main.rs` gives `--host` the `-h` short flag, conflicting with clap's automatic help option and panicking at server startup. No Android gate tests are failing.
- Exact next step for tomorrow: backend owner resolves the `server/src/main.rs` host/help flag conflict and verifies `cargo run -p ember-server -- --help`; then wait for the owner to set the next scope. Keep M11 unstarted until then.

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
