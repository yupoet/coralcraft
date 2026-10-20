//! Model Context Protocol (MCP) JSON-RPC 2.0 server (`coralcraft-mcp`, L6)
//! for agent-driven multi-page CorelDRAW illustration and vector export.
#![recursion_limit = "512"]

use coralcraft_cdr::{write_cdr7, write_cdrx_zip, write_cmx1};
use coralcraft_color::{Color, FountainBlendMode, FountainStop};
use coralcraft_doc::{
    ArrowheadKind, Document, FillStyle, FountainGeometry, LayerId, LensKind, LineCap, LineJoin, ObjectId, ObjectKind, OutlineStyle,
    ParagraphTextSpec, PatternKind, TextAlignment, TextCaseKind, TransparencyStyle, import_dxf, import_svg, load_bytes, save_json,
};
use coralcraft_engine::{ActivePageTarget, EditorEngine, EngineCommand, PaletteChannel};
use coralcraft_geom::{
    AxonometricPlane, BasicShapeKind, BooleanOp, CornerKind, DistortionKind, EllipseMode, EllipseSpec, F64, FillRule, FlowchartSymbolKind, Point,
    PolygonSpec, Rect, RectSpec, Unit, build_angular_dimension_path, build_banner_ribbon_path, build_barcode_path, build_basic_shape_path,
    build_block_arrow_path, build_bspline_path, build_callout_path, build_complex_star_path, build_connector_path, build_flowchart_symbol_path,
    build_gear_wheel_path, build_graph_paper_path, build_impact_rays_path, build_isometric_box_path, build_lsystem_path, build_perspective_grid_path,
    build_pointillizer_path, build_polyline_path, build_rose_guilloche_path, build_spiral_path, build_spirograph_path, build_superformula_path,
    build_three_point_curve_path, build_voronoi_mosaic_path,
};
use coralcraft_render::{ImpositionSpec, RenderOpts, base64_encode, export_dxf, export_eps, export_pdf, export_pdf_imposed, export_svg, render_page};
use coralcraft_tools::{AlignMode, PolygonSubMode, ZOrderOp, combined_selection_bounds};
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
                        },
                        {
                            "name": "apply_distortion",
                            "description": "Applies a non-destructive PushPull, Zipper, or Twister distortion effect to object_id.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "trace_bitmap",
                            "description": "Traces an ObjectKind::Bitmap into a closed vector Path (PowerTrace / Outline Trace).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "mirror_selection",
                            "description": "Mirrors selected objects horizontally and/or vertically around their center, optionally duplicating (copy=true).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "add_angular_dimension",
                            "description": "Adds an engineering angular dimension annotation measuring the angle at (vx, vy) between (x1, y1) and (x2, y2) with arc_radius_pt and label.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "create_boundary",
                            "description": "Creates a new closed union silhouette path around selected objects without deleting originals (Object -> Shaping -> Boundary).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "convert_outline_to_object",
                            "description": "Converts selected objects' outlines into closed filled vector paths (Object -> Convert Outline to Object / Ctrl+Shift+Q).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "set_outline_dash",
                            "description": "Sets outline width_pt and dash_pattern array on selected objects (F12 Outline Pen).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "set_fountain_fill",
                            "description": "Applies a Linear, Radial, Conical, or Square Fountain gradient fill to selected objects (F11 Fountain Fill).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "rotate_selection",
                            "description": "Rotates selected objects by angle_deg around any 9-point Object Origin (origin_index 0..=8), optionally duplicating (Alt+F8).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "scale_selection",
                            "description": "Scales selected objects by (scale_x, scale_y) around any 9-point Object Origin (origin_index 0..=8), optionally duplicating (Alt+F9).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "set_outline_arrows",
                            "description": "Sets start_arrow and end_arrow markers (none, triangle, open_arrow, circle, diamond) on selected objects (F12 Outline Pen).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "skew_selection",
                            "description": "Skews selected objects by (skew_x_deg, skew_y_deg) around any 9-point Object Origin (origin_index 0..=8), optionally duplicating (Alt+F11).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "duplicate_selection",
                            "description": "Duplicates selected objects by (offset_x_pt, offset_y_pt) and selects the copies (Ctrl+D).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "select_all",
                            "description": "Selects all unlocked visible objects on the active layer (Ctrl+A).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "apply_lens",
                            "description": "Applies a CorelDRAW Lens effect (invert, brighten, tinted, magnify) to selected objects (Alt+F3).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "reverse_path_direction",
                            "description": "Reverses the winding direction of all subpaths in an object (Shape Tool -> Reverse Subpaths).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "close_path_subpaths",
                            "description": "Closes all open subpaths in an object (Object -> Join Curves -> Auto-Close Curve).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "set_object_locked",
                            "description": "Locks or unlocks an object (Object -> Lock -> Lock / Unlock).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "set_pattern_fill",
                            "description": "Applies a Two-Color Pattern Fill (checkerboard, polka_dot, horizontal_stripe, vertical_stripe, diagonal_crosshatch) to selected objects.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "join_open_curves",
                            "description": "Joins open subpaths in an object whose endpoints lie within tolerance_pt (Object -> Join Curves / Ctrl+Shift+J).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "ungroup_all_selection",
                            "description": "Recursively ungroups all selected groups and nested subgroups down to leaf objects (Object -> Group -> Ungroup All).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "move_page",
                            "description": "Moves the 0-based page at from_index to to_index.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "reduce_path_nodes",
                            "description": "Simplifies path subpaths in an object using Ramer-Douglas-Peucker node reduction (Shape Tool -> Reduce Nodes).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "set_selection_bounds",
                            "description": "Sets exact position (x_pt, y_pt) and/or dimensions (width_pt, height_pt) of selected objects around any 9-point Object Origin (origin_index 0..=8).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "add_layer",
                            "description": "Adds a new normal layer with the given name on the active page (Object -> Layer -> New Layer).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "set_layer_properties",
                            "description": "Updates visible, locked, printable, and/or opacity on a layer (Objects Docker).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "add_bspline_curve",
                            "description": "Adds a smooth Catmull-Rom / B-Spline curve object passing through control points (B-Spline Tool).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "clear_transformations",
                            "description": "Resets the affine transformation matrix of selected objects to identity (Object -> Transformations -> Clear Transformations).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "toggle_text_kind",
                            "description": "Converts a text object between Artistic Text and Paragraph Text (Text -> Convert to Paragraph/Artistic Text / Ctrl+F8).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "find_and_replace_text",
                            "description": "Replaces all occurrences of find with replace across all unlocked text objects (Edit -> Find and Replace -> Replace Text).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "add_three_point_curve",
                            "description": "Adds a 3-Point Curve cubic Bezier arc passing through (through_x, through_y) at t = 0.5 between (x1, y1) and (x2, y2).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "change_text_case",
                            "description": "Changes the letter case of a text object (sentence, lowercase, uppercase, title, toggle) (Text -> Change Case / Shift+F3).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "insert_curve_node",
                            "description": "Subdivides a path segment at element_index on an object, inserting a new node at t = 0.5 (Shape Tool -> Add Nodes / +).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "delete_curve_node",
                            "description": "Deletes a path node at element_index on an object (Shape Tool -> Delete Nodes / -).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "convert_segment_to_line",
                            "description": "Converts the path segment at element_index on an object into a straight LineTo segment (Shape Tool -> To Line).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "convert_segment_to_curve",
                            "description": "Converts the path segment at element_index on an object into a CubicTo Bezier curve with 1/3 and 2/3 handles (Shape Tool -> To Curve).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "set_page_background",
                            "description": "Sets or clears the canvas background color on page_index (Layout -> Page Background).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "set_page_bleed",
                            "description": "Sets the print bleed margin in points on page_index (Layout -> Page Setup -> Bleed).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "add_pointillizer",
                            "description": "Adds a compound vector halftone dot mosaic inside a bounding box (Effects -> Pointillizer).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "set_object_opacity",
                            "description": "Updates the opacity (0.0..=1.0) of an object (Properties Docker).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "set_object_visible",
                            "description": "Toggles visibility (Object -> Hide / Show Object) of an object.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "rename_object",
                            "description": "Updates the user-assigned label of an object in the Objects Docker.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "add_impact_rays",
                            "description": "Adds a compound radial burst rays path around (cx, cy) from inner_radius to outer_radius (Impact Tool).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "delete_layer",
                            "description": "Deletes layer_id from its page when the page has more than one layer (Object -> Layer -> Delete Layer).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "move_layer",
                            "description": "Reorders the layer at from_index to to_index on page_index (Objects Docker).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "invert_selection",
                            "description": "Inverts the selection among unlocked, visible objects on the active target layer (Edit -> Select All -> Invert Selection).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "add_complex_star",
                            "description": "Adds a Complex Star {n/k} Schlafli star polygon around (cx, cy) with points and sharpness (Complex Star Tool).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "set_outline_properties",
                            "description": "Updates width_pt, behind_fill, and/or scale_with_object on selected objects' OutlineStyle (F12 Outline Pen).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "select_by_kind",
                            "description": "Selects all unlocked, visible objects on the active layer matching kind (rect, ellipse, polygon, path, text, bitmap, group).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "set_document_title",
                            "description": "Updates the document title metadata (File -> Document Properties).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "twirl_path",
                            "description": "Applies a localized rotational swirl around (cx, cy) within radius_pt on an object (Twirl Tool).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "clear_effects",
                            "description": "Removes all non-destructive vector effects from an object (Object -> Clear Effect).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "clone_effects_from",
                            "description": "Copies the non-destructive vector effect stack from source_id to selected objects (Effects -> Clone Effect).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "set_page_size",
                            "description": "Updates active page dimensions (width_pt, height_pt) and measurement units (mm, pt, inch, px) (Layout -> Page Setup).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "attract_repel_path",
                            "description": "Radially attracts (strength > 0) or repels (strength < 0) vertices within radius_pt of (cx, cy) on an object (Attract and Repel Tool).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "duplicate_page",
                            "description": "Duplicates the specified 0-based page (including its layers and objects) and activates the copy (Layout -> Duplicate Page).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "rename_page",
                            "description": "Renames the specified 0-based page tab (Layout -> Rename Page).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "delete_page",
                            "description": "Deletes the specified 0-based page when document.pages.len() > 1 (Layout -> Delete Page).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "smear_path",
                            "description": "Directionally drags vertices within radius_pt of (ox, oy) along (dx_pt, dy_pt) on an object (Smear Tool).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "set_paragraph_text_layout",
                            "description": "Updates columns, gutter_pt, line_spacing, and/or alignment on a ParagraphText frame (Text -> Columns / Paragraph Formatting).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "set_active_page_target",
                            "description": "Switches the active page tab (target = page with page_index, master, or desktop) in the Page Navigator Bar.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "inspect_object",
                            "description": "Returns detailed metadata (id, name, kind, visible, locked, opacity, bounds, effects_count) for an object.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "roughen_path",
                            "description": "Subdivides segments of an object with serrated zigzag normal teeth of amplitude tooth_height_pt and frequency (Roughen Tool).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "set_artistic_text_properties",
                            "description": "Updates text, font_family, and/or size_pt on an ArtisticText or ParagraphText object (Text -> Edit Text / Ctrl+Shift+T).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "list_page_objects",
                            "description": "Lists all objects on page_index with id, name, kind, layer_id, visible, locked, and opacity (Objects Docker).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "list_layers",
                            "description": "Lists all layers on page_index with id, name, role, visible, locked, printable, opacity, and object_count (Objects Docker).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "smooth_path",
                            "description": "Applies localized Jacobi Laplacian vertex smoothing within radius_pt around (center_x, center_y) with strength and iterations (Smooth Tool).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "update_rect_corners",
                            "description": "Updates corner_kind (round, scalloped, chamfered) and radii_pt on selected rectangles without converting them to curves (Property Bar).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "update_ellipse_parameters",
                            "description": "Updates ellipse mode (full, pie, arc) and start_deg/end_deg on selected ellipses without converting them to curves (Property Bar).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "list_pages",
                            "description": "Lists all pages in the document with index, id, name, width_pt, height_pt, bleed_pt, and layer_count (Pages Docker).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "knife_cut_object",
                            "description": "Slices an object along the cut line (start_x, start_y) -> (end_x, end_y) with kerf gap_pt (Knife Tool).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "crop_to_rect",
                            "description": "Crops unlocked selected objects (or all unlocked objects on the active layer) to the rectangle (x, y, width, height) (Crop Tool).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "update_polygon_parameters",
                            "description": "Updates polygon/star mode (polygon, star), points, and inner_radius_pt on selected polygons without converting them to curves (Property Bar).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "move_object_to_layer",
                            "description": "Moves object_id to target_layer_id on the active page (Objects Docker).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "erase_object_circle",
                            "description": "Subtracts a circular eraser nib of radius_pt centered at (center_x, center_y) from an object (Eraser Tool X).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "set_snap_settings",
                            "description": "Updates snap_to_objects and/or snap_to_page preferences (View -> Snap To).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "set_viewport",
                            "description": "Updates canvas zoom factor (0.05..=64.0) and pan offset (pan_x, pan_y) (Zoom / Pan Tool Z / H).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "set_active_tool",
                            "description": "Switches the active CorelDRAW toolbox tool (pick, shape, crop, zoom, freehand, rectangle, ellipse, polygon, artistic_text, interactive_fill).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "virtual_segment_delete",
                            "description": "Removes the path segment at element_index on an object, splitting the subpath at that segment (Virtual Segment Delete Tool).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "smart_fill_at_point",
                            "description": "Creates a new filled Path object from the enclosed region of overlapping closed shapes containing (x, y) (Smart Fill Tool).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "export_cmx1_byte_len",
                            "description": "Serializes the document to a clean-room RIFF CMX1 Exchange file and returns its byte length.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "export_cdrx_zip_byte_len",
                            "description": "Serializes the document to a modern CorelDRAW X4+ ZIP container (content/riffData.cdr) and returns its byte length.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "offset_object_path",
                            "description": "Offsets an object's path by distance_pt (Parallel Drawing / Offset Curve), either replacing in place or creating a parallel copy.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "set_path_fill_rule",
                            "description": "Updates the winding fill rule (non_zero or even_odd) of an object's path (Object Properties -> Fill).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "set_style_cmyk",
                            "description": "Sets fill or outline color in CMYK (c, m, y, k in 0.0..=1.0) on selected objects or creation defaults.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "set_style_hsb",
                            "description": "Sets fill or outline color in HSB (h_deg in 0..360, s and b in 0.0..=1.0) on selected objects or creation defaults.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "measure_object_geometry",
                            "description": "Computes total path length (length_pt) and signed/absolute area (area_sq_pt) of an object in world points (Object Properties -> Curve).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "swap_object_positions",
                            "description": "Swaps the bounding-box centers of two objects (first_id, second_id).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "center_selection_to_page",
                            "description": "Centers the current selection horizontally and/or vertically on the active page (Object -> Align and Distribute -> Center to Page / P).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "set_style_spot",
                            "description": "Sets fill or outline to a named Spot color (or Registration when name is 'Registration') with tint (0.0..=1.0) and CMYK fallback.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "smooth_curve_node",
                            "description": "Converts the node at element_index on an object into a smooth symmetrical C1 cubic Bezier node (Shape Tool F10 -> Smooth / Symmetrical Node).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "array_duplicate_selection",
                            "description": "Duplicates the current selection in a rows x cols 2D grid with (spacing_x_pt, spacing_y_pt) step offsets.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "add_guideline",
                            "description": "Adds a horizontal or vertical ruler guideline at position_pt on the active page's non-printing Guides layer.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "clear_guidelines",
                            "description": "Removes all guidelines from the active page's Guides layer.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "add_polyline",
                            "description": "Adds an open or closed multi-point polyline Path from points [[x,y], ...] (Freehand / Polyline Tool F5).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "circular_duplicate_selection",
                            "description": "Duplicates the current selection count times in a polar/radial circle around (center_x, center_y) over total_angle_deg.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "fit_page_to_selection",
                            "description": "Resizes the active page and translates page objects so the selection (or all unlocked visible objects) is framed with margin_pt.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "set_outline_cap_join",
                            "description": "Updates line cap (butt, round, square), line join (miter, round, bevel), and/or miter_limit on selected objects' OutlineStyle (F12 Outline Pen).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "add_gear_wheel",
                            "description": "Adds a parametric mechanical gear / cogwheel Path with teeth trapezoidal cogs and a concentric bore hole.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "align_curve_nodes",
                            "description": "Aligns the endpoints of element_indices on an object horizontally (same Y) and/or vertically (same X) to the first index (Shape Tool F10 -> Align Nodes).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "shift_selection_colors",
                            "description": "Shifts HSB hue (hue_delta_deg), saturation (saturation_delta), and brightness (brightness_delta) across all fills, fountain stops, patterns, and outlines of the selection.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "export_coralcraft_json",
                            "description": "Serializes the entire multi-page Document to lossless native .coralcraft JSON.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "add_block_arrow",
                            "description": "Adds a closed 7-vertex Block Arrow Path from (start_x, start_y) to (end_x, end_y) with configurable shaft and head dimensions.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "subdivide_path_segments",
                            "description": "Subdivides every segment of an object's path at its midpoint t = 0.5 (Shape Tool F10 -> Select All Nodes -> Add Nodes (+)).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "convert_selection_colors",
                            "description": "Converts all fills, fountain stops, two-color patterns, and outlines of the selection via sRGB inversion (invert) and/or BT.709 luminance grayscale (grayscale).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "import_coralcraft_json",
                            "description": "Loads a native .coralcraft JSON document string into the active EditorEngine.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "add_banner_ribbon",
                            "description": "Adds a compound 3-part heraldic Banner / Ribbon Path inside bounds with recessed side folds and V-shaped swallowtail notches.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "transform_curve_nodes",
                            "description": "Scales (scale_x, scale_y) and rotates (rotate_deg) the curve nodes at element_indices on an object around their centroid (Shape Tool F10).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "select_by_color",
                            "description": "Selects all unlocked visible objects on the active layer whose fill and/or outline color is within tolerance_u8 of (r, g, b).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "export_png_base64",
                            "description": "Rasterizes page_index at dpi (4x4 MSAA) to PNG and returns png_bytes length and RFC 4648 png_base64 string.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "add_flowchart_symbol",
                            "description": "Adds a closed Flowchart Symbol (process, decision, terminator, data, document) inside (x, y, width, height).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "break_path_at_node",
                            "description": "Breaks a curve at element_index on object_id, opening a closed subpath or splitting an open subpath into two (Shape Tool F10 -> Break Curve).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "equalize_selection_size",
                            "description": "Scales all selected unlocked objects around their centers to match the width and/or height of the last selected key object.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "export_pdf_base64",
                            "description": "Exports the entire multi-page document to ISO 32000-1 PDF 1.4 and returns pdf_bytes length and RFC 4648 pdf_base64 string.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "add_rose_guilloche",
                            "description": "Adds a closed Rhodonea / Guilloche Rose Curve at (cx, cy) with radius, petals_k (1..=36), and points_per_lobe (4..=64).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "morph_between_objects",
                            "description": "Topologically morphs start_id toward end_id at factor t (0.0..=1.0) with sample_count vertices and adds the resulting Path.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "distribute_exact_gap",
                            "description": "Spaces all selected unlocked objects along the horizontal or vertical axis with an exact gap_pt between adjacent bounding boxes.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "extract_document_palette",
                            "description": "Scans all objects in the document and returns the deduplicated list of colors (hex, r, g, b) used in fills, fountain stops, patterns, and outlines.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "add_barcode",
                            "description": "Adds a compound vector Barcode Path inside (x, y, width, height) encoding ASCII digits with start, center, and end guard bars.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "divide_selection",
                            "description": "Splits two selected overlapping closed objects into non-overlapping regions (A \\ B, A ∩ B, B \\ A) (Object -> Shaping -> Divide).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "distribute_selection_along_path",
                            "description": "Positions all selected unlocked objects other than spine_object_id evenly along the normalized arc length of spine_object_id.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "recolor_artwork",
                            "description": "Globally replaces colors matching (from_r, from_g, from_b) within tolerance_u8 with (to_r, to_g, to_b) across fills, gradients, patterns, and outlines.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "add_perspective_grid",
                            "description": "Adds a 2-Point Perspective Grid Path inside (x, y, width, height) with horizon_y_pt and subdivisions (2..=32).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "hatch_object_path",
                            "description": "Replaces object_id with parallel and optional cross-hatch vector engraving strokes at angle_deg and spacing_pt.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "create_intermediate_blend_paths",
                            "description": "Generates steps (1..=32) standalone intermediate morphed vector Path objects between start_id and end_id.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "export_svg_base64",
                            "description": "Exports page_index to clean-room SVG 1.1 and returns svg_bytes length and RFC 4648 svg_base64 string.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "add_superformula_shape",
                            "description": "Adds a closed Gielis Superformula Path centered at (cx, cy) with radius, symmetry m, exponents (n1, n2, n3), and steps.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "project_selection_axonometric",
                            "description": "Projects all selected unlocked objects onto one of the 30-degree isometric planes (top, left, right).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "pack_selection_in_grid",
                            "description": "Packs all selected unlocked objects into a multi-column grid starting at (origin_x, origin_y) with cols columns and gap_pt spacing.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "export_eps_base64",
                            "description": "Exports page_index to clean-room EPSF-3.0 Encapsulated PostScript and returns eps_bytes length and RFC 4648 eps_base64 string.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "add_basic_shape",
                            "description": "Adds a closed CorelDRAW Basic Shape Path (heart, cross, lightning, crescent_moon, teardrop, diamond) inside (x, y, width, height).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "add_lsystem_path",
                            "description": "Adds a procedural Lindenmayer System (L-System) fractal or organic coral/branching Path starting at (origin_x, origin_y).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "exclude_selection",
                            "description": "Computes the symmetric difference (XOR / Exclude) of >= 2 selected unlocked objects with EvenOdd fill rule.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "resolve_selection_overlaps",
                            "description": "Iteratively separates overlapping selected objects by force-directed AABB repulsion to maintain min_gap_pt clearance.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "add_voronoi_mosaic",
                            "description": "Adds a bounded Voronoi stained-glass/mosaic compound path inside a rectangle with optional mortar cell_gap_pt.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "apply_variable_width_stroke",
                            "description": "Expands an object's spine using a piecewise-linear variable-width profile (widths_pt array) into a closed filled ribbon path.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "chaikin_smooth_path",
                            "description": "Smooths polygonal or curve vertices of an object via Chaikin corner-cutting (1/4 and 3/4 subdivision) for iterations (1..=6).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "export_dxf_base64",
                            "description": "Exports a page to AutoCAD DXF (AC1009) and returns its byte length and base64-encoded payload.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "add_isometric_box",
                            "description": "Adds a parametric 3D isometric box compound path (Top, Left, and Right closed faces) centered at (cx, cy).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "wave_distort_path",
                            "description": "Applies a sinusoidal wave distortion along the local normals of an object's path.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "radial_scale_selection_positions",
                            "description": "Scales the radial distance of selected objects from a center point and rotates their angular positions by twist_deg.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "export_cdr7_base64",
                            "description": "Exports the document to RIFF CDR7 binary format and returns its byte length and base64-encoded payload.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "add_spirograph",
                            "description": "Adds a parametric hypotrochoid or epitrochoid spirograph roulette path centered at (cx, cy).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "apply_dihedral_symmetry",
                            "description": "Replaces an object with a Cn rotational or Dn dihedral radial kaleidoscope symmetry compound path around (center_x, center_y).",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "scale_selection_by_distance",
                            "description": "Scales each selected object around its own center based on its radial distance from an attractor center within falloff_radius_pt.",
                            "inputSchema": { "type": "object" }
                        },
                        {
                            "name": "export_cmx1_base64",
                            "description": "Exports the document to Corel Presentation Exchange (RIFF CMX1) binary format and returns its byte length and base64-encoded payload.",
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
                    "exclude" | "xor" => BooleanOp::Exclude,
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
            "apply_distortion" => {
                let target = self.resolve_target_id(args)?;
                let kind = match args.get("kind").and_then(Value::as_str).unwrap_or("twister") {
                    "push_pull" => DistortionKind::PushPull,
                    "zipper" => DistortionKind::Zipper,
                    _ => DistortionKind::Twister,
                };
                let amplitude = F64::new(args.get("amplitude").and_then(Value::as_f64).unwrap_or(30.0)).map_err(|e| e.to_string())?;
                let frequency = F64::new(args.get("frequency").and_then(Value::as_f64).unwrap_or(4.0)).map_err(|e| e.to_string())?;
                let ok = self.engine.execute(EngineCommand::ApplyDistortion { object_id: target, kind, amplitude, frequency });
                Ok(json!({ "distortion_applied": ok, "object_id": target.0 }))
            }
            "trace_bitmap" => {
                let target = self.resolve_target_id(args)?;
                let threshold = args.get("threshold").and_then(Value::as_u64).unwrap_or(128).min(255) as u8;
                let ok = self.engine.execute(EngineCommand::TraceBitmap { object_id: target, threshold });
                Ok(json!({ "bitmap_traced": ok, "object_id": target.0 }))
            }
            "mirror_selection" => {
                self.apply_optional_selection(args);
                let horizontal = args.get("horizontal").and_then(Value::as_bool).unwrap_or(true);
                let vertical = args.get("vertical").and_then(Value::as_bool).unwrap_or(false);
                let copy = args.get("copy").and_then(Value::as_bool).unwrap_or(false);
                let ok = self.engine.execute(EngineCommand::MirrorSelection { horizontal, vertical, copy });
                let ids: Vec<u64> = self.engine.selection.iter().map(|id| id.0).collect();
                Ok(json!({ "mirrored": ok, "selected_ids": ids }))
            }
            "add_angular_dimension" => {
                let vx = args.get("vx").and_then(Value::as_f64).unwrap_or(100.0);
                let vy = args.get("vy").and_then(Value::as_f64).unwrap_or(100.0);
                let x1 = args.get("x1").and_then(Value::as_f64).unwrap_or(180.0);
                let y1 = args.get("y1").and_then(Value::as_f64).unwrap_or(100.0);
                let x2 = args.get("x2").and_then(Value::as_f64).unwrap_or(100.0);
                let y2 = args.get("y2").and_then(Value::as_f64).unwrap_or(180.0);
                let arc_radius_pt = args.get("arc_radius_pt").and_then(Value::as_f64).unwrap_or(40.0);
                let label = args.get("label").and_then(Value::as_str).unwrap_or("90 deg");
                let font_size_pt = args.get("font_size_pt").and_then(Value::as_f64).unwrap_or(10.0);
                let vertex = Point::new(vx, vy).map_err(|e| e.to_string())?;
                let ray1 = Point::new(x1, y1).map_err(|e| e.to_string())?;
                let ray2 = Point::new(x2, y2).map_err(|e| e.to_string())?;
                let path = build_angular_dimension_path(vertex, ray1, ray2, arc_radius_pt, label, font_size_pt).map_err(|e| e.to_string())?;
                self.engine.execute(EngineCommand::AddObject { name: Some("Angular Dimension".to_owned()), kind: ObjectKind::Path(path) });
                Ok(json!({ "added": "angular_dimension", "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "create_boundary" => {
                self.apply_optional_selection(args);
                let ok = self.engine.execute(EngineCommand::CreateBoundary);
                Ok(json!({ "boundary_created": ok, "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "convert_outline_to_object" => {
                self.apply_optional_selection(args);
                let ok = self.engine.execute(EngineCommand::ConvertOutlineToObject);
                let ids: Vec<u64> = self.engine.selection.iter().map(|id| id.0).collect();
                Ok(json!({ "converted_outline": ok, "selected_ids": ids }))
            }
            "set_outline_dash" => {
                self.apply_optional_selection(args);
                let width_pt = F64::new(args.get("width_pt").and_then(Value::as_f64).unwrap_or(2.0)).map_err(|e| e.to_string())?;
                let r = args.get("r").and_then(Value::as_u64).unwrap_or(0).min(255) as u8;
                let g = args.get("g").and_then(Value::as_u64).unwrap_or(0).min(255) as u8;
                let b = args.get("b").and_then(Value::as_u64).unwrap_or(0).min(255) as u8;
                let mut dash_pattern = Vec::new();
                if let Some(arr) = args.get("dash_pattern").and_then(Value::as_array) {
                    for v in arr {
                        if let Some(f) = v.as_f64()
                            && let Ok(val) = F64::new(f)
                        {
                            dash_pattern.push(val);
                        }
                    }
                }
                let mut outline = OutlineStyle::with_width(Color::rgb8(r, g, b), width_pt);
                outline.dash_pattern = dash_pattern;
                let ok = self.engine.execute(EngineCommand::SetOutline { outline: Some(outline) });
                Ok(json!({ "outline_set": ok }))
            }
            "set_fountain_fill" => {
                self.apply_optional_selection(args);
                let angle_deg = F64::new(args.get("angle_deg").and_then(Value::as_f64).unwrap_or(0.0)).map_err(|e| e.to_string())?;
                let geometry = match args.get("type").and_then(Value::as_str).unwrap_or("linear") {
                    "radial" => FountainGeometry::Radial {
                        center_x: F64::HALF,
                        center_y: F64::HALF,
                        radius: F64::HALF,
                        focal_x: F64::HALF,
                        focal_y: F64::HALF,
                    },
                    "conical" => FountainGeometry::Conical { center_x: F64::HALF, center_y: F64::HALF, angle_deg },
                    "square" => FountainGeometry::Square { center_x: F64::HALF, center_y: F64::HALF, radius: F64::HALF, angle_deg },
                    _ => FountainGeometry::Linear { angle_deg, edge_pad: F64::ZERO },
                };
                let r1 = args.get("start_r").and_then(Value::as_u64).unwrap_or(255).min(255) as u8;
                let g1 = args.get("start_g").and_then(Value::as_u64).unwrap_or(0).min(255) as u8;
                let b1 = args.get("start_b").and_then(Value::as_u64).unwrap_or(0).min(255) as u8;
                let r2 = args.get("end_r").and_then(Value::as_u64).unwrap_or(0).min(255) as u8;
                let g2 = args.get("end_g").and_then(Value::as_u64).unwrap_or(100).min(255) as u8;
                let b2 = args.get("end_b").and_then(Value::as_u64).unwrap_or(255).min(255) as u8;
                let stops = vec![
                    FountainStop::new(0.0, Color::rgb8(r1, g1, b1)).map_err(|e| e.to_string())?,
                    FountainStop::new(1.0, Color::rgb8(r2, g2, b2)).map_err(|e| e.to_string())?,
                ];
                let fill = FillStyle::Fountain { geometry, blend_mode: FountainBlendMode::Direct, stops };
                let ok = self.engine.execute(EngineCommand::SetFill { fill: Some(fill) });
                Ok(json!({ "fountain_fill_set": ok }))
            }
            "rotate_selection" => {
                self.apply_optional_selection(args);
                let angle_deg = F64::new(args.get("angle_deg").and_then(Value::as_f64).unwrap_or(45.0)).map_err(|e| e.to_string())?;
                let origin_index = args.get("origin_index").and_then(Value::as_u64).unwrap_or(4).min(8) as u8;
                let copy = args.get("copy").and_then(Value::as_bool).unwrap_or(false);
                let ok = self.engine.execute(EngineCommand::RotateSelection { angle_deg, origin_index, copy });
                let ids: Vec<u64> = self.engine.selection.iter().map(|id| id.0).collect();
                Ok(json!({ "rotated": ok, "selected_ids": ids }))
            }
            "scale_selection" => {
                self.apply_optional_selection(args);
                let scale_x = F64::new(args.get("scale_x").and_then(Value::as_f64).unwrap_or(1.5)).map_err(|e| e.to_string())?;
                let scale_y = F64::new(args.get("scale_y").and_then(Value::as_f64).unwrap_or(1.5)).map_err(|e| e.to_string())?;
                let origin_index = args.get("origin_index").and_then(Value::as_u64).unwrap_or(4).min(8) as u8;
                let copy = args.get("copy").and_then(Value::as_bool).unwrap_or(false);
                let ok = self.engine.execute(EngineCommand::ScaleSelection { scale_x, scale_y, origin_index, copy });
                let ids: Vec<u64> = self.engine.selection.iter().map(|id| id.0).collect();
                Ok(json!({ "scaled": ok, "selected_ids": ids }))
            }
            "set_outline_arrows" => {
                self.apply_optional_selection(args);
                let parse_arrow = |s: &str| match s {
                    "triangle" => ArrowheadKind::Triangle,
                    "open_arrow" => ArrowheadKind::OpenArrow,
                    "circle" => ArrowheadKind::Circle,
                    "diamond" => ArrowheadKind::Diamond,
                    _ => ArrowheadKind::None,
                };
                let start_arrow = parse_arrow(args.get("start_arrow").and_then(Value::as_str).unwrap_or("none"));
                let end_arrow = parse_arrow(args.get("end_arrow").and_then(Value::as_str).unwrap_or("triangle"));
                let width_pt = F64::new(args.get("width_pt").and_then(Value::as_f64).unwrap_or(2.0)).map_err(|e| e.to_string())?;
                let mut outline = OutlineStyle::with_width(Color::BLACK, width_pt);
                outline.start_arrow = start_arrow;
                outline.end_arrow = end_arrow;
                let ok = self.engine.execute(EngineCommand::SetOutline { outline: Some(outline) });
                Ok(json!({ "outline_arrows_set": ok }))
            }
            "skew_selection" => {
                self.apply_optional_selection(args);
                let skew_x_deg = F64::new(args.get("skew_x_deg").and_then(Value::as_f64).unwrap_or(15.0)).map_err(|e| e.to_string())?;
                let skew_y_deg = F64::new(args.get("skew_y_deg").and_then(Value::as_f64).unwrap_or(0.0)).map_err(|e| e.to_string())?;
                let origin_index = args.get("origin_index").and_then(Value::as_u64).unwrap_or(4).min(8) as u8;
                let copy = args.get("copy").and_then(Value::as_bool).unwrap_or(false);
                let ok = self.engine.execute(EngineCommand::SkewSelection { skew_x_deg, skew_y_deg, origin_index, copy });
                let ids: Vec<u64> = self.engine.selection.iter().map(|id| id.0).collect();
                Ok(json!({ "skewed": ok, "selected_ids": ids }))
            }
            "duplicate_selection" => {
                self.apply_optional_selection(args);
                let offset_x_pt = F64::new(args.get("offset_x_pt").and_then(Value::as_f64).unwrap_or(18.0)).map_err(|e| e.to_string())?;
                let offset_y_pt = F64::new(args.get("offset_y_pt").and_then(Value::as_f64).unwrap_or(18.0)).map_err(|e| e.to_string())?;
                let ok = self.engine.execute(EngineCommand::DuplicateSelection { offset_x_pt, offset_y_pt });
                let ids: Vec<u64> = self.engine.selection.iter().map(|id| id.0).collect();
                Ok(json!({ "duplicated": ok, "selected_ids": ids }))
            }
            "select_all" => {
                self.engine.select_all();
                let ids: Vec<u64> = self.engine.selection.iter().map(|id| id.0).collect();
                Ok(json!({ "selected_ids": ids, "count": ids.len() }))
            }
            "apply_lens" => {
                self.apply_optional_selection(args);
                let kind = match args.get("kind").and_then(Value::as_str).unwrap_or("invert") {
                    "brighten" => {
                        let rate = F64::new(args.get("rate").and_then(Value::as_f64).unwrap_or(0.5)).map_err(|e| e.to_string())?;
                        LensKind::Brighten { rate }
                    }
                    "tinted" => {
                        let rate = F64::new(args.get("rate").and_then(Value::as_f64).unwrap_or(0.5)).map_err(|e| e.to_string())?;
                        let r = args.get("r").and_then(Value::as_u64).unwrap_or(255).min(255) as u8;
                        let g = args.get("g").and_then(Value::as_u64).unwrap_or(200).min(255) as u8;
                        let b = args.get("b").and_then(Value::as_u64).unwrap_or(0).min(255) as u8;
                        LensKind::Tinted { color: Color::rgb8(r, g, b), rate }
                    }
                    "magnify" => {
                        let factor = F64::new(args.get("factor").and_then(Value::as_f64).unwrap_or(2.0)).map_err(|e| e.to_string())?;
                        LensKind::Magnify { factor }
                    }
                    _ => LensKind::Invert,
                };
                let ok = self.engine.execute(EngineCommand::SetTransparency { transparency: Some(TransparencyStyle::Lens { kind }) });
                Ok(json!({ "lens_applied": ok }))
            }
            "reverse_path_direction" => {
                let target = self.resolve_target_id(args)?;
                let ok = self.engine.execute(EngineCommand::ReversePathDirection { object_id: target });
                Ok(json!({ "reversed": ok, "object_id": target.0 }))
            }
            "close_path_subpaths" => {
                let target = self.resolve_target_id(args)?;
                let ok = self.engine.execute(EngineCommand::CloseOpenSubpaths { object_id: target });
                Ok(json!({ "closed": ok, "object_id": target.0 }))
            }
            "set_object_locked" => {
                let target = self.resolve_target_id(args)?;
                let locked = args.get("locked").and_then(Value::as_bool).unwrap_or(true);
                let ok = self.engine.execute(EngineCommand::SetObjectLocked { object_id: target, locked });
                Ok(json!({ "lock_updated": ok, "object_id": target.0, "locked": locked }))
            }
            "set_pattern_fill" => {
                self.apply_optional_selection(args);
                let kind = match args.get("pattern").and_then(Value::as_str).unwrap_or("checkerboard") {
                    "polka_dot" => PatternKind::PolkaDot,
                    "horizontal_stripe" => PatternKind::HorizontalStripe,
                    "vertical_stripe" => PatternKind::VerticalStripe,
                    "diagonal_crosshatch" => PatternKind::DiagonalCrosshatch,
                    _ => PatternKind::Checkerboard,
                };
                let fg_r = args.get("fg_r").and_then(Value::as_u64).unwrap_or(0).min(255) as u8;
                let fg_g = args.get("fg_g").and_then(Value::as_u64).unwrap_or(0).min(255) as u8;
                let fg_b = args.get("fg_b").and_then(Value::as_u64).unwrap_or(0).min(255) as u8;
                let bg_r = args.get("bg_r").and_then(Value::as_u64).unwrap_or(255).min(255) as u8;
                let bg_g = args.get("bg_g").and_then(Value::as_u64).unwrap_or(255).min(255) as u8;
                let bg_b = args.get("bg_b").and_then(Value::as_u64).unwrap_or(255).min(255) as u8;
                let tile_size_pt = F64::new(args.get("tile_size_pt").and_then(Value::as_f64).unwrap_or(16.0)).map_err(|e| e.to_string())?;
                let fill = FillStyle::TwoColorPattern {
                    kind,
                    fg_color: Color::rgb8(fg_r, fg_g, fg_b),
                    bg_color: Color::rgb8(bg_r, bg_g, bg_b),
                    tile_size_pt,
                };
                let ok = self.engine.execute(EngineCommand::SetFill { fill: Some(fill) });
                Ok(json!({ "pattern_fill_set": ok }))
            }
            "join_open_curves" => {
                let target = self.resolve_target_id(args)?;
                let tolerance_pt = F64::new(args.get("tolerance_pt").and_then(Value::as_f64).unwrap_or(5.0)).map_err(|e| e.to_string())?;
                let ok = self.engine.execute(EngineCommand::JoinOpenSubpaths { object_id: target, tolerance_pt });
                Ok(json!({ "joined": ok, "object_id": target.0 }))
            }
            "ungroup_all_selection" => {
                self.apply_optional_selection(args);
                let ok = self.engine.execute(EngineCommand::UngroupAllSelection);
                let ids: Vec<u64> = self.engine.selection.iter().map(|id| id.0).collect();
                Ok(json!({ "ungrouped_all": ok, "selected_ids": ids }))
            }
            "move_page" => {
                let from_index = args.get("from_index").and_then(Value::as_u64).unwrap_or(0) as usize;
                let to_index = args.get("to_index").and_then(Value::as_u64).unwrap_or(0) as usize;
                let ok = self.engine.execute(EngineCommand::MovePage { from_index, to_index });
                Ok(json!({ "page_moved": ok, "pages": self.engine.document.pages.len(), "active_page_index": self.engine.current_page_index() }))
            }
            "reduce_path_nodes" => {
                let target = self.resolve_target_id(args)?;
                let tolerance_pt = F64::new(args.get("tolerance_pt").and_then(Value::as_f64).unwrap_or(1.5)).map_err(|e| e.to_string())?;
                let ok = self.engine.execute(EngineCommand::ReducePathNodes { object_id: target, tolerance_pt });
                Ok(json!({ "nodes_reduced": ok, "object_id": target.0 }))
            }
            "set_selection_bounds" => {
                self.apply_optional_selection(args);
                let x_pt = args.get("x_pt").and_then(Value::as_f64).map(F64::new).transpose().map_err(|e| e.to_string())?;
                let y_pt = args.get("y_pt").and_then(Value::as_f64).map(F64::new).transpose().map_err(|e| e.to_string())?;
                let width_pt = args.get("width_pt").and_then(Value::as_f64).map(F64::new).transpose().map_err(|e| e.to_string())?;
                let height_pt = args.get("height_pt").and_then(Value::as_f64).map(F64::new).transpose().map_err(|e| e.to_string())?;
                let origin_index = args.get("origin_index").and_then(Value::as_u64).unwrap_or(4).min(8) as u8;
                let ok = self.engine.execute(EngineCommand::SetSelectionBounds { x_pt, y_pt, width_pt, height_pt, origin_index });
                Ok(json!({ "bounds_set": ok }))
            }
            "add_layer" => {
                let name = args.get("name").and_then(Value::as_str).unwrap_or("Layer 2").to_owned();
                let ok = self.engine.execute(EngineCommand::AddLayer { name });
                let page_idx = self.engine.current_page_index();
                let layer_id = self.engine.document.pages.get(page_idx).and_then(|p| p.layers.last()).map(|l| l.id.0);
                Ok(json!({ "layer_added": ok, "layer_id": layer_id }))
            }
            "set_layer_properties" => {
                let layer_raw = args.get("layer_id").and_then(Value::as_u64).unwrap_or(2);
                let visible = args.get("visible").and_then(Value::as_bool);
                let locked = args.get("locked").and_then(Value::as_bool);
                let printable = args.get("printable").and_then(Value::as_bool);
                let opacity = args.get("opacity").and_then(Value::as_f64).map(F64::new).transpose().map_err(|e| e.to_string())?;
                let ok = self.engine.execute(EngineCommand::SetLayerProperties { layer_id: LayerId(layer_raw), visible, locked, printable, opacity });
                Ok(json!({ "layer_updated": ok, "layer_id": layer_raw }))
            }
            "add_bspline_curve" => {
                let closed = args.get("closed").and_then(Value::as_bool).unwrap_or(false);
                let mut pts = Vec::new();
                if let Some(arr) = args.get("points").and_then(Value::as_array) {
                    for item in arr {
                        let x = item.get("x").and_then(Value::as_f64).unwrap_or(0.0);
                        let y = item.get("y").and_then(Value::as_f64).unwrap_or(0.0);
                        pts.push(Point::new(x, y).map_err(|e| e.to_string())?);
                    }
                }
                if pts.len() < 2 {
                    pts = vec![
                        Point::new(40.0, 100.0).map_err(|e| e.to_string())?,
                        Point::new(100.0, 40.0).map_err(|e| e.to_string())?,
                        Point::new(160.0, 100.0).map_err(|e| e.to_string())?,
                    ];
                }
                let path = build_bspline_path(&pts, closed).map_err(|e| e.to_string())?;
                self.engine.execute(EngineCommand::AddObject { name: Some("B-Spline".to_owned()), kind: ObjectKind::Path(path) });
                Ok(json!({ "added": "bspline_curve", "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "clear_transformations" => {
                self.apply_optional_selection(args);
                let ok = self.engine.execute(EngineCommand::ClearTransformations);
                Ok(json!({ "transformations_cleared": ok }))
            }
            "toggle_text_kind" => {
                let target = self.resolve_target_id(args)?;
                let ok = self.engine.execute(EngineCommand::ToggleTextKind { object_id: target });
                Ok(json!({ "text_kind_toggled": ok, "object_id": target.0 }))
            }
            "find_and_replace_text" => {
                let find = args.get("find").and_then(Value::as_str).unwrap_or_default().to_owned();
                let replace = args.get("replace").and_then(Value::as_str).unwrap_or_default().to_owned();
                let ok = self.engine.execute(EngineCommand::FindAndReplaceText { find, replace });
                Ok(json!({ "text_replaced": ok }))
            }
            "add_three_point_curve" => {
                let x1 = args.get("x1").and_then(Value::as_f64).unwrap_or(20.0);
                let y1 = args.get("y1").and_then(Value::as_f64).unwrap_or(100.0);
                let x2 = args.get("x2").and_then(Value::as_f64).unwrap_or(180.0);
                let y2 = args.get("y2").and_then(Value::as_f64).unwrap_or(100.0);
                let tx = args.get("through_x").and_then(Value::as_f64).unwrap_or(100.0);
                let ty = args.get("through_y").and_then(Value::as_f64).unwrap_or(30.0);
                let start = Point::new(x1, y1).map_err(|e| e.to_string())?;
                let end = Point::new(x2, y2).map_err(|e| e.to_string())?;
                let through = Point::new(tx, ty).map_err(|e| e.to_string())?;
                let path = build_three_point_curve_path(start, end, through).map_err(|e| e.to_string())?;
                self.engine.execute(EngineCommand::AddObject { name: Some("3-Point Curve".to_owned()), kind: ObjectKind::Path(path) });
                Ok(json!({ "added": "three_point_curve", "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "change_text_case" => {
                let target = self.resolve_target_id(args)?;
                let case_kind = match args.get("case").and_then(Value::as_str).unwrap_or("uppercase") {
                    "sentence" => TextCaseKind::SentenceCase,
                    "lowercase" => TextCaseKind::Lowercase,
                    "title" => TextCaseKind::TitleCase,
                    "toggle" => TextCaseKind::ToggleCase,
                    _ => TextCaseKind::Uppercase,
                };
                let ok = self.engine.execute(EngineCommand::ChangeTextCase { object_id: target, case_kind });
                Ok(json!({ "case_changed": ok, "object_id": target.0 }))
            }
            "insert_curve_node" => {
                let target = self.resolve_target_id(args)?;
                let element_index = args.get("element_index").and_then(Value::as_u64).unwrap_or(1) as usize;
                let ok = self.engine.execute(EngineCommand::InsertCurveNode { object_id: target, element_index });
                Ok(json!({ "node_inserted": ok, "object_id": target.0 }))
            }
            "delete_curve_node" => {
                let target = self.resolve_target_id(args)?;
                let element_index = args.get("element_index").and_then(Value::as_u64).unwrap_or(1) as usize;
                let ok = self.engine.execute(EngineCommand::DeleteCurveNode { object_id: target, element_index });
                Ok(json!({ "node_deleted": ok, "object_id": target.0 }))
            }
            "convert_segment_to_line" => {
                let target = self.resolve_target_id(args)?;
                let element_index = args.get("element_index").and_then(Value::as_u64).unwrap_or(1) as usize;
                let ok = self.engine.execute(EngineCommand::ConvertSegmentToLine { object_id: target, element_index });
                Ok(json!({ "segment_converted_to_line": ok, "object_id": target.0 }))
            }
            "convert_segment_to_curve" => {
                let target = self.resolve_target_id(args)?;
                let element_index = args.get("element_index").and_then(Value::as_u64).unwrap_or(1) as usize;
                let ok = self.engine.execute(EngineCommand::ConvertSegmentToCurve { object_id: target, element_index });
                Ok(json!({ "segment_converted_to_curve": ok, "object_id": target.0 }))
            }
            "set_page_background" => {
                let page_index = args.get("page_index").and_then(Value::as_u64).map_or_else(|| self.engine.current_page_index(), |idx| idx as usize);
                let clear = args.get("clear").and_then(Value::as_bool).unwrap_or(false);
                let background = if clear {
                    None
                } else {
                    let r = args.get("r").and_then(Value::as_u64).unwrap_or(255).min(255) as u8;
                    let g = args.get("g").and_then(Value::as_u64).unwrap_or(255).min(255) as u8;
                    let b = args.get("b").and_then(Value::as_u64).unwrap_or(255).min(255) as u8;
                    Some(FillStyle::uniform(Color::rgb8(r, g, b)))
                };
                let ok = self.engine.execute(EngineCommand::SetPageBackground { page_index, background });
                Ok(json!({ "page_background_set": ok, "page_index": page_index }))
            }
            "set_page_bleed" => {
                let page_index = args.get("page_index").and_then(Value::as_u64).map_or_else(|| self.engine.current_page_index(), |idx| idx as usize);
                let bleed_pt = F64::new(args.get("bleed_pt").and_then(Value::as_f64).unwrap_or(9.0)).map_err(|e| e.to_string())?;
                let ok = self.engine.execute(EngineCommand::SetPageBleed { page_index, bleed_pt });
                Ok(json!({ "page_bleed_set": ok, "page_index": page_index }))
            }
            "add_pointillizer" => {
                let x = args.get("x").and_then(Value::as_f64).unwrap_or(40.0);
                let y = args.get("y").and_then(Value::as_f64).unwrap_or(40.0);
                let w = args.get("width").and_then(Value::as_f64).unwrap_or(160.0);
                let h = args.get("height").and_then(Value::as_f64).unwrap_or(120.0);
                let columns = args.get("columns").and_then(Value::as_u64).unwrap_or(6) as u32;
                let rows = args.get("rows").and_then(Value::as_u64).unwrap_or(4) as u32;
                let dot_scale = args.get("dot_scale").and_then(Value::as_f64).unwrap_or(0.75);
                let rect = Rect::new(x, y, x + w, y + h).map_err(|e| e.to_string())?;
                let path = build_pointillizer_path(rect, columns, rows, dot_scale).map_err(|e| e.to_string())?;
                self.engine.execute(EngineCommand::AddObject { name: Some("Pointillizer".to_owned()), kind: ObjectKind::Path(path) });
                Ok(json!({ "added": "pointillizer", "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "set_object_opacity" => {
                let target = self.resolve_target_id(args)?;
                let opacity = F64::new(args.get("opacity").and_then(Value::as_f64).unwrap_or(1.0)).map_err(|e| e.to_string())?;
                let ok = self.engine.execute(EngineCommand::SetObjectOpacity { object_id: target, opacity });
                Ok(json!({ "opacity_set": ok, "object_id": target.0 }))
            }
            "set_object_visible" => {
                let target = self.resolve_target_id(args)?;
                let visible = args.get("visible").and_then(Value::as_bool).unwrap_or(true);
                let ok = self.engine.execute(EngineCommand::SetObjectVisible { object_id: target, visible });
                Ok(json!({ "visibility_set": ok, "object_id": target.0 }))
            }
            "rename_object" => {
                let target = self.resolve_target_id(args)?;
                let name = args.get("name").and_then(Value::as_str).map(str::to_owned);
                let ok = self.engine.execute(EngineCommand::RenameObject { object_id: target, name });
                Ok(json!({ "object_renamed": ok, "object_id": target.0 }))
            }
            "add_impact_rays" => {
                let cx = args.get("cx").and_then(Value::as_f64).unwrap_or(150.0);
                let cy = args.get("cy").and_then(Value::as_f64).unwrap_or(150.0);
                let inner_radius = args.get("inner_radius").and_then(Value::as_f64).unwrap_or(20.0);
                let outer_radius = args.get("outer_radius").and_then(Value::as_f64).unwrap_or(80.0);
                let ray_count = args.get("ray_count").and_then(Value::as_u64).unwrap_or(12) as u32;
                let center = Point::new(cx, cy).map_err(|e| e.to_string())?;
                let path = build_impact_rays_path(center, inner_radius, outer_radius, ray_count).map_err(|e| e.to_string())?;
                self.engine.execute(EngineCommand::AddObject { name: Some("Impact Rays".to_owned()), kind: ObjectKind::Path(path) });
                Ok(json!({ "added": "impact_rays", "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "delete_layer" => {
                let layer_raw = args.get("layer_id").and_then(Value::as_u64).unwrap_or(2);
                let ok = self.engine.execute(EngineCommand::DeleteLayer { layer_id: LayerId(layer_raw) });
                Ok(json!({ "layer_deleted": ok, "layer_id": layer_raw }))
            }
            "move_layer" => {
                let page_index = args.get("page_index").and_then(Value::as_u64).map_or_else(|| self.engine.current_page_index(), |idx| idx as usize);
                let from_index = args.get("from_index").and_then(Value::as_u64).unwrap_or(0) as usize;
                let to_index = args.get("to_index").and_then(Value::as_u64).unwrap_or(0) as usize;
                let ok = self.engine.execute(EngineCommand::MoveLayer { page_index, from_index, to_index });
                Ok(json!({ "layer_moved": ok, "page_index": page_index }))
            }
            "invert_selection" => {
                let ok = self.engine.execute(EngineCommand::InvertSelection);
                let ids: Vec<u64> = self.engine.selection.iter().map(|id| id.0).collect();
                Ok(json!({ "inverted": ok, "selected_ids": ids }))
            }
            "add_complex_star" => {
                let cx = args.get("cx").and_then(Value::as_f64).unwrap_or(150.0);
                let cy = args.get("cy").and_then(Value::as_f64).unwrap_or(150.0);
                let radius = args.get("radius").and_then(Value::as_f64).unwrap_or(60.0);
                let points = args.get("points").and_then(Value::as_u64).unwrap_or(7) as u32;
                let sharpness = args.get("sharpness").and_then(Value::as_u64).unwrap_or(3) as u32;
                let center = Point::new(cx, cy).map_err(|e| e.to_string())?;
                let path = build_complex_star_path(center, radius, points, sharpness).map_err(|e| e.to_string())?;
                self.engine.execute(EngineCommand::AddObject { name: Some("Complex Star".to_owned()), kind: ObjectKind::Path(path) });
                Ok(json!({ "added": "complex_star", "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "set_outline_properties" => {
                self.apply_optional_selection(args);
                let width_pt = args.get("width_pt").and_then(Value::as_f64).map(F64::new).transpose().map_err(|e| e.to_string())?;
                let behind_fill = args.get("behind_fill").and_then(Value::as_bool);
                let scale_with_object = args.get("scale_with_object").and_then(Value::as_bool);
                let ok = self.engine.execute(EngineCommand::SetOutlineProperties { width_pt, behind_fill, scale_with_object });
                Ok(json!({ "outline_properties_set": ok }))
            }
            "select_by_kind" => {
                let kind = args.get("kind").and_then(Value::as_str).unwrap_or("path").to_owned();
                let ok = self.engine.execute(EngineCommand::SelectByKind { kind });
                let ids: Vec<u64> = self.engine.selection.iter().map(|id| id.0).collect();
                Ok(json!({ "selected_by_kind": ok, "selected_ids": ids }))
            }
            "set_document_title" => {
                let title = args.get("title").and_then(Value::as_str).unwrap_or("Untitled").to_owned();
                let ok = self.engine.execute(EngineCommand::SetDocumentTitle { title });
                Ok(json!({ "title_set": ok, "title": self.engine.document.title }))
            }
            "twirl_path" => {
                let target = self.resolve_target_id(args)?;
                let cx = args.get("cx").and_then(Value::as_f64).unwrap_or(100.0);
                let cy = args.get("cy").and_then(Value::as_f64).unwrap_or(100.0);
                let center = Point::new(cx, cy).map_err(|e| e.to_string())?;
                let radius_pt = F64::new(args.get("radius_pt").and_then(Value::as_f64).unwrap_or(60.0)).map_err(|e| e.to_string())?;
                let angle_deg = F64::new(args.get("angle_deg").and_then(Value::as_f64).unwrap_or(45.0)).map_err(|e| e.to_string())?;
                let ok = self.engine.execute(EngineCommand::TwirlObjectPath { object_id: target, center, radius_pt, angle_deg });
                Ok(json!({ "twirled": ok, "object_id": target.0 }))
            }
            "clear_effects" => {
                let target = self.resolve_target_id(args)?;
                let ok = self.engine.execute(EngineCommand::ClearEffects { object_id: target });
                Ok(json!({ "effects_cleared": ok, "object_id": target.0 }))
            }
            "clone_effects_from" => {
                self.apply_optional_selection(args);
                let src_raw = args.get("source_id").and_then(Value::as_u64).ok_or_else(|| "Missing source_id".to_owned())?;
                let ok = self.engine.execute(EngineCommand::CloneEffectsFrom { source_id: ObjectId(src_raw) });
                Ok(json!({ "effects_cloned": ok, "source_id": src_raw }))
            }
            "set_page_size" => {
                let width_pt = F64::new(args.get("width_pt").and_then(Value::as_f64).unwrap_or(595.276)).map_err(|e| e.to_string())?;
                let height_pt = F64::new(args.get("height_pt").and_then(Value::as_f64).unwrap_or(841.89)).map_err(|e| e.to_string())?;
                let units = match args.get("units").and_then(Value::as_str).unwrap_or("mm") {
                    "pt" => Unit::Pt,
                    "inch" => Unit::Inch,
                    "px" => Unit::Px,
                    _ => Unit::Mm,
                };
                let ok = self.engine.execute(EngineCommand::SetPageSetup { width_pt, height_pt, units });
                Ok(json!({ "page_size_set": ok }))
            }
            "attract_repel_path" => {
                let target = self.resolve_target_id(args)?;
                let cx = args.get("cx").and_then(Value::as_f64).unwrap_or(100.0);
                let cy = args.get("cy").and_then(Value::as_f64).unwrap_or(100.0);
                let center = Point::new(cx, cy).map_err(|e| e.to_string())?;
                let radius_pt = F64::new(args.get("radius_pt").and_then(Value::as_f64).unwrap_or(60.0)).map_err(|e| e.to_string())?;
                let strength = F64::new(args.get("strength").and_then(Value::as_f64).unwrap_or(0.4)).map_err(|e| e.to_string())?;
                let ok = self.engine.execute(EngineCommand::AttractRepelObjectPath { object_id: target, center, radius_pt, strength });
                Ok(json!({ "attracted_repelled": ok, "object_id": target.0 }))
            }
            "duplicate_page" => {
                let page_index = args.get("page_index").and_then(Value::as_u64).map_or_else(|| self.engine.current_page_index(), |v| v as usize);
                let ok = self.engine.execute(EngineCommand::DuplicatePage { page_index });
                Ok(json!({ "page_duplicated": ok, "pages": self.engine.document.pages.len(), "active_page_index": self.engine.current_page_index() }))
            }
            "rename_page" => {
                let page_index = args.get("page_index").and_then(Value::as_u64).map_or_else(|| self.engine.current_page_index(), |v| v as usize);
                let name = args.get("name").and_then(Value::as_str).unwrap_or("Page").to_owned();
                let ok = self.engine.execute(EngineCommand::RenamePage { page_index, name });
                Ok(json!({ "page_renamed": ok, "page_index": page_index }))
            }
            "delete_page" => {
                let page_index = args.get("page_index").and_then(Value::as_u64).map_or_else(|| self.engine.current_page_index(), |v| v as usize);
                let ok = self.engine.execute(EngineCommand::DeletePage { page_index });
                Ok(json!({ "page_deleted": ok, "pages": self.engine.document.pages.len(), "active_page_index": self.engine.current_page_index() }))
            }
            "smear_path" => {
                let target = self.resolve_target_id(args)?;
                let ox = args.get("ox").and_then(Value::as_f64).unwrap_or(100.0);
                let oy = args.get("oy").and_then(Value::as_f64).unwrap_or(100.0);
                let origin = Point::new(ox, oy).map_err(|e| e.to_string())?;
                let radius_pt = F64::new(args.get("radius_pt").and_then(Value::as_f64).unwrap_or(60.0)).map_err(|e| e.to_string())?;
                let dx_pt = F64::new(args.get("dx_pt").and_then(Value::as_f64).unwrap_or(15.0)).map_err(|e| e.to_string())?;
                let dy_pt = F64::new(args.get("dy_pt").and_then(Value::as_f64).unwrap_or(0.0)).map_err(|e| e.to_string())?;
                let ok = self.engine.execute(EngineCommand::SmearObjectPath { object_id: target, origin, radius_pt, dx_pt, dy_pt });
                Ok(json!({ "smeared": ok, "object_id": target.0 }))
            }
            "set_paragraph_text_layout" => {
                let target = self.resolve_target_id(args)?;
                let columns = args.get("columns").and_then(Value::as_u64).map(|v| v as u32);
                let gutter_pt = args.get("gutter_pt").and_then(Value::as_f64).map(F64::new).transpose().map_err(|e| e.to_string())?;
                let line_spacing = args.get("line_spacing").and_then(Value::as_f64).map(F64::new).transpose().map_err(|e| e.to_string())?;
                let alignment = args.get("alignment").and_then(Value::as_str).map(|s| match s {
                    "center" => TextAlignment::Center,
                    "right" => TextAlignment::Right,
                    "justify" => TextAlignment::Justify,
                    _ => TextAlignment::Left,
                });
                let ok =
                    self.engine.execute(EngineCommand::SetParagraphTextLayout { object_id: target, columns, gutter_pt, line_spacing, alignment });
                Ok(json!({ "layout_updated": ok, "object_id": target.0 }))
            }
            "set_active_page_target" => {
                let target = match args.get("target").and_then(Value::as_str).unwrap_or("page") {
                    "master" => ActivePageTarget::MasterPage,
                    "desktop" => ActivePageTarget::Desktop,
                    _ => {
                        let idx = args.get("page_index").and_then(Value::as_u64).unwrap_or(0) as usize;
                        ActivePageTarget::Page(idx)
                    }
                };
                self.engine.set_active_target(target);
                Ok(json!({ "active_page_index": self.engine.current_page_index() }))
            }
            "inspect_object" => {
                let target = self.resolve_target_id(args)?;
                let obj = self.engine.inspect_object(target).ok_or_else(|| format!("Object {} not found", target.0))?;
                let bbox = combined_selection_bounds(&[obj]);
                let kind_name = match &obj.kind {
                    ObjectKind::Rect(_) => "rect",
                    ObjectKind::Ellipse(_) => "ellipse",
                    ObjectKind::Polygon(_) => "polygon",
                    ObjectKind::Path(_) => "path",
                    ObjectKind::ArtisticText { .. } => "artistic_text",
                    ObjectKind::ParagraphText(_) => "paragraph_text",
                    ObjectKind::Group { .. } => "group",
                    ObjectKind::Bitmap { .. } => "bitmap",
                    ObjectKind::Unsupported(_) => "unsupported",
                };
                Ok(json!({
                    "id": obj.id.0,
                    "name": obj.name,
                    "kind": kind_name,
                    "visible": obj.visible,
                    "locked": obj.locked,
                    "opacity": obj.opacity.get(),
                    "effects_count": obj.effects.len(),
                    "bounds": {
                        "x": bbox.x0.get(),
                        "y": bbox.y0.get(),
                        "width": bbox.width(),
                        "height": bbox.height()
                    }
                }))
            }
            "roughen_path" => {
                let target = self.resolve_target_id(args)?;
                let tooth_height_pt = F64::new(args.get("tooth_height_pt").and_then(Value::as_f64).unwrap_or(2.5)).map_err(|e| e.to_string())?;
                let frequency = args.get("frequency").and_then(Value::as_u64).unwrap_or(4).max(1) as u32;
                let ok = self.engine.execute(EngineCommand::RoughenObjectPath { object_id: target, tooth_height_pt, frequency });
                Ok(json!({ "roughened": ok, "object_id": target.0 }))
            }
            "set_artistic_text_properties" => {
                let target = self.resolve_target_id(args)?;
                let text = args.get("text").and_then(Value::as_str).map(ToOwned::to_owned);
                let font_family = args.get("font_family").and_then(Value::as_str).map(ToOwned::to_owned);
                let size_pt = args.get("size_pt").and_then(Value::as_f64).map(F64::new).transpose().map_err(|e| e.to_string())?;
                let ok = self.engine.execute(EngineCommand::SetArtisticTextProperties { object_id: target, text, font_family, size_pt });
                Ok(json!({ "text_properties_updated": ok, "object_id": target.0 }))
            }
            "list_page_objects" => {
                let page_index = args.get("page_index").and_then(Value::as_u64).map_or_else(|| self.engine.current_page_index(), |v| v as usize);
                let page = self.engine.document.pages.get(page_index).ok_or_else(|| format!("Page {page_index} out of range"))?;
                let mut objs = Vec::new();
                for layer in &page.layers {
                    for obj in &layer.objects {
                        let kind_name = match &obj.kind {
                            ObjectKind::Rect(_) => "rect",
                            ObjectKind::Ellipse(_) => "ellipse",
                            ObjectKind::Polygon(_) => "polygon",
                            ObjectKind::Path(_) => "path",
                            ObjectKind::ArtisticText { .. } => "artistic_text",
                            ObjectKind::ParagraphText(_) => "paragraph_text",
                            ObjectKind::Group { .. } => "group",
                            ObjectKind::Bitmap { .. } => "bitmap",
                            ObjectKind::Unsupported(_) => "unsupported",
                        };
                        objs.push(json!({
                            "id": obj.id.0,
                            "name": obj.name,
                            "kind": kind_name,
                            "layer_id": layer.id.0,
                            "visible": obj.visible,
                            "locked": obj.locked,
                            "opacity": obj.opacity.get()
                        }));
                    }
                }
                Ok(json!({ "page_index": page_index, "objects": objs }))
            }
            "list_layers" => {
                let page_index = args.get("page_index").and_then(Value::as_u64).map_or_else(|| self.engine.current_page_index(), |v| v as usize);
                let page = self.engine.document.pages.get(page_index).ok_or_else(|| format!("Page {page_index} out of range"))?;
                let layers: Vec<Value> = page
                    .layers
                    .iter()
                    .map(|l| {
                        json!({
                            "id": l.id.0,
                            "name": l.name,
                            "role": l.role,
                            "visible": l.visible,
                            "locked": l.locked,
                            "printable": l.printable,
                            "opacity": l.opacity.get(),
                            "object_count": l.objects.len()
                        })
                    })
                    .collect();
                Ok(json!({ "page_index": page_index, "layers": layers }))
            }
            "smooth_path" => {
                let target = self.resolve_target_id(args)?;
                let cx = args.get("center_x").and_then(Value::as_f64).unwrap_or(0.0);
                let cy = args.get("center_y").and_then(Value::as_f64).unwrap_or(0.0);
                let center = Point::new(cx, cy).map_err(|e| e.to_string())?;
                let radius_pt = F64::new(args.get("radius_pt").and_then(Value::as_f64).unwrap_or(40.0)).map_err(|e| e.to_string())?;
                let strength = F64::new(args.get("strength").and_then(Value::as_f64).unwrap_or(0.5)).map_err(|e| e.to_string())?;
                let iterations = args.get("iterations").and_then(Value::as_u64).unwrap_or(3) as u32;
                let ok = self.engine.execute(EngineCommand::SmoothObjectPath { object_id: target, center, radius_pt, strength, iterations });
                Ok(json!({ "smoothed": ok, "object_id": target.0 }))
            }
            "update_rect_corners" => {
                if let Some(id_raw) = args.get("object_id").and_then(Value::as_u64) {
                    self.engine.set_selection(vec![ObjectId(id_raw)]);
                }
                let corner_kind = match args.get("corner_kind").and_then(Value::as_str).unwrap_or("round") {
                    "scalloped" => CornerKind::Scalloped,
                    "chamfered" => CornerKind::Chamfered,
                    _ => CornerKind::Round,
                };
                let uniform = F64::new(args.get("radius_pt").and_then(Value::as_f64).unwrap_or(0.0).max(0.0)).map_err(|e| e.to_string())?;
                let mut radii = [uniform; 4];
                let mut corners_locked = args.get("corners_locked").and_then(Value::as_bool).unwrap_or(true);
                if let Some(arr) = args.get("radii_pt").and_then(Value::as_array)
                    && arr.len() == 4
                {
                    for (idx, val) in arr.iter().enumerate() {
                        radii[idx] = F64::new(val.as_f64().unwrap_or(0.0).max(0.0)).map_err(|e| e.to_string())?;
                    }
                    corners_locked = args.get("corners_locked").and_then(Value::as_bool).unwrap_or(false);
                }
                let ok = self.engine.execute(EngineCommand::UpdateRectCorners { corner_kind, radii, corners_locked });
                Ok(json!({ "rect_corners_updated": ok, "selection": self.engine.selection.iter().map(|i| i.0).collect::<Vec<_>>() }))
            }
            "update_ellipse_parameters" => {
                if let Some(id_raw) = args.get("object_id").and_then(Value::as_u64) {
                    self.engine.set_selection(vec![ObjectId(id_raw)]);
                }
                let mode = match args.get("mode").and_then(Value::as_str).unwrap_or("full") {
                    "pie" => EllipseMode::Pie,
                    "arc" => EllipseMode::Arc,
                    _ => EllipseMode::Full,
                };
                let start_deg = F64::new(args.get("start_deg").and_then(Value::as_f64).unwrap_or(0.0)).map_err(|e| e.to_string())?;
                let end_deg = F64::new(args.get("end_deg").and_then(Value::as_f64).unwrap_or(270.0)).map_err(|e| e.to_string())?;
                let ok = self.engine.execute(EngineCommand::UpdateEllipseParameters { mode, start_deg, end_deg });
                Ok(json!({ "ellipse_parameters_updated": ok, "selection": self.engine.selection.iter().map(|i| i.0).collect::<Vec<_>>() }))
            }
            "list_pages" => {
                let pages: Vec<Value> = self
                    .engine
                    .document
                    .pages
                    .iter()
                    .enumerate()
                    .map(|(idx, p)| {
                        json!({
                            "index": idx,
                            "id": p.id.0,
                            "name": p.name,
                            "width_pt": p.size.width.get(),
                            "height_pt": p.size.height.get(),
                            "bleed_pt": p.bleed.get(),
                            "layer_count": p.layers.len()
                        })
                    })
                    .collect();
                Ok(json!({ "page_count": pages.len(), "active_page_index": self.engine.current_page_index(), "pages": pages }))
            }
            "knife_cut_object" => {
                let target = self.resolve_target_id(args)?;
                let sx = args.get("start_x").and_then(Value::as_f64).unwrap_or(0.0);
                let sy = args.get("start_y").and_then(Value::as_f64).unwrap_or(0.0);
                let ex = args.get("end_x").and_then(Value::as_f64).unwrap_or(100.0);
                let ey = args.get("end_y").and_then(Value::as_f64).unwrap_or(0.0);
                let line_start = Point::new(sx, sy).map_err(|e| e.to_string())?;
                let line_end = Point::new(ex, ey).map_err(|e| e.to_string())?;
                let gap_pt = F64::new(args.get("gap_pt").and_then(Value::as_f64).unwrap_or(1.5)).map_err(|e| e.to_string())?;
                let ok = self.engine.execute(EngineCommand::KnifeCutObject { object_id: target, line_start, line_end, gap_pt });
                Ok(json!({ "knife_cut": ok, "object_id": target.0 }))
            }
            "crop_to_rect" => {
                let x = args.get("x").and_then(Value::as_f64).unwrap_or(0.0);
                let y = args.get("y").and_then(Value::as_f64).unwrap_or(0.0);
                let w = args.get("width").and_then(Value::as_f64).unwrap_or(100.0);
                let h = args.get("height").and_then(Value::as_f64).unwrap_or(100.0);
                let rect = Rect::new(x, y, x + w, y + h).map_err(|e| e.to_string())?;
                let ok = self.engine.execute(EngineCommand::CropToRect { rect });
                Ok(json!({ "cropped": ok, "remaining_objects": self.engine.current_target_objects().len() }))
            }
            "update_polygon_parameters" => {
                if let Some(id_raw) = args.get("object_id").and_then(Value::as_u64) {
                    self.engine.set_selection(vec![ObjectId(id_raw)]);
                }
                let mode = match args.get("mode").and_then(Value::as_str).unwrap_or("polygon") {
                    "star" => PolygonSubMode::Star,
                    _ => PolygonSubMode::Polygon,
                };
                let points = args.get("points").and_then(Value::as_u64).unwrap_or(5) as u32;
                let inner_radius_pt = args.get("inner_radius_pt").and_then(Value::as_f64).map(F64::new).transpose().map_err(|e| e.to_string())?;
                let ok = self.engine.execute(EngineCommand::UpdatePolygonParameters { mode, points, inner_radius_pt });
                Ok(json!({ "polygon_parameters_updated": ok, "selection": self.engine.selection.iter().map(|i| i.0).collect::<Vec<_>>() }))
            }
            "move_object_to_layer" => {
                let target = self.resolve_target_id(args)?;
                let target_layer_id =
                    LayerId(args.get("target_layer_id").and_then(Value::as_u64).ok_or_else(|| "Missing target_layer_id".to_owned())?);
                let ok = self.engine.execute(EngineCommand::MoveObjectToLayer { object_id: target, target_layer_id });
                Ok(json!({ "moved_to_layer": ok, "object_id": target.0, "target_layer_id": target_layer_id.0 }))
            }
            "erase_object_circle" => {
                let target = self.resolve_target_id(args)?;
                let cx = args.get("center_x").and_then(Value::as_f64).unwrap_or(0.0);
                let cy = args.get("center_y").and_then(Value::as_f64).unwrap_or(0.0);
                let center = Point::new(cx, cy).map_err(|e| e.to_string())?;
                let radius_pt = F64::new(args.get("radius_pt").and_then(Value::as_f64).unwrap_or(10.0)).map_err(|e| e.to_string())?;
                let ok = self.engine.execute(EngineCommand::EraseObjectCircle { object_id: target, center, radius_pt });
                Ok(json!({ "erased": ok, "object_id": target.0 }))
            }
            "set_snap_settings" => {
                let snap_to_objects = args.get("snap_to_objects").and_then(Value::as_bool);
                let snap_to_page = args.get("snap_to_page").and_then(Value::as_bool);
                let ok = self.engine.execute(EngineCommand::SetSnapSettings { snap_to_objects, snap_to_page });
                Ok(json!({
                    "snap_updated": ok,
                    "snap_to_objects": self.engine.snap_to_objects,
                    "snap_to_page": self.engine.snap_to_page
                }))
            }
            "set_viewport" => {
                if let Some(z) = args.get("zoom").and_then(Value::as_f64) {
                    self.engine.zoom = F64::new(z.clamp(0.05, 64.0)).map_err(|e| e.to_string())?;
                }
                let px = args.get("pan_x").and_then(Value::as_f64).unwrap_or_else(|| self.engine.pan_pt.x.get());
                let py = args.get("pan_y").and_then(Value::as_f64).unwrap_or_else(|| self.engine.pan_pt.y.get());
                self.engine.pan_pt = Point::new(px, py).map_err(|e| e.to_string())?;
                Ok(json!({
                    "zoom": self.engine.zoom.get(),
                    "pan_x": self.engine.pan_pt.x.get(),
                    "pan_y": self.engine.pan_pt.y.get()
                }))
            }
            "set_active_tool" => {
                let tool_str = args.get("tool").and_then(Value::as_str).unwrap_or("pick");
                let tool = match tool_str {
                    "shape" => coralcraft_tools::ToolKind::Shape,
                    "crop" => coralcraft_tools::ToolKind::Crop,
                    "zoom" => coralcraft_tools::ToolKind::Zoom,
                    "freehand" | "bezier" => coralcraft_tools::ToolKind::FreehandBezier,
                    "rectangle" | "rect" => coralcraft_tools::ToolKind::Rectangle,
                    "ellipse" => coralcraft_tools::ToolKind::Ellipse,
                    "polygon" | "star" => coralcraft_tools::ToolKind::PolygonStar,
                    "artistic_text" | "text" => coralcraft_tools::ToolKind::ArtisticText,
                    "interactive_fill" | "fill" => coralcraft_tools::ToolKind::InteractiveFill,
                    _ => coralcraft_tools::ToolKind::Pick,
                };
                self.engine.tool_state.active_tool = tool;
                Ok(json!({ "active_tool": format!("{tool:?}") }))
            }
            "virtual_segment_delete" => {
                let target = self.resolve_target_id(args)?;
                let element_index = args.get("element_index").and_then(Value::as_u64).unwrap_or(1) as usize;
                let ok = self.engine.execute(EngineCommand::VirtualSegmentDelete { object_id: target, element_index });
                Ok(json!({ "segment_deleted": ok, "object_id": target.0, "element_index": element_index }))
            }
            "smart_fill_at_point" => {
                let x = args.get("x").and_then(Value::as_f64).unwrap_or(0.0);
                let y = args.get("y").and_then(Value::as_f64).unwrap_or(0.0);
                let point = Point::new(x, y).map_err(|e| e.to_string())?;
                let r = args.get("r").and_then(Value::as_u64).unwrap_or(0) as u8;
                let g = args.get("g").and_then(Value::as_u64).unwrap_or(180) as u8;
                let b = args.get("b").and_then(Value::as_u64).unwrap_or(120) as u8;
                let fill = FillStyle::uniform(Color::rgb8(r, g, b));
                let outline = self.engine.default_outline.clone();
                let ok = self.engine.execute(EngineCommand::SmartFillAtPoint { point, fill, outline });
                Ok(json!({ "smart_filled": ok, "selected_id": self.engine.selection.first().map(|i| i.0) }))
            }
            "export_cmx1_byte_len" => {
                let bytes = write_cmx1(&self.engine.document).map_err(|e| e.to_string())?;
                Ok(json!({ "cmx1_bytes": bytes.len() }))
            }
            "export_cdrx_zip_byte_len" => {
                let bytes = write_cdrx_zip(&self.engine.document).map_err(|e| e.to_string())?;
                Ok(json!({ "cdrx_zip_bytes": bytes.len() }))
            }
            "offset_object_path" => {
                let target = self.resolve_target_id(args)?;
                let distance_pt = F64::new(args.get("distance_pt").and_then(Value::as_f64).unwrap_or(5.0)).map_err(|e| e.to_string())?;
                let copy = args.get("copy").and_then(Value::as_bool).unwrap_or(false);
                let ok = self.engine.execute(EngineCommand::OffsetObjectPath { object_id: target, distance_pt, copy });
                Ok(json!({ "offset_applied": ok, "selected_id": self.engine.selection.first().map(|i| i.0) }))
            }
            "set_path_fill_rule" => {
                let target = self.resolve_target_id(args)?;
                let fill_rule = match args.get("fill_rule").and_then(Value::as_str).unwrap_or("non_zero") {
                    "even_odd" | "evenodd" => FillRule::EvenOdd,
                    _ => FillRule::NonZero,
                };
                let ok = self.engine.execute(EngineCommand::SetPathFillRule { object_id: target, fill_rule });
                Ok(json!({ "fill_rule_updated": ok, "object_id": target.0, "fill_rule": format!("{fill_rule:?}") }))
            }
            "set_style_cmyk" => {
                let channel = match args.get("channel").and_then(Value::as_str).unwrap_or("fill") {
                    "outline" | "stroke" => PaletteChannel::Outline,
                    _ => PaletteChannel::Fill,
                };
                let c = args.get("c").and_then(Value::as_f64).unwrap_or(0.0).clamp(0.0, 1.0);
                let m = args.get("m").and_then(Value::as_f64).unwrap_or(0.0).clamp(0.0, 1.0);
                let y = args.get("y").and_then(Value::as_f64).unwrap_or(0.0).clamp(0.0, 1.0);
                let k = args.get("k").and_then(Value::as_f64).unwrap_or(0.0).clamp(0.0, 1.0);
                let color = Color::cmyk(c, m, y, k).map_err(|e| e.to_string())?;
                let ok = self.engine.execute(EngineCommand::ApplyPaletteSwatch { channel, color: Some(color) });
                Ok(json!({ "cmyk_style_applied": ok }))
            }
            "set_style_hsb" => {
                let channel = match args.get("channel").and_then(Value::as_str).unwrap_or("fill") {
                    "outline" | "stroke" => PaletteChannel::Outline,
                    _ => PaletteChannel::Fill,
                };
                let h = args.get("h_deg").and_then(Value::as_f64).unwrap_or(0.0).rem_euclid(360.0);
                let s = args.get("s").and_then(Value::as_f64).unwrap_or(1.0).clamp(0.0, 1.0);
                let b = args.get("b").and_then(Value::as_f64).unwrap_or(1.0).clamp(0.0, 1.0);
                let color = Color::hsb(h, s, b).map_err(|e| e.to_string())?;
                let ok = self.engine.execute(EngineCommand::ApplyPaletteSwatch { channel, color: Some(color) });
                Ok(json!({ "hsb_style_applied": ok }))
            }
            "measure_object_geometry" => {
                let target = self.resolve_target_id(args)?;
                let (length_pt, signed_area_sq_pt) =
                    self.engine.measure_object_geometry(target).ok_or_else(|| format!("Object {} not measurable", target.0))?;
                Ok(json!({
                    "object_id": target.0,
                    "length_pt": length_pt,
                    "signed_area_sq_pt": signed_area_sq_pt,
                    "area_sq_pt": signed_area_sq_pt.abs()
                }))
            }
            "swap_object_positions" => {
                let first_id = ObjectId(args.get("first_id").and_then(Value::as_u64).ok_or_else(|| "Missing first_id".to_owned())?);
                let second_id = ObjectId(args.get("second_id").and_then(Value::as_u64).ok_or_else(|| "Missing second_id".to_owned())?);
                let ok = self.engine.execute(EngineCommand::SwapObjectPositions { first_id, second_id });
                Ok(json!({ "swapped": ok, "first_id": first_id.0, "second_id": second_id.0 }))
            }
            "center_selection_to_page" => {
                self.apply_optional_selection(args);
                let horizontal = args.get("horizontal").and_then(Value::as_bool).unwrap_or(true);
                let vertical = args.get("vertical").and_then(Value::as_bool).unwrap_or(true);
                let ok = self.engine.execute(EngineCommand::CenterSelectionToPage { horizontal, vertical });
                Ok(json!({ "centered": ok, "horizontal": horizontal, "vertical": vertical }))
            }
            "set_style_spot" => {
                let channel = match args.get("channel").and_then(Value::as_str).unwrap_or("fill") {
                    "outline" | "stroke" => PaletteChannel::Outline,
                    _ => PaletteChannel::Fill,
                };
                let name = args.get("name").and_then(Value::as_str).unwrap_or("Spot 1");
                let tint = args.get("tint").and_then(Value::as_f64).unwrap_or(1.0).clamp(0.0, 1.0);
                let c = args.get("c").and_then(Value::as_f64).unwrap_or(0.0).clamp(0.0, 1.0);
                let m = args.get("m").and_then(Value::as_f64).unwrap_or(0.0).clamp(0.0, 1.0);
                let y = args.get("y").and_then(Value::as_f64).unwrap_or(0.0).clamp(0.0, 1.0);
                let k = args.get("k").and_then(Value::as_f64).unwrap_or(0.0).clamp(0.0, 1.0);
                let color = Color::spot(name, tint, [c, m, y, k]).map_err(|e| e.to_string())?;
                let ok = self.engine.execute(EngineCommand::ApplyPaletteSwatch { channel, color: Some(color) });
                Ok(json!({ "spot_style_applied": ok, "name": name }))
            }
            "smooth_curve_node" => {
                let target = self.resolve_target_id(args)?;
                let element_index = args.get("element_index").and_then(Value::as_u64).unwrap_or(1) as usize;
                let tension = F64::new(args.get("tension").and_then(Value::as_f64).unwrap_or(0.35)).map_err(|e| e.to_string())?;
                let ok = self.engine.execute(EngineCommand::SmoothCurveNode { object_id: target, element_index, tension });
                Ok(json!({ "node_smoothed": ok, "object_id": target.0, "element_index": element_index }))
            }
            "array_duplicate_selection" => {
                self.apply_optional_selection(args);
                let rows = args.get("rows").and_then(Value::as_u64).unwrap_or(2) as u32;
                let cols = args.get("cols").and_then(Value::as_u64).unwrap_or(2) as u32;
                let spacing_x_pt = F64::new(args.get("spacing_x_pt").and_then(Value::as_f64).unwrap_or(50.0)).map_err(|e| e.to_string())?;
                let spacing_y_pt = F64::new(args.get("spacing_y_pt").and_then(Value::as_f64).unwrap_or(50.0)).map_err(|e| e.to_string())?;
                let ok = self.engine.execute(EngineCommand::ArrayDuplicateSelection { rows, cols, spacing_x_pt, spacing_y_pt });
                let ids: Vec<u64> = self.engine.selection.iter().map(|i| i.0).collect();
                Ok(json!({ "array_duplicated": ok, "created_ids": ids }))
            }
            "add_guideline" => {
                let horizontal = args.get("horizontal").and_then(Value::as_bool).unwrap_or(true);
                let position_pt = F64::new(args.get("position_pt").and_then(Value::as_f64).unwrap_or(100.0)).map_err(|e| e.to_string())?;
                let ok = self.engine.execute(EngineCommand::AddGuideline { horizontal, position_pt });
                Ok(json!({ "guideline_added": ok, "horizontal": horizontal, "position_pt": position_pt.get() }))
            }
            "clear_guidelines" => {
                let ok = self.engine.execute(EngineCommand::ClearGuidelines);
                Ok(json!({ "guidelines_cleared": ok }))
            }
            "add_polyline" => {
                let closed = args.get("closed").and_then(Value::as_bool).unwrap_or(false);
                let mut pts = Vec::new();
                if let Some(arr) = args.get("points").and_then(Value::as_array) {
                    for item in arr {
                        if let Some(pair) = item.as_array()
                            && pair.len() >= 2
                            && let (Some(x), Some(y)) = (pair[0].as_f64(), pair[1].as_f64())
                        {
                            pts.push(Point::new(x, y).map_err(|e| e.to_string())?);
                        }
                    }
                }
                if pts.len() < 2 {
                    pts = vec![Point::new(20.0, 20.0).map_err(|e| e.to_string())?, Point::new(120.0, 80.0).map_err(|e| e.to_string())?];
                }
                let path = build_polyline_path(&pts, closed).map_err(|e| e.to_string())?;
                self.engine.execute(EngineCommand::AddObject { name: Some("Polyline".to_owned()), kind: ObjectKind::Path(path) });
                Ok(json!({ "added": "polyline", "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "circular_duplicate_selection" => {
                self.apply_optional_selection(args);
                let count = args.get("count").and_then(Value::as_u64).unwrap_or(6) as u32;
                let cx = args.get("center_x").and_then(Value::as_f64).unwrap_or(150.0);
                let cy = args.get("center_y").and_then(Value::as_f64).unwrap_or(150.0);
                let center = Point::new(cx, cy).map_err(|e| e.to_string())?;
                let total_angle_deg = F64::new(args.get("total_angle_deg").and_then(Value::as_f64).unwrap_or(360.0)).map_err(|e| e.to_string())?;
                let rotate_objects = args.get("rotate_objects").and_then(Value::as_bool).unwrap_or(true);
                let ok = self.engine.execute(EngineCommand::CircularDuplicateSelection { count, center, total_angle_deg, rotate_objects });
                let ids: Vec<u64> = self.engine.selection.iter().map(|i| i.0).collect();
                Ok(json!({ "circular_duplicated": ok, "created_ids": ids }))
            }
            "fit_page_to_selection" => {
                self.apply_optional_selection(args);
                let margin_pt = F64::new(args.get("margin_pt").and_then(Value::as_f64).unwrap_or(10.0)).map_err(|e| e.to_string())?;
                let ok = self.engine.execute(EngineCommand::FitPageToSelection { margin_pt });
                let sz = self.engine.current_page().size;
                Ok(json!({ "page_fitted": ok, "width_pt": sz.width.get(), "height_pt": sz.height.get() }))
            }
            "set_outline_cap_join" => {
                self.apply_optional_selection(args);
                let cap = args.get("cap").and_then(Value::as_str).map(|s| match s {
                    "round" => LineCap::Round,
                    "square" => LineCap::Square,
                    _ => LineCap::Butt,
                });
                let join = args.get("join").and_then(Value::as_str).map(|s| match s {
                    "round" => LineJoin::Round,
                    "bevel" => LineJoin::Bevel,
                    _ => LineJoin::Miter,
                });
                let miter_limit = args.get("miter_limit").and_then(Value::as_f64).and_then(|v| F64::new(v).ok());
                let ok = self.engine.execute(EngineCommand::SetOutlineCapAndJoin { cap, join, miter_limit });
                Ok(json!({ "outline_cap_join_updated": ok }))
            }
            "add_gear_wheel" => {
                let cx = args.get("cx").and_then(Value::as_f64).unwrap_or(150.0);
                let cy = args.get("cy").and_then(Value::as_f64).unwrap_or(150.0);
                let outer_radius = args.get("outer_radius").and_then(Value::as_f64).unwrap_or(60.0);
                let inner_radius = args.get("inner_radius").and_then(Value::as_f64).unwrap_or(45.0);
                let teeth = args.get("teeth").and_then(Value::as_u64).unwrap_or(12) as u32;
                let center = Point::new(cx, cy).map_err(|e| e.to_string())?;
                let path = build_gear_wheel_path(center, outer_radius, inner_radius, teeth).map_err(|e| e.to_string())?;
                self.engine.execute(EngineCommand::AddObject { name: Some("Gear Wheel".to_owned()), kind: ObjectKind::Path(path) });
                Ok(json!({ "added": "gear_wheel", "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "align_curve_nodes" => {
                let target = self.resolve_target_id(args)?;
                let element_indices: Vec<usize> = args
                    .get("element_indices")
                    .and_then(Value::as_array)
                    .map(|arr| arr.iter().filter_map(|v| v.as_u64().map(|n| n as usize)).collect())
                    .unwrap_or_else(|| vec![0, 1]);
                let horizontal = args.get("horizontal").and_then(Value::as_bool).unwrap_or(true);
                let vertical = args.get("vertical").and_then(Value::as_bool).unwrap_or(false);
                let ok = self.engine.execute(EngineCommand::AlignCurveNodes { object_id: target, element_indices, horizontal, vertical });
                Ok(json!({ "nodes_aligned": ok, "object_id": target.0 }))
            }
            "shift_selection_colors" => {
                self.apply_optional_selection(args);
                let hue_delta_deg = F64::new(args.get("hue_delta_deg").and_then(Value::as_f64).unwrap_or(30.0)).map_err(|e| e.to_string())?;
                let saturation_delta = F64::new(args.get("saturation_delta").and_then(Value::as_f64).unwrap_or(0.0)).map_err(|e| e.to_string())?;
                let brightness_delta = F64::new(args.get("brightness_delta").and_then(Value::as_f64).unwrap_or(0.0)).map_err(|e| e.to_string())?;
                let ok = self.engine.execute(EngineCommand::ShiftSelectionColors { hue_delta_deg, saturation_delta, brightness_delta });
                Ok(json!({ "colors_shifted": ok }))
            }
            "export_coralcraft_json" => {
                let bytes = save_json(&self.engine.document).map_err(|e| e.to_string())?;
                let json_str = String::from_utf8(bytes).map_err(|e| e.to_string())?;
                Ok(json!({ "coralcraft_json_bytes": json_str.len(), "coralcraft_json": json_str }))
            }
            "add_block_arrow" => {
                let sx = args.get("start_x").and_then(Value::as_f64).unwrap_or(50.0);
                let sy = args.get("start_y").and_then(Value::as_f64).unwrap_or(100.0);
                let ex = args.get("end_x").and_then(Value::as_f64).unwrap_or(200.0);
                let ey = args.get("end_y").and_then(Value::as_f64).unwrap_or(100.0);
                let shaft_w = args.get("shaft_width_pt").and_then(Value::as_f64).unwrap_or(24.0);
                let head_w = args.get("head_width_pt").and_then(Value::as_f64).unwrap_or(48.0);
                let head_l = args.get("head_length_pt").and_then(Value::as_f64).unwrap_or(36.0);
                let start = Point::new(sx, sy).map_err(|e| e.to_string())?;
                let end = Point::new(ex, ey).map_err(|e| e.to_string())?;
                let path = build_block_arrow_path(start, end, shaft_w, head_w, head_l).map_err(|e| e.to_string())?;
                self.engine.execute(EngineCommand::AddObject { name: Some("Block Arrow".to_owned()), kind: ObjectKind::Path(path) });
                Ok(json!({ "added": "block_arrow", "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "subdivide_path_segments" => {
                let target = self.resolve_target_id(args)?;
                let ok = self.engine.execute(EngineCommand::SubdividePathSegments { object_id: target });
                Ok(json!({ "subdivided": ok, "object_id": target.0 }))
            }
            "convert_selection_colors" => {
                self.apply_optional_selection(args);
                let invert = args.get("invert").and_then(Value::as_bool).unwrap_or(false);
                let grayscale = args.get("grayscale").and_then(Value::as_bool).unwrap_or(true);
                let ok = self.engine.execute(EngineCommand::ConvertSelectionColors { invert, grayscale });
                Ok(json!({ "colors_converted": ok, "invert": invert, "grayscale": grayscale }))
            }
            "import_coralcraft_json" => {
                let raw = args.get("coralcraft_json").and_then(Value::as_str).ok_or_else(|| "Missing coralcraft_json string".to_owned())?;
                let doc = load_bytes(raw.as_bytes()).map_err(|e| e.to_string())?;
                let pages = doc.pages.len();
                self.engine = EditorEngine::new(doc);
                Ok(json!({ "status": "ok", "pages": pages }))
            }
            "add_banner_ribbon" => {
                let x = args.get("x").and_then(Value::as_f64).unwrap_or(40.0);
                let y = args.get("y").and_then(Value::as_f64).unwrap_or(60.0);
                let w = args.get("width").and_then(Value::as_f64).unwrap_or(200.0);
                let h = args.get("height").and_then(Value::as_f64).unwrap_or(60.0);
                let fold_depth_pt = args.get("fold_depth_pt").and_then(Value::as_f64).unwrap_or(30.0);
                let swallowtail_depth_pt = args.get("swallowtail_depth_pt").and_then(Value::as_f64).unwrap_or(12.0);
                let rect = Rect::new(x, y, x + w, y + h).map_err(|e| e.to_string())?;
                let path = build_banner_ribbon_path(rect, fold_depth_pt, swallowtail_depth_pt).map_err(|e| e.to_string())?;
                self.engine.execute(EngineCommand::AddObject { name: Some("Banner Ribbon".to_owned()), kind: ObjectKind::Path(path) });
                Ok(json!({ "added": "banner_ribbon", "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "transform_curve_nodes" => {
                let target = self.resolve_target_id(args)?;
                let element_indices: Vec<usize> = args
                    .get("element_indices")
                    .and_then(Value::as_array)
                    .map(|arr| arr.iter().filter_map(|v| v.as_u64().map(|n| n as usize)).collect())
                    .unwrap_or_else(|| vec![0, 1]);
                let scale_x = F64::new(args.get("scale_x").and_then(Value::as_f64).unwrap_or(1.0)).map_err(|e| e.to_string())?;
                let scale_y = F64::new(args.get("scale_y").and_then(Value::as_f64).unwrap_or(1.0)).map_err(|e| e.to_string())?;
                let rotate_deg = F64::new(args.get("rotate_deg").and_then(Value::as_f64).unwrap_or(0.0)).map_err(|e| e.to_string())?;
                let ok = self.engine.execute(EngineCommand::TransformCurveNodes { object_id: target, element_indices, scale_x, scale_y, rotate_deg });
                Ok(json!({ "nodes_transformed": ok, "object_id": target.0 }))
            }
            "select_by_color" => {
                let r = args.get("r").and_then(Value::as_u64).unwrap_or(0) as u8;
                let g = args.get("g").and_then(Value::as_u64).unwrap_or(0) as u8;
                let b = args.get("b").and_then(Value::as_u64).unwrap_or(0) as u8;
                let match_fill = args.get("match_fill").and_then(Value::as_bool).unwrap_or(true);
                let match_outline = args.get("match_outline").and_then(Value::as_bool).unwrap_or(false);
                let tolerance_u8 = args.get("tolerance_u8").and_then(Value::as_u64).unwrap_or(16).min(255) as u8;
                let ok = self.engine.execute(EngineCommand::SelectByColor { color: Color::rgb8(r, g, b), match_fill, match_outline, tolerance_u8 });
                let ids: Vec<u64> = self.engine.selection.iter().map(|i| i.0).collect();
                Ok(json!({ "selected_by_color": ok, "selection": ids }))
            }
            "export_png_base64" => {
                let page_index = args.get("page_index").and_then(Value::as_u64).map_or_else(|| self.engine.current_page_index(), |idx| idx as usize);
                let dpi = args.get("dpi").and_then(Value::as_f64).unwrap_or(72.0).clamp(18.0, 600.0);
                let opts = RenderOpts {
                    scale: dpi / 72.0,
                    transparent_background: args.get("transparent_background").and_then(Value::as_bool).unwrap_or(false),
                    export_mode: true,
                };
                let out = render_page(&self.engine.document, page_index, &opts).map_err(|e| e.to_string())?;
                let b64 = base64_encode(&out.png_bytes);
                Ok(json!({
                    "width_px": out.surface.width,
                    "height_px": out.surface.height,
                    "png_bytes": out.png_bytes.len(),
                    "png_base64": b64
                }))
            }
            "add_flowchart_symbol" => {
                let x = args.get("x").and_then(Value::as_f64).unwrap_or(50.0);
                let y = args.get("y").and_then(Value::as_f64).unwrap_or(50.0);
                let w = args.get("width").and_then(Value::as_f64).unwrap_or(140.0);
                let h = args.get("height").and_then(Value::as_f64).unwrap_or(70.0);
                let kind_str = args.get("symbol_kind").and_then(Value::as_str).unwrap_or("process");
                let kind = match kind_str.to_ascii_lowercase().as_str() {
                    "decision" | "diamond" => FlowchartSymbolKind::Decision,
                    "terminator" | "capsule" | "start" | "end" => FlowchartSymbolKind::Terminator,
                    "data" | "io" | "parallelogram" => FlowchartSymbolKind::Data,
                    "document" | "doc" => FlowchartSymbolKind::Document,
                    _ => FlowchartSymbolKind::Process,
                };
                let rect = Rect::new(x, y, x + w, y + h).map_err(|e| e.to_string())?;
                let path = build_flowchart_symbol_path(rect, kind).map_err(|e| e.to_string())?;
                self.engine.execute(EngineCommand::AddObject { name: Some("Flowchart Symbol".to_owned()), kind: ObjectKind::Path(path) });
                Ok(json!({ "added": "flowchart_symbol", "symbol_kind": kind_str, "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "break_path_at_node" => {
                let target = self.resolve_target_id(args)?;
                let element_index = args.get("element_index").and_then(Value::as_u64).unwrap_or(1) as usize;
                let ok = self.engine.execute(EngineCommand::BreakPathAtNode { object_id: target, element_index });
                Ok(json!({ "path_broken": ok, "object_id": target.0, "element_index": element_index }))
            }
            "equalize_selection_size" => {
                self.apply_optional_selection(args);
                let match_width = args.get("match_width").and_then(Value::as_bool).unwrap_or(true);
                let match_height = args.get("match_height").and_then(Value::as_bool).unwrap_or(true);
                let ok = self.engine.execute(EngineCommand::EqualizeSelectionSize { match_width, match_height });
                Ok(json!({ "size_equalized": ok, "match_width": match_width, "match_height": match_height }))
            }
            "export_pdf_base64" => {
                let pdf = export_pdf(&self.engine.document).map_err(|e| e.to_string())?;
                let b64 = base64_encode(&pdf);
                Ok(json!({
                    "pdf_bytes": pdf.len(),
                    "pdf_base64": b64
                }))
            }
            "add_rose_guilloche" => {
                let cx = args.get("cx").and_then(Value::as_f64).unwrap_or(150.0);
                let cy = args.get("cy").and_then(Value::as_f64).unwrap_or(150.0);
                let radius = args.get("radius").and_then(Value::as_f64).unwrap_or(60.0);
                let petals_k = args.get("petals_k").and_then(Value::as_u64).unwrap_or(5) as u32;
                let points_per_lobe = args.get("points_per_lobe").and_then(Value::as_u64).unwrap_or(16) as u32;
                let center = Point::new(cx, cy).map_err(|e| e.to_string())?;
                let path = build_rose_guilloche_path(center, radius, petals_k, points_per_lobe).map_err(|e| e.to_string())?;
                self.engine.execute(EngineCommand::AddObject { name: Some("Rose Guilloche".to_owned()), kind: ObjectKind::Path(path) });
                Ok(json!({ "added": "rose_guilloche", "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "morph_between_objects" => {
                let start_id = ObjectId(args.get("start_id").and_then(Value::as_u64).ok_or_else(|| "Missing start_id".to_owned())?);
                let end_id = ObjectId(args.get("end_id").and_then(Value::as_u64).ok_or_else(|| "Missing end_id".to_owned())?);
                let t = F64::new(args.get("t").and_then(Value::as_f64).unwrap_or(0.5)).map_err(|e| e.to_string())?;
                let sample_count = args.get("sample_count").and_then(Value::as_u64).unwrap_or(32) as usize;
                let ok = self.engine.execute(EngineCommand::MorphBetweenObjects { start_id, end_id, t, sample_count });
                Ok(json!({ "morphed": ok, "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "distribute_exact_gap" => {
                self.apply_optional_selection(args);
                let horizontal = args.get("horizontal").and_then(Value::as_bool).unwrap_or(true);
                let gap_pt = F64::new(args.get("gap_pt").and_then(Value::as_f64).unwrap_or(12.0)).map_err(|e| e.to_string())?;
                let ok = self.engine.execute(EngineCommand::DistributeExactGap { horizontal, gap_pt });
                Ok(json!({ "distributed_exact_gap": ok, "horizontal": horizontal, "gap_pt": gap_pt.get() }))
            }
            "extract_document_palette" => {
                let mut seen = std::collections::BTreeSet::new();
                let mut colors = Vec::new();
                let mut record_color = |c: &Color| {
                    let [r, g, b] = c.to_srgb8();
                    if seen.insert([r, g, b]) {
                        colors.push(json!({
                            "hex": format!("#{r:02X}{g:02X}{b:02X}"),
                            "r": r,
                            "g": g,
                            "b": b
                        }));
                    }
                };
                for page in &self.engine.document.pages {
                    for layer in &page.layers {
                        for obj in &layer.objects {
                            if let Some(fill) = &obj.fill {
                                match fill {
                                    FillStyle::Uniform { color } => record_color(color),
                                    FillStyle::Fountain { stops, .. } => {
                                        for s in stops {
                                            record_color(&s.color);
                                        }
                                    }
                                    FillStyle::TwoColorPattern { fg_color, bg_color, .. } => {
                                        record_color(fg_color);
                                        record_color(bg_color);
                                    }
                                    FillStyle::Unknown { .. } => {}
                                }
                            }
                            if let Some(ol) = &obj.outline {
                                record_color(&ol.color);
                            }
                        }
                    }
                }
                Ok(json!({ "color_count": colors.len(), "colors": colors }))
            }
            "add_barcode" => {
                let x = args.get("x").and_then(Value::as_f64).unwrap_or(50.0);
                let y = args.get("y").and_then(Value::as_f64).unwrap_or(50.0);
                let w = args.get("width").and_then(Value::as_f64).unwrap_or(180.0);
                let h = args.get("height").and_then(Value::as_f64).unwrap_or(60.0);
                let digits = args.get("digits").and_then(Value::as_str).unwrap_or("012345678905");
                let rect = Rect::new(x, y, x + w, y + h).map_err(|e| e.to_string())?;
                let path = build_barcode_path(rect, digits).map_err(|e| e.to_string())?;
                self.engine.execute(EngineCommand::AddObject { name: Some("Barcode".to_owned()), kind: ObjectKind::Path(path) });
                Ok(json!({ "added": "barcode", "digits": digits, "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "divide_selection" => {
                self.apply_optional_selection(args);
                let ok = self.engine.execute(EngineCommand::DivideSelection);
                let ids: Vec<u64> = self.engine.selection.iter().map(|i| i.0).collect();
                Ok(json!({ "divided": ok, "region_ids": ids }))
            }
            "distribute_selection_along_path" => {
                self.apply_optional_selection(args);
                let spine_object_id =
                    ObjectId(args.get("spine_object_id").and_then(Value::as_u64).ok_or_else(|| "Missing spine_object_id".to_owned())?);
                let rotate_to_tangent = args.get("rotate_to_tangent").and_then(Value::as_bool).unwrap_or(true);
                let ok = self.engine.execute(EngineCommand::DistributeSelectionAlongPath { spine_object_id, rotate_to_tangent });
                Ok(json!({ "distributed_along_path": ok, "spine_object_id": spine_object_id.0 }))
            }
            "recolor_artwork" => {
                self.apply_optional_selection(args);
                let fr = args.get("from_r").and_then(Value::as_u64).unwrap_or(0) as u8;
                let fg = args.get("from_g").and_then(Value::as_u64).unwrap_or(0) as u8;
                let fb = args.get("from_b").and_then(Value::as_u64).unwrap_or(0) as u8;
                let tr = args.get("to_r").and_then(Value::as_u64).unwrap_or(0) as u8;
                let tg = args.get("to_g").and_then(Value::as_u64).unwrap_or(120) as u8;
                let tb = args.get("to_b").and_then(Value::as_u64).unwrap_or(215) as u8;
                let tolerance_u8 = args.get("tolerance_u8").and_then(Value::as_u64).unwrap_or(16).min(255) as u8;
                let selection_only = args.get("selection_only").and_then(Value::as_bool).unwrap_or(false);
                let ok = self.engine.execute(EngineCommand::RecolorArtwork {
                    from_color: Color::rgb8(fr, fg, fb),
                    to_color: Color::rgb8(tr, tg, tb),
                    tolerance_u8,
                    selection_only,
                });
                Ok(json!({ "recolored": ok }))
            }
            "add_perspective_grid" => {
                let x = args.get("x").and_then(Value::as_f64).unwrap_or(20.0);
                let y = args.get("y").and_then(Value::as_f64).unwrap_or(20.0);
                let w = args.get("width").and_then(Value::as_f64).unwrap_or(240.0);
                let h = args.get("height").and_then(Value::as_f64).unwrap_or(160.0);
                let horizon_y_pt = args.get("horizon_y_pt").and_then(Value::as_f64).unwrap_or(y + 0.4 * h);
                let subdivisions = args.get("subdivisions").and_then(Value::as_u64).unwrap_or(6) as u32;
                let rect = Rect::new(x, y, x + w, y + h).map_err(|e| e.to_string())?;
                let path = build_perspective_grid_path(rect, horizon_y_pt, subdivisions).map_err(|e| e.to_string())?;
                self.engine.execute(EngineCommand::AddObject { name: Some("Perspective Grid".to_owned()), kind: ObjectKind::Path(path) });
                Ok(json!({ "added": "perspective_grid", "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "hatch_object_path" => {
                let target = self.resolve_target_id(args)?;
                let angle_deg = F64::new(args.get("angle_deg").and_then(Value::as_f64).unwrap_or(45.0)).map_err(|e| e.to_string())?;
                let spacing_pt = F64::new(args.get("spacing_pt").and_then(Value::as_f64).unwrap_or(6.0)).map_err(|e| e.to_string())?;
                let cross_hatch = args.get("cross_hatch").and_then(Value::as_bool).unwrap_or(false);
                let ok = self.engine.execute(EngineCommand::HatchObjectPath { object_id: target, angle_deg, spacing_pt, cross_hatch });
                Ok(json!({ "hatched": ok, "object_id": target.0 }))
            }
            "create_intermediate_blend_paths" => {
                let start_id = ObjectId(args.get("start_id").and_then(Value::as_u64).ok_or_else(|| "Missing start_id".to_owned())?);
                let end_id = ObjectId(args.get("end_id").and_then(Value::as_u64).ok_or_else(|| "Missing end_id".to_owned())?);
                let steps = args.get("steps").and_then(Value::as_u64).unwrap_or(3) as u32;
                let sample_count = args.get("sample_count").and_then(Value::as_u64).unwrap_or(32) as usize;
                let ok = self.engine.execute(EngineCommand::CreateIntermediateBlendPaths { start_id, end_id, steps, sample_count });
                let ids: Vec<u64> = self.engine.selection.iter().map(|i| i.0).collect();
                Ok(json!({ "blend_paths_created": ok, "created_ids": ids }))
            }
            "export_svg_base64" => {
                let page_index = args.get("page_index").and_then(Value::as_u64).map_or_else(|| self.engine.current_page_index(), |idx| idx as usize);
                let svg = export_svg(&self.engine.document, page_index).map_err(|e| e.to_string())?;
                let b64 = base64_encode(svg.as_bytes());
                Ok(json!({
                    "svg_bytes": svg.len(),
                    "svg_base64": b64
                }))
            }
            "add_superformula_shape" => {
                let cx = args.get("cx").and_then(Value::as_f64).unwrap_or(150.0);
                let cy = args.get("cy").and_then(Value::as_f64).unwrap_or(150.0);
                let radius = args.get("radius").and_then(Value::as_f64).unwrap_or(60.0);
                let m = args.get("m").and_then(Value::as_f64).unwrap_or(6.0);
                let n1 = args.get("n1").and_then(Value::as_f64).unwrap_or(1.0);
                let n2 = args.get("n2").and_then(Value::as_f64).unwrap_or(7.0);
                let n3 = args.get("n3").and_then(Value::as_f64).unwrap_or(8.0);
                let steps = args.get("steps").and_then(Value::as_u64).unwrap_or(96) as u32;
                let center = Point::new(cx, cy).map_err(|e| e.to_string())?;
                let path = build_superformula_path(center, radius, m, n1, n2, n3, steps).map_err(|e| e.to_string())?;
                self.engine.execute(EngineCommand::AddObject { name: Some("Superformula Shape".to_owned()), kind: ObjectKind::Path(path) });
                Ok(json!({ "added": "superformula_shape", "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "project_selection_axonometric" => {
                self.apply_optional_selection(args);
                let plane_str = args.get("plane").and_then(Value::as_str).unwrap_or("top");
                let plane = match plane_str.to_ascii_lowercase().as_str() {
                    "left" => AxonometricPlane::IsometricLeft,
                    "right" => AxonometricPlane::IsometricRight,
                    _ => AxonometricPlane::IsometricTop,
                };
                let ok = self.engine.execute(EngineCommand::ProjectSelectionAxonometric { plane });
                Ok(json!({ "projected_axonometric": ok, "plane": plane_str }))
            }
            "pack_selection_in_grid" => {
                self.apply_optional_selection(args);
                let ox = args.get("origin_x").and_then(Value::as_f64).unwrap_or(40.0);
                let oy = args.get("origin_y").and_then(Value::as_f64).unwrap_or(40.0);
                let origin = Point::new(ox, oy).map_err(|e| e.to_string())?;
                let cols = args.get("cols").and_then(Value::as_u64).unwrap_or(3) as u32;
                let gap_pt = F64::new(args.get("gap_pt").and_then(Value::as_f64).unwrap_or(12.0)).map_err(|e| e.to_string())?;
                let ok = self.engine.execute(EngineCommand::PackSelectionInGrid { origin, cols, gap_pt });
                Ok(json!({ "packed_in_grid": ok, "cols": cols, "gap_pt": gap_pt.get() }))
            }
            "export_eps_base64" => {
                let page_index = args.get("page_index").and_then(Value::as_u64).map_or_else(|| self.engine.current_page_index(), |idx| idx as usize);
                let eps = export_eps(&self.engine.document, page_index).map_err(|e| e.to_string())?;
                let b64 = base64_encode(eps.as_bytes());
                Ok(json!({
                    "eps_bytes": eps.len(),
                    "eps_base64": b64
                }))
            }
            "add_basic_shape" => {
                let x = args.get("x").and_then(Value::as_f64).unwrap_or(50.0);
                let y = args.get("y").and_then(Value::as_f64).unwrap_or(50.0);
                let w = args.get("width").and_then(Value::as_f64).unwrap_or(100.0);
                let h = args.get("height").and_then(Value::as_f64).unwrap_or(100.0);
                let kind_str = args.get("shape_kind").and_then(Value::as_str).unwrap_or("heart");
                let kind = match kind_str.to_ascii_lowercase().as_str() {
                    "cross" | "plus" => BasicShapeKind::Cross,
                    "lightning" | "bolt" => BasicShapeKind::Lightning,
                    "crescent_moon" | "moon" => BasicShapeKind::CrescentMoon,
                    "teardrop" | "drop" => BasicShapeKind::Teardrop,
                    "diamond" | "rhombus" => BasicShapeKind::Diamond,
                    _ => BasicShapeKind::Heart,
                };
                let rect = Rect::new(x, y, x + w, y + h).map_err(|e| e.to_string())?;
                let path = build_basic_shape_path(rect, kind).map_err(|e| e.to_string())?;
                self.engine.execute(EngineCommand::AddObject { name: Some("Basic Shape".to_owned()), kind: ObjectKind::Path(path) });
                Ok(json!({ "added": "basic_shape", "shape_kind": kind_str, "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "add_lsystem_path" => {
                let ox = args.get("origin_x").and_then(Value::as_f64).unwrap_or(150.0);
                let oy = args.get("origin_y").and_then(Value::as_f64).unwrap_or(200.0);
                let origin = Point::new(ox, oy).map_err(|e| e.to_string())?;
                let axiom = args.get("axiom").and_then(Value::as_str).unwrap_or("F");
                let rule_sym = args.get("rule_symbol").and_then(Value::as_str).unwrap_or("F");
                let rule_rep = args.get("rule_replacement").and_then(Value::as_str).unwrap_or("F[+F]F[-F]F");
                let turn_angle_deg = args.get("turn_angle_deg").and_then(Value::as_f64).unwrap_or(25.0);
                let step_pt = args.get("step_pt").and_then(Value::as_f64).unwrap_or(8.0);
                let iterations = args.get("iterations").and_then(Value::as_u64).unwrap_or(3) as u32;
                let rules = [(rule_sym, rule_rep)];
                let path = build_lsystem_path(origin, axiom, &rules, turn_angle_deg, step_pt, iterations).map_err(|e| e.to_string())?;
                self.engine.execute(EngineCommand::AddObject { name: Some("L-System Path".to_owned()), kind: ObjectKind::Path(path) });
                Ok(json!({ "added": "lsystem_path", "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "exclude_selection" => {
                self.apply_optional_selection(args);
                let ok = self.engine.execute(EngineCommand::ExcludeSelection);
                Ok(json!({ "excluded": ok, "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "resolve_selection_overlaps" => {
                self.apply_optional_selection(args);
                let min_gap_pt = F64::new(args.get("min_gap_pt").and_then(Value::as_f64).unwrap_or(8.0)).map_err(|e| e.to_string())?;
                let iterations = args.get("iterations").and_then(Value::as_u64).unwrap_or(20) as u32;
                let ok = self.engine.execute(EngineCommand::ResolveSelectionOverlaps { min_gap_pt, iterations });
                Ok(json!({ "overlaps_resolved": ok, "min_gap_pt": min_gap_pt.get(), "iterations": iterations }))
            }
            "add_voronoi_mosaic" => {
                let x = args.get("x").and_then(Value::as_f64).unwrap_or(40.0);
                let y = args.get("y").and_then(Value::as_f64).unwrap_or(40.0);
                let w = args.get("width").and_then(Value::as_f64).unwrap_or(160.0);
                let h = args.get("height").and_then(Value::as_f64).unwrap_or(120.0);
                let cell_gap_pt = args.get("cell_gap_pt").and_then(Value::as_f64).unwrap_or(2.0);
                let rect = Rect::new(x, y, x + w, y + h).map_err(|e| e.to_string())?;
                let mut seeds = Vec::new();
                if let Some(arr) = args.get("seeds").and_then(Value::as_array) {
                    for item in arr {
                        if let Some(pair) = item.as_array()
                            && pair.len() >= 2
                            && let (Some(sx), Some(sy)) = (pair[0].as_f64(), pair[1].as_f64())
                        {
                            seeds.push(Point::new(sx, sy).map_err(|e| e.to_string())?);
                        }
                    }
                }
                if seeds.is_empty() {
                    seeds = vec![
                        Point::new(x + 0.25 * w, y + 0.3 * h).map_err(|e| e.to_string())?,
                        Point::new(x + 0.75 * w, y + 0.3 * h).map_err(|e| e.to_string())?,
                        Point::new(x + 0.5 * w, y + 0.7 * h).map_err(|e| e.to_string())?,
                        Point::new(x + 0.2 * w, y + 0.8 * h).map_err(|e| e.to_string())?,
                        Point::new(x + 0.8 * w, y + 0.8 * h).map_err(|e| e.to_string())?,
                    ];
                }
                let path = build_voronoi_mosaic_path(rect, &seeds, cell_gap_pt).map_err(|e| e.to_string())?;
                self.engine.execute(EngineCommand::AddObject { name: Some("Voronoi Mosaic".to_owned()), kind: ObjectKind::Path(path) });
                Ok(json!({ "added": "voronoi_mosaic", "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "apply_variable_width_stroke" => {
                let target = self.resolve_target_id(args)?;
                let mut widths_pt = Vec::new();
                if let Some(arr) = args.get("widths_pt").and_then(Value::as_array) {
                    for v in arr {
                        if let Some(w) = v.as_f64() {
                            widths_pt.push(F64::new(w).map_err(|e| e.to_string())?);
                        }
                    }
                }
                if widths_pt.len() < 2 {
                    widths_pt = vec![
                        F64::new(2.0).map_err(|e| e.to_string())?,
                        F64::new(10.0).map_err(|e| e.to_string())?,
                        F64::new(3.0).map_err(|e| e.to_string())?,
                    ];
                }
                let ok = self.engine.execute(EngineCommand::ApplyVariableWidthStroke { object_id: target, widths_pt });
                Ok(json!({ "variable_width_stroke_applied": ok, "object_id": target.0 }))
            }
            "chaikin_smooth_path" => {
                let target = self.resolve_target_id(args)?;
                let iterations = args.get("iterations").and_then(Value::as_u64).unwrap_or(2) as u32;
                let ok = self.engine.execute(EngineCommand::ChaikinSmoothObjectPath { object_id: target, iterations });
                Ok(json!({ "chaikin_smoothed": ok, "object_id": target.0, "iterations": iterations }))
            }
            "export_dxf_base64" => {
                let page_index = args.get("page_index").and_then(Value::as_u64).map_or_else(|| self.engine.current_page_index(), |idx| idx as usize);
                let dxf = export_dxf(&self.engine.document, page_index).map_err(|e| e.to_string())?;
                let b64 = base64_encode(dxf.as_bytes());
                Ok(json!({
                    "dxf_bytes": dxf.len(),
                    "dxf_base64": b64
                }))
            }
            "add_isometric_box" => {
                let cx = args.get("cx").and_then(Value::as_f64).unwrap_or(150.0);
                let cy = args.get("cy").and_then(Value::as_f64).unwrap_or(150.0);
                let width_pt = args.get("width_pt").and_then(Value::as_f64).unwrap_or(60.0);
                let depth_pt = args.get("depth_pt").and_then(Value::as_f64).unwrap_or(50.0);
                let height_pt = args.get("height_pt").and_then(Value::as_f64).unwrap_or(70.0);
                let center = Point::new(cx, cy).map_err(|e| e.to_string())?;
                let path = build_isometric_box_path(center, width_pt, depth_pt, height_pt).map_err(|e| e.to_string())?;
                self.engine.execute(EngineCommand::AddObject { name: Some("Isometric 3D Box".to_owned()), kind: ObjectKind::Path(path) });
                Ok(json!({ "added": "isometric_box", "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "wave_distort_path" => {
                let target = self.resolve_target_id(args)?;
                let amplitude_pt = F64::new(args.get("amplitude_pt").and_then(Value::as_f64).unwrap_or(4.0)).map_err(|e| e.to_string())?;
                let frequency = F64::new(args.get("frequency").and_then(Value::as_f64).unwrap_or(4.0)).map_err(|e| e.to_string())?;
                let phase_deg = F64::new(args.get("phase_deg").and_then(Value::as_f64).unwrap_or(0.0)).map_err(|e| e.to_string())?;
                let ok = self.engine.execute(EngineCommand::WaveDistortObjectPath { object_id: target, amplitude_pt, frequency, phase_deg });
                Ok(json!({ "wave_distorted": ok, "object_id": target.0 }))
            }
            "radial_scale_selection_positions" => {
                self.apply_optional_selection(args);
                let cx = args.get("center_x").and_then(Value::as_f64).unwrap_or(150.0);
                let cy = args.get("center_y").and_then(Value::as_f64).unwrap_or(150.0);
                let center = Point::new(cx, cy).map_err(|e| e.to_string())?;
                let radial_scale = F64::new(args.get("radial_scale").and_then(Value::as_f64).unwrap_or(1.2)).map_err(|e| e.to_string())?;
                let twist_deg = F64::new(args.get("twist_deg").and_then(Value::as_f64).unwrap_or(0.0)).map_err(|e| e.to_string())?;
                let ok = self.engine.execute(EngineCommand::RadialScaleSelectionPositions { center, radial_scale, twist_deg });
                Ok(json!({ "radial_scaled": ok, "radial_scale": radial_scale.get(), "twist_deg": twist_deg.get() }))
            }
            "export_cdr7_base64" => {
                let bytes = write_cdr7(&self.engine.document).map_err(|e| e.to_string())?;
                let b64 = base64_encode(&bytes);
                Ok(json!({
                    "cdr7_bytes": bytes.len(),
                    "cdr7_base64": b64
                }))
            }
            "add_spirograph" => {
                let cx = args.get("cx").and_then(Value::as_f64).unwrap_or(150.0);
                let cy = args.get("cy").and_then(Value::as_f64).unwrap_or(150.0);
                let r_fixed_pt = args.get("r_fixed_pt").and_then(Value::as_f64).unwrap_or(60.0);
                let r_rolling_pt = args.get("r_rolling_pt").and_then(Value::as_f64).unwrap_or(20.0);
                let pen_offset_pt = args.get("pen_offset_pt").and_then(Value::as_f64).unwrap_or(15.0);
                let epitrochoid = args.get("epitrochoid").and_then(Value::as_bool).unwrap_or(false);
                let revolutions = args.get("revolutions").and_then(Value::as_u64).unwrap_or(3) as u32;
                let steps = args.get("steps").and_then(Value::as_u64).unwrap_or(120) as u32;
                let center = Point::new(cx, cy).map_err(|e| e.to_string())?;
                let path = build_spirograph_path(center, r_fixed_pt, r_rolling_pt, pen_offset_pt, epitrochoid, revolutions, steps)
                    .map_err(|e| e.to_string())?;
                self.engine.execute(EngineCommand::AddObject { name: Some("Spirograph".to_owned()), kind: ObjectKind::Path(path) });
                Ok(json!({ "added": "spirograph", "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "apply_dihedral_symmetry" => {
                let target = self.resolve_target_id(args)?;
                let cx = args.get("center_x").and_then(Value::as_f64).unwrap_or(150.0);
                let cy = args.get("center_y").and_then(Value::as_f64).unwrap_or(150.0);
                let center = Point::new(cx, cy).map_err(|e| e.to_string())?;
                let folds = args.get("folds").and_then(Value::as_u64).unwrap_or(6) as u32;
                let reflect = args.get("reflect").and_then(Value::as_bool).unwrap_or(true);
                let ok = self.engine.execute(EngineCommand::ApplyDihedralSymmetry { object_id: target, center, folds, reflect });
                Ok(json!({ "symmetry_applied": ok, "object_id": target.0, "folds": folds, "reflect": reflect }))
            }
            "scale_selection_by_distance" => {
                self.apply_optional_selection(args);
                let cx = args.get("center_x").and_then(Value::as_f64).unwrap_or(150.0);
                let cy = args.get("center_y").and_then(Value::as_f64).unwrap_or(150.0);
                let center = Point::new(cx, cy).map_err(|e| e.to_string())?;
                let min_scale = F64::new(args.get("min_scale").and_then(Value::as_f64).unwrap_or(0.5)).map_err(|e| e.to_string())?;
                let max_scale = F64::new(args.get("max_scale").and_then(Value::as_f64).unwrap_or(1.5)).map_err(|e| e.to_string())?;
                let falloff_radius_pt =
                    F64::new(args.get("falloff_radius_pt").and_then(Value::as_f64).unwrap_or(200.0)).map_err(|e| e.to_string())?;
                let ok = self.engine.execute(EngineCommand::ScaleSelectionByDistance { center, min_scale, max_scale, falloff_radius_pt });
                Ok(json!({ "scaled_by_distance": ok }))
            }
            "export_cmx1_base64" => {
                let bytes = write_cmx1(&self.engine.document).map_err(|e| e.to_string())?;
                let b64 = base64_encode(&bytes);
                Ok(json!({
                    "cmx1_bytes": bytes.len(),
                    "cmx1_base64": b64
                }))
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
        assert_eq!(tools, 196);

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
    }
}
