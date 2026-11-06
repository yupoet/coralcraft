# Craft Anything Reference

Source: https://github.com/storytold/craft-anything at `6e0978a08598ef0e46a81b29c11471dac23cd95b` (2026-11-06). Local reference: `refs/craft-anything/` (never committed).

## Useful iteration pattern

- Organize work as Study → Build → Check → Refine. A result is ready only when an executable check supports that claim.
- Keep capabilities headless-first in the engine, with CLI, MCP, and UI using the same command path.
- Return typed status and structured evidence; do not infer success from human-readable messages.
- Break roadmap items into small, independently mergeable changes with explicit dependencies and owners.
- Bound retries and refinement loops. Stop on no-progress turns and report the unresolved gap instead of spending more rounds.
- Record what was actually exercised, including platform and fixture coverage, and keep known limits visible.

## Application to CoralCraft

For the Phase V workspace and tool tasks, first make a public-documentation evidence map to existing VectorCraft commands and tools. Implement missing behavior through the shared engine command path, then check it with synthetic documents, command-level tests, and UI tests where visual interaction is involved. Refine one small tool group at a time rather than combining the whole workspace into one change.

For agent-driven work, measure progress by files changed, commands executed, and checks passed. Treat unchanged turns and unverified success claims as stop conditions. Keep Corel clean-room boundaries unchanged: public documentation and operator-owned samples only.
