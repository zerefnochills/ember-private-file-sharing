# ADR 0002: Choice of cryptography library (dryoc)

- Status: accepted
- Date: 2026-11-15
- Deciders: Antigravity (M2), per blueprint section 3 / D6

## Context
Milestone M2 requires selecting an audited, robust cryptographic library for Ember's core implementation.
Per blueprint Decision D6 and build guide section 3:
- Never write custom cryptography.
- Choose between pure Rust libsodium-compatible crates (such as `dryoc`) or C-bound `libsodium-sys-stable`.
- The library must support:
  1. `crypto_secretstream_xchacha20poly1305` (stream encryption with chunk authentication and ordering)
  2. `crypto_kdf_derive_from_key` (BLAKE2b subkey derivation)
  3. `crypto_pwhash` (Argon2id password hashing)
  4. `crypto_aead_xchacha20poly1305_ietf` (key wrapping)
  5. Constant-time operations and zeroization of secret material
  6. Multi-platform support: native desktop (Windows MSVC/GNU, Linux, macOS), mobile (Android NDK via UniFFI in M9), and WebAssembly (WASM web receiver in M5).

## Options considered
1. **`libsodium-sys-stable`**
   - Pros: Official C libsodium codebase, universally recognized and audited.
   - Cons: Requires native C compilers, complex cross-compilation harnesses for Android NDK and WASM targets (`emscripten` / `wasm32-unknown-unknown`), non-trivial build setup on developer environments without preconfigured C toolchains.
2. **`sodiumoxide`**
   - Pros: Historical Rust wrapper for libsodium.
   - Cons: Officially deprecated and unmaintained. Explicitly forbidden by build guide section 3.
3. **`dryoc` (v2.0.0)**
   - Pros:
     - 100% pure Rust implementation compatible with libsodium wire formats and test vectors.
     - Complete support for all required primitives: `crypto_secretstream_xchacha20poly1305`, `crypto_kdf`, `crypto_pwhash` (Argon2id), `crypto_aead_xchacha20poly1305_ietf`.
     - Built on vetted cryptographic foundations (RustCrypto `subtle`, `zeroize`, `curve25519-dalek`, `digest`, `sha2`).
     - Pure Rust crate compiles seamlessly to `wasm32-unknown-unknown` for the web receiver without external C toolchains or Emscripten.
     - Integrates natively with `zeroize` for zeroization on drop.

## Decision
Adopt **`dryoc`** (v2.0.0) as the primary cryptographic foundation for `ember-core`, complemented by `subtle` for constant-time comparisons and `zeroize` for secure memory zeroization.

## Consequences
- No C compiler or libsodium native library is required for building `ember-core`.
- Cross-compilation to Android (`cargo-ndk`) and WASM (`wasm-pack`) is direct and reliable.
- Test vectors generated in `testvectors/drop-v1.json` are compatible with any standard libsodium implementation (including libsodium.js and mobile wrappers).
- The security auditor in M13 can review pure Rust code and verify against standard RFC and libsodium test vectors.
