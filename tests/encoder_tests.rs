//! Encoder smoke tests: an 8x8 RGBA gradient through every encoder must
//! produce non-empty bytes that decode back to 8x8.

mod common;

use imzip::cli::{ChromaSubsampling, OutputFormat};
use imzip::encoders::{encode, EncoderOpts};

fn opts() -> EncoderOpts {
    EncoderOpts {
        quality: 80,
        progressive: true,
        chroma_subsampling: ChromaSubsampling::S420,
        png_compression: 2,
        png_colors: None,
        webp_lossless: false,
        avif_speed: 10, // fast for tests
        background: [255, 255, 255],
    }
}

fn roundtrip(bytes: &[u8], what: &str) {
    let img = image::load_from_memory(bytes)
        .unwrap_or_else(|e| panic!("{what} output should decode: {e}"));
    assert_eq!((img.width(), img.height()), (8, 8), "{what} dimensions");
}

#[test]
fn jpeg_smoke() {
    let img = common::gradient_rgba(8, 8);
    let out = encode(OutputFormat::Jpeg, &img, &opts()).unwrap();
    assert!(!out.is_empty());
    assert_eq!(&out[..2], &[0xFF, 0xD8]);
    roundtrip(&out, "jpeg");
}

#[test]
fn png_smoke() {
    let img = common::gradient_rgba(8, 8);
    let out = encode(OutputFormat::Png, &img, &opts()).unwrap();
    assert!(!out.is_empty());
    assert_eq!(&out[..4], &[0x89, b'P', b'N', b'G']);
    roundtrip(&out, "png");
}

#[test]
fn png_indexed_smoke() {
    let img = common::gradient_rgba(8, 8);
    let mut o = opts();
    o.png_colors = Some(16);
    let out = encode(OutputFormat::Png, &img, &o).unwrap();
    assert!(!out.is_empty());
    roundtrip(&out, "indexed png");
}

#[test]
fn webp_smoke() {
    let img = common::gradient_rgba(8, 8);
    let out = encode(OutputFormat::Webp, &img, &opts()).unwrap();
    assert!(!out.is_empty());
    assert_eq!(&out[..4], b"RIFF");
    assert_eq!(&out[8..12], b"WEBP");
    roundtrip(&out, "webp");
}

#[test]
fn webp_lossless_smoke() {
    let img = common::gradient_rgba(8, 8);
    let mut o = opts();
    o.webp_lossless = true;
    let out = encode(OutputFormat::Webp, &img, &o).unwrap();
    assert!(!out.is_empty());
    roundtrip(&out, "webp lossless");
}

#[test]
fn avif_smoke() {
    let img = common::gradient_rgba(8, 8);
    let out = encode(OutputFormat::Avif, &img, &opts()).unwrap();
    assert!(
        out.len() > 100,
        "avif output suspiciously small: {}",
        out.len()
    );
    assert_eq!(
        &out[4..8],
        b"ftyp",
        "avif output must start with an ftyp box"
    );
    roundtrip(&out, "avif");
}

#[test]
fn gif_smoke() {
    let img = common::gradient_rgba(8, 8);
    let out = encode(OutputFormat::Gif, &img, &opts()).unwrap();
    assert!(!out.is_empty());
    assert_eq!(&out[..3], b"GIF");
    roundtrip(&out, "gif");
}
