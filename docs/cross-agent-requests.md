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
* **Backend:** Streaming API, Noise Protocol ephemeral thread engine (`snow`, ADR 0008), PoW mailbox creation (O2), and CLI text drops (A-3) implemented. All workspace tests and vectors verified.
* **Public Core API Reference:** Detailed in [`core/README.md`](../core/README.md).
* **Pending Cross-Agent Requests:** None.

---

### A-3 — CLI text drops sender and receiver interop

- **Requested By:** Android Agent (Codex) / Coordination Plan
- **Status:** **COMPLETED** (available on `cli-text-and-ci`)
- **Summary for Android Agent (Codex):**
  1. **CLI Commands Added:**
     - `ember send --text "hello"`: sends an inline text drop.
     - `ember send --text -`: reads text from standard input (stdin) until EOF.
     - Supports all standard flags: `--relay <URL>`, `--passphrase <PW>`, `--burn`, and `--ttl <HOURS>`.
     - `ember receive <link>`: automatically inspects `metadata.kind`. When `kind == "text"`, writes decoded UTF-8 text directly to standard output (or to `--output <file>` if specified). Existing file drops continue saving to disk as normal.
     - `ember decrypt <link-or-file>`: offline decryption also automatically supports text drops to stdout or output file.
  2. **Security & Privacy:**
     - Link fragment and passphrase are never printed or logged to stdout/stderr.
  3. **Interop Usage for Android QA:**
     - Codex/Android can now run "CLI sends text, app receives it" and "app sends text, CLI receives it" end-to-end against a relay server.

---

---

### A-2 — Noise protocol thread handshake and mailbox PoW (Review items O1 & O2)

- **Status:** **COMPLETED** (available on `integration`, following ADR 0008)
- **Summary for Android Agent (M11 preparation):**
  1. **Noise Protocol Handshake:**
     - Pattern: `Noise_NNpsk2_25519_ChaChaPoly_BLAKE2s` using the `snow` crate (version 0.10.0).
     - Handshake messages are 49 bytes with version prefix `0x02`.
     - `HandshakeInitiator::new(&drop_key)` -> `init.handshake_message()` (49 bytes).
     - `HandshakeResponder::respond(&drop_key, init_msg)` -> `(resp_msg: Vec<u8> (49 bytes), resp_keys: HandshakeSessionKeys)`.
     - `init.complete(&resp_msg)` -> `init_keys: HandshakeSessionKeys`.
  2. **Directional Mailboxes:**
     - `init_keys.mailbox_out` and `init_keys.mailbox_in` (and matching responder mailboxes) derived from shared 32-byte handshake hash $h$.
  3. **Mailbox Creation PoW Requirement:**
     - `POST /v1/mailboxes` requires PoW solution matching M4: either in `CreateMailboxRequest.pow` (`{ challenge, nonce }`) or HTTP headers `X-Pow-Challenge` and `X-Pow-Nonce`.
  4. **Binary Transport Message Envelope (Version 3):**
     - Binary wire format: `v(1 byte = 0x03) | seq(8 bytes, big-endian) | ciphertext(variable)`
     - Encapsulated in `EncryptedMessagePayload`:
       - `to_bytes(&self) -> Vec<u8>`
       - `from_bytes(&[u8]) -> Result<EncryptedMessagePayload, EmberError>`
     - Enforces 4096-byte text cap (`MAX_THREAD_TEXT_BYTES`) and ciphertext bounds check before allocation.
     - Sequence policy: accepts `seq >= expected_seq` and advances state to `seq + 1`; strictly rejects `seq < expected_seq` as replay.
  5. **Core API Signatures:**
     - `ThreadSession::new(handle, keys)`
     - `session.encrypt_text(&str) -> Result<EncryptedMessagePayload, EmberError>`
     - `session.decrypt_message(&EncryptedMessagePayload) -> Result<ThreadMessage, EmberError>`
     - `session.decrypt_message_bytes(&[u8]) -> Result<ThreadMessage, EmberError>`
     - All public APIs are fully backward compatible with prior M7 interface conventions.

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
