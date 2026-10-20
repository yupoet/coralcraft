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

## 7. [Resolved] `serde_json` Requires `float_roundtrip` Feature for Bit-Exact `F64` Round-Trip

- **Symptom**: Default `serde_json` float parser had a 1-ULP discrepancy on extreme float exponents (e.g., `1.5787512879013381e-215`) during `F64` round-trip property testing.
- **Resolution**: Enabled `serde_json = { version = "1", features = ["float_roundtrip"] }` in the workspace [`Cargo.toml`](../../Cargo.toml), which uses Rust's standard library Eisel-Lemire exact float parser with zero additional dependencies.

## 8. [Resolved] CIE 1931 2° D65 White Point Z Coordinate (`1.08883`)

- **Symptom**: Using `0.98883` instead of `1.08883` for the CIE 1931 2° D65 white point Z coordinate in Bradford chromatic adaptation causes `LabD50(100, 0, 0)` to map to sRGB `[255, 255, 243]`.
- **Resolution**: Set `D65_WHITE = [0.95047, 1.00000, 1.08883]` and `D50_WHITE = [0.96422, 1.00000, 0.82521]` in [`crates/color/src/spaces.rs`](../../crates/color/src/spaces.rs), yielding exact `[255, 255, 255]` white-point mapping and `< 0.01` $\Delta E$ Bradford round-trip accuracy.

## 9. [Resolved] Dotted `.workspace = true` Keys in `xtask layers` Parser

- **Symptom**: Writing `serde.workspace = true` in a crate's `Cargo.toml` produced a raw key `"serde.workspace"` in `xtask`'s line-based dependency parser before stripping `.workspace`.
- **Resolution**: [`xtask/src/main.rs`](../../xtask/src/main.rs) `parse_dep_names` now strips any `.workspace` suffix before checking `coralcraft-*` crate layers.

## 10. Project review 2026-10-20: verification pitfalls (read before claiming anything is done)

- **Symptom**: Earlier sessions shipped false claims:
  - an invented README MCP tool table, where 162 names did not exist;
  - README CLI commands that did not exist;
  - "verified" installers that do not install: `.rpm` with no files, a fake `.flatpak` format, Linux-built `.dmg`/`.pkg` without HFS+ or Bom, a FreeBSD tarball holding a Linux musl binary with a patched OSABI byte, and a hollow Web bundle;
  - Linux binaries requiring GLIBC_2.39.
  
  `cargo xtask ci` was green throughout, because none of these were tested.
- **Rules**:
  1. Any tool or command name written in docs must come from the code. `docs/mcp.md` is generated and checked by `cargo test -p coralcraft-mcp --test docs_sync`.
  2. A package counts as verified only after it has been installed and launched on its target OS: container install tests for Linux, a native runner for macOS, Windows and FreeBSD. Never hand-roll binary package formats (dmg, pkg, rpm, flatpak, AppImage). Use the platform tools on native CI runners, as `refs/vectorcraft/packaging` does.
  3. Linux release binaries must target an old glibc baseline (`cargo zigbuild --target x86_64-unknown-linux-gnu.2.17`) or musl.
  4. Never delete untracked files you did not create without asking. The `.github/` release workflow was deleted once by mistake.
- **Open review findings**: see the reopened queue at the top of [`docs/gaps.md`](../gaps.md).

## 11. `coralcraft-cli` panics on a closed stdout (broken pipe)

- **Symptom**: `coralcraft-cli probe file.cdr | head -n 2` panics with `failed printing to stdout: Broken pipe`, because `println!` panics on EPIPE.
- **Fix (open)**: write through a locked `stdout` with `writeln!`, and treat `ErrorKind::BrokenPipe` as a normal exit.

## 12. MCP stdio host is `coralcraft-cli mcp` (also `coralcraft --mcp`)

- Both binaries call `McpServer::serve_stdio`. Known protocol gaps (notifications get responses, no `ping`, empty `inputSchema` for most tools) are tracked as R4.1 and R4.2 in `docs/gaps.md`.
