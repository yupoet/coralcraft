//! `coralcraft`: Classic CorelDRAW-style desktop application and stdio MCP host (L7).

use std::env;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::Path;
use std::process::ExitCode;

use coralcraft_cdr::parse_document;
use coralcraft_doc::Document;
use coralcraft_mcp::McpServer;
use coralcraft_ui_egui::DesktopWorkspace;

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

fn run(args: &[String]) -> Result<(), String> {
    let mut mcp_mode = false;
    let mut dump_workspace = false;
    let mut json_mode = false;
    let mut input_path: Option<String> = None;
    let mut render_out: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--mcp" => mcp_mode = true,
            "--dump-workspace" => dump_workspace = true,
            "--json" => json_mode = true,
            "--render-out" => {
                i += 1;
                let Some(val) = args.get(i) else {
                    return Err("--render-out requires a file path".to_owned());
                };
                render_out = Some(val.clone());
            }
            "--help" | "-h" => {
                println!(
                    "CoralCraft Desktop Workspace\n\
                     Usage: coralcraft [file.cdr|file.coralcraft] [--dump-workspace] [--json] [--render-out <out.png>] [--mcp]"
                );
                return Ok(());
            }
            other if !other.starts_with('-') => {
                input_path = Some(other.to_owned());
            }
            other => {
                return Err(format!("unknown argument `{other}`"));
            }
        }
        i += 1;
    }

    if mcp_mode {
        let mut server = McpServer::new();
        let stdin = io::stdin();
        let mut stdout = io::stdout().lock();
        for line in stdin.lock().lines() {
            let line = line.map_err(|e| e.to_string())?;
            if line.trim().is_empty() {
                continue;
            }
            let resp = server.handle_json_str(&line);
            writeln!(stdout, "{resp}").map_err(|e| e.to_string())?;
            stdout.flush().map_err(|e| e.to_string())?;
        }
        return Ok(());
    }

    let doc = if let Some(path_str) = &input_path { load_document_from_path(Path::new(path_str))? } else { Document::new_a4_portrait() };

    let workspace = DesktopWorkspace::new(doc);

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
    }
}
