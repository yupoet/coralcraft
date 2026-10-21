//! `coralcraft-cli probe FILE [--json]`: chunk-level inspection of a `.cdr`/`.cmx` container
//! (CoralCraft's CDR/CMX format layer; ported from the pre-fork CLI, V0.4).
//!
//! Reads at most [`MAX_PROBE_BYTES`]; exits 3 for files that are not CDR/CMX at all and 5 for
//! truncated or hostile containers, so scripts can tell "wrong file" from "broken file".

use std::fs;
use std::process::ExitCode;

use coralcraft_cdr::{CdrContainer, ProbeError, ProbeReport};

/// The largest file `probe` reads (a hostile path can't make it allocate without bound).
const MAX_PROBE_BYTES: u64 = 512 * 1024 * 1024;

pub fn run(args: &[String]) -> ExitCode {
    let mut file: Option<&str> = None;
    let mut json = false;
    for arg in args {
        match arg.as_str() {
            "--json" => json = true,
            "-h" | "--help" => {
                println!("usage: coralcraft-cli probe FILE [--json]");
                return ExitCode::SUCCESS;
            }
            _ if arg.starts_with('-') => {
                eprintln!("coralcraft-cli probe: unknown option `{arg}`");
                return ExitCode::from(2);
            }
            _ if file.is_none() => file = Some(arg),
            _ => {
                eprintln!("coralcraft-cli probe: exactly one FILE is expected");
                return ExitCode::from(2);
            }
        }
    }
    let Some(file) = file else {
        eprintln!("coralcraft-cli probe: a FILE is required");
        return ExitCode::from(2);
    };

    let Ok(bytes) = read_bounded(file) else {
        return ExitCode::from(4);
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

fn read_bounded(file: &str) -> Result<Vec<u8>, ExitCode> {
    let meta = match fs::metadata(file) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("coralcraft-cli probe: failed to read file metadata '{file}': {e}");
            return Err(ExitCode::from(4));
        }
    };
    if meta.len() > MAX_PROBE_BYTES {
        eprintln!("coralcraft-cli probe: file '{file}' size ({} bytes) exceeds maximum limit ({MAX_PROBE_BYTES} bytes)", meta.len());
        return Err(ExitCode::from(4));
    }
    fs::read(file).map_err(|e| {
        eprintln!("coralcraft-cli probe: failed to read file '{file}': {e}");
        ExitCode::from(4)
    })
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
        other => {
            println!("container: {other:?}");
        }
    }

    println!("chunks: {}", report.chunks.len());
    for chunk in &report.chunks {
        let indent = "  ".repeat(chunk.depth.saturating_add(1));
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
