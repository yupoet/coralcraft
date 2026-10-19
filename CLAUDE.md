# CLAUDE.md — CoralCraft Agent Memory & Instructions

Before starting work, read `docs/ai-memory/00-project.md`, `01-commands.md`, `02-architecture.md`, `03-current.md`, and `04-known-issues.md` (and relevant files under `docs/ai-memory/05-ref-docs/`).
When making important discoveries, update `docs/ai-memory/03-current.md`, `04-known-issues.md`, or `05-ref-docs/`. Persist important findings back into these files.

```text
docs/ai-memory/
├── 00-project.md        # Project goals, workspace modules, and directory layout
├── 01-commands.md       # Common commands, build/run/test workflows
├── 02-architecture.md   # Architecture, key design decisions, and dependency layers
├── 03-current.md        # Current task status, active issues, and next steps
├── 04-known-issues.md   # Known bugs, pitfalls, and workarounds
└── 05-ref-docs/         # External references and domain knowledge notes
```

## Language & Engineering Rules

- **All code comments, docstrings, error/warning messages, log output, documentation, and git commit messages must be written in English.**
- Full clean-room rules, licensing boundaries, asset attribution requirements, and TDD workflow rules are defined in [`AGENTS.md`](AGENTS.md) and must be strictly followed at all times.
