# 03 — 当前任务、进行中问题与下一步

> 最后更新：2026-10-19

## 1. 当前阶段状态

- **阶段**：`pre-alpha`（M0 骨架已全部交付并对齐 `vectorcraft` 工程规范，处于 **M1 启动前**）。
- **本轮已完成工作**：
  1. 克隆仓库 `https://github.com/yupoet/coralcraft` 至 `/data/coralcraft`。
  2. 执行 `./scripts/bootstrap-refs.sh` 下载全部 11 个参考仓库至 `refs/`，并将 `scripts/bootstrap-refs.sh` 升级为 40 位完整 SHA，确保 11 个参考仓全部精确检出在 [`refs/REFS.md`](../../refs/REFS.md) §D 冻结版本。
  3. 建立 `docs/ai-memory/` 持久记忆体系，并将读写规范写入 [`AGENTS.md`](../../AGENTS.md) 与 [`CLAUDE.md`](../../CLAUDE.md)。
  4. **全面对齐 `refs/vectorcraft` 风格与行为**：
     - 补齐 [`.cargo/config.toml`](../../.cargo/config.toml)（支持 `cargo xtask <cmd>` 别名）、[`clippy.toml`](../../clippy.toml)（测试允许 unwrap/expect/panic，库与二进制严禁）与 [`rustfmt.toml`](../../rustfmt.toml)（`max_width = 150`、`use_small_heuristics = "Max"`）。
     - 对齐 [`Cargo.toml`](../../Cargo.toml) 的 `[workspace.lints.clippy]`（`dbg_macro = "warn"`, `unwrap_used = "deny"`, `expect_used = "deny"`, `panic = "deny"`）及 `[profile.dev.package."*"] opt-level = 2`。
     - 升级 [`xtask/src/main.rs`](../../xtask/src/main.rs)：新增 `layers`（强制执行 `L0`..`L7` 分层架构，禁止 `L6` 以下依赖 UI 库）与 `stats`（按 crate 统计文件数、代码行数与 `#[test]` 数），并将 `cleanroom`、`assets`、`layers` 全部并入 `cargo xtask ci`。
     - 重写 [`README.md`](../../README.md) 首页与 GitHub `About`：采用与 `vectorcraft/README.md` 完全一致的章节布局、徽章配色（`#e8573f` / `#b83a24` / `#555555`）与 Star History 图表，同时在首屏与核心卖点中突出 **CorelDRAW**（及常见搜索拼写 *CoralDraw*）与 `.cdr` / `.cmx` 格式原生支持。

## 2. 下一步待执行任务（M1：CDR 格式探测纵切，见 [`docs/gaps.md`](../gaps.md)）

按 [`docs/superpowers/plans/2026-10-19-m0-bootstrap-m1-cdr-probe.md`](../superpowers/plans/2026-10-19-m0-bootstrap-m1-cdr-probe.md) 的 Slice B 顺序推进：

1. **Task 9 — RIFF 走读器（`crates/cdr/src/riff.rs`，红 → 绿）**
   - 编写合成最小 RIFF/CDR 字节流测试（`RIFF` + u32 LE size + `CDR` + 版本字节、`LIST` 子 chunk、奇数长度 1 字节 padding、截断与越界错误路径）。
   - 实现 `RiffReader` 与 chunk 迭代，绝不 panic。
2. **Task 10 — `coralcraft-cdr::probe` API 转绿**
   - 将 [`crates/cdr/src/lib.rs`](../../crates/cdr/src/lib.rs) 中现有的 5 个红灯测试全部转绿（识别 RIFF CDR/CMX + 版本字节、识别老 `WL` 头、拒绝过短或非 CDR 输入）。
   - 预留 `cargo fuzz` 目标占位。
3. **Task 11 — CLI `probe` 子命令（`apps/coralcraft-cli`）**
   - 集成测试先行：验证人类可读输出、`--json` 序列化输出、非 CDR 文件返回非零退出码且 stderr 含清晰错误信息。
4. **Task 12 — `xtask cleanroom` / `assets` 门完善**
   - 用临时违规样例驱动测试，完善 `xtask` 扫描器。
5. **Task 13 — 格式 parity 文档初版 + 逆向日志规范**
   - 建立 `docs/file-format-parity.md` 与 `docs/reverse/log-format.md`（含首条 RIFF 头部发现示例），更新 `ROADMAP.md` 自评分数。

## 3. 挂起的操作者裁决点（M3 前需确认，不阻塞 M1/M2）

- **Q1（M3 版本带优先级）**：默认先攻 X4–2026 的现代 RIFF 带。
- **Q2（自有样例语料）**：M3 解析真实对象树前，需确认使用操作者自有 `.cdr` 样例（本地 git-ignored）或独立 `coralcraft-corpus` 仓库。
- **Q3（CDR 写出兼容性验证）**：M5+ 写出战役拟采用外部社区志愿者回归矩阵验证。
