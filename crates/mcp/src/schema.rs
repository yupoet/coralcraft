//! MCP tool registry and JSON Schema definitions (`tools/list`).

use serde_json::Value;
use std::collections::BTreeSet;
use std::sync::OnceLock;

const TOOLS_JSON: &str = r##"{
  "tools": [
    {
      "name": "new_document",
      "description": "Resets the workspace to a fresh A4 Portrait multi-page document.",
      "inputSchema": {
        "type": "object",
        "properties": {}
      }
    },
    {
      "name": "document_summary",
      "description": "Returns page count, active page, selection summary, and object counts.",
      "inputSchema": {
        "type": "object",
        "properties": {}
      }
    },
    {
      "name": "add_rectangle",
      "description": "Adds a parametric CorelDRAW rectangle (x, y, width, height, radius, corner_kind).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "x": {"type": "number"},
        "y": {"type": "number"},
        "width": {"type": "number"},
        "height": {"type": "number"},
        "radius": {"type": "number"},
        "corner_kind": {"type": "string"}
      }
      }
    },
    {
      "name": "add_ellipse",
      "description": "Adds a parametric CorelDRAW ellipse (x, y, width, height, mode, start_deg, end_deg).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "x": {"type": "number"},
        "y": {"type": "number"},
        "width": {"type": "number"},
        "height": {"type": "number"},
        "mode": {"type": "string"},
        "start_deg": {"type": "number"},
        "end_deg": {"type": "number"}
      }
      }
    },
    {
      "name": "add_polygon",
      "description": "Adds a regular polygon or star (cx, cy, radius, inner_radius, points).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "cx": {"type": "number"},
        "cy": {"type": "number"},
        "radius": {"type": "number"},
        "points": {"type": "integer"},
        "inner_radius": {"type": "number"}
      }
      }
    },
    {
      "name": "add_artistic_text",
      "description": "Adds a CorelDRAW Artistic Text object (x, y, text, font_family, size_pt).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "x": {"type": "number"},
        "y": {"type": "number"},
        "text": {"type": "string"},
        "font_family": {"type": "string"},
        "size_pt": {"type": "number"}
      }
      }
    },
    {
      "name": "add_paragraph_text",
      "description": "Adds a multi-line word-wrapped Paragraph Text frame (x, y, width, height, text, font_family, size_pt, alignment, columns, gutter_pt).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "x": {"type": "number"},
        "y": {"type": "number"},
        "width": {"type": "number"},
        "height": {"type": "number"},
        "text": {"type": "string"},
        "font_family": {"type": "string"},
        "size_pt": {"type": "number"},
        "line_spacing": {"type": "number"},
        "columns": {"type": "integer"},
        "gutter_pt": {"type": "number"},
        "alignment": {"type": "string"}
      }
      }
    },
    {
      "name": "set_style_rgb",
      "description": "Sets RGB fill or outline color on the current selection (or creation defaults).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "channel": {"type": "string"},
        "clear": {"type": "boolean"},
        "r": {"type": "integer"},
        "g": {"type": "integer"},
        "b": {"type": "integer"}
      }
      }
    },
    {
      "name": "add_page",
      "description": "Inserts a new page after the active page and activates it.",
      "inputSchema": {
        "type": "object",
        "properties": {}
      }
    },
    {
      "name": "group_selection",
      "description": "Groups >= 2 selected objects into an ObjectKind::Group (Ctrl+G).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}}
      }
      }
    },
    {
      "name": "ungroup_selection",
      "description": "Ungroups selected ObjectKind::Group nodes one level (Ctrl+U).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}}
      }
      }
    },
    {
      "name": "convert_to_curves",
      "description": "Converts selected parametric shapes into editable Bezier paths (Ctrl+Q).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}}
      }
      }
    },
    {
      "name": "reorder_selection",
      "description": "Reorders selected objects within their layer (to_front, to_back, forward_one, backward_one).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}},
        "op": {"type": "string"}
      }
      }
    },
    {
      "name": "align_selection",
      "description": "Aligns selected objects (left, right, top, bottom, center_horizontal, center_vertical, center_to_page).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}},
        "mode": {"type": "string"}
      }
      }
    },
    {
      "name": "place_inside_powerclip",
      "description": "Places selected objects inside container_id as PowerClip children (up to depth 3).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}},
        "container_id": {"type": "integer"}
      },
      "required": ["container_id"]
      }
    },
    {
      "name": "extract_powerclip",
      "description": "Extracts PowerClip children from container_id back into the active layer.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "container_id": {"type": "integer"}
      },
      "required": ["container_id"]
      }
    },
    {
      "name": "export_svg",
      "description": "Exports the specified 0-based page as clean-room SVG 1.1 markup.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "page_index": {"type": "integer"}
      }
      }
    },
    {
      "name": "export_pdf_byte_len",
      "description": "Exports the multi-page document to ISO 32000-1 PDF 1.4 and returns its byte length.",
      "inputSchema": {
        "type": "object",
        "properties": {}
      }
    },
    {
      "name": "export_cdr7_byte_len",
      "description": "Exports the multi-page document to clean-room RIFF CDR7 and returns its byte length.",
      "inputSchema": {
        "type": "object",
        "properties": {}
      }
    },
    {
      "name": "undo",
      "description": "Undoes the most recent editor command.",
      "inputSchema": {
        "type": "object",
        "properties": {}
      }
    },
    {
      "name": "redo",
      "description": "Redoes the most recently undone editor command.",
      "inputSchema": {
        "type": "object",
        "properties": {}
      }
    },
    {
      "name": "boolean_selection",
      "description": "Executes a CorelDRAW Boolean operation on selected objects (weld, trim, intersect, front_minus_back, back_minus_front).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}},
        "op": {"type": "string"}
      }
      }
    },
    {
      "name": "apply_contour",
      "description": "Applies a non-destructive Contour vector effect (steps, offset_pt, inside).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "steps": {"type": "integer"},
        "offset_pt": {"type": "number"},
        "inside": {"type": "boolean"}
      }
      }
    },
    {
      "name": "add_drop_shadow",
      "description": "Applies a non-destructive Drop Shadow vector effect (offset_x, offset_y, opacity, r, g, b).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "offset_x": {"type": "number"},
        "offset_y": {"type": "number"},
        "opacity": {"type": "number"},
        "r": {"type": "integer"},
        "g": {"type": "integer"},
        "b": {"type": "integer"}
      }
      }
    },
    {
      "name": "add_extrude",
      "description": "Applies a non-destructive Extrude 3D block vector effect (depth_pt, angle_deg).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "depth_pt": {"type": "number"},
        "angle_deg": {"type": "number"}
      }
      }
    },
    {
      "name": "create_blend",
      "description": "Creates a non-destructive Blend progression between start_id and end_id (steps).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "start_id": {"type": "integer"},
        "end_id": {"type": "integer"},
        "steps": {"type": "integer"}
      },
      "required": ["start_id", "end_id"]
      }
    },
    {
      "name": "attach_blend_spine",
      "description": "Attaches a custom Bezier spine object (spine_object_id) to a Blend effect on blend_object_id.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "blend_object_id": {"type": "integer"},
        "spine_object_id": {"type": "integer"}
      },
      "required": ["blend_object_id", "spine_object_id"]
      }
    },
    {
      "name": "apply_envelope",
      "description": "Applies a non-destructive 4-corner bilinear Envelope warp (tl_x, tl_y, tr_x, tr_y, br_x, br_y, bl_x, bl_y) to object_id.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "tl_x": {"type": "number"},
        "tl_y": {"type": "number"},
        "tr_x": {"type": "number"},
        "tr_y": {"type": "number"},
        "br_x": {"type": "number"},
        "br_y": {"type": "number"},
        "bl_x": {"type": "number"},
        "bl_y": {"type": "number"}
      }
      }
    },
    {
      "name": "apply_artistic_brush",
      "description": "Applies a non-destructive Artistic Media variable-width calligraphic brush stroke (start_width_pt, end_width_pt) to object_id.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "start_width_pt": {"type": "number"},
        "end_width_pt": {"type": "number"}
      }
      }
    },
    {
      "name": "fit_text_to_path",
      "description": "Flows an Artistic Text object (text_object_id) along a spine path object (spine_object_id) with optional offset_pt.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "text_object_id": {"type": "integer"},
        "spine_object_id": {"type": "integer"},
        "offset_pt": {"type": "number"}
      },
      "required": ["text_object_id", "spine_object_id"]
      }
    },
    {
      "name": "step_and_repeat",
      "description": "Duplicates selected objects in a parametric array (count, offset_x_pt, offset_y_pt).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}},
        "count": {"type": "integer"},
        "offset_x_pt": {"type": "number"},
        "offset_y_pt": {"type": "number"}
      }
      }
    },
    {
      "name": "add_linear_dimension",
      "description": "Adds an engineering linear dimension annotation between (x1, y1) and (x2, y2) with offset_pt and label.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "x1": {"type": "number"},
        "y1": {"type": "number"},
        "x2": {"type": "number"},
        "y2": {"type": "number"},
        "offset_pt": {"type": "number"},
        "font_size_pt": {"type": "number"},
        "label": {"type": "string"}
      }
      }
    },
    {
      "name": "distribute_selection",
      "description": "Distributes >= 3 selected objects evenly (left, center_horizontal, right, spacing_horizontal, top, center_vertical, bottom, spacing_vertical).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}},
        "mode": {"type": "string"}
      }
      }
    },
    {
      "name": "apply_perspective",
      "description": "Applies a non-destructive 4-corner 2-point vanishing projective Perspective homography (tl_x, tl_y, tr_x, tr_y, br_x, br_y, bl_x, bl_y) to object_id.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "tl_x": {"type": "number"},
        "tl_y": {"type": "number"},
        "tr_x": {"type": "number"},
        "tr_y": {"type": "number"},
        "br_x": {"type": "number"},
        "br_y": {"type": "number"},
        "bl_x": {"type": "number"},
        "bl_y": {"type": "number"}
      }
      }
    },
    {
      "name": "fillet_path_corners",
      "description": "Applies Fillet (round), Scallop (scalloped), or Chamfer (chamfered) corner modification with radius_pt to object_id.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "radius_pt": {"type": "number"},
        "corner_kind": {"type": "string"}
      }
      }
    },
    {
      "name": "expand_effects",
      "description": "Bakes live non-destructive vector effects on object_id into static vector geometry.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"}
      }
      }
    },
    {
      "name": "set_transparency",
      "description": "Applies uniform or linear fountain transparency to selected objects.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "mode": {"type": "string"},
        "angle_deg": {"type": "number"},
        "start_opacity": {"type": "number"},
        "end_opacity": {"type": "number"},
        "opacity": {"type": "number"}
      }
      }
    },
    {
      "name": "export_eps",
      "description": "Exports a document page to Encapsulated PostScript (EPSF-3.0).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "page_index": {"type": "integer"}
      }
      }
    },
    {
      "name": "export_dxf",
      "description": "Exports a document page to AutoCAD ASCII DXF (AC1009 / R12).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "page_index": {"type": "integer"}
      }
      }
    },
    {
      "name": "import_svg",
      "description": "Replaces the active document by importing an SVG 1.1 string.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "svg": {"type": "string"}
      },
      "required": ["svg"]
      }
    },
    {
      "name": "import_dxf",
      "description": "Replaces the active document by importing an AutoCAD ASCII DXF string.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "dxf": {"type": "string"}
      },
      "required": ["dxf"]
      }
    },
    {
      "name": "add_spiral",
      "description": "Adds a Symmetrical (expansion=1) or Logarithmic (expansion>1) Spiral path (A tool).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "cx": {"type": "number"},
        "cy": {"type": "number"},
        "outer_radius": {"type": "number"},
        "revolutions": {"type": "integer"},
        "expansion": {"type": "number"}
      }
      }
    },
    {
      "name": "add_graph_paper",
      "description": "Adds a Graph Paper grid of columns x rows closed cells (D tool).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "x": {"type": "number"},
        "y": {"type": "number"},
        "width": {"type": "number"},
        "height": {"type": "number"},
        "columns": {"type": "integer"},
        "rows": {"type": "integer"}
      }
      }
    },
    {
      "name": "copy_properties_from",
      "description": "Copies fill, outline, and/or vector effects from source_id to selected objects (Edit -> Copy Properties From).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}},
        "source_id": {"type": "integer"},
        "copy_fill": {"type": "boolean"},
        "copy_outline": {"type": "boolean"},
        "copy_effects": {"type": "boolean"}
      },
      "required": ["source_id"]
      }
    },
    {
      "name": "export_pdf_imposed_byte_len",
      "description": "Exports an N-Up imposed press sheet (rows x cols with margins, gutters, and crop marks) to PDF 1.4.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "sheet_width_pt": {"type": "number"},
        "sheet_height_pt": {"type": "number"},
        "rows": {"type": "integer"},
        "cols": {"type": "integer"},
        "margin_pt": {"type": "number"},
        "gutter_pt": {"type": "number"},
        "crop_marks": {"type": "boolean"}
      }
      }
    },
    {
      "name": "add_connector",
      "description": "Adds a straight or orthogonal (Manhattan Z-elbow) connector line with optional corner_radius.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "x1": {"type": "number"},
        "y1": {"type": "number"},
        "x2": {"type": "number"},
        "y2": {"type": "number"},
        "orthogonal": {"type": "boolean"},
        "corner_radius": {"type": "number"}
      }
      }
    },
    {
      "name": "add_callout",
      "description": "Adds a closed rectangular callout box with a pointer tail directed at (target_x, target_y).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "x": {"type": "number"},
        "y": {"type": "number"},
        "width": {"type": "number"},
        "height": {"type": "number"},
        "target_x": {"type": "number"},
        "target_y": {"type": "number"},
        "tail_width": {"type": "number"}
      }
      }
    },
    {
      "name": "combine_selection",
      "description": "Combines >= 2 selected objects into a single EvenOdd compound path (Ctrl+L).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}}
      }
      }
    },
    {
      "name": "break_apart_selection",
      "description": "Breaks apart compound paths into separate subpath objects (Ctrl+K).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}}
      }
      }
    },
    {
      "name": "adjust_bitmap",
      "description": "Applies brightness, contrast, BT.709 grayscale, and/or color inversion to a bitmap object.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "brightness": {"type": "number"},
        "contrast": {"type": "number"},
        "grayscale": {"type": "boolean"},
        "invert": {"type": "boolean"}
      }
      }
    },
    {
      "name": "apply_distortion",
      "description": "Applies a non-destructive PushPull, Zipper, or Twister distortion effect to object_id.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "kind": {"type": "string"},
        "amplitude": {"type": "number"},
        "frequency": {"type": "number"}
      }
      }
    },
    {
      "name": "trace_bitmap",
      "description": "Traces an ObjectKind::Bitmap into a closed vector Path (PowerTrace / Outline Trace).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "threshold": {"type": "integer"}
      }
      }
    },
    {
      "name": "mirror_selection",
      "description": "Mirrors selected objects horizontally and/or vertically around their center, optionally duplicating (copy=true).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}},
        "horizontal": {"type": "boolean"},
        "vertical": {"type": "boolean"},
        "copy": {"type": "boolean"}
      }
      }
    },
    {
      "name": "add_angular_dimension",
      "description": "Adds an engineering angular dimension annotation measuring the angle at (vx, vy) between (x1, y1) and (x2, y2) with arc_radius_pt and label.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "vx": {"type": "number"},
        "vy": {"type": "number"},
        "x1": {"type": "number"},
        "y1": {"type": "number"},
        "x2": {"type": "number"},
        "y2": {"type": "number"},
        "arc_radius_pt": {"type": "number"},
        "label": {"type": "string"},
        "font_size_pt": {"type": "number"}
      }
      }
    },
    {
      "name": "create_boundary",
      "description": "Creates a new closed union silhouette path around selected objects without deleting originals (Object -> Shaping -> Boundary).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}}
      }
      }
    },
    {
      "name": "convert_outline_to_object",
      "description": "Converts selected objects' outlines into closed filled vector paths (Object -> Convert Outline to Object / Ctrl+Shift+Q).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}}
      }
      }
    },
    {
      "name": "set_outline_dash",
      "description": "Sets outline width_pt and dash_pattern array on selected objects (F12 Outline Pen).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}},
        "width_pt": {"type": "number"},
        "r": {"type": "integer"},
        "g": {"type": "integer"},
        "b": {"type": "integer"},
        "dash_pattern": {"type": "array", "items": {"type": "number"}}
      }
      }
    },
    {
      "name": "set_fountain_fill",
      "description": "Applies a Linear, Radial, Conical, or Square Fountain gradient fill to selected objects (F11 Fountain Fill).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}},
        "angle_deg": {"type": "number"},
        "type": {"type": "string"},
        "start_r": {"type": "integer"},
        "start_g": {"type": "integer"},
        "start_b": {"type": "integer"},
        "end_r": {"type": "integer"},
        "end_g": {"type": "integer"},
        "end_b": {"type": "integer"}
      }
      }
    },
    {
      "name": "rotate_selection",
      "description": "Rotates selected objects by angle_deg around any 9-point Object Origin (origin_index 0..=8), optionally duplicating (Alt+F8).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}},
        "angle_deg": {"type": "number"},
        "origin_index": {"type": "integer"},
        "copy": {"type": "boolean"}
      }
      }
    },
    {
      "name": "scale_selection",
      "description": "Scales selected objects by (scale_x, scale_y) around any 9-point Object Origin (origin_index 0..=8), optionally duplicating (Alt+F9).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}},
        "scale_x": {"type": "number"},
        "scale_y": {"type": "number"},
        "origin_index": {"type": "integer"},
        "copy": {"type": "boolean"}
      }
      }
    },
    {
      "name": "set_outline_arrows",
      "description": "Sets start_arrow and end_arrow markers (none, triangle, open_arrow, circle, diamond) on selected objects (F12 Outline Pen).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}},
        "start_arrow": {"type": "string"},
        "end_arrow": {"type": "string"},
        "width_pt": {"type": "number"}
      }
      }
    },
    {
      "name": "skew_selection",
      "description": "Skews selected objects by (skew_x_deg, skew_y_deg) around any 9-point Object Origin (origin_index 0..=8), optionally duplicating (Alt+F11).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}},
        "skew_x_deg": {"type": "number"},
        "skew_y_deg": {"type": "number"},
        "origin_index": {"type": "integer"},
        "copy": {"type": "boolean"}
      }
      }
    },
    {
      "name": "duplicate_selection",
      "description": "Duplicates selected objects by (offset_x_pt, offset_y_pt) and selects the copies (Ctrl+D).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}},
        "offset_x_pt": {"type": "number"},
        "offset_y_pt": {"type": "number"}
      }
      }
    },
    {
      "name": "select_all",
      "description": "Selects all unlocked visible objects on the active layer (Ctrl+A).",
      "inputSchema": {
        "type": "object",
        "properties": {}
      }
    },
    {
      "name": "apply_lens",
      "description": "Applies a CorelDRAW Lens effect (invert, brighten, tinted, magnify) to selected objects (Alt+F3).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}},
        "kind": {"type": "string"},
        "rate": {"type": "number"},
        "r": {"type": "integer"},
        "g": {"type": "integer"},
        "b": {"type": "integer"},
        "factor": {"type": "number"}
      }
      }
    },
    {
      "name": "reverse_path_direction",
      "description": "Reverses the winding direction of all subpaths in an object (Shape Tool -> Reverse Subpaths).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"}
      }
      }
    },
    {
      "name": "close_path_subpaths",
      "description": "Closes all open subpaths in an object (Object -> Join Curves -> Auto-Close Curve).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"}
      }
      }
    },
    {
      "name": "set_object_locked",
      "description": "Locks or unlocks an object (Object -> Lock -> Lock / Unlock).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "locked": {"type": "boolean"}
      }
      }
    },
    {
      "name": "set_pattern_fill",
      "description": "Applies a Two-Color Pattern Fill (checkerboard, polka_dot, horizontal_stripe, vertical_stripe, diagonal_crosshatch) to selected objects.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}},
        "pattern": {"type": "string"},
        "fg_r": {"type": "integer"},
        "fg_g": {"type": "integer"},
        "fg_b": {"type": "integer"},
        "bg_r": {"type": "integer"},
        "bg_g": {"type": "integer"},
        "bg_b": {"type": "integer"},
        "tile_size_pt": {"type": "number"}
      }
      }
    },
    {
      "name": "join_open_curves",
      "description": "Joins open subpaths in an object whose endpoints lie within tolerance_pt (Object -> Join Curves / Ctrl+Shift+J).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "tolerance_pt": {"type": "number"}
      }
      }
    },
    {
      "name": "ungroup_all_selection",
      "description": "Recursively ungroups all selected groups and nested subgroups down to leaf objects (Object -> Group -> Ungroup All).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}}
      }
      }
    },
    {
      "name": "move_page",
      "description": "Moves the 0-based page at from_index to to_index.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "from_index": {"type": "integer"},
        "to_index": {"type": "integer"}
      }
      }
    },
    {
      "name": "reduce_path_nodes",
      "description": "Simplifies path subpaths in an object using Ramer-Douglas-Peucker node reduction (Shape Tool -> Reduce Nodes).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "tolerance_pt": {"type": "number"}
      }
      }
    },
    {
      "name": "set_selection_bounds",
      "description": "Sets exact position (x_pt, y_pt) and/or dimensions (width_pt, height_pt) of selected objects around any 9-point Object Origin (origin_index 0..=8).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}},
        "x_pt": {"type": "number"},
        "y_pt": {"type": "number"},
        "width_pt": {"type": "number"},
        "height_pt": {"type": "number"},
        "origin_index": {"type": "integer"}
      }
      }
    },
    {
      "name": "add_layer",
      "description": "Adds a new normal layer with the given name on the active page (Object -> Layer -> New Layer).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "name": {"type": "string"}
      }
      }
    },
    {
      "name": "set_layer_properties",
      "description": "Updates visible, locked, printable, and/or opacity on a layer (Objects Docker).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "layer_id": {"type": "integer"},
        "visible": {"type": "boolean"},
        "locked": {"type": "boolean"},
        "printable": {"type": "boolean"},
        "opacity": {"type": "number"}
      }
      }
    },
    {
      "name": "add_bspline_curve",
      "description": "Adds a smooth Catmull-Rom / B-Spline curve object passing through control points (B-Spline Tool).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "closed": {"type": "boolean"},
        "points": {"type": "array"}
      }
      }
    },
    {
      "name": "clear_transformations",
      "description": "Resets the affine transformation matrix of selected objects to identity (Object -> Transformations -> Clear Transformations).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}}
      }
      }
    },
    {
      "name": "toggle_text_kind",
      "description": "Converts a text object between Artistic Text and Paragraph Text (Text -> Convert to Paragraph/Artistic Text / Ctrl+F8).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"}
      }
      }
    },
    {
      "name": "find_and_replace_text",
      "description": "Replaces all occurrences of find with replace across all unlocked text objects (Edit -> Find and Replace -> Replace Text).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "find": {"type": "string"},
        "replace": {"type": "string"}
      }
      }
    },
    {
      "name": "add_three_point_curve",
      "description": "Adds a 3-Point Curve cubic Bezier arc passing through (through_x, through_y) at t = 0.5 between (x1, y1) and (x2, y2).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "x1": {"type": "number"},
        "y1": {"type": "number"},
        "x2": {"type": "number"},
        "y2": {"type": "number"},
        "through_x": {"type": "number"},
        "through_y": {"type": "number"}
      }
      }
    },
    {
      "name": "change_text_case",
      "description": "Changes the letter case of a text object (sentence, lowercase, uppercase, title, toggle) (Text -> Change Case / Shift+F3).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "case": {"type": "string"}
      }
      }
    },
    {
      "name": "insert_curve_node",
      "description": "Subdivides a path segment at element_index on an object, inserting a new node at t = 0.5 (Shape Tool -> Add Nodes / +).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "element_index": {"type": "integer"}
      }
      }
    },
    {
      "name": "delete_curve_node",
      "description": "Deletes a path node at element_index on an object (Shape Tool -> Delete Nodes / -).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "element_index": {"type": "integer"}
      }
      }
    },
    {
      "name": "convert_segment_to_line",
      "description": "Converts the path segment at element_index on an object into a straight LineTo segment (Shape Tool -> To Line).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "element_index": {"type": "integer"}
      }
      }
    },
    {
      "name": "convert_segment_to_curve",
      "description": "Converts the path segment at element_index on an object into a CubicTo Bezier curve with 1/3 and 2/3 handles (Shape Tool -> To Curve).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "element_index": {"type": "integer"}
      }
      }
    },
    {
      "name": "set_page_background",
      "description": "Sets or clears the canvas background color on page_index (Layout -> Page Background).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "page_index": {"type": "integer"},
        "clear": {"type": "boolean"},
        "r": {"type": "integer"},
        "g": {"type": "integer"},
        "b": {"type": "integer"}
      }
      }
    },
    {
      "name": "set_page_bleed",
      "description": "Sets the print bleed margin in points on page_index (Layout -> Page Setup -> Bleed).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "page_index": {"type": "integer"},
        "bleed_pt": {"type": "number"}
      }
      }
    },
    {
      "name": "add_pointillizer",
      "description": "Adds a compound vector halftone dot mosaic inside a bounding box (Effects -> Pointillizer).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "x": {"type": "number"},
        "y": {"type": "number"},
        "width": {"type": "number"},
        "height": {"type": "number"},
        "columns": {"type": "integer"},
        "rows": {"type": "integer"},
        "dot_scale": {"type": "number"}
      }
      }
    },
    {
      "name": "set_object_opacity",
      "description": "Updates the opacity (0.0..=1.0) of an object (Properties Docker).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "opacity": {"type": "number"}
      }
      }
    },
    {
      "name": "set_object_visible",
      "description": "Toggles visibility (Object -> Hide / Show Object) of an object.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "visible": {"type": "boolean"}
      }
      }
    },
    {
      "name": "rename_object",
      "description": "Updates the user-assigned label of an object in the Objects Docker.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "name": {"type": "string"}
      }
      }
    },
    {
      "name": "add_impact_rays",
      "description": "Adds a compound radial burst rays path around (cx, cy) from inner_radius to outer_radius (Impact Tool).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "cx": {"type": "number"},
        "cy": {"type": "number"},
        "inner_radius": {"type": "number"},
        "outer_radius": {"type": "number"},
        "ray_count": {"type": "integer"}
      }
      }
    },
    {
      "name": "delete_layer",
      "description": "Deletes layer_id from its page when the page has more than one layer (Object -> Layer -> Delete Layer).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "layer_id": {"type": "integer"}
      }
      }
    },
    {
      "name": "move_layer",
      "description": "Reorders the layer at from_index to to_index on page_index (Objects Docker).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "page_index": {"type": "integer"},
        "from_index": {"type": "integer"},
        "to_index": {"type": "integer"}
      }
      }
    },
    {
      "name": "invert_selection",
      "description": "Inverts the selection among unlocked, visible objects on the active target layer (Edit -> Select All -> Invert Selection).",
      "inputSchema": {
        "type": "object",
        "properties": {}
      }
    },
    {
      "name": "add_complex_star",
      "description": "Adds a Complex Star {n/k} Schlafli star polygon around (cx, cy) with points and sharpness (Complex Star Tool).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "cx": {"type": "number"},
        "cy": {"type": "number"},
        "radius": {"type": "number"},
        "points": {"type": "integer"},
        "sharpness": {"type": "integer"}
      }
      }
    },
    {
      "name": "set_outline_properties",
      "description": "Updates width_pt, behind_fill, and/or scale_with_object on selected objects' OutlineStyle (F12 Outline Pen).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}},
        "width_pt": {"type": "number"},
        "behind_fill": {"type": "boolean"},
        "scale_with_object": {"type": "boolean"}
      }
      }
    },
    {
      "name": "select_by_kind",
      "description": "Selects all unlocked, visible objects on the active layer matching kind (rect, ellipse, polygon, path, text, bitmap, group).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "kind": {"type": "string"}
      }
      }
    },
    {
      "name": "set_document_title",
      "description": "Updates the document title metadata (File -> Document Properties).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "title": {"type": "string"}
      }
      }
    },
    {
      "name": "twirl_path",
      "description": "Applies a localized rotational swirl around (cx, cy) within radius_pt on an object (Twirl Tool).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "cx": {"type": "number"},
        "cy": {"type": "number"},
        "radius_pt": {"type": "number"},
        "angle_deg": {"type": "number"}
      }
      }
    },
    {
      "name": "clear_effects",
      "description": "Removes all non-destructive vector effects from an object (Object -> Clear Effect).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"}
      }
      }
    },
    {
      "name": "clone_effects_from",
      "description": "Copies the non-destructive vector effect stack from source_id to selected objects (Effects -> Clone Effect).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}},
        "source_id": {"type": "integer"}
      },
      "required": ["source_id"]
      }
    },
    {
      "name": "set_page_size",
      "description": "Updates active page dimensions (width_pt, height_pt) and measurement units (mm, pt, inch, px) (Layout -> Page Setup).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "width_pt": {"type": "number"},
        "height_pt": {"type": "number"},
        "units": {"type": "string"}
      }
      }
    },
    {
      "name": "attract_repel_path",
      "description": "Radially attracts (strength > 0) or repels (strength < 0) vertices within radius_pt of (cx, cy) on an object (Attract and Repel Tool).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "cx": {"type": "number"},
        "cy": {"type": "number"},
        "radius_pt": {"type": "number"},
        "strength": {"type": "number"}
      }
      }
    },
    {
      "name": "duplicate_page",
      "description": "Duplicates the specified 0-based page (including its layers and objects) and activates the copy (Layout -> Duplicate Page).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "page_index": {"type": "integer"}
      }
      }
    },
    {
      "name": "rename_page",
      "description": "Renames the specified 0-based page tab (Layout -> Rename Page).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "page_index": {"type": "integer"},
        "name": {"type": "string"}
      }
      }
    },
    {
      "name": "delete_page",
      "description": "Deletes the specified 0-based page when document.pages.len() > 1 (Layout -> Delete Page).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "page_index": {"type": "integer"}
      }
      }
    },
    {
      "name": "smear_path",
      "description": "Directionally drags vertices within radius_pt of (ox, oy) along (dx_pt, dy_pt) on an object (Smear Tool).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "ox": {"type": "number"},
        "oy": {"type": "number"},
        "radius_pt": {"type": "number"},
        "dx_pt": {"type": "number"},
        "dy_pt": {"type": "number"}
      }
      }
    },
    {
      "name": "set_paragraph_text_layout",
      "description": "Updates columns, gutter_pt, line_spacing, and/or alignment on a ParagraphText frame (Text -> Columns / Paragraph Formatting).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "columns": {"type": "integer"},
        "gutter_pt": {"type": "number"},
        "line_spacing": {"type": "number"},
        "alignment": {"type": "string"}
      }
      }
    },
    {
      "name": "set_active_page_target",
      "description": "Switches the active page tab (target = page with page_index, master, or desktop) in the Page Navigator Bar.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "target": {"type": "string"},
        "page_index": {"type": "integer"}
      }
      }
    },
    {
      "name": "inspect_object",
      "description": "Returns detailed metadata (id, name, kind, visible, locked, opacity, bounds, effects_count) for an object.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"}
      }
      }
    },
    {
      "name": "roughen_path",
      "description": "Subdivides segments of an object with serrated zigzag normal teeth of amplitude tooth_height_pt and frequency (Roughen Tool).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "tooth_height_pt": {"type": "number"},
        "frequency": {"type": "integer"}
      }
      }
    },
    {
      "name": "set_artistic_text_properties",
      "description": "Updates text, font_family, and/or size_pt on an ArtisticText or ParagraphText object (Text -> Edit Text / Ctrl+Shift+T).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "text": {"type": "string"},
        "font_family": {"type": "string"},
        "size_pt": {"type": "number"}
      }
      }
    },
    {
      "name": "list_page_objects",
      "description": "Lists all objects on page_index with id, name, kind, layer_id, visible, locked, and opacity (Objects Docker).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "page_index": {"type": "integer"}
      }
      }
    },
    {
      "name": "list_layers",
      "description": "Lists all layers on page_index with id, name, role, visible, locked, printable, opacity, and object_count (Objects Docker).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "page_index": {"type": "integer"}
      }
      }
    },
    {
      "name": "smooth_path",
      "description": "Applies localized Jacobi Laplacian vertex smoothing within radius_pt around (center_x, center_y) with strength and iterations (Smooth Tool).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "center_x": {"type": "number"},
        "center_y": {"type": "number"},
        "radius_pt": {"type": "number"},
        "strength": {"type": "number"},
        "iterations": {"type": "integer"}
      }
      }
    },
    {
      "name": "update_rect_corners",
      "description": "Updates corner_kind (round, scalloped, chamfered) and radii_pt on selected rectangles without converting them to curves (Property Bar).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "corner_kind": {"type": "string"},
        "radius_pt": {"type": "number"},
        "corners_locked": {"type": "boolean"},
        "radii_pt": {"type": "array", "items": {"type": "number"}}
      }
      }
    },
    {
      "name": "update_ellipse_parameters",
      "description": "Updates ellipse mode (full, pie, arc) and start_deg/end_deg on selected ellipses without converting them to curves (Property Bar).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "mode": {"type": "string"},
        "start_deg": {"type": "number"},
        "end_deg": {"type": "number"}
      }
      }
    },
    {
      "name": "list_pages",
      "description": "Lists all pages in the document with index, id, name, width_pt, height_pt, bleed_pt, and layer_count (Pages Docker).",
      "inputSchema": {
        "type": "object",
        "properties": {}
      }
    },
    {
      "name": "knife_cut_object",
      "description": "Slices an object along the cut line (start_x, start_y) -> (end_x, end_y) with kerf gap_pt (Knife Tool).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "start_x": {"type": "number"},
        "start_y": {"type": "number"},
        "end_x": {"type": "number"},
        "end_y": {"type": "number"},
        "gap_pt": {"type": "number"}
      }
      }
    },
    {
      "name": "crop_to_rect",
      "description": "Crops unlocked selected objects (or all unlocked objects on the active layer) to the rectangle (x, y, width, height) (Crop Tool).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "x": {"type": "number"},
        "y": {"type": "number"},
        "width": {"type": "number"},
        "height": {"type": "number"}
      }
      }
    },
    {
      "name": "update_polygon_parameters",
      "description": "Updates polygon/star mode (polygon, star), points, and inner_radius_pt on selected polygons without converting them to curves (Property Bar).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "mode": {"type": "string"},
        "points": {"type": "integer"},
        "inner_radius_pt": {"type": "number"}
      }
      }
    },
    {
      "name": "move_object_to_layer",
      "description": "Moves object_id to target_layer_id on the active page (Objects Docker).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "target_layer_id": {"type": "integer"}
      },
      "required": ["target_layer_id"]
      }
    },
    {
      "name": "erase_object_circle",
      "description": "Subtracts a circular eraser nib of radius_pt centered at (center_x, center_y) from an object (Eraser Tool X).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "center_x": {"type": "number"},
        "center_y": {"type": "number"},
        "radius_pt": {"type": "number"}
      }
      }
    },
    {
      "name": "set_snap_settings",
      "description": "Updates snap_to_objects and/or snap_to_page preferences (View -> Snap To).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "snap_to_objects": {"type": "boolean"},
        "snap_to_page": {"type": "boolean"}
      }
      }
    },
    {
      "name": "set_viewport",
      "description": "Updates canvas zoom factor (0.05..=64.0) and pan offset (pan_x, pan_y) (Zoom / Pan Tool Z / H).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "zoom": {"type": "number"},
        "pan_x": {"type": "number"},
        "pan_y": {"type": "number"}
      }
      }
    },
    {
      "name": "set_active_tool",
      "description": "Switches the active CorelDRAW toolbox tool (pick, shape, crop, zoom, freehand, rectangle, ellipse, polygon, artistic_text, interactive_fill).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "tool": {"type": "string"}
      }
      }
    },
    {
      "name": "virtual_segment_delete",
      "description": "Removes the path segment at element_index on an object, splitting the subpath at that segment (Virtual Segment Delete Tool).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "element_index": {"type": "integer"}
      }
      }
    },
    {
      "name": "smart_fill_at_point",
      "description": "Creates a new filled Path object from the enclosed region of overlapping closed shapes containing (x, y) (Smart Fill Tool).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "x": {"type": "number"},
        "y": {"type": "number"},
        "r": {"type": "integer"},
        "g": {"type": "integer"},
        "b": {"type": "integer"}
      }
      }
    },
    {
      "name": "export_cmx1_byte_len",
      "description": "Serializes the document to a clean-room RIFF CMX1 Exchange file and returns its byte length.",
      "inputSchema": {
        "type": "object",
        "properties": {}
      }
    },
    {
      "name": "export_cdrx_zip_byte_len",
      "description": "Serializes the document to a modern CorelDRAW X4+ ZIP container (content/riffData.cdr) and returns its byte length.",
      "inputSchema": {
        "type": "object",
        "properties": {}
      }
    },
    {
      "name": "offset_object_path",
      "description": "Offsets an object's path by distance_pt (Parallel Drawing / Offset Curve), either replacing in place or creating a parallel copy.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "distance_pt": {"type": "number"},
        "copy": {"type": "boolean"}
      }
      }
    },
    {
      "name": "set_path_fill_rule",
      "description": "Updates the winding fill rule (non_zero or even_odd) of an object's path (Object Properties -> Fill).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "fill_rule": {"type": "string"}
      }
      }
    },
    {
      "name": "set_style_cmyk",
      "description": "Sets fill or outline color in CMYK (c, m, y, k in 0.0..=1.0) on selected objects or creation defaults.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "channel": {"type": "string"},
        "c": {"type": "number"},
        "m": {"type": "number"},
        "y": {"type": "number"},
        "k": {"type": "number"}
      }
      }
    },
    {
      "name": "set_style_hsb",
      "description": "Sets fill or outline color in HSB (h_deg in 0..360, s and b in 0.0..=1.0) on selected objects or creation defaults.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "channel": {"type": "string"},
        "h_deg": {"type": "number"},
        "s": {"type": "number"},
        "b": {"type": "number"}
      }
      }
    },
    {
      "name": "measure_object_geometry",
      "description": "Computes total path length (length_pt) and signed/absolute area (area_sq_pt) of an object in world points (Object Properties -> Curve).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"}
      }
      }
    },
    {
      "name": "swap_object_positions",
      "description": "Swaps the bounding-box centers of two objects (first_id, second_id).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "first_id": {"type": "integer"},
        "second_id": {"type": "integer"}
      },
      "required": ["first_id", "second_id"]
      }
    },
    {
      "name": "center_selection_to_page",
      "description": "Centers the current selection horizontally and/or vertically on the active page (Object -> Align and Distribute -> Center to Page / P).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}},
        "horizontal": {"type": "boolean"},
        "vertical": {"type": "boolean"}
      }
      }
    },
    {
      "name": "set_style_spot",
      "description": "Sets fill or outline to a named Spot color (or Registration when name is 'Registration') with tint (0.0..=1.0) and CMYK fallback.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "channel": {"type": "string"},
        "name": {"type": "string"},
        "tint": {"type": "number"},
        "c": {"type": "number"},
        "m": {"type": "number"},
        "y": {"type": "number"},
        "k": {"type": "number"}
      }
      }
    },
    {
      "name": "smooth_curve_node",
      "description": "Converts the node at element_index on an object into a smooth symmetrical C1 cubic Bezier node (Shape Tool F10 -> Smooth / Symmetrical Node).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "element_index": {"type": "integer"},
        "tension": {"type": "number"}
      }
      }
    },
    {
      "name": "array_duplicate_selection",
      "description": "Duplicates the current selection in a rows x cols 2D grid with (spacing_x_pt, spacing_y_pt) step offsets.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}},
        "rows": {"type": "integer"},
        "cols": {"type": "integer"},
        "spacing_x_pt": {"type": "number"},
        "spacing_y_pt": {"type": "number"}
      }
      }
    },
    {
      "name": "add_guideline",
      "description": "Adds a horizontal or vertical ruler guideline at position_pt on the active page's non-printing Guides layer.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "horizontal": {"type": "boolean"},
        "position_pt": {"type": "number"}
      }
      }
    },
    {
      "name": "clear_guidelines",
      "description": "Removes all guidelines from the active page's Guides layer.",
      "inputSchema": {
        "type": "object",
        "properties": {}
      }
    },
    {
      "name": "add_polyline",
      "description": "Adds an open or closed multi-point polyline Path from points [[x,y], ...] (Freehand / Polyline Tool F5).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "closed": {"type": "boolean"},
        "points": {"type": "array"}
      }
      }
    },
    {
      "name": "circular_duplicate_selection",
      "description": "Duplicates the current selection count times in a polar/radial circle around (center_x, center_y) over total_angle_deg.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}},
        "count": {"type": "integer"},
        "center_x": {"type": "number"},
        "center_y": {"type": "number"},
        "total_angle_deg": {"type": "number"},
        "rotate_objects": {"type": "boolean"}
      }
      }
    },
    {
      "name": "fit_page_to_selection",
      "description": "Resizes the active page and translates page objects so the selection (or all unlocked visible objects) is framed with margin_pt.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}},
        "margin_pt": {"type": "number"}
      }
      }
    },
    {
      "name": "set_outline_cap_join",
      "description": "Updates line cap (butt, round, square), line join (miter, round, bevel), and/or miter_limit on selected objects' OutlineStyle (F12 Outline Pen).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}},
        "cap": {"type": "string"},
        "join": {"type": "string"},
        "miter_limit": {"type": "number"}
      }
      }
    },
    {
      "name": "add_gear_wheel",
      "description": "Adds a parametric mechanical gear / cogwheel Path with teeth trapezoidal cogs and a concentric bore hole.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "cx": {"type": "number"},
        "cy": {"type": "number"},
        "outer_radius": {"type": "number"},
        "inner_radius": {"type": "number"},
        "teeth": {"type": "integer"}
      }
      }
    },
    {
      "name": "align_curve_nodes",
      "description": "Aligns the endpoints of element_indices on an object horizontally (same Y) and/or vertically (same X) to the first index (Shape Tool F10 -> Align Nodes).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "horizontal": {"type": "boolean"},
        "vertical": {"type": "boolean"}
      }
      }
    },
    {
      "name": "shift_selection_colors",
      "description": "Shifts HSB hue (hue_delta_deg), saturation (saturation_delta), and brightness (brightness_delta) across all fills, fountain stops, patterns, and outlines of the selection.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}},
        "hue_delta_deg": {"type": "number"},
        "saturation_delta": {"type": "number"},
        "brightness_delta": {"type": "number"}
      }
      }
    },
    {
      "name": "export_coralcraft_json",
      "description": "Serializes the entire multi-page Document to lossless native .coralcraft JSON.",
      "inputSchema": {
        "type": "object",
        "properties": {}
      }
    },
    {
      "name": "add_block_arrow",
      "description": "Adds a closed 7-vertex Block Arrow Path from (start_x, start_y) to (end_x, end_y) with configurable shaft and head dimensions.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "start_x": {"type": "number"},
        "start_y": {"type": "number"},
        "end_x": {"type": "number"},
        "end_y": {"type": "number"},
        "shaft_width_pt": {"type": "number"},
        "head_width_pt": {"type": "number"},
        "head_length_pt": {"type": "number"}
      }
      }
    },
    {
      "name": "subdivide_path_segments",
      "description": "Subdivides every segment of an object's path at its midpoint t = 0.5 (Shape Tool F10 -> Select All Nodes -> Add Nodes (+)).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"}
      }
      }
    },
    {
      "name": "convert_selection_colors",
      "description": "Converts all fills, fountain stops, two-color patterns, and outlines of the selection via sRGB inversion (invert) and/or BT.709 luminance grayscale (grayscale).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}},
        "invert": {"type": "boolean"},
        "grayscale": {"type": "boolean"}
      }
      }
    },
    {
      "name": "import_coralcraft_json",
      "description": "Loads a native .coralcraft JSON document string into the active EditorEngine.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "coralcraft_json": {"type": "string"}
      },
      "required": ["coralcraft_json"]
      }
    },
    {
      "name": "add_banner_ribbon",
      "description": "Adds a compound 3-part heraldic Banner / Ribbon Path inside bounds with recessed side folds and V-shaped swallowtail notches.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "x": {"type": "number"},
        "y": {"type": "number"},
        "width": {"type": "number"},
        "height": {"type": "number"},
        "fold_depth_pt": {"type": "number"},
        "swallowtail_depth_pt": {"type": "number"}
      }
      }
    },
    {
      "name": "transform_curve_nodes",
      "description": "Scales (scale_x, scale_y) and rotates (rotate_deg) the curve nodes at element_indices on an object around their centroid (Shape Tool F10).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "scale_x": {"type": "number"},
        "scale_y": {"type": "number"},
        "rotate_deg": {"type": "number"}
      }
      }
    },
    {
      "name": "select_by_color",
      "description": "Selects all unlocked visible objects on the active layer whose fill and/or outline color is within tolerance_u8 of (r, g, b).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "r": {"type": "integer"},
        "g": {"type": "integer"},
        "b": {"type": "integer"},
        "match_fill": {"type": "boolean"},
        "match_outline": {"type": "boolean"},
        "tolerance_u8": {"type": "integer"}
      }
      }
    },
    {
      "name": "export_png_base64",
      "description": "Rasterizes page_index at dpi (4x4 MSAA) to PNG and returns png_bytes length and RFC 4648 png_base64 string.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "page_index": {"type": "integer"},
        "dpi": {"type": "number"},
        "transparent_background": {"type": "boolean"}
      }
      }
    },
    {
      "name": "add_flowchart_symbol",
      "description": "Adds a closed Flowchart Symbol (process, decision, terminator, data, document) inside (x, y, width, height).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "x": {"type": "number"},
        "y": {"type": "number"},
        "width": {"type": "number"},
        "height": {"type": "number"},
        "symbol_kind": {"type": "string"}
      }
      }
    },
    {
      "name": "break_path_at_node",
      "description": "Breaks a curve at element_index on object_id, opening a closed subpath or splitting an open subpath into two (Shape Tool F10 -> Break Curve).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "element_index": {"type": "integer"}
      }
      }
    },
    {
      "name": "equalize_selection_size",
      "description": "Scales all selected unlocked objects around their centers to match the width and/or height of the last selected key object.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}},
        "match_width": {"type": "boolean"},
        "match_height": {"type": "boolean"}
      }
      }
    },
    {
      "name": "export_pdf_base64",
      "description": "Exports the entire multi-page document to ISO 32000-1 PDF 1.4 and returns pdf_bytes length and RFC 4648 pdf_base64 string.",
      "inputSchema": {
        "type": "object",
        "properties": {}
      }
    },
    {
      "name": "add_rose_guilloche",
      "description": "Adds a closed Rhodonea / Guilloche Rose Curve at (cx, cy) with radius, petals_k (1..=36), and points_per_lobe (4..=64).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "cx": {"type": "number"},
        "cy": {"type": "number"},
        "radius": {"type": "number"},
        "petals_k": {"type": "integer"},
        "points_per_lobe": {"type": "integer"}
      }
      }
    },
    {
      "name": "morph_between_objects",
      "description": "Topologically morphs start_id toward end_id at factor t (0.0..=1.0) with sample_count vertices and adds the resulting Path.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "start_id": {"type": "integer"},
        "end_id": {"type": "integer"},
        "t": {"type": "number"},
        "sample_count": {"type": "integer"}
      },
      "required": ["start_id", "end_id"]
      }
    },
    {
      "name": "distribute_exact_gap",
      "description": "Spaces all selected unlocked objects along the horizontal or vertical axis with an exact gap_pt between adjacent bounding boxes.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}},
        "horizontal": {"type": "boolean"},
        "gap_pt": {"type": "number"}
      }
      }
    },
    {
      "name": "extract_document_palette",
      "description": "Scans all objects in the document and returns the deduplicated list of colors (hex, r, g, b) used in fills, fountain stops, patterns, and outlines.",
      "inputSchema": {
        "type": "object",
        "properties": {}
      }
    },
    {
      "name": "add_barcode",
      "description": "Adds a compound vector Barcode Path inside (x, y, width, height) encoding ASCII digits with start, center, and end guard bars.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "x": {"type": "number"},
        "y": {"type": "number"},
        "width": {"type": "number"},
        "height": {"type": "number"},
        "digits": {"type": "string"}
      }
      }
    },
    {
      "name": "divide_selection",
      "description": "Splits two selected overlapping closed objects into non-overlapping regions (A \\ B, A ∩ B, B \\ A) (Object -> Shaping -> Divide).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}}
      }
      }
    },
    {
      "name": "distribute_selection_along_path",
      "description": "Positions all selected unlocked objects other than spine_object_id evenly along the normalized arc length of spine_object_id.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}},
        "spine_object_id": {"type": "integer"},
        "rotate_to_tangent": {"type": "boolean"}
      },
      "required": ["spine_object_id"]
      }
    },
    {
      "name": "recolor_artwork",
      "description": "Globally replaces colors matching (from_r, from_g, from_b) within tolerance_u8 with (to_r, to_g, to_b) across fills, gradients, patterns, and outlines.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}},
        "from_r": {"type": "integer"},
        "from_g": {"type": "integer"},
        "from_b": {"type": "integer"},
        "to_r": {"type": "integer"},
        "to_g": {"type": "integer"},
        "to_b": {"type": "integer"},
        "tolerance_u8": {"type": "integer"},
        "selection_only": {"type": "boolean"}
      }
      }
    },
    {
      "name": "add_perspective_grid",
      "description": "Adds a 2-Point Perspective Grid Path inside (x, y, width, height) with horizon_y_pt and subdivisions (2..=32).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "x": {"type": "number"},
        "y": {"type": "number"},
        "width": {"type": "number"},
        "height": {"type": "number"},
        "horizon_y_pt": {"type": "number"},
        "subdivisions": {"type": "integer"}
      }
      }
    },
    {
      "name": "hatch_object_path",
      "description": "Replaces object_id with parallel and optional cross-hatch vector engraving strokes at angle_deg and spacing_pt.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "angle_deg": {"type": "number"},
        "spacing_pt": {"type": "number"},
        "cross_hatch": {"type": "boolean"}
      }
      }
    },
    {
      "name": "create_intermediate_blend_paths",
      "description": "Generates steps (1..=32) standalone intermediate morphed vector Path objects between start_id and end_id.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "start_id": {"type": "integer"},
        "end_id": {"type": "integer"},
        "steps": {"type": "integer"},
        "sample_count": {"type": "integer"}
      },
      "required": ["start_id", "end_id"]
      }
    },
    {
      "name": "export_svg_base64",
      "description": "Exports page_index to clean-room SVG 1.1 and returns svg_bytes length and RFC 4648 svg_base64 string.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "page_index": {"type": "integer"}
      }
      }
    },
    {
      "name": "add_superformula_shape",
      "description": "Adds a closed Gielis Superformula Path centered at (cx, cy) with radius, symmetry m, exponents (n1, n2, n3), and steps.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "cx": {"type": "number"},
        "cy": {"type": "number"},
        "radius": {"type": "number"},
        "m": {"type": "number"},
        "n1": {"type": "number"},
        "n2": {"type": "number"},
        "n3": {"type": "number"},
        "steps": {"type": "integer"}
      }
      }
    },
    {
      "name": "project_selection_axonometric",
      "description": "Projects all selected unlocked objects onto one of the 30-degree isometric planes (top, left, right).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}},
        "plane": {"type": "string"}
      }
      }
    },
    {
      "name": "pack_selection_in_grid",
      "description": "Packs all selected unlocked objects into a multi-column grid starting at (origin_x, origin_y) with cols columns and gap_pt spacing.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}},
        "origin_x": {"type": "number"},
        "origin_y": {"type": "number"},
        "cols": {"type": "integer"},
        "gap_pt": {"type": "number"}
      }
      }
    },
    {
      "name": "export_eps_base64",
      "description": "Exports page_index to clean-room EPSF-3.0 Encapsulated PostScript and returns eps_bytes length and RFC 4648 eps_base64 string.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "page_index": {"type": "integer"}
      }
      }
    },
    {
      "name": "add_basic_shape",
      "description": "Adds a closed CorelDRAW Basic Shape Path (heart, cross, lightning, crescent_moon, teardrop, diamond) inside (x, y, width, height).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "x": {"type": "number"},
        "y": {"type": "number"},
        "width": {"type": "number"},
        "height": {"type": "number"},
        "shape_kind": {"type": "string"}
      }
      }
    },
    {
      "name": "add_lsystem_path",
      "description": "Adds a procedural Lindenmayer System (L-System) fractal or organic coral/branching Path starting at (origin_x, origin_y).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "origin_x": {"type": "number"},
        "origin_y": {"type": "number"},
        "axiom": {"type": "string"},
        "rule_symbol": {"type": "string"},
        "rule_replacement": {"type": "string"},
        "turn_angle_deg": {"type": "number"},
        "step_pt": {"type": "number"},
        "iterations": {"type": "integer"}
      }
      }
    },
    {
      "name": "exclude_selection",
      "description": "Computes the symmetric difference (XOR / Exclude) of >= 2 selected unlocked objects with EvenOdd fill rule.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}}
      }
      }
    },
    {
      "name": "resolve_selection_overlaps",
      "description": "Iteratively separates overlapping selected objects by force-directed AABB repulsion to maintain min_gap_pt clearance.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}},
        "min_gap_pt": {"type": "number"},
        "iterations": {"type": "integer"}
      }
      }
    },
    {
      "name": "add_voronoi_mosaic",
      "description": "Adds a bounded Voronoi stained-glass/mosaic compound path inside a rectangle with optional mortar cell_gap_pt.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "x": {"type": "number"},
        "y": {"type": "number"},
        "width": {"type": "number"},
        "height": {"type": "number"},
        "cell_gap_pt": {"type": "number"},
        "seeds": {"type": "array"}
      }
      }
    },
    {
      "name": "apply_variable_width_stroke",
      "description": "Expands an object's spine using a piecewise-linear variable-width profile (widths_pt array) into a closed filled ribbon path.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "widths_pt": {"type": "array", "items": {"type": "number"}}
      }
      }
    },
    {
      "name": "chaikin_smooth_path",
      "description": "Smooths polygonal or curve vertices of an object via Chaikin corner-cutting (1/4 and 3/4 subdivision) for iterations (1..=6).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "iterations": {"type": "integer"}
      }
      }
    },
    {
      "name": "export_dxf_base64",
      "description": "Exports a page to AutoCAD DXF (AC1009) and returns its byte length and base64-encoded payload.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "page_index": {"type": "integer"}
      }
      }
    },
    {
      "name": "add_isometric_box",
      "description": "Adds a parametric 3D isometric box compound path (Top, Left, and Right closed faces) centered at (cx, cy).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "cx": {"type": "number"},
        "cy": {"type": "number"},
        "width_pt": {"type": "number"},
        "depth_pt": {"type": "number"},
        "height_pt": {"type": "number"}
      }
      }
    },
    {
      "name": "wave_distort_path",
      "description": "Applies a sinusoidal wave distortion along the local normals of an object's path.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "amplitude_pt": {"type": "number"},
        "frequency": {"type": "number"},
        "phase_deg": {"type": "number"}
      }
      }
    },
    {
      "name": "radial_scale_selection_positions",
      "description": "Scales the radial distance of selected objects from a center point and rotates their angular positions by twist_deg.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}},
        "center_x": {"type": "number"},
        "center_y": {"type": "number"},
        "radial_scale": {"type": "number"},
        "twist_deg": {"type": "number"}
      }
      }
    },
    {
      "name": "export_cdr7_base64",
      "description": "Exports the document to RIFF CDR7 binary format and returns its byte length and base64-encoded payload.",
      "inputSchema": {
        "type": "object",
        "properties": {}
      }
    },
    {
      "name": "add_spirograph",
      "description": "Adds a parametric hypotrochoid or epitrochoid spirograph roulette path centered at (cx, cy).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "cx": {"type": "number"},
        "cy": {"type": "number"},
        "r_fixed_pt": {"type": "number"},
        "r_rolling_pt": {"type": "number"},
        "pen_offset_pt": {"type": "number"},
        "epitrochoid": {"type": "boolean"},
        "revolutions": {"type": "integer"},
        "steps": {"type": "integer"}
      }
      }
    },
    {
      "name": "apply_dihedral_symmetry",
      "description": "Replaces an object with a Cn rotational or Dn dihedral radial kaleidoscope symmetry compound path around (center_x, center_y).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "center_x": {"type": "number"},
        "center_y": {"type": "number"},
        "folds": {"type": "integer"},
        "reflect": {"type": "boolean"}
      }
      }
    },
    {
      "name": "scale_selection_by_distance",
      "description": "Scales each selected object around its own center based on its radial distance from an attractor center within falloff_radius_pt.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}},
        "center_x": {"type": "number"},
        "center_y": {"type": "number"},
        "min_scale": {"type": "number"},
        "max_scale": {"type": "number"},
        "falloff_radius_pt": {"type": "number"}
      }
      }
    },
    {
      "name": "export_cmx1_base64",
      "description": "Exports the document to Corel Presentation Exchange (RIFF CMX1) binary format and returns its byte length and base64-encoded payload.",
      "inputSchema": {
        "type": "object",
        "properties": {}
      }
    },
    {
      "name": "add_concentric_rings",
      "description": "Adds a parametric concentric bullseye/target rings compound path with EvenOdd fill rule centered at (cx, cy).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "cx": {"type": "number"},
        "cy": {"type": "number"},
        "inner_radius_pt": {"type": "number"},
        "outer_radius_pt": {"type": "number"},
        "ring_count": {"type": "integer"}
      }
      }
    },
    {
      "name": "dash_object_path",
      "description": "Splits an object's path along its arc length into discrete open dashed subpaths according to dash_pattern_pt.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "dash_pattern_pt": {"type": "array", "items": {"type": "number"}}
      }
      }
    },
    {
      "name": "quantize_selection_positions",
      "description": "Snaps each selected object's top-left bounding corner to the nearest (grid_x_pt, grid_y_pt) grid multiple.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}},
        "grid_x_pt": {"type": "number"},
        "grid_y_pt": {"type": "number"}
      }
      }
    },
    {
      "name": "export_cdrx_zip_base64",
      "description": "Exports the document to modern ZIP-based CDRX (content/riffData.cdr) format and returns its byte length and base64-encoded payload.",
      "inputSchema": {
        "type": "object",
        "properties": {}
      }
    },
    {
      "name": "add_honeycomb_grid",
      "description": "Adds a hexagonal honeycomb lattice compound path filling a rectangular region.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "x": {"type": "number"},
        "y": {"type": "number"},
        "width": {"type": "number"},
        "height": {"type": "number"},
        "cell_radius_pt": {"type": "number"},
        "cell_gap_pt": {"type": "number"}
      }
      }
    },
    {
      "name": "convex_hull_selection",
      "description": "Creates a new closed convex-hull path enclosing all world-space vertices of the current selection (Object -> Shaping -> Convex Hull).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}}
      }
      }
    },
    {
      "name": "arrange_selection_on_circle",
      "description": "Places selected objects at equal angular intervals around a circle of radius_pt centered at (center_x, center_y), optionally rotating each object to align with the tangent.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}},
        "center_x": {"type": "number"},
        "center_y": {"type": "number"},
        "radius_pt": {"type": "number"},
        "start_angle_deg": {"type": "number"},
        "rotate_to_tangent": {"type": "boolean"}
      }
      }
    },
    {
      "name": "export_pdf_imposed_base64",
      "description": "Exports the document to an N-up imposed print sheet PDF 1.4 with optional crop marks and returns its byte length and base64-encoded payload.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "sheet_width_pt": {"type": "number"},
        "sheet_height_pt": {"type": "number"},
        "rows": {"type": "integer"},
        "cols": {"type": "integer"},
        "margin_pt": {"type": "number"},
        "gutter_pt": {"type": "number"},
        "crop_marks": {"type": "boolean"}
      }
      }
    },
    {
      "name": "add_hilbert_curve",
      "description": "Adds a continuous space-filling Hilbert fractal polyline path of order 1..=5 filling a rectangular region.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "x": {"type": "number"},
        "y": {"type": "number"},
        "width": {"type": "number"},
        "height": {"type": "number"},
        "order": {"type": "integer"}
      }
      }
    },
    {
      "name": "extract_path_skeleton",
      "description": "Replaces a vector object with its internal medial-chord centerline skeleton path (Object -> Shaping -> Centerline Skeleton).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "sample_steps": {"type": "integer"}
      }
      }
    },
    {
      "name": "jitter_selection_transforms",
      "description": "Applies deterministic SplitMix64 pseudo-random translation, rotation, and uniform scale around each selected object's center.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}},
        "dx_max_pt": {"type": "number"},
        "dy_max_pt": {"type": "number"},
        "angle_max_deg": {"type": "number"},
        "scale_max_pct": {"type": "number"},
        "seed": {"type": "integer"}
      }
      }
    },
    {
      "name": "export_cpl_palette_base64",
      "description": "Exports the document's extracted color swatches to a CorelDRAW binary .cpl (0xCDDC) palette and returns its byte length and base64-encoded payload.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "palette_name": {"type": "string"}
      }
      }
    },
    {
      "name": "add_lissajous_curve",
      "description": "Adds a closed harmonic Lissajous / Bowditch curve path centered at (cx, cy).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "cx": {"type": "number"},
        "cy": {"type": "number"},
        "radius_x_pt": {"type": "number"},
        "radius_y_pt": {"type": "number"},
        "freq_x": {"type": "integer"},
        "freq_y": {"type": "integer"},
        "phase_deg": {"type": "number"},
        "steps": {"type": "integer"}
      }
      }
    },
    {
      "name": "densify_object_path",
      "description": "Densifies a vector path by subdividing every segment longer than max_len_pt into uniform sub-segments.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "max_len_pt": {"type": "number"}
      }
      }
    },
    {
      "name": "align_selection_to_key_object",
      "description": "Aligns selected objects to a stationary key_object_id according to mode (left, right, top, bottom, center_horizontal, center_vertical, center).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}},
        "key_object_id": {"type": "integer"},
        "mode": {"type": "string"}
      },
      "required": ["key_object_id"]
      }
    },
    {
      "name": "export_xml_palette",
      "description": "Exports the document's extracted color swatches to a CorelDRAW XML palette (<palette>...</palette>).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "palette_name": {"type": "string"}
      }
      }
    },
    {
      "name": "add_phyllotaxis_pattern",
      "description": "Adds a Vogel golden-angle (137.507764 deg) sunflower / phyllotaxis spiral pattern of circular seed florets.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "cx": {"type": "number"},
        "cy": {"type": "number"},
        "max_radius_pt": {"type": "number"},
        "seed_count": {"type": "integer"},
        "dot_radius_pt": {"type": "number"}
      }
      }
    },
    {
      "name": "spherical_warp_path",
      "description": "Applies a CorelDRAW Fish-Eye / Spherical Lens radial warp to a vector object within radius_pt of (center_x, center_y).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "center_x": {"type": "number"},
        "center_y": {"type": "number"},
        "radius_pt": {"type": "number"},
        "magnification": {"type": "number"}
      }
      }
    },
    {
      "name": "colorize_selection_by_gradient",
      "description": "Linearly interpolates fill and/or outline sRGB colors across selected objects in selection order from start RGB to end RGB.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}},
        "start_r": {"type": "integer"},
        "start_g": {"type": "integer"},
        "start_b": {"type": "integer"},
        "end_r": {"type": "integer"},
        "end_g": {"type": "integer"},
        "end_b": {"type": "integer"},
        "apply_to_fill": {"type": "boolean"},
        "apply_to_outline": {"type": "boolean"}
      }
      }
    },
    {
      "name": "import_xml_palette",
      "description": "Parses a CorelDRAW XML palette (<palette>...</palette>) and returns its name and decoded swatches.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "xml_palette": {"type": "string"}
      },
      "required": ["xml_palette"]
      }
    },
    {
      "name": "add_halftone_wave_lines",
      "description": "Adds a compound path of parallel sinusoidal guilloche / security wave lines spanning a rectangular region.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "x": {"type": "number"},
        "y": {"type": "number"},
        "width": {"type": "number"},
        "height": {"type": "number"},
        "line_count": {"type": "integer"},
        "amplitude_pt": {"type": "number"},
        "frequency": {"type": "number"}
      }
      }
    },
    {
      "name": "relax_object_path",
      "description": "Globally relaxes all subpaths of a vector object using shrinkage-compensated Taubin/Laplacian vertex smoothing.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "iterations": {"type": "integer"},
        "lambda": {"type": "number"}
      }
      }
    },
    {
      "name": "apply_palette_swatches_to_selection",
      "description": "Cycles a list of hex color swatches (e.g. ['#E63946', '#457B9D', '#2A9D8F']) across selected objects in selection order.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}},
        "hex_colors": {"type": "array", "items": {"type": "string"}},
        "apply_to_fill": {"type": "boolean"},
        "apply_to_outline": {"type": "boolean"}
      }
      }
    },
    {
      "name": "export_pdf_with_prepress_base64",
      "description": "Exports the document to print-ready PDF 1.4 with corner crop marks, Registration (CMYK 100/100/100/100) bullseye targets, and bleed, returning byte length and base64-encoded payload.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "crop_marks": {"type": "boolean"},
        "registration_marks": {"type": "boolean"},
        "bleed_pt": {"type": "number"}
      }
      }
    },
    {
      "name": "add_celtic_knot_ring",
      "description": "Inserts a two-strand braided Celtic knot ring compound path on the active layer.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "cx": {"type": "number"},
        "cy": {"type": "number"},
        "radius_pt": {"type": "number"},
        "amplitude_pt": {"type": "number"},
        "lobes": {"type": "integer"},
        "steps": {"type": "integer"}
      }
      }
    },
    {
      "name": "fit_circle_to_object_path",
      "description": "Replaces a path object with a least-squares Kåsa algebraic circle fitted to its world-space path points.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"}
      }
      }
    },
    {
      "name": "snap_selection_to_circle_radius",
      "description": "Translates each selected unlocked object radially from (center_x, center_y) so its center lies on the circle of radius_pt.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}},
        "center_x": {"type": "number"},
        "center_y": {"type": "number"},
        "radius_pt": {"type": "number"}
      }
      }
    },
    {
      "name": "export_svg_with_prepress",
      "description": "Exports a page as an SVG 1.1 document string with prepress corner crop marks, Registration bullseye targets, and bleed.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "page_index": {"type": "integer"},
        "crop_marks": {"type": "boolean"},
        "registration_marks": {"type": "boolean"},
        "bleed_pt": {"type": "number"}
      }
      }
    },
    {
      "name": "add_truchet_tiles",
      "description": "Inserts a deterministic Smith quarter-circle Truchet tile maze path on the active layer.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "x": {"type": "number"},
        "y": {"type": "number"},
        "width": {"type": "number"},
        "height": {"type": "number"},
        "cols": {"type": "integer"},
        "rows": {"type": "integer"},
        "seed": {"type": "integer"}
      }
      }
    },
    {
      "name": "smooth_polyline_to_cubic_beziers",
      "description": "Converts each subpath of a path object into a C1-continuous Catmull-Rom cubic Bezier curve with configurable tension.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "tension": {"type": "number"}
      }
      }
    },
    {
      "name": "orient_selection_to_center",
      "description": "Rotates each selected unlocked object around its own center to face radially away from (center_x, center_y) plus angle_offset_deg.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}},
        "center_x": {"type": "number"},
        "center_y": {"type": "number"},
        "angle_offset_deg": {"type": "number"}
      }
      }
    },
    {
      "name": "import_cpl_palette_base64",
      "description": "Parses a base64-encoded CorelDRAW binary .cpl color palette and returns its palette name and swatches.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "cpl_base64": {"type": "string"}
      },
      "required": ["cpl_base64"]
      }
    },
    {
      "name": "add_sierpinski_triangle",
      "description": "Inserts a Sierpinski gasket fractal compound path of 3^depth filled triangular subpaths on the active layer.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "x": {"type": "number"},
        "y": {"type": "number"},
        "width": {"type": "number"},
        "height": {"type": "number"},
        "depth": {"type": "integer"}
      }
      }
    },
    {
      "name": "pinch_punch_object_path",
      "description": "Applies a radial Pinch (strength < 0) or Punch (strength > 0) warp to a path object within radius_pt around (center_x, center_y).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "center_x": {"type": "number"},
        "center_y": {"type": "number"},
        "radius_pt": {"type": "number"},
        "strength": {"type": "number"}
      }
      }
    },
    {
      "name": "stack_selection",
      "description": "Packs all selected unlocked objects edge-to-edge horizontally or vertically with gap_pt separation.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}},
        "horizontal": {"type": "boolean"},
        "gap_pt": {"type": "number"}
      }
      }
    },
    {
      "name": "get_document_summary",
      "description": "Returns document-level statistics including title, units, page_count, master_layer_count, total_object_count, active_page_index, and selection_count.",
      "inputSchema": {
        "type": "object",
        "properties": {}
      }
    },
    {
      "name": "add_koch_snowflake",
      "description": "Inserts a closed Koch snowflake fractal polygon path of 3 * 4^iterations edges on the active layer.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "cx": {"type": "number"},
        "cy": {"type": "number"},
        "radius_pt": {"type": "number"},
        "iterations": {"type": "integer"}
      }
      }
    },
    {
      "name": "flatten_object_path_to_polylines",
      "description": "Flattens all quadratic and cubic Bezier curves of a path object into linear polyline subpaths within flatness tolerance_pt.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "tolerance_pt": {"type": "number"}
      }
      }
    },
    {
      "name": "scale_selection_progressively",
      "description": "Scales each selected unlocked object around its own center by a linearly interpolated scale factor from start_scale to end_scale in selection order.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}},
        "start_scale": {"type": "number"},
        "end_scale": {"type": "number"}
      }
      }
    },
    {
      "name": "export_coralcraft_zip_base64",
      "description": "Exports the document as a deterministic Stored ZIP .coralcraft container (manifest.json + document.json + content-addressed blobs), returning byte length and base64-encoded payload.",
      "inputSchema": {
        "type": "object",
        "properties": {}
      }
    },
    {
      "name": "add_dragon_curve",
      "description": "Adds a Heighway Dragon fractal curve between (start_x, start_y) and (end_x, end_y) with recursive folding iterations (1..=12).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "start_x": {"type": "number"},
        "start_y": {"type": "number"},
        "end_x": {"type": "number"},
        "end_y": {"type": "number"},
        "iterations": {"type": "integer"}
      }
      }
    },
    {
      "name": "shear_warp_object_path",
      "description": "Applies a bounding-box-normalized oblique shear warp (shear_x_pt, shear_y_pt) to the target object's path.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "shear_x_pt": {"type": "number"},
        "shear_y_pt": {"type": "number"}
      }
      }
    },
    {
      "name": "rotate_selection_progressively",
      "description": "Rotates each selected unlocked object around its own center by a linearly interpolated angle from start_angle_deg to end_angle_deg in selection order.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}},
        "start_angle_deg": {"type": "number"},
        "end_angle_deg": {"type": "number"}
      }
      }
    },
    {
      "name": "import_coralcraft_zip_base64",
      "description": "Loads a base64-encoded .coralcraft Stored ZIP container (or raw JSON document) into the active workspace.",
      "inputSchema": {
        "type": "object",
        "properties": {
        "coralcraft_zip_base64": {"type": "string"}
      },
      "required": ["coralcraft_zip_base64"]
      }
    },
    {
      "name": "delete_selection",
      "description": "Deletes all unlocked selected objects on the active layer (Edit -> Delete / Delete).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}}
      }
      }
    },
    {
      "name": "translate_selection",
      "description": "Translates all unlocked selected objects by (dx_pt, dy_pt) in points (Object -> Transformations -> Position).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "selection": {"type": "array", "items": {"type": "integer"}},
        "dx_pt": {"type": "number"},
        "dy_pt": {"type": "number"}
      }
      }
    },
    {
      "name": "move_bezier_node",
      "description": "Moves the on-curve anchor point at element_index on object_id to (x, y) in points (Shape Tool F10).",
      "inputSchema": {
        "type": "object",
        "properties": {
        "object_id": {"type": "integer"},
        "element_index": {"type": "integer"},
        "x": {"type": "number"},
        "y": {"type": "number"}
      }
      }
    },
    {
      "name": "unlock_all_objects",
      "description": "Unlocks all locked objects across all layers on the active page (Object -> Lock -> Unlock All Objects).",
      "inputSchema": {
        "type": "object",
        "properties": {}
      }
    }
  ]
}"##;

/// Returns the cached `tools/list` result payload containing all 244 MCP tool definitions.
#[must_use]
pub fn tools_list_result() -> Value {
    static CACHE: OnceLock<Value> = OnceLock::new();
    CACHE.get_or_init(|| serde_json::from_str(TOOLS_JSON).unwrap_or_else(|_| serde_json::json!({"tools": []}))).clone()
}

/// Returns `true` if `name` is a registered MCP tool in `tools/list`.
#[must_use]
pub fn is_known_tool(name: &str) -> bool {
    static NAMES: OnceLock<BTreeSet<String>> = OnceLock::new();
    let set = NAMES.get_or_init(|| {
        tools_list_result()
            .get("tools")
            .and_then(Value::as_array)
            .map(|arr| arr.iter().filter_map(|t| t.get("name").and_then(Value::as_str).map(str::to_owned)).collect())
            .unwrap_or_default()
    });
    set.contains(name)
}
