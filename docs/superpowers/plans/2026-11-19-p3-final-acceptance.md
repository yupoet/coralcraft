# P3 Final Acceptance Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Harden the two remaining Task M evidence gaps, complete the final P3 poster/tracing and MCP/CLI acceptance, and run the final CI/documentation gate before moving to P4.

**Architecture:** Reuse all existing P3 implementations and evidence. This plan adds no feature unless a strengthened regression exposes a defect; any such fix is minimal, test-first, and remains on the VectorCraft document/command/export architecture.

**Tech Stack:** Rust, `vectorcraft-doc`, `vectorcraft-engine`, `corel-import`, `vectorcraft-mcp`, `coralcraft-cli`, `usvg`, Hayro-backed PDF import, and `cargo xtask`.

---

## Current review baseline

- E6 explicit-ID targeting is fixed by `5aeeace6b`; its dual-clip regression correctly covers explicit IDs, selection fallback, inner IDs, and plain-object refusal.
- Task M is implemented by `c7e1ae26b` and `7706af6d2`. Fresh checks pass: PowerClip 5/5, P3 persistence 17/17, Corel integration 124/124, corel-import ordinary suites 8+30+35, MCP docs_sync 2/2, and both 2,000-iteration fuzz gates. Oracle/sample self-skips remain non-certifying.
- M1 currently reopens both CDR and CMX successfully in a real CLI check, but its test wraps import assertions in `if let Ok(...)`, so a future import error would pass silently.
- M2 export fallback tests combine translucent mesh with Repeat/Blend, but native reopen coverage is split between a standalone mesh test and separate live suites. Add the combined native Repeat/Blend mesh evidence before calling M complete.
- P3, S2, and P0.1 remain open. Known-issue 0b is not a clean full-CI pass.

### Task 1: Harden M1 and M2 evidence before final acceptance

**Files:**
- Test: `crates/engine/tests/it/coreldraw_p3_persistence.rs`
- Modify only on a reproduced failure: the owning native Repeat/Blend/mesh persistence module
- Evidence: `docs/p3-inventory.md` §30

- [ ] **Step 1: Make M1 fail on CDR/CMX import errors**

In `nested_powerclips_keep_the_unlock_flag_and_warn_once`, remove the no-op `(0..2).filter(|_| false).count();` and replace the conditional import block with:

```rust
let bytes = vectorcraft_format::base64_decode(out["dataBase64"].as_str().unwrap()).unwrap();
let imported = corel_import::import_cdr_bytes(&bytes).unwrap_or_else(|error| panic!("{format} import failed: {error}"));
let (clips, _, paths) = inspect(&imported.document);
assert!(clips >= 2 && paths >= 3, "{format}: nested clips and non-empty paths survive: {clips} clips, {paths} paths");
```

Run:

```bash
cargo test -p vectorcraft-engine --test it coreldraw_p3_persistence::nested_powerclips_keep_the_unlock_flag_and_warn_once -- --exact --nocapture
```

Expected: PASS on the current tree; any import Err now fails the test. This is test hardening, not evidence of a prior product failure.

- [ ] **Step 2: Add combined native M2 Repeat and Blend regressions**

Add tests in `coreldraw_p3_persistence.rs` that use real commands to:
1. Create a mesh with non-1.0 point opacity, wrap it in a radial Symmetry/Repeat, save and reopen natively, and assert the Repeat source is still the same editable mesh with its point colors, opacity, positions, and Repeat parameters intact.
2. Create two translucent meshes and a Blend, save and reopen natively, and assert the Blend remains live with editable mesh sources and unchanged steps/spine/parameters.
3. Edit a mesh source after each reopen and prove the Repeat/Blend evaluated SVG changes through independent `usvg` parsing; retain the existing §18 CDR/CMX warning and non-empty tessellation anchors.

Run each new test before any production change. If either passes, record it as evidence-only. If it fails, run GitNexus impact upstream for the owning function, add the minimal failing-first fix, and rerun the complete persistence/Corel battery. Do not redesign the live model.

- [ ] **Step 3: Verify and commit the hardened matrix**

Run serially:

```bash
cargo test -p vectorcraft-engine --test it coreldraw_p3_persistence
cargo test -p vectorcraft-engine --test it coreldraw_
cargo test -p corel-import
cargo test -p vectorcraft-mcp --test docs_sync
cargo fmt --all -- --check
node .gitnexus/run.cjs detect-changes --scope all --repo .
```

Expected: all ordinary tests pass, GitNexus is complete, and oracle/sample self-skips are not counted as passes. Update §30 with exact anchors and commit as `test: harden the P3 combined matrix evidence`; use a separate `fix:` commit first if a production defect was found.

### Task 2: Complete final scenario and MCP/CLI acceptance

**Files:**
- Modify: `crates/engine/tests/it/coreldraw_p3_acceptance.rs`
- Modify: `crates/mcp/src/tests.rs`
- Regenerate only if the live registry changes: `docs/mcp.md`

- [ ] **Step 1: Extend the poster/traced-image acceptance**

Retain the existing PowerClip + Extrude + Drop Shadow + Blend + live-text + trace scenario. Add real-command coverage for Symmetry, Block Shadow, Perspective, a colour lens, E5 handle-set distortion values, and an unlocked PowerClip. Perform two native reopens with source/parameter revisions between them. Assert native editability, independent `usvg` structure, Hayro-backed PDF structure, non-empty CDR/CMX ordinary vectors, and exact warning category counts for every actual fallback. Existing rendered-control tests remain authoritative for UI interaction.

Run:

```bash
cargo test -p vectorcraft-engine --test it coreldraw_p3_acceptance -- --nocapture
```

Expected: PASS. Any product failure gets its own focused RED regression and minimal fix before proceeding.

- [ ] **Step 2: Add real MCP `run_command` coverage for every new P3 command**

Add `p3_commands_run_through_run_command` beside the P1/P2 tests in `crates/mcp/src/tests.rs`. Cover:
- `texture.new`, `texture.setProps`, `texture.list`, `paint.setTexture`
- `pattern.twoColor`, `pattern.setTwoColorProps`, `paint.setPattern`
- `object.symmetry.create`, `object.symmetry.setProps`, `object.symmetry.info`, `object.symmetry.break`
- `object.lens`
- `object.clippingMask.setContentsLocked`, including the explicit-ID dual-clip case

Assert valid calls return `isError == false` and mutate the document; include an invalid-input call returning `isError == true` with no document/history mutation. Generic `effect.apply`/`effect.setParams` need no bespoke MCP tools.

Run:

```bash
cargo test -p vectorcraft-mcp p3_commands_run_through_run_command -- --nocapture
cargo test -p vectorcraft-mcp --test docs_sync
```

- [ ] **Step 3: Verify the real CLI and integration battery**

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

Expected: all commands exit 0 and the graph report is complete. Commit with `test: complete the P3 final acceptance workflows`.

### Task 3: Run the final CI gate and synchronize project status

**Files:**
- Modify: `docs/p3-inventory.md`
- Modify: `docs/gaps.md`
- Modify: `docs/target-app-parity.md`
- Modify: `ROADMAP.md`
- Modify: `docs/ai-memory/03-current.md`
- Modify: `docs/superpowers/plans/2026-11-11-p3-signature-effects-image-workflows.md`
- Include: `docs/superpowers/plans/2026-11-19-p3-final-acceptance.md`

- [ ] **Step 1: Run the full gate**

```bash
cargo xtask ci
```

Record actual output. Any failure other than known-issue 0b blocks P3 closure. If 0b is the sole failure, record the actual pass/fail/ignored counts and state explicitly that this is not a clean full-CI pass.

- [ ] **Step 2: If and only if 0b is the sole failure, run all skipped later gates serially**

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

Expected: all exit 0; default feature trees contain bundled fonts and no-default trees do not. Record actual counts and warnings.

- [ ] **Step 3: Update only evidence-backed status**

Mark Task M and Task 13 complete only after all required checks pass. Close stale P3.6 and parity text that still lists completed E1–E6 capabilities as open. Mark P3 complete and P4 next in `ROADMAP.md` and current memory. Keep S2/P0.1 open and avoid any independent CDR/CMX certification or production-performance claim. Run `cargo xtask stats` and copy measured values only from its output.

- [ ] **Step 4: Final verification and commit**

```bash
git diff --check
cargo fmt --all -- --check
cargo test -p vectorcraft-mcp --test docs_sync
node .gitnexus/run.cjs detect-changes --scope all --repo .
git status --short --branch
```

Expected: clean checks, a complete graph report, and only intentional files. Commit final records with `docs: complete the P3 effects and image workflows`. Do not push or merge in this plan.

## Completion criteria

P3 closes only after M1 import failures cannot be hidden by its test, M2 has combined native Repeat/Blend mesh evidence, the final scenario and every new command pass real acceptance, and the full/serial gate is recorded honestly. Otherwise leave the failed item open. The next product phase is P4; S2/P0.1 remain separate blocked certification work.
