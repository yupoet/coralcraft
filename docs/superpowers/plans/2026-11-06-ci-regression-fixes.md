# CI Regression Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix all previously identified CI failures and merge the verified result to `main`.

**Architecture:** Keep the existing engine and packaging architecture. Change only the paste-anchor selection, test integer conversion, one boolean expression, and the affected CI/package inputs.

**Tech Stack:** Rust, Proptest, GitHub Actions, PowerShell/WiX, nfpm.

---

### Task 1: Engine and 32-bit test fixes

**Files:**
- Modify: `crates/engine/src/cmd/edit.rs`
- Modify: `crates/engine/src/cmd/create.rs`
- Modify: `crates/engine/src/cmd/patterncmds.rs`
- Modify: `crates/engine/src/tests_clipboard.rs`
- Modify: `crates/engine/src/tests_file.rs`
- Modify: `crates/engine/tests/import_fuzz.rs`

- [ ] **Step 1: Add the failing compound-paste regression test**

Create two rectangles, make and copy a compound, select one of its member paths, then paste in front and in back. After each paste, assert that the new compound's parent is not a compound and run `vectorcraft_testkit::invariants::check_document`.

- [ ] **Step 2: Verify the regression test fails**

Run: `cargo test -p vectorcraft-engine paste_into_a_selected_compound_member`

Expected: FAIL with `compound ... has a non-path child` before the implementation change.

- [ ] **Step 3: Fix the insertion anchors**

In `paste_clip`, Draw Behind, and Draw Inside, replace the raw paint-order selection with `selected_roots(s)?`, preserving the existing front/back index arithmetic. Add failing regressions for all three paths first.

- [ ] **Step 4: Fix Clippy and explicit 32-bit test inputs**

Write the pattern-swatch condition as `d.swatch_name_taken(new) && (!only_own || d.swatches_iter().any(|w| w.name == old && own(w)))`.

In the native-area test, write explicit `u64` cases into the serialized JSON before opening it, and compare against the clamped expected `usize` values. This covers the 32-bit deserialization overflow branch.

- [ ] **Step 5: Verify Task 1**

Run:
```bash
cargo test -p vectorcraft-engine paste_into_a_selected_compound_member
cargo test -p vectorcraft-engine draw_behind_a_selected_compound_member_inserts_beside_the_compound
cargo test -p vectorcraft-engine draw_inside_a_selected_compound_member_targets_the_whole_compound
cargo test -p vectorcraft-engine --test model_based
cargo test -p vectorcraft-engine --test import_fuzz native_area_type_with_2_to_the_60_rows_or_columns_opens_renders_and_exports
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: all commands exit 0.

### Task 2: Packaging and platform workflow fixes

**Files:**
- Modify: `.github/workflows/ci.yml`
- Modify: `.github/workflows/windows-arm64.yml`
- Modify: `.github/workflows/windows7.yml`
- Modify: `packaging/linux/nfpm.yaml`
- Modify: `packaging/linux/package.sh`
- Modify: `packaging/linux/io.github.yupoet.coralcraft.desktop`
- Modify: `packaging/windows/portable.txt`
- Modify: `packaging/check-app-specific.sh`
- Create: `docs/windows7.md`
- Modify: `docs/ai-memory/04-known-issues.md`

- [ ] **Step 1: Preserve the failing package evidence**

Confirm that `https://nfpm.goreleaser.com/gosu-nfpm.sh` returns 404, `docs/windows7.md` is absent, and the ARM64 workflow passes `app-dir: VectorCraft` while `packaging/windows/coralcraft.wxs` installs into `CoralCraft`. Add package-identity assertions before fixing the stale desktop and portable names.

- [ ] **Step 2: Apply minimal workflow and package fixes**

Install `github.com/goreleaser/nfpm/v2/cmd/nfpm@v2.48.0` with `GOBIN`, enable `set -euo pipefail`, and verify the installed binary. Build with cargo-zigbuild 0.24.1 and Zig 0.15.2 for glibc 2.31, then install the package with `apt` on Debian 11. Set the ARM64 `app-dir` to `CoralCraft`.

Add `docs/windows7.md` describing the unsupported Windows 7 SP1 x64 portable build, the host build command, and the requirement to run the actual Windows 7 checklist before claiming runtime support.

- [ ] **Step 3: Record the resolved CI pitfalls**

Add a resolved known-issues entry covering the paste anchor, 32-bit test literals, retired nfpm installer URL, Windows 7 missing documentation, and stale ARM64 directory.

- [ ] **Step 4: Validate package configuration locally**

Run:
```bash
bash -n packaging/linux/package.sh
ruby -e 'require "yaml"; ARGV.each { |f| YAML.load_file(f) }' .github/workflows/ci.yml .github/workflows/windows-arm64.yml .github/workflows/windows7.yml
test -s docs/windows7.md
git diff --check
```

Expected: all commands exit 0. Native Windows package installation remains a required remote CI check.

### Task 3: Full verification and merge

**Files:**
- Modify only if verification exposes another root cause.

- [ ] **Step 1: Run the complete local gate**

Run: `cargo xtask ci`

Expected: every configured step exits 0.

- [ ] **Step 2: Commit and merge**

Commit with an English conventional commit message, push the branch based on the latest `origin/main`, and merge it without resetting the operator's divergent local `main` checkout.

- [ ] **Step 3: Verify remote platform jobs**

Confirm the merged commit's CI, Windows 7, and Windows ARM64 runs complete successfully. Do not claim completion while any required run is pending or failed.
