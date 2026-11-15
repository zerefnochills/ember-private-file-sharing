use std::sync::Arc;

use ember_core::{decrypt_drop, encrypt_drop, DropMetadata, SecretKey};

uniffi::setup_scaffolding!();

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum FfiError {
    #[error("The encrypted data is invalid or could not be authenticated")]
    InvalidCiphertext,
    #[error("The encrypted data is not a text drop")]
    NotText,
    #[error("The text drop is not valid UTF-8")]
    InvalidText,
    #[error("The requested expiry is invalid")]
    InvalidExpiry,
}

/// A key handle that keeps secret key bytes in Rust and zeroizes them on drop.
#[derive(uniffi::Object)]
pub struct EmberCrypto {
    key: SecretKey,
}

#[uniffi::export]
impl EmberCrypto {
    #[uniffi::constructor]
    pub fn generate() -> Arc<Self> {
        Arc::new(Self {
            key: SecretKey::generate(),
        })
    }

    /// Encrypts a UTF-8 text drop in the same format used by the shared Rust core.
    pub fn encrypt_text(
        &self,
        text: String,
        created_at: u64,
        expires_at: u64,
    ) -> Result<Vec<u8>, FfiError> {
        if expires_at <= created_at {
            return Err(FfiError::InvalidExpiry);
        }

        let metadata = DropMetadata::new_text(text.len() as u64, false, created_at, expires_at);
        encrypt_drop(&self.key, &metadata, text.as_bytes(), None)
            .map_err(|_| FfiError::InvalidCiphertext)
    }

    /// Decrypts a text drop and rejects file drops instead of silently decoding them.
    pub fn decrypt_text(&self, ciphertext: Vec<u8>) -> Result<String, FfiError> {
        let (metadata, plaintext) =
            decrypt_drop(&self.key, &ciphertext).map_err(|_| FfiError::InvalidCiphertext)?;
        if metadata.kind != "text" {
            return Err(FfiError::NotText);
        }
        String::from_utf8(plaintext).map_err(|_| FfiError::InvalidText)
    }
}

#[uniffi::export]
pub fn core_version() -> String {
    ember_core::version().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ffi_roundtrips_text_through_core_format() {
        let crypto = EmberCrypto::generate();
        let ciphertext = crypto
            .encrypt_text("M9 bridge check".into(), 100, 200)
            .expect("encryption should succeed");
        assert_eq!(crypto.decrypt_text(ciphertext).unwrap(), "M9 bridge check");
    }

    #[test]
    fn ffi_rejects_expiry_before_creation() {
        let crypto = EmberCrypto::generate();
        assert!(matches!(
            crypto.encrypt_text("invalid".into(), 200, 100),
            Err(FfiError::InvalidExpiry)
        ));
    }
}
