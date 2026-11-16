# P3 Texture Slice Close-Out Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the current P3 T1 texture-fill slice with verified evidence, integrate the latest main-branch CI fix, and hand off to T2 without expanding scope.

**Architecture:** Keep the existing `TextureDef`/`Paint::Texture` model and the shared raster/SVG/PDF appearance pipelines. This plan adds no feature and no second renderer; it verifies the current candidate, records the actual gate result, and leaves the remaining P3 work in the existing phase plan.

**Tech Stack:** Rust, `vectorcraft-doc`, `vectorcraft-engine`, `vectorcraft-render`, `vectorcraft-svg`, `vectorcraft-pdf`, `vectorcraft-mcp`, `usvg`, Hayro-backed PDF import, and `cargo xtask`.

---

## Context and boundaries

- Work only in `.worktrees/p3-signature-effects-image-workflows` on `p3-signature-effects-image-workflows`.
- The candidate has 11 known modified files. `docs/p3-inventory.md` §21 records focused tests as green but has no completed full-gate result.
- Main is one commit ahead with `c193001b6`, which fixes the FreeBSD thumbnail-test race. Integrate it before the final gate.
- Do not add texture renaming, a texture picker UI, more texture kinds, or performance caching to this close-out. The unreachable rename-looking branch in `texture.setProps` is not a failing workflow and is not a reason to expand T1.
- S2 and P0.1 remain open. Self-skipping oracle/sample tests are not CDR certification.
- Do not stage root-workspace `AGENTS.md`, `CLAUDE.md`, or `.claude/` changes. Do not commit across a failing slice.

## File map

- Existing production candidate: `crates/doc/src/texture.rs`, `crates/engine/src/cmd/texturecmds.rs`, `crates/render/src/paint.rs`, `crates/svg/src/export/{nodes,paint}.rs`, and `crates/pdf/src/export/{nodes,paint}.rs`.
- Existing behavioral evidence: `crates/engine/tests/it/coreldraw_texture.rs`.
- Gate and handoff records: `docs/p3-inventory.md`, `docs/gaps.md`, `docs/ai-memory/03-current.md`, `ROADMAP.md`, and `docs/superpowers/plans/2026-11-11-p3-signature-effects-image-workflows.md`.
- This planning document: `docs/superpowers/plans/2026-11-16-p3-texture-closeout.md`.

### Task 1: Verify and commit the current T1 candidate

**Files:**
- Review: all 11 existing modified T1 files listed by `git status`.
- Modify only on a reproduced failure: the owning production file and `crates/engine/tests/it/coreldraw_texture.rs` or the owning crate's regression test.

- [x] **Step 1: Confirm the candidate boundary and whitespace**

Run:

```bash
git status --short --branch
git diff --check
git diff --stat
```

Expected: the branch is `p3-signature-effects-image-workflows`; `git diff --check` exits 0; only the known T1 production/test/documentation files are modified. Stop if unrelated files appear.

- [x] **Step 2: Run formatting and the complete focused T1 battery**

Run serially:

```bash
cargo fmt --all -- --check
cargo test -p vectorcraft-doc texture
cargo test -p vectorcraft-engine --test it coreldraw_texture -- --nocapture
cargo test -p vectorcraft-svg
cargo test -p vectorcraft-pdf
cargo test -p vectorcraft-render
cargo test -p vectorcraft-mcp --test docs_sync
cargo clippy -p vectorcraft-doc -p vectorcraft-engine -p vectorcraft-svg -p vectorcraft-pdf -p vectorcraft-render --all-targets -- -D warnings
```

Expected: every command exits 0. The texture integration filter contains the strengthened 12-test matrix, and the two document budget tests pass. Record actual counts rather than copying historical totals.

- [x] **Step 3: Handle any failure test-first**

If a command fails, add or strengthen the smallest regression in the owning test file, run it against the pre-fix code to observe the failure, then make the minimal production change and rerun Step 2. Do not commit while any focused check is red. If no command fails, make no production edit.

- [x] **Step 4: Check the graph impact of the complete diff**

Run:

```bash
node .gitnexus/run.cjs detect-changes --scope all --repo .
```

Expected: a complete result with neither `partial: true` nor `truncated: true`. Review every changed symbol and affected process. Re-run if the result is incomplete; do not treat an incomplete zero as a clean check.

- [x] **Step 5: Commit only the T1 implementation candidate**

Run:

```bash
git add crates/doc/src/texture.rs crates/engine/src/cmd/texturecmds.rs crates/engine/tests/it/coreldraw_texture.rs crates/pdf/src/export/nodes.rs crates/pdf/src/export/paint.rs crates/render/src/paint.rs crates/svg/src/export/nodes.rs crates/svg/src/export/paint.rs docs/ai-memory/03-current.md docs/gaps.md docs/p3-inventory.md
git commit -m "fix: complete the texture fill review fixes"
```

Expected: one commit containing only the reviewed T1 candidate. Do not include this new planning file in the implementation commit.

### Task 2: Integrate main before the final gate

**Files:**
- No planned source edits; conflict resolution only if Git reports a conflict.

- [x] **Step 1: Merge the current main branch**

Run:

```bash
git merge main
```

Expected: `c193001b6` becomes an ancestor of the P3 branch. If Git reports conflicts, stop and resolve them according to file ownership; never discard either side wholesale.

- [x] **Step 2: Verify both sides of the integration**

Run:

```bash
git merge-base --is-ancestor c193001b6 HEAD
cargo test -p vectorcraft-engine --test it coreldraw_texture
cargo test -p vectorcraft-ui-egui the_shown_documents_thumbnails_are_made_before_the_queued_ones_of_another
```

Expected: all commands exit 0. The texture matrix remains green and the deterministic thumbnail-race regression passes.

### Task 3: Run the full gate and record an honest T1 handoff

**Files:**
- Modify: `docs/p3-inventory.md`
- Modify: `docs/ai-memory/03-current.md`
- Modify: `docs/gaps.md`
- Modify: `ROADMAP.md`
- Modify: `docs/superpowers/plans/2026-11-11-p3-signature-effects-image-workflows.md`
- Include: `docs/superpowers/plans/2026-11-16-p3-texture-closeout.md`

- [x] **Step 1: Run the standard full CI command**

Run:

```bash
cargo xtask ci
```

Expected: record the command's actual result verbatim. A clean pass closes the gate. On this x86_64 macOS host, known-issue 0b may remain the sole workspace-test failure; that is not a clean full-CI pass and must not be hidden or silenced. Any other failure blocks T1 closure.

- [x] **Step 2: If and only if known-issue 0b is the sole failure, run every skipped later gate serially**

Run in this order:

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

Expected: every command exits 0. Default-feature trees contain `vectorcraft-text feature "bundled-fonts"`; both no-default-feature trees do not. Any other failure blocks T1 closure.

- [x] **Step 3: Update only evidence-backed status**

In `docs/p3-inventory.md` §21, replace "the full gate is running" with the actual commands, counts, known-issue result, and serial-gate outcomes. In the main P3 plan, mark T1 complete and make T2 current only if Steps 1–2 permit closure. In `docs/gaps.md`, keep P3.3 open until T2 and T3 also complete. In `ROADMAP.md` and `docs/ai-memory/03-current.md`, keep P3 in progress and name T2 as next. Leave S2/P0.1 open and keep all CDR/CMX claims limited to explicit fallback evidence.

- [x] **Step 4: Re-check documentation and the final graph diff**

Run:

```bash
git diff --check
cargo fmt --all -- --check
cargo test -p vectorcraft-mcp --test docs_sync
node .gitnexus/run.cjs detect-changes --scope all --repo .
```

Expected: all checks exit 0; the graph report is complete and matches the T1/gate files. No unrelated files or generated build output are staged.

- [x] **Step 5: Commit the gate and planning record**

Run:

```bash
git add docs/p3-inventory.md docs/ai-memory/03-current.md docs/gaps.md ROADMAP.md docs/superpowers/plans/2026-11-11-p3-signature-effects-image-workflows.md docs/superpowers/plans/2026-11-16-p3-texture-closeout.md
git commit -m "docs: record the P3 texture fill gate"
```

Expected: one documentation commit with the actual gate evidence and T2 handoff. Do not mark P3 complete.

## Completion criteria and next queue

This plan is complete when T1 is committed, main is integrated, the full-gate result is recorded honestly, and T2 is the current item in the existing P3 plan.

Continue with `docs/superpowers/plans/2026-11-11-p3-signature-effects-image-workflows.md`: T2 Corel Pattern parameters, T3 merge-mode/combined-transparency matrix, E1–E6 remaining effects/interactions, M combined persistence/warning matrix, then Task 13 final P3 acceptance. Each slice keeps its own failing-test-first, independent-reader, review, and full-gate requirements.
