# imzip

`imzip` is a fast command-line tool for **compressing, resizing and converting images** — built for one-off use and for scripts/pipelines. It processes files, directories and glob patterns in parallel with a progress bar, never aborts a batch because of a single bad file, and uses exit codes you can rely on.

Encoding is done with best-in-class codecs: **mozjpeg** for JPEG, **oxipng** (+ imagequant for palette quantization) for PNG, **libwebp** for WebP and **ravif** (pure-Rust AV1) for AVIF.

## Features

- **Batch + parallel**: files, directories (`-r` to recurse) and glob patterns (`**/*.jpg`), deduplicated, processed on all cores with a progress bar.
- **Compression**: per-format quality control, `--target-size 200KB` binary search, skip-already-optimal logic.
- **Resize**: width/height/percent/max-width/max-height/longest-edge/shortest-edge/max-megapixels; never upscales unless `--allow-upscale`.
- **Conversion**: JPEG / PNG / WebP / AVIF / GIF output, alpha flattening with `--background`.
- **Metadata**: three modes (strip-but-keep-ICC by default, `--strip-all-metadata`, `--keep-metadata`) implemented at the byte level for JPEG, PNG and WebP.
- **Config file**: `imzip.toml` / `.imziprc` discovered from cwd upwards; CLI flags override it.
- **Script-friendly**: `--dry-run`, `--quiet`, `--verbose`, exit codes 0/1/2.

## Install

```sh
cargo install --path .
# or just build:
cargo build --release   # binary at target/release/imzip
```

Requires a C toolchain (mozjpeg and libwebp compile bundled C code). No nasm needed on aarch64.

## Quick start

```sh
imzip photo.jpg                      # -> photo_imzip.jpg (recompressed, defaults)
imzip photo.jpg -q 60                # JPEG quality 60
imzip images/ -o out/ --percent 50   # batch resize into out/
imzip pic.png --format webp --target-size 200KB
imzip images/ -r --strip-all-metadata --in-place
```

## Flag reference

### Input/Output

| Flag | Description |
|---|---|
| `<INPUT>...` | Files, directories, or glob patterns (`*.jpg`, `**/*.png`). Directories are scanned one level deep unless `-r`. Globs walk as deep as the pattern requires. |
| `-r, --recursive` | Recurse into input directories. |
| `-o, --output <DIR>` | Write outputs into `DIR`; directory/glob inputs keep their relative structure under it. |
| `--in-place` | Overwrite input files in place (conflicts with `-o`). If `--format` changes the extension, a sibling file with the new extension is written and the original is kept. |
| `--name-template <T>` | Output name template (may contain subdirectories). Placeholders: `{name}` `{ext}` `{format}` `{width}` `{height}` `{index}` `{parent}`. Default: `{name}_imzip.{ext}` (only used when neither `-o` nor `--in-place` is given). |
| `--dry-run` | Print planned `input -> output` plus resize/format/quality decisions; write nothing. |
| `--force` | Overwrite existing outputs and override both skip rules (see below). |

Notes:

- Without `--force`, an already existing output file is an **error for that file** (never a silent overwrite), except `--in-place`.
- `--target-size` skips files whose input is already under the target.
- A pure recompress (no resize, no format change) that would end up **larger** than the input is skipped and reported as *already optimal*. `--force` overrides both.

### Resize (mutually exclusive modes)

| Flag | Description |
|---|---|
| `--width <PX>` / `--height <PX>` | Both: exact size. One alone: scale keeping aspect ratio. |
| `--percent <F>` | Scale to percent (e.g. `50`, `33.3`). |
| `--max-width <PX>` / `--max-height <PX>` | Downscale to at most this size (no-op if smaller). |
| `--longest-edge <PX>` | Downscale so the longest edge is PX. |
| `--shortest-edge <PX>` | Scale so the shortest edge is PX. |
| `--max-megapixels <F>` | Downscale to at most F megapixels (e.g. `2.0`). |
| `--allow-upscale` | Permit enlarging (default: never upscale; up-only requests become no-ops). |
| `--filter <F>` | `nearest`, `triangle`, `catmull-rom`, `lanczos3` (default). |

### Compression

| Flag | Description |
|---|---|
| `-q, --quality <0-100>` | Quality for lossy formats (JPEG/WebP/AVIF). Default: 75. |
| `--progressive` | Progressive JPEG. |
| `--chroma-subsampling <444\|422\|420>` | JPEG chroma subsampling (default: 420). |
| `--png-compression <0-6>` | oxipng optimization preset (default: 2). |
| `--png-colors <2-256>` | Quantize PNG to a palette of N colors (imagequant), then optimize. |
| `--webp-lossless` | Lossless WebP. |
| `--avif-speed <1-10>` | ravif speed, 1 = slow/smallest (default: 6). |
| `--target-size <SIZE>` | e.g. `200KB`, `1.5MB`, `500K`, `1048576` (SI units: 1 KB = 1000 B). Binary-searches quality (1–100, ≤7 iterations) and keeps the highest quality that fits, or the smallest result if none fits. Lossy formats only (JPEG/WebP/AVIF); errors for PNG/GIF targets. |

### Format

| Flag | Description |
|---|---|
| `--format <jpeg\|png\|webp\|avif\|gif>` | Convert output format. Default: keep input format. Inputs that cannot be re-encoded (BMP, TIFF, TGA, ICO, PNM) fall back to PNG. GIF output is static (single frame). |
| `--background <COLOR>` | Alpha-flattening background for JPEG/GIF: `#RRGGBB` or `white`/`black`/`red`/`green`/`blue` (default: white). |

### Metadata

The three modes are mutually exclusive:

| | EXIF / XMP / IPTC | ICC color profile |
|---|---|---|
| **default** | stripped | **kept** |
| `--strip-all-metadata` | stripped | stripped |
| `--keep-metadata` | copied (best effort) | kept |

Format support for metadata preservation (byte-level re-injection after encoding):

| Format | ICC | EXIF | XMP |
|---|---|---|---|
| JPEG | ✅ APP2 `ICC_PROFILE` (multi-chunk) | ✅ APP1 | ✅ APP1 |
| PNG | ✅ `iCCP` | ✅ `eXIf` | ✅ `iTXt` |
| WebP | ✅ `ICCP` | ✅ `EXIF` | ✅ `XMP ` (VP8X container rebuilt) |
| AVIF | ❌ | ❌ | ❌ |
| GIF | ❌ | ❌ | ❌ |

### Config/Diagnostics

| Flag | Description |
|---|---|
| `--config <PATH>` | Use this config file (default: search cwd and ancestors for `imzip.toml`, `.imziprc`, `.imzip.toml`). |
| `-j, --jobs <N>` | Parallel jobs (default: all CPU cores). |
| `-v, --verbose` | Print a line for every processed file. |
| `--quiet` | Errors only (stderr); no progress bar, no summary. (`-q` is quality.) |
| `--no-progress` | Disable the progress bar (also auto-disabled when stderr is not a tty). |

Human report (per-file lines + summary) goes to **stdout**, progress bar and quiet-mode errors to **stderr**.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | All files processed successfully (skips are OK). |
| 1 | One or more files failed (the batch still processed the rest). |
| 2 | Invalid arguments/usage (clap parse errors, invalid config, unknown template placeholder, …). |

## Config file

Discovery order: `--config <PATH>`, else the first of `imzip.toml`, `.imziprc`, `.imzip.toml` found in the current directory or any ancestor. All fields optional; snake_case names match the long CLI flags. Precedence: **CLI flag > config > built-in default**.

```toml
# imzip.toml — example with every key
recursive = false
output = "out"
# in_place = false
# name_template = "{name}_min.{ext}"
dry_run = false
force = false
jobs = 8
verbose = false
quiet = false
no_progress = false

# resize (one mode only)
# width = 1200
# height = 800
# percent = 50.0
# max_width = 1920
# max_height = 1080
# longest_edge = 1920
# shortest_edge = 1080
# max_megapixels = 2.0
allow_upscale = false
filter = "lanczos3"        # nearest | triangle | catmull-rom | lanczos3

quality = 80               # 0-100
# target_size = "200KB"
format = "webp"            # jpeg | png | webp | avif | gif
background = "#ffffff"
strip_all_metadata = false
keep_metadata = false

[jpeg]
progressive = true
chroma_subsampling = "420" # 444 | 422 | 420

[png]
compression = 2            # 0-6
colors = 256               # 2-256

[webp]
lossless = false

[avif]
speed = 6                  # 1-10
```

## Examples

Single-file compress:

```sh
imzip photo.jpg -q 70
# photo_imzip.jpg written next to the input
```

Batch resize a folder recursively into an output dir (structure mirrored):

```sh
imzip ~/Pictures/vacation -r -o out/ --longest-edge 1600 -q 80
```

Convert PNG to WebP with a size budget:

```sh
imzip screenshot.png --format webp --target-size 200KB
# quality is auto-searched; the chosen quality is shown with --verbose
```

Strip everything including ICC, in place:

```sh
imzip images/ -r --strip-all-metadata --in-place --force
```

Dry run (plan only):

```sh
imzip images/ -r --format avif --percent 50 --dry-run
```

Custom naming with templates:

```sh
imzip "assets/**/*.png" -o dist/ --format webp --name-template "{parent}/{name}-{width}x{height}.{ext}"
# dist/icons/logo-512x512.webp, ...
```

Pipeline usage in shell loops:

```sh
find . -name '*.jpg' -print0 | xargs -0 -n1 -P8 imzip -q 75 --quiet
for f in *.png; do imzip "$f" --format webp || echo "failed: $f"; done
```

`--quiet` + exit codes make it safe in CI: `imzip dist/ -r -q 75 --quiet || notifyFailure` (here `-q` is quality).

## Known limitations

- **AVIF and GIF outputs never carry metadata** (no ICC/EXIF/XMP preservation). AVIF *input* decoding is supported (pure-Rust dav1d).
- **GIF output is static**: only the first frame / a single frame is written.
- **GIF input animation**: only the first frame is read.
- EXIF orientation is not applied when decoding (pixels are used as stored).
- BMP/TIFF/TGA/ICO/PNM inputs without `--format` are written as PNG (they have no encoder in imzip).
- `--target-size` works only for lossy outputs (JPEG/WebP/AVIF).
- `--target-size` uses SI units (1 KB = 1000 bytes).

## Development

```sh
cargo build
cargo test        # unit + integration tests (fixtures generated on the fly)
```

Layout: `src/cli.rs` (clap), `src/config.rs` (config file), `src/resize.rs` (pure resize math), `src/pipeline.rs` (per-file pipeline), `src/encoders/` (codec adapters), `src/metadata.rs` (byte-level ICC/EXIF/XMP), `src/batch.rs` (discovery + parallel runner), `src/report.rs` (report + exit codes).
