//! Per-file pipeline: decode -> flatten/resize -> encode (with optional
//! target-size quality search) -> metadata -> write.

use crate::batch::InputEntry;
use crate::cli::{Filter, OutputFormat};
use crate::config::Settings;
use crate::encoders::{self, EncoderOpts};
use crate::error::{ImzipError, Result};
use crate::metadata::{self, MetaMode, Metadata};
use crate::report::{FileReport, Status};
use crate::resize;
use image::{DynamicImage, ImageReader, RgbaImage};
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

/// What we know about the input file's format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputKind {
    Jpeg,
    Png,
    Webp,
    Avif,
    Gif,
    /// Anything else the `image` crate can decode (bmp, tiff, tga, ico, pnm...).
    Other,
}

impl InputKind {
    /// The output format "keep input format" maps to, if re-encodable.
    fn default_output(self) -> Option<OutputFormat> {
        match self {
            InputKind::Jpeg => Some(OutputFormat::Jpeg),
            InputKind::Png => Some(OutputFormat::Png),
            InputKind::Webp => Some(OutputFormat::Webp),
            InputKind::Avif => Some(OutputFormat::Avif),
            InputKind::Gif => Some(OutputFormat::Gif),
            InputKind::Other => None, // falls back to PNG
        }
    }
}

pub fn detect_input_kind(path: &Path, bytes: &[u8]) -> InputKind {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .as_deref()
    {
        Some("jpg" | "jpeg") => return InputKind::Jpeg,
        Some("png") => return InputKind::Png,
        Some("webp") => return InputKind::Webp,
        Some("avif") => return InputKind::Avif,
        Some("gif") => return InputKind::Gif,
        _ => {}
    }
    match image::guess_format(bytes) {
        Ok(image::ImageFormat::Jpeg) => InputKind::Jpeg,
        Ok(image::ImageFormat::Png) => InputKind::Png,
        Ok(image::ImageFormat::WebP) => InputKind::Webp,
        Ok(image::ImageFormat::Avif) => InputKind::Avif,
        Ok(image::ImageFormat::Gif) => InputKind::Gif,
        _ => InputKind::Other,
    }
}

impl From<Filter> for image::imageops::FilterType {
    fn from(f: Filter) -> Self {
        match f {
            Filter::Nearest => image::imageops::FilterType::Nearest,
            Filter::Triangle => image::imageops::FilterType::Triangle,
            Filter::CatmullRom => image::imageops::FilterType::CatmullRom,
            Filter::Lanczos3 => image::imageops::FilterType::Lanczos3,
        }
    }
}

pub fn process_file(entry: &InputEntry, settings: &Settings) -> FileReport {
    let mut report = FileReport {
        index: entry.index,
        input: entry.path.clone(),
        output: None,
        in_bytes: 0,
        out_bytes: 0,
        status: Status::Failed("internal error".to_string()),
        note: None,
    };
    if let Err(e) = process_inner(entry, settings, &mut report) {
        report.status = Status::Failed(e.to_string());
    }
    report
}

fn decode(bytes: &[u8]) -> Result<DynamicImage> {
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(ImzipError::decode)?;
    reader.decode().map_err(ImzipError::decode)
}

fn process_inner(entry: &InputEntry, settings: &Settings, report: &mut FileReport) -> Result<()> {
    let in_bytes = fs::read(&entry.path)?;
    report.in_bytes = in_bytes.len() as u64;

    let kind = detect_input_kind(&entry.path, &in_bytes);
    let out_format = settings
        .format
        .or_else(|| kind.default_output())
        .unwrap_or(OutputFormat::Png);

    if settings.target_size.is_some() && !out_format.supports_target_size() {
        return Err(ImzipError::invalid(format!(
            "--target-size is only supported for lossy output (jpeg/webp/avif), not {}",
            out_format.name()
        )));
    }
    if let Some(target) = settings.target_size {
        if report.in_bytes <= target && !settings.force {
            report.status = Status::Skipped("already under target size".to_string());
            return Ok(());
        }
    }

    let img = decode(&in_bytes)?;
    let (orig_w, orig_h) = (img.width(), img.height());
    let target_dims = settings
        .resize
        .as_ref()
        .and_then(|spec| resize::compute_target(orig_w, orig_h, spec, settings.allow_upscale));
    let rgba: RgbaImage = match target_dims {
        Some((w, h)) => image::imageops::resize(&img, w, h, settings.filter.into()),
        None => img.to_rgba8(),
    };
    let dims = (rgba.width(), rgba.height());

    let out_path = output_path(entry, settings, out_format, dims)?;
    report.output = Some(out_path.clone());

    // Human-readable decision summary for --dry-run / --verbose.
    let mut decisions = vec![format!("format {}", out_format.name())];
    match target_dims {
        Some((w, h)) => decisions.push(format!("resize {orig_w}x{orig_h} -> {w}x{h}")),
        None => decisions.push(format!("size {}x{}", dims.0, dims.1)),
    }

    if settings.dry_run {
        match settings.target_size {
            Some(t) => decisions.push(format!(
                "quality auto-search <= {}",
                crate::report::human_bytes(t)
            )),
            None => decisions.push(format!("quality {}", settings.quality)),
        }
        report.note = Some(decisions.join(", "));
        report.status = Status::DryRun;
        return Ok(());
    }

    let opts = EncoderOpts {
        quality: settings.quality,
        progressive: settings.progressive,
        chroma_subsampling: settings.chroma_subsampling,
        png_compression: settings.png_compression,
        png_colors: settings.png_colors,
        webp_lossless: settings.webp_lossless,
        avif_speed: settings.avif_speed,
        background: settings.background,
    };
    let (encoded, quality_used, reached_target) =
        encode_with_search(out_format, &rgba, &opts, settings.target_size)?;

    // Metadata: extract from the original input bytes, inject after encoding.
    let meta = if settings.meta_mode == MetaMode::StripAll {
        Metadata::default()
    } else {
        match kind {
            InputKind::Jpeg => metadata::extract_jpeg(&in_bytes),
            InputKind::Png => metadata::extract_png(&in_bytes),
            InputKind::Webp => metadata::extract_webp(&in_bytes),
            _ => Metadata::default(),
        }
    };
    let final_bytes = metadata::inject(out_format, encoded, &meta, settings.meta_mode, dims)?;
    report.out_bytes = final_bytes.len() as u64;

    // "Already optimal" skip: pure recompress that only made things bigger.
    let format_changed = kind.default_output() != Some(out_format);
    let pure_recompress = !format_changed && target_dims.is_none();
    if pure_recompress && !settings.force && report.out_bytes > report.in_bytes {
        report.status = Status::AlreadyOptimal;
        return Ok(());
    }

    let is_same_file = out_path == entry.path;
    if out_path.exists() && !settings.force && !is_same_file {
        return Err(ImzipError::invalid(format!(
            "output exists: {} (use --force to overwrite)",
            out_path.display()
        )));
    }
    if let Some(parent) = out_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(&out_path, &final_bytes)?;

    if let Some(q) = quality_used {
        decisions.push(format!("quality {q}"));
        if !reached_target {
            decisions.push("target size not reached (used lowest quality)".to_string());
        }
    }
    report.note = Some(decisions.join(", "));
    report.status = Status::Success {
        quality: quality_used,
    };
    Ok(())
}

/// Encode once, or binary-search quality (1..=100, max ~7 iterations) so the
/// output fits `target`. Returns (bytes, quality used, target reached).
pub fn encode_with_search(
    format: OutputFormat,
    img: &RgbaImage,
    opts: &EncoderOpts,
    target: Option<u64>,
) -> Result<(Vec<u8>, Option<u8>, bool)> {
    let Some(target) = target else {
        return Ok((encoders::encode(format, img, opts)?, None, true));
    };

    let (mut lo, mut hi) = (1u8, 100u8);
    let mut best_fit: Option<(u8, Vec<u8>)> = None;
    let mut smallest: Option<Vec<u8>> = None;
    for _ in 0..7 {
        if lo > hi {
            break;
        }
        let mid = (lo + hi) / 2;
        let buf = encoders::encode(format, img, &opts.with_quality(mid))?;
        let size = buf.len() as u64;
        if smallest.as_ref().is_none_or(|b| size < b.len() as u64) {
            smallest = Some(buf.clone());
        }
        if size <= target {
            best_fit = Some((mid, buf));
            lo = mid.saturating_add(1);
        } else {
            hi = mid.saturating_sub(1);
        }
    }

    match best_fit {
        Some((q, buf)) => Ok((buf, Some(q), true)),
        None => {
            let buf = smallest.expect("search runs at least one iteration");
            Ok((buf, Some(1), false))
        }
    }
}

/// Inputs to name-template rendering.
pub struct NameCtx<'a> {
    pub input: &'a Path,
    pub format: OutputFormat,
    pub width: u32,
    pub height: u32,
    pub index: usize,
}

/// Render `{name} {ext} {format} {width} {height} {index} {parent}` placeholders.
pub fn render_name_template(template: &str, ctx: &NameCtx) -> String {
    let name = ctx
        .input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("image");
    let parent = ctx
        .input
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
        .unwrap_or("");
    template
        .replace("{name}", name)
        .replace("{ext}", ctx.format.extension())
        .replace("{format}", ctx.format.name())
        .replace("{width}", &ctx.width.to_string())
        .replace("{height}", &ctx.height.to_string())
        .replace("{index}", &ctx.index.to_string())
        .replace("{parent}", parent)
}

/// Compute the final output path for an input.
pub fn output_path(
    entry: &InputEntry,
    settings: &Settings,
    format: OutputFormat,
    dims: (u32, u32),
) -> Result<PathBuf> {
    if settings.in_place {
        // Same directory and stem; extension follows the output format.
        return Ok(entry.path.with_extension(format.extension()));
    }
    let template = settings
        .name_template
        .as_deref()
        .unwrap_or("{name}_imzip.{ext}");
    let name = render_name_template(
        template,
        &NameCtx {
            input: &entry.path,
            format,
            width: dims.0,
            height: dims.1,
            index: entry.index,
        },
    );
    let base: PathBuf = match &settings.output {
        Some(dir) => match &entry.root {
            // Mirror the directory structure relative to the walked root.
            Some(root) => {
                let rel_dir = entry
                    .path
                    .parent()
                    .and_then(|p| p.strip_prefix(root).ok())
                    .unwrap_or_else(|| Path::new(""));
                dir.join(rel_dir)
            }
            None => dir.clone(),
        },
        None => entry
            .path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from(".")),
    };
    Ok(base.join(name))
}
