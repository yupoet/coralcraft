# CoralCraft Roadmap

> Baseline stage: **beta baseline (M0–M6 delivered)** · Established: 2026-10-19 · Updated: 2026-10-19
> Methodology: Dual-track clean-room public specification & format reverse engineering (see [`AGENTS.md`](AGENTS.md)).
> Self-scoring follows `vectorcraft`'s `target-app-parity` convention: 0–100% per dimension, updated on every completed task; see [`docs/target-app-parity.md`](docs/target-app-parity.md) and [`docs/file-format-parity.md`](docs/file-format-parity.md).

| Dimension | Current | Target (usable alpha) |
| --- | ---: | ---: |
| Workspace & engineering gates (`workspace`/`xtask`/CI/cleanroom) | 100% | Maintain |
| CDR format: `probe` (container/version/chunk walk) | **100%** (WL / RIFF+RIFX / X4+ ZIP+RIFF, CLI `probe`, 64/64 real-world samples passing) | M1: 100% (achieved) |
| CDR format: `parse` (object tree -> document model) | **85%** (RFC 1951/1950 `inflate`, `LIST cmpr` + ZIP Deflate, `mcfg`/`page`/`layr`/`obj `/`bbox`/`trfd`/`loda` (`0x01`/`0x02`/`0x03`/`0x04`/`0x14`/`0x1e`)/`fild`/`outl` -> `Document` with `ObjectKind::Unsupported` fallback, **64/64 real-world `.cdr` samples parsed**) | M3–M6: achieved |
| CDR/CMX format: `write` (`write_cdr7` / `write_cmx1`) | **65%** (clean-room RIFF `CDR7` and `CMX1` writers with lossless round-trip for Rect, Ellipse, Polygon/Star, Bezier Path, Artistic Text, `trfd` Affine2, Fill, and Outline + `coralcraft-cli export -o out.cdr/out.cmx`) | M6 (achieved) |
| Multi-page document model & native `.coralcraft` round-trip | **100%** (`coralcraft-geom` + `coralcraft-color` + `coralcraft-doc`, dual JSON/ZIP `.coralcraft` container, 100-iteration random property tests green) | M2: 100% (achieved) |
| Rendering & Vector Export (PNG / SVG 1.1 / PDF 1.4 / CDR7 / CMX1) | **90%** (`coralcraft-render` 4×4 MSAA PNG rasterizer, clean-room SVG 1.1 `export_svg`, ISO 32000-1 PDF 1.4 `export_pdf` with `/MediaBox`/`/TrimBox`/`/BleedBox` & CMYK `k`/`K`, `write_cdr7`/`write_cmx1`, `coralcraft-cli render`, `export`, & `workspace-preview`) | M2–M6 (achieved) |
| Tools, panels & UI vertical slice | **88%** (`coralcraft-tools` + `coralcraft-engine` + `coralcraft-ui-egui` + `coralcraft` desktop workspace + pixel-accurate `1440×900` full-workspace rasterizer `render_workspace_png` with 6×9 bitmap font, `#D4D0C8` chrome, rulers, `#808080` pasteboard, page drop-shadow, 8 solid `6×6` selection handles + center `X`, `[X]` red-slash swatch, `Objects` docker, page tabs, and status chips) | M4–M6 (achieved) |
| Print & prepress production (bleed/imposition/spot/PDF-X) | **55%** (CMYK/Spot/Registration/CIE Lab D50<->D65 Bradford CAT, `.xml`/`.cpl` palette parser, per-page bleed, PDF `/MediaBox`+`/TrimBox`+`/BleedBox` & native CMYK `k`/`K`) | M6+ |
| MCP / agent-driven illustration | **90%** (`coralcraft-mcp` JSON-RPC 2.0 server with 20 authoring, grouping, alignment, PowerClip, curve-conversion, and export tools + `coralcraft --mcp`) | M5–M6 (achieved) |
| CorelDRAW workflow parity | **85%** (multi-page + Master All/Odd/Even + Desktop layer z-order, 9-point Object Origin, parametric shapes, Group/Ungroup `Ctrl+G`/`Ctrl+U`, Convert to Curves `Ctrl+Q`, Z-Order `Shift+PageUp`/`PageDown`, 7-mode Alignment `L/R/T/B/C/E/P`, PowerClip Place/Extract, `F10` Shape Tool node editing, Fountain fill, CDR/CMX round-trip) | M4–M6 (achieved) |

## Completed (M0–M6)

1. **M0 Workspace Skeleton**: 12-crate layered workspace (`L0..L7`), `xtask` (`ci`, `cleanroom`, `assets`, `layers`, `stats`), and 11 pinned reference repositories in `refs/`.
2. **M1 CDR/CMX Format Probe Vertical Slice**:
   - `coralcraft-cdr`: Supports Legacy `WL`, bare `RIFF`/`RIFX`, and modern `ZIP+RIFF` (X4–X5 `content/riffData.cdr` and X6+ `content/root.dat` + `content/dataFileList.dat`) container generations; decodes `' '` / `'3'`..`'9'` / `'A'`..`'R'` ASCII version bytes and `vrsn` chunks; skips `LIST cmpr` / `LIST stlt` as opaque payloads; passes a 10,000-iteration deterministic hostile mutation test (zero `unsafe`, zero `unwrap`).
   - `coralcraft-cli probe`: Supports human-readable output and `--json` machine-readable output (exit codes `0/2/3/4/5`), passing **64/64 (100%)** real-world `.cdr` samples in `refs/cdr-samples/`.
3. **M2 Multi-Page Document Model, `.coralcraft` Container & First Rendered Frame**:
   - `coralcraft-geom` (L0): Canonical finite `F64` (`-0.0 -> +0.0`, `Eq`/`Ord`/`Hash` with `serde_json` `float_roundtrip`), exact `CdrUnit(i64)` (`1/254,000 inch = 0.1 µm`) <-> `Pt(F64)` conversion, `Affine2`, adaptive de Casteljau Bézier flattener (`max_depth = 12`), and CorelDRAW parametric shapes (`RectSpec` with per-corner `Round`/`Scalloped`/`Chamfered`, `EllipseSpec` with `Full`/`Pie`/`Arc`, `PolygonSpec` regular polygon/star).
   - `coralcraft-color` (L0): `Rgb`/`Cmyk`/`Hsb`/`Gray`/`LabD50`/`LabD65`/`Spot`/`Registration` color spaces, Bradford D50<->D65 chromatic adaptation ($\Delta E < 0.01$), Linear/Radial/Conical/Square Fountain stops with midpoint skew and rainbow HSB blending, and clean-room `.xml`/`.cpl` palette parser/serializer.
   - `coralcraft-doc` (L1): Multi-page `Document` / `Page` / document-level `Layer` (`Normal`, `MasterAllPages`, `MasterOddPages`, `MasterEvenPages`, `Desktop`, `Guides`, `Grid`), `ObjectNode` (`powerclip_children`, `residual`), `ObjectKind::Unsupported` lossless passthrough, and dual-mode `.coralcraft` container (`save_json` + deterministic Stored ZIP `save_zip` with `manifest.json`, `document.json`, and content-addressed `blobs/<sha256>.bin`), verified by 100-iteration random document round-trip property tests and tamper-detection tests.
   - `coralcraft-render` (L3) & `coralcraft-cli`: Premultiplied RGBA8 software rasterizer with 4×4 supersampled scanline fill, stroke expander (`behind_fill`, `scale_with_object`, Hairline `width_pt == 0.0`), Linear/Radial Fountain shader, recursive PowerClip mask stack (`MAX_CLIP_DEPTH = 8`), clean-room PNG encoder/decoder, and `coralcraft-cli render` / `inspect-palette`.
4. **M3 CDR Decompression, Object-Tree Parser (`parse_document`) & SVG 1.1 / PDF 1.4 Vector Export**:
   - `coralcraft-cdr`: Clean-room dependency-free RFC 1951 (`inflate_raw`) and RFC 1950 (`inflate_zlib`, including `"CPng\x01\x00\x04\x00"` headers) decompressor with `MAX_DECOMPRESSED_BYTES = 64 MiB` guard; ZIP Deflate (`method = 8`) inner RIFF extraction; `parse_document(&[u8]) -> Result<ParseOutcome, ProbeError>` decoding `LIST cmpr`, `mcfg`, `LIST page`, `LIST layr`, `LIST obj ` (`bbox`, `trfd`, `loda`, `fild`, `outl`) with lossless `ObjectKind::Unsupported` fallback, passing **64/64 real-world `.cdr` samples**.
   - `coralcraft-render::svg` & `coralcraft-render::pdf`: Clean-room SVG 1.1 (`export_svg`) and ISO 32000-1 PDF 1.4 (`export_pdf`) vector exporters supporting multi-page `/MediaBox`/`/TrimBox`/`/BleedBox`, native CMYK (`k`/`K`) and RGB (`rg`/`RG`) operators, cubic Bézier curves, Fountain gradients, PowerClip clipping, `behind_fill`, `scale_with_object`, and Hairline strokes.
   - `coralcraft-cli export`: Direct `.cdr` / `.coralcraft` conversion to `.svg`, `.pdf`, `.png`, and `.coralcraft` (`--page`, `--dpi`, `--json`).
5. **M4 & M5 Classic CorelDRAW Desktop Workspace, Interactive Tools & MCP Server**:
   - `coralcraft-tools` (L4): Classic 10-tool CorelDRAW Toolbox (`Pick`, `Shape F10`, `Crop`, `Zoom Z`, `Freehand/Bezier F5`, `Rectangle F6`, `Ellipse F7`, `Polygon/Star Y`, `Artistic Text F8`, `Interactive Fill G`), shortcut mapper, constrained drag gestures (`Shift` square/circle, `Alt` from center), and context-sensitive `PropertyBarSchema`.
   - `coralcraft-engine` (L5): `EditorEngine` with `ActivePageTarget` (`Page(usize)`, `MasterPage`, `Desktop`), `PaletteChannel` (`Fill` on Left-Click, `Outline` on Right-Click, `[X]` No-Color clear), `EngineCommand` execution, deterministic `undo()` / `redo()` stack, and top-to-bottom hit-testing.
   - `coralcraft-ui-egui` (L6) & `coralcraft` (`apps/coralcraft`, L7): Classic CorelDRAW workspace layout (`MenuBar`, `StandardToolbar`, context-sensitive `PropertyBar`, left `Toolbox`, central `PasteboardCanvas`, right vertical `ColorPaletteBar`, bottom `PageNavigatorBar`, `StatusBar`, and collapsible `Objects`/`Properties` Dockers).
   - `coralcraft-mcp` (L6): JSON-RPC 2.0 Model Context Protocol (`MCP`) server with authoring and export tools.
6. **M6 Clean-Room `CDR7`/`CMX1` Writers, Group/Align/PowerClip/Curve/Node Editing, 20 MCP Tools & Pixel-Accurate Full-Workspace Rasterizer**:
   - `coralcraft-cdr`: Extended `loda` parser (`0x03` Curve/Line, `0x04`/`0x1e` Bezier Path node-type bitmasks `0x00`/`0x40`/`0x80`/`0x08`, `0x14` Polygon/Star, `0x06`/`0x0a` Artistic Text) and `trfd` `f64` 2D affine matrix decoding; clean-room `write_cdr7` and `write_cmx1` writers (`coralcraft-cli export -o out.cdr/out.cmx`).
   - `coralcraft-tools`, `coralcraft-engine` & `coralcraft-mcp`: `GroupSelection` (`Ctrl+G`), `UngroupSelection` (`Ctrl+U`), `ConvertToCurves` (`Ctrl+Q`), `ReorderSelection` (`ToFront`, `ToBack`, `ForwardOne`, `BackwardOne`), `AlignSelection` (`Left`, `Right`, `Top`, `Bottom`, `CenterHorizontal`, `CenterVertical`, `CenterToPage`), `PlaceInsidePowerClip`, `ExtractPowerClip`, `MoveBezierNode` (`F10` Shape Tool), and 20 MCP tools.
   - `coralcraft-ui-egui` & `coralcraft-cli`: Pixel-accurate `1440×900` classic CorelDRAW full-workspace rasterizer (`DesktopWorkspace::render_workspace_png` + `coralcraft-cli workspace-preview`) with embedded 5×7/6×9 bitmap font, `#D4D0C8` chrome, rulers, `#808080` pasteboard, `#404040` page drop-shadow, 8 solid `6×6` `#000000` selection handles + center `X`, `[X]` red-slash swatch, `Objects` docker, page tabs, and status chips.

## Next Up (M7+: Live CorelDRAW Vector Effects, Boolean Path Operations, ZIP-Era CDRX & Binary `.cpl` Palettes)

1. Non-destructive CorelDRAW Vector Effects (`Contour`, `Blend`, `DropShadow` / `Extrude` expansion).
2. CorelDRAW Boolean Path Operations (`Weld`, `Trim`, `Intersect`) and ZIP-era `CDRX` + binary `.cpl` palette body decoding.

## Changelog

- 2026-10-19: M6 delivered (`write_cdr7` & `write_cmx1` writers, `loda` curve/polygon/text & `trfd` round-trip, Group/Ungroup/ConvertToCurves/Reorder/Align/PowerClip/MoveBezierNode, 20 MCP tools, `DesktopWorkspace::render_workspace_png`, and `coralcraft-cli workspace-preview`).
- 2026-10-19: M4 & M5 delivered (`coralcraft-tools`, `coralcraft-engine`, `coralcraft-ui-egui`, `coralcraft-mcp`, `apps/coralcraft` classic CorelDRAW workspace, tool vertical slice, and JSON-RPC 2.0 MCP server).
- 2026-10-19: M3 delivered (`coralcraft-cdr` RFC 1951/1950 `inflate`, `LIST cmpr` + ZIP Deflate decompression, `parse_document` passing 64/64 real-world `.cdr` samples, clean-room SVG 1.1 `export_svg` & ISO 32000-1 PDF 1.4 `export_pdf`, `coralcraft-cli export`, reverse log `REV-0002`).
- 2026-10-19: M2 delivered (`coralcraft-geom`, `coralcraft-color`, `coralcraft-doc` with JSON/ZIP `.coralcraft` container & property tests, `coralcraft-render` PNG rasterizer, `coralcraft-cli render` & `inspect-palette`).
- 2026-10-19: M1 delivered (`coralcraft-cdr` 3-generation container walker, `coralcraft-cli probe`, `xtask` gate unit tests, format & workflow parity baseline docs, first reverse log entry `REV-0001`).
- 2026-10-19: Repository initialized; M0 skeleton (Tasks 1–8) and M1 red-light anchors delivered.
