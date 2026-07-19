//! Generate demo fixtures for manual smoke runs: `cargo run --example genimg <DIR>`

use image::{ImageBuffer, ImageEncoder, Rgba, RgbaImage};
use std::path::Path;

fn main() {
    let dir = std::env::args().nth(1).expect("usage: genimg <DIR>");
    let dir = Path::new(&dir);
    std::fs::create_dir_all(dir.join("nested")).unwrap();

    let (w, h) = (800u32, 600u32);
    let img: RgbaImage = ImageBuffer::from_fn(w, h, |x, y| {
        let n = ((x * 7 + y * 13) % 9) as i32 - 4;
        let c = |v: i32| v.clamp(0, 255) as u8;
        Rgba([
            c(((x * 255) / w) as i32 + n),
            c(((y * 255) / h) as i32 - n),
            c((((x + y) * 255) / (w + h)) as i32 + n),
            255,
        ])
    });

    // Opaque gradient -> high-quality JPEG (in root and nested dir).
    let rgb: Vec<u8> = img
        .pixels()
        .flat_map(|p| [p.0[0], p.0[1], p.0[2]])
        .collect();
    for path in [dir.join("photo.jpg"), dir.join("nested").join("photo2.jpg")] {
        let mut f = std::fs::File::create(&path).unwrap();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut f, 97)
            .write_image(&rgb, w, h, image::ExtendedColorType::Rgb8)
            .unwrap();
    }

    // RGBA with a transparent circle -> PNG.
    let mut alpha = img.clone();
    let (cx, cy, r) = (w as f32 / 2.0, h as f32 / 2.0, 180.0f32);
    for (x, y, p) in alpha.enumerate_pixels_mut() {
        let d = ((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)).sqrt();
        p.0[3] = if d < r { 0 } else { 255 };
    }
    alpha.save(dir.join("alpha.png")).unwrap();

    println!("fixtures written to {}", dir.display());
}
