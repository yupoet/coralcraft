# 03 — 当前任务、进行中问题与下一步

> 最后更新：2026-10-19

## 1. 当前阶段状态

- **阶段**：`pre-alpha`（**M0 与 M1 已 100% 交付并通过全量 CI 门禁**，进入 **M2 启动前**）。
- **本轮已完成工作**：
  1. **五路大模型交叉评审（Five-Way Plan Review）**：
     - 在 `zcode-delegate`（GLM-5.3）、`kimi-delegate`（Kimi K3）、`grok-delegate`（Grok 4.6）、`codebuddy-delegate`（DeepSeek V4.1 Flash）基础上，补充第五路内部模型 **`claude-opus-5-5-high`（Claude Opus 5.5）** 深度审查，并将全部结论汇总至 [`docs/ai-memory/05-ref-docs/04-m1-plan-four-way-review.md`](05-ref-docs/04-m1-plan-four-way-review.md)。
     - Opus 5.5 独特发现已全部吸收进 M1 代码与架构文档：Mac CMX 的大端序 `RIFX`、X6+ ZIP 容器的 `content/root.dat` + `content/dataFileList.dat` 配对、CDR v3 的空格版本字节 `"CDR "` (`0x20` → `300`) 及 2024/2025 的 `'Q'`/`'R'`、M3 预烘焙几何兜底、以及 `0.1 μm` (`1/254,000 inch`) 精确整数单位。
  2. **M1：CDR/CMX 格式探测纵切（Task 9–13 全部转绿）**：
     - [`crates/cdr/src/error.rs`](../../crates/cdr/src/error.rs)：定义 `#[non_exhaustive]` 的 `ProbeError` 与 `ProbeWarning`，支持 `serde::Serialize` / `Deserialize`。
     - [`crates/cdr/src/riff.rs`](../../crates/cdr/src/riff.rs)：实现 `inspect_riff`，支持 `RIFF`（小端）与 `RIFX`（大端）、`MAX_DEPTH = 32`、`MAX_CHUNKS = 100_000`、`u64` + `checked_add` 溢出防护、`vrsn` chunk 提取、`LIST cmpr`/`LIST stlt` 压缩/样式表载荷 opaque 跳过、以及 EOF 缺失尾随 pad byte 容忍（记录 warning）。
     - [`crates/cdr/src/zip.rs`](../../crates/cdr/src/zip.rs)：零外部解压依赖手写 ZIP EOCD + Central Directory 扫描器（`MAX_ZIP_ENTRIES = 10_000`），精准区分 X4–X5 (`content/riffData.cdr`)、X6+ (`content/root.dat` + `content/dataFileList.dat`) 与普通 `.zip`/`.docx`，并在条目为 `Stored (method == 0)` 时直接下钻解析内部 RIFF 头。
     - [`crates/cdr/src/lib.rs`](../../crates/cdr/src/lib.rs)：实现 `probe() -> Result<ProbeReport, ProbeError>`，覆盖 Legacy `WL`、Bare `RIFF`/`RIFX`、现代 `ZIP+RIFF` 三代容器；22 个单元测试（含 10,000 次确定性敌意变异测试 `hostile_random_mutations_never_panic`）全部通过。
     - [`apps/coralcraft-cli/src/main.rs`](../../apps/coralcraft-cli/src/main.rs) 与 [`apps/coralcraft-cli/tests/cli_probe.rs`](../../apps/coralcraft-cli/tests/cli_probe.rs)：实现 `coralcraft-cli probe [--json] <path>`，规范化退出码（`0=OK`, `2=用法`, `3=NotCdr/TooShort`, `4=I/O`, `5=Truncated/Invalid/LimitExceeded`），4 个集成测试全绿；并在 `refs/cdr-samples/` 全部 64 个真实 `.cdr` 样本上达成 **64/64 (100%)** 识别通过。
     - [`xtask/src/main.rs`](../../xtask/src/main.rs)：新增 `cleanroom`（运行期拼接违规串 + `.cdr` 文件检测）与 `assets`（未登记资产拦截）单元测试（Task 12）。
     - 建立 [`docs/target-app-parity.md`](../target-app-parity.md)、[`docs/file-format-parity.md`](../file-format-parity.md)、[`docs/reverse/log-format.md`](../reverse/log-format.md)（含 `REV-0001`），并更新 [`ROADMAP.md`](../../ROADMAP.md) 与 [`docs/gaps.md`](../gaps.md)（Task 13）。

## 2. 下一步待执行任务（M2：多页文档模型 + 原生格式 `.coralcraft` + 渲染首帧）

1. **`coralcraft-doc` 多页出版文档模型**：
   - `Document`、`Spread`、`Page`（支持单页独立尺寸与出血）、文档级 `Layer` 栈 + 跨页共享 `Desktop` 层。
   - 稳定句柄 `ObjectId(u64)`（供 PowerClip 容器与调和路径引用）与 `Object::Unsupported { kind, raw_ref, fallback_bounds }` 无损透传节点。
2. **`.coralcraft` 原生格式与属性测试往返**：
   - 支持单文件 JSON 与 ZIP 容器（`manifest.json` + `document.json` + `blobs/`），通过 `proptest` 属性测试保证 `load(save(doc)) == doc`。
3. **`coralcraft-render` 首帧软光栅**：
   - 基于 `tiny-skia` 渲染指定页面为 PNG，并提供 `coralcraft-cli render <input> -o <out.png>`。

## 3. 本地 CDR 样本语料验证结果（2026-10-19）

- `refs/cdr-samples/`：共 92 个文件（64 个完整真实 `.cdr` 图纸 + 28 个 PRONOM 零字节填充签名骨架）。
- `coralcraft-cli probe` 对全部 **64 个真实 `.cdr` 样本（覆盖 CDR4..CDRF 及 X7+ ZIP 容器）100% 探测成功（64/64 OK）**，无一报错或 panic。
