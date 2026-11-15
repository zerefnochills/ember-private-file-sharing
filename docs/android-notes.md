# Android development notes

## CURRENT STATE

- Milestone in progress: M10 Android send/receive completeness.
- Done: M9 is complete/tagged. M10 now includes streaming file and text transfers, protected links, bounded PoW, HTTPS relay retries, QR generation/scanning, explicit burn confirmation, progress, private temporary receive output, document pickers, and privacy flags. Shared links arrive through Android's text share intent; users can also paste or scan them.
- Done: core streaming API is merged from `main` and exposed through Rust-owned UniFFI handles. End-to-end fake-relay tests cover multi-part file roundtrip, tampering, retry, burn claim, metadata authentication, and temp cleanup. App startup wipes stale `ember-verified-*` cache files.
- Validation: full `scripts/check-android.sh` passed before the latest test-only intent update: 10 Rust binding tests, 20 Android unit tests, 5 API 34 instrumentation tests, lint, three ABI builds, dependency scan, APK 27,784,568 bytes (26.50 MiB). Then `:app:connectedDebugAndroidTest` passed again with the share-intent resolution test.
- Half-done: M10 guide still asks for metadata-stripping feedback, weak-network/resume behavior, and Android↔web/CLI end-to-end testing. Existing part-level retry is tested; real relay/document-provider/camera checks remain unavailable. Generic `ACTION_VIEW` link association is not registered because relay hosts are user-selected and a broad HTTPS handler would capture unrelated links; paste, scan, and share intents work.
- Exact next step: expose `ember_core::strip_metadata` through a bounded-memory Android flow and report exactly what was removed or why it was not stripped; then test the send confirmation UX. Known failing tests: none.

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
