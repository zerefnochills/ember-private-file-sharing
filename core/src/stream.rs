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

use dryoc::classic::crypto_secretstream_xchacha20poly1305::*;
use dryoc::constants::{
    CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_TAG_FINAL,
    CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_TAG_MESSAGE,
};
use ember_protocol::{
    ABYTES, AD_DROP, CHUNK_SIZE, METADATA_PLAIN_LEN, PART0_SIZE, STREAM_HEADER_BYTES,
};

use crate::error::EmberError;
use crate::key::SecretKey;
use crate::metadata::DropMetadata;
use crate::padding::{calculate_padded_len, verify_padding};

/// Encrypts drop metadata and content into transport parts.
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
    metadata.validate()?;

    let k_content = key.derive_content_key()?;

    let mut state = State::new();
    let mut header = [0u8; STREAM_HEADER_BYTES];

    if let Some(h) = injected_header {
        header = h;
        crypto_secretstream_xchacha20poly1305_init_pull(&mut state, &header, k_content.as_bytes());
    } else {
        crypto_secretstream_xchacha20poly1305_init_push(
            &mut state,
            &mut header,
            k_content.as_bytes(),
        );
    }

    // Encrypt Message 0 (metadata record)
    let msg0_plain = metadata.encode_record()?;
    let mut msg0_cipher = vec![0u8; METADATA_PLAIN_LEN + ABYTES];
    crypto_secretstream_xchacha20poly1305_push(
        &mut state,
        &mut msg0_cipher,
        &msg0_plain,
        Some(AD_DROP),
        CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_TAG_MESSAGE,
    )
    .map_err(|e| EmberError::Crypto(format!("Message 0 encryption failed: {e:?}")))?;

    let mut part0 = Vec::with_capacity(PART0_SIZE);
    part0.extend_from_slice(&header);
    part0.extend_from_slice(&msg0_cipher);

    // Prepare padded content
    let padded_len = calculate_padded_len(metadata.size) as usize;
    let mut padded_content = Vec::with_capacity(padded_len);
    padded_content.extend_from_slice(content);
    padded_content.resize(padded_len, 0u8);

    let chunks: Vec<&[u8]> = padded_content.chunks(CHUNK_SIZE).collect();
    let mut parts = Vec::with_capacity(1 + chunks.len());
    parts.push(part0);

    for (idx, &chunk) in chunks.iter().enumerate() {
        let is_last = idx == chunks.len() - 1;
        let tag = if is_last {
            CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_TAG_FINAL
        } else {
            CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_TAG_MESSAGE
        };

        let mut cipher = vec![0u8; chunk.len() + ABYTES];
        crypto_secretstream_xchacha20poly1305_push(
            &mut state,
            &mut cipher,
            chunk,
            Some(AD_DROP),
            tag,
        )
        .map_err(|e| EmberError::Crypto(format!("Chunk {idx} encryption failed: {e:?}")))?;

        parts.push(cipher);
    }

    Ok(parts)
}

/// Encrypts drop metadata and content into a single contiguous ciphertext stream.
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
pub fn decrypt_drop_parts<B: AsRef<[u8]>>(
    key: &SecretKey,
    parts: &[B],
) -> Result<(DropMetadata, Vec<u8>), EmberError> {
    if parts.is_empty() {
        return Err(EmberError::NoFinalTag);
    }

    let part0 = parts[0].as_ref();
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
    let mlen = crypto_secretstream_xchacha20poly1305_pull(
        &mut state,
        &mut msg0_plain,
        &mut tag,
        msg0_cipher,
        Some(AD_DROP),
    )
    .map_err(|_| EmberError::Auth)?;

    if mlen != METADATA_PLAIN_LEN || tag != CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_TAG_MESSAGE {
        return Err(EmberError::Auth);
    }

    let metadata = DropMetadata::decode_record(&msg0_plain)?;

    let mut decrypted_content = Vec::new();
    let mut seen_final = false;

    for part in &parts[1..] {
        let part_bytes = part.as_ref();
        if seen_final {
            return Err(EmberError::TrailingBytes);
        }
        if part_bytes.len() < ABYTES {
            return Err(EmberError::NoFinalTag);
        }

        let expected_plain_len = part_bytes.len() - ABYTES;
        let mut plain = vec![0u8; expected_plain_len];
        let mut chunk_tag = 0u8;
        let plen = crypto_secretstream_xchacha20poly1305_pull(
            &mut state,
            &mut plain,
            &mut chunk_tag,
            part_bytes,
            Some(AD_DROP),
        )
        .map_err(|_| EmberError::Auth)?;

        plain.truncate(plen);
        decrypted_content.extend_from_slice(&plain);

        if chunk_tag == CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_TAG_FINAL {
            seen_final = true;
        } else if chunk_tag != CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_TAG_MESSAGE {
            return Err(EmberError::Auth);
        }
    }

    if !seen_final {
        return Err(EmberError::NoFinalTag);
    }

    let size = metadata.size as usize;
    if size > decrypted_content.len() {
        return Err(EmberError::SizeMismatch);
    }

    verify_padding(&decrypted_content, size)?;
    decrypted_content.truncate(size);

    Ok((metadata, decrypted_content))
}

/// Decrypts a drop from a contiguous ciphertext byte slice.
pub fn decrypt_drop(
    key: &SecretKey,
    ciphertext: &[u8],
) -> Result<(DropMetadata, Vec<u8>), EmberError> {
    if ciphertext.len() < PART0_SIZE {
        return Err(EmberError::NoFinalTag);
    }

    // First decrypt Part 0 to obtain metadata and calculate expected chunk boundaries
    let part0 = &ciphertext[0..PART0_SIZE];
    let mut header = [0u8; STREAM_HEADER_BYTES];
    header.copy_from_slice(&part0[0..STREAM_HEADER_BYTES]);
    let msg0_cipher = &part0[STREAM_HEADER_BYTES..PART0_SIZE];

    let k_content = key.derive_content_key()?;
    let mut state = State::new();
    crypto_secretstream_xchacha20poly1305_init_pull(&mut state, &header, k_content.as_bytes());

    let mut msg0_plain = vec![0u8; METADATA_PLAIN_LEN];
    let mut tag = 0u8;
    let mlen = crypto_secretstream_xchacha20poly1305_pull(
        &mut state,
        &mut msg0_plain,
        &mut tag,
        msg0_cipher,
        Some(AD_DROP),
    )
    .map_err(|_| EmberError::Auth)?;

    if mlen != METADATA_PLAIN_LEN || tag != CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_TAG_MESSAGE {
        return Err(EmberError::Auth);
    }

    let metadata = DropMetadata::decode_record(&msg0_plain)?;

    let padded_len = calculate_padded_len(metadata.size) as usize;
    let num_content_chunks = padded_len.div_ceil(CHUNK_SIZE);

    let mut parts: Vec<&[u8]> = Vec::with_capacity(1 + num_content_chunks + 1);
    parts.push(part0);

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
            parts.push(&ciphertext[offset..offset + expected_cipher_chunk]);
            offset += expected_cipher_chunk;
        } else {
            // Stream was truncated in this chunk
            parts.push(&ciphertext[offset..]);
            offset = ciphertext.len();
            break;
        }
    }

    // If extra bytes exist past the expected stream end, include them as a trailing part
    if offset < ciphertext.len() {
        parts.push(&ciphertext[offset..]);
    }

    decrypt_drop_parts(key, &parts)
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
