//! RIFF / RIFX container and chunk walker (`coralcraft-cdr::riff`).
//!
//! Clean-room implementation based on the public Photopea CDR specification (§RIFF Format)
//! and standard RIFF rules:
//! - 12-byte header: `RIFF` (little-endian) or `RIFX` (big-endian) + `u32` size + 4-byte `form`.
//! - Each chunk has an 8-byte header (4-byte `fourcc` + `u32` size) + `size` payload bytes
//!   + 1 alignment pad byte when `size` is odd.
//! - The first 4 payload bytes of a `LIST` chunk store its `list_type`; when `list_type` is
//!   `"cmpr"` or `"stlt"`, the body is not a sequence of standard RIFF sub-chunks (per the
//!   Photopea specification) and is skipped as an opaque payload (`opaque_payload = true`).
//! - Hostile-input hardening: explicit stack iteration (no recursion), `MAX_DEPTH = 32`,
//!   `MAX_CHUNKS = 100_000`, and `u64` + `checked_add` arithmetic throughout.

use serde::{Deserialize, Serialize};

use crate::error::{ProbeError, ProbeWarning};

/// Maximum allowed `LIST` nesting depth (exceeding returns `ProbeError::LimitExceeded`).
pub const MAX_DEPTH: usize = 32;

/// Maximum total number of chunks scanned per file (prevents zero-length chunk DoS).
pub const MAX_CHUNKS: usize = 100_000;

/// Byte order of the RIFF container.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Endian {
    /// Standard `RIFF` (little-endian).
    Little,
    /// `RIFX` (big-endian, used by Mac CMX files).
    Big,
}

/// Structured summary of a single RIFF chunk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkSummary {
    /// Printable FourCC string (falls back to `0xHHHHHHHH` hex when non-printable).
    pub fourcc: String,
    /// Raw 4-byte identifier.
    pub raw_fourcc: [u8; 4],
    /// Starting byte offset of the chunk header within the inspected slice.
    pub offset: u64,
    /// Declared payload size in bytes (excluding the 8-byte header and trailing pad byte).
    pub size: u32,
    /// Nesting depth (`0` for top-level chunks, `parent.depth + 1` inside `LIST` chunks).
    pub depth: usize,
    /// 4-byte `list_type` when `raw_fourcc == *b"LIST"`.
    pub list_type: Option<String>,
    /// True if this `LIST` has a non-RIFF internal layout (`cmpr` / `stlt`) and was skipped as opaque.
    pub opaque_payload: bool,
}

/// Result of inspecting a RIFF / RIFX container stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiffInspection {
    /// Container endianness (`RIFF` = `Little`, `RIFX` = `Big`).
    pub endian: Endian,
    /// Declared byte size from header offset `4..8`.
    pub declared_size: u32,
    /// Raw 4-byte form-type at header offset `8..12` (e.g., `*b"CDR7"`, `*b"CMX1"`).
    pub form: [u8; 4],
    /// Numeric version extracted from the first top-level `"vrsn"` chunk (`size >= 2`), if present.
    pub vrsn_chunk_value: Option<u16>,
    /// Pre-order list of chunks encountered.
    pub chunks: Vec<ChunkSummary>,
    /// Non-fatal warnings recorded during inspection.
    pub warnings: Vec<ProbeWarning>,
}

/// Formats a 4-byte FourCC into a printable ASCII string, or `0xHHHHHHHH` if any byte is outside `0x20..=0x7E`.
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

/// Walks and validates a RIFF / RIFX container byte slice.
pub fn inspect_riff(bytes: &[u8]) -> Result<RiffInspection, ProbeError> {
    if bytes.len() < 4 {
        return Err(ProbeError::TooShort { needed: 4, got: bytes.len() });
    }
    let magic = slice4(bytes, 0).ok_or(ProbeError::TooShort { needed: 4, got: bytes.len() })?;
    let endian = match &magic {
        b"RIFF" => Endian::Little,
        b"RIFX" => Endian::Big,
        _ => {
            return Err(ProbeError::NotCdr { reason: format!("unknown RIFF magic {}", format_fourcc(magic)) });
        }
    };

    if bytes.len() < 12 {
        return Err(ProbeError::TooShort { needed: 12, got: bytes.len() });
    }

    let size_bytes = slice4(bytes, 4).ok_or(ProbeError::TooShort { needed: 12, got: bytes.len() })?;
    let declared_size = read_u32(endian, size_bytes);
    if declared_size < 4 {
        return Err(ProbeError::Invalid { offset: 4, reason: format!("RIFF declared_size ({declared_size}) is smaller than minimum form length 4") });
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
        let chunk_size_bytes = slice4(bytes, chunk_offset.saturating_add(4)).ok_or(ProbeError::Truncated {
            offset: chunk_offset.saturating_add(4) as u64,
            needed: 4,
            available: remaining.saturating_sub(4) as u64,
        })?;
        let chunk_size = read_u32(endian, chunk_size_bytes);

        let payload_start = chunk_offset.saturating_add(8);
        let payload_end_u64 = (payload_start as u64)
            .checked_add(u64::from(chunk_size))
            .ok_or(ProbeError::Invalid { offset: chunk_offset as u64, reason: "chunk payload offset arithmetic overflow".to_string() })?;

        if payload_end_u64 > (frame.end as u64) {
            return Err(ProbeError::Truncated {
                offset: payload_start as u64,
                needed: u64::from(chunk_size),
                available: frame.end.saturating_sub(payload_start) as u64,
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
                payload_end.saturating_add(1)
            } else if payload_end == root_end {
                // Tolerate missing trailing pad byte at the end of the root RIFF container.
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

        // Extract top-level vrsn chunk (u16 version number).
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
                    reason: format!("LIST chunk size ({chunk_size}) is smaller than 4 bytes (missing list_type)"),
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
                let next_depth = current_depth.saturating_add(1);
                if next_depth > MAX_DEPTH {
                    return Err(ProbeError::LimitExceeded { limit: "max_depth".to_string(), max: MAX_DEPTH });
                }
                stack.push(Frame { cursor: payload_start.saturating_add(4), end: payload_end, depth: next_depth });
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
#[allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
mod tests {
    use super::*;

    /// Synthesizes a minimal valid 12-byte empty RIFF container: `RIFF` + `size=4` + `form`.
    fn synth_minimal_riff(form: &[u8; 4]) -> Vec<u8> {
        let mut v = Vec::with_capacity(12);
        v.extend_from_slice(b"RIFF");
        v.extend_from_slice(&4u32.to_le_bytes());
        v.extend_from_slice(form);
        v
    }

    /// Synthesizes a regular RIFF chunk with 2-byte alignment padding.
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

    /// Synthesizes a `LIST` chunk with the given `list_type` and body.
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
        let res = inspect_riff(&data).expect("minimal 12-byte valid RIFF should parse");
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

        let res = inspect_riff(&riff).expect("multi-chunk RIFF should parse");
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
        // Per Photopea CDR spec, `cmpr` and `stlt` do not contain standard uncompressed sub-chunks.
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

        let res = inspect_riff(&riff).expect("RIFF with cmpr/stlt should skip opaque lists without error");
        assert_eq!(res.chunks.len(), 2);
        assert!(res.chunks[0].opaque_payload);
        assert_eq!(res.chunks[0].list_type.as_deref(), Some("cmpr"));
        assert!(res.chunks[1].opaque_payload);
        assert_eq!(res.chunks[1].list_type.as_deref(), Some("stlt"));
        assert_eq!(res.warnings.len(), 2);
    }

    #[test]
    fn rejects_excessive_list_nesting_depth() {
        // Construct 35 nested LIST chunks (exceeding MAX_DEPTH = 32).
        let mut nested = make_chunk(b"leaf", b"ok");
        for _ in 0..35 {
            nested = make_list(b"sect", &nested);
        }
        let mut riff = Vec::new();
        riff.extend_from_slice(b"RIFF");
        riff.extend_from_slice(&((4 + nested.len()) as u32).to_le_bytes());
        riff.extend_from_slice(b"CDR9");
        riff.extend_from_slice(&nested);

        let err = inspect_riff(&riff).expect_err("LIST nesting exceeding MAX_DEPTH must return an error");
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

        let err = inspect_riff(&riff).expect_err("u32::MAX chunk size should return Truncated");
        assert!(matches!(err, ProbeError::Truncated { .. }));
    }

    #[test]
    fn tolerates_missing_trailing_pad_at_eof_with_warning() {
        let mut riff = Vec::new();
        riff.extend_from_slice(b"RIFF");
        // form(4) + chunk header(8) + odd payload(3) = 15 bytes (missing trailing pad byte)
        riff.extend_from_slice(&15u32.to_le_bytes());
        riff.extend_from_slice(b"CDR7");
        riff.extend_from_slice(b"odd ");
        riff.extend_from_slice(&3u32.to_le_bytes());
        riff.extend_from_slice(b"xyz");

        let res = inspect_riff(&riff).expect("missing trailing pad byte at EOF should be tolerated");
        assert_eq!(res.chunks.len(), 1);
        assert!(res.warnings.contains(&ProbeWarning::MissingTrailingPad { offset: 23 }));
    }

    #[test]
    fn records_warning_on_trailing_bytes_beyond_declared_size() {
        let mut data = synth_minimal_riff(b"CDR7");
        data.extend_from_slice(b"\x00\x00\x00"); // 3 trailing bytes after valid 12-byte RIFF
        let res = inspect_riff(&data).expect("valid RIFF with trailing bytes should succeed with TrailingBytes warning");
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

        let res = inspect_riff(&data).expect("big-endian RIFX container should parse");
        assert_eq!(res.endian, Endian::Big);
        assert_eq!(&res.form, b"CMX1");
        assert_eq!(res.vrsn_chunk_value, Some(100));
    }
}
