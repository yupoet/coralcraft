# P1 Capability Inventory (Task 1 baseline)

> Working record for the P1 phase
> ([plan](superpowers/plans/2026-11-09-p1-core-vector-workflow.md)). Enumerated on the current tree
> (2026-11-10, post-P0, `bc9691ddc`) with `coralcraft-cli commands` (933 engine commands, 147 of
> them enabled menu entries), the tool catalog, and the synced `vectorcraft-ui-egui` panel/dialog
> set. Each P1 slice states its VectorCraft starting point as **exists / partial / absent**; a slice
> only implements what is *partial* or *absent* — per the plan, an *absent* owning capability means
> record the gap and take the next task instead.

## 1. Command surface the slices build on

### Selection and node editing (Tasks 2–3)

- `select.*` — 55 commands: `all`, `allOnArtboard`, `none`, `reselect`, `inverse`, `nextAbove`,
  `nextBelow`, `set`, `add`, `toggle`, `key`, `anchors`, …
- `path.*` — 34 commands: anchor editing (`appendAnchor`, `insertAnchor`, `removeAnchor`,
  `removeAnchors`, `deleteAnchors`, `moveAnchor`, `moveAnchors`, `setAnchors`, `setHandle`,
  `convertAnchor`, `convertAnchors`, `cutAtAnchors`, `split`), curve building (`create`, `close`,
  `join`, `joinScrub`, `average`, `averageJoin`, `reshape`, `reshapeSegment`, `reverse`,
  `setFillRule`, `curvature`, `curvatureEdit`), freehand/eraser family (`freehand`, `blob`,
  `eraseRegion`, `eraseSegments`, `smoothRegion`), cutting (`knife`, `lineCut`, `rectCut`,
  `mirrorCut`).
- `node.move` (ruler-guide node move, not path nodes) — path-node moves go through
  `path.moveAnchor`/`path.moveAnchors`.
- Interactive node editing lives in `crates/pathops/src/edit.rs` behind the `directSelection`,
  `anchorPoint`, `curvature`, `addAnchor`, `deleteAnchor` tools
> (`crates/tools/src/catalog.rs`, `edits_anchors()`).

### Shaping / path operations (Task 4)

- `object.pathfinder.*` — `unite`, `minusFront`, `minusBack`, `intersect`, `exclude`, `divide`,
  `outline`, `crop` (plus `object.path.outlineStroke`, `object.path.divideObjectsBelow`).
- `shapeBuilder.merge` / `shapeBuilder.regions` (interactive Shape Builder tool `shapeBuilder`).
- Compound shapes: `crates/engine/src/cmd/compoundshape.rs`; boolean engine
  `crates/pathops/src/boolean.rs` + `crates/pathops/src/pathfinder.rs`.
- CorelDRAW names (Weld/Trim/Intersect/Simplify/Boundary) and their Property Bar/docker surface do
  not exist yet — that mapping is the slice.

### Snapping and dynamic guides (Task 5)

- Commands: `guide.add`, `guide.list`, `guide.select`, `guide.move`, `guide.remove`,
  `guide.setLayer`; `prefs.get`/`prefs.set`/`prefs.list` cover the snap preferences below;
  `view.snapToGlyph.options` (glyph snapping).
- Snap engine: `crates/tools/src/guides/snap.rs` (`PointSnap`, `DrawSnap`, `Leave`,
  `construction_angles`) with target taxonomy in `crates/tools/src/guides/targets.rs`; interactive
  consumers in `crates/tools/src/{direct,draw2}.rs`.
- Preferences (`crates/engine/src/prefs.rs`): `snapping_tolerance`, `snap_tangent`,
  `snap_perpendicular`, `alignment_guides`, `transform_tools_guides`, `construction_guides`,
  `spacing_guides`, `distance_guides`, `snap_to_point_tolerance`, `glyph_snap`,
  `guide_color/style`, `grid_color/style/gridline_every/grid_subdivisions/grids_in_back`,
  `show_pixel_grid`.
- UI: `snapping_menu` in `crates/ui-egui/src/chrome/control_bar.rs`.
- CorelDRAW snap-mode naming (grid/guides/objects with edge/center/node/intersection; dynamic
  guides) is absent — mapping is the slice.

### Transforms, Align & Distribute (Task 6)

- `object.transform`, `object.move`, `object.rotate`, `object.scale`, `object.reflect`,
  `object.shear`, `object.transformAgain`, `object.transformEach`, `object.nudge`,
  `object.arrange.*`, `edit.duplicate`.
- `object.align`, `object.distribute`, `object.distributeSpacing`.
- Panels: `crates/ui-egui/src/panels/transform.rs` (Transform docker) and
  `crates/ui-egui/src/panels/align.rs` (Align; `Button::Distribute`/`Button::Spacing` wired to
  `object.distribute`/`object.distributeSpacing` with an `AlignTo` model).
- Free-transform tool `freeTransform` exists in the catalog.

### Fills, outlines, Outline Pen (Task 7)

- `paint.*` — 22 commands: `setFill`, `setStroke`, `none`, `swap`, `default`, `complement`,
  `invert`, `toggleActive`, `recent`, `proxies`, `sampleColor`, `editGradient`, `setGradientGeom`,
  freeform gradient family (`freeform.*`), `lastColor`, `lastGradient`.
- `stroke.*` — 12: `set`, `setAdvanced`, `weightUp/Down`, width-point/profile commands.
- `gradient.presets.*`; `appearance.*` — 16 (fill/stroke/effect stacks, `copyFrom`, `transfer`).
- Palette bar: `crates/ui-egui/src/palette_bar.rs` (V4.1; left-click Fill / right-click Stroke).
- Stroke panel: `crates/ui-egui/src/panels/stroke.rs`; gradient panel
  `crates/ui-egui/src/panels/gradient.rs`.
- **Absent**: a CorelDRAW Outline Pen dialog (`dialogs/` has no outline-pen dialog; nearest bases
  are `panels/stroke.rs` + `stroke.set`/`stroke.setAdvanced`).

### Styles and symbols (Task 9)

- `graphicStyle.*` — 18 commands (`new`, `apply`, `redefine`, `breakLink`, `merge`, libraries…).
- `symbol.*` — 23 commands (`new`, `place`, `edit`, `exitEditing`, `breakLink`, `replace`,
  `instance.setAppearance`, `spray`, libraries…); symbolism tools `symbolSprayer`…`symbolStyler`.
- `swatch.*` — 23 commands (+ `.cpl`/`.xml` palette support from `crates/color`).
- Panels: `panels/graphic_styles.rs`, `panels/symbols.rs`, `panels/swatches.rs`.

### Pages, master layers (Task 10)

- V3.1–V3.3 complete: `artboard.*` (13 commands; page semantics via V3 extensions
  `insertAt`/`before`/`after`/`orientation`/`applyToAllPages`/`reflowPages`), `layer.*` (26
  commands; `role`/`masterInFront`/`pageIndex` from V3.2), page navigator
  `crates/ui-egui/src/page_navigator.rs`, layer roles `crates/doc/src/coralcraft.rs`.
- Parity row already level 4; the slice only closes measured daily-use gaps.

### Persistence / CDR / MCP (Tasks 11–12)

- Native extension block: `sync_coralcraft_extension` (`crates/corel-import/src/lib.rs`) wired
  into `crates/engine/src/cmd/fileio/native.rs`; CDR mapping `export_document` /
  `import_cdr_bytes`; MCP docs regeneration `crates/mcp/tests/docs_sync.rs`
  (`CORALCRAFT_BLESS=1 cargo test -p vectorcraft-mcp --test docs_sync`).

## 2. Tool catalog and Property Bar base (Task 2)

- Catalog (`crates/tools/src/catalog.rs`, ~100 tools): selection family `selection`,
  `directSelection`, `groupSelection`, `lasso`, `magicWand`; node family `pen`, `addAnchor`,
  `deleteAnchor`, `anchorPoint`, `curvature`; draw family `pencil`, `paintbrush`, `blobBrush`,
  `lineSegment`, `rectangle`, `roundedRectangle`, `ellipse`, `polygon`, `star`, `spiral`,
  `flare`, `arc`, `rectangularGrid`, `polarGrid`, `shapeBuilder`; text `type`, `areaType`,
  `verticalType`, `typeOnPath`, `touchType`; transform `rotate`, `reflect`, `scale`, `shear`,
  `width`, `freeTransform`, `perspectiveSelection`, `puppetWarp`; modify `eraser`, `pathEraser`,
  `scissors`, `knife`, `rotateView`, `smooth`, `twirl`, …; symbols `symbolSprayer` family;
  graphs; `eyedropper`, `measure`, `blend`, `gradient`, `mesh`, `artboard`, `slice`, `hand`,
  `zoom`, `printTiling`.
- Tool options: per-tool option maps read/written generically through
  `tool.select` / `tool.setOption {tool?, key?, value?, values?}`
  (`Session::tool_options_of` / `set_tool_options` in `crates/engine/src/tooling.rs`); e.g.
  `cornerRadius` for rectangles, spiral `segments`/`decay`, star inner radius. There is no fixed
  option-key table; each tool documents its keys via `tool.select` round trip.
- Toolbox UI: `crates/ui-egui/src/toolbar.rs` (971 lines, Illustrator grouping). Property Bar:
  `crates/ui-egui/src/chrome/control_bar.rs` (anchor controls, snapping menu, isolation,
  transform group) — currently Illustrator-shaped, single generic bar, not per-ToolInfo modes.
- CorelDRAW core-tool mapping (Pick/Shape/Freehand/Bézier/Rect/Ellipse/Polygon/Spiral/Text/Zoom/
  Pan with per-tool Property Bar fields) is **absent** — the whole of Task 2. The V4.2 shortcut
  preset (`crates/ui-egui/src/shortcut_editor.rs` "CorelDRAW": F10 Shape, F5 Freehand, F6
  Rectangle, F7 Ellipse, F8 Text, Y Polygon, A Spiral, …) fixes the intended tool identities.

## 3. Slice starting points

| P1 slice | VectorCraft base | Start |
|---|---|---|
| T2 Tool mapping + Property Bar modes | tool catalog, `toolbar.rs`, `control_bar.rs`, `tool.select`/`tool.setOption` | **exists** (P1 Task 2: `crates/tools/src/coreldraw.rs` mapping, CorelDRAW workspace toolbox slots/tips, Property Bar rows in `crates/ui-egui/src/coreldraw_bar.rs`; shape rows reuse the add-on controls) |
| T3 Selection & node editing | `select.*`, `path.*`, `crates/pathops/src/edit.rs`, anchor tools + kittest suites | **exists** (P1 Task 3: added `select.marquee` — the one measured headless gap — and the D1 level-4 evidence: `crates/engine/tests/it/coreldraw_node_workflow.rs`) |
| T4 Shaping | `object.pathfinder.*`, compound shapes, Shape Builder | **exists** (P1 Task 4: Weld/Trim/Intersect/Simplify/minus map onto the shared commands; Boundary added; Property Bar buttons in `coreldraw_bar.rs`) |
| T5 Snapping & dynamic guides | guides/snap engine + prefs + `guide.*` commands + `snapping_menu` | **exists** (P1 Task 5: `view.snapToGuides` toggle added; CorelDRAW Snapping menu names; dynamic guides = construction-guides pref; angle editing open) |
| T6 Transforms, Align & Distribute | `object.*` transforms, `panels/transform.rs`, `panels/align.rs`, `object.align/distribute/distributeSpacing` | **exists** (P1 Task 6: Property Bar mirror/rotation controls; distribute modes independently verified; docker fields = synced panels) |
| T7 Fills/outlines/Outline Pen | `paint.*`, `stroke.*`, `appearance.*`, palette bar, stroke panel | **exists** (P1 Task 7: `dialogs/outline_pen.rs` over the shared stroke commands; F12 rebound in the CorelDRAW preset; appearance round trips proven) |
| T8 Core dockers | `panels/properties.rs`, `panels/layers.rs`, `panels/transform.rs`, `panels/align.rs` on the CorelDRAW workspace preset | **exists** (P1 Task 8: behaviour evidence suite `panels/tests_coreldraw_dockers.rs`; Apply to Duplicate = `copy` param; no missing fields found at daily-use level) |
| T9 Styles & symbols | `graphicStyle.*`, `symbol.*`, `swatch.*` + panels | **exists** (P1 Task 9: propagation/update/persistence evidence; Object Styles & Symbol Manager dockers in the CorelDRAW workspace) |
| T10 Pages & master layers | V3.1–V3.3 (level 4) | **exists** (P1 Task 10: reorder/role/native evidence; CDR7 first-page loss = known-issue 0e, owned by T11) |
| T11 Persistence/CDR | extension block + `corel-import` mapping | **exists** → grow with new semantics |
| T12 MCP/CLI | registry + `docs_sync.rs` | **exists** → regenerate per slice |

## 4. Maintenance

Update this file at every slice that changes a starting point (e.g. T2 turning *partial* →
*exists*), and record the Task 13 acceptance scenario outputs here.
