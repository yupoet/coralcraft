# 01 — Common Commands, Build & Test Workflows

## 1. Environment & Bootstrap

```bash
# 1. Restore external reference repositories in refs/ (fetches exact 40-char SHAs from refs/REFS.md §D)
./scripts/bootstrap-refs.sh

# Clone latest upstream branch tips instead (update refs/REFS.md §D and script afterward)
./scripts/bootstrap-refs.sh --latest

# 2. Check Rust toolchain (requires rustc >= 1.95, edition 2024)
rustc --version && cargo --version
```

## 2. Quality Gates & Testing (`cargo xtask`)

> [`.cargo/config.toml`](../../.cargo/config.toml) defines the alias `xtask = "run --package xtask --"`.

```bash
# Full CI gate (fmt --check + clippy -D warnings + test --workspace + cleanroom + assets + layers)
cargo xtask ci

# Clean-room compliance scan (blocks prohibited Corel copyright strings and .cdr/.cmx files in crates/apps/docs)
cargo xtask cleanroom

# Asset attribution gate (verifies all files under assets/ are registered in ASSETS.md)
cargo xtask assets

# Crate dependency layering gate (enforces L0..L7 one-way dependencies and forbids UI crates below L6)
cargo xtask layers

# Codebase statistics (files, lines of code, and #[test] count per crate)
cargo xtask stats

# Formatting and Clippy checks
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings

# Run all workspace unit and integration tests
cargo test --workspace

# Run CDR format crate tests only
cargo test -p coralcraft-cdr
```

## 3. Running Applications & CLI

```bash
# Run desktop application stub
cargo run -p coralcraft

# Run CLI tool (`probe` subcommand)
cargo run -p coralcraft-cli -- --help
cargo run -p coralcraft-cli -- probe <file.cdr>
cargo run -p coralcraft-cli -- probe --json <file.cdr>
```
