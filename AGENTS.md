# CoralCraft — Instructions for Agents

CoralCraft is a **fork of [storytold/vectorcraft](https://github.com/storytold/vectorcraft)** (`MIT OR Apache-2.0`, ArtCraft family) that extends it toward CorelDRAW. It is a clean-room, open-source, Rust-native vector illustration and multi-page layout application that runs natively on macOS, Windows, and Linux, and compiles to WebAssembly for the Web.

## Mission: Reuse VectorCraft, Extend Toward CorelDRAW (operator decision 2026-10-21)

- **Copy, then adapt.** VectorCraft source (`refs/vectorcraft`) is copied into this repository and reshaped. The active plan is [`docs/superpowers/plans/2026-10-21-vectorcraft-fork.md`](docs/superpowers/plans/2026-10-21-vectorcraft-fork.md) (Phase V).
- **Reuse is mandatory.** Before writing any subsystem, check whether VectorCraft already has it (geometry, boolean ops, rendering, text, effects, tracing, file I/O, docking UI, widgets, MCP, packaging). If it does, port it. A from-scratch replacement of an existing VectorCraft subsystem needs explicit operator approval.
- **Our own scope is only the CorelDRAW extension.** That covers CDR/CMX read/write, the CorelDRAW multi-page model (pages, master layers, desktop layer), the CorelDRAW workspace (toolbox, property bar, palette bar, page navigator, dockers), CorelDRAW-specific tools and interactive effects, and Corel palettes.
- **Every VectorCraft file must work.** CoralCraft opens every format VectorCraft opens (including `.vectorcraft` / `.drawcraft` documents and templates, losslessly) and saves/exports every format VectorCraft saves/exports.
- **No self-round-trip treadmill.** Do not start milestones whose only effect is making our own SVG/DXF/EPS/PDF export re-import into itself.
- Keep upstream crate and package names (`vectorcraft-*`) so upstream fixes can be merged. Mark local behaviour changes in VectorCraft-derived files with `// CoralCraft:`. Log upstream syncs in `docs/upstream-vectorcraft.md`.
- VectorCraft-derived crates keep VectorCraft's lint configuration. CoralCraft-owned crates (`cdr`, `corel-*`, apps) keep the strict lints.
- VectorCraft brand assets (logo, splash, app icons) are never reused. CoralCraft ships its own, registered in `ASSETS.md`.

## Start Every Session Here

1. **Read the AI Memory Bank (`docs/ai-memory/`)**: Before starting any work, read:
   - [`docs/ai-memory/00-project.md`](docs/ai-memory/00-project.md) — Project goals, workspace modules, and directory layout
   - [`docs/ai-memory/01-commands.md`](docs/ai-memory/01-commands.md) — Common commands, build/run/test workflows
   - [`docs/ai-memory/02-architecture.md`](docs/ai-memory/02-architecture.md) — Architecture, key design decisions, and layer rules
   - [`docs/ai-memory/03-current.md`](docs/ai-memory/03-current.md) — Current milestone status, active tasks, and next steps
   - [`docs/ai-memory/04-known-issues.md`](docs/ai-memory/04-known-issues.md) — Known bugs, pitfalls, and workarounds
   - Read [`docs/ai-memory/05-ref-docs/`](docs/ai-memory/05-ref-docs/) as needed for external format and architecture references.
2. **Persist Important Findings Back into Memory**: Whenever you make important discoveries, advance task state, encounter new pitfalls, or synthesize external knowledge, persist those findings into `docs/ai-memory/03-current.md`, `04-known-issues.md`, or `05-ref-docs/`.
3. Read [`ROADMAP.md`](ROADMAP.md) (stage, measured gap to VectorCraft, next steps) and [`docs/gaps.md`](docs/gaps.md) (prioritized gap list). Unless the operator assigns a specific task, pick the first unchecked Phase V task.
4. Read the active plan [`docs/superpowers/plans/2026-10-21-vectorcraft-fork.md`](docs/superpowers/plans/2026-10-21-vectorcraft-fork.md) and execute its tasks in order.
5. Follow the autonomous execution loop: Orient -> Plan -> Failing Test First -> Implement -> Full CI Regression (`cargo xtask ci`) -> Record (sync `docs/ai-memory/`) -> Commit. Do not pause to ask unless a decision is explicitly marked as an operator decision gate.

`plan/` is a git-ignored directory for local session scratch notes.

## Non-Negotiable Rules

### Language: English for All Code Comments, Logs, Diagnostics, and Commits

- **All** source code comments, doc comments (`///`, `//!`), error/warning messages, CLI output, log messages, test descriptions, and git commit messages **must be written in English**.

### Clean-Room: Never Use Proprietary Software

- **Never** launch, run, script, automate (VBA/COM/UI automation), screenshot, or measure CorelDRAW or any proprietary Corel application, and **never** compare our output against a running instance of it. Never use Corel software to generate test fixtures (even "synthetic" ones).
- **Never** read, disassemble, or copy anything from a Corel installation directory (binaries, presets, resources, help files, fonts, ICC profiles, object-model docs).
- **Never** copy Corel-authored code or text into this repository (including in tests). All test inputs must be handcrafted from scratch using public specifications (Photopea CDR specification, Kaitai Struct `coreldraw_cdr.ksy`, RIFF, PKWARE APPNOTE, PDF/ISO 32000).
- Application behavior must be derived strictly from **public documentation** (public user guides, published technical books, public tutorials, forum posts, cited by URL) and **operator-owned local sample files** (which are never committed to the repository).
- Beyond standard feature names, **never** copy proprietary icons, artwork, presets, or UI phrasing.
- `cargo xtask cleanroom` (run by `cargo xtask ci`) scans for prohibited copyright strings and `.cdr`/`.cmx` binary fixtures in the repository tree.

### Licensing: Never Copy GPL/AGPL or MPL Code

- **Never** copy GPL/AGPL code (Inkscape, UniConvertor, lib2geom, sK1, etc.). Study public papers/docs only, cite URLs in comments, and write clean-room implementations.
- `libcdr` is MPL-2.0: it may be read as a format-semantics reference and differential oracle, but **never** copied line-by-line; our implementation is written independently from scratch.
- All code and original assets in this repository are dual-licensed under `MIT OR Apache-2.0`.
- VectorCraft (`MIT OR Apache-2.0`), DesignCraft, CadCraft (`Apache-2.0`), and `craft-libs` (`Apache-2.0`) code **may and should** be copied and adapted. Keep their copyright notices and attribute them in `NOTICE`.

### Assets: Never Use Proprietary Icons or Artwork

- Never add, copy, trace, embed, or distribute icons, images, cursors, presets, swatch libraries, ICC profiles, or screenshots from Corel products or incompatible licenses.
- Every asset file under `assets/` must have an attribution row in [`ASSETS.md`](ASSETS.md) (path, author, source URL, license, notes); `cargo xtask assets` blocks unregistered assets.
- Fonts belong in a separate repository (following the `storytold/craft-fonts` pattern) and are never committed here.

### Quality: Shipped Code Never Panics

- Library crates return `Result`; parsers return structured errors on hostile input and never panic (all `.cdr`/`.cmx` files are treated as untrusted input).
- Every public API change must start with a failing test.

## Commits & Workflow

- Strict TDD (failing test first); conventional commits (`feat`/`fix`/`docs`/`test`/`chore`) with **English** commit messages and no AI attribution footer.
- One commit per task; never commit across a failing slice.
- `refs/` is never committed (external reference code stays local-only).

## Roadmap & Documentation

- Stage and self-assessment scores: [`ROADMAP.md`](ROADMAP.md)
- Target application workflow parity: [`docs/target-app-parity.md`](docs/target-app-parity.md)
- File format support matrix: [`docs/file-format-parity.md`](docs/file-format-parity.md)
- Prioritized gap list: [`docs/gaps.md`](docs/gaps.md)
- Format reverse-engineering log: [`docs/reverse/log-format.md`](docs/reverse/log-format.md) (records chunk semantics and evidence SHA-256 hashes, never third-party code)
