# CorelDRAW Reference Adoption Decisions (TraceDraw, P0)

> P0 adoption decision record (2026-11-10). This table is the **decision record** for reusing
> TraceDraw modules in CoralCraft; it is not permission to land any module in P0. Every adoption
> lands in its receiving phase (P1 and later) with the tests named below.

- **Reference:** TraceDraw @ `aac8563a5b4eb8399d86d285f8aa69a6ffc0deee` (version 0.43.0),
  `MIT OR Apache-2.0`, restored by [`scripts/bootstrap-refs.sh`](../scripts/bootstrap-refs.sh) into
  `refs/tracedraw/` (local only, never committed).
- **Source/test audit:** [`docs/ai-memory/05-ref-docs/07-tracedraw.md`](ai-memory/05-ref-docs/07-tracedraw.md)
  (review date 2026-11-10; four pinned suites run, source inspected by area).
- **Standing architecture constraints** (umbrella
  [`specification`](superpowers/specs/2026-11-09-coreldraw-main-application-roadmap-design.md)):
  VectorCraft remains the only application base. CoralCraft keeps VectorCraft documents, VectorCraft
  engine commands, and the VectorCraft renderer (`vello_cpu`). TraceDraw is never adopted as a second
  engine, second document model, second text engine, or second automation stack.

The initial decisions below were changed only where the Task 6 audit provides explicit source/test
evidence for a change. No decision was relaxed; the audit confirms each initial decision and adds two
tightening findings recorded under [Audit findings that constrain this table](#audit-findings-that-constrain-this-table).

## Adoption table

| Module | Initial decision | Required integration boundary | Supporting audit evidence |
|---|---|---|---|
| TraceDraw `cdr-format` containers/RIFF/redirects | Adapt after tests | Keep `coralcraft-cdr` safe-Rust errors/budgets; port missing semantics with notices; compare against libcdr and current CDR tests | [Source inspection — CDR format layer](ai-memory/05-ref-docs/07-tracedraw.md#source-inspection-by-area) (document-model independence confirmed; hostile-input tests `truncated_file_does_not_panic`, `compressed_blocks_stop_at_the_limit`, `deeply_nested_lists_stop_at_the_limit` among the 21 passing `cdr-format` tests) and [pinned test results](ai-memory/05-ref-docs/07-tracedraw.md#pinned-test-results-run-2026-11-10-as-is-no-edits-to-refstracedraw) (`tracedraw-cdr-format`: 21 passed, 0 failed) |
| TraceDraw `wl` and old-version readers | Adapt per version | Add synthetic and operator-owned-sample evidence; do not weaken existing hostile-input limits | [Source inspection — CDR reader/writer mapping](ai-memory/05-ref-docs/07-tracedraw.md#source-inspection-by-area) (writer covers versions 7–27; pre-v6 i16 1/1000-inch units) and [open limitations](ai-memory/05-ref-docs/07-tracedraw.md#open-limitations-affecting-adoption) (version 3/4 text and fill gaps; "assumed" layouts without files behind them) |
| TraceDraw `cdr` document mapping | Behaviour and selective adaptation | Map through `corel-import` into `vectorcraft-doc`; do not introduce TraceDraw `Document` into CoralCraft | [Dependency summary](ai-memory/05-ref-docs/07-tracedraw.md#dependency-summary-and-adoption-boundary) (confirmed: `cdr` maps into TraceDraw's application document model via `tracedraw_core` imports, so only behaviour — never the document type — carries over) and [pinned test results](ai-memory/05-ref-docs/07-tracedraw.md#pinned-test-results-run-2026-11-10-as-is-no-edits-to-refstracedraw) (`tracedraw-cdr`: 299 + 19 + 1 + 2 tests passed) |
| Text records and old code pages | Selective adaptation | Reuse VectorCraft text shaping; add only missing CDR decoding semantics and tests | [Source inspection — text engine](ai-memory/05-ref-docs/07-tracedraw.md#source-inspection-by-area) (old `.cdr` code pages decoded with `encoding_rs` in `crates/cdr/src/text.rs`; `text` installs into `core` through a function pointer — a pattern we do not need because VectorCraft text is first) |
| Fills, outlines, arrowheads, transparency | Selective adaptation | Reuse VectorCraft appearance/render types; preserve editable semantics and independent export checks | [Source inspection — CDR reader/writer mapping](ai-memory/05-ref-docs/07-tracedraw.md#source-inspection-by-area) (`fild`/`outl` decode-encode paths and the writer self-checker `check.rs`) and [open limitations](ai-memory/05-ref-docs/07-tracedraw.md#open-limitations-affecting-adoption) (`ftil` fill transform and pattern transparency still open upstream) |
| PowerClip / clip frame | Adapt behaviour and tests | Reuse VectorCraft clipping/groups/commands; implement full container editing rather than import-only clipping | [Source inspection — clip frames](ai-memory/05-ref-docs/07-tracedraw.md#source-inspection-by-area) (`loda` argument 8005 / `LIST clpt` reading, clip-container writing from version 11, and the edit/extract/lock UI flows in `clip_bar.rs` that define the behaviour to replicate on VectorCraft commands) |
| Live effects | Adapt behaviour and tests per effect | Reuse VectorCraft live-effect/effect-stack infrastructure; do not replace the renderer | [Baked-versus-live conflict, resolved from source](ai-memory/05-ref-docs/07-tracedraw.md#the-baked-versus-live-effects-documentation-conflict-resolved-from-source): the live-effect model (`live.rs` `Effect` stack, render-time `evaluate`, 42 tests; CDR `kept_effects`/`kept_blends` round-trip) **is implemented and test-backed** — TraceDraw's `architecture.md` is the stale document. The audit's adoption consequence is exactly this row's boundary: map effects onto VectorCraft's effect stack, do not port `live.rs` |
| Typography | VectorCraft first | TraceDraw fills only measured Corel-specific gaps; no second text engine | [Dependency summary](ai-memory/05-ref-docs/07-tracedraw.md#dependency-summary-and-adoption-boundary) (CoralCraft keeps the VectorCraft text engine) and [source inspection — text engine](ai-memory/05-ref-docs/07-tracedraw.md#source-inspection-by-area) (fontdb/rustybuzz stack documented as a reference for Corel-specific behaviour only) |
| Bitmap tracing | VectorCraft first | Use `vectorcraft-trace`; TraceDraw is a preset/behaviour reference only | [Source inspection — tracing](ai-memory/05-ref-docs/07-tracedraw.md#source-inspection-by-area) (upstream tracing is application-layer code in `apps/tracedraw/src/{trace,trace_preview}.rs`, not a portable crate, so `vectorcraft-trace` stays the engine) |
| Colour management and PDF/X | VectorCraft first | Reuse VectorCraft ICC/PDF/print; adopt only evidenced Corel-specific behaviour | [Source inspection — colour and print ownership](ai-memory/05-ref-docs/07-tracedraw.md#source-inspection-by-area) (pure-Rust ICC in `core`, Neugebauer press model, print pipeline application-owned in `apps/tracedraw`) and [dependency summary](ai-memory/05-ref-docs/07-tracedraw.md#dependency-summary-and-adoption-boundary) |
| Property Bar and dockers | Behaviour reference | Use `vectorcraft-tools`, `tool.select`, `tool.setOption`, and shared engine commands | [Source inspection — Property Bar behaviour](ai-memory/05-ref-docs/07-tracedraw.md#source-inspection-by-area) and [open limitations](ai-memory/05-ref-docs/07-tracedraw.md#open-limitations-affecting-adoption): the owning `tracedraw --lib` suite **failed locally** (51 failures), so no code adoption from `apps/tracedraw` is permitted at this pin; it stays a behaviour reference |
| MCP and scripting | Behaviour reference | Keep VectorCraft command registry/MCP/plugins; no second automation engine | [Dependency summary](ai-memory/05-ref-docs/07-tracedraw.md#dependency-summary-and-adoption-boundary) and [self-report disclaimer](ai-memory/05-ref-docs/07-tracedraw.md#self-report-disclaimer) (upstream automation feature rows are self-reports, not CoralCraft verification) |

## Audit findings that constrain this table

1. **Live effects are real in TraceDraw.** The documentation conflict (`architecture.md` says baked;
   `features.md` says live) resolves from source and tests in favour of the live model: a shape's
   `effects: Vec<live::Effect>` evaluated at draw/export time, 42 tests in `live.rs`, render-time
   `live::evaluate`, and CDR kept-effect/blend round-trips. This grounds the "adapt behaviour and
   tests per effect" decision but does not widen it — CoralCraft maps CorelDRAW effect semantics onto
   VectorCraft's live-effect/effect-stack infrastructure rather than porting TraceDraw's
   implementation. See the
   [audit section](ai-memory/05-ref-docs/07-tracedraw.md#the-baked-versus-live-effects-documentation-conflict-resolved-from-source).
2. **The TraceDraw application crate is not adoptable at this pin.** `cargo test -p tracedraw --lib`
   failed on this machine (2402 passed, 51 failed, 43 ignored; mostly UI-language assertions plus a
   font-file `NotFound`). Until it passes or the failures are shown to be irrelevant, everything under
   `apps/tracedraw` — Property Bar, dockers, print UI, tracing UI — is behaviour documentation only.
   See the [failed local test bullet](ai-memory/05-ref-docs/07-tracedraw.md#open-limitations-affecting-adoption).

## Licence, notice, and test obligations for adapted code

Every row above whose decision permits adapting code (all "Adapt..." and "Selective adaptation" rows)
carries the same licence/notice obligation, plus row-specific tests in the receiving phase:

- **Licence/notice obligation.** TraceDraw is `MIT OR Apache-2.0`, licence-compatible with CoralCraft.
  Adapted code keeps TraceDraw's upstream copyright notices; every adaptation is recorded in
  [`NOTICE`](../NOTICE) and [`docs/upstream-vectorcraft.md`](upstream-vectorcraft.md). TraceDraw's own
  `AGENTS.md` additionally requires that its clean-room provenance (`CLEANROOM.md`) not be
  misrepresented if format code is copied: its `.cdr` knowledge comes from public format notes,
  contributor-owned files, and the Kaitai Struct public description, with no libcdr (MPL-2.0) code
  ported. Upstream UI assets and presets are never copied wholesale; any asset contribution follows
  CoralCraft's asset-registration rules ([`ASSETS.md`](../ASSETS.md)).
- **Tests required in the receiving phase**, per adopted-code candidate:
  - `cdr-format` containers/RIFF/redirects: synthetic container fixtures and hostile-input
    budget/mutation tests in the `coralcraft-cdr` style; differential comparison against libcdr where
    `cdr2raw`/`cmx2raw` exist; the existing CDR regression suite stays green.
  - `wl` and old-version readers: per-version synthetic fixtures plus operator-owned-sample evidence
    (`refs/cdr-samples`); hostile-input limits must not shrink.
  - `cdr` document mapping: `corel-import` mapping tests against `vectorcraft-doc`, residual
    preservation checks, and the engine CDR/CMX pipeline regression tests.
  - Text records and old code pages: public-spec-derived decoding fixtures for each old code page;
    the VectorCraft shaping suite stays green.
  - Fills, outlines, arrowheads, transparency: editable-semantics round trips plus independent export
    checks (SVG/PDF/EPS), not self-round trips alone.
  - PowerClip / clip frame: engine commands (place inside, edit contents, extract, lock) with
    undo/redo, UI interaction tests, and CDR round-trip with residual preservation.
  - Live effects: per-effect evaluation tests on the VectorCraft effect stack, render/export semantic
    checks, and CDR kept-effect round trips.

Rows marked "VectorCraft first" or "Behaviour reference" adopt no code and therefore carry no notice
obligation beyond citation; their receiving phases still owe the VectorCraft-side tests that prove any
Corel-specific behaviour they take from the reference.

Related records: current capability levels in [`docs/target-app-parity.md`](target-app-parity.md),
current CDR verification state in [`docs/cdr-verification.md`](cdr-verification.md), and the work
queue in [`docs/gaps.md`](gaps.md).
