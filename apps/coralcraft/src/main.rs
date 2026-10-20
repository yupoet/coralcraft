//! `coralcraft`: Classic CorelDRAW-style desktop GUI application and stdio MCP host (L7).

pub mod gui_server;

use std::env;
use std::fs;
use std::io;
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
        return server.serve_stdio(io::stdin().lock(), io::stdout().lock()).map_err(|e| e.to_string());
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

    /// Serves `requests.len()` connections on an ephemeral port with `guard` and returns the raw
    /// response of each request.
    fn serve_and_collect(requests: Vec<Vec<u8>>, token: &str) -> Vec<String> {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let guard = gui_server::RequestGuard::new(token.to_owned(), port);
        let n = requests.len();
        let handle = thread::spawn(move || {
            let mut session = coralcraft_ui_egui::InteractiveGuiSession::new_with_welcome_artwork();
            for _ in 0..n {
                let (mut stream, _) = listener.accept().unwrap();
                let _ = gui_server::handle_http_connection(&mut stream, &mut session, &guard);
            }
        });
        let mut out = Vec::new();
        for req in requests {
            let req = String::from_utf8(req).unwrap().replace("{PORT}", &port.to_string());
            let mut client = TcpStream::connect(("127.0.0.1", port)).unwrap();
            client.write_all(req.as_bytes()).unwrap();
            let mut resp = Vec::new();
            client.read_to_end(&mut resp).unwrap();
            out.push(String::from_utf8_lossy(&resp).into_owned());
        }
        handle.join().unwrap();
        out
    }

    #[test]
    fn gui_server_rejects_requests_without_token_wrong_host_or_foreign_origin() {
        let t = "0123456789abcdef0123456789abcdef";
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#;
        let resps = serve_and_collect(
            vec![
                // 1. no token at all
                b"GET /api/export/svg HTTP/1.1\r\nHost: 127.0.0.1:{PORT}\r\n\r\n".to_vec(),
                // 2. wrong token
                b"GET /?token=deadbeef HTTP/1.1\r\nHost: 127.0.0.1:{PORT}\r\n\r\n".to_vec(),
                // 3. valid token but a rebinding Host header
                format!("GET /api/state?token={t} HTTP/1.1\r\nHost: evil.example:{{PORT}}\r\n\r\n").into_bytes(),
                // 4. POST with token header but a foreign Origin
                format!(
                    "POST /api/mcp HTTP/1.1\r\nHost: 127.0.0.1:{{PORT}}\r\nOrigin: https://evil.example\r\nX-CoralCraft-Token: {t}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                    body.len()
                )
                .into_bytes(),
                // 5. POST carrying the token only in the query string (no custom header): rejected
                format!(
                    "POST /api/mcp?token={t} HTTP/1.1\r\nHost: 127.0.0.1:{{PORT}}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                    body.len()
                )
                .into_bytes(),
                // 6. oversized declared body
                format!("POST /api/open HTTP/1.1\r\nHost: 127.0.0.1:{{PORT}}\r\nX-CoralCraft-Token: {t}\r\nContent-Length: 999999999999\r\n\r\n").into_bytes(),
            ],
            t,
        );
        for (i, r) in resps.iter().take(5).enumerate() {
            assert!(r.starts_with("HTTP/1.1 403"), "request {} should be forbidden, got: {}", i + 1, r.lines().next().unwrap_or(""));
        }
        assert!(resps[5].starts_with("HTTP/1.1 413"), "oversized body should be 413, got: {}", resps[5].lines().next().unwrap_or(""));
    }

    #[test]
    fn interactive_gui_http_endpoints_serve_frame_and_handle_events() {
        let t = "0123456789abcdef0123456789abcdef";
        let body = r#"{"type":"key_down","key":"F6","ctrl":false,"shift":false}"#;
        let resps = serve_and_collect(
            vec![
                format!("GET /?token={t} HTTP/1.1\r\nHost: 127.0.0.1:{{PORT}}\r\n\r\n").into_bytes(),
                format!("GET /api/frame.png?token={t} HTTP/1.1\r\nHost: localhost:{{PORT}}\r\n\r\n").into_bytes(),
                format!(
                    "POST /api/event HTTP/1.1\r\nHost: 127.0.0.1:{{PORT}}\r\nOrigin: http://127.0.0.1:{{PORT}}\r\nX-CoralCraft-Token: {t}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                    body.len()
                )
                .into_bytes(),
            ],
            t,
        );
        assert!(resps[0].starts_with("HTTP/1.1 200 OK"));
        assert!(resps[0].contains(t), "studio page must embed the session token for its own API calls");
        assert!(resps[1].starts_with("HTTP/1.1 200 OK"));
        assert!(resps[1].contains("PNG"));
        assert!(resps[2].starts_with("HTTP/1.1 200 OK"));
        assert!(resps[2].contains("Rectangle"));
    }
}
