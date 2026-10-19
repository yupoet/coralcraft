# CoralCraft 路线图

> 阶段基线：**pre-alpha（骨架）** · 建立于 2026-10-19 · 更新：2026-10-19
> 方法：行为公开文档净室 + 格式逆向双轨（见 AGENTS.md 铁律、brainstorm 文档）。
> 自评分制度仿 vectorcraft 的 target-app-parity：每个维度 0–100%，每 Task 更新；
> 数字是诚实估计而非承诺，评分方法随 M1 的 parity 文档一并说明。

| 维度 | 现状 | 目标（usable alpha） |
|---|---:|---:|
| 工程骨架（workspace/xtask/CI/净室门） | 100% | 维持 |
| CDR 格式：probe（容器/版本/chunk 走读） | 0%（红灯锚点就绪） | M1：100% |
| CDR 格式：parse（对象树→文档模型） | 0% | M3：现代 RIFF 带基础集 |
| CDR 格式：write | 0% | 长战役（M5+，先承诺子集） |
| 文档模型（多页）与原生格式往返 | 0% | M2：100%（属性测试） |
| 渲染（首帧→交互帧率） | 0% | M2 首帧 / M4 交互 |
| 工具/面板/UI 纵切 | 0% | M4：选择/形状/钢笔/文本/页面/色板 |
| 打印与印刷生产（陷印/拼版/专色/PDF-X） | 0% | M5+ |
| MCP / agent 驱动 | 0% | M5 |
| 对标 CorelDRAW 的行为 parity | 0% | M1 建立 parity 文档基线 |

## 最近（M1，见计划文档 Task 9–13）

1. `coralcraft-cdr`：RIFF 走读器 + probe 转绿 + CLI `probe` 子命令。
2. `docs/file-format-parity.md` 初版 + `docs/reverse/` 逆向日志规范。
3. cleanroom/assets 门完善（Task 12）。

## 中期（M2–M4）

- M2：doc（多页）+ render 首帧 + `.coralcraft` 原生格式属性测试往返。
- M3：CDR 读 v1（现代 RIFF 版本带），导入 warning 台账；语料策略落地
  （brainstorm §8 Q2：操作者样例 or corpus 仓）。
- M4：egui 壳 + 工具纵切 + CMYK/专色/色板。

## 长期（M5+）

- MCP server、CMX、更多 CDR 版本带、CDR 写战役、印刷生产深水区、
  宏方言（StarBasic 先例）评估。

## 变更日志

- 2026-10-19：建仓；M0 骨架（Task 1–8）交付；M1 红灯锚点就绪。
