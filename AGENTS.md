# CoralCraft — instructions for agents

CoralCraft 是一个净室（clean-room）、开源、Rust 原生的矢量插画 + 多页排版应用，
目标 CorelDRAW parity（及超越）。运行于 macOS / Windows / Linux，后续经 WASM 上 Web。
方法论与结构对标 [storytold/vectorcraft](https://github.com/storytold/vectorcraft)
（Illustrator parity）及其家族约定。

## 每个会话从这里开始

1. 读 [`ROADMAP.md`](ROADMAP.md)（阶段、各维度数字、下一步）与
   [`docs/gaps.md`](docs/gaps.md)（排序后的差距清单）。除非操作者给定任务，
   从差距清单选活干。
2. 读当前里程碑计划 `docs/superpowers/plans/*.md`，按其中的 Task 执行。
3. 遵循自主运行协议：定向 → 计划 → 失败测试先行 → 实现 → 全量回归 → 记录 → 提交。
   除非计划中标注为"操作者裁决"，不要停下来问。

`plan/` 是 gitignored 的本地会话笔记目录。

## 不可协商项（绝对规则）

### 净室：从不使用 Corel 软件（项目所有者铁律）

Corel 的 EULA 禁止逆向其软件（包括观察输入输出来重建行为）。因此：

- **从不**启动、运行、脚本化、自动化（VBA/COM/UI automation）、截图或测量
  CorelDRAW 或任何 Corel 应用，**从不**把我们的输出与它对比。
  不用 Corel 软件制造测试文件，"合成的"也不行。
- **从不**读取、反汇编或复制 Corel 安装目录内的任何东西（二进制、预设、
  资源、帮助文件、字体、ICC profile、对象模型文档副本）。
- **从不**把 Corel 撰写的代码或文本复制进本仓（测试也不行）。测试输入一律
  从公开规范（Photopea CDR 规范、RIFF、PDF/ISO 32000 等）从零手写。
- 行为只来自**公开文档**（Corel 官方在线手册、出版书籍、公开教程、论坛帖子，
  逐条引用 URL），以及**操作者自有的、发给我们本地使用的文件**
  （永不提交进仓）。
- 除功能名称外，**从不**复制 Corel 图标、 artwork、预设或措辞。
- 某个任务看似需要以上任何一条时：停下并说明，而不是做。
  `cargo xtask cleanroom`（由 `cargo xtask ci` 运行）扫描 Corel 版权标记与
  Corel 软件生成的 fixture。

### 许可：不抄 GPL/AGPL，不抄 MPL 代码

- **从不**复制 GPL/AGPL 代码（Inkscape、UniConvertor、lib2geom、sK1……）。
  需要理解其思路时只读文档与论文，并在代码注释中引用出处 URL，不搬代码。
- libcdr 是 MPL-2.0：可以读（作为格式语义参考与差异验证），**从不**逐行搬运；
  若确需集成，必须单独立项评估 MPL 条款，默认不集成、走独立实现。
- 代码与原创资产：`MIT OR Apache-2.0`。

### 资产：永不使用 Corel 图标或图像

- 永不添加、复制、描摹、内嵌或分发来自 Corel 产品或任何许可不兼容来源的
  图标、图片、光标、预设、色板/符号库、ICC profile 或截图。
- 每个资产文件必须在 [`ASSETS.md`](ASSETS.md) 有登记行（路径、作者、来源 URL、
  许可、备注）；许可文本放在资产旁；`cargo xtask assets` 拦截未登记资产。
- 字体放独立仓库（参照 storytold/craft-fonts 模式），永不进本仓。

### 质量：发布的代码不 panic

- 库层返回 `Result`，解析器对敌意输入返回错误而不是 panic（见
  vectorcraft 的 hostile-input 传统；CDR 文件按不可信输入处理）。
- 每个 crate 的公共 API 变更先写失败测试。

## 提交与流程

- 严格 TDD（失败测试先行）；conventional commits（feat/fix/docs/test/chore），
  中文提交说明，无 attribution。
- 每 Task 一个提交；不跨失败的 Slice 提交（见计划文档）。
- `refs/` 目录永不提交（第三方代码只在本地参考）。

## 路线与文档

- 阶段与自评分：[`ROADMAP.md`](ROADMAP.md)
- 对标评估：`docs/target-app-parity.md`（M1 建立）
- 格式支持矩阵：`docs/file-format-parity.md`（M1 建立）
- 差距清单：`docs/gaps.md`
- 逆向工作日志（格式层）：`docs/reverse/`（记录 chunk 语义发现，含证据文件 hash，
  不含第三方代码）
