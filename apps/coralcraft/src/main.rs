//! `coralcraft`: Classic CorelDRAW-style desktop GUI application and stdio MCP host (L7).

pub mod gui_server;

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
    if let Ok(exe_path) = env::current_exe() {
        let exe_name = exe_path.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        if exe_name.ends_with("setup.exe") || args.iter().any(|a| a == "--install") {
            return run_self_installer(&exe_path);
        }
    }

    let mut mcp_mode = false;
    let mut gui_mode = false;
    let mut gui_port: u16 = 0;
    let mut dump_workspace = false;
    let mut json_mode = false;
    let mut input_path: Option<String> = None;
    let mut render_out: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--mcp" => mcp_mode = true,
            "--gui" => gui_mode = true,
            "--gui-port" => {
                i += 1;
                let Some(val) = args.get(i) else {
                    return Err("--gui-port requires a port number".to_owned());
                };
                gui_port = val.parse::<u16>().map_err(|e| format!("invalid port `{val}`: {e}"))?;
                gui_mode = true;
            }
            "--dump-workspace" => dump_workspace = true,
            "--json" => json_mode = true,
            "--render-out" => {
                i += 1;
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
                     Usage: coralcraft [file.cdr|file.coralcraft] [--gui] [--gui-port <port>] [--dump-workspace] [--json] [--render-out <out.png>] [--mcp]",
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

    let headless_env = env::var("CORALCRAFT_HEADLESS").is_ok_and(|v| v == "1" || v == "true");
    let should_open_gui = gui_mode || (!dump_workspace && !json_mode && render_out.is_none() && !headless_env);

    if should_open_gui {
        gui_server::run_gui_server(doc, gui_port, true, None)?;
        return Ok(());
    }

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

/// Self-extracting Windows 64-bit installer handler when invoked as `*-setup.exe` or `--install`.
/// Trailer format: `[coralcraft.exe][coralcraft-cli.exe][cli_len: u64 LE][b"CCSETUP1"]`.
fn run_self_installer(self_exe: &Path) -> Result<(), String> {
    let bytes = fs::read(self_exe).map_err(|e| format!("failed to read installer executable: {e}"))?;
    let install_root =
        env::var("LOCALAPPDATA").map(|p| Path::new(&p).join("Programs").join("CoralCraft")).unwrap_or_else(|_| env::temp_dir().join("CoralCraft"));
    fs::create_dir_all(&install_root).map_err(|e| format!("failed to create {}: {e}", install_root.display()))?;

    let (app_bytes, cli_bytes): (&[u8], Option<&[u8]>) = if bytes.len() > 16 && &bytes[bytes.len() - 8..] == b"CCSETUP1" {
        let mut len_buf = [0u8; 8];
        len_buf.copy_from_slice(&bytes[bytes.len() - 16..bytes.len() - 8]);
        let cli_len = u64::from_le_bytes(len_buf) as usize;
        let trailer_start = bytes.len().saturating_sub(16 + cli_len);
        if trailer_start > 0 && trailer_start + cli_len <= bytes.len() - 16 {
            (&bytes[..trailer_start], Some(&bytes[trailer_start..trailer_start + cli_len]))
        } else {
            (&bytes[..], None)
        }
    } else {
        (&bytes[..], None)
    };

    let installed_app = install_root.join(if cfg!(windows) { "coralcraft.exe" } else { "coralcraft" });
    fs::write(&installed_app, app_bytes).map_err(|e| format!("failed to write {}: {e}", installed_app.display()))?;
    if let Some(cli) = cli_bytes {
        let installed_cli = install_root.join(if cfg!(windows) { "coralcraft-cli.exe" } else { "coralcraft-cli" });
        fs::write(&installed_cli, cli).map_err(|e| format!("failed to write {}: {e}", installed_cli.display()))?;
    }

    if cfg!(windows) {
        let ps_script = format!(
            "$ws = New-Object -ComObject WScript.Shell; \
             $sm = Join-Path $env:APPDATA 'Microsoft\\Windows\\Start Menu\\Programs\\CoralCraft.lnk'; \
             $s1 = $ws.CreateShortcut($sm); $s1.TargetPath = '{}'; $s1.Save(); \
             $dt = Join-Path ([Environment]::GetFolderPath('Desktop')) 'CoralCraft.lnk'; \
             $s2 = $ws.CreateShortcut($dt); $s2.TargetPath = '{}'; $s2.Save()",
            installed_app.display(),
            installed_app.display()
        );
        let _ = std::process::Command::new("powershell").args(["-NoProfile", "-Command", &ps_script]).status();
        let _ = std::process::Command::new(&installed_app).arg("--gui").spawn();
    }
    println!("Installed CoralCraft to {}", install_root.display());
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::thread;

    #[test]
    fn desktop_app_dumps_workspace_and_json_snapshot() {
        run(&["--dump-workspace".to_owned()]).unwrap();
        run(&["--dump-workspace".to_owned(), "--json".to_owned()]).unwrap();
    }

    #[test]
    fn interactive_gui_http_endpoints_serve_frame_and_handle_events() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = thread::spawn(move || {
            let mut session = coralcraft_ui_egui::InteractiveGuiSession::new_with_welcome_artwork();
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                gui_server::handle_http_connection(&mut stream, &mut session).unwrap();
            }
        });

        let mut client1 = TcpStream::connect(("127.0.0.1", port)).unwrap();
        client1.write_all(b"GET /api/frame.png HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
        let mut resp1 = Vec::new();
        client1.read_to_end(&mut resp1).unwrap();
        assert!(resp1.starts_with(b"HTTP/1.1 200 OK"));
        assert!(resp1.windows(8).any(|w| w == b"\x89PNG\r\n\x1a\n"));

        let mut client2 = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let body = br#"{"type":"key_down","key":"F6","ctrl":false,"shift":false}"#;
        let req2 = format!("POST /api/event HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n", body.len());
        client2.write_all(req2.as_bytes()).unwrap();
        client2.write_all(body).unwrap();
        let mut resp2 = String::new();
        client2.read_to_string(&mut resp2).unwrap();
        assert!(resp2.contains("Rectangle"));

        handle.join().unwrap();
    }
}
