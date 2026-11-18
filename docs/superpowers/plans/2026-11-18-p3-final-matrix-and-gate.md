# P3 Final Matrix and Gate Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Resolve the remaining E6 command-targeting defect, close Task M's combined persistence matrix, and complete the P3 final acceptance, automation, and CI gates without claiming unavailable CDR certification.

**Architecture:** Keep VectorCraft's document, command, render, effect, tracing, and export pipelines. Task M is primarily an evidence-consolidation slice: reuse existing fixtures and add only combined scenarios or minimal fixes proven by failing tests. Do not add another effect model, a second exporter, or a bespoke MCP tool.

**Tech Stack:** Rust, `vectorcraft-doc`, `vectorcraft-engine`, `vectorcraft-effects`, `corel-import`, `coralcraft-cdr`, `vectorcraft-mcp`, `usvg`, Hayro-backed PDF import, and `cargo xtask`.

---

## Review baseline and boundaries

- Current head: `dc9a1c2af`; the P3 worktree is clean and contains T1–T3 and E1–E6.
- Earlier T2 source/art drift and blank-name findings were fixed by `65716a9e5`; do not reopen them without a new failure.
- Fresh review reproduced one remaining E6 defect: with clip A requested through `ids` while clip B is selected, `object.clippingMask.setContentsLocked` returns and edits B. The current implementation reads only `st.selection.objects`.
- Fresh regression battery: Corel engine integration 122/122, CorelDRAW effects UI 15/15, tools 853 passed/1 ignored, document 413/413, MCP docs_sync 2/2. These tests do not cover the E6 `ids` case.
- P3, Task M, Task 13, S2, and P0.1 remain open. Self-skipping oracle/sample checks are not certification. Known-issue 0b is never a clean full-CI pass.

## File map

- E6 command and regression: `crates/engine/src/cmd/object/compound.rs`, `crates/engine/tests/it/coreldraw_powerclip.rs`.
- Combined matrix: `crates/engine/tests/it/coreldraw_p3_persistence.rs`, `crates/corel-import/tests/export_warnings.rs`, `crates/corel-import/tests/metadata_restoration.rs`, and `docs/p3-inventory.md` §30.
- Final scenario: `crates/engine/tests/it/coreldraw_p3_acceptance.rs`.
- MCP coverage: `crates/mcp/src/tests.rs` and `crates/mcp/tests/docs_sync.rs`.
- Status and evidence: `docs/p3-inventory.md`, `docs/gaps.md`, `docs/target-app-parity.md`, `ROADMAP.md`, `docs/ai-memory/03-current.md`, and `docs/superpowers/plans/2026-11-11-p3-signature-effects-image-workflows.md`.

### Task 1: Honor E6's explicit `ids` targets

**Files:**
- Modify: `crates/engine/src/cmd/object/compound.rs`
- Test: `crates/engine/tests/it/coreldraw_powerclip.rs`

- [ ] **Step 1: Add the failing two-clip regression**

Add this test using the file's existing `session`, `rectangle`, and `make_clip` helpers:

```rust
#[test]
fn lock_contents_ids_target_the_requested_clip_not_the_current_selection() {
    let mut s = session();
    let clip_a = make_clip(&mut s, rectangle(&mut s, 0.0, 0.0, 100.0, 80.0), rectangle(&mut s, 25.0, 25.0, 60.0, 50.0));
    let clip_b = make_clip(&mut s, rectangle(&mut s, 150.0, 0.0, 100.0, 80.0), rectangle(&mut s, 175.0, 25.0, 60.0, 50.0));
    s.execute("select.set", &json!({ "ids": [clip_b] })).unwrap();
    let result = s.execute("object.clippingMask.setContentsLocked", &json!({ "ids": [clip_a], "lock": false })).unwrap();
    assert_eq!(result["ids"], json!([clip_a]), "the command targets the explicit id, not the selection");
    assert!(s.doc().unwrap().doc.node(NodeId(clip_a)).unwrap().extras().contents_unlocked);
    assert!(!s.doc().unwrap().doc.node(NodeId(clip_b)).unwrap().extras().contents_unlocked);
}
```

Run:

```bash
cargo test -p vectorcraft-engine --test it coreldraw_powerclip::lock_contents_ids_target_the_requested_clip_not_the_current_selection -- --exact --nocapture
```

Expected RED: the command returns `[clip_b]` and changes B rather than A.

- [ ] **Step 2: Analyze impact and make the minimal targeting fix**

Run GitNexus impact upstream for `clip_set_contents_locked`. Reuse the existing `crate::cmd::patterncmds::roots(s, p)` resolver so `ids` selects explicit existing nodes and omission retains the current-selection behavior. Feed its returned IDs into the existing ancestor-clip loop instead of `st.selection.objects`; preserve deduplication, ancestor lookup, refusal behavior, and one undo step.

- [ ] **Step 3: Verify the fix and surrounding workflows**

Run serially:

```bash
cargo test -p vectorcraft-engine --test it coreldraw_powerclip
cargo test -p vectorcraft-engine --test it coreldraw_
cargo test -p vectorcraft-ui-egui coreldraw_effects
cargo fmt --all -- --check
cargo clippy -p vectorcraft-engine --all-targets -- -D warnings
node .gitnexus/run.cjs detect-changes --scope all --repo .
```

Expected: all tests pass and GitNexus is complete with no partial/truncated result. Commit only the command and regression with `fix: honor PowerClip lock contents ids`.

### Task 2: Freeze and close the Task M combined matrix

**Files:**
- Modify: `docs/p3-inventory.md` (new §30 matrix)
- Test: `crates/engine/tests/it/coreldraw_p3_persistence.rs`
- Test only when a gap requires it: `crates/corel-import/tests/export_warnings.rs`, `crates/corel-import/tests/metadata_restoration.rs`
- Modify only on RED: the owning production module identified by GitNexus impact

- [ ] **Step 1: Freeze the matrix before implementation**

Write §30 with one row per fixture below. Each row must name native editability, direct CDR model assertions, CDR/CMX non-empty-vector evidence, exact warning count, and negative controls. Existing tests may satisfy a row; do not rewrite working production code to make a new test meaningful.

| Fixture | Required native evidence | Required CDR/CMX and negative evidence |
|---|---|---|
| M1 Nested PowerClip, including an unlocked inner clip | Native reopen keeps both clip trees and the unlock flag; frame/content editing still works | Direct model keeps nested clips and non-empty paths; the named relock warning occurs exactly once per export, not once per nested node; locked controls stay silent |
| M2 Translucent mesh inside Repeat and Blend | Native reopen keeps editable mesh sources and Repeat/Blend parameters | Actual expanded fallbacks contain non-empty opaque tessellated paths; mesh-opacity warning occurs exactly once at each real fallback category and never proxy-warns for exactly restored sources or plain groups |
| M3 Gradient + enabled opacity mask | Native reopen keeps gradient stops and editable mask art; a further edit works | Gradient-alpha and enabled-mask warnings each occur exactly once; unchanged exactly-restored Fountain masks and disabled masks stay silent |
| M4 Bitmap + live trace + expanded trace | Native reopen keeps source pixels, live trace parameters, and separately editable expanded paths | Live trace warning occurs once; CDR/CMX has ordinary non-empty vectors and an ordinary source image; expanded traces do not emit the live warning |
| M5 Residuals and restoration failure | Native reopen preserves imported residuals and current edits without stale original appearance | Exact restoration is used only for unchanged semantics; edited appearance/container/bounds paths use current conversion; missing bounds and changed children fail closed with named warnings where applicable |
| M6 Abnormal input and generation budget | No native mutation or history entry on rejected commands | Empty live expansion and generation-budget exhaustion return structured conversion errors through CDR7/CDRX/CMX1; hostile imports return errors rather than panicking |

- [ ] **Step 2: Run existing anchors and add only missing combined assertions**

Run:

```bash
cargo test -p vectorcraft-engine --test it coreldraw_p3_persistence
cargo test -p corel-import
cargo test -p vectorcraft-doc live
cargo test -p vectorcraft-engine import_fuzz
```

For every matrix cell without an anchor, add the smallest combined regression in `coreldraw_p3_persistence.rs` (or the named corel-import suite for public conversion APIs), run it before production changes, and record its actual RED/PASS-first status. Evidence-only tests may pass immediately. Any production fix requires its own observed RED and GitNexus impact analysis. Commit each failing fixture separately with an English conventional-commit message.

- [ ] **Step 3: Run hostile-input and warning-deduplication gates**

Run serially:

```bash
CORALCRAFT_FUZZ_ITERS=2000 cargo run --manifest-path fuzz/Cargo.toml --release --bin probe
CORALCRAFT_FUZZ_ITERS=2000 cargo run --manifest-path fuzz/Cargo.toml --release --bin parse_cdr
cargo test -p vectorcraft-engine --test it coreldraw_
cargo test -p vectorcraft-mcp --test docs_sync
cargo fmt --all -- --check
```

Expected: every command exits 0, all matrix warnings are deduplicated, output is non-empty where required, and no self-skipping check is counted as certification. Record actual results in §30 and commit the matrix/evidence with `test: close the P3 combined persistence matrix`.

### Task 3: Complete the final acceptance and automation surface

**Files:**
- Modify: `crates/engine/tests/it/coreldraw_p3_acceptance.rs`
- Modify: `crates/mcp/src/tests.rs`
- Regenerate only if the live registry changes: `docs/mcp.md`

- [ ] **Step 1: Extend the final poster/traced-image scenario to the delivered P3 surface**

Keep the existing PowerClip + Extrude + Drop Shadow + Blend + live-text + trace workflow, then add Symmetry, Block Shadow, Perspective, a colour lens, E5 handle-set distortion parameters, and an unlocked PowerClip. Use real commands; do not replace the existing rendered-control tests for E1–E6. Save/reopen, revise each source/parameter, and reopen again. Assert editable native sources and parameters, independent `usvg` structure, Hayro-backed PDF structure, non-empty CDR/CMX ordinary vectors, and exact warning categories/counts for every actual fallback.

Run:

```bash
cargo test -p vectorcraft-engine --test it coreldraw_p3_acceptance -- --nocapture
```

Expected: PASS. If the scenario exposes a defect, add a focused RED regression and fix only that root cause before rerunning.

- [ ] **Step 2: Prove every new P3 command through MCP `run_command`**

Add `p3_commands_run_through_run_command` in `crates/mcp/src/tests.rs`, following the adjacent P1/P2 tests. Cover `texture.new`, `texture.setProps`, `texture.list`, `paint.setTexture`, `pattern.twoColor`, `pattern.setTwoColorProps`, `paint.setPattern`, `object.symmetry.create`, `object.symmetry.setProps`, `object.symmetry.info`, `object.symmetry.break`, `object.lens`, and `object.clippingMask.setContentsLocked`. Include valid document mutations, result inspection, at least one invalid-input `isError` result with no mutation, and the explicit-`ids` E6 case from Task 1. Existing generic `effect.apply`/`effect.setParams` commands need no bespoke tools.

Run:

```bash
cargo test -p vectorcraft-mcp p3_commands_run_through_run_command -- --nocapture
cargo test -p vectorcraft-mcp --test docs_sync
```

Expected: PASS with `isError == false` for valid calls and `true` only for intentional invalid calls. Regenerate docs only if the registry actually changed; an unchanged generated document is a valid result.

- [ ] **Step 3: Verify the real CLI catalog and final integration battery**

Run:

```bash
cargo run -p coralcraft-cli -- commands | python3 -c 'import json,sys; c=json.load(sys.stdin); s=str(c); required=["texture.new","pattern.twoColor","object.symmetry.create","object.lens","object.clippingMask.setContentsLocked"]; missing=[x for x in required if x not in s]; assert not missing, missing'
cargo test -p vectorcraft-engine --test it coreldraw_
cargo test -p vectorcraft-ui-egui coreldraw_effects
cargo test -p vectorcraft-tools
cargo test -p vectorcraft-doc
cargo fmt --all -- --check
node .gitnexus/run.cjs detect-changes --scope all --repo .
```

Expected: all commands exit 0 and the graph result is complete. Commit acceptance and automation evidence with `test: complete the P3 final acceptance workflows`.

### Task 4: Run the final gate and close P3 honestly

**Files:**
- Modify: `docs/p3-inventory.md`
- Modify: `docs/gaps.md`
- Modify: `docs/target-app-parity.md`
- Modify: `ROADMAP.md`
- Modify: `docs/ai-memory/03-current.md`
- Modify: `docs/superpowers/plans/2026-11-11-p3-signature-effects-image-workflows.md`
- Include: `docs/superpowers/plans/2026-11-18-p3-final-matrix-and-gate.md`

- [ ] **Step 1: Run full CI on the final tree**

Run:

```bash
cargo xtask ci
```

Record the actual output. Any failure other than known-issue 0b blocks closure. If 0b is the sole failure, record 4,376 (or the actual updated count) passed / 1 failed / 2 ignored and explicitly state that this is not a clean full-CI pass.

- [ ] **Step 2: If and only if known-issue 0b is the sole failure, run every skipped later gate serially**

```bash
cargo test --workspace --doc
cargo xtask cleanroom
cargo test -p vectorcraft-text --no-default-features --test without_bundled_fonts
cargo tree -p vectorcraft-ui-egui --target all -e features,normal -i vectorcraft-text
cargo tree -p vectorcraft-ui-egui --no-default-features --target all -e features,normal -i vectorcraft-text
cargo tree -p vectorcraft-mcp --target all -e features,normal -i vectorcraft-text
cargo tree -p vectorcraft-mcp --no-default-features --target all -e features,normal -i vectorcraft-text
cargo clippy -p vectorcraft-ui-egui --no-default-features --all-targets -- -D warnings
cargo test -p vectorcraft-ui-egui --no-default-features --test install_fonts
cargo xtask pdf-deps
cargo xtask assets
cargo xtask brands
cargo xtask cursors
cargo xtask i18n-report
cargo xtask layers
cargo xtask wasm
```

Expected: all exit 0; default trees contain bundled fonts and no-default trees do not. Record actual counts and warnings.

- [ ] **Step 3: Synchronize evidence and queue state**

Update §30 and add the final-gate section to `docs/p3-inventory.md`. Mark Task M and Task 13 complete only after all required checks pass. Mark P3 complete and P4 next in `ROADMAP.md` and current memory. Close the stale P3.6 row and update PowerClip/live-effect parity rows so they no longer say completed E6 or E1–E4 capabilities are open. Keep S2/P0.1 open, and state that no independent CDR/CMX certification or production-performance claim was made. Run `cargo xtask stats` and update measured values only from its output.

- [ ] **Step 4: Final verification and commit**

Run:

```bash
git diff --check
cargo fmt --all -- --check
cargo test -p vectorcraft-mcp --test docs_sync
node .gitnexus/run.cjs detect-changes --scope all --repo .
git status --short --branch
```

Expected: formatting/docs checks pass, the graph report is complete, and only intentional source/test/document files are present. Commit the final records with `docs: complete the P3 effects and image workflows`. Do not push or merge in this plan.

## Completion criteria

P3 is complete only when the E6 explicit-ID regression is fixed, every M1–M6 row has evidence, the final poster/traced-image scenario includes the delivered P3 features, all new commands pass through MCP/CLI, and the final full/serial gates are recorded honestly. Any unresolved failure becomes a named open follow-up; P3 must not be closed over it. The next product phase is P4 (Colour, Prepress, and Production Output), while S2/P0.1 remain independent blocked certification work.
