# P1 Professional Core Vector Workflow Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver the D1 core and the daily-use parts of D2/D3 — complete selection/node workflows, shaping, snapping, transforms, fills/outlines, core dockers, styles, symbols, pages, master layers, and native persistence — expanding the existing V4.3/V4.4 work, never restarting it. Exit: a user can create and revise a multi-object logo or icon document from scratch; every supported object stays editable after save/reopen; command, UI, and independent export checks pass.

**Architecture:** VectorCraft stays the only application base: VectorCraft documents, engine commands, `vello_cpu` renderer, MCP/plugins. Every capability lands as a vertical slice — model semantics (if any) → shared engine command → Property Bar/Tool/Docker interaction → native persistence → CDR mapping (where the semantic exists in CDR) → MCP/CLI → tests at each layer. TraceDraw is used only per [`docs/coreldraw-reference-adoption.md`](../coreldraw-reference-adoption.md): behaviour references and selective code adaptation with notices; no second engine, document model, text engine, or automation stack.

**Tech Stack:** `vectorcraft-doc`, `vectorcraft-engine` commands, `vectorcraft-tools`, `vectorcraft-ui-egui` (toolbar/Property Bar/panels), `corel-import`, `vectorcraft-mcp`, Rust workspace tests, MCP control-port scenarios, independent SVG/PDF export checks.

**Approved design:** `docs/superpowers/specs/2026-11-09-coreldraw-main-application-roadmap-design.md` (umbrella, §P1 and §6 evidence model).
**P0 evidence base:** `docs/target-app-parity.md` (levels), `docs/coreldraw-reference-adoption.md` (decisions), `docs/cdr-verification.md` (CDR state; S2 may still be open), `docs/ai-memory/05-ref-docs/07-tracedraw.md` (audit).

**Scope and commit boundary:** each task is one vertical slice and one or more commits with focused tests green before the next task starts. Do not start a slice whose inventory step found the owning VectorCraft capability absent — record the gap and take the next task instead.

---

## Mandatory rules for every task

- Same discipline as P0: do not stage or revert the unrelated `AGENTS.md`/`CLAUDE.md`/`.claude/` changes; never read `refs/uniconvertor/`; never launch proprietary Corel software; no sample files, fonts, or reference-repo contents in commits.
- Before every commit: `node .gitnexus/run.cjs detect-changes --scope all --repo .` must report no `partial`/`truncated` and only the task's files; stop on unexpected HIGH/CRITICAL impact.
- Every new or changed user-visible behaviour derives from public CorelDRAW documentation (cite the guide URL in the task) or from the audited TraceDraw behaviour notes — never from running Corel software.
- Every new engine command lands with: headless behaviour test, undo/redo test, UI interaction test (where interactive), MCP surface (registry + regenerated `docs/mcp.md`), and a `docs/target-app-parity.md` level update citing the new evidence.
- A capability is **Interoperable (4)** only when native reopen preserves editability AND an independent (non-self) import/export check passes; self round-trips never raise a level.

## File map (expected touch points; tasks may adjust after their inventory step)

- `crates/tools/src/catalog.rs`, `crates/tools/src/*.rs` — tool catalog and per-tool options.
- `crates/ui-egui/src/toolbar.rs`, `crates/ui-egui/src/chrome/control_bar.rs` — toolbox modes and the context-sensitive Property Bar.
- `crates/ui-egui/src/panels/{properties,layers,transform,align}.rs`, `crates/ui-egui/src/dialogs/*.rs` — core dockers and dialogs.
- `crates/engine/src/cmd/*.rs` — shared commands (selection, node editing, path ops, snapping, transforms, fills/outlines, styles, symbols).
- `crates/corel-import/src/lib.rs`, `crates/engine/src/cmd/fileio/native.rs` — CDR mapping and native persistence of any new semantics.
- `crates/mcp/src/**`, `crates/mcp/tests/docs_sync.rs`, `docs/mcp.md` — automation surface and regenerated docs.
- `docs/target-app-parity.md`, `docs/gaps.md`, `docs/ai-memory/03-current.md` — evidence and queue records.

---

### Task 1: P1 baseline inventory and evidence anchors

**Files:**
- Create: `docs/p1-inventory.md` (working notes, committed)
- Modify: `docs/target-app-parity.md` (P1 rows get a "P1 slice" annotation column entry only if missing)

- [ ] **Step 1: Enumerate the synced capabilities the slices build on**

```bash
cargo run -q --package coralcraft-cli -- commands > plan/p1-commands.txt
rg -n 'pub fn' crates/tools/src/catalog.rs | head -40
ls crates/ui-egui/src/panels/ crates/ui-egui/src/dialogs/
```

Record in `docs/p1-inventory.md`: the actual path-op/shape command ids, the tool catalog ids and option keys, the panel/dialog file names for Properties/Objects/Transform/Align, the snapping-related commands and preferences, and the style/symbol command set. Mark each P1 slice's starting point as *exists / partial / absent*.

- [ ] **Step 2: Commit the inventory**

```bash
git add docs/p1-inventory.md
git commit -m "docs: inventory the P1 core vector capabilities"
```

### Task 2: CorelDRAW tool mapping and Property Bar modes (V4.3 core)

**Files:** `crates/tools/src/catalog.rs`, `crates/ui-egui/src/toolbar.rs`, `crates/ui-egui/src/chrome/control_bar.rs`, focused test modules beside them.

- [ ] **Step 1:** From the inventory and the public guide (cite URLs), define the CorelDRAW core-tool → VectorCraft tool mapping (Pick, Shape, Freehand, Bézier, Rectangle, Ellipse, Polygon, Spiral, Text, Zoom, Pan) with each tool's Property Bar fields (option keys, defaults, live updates).
- [ ] **Step 2:** Implement mapping data + Property Bar row builders for the core tools over `tool.select`/`tool.setOption` and existing engine commands; no new engines.
- [ ] **Step 3:** Tests: `tool.select`/`tool.setOption` round trips per mapped tool; Property Bar UI tests (fields appear, values reflect selection, edits run the command) in the style of `paragraph/tests.rs::the_control_bar_*`.
- [ ] **Step 4:** Update the parity row "CorelDRAW tool mapping and Property Bar" with the new evidence link; run the mandatory checks; commit `feat: map the CorelDRAW core tools and property bar`.

### Task 3: Selection and node-editing workflows (D1, level 3→4)

**Files:** node/selection command module(s) found in Task 1, `crates/tools/src/*shape*`, Property Bar rows for node editing.

- [ ] **Step 1:** Close the measured gaps from the inventory (node select/move/add/delete, segment convert, marquee selection, selection modifiers) as commands-first slices.
- [ ] **Step 2:** Wire the Shape-tool interaction and its Property Bar to those commands; keep VectorCraft node editing behaviour unchanged when no Corel mapping applies.
- [ ] **Step 3:** Tests: headless node-edit scenario scripts, undo/redo, UI interaction (kittest) for the Shape tool; native save/reopen keeps nodes editable; independent SVG export shows the edited geometry (non-self check).
- [ ] **Step 4:** Parity row update + commit `feat: complete the CorelDRAW selection and node workflows`.

### Task 4: Shaping (weld/trim/intersect/simplify…) as commands + Property Bar

- [ ] **Step 1:** Map CorelDRAW shaping operations onto the synced pathops command set (inventory names them); add only missing operations as engine commands on `vectorcraft-pathops`.
- [ ] **Step 2:** Property Bar/docker entry points with CorelDRAW names; shortcuts per the public guide where already bound in the CorelDRAW preset.
- [ ] **Step 3:** Tests per operation: command behaviour on synthetic documents, undo, independent SVG/PDF export diff (geometric equality within tolerance), CDR export mapping when the semantic exists in CDR (cite `corel-import`).
- [ ] **Step 4:** Parity row "Shaping and path operations" + commit `feat: expose CorelDRAW shaping through shared commands`.

### Task 5: Snapping and dynamic guides

- [ ] **Step 1:** Inventory the synced snapping engine; map CorelDRAW snap modes (grid, guidelines, objects with edge/center/node/intersection, dynamic guides) onto it; add the missing modes as engine preference + command state.
- [ ] **Step 2:** Snap settings UI (Status Bar/Property Bar toggles) and guideline behaviours for daily use.
- [ ] **Step 3:** Tests: snap-resolution tests with synthetic geometry (deterministic), UI toggle tests, persistence of snap preferences.
- [ ] **Step 4:** Parity row update + commit `feat: align snapping with the CorelDRAW daily-use modes`.

### Task 6: Transforms, Align & Distribute docker behaviours

- [ ] **Step 1:** Free transform modes (rotate/skew/mirror/scale from the Property Bar), apply-to-duplicate, transform docker numeric fields — all through existing engine transform commands.
- [ ] **Step 2:** Align & Distribute docker: CorelDRAW options inventory vs the synced `align` panel; add missing distribute modes as commands.
- [ ] **Step 3:** Tests: command + UI tests per mode; independent export check for a transformed composition.
- [ ] **Step 4:** Parity row + commit `feat: complete the CorelDRAW transform and align workflows`.

### Task 7: Fills, outlines, and the Outline Pen dialog (daily use)

- [ ] **Step 1:** Uniform/gradient fill application from the palette bar and Property Bar (V4.1 base); outline width/colour/arrowheads/caps/joins from the synced appearance model; default fill/outline persistence per document.
- [ ] **Step 2:** Outline Pen dialog parity fields (public guide citation) wired to appearance commands; keep VectorCraft appearance semantics.
- [ ] **Step 3:** Tests: appearance round trips (native reopen keeps fills/outlines editable), independent SVG/PDF export checks, CDR export mapping for the covered semantics.
- [ ] **Step 4:** Parity row "Fills, outlines, palettes, and transparency" + commit `feat: finish the CorelDRAW fill and outline daily workflow`.

### Task 8: Core dockers — Object Properties, Objects, Transform, Align (V4.4)

- [ ] **Step 1:** Behaviour spec per docker from the audited TraceDraw notes (behaviour reference only — its app crate is not adoptable at this pin) and the public guide; map each field to the shared commands.
- [ ] **Step 2:** Implement missing fields/behaviours on the synced panels under the CorelDRAW workspace; no new panel framework.
- [ ] **Step 3:** Tests: per-docker interaction tests (fields reflect selection; edits run commands; undo works), MCP-visible equivalence for each docker action.
- [ ] **Step 4:** Parity rows for the affected dockers + commit `feat: complete the CorelDRAW core docker behaviours`.

### Task 9: Object and colour styles (basics) + symbols (basics)

- [ ] **Step 1:** Map CorelDRAW object/colour styles onto the synced appearance/graphic-style system; symbol manager basics onto the synced symbols (create from selection, place instance, edit source).
- [ ] **Step 2:** Style/symbol docker or panel wiring per the CorelDRAW workspace; commands-first.
- [ ] **Step 3:** Tests: style apply/redefine propagation, symbol instance update-on-source-edit, native persistence, independent export of styled/symbol documents.
- [ ] **Step 4:** Parity rows + commit `feat: ground CorelDRAW styles and symbols on the VectorCraft base`.

### Task 10: Pages and master layers — daily-use completion

- [ ] **Step 1:** From the parity row (level 4) close only the measured daily-use gaps the inventory lists (page reorder UX, per-page defaults, master-layer creation flows).
- [ ] **Step 2:** Tests extend the V3 suite; native + CDR page/layer round trips stay green.
- [ ] **Step 3:** Parity row update + commit `feat: finish the CorelDRAW page and master-layer daily use`.

### Task 11: Native persistence and CDR mapping for all P1 semantics

- [ ] **Step 1:** Audit every P1 slice's semantics for native persistence (`.vectorcraft` reopen editable) and CDR export mapping through `corel-import`; add mapping tests; unknowns degrade to documented residuals with warnings (never silent loss).
- [ ] **Step 2:** Tests: extension-block growth tests, CDR export acceptance where the semantic exists in CDR, hostile-input regressions stay green.
- [ ] **Step 3:** Commit `feat: persist and map the P1 vector semantics`.

### Task 12: MCP/CLI surface and regenerated documentation

- [ ] **Step 1:** Ensure every new command is registered, scriptable, and covered headlessly; regenerate `docs/mcp.md` (`CORALCRAFT_BLESS=1 cargo test -p vectorcraft-mcp --test docs_sync`).
- [ ] **Step 2:** Commit `feat: expose the P1 commands through MCP and CLI`.

### Task 13: P1 acceptance scenario and gate

- [ ] **Step 1:** MCP-driven end-to-end script: build a multi-object logo (shapes, paths, text, styles, symbols) → edit nodes/transforms/fills → save `.vectorcraft` → reopen → verify editability → export SVG + PDF → independent verification (SVG re-import geometry diff; PDF structure/text check). Record the exact scenario and outputs in `docs/p1-inventory.md`.
- [ ] **Step 2:** Full gate: `cargo xtask ci` (+ documented known platform failures recorded verbatim, not silenced) and the focused P1 suites.
- [ ] **Step 3:** Update `docs/target-app-parity.md` levels from the new evidence only; update `docs/gaps.md`, `docs/ai-memory/03-current.md`, and `ROADMAP.md` P1 status; write the P2 handoff line.
- [ ] **Step 4:** Commit `docs: record the P1 acceptance evidence` and, if S2 prerequisites appeared meanwhile, run the real CDR certification per `docs/cdr-verification.md` before closing S2.
