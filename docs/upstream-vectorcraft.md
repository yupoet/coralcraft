# Upstream VectorCraft — fork base, sync log, and local divergences

CoralCraft is a fork of [storytold/vectorcraft](https://github.com/storytold/vectorcraft)
(`MIT OR Apache-2.0`). The VectorCraft source tree is **copied into this repository and kept
as intact as possible**; every CorelDRAW capability is an *addition* (new `corel-*` crates,
`crates/cdr`, new files), never an edit inside a VectorCraft file. This keeps upstream
upgrades mergeable at any time.

## Fork base

| | |
|---|---|
| Upstream commit | `a466625` ("ci: configure Dependabot for Cargo and GitHub Actions", 2026-10-18) |
| Imported on | 2026-10-21 (task V0.1 of `docs/superpowers/plans/2026-10-21-vectorcraft-fork.md`) |
| Local reference checkout | `refs/vectorcraft` (git-ignored, never committed) |

### What was imported verbatim

- `crates/*` — all 24 VectorCraft crates, same directory and package names (`vectorcraft-*`).
- `apps/*` — `vectorcraft`, `vectorcraft-cli`, `vectorcraft-web`.
- `xtask/` — merged with ours: CoralCraft adds the `cleanroom` module (Corel copyright markers,
  binary `.cdr`/`.cmx` fixture gate) wired into `cargo xtask ci`; the four insertions in
  `xtask/src/main.rs` are each marked `// CoralCraft:`.
- `vendor/` (patched `windows-link` for the Windows 7 build), `licenses/`, `contributors/`,
  `examples/`, `packaging/`, `assets/` **minus brand assets**, `.github/` (workflows below,
  `dependabot.yml`, PR template), `clippy.toml`, `rustfmt.toml`, `Cargo.lock` (as the base).

### What was deliberately NOT imported

| Excluded | Why |
|---|---|
| `assets/app-icon/*`, `docs/brand/*`, `packaging/macos/dmg/background.*` and `DS_Store`, `docs/images/*` | VectorCraft brand assets (logo, app icon, DMG art showing it, app screenshots). CoralCraft ships its own originals (V0.3), registered in `ASSETS.md`. Their `ASSETS.md` rows were dropped at import. |
| `.github/workflows/release.yml` | Builds a "VectorCraft" draft release through the shared `storytold/craft-common-actions` workflow. CoralCraft restores a rebranded release pipeline in V0.3 (operator decision D2: macOS `.app.zip` first). |
| `docs/`, `README.md`, `ROADMAP.md`, `AGENTS.md`, `CLAUDE.md`, `NOTICE`, `ASSETS.md`, `.gitignore`, root `Cargo.toml` | CoralCraft keeps its own; `NOTICE`/`ASSETS.md` are merged (upstream rows retained minus brand), the root manifest is upstream's plus the additions below. |

### Root `Cargo.toml` divergences (all marked `# CoralCraft:`)

- `exclude = ["legacy", "fuzz"]` — the pre-fork workspace and the fuzz workspace stay out.
- `[workspace.package]` — CoralCraft version (`0.0.1`) and repository URL.
- `[workspace.dependencies]` adds `coralcraft-color`/`coralcraft-doc`/`coralcraft-geom`
  path entries into `legacy/` so `crates/cdr` keeps building until V1 re-points it at the
  VectorCraft document model.
- `crates/cdr` (CoralCraft-owned) carries its own strict `[lints]` (`indexing_slicing`,
  `arithmetic_side_effects`, …) because the root workspace now carries VectorCraft's milder
  lint set. Every other VectorCraft-derived crate inherits upstream lints unchanged.

## Sync procedure (before each V-phase)

1. `git -C refs/vectorcraft fetch origin && git log --on-line HEAD..origin/main` — review
   new upstream commits.
2. For each upstream change, merge `refs/vectorcraft/<path>` into the matching tree here.
   Files untouched by CoralCraft merge cleanly; files with `// CoralCraft:` markers need the
   markers re-applied (they are few and each is marked).
3. Never merge CoralCraft-owned paths (`crates/cdr`, `crates/corel-*`, `legacy/`, `docs/`,
   `AGENTS.md`, `ASSETS.md`, `NOTICE`) from upstream — upstream does not have them.
4. Log the sync below.

## Sync log

| Date | Upstream | Scope | Notes |
|---|---|---|---|
| 2026-10-21 | `a466625` | Initial fork import (V0.1) | Everything above; pre-fork code moved to `legacy/`. |
