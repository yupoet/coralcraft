# 01 — 常用命令、启动方式与测试方式

## 1. 环境与初始化命令

```bash
# 1. 重建 refs/ 第三方参考仓（按 refs/REFS.md §D 冻结的 40 位完整 SHA 拉取，约 200MB）
./scripts/bootstrap-refs.sh

# 若需拉取各参考仓最新 tip（拉完后需同步更新 refs/REFS.md §D 与脚本）
./scripts/bootstrap-refs.sh --latest

# 2. 检查 Rust 工具链（要求 rustc >= 1.95，edition 2024；当前环境为 1.97.1）
rustc --version && cargo --version
```

## 2. 质量门与测试命令（对齐 `vectorcraft` 的 `cargo xtask` 体系）

> 已配置 [`.cargo/config.toml`](../../.cargo/config.toml) 别名 `xtask = "run --package xtask --"`，可直接使用 `cargo xtask <cmd>`。

```bash
# 全套 CI 质量门（fmt --check + clippy -D warnings + cleanroom + assets + layers + test --workspace）
cargo xtask ci

# 净室合规扫描（拦截 Corel 版权样串及混入源码/文档树的 .cdr/.cmx 文件）
cargo xtask cleanroom

# 资产合规扫描（校验 assets/ 下所有文件均在 ASSETS.md 登记）
cargo xtask assets

# 依赖分层门禁（校验 L0..L7 严格单向依赖及 L6 以下无 UI 依赖）
cargo xtask layers

# 代码统计（统计每个 crate 的源码文件数、行数与 #[test] 数量）
cargo xtask stats

# 代码格式化与静态检查（已对齐 vectorcraft 的 rustfmt.toml 与 clippy.toml）
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings

# 运行全部测试（M1 Task 10 完成前，coralcraft-cdr 含 5 个预设 TDD 红灯锚点）
cargo test --workspace

# 排除红灯锚点跑其余全绿测试（M1 Task 10 转绿前使用）
cargo test --workspace --exclude coralcraft-cdr

# 单独运行 CDR 格式层测试
cargo test -p coralcraft-cdr
```

## 3. 应用与 CLI 启动方式

```bash
# 运行桌面端程序
cargo run -p coralcraft

# 运行 CLI 工具（M1 Task 11 交付 probe 子命令后支持探测文件）
cargo run -p coralcraft-cli -- --help
cargo run -p coralcraft-cli -- probe <file.cdr>
cargo run -p coralcraft-cli -- probe --json <file.cdr>
```
