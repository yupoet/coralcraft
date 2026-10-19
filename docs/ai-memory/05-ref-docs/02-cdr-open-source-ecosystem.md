# 02 — CDR/CMX 开源生态调研与许可边界

> 调研日期：2026-10-19

## 1. 现状总结：开源生态「只读、无写」

目前开源界存在多个读取/解析 `.cdr` 的项目，但**没有任何开源项目支持写出（保存回）`.cdr` 文件**。

## 2. 格式层三大核心参考源

| 项目 | 仓库 / 地址 | 编程语言 | 许可证 | 支持范围 | CoralCraft 使用边界 |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Photopea CDR-specification** | `github.com/photopea/CDR-specification` (`refs/cdr-spec/`) | 文档 | 公开逆向规范 | WL 老格式 + 1992 后 RIFF 格式、基础数据类型、chunk 表 | **格式轨结构圣经**：事实性格式结构不受版权保护，直接作为独立实现依据（不复制原文措辞） |
| **libcdr** | `github.com/LibreOffice/libcdr` (`refs/libcdr/`) | C++ | **MPL-2.0** | 官方自述支持全版本 `.cdr` 导入（源码树 `src/lib/` 仅含 `CDR*` 文件，无 `CMXParser`） | **只读不抄**：仅用于核对版本差异与 chunk 语义，结论记入 `docs/reverse/` 并注明出处，零代码搬运 |
| **UniConvertor 2.0** | `github.com/sk1project/uniconvertor` (`refs/uniconvertor/`) | Python + C | **AGPL-3.0** ⚠️ | 支持 CDR、CMX、CCX 及 CPL/XML 调色板导入 | **高危禁区**：AGPL 强传染，**严禁 Agent 打开或阅读 `refs/uniconvertor/` 源码**，仅参考其官网公开格式列表 |

## 3. 应用层与周边工具现状

- **LibreOffice Draw**：基于 `libcdr` 导入 `.cdr`，可另存为 SVG/PDF/ODF，是目前最稳定的免费读取通道。
- **Inkscape**：通过 `UniConvertor` 滤镜导入 `.cdr`。
- **sK1 2.0 (`sk1project/sk1-wx`)**：GPL-3.0 印刷向矢量编辑器，底层同样使用 UC2 引擎（GPL 禁区，不读源码）。
- **逆向工程助手（REA）**：
  - `morluto/rea`（`refs/morluto-rea/`，MIT）：主力逆向工程助手 MCP，**仅允许用于分析我们自有的 `.cdr` 样例文件或合成 fixture**，严禁指向任何商业软件二进制。
  - `cyberkaida/reverse-engineering-assistant`（`refs/rea/`）：备选逆向 MCP。

## 4. 搜索降噪提示

在 GitHub 搜索 `"cdr"` 时：
- 绝大多数高 star 仓库是电信话单 **Call Detail Record (CDR)** 或光盘刻录工具（`cdrtools` / `cdrdao`），与矢量图形无关；
- 搜索 `"coreldraw"` 时混有大量盗版安装器与 VBA 宏集合仓库，检索格式资料时需结合 `libcdr`、`riff`、`cmx` 等关键词过滤。
