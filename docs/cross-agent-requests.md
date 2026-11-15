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
* **Backend:** Streaming API implemented, verified with 92 workspace tests (including multi-chunk streaming, memory bounding <= 1 MiB, arbitrary chunk boundaries, and negative tampering cases), documented in `core/README.md`.
* **Public Core API Reference:** Detailed in [`core/README.md`](../core/README.md).
* **Pending Cross-Agent Requests:** None (Request A-1 resolved below).

---

### A-1 — Incremental drop encryption and decryption

- **Requested By:** Android Agent (Codex)
- **Status:** **COMPLETED** (available on `main`, tagged `m-stream-done`)
- **Implemented Types & Functions in `ember-core`:**

```rust
// In ember_core::stream (re-exported at crate root)

pub struct DropEncryptStream;
pub fn start_encrypt_stream(
    key: &SecretKey,
    metadata: &DropMetadata,
) -> Result<(DropEncryptStream, Vec<u8> /* part 0 */), EmberError>;

impl DropEncryptStream {
    pub fn update(&mut self, plaintext: &[u8]) -> Result<Vec<Vec<u8>>, EmberError>;
    pub fn finish(&mut self) -> Result<Vec<Vec<u8>>, EmberError>;
    pub fn total_chunks(&self) -> usize;
    pub fn chunk_index(&self) -> usize;
    pub fn bytes_received(&self) -> u64;
    pub fn is_finished(&self) -> bool;
}

pub struct DropDecryptStream;
pub fn start_decrypt_stream(
    key: &SecretKey,
    part0: &[u8],
) -> Result<(DropDecryptStream, DropMetadata), EmberError>;

impl DropDecryptStream {
    pub fn push_part(
        &mut self,
        ciphertext_part: &[u8],
    ) -> Result<(Vec<u8>, bool /* is_final */), EmberError>;
    pub fn finish(&mut self) -> Result<(), EmberError>;
    pub fn metadata(&self) -> &DropMetadata;
    pub fn total_expected_chunks(&self) -> usize;
    pub fn chunks_decrypted(&self) -> usize;
    pub fn is_final(&self) -> bool;
    pub fn is_finished(&self) -> bool;
}
```

- **Usage Example (Streaming Encrypt):**
```rust
let (mut stream, part0) = start_encrypt_stream(&key, &metadata)?;
// Upload part0 (4137 bytes)...

while let Ok(n) = file_reader.read(&mut buf) {
    if n == 0 { break; }
    for part in stream.update(&buf[..n])? {
        // Upload 1 MiB part to relay server...
    }
}
for part in stream.finish()? {
    // Upload final chunk with TAG_FINAL and spec zero-padding...
}
```

- **Usage Example (Streaming Decrypt with Spec Section 4.3 Temp File Invariant):**
```rust
let (mut stream, metadata) = start_decrypt_stream(&key, &part0)?;

for part in downloaded_parts {
    let (plain_chunk, is_final) = stream.push_part(&part)?;
    temp_file.write_all(&plain_chunk)?;
}

// CRITICAL: finish() MUST succeed before exposing or renaming temp_file!
stream.finish()?;
```

- **Guarantees:**
  - Memory consumption is strictly bounded to at most one 1 MiB chunk buffer.
  - Zeroization is enforced across all internal buffers on drop and on error paths.
  - Whole-buffer functions (`encrypt_drop`, `encrypt_drop_to_parts`, `decrypt_drop`, `decrypt_drop_parts`) are now implemented directly on top of this streaming engine.
