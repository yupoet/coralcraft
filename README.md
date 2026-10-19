<h1 align="center">CoralCraft</h1>

<p align="center">
  <b>Vector illustration, multipage layout, and print production — an open-source, clean-room studio built from scratch in pure Rust.</b>
</p>

<p align="center">
  A fast, local-first vector graphics and desktop publishing workflow targeting CorelDRAW parity (and beyond), with native <code>.cdr</code> / <code>.cmx</code> format interoperability. Designed to run natively on macOS, Windows, and Linux, and in the browser via WebAssembly.
</p>

<p align="center">
  <img alt="Status: pre-alpha" src="https://img.shields.io/badge/status-pre--alpha-e8573f?style=flat-square">
  <img alt="Written in pure Rust" src="https://img.shields.io/badge/written%20in-Rust-b83a24?style=flat-square&logo=rust&logoColor=white">
  <img alt="Runs on macOS, Windows, Linux and Web" src="https://img.shields.io/badge/runs%20on-macOS%20%C2%B7%20Windows%20%C2%B7%20Linux%20%C2%B7%20Web-555555?style=flat-square">
  <img alt="Formats: CDR, CMX, SVG, PDF" src="https://img.shields.io/badge/formats-CDR%20%C2%B7%20CMX%20%C2%B7%20SVG%20%C2%B7%20PDF-0b6f88?style=flat-square">
  <img alt="Agent-drivable over MCP" src="https://img.shields.io/badge/agents-MCP%20server-14a3c7?style=flat-square">
  <img alt="License: MIT OR Apache-2.0" src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-4d7a0a?style=flat-square">
</p>

<p align="center">
  <a href="#why-coralcraft">Why CoralCraft</a> ·
  <a href="#clean-room-methodology">Clean-Room Methodology</a> ·
  <a href="#architecture">Architecture</a> ·
  <a href="#quick-start">Quick Start</a> ·
  <a href="#roadmap--status">Roadmap</a> ·
  <a href="#license">License</a>
</p>

---

## About

**CoralCraft** is an open-source, clean-room reimplementation of the vector illustration, multipage layout, and print-production workflow popularized by CorelDRAW, written from scratch in pure Rust.

The name combines **Coral** (a nod to reef ecosystems and a phonetic tribute to the workflow it serves) + **Craft** (following the architectural conventions of the open-source [ArtCraft Crafting Apps](https://getartcraft.com/apps) ecosystem such as [VectorCraft](https://github.com/storytold/vectorcraft), [DesignCraft](https://github.com/storytold/designcraft), and [CADCraft](https://github.com/storytold/cadcraft)).

> [!IMPORTANT]
> **Trademark & Clean-Room Notice:** CoralCraft is an independent open-source project and is **not** affiliated with, endorsed by, or sponsored by Corel or Alludo. CorelDRAW is a registered trademark of its respective owner. CoralCraft contains zero proprietary code, binaries, icons, presets, fonts, or assets from Corel products.

---

## Why CoralCraft

- **The Workflow Print & Signage Professionals Expect.** Unlike single-artboard illustration tools, CoralCraft is built around a **multipage document and spread model** (`Page` / `Spread` / `Master Page` / `Layer`), tailored for signage, engraving, laser cutting, print shops, and multi-page commercial layouts.
- **Native `.cdr` & `.cmx` Interoperability.** Built-in clean-room format support (`coralcraft-cdr`) evolving across three stages:
  1. **Probe** (`M1`): container detection (`RIFF` vs. legacy `WL`), version byte identification, and hostile-input-safe RIFF chunk walking.
  2. **Parse** (`M3`): translating `.cdr` / `.cmx` object trees into the CoralCraft document model with transparent import-warning ledgers.
  3. **Write** (`M5+` long-term campaign): pioneering open-source `.cdr` export for real-world round-trip workflows.
- **Print-First Color Engine.** First-class CMYK, spot colors, and palette formats (`.cpl` / XML palettes) designed from day one for production output.
- **Fast, Native, Everywhere.** Pure Rust and `egui` — no Electron, no web view, no subscription, no telemetry. Compiles to native desktop binaries on macOS, Windows, and Linux, plus WebAssembly for the browser.
- **Agent-Native by Design.** Every document operation and tool action is exposed through a structured command API, CLI, JSON control channel, and **MCP server** (`coralcraft-mcp`), enabling AI agents to inspect, draw, lay out, and verify documents programmatically.

---

## Clean-Room Methodology

CoralCraft strictly enforces a **dual-track clean-room protocol** (see [`AGENTS.md`](AGENTS.md) and [`cargo xtask cleanroom`](xtask/src/main.rs)):

1. **Behavior Track (Public Documentation Only):**
   - Application behavior, tools, panels, and shortcuts are derived **exclusively from public documentation** (official online user manuals, published guides, books, and public forum posts, cited by URL).
   - Contributors and agents **never** launch, run, script, screenshot, measure, or disassemble CorelDRAW or any proprietary binaries, and **never** use proprietary software to generate test fixtures.
2. **Format Track (Public Specs & Owned Samples):**
   - Binary format parsing is implemented from scratch using public community specifications ([Photopea CDR Format Specification](https://github.com/photopea/CDR-specification)), public format documentation ([Document Liberation Project / libcdr wiki](https://wiki.documentfoundation.org/DLP/Libraries/libcdr)), and synthetic byte fixtures written from the specification.
   - All inputs are treated as **hostile / untrusted data**: parsers return `Result` and forbid `unwrap` / panics.
   - No GPL/AGPL code (e.g., Inkscape, UniConvertor, sK1) is ever copied or linked; MPL-2.0 references (`libcdr`) are consulted strictly as read-only semantic references with findings documented in [`docs/reverse/`](docs/).

---

## Architecture

CoralCraft is organized as a modular Rust workspace decoupling core engine crates from UI and host frontends:

```text
coralcraft/
├── crates/
│   ├── geom/           # 2D geometry kernel (Bézier curves, boolean path ops, snapping)
│   ├── color/          # Color spaces (CMYK, Spot colors, RGB) & palette formats (.cpl / XML)
│   ├── doc/            # Multipage document model (Page, Spread, Master Page, Layer, Object)
│   ├── cdr/            # Clean-room .cdr / .cmx format engine (probe → parse → build)
│   ├── render/         # SIMD software & GPU rendering pipeline
│   ├── tools/          # Interactive tool state machines & panel semantics
│   ├── engine/         # Command dispatch, undo/redo snapshots, and workspace orchestration
│   ├── ui-egui/        # Replaceable egui desktop/web frontend
│   └── mcp/            # Model Context Protocol (MCP) server for AI agents
├── apps/
│   ├── coralcraft/     # Native desktop application
│   └── coralcraft-cli/ # Headless CLI & format inspection tool (`coralcraft-cli probe`)
├── xtask/              # Quality & compliance gates (`ci`, `cleanroom`, `assets`)
├── scripts/            # Environment bootstrap scripts (`bootstrap-refs.sh`)
├── refs/               # Git-ignored local reference repositories & license boundaries (`REFS.md`)
└── docs/               # Roadmap, gap list, reverse-engineering logs, plans, and AI memory
```

---

## Quick Start

Requires **Rust 1.95+** (Edition 2024).

```sh
# Clone the repository
git clone https://github.com/yupoet/coralcraft
cd coralcraft

# (Optional) Populate git-ignored reference repos in refs/ (~200 MB, pinned SHAs in refs/REFS.md)
./scripts/bootstrap-refs.sh

# Run clean-room & asset compliance gates
cargo run -p xtask -- cleanroom
cargo run -p xtask -- assets

# Run workspace tests (excluding M1 TDD red-light anchors in coralcraft-cdr prior to Task 10)
cargo test --workspace --exclude coralcraft-cdr

# Run full CI suite (once M1 probe tasks turn green)
cargo run -p xtask -- ci

# Inspect a .cdr / .cmx container via CLI (M1 deliverable)
cargo run -p coralcraft-cli -- probe <file.cdr>
cargo run -p coralcraft-cli -- probe --json <file.cdr>
```

---

## Roadmap & Status

CoralCraft is currently in **pre-alpha (M0 scaffold complete, M1 CDR format probe in progress)**.

- **[`ROADMAP.md`](ROADMAP.md)**: Current stage, self-scored parity dimensions (`0–100%`), and milestone breakdown (`M0`–`M5+`).
- **[`docs/gaps.md`](docs/gaps.md)**: Prioritized work queue.
- **[`docs/superpowers/specs/`](docs/superpowers/specs/2026-10-19-coralcraft-brainstorm.md)**: Brainstorm & architectural decision records (`D1`–`D8`).
- **[`docs/superpowers/plans/`](docs/superpowers/plans/2026-10-19-m0-bootstrap-m1-cdr-probe.md)**: Step-by-step TDD implementation plan for M0 & M1.
- **[`refs/REFS.md`](refs/REFS.md)**: Index of reference projects, license boundaries, and pinned commits.

---

## License

Code and original assets are dual-licensed under **[MIT](LICENSE-MIT) OR [Apache-2.0](LICENSE-APACHE)**, at your option. See [`NOTICE`](NOTICE) and [`ASSETS.md`](ASSETS.md) for attribution details.
