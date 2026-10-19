# 00 — 项目目标、模块与目录说明

## 1. 项目定位与目标

**CoralCraft** 是一个开源、净室（clean-room）、纯 Rust 原生的矢量插画 + 多页排版 + 印刷生产应用，目标是实现对标 CorelDRAW 的功能等价（parity）及超越。

- **命名**：**Coral**（致敬珊瑚礁生态）+ **Craft**（对齐 [ArtCraft Crafting Apps](https://getartcraft.com/apps) 家族命名约定）。
- **核心价值（G1–G4）**：
  - **G1 应用等价**：插画、多页文档、印刷输出（标识、雕刻、广告制作、出版）达到存量用户可迁移的 parity；自有原生格式 `.coralcraft`（JSON）。
  - **G2 CDR 读**：打开/置入 `.cdr`（1992 后 RIFF 容器全版本 + 尽可能多的老 `WL` 格式），转换损失提供明确 import warning 台账。
  - **G3 CMX 读**：支持 `.cmx` 交换格式。
  - **G4 CDR 写（长期战役，M5+）**：写出可兼容打开的 `.cdr` 子集（开源界空白）。
- **非目标（NG1–NG4）**：
  - 不复刻位图编辑器全家桶（家族已有 PhotoCraft）；
  - 不内置微软 VBA，宏兼容后续（M4+）走自有 Basic 方言（参照 LibreOffice StarBasic 先例）；
  - 不做云服务，坚持 Local-first；
  - 不追求像素级 UI 复刻，重在熟悉的工作流与快捷键/工具范式。
- **许可**：`MIT OR Apache-2.0` 双许可。

## 2. 目录结构说明

```text
/data/coralcraft
├── AGENTS.md                  # Agent 最高铁律（净室规则、许可红线、资产规范、工作流）
├── CLAUDE.md                  # AI 记忆库入口说明与 AGENTS.md 索引
├── ROADMAP.md                 # 阶段基线、各维度自评分（0–100%）与里程碑路线
├── ASSETS.md                  # 原创/开源资产全量登记表（由 cargo xtask assets 校验）
├── Cargo.toml                 # Rust workspace 根清单（edition 2024, rust-version 1.95）
├── crates/                    # 核心引擎与功能库（纯 Rust，与宿主解耦）
│   ├── geom/                  # 几何内核（贝塞尔、布尔运算、捕捉）
│   ├── color/                 # 色彩系统（CMYK、专色、色板 .cpl/.xml）
│   ├── doc/                   # 多页文档模型（Page/Spread/Layer/Object，不同于单画板模型）
│   ├── cdr/                   # ★ CDR/CMX 格式层（probe → parse → build 三阶段，敌意输入防御）
│   ├── render/                # SIMD 软光栅/GPU 渲染管线
│   ├── tools/                 # 工具与面板交互语义
│   ├── engine/                # 引擎组装与命令调度
│   ├── ui-egui/               # egui 桌面/Web UI 层（可替换）
│   └── mcp/                   # MCP server 与 JSON 控制通道（Agent-native 驱动）
├── apps/                      # 可执行入口
│   ├── coralcraft/            # 桌面应用主程序
│   └── coralcraft-cli/        # CLI 与 headless 工具（如 probe 子命令）
├── xtask/                     # 工程质量门（ci / cleanroom / assets）
├── scripts/
│   └── bootstrap-refs.sh      # 一键按冻结 commit 重建 refs/ 参考仓
├── refs/                      # 第三方参考仓库（git-ignored，仅 REFS.md 入库）
│   └── REFS.md                # 参考仓清单、借鉴边界、许可证红线与冻结 commit 表
└── docs/
    ├── ai-memory/             # AI 持久记忆库（00–04 核心文档 + 05-ref-docs 知识库）
    ├── gaps.md                # 按优先级排序的差距清单（Agent 领活入口）
    ├── reverse/               # CDR 格式逆向工作日志（M1 Task 13 建立规范）
    └── superpowers/           # 规格与实施计划
        ├── specs/             # 头脑风暴与架构设计文档
        └── plans/             # 里程碑 TDD 实施计划（当前：M0/M1）
```
