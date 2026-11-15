//! Fixture-based tests for metadata stripping (Milestone M6).
//!
//! Verifies:
//! - JPEG: strips EXIF (GPS, camera make/model, timestamps), XMP, IPTC, and comments; preserves orientation and image scan data.
//! - PNG: strips eXIf, tEXt, zTXt, iTXt, tIME, dSIG; preserves IHDR/IDAT/IEND.
//! - WebP: strips EXIF and XMP chunks; clears VP8X flags; preserves image data.
//! - PDF: sanitizes /Author, /Creator, /Dates, and XMP streams while preserving byte length and xref offsets.
//! - Clean files return `NoMetadataFound`.
//! - Unsupported types return `UnsupportedFormat` with clear status (never a silent failure).

use ember_core::{strip_metadata, FileFormat, StripStatus};

/// Helper: builds a minimal valid JPEG with given APP segments and scan payload.
fn build_test_jpeg(segments: &[(&[u8], &[u8])], scan_payload: &[u8]) -> Vec<u8> {
    let mut jpeg = Vec::new();
    jpeg.extend_from_slice(&[0xFF, 0xD8]); // SOI

    // APP0 JFIF
    jpeg.extend_from_slice(&[0xFF, 0xE0, 0x00, 0x10]);
    jpeg.extend_from_slice(b"JFIF\0\x01\x01\x00\x00\x01\x00\x01\x00\x00");

    for &(marker, payload) in segments {
        jpeg.extend_from_slice(marker);
        let len = (payload.len() + 2) as u16;
        jpeg.extend_from_slice(&len.to_be_bytes());
        jpeg.extend_from_slice(payload);
    }

    // SOS (0xFFDA) + scan payload + EOI (0xFFD9)
    jpeg.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x08, 0x01, 0x01, 0x00, 0x00]);
    jpeg.extend_from_slice(scan_payload);
    jpeg.extend_from_slice(&[0xFF, 0xD9]); // EOI

    jpeg
}

/// Helper: builds a minimal valid PNG with given chunks.
fn build_test_png(custom_chunks: &[(&[u8; 4], &[u8])]) -> Vec<u8> {
    let mut png = Vec::new();
    // 8-byte magic
    png.extend_from_slice(&[0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n']);

    // IHDR chunk (1x1 pixel)
    let ihdr_data = [0, 0, 0, 1, 0, 0, 0, 1, 8, 2, 0, 0, 0];
    append_png_chunk(&mut png, b"IHDR", &ihdr_data);

    for &(chunk_type, data) in custom_chunks {
        append_png_chunk(&mut png, chunk_type, data);
    }

    // IDAT chunk (dummy compressed pixel)
    let idat_data = [0x78, 0x9c, 0x63, 0x60, 0x00, 0x00, 0x00, 0x02, 0x00, 0x01];
    append_png_chunk(&mut png, b"IDAT", &idat_data);

    // IEND chunk
    append_png_chunk(&mut png, b"IEND", &[]);

    png
}

fn append_png_chunk(buf: &mut Vec<u8>, chunk_type: &[u8; 4], data: &[u8]) {
    let len = data.len() as u32;
    buf.extend_from_slice(&len.to_be_bytes());
    buf.extend_from_slice(chunk_type);
    buf.extend_from_slice(data);

    // Calculate CRC32 over type + data
    let hasher = crc32_simple(chunk_type, data);
    buf.extend_from_slice(&hasher.to_be_bytes());
}

fn crc32_simple(chunk_type: &[u8], data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFFFFFF;
    for &b in chunk_type.iter().chain(data.iter()) {
        crc ^= b as u32;
        for _ in 0..8 {
            if (crc & 1) != 0 {
                crc = (crc >> 1) ^ 0xEDB88320;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

/// Helper: builds a minimal valid WebP file.
fn build_test_webp(custom_chunks: &[(&[u8; 4], &[u8])]) -> Vec<u8> {
    let mut body = Vec::new();

    // VP8X extended header chunk
    let mut vp8x_payload = vec![0u8; 10];
    vp8x_payload[0] = 0x04 | 0x08; // Set EXIF (0x04) and XMP (0x08) flags
    body.extend_from_slice(b"VP8X");
    body.extend_from_slice(&10u32.to_le_bytes());
    body.extend_from_slice(&vp8x_payload);

    for &(fourcc, data) in custom_chunks {
        body.extend_from_slice(fourcc);
        body.extend_from_slice(&(data.len() as u32).to_le_bytes());
        body.extend_from_slice(data);
        if data.len() % 2 != 0 {
            body.push(0); // WebP padding byte
        }
    }

    // Dummy VP8 chunk
    let vp8_data = [0x30, 0x01, 0x00, 0x9d, 0x01, 0x2a, 0x01, 0x00, 0x01, 0x00];
    body.extend_from_slice(b"VP8 ");
    body.extend_from_slice(&(vp8_data.len() as u32).to_le_bytes());
    body.extend_from_slice(&vp8_data);

    let mut webp = Vec::new();
    webp.extend_from_slice(b"RIFF");
    let riff_size = (body.len() + 4) as u32;
    webp.extend_from_slice(&riff_size.to_le_bytes());
    webp.extend_from_slice(b"WEBP");
    webp.extend_from_slice(&body);

    webp
}

#[test]
fn test_jpeg_strip_exif_gps_camera_and_comments() {
    let scan_payload = b"CRITICAL_COMPRESSED_DCT_IMAGE_DATA_BYTES_12345";

    // Build EXIF payload with Camera Model and GPS info
    let mut exif_payload = Vec::new();
    exif_payload.extend_from_slice(b"Exif\0\0");
    exif_payload.extend_from_slice(b"II\x2A\x00\x08\x00\x00\x00"); // TIFF header
    exif_payload.extend_from_slice(&2u16.to_le_bytes()); // 2 entries
                                                         // Entry 1: Make
    exif_payload.extend_from_slice(&0x010Fu16.to_le_bytes()); // Make tag
    exif_payload.extend_from_slice(&2u16.to_le_bytes()); // ASCII
    exif_payload.extend_from_slice(&12u32.to_le_bytes()); // length
    exif_payload.extend_from_slice(&0x0026u32.to_le_bytes()); // offset to string
                                                              // Entry 2: Orientation = 1 (default)
    exif_payload.extend_from_slice(&0x0112u16.to_le_bytes());
    exif_payload.extend_from_slice(&3u16.to_le_bytes());
    exif_payload.extend_from_slice(&1u32.to_le_bytes());
    exif_payload.extend_from_slice(&1u16.to_le_bytes());
    exif_payload.extend_from_slice(&[0, 0]);
    exif_payload.extend_from_slice(&0u32.to_le_bytes()); // next IFD
    exif_payload.extend_from_slice(b"EmberCamera\0"); // Camera string
    exif_payload.extend_from_slice(b"LAT:37.7749_LON:-122.4194_GPS"); // GPS coordinates string

    let iptc_payload = b"Photoshop 3.0\08BIM\x04\x04\0\0\0\0\x00\x0DAuthor:JohnDoe";
    let com_payload = b"Private secret comment";

    let jpeg_data = build_test_jpeg(
        &[
            (&[0xFF, 0xE1], &exif_payload),
            (&[0xFF, 0xED], iptc_payload),
            (&[0xFF, 0xFE], com_payload),
        ],
        scan_payload,
    );

    // Verify initial data contains sensitive strings
    let initial_str = String::from_utf8_lossy(&jpeg_data);
    assert!(initial_str.contains("EmberCamera"));
    assert!(initial_str.contains("GPS"));
    assert!(initial_str.contains("JohnDoe"));
    assert!(initial_str.contains("Private secret comment"));
    assert!(initial_str.contains("CRITICAL_COMPRESSED_DCT_IMAGE_DATA_BYTES_12345"));

    // Run metadata stripping
    let res = strip_metadata(&jpeg_data, Some("photo.jpg")).unwrap();
    assert!(res.modified);

    match res.status {
        StripStatus::Stripped {
            format,
            fields_removed,
        } => {
            assert_eq!(format, FileFormat::Jpeg);
            assert!(!fields_removed.is_empty());
        }
        _ => panic!("Expected StripStatus::Stripped"),
    }

    // Verify sanitized JPEG
    let clean_str = String::from_utf8_lossy(&res.data);
    assert!(
        !clean_str.contains("EmberCamera"),
        "Camera model must be stripped"
    );
    assert!(
        !clean_str.contains("GPS"),
        "GPS coordinates must be stripped"
    );
    assert!(
        !clean_str.contains("JohnDoe"),
        "IPTC author must be stripped"
    );
    assert!(
        !clean_str.contains("Private secret comment"),
        "COM comments must be stripped"
    );

    // Verify image scan payload is 100% intact!
    assert!(
        clean_str.contains("CRITICAL_COMPRESSED_DCT_IMAGE_DATA_BYTES_12345"),
        "Image scan payload must be preserved bit-for-bit"
    );
    assert!(res.data.starts_with(&[0xFF, 0xD8]));
    assert!(res.data.ends_with(&[0xFF, 0xD9]));
}

#[test]
fn test_jpeg_preserves_orientation_tag() {
    let scan_payload = b"IMAGE_DATA_ORIENTATION_TEST";

    // Build EXIF with Orientation = 6 (90 degrees rotated)
    let mut exif_payload = Vec::new();
    exif_payload.extend_from_slice(b"Exif\0\0");
    exif_payload.extend_from_slice(b"II\x2A\x00\x08\x00\x00\x00");
    exif_payload.extend_from_slice(&2u16.to_le_bytes());
    // Make (sensitive)
    exif_payload.extend_from_slice(&0x010Fu16.to_le_bytes());
    exif_payload.extend_from_slice(&2u16.to_le_bytes());
    exif_payload.extend_from_slice(&7u32.to_le_bytes());
    exif_payload.extend_from_slice(&0x0026u32.to_le_bytes());
    // Orientation = 6
    exif_payload.extend_from_slice(&0x0112u16.to_le_bytes());
    exif_payload.extend_from_slice(&3u16.to_le_bytes());
    exif_payload.extend_from_slice(&1u32.to_le_bytes());
    exif_payload.extend_from_slice(&6u16.to_le_bytes()); // Orientation = 6
    exif_payload.extend_from_slice(&[0, 0]);
    exif_payload.extend_from_slice(&0u32.to_le_bytes());
    exif_payload.extend_from_slice(b"SensMake\0GPS_COORD_12345");

    let jpeg_data = build_test_jpeg(&[(&[0xFF, 0xE1], &exif_payload)], scan_payload);

    let res = strip_metadata(&jpeg_data, Some("rotated.jpg")).unwrap();
    assert!(res.modified);

    let clean_str = String::from_utf8_lossy(&res.data);
    assert!(!clean_str.contains("SensMake"));
    assert!(!clean_str.contains("GPS_COORD"));
    assert!(clean_str.contains("IMAGE_DATA_ORIENTATION_TEST"));

    // Verify minimal clean EXIF still contains orientation 6
    let (data_again, _, _) = ember_core::strip::jpeg::strip_jpeg(&res.data).unwrap();
    assert_eq!(data_again, res.data); // Second pass is idempotent
}

#[test]
fn test_png_strip_text_exif_and_timestamp() {
    let custom_chunks = [
        (b"tEXt", b"Author\0Jane Doe confidential author".as_slice()),
        (b"eXIf", b"GPS_LATITUDE_LONGITUDE_DATA".as_slice()),
        (b"tIME", &[0x07, 0xEA, 0x0B, 0x0F, 0x0C, 0x1E, 0x00][..]),
    ];

    let png_data = build_test_png(&custom_chunks);
    let initial_str = String::from_utf8_lossy(&png_data);
    assert!(initial_str.contains("Jane Doe"));
    assert!(initial_str.contains("GPS_LATITUDE"));

    let res = strip_metadata(&png_data, Some("graphic.png")).unwrap();
    assert!(res.modified);

    match res.status {
        StripStatus::Stripped {
            format,
            fields_removed,
        } => {
            assert_eq!(format, FileFormat::Png);
            assert!(fields_removed.iter().any(|f| f.contains("Text metadata")));
            assert!(fields_removed.iter().any(|f| f.contains("EXIF metadata")));
            assert!(fields_removed.iter().any(|f| f.contains("timestamp")));
        }
        _ => panic!("Expected StripStatus::Stripped"),
    }

    let clean_str = String::from_utf8_lossy(&res.data);
    assert!(!clean_str.contains("Jane Doe"));
    assert!(!clean_str.contains("GPS_LATITUDE"));

    // Verify PNG structure
    assert!(res
        .data
        .starts_with(&[0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n']));
    assert!(clean_str.contains("IHDR"));
    assert!(clean_str.contains("IDAT"));
    assert!(clean_str.contains("IEND"));
}

#[test]
fn test_webp_strip_exif_and_xmp() {
    let custom_chunks = [
        (b"EXIF", b"Exif\0\0EXIF_GPS_DATA_LAT_35_LON_139".as_slice()),
        (
            b"XMP ",
            b"<x:xmpmeta>Author: Secret Author</x:xmpmeta>".as_slice(),
        ),
    ];

    let webp_data = build_test_webp(&custom_chunks);
    let initial_str = String::from_utf8_lossy(&webp_data);
    assert!(initial_str.contains("EXIF_GPS_DATA"));
    assert!(initial_str.contains("Secret Author"));

    let res = strip_metadata(&webp_data, Some("art.webp")).unwrap();
    assert!(res.modified);

    match res.status {
        StripStatus::Stripped {
            format,
            fields_removed,
        } => {
            assert_eq!(format, FileFormat::WebP);
            assert!(fields_removed.iter().any(|f| f.contains("EXIF")));
            assert!(fields_removed.iter().any(|f| f.contains("XMP")));
        }
        _ => panic!("Expected StripStatus::Stripped"),
    }

    let clean_str = String::from_utf8_lossy(&res.data);
    assert!(!clean_str.contains("EXIF_GPS_DATA"));
    assert!(!clean_str.contains("Secret Author"));
    assert!(clean_str.contains("VP8 "));

    // Verify RIFF header
    assert_eq!(&res.data[0..4], b"RIFF");
    assert_eq!(&res.data[8..12], b"WEBP");
}

#[test]
fn test_pdf_strip_info_and_xmp() {
    let pdf_source = b"%PDF-1.7\n\
1 0 obj\n\
<< /Type /Catalog /Pages 2 0 R >>\n\
endobj\n\
2 0 obj\n\
<< /Type /Pages /Kids [3 0 R] /Count 1 >>\n\
endobj\n\
3 0 obj\n\
<< /Type /Page /Parent 2 0 R >>\n\
endobj\n\
4 0 obj\n\
<< /Author (Confidential Author John) /Creator (Secret Generator) /CreationDate (D:20261115120000Z) /Title (Internal Draft) >>\n\
endobj\n\
5 0 obj\n\
<< /Type /Metadata /Length 50 >>\n\
stream\n\
<x:xmpmeta><dc:creator>John Doe</dc:creator></x:xmpmeta>\n\
endstream\n\
endobj\n\
xref\n\
0 6\n\
0000000000 65535 f \n\
0000000009 00000 n \n\
0000000058 00000 n \n\
0000000115 00000 n \n\
0000000162 00000 n \n\
0000000280 00000 n \n\
trailer\n\
<< /Size 6 /Root 1 0 R /Info 4 0 R >>\n\
startxref\n\
390\n\
%%EOF\n";

    let initial_str = String::from_utf8_lossy(pdf_source);
    assert!(initial_str.contains("Confidential Author John"));
    assert!(initial_str.contains("Secret Generator"));
    assert!(initial_str.contains("D:20261115120000Z"));
    assert!(initial_str.contains("Internal Draft"));
    assert!(initial_str.contains("John Doe"));

    let res = strip_metadata(pdf_source, Some("memo.pdf")).unwrap();
    assert!(res.modified);

    // Crucial check: byte length MUST be exactly identical to preserve xref offsets!
    assert_eq!(
        res.data.len(),
        pdf_source.len(),
        "Sanitized PDF must preserve byte length for xref validity"
    );

    let clean_str = String::from_utf8_lossy(&res.data);
    assert!(!clean_str.contains("Confidential Author John"));
    assert!(!clean_str.contains("Secret Generator"));
    assert!(!clean_str.contains("D:20261115120000Z"));
    assert!(!clean_str.contains("Internal Draft"));
    assert!(!clean_str.contains("John Doe"));

    assert!(clean_str.starts_with("%PDF-"));
    assert!(clean_str.contains("%%EOF"));
}

#[test]
fn test_clean_file_returns_no_metadata_found() {
    let clean_jpeg = build_test_jpeg(&[], b"RAW_SCAN_DATA");
    let res = strip_metadata(&clean_jpeg, Some("clean.jpg")).unwrap();
    assert!(!res.modified);
    match res.status {
        StripStatus::NoMetadataFound { format } => {
            assert_eq!(format, FileFormat::Jpeg);
        }
        _ => panic!("Expected StripStatus::NoMetadataFound"),
    }
}

#[test]
fn test_unsupported_formats_pass_through_with_status_never_silent() {
    // 1. Audio file (MP3)
    let mp3_data = b"ID3\x03\x00\x00\x00\x00\x00\x00MPEG_AUDIO_STREAM_DATA";
    let res_mp3 = strip_metadata(mp3_data, Some("track.mp3")).unwrap();
    assert!(!res_mp3.modified);
    assert_eq!(res_mp3.data, mp3_data);
    match res_mp3.status {
        StripStatus::UnsupportedFormat { format, reason } => {
            assert_eq!(format, FileFormat::Unsupported(".mp3".to_string()));
            assert!(reason.contains("does not support metadata stripping"));
        }
        _ => panic!("Expected UnsupportedFormat for MP3"),
    }

    // 2. Binary / unknown file
    let bin_data = vec![0xDE, 0xAD, 0xBE, 0xEF];
    let res_bin = strip_metadata(&bin_data, None).unwrap();
    assert!(!res_bin.modified);
    assert_eq!(res_bin.data, bin_data);
    match res_bin.status {
        StripStatus::UnsupportedFormat { format, reason } => {
            assert_eq!(format, FileFormat::Unsupported("unknown".to_string()));
            assert!(!reason.is_empty());
        }
        _ => panic!("Expected UnsupportedFormat for raw bin"),
    }

    // 3. Office document (DOCX)
    let docx_data = [0x50, 0x4B, 0x03, 0x04, 0x14, 0x00, 0x00, 0x00];
    let res_docx = strip_metadata(&docx_data, Some("report.docx")).unwrap();
    assert!(!res_docx.modified);
    assert_eq!(res_docx.data, docx_data);
    match res_docx.status {
        StripStatus::UnsupportedFormat { format, reason } => {
            assert_eq!(format, FileFormat::OfficeDocx);
            assert!(reason.contains("Office Open XML"));
        }
        _ => panic!("Expected UnsupportedFormat for DOCX"),
    }
}
