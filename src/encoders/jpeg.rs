//! JPEG encoding via mozjpeg: quality, progressive mode, chroma subsampling.

use super::{enc_err, flatten_to_rgb, EncoderOpts};
use crate::cli::ChromaSubsampling;
use crate::error::Result;
use image::RgbaImage;
use mozjpeg::{ColorSpace, Compress};

pub fn encode(img: &RgbaImage, opts: &EncoderOpts) -> Result<Vec<u8>> {
    let (w, h) = (img.width() as usize, img.height() as usize);
    // JPEG has no alpha channel: flatten onto the background color.
    let rgb = flatten_to_rgb(img, opts.background);

    let mut comp = Compress::new(ColorSpace::JCS_RGB);
    comp.set_size(w, h);
    comp.set_quality(opts.quality as f32);
    if opts.progressive {
        comp.set_progressive_mode();
    }
    // mozjpeg takes chroma *pixel sizes* relative to luma (1,1).
    let (cb, cr) = match opts.chroma_subsampling {
        ChromaSubsampling::S444 => ((1, 1), (1, 1)),
        ChromaSubsampling::S422 => ((2, 1), (2, 1)),
        ChromaSubsampling::S420 => ((2, 2), (2, 2)),
    };
    comp.set_chroma_sampling_pixel_sizes(cb, cr);

    let mut started = comp
        .start_compress(Vec::new())
        .map_err(|e| enc_err("jpeg", e))?;
    started
        .write_scanlines(&rgb)
        .map_err(|e| enc_err("jpeg", e))?;
    started.finish().map_err(|e| enc_err("jpeg", e))
}
