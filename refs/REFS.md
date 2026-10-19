# REFS — 参考仓库索引与许可边界

> `refs/` 内容永不提交（.gitignore，唯一例外是本文件）；迁移/新机时用
> `scripts/bootstrap-refs.sh` 按 §D 冻结版本一键重建（约 200MB）。
> 克隆均为浅克隆；日常更新用 `git -C refs/<dir> pull --ff-only`，更新后回写 §D。

## A. 应用与家族约定（ArtCraft / storytold）

| 目录 | 许可 | 借鉴什么 | 边界 |
|---|---|---|---|
| `vectorcraft/` | MIT OR Apache-2.0 | **方法论母本**：AGENTS.md 净室规则措辞、workspace/xttask 结构、crate 划分（geom/color/doc/…）、parity 文档与自评分制度、`format/tests/prop_format.rs` 属性测试思路、Image Trace（→ PowerTRACE 对标）、EPS/打印管线 | 只借鉴结构与思路；不搬码（家族许可兼容，可搬，但本项目自立门户，默认独立实现，引用处注明 URL） |
| `designcraft/` | Apache-2.0 | 多页/页面排版模型（InDesign 对标）——CorelDRAW 多页文档语义的最近参照 | 同上 |
| `cadcraft/` | Apache-2.0 | **格式净室先例**：未公开二进制格式（DWG/DXF）的 clean-room 解析策略、fixture 管理 | 同上 |
| `craft-libs/` | Apache-2.0 | craft-pdf / craft-print / craft-docpath / craft-ui 共享库——M2+ 决定直接 git-dep 引用还是仿接口独立实现 | 引用即可（Apache-2.0 兼容） |
| `craft-common-actions/` | 未标（仓内声明） | 共享 GitHub Actions（CI 门） | 引用前核对仓内许可声明 |
| `craft-fonts/` | Apache-2.0 | 字体分仓模式（字体永不进主仓；`CRAFT_FONTS_DIR` 可选构建输入） | 模式照搬；字体文件按其许可 |
| `libcdr/`（GitHub 镜像） | **MPL-2.0** | CDR 格式语义对照（支持版本、chunk 语义差异）、**测试语料清单**（其 test 文件可本地对照） | ⚠️ **只读不抄**：MPL 弱传染；语义结论写进 `docs/reverse/` 日志并注明出处，代码零搬运 |
| `cdr-spec/`（Photopea CDR-specification） | 未标（公开逆向文档） | **格式轨的结构圣经**：RIFF 容器、chunk 表、coord 编码、版本差异 | 事实性格式知识不受版权保护；但文档文本不复制进本仓，只引用小节与结论 |
| `uniconvertor/` | **AGPL-3.0** ⚠️ | CMX/CCX 覆盖面的存在性证明、格式清单 | **高危：AGPL 传染。绝不搬运/链接/改写其代码**；连"参考思路"也仅限其公开文档页（sk1project.net），agents 不得打开其源码目录 |
| `rea/`（cyberkaida/reverse-engineering-assistant） | 见仓内 | Ghidra 逆向 MCP（备选工具链） | 仅用于分析**我们自有**的样例文件；永不指向 Corel 二进制 |
| `morluto-rea/`（REA: Reverse Engineer Anything） | MIT | **主力逆向助手**（用户指定）：跨二进制/应用/运行时行为的 RE MCP（`npx rea-agents`），CDR 逆向战役的效率工具 | 同上——只分析自有样例文件；Corel 安装介质/二进制是净室禁区 |

> 备注：vectorcraft AGENTS.md 引用的 `storytold/craftrules` 已 404（2026-10-19
> 核实）；家族共享规则现散落在 craft-libs 与各 app 的 AGENTS.md 中。

## B. 行为轨公开文档（净室合规的行为依据，引用需带 URL）

| 来源 | 用途 |
|---|---|
| CorelDRAW 官方在线手册（Windows）：`https://product.corel.com/help/CorelDRAW/Documentation-Windows/index.html` | 工具/命令/面板行为的第一依据 |
| CorelDRAW 2022 官方用户指南 PDF（981 页）：`https://learn.corel.com/wp-content/uploads/2022/08/CorelDRAW-UserGuide-Mar22.pdf` | 离线完整行为参考（公开分发物） |
| CorelDRAW 支持与学习中心：`https://www.coreldraw.com/en/support/` · `https://www.coreldraw.com/en/learn/` | 教程/技巧，补手册未覆盖的实操行为 |
| DLP libcdr wiki：`https://wiki.documentfoundation.org/DLP/Libraries/libcdr` | 格式支持范围的第三方对照 |
| Photopea CDR 规范：`https://github.com/photopea/CDR-specification` | 格式结构（同 refs/cdr-spec） |

## C. 逆向工具链使用红线（重申）

1. REA（`morluto-rea`）等工具**只**分析：自有 `.cdr` 样例、合成 fixture、
   我们自己的产物。
2. **永不**分析 Corel 安装目录、CorelDRAW 进程、Corel 分发的任何二进制
   （含试用版）——那是 AGENTS.md 净室铁律第 1 条的绝对禁区。
3. 工具产出的结论进入 `docs/reverse/` 日志时必须附：样例文件 SHA-256、
   工具与版本、发现、置信度。

## D. 冻结版本与重建（迁移/新机引导）

冻结时间：2026-10-19（bootstrap 脚本与本表互为镜像，改动任一处须同步另一处）。

| 目录 | 上游 | 冻结 commit |
|---|---|---|
| `vectorcraft/` | github.com/storytold/vectorcraft | `a466625` |
| `designcraft/` | github.com/storytold/designcraft | `443ea46` |
| `cadcraft/` | github.com/storytold/cadcraft | `0b32c2e` |
| `craft-libs/` | github.com/storytold/craft-libs | `ae2f796` |
| `craft-common-actions/` | github.com/storytold/craft-common-actions | `d7bf8f0` |
| `craft-fonts/` | github.com/storytold/craft-fonts | `d1cf47d` |
| `libcdr/` | github.com/LibreOffice/libcdr | `22c3968` |
| `cdr-spec/` | github.com/photopea/CDR-specification | `f91b3c7` |
| `uniconvertor/` | github.com/sk1project/uniconvertor | `973d5b6` ⚠️ AGPL |
| `rea/` | github.com/cyberkaida/reverse-engineering-assistant | `a05a6b0` |
| `morluto-rea/` | github.com/morluto/rea | `ba866a2` |
