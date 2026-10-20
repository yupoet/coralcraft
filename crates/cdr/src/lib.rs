//! `coralcraft-cdr`: Clean-room CorelDRAW `.cdr` / `.cmx` file format layer.
//!
//! Three-stage architecture: `probe` (container/version inspection) -> `parse` (object tree)
//! -> `build` (writer, long-term milestone).
//! All inputs are treated as untrusted (hostile) byte streams: public APIs return `Result`
//! and never panic.
//!
//! Clean-room specification sources:
//! - Photopea CDR Format Specification:
//!   <https://github.com/photopea/CDR-specification> (local mirror in `refs/cdr-spec/`)
//! - Kaitai Struct `coreldraw_cdr.ksy`:
//!   <https://formats.kaitai.io/coreldraw_cdr/>
//! - Document Liberation Project `libcdr` public documentation:
//!   <https://wiki.documentfoundation.org/DLP/Libraries/libcdr>

pub mod error;
pub mod inflate;
pub mod parse;
pub mod riff;
pub mod zip;

use serde::{Deserialize, Serialize};

pub use crate::error::{ProbeError, ProbeWarning};
pub use crate::inflate::{MAX_DECOMPRESSED_BYTES, inflate_raw, inflate_zlib};
pub use crate::parse::{ParseOutcome, decode_fild, decode_outl, decompress_cmpr_payload, parse_document, write_cdr7, write_cdrx_zip, write_cmx1};
pub use crate::riff::{ChunkSummary, Endian, RiffInspection};
pub use crate::zip::{ZipCdrRole, ZipInspection, build_stored_zip, crc32_ieee, extract_zip_primary_riff};

/// Current JSON schema version for [`ProbeReport`].
pub const PROBE_SCHEMA_VERSION: u32 = 1;

/// Document family identified by the 4-byte RIFF form-type prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CdrKind {
    /// Primary CorelDRAW document format (`CDR?` or lowercase `cdr?`).
    Cdr,
    /// Corel Presentation Exchange format (`CMX1`).
    Cmx,
}

/// Version metadata combining the form-type 4th ASCII byte and the top-level `vrsn` chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionInfo {
    /// Raw 4th byte of the form-type (e.g., `b'7'` = `0x37`, `b'D'` = `0x44`).
    pub form_version_byte: u8,
    /// Version number (`major * 100`) derived from the form-type 4th byte
    /// (e.g., `' '`/`'3'` -> `300`, `'7'` -> `700`, `'D'` -> `1300`, `'E'` -> `1400`).
    pub form_version: Option<u16>,
    /// 16-bit version number from the top-level `vrsn` chunk, if present.
    pub vrsn_chunk: Option<u16>,
    /// Effective version (`vrsn_chunk` if present, otherwise `form_version`).
    pub effective_version: Option<u16>,
}

/// CDR container generation (covering Legacy `WL`, bare `RIFF`/`RIFX`, and X4+ `ZIP+RIFF`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "container_kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum CdrContainer {
    /// Bare RIFF / RIFX container (v3–X3 `.cdr` and `.cmx`).
    Riff { kind: CdrKind, endian: Endian, form: String, raw_form: [u8; 4], version: VersionInfo },
    /// Modern ZIP container (X4 / v14+, containing `content/riffData.cdr` or `content/root.dat`).
    Zip { primary_entry: String, role: ZipCdrRole, has_data_file_list: bool, entry_count: usize, inner_version: Option<VersionInfo> },
    /// Pre-1992 legacy format (`WL` magic, v1–v2).
    LegacyWl,
}

/// Structured diagnostic report returned by [`probe`] (shared across CLI human output, `--json`, and MCP).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProbeReport {
    /// JSON schema version (always [`PROBE_SCHEMA_VERSION`] = 1).
    pub schema_version: u32,
    /// Total byte length of the inspected input.
    pub file_size: u64,
    /// Identified container generation and version details.
    pub container: CdrContainer,
    /// Extracted RIFF chunk summaries (from bare RIFF or `Stored` inner ZIP RIFF).
    pub chunks: Vec<ChunkSummary>,
    /// Non-fatal specification deviations or warnings.
    pub warnings: Vec<ProbeWarning>,
}

/// Maps the 4th byte of a CDR form-type (ASCII version character) to its `major * 100` version code.
///
/// Per public specifications:
/// - `b' '` (`0x20`) or `b'3'` (`0x33`) -> `Some(300)` (v3)
/// - `b'4'..=b'9'` -> `Some(400..=900)` (v4–v9)
/// - `b'A'..=b'Z'` -> `Some(1000..=3500)` (v10–X3 = `A`–`D`, X4 = `E`=1400, ..., 2025 = `R`=2700)
pub fn decode_cdr_version_char(byte: u8) -> Option<u16> {
    match byte {
        b' ' | b'3' => Some(300),
        b'4'..=b'9' => Some(u16::from(byte.saturating_sub(b'0')).saturating_mul(100)),
        b'A'..=b'Z' => Some(u16::from(byte.saturating_sub(b'A')).saturating_add(10).saturating_mul(100)),
        b'a'..=b'z' => Some(u16::from(byte.saturating_sub(b'a')).saturating_add(10).saturating_mul(100)),
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
            b'1'..=b'9' => Some(u16::from(version_byte.saturating_sub(b'0')).saturating_mul(100)),
            _ => {
                warnings.push(ProbeWarning::UnknownVersionChar { byte: version_byte });
                None
            }
        };
        let effective_version = vrsn_chunk.or(form_version);
        Ok((CdrKind::Cmx, VersionInfo { form_version_byte: version_byte, form_version, vrsn_chunk, effective_version }))
    } else {
        Err(ProbeError::NotCdr { reason: format!("RIFF form-type is not CDR/CMX: {form_str}") })
    }
}

/// Identifies the container generation, version, and chunk summary list of a byte slice.
pub fn probe(bytes: &[u8]) -> Result<ProbeReport, ProbeError> {
    if bytes.len() < 4 {
        return Err(ProbeError::TooShort { needed: 4, got: bytes.len() });
    }

    let file_size = bytes.len() as u64;

    // 1. RIFF / RIFX container (v3–X3 .cdr and .cmx)
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

    // 2. Modern X4+ ZIP container (PK\x03\x04)
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

    // 3. Pre-1992 Legacy WL container (WL + 1B sub-version + 0x00, minimum 12-byte header)
    if bytes.get(0..2) == Some(b"WL") && bytes.get(3).copied() == Some(0x00) {
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

    let head = [
        bytes.first().copied().unwrap_or(0),
        bytes.get(1).copied().unwrap_or(0),
        bytes.get(2).copied().unwrap_or(0),
        bytes.get(3).copied().unwrap_or(0),
    ];
    Err(ProbeError::NotCdr { reason: format!("unknown file header magic {}", crate::riff::format_fourcc(head)) })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing, clippy::arithmetic_side_effects)]
mod tests {
    use super::*;
    use crate::zip::tests::synth_zip;

    /// Synthesizes a minimal valid 12-byte RIFF CDR/CMX container: `RIFF` + `size=4` + 4-byte ASCII `form`.
    fn synth_riff(form: &[u8; 4]) -> Vec<u8> {
        let mut v = Vec::with_capacity(12);
        v.extend_from_slice(b"RIFF");
        v.extend_from_slice(&4u32.to_le_bytes());
        v.extend_from_slice(form);
        v
    }

    #[test]
    fn detects_riff_cdr7_ascii_version() {
        let report = probe(&synth_riff(b"CDR7")).expect("RIFF/CDR7 should be recognized");
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
        let rep_x3 = probe(&synth_riff(b"CDRD")).expect("CDRD (X3) should be recognized");
        if let CdrContainer::Riff { version, .. } = rep_x3.container {
            assert_eq!(version.effective_version, Some(1300));
        } else {
            panic!("expected CdrContainer::Riff");
        }

        let rep_v3 = probe(&synth_riff(b"CDR ")).expect("CDR  (v3) should be recognized");
        if let CdrContainer::Riff { version, .. } = rep_v3.container {
            assert_eq!(version.effective_version, Some(300));
        } else {
            panic!("expected CdrContainer::Riff");
        }
    }

    #[test]
    fn accepts_lowercase_cdr6_with_warning() {
        let rep = probe(&synth_riff(b"cdr6")).expect("lowercase cdr6 should be accepted with warning");
        assert!(rep.warnings.contains(&ProbeWarning::NonCanonicalForm { form: "cdr6".to_string() }));
        if let CdrContainer::Riff { kind, version, .. } = rep.container {
            assert_eq!(kind, CdrKind::Cdr);
            assert_eq!(version.effective_version, Some(600));
        } else {
            panic!("expected CdrContainer::Riff");
        }
    }

    #[test]
    fn detects_riff_and_rifx_cmx1_containers() {
        let rep_le = probe(&synth_riff(b"CMX1")).expect("RIFF/CMX1 should be recognized");
        if let CdrContainer::Riff { kind, endian, version, .. } = rep_le.container {
            assert_eq!(kind, CdrKind::Cmx);
            assert_eq!(endian, Endian::Little);
            assert_eq!(version.form_version, Some(100));
        } else {
            panic!("expected CdrContainer::Riff");
        }

        let mut rifx = Vec::with_capacity(12);
        rifx.extend_from_slice(b"RIFX");
        rifx.extend_from_slice(&4u32.to_be_bytes());
        rifx.extend_from_slice(b"CMX1");
        let rep_be = probe(&rifx).expect("RIFX/CMX1 (Mac big-endian) should be recognized");
        if let CdrContainer::Riff { kind, endian, .. } = rep_be.container {
            assert_eq!(kind, CdrKind::Cmx);
            assert_eq!(endian, Endian::Big);
        } else {
            panic!("expected CdrContainer::Riff");
        }
    }

    #[test]
    fn detects_modern_zip_cdr_container() {
        let inner = synth_riff(b"CDRE"); // X4 = 1400
        let zip_bytes = synth_zip(&[("content/riffData.cdr", 0, &inner)]);
        let rep = probe(&zip_bytes).expect("X4 ZIP container should be recognized");
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
        let rep = probe(v).expect("Legacy WL container should be recognized");
        assert_eq!(rep.container, CdrContainer::LegacyWl);
    }

    #[test]
    fn rejects_non_cdr_riff_and_non_cdr_zip_and_garbage() {
        // 1. RIFF WAVE audio is not CDR
        let wave = synth_riff(b"WAVE");
        assert!(matches!(probe(&wave), Err(ProbeError::NotCdr { .. })));

        // 2. Plain ZIP/DOCX is not CDR
        let docx = synth_zip(&[("word/document.xml", 0, b"<w:document/>")]);
        assert!(matches!(probe(&docx), Err(ProbeError::NotCdr { .. })));

        // 3. PDF file is not CDR
        assert!(matches!(probe(b"%PDF-1.7 garbage"), Err(ProbeError::NotCdr { .. })));

        // 4. Plain text starting with "WL" is not a Legacy WL container
        assert!(matches!(probe(b"WL hello world text"), Err(ProbeError::NotCdr { .. })));
    }

    #[test]
    fn rejects_too_short_input() {
        assert_eq!(probe(b"RI"), Err(ProbeError::TooShort { needed: 4, got: 2 }));
        assert_eq!(probe(b"RIFF\x04\x00"), Err(ProbeError::TooShort { needed: 12, got: 6 }));
    }

    #[test]
    fn hostile_random_mutations_never_panic() {
        // Fixed-seed Xorshift64 applying 10,000 truncations and byte flips across RIFF, ZIP, and WL templates.
        let seeds: [Vec<u8>; 3] = [
            synth_riff(b"CDR7"),
            synth_zip(&[("content/riffData.cdr", 0, &synth_riff(b"CDRE"))]),
            b"WLe\x00\x03\x04\x05\x06\x07\x08\x09\x0A\x0B\x0C".to_vec(),
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
                    // Random truncation
                    let cut = (next_u64() as usize) % (mutated.len() + 1);
                    mutated.truncate(cut);
                }
                1 => {
                    // Random single-byte XOR flip
                    if !mutated.is_empty() {
                        let idx = (next_u64() as usize) % mutated.len();
                        mutated[idx] ^= (next_u64() & 0xFF) as u8;
                    }
                }
                2 => {
                    // Random trailing garbage bytes
                    let extra = (next_u64() % 32) as usize;
                    for _ in 0..extra {
                        mutated.push((next_u64() & 0xFF) as u8);
                    }
                }
                _ => {
                    // Pure random short slice
                    let len = (next_u64() % 64) as usize;
                    mutated.clear();
                    for _ in 0..len {
                        mutated.push((next_u64() & 0xFF) as u8);
                    }
                }
            }
            let _ = probe(&mutated);
            let _ = parse_document(&mutated);
        }
    }

    fn zlib_stored_wrap(raw: &[u8]) -> Vec<u8> {
        let mut out = b"CPng\x01\x00\x04\x00".to_vec();
        out.extend_from_slice(&[0x78, 0x01, 0x01]);
        let len = raw.len() as u16;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(raw);
        out.extend_from_slice(&crate::inflate::adler32(raw).to_be_bytes());
        out
    }

    #[test]
    fn parses_compressed_cmpr_page_layer_and_objects_with_styles_and_fallback() {
        // Construct an inner chunk stream (`Part1`) where chunk sizes are indices into `Part2`:
        // Chunk 0 (`fild`, size index 0 -> 12 bytes): fill_id=1, fill_type=1 (Uniform), model=1 (RGB), Red (255, 0, 0)
        let mut fild_payload = Vec::new();
        fild_payload.extend_from_slice(&1u32.to_le_bytes()); // fill_id = 1
        fild_payload.extend_from_slice(&1u16.to_le_bytes()); // fill_type = 1 (Uniform)
        fild_payload.extend_from_slice(&1u16.to_le_bytes()); // model = 1 (RGB)
        fild_payload.extend_from_slice(&[255, 0, 0, 0]); // R=255, G=0, B=0

        // Chunk 1 (`outl`, size index 1 -> 18 bytes): outl_id=2, flags=0x03 (behind_fill + scale_with_object), width=254000 (72pt), RGB Blue
        let mut outl_payload = Vec::new();
        outl_payload.extend_from_slice(&2u32.to_le_bytes()); // outl_id = 2
        outl_payload.extend_from_slice(&3u32.to_le_bytes()); // flags = behind_fill | scale_with_object
        outl_payload.extend_from_slice(&254_000i32.to_le_bytes()); // 1 inch = 72 pt
        outl_payload.extend_from_slice(&1u16.to_le_bytes()); // model = 1 (RGB)
        outl_payload.extend_from_slice(&[0, 0, 255, 0]);

        // Inside `LIST page` -> `LIST layr` -> `LIST obj `:
        // `bbox` (size index 2 -> 16 bytes): (-127000, 127000) to (127000, -127000)
        let mut bbox_payload = Vec::new();
        bbox_payload.extend_from_slice(&(-127_000i32).to_le_bytes());
        bbox_payload.extend_from_slice(&127_000i32.to_le_bytes());
        bbox_payload.extend_from_slice(&127_000i32.to_le_bytes());
        bbox_payload.extend_from_slice(&(-127_000i32).to_le_bytes());

        // `loda` (size index 3 -> 24 bytes): shape_code=1 (Rectangle) + fill_id=1 + outl_id=2
        let mut loda_payload = Vec::new();
        loda_payload.extend_from_slice(&1u32.to_le_bytes()); // Rectangle
        loda_payload.extend_from_slice(&1u32.to_le_bytes()); // references fill_id=1
        loda_payload.extend_from_slice(&2u32.to_le_bytes()); // references outl_id=2
        loda_payload.extend_from_slice(&[0u8; 12]);

        // Build `LIST obj ` body (index 4)
        let mut obj_body = Vec::new();
        obj_body.extend_from_slice(b"obj ");
        obj_body.extend_from_slice(b"bbox");
        obj_body.extend_from_slice(&2u32.to_le_bytes()); // block_sizes[2] = 16
        obj_body.extend_from_slice(&bbox_payload);
        obj_body.extend_from_slice(b"loda");
        obj_body.extend_from_slice(&3u32.to_le_bytes()); // block_sizes[3] = 24
        obj_body.extend_from_slice(&loda_payload);

        // Build `LIST layr` body (index 5)
        let mut layr_body = Vec::new();
        layr_body.extend_from_slice(b"layr");
        layr_body.extend_from_slice(b"LIST");
        layr_body.extend_from_slice(&4u32.to_le_bytes()); // block_sizes[4]
        layr_body.extend_from_slice(&obj_body);

        // Build `LIST page` body (index 6)
        let mut page_body = Vec::new();
        page_body.extend_from_slice(b"page");
        page_body.extend_from_slice(b"LIST");
        page_body.extend_from_slice(&5u32.to_le_bytes()); // block_sizes[5]
        page_body.extend_from_slice(&layr_body);

        // Build `Part1` stream
        let mut part1_raw = Vec::new();
        part1_raw.extend_from_slice(b"fild");
        part1_raw.extend_from_slice(&0u32.to_le_bytes()); // block_sizes[0]
        part1_raw.extend_from_slice(&fild_payload);
        part1_raw.extend_from_slice(b"outl");
        part1_raw.extend_from_slice(&1u32.to_le_bytes()); // block_sizes[1]
        part1_raw.extend_from_slice(&outl_payload);
        part1_raw.extend_from_slice(b"LIST");
        part1_raw.extend_from_slice(&6u32.to_le_bytes()); // block_sizes[6]
        part1_raw.extend_from_slice(&page_body);

        let block_sizes: [u32; 7] = [
            fild_payload.len() as u32,
            outl_payload.len() as u32,
            bbox_payload.len() as u32,
            loda_payload.len() as u32,
            obj_body.len() as u32,
            layr_body.len() as u32,
            page_body.len() as u32,
        ];
        let mut part2_raw = Vec::new();
        for sz in block_sizes {
            part2_raw.extend_from_slice(&sz.to_le_bytes());
        }

        let cpng1 = zlib_stored_wrap(&part1_raw);
        let cpng2 = zlib_stored_wrap(&part2_raw);

        let mut cmpr_payload = Vec::new();
        cmpr_payload.extend_from_slice(b"cmpr");
        cmpr_payload.extend_from_slice(&(cpng1.len() as u32).to_le_bytes());
        cmpr_payload.extend_from_slice(&(part1_raw.len() as u32).to_le_bytes());
        cmpr_payload.extend_from_slice(&(cpng2.len() as u32).to_le_bytes());
        cmpr_payload.extend_from_slice(&(part2_raw.len() as u32).to_le_bytes());
        cmpr_payload.extend_from_slice(&cpng1);
        cmpr_payload.extend_from_slice(&cpng2);

        let mut riff = Vec::new();
        let riff_size = 4 + 8 + (cmpr_payload.len() as u32) + ((cmpr_payload.len() & 1) as u32);
        riff.extend_from_slice(b"RIFF");
        riff.extend_from_slice(&riff_size.to_le_bytes());
        riff.extend_from_slice(b"CDRD"); // X3 (v1300)
        riff.extend_from_slice(b"LIST");
        riff.extend_from_slice(&(cmpr_payload.len() as u32).to_le_bytes());
        riff.extend_from_slice(&cmpr_payload);
        if cmpr_payload.len() & 1 == 1 {
            riff.push(0);
        }

        let outcome = parse_document(&riff).expect("should parse LIST cmpr CDR document");
        assert_eq!(outcome.document.pages.len(), 1);
        assert_eq!(outcome.document.pages[0].layers.len(), 1);
        assert_eq!(outcome.document.pages[0].layers[0].objects.len(), 1);
        let obj = &outcome.document.pages[0].layers[0].objects[0];
        assert!(matches!(obj.kind, coralcraft_doc::ObjectKind::Rect(_)));
        assert_eq!(obj.fill, Some(coralcraft_doc::FillStyle::uniform(coralcraft_color::Color::rgb8(255, 0, 0))));
        let outl = obj.outline.as_ref().expect("should have outline");
        assert!(outl.behind_fill);
        assert!(outl.scale_with_object);
    }

    #[test]
    fn parses_all_real_world_cdr_samples_when_present() {
        let samples_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../refs/cdr-samples");
        if !samples_dir.is_dir() {
            return;
        }
        let mut stack = vec![samples_dir];
        let mut count = 0usize;
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().and_then(|s| s.to_str()).is_some_and(|ext| ext.eq_ignore_ascii_case("cdr")) {
                    let bytes = std::fs::read(&path).expect("read sample");
                    if probe(&bytes).is_err() {
                        continue;
                    }
                    let outcome = parse_document(&bytes).unwrap_or_else(|e| panic!("failed to parse {}: {e}", path.display()));
                    assert!(!outcome.document.pages.is_empty(), "parsed document for {} must have >= 1 page", path.display());
                    count += 1;
                }
            }
        }
        assert!(count >= 64, "expected at least 64 real-world .cdr samples, parsed {count}");
    }

    #[test]
    fn parse_document_never_panics_on_tiny_declared_size_or_hostile_mutations() {
        // Regression test for RIFF headers where declared_size in 0..4 makes 8 + declared_size < 12
        for declared in 0u32..8 {
            let mut buf = Vec::new();
            buf.extend_from_slice(b"RIFF");
            buf.extend_from_slice(&declared.to_le_bytes());
            buf.extend_from_slice(b"CDR7");
            buf.extend_from_slice(b"vrsn\x02\x00\x00\x00\xdc\x02");
            let _ = parse_document(&buf);
        }

        let mut seed_riff = Vec::new();
        seed_riff.extend_from_slice(b"RIFF\x38\x00\x00\x00CDR7");
        seed_riff.extend_from_slice(b"vrsn\x02\x00\x00\x00\xdc\x02");
        seed_riff.extend_from_slice(b"LIST\x24\x00\x00\x00pageLIST\x18\x00\x00\x00layrLIST\x0c\x00\x00\x00obj bbox\x00\x00\x00\x00");
        let mut rng: u64 = 0xC0DE_2026_1019_9999;
        let mut next_u64 = || {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            rng
        };

        for _ in 0..2_000 {
            let mut mutated = seed_riff.clone();
            let mutations = (next_u64() % 6) + 1;
            for _ in 0..mutations {
                if !mutated.is_empty() {
                    let idx = (next_u64() as usize) % mutated.len();
                    mutated[idx] = (next_u64() & 0xFF) as u8;
                }
            }
            let _ = parse_document(&mutated);
        }
    }

    #[test]
    fn write_cdr7_and_write_cmx1_roundtrip_all_primitives_and_transforms() {
        use coralcraft_color::Color;
        use coralcraft_doc::{Document, FillStyle, ObjectId, ObjectKind, ObjectNode, OutlineStyle};
        use coralcraft_geom::{Affine2, EllipseSpec, F64, Path, Point, PolygonSpec, Rect, RectSpec};

        let mut doc = Document::new_a4_portrait();
        let layer = &mut doc.pages[0].layers[0];

        // 1. Sharp Rectangle with translated Affine2
        let mut rect_node = ObjectNode::new(
            ObjectId(10),
            Some("Rect".to_owned()),
            ObjectKind::Rect(RectSpec::sharp(Rect::new(40.0, 50.0, 160.0, 130.0).unwrap())),
            Some(FillStyle::uniform(Color::rgb8(220, 40, 40))),
            Some(OutlineStyle::with_width(Color::BLACK, F64::new(2.0).unwrap())),
        );
        rect_node.transform = Affine2::translation(12.0, -8.0).unwrap();
        layer.objects.push(rect_node);

        // 2. Full Ellipse with CMYK fill
        layer.objects.push(ObjectNode::new(
            ObjectId(11),
            Some("Ellipse".to_owned()),
            ObjectKind::Ellipse(EllipseSpec::full(Rect::new(200.0, 60.0, 300.0, 160.0).unwrap())),
            Some(FillStyle::uniform(Color::cmyk(0.8, 0.2, 0.0, 0.1).unwrap())),
            None,
        ));

        // 3. Cubic Bezier Path (`L1E\0` 0x1e stream)
        let mut path = Path::new();
        path.move_to(Point::new(50.0, 200.0).unwrap()).unwrap();
        path.line_to(Point::new(150.0, 200.0).unwrap()).unwrap();
        path.cubic_to(Point::new(180.0, 230.0).unwrap(), Point::new(120.0, 280.0).unwrap(), Point::new(60.0, 260.0).unwrap()).unwrap();
        path.close().unwrap();
        layer.objects.push(ObjectNode::new(
            ObjectId(12),
            Some("Curve".to_owned()),
            ObjectKind::Path(path),
            None,
            Some(OutlineStyle::hairline(Color::rgb8(0, 100, 200))),
        ));

        // 4. Star Polygon
        let star = PolygonSpec::star(Point::new(260.0, 260.0).unwrap(), 50.0, 22.0, 5, 0.0).unwrap();
        layer.objects.push(ObjectNode::new(
            ObjectId(13),
            Some("Star".to_owned()),
            ObjectKind::Polygon(star),
            Some(FillStyle::uniform(Color::rgb8(250, 200, 20))),
            None,
        ));

        // 5. Artistic Text
        layer.objects.push(ObjectNode::new(
            ObjectId(14),
            Some("Title".to_owned()),
            ObjectKind::ArtisticText {
                text: "CoralCraft M6".to_owned(),
                font_family: "Sans".to_owned(),
                size_pt: F64::new(24.0).unwrap(),
                position: Point::new(72.0, 360.0).unwrap(),
            },
            Some(FillStyle::uniform(Color::BLACK)),
            None,
        ));

        // Test CDR7 round-trip
        let cdr_bytes = write_cdr7(&doc).expect("write_cdr7");
        let cdr_probe = probe(&cdr_bytes).expect("probe cdr7");
        assert!(matches!(cdr_probe.container, CdrContainer::Riff { kind: CdrKind::Cdr, .. }));
        let cdr_parsed = parse_document(&cdr_bytes).expect("parse cdr7");
        assert_eq!(cdr_parsed.document.pages.len(), 1);
        let objs = &cdr_parsed.document.pages[0].layers[0].objects;
        assert_eq!(objs.len(), 5);
        assert!(matches!(objs[0].kind, ObjectKind::Rect(_)));
        assert!((objs[0].transform.tx.get() - 12.0).abs() < 1e-6);
        assert!(matches!(objs[1].kind, ObjectKind::Ellipse(_)));
        assert!(matches!(objs[2].kind, ObjectKind::Path(_)));
        assert!(matches!(objs[3].kind, ObjectKind::Polygon(_)));
        if let ObjectKind::ArtisticText { text, .. } = &objs[4].kind {
            assert_eq!(text, "CoralCraft M6");
        } else {
            panic!("expected ArtisticText, got {:?}", objs[4].kind);
        }

        // Test CMX1 round-trip
        let cmx_bytes = write_cmx1(&doc).expect("write_cmx1");
        let cmx_probe = probe(&cmx_bytes).expect("probe cmx1");
        assert!(matches!(cmx_probe.container, CdrContainer::Riff { kind: CdrKind::Cmx, .. }));
        let cmx_parsed = parse_document(&cmx_bytes).expect("parse cmx1");
        assert_eq!(cmx_parsed.document.pages[0].layers[0].objects.len(), 5);
    }

    #[test]
    fn write_cdr7_roundtrip_embedded_bitmap() {
        use coralcraft_doc::{Document, ObjectId, ObjectKind, ObjectNode};
        use coralcraft_geom::F64;

        let mut doc = Document::new_a4_portrait();
        let layer = &mut doc.pages[0].layers[0];

        let rgba = vec![255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255];
        layer.objects.push(ObjectNode::new(
            ObjectId(15),
            Some("Embedded Bitmap".to_owned()),
            ObjectKind::Bitmap {
                pixel_width: 2,
                pixel_height: 2,
                dpi_x: F64::new(72.0).unwrap(),
                dpi_y: F64::new(72.0).unwrap(),
                rgba_pixels: rgba.clone(),
            },
            None,
            None,
        ));

        let cdr_bytes = write_cdr7(&doc).expect("write_cdr7");
        let parsed = parse_document(&cdr_bytes).expect("parse cdr7");
        let objs = &parsed.document.pages[0].layers[0].objects;
        assert_eq!(objs.len(), 1);
        if let ObjectKind::Bitmap { pixel_width, pixel_height, rgba_pixels, .. } = &objs[0].kind {
            assert_eq!(*pixel_width, 2);
            assert_eq!(*pixel_height, 2);
            assert_eq!(rgba_pixels, &rgba);
        } else {
            panic!("expected Bitmap, got {:?}", objs[0].kind);
        }
    }

    #[test]
    fn write_cdr7_roundtrip_paragraph_text() {
        use coralcraft_doc::{Document, ObjectId, ObjectKind, ObjectNode, ParagraphTextSpec, TextAlignment};
        use coralcraft_geom::Point;

        let mut doc = Document::new_a4_portrait();
        let spec = ParagraphTextSpec::new(
            Point::new(36.0, 48.0).unwrap(),
            240.0,
            120.0,
            "Multi-column CorelDRAW paragraph text frame with clean-room wrapping.",
            "Arial",
            12.0,
            1.2,
            TextAlignment::Justify,
            2,
            12.0,
        )
        .unwrap();
        doc.pages[0].layers[0].objects.push(ObjectNode::new(
            ObjectId(21),
            Some("Paragraph Frame".to_owned()),
            ObjectKind::ParagraphText(Box::new(spec)),
            None,
            None,
        ));

        let cdr_bytes = write_cdr7(&doc).expect("write_cdr7");
        let parsed = parse_document(&cdr_bytes).expect("parse cdr7");
        let objs = &parsed.document.pages[0].layers[0].objects;
        assert_eq!(objs.len(), 1);
        if let ObjectKind::ParagraphText(pt) = &objs[0].kind {
            assert_eq!(pt.text, "Multi-column CorelDRAW paragraph text frame with clean-room wrapping.");
            assert_eq!(pt.alignment, TextAlignment::Justify);
            assert_eq!(pt.columns, 2);
            assert!((pt.gutter_pt.get() - 12.0).abs() < 1e-3);
        } else {
            panic!("expected ParagraphText, got {:?}", objs[0].kind);
        }
    }

    #[test]
    fn write_cdr7_roundtrip_envelope_effect_chunk() {
        use coralcraft_doc::{Document, ObjectId, ObjectKind, ObjectNode, VectorEffect};
        use coralcraft_geom::{Point, Rect, RectSpec};

        let mut doc = Document::new_a4_portrait();
        let mut node = ObjectNode::new(
            ObjectId(22),
            Some("Envelope Box".to_owned()),
            ObjectKind::Rect(RectSpec::sharp(Rect::new(20.0, 20.0, 120.0, 80.0).unwrap())),
            None,
            None,
        );
        let corners =
            [Point::new(15.0, 18.0).unwrap(), Point::new(130.0, 25.0).unwrap(), Point::new(125.0, 90.0).unwrap(), Point::new(10.0, 82.0).unwrap()];
        node.effects.push(VectorEffect::Envelope { corners });
        doc.pages[0].layers[0].objects.push(node);

        let cdr_bytes = write_cdr7(&doc).expect("write_cdr7");
        let parsed = parse_document(&cdr_bytes).expect("parse cdr7");
        let objs = &parsed.document.pages[0].layers[0].objects;
        assert_eq!(objs.len(), 1);
        assert_eq!(objs[0].effects.len(), 1);
        if let VectorEffect::Envelope { corners: parsed_corners } = &objs[0].effects[0] {
            for i in 0..4 {
                assert!((parsed_corners[i].x.get() - corners[i].x.get()).abs() < 1e-2);
                assert!((parsed_corners[i].y.get() - corners[i].y.get()).abs() < 1e-2);
            }
        } else {
            panic!("expected Envelope effect");
        }
    }

    #[test]
    fn write_cdr7_roundtrip_hierarchical_group_and_x6_data_file_list_m65() {
        use coralcraft_doc::{Document, ObjectId, ObjectKind, ObjectNode};
        use coralcraft_geom::{EllipseSpec, Rect, RectSpec};

        let mut doc = Document::new_a4_portrait();
        let child_a = ObjectNode::new(
            ObjectId(31),
            Some("ChildRect".to_owned()),
            ObjectKind::Rect(RectSpec::sharp(Rect::new(20.0, 20.0, 80.0, 60.0).unwrap())),
            None,
            None,
        );
        let child_b = ObjectNode::new(
            ObjectId(32),
            Some("ChildEllipse".to_owned()),
            ObjectKind::Ellipse(EllipseSpec::full(Rect::new(100.0, 30.0, 160.0, 90.0).unwrap())),
            None,
            None,
        );
        let grp = ObjectNode::new(ObjectId(33), Some("Group".to_owned()), ObjectKind::Group { children: vec![child_a, child_b] }, None, None);
        doc.pages[0].layers[0].objects.push(grp);

        let cdr_bytes = write_cdr7(&doc).expect("write_cdr7 with Group");
        let parsed = parse_document(&cdr_bytes).expect("parse cdr7 with Group");
        let objs = &parsed.document.pages[0].layers[0].objects;
        assert_eq!(objs.len(), 1);
        match &objs[0].kind {
            ObjectKind::Group { children } => {
                assert_eq!(children.len(), 2);
                assert!(matches!(children[0].kind, ObjectKind::Rect(_)));
                assert!(matches!(children[1].kind, ObjectKind::Ellipse(_)));
            }
            other => panic!("expected hierarchical ObjectKind::Group, got {other:?}"),
        }

        // Also verify X6+ `content/root.dat` + `content/dataFileList.dat` + `content/data/page2.dat` extraction
        let mut doc2 = Document::new_a4_portrait();
        doc2.pages[0].layers[0].objects.push(ObjectNode::new(
            ObjectId(41),
            Some("AuxPageRect".to_owned()),
            ObjectKind::Rect(RectSpec::sharp(Rect::new(40.0, 40.0, 140.0, 110.0).unwrap())),
            None,
            None,
        ));
        let aux_riff = write_cdr7(&doc2).unwrap();
        let x6_zip = crate::zip::build_stored_zip(&[
            ("content/root.dat", &cdr_bytes),
            ("content/dataFileList.dat", b"data/page2.dat\n"),
            ("content/data/page2.dat", &aux_riff),
        ]);
        let parsed_x6 = parse_document(&x6_zip).expect("parse X6 multi-stream ZIP");
        assert_eq!(parsed_x6.document.pages.len(), 2, "both root.dat and data/page2.dat pages must be parsed");
    }

    #[test]
    fn write_cmx1_begin_group_end_group_roundtrip_m66() {
        use coralcraft_color::Color;
        use coralcraft_doc::{Document, FillStyle, ObjectId, ObjectKind, ObjectNode, OutlineStyle};
        use coralcraft_geom::{EllipseSpec, F64, Rect, RectSpec};

        let mut doc = Document::new_a4_portrait();
        let child_a = ObjectNode::new(
            ObjectId(51),
            Some("ChildRect".to_owned()),
            ObjectKind::Rect(RectSpec::sharp(Rect::new(30.0, 30.0, 90.0, 70.0).unwrap())),
            Some(FillStyle::uniform(Color::rgb8(210, 45, 60))),
            Some(OutlineStyle::with_width(Color::rgb8(10, 20, 30), F64::new(3.0).unwrap())),
        );
        let child_b = ObjectNode::new(
            ObjectId(52),
            Some("ChildEllipse".to_owned()),
            ObjectKind::Ellipse(EllipseSpec::full(Rect::new(110.0, 40.0, 170.0, 100.0).unwrap())),
            Some(FillStyle::uniform(Color::cmyk(0.5, 0.2, 0.0, 0.1).unwrap())),
            None,
        );
        let grp = ObjectNode::new(ObjectId(53), Some("Group".to_owned()), ObjectKind::Group { children: vec![child_a, child_b] }, None, None);
        doc.pages[0].layers[0].objects.push(grp);

        let cmx_bytes = write_cmx1(&doc).expect("write_cmx1 with Group");
        let parsed = parse_document(&cmx_bytes).expect("parse cmx1 with Group");
        assert_eq!(parsed.document.pages[0].layers[0].objects.len(), 1);
        match &parsed.document.pages[0].layers[0].objects[0].kind {
            ObjectKind::Group { children } => assert_eq!(children.len(), 2),
            other => panic!("expected ObjectKind::Group, got {other:?}"),
        }

        // Also test standalone CMX1 `page` + `rclr`/`rpen`/`rotl` chunks (without trailing `LIST page`)
        let list_pos = cmx_bytes.windows(8).position(|w| &w[0..4] == b"LIST").expect("LIST page in cmx_bytes");
        let mut pure_cmx = cmx_bytes[..list_pos].to_vec();
        let pure_riff_sz = (pure_cmx.len() - 8) as u32;
        pure_cmx[4..8].copy_from_slice(&pure_riff_sz.to_le_bytes());
        let parsed_pure = parse_document(&pure_cmx).expect("parse standalone CMX1 page commands");
        assert_eq!(parsed_pure.document.pages[0].layers[0].objects.len(), 1);
        match &parsed_pure.document.pages[0].layers[0].objects[0].kind {
            ObjectKind::Group { children } => {
                assert_eq!(children.len(), 2);
                assert_eq!(children[0].fill, Some(FillStyle::uniform(Color::rgb8(210, 45, 60))));
                let outl = children[0].outline.as_ref().expect("outline from CMX1 rotl/rpen");
                assert_eq!(outl.color, Color::rgb8(10, 20, 30));
                assert!((outl.width_pt.get() - 3.0).abs() < 1e-2);
                assert_eq!(children[1].fill, Some(FillStyle::uniform(Color::cmyk(0.5, 0.2, 0.0, 0.1).unwrap())));
            }
            other => panic!("expected ObjectKind::Group from CMX1 BeginGroup/EndGroup, got {other:?}"),
        }
    }

    #[test]
    fn write_cdr7_and_decode_outl_dash_pattern_and_arrowheads_m68() {
        use coralcraft_color::Color;
        use coralcraft_doc::{ArrowheadKind, Document, LineCap, LineJoin, ObjectId, ObjectKind, ObjectNode, OutlineStyle};
        use coralcraft_geom::{F64, Rect, RectSpec};

        let mut doc = Document::new_a4_portrait();
        let mut outl = OutlineStyle::with_width(Color::rgb8(20, 120, 220), F64::new(2.0).unwrap());
        outl.cap = LineCap::Round;
        outl.join = LineJoin::Bevel;
        outl.behind_fill = true;
        outl.scale_with_object = true;
        outl.dash_pattern = vec![F64::new(6.0).unwrap(), F64::new(4.0).unwrap()];
        outl.start_arrow = ArrowheadKind::Triangle;
        outl.end_arrow = ArrowheadKind::Diamond;

        doc.pages[0].layers[0].objects.push(ObjectNode::new(
            ObjectId(71),
            Some("DashedArrowBox".to_owned()),
            ObjectKind::Rect(RectSpec::sharp(Rect::new(40.0, 40.0, 140.0, 100.0).unwrap())),
            None,
            Some(outl),
        ));

        let cdr7 = write_cdr7(&doc).expect("write_cdr7");
        let parsed = parse_document(&cdr7).expect("parse_document");
        let decoded_outl = parsed.document.pages[0].layers[0].objects[0].outline.as_ref().expect("decoded outline");
        assert_eq!(decoded_outl.cap, LineCap::Round);
        assert_eq!(decoded_outl.join, LineJoin::Bevel);
        assert!(decoded_outl.behind_fill);
        assert!(decoded_outl.scale_with_object);
        assert_eq!(decoded_outl.dash_pattern, vec![F64::new(6.0).unwrap(), F64::new(4.0).unwrap()]);
        assert_eq!(decoded_outl.start_arrow, ArrowheadKind::Triangle);
        assert_eq!(decoded_outl.end_arrow, ArrowheadKind::Diamond);
    }

    #[test]
    fn write_cmx1_and_parse_pure_cmx_rott_and_rdot_dash_pattern_m69() {
        use coralcraft_color::Color;
        use coralcraft_doc::{Document, LineCap, LineJoin, ObjectId, ObjectKind, ObjectNode, OutlineStyle};
        use coralcraft_geom::{F64, Rect, RectSpec};

        let mut doc = Document::new_a4_portrait();
        let mut outl = OutlineStyle::with_width(Color::rgb8(30, 140, 90), F64::new(2.0).unwrap());
        outl.cap = LineCap::Round;
        outl.join = LineJoin::Bevel;
        outl.behind_fill = true;
        outl.scale_with_object = true;
        outl.dash_pattern = vec![F64::new(6.0).unwrap(), F64::new(4.0).unwrap()];

        doc.pages[0].layers[0].objects.push(ObjectNode::new(
            ObjectId(81),
            Some("CmxDashedBox".to_owned()),
            ObjectKind::Rect(RectSpec::sharp(Rect::new(40.0, 40.0, 140.0, 100.0).unwrap())),
            None,
            Some(outl),
        ));

        let cmx_bytes = write_cmx1(&doc).expect("write_cmx1");
        let list_pos = cmx_bytes.windows(8).position(|w| &w[0..4] == b"LIST").expect("LIST page in cmx_bytes");
        let mut pure_cmx = cmx_bytes[..list_pos].to_vec();
        let pure_riff_sz = (pure_cmx.len() - 8) as u32;
        pure_cmx[4..8].copy_from_slice(&pure_riff_sz.to_le_bytes());

        let parsed_pure = parse_document(&pure_cmx).expect("parse standalone CMX1 with rott and rdot");
        let decoded_outl = parsed_pure.document.pages[0].layers[0].objects[0].outline.as_ref().expect("decoded CMX1 outline");
        assert_eq!(decoded_outl.color, Color::rgb8(30, 140, 90));
        assert!((decoded_outl.width_pt.get() - 2.0).abs() < 1e-2);
        assert_eq!(decoded_outl.cap, LineCap::Round);
        assert_eq!(decoded_outl.join, LineJoin::Bevel);
        assert!(decoded_outl.behind_fill);
        assert!(decoded_outl.scale_with_object);
        assert_eq!(decoded_outl.dash_pattern, vec![F64::new(6.0).unwrap(), F64::new(4.0).unwrap()]);
    }

    #[test]
    fn write_cmx1_and_parse_pure_cmx_fountain_fill_m71() {
        use coralcraft_color::{Color, FountainBlendMode, FountainStop};
        use coralcraft_doc::{Document, FillStyle, FountainGeometry, ObjectId, ObjectKind, ObjectNode, OutlineStyle};
        use coralcraft_geom::{F64, Rect, RectSpec};

        let mut doc = Document::new_a4_portrait();
        let linear_fill = FillStyle::Fountain {
            geometry: FountainGeometry::Linear { angle_deg: F64::new(45.0).unwrap(), edge_pad: F64::new(10.0).unwrap() },
            blend_mode: FountainBlendMode::Direct,
            stops: vec![
                FountainStop::new(0.0, Color::rgb8(255, 10, 20)).unwrap(),
                FountainStop::new(0.5, Color::rgb8(10, 220, 30)).unwrap(),
                FountainStop::new(1.0, Color::rgb8(20, 40, 240)).unwrap(),
            ],
        };
        let radial_fill = FillStyle::Fountain {
            geometry: FountainGeometry::Radial {
                center_x: F64::new(0.6).unwrap(),
                center_y: F64::new(0.4).unwrap(),
                radius: F64::HALF,
                focal_x: F64::new(0.6).unwrap(),
                focal_y: F64::new(0.4).unwrap(),
            },
            blend_mode: FountainBlendMode::RainbowCw,
            stops: vec![FountainStop::new(0.0, Color::rgb8(240, 200, 10)).unwrap(), FountainStop::new(1.0, Color::rgb8(80, 10, 160)).unwrap()],
        };

        doc.pages[0].layers[0].objects.push(ObjectNode::new(
            ObjectId(91),
            Some("CmxLinearFountain".to_owned()),
            ObjectKind::Rect(RectSpec::sharp(Rect::new(40.0, 40.0, 160.0, 120.0).unwrap())),
            Some(linear_fill),
            Some(OutlineStyle::with_width(Color::rgb8(15, 25, 35), F64::new(2.0).unwrap())),
        ));
        doc.pages[0].layers[0].objects.push(ObjectNode::new(
            ObjectId(92),
            Some("CmxRadialFountain".to_owned()),
            ObjectKind::Rect(RectSpec::sharp(Rect::new(180.0, 40.0, 280.0, 140.0).unwrap())),
            Some(radial_fill),
            None,
        ));

        let cmx_bytes = write_cmx1(&doc).expect("write_cmx1");
        let list_pos = cmx_bytes.windows(8).position(|w| &w[0..4] == b"LIST").expect("LIST page in cmx_bytes");
        let mut pure_cmx = cmx_bytes[..list_pos].to_vec();
        let pure_riff_sz = (pure_cmx.len() - 8) as u32;
        pure_cmx[4..8].copy_from_slice(&pure_riff_sz.to_le_bytes());

        let parsed_pure = parse_document(&pure_cmx).expect("parse standalone CMX1 with Fountain fill");
        let objs = &parsed_pure.document.pages[0].layers[0].objects;
        assert_eq!(objs.len(), 2);

        match &objs[0].fill {
            Some(FillStyle::Fountain { geometry: FountainGeometry::Linear { angle_deg, edge_pad }, blend_mode, stops }) => {
                assert!((angle_deg.get() - 45.0).abs() < 1e-3);
                assert!((edge_pad.get() - 10.0).abs() < 1e-3);
                assert_eq!(*blend_mode, FountainBlendMode::Direct);
                assert_eq!(stops.len(), 3);
                assert_eq!(stops[0].color, Color::rgb8(255, 10, 20));
                assert!((stops[1].offset.get() - 0.5).abs() < 1e-3);
                assert_eq!(stops[1].color, Color::rgb8(10, 220, 30));
                assert_eq!(stops[2].color, Color::rgb8(20, 40, 240));
            }
            other => panic!("expected Linear Fountain fill on obj[0], got {other:?}"),
        }
        let outl0 = objs[0].outline.as_ref().expect("outline after fountain fill");
        assert_eq!(outl0.color, Color::rgb8(15, 25, 35));
        assert!((outl0.width_pt.get() - 2.0).abs() < 1e-2);

        match &objs[1].fill {
            Some(FillStyle::Fountain { geometry: FountainGeometry::Radial { center_x, center_y, .. }, blend_mode, stops }) => {
                assert!((center_x.get() - 0.6).abs() < 1e-2);
                assert!((center_y.get() - 0.4).abs() < 1e-2);
                assert_eq!(*blend_mode, FountainBlendMode::RainbowCw);
                assert_eq!(stops.len(), 2);
                assert_eq!(stops[0].color, Color::rgb8(240, 200, 10));
                assert_eq!(stops[1].color, Color::rgb8(80, 10, 160));
            }
            other => panic!("expected Radial Fountain fill on obj[1], got {other:?}"),
        }
    }

    #[test]
    fn write_cdr7_and_cmx1_two_color_pattern_kind_roundtrip_m72() {
        use coralcraft_color::Color;
        use coralcraft_doc::{Document, FillStyle, ObjectId, ObjectKind, ObjectNode, OutlineStyle, PatternKind};
        use coralcraft_geom::{F64, Rect, RectSpec};

        let mut doc = Document::new_a4_portrait();
        let pat_fill = FillStyle::TwoColorPattern {
            kind: PatternKind::DiagonalCrosshatch,
            fg_color: Color::rgb8(180, 20, 40),
            bg_color: Color::rgb8(240, 245, 250),
            tile_size_pt: F64::new(18.0).unwrap(),
        };
        doc.pages[0].layers[0].objects.push(ObjectNode::new(
            ObjectId(101),
            Some("PatternBox".to_owned()),
            ObjectKind::Rect(RectSpec::sharp(Rect::new(30.0, 30.0, 150.0, 110.0).unwrap())),
            Some(pat_fill),
            Some(OutlineStyle::with_width(Color::rgb8(10, 20, 30), F64::new(1.5).unwrap())),
        ));

        let cdr7 = write_cdr7(&doc).expect("write_cdr7");
        let parsed_cdr7 = parse_document(&cdr7).expect("parse_document CDR7");
        match &parsed_cdr7.document.pages[0].layers[0].objects[0].fill {
            Some(FillStyle::TwoColorPattern { kind, fg_color, bg_color, tile_size_pt }) => {
                assert_eq!(*kind, PatternKind::DiagonalCrosshatch);
                assert_eq!(*fg_color, Color::rgb8(180, 20, 40));
                assert_eq!(*bg_color, Color::rgb8(240, 245, 250));
                assert!((tile_size_pt.get() - 18.0).abs() < 1e-2);
            }
            other => panic!("expected TwoColorPattern in CDR7, got {other:?}"),
        }

        let cmx_bytes = write_cmx1(&doc).expect("write_cmx1");
        let list_pos = cmx_bytes.windows(8).position(|w| &w[0..4] == b"LIST").expect("LIST page in cmx_bytes");
        let mut pure_cmx = cmx_bytes[..list_pos].to_vec();
        let pure_riff_sz = (pure_cmx.len() - 8) as u32;
        pure_cmx[4..8].copy_from_slice(&pure_riff_sz.to_le_bytes());

        let parsed_cmx = parse_document(&pure_cmx).expect("parse standalone CMX1 with TwoColorPattern");
        let obj0 = &parsed_cmx.document.pages[0].layers[0].objects[0];
        match &obj0.fill {
            Some(FillStyle::TwoColorPattern { kind, fg_color, bg_color, tile_size_pt }) => {
                assert_eq!(*kind, PatternKind::DiagonalCrosshatch);
                assert_eq!(*fg_color, Color::rgb8(180, 20, 40));
                assert_eq!(*bg_color, Color::rgb8(240, 245, 250));
                assert!((tile_size_pt.get() - 18.0).abs() < 1e-2);
            }
            other => panic!("expected TwoColorPattern in pure CMX1, got {other:?}"),
        }
        let outl0 = obj0.outline.as_ref().expect("outline after TwoColorPattern fill");
        assert_eq!(outl0.color, Color::rgb8(10, 20, 30));
    }

    #[test]
    fn write_cdr7_fountain_edge_pad_center_offset_and_blend_mode_roundtrip_m73() {
        use coralcraft_color::{Color, FountainBlendMode, FountainStop};
        use coralcraft_doc::{Document, FillStyle, FountainGeometry, ObjectId, ObjectKind, ObjectNode};
        use coralcraft_geom::{F64, Rect, RectSpec};

        let mut doc = Document::new_a4_portrait();
        let linear_fill = FillStyle::Fountain {
            geometry: FountainGeometry::Linear { angle_deg: F64::new(30.0).unwrap(), edge_pad: F64::new(15.0).unwrap() },
            blend_mode: FountainBlendMode::RainbowCcw,
            stops: vec![FountainStop::new(0.0, Color::rgb8(250, 20, 40)).unwrap(), FountainStop::new(1.0, Color::rgb8(20, 200, 120)).unwrap()],
        };
        let conical_fill = FillStyle::Fountain {
            geometry: FountainGeometry::Conical {
                center_x: F64::new(0.65).unwrap(),
                center_y: F64::new(0.35).unwrap(),
                angle_deg: F64::new(-45.0).unwrap(),
            },
            blend_mode: FountainBlendMode::RainbowCw,
            stops: vec![FountainStop::new(0.0, Color::rgb8(10, 80, 240)).unwrap(), FountainStop::new(1.0, Color::rgb8(240, 180, 10)).unwrap()],
        };
        doc.pages[0].layers[0].objects.push(ObjectNode::new(
            ObjectId(111),
            Some("CdrLinearPad".to_owned()),
            ObjectKind::Rect(RectSpec::sharp(Rect::new(20.0, 20.0, 120.0, 80.0).unwrap())),
            Some(linear_fill),
            None,
        ));
        doc.pages[0].layers[0].objects.push(ObjectNode::new(
            ObjectId(112),
            Some("CdrConicalCenter".to_owned()),
            ObjectKind::Rect(RectSpec::sharp(Rect::new(140.0, 20.0, 240.0, 120.0).unwrap())),
            Some(conical_fill),
            None,
        ));

        let cdr7 = write_cdr7(&doc).expect("write_cdr7");
        let parsed = parse_document(&cdr7).expect("parse_document");
        let objs = &parsed.document.pages[0].layers[0].objects;
        assert_eq!(objs.len(), 2);

        match &objs[0].fill {
            Some(FillStyle::Fountain { geometry: FountainGeometry::Linear { angle_deg, edge_pad }, blend_mode, .. }) => {
                assert!((angle_deg.get() - 30.0).abs() < 1e-3);
                assert!((edge_pad.get() - 15.0).abs() < 1e-3);
                assert_eq!(*blend_mode, FountainBlendMode::RainbowCcw);
            }
            other => panic!("expected Linear Fountain with edge_pad=15 and RainbowCcw, got {other:?}"),
        }
        match &objs[1].fill {
            Some(FillStyle::Fountain { geometry: FountainGeometry::Conical { center_x, center_y, angle_deg }, blend_mode, .. }) => {
                assert!((center_x.get() - 0.65).abs() < 1e-2);
                assert!((center_y.get() - 0.35).abs() < 1e-2);
                assert!((angle_deg.get() - (-45.0)).abs() < 1e-3);
                assert_eq!(*blend_mode, FountainBlendMode::RainbowCw);
            }
            other => panic!("expected Conical Fountain with center=(0.65,0.35) and RainbowCw, got {other:?}"),
        }
    }

    #[test]
    fn write_cdr7_parametric_rect_ellipse_and_polygon_roundtrip_m74() {
        use coralcraft_doc::{Document, ObjectId, ObjectKind, ObjectNode};
        use coralcraft_geom::{CornerKind, EllipseMode, EllipseSpec, F64, Point, PolygonSpec, Rect, RectSpec};

        let mut doc = Document::new_a4_portrait();
        let rect_spec = RectSpec {
            rect: Rect::new(20.0, 30.0, 180.0, 130.0).unwrap(),
            radii: [F64::new(4.0).unwrap(), F64::new(8.0).unwrap(), F64::new(12.0).unwrap(), F64::new(16.0).unwrap()],
            corner_kinds: [CornerKind::Round, CornerKind::Scalloped, CornerKind::Chamfered, CornerKind::Round],
        };
        let pie_spec = EllipseSpec::pie(Rect::new(200.0, 30.0, 340.0, 150.0).unwrap(), 30.0, 210.0).unwrap();
        let arc_spec = EllipseSpec::arc(Rect::new(200.0, 170.0, 340.0, 290.0).unwrap(), 45.0, 315.0).unwrap();
        let star_spec = PolygonSpec::star(Point::new(100.0, 250.0).unwrap(), 60.0, 25.0, 7, 37.5).unwrap();

        doc.pages[0].layers[0].objects.push(ObjectNode::new(ObjectId(121), Some("ParamRect".to_owned()), ObjectKind::Rect(rect_spec), None, None));
        doc.pages[0].layers[0].objects.push(ObjectNode::new(ObjectId(122), Some("ParamPie".to_owned()), ObjectKind::Ellipse(pie_spec), None, None));
        doc.pages[0].layers[0].objects.push(ObjectNode::new(ObjectId(123), Some("ParamArc".to_owned()), ObjectKind::Ellipse(arc_spec), None, None));
        doc.pages[0].layers[0].objects.push(ObjectNode::new(ObjectId(124), Some("ParamStar".to_owned()), ObjectKind::Polygon(star_spec), None, None));

        let cdr7 = write_cdr7(&doc).expect("write_cdr7");
        let parsed = parse_document(&cdr7).expect("parse_document");
        let objs = &parsed.document.pages[0].layers[0].objects;
        assert_eq!(objs.len(), 4);

        match &objs[0].kind {
            ObjectKind::Rect(r) => {
                assert_eq!(r.radii, rect_spec.radii);
                assert_eq!(r.corner_kinds, rect_spec.corner_kinds);
            }
            other => panic!("expected ObjectKind::Rect, got {other:?}"),
        }
        match &objs[1].kind {
            ObjectKind::Ellipse(e) => {
                assert_eq!(e.mode, EllipseMode::Pie);
                assert!((e.start_deg.get() - 30.0).abs() < 1e-6);
                assert!((e.end_deg.get() - 210.0).abs() < 1e-6);
            }
            other => panic!("expected ObjectKind::Ellipse(Pie), got {other:?}"),
        }
        match &objs[2].kind {
            ObjectKind::Ellipse(e) => {
                assert_eq!(e.mode, EllipseMode::Arc);
                assert!((e.start_deg.get() - 45.0).abs() < 1e-6);
                assert!((e.end_deg.get() - 315.0).abs() < 1e-6);
            }
            other => panic!("expected ObjectKind::Ellipse(Arc), got {other:?}"),
        }
        match &objs[3].kind {
            ObjectKind::Polygon(p) => {
                assert_eq!(p.points, 7);
                assert!((p.rotation_deg.get() - 37.5).abs() < 1e-6);
            }
            other => panic!("expected ObjectKind::Polygon, got {other:?}"),
        }
    }

    #[test]
    fn write_cdr7_transparency_lens_and_powerclip_children_roundtrip_m75() {
        use coralcraft_color::Color;
        use coralcraft_doc::{Document, FillStyle, LensKind, ObjectId, ObjectKind, ObjectNode, TransparencyStyle};
        use coralcraft_geom::{EllipseSpec, F64, Rect, RectSpec};

        let mut doc = Document::new_a4_portrait();
        let mut obj_uniform = ObjectNode::new(
            ObjectId(131),
            Some("UniformTrns".to_owned()),
            ObjectKind::Rect(RectSpec::sharp(Rect::new(20.0, 20.0, 120.0, 80.0).unwrap())),
            Some(FillStyle::uniform(Color::rgb8(200, 40, 60))),
            None,
        );
        obj_uniform.transparency = Some(TransparencyStyle::uniform(0.45));
        obj_uniform.powerclip_children.push(ObjectNode::new(
            ObjectId(132),
            Some("ClipChild".to_owned()),
            ObjectKind::Ellipse(EllipseSpec::full(Rect::new(30.0, 30.0, 90.0, 70.0).unwrap())),
            Some(FillStyle::uniform(Color::rgb8(40, 180, 90))),
            None,
        ));

        let mut obj_fountain_trns = ObjectNode::new(
            ObjectId(133),
            Some("FountainTrns".to_owned()),
            ObjectKind::Rect(RectSpec::sharp(Rect::new(140.0, 20.0, 240.0, 80.0).unwrap())),
            Some(FillStyle::uniform(Color::rgb8(30, 90, 210))),
            None,
        );
        obj_fountain_trns.transparency = Some(TransparencyStyle::fountain_linear(60.0, 0.1, 0.9));

        let mut obj_lens = ObjectNode::new(
            ObjectId(134),
            Some("TintedLens".to_owned()),
            ObjectKind::Ellipse(EllipseSpec::full(Rect::new(20.0, 100.0, 120.0, 200.0).unwrap())),
            None,
            None,
        );
        obj_lens.transparency =
            Some(TransparencyStyle::Lens { kind: LensKind::Tinted { color: Color::rgb8(20, 120, 220), rate: F64::new(0.6).unwrap() } });

        doc.pages[0].layers[0].objects.push(obj_uniform);
        doc.pages[0].layers[0].objects.push(obj_fountain_trns);
        doc.pages[0].layers[0].objects.push(obj_lens);

        let cdr7 = write_cdr7(&doc).expect("write_cdr7");
        let parsed = parse_document(&cdr7).expect("parse_document");
        let objs = &parsed.document.pages[0].layers[0].objects;
        assert_eq!(objs.len(), 3);

        assert_eq!(objs[0].transparency, Some(TransparencyStyle::uniform(0.45)));
        assert_eq!(objs[0].powerclip_children.len(), 1);
        assert!(matches!(objs[0].powerclip_children[0].kind, ObjectKind::Ellipse(_)));

        match &objs[1].transparency {
            Some(TransparencyStyle::Fountain { stops, .. }) => {
                assert_eq!(stops.len(), 2);
            }
            other => panic!("expected Fountain transparency, got {other:?}"),
        }

        match &objs[2].transparency {
            Some(TransparencyStyle::Lens { kind: LensKind::Tinted { color, rate } }) => {
                assert_eq!(*color, Color::rgb8(20, 120, 220));
                assert!((rate.get() - 0.6).abs() < 1e-6);
            }
            other => panic!("expected Tinted Lens transparency, got {other:?}"),
        }
    }

    #[test]
    fn write_cdr7_all_vector_effects_and_object_layer_page_metadata_roundtrip_m76() {
        use coralcraft_color::Color;
        use coralcraft_doc::{Document, FillStyle, LayerRole, ObjectId, ObjectKind, ObjectNode, VectorEffect};
        use coralcraft_geom::{DistortionKind, F64, Path, Point, Rect, RectSpec};

        let mut doc = Document::new_a4_portrait();
        doc.pages[0].name = "Cover Spread".to_owned();
        doc.pages[0].bleed = F64::new(9.0).unwrap();
        doc.pages[0].background = Some(FillStyle::uniform(Color::rgb8(245, 240, 230)));

        let layer = &mut doc.pages[0].layers[0];
        layer.name = "Artwork Layer".to_owned();
        layer.role = LayerRole::MasterAllPages;
        layer.printable = false;
        layer.locked = true;
        layer.master_in_front = true;
        layer.opacity = F64::new(0.85).unwrap();

        let mut spine = Path::new();
        spine.move_to(Point::new(20.0, 20.0).unwrap()).unwrap();
        spine.line_to(Point::new(180.0, 60.0).unwrap()).unwrap();

        let corners =
            [Point::new(25.0, 25.0).unwrap(), Point::new(155.0, 30.0).unwrap(), Point::new(160.0, 115.0).unwrap(), Point::new(20.0, 110.0).unwrap()];

        let mut obj = ObjectNode::new(
            ObjectId(141),
            Some("HeroFX".to_owned()),
            ObjectKind::Rect(RectSpec::sharp(Rect::new(20.0, 20.0, 160.0, 120.0).unwrap())),
            Some(FillStyle::uniform(Color::rgb8(220, 50, 70))),
            None,
        );
        obj.visible = false;
        obj.locked = true;
        obj.opacity = F64::new(0.75).unwrap();
        obj.effects = vec![
            VectorEffect::Contour { steps: 3, offset_pt: F64::new(4.5).unwrap(), inside: true },
            VectorEffect::DropShadow {
                offset_x: F64::new(6.0).unwrap(),
                offset_y: F64::new(-5.0).unwrap(),
                opacity: F64::new(0.6).unwrap(),
                color: Color::rgb8(30, 40, 50),
            },
            VectorEffect::Extrude { depth_pt: F64::new(18.0).unwrap(), angle_deg: F64::new(35.0).unwrap() },
            VectorEffect::Blend { target_id: ObjectId(999), steps: 5, spine: Some(spine.clone()) },
            VectorEffect::ArtisticMediaBrush { start_width_pt: F64::new(2.0).unwrap(), end_width_pt: F64::new(10.0).unwrap() },
            VectorEffect::TextOnPath { spine: spine.clone(), offset_pt: F64::new(3.5).unwrap() },
            VectorEffect::Perspective { corners },
            VectorEffect::Distortion { kind: DistortionKind::Zipper, amplitude: F64::new(8.0).unwrap(), frequency: F64::new(4.0).unwrap() },
        ];
        layer.objects.push(obj);

        let cdr7 = write_cdr7(&doc).expect("write_cdr7");
        let parsed = parse_document(&cdr7).expect("parse_document");
        let p0 = &parsed.document.pages[0];
        assert_eq!(p0.name, "Cover Spread");
        assert!((p0.bleed.get() - 9.0).abs() < 1e-6);
        assert_eq!(p0.background, Some(FillStyle::uniform(Color::rgb8(245, 240, 230))));

        let l0 = &p0.layers[0];
        assert_eq!(l0.name, "Artwork Layer");
        assert_eq!(l0.role, LayerRole::MasterAllPages);
        assert!(!l0.printable);
        assert!(l0.locked);
        assert!(l0.master_in_front);
        assert!((l0.opacity.get() - 0.85).abs() < 1e-6);

        let o0 = &l0.objects[0];
        assert_eq!(o0.name.as_deref(), Some("HeroFX"));
        assert!(!o0.visible);
        assert!(o0.locked);
        assert!((o0.opacity.get() - 0.75).abs() < 1e-6);
        assert_eq!(o0.effects.len(), 8);
        assert_eq!(o0.effects[0], VectorEffect::Contour { steps: 3, offset_pt: F64::new(4.5).unwrap(), inside: true });
        assert_eq!(
            o0.effects[1],
            VectorEffect::DropShadow {
                offset_x: F64::new(6.0).unwrap(),
                offset_y: F64::new(-5.0).unwrap(),
                opacity: F64::new(0.6).unwrap(),
                color: Color::rgb8(30, 40, 50),
            }
        );
        assert_eq!(o0.effects[2], VectorEffect::Extrude { depth_pt: F64::new(18.0).unwrap(), angle_deg: F64::new(35.0).unwrap() });
        match &o0.effects[3] {
            VectorEffect::Blend { target_id, steps, spine: Some(sp) } => {
                assert_eq!(*target_id, ObjectId(999));
                assert_eq!(*steps, 5);
                assert_eq!(sp.elements.len(), 2);
            }
            other => panic!("expected Blend with spine, got {other:?}"),
        }
        assert_eq!(o0.effects[4], VectorEffect::ArtisticMediaBrush { start_width_pt: F64::new(2.0).unwrap(), end_width_pt: F64::new(10.0).unwrap() });
        match &o0.effects[5] {
            VectorEffect::TextOnPath { spine: sp, offset_pt } => {
                assert_eq!(sp.elements.len(), 2);
                assert!((offset_pt.get() - 3.5).abs() < 1e-6);
            }
            other => panic!("expected TextOnPath, got {other:?}"),
        }
        match &o0.effects[6] {
            VectorEffect::Perspective { corners: parsed_c } => {
                for i in 0..4 {
                    assert!((parsed_c[i].x.get() - corners[i].x.get()).abs() < 1e-2);
                    assert!((parsed_c[i].y.get() - corners[i].y.get()).abs() < 1e-2);
                }
            }
            other => panic!("expected Perspective, got {other:?}"),
        }
        assert_eq!(
            o0.effects[7],
            VectorEffect::Distortion { kind: DistortionKind::Zipper, amplitude: F64::new(8.0).unwrap(), frequency: F64::new(4.0).unwrap() }
        );
    }

    #[test]
    fn write_cdr7_document_title_units_master_layers_and_group_metadata_roundtrip_m77() {
        use coralcraft_color::Color;
        use coralcraft_doc::{Document, FillStyle, Layer, LayerId, LayerRole, ObjectId, ObjectKind, ObjectNode};
        use coralcraft_geom::{Affine2, EllipseSpec, F64, Rect, RectSpec, Unit};

        let mut doc = Document::new_a4_portrait();
        doc.title = "Annual Report 2026".to_owned();
        doc.units = Unit::Inch;

        let mut master_fg = Layer::new_normal(LayerId(50), "Header Master");
        master_fg.role = LayerRole::MasterAllPages;
        master_fg.master_in_front = true;
        master_fg.opacity = F64::new(0.9).unwrap();
        master_fg.objects.push(ObjectNode::new(
            ObjectId(151),
            Some("HeaderBar".to_owned()),
            ObjectKind::Rect(RectSpec::sharp(Rect::new(0.0, 0.0, 595.0, 30.0).unwrap())),
            Some(FillStyle::uniform(Color::rgb8(20, 40, 80))),
            None,
        ));
        doc.master_layers[0].objects.push(ObjectNode::new(
            ObjectId(152),
            Some("PasteboardNote".to_owned()),
            ObjectKind::Ellipse(EllipseSpec::full(Rect::new(620.0, 50.0, 680.0, 110.0).unwrap())),
            Some(FillStyle::uniform(Color::rgb8(240, 200, 40))),
            None,
        ));
        doc.master_layers.push(master_fg);

        let child = ObjectNode::new(
            ObjectId(153),
            Some("BadgeRect".to_owned()),
            ObjectKind::Rect(RectSpec::sharp(Rect::new(40.0, 40.0, 120.0, 100.0).unwrap())),
            Some(FillStyle::uniform(Color::rgb8(200, 60, 40))),
            None,
        );
        let mut grp = ObjectNode::new(ObjectId(154), Some("LogoGroup".to_owned()), ObjectKind::Group { children: vec![child] }, None, None);
        grp.visible = false;
        grp.locked = true;
        grp.opacity = F64::new(0.65).unwrap();
        grp.transform = Affine2::translation(25.0, -15.0).unwrap();
        doc.pages[0].layers[0].objects.push(grp);

        let cdr7 = write_cdr7(&doc).expect("write_cdr7");
        let parsed = parse_document(&cdr7).expect("parse_document");
        assert_eq!(parsed.document.title, "Annual Report 2026");
        assert_eq!(parsed.document.units, Unit::Inch);
        assert_eq!(parsed.document.master_layers.len(), 2);
        assert_eq!(parsed.document.master_layers[0].role, LayerRole::Desktop);
        assert_eq!(parsed.document.master_layers[0].objects.len(), 1);
        assert_eq!(parsed.document.master_layers[0].objects[0].name.as_deref(), Some("PasteboardNote"));
        assert_eq!(parsed.document.master_layers[1].name, "Header Master");
        assert_eq!(parsed.document.master_layers[1].role, LayerRole::MasterAllPages);
        assert!(parsed.document.master_layers[1].master_in_front);
        assert!((parsed.document.master_layers[1].opacity.get() - 0.9).abs() < 1e-6);
        assert_eq!(parsed.document.master_layers[1].objects.len(), 1);

        let g0 = &parsed.document.pages[0].layers[0].objects[0];
        assert_eq!(g0.name.as_deref(), Some("LogoGroup"));
        assert!(!g0.visible);
        assert!(g0.locked);
        assert!((g0.opacity.get() - 0.65).abs() < 1e-6);
        assert!((g0.transform.tx.get() - 25.0).abs() < 1e-4);
        assert!((g0.transform.ty.get() - (-15.0)).abs() < 1e-4);
    }

    #[test]
    fn write_cdr7_document_palette_and_object_page_doc_residual_chunks_roundtrip_m78() {
        use coralcraft_color::{Color, Palette, PaletteSwatch};
        use coralcraft_doc::{ChunkTag, Document, FillStyle, ObjectId, ObjectKind, ObjectNode, RawChunk, RawPayload};
        use coralcraft_geom::{Rect, RectSpec};

        let mut doc = Document::new_a4_portrait();
        doc.palette = Palette {
            name: "Custom Brand Swatches".to_owned(),
            guid: Some("brand-2026-guid".to_owned()),
            swatches: vec![
                PaletteSwatch { name: "Brand Crimson".to_owned(), color: Color::rgb8(210, 30, 45) },
                PaletteSwatch { name: "Process Teal".to_owned(), color: Color::cmyk(0.8, 0.1, 0.3, 0.05).unwrap() },
                PaletteSwatch { name: "Golden HSB".to_owned(), color: Color::hsb(45.0, 0.9, 0.95).unwrap() },
                PaletteSwatch { name: "Neutral Gray".to_owned(), color: Color::gray(0.4).unwrap() },
                PaletteSwatch { name: "Metallic Foil".to_owned(), color: Color::spot("Metallic Foil", 0.75, [0.1, 0.2, 0.6, 0.1]).unwrap() },
                PaletteSwatch { name: "Reg Mark".to_owned(), color: Color::Registration },
            ],
        };
        doc.residual.chunks.push(RawChunk {
            tag: ChunkTag(*b"udoc"),
            flags: 0,
            slot: 0,
            payload: RawPayload(vec![0xAA, 0xBB, 0xCC, 0xDD]),
            src_offset: 0,
        });
        doc.pages[0].residual.chunks.push(RawChunk {
            tag: ChunkTag(*b"upag"),
            flags: 0,
            slot: 0,
            payload: RawPayload(vec![0x11, 0x22, 0x33]),
            src_offset: 0,
        });

        let mut node = ObjectNode::new(
            ObjectId(160),
            Some("ResidualBox".to_owned()),
            ObjectKind::Rect(RectSpec::sharp(Rect::new(20.0, 20.0, 120.0, 90.0).unwrap())),
            Some(FillStyle::uniform(Color::rgb8(210, 30, 45))),
            None,
        );
        node.residual.chunks.push(RawChunk { tag: ChunkTag(*b"uobj"), flags: 0, slot: 0, payload: RawPayload(vec![1, 2, 3, 4, 5]), src_offset: 0 });
        doc.pages[0].layers[0].objects.push(node);

        let cdr7 = write_cdr7(&doc).expect("write_cdr7");
        let parsed = parse_document(&cdr7).expect("parse_document");
        assert_eq!(parsed.document.palette, doc.palette);
        assert!(parsed.document.residual.chunks.iter().any(|c| c.tag == ChunkTag(*b"udoc") && c.payload.0 == vec![0xAA, 0xBB, 0xCC, 0xDD]));
        assert!(parsed.document.pages[0].residual.chunks.iter().any(|c| c.tag == ChunkTag(*b"upag") && c.payload.0 == vec![0x11, 0x22, 0x33]));
        let obj0 = &parsed.document.pages[0].layers[0].objects[0];
        assert!(obj0.residual.chunks.iter().any(|c| c.tag == ChunkTag(*b"uobj") && c.payload.0 == vec![1, 2, 3, 4, 5]));
    }

    #[test]
    fn write_cmx1_and_parse_pure_cmx_multipage_and_begin_end_layer_m79() {
        use std::collections::BTreeMap;

        use coralcraft_color::Color;
        use coralcraft_doc::{Document, FillStyle, Layer, LayerId, ObjectId, ObjectKind, ObjectNode, Page, PageId, Residual};
        use coralcraft_geom::{EllipseSpec, F64, Rect, RectSpec, Size};

        let mut doc = Document::new_a4_portrait();
        doc.pages[0].size = Size::new(500.0, 400.0).unwrap();
        doc.pages[0].layers[0].name = "Background".to_owned();
        doc.pages[0].layers[0].locked = true;
        doc.pages[0].layers[0].objects.push(ObjectNode::new(
            ObjectId(170),
            Some("BgBox".to_owned()),
            ObjectKind::Rect(RectSpec::sharp(Rect::new(10.0, 10.0, 110.0, 90.0).unwrap())),
            Some(FillStyle::uniform(Color::rgb8(30, 60, 120))),
            None,
        ));
        let mut layer2 = Layer::new_normal(LayerId(171), "Artwork");
        layer2.objects.push(ObjectNode::new(
            ObjectId(172),
            Some("ArtCircle".to_owned()),
            ObjectKind::Ellipse(EllipseSpec::full(Rect::new(150.0, 100.0, 250.0, 200.0).unwrap())),
            Some(FillStyle::uniform(Color::rgb8(220, 80, 40))),
            None,
        ));
        doc.pages[0].layers.push(layer2);

        let mut notes_layer = Layer::new_normal(LayerId(173), "Notes");
        notes_layer.objects.push(ObjectNode::new(
            ObjectId(174),
            Some("NoteRect".to_owned()),
            ObjectKind::Rect(RectSpec::sharp(Rect::new(20.0, 20.0, 100.0, 80.0).unwrap())),
            Some(FillStyle::uniform(Color::rgb8(40, 160, 80))),
            None,
        ));
        doc.pages.push(Page {
            id: PageId(2),
            name: "Page 2".to_owned(),
            size: Size::new(300.0, 600.0).unwrap(),
            bleed: F64::ZERO,
            background: Some(FillStyle::uniform(Color::WHITE)),
            layers: vec![notes_layer],
            residual: Residual::default(),
            extra: BTreeMap::new(),
        });

        let cmx = write_cmx1(&doc).expect("write_cmx1");
        let list_pos = cmx.windows(8).position(|w| &w[0..4] == b"LIST" && &w[4..8] != b"CMX1").unwrap_or(cmx.len());
        let mut pure_cmx = cmx[..list_pos].to_vec();
        let new_riff_len = (pure_cmx.len() - 8) as u32;
        pure_cmx[4..8].copy_from_slice(&new_riff_len.to_le_bytes());

        let parsed = parse_document(&pure_cmx).expect("parse pure multipage CMX1");
        assert_eq!(parsed.document.pages.len(), 2);
        assert!((parsed.document.pages[0].size.width.get() - 500.0).abs() < 1e-2);
        assert!((parsed.document.pages[0].size.height.get() - 400.0).abs() < 1e-2);
        assert_eq!(parsed.document.pages[0].layers.len(), 2);
        assert_eq!(parsed.document.pages[0].layers[0].name, "Background");
        assert!(parsed.document.pages[0].layers[0].locked);
        assert_eq!(parsed.document.pages[0].layers[0].objects.len(), 1);
        assert_eq!(parsed.document.pages[0].layers[1].name, "Artwork");
        assert_eq!(parsed.document.pages[0].layers[1].objects.len(), 1);

        assert!((parsed.document.pages[1].size.width.get() - 300.0).abs() < 1e-2);
        assert!((parsed.document.pages[1].size.height.get() - 600.0).abs() < 1e-2);
        assert_eq!(parsed.document.pages[1].layers.len(), 1);
        assert_eq!(parsed.document.pages[1].layers[0].name, "Notes");
        assert_eq!(parsed.document.pages[1].layers[0].objects.len(), 1);
    }

    #[test]
    fn write_cdr7_layer_group_residuals_unsupported_object_and_unknown_fill_roundtrip_m80() {
        use coralcraft_color::Color;
        use coralcraft_doc::{
            ChunkTag, Document, FillStyle, ObjectId, ObjectKind, ObjectNode, RawChunk, RawPayload, Residual, TransparencyStyle, UnsupportedObject,
            VectorEffect,
        };
        use coralcraft_geom::{F64, Rect, RectSpec};

        let mut doc = Document::new_a4_portrait();
        doc.pages[0].layers[0].residual.chunks.push(RawChunk {
            tag: ChunkTag(*b"lyrc"),
            flags: 0,
            slot: 1,
            payload: RawPayload(vec![0x11, 0x22, 0x33, 0x44]),
            src_offset: 0,
        });
        doc.master_layers[0].residual.chunks.push(RawChunk {
            tag: ChunkTag(*b"mlyc"),
            flags: 0,
            slot: 1,
            payload: RawPayload(vec![0x55, 0x66]),
            src_offset: 0,
        });

        let unk_fill = FillStyle::Unknown { tag: ChunkTag(*b"txfl"), payload: RawPayload(vec![0xDE, 0xAD, 0xBE, 0xEF, 0x42]) };
        let box_with_unk_fill = ObjectNode::new(
            ObjectId(180),
            Some("UnkFillRect".to_owned()),
            ObjectKind::Rect(RectSpec::sharp(Rect::new(10.0, 20.0, 90.0, 70.0).unwrap())),
            Some(unk_fill.clone()),
            None,
        );
        doc.pages[0].layers[0].objects.push(box_with_unk_fill);

        let unsup_child = ObjectNode::new(
            ObjectId(181),
            Some("UnsupChild".to_owned()),
            ObjectKind::Rect(RectSpec::sharp(Rect::new(15.0, 25.0, 45.0, 55.0).unwrap())),
            Some(FillStyle::uniform(Color::rgb8(100, 150, 200))),
            None,
        );
        let mut unsup_res = Residual::default();
        unsup_res.chunks.push(RawChunk { tag: ChunkTag(*b"usrc"), flags: 0, slot: 0, payload: RawPayload(vec![1, 2, 3]), src_offset: 0 });
        let unsup = ObjectNode::new(
            ObjectId(182),
            Some("OpaqueMesh".to_owned()),
            ObjectKind::Unsupported(UnsupportedObject {
                tag: ChunkTag(*b"mesh"),
                class_hint: Some("GradientMesh".to_owned()),
                payload: RawPayload(vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06]),
                fallback_bounds: Some(Rect::new(50.0, 60.0, 150.0, 140.0).unwrap()),
                absolute_geometry: true,
                children: vec![unsup_child],
                residual: unsup_res,
            }),
            None,
            None,
        );
        doc.pages[0].layers[0].objects.push(unsup);

        let grp_child = ObjectNode::new(
            ObjectId(183),
            Some("GrpChild".to_owned()),
            ObjectKind::Rect(RectSpec::sharp(Rect::new(100.0, 100.0, 180.0, 160.0).unwrap())),
            Some(FillStyle::uniform(Color::rgb8(200, 80, 40))),
            None,
        );
        let pc_child = ObjectNode::new(
            ObjectId(184),
            Some("GrpPowerClip".to_owned()),
            ObjectKind::Rect(RectSpec::sharp(Rect::new(110.0, 110.0, 140.0, 140.0).unwrap())),
            Some(FillStyle::uniform(Color::rgb8(40, 200, 120))),
            None,
        );
        let mut grp = ObjectNode::new(ObjectId(185), Some("RichGroup".to_owned()), ObjectKind::Group { children: vec![grp_child] }, None, None);
        grp.transparency = Some(TransparencyStyle::uniform(0.7));
        grp.effects.push(VectorEffect::Contour { steps: 2, offset_pt: F64::new(3.0).unwrap(), inside: false });
        grp.powerclip_children.push(pc_child);
        grp.residual.chunks.push(RawChunk { tag: ChunkTag(*b"grpc"), flags: 0, slot: 0, payload: RawPayload(vec![9, 8, 7]), src_offset: 0 });
        doc.pages[0].layers[0].objects.push(grp);

        let bytes = write_cdr7(&doc).expect("write_cdr7 M80");
        let parsed = parse_document(&bytes).expect("parse_document M80");

        let lyr = &parsed.document.pages[0].layers[0];
        assert!(lyr.residual.chunks.iter().any(|c| &c.tag.0 == b"lyrc" && c.payload.0 == vec![0x11, 0x22, 0x33, 0x44]));
        assert!(parsed.document.master_layers[0].residual.chunks.iter().any(|c| &c.tag.0 == b"mlyc" && c.payload.0 == vec![0x55, 0x66]));

        assert_eq!(lyr.objects[0].fill, Some(unk_fill));

        let ObjectKind::Unsupported(u) = &lyr.objects[1].kind else {
            panic!("expected ObjectKind::Unsupported, got {:?}", lyr.objects[1].kind);
        };
        assert_eq!(&u.tag.0, b"mesh");
        assert_eq!(u.class_hint.as_deref(), Some("GradientMesh"));
        assert_eq!(u.payload.0, vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06]);
        assert!(u.absolute_geometry);
        assert_eq!(u.fallback_bounds, Some(Rect::new(50.0, 60.0, 150.0, 140.0).unwrap()));
        assert_eq!(u.children.len(), 1);
        assert_eq!(u.children[0].name.as_deref(), Some("UnsupChild"));
        assert!(u.residual.chunks.iter().any(|c| &c.tag.0 == b"usrc" && c.payload.0 == vec![1, 2, 3]));

        let p_grp = &lyr.objects[2];
        assert_eq!(p_grp.name.as_deref(), Some("RichGroup"));
        assert_eq!(p_grp.transparency, Some(TransparencyStyle::uniform(0.7)));
        assert_eq!(p_grp.effects.len(), 1);
        assert_eq!(p_grp.powerclip_children.len(), 1);
        assert_eq!(p_grp.powerclip_children[0].name.as_deref(), Some("GrpPowerClip"));
        assert!(p_grp.residual.chunks.iter().any(|c| &c.tag.0 == b"grpc" && c.payload.0 == vec![9, 8, 7]));
    }

    #[test]
    fn write_cmx1_rclr_color_models_lens_transparency_and_shape_metadata_roundtrip_m81() {
        use coralcraft_color::Color;
        use coralcraft_doc::{Document, FillStyle, LensKind, ObjectId, ObjectKind, ObjectNode, ParagraphTextSpec, TextAlignment, TransparencyStyle};
        use coralcraft_geom::{Affine2, CornerKind, EllipseMode, EllipseSpec, F64, Point, PolygonSpec, Rect, RectSpec};

        let mut doc = Document::new_a4_portrait();
        let page = doc.pages.first_mut().expect("page 0");
        let layer = page.layers.first_mut().expect("layer 0");

        // 1. Parametric Rect with per-corner kinds, HSB fill, Uniform transparency, and transform/metadata
        let mut r_node = ObjectNode::new(
            ObjectId(1),
            Some("CmxRoundedRect".to_owned()),
            ObjectKind::Rect(RectSpec {
                rect: Rect::new(40.0, 50.0, 180.0, 130.0).unwrap(),
                radii: [F64::new(10.0).unwrap(), F64::new(15.0).unwrap(), F64::new(20.0).unwrap(), F64::new(5.0).unwrap()],
                corner_kinds: [CornerKind::Round, CornerKind::Scalloped, CornerKind::Chamfered, CornerKind::Round],
            }),
            Some(FillStyle::uniform(Color::hsb(210.0, 0.75, 0.85).unwrap())),
            None,
        );
        r_node.visible = false;
        r_node.locked = true;
        r_node.opacity = F64::new(0.8).unwrap();
        r_node.transform = Affine2::translation(12.5, -7.5).unwrap();
        r_node.transparency = Some(TransparencyStyle::uniform(0.65));
        layer.objects.push(r_node);

        // 2. Parametric Pie Ellipse with Spot color fill and Tinted Lens transparency
        let spot_col = Color::spot("PANTONE 185 C", 0.8, [0.0, 0.91, 0.76, 0.0]).unwrap();
        let mut e_node = ObjectNode::new(
            ObjectId(2),
            Some("CmxPieEllipse".to_owned()),
            ObjectKind::Ellipse(EllipseSpec {
                rect: Rect::new(60.0, 160.0, 180.0, 260.0).unwrap(),
                start_deg: F64::new(30.0).unwrap(),
                end_deg: F64::new(240.0).unwrap(),
                mode: EllipseMode::Pie,
            }),
            Some(FillStyle::uniform(spot_col.clone())),
            None,
        );
        e_node.transparency =
            Some(TransparencyStyle::Lens { kind: LensKind::Tinted { color: Color::gray(0.3).unwrap(), rate: F64::new(0.45).unwrap() } });
        layer.objects.push(e_node);

        // 3. Star Polygon with Registration fill and Magnify Lens
        let mut p_node = ObjectNode::new(
            ObjectId(3),
            Some("CmxStar".to_owned()),
            ObjectKind::Polygon(PolygonSpec::star(Point::new(300.0, 200.0).unwrap(), 50.0, 22.0, 6, 15.0).unwrap()),
            Some(FillStyle::uniform(Color::Registration)),
            None,
        );
        p_node.transparency = Some(TransparencyStyle::Lens { kind: LensKind::Magnify { factor: F64::new(2.5).unwrap() } });
        layer.objects.push(p_node);

        // 4. ArtisticText and ParagraphText inside a transformed Group
        let atxt = ObjectNode::new(
            ObjectId(4),
            Some("CmxHeadline".to_owned()),
            ObjectKind::ArtisticText {
                text: "CMX Headline".to_owned(),
                font_family: "Arial".to_owned(),
                size_pt: F64::new(20.0).unwrap(),
                position: Point::new(80.0, 320.0).unwrap(),
            },
            Some(FillStyle::uniform(Color::gray(0.2).unwrap())),
            None,
        );
        let ptxt_spec = ParagraphTextSpec::new(
            Point::new(80.0, 360.0).unwrap(),
            200.0,
            90.0,
            "CMX body paragraph",
            "Times",
            12.0,
            1.25,
            TextAlignment::Center,
            2,
            10.0,
        )
        .unwrap();
        let ptxt = ObjectNode::new(
            ObjectId(5),
            Some("CmxParagraph".to_owned()),
            ObjectKind::ParagraphText(Box::new(ptxt_spec.clone())),
            Some(FillStyle::uniform(Color::cmyk(0.1, 0.2, 0.3, 0.4).unwrap())),
            None,
        );
        let mut grp = ObjectNode::new(ObjectId(6), Some("CmxTextGroup".to_owned()), ObjectKind::Group { children: vec![atxt, ptxt] }, None, None);
        grp.opacity = F64::new(0.9).unwrap();
        grp.transform = Affine2::translation(5.0, 10.0).unwrap();
        layer.objects.push(grp);

        let bytes = write_cmx1(&doc).expect("write_cmx1 M81");
        let parsed = parse_document(&bytes).expect("parse_document M81");
        let p_lyr = &parsed.document.pages[0].layers[0];
        assert_eq!(p_lyr.objects.len(), 4);

        let o0 = &p_lyr.objects[0];
        assert_eq!(o0.name.as_deref(), Some("CmxRoundedRect"));
        assert!(!o0.visible);
        assert!(o0.locked);
        assert!((o0.opacity.get() - 0.8).abs() < 1e-6);
        assert_eq!(o0.transform, Affine2::translation(12.5, -7.5).unwrap());
        assert_eq!(o0.fill, Some(FillStyle::uniform(Color::hsb(210.0, 0.75, 0.85).unwrap())));
        assert_eq!(o0.transparency, Some(TransparencyStyle::uniform(0.65)));
        let ObjectKind::Rect(rs) = &o0.kind else {
            panic!("expected ObjectKind::Rect, got {:?}", o0.kind);
        };
        assert_eq!(rs.corner_kinds, [CornerKind::Round, CornerKind::Scalloped, CornerKind::Chamfered, CornerKind::Round]);
        assert!((rs.radii[1].get() - 15.0).abs() < 1e-6);

        let o1 = &p_lyr.objects[1];
        assert_eq!(o1.name.as_deref(), Some("CmxPieEllipse"));
        assert_eq!(o1.fill, Some(FillStyle::uniform(spot_col)));
        assert_eq!(
            o1.transparency,
            Some(TransparencyStyle::Lens { kind: LensKind::Tinted { color: Color::gray(0.3).unwrap(), rate: F64::new(0.45).unwrap() } })
        );
        let ObjectKind::Ellipse(es) = &o1.kind else {
            panic!("expected ObjectKind::Ellipse, got {:?}", o1.kind);
        };
        assert_eq!(es.mode, EllipseMode::Pie);
        assert!((es.start_deg.get() - 30.0).abs() < 1e-6);
        assert!((es.end_deg.get() - 240.0).abs() < 1e-6);

        let o2 = &p_lyr.objects[2];
        assert_eq!(o2.name.as_deref(), Some("CmxStar"));
        assert_eq!(o2.fill, Some(FillStyle::uniform(Color::Registration)));
        assert_eq!(o2.transparency, Some(TransparencyStyle::Lens { kind: LensKind::Magnify { factor: F64::new(2.5).unwrap() } }));
        let ObjectKind::Polygon(ps) = &o2.kind else {
            panic!("expected ObjectKind::Polygon, got {:?}", o2.kind);
        };
        assert_eq!(ps.points, 6);
        assert!((ps.rotation_deg.get() - 15.0).abs() < 1e-6);

        let o3 = &p_lyr.objects[3];
        assert_eq!(o3.name.as_deref(), Some("CmxTextGroup"));
        assert!((o3.opacity.get() - 0.9).abs() < 1e-6);
        assert_eq!(o3.transform, Affine2::translation(5.0, 10.0).unwrap());
        let ObjectKind::Group { children } = &o3.kind else {
            panic!("expected ObjectKind::Group, got {:?}", o3.kind);
        };
        assert_eq!(children.len(), 2);
        assert_eq!(children[0].fill, Some(FillStyle::uniform(Color::gray(0.2).unwrap())));
        let ObjectKind::ArtisticText { text, font_family, .. } = &children[0].kind else {
            panic!("expected ArtisticText, got {:?}", children[0].kind);
        };
        assert_eq!(text, "CMX Headline");
        assert_eq!(font_family, "Arial");
        let ObjectKind::ParagraphText(pt) = &children[1].kind else {
            panic!("expected ParagraphText, got {:?}", children[1].kind);
        };
        assert_eq!(pt.text, ptxt_spec.text);
        assert_eq!(pt.alignment, TextAlignment::Center);
        assert_eq!(pt.columns, 2);
    }
}
