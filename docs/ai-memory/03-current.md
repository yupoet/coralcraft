# 03 — Current Milestone Status, Active Tasks & Next Steps

> Last updated: 2026-10-20

## 1. Current Stage Status

- **Stage**: `usable beta baseline` (**M0 through M10 are 100% delivered and passing all CI gates**).
- **Completed in This Cycle**:
  1. **M8: Embedded Raster Bitmaps (`bmp ` / `CDIB`), Uniform & Fountain Transparency Compositing, Interactive Fill On-Canvas Vector Handles & 27 MCP Tools (`commit 214c9de`, `6c23adf`, `04856ec`)**.
  2. **M9: Paragraph Text Frames (`ObjectKind::ParagraphText`), CDR `txtd`/`PTXT` Round-Trip, Custom Blend Spines (`ArcLengthTable`), Prepress Marks (`PrepressOptions`) & 29 MCP Tools (`commit b656584`, `723eed0`)**.
  3. **M10: Envelope 4-Corner Cage Warp (`VectorEffect::Envelope`), Artistic Media Calligraphic Brush Strokes (`VectorEffect::ArtisticMediaBrush`), Canvas Paragraph Text Dashed Frame & Red Overflow Indicator, and 31 MCP Tools**:
     - `coralcraft-geom`: Added `warp_path_bilinear` and `expand_artistic_brush_stroke` along `ArcLengthTable`.
     - `coralcraft-doc`, `coralcraft-engine`, `coralcraft-render`: Added `VectorEffect::Envelope { corners }` and `VectorEffect::ArtisticMediaBrush { start_width_pt, end_width_pt }`, `EngineCommand::ApplyEnvelope`, `EngineCommand::ApplyArtisticBrush`, `ExpandEffects` baking, and live deformed rendering.
     - `coralcraft-ui-egui` & `coralcraft-mcp`: Added dashed `#0055E5` Paragraph Text frame outline and bottom-center red overflow indicator handle on canvas, and expanded MCP server to **31 tools** (`apply_envelope`, `apply_artistic_brush`).

## 2. Multi-Model Cross-Validation Consensus (M10 -> M11)

- **GLM-5.2 (`zcode`)**: Specified CorelDRAW 4-corner Envelope (`enve`) bilinear mapping `P(u,v) = (1-u)(1-v)p00 + u(1-v)p10 + uv*p11 + (1-u)v*p01` with corner order `[TL, TR, BR, BL]`.
- **Qwen 3.8 Max (`kimi`)**: Provided the variable-width Calligraphic/Artistic Media offset formula `E_L(s) = S(s) + (w(s)/2)*N(s)` and `E_R(s) = S(s) - (w(s)/2)*N(s)` along `ArcLengthTable`.
- **DeepSeek V4.1 Flash (`codebuddy`)**: Designed `warp_path_bilinear` and `expand_artistic_brush_stroke` in `coralcraft-geom` and Paragraph Text dashed frame + red overflow indicator in `coralcraft-ui-egui`.

## 3. Next Up (M11: Fit Text to Path, Step and Repeat Array Duplication & CDR Envelope Round-Trip)

1. **Fit Text to Path (`Text -> Fit Text to Path`)**: Per-glyph placement and tangent rotation along an arbitrary `Path` spine using `ArcLengthTable` in `coralcraft-geom`, `coralcraft-engine`, `coralcraft-render`, and `coralcraft-mcp`.
2. **Step and Repeat (`Edit -> Step and Repeat` / `Ctrl+Shift+D`)**: Parametric array duplication (`count`, `offset_x_pt`, `offset_y_pt`) with full undo/redo in `coralcraft-engine` and `coralcraft-mcp`.
3. **CDR `enve` Envelope Chunk Round-Trip (`write_cdr7` & `parse_document`)**: Persist and restore `VectorEffect::Envelope` 4-corner cage coordinates in RIFF `CDR7` object streams.

