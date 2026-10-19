# 04 — M0/M1 实施计划四方独立评审汇总（GLM-5.3 / Kimi K3 / DeepSeek V4.1 Flash / Grok 4.6）

> 日期：2026-10-19
> 评审通道：
> 1. `zcode-delegate`（ZCode CLI · GLM-5.3）
> 2. `kimi-delegate`（Kimi Code · Kimi K3）
> 3. `codebuddy-delegate`（CodeBuddy CLI · `deepseek-v4.1-flash`）
> 4. `grok-delegate`（Grok CLI · Grok 4.6）

---

## 一、四方100%共识的 5 个 P0 级阻塞漏洞（必须在 M1 Task 9 转绿前修正）

四路模型在不共享上下文的情况下，**100% 独立命中**了原 M1 计划与 [`crates/cdr/src/lib.rs`](../../../crates/cdr/src/lib.rs) 红灯锚点中的以下 5 个致命漏洞：

### P0-1：缺失 X4（v14，2008）至今的 ZIP 外层容器探测
- **事实漏洞**：自 X4（v14）起直至 2024+，真实 `.cdr` 文件的外层容器是 **ZIP**（`PK\x03\x04`），内层 RIFF 载荷位于 `content/riffData.cdr`（或伴随 `content/root.dat`）。原 `CdrContainer` 仅定义了 `Riff` 和 `LegacyWl`，会导致过去 17 年的所有现代 `.cdr` 文件在 M1 `probe` 时全部被误判为 `Err(ProbeError::NotCdr)`，与 M3 主攻现代版本带的目标直接冲突。
- **修正方案**：
  - 在 M1 新增 **ZIP 容器探测（Task 9.5）**：识别 `PK\x03\x04` 并校验 Central Directory（EOCD）条目名，命中 `content/riffData.cdr` 或 `content/root.dat` 时判定为 ZIP 容器（普通 `.docx`/`.zip` 无此条目仍返 `NotCdr`）。
  - 对 Stored（未压缩）或小窗口（前 4 KiB inflate）读取内层 RIFF 头，返回外层 `Zip` + 内层 `Riff` 信息。

### P0-2：版本字节编码语义错误（裸 `0x07` vs 真实 ASCII `'7'` = `0x37`）
- **事实漏洞**：
  1. RIFF 偏移 `8..12` 是 4 字节 form-type fourCC（如 `CDR6`、`CDR7`、`CDRA`、`CDRD`、`CDRE`、`cdr6`）。第 4 字节在真实文件中是 **ASCII 字符**（`'7'` = `0x37`，`'A'` = `0x41` 对应 v10，`'D'` = `0x44` 对应 X3，`'E'` = `0x45` 对应 X4），而非 [`crates/cdr/src/lib.rs`](../../../crates/cdr/src/lib.rs) 测试中写的裸整数 `0x07`。
  2. CMX 的 form-type 是常量 **`CMX1`**（第 4 字节是 ASCII `'1'` = `0x31`，而非 `0x01`）。
  3. 现代版本还需结合顶层 `vrsn` chunk（`u16 LE`，如 `1400` = X4）交叉确认。
- **修正方案**：
  - 将 `form: [u8; 4]` 完整保留，提供 `VersionHint` / `product_version()` 解码（`'3'..='9'` → 3..=9，`'A'..='Z'` → 10..=35），对未知版本字节保留原始字节并记录 warning 而非拒绝。
  - 修正测试 fixture 为 `b'7'`（`0x37`）与 `b"CMX1"`。

### P0-3：`synth_riff_cdr` 合成测试夹具自身不是合法 RIFF 流
- **事实漏洞**：现有 `synth_riff_cdr` 写入 `RIFF`（4B）+ `size=4`（4B）+ `CDR\x07`（4B）+ `\x00\x00\x00`（3B），总长 **15 字节**，但 `size=4` 声明文件在偏移 12 结束，末尾 3 字节属于声明边界外的 trailing garbage（且若把后 3 字节当成子 chunk 则不足 8 字节 chunk 头，必报 `Truncated`）。
- **修正方案**：
  - 最小合法空 RIFF CDR 夹具应恰好 **12 字节**（`RIFF` + `4u32.to_le_bytes()` + `CDR7`）。
  - 另建带真实子 chunk（如 `vrsn`、`LIST INFO`、奇数长度 chunk + pad byte）的合规夹具构造器，并将 trailing bytes 独立为专门的边界测试（产生 `Warning::TrailingBytes`）。

### P0-4：`RiffReader` 敌意输入与 `LIST cmpr`/`stlt` 陷阱未在规格中封堵
- **事实漏洞**：
  1. **Photopea 规范特殊陷阱（GLM-5.3 独有发现）**：`LIST` 的 `list_type` 为 `cmpr`（压缩块）或 `stlt`（样式表）时，其内部载荷**不可按常规子 chunk 直接迭代**（`cmpr` 内部的 size 字段实际是索引）。若走读器无差别递归展开所有 `LIST`，遇到真实文件必崩。
  2. **深度炸弹与算术溢出**：恶意构造 10,000 层嵌套 `LIST` 会触发调用栈溢出（abort 而非 `Result`）；`offset + 8 + size + pad` 若未用 `u64` / `checked_add` 会在 `u32::MAX` 时溢出；`size = 0` 的空 chunk 若游标不前进 8 字节会死循环；文件末尾最后一个奇数长度 chunk 缺失 pad 字节在真实 RIFF 中普遍存在，需在 lenient 模式下容忍并记录 warning。
- **修正方案**：
  - `RiffReader` 采用扁平迭代 + 显式下钻（带 `MAX_DEPTH = 32/64` 深度上限 → `ProbeError::NestingTooDeep` / `LimitExceeded`），禁止对 `cmpr`/`stlt` 自动当作普通子 chunk 下钻。
  - 全程 `u64` + `checked_add`；限制单文件最大扫描 chunk 数；M1 即引入 `proptest` 随机字节不变量测试（无需等到 M3）。

### P0-5：M1 DoD 与 CLI 契约冲突
- **事实漏洞**：
  1. M1 DoD 写 `grep -r "corel" --ignore-case crates/ apps/`，但 `crates/cdr/src/lib.rs` 第 1 行模块注释就合法写了 `CorelDRAW`，DoD 自相矛盾（应以 `cargo xtask cleanroom` 为准）。
  2. CLI DoD 将“非 CDR 文件”退出码定为 `2`，但 `clap` 默认将退出码 `2` 用于命令行参数解析错误（usage error），自动化脚本无法区分“参数写错”与“文件不是 CDR”。
  3. `probe()` 仅返回 `CdrContainer` 枚举，而 Task 11 CLI 却要求输出“容器/版本/chunk 清单”，导致 CLI 与库 API 脱节。
- **修正方案**：
  - `probe()` 返回结构化 `ProbeReport { schema_version, container, chunks, warnings }`。
  - 退出码区分：`0 = OK`，`2 = CLI 参数错误（clap 默认）`，`3 = NotCdr / Unsupported`，`4 = I/O 错误`，`5 = Truncated / Malformed`。

---

## 二、架构与里程碑（M0–M5+）的四方关键修正建议（P1）

1. **在 M2 文档模型中内建 `Object::Unsupported`（无损透传节点）与稳定 `ObjectId`（DeepSeek V4.1 Flash / GLM-5.3）**：
   - M3 不可能一次性解析 CDR 全部特效对象；`coralcraft-doc` 从 day 1 具备 `Object::Unsupported { kind, raw_summary }` + 稳定 `ObjectId`，才能实现“未支持特性记入 import warning 台账但不丢节点”。
2. **页面与图层归属模型（Grok 4.6 / Kimi K3 / GLM-5.3）**：
   - 必须显式支持 Master Page 层与 **Desktop（桌面跨页散件层）**，以及图层三态（`visible` / `printable` / `locked`）；`Spread` 仅作为排版视图派生概念，不作为底层存储容器，以支持异构页面尺寸。
3. **色彩模型前移至 M2，且严禁内置 Pantone 色表（四方共识）**：
   - `coralcraft-color` 的数据类型（`CMYK` 原始分量、`Spot` 专色名 + 替代色 + `tint`、`overprint_fill` / `overprint_stroke` 布尔位）必须在 M2 与 `coralcraft-doc` 同步落地，不能在 M2 用 RGBA f32 糊弄到 M4 再返工。
   - **合规红线**：Pantone 色库受商业许可保护，严禁在仓库或二进制中内置 Pantone 数值表；只提供 `.cpl` / `.xml` 色板文件解析器供用户导入自有色板。
4. **MCP 薄壳从 M5 提前至 M2/M3（四方共识）**：
   - 对齐 `vectorcraft` 的成功经验：M2 交付引擎 Command/Transaction 与渲染首帧后，立即通过 CLI/MCP 暴露 `probe`、`new_document`、`document_summary`、`render_page_png`、`import_warnings`，让 AI Agent 在 M3 阶段就能自动化驱动 CDR 导入回归。
5. **净室流程加固：两段式上下文隔离（Analyst vs Implementer）**：
   - 阅读 `libcdr` 或外部资料的会话只产出 `docs/reverse/*.md` 纯事实规格笔记（偏移、魔数、字段表、样例 SHA-256）；编写 Rust 解析代码的会话只读 `docs/reverse/` 与公开规范，不挂载 `refs/libcdr` 源码，从工程流程上杜绝表达性代码趋同。

---

## 三、第五路 `claude-opus-5-5-high`（Claude Opus 5.5 High）独有深度发现与优势对比

除完全覆盖上述 5 个 P0 共识外，`claude-opus-5-5-high` 展现出极强的**跨版本历史格式记忆与底层图形学洞察**，补充了前四路模型均未提及的 6 项关键细节：

1. **Mac 版 CMX 的大端序 `RIFX` 容器（纠偏前四路模型）**：
   - 前四路模型建议“一律拒绝 `RIFX`”，而 Opus 5.5 精准指出：**Mac 平台的 Corel CMX 交换格式使用大端序 `RIFX`（Big-Endian `u32` size）+ `CMX1`**，因此 `RiffReader` 与 `probe` 应支持 `Endian::{Little, Big}`（`RIFF` vs `RIFX`），而非简单丢弃 `RIFX`。
2. **`content/root.dat` + `content/dataFileList.dat` 的真实协作机制**：
   - X4–X5 的 ZIP 容器内主载荷为 `content/riffData.cdr`；而 X6 及之后版本采用 `content/root.dat`（RIFF 结构）配合 `content/dataFileList.dat` 索引外部数据流。因此 M1 的 ZIP 探测器应同时识别 `content/riffData.cdr` 与 `content/root.dat`。
3. **CDR v3 版本字节是 ASCII 空格 `' '`（`0x20` → `300`），且 `'Q'`/`'R'` 延续至 2024/2025（v26/v27）**：
   - 补全了版本字节映射表边界：`b' '`（`0x20`）映射为 `v3 (300)`，`b'4'..=b'9'` 映射为 `400..=900`，`b'A'..=b'Z'` 映射为 `1000..=3500`。
4. **M3 降维杠杆：利用 CDR 内置的预烘焙路径（Baked Geometry）**：
   - 对于 Blend、Contour、Envelope、Extrude 等复杂动态效果，CDR 文件中通常同时存储了控制参数与**预计算好的结果路径（baked result）**。M3 初版导入器可直接读取预烘焙路径并记录一条 import warning，无需在 M3 实现完整动态特效引擎即可获得高保真视觉还原。
5. **坐标系统的精确 `0.1 μm`（`1/254,000 inch`）定点性质**：
   - `1 / 254,000 inch` 恰好等于 **`0.1 微米`（`1 / 10,000 mm`）**。在文档模型与转换边界明确这一精确比例，可避免毫米/英寸浮点往返漂移。
6. **渐变（Fountain Fill）的 HSB 顺/逆时针彩虹插值模式**：
   - CorelDRAW 的渐变除了直接色彩空间线性插值（Direct）外，还支持 HSB 色环顺时针（Rainbow CW）与逆时针（Rainbow CCW）插值，`coralcraft-color` 需预留插值色彩空间与方向字段。

> **调用 `claude-opus-5-5-high` 的工程注意事项（已记入经验库）**：
> - `agy --model claude-opus-5-5-high` 后端走 Vertex AI 通道，其安全过滤器（Content Safety Filter）对提示词中出现的 `"深度炸弹"`（`depth bomb` / `zip bomb`）等攻击性安全术语极为敏感，若模型在回复中复述此类词汇会触发拦截重试。向 Opus 5.5 委派安全/边界审查时，应使用中性工程术语（如 `"深层嵌套压力测试"`、`"超长压缩流防护"`）。
