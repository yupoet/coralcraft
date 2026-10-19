# CoralCraft

**CorelDRAW 级矢量与排版能力的开源、净室（clean-room）重实现——纯 Rust。**

CoralCraft 是一个目标为 CorelDRAW 功能对等（parity）及超越的开源矢量插画 +
多页排版 + 印刷生产应用。名字来自 **Coral**（Corel 的谐音，也致敬珊瑚礁生态）+
**Craft**（[ArtCraft Crafting Apps](https://getartcraft.com/apps) 家族命名约定）。

> ⚠️ CoralCraft 与 Corel 公司没有任何关联。CorelDRAW 是 Corel 公司的注册商标。
> CoralCraft 不包含、不分发任何 Corel 代码、资产或文档。

## 现状：pre-alpha（脚手架阶段）

- 2026-10-19 建仓。当前只有工作区骨架、净室规则、参考仓库索引和 M0/M1 计划。
- 路线图见 [`ROADMAP.md`](ROADMAP.md)，差距清单见 [`docs/gaps.md`](docs/gaps.md)。

## 方法论（一句话版）

**行为来自公开文档，格式来自逆向的自有文件，两轨净室并行。**

- 应用行为（工具、面板、命令、快捷键）：只参考 Corel 公开手册、公开教程与
  论坛帖子（逐条引用 URL）。**从不运行、观察、反汇编 CorelDRAW 本体。**
- CDR/CMX 文件格式：以社区已发表的逆向成果为起点
  （[Photopea CDR 规范](https://github.com/photopea/CDR-specification)、
  [libcdr](https://github.com/LibreOffice/libcdr) 的公开语义、自有样例文件），
  独立实现，不抄袭任何代码。
- 详见 [`AGENTS.md`](AGENTS.md)（绝对规则）与
  [`docs/superpowers/specs/2026-10-19-coralcraft-brainstorm.md`](docs/superpowers/specs/2026-10-19-coralcraft-brainstorm.md)。

## 工程约定（对齐 ArtCraft / vectorcraft）

- 纯 Rust，workspace 布局：`crates/*`（引擎库）+ `apps/*`（桌面/CLI/Web）+ `xtask`。
- `MIT OR Apache-2.0` 双许可；不引入 GPL/AGPL 代码（含 Inkscape、UniConvertor、lib2geom）。
- 资产政策：所有图标/字体/样本必须原创或带兼容许可，`ASSETS.md` 全量登记。
- Agent-native：MCP server + JSON 控制通道，让 AI agent 像人一样画图（M2+）。
- 严格 TDD：失败测试先行；`cargo xtask ci` = fmt + clippy + test + cleanroom 检查。

## 快速开始

```sh
git clone https://github.com/yupoet/coralcraft
cd coralcraft
scripts/bootstrap-refs.sh      # 重建 refs/ 参考仓（冻结版本，约 200MB，见 refs/REFS.md §D）
rustup component add rustfmt clippy
cargo test --workspace --exclude coralcraft-cdr   # 其余全绿（cdr 为 M1 TDD 红灯锚点）
cargo run -p coralcraft-cli -- probe <file.cdr>   # M1 交付后可用
```

## 参考项目

`refs/` 目录（git-ignored，永不提交）持有全部可借鉴的第三方仓库与用途说明，
见 [`refs/REFS.md`](refs/REFS.md)。

## 许可

代码与原创资产：`MIT OR Apache-2.0`（见 `LICENSE-MIT`、`LICENSE-APACHE`、`NOTICE`）。
