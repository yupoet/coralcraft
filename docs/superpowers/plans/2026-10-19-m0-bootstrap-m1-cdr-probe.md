# CoralCraft M0/M1 实施计划：骨架引导 + CDR 格式探测（经五方评审修订版）

> **For agentic workers:** REQUIRED SUB-SKILL: use `superpowers:subagent-driven-development`
> （推荐）或 `superpowers:executing-plans` 按任务执行；每项严格遵守失败测试先行、
> 实现、全量回归（`cargo test --workspace`）、独立审查（`zcode-delegate`）。
> **Prerequisite:** M0 Task 1–8 已于 2026-10-19 引导完成；M1 经五路模型（GLM-5.3 /
> Kimi K3 / DeepSeek V4.1 Flash / Grok 4.6 / Claude Opus 5.5 High）联合盲审修订后，
> 从 Task 9 开始推进。

**Goal:** 建立 CoralCraft 工程骨架（ArtCraft 兼容约定）并交付第一个真实能力：
`coralcraft-cdr` 的格式探测层（probe）——识别 CDR/CMX 全世代容器（裸 `RIFF`/`RIFX`、
X4+ `ZIP` 内嵌 `content/riffData.cdr` 或 `content/root.dat`、老 `WL`）、ASCII 版本映射与
`vrsn` chunk 提取、受限安全的 RIFF chunk 走读器与 CLI `probe` 子命令，全部基于合法合成 fixture（净室合规）。

**Architecture:** Rust workspace（`crates/*` + `apps/*` + `xtask`），crate 划分与 `L0`–`L7` 分层对齐
vectorcraft；`coralcraft-cdr` 按 probe → parse → build 三阶段演进，本计划只做 probe。
CDR 文件按**敌意输入**处理：任何解析错误返回 `Result`，禁止 `unwrap`/`expect`/`panic`。

**Tech Stack:** Rust 1.95+（edition 2024）· serde · serde_json · thiserror ·（CLI）clap ·
参考 Photopea CDR 规范（`refs/cdr-spec/`）与 RIFF/ZIP 通用结构。**不引入** libcdr
（MPL，仅黑盒/文档对照）、任何 GPL/AGPL 代码。

**Design:** `docs/superpowers/specs/2026-10-19-coralcraft-brainstorm.md`（裁决 D1–D8）及
[`docs/ai-memory/05-ref-docs/04-m1-plan-four-way-review.md`](../../ai-memory/05-ref-docs/04-m1-plan-four-way-review.md)（五方评审修订记录）。

## Delivery Slices

1. **Slice A（骨架，M0）**：Task 1–8——目录、workspace、占位 crate、净室与资产/分层
   xtask 门、refs、文档、许可证、红灯锚点。✅ 2026-10-19 已全部交付。
2. **Slice B（探测纵切，M1）**：Task 9–13——RIFF 走读器、ZIP 容器探测、`probe()` 报告 API、
   CLI `probe`、cleanroom/assets 门自测、格式与对标 parity 文档初版 + 逆向日志规范。
   **禁止在 probe 未全绿前开始 parse 层。**
3. **Slice C（M2 预告，不在本计划）**：doc（含 `Object::Unsupported` 无损透传节点、Page/Master/Desktop 层）+
   color（CMYK/Spot/Overprint 基元）+ render 首帧 + 原生格式 + MCP 读侧薄壳。

## Global Constraints

- **净室铁律**（AGENTS.md）：从不运行/观察/反汇编 CorelDRAW；fixture 一律从
  公开规范手写合成；`cargo xtask cleanroom` 拦截 Corel 版权标记与二进制 fixture。
- **许可**：MIT OR Apache-2.0；不抄 GPL/AGPL/MPL 代码；refs/ 永不提交。
- **敌意输入**：所有格式层 API 返回 `Result`；全 `u64` + `checked_add` 算术；显式嵌套深度上限
  （`MAX_DEPTH = 32`）与 chunk 数量上限（`MAX_CHUNKS = 100_000`）；对 `LIST cmpr`/`stlt` 不按普通子 chunk 展开；
  禁止 `unwrap`/`expect`/`panic`（workspace 级 `deny`）。
- **TDD**：失败测试先行；每个 Task 或 Slice 通过回归并经 `zcode-delegate` 复核后提交（中文说明，无 attribution）。

## File Structure（M1 交付后）

```text
crates/cdr/
├── Cargo.toml
└── src/
    ├── lib.rs          # probe API：ProbeReport、CdrContainer、CdrKind、VersionInfo、probe()
    ├── error.rs        # ProbeError、ProbeWarning（thiserror，#[non_exhaustive]）
    ├── riff.rs         # RIFF/RIFX 走读器：ChunkSummary、迭代与受限下钻、padding、vrsn 提取
    └── zip.rs          # X4+ ZIP 容器探测：EOCD + Central Directory 条目识别与 Stored 内层 RIFF 探测
apps/coralcraft-cli/
└── src/main.rs         # clap：probe [--json] <file>（退出码：0=OK, 2=参数错误, 3=非CDR, 4=IO, 5=损坏/截断）
docs/
├── target-app-parity.md    # 对标 CorelDRAW 功能覆盖矩阵初版（M1 基线）
├── file-format-parity.md   # 格式支持矩阵初版（WL / 裸 RIFF / ZIP-RIFF / CMX 行 + 证据链接）
└── reverse/log-format.md   # 逆向工作日志规范 + 首条示例（RIFF/ZIP 容器与 ASCII 版本编码）
```

## Tasks

### Task 1–8 — M0 骨架与工程基建 ✅（2026-10-19 已完成）

已完成仓库初始化、`AGENTS.md`/`CLAUDE.md`、`docs/ai-memory/`、`.cargo/config.toml`、
`clippy.toml`、`rustfmt.toml`、`xtask`（`ci`/`cleanroom`/`assets`/`layers`/`stats`）与 11 个参考仓固定版本检出。

### Task 9 — `error.rs` 与 `riff.rs` 走读器（红→绿）

1. **红**：编写合成最小合法 RIFF/RIFX 字节流测试：
   - 最小合法 12 字节流（`RIFF` + `size=4` + `CDR7`）；
   - 含 `vrsn`（`u16 LE`，如 `700`/`1400`）、奇数长度 chunk（带 1 字节 pad）与 `LIST INFO` 子 chunk 的多 chunk 合法流；
   - `LIST cmpr` 与 `LIST stlt` 标记为不透明载荷（不作为普通子 chunk 拆解）；
   - 敌意边界：超过 `MAX_DEPTH (32)` 的深层嵌套 `LIST` → `ProbeError::LimitExceeded`；`size = u32::MAX` 越界 → `ProbeError::Truncated`；文件中间缺 pad → `ProbeError::Truncated`；文件末尾奇数 chunk 缺 pad → 容忍并记 `ProbeWarning::MissingTrailingPad`；声明 `size` 小于文件实际长度 → 记 `ProbeWarning::TrailingBytes`。
2. **绿**：实现 `error.rs` 与 `riff.rs`（全 `u64` + `checked_add` 运算、显式栈深度控制、零 `unwrap`）。

### Task 9.5 — `zip.rs` 现代 CDR（X4+）ZIP 容器探测（红→绿）

1. **红**：编写合成最小 ZIP 字节流测试（纯内存构造 Local File Header + Central Directory + EOCD）：
   - 含 `content/riffData.cdr`（Stored 未压缩，内含合法 `RIFF....CDRE` 头）→ 识别为 `CdrContainer::Zip`，且解出内层 `CDRE`（v14/X4 = 1400）；
   - 含 `content/root.dat`（X6+ 布局，伴随 `content/dataFileList.dat`）→ 识别为 `CdrContainer::Zip`；
   - 普通非 CDR 的合法 ZIP（如仅含 `word/document.xml` 的 `.docx` 结构）→ 返回 `Err(ProbeError::NotCdr)`；
   - 伪造 `PK\x03\x04` 但无合法 EOCD / Central Directory 截断 → 返回 `Err(ProbeError::Truncated)` 或 `Invalid`。
2. **绿**：实现无外部重依赖、纯内存切片的 ZIP EOCD + Central Directory 走读器（限制最大条目数 `MAX_ZIP_ENTRIES = 10_000`，仅读取条目名与 Stored 内层头部切片，Deflate 条目上报 `inner: None` 并带说明）。

### Task 10 — `coralcraft-cdr::probe` 报告 API 转绿

1. **红**：更新 `crates/cdr/src/lib.rs` 的红灯测试，覆盖：
   - `RIFF....CDR7`（ASCII `'7'` = `0x37`）→ `CdrKind::Cdr`, `form = *b"CDR7"`, `product_version = Some(700)`；
   - `RIFF....CDRD`（ASCII `'D'` = `0x44`）→ `product_version = Some(1300)`（X3）；
   - `RIFF....CDR `（ASCII `' '` = `0x20`）→ `product_version = Some(300)`（v3）；
   - 小写 `cdr6` → 大小写兼容识别 + `ProbeWarning::NonCanonicalForm`；
   - `RIFF....CMX1` 与大端 `RIFX....CMX1` → `CdrKind::Cmx`, `endian = Little/Big`；
   - `RIFF....WAVE` → `Err(ProbeError::NotCdr)`；
   - X4+ ZIP（含 `content/riffData.cdr`）→ `CdrContainer::Zip`；
   - `WL` 头部（≥ 12 字节）→ `CdrContainer::LegacyWl`；
   - `< 4` 字节 → `Err(ProbeError::TooShort)`；任意非 CDR 数据 → `Err(ProbeError::NotCdr)`。
2. **绿**：接线 `riff.rs` 与 `zip.rs`，产出结构化 `ProbeReport`（派生 `Serialize`/`Deserialize`）。

### Task 11 — CLI `probe` 子命令（`apps/coralcraft-cli`）

1. **红**：编写 CLI 集成测试（`apps/coralcraft-cli/tests/cli_probe.rs`）：
   - 对合成 RIFF CDR 与合成 ZIP CDR 文件断言人类可读 stdout 包含容器类型、表单四码、版本号与 chunk 数量；
   - `--json` 输出可被 `serde_json` 反序列化且含 `"schema_version": 1`；
   - 非 CDR 文件退出码为 `3`（区分于 `clap` 参数错误的退出码 `2`），stderr 含清晰原因。
2. **绿**：实现 `apps/coralcraft-cli/src/main.rs` 的 `probe` 子命令。

### Task 12 — `xtask cleanroom` / `assets` 门自测完善

1. **红**：在 `xtask/src/main.rs` 编写单元测试，用临时目录与运行时拼接的禁止版权串（避免测试源码自指）、伪造 `.cdr` 文件以及未登记资产，断言 `cleanroom_in_root` 与 `assets_in_root` 能准确拦截。
2. **绿**：将 `cleanroom` / `assets` 抽象为接受根目录路径的可测函数并通过全部测试。

### Task 13 — Parity 文档初版 + 逆向日志规范 + ROADMAP 更新

1. 建立 `docs/target-app-parity.md`（对标 CorelDRAW 核心工作流维度基线表）与 `docs/file-format-parity.md`（区分 `WL`、裸 `RIFF`、现代 `ZIP+RIFF`、`CMX`、`SVG`、`PDF` 等格式矩阵）。
2. 建立 `docs/reverse/log-format.md`（逆向工作日志规范 + 首条容器与版本编码记录）。
3. 更新 `ROADMAP.md` 与 `docs/gaps.md` 中 M1 已完成状态与自评分数。

## 验收（M1 Definition of Done）

```bash
cargo xtask ci                                              # fmt + clippy + cleanroom + assets + layers + 全量测试全绿
cargo run -p coralcraft-cli -- probe --json <合成.cdr>      # 输出 schema_version=1 的 JSON 报告
cargo run -p coralcraft-cli -- probe <非CDR文件>            # 干净报错，退出码 3（不与 clap 的 2 冲突）
```

## 计划外变更记录

- **2026-10-19（五方独立评审修订）**：根据 GLM-5.3、Kimi K3、DeepSeek V4.1 Flash、Grok 4.6 与 Claude Opus 5.5 High 的评审结论，在 M1 补入 **Task 9.5（X4+ ZIP 容器探测）**、修正 ASCII 版本字符编码（`' '`=300, `'3'..'9'`=300..900, `'A'..'Z'`=1000..3500）与 `RIFX` 大端 CMX 支持、修复 12 字节最小合法 RIFF 夹具、增加 `LIST cmpr`/`stlt` 不透明处理与 `MAX_DEPTH=32` 限制、并将 CLI 非 CDR 退出码调整为 `3`。
