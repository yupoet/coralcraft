# 03 — Current Milestone Status, Active Tasks & Next Steps

> Last updated: 2026-10-19

## 1. Current Stage Status

- **Stage**: `usable beta baseline` (**M0 through M7 are 100% delivered and passing all CI gates**).
- **Completed in This Cycle**:
  1. **M6: Clean-Room `CDR7`/`CMX1` Writers, Group/Align/PowerClip/Curve/Node Editing, 20 MCP Tools & Workspace Rasterizer (`commit 36493c5`, `45190a8`, `b6b68cb`)**:
     - `write_cdr7` and `write_cmx1` writers with round-trip for all geometric primitives and `trfd` 2D affine matrix decoding.
     - Group/Ungroup, ConvertToCurves, Z-Order, 7-mode Align, PowerClip Place/Extract, MoveBezierNode.
     - Pixel-accurate `1440×900` classic CorelDRAW full-workspace rasterizer (`DesktopWorkspace::render_workspace_png` + `coralcraft-cli workspace-preview`).
  2. **M7: Boolean Path Operations, Live Vector Effects, CDRX ZIP Writer, Binary `.cpl` Palettes & Workspace Enhancements (`commit 6616608`, `f17fd6a`, and current working slice`)**:
     - `coralcraft-color`: Clean-room binary `.cpl` palette body parser & serializer (`0xCDDC` RGB, `0xCCDC` CMYK, `0xCDCD` Spot/Pantone).
     - `coralcraft-cdr`: Modern X4+ ZIP container writer `write_cdrx_zip` with IEEE 802.3 CRC-32 and stored Deflate archives (`mimetype`, `content/riffData.cdr` `RIFF....CDRE` v1400, `META-INF/metadata.xml`).
     - `coralcraft-geom`: 2D Boolean path operations (`Weld`, `Trim`, `Intersect`, `FrontMinusBack`, `BackMinusFront`) and resolution-independent `artistic_text_to_path` enabling `ConvertToCurves` (`Ctrl+Q`) on `ArtisticText`.
     - `coralcraft-doc`: Non-destructive vector effect stack on `ObjectNode` (`VectorEffect::{Contour, DropShadow, Extrude, Blend}`).
     - `coralcraft-engine` & `coralcraft-render`: Engine commands with full undo/redo, live effect expansion, and 26 MCP tools.
     - `coralcraft-ui-egui`: 10 vector tool glyphs for Toolbox, 14 standard toolbar icons, dashed `#0055E5` blue ruler guidelines, and `Objects` docker type badges (`[Grp]`, `[Pwr]`, `[Eff]`, `[Cur]`, `[Txt]`, `[Rec]`, `[Elp]`, `[Pol]`, `[Str]`, `[Unk]`).

## 2. Multi-Model Cross-Validation Consensus (M7 -> M8)

- **GLM-5.2 (`zcode`)**: Validated `write_cdrx_zip` entry order (`mimetype` entry #0, stored) and even-byte RIFF chunk alignment. Recommends M8 prioritize embedded raster bitmap `bmp ` chunk extraction/re-embedding followed by transparency `lens` chunk and `txtd`/`txts` text runs.
- **Qwen 3.8 Max (`kimi`)**: Identified winding number degeneracy safeguards (snap-to-grid with strict epsilon), cycle detection in effect DAGs, clean-room boundary on color palettes (algorithmic test palettes without proprietary Pantone tables), and open-standard font metrics (`rustybuzz`/`cosmic-text`).
- **DeepSeek V4.1 Flash (`codebuddy`)**: Proposed modeling Blend as a self-contained container `ObjectKind::Blend` with `BlendSpine` (cumulative arc-length LUT, tangent/normal frame calculation, unrolled rotation/log-scale transforms). Defined `TransparencyStyle::Fountain` as a node-level compositing attribute with offscreen surface blending, and established a 14-test geometric clipping/winding verification matrix.
- **Grok 4.6 (`grok`)**: Documented CorelDRAW guideline behavior (infinite non-printing lines on Guides layer, dragging from rulers, `#0055E5` blue dashed styling), Object Manager hierarchy conventions, and prioritized the interactive editing workflow for M8: (1) Interactive Fountain Fill vector handles -> (2) Interactive Transparency vector handles -> (3) Interactive Drop Shadow handles -> (4) Interactive Contour drag handles.

## 3. Next Up (M8: Embedded Bitmaps, Transparency Lens, Interactive Fountain/Transparency Vectors, and Typography Runs)

1. **Embedded Raster Bitmaps (`bmp ` / `CDIB`)**: Parse and re-embed raster bitmap chunks with resolution and DPI metadata in `coralcraft-cdr`.
2. **Transparency & Lens Compositing**: `TransparencyStyle::Uniform` and `TransparencyStyle::Fountain` on `ObjectNode` with offscreen alpha-multiplier blending in `coralcraft-render`.
3. **Interactive On-Canvas Vector Handles**: Start, end, and midpoint handles on canvas for Fountain Fill and Transparency, supporting angle rotation, skew, and palette swatch dropping in `coralcraft-tools` & `coralcraft-ui-egui`.
4. **Self-Contained Blend Spines (`BlendSpine`)**: Path attachment, arc-length LUT parameterization, and node deformation in `coralcraft-doc`, `coralcraft-geom`, and `coralcraft-engine`.
