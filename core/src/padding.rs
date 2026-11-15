//! Padding rules and verification for Ember ciphertext drops.
//!
//! Follows `docs/protocol-spec.md` section 6:
//! Content is padded to 64 KiB bands (minimum 4096 bytes) to obscure precise file length.

use crate::error::EmberError;
use ember_protocol::{MIN_PADDING_BYTES, PADDING_BAND_BYTES};

/// Calculates the padded length for a given content plaintext size.
pub fn calculate_padded_len(size: u64) -> u64 {
    if size <= MIN_PADDING_BYTES {
        MIN_PADDING_BYTES
    } else {
        size.div_ceil(PADDING_BAND_BYTES) * PADDING_BAND_BYTES
    }
}

/// Verifies that all bytes in `padded_content` beyond `size` are zeroes.
pub fn verify_padding(padded_content: &[u8], size: usize) -> Result<(), EmberError> {
    if size > padded_content.len() {
        return Err(EmberError::SizeMismatch);
    }

    for &byte in padded_content[size..].iter() {
        if byte != 0 {
            return Err(EmberError::InvalidPadding);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spec_reference_table_padding() {
        // Spec section 5.1 reference numbers:
        assert_eq!(calculate_padded_len(0), 4096);
        assert_eq!(calculate_padded_len(1), 4096);
        assert_eq!(calculate_padded_len(4096), 4096);
        assert_eq!(calculate_padded_len(4097), 65536);
        assert_eq!(calculate_padded_len(65536), 65536);
        assert_eq!(calculate_padded_len(65537), 131072);
        assert_eq!(calculate_padded_len(1_048_576), 1_048_576);
        assert_eq!(calculate_padded_len(1_048_577), 1_114_112);
        assert_eq!(calculate_padded_len(52_428_800), 52_428_800);
    }

    #[test]
    fn test_verify_padding_valid() {
        let mut buf = vec![42u8; 10];
        buf.extend(vec![0u8; 4086]);
        assert!(verify_padding(&buf, 10).is_ok());
    }

    #[test]
    fn test_verify_padding_invalid() {
        let mut buf = vec![42u8; 10];
        buf.extend(vec![0u8; 20]);
        buf[15] = 1;
        assert_eq!(verify_padding(&buf, 10), Err(EmberError::InvalidPadding));
    }
}
