//! PDF metadata stripper.
//!
//! Strips document author, creator, producer, creation/modification dates, title, subject,
//! and XMP metadata streams from PDF files.
//! Preserves document byte offsets and xref tables intact by space-padding sanitized values.

use crate::error::EmberError;

const SENSITIVE_PDF_KEYS: &[&[u8]] = &[
    b"/Author",
    b"/Creator",
    b"/Producer",
    b"/CreationDate",
    b"/ModDate",
    b"/Title",
    b"/Subject",
    b"/Keywords",
];

/// Strips document metadata from a PDF file while preserving cross-reference integrity.
pub fn strip_pdf(data: &[u8]) -> Result<(Vec<u8>, Vec<String>, bool), EmberError> {
    if data.len() < 5 || !data.starts_with(b"%PDF-") {
        return Err(EmberError::InvalidMetadata(
            "Invalid PDF: missing '%PDF-' header".to_string(),
        ));
    }

    let mut out = data.to_vec();
    let mut removed_fields = Vec::new();
    let mut modified = false;

    // 1. Sanitize sensitive /Info dictionary keys: e.g. /Author (John Doe) or /Author <hex>
    for &key in SENSITIVE_PDF_KEYS {
        let key_str = std::str::from_utf8(key).unwrap_or("/Key");
        let mut search_idx = 0;

        while let Some(pos) = find_subsequence(&out[search_idx..], key) {
            let key_offset = search_idx + pos;
            let after_key = key_offset + key.len();

            // Find value start: skip whitespace
            let mut val_start = after_key;
            while val_start < out.len()
                && (out[val_start] == b' '
                    || out[val_start] == b'\t'
                    || out[val_start] == b'\r'
                    || out[val_start] == b'\n')
            {
                val_start += 1;
            }

            if val_start < out.len() {
                if out[val_start] == b'(' {
                    // String literal value: (content with possible escaped parentheses)
                    let mut depth = 1;
                    let mut val_end = val_start + 1;
                    let mut escaped = false;

                    while val_end < out.len() && depth > 0 {
                        let b = out[val_end];
                        if escaped {
                            escaped = false;
                        } else if b == b'\\' {
                            escaped = true;
                        } else if b == b'(' {
                            depth += 1;
                        } else if b == b')' {
                            depth -= 1;
                        }
                        val_end += 1;
                    }

                    if depth == 0 && val_end > val_start + 2 {
                        // Clear content between '(' and ')' with spaces
                        let content_len = (val_end - 1) - (val_start + 1);
                        if content_len > 0 {
                            for b in &mut out[val_start + 1..val_end - 1] {
                                *b = b' ';
                            }
                            let field_desc = format!("PDF Info field: {key_str}");
                            if !removed_fields.contains(&field_desc) {
                                removed_fields.push(field_desc);
                            }
                            modified = true;
                        }
                    }
                } else if out[val_start] == b'<'
                    && val_start + 1 < out.len()
                    && out[val_start + 1] != b'<'
                {
                    // Hex string literal: <4A6F686E>
                    if let Some(end_hex) = out[val_start..].iter().position(|&b| b == b'>') {
                        let hex_end = val_start + end_hex;
                        for b in &mut out[val_start + 1..hex_end] {
                            *b = b'0';
                        }
                        let field_desc = format!("PDF Info field (hex): {key_str}");
                        if !removed_fields.contains(&field_desc) {
                            removed_fields.push(field_desc);
                        }
                        modified = true;
                    }
                }
            }

            search_idx = after_key;
        }
    }

    // 2. Sanitize XMP Metadata XML packets: between <x:xmpmeta and </x:xmpmeta>
    let xmp_start_tag = b"<x:xmpmeta";
    let xmp_end_tag = b"</x:xmpmeta>";

    let mut search_idx = 0;
    while let Some(start_pos) = find_subsequence(&out[search_idx..], xmp_start_tag) {
        let abs_start = search_idx + start_pos;
        if let Some(end_pos) = find_subsequence(&out[abs_start..], xmp_end_tag) {
            let abs_end = abs_start + end_pos + xmp_end_tag.len();

            // Space-pad everything inside the XMP tag
            for b in &mut out[abs_start..abs_end] {
                *b = b' ';
            }

            removed_fields.push("PDF XMP metadata stream packet".to_string());
            modified = true;
            search_idx = abs_end;
        } else {
            break;
        }
    }

    if !modified {
        return Ok((data.to_vec(), Vec::new(), false));
    }

    Ok((out, removed_fields, true))
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}
