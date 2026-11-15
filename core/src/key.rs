//! Key handling, generation, derivation, and zeroization for Ember.

use std::fmt;
use subtle::ConstantTimeEq;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::error::EmberError;
use ember_protocol::{KDF_CONTEXT, SUBKEY_ID_CONTENT, SUBKEY_ID_THREAD_PSK};

/// A 32-byte secret key with automatic zeroization on drop.
/// Debug formatting explicitly suppresses key bytes to prevent accidental logging.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct SecretKey(pub [u8; 32]);

impl fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SecretKey([REDACTED])")
    }
}

impl PartialEq for SecretKey {
    fn eq(&self, other: &Self) -> bool {
        self.0.ct_eq(&other.0).into()
    }
}

impl Eq for SecretKey {}

impl SecretKey {
    /// Generates a new random 32-byte secret key using the operating system CSPRNG.
    pub fn generate() -> Self {
        use dryoc::rng::copy_randombytes;
        let mut key = [0u8; 32];
        copy_randombytes(&mut key);
        SecretKey(key)
    }

    /// Creates a SecretKey from existing bytes.
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        SecretKey(bytes)
    }

    /// Exposes a reference to the raw 32 bytes of the key.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Derives the 32-byte drop content encryption sub-key (id 1) using BLAKE2b KDF.
    pub fn derive_content_key(&self) -> Result<SecretKey, EmberError> {
        self.derive_subkey(SUBKEY_ID_CONTENT)
    }

    /// Derives the 32-byte thread pre-shared key (id 3) reserved for milestone M7.
    pub fn derive_thread_psk(&self) -> Result<SecretKey, EmberError> {
        self.derive_subkey(SUBKEY_ID_THREAD_PSK)
    }

    /// Derives an arbitrary 32-byte sub-key using `crypto_kdf_derive_from_key`.
    pub fn derive_subkey(&self, id: u64) -> Result<SecretKey, EmberError> {
        use dryoc::classic::crypto_kdf::crypto_kdf_derive_from_key;
        let mut subkey = [0u8; 32];
        crypto_kdf_derive_from_key(&mut subkey, id, &KDF_CONTEXT, &self.0)
            .map_err(|e| EmberError::Crypto(format!("KDF derivation failed: {e:?}")))?;
        Ok(SecretKey(subkey))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secret_key_debug_is_redacted() {
        let key = SecretKey::generate();
        let debug_str = format!("{key:?}");
        assert_eq!(debug_str, "SecretKey([REDACTED])");
    }

    #[test]
    fn test_subkey_derivation_deterministic() {
        let key = SecretKey::from_bytes([42u8; 32]);
        let k_content1 = key.derive_content_key().unwrap();
        let k_content2 = key.derive_content_key().unwrap();
        assert_eq!(k_content1, k_content2);

        let k_thread = key.derive_thread_psk().unwrap();
        assert_ne!(k_content1, k_thread);
    }
}
