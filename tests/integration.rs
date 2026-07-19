//! End-to-end tests driving the built binary (`env!("CARGO_BIN_EXE_imzip")`)
//! against programmatically generated fixtures in tempdirs.

mod common;

use std::path::Path;
use std::process::{Command, Output};
use tempfile::TempDir;

fn run_imzip(args: &[&str], cwd: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_imzip"))
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("failed to run imzip")
}

fn assert_code(output: &Output, code: i32) {
    assert_eq!(
        output.status.code(),
        Some(code),
        "expected exit {code}, got {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

/// (a) single-file compress jpg -> jpg with quality: exit 0, output exists, smaller.
#[test]
fn single_file_jpeg_compress() {
    let tmp = TempDir::new().unwrap();
    let input = common::save_jpeg(tmp.path(), "photo.jpg", 128, 128, 97);
    let in_size = std::fs::metadata(&input).unwrap().len();
    let out_dir = tmp.path().join("out");

    let output = run_imzip(
        &[
            input.to_str().unwrap(),
            "-q",
            "60",
            "-o",
            out_dir.to_str().unwrap(),
        ],
        tmp.path(),
    );
    assert_code(&output, 0);

    let produced = out_dir.join("photo_imzip.jpg");
    assert!(produced.exists(), "expected {}", produced.display());
    let out_size = std::fs::metadata(&produced).unwrap().len();
    assert!(out_size < in_size, "expected {out_size} < {in_size}");
    image::load_from_memory(&std::fs::read(&produced).unwrap()).unwrap();
}

/// (b) batch dir of generated pngs resized to 50%: exit 0, dimensions halved.
#[test]
fn batch_dir_resize_half() {
    let tmp = TempDir::new().unwrap();
    let in_dir = tmp.path().join("in");
    std::fs::create_dir(&in_dir).unwrap();
    for i in 0..3 {
        common::save_png(&in_dir, &format!("img{i}.png"), 64, 32);
    }
    let out_dir = tmp.path().join("out");

    let output = run_imzip(
        &[
            in_dir.to_str().unwrap(),
            "-o",
            out_dir.to_str().unwrap(),
            "--percent",
            "50",
        ],
        tmp.path(),
    );
    assert_code(&output, 0);

    for i in 0..3 {
        let p = out_dir.join(format!("img{i}_imzip.png"));
        assert!(p.exists(), "missing {}", p.display());
        assert_eq!(image::image_dimensions(&p).unwrap(), (32, 16));
    }
}

/// (c) dry-run writes nothing.
#[test]
fn dry_run_writes_nothing() {
    let tmp = TempDir::new().unwrap();
    let input = common::save_png(tmp.path(), "a.png", 64, 32);
    let out_dir = tmp.path().join("out");

    let output = run_imzip(
        &[
            input.to_str().unwrap(),
            "-o",
            out_dir.to_str().unwrap(),
            "--percent",
            "50",
            "--dry-run",
        ],
        tmp.path(),
    );
    assert_code(&output, 0);
    assert!(
        !out_dir.join("a_imzip.png").exists(),
        "dry-run must not write"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("DRY"),
        "expected dry-run plan in stdout:\n{stdout}"
    );
}

/// (d) default keeps iCCP but strips eXIf; --keep-metadata keeps both;
/// --strip-all-metadata drops both.
#[test]
fn metadata_modes_on_png() {
    use imzip::cli::OutputFormat;
    use imzip::metadata::{extract_png, inject, MetaMode, Metadata};

    let tmp = TempDir::new().unwrap();
    let base = common::save_png(tmp.path(), "meta.png", 16, 16);
    let meta = Metadata {
        icc: Some(vec![7u8; 2048]),
        exif: Some(b"II*\0test-exif".to_vec()),
        xmp: None,
    };
    let with_meta = inject(
        OutputFormat::Png,
        std::fs::read(&base).unwrap(),
        &meta,
        MetaMode::KeepAll,
        (16, 16),
    )
    .unwrap();
    std::fs::write(&base, &with_meta).unwrap();
    let input = base.to_str().unwrap().to_string();

    // --force overrides the "already optimal" skip: injected metadata makes
    // outputs larger than the tiny source, and we still want the file written.
    let run_case = |dir_name: &str, extra: &[&str]| -> Metadata {
        let out_dir = tmp.path().join(dir_name);
        let mut args = vec![input.as_str(), "-o", out_dir.to_str().unwrap(), "--force"];
        args.extend_from_slice(extra);
        let output = run_imzip(&args, tmp.path());
        assert_code(&output, 0);
        let produced = out_dir.join("meta_imzip.png");
        assert!(produced.exists());
        extract_png(&std::fs::read(&produced).unwrap())
    };

    let default_mode = run_case("out_default", &[]);
    assert!(default_mode.icc.is_some(), "default must keep ICC");
    assert!(default_mode.exif.is_none(), "default must strip EXIF");

    let keep = run_case("out_keep", &["--keep-metadata"]);
    assert!(keep.icc.is_some());
    assert_eq!(keep.exif.as_deref(), Some(&b"II*\0test-exif"[..]));

    let stripped = run_case("out_strip", &["--strip-all-metadata"]);
    assert!(stripped.icc.is_none(), "strip-all must drop ICC");
    assert!(stripped.exif.is_none());
}

/// (e) a broken file in a batch fails the batch (exit 1) but other files are
/// still processed.
#[test]
fn batch_continues_after_failure() {
    let tmp = TempDir::new().unwrap();
    let in_dir = tmp.path().join("in");
    std::fs::create_dir(&in_dir).unwrap();
    common::save_png(&in_dir, "good.png", 32, 32);
    std::fs::write(in_dir.join("bogus.jpg"), b"this is not an image").unwrap();
    let out_dir = tmp.path().join("out");

    let output = run_imzip(
        &[in_dir.to_str().unwrap(), "-o", out_dir.to_str().unwrap()],
        tmp.path(),
    );
    assert_code(&output, 1);
    assert!(
        out_dir.join("good_imzip.png").exists(),
        "good file must still be processed"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("FAIL"), "expected failure line:\n{stdout}");
}

/// Bonus: name template with subdirectory placeholders.
#[test]
fn name_template_with_subdirs() {
    let tmp = TempDir::new().unwrap();
    let in_dir = tmp.path().join("in");
    std::fs::create_dir(&in_dir).unwrap();
    common::save_png(&in_dir, "pic.png", 64, 32);
    let out_dir = tmp.path().join("out");

    let output = run_imzip(
        &[
            in_dir.to_str().unwrap(),
            "-o",
            out_dir.to_str().unwrap(),
            "--percent",
            "50",
            "--name-template",
            "{parent}/{name}-{width}x{height}.{ext}",
        ],
        tmp.path(),
    );
    assert_code(&output, 0);
    let produced = out_dir.join("in").join("pic-32x16.png");
    assert!(produced.exists(), "expected {}", produced.display());
    assert_eq!(image::image_dimensions(&produced).unwrap(), (32, 16));
}

/// Bonus: config file in cwd is discovered and applied (percent = 50.0).
#[test]
fn config_file_is_applied() {
    let tmp = TempDir::new().unwrap();
    common::save_png(tmp.path(), "a.png", 64, 32);
    std::fs::write(tmp.path().join("imzip.toml"), "percent = 50.0\n").unwrap();
    let out_dir = tmp.path().join("out");

    let output = run_imzip(&["a.png", "-o", out_dir.to_str().unwrap()], tmp.path());
    assert_code(&output, 0);
    let produced = out_dir.join("a_imzip.png");
    assert!(produced.exists());
    assert_eq!(image::image_dimensions(&produced).unwrap(), (32, 16));
}

/// Bonus: --target-size binary search lands under the target.
#[test]
fn target_size_is_respected() {
    let tmp = TempDir::new().unwrap();
    let input = common::save_jpeg(tmp.path(), "big.jpg", 256, 256, 97);
    let in_size = std::fs::metadata(&input).unwrap().len();
    assert!(
        in_size > 20_000,
        "fixture should start above target, got {in_size}"
    );
    let out_dir = tmp.path().join("out");

    let output = run_imzip(
        &[
            input.to_str().unwrap(),
            "-o",
            out_dir.to_str().unwrap(),
            "--target-size",
            "20KB",
        ],
        tmp.path(),
    );
    assert_code(&output, 0);
    let produced = out_dir.join("big_imzip.jpg");
    assert!(produced.exists());
    let out_size = std::fs::metadata(&produced).unwrap().len();
    assert!(out_size <= 20_000, "expected <= 20000, got {out_size}");
}

/// Bonus: png -> webp conversion changes the extension.
#[test]
fn format_conversion_changes_extension() {
    let tmp = TempDir::new().unwrap();
    let input = common::save_png(tmp.path(), "pic.png", 48, 48);
    let out_dir = tmp.path().join("out");

    let output = run_imzip(
        &[
            input.to_str().unwrap(),
            "-o",
            out_dir.to_str().unwrap(),
            "--format",
            "webp",
            "-q",
            "70",
        ],
        tmp.path(),
    );
    assert_code(&output, 0);
    let produced = out_dir.join("pic_imzip.webp");
    assert!(produced.exists());
    let bytes = std::fs::read(&produced).unwrap();
    assert_eq!(&bytes[..4], b"RIFF");
}
