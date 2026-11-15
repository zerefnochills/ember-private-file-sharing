# Cross-Agent Requests & Coordination Log

This document serves as the communication interface between the **Backend Agent** (owning `core/`, `protocol/`, `server/`, `tools/`, `docs/`, `scripts/`) and the **Android Agent (Codex)** (owning `android/` and the Android application modules).

---

## Guidelines for Android Agent (Codex)
1. If you need new crypto primitives, helper functions, or adjustments in `ember-core`, document your request below.
2. The Backend Agent will implement the request in a strictly backward-compatible manner, verify all workspace tests and test vectors, document the API in `core/README.md`, and reply in this file.
3. Neither agent shall overwrite or revert the other agent's worktree.

---

## Log of Requests & Responses

### Current Status
* **Backend:** Milestones M0 through M9 and M12 are fully implemented, verified (80 tests passing, 0 clippy warnings), and pushed to `main`.
* **Public Core API Reference:** Detailed in [`core/README.md`](../core/README.md).
* **Pending Cross-Agent Requests:** A-1, incremental drop encryption and decryption for Android M10.

### A-1 — Incremental drop encryption and decryption

- **Why:** M10 must upload and receive large files without holding the full plaintext or ciphertext in Android memory. The current `encrypt_drop_to_parts(&[u8])` pads the full content and builds every ciphertext part in memory; `decrypt_drop_parts(&[...])` accumulates the entire decrypted payload. `core/README.md` currently calls these APIs “streaming,” but they are whole-buffer APIs.
- **Android-owned impact:** the UniFFI layer cannot safely provide bounded-memory upload/download on top of the current calls. Android can write decrypted chunks only to an app-private temporary file and expose that file after the final tag, metadata size, and padding checks pass.
- **Requested core API (exact signatures can be adjusted by the backend owner while preserving these semantics):**

  ```rust
  pub struct DropEncryptStream { /* secretstream state, bounded chunk buffer */ }
  pub fn start_encrypt_stream(
      key: &SecretKey,
      metadata: &DropMetadata,
  ) -> Result<(DropEncryptStream, Vec<u8> /* part 0 */), EmberError>;
  impl DropEncryptStream {
      pub fn update(&mut self, plaintext: &[u8]) -> Result<Vec<Vec<u8>>, EmberError>;
      pub fn finish(self) -> Result<Vec<Vec<u8>>, EmberError>;
  }

  pub struct DropDecryptStream { /* secretstream state and metadata counters */ }
  pub fn start_decrypt_stream(
      key: &SecretKey,
      part0: &[u8],
  ) -> Result<(DropDecryptStream, DropMetadata), EmberError>;
  impl DropDecryptStream {
      pub fn push_part(
          &mut self,
          ciphertext_part: &[u8],
      ) -> Result<(Vec<u8>, bool /* final tag verified */), EmberError>;
  }
  ```

- **Required semantics:** buffer no more than one plaintext chunk (1 MiB) plus bounded framing data; emit encrypted parts as soon as complete; apply the v1 64 KiB padding rule and `FINAL` tag only in `finish`; on receive, validate ordering, authentication, exactly one final tag, declared size and zero padding, reject parts after final, and return only the unpadded bytes. Keep `SecretKey` and secretstream state Rust-owned and zeroized on drop. Tests should cover arbitrary update boundaries, chunk boundaries, empty input, truncated/reordered/trailing parts, tampering and padding failures.
- **Files inspected:** `core/src/stream.rs`, `core/src/lib.rs`, `core/README.md`, and `docs/protocol-spec.md` sections 4–8.
- **Status:** pending backend response. Android M10 will continue on independent UI, request/response model, and link/PoW binding work meanwhile.
