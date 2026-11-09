//! Keeps `docs/mcp.md` in sync with the live MCP registry and protocol surface.
//!
//! Regenerate with `CORALCRAFT_BLESS=1 cargo test -p vectorcraft-mcp --test docs_sync`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing, clippy::arithmetic_side_effects)]

use std::fmt::Write as _;
use std::path::PathBuf;

use serde_json::Value;
use vectorcraft_mcp::{Headless, Server};

const BEGIN: &str = "<!-- BEGIN GENERATED TOOL LIST (cargo test -p vectorcraft-mcp --test docs_sync) -->";
const LEGACY_BEGIN: &str = "<!-- BEGIN GENERATED TOOL LIST (cargo test -p coralcraft-mcp --test docs_sync) -->";
const END: &str = "<!-- END GENERATED TOOL LIST -->";

fn live_tools() -> Vec<(String, String)> {
    let mut server = Server::new(Box::new(Headless::with_document()));
    let response = server
        .handle_line(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#)
        .expect("tools/list returns a response");
    let value: Value = serde_json::from_str(&response).unwrap();
    let mut tools: Vec<(String, String)> = value["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| {
            let name = tool["name"].as_str().unwrap().to_owned();
            let description = tool["description"]
                .as_str()
                .unwrap_or_default()
                .replace('|', "\\|")
                .replace('\n', " ");
            (name, description)
        })
        .collect();
    tools.sort();
    tools
}

fn render_block(tools: &[(String, String)]) -> String {
    let mut out = String::new();
    out.push_str(BEGIN);
    out.push('\n');
    let _ = write!(out, "\n{} tools, sorted by name.\n\n| Tool | Description |\n|---|---|\n", tools.len());
    for (name, description) in tools {
        let _ = writeln!(out, "| `{name}` | {description} |");
    }
    out.push('\n');
    out.push_str(END);
    out
}

fn document() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/mcp.md");
    std::fs::read_to_string(path).expect("docs/mcp.md must exist").replace("\r\n", "\n")
}

#[test]
fn docs_mcp_md_tool_list_matches_live_registry() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/mcp.md");
    let doc = document();
    let start = doc
        .find(BEGIN)
        .or_else(|| doc.find(LEGACY_BEGIN))
        .expect("docs/mcp.md is missing the BEGIN marker");
    let end = doc.find(END).expect("docs/mcp.md is missing the END marker") + END.len();
    let expected = render_block(&live_tools());
    let current = &doc[start..end];
    if current != expected {
        if std::env::var("CORALCRAFT_BLESS").is_ok_and(|value| value == "1") {
            let updated = format!("{}{}{}", &doc[..start], expected, &doc[end..]);
            std::fs::write(&path, updated).unwrap();
        } else {
            panic!("docs/mcp.md is stale; run `CORALCRAFT_BLESS=1 cargo test -p vectorcraft-mcp --test docs_sync`");
        }
    }
}

#[test]
fn docs_mcp_md_protocol_surface_is_current() {
    let doc = document();
    for required in [
        "2026-07-28",
        "2025-06-18",
        "resources/list",
        "resources/templates/list",
        "resources/read",
        "prompts/list",
        "prompts/get",
        "completion/complete",
        "logging/setLevel",
    ] {
        assert!(doc.contains(required), "docs/mcp.md does not document {required}");
    }
    assert!(!doc.contains("246 tools, sorted by name"), "legacy tool count remains");
}
