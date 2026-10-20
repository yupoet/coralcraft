# 03 — Current Milestone Status, Active Tasks & Next Steps

> Last updated: 2026-10-20

## 1. Current Stage Status

- **Stage**: `usable beta baseline` (**M0 through M11 are 100% delivered and passing all CI gates**).
- **Completed in This Cycle**:
  1. **M8: Embedded Raster Bitmaps (`bmp ` / `CDIB`), Uniform & Fountain Transparency Compositing, Interactive Fill On-Canvas Vector Handles & 27 MCP Tools (`commit 214c9de`, `6c23adf`, `04856ec`)**.
  2. **M9: Paragraph Text Frames (`ObjectKind::ParagraphText`), CDR `txtd`/`PTXT` Round-Trip, Custom Blend Spines (`ArcLengthTable`), Prepress Marks (`PrepressOptions`) & 29 MCP Tools (`commit b656584`, `723eed0`)**.
  3. **M10: Envelope 4-Corner Cage Warp (`VectorEffect::Envelope`), Artistic Media Calligraphic Brush Strokes (`VectorEffect::ArtisticMediaBrush`), Canvas Paragraph Text Dashed Frame & Red Overflow Indicator, and 31 MCP Tools (`commit a062dc5`)**.
  4. **M11: Fit Text to Path (`fit_text_to_path` & `VectorEffect::TextOnPath`), Step and Repeat Array Duplication (`EngineCommand::StepAndRepeat`), CDR `enve` 4-Corner Envelope Chunk Round-Trip, and 33 MCP Tools**:
     - `coralcraft-geom`: Added `fit_text_to_path` laying out glyphs along arbitrary `Path` spines using `ArcLengthTable` tangent rotation and perpendicular baseline offset.
     - `coralcraft-doc`, `coralcraft-engine`, `coralcraft-render`: Added `VectorEffect::TextOnPath { spine, offset_pt }`, `EngineCommand::FitTextToPath`, `EngineCommand::StepAndRepeat`, and `ExpandEffects` baking.
     - `coralcraft-cdr` & `coralcraft-mcp`: Added 32-byte `enve` 4-corner Envelope chunk encoding/decoding in `write_cdr7` & `parse_document`, and expanded MCP server to **33 tools** (`fit_text_to_path`, `step_and_repeat`).

## 2. Multi-Model Cross-Validation Consensus (M11 -> M12)

- **GLM-5.2 (`zcode`)**: Specified the 32-byte CDR `enve` chunk layout (`4 * (i32, i32)` `CdrUnit` ticks for `[TL, TR, BR, BL]`) and per-glyph arc-length advance + tangent rotation matrix.
- **Qwen 3.8 Max (`kimi`)**: Derived the exact composite 2D affine matrix $M_i = T(P(s_i)) \cdot R(\theta_i)$ and `StepAndRepeat` ID allocation and selection behavior.
- **DeepSeek V4.1 Flash (`codebuddy`)**: Designed `fit_text_to_path` in `coralcraft-geom` and `FitTextToPath` / `StepAndRepeat` commands in `coralcraft-engine`.

## 3. Next Up (M12: Engineering Dimension Lines & Smart Object Distribution)

1. **Dimension Lines (`LinearDimension`) & Technical Callouts**: Parametric horizontal/vertical/slanted engineering dimension lines with extension ticks, arrows, and centered measurement labels.
2. **Smart Distribute (`DistributeSelection`)**: Equal spacing distribution across `>= 3` selected objects in `coralcraft-tools`, `coralcraft-engine`, and `coralcraft-mcp`.

