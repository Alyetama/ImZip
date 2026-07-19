//! AVIF encoding via ravif (pure Rust, rav1e based).

use super::{enc_err, EncoderOpts};
use crate::error::Result;
use image::RgbaImage;
use ravif::{Encoder as RavifEncoder, Img};
use rgb::FromSlice;

pub fn encode(img: &RgbaImage, opts: &EncoderOpts) -> Result<Vec<u8>> {
    let (w, h) = (img.width() as usize, img.height() as usize);
    let rgba: &[ravif::RGBA8] = img.as_raw().as_rgba();
    let view = Img::new(rgba, w, h);
    let encoded = RavifEncoder::new()
        .with_quality(opts.quality as f32)
        .with_speed(opts.avif_speed)
        .encode_rgba(view)
        .map_err(|e| enc_err("avif", e))?;
    Ok(encoded.avif_file)
}
