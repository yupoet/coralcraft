//! Experimental localhost preview server for the CoralCraft workspace (`gui_server`).
//!
//! This is a stop-gap until the native egui shell lands (docs/gaps.md R2.1). Because any web page
//! the user visits can send requests to `127.0.0.1`, every request is checked by [`RequestGuard`]:
//! - a per-launch 128-bit token (query parameter for GET, `X-CoralCraft-Token` header for POST;
//!   a custom header cannot be sent cross-origin without a CORS preflight, which is never granted);
//! - the `Host` header must name the loopback address and port (blocks DNS rebinding);
//! - an `Origin` header, when present, must be the server's own origin;
//! - header and body sizes are capped and sockets have read/write timeouts.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::Command;
use std::time::Duration;

use coralcraft_cdr::{parse_document, write_cdr7};
use coralcraft_doc::{Document, import_dxf, import_svg, load_bytes, save_zip};
use coralcraft_mcp::McpServer;
use coralcraft_render::{export_pdf, export_svg};
use coralcraft_ui_egui::{InteractiveGuiSession, MouseButton};
use serde_json::{Value, json};

const STUDIO_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>CoralCraft — Vector Illustration &amp; Page Layout</title>
<style>
  * { box-sizing: border-box; user-select: none; -webkit-user-select: none; }
  html, body {
    margin: 0; padding: 0; width: 100%; height: 100%;
    background: #2b2b2b; color: #f0f0f0;
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
    display: flex; flex-direction: column; overflow: hidden;
  }
  #native-bar {
    height: 36px; background: #1e1e1e; border-bottom: 1px solid #3c3c3c;
    display: flex; align-items: center; padding: 0 10px; gap: 8px; font-size: 12px;
  }
  .brand { font-weight: 700; color: #ff6b4a; margin-right: 6px; letter-spacing: 0.4px; }
  .btn {
    background: #333; color: #eee; border: 1px solid #555; border-radius: 4px;
    padding: 4px 10px; font-size: 12px; cursor: pointer; text-decoration: none;
  }
  .btn:hover { background: #444; border-color: #777; }
  .btn-primary { background: #0066cc; border-color: #0077ee; }
  .btn-primary:hover { background: #0077ee; }
  #mcp-input {
    flex: 1; background: #141414; color: #ddd; border: 1px solid #444;
    border-radius: 4px; padding: 4px 8px; font-family: monospace; font-size: 12px;
  }
  #viewport-wrap {
    flex: 1; display: flex; align-items: center; justify-content: center;
    background: #505050; overflow: auto; position: relative;
  }
  #workspace-canvas {
    box-shadow: 0 8px 28px rgba(0,0,0,0.45);
    cursor: crosshair; background: #d4d0c8;
  }
  #file-input { display: none; }
  #toast {
    position: fixed; bottom: 32px; right: 16px; background: rgba(20,20,20,0.92);
    color: #fff; padding: 6px 12px; border-radius: 4px; font-size: 12px;
    border: 1px solid #555; display: none; pointer-events: none;
  }
</style>
</head>
<body>
  <div id="native-bar">
    <span class="brand">CORALCRAFT</span>
    <button class="btn" onclick="document.getElementById('file-input').click()">Open (.cdr / .coralcraft / .svg / .dxf)</button>
    <input type="file" id="file-input" accept=".cdr,.cmx,.coralcraft,.svg,.dxf" onchange="uploadFile(this.files[0])">
    <a class="btn" href="/api/export/coralcraft?token=__CC_TOKEN__" download="document.coralcraft">Save .coralcraft</a>
    <a class="btn" href="/api/export/cdr?token=__CC_TOKEN__" download="document.cdr">Export .CDR</a>
    <a class="btn" href="/api/export/svg?token=__CC_TOKEN__" download="document.svg">Export .SVG</a>
    <a class="btn" href="/api/export/pdf?token=__CC_TOKEN__" download="document.pdf">Export .PDF</a>
    <a class="btn" href="/api/frame.png?token=__CC_TOKEN__" download="workspace.png">Export .PNG</a>
    <input id="mcp-input" type="text" placeholder='Run any of 240 MCP tools, e.g. {"name":"add_koch_snowflake","arguments":{"cx":300,"cy":260,"radius_pt":70,"iterations":3}}' onkeydown="if(event.key==='Enter')runMcp()">
    <button class="btn btn-primary" onclick="runMcp()">Run Tool</button>
  </div>
  <div id="viewport-wrap">
    <img id="workspace-canvas" src="/api/frame.png?token=__CC_TOKEN__" width="1440" height="900" draggable="false" alt="CoralCraft Workspace">
  </div>
  <div id="toast"></div>
<script>
const TOKEN = '__CC_TOKEN__';
const img = document.getElementById('workspace-canvas');
const toast = document.getElementById('toast');
let busy = false;
let queuedMove = null;

function showToast(msg) {
  toast.textContent = msg;
  toast.style.display = 'block';
  clearTimeout(window._toastTimer);
  window._toastTimer = setTimeout(() => { toast.style.display = 'none'; }, 2400);
}

function refreshFrame() {
  img.src = '/api/frame.png?token=' + TOKEN + '&t=' + Date.now();
}

async function sendEvent(payload) {
  try {
    const res = await fetch('/api/event', {
      method: 'POST',
      headers: {'Content-Type': 'application/json', 'X-CoralCraft-Token': TOKEN},
      body: JSON.stringify(payload)
    });
    if (res.ok) {
      refreshFrame();
    }
  } catch (e) {}
}

function coords(e) {
  const r = img.getBoundingClientRect();
  const scaleX = 1440 / r.width;
  const scaleY = 900 / r.height;
  return {
    x: Math.round((e.clientX - r.left) * scaleX),
    y: Math.round((e.clientY - r.top) * scaleY)
  };
}

img.addEventListener('contextmenu', e => e.preventDefault());

img.addEventListener('mousedown', e => {
  e.preventDefault();
  const c = coords(e);
  const btn = e.button === 2 ? 'right' : (e.button === 1 ? 'middle' : 'left');
  sendEvent({ type: 'pointer_down', x: c.x, y: c.y, button: btn, shift: e.shiftKey, alt: e.altKey });
});

img.addEventListener('mousemove', e => {
  if (e.buttons === 0) return;
  const c = coords(e);
  queuedMove = { type: 'pointer_move', x: c.x, y: c.y, shift: e.shiftKey, alt: e.altKey };
  if (!busy) {
    busy = true;
    const m = queuedMove;
    queuedMove = null;
    sendEvent(m).finally(() => { busy = false; });
  }
});

window.addEventListener('mouseup', e => {
  const c = coords(e);
  const btn = e.button === 2 ? 'right' : 'left';
  sendEvent({ type: 'pointer_up', x: c.x, y: c.y, button: btn, shift: e.shiftKey, alt: e.altKey });
});

window.addEventListener('keydown', e => {
  if (document.activeElement && document.activeElement.id === 'mcp-input') return;
  if (['F5','F6','F7','F8','F10','Delete','Backspace'].includes(e.key) || e.ctrlKey || e.metaKey) {
    e.preventDefault();
  }
  sendEvent({ type: 'key_down', key: e.key, ctrl: e.ctrlKey || e.metaKey, shift: e.shiftKey });
});

async function runMcp() {
  const raw = document.getElementById('mcp-input').value.trim();
  if (!raw) return;
  let parsed;
  try {
    parsed = JSON.parse(raw);
  } catch (err) {
    showToast('Invalid JSON: ' + err.message);
    return;
  }
  const req = {
    jsonrpc: '2.0',
    id: 1,
    method: 'tools/call',
    params: parsed
  };
  const res = await fetch('/api/mcp', {
    method: 'POST',
    headers: {'Content-Type': 'application/json', 'X-CoralCraft-Token': TOKEN},
    body: JSON.stringify(req)
  });
  const out = await res.json();
  refreshFrame();
  showToast('MCP: ' + JSON.stringify(out.result ? out.result.structuredContent : out.error));
}

async function uploadFile(file) {
  if (!file) return;
  const buf = await file.arrayBuffer();
  const res = await fetch('/api/open?name=' + encodeURIComponent(file.name), {
    method: 'POST',
    headers: {'X-CoralCraft-Token': TOKEN},
    body: new Uint8Array(buf)
  });
  const out = await res.json();
  refreshFrame();
  showToast(out.message || 'Loaded ' + file.name);
}
</script>
</body>
</html>
"#;

/// Starts the interactive GUI server on `127.0.0.1:port` (pass `0` for an OS-assigned port),
/// optionally opens a native desktop window, and processes requests.
pub fn run_gui_server(initial_doc: Document, port: u16, open_window: bool, max_requests: Option<usize>) -> Result<u16, String> {
    let listener = TcpListener::bind(("127.0.0.1", port)).map_err(|e| format!("failed to bind GUI server: {e}"))?;
    let local_port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let guard = RequestGuard::new(generate_session_token(), local_port);
    let url = format!("http://127.0.0.1:{local_port}/?token={}", guard.token);

    let mut session = InteractiveGuiSession::new(initial_doc);
    if session.workspace.engine.document.pages.iter().all(|p| p.layers.iter().all(|l| l.objects.is_empty())) {
        session.populate_starter_scene();
    }

    println!("CoralCraft GUI Studio listening at {url}");
    if open_window {
        launch_native_app_window(&url);
    }

    let mut handled = 0usize;
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else {
            continue;
        };
        let _ = stream.set_read_timeout(Some(READ_TIMEOUT));
        let _ = stream.set_write_timeout(Some(WRITE_TIMEOUT));
        let _ = handle_http_connection(&mut stream, &mut session, &guard);
        handled += 1;
        if let Some(limit) = max_requests
            && handled >= limit
        {
            break;
        }
    }
    Ok(local_port)
}

/// Maximum size of the request line plus headers.
const MAX_HEADER_BYTES: usize = 64 * 1024;
/// Maximum accepted request body (documents opened through `/api/open`).
const MAX_BODY_BYTES: usize = 64 * 1024 * 1024;
const READ_TIMEOUT: Duration = Duration::from_secs(5);
const WRITE_TIMEOUT: Duration = Duration::from_secs(30);

/// Per-launch request authorization for the preview server.
#[derive(Debug, Clone)]
pub struct RequestGuard {
    token: String,
    port: u16,
}

impl RequestGuard {
    /// Creates a guard that accepts `token` on the loopback server bound to `port`.
    #[must_use]
    pub fn new(token: String, port: u16) -> Self {
        Self { token, port }
    }

    fn host_allowed(&self, host: Option<&str>) -> bool {
        let Some(host) = host else { return false };
        let host = host.trim();
        [format!("127.0.0.1:{}", self.port), format!("localhost:{}", self.port)].iter().any(|h| h.eq_ignore_ascii_case(host))
    }

    fn origin_allowed(&self, origin: Option<&str>) -> bool {
        match origin {
            None => true,
            Some(o) => {
                let o = o.trim();
                [format!("http://127.0.0.1:{}", self.port), format!("http://localhost:{}", self.port)].iter().any(|a| a.eq_ignore_ascii_case(o))
            }
        }
    }

    fn token_matches(&self, candidate: Option<&str>) -> bool {
        candidate.is_some_and(|c| constant_time_eq(c.as_bytes(), self.token.as_bytes()))
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Returns a 128-bit hex token seeded from the standard library's per-process random SipHash keys,
/// the process id and the clock (no external RNG dependency).
fn generate_session_token() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let mut words = [0u64; 2];
    for (i, w) in words.iter_mut().enumerate() {
        let mut h = RandomState::new().build_hasher();
        h.write_u128(nanos);
        h.write_u32(std::process::id());
        h.write_usize(i);
        *w = h.finish();
    }
    format!("{:016x}{:016x}", words[0], words[1])
}

fn header_value<'a>(headers: &'a str, name: &str) -> Option<&'a str> {
    headers.lines().skip(1).find_map(|line| {
        let (k, v) = line.split_once(':')?;
        k.trim().eq_ignore_ascii_case(name).then_some(v.trim())
    })
}

fn query_param<'a>(raw_path: &'a str, name: &str) -> Option<&'a str> {
    let query = raw_path.split_once('?')?.1;
    query.split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        (k == name).then_some(v)
    })
}

/// Handles a single HTTP request against [`InteractiveGuiSession`] after [`RequestGuard`] checks.
pub fn handle_http_connection(stream: &mut TcpStream, session: &mut InteractiveGuiSession, guard: &RequestGuard) -> Result<(), String> {
    // Read until the end of the headers (possibly across several reads), bounded by MAX_HEADER_BYTES.
    let mut buf: Vec<u8> = Vec::with_capacity(8192);
    let header_end = loop {
        if let Some(end) = find_header_end(&buf) {
            break end;
        }
        if buf.len() >= MAX_HEADER_BYTES {
            return write_response(stream, "431 Request Header Fields Too Large", "text/plain", b"headers too large");
        }
        let mut chunk = [0u8; 8192];
        let r = stream.read(&mut chunk).map_err(|e| e.to_string())?;
        if r == 0 {
            if buf.is_empty() {
                return Ok(());
            }
            return write_response(stream, "400 Bad Request", "text/plain", b"incomplete request");
        }
        buf.extend_from_slice(chunk.get(..r).unwrap_or_default());
    };
    let header_str = String::from_utf8_lossy(buf.get(..header_end).unwrap_or_default()).into_owned();
    let first_line = header_str.lines().next().unwrap_or("");
    let mut parts = first_line.split_whitespace();
    let method = parts.next().unwrap_or("GET");
    let raw_path = parts.next().unwrap_or("/");
    let path = raw_path.split('?').next().unwrap_or("/");

    if !guard.host_allowed(header_value(&header_str, "host")) || !guard.origin_allowed(header_value(&header_str, "origin")) {
        return write_response(stream, "403 Forbidden", "text/plain", b"forbidden");
    }
    let authorized = match method {
        "GET" => guard.token_matches(query_param(raw_path, "token")),
        // State-changing requests must carry the custom header (never sent cross-origin without a
        // CORS preflight, which this server never grants).
        _ => guard.token_matches(header_value(&header_str, "x-coralcraft-token")),
    };
    if !authorized {
        return write_response(stream, "403 Forbidden", "text/plain", b"forbidden");
    }

    let Some(declared_len) = parse_content_length(&header_str) else {
        return write_response(stream, "400 Bad Request", "text/plain", b"malformed content-length");
    };
    let content_len = match usize::try_from(declared_len) {
        Ok(len) if len <= MAX_BODY_BYTES => len,
        _ => return write_response(stream, "413 Payload Too Large", "text/plain", b"payload too large"),
    };
    let mut body = buf.get(header_end..).unwrap_or_default().to_vec();
    body.truncate(content_len);
    while body.len() < content_len {
        let mut chunk = [0u8; 8192];
        let r = stream.read(&mut chunk).map_err(|e| e.to_string())?;
        if r == 0 {
            break;
        }
        let take = r.min(content_len - body.len());
        body.extend_from_slice(chunk.get(..take).unwrap_or_default());
    }

    match (method, path) {
        ("GET", "/") => {
            let html = STUDIO_HTML.replace("__CC_TOKEN__", &guard.token);
            write_response(stream, "200 OK", "text/html; charset=utf-8", html.as_bytes())
        }
        ("GET", "/api/frame.png") => {
            let rendered = session.render_frame().map_err(|e| e.to_string())?;
            write_response(stream, "200 OK", "image/png", &rendered.png_bytes)
        }
        ("GET", "/api/state") => {
            let snap = session.workspace.snapshot();
            let json_bytes = serde_json::to_vec(&snap).map_err(|e| e.to_string())?;
            write_response(stream, "200 OK", "application/json", &json_bytes)
        }
        ("POST", "/api/event") => {
            if let Ok(val) = serde_json::from_slice::<Value>(&body) {
                dispatch_json_event(session, &val);
            }
            let snap = session.workspace.snapshot();
            let resp = json!({
                "ok": true,
                "active_tool": format!("{:?}", snap.active_tool),
                "selection_summary": snap.status_bar.selection_summary,
                "pages": snap.page_tabs.len().saturating_sub(2)
            });
            write_response(stream, "200 OK", "application/json", resp.to_string().as_bytes())
        }
        ("POST", "/api/mcp") => {
            let req_str = String::from_utf8_lossy(&body);
            let mut mcp = McpServer::with_engine(session.workspace.engine.clone());
            let resp_str = mcp.handle_json_str(&req_str);
            session.workspace.engine = mcp.engine;
            session.dirty = true;
            write_response(stream, "200 OK", "application/json", resp_str.as_bytes())
        }
        ("POST", "/api/open") => {
            let name = query_param(raw_path, "name").unwrap_or("document");
            let msg = match load_any_document(name, &body) {
                Ok(doc) => {
                    session.workspace.engine = coralcraft_engine::EditorEngine::new(doc);
                    session.dirty = true;
                    format!("Opened {name}")
                }
                Err(e) => format!("Failed to open {name}: {e}"),
            };
            let resp = json!({ "ok": true, "message": msg });
            write_response(stream, "200 OK", "application/json", resp.to_string().as_bytes())
        }
        ("GET", "/api/export/svg") => {
            let svg = export_svg(&session.workspace.engine.document, session.workspace.engine.current_page_index()).map_err(|e| e.to_string())?;
            write_response(stream, "200 OK", "image/svg+xml", svg.as_bytes())
        }
        ("GET", "/api/export/pdf") => {
            let pdf = export_pdf(&session.workspace.engine.document).map_err(|e| e.to_string())?;
            write_response(stream, "200 OK", "application/pdf", &pdf)
        }
        ("GET", "/api/export/cdr") => {
            let cdr = write_cdr7(&session.workspace.engine.document).map_err(|e| e.to_string())?;
            write_response(stream, "200 OK", "application/octet-stream", &cdr)
        }
        ("GET", "/api/export/coralcraft") => {
            let zip = save_zip(&session.workspace.engine.document).map_err(|e| e.to_string())?;
            write_response(stream, "200 OK", "application/octet-stream", &zip)
        }
        _ => write_response(stream, "404 Not Found", "text/plain", b"not found"),
    }
}

fn dispatch_json_event(session: &mut InteractiveGuiSession, val: &Value) {
    let kind = val.get("type").and_then(Value::as_str).unwrap_or("");
    let x = val.get("x").and_then(Value::as_i64).unwrap_or(0) as i32;
    let y = val.get("y").and_then(Value::as_i64).unwrap_or(0) as i32;
    let shift = val.get("shift").and_then(Value::as_bool).unwrap_or(false);
    let alt = val.get("alt").and_then(Value::as_bool).unwrap_or(false);
    let button = match val.get("button").and_then(Value::as_str) {
        Some("right") => MouseButton::Right,
        Some("middle") => MouseButton::Middle,
        _ => MouseButton::Left,
    };
    match kind {
        "pointer_down" => session.pointer_down(x, y, button, shift, alt),
        "pointer_move" => session.pointer_move(x, y, shift, alt),
        "pointer_up" => session.pointer_up(x, y, button, shift, alt),
        "key_down" => {
            let key = val.get("key").and_then(Value::as_str).unwrap_or("");
            let ctrl = val.get("ctrl").and_then(Value::as_bool).unwrap_or(false);
            session.key_down(key, ctrl, shift);
        }
        _ => {}
    }
}

fn load_any_document(name: &str, bytes: &[u8]) -> Result<Document, String> {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".svg")
        && let Ok(text) = std::str::from_utf8(bytes)
    {
        return import_svg(text).map_err(|e| e.to_string());
    }
    if lower.ends_with(".dxf")
        && let Ok(text) = std::str::from_utf8(bytes)
    {
        return import_dxf(text).map_err(|e| e.to_string());
    }
    if let Ok(doc) = load_bytes(bytes) {
        return Ok(doc);
    }
    let outcome = parse_document(bytes).map_err(|e| e.to_string())?;
    Ok(outcome.document)
}

fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n").map(|idx| idx + 4)
}

/// Parses `Content-Length` as `u64` so values that do not fit a 32-bit `usize` are still seen as
/// too large (rather than silently treated as 0). Absent header = 0; malformed = `None`.
fn parse_content_length(headers: &str) -> Option<u64> {
    match header_value(headers, "content-length") {
        None => Some(0),
        Some(v) => v.parse::<u64>().ok(),
    }
}

fn write_response(stream: &mut TcpStream, status: &str, content_type: &str, body: &[u8]) -> Result<(), String> {
    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes()).map_err(|e| e.to_string())?;
    stream.write_all(body).map_err(|e| e.to_string())?;
    stream.flush().map_err(|e| e.to_string())
}

fn launch_native_app_window(url: &str) {
    let app_arg = format!("--app={url}");
    if cfg!(target_os = "macos") {
        let chrome = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
        if std::path::Path::new(chrome).exists() {
            let _ = Command::new(chrome).args([&app_arg, "--window-size=1440,936"]).spawn();
            return;
        }
        let _ = Command::new("open").arg(url).spawn();
    } else if cfg!(target_os = "windows") {
        if Command::new("msedge").args([&app_arg, "--window-size=1440,936"]).spawn().is_ok() {
            return;
        }
        let _ = Command::new("cmd").args(["/C", "start", "", url]).spawn();
    } else {
        for bin in ["google-chrome", "chromium", "chromium-browser", "microsoft-edge", "brave-browser"] {
            if Command::new(bin).args([&app_arg, "--window-size=1440,936"]).spawn().is_ok() {
                return;
            }
        }
        let _ = Command::new("xdg-open").arg(url).spawn();
    }
}
