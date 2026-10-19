//! CoralCraft CLI (`coralcraft-cli`).
//!
//! Exit code convention:
//! - `0`: Success
//! - `2`: CLI argument usage error (`clap` default)
//! - `3`: Not a CDR/CMX file (`NotCdr` / `TooShort`) or unrecognized input format
//! - `4`: File I/O error or file size exceeds safety limit
//! - `5`: Corrupted/truncated container or format parse error
//! - `6`: Rendering or domain pipeline failure

use std::fs;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use coralcraft_cdr::{CdrContainer, ProbeError, ProbeReport};
use coralcraft_color::{ColorError, load_palette_bytes};
use coralcraft_doc::load_bytes;
use coralcraft_render::{RenderOpts, render_page};
use serde::Serialize;

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
    /// Render a `.coralcraft` document page to a deterministic PNG file
    Render {
        /// Path to the input `.coralcraft` file (JSON or ZIP container)
        input: String,
        /// Output PNG file path
        #[arg(short = 'o', long = "output")]
        output: String,
        /// 1-based page number to render (default: 1)
        #[arg(long, default_value_t = 1)]
        page: usize,
        /// Scale factor in pixels per point (default: 1.0 = 72 DPI)
        #[arg(long, default_value_t = 1.0)]
        scale: f64,
        /// Render with a transparent background instead of paper white
        #[arg(long)]
        transparent: bool,
        /// Emit structured JSON summary
        #[arg(long)]
        json: bool,
    },
    /// Inspect a `.xml` or `.cpl` color palette file
    InspectPalette {
        /// Path to the palette file (`.xml` or `.cpl`)
        file: String,
        /// Emit structured JSON output (`schema_version = 2`)
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

/// Maximum file size read into memory at once by the CLI (512 MiB) to prevent OOM on hostile inputs.
const MAX_CLI_FILE_BYTES: u64 = 512 * 1024 * 1024;

fn read_bounded_file(subcmd: &str, file: &str) -> Result<Vec<u8>, ExitCode> {
    let meta = match fs::metadata(file) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("coralcraft-cli {subcmd}: failed to read file metadata '{file}': {e}");
            return Err(ExitCode::from(4));
        }
    };
    if meta.len() > MAX_CLI_FILE_BYTES {
        eprintln!("coralcraft-cli {subcmd}: file '{file}' size ({} bytes) exceeds maximum limit ({} bytes)", meta.len(), MAX_CLI_FILE_BYTES);
        return Err(ExitCode::from(4));
    }
    fs::read(file).map_err(|e| {
        eprintln!("coralcraft-cli {subcmd}: failed to read file '{file}': {e}");
        ExitCode::from(4)
    })
}

fn run_probe(file: &str, json: bool) -> ExitCode {
    let bytes = match read_bounded_file("probe", file) {
        Ok(b) => b,
        Err(code) => return code,
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

#[derive(Serialize)]
struct RenderSummaryJson {
    schema_version: u32,
    input: String,
    output: String,
    page: usize,
    width: u32,
    height: u32,
    png_bytes: usize,
    warnings: Vec<coralcraft_render::RenderWarning>,
}

fn run_render(input: &str, output: &str, page_1based: usize, scale: f64, transparent: bool, json: bool) -> ExitCode {
    if page_1based == 0 {
        eprintln!("coralcraft-cli render: --page is 1-based and must be >= 1");
        return ExitCode::from(2);
    }
    let bytes = match read_bounded_file("render", input) {
        Ok(b) => b,
        Err(code) => return code,
    };

    let doc = match load_bytes(&bytes) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("coralcraft-cli render: failed to load document '{input}': {e}");
            return ExitCode::from(5);
        }
    };

    let opts = RenderOpts { scale, transparent_background: transparent, export_mode: true };

    let rendered = match render_page(&doc, page_1based - 1, &opts) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("coralcraft-cli render: render pipeline failed for '{input}': {e}");
            return ExitCode::from(6);
        }
    };

    if let Err(e) = fs::write(output, &rendered.png_bytes) {
        eprintln!("coralcraft-cli render: failed to write PNG '{output}': {e}");
        return ExitCode::from(4);
    }

    if json {
        let summary = RenderSummaryJson {
            schema_version: 2,
            input: input.to_owned(),
            output: output.to_owned(),
            page: page_1based,
            width: rendered.surface.width,
            height: rendered.surface.height,
            png_bytes: rendered.png_bytes.len(),
            warnings: rendered.warnings,
        };
        if let Ok(s) = serde_json::to_string_pretty(&summary) {
            println!("{s}");
        }
    } else {
        println!(
            "rendered page {} of '{}' -> '{}' ({}x{} px, {} bytes, {} warnings)",
            page_1based,
            input,
            output,
            rendered.surface.width,
            rendered.surface.height,
            rendered.png_bytes.len(),
            rendered.warnings.len()
        );
    }

    ExitCode::SUCCESS
}

#[derive(Serialize)]
struct PaletteSummaryJson {
    schema_version: u32,
    name: String,
    guid: Option<String>,
    swatch_count: usize,
    swatches: Vec<SwatchEntryJson>,
}

#[derive(Serialize)]
struct SwatchEntryJson {
    index: usize,
    name: String,
    srgb_hex: String,
    color: coralcraft_color::Color,
}

fn run_inspect_palette(file: &str, json: bool) -> ExitCode {
    let bytes = match read_bounded_file("inspect-palette", file) {
        Ok(b) => b,
        Err(code) => return code,
    };

    match load_palette_bytes(&bytes) {
        Ok(pal) => {
            if json {
                let summary = PaletteSummaryJson {
                    schema_version: 2,
                    name: pal.name.clone(),
                    guid: pal.guid.clone(),
                    swatch_count: pal.swatches.len(),
                    swatches: pal
                        .swatches
                        .iter()
                        .enumerate()
                        .map(|(i, sw)| SwatchEntryJson { index: i, name: sw.name.clone(), srgb_hex: sw.color.to_hex(), color: sw.color.clone() })
                        .collect(),
                };
                if let Ok(s) = serde_json::to_string_pretty(&summary) {
                    println!("{s}");
                }
            } else {
                println!("palette: {} ({} swatches)", pal.name, pal.swatches.len());
                for (i, sw) in pal.swatches.iter().enumerate() {
                    println!("  [{i}] {} -> {}", sw.name, sw.color.to_hex());
                }
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            let code = match err {
                ColorError::UnknownPaletteFormat => 3,
                _ => 5,
            };
            eprintln!("coralcraft-cli inspect-palette: failed to parse '{file}': {err}");
            ExitCode::from(code)
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Probe { file, json } => run_probe(&file, json),
        Command::Render { input, output, page, scale, transparent, json } => run_render(&input, &output, page, scale, transparent, json),
        Command::InspectPalette { file, json } => run_inspect_palette(&file, json),
    }
}
