<h1 align="center">CoralCraft</h1>

<p align="center">
  <b>Vector illustration, multipage layout, and print production; an open-source, CorelDRAW-parity application in pure Rust.</b>
</p>

<p align="center">
  A fork of <a href="https://github.com/storytold/vectorcraft"><b>VectorCraft</b></a> (MIT OR Apache-2.0) extended toward <b>CorelDRAW</b> parity (often searched as <i>CoralDraw</i>), with clean-room <code>.cdr</code> and <code>.cmx</code> format support. It keeps every VectorCraft file format working and runs natively on macOS, Windows (x64 and ARM64), Linux and FreeBSD, plus in the browser via WebAssembly.
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

**Pre-release (active Phase V development).** On 2026-10-21 CoralCraft re-based on a fork of [storytold/vectorcraft](https://github.com/storytold/vectorcraft) (`a466625`), and on 2026-11-08 completed a **full source sync** of the entire upstream history up to `36ef34df` (8,676 commits) while preserving every CoralCraft extension — all 130 merge conflicts resolved by an audited ownership ledger ([`docs/upstream-vectorcraft.md`](docs/upstream-vectorcraft.md), [`docs/upstream-divergences.tsv`](docs/upstream-divergences.tsv)). Work continues with CorelDRAW format interoperability, multi-page layout, workspace presets, and CorelDRAW-specific tools; progress is tracked in [`ROADMAP.md`](ROADMAP.md) and [`docs/gaps.md`](docs/gaps.md).

**What works today (Phases V0–V4.2 and the full VectorCraft sync complete):**

- **Native desktop & Web application (`coralcraft`, `vectorcraft-web`).** Full `eframe`/`egui` GPU/CPU desktop shell with the complete VectorCraft toolbox, ~40 dockable panels, 125 dialogs, multi-column/docking workspace management, and a built-in **`CorelDRAW` workspace preset** (`Window → Workspace → CorelDRAW`) featuring a single-column left Toolbox, top Property Bar, right vertical Color Palette bar (Left-click sets Fill, Right-click sets Outline), bottom Page Navigator bar, and right-side Object Properties / Objects / Transform / Align dockers.
- **Clean-room `.cdr` and `.cmx` read/write (`coralcraft-cdr`, `corel-import`).**
  - Opens legacy `WL` (CDR 2), bare `RIFF`/`RIFX` (CDR 3–X3), modern `ZIP+RIFF` (`content/riffData.cdr` X4–X5 and `content/root.dat` X6+), and Corel Presentation Exchange `.cmx` files directly into the VectorCraft document model, preserving lossless CorelDRAW residuals under `document.unknown["coralcraft"]`.
  - Verified against **65/65 valid real-world `.cdr` samples** in `refs/cdr-samples/` with exact page, layer, and object counts — a historical V1.4 result (pre-2026-11-08-sync tree). The current-tree verification state is tracked separately in [`docs/cdr-verification.md`](docs/cdr-verification.md), where it is currently **unverified** because the oracle tools and sample corpus are absent from the development machine.
  - Writes spec-conformant **CDR7** (`RIFF....CDR7`), **CDRX** (X4+ `ZIP+RIFF`), and **CMX1** (`RIFF....CMX1`) files from Save As, Export, CLI `convert`, and MCP `export`/`save_file`, verified against `libcdr`'s `cdr2raw` and `cmx2raw` differential oracles (historical verification; current state in [`docs/cdr-verification.md`](docs/cdr-verification.md)).
- **100% VectorCraft file format compatibility (30 formats / 46 open extensions).** Opens and saves every format supported by VectorCraft at the synced upstream commit (`.vectorcraft`, `.drawcraft`, `.vctemplate`, `.svg`, `.svgz`, `.pdf`, `.eps`, `.ps`, `.ai`, `.ait`, `.dxf`, `.emf`, `.wmf`, `.af`/`.afdesign`/`.afpub`, `.psd`/`.psb`, `.png`, `.jpg`, `.webp`, `.gif`, `.tif`/`.tiff`, `.bmp`, `.tga`, and — added by the sync — `.pcx`, `.pxr` and JPEG 2000 `.jp2`/`.j2k`), plus `.cdr`, `.cmx`, and legacy pre-fork `.coralcraft` JSON/ZIP archives. See [`docs/file-format-parity.md`](docs/file-format-parity.md).
- **CorelDRAW multi-page & master-layer model.**
  - Per-page dimensions, `Portrait`/`Landscape` orientation, and automatic horizontal spread reflow on insert, delete, resize, and drag-reorder.
  - **Master layers** (`MasterAllPages`, `MasterOddPages`, `MasterEvenPages`, configurable foreground/background) that repeat across matching pages, plus a shared non-printable **`Desktop` (pasteboard) layer**, rendered in CorelDRAW's 4-bucket layer order.
  - Bottom **Page Navigator bar** (`1 of N` counter, First/Previous/Insert Before/Page Tabs/Add After/Next/Last/Delete, double-click inline rename, drag-and-drop page reorder, right-click page context menu) and Layer Role badges (`M:All`, `M:Odd`, `M:Even`, `Desk`) in the Layers panel and Layer Options dialog.
- **Public-documentation `CorelDRAW` shortcut preset (V4.2).** `F5`–`F12` tool dispatch (`F5` Freehand, `F6` Rectangle, `F7` Ellipse, `F10` Shape…), `Ctrl+G/U/L/K/Q/D` commands, and `Alt+F7`–`Alt+F10` / `F11` / `F12` docker toggles, conflict-free beside the built-in presets.

**Active product roadmap (P0–P5; see [`ROADMAP.md`](ROADMAP.md)):**

- **P0:** Evidence baseline, current CDR verification status, and TraceDraw adoption audit.
- **P1:** Professional core vector workflow, expanding V4.3/V4.4 after adoption decisions.
- **P2:** Professional text and multipage layout.
- **P3:** PowerClip, live effects, tracing, and image workflows.
- **P4:** Colour management, prepress, and production output.
- **P5:** Automation, performance, and product hardening.

CDR/CMX interoperability, MCP/CLI, upstream syncs, and legacy retirement advance throughout these phases. The migration's V7 is not the end of the CorelDRAW product roadmap.

## Quick start

Requires **Rust 1.95+** (Edition 2024).

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
2. Linux `.deb`, `.rpm`, AppImage, and Flatpak (`packaging/linux/`); the `.deb` is install-tested in CI.
3. Windows installer and portable `.zip` for x64 (`packaging/windows/`); ARM64 packages are built, installed and launched on a native Windows ARM64 runner in CI.

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
