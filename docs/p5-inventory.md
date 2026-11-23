# P5 Inventory — Representative Documents and Measured Baselines

> Frozen 2026-11-23 by Task 1 of
> [`docs/superpowers/plans/2026-11-22-p5-automation-performance-hardening.md`](superpowers/plans/2026-11-22-p5-automation-performance-hardening.md).
> Every number below is a measured value from this machine, recorded verbatim from the runs named
> beside it. Nothing is extrapolated, and **no new performance threshold is claimed here**: the
> only budgets cited are the harness's pre-existing ones. Regression budgets for the classes are
> set in a later task only from repeated stable baselines.

## 1. Entry gate

P4 is marked **Delivered 2026-11-23** in `ROADMAP.md` (`2edc59aa0`), so P5 may run. S2/P0.1 stay
open. All measurements ran serially on this development machine — **x86_64 macOS, 16 cores,
release profile** — with the 1-minute load average recorded per run; the `perf` harness flags a
machine busy above 75% of cores and no run below was flagged noisy.

## 2. The three representative project classes

All three classes are **synthetic**: deterministic command sequences built by the harness itself
(`apps/coralcraft-cli/src/perf.rs`, `perf --class <name>`, committed with this baseline). No
operator-owned or restricted document is committed; when operator files are measured later they
stay local and are recorded by shape and SHA-256 only.

| Class | Definition (what the builder makes) | Measured shape (identical every run) | Saved bytes |
|---|---|---|---|
| `illustration` | A many-object vector illustration: one 420×300 page, 2,400 shapes (rect/ellipse/star grid, deterministic colours, every fifth at 80% opacity), 40 guides, 5 symbols × 20 placed instances, 30 text objects | **2,535 objects (30 text), 14,400 anchors, 1 artboard, 1 layer** | 1,689,837 |
| `brochure` | A multipage brochure: 4 mixed-size pages (420×300 + 300×400 + 420×300 + 300×300), 12 rectangles + one body text per page, a `masterAll` master layer with the header, four placed 512×512 PNG bitmaps, one 3×4 table, and one CDR export→import round trip leaving the CoralCraft residual block | **69 objects (16 text, 4 images of 512×512), 192 anchors, 4 artboards, 5 layers** | 31,648,787 |
| `poster` | A P3/P4 feature-dense poster: one 420×300 CMYK page with a nested PowerClip, a 10-step Blend, a 4×4 mesh with a half-opacity point, a two-colour pattern fill, a texture fill, 50% transparency, drop shadow + extrude + zig-zag effects, a live traced 96×96 bitmap, a spot swatch with overprint, 9 pt bleed, and separations print setup | **21 objects (1 text, 1 image), 56 anchors, 3 effect entries, 1 artboard, 1 layer** | 181,349 |

**Hash note (measured):** the saved-byte SHA-256 is **not stable across runs** for `illustration`
and varies occasionally for `poster` (the native format carries per-save identifiers), while the
byte *length* and every count are identical run to run (`brochure`'s hash happened to repeat).
Equivalence checks must therefore compare structure and length, not native bytes; the class
definition (the committed builder at a commit) is the identity.

## 3. Stock baseline — `perf --paths 10000` (run `plan/p5-t1-baseline-stock.log`, load 4.5)

| Measurement | Measured | Harness budget | |
|---|---:|---:|---|
| render: fit page, 2880×1800 | 20.41 ms | 100 ms | ok |
| render: background refresh after a pan | 21.94 ms | 100 ms | ok |
| render: 400% zoom, 2880×1800 | 9.63 ms | 16 ms | ok |
| hit test (per click) | 0.29 ms | 2 ms | ok |
| save .vectorcraft | 21.71 ms | 300 ms | ok |
| open .vectorcraft | 71.57 ms | 300 ms | ok |
| export SVG | 77.89 ms | 500 ms | ok |
| Pathfinder Unite, 1,000 paths | 94.98 ms | 150 ms | ok |
| handle drag, 5,000-anchor path (per move) | 0.77 ms | 8 ms | ok |
| segment drag, 5,000-anchor path (per move) | 0.96 ms | 8 ms | ok |

All ten budgets pass at 10,000 paths.

## 4. Deep baseline — `perf --deep` (50,000 paths; run `plan/p5-t1-baseline-deep.log`, load 3.6)

Tracked timings (ms): Pathfinder exclude 15.09, minusFront 13.10, minusBack 13.28, divide 22.08,
trim 17.28, merge 90.44, crop 13.69, outline 14.90; type one character into 20,000-character area
text 22.01; hover over empty canvas 40.13; hover with all selected 150.19; hit-test miss 13.21;
marquee 400×300 22.50; smart-guide targets in view 6.64; `select.toggle` in a 25,000-object
selection 12.09; render 4 embedded 24 MP photos with 3 renderers (cold) 1019.25; import SVG with
one embedded 2145 KB PNG 25.00.

Memory (resident, MB): start 520 → document built 562 → **after 45 whole-selection edits 1655**
(the undo history holds whole-document snapshots of a 50k-path document — a measured Task 2
candidate) → after live effects 606 → after Pathfinder 605 → after typing 610 → after pointing
614 → with 4 embedded 24 MP photos 797 → after rendering them with 3 renderers 1229 → after an
embedded-picture import 1241 → after the SVG export and save→open→save 1300.

Budget rows: fit render 94.21 ms (100, ok), pan refresh 97.26 ms (100, ok), **400% zoom
28.70 ms (16, OVER)**, hit test 0.31 ms (ok), save 101.10 ms (ok), **open 361.43 ms (300,
OVER)**, export SVG 392.41 ms (500, ok), Pathfinder Unite 92.18 ms (150, ok), handle drag
4.53 ms (ok), segment drag 5.67 ms (ok). **Two pre-existing budgets are exceeded at 50,000
paths** — recorded as the first measured optimization candidates for Task 2 (zoomed-render
culling and native open), not fixed here.

The CorelDRAW integration battery on the same tree: **137 passed / 0 failed**
(`plan/p5-t1-coreldraw.log`).

## 5. Per-class scenario measurements (median of 3 runs; range in parentheses)

Runs `plan/p5-t1-classes.log` (serial, one process per run). `--` = below the 0.005 ms floor.

| Scenario | illustration | brochure | poster |
|---|---:|---:|---:|
| build (commands, once) | 1,753 ms (1,742–1,775) | 252 ms (248–298) | 12 ms (11–12) |
| open (load saved bytes) | 21.68 (20.97–22.54) | 388.83 (386.04–462.45) | 1.06 (0.93–1.24) |
| idle render 2880×1800 (warm) | 14.29 (13.86–14.42) | 19.84 (19.66–23.72) | 18.04 (15.57–18.85) |
| hit test (per click) | 0.27 (0.26–0.27) | 0.01 | — |
| select.all | 0.77 (0.77–0.80) | 0.02 | 0.01 |
| move the whole selection | 7.92 (7.79–7.92) | 0.20 (0.19–0.24) | 0.10 (0.09–0.11) |
| undo / redo the move | 0.77 / 0.76 | 0.02 / 0.02 | 0.01 / 0.01 |
| pointer drag (per move) | 10.38 (10.27–10.67) | 0.21 (0.20–0.25) | 0.12 (0.12–0.13) |
| save .vectorcraft | 12.35 (11.56–12.77) | 900.85 (897.23–934.98) | 2.58 (2.34–2.58) |
| export SVG | 22.38 (22.10–23.36) | 2.72 (2.17–3.03) | 2.15 (2.09–2.22) |
| export PDF | 88.30 (87.48–91.62) | 207.62 (204.86–297.01) | 32.63 (30.58–32.91) |
| export CDR | 19.75 (17.91–20.02) | 522.53 (474.85–595.08) | 2.02 (1.70–2.02) |
| resident memory after the suite | 162 MB (158–164) | 1,091 MB (1,066–1,096) | 101 MB (101–105) |

Measured observations for Task 2 (candidates, not conclusions): the **brochure's native
save (~0.9 s), CDR export (~0.5 s), open (~0.4 s), and ~1.1 GB resident set** all track its four
512×512 bitmaps inside a 31.6 MB file — image-heavy native persistence is the largest measured
class cost; the illustration's whole-selection move (7.9 ms) and drag (10.4 ms) track its 2,535
objects; the 50k-path deep run's zoomed render and open exceed their budgets as recorded in §4.

## 6. Recovery-save and startup

The recovery store's save/restore costs and true application startup are measured in Task 4
(crash recovery) with its own harness; no number is claimed here.

## 7. Thresholds policy

**No new budget is set by this document.** The class medians above come from three runs on one
machine in one session; the harness's pre-existing budgets (§3–§4) are the only thresholds
cited. Task 2 may set class regression budgets only after repeated stable baselines with their
variance recorded, and every later claim must name its document class, machine, profile, and
runs.

## 8. Task 2 optimization record (2026-11-23)

**Candidate selection (measured, not guessed).** The zoom-render budget overrun was profiled
first (`perf --bench zoom`, 29.17 ms median): the samples sit inside `vello_cpu`
(`rasterize_region`, `vectorize_avx2`) — an external rasterization dependency, outside the
crates this plan may touch, so it is recorded and skipped. The largest bottleneck in our own
code was then measured: the **engine's native save of the brochure class**, 924.43 ms median
(1,073/1,117 in later probes) against 149.87 ms for the format layer alone — a ~775 ms
difference owned by `crates/engine`+`crates/corel-import`.

**Root cause (measured composition of the 31.6 MB `coralcraft` extension block):**
`object_meta` 15,796,815 chars + `master_layers` 15,748,085 chars = 99.8% of the block, while
the two entries `sync_coralcraft_extension` actually rewrites (`units`, `pages`) total **759
chars**. The old sync cloned the whole block's `Value`, parsed it all
(`from_value`), and re-serialized it all (`to_value`) on **every** native save (and on every
page-edit caller).

**The change** (`perf:` commit): `sync_coralcraft_extension` now splices only the `units` and
`pages` entries into the existing `Value` in place; the megabyte residuals are never parsed.
Guard tests: a new equivalence regression asserts the spliced block is string-identical to the
whole-block round trip (and that a rename through the engine path keeps page residuals and the
palette), the V2.2 lossless-extension test and the full Corel integration battery (137/137)
pass, and the brochure's saved length is unchanged (31,648,787 bytes).

**Measured result (same benchmark, median of 3 runs each):**

| | before | after | |
|---|---:|---:|---|
| `document.serialize vectorcraft`, brochure | 924.43 ms (924–1,117 across runs) | **343.43 ms** (340.12/343.43/349.57) | 2.7× |
| `save_file` (format layer, control) | 129–150 ms | 127–129 ms | unchanged |

The improvement (~580 ms per save, ~2.7×) is far beyond the observed run-to-run noise (±25 ms).
Every CDR-origin document benefits on every save and every page-affecting edit.

## 9. Task 6–7 record: platform packages and the closing gate (2026-11-23)

**Packages built and launched on the actual target (this machine, macOS x86_64, darwin 25.6):**

- `packaging/macos/package.sh --arch x86_64` exited 0 and produced the signed `CoralCraft.app`
  (staged in `target/macos-package/`), `dist/release/coralcraft-0.0.1-macos-x86_64.dmg`, and
  `dist/release/coralcraft-cli-0.0.1-macos-x86_64.zip`. The `.app.zip` was made exactly as the
  release workflow does (`ditto -c -k --keepParent`) and unpacked into a scratch folder.
- Archive identity: `codesign -dv` reports `Identifier=io.github.yupoet.coralcraft`,
  `Signature=adhoc` (`flags=0x10002(adhoc,runtime)`) — the documented local-testing signature;
  Gatekeeper on other Macs would reject it, which the script header states. No notarization is
  claimed.
- Launch: the unpacked app's binary `--version` → `coralcraft 0.0.1`; the CLI zip's
  `coralcraft-cli --version` → `coralcraft-cli 0.0.1`; `packaging/check-app-specific.sh` → ok.
- The real GUI acceptance: the installed `CoralCraft.app` was launched as a process with
  `--new-instance --control <port>` and its preferences redirected into a temp
  `CORALCRAFT_CONFIG_DIR`; over the JSON-lines channel it created a document, added a rectangle
  and the text "Installed launch proof", and exported an SVG holding that text (573 bytes) and a
  PDF (11,135 bytes), then terminated cleanly. The developer's app and preferences were never
  touched.
- **Other platforms ride their CI, not this machine**: the macOS universal DMG + `.app.zip`
  (release.yml), Linux `.deb` (with the Debian 11 glibc baseline smoke), `.rpm`, AppImage,
  Flatpak, Windows x64 installer/portable + ARM64, FreeBSD, and the wasm web bundle are built and
  installed by their workflows; this record does not claim any of them as locally validated.

**Task 7 Step 1 — the per-class production chains** (`2ee27f4c6`): for each representative class,
`representative_production_chain_per_class` runs sustained mixed edits (text edit → move → undo
of the move), a native save/reopen that keeps the edit, an SVG export, a PDF/X-4 export, a CDR
export whose warnings all belong to the named degradation families (live expansion, merge modes,
mesh opacity, dropped overprint, relock, texture, live trace, pattern — nothing unexplained), and
the recovery chain (a modified copy in a `MemoryStore`, the saving app going away, a fresh session
restoring every copy with the last edit intact). The other Step-1 pieces are the committed
suites: sustained editing (`coreldraw_p5_hardening`), automation (`p3_p4_surface…`,
`…run_through_run_command`), the real crash/restart (`apps/coralcraft/tests/it/recovery_restart`),
and the installed-package launch above.

**Final gate (2026-11-23):** `cargo xtask ci` — fmt ok, clippy ok (`-D warnings`), workspace
tests stopped solely at the documented platform-flake known-issue 0b: engine library **4,378
passed / 1 failed / 2 ignored**, recorded verbatim, not silenced. **Not a clean full-CI pass.**
All 16 skipped later gates then ran serially and exited 0: doctests, cleanroom (2,661 files),
no-bundled-fonts, the four feature trees, no-default UI Clippy, install-fonts, pdf-deps, assets
(309), brands, cursors, i18n-report, layers (30 crates), wasm. Measured tree: `cargo xtask stats`
→ **2,063 files, 893,582 lines, 14,871 `#[test]`, 66 `proptest!`**.

**Honest limits at close:** known-issue 0b stays red on this host (green in CI); S2/P0.1 (real
libcdr oracle + operator samples) remain open as independent certification work; the zoom-render
budget overrun at 50k paths was profiled into `vello_cpu` (external dependency, outside the
optimizable crates) and stays recorded in §4/§8; only macOS x86_64 is locally
package-validated. **P5 closes; the roadmap P0–P5 is complete on this branch pending S2.**
