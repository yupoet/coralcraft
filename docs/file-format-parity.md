# File Format Parity Matrix

> [!NOTE]
> **Updated 2026-10-21 (`R0.1`–`R8.1` completed):** All format parsers and writers are backed by behavioural tests, deterministic hostile-input mutation tests, nightly `cargo-fuzz` targets (`fuzz/`), and `libcdr` (`cdr2raw` / `cmx2raw`) differential oracle verification. Re-estimated per-dimension scores are tracked in [`ROADMAP.md`](../ROADMAP.md) and the completed work queue is in [`gaps.md`](gaps.md).


CoralCraft treats CorelDRAW `.cdr` and `.cmx` as first-class citizens alongside our open native `.coralcraft` format and standard vector interchange formats.

All `.cdr` and `.cmx` format knowledge is clean-room derived from public specifications:
- [Photopea CDR Format Specification](https://www.photopea.com/tuts/cdr-format-specification/)
- [Kaitai Struct `coreldraw_cdr.ksy`](https://formats.kaitai.io/coreldraw_cdr/)
- [LibreOffice `libcdr` Public Architecture Notes](https://wiki.documentfoundation.org/DLP/Libraries/libcdr)

## Format Support Overview

| Format | Extension | Generation / Variant | Probe (`M1`) | Open / Import | Save / Export | Target Milestone |
| --- | --- | --- | --- | --- | --- | --- |
| **CorelDRAW Legacy WL** | `.cdr` | v1 – v2 (`WL` header) | **Yes** (`CdrWl`, v100–v200) | Graceful diagnostic (`UnsupportedContainer`) | No | M1 (Probe) |
| **CorelDRAW RIFF** | `.cdr` | v3 – X3 (v300 – v1300, bare `RIFF` + `CDR*`) | **Yes** (`Riff` + `vrsn`, `LIST cmpr`/`stlt` safe) | **Yes** (`parse_document`, `LIST cmpr` zlib/raw inflate, `mcfg`/`page`/`layr`/`obj `/`bbox`/`trfd`/`loda`/`fild`/`outl`/`CDIB`/`txtd`/`enve` + `ObjectKind::Unsupported` fallback) | **Yes** (`coralcraft_cdr::write_cdr7`) | M3 / M6–M11 (Delivered) |
| **CorelDRAW ZIP+RIFF** | `.cdr`, `.cdrx` | X4 – 2025+ (v1400 – v2600+, ZIP + `content/riffData.cdr` or `content/root.dat`) | **Yes** (`ZipCdr` EOCD + Central Directory + Stored/Deflate inner RIFF) | **Yes** (`extract_zip_primary_riff` Stored & Deflate + `parse_document`, 64/64 real-world samples green) | **Yes** (`coralcraft_cdr::write_cdrx_zip`) | M3 / M7 (Delivered) |
| **Corel Presentation Exchange** | `.cmx` | CMX1 (`RIFF` LE / `RIFX` BE + `CMX1` / `cdr6`) | **Yes** (`Cmx` LE/BE) | **Yes** (`parse_document` baseline) | **Yes** (`coralcraft_cdr::write_cmx1`) | M3 / M6 (Delivered) |
| **CoralCraft Native** | `.coralcraft` | Open ZIP container (`manifest.json` + `document.json` + `blobs/`) or single-file JSON | — | **Yes** (`Document::load_bytes`) | **Yes** (`save_json` / `save_zip`) | M2 (Delivered) |
| **Scalable Vector Graphics** | `.svg` | SVG 1.1 | — | **Yes** (`coralcraft_doc::import_svg`) | **Yes** (`coralcraft_render::export_svg`) | M3 / M14 (Delivered) |
| **Portable Document Format** | `.pdf` | ISO 32000-1 PDF 1.4 (`/MediaBox`, `/TrimBox`, `/BleedBox`, CMYK `k`/`K`, RGB `rg`/`RG`) | — | **Yes** (`coralcraft_doc::import_pdf`) | **Yes** (`coralcraft_render::export_pdf`) | M3 / M9 / M88 (Delivered) |
| **Portable Network Graphics** | `.png` | RGBA8 PNG with 4×4 MSAA | — | — | **Yes** (`coralcraft_render::encode_png`) | M2 (Delivered) |
| **Encapsulated PostScript** | `.eps` | PostScript / `EPSF-3.0` (`%%BoundingBox`, `%%HiResBoundingBox`, CMYK `setcmykcolor`, RGB `setrgbcolor`) | — | **Yes** (`coralcraft_doc::import_eps`) | **Yes** (`coralcraft_render::export_eps`) | M14 / M88 (Delivered) |
| **AutoCAD Drawing Exchange** | `.dxf` | ASCII DXF (`AC1009` / R12 `ENTITIES`: `LINE`, `POLYLINE`/`VERTEX`/`SEQEND`, `LWPOLYLINE`, `CIRCLE`) | — | **Yes** (`coralcraft_doc::import_dxf`) | **Yes** (`coralcraft_render::export_dxf`) | M14 (Delivered) |
| **Corel Palette** | `.cpl`, `.xml` | User-imported spot/process color palettes (`0xCDDC` RGB, `0xCCDC` CMYK, `0xCDCD` Spot) | — | **Yes** (`parse_xml_palette` / `parse_binary_cpl`) | **Yes** (`to_xml_palette` / `serialize_binary_cpl`) | M2 / M7 / R5.4 (Delivered) |

## M1–M3 Format Capabilities (`coralcraft-cdr`, `coralcraft-doc`, `coralcraft-render`)

1. **Three-Generation Container Recognition & Decompression**:
   - **Legacy `WL` (`CdrWl`)**: Detects `b"WL"` headers (`v100`–`v200`).
   - **Bare `RIFF` / `RIFX` (`Riff`)**: Parses little-endian `RIFF` and big-endian `RIFX` headers, decodes ASCII version tags (`"CDR "`/`"CDR3"` through `"CDRR"`, plus `"CMX1"`/`"cdr6"`), and decompresses `LIST cmpr` blocks (4-byte header + zlib/CPng/raw Deflate stream via `crates/cdr/src/inflate.rs`).
   - **Modern `ZIP` (`ZipCdr`, X4 / v1400+)**: Scans the ZIP End of Central Directory (EOCD) and Central Directory and extracts both Stored (`0`) and Deflate (`8`) `content/riffData.cdr` / `content/root.dat` streams.
2. **Object-Tree Decoding (`coralcraft_cdr::parse_document`)**:
   - Extracts document/page metrics (`mcfg`), pages (`LIST page`), layers (`LIST layr`), and objects (`LIST obj ` with `bbox`, `trfd`, `loda`, `fild`, `outl`), mapping unrecognized chunks losslessly into `ObjectKind::Unsupported` with structured `ParseWarning` diagnostics.
   - Passes **64/64 (100%)** real-world `.cdr` samples in `refs/cdr-samples/`.
3. **Clean-Room Vector & Raster Export (`coralcraft-cli export` / `render`)**:
   - Exports any `.cdr` or `.coralcraft` file to SVG 1.1 (`.svg`), multi-page ISO 32000-1 PDF 1.4 (`.pdf`), 4×4 MSAA `.png`, or `.coralcraft`.
