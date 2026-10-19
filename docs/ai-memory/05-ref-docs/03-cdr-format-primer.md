# 03 — CDR / CMX 文件格式结构梳理

> 依据：Photopea 公开格式规范（`https://github.com/photopea/CDR-specification`）、Document Liberation Project / `libcdr` 公开文档与五方评审核验结论。

## 1. 三大容器世代

1. **老格式（Legacy `WL`，1992 年前 v1–v2）**：
   - 文件起始魔数为 2 字节 ASCII：`"WL"`（`0x57 0x4C`）。
   - `coralcraft-cdr` 在 probe 阶段将其识别为 `CdrContainer::LegacyWl`。
2. **裸 RIFF / RIFX 容器（1992–2006，v3–X3 及 CMX 交换格式）**：
   - 遵循标准 Little-Endian `RIFF` 容器（Mac 版 CMX 可为 Big-Endian `RIFX`）。
3. **现代 ZIP + RIFF 容器（2008 X4 / v14 至今 2024+）**：
   - 外层为标准 **ZIP 归档**（`PK\x03\x04`，尾部含 EOCD `PK\x05\x06` 与 Central Directory）。
   - X4–X5 主 RIFF 载荷位于 `content/riffData.cdr`；X6 及以后版本主 RIFF 载荷常位于 `content/root.dat`（配合 `content/dataFileList.dat` 索引外部数据流）。

## 2. RIFF / RIFX 容器头部结构（前 12 字节）

| 偏移 (Bytes) | 长度 | 类型 | 含义 |
| :--- | :--- | :--- | :--- |
| `0..4` | 4 | `[u8; 4]` | 容器魔数 `"RIFF"`（小端）或 `"RIFX"`（大端，见于 Mac CMX） |
| `4..8` | 4 | `u32` | 后续数据总字节数（标准情况下等于 `file_size - 8`） |
| `8..12` | 4 | `[u8; 4]` | Form-type 四字符码（如 `"CDR7"`、`"CDRD"`、`"CDRE"`、小写 `"cdr6"` 或 `"CMX1"`） |
| `12..` | 变长 | `CHUNKS` | 顶层 chunk 序列（首个 chunk 通常为 `"vrsn"`，载荷为 `u16` 版本号如 `700`/`1400`） |

### Form-type 第 4 字节（偏移 11）ASCII 版本编码规则

- **CDR 系列（前 3 字节为 `CDR` 或 `cdr`）**：
  - `' '`（`0x20`）或 `'3'`（`0x33`）→ 版本 3（`300`）
  - `'4'`..=`'9'`（`0x34`..=`0x39`）→ 版本 4..=9（`400`..=`900`）
  - `'A'`..=`'Z'`（`0x41`..=`0x5A`）→ 版本 10..=35（如 `'A'`=`1000`, `'D'`=`1300` [X3], `'E'`=`1400` [X4], `'F'`=`1500` [X5], `'G'`=`1600` [X6], `'H'`=`1700` [X7]）
  - 若顶层存在 `vrsn` chunk（2 字节整数），以 `vrsn` 数值版本为权威交叉校验。
- **CMX 系列**：
  - Form-type 固定为 `"CMX1"`（`0x43 0x4D 0x58 0x31`）。

## 3. Chunk 与 LIST 结构规则

每个 chunk 由 8 字节头部 + 内容组成：

| 字段 | 长度 | 类型 | 含义 |
| :--- | :--- | :--- | :--- |
| `fourcc` | 4 | `[u8; 4]` | Chunk 四字符标识符（如 `"LIST"`、`"vrsn"`、`"doc "`、`"page"` 等） |
| `size` | 4 | `u32` | Chunk 内容字节数 $N$（不含 8 字节头与末尾对齐字节） |
| `data` | $N$ | `bytes` | Chunk 载荷数据 |
| `padding` | $0 \text{ or } 1$ | `u8` | **对齐规则**：当 $N$ 为奇数（`N % 2 == 1`）时，后面紧跟 1 字节 padding，下一个 chunk 从偶数字节偏移开始（文件末尾最后一个 chunk 缺失 pad 字节在真实文件中需容忍并记 warning） |

- **`LIST` Chunk 规则与 `cmpr`/`stlt` 特例**：
  - 当 `fourcc == "LIST"` 时，要求 $N \ge 4$，前 4 字节为 `list_type`（`[u8; 4]`）。
  - **关键特例**：当 `list_type` 为 `"cmpr"`（压缩列表）或 `"stlt"`（样式表）时，其后续 $N - 4$ 字节**不是标准子 chunk 序列**（例如 `"cmpr"` 内部 chunk 的 size 字段实际为索引），走读器不得将其当作普通子 chunk 下钻展开。

## 4. 坐标与基本数据类型约定

- `uint2` / `sint2`：16 位无符号/有符号整数
- `uint4` / `sint4`：32 位无符号/有符号整数
- `doubl`：64 位 IEEE 754 浮点数
- `coord`（坐标值）：
  - 当版本 $V < 600$ 时：`sint2 / 1000.0` 英寸
  - 当版本 $V \ge 600$ 时：`sint4 / 254000.0` 英寸（即精确的 **$0.1\ \mu\text{m}$** = $1 / 10000\ \text{mm}$ 定点单位）
