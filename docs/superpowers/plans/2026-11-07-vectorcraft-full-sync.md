# VectorCraft Full Sync Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Synchronize CoralCraft's source from the original VectorCraft import base to `36ef34dfdd8dfd1abc19fd209678860a00583558`, while preserving and testing every existing CoralCraft extension.

**Architecture:** Use a real three-way merge from `a466625c8987550635cda951ebf36e08e1e80a56` onto `origin/main@8208a6c1e950c237b7f93e5239861af4d8074aab` in an isolated worktree. Resolve the 130 known conflicted paths by ownership, adapt CoralCraft features to the target APIs, and require a target-hash/ledger audit plus behavioral tests before the final merge commit.

**Tech Stack:** Rust 2024, Cargo workspaces, VectorCraft `vectorcraft-*` crates, CoralCraft `coralcraft-cdr` and `corel-import`, egui/eframe, craft-libs, Git three-way merge, GitHub Actions.

**Approved design:** `docs/superpowers/specs/2026-11-07-vectorcraft-full-sync-design.md`.

**Scope and commit boundary:** The upstream dependency and API changes are coupled. Tasks 3–8 are separate review and focused-test slices in one open merge; do not create knowingly broken intermediate commits. Commit the integrated source only after Tasks 3–9 pass. Fixes discovered by the full build in Task 10 receive their own narrow red/green commits.

---

### Task 1: Record the fixed inputs and divergence ledger

**Files:**
- Create: `docs/upstream-divergences.tsv`
- Modify: `docs/upstream-vectorcraft.md`
- Reference: `docs/superpowers/specs/2026-11-07-vectorcraft-full-sync-design.md`

- [ ] **Step 1: Create the ledger schema and initial owned-path rows**

Create a tab-separated file with no quoting or multiline fields. Start with these rows, then add one row for each additional intentional target difference discovered during the merge:

```tsv
path	class	reason
AGENTS.md	project-policy	CoralCraft agent rules remain authoritative
README.md	project-policy	CoralCraft product and measured status remain authoritative
ROADMAP.md	project-policy	CoralCraft roadmap and measured scores remain authoritative
ASSETS.md	asset-ownership	Merged notices exclude VectorCraft brand assets
NOTICE	asset-ownership	Retain applicable upstream notices without adding brand grants
crates/cdr	coralcraft-owned	CDR and CMX parser and writer
crates/corel-import	coralcraft-owned	VectorCraft document bridge and lossless CoralCraft metadata
legacy	coralcraft-owned	Pre-fork compatibility code retained until V7
apps/coralcraft	coralcraft-adaptation	Upstream desktop advances under the CoralCraft package identity
apps/coralcraft-cli	coralcraft-adaptation	Upstream CLI advances under the CoralCraft package identity and probe command
```

For upstream-owned paths intentionally absent from CoralCraft, use the exact target path, class `excluded`, and a reason naming the brand, project-policy, or obsolete-support rule. Do not use wildcard rows: the audit compares exact paths.

- [ ] **Step 2: Add the fixed-point record**

Add a new sync-log entry with this exact target and base:

```markdown
| 2026-11-07 | `36ef34dfdd8dfd1abc19fd209678860a00583558` | Full source sync from `a466625c8987550635cda951ebf36e08e1e80a56` | Integration in progress; completion requires the hash audit and all gates in the approved design. |
```

Do not mark the sync complete in this task.

- [ ] **Step 3: Validate the ledger mechanically**

Run:

```bash
python3 - <<'PY'
import csv
from pathlib import Path
p = Path('docs/upstream-divergences.tsv')
rows = list(csv.DictReader(p.open(), delimiter='\t'))
assert rows and set(rows[0]) == {'path', 'class', 'reason'}
paths = [r['path'] for r in rows]
assert len(paths) == len(set(paths)), 'duplicate ledger path'
assert all(r['path'] and r['class'] and r['reason'] for r in rows)
assert all('\n' not in r['path'] for r in rows)
print(f'{len(rows)} divergence rows valid')
PY
```

Expected: `10 divergence rows valid`, or a larger exact count if reviewed rows were added.

- [ ] **Step 4: Commit only the planning records**

```bash
git add docs/upstream-divergences.tsv docs/upstream-vectorcraft.md
git commit -m "docs: record VectorCraft full-sync inputs"
```

- [ ] **Step 5: Merge the approved planning records before the source merge**

```bash
git push -u origin HEAD
gh pr create --repo yupoet/coralcraft --base main --head "$(git branch --show-current)" --title "Plan the VectorCraft full sync" --body "Includes the approved design, implementation plan, fixed commits, and initial divergence ledger."
gh pr checks --watch
gh pr merge --squash --delete-branch --match-head-commit "$(git rev-parse HEAD)"
git fetch origin --prune
```

Expected: the planning PR passes its checks and merges. The source integration starts from the resulting `origin/main`, which contains the plan and ledger.

---

### Task 2: Create the isolated merge and reproduce the conflict inventory

**Files:**
- Create locally, never commit: `plan/upstream-merge-conflicts.txt`
- Modify during merge: all paths reported by Git

- [ ] **Step 1: Create the integration worktree from the merged mainline**

From the repository root:

```bash
git fetch origin --prune
git merge-base --is-ancestor 8208a6c1e950c237b7f93e5239861af4d8074aab origin/main
git worktree add plan/sync-vectorcraft -b sync/vectorcraft-36ef34d origin/main
cd plan/sync-vectorcraft
```

The code baseline remains `8208a6c`; the planning merge may add documentation on top of it. Review any additional functional commit before merging, and never reset an existing local `main`.

- [ ] **Step 2: Fetch only the exact target and base objects**

```bash
git fetch --no-tags --depth=1 https://github.com/storytold/vectorcraft.git 36ef34dfdd8dfd1abc19fd209678860a00583558
git fetch --no-tags --depth=1 https://github.com/storytold/vectorcraft.git a466625c8987550635cda951ebf36e08e1e80a56
git cat-file -t 36ef34dfdd8dfd1abc19fd209678860a00583558
git cat-file -t a466625c8987550635cda951ebf36e08e1e80a56
```

Expected: both commands print `commit`.

- [ ] **Step 3: Install a temporary graft so the normal merge sees the real base**

The initial import commit `9cf935ba9d0bed596d3eede268948a0d1f7e599f` was a snapshot rather than a child of upstream. Temporarily graft it to the real base for this merge only:

```bash
git replace --graft 9cf935ba9d0bed596d3eede268948a0d1f7e599f a466625c8987550635cda951ebf36e08e1e80a56
test "$(git merge-base HEAD 36ef34dfdd8dfd1abc19fd209678860a00583558)" = "a466625c8987550635cda951ebf36e08e1e80a56"
git merge --no-commit --no-ff 36ef34dfdd8dfd1abc19fd209678860a00583558
```

Expected: the merge stops with conflicts. The graft is local session state; remove it after the integrated merge commit in Task 9. To abandon the merge:

```bash
git merge --abort
git replace -d 9cf935ba9d0bed596d3eede268948a0d1f7e599f
```

- [ ] **Step 4: Capture and classify every conflict**

```bash
mkdir -p plan
git diff --name-only --diff-filter=U > plan/upstream-merge-conflicts.txt
wc -l plan/upstream-merge-conflicts.txt
```

Expected: 130 paths if the fixed inputs are unchanged. Investigate any different count as an input/history discrepancy before resolving code. Assign each path to one ownership class in `docs/upstream-divergences.tsv` when its result differs from the target.

---

### Task 3: Align manifests, application renames, and project-owned paths

**Files:**
- Modify: `Cargo.toml`, `Cargo.lock`
- Modify: `apps/coralcraft/**`, `apps/coralcraft-cli/**`, `apps/vectorcraft-web/**`
- Modify: `xtask/src/main.rs`, `xtask/src/layers.rs`
- Preserve: `crates/cdr/**`, `crates/corel-import/**`, `legacy/**`
- Preserve CoralCraft ownership: `docs/**`, `README.md`, `ROADMAP.md`, `AGENTS.md`, `CLAUDE.md`, `ASSETS.md`, `NOTICE`
- Exclude: upstream brand assets and brand documentation

- [ ] **Step 1: Rebuild the root manifest from the target plus CoralCraft additions**

Take the target's dependency versions, patch sources, features, lints, and profile settings. Retain these exact CoralCraft workspace entries in their existing sections:

```toml
exclude = ["legacy", "fuzz"]
default-members = ["crates/*", "apps/coralcraft", "apps/coralcraft-cli", "xtask"]

coralcraft-color = { path = "legacy/crates/color" }
coralcraft-doc = { path = "legacy/crates/doc" }
coralcraft-geom = { path = "legacy/crates/geom" }
coralcraft-cdr = { path = "crates/cdr" }
corel-import = { path = "crates/corel-import" }
```

Keep CoralCraft's version and repository. Keep `apps/coralcraft` and `apps/coralcraft-cli` on the target dependency set, adding only the existing CDR/probe requirements. Register `coralcraft-cdr` and `corel-import` in `xtask/src/layers.rs`; retain `cargo xtask cleanroom` in `xtask/src/main.rs`.

- [ ] **Step 2: Map upstream application changes into CoralCraft paths**

For every merge message naming `apps/vectorcraft`, place the resulting file under `apps/coralcraft`; for `apps/vectorcraft-cli`, use `apps/coralcraft-cli`. Accept the target's new modules and `tests/it/` layout. Preserve the CoralCraft `probe` module and its synthetic-fixture tests. Keep the web package at `apps/vectorcraft-web` and accept target behavior there.

Check that no duplicate upstream application packages remain:

```bash
test ! -e apps/vectorcraft/Cargo.toml
test ! -e apps/vectorcraft-cli/Cargo.toml
```

- [ ] **Step 3: Resolve policy, documentation, and asset conflicts without importing brand material**

Keep CoralCraft's policy and roadmap files. Review target technical documentation and record relevant API changes in CoralCraft's own documents instead of replacing those files. Keep original CoralCraft icons and asset rows. Do not restore target-only `assets/app-icon`, `docs/brand`, `docs/images`, or DMG artwork excluded by the approved design.

- [ ] **Step 4: Regenerate the lockfile and run structural checks**

```bash
cargo metadata --format-version 1 --no-deps > plan/sync-metadata.json
cargo xtask layers
cargo xtask assets
cargo xtask cleanroom
```

Expected: all commands exit 0. If manifest resolution requires downloading dependencies, allow Cargo to update `Cargo.lock`; do not edit lockfile conflict markers manually.

---

### Task 4: Adapt `corel-import` to the target document model

**Files:**
- Modify: `crates/corel-import/src/lib.rs`
- Modify: `crates/corel-import/tests/oracle_and_samples.rs`
- Modify: `crates/doc/src/lib.rs`
- Preserve: `crates/doc/src/coralcraft.rs`
- Test: `crates/corel-import/**`, `crates/doc/src/coralcraft.rs`

- [ ] **Step 1: Run the existing bridge tests against the merged APIs**

```bash
cargo test -p corel-import
cargo test -p vectorcraft-doc corel_layer_role_page_matching_and_roundtrip
```

Expected at the start of adaptation: compilation failures identify each changed field or type. A filter that matches zero tests is not a pass.

- [ ] **Step 2: Adapt extension storage without weakening losslessness**

The target retains `Document::unknown` but changes its type to copy-on-write `Shared<serde_json::Map<String, Value>>`. Existing map reads work through `Deref`; use mutable map operations through `DerefMut` rather than replacing the field with a new serialization mechanism:

```rust
// CoralCraft: Shared derefs mutably with copy-on-write semantics.
let val = serde_json::to_value(data).map_err(|e| CorelImportError::Conversion(e.to_string()))?;
doc.unknown.insert("coralcraft".into(), val);
```

Use the existing `CorelImportError` conversion for serialization errors; do not unwrap. Preserve `CoralCraftDocData`, residuals, page metadata, layer metadata, and palettes. Keep `sync_coralcraft_extension` as a no-op for pure VectorCraft documents.

- [ ] **Step 3: Run focused bridge and metadata tests**

```bash
cargo test -p corel-import
cargo test -p vectorcraft-doc corel
```

Expected: all matched tests pass with zero failures. Confirm from output that the filters matched tests.

---

### Task 5: Rehome CDR/CMX and legacy import in the target file-I/O architecture

**Files:**
- Modify: `crates/engine/Cargo.toml`
- Modify: `crates/engine/src/cmd/fileio.rs`
- Modify: `crates/engine/src/cmd/fileio/formats.rs`
- Modify: `crates/engine/src/cmd/fileio/load.rs`
- Modify: `crates/engine/src/cmd/fileio/save.rs`
- Modify: `crates/engine/src/cmd/fileio/encode.rs`
- Modify: `crates/engine/src/cmd/fileio/export.rs`
- Modify: `crates/engine/src/cmd/fileio/screens.rs`
- Modify: `crates/engine/src/cmd/fileio/tests.rs` or the target's matching focused test modules
- Modify: `crates/mcp/src/tools.rs`, `crates/mcp/src/tests_fileio.rs`
- Modify: `crates/ui-egui/src/io.rs`

- [ ] **Step 1: Restore the format registrations in `formats.rs`**

Append these existing CoralCraft definitions to the target `FORMATS`; do not restore the deleted old `fileio/mod.rs`:

```rust
// CoralCraft: CorelDRAW container version option.
const CDR_VERSION: FormatOption = FormatOption {
    name: "cdrVersion",
    ty: "string",
    default: "\"cdr7\"",
    description: "cdr7 (classic RIFF CDR7 container) or cdrx (modern X4+ ZIP+RIFF container)",
};
const CDR_OPTIONS: &[FormatOption] = &[CDR_VERSION];

// Append to FORMATS.
Format {
    id: "cdr",
    label: "CorelDRAW",
    extensions: &["cdr"],
    mime: "application/vnd.corel-draw",
    read: true,
    write: true,
    raster: false,
    options: CDR_OPTIONS,
},
Format {
    id: "cmx",
    label: "Corel Presentation Exchange",
    extensions: &["cmx"],
    mime: "image/x-cmx",
    read: true,
    write: true,
    raster: false,
    options: &[],
},
reader("coralcraft", "CoralCraft (Legacy)", &["coralcraft"], "application/x-coralcraft", false),
```

Include `cdr`, `cmx`, and `coralcraft` in the target Open and Place extension lists. Append `cdr` and `cmx` to `SAVE_FORMATS`, but not to Export for Screens. Preserve CDR options on Save As and expose both formats through MCP export/save descriptions.

- [ ] **Step 2: Restore content detection and loading through `corel-import`**

Add the existing probe mapping in `load.rs::detect` before extension fallback, and retain these load arms in `load_with`:

```rust
// CoralCraft: CorelDRAW, CMX, and pre-fork CoralCraft containers.
"cdr" | "cmx" => {
    let out = corel_import::import_cdr_bytes(bytes)
        .map_err(|e| err(format!("can't open `{}`: {e}", file_name(name))))?;
    (out.document, out.warnings, false)
}
"coralcraft" => {
    let (d, warnings) = corel_import::import_legacy_coralcraft_bytes(bytes)
        .map_err(|e| err(format!("can't open `{}`: {e}", file_name(name))))?;
    converted = true;
    (d, warnings, false)
}
```

Keep the PR #3 oversized-PNG IHDR fallback and warning behavior in the target raster loader; do not overwrite it with the older target logic.

- [ ] **Step 3: Restore CDR/CMX encoding**

In `encode.rs::encode_files`, route `cdr` to `write_cdr7_document` or `write_cdrx_zip_document` according to `cdrVersion`, and route `cmx` to `write_cmx1_document`. Reuse the current accepted aliases and errors from `8208a6c`; reject unknown versions with `bad(C, ...)` and propagate encoder failures.

Call `corel_import::sync_coralcraft_extension` from the target native save path only when the extension block exists. Do not add the block to pure VectorCraft documents.

- [ ] **Step 4: Run the existing behavior matrix on the merged tree**

Run each filter and inspect that it matches a test:

```bash
cargo test -p vectorcraft-engine cdr_and_cmx_are_in_format_tables_and_open_and_place
cargo test -p vectorcraft-engine cdr_and_cmx_writers_export_serialize_and_save_as
cargo test -p vectorcraft-engine vectorcraft_format_compatibility_matrix_covers_every_format_v2_1
cargo test -p vectorcraft-engine vectorcraft_drawcraft_and_vctemplate_preserve_coralcraft_extension_block_losslessly_v2_2
cargo test -p vectorcraft-engine legacy_coralcraft_json_and_zip_containers_open_and_place_losslessly_v2_3
cargo test -p vectorcraft-engine tests_linkspanel::an_image_too_large_to_draw_is_reported -- --exact
```

Expected: every command reports zero failures and a nonzero matched-test count. Update test module locations only when the target reorganized them; preserve the assertions and synthetic fixtures.

- [ ] **Step 5: Run local oracle/sample checks when their prerequisites exist**

```bash
cargo test -p vectorcraft-engine cdr_and_cmx_engine_pipeline_passes_libcdr_oracle_and_real_samples
cargo test -p corel-import --test oracle_and_samples
```

Expected: zero failures when the pinned local oracle and operator-owned sample directory are available. If either prerequisite is absent, record exactly that limitation; do not download unowned fixtures or claim sample coverage.

---

### Task 6: Adapt multi-page, master-layer, and rendering behavior

**Files:**
- Modify: `crates/engine/src/cmd/layer.rs`
- Modify: `crates/engine/src/cmd/panelcmds.rs`
- Modify: `crates/render/src/lib.rs`
- Modify: `crates/render/src/tests/layeropts.rs`
- Modify: `crates/corel-import/src/lib.rs`
- Test: existing V3.1–V3.3 tests in those files

- [ ] **Step 1: Reapply page commands to the target artboard APIs**

Retain the existing public parameters `insertAt`, `before`, `after`, `orientation`, `applyToAllPages`, `deleteArt`, and `reflowPages`. Route them through `insert_page_at`, `delete_pages_reflow`, `resize_page_reflow`, and `reorder_page_reflow`. Calls without CoralCraft page parameters or metadata must use the target's ordinary artboard behavior unchanged.

- [ ] **Step 2: Reapply master/Desktop layer semantics**

Preserve `CorelLayerRole::{Normal, MasterAllPages, MasterEvenPages, MasterOddPages, Desktop, Guides, Grid, Unknown}`, `masterInFront`, `pageIndex`, copy-on-duplicate, removal-on-delete, and CDR export routing. In the target renderer, retain background-master → normal-page → foreground-master ordering and Desktop/overlay separation; retain non-printable exclusion for page exports.

- [ ] **Step 3: Run focused page/layer/render tests**

```bash
cargo test -p vectorcraft-engine coreldraw
cargo test -p vectorcraft-engine artboard
cargo test -p vectorcraft-render layer
cargo test -p vectorcraft-doc corel
cargo test -p corel-import
```

Expected: zero failures with nonzero test counts. Compare failures against `8208a6c`; preserve a target fix when both sides changed the same behavior, then reapply only the CoralCraft semantic extension.

---

### Task 7: Adapt the CoralCraft UI, CLI, and MCP surfaces

**Files:**
- Modify: `crates/ui-egui/src/lib.rs`
- Modify: `crates/ui-egui/src/state.rs`
- Modify: `crates/ui-egui/src/menus.rs`
- Modify: `crates/ui-egui/src/workspaces.rs`
- Modify: `crates/ui-egui/src/shortcut_editor.rs`, `crates/ui-egui/src/shortcuts.rs`
- Modify: `crates/ui-egui/src/chrome/control_bar.rs`
- Modify: `crates/ui-egui/src/panels/layers.rs`, `crates/ui-egui/src/dialogs/layer_options.rs`
- Preserve and adapt: `crates/ui-egui/src/page_navigator.rs`, `crates/ui-egui/src/palette_bar.rs`
- Modify: `apps/coralcraft/**`, `apps/coralcraft-cli/**`, `crates/mcp/**`

- [ ] **Step 1: Reconnect UI modules and workspace state**

Register the existing page navigator and palette bar in the target frame order: left toolbox, top control/property bar, right palette and dockers, bottom page navigator and status bar. Preserve `window.paletteBar`, `window.pageNavigator`, workspace round-trip, and the CorelDRAW docker layout. Use `chrome/control_bar.rs` for target control-bar changes; do not restore the monolithic old `chrome.rs`.

- [ ] **Step 2: Reapply shortcuts, layer controls, and identity**

Preserve the CorelDRAW preset and F1–F12 dispatch against the target keymap. Preserve role badges and role/master options in the target layer UI. Keep CoralCraft app/MCP names, help links, app-level environment variables, preferences directories, original icons, and CLI `probe`; keep VectorCraft library and file-format identities.

- [ ] **Step 3: Run focused UI and host tests**

```bash
cargo test -p vectorcraft-ui-egui page_navigator
cargo test -p vectorcraft-ui-egui palette_bar
cargo test -p vectorcraft-ui-egui coreldraw_workspace
cargo test -p vectorcraft-ui-egui shortcut
cargo test -p vectorcraft-ui-egui dialog_sliders_report_the_numbers_their_fields_show
cargo test -p coralcraft-cli
cargo test -p vectorcraft-mcp
```

Expected: zero failures and nonzero matched counts for filtered commands. The AccessKit slider assertion must remain order-independent.

- [ ] **Step 4: Exercise a real UI command path**

Run the integrated desktop with a local control port and synthetic document, then invoke `ui.tool.select`, `engine.execute`, `ui.render`, and `ui.inspect`. Verify the CorelDRAW workspace, page navigation, palette fill/stroke actions, and one target-added tool. Record the exact requests and response status in `docs/ai-memory/03-current.md`. A successful launch alone is not behavioral verification.

---

### Task 8: Merge platform, packaging, workflow, and 32-bit protections

**Files:**
- Modify: `.github/workflows/**`
- Modify: `packaging/**`
- Modify: `apps/coralcraft/src/xwayland.rs` and other target host files named by conflicts
- Modify: `crates/pdf/src/streams.rs`, `crates/metafile/src/import/emf.rs`, `crates/trace/src/lib.rs`, `crates/trace/src/subpixel.rs`
- Modify: `docs/windows7.md` only with executed evidence

- [ ] **Step 1: Preserve local platform fixes on top of target changes**

Reapply the semantic checks from PR #3 when the target has not already incorporated equivalent behavior: checked PDF predictor columns, checked EMF bitmap offsets, bounded tracing dimensions and pixel indices, oversized-image retention, XWayland handling, package identities, and Linux package baseline checks. Prefer a target helper when it now provides the same checked operation.

- [ ] **Step 2: Keep i686 execution serial**

Retain this exact test invocation in `.github/workflows/ci.yml` unless the target's replacement workflow has an equivalent serial setting:

```yaml
run: cargo test --workspace --target i686-unknown-linux-gnu -- --test-threads=1
```

Retain clean-room, asset, format-oracle, wasm, FreeBSD, Windows ARM64, and package smoke coverage. Do not remove a guard without recording what now prevents its original failure.

- [ ] **Step 3: Resolve removed Windows 7 support explicitly**

Inspect the target's replacement platform requirements. Keep the CoralCraft Windows 7 path only if it builds and its native smoke test runs. If the target dependencies make that impossible, stop with the concrete compiler/platform evidence for an operator decision; do not silently delete the support files or claim support.

- [ ] **Step 4: Run focused hostile-input and packaging checks**

```bash
cargo test -p vectorcraft-pdf predictors_the_reader_can_apply_pass
cargo test -p vectorcraft-engine --test import_fuzz pdf_streams_the_reader_would_fail_on_are_refused -- --exact
cargo test -p vectorcraft-engine --test import_fuzz pdf_resources_the_reader_cannot_parse_never_panic -- --exact
cargo test -p vectorcraft-engine --test import_fuzz random_emf_records_never_panic -- --exact
cargo test -p vectorcraft-trace an_image_too_large_to_trace_is_refused_before_decoding
cargo test -p vectorcraft-trace decode_to_fit_scales_an_oversized_image_down
packaging/check-app-specific.sh
```

Expected: zero failures and a successful package-identity check. Repeat the same focused tests on i686 before the final full gate when the local container is available.

---

### Task 9: Prove target coverage and create the integrated merge commit

**Files:**
- Modify: `docs/upstream-divergences.tsv`
- Modify: `docs/upstream-vectorcraft.md`
- Modify: `docs/ai-memory/03-current.md`, `docs/ai-memory/04-known-issues.md`
- Create locally: `plan/upstream-target-tree.txt`, `plan/upstream-result-tree.txt`, `plan/upstream-actual-differences.txt`

- [ ] **Step 1: Reject unresolved conflicts and merge markers**

```bash
test -z "$(git diff --name-only --diff-filter=U)"
git diff --check
cargo fmt --all -- --check
```

Expected: no unmerged paths, whitespace errors, or formatting diff. Also search source and configuration files for conflict markers and inspect every match rather than blindly deleting generated text.

- [ ] **Step 2: Generate target/result hash manifests**

```bash
TARGET=36ef34dfdd8dfd1abc19fd209678860a00583558
git ls-tree -r "$TARGET" > plan/upstream-target-tree.txt
git ls-tree -r HEAD > plan/upstream-result-tree.before-sync.txt
git add -A
git write-tree > plan/upstream-result-tree-id.txt
git ls-tree -r "$(cat plan/upstream-result-tree-id.txt)" > plan/upstream-result-tree.txt
```

The manifests contain mode, type, blob hash, and exact path for every file. Keep them local; the committed ledger is the compact audit record.

- [ ] **Step 3: Require every target difference to have an exact ledger row**

```bash
TARGET=36ef34dfdd8dfd1abc19fd209678860a00583558
RESULT=$(cat plan/upstream-result-tree-id.txt)
git diff --name-only "$TARGET" "$RESULT" -- > plan/upstream-actual-differences.txt
python3 - <<'PY'
import csv
from pathlib import Path
all_differences = set(Path('plan/upstream-actual-differences.txt').read_text().splitlines())
target_paths = {line.split('\t', 1)[1] for line in Path('plan/upstream-target-tree.txt').read_text().splitlines()}
actual = all_differences & target_paths
with open('docs/upstream-divergences.tsv', newline='') as f:
    rows = list(csv.DictReader(f, delimiter='\t'))
expected = {r['path'] for r in rows}
missing = sorted(actual - expected)
stale = sorted((expected & target_paths) - actual)
assert not missing, 'unexplained target differences:\n' + '\n'.join(missing)
assert not stale, 'stale target-path ledger rows:\n' + '\n'.join(stale)
print(f'all {len(actual)} target differences are explained')
PY
```

Expected: zero unexplained and zero stale rows. Add or correct exact ledger rows until the audit passes. CoralCraft-owned extra files absent from the target are checked through the ledger and behavior tests; target paths that differ or disappear must always appear here.

- [ ] **Step 4: Record the actual sync evidence**

Update `docs/upstream-vectorcraft.md` with the completed scope, ledger location, tests actually run, and remaining intentional differences. Update current status and known issues without claiming the full gate before Task 10. Keep measured CoralCraft scores separate from upstream scores.

- [ ] **Step 5: Commit the merge and remove the temporary graft**

```bash
git add -A
git commit -m "feat: synchronize VectorCraft 36ef34d"
git replace -d 9cf935ba9d0bed596d3eede268948a0d1f7e599f
git rev-list --parents -n 1 HEAD
```

Expected: the commit has the previous CoralCraft integration HEAD and `36ef34dfdd8dfd1abc19fd209678860a00583558` as parents. `git replace -d` must succeed; no replace ref remains for the import commit.

---

### Task 10: Run the milestone build, fix regressions narrowly, and merge the PR

**Files:**
- Modify only the files implicated by a reproduced failure
- Update: `docs/upstream-vectorcraft.md`, `docs/ai-memory/03-current.md`, parity documents affected by measured results

- [ ] **Step 1: Run the complete local milestone gate once**

```bash
cargo xtask ci
cargo test --manifest-path legacy/Cargo.toml
```

Expected: all steps exit 0. Do not start a second full build while one is running. On failure, capture the exact failing test, reproduce it alone, add or restore a failing regression test, apply the narrow fix, rerun that test, and commit the fix separately in English.

- [ ] **Step 2: Run the real 32-bit suite**

Use the existing i686 container/cache setup and run:

```bash
CARGO_TARGET_DIR=/target cargo test --workspace --target i686-unknown-linux-gnu -- --test-threads=1
```

Expected: zero failures. Keep the run serial; a 32-bit address-space allocation failure is not evidence of a product assertion failure.

- [ ] **Step 3: Review the delivered diff and push the integration branch**

```bash
git status --short
git diff --check origin/main...HEAD
git log --oneline origin/main..HEAD
git push -u origin sync/vectorcraft-36ef34d
```

Expected: clean worktree, no whitespace errors, and only sync/fix commits. Review the ledger and target manifests alongside the diff. Do not claim an `agy` review unless it returned actual findings or an explicit no-blocker conclusion.

- [ ] **Step 4: Open the PR and require every platform check**

```bash
gh pr create --repo yupoet/coralcraft --base main --head sync/vectorcraft-36ef34d --title "Sync VectorCraft 36ef34d" --body-file plan/sync-pr-body.md
gh pr checks --watch
```

The PR body must name the target, base, intentional divergence ledger, exact local tests, unrun local sample/platform checks, and clean-room boundaries. Expected: all required macOS, Ubuntu, Windows, FreeBSD, ARM64, lint, fuzz, 32-bit, package, and install checks pass on the exact head commit.

- [ ] **Step 5: Merge only the verified head and verify main**

```bash
gh pr merge --merge --delete-branch --match-head-commit "$(git rev-parse HEAD)"
git fetch origin --prune
gh pr list --repo yupoet/coralcraft --state open
git log -1 --oneline origin/main
```

Use a merge commit here, not squash, so the merged result retains the CoralCraft integration and VectorCraft target ancestry. Expected: the PR is merged, `origin/main` contains the sync, and no open PR remains. If another PR is open for unrelated work, report it rather than closing it automatically.

- [ ] **Step 6: Open the V4.3 follow-up from the synced tool architecture**

Update `docs/ai-memory/03-current.md`, `docs/gaps.md`, and `ROADMAP.md` with measured post-sync state. Start V4.3 from `crates/tools/src/catalog.rs`, `crates/ui-egui/src/toolbar.rs`, and `crates/ui-egui/src/chrome/control_bar.rs`, using the public CorelDRAW guide URL in the approved spec. Implement one small tool group at a time through `tool.select`, `tool.setOption`, and shared engine commands; do not claim CorelDRAW parity from the sync itself.
