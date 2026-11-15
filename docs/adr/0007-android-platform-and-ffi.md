# ADR 0007: Android minimum API and Rust bridge

- Status: accepted
- Date: 2026-11-15
- Deciders: Ember maintainers

## Context

M9 introduces the first Android app module and Kotlin access to `ember-core`. The app must support low-end devices, avoid Google Play Services, and use UniFFI rather than duplicating cryptographic behavior in Kotlin. The blueprint already requires Android 13+ nearby-Wi-Fi permissions for the later Nearby milestone, but that does not require raising the base online app minimum.

## Decision

- Set the initial minimum Android API to 26 (Android 8.0). This keeps a broad OS range and is also the API floor needed by the planned `LocalOnlyHotspot` flow in M14.
- Use a small UniFFI `cdylib` crate in `android-bindings/` as the Kotlin boundary. Keep key bytes inside Rust-owned key handles so `SecretKey` retains its existing zeroization-on-drop behavior.
- Use UniFFI 0.32.2 as a pinned M9 starting version; update only with generated-binding and Android ABI regression checks.
- Start the Android project at API 26, compile against API 37, and use AGP 9.5.0, Gradle 9.6.0, Kotlin 2.4.10, and the Compose BOM 2026.09.00. These are fixed versions, not dynamic selectors.
- Do not add Google Play Services dependencies. Validate this with an Android dependency allow-list in CI (`android/scripts/check_dependencies.ps1` and `.sh`).
- Keep the first exposed operations focused on core cryptographic operations: key generation, key import from raw bytes, text encryption/decryption, file drop encryption/decryption, and core version retrieval. Network transfer, file picker integrations, reporting, and threads follow in subsequent milestones.
- Cross-compile for `arm64-v8a` (`aarch64-linux-android`), `armeabi-v7a` (`armv7-linux-androideabi`), and `x86_64` (`x86_64-linux-android`) using NDK 28 LLVM toolchain. Target libraries are placed in `android/app/src/main/jniLibs/` and remain compact (< 1 MB per ABI).

## Consequences

- Zero Google Play Services, Firebase, FCM, or ML Kit dependencies are introduced.
- Safe Rust boundaries: `ember-core` retains `#![forbid(unsafe_code)]`, with UniFFI FFI glue contained exclusively in `android-bindings/`.
- Shared drop test vectors (`testvectors/drop-v1.json`) can be verified from both Rust and Kotlin test suites.
- Host and device build reproducibility is documented in `android/README.md` and automated via `scripts/check-android.sh`.
