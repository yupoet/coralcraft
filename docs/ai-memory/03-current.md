# 03 — Current Milestone Status, Active Tasks & Next Steps

> Last updated: 2026-10-20

## 1. Current Stage Status

- **Stage**: `usable beta baseline` (**M0 through M35 are 100% delivered and passing all CI gates**).
- **Completed in This Cycle**:
  1. **M8: Embedded Raster Bitmaps (`bmp ` / `CDIB`), Uniform & Fountain Transparency Compositing, Interactive Fill On-Canvas Vector Handles & 27 MCP Tools (`commit 214c9de`, `6c23adf`, `04856ec`)**.
  2. **M9: Paragraph Text Frames (`ObjectKind::ParagraphText`), CDR `txtd`/`PTXT` Round-Trip, Custom Blend Spines (`ArcLengthTable`), Prepress Marks (`PrepressOptions`) & 29 MCP Tools (`commit b656584`, `723eed0`)**.
  3. **M10: Envelope 4-Corner Cage Warp (`VectorEffect::Envelope`), Artistic Media Calligraphic Brush Strokes (`VectorEffect::ArtisticMediaBrush`), Canvas Paragraph Text Dashed Frame & Red Overflow Indicator, and 31 MCP Tools (`commit a062dc5`)**.
  4. **M11: Fit Text to Path (`fit_text_to_path` & `VectorEffect::TextOnPath`), Step and Repeat Array Duplication (`EngineCommand::StepAndRepeat`), CDR `enve` 4-Corner Envelope Chunk Round-Trip, and 33 MCP Tools (`commit 4b1c044`)**.
  5. **M12: Engineering Linear Dimension Lines (`build_linear_dimension_path`), 8-Mode Smart Object Distribution (`DistributeSelection`), and 35 MCP Tools (`commit dea7dad`)**.
  6. **M13: 2-Point Vanishing Projective Perspective Homography (`warp_path_perspective`), Polyline Fillet/Scallop/Chamfer Corner Modifier (`fillet_path_corners`), and 37 MCP Tools (`commit 111cbaf`)**.
  7. **M14: Encapsulated PostScript (`EPSF-3.0`), AutoCAD `DXF` (`AC1009`) Export/Import, SVG 1.1 Vector Import (`import_svg`), and 41 MCP Tools (`commit 4e50004`)**.
  8. **M15: Symmetrical & Logarithmic Spirals (`build_spiral_path`), Graph Paper Grid (`build_graph_paper_path`), N-Up PDF Sheet Imposition (`export_pdf_imposed`), Attributes Eyedropper (`CopyPropertiesFrom`), and 45 MCP Tools (`commit 2682dd1`)**.
  9. **M16: Orthogonal Connectors (`build_connector_path`), Callout Boxes (`build_callout_path`), Combine/Break Apart (`Ctrl+L`/`Ctrl+K`), Bitmap Adjustments (`AdjustBitmap`), and 50 MCP Tools (`commit fecf0a1`)**.
  10. **M17: Interactive Distortion Effects (`PushPull`/`Zipper`/`Twister`), PowerTrace Bitmap-to-Vector Outline Tracer (`trace_bitmap_to_path`), Mirror & Symmetry Duplication (`MirrorSelection`), and 53 MCP Tools (`commit 95a59d9`)**.
  11. **M18: Engineering Angular Dimensions (`build_angular_dimension_path`), Create Boundary Silhouette (`CreateBoundary`), Convert Outline to Object (`Ctrl+Shift+Q`), and 56 MCP Tools (`commit 9cd7a16`)**.
  12. **M19: Dashed/Dotted Outline Strokes (`OutlineStyle::dash_pattern`), Conical & Square Fountain Gradients (`FountainGeometry::{Conical, Square}`), 9-Point Object Origin Transformations (`RotateSelection` & `ScaleSelection`), and 60 MCP Tools (`commit 697d584`)**.
  13. **M20: Outline Start/End Arrowhead Markers (`ArrowheadKind`), 9-Point Skew/Shear Transformation (`SkewSelection` / `Alt+F11`), Duplicate (`Ctrl+D`) & Select All (`Ctrl+A`), and 64 MCP Tools (`commit f2fe109`)**.
  14. **M21: CorelDRAW Lens Effects (`Alt+F3`: `Invert`, `Brighten`, `Tinted`, `Magnify`), Subpath Winding Reversal (`reverse_path_direction`), Auto-Close Subpaths (`close_open_subpaths`), Object Lock/Unlock (`SetObjectLocked` / `UnlockAllObjects`), and 68 MCP Tools (`commit 3823cbe`)**.
  15. **M22: Two-Color Pattern Fills (`PatternKind` & `FillStyle::TwoColorPattern`), Join Open Curves (`join_open_subpaths` / `Ctrl+Shift+J`), Recursive Ungroup All (`UngroupAllSelection`), Page Reordering (`MovePage`), and 72 MCP Tools (`commit d3cee8e`)**.
  16. **M23: Ramer-Douglas-Peucker Curve Simplification (`simplify_path_nodes`), 9-Point Exact Selection Position & Size (`SetSelectionBounds`), Layer Creation & Property Controls (`AddLayer` & `SetLayerProperties`), and 76 MCP Tools (`commit e34e5c6`)**.
  17. **M24: Smooth Catmull-Rom / B-Spline Curves (`build_bspline_path`), Clear Transformations (`ClearTransformations`), Artistic <-> Paragraph Text Conversion (`ToggleTextKind` / `Ctrl+F8`), Find and Replace Text (`FindAndReplaceText`), and 80 MCP Tools (`commit 5f18008`)**.
  18. **M25: 3-Point Curve (`build_three_point_curve_path`), Text Case Conversion (`TextCaseKind` & `ChangeTextCase` / `Shift+F3`), Shape Tool Node Insertion & Deletion (`insert_curve_node_at` & `delete_curve_node_at`), and 84 MCP Tools (`commit 66672ee`)**.
  19. **M26: Shape Tool Segment Conversion (`convert_segment_to_line` & `convert_segment_to_curve`), Page Background & Bleed Configuration (`SetPageBackground` & `SetPageBleed`), and 88 MCP Tools (`commit 162392c`)**.
  20. **M27: Pointillizer Vector Halftone Mosaic (`build_pointillizer_path`), Object Manager Per-Object Controls (`SetObjectOpacity`, `SetObjectVisible`, `RenameObject`), and 92 MCP Tools (`commit d733784`)**.
  21. **M28: Impact Tool Radial Burst Rays (`build_impact_rays_path`), Layer Deletion & Reordering (`DeleteLayer` & `MoveLayer`), Invert Selection (`InvertSelection`), and 96 MCP Tools (`commit d9c889e`)**.
  22. **M29: Complex Star `{n/k}` Schläfli Star Polygons (`build_complex_star_path`), Outline Pen Properties (`SetOutlineProperties`), Select By Kind (`SelectByKind`), Document Title (`SetDocumentTitle`), and 100 MCP Tools (`commit 5d9c3c5`)**.
  23. **M30: Twirl Tool Localized Path Deformer (`twirl_path_around_point`), Clear & Clone Vector Effects (`ClearEffects` & `CloneEffectsFrom`), Page Size & Units (`set_page_size`), and 104 MCP Tools (`commit cdc5170`)**.
  24. **M31: Attract and Repel Tool Localized Path Deformer (`radial_push_path_around_point`), Page Duplication/Rename/Delete (`duplicate_page`, `rename_page`, `delete_page`), and 108 MCP Tools (`commit 757e7ac`)**.
  25. **M32: Smear Tool Localized Path Deformer (`smear_path_from_point`), Paragraph Text Frame Layout (`SetParagraphTextLayout`), Page Target Switch (`set_active_page_target`), Object Inspection (`inspect_object`), and 112 MCP Tools (`commit 6b873eb`)**.
  26. **M33: Roughen Brush Localized Path Deformer (`roughen_path_segments`), Artistic Text Typography Properties (`SetArtisticTextProperties`), Page Objects & Layer Listing (`list_page_objects` & `list_layers`), and 116 MCP Tools (`commit 9437435`)**.
  27. **M34: Smooth Tool Localized Laplacian Path Deformer (`smooth_path_around_point`), Parametric Rectangle & Ellipse MCP Tools (`update_rect_corners`, `update_ellipse_parameters`), Document Pages Listing (`list_pages`), and 120 MCP Tools (`commit 20f68c9`)**.
  28. **M35: Knife Tool (`knife_cut_path_by_line`), Crop Tool (`CropToRect`), Polygon/Star Live Updates (`update_polygon_parameters`), Cross-Layer Object Transfer (`MoveObjectToLayer`), and 124 MCP Tools**:
      - `coralcraft-geom`: Added `knife_cut_path_by_line`.
      - `coralcraft-engine` & `coralcraft-mcp`: Added `KnifeCutObject`, `CropToRect`, `MoveObjectToLayer`, `knife_cut_object`, `crop_to_rect`, `update_polygon_parameters`, `move_object_to_layer`, and expanded MCP server to **124 tools**.

## 2. Multi-Model Cross-Validation Consensus (M14)

- **GLM-5.2 (`zcode`)**: Specified clean-room `EPSF-3.0` DSC header (`%!PS-Adobe-3.0 EPSF-3.0`, `%%BoundingBox`, `%%HiResBoundingBox`) and `AC1009` DXF group codes (`10`/`20`, `70`, `POLYLINE`/`VERTEX`/`SEQEND`).
- **Qwen 3.8 Max (`kimi`)**: Verified PostScript bottom-left Y-flip coordinate transform (`y_ps = page_height - y_doc`) and quadratic-to-cubic degree elevation (`2/3` control points).
- **DeepSeek V4.1 Flash (`codebuddy`)**: Designed `export_eps`, `export_dxf`, `import_svg`, and `import_dxf` across `coralcraft-render`, `coralcraft-doc`, `coralcraft-cli`, and `coralcraft-mcp`.

## 3. Summary of Completed Parity Matrix

All 30 items in `docs/gaps.md` and every row of `docs/file-format-parity.md` and `docs/target-app-parity.md` across M0–M14 are 100% delivered and verified.

