# CoralCraft 路线图

> 阶段基线：**pre-alpha（M1 已交付）** · 建立于 2026-10-19 · 更新：2026-10-19
> 方法：行为公开文档净室 + 格式逆向双轨（见 AGENTS.md 铁律、brainstorm 文档）。
> 自评分制度仿 vectorcraft 的 target-app-parity：每个维度 0–100%，每 Task 更新；
> 数字是诚实估计而非承诺，评分方法见 [`docs/target-app-parity.md`](docs/target-app-parity.md) 与 [`docs/file-format-parity.md`](docs/file-format-parity.md)。

| 维度 | 现状 | 目标（usable alpha） |
|---|---:|---:|
| 工程骨架（workspace/xtask/CI/净室门） | 100% | 维持 |
| CDR 格式：probe（容器/版本/chunk 走读） | **100%**（WL / RIFF+RIFX / X4+ ZIP+RIFF 三代容器、CLI `probe`、64/64 真实样本 100% 识别） | M1：100%（已达成） |
| CDR 格式：parse（对象树→文档模型） | 0% | M3：现代 RIFF 带基础集 |
| CDR 格式：write | 0% | 长战役（M5+，先承诺子集） |
| 文档模型（多页）与原生格式往返 | 0% | M2：100%（属性测试） |
| 渲染（首帧→交互帧率） | 0% | M2 首帧 / M4 交互 |
| 工具/面板/UI 纵切 | 0% | M4：选择/形状/钢笔/文本/页面/色板 |
| 打印与印刷生产（陷印/拼版/专色/PDF-X） | 0% | M5+ |
| MCP / agent 驱动 | 0% | M5 |
| 对标 CorelDRAW 的行为 parity | 5%（parity 文档基线已建立） | M1 建立 parity 文档基线（已达成） |

## 已完成（M0–M1，见计划文档 Task 1–13）

1. **M0 工程骨架**：Workspace 12 个 crate 分层架构、`xtask`（`ci`/`cleanroom`/`assets`/`layers`/`stats`）、11 个 `refs/` 参考仓冻结拉取。
2. **M1 CDR/CMX 格式探测纵切**：
   - `coralcraft-cdr`：支持 Legacy `WL`、Bare `RIFF`/`RIFX`、现代 `ZIP+RIFF`（X4–X5 `content/riffData.cdr` 与 X6+ `content/root.dat` + `content/dataFileList.dat`）三代容器；精确解码 `' '` / `'3'`..`'9'` / `'A'`..`'R'` ASCII 版本字节与 `vrsn` chunk；对 `LIST cmpr` / `LIST stlt` 执行 opaque 载荷跳过；通过 10,000 次确定性敌意变异测试（零 `unsafe`、零 `unwrap`）。
   - `coralcraft-cli probe`：支持人类可读输出与 `--json` 机器可读输出（退出码 `0/2/3/4/5`），在本地 `refs/cdr-samples/` 全部 64 个真实 `.cdr` 样本上达成 100% 探测通过。
   - 建立 [`docs/target-app-parity.md`](docs/target-app-parity.md)、[`docs/file-format-parity.md`](docs/file-format-parity.md) 与 [`docs/reverse/log-format.md`](docs/reverse/log-format.md)（含首条 `REV-0001` 逆向记录）。

## 最近（M2：多页文档模型 + 原生格式 `.coralcraft` + 渲染首帧）

1. `coralcraft-doc`：多页出版模型（`Document` / `Spread` / `Page` / 文档级 `Layer` + `Desktop` 层 / `ObjectId` 稳定句柄 / `Object::Unsupported` 无损透传节点）。
2. `.coralcraft` 原生格式：ZIP + `manifest.json` + `document.json` + `blobs/`（及单文件 JSON 模式），`proptest` 属性测试往返。
3. `coralcraft-render`：基于 `tiny-skia` 的静态页面首帧光栅化（PNG 输出 + `coralcraft-cli render`）。

## 中期（M3–M4）

- M3：CDR 读 v1（`LIST cmpr` 双流解压 + `page`/`layr`/`obj ` 解析 + 预烘焙几何兜底），导入 warning 台账。
- M4：egui 壳 + 工具纵切 + CMYK/专色/色板（用户 `.cpl`/`.xml` 导入）。

## 长期（M5+）

- MCP server、CMX、更多 CDR 版本带、CDR 写战役、印刷生产深水区、宏方言评估。

## 变更日志

- 2026-10-19：M1 交付（`coralcraft-cdr` 三代容器走读器、`coralcraft-cli probe`、`xtask` 扫描门单测、格式与对标 parity 基线文档、首条逆向日志 `REV-0001`）；通过五路大模型独立评审（GLM-5.3、Kimi K3、Grok 4.6、DeepSeek V4.1 Flash、Claude Opus 5.5）。
- 2026-10-19：建仓；M0 骨架（Task 1–8）交付；M1 红灯锚点就绪。
