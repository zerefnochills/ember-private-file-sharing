//! Encrypted drop metadata record (Message 0).
//!
//! Follows `docs/protocol-spec.md` section 4.1:
//! - Exactly 4096 bytes of plaintext.
//! - Layout: `u32 big-endian json_len` || `json (UTF-8)` || zero padding up to 4096.
//! - `json_len` MUST be at most 4092.

use serde::{Deserialize, Serialize};

use crate::error::EmberError;
use ember_protocol::METADATA_PLAIN_LEN;

/// Metadata describing an encrypted drop payload.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DropMetadata {
    /// Metadata format version (must be 1).
    pub v: u32,
    /// Kind of content: "file" or "text".
    pub kind: String,
    /// Original file name (UTF-8, at most 255 bytes; empty string for text).
    pub name: String,
    /// Declared MIME type (at most 127 bytes, informational only).
    pub mime: String,
    /// True plaintext size in bytes before padding.
    pub size: u64,
    /// Sender's intent: burn after first completed view.
    pub burn: bool,
    /// Unix timestamp in seconds when the drop was created.
    pub created: u64,
    /// Unix timestamp in seconds when the sender expects expiry.
    pub expires: u64,
}

impl DropMetadata {
    /// Constructs metadata for a file drop.
    pub fn new_file(
        name: impl Into<String>,
        mime: impl Into<String>,
        size: u64,
        burn: bool,
        created: u64,
        expires: u64,
    ) -> Self {
        Self {
            v: 1,
            kind: "file".to_string(),
            name: name.into(),
            mime: mime.into(),
            size,
            burn,
            created,
            expires,
        }
    }

    /// Constructs metadata for a plain text drop.
    pub fn new_text(size: u64, burn: bool, created: u64, expires: u64) -> Self {
        Self {
            v: 1,
            kind: "text".to_string(),
            name: String::new(),
            mime: "text/plain; charset=utf-8".to_string(),
            size,
            burn,
            created,
            expires,
        }
    }

    /// Returns a sanitized filename safe for saving to disk, stripping path components and control chars.
    pub fn sanitized_file_name(&self) -> String {
        if self.kind == "text" || self.name.trim().is_empty() {
            return if self.kind == "text" {
                "message.txt".to_string()
            } else {
                "drop.bin".to_string()
            };
        }

        // Split on BOTH separators ourselves: `Path::file_name` only honours the separators of
        // the platform running the code, but the sender can be on any platform.
        let base_name = self
            .name
            .rsplit(['/', '\\'])
            .find(|part| !part.is_empty())
            .unwrap_or("");

        let sanitized: String = base_name
            .chars()
            .map(|c| {
                if c.is_control() || matches!(c, ':' | '<' | '>' | '"' | '|' | '?' | '*') {
                    '_'
                } else {
                    c
                }
            })
            .collect();

        let trimmed = sanitized.trim_matches(|c: char| c == '.' || c.is_whitespace());
        if trimmed.is_empty() {
            return "drop.bin".to_string();
        }

        // Windows reserved device names (with or without extension) become ordinary names.
        let stem = trimmed.split('.').next().unwrap_or("").to_ascii_uppercase();
        let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || (stem.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && stem.as_bytes()[3].is_ascii_digit()
                && stem.as_bytes()[3] != b'0');
        if reserved {
            format!("_{trimmed}")
        } else {
            trimmed.to_string()
        }
    }

    /// Validates metadata constraints according to section 4.1.
    pub fn validate(&self) -> Result<(), EmberError> {
        if self.v != 1 {
            return Err(EmberError::UnsupportedVersion(self.v as u8));
        }

        if self.kind != "file" && self.kind != "text" {
            return Err(EmberError::InvalidMetadata(format!(
                "Invalid kind: '{}' (must be 'file' or 'text')",
                self.kind
            )));
        }

        if self.size > ember_protocol::MAX_CONTENT_BYTES {
            return Err(EmberError::InvalidMetadata(format!(
                "Content size {} exceeds the maximum of {} bytes",
                self.size,
                ember_protocol::MAX_CONTENT_BYTES
            )));
        }

        if self.name.len() > 255 {
            return Err(EmberError::InvalidMetadata(
                "File name exceeds maximum 255 bytes".to_string(),
            ));
        }

        if self.mime.len() > 127 {
            return Err(EmberError::InvalidMetadata(
                "MIME type exceeds maximum 127 bytes".to_string(),
            ));
        }

        Ok(())
    }

    /// Serializes metadata into the exact 4096-byte plaintext record for Message 0.
    pub fn encode_record(&self) -> Result<[u8; METADATA_PLAIN_LEN], EmberError> {
        self.validate()?;

        let json_bytes = serde_json::to_vec(self)
            .map_err(|e| EmberError::InvalidMetadata(format!("JSON serialization failed: {e}")))?;

        if json_bytes.len() > METADATA_PLAIN_LEN - 4 {
            return Err(EmberError::InvalidMetadata(format!(
                "Serialized metadata JSON ({} bytes) exceeds maximum 4092 bytes",
                json_bytes.len()
            )));
        }

        let mut record = [0u8; METADATA_PLAIN_LEN];
        let len_be = (json_bytes.len() as u32).to_be_bytes();
        record[0..4].copy_from_slice(&len_be);
        record[4..4 + json_bytes.len()].copy_from_slice(&json_bytes);

        // Bytes from 4 + json_bytes.len() to 4096 remain 0 as required
        Ok(record)
    }

    /// Deserializes and validates a 4096-byte plaintext record from Message 0.
    pub fn decode_record(record: &[u8]) -> Result<Self, EmberError> {
        if record.len() != METADATA_PLAIN_LEN {
            return Err(EmberError::InvalidMetadata(format!(
                "Metadata record length is {} bytes (expected 4096)",
                record.len()
            )));
        }

        let json_len = u32::from_be_bytes([record[0], record[1], record[2], record[3]]) as usize;
        if json_len > METADATA_PLAIN_LEN - 4 {
            return Err(EmberError::InvalidMetadata(format!(
                "Declared JSON length {} exceeds limit 4092",
                json_len
            )));
        }

        // Verify zero padding after JSON bytes
        for &byte in record[4 + json_len..METADATA_PLAIN_LEN].iter() {
            if byte != 0 {
                return Err(EmberError::InvalidPadding);
            }
        }

        let meta: DropMetadata = serde_json::from_slice(&record[4..4 + json_len])
            .map_err(|e| EmberError::InvalidMetadata(format!("JSON parse failed: {e}")))?;

        meta.validate()?;
        Ok(meta)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_decode_roundtrip() {
        let meta = DropMetadata::new_file(
            "report.pdf",
            "application/pdf",
            123_456,
            true,
            1_700_000_000,
            1_700_043_200,
        );

        let record = meta.encode_record().unwrap();
        assert_eq!(record.len(), 4096);

        let decoded = DropMetadata::decode_record(&record).unwrap();
        assert_eq!(meta, decoded);
    }

    #[test]
    fn test_decode_invalid_padding_fails() {
        let meta = DropMetadata::new_text(50, false, 1_000, 2_000);
        let mut record = meta.encode_record().unwrap();
        // Corrupt a byte in the zero-padding region
        record[4000] = 0xFF;
        assert_eq!(
            DropMetadata::decode_record(&record),
            Err(EmberError::InvalidPadding)
        );
    }

    #[test]
    fn test_sanitize_file_name() {
        let meta = DropMetadata::new_file("../../etc/passwd", "text/plain", 10, false, 0, 0);
        assert_eq!(meta.sanitized_file_name(), "passwd");

        let meta_traversal = DropMetadata::new_file(
            "C:\\Windows\\System32\\cmd.exe",
            "application/octet-stream",
            10,
            false,
            0,
            0,
        );
        assert_eq!(meta_traversal.sanitized_file_name(), "cmd.exe");
    }

    #[test]
    fn test_sanitize_file_name_cross_platform_and_reserved() {
        let cases = [
            ("..\\..\\evil.exe", "evil.exe"),
            ("a/b\\c.txt", "c.txt"),
            ("dir/", "dir"),
            ("CON", "_CON"),
            ("nul.txt", "_nul.txt"),
            ("com1.log", "_com1.log"),
            ("COM0.txt", "COM0.txt"),
            ("a:b*c?.txt", "a_b_c_.txt"),
            ("...", "drop.bin"),
            ("/", "drop.bin"),
        ];
        for (input, expected) in cases {
            let meta = DropMetadata::new_file(input, "text/plain", 1, false, 0, 0);
            assert_eq!(meta.sanitized_file_name(), expected, "input {input:?}");
        }
    }

    #[test]
    fn test_validate_rejects_absurd_size() {
        let meta = DropMetadata::new_file("a.bin", "x/y", u64::MAX, false, 0, 0);
        assert!(meta.validate().is_err());
        let meta = DropMetadata::new_file(
            "a.bin",
            "x/y",
            ember_protocol::MAX_CONTENT_BYTES + 1,
            false,
            0,
            0,
        );
        assert!(meta.validate().is_err());
        let ok = DropMetadata::new_file(
            "a.bin",
            "x/y",
            ember_protocol::MAX_CONTENT_BYTES,
            false,
            0,
            0,
        );
        assert!(ok.validate().is_ok());
    }
}
