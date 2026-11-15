//! Error types for Ember core operations.

use thiserror::Error;

/// Core error types for encryption, decryption, parsing, and validation.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum EmberError {
    /// Cryptographic tag verification or authentication failed (tampering detected).
    #[error("Authentication failed: invalid tag or ciphertext corrupted")]
    Auth,

    /// Cipherstream ended prematurely without the required FINAL tag.
    #[error("Cipherstream ended prematurely without FINAL tag")]
    NoFinalTag,

    /// Extraneous bytes detected after the FINAL stream tag.
    #[error("Unexpected trailing bytes after FINAL chunk")]
    TrailingBytes,

    /// Padding bytes after the declared content size are non-zero.
    #[error("Invalid padding: non-zero byte encountered in padding region")]
    InvalidPadding,

    /// Stated plaintext size exceeds actual decrypted content length.
    #[error("Size mismatch: stated content size exceeds decrypted content length")]
    SizeMismatch,

    /// Metadata record was malformed, invalid JSON, or violated protocol constraints.
    #[error("Invalid metadata record: {0}")]
    InvalidMetadata(String),

    /// Link or payload was malformed or failed base64url decoding.
    #[error("Invalid link: {0}")]
    InvalidLink(String),

    /// Unsupported protocol or link version.
    #[error("Unsupported version: {0} (upgrade required)")]
    UnsupportedVersion(u8),

    /// Unsupported or reserved flag bits are set.
    #[error("Unsupported flag bits: 0x{0:02x}")]
    UnsupportedFlags(u8),

    /// Passphrase is required to open this link, but none was supplied.
    #[error("Passphrase required to decrypt this drop")]
    PassphraseRequired,

    /// A passphrase was provided, but the link is not passphrase-protected.
    #[error("Passphrase provided for an unprotected drop")]
    PassphraseNotAllowed,

    /// Argon2id parameters fall outside the permitted bounds.
    #[error("Invalid passphrase parameters: {0}")]
    InvalidPassphraseParams(String),

    /// Underlying cryptographic library error.
    #[error("Cryptographic operation failed: {0}")]
    Crypto(String),

    /// I/O error occurred during file or stream processing.
    #[error("I/O error: {0}")]
    Io(String),

    /// Proof-of-work challenge or solution was invalid.
    #[error("Proof of work error: {0}")]
    InvalidPow(String),

    /// Challenge or resource has expired.
    #[error("Resource expired: {0}")]
    Expired(String),
}

impl From<std::io::Error> for EmberError {
    fn from(err: std::io::Error) -> Self {
        EmberError::Io(err.to_string())
    }
}
