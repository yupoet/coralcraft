//! Keeps `docs/mcp.md` in sync with the live `tools/list` registry.
//!
//! The tool reference section of `docs/mcp.md` (between the `BEGIN GENERATED TOOL LIST` and
//! `END GENERATED TOOL LIST` markers) is generated from the server's own `tools/list` response.
//! Regenerate it with `CORALCRAFT_BLESS=1 cargo test -p coralcraft-mcp --test docs_sync`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use coralcraft_mcp::McpServer;
use serde_json::Value;

const BEGIN: &str = "<!-- BEGIN GENERATED TOOL LIST (cargo test -p coralcraft-mcp --test docs_sync) -->";
const END: &str = "<!-- END GENERATED TOOL LIST -->";

fn live_tools() -> Vec<(String, String)> {
    let mut server = McpServer::new();
    let resp = server.handle_json_str(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#);
    let v: Value = serde_json::from_str(&resp).unwrap();
    let mut tools: Vec<(String, String)> = v["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| {
            let name = t["name"].as_str().unwrap().to_owned();
            let desc = t["description"].as_str().unwrap_or_default().replace('|', "\\|").replace('\n', " ");
            (name, desc)
        })
        .collect();
    tools.sort();
    tools
}

fn render_block(tools: &[(String, String)]) -> String {
    let mut out = String::new();
    out.push_str(BEGIN);
    out.push('\n');
    out.push_str(&format!("\n{} tools, sorted by name.\n\n| Tool | Description |\n|---|---|\n", tools.len()));
    for (name, desc) in tools {
        out.push_str(&format!("| `{name}` | {desc} |\n"));
    }
    out.push('\n');
    out.push_str(END);
    out
}

#[test]
fn docs_mcp_md_tool_list_matches_live_registry() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/mcp.md");
    // Normalize line endings: Windows checkouts may convert the file to CRLF.
    let doc = std::fs::read_to_string(&path).expect("docs/mcp.md must exist").replace("\r\n", "\n");
    let start = doc.find(BEGIN).expect("docs/mcp.md is missing the BEGIN GENERATED TOOL LIST marker");
    let end = doc.find(END).expect("docs/mcp.md is missing the END GENERATED TOOL LIST marker") + END.len();
    let expected = render_block(&live_tools());
    let current = &doc[start..end];
    if current != expected {
        if std::env::var("CORALCRAFT_BLESS").is_ok_and(|v| v == "1") {
            let updated = format!("{}{}{}", &doc[..start], expected, &doc[end..]);
            std::fs::write(&path, updated).unwrap();
        } else {
            panic!("docs/mcp.md tool list is stale; run `CORALCRAFT_BLESS=1 cargo test -p coralcraft-mcp --test docs_sync`");
        }
    }
}
