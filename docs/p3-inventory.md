# P3 Signature Effects and Image Workflows Inventory

> Baseline: `13232f0a917086f104edc4396da856c43bb5ae6a` (P2 complete). This inventory distinguishes an existing implementation from CorelDRAW workflow evidence; neither a registered command nor a self-round trip proves level 4.

Measured with the same-revision CLI/xtask binaries: the JSON command catalog was captured successfully; `cargo xtask stats` reports 2,039 files, 879,598 lines, 14,647 test attributes, and 66 proptest blocks.

## 1. Existing reusable base

| Slice | Existing implementation | Existing evidence | P3 acceptance still required |
|---|---|---|---|
| PowerClip | `object.clippingMask.make/release/editContents/editMask` in `crates/engine/src/cmd/object/commands.rs`; clip-group document/render support | **P3 Task 2:** two engine lifecycle tests and one CorelDRAW Object-menu test pass; native editing/nesting/release, independent `usvg` and Hayro-backed PDF reads | Dedicated CorelDRAW PowerClip naming and lock-contents-to-frame UI remain open; CDR mapping is audited in Task 10 |
| Blend | `object.blend.make/options/info/release/expand`, spine and order commands in `cmd/live/`; `panels/blend.rs`, `dialogs/blend_options.rs`, canvas blend controls | **P3 Task 4:** steps/spine/order/source editing, undo/redo, native reopen, independent SVG geometry, and CorelDRAW Blend-panel test pass | CDR live/flattened mapping is audited in Task 10 |
| Contour | `addon.contour.set` over `path.offsetPath`; `canvas/contour_bar.rs` | **P3 Task 4:** live update, native parameter persistence, independent SVG, explicit expand, and existing on-canvas corner/bake pointer test pass | Multi-step contour presets and CDR mapping remain open |
| Envelope | `object.envelope.makeWithWarp/makeWithMesh/makeWithTopObject`, reset/options/release/expand/editContents/setMeshPoint | **P3 Task 5:** warp options, content editing, mesh reset/point editing, two native reopens, independent SVG, expand, and CorelDRAW menu evidence pass | CDR mapping and top-object workflow are audited later |
| Distort | `distort.twist` through `effect.apply/setParams`; shared effect dialog | **P3 Task 5:** parameter edit, native reopen, independent SVG, and CorelDRAW dialog evidence pass | Push/pull and zipper-specific interactive handles remain open |
| Extrude/shadows/other signature effects | `threeD.extrude` (including bevel) and `stylize.dropShadow` in the ordered appearance stack | **P3 progress:** ordered parameter edits, native reopen, independent SVG+PDF, and CorelDRAW dialog tests pass | Block Shadow, Lens, Perspective, Symmetry, and their CorelDRAW-specific controls remain open |
| Effect stack | `effect.apply/list/remove/setParams/expandAppearance/duplicate/move`; visibility is handled by `setParams` | **P3 Task 7:** reorder/duplicate/hide/remove/undo/redo/expand, invalid-input atomicity, native reopen, CorelDRAW Appearance and existing drag/eye tests pass | No new stack model is needed; CDR fallback is audited in Task 10 |
| Advanced fills | Existing paint model, patterns/gradients/textures, `object.mesh.*` | **P3 progress:** fountain, vector-pattern, and mesh native/edit/independent SVG+PDF tests pass | Two-colour/bitmap patterns, textures, and their CorelDRAW controls remain open |
| Transparency | Existing transparency commands, merge modes, opacity masks, Transparency panel | **P3 progress:** object opacity + Multiply native/undo/export evidence and one CorelDRAW Transparency-panel test pass | Graded transparency, opacity masks, merge-mode matrix, and explicit export fallback remain open |
| Bitmap workflows | `file.place`, `object.cropImage`, and `adjust.brightnessContrast` | **P3 progress:** synthetic bitmap crop, undo/redo, adjustment parameters, native reopen, independent SVG, and invalid-input atomicity evidence pass | A dedicated resample command, other adjustments/effects, and explicit external-editor behaviour remain open |
| PowerTRACE | `imageTrace.make/makeAndExpand/setView/presets.*` in `cmd/buildcmds/trace.rs`; `panels/image_trace.rs`; `vectorcraft-trace` | **P3 progress:** synthetic colour trace, Outlines view, native live reopen, editable expanded paths, independent SVG, and CorelDRAW Image Trace panel workflow pass | Centreline, colour reduction, smoothing, presets, and undo/redo remain open |
| CDR/CMX | `corel-import` and `coralcraft-cdr` mapping, residuals and warning paths | **P3 progress:** PowerClip + Extrude/Drop Shadow CDR/CMX reopen; live Blend now exports expanded vector groups with an explicit warning instead of an empty path | Combined mesh/transparency/bitmap/trace audit, hostile-input gate, residual matrix, and S2 real oracle/samples remain open |
| Automation | All features already use shared engine command dispatch; MCP `run_command`; generated `docs/mcp.md` | **P3 Task 11:** no new command IDs were added; both live-registry/protocol `docs_sync` tests pass with a clean generated document | Future commands must enter the same shared dispatch and add invalid-input evidence |

## 2. Evidence and architecture boundaries

- Native `.vectorcraft`/`.drawcraft`/`.vctemplate` reopen must preserve editable source objects and parameters.
- `usvg` and Hayro-backed `vectorcraft-pdf` are the independent SVG/PDF readers. Text search in our own export is supplementary, not independent parsing.
- SVG/PDF expansion may show generated geometry while the native document keeps live effects. Expansion is not evidence that CDR retains live effects.
- CDR/CMX must warn when it flattens or drops live semantics. No oracle or real-sample result may be claimed while S2 prerequisites are absent.
- Reuse VectorCraft only. TraceDraw remains a selectively audited reference/donor, not a second application base.

## 3. Acceptance scenarios

1. **Poster:** nested PowerClip, advanced fill/transparency, Blend, Envelope, shadow, and live text; native reopen and revision; independent SVG/PDF reads; explicit CDR/CMX fallback ledger.
2. **Traced image:** handcrafted synthetic bitmap, tracing preset/mode/colour/smoothing, editable expanded paths, bitmap adjustment, native reopen and independent SVG read.
3. **Automation:** all new commands run through shared `run_command`; invalid input returns errors without mutation; generated MCP documentation matches the live registry.
4. **Measured P3 acceptance:** `coreldraw_p3_acceptance` passes with PowerClip + Extrude + Drop Shadow + Blend + live text + traced paths, two native reopens, source/parameter/text revision, independent `usvg` and Hayro-backed PDF reads, and an explicit CDR live-Blend warning. This does not close the remaining P3 capability rows.

## 4. Gate baseline

P2's final `cargo xtask ci` result was fmt and Clippy passed, workspace tests 4,375 passed and 1 failed. The only failure was known-issue 0b (`tests::constrain_new::constrain_angle_turns_the_base_orientation_of_polygons_and_stars`), an existing x86_64 macOS 1-ulp issue. All gates skipped by that stop were run separately and passed. P3 must record its own results and must not describe that baseline as a clean full-CI pass.

Final P3 working-tree stats: 2,046 files, 880,464 lines, 14,672 test attributes, and 66 proptest blocks.

P3 gate: the first CI run stopped on formatting in the new acceptance test; after formatting and re-running that test, the full gate passed fmt and Clippy, then workspace tests finished **4,375 passed / 1 failed / 2 ignored**, solely known-issue 0b. The large-edit 50,000-object test passed without the earlier concurrent-build timeout. Doctests, cleanroom, no-bundled-fonts tests/Clippy/install-font checks, pdf-deps, assets, brands, cursors, i18n-report, layers, and wasm were then run separately and all exited successfully. Both no-default-feature dependency trees contain no `bundled-fonts` feature. Wasm emitted four existing UI warnings, not errors. This is not a clean full-CI claim because known-issue 0b remains red.
