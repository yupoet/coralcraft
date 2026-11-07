# VectorCraft Full Sync — Design

Date: 2026-11-07
Status: approved design; implementation plan pending

## 1. Decision and context

The operator selected a full VectorCraft synchronization before further CorelDRAW tool work. The next implementation milestone is therefore a base-aware upstream sync, followed by V4.3 only after the synced application preserves CoralCraft's existing behavior.

The sync uses three fixed points:

| Role | Commit |
|---|---|
| Original VectorCraft import base | `a466625c8987550635cda951ebf36e08e1e80a56` |
| CoralCraft local baseline | `8208a6c1e950c237b7f93e5239861af4d8074aab` (`origin/main`, PR #3 merged) |
| VectorCraft sync target | `36ef34dfdd8dfd1abc19fd209678860a00583558` |

The local reference checkouts are already at the target. Updating `refs/` alone does not count as a source sync.

## 2. Measured scope

A target diff from the original base contains 1,991 changed files, approximately 541,730 inserted lines and 153,863 deleted lines across 8,676 upstream commits. It includes dependency, document-model, engine, tool, UI, application, packaging, and workflow changes.

A read-only `git merge-tree` preflight using the real base, local baseline, and target reported 130 conflicted paths. The remaining upstream changes merged automatically in that preflight. The largest conflict groups are application renames, file I/O refactoring, CoralCraft page/layer behavior, UI workspace integration, packaging, CI, and project documentation.

This scope is too large for an unreviewed overwrite or a claim that a single compile establishes parity. It needs an isolated integration branch, explicit ownership rules, targeted behavioral checks during adaptation, and a full build at the milestone boundary.

## 3. Goals

1. Import all VectorCraft source and behavioral changes through the target commit, except documented brand and project-ownership exclusions.
2. Preserve every existing CoralCraft capability, including CDR/CMX I/O, legacy document import, native-format extension data, multi-page documents, master and Desktop layers, page navigation, palette bar, CorelDRAW shortcuts and workspace, CLI/MCP identity, original assets, and platform checks.
3. Preserve compatibility with every VectorCraft format supported at either the old base or the target.
4. Keep the application on the shared engine command path. UI, CLI, MCP, and automation must not gain separate implementations of the same operation.
5. Produce a machine-checkable account of upstream-identical files and intentional CoralCraft divergences.
6. Run focused tests while resolving each functional area, then run the complete local and remote regression gates at the milestone end.

## 4. Non-goals

- No claim of CorelDRAW parity or completion of V4.3 as a side effect of this sync.
- No wholesale import of VectorCraft brand assets, trademarks, screenshots, fonts, or project roadmap into CoralCraft.
- No replacement of CoralCraft's public-documentation constraints with observations of proprietary software.
- No unrelated CorelDRAW features, format self-round-trip milestones, or general refactoring outside the sync.
- No full workspace rebuild after every small conflict-resolution step.

## 5. Integration architecture

### 5.1 Isolated, base-aware merge

Create `sync/vectorcraft-36ef34d` from the local baseline in a dedicated worktree. Fetch the exact base and target commits without resetting or overwriting `main`. Use the original base for a real three-way merge so that unchanged upstream files can advance without being mistaken for CoralCraft additions.

The merge result must retain the local mainline and the upstream target as identifiable inputs. Do not use a directory copy as the sole merge mechanism: it loses rename detection and the distinction between upstream edits and CoralCraft edits.

### 5.2 Ownership classes

Every conflicted or locally modified path receives one of these classifications in the sync ledger:

| Class | Resolution rule |
|---|---|
| Upstream-managed, no CoralCraft divergence | Match the target byte-for-byte. |
| Upstream-managed with CoralCraft adaptation | Take the target behavior, then reapply the CoralCraft semantic delta against the new APIs. Keep or add `// CoralCraft:` / `# CoralCraft:` at each local integration point. |
| CoralCraft-owned | Preserve and adapt the local implementation; upstream has no authoritative replacement. |
| Project policy and roadmap | Keep CoralCraft ownership. Import upstream technical findings only after review and paraphrase them in the relevant CoralCraft document. |
| Brand and trademark material | Keep CoralCraft originals and exclusions; never import VectorCraft brand assets. |
| Obsolete or removed upstream path | Remove it only when its CoralCraft behavior has a tested replacement or an explicit, documented operator decision permits dropping it. |

The current tree has 45 files containing explicit CoralCraft markers. Those markers are an inventory aid, not proof that every divergence is marked. The sync ledger also includes unmarked rebrand and compatibility changes found by the three-way diff.

### 5.3 Application and package identities

Map upstream `apps/vectorcraft` changes to `apps/coralcraft`, and upstream `apps/vectorcraft-cli` changes to `apps/coralcraft-cli`, including newly added modules and test-directory moves. Keep `apps/vectorcraft-web` as the upstream web package unless the existing CoralCraft integration requires a narrow adaptation.

Preserve CoralCraft package names, executable names, bundle/application IDs, preference directories, app-level environment variables, MCP server identity, help links, and original icons. Keep upstream library package names and VectorCraft file-format identities.

### 5.4 Manifest and dependency alignment

Start from the target workspace manifest and dependency pins, then restore the CoralCraft workspace exclusions, owned crates, legacy path dependencies, repository identity, and required lint differences. This includes the target's shared `craft-libs`, egui/eframe, and patch-source changes as a coordinated unit.

Regenerate `Cargo.lock` through Cargo after the manifests are consistent. Do not manually resolve lockfile conflicts hunk by hunk. Confirm dependency licenses and clean-room checks before accepting new assets or dependencies.

## 6. Functional adaptation requirements

### 6.1 Document model and native compatibility

Confirm the target document model still provides a suitable namespaced extension location for `CoralCraftDocData`. If its storage API changed, adapt `corel-import` without dropping unknown fields, page metadata, layer metadata, CDR residuals, unsupported objects, or palette data.

Pure VectorCraft documents must remain unchanged by CoralCraft extension synchronization. Documents containing CoralCraft data must round-trip through `.vectorcraft`, `.drawcraft`, and `.vctemplate` without losing that data. Legacy `.coralcraft` JSON and ZIP inputs must continue to open through the conversion path.

### 6.2 CDR/CMX and the refactored file I/O layer

The target deletes or reorganizes the old `fileio/mod.rs` structure. Rehome the existing CDR/CMX format registration, content detection, Open, Place, Save As, Export, Serialize, and MCP descriptions into the target's new structure rather than restoring the deleted old module.

Preserve the CDR7, CDRX, and CMX1 writers, legacy WL input, legacy `.coralcraft` import, format options, and the distinction between native saving and export. Synthetic hostile-input and overflow regressions from PR #3 must remain active on 32-bit and 64-bit targets.

Where local operator-owned samples and the public libcdr oracle are available, verify structural page/layer/object counts and writer acceptance. These local fixtures are never committed.

### 6.3 Pages, layers, and rendering

Adapt multi-page insertion, deletion, resizing, orientation, reordering, and object reflow to the target artboard APIs. Preserve normal VectorCraft artboard behavior when CoralCraft page parameters and metadata are absent.

Adapt `CorelLayerRole`, master-layer repetition, Desktop/overlay behavior, print exclusion, and front/back master ordering to the target renderer. Layer creation, duplication, property editing, and CDR export must retain role metadata and page routing.

### 6.4 UI and shared commands

Reapply the page navigator, vertical palette bar, CorelDRAW workspace preset, layer-role controls, CorelDRAW shortcut preset, function-key tool dispatch, and branded UI surfaces to the target UI architecture.

New upstream commands and tools remain available; CoralCraft menus and dockers must not hide them unintentionally. CoralCraft UI actions continue through `app.run` and engine commands, with structured errors rather than silent `.ok()` handling where a failure matters to the user.

The later V4.3 evidence map will use the synced tool catalog and property-bar implementation. The public CorelDRAW behavior source for that follow-up is the publicly distributed user guide registered in `refs/REFS.md`:
`https://learn.corel.com/wp-content/uploads/2022/08/CorelDRAW-UserGuide-Mar22.pdf`.

### 6.5 Platform and CI behavior

Merge platform fixes semantically, including 32-bit checked arithmetic, serial i686 tests, Linux package identity and baseline checks, Windows ARM64 installation, FreeBSD, and macOS packaging. Preserve clean-room and asset gates alongside the target's updated quality gates.

Upstream removed some Windows 7 files. Existing CoralCraft Windows 7 support must not silently disappear: either retain and exercise the adaptation or stop for an explicit operator decision with the incompatibility evidence. No documentation may claim support that was not executed on the named platform.

## 7. Error handling, clean room, and assets

- All parsers and control surfaces treat input as hostile, return structured errors, and retain allocation, recursion, and arithmetic bounds.
- Keep the existing no-panic expectations and imported upstream guards. A merge conflict must not be resolved by deleting a validation or regression test merely because its API changed.
- Test documents and byte streams are synthesized from public specifications or taken from operator-owned local samples that remain untracked.
- Never execute or automate Corel or Adobe software, and never import their assets or authored fixtures.
- Do not copy GPL/AGPL code or MPL implementation code. Public format semantics and permitted MIT/Apache family source remain separate concerns.
- Every added asset requires an `ASSETS.md` entry; fonts remain outside this repository.

## 8. Verification design

### 8.1 During adaptation

Each functional area receives focused tests immediately after its APIs are reconciled:

1. Manifest and layer checks for workspace structure and dependency rules.
2. Document/native-format extension and legacy import round-trips.
3. CDR/CMX format matrix, synthetic input, writer/oracle, and available local-sample checks.
4. Page/layer command tests and renderer ordering/exclusion tests.
5. UI interaction and accessibility tests for page navigation, palette, workspace, shortcuts, and updated upstream controls.
6. CLI/MCP command-path tests and package identity checks.

These are evidence for the area being adapted, not a substitute for the final full build. Record the exact commands, targets, fixtures, and platforms that actually ran.

### 8.2 Full-sync proof

Produce a sync manifest covering every upstream-managed path:

- target path and target blob hash;
- resulting CoralCraft blob hash, or intentional absence;
- ownership/divergence identifier from the sync ledger.

Every upstream-managed path without an approved divergence must be byte-identical to the target. Every differing or absent path must link to a ledger reason. This check distinguishes a real source sync from a reference refresh or selective feature port.

### 8.3 Milestone-boundary gates

Only after the focused adaptations are complete, run the complete gates on the integrated deliverable:

- formatting, lockfile consistency, lint, workspace and legacy tests;
- `cargo xtask ci`, including layers, assets, clean-room, wasm, and package checks;
- the 32-bit workspace suite with serial test execution;
- macOS, Ubuntu, Windows, FreeBSD, and Windows ARM64 remote checks;
- actual package installation and launch where the existing workflows provide it.

Any failure returns to the owning functional area for a narrow fix and focused reproduction before the full gate is retried.

## 9. Commits and records

Use an integration branch and pull request; do not push directly to `main`. Keep the mainline intact until the integrated result passes the milestone gates. Commit only completed, verified slices; if dependency coupling prevents an intermediate tree from building, keep that work within the current sync slice rather than committing a knowingly broken intermediate state.

Update these records with the integrated change:

- `docs/upstream-vectorcraft.md`: new base/target, conflict policy, sync manifest, and remaining divergences;
- `docs/ai-memory/03-current.md`: actual sync status and next task;
- `docs/ai-memory/04-known-issues.md`: newly discovered API or platform pitfalls;
- `ROADMAP.md`, `docs/gaps.md`, and parity matrices: measured changes only, without copying upstream scores as CoralCraft scores;
- `NOTICE` and `ASSETS.md`: only where actual retained notices or assets require changes.

## 10. Completion criteria

The sync is complete only when all of the following hold:

1. The target and base commits above are recorded and the integration merge contains the intended inputs.
2. The full-sync manifest has no unexplained upstream-managed difference or absence.
3. All existing CoralCraft capabilities listed in Goals have current executable checks on the integrated tree.
4. No known regression or unresolved merge marker remains.
5. The complete local milestone gate and all required remote platform checks pass on the exact merge candidate.
6. The pull request is merged to `main`, and the remaining intentional divergences and unverified limits are documented.

After that point, V4.3 starts as small CorelDRAW tool groups against the synced VectorCraft tool and property-bar architecture.
