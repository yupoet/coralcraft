# 03 — Current Milestone Status, Active Tasks & Next Steps

> Last updated: 2026-10-19

## 1. Current Stage Status

- **Stage**: `pre-alpha` (**M0, M1, and M2 are 100% delivered and passing all CI gates**; advancing into **M3** and **M4**).
- **Completed in This Cycle**:
  1. **M1: CDR/CMX Format Probe Vertical Slice (Tasks 9–13 Green)**:
     - `coralcraft-cdr::probe()` covering Legacy `WL`, bare `RIFF`/`RIFX`, and modern `ZIP+RIFF` containers; 22 unit tests green including a 10,000-iteration deterministic hostile mutation test (`hostile_random_mutations_never_panic`).
     - `coralcraft-cli probe [--json] <path>` passing **64/64 (100%)** real-world `.cdr` samples in `refs/cdr-samples/`.
  2. **M2: Multi-Page Document Model, `.coralcraft` Container & First Rendered Frame (Slices 1–4 Green)**:
     - [`crates/geom/`](../../crates/geom/src/lib.rs) (L0, `commit 18bb494`): Canonical finite `F64` (`-0.0 -> +0.0`, `Eq`/`Ord`/`Hash` with `serde_json` `float_roundtrip`), exact `CdrUnit(i64)` (`1/254,000 inch = 0.1 µm`) <-> `Pt(F64)` via `31_750 / 9` rational, `Affine2`, adaptive de Casteljau Bézier flattener (`max_depth = 12`), and CorelDRAW parametric shapes (`RectSpec` with per-corner `Round`/`Scalloped`/`Chamfered`, `EllipseSpec` with `Full`/`Pie`/`Arc`, `PolygonSpec` regular polygon/star).
     - [`crates/color/`](../../crates/color/src/lib.rs) (L0, `commit 4e87d30`): `Rgb`/`Cmyk`/`Hsb`/`Gray`/`LabD50`/`LabD65`/`Spot`/`Registration`, Bradford D50<->D65 CAT ($\Delta E < 0.01$), Fountain stops (`Direct`, `RainbowCw`, `RainbowCcw`, midpoint skew), and clean-room `.xml`/`.cpl` palette parser/serializer.
     - [`crates/doc/`](../../crates/doc/src/lib.rs) (L1, `commit 0a0d4d0`): Multi-page `Document` / `Page` / document-level `Layer` (`Normal`, `MasterAllPages`, `MasterOddPages`, `MasterEvenPages`, `Desktop`, `Guides`, `Grid`), `Document::effective_layers_for_page()`, `ObjectNode` (`powerclip_children`, `residual`), `ObjectKind::Unsupported` lossless passthrough, and dual-mode `.coralcraft` container (`save_json`, deterministic Stored ZIP `save_zip` with `manifest.json` + `document.json` + `blobs/<sha256>.bin`, `load_bytes`), verified by 100-iteration random document round-trip property tests and tamper-detection tests.
     - [`crates/render/`](../../crates/render/src/lib.rs) & [`apps/coralcraft-cli/`](../../apps/coralcraft-cli/src/main.rs) (`commit ff3fe51`): Premultiplied RGBA8 software rasterizer with 4×4 supersampled scanline filler, stroke expander (`behind_fill`, `scale_with_object`, Hairline `width_pt == 0.0`), Linear/Radial Fountain shader, recursive PowerClip offscreen mask stack (`MAX_CLIP_DEPTH = 8`), clean-room PNG encoder/decoder, and `coralcraft-cli render` / `inspect-palette`.

## 2. Next Up (M3: CDR Object-Tree Parser + SVG/PDF Vector Export & M4: CorelDRAW-Style `egui` Desktop Shell)

1. **`coralcraft-cdr` Decompression & Object-Tree Parser (M3)**:
   - Clean-room RFC 1951/1950 inflate (`crates/cdr/src/inflate.rs`) for `LIST cmpr` blocks and ZIP Deflate (`method = 8`) inner entries (`content/riffData.cdr`, `content/root.dat`) with `MAX_DECOMPRESSED_BYTES` safety limits.
   - `coralcraft_cdr::parse_document(&[u8]) -> Result<ParseOutcome, ProbeError>` extracting pages (`page`), layers (`layr`), objects (`obj ` / `loda` / `trfd` / `outl` / `fild` / `LIST stlt`), and falling back to `ObjectKind::Unsupported` with structured warnings across all 64 real-world `.cdr` samples.
2. **Vector Export (`SVG` & `PDF`) + CLI `export` (M3)**:
   - Clean-room SVG 1.1 (`export_svg`) and ISO 32000-1 PDF 1.4 (`export_pdf`, multi-page `/Pages`, CMYK `k`/`K`, RGB `rg`/`RG`, cubic Bézier paths, PowerClip clipping) + `coralcraft-cli export <input> -o <out.{svg,pdf,png,coralcraft}>`.
3. **CorelDRAW-Style `egui` Desktop Shell (M4)**:
   - `coralcraft-tools`, `coralcraft-engine`, `coralcraft-ui-egui`, and `coralcraft-desktop` replicating CorelDRAW's classic workspace layout, Property Bar, Toolbox, right vertical Color Palette Bar (Left-Click Fill / Right-Click Outline), bottom Page Navigator Tabs, Status Bar, and Dockers.
