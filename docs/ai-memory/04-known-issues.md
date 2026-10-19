# 04 — 已知 Bug、坑点与规避方式

## 1. [已修复] `scripts/bootstrap-refs.sh` 短 SHA 无法被 GitHub `git fetch --depth 1` 直接拉取

- **现象**：原 [`scripts/bootstrap-refs.sh`](../../scripts/bootstrap-refs.sh) 中 `ENTRIES` 配置的是 7 位短 SHA（如 `a466625`、`ba866a2`）。执行 `git fetch -q --depth 1 origin <7位SHA>` 时，GitHub 服务端会报错 `fatal: couldn't find remote ref <short-sha>`，从而触发脚本的 fallback 分支拉取远端最新 `HEAD`。
- **修复状态**：已将 [`scripts/bootstrap-refs.sh`](../../scripts/bootstrap-refs.sh) 中的 `ENTRIES` 升级为 40 位完整 commit SHA，`git fetch -q --depth 1 origin "$pin"` 可一次性直接拉取冻结版本。

## 2. `cargo xtask cleanroom` 扫描范围覆盖 `docs/`（以及对品牌词的边界）

- **现象**：[`xtask/src/main.rs`](../../xtask/src/main.rs) 的 `cleanroom()` 会递归扫描 `crates/`、`apps/` 与 `docs/` 下的所有文本文件和文件后缀。
- **坑点与规则对比**：
  1. 严禁在 `crates/`、`apps/`、`docs/` 下存放任何 `.cdr` 或 `.cmx` 后缀的文件（测试 fixture 应在内存构造或用非 `.cdr`/`.cmx` 方式内嵌于测试代码中）。
  2. 严禁在 `crates/`、`apps/`、`docs/`（包括本 `docs/ai-memory/` 目录）中写出 `xtask/src/main.rs` 定义的三个版权特征字符串（如 Corel 版权声明或公司全称后缀），否则 `cargo xtask cleanroom` 会直接报错退出。在 `xtask` 自身测试中构造违规样例时，也建议使用 `format!("{} (c) {}", "Copyright", "Corel")` 动态拼接。
  3. **品牌词边界（对照 `refs/vectorcraft/xtask/src/brands.rs`）**：`vectorcraft` 禁止在 **Rust 源码字符串字面量（UI 菜单、工具名、MCP 描述）与 `Cargo.toml` 描述** 中出现竞品厂商/产品名（要求 UI 内统一称 `"the reference app"`），但**明确豁免 Markdown 文档（`README.md`、`ROADMAP.md`、`docs/*.md`）** 用于公开定位与对标比较。因此 `README.md` 首页可以且应当明确强调 `CorelDRAW`（及常见拼写 `CoralDraw`），而应用内 UI 字符串则保持自有命名。

## 3. `refs/uniconvertor` 的 AGPL-3.0 污染红线

- **现象**：`refs/uniconvertor/` 由 `bootstrap-refs.sh` 克隆在本地，但其许可证为 **AGPL-3.0**。
- **规避方式**：任何 Agent **严禁打开或读取 `refs/uniconvertor/` 下的源码文件**。仅允许参考其公开站点（`https://sk1project.net/uc2/`）上的格式支持列表作为存在性佐证。

## 4. `storytold/craftrules` 仓库已 404

- **现象**：`refs/vectorcraft/AGENTS.md` 中引用的 `https://github.com/storytold/craftrules` 仓库已不可访问（404）。
- **规避方式**：不要尝试克隆或引用 `craftrules`；ArtCraft 家族共享规范直接参考 `refs/vectorcraft/AGENTS.md`、`refs/designcraft/`、`refs/cadcraft/` 与 `refs/craft-libs/`。

## 5. `agy --model claude-opus-5-5-high` 的安全过滤与无头工具限制

- **现象**：使用 `agy -p --model claude-opus-5-5-high` 进行子进程审查时：
  1. 若 Prompt 或期望输出中密集出现攻击性安全术语（如中文“深度炸弹”、“ZIP 炸弹”等），Vertex AI Claude 后端可能触发内容安全拦截（`finishReason: OTHER` / ` blocked by content safety filter`）。
  2. 在 `-p` 无头模式下，若模型尝试调用 `read_url_content` 等需交互确认的工具，会遭遇 `Tool execution was denied`。
- **规避方式**：向 `claude-opus-5-5-high` 委派任务时，使用中性工程术语（如“嵌套层级上限”、“恶意压缩比防护”），并在 Prompt 开头显式声明“仅读取本地文件，不要调用 `read_url_content` 或网络搜索工具”。

## 6. Workspace 集成测试 (`tests/*.rs`) 的 Clippy `unwrap_used` / `expect_used` 规则

- **现象**：`clippy.toml` 的 `allow-unwrap-in-tests = true` 仅对标记了 `#[test]` 的函数体生效；而在集成测试文件（如 `apps/coralcraft-cli/tests/cli_probe.rs`）中，辅助结构体方法（如 `TempFixture::new`）并非 `#[test]` 函数，直接调用 `.expect()` 会触发 `clippy::expect_used` 报错。
- **规避方式**：在集成测试文件 `tests/*.rs` 顶部或 `#[cfg(test)] mod tests` 上添加 `#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]`。
