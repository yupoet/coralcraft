<h1 align="center">CoralCraft</h1>

<p align="center">
  <b>Vector illustration, multipage layout, and print production; an open-source, CorelDRAW-parity application in pure Rust.</b>
</p>

<p align="center">
  A fork of <a href="https://github.com/storytold/vectorcraft"><b>VectorCraft</b></a> (MIT OR Apache-2.0) extended toward <b>CorelDRAW</b> parity (often searched as <i>CoralDraw</i>), with clean-room <code>.cdr</code> and <code>.cmx</code> format support. It keeps every VectorCraft file format working. The goal is native apps on macOS, Windows, Linux and FreeBSD, plus the browser via WebAssembly.
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

**Pre-release.** A full project review on 2026-10-20 found that several earlier claims in this README and in [`ROADMAP.md`](ROADMAP.md) overstated what works. The review and its phased fix plan are tracked in [`docs/gaps.md`](docs/gaps.md). Known problems are listed in [`docs/ai-memory/04-known-issues.md`](docs/ai-memory/04-known-issues.md).

**What works today:**

- **CDR/CMX probe.** `coralcraft-cli probe` detects the container generation (legacy `WL`, RIFF/RIFX, X4+ ZIP+RIFF), the version, and the chunk tree. It is hardened against malformed input.
- **CDR parse (partial).** Pages, layers, curves, rectangles, ellipses, text, fills, outlines and some bitmaps from real `.cdr` files are mapped into the CoralCraft document model. Anything not understood becomes an `Unsupported` object with a warning.
- **Document model.** Multi-page documents with master and desktop layers, saved as native `.coralcraft` JSON or a stored-ZIP container.
- **Headless rendering and export.** PNG raster output, plus SVG 1.1, PDF 1.4, EPS and DXF export, through the CLI and the MCP server.
- **Editing engine.** Transactional undo/redo engine with object and history budgets, exposed to agents as 244 MCP tools: shapes, generators, path editing, transforms, styles, pages and layers, import and export.

**Known major gaps** (being fixed in order; see [`docs/gaps.md`](docs/gaps.md)):

- **No interactive desktop UI yet.** A native `egui`/`eframe` shell, following [VectorCraft](https://github.com/storytold/vectorcraft), is planned. The current `coralcraft` binary serves an experimental localhost preview that is not hardened and should not be used on untrusted networks.
- **Boolean operations** (Weld/Trim/Intersect) give wrong geometry for many inputs.
- **CDR/CMX writers** emit a CoralCraft-specific layout that other CorelDRAW readers are not expected to open yet. Spec-conformant writers are planned.
- **Hostile-input hardening** of the CDR parser still has open decompression and allocation-budget issues.
- **No packaged releases yet** (see [Downloads](#downloads)).

## Quick start

Requires **Rust 1.95+** (Edition 2024).

```sh
git clone https://github.com/yupoet/coralcraft
cd coralcraft

# Inspect, render and export documents
cargo run --release -p coralcraft-cli -- probe <file.cdr>            # container, version, chunk tree (--json available)
cargo run --release -p coralcraft-cli -- render <file.cdr> -o page.png --scale 2
cargo run --release -p coralcraft-cli -- export <file.cdr> -o out.svg  # .svg .pdf .eps .dxf .png .cdr .cmx .coralcraft
cargo run --release -p coralcraft-cli -- inspect-palette <palette.xml|palette.cpl>
cargo run --release -p coralcraft-cli -- workspace-preview <file.cdr> -o workspace.png

# MCP server on stdio
cargo run --release -p coralcraft-cli -- mcp

# Workspace quality, layering & clean-room gates
cargo xtask ci          # fmt, clippy -D warnings, tests, cleanroom, assets, layers
cargo xtask stats       # crate-by-crate file, line & test counts
```

## MCP server for agents

`coralcraft-cli mcp` runs a Model Context Protocol server (newline-delimited JSON-RPC 2.0 on stdio) with **244 tools**. Register it with Claude Code:

```sh
cargo build --release -p coralcraft-cli
claude mcp add coralcraft -- "$PWD/target/release/coralcraft-cli" mcp
```

The tools cover:

- Document, page and layer management.
- Parametric shapes and generators: `add_rectangle`, `add_polygon`, `add_spiral`, `add_gear_wheel`, `add_voronoi_mosaic`, `add_hilbert_curve`, and more.
- Path and node editing: `insert_curve_node`, `offset_object_path`, `knife_cut_object`, `reduce_path_nodes`, and more.
- Transforms and arrangement: `rotate_selection`, `align_selection`, `step_and_repeat`, `pack_selection_in_grid`, and more.
- Styles: `set_style_cmyk`, `set_fountain_fill`, `set_outline_properties`, `set_transparency`, and more.
- Effects: `apply_contour`, `apply_envelope`, `apply_perspective`, `create_blend`, and more.
- Text: `add_artistic_text`, `add_paragraph_text`, `fit_text_to_path`.
- Import/export: `import_svg`, `import_dxf`, `export_pdf_base64`, `export_svg`, `export_png_base64`, and more.
- Undo/redo.

The complete, generated tool list and the current protocol limitations are in [`docs/mcp.md`](docs/mcp.md). A test keeps that list in sync with the server.

## Downloads

There are **no packaged releases yet**. An earlier `v0.0.1` release was withdrawn because several of its installers were invalid. Packages will be built and published by GitHub Actions on native runners, following VectorCraft's release pipeline:

1. macOS universal `.app.zip`, ad-hoc signed and not notarized. This comes first.
2. Linux, Windows, FreeBSD and Web packages follow later.

Until then, build from source with `cargo build --release`.

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

CoralCraft's workspace architecture, crate layering (`L0`–`L7`), agent-native MCP design, and clean-room engineering methodology are modeled after [VectorCraft](https://github.com/storytold/vectorcraft) and the [ArtCraft Crafting Apps](https://getartcraft.com/apps) family (`DesignCraft`, `CADCraft`, `PhotoCraft`, `PdfCraft`).
