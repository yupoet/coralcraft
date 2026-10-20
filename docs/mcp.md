# CoralCraft MCP server

`coralcraft-cli mcp` runs a [Model Context Protocol](https://modelcontextprotocol.io) server on stdio
(newline-delimited JSON-RPC 2.0). The desktop binary serves the same protocol with `coralcraft --mcp`.
Agents use it to create, edit, inspect and export CoralCraft documents in an in-process, headless
engine session. Only protocol messages are written to stdout.

## Build and register

```sh
cargo build --release -p coralcraft-cli
claude mcp add coralcraft -- "$PWD/target/release/coralcraft-cli" mcp
```

Other clients use the same command in their JSON config:

```json
{"mcpServers": {"coralcraft": {"command": "/abs/path/target/release/coralcraft-cli", "args": ["mcp"]}}}
```

## Protocol

- Methods: `initialize`, `ping`, `tools/list`, `tools/call`.
- Notifications (messages without an `id`, such as `notifications/initialized` and `notifications/cancelled`) produce no response on stdio.
- The server negotiates protocol versions `2024-11-05` and `2025-03-26` (defaulting to `2024-11-05`).
- Every tool in `tools/list` publishes a complete `inputSchema` (`{"type": "object", "properties": {...}}`).
- Tool failures and engine command rejections are returned as tool results with `isError: true`.

## Known limitations

These are tracked in the project review plan and [`docs/ai-memory/04-known-issues.md`](ai-memory/04-known-issues.md):

- Boolean tools (`boolean_selection`, `divide_selection`, `exclude_selection`) give wrong geometry for many
  inputs.
- The CDR/CMX exporters (`export_cdr7_*`, `export_cdrx_zip_*`, `export_cmx1_*`) write a CoralCraft-specific
  layout that other CorelDRAW readers are not expected to open yet.

## Tool reference

<!-- BEGIN GENERATED TOOL LIST (cargo test -p coralcraft-mcp --test docs_sync) -->

244 tools, sorted by name.

| Tool | Description |
|---|---|
| `add_angular_dimension` | Adds an engineering angular dimension annotation measuring the angle at (vx, vy) between (x1, y1) and (x2, y2) with arc_radius_pt and label. |
| `add_artistic_text` | Adds a CorelDRAW Artistic Text object (x, y, text, font_family, size_pt). |
| `add_banner_ribbon` | Adds a compound 3-part heraldic Banner / Ribbon Path inside bounds with recessed side folds and V-shaped swallowtail notches. |
| `add_barcode` | Adds a compound vector Barcode Path inside (x, y, width, height) encoding ASCII digits with start, center, and end guard bars. |
| `add_basic_shape` | Adds a closed CorelDRAW Basic Shape Path (heart, cross, lightning, crescent_moon, teardrop, diamond) inside (x, y, width, height). |
| `add_block_arrow` | Adds a closed 7-vertex Block Arrow Path from (start_x, start_y) to (end_x, end_y) with configurable shaft and head dimensions. |
| `add_bspline_curve` | Adds a smooth Catmull-Rom / B-Spline curve object passing through control points (B-Spline Tool). |
| `add_callout` | Adds a closed rectangular callout box with a pointer tail directed at (target_x, target_y). |
| `add_celtic_knot_ring` | Inserts a two-strand braided Celtic knot ring compound path on the active layer. |
| `add_complex_star` | Adds a Complex Star {n/k} Schlafli star polygon around (cx, cy) with points and sharpness (Complex Star Tool). |
| `add_concentric_rings` | Adds a parametric concentric bullseye/target rings compound path with EvenOdd fill rule centered at (cx, cy). |
| `add_connector` | Adds a straight or orthogonal (Manhattan Z-elbow) connector line with optional corner_radius. |
| `add_dragon_curve` | Adds a Heighway Dragon fractal curve between (start_x, start_y) and (end_x, end_y) with recursive folding iterations (1..=12). |
| `add_drop_shadow` | Applies a non-destructive Drop Shadow vector effect (offset_x, offset_y, opacity, r, g, b). |
| `add_ellipse` | Adds a parametric CorelDRAW ellipse (x, y, width, height, mode, start_deg, end_deg). |
| `add_extrude` | Applies a non-destructive Extrude 3D block vector effect (depth_pt, angle_deg). |
| `add_flowchart_symbol` | Adds a closed Flowchart Symbol (process, decision, terminator, data, document) inside (x, y, width, height). |
| `add_gear_wheel` | Adds a parametric mechanical gear / cogwheel Path with teeth trapezoidal cogs and a concentric bore hole. |
| `add_graph_paper` | Adds a Graph Paper grid of columns x rows closed cells (D tool). |
| `add_guideline` | Adds a horizontal or vertical ruler guideline at position_pt on the active page's non-printing Guides layer. |
| `add_halftone_wave_lines` | Adds a compound path of parallel sinusoidal guilloche / security wave lines spanning a rectangular region. |
| `add_hilbert_curve` | Adds a continuous space-filling Hilbert fractal polyline path of order 1..=5 filling a rectangular region. |
| `add_honeycomb_grid` | Adds a hexagonal honeycomb lattice compound path filling a rectangular region. |
| `add_impact_rays` | Adds a compound radial burst rays path around (cx, cy) from inner_radius to outer_radius (Impact Tool). |
| `add_isometric_box` | Adds a parametric 3D isometric box compound path (Top, Left, and Right closed faces) centered at (cx, cy). |
| `add_koch_snowflake` | Inserts a closed Koch snowflake fractal polygon path of 3 * 4^iterations edges on the active layer. |
| `add_layer` | Adds a new normal layer with the given name on the active page (Object -> Layer -> New Layer). |
| `add_linear_dimension` | Adds an engineering linear dimension annotation between (x1, y1) and (x2, y2) with offset_pt and label. |
| `add_lissajous_curve` | Adds a closed harmonic Lissajous / Bowditch curve path centered at (cx, cy). |
| `add_lsystem_path` | Adds a procedural Lindenmayer System (L-System) fractal or organic coral/branching Path starting at (origin_x, origin_y). |
| `add_page` | Inserts a new page after the active page and activates it. |
| `add_paragraph_text` | Adds a multi-line word-wrapped Paragraph Text frame (x, y, width, height, text, font_family, size_pt, alignment, columns, gutter_pt). |
| `add_perspective_grid` | Adds a 2-Point Perspective Grid Path inside (x, y, width, height) with horizon_y_pt and subdivisions (2..=32). |
| `add_phyllotaxis_pattern` | Adds a Vogel golden-angle (137.507764 deg) sunflower / phyllotaxis spiral pattern of circular seed florets. |
| `add_pointillizer` | Adds a compound vector halftone dot mosaic inside a bounding box (Effects -> Pointillizer). |
| `add_polygon` | Adds a regular polygon or star (cx, cy, radius, inner_radius, points). |
| `add_polyline` | Adds an open or closed multi-point polyline Path from points [[x,y], ...] (Freehand / Polyline Tool F5). |
| `add_rectangle` | Adds a parametric CorelDRAW rectangle (x, y, width, height, radius, corner_kind). |
| `add_rose_guilloche` | Adds a closed Rhodonea / Guilloche Rose Curve at (cx, cy) with radius, petals_k (1..=36), and points_per_lobe (4..=64). |
| `add_sierpinski_triangle` | Inserts a Sierpinski gasket fractal compound path of 3^depth filled triangular subpaths on the active layer. |
| `add_spiral` | Adds a Symmetrical (expansion=1) or Logarithmic (expansion>1) Spiral path (A tool). |
| `add_spirograph` | Adds a parametric hypotrochoid or epitrochoid spirograph roulette path centered at (cx, cy). |
| `add_superformula_shape` | Adds a closed Gielis Superformula Path centered at (cx, cy) with radius, symmetry m, exponents (n1, n2, n3), and steps. |
| `add_three_point_curve` | Adds a 3-Point Curve cubic Bezier arc passing through (through_x, through_y) at t = 0.5 between (x1, y1) and (x2, y2). |
| `add_truchet_tiles` | Inserts a deterministic Smith quarter-circle Truchet tile maze path on the active layer. |
| `add_voronoi_mosaic` | Adds a bounded Voronoi stained-glass/mosaic compound path inside a rectangle with optional mortar cell_gap_pt. |
| `adjust_bitmap` | Applies brightness, contrast, BT.709 grayscale, and/or color inversion to a bitmap object. |
| `align_curve_nodes` | Aligns the endpoints of element_indices on an object horizontally (same Y) and/or vertically (same X) to the first index (Shape Tool F10 -> Align Nodes). |
| `align_selection` | Aligns selected objects (left, right, top, bottom, center_horizontal, center_vertical, center_to_page). |
| `align_selection_to_key_object` | Aligns selected objects to a stationary key_object_id according to mode (left, right, top, bottom, center_horizontal, center_vertical, center). |
| `apply_artistic_brush` | Applies a non-destructive Artistic Media variable-width calligraphic brush stroke (start_width_pt, end_width_pt) to object_id. |
| `apply_contour` | Applies a non-destructive Contour vector effect (steps, offset_pt, inside). |
| `apply_dihedral_symmetry` | Replaces an object with a Cn rotational or Dn dihedral radial kaleidoscope symmetry compound path around (center_x, center_y). |
| `apply_distortion` | Applies a non-destructive PushPull, Zipper, or Twister distortion effect to object_id. |
| `apply_envelope` | Applies a non-destructive 4-corner bilinear Envelope warp (tl_x, tl_y, tr_x, tr_y, br_x, br_y, bl_x, bl_y) to object_id. |
| `apply_lens` | Applies a CorelDRAW Lens effect (invert, brighten, tinted, magnify) to selected objects (Alt+F3). |
| `apply_palette_swatches_to_selection` | Cycles a list of hex color swatches (e.g. ['#E63946', '#457B9D', '#2A9D8F']) across selected objects in selection order. |
| `apply_perspective` | Applies a non-destructive 4-corner 2-point vanishing projective Perspective homography (tl_x, tl_y, tr_x, tr_y, br_x, br_y, bl_x, bl_y) to object_id. |
| `apply_variable_width_stroke` | Expands an object's spine using a piecewise-linear variable-width profile (widths_pt array) into a closed filled ribbon path. |
| `arrange_selection_on_circle` | Places selected objects at equal angular intervals around a circle of radius_pt centered at (center_x, center_y), optionally rotating each object to align with the tangent. |
| `array_duplicate_selection` | Duplicates the current selection in a rows x cols 2D grid with (spacing_x_pt, spacing_y_pt) step offsets. |
| `attach_blend_spine` | Attaches a custom Bezier spine object (spine_object_id) to a Blend effect on blend_object_id. |
| `attract_repel_path` | Radially attracts (strength > 0) or repels (strength < 0) vertices within radius_pt of (cx, cy) on an object (Attract and Repel Tool). |
| `boolean_selection` | Executes a CorelDRAW Boolean operation on selected objects (weld, trim, intersect, front_minus_back, back_minus_front). |
| `break_apart_selection` | Breaks apart compound paths into separate subpath objects (Ctrl+K). |
| `break_path_at_node` | Breaks a curve at element_index on object_id, opening a closed subpath or splitting an open subpath into two (Shape Tool F10 -> Break Curve). |
| `center_selection_to_page` | Centers the current selection horizontally and/or vertically on the active page (Object -> Align and Distribute -> Center to Page / P). |
| `chaikin_smooth_path` | Smooths polygonal or curve vertices of an object via Chaikin corner-cutting (1/4 and 3/4 subdivision) for iterations (1..=6). |
| `change_text_case` | Changes the letter case of a text object (sentence, lowercase, uppercase, title, toggle) (Text -> Change Case / Shift+F3). |
| `circular_duplicate_selection` | Duplicates the current selection count times in a polar/radial circle around (center_x, center_y) over total_angle_deg. |
| `clear_effects` | Removes all non-destructive vector effects from an object (Object -> Clear Effect). |
| `clear_guidelines` | Removes all guidelines from the active page's Guides layer. |
| `clear_transformations` | Resets the affine transformation matrix of selected objects to identity (Object -> Transformations -> Clear Transformations). |
| `clone_effects_from` | Copies the non-destructive vector effect stack from source_id to selected objects (Effects -> Clone Effect). |
| `close_path_subpaths` | Closes all open subpaths in an object (Object -> Join Curves -> Auto-Close Curve). |
| `colorize_selection_by_gradient` | Linearly interpolates fill and/or outline sRGB colors across selected objects in selection order from start RGB to end RGB. |
| `combine_selection` | Combines >= 2 selected objects into a single EvenOdd compound path (Ctrl+L). |
| `convert_outline_to_object` | Converts selected objects' outlines into closed filled vector paths (Object -> Convert Outline to Object / Ctrl+Shift+Q). |
| `convert_segment_to_curve` | Converts the path segment at element_index on an object into a CubicTo Bezier curve with 1/3 and 2/3 handles (Shape Tool -> To Curve). |
| `convert_segment_to_line` | Converts the path segment at element_index on an object into a straight LineTo segment (Shape Tool -> To Line). |
| `convert_selection_colors` | Converts all fills, fountain stops, two-color patterns, and outlines of the selection via sRGB inversion (invert) and/or BT.709 luminance grayscale (grayscale). |
| `convert_to_curves` | Converts selected parametric shapes into editable Bezier paths (Ctrl+Q). |
| `convex_hull_selection` | Creates a new closed convex-hull path enclosing all world-space vertices of the current selection (Object -> Shaping -> Convex Hull). |
| `copy_properties_from` | Copies fill, outline, and/or vector effects from source_id to selected objects (Edit -> Copy Properties From). |
| `create_blend` | Creates a non-destructive Blend progression between start_id and end_id (steps). |
| `create_boundary` | Creates a new closed union silhouette path around selected objects without deleting originals (Object -> Shaping -> Boundary). |
| `create_intermediate_blend_paths` | Generates steps (1..=32) standalone intermediate morphed vector Path objects between start_id and end_id. |
| `crop_to_rect` | Crops unlocked selected objects (or all unlocked objects on the active layer) to the rectangle (x, y, width, height) (Crop Tool). |
| `dash_object_path` | Splits an object's path along its arc length into discrete open dashed subpaths according to dash_pattern_pt. |
| `delete_curve_node` | Deletes a path node at element_index on an object (Shape Tool -> Delete Nodes / -). |
| `delete_layer` | Deletes layer_id from its page when the page has more than one layer (Object -> Layer -> Delete Layer). |
| `delete_page` | Deletes the specified 0-based page when document.pages.len() > 1 (Layout -> Delete Page). |
| `delete_selection` | Deletes all unlocked selected objects on the active layer (Edit -> Delete / Delete). |
| `densify_object_path` | Densifies a vector path by subdividing every segment longer than max_len_pt into uniform sub-segments. |
| `distribute_exact_gap` | Spaces all selected unlocked objects along the horizontal or vertical axis with an exact gap_pt between adjacent bounding boxes. |
| `distribute_selection` | Distributes >= 3 selected objects evenly (left, center_horizontal, right, spacing_horizontal, top, center_vertical, bottom, spacing_vertical). |
| `distribute_selection_along_path` | Positions all selected unlocked objects other than spine_object_id evenly along the normalized arc length of spine_object_id. |
| `divide_selection` | Splits two selected overlapping closed objects into non-overlapping regions (A \ B, A ∩ B, B \ A) (Object -> Shaping -> Divide). |
| `document_summary` | Returns page count, active page, selection summary, and object counts. |
| `duplicate_page` | Duplicates the specified 0-based page (including its layers and objects) and activates the copy (Layout -> Duplicate Page). |
| `duplicate_selection` | Duplicates selected objects by (offset_x_pt, offset_y_pt) and selects the copies (Ctrl+D). |
| `equalize_selection_size` | Scales all selected unlocked objects around their centers to match the width and/or height of the last selected key object. |
| `erase_object_circle` | Subtracts a circular eraser nib of radius_pt centered at (center_x, center_y) from an object (Eraser Tool X). |
| `exclude_selection` | Computes the symmetric difference (XOR / Exclude) of >= 2 selected unlocked objects with EvenOdd fill rule. |
| `expand_effects` | Bakes live non-destructive vector effects on object_id into static vector geometry. |
| `export_cdr7_base64` | Exports the document to RIFF CDR7 binary format and returns its byte length and base64-encoded payload. |
| `export_cdr7_byte_len` | Exports the multi-page document to clean-room RIFF CDR7 and returns its byte length. |
| `export_cdrx_zip_base64` | Exports the document to modern ZIP-based CDRX (content/riffData.cdr) format and returns its byte length and base64-encoded payload. |
| `export_cdrx_zip_byte_len` | Serializes the document to a modern CorelDRAW X4+ ZIP container (content/riffData.cdr) and returns its byte length. |
| `export_cmx1_base64` | Exports the document to Corel Presentation Exchange (RIFF CMX1) binary format and returns its byte length and base64-encoded payload. |
| `export_cmx1_byte_len` | Serializes the document to a clean-room RIFF CMX1 Exchange file and returns its byte length. |
| `export_coralcraft_json` | Serializes the entire multi-page Document to lossless native .coralcraft JSON. |
| `export_coralcraft_zip_base64` | Exports the document as a deterministic Stored ZIP .coralcraft container (manifest.json + document.json + content-addressed blobs), returning byte length and base64-encoded payload. |
| `export_cpl_palette_base64` | Exports the document's extracted color swatches to a CorelDRAW binary .cpl (0xCDDC) palette and returns its byte length and base64-encoded payload. |
| `export_dxf` | Exports a document page to AutoCAD ASCII DXF (AC1009 / R12). |
| `export_dxf_base64` | Exports a page to AutoCAD DXF (AC1009) and returns its byte length and base64-encoded payload. |
| `export_eps` | Exports a document page to Encapsulated PostScript (EPSF-3.0). |
| `export_eps_base64` | Exports page_index to clean-room EPSF-3.0 Encapsulated PostScript and returns eps_bytes length and RFC 4648 eps_base64 string. |
| `export_pdf_base64` | Exports the entire multi-page document to ISO 32000-1 PDF 1.4 and returns pdf_bytes length and RFC 4648 pdf_base64 string. |
| `export_pdf_byte_len` | Exports the multi-page document to ISO 32000-1 PDF 1.4 and returns its byte length. |
| `export_pdf_imposed_base64` | Exports the document to an N-up imposed print sheet PDF 1.4 with optional crop marks and returns its byte length and base64-encoded payload. |
| `export_pdf_imposed_byte_len` | Exports an N-Up imposed press sheet (rows x cols with margins, gutters, and crop marks) to PDF 1.4. |
| `export_pdf_with_prepress_base64` | Exports the document to print-ready PDF 1.4 with corner crop marks, Registration (CMYK 100/100/100/100) bullseye targets, and bleed, returning byte length and base64-encoded payload. |
| `export_png_base64` | Rasterizes page_index at dpi (4x4 MSAA) to PNG and returns png_bytes length and RFC 4648 png_base64 string. |
| `export_svg` | Exports the specified 0-based page as clean-room SVG 1.1 markup. |
| `export_svg_base64` | Exports page_index to clean-room SVG 1.1 and returns svg_bytes length and RFC 4648 svg_base64 string. |
| `export_svg_with_prepress` | Exports a page as an SVG 1.1 document string with prepress corner crop marks, Registration bullseye targets, and bleed. |
| `export_xml_palette` | Exports the document's extracted color swatches to a CorelDRAW XML palette (<palette>...</palette>). |
| `extract_document_palette` | Scans all objects in the document and returns the deduplicated list of colors (hex, r, g, b) used in fills, fountain stops, patterns, and outlines. |
| `extract_path_skeleton` | Replaces a vector object with its internal medial-chord centerline skeleton path (Object -> Shaping -> Centerline Skeleton). |
| `extract_powerclip` | Extracts PowerClip children from container_id back into the active layer. |
| `fillet_path_corners` | Applies Fillet (round), Scallop (scalloped), or Chamfer (chamfered) corner modification with radius_pt to object_id. |
| `find_and_replace_text` | Replaces all occurrences of find with replace across all unlocked text objects (Edit -> Find and Replace -> Replace Text). |
| `fit_circle_to_object_path` | Replaces a path object with a least-squares Kåsa algebraic circle fitted to its world-space path points. |
| `fit_page_to_selection` | Resizes the active page and translates page objects so the selection (or all unlocked visible objects) is framed with margin_pt. |
| `fit_text_to_path` | Flows an Artistic Text object (text_object_id) along a spine path object (spine_object_id) with optional offset_pt. |
| `flatten_object_path_to_polylines` | Flattens all quadratic and cubic Bezier curves of a path object into linear polyline subpaths within flatness tolerance_pt. |
| `get_document_summary` | Returns document-level statistics including title, units, page_count, master_layer_count, total_object_count, active_page_index, and selection_count. |
| `group_selection` | Groups >= 2 selected objects into an ObjectKind::Group (Ctrl+G). |
| `hatch_object_path` | Replaces object_id with parallel and optional cross-hatch vector engraving strokes at angle_deg and spacing_pt. |
| `import_coralcraft_json` | Loads a native .coralcraft JSON document string into the active EditorEngine. |
| `import_coralcraft_zip_base64` | Loads a base64-encoded .coralcraft Stored ZIP container (or raw JSON document) into the active workspace. |
| `import_cpl_palette_base64` | Parses a base64-encoded CorelDRAW binary .cpl color palette and returns its palette name and swatches. |
| `import_dxf` | Replaces the active document by importing an AutoCAD ASCII DXF string. |
| `import_svg` | Replaces the active document by importing an SVG 1.1 string. |
| `import_xml_palette` | Parses a CorelDRAW XML palette (<palette>...</palette>) and returns its name and decoded swatches. |
| `insert_curve_node` | Subdivides a path segment at element_index on an object, inserting a new node at t = 0.5 (Shape Tool -> Add Nodes / +). |
| `inspect_object` | Returns detailed metadata (id, name, kind, visible, locked, opacity, bounds, effects_count) for an object. |
| `invert_selection` | Inverts the selection among unlocked, visible objects on the active target layer (Edit -> Select All -> Invert Selection). |
| `jitter_selection_transforms` | Applies deterministic SplitMix64 pseudo-random translation, rotation, and uniform scale around each selected object's center. |
| `join_open_curves` | Joins open subpaths in an object whose endpoints lie within tolerance_pt (Object -> Join Curves / Ctrl+Shift+J). |
| `knife_cut_object` | Slices an object along the cut line (start_x, start_y) -> (end_x, end_y) with kerf gap_pt (Knife Tool). |
| `list_layers` | Lists all layers on page_index with id, name, role, visible, locked, printable, opacity, and object_count (Objects Docker). |
| `list_page_objects` | Lists all objects on page_index with id, name, kind, layer_id, visible, locked, and opacity (Objects Docker). |
| `list_pages` | Lists all pages in the document with index, id, name, width_pt, height_pt, bleed_pt, and layer_count (Pages Docker). |
| `measure_object_geometry` | Computes total path length (length_pt) and signed/absolute area (area_sq_pt) of an object in world points (Object Properties -> Curve). |
| `mirror_selection` | Mirrors selected objects horizontally and/or vertically around their center, optionally duplicating (copy=true). |
| `morph_between_objects` | Topologically morphs start_id toward end_id at factor t (0.0..=1.0) with sample_count vertices and adds the resulting Path. |
| `move_bezier_node` | Moves the on-curve anchor point at element_index on object_id to (x, y) in points (Shape Tool F10). |
| `move_layer` | Reorders the layer at from_index to to_index on page_index (Objects Docker). |
| `move_object_to_layer` | Moves object_id to target_layer_id on the active page (Objects Docker). |
| `move_page` | Moves the 0-based page at from_index to to_index. |
| `new_document` | Resets the workspace to a fresh A4 Portrait multi-page document. |
| `offset_object_path` | Offsets an object's path by distance_pt (Parallel Drawing / Offset Curve), either replacing in place or creating a parallel copy. |
| `orient_selection_to_center` | Rotates each selected unlocked object around its own center to face radially away from (center_x, center_y) plus angle_offset_deg. |
| `pack_selection_in_grid` | Packs all selected unlocked objects into a multi-column grid starting at (origin_x, origin_y) with cols columns and gap_pt spacing. |
| `pinch_punch_object_path` | Applies a radial Pinch (strength < 0) or Punch (strength > 0) warp to a path object within radius_pt around (center_x, center_y). |
| `place_inside_powerclip` | Places selected objects inside container_id as PowerClip children (up to depth 3). |
| `project_selection_axonometric` | Projects all selected unlocked objects onto one of the 30-degree isometric planes (top, left, right). |
| `quantize_selection_positions` | Snaps each selected object's top-left bounding corner to the nearest (grid_x_pt, grid_y_pt) grid multiple. |
| `radial_scale_selection_positions` | Scales the radial distance of selected objects from a center point and rotates their angular positions by twist_deg. |
| `recolor_artwork` | Globally replaces colors matching (from_r, from_g, from_b) within tolerance_u8 with (to_r, to_g, to_b) across fills, gradients, patterns, and outlines. |
| `redo` | Redoes the most recently undone editor command. |
| `reduce_path_nodes` | Simplifies path subpaths in an object using Ramer-Douglas-Peucker node reduction (Shape Tool -> Reduce Nodes). |
| `relax_object_path` | Globally relaxes all subpaths of a vector object using shrinkage-compensated Taubin/Laplacian vertex smoothing. |
| `rename_object` | Updates the user-assigned label of an object in the Objects Docker. |
| `rename_page` | Renames the specified 0-based page tab (Layout -> Rename Page). |
| `reorder_selection` | Reorders selected objects within their layer (to_front, to_back, forward_one, backward_one). |
| `resolve_selection_overlaps` | Iteratively separates overlapping selected objects by force-directed AABB repulsion to maintain min_gap_pt clearance. |
| `reverse_path_direction` | Reverses the winding direction of all subpaths in an object (Shape Tool -> Reverse Subpaths). |
| `rotate_selection` | Rotates selected objects by angle_deg around any 9-point Object Origin (origin_index 0..=8), optionally duplicating (Alt+F8). |
| `rotate_selection_progressively` | Rotates each selected unlocked object around its own center by a linearly interpolated angle from start_angle_deg to end_angle_deg in selection order. |
| `roughen_path` | Subdivides segments of an object with serrated zigzag normal teeth of amplitude tooth_height_pt and frequency (Roughen Tool). |
| `scale_selection` | Scales selected objects by (scale_x, scale_y) around any 9-point Object Origin (origin_index 0..=8), optionally duplicating (Alt+F9). |
| `scale_selection_by_distance` | Scales each selected object around its own center based on its radial distance from an attractor center within falloff_radius_pt. |
| `scale_selection_progressively` | Scales each selected unlocked object around its own center by a linearly interpolated scale factor from start_scale to end_scale in selection order. |
| `select_all` | Selects all unlocked visible objects on the active layer (Ctrl+A). |
| `select_by_color` | Selects all unlocked visible objects on the active layer whose fill and/or outline color is within tolerance_u8 of (r, g, b). |
| `select_by_kind` | Selects all unlocked, visible objects on the active layer matching kind (rect, ellipse, polygon, path, text, bitmap, group). |
| `set_active_page_target` | Switches the active page tab (target = page with page_index, master, or desktop) in the Page Navigator Bar. |
| `set_active_tool` | Switches the active CorelDRAW toolbox tool (pick, shape, crop, zoom, freehand, rectangle, ellipse, polygon, artistic_text, interactive_fill). |
| `set_artistic_text_properties` | Updates text, font_family, and/or size_pt on an ArtisticText or ParagraphText object (Text -> Edit Text / Ctrl+Shift+T). |
| `set_document_title` | Updates the document title metadata (File -> Document Properties). |
| `set_fountain_fill` | Applies a Linear, Radial, Conical, or Square Fountain gradient fill to selected objects (F11 Fountain Fill). |
| `set_layer_properties` | Updates visible, locked, printable, and/or opacity on a layer (Objects Docker). |
| `set_object_locked` | Locks or unlocks an object (Object -> Lock -> Lock / Unlock). |
| `set_object_opacity` | Updates the opacity (0.0..=1.0) of an object (Properties Docker). |
| `set_object_visible` | Toggles visibility (Object -> Hide / Show Object) of an object. |
| `set_outline_arrows` | Sets start_arrow and end_arrow markers (none, triangle, open_arrow, circle, diamond) on selected objects (F12 Outline Pen). |
| `set_outline_cap_join` | Updates line cap (butt, round, square), line join (miter, round, bevel), and/or miter_limit on selected objects' OutlineStyle (F12 Outline Pen). |
| `set_outline_dash` | Sets outline width_pt and dash_pattern array on selected objects (F12 Outline Pen). |
| `set_outline_properties` | Updates width_pt, behind_fill, and/or scale_with_object on selected objects' OutlineStyle (F12 Outline Pen). |
| `set_page_background` | Sets or clears the canvas background color on page_index (Layout -> Page Background). |
| `set_page_bleed` | Sets the print bleed margin in points on page_index (Layout -> Page Setup -> Bleed). |
| `set_page_size` | Updates active page dimensions (width_pt, height_pt) and measurement units (mm, pt, inch, px) (Layout -> Page Setup). |
| `set_paragraph_text_layout` | Updates columns, gutter_pt, line_spacing, and/or alignment on a ParagraphText frame (Text -> Columns / Paragraph Formatting). |
| `set_path_fill_rule` | Updates the winding fill rule (non_zero or even_odd) of an object's path (Object Properties -> Fill). |
| `set_pattern_fill` | Applies a Two-Color Pattern Fill (checkerboard, polka_dot, horizontal_stripe, vertical_stripe, diagonal_crosshatch) to selected objects. |
| `set_selection_bounds` | Sets exact position (x_pt, y_pt) and/or dimensions (width_pt, height_pt) of selected objects around any 9-point Object Origin (origin_index 0..=8). |
| `set_snap_settings` | Updates snap_to_objects and/or snap_to_page preferences (View -> Snap To). |
| `set_style_cmyk` | Sets fill or outline color in CMYK (c, m, y, k in 0.0..=1.0) on selected objects or creation defaults. |
| `set_style_hsb` | Sets fill or outline color in HSB (h_deg in 0..360, s and b in 0.0..=1.0) on selected objects or creation defaults. |
| `set_style_rgb` | Sets RGB fill or outline color on the current selection (or creation defaults). |
| `set_style_spot` | Sets fill or outline to a named Spot color (or Registration when name is 'Registration') with tint (0.0..=1.0) and CMYK fallback. |
| `set_transparency` | Applies uniform or linear fountain transparency to selected objects. |
| `set_viewport` | Updates canvas zoom factor (0.05..=64.0) and pan offset (pan_x, pan_y) (Zoom / Pan Tool Z / H). |
| `shear_warp_object_path` | Applies a bounding-box-normalized oblique shear warp (shear_x_pt, shear_y_pt) to the target object's path. |
| `shift_selection_colors` | Shifts HSB hue (hue_delta_deg), saturation (saturation_delta), and brightness (brightness_delta) across all fills, fountain stops, patterns, and outlines of the selection. |
| `skew_selection` | Skews selected objects by (skew_x_deg, skew_y_deg) around any 9-point Object Origin (origin_index 0..=8), optionally duplicating (Alt+F11). |
| `smart_fill_at_point` | Creates a new filled Path object from the enclosed region of overlapping closed shapes containing (x, y) (Smart Fill Tool). |
| `smear_path` | Directionally drags vertices within radius_pt of (ox, oy) along (dx_pt, dy_pt) on an object (Smear Tool). |
| `smooth_curve_node` | Converts the node at element_index on an object into a smooth symmetrical C1 cubic Bezier node (Shape Tool F10 -> Smooth / Symmetrical Node). |
| `smooth_path` | Applies localized Jacobi Laplacian vertex smoothing within radius_pt around (center_x, center_y) with strength and iterations (Smooth Tool). |
| `smooth_polyline_to_cubic_beziers` | Converts each subpath of a path object into a C1-continuous Catmull-Rom cubic Bezier curve with configurable tension. |
| `snap_selection_to_circle_radius` | Translates each selected unlocked object radially from (center_x, center_y) so its center lies on the circle of radius_pt. |
| `spherical_warp_path` | Applies a CorelDRAW Fish-Eye / Spherical Lens radial warp to a vector object within radius_pt of (center_x, center_y). |
| `stack_selection` | Packs all selected unlocked objects edge-to-edge horizontally or vertically with gap_pt separation. |
| `step_and_repeat` | Duplicates selected objects in a parametric array (count, offset_x_pt, offset_y_pt). |
| `subdivide_path_segments` | Subdivides every segment of an object's path at its midpoint t = 0.5 (Shape Tool F10 -> Select All Nodes -> Add Nodes (+)). |
| `swap_object_positions` | Swaps the bounding-box centers of two objects (first_id, second_id). |
| `toggle_text_kind` | Converts a text object between Artistic Text and Paragraph Text (Text -> Convert to Paragraph/Artistic Text / Ctrl+F8). |
| `trace_bitmap` | Traces an ObjectKind::Bitmap into a closed vector Path (PowerTrace / Outline Trace). |
| `transform_curve_nodes` | Scales (scale_x, scale_y) and rotates (rotate_deg) the curve nodes at element_indices on an object around their centroid (Shape Tool F10). |
| `translate_selection` | Translates all unlocked selected objects by (dx_pt, dy_pt) in points (Object -> Transformations -> Position). |
| `twirl_path` | Applies a localized rotational swirl around (cx, cy) within radius_pt on an object (Twirl Tool). |
| `undo` | Undoes the most recent editor command. |
| `ungroup_all_selection` | Recursively ungroups all selected groups and nested subgroups down to leaf objects (Object -> Group -> Ungroup All). |
| `ungroup_selection` | Ungroups selected ObjectKind::Group nodes one level (Ctrl+U). |
| `unlock_all_objects` | Unlocks all locked objects across all layers on the active page (Object -> Lock -> Unlock All Objects). |
| `update_ellipse_parameters` | Updates ellipse mode (full, pie, arc) and start_deg/end_deg on selected ellipses without converting them to curves (Property Bar). |
| `update_polygon_parameters` | Updates polygon/star mode (polygon, star), points, and inner_radius_pt on selected polygons without converting them to curves (Property Bar). |
| `update_rect_corners` | Updates corner_kind (round, scalloped, chamfered) and radii_pt on selected rectangles without converting them to curves (Property Bar). |
| `virtual_segment_delete` | Removes the path segment at element_index on an object, splitting the subpath at that segment (Virtual Segment Delete Tool). |
| `wave_distort_path` | Applies a sinusoidal wave distortion along the local normals of an object's path. |

<!-- END GENERATED TOOL LIST -->
