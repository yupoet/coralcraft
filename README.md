<h1 align="center">CoralCraft</h1>

<p align="center">
  <b>Vector illustration, multipage layout, and print production; an open-source, modern CorelDRAW-parity application, built in pure Rust.</b>
</p>

<p align="center">
  A fast, open-source vector illustration and desktop publishing workflow targeting <b>CorelDRAW</b> parity (often searched as <i>CoralDraw</i>) with native <code>.cdr</code> and <code>.cmx</code> format support. It runs natively on macOS, Windows, Linux and FreeBSD, and in the browser via WebAssembly.
</p>

<p align="center">
  <img alt="Status: in active development" src="https://img.shields.io/badge/status-in%20active%20development-e8573f">
  <img alt="Written in pure Rust" src="https://img.shields.io/badge/pure-Rust-b83a24?logo=rust&logoColor=white">
  <img alt="Target parity: CorelDRAW (.cdr / .cmx)" src="https://img.shields.io/badge/parity-CorelDRAW%20(.cdr%20%C2%B7%20.cmx)-555555">
  <img alt="Runs on macOS, Windows, Linux, FreeBSD and the web" src="https://img.shields.io/badge/runs%20on-macOS%20%C2%B7%20Windows%20%C2%B7%20Linux%20%C2%B7%20FreeBSD%20%C2%B7%20Web-555555">
  <img alt="MCP server for agents" src="https://img.shields.io/badge/agents-MCP%20server-555555">
  <img alt="License: MIT OR Apache-2.0" src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-555555">
</p>

<p align="center">
  <a href="#why-coralcraft">Why CoralCraft</a> ·
  <a href="#quick-start">Quick start</a> ·
  <a href="#status">Status</a> ·
  <a href="#clean-room-methodology">Clean-room methodology</a> ·
  <a href="#downloads">Downloads</a> ·
  <a href="#star-history">Star history</a> ·
  <a href="#license-and-credits">License</a>
</p>

## Why CoralCraft

- **Familiar to CorelDRAW users.** Designed around the layout, tools, menus, dockers and shortcuts of **CorelDRAW** (frequently searched as *CoralDraw*): Pick, Shape (node editing), Freehand and Bézier, Smart Fill, Shaping (`Weld` / `Trim` / `Intersect`), Contour, Blend, PowerClip, Object Manager, Multipage spreads and Color Palettes.
- **Native `.cdr` and `.cmx` format support.** A dedicated clean-room format engine (`coralcraft-cdr`) for CorelDRAW `.cdr` files (from legacy `WL` v1–v2 and RIFF-based v3–X3 to ZIP+RIFF X4–2024+) and `.cmx` exchange files, progressing across three milestones:
  1. **Probe (`M1`)**: container detection (`RIFF` vs. `WL` vs. `ZIP+RIFF`), version identification, and hostile-input-safe chunk tree inspection.
  2. **Parse (`M3`)**: translating `.cdr` / `.cmx` object hierarchies, curves, fills, strokes, pages and layers into the CoralCraft document model with structured import-warning ledgers.
  3. **Write (`M5+`)**: pioneering open-source `.cdr` export for round-trip production workflows.
- **Multipage & print-first.** Unlike single-artboard illustration tools, CoralCraft's document model (`coralcraft-doc`) is built from day one around multipage documents, facing-page spreads, master pages, drawing scales (`1:10`, `1:100`), and CMYK + Spot color palettes (`.cpl` / XML) for signage, print shops, vinyl cutting, laser engraving and packaging.
- **Fast.** Multithreaded SIMD rendering off the UI thread, backed by a zero-dependency 2D geometry kernel (`coralcraft-geom`) and hybrid software/GPU rendering (`coralcraft-render`).
- **Robust.** Every parser treats `.cdr` / `.cmx` files as untrusted hostile input — `unwrap`, `expect` and `panic` are denied at the workspace level (`Cargo.toml` lints), curve booleans are exact, and undo/redo uses structural sharing.
- **Open.** A documented native format alongside `.cdr`, `.cmx`, SVG, PDF (including PDF/X), EPS, DXF/DWG and EMF/WMF exchange.
- **Agent-native.** Every menu item, tool gesture, docker and document operation is designed to be driven over a JSON control channel and an **MCP server** (`coralcraft-mcp`), so Claude Code and other AI agents can inspect, draw, lay out and export documents programmatically.
- **Everywhere.** One pure-Rust codebase for native desktop apps on macOS, Windows, Linux and FreeBSD, and the same `egui` interface in the browser via WebAssembly.

## Quick start

Requires **Rust 1.95+** (Edition 2024).

```sh
git clone https://github.com/yupoet/coralcraft
cd coralcraft

# (Optional) Populate git-ignored reference repositories in refs/ (pinned SHAs in refs/REFS.md)
./scripts/bootstrap-refs.sh

# Run the desktop app & CLI
cargo run --release -p coralcraft                           # desktop app
cargo run --release -p coralcraft-cli -- probe <file.cdr>   # inspect .cdr / .cmx container (M1)
cargo run --release -p coralcraft-cli -- probe --json <file.cdr>

# Workspace quality, layering & clean-room gates (aligned with vectorcraft xtask)
cargo xtask cleanroom                                       # scan for forbidden vendor copyright markers & fixtures
cargo xtask assets                                          # verify every asset is registered in ASSETS.md
cargo xtask layers                                          # enforce L0..L7 crate dependency layering
cargo xtask stats                                           # crate-by-crate file, line & test counts
cargo xtask ci                                              # fmt, clippy, cleanroom, assets, layers, tests
```

### Use it from Claude Code and other agents

Once the MCP stdio endpoint in `coralcraft-cli` is enabled (`M4`), register CoralCraft with Claude Code:

```sh
claude mcp add coralcraft -- /path/to/coralcraft-cli mcp
```

## Status

CoralCraft is under active development. [**ROADMAP.md**](ROADMAP.md) has the current stage, self-scored percentages by dimension and milestone criteria; [`docs/gaps.md`](docs/gaps.md) tracks the prioritized work queue; and [`docs/superpowers/plans/`](docs/superpowers/plans/2026-10-19-m0-bootstrap-m1-cdr-probe.md) contains the active step-by-step TDD implementation plan.

**Where we are (2026-10-19): pre-alpha (`M0` complete, `M1` in progress).** The 12-member Rust workspace (`crates/*`, `apps/*`, `xtask`), strict layering (`L0`–`L7`), clean-room and asset verification gates (`cargo xtask cleanroom`, `cargo xtask assets`, `cargo xtask layers`), reference repository bootstrap (`scripts/bootstrap-refs.sh`), and persistent AI memory (`docs/ai-memory/`) are in place. Next up in `M1` is turning the red-light TDD anchors in `crates/cdr/tests/probe_smoke.rs` green and publishing [`docs/target-app-parity.md`](docs/gaps.md) and [`docs/file-format-parity.md`](docs/gaps.md).

**What's missing:**
- `M1` CDR/CMX container probe (`RIFF` / `WL` / `ZIP+RIFF` detection, version map, hostile-input fuzzing) and initial parity matrices (`docs/target-app-parity.md`, `docs/file-format-parity.md`);
- `M2` interactive canvas, `Pick` / `Shape` / `Bezier` / rectangle / ellipse tools, boolean shaping (`Weld` / `Trim` / `Intersect`), and Undo/Redo history;
- `M3` real-world `.cdr` object tree parsing (`Page` / `Layer` / `Curve` / `Rectangle` / `Ellipse` / `Text` + fill/stroke) and multipage spread switching;
- `M4` live effects (Contour, Blend, Envelope, Extrude, PowerClip), artistic & paragraph text, PDF/X & SVG export, and the `coralcraft-mcp` server;
- `M5+` open-source `.cdr` writer/exporter and a modern scripting/automation replacement for CorelDRAW VBA macros.

**Where we're going:** immediate focus is **M1 (`coralcraft-cdr` probe + parity matrices)** following strict red-green-refactor TDD in [`docs/superpowers/plans/2026-10-19-m0-bootstrap-m1-cdr-probe.md`](docs/superpowers/plans/2026-10-19-m0-bootstrap-m1-cdr-probe.md). The ranked backlog is in [`docs/gaps.md`](docs/gaps.md).

**Workspace:** `crates/{geom, color, doc, cdr, render, tools, engine, ui-egui, mcp}` and `apps/{coralcraft, coralcraft-cli}`. The `egui` frontend is isolated in its own `L6` crate (`coralcraft-ui-egui`), enforced by `cargo xtask layers`, so the UI can be swapped without touching the core engine.

Agent and contributor rules (clean-room protocol, license boundaries, asset policy, zero panics in shipped code, TDD gates) are in [`AGENTS.md`](AGENTS.md) and [`CLAUDE.md`](CLAUDE.md). Every bundled asset is registered with its license in [`ASSETS.md`](ASSETS.md).

## Clean-room methodology

CoralCraft follows the clean-room engineering discipline established by [storytold/vectorcraft](https://github.com/storytold/vectorcraft), extended with a dual-track protocol for `.cdr` / `.cmx` format interoperability (see [`AGENTS.md`](AGENTS.md)):

1. **Behavior track (public documentation only):**
   - Application features, workflows, dockers and shortcuts are derived **exclusively from public documentation** (official online user manuals, published books, public tutorials and forum discussions, cited by URL).
   - Contributors and AI agents **never** run, script, automate, screenshot, measure or disassemble CorelDRAW or any Corel software, and **never** use Corel software to create test files.
2. **Format track (public specifications & owned samples):**
   - Binary format support is written from scratch in safe Rust using public specifications ([Photopea CDR Format Specification](https://github.com/photopea/CDR-specification), [Document Liberation Project / libcdr documentation](https://wiki.documentfoundation.org/DLP/Libraries/libcdr), RIFF and ISO 32000 specs) and synthetic byte fixtures authored directly in code.
   - No GPL/AGPL code (such as Inkscape, UniConvertor or sK1) is ever copied or linked. MPL-2.0 references (`libcdr`) are consulted strictly as read-only format documentation, with chunk-level findings recorded in `docs/reverse/`.

> [!NOTE]
> **Trademark Disclaimer:** CoralCraft is an independent open-source project and is **not** affiliated with, endorsed by, or sponsored by Corel or Alludo. **CorelDRAW** is a registered trademark of its respective owner and is referenced here solely for descriptive interoperability and feature-parity comparison.

## Downloads

Pre-built binaries for **Windows** (`x64` / `arm64`), **macOS** (universal `.dmg`), **Linux** (`AppImage`, `Flatpak`, `.deb`, `.rpm`, `.tar.gz`), **FreeBSD** and **Web (WebAssembly)** will be published on [GitHub Releases](https://github.com/yupoet/coralcraft/releases) starting with the first interactive milestone (`M2`/`M3`). Until then, build from source with `cargo build --release`.

## Star history

[![Star History Chart](https://api.star-history.com/svg?repos=yupoet/coralcraft&type=Date&legend=top-left)](https://www.star-history.com/?repos=yupoet%2Fcoralcraft&type=date&legend=top-left)

## License and credits

CoralCraft is dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option. Required notices are in [NOTICE](NOTICE), and every bundled asset is listed with its author, source and license in [ASSETS.md](ASSETS.md).

CoralCraft's workspace architecture, crate layering (`L0`–`L7`), agent-native MCP design, and clean-room engineering methodology are modeled after [VectorCraft](https://github.com/storytold/vectorcraft) and the [ArtCraft Crafting Apps](https://getartcraft.com/apps) family (`DesignCraft`, `CADCraft`, `PhotoCraft`, `PdfCraft`).
