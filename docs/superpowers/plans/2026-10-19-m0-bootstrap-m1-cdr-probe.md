# CoralCraft M0/M1 实施计划：骨架引导 + CDR 格式探测

> **For agentic workers:** REQUIRED SUB-SKILL: use `superpowers:subagent-driven-development`
> （推荐）或 `superpowers:executing-plans` 按任务执行；每项严格遵守失败测试先行、
> 实现、全量回归（`cargo test --workspace`）、双审查。
> **Prerequisite:** M0 Task 1–8 已于 2026-10-19 引导完成（见各 Task 状态行）；
> M1 从 Task 9 开始，可由 subagent 独立领取。

**Goal:** 建立 CoralCraft 工程骨架（ArtCraft 兼容约定）并交付第一个真实能力：
`coralcraft-cdr` 的格式探测层（probe）——识别 CDR 容器（RIFF/老 WL）、版本字节、
chunk 走读器与 CLI `probe` 子命令，全部基于合成 fixture（净室合规）。

**Architecture:** Rust workspace（`crates/*` + `apps/*` + `xtask`），crate 划分对齐
vectorcraft；`coralcraft-cdr` 按 probe → parse → build 三阶段演进，本计划只做 probe。
CDR 文件按**敌意输入**处理：任何解析错误返回 `Result`，禁止 panic。

**Tech Stack:** Rust 1.96+（edition 2024）· serde · thiserror ·（CLI）clap ·
参考 Photopea CDR 规范（`refs/cdr-spec/`）与 RIFF 通用结构。**不引入** libcdr
（MPL，仅语义参考）、任何 GPL/AGPL 代码。

**Design:** `docs/superpowers/specs/2026-10-19-coralcraft-brainstorm.md`（裁决 D1–D8）

## Delivery Slices

1. **Slice A（骨架，M0）**：Task 1–8——目录、workspace、占位 crate、净室与资产
   xtask 门、refs、文档、许可证、红灯锚点。✅ 2026-10-19 已全部交付。
2. **Slice B（探测纵切，M1）**：Task 9–13——probe API、chunk 走读器、CLI probe、
   格式 parity 文档初版、逆向工作日志规范。**禁止在 probe 未全绿前开始 parse 层。**
3. **Slice C（M2 预告，不在本计划）**：doc/render/原生格式。

## Global Constraints

- **净室铁律**（AGENTS.md）：从不运行/观察/反汇编 CorelDRAW；fixture 一律从
  公开规范手写合成；`cargo xtask cleanroom` 拦截 Corel 版权标记。
- **许可**：MIT OR Apache-2.0；不抄 GPL/AGPL/MPL 代码；refs/ 永不提交。
- **敌意输入**：所有格式层 API 返回 `Result`；截断/奇数 padding/未知 chunk
  必须有明确错误或跳过策略，禁止 unwrap/panic（`#![deny(clippy::unwrap_used)]`
  于格式 crate）。
- **TDD**：失败测试先行；每个 Task 一个 conventional commit（中文说明，无 attribution）。
- **parity 文档自评分**：仿 vectorcraft 的 target-app-parity 模式，M1 建立基线，
  后续每 Task 更新。

## File Structure（M1 交付后）

```text
crates/cdr/
├── Cargo.toml
└── src/
    ├── lib.rs          # probe API：CdrContainer、CdrKind、probe()、ProbeError
    ├── riff.rs         # RIFF 走读器：chunk 迭代、padding、size 校验
    └── error.rs        # ProbeError（thiserror）
apps/coralcraft-cli/
└── src/main.rs         # clap：probe <file> → 人读输出 + --json
docs/
├── file-format-parity.md   # 格式支持矩阵初版（cdr/cmx 行 + 计划列）
└── reverse/log-format.md   # 逆向工作日志规范 + 首条示例（RIFF 头部发现）
```

## Tasks

### Task 1 — 仓库与目录骨架 ✅（2026-10-19）

`coralcraft/` git init（branch main）、`docs/superpowers/{specs,plans}/`、
`refs/`（9 个浅克隆 + REFS.md）、`.gitignore`（含 `/refs/`、`/plan/`、Rust 产物）。

### Task 2 — 净室与资产规则落地 ✅（2026-10-19）

`AGENTS.md`（Corel 版净室铁律 + 许可 + 资产 + 质量）、`README.md`。

### Task 3 — workspace 清单 ✅（2026-10-19）

根 `Cargo.toml`：resolver 3、edition 2024、rust-version 1.95、
license `MIT OR Apache-2.0`、members `crates/* apps/* xtask`、
workspace.dependencies（serde/thiserror/clap）。

### Task 4 — 占位 crate ✅（2026-10-19）

`crates/{geom,color,doc,render,tools,engine,ui-egui,mcp}`：空 lib + 一行模块文档
+ 冒烟测试（各自 crate 名出现在 `--list`）。不引外部依赖。

### Task 5 — coralcraft-cdr 红灯锚点 ✅（2026-10-19）

`crates/cdr`：probe API 签名 + `todo!("M1 Task 9")` + 三个失败测试
（RIFF 检出/legacy WL 检出/拒绝非 CDR）。**`cargo test -p coralcraft-cdr` 预期红。**

### Task 6 — CLI 骨架 ✅（2026-10-19）

`apps/coralcraft-cli`：clap 占位（`probe` 子命令声明但 `unimplemented!`）+
`apps/coralcraft`：桌面 bin 占位（打印版本，无窗口）。

### Task 7 — xtask 门 ✅（2026-10-19）

`xtask`：`cargo xtask ci` = fmt --check + clippy -D warnings + test --workspace；
`cleanroom`：扫描 staged/源码中 "Corel"/"corel" 版权样串与 `.cdr` 二进制 fixture
（占位实现，M1 Task 12 完善）。

### Task 8 — 文档与许可 ✅（2026-10-19）

ROADMAP.md（pre-alpha 基线）、gaps.md、LICENSE-MIT、LICENSE-APACHE、NOTICE、
ASSETS.md（空表头）、brainstorm 文档、本计划。

### Task 9 — RIFF 走读器（红→绿）

1. **红**：`riff.rs` 测试——合成最小 RIFF/CDR 字节流（按 Photopea 规范 §RIFF：
   `RIFF` + u32 LE size + `CDR` + 版本字节；`LIST` 子 chunk；奇数 padding），
   断言 chunk 序列、size 边界、padding 步进。
2. **绿**：实现 `RiffReader::new(&[u8]) -> Result<Self, ProbeError>` 与
   `next_chunk()`；截断输入 → `ProbeError::Truncated`；size 越界 → `Invalid`。
3. 回归：`cargo test --workspace` 全绿；`cargo xtask ci` 过。

### Task 10 — probe API 转绿

1. **红**（已存在于 Task 5）：三个 probe 测试转绿条件就绪。
2. **绿**：`probe(&[u8])` → `CdrContainer::Riff { version_byte } |
   CdrContainer::LegacyWl | Err(NotCdr)`；`CdrKind` 枚举（Cdr/Cmx 预留：第 4–7
   字节为 `CDR`/`CMX` 判别，来源 Photopea 规范 chunk 头 + libcdr 文档页对照记录
   进 `docs/reverse/`）。
3. fuzz 预埋：`cargo fuzz` 目标文件占位（不强制跑，M3 接敌意语料）。

### Task 11 — CLI probe 子命令

1. **红**：CLI 集成测试——对合成 fixture 断言 stdout 含 `RIFF` 与版本、
   `--json` 输出可 `serde_json` 反序列化、非 CDR 文件退出码非零且 stderr 有
   `not a CDR`。
2. **绿**：接线 `coralcraft-cdr::probe`。

### Task 12 — cleanroom/assets 门完善

1. **红**：xtask 测试——含伪造 Corel 版权样串（样串集合以 cleanroom 扫描器定义
   为准，不在文档中直书）的临时文件被检出；`docs/` 下出现 `.cdr` 文件被检出。
2. **绿**：实现扫描器（源码树 glob，跳过 refs/ 与 target/）。

### Task 13 — 格式 parity 文档初版 + 逆向日志规范

1. `docs/file-format-parity.md`：表头 + `.cdr` 行（open: probe-only / parse: no /
   write: no）+ `.cmx` 行（planned）+ 计划列与证据链接（Photopea spec、libcdr
   wiki）。
2. `docs/reverse/log-format.md`：日志条目模板（发现、证据文件 SHA-256、规范
   出处、置信度）+ 首条示例（RIFF 容器与版本字节）。
3. 更新 ROADMAP.md 数字；提交。

## 验收（M1 Definition of Done）

```bash
cargo xtask ci                                   # 全绿
cargo run -p coralcraft-cli -- probe <合成.cdr>  # 输出容器/版本/chunk 清单
cargo run -p coralcraft-cli -- probe <任意文件>  # 干净报错，退出码 2
grep -r "corel" --ignore-case crates/ apps/      # 无版权样串（功能词除外）
git log --oneline                                # 每 Task 一提交
```

## 计划外变更

执行中发现的结构性偏差回写本节（仿 aide-captain 计划修订惯例），不静默改架构。
