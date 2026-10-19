# 03 — Current Milestone Status, Active Tasks & Next Steps

> Last updated: 2026-10-20

## 1. Current Stage Status

- **Stage**: `usable beta baseline` (**M0 through M9 are 100% delivered and passing all CI gates**).
- **Completed in This Cycle**:
  1. **M8: Embedded Raster Bitmaps (`bmp ` / `CDIB`), Uniform & Fountain Transparency Compositing, Interactive Fill On-Canvas Vector Handles & 27 MCP Tools (`commit 214c9de`, `6c23adf`, `04856ec`)**:
     - `coralcraft-doc`: Added `ObjectKind::Bitmap { pixel_width, pixel_height, dpi_x, dpi_y, rgba_pixels }` and `TransparencyStyle::{Uniform, Fountain}` on `ObjectNode`.
     - `coralcraft-cdr`: Added `decode_loda_bitmap` (shape_code=6), `decode_bmp_or_cdib_chunk`, and `write_cdr7` bitmap serialization with round-trip verification.
     - `coralcraft-render`: Added bilinear transformed bitmap blitter (`paint_bitmap`), SVG 1.1 `<image href="data:image/png;base64,...">`, PDF 1.4 Inline Image (`BI ... ID ... EI` with `/AHx`), offscreen premultiplied alpha compositing for `TransparencyStyle::Fountain`, and SVG `<mask>` export.
  2. **M9: Paragraph Text Frames (`ObjectKind::ParagraphText`), CDR `txtd`/`PTXT` Round-Trip, Custom Blend Spines (`ArcLengthTable`), Prepress Marks (`PrepressOptions`) & 29 MCP Tools (`commit b656584` and M9 Slice 2/3)**:
     - `coralcraft-doc`: Added `ObjectKind::ParagraphText(Box<ParagraphTextSpec>)`, `TextAlignment` (`Left`, `Center`, `Right`, `Justify`), and `layout_paragraph_text` multi-column word-wrapping and overflow detection.
     - `coralcraft-cdr`: Added `txtd`/`txts` text chunk decoding (ASCII/UTF-8/UTF-16LE) and `loda` `shape_code=7` (`PTXT`) round-trip in `write_cdr7`.
     - `coralcraft-geom`, `coralcraft-engine`, `coralcraft-mcp`: Added `ArcLengthTable` cumulative arc-length LUT (`from_path`, `sample_normalized`), `VectorEffect::Blend { target_id, steps, spine: Option<Path> }`, `EngineCommand::AttachBlendSpine`, and 29 MCP tools (`add_paragraph_text`, `attach_blend_spine`).
     - `coralcraft-render`: Added raster/SVG `<tspan>`/PDF `Tw` word-spacing rendering for `ParagraphText`, live intermediate Blend step rendering along custom spine paths, and `PrepressOptions` (`export_pdf_with_prepress`, `export_svg_with_prepress`) with corner crop marks and Registration `CMYK 100/100/100/100` (`1 1 1 1 K`) bullseye targets.

## 2. Multi-Model Cross-Validation Consensus (M9 -> M10)

- **GLM-5.2 (`zcode`)**: Specified `txtd` encoding flags (`0x00` ASCII, `0x01` UTF-8, `0x02` UTF-16LE), `BlendSpine` cumulative arc-length LUT, and Registration `1 1 1 1 K` prepress mark geometry.
- **Qwen 3.8 Max (`kimi`)**: Provided multi-column word-wrapping and justification formulas ($\delta = (W_c - W_L) / (N_w - 1)$) and binary-search inverse arc-length parameterization.
- **DeepSeek V4.1 Flash (`codebuddy`)**: Designed `ArcLengthTable` over `flatten_path` in `coralcraft-geom` and `PrepressOptions` preserving byte-identical output for default options.
- **Grok 4.6 (`grok`)**: Prioritized M10: Envelope / Perspective 4-corner distortion (`VectorEffect::Envelope`), Artistic Media / Calligraphic brush strokes, and Paragraph Text dashed frame + red overflow indicator on canvas.

## 3. Next Up (M10: Envelope / Perspective Distortion, Calligraphic Brush Strokes & Canvas Paragraph Frame Overlay)

1. **Envelope & Perspective 4-Corner Cage Distortion (`VectorEffect::Envelope`)**: Bilinear/Coons patch warp of vector paths in `coralcraft-geom`, `coralcraft-doc`, `coralcraft-engine`, and `coralcraft-render`.
2. **Calligraphic & Artistic Media Brush Stroke Profiles**: Variable-width stroke outlines along `Path` spines in `coralcraft-geom` and `coralcraft-render`.
3. **Paragraph Text Frame Canvas Overlay & Red Overflow Indicator**: Dashed frame outline and bottom-center overflow handle in `coralcraft-ui-egui`.

