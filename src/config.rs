//! Config file discovery (`imzip.toml` / `.imziprc` / `.imzip.toml`) and
//! merging with CLI flags. Precedence: CLI flag > config file > built-in default.

use crate::cli::{self, ChromaSubsampling, Cli, Filter, OutputFormat};
use crate::metadata::MetaMode;
use crate::resize::ResizeSpec;
use clap::ValueEnum;
use serde::Deserialize;
use std::path::{Path, PathBuf};

pub const CONFIG_NAMES: [&str; 3] = ["imzip.toml", ".imziprc", ".imzip.toml"];

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FileConfig {
    pub recursive: Option<bool>,
    pub output: Option<PathBuf>,
    pub in_place: Option<bool>,
    pub name_template: Option<String>,
    pub dry_run: Option<bool>,
    pub force: Option<bool>,
    pub jobs: Option<usize>,
    pub verbose: Option<bool>,
    pub quiet: Option<bool>,
    pub no_progress: Option<bool>,

    pub width: Option<u32>,
    pub height: Option<u32>,
    pub percent: Option<f32>,
    pub max_width: Option<u32>,
    pub max_height: Option<u32>,
    pub longest_edge: Option<u32>,
    pub shortest_edge: Option<u32>,
    pub max_megapixels: Option<f32>,
    pub allow_upscale: Option<bool>,
    pub filter: Option<String>,

    pub quality: Option<u8>,
    pub target_size: Option<String>,

    pub format: Option<String>,
    pub background: Option<String>,
    pub strip_all_metadata: Option<bool>,
    pub keep_metadata: Option<bool>,

    pub jpeg: Option<JpegConfig>,
    pub png: Option<PngConfig>,
    pub webp: Option<WebpConfig>,
    pub avif: Option<AvifConfig>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct JpegConfig {
    pub progressive: Option<bool>,
    pub chroma_subsampling: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PngConfig {
    #[serde(rename = "compression")]
    pub png_compression: Option<u8>,
    #[serde(rename = "colors")]
    pub png_colors: Option<u16>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WebpConfig {
    pub lossless: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AvifConfig {
    pub speed: Option<u8>,
}

/// Fully resolved settings: CLI + config + defaults, all validated.
#[derive(Debug, Clone)]
pub struct Settings {
    pub recursive: bool,
    pub output: Option<PathBuf>,
    pub in_place: bool,
    pub name_template: Option<String>,
    pub dry_run: bool,
    pub force: bool,
    pub jobs: usize,
    pub verbose: bool,
    pub quiet: bool,
    pub no_progress: bool,

    pub resize: Option<ResizeSpec>,
    pub allow_upscale: bool,
    pub filter: Filter,

    pub quality: u8,
    pub progressive: bool,
    pub chroma_subsampling: ChromaSubsampling,
    pub png_compression: u8,
    pub png_colors: Option<u16>,
    pub webp_lossless: bool,
    pub avif_speed: u8,
    pub target_size: Option<u64>,

    pub format: Option<OutputFormat>,
    pub background: [u8; 3],
    pub meta_mode: MetaMode,
}

/// Find the config file: explicit `--config` path, else search the current
/// directory and its ancestors for the known names.
pub fn find_config(explicit: Option<&Path>) -> Result<Option<PathBuf>, String> {
    if let Some(p) = explicit {
        if p.is_file() {
            return Ok(Some(p.to_path_buf()));
        }
        return Err(format!("config file not found: {}", p.display()));
    }
    let mut dir = std::env::current_dir().map_err(|e| format!("cannot read cwd: {e}"))?;
    loop {
        for name in CONFIG_NAMES {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Ok(Some(candidate));
            }
        }
        if !dir.pop() {
            return Ok(None);
        }
    }
}

pub fn load(path: &Path) -> Result<FileConfig, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read config {}: {e}", path.display()))?;
    toml::from_str(&text).map_err(|e| format!("invalid config {}: {e}", path.display()))
}

fn parse_enum<T: ValueEnum>(value: &str, what: &str) -> Result<T, String> {
    T::from_str(value, true).map_err(|_| {
        let variants: Vec<String> = T::value_variants()
            .iter()
            .filter_map(|v| v.to_possible_value().map(|p| p.get_name().to_string()))
            .collect();
        format!(
            "invalid {what} '{value}' (allowed: {})",
            variants.join(", ")
        )
    })
}

/// Build a ResizeSpec from loose fields; errors when several modes are set.
#[allow(clippy::too_many_arguments)]
fn build_spec(
    width: Option<u32>,
    height: Option<u32>,
    percent: Option<f32>,
    max_width: Option<u32>,
    max_height: Option<u32>,
    longest_edge: Option<u32>,
    shortest_edge: Option<u32>,
    max_megapixels: Option<f32>,
    source: &str,
) -> Result<Option<ResizeSpec>, String> {
    if width.is_some() || height.is_some() {
        if width == Some(0) || height == Some(0) {
            return Err(format!("{source}: width/height must be at least 1"));
        }
        return Ok(Some(ResizeSpec::Dimensions { width, height }));
    }
    let mut modes: Vec<ResizeSpec> = Vec::new();
    if let Some(p) = percent {
        if p <= 0.0 || !p.is_finite() {
            return Err(format!("{source}: percent must be a positive number"));
        }
        modes.push(ResizeSpec::Percent(p));
    }
    if let Some(m) = max_width {
        modes.push(ResizeSpec::MaxWidth(m));
    }
    if let Some(m) = max_height {
        modes.push(ResizeSpec::MaxHeight(m));
    }
    if let Some(m) = longest_edge {
        modes.push(ResizeSpec::LongestEdge(m));
    }
    if let Some(m) = shortest_edge {
        modes.push(ResizeSpec::ShortestEdge(m));
    }
    if let Some(mp) = max_megapixels {
        if mp <= 0.0 || !mp.is_finite() {
            return Err(format!(
                "{source}: max_megapixels must be a positive number"
            ));
        }
        modes.push(ResizeSpec::MaxMegapixels(mp));
    }
    if modes.len() > 1 {
        return Err(format!(
            "{source}: conflicting resize options (pick one mode)"
        ));
    }
    Ok(modes.into_iter().next())
}

pub fn resolve(cli: &Cli) -> Result<Settings, String> {
    let cfg: FileConfig = match find_config(cli.diag.config.as_deref())? {
        Some(path) => load(&path)?,
        None => FileConfig::default(),
    };

    // Resize: CLI wins as a whole; config is only used when no CLI resize flag is given.
    let r = &cli.resize;
    let cli_spec = build_spec(
        r.width,
        r.height,
        r.percent,
        r.max_width,
        r.max_height,
        r.longest_edge,
        r.shortest_edge,
        r.max_megapixels,
        "cli",
    )?;
    let resize = match cli_spec {
        Some(spec) => Some(spec),
        None => build_spec(
            cfg.width,
            cfg.height,
            cfg.percent,
            cfg.max_width,
            cfg.max_height,
            cfg.longest_edge,
            cfg.shortest_edge,
            cfg.max_megapixels,
            "config",
        )?,
    };

    let filter = match r.filter {
        Some(f) => f,
        None => match &cfg.filter {
            Some(s) => parse_enum::<Filter>(s, "filter")?,
            None => Filter::default(),
        },
    };

    let quality = cli.compression.quality.or(cfg.quality).unwrap_or(75);
    if quality > 100 {
        return Err("config: quality must be 0-100".to_string());
    }

    let jpeg_cfg = cfg.jpeg.unwrap_or_default();
    let png_cfg = cfg.png.unwrap_or_default();
    let webp_cfg = cfg.webp.unwrap_or_default();
    let avif_cfg = cfg.avif.unwrap_or_default();

    let progressive = cli.compression.progressive || jpeg_cfg.progressive.unwrap_or(false);

    let chroma_subsampling = match cli.compression.chroma_subsampling {
        Some(c) => c,
        None => match &jpeg_cfg.chroma_subsampling {
            Some(s) => parse_enum::<ChromaSubsampling>(s, "chroma_subsampling")?,
            None => ChromaSubsampling::S420,
        },
    };

    let png_compression = cli
        .compression
        .png_compression
        .or(png_cfg.png_compression)
        .unwrap_or(2);
    if png_compression > 6 {
        return Err("config: png.compression must be 0-6".to_string());
    }

    let png_colors = cli.compression.png_colors.or(png_cfg.png_colors);
    if let Some(n) = png_colors {
        if !(2..=256).contains(&n) {
            return Err("config: png.colors must be 2-256".to_string());
        }
    }

    let webp_lossless = cli.compression.webp_lossless || webp_cfg.lossless.unwrap_or(false);

    let avif_speed = cli.compression.avif_speed.or(avif_cfg.speed).unwrap_or(6);
    if !(1..=10).contains(&avif_speed) {
        return Err("config: avif.speed must be 1-10".to_string());
    }

    let target_size = match cli.compression.target_size.clone().or(cfg.target_size) {
        Some(s) => {
            let bytes = cli::parse_size(&s)?;
            if bytes == 0 {
                return Err("target size must be greater than 0".to_string());
            }
            Some(bytes)
        }
        None => None,
    };

    let format = match cli.format.format {
        Some(f) => Some(f),
        None => match &cfg.format {
            Some(s) => Some(parse_enum::<OutputFormat>(s, "format")?),
            None => None,
        },
    };

    let background = match cli.format.background {
        Some(b) => b,
        None => match &cfg.background {
            Some(s) => cli::parse_background(s)?,
            None => [0xFF, 0xFF, 0xFF],
        },
    };

    let strip_all = cli.metadata.strip_all_metadata || cfg.strip_all_metadata.unwrap_or(false);
    let keep = cli.metadata.keep_metadata || cfg.keep_metadata.unwrap_or(false);
    if strip_all && keep {
        return Err("--strip-all-metadata conflicts with --keep-metadata".to_string());
    }
    let meta_mode = if strip_all {
        MetaMode::StripAll
    } else if keep {
        MetaMode::KeepAll
    } else {
        MetaMode::Default
    };

    let verbose = cli.diag.verbose || cfg.verbose.unwrap_or(false);
    let quiet = cli.diag.quiet || cfg.quiet.unwrap_or(false);
    if verbose && quiet {
        return Err("--verbose conflicts with --quiet".to_string());
    }

    let output = cli.io.output.clone().or(cfg.output);
    let in_place = cli.io.in_place || cfg.in_place.unwrap_or(false);
    if output.is_some() && in_place {
        return Err("--in-place conflicts with --output".to_string());
    }

    let name_template = cli.io.name_template.clone().or(cfg.name_template);
    if let Some(t) = &name_template {
        cli::validate_template(t)?;
    }

    let cores = || {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
    };
    let jobs = if cli.diag.sequential {
        1
    } else {
        match cli.diag.jobs {
            Some(cli::Jobs::Auto) => cores(),
            Some(cli::Jobs::N(n)) => n,
            // config: a positive number pins the count, 0 or missing means auto
            None => cfg.jobs.filter(|&n| n >= 1).unwrap_or_else(cores),
        }
    };

    Ok(Settings {
        recursive: cli.io.recursive || cfg.recursive.unwrap_or(false),
        output,
        in_place,
        name_template,
        dry_run: cli.io.dry_run || cfg.dry_run.unwrap_or(false),
        force: cli.io.force || cfg.force.unwrap_or(false),
        jobs,
        verbose,
        quiet,
        no_progress: cli.diag.no_progress || cfg.no_progress.unwrap_or(false),
        resize,
        allow_upscale: cli.resize.allow_upscale || cfg.allow_upscale.unwrap_or(false),
        filter,
        quality,
        progressive,
        chroma_subsampling,
        png_compression,
        png_colors,
        webp_lossless,
        avif_speed,
        target_size,
        format,
        background,
        meta_mode,
    })
}
