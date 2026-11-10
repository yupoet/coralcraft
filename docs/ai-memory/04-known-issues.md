# 04 — Known Issues, Pitfalls & Workarounds

## 0h. [Resolved 2026-11-11, P3 Task 10] CDR live objects silently exported as empty paths

- `corel-import::convert_vc_node_to_cdr` previously sent Blend/Envelope/Mesh and other live kinds to its fallback with `node.path_data().cloned().unwrap_or_default()`, which could write an empty path without a warning.
- Regression `coreldraw_p3_persistence::cdr_live_blend_export_warns_and_keeps_visible_vector_geometry` failed with `warnings == []` before the fix. Live nodes now use the existing `vectorcraft_doc::live::expand_deep`, and `encode_all_of` reports which live kinds become plain vector groups in CDR/CMX. Native sources remain editable; the self-round trip is supplementary, not independent CDR certification. S2 remains open.

## 0g. P2 text/layout boundaries (2026-11-11)

- Dynamic page numbers are dedicated text objects marked by `NodeExtras::page_number`; their stored text is `#`, with evaluation only at canvas/SVG/PDF output. Arbitrary inline fields in longer text runs are not implemented.
- CDR/CMX exports split dynamic page numbers into ordinary editable per-page text and emit a warning; the dynamic field and master semantics remain native-only. The public-spec `stlt` Font Encoding / `font` Encoding ID records are not yet linked to text runs; `crates/cdr/src/text.rs` supplies bounded decoders but no byte-based guessing is allowed.
- `layout.table` creates ordinary groups of editable cell paths and live text with at most 1000 cells. It is not a dedicated interactive table tool.
- S2 stays open: self-skipping oracle/sample tests are not real CDR certification.

## 0. VectorCraft Sync (2026-11-07): API drift map for the 36ef34d merge

- **`egui` fork (rev 6d518fc) renamed accessibility types**: `egui::WidgetType::SelectableLabel`/`Button`/`Label` → `egui::Role::*`, and `ui.close_menu()` → `ui.close()`. CoralCraft UI modules (e.g. `page_navigator.rs`) must use the `Role` API.
- **`craft_keymap::Overrides` replaced `BTreeMap<String, String>`**: shortcut presets build with `Overrides::new()` + `assign(&mut Overrides, …)`; `preset()` returns `Option<Overrides>`. Multi-binding values are `Vec<String>`.
- **Docking layouts must contain the canvas**: `panel_docking::valid` rejects any `Layout` without the `canvas` panel; build workspace presets through `panel_docking::with_canvas(columns, floating)`.
- **`vectorcraft_doc::Document::unknown` is copy-on-write `Shared<Map>`**: reads go through `Deref`; mutations through `to_mut()`/`DerefMut` — the `sync_coralcraft_extension` hook in `fileio/native.rs` already matches this.
- **New required struct fields** (serde defaults, must be set in struct literals): `Artboard` `locked/show_video_safe_areas/video_ruler_pixel_aspect_ratio(1.0)/color(None)`; `Gradient` `dither(false)/method(GradientMethod::Classic)`; `LiveShape::Rectangle|Polygon` `radii_relative(false)`; `TextObject` `ruby/first_para_continues/system_layout/fractional_widths/list_count`; `ImageObject` `pixels(None)`; `Unit` gained `Q|Ha` (quarter-mm → `CdrUnit::Mm`).
- **Shallow `--depth=1` fetches break `merge-base`**: fetching the sync base and target with `--depth=1` leaves them as shallow boundaries; deepen from the local `refs/vectorcraft` clone with `--depth=10000` before the graft and merge.
- **`git merge-tree --name-only` overcounts conflicts** (~386 vs the real merge's 130): it lists per-stage entries; the authoritative conflict inventory is `git diff --name-only --diff-filter=U` from the actual merge.

## 0b. Platform: `constrain_angle_turns_the_base_orientation_of_polygons_and_stars` fails on x86_64 macOS only

- **Symptom**: the engine test asserts a polygon drawn at Constrain Angle 360° equals the 0° one byte-for-byte; on this x86_64 macOS host the anchors differ by 1 ulp (`50.00000000000002` vs `50.0`).
- **Ground truth**: the test fails identically on the pristine upstream target `36ef34df` checked out standalone on this machine — it is libm sensitivity inherited from upstream, not a sync regression. Upstream's own CI (and ours) run on ubuntu / windows / arm64-macOS where the values come out exact.
- **Handling**: not "fixed" locally (that would diverge engine files upstream owns); the sync PR's platform checks are the authoritative green signal for it.

## 0c. Pre-existing: legacy `docs_sync` test cannot find its fixture (fails at 8208a6c1e too)

- **Symptom**: `cargo test --manifest-path legacy/Cargo.toml` fails `docs_mcp_md_tool_list_matches_live_registry` — it reads `legacy/docs/mcp.md`, which does not exist after the fork moved the pre-fork tree into `legacy/`.
- **Ground truth**: the failure reproduces on the pre-sync baseline `8208a6c1e`, so it predates this sync (the fixture moved at V0.1).
- **Handling**: out of the sync's scope (legacy code is frozen until V7); recorded here so the milestone gate accounts for it as pre-existing.

## 0f. [Resolved 2026-11-11, P1 review] Marquee isolation and Outline Pen solid/error handling

- `select.marquee` passed `None` instead of the active isolation scope to `hit::marquee`, allowing object selection outside the isolated object. Regression: `coreldraw_node_workflow::marquee_selection_respects_the_isolation_scope` failed with both objects selected before the fix and passes with `st.isolation` forwarded.
- Outline Pen Solid omitted `dash`, leaving an existing dash pattern unchanged; `confirm` now writes `dash: null`. Invalid colours are validated before the stroke edit, and colour-command errors propagate instead of closing as success. Regressions: `solid_clears_an_existing_dash_pattern` and `an_invalid_colour_is_refused_without_an_outline_edit` in `crates/ui-egui/src/dialogs/outline_pen.rs`.
- The P1 SVG evidence now includes independent `usvg` parsing, and the acceptance PDF is opened through the Hayro-backed reader. Text-only export assertions and self-reimport are not described as independent evidence by themselves.
- The review's full CI run stopped at known-issue 0b and skipped later gates, so each skipped gate was run separately. A P0 divergence-ledger target-product name failed `cargo xtask brands`; its description was made neutral and the rerun passed. `no-bundled-fonts` is registered only as a CI step, not a direct xtask command; its tests, Clippy, and feature-tree checks were run explicitly from `xtask/src/main.rs`.

## 0e. [Resolved 2026-11-10, P1 Task 11] CDR7 multi-page round trip drops the first page

`write_cdr7` (`crates/cdr/src/parse.rs`, `write_riff_document`) round-trips a 3-artboard document
back as 2 pages: the first (default-named) artboard is lost, pages 2..N and the master layer's
role survive, and page sizes come back in mm-as-points units (200 pt reads 70.5556). Reproducer:
`coreldraw_pages::pages_and_master_layers_survive_the_daily_round_trips` in
`crates/engine/tests/it/coreldraw_pages.rs`. **Fix:** the reader's master-page heuristic
(`crates/cdr/src/parse.rs`) skipped a first `LIST page` with no objects even when it carried its
own `pmta` page name; it now skips only unnamed first pages, so a named first page round-trips
(`write_cdr7_roundtrips_multiple_pages`).

## 0d. P0 evidence baseline: two parity documents, the MCP docs generator, and CDR verification state (2026-11-10)

- **Two parity documents, two different targets**: [`docs/vectorcraft-target-app-parity.md`](../vectorcraft-target-app-parity.md) is upstream VectorCraft's **Adobe Illustrator** parity evidence (preserved verbatim after the sync); [`docs/target-app-parity.md`](../target-app-parity.md) is CoralCraft's **CorelDRAW** D1-D6 evidence matrix (levels 0-5, no aggregate percentage). Never cite one as the other; VectorCraft-derived docs link to the former, CoralCraft docs to the latter.
- **The MCP docs generator is `CORALCRAFT_BLESS=1 cargo test -p vectorcraft-mcp --test docs_sync`** (crate is `vectorcraft-mcp` since the 2026-11-08 sync; the old `coralcraft-mcp` command no longer exists). `crates/mcp/tests/docs_sync.rs` fails when `docs/mcp.md`'s generated tool list or documented protocol surface is stale.
- **Current CDR oracle/sample state is UNVERIFIED**: on the 2026-11-10 development machine `cdr2raw` and `cmx2raw` are not installed and `refs/cdr-samples` is absent, so all six checks in [`docs/cdr-verification.md`](../cdr-verification.md) are UNVERIFIED (self-skipping tests are not passes). This keeps S2 and P0.1 open and pins the CDR/CMX row of the parity matrix at level 2; the 65/65 sample result is historical (V1.4) only.
- **TraceDraw's baked-versus-live effects documentation conflict is resolved from source**: TraceDraw's `architecture.md` says effects bake; `features.md` says live. Source and tests show the **live** model is implemented (`live.rs` effect stack evaluated at render/export, 42 tests, CDR `kept_effects`/`kept_blends` round trips); the architecture doc is stale. Conclusion and adoption consequence (map onto VectorCraft effect stacks, do not port `live.rs`) are recorded in [`docs/ai-memory/05-ref-docs/07-tracedraw.md`](05-ref-docs/07-tracedraw.md) and [`docs/coreldraw-reference-adoption.md`](../coreldraw-reference-adoption.md).

## 1. [Resolved] Short SHAs in `scripts/bootstrap-refs.sh` Rejected by GitHub `git fetch --depth 1`

- **Symptom**: 7-character short SHAs in `scripts/bootstrap-refs.sh` caused `git fetch -q --depth 1 origin <short-sha>` to fail with `fatal: couldn't find remote ref`, triggering the fallback branch clone.
- **Resolution**: Upgraded all 11 entries in [`scripts/bootstrap-refs.sh`](../../scripts/bootstrap-refs.sh) and [`refs/REFS.md`](../../refs/REFS.md) to full 40-character commit SHAs.

## 2. `cargo xtask cleanroom` Scope Covers `docs/` (and Brand Name Rules)

- **Symptom**: [`xtask/src/main.rs`](../../xtask/src/main.rs) `cleanroom()` recursively scans `crates/`, `apps/`, and `docs/` for `.cdr`/`.cmx` file extensions and 3 prohibited Corel copyright strings.
- **Rules**:
  1. Never place `.cdr` or `.cmx` files inside `crates/`, `apps/`, or `docs/` (construct synthetic fixtures in memory or write temporary files to `std::env::temp_dir()`).
  2. Never write the 3 prohibited copyright strings in `crates/`, `apps/`, or `docs/`. In `xtask` unit tests, construct test strings at runtime (`format!("{} (c) {}", "Copyright", "Corel")`).
  3. **Brand Name Boundary**: Following `refs/vectorcraft/xtask/src/brands.rs`, Markdown documentation (`README.md`, `ROADMAP.md`, `docs/*.md`) may reference `CorelDRAW` (and `CoralDraw`) for public positioning and parity matrices, while shipped UI strings use CoralCraft's own terminology.

## 3. `refs/uniconvertor` AGPL-3.0 Contamination Boundary

- **Symptom**: `refs/uniconvertor/` is cloned locally by `bootstrap-refs.sh`, but is licensed under **AGPL-3.0**.
- **Rule**: Agents must **never** open or read source files under `refs/uniconvertor/`.

## 4. `storytold/craftrules` Repository Returns 404

- **Symptom**: `https://github.com/storytold/craftrules` referenced in `refs/vectorcraft/AGENTS.md` returns 404.
- **Workaround**: Reference `refs/vectorcraft/AGENTS.md`, `refs/designcraft/`, `refs/cadcraft/`, and `refs/craft-libs/` directly.

## 5. `agy --model claude-opus-5-5-high` Content Filter & Headless Tool Notes

- **Symptom**: When invoking `agy -p --model claude-opus-5-5-high`:
  1. Repeating exploit terminology (such as "depth bomb" / "zip bomb") can trigger the Vertex AI content safety filter (`finishReason: OTHER`).
  2. Calling interactive tools like `read_url_content` in `-p` headless mode returns `Tool execution was denied`.
- **Workaround**: Use neutral engineering terminology ("nesting depth limit", "compression ratio limit") and instruct the model to read local files only without calling `read_url_content`.

## 6. Clippy `unwrap_used` / `expect_used` in Integration Tests (`tests/*.rs`)

- **Symptom**: `clippy.toml`'s `allow-unwrap-in-tests = true` only applies inside functions annotated with `#[test]`. Helper structs/methods in `tests/*.rs` trigger `clippy::expect_used` unless allowed at the module/crate level.
- **Workaround**: Add `#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]` at the top of integration test files (`tests/*.rs`) or on `#[cfg(test)] mod tests`.

## 7. [Resolved] `serde_json` Requires `float_roundtrip` Feature for Bit-Exact `F64` Round-Trip

- **Symptom**: Default `serde_json` float parser had a 1-ULP discrepancy on extreme float exponents (e.g., `1.5787512879013381e-215`) during `F64` round-trip property testing.
- **Resolution**: Enabled `serde_json = { version = "1", features = ["float_roundtrip"] }` in the workspace [`Cargo.toml`](../../Cargo.toml), which uses Rust's standard library Eisel-Lemire exact float parser with zero additional dependencies.

## 8. [Resolved] CIE 1931 2° D65 White Point Z Coordinate (`1.08883`)

- **Symptom**: Using `0.98883` instead of `1.08883` for the CIE 1931 2° D65 white point Z coordinate in Bradford chromatic adaptation causes `LabD50(100, 0, 0)` to map to sRGB `[255, 255, 243]`.
- **Resolution**: Set `D65_WHITE = [0.95047, 1.00000, 1.08883]` and `D50_WHITE = [0.96422, 1.00000, 0.82521]` in [`crates/color/src/spaces.rs`](../../crates/color/src/spaces.rs), yielding exact `[255, 255, 255]` white-point mapping and `< 0.01` $\Delta E$ Bradford round-trip accuracy.

## 9. [Resolved] Dotted `.workspace = true` Keys in `xtask layers` Parser

- **Symptom**: Writing `serde.workspace = true` in a crate's `Cargo.toml` produced a raw key `"serde.workspace"` in `xtask`'s line-based dependency parser before stripping `.workspace`.
- **Resolution**: [`xtask/src/main.rs`](../../xtask/src/main.rs) `parse_dep_names` now strips any `.workspace` suffix before checking `coralcraft-*` crate layers.

## 10. Project review 2026-10-20: verification pitfalls (read before claiming anything is done)

- **Symptom**: Earlier sessions shipped false claims:
  - an invented README MCP tool table, where 162 names did not exist;
  - README CLI commands that did not exist;
  - "verified" installers that do not install: `.rpm` with no files, a fake `.flatpak` format, Linux-built `.dmg`/`.pkg` without HFS+ or Bom, a FreeBSD tarball holding a Linux musl binary with a patched OSABI byte, and a hollow Web bundle;
  - Linux binaries requiring GLIBC_2.39.
  
  `cargo xtask ci` was green throughout, because none of these were tested.
- **Rules**:
  1. Any tool or command name written in docs must come from the code. `docs/mcp.md` is generated and checked by `CORALCRAFT_BLESS=1 cargo test -p vectorcraft-mcp --test docs_sync`.
  2. A package counts as verified only after it has been installed and launched on its target OS: container install tests for Linux, a native runner for macOS, Windows and FreeBSD. Never hand-roll binary package formats (dmg, pkg, rpm, flatpak, AppImage). Use the platform tools on native CI runners, as `refs/vectorcraft/packaging` does.
  3. Linux release binaries must target an old glibc baseline (`cargo zigbuild --target x86_64-unknown-linux-gnu.2.17`) or musl.
  4. Never delete untracked files you did not create without asking. The `.github/` release workflow was deleted once by mistake.
- **Open review findings**: see the reopened queue at the top of [`docs/gaps.md`](../gaps.md).

## 11. `coralcraft-cli` panics on a closed stdout (broken pipe)

- **Symptom**: `coralcraft-cli probe file.cdr | head -n 2` panics with `failed printing to stdout: Broken pipe`, because `println!` panics on EPIPE.
- **Fix (open)**: write through a locked `stdout` with `writeln!`, and treat `ErrorKind::BrokenPipe` as a normal exit.

## 12. MCP stdio host is `coralcraft-cli mcp` (also `coralcraft --mcp`)

- Both binaries call `McpServer::serve_stdio`. Known protocol gaps (notifications get responses, no `ping`, empty `inputSchema` for most tools) are tracked as R4.1 and R4.2 in `docs/gaps.md`.

## 13. VectorCraft fork pitfalls (Phase V, 2026-10-21)

- **Lint mismatch**: our workspace denies `indexing_slicing`, `arithmetic_side_effects`, `unwrap_used`, `expect_used`, `panic` and forbids `unsafe_code`. VectorCraft code does not follow all of these, and some platform modules (macOS window, fonts) may need `unsafe`. Use per-crate `[lints]`: VectorCraft-derived crates keep VectorCraft's settings; CoralCraft-owned crates (`cdr`, `corel-*`, apps) keep the strict set. Do not mass-edit upstream code just to satisfy our lints.
- **Clean-room scan**: `cargo xtask cleanroom` must keep blocking Corel strings and `.cdr`/`.cmx` binaries. VectorCraft legitimately mentions Adobe/Illustrator; that is not a violation. Adjust the scanner allow-list rather than editing upstream text.
- **Brand assets**: VectorCraft's logo, splash, and app icons must not ship in CoralCraft. Every remaining file under `assets/` needs an `ASSETS.md` row.
- **Upstream mergeability**: keep upstream directory and package names (`vectorcraft-*`). Rebrand only user-visible strings, app IDs, and bundle IDs. Put large CorelDRAW features in `corel-*` crates instead of editing upstream files.
- **Fork base**: the imported fork base remains `a466625`; the local `refs/vectorcraft` reference is newer and pinned in `refs/REFS.md`. Record the base commit and every later sync in `docs/upstream-vectorcraft.md`.
- **Post-V0.1 state quirks**:
  1. Two workspaces share the repository tree: the main one and `legacy/` (plus `fuzz/`). Run legacy checks with `--manifest-path legacy/Cargo.toml`; `legacy/target/` is git-ignored.
  2. `crates/cdr` still builds against `legacy/crates/{color,doc,geom}` through root workspace path deps; V1 replaces those with the VectorCraft document model. Until then CDR tests run in the main `cargo test --workspace` (our `ci.yml` still installs `libcdr-tools` for them).
  3. The merged `ASSETS.md` holds upstream's rows minus brand rows; `cargo xtask assets` verifies them. `hayro-*` rows describe pinned git-dependency data, not files in this tree.
  4. `refs/vectorcraft` is itself a git checkout: **always `cd /data/coralcraft` (absolute path) before repo-wide commands** — a stray `git mv` once ran inside `refs/` and had to be reset.
  5. Upstream CI workflows (`freebsd`, `packaging-lint`, `print`, `windows7`, `windows-arm64`, `affinity-corpus`) were copied and rebranded where they name the app; `nix` was dropped (no flake at the fork base). The freebsd/windows-arm64 jobs call shared `storytold/craft-common-actions` reusable workflows at `@main` — their internals are upstream's and may name vectorcraft.
  6. After the V0.3 rebrand, **library-level env vars keep upstream names** (`VECTORCRAFT_SYNC_RENDER`, `VECTORCRAFT_LOCALE`, `VECTORCRAFT_SYSTEM_CURSORS`, `VECTORCRAFT_I18N_DUMP*`, `VECTORCRAFT_SYNC_OPEN`, `VECTORCRAFT_TRACE_CAPTURE`); app-level ones are `CORALCRAFT_*`. `vectorcraft-cli` keeps its binary/package name until V0.4 but already reads `CORALCRAFT_*` prefs env vars (it shares the app's `profile.rs`).
  7. macOS format identity stays upstream's on purpose: the UTI `ai.storyteller.vectorcraft.document`, the `application/x-vectorcraft` MIME, the "VectorCraft (Document|Template)" filter labels, and `.vectorcraft`/`.drawcraft`/`.vctemplate` extensions (D7). Don't "rebrand" those in future sweeps.

## 14. [Resolved] CI regressions identified in October 2026

- **Compound insertion invariant**: Paste in Front / Paste in Back and Draw Behind / Inside used a raw selected compound member as the insertion anchor, allowing new art inside a compound or its member. Resolve anchors through `selected_roots`. Draw Inside mask copies recursively reassign ids with `d.reid`, not only the root id. Keep deterministic regressions in `tests_clipboard.rs` and `tests_file.rs`.
- **32-bit oversized values**: write explicit `u64` counts into serialized native JSON before opening it. Do not saturate the in-memory `usize` first, because that skips the reader's `u64 > usize::MAX` branch on 32-bit targets. Direct layout tests use explicit `u64` cases followed by saturating conversion. Kernel bitmap fixtures must also be generated for the target's word size instead of reusing a 64-bit bitmap on i686. Sampling arithmetic and PDF offset limits use checked/wide arithmetic, and TIFF overflow expectations follow the target's `usize` limit. Oversized raster files keep their original bytes and header dimensions; Place/Open report them without forcing a decode or losing the linked object. On 32-bit targets, the PNG decoder's output-buffer sizing can overflow even for a dimensions-only call, so both the raster loader and its warning path read the fixed-position IHDR fields instead. PDF predictor columns use target-independent `i64` parsing; an out-of-range value must be rejected rather than treated as the default of one column. EMF bitmap-header offsets use the checked `slice` helper, never unchecked `offset + header-size` arithmetic on `usize`. Tracing applies the same PNG-header fallback and checked pixel-index arithmetic. AccessKit node order is not stable across targets; tests must compare same-labelled slider values without assuming tree order. The i686 workflow runs tests serially because concurrent UI fixtures can exhaust the 32-bit process address space.
- **Linux package baseline**: build with cargo-zigbuild for glibc 2.31, make nfpm metadata declare that floor, and install with `apt` on Debian 11 so dependencies are resolved. The MCP smoke pipeline runs under `bash -eo pipefail`. Cargo writes the target directory without the `.2.31` suffix; package staging must use `x86_64-unknown-linux-gnu`. Debian 11 uses archive.debian.org after the main repositories stop serving its payloads; the minimal container has no CA certificates, so APT uses HTTP with signed repository metadata.
- **CI runner environment**: a pip-installed Zig is not necessarily on PATH; add its package directory explicitly. Remove only unused Android, .NET, and CodeQL tools from the Ubuntu runner when the workspace build exhausts the disk, and record free space before and after.
- **nfpm installation**: the retired `gosu-nfpm.sh` URL returned 404 and the workflow pipeline did not fail. Install the fixed Go module release with strict shell error handling and verify the binary.
- **Package identity**: the ARM64 smoke test must use `app-dir: CoralCraft`, matching the MSI. Linux desktop entries and Windows portable instructions must use CoralCraft names and executable paths; `packaging/check-app-specific.sh` guards them. The Windows 7 portable script requires `docs/windows7.md`; host tests alone never establish Windows 7 support.
