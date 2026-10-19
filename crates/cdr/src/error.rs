//! `coralcraft-cdr` 的错误与告警类型。
//!
//! 所有输入均按敌意（不可信）字节流处理：解析失败一律返回结构化 [`ProbeError`]，
//! 可容忍的规范偏差（如尾随垃圾字节、文件末尾缺对齐字节、小写四字符码）记入 [`ProbeWarning`]。

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// `probe` 与底层容器走读器的错误类型。全部可恢复、可序列化展示给用户或 CLI，禁止 panic。
#[derive(Debug, Clone, Error, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ProbeError {
    /// 输入太短，连基本魔数或容器头都不足。
    #[error("输入字节过短（需要至少 {needed} 字节，实际 {got} 字节）")]
    TooShort { needed: usize, got: usize },

    /// 不是 CDR/CMX 文件（魔数不匹配或 ZIP 内不含 CDR 条目）。
    #[error("不是 CDR/CMX 文件：{reason}")]
    NotCdr { reason: String },

    /// 容器或 chunk 声明的长度超出实际可用字节边界（文件截断）。
    #[error("文件在偏移 {offset} 处截断（需要 {needed} 字节，剩余 {available} 字节）")]
    Truncated { offset: u64, needed: u64, available: u64 },

    /// 容器结构非法（如 LIST 载荷小于 4 字节、ZIP 目录损坏、加密条目等）。
    #[error("偏移 {offset} 处结构非法：{reason}")]
    Invalid { offset: u64, reason: String },

    /// 超出敌意输入防护上限（如 LIST 嵌套深度超过上限或 chunk 总数过多）。
    #[error("超出安全解析上限（{limit} 上限为 {max}）")]
    LimitExceeded { limit: String, max: usize },
}

/// 探测过程中发现的非致命规范偏差或降级提示。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ProbeWarning {
    /// 文件实际长度大于 RIFF 头部声明的 `8 + declared_size`（存在尾随字节）。
    TrailingBytes { declared_end: u64, actual_len: u64 },

    /// 文件末尾最后一个奇数长度 chunk 省略了 1 字节对齐 padding（RIFF 实践常见）。
    MissingTrailingPad { offset: u64 },

    /// 奇数长度 chunk 的对齐字节非零。
    NonZeroPadByte { offset: u64, byte: u8 },

    /// Form-type 使用了小写前缀（如 `cdr6` 而非标准大写 `CDR6`）。
    NonCanonicalForm { form: String },

    /// Form-type 第 4 字节不在已知的 ASCII 版本映射表内。
    UnknownVersionChar { byte: u8 },

    /// 顶层 `vrsn` chunk 数值版本与 Form-type 字符推导的版本不一致。
    VersionMismatch { form_version: u16, vrsn_chunk: u16 },

    /// 遇到 `cmpr` 或 `stlt` 等非标准子 chunk 布局的特殊 `LIST`，已按不透明载荷整体跳过。
    OpaqueListSkipped { list_type: String, offset: u64, size: u32 },

    /// ZIP 容器内层主条目处于压缩态（如 Deflate），M1 probe 阶段未解压内层 RIFF 头。
    ZipInnerCompressed { entry: String, compression_method: u16 },
}
