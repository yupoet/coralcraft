//! Robustness tests: hostile tool arguments must yield error results, never crash the server.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing, clippy::arithmetic_side_effects)]

use coralcraft_mcp::McpServer;
use serde_json::{Value, json};

fn call(server: &mut McpServer, name: &str, arguments: Value) -> Value {
    let req = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": name, "arguments": arguments}});
    serde_json::from_str(&server.handle_json_str(&req.to_string())).unwrap()
}

#[test]
fn importing_a_document_without_pages_is_a_tool_error_and_the_server_keeps_working() {
    let mut server = McpServer::new();
    let exported = call(&mut server, "export_coralcraft_json", json!({}));
    let text = exported["result"]["content"][0]["text"].as_str().unwrap();
    let mut doc: Value = serde_json::from_str(serde_json::from_str::<Value>(text).unwrap()["coralcraft_json"].as_str().unwrap()).unwrap();
    doc["pages"] = json!([]);

    let resp = call(&mut server, "import_coralcraft_json", json!({"coralcraft_json": doc.to_string()}));
    assert_eq!(resp["result"]["isError"], json!(true), "empty-page import must be rejected: {resp}");

    // The server must still be usable afterwards.
    let rect = call(&mut server, "add_rectangle", json!({}));
    assert_eq!(rect["result"]["isError"], json!(false), "server unusable after rejected import: {rect}");
    let summary = call(&mut server, "document_summary", json!({}));
    assert_eq!(summary["result"]["isError"], json!(false));
}

#[test]
fn svg_import_with_multibyte_colors_does_not_crash_the_server() {
    let mut server = McpServer::new();
    let resp = call(&mut server, "import_svg", json!({"svg": "<svg><rect x=\"1\" y=\"1\" width=\"5\" height=\"5\" fill=\"#aé€\"/></svg>"}));
    assert!(resp.get("result").is_some(), "expected a tool result: {resp}");
}
