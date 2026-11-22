# P4 Inventory — Colour, Prepress, and Production Output

> Frozen 2026-11-22 by Task 1 of
> [`docs/superpowers/plans/2026-11-22-p4-colour-prepress-production.md`](superpowers/plans/2026-11-22-p4-colour-prepress-production.md).
> Measured baseline and the frozen positive/negative matrix for the phase. Evidence levels are the
> 0–5 scale of [`docs/target-app-parity.md`](target-app-parity.md) (0 absent, 1 model, 2 command,
> 3 interactive, 4 interoperable, 5 production). Every claim below names the file that proves it;
> nothing is estimated.

## 1. Entry gate (Task 1 Step 1) — BLOCKED on P3 Task 13

- Worktree `.worktrees/p3-signature-effects-image-workflows`, branch
  `p3-signature-effects-image-workflows`, HEAD after this session's planning commit `b99c84332`
  (P4/P5 plans + ROADMAP links; before it `ec4532c6e docs: update current application
  capabilities`). The P4/P5 plan files were committed there first.
- **P3 is not finally closed.** `docs/superpowers/plans/2026-11-19-p3-final-acceptance.md` still
  has 11 unchecked steps (M1/M2 hardening, the extended poster/traced-image acceptance, real MCP
  `run_command` coverage, the full gate) and
  `docs/superpowers/plans/2026-11-18-p3-final-matrix-and-gate.md` has 14; the last P3 code commits
  closed Task M (`docs/p3-inventory.md` §30, 2026-11-19: `coreldraw_p3_persistence` 17/17,
  fuzz gates, "Task 3's acceptance additions are next"). ROADMAP.md still lists P3 as **In
  progress**.
- Therefore, per the P4 entry gate, **only this read-only inventory ran**; no P4 production
  milestone is committed until P3 Task 13 (final acceptance + gate + evidence sync) closes and
  P3 is marked complete in ROADMAP.md.

## 2. Measured baseline (Task 1 Step 2)

Run 2026-11-22 on the P3 worktree tree above (`cargo test` filters exactly as the plan spells
them; `cargo xtask stats` for the tree). Actual outputs:

| Command | Result |
|---|---|
| `cargo test -p vectorcraft-engine pdfx` | exit 0 — engine lib 3 matched / 3 passed / 0 failed (the module's 4th test, `choosing_a_pdfx_standard_…`, doesn't match the word filter) |
| `cargo test -p vectorcraft-engine variables` | exit 0 — engine lib 91 matched / 91 passed / 0 failed (the `tests::variables` module) |
| `cargo test -p vectorcraft-engine tests_batch` | exit 0 — engine lib 16 matched / 16 passed / 0 failed |
| `cargo test -p vectorcraft-ui-egui separations` | exit 0 — 2 matched / 2 passed / 0 failed |
| `cargo test -p vectorcraft-ui-egui tests_print` | exit 0 — 13 matched / 13 passed / 0 failed |
| `cargo xtask stats` | 2,059 files, 890,865 lines, 14,844 `#[test]`, 66 `proptest!` (engine 252,659 lines / 4,927 tests; ui-egui 214,248 / 3,334; pdf 30,349 / 450; render 32,963 / 579; color 9,028 / 149) |

Full logs: `plan/p4-t1-*.log` (git-ignored scratch). Every suite ran on the P3 worktree tree at
HEAD `b99c84332`.

Existing engine test modules behind those filters (function counts from the files):
`crates/engine/src/tests/variables.rs` (99 test fns, 2,735 lines),
`colormgmt.rs` (32), `printpreview.rs` (24), `overprint.rs` (19), `print.rs` (14),
`printtiling.rs` (13), `printadvanced.rs` (10), plus `crates/engine/src/cmd/fileio/tests_pdfx.rs`
(4) and `tests_batch.rs` (19), `crates/engine/tests/color_settings.rs`, and the UI files
`crates/ui-egui/src/dialogs/tests_print.rs`, `tests_print_advanced.rs`, `tests_print_presets.rs`,
`tests_screens.rs`, `panels/separations.rs` (1 in-module test).

## 3. The reusable base (what already exists, by domain)

All of this is synced-VectorCraft infrastructure measured above; P4 adds end-to-end Corel-workflow
evidence, not a second engine.

- **ICC / colour settings / proof colours.** `vectorcraft_color::cms` (profiles by
  `ProfileKind`, `Cms`, `Intent`, 17³ `CmykLut`/`ProofLut`, `PROCESS_PLATES`). Engine commands in
  `crates/engine/src/cmd/colormgmt.rs`: `color.loadProfile` / `removeProfile` / `convert` /
  `gamutCheck` / `plates`, `edit.assignProfile` / `colorSettings`, `edit.colors.overprintBlack`,
  `file.documentColorMode`, `object.convertDocumentColorMode`, `swatch.setSpot`,
  `view.overprintPreview` / `proofColors` / `proofSetup` / `separationsPreview`. A document's
  assigned profiles persist in `Document::color_profiles` and activate on assignment. Soft proof
  and separations rendering in `crates/render/src/proof.rs` (display → proof space → display
  through a cached LUT; Separations Preview recolours the document — process inks
  colour-managed, spot swatch inks at tint, Registration on every plate, placed-image pixels
  separated per pixel; Overprint Preview approximates with Darken).
- **Spot plates.** Spot swatches (`Swatch { global, spot }`, `swatch.setSpot`), spot plates in
  Separations Preview (`spot_colours_get_their_own_plate`), PDF `/Separation` colour spaces
  (`pdf_spot_colour_is_a_separation`), print separations one grey page per printing ink with
  `inks: [{name, print, frequency, angle}]` and `spotsToProcess`
  (`crates/pdf/src/print.rs`). CDR/CMX spot colours round-trip with tint + fallback CMYK
  (`CdrColor::Spot` in `crates/corel-import/src/lib.rs`).
- **Overprint.** Per-fill/stroke/character `FillLayer::overprint`;
  `object.setOverprint` (one undo step, reported by `attributes.info`);
  composite-output `advanced.overprints: preserve|discard|simulate` (`/OP` when preserved);
  `edit.colors.overprintBlack`; preview pixels proven in
  `crates/engine/src/tests/overprint.rs` (cyan stroke overprinting yellow previews green under
  the stroke only). **Gap:** no `overprint` in `crates/cdr` — CDR/CMX export drops overprint
  flags today with no warning (Task 2/6 must make this a named, exact warning or round-trip it).
- **PDF/X-1a:2001.** Preset "PDF/X-1a:2001" (PDF 1.3) in `crates/pdf/src/presets.rs`.
  `crates/pdf/src/pdfx.rs`: `cmyk_only` + `flattens`; `verify` refuses the written file when it
  still has transparency (`/SMask`, `/Transparency`, `/CA`/`/ca` < 1, non-Normal `/BM`), any
  `/DeviceRGB` `/CalRGB` `/CalGray` `/Lab` `/ICCBased` colour, or a `FontDescriptor` without a
  `FontFile`; `/GTS_PDFX` output intent checked by `check` (CMYK profile, or a registered
  printing condition by name; RGB/grey profiles and unknown names refused); colours always
  converted to a CMYK destination and written untagged (`crates/pdf/src/output.rs`); title and
  creation date enforced; `/GTS_PDFXVersion` + `/GTS_PDFXConformance` info entries.
- **PDF/X-3:2002.** Preset (PDF 1.3), `flattens` but not `cmyk_only`: ICC-based (tagged)
  colours permitted, an RGB output intent is allowed, a registry-named condition may stand in
  for an embedded profile. Fonts/embedding/no-encryption rules shared with the X-1a checks.
- **PDF/X-4:2010.** Preset (PDF 1.6): live transparency and layers kept, colours always tagged,
  output intent always embeds its profile; XMP edit writes `pdfxid:GTS_PDFXVersion`,
  `pdf:Trapped`, `xmpMM:VersionID` (`crates/pdf/src/pdfx.rs::xmp`).
- **Page boxes.** TrimBox = the artboard on every page for PDF/X; BleedBox/TrimBox written in
  page space (`crates/pdf/src/print.rs`), `CropTo::{Trim, Bleed}` in `crates/pdf/src/pages.rs`;
  document bleed via `BleedSettings { useDocument, top… }` from `document.setup`.
- **Fonts.** PDF/X `verify` requires a `FontFile` in every `FontDescriptor` (embedded or
  outlines); print `graphics.fonts: none|subset|complete`; fallback-font and overset-type
  warnings in `print.preview` (24 tests in `crates/engine/src/tests/printpreview.rs`).
- **Marks.** `MarkSettings { trim, registration, colorBars, pageInfo, kind: roman|japanese,
  weight, offset }` (`crates/pdf/src/settings.rs:306`), drawn around the artboard at the paper's
  scale for export and print; page information includes the tile and the ink.
- **Print preview / print.** `print.setup` (the full schema: copies/collate/reverse, artboards
  and ranges, `skipBlank`, paper catalogue incl. JIS, orientation/`autoRotate`/`transverse`,
  `printLayers`, `placement`, `scaling: none|fit|custom|tileFull|tileImageable` with
  `scale`/`overlap`/`tileRange`, `margin`, `marks`, `bleed`, `output` incl. separations
  `inks`, `graphics`, `color { intent, preserveNumbers, profile }`, `advanced` incl.
  `printAsBitmap`, `overprints`, `flattenerPreset`, `tileOrigin`) stored with the document as
  one undo step; `print.preview` returns `{pages, sheets (with `transform`/`area`/`trim`),
  tiles, inks, warnings, settings}` without printing; `file.print` writes the print-ready PDF
  (or PostScript via `plan`). Halftone screens/flatness are honestly left to the output device
  with a warning (`crates/pdf/src/print.rs:240`).
- **Imposition / tiling.** Print tiling is the imposition base: `tileFull`/`tileImageable`
  scaling, `overlap`, `tileRange`; `print.tiling` returns the pages in document space,
  `view.printTiling` shows them on canvas, `print.tiling.set` places them (`TileOrigin`, saved
  with the print settings, one undo step) — `crates/engine/src/cmd/printtiling.rs`,
  13 tests. The legacy pre-fork `export_pdf_imposed` stack is not ported; per the plan it stays
  out unless a failing case proves the print tiler cannot express the workflow.
- **Data merge.** `crates/doc/src/variables.rs` + `crates/engine/src/cmd/variablecmds.rs`:
  text/visibility/image/graph variables bound to objects, datasets applied whole in one undo
  step (`dataset.select`/`next`/`prev`/`new`/`set`/`update`/`capture`/`rename`/`delete`/`list`,
  `dataset.import`/`export` CSV with `#`/`@` header prefixes, `variable.loadLibrary` /
  `saveLibrary` XML), overflow warnings, lock semantics, threaded-story handling — 99 tests in
  `crates/engine/src/tests/variables.rs`.
- **Batch export.** `document.exportForScreens` (`crates/engine/src/cmd/fileio/screens.rs`):
  artboards/assets/layers/selection × format rows × sizes, naming patterns, folders by
  row/format/item, per-file or ZIP download, export presets (`save/use/delete/export/import`
  with size limits and refusals) — 19 tests in `tests_batch.rs`.
- **Presets.** Print presets `print.presets.list/save/delete/export/import` (built-in `[Default]`
  protected, user presets in `Prefs::print_presets`); PDF presets `builtin_presets()` (app
  default keeps editability; High Quality Print, Press Quality, Smallest File Size, the three
  PDF/X) plus user presets in app preferences; flattener presets (`flattener.presets.list`).
- **Desktop print path.** `crates/ui-egui/src/print.rs` hands the engine's print-ready PDF to
  the host `PrintService` (desktop spooler / browser dialog), or saves it; `print.printers`
  lists printers. The Print dialog (`crates/ui-egui/src/dialogs/print.rs`) fills
  `print.setup`'s schema through `ui.dialog.set`, previews pages/inks/summary from
  `print.preview`, and drives preset/printer choice; `dialogs/tests_print.rs` runs the dialog
  through a fake print service (4 tests) and `tests_print_advanced.rs` / `tests_print_presets.rs`
  exist beside it.
- **MCP/CLI.** Every engine command above is reachable through MCP `run_command` (the
  `crates/mcp` registry maps tool schemas to engine commands; `docs/mcp.md` stays in sync via
  `crates/mcp/tests/docs_sync.rs`), and through `coralcraft-cli`.

## 4. Frozen P4 matrix (Task 1 Step 3)

One row per required item. "Positive" = the assertions P4 must prove end to end; "Negative" =
refusals/atomicity that must leave document and history unchanged; "Native" = native persistence
that must survive reopen; "Independent" = the non-self evidence; "CDR/CMX" = the exact
degradation/import-export warning behaviour asserted. Start level is measured from §2/§3; the
target for every row is level 4, and 5 for the final scenario row.

| # | Item | Positive | Negative | Native | Independent | CDR/CMX | Start |
|---|---|---|---|---|---|---|---|
| 1 | ICC / proof colours | `color.loadProfile` + `edit.colorSettings` + `view.proofSetup`/`proofColors` cover every rendering intent and BPC; preview pixels change per target (press CMYK, tritanopia) | Unknown profile name, unknown intent, non-finite colour values refused; document + history unchanged | `Document::color_profiles`, colour settings and proof view state survive save/reopen | Pixel evidence through `render` proof path; plate/pixel assertions, not screenshots | Spot/CMYK colours round-trip (`CdrColor::Spot`); profile assignments are VectorCraft-native only — warning expectation: none claimed beyond existing colour warnings | 3 |
| 2 | Spot plates | Named spot swatch prints its own plate (Separations Preview + separations print PDF, `/Separation` with the swatch name); `color.plates` lists it; Registration prints on every plate | Invalid ink name refused by `print.setup` output.inks; unprintable ink excluded | Spot swatch + bindings survive native reopen; plate set identical after reopen | Hayro read of the separations PDF: one page per ink, `/Separation` names, ink order | Spot round-trips with tint + fallback CMYK exactly (existing mapping); unknown import kinds already warn | 3 |
| 3 | Overprint | Overprint fill/stroke/character kept (`object.setOverprint`), preview shows inks underneath (Darken), separations honour overprint; composite `preserve` writes `/OP` | `object.setOverprint` on locked/missing ids refused atomically | Overprint flags per fill/stroke/character survive reopen; undo/redo exact | Hayro read: `/OP` keys in the print PDF; preview pixel assertions | **Gap:** CDR/CMX export currently drops overprint silently — P4 asserts an exact named warning per channel (or adds round-trip) | 3 |
| 4 | PDF/X-1a | Mixed CMYK/RGB/spot/transparency/font doc exports as X-1a: PDF 1.3 header, `/GTS_PDFX` + `/GTS_PDFXVersion`+`/GTS_PDFXConformance`, CMYK/spot-only colour (no `/DeviceRGB`//`ICCBased`), flattened (no `/SMask`//`BM`≠Normal), fonts embedded, TrimBox per page, no encryption, no editing data | RGB output intent, grey profile, unknown profile w/o registry, missing title/date all refused; the *written file* re-checked by `verify` and refused if it breaks the standard | Source document untouched and still editable after export | Hayro structural read of the exported file confirming every claim above | Print/PDF-X channels are export-only; CDR/CMX warnings for dropped live effects already exist — assert exactness | 3 |
| 5 | PDF/X-3 | Same document exports as X-3: PDF 1.3, flattened, ICC-based tagged colours allowed, output intent present (registry name or embedded profile), fonts embedded, no encryption | Same refusal family as X-1a except an RGB output intent is legal; written-file re-check | Source editable | Hayro read: version, flattening, output intent, tagged colour spaces | As row 4 | 3 |
| 6 | PDF/X-4 | Same document exports as X-4: PDF 1.6, live transparency + layers kept, colours tagged, output intent embeds its profile, XMP `pdfxid:GTS_PDFXVersion`/Trapped, fonts embedded, no encryption; `preserveEditing` off unless the standard permits the selected option | Unknown profile always refused (no registry shortcut); written-file re-check | Source editable | Hayro read: transparency present, XMP entries, embedded profile | As row 4 | 3 |
| 7 | Media/trim/bleed boxes | Exact MediaBox/TrimBox/BleedBox geometry for mixed-size artboards + document bleed + custom bleed overrides, in every export and print channel | Out-of-range bleed/paper refused (`72–14400 pt` custom paper) | Document bleed from `document.setup` persists; boxes re-derived identically after reopen | Hayro read of the box arrays; `print.preview` sheet geometry cross-check | Boxes are per-export; CDR page geometry round-trips as artboards (already evidenced) | 3 |
| 8 | Fonts | PDF/X files embed font subsets (`/FontFile` in every descriptor); `graphics.fonts: none|subset|complete` honoured in print; fallback-font and overset warnings from `print.preview` | Missing/unusable font refuses the export (or converts to outlines only when asked), never a silent substitution | Text stays live/editable in the source after export | Hayro read: `FontFile` presence, subset prefixes; text extraction sanity | Text model round-trips (P2); no new CDR font claims | 3 |
| 9 | Marks | Trim/registration/color-bar/page-info marks (roman + japanese kinds, weight/offset) on export and print, at paper scale, outside the trim | Marks with no bleed/offset room refused or warned per existing semantics | Marks are export settings, not document state; presets persist them | Hayro read: mark geometry outside TrimBox; page-info names tile + ink | Marks are export-only; no CDR/CMX channel | 3 |
| 10 | Print preview | `print.preview` pages/sheets/tiles/inks/warnings match the printed bytes: transforms, areas, trim rects, ink `{name, spot, print, frequency, angle}`; portrait/landscape/auto-rotate/transverse; fit/custom/none scaling; margins/imageable area; collate/reverse/skip-blank | Bad settings (unknown media, range, scale) refused by `print.setup` atomically | `print_setup` saved with the document (one undo step) and restored on reopen | Preview cross-checked against the `file.print` PDF pages (page count, order, geometry) | Print setup is VectorCraft-native; CDR/CMX carries none — no warning claimed | 3 |
| 11 | Imposition / tiling | `tileFull`/`tileImageable` with rows/columns/overlap/`tileRange`: sheet count, each artboard's target rectangle, clipping at page bounds, marks, non-empty pages; `print.tiling`/`view.printTiling`/`print.tiling.set` consistent | Impossible tile ranges/overlap refused; `print.tiling.set` on unprintable artboard refused | `tileOrigin` persists with print settings; tiling re-derived identically | Hayro read: every imposed page/tile non-empty, geometry as previewed | As row 10 | 3 |
| 12 | Data merge | Template with text/visibility/linked-image/graph variables + ≥3 rows: `dataset.select` per row exports a distinct PDF (text, visibility, image, graph structure verified); one undo step per row; `dataset.import`/`export` CSV; `variable.loadLibrary`/`saveLibrary` | Invalid row/CSV/import refused with no mutation; missing image is an error; overflow warned | Bindings + datasets survive native reopen; template still bound after the loop | Hayro read of each variant PDF's text/structure; per-row diff proves the merge | Variables are VectorCraft-native; CDR/CMX flattens to current values — assert exact existing warning if any, add named warning if silent | 3 |
| 13 | Batch export | `document.exportForScreens` over all variants × {SVG, PNG, PDF/X}: unique sanitized paths, per-format folders, no overwrites, every file non-empty | Oversize/duplicate/invalid presets refused (existing size-limit tests) | Export settings/presets persist (`document.exportSettings`, export presets in prefs) | Independent read of one output per format (usvg for SVG, PNG decode, Hayro for PDF/X) | Not a CDR/CMX channel | 3 |
| 14 | Presets | Print presets and PDF presets save/list/delete/export/import and are applied by the dialog and by `print.setup {preset}`; `[Default]` protected | Oversize preset imports, unknown keys, duplicate handling refused (existing tests) | User presets persist in preferences across restarts | Preset application observed through real UI + engine settings echo | Not a CDR/CMX channel | 3 |
| 15 | MCP / CLI | Every P4-domain command driven through real MCP `run_command` (colour, overprint, pdfx export, print, tiling, datasets, batch, presets), incl. one invalid-input error with no mutation; `docs/mcp.md` sync test green | Malformed JSON-RPC / unknown command / wrong params → structured errors, no mutation | — | `crates/mcp/tests/docs_sync.rs` + protocol tests | — | 3 |
| 16 | Final production scenario | Two-page doc: CMYK/process + named spot + overprint + embedded/subset font + data variant + mixed sizes → PDF/X-1a, X-3, X-4, separations print PDF, imposed PDF; native reopen still editable; every degraded channel's CDR/CMX warning exact | Any refusal mid-scenario leaves the document unchanged | Full native round-trip of the source document | Independent reads of all five outputs (boxes, fonts, output intents, spot plates, `/OP`, marks, imposition geometry, variants) | Exact warning per degraded channel; never claim native semantics for evaluated geometry | 2 |

## 5. File-map corrections (plan vs tree)

- The plan's file map names `crates/engine/tests/variables.rs`; the variables suite actually
  lives in-module at `crates/engine/src/tests/variables.rs` (99 tests) — Task 5 adds to the
  existing module, not a new `tests/` file.
- Separations UI tests live in-module (`crates/ui-egui/src/panels/separations.rs`, currently 1
  test); Task 2's real-interaction cases (toggle, Alt-click solo, proof-target change) extend it.
- `crates/engine/tests/it/coreldraw_p4_production.rs` and `coreldraw_p4_acceptance.rs` do not
  exist yet (Tasks 2–6 create and register them in `crates/engine/tests/it/main.rs`).
- No `export_pdf_imposed` exists in the current tree (legacy pre-fork only); Task 4 uses print
  tiling as the imposition base per the plan.

## 6. Evidence boundaries

- No local veraPDF / pdfcpu / qpdf / MuPDF / Ghostscript was found during planning, and **veraPDF
  targets PDF/A, not PDF/X** — it must not be used as a PDF/X conformance claim. P4's independent
  evidence is Hayro-backed reads (`vectorcraft_pdf::import` via `hayro-interpret`,
  `crates/pdf/src/lib.rs:25`) plus explicit structural/semantic assertions written by us. Any
  additional third-party preflight tool needs a separate operator decision naming its actual
  PDF/X coverage.
- S2/P0.1 (real CDR certification through libcdr tools + operator samples) remain open
  throughout P4; self-skipping tests and self-round-trips are not certification.
- HPGL, cutter nesting, and a PHOTO-PAINT-class bitmap editor stay out of scope (operator
  constraint) unless a concrete verified use case and public behaviour reference are supplied.
