# 03 — Current Milestone Status, Active Tasks & Next Steps

> Last updated: 2026-10-19

## 1. Current Stage Status

- **Stage**: `pre-alpha` (**M0, M1, M2, and M3 are 100% delivered and passing all CI gates**; advancing into **M4** and **M5**).
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

## 2. Next Up (M4: CorelDRAW-Style `egui` Desktop Shell + Interactive Tool Slice & M5: MCP Server)

1. **`coralcraft-tools` (L4)**:
   - Classic CorelDRAW tool suite (`Pick`, `Shape F10`, `Crop`, `Zoom Z`, `Freehand/Bezier F5`, `Rectangle F6`, `Ellipse F7`, `Polygon/Star Y`, `Artistic Text F8`, `Interactive Fill G`), shortcut mapping (`tool_for_shortcut`), in-progress `GestureState`, and context-sensitive `PropertyBarSchema`.
2. **`coralcraft-engine` (L5)**:
   - `EditorEngine` owning `Document`, `ActivePageTarget` (`Page(usize)`, `MasterPage`, `Desktop`), `Selection`, default `FillStyle`/`OutlineStyle`, `Command` execution, deterministic `undo()`/`redo()` stack, and page hit-testing (`hit_test_page`).
3. **`coralcraft-ui-egui` (L6), `coralcraft-desktop` (`apps/coralcraft`, L7), and `coralcraft-mcp` (L6)**:
   - Classic CorelDRAW desktop workspace layout (`MenuBar`, `StandardToolbar`, context-sensitive `PropertyBar`, left `Toolbox`, central `PasteboardCanvas`, right vertical `ColorPaletteBar` with Left-Click Fill / Right-Click Outline, bottom `PageNavigatorBar`, `StatusBar`, and `Objects`/`Properties` Dockers) + `coralcraft-mcp` JSON-RPC tool server.
