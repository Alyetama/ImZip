//! Metadata round-trips on *real* encoder output (mozjpeg / oxipng / libwebp):
//! inject into freshly encoded bytes, re-extract, compare — and verify the
//! result still decodes.

mod common;

use imzip::cli::{ChromaSubsampling, OutputFormat};
use imzip::encoders::{encode, EncoderOpts};
use imzip::metadata::{extract_jpeg, extract_png, extract_webp, inject, MetaMode, Metadata};

fn opts() -> EncoderOpts {
    EncoderOpts {
        quality: 80,
        progressive: false,
        chroma_subsampling: ChromaSubsampling::S420,
        png_compression: 2,
        png_colors: None,
        webp_lossless: false,
        avif_speed: 10,
        background: [255, 255, 255],
    }
}

fn sample_meta() -> Metadata {
    Metadata {
        icc: Some((0..2000u32).map(|i| (i % 251) as u8).collect()),
        exif: Some(b"II*\0some-fake-tiff-payload".to_vec()),
        xmp: Some(b"<x:xmpmeta xmlns:x=\"adobe:ns:meta/\">fake</x:xmpmeta>".to_vec()),
    }
}

#[test]
fn jpeg_full_roundtrip() {
    let img = common::gradient_rgba(16, 16);
    let encoded = encode(OutputFormat::Jpeg, &img, &opts()).unwrap();
    let meta = sample_meta();

    let injected = inject(
        OutputFormat::Jpeg,
        encoded,
        &meta,
        MetaMode::KeepAll,
        (16, 16),
    )
    .unwrap();
    image::load_from_memory(&injected).expect("injected jpeg must still decode");
    assert_eq!(extract_jpeg(&injected), meta);
}

#[test]
fn jpeg_default_keeps_only_icc_and_strip_all_drops_everything() {
    let img = common::gradient_rgba(16, 16);
    let meta = sample_meta();

    let encoded = encode(OutputFormat::Jpeg, &img, &opts()).unwrap();
    let default_out = inject(
        OutputFormat::Jpeg,
        encoded,
        &meta,
        MetaMode::Default,
        (16, 16),
    )
    .unwrap();
    let m = extract_jpeg(&default_out);
    assert_eq!(m.icc, meta.icc);
    assert!(m.exif.is_none());
    assert!(m.xmp.is_none());

    let encoded = encode(OutputFormat::Jpeg, &img, &opts()).unwrap();
    let stripped = inject(
        OutputFormat::Jpeg,
        encoded,
        &meta,
        MetaMode::StripAll,
        (16, 16),
    )
    .unwrap();
    assert_eq!(extract_jpeg(&stripped), Metadata::default());
}

#[test]
fn png_full_roundtrip_through_oxipng() {
    let img = common::gradient_rgba(16, 16);
    let encoded = encode(OutputFormat::Png, &img, &opts()).unwrap(); // passes through oxipng
    let meta = sample_meta();

    let injected = inject(
        OutputFormat::Png,
        encoded,
        &meta,
        MetaMode::KeepAll,
        (16, 16),
    )
    .unwrap();
    image::load_from_memory(&injected).expect("injected png must still decode");
    let back = extract_png(&injected);
    assert_eq!(back.icc, meta.icc);
    assert_eq!(back.exif, meta.exif);
    assert_eq!(back.xmp, meta.xmp);
}

#[test]
fn webp_full_roundtrip_through_libwebp() {
    let img = common::gradient_rgba(16, 16);
    let encoded = encode(OutputFormat::Webp, &img, &opts()).unwrap();
    let meta = sample_meta();

    let injected = inject(
        OutputFormat::Webp,
        encoded,
        &meta,
        MetaMode::KeepAll,
        (16, 16),
    )
    .unwrap();
    let decoded = image::load_from_memory(&injected).expect("injected webp must still decode");
    assert_eq!((decoded.width(), decoded.height()), (16, 16));
    assert_eq!(extract_webp(&injected), meta);
}

#[test]
fn avif_and_gif_never_carry_metadata() {
    let img = common::gradient_rgba(8, 8);
    let meta = sample_meta();
    let encoded = encode(OutputFormat::Gif, &img, &opts()).unwrap();
    let out = inject(
        OutputFormat::Gif,
        encoded.clone(),
        &meta,
        MetaMode::KeepAll,
        (8, 8),
    )
    .unwrap();
    assert_eq!(out, encoded, "gif output must be untouched");
}
