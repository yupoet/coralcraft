# CorelDRAW Parity — CoralCraft Evidence Matrix (D1–D6)

> **Target:** the complete CorelDRAW main application, tracked by capability domain D1–D6 with
> evidence levels 0–5. This file replaced the pre-P0 matrix of manually estimated percentages; those
> claims were withdrawn by the umbrella specification. There is deliberately **no aggregate
> percentage** in this document.
> Umbrella roadmap:
> [`docs/superpowers/specs/2026-11-09-coreldraw-main-application-roadmap-design.md`](superpowers/specs/2026-11-09-coreldraw-main-application-roadmap-design.md)
> (direction approved by the operator 2026-11-09).
>
> A separate document exists with a similar name:
> [`docs/vectorcraft-target-app-parity.md`](vectorcraft-target-app-parity.md) is upstream VectorCraft's
> own **Adobe Illustrator** parity assessment, preserved verbatim after the 2026-11-08 sync. It
> measures a different target against different evidence and is never evidence of CorelDRAW parity.

## Evidence levels

Each capability row has a current level and links to its evidence:

| Level | Name | Minimum meaning |
|---|---:|---|
| 0 | Absent | No supported implementation. |
| 1 | Model | Document semantics and native serialization exist. |
| 2 | Command | Shared engine command, undo/redo, and headless behaviour tests pass. |
| 3 | Interactive | Real tool/Property Bar/Docker interaction and UI tests pass. |
| 4 | Interoperable | Native reopen preserves editability; applicable independent import/export checks pass. |
| 5 | Production | The feature passes its real workflow, target-platform, and performance acceptance scenario. |

A menu entry, command registration, file extension, or self-round trip alone is never completion.
Feature counts are derived from the matrix, not manually estimated percentages.

## Current-tree capability matrix

Levels are conservative floors: a level is raised only by a current-tree test cited in the row, never
by a historical result. A domain is never marked complete because one layer exists — for example, a
level 4 row still owes its level 5 production scenario.

| Capability | Level | Current-tree evidence | Historical evidence |
|---|---:|---|---|
| CorelDRAW tool mapping and Property Bar | 3 | P1 Task 2 (V4.3 core): the eleven public-guide core tools map onto VectorCraft tools with their V4.2 shortcuts ([`corel_tools_map_onto_known_vectorcraft_tools`](../crates/tools/src/coreldraw.rs)); the CorelDRAW workspace shows their toolbox with CorelDRAW labels ([`coreldraw_workspace_shows_the_core_tool_toolbox`](../crates/ui-egui/src/toolbar/tests.rs)) and Property Bar rows over `tool.select`/`tool.setOption`/`object.setLiveShape`, with the upstream bar unchanged outside the workspace ([`property_bar_rows_show_per_core_tool`](../crates/ui-egui/src/coreldraw_bar.rs)). Workspace shell proven by [`coreldraw_workspace_preset_configures_bars_and_dockers_and_round_trips_v4_1`](../crates/ui-egui/src/workspaces.rs) and the shortcut preset by [`coreldraw_shortcut_preset_is_conflict_free_and_maps_public_shortcuts_v4_2`](../crates/ui-egui/src/shortcut_editor.rs). Level 4 needs the persistence/interop evidence of P1 Tasks 11/13. | — |
| Selection and node interaction | 4 | P1 Task 3: the Shape workflow as commands with undo/redo (node select/move/insert/delete/convert, selection modifiers) plus the new `select.marquee` (touched/`inside`/modes), native `.vectorcraft` save-reopen keeping the nodes editable, and an independent parse of the exported SVG geometry in [`coreldraw_node_workflow.rs`](../crates/engine/tests/it/coreldraw_node_workflow.rs). Interactive evidence from the synced tools: [`marquee_selects_anchors`](../crates/tools/src/direct/tests.rs) (the mapped Shape tool's marquee), selection behaviour in [`crates/engine/tests/it/selection_commands.rs`](../crates/engine/tests/it/selection_commands.rs), node editing in [`crates/pathops/src/edit.rs`](../crates/pathops/src/edit.rs). Level 5 production scenario lands with P1 Task 13. | — |
| Shaping and path operations | 3 | VectorCraft command/UI coverage in [`crates/pathops/src/boolean.rs`](../crates/pathops/src/boolean.rs) and [`crates/pathops/src/pathfinder.rs`](../crates/pathops/src/pathfinder.rs); Corel-specific mapping (Weld/Trim/Intersect/Simplify/Boundary naming and workflow) remains open ([`docs/gaps.md`](gaps.md), P1). | — |
| Pages, master layers, and page navigator | 4 | V2.2 native persistence plus V3 command/UI tests: [`vectorcraft_drawcraft_and_vctemplate_preserve_coralcraft_extension_block_losslessly_v2_2`](../crates/engine/src/cmd/fileio/tests.rs) (native reopen preserves the model), [`coreldraw_master_and_desktop_layer_commands_and_roundtrip_v3_2`](../crates/engine/src/cmd/layer.rs), and [`page_navigator_renders_tabs_and_executes_add_delete_rename_reorder_and_switch_v3_3`](../crates/ui-egui/src/page_navigator.rs). Level 5 production scenarios (long-document recovery, facing pages) are open. | — |
| Fills, outlines, palettes, and transparency | 3 | VectorCraft UI/render plus CoralCraft palette bar: [`palette_bar_renders_swatches_and_applies_fill_on_left_click_and_stroke_on_right_click_v4_1`](../crates/ui-egui/src/palette_bar.rs); fountain/pattern/transparency paint semantics live in the synced VectorCraft appearance stack ([`crates/doc/src/`](../crates/doc/src/), [`crates/render/src/`](../crates/render/src/)). Domain-specific interoperability is incomplete: Corel `.cpl`/`.xml` palettes in the bar and mesh/pattern-fountain CDR fidelity are open ([`docs/gaps.md`](gaps.md), V5.3/P1–P3). | — |
| Artistic/paragraph text and typography | 3 | Synced VectorCraft text engine and UI: shaping/hyphenation/justification in [`crates/text/src/`](../crates/text/src/) with engine coverage in [`crates/engine/src/cmd/fileio/tests_text.rs`](../crates/engine/src/cmd/fileio/tests_text.rs). Corel CDR semantic coverage is incomplete (old code pages, tabs/bullets/drop caps from `stlt`); decoding gaps and the VectorCraft-first decision are recorded in [`docs/coreldraw-reference-adoption.md`](coreldraw-reference-adoption.md). | — |
| PowerClip container workflow | 2 | Import/render/native residual support exists: PowerClip structures map through [`crates/corel-import/src/lib.rs`](../crates/corel-import/src/lib.rs) with residuals preserved in the CoralCraft extension block. Full edit/extract/lock workflow is open ([`docs/gaps.md`](gaps.md), V5.1/P3); the TraceDraw behaviour reference decision is in [`docs/coreldraw-reference-adoption.md`](coreldraw-reference-adoption.md). | — |
| CorelDRAW live-effect workflow | 2 | VectorCraft effects exist ([`crates/effects/src/`](../crates/effects/src/) with the engine effect stack); CorelDRAW effect mapping and on-canvas handles are open ([`docs/gaps.md`](gaps.md), V5.2/P3). The TraceDraw audit confirmed a real, test-backed live-effect model to adapt **per effect onto VectorCraft infrastructure** — see the [live-effect finding](coreldraw-reference-adoption.md#adoption-table). | — |
| Bitmap tracing | 3 | Synced VectorCraft tracing and UI tests: [`crates/trace/src/`](../crates/trace/src/) with quality presets covered under [`crates/trace/tests/`](../crates/trace/tests/). PowerTRACE-specific presets/behaviour are a P3 reference item ([`docs/coreldraw-reference-adoption.md`](coreldraw-reference-adoption.md)). | — |
| Colour management and prepress | 3 | Synced VectorCraft ICC/PDF/print infrastructure: PDF/X output covered by [`crates/engine/src/cmd/fileio/tests_pdfx.rs`](../crates/engine/src/cmd/fileio/tests_pdfx.rs) and colour settings by [`crates/engine/tests/color_settings.rs`](../crates/engine/tests/color_settings.rs). Corel workflow acceptance (spot plates through separations, overprint preview, imposition, data merge) is open (P4). | — |
| CDR/CMX read/write | 2 | Engine read/write commands and regression tests exist: [`cdr_and_cmx_engine_pipeline_passes_libcdr_oracle_and_real_samples`](../crates/engine/src/cmd/fileio/tests.rs) and [`crates/corel-import/tests/oracle_and_samples.rs`](../crates/corel-import/tests/oracle_and_samples.rs). Current independent oracle state is recorded separately in [`docs/cdr-verification.md`](cdr-verification.md) — **all six checks are UNVERIFIED on the current tree**, so level 2 stands; interactive and independent evidence must raise this conservatively. | V1.4 (2026-10-22, pre-sync tree) ran the libcdr `cdr2raw`/`cmx2raw` differential oracle and opened 65/65 valid real-world `.cdr` samples with exact counts — see V1.4 in [`docs/gaps.md`](gaps.md). Historical only; it does not upgrade the current-tree level. |
| VectorCraft native/file compatibility | 4 | V2.1–V2.2 compatibility and lossless extension tests: [`vectorcraft_format_compatibility_matrix_covers_every_format_v2_1`](../crates/engine/src/cmd/fileio/tests.rs), [`vectorcraft_drawcraft_and_vctemplate_preserve_coralcraft_extension_block_losslessly_v2_2`](../crates/engine/src/cmd/fileio/tests.rs), and [`legacy_coralcraft_json_and_zip_containers_open_and_place_losslessly_v2_3`](../crates/engine/src/cmd/fileio/tests.rs). Format matrix details in [`docs/file-format-parity.md`](file-format-parity.md). | — |
| MCP and CLI | 2 | Synced headless command/protocol tests; regenerated docs prove registry coverage: [`crates/mcp/tests/docs_sync.rs`](../crates/mcp/tests/docs_sync.rs) and [`crates/mcp/tests/protocol_props.rs`](../crates/mcp/tests/protocol_props.rs) behind the generated tool reference in [`docs/mcp.md`](mcp.md). CorelDRAW-specific command coverage grows with each phase (continuous workstream, not a late phase). | — |
| Sandboxed scripting/plugins | 2 | Synced VectorCraft infrastructure: the WebAssembly plug-in sandbox in [`crates/plugins/src/`](../crates/plugins/src/) (D6 base; not a VBA/COM layer — see the note in [`docs/plugins.md`](plugins.md)). CorelDRAW object-model coverage is open (P5). | — |

## How to read and maintain this matrix

- **Current-tree evidence** means a test or command that runs against the current repository tree.
  **Historical evidence** means an older milestone result; it is kept for context and never raises a
  level. The only historical result currently cited is the V1.4 CDR oracle/sample run above.
- A capability reaches level 4 for CDR/CMX only when [`docs/cdr-verification.md`](cdr-verification.md)
  records real (non-self-skipping) oracle and sample passes on the current tree; until then the CDR/CMX
  row stays at level 2 regardless of writer/reader test counts.
- TraceDraw-derived behaviour follows the adoption boundaries in
  [`docs/coreldraw-reference-adoption.md`](coreldraw-reference-adoption.md); that document, not this
  matrix, decides what may be ported.
- Work order comes from [`docs/gaps.md`](gaps.md); phase exit criteria come from the umbrella
  specification.
