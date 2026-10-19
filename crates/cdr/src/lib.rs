//! coralcraft-cdr：CorelDRAW `.cdr` / `.cmx` 文件格式层（净室实现）。
//!
//! 三阶段演进：probe（容器/版本识别）→ parse（对象树）→ build（写出，长期战役）。
//! 一切输入按敌意数据处理：API 只返回 `Result`，禁止 panic。
//!
//! 行为依据（净室来源，逐条可溯）：
//! - Photopea CDR 规范（社区逆向的人类可读格式文档）：
//!   <https://github.com/photopea/CDR-specification>（本地镜像 `refs/cdr-spec/`）
//! - libcdr 公开文档页（语义差异对照，仅读不抄码）：
//!   <https://wiki.documentfoundation.org/DLP/Libraries/libcdr>
//!
//! 本 crate 当前处于 M1 Task 9/10 之前：probe 为红灯锚点（`todo!`），
//! 见 `docs/superpowers/plans/2026-10-19-m0-bootstrap-m1-cdr-probe.md`。

use thiserror::Error;

/// probe 阶段的失败。全部可恢复、可展示给用户，禁止 panic。
#[derive(Debug, Error, PartialEq)]
pub enum ProbeError {
    /// 输入太短，连容器魔数都不够。
    #[error("输入不足 12 字节，无法识别容器")]
    TooShort,
    /// 不是 CDR/CMX 文件（魔数不匹配）。
    #[error("不是 CDR/CMX 文件：{0}")]
    NotCdr(String),
}

/// CDR 家族容器类型。
///
/// 依据 Photopea 规范：1992 年前老格式以 ASCII `WL` 开头；
/// 之后为 RIFF 容器，形式为 `RIFF` + u32 LE 长度 + `CDR`/`CMX` + 版本字节。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CdrContainer {
    /// 现代 RIFF 容器；`kind` 区分 CDR/CMX，`version_byte` 为格式版本（如 0x07 = 版本 7）。
    Riff { kind: CdrKind, version_byte: u8 },
    /// 1992 年前的老格式（`WL` 魔数）。M3 之后视语料再排期。
    LegacyWl,
}

/// RIFF 容器内的文档种类（第 8–10 字节的四码，第 11 字节是版本）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CdrKind {
    Cdr,
    Cmx,
}

/// 识别字节流的容器类型。M1 Task 10 的实现目标（当前为红灯锚点）。
///
/// # 红灯测试约定（TDD）
/// - `RIFF....CDR\x07` → `Riff { kind: Cdr, version_byte: 7 }`
/// - `RIFF....CMX\x01` → `Riff { kind: Cmx, version_byte: 1 }`
/// - `WL` 开头 → `LegacyWl`
/// - 其他 → `NotCdr`
pub fn probe(_bytes: &[u8]) -> Result<CdrContainer, ProbeError> {
    todo!("M1 Task 9/10：先 riff.rs 走读器，后转绿（见计划文档）")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 合成 fixture：Photopea 规范 §RIFF Format 的最小头部。
    /// `RIFF` + u32 LE size + `CDR` + 版本字节 0x07。
    fn synth_riff_cdr(version: u8) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(b"RIFF");
        v.extend_from_slice(&4u32.to_le_bytes()); // 后续数据长度（示意）
        v.extend_from_slice(b"CDR");
        v.push(version);
        v.extend_from_slice(b"\x00\x00\x00"); // 占位，凑 12 字节
        v
    }

    #[test]
    fn detects_riff_cdr_container_and_version() {
        let got = probe(&synth_riff_cdr(0x07)).expect("RIFF/CDR 头应识别成功");
        assert_eq!(got, CdrContainer::Riff { kind: CdrKind::Cdr, version_byte: 0x07 });
    }

    #[test]
    fn detects_riff_cmx_container() {
        let mut v = synth_riff_cdr(0x01);
        v[8..11].copy_from_slice(b"CMX");
        let got = probe(&v).expect("RIFF/CMX 头应识别成功");
        assert_eq!(got, CdrContainer::Riff { kind: CdrKind::Cmx, version_byte: 0x01 });
    }

    #[test]
    fn detects_legacy_wl_container() {
        let v = b"WL\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00";
        assert_eq!(probe(v), Ok(CdrContainer::LegacyWl));
    }

    #[test]
    fn rejects_non_cdr_input() {
        let v = b"PDF-1.7 garbage";
        assert!(matches!(probe(v), Err(ProbeError::NotCdr(_))));
    }

    #[test]
    fn rejects_too_short_input() {
        assert_eq!(probe(b"RIFF"), Err(ProbeError::TooShort));
    }
}
