# Android M9 starter

This directory is the Android entry point for the M9 Rust-core integration. The Rust bridge lives in `ember-ffi/`; the `app/` module contains the first Compose debug screen and Gradle project. Native compilation and generated-binding checks require the Android SDK, NDK, Gradle and `cargo-ndk`.

## Host-side bridge check

From the repository root, run:

```powershell
cargo test -p ember-android-ffi
```

This checks the Kotlin-facing bridge's Rust API and exercises encryption/decryption through the same core format. It does not prove that Kotlin can load the native library on an Android device.

## Device build prerequisites

- JDK 17 or newer
- Android SDK and NDK
- Gradle wrapper
- `cargo-ndk`
- Rust targets `aarch64-linux-android`, `armv7-linux-androideabi`, and `x86_64-linux-android`

The current development environment has Java but does not have Gradle, `adb`, or `cargo-ndk`. Install `cargo-ndk` and the three Android Rust targets, then run `android/build-native.ps1` to build all three ABIs and generate Kotlin bindings using the pinned UniFFI CLI. Complete the forbidden-dependency CI check and device/emulator test-vector run after installing the Android toolchain.

See `docs/adr/0007-android-platform-and-ffi.md` for the initial platform choices and constraints.
