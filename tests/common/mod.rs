#![allow(dead_code)]

use image::{ImageBuffer, ImageEncoder, Rgba, RgbaImage};
use std::path::{Path, PathBuf};

/// Deterministic gradient with mild dither noise (so lossy encoders have
/// something to chew on and quality differences are visible in file size).
pub fn gradient_rgba(w: u32, h: u32) -> RgbaImage {
    let (w, h) = (w.max(1), h.max(1));
    ImageBuffer::from_fn(w, h, |x, y| {
        let n = ((x * 7 + y * 13) % 7) as i32 - 3;
        let clamp = |v: i32| v.clamp(0, 255) as u8;
        Rgba([
            clamp(((x * 255) / w) as i32 + n),
            clamp(((y * 255) / h) as i32 - n),
            clamp((((x + y) * 255) / (w + h)) as i32 + n),
            255,
        ])
    })
}

pub fn gradient_rgb(w: u32, h: u32) -> Vec<u8> {
    gradient_rgba(w, h)
        .pixels()
        .flat_map(|p| [p.0[0], p.0[1], p.0[2]])
        .collect()
}

pub fn save_png(dir: &Path, name: &str, w: u32, h: u32) -> PathBuf {
    let p = dir.join(name);
    gradient_rgba(w, h).save(&p).unwrap();
    p
}

pub fn save_jpeg(dir: &Path, name: &str, w: u32, h: u32, quality: u8) -> PathBuf {
    let p = dir.join(name);
    let rgb = gradient_rgb(w, h);
    {
        let mut f = std::fs::File::create(&p).unwrap();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut f, quality)
            .write_image(&rgb, w, h, image::ExtendedColorType::Rgb8)
            .unwrap();
    }
    p
}
