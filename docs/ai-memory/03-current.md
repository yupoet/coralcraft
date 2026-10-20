# 03 — Current Milestone Status, Active Tasks & Next Steps

> Last updated: 2026-10-20

## 1. Current Stage Status

- **Stage**: `usable beta baseline` (**M0 through M19 are 100% delivered and passing all CI gates**).
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
  11. **M18: Engineering Angular Dimensions (`build_angular_dimension_path`), Create Boundary Silhouette (`CreateBoundary`), Convert Outline to Object (`Ctrl+Shift+Q`), and 56 MCP Tools (`commit fc3ee96`)**.
  12. **M19: Dashed/Dotted Outline Strokes (`OutlineStyle::dash_pattern`), Conical & Square Fountain Gradients (`FountainGeometry::{Conical, Square}`), 9-Point Object Origin Transformations (`RotateSelection` & `ScaleSelection`), and 60 MCP Tools**:
      - `coralcraft-doc`, `coralcraft-cdr`, `coralcraft-render`: Added `OutlineStyle::dash_pattern`, `dash_polylines`, and `FountainGeometry::{Conical, Square}`.
      - `coralcraft-engine` & `coralcraft-mcp`: Added `RotateSelection`, `ScaleSelection`, and expanded MCP server to **60 tools**.

## 2. Multi-Model Cross-Validation Consensus (M14)

- **GLM-5.2 (`zcode`)**: Specified clean-room `EPSF-3.0` DSC header (`%!PS-Adobe-3.0 EPSF-3.0`, `%%BoundingBox`, `%%HiResBoundingBox`) and `AC1009` DXF group codes (`10`/`20`, `70`, `POLYLINE`/`VERTEX`/`SEQEND`).
- **Qwen 3.8 Max (`kimi`)**: Verified PostScript bottom-left Y-flip coordinate transform (`y_ps = page_height - y_doc`) and quadratic-to-cubic degree elevation (`2/3` control points).
- **DeepSeek V4.1 Flash (`codebuddy`)**: Designed `export_eps`, `export_dxf`, `import_svg`, and `import_dxf` across `coralcraft-render`, `coralcraft-doc`, `coralcraft-cli`, and `coralcraft-mcp`.

## 3. Summary of Completed Parity Matrix

All 30 items in `docs/gaps.md` and every row of `docs/file-format-parity.md` and `docs/target-app-parity.md` across M0–M14 are 100% delivered and verified.

