//! Abuse and moderation report encryption at rest.
//!
//! Uses libsodium-compatible sealed boxes (`crypto_box_seal` / `crypto_box_seal_open`)
//! to encrypt sensitive report fields (`content` and `link_key`) to a reviewer's
//! X25519 public key. Only an authorized reviewer holding the corresponding secret key
//! can decrypt the report payloads.

use dryoc::classic::crypto_box::{crypto_box_keypair, crypto_box_seal, crypto_box_seal_open};
use dryoc::constants::{
    CRYPTO_BOX_PUBLICKEYBYTES, CRYPTO_BOX_SEALBYTES, CRYPTO_BOX_SECRETKEYBYTES,
};

use crate::error::EmberError;

/// Format prefix for sealed report fields.
pub const SEALED_PREFIX: &str = "sealed:";

/// Generates a new reviewer keypair `(public_key, secret_key)` using libsodium crypto_box.
pub fn generate_reviewer_keypair() -> (
    [u8; CRYPTO_BOX_PUBLICKEYBYTES],
    [u8; CRYPTO_BOX_SECRETKEYBYTES],
) {
    crypto_box_keypair()
}

/// Encrypts a report plaintext string to a reviewer's public key using a libsodium sealed box.
/// Returns a string prefixed with `sealed:` followed by standard base64 ciphertext.
pub fn seal_report_field(
    plaintext: &[u8],
    recipient_pk: &[u8; CRYPTO_BOX_PUBLICKEYBYTES],
) -> Result<String, EmberError> {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;

    let mut ciphertext = vec![0u8; plaintext.len() + CRYPTO_BOX_SEALBYTES];
    crypto_box_seal(&mut ciphertext, plaintext, recipient_pk)
        .map_err(|e| EmberError::Crypto(format!("Failed to seal field: {e:?}")))?;
    Ok(format!("{SEALED_PREFIX}{}", STANDARD.encode(&ciphertext)))
}

/// Decrypts a sealed report field using the reviewer's public and secret keys.
pub fn unseal_report_field(
    sealed_str: &str,
    recipient_pk: &[u8; CRYPTO_BOX_PUBLICKEYBYTES],
    recipient_sk: &[u8; CRYPTO_BOX_SECRETKEYBYTES],
) -> Result<Vec<u8>, EmberError> {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;

    let b64 = sealed_str.strip_prefix(SEALED_PREFIX).unwrap_or(sealed_str);
    let ciphertext = STANDARD
        .decode(b64)
        .map_err(|e| EmberError::Crypto(format!("Invalid base64 encoding: {e}")))?;

    if ciphertext.len() < CRYPTO_BOX_SEALBYTES {
        return Err(EmberError::Auth);
    }

    let mut plaintext = vec![0u8; ciphertext.len() - CRYPTO_BOX_SEALBYTES];
    crypto_box_seal_open(&mut plaintext, &ciphertext, recipient_pk, recipient_sk)
        .map_err(|_| EmberError::Auth)?;

    Ok(plaintext)
}

/// Converts a 32-byte key to a 64-character hex string.
pub fn key_to_hex(key: &[u8; 32]) -> String {
    let mut s = String::with_capacity(64);
    for &b in key {
        use std::fmt::Write;
        write!(&mut s, "{:02x}", b).unwrap();
    }
    s
}

/// Parses a 64-character hex string into a 32-byte key.
pub fn parse_hex_key(hex_str: &str) -> Result<[u8; 32], EmberError> {
    let clean = hex_str.trim();
    if clean.len() != 64 {
        return Err(EmberError::Crypto(format!(
            "Key must be exactly 64 hex characters (32 bytes), got {}",
            clean.len()
        )));
    }
    let mut key = [0u8; 32];
    for (i, byte) in key.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&clean[i * 2..i * 2 + 2], 16).map_err(|e| {
            EmberError::Crypto(format!("Invalid hex character at index {}: {e}", i * 2))
        })?;
    }
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_report_sealed_box_roundtrip() {
        let (pk, sk) = generate_reviewer_keypair();
        let payload = b"Reporter canary sensitive plaintext content";

        let sealed = seal_report_field(payload, &pk).unwrap();
        assert!(sealed.starts_with(SEALED_PREFIX));
        assert_ne!(sealed.as_bytes(), payload);

        let unsealed = unseal_report_field(&sealed, &pk, &sk).unwrap();
        assert_eq!(unsealed, payload);
    }

    #[test]
    fn test_report_sealed_box_wrong_key_fails() {
        let (pk1, _) = generate_reviewer_keypair();
        let (pk2, sk2) = generate_reviewer_keypair();
        let payload = b"Confidential file link key";

        let sealed = seal_report_field(payload, &pk1).unwrap();
        // Trying to decrypt with keypair 2 must fail auth
        let res = unseal_report_field(&sealed, &pk2, &sk2);
        assert_eq!(res, Err(EmberError::Auth));
    }

    #[test]
    fn test_hex_key_roundtrip() {
        let (pk, sk) = generate_reviewer_keypair();
        let pk_hex = key_to_hex(&pk);
        let sk_hex = key_to_hex(&sk);
        assert_eq!(pk_hex.len(), 64);
        assert_eq!(sk_hex.len(), 64);

        let parsed_pk = parse_hex_key(&pk_hex).unwrap();
        let parsed_sk = parse_hex_key(&sk_hex).unwrap();
        assert_eq!(parsed_pk, pk);
        assert_eq!(parsed_sk, sk);

        // Invalid lengths or characters fail cleanly
        assert!(parse_hex_key("abcd").is_err());
        assert!(parse_hex_key(&"z".repeat(64)).is_err());
    }
}
