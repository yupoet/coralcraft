<h1 align="center">CoralCraft</h1>

<p align="center">
  <b>Vector illustration, multipage layout, and print production; an open-source, CorelDRAW-parity application in pure Rust.</b>
</p>

<p align="center">
  A fork of <a href="https://github.com/storytold/vectorcraft"><b>VectorCraft</b></a> (MIT OR Apache-2.0) extended toward <b>CorelDRAW</b> parity (often searched as <i>CoralDraw</i>), with clean-room <code>.cdr</code> and <code>.cmx</code> format support. It keeps every VectorCraft file format working and runs natively on macOS, Windows (x64 and ARM64), Linux and FreeBSD, plus in the browser via WebAssembly.
</p>

<p align="center">
  <img alt="Status: pre-alpha, in active development" src="https://img.shields.io/badge/status-pre--alpha-e8573f">
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

**Pre-alpha: P1 and P2 are complete; P3 is in final acceptance.** CoralCraft re-based on a fork of [storytold/vectorcraft](https://github.com/storytold/vectorcraft) and completed a full source sync through `36ef34df` while preserving every CoralCraft extension. The native desktop application, drawing and editing commands, and multi-format input/output are implemented and exercised by automated tests. This is not yet a production-ready replacement for VectorCraft or CorelDRAW; remaining work and evidence limits are tracked in [`ROADMAP.md`](ROADMAP.md) and [`docs/gaps.md`](docs/gaps.md).

**What works today:**

- **Native desktop & Web application (`coralcraft`, `vectorcraft-web`).** Full `eframe`/`egui` GPU/CPU desktop shell with the complete VectorCraft toolbox, about 40 dockable panels, 125 dialogs, and multi-column/docking workspace management. The built-in **`CorelDRAW` workspace preset** (`Window → Workspace → CorelDRAW`) provides a single-column left Toolbox, top Property Bar, right vertical Color Palette bar, bottom Page Navigator, and right-side dockers.
- **Vector drawing, editing, and text workflows.** Selection and Bézier-node editing, shaping, snapping, transforms, fills/outlines, symbols and styles, artistic and paragraph text, linked frames, path text, typography, editable tables, dynamic page numbers, and native save/reopen with undo/redo.
- **CorelDRAW multi-page & master-layer model.** Per-page dimensions and orientation, page insert/delete/rename/reorder, Master All/Odd/Even and Desktop layers, and a bottom Page Navigator with editable page tabs.
- **PowerClip, live effects, and image workflows.** Blend, Contour, Envelope, Distort, Extrude, Drop Shadow, Block Shadow, Perspective, colour lenses, Symmetry, dedicated Push/Pull and Zipper handles, PowerClip Lock Contents to Frame, bitmap crop/resample/adjustments, and PowerTRACE with editable expanded paths.
- **Advanced fills and transparency.** Reusable texture definitions, Corel two-colour pattern parameters, generic vector/bitmap patterns, mesh fills, gradient stop opacity, editable opacity masks, and the 16-mode merge-mode matrix, with explicit CDR/CMX degradation warnings where the formats cannot preserve native semantics.
- **Clean-room `.cdr` and `.cmx` read/write (`coralcraft-cdr`, `corel-import`).**
  - Opens legacy `WL`, bare `RIFF`/`RIFX`, modern `ZIP+RIFF`, and Corel Presentation Exchange files into the VectorCraft document model, preserving Corel residuals under `document.unknown["coralcraft"]`.
  - Writes **CDR7** (`RIFF....CDR7`), **CDRX** (X4+ `ZIP+RIFF`), and **CMX1** (`RIFF....CMX1`) through Save As, Export, CLI, and MCP. Unsupported semantics use named warnings and, where possible, lossless native residuals rather than silent substitution.
  - Historical V1.4 evidence opened 65/65 valid operator-owned `.cdr` samples with exact page, layer, and object counts. **Current-tree independent verification is still UNVERIFIED** because the oracle tools and sample corpus are absent from the development machine; see [`docs/cdr-verification.md`](docs/cdr-verification.md). Do not interpret self-skipping tests as certification.
- **VectorCraft format compatibility (30 formats / 46 open extensions).** Opens and saves every format supported by VectorCraft at the synced upstream commit, including `.vectorcraft`, `.drawcraft`, `.vctemplate`, `.svg`, `.svgz`, `.pdf`, `.eps`, `.ai`, `.dxf`, `.emf`, `.wmf`, `.psd`, common raster formats, PCX/Pixar, and JPEG 2000, plus `.cdr`, `.cmx`, and legacy pre-fork `.coralcraft` archives. See [`docs/file-format-parity.md`](docs/file-format-parity.md).
- **Public-documentation `CorelDRAW` shortcut preset.** `F5`–`F12` tool dispatch, `Ctrl+G/U/L/K/Q/D` commands, and `Alt+F7`–`Alt+F10` / `F11` / `F12` docker toggles, alongside the existing VectorCraft presets.
- **CLI and MCP automation.** The same engine commands drive the desktop, headless CLI, and MCP `run_command`, including document/page/layer management, drawing, text, effects, tracing, and multi-format import/export. See [`docs/mcp.md`](docs/mcp.md).

**Verified end-to-end:** a real CLI smoke scenario creates a rectangle, an ellipse, text, and fill colours, then exports and reopens `.vectorcraft`, `.cdr`, `.cmx`, `.svg`, and `.pdf` successfully. The native desktop binary also builds and reports `coralcraft 0.0.1`, and CorelDRAW workspace UI tests pass. These are development validations, not a production-certification claim.

**Active product roadmap (P0–P5; see [`ROADMAP.md`](ROADMAP.md)):**

- **P0/P1/P2:** Evidence baseline, professional core vector workflow, and professional text/multipage layout are delivered.
- **P3:** Texture, Corel pattern parameters, merge modes, Symmetry, Block Shadow, Perspective, Lens, Push/Pull and Zipper handles, PowerClip locking, and the combined persistence matrix are implemented. Final acceptance, remaining evidence hardening, and the final full gate are in progress.
- **P4:** Colour management, prepress, and production output are still open.
- **P5:** Automation, performance, crash recovery, and platform/product hardening are still open.

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

# Create a document and draw a rectangle through real engine commands
cargo run --release -p coralcraft-cli -- run \
  --cmd file.new --params '{"width":800,"height":600}' \
  --cmd shape.rectangle --params '{"x":40,"y":40,"width":240,"height":160}' \
  --export out.cdr

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

It exposes structured tools including `open_file`, `save_file`, `export`, `render_preview`, `inspect_document`, `list_commands`, `run_command`, and `command_batch`, plus hundreds of engine commands covering document/page/artboard/layer management, live shapes, boolean pathfinder operations, Bézier node editing, typography, gradients/meshes/patterns, vector effects, image tracing, and multi-format import/export (including `.cdr` and `.cmx`). See [`docs/mcp.md`](docs/mcp.md).

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
