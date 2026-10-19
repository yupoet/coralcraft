# Parity with CorelDRAW (Target Workflow Matrix)

CoralCraft is a clean-room, open-source vector illustration and multi-page layout application targeting CorelDRAW workflow parity (and beyond). All feature descriptions and keyboard/tool conventions below are derived strictly from public documentation (CorelDRAW public user guides, published technical books, and open specifications).

## Summary Matrix

| Workflow Domain | Current Status (M1) | Target Milestone | Notes |
| --- | --- | --- | --- |
| **Document Model & Multi-Page** | 0% (planned) | M2 / M6 | Multi-page spreads, Master Pages, Desktop layer (`ObjectId` + `Object::Unsupported` lossless passthrough) |
| **Objects & Geometry** | 0% (planned) | M2 | Parametric Rectangle, Ellipse, Polygon/Star, Spiral, Bezier/B-Spline Curve, Artistic & Paragraph Text, Groups |
| **Fills, Outlines & Palette** | 0% (planned) | M2 / M3 | Uniform, Fountain (Linear/Radial/Conical/Square, Direct + Rainbow CW/CCW), Pattern, Calligraphic Outlines, Spot/Palette colors (`.cpl` / `.xml` import) |
| **CorelDRAW Signature Effects** | 0% (planned) | M3 / M7 | PowerClip (`container_id`), Contour, Blend, Envelope, Extrude, Perspective, Lens |
| **Interactive Tools & Node Editing** | 0% (planned) | M5 | Pick, Shape (`F10`), Pen/Bezier, Rectangle (`F6`), Ellipse (`F7`), Polygon (`Y`), Artistic/Paragraph Text (`F8`), Interactive Fill (`G`) |
| **Prepress, Print & Imposition** | 0% (planned) | M8 | Trim/Bleed/Slug boxes, Crop/Registration marks, Color-managed PDF/X export, Hairline check |
| **Automation & Agentic MCP** | 0% (planned) | M9 | `coralcraft-mcp` JSON-RPC server + Rhai/Lua scripting engine |

## 1. Document & Multi-Page Architecture

Unlike single-canvas or artboard-only vector editors, CorelDRAW documents combine a multi-page publication model with a shared pasteboard:

- **Pages & Facing Spreads**: Per-page dimensions, orientation, and bleed next to a document-wide default page size.
- **Document-Level Layer Stack**: Layers (Normal, Master, Guides, Desktop, Grid) are defined at the document level, while object instances bind to `(Page | Master | Desktop, LayerId)`.
- **Desktop Layer**: Objects dragged outside a page boundary automatically migrate to the shared `Desktop` layer and remain visible across page switches.
- **Units & Coordinate Precision**: Internal CDR coordinates use `1 / 254,000 inch` (`0.1 μm` = `1 / 10,000 mm`) signed 32-bit integers with a center-origin (`(0, 0)` at page center, `+Y` up); CoralCraft's `coralcraft-doc` normalizes coordinates to `Pt` (`1/72 inch`, `f64`) while preserving exact unit conversion helpers.

## 2. Objects, Fills & Outlines

- **Parametric Primitives**: Rectangles (per-corner radius and corner type: Round / Scalloped / Chamfered), Ellipses (Pie / Arc angles), Polygons & Stars, Spirals.
- **Curves**: Cubic Bezier and B-Spline subpaths with per-node Cusp / Smooth / Symmetrical continuity flags.
- **Outlines**: Width (`0.0` = Hairline), Calligraphic nib angle and stretch, line caps/joins, dash patterns, start/end arrowheads, and **Behind Fill** / **Scale with Object** flags.
- **Fills**: Uniform, Fountain (Linear, Radial, Conical, Square; Direct RGB/CMYK/Lab and HSB Rainbow CW/CCW interpolation), Pattern, Texture, and PostScript fills.

## 3. CorelDRAW Signature Effects

- **PowerClip**: Any closed curve or parametric shape can act as a clipping container (`powerclip_container_id` / `powerclip_children`) with "Auto-center new content" and "Lock contents to PowerClip" semantics.
- **Live Vector Effects (with Pre-baked Fallback)**: CDR files store both live effect parameters (`Blend`, `Contour`, `Envelope`, `Extrude`) and pre-baked display geometry. M3 imports pre-baked geometry alongside structured effect metadata; M7 provides interactive live editing.
