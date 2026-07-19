//! CLI parsing: valid combos, conflicts, range validation, small parsers.

use clap::Parser;
use imzip::cli::{parse_background, parse_size, validate_template, Cli};

#[test]
fn parses_minimal_invocation() {
    let cli = Cli::try_parse_from(["imzip", "a.jpg"]).unwrap();
    assert_eq!(cli.io.inputs, vec!["a.jpg"]);
    assert!(cli.compression.quality.is_none());
    assert!(cli.resize.width.is_none());
    assert!(!cli.io.recursive);
}

#[test]
fn width_and_height_together_allowed() {
    let cli = Cli::try_parse_from(["imzip", "a.jpg", "--width", "100", "--height", "50"]).unwrap();
    assert_eq!(cli.resize.width, Some(100));
    assert_eq!(cli.resize.height, Some(50));
}

#[test]
fn conflicting_resize_modes_rejected() {
    assert!(
        Cli::try_parse_from(["imzip", "a.jpg", "--percent", "50", "--max-width", "100"]).is_err()
    );
    assert!(Cli::try_parse_from(["imzip", "a.jpg", "--percent", "50", "--width", "100"]).is_err());
    assert!(Cli::try_parse_from([
        "imzip",
        "a.jpg",
        "--longest-edge",
        "100",
        "--shortest-edge",
        "100"
    ])
    .is_err());
    assert!(Cli::try_parse_from([
        "imzip",
        "a.jpg",
        "--max-height",
        "10",
        "--max-megapixels",
        "2.0"
    ])
    .is_err());
}

#[test]
fn metadata_flags_conflict() {
    assert!(
        Cli::try_parse_from(["imzip", "a.jpg", "--strip-all-metadata", "--keep-metadata"]).is_err()
    );
}

#[test]
fn in_place_conflicts_with_output() {
    assert!(Cli::try_parse_from(["imzip", "a.jpg", "--in-place", "-o", "out"]).is_err());
    assert!(Cli::try_parse_from(["imzip", "a.jpg", "--in-place"]).is_ok());
}

#[test]
fn verbose_conflicts_with_quiet() {
    assert!(Cli::try_parse_from(["imzip", "a.jpg", "--verbose", "--quiet"]).is_err());
}

#[test]
fn quality_range_rejected() {
    assert!(Cli::try_parse_from(["imzip", "a.jpg", "-q", "101"]).is_err());
    assert!(Cli::try_parse_from(["imzip", "a.jpg", "-q", "100"]).is_ok());
    assert!(Cli::try_parse_from(["imzip", "a.jpg", "-q", "0"]).is_ok());
}

#[test]
fn other_ranges_validated() {
    assert!(Cli::try_parse_from(["imzip", "a.jpg", "--png-compression", "7"]).is_err());
    assert!(Cli::try_parse_from(["imzip", "a.jpg", "--png-colors", "1"]).is_err());
    assert!(Cli::try_parse_from(["imzip", "a.jpg", "--png-colors", "257"]).is_err());
    assert!(Cli::try_parse_from(["imzip", "a.jpg", "--png-colors", "256"]).is_ok());
    assert!(Cli::try_parse_from(["imzip", "a.jpg", "--avif-speed", "0"]).is_err());
    assert!(Cli::try_parse_from(["imzip", "a.jpg", "--avif-speed", "11"]).is_err());
    assert!(Cli::try_parse_from(["imzip", "a.jpg", "--width", "0"]).is_err());
    assert!(Cli::try_parse_from(["imzip", "a.jpg", "--percent", "0"]).is_err());
    assert!(Cli::try_parse_from(["imzip", "a.jpg", "--percent", "-5"]).is_err());
}

#[test]
fn valid_full_combo_parses() {
    let cli = Cli::try_parse_from([
        "imzip",
        "a.jpg",
        "b.png",
        "-r",
        "-o",
        "out",
        "-q",
        "80",
        "--progressive",
        "--chroma-subsampling",
        "422",
        "--format",
        "webp",
        "--longest-edge",
        "1024",
        "--filter",
        "triangle",
        "--keep-metadata",
        "--dry-run",
        "--jobs",
        "4",
    ])
    .unwrap();
    assert_eq!(cli.io.inputs, vec!["a.jpg", "b.png"]);
    assert_eq!(cli.compression.quality, Some(80));
    assert!(cli.compression.progressive);
    assert!(cli.io.dry_run);
}

#[test]
fn target_size_parsing() {
    assert_eq!(parse_size("1048576").unwrap(), 1048576);
    assert_eq!(parse_size("42").unwrap(), 42);
    assert_eq!(parse_size("512B").unwrap(), 512);
    assert_eq!(parse_size("200KB").unwrap(), 200_000);
    assert_eq!(parse_size("200kb").unwrap(), 200_000);
    assert_eq!(parse_size("500K").unwrap(), 500_000);
    assert_eq!(parse_size("1.5MB").unwrap(), 1_500_000);
    assert_eq!(parse_size("2m").unwrap(), 2_000_000);
    assert_eq!(parse_size(" 300KB ").unwrap(), 300_000);
    assert!(parse_size("").is_err());
    assert!(parse_size("abc").is_err());
    assert!(parse_size("KB").is_err());
    assert!(parse_size("10GB").is_err());
    assert!(parse_size("-5KB").is_err());
}

#[test]
fn template_validation() {
    assert!(validate_template("{name}.{ext}").is_ok());
    assert!(validate_template("{parent}/{name}-{width}x{height}-{index}.{format}.{ext}").is_ok());
    assert!(validate_template("plain-name.jpg").is_ok());
    assert!(validate_template("{bogus}").is_err());
    assert!(validate_template("{name").is_err());
    assert!(validate_template("name}").is_err());
    assert!(validate_template("{}").is_err());
}

#[test]
fn background_parsing() {
    assert_eq!(parse_background("#ff0080").unwrap(), [255, 0, 128]);
    assert_eq!(parse_background("ff0080").unwrap(), [255, 0, 128]);
    assert_eq!(parse_background("white").unwrap(), [255, 255, 255]);
    assert_eq!(parse_background("black").unwrap(), [0, 0, 0]);
    assert_eq!(parse_background("Blue").unwrap(), [0, 0, 255]);
    assert!(parse_background("transparent").is_err());
    assert!(parse_background("#12345").is_err());
    assert!(parse_background("#gg0000").is_err());
}
