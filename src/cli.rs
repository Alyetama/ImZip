//! Command-line definition (clap derive) plus small validation helpers.

use clap::{Args, Parser, ValueEnum};
use std::path::PathBuf;

const LONG_ABOUT: &str = "\
imzip compresses, resizes and converts images, one-off or in batch, with
parallel processing and a progress bar.

INPUT may be a mix of files, directories and glob patterns (e.g. \"src/**/*.png\").
Directories are scanned non-recursively unless -r is given. Matched files are
de-duplicated. Per-file failures never abort the batch; the exit code tells you
if anything failed (0 = ok, 1 = some files failed, 2 = usage error).

METADATA MODES (mutually exclusive):
  default                 strip EXIF/XMP/IPTC, KEEP the ICC color profile
  --strip-all-metadata    drop everything, including ICC
  --keep-metadata         copy ICC + EXIF + XMP into the output (best effort)
Metadata is only preserved for JPEG, PNG and WebP outputs; AVIF and GIF
outputs never carry metadata.";

#[derive(Parser, Debug)]
#[command(name = "imzip", version, about = "Compress, resize and convert images in batch", long_about = LONG_ABOUT)]
pub struct Cli {
    #[command(flatten)]
    pub io: IoOpts,
    #[command(flatten)]
    pub resize: ResizeOpts,
    #[command(flatten)]
    pub compression: CompressionOpts,
    #[command(flatten)]
    pub format: FormatOpts,
    #[command(flatten)]
    pub metadata: MetadataOpts,
    #[command(flatten)]
    pub diag: DiagOpts,
}

#[derive(Args, Debug)]
#[command(next_help_heading = "Input/Output")]
pub struct IoOpts {
    /// Input files, directories, or glob patterns (e.g. "images/**/*.jpg")
    #[arg(value_name = "INPUT", required = true)]
    pub inputs: Vec<String>,

    /// Recurse into input directories (default: direct children only)
    #[arg(short, long)]
    pub recursive: bool,

    /// Write outputs into DIR; directory inputs keep their relative structure
    #[arg(short, long, value_name = "DIR", conflicts_with = "in_place")]
    pub output: Option<PathBuf>,

    /// Overwrite input files in place
    #[arg(long)]
    pub in_place: bool,

    /// Output name template; placeholders: {name} {ext} {format} {width} {height} {index} {parent}; may contain subdirectories. Default: {name}_imzip.{ext}
    #[arg(long, value_name = "TEMPLATE")]
    pub name_template: Option<String>,

    /// Print planned input -> output actions and decisions, write nothing
    #[arg(long)]
    pub dry_run: bool,

    /// Overwrite existing outputs and override skip rules
    #[arg(long)]
    pub force: bool,
}

#[derive(Args, Debug, Default)]
#[command(next_help_heading = "Resize")]
pub struct ResizeOpts {
    /// Target width in px; with --height = exact size, alone = keep aspect ratio
    #[arg(long, value_name = "PX", value_parser = clap::value_parser!(u32).range(1..))]
    pub width: Option<u32>,

    /// Target height in px; with --width = exact size, alone = keep aspect ratio
    #[arg(long, value_name = "PX", value_parser = clap::value_parser!(u32).range(1..))]
    pub height: Option<u32>,

    /// Scale to PCT percent of original (e.g. 50 or 33.3)
    #[arg(long, value_name = "PCT", value_parser = parse_positive_f32, group = "resize", conflicts_with_all = ["width", "height"])]
    pub percent: Option<f32>,

    /// Downscale to at most PX width
    #[arg(long, value_name = "PX", value_parser = clap::value_parser!(u32).range(1..), group = "resize", conflicts_with_all = ["width", "height"])]
    pub max_width: Option<u32>,

    /// Downscale to at most PX height
    #[arg(long, value_name = "PX", value_parser = clap::value_parser!(u32).range(1..), group = "resize", conflicts_with_all = ["width", "height"])]
    pub max_height: Option<u32>,

    /// Downscale so the longest edge is PX
    #[arg(long, value_name = "PX", value_parser = clap::value_parser!(u32).range(1..), group = "resize", conflicts_with_all = ["width", "height"])]
    pub longest_edge: Option<u32>,

    /// Scale so the shortest edge is PX
    #[arg(long, value_name = "PX", value_parser = clap::value_parser!(u32).range(1..), group = "resize", conflicts_with_all = ["width", "height"])]
    pub shortest_edge: Option<u32>,

    /// Downscale to at most MP megapixels (e.g. 2.0)
    #[arg(long, value_name = "MP", value_parser = parse_positive_f32, group = "resize", conflicts_with_all = ["width", "height"])]
    pub max_megapixels: Option<f32>,

    /// Allow enlarging images (default: never upscale)
    #[arg(long)]
    pub allow_upscale: bool,

    /// Resampling filter
    #[arg(long, value_enum)]
    pub filter: Option<Filter>,
}

#[derive(Args, Debug, Default)]
#[command(next_help_heading = "Compression")]
pub struct CompressionOpts {
    /// Quality for lossy formats (JPEG/WebP/AVIF), 0-100
    #[arg(short, long, value_name = "0-100", value_parser = clap::value_parser!(u8).range(0..=100))]
    pub quality: Option<u8>,

    /// Progressive JPEG
    #[arg(long)]
    pub progressive: bool,

    /// JPEG chroma subsampling
    #[arg(long, value_enum)]
    pub chroma_subsampling: Option<ChromaSubsampling>,

    /// PNG optimization level (oxipng preset)
    #[arg(long, value_name = "0-6", value_parser = clap::value_parser!(u8).range(0..=6))]
    pub png_compression: Option<u8>,

    /// Quantize PNG to N colors (2-256, palette image via imagequant)
    #[arg(long, value_name = "N", value_parser = clap::value_parser!(u16).range(2..=256))]
    pub png_colors: Option<u16>,

    /// Encode WebP losslessly
    #[arg(long)]
    pub webp_lossless: bool,

    /// AVIF encoder speed (1 = slow/smallest, 10 = fast)
    #[arg(long, value_name = "1-10", value_parser = clap::value_parser!(u8).range(1..=10))]
    pub avif_speed: Option<u8>,

    /// Target output size (e.g. 200KB, 1.5MB, 500K, 1048576); binary-searches quality. Lossy formats only
    #[arg(long, value_name = "SIZE")]
    pub target_size: Option<String>,
}

#[derive(Args, Debug, Default)]
#[command(next_help_heading = "Format")]
pub struct FormatOpts {
    /// Convert output to this format (default: keep input format; inputs that cannot be re-encoded, e.g. BMP/TIFF, fall back to PNG)
    #[arg(long, value_enum)]
    pub format: Option<OutputFormat>,

    /// Background for flattening alpha when writing JPEG/GIF: #RRGGBB or white|black|red|green|blue (default: white)
    #[arg(long, value_name = "COLOR", value_parser = parse_background)]
    pub background: Option<[u8; 3]>,
}

#[derive(Args, Debug, Default)]
#[command(next_help_heading = "Metadata")]
pub struct MetadataOpts {
    /// Drop ALL metadata including the ICC color profile (default keeps ICC)
    #[arg(long, conflicts_with = "keep_metadata")]
    pub strip_all_metadata: bool,

    /// Copy ICC + EXIF + XMP from input to output, best effort (JPEG/PNG/WebP only)
    #[arg(long)]
    pub keep_metadata: bool,
}

#[derive(Args, Debug, Default)]
#[command(next_help_heading = "Config/Diagnostics")]
pub struct DiagOpts {
    /// Config file path (default: search cwd and ancestors for imzip.toml, .imziprc, .imzip.toml)
    #[arg(long, value_name = "PATH")]
    pub config: Option<PathBuf>,

    /// Parallel worker threads: a number, or "auto" for all CPU cores (default: auto)
    #[arg(short, long, value_name = "N")]
    pub jobs: Option<Jobs>,

    /// Process files one at a time, no parallelism (same as --jobs 1)
    #[arg(long, conflicts_with = "jobs")]
    pub sequential: bool,

    /// Print a detail line for every processed file
    #[arg(short, long, conflicts_with = "quiet")]
    pub verbose: bool,

    /// Only print errors (to stderr); no progress bar, no summary
    #[arg(long)]
    pub quiet: bool,

    /// Disable the progress bar
    #[arg(long)]
    pub no_progress: bool,
}

/// Value for `--jobs`: an explicit worker count, or `auto` for all CPU cores.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Jobs {
    Auto,
    N(usize),
}

impl std::str::FromStr for Jobs {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.eq_ignore_ascii_case("auto") {
            return Ok(Jobs::Auto);
        }
        match s.parse::<usize>() {
            Ok(n) if n >= 1 => Ok(Jobs::N(n)),
            _ => Err(format!(
                "invalid jobs value `{s}`: expected `auto` or a number >= 1"
            )),
        }
    }
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputFormat {
    Jpeg,
    Png,
    Webp,
    Avif,
    Gif,
}

impl OutputFormat {
    pub fn extension(self) -> &'static str {
        match self {
            OutputFormat::Jpeg => "jpg",
            OutputFormat::Png => "png",
            OutputFormat::Webp => "webp",
            OutputFormat::Avif => "avif",
            OutputFormat::Gif => "gif",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            OutputFormat::Jpeg => "jpeg",
            OutputFormat::Png => "png",
            OutputFormat::Webp => "webp",
            OutputFormat::Avif => "avif",
            OutputFormat::Gif => "gif",
        }
    }

    /// Formats that support a full alpha channel (JPEG/GIF flatten).
    pub fn supports_alpha(self) -> bool {
        matches!(
            self,
            OutputFormat::Png | OutputFormat::Webp | OutputFormat::Avif
        )
    }

    /// Formats where quality-based target-size search makes sense.
    pub fn supports_target_size(self) -> bool {
        matches!(
            self,
            OutputFormat::Jpeg | OutputFormat::Webp | OutputFormat::Avif
        )
    }
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChromaSubsampling {
    #[value(name = "444")]
    S444,
    #[value(name = "422")]
    S422,
    #[value(name = "420")]
    S420,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Filter {
    Nearest,
    Triangle,
    CatmullRom,
    #[default]
    Lanczos3,
}

/// Parse a positive, finite f32 (used for --percent / --max-megapixels).
pub fn parse_positive_f32(s: &str) -> Result<f32, String> {
    let v: f32 = s.parse().map_err(|_| format!("invalid number: {s}"))?;
    if !v.is_finite() || v <= 0.0 {
        return Err(format!("value must be a positive number, got {s}"));
    }
    Ok(v)
}

/// Parse a background color: `#RRGGBB`/`RRGGBB` or a name.
pub fn parse_background(s: &str) -> Result<[u8; 3], String> {
    let t = s.trim();
    match t.to_ascii_lowercase().as_str() {
        "white" => return Ok([0xFF, 0xFF, 0xFF]),
        "black" => return Ok([0x00, 0x00, 0x00]),
        "red" => return Ok([0xFF, 0x00, 0x00]),
        "green" => return Ok([0x00, 0x80, 0x00]),
        "blue" => return Ok([0x00, 0x00, 0xFF]),
        _ => {}
    }
    let hex = t.strip_prefix('#').unwrap_or(t);
    if hex.len() == 6 && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        let v = u32::from_str_radix(hex, 16).map_err(|e| e.to_string())?;
        return Ok([
            ((v >> 16) & 0xFF) as u8,
            ((v >> 8) & 0xFF) as u8,
            (v & 0xFF) as u8,
        ]);
    }
    Err(format!(
        "invalid color '{s}': expected #RRGGBB or one of white, black, red, green, blue"
    ))
}

/// Parse a human size like `200KB`, `1.5MB`, `500K`, `1048576` into bytes.
/// Uses SI units: 1 KB = 1000 bytes, 1 MB = 1_000_000 bytes.
pub fn parse_size(s: &str) -> Result<u64, String> {
    let t = s.trim();
    let split = t
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(t.len());
    let (num, unit) = t.split_at(split);
    if num.is_empty() {
        return Err(format!("invalid size '{s}': missing number"));
    }
    let value: f64 = num
        .parse()
        .map_err(|_| format!("invalid size number: '{num}'"))?;
    if !value.is_finite() || value < 0.0 {
        return Err(format!("invalid size '{s}'"));
    }
    let mult: f64 = match unit.trim().to_ascii_lowercase().as_str() {
        "" | "b" => 1.0,
        "k" | "kb" => 1_000.0,
        "m" | "mb" => 1_000_000.0,
        other => {
            return Err(format!(
                "unknown size suffix '{other}' (use B, K/KB or M/MB)"
            ))
        }
    };
    Ok((value * mult).round() as u64)
}

pub const TEMPLATE_VARS: [&str; 7] = [
    "name", "ext", "format", "width", "height", "index", "parent",
];

/// Validate a `--name-template`: every `{...}` must be a known placeholder.
pub fn validate_template(template: &str) -> Result<(), String> {
    let bytes = template.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => {
                let end = template[i..]
                    .find('}')
                    .ok_or_else(|| "unclosed '{' in name template".to_string())?
                    + i;
                let var = &template[i + 1..end];
                if !TEMPLATE_VARS.contains(&var) {
                    return Err(format!(
                        "unknown template placeholder '{{{var}}}' (allowed: {})",
                        TEMPLATE_VARS.join(", ")
                    ));
                }
                i = end + 1;
            }
            b'}' => return Err("unmatched '}' in name template".to_string()),
            _ => i += 1,
        }
    }
    Ok(())
}
