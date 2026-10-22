<h1 align="center">CoralCraft</h1>

<p align="center">
  <b>Vector illustration, multipage layout, and print production; an open-source, CorelDRAW-parity application in pure Rust.</b>
</p>

<p align="center">
  A fork of <a href="https://github.com/storytold/vectorcraft"><b>VectorCraft</b></a> (MIT OR Apache-2.0) extended toward <b>CorelDRAW</b> parity (often searched as <i>CoralDraw</i>), with clean-room <code>.cdr</code> and <code>.cmx</code> format support. It keeps every VectorCraft file format working and runs natively on macOS, Windows, Linux and FreeBSD, plus in the browser via WebAssembly.
</p>

<p align="center">
  <img alt="Status: pre-release, in active development" src="https://img.shields.io/badge/status-pre--release-e8573f">
  <img alt="Written in pure Rust" src="https://img.shields.io/badge/pure-Rust-b83a24?logo=rust&logoColor=white">
  <img alt="Target parity: CorelDRAW (.cdr / .cmx)" src="https://img.shields.io/badge/parity-CorelDRAW%20(.cdr%20%C2%B7%20.cmx)-555555">
  <img alt="MCP automation server" src="https://img.shields.io/badge/MCP-automation%20server-0969da">
  <img alt="License: MIT OR Apache-2.0" src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-555555">
</p>

<p align="center">
  <a href="#status">Status</a> ·
  <a href="#quick-start">Quick start</a> ·
  <a href="#mcp-server-for-agents">MCP server</a> ·
  <a href="#downloads">Downloads</a> ·
  <a href="#clean-room-methodology">Clean-room methodology</a> ·
  <a href="#license-and-credits">License</a>
</p>

## Status

**Pre-release (active Phase V development).** On 2026-10-21 CoralCraft re-based on a fork of [storytold/vectorcraft](https://github.com/storytold/vectorcraft) (`a466625`) and is extending it with CorelDRAW format interoperability, multi-page layout, workspace presets, and CorelDRAW-specific tools. Progress is tracked in [`ROADMAP.md`](ROADMAP.md), [`docs/gaps.md`](docs/gaps.md), and [`docs/superpowers/plans/2026-10-21-vectorcraft-fork.md`](docs/superpowers/plans/2026-10-21-vectorcraft-fork.md).

**What works today (Phases V0–V4.1 complete):**

- **Native desktop & Web application (`coralcraft`, `vectorcraft-web`).** Full `eframe`/`egui` GPU/CPU desktop shell with 89 interactive canvas tools, ~40 dockable panels, 125 dialogs, multi-column/docking workspace management, and a built-in **`CorelDRAW` workspace preset** (`Window → Workspace → CorelDRAW`) featuring a single-column left Toolbox, top Property Bar, right vertical Color Palette bar (Left-click sets Fill, Right-click sets Outline), bottom Page Navigator bar, and right-side Object Properties / Objects / Transform / Align dockers.
- **Clean-room `.cdr` and `.cmx` read/write (`coralcraft-cdr`, `corel-import`).**
  - Opens legacy `WL` (CDR 2), bare `RIFF`/`RIFX` (CDR 3–X3), modern `ZIP+RIFF` (`content/riffData.cdr` X4–X5 and `content/root.dat` X6+), and Corel Presentation Exchange `.cmx` files directly into the VectorCraft document model, preserving lossless CorelDRAW residuals under `document.unknown["coralcraft"]`.
  - Verified against **65/65 valid real-world `.cdr` samples** in `refs/cdr-samples/` with exact page, layer, and object counts.
  - Writes spec-conformant **CDR7** (`RIFF....CDR7`), **CDRX** (X4+ `ZIP+RIFF`), and **CMX1** (`RIFF....CMX1`) files from Save As, Export, CLI `convert`, and MCP `export`/`save_file`, verified against `libcdr`'s `cdr2raw` and `cmx2raw` differential oracles.
- **100% VectorCraft file format compatibility (27 formats / 34 open extensions).** Opens and saves every format supported by VectorCraft (`.vectorcraft`, `.drawcraft`, `.vctemplate`, `.svg`, `.svgz`, `.pdf`, `.eps`, `.ps`, `.ai`, `.dxf`, `.dwg`, `.emf`, `.wmf`, `.fig`, `.sketch`, `.afdesign`, `.png`, `.jpg`, `.webp`, `.gif`, `.bmp`, `.tif`, `.tga`, `.ico`, `.psd`, `.ase`, `.aco`, `.csv`), plus `.cdr`, `.cmx`, and legacy pre-fork `.coralcraft` JSON/ZIP archives.See [`docs/file-format-parity.md`](docs/file-format-parity.md).
- **CorelDRAW multi-page & master-layer model.**
  - Per-page dimensions, `Portrait`/`Landscape` orientation, and automatic horizontal spread reflow on insert, delete, resize, and drag-reorder.
  - **Master layers** (`MasterAllPages`, `MasterOddPages`, `MasterEvenPages`, configurable foreground/background) that repeat across matching pages, plus a shared non-printable **`Desktop` (pasteboard) layer**, rendered in CorelDRAW's 4-bucket layer order.
  - Bottom **Page Navigator bar** (`1 of N` counter, First/Previous/Insert Before/Page Tabs/Add After/Next/Last/Delete, double-click inline rename, drag-and-drop page reorder, right-click page context menu) and Layer Role badges (`M:All`, `M:Odd`, `M:Even`, `Desk`) in the Layers panel and Layer Options dialog.

**Remaining Phase V roadmap (`V4.2`–`V7.1`; see [`docs/gaps.md`](docs/gaps.md)):**

- **V4.2–V4.4:** Public-documentation CorelDRAW shortcut preset (`F5`–`F12`, `Ctrl+G/U/L/K/Q`, `Alt+F7`–`F10`), CorelDRAW toolbox & context-sensitive Property Bar modes, and CorelDRAW docker refinements.
- **V5.1–V5.4:** PowerClip container workflow (Place Inside, Edit Contents, Extract, Lock Contents), interactive effects with on-canvas handles, Corel `.cpl`/`.xml` palette import/export in the palette bar and Swatches panel, and Smart Fill / Connector / Dimension / PowerTRACE integration.
- **V6.1–V6.2:** CoralCraft-specific MCP tools merged into the VectorCraft MCP server and regenerated [`docs/mcp.md`](docs/mcp.md).
- **V7.1:** Retirement of the pre-fork `legacy/` workspace directory.

## Quick start

Requires **Rust 1.85+** (Edition 2024).

```sh
git clone https://github.com/yupoet/coralcraft
cd coralcraft

# Launch the native CoralCraft desktop application
cargo run --release -p coralcraft
cargo run --release -p coralcraft -- <file.cdr|file.vectorcraft|file.svg>

# Inspect, convert, and script documents from the CLI
cargo run --release -p coralcraft-cli -- probe <file.cdr>            # container, version, chunk tree (--json available)
cargo run --release -p coralcraft-cli -- info <file.cdr>             # document dimensions, artboards, layers, objects
cargo run --release -p coralcraft-cli -- convert <file.cdr> out.svg  # .svg .pdf .eps .dxf .png .cdr .cmx .vectorcraft
cargo run --release -p coralcraft-cli -- run -c 'file.new {"width":800,"height":600}' -o out.cdr

# Start the MCP server on stdio
cargo run --release -p coralcraft-cli -- mcp
# or: cargo run --release -p coralcraft -- --mcp

# Workspace quality, layering & clean-room gates
cargo xtask ci          # fmt, clippy -D warnings, test, cleanroom, no-bundled-fonts, pdf-deps, assets, brands, cursors, layers, wasm
cargo xtask stats       # crate-by-crate file, line & test counts
```

## MCP server for agents

`coralcraft-cli mcp` (or `coralcraft --mcp`) runs a Model Context Protocol server (JSON-RPC 2.0 on stdio) backed by `vectorcraft-mcp` and `vectorcraft-engine`. Register it with Claude Code:

```sh
cargo build --release -p coralcraft-cli
claude mcp add coralcraft -- "$PWD/target/release/coralcraft-cli" mcp
```

It exposes structured tools (`open_file`, `save_file`, `export`, `render`, `inspect`, `get_selection`, `list_commands`, `run`, `batch`, etc.) and over 400 engine commands covering document/page/artboard/layer management, live shapes, boolean pathfinder operations, Bézier node editing, typography, gradients/meshes/patterns, vector effects, image tracing, and multi-format import/export (including `.cdr` and `.cmx`). See [`docs/mcp.md`](docs/mcp.md).

## Downloads

GitHub Actions builds native packages via [`release.yml`](.github/workflows/release.yml) and verifies Linux `.deb` installation in [`ci.yml`](.github/workflows/ci.yml):

1. macOS universal `.dmg` and `.app.zip` (`packaging/macos/package.sh`).
2. Linux `.deb`, `.rpm`, AppImage, and Flatpak (`packaging/linux/`).
3. Windows installer and portable `.zip` (`packaging/windows/`).

Until the next tagged release is published, build from source with `cargo build --release -p coralcraft`.

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

## Star history

[![Star History Chart](https://api.star-history.com/svg?repos=yupoet/coralcraft&type=Date&legend=top-left)](https://www.star-history.com/?repos=yupoet%2Fcoralcraft&type=date&legend=top-left)

## License and credits

CoralCraft is dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option. Required notices are in [NOTICE](NOTICE), and every bundled asset is listed with its author, source and license in [ASSETS.md](ASSETS.md).

CoralCraft is a fork of [VectorCraft](https://github.com/storytold/vectorcraft) in the [ArtCraft Crafting Apps](https://getartcraft.com/apps) family (`DesignCraft`, `CADCraft`, `PhotoCraft`, `PdfCraft`).
