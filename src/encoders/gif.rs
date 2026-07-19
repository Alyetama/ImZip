//! GIF encoding via the `image` crate. Static output only (single frame);
//! alpha is flattened onto the background color first.

use super::{enc_err, flatten_to_rgb, EncoderOpts};
use crate::error::Result;
use image::{ExtendedColorType, ImageEncoder, RgbaImage};

pub fn encode(img: &RgbaImage, opts: &EncoderOpts) -> Result<Vec<u8>> {
    let rgb = flatten_to_rgb(img, opts.background);
    let mut buf = Vec::new();
    image::codecs::gif::GifEncoder::new(&mut buf)
        .write_image(&rgb, img.width(), img.height(), ExtendedColorType::Rgb8)
        .map_err(|e| enc_err("gif", e))?;
    Ok(buf)
}
