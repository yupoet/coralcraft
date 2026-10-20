# Parity with CorelDRAW (Target Workflow Matrix)

> [!NOTE]
> **Updated 2026-10-21 (`R0.1`–`R8.1` completed):** All workflow domains below are backed by behavioural tests, including the native `eframe`/`egui` 0.36 desktop workspace shell (`CoralCraftApp` + `egui_kittest::Harness`), planar subdivision polygon boolean clipper (`boolean.rs` with area-identity & grid-oracle tests), Kaitai/Photopea-aligned CDR7/CMX1 writers verified against `libcdr` (`cdr2raw` / `cmx2raw`), and 244 typed MCP tools (`coralcraft-mcp`). Re-estimated per-dimension scores are in [`ROADMAP.md`](../ROADMAP.md) and the completed work queue is in [`gaps.md`](gaps.md).


CoralCraft is a clean-room, open-source vector illustration and multi-page layout application targeting CorelDRAW workflow parity (and beyond). All feature descriptions and keyboard/tool conventions below are derived strictly from public documentation (CorelDRAW public user guides, published technical books, and open specifications).

## Summary Matrix

| Workflow Domain | Current Status (M0–M14) | Target Milestone | Notes |
| --- | --- | --- | --- |
| **Document Model & Multi-Page** | **100%** (delivered) | M2 (Delivered) | Multi-page spreads, Master Pages (`MasterAllPages`, `MasterOddPages`, `MasterEvenPages`), Desktop pasteboard (`ObjectId` + `ObjectKind::Unsupported` lossless passthrough) |
| **Objects & Geometry** | **100%** (delivered) | M2–M13 (Delivered) | Parametric Rectangle (`Round`/`Scalloped`/`Chamfered`), Ellipse (`Full`/`Pie`/`Arc`), Polygon/Star, Cubic/Quadratic Bézier Curve, Fillet/Scallop/Chamfer curves, Artistic Text, Paragraph Text multi-column frames, Linear Dimension lines, Embedded Bitmaps, 2D Boolean ops (`Weld`/`Trim`/`Intersect`/`FrontMinusBack`/`BackMinusFront`), Groups |
| **Fills, Outlines & Palette** | **100%** (delivered) | M2–M8 (Delivered) | Uniform, Fountain (`Linear`/`Radial`/`Conical`/`Square`, Direct + Rainbow CW/CCW), Uniform/Fountain Transparency (`TransparencyStyle`), Outline (`behind_fill`, `scale_with_object`, Hairline), Spot/Registration/Lab colors, binary `.cpl` / `.xml` palette import & export |
| **CorelDRAW Signature Effects** | **100%** (delivered) | M3–M13 (Delivered) | Recursive PowerClip (`MAX_CLIP_DEPTH = 8`), Contour, Drop Shadow, Extrude, Blend + custom `ArcLengthTable` Spine, 4-corner Envelope (`enve` CDR round-trip), 2-point Perspective homography, Artistic Media Brush, Fit Text to Path |
| **Interactive Tools & Classic Workspace** | **100%** (delivered) | M4–M12 (Delivered) | Classic CorelDRAW workspace (`MenuBar`, `StandardToolbar`, context-sensitive `PropertyBar`, left `Toolbox` with 10 vector glyphs, right `ColorPaletteBar` (`[X]` No-Color, Left-Click Fill / Right-Click Outline), bottom `PageNavigatorBar`, `StatusBar`, `Objects`/`Properties` Dockers, `1440×900` full-workspace rasterizer, Interactive Fill handles, dashed `#0055E5` guidelines, 7-mode Alignment + 8-mode Distribution, Step & Repeat) |
| **Prepress, Print & Interchange** | **100%** (delivered) | M3–M14 (Delivered) | ISO 32000-1 PDF 1.4 (`/MediaBox`, `/TrimBox`, `/BleedBox`, CMYK `k`/`K`), `PrepressOptions` (crop & Registration `1 1 1 1 K` marks), `EPSF-3.0` (`export_eps`), AutoCAD DXF (`export_dxf` / `import_dxf`), SVG 1.1 (`export_svg` / `import_svg`), CDR7/CMX1/CDRX writers |
| **Automation & Agentic MCP** | **90%** (delivered) | M5–M64 / R4.1–R4.2 (Delivered) | `coralcraft-mcp` JSON-RPC 2.0 server (**244** typed authoring, effect, layout, and interchange tools with generated `inputSchema.properties`) + `coralcraft --mcp` & `coralcraft-cli mcp` |

## 1. Document & Multi-Page Architecture

Unlike single-canvas or artboard-only vector editors, CorelDRAW documents combine a multi-page publication model with a shared pasteboard:

- **Pages & Facing Spreads**: Per-page dimensions, orientation, and bleed next to a document-wide default page size.
- **Document-Level Layer Stack**: Layers (Normal, Master, Guides, Desktop, Grid) are defined at the document level, while object instances bind to `(Page | Master | Desktop, LayerId)`.
- **Desktop Layer**: Objects placed on the `Desktop` pasteboard remain visible across page switches on screen while staying non-printing by default during export.
- **Units & Coordinate Precision**: Internal CDR coordinates use `1 / 254,000 inch` (`0.1 μm` = `1 / 10,000 mm`) signed integers; `coralcraft-geom` provides exact `CdrUnit(i64)` <-> `Pt(F64)` conversion (`31_750 / 9` rational).

## 2. Classic CorelDRAW Workspace & Interactive Tool Architecture

- **Top Chrome**: Fixed 11-category Menu Bar (`File`, `Edit`, `View`, `Layout`, `Object`, `Effects`, `Bitmaps`, `Text`, `Tools`, `Window`, `Help`), Standard Toolbar, and context-sensitive Property Bar driven by `coralcraft_tools::build_property_bar_schema`.
- **Left Vertical Toolbox**: Single-column tool bar with CorelDRAW shortcuts (`F10` Shape, `Z` Zoom, `F5` Freehand/Bezier, `F6` Rectangle, `F7` Ellipse, `Y` Polygon/Star, `F8` Artistic Text, `G` Interactive Fill, `Escape` Pick).
- **Right Vertical Color Palette Bar**: Top `[X]` No-Color cell followed by scrollable swatches; Left-Click sets Fill, Right-Click sets Outline (updating selection or next-object creation defaults).
- **Bottom Page Navigator & Status Bar**: `+`, `Page 1..N` (Rename, Duplicate, Delete), `Master Page`, and `Desktop` tabs above the Status Bar (`x, y`, selection summary, Fill chip, Outline chip).
- **Right Collapsible Dockers**: Tabbed `Objects` (Object Manager tree) and `Properties` inspector.
