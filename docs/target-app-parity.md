# Parity with CorelDRAW (Target Workflow Matrix)

CoralCraft is a clean-room, open-source vector illustration and multi-page layout application targeting CorelDRAW workflow parity (and beyond). All feature descriptions and keyboard/tool conventions below are derived strictly from public documentation (CorelDRAW public user guides, published technical books, and open specifications).

## Summary Matrix

| Workflow Domain | Current Status (M4/M5) | Target Milestone | Notes |
| --- | --- | --- | --- |
| **Document Model & Multi-Page** | **100%** (delivered) | M2 (Delivered) | Multi-page spreads, Master Pages (`MasterAllPages`, `MasterOddPages`, `MasterEvenPages`), Desktop pasteboard (`ObjectId` + `ObjectKind::Unsupported` lossless passthrough) |
| **Objects & Geometry** | **85%** (delivered) | M2 (Delivered) | Parametric Rectangle (`Round`/`Scalloped`/`Chamfered`), Ellipse (`Full`/`Pie`/`Arc`), Polygon/Star, Cubic/Quadratic Bézier Curve, Artistic Text, Groups |
| **Fills, Outlines & Palette** | **80%** (delivered) | M2 / M3 (Delivered) | Uniform, Fountain (`Linear`/`Radial`/`Conical`/`Square`, Direct + Rainbow CW/CCW), Outline (`behind_fill`, `scale_with_object`, Hairline), Spot/Registration/Lab colors, `.cpl` / `.xml` palette import |
| **CorelDRAW Signature Effects** | **40%** (PowerClip delivered) | M3 / M7 | Recursive PowerClip (`powerclip_children`, `MAX_CLIP_DEPTH = 8`) in PNG, SVG 1.1, and PDF 1.4 |
| **Interactive Tools & Classic Workspace** | **75%** (delivered) | M4 (Delivered) | Classic CorelDRAW workspace (`MenuBar`, `StandardToolbar`, context-sensitive `PropertyBar`, left `Toolbox` (`Pick`, `Shape F10`, `Crop`, `Zoom Z`, `Freehand/Bezier F5`, `Rectangle F6`, `Ellipse F7`, `Polygon/Star Y`, `Artistic Text F8`, `Interactive Fill G`), right `ColorPaletteBar` (`[X]` No-Color, Left-Click Fill / Right-Click Outline), bottom `PageNavigatorBar`, `StatusBar`, `Objects`/`Properties` Dockers) |
| **Prepress, Print & Imposition** | **45%** (delivered) | M3 / M8 | ISO 32000-1 PDF 1.4 `/MediaBox` + `/TrimBox` + `/BleedBox`, native CMYK (`k`/`K`), Spot/Registration/CIE Lab D50<->D65 Bradford CAT |
| **Automation & Agentic MCP** | **75%** (MCP server delivered) | M5 (Delivered) | `coralcraft-mcp` JSON-RPC 2.0 server (`12` authoring & export tools) + `coralcraft --mcp` |

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
