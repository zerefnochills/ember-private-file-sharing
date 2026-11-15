//! Ember core: encryption, link format, expiry rules, metadata stripping.

pub mod error;
pub mod key;
pub mod link;
pub mod metadata;
pub mod padding;
pub mod stream;
pub mod wrap;

pub use error::EmberError;
pub use key::SecretKey;
pub use link::{
    build_link, decode_payload, decode_payload_bytes, encode_payload, generate_blob_id, parse_link,
    payload_to_bytes, validate_blob_id, LinkPayload, ParsedLink,
};
pub use metadata::DropMetadata;
pub use padding::{calculate_padded_len, verify_padding};
pub use stream::{decrypt_drop, decrypt_drop_parts, encrypt_drop, encrypt_drop_to_parts};
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
