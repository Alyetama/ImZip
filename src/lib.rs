//! imzip — batch image compressor / resizer / converter.
//!
//! The binary is a thin shell around [`run`]; all logic lives in the library
//! so integration tests can exercise it directly.

pub mod batch;
pub mod cli;
pub mod config;
pub mod encoders;
pub mod error;
pub mod metadata;
pub mod pipeline;
pub mod report;
pub mod resize;

use clap::Parser;

/// Resolve config, discover inputs, run the batch. Returns the exit code.
/// `Err` means a usage-level error (exit code 2).
pub fn run(cli: cli::Cli) -> Result<i32, String> {
    let settings = config::resolve(&cli)?;
    let (entries, discovery_errors) = batch::discover(&cli.io.inputs, settings.recursive);
    if entries.is_empty() && discovery_errors.is_empty() {
        return Err("no input files matched".to_string());
    }
    Ok(batch::run_batch(entries, discovery_errors, &settings))
}

/// Parse argv, run, map everything to a process exit code.
pub fn main_cli() -> i32 {
    let cli = cli::Cli::parse();
    match run(cli) {
        Ok(code) => code,
        Err(msg) => {
            eprintln!("imzip: error: {msg}");
            2
        }
    }
}
