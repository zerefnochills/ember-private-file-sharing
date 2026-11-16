//! Comprehensive property, roundtrip, and tamper tests for Ember core.
//!
//! Follows EMBER_BUILD_GUIDE.md Milestone M2 requirements:
//! - Roundtrip for empty, tiny, chunk-boundary and large inputs
//! - Tamper tests (flip any byte, truncate, reorder chunks, trailing bytes)
//! - Decryption must fail under any tampering

use ember_core::{
    build_link, decrypt_drop, decrypt_drop_parts, encrypt_drop, encrypt_drop_to_parts,
    generate_blob_id, parse_link, unwrap_key, wrap_key, DropMetadata, EmberError, LinkPayload,
    SecretKey, WrapParams,
};
use ember_protocol::{PART0_SIZE, STREAM_HEADER_BYTES};

#[test]
fn test_roundtrips_various_sizes() {
    let key = SecretKey::generate();
    let test_sizes = [
        0usize, 1, 17, 256, 4095, 4096, 4097, 65535, 65536, 65537, 131072,
        1_048_576, // 1 MiB chunk boundary
        1_048_577, // 1 MiB + 1 (2 chunks)
        1_500_000,
    ];

    for &size in &test_sizes {
        let content: Vec<u8> = (0..size).map(|i| (i % 256) as u8).collect();
        let meta = DropMetadata::new_file(
            format!("file_{size}.bin"),
            "application/octet-stream",
            size as u64,
            false,
            1_700_000_000,
            1_700_043_200,
        );

        // 1. Contiguous stream roundtrip
        let ciphertext = encrypt_drop(&key, &meta, &content).unwrap();
        let (dec_meta, dec_content) = decrypt_drop(&key, &ciphertext).unwrap();
        assert_eq!(dec_meta, meta, "Metadata mismatch for size {size}");
        assert_eq!(dec_content, content, "Content mismatch for size {size}");

        // 2. Transport parts roundtrip
        let parts = encrypt_drop_to_parts(&key, &meta, &content).unwrap();
        let (dec_meta_p, dec_content_p) = decrypt_drop_parts(&key, &parts).unwrap();
        assert_eq!(
            dec_meta_p, meta,
            "Metadata mismatch in parts for size {size}"
        );
        assert_eq!(
            dec_content_p, content,
            "Content mismatch in parts for size {size}"
        );
    }
}

#[test]
fn test_tamper_flip_every_byte_in_part0_and_chunk() {
    let key = SecretKey::generate();
    let content = b"Small payload for byte-by-byte corruption testing.";
    let meta = DropMetadata::new_file(
        "test.bin",
        "application/octet-stream",
        content.len() as u64,
        false,
        1000,
        2000,
    );

    let original = encrypt_drop(&key, &meta, content).unwrap();

    // Sample across header, msg0, and chunk regions
    let indices_to_test = [
        0, 1, 10, 23, // in stream_header
        24, 25, 100, 500, 2000, 4136, // in msg0 ciphertext
        4137, 4138, 4200, 8249, // in content chunk ciphertext
    ];

    for &idx in &indices_to_test {
        if idx < original.len() {
            let mut tampered = original.clone();
            tampered[idx] ^= 0x01;
            let res = decrypt_drop(&key, &tampered);
            assert_eq!(
                res,
                Err(EmberError::Auth),
                "Flipping byte at index {idx} MUST cause Auth error"
            );
        }
    }
}

#[test]
fn test_tamper_reorder_chunks_fails() {
    let key = SecretKey::generate();
    // 1 MiB + 1 byte guarantees exactly 2 content chunks (Part 1 and Part 2)
    let size = 1_048_577usize;
    let content: Vec<u8> = vec![0xAB; size];
    let meta = DropMetadata::new_file(
        "large.bin",
        "application/octet-stream",
        size as u64,
        false,
        1000,
        2000,
    );

    let parts = encrypt_drop_to_parts(&key, &meta, &content).unwrap();
    assert_eq!(parts.len(), 3); // Part 0 (meta), Part 1 (chunk 1), Part 2 (chunk 2)

    // Reorder Part 1 and Part 2
    let reordered = vec![parts[0].clone(), parts[2].clone(), parts[1].clone()];
    let res = decrypt_drop_parts(&key, &reordered);
    assert_eq!(
        res,
        Err(EmberError::Auth),
        "Reordered chunks MUST fail authentication"
    );
}

#[test]
fn test_tamper_truncated_ciphertexts() {
    let key = SecretKey::generate();
    let content = b"Payload for truncation tests";
    let meta = DropMetadata::new_file(
        "trunc.bin",
        "application/octet-stream",
        content.len() as u64,
        false,
        1000,
        2000,
    );

    let ciphertext = encrypt_drop(&key, &meta, content).unwrap();

    // Various truncation lengths
    let trunc_lengths = [
        0,
        1,
        10,
        STREAM_HEADER_BYTES,
        1000,
        PART0_SIZE - 1,
        PART0_SIZE,
    ];

    for &len in &trunc_lengths {
        let truncated = &ciphertext[..len];
        let res = decrypt_drop(&key, truncated);
        assert_eq!(
            res,
            Err(EmberError::NoFinalTag),
            "Truncated ciphertext at length {len} MUST produce NoFinalTag"
        );
    }
}

#[test]
fn test_tamper_trailing_bytes() {
    let key = SecretKey::generate();
    let content = b"Payload for trailing bytes tests";
    let meta = DropMetadata::new_file(
        "trail.bin",
        "application/octet-stream",
        content.len() as u64,
        false,
        1000,
        2000,
    );

    let ciphertext = encrypt_drop(&key, &meta, content).unwrap();

    let extra_payloads: [&[u8]; 4] = [
        &[0x00],
        &[0xFF; 16],
        &[0x42; 17],
        b"Extraneous bytes injected after cipherstream has finished",
    ];

    for &extra in &extra_payloads {
        let mut tampered = ciphertext.clone();
        tampered.extend_from_slice(extra);
        let res = decrypt_drop(&key, &tampered);
        assert_eq!(
            res,
            Err(EmberError::TrailingBytes),
            "Appending {} extra bytes MUST produce TrailingBytes error",
            extra.len()
        );
    }
}

#[test]
fn test_passphrase_wrap_and_tamper() {
    let key = SecretKey::generate();
    let passphrase = "my-secure-ember-passphrase";
    let params = WrapParams {
        ops: 1,
        mem_kib: 8192,
    };

    let payload = wrap_key(&key, passphrase, Some(params)).unwrap();
    assert_eq!(payload.len(), 95);

    // Roundtrip
    let unwrapped = unwrap_key(&payload, passphrase).unwrap();
    assert_eq!(key, unwrapped);

    // Wrong passphrase fails auth
    assert_eq!(
        unwrap_key(&payload, "wrong-passphrase"),
        Err(EmberError::Auth)
    );

    // Tamper each section of the 95-byte payload
    // 1. Version byte
    let mut bad_ver = payload.clone();
    bad_ver[0] = 0x02;
    assert_eq!(
        unwrap_key(&bad_ver, passphrase),
        Err(EmberError::UnsupportedVersion(2))
    );

    // 2. Flags byte
    let mut bad_flags = payload.clone();
    bad_flags[1] = 0x02; // Bit 1 reserved
    assert_eq!(
        unwrap_key(&bad_flags, passphrase),
        Err(EmberError::UnsupportedFlags(0x02))
    );

    // 3. Salt corruption
    let mut bad_salt = payload.clone();
    bad_salt[10] ^= 0xFF;
    assert_eq!(unwrap_key(&bad_salt, passphrase), Err(EmberError::Auth));

    // 4. Nonce corruption
    let mut bad_nonce = payload.clone();
    bad_nonce[30] ^= 0xFF;
    assert_eq!(unwrap_key(&bad_nonce, passphrase), Err(EmberError::Auth));

    // 5. Ciphertext corruption
    let mut bad_ct = payload.clone();
    bad_ct[60] ^= 0xFF;
    assert_eq!(unwrap_key(&bad_ct, passphrase), Err(EmberError::Auth));

    // 6. Parameter bounds checks
    assert!(WrapParams {
        ops: 0,
        mem_kib: 65536
    }
    .validate()
    .is_err());
    assert!(WrapParams {
        ops: 11,
        mem_kib: 65536
    }
    .validate()
    .is_err());
    assert!(WrapParams {
        ops: 3,
        mem_kib: 4096
    }
    .validate()
    .is_err());
    assert!(WrapParams {
        ops: 3,
        mem_kib: 300_000
    }
    .validate()
    .is_err());
}

#[test]
fn test_link_build_and_parse_tamper() {
    let key = SecretKey::generate();
    let payload = LinkPayload::Unprotected(key);
    let blob_id = generate_blob_id();

    let link = build_link("ember.domain.org", &blob_id, &payload).unwrap();
    let parsed = parse_link(&link).unwrap();
    assert_eq!(parsed.host, "ember.domain.org");
    assert_eq!(parsed.blob_id, blob_id);
    assert_eq!(parsed.payload, payload);

    // Malformed URLs
    assert!(parse_link("not-a-url").is_err());
    assert!(parse_link("https://ember.domain.org/d/short#payload").is_err()); // blob_id too short
    let base_no_frag = format!("https://ember.domain.org/d/{blob_id}");
    assert!(parse_link(&base_no_frag).is_err()); // missing #
    let base_empty_frag = format!("https://ember.domain.org/d/{blob_id}#");
    assert!(parse_link(&base_empty_frag).is_err()); // empty fragment
}
