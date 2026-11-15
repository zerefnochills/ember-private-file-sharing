//! Metadata stripping module for Ember.
//!
//! Provides automatic sanitization of location and author metadata before encryption
//! according to `docs/EMBER_BUILD_GUIDE.md` milestone M6.
//!
//! Supported formats:
//! - JPEG: strips EXIF (GPS, camera make/model, timestamps), XMP, IPTC, and comments; preserves orientation.
//! - PNG: strips eXIf, tEXt, zTXt, iTXt, tIME, dSIG; preserves image data.
//! - WebP: strips EXIF and XMP chunks; clears VP8X flags; preserves image data.
//! - PDF: sanitizes /Info author, creator, dates, keywords, and XMP metadata streams while preserving xref integrity.
//!
//! Unsupported formats pass through with clear status reporting (never a silent failure).

pub mod jpeg;
pub mod pdf;
pub mod png;
pub mod webp;

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::error::EmberError;

/// Recognized file format for metadata stripping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileFormat {
    Jpeg,
    Png,
    WebP,
    Pdf,
    OfficeDocx,
    OfficeXlsx,
    OfficePptx,
    Unsupported(String),
}

impl std::fmt::Display for FileFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Jpeg => write!(f, "JPEG Image"),
            Self::Png => write!(f, "PNG Image"),
            Self::WebP => write!(f, "WebP Image"),
            Self::Pdf => write!(f, "PDF Document"),
            Self::OfficeDocx => write!(f, "Microsoft Word (DOCX)"),
            Self::OfficeXlsx => write!(f, "Microsoft Excel (XLSX)"),
            Self::OfficePptx => write!(f, "Microsoft PowerPoint (PPTX)"),
            Self::Unsupported(kind) => write!(f, "Unsupported ({kind})"),
        }
    }
}

/// Status of the metadata stripping operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StripStatus {
    /// Sensitive metadata was detected and removed.
    Stripped {
        format: FileFormat,
        fields_removed: Vec<String>,
    },
    /// Format is supported, but no sensitive metadata was present.
    NoMetadataFound { format: FileFormat },
    /// Format is not supported for metadata stripping; file passed through untouched.
    UnsupportedFormat { format: FileFormat, reason: String },
}

/// Result of a metadata stripping pass.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetadataStripResult {
    /// Detailed status of the operation for UI feedback.
    pub status: StripStatus,
    /// Whether any bytes were modified.
    pub modified: bool,
    /// Output file payload (sanitized if modified, original if not).
    pub data: Vec<u8>,
}

/// Detects file format based on magic bytes and optional file name hint.
pub fn detect_format(data: &[u8], filename_hint: Option<&str>) -> FileFormat {
    // 1. Inspect magic bytes
    if data.len() >= 3 && data[0] == 0xFF && data[1] == 0xD8 && data[2] == 0xFF {
        return FileFormat::Jpeg;
    }

    if data.len() >= 8 && data[0..8] == [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'] {
        return FileFormat::Png;
    }

    if data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        return FileFormat::WebP;
    }

    if data.len() >= 5 && data.starts_with(b"%PDF-") {
        return FileFormat::Pdf;
    }

    // Zip-based Office formats
    if data.len() >= 4 && data[0..4] == [0x50, 0x4B, 0x03, 0x04] {
        if let Some(hint) = filename_hint {
            let ext = Path::new(hint)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            match ext.as_str() {
                "docx" => return FileFormat::OfficeDocx,
                "xlsx" => return FileFormat::OfficeXlsx,
                "pptx" => return FileFormat::OfficePptx,
                _ => {}
            }
        }
    }

    // 2. Check filename extension as fallback
    if let Some(hint) = filename_hint {
        let ext = Path::new(hint)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        match ext.as_str() {
            "jpg" | "jpeg" => FileFormat::Jpeg,
            "png" => FileFormat::Png,
            "webp" => FileFormat::WebP,
            "pdf" => FileFormat::Pdf,
            "docx" => FileFormat::OfficeDocx,
            "xlsx" => FileFormat::OfficeXlsx,
            "pptx" => FileFormat::OfficePptx,
            other if !other.is_empty() => FileFormat::Unsupported(format!(".{other}")),
            _ => FileFormat::Unsupported("unknown".to_string()),
        }
    } else {
        FileFormat::Unsupported("unknown".to_string())
    }
}

/// Strips location, author, and camera metadata from supported file formats.
/// Returns a structured `MetadataStripResult` detailing what was removed.
/// Unsupported formats pass through with clear status reporting and are never silently dropped.
pub fn strip_metadata(
    data: &[u8],
    filename_hint: Option<&str>,
) -> Result<MetadataStripResult, EmberError> {
    let format = detect_format(data, filename_hint);

    match format {
        FileFormat::Jpeg => {
            let (clean_bytes, removed, modified) = jpeg::strip_jpeg(data)?;
            if modified {
                Ok(MetadataStripResult {
                    status: StripStatus::Stripped {
                        format: FileFormat::Jpeg,
                        fields_removed: removed,
                    },
                    modified: true,
                    data: clean_bytes,
                })
            } else {
                Ok(MetadataStripResult {
                    status: StripStatus::NoMetadataFound {
                        format: FileFormat::Jpeg,
                    },
                    modified: false,
                    data: data.to_vec(),
                })
            }
        }
        FileFormat::Png => {
            let (clean_bytes, removed, modified) = png::strip_png(data)?;
            if modified {
                Ok(MetadataStripResult {
                    status: StripStatus::Stripped {
                        format: FileFormat::Png,
                        fields_removed: removed,
                    },
                    modified: true,
                    data: clean_bytes,
                })
            } else {
                Ok(MetadataStripResult {
                    status: StripStatus::NoMetadataFound {
                        format: FileFormat::Png,
                    },
                    modified: false,
                    data: data.to_vec(),
                })
            }
        }
        FileFormat::WebP => {
            let (clean_bytes, removed, modified) = webp::strip_webp(data)?;
            if modified {
                Ok(MetadataStripResult {
                    status: StripStatus::Stripped {
                        format: FileFormat::WebP,
                        fields_removed: removed,
                    },
                    modified: true,
                    data: clean_bytes,
                })
            } else {
                Ok(MetadataStripResult {
                    status: StripStatus::NoMetadataFound {
                        format: FileFormat::WebP,
                    },
                    modified: false,
                    data: data.to_vec(),
                })
            }
        }
        FileFormat::Pdf => {
            let (clean_bytes, removed, modified) = pdf::strip_pdf(data)?;
            if modified {
                Ok(MetadataStripResult {
                    status: StripStatus::Stripped {
                        format: FileFormat::Pdf,
                        fields_removed: removed,
                    },
                    modified: true,
                    data: clean_bytes,
                })
            } else {
                Ok(MetadataStripResult {
                    status: StripStatus::NoMetadataFound {
                        format: FileFormat::Pdf,
                    },
                    modified: false,
                    data: data.to_vec(),
                })
            }
        }
        FileFormat::OfficeDocx | FileFormat::OfficeXlsx | FileFormat::OfficePptx => {
            Ok(MetadataStripResult {
                status: StripStatus::UnsupportedFormat {
                    format: format.clone(),
                    reason: "Office Open XML metadata stripping is not supported in this version"
                        .to_string(),
                },
                modified: false,
                data: data.to_vec(),
            })
        }
        FileFormat::Unsupported(ref kind) => Ok(MetadataStripResult {
            status: StripStatus::UnsupportedFormat {
                format: format.clone(),
                reason: format!("File type '{kind}' does not support metadata stripping"),
            },
            modified: false,
            data: data.to_vec(),
        }),
    }
}
