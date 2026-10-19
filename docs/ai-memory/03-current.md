# 03 — Current Milestone Status, Active Tasks & Next Steps

> Last updated: 2026-10-19

## 1. Current Stage Status

- **Stage**: `alpha baseline` (**M0, M1, M2, M3, M4, and M5 are 100% delivered and passing all CI gates**).
- **Completed in This Cycle**:
  1. **M1: CDR/CMX Format Probe Vertical Slice (Tasks 9–13 Green)**:
     - `coralcraft-cdr::probe()` covering Legacy `WL`, bare `RIFF`/`RIFX`, and modern `ZIP+RIFF` containers; 22 unit tests green including a 10,000-iteration deterministic hostile mutation test (`hostile_random_mutations_never_panic`).
     - `coralcraft-cli probe [--json] <path>` passing **64/64 (100%)** real-world `.cdr` samples in `refs/cdr-samples/`.
  2. **M2: Multi-Page Document Model, `.coralcraft` Container & First Rendered Frame (Slices 1–4 Green)**:
     - [`crates/geom/`](../../crates/geom/src/lib.rs) (L0, `commit 18bb494`): Canonical finite `F64`, exact `CdrUnit(i64)` <-> `Pt(F64)`, `Affine2`, adaptive de Casteljau Bézier flattener (`max_depth = 12`), and CorelDRAW parametric shapes (`RectSpec`, `EllipseSpec`, `PolygonSpec`).
     - [`crates/color/`](../../crates/color/src/lib.rs) (L0, `commit 4e87d30`): `Rgb`/`Cmyk`/`Hsb`/`Gray`/`LabD50`/`LabD65`/`Spot`/`Registration`, Bradford D50<->D65 CAT ($\Delta E < 0.01$), Fountain stops, and clean-room `.xml`/`.cpl` palette parser/serializer.
     - [`crates/doc/`](../../crates/doc/src/lib.rs) (L1, `commit 0a0d4d0`): Multi-page `Document` / `Page` / document-level `Layer` (`Normal`, `MasterAllPages`, `MasterOddPages`, `MasterEvenPages`, `Desktop`, `Guides`, `Grid`), `ObjectNode` (`powerclip_children`, `residual`), `ObjectKind::Unsupported` lossless passthrough, and dual-mode `.coralcraft` container (`save_json`, deterministic Stored ZIP `save_zip`), verified by 100-iteration random document round-trip property tests.
     - [`crates/render/`](../../crates/render/src/lib.rs) & [`apps/coralcraft-cli/`](../../apps/coralcraft-cli/src/main.rs) (`commit ff3fe51`): Premultiplied RGBA8 software rasterizer with 4×4 MSAA, stroke expander (`behind_fill`, `scale_with_object`, Hairline), Linear/Radial Fountain shader, recursive PowerClip mask stack (`MAX_CLIP_DEPTH = 8`), PNG encoder/decoder, and `coralcraft-cli render` / `inspect-palette`.
  3. **M3: CDR Decompression, Object-Tree Parser (`parse_document`) & SVG 1.1 / PDF 1.4 Vector Export (Slices 1–3 Green)**:
     - [`crates/render/src/svg.rs`](../../crates/render/src/svg.rs) & [`crates/render/src/pdf.rs`](../../crates/render/src/pdf.rs) (`commit 1799e81`): Clean-room SVG 1.1 (`export_svg`) and ISO 32000-1 PDF 1.4 (`export_pdf`, multi-page `/MediaBox`/`/TrimBox`/`/BleedBox`, CMYK `k`/`K`, RGB `rg`/`RG`, cubic Bézier paths, Fountain gradients, PowerClip clipping).
     - [`crates/cdr/src/inflate.rs`](../../crates/cdr/src/inflate.rs) & [`crates/cdr/src/parse.rs`](../../crates/cdr/src/parse.rs) (`commit 5c7cc70`): Dependency-free RFC 1951/1950 inflate (`MAX_DECOMPRESSED_BYTES = 64 MiB`), ZIP Deflate inner RIFF extraction, and `parse_document(&[u8])` (`LIST cmpr`, `mcfg`, `LIST page`, `LIST layr`, `LIST obj ` with `bbox`/`trfd`/`loda`/`fild`/`outl` and `ObjectKind::Unsupported` fallback), passing **64/64 real-world `.cdr` samples**.
     - [`apps/coralcraft-cli/src/main.rs`](../../apps/coralcraft-cli/src/main.rs) (`commit 5c7cc70`): `coralcraft-cli export <input> -o <output.{svg,pdf,png,coralcraft,json}> [--page <1-based>] [--dpi <f64>] [--json]`.
  4. **M4 & M5: Classic CorelDRAW Desktop Workspace, Interactive Tools & MCP Server (`commit 9f10a27`)**:
     - [`crates/tools/src/lib.rs`](../../crates/tools/src/lib.rs) (L4): Classic 10-tool CorelDRAW Toolbox (`Pick`, `Shape F10`, `Crop`, `Zoom Z`, `Freehand/Bezier F5`, `Rectangle F6`, `Ellipse F7`, `Polygon/Star Y`, `Artistic Text F8`, `Interactive Fill G`), shortcut bindings, constrained drag gestures, and context-sensitive `PropertyBarSchema`.
     - [`crates/engine/src/lib.rs`](../../crates/engine/src/lib.rs) (L5): `EditorEngine` with `ActivePageTarget` (`Page(usize)`, `MasterPage`, `Desktop`), `PaletteChannel` (`Fill` on Left-Click, `Outline` on Right-Click, `[X]` No-Color clear), `EngineCommand`, deterministic `undo()` / `redo()` stack, and hit-testing.
     - [`crates/ui-egui/src/lib.rs`](../../crates/ui-egui/src/lib.rs) (L6) & [`apps/coralcraft/src/main.rs`](../../apps/coralcraft/src/main.rs) (L7): Classic CorelDRAW workspace (`MenuBar`, `StandardToolbar`, context-sensitive `PropertyBar`, left `Toolbox`, central `PasteboardCanvas`, right vertical `ColorPaletteBar`, bottom `PageNavigatorBar`, `StatusBar`, and collapsible `Objects`/`Properties` Dockers).
     - [`crates/mcp/src/lib.rs`](../../crates/mcp/src/lib.rs) (L6): JSON-RPC 2.0 Model Context Protocol (`MCP`) server with 12 authoring and export tools.

## 2. Next Up (M6+: Extended CDR Version Bands, `LIST stlt` Style Resolution & Live Vector Effects)

1. **`LIST stlt` Shared Style Table & `txsm` Text Decoding**:
   - Resolve object style references into `LIST stlt` sub-chunks (`fild`, `outl`, `font`, `align`) for CDR X4–2025+ files.
2. **Live CorelDRAW Vector Effects**:
   - Interactive `Contour`, `Blend`, `Envelope`, and `Extrude` operators.
