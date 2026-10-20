//! CoralCraft CLI (`coralcraft-cli`).
//!
//! Exit code convention:
//! - `0`: Success
//! - `2`: CLI argument usage error (`clap` default)
//! - `3`: Not a CDR/CMX file (`NotCdr` / `TooShort`) or unrecognized input/output format
//! - `4`: File I/O error or file size exceeds safety limit
//! - `5`: Corrupted/truncated container or format parse error
//! - `6`: Rendering or vector export pipeline failure

use std::fs;
use std::path::Path;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use coralcraft_cdr::{CdrContainer, ProbeError, ProbeReport, parse_document};
use coralcraft_color::{ColorError, load_palette_bytes};
use coralcraft_doc::{DocError, Document, import_dxf, import_svg, load_bytes, save_json, save_zip};
use coralcraft_engine::ActivePageTarget;
use coralcraft_render::{RenderOpts, export_dxf, export_eps, export_pdf, export_svg, render_page};
use coralcraft_ui_egui::DesktopWorkspace;
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
    /// Render a `.coralcraft` or `.cdr` document page to a deterministic PNG file
    Render {
        /// Path to the input `.coralcraft` or `.cdr` file
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
    /// Export a `.cdr` or `.coralcraft` document to `.svg`, `.pdf`, `.png`, `.cdr`, `.cmx`, or `.coralcraft`
    Export {
        /// Path to the input `.cdr` or `.coralcraft` file
        input: String,
        /// Output file path (`.svg`, `.pdf`, `.png`, `.cdr`, `.cmx`, `.coralcraft`, or `.json`)
        #[arg(short = 'o', long = "output")]
        output: String,
        /// 1-based page number for single-page formats (`svg`, `png`) (default: 1)
        #[arg(long, default_value_t = 1)]
        page: usize,
        /// Rasterization DPI for `.png` output (default: 96.0)
        #[arg(long, default_value_t = 96.0)]
        dpi: f64,
        /// Emit structured JSON summary
        #[arg(long)]
        json: bool,
    },
    /// Render the complete 7-panel classic CorelDRAW desktop workspace to a PNG screenshot
    WorkspacePreview {
        /// Path to the input `.cdr`, `.cmx`, or `.coralcraft` file
        input: String,
        /// Output PNG file path
        #[arg(short = 'o', long = "output")]
        output: String,
        /// 1-based page number to display (default: 1)
        #[arg(long, default_value_t = 1)]
        page: usize,
        /// Window width in pixels (default: 1440)
        #[arg(long, default_value_t = 1440)]
        width: u32,
        /// Window height in pixels (default: 900)
        #[arg(long, default_value_t = 900)]
        height: u32,
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
    /// Run the Model Context Protocol (MCP) server on stdio (newline-delimited JSON-RPC 2.0)
    Mcp,
}

/// Runs the stdio MCP server until stdin reaches EOF.
fn run_mcp() -> ExitCode {
    let mut server = coralcraft_mcp::McpServer::new();
    match server.serve_stdio(std::io::stdin().lock(), std::io::stdout().lock()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("mcp: stdio transport error: {err}");
            ExitCode::from(1)
        }
    }
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

/// Loads a native `.coralcraft`, `.svg`, `.dxf`, or `.cdr`/`.cmx` document.
fn load_any_document(subcmd: &str, input: &str, bytes: &[u8]) -> Result<(Document, Vec<String>), ExitCode> {
    let ext = Path::new(input).extension().and_then(|s| s.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();
    if ext == "svg" {
        let text = std::str::from_utf8(bytes).map_err(|e| {
            eprintln!("coralcraft-cli {subcmd}: invalid UTF-8 in SVG '{input}': {e}");
            ExitCode::from(5)
        })?;
        return import_svg(text).map(|doc| (doc, Vec::new())).map_err(|e| {
            eprintln!("coralcraft-cli {subcmd}: failed to import SVG '{input}': {e}");
            ExitCode::from(5)
        });
    }
    if ext == "dxf" {
        let text = std::str::from_utf8(bytes).map_err(|e| {
            eprintln!("coralcraft-cli {subcmd}: invalid UTF-8 in DXF '{input}': {e}");
            ExitCode::from(5)
        })?;
        return import_dxf(text).map(|doc| (doc, Vec::new())).map_err(|e| {
            eprintln!("coralcraft-cli {subcmd}: failed to import DXF '{input}': {e}");
            ExitCode::from(5)
        });
    }

    match load_bytes(bytes) {
        Ok(doc) => return Ok((doc, Vec::new())),
        Err(DocError::BadMagic | DocError::InvalidZipContainer(_)) => {}
        Err(e) => {
            eprintln!("coralcraft-cli {subcmd}: failed to load .coralcraft document '{input}': {e}");
            return Err(ExitCode::from(5));
        }
    }

    match parse_document(bytes) {
        Ok(outcome) => Ok((outcome.document, outcome.parse_warnings)),
        Err(e) => {
            eprintln!("coralcraft-cli {subcmd}: failed to load or parse '{input}': {e}");
            Err(ExitCode::from(5))
        }
    }
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

    let (doc, _parse_warnings) = match load_any_document("render", input, &bytes) {
        Ok(pair) => pair,
        Err(code) => return code,
    };

    let opts = RenderOpts { scale, transparent_background: transparent, export_mode: true };

    let rendered = match render_page(&doc, page_1based.saturating_sub(1), &opts) {
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
struct ExportSummaryJson {
    schema_version: u32,
    input: String,
    output: String,
    format: String,
    page: usize,
    page_count: usize,
    output_bytes: usize,
    parse_warnings: Vec<String>,
}

fn run_export(input: &str, output: &str, page_1based: usize, dpi: f64, json: bool) -> ExitCode {
    if page_1based == 0 {
        eprintln!("coralcraft-cli export: --page is 1-based and must be >= 1");
        return ExitCode::from(2);
    }
    if !dpi.is_finite() || dpi <= 0.0 || dpi > 4800.0 {
        eprintln!("coralcraft-cli export: --dpi must be in (0.0, 4800.0]");
        return ExitCode::from(2);
    }

    let bytes = match read_bounded_file("export", input) {
        Ok(b) => b,
        Err(code) => return code,
    };

    let (doc, parse_warnings) = match load_any_document("export", input, &bytes) {
        Ok(pair) => pair,
        Err(code) => return code,
    };

    for w in &parse_warnings {
        eprintln!("[warn] {w}");
    }

    let ext = Path::new(output).extension().and_then(|s| s.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();

    let (format_name, out_bytes) = match ext.as_str() {
        "svg" => match export_svg(&doc, page_1based.saturating_sub(1)) {
            Ok(svg) => ("svg", svg.into_bytes()),
            Err(e) => {
                eprintln!("coralcraft-cli export: SVG export failed for '{input}': {e}");
                return ExitCode::from(6);
            }
        },
        "pdf" => match export_pdf(&doc) {
            Ok(pdf) => ("pdf", pdf),
            Err(e) => {
                eprintln!("coralcraft-cli export: PDF export failed for '{input}': {e}");
                return ExitCode::from(6);
            }
        },
        "png" => {
            let opts = RenderOpts { scale: dpi / 72.0, transparent_background: false, export_mode: true };
            match render_page(&doc, page_1based.saturating_sub(1), &opts) {
                Ok(r) => ("png", r.png_bytes),
                Err(e) => {
                    eprintln!("coralcraft-cli export: PNG export failed for '{input}': {e}");
                    return ExitCode::from(6);
                }
            }
        }
        "coralcraft" => match save_zip(&doc) {
            Ok(z) => ("coralcraft", z),
            Err(e) => {
                eprintln!("coralcraft-cli export: .coralcraft ZIP save failed: {e}");
                return ExitCode::from(6);
            }
        },
        "json" => match save_json(&doc) {
            Ok(j) => ("json", j),
            Err(e) => {
                eprintln!("coralcraft-cli export: .coralcraft JSON save failed: {e}");
                return ExitCode::from(6);
            }
        },
        "cdr" => match coralcraft_cdr::write_cdr7(&doc) {
            Ok(c) => ("cdr", c),
            Err(e) => {
                eprintln!("coralcraft-cli export: CDR7 export failed: {e}");
                return ExitCode::from(6);
            }
        },
        "cmx" => match coralcraft_cdr::write_cmx1(&doc) {
            Ok(c) => ("cmx", c),
            Err(e) => {
                eprintln!("coralcraft-cli export: CMX1 export failed: {e}");
                return ExitCode::from(6);
            }
        },
        "eps" => match export_eps(&doc, page_1based.saturating_sub(1)) {
            Ok(eps) => ("eps", eps.into_bytes()),
            Err(e) => {
                eprintln!("coralcraft-cli export: EPS export failed for '{input}': {e}");
                return ExitCode::from(6);
            }
        },
        "dxf" => match export_dxf(&doc, page_1based.saturating_sub(1)) {
            Ok(dxf) => ("dxf", dxf.into_bytes()),
            Err(e) => {
                eprintln!("coralcraft-cli export: DXF export failed for '{input}': {e}");
                return ExitCode::from(6);
            }
        },
        other => {
            eprintln!(
                "coralcraft-cli export: unsupported output extension '.{other}' (expected .svg, .pdf, .eps, .dxf, .png, .cdr, .cmx, .coralcraft, or .json)"
            );
            return ExitCode::from(3);
        }
    };

    if let Err(e) = fs::write(output, &out_bytes) {
        eprintln!("coralcraft-cli export: failed to write '{output}': {e}");
        return ExitCode::from(4);
    }

    if json {
        let summary = ExportSummaryJson {
            schema_version: 2,
            input: input.to_owned(),
            output: output.to_owned(),
            format: format_name.to_owned(),
            page: page_1based,
            page_count: doc.pages.len(),
            output_bytes: out_bytes.len(),
            parse_warnings,
        };
        if let Ok(s) = serde_json::to_string_pretty(&summary) {
            println!("{s}");
        }
    } else {
        println!(
            "exported '{}' -> '{}' (format={}, pages={}, {} bytes, {} warnings)",
            input,
            output,
            format_name,
            doc.pages.len(),
            out_bytes.len(),
            parse_warnings.len()
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
        Err(ColorError::UnsupportedBinaryCpl) => {
            eprintln!("coralcraft-cli inspect-palette: binary .cpl palette header detected (body decoding scheduled for M4)");
            ExitCode::from(3)
        }
        Err(e) => {
            eprintln!("coralcraft-cli inspect-palette: failed to parse palette '{file}': {e}");
            ExitCode::from(5)
        }
    }
}

fn run_workspace_preview(input: &str, output: &str, page_1based: usize, width: u32, height: u32, json: bool) -> ExitCode {
    if page_1based == 0 {
        eprintln!("coralcraft-cli workspace-preview: --page is 1-based and must be >= 1");
        return ExitCode::from(2);
    }
    let bytes = match read_bounded_file("workspace-preview", input) {
        Ok(b) => b,
        Err(code) => return code,
    };
    let (doc, _parse_warnings) = match load_any_document("workspace-preview", input, &bytes) {
        Ok(pair) => pair,
        Err(code) => return code,
    };

    let page_idx = page_1based.saturating_sub(1);
    if page_idx >= doc.pages.len() {
        eprintln!("coralcraft-cli workspace-preview: page {} out of range (document has {} pages)", page_1based, doc.pages.len());
        return ExitCode::from(6);
    }

    let mut ws = DesktopWorkspace::new(doc);
    ws.window_width = width.max(640);
    ws.window_height = height.max(480);
    ws.engine.set_active_target(ActivePageTarget::Page(page_idx));
    if let Some(first_obj) = ws.engine.document.pages.get(page_idx).into_iter().flat_map(|p| &p.layers).flat_map(|l| &l.objects).next() {
        let id = first_obj.id;
        ws.engine.set_selection(vec![id]);
    }

    let rendered = match ws.render_workspace_png() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("coralcraft-cli workspace-preview: workspace render failed for '{input}': {e}");
            return ExitCode::from(6);
        }
    };

    if let Err(e) = fs::write(output, &rendered.png_bytes) {
        eprintln!("coralcraft-cli workspace-preview: failed to write PNG '{output}': {e}");
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
            "rendered workspace preview page {} of '{}' -> '{}' ({}x{} px, {} bytes)",
            page_1based,
            input,
            output,
            rendered.surface.width,
            rendered.surface.height,
            rendered.png_bytes.len()
        );
    }

    ExitCode::SUCCESS
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Probe { file, json } => run_probe(&file, json),
        Command::Render { input, output, page, scale, transparent, json } => run_render(&input, &output, page, scale, transparent, json),
        Command::Export { input, output, page, dpi, json } => run_export(&input, &output, page, dpi, json),
        Command::WorkspacePreview { input, output, page, width, height, json } => run_workspace_preview(&input, &output, page, width, height, json),
        Command::InspectPalette { file, json } => run_inspect_palette(&file, json),
        Command::Mcp => run_mcp(),
    }
}
