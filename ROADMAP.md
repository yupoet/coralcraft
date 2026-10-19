# CoralCraft Roadmap

> Baseline stage: **alpha baseline (M0–M5 delivered)** · Established: 2026-10-19 · Updated: 2026-10-19
> Methodology: Dual-track clean-room public specification & format reverse engineering (see [`AGENTS.md`](AGENTS.md)).
> Self-scoring follows `vectorcraft`'s `target-app-parity` convention: 0–100% per dimension, updated on every completed task; see [`docs/target-app-parity.md`](docs/target-app-parity.md) and [`docs/file-format-parity.md`](docs/file-format-parity.md).

| Dimension | Current | Target (usable alpha) |
| --- | ---: | ---: |
| Workspace & engineering gates (`workspace`/`xtask`/CI/cleanroom) | 100% | Maintain |
| CDR format: `probe` (container/version/chunk walk) | **100%** (WL / RIFF+RIFX / X4+ ZIP+RIFF, CLI `probe`, 64/64 real-world samples passing) | M1: 100% (achieved) |
| CDR format: `parse` (object tree -> document model) | **75%** (RFC 1951/1950 `inflate`, `LIST cmpr` + ZIP Deflate, `mcfg`/`page`/`layr`/`obj `/`bbox`/`trfd`/`loda`/`fild`/`outl` -> `Document` with `ObjectKind::Unsupported` fallback, **64/64 real-world `.cdr` samples parsed**) | M3: modern RIFF baseline (achieved) |
| CDR format: `write` | 0% | Long-term campaign (M6+) |
| Multi-page document model & native `.coralcraft` round-trip | **100%** (`coralcraft-geom` + `coralcraft-color` + `coralcraft-doc`, dual JSON/ZIP `.coralcraft` container, 100-iteration random property tests green) | M2: 100% (achieved) |
| Rendering & Vector Export (PNG / SVG 1.1 / PDF 1.4) | **85%** (`coralcraft-render` 4×4 MSAA PNG rasterizer, clean-room SVG 1.1 `export_svg`, ISO 32000-1 PDF 1.4 `export_pdf` with `/MediaBox`/`/TrimBox`/`/BleedBox` & CMYK `k`/`K`, `coralcraft-cli render` & `export`) | M2–M4 (achieved) |
| Tools, panels & UI vertical slice | **75%** (`coralcraft-tools` + `coralcraft-engine` + `coralcraft-ui-egui` + `coralcraft` desktop workspace: Menu Bar, Standard Toolbar, context-sensitive Property Bar, Toolbox, Pasteboard, Color Palette Bar, Page Navigator, Status Bar, Dockers) | M4 (achieved) |
| Print & prepress production (bleed/imposition/spot/PDF-X) | **45%** (CMYK/Spot/Registration/CIE Lab D50<->D65 Bradford CAT, `.xml`/`.cpl` palette parser, per-page bleed, PDF `/MediaBox`+`/TrimBox`+`/BleedBox` & native CMYK `k`/`K`) | M6+ |
| MCP / agent-driven illustration | **75%** (`coralcraft-mcp` JSON-RPC 2.0 server with 12 authoring & export tools + `coralcraft --mcp`) | M5 (achieved) |
| CorelDRAW workflow parity | **70%** (multi-page + Master All/Odd/Even + Desktop layer z-order, 9-point Object Origin, parametric shapes, PowerClip, Fountain fill, classic CorelDRAW workspace, CDR -> SVG/PDF/PNG/`.coralcraft` pipeline) | M4/M5 (achieved) |

## Completed (M0–M5)

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
   - `coralcraft-mcp` (L6): JSON-RPC 2.0 Model Context Protocol (`MCP`) server with 12 vector authoring and export tools (`new_document`, `document_summary`, `add_rectangle`, `add_ellipse`, `add_polygon`, `add_artistic_text`, `set_style_rgb`, `add_page`, `export_svg`, `export_pdf_byte_len`, `undo`, `redo`).

## Next Up (M6+: Extended CDR Version Bands, Live Vector Effects & Prepress Production)

1. Extended CDR chunk decoding (`LIST stlt` style tables, `txsm` text formatting, `bmp ` embedded bitmaps, `spnd` PowerClip container links).
2. Live CorelDRAW vector effects (`Contour`, `Blend`, `Envelope`, `Extrude`) and SVG import.

## Changelog

- 2026-10-19: M4 & M5 delivered (`coralcraft-tools`, `coralcraft-engine`, `coralcraft-ui-egui`, `coralcraft-mcp`, `apps/coralcraft` classic CorelDRAW workspace, tool vertical slice, and JSON-RPC 2.0 MCP server).
- 2026-10-19: M3 delivered (`coralcraft-cdr` RFC 1951/1950 `inflate`, `LIST cmpr` + ZIP Deflate decompression, `parse_document` passing 64/64 real-world `.cdr` samples, clean-room SVG 1.1 `export_svg` & ISO 32000-1 PDF 1.4 `export_pdf`, `coralcraft-cli export`, reverse log `REV-0002`).
- 2026-10-19: M2 delivered (`coralcraft-geom`, `coralcraft-color`, `coralcraft-doc` with JSON/ZIP `.coralcraft` container & property tests, `coralcraft-render` PNG rasterizer, `coralcraft-cli render` & `inspect-palette`).
- 2026-10-19: M1 delivered (`coralcraft-cdr` 3-generation container walker, `coralcraft-cli probe`, `xtask` gate unit tests, format & workflow parity baseline docs, first reverse log entry `REV-0001`).
- 2026-10-19: Repository initialized; M0 skeleton (Tasks 1–8) and M1 red-light anchors delivered.
