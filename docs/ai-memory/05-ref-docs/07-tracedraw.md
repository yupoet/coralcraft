# TraceDraw Reference Audit

- **Pin:** `aac8563a5b4eb8399d86d285f8aa69a6ffc0deee` (version 0.43.0), restored by `scripts/bootstrap-refs.sh` into `refs/tracedraw/` (never committed).
- **Repository:** `https://github.com/tracedraw/tracedraw`
- **Licence:** `MIT OR Apache-2.0` (workspace `Cargo.toml`; each crate ships `LICENSE-MIT` and `LICENSE-APACHE`).
- **Attribution requirement:** adaptation is permitted, but upstream copyright notices must be kept and every adaptation recorded in `NOTICE` and `docs/upstream-vectorcraft.md` (see `refs/REFS.md` section A). TraceDraw's own `AGENTS.md` additionally requires that its clean-room provenance (`CLEANROOM.md`) not be misrepresented if format code is copied: its `.cdr` knowledge comes from public format notes, files its contributors own, and the Kaitai Struct public description; the libcdr documentation was consulted only to confirm layouts and none of its code (MPL 2.0) was ported.
- **Review date:** 2026-11-10 (macOS 25.6.0 x86_64, rustc/cargo 1.96.1). Tests and source inspection were run against the pinned checkout above.

TraceDraw is an independent Rust-only vector editor (egui/eframe desktop app plus WebAssembly build) with its own clean-room `.cdr` reader/writer. Its local rules that matter to us: everything is a `tracedraw_core::Command` applied through an engine; non-test code never panics on file input (opening any file must yield a document); `core` and `cdr-format` have no workspace dependencies; millimetres everywhere in the model; `.cdr` units are 1/254000 inch (= 0.0001 mm).

## Dependency summary and adoption boundary

Workspace dependency direction (strict, from `docs/architecture.md`, confirmed in the crate `Cargo.toml`s):

- `crates/core` owns the document model (`Document` > `Page` > `Layer` > `Shape`), the `Command` enum, the engine (snapshot undo/redo), geometry, style, colour, live effects, ICC, and the press model. It depends on nothing else in the workspace.
- `crates/cdr-format` is the `.cdr` file structure only (RIFF/ZIP containers, chunk tree, `loda` argument decoding, embedded-font EOT, OLE compound files, unknown-record table). Its `Cargo.toml` dependencies are `thiserror`, `log`, `zip`, `flate2`, `tiny-skia` only; `src/lib.rs` states "Nothing here depends on the rest of the workspace", and a grep for `tracedraw_core` in `crates/cdr-format/src/` finds no import. **Confirmed: `cdr-format` is document-model independent.**
- `crates/cdr` depends on `tracedraw-core` and `tracedraw-cdr-format`; `parse.rs` imports `tracedraw_core::{Document, document::{...}, geometry, style, ...}` and maps chunks into the core document best effort. **Confirmed: `cdr` maps into TraceDraw's application document model.**
- `crates/render` is a CPU rasteriser on `tiny-skia` (single `lib.rs`, ~90 test functions inline); `crates/text` depends on `core` only and installs itself into core through a function pointer; `crates/io` (native/SVG/PDF) depends on `core` and `render`; `tracecut` (plotter cutting/nesting) depends on `core` and `serde`; egui stays in `apps/`.

**CoralCraft does not adopt this stack:** we keep VectorCraft documents, VectorCraft engine commands, and the VectorCraft renderer (`crates/render` uses `vello_cpu`), per `docs/coreldraw-reference-adoption.md`. TraceDraw modules are behaviour references or selective-adaptation sources, never a second engine.

## Pinned test results (run 2026-11-10, as-is, no edits to refs/tracedraw)

Platform: macOS 25.6.0 x86_64, cargo 1.96.1. Commands run from the CoralCraft repository root against the pinned checkout. Verbatim result summaries:

1. `cargo test --manifest-path refs/tracedraw/Cargo.toml -p tracedraw-cdr-format`
   - Exit status: 0
   - `running 21 tests` ... `test result: ok. 21 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s`
   - Doc-tests: `running 0 tests` ... `test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s`

2. `cargo test --manifest-path refs/tracedraw/Cargo.toml -p tracedraw-cdr`
   - Exit status: 0
   - lib unit tests: `running 299 tests` ... `test result: ok. 299 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.71s`
   - `tests/conformance.rs`: `running 19 tests` ... `test result: ok. 19 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.23s`
   - `tests/paragraph_shadow.rs`: `running 1 test` ... `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.09s`
   - `tests/paragraph_tabs.rs`: `running 2 tests` ... `test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s`
   - Doc-tests: `running 0 tests` ... `test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s`

3. `cargo test --manifest-path refs/tracedraw/Cargo.toml -p tracedraw-core --lib`
   - Exit status: 0
   - `running 484 tests` ... `test result: ok. 483 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 1.85s`

4. `cargo test --manifest-path refs/tracedraw/Cargo.toml -p tracedraw --lib`
   - Exit status: 101 (**FAILED**)
   - `running 2496 tests` ... `test result: FAILED. 2402 passed; 51 failed; 43 ignored; 0 measured; 0 filtered out; finished in 263.56s`
   - `error: test failed, to rerun pass `-p tracedraw --lib``
   - 51 failed tests, all in the application crate (`apps/tracedraw`): `pick_drag_tests::*` (14), `ui::*` (26), `package::*` (2), `export_assets::*` (2), `time_tracker`/`ui::time_tracker` (4), and one each in `app`, `content_link`, `text_editing`.
   - Observed failure pattern (our reading of the panic output, not an upstream statement): the overwhelming majority are UI-string assertions expecting English while the app under test resolved a Chinese interface language on this machine, for example `left: ["Flyer/页 1.svg"]` vs `right: ["Flyer/Page 1.svg"]` in `export_assets`, the Chinese browser notice printed by `app::tests::the_browser_notice_tells_what_fonts_there_are`, and missing English status strings such as `"text content linked with 1 more"` (`content_link`). A few look environment-dependent instead (`package::tests::a_package_holds_the_drawing_its_pictures_fonts_and_a_report` fails on a font file `NotFound`). Per the P0 plan this failure blocks adoption of the owning application-crate module (the egui app UI) and does not fail this task; the suite was not re-run with a modified environment and nothing in `refs/tracedraw` was edited.

Interpretation for adoption: the three crates that matter as adaptation candidates (`cdr-format`, `cdr`, `core`) pass their pinned suites locally. The failing suite owns the egui application UI, which CoralCraft uses only as a behaviour reference anyway (never as code to adopt).

## Source inspection by area

Files inspected under the pinned checkout (implementation plus inline tests):

- **CDR format layer (container/RIFF/redirects):** `crates/cdr-format/src/{lib,container,riff,loda,raw,eot,ole}.rs`; tests inline in each file (the 21 passing tests above, including `version_16_redirects_resolve_into_external_streams`, `cmpr_lists_use_cpng_blocks_and_a_sizes_pool`, `truncated_file_does_not_panic`, `compressed_blocks_stop_at_the_limit`, `deeply_nested_lists_stop_at_the_limit`). Decompression-bomb ceilings are TraceDraw's own limits (members inflate to at most 512 MB, 1 GB per file, 3 GB external data), noted in `docs/cdr-format.md` "Containers".
- **CDR reader/writer mapping:** `crates/cdr/src/{lib,parse,write,write/,text,cmx,wl,blend_record,kept_blends,kept_effects,shadow_image,spot,check,container}.rs` plus `document_styles.json`; integration tests `crates/cdr/tests/{conformance,paragraph_shadow,paragraph_tabs}.rs`. `check.rs` is a self-checker the writer's tests run on every file they produce. The writer covers versions 7 to 27 (containers: RIFF to v13, ZIP `content/riffData.cdr` for 14-15, ZIP `root.dat` + redirects from 16).
- **Document model and commands:** `crates/core/src/{document,command,engine,id,geometry,nodes,shaping,style}.rs`. Document types: `Document` > `Page` > `Layer` > `Shape`; `ShapeKind` = Rect, Ellipse, Polygon, Path, Text (artistic or framed paragraph, optional on-path), Group, ClipFrame, Table, SymbolInstance, Bitmap. Ids come from `IdSource` and are never reused; a second `Uid` (128-bit) gives objects identity across saves and drawings.
- **Coordinate system:** model units are millimetres, Y up, origin at the page's bottom-left corner (`docs/architecture.md`). `.cdr` files use i32 1/254000 inch (i16 1/1000 inch before version 6) with the page origin at the page centre and objects stored front to back; `crates/cdr/src/parse.rs` shifts the origin so the page's bottom-left is (0, 0) and reverses object/layer order into bottom-to-top.
- **Live effects:** `crates/core/src/live.rs` (the `Effect` enum, `evaluate`), `effects.rs` (static/baked geometry helpers), `distort.rs`, `envelope.rs`, `extrude.rs`, `extrude3d.rs`, `shadow.rs`, `painterly.rs`; rendering side `crates/render/src/lib.rs` (`live::evaluate` call at the shape-draw site); CDR round-trip side `crates/cdr/src/{kept_effects,kept_blends,blend_record}.rs`. 42 tests inline in `live.rs`, 94 test functions in `render/src/lib.rs` (including lens and effect rendering tests).
- **Clip frames (PowerClip analogue):** `ShapeKind::ClipFrame { frame, contents }` in `crates/core/src/document.rs`; render clips `contents` to the `frame` object's outline through a mask (`crates/render/src/lib.rs`); CDR side reads `loda` argument 8005 naming a `LIST clpt` group by `spnd` and cropped bitmaps as clip frames with the crop outline as frame (`crates/cdr/src/parse.rs`); the writer emits clip containers from version 11, a clip frame holding one bitmap as that bitmap with the frame as its crop path, other clip frames as a group of the frame outline plus contents (`crates/cdr/src/write.rs`). UI editing flows (edit in place, extract, lock) live in `apps/tracedraw/src/ui/clip_bar.rs` and the interaction code.
- **Text engine:** `crates/text/src/{lib,catalog,installed,hyphen,traits}.rs` plus `devanagari_tests.rs` and `tamil_tests.rs`. `fontdb` discovers system fonts, `rustybuzz` shapes runs, `ttf-parser` delivers outlines; the crate installs itself into core through a function pointer (`tracedraw_core::document::text_outline::set`, called by `text::install()`), so `core` computes text bounds without depending on fonts. Old `.cdr` code pages are decoded with `encoding_rs` (`crates/cdr/src/text.rs`).
- **Colour and print ownership:** colour model and conversions in `crates/core/src/{color,color_convert}.rs`; ICC v2/v4 profile reading and transforms in pure Rust in `crates/core/src/icc.rs`; the built-in CMYK press model (Yule-Nielsen modified Neugebauer) in `crates/core/src/press.rs`; spot separations in `picture_inks.rs`; a drawing's own CMYK profile is carried from `.cdr` (`color/profiles/cmyk/*.icc`). Print/imposition UI and pipeline are application-owned: `apps/tracedraw/src/{export,imposition}.rs`, `apps/tracedraw/src/ui/{print_preview,print_merge_docker,color_management,convert_colors,preflight_dialog,publish_pdf}.rs`; PDF/PDF-X writing is `crates/io`.
- **Tracing:** bitmap tracing is application-layer code, `apps/tracedraw/src/{trace,trace_preview}.rs` (not a crate); the separate `crates/tracecut` is plotter cutting/nesting geometry. CoralCraft keeps `vectorcraft-trace` as the engine.
- **Property Bar behaviour:** `apps/tracedraw/src/ui/{propbar,bars,bar_items,context}.rs` build the stacked-field property bar (page, object, outline, zoom bars) and per-tool bars (`contour_bar.rs`, `distort_bar.rs`, `extrude_bar.rs`, `envelope_ui.rs`, `clip_bar.rs`, `transparency_docker.rs`, ...); the workspace layout is described in `apps/tracedraw/src/ui/mod.rs`. The app UI is behaviour-reference only for us (its lib test suite failed locally, see above).

## The baked-versus-live effects documentation conflict, resolved from source

- `docs/architecture.md` (core section) says of `effects.rs`: "blend, contour, extrude, distort, brushes. These bake their result today; live effects are a planned change to the model (an `Effect` list on the shape, re-evaluated on render)."
- `docs/features.md` section 6 instead marks drop shadow, contour, blend, distort, envelope, extrude, bevel, block shadow, symmetry and lens as "works" and repeatedly "live", and `docs/blueprint-gaps.md` (Automation) lists a scriptable `Effects` object model.

**Conclusion from source and tests: the live-effect model is implemented; `docs/architecture.md` is stale.** Evidence:

- `crates/core/src/document.rs` (Shape): `pub effects: Vec<crate::live::Effect>` documented as "Live effects, evaluated at render time (see `live.rs`)".
- `crates/core/src/live.rs` module doc: "Live (non-destructive) effects. A shape carries a stack of `Effect`s that are evaluated whenever it is drawn or exported; the source geometry is never changed." The `Effect` enum has Contour, Blend, Distort, Envelope, Perspective, Extrude, Bevel, BlockShadow, Symmetry, Lens, Transparency.
- `crates/render/src/lib.rs` calls `tracedraw_core::live::evaluate(shape)` when drawing a shape, with lens/transparency handled as pixel operations; 42 tests in `live.rs` and the lens/effect tests in `render` exercise the evaluation.
- `crates/cdr/src/kept_effects.rs` writes a private package member (version 14+) so objects saved with live effects open live again (#1031); `blend_record.rs`/`kept_blends.rs` read and keep live blends (#1192).
- `crates/core/src/effects.rs` itself still is, and is documented as, the static/baked path: "All work on flattened polylines and return new paths; the results are baked, not kept live." Both documents agree about that file; only architecture.md's "planned change" wording lags behind `live.rs`.

Adoption consequence: live-effect behaviour in TraceDraw is real and test-backed, but CoralCraft still maps effects onto VectorCraft's live-effect/effect-stack infrastructure rather than porting `live.rs`.

## Self-report disclaimer

TraceDraw's feature counts and statuses (`docs/features.md`: "works 192, partial 7, missing 1, not applicable 4"; every "works" row) are upstream self-reports tied to TraceDraw's own tests and corpora. They are not CoralCraft verification. Only the four suites recorded above were run locally, and only the `cdr-format`, `cdr`, and `core` crates passed. No CDR sample corpus of ours was run against TraceDraw's reader in this audit.

## Open limitations affecting adoption

From `docs/blueprint-gaps.md` and `docs/cdr-format.md` at the pin:

- `.cdr` reader still open: splines (object 0x26 seen only as connector lines), PostScript (6) fills, envelopes of version 3 text, version 3 fill types 6/7/10, text of version 4, tabs/bullets/drop caps from `stlt` (tables skipped by size), the `ftil` fill transform, **lenses other than invert** and **other effects**, fonts embedded with MicroType Express compression. Layouts marked "assumed" in `docs/cdr-format.md` (`bmpf`, `arrw`, version 16+ style-string content, opacity direction, alignment codes) have no file behind them.
- `.cdr` content marked "partial" in `features.md`: splines, PostScript fills, pattern transparency, tabs/bullets/drop caps from style tables, lenses other than invert, effects (effects are saved as what they draw plus TraceDraw's private member, not as native effect records).
- CMX: import lacks lenses and bitmap fills; there is no public specification (sample-file derived). CMX export does not exist. Other missing imports: DWG (proprietary), CGM.
- PostScript fills (legacy) show a stand-in pattern; the procedures are named by number only.
- TraceDraw feature rows still partial: `.cdr` content coverage, CMX import, print preview, data merge, colour styles (no live link), Find and Replace (no search by effects), the Android app; system windows missing.
- Writing versions 7 to 14 cannot be validated against current releases of the writing program (per `docs/cdr-format.md` "Writing", from a public compatibility page, unverified here); paragraph text is written as artistic lines by default because paragraph frames hung another program's 27.2 release (#1698).
- **Failed local test:** `cargo test --manifest-path refs/tracedraw/Cargo.toml -p tracedraw --lib` FAILED on this machine (2402 passed, 51 failed, 43 ignored; see above). Until it passes (or the failures are shown to be purely environmental and irrelevant), no code from `apps/tracedraw` may be adopted; it stays a behaviour reference.
- Oracle gap on our side: this audit ran TraceDraw's own tests only. TraceDraw's `.cdr` reader was not differentially compared against libcdr or our `corel-import` here; that comparison belongs to the adopting phases.
