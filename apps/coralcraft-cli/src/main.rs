//! CoralCraft CLI (`coralcraft-cli`).
//!
//! Exit code convention:
//! - `0`: Success
//! - `2`: CLI argument usage error (`clap` default)
//! - `3`: Not a CDR/CMX file (`NotCdr` / `TooShort`)
//! - `4`: File I/O error or file size exceeds safety limit
//! - `5`: Corrupted/truncated container or safety limit exceeded (`Truncated` / `Invalid` / `LimitExceeded`)

use std::fs;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use coralcraft_cdr::{CdrContainer, ProbeError, ProbeReport};

#[derive(Parser)]
#[command(name = "coralcraft-cli", version, about = "CoralCraft command-line utility")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Inspect the container generation, version, and top-level chunks of a CDR/CMX file
    Probe {
        /// Path to the file to inspect
        file: String,
        /// Emit structured JSON output (`schema_version = 1`)
        #[arg(long)]
        json: bool,
    },
}

fn print_human_report(file: &str, report: &ProbeReport) {
    println!("file: {file} ({} bytes)", report.file_size);
    match &report.container {
        CdrContainer::Riff { kind, endian, form, version, .. } => {
            println!(
                "container: RIFF ({endian:?}), kind={kind:?}, form={form}, effective_version={:?} (form={:?}, vrsn={:?})",
                version.effective_version, version.form_version, version.vrsn_chunk
            );
        }
        CdrContainer::Zip { primary_entry, role, has_data_file_list, entry_count, inner_version } => {
            println!(
                "container: ZIP (role={role:?}, primary={primary_entry}, entries={entry_count}, data_file_list={has_data_file_list}), inner_version={:?}",
                inner_version.and_then(|v| v.effective_version)
            );
        }
        CdrContainer::LegacyWl => {
            println!("container: Legacy WL (pre-1992 CorelDRAW v1/v2)");
        }
        _ => {
            println!("container: {:?}", report.container);
        }
    }

    println!("chunks: {}", report.chunks.len());
    for chunk in &report.chunks {
        let indent = "  ".repeat(chunk.depth + 1);
        if let Some(lt) = &chunk.list_type {
            println!("{indent}{} ({lt}) offset={} size={} opaque={}", chunk.fourcc, chunk.offset, chunk.size, chunk.opaque_payload);
        } else {
            println!("{indent}{} offset={} size={}", chunk.fourcc, chunk.offset, chunk.size);
        }
    }

    if !report.warnings.is_empty() {
        println!("warnings ({}):", report.warnings.len());
        for w in &report.warnings {
            println!("  - {w:?}");
        }
    }
}

/// Maximum file size read into memory at once by the CLI probe (512 MiB) to prevent OOM on hostile inputs.
const MAX_PROBE_FILE_BYTES: u64 = 512 * 1024 * 1024;

fn run_probe(file: &str, json: bool) -> ExitCode {
    let meta = match fs::metadata(file) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("coralcraft-cli probe: failed to read file metadata '{file}': {e}");
            return ExitCode::from(4);
        }
    };
    if meta.len() > MAX_PROBE_FILE_BYTES {
        eprintln!("coralcraft-cli probe: file '{file}' size ({} bytes) exceeds maximum probe limit ({} bytes)", meta.len(), MAX_PROBE_FILE_BYTES);
        return ExitCode::from(4);
    }

    let bytes = match fs::read(file) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("coralcraft-cli probe: failed to read file '{file}': {e}");
            return ExitCode::from(4);
        }
    };

    match coralcraft_cdr::probe(&bytes) {
        Ok(report) => {
            if json {
                match serde_json::to_string_pretty(&report) {
                    Ok(s) => println!("{s}"),
                    Err(e) => {
                        eprintln!("coralcraft-cli probe: JSON serialization failed: {e}");
                        return ExitCode::from(5);
                    }
                }
            } else {
                print_human_report(file, &report);
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            let code = match &err {
                ProbeError::NotCdr { .. } | ProbeError::TooShort { .. } => 3,
                ProbeError::Truncated { .. } | ProbeError::Invalid { .. } | ProbeError::LimitExceeded { .. } => 5,
                _ => 5,
            };
            eprintln!("coralcraft-cli probe: not a CDR/CMX or invalid container ({file}): {err}");
            ExitCode::from(code)
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Probe { file, json } => run_probe(&file, json),
    }
}
