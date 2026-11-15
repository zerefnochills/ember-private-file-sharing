//! Proof-of-work challenge, solving, and verification primitives for Ember.
//!
//! Provides a standard SHA-256 hashcash solver suitable for both native and WebAssembly (WASM) targets,
//! along with stateless signed challenge generation and verification.

use base64::engine::general_purpose::URL_SAFE_NO_PAD as BASE64_URL;
use base64::Engine;
use dryoc::classic::crypto_generichash::crypto_generichash;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

use crate::error::EmberError;

/// Counts leading zero bits in a 32-byte hash.
pub fn count_leading_zero_bits(hash: &[u8; 32]) -> u32 {
    let mut zeros = 0u32;
    for &b in hash {
        let lz = b.leading_zeros();
        zeros += lz;
        if lz != 8 {
            break;
        }
    }
    zeros
}

/// Computes SHA-256(challenge || nonce_be_bytes).
pub fn hash_pow(challenge: &[u8], nonce: u64) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(challenge);
    hasher.update(nonce.to_be_bytes());
    hasher.finalize().into()
}

/// Verifies whether `nonce` solves the PoW puzzle for `challenge` at `difficulty`.
pub fn verify_pow(challenge: &[u8], nonce: u64, difficulty: u32) -> bool {
    let hash = hash_pow(challenge, nonce);
    count_leading_zero_bits(&hash) >= difficulty
}

/// Solves a proof-of-work puzzle by finding a nonce such that
/// `hash_pow(challenge, nonce)` has at least `difficulty` leading zero bits.
///
/// If `max_iterations` is provided, stops after that many nonces and returns an error.
pub fn solve_pow(
    challenge: &[u8],
    difficulty: u32,
    max_iterations: Option<u64>,
) -> Result<u64, EmberError> {
    let limit = max_iterations.unwrap_or(u64::MAX);
    for nonce in 0..limit {
        if verify_pow(challenge, nonce, difficulty) {
            return Ok(nonce);
        }
    }
    Err(EmberError::InvalidPow(
        "PoW solver exceeded iteration limit without finding solution".into(),
    ))
}

/// A decoded proof-of-work challenge token issued by the relay server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PowChallengeToken {
    /// 16-byte random challenge identifier.
    pub id: [u8; 16],
    /// Unix timestamp when the challenge was created.
    pub created_at: u64,
    /// Unix timestamp when the challenge expires.
    pub expires_at: u64,
    /// Required number of leading zero bits.
    pub difficulty: u32,
}

impl PowChallengeToken {
    /// Serializes and signs a new challenge token into a URL-safe base64 string.
    ///
    /// Raw payload layout:
    /// - bytes 0..16:  id (16 bytes)
    /// - bytes 16..24: created_at (8 bytes, big-endian)
    /// - bytes 24..32: expires_at (8 bytes, big-endian)
    /// - bytes 32..36: difficulty (4 bytes, big-endian)
    /// - bytes 36..52: keyed BLAKE2b authentication tag (16 bytes)
    pub fn create(
        id: [u8; 16],
        created_at: u64,
        expires_at: u64,
        difficulty: u32,
        server_secret: &[u8; 32],
    ) -> String {
        let mut raw = [0u8; 52];
        raw[0..16].copy_from_slice(&id);
        raw[16..24].copy_from_slice(&created_at.to_be_bytes());
        raw[24..32].copy_from_slice(&expires_at.to_be_bytes());
        raw[32..36].copy_from_slice(&difficulty.to_be_bytes());

        let mut tag = [0u8; 16];
        crypto_generichash(&mut tag, &raw[0..36], Some(server_secret))
            .expect("keyed hash should succeed");
        raw[36..52].copy_from_slice(&tag);

        BASE64_URL.encode(raw)
    }

    /// Decodes, verifies the signature, and checks expiry for a challenge token string.
    pub fn verify_and_decode(
        token_str: &str,
        server_secret: &[u8; 32],
        now: u64,
    ) -> Result<Self, EmberError> {
        let raw = BASE64_URL
            .decode(token_str)
            .map_err(|e| EmberError::InvalidPow(format!("Invalid challenge encoding: {e}")))?;

        if raw.len() != 52 {
            return Err(EmberError::InvalidPow(format!(
                "Invalid challenge length: expected 52 bytes, got {}",
                raw.len()
            )));
        }

        let payload = &raw[0..36];
        let tag = &raw[36..52];

        let mut expected_tag = [0u8; 16];
        crypto_generichash(&mut expected_tag, payload, Some(server_secret))
            .map_err(|e| EmberError::Crypto(format!("Hash error: {e:?}")))?;

        if tag.ct_eq(&expected_tag).unwrap_u8() != 1 {
            return Err(EmberError::InvalidPow("Invalid challenge signature".into()));
        }

        let mut id = [0u8; 16];
        id.copy_from_slice(&raw[0..16]);

        let created_at = u64::from_be_bytes(raw[16..24].try_into().unwrap());
        let expires_at = u64::from_be_bytes(raw[24..32].try_into().unwrap());
        let difficulty = u32::from_be_bytes(raw[32..36].try_into().unwrap());

        if now > expires_at {
            return Err(EmberError::Expired("Challenge expired".into()));
        }

        Ok(Self {
            id,
            created_at,
            expires_at,
            difficulty,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_leading_zero_bits() {
        let mut hash = [0u8; 32];
        assert_eq!(count_leading_zero_bits(&hash), 256);

        hash[0] = 0b0000_0001;
        assert_eq!(count_leading_zero_bits(&hash), 7);

        hash[0] = 0b0010_0000;
        assert_eq!(count_leading_zero_bits(&hash), 2);

        hash[0] = 0b1000_0000;
        assert_eq!(count_leading_zero_bits(&hash), 0);

        hash = [0xff; 32];
        assert_eq!(count_leading_zero_bits(&hash), 0);

        hash = [0u8; 32];
        hash[0] = 0;
        hash[1] = 0;
        hash[2] = 0b0000_1111;
        assert_eq!(count_leading_zero_bits(&hash), 16 + 4);
    }

    #[test]
    fn test_solve_and_verify_pow() {
        let challenge = b"test_challenge_string_12345";
        let difficulty = 10; // Fast for unit tests (~1024 iterations average)

        let nonce = solve_pow(challenge, difficulty, Some(100_000)).expect("should find nonce");
        assert!(verify_pow(challenge, nonce, difficulty));

        // Wrong nonce fails
        assert!(!verify_pow(challenge, nonce.wrapping_add(1), 250));
    }

    #[test]
    fn test_challenge_token_roundtrip() {
        let secret = [77u8; 32];
        let id = [1u8; 16];
        let now = 1_700_000_000;
        let expires = now + 300;
        let difficulty = 16;

        let token_str = PowChallengeToken::create(id, now, expires, difficulty, &secret);
        let parsed =
            PowChallengeToken::verify_and_decode(&token_str, &secret, now).expect("valid token");

        assert_eq!(parsed.id, id);
        assert_eq!(parsed.created_at, now);
        assert_eq!(parsed.expires_at, expires);
        assert_eq!(parsed.difficulty, difficulty);
    }

    #[test]
    fn test_challenge_token_tamper_fails() {
        let secret = [77u8; 32];
        let wrong_secret = [78u8; 32];
        let id = [1u8; 16];
        let now = 1_700_000_000;
        let expires = now + 300;
        let difficulty = 16;

        let token_str = PowChallengeToken::create(id, now, expires, difficulty, &secret);

        // Wrong secret
        assert!(PowChallengeToken::verify_and_decode(&token_str, &wrong_secret, now).is_err());

        // Expired
        assert!(PowChallengeToken::verify_and_decode(&token_str, &secret, expires + 1).is_err());
    }
}
