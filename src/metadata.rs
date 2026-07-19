//! Byte-level metadata handling for JPEG, PNG and WebP.
//!
//! All encoders used in this crate produce clean output without metadata, so
//! the pipeline extracts metadata from the *input* bytes and re-injects the
//! wanted parts into the *output* bytes after encoding.
//!
//! Normalization rules:
//! - `exif` is stored WITHOUT the JPEG "Exif\0\0" prefix (raw TIFF payload),
//!   matching what PNG `eXIf` and WebP `EXIF` chunks carry.
//! - `icc` is the raw ICC profile (JPEG APP2 multi-chunk sequences are
//!   reassembled on extract and re-chunked on inject).
//! - `xmp` is the raw XMP packet.

use crate::cli::OutputFormat;
use crate::error::{ImzipError, Result};
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use std::io::{Read, Write};

const EXIF_HEADER: &[u8] = b"Exif\0\0";
const XMP_NS_HEADER: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";
const ICC_HEADER: &[u8] = b"ICC_PROFILE\0";
const PNG_SIG: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
const PNG_ICCP_KEYWORD: &[u8] = b"ICC Profile";
const PNG_XMP_KEYWORD: &[u8] = b"XML:com.adobe.xmp";

/// How much metadata the output should carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetaMode {
    /// Strip EXIF/XMP/IPTC, keep the ICC color profile.
    Default,
    /// Drop everything, ICC included.
    StripAll,
    /// Copy ICC + EXIF + XMP (best effort).
    KeepAll,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Metadata {
    pub icc: Option<Vec<u8>>,
    pub exif: Option<Vec<u8>>,
    pub xmp: Option<Vec<u8>>,
}

impl Metadata {
    pub fn is_empty(&self) -> bool {
        self.icc.is_none() && self.exif.is_none() && self.xmp.is_none()
    }
}

/// Inject `meta` (filtered by `mode`) into freshly encoded `data`.
/// AVIF and GIF outputs are returned unchanged.
pub fn inject(
    format: OutputFormat,
    data: Vec<u8>,
    meta: &Metadata,
    mode: MetaMode,
    dims: (u32, u32),
) -> Result<Vec<u8>> {
    if mode == MetaMode::StripAll {
        return Ok(data);
    }
    let effective = match mode {
        MetaMode::Default => Metadata {
            icc: meta.icc.clone(),
            ..Metadata::default()
        },
        MetaMode::KeepAll => meta.clone(),
        MetaMode::StripAll => unreachable!(),
    };
    if effective.is_empty() {
        return Ok(data);
    }
    match format {
        OutputFormat::Jpeg => inject_jpeg(&data, &effective),
        OutputFormat::Png => inject_png(&data, &effective),
        OutputFormat::Webp => inject_webp(&data, &effective, dims.0, dims.1),
        OutputFormat::Avif | OutputFormat::Gif => Ok(data),
    }
}

// ---------------------------------------------------------------- JPEG

/// Extract EXIF (APP1 `Exif\0\0`), XMP (APP1 XMP namespace) and ICC
/// (APP2 `ICC_PROFILE\0`, reassembling multi-chunk sequences) from a JPEG.
pub fn extract_jpeg(data: &[u8]) -> Metadata {
    let mut meta = Metadata::default();
    if data.len() < 4 || data[0] != 0xFF || data[1] != 0xD8 {
        return meta;
    }
    let mut pos = 2;
    let mut icc_chunks: Vec<(u8, Vec<u8>)> = Vec::new();
    while pos + 4 <= data.len() {
        if data[pos] != 0xFF {
            break;
        }
        let marker = data[pos + 1];
        // Standalone markers without a length field.
        if marker == 0xD8 || marker == 0x01 || (0xD0..=0xD7).contains(&marker) {
            pos += 2;
            continue;
        }
        if marker == 0xDA {
            break; // SOS: image data follows, no more metadata segments
        }
        let len = u16::from_be_bytes([data[pos + 2], data[pos + 3]]) as usize;
        if len < 2 || pos + 2 + len > data.len() {
            break;
        }
        let payload = &data[pos + 4..pos + 2 + len];
        match marker {
            0xE1 => {
                if payload.starts_with(EXIF_HEADER) && meta.exif.is_none() {
                    meta.exif = Some(payload[EXIF_HEADER.len()..].to_vec());
                } else if payload.starts_with(XMP_NS_HEADER) && meta.xmp.is_none() {
                    meta.xmp = Some(payload[XMP_NS_HEADER.len()..].to_vec());
                }
            }
            0xE2 if payload.len() >= ICC_HEADER.len() + 2 && payload.starts_with(ICC_HEADER) => {
                let seq = payload[ICC_HEADER.len()];
                if seq >= 1 {
                    icc_chunks.push((seq, payload[ICC_HEADER.len() + 2..].to_vec()));
                }
            }
            _ => {}
        }
        pos += 2 + len;
    }
    if !icc_chunks.is_empty() {
        icc_chunks.sort_by_key(|(seq, _)| *seq);
        let mut icc = Vec::new();
        for (_, chunk) in icc_chunks {
            icc.extend_from_slice(&chunk);
        }
        meta.icc = Some(icc);
    }
    meta
}

/// Insert EXIF/XMP/ICC segments right after SOI. ICC is split into
/// spec-compliant `n/m` APP2 chunks when it does not fit a single segment.
pub fn inject_jpeg(data: &[u8], meta: &Metadata) -> Result<Vec<u8>> {
    if data.len() < 2 || data[0] != 0xFF || data[1] != 0xD8 {
        return Err(ImzipError::metadata("invalid JPEG: missing SOI"));
    }
    let mut out = Vec::with_capacity(data.len() + 1024);
    out.extend_from_slice(&data[..2]); // SOI

    // One APPn segment payload is limited to 65533 bytes (len field includes itself).
    if let Some(exif) = &meta.exif {
        let payload_len = EXIF_HEADER.len() + exif.len();
        if payload_len + 2 <= 0xFFFF {
            out.extend_from_slice(&[0xFF, 0xE1]);
            out.extend_from_slice(&((payload_len + 2) as u16).to_be_bytes());
            out.extend_from_slice(EXIF_HEADER);
            out.extend_from_slice(exif);
        }
    }
    if let Some(xmp) = &meta.xmp {
        let payload_len = XMP_NS_HEADER.len() + xmp.len();
        if payload_len + 2 <= 0xFFFF {
            out.extend_from_slice(&[0xFF, 0xE1]);
            out.extend_from_slice(&((payload_len + 2) as u16).to_be_bytes());
            out.extend_from_slice(XMP_NS_HEADER);
            out.extend_from_slice(xmp);
        }
    }
    if let Some(icc) = &meta.icc {
        let max_chunk = 0xFFFF - 2 - ICC_HEADER.len() - 2; // len bytes + header + seq/count
        let count = icc.len().div_ceil(max_chunk);
        if count <= 255 {
            for (i, chunk) in icc.chunks(max_chunk).enumerate() {
                let payload_len = ICC_HEADER.len() + 2 + chunk.len();
                out.extend_from_slice(&[0xFF, 0xE2]);
                out.extend_from_slice(&((payload_len + 2) as u16).to_be_bytes());
                out.extend_from_slice(ICC_HEADER);
                out.push((i + 1) as u8);
                out.push(count as u8);
                out.extend_from_slice(chunk);
            }
        }
    }
    out.extend_from_slice(&data[2..]);
    Ok(out)
}

// ---------------------------------------------------------------- PNG

/// Extract ICC (`iCCP`), EXIF (`eXIf`) and XMP (`iTXt` with the XMP keyword).
pub fn extract_png(data: &[u8]) -> Metadata {
    let mut meta = Metadata::default();
    if data.len() < 8 || &data[..8] != PNG_SIG {
        return meta;
    }
    let mut pos = 8;
    while pos + 12 <= data.len() {
        let len = u32::from_be_bytes(data[pos..pos + 4].try_into().unwrap()) as usize;
        let ctype: &[u8; 4] = data[pos + 4..pos + 8].try_into().unwrap();
        if pos + 12 + len > data.len() {
            break;
        }
        let payload = &data[pos + 8..pos + 8 + len];
        match ctype {
            b"iCCP" => {
                if let Some(icc) = parse_iccp(payload) {
                    meta.icc = Some(icc);
                }
            }
            b"eXIf" => meta.exif = Some(payload.to_vec()),
            b"iTXt" => {
                if let Some(xmp) = parse_itxt_xmp(payload) {
                    meta.xmp = Some(xmp);
                }
            }
            b"IEND" => break,
            _ => {}
        }
        pos += 12 + len;
    }
    meta
}

fn parse_iccp(payload: &[u8]) -> Option<Vec<u8>> {
    let nul = payload.iter().position(|&b| b == 0)?;
    if &payload[..nul] != PNG_ICCP_KEYWORD {
        return None;
    }
    let rest = payload.get(nul + 1..)?;
    if *rest.first()? != 0 {
        return None; // unknown compression method
    }
    let mut icc = Vec::new();
    ZlibDecoder::new(&rest[1..]).read_to_end(&mut icc).ok()?;
    Some(icc)
}

fn parse_itxt_xmp(payload: &[u8]) -> Option<Vec<u8>> {
    let nul = payload.iter().position(|&b| b == 0)?;
    if &payload[..nul] != PNG_XMP_KEYWORD {
        return None;
    }
    let rest = payload.get(nul + 1..)?;
    if rest.len() < 2 || rest[0] != 0 {
        return None; // compressed iTXt not supported
    }
    let rest = &rest[2..]; // compression flag + method
    let lang_end = rest.iter().position(|&b| b == 0)?;
    let rest = &rest[lang_end + 1..];
    let translated_end = rest.iter().position(|&b| b == 0)?;
    Some(rest[translated_end + 1..].to_vec())
}

fn write_png_chunk(out: &mut Vec<u8>, ctype: &[u8; 4], payload: &[u8]) {
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(ctype);
    out.extend_from_slice(payload);
    let mut crc_input = Vec::with_capacity(4 + payload.len());
    crc_input.extend_from_slice(ctype);
    crc_input.extend_from_slice(payload);
    out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
}

/// Insert `iCCP`/`eXIf`/`iTXt` chunks right before the first `IDAT`.
pub fn inject_png(data: &[u8], meta: &Metadata) -> Result<Vec<u8>> {
    if data.len() < 8 || &data[..8] != PNG_SIG {
        return Err(ImzipError::metadata("invalid PNG: bad signature"));
    }
    let mut pos = 8;
    let mut idat_pos = None;
    while pos + 12 <= data.len() {
        let len = u32::from_be_bytes(data[pos..pos + 4].try_into().unwrap()) as usize;
        if &data[pos + 4..pos + 8] == b"IDAT" {
            idat_pos = Some(pos);
            break;
        }
        pos += 12 + len;
    }
    let insert_at = idat_pos.ok_or_else(|| ImzipError::metadata("invalid PNG: no IDAT chunk"))?;

    let mut chunks = Vec::new();
    if let Some(icc) = &meta.icc {
        let mut payload = PNG_ICCP_KEYWORD.to_vec();
        payload.push(0); // keyword separator
        payload.push(0); // compression method: zlib deflate
        let compressed = {
            let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
            enc.write_all(icc).map_err(ImzipError::Io)?;
            enc.finish().map_err(ImzipError::Io)?
        };
        payload.extend_from_slice(&compressed);
        write_png_chunk(&mut chunks, b"iCCP", &payload);
    }
    if let Some(exif) = &meta.exif {
        write_png_chunk(&mut chunks, b"eXIf", exif);
    }
    if let Some(xmp) = &meta.xmp {
        let mut payload = PNG_XMP_KEYWORD.to_vec();
        payload.push(0); // keyword separator
        payload.push(0); // compression flag: uncompressed
        payload.push(0); // compression method
        payload.push(0); // language tag (empty)
        payload.push(0); // translated keyword (empty)
        payload.extend_from_slice(xmp);
        write_png_chunk(&mut chunks, b"iTXt", &payload);
    }

    let mut out = Vec::with_capacity(data.len() + chunks.len());
    out.extend_from_slice(&data[..insert_at]);
    out.extend_from_slice(&chunks);
    out.extend_from_slice(&data[insert_at..]);
    Ok(out)
}

/// CRC-32 (IEEE) as used by PNG chunks.
fn crc32(data: &[u8]) -> u32 {
    static TABLE: std::sync::OnceLock<[u32; 256]> = std::sync::OnceLock::new();
    let table = TABLE.get_or_init(|| {
        let mut t = [0u32; 256];
        for (i, e) in t.iter_mut().enumerate() {
            let mut c = i as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 {
                    0xEDB8_8320 ^ (c >> 1)
                } else {
                    c >> 1
                };
            }
            *e = c;
        }
        t
    });
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc = table[((crc ^ b as u32) & 0xFF) as usize] ^ (crc >> 8);
    }
    crc ^ 0xFFFF_FFFF
}

// ---------------------------------------------------------------- WebP

/// RIFF chunk directory entry: (fourcc, payload offset, payload len).
fn riff_chunks(data: &[u8]) -> Option<Vec<([u8; 4], usize, usize)>> {
    if data.len() < 12 || &data[0..4] != b"RIFF" || &data[8..12] != b"WEBP" {
        return None;
    }
    let mut chunks = Vec::new();
    let mut pos = 12;
    while pos + 8 <= data.len() {
        let fourcc: [u8; 4] = data[pos..pos + 4].try_into().unwrap();
        let size = u32::from_le_bytes(data[pos + 4..pos + 8].try_into().unwrap()) as usize;
        let start = pos + 8;
        if start + size > data.len() {
            break;
        }
        chunks.push((fourcc, start, size));
        pos = start + size + (size % 2); // chunks are padded to even sizes
    }
    Some(chunks)
}

/// Extract `ICCP`, `EXIF` and `XMP ` chunks from a WebP container.
pub fn extract_webp(data: &[u8]) -> Metadata {
    let mut meta = Metadata::default();
    let Some(chunks) = riff_chunks(data) else {
        return meta;
    };
    for (fourcc, start, size) in chunks {
        let payload = &data[start..start + size];
        match &fourcc {
            b"ICCP" => meta.icc = Some(payload.to_vec()),
            b"EXIF" => meta.exif = Some(payload.to_vec()),
            b"XMP " => meta.xmp = Some(payload.to_vec()),
            _ => {}
        }
    }
    meta
}

fn push_riff_chunk(out: &mut Vec<u8>, fourcc: &[u8; 4], payload: &[u8]) {
    out.extend_from_slice(fourcc);
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
    if payload.len() % 2 == 1 {
        out.push(0); // RIFF chunks are even-aligned
    }
}

/// Inject metadata into a freshly encoded (usually simple VP8/VP8L) WebP.
/// Builds or updates the VP8X chunk (feature flags + canvas size) and fixes
/// the RIFF size. `dims` are the pixel dimensions of the encoded image.
pub fn inject_webp(data: &[u8], meta: &Metadata, width: u32, height: u32) -> Result<Vec<u8>> {
    let chunks =
        riff_chunks(data).ok_or_else(|| ImzipError::metadata("invalid WebP: bad RIFF header"))?;

    // Preserve any pre-existing VP8X flag bits (e.g. alpha), then add ours.
    let mut flags: u8 = 0;
    for (fourcc, start, size) in &chunks {
        if fourcc == b"VP8X" && *size >= 10 {
            flags |= data[*start];
        }
    }
    if meta.icc.is_some() {
        flags |= 0x20;
    }
    if meta.exif.is_some() {
        flags |= 0x08;
    }
    if meta.xmp.is_some() {
        flags |= 0x04;
    }

    let mut body = Vec::with_capacity(data.len());
    let mut vp8x = [0u8; 10];
    vp8x[0] = flags;
    let w = width.saturating_sub(1).min(0x00FF_FFFF);
    let h = height.saturating_sub(1).min(0x00FF_FFFF);
    vp8x[4..7].copy_from_slice(&w.to_le_bytes()[..3]);
    vp8x[7..10].copy_from_slice(&h.to_le_bytes()[..3]);
    push_riff_chunk(&mut body, b"VP8X", &vp8x);

    // ICC must precede the image data.
    if let Some(icc) = &meta.icc {
        push_riff_chunk(&mut body, b"ICCP", icc);
    }
    // Copy all non-metadata chunks (bitstream etc.) in original order.
    for (fourcc, start, size) in &chunks {
        if matches!(fourcc, b"VP8X" | b"ICCP" | b"EXIF" | b"XMP ") {
            continue;
        }
        push_riff_chunk(&mut body, fourcc, &data[*start..*start + *size]);
    }
    // EXIF and XMP go at the end.
    if let Some(exif) = &meta.exif {
        push_riff_chunk(&mut body, b"EXIF", exif);
    }
    if let Some(xmp) = &meta.xmp {
        push_riff_chunk(&mut body, b"XMP ", xmp);
    }

    let mut out = Vec::with_capacity(12 + body.len());
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&((4 + body.len()) as u32).to_le_bytes());
    out.extend_from_slice(b"WEBP");
    out.extend_from_slice(&body);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_png() -> Vec<u8> {
        // 1x1 RGBA PNG: signature + IHDR + IDAT + IEND (contents are not
        // required to be a valid image for chunk-level tests).
        let mut data = PNG_SIG.to_vec();
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&1u32.to_be_bytes()); // width
        ihdr.extend_from_slice(&1u32.to_be_bytes()); // height
        ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // depth, RGBA, compression, filter, interlace
        write_png_chunk(&mut data, b"IHDR", &ihdr);
        write_png_chunk(&mut data, b"IDAT", &[0x78, 0x9C, 0x00, 0x01]);
        write_png_chunk(&mut data, b"IEND", &[]);
        data
    }

    fn minimal_jpeg() -> Vec<u8> {
        // SOI + APP0 (JFIF) + SOS (empty scan).
        let mut data = vec![0xFF, 0xD8];
        data.extend_from_slice(&[0xFF, 0xE0, 0x00, 0x07]);
        data.extend_from_slice(b"JFIF\0");
        data.extend_from_slice(&[0xFF, 0xDA]);
        data
    }

    #[test]
    fn png_iccp_roundtrip() {
        let icc: Vec<u8> = (0..50_000u32).map(|i| (i % 251) as u8).collect();
        let meta = Metadata {
            icc: Some(icc.clone()),
            ..Metadata::default()
        };
        let injected = inject_png(&minimal_png(), &meta).unwrap();
        let back = extract_png(&injected);
        assert_eq!(back.icc.as_deref(), Some(icc.as_slice()));
        assert!(back.exif.is_none());
    }

    #[test]
    fn png_exif_and_xmp_roundtrip() {
        let meta = Metadata {
            icc: None,
            exif: Some(b"II*\0fake-tiff".to_vec()),
            xmp: Some(b"<x:xmpmeta>fake</x:xmpmeta>".to_vec()),
        };
        let injected = inject_png(&minimal_png(), &meta).unwrap();
        let back = extract_png(&injected);
        assert_eq!(back, meta);
    }

    #[test]
    fn png_iccp_chunk_is_before_idat() {
        let meta = Metadata {
            icc: Some(vec![1, 2, 3]),
            ..Metadata::default()
        };
        let injected = inject_png(&minimal_png(), &meta).unwrap();
        let iccp = injected.windows(4).position(|w| w == b"iCCP").unwrap();
        let idat = injected.windows(4).position(|w| w == b"IDAT").unwrap();
        assert!(iccp < idat);
    }

    #[test]
    fn jpeg_exif_icc_xmp_roundtrip() {
        let meta = Metadata {
            icc: Some(vec![9u8; 1000]),
            exif: Some(b"MM\0*fake".to_vec()),
            xmp: Some(b"<xmp>fake</xmp>".to_vec()),
        };
        let injected = inject_jpeg(&minimal_jpeg(), &meta).unwrap();
        let back = extract_jpeg(&injected);
        assert_eq!(back, meta);
    }

    #[test]
    fn jpeg_large_icc_is_chunked_and_reassembled() {
        // Force a multi-chunk ICC sequence (> 65519 bytes).
        let icc: Vec<u8> = (0..150_000u32).map(|i| (i % 253) as u8).collect();
        let meta = Metadata {
            icc: Some(icc.clone()),
            ..Metadata::default()
        };
        let injected = inject_jpeg(&minimal_jpeg(), &meta).unwrap();
        // Must contain at least two APP2 ICC segments.
        let count = injected
            .windows(ICC_HEADER.len())
            .filter(|w| *w == ICC_HEADER)
            .count();
        assert!(count >= 2);
        let back = extract_jpeg(&injected);
        assert_eq!(back.icc.as_deref(), Some(icc.as_slice()));
    }

    #[test]
    fn webp_roundtrip_simple_container() {
        // Minimal simple WebP: RIFF + WEBP + VP8L chunk (fake 5-byte payload,
        // odd length to exercise padding).
        let mut data = b"RIFF".to_vec();
        data.extend_from_slice(&21u32.to_le_bytes());
        data.extend_from_slice(b"WEBP");
        data.extend_from_slice(b"VP8L");
        data.extend_from_slice(&5u32.to_le_bytes());
        data.extend_from_slice(&[0x2F, 0, 0, 0, 0]);
        data.push(0); // padding

        let meta = Metadata {
            icc: Some(vec![7u8; 100]),
            exif: Some(b"II*\0".to_vec()),
            xmp: Some(b"<xmp/>".to_vec()),
        };
        let injected = inject_webp(&data, &meta, 64, 32).unwrap();

        // RIFF size must be consistent.
        let riff_size = u32::from_le_bytes(injected[4..8].try_into().unwrap()) as usize;
        assert_eq!(riff_size + 8, injected.len());

        let back = extract_webp(&injected);
        assert_eq!(back, meta);

        // VP8X flags: icc + exif + xmp set, canvas 64x32 (stored minus 1).
        let chunks = riff_chunks(&injected).unwrap();
        let (_, start, size) = chunks.iter().find(|(f, _, _)| f == b"VP8X").unwrap();
        assert_eq!(*size, 10);
        assert_eq!(injected[*start] & 0x2C, 0x2C);
        let w = u32::from_le_bytes([
            injected[*start + 4],
            injected[*start + 5],
            injected[*start + 6],
            0,
        ]);
        let h = u32::from_le_bytes([
            injected[*start + 7],
            injected[*start + 8],
            injected[*start + 9],
            0,
        ]);
        assert_eq!((w, h), (63, 31));
    }
}
