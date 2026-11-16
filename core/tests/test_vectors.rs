//! Shared test vectors generation and validation against docs/protocol-spec.md section 10.

use std::fs;
use std::path::Path;

use ember_core::{
    decode_payload, decrypt_drop, encrypt_drop_to_parts_with_header, encrypt_drop_with_header,
    unwrap_key, wrap_key_with_injected, DropMetadata, EmberError, SecretKey, WrapParams,
};
use ember_protocol::STREAM_HEADER_BYTES;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
struct DecryptExpect {
    kind: String,
    name: String,
    mime: String,
    size: u64,
    content_hex: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct DecryptCase {
    name: String,
    #[serde(rename = "K_hex")]
    k_hex: String,
    ciphertext_hex: String,
    expect: DecryptExpect,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct WrapCase {
    name: String,
    passphrase_utf8: String,
    ops: u8,
    mem_kib: u32,
    salt_hex: String,
    nonce_hex: String,
    #[serde(rename = "K_hex")]
    k_hex: String,
    payload_hex: String,
    payload_b64url: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct NegativeCase {
    name: String,
    #[serde(rename = "K_hex", skip_serializing_if = "Option::is_none")]
    k_hex: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ciphertext_hex: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    payload_b64url: Option<String>,
    expect_error: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct DropVectors {
    version: u32,
    kdf_context: String,
    decrypt_cases: Vec<DecryptCase>,
    wrap_cases: Vec<WrapCase>,
    negative_cases: Vec<NegativeCase>,
}

fn to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        use std::fmt::Write;
        write!(&mut s, "{:02x}", b).unwrap();
    }
    s
}

fn from_hex(s: &str) -> Vec<u8> {
    assert_eq!(s.len() % 2, 0, "Hex string must have even length");
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("Valid hex digit"))
        .collect()
}

fn generate_vectors() -> DropVectors {
    let mut decrypt_cases = Vec::new();

    // 1. empty-file
    {
        let k_bytes = [0x01u8; 32];
        let key = SecretKey::from_bytes(k_bytes);
        let header = [0x11u8; STREAM_HEADER_BYTES];
        let meta = DropMetadata::new_file(
            "empty.txt",
            "text/plain",
            0,
            false,
            1_700_000_000,
            1_700_043_200,
        );
        let content = b"";
        let cipher = encrypt_drop_with_header(&key, &meta, content, Some(header)).unwrap();
        decrypt_cases.push(DecryptCase {
            name: "empty-file".to_string(),
            k_hex: to_hex(&k_bytes),
            ciphertext_hex: to_hex(&cipher),
            expect: DecryptExpect {
                kind: "file".to_string(),
                name: "empty.txt".to_string(),
                mime: "text/plain".to_string(),
                size: 0,
                content_hex: to_hex(content),
            },
        });
    }

    // 2. one-byte
    let one_byte_cipher: Vec<u8>;
    let one_byte_k: [u8; 32];
    {
        let k_bytes = [0x02u8; 32];
        one_byte_k = k_bytes;
        let key = SecretKey::from_bytes(k_bytes);
        let header = [0x22u8; STREAM_HEADER_BYTES];
        let meta = DropMetadata::new_file(
            "single_byte.bin",
            "application/octet-stream",
            1,
            false,
            1_700_000_000,
            1_700_043_200,
        );
        let content = b"Z";
        let cipher = encrypt_drop_with_header(&key, &meta, content, Some(header)).unwrap();
        one_byte_cipher = cipher.clone();
        decrypt_cases.push(DecryptCase {
            name: "one-byte".to_string(),
            k_hex: to_hex(&k_bytes),
            ciphertext_hex: to_hex(&cipher),
            expect: DecryptExpect {
                kind: "file".to_string(),
                name: "single_byte.bin".to_string(),
                mime: "application/octet-stream".to_string(),
                size: 1,
                content_hex: to_hex(content),
            },
        });
    }

    // 3. exact-4096
    {
        let k_bytes = [0x03u8; 32];
        let key = SecretKey::from_bytes(k_bytes);
        let header = [0x33u8; STREAM_HEADER_BYTES];
        let meta = DropMetadata::new_file(
            "block_4096.dat",
            "application/octet-stream",
            4096,
            false,
            1_700_000_000,
            1_700_043_200,
        );
        let content: Vec<u8> = (0..4096).map(|i| (i % 251) as u8).collect();
        let cipher = encrypt_drop_with_header(&key, &meta, &content, Some(header)).unwrap();
        decrypt_cases.push(DecryptCase {
            name: "exact-4096".to_string(),
            k_hex: to_hex(&k_bytes),
            ciphertext_hex: to_hex(&cipher),
            expect: DecryptExpect {
                kind: "file".to_string(),
                name: "block_4096.dat".to_string(),
                mime: "application/octet-stream".to_string(),
                size: 4096,
                content_hex: to_hex(&content),
            },
        });
    }

    // 4. boundary-4097
    {
        let k_bytes = [0x04u8; 32];
        let key = SecretKey::from_bytes(k_bytes);
        let header = [0x44u8; STREAM_HEADER_BYTES];
        let meta = DropMetadata::new_file(
            "boundary_4097.dat",
            "application/octet-stream",
            4097,
            false,
            1_700_000_000,
            1_700_043_200,
        );
        let content: Vec<u8> = (0..4097).map(|i| (i % 241) as u8).collect();
        let cipher = encrypt_drop_with_header(&key, &meta, &content, Some(header)).unwrap();
        decrypt_cases.push(DecryptCase {
            name: "boundary-4097".to_string(),
            k_hex: to_hex(&k_bytes),
            ciphertext_hex: to_hex(&cipher),
            expect: DecryptExpect {
                kind: "file".to_string(),
                name: "boundary_4097.dat".to_string(),
                mime: "application/octet-stream".to_string(),
                size: 4097,
                content_hex: to_hex(&content),
            },
        });
    }

    // 5. exact-1mib
    {
        let k_bytes = [0x05u8; 32];
        let key = SecretKey::from_bytes(k_bytes);
        let header = [0x55u8; STREAM_HEADER_BYTES];
        let meta = DropMetadata::new_file(
            "exact_1mib.bin",
            "application/octet-stream",
            1_048_576,
            false,
            1_700_000_000,
            1_700_043_200,
        );
        let content: Vec<u8> = (0..1_048_576).map(|i| (i % 239) as u8).collect();
        let cipher = encrypt_drop_with_header(&key, &meta, &content, Some(header)).unwrap();
        decrypt_cases.push(DecryptCase {
            name: "exact-1mib".to_string(),
            k_hex: to_hex(&k_bytes),
            ciphertext_hex: to_hex(&cipher),
            expect: DecryptExpect {
                kind: "file".to_string(),
                name: "exact_1mib.bin".to_string(),
                mime: "application/octet-stream".to_string(),
                size: 1_048_576,
                content_hex: to_hex(&content),
            },
        });
    }

    // 6. boundary-1mib-plus-1
    let boundary_parts: Vec<Vec<u8>>;
    let boundary_k: [u8; 32];
    {
        let k_bytes = [0x06u8; 32];
        boundary_k = k_bytes;
        let key = SecretKey::from_bytes(k_bytes);
        let header = [0x66u8; STREAM_HEADER_BYTES];
        let meta = DropMetadata::new_file(
            "boundary_1mib_plus_1.bin",
            "application/octet-stream",
            1_048_577,
            false,
            1_700_000_000,
            1_700_043_200,
        );
        let content: Vec<u8> = (0..1_048_577).map(|i| (i % 233) as u8).collect();
        boundary_parts =
            encrypt_drop_to_parts_with_header(&key, &meta, &content, Some(header)).unwrap();
        let mut cipher = Vec::new();
        for p in &boundary_parts {
            cipher.extend_from_slice(p);
        }
        decrypt_cases.push(DecryptCase {
            name: "boundary-1mib-plus-1".to_string(),
            k_hex: to_hex(&k_bytes),
            ciphertext_hex: to_hex(&cipher),
            expect: DecryptExpect {
                kind: "file".to_string(),
                name: "boundary_1mib_plus_1.bin".to_string(),
                mime: "application/octet-stream".to_string(),
                size: 1_048_577,
                content_hex: to_hex(&content),
            },
        });
    }

    // 7. text-drop
    {
        let k_bytes = [0x07u8; 32];
        let key = SecretKey::from_bytes(k_bytes);
        let header = [0x77u8; STREAM_HEADER_BYTES];
        let text = b"Ember secure zero-knowledge encrypted drop.";
        let meta = DropMetadata::new_text(text.len() as u64, true, 1_700_000_000, 1_700_043_200);
        let cipher = encrypt_drop_with_header(&key, &meta, text, Some(header)).unwrap();
        decrypt_cases.push(DecryptCase {
            name: "text-drop".to_string(),
            k_hex: to_hex(&k_bytes),
            ciphertext_hex: to_hex(&cipher),
            expect: DecryptExpect {
                kind: "text".to_string(),
                name: "".to_string(),
                mime: "text/plain; charset=utf-8".to_string(),
                size: text.len() as u64,
                content_hex: to_hex(text),
            },
        });
    }

    // 8. utf8-filename
    {
        let k_bytes = [0x08u8; 32];
        let key = SecretKey::from_bytes(k_bytes);
        let header = [0x88u8; STREAM_HEADER_BYTES];
        let content = b"%PDF-1.4 test vector file content for Ember";
        let meta = DropMetadata::new_file(
            "документ_ember_\u{26a1}.pdf",
            "application/pdf",
            content.len() as u64,
            false,
            1_700_000_000,
            1_700_043_200,
        );
        let cipher = encrypt_drop_with_header(&key, &meta, content, Some(header)).unwrap();
        decrypt_cases.push(DecryptCase {
            name: "utf8-filename".to_string(),
            k_hex: to_hex(&k_bytes),
            ciphertext_hex: to_hex(&cipher),
            expect: DecryptExpect {
                kind: "file".to_string(),
                name: "документ_ember_\u{26a1}.pdf".to_string(),
                mime: "application/pdf".to_string(),
                size: content.len() as u64,
                content_hex: to_hex(content),
            },
        });
    }

    // Wrap cases
    let mut wrap_cases = Vec::new();
    {
        let k_bytes: [u8; 32] = (1..=32).collect::<Vec<u8>>().try_into().unwrap();
        let key = SecretKey::from_bytes(k_bytes);
        let salt = [
            0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d,
            0x1e, 0x1f,
        ];
        let nonce = [
            0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d,
            0x2e, 0x2f, 0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37,
        ];
        let params = WrapParams {
            ops: 3,
            mem_kib: 65536,
        };
        let passphrase = "correct-horse-battery-staple";
        let payload =
            wrap_key_with_injected(&key, passphrase, Some(params), Some(salt), Some(nonce))
                .unwrap();
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        use base64::Engine;
        let payload_b64 = URL_SAFE_NO_PAD.encode(&payload);

        wrap_cases.push(WrapCase {
            name: "default-params".to_string(),
            passphrase_utf8: passphrase.to_string(),
            ops: params.ops,
            mem_kib: params.mem_kib,
            salt_hex: to_hex(&salt),
            nonce_hex: to_hex(&nonce),
            k_hex: to_hex(&k_bytes),
            payload_hex: to_hex(&payload),
            payload_b64url: payload_b64,
        });
    }

    // Negative cases
    let mut negative_cases = Vec::new();

    // 1. flipped-byte
    {
        let mut tampered = one_byte_cipher;
        tampered[50] ^= 0x01;
        negative_cases.push(NegativeCase {
            name: "flipped-byte".to_string(),
            k_hex: Some(to_hex(&one_byte_k)),
            ciphertext_hex: Some(to_hex(&tampered)),
            payload_b64url: None,
            expect_error: "auth".to_string(),
        });
    }

    // 2. truncated (drop Part 2 of boundary-1mib-plus-1)
    {
        let mut truncated = Vec::new();
        truncated.extend_from_slice(&boundary_parts[0]);
        truncated.extend_from_slice(&boundary_parts[1]);
        negative_cases.push(NegativeCase {
            name: "truncated".to_string(),
            k_hex: Some(to_hex(&boundary_k)),
            ciphertext_hex: Some(to_hex(&truncated)),
            payload_b64url: None,
            expect_error: "no-final".to_string(),
        });
    }

    // 3. trailing-bytes
    {
        let empty_cipher = from_hex(&decrypt_cases[0].ciphertext_hex);
        let mut with_trailing = empty_cipher;
        with_trailing.extend_from_slice(b"EXTRA_TRAILING_BYTES");
        negative_cases.push(NegativeCase {
            name: "trailing-bytes".to_string(),
            k_hex: Some(decrypt_cases[0].k_hex.clone()),
            ciphertext_hex: Some(to_hex(&with_trailing)),
            payload_b64url: None,
            expect_error: "trailing".to_string(),
        });
    }

    // 4. reordered-chunks (swap Part 1 and Part 2)
    {
        let mut reordered = Vec::new();
        reordered.extend_from_slice(&boundary_parts[0]);
        reordered.extend_from_slice(&boundary_parts[2]); // swapped
        reordered.extend_from_slice(&boundary_parts[1]);
        negative_cases.push(NegativeCase {
            name: "reordered-chunks".to_string(),
            k_hex: Some(to_hex(&boundary_k)),
            ciphertext_hex: Some(to_hex(&reordered)),
            payload_b64url: None,
            expect_error: "auth".to_string(),
        });
    }

    // 5. bad-link-version
    {
        let mut bad_payload = vec![0x02, 0x00]; // version 2
        bad_payload.extend_from_slice(&[0u8; 32]);
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        use base64::Engine;
        let b64 = URL_SAFE_NO_PAD.encode(&bad_payload);
        negative_cases.push(NegativeCase {
            name: "bad-link-version".to_string(),
            k_hex: None,
            ciphertext_hex: None,
            payload_b64url: Some(b64),
            expect_error: "version".to_string(),
        });
    }

    DropVectors {
        version: 1,
        kdf_context: "EMBERv1_".to_string(),
        decrypt_cases,
        wrap_cases,
        negative_cases,
    }
}

#[test]
fn test_generate_and_validate_all_vectors() {
    let vector_path = Path::new("../../testvectors/drop-v1.json");
    let fallback_path = Path::new("../testvectors/drop-v1.json");
    let target_path = if vector_path.parent().unwrap().exists() {
        vector_path
    } else {
        fallback_path
    };

    let vectors = if target_path.exists() {
        let content = fs::read_to_string(target_path).unwrap();
        serde_json::from_str::<DropVectors>(&content).unwrap()
    } else {
        let generated = generate_vectors();
        let json_str = serde_json::to_string_pretty(&generated).unwrap();
        fs::write(target_path, json_str).unwrap();
        generated
    };

    assert_eq!(vectors.version, 1);
    assert_eq!(vectors.kdf_context, "EMBERv1_");

    // Test all decrypt cases
    for case in &vectors.decrypt_cases {
        let k_bytes = from_hex(&case.k_hex);
        assert_eq!(k_bytes.len(), 32);
        let mut key_arr = [0u8; 32];
        key_arr.copy_from_slice(&k_bytes);
        let key = SecretKey::from_bytes(key_arr);

        let ciphertext = from_hex(&case.ciphertext_hex);
        let (meta, content) = decrypt_drop(&key, &ciphertext)
            .unwrap_or_else(|e| panic!("Failed to decrypt case '{}': {:?}", case.name, e));

        assert_eq!(
            meta.kind, case.expect.kind,
            "Kind mismatch in case '{}'",
            case.name
        );
        assert_eq!(
            meta.name, case.expect.name,
            "Name mismatch in case '{}'",
            case.name
        );
        assert_eq!(
            meta.mime, case.expect.mime,
            "MIME mismatch in case '{}'",
            case.name
        );
        assert_eq!(
            meta.size, case.expect.size,
            "Size mismatch in case '{}'",
            case.name
        );
        assert_eq!(
            to_hex(&content),
            case.expect.content_hex,
            "Content mismatch in case '{}'",
            case.name
        );
    }

    // Test all wrap cases
    for case in &vectors.wrap_cases {
        let k_bytes = from_hex(&case.k_hex);
        assert_eq!(k_bytes.len(), 32);
        let mut key_arr = [0u8; 32];
        key_arr.copy_from_slice(&k_bytes);
        let key = SecretKey::from_bytes(key_arr);

        let salt_bytes = from_hex(&case.salt_hex);
        let mut salt = [0u8; 16];
        salt.copy_from_slice(&salt_bytes);

        let nonce_bytes = from_hex(&case.nonce_hex);
        let mut nonce = [0u8; 24];
        nonce.copy_from_slice(&nonce_bytes);

        let params = WrapParams {
            ops: case.ops,
            mem_kib: case.mem_kib,
        };

        let payload = wrap_key_with_injected(
            &key,
            &case.passphrase_utf8,
            Some(params),
            Some(salt),
            Some(nonce),
        )
        .unwrap();

        assert_eq!(to_hex(&payload), case.payload_hex);

        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        use base64::Engine;
        let b64 = URL_SAFE_NO_PAD.encode(&payload);
        assert_eq!(b64, case.payload_b64url);

        // Unwrap verification
        let unwrapped = unwrap_key(&payload, &case.passphrase_utf8).unwrap();
        assert_eq!(key, unwrapped);
    }

    // Test all negative cases
    for case in &vectors.negative_cases {
        match case.expect_error.as_str() {
            "auth" => {
                let k_bytes = from_hex(case.k_hex.as_ref().unwrap());
                let mut key_arr = [0u8; 32];
                key_arr.copy_from_slice(&k_bytes);
                let key = SecretKey::from_bytes(key_arr);
                let ciphertext = from_hex(case.ciphertext_hex.as_ref().unwrap());
                assert_eq!(
                    decrypt_drop(&key, &ciphertext),
                    Err(EmberError::Auth),
                    "Expected Auth error in negative case '{}'",
                    case.name
                );
            }
            "no-final" => {
                let k_bytes = from_hex(case.k_hex.as_ref().unwrap());
                let mut key_arr = [0u8; 32];
                key_arr.copy_from_slice(&k_bytes);
                let key = SecretKey::from_bytes(key_arr);
                let ciphertext = from_hex(case.ciphertext_hex.as_ref().unwrap());
                assert_eq!(
                    decrypt_drop(&key, &ciphertext),
                    Err(EmberError::NoFinalTag),
                    "Expected NoFinalTag error in negative case '{}'",
                    case.name
                );
            }
            "trailing" => {
                let k_bytes = from_hex(case.k_hex.as_ref().unwrap());
                let mut key_arr = [0u8; 32];
                key_arr.copy_from_slice(&k_bytes);
                let key = SecretKey::from_bytes(key_arr);
                let ciphertext = from_hex(case.ciphertext_hex.as_ref().unwrap());
                assert_eq!(
                    decrypt_drop(&key, &ciphertext),
                    Err(EmberError::TrailingBytes),
                    "Expected TrailingBytes error in negative case '{}'",
                    case.name
                );
            }
            "version" => {
                let b64 = case.payload_b64url.as_ref().unwrap();
                assert!(
                    matches!(decode_payload(b64), Err(EmberError::UnsupportedVersion(_))),
                    "Expected UnsupportedVersion error in negative case '{}'",
                    case.name
                );
            }
            other => panic!("Unknown expected error: {other}"),
        }
    }
}
