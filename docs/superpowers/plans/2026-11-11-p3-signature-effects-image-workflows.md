# P3 Signature Effects and Image Workflows Implementation Plan

> **For agentic workers:** execute one task at a time, test-first. This file keeps only the open tasks and their acceptance conditions. Delivered history, RED/GREEN evidence, and focused-gate results live in [`docs/p3-inventory.md`](../../p3-inventory.md) — do not append logs here or to `docs/gaps.md`.

**Goal:** Deliver the D4 signature workflows — PowerClip, interactive effects with handles, Corel fills (pattern parameters, texture, merge modes), PowerTRACE — on the VectorCraft base, with native editability, independent SVG/PDF evidence, and explicit CDR/CMX degradations. A catalog effect id alone is never completion.

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
- `crates/engine/tests/it/coreldraw_texture.rs`: the texture-fill reusable-model slice (Task T1).
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

### Delivered (terse; evidence in `docs/p3-inventory.md`)

Tasks 1, 2, 4, 5, 7, 8, 9, 11 complete; Task 3 Step 2 (transparency/masks) complete; Task 10's
mesh-opacity/empty-live slices and the mesh expansion-provenance closure (`d18ce9f3b`) complete;
Task 12's historical acceptance recorded. The open work is the task list below, in order.

---

## Open tasks

### Task T1: Texture fill — the reusable model (P3.3, slice 1 of 3) — CURRENT

Mirror the pattern architecture (`Paint::Pattern` + document `patterns`) with a texture model that
is **not** an ordinary pattern relabelled: a document-level reusable `TextureDef` (name, procedural
`kind` with parameters, two colours), a distinct `Paint::Texture` referencing it, deterministic
bounded synthesis for render/export, and shared-engine commands.

**Positive/negative matrix:** [`docs/p3-inventory.md` §19](../../p3-inventory.md) freezes this
slice's semantics; write the matrix there before implementation and keep it in sync.

**Acceptance (all required; nothing substitutes):**
- Real editing through commands: create/edit a definition, apply to objects, edit propagates to
  every user; undo/redo of each step.
- Native reopen keeps the definition and its users editable (edit-after-reopen proven).
- Independent evidence: `usvg` reads the synthesized tiles' actual geometry/colours in SVG; the
  Hayro-backed PDF reader reconstructs the tile art (structure, not live-PDF claims).
- CDR/CMX export carries an explicit texture fallback warning (never a silent checkerboard).
- Negatives: applying an unknown texture errors; the texture is not listed by pattern queries and
  a pattern is not accepted where a texture is required.

### Task T2: Corel Pattern parameters (P3.3, slice 2 of 3)

Map the CorelDRAW pattern model's foreground/background/kind parameters onto the synced pattern
architecture, or degrade with a precise warning naming exactly what is lost. Generic `PatternDef.art`
workflows are already proven and do not count for this task.

**Acceptance:** a parameter-level mapping with tests for each parameter (edit, undo/redo, native
reopen), or per-parameter fallback warnings; independent SVG/PDF evidence; CDR/CMX behaviour stated
explicitly (today's default checkerboard substitution becomes a named, warned degradation or a real
mapping).

### Task T3: Merge-mode matrix and combined transparency (P3.3, slice 3 of 3)

**Acceptance:** the merge-mode matrix (each supported mode: edit, undo/redo, native reopen,
independent SVG/PDF read); combined scenarios of mesh + gradient + opacity mask with their CDR/CMX
channel warnings deduplicated; per-mode or per-combination degradation warnings where CDR/CMX
cannot retain the semantics.

### Tasks E1–E6: remaining D4 effects and interactions (old Task 6, split)

Each is its own task with the same acceptance shape; none substitutes for another:
**E1 Symmetry · E2 Block Shadow · E3 Perspective · E4 Lens · E5 Push/Pull and Zipper dedicated
handles · E6 PowerClip lock-contents-to-frame UI.**

**Acceptance per task:** existing-capability inventory first; a failing scenario written before the
fix; native edit and reopen; the real rendered control (synthetic input/AccessKit, not command-only
invocation); independent SVG/PDF reading; an explicit CDR/CMX degradation. A catalog effect id
existing is not completion.

### Task M: P3.10 combined matrix (old Task 10 closure)

Replace the generic "combined matrix" wording with explicit fixtures: nested PowerClip; translucent
mesh inside Repeat/Blend; gradient + opacity mask; bitmap + trace; metadata residual and
restoration-failure paths.

**Acceptance:** native reopen editable; direct CDR model checks; CDR/CMX ordinary-vector
supplementary evidence; warning deduplication; non-empty output; abnormal-input and
generation-budget errors. Self-round-trips and self-skipping oracles are not independent
certification; S2 stays open.

### Task 12 (historical, retained)

The recorded poster/traced-image acceptance stands as evidence. It does **not** replace Task 13.

### Task 13: final P3 gate — OPEN

- [ ] Rerun `run_command` MCP coverage and `docs_sync` for every command added during P3.
- [ ] Rerun the poster/traced-image acceptance scenarios on the final tree.
- [ ] Full `cargo xtask ci` plus every serial gate; results recorded verbatim (known-issue 0b
      documented, not silenced) in `docs/p3-inventory.md`.
- [ ] Update parity rows only where new evidence exists.
- [ ] P3 closes only when every open item above is resolved; unresolved items become named
      follow-ups, never silent closures.

---

## Maintenance rules

- This file holds only open tasks and acceptance conditions; RED/GREEN detail and gates go to
  `docs/p3-inventory.md`.
- Before each slice, write the complete positive/negative matrix (in `p3-inventory.md`) to freeze
  the slice's semantics. Newly found regressions are never ignored; unrelated older issues are
  filed as follow-ups instead of blocking the slice.
- The same reviewer re-reviews each slice (spec, then quality); a full gate runs once per slice
  after both reviews pass — focused tests only during development.
- No planning agents for already-clear fixes; a design review happens only at a genuine
  architectural fork.
