# 03 — Current Milestone Status, Active Tasks & Next Steps

> Last updated: 2026-10-19

## 1. Current Stage Status

- **Stage**: `pre-alpha` (**M0 and M1 are 100% delivered and passing all CI gates**; ready to start **M2**).
- **Completed in This Cycle**:
  1. **Five-Way Independent Plan Review**:
     - Conducted independent reviews across `zcode-delegate` (GLM-5.3), `kimi-delegate` (Kimi K3), `grok-delegate` (Grok 4.6), `codebuddy-delegate` (DeepSeek V4.1 Flash), and internal **`claude-opus-5-5-high` (Claude Opus 5.5)**, synthesized in [`docs/ai-memory/05-ref-docs/04-m1-plan-four-way-review.md`](05-ref-docs/04-m1-plan-four-way-review.md).
     - All unique Opus 5.5 findings were incorporated into M1 code and architecture docs: big-endian `RIFX` for Mac CMX, `content/root.dat` + `content/dataFileList.dat` pairing in X6+ ZIP containers, CDR v3 space version byte (`"CDR "` -> `300`) and 2024/2025 (`'Q'`/`'R'`), M3 pre-baked geometry fallback, and exact `0.1 μm` (`1/254,000 inch`) integer units.
  2. **M1: CDR/CMX Format Probe Vertical Slice (Tasks 9–13 Green)**:
     - [`crates/cdr/src/error.rs`](../../crates/cdr/src/error.rs): `#[non_exhaustive]` `ProbeError` and `ProbeWarning` with `serde::Serialize` / `Deserialize`.
     - [`crates/cdr/src/riff.rs`](../../crates/cdr/src/riff.rs): `inspect_riff` supporting `RIFF` (LE) and `RIFX` (BE), `MAX_DEPTH = 32`, `MAX_CHUNKS = 100_000`, `u64` + `checked_add` overflow protection, `vrsn` chunk extraction, `LIST cmpr`/`LIST stlt` opaque payload skipping, and EOF missing pad tolerance.
     - [`crates/cdr/src/zip.rs`](../../crates/cdr/src/zip.rs): Dependency-free ZIP EOCD + Central Directory scanner (`MAX_ZIP_ENTRIES = 10_000`) distinguishing X4–X5 (`content/riffData.cdr`), X6+ (`content/root.dat` + `content/dataFileList.dat`), and non-CDR `.zip`/`.docx` archives, with automatic inner RIFF inspection for `Stored` (`method == 0`) entries.
     - [`crates/cdr/src/lib.rs`](../../crates/cdr/src/lib.rs): `probe() -> Result<ProbeReport, ProbeError>` covering Legacy `WL`, bare `RIFF`/`RIFX`, and modern `ZIP+RIFF` containers; 22 unit tests green including a 10,000-iteration deterministic hostile mutation test (`hostile_random_mutations_never_panic`).
     - [`apps/coralcraft-cli/src/main.rs`](../../apps/coralcraft-cli/src/main.rs) & [`apps/coralcraft-cli/tests/cli_probe.rs`](../../apps/coralcraft-cli/tests/cli_probe.rs): `coralcraft-cli probe [--json] <path>` with 512 MiB file size guard and standardized exit codes (`0/2/3/4/5`); 4 integration tests green and **64/64 (100%)** real-world `.cdr` samples in `refs/cdr-samples/` passing.
     - [`xtask/src/main.rs`](../../xtask/src/main.rs): Added unit tests for `cleanroom` and `assets` gates and hardened directory traversal against following symlinks (Task 12).
     - Established [`docs/target-app-parity.md`](../target-app-parity.md), [`docs/file-format-parity.md`](../file-format-parity.md), [`docs/reverse/log-format.md`](../reverse/log-format.md) (`REV-0001`), and updated [`ROADMAP.md`](../../ROADMAP.md) and [`docs/gaps.md`](../gaps.md) (Task 13).
  3. **English-Only Convention Applied**:
     - Converted all source comments, docstrings, error/warning messages, CLI/xtask logs, Cargo manifest descriptions, and repository documentation (`AGENTS.md`, `CLAUDE.md`, `ROADMAP.md`, `ASSETS.md`, `NOTICE`, `docs/gaps.md`, `refs/REFS.md`) to English.

## 2. Next Up (M2: Multi-Page Document Model + Native `.coralcraft` Format + First Frame Render)

1. **`coralcraft-doc` Multi-Page Publication Model**:
   - `Document`, `Spread`, `Page` (per-page dimensions and bleed), document-level `Layer` stack + cross-page `Desktop` layer.
   - Stable `ObjectId(u64)` handles and lossless `Object::Unsupported { kind, raw_ref, fallback_bounds }` passthrough node.
2. **`.coralcraft` Native Format & Property-Tested Round-Trip**:
   - Single-file JSON and ZIP container (`manifest.json` + `document.json` + `blobs/`) verified with `proptest` (`load(save(doc)) == doc`).
3. **`coralcraft-render` First Frame Software Rasterizer**:
   - Static page PNG rendering via `tiny-skia` + `coralcraft-cli render <input> -o <out.png>`.
