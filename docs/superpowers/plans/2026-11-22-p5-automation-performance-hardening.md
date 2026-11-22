# P5 Automation, Performance, and Product Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prove representative documents through sustained editing, automation, save/reopen, crash recovery, export, and native platform installation, with measured performance baselines and evidence-based optimization.

**Architecture:** Reuse the existing `coralcraft-cli perf` harness, bounded document/render pipelines, Data Recovery store, MCP/command batch and WASM plug-in sandbox, and native packaging workflows. Measure before optimizing; do not introduce a second benchmark framework, scripting runtime, recovery format, or hand-rolled package format.

**Tech Stack:** Rust, `coralcraft-cli perf`, `vectorcraft-engine`, `vectorcraft-render`, `vectorcraft-mcp`, `vectorcraft-plugins`, Data Recovery, GitHub Actions, native platform package tools, and `cargo xtask`.

---

## Entry gate and evidence boundaries

- **Prerequisite:** P4 must be complete and committed. Before that, only the read-only baseline inventory may run.
- Existing infrastructure already includes a synthetic performance harness, deep memory/performance probes, Data Recovery with native documents and separate blobs, crash-start GPU tracking, MCP `run_command`/`command_batch`, a WASM plug-in sandbox, and native CI packaging/install jobs.
- Performance numbers are valid only for the named document, command, platform, toolchain, and run. Establish a baseline before setting thresholds; no unmeasured number is an existing result.
- Use synthetic fixtures in the repository and operator-owned local documents only. Never commit `.cdr`/`.cmx` samples, proprietary documents, fonts, or unregistered assets. Record fixture shape, dimensions, object/effect counts, and SHA-256 for local-only corpora without copying the documents into the repository.
- Do not use a proprietary application as an oracle, and do not hand-roll DMG/PKG/RPM/Flatpak/AppImage formats. Install and launch packages on their actual target OS.
- S2/P0.1 remain open until real libcdr tools and operator samples exist. P5 must not label those checks as passing.
- Every production change starts with an observed failing test or a measured regression, followed by GitNexus impact analysis. Run spec and quality reviews per task and require complete `detect_changes` before committing.

## File map

- Inventory/evidence: `docs/p5-inventory.md`, `docs/gaps.md`, `docs/target-app-parity.md`, `ROADMAP.md`, `docs/ai-memory/03-current.md`, `docs/ai-memory/04-known-issues.md`.
- Performance: `apps/coralcraft-cli/src/perf.rs` and the `bench` subcommand in `apps/coralcraft-cli/src/main.rs`, `crates/render/src/`, `crates/doc/src/live/expand.rs`, `crates/corel-import/src/lib.rs`.
- Sustained behavior: `crates/engine/tests/it/coreldraw_p5_hardening.rs`, registered in `crates/engine/tests/it/main.rs`.
- Recovery: `crates/engine/src/cmd/recovery.rs`, `apps/coralcraft/src/gpu.rs`, `apps/coralcraft/src/prefs.rs`, `crates/ui-egui/src/tests/recovery.rs`.
- Automation: `crates/mcp/src/tests.rs`, `crates/mcp/tests/protocol_props.rs`, `crates/plugins/src/`, `docs/mcp.md` only if the registry changes.
- Platform: `.github/workflows/{ci,release,freebsd,windows-arm64}.yml`, `packaging/macos/package.sh`, `packaging/linux/package.sh`, `packaging/windows/`, `packaging/web/`, `packaging/check-app-specific.sh`.

### Task 1: Establish representative projects and the performance baseline

**Files:**
- Create: `docs/p5-inventory.md`
- Modify only after baseline data exists: `apps/coralcraft-cli/src/perf.rs`

- [ ] **Step 1: Verify P4 and define the representative document set**

Confirm P4 is marked complete. Define three reproducible project classes in `docs/p5-inventory.md`:
1. A many-object vector illustration with selection, guides, nodes, symbols, and text.
2. A multipage brochure with linked text, master layers, tables, images, and native/CDR residuals.
3. A poster with nested PowerClip, live effects, mesh/pattern/texture, transparency, trace results, and P4 production settings.

Record object counts, page counts, anchor counts, bitmap dimensions, effect counts, source hashes, and which scenarios are synthetic versus operator-owned local files. Do not commit restricted source documents.

- [ ] **Step 2: Capture the current baseline before optimizing**

Start small and serially; do not run deep memory probes concurrently with other builds:

```bash
cargo run --release -p coralcraft-cli -- perf --paths 10000
cargo test -p vectorcraft-engine --test it coreldraw_
```

Then, only when sufficient memory is available, run `cargo run --release -p coralcraft-cli -- perf --deep` and record render, hit test, marquee, guide collection, selection drag, whole-selection edit/undo/redo, live effects, text editing, image import, save/reopen, memory, and output hashes. If the machine is busy, mark the run noisy and do not use it as a gate.

- [ ] **Step 3: Freeze scenario and budget tables**

For each project class, record startup/open, idle render, pointer drag, selection edit, native save/reopen, SVG/PDF/CDR export, memory, and recovery-save measurements. Set regression thresholds only from repeated stable baselines with variance recorded. Commit the baseline with `docs: establish the P5 performance baseline`.

### Task 2: Optimize only measured hot paths

**Files:**
- Modify: `apps/coralcraft-cli/src/perf.rs`
- Modify only with a measured bottleneck: the owning function in `crates/render`, `crates/doc`, `crates/corel-import`, or `crates/engine`
- Test: the owning crate's existing regression suite plus `coreldraw_p5_hardening.rs`

- [ ] **Step 1: Add a failing or measured-regression benchmark**

Select the largest measured bottleneck, not a guessed one. Candidate areas from existing documented costs are live expansion, large-document selection/hit testing, deep-copy/native snapshots, and legacy CDR projection reconstruction. Add a deterministic benchmark that demonstrates the issue and keep output hashes unchanged.

- [ ] **Step 2: Make the smallest semantics-preserving optimization**

Run GitNexus impact first. Reuse existing caches, copy-on-write structures, culling, and bounded pipelines. Do not weaken node/decompression/bitmap budgets, change visible output, or remove warnings. Require an independent SVG/PDF structure check and native-reopen equivalence where relevant.

- [ ] **Step 3: Prove the improvement and regressions**

Run the same benchmark at least three times, report median and variance, and require improvement beyond observed noise. Run the owning crate suite and the full Corel integration battery. If there is no meaningful improvement, revert rather than retaining complexity. Commit each validated optimization separately with an English `perf:` message and the measured result.

### Task 3: Prove sustained editing and bounded-resource behaviour

**Files:**
- Create: `crates/engine/tests/it/coreldraw_p5_hardening.rs`
- Modify only on RED: the owning production module

- [ ] **Step 1: Add sustained-editing scenarios**

For each representative document, perform hundreds of mixed edit/undo/redo cycles: selection, node edits, text changes, effects, page reorder, symbols, pattern/texture changes, and native save/reopen. Assert stable IDs, valid selection, exact history depth policy, unchanged native semantics, and deterministic final output. Keep runtime bounded and put long soak tests behind an explicit opt-in environment flag rather than slowing every CI run.

- [ ] **Step 2: Verify hostile and near-budget inputs**

Exercise maximum practical object counts, deep nesting, empty live expansion, corrupt/missing images, huge dimensions, and generation-budget exhaustion. Assert structured errors, no panic, no unbounded allocation, no silent empty export, and no mutation/history entry after rejection. Run:

```bash
cargo test -p vectorcraft-engine --test it coreldraw_p5_hardening -- --nocapture
cargo test -p vectorcraft-engine import_fuzz
CORALCRAFT_FUZZ_ITERS=2000 cargo run --manifest-path fuzz/Cargo.toml --release --bin probe
CORALCRAFT_FUZZ_ITERS=2000 cargo run --manifest-path fuzz/Cargo.toml --release --bin parse_cdr
```

Expected: all exit 0. Commit with `test: prove P5 sustained editing and bounded resources`.

### Task 4: Verify crash recovery through real process restart

**Files:**
- Test: `crates/engine/src/cmd/recovery.rs`, `crates/ui-egui/src/tests/recovery.rs`, and `coreldraw_p5_hardening.rs`
- Modify only on RED: `crates/engine/src/cmd/recovery.rs`, `apps/coralcraft/src/prefs.rs`, `apps/coralcraft/src/gpu.rs`

- [ ] **Step 1: Exercise every recovery-store contract**

Test modified native documents with images and CoralCraft extension data, save/restore/discard, per-app area ownership, stale heartbeats, competing processes, changed blobs, and old-format copies. Assert the original file is never overwritten during restore and restored documents remain modified with their original path available for Save As.

- [ ] **Step 2: Run a real crash/restart acceptance**

Use a temporary preferences/recovery directory and spawn a test application process with a representative modified document. Terminate that child process, start a fresh one, and verify recovery list/restore, text/effects/images/page data, and subsequent native save. Do not kill the developer's running app or use the normal preferences directory. Repeat on each supported native CI platform where the harness exists; record actual platforms rather than claiming all.

Run:

```bash
cargo test -p vectorcraft-engine recovery
cargo test -p vectorcraft-ui-egui recovery
cargo test -p coralcraft recovery
cargo test -p vectorcraft-engine --test it coreldraw_p5_hardening
```

Expected: all pass. Commit with `test: verify P5 crash recovery and restart`.

### Task 5: Complete automation, scripting, and path-confinement evidence

**Files:**
- Test: `crates/mcp/src/tests.rs`, `crates/mcp/tests/protocol_props.rs`, and the owning `crates/plugins` tests
- Modify only on RED: the existing dispatcher, sandbox, or file-access layer

- [ ] **Step 1: Cover the complete P3/P4 command surface**

Add real MCP `run_command`/`command_batch` scenarios for every command added in those phases, including data-driven variants, PDF/X, print, imposition, texture/pattern, symmetry, lens, and PowerClip lock. Verify results, invalid-input `isError`, atomic batches, step references, and native reopen. No bespoke duplicate tool and no VBA/COM layer.

- [ ] **Step 2: Verify sandbox and file boundaries**

Exercise WASM plug-in resource limits, automation read/write roots, symlink escape attempts, oversized batches, and protocol notifications. Assert denied accesses return structured errors and never read/write outside the granted roots. Run:

```bash
cargo test -p vectorcraft-mcp
cargo test -p vectorcraft-plugins
cargo test -p vectorcraft-mcp --test docs_sync
```

Expected: all pass with generated documentation current. Commit with `test: complete P5 automation and sandbox hardening`.

### Task 6: Verify native packages and browser output on their targets

**Files:**
- Modify only on reproduced failure: `.github/workflows/*.yml` and `packaging/**`
- Evidence: `docs/p5-inventory.md`

- [ ] **Step 1: Build packages with the platform tools**

Run the existing release/CI workflows without changing their package formats: macOS universal DMG and `.app.zip`, Linux glibc-targeted `.deb`/`.rpm`/AppImage/Flatpak, Windows x64 installer/portable and ARM64 package, FreeBSD native package, and the wasm web bundle. Do not claim an artifact from a different OS is locally validated.

- [ ] **Step 2: Install and launch on the actual targets**

Use the existing native CI runners/container smoke jobs to install each package, launch CoralCraft and `coralcraft-cli --version`, create/open a small document, and export SVG/PDF. Check macOS archive signature/identity, Debian 11 glibc baseline and required dependencies, Windows install policy and ARM64 native launch, FreeBSD native binary, and browser asset availability. Run `packaging/check-app-specific.sh` where applicable. Record workflow run URLs, OS/architecture, commands, and results in `docs/p5-inventory.md`.

- [ ] **Step 3: Fix only evidenced package failures**

Any packaging change starts with the failing install/launch log. Keep package identity, old-glibc target, signing, and actual install smoke assertions intact. Commit each platform fix separately with a conventional `fix:` message; never merge an unverified artifact.

### Task 7: Run the final production acceptance and close P5

**Files:**
- Modify: `docs/p5-inventory.md`, `docs/gaps.md`, `docs/target-app-parity.md`, `ROADMAP.md`, `docs/ai-memory/03-current.md`, `docs/ai-memory/04-known-issues.md`, `README.md`, `docs/superpowers/plans/2026-11-22-p5-automation-performance-hardening.md`

- [ ] **Step 1: Re-run representative production workflows**

For every representative project, prove sustained editing → automation → native save/reopen → recovery restore → SVG/PDF/PDF-X/CDR export with exact warning expectations → target-platform install/launch. Compare final output structure and performance against the baseline without hiding noisy measurements or changed output hashes.

- [ ] **Step 2: Run the full quality gate**

```bash
cargo xtask ci
```

Record the actual result. Any failure other than known-issue 0b blocks P5. If 0b is the sole failure, run serially:

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

Expected: all exit 0 and font-tree assertions hold. This is still not a clean full-CI pass while 0b remains red.

- [ ] **Step 3: Record release evidence honestly**

Update roadmap/parity/README with actual platforms, scenarios, measurements, and open limits. Do not mark P5 complete if any representative project, recovery, automation, or platform scenario is missing. Keep S2/P0.1 open and list them explicitly as independent certification work. Run:

```bash
cargo xtask stats
git diff --check
node .gitnexus/run.cjs detect-changes --scope all --repo .
git status --short --branch
```

Expected: complete graph analysis and intentional files only. Commit with `docs: complete the P5 product hardening evidence`.

## Completion criteria

P5 is complete only when representative projects pass sustained editing, automation, native/reopen, real crash recovery, export, and target-platform installation with stable measured budgets and honest evidence. Do not close over a missing platform, noisy performance result, self-skipping oracle, or unverified package. Any remaining item becomes a named follow-up rather than a silent completion claim.
