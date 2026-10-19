# 04 — 已知 Bug、坑点与规避方式

## 1. `scripts/bootstrap-refs.sh` 短 SHA 无法被 GitHub `git fetch --depth 1` 直接拉取

- **现象**：[`scripts/bootstrap-refs.sh`](../../scripts/bootstrap-refs.sh) 中 `ENTRIES` 配置的是 7 位短 SHA（如 `a466625`、`ba866a2`）。执行 `git fetch -q --depth 1 origin <7位SHA>` 时，GitHub 服务端会报错 `fatal: couldn't find remote ref <short-sha>`，从而触发脚本的 fallback 分支 `git clone --depth 1 --single-branch` 拉取远端最新 `HEAD`。當上游仓库有新提交时（例如 `morluto-rea` 远端已推进到 `3927542`），会导致本地检出版本偏离 [`refs/REFS.md`](../../refs/REFS.md) §D 的冻结版本。
- **规避/修复方式**：
  - Git 传输协议仅支持对完整 **40 位 commit SHA** 执行按哈希浅拉取（`git fetch --depth 1 origin <40-char-sha>`）。
  - 当前环境已手动通过 `git -C refs/morluto-rea fetch --depth 20 origin && git -C refs/morluto-rea checkout --detach ba866a2` 还原到精确冻结版本。
  - 若需修改 `scripts/bootstrap-refs.sh`，应将 `ENTRIES` 中的 `pin` 字段替换为以下已核实的 40 位完整 SHA：
    - `vectorcraft`: `a466625c8987550635cda951ebf36e08e1e80a56`
    - `designcraft`: `443ea4640ca368fcf0241b04813bd3817acd5df8`
    - `cadcraft`: `0b32c2e6a14c761835ea62c01e7b20845971d781`
    - `craft-libs`: `ae2f796b673f29cf3161f36eb6a8933f0a3fe028`
    - `craft-common-actions`: `d7bf8f06e138618890d0ebe8a84a18b010bf3f66`
    - `craft-fonts`: `d1cf47d05e04426615fbda1d637b9ca6b1e7af35`
    - `libcdr`: `22c396810e72d40b94f78c192eb06e6d09bc34a0`
    - `cdr-spec`: `f91b3c7e8c812b28473f6399e95a624a72a3d6b8`
    - `uniconvertor`: `973d5b6f8fccce7e7bf9bc88e91bc80f9f9d9472`
    - `rea`: `a05a6b0a18649efd6d74ec8447bdadf8d468c365`
    - `morluto-rea`: `ba866a26ecbe179a717e746cbc0d1605ae616ad5`

## 2. `cargo xtask cleanroom` 扫描范围覆盖 `docs/`

- **现象**：[`xtask/src/main.rs`](../../xtask/src/main.rs) 的 `cleanroom()` 会递归扫描 `crates/`、`apps/` 与 `docs/` 下的所有文本文件和文件后缀。
- **坑点**：
  1. 严禁在 `crates/`、`apps/`、`docs/` 下存放任何 `.cdr` 或 `.cmx` 后缀的文件（哪怕是数字节的合成测试文件也不行，测试 fixture 应在内存构造或用非 `.cdr`/`.cmx` 方式内嵌于测试代码中）。
  2. 严禁在 `docs/`（包括本 `docs/ai-memory/` 目录）中写出 `xtask/src/main.rs` 第 85 行定义的三个版权特征字符串，否则 `cargo run -p xtask -- cleanroom` 会直接报错退出。

## 3. `refs/uniconvertor` 的 AGPL-3.0 污染红线

- **现象**：`refs/uniconvertor/` 由 `bootstrap-refs.sh` 克隆在本地，但其许可证为 **AGPL-3.0**。
- **规避方式**：任何 Agent **严禁打开或读取 `refs/uniconvertor/` 下的源码文件**。仅允许参考其公开站点（`https://sk1project.net/uc2/`）上的格式支持列表作为存在性佐证。

## 4. `storytold/craftrules` 仓库已 404

- **现象**：`refs/vectorcraft/AGENTS.md` 中引用的 `https://github.com/storytold/craftrules` 仓库已不可访问（404）。
- **规避方式**：不要尝试克隆或引用 `craftrules`；ArtCraft 家族共享规范直接参考 `refs/vectorcraft/AGENTS.md`、`refs/designcraft/`、`refs/cadcraft/` 与 `refs/craft-libs/`。
