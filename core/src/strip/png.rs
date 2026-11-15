//! PNG metadata stripper.
//!
//! Strips ancillary metadata chunks:
//! - `eXIf`: EXIF metadata (GPS coordinates, camera model, timestamps)
//! - `tEXt`: Latin-1 text metadata (Author, Software, Description, etc.)
//! - `zTXt`: Compressed text metadata
//! - `iTXt`: International UTF-8 text and XMP metadata packets
//! - `tIME`: Last modification timestamp
//! - `dSIG`: Digital signatures
//!
//! Preserves all critical rendering chunks (`IHDR`, `PLTE`, `IDAT`, `IEND`, `pHYs`, `sRGB`, `cHRM`, `gAMA`)
//! completely bit-for-bit without decompression loss.

use crate::error::EmberError;

const PNG_MAGIC: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];

/// Strips metadata chunks from a PNG file.
pub fn strip_png(data: &[u8]) -> Result<(Vec<u8>, Vec<String>, bool), EmberError> {
    if data.len() < 8 || data[0..8] != PNG_MAGIC {
        return Err(EmberError::InvalidMetadata(
            "Invalid PNG: missing 8-byte magic header".to_string(),
        ));
    }

    let mut removed_fields = Vec::new();
    let mut out = Vec::with_capacity(data.len());
    out.extend_from_slice(&PNG_MAGIC);

    let mut offset = 8;
    let mut modified = false;

    while offset + 12 <= data.len() {
        let chunk_len = u32::from_be_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]) as usize;

        let total_chunk_len = 12 + chunk_len; // 4 len + 4 type + data + 4 crc
        if offset + total_chunk_len > data.len() {
            return Err(EmberError::InvalidMetadata(
                "Truncated or corrupted PNG chunk".to_string(),
            ));
        }

        let chunk_type = &data[offset + 4..offset + 8];
        let chunk_full = &data[offset..offset + total_chunk_len];

        let should_strip = match chunk_type {
            b"eXIf" => {
                removed_fields.push("EXIF metadata / GPS coordinates (eXIf)".to_string());
                true
            }
            b"tEXt" => {
                removed_fields.push("Text metadata (tEXt)".to_string());
                true
            }
            b"zTXt" => {
                removed_fields.push("Compressed text metadata (zTXt)".to_string());
                true
            }
            b"iTXt" => {
                removed_fields.push("International text / XMP metadata (iTXt)".to_string());
                true
            }
            b"tIME" => {
                removed_fields.push("Modification timestamp (tIME)".to_string());
                true
            }
            b"dSIG" => {
                removed_fields.push("Digital signature (dSIG)".to_string());
                true
            }
            _ => false,
        };

        if should_strip {
            modified = true;
        } else {
            out.extend_from_slice(chunk_full);
        }

        offset += total_chunk_len;
    }

    if !modified {
        return Ok((data.to_vec(), Vec::new(), false));
    }

    Ok((out, removed_fields, true))
}
