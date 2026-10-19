# 04 — Known Issues, Pitfalls & Workarounds

## 1. [Resolved] Short SHAs in `scripts/bootstrap-refs.sh` Rejected by GitHub `git fetch --depth 1`

- **Symptom**: 7-character short SHAs in `scripts/bootstrap-refs.sh` caused `git fetch -q --depth 1 origin <short-sha>` to fail with `fatal: couldn't find remote ref`, triggering the fallback branch clone.
- **Resolution**: Upgraded all 11 entries in [`scripts/bootstrap-refs.sh`](../../scripts/bootstrap-refs.sh) and [`refs/REFS.md`](../../refs/REFS.md) to full 40-character commit SHAs.

## 2. `cargo xtask cleanroom` Scope Covers `docs/` (and Brand Name Rules)

- **Symptom**: [`xtask/src/main.rs`](../../xtask/src/main.rs) `cleanroom()` recursively scans `crates/`, `apps/`, and `docs/` for `.cdr`/`.cmx` file extensions and 3 prohibited Corel copyright strings.
- **Rules**:
  1. Never place `.cdr` or `.cmx` files inside `crates/`, `apps/`, or `docs/` (construct synthetic fixtures in memory or write temporary files to `std::env::temp_dir()`).
  2. Never write the 3 prohibited copyright strings in `crates/`, `apps/`, or `docs/`. In `xtask` unit tests, construct test strings at runtime (`format!("{} (c) {}", "Copyright", "Corel")`).
  3. **Brand Name Boundary**: Following `refs/vectorcraft/xtask/src/brands.rs`, Markdown documentation (`README.md`, `ROADMAP.md`, `docs/*.md`) may reference `CorelDRAW` (and `CoralDraw`) for public positioning and parity matrices, while shipped UI strings use CoralCraft's own terminology.

## 3. `refs/uniconvertor` AGPL-3.0 Contamination Boundary

- **Symptom**: `refs/uniconvertor/` is cloned locally by `bootstrap-refs.sh`, but is licensed under **AGPL-3.0**.
- **Rule**: Agents must **never** open or read source files under `refs/uniconvertor/`.

## 4. `storytold/craftrules` Repository Returns 404

- **Symptom**: `https://github.com/storytold/craftrules` referenced in `refs/vectorcraft/AGENTS.md` returns 404.
- **Workaround**: Reference `refs/vectorcraft/AGENTS.md`, `refs/designcraft/`, `refs/cadcraft/`, and `refs/craft-libs/` directly.

## 5. `agy --model claude-opus-5-5-high` Content Filter & Headless Tool Notes

- **Symptom**: When invoking `agy -p --model claude-opus-5-5-high`:
  1. Repeating exploit terminology (such as "depth bomb" / "zip bomb") can trigger the Vertex AI content safety filter (`finishReason: OTHER`).
  2. Calling interactive tools like `read_url_content` in `-p` headless mode returns `Tool execution was denied`.
- **Workaround**: Use neutral engineering terminology ("nesting depth limit", "compression ratio limit") and instruct the model to read local files only without calling `read_url_content`.

## 6. Clippy `unwrap_used` / `expect_used` in Integration Tests (`tests/*.rs`)

- **Symptom**: `clippy.toml`'s `allow-unwrap-in-tests = true` only applies inside functions annotated with `#[test]`. Helper structs/methods in `tests/*.rs` trigger `clippy::expect_used` unless allowed at the module/crate level.
- **Workaround**: Add `#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]` at the top of integration test files (`tests/*.rs`) or on `#[cfg(test)] mod tests`.
