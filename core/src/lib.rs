//! Ember core: encryption, link format, expiry rules, metadata stripping.

pub mod error;
pub mod key;
pub mod link;
pub mod metadata;
pub mod padding;
pub mod pow;
pub mod report;
pub mod stream;
pub mod strip;
pub mod thread;
pub mod wrap;

pub use error::EmberError;
pub use key::SecretKey;
pub use link::{
    build_link, decode_payload, decode_payload_bytes, encode_payload, generate_blob_id, parse_link,
    payload_to_bytes, validate_blob_id, LinkPayload, ParsedLink,
};
pub use metadata::DropMetadata;
pub use padding::{calculate_padded_len, verify_padding};
pub use pow::{count_leading_zero_bits, hash_pow, solve_pow, verify_pow, PowChallengeToken};
pub use report::{
    generate_reviewer_keypair, key_to_hex, parse_hex_key, seal_report_field, unseal_report_field,
    SEALED_PREFIX,
};
pub use stream::{
    decrypt_drop, decrypt_drop_parts, encrypt_drop, encrypt_drop_to_parts, start_decrypt_stream,
    start_encrypt_stream, DropDecryptStream, DropEncryptStream,
};
#[cfg(feature = "test-hooks")]
pub use stream::{
    encrypt_drop_to_parts_with_header, encrypt_drop_with_header, start_encrypt_stream_with_header,
};
pub use strip::{detect_format, strip_metadata, FileFormat, MetadataStripResult, StripStatus};
pub use thread::{
    derive_thread_psk, generate_handle, EncryptedMessagePayload, HandshakeInitiator,
    HandshakeResponder, HandshakeSessionKeys, MailboxCapability, ReceivingRatchet, SendingRatchet,
    ThreadMessage, ThreadReceiver, ThreadSender, ThreadSession,
};
#[cfg(feature = "test-hooks")]
pub use wrap::wrap_key_with_injected;
pub use wrap::{unwrap_key, wrap_key, WrapParams};

/// Crate version, exposed so clients can report which core they embed.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_not_empty() {
        assert!(!version().is_empty());
    }
}
