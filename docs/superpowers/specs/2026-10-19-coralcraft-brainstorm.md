# CoralCraft 头脑风暴：CorelDRAW 的开源净室重实现是否成立、怎么立项

> 日期：2026-10-19 · 状态：brainstorm 产物（自主模式，含假定裁决，开放问题见 §8）
> 流程位置：brainstorm → **本文** → spec（M1 前）→ writing-plans → subagent-driven development

## 1. 问题陈述

CorelDRAW 在矢量插画 + 多页排版 + 印刷生产（标识、雕刻、广告制作、出版）领域
仍有大量存量付费用户与海量 `.cdr` 资产，但：

1. 无任何开源软件能读写 CDR（读：libcdr/UniConvertor/Photopea 部分覆盖；写：零）。
2. 无任何开源应用对 CorelDRAW 做 parity（Inkscape 对标 Illustrator，sK1 已停滞，
   LibreOffice Draw 是通用绘图而非对标复刻）。
3. 印刷/标识/雕刻社区被 Adobe 抛弃、被 Corel 年费绑架，是付费意愿最强的弃儿群体。

**核心问题**：用「行为公开文档净室 + 格式逆向」双轨方法论（ArtCraft 已在 13 个
应用上工业化验证），为 CorelDRAW 建一个开源等价物（CoralCraft），是否成立、
风险何在、如何切里程碑？

## 2. 事实基础（调研结论，2026-10-19）

- ArtCraft 家族（storytold org，55 仓）：13 个 clean-room 应用对标 Photoshop/
  Illustrator/Premiere/Acrobat/Word/Excel/AutoCAD 等，全部纯 Rust、MIT OR
  Apache-2.0、全部活跃。PhotoCraft 67.8k star，vectorcraft 三周内从建仓到
  alpha 自评 65% 就绪。方法论：行为只来自公开文档（AGENTS.md 铁律），格式
  需要时逆向（`crates/affinity` 读未公开的 .af、`indd-utils` 解析 INDD）。
- vectorcraft 对 Illustrator 的 parity 已覆盖 CorelDRAW 插画面的大半
  （Pen/Pathfinder/文字/渐变/打印/PDF/SVG/EPS/DXF）。
- CDR 格式：RIFF 容器（1992 后）+ 老的 `WL` 前缀格式；PhotoPea 规范给出
  结构级人类可读文档；libcdr（MPL-2.0）声称解析全版本；UniConvertor（AGPL）
  另覆盖 CMX/CCX。Corel 从未官方公开规范，但 15+ 年从未追究任何公开逆向。
- 法律面：格式/接口本身不受版权保护；美欧判例与指令为互操作性逆向留有通道
  （详见 §6 风险）。
- `craftrules` 仓库已 404（vectorcraft AGENTS.md 的引用失效），家族共享基建
  实际落在 `craft-libs`（craft-pdf/craft-print/craft-docpath/craft-ui）、
  `craft-common-actions`（CI）、`craft-fonts`（字体）。

## 3. 目标 / 非目标

**目标（G1–G4）**

- G1 **应用等价**：CorelDRAW 主要工作流（插画、多页文档、印刷输出）达到
  「存量用户可迁移」的 parity，自有原生格式 `.coralcraft`（JSON）。
- G2 **CDR 读**：打开/置入 `.cdr`（RIFF 全版本 + 尽可能多的 `WL`），转换损失
  有明确warning 台账（仿 vectorcraft import warning 模式）。
- G3 **CMX 读**：Corel 交换格式（对外协作的老通道）。
- G4 **CDR 写（长期战役）**：写出真实 CorelDRAW 可打开的 `.cdr`。无先例，
  允许降级承诺（如仅写出被广泛可读的子集版本）。

**非目标（NG1–NG4）**

- NG1 不复刻 Corel PHOTO-PAINT / Font Manager / CorelDRAW.App 全家桶（那是
  另一个 app 的位置，家族已有 PhotoCraft）。
- NG2 不内置微软 VBA；宏兼容走自有 Basic 方言（LibreOffice StarBasic 先例），
  M4+ 再议。
- NG3 不做云服务；本地优先。
- NG4 不追求像素级 UI 复刻；「熟悉的工作流」而非「相同的截图」。

## 4. 关键裁决（自主模式下的假定，操作者可推翻）

| # | 裁决 | 理由 |
|---|---|---|
| D1 | 纯 Rust，workspace 布局与 crate 划分对齐 vectorcraft（geom/color/doc/cdr/render/tools/engine/ui-egui/mcp + apps + xtask） | 家族基建（craft-libs、craft-common-actions、craft-fonts）直接复用；agent 生态一致 |
| D2 | UI 用 egui（独立 `ui-egui` crate，可换） | vectorcraft 同款；引擎与 UI 解耦 |
| D3 | 净室铁律照抄 vectorcraft 措辞，对象换成 Corel：从不运行/观察/反汇编 CorelDRAW；行为只来自公开文档 | Corel EULA 同样禁逆向；这是法务防火墙 |
| D4 | 格式轨：以 Photopea 规范为结构圣经，libcdr 只作语义差异验证（读，不抄码），UniConvertor 只读其文档不进 refs 代码树 | AGPL 不可抄；MPL 可读不可搬 |
| D5 | 许可 `MIT OR Apache-2.0`，双文件 + NOTICE | 家族一致 |
| D6 | 逆向工具链引入 REA（cyberkaida/reverse-engineering-assistant，Ghidra MCP）等助手，放 `refs/`，仅用于分析**我们自有的样例文件**，不用于 Corel 二进制 | 用户点名要求；用途合规边界写死 |
| D7 | 仓库位置 `/Users/paris/Developer/coralcraft`（与 aide-captain 平级的独立仓） | 独立项目不混入私有仓 |
| D8 | 命名 CoralCraft（非 CorelDraw/CorlCraft 等任何含 Corel 商标的拼法） | 商标红线 |

## 5. 架构草图（M0 骨架已落地）

```text
coralcraft/
├── crates/
│   ├── geom        几何（贝塞尔、布尔、捕捉）      ← 借鉴 vectorcraft-geom 结构
│   ├── color       色彩（CMYK/专色/色板——Corel 用户刚需）
│   ├── doc         文档模型（多页！≠vectorcraft画板，借鉴 designcraft 页模型）
│   ├── cdr         CDR/CMX 格式层（probe→parse→build 三阶段）★净室格式轨
│   ├── render      SIMD 渲染
│   ├── tools       工具/面板语义
│   ├── engine      引擎组装
│   ├── ui-egui     UI（可换）
│   └── mcp         MCP server（agent 驱动）
├── apps/coralcraft（桌面） apps/coralcraft-cli（CLI+headless）
├── xtask/          ci/cleanroom/assets/bundle
├── refs/           参考仓库（git-ignored，永不提交）+ REFS.md 索引
└── docs/superpowers/{specs,plans} + reverse/（逆向工作日志）
```

**与 vectorcraft 的关键差异**：多页文档模型（借 designcraft）、CDR 格式轨
（独有 crate）、CMYK/专色优先级前移（Corel 用户在印刷现场）、色板文件
（.cpl/.xml palette）支持。

## 6. 风险与缓解

| 风险 | 等级 | 缓解 |
|---|---|---|
| 法律：Corel 主张 EULA/版权 | 低-中 | 双轨净室（行为/格式分离）；格式事实不受版权保护（Baker v. Selden 一脉、欧盟 2009/24/EC）；15 年无人被诉的实践先例；不放 Corel 资产/文本/码表 |
| 法律：专利（描摹/色彩管理算法） | 中 | 实现前检索；老专利多已过期；绕开仍在期专利 |
| CDR 写兼容性 | 高 | 独立长战役（M5+）；先 promise 子集；用 libcdr/Photopea/多版本 Corel 试用版？❌ 不行——净室禁令；改用「用户社区回归矩阵」（志愿者用正版验证，见 §8 Q3） |
| 规模（35 年功能面） | 高 | parity 文档自评分驱动；家族 crate 复用；agent-native 开发速度 |
| 样例语料不足 | 中 | M1 起建 `coralcraft-corpus` 独立仓模式（借鉴 photocraft-corpus）；libcdr 测试集（MPL）可本地对照，不进本仓 |
| 单人/小队不可持续 | 高 | MCP 驱动社区贡献模式是家族验证过的解法；本项目从 day 1 agent-native |

## 7. 里程碑切分（供 plan 展开）

- **M0 bootstrap（本日）**：骨架、净室规则、refs、计划文档、TDD 红灯锚点。
- **M1 格式探测**：`coralcraft-cdr` probe（RIFF/WL/版本字节/chunk 走读器）+
  `docs/file-format-parity.md` 初版 + 逆向工作日志格式。纯合成 fixture。
- **M2 文档模型与渲染**：doc（多页）+ render 首帧 + 原生格式往返（属性测试，
  仿 vectorcraft prop_format）。
- **M3 CDR 读 v1**：RIFF 版本 ≥ X4 的页面/对象/填充/文本基础解析 → 转
  `.coralcraft`。导入 warning 台账。
- **M4 UI + 工具纵切**：egui 壳、选择/形状/钢笔、页面面板、CMYK 色板。
- **M5+ **：MCP、CMX、更多版本、CDR 写战役、印刷生产（陷印/拼版/专色）。

## 8. 开放问题（操作者裁决点）

- Q1（优先级）：M3 的 CDR 读先攻哪个版本带？（假定：X4–2026 的 RIFF 现代带，
  因存量最多；老 `WL` 缓行）
- Q2（语料）：操作者手上有没有自有 CDR 样例可提供做 fixture（本地使用、永不
  提交）？没有则 M3 前需要解决语料来源。
- Q3（写验证）：CDR 写出后的「真实 CorelDRAW 可打开」验证，是否组织社区志愿
  者回归矩阵（志愿者自持正版，不违反我们的净室禁令——禁令约束本仓与 agent，
  不约束外部用户）？
- Q4（品牌）：是否加入 ArtCraft 家族命名/站点，还是完全独立运作？（当前假定：
  独立仓、兼容家族约定，未与 storytold 建立任何关系）

## 9. 结论

**成立。** 方法论已由 ArtCraft 家族在 13 个应用上验证为可工业复制；CorelDRAW
的格式逆向已完成 60%（读侧有三方开源成果可对照），缺的正是「一个对标
CorelDRAW 的应用壳 + 写出侧战役」。风险集中在 CDR 写兼容性与规模，均有缓解
路径。按 §7 里程碑推进，M1–M4 可在数月量级内交出「能开 CDR、能画、能排页」
的纵切。
