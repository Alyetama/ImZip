//! Name-template rendering.

use imzip::cli::OutputFormat;
use imzip::pipeline::{render_name_template, NameCtx};
use std::path::Path;

#[test]
fn renders_all_placeholders() {
    let ctx = NameCtx {
        input: Path::new("photos/vacation/beach.png"),
        format: OutputFormat::Webp,
        width: 800,
        height: 600,
        index: 3,
    };
    assert_eq!(render_name_template("{name}.{ext}", &ctx), "beach.webp");
    assert_eq!(
        render_name_template("{parent}/{name}-{width}x{height}-{index}.{format}", &ctx),
        "vacation/beach-800x600-3.webp"
    );
}

#[test]
fn extension_follows_output_format() {
    let ctx = NameCtx {
        input: Path::new("a.jpg"),
        format: OutputFormat::Avif,
        width: 10,
        height: 10,
        index: 1,
    };
    assert_eq!(
        render_name_template("{name}_imzip.{ext}", &ctx),
        "a_imzip.avif"
    );
    // A hardcoded extension in the template is left alone.
    assert_eq!(render_name_template("{name}.out", &ctx), "a.out");
}

#[test]
fn file_without_parent_dir_gets_empty_parent() {
    let ctx = NameCtx {
        input: Path::new("solo.png"),
        format: OutputFormat::Png,
        width: 1,
        height: 1,
        index: 1,
    };
    assert_eq!(render_name_template("x{parent}y_{name}", &ctx), "xy_solo");
}
