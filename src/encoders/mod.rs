//! Encoder dispatch. Every encoder takes the final RGBA image and returns
//! encoded bytes *without* any metadata (metadata is re-injected afterwards
//! by `crate::metadata`).

pub mod avif;
pub mod gif;
pub mod jpeg;
pub mod png;
pub mod webp;

use crate::cli::{ChromaSubsampling, OutputFormat};
use crate::error::{ImzipError, Result};
use image::RgbaImage;

#[derive(Clone, Debug)]
pub struct EncoderOpts {
    pub quality: u8,
    pub progressive: bool,
    pub chroma_subsampling: ChromaSubsampling,
    pub png_compression: u8,
    pub png_colors: Option<u16>,
    pub webp_lossless: bool,
    pub avif_speed: u8,
    pub background: [u8; 3],
}

impl EncoderOpts {
    pub fn with_quality(&self, quality: u8) -> Self {
        EncoderOpts {
            quality,
            ..self.clone()
        }
    }
}

pub fn encode(format: OutputFormat, img: &RgbaImage, opts: &EncoderOpts) -> Result<Vec<u8>> {
    match format {
        OutputFormat::Jpeg => jpeg::encode(img, opts),
        OutputFormat::Png => png::encode(img, opts),
        OutputFormat::Webp => webp::encode(img, opts),
        OutputFormat::Avif => avif::encode(img, opts),
        OutputFormat::Gif => gif::encode(img, opts),
    }
}

/// Flatten RGBA onto a solid background, returning packed RGB bytes.
pub fn flatten_to_rgb(img: &RgbaImage, bg: [u8; 3]) -> Vec<u8> {
    let mut out = Vec::with_capacity((img.width() * img.height() * 3) as usize);
    for p in img.pixels() {
        let [r, g, b, a] = p.0;
        if a == 255 {
            out.extend_from_slice(&[r, g, b]);
        } else if a == 0 {
            out.extend_from_slice(&bg);
        } else {
            let af = a as u32;
            let blend =
                |c: u8, bg: u8| ((c as u32 * af + bg as u32 * (255 - af) + 127) / 255) as u8;
            out.extend_from_slice(&[blend(r, bg[0]), blend(g, bg[1]), blend(b, bg[2])]);
        }
    }
    out
}

pub(crate) fn enc_err(format: &'static str, e: impl std::fmt::Display) -> ImzipError {
    ImzipError::encode(format, e)
}
