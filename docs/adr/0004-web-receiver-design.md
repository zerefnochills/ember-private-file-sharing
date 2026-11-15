# ADR 0004: Web Receiver Architecture and Client-Side Decryption

- Status: accepted
- Date: 2026-11-15
- Deciders: Antigravity

## Context
Milestone M5 requires a minimal, receive-only web page so any recipient can open an Ember share link in a modern web browser without installing dedicated client software.

Key requirements and constraints:
1. Strict adherence to `docs/protocol-spec.md` wire formats and `docs/threat-model.md` trust boundaries (specifically Threat T3a: the web receiver inherently trusts the relay server host delivering the JavaScript).
2. Client-side decryption: Master keys in URL hash fragments (`#...`) are never transmitted over the network (RFC 3986 enforces that fragments are not sent in HTTP request lines).
3. Cryptographic consistency: The implementation must pass all test vectors in `testvectors/drop-v1.json`, including Argon2id key unwrapping, BLAKE2b sub-key derivation, and XChaCha20-Poly1305 secretstream decryption with authenticated headers and additional authenticated data (`ember-drop-v1`).
4. Strict security posture: Zero external dependencies, no CDN, no third-party scripts, strict Content Security Policy (`default-src 'none'`), and explicit security warnings.
5. Bot protection: Bot previews (plain `GET /v1/blobs/{id}` or `GET /d/{id}`) must never trigger burn-after-view deletion; drops are only burned when an explicit `POST /v1/blobs/{id}/claim` is followed by `POST /v1/blobs/{id}/finish`.

## Options Considered

1. **Option A: Compile Rust `ember-core` to WebAssembly (`wasm32-unknown-unknown` via `wasm-pack` / `wasm-bindgen`).**
   - *Pros:* Code sharing with the core Rust crate; guarantees matching implementation logic.
   - *Cons:* Compiling `dryoc` and its dependencies to WASM requires additional wasm tooling in CI; binary size for wasm glue and runtime is larger.

2. **Option B: Pure JavaScript using `libsodium.js` (`libsodium-wrappers-sumo`).**
   - *Pros:* Official, heavily audited WebAssembly build of libsodium. Exposes identical primitives matching dryoc's libsodium compatibility layer (`crypto_secretstream_xchacha20poly1305`, `crypto_pwhash` Argon2id13, `crypto_aead_xchacha20poly1305_ietf`, `crypto_kdf_derive_from_key`). Fast, stable, and tested directly against shared test vectors.
   - *Cons:* Maintained as an independent JavaScript implementation requiring thorough vector validation.

## Decision

We chose **Option B (`libsodium-wrappers-sumo`) bundled via `esbuild`**:
- The cryptography module (`web-receiver/crypto.js`) directly implements link parsing, Argon2id unwrapping, BLAKE2b sub-key derivation, and secretstream chunk pull operations.
- All 14 test vector test cases (8 decrypt cases, 1 passphrase wrap case, 5 negative tamper cases) in `testvectors/drop-v1.json` pass with zero failures.
- The web receiver frontend (`index.html`, `style.css`, `app.js`) is statically compiled and directly embedded into `ember-server` via `include_str!` and `include_bytes!`, making the server binary completely self-contained.
- The server serves the app on `GET /` and `GET /d/{id}` with strict HTTP security headers:
  - `Content-Security-Policy: default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; connect-src 'self'; img-src 'self' data: blob:;`
  - `X-Content-Type-Options: nosniff`
  - `X-Frame-Options: DENY`
  - `Referrer-Policy: no-referrer`
  - `Cache-Control: no-store` (for HTML)

## Consequences
- The web receiver is fully standalone and operates with zero third-party assets.
- Explicit warnings are displayed to users regarding threat T3a (host trust) and the retention rights of recipients.
- Independent auditors can inspect `web-receiver/crypto.js` alongside `testvectors/drop-v1.json` to verify cryptographic compliance.
