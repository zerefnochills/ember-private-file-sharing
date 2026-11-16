# Ember Security Audit Scope & Verification Package

This document outlines the scope, cryptographic boundaries, architectural invariants, and verification instructions prepared for independent third-party security auditors evaluating the Ember codebase.

---

## 1. System Overview & Core Invariants

Ember is an account-free, end-to-end encrypted file and message drop system. The primary security goal is ensuring that neither the relay server operator, network intermediaries, nor compromised cloud infrastructure can decrypt dropped files or messages.

### Critical Security Invariants:
1. **Server Blindness:** Cryptographic keys and URL link fragments (`#<payload>`) MUST never reach the relay server over the network or be written to disk/logs.
2. **Deterministic Cryptographic Primitives:** Strict reliance on standard libsodium-compatible primitives (`dryoc` v2.0):
   * Stream Cipher: `crypto_secretstream_xchacha20poly1305` (1 MiB chunks with final message tag `TAG_FINAL`).
   * Key Wrapping: Argon2id (64 MiB memory, 3 iterations) + XChaCha20-Poly1305.
   * Session-Level Forward-Secret Transport: Ephemeral X25519 DH authenticated with drop PSK (Noise NNpsk2 via snow) with session-level transport encryption.
   * Proof-of-Work: SHA-256 hashcash challenges signed with server HMAC.
3. **No Secret Logging:** Server access logs record only coarse metrics (route templates, response size buckets `<1KB`..`>50MB`, no resource IDs, no plaintexts, no keys).
4. **Hard Memory Safety:** Cryptographic crates enforce `#![forbid(unsafe_code)]`. All secret keys implement `ZeroizeOnDrop` and constant-time comparisons (`subtle::ConstantTimeEq`).

---

## 2. In-Scope Components

| Component / Crate | Path | Primary Language | Critical Audit Areas |
|---|---|---|---|
| **Ember Core Engine** | `core/src/` | Pure Rust | `key.rs`, `stream.rs`, `wrap.rs`, `pow.rs`, `strip.rs`, `thread/*.rs` |
| **Protocol Definition** | `protocol/src/` | Rust | Wire formats, chunk layouts, cryptographic constants, bounds |
| **Relay Server** | `server/src/` | Rust (Axum) | `routes.rs`, `storage/*.rs`, `access_log.rs`, `pow.rs`, `rate_limit.rs`, `sweeper.rs` |
| **Admin & Client Tools** | `tools/` | Rust | `cli/src/main.rs`, `admin-cli/src/main.rs` |
| **UniFFI Native Bridge** | `android/ember-ffi/` | Rust / C-ABI | `src/lib.rs` (safe FFI boundaries, zeroization across foreign boundary) |
| **Web Receiver** | `web-receiver/` | HTML / JS / WebCrypto | Static single-page decryption harness, bot preview isolation |

---

## 3. Out-of-Scope Items
* Operating system kernel security and hardware attacks (cold boot, electron microscopy).
* Compromised recipient endpoints (an authentic recipient can choose to retain or export decrypted files).
* Cellular infrastructure denial-of-service (carrier-level IP blocking).

---

## 4. Key Documentation References
* **Protocol Specification:** [`docs/protocol-spec.md`](../protocol-spec.md) (authoritative wire framing and crypto layouts).
* **Threat Model:** [`docs/threat-model.md`](../threat-model.md) (assets, trust boundaries, 18 evaluated threat vectors).
* **Logging & Privacy Policy:** [`docs/privacy-and-logging.md`](../privacy-and-logging.md) (server field declarations).
* **Moderation & Takedowns:** [`docs/takedown-process.md`](../takedown-process.md) (technical deletion workflows).

---

## 5. Auditor Verification Instructions

### 5.1 Environment Prerequisites
* Rust toolchain >= 1.89 (currently verified on Rust 1.100.0).
* Standard `cargo` build environment.

### 5.2 Compiling & Running Automated Invariant Tests
```bash
# 1. Format and strict lint check
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings

# 2. Complete test suite execution (including property tests, tamper tests, and privacy tests)
cargo test --workspace

# 3. Dedicated shared test vectors verification
cargo test -p ember-core --test test_vectors
cargo test -p ember-core --test thread_vectors

# 4. Access log privacy canary tests
cargo test -p ember-server --test access_log_tests
```

### 5.3 Test Vectors
Auditors should verify all vectors in `testvectors/drop-v1.json` and `testvectors/thread-v1.json`. Every vector must decrypt cleanly, and all corrupted/truncated/reordered negative test cases must fail authentication.
