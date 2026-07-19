//! PNG encoding: plain RGBA via the `image` crate or palette-quantized via
//! imagequant + the `png` crate, then always optimized by oxipng.

use super::{enc_err, EncoderOpts};
use crate::error::Result;
use image::{ExtendedColorType, ImageEncoder, RgbaImage};
use rgb::FromSlice;

pub fn encode(img: &RgbaImage, opts: &EncoderOpts) -> Result<Vec<u8>> {
    let raw = match opts.png_colors {
        Some(colors) => encode_indexed(img, colors)?,
        None => encode_rgba(img)?,
    };
    let oopts = oxipng::Options::from_preset(opts.png_compression);
    oxipng::optimize_from_memory(&raw, &oopts).map_err(|e| enc_err("png", e))
}

fn encode_rgba(img: &RgbaImage) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    image::codecs::png::PngEncoder::new(&mut buf)
        .write_image(
            img.as_raw(),
            img.width(),
            img.height(),
            ExtendedColorType::Rgba8,
        )
        .map_err(|e| enc_err("png", e))?;
    Ok(buf)
}

/// Quantize to at most `colors` palette entries and write an indexed PNG
/// (PLTE + tRNS) with the `png` crate.
fn encode_indexed(img: &RgbaImage, colors: u16) -> Result<Vec<u8>> {
    let (w, h) = (img.width() as usize, img.height() as usize);

    let mut attr = imagequant::new();
    attr.set_max_colors(colors as u32)
        .map_err(|e| enc_err("png", e))?;
    let mut liq_img = attr
        .new_image_borrowed(img.as_raw().as_rgba(), w, h, 0.0)
        .map_err(|e| enc_err("png", e))?;
    let mut result = attr.quantize(&mut liq_img).map_err(|e| enc_err("png", e))?;
    result
        .set_dithering_level(1.0)
        .map_err(|e| enc_err("png", e))?;
    let (palette, indices) = result
        .remapped(&mut liq_img)
        .map_err(|e| enc_err("png", e))?;

    let mut pal_bytes = Vec::with_capacity(palette.len() * 3);
    for c in &palette {
        pal_bytes.extend_from_slice(&[c.r, c.g, c.b]);
    }
    // tRNS may not have trailing fully-opaque entries.
    let trns: Vec<u8> = match palette.iter().rposition(|c| c.a < 255) {
        Some(last) => palette[..=last].iter().map(|c| c.a).collect(),
        None => Vec::new(),
    };

    let mut buf = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut buf, img.width(), img.height());
        encoder.set_color(png::ColorType::Indexed);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_palette(pal_bytes);
        if !trns.is_empty() {
            encoder.set_trns(trns);
        }
        let mut writer = encoder.write_header().map_err(|e| enc_err("png", e))?;
        writer
            .write_image_data(&indices)
            .map_err(|e| enc_err("png", e))?;
        writer.finish().map_err(|e| enc_err("png", e))?;
    }
    Ok(buf)
}
