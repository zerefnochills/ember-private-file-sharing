//! WebP metadata stripper.
//!
//! Strips `EXIF` and `XMP ` chunks from WebP RIFF containers.
//! If a `VP8X` extended header is present, clears the EXIF and XMP flag bits.
//! Updates the RIFF file size header and preserves all image data (`VP8 `, `VP8L`, `ALPH`, etc.).

use crate::error::EmberError;

/// Strips EXIF and XMP metadata chunks from a WebP file.
pub fn strip_webp(data: &[u8]) -> Result<(Vec<u8>, Vec<String>, bool), EmberError> {
    if data.len() < 12 || &data[0..4] != b"RIFF" || &data[8..12] != b"WEBP" {
        return Err(EmberError::InvalidMetadata(
            "Invalid WebP: missing RIFF/WEBP header".to_string(),
        ));
    }

    let mut removed_fields = Vec::new();
    let mut modified = false;

    // Output buffer
    let mut out = Vec::with_capacity(data.len());
    out.extend_from_slice(&data[0..12]); // RIFF + size + WEBP

    let mut offset = 12;
    while offset + 8 <= data.len() {
        let fourcc = &data[offset..offset + 4];
        let chunk_size = u32::from_le_bytes([
            data[offset + 4],
            data[offset + 5],
            data[offset + 6],
            data[offset + 7],
        ]) as usize;

        let padded_size = chunk_size + (chunk_size % 2);
        let total_chunk_len = 8 + padded_size;

        if offset + total_chunk_len > data.len() {
            return Err(EmberError::InvalidMetadata(
                "Truncated or corrupted WebP chunk".to_string(),
            ));
        }

        let _chunk_data = &data[offset + 8..offset + 8 + chunk_size];

        match fourcc {
            b"EXIF" => {
                removed_fields.push("EXIF metadata / GPS coordinates (EXIF)".to_string());
                modified = true;
            }
            b"XMP " => {
                removed_fields.push("XMP metadata packet (XMP )".to_string());
                modified = true;
            }
            b"VP8X" => {
                // If VP8X extended header, copy but adjust flags if needed later
                let mut vp8x_header = data[offset..offset + total_chunk_len].to_vec();
                if vp8x_header.len() >= 12 {
                    // Flags are in bytes 8..12 of the chunk
                    let flags = u32::from_le_bytes([
                        vp8x_header[8],
                        vp8x_header[9],
                        vp8x_header[10],
                        vp8x_header[11],
                    ]);
                    // Clear EXIF flag (bit 2 = 0x04) and XMP flag (bit 3 = 0x08)
                    let new_flags = flags & !0x04 & !0x08;
                    vp8x_header[8..12].copy_from_slice(&new_flags.to_le_bytes());
                }
                out.extend_from_slice(&vp8x_header);
            }
            _ => {
                out.extend_from_slice(&data[offset..offset + total_chunk_len]);
            }
        }

        offset += total_chunk_len;
    }

    if !modified {
        return Ok((data.to_vec(), Vec::new(), false));
    }

    // Update RIFF total size: file length - 8
    let new_riff_size = (out.len() - 8) as u32;
    out[4..8].copy_from_slice(&new_riff_size.to_le_bytes());

    Ok((out, removed_fields, true))
}
