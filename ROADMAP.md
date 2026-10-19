# CoralCraft Roadmap

> Baseline stage: **pre-alpha (M1 delivered)** · Established: 2026-10-19 · Updated: 2026-10-19
> Methodology: Dual-track clean-room public specification & format reverse engineering (see [`AGENTS.md`](AGENTS.md)).
> Self-scoring follows `vectorcraft`'s `target-app-parity` convention: 0–100% per dimension, updated on every completed task; see [`docs/target-app-parity.md`](docs/target-app-parity.md) and [`docs/file-format-parity.md`](docs/file-format-parity.md).

| Dimension | Current | Target (usable alpha) |
| --- | ---: | ---: |
| Workspace & engineering gates (`workspace`/`xtask`/CI/cleanroom) | 100% | Maintain |
| CDR format: `probe` (container/version/chunk walk) | **100%** (WL / RIFF+RIFX / X4+ ZIP+RIFF, CLI `probe`, 64/64 real-world samples passing) | M1: 100% (achieved) |
| CDR format: `parse` (object tree -> document model) | 0% | M3: modern RIFF baseline |
| CDR format: `write` | 0% | Long-term campaign (M5+) |
| Multi-page document model & native `.coralcraft` round-trip | 0% | M2: 100% (property-tested) |
| Rendering (first frame -> interactive frame rate) | 0% | M2 first frame / M4 interactive |
| Tools, panels & UI vertical slice | 0% | M4: Pick/Shape/Pen/Text/Pages/Palette |
| Print & prepress production (bleed/imposition/spot/PDF-X) | 0% | M5+ |
| MCP / agent-driven illustration | 0% | M5 |
| CorelDRAW workflow parity | 5% (parity matrix baseline established) | M1 parity matrix baseline (achieved) |

## Completed (M0–M1, Tasks 1–13)

1. **M0 Workspace Skeleton**: 12-crate layered workspace (`L0..L7`), `xtask` (`ci`, `cleanroom`, `assets`, `layers`, `stats`), and 11 pinned reference repositories in `refs/`.
2. **M1 CDR/CMX Format Probe Vertical Slice**:
   - `coralcraft-cdr`: Supports Legacy `WL`, bare `RIFF`/`RIFX`, and modern `ZIP+RIFF` (X4–X5 `content/riffData.cdr` and X6+ `content/root.dat` + `content/dataFileList.dat`) container generations; decodes `' '` / `'3'`..`'9'` / `'A'`..`'R'` ASCII version bytes and `vrsn` chunks; skips `LIST cmpr` / `LIST stlt` as opaque payloads; passes a 10,000-iteration deterministic hostile mutation test (zero `unsafe`, zero `unwrap`).
   - `coralcraft-cli probe`: Supports human-readable output and `--json` machine-readable output (exit codes `0/2/3/4/5`), passing **64/64 (100%)** real-world `.cdr` samples in `refs/cdr-samples/`.
   - Established [`docs/target-app-parity.md`](docs/target-app-parity.md), [`docs/file-format-parity.md`](docs/file-format-parity.md), and [`docs/reverse/log-format.md`](docs/reverse/log-format.md) (entry `REV-0001`).

## Next Up (M2: Multi-Page Document Model + Native `.coralcraft` Format + First Rendered Frame)

1. `coralcraft-doc`: Multi-page publication model (`Document` / `Spread` / `Page` / document-level `Layer` stack + shared `Desktop` layer / stable `ObjectId` handles / lossless `Object::Unsupported` passthrough node).
2. `.coralcraft` native format: ZIP (`manifest.json` + `document.json` + `blobs/`) and single-file JSON mode with `proptest` round-trip verification.
3. `coralcraft-render`: Static page rasterization via `tiny-skia` (PNG output + `coralcraft-cli render`).

## Mid-Term (M3–M4)

- M3: CDR parser v1 (`LIST cmpr` dual-stream decompression + `page`/`layr`/`obj ` parsing + pre-baked geometry fallback) with structured import warning ledger.
- M4: `egui` desktop shell + interactive tool vertical slice + CMYK/spot color palettes (`.cpl`/`.xml` import).

## Long-Term (M5+)

- MCP server, CMX import/export, broader CDR version bands, CDR writer campaign, prepress production, and scripting engine.

## Changelog

- 2026-10-19: M1 delivered (`coralcraft-cdr` 3-generation container walker, `coralcraft-cli probe`, `xtask` gate unit tests, format & workflow parity baseline docs, first reverse log entry `REV-0001`); reviewed across 5 independent models (GLM-5.3, Kimi K3, Grok 4.6, DeepSeek V4.1 Flash, Claude Opus 5.5).
- 2026-10-19: Repository initialized; M0 skeleton (Tasks 1–8) and M1 red-light anchors delivered.
