# ADR 0007: Android minimum API and Rust bridge

- Status: proposed for M9
- Date: 2026-11-15
- Deciders: Ember maintainers

## Context

M9 introduces the first Android app module and Kotlin access to `ember-core`. The app must support low-end devices, avoid Google Play Services, and use UniFFI rather than duplicating cryptographic behavior in Kotlin. The blueprint already requires Android 13+ nearby-Wi-Fi permissions for the later Nearby milestone, but that does not require raising the base online app minimum.

## Decision

- Set the initial minimum Android API to 26 (Android 8.0). This keeps a broad OS range and is also the API floor needed by the planned `LocalOnlyHotspot` flow in M14.
- Use a small UniFFI `cdylib` crate in `android/ember-ffi` as the Kotlin boundary. Keep key bytes inside Rust-owned key handles so `SecretKey` retains its existing zeroization-on-drop behavior.
- Use UniFFI 0.32.2 as a pinned M9 starting version; update only with generated-binding and Android ABI regression checks.
- Start the Android project at API 26, compile against API 37, and use AGP 9.5.0, Gradle 9.6.0, Kotlin 2.4.10, and the Compose BOM 2026.09.00. These are fixed versions, not dynamic selectors.
- Do not add Google Play Services dependencies. Validate this with an Android dependency allow-list in CI before M9 is complete.
- Keep the first exposed operations deliberately narrow: core version and local text encryption/decryption. Network transfer, file access, reporting, and threads remain later milestones.

## Consequences and validation limits

The Rust bridge can be tested on the host, but host tests do not verify generated Kotlin bindings, Android ABI compatibility, native library loading, APK size, or device behavior. M9 remains incomplete until those checks run with Android SDK/NDK, Gradle, `cargo-ndk`, and a device or emulator.
