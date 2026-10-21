//! Model Context Protocol (MCP) JSON-RPC 2.0 server (`coralcraft-mcp`, L6)
//! for agent-driven multi-page CorelDRAW illustration and vector export.
#![recursion_limit = "2048"]

mod handlers;
mod schema;

use coralcraft_engine::EditorEngine;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Maximum length in bytes of a single newline-delimited JSON-RPC message on stdio (64 MiB).
const MAX_STDIO_LINE_BYTES: usize = 64 * 1024 * 1024;

/// Supported MCP protocol versions, ordered with default fallback first.
const SUPPORTED_PROTOCOL_VERSIONS: [&str; 2] = ["2024-11-05", "2025-03-26"];

/// Incoming JSON-RPC 2.0 request envelope.
#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    #[serde(default)]
    pub id: Value,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

/// Outgoing JSON-RPC 2.0 response envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// JSON-RPC 2.0 error object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
}

/// Stateful MCP server wrapping [`EditorEngine`] for agent-driven vector authoring.
#[derive(Debug, Clone, Default)]
pub struct McpServer {
    pub engine: EditorEngine,
    last_command_failed: bool,
}

impl McpServer {
    /// Creates a new MCP server initialized with a blank A4 Portrait document.
    #[must_use]
    pub fn new() -> Self {
        Self { engine: EditorEngine::default(), last_command_failed: false }
    }

    /// Creates a new MCP server wrapping an existing [`EditorEngine`].
    #[must_use]
    pub fn with_engine(engine: EditorEngine) -> Self {
        Self { engine, last_command_failed: false }
    }

    fn serialize_response(resp: &JsonRpcResponse) -> String {
        serde_json::to_string(resp)
            .unwrap_or_else(|_| r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32603,"message":"Internal serialization error"}}"#.to_owned())
    }

    fn error_response(id: Value, code: i32, message: impl Into<String>) -> JsonRpcResponse {
        JsonRpcResponse { jsonrpc: "2.0".to_owned(), id, result: None, error: Some(JsonRpcError { code, message: message.into() }) }
    }

    /// Serves newline-delimited JSON-RPC 2.0 messages from `reader` until EOF, writing one response
    /// line per request to `writer` (the MCP stdio transport). Blank lines and JSON-RPC notifications
    /// produce no output. Lines with invalid UTF-8 or exceeding [`MAX_STDIO_LINE_BYTES`] produce a
    /// `-32700` Parse error response without terminating the session.
    ///
    /// # Errors
    /// Returns an I/O error when reading from `reader` or writing to `writer` fails.
    pub fn serve_stdio<R: std::io::BufRead, W: std::io::Write>(&mut self, mut reader: R, mut writer: W) -> std::io::Result<()> {
        let mut line_buf = Vec::new();
        let mut oversized = false;
        loop {
            let (done, newline_pos, chunk_len) = {
                let available = reader.fill_buf()?;
                if available.is_empty() {
                    (true, None, 0)
                } else {
                    let pos = available.iter().position(|&b| b == b'\n');
                    (false, pos, available.len())
                }
            };
            if done {
                if !line_buf.is_empty() || oversized {
                    self.process_raw_stdio_line(&line_buf, oversized, &mut writer)?;
                }
                break;
            }
            if let Some(pos) = newline_pos {
                let take = pos.saturating_add(1);
                if !oversized {
                    let slice = reader.fill_buf()?.get(..pos).unwrap_or_default();
                    if line_buf.len().saturating_add(slice.len()) <= MAX_STDIO_LINE_BYTES {
                        line_buf.extend_from_slice(slice);
                    } else {
                        oversized = true;
                    }
                }
                reader.consume(take);
                self.process_raw_stdio_line(&line_buf, oversized, &mut writer)?;
                line_buf.clear();
                oversized = false;
            } else {
                if !oversized {
                    let available = reader.fill_buf()?;
                    if line_buf.len().saturating_add(available.len()) <= MAX_STDIO_LINE_BYTES {
                        line_buf.extend_from_slice(available);
                    } else {
                        oversized = true;
                        line_buf.clear();
                    }
                }
                reader.consume(chunk_len);
            }
        }
        Ok(())
    }

    fn process_raw_stdio_line<W: std::io::Write>(&mut self, raw: &[u8], oversized: bool, writer: &mut W) -> std::io::Result<()> {
        if oversized {
            let resp = Self::serialize_response(&Self::error_response(
                Value::Null,
                -32700,
                format!("Parse error: request line exceeds maximum length of {MAX_STDIO_LINE_BYTES} bytes"),
            ));
            writeln!(writer, "{resp}")?;
            writer.flush()?;
            return Ok(());
        }
        match std::str::from_utf8(raw) {
            Ok(line) => {
                if line.trim().is_empty() {
                    return Ok(());
                }
                if let Some(resp) = self.handle_message(line) {
                    writeln!(writer, "{resp}")?;
                    writer.flush()?;
                }
            }
            Err(err) => {
                let resp = Self::serialize_response(&Self::error_response(Value::Null, -32700, format!("Parse error: invalid UTF-8 ({err})")));
                writeln!(writer, "{resp}")?;
                writer.flush()?;
            }
        }
        Ok(())
    }

    /// Processes a single JSON-RPC 2.0 message line. Returns `None` for valid JSON-RPC 2.0
    /// notifications (requests with no `id` member).
    #[must_use]
    pub fn handle_message(&mut self, line: &str) -> Option<String> {
        let raw_val: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(err) => {
                return Some(Self::serialize_response(&Self::error_response(Value::Null, -32700, format!("Parse error: {err}"))));
            }
        };
        let Some(obj) = raw_val.as_object() else {
            return Some(Self::serialize_response(&Self::error_response(
                Value::Null,
                -32600,
                "Invalid Request: JSON-RPC message must be a JSON object",
            )));
        };
        let has_id = obj.contains_key("id");
        let id = obj.get("id").cloned().unwrap_or(Value::Null);
        let jsonrpc = obj.get("jsonrpc").and_then(Value::as_str).unwrap_or_default();
        let method = obj.get("method").and_then(Value::as_str).unwrap_or_default();
        if jsonrpc != "2.0" || method.is_empty() {
            return Some(Self::serialize_response(&Self::error_response(
                id,
                -32600,
                "Invalid Request: 'jsonrpc' must be '2.0' and 'method' must be a non-empty string",
            )));
        }
        if !has_id {
            // Valid JSON-RPC 2.0 notification (e.g. `notifications/initialized`, `notifications/cancelled`):
            // must not emit any response over the transport.
            return None;
        }
        let req = JsonRpcRequest {
            jsonrpc: jsonrpc.to_owned(),
            id: id.clone(),
            method: method.to_owned(),
            params: obj.get("params").cloned().unwrap_or(Value::Null),
        };
        // Defense in depth: shipped code must not panic, but if an undiscovered bug does,
        // report an internal error instead of taking the whole stdio server down.
        let resp = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.handle_request(req))) {
            Ok(resp) => resp,
            Err(_) => Self::error_response(id, -32603, "Internal error: request handler panicked"),
        };
        Some(Self::serialize_response(&resp))
    }

    /// Processes a single JSON-RPC 2.0 request string and returns a serialized JSON-RPC 2.0 response.
    /// For notifications (which produce no response in `handle_message`), returns an empty string.
    #[must_use]
    pub fn handle_json_str(&mut self, line: &str) -> String {
        self.handle_message(line).unwrap_or_default()
    }

    /// Handles a parsed [`JsonRpcRequest`].
    pub fn handle_request(&mut self, req: JsonRpcRequest) -> JsonRpcResponse {
        if req.jsonrpc != "2.0" || req.method.is_empty() {
            return Self::error_response(req.id, -32600, "Invalid Request: 'jsonrpc' must be '2.0' and 'method' must be a non-empty string");
        }
        match req.method.as_str() {
            "ping" => JsonRpcResponse { jsonrpc: "2.0".to_owned(), id: req.id, result: Some(json!({})), error: None },
            "initialize" => {
                let requested = req.params.get("protocolVersion").and_then(Value::as_str).unwrap_or(SUPPORTED_PROTOCOL_VERSIONS[0]);
                let negotiated = if SUPPORTED_PROTOCOL_VERSIONS.contains(&requested) { requested } else { SUPPORTED_PROTOCOL_VERSIONS[0] };
                JsonRpcResponse {
                    jsonrpc: "2.0".to_owned(),
                    id: req.id,
                    result: Some(json!({
                        "protocolVersion": negotiated,
                        "serverInfo": {
                            "name": "coralcraft-mcp",
                            "version": env!("CARGO_PKG_VERSION"),
                        },
                        "capabilities": {
                            "tools": {}
                        }
                    })),
                    error: None,
                }
            }
            "tools/list" => JsonRpcResponse { jsonrpc: "2.0".to_owned(), id: req.id, result: Some(schema::tools_list_result()), error: None },
            "tools/call" => {
                let Some(name) = req.params.get("name").and_then(Value::as_str).filter(|s| !s.is_empty()) else {
                    return Self::error_response(req.id, -32602, "Invalid params: missing or non-string tool 'name'");
                };
                if !schema::is_known_tool(name) {
                    return Self::error_response(req.id, -32602, format!("Invalid params: unknown tool '{name}'"));
                }
                let args = match req.params.get("arguments") {
                    None | Some(Value::Null) => json!({}),
                    Some(v @ Value::Object(_)) => v.clone(),
                    Some(_) => {
                        return Self::error_response(req.id, -32602, "Invalid params: 'arguments' must be a JSON object");
                    }
                };
                self.last_command_failed = false;
                match self.call_tool(name, &args) {
                    Ok(content) => JsonRpcResponse {
                        jsonrpc: "2.0".to_owned(),
                        id: req.id,
                        result: Some(json!({
                            "content": [{ "type": "text", "text": content.to_string() }],
                            "structuredContent": content,
                            "isError": self.last_command_failed
                        })),
                        error: None,
                    },
                    Err(msg) => JsonRpcResponse {
                        jsonrpc: "2.0".to_owned(),
                        id: req.id,
                        result: Some(json!({
                            "content": [{ "type": "text", "text": msg }],
                            "isError": true
                        })),
                        error: None,
                    },
                }
            }
            other => Self::error_response(req.id, -32601, format!("Method not found: {other}")),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing, clippy::arithmetic_side_effects)]
mod tests {
    use super::*;

    #[test]
    fn mcp_server_handles_initialize_list_and_end_to_end_authoring_tools() {
        let mut server = McpServer::new();

        let init_resp: JsonRpcResponse = serde_json::from_str(&server.handle_json_str(r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#)).unwrap();
        assert!(init_resp.error.is_none());

        let list_resp: JsonRpcResponse = serde_json::from_str(&server.handle_json_str(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#)).unwrap();
        let tools = list_resp.result.unwrap()["tools"].as_array().unwrap().len();
        assert_eq!(tools, 246);

        // Add a chamfered rectangle and an ellipse, align, group, ungroup, convert to curves, and export to SVG 1.1, PDF 1.4, and CDR7
        let r1: JsonRpcResponse = serde_json::from_str(
            &server.handle_json_str(
                r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"add_rectangle","arguments":{"x":20.0,"y":30.0,"width":160.0,"height":90.0,"radius":10.0,"corner_kind":"chamfered"}}}"#,
            ),
        )
        .unwrap();
        let id1 = r1.result.unwrap()["structuredContent"]["selected_id"].as_u64().unwrap();

        let r2: JsonRpcResponse = serde_json::from_str(
            &server.handle_json_str(
                r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"add_ellipse","arguments":{"x":80.0,"y":90.0,"width":100.0,"height":100.0}}}"#,
            ),
        )
        .unwrap();
        let id2 = r2.result.unwrap()["structuredContent"]["selected_id"].as_u64().unwrap();

        let _ = server.handle_json_str(
            r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"set_style_rgb","arguments":{"channel":"fill","r":220,"g":40,"b":50}}}"#,
        );

        // Apply Contour and Drop Shadow via MCP
        let contour_resp: JsonRpcResponse = serde_json::from_str(&server.handle_json_str(&format!(
            r#"{{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{{"name":"apply_contour","arguments":{{"object_id":{id1},"steps":2,"offset_pt":3.0}}}}}}"#
        ))).unwrap();
        assert!(contour_resp.result.unwrap()["structuredContent"]["contour_applied"].as_bool().unwrap());

        let shadow_resp: JsonRpcResponse = serde_json::from_str(&server.handle_json_str(&format!(
            r#"{{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{{"name":"add_drop_shadow","arguments":{{"object_id":{id1},"offset_x":4.0,"offset_y":4.0,"opacity":0.5}}}}}}"#
        ))).unwrap();
        assert!(shadow_resp.result.unwrap()["structuredContent"]["shadow_added"].as_bool().unwrap());

        // Boolean Weld via MCP
        let weld_resp: JsonRpcResponse = serde_json::from_str(&server.handle_json_str(&format!(
            r#"{{"jsonrpc":"2.0","id":8,"method":"tools/call","params":{{"name":"boolean_selection","arguments":{{"op":"weld","selection":[{id1},{id2}]}}}}}}"#
        ))).unwrap();
        assert!(weld_resp.result.unwrap()["structuredContent"]["boolean_executed"].as_bool().unwrap());

        let cdr_resp: JsonRpcResponse = serde_json::from_str(
            &server.handle_json_str(r#"{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"export_cdr7_byte_len","arguments":{}}}"#),
        )
        .unwrap();
        let cdr_len = cdr_resp.result.unwrap()["structuredContent"]["cdr7_bytes"].as_u64().unwrap();
        assert!(cdr_len > 60);

        let svg_resp: JsonRpcResponse = serde_json::from_str(
            &server.handle_json_str(r#"{"jsonrpc":"2.0","id":10,"method":"tools/call","params":{"name":"export_svg","arguments":{"page_index":0}}}"#),
        )
        .unwrap();
        let svg = svg_resp.result.unwrap()["structuredContent"]["svg"].as_str().unwrap().to_owned();
        assert!(svg.contains("<svg"));
        assert!(svg.contains("<path"));

        let pdf_resp: JsonRpcResponse = serde_json::from_str(
            &server.handle_json_str(r#"{"jsonrpc":"2.0","id":11,"method":"tools/call","params":{"name":"export_pdf_byte_len","arguments":{}}}"#),
        )
        .unwrap();
        let pdf_len = pdf_resp.result.unwrap()["structuredContent"]["pdf_bytes"].as_u64().unwrap();
        assert!(pdf_len > 200);

        let bs_resp: JsonRpcResponse = serde_json::from_str(
            &server.handle_json_str(
                r#"{"jsonrpc":"2.0","id":12,"method":"tools/call","params":{"name":"add_basic_shape","arguments":{"x":40.0,"y":40.0,"width":100.0,"height":90.0,"shape_kind":"heart"}}}"#,
            ),
        )
        .unwrap();
        let bs_id = bs_resp.result.unwrap()["structuredContent"]["selected_id"].as_u64().unwrap();

        let lsys_resp: JsonRpcResponse = serde_json::from_str(
            &server.handle_json_str(
                r#"{"jsonrpc":"2.0","id":13,"method":"tools/call","params":{"name":"add_lsystem_path","arguments":{"origin_x":80.0,"origin_y":120.0,"axiom":"F","rule_symbol":"F","rule_replacement":"F[+F]F[-F]F","turn_angle_deg":25.0,"step_pt":8.0,"iterations":2}}}"#,
            ),
        )
        .unwrap();
        let lsys_id = lsys_resp.result.unwrap()["structuredContent"]["selected_id"].as_u64().unwrap();

        let rso_resp: JsonRpcResponse = serde_json::from_str(&server.handle_json_str(&format!(
            r#"{{"jsonrpc":"2.0","id":14,"method":"tools/call","params":{{"name":"resolve_selection_overlaps","arguments":{{"selection":[{bs_id},{lsys_id}],"min_gap_pt":10.0,"iterations":12}}}}}}"#
        )))
        .unwrap();
        assert!(rso_resp.result.unwrap()["structuredContent"]["overlaps_resolved"].as_bool().unwrap());

        let excl_resp: JsonRpcResponse = serde_json::from_str(&server.handle_json_str(&format!(
            r#"{{"jsonrpc":"2.0","id":15,"method":"tools/call","params":{{"name":"exclude_selection","arguments":{{"selection":[{bs_id},{lsys_id}]}}}}}}"#
        )))
        .unwrap();
        assert!(excl_resp.result.unwrap()["structuredContent"]["excluded"].as_bool().unwrap());

        let vor_resp: JsonRpcResponse = serde_json::from_str(
            &server.handle_json_str(
                r#"{"jsonrpc":"2.0","id":16,"method":"tools/call","params":{"name":"add_voronoi_mosaic","arguments":{"x":30.0,"y":30.0,"width":120.0,"height":90.0,"cell_gap_pt":2.0}}}"#,
            ),
        )
        .unwrap();
        let vor_id = vor_resp.result.unwrap()["structuredContent"]["selected_id"].as_u64().unwrap();

        let chk_resp: JsonRpcResponse = serde_json::from_str(&server.handle_json_str(&format!(
            r#"{{"jsonrpc":"2.0","id":17,"method":"tools/call","params":{{"name":"chaikin_smooth_path","arguments":{{"object_id":{vor_id},"iterations":2}}}}}}"#
        )))
        .unwrap();
        assert!(chk_resp.result.unwrap()["structuredContent"]["chaikin_smoothed"].as_bool().unwrap());

        let vws_resp: JsonRpcResponse = serde_json::from_str(&server.handle_json_str(&format!(
            r#"{{"jsonrpc":"2.0","id":18,"method":"tools/call","params":{{"name":"apply_variable_width_stroke","arguments":{{"object_id":{vor_id},"widths_pt":[2.0,8.0,3.0]}}}}}}"#
        )))
        .unwrap();
        assert!(vws_resp.result.unwrap()["structuredContent"]["variable_width_stroke_applied"].as_bool().unwrap());

        let dxf_b64_resp: JsonRpcResponse = serde_json::from_str(&server.handle_json_str(
            r#"{"jsonrpc":"2.0","id":19,"method":"tools/call","params":{"name":"export_dxf_base64","arguments":{"page_index":0}}}"#,
        ))
        .unwrap();
        assert!(dxf_b64_resp.result.unwrap()["structuredContent"]["dxf_bytes"].as_u64().unwrap() > 20);

        let ibox_resp: JsonRpcResponse = serde_json::from_str(
            &server.handle_json_str(
                r#"{"jsonrpc":"2.0","id":20,"method":"tools/call","params":{"name":"add_isometric_box","arguments":{"cx":100.0,"cy":100.0,"width_pt":50.0,"depth_pt":40.0,"height_pt":60.0}}}"#,
            ),
        )
        .unwrap();
        let ibox_id = ibox_resp.result.unwrap()["structuredContent"]["selected_id"].as_u64().unwrap();

        let wave_resp: JsonRpcResponse = serde_json::from_str(&server.handle_json_str(&format!(
            r#"{{"jsonrpc":"2.0","id":21,"method":"tools/call","params":{{"name":"wave_distort_path","arguments":{{"object_id":{ibox_id},"amplitude_pt":3.0,"frequency":4.0,"phase_deg":0.0}}}}}}"#
        )))
        .unwrap();
        assert!(wave_resp.result.unwrap()["structuredContent"]["wave_distorted"].as_bool().unwrap());

        let rscale_resp: JsonRpcResponse = serde_json::from_str(&server.handle_json_str(&format!(
            r#"{{"jsonrpc":"2.0","id":22,"method":"tools/call","params":{{"name":"radial_scale_selection_positions","arguments":{{"selection":[{ibox_id},{vor_id}],"center_x":100.0,"center_y":100.0,"radial_scale":1.25,"twist_deg":15.0}}}}}}"#
        )))
        .unwrap();
        assert!(rscale_resp.result.unwrap()["structuredContent"]["radial_scaled"].as_bool().unwrap());

        let cdr_b64_resp: JsonRpcResponse = serde_json::from_str(
            &server.handle_json_str(r#"{"jsonrpc":"2.0","id":23,"method":"tools/call","params":{"name":"export_cdr7_base64","arguments":{}}}"#),
        )
        .unwrap();
        assert!(cdr_b64_resp.result.unwrap()["structuredContent"]["cdr7_bytes"].as_u64().unwrap() > 60);

        let sp_resp: JsonRpcResponse = serde_json::from_str(
            &server.handle_json_str(
                r#"{"jsonrpc":"2.0","id":24,"method":"tools/call","params":{"name":"add_spirograph","arguments":{"cx":120.0,"cy":120.0,"r_fixed_pt":60.0,"r_rolling_pt":20.0,"pen_offset_pt":15.0,"epitrochoid":false,"revolutions":3,"steps":96}}}"#,
            ),
        )
        .unwrap();
        let sp_id = sp_resp.result.unwrap()["structuredContent"]["selected_id"].as_u64().unwrap();

        let dsym_resp: JsonRpcResponse = serde_json::from_str(&server.handle_json_str(&format!(
            r#"{{"jsonrpc":"2.0","id":25,"method":"tools/call","params":{{"name":"apply_dihedral_symmetry","arguments":{{"object_id":{sp_id},"center_x":120.0,"center_y":120.0,"folds":4,"reflect":true}}}}}}"#
        )))
        .unwrap();
        assert!(dsym_resp.result.unwrap()["structuredContent"]["symmetry_applied"].as_bool().unwrap());

        let sdist_resp: JsonRpcResponse = serde_json::from_str(&server.handle_json_str(&format!(
            r#"{{"jsonrpc":"2.0","id":26,"method":"tools/call","params":{{"name":"scale_selection_by_distance","arguments":{{"selection":[{sp_id},{ibox_id}],"center_x":100.0,"center_y":100.0,"min_scale":0.6,"max_scale":1.4,"falloff_radius_pt":150.0}}}}}}"#
        )))
        .unwrap();
        assert!(sdist_resp.result.unwrap()["structuredContent"]["scaled_by_distance"].as_bool().unwrap());

        let cmx_b64_resp: JsonRpcResponse = serde_json::from_str(
            &server.handle_json_str(r#"{"jsonrpc":"2.0","id":27,"method":"tools/call","params":{"name":"export_cmx1_base64","arguments":{}}}"#),
        )
        .unwrap();
        assert!(cmx_b64_resp.result.unwrap()["structuredContent"]["cmx1_bytes"].as_u64().unwrap() > 40);

        let rings_resp: JsonRpcResponse = serde_json::from_str(
            &server.handle_json_str(
                r#"{"jsonrpc":"2.0","id":28,"method":"tools/call","params":{"name":"add_concentric_rings","arguments":{"cx":140.0,"cy":140.0,"inner_radius_pt":15.0,"outer_radius_pt":60.0,"ring_count":4}}}"#,
            ),
        )
        .unwrap();
        let rings_id = rings_resp.result.unwrap()["structuredContent"]["selected_id"].as_u64().unwrap();

        let dash_resp: JsonRpcResponse = serde_json::from_str(&server.handle_json_str(&format!(
            r#"{{"jsonrpc":"2.0","id":29,"method":"tools/call","params":{{"name":"dash_object_path","arguments":{{"object_id":{rings_id},"dash_pattern_pt":[10.0,5.0]}}}}}}"#
        )))
        .unwrap();
        assert!(dash_resp.result.unwrap()["structuredContent"]["dashed"].as_bool().unwrap());

        let qpos_resp: JsonRpcResponse = serde_json::from_str(&server.handle_json_str(&format!(
            r#"{{"jsonrpc":"2.0","id":30,"method":"tools/call","params":{{"name":"quantize_selection_positions","arguments":{{"selection":[{rings_id},{sp_id}],"grid_x_pt":10.0,"grid_y_pt":10.0}}}}}}"#
        )))
        .unwrap();
        assert!(qpos_resp.result.unwrap()["structuredContent"]["quantized"].as_bool().unwrap());

        let cdrx_b64_resp: JsonRpcResponse = serde_json::from_str(
            &server.handle_json_str(r#"{"jsonrpc":"2.0","id":31,"method":"tools/call","params":{"name":"export_cdrx_zip_base64","arguments":{}}}"#),
        )
        .unwrap();
        assert!(cdrx_b64_resp.result.unwrap()["structuredContent"]["cdrx_zip_bytes"].as_u64().unwrap() > 80);

        let hc_resp: JsonRpcResponse = serde_json::from_str(
            &server.handle_json_str(
                r#"{"jsonrpc":"2.0","id":32,"method":"tools/call","params":{"name":"add_honeycomb_grid","arguments":{"x":30.0,"y":30.0,"width":120.0,"height":90.0,"cell_radius_pt":18.0,"cell_gap_pt":2.0}}}"#,
            ),
        )
        .unwrap();
        let hc_id = hc_resp.result.unwrap()["structuredContent"]["selected_id"].as_u64().unwrap();

        let chull_resp: JsonRpcResponse = serde_json::from_str(&server.handle_json_str(&format!(
            r#"{{"jsonrpc":"2.0","id":33,"method":"tools/call","params":{{"name":"convex_hull_selection","arguments":{{"selection":[{hc_id},{rings_id}]}}}}}}"#
        )))
        .unwrap();
        assert!(chull_resp.result.unwrap()["structuredContent"]["convex_hull_created"].as_bool().unwrap());

        let acirc_resp: JsonRpcResponse = serde_json::from_str(&server.handle_json_str(&format!(
            r#"{{"jsonrpc":"2.0","id":34,"method":"tools/call","params":{{"name":"arrange_selection_on_circle","arguments":{{"selection":[{hc_id},{rings_id},{sp_id}],"center_x":150.0,"center_y":150.0,"radius_pt":80.0,"start_angle_deg":0.0,"rotate_to_tangent":true}}}}}}"#
        )))
        .unwrap();
        assert!(acirc_resp.result.unwrap()["structuredContent"]["arranged_on_circle"].as_bool().unwrap());

        let pdf_imp_b64_resp: JsonRpcResponse = serde_json::from_str(
            &server.handle_json_str(
                r#"{"jsonrpc":"2.0","id":35,"method":"tools/call","params":{"name":"export_pdf_imposed_base64","arguments":{"rows":2,"cols":2,"crop_marks":true}}}"#,
            ),
        )
        .unwrap();
        assert!(pdf_imp_b64_resp.result.unwrap()["structuredContent"]["pdf_imposed_bytes"].as_u64().unwrap() > 150);

        let hilb_resp: JsonRpcResponse = serde_json::from_str(
            &server.handle_json_str(
                r#"{"jsonrpc":"2.0","id":36,"method":"tools/call","params":{"name":"add_hilbert_curve","arguments":{"x":20.0,"y":20.0,"width":100.0,"height":100.0,"order":3}}}"#,
            ),
        )
        .unwrap();
        let hilb_id = hilb_resp.result.unwrap()["structuredContent"]["selected_id"].as_u64().unwrap();

        let skel_resp: JsonRpcResponse = serde_json::from_str(&server.handle_json_str(&format!(
            r#"{{"jsonrpc":"2.0","id":37,"method":"tools/call","params":{{"name":"extract_path_skeleton","arguments":{{"object_id":{hc_id},"sample_steps":24}}}}}}"#
        )))
        .unwrap();
        assert!(skel_resp.result.unwrap()["structuredContent"]["skeleton_extracted"].as_bool().unwrap());

        let jit_resp: JsonRpcResponse = serde_json::from_str(&server.handle_json_str(&format!(
            r#"{{"jsonrpc":"2.0","id":38,"method":"tools/call","params":{{"name":"jitter_selection_transforms","arguments":{{"selection":[{hilb_id},{hc_id}],"dx_max_pt":5.0,"dy_max_pt":5.0,"angle_max_deg":12.0,"scale_max_pct":15.0,"seed":99}}}}}}"#
        )))
        .unwrap();
        assert!(jit_resp.result.unwrap()["structuredContent"]["jittered"].as_bool().unwrap());

        let cpl_b64_resp: JsonRpcResponse = serde_json::from_str(
            &server.handle_json_str(
                r#"{"jsonrpc":"2.0","id":39,"method":"tools/call","params":{"name":"export_cpl_palette_base64","arguments":{"palette_name":"M56 Palette"}}}"#,
            ),
        )
        .unwrap();
        assert!(cpl_b64_resp.result.unwrap()["structuredContent"]["cpl_bytes"].as_u64().unwrap() > 10);

        let liss_resp: JsonRpcResponse = serde_json::from_str(
            &server.handle_json_str(
                r#"{"jsonrpc":"2.0","id":40,"method":"tools/call","params":{"name":"add_lissajous_curve","arguments":{"cx":140.0,"cy":140.0,"radius_x_pt":60.0,"radius_y_pt":45.0,"freq_x":3,"freq_y":2,"phase_deg":45.0,"steps":96}}}"#,
            ),
        )
        .unwrap();
        let liss_id = liss_resp.result.unwrap()["structuredContent"]["selected_id"].as_u64().unwrap();

        let dens_resp: JsonRpcResponse = serde_json::from_str(&server.handle_json_str(&format!(
            r#"{{"jsonrpc":"2.0","id":41,"method":"tools/call","params":{{"name":"densify_object_path","arguments":{{"object_id":{liss_id},"max_len_pt":8.0}}}}}}"#
        )))
        .unwrap();
        assert!(dens_resp.result.unwrap()["structuredContent"]["densified"].as_bool().unwrap());

        let alkey_resp: JsonRpcResponse = serde_json::from_str(&server.handle_json_str(&format!(
            r#"{{"jsonrpc":"2.0","id":42,"method":"tools/call","params":{{"name":"align_selection_to_key_object","arguments":{{"selection":[{hilb_id},{liss_id}],"key_object_id":{liss_id},"mode":"center"}}}}}}"#
        )))
        .unwrap();
        assert!(alkey_resp.result.unwrap()["structuredContent"]["aligned_to_key"].as_bool().unwrap());

        let xml_pal_resp: JsonRpcResponse = serde_json::from_str(&server.handle_json_str(
            r#"{"jsonrpc":"2.0","id":43,"method":"tools/call","params":{"name":"export_xml_palette","arguments":{"palette_name":"M57 Palette"}}}"#,
        ))
        .unwrap();
        let xml_pal_str = xml_pal_resp.result.unwrap()["structuredContent"]["xml_palette"].as_str().unwrap().to_owned();
        assert!(xml_pal_str.contains("<palette"));

        let phyllo_resp: JsonRpcResponse = serde_json::from_str(
            &server.handle_json_str(
                r#"{"jsonrpc":"2.0","id":44,"method":"tools/call","params":{"name":"add_phyllotaxis_pattern","arguments":{"cx":140.0,"cy":140.0,"max_radius_pt":55.0,"seed_count":16,"dot_radius_pt":3.0}}}"#,
            ),
        )
        .unwrap();
        let phyllo_id = phyllo_resp.result.unwrap()["structuredContent"]["selected_id"].as_u64().unwrap();

        let swarp_resp: JsonRpcResponse = serde_json::from_str(&server.handle_json_str(&format!(
            r#"{{"jsonrpc":"2.0","id":45,"method":"tools/call","params":{{"name":"spherical_warp_path","arguments":{{"object_id":{liss_id},"center_x":140.0,"center_y":140.0,"radius_pt":60.0,"magnification":1.6}}}}}}"#
        )))
        .unwrap();
        assert!(swarp_resp.result.unwrap()["structuredContent"]["spherical_warped"].as_bool().unwrap());

        let cgrad_resp: JsonRpcResponse = serde_json::from_str(&server.handle_json_str(&format!(
            r#"{{"jsonrpc":"2.0","id":46,"method":"tools/call","params":{{"name":"colorize_selection_by_gradient","arguments":{{"selection":[{phyllo_id},{liss_id}],"start_r":240,"start_g":40,"start_b":60,"end_r":40,"end_g":140,"end_b":240,"apply_to_fill":true,"apply_to_outline":true}}}}}}"#
        )))
        .unwrap();
        assert!(cgrad_resp.result.unwrap()["structuredContent"]["colorized_by_gradient"].as_bool().unwrap());

        let imp_xml_req = json!({
            "jsonrpc": "2.0",
            "id": 47,
            "method": "tools/call",
            "params": {
                "name": "import_xml_palette",
                "arguments": {
                    "xml_palette": xml_pal_str
                }
            }
        });
        let imp_xml_resp: JsonRpcResponse = serde_json::from_str(&server.handle_json_str(&imp_xml_req.to_string())).unwrap();
        assert!(imp_xml_resp.result.unwrap()["structuredContent"]["swatch_count"].as_u64().unwrap() >= 1);

        let hwave_resp: JsonRpcResponse = serde_json::from_str(
            &server.handle_json_str(
                r#"{"jsonrpc":"2.0","id":48,"method":"tools/call","params":{"name":"add_halftone_wave_lines","arguments":{"x":30.0,"y":30.0,"width":160.0,"height":80.0,"line_count":6,"amplitude_pt":4.0,"frequency":3.0}}}"#,
            ),
        )
        .unwrap();
        let hwave_id = hwave_resp.result.unwrap()["structuredContent"]["selected_id"].as_u64().unwrap();

        let relax_resp: JsonRpcResponse = serde_json::from_str(&server.handle_json_str(&format!(
            r#"{{"jsonrpc":"2.0","id":49,"method":"tools/call","params":{{"name":"relax_object_path","arguments":{{"object_id":{hwave_id},"iterations":3,"lambda":0.45}}}}}}"#
        )))
        .unwrap();
        assert!(relax_resp.result.unwrap()["structuredContent"]["relaxed"].as_bool().unwrap());

        let apsw_resp: JsonRpcResponse = serde_json::from_str(&server.handle_json_str(&format!(
            r##"{{"jsonrpc":"2.0","id":50,"method":"tools/call","params":{{"name":"apply_palette_swatches_to_selection","arguments":{{"selection":[{hwave_id},{phyllo_id},{liss_id}],"hex_colors":["#E63946","#457B9D","#2A9D8F"],"apply_to_fill":true,"apply_to_outline":true}}}}}}"##
        )))
        .unwrap();
        assert!(apsw_resp.result.unwrap()["structuredContent"]["swatches_applied"].as_bool().unwrap());

        let prep_pdf_resp: JsonRpcResponse = serde_json::from_str(
            &server.handle_json_str(
                r#"{"jsonrpc":"2.0","id":51,"method":"tools/call","params":{"name":"export_pdf_with_prepress_base64","arguments":{"crop_marks":true,"registration_marks":true,"bleed_pt":9.0}}}"#,
            ),
        )
        .unwrap();
        assert!(prep_pdf_resp.result.unwrap()["structuredContent"]["pdf_prepress_bytes"].as_u64().unwrap() > 200);

        let knot_resp: JsonRpcResponse = serde_json::from_str(
            &server.handle_json_str(
                r#"{"jsonrpc":"2.0","id":52,"method":"tools/call","params":{"name":"add_celtic_knot_ring","arguments":{"cx":150.0,"cy":150.0,"radius_pt":55.0,"amplitude_pt":8.0,"lobes":6,"steps":72}}}"#,
            ),
        )
        .unwrap();
        let knot_id = knot_resp.result.unwrap()["structuredContent"]["selected_id"].as_u64().unwrap();

        let fitc_resp: JsonRpcResponse = serde_json::from_str(&server.handle_json_str(&format!(
            r#"{{"jsonrpc":"2.0","id":53,"method":"tools/call","params":{{"name":"fit_circle_to_object_path","arguments":{{"object_id":{liss_id}}}}}}}"#
        )))
        .unwrap();
        assert!(fitc_resp.result.unwrap()["structuredContent"]["circle_fitted"].as_bool().unwrap());

        let snapc_resp: JsonRpcResponse = serde_json::from_str(&server.handle_json_str(&format!(
            r#"{{"jsonrpc":"2.0","id":54,"method":"tools/call","params":{{"name":"snap_selection_to_circle_radius","arguments":{{"selection":[{knot_id},{liss_id}],"center_x":150.0,"center_y":150.0,"radius_pt":75.0}}}}}}"#
        )))
        .unwrap();
        assert!(snapc_resp.result.unwrap()["structuredContent"]["snapped_to_circle_radius"].as_bool().unwrap());

        let prep_svg_resp: JsonRpcResponse = serde_json::from_str(
            &server.handle_json_str(
                r#"{"jsonrpc":"2.0","id":55,"method":"tools/call","params":{"name":"export_svg_with_prepress","arguments":{"crop_marks":true,"registration_marks":true,"bleed_pt":9.0}}}"#,
            ),
        )
        .unwrap();
        assert!(prep_svg_resp.result.unwrap()["structuredContent"]["svg"].as_str().unwrap().contains("<svg"));

        let truch_resp: JsonRpcResponse = serde_json::from_str(
            &server.handle_json_str(
                r#"{"jsonrpc":"2.0","id":56,"method":"tools/call","params":{"name":"add_truchet_tiles","arguments":{"x":40.0,"y":40.0,"width":160.0,"height":120.0,"cols":4,"rows":3,"seed":42}}}"#,
            ),
        )
        .unwrap();
        let truch_id = truch_resp.result.unwrap()["structuredContent"]["selected_id"].as_u64().unwrap();

        let smc_resp: JsonRpcResponse = serde_json::from_str(&server.handle_json_str(&format!(
            r#"{{"jsonrpc":"2.0","id":57,"method":"tools/call","params":{{"name":"smooth_polyline_to_cubic_beziers","arguments":{{"object_id":{knot_id},"tension":1.0}}}}}}"#
        )))
        .unwrap();
        assert!(smc_resp.result.unwrap()["structuredContent"]["smoothed_to_cubic"].as_bool().unwrap());

        let oric_resp: JsonRpcResponse = serde_json::from_str(&server.handle_json_str(&format!(
            r#"{{"jsonrpc":"2.0","id":58,"method":"tools/call","params":{{"name":"orient_selection_to_center","arguments":{{"selection":[{truch_id},{knot_id}],"center_x":150.0,"center_y":150.0,"angle_offset_deg":45.0}}}}}}"#
        )))
        .unwrap();
        assert!(oric_resp.result.unwrap()["structuredContent"]["oriented_to_center"].as_bool().unwrap());

        let exp_cpl_resp: JsonRpcResponse = serde_json::from_str(&server.handle_json_str(
            r#"{"jsonrpc":"2.0","id":59,"method":"tools/call","params":{"name":"export_cpl_palette_base64","arguments":{"palette_name":"M61 CPL"}}}"#,
        ))
        .unwrap();
        let cpl_b64 = exp_cpl_resp.result.unwrap()["structuredContent"]["cpl_base64"].as_str().unwrap().to_owned();

        let imp_cpl_req = json!({
            "jsonrpc": "2.0",
            "id": 60,
            "method": "tools/call",
            "params": {
                "name": "import_cpl_palette_base64",
                "arguments": {
                    "cpl_base64": cpl_b64
                }
            }
        });
        let imp_cpl_resp: JsonRpcResponse = serde_json::from_str(&server.handle_json_str(&imp_cpl_req.to_string())).unwrap();
        assert!(imp_cpl_resp.result.unwrap()["structuredContent"]["swatch_count"].as_u64().unwrap() >= 1);

        let sierp_resp: JsonRpcResponse = serde_json::from_str(
            &server.handle_json_str(
                r#"{"jsonrpc":"2.0","id":61,"method":"tools/call","params":{"name":"add_sierpinski_triangle","arguments":{"x":40.0,"y":40.0,"width":140.0,"height":120.0,"depth":2}}}"#,
            ),
        )
        .unwrap();
        let sierp_id = sierp_resp.result.unwrap()["structuredContent"]["selected_id"].as_u64().unwrap();

        let pp_resp: JsonRpcResponse = serde_json::from_str(&server.handle_json_str(&format!(
            r#"{{"jsonrpc":"2.0","id":62,"method":"tools/call","params":{{"name":"pinch_punch_object_path","arguments":{{"object_id":{sierp_id},"center_x":110.0,"center_y":100.0,"radius_pt":80.0,"strength":-0.35}}}}}}"#
        )))
        .unwrap();
        assert!(pp_resp.result.unwrap()["structuredContent"]["pinched_punched"].as_bool().unwrap());

        let stack_resp: JsonRpcResponse = serde_json::from_str(&server.handle_json_str(&format!(
            r#"{{"jsonrpc":"2.0","id":63,"method":"tools/call","params":{{"name":"stack_selection","arguments":{{"selection":[{sierp_id},{truch_id}],"horizontal":true,"gap_pt":12.0}}}}}}"#
        )))
        .unwrap();
        assert!(stack_resp.result.unwrap()["structuredContent"]["stacked"].as_bool().unwrap());

        let sum_resp: JsonRpcResponse = serde_json::from_str(
            &server.handle_json_str(r#"{"jsonrpc":"2.0","id":64,"method":"tools/call","params":{"name":"get_document_summary","arguments":{}}}"#),
        )
        .unwrap();
        assert!(sum_resp.result.unwrap()["structuredContent"]["total_object_count"].as_u64().unwrap() >= 5);

        let koch_resp: JsonRpcResponse = serde_json::from_str(
            &server.handle_json_str(
                r#"{"jsonrpc":"2.0","id":65,"method":"tools/call","params":{"name":"add_koch_snowflake","arguments":{"cx":150.0,"cy":150.0,"radius_pt":55.0,"iterations":2}}}"#,
            ),
        )
        .unwrap();
        let koch_id = koch_resp.result.unwrap()["structuredContent"]["selected_id"].as_u64().unwrap();

        let flat_resp: JsonRpcResponse = serde_json::from_str(&server.handle_json_str(&format!(
            r#"{{"jsonrpc":"2.0","id":66,"method":"tools/call","params":{{"name":"flatten_object_path_to_polylines","arguments":{{"object_id":{truch_id},"tolerance_pt":0.5}}}}}}"#
        )))
        .unwrap();
        assert!(flat_resp.result.unwrap()["structuredContent"]["flattened_to_polylines"].as_bool().unwrap());

        let scp_resp: JsonRpcResponse = serde_json::from_str(&server.handle_json_str(&format!(
            r#"{{"jsonrpc":"2.0","id":67,"method":"tools/call","params":{{"name":"scale_selection_progressively","arguments":{{"selection":[{koch_id},{truch_id}],"start_scale":0.8,"end_scale":1.2}}}}}}"#
        )))
        .unwrap();
        assert!(scp_resp.result.unwrap()["structuredContent"]["scaled_progressively"].as_bool().unwrap());

        let cz_resp: JsonRpcResponse =
            serde_json::from_str(&server.handle_json_str(
                r#"{"jsonrpc":"2.0","id":68,"method":"tools/call","params":{"name":"export_coralcraft_zip_base64","arguments":{}}}"#,
            ))
            .unwrap();
        let cz_result = cz_resp.result.unwrap();
        assert!(cz_result["structuredContent"]["coralcraft_zip_bytes"].as_u64().unwrap() > 100);
        let cz_b64 = cz_result["structuredContent"]["coralcraft_zip_base64"].as_str().unwrap().to_owned();

        let drag_resp: JsonRpcResponse = serde_json::from_str(
            &server.handle_json_str(
                r#"{"jsonrpc":"2.0","id":69,"method":"tools/call","params":{"name":"add_dragon_curve","arguments":{"start_x":60.0,"start_y":120.0,"end_x":200.0,"end_y":120.0,"iterations":5}}}"#,
            ),
        )
        .unwrap();
        let drag_id = drag_resp.result.unwrap()["structuredContent"]["selected_id"].as_u64().unwrap();

        let sw_resp: JsonRpcResponse = serde_json::from_str(&server.handle_json_str(&format!(
            r#"{{"jsonrpc":"2.0","id":70,"method":"tools/call","params":{{"name":"shear_warp_object_path","arguments":{{"object_id":{drag_id},"shear_x_pt":12.0,"shear_y_pt":-4.0}}}}}}"#
        )))
        .unwrap();
        assert!(sw_resp.result.unwrap()["structuredContent"]["shear_warped"].as_bool().unwrap());

        let rotp_resp: JsonRpcResponse = serde_json::from_str(&server.handle_json_str(&format!(
            r#"{{"jsonrpc":"2.0","id":71,"method":"tools/call","params":{{"name":"rotate_selection_progressively","arguments":{{"selection":[{drag_id},{koch_id}],"start_angle_deg":-15.0,"end_angle_deg":45.0}}}}}}"#
        )))
        .unwrap();
        assert!(rotp_resp.result.unwrap()["structuredContent"]["rotated_progressively"].as_bool().unwrap());

        let imp_cz_req = json!({
            "jsonrpc": "2.0",
            "id": 72,
            "method": "tools/call",
            "params": {
                "name": "import_coralcraft_zip_base64",
                "arguments": {
                    "coralcraft_zip_base64": cz_b64
                }
            }
        });
        let imp_cz_resp: JsonRpcResponse = serde_json::from_str(&server.handle_json_str(&imp_cz_req.to_string())).unwrap();
        assert!(imp_cz_resp.result.unwrap()["structuredContent"]["imported_coralcraft"].as_bool().unwrap());

        let exp_eps_resp: JsonRpcResponse = serde_json::from_str(
            &server.handle_json_str(r#"{"jsonrpc":"2.0","id":73,"method":"tools/call","params":{"name":"export_eps","arguments":{}}}"#),
        )
        .unwrap();
        let eps_str = exp_eps_resp.result.unwrap()["structuredContent"]["eps"].as_str().unwrap().to_owned();
        let imp_eps_req = json!({
            "jsonrpc": "2.0",
            "id": 74,
            "method": "tools/call",
            "params": {
                "name": "import_eps",
                "arguments": { "eps": eps_str }
            }
        });
        let imp_eps_resp: JsonRpcResponse = serde_json::from_str(&server.handle_json_str(&imp_eps_req.to_string())).unwrap();
        assert!(imp_eps_resp.result.unwrap()["structuredContent"]["imported_eps"].as_bool().unwrap());

        let exp_pdf_resp: JsonRpcResponse = serde_json::from_str(
            &server.handle_json_str(r#"{"jsonrpc":"2.0","id":75,"method":"tools/call","params":{"name":"export_pdf_base64","arguments":{}}}"#),
        )
        .unwrap();
        let pdf_b64 = exp_pdf_resp.result.unwrap()["structuredContent"]["pdf_base64"].as_str().unwrap().to_owned();
        let imp_pdf_req = json!({
            "jsonrpc": "2.0",
            "id": 76,
            "method": "tools/call",
            "params": {
                "name": "import_pdf_base64",
                "arguments": { "pdf_base64": pdf_b64 }
            }
        });
        let imp_pdf_resp: JsonRpcResponse = serde_json::from_str(&server.handle_json_str(&imp_pdf_req.to_string())).unwrap();
        assert!(imp_pdf_resp.result.unwrap()["structuredContent"]["imported_pdf"].as_bool().unwrap());
    }

    #[test]
    fn mcp_protocol_compliance_r4_1() {
        let mut server = McpServer::new();

        // 1. JSON-RPC 2.0 notifications (no `id` field) must produce no response.
        assert!(server.handle_message(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).is_none());
        assert!(server.handle_message(r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":1}}"#).is_none());

        // 2. `ping` returns an empty object result.
        let ping_raw = server.handle_message(r#"{"jsonrpc":"2.0","id":10,"method":"ping"}"#).unwrap();
        let ping_resp: JsonRpcResponse = serde_json::from_str(&ping_raw).unwrap();
        assert_eq!(ping_resp.id, json!(10));
        assert_eq!(ping_resp.result, Some(json!({})));
        assert!(ping_resp.error.is_none());

        // 3. `initialize` negotiates protocolVersion.
        let init_2025: JsonRpcResponse = serde_json::from_str(
            &server.handle_message(r#"{"jsonrpc":"2.0","id":11,"method":"initialize","params":{"protocolVersion":"2025-03-26"}}"#).unwrap(),
        )
        .unwrap();
        assert_eq!(init_2025.result.unwrap()["protocolVersion"], "2025-03-26");

        let init_fallback: JsonRpcResponse = serde_json::from_str(
            &server.handle_message(r#"{"jsonrpc":"2.0","id":12,"method":"initialize","params":{"protocolVersion":"1999-01-01"}}"#).unwrap(),
        )
        .unwrap();
        assert_eq!(init_fallback.result.unwrap()["protocolVersion"], "2024-11-05");

        // 4. Invalid Request (-32600) when jsonrpc != "2.0" or method is empty.
        let bad_ver: JsonRpcResponse = serde_json::from_str(&server.handle_message(r#"{"jsonrpc":"1.0","id":13,"method":"ping"}"#).unwrap()).unwrap();
        assert_eq!(bad_ver.error.unwrap().code, -32600);

        // 5. Invalid params (-32602) when `tools/call` is missing `name` or specifies an unknown tool.
        let missing_name: JsonRpcResponse =
            serde_json::from_str(&server.handle_message(r#"{"jsonrpc":"2.0","id":14,"method":"tools/call","params":{}}"#).unwrap()).unwrap();
        assert_eq!(missing_name.error.unwrap().code, -32602);

        let unknown_tool: JsonRpcResponse = serde_json::from_str(
            &server.handle_message(r#"{"jsonrpc":"2.0","id":15,"method":"tools/call","params":{"name":"nonexistent_tool","arguments":{}}}"#).unwrap(),
        )
        .unwrap();
        assert_eq!(unknown_tool.error.unwrap().code, -32602);

        // 6. Byte-safe `serve_stdio` survives non-UTF8 line and notification line, then answers subsequent `ping`.
        let mut input = Vec::new();
        input.extend_from_slice(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n");
        input.extend_from_slice(b"\xff\xfe\xfd\n");
        input.extend_from_slice(b"{\"jsonrpc\":\"2.0\",\"id\":99,\"method\":\"ping\"}\n");
        let mut output = Vec::new();
        server.serve_stdio(std::io::Cursor::new(input), &mut output).unwrap();
        let out_str = String::from_utf8(output).unwrap();
        let lines: Vec<&str> = out_str.lines().collect();
        assert_eq!(lines.len(), 2, "expected 2 response lines (parse error + ping), got: {lines:?}");
        let err_line: JsonRpcResponse = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(err_line.error.unwrap().code, -32700);
        let ok_line: JsonRpcResponse = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(ok_line.id, json!(99));
        assert_eq!(ok_line.result, Some(json!({})));
    }

    #[test]
    fn mcp_tools_schema_and_all_tools_smoke_r4_2() {
        let mut server = McpServer::new();
        let list_resp: JsonRpcResponse = serde_json::from_str(&server.handle_json_str(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#)).unwrap();
        let tools = list_resp.result.unwrap()["tools"].as_array().unwrap().clone();
        assert_eq!(tools.len(), 246);

        for tool in &tools {
            let name = tool["name"].as_str().unwrap();
            let schema = &tool["inputSchema"];
            assert_eq!(schema["type"], "object", "tool {name} inputSchema.type must be 'object'");
            assert!(schema["properties"].is_object(), "tool {name} inputSchema.properties must be an object");
        }

        // Verify engine rejection sets `isError: true` in MCP tool result.
        let fail_group: JsonRpcResponse = serde_json::from_str(
            &server.handle_json_str(r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"group_selection","arguments":{}}}"#),
        )
        .unwrap();
        assert_eq!(fail_group.result.unwrap()["isError"], true);

        // Verify the 4 new tools: translate_selection, move_bezier_node, unlock_all_objects, delete_selection.
        let r_rect: JsonRpcResponse = serde_json::from_str(
            &server.handle_json_str(r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"add_rectangle","arguments":{"x":10.0,"y":10.0,"width":50.0,"height":40.0}}}"#),
        )
        .unwrap();
        assert_eq!(r_rect.result.as_ref().unwrap()["isError"], false);
        let obj_id = r_rect.result.unwrap()["structuredContent"]["selected_id"].as_u64().unwrap();

        let r_tr: JsonRpcResponse = serde_json::from_str(&server.handle_json_str(&format!(
            r#"{{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{{"name":"translate_selection","arguments":{{"selection":[{obj_id}],"dx_pt":15.0,"dy_pt":25.0}}}}}}"#
        )))
        .unwrap();
        assert_eq!(r_tr.result.as_ref().unwrap()["isError"], false);
        assert_eq!(r_tr.result.unwrap()["structuredContent"]["translated"], true);

        let _ = server.handle_json_str(&format!(
            r#"{{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{{"name":"convert_to_curves","arguments":{{"selection":[{obj_id}]}}}}}}"#
        ));
        let r_mv: JsonRpcResponse = serde_json::from_str(&server.handle_json_str(&format!(
            r#"{{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{{"name":"move_bezier_node","arguments":{{"object_id":{obj_id},"element_index":0,"x":30.0,"y":40.0}}}}}}"#
        )))
        .unwrap();
        assert_eq!(r_mv.result.as_ref().unwrap()["isError"], false);
        assert_eq!(r_mv.result.unwrap()["structuredContent"]["node_moved"], true);

        let _ = server.handle_json_str(&format!(
            r#"{{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{{"name":"set_object_locked","arguments":{{"object_id":{obj_id},"locked":true}}}}}}"#
        ));
        let r_unlock: JsonRpcResponse = serde_json::from_str(
            &server.handle_json_str(r#"{"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"unlock_all_objects","arguments":{}}}"#),
        )
        .unwrap();
        assert_eq!(r_unlock.result.as_ref().unwrap()["isError"], false);
        assert_eq!(r_unlock.result.unwrap()["structuredContent"]["unlocked_all"], true);

        let r_del: JsonRpcResponse = serde_json::from_str(&server.handle_json_str(&format!(
            r#"{{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{{"name":"delete_selection","arguments":{{"selection":[{obj_id}]}}}}}}"#
        )))
        .unwrap();
        assert_eq!(r_del.result.as_ref().unwrap()["isError"], false);
        assert_eq!(r_del.result.unwrap()["structuredContent"]["deleted"], true);

        // Table-driven smoke test across all 246 registered tools in `tools/list`.
        for (idx, tool) in tools.iter().enumerate() {
            let name = tool["name"].as_str().unwrap();
            let req = json!({
                "jsonrpc": "2.0",
                "id": 1000 + idx,
                "method": "tools/call",
                "params": {
                    "name": name,
                    "arguments": {}
                }
            });
            let resp: JsonRpcResponse = serde_json::from_str(&server.handle_json_str(&req.to_string())).unwrap();
            assert!(resp.error.is_none(), "tool {name} returned unexpected JSON-RPC protocol error: {:?}", resp.error);
            let result = resp.result.expect("tool call must produce a result object");
            assert!(result["content"].is_array(), "tool {name} result.content must be an array");
            assert!(result["isError"].is_boolean(), "tool {name} result.isError must be a boolean");
        }
    }
}
