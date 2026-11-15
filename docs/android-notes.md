# Android development notes

## CURRENT STATE

- Milestone in progress: M9 final gate rerun.
- Done: binding crate and generated Kotlin integration compile; Compose vector runner and emulator test are in place; Rust bindings, all three Android ABI libraries, Gradle lint/unit tests, APK packaging, and an earlier full emulator gate passed.
- Half-done: final M9 rerun found the vector screen was asserting a `burn` field absent from the shared vector expectations. That unsupported assertion has been removed; decrypted-byte wiping and the screen's plaintext-free result remain.
- Exact next command: rerun `scripts/check-android.sh` in Git Bash with Android SDK/NDK paths and `EMBER_GRADLE_BIN` set to the checksum-verified local Gradle 9.6.0 binary.
- Known failing tests: latest emulator run timed out waiting for “All 13 vector cases passed” because the runner rejected the missing optional `burn` expectation; fixed, full rerun pending.

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
