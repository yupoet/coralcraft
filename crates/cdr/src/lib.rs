//! coralcraft-cdr：CorelDRAW `.cdr` / `.cmx` 文件格式层（净室实现）。
//!
//! 三阶段演进：probe（容器/版本识别）→ parse（对象树）→ build（写出，长期战役）。
//! 一切输入按敌意数据处理：API 只返回 `Result`，禁止 panic。
//!
//! 行为依据（净室来源，逐条可溯）：
//! - Photopea CDR 规范（社区逆向的人类可读格式文档）：
//!   <https://github.com/photopea/CDR-specification>（本地镜像 `refs/cdr-spec/`）
//! - libcdr 公开文档页：
//!   <https://wiki.documentfoundation.org/DLP/Libraries/libcdr>

pub mod error;
pub mod riff;
pub mod zip;

use serde::{Deserialize, Serialize};

pub use crate::error::{ProbeError, ProbeWarning};
pub use crate::riff::{ChunkSummary, Endian, RiffInspection};
pub use crate::zip::{ZipCdrRole, ZipInspection};

/// 当前 `ProbeReport` 的 JSON Schema 版本号。
pub const PROBE_SCHEMA_VERSION: u32 = 1;

/// RIFF 容器内的文档种类（由 4 字节 form-type 前缀判别）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CdrKind {
    /// CorelDRAW 主文档格式（`CDR?` 或小写 `cdr?`）。
    Cdr,
    /// Corel Presentation Exchange 交换格式（`CMX1`）。
    Cmx,
}

/// 版本识别信息（结合 Form-type 第 4 字节 ASCII 编码与顶层 `vrsn` chunk）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionInfo {
    /// Form-type 第 4 字节原始值（例如 `b'7'` = `0x37`，`b'D'` = `0x44`）。
    pub form_version_byte: u8,
    /// 由 Form-type 第 4 字节推导的百倍版本号（如 `' '`/`'3'` → `300`，`'7'` → `700`，`'D'` → `1300`，`'E'` → `1400`）。
    pub form_version: Option<u16>,
    /// 顶层 `vrsn` chunk 中的 16 位版本号（若存在）。
    pub vrsn_chunk: Option<u16>,
    /// 综合判定的有效版本号（优先采用 `vrsn_chunk`，否则回退至 `form_version`）。
    pub effective_version: Option<u16>,
}

/// CDR 家族容器类型（覆盖三大历史世代：Legacy `WL`、裸 `RIFF`/`RIFX`、X4+ `ZIP+RIFF`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "container_kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum CdrContainer {
    /// 裸 RIFF / RIFX 容器（v3–X3 `.cdr` 及 `.cmx`）。
    Riff { kind: CdrKind, endian: Endian, form: String, raw_form: [u8; 4], version: VersionInfo },
    /// 现代 ZIP 封装容器（X4 / v14 及以后版本，内含 `content/riffData.cdr` 或 `content/root.dat`）。
    Zip { primary_entry: String, role: ZipCdrRole, has_data_file_list: bool, entry_count: usize, inner_version: Option<VersionInfo> },
    /// 1992 年前的老格式（`WL` 魔数，v1–v2）。
    LegacyWl,
}

/// `probe` 的完整结构化诊断报告（供 CLI 人类可读输出、`--json` 与 MCP 调用共享）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProbeReport {
    /// JSON 输出结构版本（恒为 [`PROBE_SCHEMA_VERSION`] = 1）。
    pub schema_version: u32,
    /// 输入字节流总长度。
    pub file_size: u64,
    /// 识别出的容器与版本详情。
    pub container: CdrContainer,
    /// 提取到的 RIFF chunk 摘要列表（裸 RIFF 或 Stored ZIP 内层 RIFF）。
    pub chunks: Vec<ChunkSummary>,
    /// 非致命规范偏差或降级告警。
    pub warnings: Vec<ProbeWarning>,
}

/// 将 CDR Form-type 的第 4 字节（ASCII 版本字符）映射为规范中的百倍版本号 $V$。
///
/// 依据公开规范与版本演进表：
/// - `b' '` (`0x20`) 或 `b'3'` (`0x33`) → `Some(300)` (v3)
/// - `b'4'..=b'9'` → `Some(400..=900)` (v4–v9)
/// - `b'A'..=b'Z'` → `Some(1000..=3500)` (v10–X3 为 `A`–`D`，X4 为 `E`=1400，以此类推)
pub fn decode_cdr_version_char(byte: u8) -> Option<u16> {
    match byte {
        b' ' | b'3' => Some(300),
        b'4'..=b'9' => Some(u16::from(byte - b'0') * 100),
        b'A'..=b'Z' => Some((u16::from(byte - b'A') + 10) * 100),
        b'a'..=b'z' => Some((u16::from(byte - b'a') + 10) * 100),
        _ => None,
    }
}

fn classify_riff_form(form: [u8; 4], vrsn_chunk: Option<u16>, warnings: &mut Vec<ProbeWarning>) -> Result<(CdrKind, VersionInfo), ProbeError> {
    let prefix = [form[0], form[1], form[2]];
    let version_byte = form[3];
    let form_str = crate::riff::format_fourcc(form);

    if &prefix == b"CDR" || &prefix == b"cdr" {
        if &prefix == b"cdr" {
            warnings.push(ProbeWarning::NonCanonicalForm { form: form_str });
        }
        let form_version = decode_cdr_version_char(version_byte);
        if form_version.is_none() {
            warnings.push(ProbeWarning::UnknownVersionChar { byte: version_byte });
        }
        if let (Some(fv), Some(vc)) = (form_version, vrsn_chunk)
            && fv != vc
        {
            warnings.push(ProbeWarning::VersionMismatch { form_version: fv, vrsn_chunk: vc });
        }
        let effective_version = vrsn_chunk.or(form_version);
        Ok((CdrKind::Cdr, VersionInfo { form_version_byte: version_byte, form_version, vrsn_chunk, effective_version }))
    } else if prefix.eq_ignore_ascii_case(b"CMX") {
        if &prefix != b"CMX" {
            warnings.push(ProbeWarning::NonCanonicalForm { form: form_str });
        }
        let form_version = match version_byte {
            b'1'..=b'9' => Some(u16::from(version_byte - b'0') * 100),
            _ => {
                warnings.push(ProbeWarning::UnknownVersionChar { byte: version_byte });
                None
            }
        };
        let effective_version = vrsn_chunk.or(form_version);
        Ok((CdrKind::Cmx, VersionInfo { form_version_byte: version_byte, form_version, vrsn_chunk, effective_version }))
    } else {
        Err(ProbeError::NotCdr { reason: format!("RIFF form-type 并非 CDR/CMX：{form_str}") })
    }
}

/// 识别字节流的容器类型、版本与顶层 chunk 清单。
pub fn probe(bytes: &[u8]) -> Result<ProbeReport, ProbeError> {
    if bytes.len() < 4 {
        return Err(ProbeError::TooShort { needed: 4, got: bytes.len() });
    }

    let file_size = bytes.len() as u64;

    // 1. RIFF / RIFX 容器（v3–X3 .cdr 及 .cmx）
    if matches!(bytes.get(0..4), Some(b"RIFF" | b"RIFX")) {
        let mut riff_info = crate::riff::inspect_riff(bytes)?;
        let (kind, version) = classify_riff_form(riff_info.form, riff_info.vrsn_chunk_value, &mut riff_info.warnings)?;
        let form_str = crate::riff::format_fourcc(riff_info.form);
        return Ok(ProbeReport {
            schema_version: PROBE_SCHEMA_VERSION,
            file_size,
            container: CdrContainer::Riff { kind, endian: riff_info.endian, form: form_str, raw_form: riff_info.form, version },
            chunks: riff_info.chunks,
            warnings: riff_info.warnings,
        });
    }

    // 2. X4+ 现代 ZIP 容器（PK\x03\x04）
    if bytes.get(0..4) == Some(b"PK\x03\x04") {
        let mut zip_info = crate::zip::inspect_zip_cdr(bytes)?;
        let mut chunks = Vec::new();
        let inner_version = if let Some(inner) = zip_info.inner_riff.take() {
            let (_kind, ver) = classify_riff_form(inner.form, inner.vrsn_chunk_value, &mut zip_info.warnings)?;
            chunks = inner.chunks;
            Some(ver)
        } else {
            None
        };
        return Ok(ProbeReport {
            schema_version: PROBE_SCHEMA_VERSION,
            file_size,
            container: CdrContainer::Zip {
                primary_entry: zip_info.primary_entry,
                role: zip_info.role,
                has_data_file_list: zip_info.has_data_file_list,
                entry_count: zip_info.entry_count,
                inner_version,
            },
            chunks,
            warnings: zip_info.warnings,
        });
    }

    // 3. 1992 年前的 Legacy WL 容器（WL + 1B sub-version + 0x00，至少 12 字节头部）
    if bytes.get(0..2) == Some(b"WL") && bytes[3] == 0x00 {
        if bytes.len() < 12 {
            return Err(ProbeError::TooShort { needed: 12, got: bytes.len() });
        }
        return Ok(ProbeReport {
            schema_version: PROBE_SCHEMA_VERSION,
            file_size,
            container: CdrContainer::LegacyWl,
            chunks: Vec::new(),
            warnings: Vec::new(),
        });
    }

    let head = [bytes[0], bytes[1], bytes[2], bytes[3]];
    Err(ProbeError::NotCdr { reason: format!("未知文件头部魔数 {}", crate::riff::format_fourcc(head)) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zip::tests::synth_zip;

    /// 合成最小合法 12 字节 RIFF CDR/CMX 容器：`RIFF` + `size=4` + 4 字节 ASCII `form`。
    fn synth_riff(form: &[u8; 4]) -> Vec<u8> {
        let mut v = Vec::with_capacity(12);
        v.extend_from_slice(b"RIFF");
        v.extend_from_slice(&4u32.to_le_bytes());
        v.extend_from_slice(form);
        v
    }

    #[test]
    fn detects_riff_cdr7_ascii_version() {
        let report = probe(&synth_riff(b"CDR7")).expect("RIFF/CDR7 应识别成功");
        assert_eq!(report.schema_version, PROBE_SCHEMA_VERSION);
        assert_eq!(
            report.container,
            CdrContainer::Riff {
                kind: CdrKind::Cdr,
                endian: Endian::Little,
                form: "CDR7".to_string(),
                raw_form: *b"CDR7",
                version: VersionInfo { form_version_byte: b'7', form_version: Some(700), vrsn_chunk: None, effective_version: Some(700) },
            }
        );
    }

    #[test]
    fn detects_riff_cdrd_x3_and_v3_space_versions() {
        let rep_x3 = probe(&synth_riff(b"CDRD")).expect("CDRD (X3) 应识别成功");
        if let CdrContainer::Riff { version, .. } = rep_x3.container {
            assert_eq!(version.effective_version, Some(1300));
        } else {
            panic!("预期 CdrContainer::Riff");
        }

        let rep_v3 = probe(&synth_riff(b"CDR ")).expect("CDR  (v3) 应识别成功");
        if let CdrContainer::Riff { version, .. } = rep_v3.container {
            assert_eq!(version.effective_version, Some(300));
        } else {
            panic!("预期 CdrContainer::Riff");
        }
    }

    #[test]
    fn accepts_lowercase_cdr6_with_warning() {
        let rep = probe(&synth_riff(b"cdr6")).expect("小写 cdr6 应兼容识别");
        assert!(rep.warnings.contains(&ProbeWarning::NonCanonicalForm { form: "cdr6".to_string() }));
        if let CdrContainer::Riff { kind, version, .. } = rep.container {
            assert_eq!(kind, CdrKind::Cdr);
            assert_eq!(version.effective_version, Some(600));
        } else {
            panic!("预期 CdrContainer::Riff");
        }
    }

    #[test]
    fn detects_riff_and_rifx_cmx1_containers() {
        let rep_le = probe(&synth_riff(b"CMX1")).expect("RIFF/CMX1 应识别成功");
        if let CdrContainer::Riff { kind, endian, version, .. } = rep_le.container {
            assert_eq!(kind, CdrKind::Cmx);
            assert_eq!(endian, Endian::Little);
            assert_eq!(version.form_version, Some(100));
        } else {
            panic!("预期 CdrContainer::Riff");
        }

        let mut rifx = Vec::with_capacity(12);
        rifx.extend_from_slice(b"RIFX");
        rifx.extend_from_slice(&4u32.to_be_bytes());
        rifx.extend_from_slice(b"CMX1");
        let rep_be = probe(&rifx).expect("RIFX/CMX1 (Mac 大端) 应识别成功");
        if let CdrContainer::Riff { kind, endian, .. } = rep_be.container {
            assert_eq!(kind, CdrKind::Cmx);
            assert_eq!(endian, Endian::Big);
        } else {
            panic!("预期 CdrContainer::Riff");
        }
    }

    #[test]
    fn detects_modern_zip_cdr_container() {
        let inner = synth_riff(b"CDRE"); // X4 = 1400
        let zip_bytes = synth_zip(&[("content/riffData.cdr", 0, &inner)]);
        let rep = probe(&zip_bytes).expect("X4 ZIP 容器应识别成功");
        assert_eq!(
            rep.container,
            CdrContainer::Zip {
                primary_entry: "content/riffData.cdr".to_string(),
                role: ZipCdrRole::RiffDataCdr,
                has_data_file_list: false,
                entry_count: 1,
                inner_version: Some(VersionInfo {
                    form_version_byte: b'E',
                    form_version: Some(1400),
                    vrsn_chunk: None,
                    effective_version: Some(1400),
                }),
            }
        );
    }

    #[test]
    fn detects_legacy_wl_container() {
        let v = b"WL\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00";
        let rep = probe(v).expect("WL 老容器应识别成功");
        assert_eq!(rep.container, CdrContainer::LegacyWl);
    }

    #[test]
    fn rejects_non_cdr_riff_and_non_cdr_zip_and_garbage() {
        // 1. RIFF WAVE 音频不是 CDR
        let wave = synth_riff(b"WAVE");
        assert!(matches!(probe(&wave), Err(ProbeError::NotCdr { .. })));

        // 2. 普通 ZIP/DOCX 不是 CDR
        let docx = synth_zip(&[("word/document.xml", 0, b"<w:document/>")]);
        assert!(matches!(probe(&docx), Err(ProbeError::NotCdr { .. })));

        // 3. PDF 文件不是 CDR
        assert!(matches!(probe(b"%PDF-1.7 garbage"), Err(ProbeError::NotCdr { .. })));
    }

    #[test]
    fn rejects_too_short_input() {
        assert_eq!(probe(b"RI"), Err(ProbeError::TooShort { needed: 4, got: 2 }));
        assert_eq!(probe(b"RIFF\x04\x00"), Err(ProbeError::TooShort { needed: 12, got: 6 }));
    }

    #[test]
    fn hostile_random_mutations_never_panic() {
        // 固定种子 Xorshift64 对 RIFF、ZIP、WL 模板做 10,000 次截断与字节翻转，断言绝不 panic
        let seeds: [Vec<u8>; 3] = [
            synth_riff(b"CDR7"),
            synth_zip(&[("content/riffData.cdr", 0, &synth_riff(b"CDRE"))]),
            b"WL\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0A\x0B\x0C".to_vec(),
        ];
        let mut state: u64 = 0xC0DEC0FFEE123456;
        let mut next_u64 = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };

        for i in 0..10_000usize {
            let base = &seeds[i % seeds.len()];
            let mut mutated = base.clone();
            let mode = next_u64() % 4;
            match mode {
                0 => {
                    // 随机截断
                    let cut = (next_u64() as usize) % (mutated.len() + 1);
                    mutated.truncate(cut);
                }
                1 => {
                    // 随机单字节翻转
                    if !mutated.is_empty() {
                        let idx = (next_u64() as usize) % mutated.len();
                        mutated[idx] ^= (next_u64() & 0xFF) as u8;
                    }
                }
                2 => {
                    // 随机追加垃圾字节
                    let extra = (next_u64() % 32) as usize;
                    for _ in 0..extra {
                        mutated.push((next_u64() & 0xFF) as u8);
                    }
                }
                _ => {
                    // 纯随机短切片
                    let len = (next_u64() % 64) as usize;
                    mutated.clear();
                    for _ in 0..len {
                        mutated.push((next_u64() & 0xFF) as u8);
                    }
                }
            }
            let _ = probe(&mutated);
        }
    }
}
