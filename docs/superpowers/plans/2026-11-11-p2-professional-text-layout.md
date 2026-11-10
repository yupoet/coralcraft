# P2 Professional Text and Multipage Layout Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver a CorelDRAW daily-use text and multipage layout workflow: editable artistic/paragraph text, linked frames, path text, wrapping, professional character/paragraph controls, dynamic page numbers, editable tables, master-page content, native/CDR persistence, MCP access, and an independently read brochure acceptance scenario.

**Architecture:** VectorCraft remains the only document, text, render, command, and UI base. Reuse `TextObject`/`TextRun`, `vectorcraft-text`, existing `text.*` commands, Character/Paragraph panels, and the CorelDRAW workspace. Add only CorelDRAW-specific interaction and missing semantics. CDR text decoding remains in safe Rust with bounded input, explicit warnings, and residual preservation; no second text engine or TraceDraw document model.

**Tech Stack:** `vectorcraft-doc`, `vectorcraft-text`, `vectorcraft-engine`, `vectorcraft-tools`, `vectorcraft-ui-egui`, `coralcraft-cdr`, `corel-import`, `vectorcraft-mcp`, `usvg`, Hayro-backed `vectorcraft-pdf`, and `encoding_rs` for explicitly identified legacy code pages.

---

## Mandatory rules

- Do not stage or revert `AGENTS.md`, `CLAUDE.md`, or `.claude/` unless the operator explicitly includes them. Keep every task's commit limited to its named files.
- Before editing an existing function, run GitNexus `impact` upstream and review direct callers. Before every commit run `node .gitnexus/run.cjs detect-changes --scope all --repo .`; `partial` or `truncated` is not a clean result.
- New behaviour starts with a failing test that is run and observed failing. No production code precedes that test.
- Use public CorelDRAW documentation or the audited TraceDraw behaviour record. Never run proprietary Corel software and never commit its files or assets.
- Level 4 requires native editability after reopen plus independent import/export evidence. `usvg` and the Hayro-backed PDF reader are independent parser evidence; a text assertion or self-reimport alone is not.
- S2 remains open until the real CDR oracle and sample prerequisites exist. New CDR tests must not claim to close it.
- `cargo xtask ci` stops at known-issue 0b on this x86_64 host. If that is the only failure, run each later skipped gate separately and record the exact results instead of claiming a clean gate.

## File map

- `docs/p2-inventory.md`: command/panel/format inventory and evidence anchors.
- `crates/ui-egui/src/panels/character.rs`, `paragraph.rs`: the existing shared Property Bar controls; reuse them rather than creating a second text bar.
- `crates/ui-egui/src/panels/tests_coreldraw_text.rs`: CorelDRAW workspace evidence for the shared text controls.
- `crates/engine/tests/it/coreldraw_text.rs`: artistic/paragraph text, typography, and thread behaviour.
- `crates/engine/tests/it/coreldraw_layout.rs`: path text, wrap, dynamic page numbers, master content, and editable tables.
- `crates/engine/tests/it/coreldraw_cdr_text.rs`: legacy text decoding, warning, residual, and writer evidence.
- `crates/engine/tests/it/coreldraw_p2_acceptance.rs`: multipage brochure acceptance.
- `crates/engine/tests/it/main.rs`: integration-test module registration.
- `crates/doc/src/text/runs.rs`: only if the page-number task's failing tests prove a dynamic field is required.
- `crates/engine/src/cmd/textedit.rs`, `threads.rs`, `create.rs`: only the missing commands demonstrated by inventory tests.
- `crates/cdr/src/text.rs` (new): bounded legacy text decoding helpers, independent of UI.
- `crates/cdr/src/parse.rs`, `crates/cdr/src/lib.rs`, `crates/cdr/Cargo.toml`, `Cargo.toml`: CDR decoder integration and `encoding_rs` dependency.
- `crates/corel-import/src/lib.rs`: CDR ↔ VectorCraft text mapping and warning propagation.
- `docs/target-app-parity.md`, `docs/gaps.md`, `ROADMAP.md`, `docs/ai-memory/03-current.md`, `docs/ai-memory/04-known-issues.md`: evidence and queue updates.

### Task 1: Build the P2 inventory and failing evidence anchors

**Files:**
- Create: `docs/p2-inventory.md`

- [ ] **Step 1: Capture the actual command and UI inventory**

Run:

```bash
cargo run -p coralcraft-cli -- commands > /tmp/coralcraft-p2-commands.txt
cargo xtask stats
```

Expected: the command catalog and current statistics are printed. Read `crates/engine/src/cmd/{create,typecmd,textedit,textstyles,threads,textwrap}.rs`, `crates/ui-egui/src/panels/{character,paragraph,opentype,glyphs,tabs,text_styles}.rs`, and the current text/layout parity rows.

- [ ] **Step 2: Write the inventory with exact starting points**

Create `docs/p2-inventory.md` with rows for:

| Slice | Existing base | Evidence needed |
|---|---|---|
| Font family/style/size | `panels::character::{font_pickers,size_field}` and `text.setStyle`/`text.setRangeStyle` | CorelDRAW Property Bar visibility, edits, undo, persistence |
| Paragraph alignment/lists | `panels::paragraph`, `text.setStyle`, `text.setFormat` | CorelDRAW controls and mixed-selection behaviour |
| Linked frames | `text.thread.create/add/break/remove` and `text.thread.info` | Reflow, undo/redo, UI port, native reopen |
| Path text/wrap/fit | `text.createInPath`, `text.areaOptions`, `text.reshapePath`, existing wrap engine | Editable geometry and independent output |
| Page numbers | No dynamic page-number field found in `TextRun` | Per-page evaluation, reorder, native persistence, CDR fallback |
| Editable tables | Existing paths, text, groups, and inline art | Cell edit, reflow, native reopen, export |
| CDR text | `decode_txtd_chunk`, `decode_loda_*text`, `corel-import` text mapping | Legacy code-page fixtures, warnings, residuals, writer evidence |
| Acceptance | P1 native/SVG/PDF helpers | Brochure with pages, linked text, page numbers, image, independent PDF read |

Each row must name the actual source and test paths. Commit the inventory after graph analysis:

```bash
git add docs/p2-inventory.md
git commit -m "docs: inventory the P2 text and layout capabilities"
```

### Task 2: Verify the CorelDRAW font and size Property Bar

**Files:**
- Create: `crates/ui-egui/src/panels/tests_coreldraw_text.rs`
- Modify: `crates/ui-egui/src/panels.rs`

- [ ] **Step 1: Verify the existing controls before changing production code**

The shared Control bar already calls `panels::character::control_bar`, which reuses `font_pickers` and `size_field` while text is selected or edited. Do not create `coreldraw_text_bar.rs` or duplicate font lookup, shaping, or command construction.

Create `tests_coreldraw_text.rs` with `text_bar_edits_the_selected_font_and_size`: select the CorelDRAW workspace, create and select text, draw the real Control bar, choose a font-size preset through pointer input, and verify the selected model. Exercise the font style through the widget's actual `text.setStyle {style}` command, then verify undo/redo and native reopen. Also verify that the text controls disappear with no text selected.

This is an evidence slice for existing behavior, so a passing test does not justify production code. If it exposes a real CorelDRAW-specific gap, add a focused failing regression test before fixing that gap.

- [ ] **Step 2: Run and commit**

```bash
cargo test -p vectorcraft-ui-egui coreldraw_text -- --nocapture
cargo test -p vectorcraft-ui-egui outline_pen
```

Expected: both pass. Commit only the test module, its registration, and this plan correction with `test: verify the CorelDRAW text property bar`.

### Task 3: Verify paragraph alignment and list controls

**Files:**
- Modify: `crates/ui-egui/src/panels/tests_coreldraw_text.rs`
- Create: `crates/engine/tests/it/coreldraw_text.rs`
- Modify: `crates/engine/tests/it/main.rs`

- [ ] **Step 1: Add paragraph behaviour evidence**

In `coreldraw_text.rs`, create an area text object and verify left/centre/right/justify plus bullet and numbered-list edits through the existing commands:

```rust
s.execute("text.setStyle", &json!({"id": id, "justify": "center"})).unwrap();
s.execute("text.setFormat", &json!({"ids": [id], "list": "bullet", "bullet": "square"})).unwrap();
s.execute("text.setFormat", &json!({"ids": [id], "list": "number", "number": "upperRoman"})).unwrap();
```

Assert the model changes, undo/redo restores each state, and native reopen preserves it. Run `cargo test -p vectorcraft-engine --test it coreldraw_text -- --nocapture`. Existing behaviour that passes needs only evidence; a missing behaviour requires a failing regression test before implementation.

- [ ] **Step 2: Verify the shared CorelDRAW controls**

The shared Control bar already exposes left/centre/right alignment and a Paragraph popover; the Paragraph panel supplies justification, bullets, and numbering through `align` and `run_on_paragraphs`. Cover the CorelDRAW workspace's real controls with pointer/accessibility evidence. Do not add a second paragraph row or formatter; change production code only for a measured CorelDRAW-specific gap.

- [ ] **Step 3: Verify and commit**

```bash
cargo test -p vectorcraft-engine --test it coreldraw_text -- --nocapture
cargo test -p vectorcraft-ui-egui coreldraw_text -- --nocapture
```

Expected: pass. Commit with `test: verify CorelDRAW paragraph controls` when evidence-only, or `feat: expose CorelDRAW paragraph controls` if a measured gap requires wiring.

### Task 4: Prove linked-frame editing and UI threading

**Files:**
- Modify: `crates/engine/tests/it/coreldraw_text.rs`
- Modify: `crates/ui-egui/src/panels/tests_coreldraw_text.rs`

- [ ] **Step 1: Write the failing linked-frame test**

Create two area frames with enough text to overflow, then link and edit them:

```rust
s.execute("text.thread.create", &json!({})).unwrap();
let threads = s.execute("text.thread.info", &json!({})).unwrap();
assert_eq!(threads.as_array().map(Vec::len), Some(1));
s.execute("text.editRange", &json!({"id": first, "start": 0, "end": 0, "insert": "Revised introduction. "})).unwrap();
```

Verify thread order, text reflow, undo/redo, `text.thread.break` behaviour, and native reopen. Existing behaviour that passes needs evidence, not a duplicate engine feature.

- [ ] **Step 2: Verify the existing CorelDRAW UI path**

Drive the shared Type › Threaded Text commands in the CorelDRAW workspace and run the existing Selection-tool port pointer tests, including double-click to break a link. Add no second context menu or port renderer. Change production code only if these tests expose a CorelDRAW-specific gap.

- [ ] **Step 3: Verify and commit**

```bash
cargo test -p vectorcraft-engine --test it coreldraw_text -- --nocapture
cargo test -p vectorcraft-ui-egui coreldraw_text -- --nocapture
cargo test -p vectorcraft-ui-egui a_double_click_on_a_threaded_out_port_breaks_the_thread
```

Commit with `test: verify CorelDRAW linked-text workflows` when evidence-only, or the appropriate `feat:` if a measured gap requires wiring.

### Task 5: Prove path text, wrapping, and frame fitting

**Files:**
- Create: `crates/engine/tests/it/coreldraw_layout.rs`
- Modify: `crates/engine/tests/it/main.rs`

- [ ] **Step 1: Write the failing layout test**

Build a document containing an open path, path text, a wrap object, and an area frame:

```rust
let path = s.execute("path.create", &json!({"d": "M20 100 C100 20 200 20 280 100"})).unwrap()["id"].clone();
let text = s.execute("text.createInPath", &json!({"path": path, "mode": "onPath", "text": "Editable path text"})).unwrap()["id"].clone();
s.execute("text.reshapePath", &json!({"id": text, "anchors": [[0, 0]], "dx": 0, "dy": 20})).unwrap();
```

Assert the path/text remain editable after native reopen, the wrap engine responds to object changes, and area fit/overflow state changes through `text.areaOptions`. Parse exported SVG with independent `usvg` and open exported PDF through `vectorcraft_pdf::import`.

- [ ] **Step 2: Implement only failures reproduced by the test**

Use existing `text.createInPath`, `text.reshapePath`, `text.areaOptions`, and wrap commands. Do not add replacement layout algorithms. Record any missing CorelDRAW command mapping in `docs/gaps.md` with the exact failing test.

- [ ] **Step 3: Verify and commit**

```bash
cargo test -p vectorcraft-engine --test it coreldraw_layout -- --nocapture
```

Expected: pass, including independent `usvg` and Hayro checks. Commit with `feat: complete CorelDRAW text layout workflows` if production wiring changed, or `test: verify CorelDRAW text layout workflows` if the task is evidence-only.

### Task 6: Cover typography, styles, glyphs, and missing-font behaviour

**Files:**
- Modify: `crates/engine/tests/it/coreldraw_text.rs`
- Modify: `crates/ui-egui/src/panels/tests_coreldraw_text.rs`

- [ ] **Step 1: Add failing evidence for the selected typography path**

Exercise character and paragraph styles, OpenType features, glyph alternates, variable-font axes where available, and missing-font reporting through the existing commands. Assert style propagation, local overrides, undo/redo, and native reopen. Keep tests independent of host-installed fonts by using the existing test-font fixtures; never add a font file to the repository.

- [ ] **Step 2: Verify the relevant existing panels in the CorelDRAW workspace**

Open Character, Paragraph, OpenType, Glyphs, Character Styles, and Paragraph Styles through the existing `window.panel` commands, accepting both docked and floating presentation. Add no new font engine, glyph model, or panel system. If all behaviour already passes, keep this as a test and documentation slice.

- [ ] **Step 3: Verify and commit**

```bash
cargo test -p vectorcraft-engine --test it coreldraw_text -- --nocapture
cargo test -p vectorcraft-ui-egui coreldraw_text -- --nocapture
```

Commit with `test: verify CorelDRAW typography workflows` or the appropriate `feat:` if wiring changed.

### Task 7: Add dynamic page-number fields

**Files:**
- Modify: `crates/doc/src/node/{extras,wire}.rs`, `node.rs`, and `coralcraft.rs`
- Modify: `crates/engine/src/cmd/{create,newart}.rs` and `cmd/fileio/{export,encode}.rs`
- Modify: `crates/render/src/{lib,renderer}.rs` and `renderer/{nodes,text}.rs`
- Modify: `crates/svg/src/export/nodes.rs` and `crates/pdf/src/export/{entry,exporter,text,writer}.rs`
- Modify: `crates/corel-import/src/lib.rs` and `crates/engine/tests/it/coreldraw_layout.rs`

- [ ] **Step 1: Write the failing dynamic-field test**

Create three pages, insert a dedicated page-number text object into a master layer, reorder pages, and require evaluated text to follow each current page. Native reopen must keep the field dynamic. The initial test fails because `text.pageNumber.insert` does not exist.

- [ ] **Step 2: Use the smallest document representation**

Store an object-level `page_number` flag in the existing sparse `NodeExtras`, with a serde default in `NodeWire`; do not add a field to every `TextRun` literal. `Node::page_number_text` creates a temporary evaluated text copy while the stored placeholder remains `#`. `text.pageNumber.insert {x, y, …}` creates this object in one history step. This slice covers dedicated page-number objects, not arbitrary inline fields mixed into a longer run.

Evaluate at the canvas, SVG, and PDF boundaries. `Document::corel_layers_for_page` supplies the ordered, translated master/normal/desktop layer stack for single-page export. Ordinary text and all non-field rendering paths remain unchanged.

- [ ] **Step 3: Map the CDR fallback explicitly**

CDR/CMX export splits page-number objects into applicable pages as ordinary editable text, leaving other master content on the master layer. The command warns that the dynamic field and master semantics are retained only in the native document. Do not claim native CDR dynamic-field support without a cited format layout and test.

- [ ] **Step 4: Verify and commit**

```bash
cargo test -p vectorcraft-doc text
cargo test -p vectorcraft-engine --test it coreldraw_layout -- --nocapture
cargo test -p corel-import
cargo test -p vectorcraft-render layeropts
```

Expected: creation, reorder, native reopen, canvas/SVG/PDF evaluation, CDR fallback, and warnings pass. Commit with `feat: add dynamic page-number fields`.

### Task 8: Add an editable table layout command

**Files:**
- Modify: `crates/engine/src/cmd/create.rs`
- Modify: `crates/engine/tests/it/coreldraw_layout.rs`

- [ ] **Step 1: Write the failing table test**

Require a command `layout.table` taking `{x, y, width, height, rows, columns, text?: [[…]]}` and producing one group of editable cell rectangles and text objects. The test edits a cell after creation, checks row/column counts, undo/redo, native reopen, and independent SVG/PDF parsing. Reject zero, negative, non-finite, or excessive rows/columns before editing the document.

- [ ] **Step 2: Implement the command from existing objects**

Build the group using existing `Node`/text/path objects and one `s.edit` transaction. Use checked arithmetic, finite bounds, and a fixed input budget. Do not introduce a new table renderer. Cell text remains live `TextObject`; cell borders remain editable paths.

- [ ] **Step 3: Verify and commit**

```bash
cargo test -p vectorcraft-engine --test it coreldraw_layout -- --nocapture
cargo test -p vectorcraft-engine --test it coreldraw_p1_acceptance -- --exact
```

Expected: both pass. Commit with `feat: add editable CorelDRAW table layouts`.

### Task 9: Add bounded legacy CDR text decoding and warnings

**Files:**
- Create: `crates/cdr/src/text.rs`
- Modify: `crates/cdr/src/lib.rs`
- Modify: `crates/cdr/src/parse.rs`
- Modify: `crates/cdr/Cargo.toml`
- Modify: `Cargo.toml`
- Modify: `crates/corel-import/src/lib.rs`
- Create: `crates/engine/tests/it/coreldraw_cdr_text.rs`
- Modify: `crates/engine/tests/it/main.rs`

- [ ] **Step 1: Write synthetic failing code-page tests**

Create handcrafted byte fixtures for Windows-1252 and Windows-1250, including a non-ASCII character and a NUL terminator. Require exact Unicode output, malformed/truncated input to return a structured error, and no panic. Do not use real or proprietary CDR fixtures.

- [ ] **Step 2: Implement the bounded decoder**

Add `encoding_rs = "0.8"` to workspace dependencies and `encoding_rs.workspace = true` to `coralcraft-cdr`. Implement a decoder that takes bytes plus an explicitly identified `encoding_rs::Encoding`; do not guess a code page from text. Integrate it only where the CDR parser has an evidenced encoding field. Unknown encodings produce a warning and residual, not silent UTF-8 replacement.

- [ ] **Step 3: Verify mapping and writer evidence**

Through `corel-import`, verify text remains editable, unknown encoding metadata is not discarded, and the CDR writer either preserves the original encoding or reports the conversion. Self-round-trip is supplementary; S2 remains open.

- [ ] **Step 4: Verify and commit**

```bash
cargo test -p coralcraft-cdr
cargo test -p corel-import
cargo test -p vectorcraft-engine --test it coreldraw_cdr_text -- --nocapture
```

Expected: pass with no unbounded allocations and no silent unknown-code-page fallback. Commit with `feat: decode evidenced legacy CDR text encodings`.

### Task 10: Prove native and CDR persistence of the P2 semantics

**Files:**
- Modify: `crates/engine/tests/it/coreldraw_layout.rs`
- Modify: `crates/engine/tests/it/coreldraw_text.rs`
- Modify: `crates/engine/tests/it/coreldraw_cdr_text.rs`

- [ ] **Step 1: Add one consolidated persistence test**

Build one document with linked text, path text, paragraph styles, a dynamic page-number field, a table, and master content. Save/reopen `.vectorcraft`, verify each remains editable, then perform CDR export/reopen and verify supported semantics plus explicit warnings for unsupported dynamic data.

- [ ] **Step 2: Fix only failures demonstrated by the consolidated test**

Use the existing CoralCraft extension block and `corel-import` residual mapping. No self-round-trip-only milestone and no flattening presented as editability.

- [ ] **Step 3: Verify and commit**

```bash
cargo test -p vectorcraft-engine --test it coreldraw_ -- --nocapture
cargo test -p corel-import
```

Commit with `test: verify P2 persistence and CDR fallbacks` if evidence-only, otherwise `fix: preserve P2 text and layout semantics`.

### Task 11: Cover MCP and CLI access without changing the registry shape

**Files:**
- Modify: `crates/mcp/src/tests.rs`
- Modify: `docs/mcp.md` only through the documented generator

- [ ] **Step 1: Write the failing MCP test**

Drive `text.pageNumber.insert` and `layout.table` through `run_command`, verifying real document changes and error results for invalid input. Existing generic `run_command` coverage should make new commands available without a new MCP tool.

- [ ] **Step 2: Run protocol and docs checks**

```bash
cargo test -p vectorcraft-mcp
CORALCRAFT_BLESS=1 cargo test -p vectorcraft-mcp --test docs_sync
cargo test -p vectorcraft-mcp --test docs_sync
```

Expected: pass; generated docs reflect the live registry without hard-coded counts.

- [ ] **Step 3: Commit**

```bash
git add crates/mcp/src/tests.rs docs/mcp.md
git commit -m "test: expose P2 workflows through MCP and CLI"
```

### Task 12: Run the brochure acceptance and P2 gate

**Files:**
- Create: `crates/engine/tests/it/coreldraw_p2_acceptance.rs`
- Modify: `crates/engine/tests/it/main.rs`
- Modify: `docs/p2-inventory.md`
- Modify: `docs/target-app-parity.md`
- Modify: `docs/gaps.md`
- Modify: `ROADMAP.md`
- Modify: `docs/ai-memory/03-current.md`
- Modify: `docs/ai-memory/04-known-issues.md`

- [ ] **Step 1: Write and run the failing brochure scenario**

Build a three-page brochure with styled title/body text, linked frames, path text, a wrap object, dynamic page numbers on master content, an editable table, and an embedded synthetic bitmap. Revise text after native reopen, reorder pages, and export PDF. Open the PDF with `vectorcraft_pdf::import`, verify three pages and editable content, and parse SVG with independent `usvg`.

- [ ] **Step 2: Run the focused suite and full gate**

```bash
cargo test -p vectorcraft-engine --test it coreldraw_ -- --nocapture
cargo test -p vectorcraft-ui-egui coreldraw_
cargo test -p coralcraft-cdr
cargo test -p corel-import
cargo test -p vectorcraft-mcp
cargo fmt --all -- --check
cargo xtask ci
```

Expected: all focused suites pass. If full CI stops only at known-issue 0b, run each skipped later gate separately as in the P1 review and record the exact results. Do not claim a clean gate with a failure present.

- [ ] **Step 3: Update evidence and the queue**

Update text/layout parity rows only to the levels supported by the cited evidence. Mark P2 complete only if the brochure and persistence criteria pass. Keep CDR S2 open and list every remaining unsupported CDR text semantic explicitly. Set the next task to the first open P3 item.

- [ ] **Step 4: Run graph analysis and commit**

```bash
node .gitnexus/run.cjs detect-changes --scope all --repo .
git add docs/p2-inventory.md docs/target-app-parity.md docs/gaps.md ROADMAP.md docs/ai-memory/03-current.md docs/ai-memory/04-known-issues.md crates/engine/tests/it/coreldraw_p2_acceptance.rs crates/engine/tests/it/main.rs
git commit -m "docs: record the P2 acceptance evidence"
```

Expected: no unrelated files, no hidden test skips, and the actual next queue item is unambiguous.
