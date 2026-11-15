//! JPEG metadata stripper.
//!
//! Strips APP1 (EXIF, GPS, XMP, camera make/model, timestamps), APP2, APP13 (Photoshop/IPTC),
//! and COM (Comments) marker segments.
//! If EXIF contains an orientation tag (0x0112), a minimal clean EXIF APP1 segment
//! containing ONLY the orientation tag is preserved so the image renders with correct orientation.
//! All image data (DQT, SOF, DHT, SOS scan data up to EOI) remains 100% bit-for-bit intact.

use crate::error::EmberError;

/// Strips EXIF, XMP, IPTC, and comment metadata from a JPEG file.
/// Preserves orientation if present in EXIF.
pub fn strip_jpeg(data: &[u8]) -> Result<(Vec<u8>, Vec<String>, bool), EmberError> {
    if data.len() < 4 || data[0] != 0xFF || data[1] != 0xD8 {
        return Err(EmberError::InvalidMetadata(
            "Invalid JPEG: missing SOI marker (0xFFD8)".to_string(),
        ));
    }

    let mut removed_fields = Vec::new();
    let mut orientation_value: Option<u16> = None;
    let mut had_sensitive_metadata = false;

    // First pass: scan segments, detect orientation, detect sensitive metadata
    let mut offset = 2;
    while offset + 1 < data.len() {
        if data[offset] != 0xFF {
            // Not a marker boundary
            break;
        }

        let marker = data[offset + 1];
        offset += 2;

        // Skip standalone fill bytes 0xFF
        if marker == 0xFF || marker == 0x00 {
            continue;
        }

        // SOI (0xD8), EOI (0xD9), RST0-RST7 (0xD0-0xD7) have no length field
        if marker == 0xD8 || marker == 0xD9 || (0xD0..=0xD7).contains(&marker) {
            continue;
        }

        // SOS (0xDA): Start of Scan. The rest is compressed bitstream up to EOI.
        if marker == 0xDA {
            break;
        }

        if offset + 2 > data.len() {
            return Err(EmberError::InvalidMetadata(
                "Truncated JPEG marker segment length".to_string(),
            ));
        }

        let seg_len = u16::from_be_bytes([data[offset], data[offset + 1]]) as usize;
        if seg_len < 2 || offset + seg_len > data.len() {
            return Err(EmberError::InvalidMetadata(
                "Invalid JPEG marker segment length".to_string(),
            ));
        }

        let payload = &data[offset + 2..offset + seg_len];

        match marker {
            0xE1 => {
                // APP1: EXIF or XMP
                if payload.starts_with(b"Exif\0\0") {
                    had_sensitive_metadata = true;
                    if !removed_fields
                        .iter()
                        .any(|f: &String| f.contains("EXIF metadata"))
                    {
                        removed_fields.push("EXIF metadata / GPS coordinates (APP1)".to_string());
                    }
                    if let Some(orient) = extract_exif_orientation(&payload[6..]) {
                        if (1..=8).contains(&orient) {
                            orientation_value = Some(orient);
                        }
                    }
                } else if payload.starts_with(b"http://ns.adobe.com/xap/1.0/") {
                    had_sensitive_metadata = true;
                    removed_fields.push("XMP metadata packet (APP1)".to_string());
                } else {
                    had_sensitive_metadata = true;
                    removed_fields.push("APP1 metadata segment".to_string());
                }
            }
            0xE2 => {
                // APP2: Flashpix, preview, or ICC
                if payload.starts_with(b"FPXR\0") {
                    had_sensitive_metadata = true;
                    removed_fields.push("Flashpix metadata (APP2)".to_string());
                }
            }
            0xED => {
                // APP13: Photoshop IRB / IPTC
                had_sensitive_metadata = true;
                removed_fields.push("Photoshop / IPTC metadata (APP13)".to_string());
            }
            0xFE => {
                // COM: Comment
                had_sensitive_metadata = true;
                removed_fields.push("User comments (COM)".to_string());
            }
            _ => {}
        }

        offset += seg_len;
    }

    if !had_sensitive_metadata {
        return Ok((data.to_vec(), Vec::new(), false));
    }

    // Second pass: construct clean JPEG
    let mut out = Vec::with_capacity(data.len());
    out.extend_from_slice(&[0xFF, 0xD8]); // SOI

    // If orientation was non-default (> 1), emit minimal clean EXIF APP1
    if let Some(orient) = orientation_value {
        if orient > 1 {
            let minimal_exif = build_minimal_orientation_app1(orient);
            out.extend_from_slice(&minimal_exif);
        }
    }

    offset = 2;
    while offset + 1 < data.len() {
        if data[offset] != 0xFF {
            // Append remaining bytes (scan data) and break
            out.extend_from_slice(&data[offset..]);
            break;
        }

        let marker = data[offset + 1];
        let seg_start = offset;
        offset += 2;

        if marker == 0xFF || marker == 0x00 {
            out.extend_from_slice(&[0xFF, marker]);
            continue;
        }

        if marker == 0xD8 || (0xD0..=0xD7).contains(&marker) {
            out.extend_from_slice(&[0xFF, marker]);
            continue;
        }

        if marker == 0xD9 {
            out.extend_from_slice(&[0xFF, 0xD9]);
            break;
        }

        if marker == 0xDA {
            // SOS: Copy SOS header and all remaining data to end
            out.extend_from_slice(&data[seg_start..]);
            break;
        }

        let seg_len = u16::from_be_bytes([data[offset], data[offset + 1]]) as usize;
        let seg_end = offset + seg_len;

        // Skip sensitive segments
        let should_strip = match marker {
            0xE1 => true, // All original APP1 (replaced by minimal if needed)
            0xED => true, // APP13 Photoshop/IPTC
            0xFE => true, // COM Comments
            0xE2 if data[offset + 2..seg_end].starts_with(b"FPXR\0") => true,
            _ => false,
        };

        if !should_strip {
            out.extend_from_slice(&data[seg_start..seg_end]);
        }

        offset = seg_end;
    }

    Ok((out, removed_fields, true))
}

/// Parses TIFF header inside EXIF payload to extract Orientation tag 0x0112.
fn extract_exif_orientation(tiff: &[u8]) -> Option<u16> {
    if tiff.len() < 8 {
        return None;
    }

    let is_le = if &tiff[0..2] == b"II" {
        true
    } else if &tiff[0..2] == b"MM" {
        false
    } else {
        return None;
    };

    let read_u16 = |buf: &[u8]| -> u16 {
        if is_le {
            u16::from_le_bytes([buf[0], buf[1]])
        } else {
            u16::from_be_bytes([buf[0], buf[1]])
        }
    };

    let read_u32 = |buf: &[u8]| -> u32 {
        if is_le {
            u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]])
        } else {
            u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]])
        }
    };

    let magic = read_u16(&tiff[2..4]);
    if magic != 42 {
        return None;
    }

    let ifd0_offset = read_u32(&tiff[4..8]) as usize;
    if ifd0_offset + 2 > tiff.len() {
        return None;
    }

    let num_entries = read_u16(&tiff[ifd0_offset..ifd0_offset + 2]) as usize;
    let mut entry_offset = ifd0_offset + 2;

    for _ in 0..num_entries {
        if entry_offset + 12 > tiff.len() {
            break;
        }

        let tag = read_u16(&tiff[entry_offset..entry_offset + 2]);
        let tag_type = read_u16(&tiff[entry_offset + 2..entry_offset + 4]);
        let count = read_u32(&tiff[entry_offset + 4..entry_offset + 8]);

        if tag == 0x0112 && tag_type == 3 && count == 1 {
            // Orientation tag (SHORT, count 1)
            let val = read_u16(&tiff[entry_offset + 8..entry_offset + 10]);
            return Some(val);
        }

        entry_offset += 12;
    }

    None
}

/// Builds a minimal 30-byte EXIF APP1 segment containing ONLY orientation.
fn build_minimal_orientation_app1(orientation: u16) -> Vec<u8> {
    let mut seg = Vec::with_capacity(32);
    seg.extend_from_slice(&[0xFF, 0xE1]); // APP1 marker

    // Payload: "Exif\0\0" (6B) + TIFF (II, 42, offset 8) (8B) + 1 entry (14B) + next IFD 0 (4B) = 32 bytes
    // Marker length = payload length + 2 = 34 bytes
    let seg_len: u16 = 34;
    seg.extend_from_slice(&seg_len.to_be_bytes());
    seg.extend_from_slice(b"Exif\0\0");

    // TIFF Header: Little Endian ("II"), 42, IFD0 offset 8
    seg.extend_from_slice(b"II");
    seg.extend_from_slice(&42u16.to_le_bytes());
    seg.extend_from_slice(&8u32.to_le_bytes());

    // IFD0: 1 entry
    seg.extend_from_slice(&1u16.to_le_bytes());

    // Tag 0x0112 (Orientation), Type 3 (SHORT), Count 1, Value
    seg.extend_from_slice(&0x0112u16.to_le_bytes());
    seg.extend_from_slice(&3u16.to_le_bytes());
    seg.extend_from_slice(&1u32.to_le_bytes());
    seg.extend_from_slice(&orientation.to_le_bytes());
    seg.extend_from_slice(&[0u8, 0u8]); // 2 bytes padding to fill 4-byte value field

    // Offset to next IFD: 0
    seg.extend_from_slice(&0u32.to_le_bytes());

    seg
}
