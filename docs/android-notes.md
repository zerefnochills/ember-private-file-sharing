# Android development notes

## CURRENT STATE

- Milestone in progress: M10 API integration groundwork.
- Done: M9 is complete. M10 has Rust-side UniFFI helpers for protected/unprotected link build/open without returning raw keys, host/blob ID inspection, and bounded PoW solving. The generated Kotlin methods were exercised on API 34, including Argon2id link wrapping/unwrapping. Latest full gate passed: 8 Rust binding tests, 7 Android unit tests, 3 emulator tests, all 13 vectors, lint, three ABI builds, dependency check, and APK build (21,313,267 bytes / 20.33 MiB).
- Half-done: M10 app screens and full transfer acceptance remain incomplete. Implemented Compose Send/Receive navigation, HTTPS relay host input, shared-link intake, one-view confirmation, send-link chooser, and a generated QR containing the complete link fragment. Added `TextDropTransfer` for <=1 MiB text only with authenticated metadata checks, claim/finish sequencing, and buffer wiping. QR camera scanning and network-level text-flow tests remain unavailable. The backend coordination log reports A-1 implemented/tagged on main; this Android worktree has not merged it yet, pending the explicit handoff instruction. Full `scripts/check-android.sh` passed: 9 Rust binding tests, 14 Android unit tests, 3 API 34 instrumentation tests, lint, all 3 native ABIs, dependency scan, APK 25,613,206 bytes (24.43 MiB).
- Half-done: added injected relay connections and two orchestration tests. Burn text now uploads, claims, downloads in order, decrypts and checks metadata, finishes, and returns plaintext; tampering fails authentication and never calls finish. `:app:testDebugUnitTest` passes (16 tests total). These are deterministic in-memory transport tests; a deployed relay has not been exercised.
- Exact next step: add QR scanning. Merge `main` and bind file streaming only when instructed. Known failing tests: none.
- Known failing tests: none. M10 acceptance remains pending, including link/PoW UI, end-to-end transfer, weak-network recovery, and bounded-memory streaming. The final M9 APK before M10 additions was 19,779,333 bytes (18.86 MiB).

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
