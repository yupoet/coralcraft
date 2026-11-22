# P4 Colour, Prepress, and Production Output Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver independently checked CMYK/spot, proof, PDF/X, print, imposition, and data-driven production workflows on the existing VectorCraft pipeline, with native editability and explicit degradation warnings.

**Architecture:** Reuse `vectorcraft-color`, `vectorcraft-render::proof`, `vectorcraft-pdf`'s PDF/X/print engine, existing print/variables/export commands, and the desktop print/separations UI. Add only missing end-to-end behavior proven by failing tests; do not create a second colour engine, PDF writer, data-merge model, or imposition stack.

**Tech Stack:** Rust, `vectorcraft-color`, `vectorcraft-render`, `vectorcraft-pdf`, `vectorcraft-engine`, `vectorcraft-ui-egui`, Hayro-backed PDF import, `usvg`, existing CLI/MCP, and `cargo xtask`.

---

## Entry gate and evidence boundaries

- **Prerequisite:** P3 Task 13 must be complete, including the M1/M2 evidence hardening in `docs/superpowers/plans/2026-11-19-p3-final-acceptance.md`. Before that, only Task 1's read-only inventory may run; no P4 production milestone should be committed.
- Existing infrastructure is substantial: PDF/X-1a/X-3/X-4 presets and output intents, ICC conversion, proof colours, separations, overprint preview, print preview/settings, print tiling, variables/data sets, batch export, and print/PDF presets already exist. The phase is primarily an integration and independent-evidence phase.
- Current independent CDR certification remains open as S2/P0.1. Self-skipping tests and self-round trips are not independent certification.
- No local veraPDF/pdfcpu/qpdf/MuPDF/Ghostscript was found during planning. **veraPDF targets PDF/A, not PDF/X, and must not be used as a PDF/X conformance claim.** P4 evidence uses Hayro-backed independent PDF reads plus explicit PDF/X structural/semantic assertions. Any additional third-party preflight tool requires a separate operator decision and must identify its actual PDF/X coverage.
- HPGL, cutter nesting, and a full PHOTO-PAINT-class bitmap editor are out of scope unless the operator supplies a concrete verified use case and public behaviour reference.
- Every production change starts with an observed failing test and GitNexus impact analysis. Every task runs spec then quality review and `detect_changes` before committing; partial/truncated graph results are not clean.

## File map

- Inventory/evidence: `docs/p4-inventory.md`, `docs/gaps.md`, `docs/target-app-parity.md`, `ROADMAP.md`, `docs/ai-memory/03-current.md`.
- PDF/X: `crates/pdf/src/pdfx.rs`, `crates/pdf/src/output.rs`, `crates/pdf/src/presets.rs`, `crates/engine/src/cmd/fileio/tests_pdfx.rs`.
- Proof/separations: `crates/render/src/proof.rs`, `crates/ui-egui/src/panels/separations.rs`, `crates/engine/src/cmd/overprint.rs`, `crates/engine/src/cmd/colormgmt.rs`.
- Print/marks/tiling: `crates/pdf/src/print.rs`, `crates/engine/src/cmd/print.rs`, `crates/engine/src/cmd/printadvanced.rs`, `crates/engine/src/cmd/printtiling.rs`, `crates/ui-egui/src/dialogs/print.rs`, `crates/ui-egui/src/print.rs`.
- Data/presets/batch: `crates/doc/src/variables.rs`, `crates/engine/src/cmd/variablecmds.rs`, `crates/engine/tests/variables.rs`, `crates/engine/src/cmd/fileio/screens.rs`, `crates/engine/src/cmd/fileio/tests_batch.rs`, `crates/engine/src/cmd/printpresets.rs`.
- New integration evidence: `crates/engine/tests/it/coreldraw_p4_production.rs` and `crates/engine/tests/it/coreldraw_p4_acceptance.rs`, registered in `crates/engine/tests/it/main.rs`.
- Automation: `crates/mcp/src/tests.rs`, `crates/mcp/tests/docs_sync.rs`, `docs/mcp.md` only if the registry changes.

### Task 1: Establish the P4 baseline and positive/negative matrix

**Files:**
- Create: `docs/p4-inventory.md`
- Review only: all files in the file map

- [ ] **Step 1: Verify the P3 prerequisite and current branch** (2026-11-22: verified — **blocked**; 11 unchecked steps remain in the P3 final-acceptance plan and P3 is still "In progress" in `ROADMAP.md`, so the blocker is recorded in `docs/p4-inventory.md` §1 and work stops after the inventory)

Run:

```bash
git status --short --branch
git log -8 --oneline
test -f docs/superpowers/plans/2026-11-19-p3-final-acceptance.md
```

Expected: the P3 final-acceptance work is committed and P3 is marked complete in `ROADMAP.md`. If not, record the blocker and stop after the inventory.

- [ ] **Step 2: Measure the reusable base instead of estimating percentages** (2026-11-22: all six commands run, exit 0, counts in `docs/p4-inventory.md` §2)

Run:

```bash
cargo test -p vectorcraft-engine pdfx
cargo test -p vectorcraft-engine variables
cargo test -p vectorcraft-engine tests_batch
cargo test -p vectorcraft-ui-egui separations
cargo test -p vectorcraft-ui-egui tests_print
cargo xtask stats
```

Record actual counts, existing commands, independent readers, missing end-to-end links, and whether each row starts at evidence level 1–4. Do not add a feature from a legacy crate merely because it exists there; current VectorCraft print tiling is the default imposition base.

- [ ] **Step 3: Freeze the P4 matrix** (2026-11-22: 16-row matrix frozen in `docs/p4-inventory.md` §4, file-map corrections §5, evidence boundaries §6)

Create `docs/p4-inventory.md` with one row per item: ICC/proof, spot plates, overprint, PDF/X-1a, X-3, X-4, media/trim/bleed boxes, fonts, marks, print preview, imposition, data merge, batch export, presets, MCP/CLI, and final production scenario. Each row names positive assertions, refusal/atomicity cases, native persistence, independent evidence, and CDR/CMX warning expectations. Commit only after the matrix is complete with `docs: inventory the P4 production capabilities`.

### Task 2: Prove ICC, proof colours, spot plates, and overprint end to end

**Files:**
- Test: `crates/engine/tests/it/coreldraw_p4_production.rs`
- Modify only on RED: `crates/render/src/proof.rs`, `crates/engine/src/cmd/overprint.rs`, `crates/engine/src/cmd/colormgmt.rs`
- UI test: `crates/ui-egui/src/panels/separations.rs`

- [ ] **Step 1: Add failing-first production fixtures**

Create a document with process CMYK, one named spot swatch, one overprinting object, and one normal object. Cover each rendering intent and black-point compensation setting. Assert exact native settings after undo/redo and reopen, visible plate names, spot-vs-process identity, and overprint mode preservation. Negative cases: unknown profile/intent, invalid ink name, and non-finite colour values must leave document and history unchanged.

- [ ] **Step 2: Verify independent plate and pixel evidence**

Assert `print.preview` reports one sheet per expected ink with correct `ink`, `spot`, `print`, `frequency`, and `angle`. Read the resulting PDF through Hayro and check `/Separation`, spot names, overprint keys, and output intent where applicable. Check the separations panel through real UI interaction: toggle one plate, Alt-click for solo, change proof target, and confirm the rendered preview changes. Any visual claim needs pixel/ink evidence, not a screenshot alone.

Run:

```bash
cargo test -p vectorcraft-engine --test it coreldraw_p4_production -- --nocapture
cargo test -p vectorcraft-render proof
cargo test -p vectorcraft-ui-egui separations
cargo fmt --all -- --check
```

Expected: every new assertion passes. Commit with `test: verify P4 colour proof and separations`, or a separate `fix:` commit for a reproduced defect first.

### Task 3: Close the PDF/X-1a, X-3, and X-4 rule matrix

**Files:**
- Test: `crates/engine/src/cmd/fileio/tests_pdfx.rs` and `crates/engine/tests/it/coreldraw_p4_production.rs`
- Modify only on RED: `crates/pdf/src/pdfx.rs`, `crates/pdf/src/output.rs`, `crates/pdf/src/presets.rs`

- [ ] **Step 1: Assert every standard's required and forbidden constructs**

For each standard, export the same mixed CMYK/RGB/spot/transparency/font document and independently read it with Hayro. Assert:
- X-1a: PDF 1.3, `/GTS_PDFX` and version metadata, CMYK/spot only, no RGB/ICCBased device colour, flattened transparency, embedded font subsets, no encryption, and no live editing data.
- X-3: PDF 1.3 with flattened transparency and permitted colour-managed colours, output intent present, embedded fonts, no encryption.
- X-4: PDF 1.6 with live transparency and colour management, output intent present, embedded fonts, no encryption, and no preserve-editing payload unless the standard explicitly permits the selected option.
- All: exact media/trim/bleed boxes and no mutation of the native document.

Refuse invalid version/security/output-intent combinations atomically. Preserve unsupported live effects only as evaluated output with named warnings; native sources stay editable.

- [ ] **Step 2: Run independent structure and regression checks**

Run:

```bash
cargo test -p vectorcraft-engine pdfx -- --nocapture
cargo test -p vectorcraft-pdf
cargo test -p vectorcraft-engine --test it coreldraw_p4_production
```

Expected: all assertions pass. Record that Hayro is an independent structural reader, not a complete ISO PDF/X preflight suite. Commit with `test: verify the P4 PDFX rule matrix`.

### Task 4: Verify page boxes, marks, print preview, and imposition

**Files:**
- Test: `crates/engine/tests/it/coreldraw_p4_production.rs`
- Modify only on RED: `crates/pdf/src/print.rs`, `crates/engine/src/cmd/print.rs`, `crates/engine/src/cmd/printadvanced.rs`, `crates/engine/src/cmd/printtiling.rs`
- UI test: `crates/ui-egui/src/dialogs/tests_print.rs`

- [ ] **Step 1: Cover the print-layout matrix**

Use mixed-size artboards and document bleed. Assert exact media/trim/bleed boxes; trim/registration/color-bar/page-info marks; portrait/landscape/auto-rotate/transverse; scaling fit/custom/none; margins and imageable area; copy collation/reverse/skip-blank; composite and separations. Check `print.preview` page transforms before comparing `file.print` bytes.

- [ ] **Step 2: Use the existing print tiling for N-up/imposition evidence**

Exercise `tileFull` and `tileImageable` with rows/columns/overlap/tileRange. Assert sheet count, each source artboard's target rectangle, clipping at page bounds, and marks. Independently read the PDF and confirm every imposed page/tile is nonempty. Do not port the legacy `export_pdf_imposed` stack unless a failing case proves the current print tiler cannot express the workflow.

- [ ] **Step 3: Prove the actual desktop print path**

Drive the Print dialog through synthetic input: select a preset, change scaling/marks, inspect preview, and produce a print-ready PDF. Exercise the host print service rather than only invoking `file.print` directly. Run:

```bash
cargo test -p vectorcraft-engine --test it coreldraw_p4_production
cargo test -p vectorcraft-ui-egui tests_print
cargo test -p vectorcraft-ui-egui tests_print_advanced
```

Expected: all pass with exact page geometry. Commit with `test: verify P4 print and imposition workflows`.

### Task 5: Complete data-driven variants, batch export, and reusable presets

**Files:**
- Test: `crates/engine/tests/variables.rs`, `crates/engine/src/cmd/fileio/tests_batch.rs`, `crates/engine/tests/it/coreldraw_p4_production.rs`
- Modify only on RED: `crates/engine/src/cmd/variablecmds.rs`, `crates/engine/src/cmd/fileio/screens.rs`, `crates/engine/src/cmd/printpresets.rs`
- Automation test: `crates/mcp/src/tests.rs`

- [ ] **Step 1: Produce variants from existing variables and data sets**

Create a template with bound text, visibility, linked image, and graph variables plus at least three data rows. Apply each row through `dataset.select`, export a PDF per row, and independently verify its text/visibility/image/graph structure. Prove one undo step per row, template bindings after native reopen, overflow warnings, missing image errors, and invalid row atomicity. Use `command_batch` or `run_command` for the entire loop; do not add a second merge model.

- [ ] **Step 2: Verify batch export and preset reuse**

Use `document.exportForScreens` for all variants and output SVG, PNG, and PDF/X where supported. Assert unique sanitized paths, per-format folders, no overwrites, and nonempty independent output. Save/reapply print and PDF presets and prove native/preference persistence and actual UI application. Every new command must pass through real MCP `run_command`, including one invalid-input error with no mutation.

Run:

```bash
cargo test -p vectorcraft-engine variables
cargo test -p vectorcraft-engine tests_batch
cargo test -p vectorcraft-engine --test it coreldraw_p4_production
cargo test -p vectorcraft-mcp --test docs_sync
```

Expected: all pass. Commit with `test: verify P4 data-driven production workflows`.

### Task 6: Run the P4 acceptance scenario and final gate

**Files:**
- Create: `crates/engine/tests/it/coreldraw_p4_acceptance.rs`
- Modify: `docs/p4-inventory.md`, `docs/gaps.md`, `docs/target-app-parity.md`, `ROADMAP.md`, `docs/ai-memory/03-current.md`, `docs/superpowers/plans/2026-11-22-p4-colour-prepress-production.md`

- [ ] **Step 1: Execute a complete production job**

Build a two-page document containing CMYK/process art, a named spot ink, overprint, embedded/subset font text, a data-driven variant, and mixed-size pages. Export PDF/X-1a, X-3, X-4, a separations print PDF, and an imposed PDF. Reopen the native document and edit the source. Independently check page boxes, font resources, output intents, spot plates, overprint keys, marks, imposed geometry, and each variant. Assert exact CDR/CMX warnings for every degraded channel and never claim native semantics survived when only evaluated geometry remains.

Run:

```bash
cargo test -p vectorcraft-engine --test it coreldraw_p4_acceptance -- --nocapture
cargo test -p vectorcraft-engine --test it coreldraw_
cargo test -p vectorcraft-ui-egui separations
cargo test -p vectorcraft-ui-egui tests_print
cargo test -p vectorcraft-mcp --test docs_sync
```

Expected: all pass. Commit with `test: complete the P4 production acceptance`.

- [ ] **Step 2: Run full CI and all required serial gates**

```bash
cargo xtask ci
```

Record the actual result. Any failure other than known-issue 0b blocks P4. If 0b is the sole failure, run serially:

```bash
cargo test --workspace --doc
cargo xtask cleanroom
cargo test -p vectorcraft-text --no-default-features --test without_bundled_fonts
cargo tree -p vectorcraft-ui-egui --target all -e features,normal -i vectorcraft-text
cargo tree -p vectorcraft-ui-egui --no-default-features --target all -e features,normal -i vectorcraft-text
cargo tree -p vectorcraft-mcp --target all -e features,normal -i vectorcraft-text
cargo tree -p vectorcraft-mcp --no-default-features --target all -e features,normal -i vectorcraft-text
cargo clippy -p vectorcraft-ui-egui --no-default-features --all-targets -- -D warnings
cargo test -p vectorcraft-ui-egui --no-default-features --test install_fonts
cargo xtask pdf-deps
cargo xtask assets
cargo xtask brands
cargo xtask cursors
cargo xtask i18n-report
cargo xtask layers
cargo xtask wasm
```

Expected: all exit 0; font-tree assertions match the existing policy. Never call the 0b run a clean full-CI pass.

- [ ] **Step 3: Record completion and hand off to P5**

Update parity only where evidence exists, close P4 in the roadmap, and name P5 next. Keep S2/P0.1 open and record that formal third-party PDF/X conformance certification was not performed. Run:

```bash
cargo xtask stats
git diff --check
node .gitnexus/run.cjs detect-changes --scope all --repo .
git status --short --branch
```

Expected: complete graph analysis and intentional files only. Commit with `docs: complete the P4 production workflows`.

## Completion criteria

P4 is complete only when the colour/proof, PDF/X, print/imposition, data-driven, and final production matrices all pass with independent structural evidence, real UI and MCP/CLI paths, native editability, and the final gate recorded honestly. The next phase is P5; S2/P0.1 remain separate certification work.
