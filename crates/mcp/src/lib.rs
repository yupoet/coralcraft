//! Model Context Protocol (MCP) JSON-RPC 2.0 server (`coralcraft-mcp`, L6)
//! for agent-driven multi-page CorelDRAW illustration and vector export.

use coralcraft_cdr::write_cdr7;
use coralcraft_color::Color;
use coralcraft_doc::{Document, ObjectId, ObjectKind};
use coralcraft_engine::{EditorEngine, EngineCommand, PaletteChannel};
use coralcraft_geom::{CornerKind, EllipseSpec, F64, Point, PolygonSpec, Rect, RectSpec};
use coralcraft_render::{export_pdf, export_svg};
use coralcraft_tools::{AlignMode, ZOrderOp};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

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
}

impl McpServer {
    /// Creates a new MCP server initialized with a blank A4 Portrait document.
    #[must_use]
    pub fn new() -> Self {
        Self { engine: EditorEngine::default() }
    }

    /// Processes a single JSON-RPC 2.0 request string and returns a serialized JSON-RPC 2.0 response.
    #[must_use]
    pub fn handle_json_str(&mut self, line: &str) -> String {
        let resp = match serde_json::from_str::<JsonRpcRequest>(line) {
            Ok(req) => self.handle_request(req),
            Err(err) => JsonRpcResponse {
                jsonrpc: "2.0".to_owned(),
                id: Value::Null,
                result: None,
                error: Some(JsonRpcError { code: -32700, message: format!("Parse error: {err}") }),
            },
        };
        serde_json::to_string(&resp)
            .unwrap_or_else(|_| r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32603,"message":"Internal serialization error"}}"#.to_owned())
    }

    /// Handles a parsed [`JsonRpcRequest`].
    pub fn handle_request(&mut self, req: JsonRpcRequest) -> JsonRpcResponse {
        match req.method.as_str() {
            "initialize" => JsonRpcResponse {
                jsonrpc: "2.0".to_owned(),
                id: req.id,
                result: Some(json!({
                    "protocolVersion": "2024-11-05",
                    "serverInfo": {
                        "name": "coralcraft-mcp",
                        "version": env!("CARGO_PKG_VERSION"),
                    },
                    "capabilities": {
                        "tools": {}
                    }
                })),
                error: None,
            },
            "tools/list" => JsonRpcResponse {
                jsonrpc: "2.0".to_owned(),
                id: req.id,
                result: Some(json!({
                    "tools": [
                        {
                            "name": "new_document",
                            "description": "Resets the workspace to a fresh A4 Portrait multi-page document.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "document_summary",
                            "description": "Returns page count, active page, selection summary, and object counts.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "add_rectangle",
                            "description": "Adds a parametric CorelDRAW rectangle (x, y, width, height, radius, corner_kind).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "add_ellipse",
                            "description": "Adds a parametric CorelDRAW ellipse (x, y, width, height, mode, start_deg, end_deg).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "add_polygon",
                            "description": "Adds a regular polygon or star (cx, cy, radius, inner_radius, points).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "add_artistic_text",
                            "description": "Adds a CorelDRAW Artistic Text object (x, y, text, font_family, size_pt).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "set_style_rgb",
                            "description": "Sets RGB fill or outline color on the current selection (or creation defaults).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "add_page",
                            "description": "Inserts a new page after the active page and activates it.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "group_selection",
                            "description": "Groups >= 2 selected objects into an ObjectKind::Group (Ctrl+G).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "ungroup_selection",
                            "description": "Ungroups selected ObjectKind::Group nodes one level (Ctrl+U).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "convert_to_curves",
                            "description": "Converts selected parametric shapes into editable Bezier paths (Ctrl+Q).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "reorder_selection",
                            "description": "Reorders selected objects within their layer (to_front, to_back, forward_one, backward_one).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "align_selection",
                            "description": "Aligns selected objects (left, right, top, bottom, center_horizontal, center_vertical, center_to_page).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "place_inside_powerclip",
                            "description": "Places selected objects inside container_id as PowerClip children (up to depth 3).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "extract_powerclip",
                            "description": "Extracts PowerClip children from container_id back into the active layer.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "export_svg",
                            "description": "Exports the specified 0-based page as clean-room SVG 1.1 markup.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "export_pdf_byte_len",
                            "description": "Exports the multi-page document to ISO 32000-1 PDF 1.4 and returns its byte length.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "export_cdr7_byte_len",
                            "description": "Exports the multi-page document to clean-room RIFF CDR7 and returns its byte length.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "undo",
                            "description": "Undoes the most recent editor command.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "redo",
                            "description": "Redoes the most recently undone editor command.",
                            "inputSchema": { "type": "object" }
                        }
                    ]
                })),
                error: None,
            },
            "tools/call" => {
                let name = req.params.get("name").and_then(Value::as_str).unwrap_or_default();
                let args = req.params.get("arguments").cloned().unwrap_or_else(|| json!({}));
                match self.call_tool(name, &args) {
                    Ok(content) => JsonRpcResponse {
                        jsonrpc: "2.0".to_owned(),
                        id: req.id,
                        result: Some(json!({
                            "content": [{ "type": "text", "text": content.to_string() }],
                            "structuredContent": content,
                            "isError": false
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
            other => JsonRpcResponse {
                jsonrpc: "2.0".to_owned(),
                id: req.id,
                result: None,
                error: Some(JsonRpcError { code: -32601, message: format!("Method not found: {other}") }),
            },
        }
    }

    fn apply_optional_selection(&mut self, args: &Value) {
        if let Some(arr) = args.get("selection").and_then(Value::as_array) {
            let ids: Vec<ObjectId> = arr.iter().filter_map(|v| v.as_u64().map(ObjectId)).collect();
            self.engine.set_selection(ids);
        }
    }

    fn call_tool(&mut self, name: &str, args: &Value) -> Result<Value, String> {
        match name {
            "new_document" => {
                self.engine = EditorEngine::new(Document::new_a4_portrait());
                Ok(json!({ "status": "ok", "pages": self.engine.document.pages.len() }))
            }
            "document_summary" => {
                let total_objects: usize = self.engine.document.pages.iter().flat_map(|p| &p.layers).map(|l| l.objects.len()).sum();
                let status = self.engine.status_bar_snapshot();
                Ok(json!({
                    "title": self.engine.document.title,
                    "pages": self.engine.document.pages.len(),
                    "active_page_index": self.engine.current_page_index(),
                    "total_page_objects": total_objects,
                    "selection_summary": status.selection_summary,
                }))
            }
            "add_rectangle" => {
                let x = args.get("x").and_then(Value::as_f64).unwrap_or(50.0);
                let y = args.get("y").and_then(Value::as_f64).unwrap_or(50.0);
                let w = args.get("width").and_then(Value::as_f64).unwrap_or(120.0);
                let h = args.get("height").and_then(Value::as_f64).unwrap_or(80.0);
                let radius = args.get("radius").and_then(Value::as_f64).unwrap_or(0.0);
                let corner_kind = match args.get("corner_kind").and_then(Value::as_str) {
                    Some("scalloped") => CornerKind::Scalloped,
                    Some("chamfered") => CornerKind::Chamfered,
                    _ => CornerKind::Round,
                };
                let rect = Rect::new(x, y, x + w, y + h).map_err(|e| e.to_string())?;
                let spec = RectSpec::uniform(rect, radius, corner_kind).map_err(|e| e.to_string())?;
                self.engine.execute(EngineCommand::AddObject { name: Some("Rectangle".to_owned()), kind: ObjectKind::Rect(spec) });
                Ok(json!({ "added": "rectangle", "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "add_ellipse" => {
                let x = args.get("x").and_then(Value::as_f64).unwrap_or(60.0);
                let y = args.get("y").and_then(Value::as_f64).unwrap_or(60.0);
                let w = args.get("width").and_then(Value::as_f64).unwrap_or(100.0);
                let h = args.get("height").and_then(Value::as_f64).unwrap_or(100.0);
                let rect = Rect::new(x, y, x + w, y + h).map_err(|e| e.to_string())?;
                let spec = match args.get("mode").and_then(Value::as_str) {
                    Some("pie") => {
                        let start = args.get("start_deg").and_then(Value::as_f64).unwrap_or(0.0);
                        let end = args.get("end_deg").and_then(Value::as_f64).unwrap_or(270.0);
                        EllipseSpec::pie(rect, start, end).map_err(|e| e.to_string())?
                    }
                    Some("arc") => {
                        let start = args.get("start_deg").and_then(Value::as_f64).unwrap_or(0.0);
                        let end = args.get("end_deg").and_then(Value::as_f64).unwrap_or(180.0);
                        EllipseSpec::arc(rect, start, end).map_err(|e| e.to_string())?
                    }
                    _ => EllipseSpec::full(rect),
                };
                self.engine.execute(EngineCommand::AddObject { name: Some("Ellipse".to_owned()), kind: ObjectKind::Ellipse(spec) });
                Ok(json!({ "added": "ellipse", "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "add_polygon" => {
                let cx = args.get("cx").and_then(Value::as_f64).unwrap_or(150.0);
                let cy = args.get("cy").and_then(Value::as_f64).unwrap_or(150.0);
                let radius = args.get("radius").and_then(Value::as_f64).unwrap_or(50.0);
                let points = args.get("points").and_then(Value::as_u64).unwrap_or(5) as u32;
                let center = Point::new(cx, cy).map_err(|e| e.to_string())?;
                let spec = if let Some(inner) = args.get("inner_radius").and_then(Value::as_f64) {
                    PolygonSpec::star(center, radius, inner, points, 0.0).map_err(|e| e.to_string())?
                } else {
                    PolygonSpec::regular(center, radius, points, 0.0).map_err(|e| e.to_string())?
                };
                self.engine.execute(EngineCommand::AddObject { name: Some("Polygon".to_owned()), kind: ObjectKind::Polygon(spec) });
                Ok(json!({ "added": "polygon", "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "add_artistic_text" => {
                let x = args.get("x").and_then(Value::as_f64).unwrap_or(72.0);
                let y = args.get("y").and_then(Value::as_f64).unwrap_or(72.0);
                let text = args.get("text").and_then(Value::as_str).unwrap_or("CoralCraft").to_owned();
                let font_family = args.get("font_family").and_then(Value::as_str).unwrap_or("Sans").to_owned();
                let size_pt = F64::new(args.get("size_pt").and_then(Value::as_f64).unwrap_or(24.0)).map_err(|e| e.to_string())?;
                let position = Point::new(x, y).map_err(|e| e.to_string())?;
                self.engine.execute(EngineCommand::AddObject {
                    name: Some("Artistic Text".to_owned()),
                    kind: ObjectKind::ArtisticText { text, font_family, size_pt, position },
                });
                Ok(json!({ "added": "artistic_text", "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "set_style_rgb" => {
                let channel = match args.get("channel").and_then(Value::as_str) {
                    Some("outline") => PaletteChannel::Outline,
                    _ => PaletteChannel::Fill,
                };
                let clear = args.get("clear").and_then(Value::as_bool).unwrap_or(false);
                let color = if clear {
                    None
                } else {
                    let r_u64 = args.get("r").and_then(Value::as_u64).unwrap_or(0);
                    let g_u64 = args.get("g").and_then(Value::as_u64).unwrap_or(0);
                    let b_u64 = args.get("b").and_then(Value::as_u64).unwrap_or(0);
                    let r = u8::try_from(r_u64).map_err(|_| format!("RGB r={r_u64} out of range 0..=255"))?;
                    let g = u8::try_from(g_u64).map_err(|_| format!("RGB g={g_u64} out of range 0..=255"))?;
                    let b = u8::try_from(b_u64).map_err(|_| format!("RGB b={b_u64} out of range 0..=255"))?;
                    Some(Color::rgb8(r, g, b))
                };
                self.engine.execute(EngineCommand::ApplyPaletteSwatch { channel, color });
                Ok(json!({ "status": "ok" }))
            }
            "add_page" => {
                self.engine.execute(EngineCommand::AddPageAfterActive);
                Ok(json!({ "pages": self.engine.document.pages.len(), "active_page_index": self.engine.current_page_index() }))
            }
            "group_selection" => {
                self.apply_optional_selection(args);
                let ok = self.engine.execute(EngineCommand::GroupSelection);
                Ok(json!({ "grouped": ok, "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "ungroup_selection" => {
                self.apply_optional_selection(args);
                let ok = self.engine.execute(EngineCommand::UngroupSelection);
                let ids: Vec<u64> = self.engine.selection.iter().map(|id| id.0).collect();
                Ok(json!({ "ungrouped": ok, "selected_ids": ids }))
            }
            "convert_to_curves" => {
                self.apply_optional_selection(args);
                let ok = self.engine.execute(EngineCommand::ConvertToCurves);
                Ok(json!({ "converted": ok }))
            }
            "reorder_selection" => {
                self.apply_optional_selection(args);
                let op = match args.get("op").and_then(Value::as_str).unwrap_or("to_front") {
                    "to_front" => ZOrderOp::ToFront,
                    "to_back" => ZOrderOp::ToBack,
                    "forward_one" => ZOrderOp::ForwardOne,
                    "backward_one" => ZOrderOp::BackwardOne,
                    other => return Err(format!("Unsupported reorder op: {other}")),
                };
                let ok = self.engine.execute(EngineCommand::ReorderSelection { op });
                Ok(json!({ "reordered": ok }))
            }
            "align_selection" => {
                self.apply_optional_selection(args);
                let mode = match args.get("mode").and_then(Value::as_str).unwrap_or("center_to_page") {
                    "left" => AlignMode::Left,
                    "right" => AlignMode::Right,
                    "top" => AlignMode::Top,
                    "bottom" => AlignMode::Bottom,
                    "center_horizontal" => AlignMode::CenterHorizontal,
                    "center_vertical" => AlignMode::CenterVertical,
                    "center_to_page" => AlignMode::CenterToPage,
                    other => return Err(format!("Unsupported align mode: {other}")),
                };
                let ok = self.engine.execute(EngineCommand::AlignSelection { mode });
                Ok(json!({ "aligned": ok }))
            }
            "place_inside_powerclip" => {
                self.apply_optional_selection(args);
                let container_raw = args.get("container_id").and_then(Value::as_u64).ok_or_else(|| "Missing container_id".to_owned())?;
                let ok = self.engine.execute(EngineCommand::PlaceInsidePowerClip { container_id: ObjectId(container_raw) });
                Ok(json!({ "placed": ok, "container_id": container_raw }))
            }
            "extract_powerclip" => {
                let container_raw = args.get("container_id").and_then(Value::as_u64).ok_or_else(|| "Missing container_id".to_owned())?;
                let ok = self.engine.execute(EngineCommand::ExtractPowerClip { container_id: ObjectId(container_raw) });
                let ids: Vec<u64> = self.engine.selection.iter().map(|id| id.0).collect();
                Ok(json!({ "extracted": ok, "extracted_ids": ids }))
            }
            "export_svg" => {
                let page_index = args.get("page_index").and_then(Value::as_u64).map_or_else(|| self.engine.current_page_index(), |idx| idx as usize);
                let svg = export_svg(&self.engine.document, page_index).map_err(|e| e.to_string())?;
                Ok(json!({ "svg": svg }))
            }
            "export_pdf_byte_len" => {
                let pdf = export_pdf(&self.engine.document).map_err(|e| e.to_string())?;
                Ok(json!({ "pdf_bytes": pdf.len() }))
            }
            "export_cdr7_byte_len" => {
                let cdr = write_cdr7(&self.engine.document).map_err(|e| e.to_string())?;
                Ok(json!({ "cdr7_bytes": cdr.len() }))
            }
            "undo" => Ok(json!({ "undone": self.engine.undo() })),
            "redo" => Ok(json!({ "redone": self.engine.redo() })),
            other => Err(format!("Unknown tool: {other}")),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn mcp_server_handles_initialize_list_and_end_to_end_authoring_tools() {
        let mut server = McpServer::new();

        let init_resp: JsonRpcResponse = serde_json::from_str(&server.handle_json_str(r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#)).unwrap();
        assert!(init_resp.error.is_none());

        let list_resp: JsonRpcResponse = serde_json::from_str(&server.handle_json_str(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#)).unwrap();
        let tools = list_resp.result.unwrap()["tools"].as_array().unwrap().len();
        assert_eq!(tools, 20);

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

        let group_resp: JsonRpcResponse = serde_json::from_str(&server.handle_json_str(&format!(
            r#"{{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{{"name":"group_selection","arguments":{{"selection":[{id1},{id2}]}}}}}}"#
        )))
        .unwrap();
        assert!(group_resp.result.unwrap()["structuredContent"]["grouped"].as_bool().unwrap());

        let ungroup_resp: JsonRpcResponse = serde_json::from_str(
            &server.handle_json_str(r#"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"ungroup_selection","arguments":{}}}"#),
        )
        .unwrap();
        assert!(ungroup_resp.result.unwrap()["structuredContent"]["ungrouped"].as_bool().unwrap());

        let cdr_resp: JsonRpcResponse = serde_json::from_str(
            &server.handle_json_str(r#"{"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"export_cdr7_byte_len","arguments":{}}}"#),
        )
        .unwrap();
        let cdr_len = cdr_resp.result.unwrap()["structuredContent"]["cdr7_bytes"].as_u64().unwrap();
        assert!(cdr_len > 60);

        let svg_resp: JsonRpcResponse = serde_json::from_str(
            &server.handle_json_str(r#"{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"export_svg","arguments":{"page_index":0}}}"#),
        )
        .unwrap();
        let svg = svg_resp.result.unwrap()["structuredContent"]["svg"].as_str().unwrap().to_owned();
        assert!(svg.contains("<svg"));
        assert!(svg.contains("<path"));

        let pdf_resp: JsonRpcResponse = serde_json::from_str(
            &server.handle_json_str(r#"{"jsonrpc":"2.0","id":10,"method":"tools/call","params":{"name":"export_pdf_byte_len","arguments":{}}}"#),
        )
        .unwrap();
        let pdf_len = pdf_resp.result.unwrap()["structuredContent"]["pdf_bytes"].as_u64().unwrap();
        assert!(pdf_len > 200);
    }
}
