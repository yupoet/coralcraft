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
| Extrude/shadows/other signature effects | Ordered appearance effects in `cmd/effectcmd.rs`, `panels/properties.rs`, Appearance and effect dialogs | Existing effects tests | Per-effect D4 inventory; source/parameters after reopen; explicit raster/vector fallback |
| Effect stack | `effect.apply/list/remove/setParams/expandAppearance/duplicate/move`; visibility is handled by `setParams` | Effect command and Appearance tests | Ordered edit/hide/remove/clone/expand in CorelDRAW workspace, native persistence |
| Advanced fills | Existing paint model, patterns/gradients/textures, `object.mesh.*` | **P3 progress:** fountain, vector-pattern, and mesh native/edit/independent SVG+PDF tests pass | Two-colour/bitmap patterns, textures, and their CorelDRAW controls remain open |
| Transparency | Existing transparency commands, merge modes, opacity masks, Transparency panel | **P3 progress:** object opacity + Multiply native/undo/export evidence and one CorelDRAW Transparency-panel test pass | Graded transparency, opacity masks, merge-mode matrix, and explicit export fallback remain open |
| Bitmap workflows | Existing image placement, crop/resample/adjustment and bitmap-effect commands | Image/effect tests | Synthetic-image end-to-end workflow, native reopen, explicit external-editor result |
| PowerTRACE | `imageTrace.make/makeAndExpand/setView/presets.*` in `cmd/buildcmds/trace.rs`; `panels/image_trace.rs`; `vectorcraft-trace` | Trace engine, UI run, and tracer tests | Outline/centreline/colour/smoothing/preset evidence; editable expanded paths and independent SVG |
| CDR/CMX | `corel-import` and `coralcraft-cdr` mapping, residuals and warning paths | P1/P2 appearance/text/page tests | Per-feature supported mapping or explicit warning/residual; S2 remains open |
| Automation | All features already use shared engine command dispatch; MCP `run_command`; generated `docs/mcp.md` | P1/P2 `run_command` and docs-sync tests | Every new P3 command exercised through real dispatch; generated docs clean |

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

## 4. Gate baseline

P2's final `cargo xtask ci` result was fmt and Clippy passed, workspace tests 4,375 passed and 1 failed. The only failure was known-issue 0b (`tests::constrain_new::constrain_angle_turns_the_base_orientation_of_polygons_and_stars`), an existing x86_64 macOS 1-ulp issue. All gates skipped by that stop were run separately and passed. P3 must record its own results and must not describe that baseline as a clean full-CI pass.
