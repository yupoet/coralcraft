//! `coralcraft`: Native `egui`/`eframe` desktop vector illustration application and stdio MCP host (L7).
#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]

use std::env;
use std::fs;
use std::io;
use std::path::Path;
use std::process::ExitCode;

use coralcraft_cdr::parse_document;
use coralcraft_doc::Document;
use coralcraft_mcp::McpServer;
use coralcraft_ui_egui::{CoralCraftApp, DesktopWorkspace, Services};

struct EframeHost {
    app: CoralCraftApp,
}

impl eframe::App for EframeHost {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.app.ui(ui);
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("error: {msg}");
            ExitCode::from(1)
        }
    }
}

fn build_desktop_services() -> Services {
    #[cfg(not(target_arch = "wasm32"))]
    {
        Services {
            pick_open: Some(Box::new(|| {
                rfd::FileDialog::new()
                    .set_title("Open Vector Document")
                    .add_filter("Vector Documents", &["coralcraft", "cdr", "cmx", "svg", "dxf", "json"])
                    .pick_file()
            })),
            pick_save: Some(Box::new(|suggested| {
                rfd::FileDialog::new()
                    .set_title("Save / Export Document")
                    .set_file_name(suggested)
                    .add_filter("CoralCraft & Vector Formats", &["coralcraft", "svg", "pdf", "eps", "dxf", "png", "cdr", "cmx", "json"])
                    .save_file()
            })),
            clipboard_get_text: Some(Box::new(|| arboard::Clipboard::new().ok()?.get_text().ok())),
            clipboard_set_text: Some(Box::new(|text| {
                let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
                cb.set_text(text.to_owned()).map_err(|e| e.to_string())
            })),
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        Services::default()
    }
}

fn native_options() -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("CoralCraft")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([800.0, 540.0])
            .with_drag_and_drop(true)
            .with_app_id("org.coralcraft.CoralCraft"),
        ..Default::default()
    }
}

fn run(args: &[String]) -> Result<(), String> {
    let mut mcp_mode = false;
    let mut gui_mode = false;
    let mut dump_workspace = false;
    let mut json_mode = false;
    let mut input_path: Option<String> = None;
    let mut render_out: Option<String> = None;

    let mut i = 0_usize;
    while let Some(arg) = args.get(i) {
        match arg.as_str() {
            "--mcp" => mcp_mode = true,
            "--gui" => gui_mode = true,
            "--dump-workspace" => dump_workspace = true,
            "--json" => json_mode = true,
            "--render-out" => {
                i = i.saturating_add(1);
                let Some(val) = args.get(i) else {
                    return Err("--render-out requires a file path".to_owned());
                };
                render_out = Some(val.clone());
            }
            "--version" | "-V" => {
                println!("coralcraft {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "--help" | "-h" => {
                println!(
                    "CoralCraft Desktop Workspace v{}\n\
                     Usage: coralcraft [file.cdr|file.coralcraft] [--gui] [--dump-workspace] [--json] [--render-out <out.png>] [--mcp]",
                    env!("CARGO_PKG_VERSION")
                );
                return Ok(());
            }
            // macOS Finder passes -psn_0_... when launching .app bundles
            psn if psn.starts_with("-psn_") => {
                gui_mode = true;
            }
            other if !other.starts_with('-') => {
                input_path = Some(other.to_owned());
            }
            other => {
                return Err(format!("unknown argument `{other}`"));
            }
        }
        i = i.saturating_add(1);
    }

    if mcp_mode {
        let mut server = McpServer::new();
        return server.serve_stdio(io::stdin().lock(), io::stdout().lock()).map_err(|e| e.to_string());
    }

    let doc = if let Some(path_str) = &input_path { Some(load_document_from_path(Path::new(path_str))?) } else { None };

    let headless_env = env::var("CORALCRAFT_HEADLESS").is_ok_and(|v| v == "1" || v == "true");
    let should_open_gui = gui_mode || (!dump_workspace && !json_mode && render_out.is_none() && !headless_env);

    if should_open_gui {
        let services = build_desktop_services();
        let mut app = match doc {
            Some(d) => CoralCraftApp::new(d, services),
            None => CoralCraftApp::new_with_welcome_artwork(services),
        };
        if let Some(path_str) = &input_path {
            app.current_file_path = Some(Path::new(path_str).to_path_buf());
        }
        return eframe::run_native("CoralCraft", native_options(), Box::new(move |_cc| Ok(Box::new(EframeHost { app }))))
            .map_err(|e| format!("failed to start native window: {e}"));
    }

    let workspace = DesktopWorkspace::new(doc.unwrap_or_else(Document::new_a4_portrait));

    if let Some(out_path) = render_out {
        let rendered = workspace.render_active_page(96.0).map_err(|e| e.to_string())?;
        fs::write(&out_path, &rendered.png_bytes).map_err(|e| e.to_string())?;
    }

    if dump_workspace || json_mode || input_path.is_none() {
        let snap = workspace.snapshot();
        if json_mode {
            let text = serde_json::to_string_pretty(&snap).map_err(|e| e.to_string())?;
            println!("{text}");
        } else {
            println!(
                "CoralCraft Workspace ({}x{}): tool={:?}, context={:?}, pages={}, palette_swatches={}, status=\"{}\"",
                snap.layout.window_width,
                snap.layout.window_height,
                snap.active_tool,
                snap.property_bar.context,
                snap.page_tabs.len().saturating_sub(2),
                snap.palette_cells.len().saturating_sub(1),
                snap.status_bar.selection_summary
            );
        }
    }

    Ok(())
}

fn load_document_from_path(path: &Path) -> Result<Document, String> {
    let bytes = fs::read(path).map_err(|e| format!("failed to read `{}`: {e}", path.display()))?;
    if let Ok(doc) = coralcraft_doc::load_bytes(&bytes) {
        return Ok(doc);
    }
    let outcome = parse_document(&bytes).map_err(|e| format!("failed to parse `{}` as CDR/CMX: {e}", path.display()))?;
    Ok(outcome.document)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn desktop_app_dumps_workspace_and_json_snapshot() {
        run(&["--dump-workspace".to_owned()]).unwrap();
        run(&["--dump-workspace".to_owned(), "--json".to_owned()]).unwrap();
        run(&["--version".to_owned()]).unwrap();
        run(&["--help".to_owned()]).unwrap();
    }

    #[test]
    fn native_options_configure_viewport_and_desktop_services() {
        let opts = native_options();
        assert_eq!(opts.viewport.title.as_deref(), Some("CoralCraft"));
        assert_eq!(opts.viewport.inner_size, Some(egui::vec2(1440.0, 900.0)));
        let services = build_desktop_services();
        assert!(services.pick_open.is_some());
        assert!(services.pick_save.is_some());
        assert!(services.clipboard_get_text.is_some());
        assert!(services.clipboard_set_text.is_some());
    }
}
