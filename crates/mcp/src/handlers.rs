//! MCP `tools/call` handler implementations for all 244 CoralCraft vector authoring & export tools (`R8.1`).

use coralcraft_cdr::{write_cdr7, write_cdrx_zip, write_cmx1};
use coralcraft_color::{
    Color, FountainBlendMode, FountainStop, Palette, PaletteSwatch, parse_binary_cpl, parse_xml_palette, serialize_binary_cpl, serialize_xml_palette,
};
use coralcraft_doc::{
    ArrowheadKind, Document, FillStyle, FountainGeometry, LayerId, LensKind, LineCap, LineJoin, ObjectId, ObjectKind, OutlineStyle,
    ParagraphTextSpec, PatternKind, TextAlignment, TextCaseKind, TransparencyStyle, import_dxf, import_svg, load_bytes, save_json, save_zip,
};
use coralcraft_engine::{ActivePageTarget, EditorEngine, EngineCommand, PaletteChannel};
use coralcraft_geom::{
    AxonometricPlane, BasicShapeKind, BooleanOp, CornerKind, DistortionKind, EllipseMode, EllipseSpec, F64, FillRule, FlowchartSymbolKind, Point,
    PolygonSpec, Rect, RectSpec, Unit, build_angular_dimension_path, build_banner_ribbon_path, build_barcode_path, build_basic_shape_path,
    build_block_arrow_path, build_bspline_path, build_callout_path, build_celtic_knot_ring_path, build_complex_star_path,
    build_concentric_rings_path, build_connector_path, build_dragon_curve_path, build_flowchart_symbol_path, build_gear_wheel_path,
    build_graph_paper_path, build_halftone_wave_lines_path, build_hilbert_curve_path, build_honeycomb_grid_path, build_impact_rays_path,
    build_isometric_box_path, build_koch_snowflake_path, build_lissajous_path, build_lsystem_path, build_perspective_grid_path,
    build_phyllotaxis_path, build_pointillizer_path, build_polyline_path, build_rose_guilloche_path, build_sierpinski_triangle_path,
    build_spiral_path, build_spirograph_path, build_superformula_path, build_three_point_curve_path, build_truchet_tiles_path,
    build_voronoi_mosaic_path,
};
use coralcraft_render::{
    ImpositionSpec, PrepressOptions, RenderOpts, base64_decode, base64_encode, export_dxf, export_eps, export_pdf, export_pdf_imposed,
    export_pdf_with_prepress, export_svg, export_svg_with_prepress, render_page,
};
use coralcraft_tools::{AlignMode, PolygonSubMode, ZOrderOp, combined_selection_bounds};
use serde_json::{Value, json};

use crate::McpServer;

impl McpServer {
    fn exec(&mut self, cmd: EngineCommand) -> bool {
        let ok = self.engine.execute(cmd);
        if !ok {
            self.last_command_failed = true;
        }
        ok
    }

    fn apply_optional_selection(&mut self, args: &Value) {
        if let Some(arr) = args.get("selection").and_then(Value::as_array) {
            let ids: Vec<ObjectId> = arr.iter().filter_map(|v| v.as_u64().map(ObjectId)).collect();
            self.engine.set_selection(ids);
        }
    }

    pub(crate) fn call_tool(&mut self, name: &str, args: &Value) -> Result<Value, String> {
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
                self.exec(EngineCommand::AddObject { name: Some("Rectangle".to_owned()), kind: ObjectKind::Rect(spec) });
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
                self.exec(EngineCommand::AddObject { name: Some("Ellipse".to_owned()), kind: ObjectKind::Ellipse(spec) });
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
                self.exec(EngineCommand::AddObject { name: Some("Polygon".to_owned()), kind: ObjectKind::Polygon(spec) });
                Ok(json!({ "added": "polygon", "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "add_artistic_text" => {
                let x = args.get("x").and_then(Value::as_f64).unwrap_or(72.0);
                let y = args.get("y").and_then(Value::as_f64).unwrap_or(72.0);
                let text = args.get("text").and_then(Value::as_str).unwrap_or("CoralCraft").to_owned();
                let font_family = args.get("font_family").and_then(Value::as_str).unwrap_or("Sans").to_owned();
                let size_pt = F64::new(args.get("size_pt").and_then(Value::as_f64).unwrap_or(24.0)).map_err(|e| e.to_string())?;
                let position = Point::new(x, y).map_err(|e| e.to_string())?;
                self.exec(EngineCommand::AddObject {
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
                self.exec(EngineCommand::AddObject { name: Some("Paragraph Text".to_owned()), kind: ObjectKind::ParagraphText(Box::new(spec)) });
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
                self.exec(EngineCommand::ApplyPaletteSwatch { channel, color });
                Ok(json!({ "status": "ok" }))
            }
            "add_page" => {
                self.exec(EngineCommand::AddPageAfterActive);
                Ok(json!({ "pages": self.engine.document.pages.len(), "active_page_index": self.engine.current_page_index() }))
            }
            "group_selection" => {
                self.apply_optional_selection(args);
                let ok = self.exec(EngineCommand::GroupSelection);
                Ok(json!({ "grouped": ok, "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "ungroup_selection" => {
                self.apply_optional_selection(args);
                let ok = self.exec(EngineCommand::UngroupSelection);
                let ids: Vec<u64> = self.engine.selection.iter().map(|id| id.0).collect();
                Ok(json!({ "ungrouped": ok, "selected_ids": ids }))
            }
            "convert_to_curves" => {
                self.apply_optional_selection(args);
                let ok = self.exec(EngineCommand::ConvertToCurves);
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
                let ok = self.exec(EngineCommand::ReorderSelection { op });
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
                let ok = self.exec(EngineCommand::AlignSelection { mode });
                Ok(json!({ "aligned": ok }))
            }
            "place_inside_powerclip" => {
                self.apply_optional_selection(args);
                let container_raw = args.get("container_id").and_then(Value::as_u64).ok_or_else(|| "Missing container_id".to_owned())?;
                let ok = self.exec(EngineCommand::PlaceInsidePowerClip { container_id: ObjectId(container_raw) });
                Ok(json!({ "placed": ok, "container_id": container_raw }))
            }
            "extract_powerclip" => {
                let container_raw = args.get("container_id").and_then(Value::as_u64).ok_or_else(|| "Missing container_id".to_owned())?;
                let ok = self.exec(EngineCommand::ExtractPowerClip { container_id: ObjectId(container_raw) });
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
            "undo" => {
                let ok = self.engine.undo();
                if !ok {
                    self.last_command_failed = true;
                }
                Ok(json!({ "undone": ok }))
            }
            "redo" => {
                let ok = self.engine.redo();
                if !ok {
                    self.last_command_failed = true;
                }
                Ok(json!({ "redone": ok }))
            }
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
                let ok = self.exec(EngineCommand::BooleanSelection { op });
                let ids: Vec<u64> = self.engine.selection.iter().map(|id| id.0).collect();
                Ok(json!({ "boolean_executed": ok, "result_selection": ids }))
            }
            "apply_contour" => {
                let target = self.resolve_target_id(args)?;
                let steps = args.get("steps").and_then(Value::as_u64).unwrap_or(1) as u32;
                let offset_raw = args.get("offset_pt").and_then(Value::as_f64).unwrap_or(4.0);
                let offset_pt = F64::new(offset_raw).map_err(|e| e.to_string())?;
                let inside = args.get("inside").and_then(Value::as_bool).unwrap_or(false);
                let ok = self.exec(EngineCommand::ApplyContour { object_id: target, steps, offset_pt, inside });
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
                let ok = self.exec(EngineCommand::AddDropShadow { object_id: target, offset_x: ox, offset_y: oy, opacity, color });
                Ok(json!({ "shadow_added": ok, "object_id": target.0 }))
            }
            "add_extrude" => {
                let target = self.resolve_target_id(args)?;
                let depth_pt = F64::new(args.get("depth_pt").and_then(Value::as_f64).unwrap_or(20.0)).map_err(|e| e.to_string())?;
                let angle_deg = F64::new(args.get("angle_deg").and_then(Value::as_f64).unwrap_or(45.0)).map_err(|e| e.to_string())?;
                let ok = self.exec(EngineCommand::AddExtrude { object_id: target, depth_pt, angle_deg });
                Ok(json!({ "extrude_added": ok, "object_id": target.0 }))
            }
            "create_blend" => {
                let start_raw = args.get("start_id").and_then(Value::as_u64).ok_or_else(|| "Missing start_id".to_owned())?;
                let end_raw = args.get("end_id").and_then(Value::as_u64).ok_or_else(|| "Missing end_id".to_owned())?;
                let steps = args.get("steps").and_then(Value::as_u64).unwrap_or(5) as u32;
                let ok = self.exec(EngineCommand::CreateBlend { start_id: ObjectId(start_raw), end_id: ObjectId(end_raw), steps });
                Ok(json!({ "blend_created": ok, "start_id": start_raw, "end_id": end_raw }))
            }
            "attach_blend_spine" => {
                let blend_raw = args.get("blend_object_id").and_then(Value::as_u64).ok_or_else(|| "Missing blend_object_id".to_owned())?;
                let spine_raw = args.get("spine_object_id").and_then(Value::as_u64).ok_or_else(|| "Missing spine_object_id".to_owned())?;
                let ok = self.exec(EngineCommand::AttachBlendSpine { blend_object_id: ObjectId(blend_raw), spine_object_id: ObjectId(spine_raw) });
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
                let ok = self.exec(EngineCommand::ApplyEnvelope { object_id: target, corners });
                Ok(json!({ "envelope_applied": ok, "object_id": target.0 }))
            }
            "apply_artistic_brush" => {
                let target = self.resolve_target_id(args)?;
                let start_w = F64::new(args.get("start_width_pt").and_then(Value::as_f64).unwrap_or(12.0)).map_err(|e| e.to_string())?;
                let end_w = F64::new(args.get("end_width_pt").and_then(Value::as_f64).unwrap_or(2.0)).map_err(|e| e.to_string())?;
                let ok = self.exec(EngineCommand::ApplyArtisticBrush { object_id: target, start_width_pt: start_w, end_width_pt: end_w });
                Ok(json!({ "artistic_brush_applied": ok, "object_id": target.0 }))
            }
            "fit_text_to_path" => {
                let text_raw = args.get("text_object_id").and_then(Value::as_u64).ok_or_else(|| "Missing text_object_id".to_owned())?;
                let spine_raw = args.get("spine_object_id").and_then(Value::as_u64).ok_or_else(|| "Missing spine_object_id".to_owned())?;
                let offset_pt = F64::new(args.get("offset_pt").and_then(Value::as_f64).unwrap_or(0.0)).map_err(|e| e.to_string())?;
                let ok =
                    self.exec(EngineCommand::FitTextToPath { text_object_id: ObjectId(text_raw), spine_object_id: ObjectId(spine_raw), offset_pt });
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
                let ok = self.exec(EngineCommand::StepAndRepeat { count, offset_x_pt: ox, offset_y_pt: oy });
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
                self.exec(EngineCommand::AddObject { name: Some("Linear Dimension".to_owned()), kind: ObjectKind::Path(path) });
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
                let ok = self.exec(EngineCommand::DistributeSelection { mode });
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
                let ok = self.exec(EngineCommand::ApplyPerspective { object_id: target, corners });
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
                let ok = self.exec(EngineCommand::FilletPathCorners { object_id: target, radius_pt, corner_kind });
                Ok(json!({ "filleted": ok, "object_id": target.0 }))
            }
            "expand_effects" => {
                let target = self.resolve_target_id(args)?;
                let ok = self.exec(EngineCommand::ExpandEffects { object_id: target });
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
                let ok = self.exec(EngineCommand::SetTransparency { transparency });
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
                let count: usize = doc.pages.iter().flat_map(|p| p.layers.iter()).map(|l| l.objects.len()).sum();
                self.engine = EditorEngine::new(doc);
                Ok(json!({ "status": "ok", "imported_objects": count }))
            }
            "import_dxf" => {
                let dxf = args.get("dxf").and_then(Value::as_str).ok_or_else(|| "Missing dxf string".to_owned())?;
                let doc = import_dxf(dxf).map_err(|e| e.to_string())?;
                let count: usize = doc.pages.iter().flat_map(|p| p.layers.iter()).map(|l| l.objects.len()).sum();
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
                self.exec(EngineCommand::AddObject { name: Some("Spiral".to_owned()), kind: ObjectKind::Path(path) });
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
                self.exec(EngineCommand::AddObject { name: Some("Graph Paper".to_owned()), kind: ObjectKind::Path(path) });
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
                let ok = self.exec(EngineCommand::CopyPropertiesFrom { source_id: ObjectId(src_raw), copy_fill, copy_outline, copy_effects });
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
                self.exec(EngineCommand::AddObject { name: Some("Connector".to_owned()), kind: ObjectKind::Path(path) });
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
                self.exec(EngineCommand::AddObject { name: Some("Callout".to_owned()), kind: ObjectKind::Path(path) });
                Ok(json!({ "added": "callout", "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "combine_selection" => {
                self.apply_optional_selection(args);
                let ok = self.exec(EngineCommand::CombineSelection);
                Ok(json!({ "combined": ok, "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "break_apart_selection" => {
                self.apply_optional_selection(args);
                let ok = self.exec(EngineCommand::BreakApartSelection);
                let ids: Vec<u64> = self.engine.selection.iter().map(|id| id.0).collect();
                Ok(json!({ "broken_apart": ok, "selected_ids": ids }))
            }
            "adjust_bitmap" => {
                let target = self.resolve_target_id(args)?;
                let brightness = F64::new(args.get("brightness").and_then(Value::as_f64).unwrap_or(0.0)).map_err(|e| e.to_string())?;
                let contrast = F64::new(args.get("contrast").and_then(Value::as_f64).unwrap_or(0.0)).map_err(|e| e.to_string())?;
                let grayscale = args.get("grayscale").and_then(Value::as_bool).unwrap_or(false);
                let invert = args.get("invert").and_then(Value::as_bool).unwrap_or(false);
                let ok = self.exec(EngineCommand::AdjustBitmap { object_id: target, brightness, contrast, grayscale, invert });
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
                let ok = self.exec(EngineCommand::ApplyDistortion { object_id: target, kind, amplitude, frequency });
                Ok(json!({ "distortion_applied": ok, "object_id": target.0 }))
            }
            "trace_bitmap" => {
                let target = self.resolve_target_id(args)?;
                let threshold = args.get("threshold").and_then(Value::as_u64).unwrap_or(128).min(255) as u8;
                let ok = self.exec(EngineCommand::TraceBitmap { object_id: target, threshold });
                Ok(json!({ "bitmap_traced": ok, "object_id": target.0 }))
            }
            "mirror_selection" => {
                self.apply_optional_selection(args);
                let horizontal = args.get("horizontal").and_then(Value::as_bool).unwrap_or(true);
                let vertical = args.get("vertical").and_then(Value::as_bool).unwrap_or(false);
                let copy = args.get("copy").and_then(Value::as_bool).unwrap_or(false);
                let ok = self.exec(EngineCommand::MirrorSelection { horizontal, vertical, copy });
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
                self.exec(EngineCommand::AddObject { name: Some("Angular Dimension".to_owned()), kind: ObjectKind::Path(path) });
                Ok(json!({ "added": "angular_dimension", "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "create_boundary" => {
                self.apply_optional_selection(args);
                let ok = self.exec(EngineCommand::CreateBoundary);
                Ok(json!({ "boundary_created": ok, "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "convert_outline_to_object" => {
                self.apply_optional_selection(args);
                let ok = self.exec(EngineCommand::ConvertOutlineToObject);
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
                let ok = self.exec(EngineCommand::SetOutline { outline: Some(outline) });
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
                let ok = self.exec(EngineCommand::SetFill { fill: Some(fill) });
                Ok(json!({ "fountain_fill_set": ok }))
            }
            "rotate_selection" => {
                self.apply_optional_selection(args);
                let angle_deg = F64::new(args.get("angle_deg").and_then(Value::as_f64).unwrap_or(45.0)).map_err(|e| e.to_string())?;
                let origin_index = args.get("origin_index").and_then(Value::as_u64).unwrap_or(4).min(8) as u8;
                let copy = args.get("copy").and_then(Value::as_bool).unwrap_or(false);
                let ok = self.exec(EngineCommand::RotateSelection { angle_deg, origin_index, copy });
                let ids: Vec<u64> = self.engine.selection.iter().map(|id| id.0).collect();
                Ok(json!({ "rotated": ok, "selected_ids": ids }))
            }
            "scale_selection" => {
                self.apply_optional_selection(args);
                let scale_x = F64::new(args.get("scale_x").and_then(Value::as_f64).unwrap_or(1.5)).map_err(|e| e.to_string())?;
                let scale_y = F64::new(args.get("scale_y").and_then(Value::as_f64).unwrap_or(1.5)).map_err(|e| e.to_string())?;
                let origin_index = args.get("origin_index").and_then(Value::as_u64).unwrap_or(4).min(8) as u8;
                let copy = args.get("copy").and_then(Value::as_bool).unwrap_or(false);
                let ok = self.exec(EngineCommand::ScaleSelection { scale_x, scale_y, origin_index, copy });
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
                let ok = self.exec(EngineCommand::SetOutline { outline: Some(outline) });
                Ok(json!({ "outline_arrows_set": ok }))
            }
            "skew_selection" => {
                self.apply_optional_selection(args);
                let skew_x_deg = F64::new(args.get("skew_x_deg").and_then(Value::as_f64).unwrap_or(15.0)).map_err(|e| e.to_string())?;
                let skew_y_deg = F64::new(args.get("skew_y_deg").and_then(Value::as_f64).unwrap_or(0.0)).map_err(|e| e.to_string())?;
                let origin_index = args.get("origin_index").and_then(Value::as_u64).unwrap_or(4).min(8) as u8;
                let copy = args.get("copy").and_then(Value::as_bool).unwrap_or(false);
                let ok = self.exec(EngineCommand::SkewSelection { skew_x_deg, skew_y_deg, origin_index, copy });
                let ids: Vec<u64> = self.engine.selection.iter().map(|id| id.0).collect();
                Ok(json!({ "skewed": ok, "selected_ids": ids }))
            }
            "duplicate_selection" => {
                self.apply_optional_selection(args);
                let offset_x_pt = F64::new(args.get("offset_x_pt").and_then(Value::as_f64).unwrap_or(18.0)).map_err(|e| e.to_string())?;
                let offset_y_pt = F64::new(args.get("offset_y_pt").and_then(Value::as_f64).unwrap_or(18.0)).map_err(|e| e.to_string())?;
                let ok = self.exec(EngineCommand::DuplicateSelection { offset_x_pt, offset_y_pt });
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
                let ok = self.exec(EngineCommand::SetTransparency { transparency: Some(TransparencyStyle::Lens { kind }) });
                Ok(json!({ "lens_applied": ok }))
            }
            "reverse_path_direction" => {
                let target = self.resolve_target_id(args)?;
                let ok = self.exec(EngineCommand::ReversePathDirection { object_id: target });
                Ok(json!({ "reversed": ok, "object_id": target.0 }))
            }
            "close_path_subpaths" => {
                let target = self.resolve_target_id(args)?;
                let ok = self.exec(EngineCommand::CloseOpenSubpaths { object_id: target });
                Ok(json!({ "closed": ok, "object_id": target.0 }))
            }
            "set_object_locked" => {
                let target = self.resolve_target_id(args)?;
                let locked = args.get("locked").and_then(Value::as_bool).unwrap_or(true);
                let ok = self.exec(EngineCommand::SetObjectLocked { object_id: target, locked });
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
                let ok = self.exec(EngineCommand::SetFill { fill: Some(fill) });
                Ok(json!({ "pattern_fill_set": ok }))
            }
            "join_open_curves" => {
                let target = self.resolve_target_id(args)?;
                let tolerance_pt = F64::new(args.get("tolerance_pt").and_then(Value::as_f64).unwrap_or(5.0)).map_err(|e| e.to_string())?;
                let ok = self.exec(EngineCommand::JoinOpenSubpaths { object_id: target, tolerance_pt });
                Ok(json!({ "joined": ok, "object_id": target.0 }))
            }
            "ungroup_all_selection" => {
                self.apply_optional_selection(args);
                let ok = self.exec(EngineCommand::UngroupAllSelection);
                let ids: Vec<u64> = self.engine.selection.iter().map(|id| id.0).collect();
                Ok(json!({ "ungrouped_all": ok, "selected_ids": ids }))
            }
            "move_page" => {
                let from_index = args.get("from_index").and_then(Value::as_u64).unwrap_or(0) as usize;
                let to_index = args.get("to_index").and_then(Value::as_u64).unwrap_or(0) as usize;
                let ok = self.exec(EngineCommand::MovePage { from_index, to_index });
                Ok(json!({ "page_moved": ok, "pages": self.engine.document.pages.len(), "active_page_index": self.engine.current_page_index() }))
            }
            "reduce_path_nodes" => {
                let target = self.resolve_target_id(args)?;
                let tolerance_pt = F64::new(args.get("tolerance_pt").and_then(Value::as_f64).unwrap_or(1.5)).map_err(|e| e.to_string())?;
                let ok = self.exec(EngineCommand::ReducePathNodes { object_id: target, tolerance_pt });
                Ok(json!({ "nodes_reduced": ok, "object_id": target.0 }))
            }
            "set_selection_bounds" => {
                self.apply_optional_selection(args);
                let x_pt = args.get("x_pt").and_then(Value::as_f64).map(F64::new).transpose().map_err(|e| e.to_string())?;
                let y_pt = args.get("y_pt").and_then(Value::as_f64).map(F64::new).transpose().map_err(|e| e.to_string())?;
                let width_pt = args.get("width_pt").and_then(Value::as_f64).map(F64::new).transpose().map_err(|e| e.to_string())?;
                let height_pt = args.get("height_pt").and_then(Value::as_f64).map(F64::new).transpose().map_err(|e| e.to_string())?;
                let origin_index = args.get("origin_index").and_then(Value::as_u64).unwrap_or(4).min(8) as u8;
                let ok = self.exec(EngineCommand::SetSelectionBounds { x_pt, y_pt, width_pt, height_pt, origin_index });
                Ok(json!({ "bounds_set": ok }))
            }
            "add_layer" => {
                let name = args.get("name").and_then(Value::as_str).unwrap_or("Layer 2").to_owned();
                let ok = self.exec(EngineCommand::AddLayer { name });
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
                let ok = self.exec(EngineCommand::SetLayerProperties { layer_id: LayerId(layer_raw), visible, locked, printable, opacity });
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
                self.exec(EngineCommand::AddObject { name: Some("B-Spline".to_owned()), kind: ObjectKind::Path(path) });
                Ok(json!({ "added": "bspline_curve", "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "clear_transformations" => {
                self.apply_optional_selection(args);
                let ok = self.exec(EngineCommand::ClearTransformations);
                Ok(json!({ "transformations_cleared": ok }))
            }
            "toggle_text_kind" => {
                let target = self.resolve_target_id(args)?;
                let ok = self.exec(EngineCommand::ToggleTextKind { object_id: target });
                Ok(json!({ "text_kind_toggled": ok, "object_id": target.0 }))
            }
            "find_and_replace_text" => {
                let find = args.get("find").and_then(Value::as_str).unwrap_or_default().to_owned();
                let replace = args.get("replace").and_then(Value::as_str).unwrap_or_default().to_owned();
                let ok = self.exec(EngineCommand::FindAndReplaceText { find, replace });
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
                self.exec(EngineCommand::AddObject { name: Some("3-Point Curve".to_owned()), kind: ObjectKind::Path(path) });
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
                let ok = self.exec(EngineCommand::ChangeTextCase { object_id: target, case_kind });
                Ok(json!({ "case_changed": ok, "object_id": target.0 }))
            }
            "insert_curve_node" => {
                let target = self.resolve_target_id(args)?;
                let element_index = args.get("element_index").and_then(Value::as_u64).unwrap_or(1) as usize;
                let ok = self.exec(EngineCommand::InsertCurveNode { object_id: target, element_index });
                Ok(json!({ "node_inserted": ok, "object_id": target.0 }))
            }
            "delete_curve_node" => {
                let target = self.resolve_target_id(args)?;
                let element_index = args.get("element_index").and_then(Value::as_u64).unwrap_or(1) as usize;
                let ok = self.exec(EngineCommand::DeleteCurveNode { object_id: target, element_index });
                Ok(json!({ "node_deleted": ok, "object_id": target.0 }))
            }
            "convert_segment_to_line" => {
                let target = self.resolve_target_id(args)?;
                let element_index = args.get("element_index").and_then(Value::as_u64).unwrap_or(1) as usize;
                let ok = self.exec(EngineCommand::ConvertSegmentToLine { object_id: target, element_index });
                Ok(json!({ "segment_converted_to_line": ok, "object_id": target.0 }))
            }
            "convert_segment_to_curve" => {
                let target = self.resolve_target_id(args)?;
                let element_index = args.get("element_index").and_then(Value::as_u64).unwrap_or(1) as usize;
                let ok = self.exec(EngineCommand::ConvertSegmentToCurve { object_id: target, element_index });
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
                let ok = self.exec(EngineCommand::SetPageBackground { page_index, background });
                Ok(json!({ "page_background_set": ok, "page_index": page_index }))
            }
            "set_page_bleed" => {
                let page_index = args.get("page_index").and_then(Value::as_u64).map_or_else(|| self.engine.current_page_index(), |idx| idx as usize);
                let bleed_pt = F64::new(args.get("bleed_pt").and_then(Value::as_f64).unwrap_or(9.0)).map_err(|e| e.to_string())?;
                let ok = self.exec(EngineCommand::SetPageBleed { page_index, bleed_pt });
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
                self.exec(EngineCommand::AddObject { name: Some("Pointillizer".to_owned()), kind: ObjectKind::Path(path) });
                Ok(json!({ "added": "pointillizer", "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "set_object_opacity" => {
                let target = self.resolve_target_id(args)?;
                let opacity = F64::new(args.get("opacity").and_then(Value::as_f64).unwrap_or(1.0)).map_err(|e| e.to_string())?;
                let ok = self.exec(EngineCommand::SetObjectOpacity { object_id: target, opacity });
                Ok(json!({ "opacity_set": ok, "object_id": target.0 }))
            }
            "set_object_visible" => {
                let target = self.resolve_target_id(args)?;
                let visible = args.get("visible").and_then(Value::as_bool).unwrap_or(true);
                let ok = self.exec(EngineCommand::SetObjectVisible { object_id: target, visible });
                Ok(json!({ "visibility_set": ok, "object_id": target.0 }))
            }
            "rename_object" => {
                let target = self.resolve_target_id(args)?;
                let name = args.get("name").and_then(Value::as_str).map(str::to_owned);
                let ok = self.exec(EngineCommand::RenameObject { object_id: target, name });
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
                self.exec(EngineCommand::AddObject { name: Some("Impact Rays".to_owned()), kind: ObjectKind::Path(path) });
                Ok(json!({ "added": "impact_rays", "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "delete_layer" => {
                let layer_raw = args.get("layer_id").and_then(Value::as_u64).unwrap_or(2);
                let ok = self.exec(EngineCommand::DeleteLayer { layer_id: LayerId(layer_raw) });
                Ok(json!({ "layer_deleted": ok, "layer_id": layer_raw }))
            }
            "move_layer" => {
                let page_index = args.get("page_index").and_then(Value::as_u64).map_or_else(|| self.engine.current_page_index(), |idx| idx as usize);
                let from_index = args.get("from_index").and_then(Value::as_u64).unwrap_or(0) as usize;
                let to_index = args.get("to_index").and_then(Value::as_u64).unwrap_or(0) as usize;
                let ok = self.exec(EngineCommand::MoveLayer { page_index, from_index, to_index });
                Ok(json!({ "layer_moved": ok, "page_index": page_index }))
            }
            "invert_selection" => {
                let ok = self.exec(EngineCommand::InvertSelection);
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
                self.exec(EngineCommand::AddObject { name: Some("Complex Star".to_owned()), kind: ObjectKind::Path(path) });
                Ok(json!({ "added": "complex_star", "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "set_outline_properties" => {
                self.apply_optional_selection(args);
                let width_pt = args.get("width_pt").and_then(Value::as_f64).map(F64::new).transpose().map_err(|e| e.to_string())?;
                let behind_fill = args.get("behind_fill").and_then(Value::as_bool);
                let scale_with_object = args.get("scale_with_object").and_then(Value::as_bool);
                let ok = self.exec(EngineCommand::SetOutlineProperties { width_pt, behind_fill, scale_with_object });
                Ok(json!({ "outline_properties_set": ok }))
            }
            "select_by_kind" => {
                let kind = args.get("kind").and_then(Value::as_str).unwrap_or("path").to_owned();
                let ok = self.exec(EngineCommand::SelectByKind { kind });
                let ids: Vec<u64> = self.engine.selection.iter().map(|id| id.0).collect();
                Ok(json!({ "selected_by_kind": ok, "selected_ids": ids }))
            }
            "set_document_title" => {
                let title = args.get("title").and_then(Value::as_str).unwrap_or("Untitled").to_owned();
                let ok = self.exec(EngineCommand::SetDocumentTitle { title });
                Ok(json!({ "title_set": ok, "title": self.engine.document.title }))
            }
            "twirl_path" => {
                let target = self.resolve_target_id(args)?;
                let cx = args.get("cx").and_then(Value::as_f64).unwrap_or(100.0);
                let cy = args.get("cy").and_then(Value::as_f64).unwrap_or(100.0);
                let center = Point::new(cx, cy).map_err(|e| e.to_string())?;
                let radius_pt = F64::new(args.get("radius_pt").and_then(Value::as_f64).unwrap_or(60.0)).map_err(|e| e.to_string())?;
                let angle_deg = F64::new(args.get("angle_deg").and_then(Value::as_f64).unwrap_or(45.0)).map_err(|e| e.to_string())?;
                let ok = self.exec(EngineCommand::TwirlObjectPath { object_id: target, center, radius_pt, angle_deg });
                Ok(json!({ "twirled": ok, "object_id": target.0 }))
            }
            "clear_effects" => {
                let target = self.resolve_target_id(args)?;
                let ok = self.exec(EngineCommand::ClearEffects { object_id: target });
                Ok(json!({ "effects_cleared": ok, "object_id": target.0 }))
            }
            "clone_effects_from" => {
                self.apply_optional_selection(args);
                let src_raw = args.get("source_id").and_then(Value::as_u64).ok_or_else(|| "Missing source_id".to_owned())?;
                let ok = self.exec(EngineCommand::CloneEffectsFrom { source_id: ObjectId(src_raw) });
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
                let ok = self.exec(EngineCommand::SetPageSetup { width_pt, height_pt, units });
                Ok(json!({ "page_size_set": ok }))
            }
            "attract_repel_path" => {
                let target = self.resolve_target_id(args)?;
                let cx = args.get("cx").and_then(Value::as_f64).unwrap_or(100.0);
                let cy = args.get("cy").and_then(Value::as_f64).unwrap_or(100.0);
                let center = Point::new(cx, cy).map_err(|e| e.to_string())?;
                let radius_pt = F64::new(args.get("radius_pt").and_then(Value::as_f64).unwrap_or(60.0)).map_err(|e| e.to_string())?;
                let strength = F64::new(args.get("strength").and_then(Value::as_f64).unwrap_or(0.4)).map_err(|e| e.to_string())?;
                let ok = self.exec(EngineCommand::AttractRepelObjectPath { object_id: target, center, radius_pt, strength });
                Ok(json!({ "attracted_repelled": ok, "object_id": target.0 }))
            }
            "duplicate_page" => {
                let page_index = args.get("page_index").and_then(Value::as_u64).map_or_else(|| self.engine.current_page_index(), |v| v as usize);
                let ok = self.exec(EngineCommand::DuplicatePage { page_index });
                Ok(json!({ "page_duplicated": ok, "pages": self.engine.document.pages.len(), "active_page_index": self.engine.current_page_index() }))
            }
            "rename_page" => {
                let page_index = args.get("page_index").and_then(Value::as_u64).map_or_else(|| self.engine.current_page_index(), |v| v as usize);
                let name = args.get("name").and_then(Value::as_str).unwrap_or("Page").to_owned();
                let ok = self.exec(EngineCommand::RenamePage { page_index, name });
                Ok(json!({ "page_renamed": ok, "page_index": page_index }))
            }
            "delete_page" => {
                let page_index = args.get("page_index").and_then(Value::as_u64).map_or_else(|| self.engine.current_page_index(), |v| v as usize);
                let ok = self.exec(EngineCommand::DeletePage { page_index });
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
                let ok = self.exec(EngineCommand::SmearObjectPath { object_id: target, origin, radius_pt, dx_pt, dy_pt });
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
                let ok = self.exec(EngineCommand::SetParagraphTextLayout { object_id: target, columns, gutter_pt, line_spacing, alignment });
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
                let ok = self.exec(EngineCommand::RoughenObjectPath { object_id: target, tooth_height_pt, frequency });
                Ok(json!({ "roughened": ok, "object_id": target.0 }))
            }
            "set_artistic_text_properties" => {
                let target = self.resolve_target_id(args)?;
                let text = args.get("text").and_then(Value::as_str).map(ToOwned::to_owned);
                let font_family = args.get("font_family").and_then(Value::as_str).map(ToOwned::to_owned);
                let size_pt = args.get("size_pt").and_then(Value::as_f64).map(F64::new).transpose().map_err(|e| e.to_string())?;
                let ok = self.exec(EngineCommand::SetArtisticTextProperties { object_id: target, text, font_family, size_pt });
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
                let ok = self.exec(EngineCommand::SmoothObjectPath { object_id: target, center, radius_pt, strength, iterations });
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
                    for (slot, val) in radii.iter_mut().zip(arr.iter()) {
                        *slot = F64::new(val.as_f64().unwrap_or(0.0).max(0.0)).map_err(|e| e.to_string())?;
                    }
                    corners_locked = args.get("corners_locked").and_then(Value::as_bool).unwrap_or(false);
                }
                let ok = self.exec(EngineCommand::UpdateRectCorners { corner_kind, radii, corners_locked });
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
                let ok = self.exec(EngineCommand::UpdateEllipseParameters { mode, start_deg, end_deg });
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
                let ok = self.exec(EngineCommand::KnifeCutObject { object_id: target, line_start, line_end, gap_pt });
                Ok(json!({ "knife_cut": ok, "object_id": target.0 }))
            }
            "crop_to_rect" => {
                let x = args.get("x").and_then(Value::as_f64).unwrap_or(0.0);
                let y = args.get("y").and_then(Value::as_f64).unwrap_or(0.0);
                let w = args.get("width").and_then(Value::as_f64).unwrap_or(100.0);
                let h = args.get("height").and_then(Value::as_f64).unwrap_or(100.0);
                let rect = Rect::new(x, y, x + w, y + h).map_err(|e| e.to_string())?;
                let ok = self.exec(EngineCommand::CropToRect { rect });
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
                let ok = self.exec(EngineCommand::UpdatePolygonParameters { mode, points, inner_radius_pt });
                Ok(json!({ "polygon_parameters_updated": ok, "selection": self.engine.selection.iter().map(|i| i.0).collect::<Vec<_>>() }))
            }
            "move_object_to_layer" => {
                let target = self.resolve_target_id(args)?;
                let target_layer_id =
                    LayerId(args.get("target_layer_id").and_then(Value::as_u64).ok_or_else(|| "Missing target_layer_id".to_owned())?);
                let ok = self.exec(EngineCommand::MoveObjectToLayer { object_id: target, target_layer_id });
                Ok(json!({ "moved_to_layer": ok, "object_id": target.0, "target_layer_id": target_layer_id.0 }))
            }
            "erase_object_circle" => {
                let target = self.resolve_target_id(args)?;
                let cx = args.get("center_x").and_then(Value::as_f64).unwrap_or(0.0);
                let cy = args.get("center_y").and_then(Value::as_f64).unwrap_or(0.0);
                let center = Point::new(cx, cy).map_err(|e| e.to_string())?;
                let radius_pt = F64::new(args.get("radius_pt").and_then(Value::as_f64).unwrap_or(10.0)).map_err(|e| e.to_string())?;
                let ok = self.exec(EngineCommand::EraseObjectCircle { object_id: target, center, radius_pt });
                Ok(json!({ "erased": ok, "object_id": target.0 }))
            }
            "set_snap_settings" => {
                let snap_to_objects = args.get("snap_to_objects").and_then(Value::as_bool);
                let snap_to_page = args.get("snap_to_page").and_then(Value::as_bool);
                let ok = self.exec(EngineCommand::SetSnapSettings { snap_to_objects, snap_to_page });
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
                let ok = self.exec(EngineCommand::VirtualSegmentDelete { object_id: target, element_index });
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
                let ok = self.exec(EngineCommand::SmartFillAtPoint { point, fill, outline });
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
                let ok = self.exec(EngineCommand::OffsetObjectPath { object_id: target, distance_pt, copy });
                Ok(json!({ "offset_applied": ok, "selected_id": self.engine.selection.first().map(|i| i.0) }))
            }
            "set_path_fill_rule" => {
                let target = self.resolve_target_id(args)?;
                let fill_rule = match args.get("fill_rule").and_then(Value::as_str).unwrap_or("non_zero") {
                    "even_odd" | "evenodd" => FillRule::EvenOdd,
                    _ => FillRule::NonZero,
                };
                let ok = self.exec(EngineCommand::SetPathFillRule { object_id: target, fill_rule });
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
                let ok = self.exec(EngineCommand::ApplyPaletteSwatch { channel, color: Some(color) });
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
                let ok = self.exec(EngineCommand::ApplyPaletteSwatch { channel, color: Some(color) });
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
                let ok = self.exec(EngineCommand::SwapObjectPositions { first_id, second_id });
                Ok(json!({ "swapped": ok, "first_id": first_id.0, "second_id": second_id.0 }))
            }
            "center_selection_to_page" => {
                self.apply_optional_selection(args);
                let horizontal = args.get("horizontal").and_then(Value::as_bool).unwrap_or(true);
                let vertical = args.get("vertical").and_then(Value::as_bool).unwrap_or(true);
                let ok = self.exec(EngineCommand::CenterSelectionToPage { horizontal, vertical });
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
                let ok = self.exec(EngineCommand::ApplyPaletteSwatch { channel, color: Some(color) });
                Ok(json!({ "spot_style_applied": ok, "name": name }))
            }
            "smooth_curve_node" => {
                let target = self.resolve_target_id(args)?;
                let element_index = args.get("element_index").and_then(Value::as_u64).unwrap_or(1) as usize;
                let tension = F64::new(args.get("tension").and_then(Value::as_f64).unwrap_or(0.35)).map_err(|e| e.to_string())?;
                let ok = self.exec(EngineCommand::SmoothCurveNode { object_id: target, element_index, tension });
                Ok(json!({ "node_smoothed": ok, "object_id": target.0, "element_index": element_index }))
            }
            "array_duplicate_selection" => {
                self.apply_optional_selection(args);
                let rows = args.get("rows").and_then(Value::as_u64).unwrap_or(2) as u32;
                let cols = args.get("cols").and_then(Value::as_u64).unwrap_or(2) as u32;
                let spacing_x_pt = F64::new(args.get("spacing_x_pt").and_then(Value::as_f64).unwrap_or(50.0)).map_err(|e| e.to_string())?;
                let spacing_y_pt = F64::new(args.get("spacing_y_pt").and_then(Value::as_f64).unwrap_or(50.0)).map_err(|e| e.to_string())?;
                let ok = self.exec(EngineCommand::ArrayDuplicateSelection { rows, cols, spacing_x_pt, spacing_y_pt });
                let ids: Vec<u64> = self.engine.selection.iter().map(|i| i.0).collect();
                Ok(json!({ "array_duplicated": ok, "created_ids": ids }))
            }
            "add_guideline" => {
                let horizontal = args.get("horizontal").and_then(Value::as_bool).unwrap_or(true);
                let position_pt = F64::new(args.get("position_pt").and_then(Value::as_f64).unwrap_or(100.0)).map_err(|e| e.to_string())?;
                let ok = self.exec(EngineCommand::AddGuideline { horizontal, position_pt });
                Ok(json!({ "guideline_added": ok, "horizontal": horizontal, "position_pt": position_pt.get() }))
            }
            "clear_guidelines" => {
                let ok = self.exec(EngineCommand::ClearGuidelines);
                Ok(json!({ "guidelines_cleared": ok }))
            }
            "add_polyline" => {
                let closed = args.get("closed").and_then(Value::as_bool).unwrap_or(false);
                let mut pts = Vec::new();
                if let Some(arr) = args.get("points").and_then(Value::as_array) {
                    for item in arr {
                        if let Some(pair) = item.as_array()
                            && let (Some(x), Some(y)) = (pair.first().and_then(Value::as_f64), pair.get(1).and_then(Value::as_f64))
                        {
                            pts.push(Point::new(x, y).map_err(|e| e.to_string())?);
                        }
                    }
                }
                if pts.len() < 2 {
                    pts = vec![Point::new(20.0, 20.0).map_err(|e| e.to_string())?, Point::new(120.0, 80.0).map_err(|e| e.to_string())?];
                }
                let path = build_polyline_path(&pts, closed).map_err(|e| e.to_string())?;
                self.exec(EngineCommand::AddObject { name: Some("Polyline".to_owned()), kind: ObjectKind::Path(path) });
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
                let ok = self.exec(EngineCommand::CircularDuplicateSelection { count, center, total_angle_deg, rotate_objects });
                let ids: Vec<u64> = self.engine.selection.iter().map(|i| i.0).collect();
                Ok(json!({ "circular_duplicated": ok, "created_ids": ids }))
            }
            "fit_page_to_selection" => {
                self.apply_optional_selection(args);
                let margin_pt = F64::new(args.get("margin_pt").and_then(Value::as_f64).unwrap_or(10.0)).map_err(|e| e.to_string())?;
                let ok = self.exec(EngineCommand::FitPageToSelection { margin_pt });
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
                let ok = self.exec(EngineCommand::SetOutlineCapAndJoin { cap, join, miter_limit });
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
                self.exec(EngineCommand::AddObject { name: Some("Gear Wheel".to_owned()), kind: ObjectKind::Path(path) });
                Ok(json!({ "added": "gear_wheel", "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "align_curve_nodes" => {
                let target = self.resolve_target_id(args)?;
                let element_indices: Vec<usize> = args
                    .get("element_indices")
                    .and_then(Value::as_array)
                    .map_or_else(|| vec![0, 1], |arr| arr.iter().filter_map(|v| v.as_u64().map(|n| n as usize)).collect());
                let horizontal = args.get("horizontal").and_then(Value::as_bool).unwrap_or(true);
                let vertical = args.get("vertical").and_then(Value::as_bool).unwrap_or(false);
                let ok = self.exec(EngineCommand::AlignCurveNodes { object_id: target, element_indices, horizontal, vertical });
                Ok(json!({ "nodes_aligned": ok, "object_id": target.0 }))
            }
            "shift_selection_colors" => {
                self.apply_optional_selection(args);
                let hue_delta_deg = F64::new(args.get("hue_delta_deg").and_then(Value::as_f64).unwrap_or(30.0)).map_err(|e| e.to_string())?;
                let saturation_delta = F64::new(args.get("saturation_delta").and_then(Value::as_f64).unwrap_or(0.0)).map_err(|e| e.to_string())?;
                let brightness_delta = F64::new(args.get("brightness_delta").and_then(Value::as_f64).unwrap_or(0.0)).map_err(|e| e.to_string())?;
                let ok = self.exec(EngineCommand::ShiftSelectionColors { hue_delta_deg, saturation_delta, brightness_delta });
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
                self.exec(EngineCommand::AddObject { name: Some("Block Arrow".to_owned()), kind: ObjectKind::Path(path) });
                Ok(json!({ "added": "block_arrow", "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "subdivide_path_segments" => {
                let target = self.resolve_target_id(args)?;
                let ok = self.exec(EngineCommand::SubdividePathSegments { object_id: target });
                Ok(json!({ "subdivided": ok, "object_id": target.0 }))
            }
            "convert_selection_colors" => {
                self.apply_optional_selection(args);
                let invert = args.get("invert").and_then(Value::as_bool).unwrap_or(false);
                let grayscale = args.get("grayscale").and_then(Value::as_bool).unwrap_or(true);
                let ok = self.exec(EngineCommand::ConvertSelectionColors { invert, grayscale });
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
                self.exec(EngineCommand::AddObject { name: Some("Banner Ribbon".to_owned()), kind: ObjectKind::Path(path) });
                Ok(json!({ "added": "banner_ribbon", "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "transform_curve_nodes" => {
                let target = self.resolve_target_id(args)?;
                let element_indices: Vec<usize> = args
                    .get("element_indices")
                    .and_then(Value::as_array)
                    .map_or_else(|| vec![0, 1], |arr| arr.iter().filter_map(|v| v.as_u64().map(|n| n as usize)).collect());
                let scale_x = F64::new(args.get("scale_x").and_then(Value::as_f64).unwrap_or(1.0)).map_err(|e| e.to_string())?;
                let scale_y = F64::new(args.get("scale_y").and_then(Value::as_f64).unwrap_or(1.0)).map_err(|e| e.to_string())?;
                let rotate_deg = F64::new(args.get("rotate_deg").and_then(Value::as_f64).unwrap_or(0.0)).map_err(|e| e.to_string())?;
                let ok = self.exec(EngineCommand::TransformCurveNodes { object_id: target, element_indices, scale_x, scale_y, rotate_deg });
                Ok(json!({ "nodes_transformed": ok, "object_id": target.0 }))
            }
            "select_by_color" => {
                let r = args.get("r").and_then(Value::as_u64).unwrap_or(0) as u8;
                let g = args.get("g").and_then(Value::as_u64).unwrap_or(0) as u8;
                let b = args.get("b").and_then(Value::as_u64).unwrap_or(0) as u8;
                let match_fill = args.get("match_fill").and_then(Value::as_bool).unwrap_or(true);
                let match_outline = args.get("match_outline").and_then(Value::as_bool).unwrap_or(false);
                let tolerance_u8 = args.get("tolerance_u8").and_then(Value::as_u64).unwrap_or(16).min(255) as u8;
                let ok = self.exec(EngineCommand::SelectByColor { color: Color::rgb8(r, g, b), match_fill, match_outline, tolerance_u8 });
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
                self.exec(EngineCommand::AddObject { name: Some("Flowchart Symbol".to_owned()), kind: ObjectKind::Path(path) });
                Ok(json!({ "added": "flowchart_symbol", "symbol_kind": kind_str, "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "break_path_at_node" => {
                let target = self.resolve_target_id(args)?;
                let element_index = args.get("element_index").and_then(Value::as_u64).unwrap_or(1) as usize;
                let ok = self.exec(EngineCommand::BreakPathAtNode { object_id: target, element_index });
                Ok(json!({ "path_broken": ok, "object_id": target.0, "element_index": element_index }))
            }
            "equalize_selection_size" => {
                self.apply_optional_selection(args);
                let match_width = args.get("match_width").and_then(Value::as_bool).unwrap_or(true);
                let match_height = args.get("match_height").and_then(Value::as_bool).unwrap_or(true);
                let ok = self.exec(EngineCommand::EqualizeSelectionSize { match_width, match_height });
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
                self.exec(EngineCommand::AddObject { name: Some("Rose Guilloche".to_owned()), kind: ObjectKind::Path(path) });
                Ok(json!({ "added": "rose_guilloche", "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "morph_between_objects" => {
                let start_id = ObjectId(args.get("start_id").and_then(Value::as_u64).ok_or_else(|| "Missing start_id".to_owned())?);
                let end_id = ObjectId(args.get("end_id").and_then(Value::as_u64).ok_or_else(|| "Missing end_id".to_owned())?);
                let t = F64::new(args.get("t").and_then(Value::as_f64).unwrap_or(0.5)).map_err(|e| e.to_string())?;
                let sample_count = args.get("sample_count").and_then(Value::as_u64).unwrap_or(32) as usize;
                let ok = self.exec(EngineCommand::MorphBetweenObjects { start_id, end_id, t, sample_count });
                Ok(json!({ "morphed": ok, "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "distribute_exact_gap" => {
                self.apply_optional_selection(args);
                let horizontal = args.get("horizontal").and_then(Value::as_bool).unwrap_or(true);
                let gap_pt = F64::new(args.get("gap_pt").and_then(Value::as_f64).unwrap_or(12.0)).map_err(|e| e.to_string())?;
                let ok = self.exec(EngineCommand::DistributeExactGap { horizontal, gap_pt });
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
                self.exec(EngineCommand::AddObject { name: Some("Barcode".to_owned()), kind: ObjectKind::Path(path) });
                Ok(json!({ "added": "barcode", "digits": digits, "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "divide_selection" => {
                self.apply_optional_selection(args);
                let ok = self.exec(EngineCommand::DivideSelection);
                let ids: Vec<u64> = self.engine.selection.iter().map(|i| i.0).collect();
                Ok(json!({ "divided": ok, "region_ids": ids }))
            }
            "distribute_selection_along_path" => {
                self.apply_optional_selection(args);
                let spine_object_id =
                    ObjectId(args.get("spine_object_id").and_then(Value::as_u64).ok_or_else(|| "Missing spine_object_id".to_owned())?);
                let rotate_to_tangent = args.get("rotate_to_tangent").and_then(Value::as_bool).unwrap_or(true);
                let ok = self.exec(EngineCommand::DistributeSelectionAlongPath { spine_object_id, rotate_to_tangent });
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
                let ok = self.exec(EngineCommand::RecolorArtwork {
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
                self.exec(EngineCommand::AddObject { name: Some("Perspective Grid".to_owned()), kind: ObjectKind::Path(path) });
                Ok(json!({ "added": "perspective_grid", "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "hatch_object_path" => {
                let target = self.resolve_target_id(args)?;
                let angle_deg = F64::new(args.get("angle_deg").and_then(Value::as_f64).unwrap_or(45.0)).map_err(|e| e.to_string())?;
                let spacing_pt = F64::new(args.get("spacing_pt").and_then(Value::as_f64).unwrap_or(6.0)).map_err(|e| e.to_string())?;
                let cross_hatch = args.get("cross_hatch").and_then(Value::as_bool).unwrap_or(false);
                let ok = self.exec(EngineCommand::HatchObjectPath { object_id: target, angle_deg, spacing_pt, cross_hatch });
                Ok(json!({ "hatched": ok, "object_id": target.0 }))
            }
            "create_intermediate_blend_paths" => {
                let start_id = ObjectId(args.get("start_id").and_then(Value::as_u64).ok_or_else(|| "Missing start_id".to_owned())?);
                let end_id = ObjectId(args.get("end_id").and_then(Value::as_u64).ok_or_else(|| "Missing end_id".to_owned())?);
                let steps = args.get("steps").and_then(Value::as_u64).unwrap_or(3) as u32;
                let sample_count = args.get("sample_count").and_then(Value::as_u64).unwrap_or(32) as usize;
                let ok = self.exec(EngineCommand::CreateIntermediateBlendPaths { start_id, end_id, steps, sample_count });
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
                self.exec(EngineCommand::AddObject { name: Some("Superformula Shape".to_owned()), kind: ObjectKind::Path(path) });
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
                let ok = self.exec(EngineCommand::ProjectSelectionAxonometric { plane });
                Ok(json!({ "projected_axonometric": ok, "plane": plane_str }))
            }
            "pack_selection_in_grid" => {
                self.apply_optional_selection(args);
                let ox = args.get("origin_x").and_then(Value::as_f64).unwrap_or(40.0);
                let oy = args.get("origin_y").and_then(Value::as_f64).unwrap_or(40.0);
                let origin = Point::new(ox, oy).map_err(|e| e.to_string())?;
                let cols = args.get("cols").and_then(Value::as_u64).unwrap_or(3) as u32;
                let gap_pt = F64::new(args.get("gap_pt").and_then(Value::as_f64).unwrap_or(12.0)).map_err(|e| e.to_string())?;
                let ok = self.exec(EngineCommand::PackSelectionInGrid { origin, cols, gap_pt });
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
                self.exec(EngineCommand::AddObject { name: Some("Basic Shape".to_owned()), kind: ObjectKind::Path(path) });
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
                self.exec(EngineCommand::AddObject { name: Some("L-System Path".to_owned()), kind: ObjectKind::Path(path) });
                Ok(json!({ "added": "lsystem_path", "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "exclude_selection" => {
                self.apply_optional_selection(args);
                let ok = self.exec(EngineCommand::ExcludeSelection);
                Ok(json!({ "excluded": ok, "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "resolve_selection_overlaps" => {
                self.apply_optional_selection(args);
                let min_gap_pt = F64::new(args.get("min_gap_pt").and_then(Value::as_f64).unwrap_or(8.0)).map_err(|e| e.to_string())?;
                let iterations = args.get("iterations").and_then(Value::as_u64).unwrap_or(20) as u32;
                let ok = self.exec(EngineCommand::ResolveSelectionOverlaps { min_gap_pt, iterations });
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
                            && let (Some(sx), Some(sy)) = (pair.first().and_then(Value::as_f64), pair.get(1).and_then(Value::as_f64))
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
                self.exec(EngineCommand::AddObject { name: Some("Voronoi Mosaic".to_owned()), kind: ObjectKind::Path(path) });
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
                let ok = self.exec(EngineCommand::ApplyVariableWidthStroke { object_id: target, widths_pt });
                Ok(json!({ "variable_width_stroke_applied": ok, "object_id": target.0 }))
            }
            "chaikin_smooth_path" => {
                let target = self.resolve_target_id(args)?;
                let iterations = args.get("iterations").and_then(Value::as_u64).unwrap_or(2) as u32;
                let ok = self.exec(EngineCommand::ChaikinSmoothObjectPath { object_id: target, iterations });
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
                self.exec(EngineCommand::AddObject { name: Some("Isometric 3D Box".to_owned()), kind: ObjectKind::Path(path) });
                Ok(json!({ "added": "isometric_box", "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "wave_distort_path" => {
                let target = self.resolve_target_id(args)?;
                let amplitude_pt = F64::new(args.get("amplitude_pt").and_then(Value::as_f64).unwrap_or(4.0)).map_err(|e| e.to_string())?;
                let frequency = F64::new(args.get("frequency").and_then(Value::as_f64).unwrap_or(4.0)).map_err(|e| e.to_string())?;
                let phase_deg = F64::new(args.get("phase_deg").and_then(Value::as_f64).unwrap_or(0.0)).map_err(|e| e.to_string())?;
                let ok = self.exec(EngineCommand::WaveDistortObjectPath { object_id: target, amplitude_pt, frequency, phase_deg });
                Ok(json!({ "wave_distorted": ok, "object_id": target.0 }))
            }
            "radial_scale_selection_positions" => {
                self.apply_optional_selection(args);
                let cx = args.get("center_x").and_then(Value::as_f64).unwrap_or(150.0);
                let cy = args.get("center_y").and_then(Value::as_f64).unwrap_or(150.0);
                let center = Point::new(cx, cy).map_err(|e| e.to_string())?;
                let radial_scale = F64::new(args.get("radial_scale").and_then(Value::as_f64).unwrap_or(1.2)).map_err(|e| e.to_string())?;
                let twist_deg = F64::new(args.get("twist_deg").and_then(Value::as_f64).unwrap_or(0.0)).map_err(|e| e.to_string())?;
                let ok = self.exec(EngineCommand::RadialScaleSelectionPositions { center, radial_scale, twist_deg });
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
                self.exec(EngineCommand::AddObject { name: Some("Spirograph".to_owned()), kind: ObjectKind::Path(path) });
                Ok(json!({ "added": "spirograph", "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "apply_dihedral_symmetry" => {
                let target = self.resolve_target_id(args)?;
                let cx = args.get("center_x").and_then(Value::as_f64).unwrap_or(150.0);
                let cy = args.get("center_y").and_then(Value::as_f64).unwrap_or(150.0);
                let center = Point::new(cx, cy).map_err(|e| e.to_string())?;
                let folds = args.get("folds").and_then(Value::as_u64).unwrap_or(6) as u32;
                let reflect = args.get("reflect").and_then(Value::as_bool).unwrap_or(true);
                let ok = self.exec(EngineCommand::ApplyDihedralSymmetry { object_id: target, center, folds, reflect });
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
                let ok = self.exec(EngineCommand::ScaleSelectionByDistance { center, min_scale, max_scale, falloff_radius_pt });
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
            "add_concentric_rings" => {
                let cx = args.get("cx").and_then(Value::as_f64).unwrap_or(150.0);
                let cy = args.get("cy").and_then(Value::as_f64).unwrap_or(150.0);
                let inner_radius_pt = args.get("inner_radius_pt").and_then(Value::as_f64).unwrap_or(15.0);
                let outer_radius_pt = args.get("outer_radius_pt").and_then(Value::as_f64).unwrap_or(60.0);
                let ring_count = args.get("ring_count").and_then(Value::as_u64).unwrap_or(4) as u32;
                let center = Point::new(cx, cy).map_err(|e| e.to_string())?;
                let path = build_concentric_rings_path(center, inner_radius_pt, outer_radius_pt, ring_count).map_err(|e| e.to_string())?;
                self.exec(EngineCommand::AddObject { name: Some("Concentric Rings".to_owned()), kind: ObjectKind::Path(path) });
                Ok(json!({ "added": "concentric_rings", "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "dash_object_path" => {
                let target = self.resolve_target_id(args)?;
                let mut dash_pattern_pt = Vec::new();
                if let Some(arr) = args.get("dash_pattern_pt").and_then(Value::as_array) {
                    for v in arr {
                        if let Some(d) = v.as_f64() {
                            dash_pattern_pt.push(F64::new(d).map_err(|e| e.to_string())?);
                        }
                    }
                }
                if dash_pattern_pt.is_empty() {
                    dash_pattern_pt = vec![F64::new(10.0).map_err(|e| e.to_string())?, F64::new(5.0).map_err(|e| e.to_string())?];
                }
                let ok = self.exec(EngineCommand::DashObjectPath { object_id: target, dash_pattern_pt });
                Ok(json!({ "dashed": ok, "object_id": target.0 }))
            }
            "quantize_selection_positions" => {
                self.apply_optional_selection(args);
                let grid_x_pt = F64::new(args.get("grid_x_pt").and_then(Value::as_f64).unwrap_or(10.0)).map_err(|e| e.to_string())?;
                let grid_y_pt = F64::new(args.get("grid_y_pt").and_then(Value::as_f64).unwrap_or(10.0)).map_err(|e| e.to_string())?;
                let ok = self.exec(EngineCommand::QuantizeSelectionPositions { grid_x_pt, grid_y_pt });
                Ok(json!({ "quantized": ok, "grid_x_pt": grid_x_pt.get(), "grid_y_pt": grid_y_pt.get() }))
            }
            "export_cdrx_zip_base64" => {
                let bytes = write_cdrx_zip(&self.engine.document).map_err(|e| e.to_string())?;
                let b64 = base64_encode(&bytes);
                Ok(json!({
                    "cdrx_zip_bytes": bytes.len(),
                    "cdrx_zip_base64": b64
                }))
            }
            "add_honeycomb_grid" => {
                let x = args.get("x").and_then(Value::as_f64).unwrap_or(40.0);
                let y = args.get("y").and_then(Value::as_f64).unwrap_or(40.0);
                let w = args.get("width").and_then(Value::as_f64).unwrap_or(140.0);
                let h = args.get("height").and_then(Value::as_f64).unwrap_or(100.0);
                let cell_radius_pt = args.get("cell_radius_pt").and_then(Value::as_f64).unwrap_or(18.0);
                let cell_gap_pt = args.get("cell_gap_pt").and_then(Value::as_f64).unwrap_or(2.0);
                let bounds = Rect::new(x, y, x + w, y + h).map_err(|e| e.to_string())?;
                let path = build_honeycomb_grid_path(bounds, cell_radius_pt, cell_gap_pt).map_err(|e| e.to_string())?;
                self.exec(EngineCommand::AddObject { name: Some("Honeycomb Grid".to_owned()), kind: ObjectKind::Path(path) });
                Ok(json!({ "added": "honeycomb_grid", "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "convex_hull_selection" => {
                self.apply_optional_selection(args);
                let ok = self.exec(EngineCommand::ConvexHullSelection);
                Ok(json!({ "convex_hull_created": ok, "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "arrange_selection_on_circle" => {
                self.apply_optional_selection(args);
                let cx = args.get("center_x").and_then(Value::as_f64).unwrap_or(150.0);
                let cy = args.get("center_y").and_then(Value::as_f64).unwrap_or(150.0);
                let center = Point::new(cx, cy).map_err(|e| e.to_string())?;
                let radius_pt = F64::new(args.get("radius_pt").and_then(Value::as_f64).unwrap_or(80.0)).map_err(|e| e.to_string())?;
                let start_angle_deg = F64::new(args.get("start_angle_deg").and_then(Value::as_f64).unwrap_or(0.0)).map_err(|e| e.to_string())?;
                let rotate_to_tangent = args.get("rotate_to_tangent").and_then(Value::as_bool).unwrap_or(false);
                let ok = self.exec(EngineCommand::ArrangeSelectionOnCircle { center, radius_pt, start_angle_deg, rotate_to_tangent });
                Ok(json!({ "arranged_on_circle": ok, "radius_pt": radius_pt.get() }))
            }
            "export_pdf_imposed_base64" => {
                let sw = F64::new(args.get("sheet_width_pt").and_then(Value::as_f64).unwrap_or(612.0)).map_err(|e| e.to_string())?;
                let sh = F64::new(args.get("sheet_height_pt").and_then(Value::as_f64).unwrap_or(792.0)).map_err(|e| e.to_string())?;
                let rows = args.get("rows").and_then(Value::as_u64).unwrap_or(2) as u32;
                let cols = args.get("cols").and_then(Value::as_u64).unwrap_or(2) as u32;
                let margin = F64::new(args.get("margin_pt").and_then(Value::as_f64).unwrap_or(18.0)).map_err(|e| e.to_string())?;
                let gutter = F64::new(args.get("gutter_pt").and_then(Value::as_f64).unwrap_or(12.0)).map_err(|e| e.to_string())?;
                let crop_marks = args.get("crop_marks").and_then(Value::as_bool).unwrap_or(true);
                let spec = ImpositionSpec { sheet_width_pt: sw, sheet_height_pt: sh, rows, cols, margin_pt: margin, gutter_pt: gutter, crop_marks };
                let pdf = export_pdf_imposed(&self.engine.document, &spec).map_err(|e| e.to_string())?;
                let b64 = base64_encode(&pdf);
                Ok(json!({
                    "pdf_imposed_bytes": pdf.len(),
                    "pdf_imposed_base64": b64
                }))
            }
            "add_hilbert_curve" => {
                let x = args.get("x").and_then(Value::as_f64).unwrap_or(40.0);
                let y = args.get("y").and_then(Value::as_f64).unwrap_or(40.0);
                let w = args.get("width").and_then(Value::as_f64).unwrap_or(120.0);
                let h = args.get("height").and_then(Value::as_f64).unwrap_or(120.0);
                let order = args.get("order").and_then(Value::as_u64).unwrap_or(3) as u32;
                let bounds = Rect::new(x, y, x + w, y + h).map_err(|e| e.to_string())?;
                let path = build_hilbert_curve_path(bounds, order).map_err(|e| e.to_string())?;
                self.exec(EngineCommand::AddObject { name: Some("Hilbert Curve".to_owned()), kind: ObjectKind::Path(path) });
                Ok(json!({ "added": "hilbert_curve", "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "extract_path_skeleton" => {
                let target = self.resolve_target_id(args)?;
                let sample_steps = args.get("sample_steps").and_then(Value::as_u64).unwrap_or(32) as u32;
                let ok = self.exec(EngineCommand::ExtractPathSkeleton { object_id: target, sample_steps });
                Ok(json!({ "skeleton_extracted": ok, "object_id": target.0 }))
            }
            "jitter_selection_transforms" => {
                self.apply_optional_selection(args);
                let dx_max_pt = F64::new(args.get("dx_max_pt").and_then(Value::as_f64).unwrap_or(6.0)).map_err(|e| e.to_string())?;
                let dy_max_pt = F64::new(args.get("dy_max_pt").and_then(Value::as_f64).unwrap_or(6.0)).map_err(|e| e.to_string())?;
                let angle_max_deg = F64::new(args.get("angle_max_deg").and_then(Value::as_f64).unwrap_or(15.0)).map_err(|e| e.to_string())?;
                let scale_max_pct = F64::new(args.get("scale_max_pct").and_then(Value::as_f64).unwrap_or(20.0)).map_err(|e| e.to_string())?;
                let seed = args.get("seed").and_then(Value::as_u64).unwrap_or(42);
                let ok = self.exec(EngineCommand::JitterSelectionTransforms { dx_max_pt, dy_max_pt, angle_max_deg, scale_max_pct, seed });
                Ok(json!({ "jittered": ok, "seed": seed }))
            }
            "export_cpl_palette_base64" => {
                let palette_name = args.get("palette_name").and_then(Value::as_str).unwrap_or("Document Palette").to_owned();
                let mut seen = std::collections::BTreeSet::new();
                let mut swatches = Vec::new();
                let mut record = |c: &Color| {
                    let [r, g, b] = c.to_srgb8();
                    if seen.insert([r, g, b]) {
                        swatches.push(PaletteSwatch { name: format!("#{r:02X}{g:02X}{b:02X}"), color: c.clone() });
                    }
                };
                for page in &self.engine.document.pages {
                    for layer in &page.layers {
                        for obj in &layer.objects {
                            if let Some(fill) = &obj.fill {
                                match fill {
                                    FillStyle::Uniform { color } => record(color),
                                    FillStyle::Fountain { stops, .. } => {
                                        for s in stops {
                                            record(&s.color);
                                        }
                                    }
                                    FillStyle::TwoColorPattern { fg_color, bg_color, .. } => {
                                        record(fg_color);
                                        record(bg_color);
                                    }
                                    FillStyle::Unknown { .. } => {}
                                }
                            }
                            if let Some(ol) = &obj.outline {
                                record(&ol.color);
                            }
                        }
                    }
                }
                let palette =
                    if swatches.is_empty() { Palette::default_workspace_palette() } else { Palette { name: palette_name, guid: None, swatches } };
                let bytes = serialize_binary_cpl(&palette);
                let b64 = base64_encode(&bytes);
                Ok(json!({
                    "swatch_count": palette.swatches.len(),
                    "cpl_bytes": bytes.len(),
                    "cpl_base64": b64
                }))
            }
            "add_lissajous_curve" => {
                let cx = args.get("cx").and_then(Value::as_f64).unwrap_or(150.0);
                let cy = args.get("cy").and_then(Value::as_f64).unwrap_or(150.0);
                let radius_x_pt = args.get("radius_x_pt").and_then(Value::as_f64).unwrap_or(60.0);
                let radius_y_pt = args.get("radius_y_pt").and_then(Value::as_f64).unwrap_or(45.0);
                let freq_x = args.get("freq_x").and_then(Value::as_u64).unwrap_or(3) as u32;
                let freq_y = args.get("freq_y").and_then(Value::as_u64).unwrap_or(2) as u32;
                let phase_deg = args.get("phase_deg").and_then(Value::as_f64).unwrap_or(45.0);
                let steps = args.get("steps").and_then(Value::as_u64).unwrap_or(120) as u32;
                let center = Point::new(cx, cy).map_err(|e| e.to_string())?;
                let path = build_lissajous_path(center, radius_x_pt, radius_y_pt, freq_x, freq_y, phase_deg, steps).map_err(|e| e.to_string())?;
                self.exec(EngineCommand::AddObject { name: Some("Lissajous Curve".to_owned()), kind: ObjectKind::Path(path) });
                Ok(json!({ "added": "lissajous_curve", "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "densify_object_path" => {
                let target = self.resolve_target_id(args)?;
                let max_len_pt = F64::new(args.get("max_len_pt").and_then(Value::as_f64).unwrap_or(8.0)).map_err(|e| e.to_string())?;
                let ok = self.exec(EngineCommand::DensifyObjectPath { object_id: target, max_len_pt });
                Ok(json!({ "densified": ok, "object_id": target.0, "max_len_pt": max_len_pt.get() }))
            }
            "align_selection_to_key_object" => {
                self.apply_optional_selection(args);
                let key_id_raw = args.get("key_object_id").and_then(Value::as_u64).ok_or_else(|| "Missing key_object_id".to_owned())?;
                let mode = match args.get("mode").and_then(Value::as_str).unwrap_or("center") {
                    "left" => AlignMode::Left,
                    "right" => AlignMode::Right,
                    "top" => AlignMode::Top,
                    "bottom" => AlignMode::Bottom,
                    "center_horizontal" => AlignMode::CenterHorizontal,
                    "center_vertical" => AlignMode::CenterVertical,
                    _ => AlignMode::CenterToPage,
                };
                let ok = self.exec(EngineCommand::AlignSelectionToKeyObject { key_object_id: ObjectId(key_id_raw), mode });
                Ok(json!({ "aligned_to_key": ok, "key_object_id": key_id_raw }))
            }
            "export_xml_palette" => {
                let palette_name = args.get("palette_name").and_then(Value::as_str).unwrap_or("Document Palette").to_owned();
                let mut seen = std::collections::BTreeSet::new();
                let mut swatches = Vec::new();
                let mut record = |c: &Color| {
                    let [r, g, b] = c.to_srgb8();
                    if seen.insert([r, g, b]) {
                        swatches.push(PaletteSwatch { name: format!("#{r:02X}{g:02X}{b:02X}"), color: c.clone() });
                    }
                };
                for page in &self.engine.document.pages {
                    for layer in &page.layers {
                        for obj in &layer.objects {
                            if let Some(fill) = &obj.fill {
                                match fill {
                                    FillStyle::Uniform { color } => record(color),
                                    FillStyle::Fountain { stops, .. } => {
                                        for s in stops {
                                            record(&s.color);
                                        }
                                    }
                                    FillStyle::TwoColorPattern { fg_color, bg_color, .. } => {
                                        record(fg_color);
                                        record(bg_color);
                                    }
                                    FillStyle::Unknown { .. } => {}
                                }
                            }
                            if let Some(ol) = &obj.outline {
                                record(&ol.color);
                            }
                        }
                    }
                }
                let palette =
                    if swatches.is_empty() { Palette::default_workspace_palette() } else { Palette { name: palette_name, guid: None, swatches } };
                let xml = serialize_xml_palette(&palette);
                Ok(json!({
                    "swatch_count": palette.swatches.len(),
                    "xml_palette": xml
                }))
            }
            "add_phyllotaxis_pattern" => {
                let cx = args.get("cx").and_then(Value::as_f64).unwrap_or(150.0);
                let cy = args.get("cy").and_then(Value::as_f64).unwrap_or(150.0);
                let max_radius_pt = args.get("max_radius_pt").and_then(Value::as_f64).unwrap_or(60.0);
                let seed_count = args.get("seed_count").and_then(Value::as_u64).unwrap_or(24) as u32;
                let dot_radius_pt = args.get("dot_radius_pt").and_then(Value::as_f64).unwrap_or(3.0);
                let center = Point::new(cx, cy).map_err(|e| e.to_string())?;
                let path = build_phyllotaxis_path(center, max_radius_pt, seed_count, dot_radius_pt).map_err(|e| e.to_string())?;
                self.exec(EngineCommand::AddObject { name: Some("Phyllotaxis Pattern".to_owned()), kind: ObjectKind::Path(path) });
                Ok(json!({ "added": "phyllotaxis_pattern", "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "spherical_warp_path" => {
                let target = self.resolve_target_id(args)?;
                let cx = args.get("center_x").and_then(Value::as_f64).unwrap_or(150.0);
                let cy = args.get("center_y").and_then(Value::as_f64).unwrap_or(150.0);
                let center = Point::new(cx, cy).map_err(|e| e.to_string())?;
                let radius_pt = F64::new(args.get("radius_pt").and_then(Value::as_f64).unwrap_or(60.0)).map_err(|e| e.to_string())?;
                let magnification = F64::new(args.get("magnification").and_then(Value::as_f64).unwrap_or(1.6)).map_err(|e| e.to_string())?;
                let ok = self.exec(EngineCommand::SphericalWarpObjectPath { object_id: target, center, radius_pt, magnification });
                Ok(json!({ "spherical_warped": ok, "object_id": target.0, "magnification": magnification.get() }))
            }
            "colorize_selection_by_gradient" => {
                self.apply_optional_selection(args);
                let sr = args.get("start_r").and_then(Value::as_u64).unwrap_or(240).min(255) as u8;
                let sg = args.get("start_g").and_then(Value::as_u64).unwrap_or(50).min(255) as u8;
                let sb = args.get("start_b").and_then(Value::as_u64).unwrap_or(50).min(255) as u8;
                let er = args.get("end_r").and_then(Value::as_u64).unwrap_or(40).min(255) as u8;
                let eg = args.get("end_g").and_then(Value::as_u64).unwrap_or(120).min(255) as u8;
                let eb = args.get("end_b").and_then(Value::as_u64).unwrap_or(240).min(255) as u8;
                let apply_to_fill = args.get("apply_to_fill").and_then(Value::as_bool).unwrap_or(true);
                let apply_to_outline = args.get("apply_to_outline").and_then(Value::as_bool).unwrap_or(false);
                let ok = self.exec(EngineCommand::ColorizeSelectionByGradient {
                    start_color: Color::rgb8(sr, sg, sb),
                    end_color: Color::rgb8(er, eg, eb),
                    apply_to_fill,
                    apply_to_outline,
                });
                Ok(json!({ "colorized_by_gradient": ok }))
            }
            "import_xml_palette" => {
                let xml = args.get("xml_palette").and_then(Value::as_str).ok_or_else(|| "Missing xml_palette string".to_owned())?;
                let pal = parse_xml_palette(xml).map_err(|e| e.to_string())?;
                let swatches: Vec<Value> = pal
                    .swatches
                    .iter()
                    .map(|sw| {
                        let [r, g, b] = sw.color.to_srgb8();
                        json!({
                            "name": sw.name,
                            "hex": format!("#{r:02X}{g:02X}{b:02X}"),
                            "r": r,
                            "g": g,
                            "b": b
                        })
                    })
                    .collect();
                Ok(json!({
                    "palette_name": pal.name,
                    "swatch_count": swatches.len(),
                    "swatches": swatches
                }))
            }
            "add_halftone_wave_lines" => {
                let x = args.get("x").and_then(Value::as_f64).unwrap_or(40.0);
                let y = args.get("y").and_then(Value::as_f64).unwrap_or(40.0);
                let w = args.get("width").and_then(Value::as_f64).unwrap_or(160.0);
                let h = args.get("height").and_then(Value::as_f64).unwrap_or(80.0);
                let line_count = args.get("line_count").and_then(Value::as_u64).unwrap_or(8) as u32;
                let amplitude_pt = args.get("amplitude_pt").and_then(Value::as_f64).unwrap_or(4.0);
                let frequency = args.get("frequency").and_then(Value::as_f64).unwrap_or(3.0);
                let bounds = Rect::new(x, y, x + w, y + h).map_err(|e| e.to_string())?;
                let path = build_halftone_wave_lines_path(bounds, line_count, amplitude_pt, frequency).map_err(|e| e.to_string())?;
                self.exec(EngineCommand::AddObject { name: Some("Halftone Wave Lines".to_owned()), kind: ObjectKind::Path(path) });
                Ok(json!({ "added": "halftone_wave_lines", "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "relax_object_path" => {
                let target = self.resolve_target_id(args)?;
                let iterations = args.get("iterations").and_then(Value::as_u64).unwrap_or(3) as u32;
                let lambda = F64::new(args.get("lambda").and_then(Value::as_f64).unwrap_or(0.45)).map_err(|e| e.to_string())?;
                let ok = self.exec(EngineCommand::RelaxObjectPath { object_id: target, iterations, lambda });
                Ok(json!({ "relaxed": ok, "object_id": target.0, "iterations": iterations }))
            }
            "apply_palette_swatches_to_selection" => {
                self.apply_optional_selection(args);
                let mut colors = Vec::new();
                if let Some(arr) = args.get("hex_colors").and_then(Value::as_array) {
                    for v in arr {
                        if let Some(hex) = v.as_str()
                            && let Ok(c) = Color::from_hex(hex)
                        {
                            colors.push(c);
                        }
                    }
                }
                if colors.is_empty() {
                    colors = vec![Color::rgb8(230, 57, 70), Color::rgb8(69, 123, 157), Color::rgb8(42, 157, 143)];
                }
                let apply_to_fill = args.get("apply_to_fill").and_then(Value::as_bool).unwrap_or(true);
                let apply_to_outline = args.get("apply_to_outline").and_then(Value::as_bool).unwrap_or(false);
                let ok = self.exec(EngineCommand::ApplyPaletteSwatchesToSelection { colors, apply_to_fill, apply_to_outline });
                Ok(json!({ "swatches_applied": ok }))
            }
            "export_pdf_with_prepress_base64" => {
                let crop_marks = args.get("crop_marks").and_then(Value::as_bool).unwrap_or(true);
                let registration_marks = args.get("registration_marks").and_then(Value::as_bool).unwrap_or(true);
                let bleed_pt = F64::new(args.get("bleed_pt").and_then(Value::as_f64).unwrap_or(9.0)).map_err(|e| e.to_string())?;
                let opts = PrepressOptions { crop_marks, registration_marks, bleed_pt, ..PrepressOptions::default() };
                let pdf = export_pdf_with_prepress(&self.engine.document, &opts).map_err(|e| e.to_string())?;
                let b64 = base64_encode(&pdf);
                Ok(json!({
                    "pdf_prepress_bytes": pdf.len(),
                    "pdf_prepress_base64": b64
                }))
            }
            "add_celtic_knot_ring" => {
                let cx = args.get("cx").and_then(Value::as_f64).unwrap_or(150.0);
                let cy = args.get("cy").and_then(Value::as_f64).unwrap_or(150.0);
                let radius_pt = args.get("radius_pt").and_then(Value::as_f64).unwrap_or(55.0);
                let amplitude_pt = args.get("amplitude_pt").and_then(Value::as_f64).unwrap_or(8.0);
                let lobes = args.get("lobes").and_then(Value::as_u64).unwrap_or(6) as u32;
                let steps = args.get("steps").and_then(Value::as_u64).unwrap_or(96) as u32;
                let center = Point::new(cx, cy).map_err(|e| e.to_string())?;
                let path = build_celtic_knot_ring_path(center, radius_pt, amplitude_pt, lobes, steps).map_err(|e| e.to_string())?;
                self.exec(EngineCommand::AddObject { name: Some("Celtic Knot Ring".to_owned()), kind: ObjectKind::Path(path) });
                Ok(json!({ "added": "celtic_knot_ring", "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "fit_circle_to_object_path" => {
                let target = self.resolve_target_id(args)?;
                let ok = self.exec(EngineCommand::FitCircleToObjectPath { object_id: target });
                Ok(json!({ "circle_fitted": ok, "object_id": target.0 }))
            }
            "snap_selection_to_circle_radius" => {
                self.apply_optional_selection(args);
                let cx = args.get("center_x").and_then(Value::as_f64).unwrap_or(150.0);
                let cy = args.get("center_y").and_then(Value::as_f64).unwrap_or(150.0);
                let center = Point::new(cx, cy).map_err(|e| e.to_string())?;
                let radius_pt = F64::new(args.get("radius_pt").and_then(Value::as_f64).unwrap_or(80.0)).map_err(|e| e.to_string())?;
                let ok = self.exec(EngineCommand::SnapSelectionToCircleRadius { center, radius_pt });
                Ok(json!({ "snapped_to_circle_radius": ok, "radius_pt": radius_pt.get() }))
            }
            "export_svg_with_prepress" => {
                let page_index = args.get("page_index").and_then(Value::as_u64).map_or_else(|| self.engine.current_page_index(), |idx| idx as usize);
                let crop_marks = args.get("crop_marks").and_then(Value::as_bool).unwrap_or(true);
                let registration_marks = args.get("registration_marks").and_then(Value::as_bool).unwrap_or(true);
                let bleed_pt = F64::new(args.get("bleed_pt").and_then(Value::as_f64).unwrap_or(9.0)).map_err(|e| e.to_string())?;
                let opts = PrepressOptions { crop_marks, registration_marks, bleed_pt, ..PrepressOptions::default() };
                let svg = export_svg_with_prepress(&self.engine.document, page_index, &opts).map_err(|e| e.to_string())?;
                Ok(json!({
                    "page_index": page_index,
                    "svg_prepress_len": svg.len(),
                    "svg": svg
                }))
            }
            "add_truchet_tiles" => {
                let x = args.get("x").and_then(Value::as_f64).unwrap_or(40.0);
                let y = args.get("y").and_then(Value::as_f64).unwrap_or(40.0);
                let w = args.get("width").and_then(Value::as_f64).unwrap_or(160.0);
                let h = args.get("height").and_then(Value::as_f64).unwrap_or(120.0);
                let cols = args.get("cols").and_then(Value::as_u64).unwrap_or(6) as u32;
                let rows = args.get("rows").and_then(Value::as_u64).unwrap_or(4) as u32;
                let seed = args.get("seed").and_then(Value::as_u64).unwrap_or(42);
                let bounds = Rect::new(x, y, x + w, y + h).map_err(|e| e.to_string())?;
                let path = build_truchet_tiles_path(bounds, cols, rows, seed).map_err(|e| e.to_string())?;
                self.exec(EngineCommand::AddObject { name: Some("Truchet Tiles".to_owned()), kind: ObjectKind::Path(path) });
                Ok(json!({ "added": "truchet_tiles", "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "smooth_polyline_to_cubic_beziers" => {
                let target = self.resolve_target_id(args)?;
                let tension = F64::new(args.get("tension").and_then(Value::as_f64).unwrap_or(1.0)).map_err(|e| e.to_string())?;
                let ok = self.exec(EngineCommand::SmoothPolylineToCubicBeziers { object_id: target, tension });
                Ok(json!({ "smoothed_to_cubic": ok, "object_id": target.0, "tension": tension.get() }))
            }
            "orient_selection_to_center" => {
                self.apply_optional_selection(args);
                let cx = args.get("center_x").and_then(Value::as_f64).unwrap_or(150.0);
                let cy = args.get("center_y").and_then(Value::as_f64).unwrap_or(150.0);
                let center = Point::new(cx, cy).map_err(|e| e.to_string())?;
                let angle_offset_deg = F64::new(args.get("angle_offset_deg").and_then(Value::as_f64).unwrap_or(0.0)).map_err(|e| e.to_string())?;
                let ok = self.exec(EngineCommand::OrientSelectionToCenter { center, angle_offset_deg });
                Ok(json!({ "oriented_to_center": ok, "angle_offset_deg": angle_offset_deg.get() }))
            }
            "import_cpl_palette_base64" => {
                let b64 = args.get("cpl_base64").and_then(Value::as_str).ok_or_else(|| "Missing cpl_base64 string".to_owned())?;
                let bytes = base64_decode(b64)?;
                let pal = parse_binary_cpl(&bytes).map_err(|e| e.to_string())?;
                let swatches: Vec<Value> = pal
                    .swatches
                    .iter()
                    .map(|sw| {
                        let [r, g, b] = sw.color.to_srgb8();
                        json!({
                            "name": sw.name,
                            "hex": format!("#{r:02X}{g:02X}{b:02X}"),
                            "r": r,
                            "g": g,
                            "b": b
                        })
                    })
                    .collect();
                Ok(json!({
                    "palette_name": pal.name,
                    "swatch_count": swatches.len(),
                    "swatches": swatches
                }))
            }
            "add_sierpinski_triangle" => {
                let x = args.get("x").and_then(Value::as_f64).unwrap_or(40.0);
                let y = args.get("y").and_then(Value::as_f64).unwrap_or(40.0);
                let w = args.get("width").and_then(Value::as_f64).unwrap_or(160.0);
                let h = args.get("height").and_then(Value::as_f64).unwrap_or(140.0);
                let depth = args.get("depth").and_then(Value::as_u64).unwrap_or(3) as u32;
                let bounds = Rect::new(x, y, x + w, y + h).map_err(|e| e.to_string())?;
                let path = build_sierpinski_triangle_path(bounds, depth).map_err(|e| e.to_string())?;
                self.exec(EngineCommand::AddObject { name: Some("Sierpinski Triangle".to_owned()), kind: ObjectKind::Path(path) });
                Ok(json!({ "added": "sierpinski_triangle", "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "pinch_punch_object_path" => {
                let target = self.resolve_target_id(args)?;
                let cx = args.get("center_x").and_then(Value::as_f64).unwrap_or(150.0);
                let cy = args.get("center_y").and_then(Value::as_f64).unwrap_or(150.0);
                let center = Point::new(cx, cy).map_err(|e| e.to_string())?;
                let radius_pt = F64::new(args.get("radius_pt").and_then(Value::as_f64).unwrap_or(60.0)).map_err(|e| e.to_string())?;
                let strength = F64::new(args.get("strength").and_then(Value::as_f64).unwrap_or(-0.4)).map_err(|e| e.to_string())?;
                let ok = self.exec(EngineCommand::PinchPunchObjectPath { object_id: target, center, radius_pt, strength });
                Ok(json!({ "pinched_punched": ok, "object_id": target.0, "strength": strength.get() }))
            }
            "stack_selection" => {
                self.apply_optional_selection(args);
                let horizontal = args.get("horizontal").and_then(Value::as_bool).unwrap_or(true);
                let gap_pt = F64::new(args.get("gap_pt").and_then(Value::as_f64).unwrap_or(8.0)).map_err(|e| e.to_string())?;
                let ok = self.exec(EngineCommand::StackSelection { horizontal, gap_pt });
                Ok(json!({ "stacked": ok, "horizontal": horizontal, "gap_pt": gap_pt.get() }))
            }
            "get_document_summary" => {
                let mut total_objects = 0usize;
                for page in &self.engine.document.pages {
                    for layer in &page.layers {
                        total_objects = total_objects.saturating_add(layer.objects.len());
                    }
                }
                for layer in &self.engine.document.master_layers {
                    total_objects = total_objects.saturating_add(layer.objects.len());
                }
                Ok(json!({
                    "title": self.engine.document.title,
                    "units": format!("{:?}", self.engine.document.units),
                    "page_count": self.engine.document.pages.len(),
                    "master_layer_count": self.engine.document.master_layers.len(),
                    "total_object_count": total_objects,
                    "active_page_index": self.engine.current_page_index(),
                    "selection_count": self.engine.selection.len()
                }))
            }
            "add_koch_snowflake" => {
                let cx = args.get("cx").and_then(Value::as_f64).unwrap_or(150.0);
                let cy = args.get("cy").and_then(Value::as_f64).unwrap_or(150.0);
                let radius_pt = args.get("radius_pt").and_then(Value::as_f64).unwrap_or(60.0);
                let iterations = args.get("iterations").and_then(Value::as_u64).unwrap_or(3) as u32;
                let center = Point::new(cx, cy).map_err(|e| e.to_string())?;
                let path = build_koch_snowflake_path(center, radius_pt, iterations).map_err(|e| e.to_string())?;
                self.exec(EngineCommand::AddObject { name: Some("Koch Snowflake".to_owned()), kind: ObjectKind::Path(path) });
                Ok(json!({ "added": "koch_snowflake", "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "flatten_object_path_to_polylines" => {
                let target = self.resolve_target_id(args)?;
                let tolerance_pt = F64::new(args.get("tolerance_pt").and_then(Value::as_f64).unwrap_or(0.5)).map_err(|e| e.to_string())?;
                let ok = self.exec(EngineCommand::FlattenObjectPathToPolylines { object_id: target, tolerance_pt });
                Ok(json!({ "flattened_to_polylines": ok, "object_id": target.0, "tolerance_pt": tolerance_pt.get() }))
            }
            "scale_selection_progressively" => {
                self.apply_optional_selection(args);
                let start_scale = F64::new(args.get("start_scale").and_then(Value::as_f64).unwrap_or(0.75)).map_err(|e| e.to_string())?;
                let end_scale = F64::new(args.get("end_scale").and_then(Value::as_f64).unwrap_or(1.25)).map_err(|e| e.to_string())?;
                let ok = self.exec(EngineCommand::ScaleSelectionProgressively { start_scale, end_scale });
                Ok(json!({ "scaled_progressively": ok, "start_scale": start_scale.get(), "end_scale": end_scale.get() }))
            }
            "export_coralcraft_zip_base64" => {
                let zip_bytes = save_zip(&self.engine.document).map_err(|e| e.to_string())?;
                let b64 = base64_encode(&zip_bytes);
                Ok(json!({
                    "coralcraft_zip_bytes": zip_bytes.len(),
                    "coralcraft_zip_base64": b64
                }))
            }
            "add_dragon_curve" => {
                let sx = args.get("start_x").and_then(Value::as_f64).unwrap_or(60.0);
                let sy = args.get("start_y").and_then(Value::as_f64).unwrap_or(140.0);
                let ex = args.get("end_x").and_then(Value::as_f64).unwrap_or(220.0);
                let ey = args.get("end_y").and_then(Value::as_f64).unwrap_or(140.0);
                let iterations = args.get("iterations").and_then(Value::as_u64).unwrap_or(6) as u32;
                let start = Point::new(sx, sy).map_err(|e| e.to_string())?;
                let end = Point::new(ex, ey).map_err(|e| e.to_string())?;
                let path = build_dragon_curve_path(start, end, iterations).map_err(|e| e.to_string())?;
                self.exec(EngineCommand::AddObject { name: Some("Dragon Curve".to_owned()), kind: ObjectKind::Path(path) });
                Ok(json!({ "added": "dragon_curve", "selected_id": self.engine.selection.first().map(|id| id.0) }))
            }
            "shear_warp_object_path" => {
                let target = self.resolve_target_id(args)?;
                let shear_x_pt = F64::new(args.get("shear_x_pt").and_then(Value::as_f64).unwrap_or(12.0)).map_err(|e| e.to_string())?;
                let shear_y_pt = F64::new(args.get("shear_y_pt").and_then(Value::as_f64).unwrap_or(0.0)).map_err(|e| e.to_string())?;
                let ok = self.exec(EngineCommand::ShearWarpObjectPath { object_id: target, shear_x_pt, shear_y_pt });
                Ok(json!({ "shear_warped": ok, "object_id": target.0, "shear_x_pt": shear_x_pt.get(), "shear_y_pt": shear_y_pt.get() }))
            }
            "rotate_selection_progressively" => {
                self.apply_optional_selection(args);
                let start_angle_deg = F64::new(args.get("start_angle_deg").and_then(Value::as_f64).unwrap_or(-15.0)).map_err(|e| e.to_string())?;
                let end_angle_deg = F64::new(args.get("end_angle_deg").and_then(Value::as_f64).unwrap_or(45.0)).map_err(|e| e.to_string())?;
                let ok = self.exec(EngineCommand::RotateSelectionProgressively { start_angle_deg, end_angle_deg });
                Ok(json!({ "rotated_progressively": ok, "start_angle_deg": start_angle_deg.get(), "end_angle_deg": end_angle_deg.get() }))
            }
            "import_coralcraft_zip_base64" => {
                let b64 =
                    args.get("coralcraft_zip_base64").and_then(Value::as_str).ok_or_else(|| "Missing coralcraft_zip_base64 string".to_owned())?;
                let bytes = base64_decode(b64)?;
                let doc = load_bytes(&bytes).map_err(|e| e.to_string())?;
                let page_count = doc.pages.len();
                let mut total_objects = 0usize;
                for page in &doc.pages {
                    for layer in &page.layers {
                        total_objects = total_objects.saturating_add(layer.objects.len());
                    }
                }
                self.engine = EditorEngine::new(doc);
                Ok(json!({
                    "imported_coralcraft": true,
                    "page_count": page_count,
                    "total_object_count": total_objects
                }))
            }
            "delete_selection" => {
                self.apply_optional_selection(args);
                let ok = self.exec(EngineCommand::DeleteSelection);
                let ids: Vec<u64> = self.engine.selection.iter().map(|id| id.0).collect();
                Ok(json!({ "deleted": ok, "remaining_selection": ids }))
            }
            "translate_selection" => {
                self.apply_optional_selection(args);
                let dx_pt = F64::new(args.get("dx_pt").and_then(Value::as_f64).unwrap_or(10.0)).map_err(|e| e.to_string())?;
                let dy_pt = F64::new(args.get("dy_pt").and_then(Value::as_f64).unwrap_or(10.0)).map_err(|e| e.to_string())?;
                let ok = self.exec(EngineCommand::TranslateSelection { dx_pt, dy_pt });
                Ok(json!({ "translated": ok, "dx_pt": dx_pt.get(), "dy_pt": dy_pt.get() }))
            }
            "move_bezier_node" => {
                let target = self.resolve_target_id(args)?;
                let element_index = args.get("element_index").and_then(Value::as_u64).unwrap_or(0) as usize;
                let x = args.get("x").and_then(Value::as_f64).unwrap_or(50.0);
                let y = args.get("y").and_then(Value::as_f64).unwrap_or(50.0);
                let new_pt = Point::new(x, y).map_err(|e| e.to_string())?;
                let ok = self.exec(EngineCommand::MoveBezierNode { object_id: target, element_index, new_pt });
                Ok(json!({ "node_moved": ok, "object_id": target.0, "element_index": element_index }))
            }
            "unlock_all_objects" => {
                let ok = self.exec(EngineCommand::UnlockAllObjects);
                Ok(json!({ "unlocked_all": ok }))
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
