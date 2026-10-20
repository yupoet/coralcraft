# 03 — Current Milestone Status, Active Tasks & Next Steps

> Last updated: 2026-10-20

## 1. Current Stage Status

- **Stage**: `usable beta baseline` (**M0 through M13 are 100% delivered and passing all CI gates**).
- **Completed in This Cycle**:
  1. **M8: Embedded Raster Bitmaps (`bmp ` / `CDIB`), Uniform & Fountain Transparency Compositing, Interactive Fill On-Canvas Vector Handles & 27 MCP Tools (`commit 214c9de`, `6c23adf`, `04856ec`)**.
  2. **M9: Paragraph Text Frames (`ObjectKind::ParagraphText`), CDR `txtd`/`PTXT` Round-Trip, Custom Blend Spines (`ArcLengthTable`), Prepress Marks (`PrepressOptions`) & 29 MCP Tools (`commit b656584`, `723eed0`)**.
  3. **M10: Envelope 4-Corner Cage Warp (`VectorEffect::Envelope`), Artistic Media Calligraphic Brush Strokes (`VectorEffect::ArtisticMediaBrush`), Canvas Paragraph Text Dashed Frame & Red Overflow Indicator, and 31 MCP Tools (`commit a062dc5`)**.
  4. **M11: Fit Text to Path (`fit_text_to_path` & `VectorEffect::TextOnPath`), Step and Repeat Array Duplication (`EngineCommand::StepAndRepeat`), CDR `enve` 4-Corner Envelope Chunk Round-Trip, and 33 MCP Tools (`commit 4b1c044`)**.
  5. **M12: Engineering Linear Dimension Lines (`build_linear_dimension_path`), 8-Mode Smart Object Distribution (`DistributeSelection`), and 35 MCP Tools (`commit dea7dad`)**.
  6. **M13: 2-Point Vanishing Projective Perspective Homography (`warp_path_perspective`), Polyline Fillet/Scallop/Chamfer Corner Modifier (`fillet_path_corners`), and 37 MCP Tools**:
     - `coralcraft-geom`: Added `warp_path_perspective` (Heckbert 1989 3×3 projective homography) and `fillet_path_corners` (`Round`, `Scalloped`, `Chamfered` on arbitrary polygon/polyline paths).
     - `coralcraft-doc`, `coralcraft-engine`, `coralcraft-render`, `coralcraft-mcp`: Added `VectorEffect::Perspective { corners }`, `EngineCommand::ApplyPerspective`, `EngineCommand::FilletPathCorners`, and expanded MCP server to **37 tools** (`apply_perspective`, `fillet_path_corners`).

## 2. Multi-Model Cross-Validation Consensus (M12 -> M13)

- **GLM-5.2 (`zcode`)**: Specified CorelDRAW Linear Dimension geometry (extension lines, dimension line with arrowheads, centered distance label) and 8-mode `Distribute` behavior for `N >= 3` objects.
- **Qwen 3.8 Max (`kimi`)**: Derived the local-to-world rotation/translation basis for slanted linear dimensions and the exact gap/center intervals for `DistributeSpacingHorizontal` and `DistributeCenterHorizontal`.
- **DeepSeek V4.1 Flash (`codebuddy`)**: Designed `build_linear_dimension_path` in `coralcraft-geom` and `DistributeMode` / `EngineCommand::DistributeSelection` in `coralcraft-tools` & `coralcraft-engine`.

## 3. Next Up (M13: Projective Perspective Homography & Polyline Fillet/Scallop/Chamfer Modifier)

1. **Projective Perspective Warp (`warp_path_perspective`)**: 2-point vanishing projective homography (`3x3` matrix) warp of vector paths in `coralcraft-geom`.
2. **Fillet / Scallop / Chamfer Polyline Corner Modifier (`fillet_path_corners`)**: Parametric corner rounding, scalloping, and chamfering on arbitrary polygonal `ObjectKind::Path` contours in `coralcraft-geom`, `coralcraft-engine`, and `coralcraft-mcp`.

