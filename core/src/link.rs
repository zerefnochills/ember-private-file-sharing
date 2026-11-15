//! Link building, parsing, and base64url payload handling.
//!
//! Follows `docs/protocol-spec.md` section 5.2:
//! `https://<host>/d/<blobId>#<payload>`

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;

use crate::error::EmberError;
use crate::key::SecretKey;
use ember_protocol::{
    BLOB_ID_BYTES, FLAG_PASSPHRASE_PROTECTED, LINK_PAYLOAD_UNWRAPPED_LEN, LINK_PAYLOAD_WRAPPED_LEN,
    LINK_VERSION_1,
};

/// The secret payload encoded in the link fragment after `#`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkPayload {
    /// Raw unencrypted master key K (34 bytes raw: version + flags + K).
    Unprotected(SecretKey),
    /// Passphrase-wrapped master key (95 bytes raw: version + flags + ops + mem + salt + nonce + wrapped_K).
    Protected(Vec<u8>),
}

/// Parsed components of an Ember drop link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedLink {
    /// Hostname and optional port (e.g. "ember.example.com" or "localhost:8080").
    pub host: String,
    /// Server-assigned 22-character base64url blob identifier.
    pub blob_id: String,
    /// Fragment payload.
    pub payload: LinkPayload,
}

/// Generates a random 16-byte blob ID encoded as 22 base64url characters.
pub fn generate_blob_id() -> String {
    use dryoc::rng::copy_randombytes;
    let mut id = [0u8; BLOB_ID_BYTES];
    copy_randombytes(&mut id);
    URL_SAFE_NO_PAD.encode(id)
}

/// Validates whether a string is a valid 22-character base64url 16-byte blob ID.
pub fn validate_blob_id(blob_id: &str) -> Result<(), EmberError> {
    if blob_id.len() != 22 {
        return Err(EmberError::InvalidLink(format!(
            "Blob ID length is {} (expected 22 base64url characters)",
            blob_id.len()
        )));
    }

    let decoded = URL_SAFE_NO_PAD
        .decode(blob_id.as_bytes())
        .map_err(|e| EmberError::InvalidLink(format!("Blob ID is not valid base64url: {e}")))?;

    if decoded.len() != BLOB_ID_BYTES {
        return Err(EmberError::InvalidLink(format!(
            "Decoded blob ID is {} bytes (expected 16)",
            decoded.len()
        )));
    }

    Ok(())
}

/// Serializes a LinkPayload into raw bytes.
pub fn payload_to_bytes(payload: &LinkPayload) -> Vec<u8> {
    match payload {
        LinkPayload::Unprotected(key) => {
            let mut bytes = vec![0u8; LINK_PAYLOAD_UNWRAPPED_LEN];
            bytes[0] = LINK_VERSION_1;
            bytes[1] = 0x00; // flags = 0
            bytes[2..34].copy_from_slice(key.as_bytes());
            bytes
        }
        LinkPayload::Protected(wrapped_bytes) => wrapped_bytes.clone(),
    }
}

/// Encodes a LinkPayload into a URL-safe unpadded base64 string.
pub fn encode_payload(payload: &LinkPayload) -> String {
    let bytes = payload_to_bytes(payload);
    URL_SAFE_NO_PAD.encode(&bytes)
}

/// Decodes and validates a LinkPayload from a base64url string.
pub fn decode_payload(payload_b64: &str) -> Result<LinkPayload, EmberError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(payload_b64.as_bytes())
        .map_err(|e| EmberError::InvalidLink(format!("Base64url decoding failed: {e}")))?;

    decode_payload_bytes(&bytes)
}

/// Decodes and validates a LinkPayload from raw bytes.
pub fn decode_payload_bytes(bytes: &[u8]) -> Result<LinkPayload, EmberError> {
    if bytes.is_empty() {
        return Err(EmberError::InvalidLink("Empty payload".to_string()));
    }

    let version = bytes[0];
    if version != LINK_VERSION_1 {
        return Err(EmberError::UnsupportedVersion(version));
    }

    if bytes.len() < 2 {
        return Err(EmberError::InvalidLink(
            "Truncated payload header".to_string(),
        ));
    }

    let flags = bytes[1];
    // Bits 1-7 MUST be 0
    if flags & !FLAG_PASSPHRASE_PROTECTED != 0 {
        return Err(EmberError::UnsupportedFlags(flags));
    }

    let is_protected = (flags & FLAG_PASSPHRASE_PROTECTED) != 0;

    if !is_protected {
        if bytes.len() != LINK_PAYLOAD_UNWRAPPED_LEN {
            return Err(EmberError::InvalidLink(format!(
                "Unprotected payload length is {} bytes (expected 34)",
                bytes.len()
            )));
        }
        let mut key = [0u8; 32];
        key.copy_from_slice(&bytes[2..34]);
        Ok(LinkPayload::Unprotected(SecretKey::from_bytes(key)))
    } else {
        if bytes.len() != LINK_PAYLOAD_WRAPPED_LEN {
            return Err(EmberError::InvalidLink(format!(
                "Passphrase-protected payload length is {} bytes (expected 95)",
                bytes.len()
            )));
        }
        Ok(LinkPayload::Protected(bytes.to_vec()))
    }
}

/// Constructs a full Ember share link: `https://<host>/d/<blob_id>#<payload>`.
pub fn build_link(host: &str, blob_id: &str, payload: &LinkPayload) -> Result<String, EmberError> {
    validate_blob_id(blob_id)?;
    let clean_host = host.trim_matches('/');
    let payload_b64 = encode_payload(payload);
    Ok(format!("https://{clean_host}/d/{blob_id}#{payload_b64}"))
}

/// Parses an Ember share link URL.
pub fn parse_link(url: &str) -> Result<ParsedLink, EmberError> {
    let fragment_idx = url
        .find('#')
        .ok_or_else(|| EmberError::InvalidLink("Link is missing '#' fragment".to_string()))?;

    let base_part = &url[..fragment_idx];
    let fragment = &url[fragment_idx + 1..];

    if fragment.is_empty() {
        return Err(EmberError::InvalidLink("Fragment is empty".to_string()));
    }

    let stripped = base_part
        .strip_prefix("https://")
        .or_else(|| base_part.strip_prefix("http://"))
        .ok_or_else(|| {
            EmberError::InvalidLink("Link must begin with https:// (or http:// in dev)".to_string())
        })?;

    let slash_idx = stripped
        .find('/')
        .ok_or_else(|| EmberError::InvalidLink("Missing path in link".to_string()))?;

    let host = stripped[..slash_idx].to_string();
    let path = &stripped[slash_idx..];

    let blob_id = path
        .strip_prefix("/d/")
        .ok_or_else(|| EmberError::InvalidLink("Link path must begin with '/d/'".to_string()))?;

    validate_blob_id(blob_id)?;

    let payload = decode_payload(fragment)?;

    Ok(ParsedLink {
        host,
        blob_id: blob_id.to_string(),
        payload,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unprotected_link_roundtrip() {
        let key = SecretKey::generate();
        let blob_id = generate_blob_id();
        let payload = LinkPayload::Unprotected(key.clone());

        let link = build_link("ember.example.com", &blob_id, &payload).unwrap();
        assert!(link.starts_with("https://ember.example.com/d/"));

        let parsed = parse_link(&link).unwrap();
        assert_eq!(parsed.host, "ember.example.com");
        assert_eq!(parsed.blob_id, blob_id);
        assert_eq!(parsed.payload, LinkPayload::Unprotected(key));
    }

    #[test]
    fn test_protected_link_roundtrip() {
        let key = SecretKey::generate();
        let blob_id = generate_blob_id();
        let wrapped = crate::wrap::wrap_key(
            &key,
            "secret",
            Some(crate::wrap::WrapParams {
                ops: 1,
                mem_kib: 8192,
            }),
            None,
            None,
        )
        .unwrap();

        let payload = LinkPayload::Protected(wrapped.clone());
        let link = build_link("ember.example.com", &blob_id, &payload).unwrap();

        let parsed = parse_link(&link).unwrap();
        assert_eq!(parsed.payload, LinkPayload::Protected(wrapped));
    }

    #[test]
    fn test_reject_unsupported_version() {
        let mut bytes = vec![0x02, 0x00]; // version 2
        bytes.extend_from_slice(&[0u8; 32]);
        let b64 = URL_SAFE_NO_PAD.encode(&bytes);
        assert_eq!(decode_payload(&b64), Err(EmberError::UnsupportedVersion(2)));
    }

    #[test]
    fn test_reject_unsupported_flags() {
        let mut bytes = vec![0x01, 0x02]; // unknown flag bit 1
        bytes.extend_from_slice(&[0u8; 32]);
        let b64 = URL_SAFE_NO_PAD.encode(&bytes);
        assert_eq!(
            decode_payload(&b64),
            Err(EmberError::UnsupportedFlags(0x02))
        );
    }
}
