# 03 — CDR 文件格式结构初探（Photopea 规范梳理）

> 依据：`refs/cdr-spec/README.md`（Photopea 公开逆向规范 `https://github.com/photopea/CDR-specification`）

## 1. 两大容器世代

1. **老格式（Old Format，1992 年前）**：
   - 文件起始魔数为 2 字节 ASCII：`"WL"`（`0x57 0x4C`）。
   - `coralcraft-cdr` 在 probe 阶段将其识别为 `CdrContainer::LegacyWl`。
2. **RIFF 格式（1992 年后至今主流格式）**：
   - 遵循标准 Little-Endian RIFF 容器结构，文件带有一个关联版本号 $V$。

## 2. RIFF 容器头部结构（前 12 字节）

| 偏移 (Bytes) | 长度 | 类型 | 含义 |
| :--- | :--- | :--- | :--- |
| `0..4` | 4 | `asci4` | 固定魔数 `"RIFF"` |
| `4..8` | 4 | `u32 LE` | 后续数据总字节数（通常等于 `file_size - 8`） |
| `8..11` | 3 | `asci3` | 文档类型标识：`"CDR"` 或 `"CMX"` |
| `11` | 1 | `u8` | 格式版本字节（`version_byte`，例如 `'7'` / `0x07` 等，具体映射随版本演进） |
| `12..` | 变长 | `CHUNKS` | 顶层 chunk 序列 |

## 3. Chunk 与 LIST 结构规则

每个 chunk 由 8 字节头部 + 内容组成：

| 字段 | 长度 | 类型 | 含义 |
| :--- | :--- | :--- | :--- |
| `fourcc` | 4 | `asci4` | Chunk 四字符标识符（如 `"LIST"`、`"vrsn"`、`"doc "` 等） |
| `size` | 4 | `u32 LE` | Chunk 内容字节数 $N$（不含 8 字节头与末尾对齐字节） |
| `data` | $N$ | `bytes` | Chunk 载荷数据 |
| `padding` | $0 \text{ or } 1$ | `u8` | **对齐规则**：当 $N$ 为奇数（`N % 2 == 1`）时，后面紧跟 1 字节 padding，下一个 chunk 从偶数字节偏移开始 |

- **`LIST` Chunk 特例**：
  - 当 `fourcc == "LIST"` 时，$N$ 字节内容的前 4 字节为 `list_type`（`asci4`），后续 $N - 4$ 字节通常为嵌套的子 chunk 序列（注意：规范指出少数特殊 `LIST` 的载荷不可按常规子 chunk 解释，解析器需具备按 `size` 安全跳过的能力）。

## 4. 规范中的基本数据类型约定

- `uint2` / `sint2`：16 位无符号/有符号整数（Little-Endian）
- `uint4` / `sint4`：32 位无符号/有符号整数（Little-Endian）
- `doubl`：64 位 IEEE 754 浮点数（Little-Endian）
- `coord`（坐标值）：
  - 当版本 $V < 600$ 时：`sint2 / 1000.0`
  - 当版本 $V \ge 600$ 时：`sint4 / 254000.0`
- `string`：以 `0x00` 结尾的非零 ASCII 字节序列
