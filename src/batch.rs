//! Input discovery (files / directories / glob patterns) and the parallel
//! batch runner (rayon + indicatif).

use crate::config::Settings;
use crate::pipeline::process_file;
use crate::report::{print_reports, FileReport, Status};
use globset::GlobBuilder;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::collections::HashSet;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Extensions we attempt to decode; used to filter directory walk results.
pub const SUPPORTED_EXTENSIONS: [&str; 16] = [
    "jpg", "jpeg", "png", "webp", "avif", "gif", "bmp", "tiff", "tif", "tga", "ico", "pnm", "pgm",
    "ppm", "pbm", "pam",
];

#[derive(Debug, Clone)]
pub struct InputEntry {
    pub path: PathBuf,
    /// Walked root used for directory mirroring (dir inputs: the dir itself;
    /// glob inputs: the static prefix; plain files: None).
    pub root: Option<PathBuf>,
    /// 1-based index in the sorted batch, usable as `{index}` in templates.
    pub index: usize,
}

pub fn has_glob_meta(s: &str) -> bool {
    s.contains('*') || s.contains('?') || s.contains('[') || s.contains('{')
}

/// The non-glob leading path components of a pattern, used as walk root.
fn static_glob_root(pattern: &str) -> PathBuf {
    let mut root = PathBuf::new();
    for comp in Path::new(pattern).components() {
        let s = comp.as_os_str().to_string_lossy();
        if has_glob_meta(&s) {
            break;
        }
        root.push(comp.as_os_str());
    }
    if root.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        root
    }
}

/// Expand all inputs into a deduplicated, sorted list of files plus a list of
/// (input, error) pairs for inputs that could not be resolved at all.
pub fn discover(inputs: &[String], recursive: bool) -> (Vec<InputEntry>, Vec<(String, String)>) {
    let mut found: Vec<(PathBuf, Option<PathBuf>)> = Vec::new();
    let mut errors: Vec<(String, String)> = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();

    fn push(
        path: PathBuf,
        root: Option<PathBuf>,
        found: &mut Vec<(PathBuf, Option<PathBuf>)>,
        seen: &mut HashSet<PathBuf>,
    ) {
        let key = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if seen.insert(key) {
            found.push((path, root));
        }
    }

    for input in inputs {
        if has_glob_meta(input) {
            let root = static_glob_root(input);
            if !root.is_dir() {
                errors.push((
                    input.clone(),
                    format!("glob base directory not found: {}", root.display()),
                ));
                continue;
            }
            let matcher = match GlobBuilder::new(input).literal_separator(true).build() {
                Ok(g) => g.compile_matcher(),
                Err(e) => {
                    errors.push((input.clone(), format!("invalid glob pattern: {e}")));
                    continue;
                }
            };
            for e in WalkDir::new(&root).into_iter().filter_map(|e| e.ok()) {
                if !e.file_type().is_file() {
                    continue;
                }
                let path = e.path().to_path_buf();
                // Normalize a leading "./" away so "*.jpg" matches "./a.jpg".
                let candidate = if root == Path::new(".") {
                    path.strip_prefix(".")
                        .map(Path::to_path_buf)
                        .unwrap_or_else(|_| path.clone())
                } else {
                    path.clone()
                };
                if matcher.is_match(&candidate) {
                    push(path, Some(root.clone()), &mut found, &mut seen);
                }
            }
        } else {
            let p = PathBuf::from(input);
            if p.is_dir() {
                let walker = WalkDir::new(&p).follow_links(false);
                let walker = if recursive {
                    walker
                } else {
                    walker.max_depth(1)
                };
                for e in walker.into_iter().filter_map(|e| e.ok()) {
                    if !e.file_type().is_file() {
                        continue;
                    }
                    let path = e.path().to_path_buf();
                    let supported = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(|e| SUPPORTED_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
                        .unwrap_or(false);
                    if supported {
                        push(path, Some(p.clone()), &mut found, &mut seen);
                    }
                }
            } else if p.is_file() {
                // Explicitly named files are taken as-is, whatever the extension.
                push(p, None, &mut found, &mut seen);
            } else {
                errors.push((input.clone(), "no such file or directory".to_string()));
            }
        }
    }

    found.sort_by(|a, b| a.0.cmp(&b.0));
    let entries = found
        .into_iter()
        .enumerate()
        .map(|(i, (path, root))| InputEntry {
            path,
            root,
            index: i + 1,
        })
        .collect();
    (entries, errors)
}

/// Run the batch in parallel, print progress + report, return the exit code.
pub fn run_batch(
    entries: Vec<InputEntry>,
    discovery_errors: Vec<(String, String)>,
    settings: &Settings,
) -> i32 {
    let show_progress = !settings.quiet && !settings.no_progress && std::io::stderr().is_terminal();
    let pb = if show_progress {
        let pb = ProgressBar::new(entries.len() as u64);
        pb.set_style(
            ProgressStyle::with_template("[{bar:40.cyan/blue}] {pos}/{len}")
                .unwrap_or_else(|_| ProgressStyle::default_bar()),
        );
        pb
    } else {
        ProgressBar::hidden()
    };

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(settings.jobs)
        .build()
        .unwrap_or_else(|_| {
            rayon::ThreadPoolBuilder::new()
                .build()
                .expect("rayon thread pool")
        });

    let worker_pb = pb.clone();
    let mut reports: Vec<FileReport> = pool.install(|| {
        entries
            .par_iter()
            .map(|entry| {
                let report = process_file(entry, settings);
                worker_pb.inc(1);
                report
            })
            .collect()
    });
    pb.finish_and_clear();

    for (input, msg) in discovery_errors {
        reports.push(FileReport {
            index: usize::MAX,
            input: PathBuf::from(input),
            output: None,
            in_bytes: 0,
            out_bytes: 0,
            status: Status::Failed(msg),
            note: None,
        });
    }
    reports.sort_by_key(|r| r.index);

    print_reports(&reports, settings)
}
