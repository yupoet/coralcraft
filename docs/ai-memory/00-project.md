# 00 — Project Goals, Modules & Directory Layout

## 1. Mission & Goals

**CoralCraft** is an open-source, clean-room, Rust-native vector illustration, multi-page layout, and print-production application targeting CorelDRAW workflow parity (and beyond).

- **Naming**: **Coral** (coral reef ecosystem) + **Craft** (aligned with the [ArtCraft Crafting Apps](https://getartcraft.com/apps) family naming convention).
- **Core Goals (`G1`–`G4`)**:
  - **G1 Application Parity**: Vector illustration, multi-page publication layout, and print production (signage, engraving, prepress, publishing); open native `.coralcraft` format.
  - **G2 CDR Read/Import**: Native `.cdr` inspection and import (bare RIFF v3–X3, modern ZIP+RIFF X4–2025+, and legacy `WL` detection) with a structured import warning ledger.
  - **G3 CMX Read/Import**: Support for Corel Presentation Exchange (`.cmx`, both little-endian `RIFF` and big-endian `RIFX`).
  - **G4 CDR Write (Long-Term, M5+)**: Export a documented compatible subset of `.cdr`.
- **Non-Goals (`NG1`–`NG4`)**:
  - Not a full bitmap photo editor;
  - No Microsoft VBA runtime (scripting uses an embedded sandboxed engine in M9);
  - Strict local-first operation (no mandatory cloud accounts or telemetry);
  - Workflow and shortcut parity rather than pixel-cloning proprietary UI chrome.
- **License**: Dual-licensed under `MIT OR Apache-2.0`.

## 2. Directory Layout

```text
/data/coralcraft
├── AGENTS.md                  # Agent rules (clean-room, licensing, assets, English comments/logs/commits)
├── CLAUDE.md                  # AI memory entry point and index
├── ROADMAP.md                 # Stage baseline, per-dimension self-scores (0–100%), and milestone roadmap
├── ASSETS.md                  # Asset attribution ledger (verified by `cargo xtask assets`)
├── Cargo.toml                 # Rust workspace root manifest (edition 2024, rust-version 1.95)
├── crates/                    # Pure-Rust engine and format crates (decoupled from UI host)
│   ├── geom/                  # L0: 2D geometry kernel (Bezier, boolean ops, snapping)
│   ├── color/                 # L0: Color spaces (CMYK, spot colors, .cpl/.xml palettes)
│   ├── doc/                   # L1: Multi-page document model (Document/Spread/Page/Layer/Desktop/Object)
│   ├── cdr/                   # L3: CDR/CMX format layer (probe -> parse -> build, hostile-input hardened)
│   ├── render/                # L3: SIMD software/GPU rasterization pipeline
│   ├── tools/                 # L4: Interactive tool and panel state machines
│   ├── engine/                # L5: Engine assembly and command dispatch
│   ├── ui-egui/               # L6: Replaceable egui desktop/Web frontend
│   └── mcp/                   # L6: MCP server for agent-driven vector illustration
├── apps/                      # Binary entry points (L7)
│   ├── coralcraft/            # Desktop GUI binary
│   └── coralcraft-cli/        # CLI & headless tool (`probe`, `render`, etc.)
├── xtask/                     # L7: Engineering quality gates (`ci`, `cleanroom`, `assets`, `layers`, `stats`)
├── scripts/
│   └── bootstrap-refs.sh      # Reconstructs pinned reference repos in `refs/`
├── refs/                      # External reference repos (git-ignored, only `REFS.md` is tracked)
│   └── REFS.md                # Reference repo index, license boundaries, and pinned commits
└── docs/
    ├── ai-memory/             # Persistent AI memory bank (`00`–`04` + `05-ref-docs/`)
    ├── file-format-parity.md  # File format support matrix
    ├── target-app-parity.md   # CorelDRAW workflow parity matrix
    ├── gaps.md                # Prioritized gap list
    ├── reverse/               # Clean-room CDR format reverse-engineering log (`log-format.md`)
    └── superpowers/           # Specifications and milestone TDD implementation plans
```
