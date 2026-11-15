# ADR 0005: Pre-Encryption Metadata Stripping Architecture

- Status: accepted
- Date: 2026-11-15
- Deciders: Antigravity

## Context
Milestone M6 requires stripping location (GPS), camera hardware, author, and creation metadata from files before encryption occurs in `ember-core`.
Key requirements and constraints from `docs/EMBER_BUILD_GUIDE.md`:
1. Images: strip EXIF, XMP, IPTC, and comments for JPEG, PNG, and WebP; preserve orientation so image rendering is correct.
2. Documents: sanitize PDF /Info dictionary (author, creator, dates, keywords) and XMP stream packets without breaking cross-reference table (xref) offsets.
3. Unsupported formats: pass through with a structured, transparent status (`StripStatus::UnsupportedFormat`); never fail silently.
4. UI Contract: return a structured `MetadataStripResult` detailing exactly which fields were removed so user-facing apps can inform the sender.
5. Image integrity: image pixel payloads (JPEG scan data, PNG IDAT chunks, WebP VP8/VP8L bitstreams) must remain bit-for-bit unchanged without decompression loss.

## Decision

We implemented format-aware, byte-level metadata strippers directly in `ember-core::strip`:

1. **JPEG (`strip::jpeg`)**:
   - Parses marker segments starting from SOI (`0xFFD8`).
   - Strips APP1 (EXIF / XMP), APP2 (Flashpix), APP13 (Photoshop IRB / IPTC), and COM (Comments).
   - If an EXIF orientation tag (`0x0112`) is present with value > 1, generates a clean minimal 34-byte APP1 segment containing *only* tag `0x0112` and zero GPS, camera, or timestamp fields.
   - Preserves APP0 (JFIF) and all compressed scan data between SOS (`0xFFDA`) and EOI (`0xFFD9`) bit-for-bit.

2. **PNG (`strip::png`)**:
   - Parses chunk structure from 8-byte PNG magic.
   - Strips `eXIf` (EXIF / GPS), `tEXt` (Latin-1 text), `zTXt` (compressed text), `iTXt` (international text / XMP), `tIME` (modification timestamp), and `dSIG` (digital signatures).
   - Preserves critical rendering chunks (`IHDR`, `PLTE`, `IDAT`, `IEND`, `pHYs`, `sRGB`, `cHRM`, `gAMA`).

3. **WebP (`strip::webp`)**:
   - Parses RIFF container chunks.
   - Strips `EXIF` and `XMP ` chunks.
   - If `VP8X` extended header is present, clears bit 2 (EXIF) and bit 3 (XMP).
   - Updates the 32-bit RIFF file size in bytes 4..8 and leaves image bitstream (`VP8 `, `VP8L`, `ALPH`) untouched.

4. **PDF (`strip::pdf`)**:
   - Detects `%PDF-` header.
   - Sanitizes sensitive keys in `/Info` dictionary (`/Author`, `/Creator`, `/Producer`, `/CreationDate`, `/ModDate`, `/Title`, `/Subject`, `/Keywords`) by space-padding the contents inside string literals (`(...)` or `<...>`).
   - Sanitizes XML packets inside `<x:xmpmeta>...</x:xmpmeta>` streams.
   - Because byte counts remain identical, all cross-reference (xref) table byte offsets and `startxref` values remain 100% valid, preventing viewer repair prompts while stripping author identities.

5. **CLI Integration (`ember-cli`)**:
   - `ember encrypt` and `ember send` run `strip_metadata` by default and display removed metadata categories to stdout.
   - Added `--no-strip` flag to allow opting out.

## Consequences
- No external heavy dependencies were added; zero unsafe code.
- Metadata stripping is executed on the sender's client before symmetric encryption, ensuring neither the relay server nor recipient ever receives embedded GPS coordinates, camera hardware serials, or document author names.
