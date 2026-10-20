//! Model Context Protocol (MCP) JSON-RPC 2.0 server (`coralcraft-mcp`, L6)
//! for agent-driven multi-page CorelDRAW illustration and vector export.

use coralcraft_cdr::write_cdr7;
use coralcraft_color::Color;
use coralcraft_doc::{Document, ObjectId, ObjectKind, ParagraphTextSpec, TextAlignment, TransparencyStyle, import_dxf, import_svg};
use coralcraft_engine::{EditorEngine, EngineCommand, PaletteChannel};
use coralcraft_geom::{
    BooleanOp, CornerKind, EllipseSpec, F64, Point, PolygonSpec, Rect, RectSpec, build_callout_path, build_connector_path, build_graph_paper_path,
    build_spiral_path,
};
use coralcraft_render::{ImpositionSpec, export_dxf, export_eps, export_pdf, export_pdf_imposed, export_svg};
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
                            "name": "add_paragraph_text",
                            "description": "Adds a multi-line word-wrapped Paragraph Text frame (x, y, width, height, text, font_family, size_pt, alignment, columns, gutter_pt).",
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
                        },
                        {
                            "name": "boolean_selection",
                            "description": "Executes a CorelDRAW Boolean operation on selected objects (weld, trim, intersect, front_minus_back, back_minus_front).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "apply_contour",
                            "description": "Applies a non-destructive Contour vector effect (steps, offset_pt, inside).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "add_drop_shadow",
                            "description": "Applies a non-destructive Drop Shadow vector effect (offset_x, offset_y, opacity, r, g, b).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "add_extrude",
                            "description": "Applies a non-destructive Extrude 3D block vector effect (depth_pt, angle_deg).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "create_blend",
                            "description": "Creates a non-destructive Blend progression between start_id and end_id (steps).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "attach_blend_spine",
                            "description": "Attaches a custom Bezier spine object (spine_object_id) to a Blend effect on blend_object_id.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "apply_envelope",
                            "description": "Applies a non-destructive 4-corner bilinear Envelope warp (tl_x, tl_y, tr_x, tr_y, br_x, br_y, bl_x, bl_y) to object_id.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "apply_artistic_brush",
                            "description": "Applies a non-destructive Artistic Media variable-width calligraphic brush stroke (start_width_pt, end_width_pt) to object_id.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "fit_text_to_path",
                            "description": "Flows an Artistic Text object (text_object_id) along a spine path object (spine_object_id) with optional offset_pt.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "step_and_repeat",
                            "description": "Duplicates selected objects in a parametric array (count, offset_x_pt, offset_y_pt).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "add_linear_dimension",
                            "description": "Adds an engineering linear dimension annotation between (x1, y1) and (x2, y2) with offset_pt and label.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "distribute_selection",
                            "description": "Distributes >= 3 selected objects evenly (left, center_horizontal, right, spacing_horizontal, top, center_vertical, bottom, spacing_vertical).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "apply_perspective",
                            "description": "Applies a non-destructive 4-corner 2-point vanishing projective Perspective homography (tl_x, tl_y, tr_x, tr_y, br_x, br_y, bl_x, bl_y) to object_id.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "fillet_path_corners",
                            "description": "Applies Fillet (round), Scallop (scalloped), or Chamfer (chamfered) corner modification with radius_pt to object_id.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "expand_effects",
                            "description": "Bakes live non-destructive vector effects on object_id into static vector geometry.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "set_transparency",
                            "description": "Applies uniform or linear fountain transparency to selected objects.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "export_eps",
                            "description": "Exports a document page to Encapsulated PostScript (EPSF-3.0).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "export_dxf",
                            "description": "Exports a document page to AutoCAD ASCII DXF (AC1009 / R12).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "import_svg",
                            "description": "Replaces the active document by importing an SVG 1.1 string.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "import_dxf",
                            "description": "Replaces the active document by importing an AutoCAD ASCII DXF string.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "add_spiral",
                            "description": "Adds a Symmetrical (expansion=1) or Logarithmic (expansion>1) Spiral path (A tool).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "add_graph_paper",
                            "description": "Adds a Graph Paper grid of columns x rows closed cells (D tool).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "copy_properties_from",
                            "description": "Copies fill, outline, and/or vector effects from source_id to selected objects (Edit -> Copy Properties From).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "export_pdf_imposed_byte_len",
                            "description": "Exports an N-Up imposed press sheet (rows x cols with margins, gutters, and crop marks) to PDF 1.4.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "add_connector",
                            "description": "Adds a straight or orthogonal (Manhattan Z-elbow) connector line with optional corner_radius.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "add_callout",
                            "description": "Adds a closed rectangular callout box with a pointer tail directed at (target_x, target_y).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "combine_selection",
                            "description": "Combines >= 2 selected objects into a single EvenOdd compound path (Ctrl+L).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "break_apart_selection",
                            "description": "Breaks apart compound paths into separate subpath objects (Ctrl+K).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "adjust_bitmap",
                            "description": "Applies brightness, contrast, BT.709 grayscale, and/or color inversion to a bitmap object.",
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
            "add_paragraph_text" => {
                let x = args.get("x").and_then(Value::as_f64).unwrap_or(72.0);
                let y = args.get("y").and_then(Value::as_f64).unwrap_or(72.0);
                let width = args.get("width").and_then(Value::as_f64).unwrap_or(240.0);
                let height = args.get("height").and_then(Value::as_f64).unwrap_or(120.0);
                let text = args.get("text").and_then(Value::as_str).unwrap_or("Paragraph text frame").to_owned();
                let font_family = args.get("font_family").and_then(Value::as_str).unwrap_or("Arial").to_owned();
                let size_pt = args.get("size_pt").and_then(Value::as_f64).unwrap_or(12.0);
                let line_spacing = args.get("line_spacing").and_then(Value::as_f64).unwrap_or(1.2);
                let columns = args.get("columns").and_then(Value::as_u64).unwrap_or(1) as u32;
                let gutter_pt = args.get("gutter_pt").and_then(Value::as_f64).unwrap_or(12.0);
                let alignment = match args.get("alignment").and_then(Value::as_str) {
                    Some("center") => TextAlignment::Center,
                    Some("right") => TextAlignment::Right,
                    Some("justify") => TextAlignment::Justify,
                    _ => TextAlignment::Left,
                };
                let position = Point::new(x, y).map_err(|e| e.to_string())?;
                let spec = ParagraphTextSpec::new(position, width, height, text, font_family, size_pt, line_spacing, alignment, columns, gutter_pt)
                    .map_err(|e| e.to_string())?;
                self.engine
                    .execute(EngineCommand::AddObject { name: Some("Paragraph Text".to_owned()), kind: ObjectKind::ParagraphText(Box::new(spec)) });
                Ok(json!({ "added": "paragraph_text", "selected_id": self.engine.selection.first().map(|id| id.0) }))
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
            "boolean_selection" => {
                self.apply_optional_selection(args);
                let op_str = args.get("op").and_then(Value::as_str).unwrap_or("weld");
                let op = match op_str {
                    "weld" => BooleanOp::Weld,
                    "trim" => BooleanOp::Trim,
                    "intersect" => BooleanOp::Intersect,
                    "front_minus_back" => BooleanOp::FrontMinusBack,
                    "back_minus_front" => BooleanOp::BackMinusFront,
                    other => return Err(format!("Unsupported boolean op: {other}")),
                };
                let ok = self.engine.execute(EngineCommand::BooleanSelection { op });
                let ids: Vec<u64> = self.engine.selection.iter().map(|id| id.0).collect();
                Ok(json!({ "boolean_executed": ok, "result_selection": ids }))
            }
            "apply_contour" => {
                let target = self.resolve_target_id(args)?;
                let steps = args.get("steps").and_then(Value::as_u64).unwrap_or(1) as u32;
                let offset_raw = args.get("offset_pt").and_then(Value::as_f64).unwrap_or(4.0);
                let offset_pt = F64::new(offset_raw).map_err(|e| e.to_string())?;
                let inside = args.get("inside").and_then(Value::as_bool).unwrap_or(false);
                let ok = self.engine.execute(EngineCommand::ApplyContour { object_id: target, steps, offset_pt, inside });
                Ok(json!({ "contour_applied": ok, "object_id": target.0 }))
            }
            "add_drop_shadow" => {
                let target = self.resolve_target_id(args)?;
                let ox = F64::new(args.get("offset_x").and_then(Value::as_f64).unwrap_or(5.0)).map_err(|e| e.to_string())?;
                let oy = F64::new(args.get("offset_y").and_then(Value::as_f64).unwrap_or(5.0)).map_err(|e| e.to_string())?;
                let opacity = F64::new(args.get("opacity").and_then(Value::as_f64).unwrap_or(0.5)).map_err(|e| e.to_string())?;
                let r = args.get("r").and_then(Value::as_u64).unwrap_or(0) as u8;
                let g = args.get("g").and_then(Value::as_u64).unwrap_or(0) as u8;
                let b = args.get("b").and_then(Value::as_u64).unwrap_or(0) as u8;
                let color = Color::rgb8(r, g, b);
                let ok = self.engine.execute(EngineCommand::AddDropShadow { object_id: target, offset_x: ox, offset_y: oy, opacity, color });
                Ok(json!({ "shadow_added": ok, "object_id": target.0 }))
            }
            "add_extrude" => {
                let target = self.resolve_target_id(args)?;
                let depth_pt = F64::new(args.get("depth_pt").and_then(Value::as_f64).unwrap_or(20.0)).map_err(|e| e.to_string())?;
                let angle_deg = F64::new(args.get("angle_deg").and_then(Value::as_f64).unwrap_or(45.0)).map_err(|e| e.to_string())?;
                let ok = self.engine.execute(EngineCommand::AddExtrude { object_id: target, depth_pt, angle_deg });
                Ok(json!({ "extrude_added": ok, "object_id": target.0 }))
            }
            "create_blend" => {
                let start_raw = args.get("start_id").and_then(Value::as_u64).ok_or_else(|| "Missing start_id".to_owned())?;
                let end_raw = args.get("end_id").and_then(Value::as_u64).ok_or_else(|| "Missing end_id".to_owned())?;
                let steps = args.get("steps").and_then(Value::as_u64).unwrap_or(5) as u32;
                let ok = self.engine.execute(EngineCommand::CreateBlend { start_id: ObjectId(start_raw), end_id: ObjectId(end_raw), steps });
                Ok(json!({ "blend_created": ok, "start_id": start_raw, "end_id": end_raw }))
            }
            "attach_blend_spine" => {
                let blend_raw = args.get("blend_object_id").and_then(Value::as_u64).ok_or_else(|| "Missing blend_object_id".to_owned())?;
                let spine_raw = args.get("spine_object_id").and_then(Value::as_u64).ok_or_else(|| "Missing spine_object_id".to_owned())?;
                let ok = self
                    .engine
                    .execute(EngineCommand::AttachBlendSpine { blend_object_id: ObjectId(blend_raw), spine_object_id: ObjectId(spine_raw) });
                Ok(json!({ "spine_attached": ok, "blend_object_id": blend_raw, "spine_object_id": spine_raw }))
            }
            "apply_envelope" => {
                let target = self.resolve_target_id(args)?;
                let tl_x = args.get("tl_x").and_then(Value::as_f64).unwrap_or(0.0);
                let tl_y = args.get("tl_y").and_then(Value::as_f64).unwrap_or(0.0);
                let tr_x = args.get("tr_x").and_then(Value::as_f64).unwrap_or(100.0);
                let tr_y = args.get("tr_y").and_then(Value::as_f64).unwrap_or(0.0);
                let br_x = args.get("br_x").and_then(Value::as_f64).unwrap_or(100.0);
                let br_y = args.get("br_y").and_then(Value::as_f64).unwrap_or(100.0);
                let bl_x = args.get("bl_x").and_then(Value::as_f64).unwrap_or(0.0);
                let bl_y = args.get("bl_y").and_then(Value::as_f64).unwrap_or(100.0);
                let corners = [
                    Point::new(tl_x, tl_y).map_err(|e| e.to_string())?,
                    Point::new(tr_x, tr_y).map_err(|e| e.to_string())?,
                    Point::new(br_x, br_y).map_err(|e| e.to_string())?,
                    Point::new(bl_x, bl_y).map_err(|e| e.to_string())?,
                ];
                let ok = self.engine.execute(EngineCommand::ApplyEnvelope { object_id: target, corners });
                Ok(json!({ "envelope_applied": ok, "object_id": target.0 }))
            }
            "apply_artistic_brush" => {
                let target = self.resolve_target_id(args)?;
                let start_w = F64::new(args.get("start_width_pt").and_then(Value::as_f64).unwrap_or(12.0)).map_err(|e| e.to_string())?;
                let end_w = F64::new(args.get("end_width_pt").and_then(Value::as_f64).unwrap_or(2.0)).map_err(|e| e.to_string())?;
                let ok = self.engine.execute(EngineCommand::ApplyArtisticBrush { object_id: target, start_width_pt: start_w, end_width_pt: end_w });
                Ok(json!({ "artistic_brush_applied": ok, "object_id": target.0 }))
            }
            "fit_text_to_path" => {
                let text_raw = args.get("text_object_id").and_then(Value::as_u64).ok_or_else(|| "Missing text_object_id".to_owned())?;
                let spine_raw = args.get("spine_object_id").and_then(Value::as_u64).ok_or_else(|| "Missing spine_object_id".to_owned())?;
                let offset_pt = F64::new(args.get("offset_pt").and_then(Value::as_f64).unwrap_or(0.0)).map_err(|e| e.to_string())?;
                let ok = self.engine.execute(EngineCommand::FitTextToPath {
                    text_object_id: ObjectId(text_raw),
                    spine_object_id: ObjectId(spine_raw),
                    offset_pt,
                });
                Ok(json!({ "text_fitted": ok, "text_object_id": text_raw, "spine_object_id": spine_raw }))
            }
            "step_and_repeat" => {
                if let Some(arr) = args.get("selection").and_then(Value::as_array) {
                    let ids: Vec<ObjectId> = arr.iter().filter_map(Value::as_u64).map(ObjectId).collect();
                    self.engine.set_selection(ids);
                }
                let count = args.get("count").and_then(Value::as_u64).unwrap_or(1) as u32;
                let ox = F64::new(args.get("offset_x_pt").and_then(Value::as_f64).unwrap_or(20.0)).map_err(|e| e.to_string())?;
                let oy = F64::new(args.get("offset_y_pt").and_then(Value::as_f64).unwrap_or(20.0)).map_err(|e| e.to_string())?;
                let ok = self.engine.execute(EngineCommand::StepAndRepeat { count, offset_x_pt: ox, offset_y_pt: oy });
                let ids: Vec<u64> = self.engine.selection.iter().map(|id| id.0).collect();
                Ok(json!({ "step_and_repeat_executed": ok, "created_selection": ids }))
            }
            "add_linear_dimension" => {
                let x1 = args.get("x1").and_then(Value::as_f64).unwrap_or(20.0);
                let y1 = args.get("y1").and_then(Value::as_f64).unwrap_or(50.0);
                let x2 = args.get("x2").and_then(Value::as_f64).unwrap_or(140.0);
                let y2 = args.get("y2").and_then(Value::as_f64).unwrap_or(50.0);
                let offset_pt = args.get("offset_pt").and_then(Value::as_f64).unwrap_or(16.0);
                let font_size_pt = args.get("font_size_pt").and_then(Value::as_f64).unwrap_or(10.0);
                let p1 = Point::new(x1, y1).map_err(|e| e.to_string())?;
                let p2 = Point::new(x2, y2).map_err(|e| e.to_string())?;
                let default_label = format!("{:.1} pt", p1.distance(p2));
                let label = args.get("label").and_then(Value::as_str).unwrap_or(&default_label);
                let path = coralcraft_geom::build_linear_dimension_path(p1, p2, offset_pt, label, font_size_pt).map_err(|e| e.to_string())?;
                self.engine.execute(EngineCommand::AddObject { name: Some("Linear Dimension".to_owned()), kind: ObjectKind::Path(path) });
                let sel = self.engine.selection.first().map(|id| id.0);
                Ok(json!({ "selected_id": sel, "label": label }))
            }
            "distribute_selection" => {
                if let Some(arr) = args.get("selection").and_then(Value::as_array) {
                    let ids: Vec<ObjectId> = arr.iter().filter_map(Value::as_u64).map(ObjectId).collect();
                    self.engine.set_selection(ids);
                }
                let mode_str = args.get("mode").and_then(Value::as_str).unwrap_or("spacing_horizontal");
                let mode = match mode_str {
                    "left" => coralcraft_tools::DistributeMode::Left,
                    "center_horizontal" => coralcraft_tools::DistributeMode::CenterHorizontal,
                    "right" => coralcraft_tools::DistributeMode::Right,
                    "spacing_horizontal" => coralcraft_tools::DistributeMode::SpacingHorizontal,
                    "top" => coralcraft_tools::DistributeMode::Top,
                    "center_vertical" => coralcraft_tools::DistributeMode::CenterVertical,
                    "bottom" => coralcraft_tools::DistributeMode::Bottom,
                    "spacing_vertical" => coralcraft_tools::DistributeMode::SpacingVertical,
                    other => return Err(format!("Unsupported distribute mode: {other}")),
                };
                let ok = self.engine.execute(EngineCommand::DistributeSelection { mode });
                Ok(json!({ "distributed": ok, "mode": mode_str }))
            }
            "apply_perspective" => {
                let target = self.resolve_target_id(args)?;
                let tl_x = args.get("tl_x").and_then(Value::as_f64).unwrap_or(0.0);
                let tl_y = args.get("tl_y").and_then(Value::as_f64).unwrap_or(0.0);
                let tr_x = args.get("tr_x").and_then(Value::as_f64).unwrap_or(100.0);
                let tr_y = args.get("tr_y").and_then(Value::as_f64).unwrap_or(0.0);
                let br_x = args.get("br_x").and_then(Value::as_f64).unwrap_or(100.0);
                let br_y = args.get("br_y").and_then(Value::as_f64).unwrap_or(100.0);
                let bl_x = args.get("bl_x").and_then(Value::as_f64).unwrap_or(0.0);
                let bl_y = args.get("bl_y").and_then(Value::as_f64).unwrap_or(100.0);
                let corners = [
                    Point::new(tl_x, tl_y).map_err(|e| e.to_string())?,
                    Point::new(tr_x, tr_y).map_err(|e| e.to_string())?,
                    Point::new(br_x, br_y).map_err(|e| e.to_string())?,
                    Point::new(bl_x, bl_y).map_err(|e| e.to_string())?,
                ];
                let ok = self.engine.execute(EngineCommand::ApplyPerspective { object_id: target, corners });
                Ok(json!({ "perspective_applied": ok, "object_id": target.0 }))
            }
            "fillet_path_corners" => {
                let target = self.resolve_target_id(args)?;
                let radius_pt = F64::new(args.get("radius_pt").and_then(Value::as_f64).unwrap_or(6.0)).map_err(|e| e.to_string())?;
                let corner_kind = match args.get("corner_kind").and_then(Value::as_str).unwrap_or("round") {
                    "scalloped" => CornerKind::Scalloped,
                    "chamfered" => CornerKind::Chamfered,
                    _ => CornerKind::Round,
                };
                let ok = self.engine.execute(EngineCommand::FilletPathCorners { object_id: target, radius_pt, corner_kind });
                Ok(json!({ "filleted": ok, "object_id": target.0 }))
            }
            "expand_effects" => {
                let target = self.resolve_target_id(args)?;
                let ok = self.engine.execute(EngineCommand::ExpandEffects { object_id: target });
                Ok(json!({ "effects_expanded": ok, "object_id": target.0 }))
            }
            "set_transparency" => {
                let mode = args.get("mode").and_then(Value::as_str).unwrap_or("uniform");
                let transparency = match mode {
                    "none" => None,
                    "fountain" => {
                        let angle = args.get("angle_deg").and_then(Value::as_f64).unwrap_or(0.0);
                        let start_op = args.get("start_opacity").and_then(Value::as_f64).unwrap_or(1.0);
                        let end_op = args.get("end_opacity").and_then(Value::as_f64).unwrap_or(0.0);
                        Some(TransparencyStyle::fountain_linear(angle, start_op, end_op))
                    }
                    _ => {
                        let op = args.get("opacity").and_then(Value::as_f64).unwrap_or(0.5);
                        Some(TransparencyStyle::uniform(op))
                    }
                };
                let ok = self.engine.execute(EngineCommand::SetTransparency { transparency });
                Ok(json!({ "transparency_set": ok, "mode": mode }))
            }
            "export_eps" => {
                let page_index = args.get("page_index").and_then(Value::as_u64).map_or_else(|| self.engine.current_page_index(), |idx| idx as usize);
                let eps = export_eps(&self.engine.document, page_index).map_err(|e| e.to_string())?;
                Ok(json!({ "eps": eps }))
            }
            "export_dxf" => {
                let page_index = args.get("page_index").and_then(Value::as_u64).map_or_else(|| self.engine.current_page_index(), |idx| idx as usize);
                let dxf = export_dxf(&self.engine.document, page_index).map_err(|e| e.to_string())?;
                Ok(json!({ "dxf": dxf }))
            }
            "import_svg" => {
                let svg = args.get("svg").and_then(Value::as_str).ok_or_else(|| "Missing svg string".to_owned())?;
                let doc = import_svg(svg).map_err(|e| e.to_string())?;
                let count = doc.pages[0].layers[0].objects.len();
                self.engine = EditorEngine::new(doc);
                Ok(json!({ "status": "ok", "imported_objects": count }))
            }
            "import_dxf" => {
                let dxf = args.get("dxf").and_then(Value::as_str).ok_or_else(|| "Missing dxf string".to_owned())?;
                let doc = import_dxf(dxf).map_err(|e| e.to_string())?;
                let count = doc.pages[0].layers[0].objects.len();
                self.engine = EditorEngine::new(doc);
                Ok(json!({ "status": "ok", "imported_objects": count }))
            }
            "add_spiral" => {
                let cx = args.get("cx").and_then(Value::as_f64).unwrap_or(150.0);
                let cy = args.get("cy").and_then(Value::as_f64).unwrap_or(150.0);
                let radius = args.get("outer_radius").and_then(Value::as_f64).unwrap_or(60.0);
                let revolutions = args.get("revolutions").and_then(Value::as_u64).unwrap_or(4) as u32;
                let expansion = args.get("expansion").and_then(Value::as_f64).unwrap_or(1.0);
                let center = Point::new(cx, cy).map_err(|e| e.to_string())?;
                let path = build_spiral_path(center, radius, revolutions, expansion).map_err(|e| e.to_string())?;
                self.engine.execute(EngineCommand::AddObject { name: Some("Spiral".to_owned()), kind: ObjectKind::Path(path) });
                Ok(json!({ "added": "spiral", "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "add_graph_paper" => {
                let x = args.get("x").and_then(Value::as_f64).unwrap_or(50.0);
                let y = args.get("y").and_then(Value::as_f64).unwrap_or(50.0);
                let w = args.get("width").and_then(Value::as_f64).unwrap_or(160.0);
                let h = args.get("height").and_then(Value::as_f64).unwrap_or(120.0);
                let cols = args.get("columns").and_then(Value::as_u64).unwrap_or(4) as u32;
                let rows = args.get("rows").and_then(Value::as_u64).unwrap_or(3) as u32;
                let rect = Rect::new(x, y, x + w, y + h).map_err(|e| e.to_string())?;
                let path = build_graph_paper_path(rect, cols, rows).map_err(|e| e.to_string())?;
                self.engine.execute(EngineCommand::AddObject { name: Some("Graph Paper".to_owned()), kind: ObjectKind::Path(path) });
                Ok(json!({ "added": "graph_paper", "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "copy_properties_from" => {
                if let Some(arr) = args.get("selection").and_then(Value::as_array) {
                    let ids: Vec<ObjectId> = arr.iter().filter_map(Value::as_u64).map(ObjectId).collect();
                    self.engine.set_selection(ids);
                }
                let src_raw = args.get("source_id").and_then(Value::as_u64).ok_or_else(|| "Missing source_id".to_owned())?;
                let copy_fill = args.get("copy_fill").and_then(Value::as_bool).unwrap_or(true);
                let copy_outline = args.get("copy_outline").and_then(Value::as_bool).unwrap_or(true);
                let copy_effects = args.get("copy_effects").and_then(Value::as_bool).unwrap_or(true);
                let ok =
                    self.engine.execute(EngineCommand::CopyPropertiesFrom { source_id: ObjectId(src_raw), copy_fill, copy_outline, copy_effects });
                Ok(json!({ "copied": ok, "source_id": src_raw }))
            }
            "export_pdf_imposed_byte_len" => {
                let sw = F64::new(args.get("sheet_width_pt").and_then(Value::as_f64).unwrap_or(612.0)).map_err(|e| e.to_string())?;
                let sh = F64::new(args.get("sheet_height_pt").and_then(Value::as_f64).unwrap_or(792.0)).map_err(|e| e.to_string())?;
                let rows = args.get("rows").and_then(Value::as_u64).unwrap_or(2) as u32;
                let cols = args.get("cols").and_then(Value::as_u64).unwrap_or(2) as u32;
                let margin = F64::new(args.get("margin_pt").and_then(Value::as_f64).unwrap_or(18.0)).map_err(|e| e.to_string())?;
                let gutter = F64::new(args.get("gutter_pt").and_then(Value::as_f64).unwrap_or(12.0)).map_err(|e| e.to_string())?;
                let crop_marks = args.get("crop_marks").and_then(Value::as_bool).unwrap_or(true);
                let spec = ImpositionSpec { sheet_width_pt: sw, sheet_height_pt: sh, rows, cols, margin_pt: margin, gutter_pt: gutter, crop_marks };
                let pdf = export_pdf_imposed(&self.engine.document, &spec).map_err(|e| e.to_string())?;
                Ok(json!({ "pdf_imposed_bytes": pdf.len() }))
            }
            "add_connector" => {
                let x1 = args.get("x1").and_then(Value::as_f64).unwrap_or(20.0);
                let y1 = args.get("y1").and_then(Value::as_f64).unwrap_or(20.0);
                let x2 = args.get("x2").and_then(Value::as_f64).unwrap_or(140.0);
                let y2 = args.get("y2").and_then(Value::as_f64).unwrap_or(100.0);
                let orthogonal = args.get("orthogonal").and_then(Value::as_bool).unwrap_or(true);
                let corner_radius = args.get("corner_radius").and_then(Value::as_f64).unwrap_or(0.0);
                let p1 = Point::new(x1, y1).map_err(|e| e.to_string())?;
                let p2 = Point::new(x2, y2).map_err(|e| e.to_string())?;
                let path = build_connector_path(p1, p2, orthogonal, corner_radius).map_err(|e| e.to_string())?;
                self.engine.execute(EngineCommand::AddObject { name: Some("Connector".to_owned()), kind: ObjectKind::Path(path) });
                Ok(json!({ "added": "connector", "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "add_callout" => {
                let x = args.get("x").and_then(Value::as_f64).unwrap_or(40.0);
                let y = args.get("y").and_then(Value::as_f64).unwrap_or(40.0);
                let w = args.get("width").and_then(Value::as_f64).unwrap_or(120.0);
                let h = args.get("height").and_then(Value::as_f64).unwrap_or(60.0);
                let tx = args.get("target_x").and_then(Value::as_f64).unwrap_or(100.0);
                let ty = args.get("target_y").and_then(Value::as_f64).unwrap_or(130.0);
                let tail_w = args.get("tail_width").and_then(Value::as_f64).unwrap_or(16.0);
                let rect = Rect::new(x, y, x + w, y + h).map_err(|e| e.to_string())?;
                let target_pt = Point::new(tx, ty).map_err(|e| e.to_string())?;
                let path = build_callout_path(rect, target_pt, tail_w).map_err(|e| e.to_string())?;
                self.engine.execute(EngineCommand::AddObject { name: Some("Callout".to_owned()), kind: ObjectKind::Path(path) });
                Ok(json!({ "added": "callout", "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "combine_selection" => {
                self.apply_optional_selection(args);
                let ok = self.engine.execute(EngineCommand::CombineSelection);
                Ok(json!({ "combined": ok, "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "break_apart_selection" => {
                self.apply_optional_selection(args);
                let ok = self.engine.execute(EngineCommand::BreakApartSelection);
                let ids: Vec<u64> = self.engine.selection.iter().map(|id| id.0).collect();
                Ok(json!({ "broken_apart": ok, "selected_ids": ids }))
            }
            "adjust_bitmap" => {
                let target = self.resolve_target_id(args)?;
                let brightness = F64::new(args.get("brightness").and_then(Value::as_f64).unwrap_or(0.0)).map_err(|e| e.to_string())?;
                let contrast = F64::new(args.get("contrast").and_then(Value::as_f64).unwrap_or(0.0)).map_err(|e| e.to_string())?;
                let grayscale = args.get("grayscale").and_then(Value::as_bool).unwrap_or(false);
                let invert = args.get("invert").and_then(Value::as_bool).unwrap_or(false);
                let ok = self.engine.execute(EngineCommand::AdjustBitmap { object_id: target, brightness, contrast, grayscale, invert });
                Ok(json!({ "bitmap_adjusted": ok, "object_id": target.0 }))
            }
            other => Err(format!("Unknown tool: {other}")),
        }
    }

    fn resolve_target_id(&self, args: &Value) -> Result<ObjectId, String> {
        if let Some(id_raw) = args.get("object_id").and_then(Value::as_u64) {
            Ok(ObjectId(id_raw))
        } else if let Some(first) = self.engine.selection.first() {
            Ok(*first)
        } else {
            Err("No object_id specified and selection is empty".to_owned())
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
        assert_eq!(tools, 50);

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
    }
}
