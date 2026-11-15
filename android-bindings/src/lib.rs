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
    #[error("The provided secret key must be exactly 32 bytes")]
    InvalidKey,
    #[error("The link is invalid or uses an unsupported version")]
    InvalidLink,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct DecryptedDrop {
    pub kind: String,
    pub name: String,
    pub mime: String,
    pub size: u64,
    pub burn: bool,
    pub created_at: u64,
    pub expires_at: u64,
    pub content: Vec<u8>,
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

    /// Creates a key handle from existing 32-byte secret key material.
    #[uniffi::constructor]
    pub fn from_key_bytes(bytes: Vec<u8>) -> Result<Arc<Self>, FfiError> {
        if bytes.len() != 32 {
            return Err(FfiError::InvalidKey);
        }
        let mut key_arr = [0u8; 32];
        key_arr.copy_from_slice(&bytes);
        Ok(Arc::new(Self {
            key: SecretKey::from_bytes(key_arr),
        }))
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

    /// Encrypts a file drop payload.
    pub fn encrypt_file(
        &self,
        name: String,
        mime: String,
        content: Vec<u8>,
        burn: bool,
        created_at: u64,
        expires_at: u64,
    ) -> Result<Vec<u8>, FfiError> {
        if expires_at <= created_at {
            return Err(FfiError::InvalidExpiry);
        }

        let metadata = DropMetadata::new_file(
            name,
            mime,
            content.len() as u64,
            burn,
            created_at,
            expires_at,
        );
        encrypt_drop(&self.key, &metadata, &content, None).map_err(|_| FfiError::InvalidCiphertext)
    }

    /// Decrypts a full drop payload returning its metadata and plaintext content.
    pub fn decrypt_drop_payload(&self, ciphertext: Vec<u8>) -> Result<DecryptedDrop, FfiError> {
        let (metadata, plaintext) =
            decrypt_drop(&self.key, &ciphertext).map_err(|_| FfiError::InvalidCiphertext)?;
        Ok(DecryptedDrop {
            kind: metadata.kind,
            name: metadata.name,
            mime: metadata.mime,
            size: metadata.size,
            burn: metadata.burn,
            created_at: metadata.created,
            expires_at: metadata.expires,
            content: plaintext,
        })
    }
}

#[uniffi::export]
pub fn core_version() -> String {
    ember_core::version().to_string()
}

/// Validates the shared link format using the Rust core without exposing key bytes to Kotlin.
#[uniffi::export]
pub fn validate_link(link: String) -> Result<(), FfiError> {
    ember_core::parse_link(&link)
        .map(|_| ())
        .map_err(|_| FfiError::InvalidLink)
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
    fn ffi_roundtrips_file_through_core_format() {
        let crypto = EmberCrypto::generate();
        let ciphertext = crypto
            .encrypt_file(
                "test.bin".into(),
                "application/octet-stream".into(),
                vec![0xAA, 0xBB, 0xCC],
                true,
                100,
                200,
            )
            .expect("encryption should succeed");
        let decrypted = crypto.decrypt_drop_payload(ciphertext).unwrap();
        assert_eq!(decrypted.kind, "file");
        assert_eq!(decrypted.name, "test.bin");
        assert_eq!(decrypted.content, vec![0xAA, 0xBB, 0xCC]);
        assert!(decrypted.burn);
    }

    #[test]
    fn ffi_rejects_expiry_before_creation() {
        let crypto = EmberCrypto::generate();
        assert!(matches!(
            crypto.encrypt_text("invalid".into(), 200, 100),
            Err(FfiError::InvalidExpiry)
        ));
    }

    #[test]
    fn ffi_from_key_bytes_validates_key_length() {
        assert!(matches!(
            EmberCrypto::from_key_bytes(vec![0u8; 31]),
            Err(FfiError::InvalidKey)
        ));
        assert!(matches!(
            EmberCrypto::from_key_bytes(vec![0u8; 33]),
            Err(FfiError::InvalidKey)
        ));
        assert!(EmberCrypto::from_key_bytes(vec![0u8; 32]).is_ok());
    }

    #[test]
    fn ffi_rejects_unknown_link_version() {
        let link = "https://ember.example/d/AAAAAAAAAAAAAAAAAAAAAA#AgAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        assert!(matches!(
            validate_link(link.into()),
            Err(FfiError::InvalidLink)
        ));
    }
}
