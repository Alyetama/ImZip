//! Per-file reports, batch summary and exit-code mapping.

use crate::config::Settings;
use std::path::PathBuf;

#[derive(Debug)]
pub enum Status {
    Success {
        quality: Option<u8>,
    },
    /// Skipped by a skip rule (input under target size, ...).
    Skipped(String),
    /// Pure recompress would have produced a larger file; nothing written.
    AlreadyOptimal,
    /// --dry-run: planned but not written.
    DryRun,
    Failed(String),
}

#[derive(Debug)]
pub struct FileReport {
    pub index: usize,
    pub input: PathBuf,
    pub output: Option<PathBuf>,
    pub in_bytes: u64,
    pub out_bytes: u64,
    pub status: Status,
    pub note: Option<String>,
}

pub fn human_bytes(n: u64) -> String {
    if n < 1_000 {
        format!("{n} B")
    } else if n < 1_000_000 {
        format!("{:.1} KB", n as f64 / 1_000.0)
    } else {
        format!("{:.2} MB", n as f64 / 1_000_000.0)
    }
}

fn saved_pct(in_bytes: u64, out_bytes: u64) -> f64 {
    if in_bytes == 0 {
        0.0
    } else {
        100.0 - (out_bytes as f64 / in_bytes as f64) * 100.0
    }
}

/// Print the human report to stdout (errors to stderr when quiet) and return
/// the process exit code: 1 if any file failed, else 0.
pub fn print_reports(reports: &[FileReport], settings: &Settings) -> i32 {
    let (mut ok, mut skipped, mut failed, mut planned) = (0usize, 0usize, 0usize, 0usize);
    let (mut total_in, mut total_out) = (0u64, 0u64);

    for r in reports {
        match &r.status {
            Status::Success { quality } => {
                ok += 1;
                total_in += r.in_bytes;
                total_out += r.out_bytes;
                if settings.verbose && !settings.quiet {
                    let mut line = format!(
                        "OK   {} -> {} ({}, saved {:.1}%)",
                        r.input.display(),
                        r.output
                            .as_ref()
                            .map(|p| p.display().to_string())
                            .unwrap_or_default(),
                        human_bytes(r.out_bytes),
                        saved_pct(r.in_bytes, r.out_bytes),
                    );
                    if let Some(q) = quality {
                        line.push_str(&format!(" [quality {q}]"));
                    }
                    println!("{line}");
                }
            }
            Status::Skipped(reason) => {
                skipped += 1;
                if settings.verbose && !settings.quiet {
                    println!("SKIP {} ({reason})", r.input.display());
                }
            }
            Status::AlreadyOptimal => {
                skipped += 1;
                if settings.verbose && !settings.quiet {
                    println!("SKIP {} (already optimal)", r.input.display());
                }
            }
            Status::DryRun => {
                planned += 1;
                if !settings.quiet {
                    let note = r.note.as_deref().unwrap_or("");
                    println!(
                        "DRY  {} -> {} [{}]",
                        r.input.display(),
                        r.output
                            .as_ref()
                            .map(|p| p.display().to_string())
                            .unwrap_or_default(),
                        note
                    );
                }
            }
            Status::Failed(msg) => {
                failed += 1;
                if settings.quiet {
                    eprintln!("error: {}: {msg}", r.input.display());
                } else {
                    println!("FAIL {}: {msg}", r.input.display());
                }
            }
        }
    }

    if !settings.quiet {
        if settings.dry_run {
            println!("Dry run: {planned} file(s) planned, {failed} failed — nothing written.");
        } else {
            println!(
                "Done: {ok} ok, {skipped} skipped, {failed} failed | {} -> {} ({:.1}% saved)",
                human_bytes(total_in),
                human_bytes(total_out),
                saved_pct(total_in, total_out),
            );
        }
    }

    if failed > 0 {
        1
    } else {
        0
    }
}
