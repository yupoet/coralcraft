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
- `crates/ui-egui/src/coreldraw_text_bar.rs`: CorelDRAW text Property Bar, separate from the general `coreldraw_bar.rs`.
- `crates/ui-egui/src/chrome/control_bar.rs`, `crates/ui-egui/src/lib.rs`: text-bar registration only.
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

### Task 2: Add the CorelDRAW font and size Property Bar

**Files:**
- Create: `crates/ui-egui/src/coreldraw_text_bar.rs`
- Modify: `crates/ui-egui/src/chrome/control_bar.rs`
- Modify: `crates/ui-egui/src/lib.rs`

- [ ] **Step 1: Write the failing UI test**

Create `coreldraw_text_bar.rs` with a `#[cfg(test)]` test named `text_bar_edits_the_selected_font_and_size` that creates a `VectorcraftApp`, selects the CorelDRAW workspace, creates text through `text.create`, selects it, draws the new bar, and then runs the same `text.setStyle` calls used by the widgets:

```rust
app.run("text.setStyle", json!({"id": id, "size": 36.0, "fontStyle": "Bold"})).unwrap();
let node = app.run("document.node", json!({"id": id, "summary": true})).unwrap();
assert_eq!(node["fontSize"].as_f64(), Some(36.0));
```

Run `cargo test -p vectorcraft-ui-egui coreldraw_text_bar -- --nocapture` before the bar exists and confirm the test fails because the module/controls are absent.

- [ ] **Step 2: Implement the thin bar over existing Character controls**

Implement `pub fn show(app: &mut VectorcraftApp, ui: &mut Ui) -> bool` so it returns false outside the CorelDRAW workspace and when the mapped Text tool/selected text is absent. When active, call the existing `panels::character::{font_pickers,size_field}` with unique ids `corel-cb-font`, `corel-cb-font-style`, and `corel-cb-font-size`. Do not duplicate font lookup, shaping, or command construction.

Register `coreldraw_text_bar` in `lib.rs` and call `show` from the Control bar after the P1 tool-specific rows. Keep all existing upstream bars unchanged outside the CorelDRAW workspace.

- [ ] **Step 3: Run and commit**

```bash
cargo test -p vectorcraft-ui-egui coreldraw_text_bar -- --nocapture
cargo test -p vectorcraft-ui-egui outline_pen
```

Expected: both pass. Commit only the three UI files with `feat: add the CorelDRAW text property bar`.

### Task 3: Add paragraph alignment and list controls

**Files:**
- Modify: `crates/ui-egui/src/coreldraw_text_bar.rs`
- Create: `crates/engine/tests/it/coreldraw_text.rs`
- Modify: `crates/engine/tests/it/main.rs`

- [ ] **Step 1: Write failing paragraph behaviour tests**

In `coreldraw_text.rs`, create an area text object and verify left/centre/right/justify plus bullet and numbered-list edits through the existing commands:

```rust
s.execute("text.setStyle", &json!({"id": id, "justify": "center"})).unwrap();
s.execute("text.setFormat", &json!({"ids": [id], "list": "bullet", "bullet": "square"})).unwrap();
s.execute("text.setFormat", &json!({"ids": [id], "list": "number", "number": "upperRoman"})).unwrap();
```

Assert the model changes, undo/redo restores each state, and native reopen preserves it. Run `cargo test -p vectorcraft-engine --test it coreldraw_text -- --nocapture`; confirm failure before any missing wiring is added.

- [ ] **Step 2: Wire the CorelDRAW controls to existing Paragraph behaviour**

Use `panels::paragraph::align` and `run_on_paragraphs` for alignment/list actions. Add left, centre, right, justify, bullet, and numbered-list controls to `coreldraw_text_bar.rs`, with unique AccessKit labels. Do not implement a second paragraph formatter.

- [ ] **Step 3: Verify and commit**

```bash
cargo test -p vectorcraft-engine --test it coreldraw_text -- --nocapture
cargo test -p vectorcraft-ui-egui coreldraw_text_bar -- --nocapture
```

Expected: pass. Commit with `feat: expose CorelDRAW paragraph controls`.

### Task 4: Prove linked-frame editing and UI threading

**Files:**
- Modify: `crates/engine/tests/it/coreldraw_text.rs`
- Modify: `crates/ui-egui/src/coreldraw_text_bar.rs`

- [ ] **Step 1: Write the failing linked-frame test**

Create two area frames with enough text to overflow, then link and edit them:

```rust
s.execute("text.thread.create", &json!({})).unwrap();
let threads = s.execute("text.thread.info", &json!({})).unwrap();
assert_eq!(threads.as_array().map(Vec::len), Some(1));
s.execute("text.editRange", &json!({"id": first, "start": 0, "end": 0, "insert": "Revised introduction. "})).unwrap();
```

Verify thread order, text reflow, undo/redo, `text.thread.break` behaviour, and native reopen. If the daily-use command path already passes, the initial failure must come from the missing CorelDRAW UI linkage, not from adding a duplicate engine feature.

- [ ] **Step 2: Add only the measured UI gap**

Expose thread creation/add/break through the CorelDRAW text context controls using the existing command ids. Use the existing Selection-tool ports for on-canvas threading; do not add another port renderer. If inventory confirms the port interaction already works, cover it with an egui pointer test and leave production code unchanged.

- [ ] **Step 3: Verify and commit**

```bash
cargo test -p vectorcraft-engine --test it coreldraw_text -- --nocapture
cargo test -p vectorcraft-ui-egui coreldraw_text_bar -- --nocapture
```

Commit with `feat: complete CorelDRAW linked-text workflows` only when both engine and UI evidence are present.

### Task 5: Prove path text, wrapping, and frame fitting

**Files:**
- Create: `crates/engine/tests/it/coreldraw_layout.rs`
- Modify: `crates/engine/tests/it/main.rs`

- [ ] **Step 1: Write the failing layout test**

Build a document containing an open path, path text, a wrap object, and an area frame:

```rust
let path = s.execute("path.create", &json!({"d": "M20 100 C100 20 200 20 280 100"})).unwrap()["id"].clone();
let text = s.execute("text.createInPath", &json!({"path": path, "mode": "onPath", "text": "Editable path text"})).unwrap()["id"].clone();
s.execute("text.reshapePath", &json!({"id": text, "path": "M20 120 C100 40 200 40 280 120"})).unwrap();
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
- Modify: `crates/ui-egui/src/coreldraw_text_bar.rs`

- [ ] **Step 1: Add failing evidence for the selected typography path**

Exercise character and paragraph styles, OpenType features, glyph alternates, variable-font axes where available, and missing-font reporting through the existing commands. Assert style propagation, local overrides, undo/redo, and native reopen. Keep tests independent of host-installed fonts by using the existing test-font fixtures; never add a font file to the repository.

- [ ] **Step 2: Expose the relevant existing panels/commands in the CorelDRAW workspace**

Link the Property Bar to Character, Paragraph, OpenType, Glyphs, and Text Styles panels through existing `window.panel` commands. Add no new font engine or glyph model. If all behaviour already passes, keep this as a test and documentation slice.

- [ ] **Step 3: Verify and commit**

```bash
cargo test -p vectorcraft-engine --test it coreldraw_text -- --nocapture
cargo test -p vectorcraft-ui-egui coreldraw_text_bar -- --nocapture
```

Commit with `test: verify CorelDRAW typography workflows` or the appropriate `feat:` if wiring changed.

### Task 7: Add dynamic page-number fields

**Files:**
- Modify: `crates/doc/src/text/runs.rs`
- Modify: `crates/engine/src/cmd/textedit.rs`
- Modify: `crates/engine/tests/it/coreldraw_layout.rs`
- Modify: `crates/corel-import/src/lib.rs`

- [ ] **Step 1: Write the failing dynamic-field test**

The test creates three pages, inserts a page-number field into master-page text, reorders pages, and requires the evaluated text to follow each current page. Native reopen must keep the field dynamic rather than flattening it. The test fails on the current tree because no such field exists.

- [ ] **Step 2: Add the smallest document representation**

Add an optional field to `TextRun` with serde defaults so old files are unchanged:

```rust
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum TextField {
    #[serde(rename = "pageNumber")]
    PageNumber,
}
```

Add `pub field: Option<TextField>` to `TextRun`, defaulting and skipping serialization when absent. `TextRun::new` and `TextRun::inline` initialize it to `None`. Add `text.pageNumber.insert` to insert the field at a caret/selection. Resolve the field from the current artboard at layout/render time; never store a page's current number as the field's permanent text.

- [ ] **Step 3: Map the CDR fallback explicitly**

When CDR cannot carry the dynamic field, export the evaluated number with a warning and preserve the native field in the CoralCraft native extension. Do not claim native CDR dynamic-field support without a cited format layout and test.

- [ ] **Step 4: Verify and commit**

```bash
cargo test -p vectorcraft-doc text
cargo test -p vectorcraft-engine --test it coreldraw_layout -- --nocapture
cargo test -p corel-import
```

Expected: page creation, reorder, native reopen, export fallback, and warnings pass. Commit with `feat: add dynamic page-number fields`.

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
