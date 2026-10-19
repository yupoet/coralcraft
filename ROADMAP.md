# CoralCraft Roadmap

> Baseline stage: **pre-alpha (M2 delivered)** · Established: 2026-10-19 · Updated: 2026-10-19
> Methodology: Dual-track clean-room public specification & format reverse engineering (see [`AGENTS.md`](AGENTS.md)).
> Self-scoring follows `vectorcraft`'s `target-app-parity` convention: 0–100% per dimension, updated on every completed task; see [`docs/target-app-parity.md`](docs/target-app-parity.md) and [`docs/file-format-parity.md`](docs/file-format-parity.md).

| Dimension | Current | Target (usable alpha) |
| --- | ---: | ---: |
| Workspace & engineering gates (`workspace`/`xtask`/CI/cleanroom) | 100% | Maintain |
| CDR format: `probe` (container/version/chunk walk) | **100%** (WL / RIFF+RIFX / X4+ ZIP+RIFF, CLI `probe`, 64/64 real-world samples passing) | M1: 100% (achieved) |
| CDR format: `parse` (object tree -> document model) | 0% | M3: modern RIFF baseline |
| CDR format: `write` | 0% | Long-term campaign (M5+) |
| Multi-page document model & native `.coralcraft` round-trip | **100%** (`coralcraft-geom` + `coralcraft-color` + `coralcraft-doc`, dual JSON/ZIP `.coralcraft` container, 100-iteration random property tests green) | M2: 100% (achieved) |
| Rendering (first frame -> interactive frame rate) | **50%** (`coralcraft-render` software rasterizer with 4×4 MSAA, Linear/Radial Fountain, PowerClip mask stack, `behind_fill`, `scale_with_object`, Hairline, CLI `render`) | M2 first frame (achieved) / M4 interactive |
| Tools, panels & UI vertical slice | 0% | M4: Pick/Shape/Pen/Text/Pages/Palette |
| Print & prepress production (bleed/imposition/spot/PDF-X) | **15%** (CMYK/Spot/Registration/CIE Lab D50<->D65 Bradford CAT, `.xml`/`.cpl` palette parser, per-page bleed) | M5+ |
| MCP / agent-driven illustration | 0% | M5 |
| CorelDRAW workflow parity | **20%** (multi-page + Master All/Odd/Even + Desktop layer z-order, 9-point Object Origin, parametric shapes, PowerClip, Fountain fill) | M4 interactive desktop slice |

## Completed (M0–M2)

1. **M0 Workspace Skeleton**: 12-crate layered workspace (`L0..L7`), `xtask` (`ci`, `cleanroom`, `assets`, `layers`, `stats`), and 11 pinned reference repositories in `refs/`.
2. **M1 CDR/CMX Format Probe Vertical Slice**:
   - `coralcraft-cdr`: Supports Legacy `WL`, bare `RIFF`/`RIFX`, and modern `ZIP+RIFF` (X4–X5 `content/riffData.cdr` and X6+ `content/root.dat` + `content/dataFileList.dat`) container generations; decodes `' '` / `'3'`..`'9'` / `'A'`..`'R'` ASCII version bytes and `vrsn` chunks; skips `LIST cmpr` / `LIST stlt` as opaque payloads; passes a 10,000-iteration deterministic hostile mutation test (zero `unsafe`, zero `unwrap`).
   - `coralcraft-cli probe`: Supports human-readable output and `--json` machine-readable output (exit codes `0/2/3/4/5`), passing **64/64 (100%)** real-world `.cdr` samples in `refs/cdr-samples/`.
3. **M2 Multi-Page Document Model, `.coralcraft` Container & First Rendered Frame**:
   - `coralcraft-geom` (L0): Canonical finite `F64` (`-0.0 -> +0.0`, `Eq`/`Ord`/`Hash` with `serde_json` `float_roundtrip`), exact `CdrUnit(i64)` (`1/254,000 inch = 0.1 µm`) <-> `Pt(F64)` conversion, `Affine2`, adaptive de Casteljau Bézier flattener (`max_depth = 12`), and CorelDRAW parametric shapes (`RectSpec` with per-corner `Round`/`Scalloped`/`Chamfered`, `EllipseSpec` with `Full`/`Pie`/`Arc`, `PolygonSpec` regular polygon/star).
   - `coralcraft-color` (L0): `Rgb`/`Cmyk`/`Hsb`/`Gray`/`LabD50`/`LabD65`/`Spot`/`Registration` color spaces, Bradford D50<->D65 chromatic adaptation ($\Delta E < 0.01$), Linear/Radial/Conical/Square Fountain stops with midpoint skew and rainbow HSB blending, and clean-room `.xml`/`.cpl` palette parser/serializer.
   - `coralcraft-doc` (L1): Multi-page `Document` / `Page` / document-level `Layer` (`Normal`, `MasterAllPages`, `MasterOddPages`, `MasterEvenPages`, `Desktop`, `Guides`, `Grid`), `ObjectNode` (`powerclip_children`, `residual`), `ObjectKind::Unsupported` lossless passthrough, and dual-mode `.coralcraft` container (`save_json` + deterministic Stored ZIP `save_zip` with `manifest.json`, `document.json`, and content-addressed `blobs/<sha256>.bin`), verified by 100-iteration random document round-trip property tests and tamper-detection tests.
   - `coralcraft-render` (L3) & `coralcraft-cli`: Premultiplied RGBA8 software rasterizer with 4×4 supersampled scanline fill, stroke expander (`behind_fill`, `scale_with_object`, Hairline `width_pt == 0.0`), Linear/Radial Fountain shader, recursive PowerClip mask stack (`MAX_CLIP_DEPTH = 8`), clean-room PNG encoder/decoder, and `coralcraft-cli render` / `inspect-palette`.

## Next Up (M3: CDR Object-Tree Parser + SVG/PDF Vector Export)

1. `coralcraft-cdr`: Clean-room RFC 1951/1950 inflate (`LIST cmpr` dual-stream decompression + ZIP Deflate inner entries with `MAX_DECOMPRESSED_BYTES` guard) and `parse_document()` (`page`/`layr`/`obj `/`loda`/`trfd`/`outl`/`fild`/`LIST stlt` -> `coralcraft_doc::Document` with `ObjectKind::Unsupported` fallback).
2. Vector Export (`SVG` & `PDF`): Clean-room SVG 1.1 and ISO 32000-1 PDF 1.4 multi-page export (`export_svg`, `export_pdf`) + `coralcraft-cli export`.

## Mid-Term (M4–M5)

- M4: CorelDRAW-style `egui` desktop workspace (`coralcraft-tools` + `coralcraft-engine` + `coralcraft-ui-egui` + `coralcraft-desktop`: Menu Bar, Standard Toolbar, context-sensitive Property Bar, left Toolbox, central multi-page Pasteboard, right vertical Color Palette Bar with Left-Click Fill / Right-Click Outline, bottom Page Navigator Tabs, Status Bar, and Object/Property Dockers).
- M5+: MCP server, CMX import/export, broader CDR version bands, CDR writer campaign, prepress production, and scripting engine.

## Changelog

- 2026-10-19: M2 delivered (`coralcraft-geom`, `coralcraft-color`, `coralcraft-doc` with JSON/ZIP `.coralcraft` container & property tests, `coralcraft-render` PNG rasterizer, `coralcraft-cli render` & `inspect-palette`).
- 2026-10-19: M1 delivered (`coralcraft-cdr` 3-generation container walker, `coralcraft-cli probe`, `xtask` gate unit tests, format & workflow parity baseline docs, first reverse log entry `REV-0001`); reviewed across 5 independent models (GLM-5.3, Kimi K3, Grok 4.6, DeepSeek V4.1 Flash, Claude Opus 5.5).
- 2026-10-19: Repository initialized; M0 skeleton (Tasks 1–8) and M1 red-light anchors delivered.
