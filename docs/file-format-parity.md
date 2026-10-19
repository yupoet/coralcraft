# File Format Parity Matrix

CoralCraft treats CorelDRAW `.cdr` and `.cmx` as first-class citizens alongside our open native `.coralcraft` format and standard vector interchange formats.

All `.cdr` and `.cmx` format knowledge is clean-room derived from public specifications:
- [Photopea CDR Format Specification](https://www.photopea.com/tuts/cdr-format-specification/)
- [Kaitai Struct `coreldraw_cdr.ksy`](https://formats.kaitai.io/coreldraw_cdr/)
- [LibreOffice `libcdr` Public Architecture Notes](https://wiki.documentfoundation.org/DLP/Libraries/libcdr)

## Format Support Overview

| Format | Extension | Generation / Variant | Probe (`M1`) | Open / Import | Save / Export | Target Milestone |
| --- | --- | --- | --- | --- | --- | --- |
| **CorelDRAW Legacy WL** | `.cdr` | v1 – v2 (`WL` header) | **Yes** (`CdrWl`, v100–v200) | Planned (graceful diagnostic) | No | M1 (Probe) |
| **CorelDRAW RIFF** | `.cdr` | v3 – X3 (v300 – v1300, bare `RIFF` + `CDR*`) | **Yes** (`Riff` + `vrsn`, `LIST cmpr`/`stlt` safe) | Planned | No | M3 (v6–X3 import) |
| **CorelDRAW ZIP+RIFF** | `.cdr`, `.cdrx` | X4 – 2025+ (v1400 – v2600+, ZIP + `content/riffData.cdr` or `content/root.dat`) | **Yes** (`ZipCdr` EOCD + Central Directory + Stored inner RIFF) | Planned | No | M3 (X4+ import) |
| **Corel Presentation Exchange** | `.cmx` | CMX1 (`RIFF` LE / `RIFX` BE + `CMX1` / `cdr6`) | **Yes** (`Cmx` LE/BE) | Planned | Planned | M3 (Import) |
| **CoralCraft Native** | `.coralcraft` | Open ZIP container (`manifest.json` + `document.json` + `blobs/`) or single-file JSON | Planned | Planned | Planned | M2 (Round-trip) |
| **Scalable Vector Graphics** | `.svg` | SVG 1.1 / 2.0 | — | Planned | Planned | M3 (Export) / M6 (Import) |
| **Portable Document Format** | `.pdf` | PDF 1.7 / PDF/X-1a / PDF/X-4 | — | Planned (`craft-pdf`) | Planned (`craft-pdf`) | M3 (Export) / M8 (Prepress) |
| **Encapsulated PostScript** | `.eps` | PostScript / EPS | — | Planned | Planned | M8 |
| **AutoCAD Drawing Exchange** | `.dxf` | ASCII / Binary DXF | — | Planned | Planned | M8 |
| **Corel Palette** | `.cpl`, `.xml` | User-imported spot/process color palettes (no built-in proprietary swatch tables) | — | Planned | Planned | M2 / M8 |

## M1 Probe Capabilities (`coralcraft-cdr::probe` & `coralcraft-cli probe`)

Implemented and verified in M1:

1. **Three-Generation Container Recognition**:
   - **Legacy `WL` (`CdrWl`)**: Detects `b"WL"` headers (`v100`–`v200`).
   - **Bare `RIFF` / `RIFX` (`Riff`)**: Parses little-endian `RIFF` and big-endian `RIFX` headers, decodes ASCII version tags (`"CDR "`/`"CDR3"` through `"CDRR"`, plus `"CMX1"`/`"cdr6"`), and walks top-level and nested `LIST` / `cdr6` chunks.
   - **Modern `ZIP` (`ZipCdr`, X4 / v1400+)**: Scans the ZIP End of Central Directory (EOCD) and Central Directory without external decompression dependencies, distinguishes genuine CorelDRAW ZIP archives (`content/riffData.cdr` for X4–X5, `content/root.dat` + `content/dataFileList.dat` for X6+) from unrelated `.zip`/`.docx`/`.ora` files, and inspects Stored (`method == 0`) inner RIFF headers when available.
2. **Hostile-Input Hardening**:
   - Zero `unsafe`, zero `.unwrap()` / `.expect()` / `panic!` in library code.
   - Enforces `MAX_DEPTH = 32`, `MAX_CHUNKS = 100_000`, and `MAX_ZIP_ENTRIES = 10_000`.
   - Uses `u64` + `checked_add` for all chunk offset arithmetic.
   - Treats `LIST cmpr` and `LIST stlt` payloads as opaque (never descending into raw/zlib-compressed streams as uncompressed sub-chunks).
   - Verified with a 10,000-iteration deterministic hostile mutation test (`hostile_random_mutations_never_panic`).
