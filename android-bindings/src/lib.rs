use std::sync::{Arc, Mutex};

use ember_core::{
    decrypt_drop, encrypt_drop, start_decrypt_stream, start_encrypt_stream, DropDecryptStream,
    DropEncryptStream, DropMetadata, LinkPayload, SecretKey, StripStatus,
};
use zeroize::Zeroize;

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
    #[error("A passphrase is required for this link")]
    MissingPassphrase,
    #[error("A passphrase was supplied for an unprotected link")]
    UnexpectedPassphrase,
    #[error("The passphrase could not unlock this link")]
    InvalidPassphrase,
    #[error("The proof-of-work challenge could not be solved within the work limit")]
    PowFailed,
    #[error("The proof-of-work difficulty is outside the supported range")]
    InvalidPowDifficulty,
    #[error("Metadata stripping is limited to 8 MiB to preserve bounded memory")]
    StripInputTooLarge,
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

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct DropMetadataInfo {
    pub kind: String,
    pub name: String,
    pub safe_name: String,
    pub mime: String,
    pub size: u64,
    pub burn: bool,
    pub created_at: u64,
    pub expires_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct DecryptedChunk {
    pub plaintext: Vec<u8>,
    pub is_final: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct MetadataStripReport {
    pub status: String,
    pub format: String,
    pub fields_removed: Vec<String>,
    pub reason: String,
    pub modified: bool,
    pub content: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct DropLinkInfo {
    pub host: String,
    pub blob_id: String,
    pub passphrase_protected: bool,
}

/// A key handle that keeps secret key bytes in Rust and zeroizes them on drop.
#[derive(uniffi::Object)]
pub struct EmberCrypto {
    key: SecretKey,
}

/// Rust-owned incremental encryptor. Payload chunks are capped by the shared core engine.
#[derive(uniffi::Object)]
pub struct EmberEncryptStream {
    stream: Mutex<DropEncryptStream>,
    part0: Mutex<Option<Vec<u8>>>,
}

/// Rust-owned incremental decryptor. Callers must keep output private until `finish` succeeds.
#[derive(uniffi::Object)]
pub struct EmberDecryptStream {
    stream: Mutex<DropDecryptStream>,
    metadata: DropMetadataInfo,
}

#[uniffi::export]
impl EmberEncryptStream {
    #[uniffi::constructor]
    pub fn new(
        crypto: Arc<EmberCrypto>,
        name: String,
        mime: String,
        size: u64,
        burn: bool,
        created_at: u64,
        expires_at: u64,
    ) -> Result<Arc<Self>, FfiError> {
        if expires_at <= created_at {
            return Err(FfiError::InvalidExpiry);
        }
        let metadata = DropMetadata::new_file(name, mime, size, burn, created_at, expires_at);
        let (stream, part0) = start_encrypt_stream(&crypto.key, &metadata)
            .map_err(|_| FfiError::InvalidCiphertext)?;
        Ok(Arc::new(Self {
            stream: Mutex::new(stream),
            part0: Mutex::new(Some(part0)),
        }))
    }

    /// Returns encrypted Message 0 exactly once.
    pub fn take_part0(&self) -> Result<Vec<u8>, FfiError> {
        self.part0
            .lock()
            .map_err(|_| FfiError::InvalidCiphertext)?
            .take()
            .ok_or(FfiError::InvalidCiphertext)
    }

    /// Encrypts one caller-provided chunk and wipes the input buffer before returning.
    pub fn update(&self, mut plaintext: Vec<u8>) -> Result<Vec<Vec<u8>>, FfiError> {
        let result = match self.stream.lock() {
            Ok(mut stream) => stream
                .update(&plaintext)
                .map_err(|_| FfiError::InvalidCiphertext),
            Err(_) => Err(FfiError::InvalidCiphertext),
        };
        plaintext.zeroize();
        result
    }

    pub fn finish(&self) -> Result<Vec<Vec<u8>>, FfiError> {
        self.stream
            .lock()
            .map_err(|_| FfiError::InvalidCiphertext)?
            .finish()
            .map_err(|_| FfiError::InvalidCiphertext)
    }
}

#[uniffi::export]
impl EmberDecryptStream {
    #[uniffi::constructor]
    pub fn new(crypto: Arc<EmberCrypto>, mut part0: Vec<u8>) -> Result<Arc<Self>, FfiError> {
        let result = start_decrypt_stream(&crypto.key, &part0);
        part0.zeroize();
        let (stream, metadata) = result.map_err(|_| FfiError::InvalidCiphertext)?;
        Ok(Arc::new(Self {
            stream: Mutex::new(stream),
            metadata: metadata_info(metadata),
        }))
    }

    pub fn metadata(&self) -> DropMetadataInfo {
        self.metadata.clone()
    }

    /// Authenticates one relay part and returns plaintext for an app-private temporary file.
    pub fn push_part(&self, mut ciphertext_part: Vec<u8>) -> Result<DecryptedChunk, FfiError> {
        let result = match self.stream.lock() {
            Ok(mut stream) => stream
                .push_part(&ciphertext_part)
                .map(|(plaintext, is_final)| DecryptedChunk {
                    plaintext,
                    is_final,
                })
                .map_err(|_| FfiError::InvalidCiphertext),
            Err(_) => Err(FfiError::InvalidCiphertext),
        };
        ciphertext_part.zeroize();
        result
    }

    pub fn finish(&self) -> Result<(), FfiError> {
        self.stream
            .lock()
            .map_err(|_| FfiError::InvalidCiphertext)?
            .finish()
            .map_err(|_| FfiError::InvalidCiphertext)
    }
}

fn metadata_info(metadata: DropMetadata) -> DropMetadataInfo {
    let safe_name = metadata.sanitized_file_name();
    DropMetadataInfo {
        kind: metadata.kind,
        name: metadata.name,
        safe_name,
        mime: metadata.mime,
        size: metadata.size,
        burn: metadata.burn,
        created_at: metadata.created,
        expires_at: metadata.expires,
    }
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
    pub fn from_key_bytes(mut bytes: Vec<u8>) -> Result<Arc<Self>, FfiError> {
        if bytes.len() != 32 {
            bytes.zeroize();
            return Err(FfiError::InvalidKey);
        }
        let mut key_arr = [0u8; 32];
        key_arr.copy_from_slice(&bytes);
        bytes.zeroize();
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
        self.encrypt_text_with_options(text, false, created_at, expires_at)
    }

    /// Encrypts text with an authenticated burn-after-view intent in metadata.
    pub fn encrypt_text_with_options(
        &self,
        mut text: String,
        burn: bool,
        created_at: u64,
        expires_at: u64,
    ) -> Result<Vec<u8>, FfiError> {
        if expires_at <= created_at {
            text.zeroize();
            return Err(FfiError::InvalidExpiry);
        }

        let metadata = DropMetadata::new_text(text.len() as u64, burn, created_at, expires_at);
        let result = encrypt_drop(&self.key, &metadata, text.as_bytes())
            .map_err(|_| FfiError::InvalidCiphertext);
        text.zeroize();
        result
    }

    /// Decrypts a text drop and rejects file drops instead of silently decoding them.
    pub fn decrypt_text(&self, ciphertext: Vec<u8>) -> Result<String, FfiError> {
        let (metadata, mut plaintext) =
            decrypt_drop(&self.key, &ciphertext).map_err(|_| FfiError::InvalidCiphertext)?;
        if metadata.kind != "text" {
            plaintext.zeroize();
            return Err(FfiError::NotText);
        }
        match String::from_utf8(plaintext) {
            Ok(text) => Ok(text),
            Err(error) => {
                let mut bytes = error.into_bytes();
                bytes.zeroize();
                Err(FfiError::InvalidText)
            }
        }
    }

    /// Encrypts a file drop payload.
    pub fn encrypt_file(
        &self,
        name: String,
        mime: String,
        mut content: Vec<u8>,
        burn: bool,
        created_at: u64,
        expires_at: u64,
    ) -> Result<Vec<u8>, FfiError> {
        if expires_at <= created_at {
            content.zeroize();
            return Err(FfiError::InvalidExpiry);
        }

        let size = content.len() as u64;
        let metadata = DropMetadata::new_file(name, mime, size, burn, created_at, expires_at);
        let result =
            encrypt_drop(&self.key, &metadata, &content).map_err(|_| FfiError::InvalidCiphertext);
        content.zeroize();
        result
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

    /// Builds an unprotected v1 link without returning the link key to Kotlin.
    pub fn build_link(&self, host: String, blob_id: String) -> Result<String, FfiError> {
        ember_core::build_link(&host, &blob_id, &LinkPayload::Unprotected(self.key.clone()))
            .map_err(|_| FfiError::InvalidLink)
    }

    /// Wraps this object's key in Rust and builds a passphrase-protected v1 link.
    pub fn build_protected_link(
        &self,
        host: String,
        blob_id: String,
        mut passphrase: String,
    ) -> Result<String, FfiError> {
        let wrapped_result = ember_core::wrap_key(&self.key, &passphrase, None);
        passphrase.zeroize();
        let mut payload =
            LinkPayload::Protected(wrapped_result.map_err(|_| FfiError::InvalidPassphrase)?);
        let result = ember_core::build_link(&host, &blob_id, &payload);
        zeroize_link_payload(&mut payload);
        result.map_err(|_| FfiError::InvalidLink)
    }

    /// Parses a link and creates a Rust-owned key handle; link and passphrase buffers are wiped.
    #[uniffi::constructor]
    pub fn from_link(
        mut link: String,
        mut passphrase: Option<String>,
    ) -> Result<std::sync::Arc<Self>, FfiError> {
        let parsed_result = ember_core::parse_link(&link);
        link.zeroize();
        let parsed = match parsed_result {
            Ok(parsed) => parsed,
            Err(_) => {
                zeroize_passphrase(&mut passphrase);
                return Err(FfiError::InvalidLink);
            }
        };

        let key = match parsed.payload {
            LinkPayload::Unprotected(key) => {
                if passphrase.is_some() {
                    zeroize_passphrase(&mut passphrase);
                    return Err(FfiError::UnexpectedPassphrase);
                }
                key
            }
            LinkPayload::Protected(mut wrapped) => {
                let Some(mut phrase) = passphrase.take() else {
                    wrapped.zeroize();
                    return Err(FfiError::MissingPassphrase);
                };
                let key_result = ember_core::unwrap_key(&wrapped, &phrase);
                phrase.zeroize();
                wrapped.zeroize();
                key_result.map_err(|_| FfiError::InvalidPassphrase)?
            }
        };

        Ok(std::sync::Arc::new(Self { key }))
    }
}

#[uniffi::export]
pub fn core_version() -> String {
    ember_core::version().to_string()
}

/// Validates the shared link format using the Rust core without exposing key bytes to Kotlin.
#[uniffi::export]
pub fn validate_link(mut link: String) -> Result<(), FfiError> {
    let parsed = ember_core::parse_link(&link);
    link.zeroize();
    parsed
        .map(|mut parsed| {
            zeroize_link_payload(&mut parsed.payload);
        })
        .map_err(|_| FfiError::InvalidLink)
}

/// Parses only the non-secret routing fields needed to contact the relay.
#[uniffi::export]
pub fn inspect_link(mut link: String) -> Result<DropLinkInfo, FfiError> {
    let parsed_result = ember_core::parse_link(&link);
    link.zeroize();
    let parsed = parsed_result.map_err(|_| FfiError::InvalidLink)?;
    let mut payload = parsed.payload;
    let passphrase_protected = matches!(&payload, LinkPayload::Protected(_));
    zeroize_link_payload(&mut payload);
    Ok(DropLinkInfo {
        host: parsed.host,
        blob_id: parsed.blob_id,
        passphrase_protected,
    })
}

/// Solves the server-issued hashcash challenge with a bounded amount of device work.
#[uniffi::export]
pub fn solve_pow(challenge: String, difficulty: u32) -> Result<u64, FfiError> {
    if difficulty > 32 {
        return Err(FfiError::InvalidPowDifficulty);
    }
    ember_core::solve_pow(challenge.as_bytes(), difficulty, Some(5_000_000))
        .map_err(|_| FfiError::PowFailed)
}

/// Runs the shared Rust metadata stripper with an Android memory cap and structured feedback.
#[uniffi::export]
pub fn strip_file_metadata(
    mut content: Vec<u8>,
    filename_hint: String,
) -> Result<MetadataStripReport, FfiError> {
    if content.len() > 8 * 1024 * 1024 {
        content.zeroize();
        return Err(FfiError::StripInputTooLarge);
    }
    let result = ember_core::strip_metadata(&content, Some(&filename_hint));
    content.zeroize();
    let result = result.map_err(|_| FfiError::InvalidCiphertext)?;
    let (status, format, fields_removed, reason) = match result.status {
        StripStatus::Stripped {
            format,
            fields_removed,
        } => (
            "stripped",
            format.to_string(),
            fields_removed,
            String::new(),
        ),
        StripStatus::NoMetadataFound { format } => {
            ("clean", format.to_string(), Vec::new(), String::new())
        }
        StripStatus::UnsupportedFormat { format, reason } => {
            ("unsupported", format.to_string(), Vec::new(), reason)
        }
    };
    Ok(MetadataStripReport {
        status: status.into(),
        format,
        fields_removed,
        reason,
        modified: result.modified,
        content: result.data,
    })
}

fn zeroize_passphrase(passphrase: &mut Option<String>) {
    if let Some(value) = passphrase.as_mut() {
        value.zeroize();
    }
}

fn zeroize_link_payload(payload: &mut LinkPayload) {
    if let LinkPayload::Protected(wrapped) = payload {
        wrapped.zeroize();
    }
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
    fn ffi_streaming_file_roundtrip_keeps_parts_bounded_and_metadata_authenticated() {
        let sender = EmberCrypto::generate();
        let receiver_key = sender.key.clone();
        let content: Vec<u8> = (0..(1_048_576 + 37))
            .map(|index| (index % 251) as u8)
            .collect();
        let stream = EmberEncryptStream::new(
            Arc::clone(&sender),
            "../payload.bin".into(),
            "application/octet-stream".into(),
            content.len() as u64,
            true,
            100,
            200,
        )
        .unwrap();

        let mut encrypted_parts = vec![stream.take_part0().unwrap()];
        assert!(stream.take_part0().is_err());
        for chunk in content.chunks(64 * 1024) {
            encrypted_parts.extend(stream.update(chunk.to_vec()).unwrap());
        }
        encrypted_parts.extend(stream.finish().unwrap());
        assert!(encrypted_parts.iter().all(|part| part.len() <= 1_048_593));

        let decryptor = EmberDecryptStream::new(
            Arc::new(EmberCrypto { key: receiver_key }),
            encrypted_parts.remove(0),
        )
        .unwrap();
        let metadata = decryptor.metadata();
        assert_eq!(metadata.kind, "file");
        assert_eq!(metadata.name, "../payload.bin");
        assert_eq!(metadata.safe_name, "payload.bin");
        assert_eq!(metadata.size, content.len() as u64);
        assert!(metadata.burn);

        let mut recovered = Vec::new();
        let mut saw_final = false;
        for part in encrypted_parts {
            let chunk = decryptor.push_part(part).unwrap();
            recovered.extend(chunk.plaintext);
            saw_final = chunk.is_final;
        }
        assert!(saw_final);
        decryptor.finish().unwrap();
        assert_eq!(recovered, content);
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
    fn ffi_text_options_authenticate_burn_intent() {
        let crypto = EmberCrypto::generate();
        let ciphertext = crypto
            .encrypt_text_with_options("burn intent".into(), true, 100, 200)
            .unwrap();
        let (metadata, plaintext) = ember_core::decrypt_drop(&crypto.key, &ciphertext).unwrap();
        assert!(metadata.burn);
        assert_eq!(plaintext, b"burn intent");
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

    #[test]
    fn ffi_builds_and_opens_unprotected_link_without_exporting_key() {
        let crypto = EmberCrypto::generate();
        let ciphertext = crypto
            .encrypt_text("link roundtrip".into(), 100, 200)
            .unwrap();
        let link = crypto
            .build_link("ember.example.com".into(), "AAAAAAAAAAAAAAAAAAAAAA".into())
            .unwrap();
        let info = inspect_link(link.clone()).unwrap();
        assert_eq!(info.host, "ember.example.com");
        assert_eq!(info.blob_id, "AAAAAAAAAAAAAAAAAAAAAA");
        assert!(!info.passphrase_protected);

        let opened = EmberCrypto::from_link(link, None).unwrap();
        assert_eq!(opened.decrypt_text(ciphertext).unwrap(), "link roundtrip");
    }

    #[test]
    fn ffi_builds_and_opens_passphrase_protected_link() {
        let crypto = EmberCrypto::generate();
        let ciphertext = crypto
            .encrypt_text("protected link roundtrip".into(), 100, 200)
            .unwrap();
        let link = crypto
            .build_protected_link(
                "ember.example.com".into(),
                "AAAAAAAAAAAAAAAAAAAAAA".into(),
                "correct horse battery staple".into(),
            )
            .unwrap();
        assert!(inspect_link(link.clone()).unwrap().passphrase_protected);
        assert!(matches!(
            EmberCrypto::from_link(link.clone(), None),
            Err(FfiError::MissingPassphrase)
        ));
        assert!(matches!(
            EmberCrypto::from_link(link.clone(), Some("wrong".into())),
            Err(FfiError::InvalidPassphrase)
        ));

        let opened =
            EmberCrypto::from_link(link, Some("correct horse battery staple".into())).unwrap();
        assert_eq!(
            opened.decrypt_text(ciphertext).unwrap(),
            "protected link roundtrip"
        );
    }

    #[test]
    fn ffi_solves_bounded_pow_challenge() {
        let challenge = "m10-test-challenge";
        let nonce = solve_pow(challenge.into(), 8).unwrap();
        assert!(ember_core::verify_pow(challenge.as_bytes(), nonce, 8));
        assert!(matches!(
            solve_pow(challenge.into(), 33),
            Err(FfiError::InvalidPowDifficulty)
        ));
    }

    #[test]
    fn ffi_metadata_strip_returns_removed_fields_and_content() {
        let mut jpeg = vec![0xFF, 0xD8, 0xFF, 0xE1, 0, 14];
        jpeg.extend_from_slice(b"Exif\0\0secret");
        jpeg.extend_from_slice(&[0xFF, 0xDA, 0, 8, 1, 1, 0, 0, 1, 2, 3, 0xFF, 0xD9]);

        let report = strip_file_metadata(jpeg, "photo.jpg".into()).unwrap();
        assert_eq!(report.status, "stripped");
        assert_eq!(report.format, "JPEG Image");
        assert!(report.modified);
        assert!(report
            .fields_removed
            .iter()
            .any(|field| field.contains("EXIF metadata")));
        assert!(!report.content.windows(6).any(|window| window == b"secret"));
    }

    #[test]
    fn ffi_metadata_strip_reports_unsupported_formats_without_modifying_bytes() {
        let bytes = b"opaque payload".to_vec();
        let report = strip_file_metadata(bytes.clone(), "payload.bin".into()).unwrap();
        assert_eq!(report.status, "unsupported");
        assert!(!report.modified);
        assert!(!report.reason.is_empty());
        assert_eq!(report.content, bytes);
    }

    #[test]
    fn ffi_metadata_strip_rejects_inputs_over_android_budget() {
        assert!(matches!(
            strip_file_metadata(vec![0; 8 * 1024 * 1024 + 1], "large.jpg".into()),
            Err(FfiError::StripInputTooLarge)
        ));
    }
}
