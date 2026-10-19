//! RIFF / RIFX 容器与 chunk 走读器（`coralcraft-cdr::riff`）。
//!
//! 依据 Photopea CDR 规范 §RIFF Format 与通用 RIFF 规则：
//! - 头部 12 字节：`RIFF`（小端）或 `RIFX`（大端）+ `u32` size + 4 字节 `form`。
//! - 每个 chunk 为 8 字节头（4 字节 `fourcc` + `u32` size）+ `size` 字节载荷 + 奇数长度 1 字节对齐 pad。
//! - `LIST` chunk 的前 4 字节为 `list_type`；当 `list_type` 为 `"cmpr"` 或 `"stlt"` 时，
//!   内部不是标准子 chunk 序列（Photopea 规范特例），走读器将其标为 `opaque_payload = true` 并整体跳过。
//! - 敌意输入防护：显式栈迭代（禁止递归栈溢出），深度上限 `MAX_DEPTH = 32`，
//!   总 chunk 数上限 `MAX_CHUNKS = 100_000`，全程 `u64` + `checked_add`。

use serde::{Deserialize, Serialize};

use crate::error::{ProbeError, ProbeWarning};

/// 最大允许的 `LIST` 嵌套深度（超出返回 `ProbeError::LimitExceeded`）。
pub const MAX_DEPTH: usize = 32;

/// 单文件最大允许扫描的 chunk 总数（防止百万级 0 长度 chunk DoS）。
pub const MAX_CHUNKS: usize = 100_000;

/// RIFF 容器字节序。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Endian {
    /// 标准 `RIFF`（Little-Endian）。
    Little,
    /// `RIFX`（Big-Endian，见于 Mac 平台 CMX 文件）。
    Big,
}

/// 单个 RIFF chunk 的结构化摘要。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkSummary {
    /// 可读的四字符码（非可见 ASCII 时回退为 `0xhhhhhhhh` 十六进制表示）。
    pub fourcc: String,
    /// 原始 4 字节标识符。
    pub raw_fourcc: [u8; 4],
    /// Chunk 头部在输入切片中的起始偏移（字节）。
    pub offset: u64,
    /// Chunk 载荷声明长度（不含 8 字节头与末尾 pad）。
    pub size: u32,
    /// 嵌套深度（顶层 chunk 为 0，`LIST` 内部子 chunk 为 `parent.depth + 1`）。
    pub depth: usize,
    /// 当 `raw_fourcc == *b"LIST"` 时，记录其 4 字节 `list_type`。
    pub list_type: Option<String>,
    /// 是否为不可按常规子 chunk 拆解的特殊 `LIST`（如 `cmpr` / `stlt`）。
    pub opaque_payload: bool,
}

/// RIFF / RIFX 容器走读结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiffInspection {
    /// 容器字节序（`RIFF` = `Little`，`RIFX` = `Big`）。
    pub endian: Endian,
    /// 头部偏移 `4..8` 声明的后续字节数。
    pub declared_size: u32,
    /// 头部偏移 `8..12` 的原始 4 字节 form-type（如 `*b"CDR7"`、`*b"CMX1"`）。
    pub form: [u8; 4],
    /// 若顶层存在 `"vrsn"` chunk（`size >= 2`），提取出的 16 位数值版本（如 `700`、`1400`）。
    pub vrsn_chunk_value: Option<u16>,
    /// 按先序遍历收集的 chunk 清单。
    pub chunks: Vec<ChunkSummary>,
    /// 走读过程中记录的非致命告警。
    pub warnings: Vec<ProbeWarning>,
}

/// 将 4 字节四字符码转为安全可读字符串（若全为可打印 ASCII `0x20..=0x7E` 则转字符，否则格式化为十六进制）。
pub fn format_fourcc(raw: [u8; 4]) -> String {
    if raw.iter().all(|&b| (0x20..=0x7E).contains(&b)) {
        String::from_utf8_lossy(&raw).into_owned()
    } else {
        format!("0x{:02X}{:02X}{:02X}{:02X}", raw[0], raw[1], raw[2], raw[3])
    }
}

fn read_u16(endian: Endian, b: [u8; 2]) -> u16 {
    match endian {
        Endian::Little => u16::from_le_bytes(b),
        Endian::Big => u16::from_be_bytes(b),
    }
}

fn read_u32(endian: Endian, b: [u8; 4]) -> u32 {
    match endian {
        Endian::Little => u32::from_le_bytes(b),
        Endian::Big => u32::from_be_bytes(b),
    }
}

fn slice4(bytes: &[u8], offset: usize) -> Option<[u8; 4]> {
    let s = bytes.get(offset..offset.checked_add(4)?)?;
    <[u8; 4]>::try_from(s).ok()
}

fn slice2(bytes: &[u8], offset: usize) -> Option<[u8; 2]> {
    let s = bytes.get(offset..offset.checked_add(2)?)?;
    <[u8; 2]>::try_from(s).ok()
}

struct Frame {
    cursor: usize,
    end: usize,
    depth: usize,
}

/// 走读并校验 RIFF / RIFX 容器字节流。
pub fn inspect_riff(bytes: &[u8]) -> Result<RiffInspection, ProbeError> {
    if bytes.len() < 4 {
        return Err(ProbeError::TooShort { needed: 4, got: bytes.len() });
    }
    let magic = slice4(bytes, 0).ok_or(ProbeError::TooShort { needed: 4, got: bytes.len() })?;
    let endian = match &magic {
        b"RIFF" => Endian::Little,
        b"RIFX" => Endian::Big,
        _ => {
            return Err(ProbeError::NotCdr { reason: format!("未知 RIFF 魔数 {}", format_fourcc(magic)) });
        }
    };

    if bytes.len() < 12 {
        return Err(ProbeError::TooShort { needed: 12, got: bytes.len() });
    }

    let size_bytes = slice4(bytes, 4).ok_or(ProbeError::TooShort { needed: 12, got: bytes.len() })?;
    let declared_size = read_u32(endian, size_bytes);
    if declared_size < 4 {
        return Err(ProbeError::Invalid { offset: 4, reason: format!("RIFF declared_size ({declared_size}) 小于最小 form 长度 4") });
    }

    let form = slice4(bytes, 8).ok_or(ProbeError::TooShort { needed: 12, got: bytes.len() })?;
    let actual_len = bytes.len() as u64;
    let declared_end = 8u64.saturating_add(u64::from(declared_size));

    if declared_end > actual_len {
        return Err(ProbeError::Truncated { offset: 8, needed: u64::from(declared_size), available: actual_len.saturating_sub(8) });
    }

    let mut warnings = Vec::new();
    if declared_end < actual_len {
        warnings.push(ProbeWarning::TrailingBytes { declared_end, actual_len });
    }

    let root_end = declared_end as usize;
    let mut stack = vec![Frame { cursor: 12, end: root_end, depth: 0 }];
    let mut chunks = Vec::new();
    let mut vrsn_chunk_value = None;

    while let Some(frame) = stack.last_mut() {
        if frame.cursor >= frame.end {
            stack.pop();
            continue;
        }

        let chunk_offset = frame.cursor;
        let remaining = frame.end.saturating_sub(chunk_offset);
        if remaining < 8 {
            return Err(ProbeError::Truncated { offset: chunk_offset as u64, needed: 8, available: remaining as u64 });
        }

        if chunks.len() >= MAX_CHUNKS {
            return Err(ProbeError::LimitExceeded { limit: "max_chunks".to_string(), max: MAX_CHUNKS });
        }

        let raw_fourcc =
            slice4(bytes, chunk_offset).ok_or(ProbeError::Truncated { offset: chunk_offset as u64, needed: 4, available: remaining as u64 })?;
        let chunk_size_bytes = slice4(bytes, chunk_offset + 4).ok_or(ProbeError::Truncated {
            offset: (chunk_offset + 4) as u64,
            needed: 4,
            available: remaining.saturating_sub(4) as u64,
        })?;
        let chunk_size = read_u32(endian, chunk_size_bytes);

        let payload_start = chunk_offset + 8;
        let payload_end_u64 = (payload_start as u64)
            .checked_add(u64::from(chunk_size))
            .ok_or(ProbeError::Invalid { offset: chunk_offset as u64, reason: "chunk 载荷偏移算术溢出".to_string() })?;

        if payload_end_u64 > (frame.end as u64) {
            return Err(ProbeError::Truncated {
                offset: payload_start as u64,
                needed: u64::from(chunk_size),
                available: (frame.end - payload_start) as u64,
            });
        }

        let payload_end = payload_end_u64 as usize;
        let needs_pad = (chunk_size & 1) == 1;
        let next_cursor = if needs_pad {
            if payload_end < frame.end {
                if let Some(&pad_byte) = bytes.get(payload_end)
                    && pad_byte != 0
                {
                    warnings.push(ProbeWarning::NonZeroPadByte { offset: payload_end as u64, byte: pad_byte });
                }
                payload_end + 1
            } else if payload_end == root_end {
                // 文件末尾最后一个奇数长度 chunk 省略了 pad 字节：容忍并记录告警
                warnings.push(ProbeWarning::MissingTrailingPad { offset: payload_end as u64 });
                payload_end
            } else {
                return Err(ProbeError::Truncated { offset: payload_end as u64, needed: 1, available: 0 });
            }
        } else {
            payload_end
        };

        let current_depth = frame.depth;
        frame.cursor = next_cursor;

        // 提取顶层 vrsn chunk（uint16 版本号）
        if current_depth == 0
            && &raw_fourcc == b"vrsn"
            && chunk_size >= 2
            && vrsn_chunk_value.is_none()
            && let Some(raw_vrsn) = slice2(bytes, payload_start)
        {
            vrsn_chunk_value = Some(read_u16(endian, raw_vrsn));
        }

        let fourcc_str = format_fourcc(raw_fourcc);

        if &raw_fourcc == b"LIST" {
            if chunk_size < 4 {
                return Err(ProbeError::Invalid {
                    offset: chunk_offset as u64,
                    reason: format!("LIST chunk 长度 ({chunk_size}) 小于 4 字节，缺少 list_type"),
                });
            }
            let raw_list_type =
                slice4(bytes, payload_start).ok_or(ProbeError::Truncated { offset: payload_start as u64, needed: 4, available: 0 })?;
            let list_type_str = format_fourcc(raw_list_type);
            let is_opaque = matches!(&raw_list_type, b"cmpr" | b"stlt");

            chunks.push(ChunkSummary {
                fourcc: fourcc_str,
                raw_fourcc,
                offset: chunk_offset as u64,
                size: chunk_size,
                depth: current_depth,
                list_type: Some(list_type_str.clone()),
                opaque_payload: is_opaque,
            });

            if is_opaque {
                warnings.push(ProbeWarning::OpaqueListSkipped { list_type: list_type_str, offset: chunk_offset as u64, size: chunk_size });
            } else {
                let next_depth = current_depth + 1;
                if next_depth > MAX_DEPTH {
                    return Err(ProbeError::LimitExceeded { limit: "max_depth".to_string(), max: MAX_DEPTH });
                }
                stack.push(Frame { cursor: payload_start + 4, end: payload_end, depth: next_depth });
            }
        } else {
            chunks.push(ChunkSummary {
                fourcc: fourcc_str,
                raw_fourcc,
                offset: chunk_offset as u64,
                size: chunk_size,
                depth: current_depth,
                list_type: None,
                opaque_payload: false,
            });
        }
    }

    Ok(RiffInspection { endian, declared_size, form, vrsn_chunk_value, chunks, warnings })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造最小合法的 12 字节空 RIFF 容器：`RIFF` + `size=4` + `form`。
    fn synth_minimal_riff(form: &[u8; 4]) -> Vec<u8> {
        let mut v = Vec::with_capacity(12);
        v.extend_from_slice(b"RIFF");
        v.extend_from_slice(&4u32.to_le_bytes());
        v.extend_from_slice(form);
        v
    }

    /// 构造一个带 padding 的普通 chunk 字节块。
    fn make_chunk(fourcc: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(fourcc);
        v.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        v.extend_from_slice(payload);
        if payload.len() % 2 == 1 {
            v.push(0x00);
        }
        v
    }

    /// 构造一个 `LIST` chunk 字节块。
    fn make_list(list_type: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut payload = Vec::with_capacity(4 + body.len());
        payload.extend_from_slice(list_type);
        payload.extend_from_slice(body);
        make_chunk(b"LIST", &payload)
    }

    #[test]
    fn parses_minimal_12_byte_riff() {
        let data = synth_minimal_riff(b"CDR7");
        assert_eq!(data.len(), 12);
        let res = inspect_riff(&data).expect("12 字节最小合法 RIFF 应解析成功");
        assert_eq!(res.endian, Endian::Little);
        assert_eq!(res.declared_size, 4);
        assert_eq!(&res.form, b"CDR7");
        assert!(res.chunks.is_empty());
        assert!(res.warnings.is_empty());
    }

    #[test]
    fn parses_vrsn_odd_padding_and_nested_list_chunks() {
        let vrsn = make_chunk(b"vrsn", &700u16.to_le_bytes());
        let odd_chunk = make_chunk(b"DISP", b"abc"); // size = 3 + 1 byte pad
        let info_child = make_chunk(b"INAM", b"test");
        let list_info = make_list(b"INFO", &info_child);

        let mut body = Vec::new();
        body.extend_from_slice(&vrsn);
        body.extend_from_slice(&odd_chunk);
        body.extend_from_slice(&list_info);

        let mut riff = Vec::new();
        riff.extend_from_slice(b"RIFF");
        riff.extend_from_slice(&((4 + body.len()) as u32).to_le_bytes());
        riff.extend_from_slice(b"CDR7");
        riff.extend_from_slice(&body);

        let res = inspect_riff(&riff).expect("多 chunk RIFF 应解析成功");
        assert_eq!(res.vrsn_chunk_value, Some(700));
        assert_eq!(res.chunks.len(), 4);
        assert_eq!(res.chunks[0].fourcc, "vrsn");
        assert_eq!(res.chunks[0].depth, 0);
        assert_eq!(res.chunks[1].fourcc, "DISP");
        assert_eq!(res.chunks[1].size, 3);
        assert_eq!(res.chunks[2].fourcc, "LIST");
        assert_eq!(res.chunks[2].list_type.as_deref(), Some("INFO"));
        assert!(!res.chunks[2].opaque_payload);
        assert_eq!(res.chunks[3].fourcc, "INAM");
        assert_eq!(res.chunks[3].depth, 1);
    }

    #[test]
    fn skips_opaque_cmpr_and_stlt_lists_without_descending() {
        // Photopea 规范特例：cmpr 与 stlt 内部不是普通子 chunk（size 字段实为索引），若强行下钻会报错
        let fake_cmpr_body = b"\xFF\xFF\xFF\xFFnot_valid_subchunks";
        let cmpr_list = make_list(b"cmpr", fake_cmpr_body);
        let stlt_list = make_list(b"stlt", b"\x01\x02\x03\x04\x99\x99\x99\x99");

        let mut body = Vec::new();
        body.extend_from_slice(&cmpr_list);
        body.extend_from_slice(&stlt_list);

        let mut riff = Vec::new();
        riff.extend_from_slice(b"RIFF");
        riff.extend_from_slice(&((4 + body.len()) as u32).to_le_bytes());
        riff.extend_from_slice(b"CDR8");
        riff.extend_from_slice(&body);

        let res = inspect_riff(&riff).expect("含 cmpr/stlt 的 RIFF 应整体跳过不透明列表而不报错");
        assert_eq!(res.chunks.len(), 2);
        assert!(res.chunks[0].opaque_payload);
        assert_eq!(res.chunks[0].list_type.as_deref(), Some("cmpr"));
        assert!(res.chunks[1].opaque_payload);
        assert_eq!(res.chunks[1].list_type.as_deref(), Some("stlt"));
        assert_eq!(res.warnings.len(), 2);
    }

    #[test]
    fn rejects_excessive_list_nesting_depth() {
        // 构造 35 层嵌套 LIST（超过 MAX_DEPTH = 32）
        let mut nested = make_chunk(b"leaf", b"ok");
        for _ in 0..35 {
            nested = make_list(b"sect", &nested);
        }
        let mut riff = Vec::new();
        riff.extend_from_slice(b"RIFF");
        riff.extend_from_slice(&((4 + nested.len()) as u32).to_le_bytes());
        riff.extend_from_slice(b"CDR9");
        riff.extend_from_slice(&nested);

        let err = inspect_riff(&riff).expect_err("超过 MAX_DEPTH 的嵌套 LIST 必须返回错误而非栈溢出");
        assert_eq!(err, ProbeError::LimitExceeded { limit: "max_depth".to_string(), max: MAX_DEPTH });
    }

    #[test]
    fn rejects_overflowing_chunk_size_without_panic() {
        let mut riff = Vec::new();
        riff.extend_from_slice(b"RIFF");
        riff.extend_from_slice(&12u32.to_le_bytes());
        riff.extend_from_slice(b"CDR7");
        riff.extend_from_slice(b"badc");
        riff.extend_from_slice(&u32::MAX.to_le_bytes());

        let err = inspect_riff(&riff).expect_err("u32::MAX chunk size 应返回 Truncated");
        assert!(matches!(err, ProbeError::Truncated { .. }));
    }

    #[test]
    fn tolerates_missing_trailing_pad_at_eof_with_warning() {
        let mut riff = Vec::new();
        riff.extend_from_slice(b"RIFF");
        // form(4) + chunk header(8) + odd payload(3) = 15 bytes (无末尾 pad)
        riff.extend_from_slice(&15u32.to_le_bytes());
        riff.extend_from_slice(b"CDR7");
        riff.extend_from_slice(b"odd ");
        riff.extend_from_slice(&3u32.to_le_bytes());
        riff.extend_from_slice(b"xyz");

        let res = inspect_riff(&riff).expect("EOF 处缺失末尾对齐字节应容忍");
        assert_eq!(res.chunks.len(), 1);
        assert!(res.warnings.contains(&ProbeWarning::MissingTrailingPad { offset: 23 }));
    }

    #[test]
    fn records_warning_on_trailing_bytes_beyond_declared_size() {
        let mut data = synth_minimal_riff(b"CDR7");
        data.extend_from_slice(b"\x00\x00\x00"); // 12 字节合法 RIFF 后多出 3 字节尾随垃圾
        let res = inspect_riff(&data).expect("带尾随垃圾字节的合法 RIFF 应成功并记录 TrailingBytes 告警");
        assert!(res.warnings.contains(&ProbeWarning::TrailingBytes { declared_end: 12, actual_len: 15 }));
    }

    #[test]
    fn supports_big_endian_rifx_container() {
        let mut data = Vec::new();
        data.extend_from_slice(b"RIFX");
        data.extend_from_slice(&14u32.to_be_bytes());
        data.extend_from_slice(b"CMX1");
        data.extend_from_slice(b"vrsn");
        data.extend_from_slice(&2u32.to_be_bytes());
        data.extend_from_slice(&100u16.to_be_bytes());

        let res = inspect_riff(&data).expect("大端 RIFX 容器应解析成功");
        assert_eq!(res.endian, Endian::Big);
        assert_eq!(&res.form, b"CMX1");
        assert_eq!(res.vrsn_chunk_value, Some(100));
    }
}
