# 01 — ArtCraft (`storytold`) 生态与净室方法论梳理

> 调研日期：2026-10-19 · 来源：`https://getartcraft.com/apps` 与 `https://github.com/orgs/storytold/repositories`

## 1. ArtCraft "Crafting Apps" 13 应用矩阵

ArtCraft 是一套用纯 Rust 从零编写、采用宽松开源许可（`MIT OR Apache-2.0` 或 `Apache-2.0`）的商业软件净室重实现套件，支持 macOS、Windows、Linux 及 Web（WASM）：

| # | 应用 | 对标商业软件 | 状态（2026-10） | Stars（2026-10-19） | 与 CoralCraft 的关系 |
| :--- | :--- | :--- | :--- | ---: | :--- |
| 1 | **PhotoCraft** | Photoshop | Early alpha | 67,775 | 位图编辑，家族已有故 CoralCraft 不做 PHOTO-PAINT |
| 2 | **PdfCraft** | Acrobat | Early alpha | 16,019 | PDF 阅读/拆分/印前管线参考 |
| 3 | **WordCraft** | Word | In development | 14,484 | 富文本流式排版参考 |
| 4 | **LightCraft** | Lightroom | In development | 13,740 | 非破坏性色彩管线参考 |
| 5 | **VectorCraft** | Illustrator | In development (alpha) | 9,886 | **工程与方法论母本**（几何/布尔/文字/描摹/打印） |
| 6 | **DeckCraft** | PowerPoint | In development | 3,830 | 多页演示模型 |
| 7 | **SoundCraft** | Pro Tools | In development | 3,291 | DAW |
| 8 | **FigCraft** | Figma | Early alpha | 2,695 | 界面设计与协作画布 |
| 9 | **DesignCraft** | InDesign | In development | — | **多页/跨页排版文档模型核心参考**（`refs/designcraft/`） |
| 10 | **CADCraft** | AutoCAD | In development | — | **未公开二进制格式 DWG/DXF 净室解析先例**（`refs/cadcraft/`） |
| 11 | **FilmCraft** | Premiere Pro | In development | — | 视频非线性编辑 |
| 12 | **EffectCraft** | After Effects | In development | — | 动态图形与特效 |
| 13 | **GridCraft** | Excel | In development | — | 电子表格 |

## 2. VectorCraft 对 CorelDRAW 场景的覆盖与缺口

- **已覆盖能力**：Pen 钢笔、Pathfinder 布尔运算、Appearance 外观、渐变、CJK 与双向文字引擎、Image Trace（对标 PowerTRACE）、SVG/PDF/EPS/DXF 导入导出（含对 CorelDRAW 导出 EPS 的兼容测试）。
- **核心缺口（CoralCraft 立项原因）**：
  1. **完全不支持 `.cdr` / `.cmx`**：`vectorcraft/docs/file-format-parity.md` 明确标注 `CorelDRAW .cdr | open: no | save: no`。
  2. **工作范式差异**：VectorCraft 采用 Illustrator 的单画板（Artboard）范式，而 CorelDRAW 核心用户依赖多页文档（Page/Spread）、主页面（Master Page）与印刷拼版工作流。
  3. **行业刚需特性**：标识/雕刻/印刷行业重度依赖专色/CMYK 预设、多页拼版及宏自动化。

## 3. ArtCraft 工程方法论提炼

1. **行为层净室**：只用公开文档（官方手册、公开规范、论坛帖子，逐条引用 URL），从不运行、截图或反汇编对标软件。
2. **格式层逆向先例**：对未公开的私有格式（如 VectorCraft 的 `crates/affinity` 原生读取 `.af`、`storytold/indd-utils` 解析 InDesign `.indd`、`cadcraft` 解析 DWG），采用自有样本与公开逆向知识独立实现解析器，并用属性测试（`prop_format.rs`）保障往返一致性。
3. **资产净室**：所有图标、光标、色板、样本必须原创或采用兼容开源许可，`ASSETS.md` 全量登记，`cargo xtask assets` 自动拦截。
4. **字体分仓**：字体统一放在 [`storytold/craft-fonts`](https://github.com/storytold/craft-fonts)，通过 `CRAFT_FONTS_DIR` 环境变量可选接入，主仓永不提交字体文件。
5. **Agent-native 开发**：内置 CLI、JSON 控制通道与 MCP Server，使 AI Agent 可直接驱动应用验证功能。
