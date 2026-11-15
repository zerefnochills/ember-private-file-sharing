# Reproducible Builds and Verification Guide

This guide documents how third parties, researchers, and users can independently compile and verify bit-for-bit reproducible releases of the Ember ecosystem, including the Rust binaries (`ember`, `ember-server`, `ember-admin`) and the Android APK.

---

## 1. Verification Principles

1. **Deterministic Compiler Toolchain:** Pinned compiler versions ensure that AST lowerings, optimizations, and metadata generation produce bitwise identical object files.
2. **Normalized Metadata & Timestamps:** Embedded build timestamps are clamped using the standard `SOURCE_DATE_EPOCH` environment variable.
3. **Hermetic Environment:** Builds are executed without network access during the compilation phase, relying exclusively on pre-fetched, hash-checked dependencies (`Cargo.lock` and Gradle checksum verification).

---

## 2. Verifying Rust Binaries (`ember`, `ember-server`, `ember-admin`)

### 2.1 Pinned Environment
* **Rust Toolchain:** `rustc 1.100.0` or later (as indicated in Cargo workspace).
* **Target:** `x86_64-unknown-linux-gnu` (Linux release target) or host target.
* **Environment Configuration:**
  ```bash
  export SOURCE_DATE_EPOCH=$(git log -1 --pretty=%ct)
  export CARGO_HOME="$HOME/.cargo"
  export RUSTFLAGS="--remap-path-prefix=$PWD=/build"
  ```

### 2.2 Compilation Step
```bash
# Verify checksums of dependencies
cargo check --locked

# Compile release binaries with path remapping and LTO
cargo build --release --locked --bins
```

### 2.3 Checksum Generation
```bash
sha256sum target/release/ember target/release/ember-server target/release/ember-admin > SHA256SUMS.txt
```

---

## 3. Verifying the Android APK (`com.ember.mobile`)

### 3.1 Pinned Android Toolchain
* **Java Development Kit (JDK):** OpenJDK 17.0.10+
* **Android SDK:** Platform 35, Build-Tools 35.0.0
* **Android NDK:** r28 (28.0.12433566)
* **Gradle:** 9.6.0 via official wrapper
* **Android Gradle Plugin (AGP):** 9.5.0
* **Kotlin:** 2.4.10

### 3.2 Compilation Step

1. **Build Native Shared Libraries (`libember_android_ffi.so`):**
   ```bash
   cd android
   # Windows:
   powershell -ExecutionPolicy Bypass -File ./build-native.ps1
   # Linux / macOS:
   ./build-native.sh
   ```
   This compiles native binaries for `arm64-v8a`, `armeabi-v7a`, and `x86_64` and places them into `app/src/main/jniLibs/`.

2. **Assemble Unsigned APK:**
   ```bash
   ./gradlew assembleRelease --no-daemon -Dorg.gradle.project.android.enableR8=true
   ```

### 3.3 Verifying APK Reproducibility (`diffoscope` and `apkdiff`)
To verify an APK downloaded from F-Droid or GitHub Releases against your local build:

```bash
# Using apksig / apkdiff to verify code and resource equivalence
apkdiff official-release.apk app/build/outputs/apk/release/app-release-unsigned.apk

# Inspecting internal differences (excluding signatures):
diffoscope official-release.apk app/build/outputs/apk/release/app-release-unsigned.apk
```
Any divergence in output must be traceable exclusively to APK signing blocks (`META-INF/*.RSA` and `META-INF/*.SF`).
