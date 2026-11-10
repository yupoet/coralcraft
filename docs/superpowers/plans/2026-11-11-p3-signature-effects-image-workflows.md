# P3 Signature Effects and Image Workflows Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver editable PowerClip, advanced fill/transparency, live-effect, bitmap, and tracing workflows with native persistence, explicit interchange fallbacks, shared automation, and independently read poster/traced-image acceptance evidence.

**Architecture:** VectorCraft remains the only document, effects, tracing, renderer, command, and UI base. Reuse its clip groups, `NodeKind::Blend`/`Envelope`, appearance effect stack, mesh and paint models, `vectorcraft-trace`, dockers, and control-bar overlays. Add only CorelDRAW-specific mappings and gaps proven by failing tests; never introduce a TraceDraw document, renderer, text engine, or automation engine.

**Tech Stack:** `vectorcraft-doc`, `vectorcraft-effects`, `vectorcraft-engine`, `vectorcraft-tools`, `vectorcraft-ui-egui`, `vectorcraft-trace`, `corel-import`, `vectorcraft-mcp`, `usvg`, and Hayro-backed `vectorcraft-pdf`.

---

## Mandatory rules

- Do not stage or revert root-workspace `AGENTS.md`, `CLAUDE.md`, or `.claude/` changes. Keep each commit limited to its task files.
- Before editing an existing function, run GitNexus `impact` upstream and review direct callers. Before every commit run `detect_changes`; `partial` or `truncated` is not clean.
- Every production change starts with a failing test that is run and observed failing. Evidence-only tests may pass without forcing new code.
- Level 4 requires native editability after reopen and independent import/export evidence. Self-round trips and skipped tests are not independent evidence.
- CDR/CMX export must preserve supported semantics or emit a warning and lossless residual where possible. S2 remains open without the real oracle and samples.
- `cargo xtask ci` stops at known-issue 0b on this x86_64 host. If it remains the only failure, run each skipped later gate separately and do not claim a clean full gate.

## File map

- `docs/p3-inventory.md`: measured command, UI, persistence, export, and open-gap inventory.
- `crates/engine/tests/it/coreldraw_powerclip.rs`: PowerClip lifecycle and interchange.
- `crates/engine/tests/it/coreldraw_fills_transparency.rs`: advanced paint, mesh, and transparency.
- `crates/engine/tests/it/coreldraw_live_effects.rs`: Blend/Contour/Envelope/Distort and effect-stack evidence.
- `crates/engine/tests/it/coreldraw_image_workflows.rs`: bitmap editing and PowerTRACE evidence.
- `crates/engine/tests/it/coreldraw_p3_persistence.rs`: consolidated native/CDR/CMX persistence and warning audit.
- `crates/engine/tests/it/coreldraw_p3_acceptance.rs`: poster and traced-image acceptance scenarios.
- `crates/engine/tests/it/main.rs`: integration-test module registration.
- `crates/ui-egui/src/panels/tests_coreldraw_effects.rs`: real CorelDRAW workspace control and docker tests.
- `crates/ui-egui/src/coreldraw_bar.rs`, `workspaces.rs`, `menus/`: only measured CorelDRAW-specific UI gaps.
- `crates/engine/src/cmd/object/commands.rs`, `cmd/live/`, `cmd/effectcmd.rs`, `cmd/buildcmds/trace.rs`: only measured shared-command gaps.
- `crates/corel-import/src/lib.rs`, `crates/cdr/src/`: only format mappings established by public specifications and failing tests.
- `crates/mcp/tests/docs_sync.rs`, `docs/mcp.md`: live registry and generated documentation.
- `docs/target-app-parity.md`, `docs/gaps.md`, `ROADMAP.md`, `docs/ai-memory/03-current.md`: evidence and queue updates.

### Task 1: Build the P3 inventory and evidence anchors

**Files:**
- Create: `docs/p3-inventory.md`
- Create: this plan

- [x] **Step 1: Capture the actual catalog and measured baseline**

Run `cargo run -p coralcraft-cli -- commands` and `cargo xtask stats`. Inventory the clip, live, effect, paint, transparency, image, and trace command families; the Blend/Image Trace/Properties/Appearance/Transparency panels; native wire support; CDR mappings; and existing independent SVG/PDF readers.

- [x] **Step 2: Write and commit the inventory**

For every P3 slice record its existing entry points, current test evidence, missing behaviour, and required independent evidence. Commit the inventory and plan with `docs: plan the P3 effects and image workflows`.

### Task 2: Complete the PowerClip editing lifecycle

**Files:**
- Create: `crates/engine/tests/it/coreldraw_powerclip.rs`
- Modify: `crates/engine/tests/it/main.rs`
- Modify: `crates/ui-egui/src/panels/tests_coreldraw_effects.rs`
- Modify only on measured gaps: `crates/engine/src/cmd/object/commands.rs`, `crates/ui-egui/src/menus/`

- [x] **Step 1: Write the failing lifecycle test**

Create two paths in paint order and run:

```rust
s.execute("select.set", &json!({"ids": [contents, frame]})).unwrap();
let clip = s.execute("object.clippingMask.make", &json!({})).unwrap()["id"].as_u64().unwrap();
s.execute("object.clippingMask.editContents", &json!({})).unwrap();
s.execute("object.move", &json!({"dx": 12.0, "dy": 4.0})).unwrap();
s.execute("object.clippingMask.editMask", &json!({})).unwrap();
```

Require editable contents/mask selection, undo/redo, nested clips, release, and native reopen. Export SVG and require independent `usvg` parsing plus a non-empty `clipPath`; export PDF and read it through `vectorcraft_pdf::import`.

- [x] **Step 2: Verify CorelDRAW interaction and fix only measured gaps**

Drive Object menu place-inside/edit/extract actions in the CorelDRAW workspace through their real controls. Reuse `object.clippingMask.*`; add aliases or menu wiring only if the test exposes a missing workflow. Locking contents must not silently flatten the source.

- [x] **Step 3: Verify and commit**

Run `cargo test -p vectorcraft-engine --test it coreldraw_powerclip -- --nocapture` and `cargo test -p vectorcraft-ui-egui coreldraw_effects -- --nocapture`. Commit with `feat: complete the CorelDRAW PowerClip workflow` when production wiring changes, otherwise `test: verify the CorelDRAW PowerClip workflow`.

### Task 3: Verify advanced fills and transparency semantics

**Files:**
- Create: `crates/engine/tests/it/coreldraw_fills_transparency.rs`
- Modify: `crates/engine/tests/it/main.rs`
- Modify: `crates/ui-egui/src/panels/tests_coreldraw_effects.rs`

- [ ] **Step 1: Add failing evidence for each supported paint family**

Exercise fountain, two-colour, vector/bitmap pattern, texture, and mesh fills through existing `paint.*` and `object.mesh.*` commands. For example:

```rust
s.execute("object.mesh.create", &json!({"id": shape, "rows": 2, "cols": 2})).unwrap();
s.execute("object.mesh.setPointColor", &json!({"id": shape, "index": 0, "color": "#ff0000"})).unwrap();
s.execute("object.mesh.movePoint", &json!({"id": shape, "index": 1, "x": 90.0, "y": 30.0})).unwrap();
```

Assert undo/redo, native editability, and independent SVG/PDF structure. Do not accept a preview-only fill as editable semantics.

Progress: fountain, vector-pattern, and mesh tests pass with native edit/reopen and independent SVG/PDF reads. Two-colour/bitmap patterns and textures remain uncovered, so this step stays open.

- [ ] **Step 2: Verify transparency and merge behaviour**

Exercise uniform/graded transparency, merge modes, and opacity masks through existing transparency commands and panels. Require source editability and explicit export fallback where an output format cannot retain the live model.

- [ ] **Step 3: Verify and commit**

Run the new engine/UI filters plus existing mesh and transparency suites. Commit with `test: verify CorelDRAW fills and transparency` if evidence-only, or a focused `feat:` for reproduced gaps.

### Task 4: Prove Blend and Contour interactive editing

**Files:**
- Create: `crates/engine/tests/it/coreldraw_live_effects.rs`
- Modify: `crates/engine/tests/it/main.rs`
- Modify: `crates/ui-egui/src/panels/tests_coreldraw_effects.rs`

- [x] **Step 1: Write the live Blend test**

```rust
let blend = s.execute("object.blend.make", &json!({"ids": [a, b], "steps": 4})).unwrap()["id"].as_u64().unwrap();
s.execute("object.blend.options", &json!({"id": blend, "steps": 7})).unwrap();
s.execute("select.set", &json!({"ids": [blend]})).unwrap();
let info = s.execute("object.blend.info", &json!({})).unwrap();
assert_eq!(info["steps"].as_u64(), Some(7));
```

Edit spine anchors, reverse direction/order, undo/redo, release, and reopen natively. Independent SVG output must contain expanded visible geometry while the native source stays a live Blend.

- [x] **Step 2: Write the Contour and on-canvas evidence**

Exercise the existing contour effect and `canvas/contour_bar.rs` controls with real pointer input; verify parameter edits and source geometry remain editable. Add no second contour renderer or effect model.

- [x] **Step 3: Verify and commit**

Run the live-effect engine filter, CorelDRAW UI filter, and existing blend/contour suites. Commit with `feat: complete CorelDRAW blend and contour editing` or `test:` when evidence-only.

### Task 5: Prove Envelope and Distort editing

**Files:**
- Modify: `crates/engine/tests/it/coreldraw_live_effects.rs`
- Modify: `crates/ui-egui/src/panels/tests_coreldraw_effects.rs`

- [x] **Step 1: Add failing Envelope evidence**

Create a warp envelope, change its options, edit its contents, reopen natively, then reset to a mesh and edit a mesh point before a second reopen and explicit expansion:

```rust
s.execute("select.set", &json!({"ids": [shape]})).unwrap();
let envelope = s.execute("object.envelope.makeWithWarp", &json!({"style": "arc", "bend": 25.0})).unwrap()["id"].as_u64().unwrap();
s.execute("object.envelope.resetWithMesh", &json!({"id": envelope, "rows": 2, "cols": 2})).unwrap();
s.execute("object.envelope.setMeshPoint", &json!({"id": envelope, "index": 4, "x": 85.0, "y": 45.0})).unwrap();
```

- [x] **Step 2: Add Distort and UI evidence**

Apply the existing twister effect through `effect.apply`, edit with `effect.setParams`, and open its real dialog in the CorelDRAW workspace. Require native parameter persistence and independent exported geometry.

- [x] **Step 3: Verify and commit**

Run the new filters and existing envelope/effect suites. Commit only the tested slice.

### Task 6: Complete Extrude, shadows, and remaining signature effects

**Files:**
- Modify: `crates/engine/tests/it/coreldraw_live_effects.rs`
- Modify: `crates/ui-egui/src/panels/tests_coreldraw_effects.rs`

- [ ] **Step 1: Inventory and test each effect against D4**

Cover Extrude, Drop/Block Shadow, Transparency, Lens, Perspective, Bevel, and Symmetry using existing effect IDs discovered by `effect.list`. A representative test applies and edits an effect:

```rust
let applied = s.execute("effect.apply", &json!({"id": shape, "effect": effect_id, "params": params})).unwrap();
let index = applied["index"].as_u64().unwrap();
s.execute("effect.setParams", &json!({"id": shape, "index": index, "params": revised})).unwrap();
```

Require source/parameters to survive native reopen; record unsupported target behaviours explicitly rather than inventing an effect.

Progress: Extrude + Bevel and Drop Shadow pass native/edit/independent SVG+PDF and dialog evidence. Block Shadow, Lens, Perspective, and Symmetry remain open, so this task is not checked.

- [ ] **Step 2: Verify real controls and independent export**

Drive the existing dialogs, property bars, or canvas handles in the CorelDRAW workspace. Parse SVG with `usvg` and PDF with Hayro; distinguish retained vector semantics from raster fallback.

- [ ] **Step 3: Verify and commit**

Run focused engine/UI tests and existing effect catalog suites. Commit each measured implementation gap with its regression test; do not mark absent effects complete from catalog presence alone.

### Task 7: Complete ordered live-effect stack operations

**Files:**
- Modify: `crates/engine/tests/it/coreldraw_live_effects.rs`
- Modify: `crates/ui-egui/src/panels/tests_coreldraw_effects.rs`

- [x] **Step 1: Write the failing stack test**

Apply two compatible effects, then run `effect.move`, `effect.duplicate`, `effect.setParams {"visible": false}`, `effect.remove`, and `effect.expandAppearance`. Assert exact order, parameters, visibility, undo/redo, and native reopen. Verify cloning through the existing clone/copy-properties workflow rather than adding a parallel stack.

- [x] **Step 2: Verify the Appearance docker**

Drive reorder, hide, edit, duplicate, remove, and expand through the actual CorelDRAW workspace Appearance controls. Failed commands must leave document and history unchanged.

- [x] **Step 3: Verify and commit**

Run the focused tests plus `vectorcraft-effects` and Appearance suites. Commit with `feat: complete CorelDRAW live-effect stack editing` or `test:` if evidence-only.

### Task 8: Complete bitmap placement and common editing workflows

**Files:**
- Create: `crates/engine/tests/it/coreldraw_image_workflows.rs`
- Modify: `crates/engine/tests/it/main.rs`
- Modify: `crates/ui-egui/src/panels/tests_coreldraw_effects.rs`

- [ ] **Step 1: Add failing bitmap workflow evidence**

Place a handcrafted synthetic bitmap; exercise crop, resample, colour adjustment, and a common bitmap effect using existing commands. Assert source pixels, adjustment parameters, undo/redo, native reopen, and PowerClip composition. Do not turn editable vectors into bitmaps to satisfy the scenario.

- [ ] **Step 2: Verify external-editor handoff honestly**

Exercise the existing external-editor command if present. Require an explicit unavailable/error result when no editor is configured; no test may claim a round trip it did not execute.

- [ ] **Step 3: Verify and commit**

Run the image workflow and existing bitmap/effect tests. Commit only implemented and observed behaviour.

### Task 9: Complete PowerTRACE outline/centreline workflows

**Files:**
- Modify: `crates/engine/tests/it/coreldraw_image_workflows.rs`
- Modify: `crates/ui-egui/src/panels/tests_coreldraw_effects.rs`

- [ ] **Step 1: Write the failing trace composition test**

Use a handcrafted bitmap and run:

```rust
let traced = s.execute("imageTrace.make", &json!({"id": bitmap, "preset": "Default", "params": {"mode": "color"}})).unwrap()["id"].as_u64().unwrap();
s.execute("imageTrace.setView", &json!({"id": traced, "view": "traced"})).unwrap();
let expanded = s.execute("imageTrace.makeAndExpand", &json!({"id": traced})).unwrap();
assert!(expanded["ids"].as_array().is_some_and(|ids| !ids.is_empty()));
```

Verify outline and centreline modes supported by `vectorcraft-trace`, colour reduction, smoothing, presets, undo/redo, editable expanded paths, and native reopen. Parse the expanded SVG independently with `usvg`.

- [ ] **Step 2: Verify the Image Trace docker and CorelDRAW naming**

Drive preset, mode, colour, smoothing, view, and expand controls in the CorelDRAW workspace. Reuse `panels/image_trace.rs`; only add a CorelDRAW label/mapping where the workflow already has the same semantics.

- [ ] **Step 3: Verify and commit**

Run `coreldraw_image_workflows`, the CorelDRAW UI filter, and `vectorcraft-trace` tests. Commit with `feat: complete CorelDRAW PowerTRACE workflows` or `test:` if evidence-only.

### Task 10: Audit native and CDR/CMX persistence and fallbacks

**Files:**
- Create: `crates/engine/tests/it/coreldraw_p3_persistence.rs`
- Modify: `crates/engine/tests/it/main.rs`
- Modify only on measured gaps: `crates/corel-import/src/lib.rs`, `crates/cdr/src/`

- [ ] **Step 1: Write failing persistence and warning tests**

Build a document with nested PowerClip, live effects, mesh/transparency, bitmap, and trace output. Reopen native and edit every source. Export CDR7/CMX1 and require supported structures to reopen; unsupported live semantics must return a named warning and retain a native residual where the format permits. Never claim CDR dynamic effects from a flattened self-round trip.

- [ ] **Step 2: Implement only cited format mappings**

Use the public CDR specification and audited adoption record. Unknown chunks remain bounded residuals; malformed input returns structured errors without panic. S2 stays open without libcdr tools and real samples.

- [ ] **Step 3: Verify and commit**

Run the persistence test, `corel-import`, `coralcraft-cdr`, and hostile-input suites. Record exact fallback warnings in `docs/p3-inventory.md`.

### Task 11: Verify shared MCP/CLI automation and regenerate docs

**Files:**
- Modify: `crates/engine/tests/it/coreldraw_p3_persistence.rs`
- Modify: `crates/mcp/tests/` only if a new command requires registry coverage
- Regenerate: `docs/mcp.md`

- [ ] **Step 1: Run every new P3 command through `run_command`**

Verify success results, structured invalid-input failures, and no mutation on rejection. Existing commands need no duplicate MCP tool. A representative call is:

```rust
let result = s.run_command("object.clippingMask.make", json!({}));
assert!(result.is_ok());
```

- [ ] **Step 2: Regenerate and verify the live registry**

Run `CORALCRAFT_BLESS=1 cargo test -p vectorcraft-mcp --test docs_sync`, then run the same test without blessing and require a clean diff. Commit with `feat: expose P3 workflows through shared automation` or `test:` if no surface changed.

### Task 12: Run poster/traced-image acceptance and the P3 gate

**Files:**
- Create: `crates/engine/tests/it/coreldraw_p3_acceptance.rs`
- Modify: `crates/engine/tests/it/main.rs`, `docs/p3-inventory.md`, parity/roadmap/memory files

- [ ] **Step 1: Write the failing end-to-end scenarios**

Create a poster with nested PowerClip, advanced fill/transparency, Blend, Envelope, shadow, and live text; create a traced-image composition with editable expanded paths and a bitmap adjustment. Save/reopen, revise effect parameters and source content, export SVG/PDF, and read them independently with `usvg` and Hayro. Assert the native live models still edit after export.

- [ ] **Step 2: Run the full gate and all skipped gates**

Run `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo xtask ci`. If only known-issue 0b fails, separately run doctests, cleanroom, no-bundled-fonts, pdf-deps, assets, brands, cursors, i18n-report, layers, and wasm; record exact counts and failures.

- [ ] **Step 3: Update evidence and commit**

Check only P3 rows backed by the scenarios. Leave S2 and every unimplemented D4 behaviour open with exact evidence. Commit with `feat: complete the P3 effects and image workflows` only if the P3 exit criterion is actually met; otherwise commit the tested acceptance slice and leave the queue open.
