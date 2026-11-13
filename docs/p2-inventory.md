# P2 Capability Inventory

> Recorded on the P2 branch baseline (`ee7c5f44f`, 2026-11-11), following the [P2 implementation plan](superpowers/plans/2026-11-11-p2-professional-text-layout.md). The command inventory comes from `coralcraft-cli commands` on the same code baseline: 935 commands — 35 `text.*`, 23 `type.*`, 13 `artboard.*`, 26 `layer.*`, and no `layout.*`. Measured `cargo xtask stats`: 2,033 files, 878,299 Rust lines, 14,625 source `#[test]` attributes, and 66 `proptest!` blocks.

## Slice starting points

| Slice | Existing base | Start and evidence needed |
|---|---|---|
| Font family, style, and size | `crates/ui-egui/src/panels/character.rs`: `font_pickers`, `size_field`, `control_bar`; engine `text.setStyle` / `text.setRangeStyle` | **Partial.** The shared Control bar already has these controls while text is selected or edited (`character.rs:982-997`). Do not add a duplicate row. P2 evidence covers the CorelDRAW workspace, no-selection defaults, edits, undo/redo, and native persistence. |
| Paragraph alignment and lists | `crates/ui-egui/src/panels/paragraph.rs`: `align`, `run_on_paragraphs`, `control_bar`; engine `text.setStyle` / `text.setFormat` | **Partial.** Shared alignment/paragraph controls exist. Prove CorelDRAW mapping, mixed-selection behaviour, bullets/numbering, undo/redo, and persistence; add only missing no-selection or workspace-specific behaviour. |
| Linked text frames | `crates/engine/src/cmd/threads.rs`: `text.thread.create/add/releaseSelection/remove/break/info`; existing on-canvas ports | **Exists.** Tests must cover reflow, undo/redo, port interaction, break/remove, and native reopen. Add no second thread model. |
| Path text, wrapping, and fit | `text.createInPath`, `text.areaOptions`, `text.reshapePath`, `text.fitHeadline`; `crates/engine/src/cmd/textwrap.rs`; `crates/text/src/layout/` | **Exists.** Prove editing after native reopen and independent output through `usvg` and the Hayro-backed PDF reader. |
| Typography and styles | `text.setStyle`, `text.setFormat`, `text.setRangeStyle`, `text.setGlyphAlternate`, `text.glyphAlternates`, `text.missingFonts`; Character/Paragraph/OpenType/Glyphs/Text Styles panels | **Exists for the daily workflow.** Evidence must use existing synthetic test fonts and no host-dependent font assertion. Spell check remains open and is not claimed by P2. |
| Dynamic page numbers | `TextObject`/`TextRun` in `crates/doc/src/text.rs` and `text/runs.rs`; artboard commands | **Absent.** No dynamic page-number field exists. Required evidence: per-page evaluation, reorder, native persistence, and explicit CDR fallback. Adding a field to every `TextRun` literal is a broad change (113 literals in 29 files were found during review); choose the smallest model boundary and re-run impact before implementing. |
| Editable tables | Existing paths, live text, groups, and inline art; no `layout.*` command | **Absent.** `layout.table` must produce editable cell paths/text in one transaction with checked input limits, undo/redo, native reopen, and independent export evidence. |
| CDR text | `crates/cdr/src/parse.rs`: `decode_txtd_chunk`, `decode_loda_artistic_text`, `decode_loda_paragraph_text`; `crates/corel-import/src/lib.rs` text mapping | **Partial.** Current `txtd` handles ASCII/UTF-8/UTF-16LE, but legacy code-page decoding is not evidenced. Add only explicitly identified encodings, bounded errors, warnings, and residual preservation. Unknown encodings must not silently become UTF-8. S2 stays open. |
| Native and CDR persistence | `sync_coralcraft_extension` in `corel-import`; native V2.2 tests; P1 persistence/acceptance suites | **Exists.** P2 adds one consolidated document and verifies each semantic after native reopen; CDR unsupported data requires warnings, not silent flattening. |
| MCP and CLI | Generic `run_command` in `vectorcraft-mcp`; generated docs through `crates/mcp/tests/docs_sync.rs` | **Exists.** New commands are exercised through `run_command`; docs are regenerated rather than hand-counted. |
| Brochure acceptance | P1 `coreldraw_p1_acceptance.rs`, independent `usvg`, and `vectorcraft_pdf::import` | **New scenario.** Three pages with styled/linked/path text, dynamic page numbers on master content, an editable table, and a synthetic bitmap; revise, reorder, reopen, export, and independently read the PDF. |

## Delivered evidence (2026-11-11)

- Tasks 2–6 are evidence-only reuse slices: the shared Character/Paragraph controls, linked-text menu and canvas ports, path text/wrap/fit, and typography panels all pass CorelDRAW workspace tests. No duplicate text bar, formatter, thread model, or font engine was added.
- Task 7 (`3496cd828`) adds an object-level dynamic page-number field in `NodeExtras`; canvas, SVG, and PDF evaluate per page. CDR/CMX writes editable per-page text with an explicit warning. It does not claim arbitrary inline fields or native CDR dynamic fields.
- Task 8 (`79346f767`) adds bounded, atomic `layout.table` with editable paths/live text. Interactive table-tool editing remains open.
- Task 9 (`bef6489ac`) adds bounded legacy decoding and strict `txtd` warnings/residuals. The `stlt`/font-record-to-text encoding association landed 2026-11-12 (P2.CDR-TEXT, REV-0011): 8-bit `txtd` decodes with the one encoding the document's font records agree on, never guessed. S2 is unchanged.
- Tasks 10–12 add consolidated persistence, real MCP `run_command`, and `coreldraw_p2_acceptance.rs`: a three-page brochure built, reopened, revised, reordered, exported, and read through independent `usvg` and Hayro-backed PDF paths.
- Gate: `cargo xtask ci` passed fmt and Clippy, then workspace tests finished 4,375 passed / 1 failed — only documented x86_64 known-issue 0b. The earlier concurrent performance timeout did not recur. Doctests, cleanroom, the source-equivalent no-bundled-fonts checks, pdf-deps, assets, brands, cursors, i18n, layers, and wasm were each run separately and passed.
- Final measured `cargo xtask stats`: 2,039 files, 879,598 Rust lines, 14,647 source test attributes, and 66 proptest blocks. The focused CorelDRAW engine suite has 28 passing tests; the full CorelDRAW UI filter has 17 passing tests.

## Evidence rules used by every P2 task

1. Production changes start with a test observed failing on the pre-change tree.
2. Existing VectorCraft capability is covered by tests rather than replaced.
3. Native level 4 requires editable reopen plus independent parser/output evidence; text-only assertions or self-reimport alone are insufficient.
4. Current CDR oracle/sample checks remain `UNVERIFIED` in `docs/cdr-verification.md`; no P2 test closes S2.
5. Record any newly found gap here and in `docs/gaps.md`, with the exact failing test and owner.
