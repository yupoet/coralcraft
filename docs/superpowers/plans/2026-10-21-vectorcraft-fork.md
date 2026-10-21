# Plan: Rebase CoralCraft on a VectorCraft Fork (Phase V)

> Status: **active** (operator decision 2026-10-21). Supersedes the M-series milestone loop.
> Upstream base: `refs/vectorcraft` @ `a466625` (2026-10-18, `MIT OR Apache-2.0`).

## 1. Operator decisions (2026-10-21)

| ID | Decision |
|---|---|
| **D6** | **Copy, then adapt.** CoralCraft is a fork of VectorCraft. VectorCraft source is copied into this repository and reshaped; it is not consumed as an external dependency. |
| **D7** | **Compatibility with every VectorCraft file.** CoralCraft opens every format VectorCraft opens (including native `.vectorcraft` / `.drawcraft` documents and templates, losslessly) and saves/exports every format VectorCraft saves/exports. |
| **D8** | **Reuse is mandatory.** If VectorCraft already has a subsystem (geometry, boolean ops, rendering, text, effects, tracing, SVG/PDF/EPS/AI/DXF/EMF/WMF/PSD/Affinity I/O, docking UI, widgets, MCP plumbing, packaging), we port it. Writing a from-scratch replacement needs explicit operator approval. |
| **D9** | **CoralCraft's own scope is only the CorelDRAW extension.** That means CDR/CMX read/write, the CorelDRAW multi-page model (pages, master layers, desktop layer), the CorelDRAW workspace (toolbox, property bar, palette bar, page navigator, dockers), CorelDRAW-specific tools and interactive effects, and Corel palettes. |
| **D10** | **Format self-round-trip work is frozen.** M89 is the last milestone of that kind. No new milestones that only make our own SVG/DXF/EPS/PDF export re-import into itself. |

## 2. Why

The 2026-10-21 review measured CoralCraft at about 8–10% of VectorCraft as a desktop application:
- 53k vs 577k Rust lines.
- 2 vs about 40 panels.
- 6 vs 89 working canvas tools.
- 165 vs 8,275 tests.
- No real text engine; full-page CPU re-rasterization on every redraw.

`refs/REFS.md` already named VectorCraft as the "core engine foundation", but the project reimplemented everything instead. Re-basing on VectorCraft closes most of that gap at once, so our effort goes only to CorelDRAW-specific work.

## 3. Repository layout after V0

```text
crates/            <- VectorCraft crates copied verbatim (same directory and package names, e.g. vectorcraft-engine)
crates/cdr/        <- coralcraft-cdr (CoralCraft-owned; no VectorCraft equivalent)
crates/corel-*/    <- new CoralCraft-owned extension crates (workspace, multipage, palettes, ...)
apps/coralcraft/   <- desktop app (rebased on apps/vectorcraft, rebranded)
apps/coralcraft-cli/
legacy/            <- pre-fork CoralCraft crates (geom, color, doc, render, tools, engine, ui-egui, mcp),
                      excluded from the workspace; deleted piece by piece as their unique content is ported
docs/upstream-vectorcraft.md  <- upstream commit, sync log, and list of local divergences
```

Rules:
- Keep upstream package and directory names (`vectorcraft-*`) so upstream fixes can be merged. Rebrand only user-visible strings, app IDs, bundle IDs, and icons.
- Every local change to a VectorCraft-derived file should be small and marked with a `// CoralCraft:` comment when it diverges from upstream behaviour. Put large CorelDRAW features in `corel-*` crates instead.
- VectorCraft-derived crates keep VectorCraft's lint configuration. CoralCraft-owned crates (`cdr`, `corel-*`, apps) keep the strict lints (`indexing_slicing`, `arithmetic_side_effects`, no `unwrap`/`expect`/`panic`).
- Keep VectorCraft copyright notices. Add VectorCraft attribution to `NOTICE` and the VectorCraft `contributors/` list.
- VectorCraft brand assets (logo, splash, icons) are **not** reused. Replace them with CoralCraft originals registered in `ASSETS.md`.

## 4. Phase V task queue (execute in order; one commit per task, `cargo xtask ci` green)

### V0: Fork import and green build
- [x] V0.1 Move pre-fork crates to `legacy/` (except `crates/cdr`). Copy VectorCraft `crates/*`, `apps/*`, `xtask` (merged with ours: keep `cleanroom`, `assets`, `layers`), `vendor/`, `assets/` (non-brand only), `packaging/`, and CI workflows. Record `a466625` in `docs/upstream-vectorcraft.md`.
- [x] V0.2 Workspace builds; VectorCraft's own test suite passes unchanged; `cargo xtask ci` adapted (cleanroom scan still blocks Corel strings and `.cdr`/`.cmx` binaries).
- [ ] V0.3 Rebrand `apps/vectorcraft` -> `apps/coralcraft` (binary name, window title, app ID, bundle ID, About box, prefs directory, MCP server name). Add original CoralCraft icon assets.
- [ ] V0.4 `coralcraft-cli` rebased on `vectorcraft-cli`. Keep the `probe` subcommand from the old CLI.

### V1: CDR/CMX on the VectorCraft document model
- [ ] V1.1 Port `coralcraft-cdr` `parse_document` output to the VectorCraft `Document` (via a `corel-import` mapping layer). Unsupported CDR objects keep a lossless residual.
- [ ] V1.2 `.cdr` / `.cmx` appear in the Open/Place format tables (`FORMATS`, `OPEN_EXTS`, `PLACE_EXTS`) and in MCP `file.open`.
- [ ] V1.3 CDR7 / CDRX / CMX1 writers consume the VectorCraft `Document`. Add Save As / Export entries.
- [ ] V1.4 Re-point the libcdr `cdr2raw` differential tests and the real-sample probe (`refs/cdr-samples`) at the new pipeline. A sample counts as passing only if it opens with a correct page/layer/object count, not merely without crashing.

### V2: Full VectorCraft file compatibility (D7)
- [ ] V2.1 Compatibility matrix test: for every entry in VectorCraft's `FORMATS` table, CoralCraft opens and saves/exports through the same code path. CI fails if a VectorCraft format is missing.
- [ ] V2.2 `.vectorcraft` / `.drawcraft` / `.vctemplate` documents open and save losslessly. CoralCraft-only data (pages, master layers, CDR residuals) is stored in a namespaced extension block that VectorCraft ignores.
- [ ] V2.3 Legacy `.coralcraft` (pre-fork JSON/ZIP) import converter, so existing files still open.
- [ ] V2.4 Update `docs/file-format-parity.md`: VectorCraft formats, CDR/CMX, Corel palettes.

### V3: CorelDRAW multi-page model
- [ ] V3.1 Multi-page documents (per-page size and orientation) on top of VectorCraft artboards (study `refs/designcraft` for spreads and master pages).
- [ ] V3.2 Master layers (all / odd / even pages) and the shared Desktop layer. Render order matches CorelDRAW's public documentation.
- [ ] V3.3 Page navigator (tabs, add/delete/rename/reorder pages).

### V4: CorelDRAW workspace
- [ ] V4.1 CorelDRAW workspace preset on VectorCraft docking: left toolbox, top property bar, right vertical palette bar, bottom page navigator and status bar, right dockers. Saved as a workspace.
- [ ] V4.2 CorelDRAW shortcut scheme (F5–F12, Ctrl+G/U/L/K/Q, Alt+F7–F11, P, G, ...) as a selectable shortcut set, from public documentation only.
- [ ] V4.3 CorelDRAW tool mapping onto VectorCraft tools (Pick, Shape F10, Crop, Zoom, Freehand/Bezier/Pen, Rectangle, Ellipse, Polygon/Star, Text F8, Interactive Fill G). Context-sensitive property bar.
- [ ] V4.4 Dockers: Object Properties (Fill/Outline/Transparency), Objects (Object Manager), Transform, Align & Distribute, Color, Text. Reuse VectorCraft panels; reshape them only where CorelDRAW semantics differ.

### V5: CorelDRAW-specific features
- [ ] V5.1 PowerClip (place inside, edit contents, extract, lock contents).
- [ ] V5.2 Interactive effects with on-canvas handles: Blend, Contour, Distort (Push/Pull, Zipper, Twister), Envelope, Extrude, Drop Shadow, Transparency. Reuse VectorCraft effects and blends where they exist.
- [ ] V5.3 Corel `.cpl` / `.xml` palettes in the palette bar and the swatch system.
- [ ] V5.4 Smart Fill, Connector, Dimension, and PowerTRACE (on VectorCraft `trace`).

### V6: MCP and CLI
- [ ] V6.1 Start from VectorCraft's MCP server. Port only CoralCraft-only tools (CDR/CMX I/O, pages, master layers, CorelDRAW effects). Retire duplicates of VectorCraft tools.
- [ ] V6.2 Regenerate `docs/mcp.md`. Keep `coralcraft-cli mcp` and `coralcraft --mcp`.

### V7: Retire `legacy/`
- [ ] V7.1 Delete each legacy crate once nothing references it. Record what was ported where in `docs/upstream-vectorcraft.md`.

## 5. Acceptance metric (tracked in `ROADMAP.md`)

Re-measure after every V-phase against `refs/vectorcraft`:
- Lines of code, panels, working canvas tools, `#[test]` count, VectorCraft formats opened/saved.
- CDR sample open fidelity.

The target after V2 is VectorCraft parity on everything VectorCraft already does, plus CDR/CMX.

## 6. Upstream sync

- Before each V-phase, check `refs/vectorcraft` for new upstream commits. Merge relevant fixes and log them in `docs/upstream-vectorcraft.md`.
- Never send CorelDRAW-specific code upstream without operator approval.
