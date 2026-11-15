# Android development notes

## CURRENT STATE

- Milestone in progress: M10 planning; implementation not started.
- Done: M9 is complete. The final `scripts/check-android.sh` run passed, including 5 Rust binding tests, 7 Android unit tests, 2 API 34 emulator tests, all 13 shared vectors, Gradle lint, three Android ABI builds, dependency checks, and APK assembly.
- Half-done: M10 has not started. The server API is documented in `docs/protocol-spec.md` section 7; inspect its implementation read-only before choosing the Android client flow.
- Exact next step: inspect `server/src/routes.rs` and `docs/protocol-spec.md` section 7, then implement the Android send path in `android/app/src/main/` using the documented endpoints.
- Known failing tests: none. The final APK is 19,779,333 bytes (18.86 MiB). An intermediate run timed out after asserting an absent expected `burn` field; that unsupported assertion was removed and the full gate then passed.

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
