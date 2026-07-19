//! WebP encoding via the `webp` crate (libwebp).

use super::{enc_err, EncoderOpts};
use crate::error::Result;
use image::RgbaImage;

pub fn encode(img: &RgbaImage, opts: &EncoderOpts) -> Result<Vec<u8>> {
    let encoder = webp::Encoder::from_rgba(img.as_raw(), img.width(), img.height());
    let mem = if opts.webp_lossless {
        encoder.encode_lossless()
    } else {
        encoder
            .encode_simple(false, opts.quality as f32)
            .map_err(|e| enc_err("webp", format!("{e:?}")))?
    };
    Ok(mem.to_vec())
}
