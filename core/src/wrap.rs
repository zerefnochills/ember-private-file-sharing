//! Passphrase key wrapping and unwrapping with Argon2id and XChaCha20-Poly1305.
//!
//! Follows `docs/protocol-spec.md` section 5.2.

use dryoc::classic::crypto_aead_xchacha20poly1305_ietf::*;
use dryoc::classic::crypto_pwhash::*;
use dryoc::rng::copy_randombytes;
use zeroize::Zeroize;

use crate::error::EmberError;
use crate::key::SecretKey;
use ember_protocol::{
    AD_WRAP_PREFIX_LEN, DEFAULT_ARGON2_MEM_KIB, DEFAULT_ARGON2_OPS, FLAG_PASSPHRASE_PROTECTED,
    LINK_PAYLOAD_WRAPPED_LEN, LINK_VERSION_1, MAX_ARGON2_MEM_KIB, MAX_ARGON2_OPS,
    MIN_ARGON2_MEM_KIB, MIN_ARGON2_OPS,
};

/// Parameters used for Argon2id key derivation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WrapParams {
    pub ops: u8,
    pub mem_kib: u32,
}

impl Default for WrapParams {
    fn default() -> Self {
        Self {
            ops: DEFAULT_ARGON2_OPS,
            mem_kib: DEFAULT_ARGON2_MEM_KIB,
        }
    }
}

impl WrapParams {
    /// Validates parameters against the protocol security bounds.
    pub fn validate(&self) -> Result<(), EmberError> {
        if self.ops < MIN_ARGON2_OPS || self.ops > MAX_ARGON2_OPS {
            return Err(EmberError::InvalidPassphraseParams(format!(
                "Argon2id ops {} is outside allowed bounds [{}, {}]",
                self.ops, MIN_ARGON2_OPS, MAX_ARGON2_OPS
            )));
        }

        if self.mem_kib < MIN_ARGON2_MEM_KIB || self.mem_kib > MAX_ARGON2_MEM_KIB {
            return Err(EmberError::InvalidPassphraseParams(format!(
                "Argon2id memory {} KiB is outside allowed bounds [{}, {}]",
                self.mem_kib, MIN_ARGON2_MEM_KIB, MAX_ARGON2_MEM_KIB
            )));
        }

        Ok(())
    }
}

fn wrap_key_inner(
    key: &SecretKey,
    passphrase: &str,
    params: Option<WrapParams>,
    injected_salt: Option<[u8; 16]>,
    injected_nonce: Option<[u8; 24]>,
) -> Result<Vec<u8>, EmberError> {
    let p = params.unwrap_or_default();
    p.validate()?;

    let salt = injected_salt.unwrap_or_else(|| {
        let mut s = [0u8; 16];
        copy_randombytes(&mut s);
        s
    });

    let nonce = injected_nonce.unwrap_or_else(|| {
        let mut n = [0u8; 24];
        copy_randombytes(&mut n);
        n
    });

    let mut payload = vec![0u8; LINK_PAYLOAD_WRAPPED_LEN];
    payload[0] = LINK_VERSION_1;
    payload[1] = FLAG_PASSPHRASE_PROTECTED;
    payload[2] = p.ops;
    payload[3..7].copy_from_slice(&p.mem_kib.to_be_bytes());
    payload[7..23].copy_from_slice(&salt);
    payload[23..47].copy_from_slice(&nonce);

    let ad = &payload[0..AD_WRAP_PREFIX_LEN];

    // Derive wrapping key P from passphrase via Argon2id
    let mut derived_key = [0u8; 32];
    let mem_limit_bytes = (p.mem_kib as usize) * 1024;
    crypto_pwhash(
        &mut derived_key,
        passphrase.as_bytes(),
        &salt,
        p.ops as u64,
        mem_limit_bytes,
        PasswordHashAlgorithm::Argon2id13,
    )
    .map_err(|e| EmberError::Crypto(format!("Argon2id derivation failed: {e:?}")))?;

    // Encrypt key with derived key P, nonce, and ad
    let mut wrapped_k = [0u8; 48];
    let encrypt_res = crypto_aead_xchacha20poly1305_ietf_encrypt(
        &mut wrapped_k,
        key.as_bytes(),
        Some(ad),
        &nonce,
        &derived_key,
    );

    derived_key.zeroize();

    encrypt_res.map_err(|e| EmberError::Crypto(format!("Key wrap encryption failed: {e:?}")))?;
    payload[47..95].copy_from_slice(&wrapped_k);

    Ok(payload)
}

/// Wraps a master secret key `K` with a passphrase using Argon2id and XChaCha20-Poly1305.
/// Produces a 95-byte binary payload suitable for link encoding.
pub fn wrap_key(
    key: &SecretKey,
    passphrase: &str,
    params: Option<WrapParams>,
) -> Result<Vec<u8>, EmberError> {
    wrap_key_inner(key, passphrase, params, None, None)
}

/// Test hook for deterministic vector generation and verification.
/// Compiled out unless the `test-hooks` cargo feature is enabled.
#[cfg(feature = "test-hooks")]
pub fn wrap_key_with_injected(
    key: &SecretKey,
    passphrase: &str,
    params: Option<WrapParams>,
    injected_salt: Option<[u8; 16]>,
    injected_nonce: Option<[u8; 24]>,
) -> Result<Vec<u8>, EmberError> {
    wrap_key_inner(key, passphrase, params, injected_salt, injected_nonce)
}

/// Unwraps a master secret key `K` from a 95-byte passphrase-protected binary payload.
pub fn unwrap_key(payload: &[u8], passphrase: &str) -> Result<SecretKey, EmberError> {
    if payload.len() != LINK_PAYLOAD_WRAPPED_LEN {
        return Err(EmberError::InvalidLink(format!(
            "Wrapped payload length is {} bytes (expected 95)",
            payload.len()
        )));
    }

    let version = payload[0];
    if version != LINK_VERSION_1 {
        return Err(EmberError::UnsupportedVersion(version));
    }

    let flags = payload[1];
    if flags != FLAG_PASSPHRASE_PROTECTED {
        if flags & !FLAG_PASSPHRASE_PROTECTED != 0 {
            return Err(EmberError::UnsupportedFlags(flags));
        }
        return Err(EmberError::InvalidLink(
            "Payload flag is not passphrase protected".to_string(),
        ));
    }

    let ops = payload[2];
    let mem_kib = u32::from_be_bytes([payload[3], payload[4], payload[5], payload[6]]);
    let params = WrapParams { ops, mem_kib };
    params.validate()?;

    let mut salt = [0u8; 16];
    salt.copy_from_slice(&payload[7..23]);

    let mut nonce = [0u8; 24];
    nonce.copy_from_slice(&payload[23..47]);

    let ad = &payload[0..AD_WRAP_PREFIX_LEN];
    let ciphertext = &payload[47..95];

    let mut derived_key = [0u8; 32];
    let mem_limit_bytes = (mem_kib as usize) * 1024;
    crypto_pwhash(
        &mut derived_key,
        passphrase.as_bytes(),
        &salt,
        ops as u64,
        mem_limit_bytes,
        PasswordHashAlgorithm::Argon2id13,
    )
    .map_err(|e| EmberError::Crypto(format!("Argon2id derivation failed: {e:?}")))?;

    let mut raw_key = [0u8; 32];
    let decrypt_res = crypto_aead_xchacha20poly1305_ietf_decrypt(
        &mut raw_key,
        ciphertext,
        Some(ad),
        &nonce,
        &derived_key,
    );

    derived_key.zeroize();

    if decrypt_res.is_err() {
        raw_key.zeroize();
        return Err(EmberError::Auth);
    }

    let key = SecretKey::from_bytes(raw_key);
    raw_key.zeroize();
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wrap_unwrap_roundtrip() {
        let key = SecretKey::generate();
        let passphrase = "correct-horse-battery-staple";
        // Use lightweight params for quick unit test
        let params = WrapParams {
            ops: 1,
            mem_kib: 8192,
        };

        let wrapped = wrap_key(&key, passphrase, Some(params)).unwrap();
        assert_eq!(wrapped.len(), 95);

        let unwrapped = unwrap_key(&wrapped, passphrase).unwrap();
        assert_eq!(key, unwrapped);
    }

    #[test]
    fn test_unwrap_wrong_passphrase_fails() {
        let key = SecretKey::generate();
        let params = WrapParams {
            ops: 1,
            mem_kib: 8192,
        };

        let wrapped = wrap_key(&key, "correct-passphrase", Some(params)).unwrap();
        let res = unwrap_key(&wrapped, "wrong-passphrase");
        assert_eq!(res, Err(EmberError::Auth));
    }

    #[test]
    fn test_unwrap_corrupted_ciphertext_fails() {
        let key = SecretKey::generate();
        let params = WrapParams {
            ops: 1,
            mem_kib: 8192,
        };

        let mut wrapped = wrap_key(&key, "passphrase", Some(params)).unwrap();
        wrapped[50] ^= 0x01; // Tamper with ciphertext
        let res = unwrap_key(&wrapped, "passphrase");
        assert_eq!(res, Err(EmberError::Auth));
    }
}
