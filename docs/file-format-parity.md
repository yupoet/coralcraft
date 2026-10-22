# File Format Parity Matrix

> [!IMPORTANT]
> **Updated 2026-10-22 (Phase V1 & V2 completed, operator decision D7):** CoralCraft is built on the VectorCraft file I/O pipeline (`crates/engine/src/cmd/fileio/`) extended with `coralcraft-cdr` and `corel-import`. CoralCraft opens, places, saves, and exports **every format VectorCraft supports** (`FORMATS` = 27 entries, `OPEN_EXTS` = 34 extensions, `PLACE_EXTS` = 28 extensions) plus CorelDRAW `.cdr`/`.cmx`, legacy `.coralcraft` containers, and Corel `.cpl`/`.xml` palettes. CI enforces full coverage via `vectorcraft_format_compatibility_matrix_covers_every_format_v2_1`.

All `.cdr` and `.cmx` format support is clean-room derived from public specifications:
- [Photopea CDR Format Specification](https://www.photopea.com/tuts/cdr-format-specification/)
- [Kaitai Struct `coreldraw_cdr.ksy`](https://formats.kaitai.io/coreldraw_cdr/)
- [LibreOffice `libcdr` Public Architecture Notes](https://wiki.documentfoundation.org/DLP/Libraries/libcdr)

## 1. Document & Interchange Formats (`FORMATS` in `vectorcraft-engine`)

| Format ID | Name | Extensions | Open | Place | Save / Export | Implementation & Notes |
| --- | --- | --- | :---: | :---: | :---: | --- |
| `vectorcraft` | VectorCraft Document | `.vectorcraft`, `.drawcraft` | Yes | No | Yes | Native binary/compressed container (`vectorcraft-format` v1–v4). Preserves CoralCraft multi-page, master-layer, palette, and CDR residual metadata losslessly in `document.unknown["coralcraft"]` (`V2.2`). |
| `template` | VectorCraft Template | `.vctemplate` | Yes | No | Yes | Native template container (`Save As Template…` / `New from Template…`); preserves `document.unknown["coralcraft"]` losslessly (`V2.2`). |
| `cdr` | CorelDRAW | `.cdr` | Yes | Yes | Yes | **Read:** RIFF `CDR3`–`CDRD` (v3–X3) and ZIP+RIFF `CDRE`+ (X4–2025+, `content/riffData.cdr` & `content/root.dat`), plus `WL` v1–v2 container probe (`coralcraft-cdr` + `corel-import`). **Write:** `cdrVersion: "cdr7"` (RIFF CDR7) and `"cdrx"` (X4+ ZIP+RIFF) (`V1.1`–`V1.4`). |
| `cmx` | Corel Presentation Exchange | `.cmx` | Yes | Yes | Yes | **Read:** Little-endian `RIFF` and big-endian `RIFX` `CMX1`/`cdr6`. **Write:** Clean-room `CMX1` RIFF (`coralcraft_cdr::write_cmx1`, `V1.2`–`V1.4`). |
| `coralcraft` | CoralCraft (Legacy) | `.coralcraft` | Yes | Yes | Upgrade to `.vectorcraft` | Pre-fork single-file JSON (`save_json`) and Stored/Deflated ZIP (`manifest.json` + `document.json` + `blobs/<sha256>.bin`) converter (`converted = true`, `V2.3`). |
| `svg` | SVG | `.svg` | Yes | Yes | Yes | SVG 1.1 / SVG 2 (`vectorcraft-svg`), CSS styling, text/outlines, linked/embedded images, multi-artboard export, and Preserve Editing native restoration. |
| `svgz` | Compressed SVG | `.svgz` | Yes | Yes | Yes | Gzip-compressed SVG read, place, save, and export. |
| `pdf` | PDF | `.pdf` | Yes | Yes | Yes | ISO 32000-1 PDF 1.4–1.7 & PDF/X (`vectorcraft-pdf`), multi-page import/export, page boxes (`crop`/`trim`/`bleed`/`media`/`art`), CMYK/RGB/Gray/Spot inks, passwords, and Preserve Editing restoration. |
| `ai` | Adobe Illustrator | `.ai` | Yes | Yes | Yes | PDF-compatible `.ai` and legacy PostScript `.ai` (`legacy_ai`), with native editing-data restoration. |
| `ait` | Adobe Illustrator Template | `.ait` | Yes | Yes | Yes | Opens as an untitled document from template; written via `file.saveAsTemplate`. |
| `eps` | EPS / PostScript | `.eps`, `.ps`, `.epsf`, `.epsi` | Yes | Yes | Yes | Clean-room PostScript interpreter (`vectorcraft-eps`), TIFF preview fallback, transparency flattening, and Preserve Editing restoration. |
| `dxf` | AutoCAD DXF | `.dxf` | Yes | Yes | Yes | ASCII DXF (`vectorcraft-cad` R12–2018), model/paper layouts, blocks/symbols, layers, unit scaling, and multi-artboard export. |
| `emf` | Enhanced Metafile | `.emf` | Yes | Yes | Yes | Windows Enhanced Metafile (`vectorcraft-metafile`), Bezier curves, clipping, gradients, and images. |
| `wmf` | Windows Metafile | `.wmf` | Yes | Yes | Yes | Placeable 16-bit Windows Metafile (`vectorcraft-metafile`). |
| `affinity` | Affinity | `.af`, `.afdesign`, `.afpub`, `.afphoto` | Yes | Yes | No | Affinity Designer / Publisher / Photo container reader (`vectorcraft-affinity`). |
| `png` | PNG | `.png` | Yes | Yes | Yes | 8/16-bit RGBA/RGB/Gray PNG, pHYs PPI metadata, sRGB/ICC profile embedding, Adam7 interlacing, and HTML/NCSA image maps. |
| `png8` | PNG-8 | `.png` | — | — | Yes | Indexed 2–256 color PNG export with perceptual/selective/adaptive/web quantization and diffusion/pattern/noise dithering. |
| `jpg` | JPEG | `.jpg`, `.jpeg`, `.jpe`, `.jfif` | Yes | Yes | Yes | Baseline, optimized, and progressive JPEG (`rgb`/`cmyk`/`gray`), quality 0–100, and ICC profile embedding. |
| `webp` | WebP | `.webp` | Yes | Yes | Yes | Lossless RGBA WebP read, place, and export. |
| `gif` | GIF | `.gif` | Yes | Yes | Yes | Indexed 2–256 color GIF read, place, and export with transparency, matte color, interlacing, and dithering. |
| `tiff` | TIFF | `.tif`, `.tiff` | Yes | Yes | Yes | Little/big-endian TIFF (`rgb`/`cmyk`/`gray`), LZW compression, alpha channel, and ICC profile embedding. |
| `bmp` | BMP | `.bmp`, `.dib` | Yes | Yes | Yes | Windows and OS/2 BMP/DIB (1, 4, 8, 16, 24, 32-bit, RLE4/RLE8, top-down/bottom-up rows). |
| `tga` | TGA | `.tga`, `.icb`, `.vda`, `.vst` | Yes | Yes | Yes | Truevision TGA (16, 24, 32-bit uncompressed and RLE). |
| `psd` | Photoshop PSD / PSB | `.psd`, `.psb` | Yes | Yes | Yes | Layered Photoshop PSD/PSB (`rgb`/`cmyk`/`gray`), layer groups, blend modes, opacity, and `maxEditability` object layers. |
| `txt` | Plain Text | `.txt` | No | Yes | Yes | Places text into text frames; exports document text stories in stacking order (UTF-8 or UTF-16 BOM, LF/CRLF). |

## 2. Color Palette Formats (`coralcraft-color` & `vectorcraft-color`)

| Format | Extensions | Open / Import | Save / Export | Details |
| --- | --- | :---: | :---: | --- |
| **Corel Binary Palette** | `.cpl` | Yes | Yes | Clean-room binary `.cpl` parser and serializer supporting `0xCDDC` (RGB), `0xCCDC` (CMYK), and `0xCDCD` (Spot / Pantone) color models (`coralcraft_color::parse_binary_cpl` / `serialize_binary_cpl`). |
| **Corel XML Palette** | `.xml` | Yes | Yes | Clean-room Corel XML palette parser and serializer with normalized `0.0..=1.0` / percentage tint handling (`coralcraft_color::parse_xml_palette` / `to_xml_palette`). |
| **VectorCraft Swatch Libraries** | `.ase`, `.aco`, `.gpl`, `.svg` | Yes | Yes | Adobe Swatch Exchange (`.ase`), Photoshop Color Swatches (`.aco`), GIMP Palette (`.gpl`), and SVG swatch libraries in the Swatches panel. |

## 3. Verification & Regression Gates

1. **Full Format Matrix Gate (`V2.1`)**:
   - `cmd::fileio::tests::vectorcraft_format_compatibility_matrix_covers_every_format_v2_1` verifies that all 24 upstream VectorCraft format IDs plus `cdr`, `cmx`, and `coralcraft` are registered in `FORMATS` with exact `read`, `write`, `extensions`, `OPEN_EXTS`, and `PLACE_EXTS` membership, and exercises `document.export`, `document.serialize`, and `document.open` across every format and file extension.
   - `cmd::fileio::tests::the_macos_bundle_opens_every_readable_format` verifies that `packaging/macos/Info.plist.in` declares every extension in `OPEN_EXTS`.
2. **Lossless CoralCraft Extension Block (`V2.2`)**:
   - `cmd::fileio::tests::vectorcraft_drawcraft_and_vctemplate_preserve_coralcraft_extension_block_losslessly_v2_2` verifies that multi-page CorelDRAW documents, master layers (`MasterAllPages`, `MasterOddPages`, `MasterEvenPages`, `Desktop`), Corel palettes, unknown RIFF chunks (`Residual`), and `ObjectKind::Unsupported` nodes survive `.vectorcraft` (v1–v4, compressed and uncompressed), `.drawcraft`, and `.vctemplate` round-trips bit-for-bit, while pure VectorCraft documents do not gain a `"coralcraft"` key.
3. **Legacy `.coralcraft` Import Converter (`V2.3`)**:
   - `cmd::fileio::tests::legacy_coralcraft_json_and_zip_containers_open_and_place_losslessly_v2_3` verifies that pre-fork `.coralcraft` JSON and ZIP files open and place into `vectorcraft-engine` with `converted = true` and lossless `document.unknown["coralcraft"]` preservation.
4. **`libcdr` Differential Oracle & Real-World Sample Suite (`V1.4`)**:
   - `crates/corel-import/tests/oracle_and_samples.rs` and `cmd::fileio::tests::cdr_and_cmx_engine_pipeline_passes_libcdr_oracle_and_real_samples` verify that exported `CDR7` and `CMX1` streams pass `cdr2raw` / `cmx2raw` with expected page and path counts, and that all **65/65 valid real-world `.cdr` samples** in `refs/cdr-samples` open into `vectorcraft_doc::Document` with exact `page_count`, `layer_count`, and `object_count`.
