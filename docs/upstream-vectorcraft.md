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

### Source divergences forced by the brand rule (V0.2)

VectorCraft embeds its brand icon in two places; both now point at CoralCraft's original icon
(`assets/app-icon/coralcraft-*.png`, rendered by `packaging/icons-coral.py`, registered in
`ASSETS.md`). Each edit is marked `// CoralCraft:`:

- `crates/ui-egui/src/brand.rs` — the 128 px app-bar mark.
- `apps/vectorcraft/src/main.rs` — the window icon (256 px / macOS 512 px full-bleed).

Other small marked edits: `xtask/src/layers.rs` registers `coralcraft-cdr` (L3, format layer)
in the dependency table.

### V0.3 rebrand (2026-10-21)

The desktop app is CoralCraft; the library crates stay upstream's. Rename surfaces, all marked
`// CoralCraft:` where non-mechanical:

- `apps/vectorcraft` → `apps/coralcraft`; package and binary `coralcraft`. The CLI keeps the
  upstream name `vectorcraft-cli` until V0.4; `vectorcraft-web` keeps its upstream name.
- App identity strings: window title, About box, app menu (`ui-egui` `APP_NAME`), `--version`
  output, log file name and prefixes, error/recovery dialogs, MCP `serverInfo`
  (`coralcraft`/`CoralCraft`), the default shortcut preset (`CoralCraft Defaults`), prefs
  folder (`~/Library/Application Support/CoralCraft`, `%APPDATA%\CoralCraft`,
  `~/.config/coralcraft`), portable-mode folder (`CoralCraftData`) and markers.
- App-level env vars: `CORALCRAFT_CONFIG_DIR`, `CORALCRAFT_NO_PREFS`,
  `CORALCRAFT_CONTROL_PORT`, `CORALCRAFT_AUTOMATION_{READ,WRITE}_ROOT`,
  `CORALCRAFT_IN_WINDOW_MENUS`, `CORALCRAFT_GPU_SKIP`, `CORALCRAFT_REQUIRE_WINRES`,
  `CORALCRAFT_{VERSION,BUILD_SHA,BUILD_DATE,MAINTAINER}` (packaging). **Library-level env
  vars keep upstream names** (`VECTORCRAFT_SYNC_RENDER`, `VECTORCRAFT_LOCALE`,
  `VECTORCRAFT_SYSTEM_CURSORS`, `VECTORCRAFT_I18N_DUMP*`, `VECTORCRAFT_SYNC_OPEN`,
  `VECTORCRAFT_TRACE_CAPTURE`): they are upstream tooling interfaces inside library crates.
- Format identity is untouched: `.vectorcraft`/`.drawcraft`/`.vctemplate` extensions, the
  "VectorCraft"/"VectorCraft Template" file-filter labels, `application/x-vectorcraft`, and
  the macOS UTI `ai.storyteller.vectorcraft.document` all stay (D7: they name the upstream
  file format, which CoralCraft opens and saves).
- App/bundle ID: `ai.storyteller.vectorcraft` → `io.github.yupoet.coralcraft` (desktop file,
  metainfo, flatpak manifests, `Info.plist` `CFBundleIdentifier`). The macOS document-type
  names stay "VectorCraft Document/Template" (format names).
- `crates/engine/src/cmd/help.rs`: help links point at `github.com/yupoet/coralcraft`;
  the Discord stays the upstream ArtCraft community, labeled as such.
- `vectorcraft-mcp` reports itself as `coralcraft`/`CoralCraft` in `initialize`.
- Icons: original CoralCraft artwork (`packaging/icons-coral.py` renders the 128/256/512 PNGs,
  the hicolor set as `io.github.yupoet.coralcraft`, `.icns`, `.ico`, and the DMG background);
  registered in `ASSETS.md`. `packaging/icons.sh` and `check-app-specific.sh` are rewritten for
  them (upstream's versions rendered VectorCraft's own artwork). The DMG ships without a
  positioned `.DS_Store` layout for now (background only); Windows MSI GUIDs are fresh ones.
- `packaging/macos/package.sh` keeps upstream's flow; the `.DS_Store` copy is conditional.
  `ci.yml` re-enables the Linux `.deb` install smoke test; `release.yml` (D2: macOS first)
  builds the universal DMG + CLI zip plus a plain `.app.zip` via `packaging/macos/package.sh`.
- `apps/coralcraft` inherits the **upstream** lint set (the plan's "apps keep strict lints"
  line predates the import; mass-editing upstream app code to satisfy stricter lints would
  break mergeability, which known-issue #13 forbids). CoralCraft-authored crates that replace
  app code (the V0.4 CLI) carry the strict set.

### V0.4 CLI rebase (2026-10-21)

`apps/vectorcraft-cli` → `apps/coralcraft-cli` (package and binary `coralcraft-cli`). The
upstream CLI is kept as the base (manual arg parsing, mcp/run/convert/info/bench/perf); the
pre-fork CLI's `probe` subcommand is ported as `apps/coralcraft-cli/src/probe.rs`
(`coralcraft-cli probe FILE [--json]`, exit 3 = not CDR/CMX, 5 = broken container, 4 = I/O,
2 = usage), with `coralcraft-cdr` added to the workspace dependencies. Its tests
(`tests/cli_probe.rs`) build synthetic RIFF/ZIP byte streams in the temp dir, so no binary
fixtures enter the tree (clean-room). All `vectorcraft-cli` references across the tree follow
the rename (binary names in packaging, workflows, the layer table, doc comments).

### V1.1 `corel-import` bridge crate (2026-10-22)

- Added CoralCraft-owned `crates/corel-import` (`corel-import` in root `Cargo.toml` and registered at L3 in `xtask/src/layers.rs` with `INTRA_LAYER_ORDER = &["coralcraft-cdr", "format", "corel-import"]`).
- Maps `coralcraft_doc::Document` ↔ `vectorcraft_doc::Document` and preserves lossless residuals (`CoralCraftDocData`) under `Document::unknown["coralcraft"]`. No VectorCraft crate source files were modified in V1.1.

### V1.2 `.cdr`/`.cmx` in Open/Place tables and MCP `open_file` (2026-10-22)

- `crates/engine/Cargo.toml` adds `coralcraft-cdr` and `corel-import` dependencies; `crates/mcp/Cargo.toml` adds `corel-import` dev-dependency (all marked `# CoralCraft:`).
- `crates/engine/src/cmd/fileio/mod.rs` appends `cdr` and `cmx` entries to `FORMATS`, `OPEN_EXTS`, and `PLACE_EXTS` (marked `// CoralCraft:`).
- `crates/engine/src/cmd/fileio/load.rs` sniffs CDR/CMX containers via `coralcraft_cdr::probe` in `detect` and loads `.cdr`/`.cmx` via `corel_import::import_cdr_bytes` in `load_with` (marked `// CoralCraft:`).
- `packaging/macos/Info.plist.in` declares `.cdr` and `.cmx` document types so `the_macos_bundle_opens_every_readable_format` passes.

### V1.3 CDR7 / CDRX / CMX1 writers in Save As and Export (2026-10-22)

- `crates/engine/src/cmd/fileio/mod.rs` sets `write: true` on `cdr` (with `CDR_OPTIONS` for `cdrVersion: "cdr7" | "cdrx"`) and `cmx`, and lists `cdr|cmx` in `document.serialize`, `document.export`, and `document.exportSelection` (marked `// CoralCraft:`).
- `crates/engine/src/cmd/fileio/save.rs` appends `"cdr"` and `"cmx"` to `SAVE_FORMATS` and updates `document.save` and `file.saveAs` specs (marked `// CoralCraft:`).
- `crates/engine/src/cmd/fileio/encode.rs` encodes `"cdr"` (`cdr7` or `cdrx`) and `"cmx"` (`cmx1`) via `corel_import` (marked `// CoralCraft:`).
- `crates/engine/src/cmd/fileio/screens.rs` excludes `"cdr"` and `"cmx"` from `Export for Screens` rows (marked `// CoralCraft:`).
- `crates/ui-egui/src/io.rs` asks `"cdr"` options on `SaveMode::SaveAs` only, matching `"vectorcraft"` and `"ai"`.
- `crates/mcp/src/tools.rs` mentions `cdr` and `cmx` in the `export` tool description.

### V1.4 `libcdr` differential oracle and real-sample checks on the VectorCraft pipeline (2026-10-22)

- `crates/corel-import/src/lib.rs` adds `CdrImportMetrics` and `document_metrics(&vectorcraft_doc::Document)` for structural verification (`page_count`, `layer_count`, `object_count`).
- `crates/corel-import/tests/oracle_and_samples.rs` and `crates/engine/src/cmd/fileio/tests.rs` verify `cdr2raw`/`cmx2raw` differential oracle acceptance and exact page/layer/object counts across all real-world `.cdr` samples in `refs/cdr-samples`.

### V2.1 Compatibility matrix test over every `FORMATS` entry (2026-10-22)

- `crates/engine/src/cmd/fileio/tests.rs` adds `vectorcraft_format_compatibility_matrix_covers_every_format_v2_1` gating all 24 upstream VectorCraft formats plus `cdr` and `cmx` in `FORMATS` and testing export/serialize/open across every format and extension.

### V2.2 `.vectorcraft` / `.drawcraft` / `.vctemplate` lossless round-trip with `"coralcraft"` extension block (2026-10-22)

- `crates/corel-import/src/lib.rs` adds `sync_coralcraft_extension(&mut vectorcraft_doc::Document)` and re-exports `coralcraft_color`, `coralcraft_doc`, and `coralcraft_geom`.
- `crates/engine/src/cmd/fileio/native.rs` synchronizes `document.unknown["coralcraft"]` on save when present (marked `// CoralCraft:`), leaving pure VectorCraft documents untouched.

### V2.3 Legacy `.coralcraft` (pre-fork JSON/ZIP) import converter (2026-10-22)

- `crates/corel-import/src/lib.rs` adds `is_legacy_coralcraft` and `import_legacy_coralcraft_bytes`, decoding both plain JSON (`coralcraft_doc::Document::load_json`) and Stored/Deflated ZIP containers (`document.json`) into `vectorcraft_doc::Document` with lossless `"coralcraft"` extension metadata.
- `crates/engine/src/cmd/fileio/mod.rs` appends `reader("coralcraft", "CoralCraft (Legacy)", &["coralcraft"], "application/x-coralcraft", false)` to `FORMATS` and `"coralcraft"` to `OPEN_EXTS` and `PLACE_EXTS` (marked `// CoralCraft:`), keeping upstream `.vectorcraft`/`.drawcraft`/`.vctemplate` entries untouched.
- `crates/engine/src/cmd/fileio/load.rs` detects and loads `"coralcraft"` with `converted = true` (marked `// CoralCraft:`), so saving upgrades legacy documents to `.vectorcraft`.
- `packaging/macos/Info.plist.in` declares `<string>coralcraft</string>` under imported document types.

### V2.4 `docs/file-format-parity.md` rewrite (2026-10-22)

- Rewrote `docs/file-format-parity.md` to cover all 27 entries in `FORMATS` (24 upstream VectorCraft formats plus `cdr`, `cmx`, and legacy `coralcraft`), Corel `.cpl`/`.xml` palettes, and V1/V2 verification gates; updated post-V2 measured metrics in `ROADMAP.md`.

### CI divergences

- Our `ci.yml` (not upstream's — upstream has no `ci.yml` at this commit) runs
  `cargo xtask ci` on Linux/macOS/Windows; V0.2 adds the `wasm32-unknown-unknown` toolchain
  target (the ci chain's `wasm` step needs it) and keeps the `libcdr-tools` install (the CDR
  differential tests still run through `crates/cdr`).
- `linux-package-smoke` was disabled at V0.2 and re-enabled at V0.3 with the rebranded
  packaging (it installs the `.deb` on Debian 11 and runs both binaries).
- Upstream workflows copied and rebranded in V0.3 where they name the app: `freebsd`,
  `windows-arm64` (`app-name: coralcraft`), `windows7`, `print`, `packaging-lint`,
  `affinity-corpus`. Dropped: `nix` (no `flake.nix` at this fork base), upstream
  `release.yml` (shared craft-common-actions pipeline builds a "VectorCraft" draft release);
  our own `release.yml` (D2: macOS first) builds the universal DMG + `.app.zip`.
- `legacy/` crates reach `crates/cdr` through direct `path = "../../../crates/cdr"` deps
  (Cargo forbids workspace-dependency paths outside the workspace root), so the legacy
  workspace still builds standalone.

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
