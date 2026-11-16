# Ember Core (`ember-core`)

The reference cryptographic and protocol engine for the Ember private file drop ecosystem.

`ember-core` is a pure-Rust library enforcing `#![forbid(unsafe_code)]`. It implements the authoritative wire format defined in [`docs/protocol-spec.md`](../docs/protocol-spec.md) and strict zeroization of cryptographic material.

## Guarantees & Cryptographic Primitives

- **Pure Libsodium Compatibility**: Uses `dryoc` primitives matching libsodium without custom ciphers:
  - Stream Cipher: `crypto_secretstream_xchacha20poly1305`
  - Key Derivation: `crypto_kdf_derive_from_key` with context `EMBERv1_` (8 bytes)
  - Key Wrapping: Argon2id (`pwhash`) + `crypto_aead_xchacha20poly1305_ietf` (95-byte payload)
  - Diffie-Hellman: X25519 authenticated with drop PSK via BLAKE2b keyed hashing
  - Proof of Work: SHA-256 hashcash
- **Zero Plaintext / Secret Logging**: `SecretKey` explicitly suppresses key bytes in `fmt::Debug` (`SecretKey([REDACTED])`), implements `zeroize::ZeroizeOnDrop`, and performs comparisons in constant time via `subtle::ConstantTimeEq`.
- **Pre-encryption Metadata Stripping**: Strips EXIF/GPS/XMP from JPEG, PNG, and WebP, and sanitizes PDF metadata while preserving xref offsets.

---

## Public API Reference

### 1. Key Management (`ember_core::key`)

```rust
use ember_core::SecretKey;

// Generate 32-byte CSPRNG secret key
let key = SecretKey::generate();

// Import from existing 32-byte array
let key = SecretKey::from_bytes(key_bytes);

// Derivation of sub-keys
let content_key = key.derive_content_key()?;
let thread_psk = key.derive_thread_psk()?;
```

### 2. Passphrase Key Wrapping (`ember_core::wrap`)

Protects the drop key with a user-chosen passphrase using Argon2id and XChaCha20-Poly1305:

```rust
use ember_core::{wrap_key, unwrap_key, WrapParams};

// Wrap 32-byte key into a 95-byte payload
let wrapped = wrap_key(&key, "correct horse battery staple", WrapParams::default())?;

// Unwrap back to SecretKey
let unwrapped_key = unwrap_key(&wrapped, "correct horse battery staple")?;
```

### 3. Metadata & Padding (`ember_core::metadata`, `ember_core::padding`)

Message 0 is an authenticated 4096-byte record with zero-padding:

```rust
use ember_core::DropMetadata;

// File drop
let meta = DropMetadata::new_file(
    "report.pdf",
    "application/pdf",
    content_len,
    true,        // burn-after-view
    created_at,  // unix timestamp seconds
    expires_at,  // unix timestamp seconds
);

// Text drop
let text_meta = DropMetadata::new_text(content_len, false, created_at, expires_at);
```

### 4. Incremental Streaming & Whole-Buffer API (`ember_core::stream`)

`ember-core` provides bounded-memory incremental streaming for both encryption and decryption, designed specifically for low-memory devices (e.g. mobile phones) handling large drops:

#### Incremental Encryption (`DropEncryptStream`)
Buffers at most **1 MiB** of plaintext in memory regardless of file size:

```rust
use ember_core::{start_encrypt_stream, DropMetadata, SecretKey};

// 1. Initialize stream: encrypts Message 0 and produces Part 0 (4137 bytes)
let (mut stream, part0) = start_encrypt_stream(&key, &metadata)?;
// Upload part0 to relay server...

// 2. Feed arbitrary plaintext slices (e.g. read from an InputStream or file buffer)
// Emits 1 MiB ciphertext parts as soon as full chunks accumulate
while let Ok(n) = file.read(&mut buf) {
    if n == 0 { break; }
    let parts = stream.update(&buf[..n])?;
    for part in parts {
        // Upload 1,048,593-byte part to relay server...
    }
}

// 3. Finalize: applies spec zero-padding and encrypts final chunk with TAG_FINAL
let final_parts = stream.finish()?;
for part in final_parts {
    // Upload final part...
}
```

#### Incremental Decryption (`DropDecryptStream`)
Processes incoming parts one by one with strict bounded memory:

```rust
use ember_core::{start_decrypt_stream, SecretKey};

// 1. Initialize with Part 0: verifies stream header and authenticates Message 0
let (mut stream, metadata) = start_decrypt_stream(&key, &part0)?;

// 2. Push each transport part (Part 1..n) as it is downloaded
for part in downloaded_parts {
    let (plain_chunk, is_final) = stream.push_part(&part)?;
    // Write plain_chunk to an app-private temporary file...
}

// 3. Complete and enforce all protocol spec receiver checks
stream.finish()?; // MUST succeed before moving/renaming temporary file to user destination!
```

> **CRITICAL SECURITY REQUIREMENT (Protocol Spec Section 4.3):**
> Callers writing decrypted chunks to disk MUST write them to an app-private temporary file. Callers MUST NOT expose or treat plaintext as valid until `stream.finish()` returns `Ok(())`. If any part fails authentication, padding verification, or length checks, or if `stream.finish()` fails (e.g. missing `FINAL` tag, truncated stream, trailing parts), callers MUST securely delete the temporary file.

#### Whole-Buffer Compatibility Functions
Convenient wrappers built directly on top of the streaming engine:

```rust
use ember_core::{encrypt_drop, encrypt_drop_to_parts, decrypt_drop, decrypt_drop_parts};

// Full stream encrypt
let ciphertext = encrypt_drop(&key, &meta, plaintext_bytes)?;

// Resumable chunked parts (Part 0 = 4,137 B; Chunk parts = 1,048,593 B)
let parts = encrypt_drop_to_parts(&key, &meta, plaintext_bytes)?;

// Decrypt
let (meta, plaintext) = decrypt_drop(&key, &ciphertext)?;
let (meta, plaintext) = decrypt_drop_parts(&key, &parts)?;
```

### 5. Links & Addressing (`ember_core::link`)

```rust
use ember_core::{generate_blob_id, build_link, parse_link, LinkPayload};

// 128-bit random URL-safe base64 ID
let blob_id = generate_blob_id();

// Build drop link: https://relay.example.com/d/<blob_id>#<payload>
let link = build_link("relay.example.com", &blob_id, &LinkPayload::Unprotected(key.clone()))?;

// Parse drop link (extracts host, blob_id, and payload without leaking to network)
let parsed = parse_link(&link)?;
```

### 6. Proof of Work (`ember_core::pow`)

WASM-ready SHA-256 hashcash solver and challenge verification:

```rust
use ember_core::{solve_pow, verify_pow, count_leading_zero_bits};

let nonce = solve_pow(&challenge_bytes, difficulty);
let valid = verify_pow(&challenge_bytes, nonce, difficulty);
```

### 7. Metadata Stripping (`ember_core::strip`)

```rust
use ember_core::{strip_metadata, detect_format, FileFormat, StripStatus};

let result = strip_metadata(&raw_file_bytes);
if result.status == StripStatus::Stripped {
    println!("Stripped {} fields from {:?}", result.fields_removed.len(), result.format);
    let clean_bytes = result.data;
}
```

### 8. Ephemeral 1:1 Threads (`ember_core::thread`)

Noise-inspired ephemeral key exchange and forward-secret symmetric ratchets:

```rust
use ember_core::thread::{
    generate_handle, HandshakeInitiator, HandshakeResponder, ThreadSession,
};

// Local-only pseudonymous handles (never sent to server)
let handle = generate_handle(); // e.g. "amber-fox-4821"

// Initiator creates handshake message to post to drop rendezvous
let (initiator, init_msg) = HandshakeInitiator::new(&drop_key)?;

// Responder processes initiator message and creates response
let (responder, resp_msg, session_keys_resp) = HandshakeResponder::respond(&drop_key, &init_msg)?;

// Initiator completes handshake
let session_keys_init = initiator.finalize(&resp_msg)?;

// Both sides instantiate ThreadSession for ratchet encryption
let mut session_alice = ThreadSession::from_initiator(session_keys_init);
let mut session_bob = ThreadSession::from_responder(session_keys_resp);

// Forward-secret ratchet message encryption
let envelope = session_alice.send(b"Hello Bob!")?;
let decrypted = session_bob.receive(&envelope)?;
assert_eq!(decrypted, b"Hello Bob!");
```
