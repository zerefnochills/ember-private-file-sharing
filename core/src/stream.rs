//! Drop stream encryption and decryption using libsodium secretstream.
//!
//! Follows `docs/protocol-spec.md` sections 4 and 5:
//! - Uses `crypto_secretstream_xchacha20poly1305`
//! - Sub-key id 1 (`K_content`) derived from master key K
//! - 24-byte random stream header
//! - Message 0: 4096-byte metadata record, tag `MESSAGE`, additional data `AD_DROP`
//! - Messages 1..n: 1 MiB padded content chunks, intermediate tag `MESSAGE`, last tag `FINAL`, additional data `AD_DROP`
//! - Part 0: 4137 bytes (`stream_header || msg0_ciphertext`)
//! - Part 1..n: at most 1,048,593 bytes each
//!
//! Provides incremental, bounded-memory streaming encryptor ([`DropEncryptStream`])
//! and decryptor ([`DropDecryptStream`]), as well as whole-buffer compatibility wrappers.

use dryoc::classic::crypto_secretstream_xchacha20poly1305::*;
use dryoc::constants::{
    CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_TAG_FINAL,
    CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_TAG_MESSAGE,
};
use ember_protocol::{
    ABYTES, AD_DROP, CHUNK_SIZE, MAX_PREALLOC_BYTES, METADATA_PLAIN_LEN, PART0_SIZE,
    STREAM_HEADER_BYTES,
};
use zeroize::Zeroize;

use crate::error::EmberError;
use crate::key::SecretKey;
use crate::metadata::DropMetadata;
use crate::padding::calculate_padded_len;

/// Streaming drop encryptor that buffers no more than one 1 MiB chunk at a time.
///
/// Encrypts drop metadata into Part 0 upon creation, then accepts arbitrary plaintext
/// chunks via [`DropEncryptStream::update`], emitting transport parts as soon as full
/// 1 MiB chunks accumulate. Final padding (per spec section 6) and the `FINAL` stream tag
/// are applied during [`DropEncryptStream::finish`].
pub struct DropEncryptStream {
    state: State,
    metadata: DropMetadata,
    padded_len: u64,
    total_chunks: usize,
    chunk_index: usize,
    buffer: Vec<u8>,
    bytes_received: u64,
    finished: bool,
}

impl DropEncryptStream {
    /// Initializes a new encrypt stream, validating metadata and encrypting Message 0 into Part 0.
    ///
    /// Memory consumption is bounded to one 1 MiB chunk regardless of total file size.
    pub fn new(
        key: &SecretKey,
        metadata: &DropMetadata,
        injected_header: Option<[u8; STREAM_HEADER_BYTES]>,
    ) -> Result<(Self, Vec<u8>), EmberError> {
        metadata.validate()?;

        let padded_len = calculate_padded_len(metadata.size);
        let total_chunks = (padded_len as usize).div_ceil(CHUNK_SIZE);

        let k_content = key.derive_content_key()?;
        let mut state = State::new();
        let mut header = [0u8; STREAM_HEADER_BYTES];

        if let Some(h) = injected_header {
            header = h;
            crypto_secretstream_xchacha20poly1305_init_pull(
                &mut state,
                &header,
                k_content.as_bytes(),
            );
        } else {
            crypto_secretstream_xchacha20poly1305_init_push(
                &mut state,
                &mut header,
                k_content.as_bytes(),
            );
        }

        // Encrypt Message 0 (metadata record)
        let mut msg0_plain = metadata.encode_record()?;
        let mut msg0_cipher = vec![0u8; METADATA_PLAIN_LEN + ABYTES];
        let push_res = crypto_secretstream_xchacha20poly1305_push(
            &mut state,
            &mut msg0_cipher,
            &msg0_plain,
            Some(AD_DROP),
            CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_TAG_MESSAGE,
        );
        msg0_plain.zeroize();
        push_res.map_err(|e| EmberError::Crypto(format!("Message 0 encryption failed: {e:?}")))?;

        let mut part0 = Vec::with_capacity(PART0_SIZE);
        part0.extend_from_slice(&header);
        part0.extend_from_slice(&msg0_cipher);

        let stream = Self {
            state,
            metadata: metadata.clone(),
            padded_len,
            total_chunks,
            chunk_index: 0,
            buffer: Vec::with_capacity(CHUNK_SIZE),
            bytes_received: 0,
            finished: false,
        };

        Ok((stream, part0))
    }

    /// The metadata record being encrypted.
    pub fn metadata(&self) -> &DropMetadata {
        &self.metadata
    }

    /// Total number of content parts expected (excluding Part 0).
    pub fn total_chunks(&self) -> usize {
        self.total_chunks
    }

    /// Number of content chunks encrypted and emitted so far.
    pub fn chunk_index(&self) -> usize {
        self.chunk_index
    }

    /// Total unencrypted bytes received via [`DropEncryptStream::update`] so far.
    pub fn bytes_received(&self) -> u64 {
        self.bytes_received
    }

    /// Whether this stream has been finalized.
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    /// Current allocated capacity of the internal plaintext chunk buffer (must not exceed 1 MiB).
    pub fn internal_buffer_capacity(&self) -> usize {
        self.buffer.capacity()
    }

    /// Feeds plaintext bytes into the streaming encryptor.
    ///
    /// Accepts arbitrary slice sizes (from 1 byte to gigabytes). Emits completed 1 MiB
    /// ciphertext parts as soon as full chunks accumulate. Does not emit the final chunk
    /// until [`DropEncryptStream::finish`] is called.
    pub fn update(&mut self, plaintext: &[u8]) -> Result<Vec<Vec<u8>>, EmberError> {
        if self.finished {
            return Err(EmberError::Crypto("Stream already finished".into()));
        }

        if self.bytes_received + (plaintext.len() as u64) > self.metadata.size {
            self.buffer.zeroize();
            self.buffer.clear();
            return Err(EmberError::SizeMismatch);
        }

        self.bytes_received += plaintext.len() as u64;

        let mut parts = Vec::new();
        let mut offset = 0;

        while offset < plaintext.len() {
            let available_in_chunk = CHUNK_SIZE - self.buffer.len();
            let to_copy = std::cmp::min(available_in_chunk, plaintext.len() - offset);
            self.buffer
                .extend_from_slice(&plaintext[offset..offset + to_copy]);
            offset += to_copy;

            // When buffer reaches 1 MiB and we have not reached the final chunk:
            // Intermediate content chunks are emitted with tag MESSAGE.
            // The final chunk (chunk_index == total_chunks - 1) is deferred to finish()
            // to allow zero padding and tag FINAL.
            if self.buffer.len() == CHUNK_SIZE && self.chunk_index < self.total_chunks - 1 {
                let mut cipher = vec![0u8; CHUNK_SIZE + ABYTES];
                let push_res = crypto_secretstream_xchacha20poly1305_push(
                    &mut self.state,
                    &mut cipher,
                    &self.buffer,
                    Some(AD_DROP),
                    CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_TAG_MESSAGE,
                );
                self.buffer.zeroize();
                self.buffer.clear();

                if let Err(e) = push_res {
                    return Err(EmberError::Crypto(format!(
                        "Chunk encryption failed: {e:?}"
                    )));
                }

                parts.push(cipher);
                self.chunk_index += 1;
            }
        }

        Ok(parts)
    }

    /// Finalizes the stream by applying spec zero-padding to the final chunk, encrypting
    /// it with `TAG_FINAL`, and zeroizing intermediate buffers.
    ///
    /// Returns the final ciphertext part (or fails if `bytes_received != metadata.size`).
    pub fn finish(&mut self) -> Result<Vec<Vec<u8>>, EmberError> {
        if self.finished {
            return Err(EmberError::Crypto("Stream already finished".into()));
        }

        if self.bytes_received != self.metadata.size {
            self.buffer.zeroize();
            self.buffer.clear();
            return Err(EmberError::SizeMismatch);
        }

        // Expected length of the final content chunk after padding
        let final_chunk_len = (self.padded_len as usize) - (self.total_chunks - 1) * CHUNK_SIZE;
        self.buffer.resize(final_chunk_len, 0u8);

        let mut cipher = vec![0u8; final_chunk_len + ABYTES];
        let push_res = crypto_secretstream_xchacha20poly1305_push(
            &mut self.state,
            &mut cipher,
            &self.buffer,
            Some(AD_DROP),
            CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_TAG_FINAL,
        );
        self.buffer.zeroize();
        self.buffer.clear();
        self.finished = true;
        self.chunk_index += 1;

        push_res
            .map_err(|e| EmberError::Crypto(format!("Final chunk encryption failed: {e:?}")))?;

        Ok(vec![cipher])
    }
}

impl Drop for DropEncryptStream {
    fn drop(&mut self) {
        self.buffer.zeroize();
    }
}

/// Convenience entry point to start a streaming drop encryption.
///
/// Returns the stateful [`DropEncryptStream`] and Part 0 (4137 bytes).
pub fn start_encrypt_stream(
    key: &SecretKey,
    metadata: &DropMetadata,
) -> Result<(DropEncryptStream, Vec<u8>), EmberError> {
    DropEncryptStream::new(key, metadata, None)
}

/// Starts streaming encryption with an injected 24-byte header (for deterministic test vectors).
pub fn start_encrypt_stream_with_header(
    key: &SecretKey,
    metadata: &DropMetadata,
    injected_header: Option<[u8; STREAM_HEADER_BYTES]>,
) -> Result<(DropEncryptStream, Vec<u8>), EmberError> {
    DropEncryptStream::new(key, metadata, injected_header)
}

/// Streaming drop decryptor that processes one transport part at a time.
///
/// # Security Invariant (Protocol Spec Section 4.3):
/// Callers streaming decrypted chunks to disk or downstream consumers MUST write to an
/// app-private temporary location. Callers MUST NOT expose or treat plaintext as valid
/// until [`DropDecryptStream::finish`] succeeds. If any part fails authentication, padding
/// check, or length constraints, or if `finish` fails, callers MUST discard and securely
/// wipe the temporary plaintext file.
pub struct DropDecryptStream {
    state: State,
    metadata: DropMetadata,
    expected_padded_len: u64,
    total_expected_chunks: usize,
    chunks_decrypted: usize,
    total_plain_bytes_decrypted: u64,
    seen_final: bool,
    finished: bool,
}

impl DropDecryptStream {
    /// Initializes a decrypt stream by verifying and decrypting Part 0.
    ///
    /// Validates stream header and Message 0, returning the stream instance.
    pub fn new(key: &SecretKey, part0: &[u8]) -> Result<Self, EmberError> {
        if part0.len() < PART0_SIZE {
            return Err(EmberError::NoFinalTag);
        }
        if part0.len() > PART0_SIZE {
            return Err(EmberError::Auth);
        }

        let mut header = [0u8; STREAM_HEADER_BYTES];
        header.copy_from_slice(&part0[0..STREAM_HEADER_BYTES]);
        let msg0_cipher = &part0[STREAM_HEADER_BYTES..PART0_SIZE];

        let k_content = key.derive_content_key()?;
        let mut state = State::new();
        crypto_secretstream_xchacha20poly1305_init_pull(&mut state, &header, k_content.as_bytes());

        let mut msg0_plain = vec![0u8; METADATA_PLAIN_LEN];
        let mut tag = 0u8;
        let pull_res = crypto_secretstream_xchacha20poly1305_pull(
            &mut state,
            &mut msg0_plain,
            &mut tag,
            msg0_cipher,
            Some(AD_DROP),
        );

        if pull_res.is_err() {
            msg0_plain.zeroize();
            return Err(EmberError::Auth);
        }

        let mlen = pull_res.unwrap();
        if mlen != METADATA_PLAIN_LEN || tag != CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_TAG_MESSAGE {
            msg0_plain.zeroize();
            return Err(EmberError::Auth);
        }

        let metadata = DropMetadata::decode_record(&msg0_plain);
        msg0_plain.zeroize();
        let metadata = metadata?;

        let expected_padded_len = calculate_padded_len(metadata.size);
        let total_expected_chunks = (expected_padded_len as usize).div_ceil(CHUNK_SIZE);

        Ok(Self {
            state,
            metadata,
            expected_padded_len,
            total_expected_chunks,
            chunks_decrypted: 0,
            total_plain_bytes_decrypted: 0,
            seen_final: false,
            finished: false,
        })
    }

    /// The verified drop metadata decoded from Part 0.
    pub fn metadata(&self) -> &DropMetadata {
        &self.metadata
    }

    /// Total number of content parts expected (excluding Part 0).
    pub fn total_expected_chunks(&self) -> usize {
        self.total_expected_chunks
    }

    /// The calculated padded content length in bytes per spec section 6.
    pub fn expected_padded_len(&self) -> u64 {
        self.expected_padded_len
    }

    /// Total unpadded plaintext bytes decrypted and verified so far.
    pub fn total_plain_bytes_decrypted(&self) -> u64 {
        self.total_plain_bytes_decrypted
    }

    /// Number of content parts verified and decrypted so far.
    pub fn chunks_decrypted(&self) -> usize {
        self.chunks_decrypted
    }

    /// Whether the verified `FINAL` tag chunk has been processed.
    pub fn is_final(&self) -> bool {
        self.seen_final
    }

    /// Whether [`DropDecryptStream::finish`] has succeeded.
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    /// Pushes the next transport part (Part 1..n) into the decryptor.
    ///
    /// Verifies authentication and ordering. If intermediate (`TAG_MESSAGE`), returns the 1 MiB chunk
    /// and `false`. If the final chunk (`TAG_FINAL`), verifies zero-padding per spec section 4.3,
    /// strips the padding bytes, and returns `(unpadded_bytes, true)`.
    ///
    /// Rejects parts after `FINAL` with [`EmberError::TrailingBytes`].
    pub fn push_part(&mut self, ciphertext_part: &[u8]) -> Result<(Vec<u8>, bool), EmberError> {
        if self.finished || self.seen_final {
            return Err(EmberError::TrailingBytes);
        }

        if ciphertext_part.len() < ABYTES {
            return Err(EmberError::NoFinalTag);
        }

        if ciphertext_part.len() > CHUNK_SIZE + ABYTES {
            return Err(EmberError::Auth);
        }

        let expected_plain_len = ciphertext_part.len() - ABYTES;
        let mut plain = vec![0u8; expected_plain_len];
        let mut chunk_tag = 0u8;

        let pull_res = crypto_secretstream_xchacha20poly1305_pull(
            &mut self.state,
            &mut plain,
            &mut chunk_tag,
            ciphertext_part,
            Some(AD_DROP),
        );

        if pull_res.is_err() {
            plain.zeroize();
            return Err(EmberError::Auth);
        }

        let plen = pull_res.unwrap();
        plain.truncate(plen);

        if chunk_tag == CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_TAG_FINAL {
            self.seen_final = true;

            // Final chunk must be at least 4096 bytes and at most 1 MiB
            if plain.len() < 4096 || plain.len() > CHUNK_SIZE {
                plain.zeroize();
                return Err(EmberError::Auth);
            }

            let remaining_content = self
                .metadata
                .size
                .saturating_sub(self.total_plain_bytes_decrypted);
            if (remaining_content as usize) > plain.len() {
                plain.zeroize();
                return Err(EmberError::SizeMismatch);
            }

            let content_len = remaining_content as usize;

            // Verify padding bytes are all 0x00 per spec section 4.3 rule 4
            for &b in &plain[content_len..] {
                if b != 0 {
                    plain.zeroize();
                    return Err(EmberError::InvalidPadding);
                }
            }

            plain.truncate(content_len);
            self.total_plain_bytes_decrypted += content_len as u64;
            self.chunks_decrypted += 1;
            Ok((plain, true))
        } else if chunk_tag == CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_TAG_MESSAGE {
            // Intermediate content chunks MUST be exactly 1 MiB
            if plain.len() != CHUNK_SIZE {
                plain.zeroize();
                return Err(EmberError::Auth);
            }

            if self.total_plain_bytes_decrypted + (plain.len() as u64) > self.metadata.size {
                plain.zeroize();
                return Err(EmberError::SizeMismatch);
            }

            self.total_plain_bytes_decrypted += plain.len() as u64;
            self.chunks_decrypted += 1;
            Ok((plain, false))
        } else {
            plain.zeroize();
            Err(EmberError::Auth)
        }
    }

    /// Enforces stream completion checks per protocol spec section 4.3.
    ///
    /// Fails with [`EmberError::NoFinalTag`] if the stream ended without `FINAL`,
    /// or [`EmberError::SizeMismatch`] if total unpadded content bytes do not match
    /// the verified metadata size.
    pub fn finish(&mut self) -> Result<(), EmberError> {
        if self.finished {
            return Ok(());
        }

        if !self.seen_final {
            return Err(EmberError::NoFinalTag);
        }

        if self.chunks_decrypted != self.total_expected_chunks {
            return Err(EmberError::NoFinalTag);
        }

        if self.total_plain_bytes_decrypted != self.metadata.size {
            return Err(EmberError::SizeMismatch);
        }

        self.finished = true;
        Ok(())
    }
}

/// Convenience entry point to start streaming drop decryption from Part 0.
///
/// Returns the stateful [`DropDecryptStream`] and decoded [`DropMetadata`].
pub fn start_decrypt_stream(
    key: &SecretKey,
    part0: &[u8],
) -> Result<(DropDecryptStream, DropMetadata), EmberError> {
    let stream = DropDecryptStream::new(key, part0)?;
    let metadata = stream.metadata().clone();
    Ok((stream, metadata))
}

/// Encrypts drop metadata and content into transport parts.
///
/// Built on top of the streaming engine ([`DropEncryptStream`]).
/// Part 0 is 4137 bytes (24-byte header + 4113-byte encrypted Message 0).
/// Part 1..n are content chunk ciphertexts (at most 1,048,593 bytes each).
pub fn encrypt_drop_to_parts(
    key: &SecretKey,
    metadata: &DropMetadata,
    content: &[u8],
    injected_header: Option<[u8; STREAM_HEADER_BYTES]>,
) -> Result<Vec<Vec<u8>>, EmberError> {
    if metadata.size != content.len() as u64 {
        return Err(EmberError::SizeMismatch);
    }

    let (mut stream, part0) = start_encrypt_stream_with_header(key, metadata, injected_header)?;
    let mut parts = Vec::with_capacity(1 + stream.total_chunks());
    parts.push(part0);

    let mut chunk_parts = stream.update(content)?;
    parts.append(&mut chunk_parts);

    let mut final_parts = stream.finish()?;
    parts.append(&mut final_parts);

    Ok(parts)
}

/// Encrypts drop metadata and content into a single contiguous ciphertext stream.
///
/// Built on top of the streaming engine ([`DropEncryptStream`]).
pub fn encrypt_drop(
    key: &SecretKey,
    metadata: &DropMetadata,
    content: &[u8],
    injected_header: Option<[u8; STREAM_HEADER_BYTES]>,
) -> Result<Vec<u8>, EmberError> {
    let parts = encrypt_drop_to_parts(key, metadata, content, injected_header)?;
    let total_len: usize = parts.iter().map(|p| p.len()).sum();
    let mut ciphertext = Vec::with_capacity(total_len);
    for part in parts {
        ciphertext.extend_from_slice(&part);
    }
    Ok(ciphertext)
}

/// Decrypts a drop from transport parts (Part 0, Part 1, ..., Part n).
///
/// Built on top of the streaming engine ([`DropDecryptStream`]).
pub fn decrypt_drop_parts<B: AsRef<[u8]>>(
    key: &SecretKey,
    parts: &[B],
) -> Result<(DropMetadata, Vec<u8>), EmberError> {
    if parts.is_empty() {
        return Err(EmberError::NoFinalTag);
    }

    let (mut stream, metadata) = start_decrypt_stream(key, parts[0].as_ref())?;
    let mut content = Vec::with_capacity((metadata.size as usize).min(MAX_PREALLOC_BYTES));

    for part in &parts[1..] {
        let (plain_chunk, _is_final) = stream.push_part(part.as_ref())?;
        content.extend_from_slice(&plain_chunk);
    }

    stream.finish()?;
    Ok((metadata, content))
}

/// Decrypts a drop from a contiguous ciphertext byte slice.
///
/// Built on top of the streaming engine ([`DropDecryptStream`]).
pub fn decrypt_drop(
    key: &SecretKey,
    ciphertext: &[u8],
) -> Result<(DropMetadata, Vec<u8>), EmberError> {
    if ciphertext.len() < PART0_SIZE {
        return Err(EmberError::NoFinalTag);
    }

    let part0 = &ciphertext[0..PART0_SIZE];
    let (mut stream, metadata) = start_decrypt_stream(key, part0)?;

    let padded_len = calculate_padded_len(metadata.size) as usize;
    let num_content_chunks = padded_len.div_ceil(CHUNK_SIZE);

    let mut content = Vec::with_capacity((metadata.size as usize).min(MAX_PREALLOC_BYTES));
    let mut offset = PART0_SIZE;

    for i in 0..num_content_chunks {
        let is_last = i == num_content_chunks - 1;
        let expected_plain_chunk = if is_last {
            padded_len - (num_content_chunks - 1) * CHUNK_SIZE
        } else {
            CHUNK_SIZE
        };
        let expected_cipher_chunk = expected_plain_chunk + ABYTES;

        if offset + expected_cipher_chunk <= ciphertext.len() {
            let part_slice = &ciphertext[offset..offset + expected_cipher_chunk];
            let (plain_chunk, _is_final) = stream.push_part(part_slice)?;
            content.extend_from_slice(&plain_chunk);
            offset += expected_cipher_chunk;
        } else {
            // Ciphertext stream is truncated in this chunk
            let part_slice = &ciphertext[offset..];
            let _ = stream.push_part(part_slice);
            return Err(EmberError::NoFinalTag);
        }
    }

    // Extraneous trailing bytes past expected chunks
    if offset < ciphertext.len() {
        let trailing = &ciphertext[offset..];
        let _ = stream.push_part(trailing);
        return Err(EmberError::TrailingBytes);
    }

    stream.finish()?;
    Ok((metadata, content))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_empty_file_roundtrip() {
        let key = SecretKey::generate();
        let meta = DropMetadata::new_file("empty.txt", "text/plain", 0, false, 1000, 2000);
        let content = b"";

        let ciphertext = encrypt_drop(&key, &meta, content, None).unwrap();
        assert_eq!(ciphertext.len(), 8250); // 4137 + 4113 = 8250 from spec table

        let (dec_meta, dec_content) = decrypt_drop(&key, &ciphertext).unwrap();
        assert_eq!(meta, dec_meta);
        assert_eq!(content.to_vec(), dec_content);
    }

    #[test]
    fn test_encrypt_decrypt_1_byte_roundtrip() {
        let key = SecretKey::generate();
        let meta =
            DropMetadata::new_file("one.bin", "application/octet-stream", 1, false, 1000, 2000);
        let content = b"Z";

        let ciphertext = encrypt_drop(&key, &meta, content, None).unwrap();
        assert_eq!(ciphertext.len(), 8250); // padded_len = 4096 -> 4137 + 4113 = 8250

        let (dec_meta, dec_content) = decrypt_drop(&key, &ciphertext).unwrap();
        assert_eq!(meta, dec_meta);
        assert_eq!(content.to_vec(), dec_content);
    }

    #[test]
    fn test_encrypt_decrypt_4096_bytes_roundtrip() {
        let key = SecretKey::generate();
        let meta = DropMetadata::new_file(
            "exact_min.bin",
            "application/octet-stream",
            4096,
            false,
            1000,
            2000,
        );
        let content = vec![0x42u8; 4096];

        let ciphertext = encrypt_drop(&key, &meta, &content, None).unwrap();
        assert_eq!(ciphertext.len(), 8250); // 4096 + 17 = 4113; 4137 + 4113 = 8250

        let (dec_meta, dec_content) = decrypt_drop(&key, &ciphertext).unwrap();
        assert_eq!(meta, dec_meta);
        assert_eq!(content, dec_content);
    }

    #[test]
    fn test_encrypt_decrypt_4097_bytes_roundtrip() {
        let key = SecretKey::generate();
        let meta = DropMetadata::new_file(
            "cross_band.bin",
            "application/octet-stream",
            4097,
            false,
            1000,
            2000,
        );
        let content = vec![0x77u8; 4097];

        let ciphertext = encrypt_drop(&key, &meta, &content, None).unwrap();
        // padded_len = 65,536 -> chunk size = 65,536 + 17 = 65,553; 4137 + 65,553 = 69,690 from spec table
        assert_eq!(ciphertext.len(), 69690);

        let (dec_meta, dec_content) = decrypt_drop(&key, &ciphertext).unwrap();
        assert_eq!(meta, dec_meta);
        assert_eq!(content, dec_content);
    }

    #[test]
    fn test_encrypt_decrypt_1_mib_roundtrip() {
        let key = SecretKey::generate();
        let meta = DropMetadata::new_file(
            "one_mib.bin",
            "application/octet-stream",
            1_048_576,
            false,
            1000,
            2000,
        );
        let content = vec![0x99u8; 1_048_576];

        let ciphertext = encrypt_drop(&key, &meta, &content, None).unwrap();
        // 1 MiB padded = 1 MiB -> 4137 + 1,048,593 = 1,052,730 from spec table
        assert_eq!(ciphertext.len(), 1_052_730);

        let (dec_meta, dec_content) = decrypt_drop(&key, &ciphertext).unwrap();
        assert_eq!(meta, dec_meta);
        assert_eq!(content, dec_content);
    }

    #[test]
    fn test_encrypt_decrypt_1_mib_plus_1_roundtrip() {
        let key = SecretKey::generate();
        let meta = DropMetadata::new_file(
            "two_chunks.bin",
            "application/octet-stream",
            1_048_577,
            false,
            1000,
            2000,
        );
        let content = vec![0x55u8; 1_048_577];

        let ciphertext = encrypt_drop(&key, &meta, &content, None).unwrap();
        // 1 MiB + 1 -> padded = 1,114,112 (2 chunks) -> 4137 + 1,048,593 + 65,553 = 1,118,283 from spec table
        assert_eq!(ciphertext.len(), 1_118_283);

        let (dec_meta, dec_content) = decrypt_drop(&key, &ciphertext).unwrap();
        assert_eq!(meta, dec_meta);
        assert_eq!(content, dec_content);
    }

    #[test]
    fn test_encrypt_decrypt_text_drop_roundtrip() {
        let key = SecretKey::generate();
        let text = "Secret message payload";
        let meta = DropMetadata::new_text(text.len() as u64, true, 1000, 2000);

        let ciphertext = encrypt_drop(&key, &meta, text.as_bytes(), None).unwrap();
        let (dec_meta, dec_content) = decrypt_drop(&key, &ciphertext).unwrap();
        assert_eq!(meta, dec_meta);
        assert_eq!(text.as_bytes(), dec_content.as_slice());
    }

    #[test]
    fn test_streaming_multi_chunk_arbitrary_slice_updates() {
        let key = SecretKey::generate();
        // 2.5 MiB content (3 chunks: 1 MiB, 1 MiB, ~0.5 MiB padded)
        let size = 2_500_000;
        let mut content = Vec::with_capacity(size);
        for i in 0..size {
            content.push((i % 251) as u8);
        }
        let meta = DropMetadata::new_file(
            "multi.bin",
            "application/octet-stream",
            size as u64,
            false,
            1000,
            2000,
        );

        // 1. Streaming encrypt with variable buffer sizes (e.g. 17 bytes, 64 KiB, 512 KiB)
        let (mut enc_stream, part0) = start_encrypt_stream(&key, &meta).unwrap();
        assert_eq!(enc_stream.total_chunks(), 3);

        let mut parts = vec![part0];
        let slice_sizes = [17, 65536, 100, 524288, 12345, 1048576, 77];
        let mut offset = 0;
        let mut size_idx = 0;

        while offset < content.len() {
            let chunk_sz = std::cmp::min(
                slice_sizes[size_idx % slice_sizes.len()],
                content.len() - offset,
            );
            let slice = &content[offset..offset + chunk_sz];
            let mut emitted = enc_stream.update(slice).unwrap();
            parts.append(&mut emitted);
            offset += chunk_sz;
            size_idx += 1;
        }

        let mut final_part = enc_stream.finish().unwrap();
        parts.append(&mut final_part);
        assert_eq!(parts.len(), 4); // part0 + 3 content parts

        // 2. Streaming decrypt part by part
        let (mut dec_stream, dec_meta) = start_decrypt_stream(&key, &parts[0]).unwrap();
        assert_eq!(meta, dec_meta);
        assert_eq!(dec_stream.total_expected_chunks(), 3);

        let mut decrypted = Vec::new();
        for (idx, part) in parts[1..].iter().enumerate() {
            let is_last_expected = idx == parts.len() - 2;
            let (plain_chunk, is_final) = dec_stream.push_part(part).unwrap();
            assert_eq!(is_final, is_last_expected);
            decrypted.extend_from_slice(&plain_chunk);
        }

        dec_stream.finish().unwrap();
        assert_eq!(content, decrypted);
    }

    #[test]
    fn test_streaming_memory_bounded_verification() {
        let key = SecretKey::generate();
        let size = 3_000_000; // 3 MiB
        let meta = DropMetadata::new_file(
            "large.bin",
            "application/octet-stream",
            size as u64,
            false,
            1000,
            2000,
        );

        let (mut enc_stream, _part0) = start_encrypt_stream(&key, &meta).unwrap();
        let large_buffer = vec![0xAAu8; size];

        // Feed the full 3 MiB in one call
        let parts = enc_stream.update(&large_buffer).unwrap();
        // Emitted the first 2 full 1 MiB chunks
        assert_eq!(parts.len(), 2);
        // The internal buffer capacity must never grow beyond 1 MiB (CHUNK_SIZE)
        assert!(enc_stream.internal_buffer_capacity() <= CHUNK_SIZE);

        let final_parts = enc_stream.finish().unwrap();
        assert_eq!(final_parts.len(), 1);
    }

    #[test]
    fn test_tamper_flipped_byte_fails_auth() {
        let key = SecretKey::generate();
        let meta = DropMetadata::new_text(10, false, 1000, 2000);
        let content = vec![42u8; 10];

        let mut ciphertext = encrypt_drop(&key, &meta, &content, None).unwrap();
        // Flip a byte in Part 0
        ciphertext[30] ^= 0x01;
        assert_eq!(decrypt_drop(&key, &ciphertext), Err(EmberError::Auth));

        let mut ciphertext2 = encrypt_drop(&key, &meta, &content, None).unwrap();
        // Flip a byte in Part 1 (content chunk)
        ciphertext2[4200] ^= 0x01;
        assert_eq!(decrypt_drop(&key, &ciphertext2), Err(EmberError::Auth));
    }

    #[test]
    fn test_streaming_tamper_flipped_byte_in_part0() {
        let key = SecretKey::generate();
        let meta = DropMetadata::new_text(10, false, 1000, 2000);
        let mut parts = encrypt_drop_to_parts(&key, &meta, b"0123456789", None).unwrap();

        parts[0][50] ^= 0x01; // flip in Part 0
        assert_eq!(
            start_decrypt_stream(&key, &parts[0]).err(),
            Some(EmberError::Auth)
        );
    }

    #[test]
    fn test_streaming_tamper_flipped_byte_in_chunk() {
        let key = SecretKey::generate();
        let size = 1_048_577; // 2 chunks
        let meta = DropMetadata::new_file(
            "flip.bin",
            "application/octet-stream",
            size,
            false,
            1000,
            2000,
        );
        let content = vec![0x11u8; size as usize];
        let mut parts = encrypt_drop_to_parts(&key, &meta, &content, None).unwrap();

        // Flip byte in chunk 1
        parts[1][100] ^= 0x01;
        let (mut dec_stream, _) = start_decrypt_stream(&key, &parts[0]).unwrap();
        assert_eq!(
            dec_stream.push_part(&parts[1]).err(),
            Some(EmberError::Auth)
        );
    }

    #[test]
    fn test_streaming_tamper_reordered_parts_fail() {
        let key = SecretKey::generate();
        let size = 1_048_577; // 2 content chunks (Part 1 and Part 2)
        let meta = DropMetadata::new_file(
            "reorder.bin",
            "application/octet-stream",
            size,
            false,
            1000,
            2000,
        );
        let content = vec![0x22u8; size as usize];
        let parts = encrypt_drop_to_parts(&key, &meta, &content, None).unwrap();

        let (mut dec_stream, _) = start_decrypt_stream(&key, &parts[0]).unwrap();
        // Push Part 2 first before Part 1
        assert_eq!(
            dec_stream.push_part(&parts[2]).err(),
            Some(EmberError::Auth)
        );
    }

    #[test]
    fn test_streaming_tamper_trailing_part_after_final_fails() {
        let key = SecretKey::generate();
        let meta = DropMetadata::new_text(5, false, 1000, 2000);
        let parts = encrypt_drop_to_parts(&key, &meta, b"hello", None).unwrap();

        let (mut dec_stream, _) = start_decrypt_stream(&key, &parts[0]).unwrap();
        let (_plain, is_final) = dec_stream.push_part(&parts[1]).unwrap();
        assert!(is_final);

        // Try pushing an extra part after final
        assert_eq!(
            dec_stream.push_part(&parts[1]).err(),
            Some(EmberError::TrailingBytes)
        );
    }

    #[test]
    fn test_streaming_finish_without_final_fails() {
        let key = SecretKey::generate();
        let size = 1_048_577; // 2 content chunks
        let meta = DropMetadata::new_file(
            "truncated.bin",
            "application/octet-stream",
            size,
            false,
            1000,
            2000,
        );
        let content = vec![0x33u8; size as usize];
        let parts = encrypt_drop_to_parts(&key, &meta, &content, None).unwrap();

        let (mut dec_stream, _) = start_decrypt_stream(&key, &parts[0]).unwrap();
        dec_stream.push_part(&parts[1]).unwrap(); // pushed part 1 (MESSAGE tag)
                                                  // Attempt to finish without having received part 2 (FINAL tag)
        assert_eq!(dec_stream.finish().err(), Some(EmberError::NoFinalTag));
    }

    #[test]
    fn test_tamper_trailing_bytes_detected() {
        let key = SecretKey::generate();
        let meta = DropMetadata::new_text(10, false, 1000, 2000);
        let content = vec![42u8; 10];

        let mut ciphertext = encrypt_drop(&key, &meta, &content, None).unwrap();
        ciphertext.extend_from_slice(b"EXTRA_TRAILING_BYTES");
        assert_eq!(
            decrypt_drop(&key, &ciphertext),
            Err(EmberError::TrailingBytes)
        );
    }

    #[test]
    fn test_tamper_truncated_stream_detected() {
        let key = SecretKey::generate();
        let meta = DropMetadata::new_text(10, false, 1000, 2000);
        let content = vec![42u8; 10];

        let ciphertext = encrypt_drop(&key, &meta, &content, None).unwrap();
        // Truncate to just Part 0 (missing the final content chunk)
        let truncated = &ciphertext[0..PART0_SIZE];
        assert_eq!(decrypt_drop(&key, truncated), Err(EmberError::NoFinalTag));
    }
}
