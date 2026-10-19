# 03 — Current Milestone Status, Active Tasks & Next Steps

> Last updated: 2026-10-20

## 1. Current Stage Status

- **Stage**: `usable beta baseline` (**M0 through M8 are 100% delivered and passing all CI gates**).
- **Completed in This Cycle**:
  1. **M7: Boolean Path Operations, Live Vector Effects, CDRX ZIP Writer, Binary `.cpl` Palettes & Workspace Enhancements (`commit 6616608`, `f17fd6a`, `7763e2c`)**:
     - `coralcraft-color`: Clean-room binary `.cpl` palette body parser & serializer (`0xCDDC` RGB, `0xCCDC` CMYK, `0xCDCD` Spot/Pantone).
     - `coralcraft-cdr`: Modern X4+ ZIP container writer `write_cdrx_zip` with IEEE 802.3 CRC-32 and stored Deflate archives (`mimetype`, `content/riffData.cdr` `RIFF....CDRE` v1400, `META-INF/metadata.xml`).
     - `coralcraft-geom`: 2D Boolean path operations (`Weld`, `Trim`, `Intersect`, `FrontMinusBack`, `BackMinusFront`) and resolution-independent `artistic_text_to_path` enabling `ConvertToCurves` (`Ctrl+Q`) on `ArtisticText`.
     - `coralcraft-doc`: Non-destructive vector effect stack on `ObjectNode` (`VectorEffect::{Contour, DropShadow, Extrude, Blend}`).
  2. **M8: Embedded Raster Bitmaps (`bmp ` / `CDIB`), Uniform & Fountain Transparency Compositing, Interactive Fill On-Canvas Vector Handles & 27 MCP Tools (`commit 214c9de`, `6c23adf`, and M8 Slice 3)**:
     - `coralcraft-doc`: Added `ObjectKind::Bitmap { pixel_width, pixel_height, dpi_x, dpi_y, rgba_pixels }` and `TransparencyStyle::{Uniform, Fountain}` on `ObjectNode`.
     - `coralcraft-cdr`: Added `decode_loda_bitmap` (shape_code=6), `decode_bmp_or_cdib_chunk` (Windows `BITMAPINFOHEADER` 24-bit BGR / 32-bit BGRA bottom-up scanlines and `BMP ` container), and `write_cdr7` bitmap serialization with round-trip verification.
     - `coralcraft-render`: Added bilinear transformed bitmap blitter (`paint_bitmap`), SVG 1.1 `<image href="data:image/png;base64,...">`, PDF 1.4 Inline Image (`BI ... ID ... EI` with `/AHx`), offscreen premultiplied alpha compositing for `TransparencyStyle::Fountain`, and SVG `<mask>` export.
     - `coralcraft-engine`, `coralcraft-mcp`, `coralcraft-ui-egui`: Added `EngineCommand::SetTransparency`, `set_transparency` MCP tool (27 tools total), `ToolKind::InteractiveFill` (`G`) drag-to-angle vector editing, and on-canvas vector handles (Start color swatch, Corel blue vector line, Midpoint slider, End color swatch).

## 2. Multi-Model Cross-Validation Consensus (M8 -> M9)

- **GLM-5.2 (`zcode`)**: Validated CDR7 `bmp ` / `CDIB` chunk alignment and `loda` shape_code=6 descriptor layout. Recommends M9 focus on `txtd`/`txts` paragraph text frames and multi-column typography flow.
- **Qwen 3.8 Max (`kimi`)**: Verified bilinear edge anti-aliasing with transparent border clamping and Fountain Transparency luminance-to-alpha mapping `α(t) = 1.0 - T(u)` matching SVG `<mask>`.
- **DeepSeek V4.1 Flash (`codebuddy`)**: Confirmed PDF inline image CTM `w 0 0 -h 0 h cm` under flipped page coordinates and designed `BlendSpine` arc-length LUT parameterization for M9.
- **Grok 4.6 (`grok`)**: Verified CorelDRAW Interactive Fill (`G`) on-canvas handle styling and prioritized Paragraph Text Frames (`F8` drag frame), custom Blend Spines, and prepress marks for M9.

## 3. Next Up (M9: Paragraph Text Frames, Custom Blend Spines & Prepress Marks)

1. **Paragraph Text Frames (`ObjectKind::ParagraphText`) & CDR `txtd`/`txts` Extraction**: Word-wrapped text boxes, alignment (`Left`, `Center`, `Right`, `Justify`), multi-column flow, and CDR text chunk decoding.
2. **Custom Path Spine for Blend (`BlendSpine`)**: Cumulative arc-length LUT, tangent/normal frame calculation along a custom `BezierPath` spine, and MCP/engine integration.
3. **Prepress Marks in PDF/SVG Export**: Crop marks, registration targets, and bleed box rendering in `coralcraft-render`.

