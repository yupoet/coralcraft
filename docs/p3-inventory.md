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
| Advanced fills | Existing paint model, arbitrary vector/image `PatternDef.art`, gradients, `object.mesh.*` | **P3 progress:** fountain, vector-pattern, and mesh native/edit/independent SVG+PDF tests pass. Generic two-colour pattern art now proves two ordinary vector objects, ordinary colour edits, undo/redo, native reopen and another edit; independent `usvg` checks the target's actual tile rect and recursive non-empty paths with literal edited RGB values, while Hayro-backed PDF import reconstructs a connected real pattern definition and vector tile art. Generic bitmap pattern art now proves `file.place {replace:true}` inside pattern editing, undo/redo, native reopen, an editable `NodeKind::Image` with exact replacement pixels, and an unflattened vector target; independent `usvg` decodes the connected tile's exact replacement PNG and Hayro-backed PDF import reconstructs the bitmap pattern/tile art. The rendered CorelDRAW Object > Pattern Make/Edit controls are clicked through synthetic pointer events and the visible Pattern Options Done control is clicked through AccessKit, proving a real edit/done/edit/done lifecycle without direct menu invocation or UI-state injection | This is generic two-colour and bitmap pattern art, not a Corel-specific foreground/background/kind model. Texture fill remains unsupported, so Task 3 Step 1 stays open. Generic CDR export currently substitutes a default black/white checkerboard, so CDR pattern parameter fidelity remains open. PDF evidence is reconstructed structure, not live PDF editing |
| Transparency | Existing transparency commands, merge modes, opacity masks, Transparency panel | **P3 Task 3 Step 2:** uniform opacity + Multiply remains proven. Graded stop opacity edits survive undo/redo/native reopen; independent `usvg` verifies real gradient offsets/colours/alpha. A top vector object becomes editable mask art outside the layer tree; clip/invert, disable/enable, source/target edits, undo/redo/native reopen, and release/restoration pass. Independent `usvg` proves the edited vector mask art is connected to the target. Actual CorelDRAW Transparency panel Make Mask/Invert Mask/Release controls pass through AccessKit | Step 1 texture fill, a broader merge-mode matrix, and consolidated CDR/CMX transparency fallbacks remain open. Graded PDF evidence is one page with vector geometry; mask PDF evidence is one page with reconstructed vector soft-mask structure. Neither is a live-PDF/native-editing-relationship claim |
| Bitmap workflows | `file.place`, `object.cropImage`, `object.resampleImage`, `adjust.brightnessContrast`, and live `blur.gaussian`; existing `links.editOriginal` handoff and generic effect dialog | **P3 Task 8:** synthetic bitmap crop, undo/redo, adjustment parameters, native reopen, independent SVG, and invalid-input atomicity evidence pass. Resampling adds decoded output dimensions/pixels for every filter, identity/appearance/display-bounds preservation, undo/redo/native reopen, linked-source-to-embedded-PNG derivation without source-file changes, structural independent `usvg` image size/transform checks, and real MCP `run_command`. The final PowerClip scenario preserves original PNG bytes/pixels, the vector frame/live clip, and ordered effects/radius through native reopen; independent `usvg` proves the connected non-empty clip, actual image, and Gaussian structure. Hayro-backed PDF import proves a real page and rendered raster composition, not live clip/effect semantics. The CorelDRAW Gaussian dialog confirms a radius edit on a selected bitmap. Links tests prove system/PhotoCraft handoff and explicit errors 9/9, not an external-editor round trip | CMYK and 32-bit-float rasters are rejected rather than silently converted |
| PowerTRACE | `imageTrace.make/makeAndExpand/setView/presets.*` in `cmd/buildcmds/trace.rs`; `panels/image_trace.rs`; `vectorcraft-trace` | **P3 Task 9:** outline and centreline make/view/make-and-expand/native-reopen/independent-`usvg` evidence passes. The expanded centreline itself is selected and edited to `#e04838` at 7.25 pt. Independent `usvg` finds exactly one matching path and checks literal stroke, no Close segment, and horizontal endpoint relationship/length. Limited-palette retrace reduces four unique fills to two in document and `usvg`; higher Paths fidelity adds anchors on the fixed sine fixture in document and export. Saved-preset lifecycle/revision/invalid-input and Prefs-vs-document persistence pass. Make/make-and-expand/retrace/view undo/redo checks identity, selection, parent/stacking position and history stacks, including live metadata/hidden-source restoration and expanded editable geometry. Invalid retrace is atomic with exactly one redo entry before successful redo. Actual CorelDRAW panel controls drive preset, a real Color→Grayscale→Color transition with neutral grayscale fills and exact original Colour palette restoration, colours, Paths, view, Preview, Trace, and same-ID Expand | Consolidated CDR/CMX trace fallback remains under Task 10; no real-sample or CDR certification is claimed |
| CDR/CMX | `corel-import` and `coralcraft-cdr` mapping, residuals and warning paths | **P3 progress:** PowerClip + Extrude/Drop Shadow CDR/CMX reopen; live Blend exports expanded vector groups with an explicit warning instead of an empty path. Used generic pattern paints and live trace metadata now have explicit CDR/CMX fallback warnings; native editability, retrace, ordinary-object self-reimport, edited expanded-path persistence, unused-pattern and expanded-trace negative controls pass | Combined mesh/transparency/bitmap/residual matrix and S2 real oracle/samples remain open. The exact new warnings and focused gate are in §5; self-reimport is not independent CDR certification |
| Automation | All features use shared engine command dispatch; MCP `run_command`; generated `docs/mcp.md` | **P3 Task 11:** `object.resampleImage` has real `run_command` coverage and no bespoke tool; both live-registry/protocol `docs_sync` tests pass with a clean generated document | Future commands must enter the same shared dispatch and add invalid-input evidence |

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

Current P3 working-tree stats after the Task 10 nested-bitmap reconstruction follow-up: 2,048 files, 884,864 lines, 14,760 test attributes, and 66 proptest blocks. The desktop layer totals 533,934 lines (`vectorcraft-ui-egui` 213,705 + `coralcraft` 13,519 + `vectorcraft-tools` 57,314 + `vectorcraft-engine` 249,396).

P3 gate: the first CI run stopped on formatting in the new acceptance test; after formatting and re-running that test, the full gate passed fmt and Clippy, then workspace tests finished **4,375 passed / 1 failed / 2 ignored**, solely known-issue 0b. The large-edit 50,000-object test passed without the earlier concurrent-build timeout. Doctests, cleanroom, no-bundled-fonts tests/Clippy/install-font checks, pdf-deps, assets, brands, cursors, i18n-report, layers, and wasm were then run separately and all exited successfully. Both no-default-feature dependency trees contain no `bundled-fonts` feature. Wasm emitted four existing UI warnings, not errors. This is not a clean full-CI claim because known-issue 0b remains red.

Task 8 resample gate at `643f06c51`: `cargo xtask ci` passed fmt and Clippy, then engine library tests finished **4,376 passed / 1 failed / 2 ignored**, solely known-issue 0b. The complete CorelDRAW integration group passed **53/53**. Every skipped later gate was separately run and passed, including doctests, all no-bundled-fonts checks and both dependency-tree assertions, cleanroom, pdf-deps, assets, brands, cursors, i18n-report, layers, and wasm. This is not a clean full-CI claim.

Task 8 final focused gate: image workflows **11/11**, PowerClip **2/2**, CorelDRAW effects UI **8/8**, raster filters **11/11**, Gaussian SVG **3/3**, and `vectorcraft-effects` **380 passed / 2 ignored** pass. Existing bitmap-filtered tests, `cargo fmt --all -- --check`, focused engine/UI Clippy with `-D warnings`, and unblessed MCP `docs_sync` **2/2** also pass. This evidence-only slice did not change production code or the generated MCP registry document.

Completed Task 8 gate at `dfab92929`: `cargo xtask ci` passed fmt and Clippy, then engine library tests finished **4,376 passed / 1 failed / 2 ignored**, solely known-issue 0b. The complete CorelDRAW integration group passed **54/54**. Every skipped later gate was separately run and passed: doctests, cleanroom, no-bundled-fonts tests/Clippy/install-font checks, pdf-deps, assets, brands, cursors, i18n-report, layers, and wasm. This is not a clean full-CI claim.

Task 3 Step 2 focused gate: `coreldraw_fills_transparency` **6/6**, existing opacity-mask engine tests **31/31**, gradient-filtered engine tests **102/102**, CorelDRAW effects UI **9/9**, Transparency panel **4/4**, mask-view panel tests **3/3**, SVG mask options **4/4**, SVG round trips **46/46**, PDF opacity-mask tests **5/5**, and PDF gradient tests **16/16** pass. `cargo fmt --all -- --check`, focused engine/UI Clippy with `-D warnings`, and unblessed MCP `docs_sync` **2/2** also pass. This evidence-only slice changed no production code or generated MCP registry document. The graded PDF check proves one page with vector geometry; the mask PDF check proves one page with reconstructed vector soft-mask structure. Neither claims live PDF editing or preservation of the original native editing relationships.

Task 3 Step 2 gate at `209e66996`: `cargo xtask ci` passed fmt and Clippy, then engine library tests finished **4,376 passed / 1 failed / 2 ignored**, solely known-issue 0b. The complete CorelDRAW integration group passed **56/56**. Every skipped later gate was separately run and passed: doctests, cleanroom, no-bundled-fonts tests/Clippy/install-font checks, pdf-deps, assets, brands, cursors, i18n-report, layers, and wasm. This is not a clean full-CI claim.

Generic two-colour/bitmap pattern-art focused gate: `coreldraw_fills_transparency` **8/8**, pattern-filtered engine tests **143/143**, CorelDRAW effects UI **10/10**, Pattern Options-filtered UI **10/10**, SVG pattern tests **1/1 unit + 3/3 integration**, and PDF pattern tests **4/4** pass. `cargo fmt --all -- --check`, focused engine/UI Clippy with `-D warnings`, and unblessed MCP `docs_sync` **2/2** also pass. This evidence-only slice changed no production code or generated MCP registry document. Independent `usvg` proves the actual target-connected tile rectangle, recursive edited vector RGB paths, and exact replacement PNG dimensions/pixels. Hayro-backed PDF import proves reconstructed pattern definitions and vector/bitmap tile art connected to the target paint; neither PDF check claims live PDF editing. Task 3 Step 1 stays open for texture fill; Step 3, P3.3, and P3 stay open.

Generic two-colour/bitmap pattern-art final gate at `8998aa38d`: `cargo xtask ci` passed fmt and Clippy, then engine library tests finished **4,376 passed / 1 failed / 2 ignored**, solely known-issue 0b. The complete CorelDRAW integration group passed **58/58**. Every skipped later gate was separately run and passed: doctests, cleanroom, no-bundled-fonts tests/Clippy/install-font checks and both dependency-tree assertions, pdf-deps, assets, brands, cursors, i18n-report, layers, and wasm. A concurrent-feature-build rlib failure in doctests was rerun serially and passed; it was not a product test failure. Wasm emitted the same four existing UI warnings, not errors. This is not a clean full-CI claim.

Rendered-menu evidence follow-up: CorelDRAW effects UI **10/10**, menu tests **40/40**, and Pattern Options-filtered UI **10/10** pass, along with `cargo fmt --all` and focused UI Clippy with `-D warnings`. The test renders `menus::menu_bar` in a persistent `egui::Context`, locates the visible Object, Pattern, and Make/Edit text, and sends synthetic PointerMoved/press/release events through the repository's real menu-test helpers. It does not call `menus::invoke` or `click_target`, and it does not inject menu/panel state. The visible Pattern Options Done control is clicked through AccessKit in both cycles of the same application model.

Task 9 PowerTRACE focused gate: image workflows **16/16**, CorelDRAW effects UI **10/10**, Image Trace panel tests **43/43**, `vectorcraft-trace` **252 passed / 3 existing ignored**, and engine `image_trace` tests **30/30** pass. `cargo fmt --all -- --check`, focused engine/UI Clippy with `-D warnings`, and unblessed MCP `docs_sync` **2/2** also pass. This evidence-only slice changed no production code or generated MCP registry document. P3.9 is complete; P3.10, the other open capability rows, S2, and P3 remain open.

Task 9 final gate at `7f1c82107`: `cargo xtask ci` passed fmt and Clippy, then engine library tests finished **4,376 passed / 1 failed / 2 ignored**, solely known-issue 0b. The complete CorelDRAW integration group passed **63/63**. Every skipped later gate was then run serially and passed: doctests, cleanroom, no-bundled-fonts tests/Clippy/install-font checks, pdf-deps, assets, brands, cursors, i18n-report, layers, and wasm. This is not a clean full-CI claim.


## 5. Task 10 focused fallback-warning slice

The two new CDR/CMX warnings are exactly:

- `generic Pattern paints are substituted with a default black/white checkerboard/black or dropped in CDR/CMX; native pattern definitions and paints remain editable`
- `live trace parameters and source relationships are dropped in CDR/CMX; vector results and the hidden source become ordinary objects`

Warnings are collected during `corel-import` conversion through `export_document_with_warnings` and the warning-returning CDR7/CDRX/CMX1 writers; existing export/write APIs retain their signatures, and legacy wrappers return the same bytes as the warning-returning functions at the same revision; this is wrapper equivalence, not independent base-byte certification. The engine supplies its normalized export document after linked-art materialization and `without_edit_modes`, so linked `PlacedDocument::art` is covered exactly once per fallback category. Appearance warnings are emitted only for the last visible fill/stroke actually converted (plus actual text-style colour fallback), never for invisible or shadowed items, unused definitions, or symbol/mask editing-only layers. Direct `corel_import::export_document` inspection proves the existing 12 pt black/white checker substitution. The trace warning is driven by actual `extras().trace` metadata during the same node conversion, so expanded traces do not warn.

RED evidence before the production edit: `generic_pattern_export_warns_without_mutating_the_editable_native_pattern` and `live_trace_export_warns_and_reopens_only_as_ordinary_vectors_and_source_image` both failed on `warnings == []`; the expanded-trace control already passed. GREEN: `coreldraw_p3_persistence` **5/5**, image workflows **16/16**, fills/transparency **8/8**, the complete CorelDRAW integration group **66/66**, warning-filtered engine tests **33/33**, `corel-import` **8 unit + 2 oracle self-skip**, `coralcraft-cdr` **58 unit + 2 oracle self-skip**, hostile import fuzz **190/190**, and MCP `docs_sync` **2/2** pass. Formatting and focused engine Clippy with `-D warnings` pass. Both `CORALCRAFT_FUZZ_ITERS=2000 cargo run --manifest-path fuzz/Cargo.toml --release --bin probe` and `--bin parse_cdr` exited successfully. The oracle/sample tests self-skip on this host and are not independent CDR certification.

Native reopen retains editable named pattern definitions/paints and live-trace preset/params/view, and permits retrace. CDR/CMX self-reimport contains non-empty ordinary vector paths and an ordinary source image without `extras().trace`; edited expanded centreline stroke colour and geometry survive native/CDR/CMX reopen without a live-trace warning. This does not claim native Pattern or live PowerTRACE persistence in CDR/CMX. The combined mesh/transparency/bitmap/residual matrix remains open, so Task 10/P3.10, P3, and S2 stay open.

Full CI for this slice passed formatting and workspace Clippy, then stopped at the sole engine-library failure, known-issue 0b: **4,376 passed / 1 failed / 2 ignored**. Every skipped later gate was then run serially and passed: doctests, cleanroom, no-bundled-fonts tests/Clippy/install-font checks and both dependency-tree assertions, pdf-deps, assets, brands, cursors, i18n-report, layers, and wasm. Wasm emitted the same four existing UI warnings, not errors. This is not a clean full-CI claim.


## 6. Task 10 normalized-warning review

Review found that the initial stored-layer scan missed linked `PlacedDocument::art` and warned for invisible, shadowed, and temporary-edit-only patterns. Before the production fix, `linked_placed_pattern_and_live_trace_warn_once_per_fallback_category` returned only the existing Placed Document warning, `invisible_and_shadowed_pattern_items_do_not_warn` returned a generic-pattern warning, and `temporary_symbol_and_mask_editing_layers_do_not_warn` returned a generic-pattern warning. The public API test also failed because the warning-returning APIs did not exist.

The fix keeps warning text unchanged and records it in `corel-import` at actual conversion sites. The engine routes CDR7/CDRX/CMX1 through warning-returning writers over its existing normalized document; compatibility wrappers preserve `export_document` and all three existing writer signatures, returning the same bytes as the warning-returning functions at that revision. Appearance conversion now selects the final visible fill/stroke before converting it. Linked placed pattern/trace art warns once per category, while hidden, shadowed, unused-definition, expanded-trace, and temporary-edit-only controls stay silent.

GREEN: persistence **8/8**, public export API compatibility **1/1**, complete CorelDRAW integration **69/69**, placed-document tests **41/41**, appearance tests **84/84**, warning-filtered tests **33/33**, `corel-import` **8 unit + 1 public API + 2 oracle self-skip**, `coralcraft-cdr` **58 unit + 2 oracle self-skip**, hostile import fuzz **190/190**, and MCP `docs_sync` **2/2** pass. Formatting and focused corel-import/engine Clippy with `-D warnings` pass. Both 2,000-iteration release `probe` and `parse_cdr` runs exit successfully. Self-skipping oracle/sample tests remain non-certifying; Task 10/P3.10, P3, and S2 stay open.

The normalized-review full CI run passed formatting and workspace Clippy, then stopped only at known-issue 0b: **4,376 passed / 1 failed / 2 ignored**. Every skipped later gate was run serially and passed: doctests, cleanroom, no-bundled-fonts tests/Clippy/install-font checks and both dependency-tree assertions, pdf-deps, assets, brands, cursors, i18n-report, layers, and wasm. Wasm emitted the same four existing UI warnings, not errors. This is not a clean full-CI claim.


## 7. Task 10 remaining conversion channels

Final review found four coverage gaps: later text-run Pattern fills, any text-run Pattern stroke, image colorize Pattern, and enabled opacity-mask art containing Pattern plus live trace metadata. Before the production change, the three paint-channel tests returned `warnings == []`; the active-mask test returned no live-trace warning, and the shared wording assertion failed because dropped channels were not named.

Conversion now checks every text run's fill/stroke and image `extras().colorize`, and recursively inspects enabled `extras().mask` art. Temporary editing layers remain excluded by normalization, while the active-mask case warns once per category. The pattern warning is now exactly:

`generic Pattern paints are substituted with a default black/white checkerboard/black or dropped in CDR/CMX; native pattern definitions and paints remain editable`

This wording distinguishes substitution from dropped paint/metadata; no preservation or new format mapping is claimed. The live-trace warning is unchanged. Linked placed art, visible-paint selection, unused-definition/expanded-trace controls, temporary symbol/mask editing controls, and same-revision legacy-wrapper equivalence remain covered.

GREEN: persistence **12/12**, public export API compatibility **1/1**, complete CorelDRAW integration **73/73**, placed-document tests **41/41**, appearance tests **84/84**, warning-filtered tests **33/33**, `corel-import` **8 unit + 1 public API + 2 oracle self-skip**, `coralcraft-cdr` **58 unit + 2 oracle self-skip**, hostile import fuzz **190/190**, and MCP `docs_sync` **2/2** pass. Formatting and focused corel-import/engine Clippy with `-D warnings` pass. Both 2,000-iteration release `probe` and `parse_cdr` runs exit successfully. Self-skipping oracle/sample tests remain non-certifying; Task 10/P3.10, P3, and S2 stay open.

The channel-coverage full CI run passed formatting and workspace Clippy, then stopped only at known-issue 0b: **4,376 passed / 1 failed / 2 ignored**. Every skipped later gate was run serially and passed: doctests, cleanroom, no-bundled-fonts tests/Clippy/install-font checks and both dependency-tree assertions, pdf-deps, assets, brands, cursors, i18n-report, layers, and wasm. Wasm emitted the same four existing UI warnings, not errors. This is not a clean full-CI claim.


## 8. Task 10 metadata-restoration warning coverage

Final review found that early `CoralCraftObjectMeta` restoration returned before appearance, later text-run, and image-colorize warning hooks. Public-API regressions import metadata-restorable path, text, and bitmap objects and then apply Pattern without changing name/bounds/visibility/lock. Before the production change, all three returned no generic-pattern warning; the path's live-trace control already warned.

Detection now runs before early restoration. The restorable path emits one generic-pattern and one live-trace warning while its original solid fill and the old `export_document` result remain unchanged, proving this is warning-only coverage rather than a restoration redesign. Later text-run Pattern fill/stroke and image Pattern colorize also warn once. Existing ordinary/placed/mask/text/colorize channels and negative controls remain green.

GREEN: public export API tests **4/4**, persistence **12/12**, complete CorelDRAW integration **73/73**, placed-document tests **41/41**, appearance tests **84/84**, warning-filtered tests **33/33**, `corel-import` **8 unit + 4 public API + 2 oracle self-skip**, `coralcraft-cdr` **58 unit + 2 oracle self-skip**, hostile import fuzz **190/190**, and MCP `docs_sync` **2/2** pass. Formatting and focused corel-import/engine Clippy with `-D warnings` pass. Both 2,000-iteration release `probe` and `parse_cdr` runs exit successfully.

The early predicate still compares only name/bounds/visibility/lock, so it can restore old appearance after an edit. That separate appearance-restoration limitation is recorded in known issues and remains open; the warning does not claim the edited appearance is preserved. Task 10/P3.10, P3, and S2 stay open.

The metadata-restoration full CI run passed formatting and workspace Clippy, then stopped only at known-issue 0b: **4,376 passed / 1 failed / 2 ignored**. Every skipped later gate was run serially and passed: doctests, cleanroom, no-bundled-fonts tests/Clippy/install-font checks and both dependency-tree assertions, pdf-deps, assets, brands, cursors, i18n-report, layers, and wasm. Wasm emitted the same four existing UI warnings, not errors. This is not a clean full-CI claim.


## 9. Task 10 nested-restoration warning coverage

Final review found two nested bypasses: a restored group whose current kind/child count no longer matched metadata returned without visiting current children, and successful PowerClip restoration skipped the frame. Public-API regressions delete a non-bounds-determining child from a restorable two-child group and apply Pattern to the remaining child; a separate restorable PowerClip receives Pattern and live trace metadata on its frame. Before the fix, both returned `warnings == []` for CDR and CMX.

Structural mismatches now convert current children into a discarded value solely for warning collection. Successful PowerClip restoration inspects the frame separately before converting contents. Children already used by restored output are not converted a second time, restoration mappings remain unchanged, and each category is deduplicated.

GREEN: public export API tests **6/6**, persistence **12/12**, complete CorelDRAW integration **73/73**, placed-document tests **41/41**, appearance tests **84/84**, warning-filtered tests **33/33**, `corel-import` **8 unit + 6 public API + 2 oracle self-skip**, `coralcraft-cdr` **58 unit + 2 oracle self-skip**, hostile import fuzz **190/190**, and MCP `docs_sync` **2/2** pass. Formatting and focused corel-import/engine Clippy with `-D warnings` pass. Both 2,000-iteration release `probe` and `parse_cdr` runs exit successfully.

Compatibility assertions compare legacy wrappers with warning-returning functions at the same revision. This is wrapper equivalence, not independent base-byte proof; no current-HEAD hashes are presented as hashes from `6a84b3dec`. The separate early-restoration appearance limitation remains open. Task 10/P3.10, P3, and S2 stay open.

The nested-restoration full CI run passed formatting and workspace Clippy, then stopped only at known-issue 0b: **4,376 passed / 1 failed / 2 ignored**. Every skipped later gate was run serially and passed: doctests, cleanroom, no-bundled-fonts tests/Clippy/install-font checks and both dependency-tree assertions, pdf-deps, assets, brands, cursors, i18n-report, layers, and wasm. Wasm emitted the same four existing UI warnings, not errors. This is not a clean full-CI claim.


## 10. Task 10 appearance-aware metadata restoration

RED root-cause evidence: `same_bounds_solid_fill_edit_exports_the_current_color` imported a black rectangle, changed only its fill to red, and observed `export_document` restore the original black. Focused regressions also proved stale restoration after same-length text content/first-run edits, later-run style edits, same-bounds path and group-child deletion, image-byte replacement, effect edits, unsupported-container edits, and PowerClip frame/content edits. The old public warning test incorrectly asserted the restored solid fill; it now asserts the current generic-Pattern fallback while retaining every warning assertion.

The fix stores each root projection in the existing `CoralCraftDocData.extra` extension map under `corel.import.original_vc.v1.<node-id>`, populated only by a root wrapper around the three top-level import loops. Nested objects have no sidecar entry, avoiding recursive native-snapshot duplication. Native save/reopen preserves the root projection, while exported CDR extras exclude this internal namespace. Legacy metadata without a sidecar is reconstructed once per top-level subtree through the existing CDR→VC converter with isolated temporary state.

Restoration compares normalized shallow VC shells: local IDs and text layout caches are canonicalized, mask-art IDs are normalized, and main children are cleared only for the shell comparison while container kind/clip remains. Semantic references in extras are not zeroed. Images compare exact encoded bytes from the current document against reconstructed/original expected data, not keys; same-key byte mutation and missing data fail closed, while equivalent bytes under a new key may match. Any unknown semantic difference fails closed. Name/bounds/visibility/lock checks have no unsupported-object wildcards.

Matching groups and unsupported containers deep-clone the original object for residual fields, then always replace children with all converted current children, passing corresponding expected children by stable identity where structure permits; no old-children count fallback remains. This is not a shell-only clone optimization. Legacy reconstruction builds a full temporary metadata map that is discarded, retaining that performance limitation. PowerClips compare outer and frame shells separately, convert the current frame once, preserve original frame/container data only when it matches, and convert every current contents child without a count guard. Existing Pattern/live-trace warning pre-scans and deduplication remain, with no discarded warning-only child conversion.

Public export/write signatures and wrapper behaviour are unchanged. The original metadata literal and `Eq` on both metadata structs are preserved because projections live only in the existing extension map. No new CDR preservation is claimed for later-run styles, image colorize, gradient stop alpha, blend modes, or other fields the converter cannot represent; those cases export current conversion/fallback instead of stale originals, with existing warnings where applicable.

GREEN: metadata restoration **21/21**, `corel-import` **8 unit + 6 public API + 21 restoration + 2 oracle self-skip**, persistence **12/12**, complete CorelDRAW integration **73/73**, placed documents **41/41**, appearance **84/84**, warnings **33/33**, `coralcraft-cdr` **58 + 2 self-skip**, hostile import fuzz **190/190**, both 2,000-iteration release probe/parse runs, formatting, focused Clippy with `-D warnings`, and MCP docs_sync **2/2** pass. Full CI/later serial gates remain for the controller. The combined mesh/transparency/bitmap/residual matrix and S2 remain open; Task 10/P3.10, P3, and S2 stay open.


## 11. Task 10 frame-transparency and missing-bounds review follow-up

Spec review of `358d1d2d0` found two remaining violations. RED: `powerclip_frame_transparency_edit_uses_current_conversion` and `powerclip_frame_transparency_removal_uses_current_conversion_in_restore_and_fallback` both retained the original fountain transparency after the frame mask changed or was removed. `missing_current_bounds_fail_closed_for_original_unknown_fill` restored the original Unknown fill and unsupported payload through the `(None, None)` bounds arm instead of exporting the current gray/path conversion.

The restored-shell and fallback PowerClip paths now propagate converted frame `transparency` alongside kind/fill/outline/transform/opacity/effects. This adds no new opacity-mask conversion: edited or removed mask art exports the converted `None` rather than stale original transparency, while an unchanged transparent PowerClip still restores its exact original transparency. Existing warning collection and deduplication are unchanged.

Missing current bounds now always fail closed, including when expected bounds are also absent. Bounded unchanged objects continue to restore exact native patterns, unknown fills, unsupported payloads, and residuals. A no-bounds unsupported object uses its current conversion instead of restoring original semantics.

GREEN: metadata restoration **25/25**, `corel-import` **8 unit + 6 public API + 25 restoration + 2 oracle self-skip**, persistence **12/12**, complete CorelDRAW integration **73/73**, placed documents **41/41**, appearance **84/84**, warnings **33/33**, formatting, focused Clippy with `-D warnings`, and MCP docs_sync **2/2** pass. Full CI/later serial gates remain for the controller. Later-run styles, colorize, gradient stop alpha, blend modes, edited opacity masks, and other unrepresentable fields remain conversion/fallback limits. The combined mesh/transparency/bitmap/residual matrix and S2 remain open; Task 10/P3.10, P3, and S2 stay open.


## 12. Task 10 nested-cache, frame-state, and source-compatibility review

Quality review of `0f1a8ea8` found three Important issues with failing-first regressions. Nested point-text cache differences rejected unchanged group and unsupported shells in both stored-snapshot and legacy modes, dropping shell-only Unknown/residual data. Frame visible/locked edits were overwritten by outer state in both restored-shell and fallback paths. A downstream-style `CoralCraftObjectMeta { original, page_index, initial_local_bounds }` literal failed to compile because `original_vc` was required, and both public metadata structs lacked `Eq`.

Bounds comparison now recursively clears `TextObject.cached_bounds` and `cached_baselines` in the cloned comparison tree using `Arc::make_mut` for child mutation. Legitimate nested cache changes therefore cannot reject unchanged group/unsupported shells. Visibility/lock precedence is projection-aware: restored outer + changed frame uses converted frame state; changed outer + matching frame uses current outer state; both changed uses current outer state because CDR exposes one container state and cannot represent conflicting values; no reliable expected metadata defaults to current outer. Other frame appearance mappings are unchanged.

Root projections now live in the existing `CoralCraftDocData.extra` extension map under `corel.import.original_vc.v1.<node-id>`. Nested objects have no sidecar entry. The original public struct shape and `Eq` on both metadata structs are restored, and the old-style literal plus `Eq` assertions compile and pass. The sidecar persists through native save/reopen, is filtered from exported CDR extras, and older/superseded documents use the existing isolated reconstruction fallback. Export/write signatures are unchanged.

Native reader compatibility boundary: every snapshot node is still a real serialized node whose ID counts against the shared `with_node_budget` used by native loading (`MAX_READ_NODES`: 4,194,304 on desktop, 524,288 on wasm32). Root-only projections increase the total count, so a document near the cap can be rejected where its pre-snapshot form fit. The budget is unchanged and is not weakened.

GREEN: metadata restoration **32/32**, public API **6/6**, `corel-import` **8 unit + 6 public API + 32 restoration + 2 oracle self-skip**, persistence **12/12**, complete CorelDRAW integration **73/73**, placed documents **41/41**, appearance **84/84**, warnings **33/33**, formatting, focused Clippy with `-D warnings`, and MCP docs_sync **2/2** pass. Full CI/later serial gates remain for the controller. Later-run styles, colorize, gradient stop alpha, blend modes, edited opacity masks, near-budget native documents, and other existing conversion limits remain explicit. Task 10/P3.10, P3, and S2 stay open.


## 13. Task 10 projection-aware PowerClip precedence re-review

Re-review of `6ac1d4ae` found that an unchanged original frame could overwrite current outer visibility/lock. New outer-only, conflicting both-changed, and metadata-free regressions all failed before the fix. An older forced-fallback test changed both outer and frame and initially encoded the wrong frame precedence; it now asserts the approved outer precedence, while the frame-only restored-shell control remains unchanged and green.

Visibility/lock precedence is now explicit:

- Restored outer shell + changed frame: converted current frame wins.
- Changed outer shell + matching frame: current outer wins; original frame cannot overwrite it.
- Both changed: current outer wins because CDR exposes one container state and cannot represent conflicting values.
- No reliable expected metadata: current outer wins by default.

Fallback conversion therefore always assigns current outer visibility/lock, while the restored-shell branch retains converted frame precedence. Fill, outline, transform, opacity, transparency, and effect mappings are unchanged.

The documentation also corrects the prior shell-only implication: matching containers deep-clone the original object for residual fields before replacing children, and legacy reconstruction builds a full temporary metadata map that is discarded. These remain performance costs; no shell-only clone or metadata-free reconstruction optimization is claimed.

GREEN: metadata restoration **35/35**, public API **6/6**, `corel-import` **8 unit + 6 public API + 35 restoration + 2 oracle self-skip**, persistence **12/12**, complete CorelDRAW integration **73/73**, placed documents **41/41**, appearance **84/84**, warnings **33/33**, formatting, focused Clippy with `-D warnings`, and MCP docs_sync **2/2** pass. Final gate at `01d4ab95`: `cargo xtask ci` passed fmt and Clippy, then engine library tests finished **4,376 passed / 1 failed / 2 ignored**, solely known-issue 0b. Every skipped later gate was run serially and passed: doctests, cleanroom, no-bundled-fonts tests/Clippy/install-font checks, pdf-deps, assets, brands, cursors, i18n-report, layers, and wasm. This is not a clean full-CI claim. Existing conversion-fidelity, native-budget, and deep-clone/temporary-metadata performance limits remain. Task 10/P3.10, P3, and S2 stay open.



## 14. Task 10 transparency and bitmap fallback slice

The new conversion warnings are exactly:

- `gradient stop opacity is dropped in CDR/CMX; native gradient stops remain editable`
- `enabled opacity masks are dropped in CDR/CMX; native opacity masks remain editable`
- `unsupported bitmap effects are dropped in CDR/CMX: <sorted, deduplicated IDs>; native effect parameters remain editable`
- `missing image pixels are replaced with opaque white in CDR/CMX; native image bytes and references remain unchanged`
- `undecodable or empty image pixels are replaced with opaque white in CDR/CMX; native image bytes and references remain unchanged`

Gradient alpha is checked on the visible fill/stroke selected by appearance conversion and on text-run and image-colorize paints. Opaque, invisible, and shadowed gradients stay silent. Enabled masks are inspected recursively, but the dropped-mask warning is committed only when exact original restoration does not succeed. An unchanged imported CDR Fountain therefore restores its original transparency with no dropped-mask warning; an edited mask exports converted `transparency: None` and warns, while a disabled mask stays silent. Native gradient stops and mask art remain editable.

The supported bitmap-effect IDs are exactly `stylize.dropShadow`, `path.offsetPath`, `threeD.extrude`, `distort.puckerBloat`, `distort.zigZag`, and `distort.twist`. The converter's existing match is the only support list: visible unmatched IDs are collected in a `BTreeSet` and emitted once in sorted order, including after edited metadata restoration fails. Invisible unsupported effects and the six supported visible IDs do not warn.

Missing image blobs and present-but-undecodable or empty decoded pixels have separate warning categories. Small fallbacks remain opaque white RGBA, valid all-white PNGs do not warn, and export does not mutate native image keys or bytes. Both white-fallback allocation paths use checked `u64` dimension/byte multiplication and `coralcraft_cdr::parse::MAX_BITMAP_PIXELS` (64 Mpx / 256 MiB). Overflow or over-budget fallback dimensions return `CorelImportError::Conversion` through the existing fallible APIs instead of allocating or panicking; this includes `encoded_original_bitmap`.

RED evidence: all eight new public tests failed before the production change—six on missing warning categories and both huge-dimension tests on capacity overflow. The engine gradient/mask test also failed with `warnings: []`; its opaque/disabled and invisible/shadowed controls passed. GREEN: public export/warnings **14/14**, metadata restoration **35/35**, `corel-import` **8 unit + 14 public + 35 restoration + 2 oracle self-skip**, persistence **14/14**, complete CorelDRAW integration **75/75**, placed documents **41/41**, appearance **84/84**, warnings **33/33**, `coralcraft-cdr` **58 + 2 self-skip**, hostile import fuzz **190/190**, both 2,000-iteration release probes, formatting, focused Clippy with `-D warnings`, and MCP docs_sync **2/2** pass. Current stats: **2,048 files, 884,657 lines, 14,756 test attributes, 66 proptest blocks; desktop layer 533,881 lines**. Full CI and later serial gates remain for the controller. Self-skipping oracle/sample checks are not certification. The combined mesh/transparency/bitmap/residual matrix remains open; Task 10/P3.10, P3, and S2 stay open.



## 15. Task 10 transparency/bitmap quality-review follow-ups

RED evidence: the existing stored huge-original-bitmap test passed while the new legacy-reconstruction variant with the original-VC sidecar cleared panicked with capacity overflow before reaching `encoded_original_bitmap`. The item-only public `effect.apply {item: 0}` regression returned `warnings: []`; an initial combined test was corrected because object-level effects masked that gap. The strengthened unchanged-Fountain assertions require completely empty warnings and already passed as an evidence-only guard.

Legacy reconstruction now validates bitmap dimensions through `checked_rgba_len` before calling `convert_cdr_node_to_vc`. On overflow or over-budget dimensions it records the conversion error and returns no expected projection, so current conversion reaches the existing structured `CorelImportError::Conversion` without reaching `raw.resize` or allocating. Stored and legacy huge-dimension tests both pass, and public signatures are unchanged.

Appearance conversion now returns the effect slices for the same last-visible fill/stroke items whose paints it converts. After exact restoration fails, those slices are scanned by the existing six-arm effect conversion alongside `node.appearance.effects`, with no separate allowlist. Item-only effects warn in CDR and CMX; combined object/item IDs remain sorted and deduplicated; invisible effects and shadowed items stay silent. Unchanged direct and writer Fountain exports assert `warnings.is_empty()`.

GREEN: public export/warnings **15/15**, metadata restoration **35/35**, `corel-import` **8 unit + 15 public + 35 restoration + 2 oracle self-skip**, persistence **15/15**, complete CorelDRAW integration **76/76**, placed documents **41/41**, appearance **84/84**, warnings **33/33**, `coralcraft-cdr` **58 + 2 self-skip**, hostile import fuzz **190/190**, both 2,000-iteration release probes, formatting, focused Clippy with `-D warnings`, and MCP docs_sync **2/2** pass. Current stats: **2,048 files, 884,766 lines, 14,758 test attributes, 66 proptest blocks; desktop layer 533,934 lines**. Full CI and later serial gates remain for the controller. Self-skipping oracle/sample checks are not certification. The combined mesh/transparency/bitmap/residual matrix remains open; Task 10/P3.10, P3, and S2 stay open.



## 16. Task 10 nested-bitmap reconstruction review

RED evidence: new legacy regressions placed a `u32::MAX × u32::MAX` bitmap at depth two in an ordinary group and in a PowerClip. Both panicked with capacity overflow because the previous preflight examined only `metadata.original.kind` at the root. Recursive group, unsupported-object, and PowerClip conversion could still reach the unguarded `raw.resize`.

The root-only check is replaced by an iterative complete-original-tree preflight. It visits every ordinary `ObjectKind::Group` child, every `ObjectKind::Unsupported` child, and every `powerclip_children` descendant at arbitrary depth. Each bitmap dimension is checked with `checked_rgba_len`; overflow or over-budget input records the conversion error and returns no expected projection before recursive reconstruction can allocate. Public signatures and stored/root legacy behavior are unchanged, and item-effect/Fountain controls remain green.

GREEN: public export/warnings **17/17**, metadata restoration **35/35**, `corel-import` **8 unit + 17 public + 35 restoration + 2 oracle self-skip**, persistence **15/15**, complete CorelDRAW integration **76/76**, placed documents **41/41**, appearance **84/84**, warnings **33/33**, `coralcraft-cdr` **58 + 2 self-skip**, hostile import fuzz **190/190**, both 2,000-iteration release probes, formatting, focused Clippy with `-D warnings`, and MCP docs_sync **2/2** pass. Current stats: **2,048 files, 884,864 lines, 14,760 test attributes, 66 proptest blocks; desktop layer 533,934 lines**. Self-skipping oracle/sample checks are not certification. The combined mesh/transparency/bitmap/residual matrix remains open; Task 10/P3.10, P3, and S2 stay open.

Final gate at `4745ec3f`: `cargo xtask ci` passed formatting and workspace Clippy, then engine library tests finished **4,376 passed / 1 failed / 2 ignored**, solely known-issue 0b. The complete CorelDRAW integration group passed **76/76**. Every skipped later gate was run serially and passed: doctests, cleanroom, no-bundled-fonts test/Clippy/install-font checks, pdf-deps, assets, brands, cursors, i18n-report, layers, and wasm. Wasm emitted the same four existing UI warnings, not errors. This is not a clean full-CI claim.



## 17. Task 10 mesh-opacity fallback and empty-live fail-closed slice

The new mesh warning is exactly:

`mesh point opacity is dropped in CDR/CMX, so expanded patches are opaque; tessellated patch colours and positions are retained, and native mesh points remain editable`

Before live expansion, conversion checks original `NodeKind::Mesh` descendants for point opacity below 1.0. The warning is committed only after exact original restoration fails, so all-opaque meshes and successful exact restoration stay silent. Direct public `corel-import` model assertions require a non-empty group of ordinary non-empty paths with retained tessellated RGB colour, 10..110 × 20..80 coordinates, and object opacity 1.0. CDR7 and CMX1 warning-returning writers each emit the warning once; legacy wrappers return identical bytes at the same revision. Native mesh data is unchanged.

Any originally live node whose deep expansion produces an empty group now records `CorelImportError::Conversion("live expansion produced an empty object; refusing to export empty CDR/CMX geometry")` instead of writing a successful empty object. This covers invalid/empty meshes, a nested live child, and generation-budget exhaustion through `export_document`, `export_document_with_warnings`, and all existing CDR7/CDRX/CMX1 writers. The new public `vectorcraft_doc::live::live_expansion_failed()` query describes the latest outermost deep expansion on the current thread: it resets at the next outermost expansion, remains sticky across nested failures, and can be queried after expansion returns. Existing expansion signatures and the production `MAX_GENERATED` budget of 4,000,000 are unchanged. A test-local RAII budget override proves the real zero-budget exhaustion branch and restores the previous thread-local state on drop; no override is public.

RED evidence: the doc tests initially did not compile because `live_expansion_failed` was absent; the public mesh test returned `warnings == []` (0 matching warnings instead of 1); both empty-expansion tests returned successful exports instead of `CorelImportError::Conversion`; and the engine persistence test returned only the existing generic live-Mesh expansion warning. The all-opaque and exact-restoration negative controls passed before the fix and remain green.

Spec-review RED follow-up: an ordinary parent with a changed name failed restoration while its nested translucent mesh restored its original shell. The nested output proved restoration with original opacity 0.37 rather than current opacity 1.0 and retained non-empty children, but the parent's pre-scan incorrectly returned `["mesh point opacity is dropped in CDR/CMX, so expanded patches are opaque; tessellated patch colours and positions are retained, and native mesh points remain editable"]`. Source-mesh detection now runs only when `live::is_live(node)` is true, before that live node is replaced by its expansion. Ordinary-group children are handled by their own restoration-dependent conversions, and opacity-mask art is not scanned wholesale; the existing recursive mask conversion remains the only mask path. Warning text and commit-after-local-restoration ordering are unchanged, as is empty-expansion status.

Code-quality RED follow-ups: a changed live parent failed restoration while its nested translucent mesh restored original opacity 0.37 and non-empty output, yet the ancestor's aggregate boolean emitted the mesh warning. An exactly restored owner with enabled translucent-mesh mask art also emitted the warning from the discarded mask pre-scan. Both negative controls failed with the exact warning array; the fallback-owner positive control already emitted one mesh warning and one dropped-mask warning. Source meshes now contribute their unique IDs to a private per-export set, and the category is committed after a node's own exact-restoration early return only when that expanded node's ID is in the set. Live ancestors no longer proxy nested mesh results. Mask pre-scans checkpoint and restore only mesh-opacity warning/source state; conversion errors and all other warning categories remain untouched, and the mask contribution merges only after owner restoration fails. The `live_expansion_failed` API documentation now names both `expand_deep` and `expand_deep_hooks` and the existing reentrant live-evaluation guard without adding another nesting mechanism.

GREEN: `vectorcraft-doc` **406/406**, public export/warnings **26/26**, metadata restoration **35/35**, `corel-import` **8 unit + 26 public + 35 restoration + 2 oracle self-skip**, persistence **16/16**, complete CorelDRAW integration **77/77**, placed documents **41/41**, appearance **84/84**, warning-filtered engine tests **33/33**, `coralcraft-cdr` **58 + 2 self-skip**, hostile import fuzz **190/190**, both 2,000-iteration release probes, formatting, focused doc/corel-import/engine Clippy with `-D warnings`, and MCP docs_sync **2/2** pass. Current stats: **2,048 files, 885,298 lines, 14,772 test attributes, 66 proptest blocks**. Full CI and later serial gates remain for the controller. Self-skipping oracle/sample checks are not independent certification; CDR/CMX reopen proves only ordinary non-empty vector paths without live mesh metadata, not live-mesh restoration. The combined matrix remains open, and this slice does not close Task 10/P3.10, P3, or S2.

## 18. Mesh-opacity attribution closed on expansion provenance (P3 mesh review)

The review of the mesh slice found the original-id attribution blind to generated art: a Repeat
holds its source meshes in its spec (no child conversion ever reaches them), and a Repeat's copies
and a Blend's intermediates carry `NodeId(0)` or cloned ids, so `mesh_point_opacity_sources
.contains(top id)` could only fire when the top node itself was the mesh. A translucent-mesh Repeat
therefore exported silently.

Attribution is now expansion provenance (`live_drops_mesh_point_opacity` in `corel-import`), never
original node ids alone, and generated node ids stay untouched:

- the node itself, when it is a translucent mesh;
- a Repeat's spec-held source art, scanned recursively (nested Repeats included);
- the generated copies of one live evaluation level (`live::expand_live`), recognized by carrying
  no id of the original tree — a Blend's intermediate meshes qualify, while its sources keep their
  ids and convert on their own;
- original child subtrees are deliberately excluded: each child owns its own warning in its own
  fallback, so a restored child never proxy-warns its parent.

New regressions, each observed failing before the fix (the plain Repeat case) and green after:
translucent-mesh Repeat warns exactly once over its expanded opaque copies; the review's exact
Blend scenario — the two source objects restore exactly from the stored original while the
generated intermediates fall back — warns exactly once; a plain group over a translucent mesh
still yields exactly the single child warning; and every existing control (exact/nested/mask
restoration silence, empty-expansion fail-closed, dedup, all-opaque silence) stays green. GREEN
battery: corel-import 8 unit + 30 public export/warnings + 35 restoration + 2 oracle self-skip,
`vectorcraft-doc` 406/406, engine integration 266/266, MCP docs_sync 2/2, fmt, focused Clippy
`-D warnings`. Full CI/serial gates run after this commit; S2 stays open, and this closes the
mesh-opacity attribution question without closing Task 10/P3.10 or P3. Gate (`d18ce9f3b`, 2026-11-14): fmt/clippy ok, tests 4,376/1 (known-issue 0b only, verbatim), all serial gates green — not a clean full-CI pass (0b).

## 19. Task T1 Texture fill — the reusable model: positive/negative matrix (frozen 2026-11-14)

The texture model mirrors the pattern architecture and is not an ordinary pattern relabelled: a
document-level reusable `TextureDef` (name, procedural `kind` with parameters, two colours), a
distinct `Paint::Texture { texture }`, deterministic bounded synthesis for render/export, and
shared-engine commands. Semantics frozen for this slice:

**Positives (each must pass):**

| # | Behaviour |
|---|---|
| P1 | `texture.new` creates a named definition (`kind`, parameters, foreground/background) and `texture.list` reports it with those parameters. |
| P2 | `paint.setTexture` applies the definition to an object; the object's inspected paint is the texture (not a pattern, not a solid). |
| P3 | A second object painted with the same name reuses the ONE definition (definition count does not grow). |
| P4 | Editing the definition (`texture.setProps`) propagates to every user's rendered tiles (both objects' exported tile colours change). |
| P5 | Undo/redo of definition edits and of `paint.setTexture` round-trips exactly. |
| P6 | Native `.vectorcraft` save/reopen keeps the definition and its users; an edit after reopen still propagates. |
| P7 | SVG export renders the synthesized tiles; independent `usvg` reads the tiles' actual geometry and both colours. |
| P8 | PDF export reconstructs the tile art (Hayro-backed read: a page with the tile geometry — structure, not live-PDF claims). |
| P9 | CDR/CMX export emits exactly one explicit texture fallback warning (the synthesized tiles or the named degradation, never a silent default checkerboard). |

**Negatives (each must pass — no silent fallbacks, no pattern/texture confusion):**

| # | Behaviour |
|---|---|
| N1 | `paint.setTexture {texture: "<unknown>"}` errors and changes nothing. |
| N2 | A texture definition is NOT listed by `pattern.list`, and a pattern name is refused by `paint.setTexture`. |
| N3 | Texture paint survives no channel that cannot represent it without the P9 warning (no silent flattening into a plain pattern/solid on CDR/CMX paths). |
| N4 | Invalid definition parameters (unknown kind, out-of-range parameter values) are refused at `texture.new`/`texture.setProps` without document change. |

Out of scope for this slice (follow-ups, not blockers): the Corel foreground/background/kind mapping
depth (Task T2), merge-mode interactions (Task T3), a texture picker UI.

## 20. Task T1 texture-fill model core (delivered) and its open remainder

RED first: all five initial tests failed on `UnknownCommand("texture.new")` (and, after the
commands landed, on the summary-format and SVG-tile assertions). The delivered model core:

- `vectorcraft-doc`: `TextureDef { name, kind: TextureKind (stipple), density 0..1, foreground,
  background }` with deterministic bounded cell synthesis (`tiles`/`tile_nodes`), held in
  `Document.textures`; `Document::texture(name)`.
- `vectorcraft-color`: a distinct `Paint::Texture { texture }` kind (label `texture <name>`) —
  never an ordinary pattern in disguise; every exhaustive `Paint` match across the workspace now
  names it.
- Commands (`crates/engine/src/cmd/texturecmds.rs`): `texture.new`, `texture.setProps` (one undo
  step, duplicate names refused), `texture.list`, `paint.setTexture` (unknown names and pattern
  names refused).
- Export: CDR/CMX emits exactly one explicit fallback warning (`texture fill is synthesized to
  tiles in CDR/CMX; …`); SVG/PDF/EPS/metafile/CAD degrade to the definition's background colour
  with named notes.
- Native `.vectorcraft` reopen keeps the definition and it still edits after reopen.

Matrix status: P1–P3, P5, P6, P9, N1, N2, N4 green (coreldraw_texture 4 passed; negatives refuse
unknown/pattern names and invalid kinds/densities). **Open remainder, kept as two `#[ignore]`-named
anchors runnable with `--ignored` (P4/P7/P8): independent SVG/PDF tile synthesis** — until the
tiles actually render, edit propagation is asserted through `texture.list`, not the export, and
T1 stays open. GREEN battery: doc 406, color 149, corel-import 8+30+35+2, engine CorelDRAW
integration 84, MCP docs_sync 2; fmt and focused Clippy `-D warnings` clean.

## 21. Task T1 closed: the texture tiles render (P4/P7/P8 delivered)

The §20 open remainder landed: a texture-filled path now draws its deterministic tiles in both
independent exporters — the SVG writer emits the shape in the definition's background colour (its
strokes, effects and opacity kept) and the foreground cells over it, clipped to the shape
(`texture_tiles` in `crates/svg/src/export/nodes.rs`); the PDF writer draws the same through
`push_clip_path` (`texture_tiles` in `crates/pdf/src/export/nodes.rs`). The two former `#[ignore]`
anchors are un-ignored and green, and P4 lives in the model test again: an edit of the definition
reaches every user's rendered tiles (the old colour disappears from the SVG, the new one appears),
undo/redo round-trips, and the reopen test's post-reopen edit propagates through the export.
`coreldraw_texture` **6/6**; svg **67+7+446** and pdf suites green; fmt and focused Clippy clean.
Task T1's §19 matrix is fully green — the slice closes subject to the same-reviewer re-review and
the full gate.
