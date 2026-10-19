//! ZIP container inspector for modern CDR files (CorelDRAW X4 / v14 and later) (`coralcraft-cdr::zip`).
//!
//! Based on public format documentation (Document Liberation Project / libcdr wiki & PKWARE APPNOTE):
//! - Starting with X4 (v14, 2008), `.cdr` files use a standard ZIP archive outer container
//!   (`PK\x03\x04` + Central Directory + EOCD `PK\x05\x06`).
//! - X4–X5 store the primary RIFF stream at `content/riffData.cdr`.
//! - X6 and later store the primary RIFF root at `content/root.dat` (paired with `content/dataFileList.dat`).
//! - **Anti-false-positive rule**: Never classify a file as CDR based on `PK\x03\x04` alone, or
//!   `.docx`/`.odt`/plain `.zip` files would false-positive. The Central Directory must contain
//!   `content/riffData.cdr` or `content/root.dat`, otherwise `Err(ProbeError::NotCdr)` is returned.

use serde::{Deserialize, Serialize};

use crate::error::{ProbeError, ProbeWarning};
use crate::riff::RiffInspection;

/// Maximum number of Central Directory entries scanned per ZIP archive.
pub const MAX_ZIP_ENTRIES: usize = 10_000;

/// Primary RIFF payload role identified inside a ZIP-based CDR container.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ZipCdrRole {
    /// `content/riffData.cdr` (X4–X5 and compatibility saves).
    RiffDataCdr,
    /// `content/root.dat` (X6+ split RIFF root container).
    RootDat,
}

/// Result of inspecting a ZIP-based CDR container.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZipInspection {
    /// Matched primary payload entry path (`"content/riffData.cdr"` or `"content/root.dat"`).
    pub primary_entry: String,
    /// Classification of the primary payload role.
    pub role: ZipCdrRole,
    /// Whether `"content/dataFileList.dat"` is also present (X6+ external stream index).
    pub has_data_file_list: bool,
    /// Total number of entries in the ZIP Central Directory.
    pub entry_count: usize,
    /// Inner RIFF inspection result when the primary entry uses `Stored` (`method == 0`).
    pub inner_riff: Option<RiffInspection>,
    /// Non-fatal warnings recorded during inspection (e.g., primary entry is Deflate-compressed).
    pub warnings: Vec<ProbeWarning>,
}

fn read_u16_le(bytes: &[u8], offset: usize) -> Option<u16> {
    let s = bytes.get(offset..offset.checked_add(2)?)?;
    Some(u16::from_le_bytes(<[u8; 2]>::try_from(s).ok()?))
}

fn read_u32_le(bytes: &[u8], offset: usize) -> Option<u32> {
    let s = bytes.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_le_bytes(<[u8; 4]>::try_from(s).ok()?))
}

/// Scans backward from the end of the file to locate the End of Central Directory (EOCD `PK\x05\x06`).
fn find_eocd_offset(bytes: &[u8]) -> Option<usize> {
    if bytes.len() < 22 {
        return None;
    }
    let min_offset = bytes.len().saturating_sub(22 + 65_535);
    let mut pos = bytes.len() - 22;
    loop {
        if bytes.get(pos..pos + 4) == Some(b"PK\x05\x06")
            && let Some(comment_len) = read_u16_le(bytes, pos + 20)
            && pos + 22 + usize::from(comment_len) == bytes.len()
        {
            return Some(pos);
        }
        if pos == min_offset {
            break;
        }
        pos -= 1;
    }
    None
}

struct CdEntry {
    name: String,
    flags: u16,
    method: u16,
    compressed_size: u32,
    uncompressed_size: u32,
    local_header_offset: u32,
}

/// Inspects a byte stream starting with `PK\x03\x04` to determine whether it is a modern ZIP-wrapped `.cdr`.
pub fn inspect_zip_cdr(bytes: &[u8]) -> Result<ZipInspection, ProbeError> {
    if bytes.len() < 4 {
        return Err(ProbeError::TooShort { needed: 4, got: bytes.len() });
    }
    if bytes.get(0..4) != Some(b"PK\x03\x04") {
        return Err(ProbeError::NotCdr { reason: "not a ZIP Local File Header magic".to_string() });
    }

    let eocd_pos = find_eocd_offset(bytes).ok_or(ProbeError::Truncated { offset: bytes.len() as u64, needed: 22, available: 0 })?;

    let total_entries = usize::from(
        read_u16_le(bytes, eocd_pos + 10)
            .ok_or(ProbeError::Invalid { offset: eocd_pos as u64, reason: "failed to read EOCD entry count".to_string() })?,
    );

    if total_entries > MAX_ZIP_ENTRIES {
        return Err(ProbeError::LimitExceeded { limit: "max_zip_entries".to_string(), max: MAX_ZIP_ENTRIES });
    }

    let cd_size = read_u32_le(bytes, eocd_pos + 12)
        .ok_or(ProbeError::Invalid { offset: eocd_pos as u64, reason: "failed to read EOCD Central Directory size".to_string() })?
        as usize;
    let cd_offset = read_u32_le(bytes, eocd_pos + 16)
        .ok_or(ProbeError::Invalid { offset: eocd_pos as u64, reason: "failed to read EOCD Central Directory offset".to_string() })?
        as usize;

    let cd_end = cd_offset
        .checked_add(cd_size)
        .ok_or(ProbeError::Invalid { offset: eocd_pos as u64, reason: "Central Directory range arithmetic overflow".to_string() })?;
    if cd_end > eocd_pos {
        return Err(ProbeError::Truncated {
            offset: cd_offset as u64,
            needed: cd_size as u64,
            available: bytes.len().saturating_sub(cd_offset) as u64,
        });
    }

    let mut cursor = cd_offset;
    let mut primary: Option<(ZipCdrRole, CdEntry)> = None;
    let mut has_data_file_list = false;

    for _ in 0..total_entries {
        if cursor + 46 > cd_end {
            return Err(ProbeError::Truncated { offset: cursor as u64, needed: 46, available: cd_end.saturating_sub(cursor) as u64 });
        }
        if bytes.get(cursor..cursor + 4) != Some(b"PK\x01\x02") {
            return Err(ProbeError::Invalid { offset: cursor as u64, reason: "invalid Central Directory entry signature".to_string() });
        }

        let flags = read_u16_le(bytes, cursor + 8).unwrap_or(0);
        let method = read_u16_le(bytes, cursor + 10).unwrap_or(0);
        let compressed_size = read_u32_le(bytes, cursor + 20).unwrap_or(0);
        let uncompressed_size = read_u32_le(bytes, cursor + 24).unwrap_or(0);
        let name_len = usize::from(read_u16_le(bytes, cursor + 28).unwrap_or(0));
        let extra_len = usize::from(read_u16_le(bytes, cursor + 30).unwrap_or(0));
        let comment_len = usize::from(read_u16_le(bytes, cursor + 32).unwrap_or(0));
        let local_header_offset = read_u32_le(bytes, cursor + 42).unwrap_or(0);

        let name_start = cursor + 46;
        let name_end = name_start
            .checked_add(name_len)
            .ok_or(ProbeError::Invalid { offset: cursor as u64, reason: "ZIP entry name length overflow".to_string() })?;
        let next_cursor = name_end
            .checked_add(extra_len)
            .and_then(|v| v.checked_add(comment_len))
            .ok_or(ProbeError::Invalid { offset: cursor as u64, reason: "ZIP Central Directory cursor overflow".to_string() })?;

        if next_cursor > cd_end {
            return Err(ProbeError::Truncated {
                offset: cursor as u64,
                needed: (next_cursor - cursor) as u64,
                available: cd_end.saturating_sub(cursor) as u64,
            });
        }

        let raw_name = bytes.get(name_start..name_end).unwrap_or(&[]);
        let name = String::from_utf8_lossy(raw_name).into_owned();

        // `content/riffData.cdr` takes precedence over `content/root.dat` if both exist.
        if name.eq_ignore_ascii_case("content/dataFileList.dat") {
            has_data_file_list = true;
        } else if name.eq_ignore_ascii_case("content/riffData.cdr") {
            primary = Some((ZipCdrRole::RiffDataCdr, CdEntry { name, flags, method, compressed_size, uncompressed_size, local_header_offset }));
        } else if name.eq_ignore_ascii_case("content/root.dat") && primary.is_none() {
            primary = Some((ZipCdrRole::RootDat, CdEntry { name, flags, method, compressed_size, uncompressed_size, local_header_offset }));
        }

        cursor = next_cursor;
    }

    let Some((role, entry)) = primary else {
        return Err(ProbeError::NotCdr { reason: "ZIP archive does not contain content/riffData.cdr or content/root.dat".to_string() });
    };

    if (entry.flags & 1) != 0 {
        return Err(ProbeError::Invalid {
            offset: u64::from(entry.local_header_offset),
            reason: format!("ZIP entry {} is encrypted and cannot be probed", entry.name),
        });
    }

    let mut warnings = Vec::new();
    let inner_riff = if entry.method == 0 {
        let lfh_offset = entry.local_header_offset as usize;
        if lfh_offset.checked_add(30).is_none_or(|end| end > bytes.len()) {
            return Err(ProbeError::Truncated {
                offset: u64::from(entry.local_header_offset),
                needed: 30,
                available: bytes.len().saturating_sub(lfh_offset) as u64,
            });
        }
        if bytes.get(lfh_offset..lfh_offset + 4) != Some(b"PK\x03\x04") {
            return Err(ProbeError::Invalid {
                offset: u64::from(entry.local_header_offset),
                reason: "invalid Local File Header signature for inner entry".to_string(),
            });
        }
        let lfh_name_len = usize::from(read_u16_le(bytes, lfh_offset + 26).unwrap_or(0));
        let lfh_extra_len = usize::from(read_u16_le(bytes, lfh_offset + 28).unwrap_or(0));
        let data_start = lfh_offset.checked_add(30).and_then(|v| v.checked_add(lfh_name_len)).and_then(|v| v.checked_add(lfh_extra_len)).ok_or(
            ProbeError::Invalid { offset: u64::from(entry.local_header_offset), reason: "Local File Header data offset overflow".to_string() },
        )?;
        if entry.compressed_size != entry.uncompressed_size {
            return Err(ProbeError::Invalid {
                offset: data_start as u64,
                reason: format!("Stored entry compressed_size ({}) != uncompressed_size ({})", entry.compressed_size, entry.uncompressed_size),
            });
        }
        let data_end = data_start
            .checked_add(entry.compressed_size as usize)
            .ok_or(ProbeError::Invalid { offset: data_start as u64, reason: "Stored entry data length overflow".to_string() })?;
        let inner_slice = bytes.get(data_start..data_end).ok_or(ProbeError::Truncated {
            offset: data_start as u64,
            needed: u64::from(entry.compressed_size),
            available: bytes.len().saturating_sub(data_start) as u64,
        })?;
        let riff_res = crate::riff::inspect_riff(inner_slice)?;
        warnings.extend(riff_res.warnings.iter().cloned());
        Some(riff_res)
    } else {
        warnings.push(ProbeWarning::ZipInnerCompressed { entry: entry.name.clone(), compression_method: entry.method });
        None
    };

    Ok(ZipInspection { primary_entry: entry.name, role, has_data_file_list, entry_count: total_entries, inner_riff, warnings })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Synthesizes a minimal valid ZIP archive byte stream in memory (supporting multiple entries and compression methods).
    ///
    /// Handcrafted from the public PKWARE APPNOTE specification: Local File Headers + Central Directory + EOCD.
    pub(crate) fn synth_zip(entries: &[(&str, u16, &[u8])]) -> Vec<u8> {
        let mut buf = Vec::new();
        let mut offsets = Vec::with_capacity(entries.len());

        // 1. Local File Headers + Payloads
        for &(name, method, data) in entries {
            offsets.push(buf.len() as u32);
            let name_bytes = name.as_bytes();
            let len = data.len() as u32;
            buf.extend_from_slice(b"PK\x03\x04");
            buf.extend_from_slice(&20u16.to_le_bytes()); // version needed
            buf.extend_from_slice(&0u16.to_le_bytes()); // flags
            buf.extend_from_slice(&method.to_le_bytes()); // compression method (0=stored, 8=deflate)
            buf.extend_from_slice(&0u16.to_le_bytes()); // mod time
            buf.extend_from_slice(&0u16.to_le_bytes()); // mod date
            buf.extend_from_slice(&0u32.to_le_bytes()); // crc32 (not enforced during structural probe)
            buf.extend_from_slice(&len.to_le_bytes()); // compressed size
            buf.extend_from_slice(&len.to_le_bytes()); // uncompressed size
            buf.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes()); // file name len
            buf.extend_from_slice(&0u16.to_le_bytes()); // extra len
            buf.extend_from_slice(name_bytes);
            buf.extend_from_slice(data);
        }

        let cd_offset = buf.len() as u32;

        // 2. Central Directory Headers
        for (idx, &(name, method, data)) in entries.iter().enumerate() {
            let name_bytes = name.as_bytes();
            let len = data.len() as u32;
            buf.extend_from_slice(b"PK\x01\x02");
            buf.extend_from_slice(&20u16.to_le_bytes()); // version made by
            buf.extend_from_slice(&20u16.to_le_bytes()); // version needed
            buf.extend_from_slice(&0u16.to_le_bytes()); // flags
            buf.extend_from_slice(&method.to_le_bytes()); // compression method
            buf.extend_from_slice(&0u16.to_le_bytes()); // mod time
            buf.extend_from_slice(&0u16.to_le_bytes()); // mod date
            buf.extend_from_slice(&0u32.to_le_bytes()); // crc32
            buf.extend_from_slice(&len.to_le_bytes()); // compressed size
            buf.extend_from_slice(&len.to_le_bytes()); // uncompressed size
            buf.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes()); // file name len
            buf.extend_from_slice(&0u16.to_le_bytes()); // extra len
            buf.extend_from_slice(&0u16.to_le_bytes()); // comment len
            buf.extend_from_slice(&0u16.to_le_bytes()); // disk number start
            buf.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
            buf.extend_from_slice(&0u32.to_le_bytes()); // external attrs
            buf.extend_from_slice(&offsets[idx].to_le_bytes()); // local header offset
            buf.extend_from_slice(name_bytes);
        }

        let cd_size = (buf.len() as u32) - cd_offset;
        let count = entries.len() as u16;

        // 3. End of Central Directory (EOCD)
        buf.extend_from_slice(b"PK\x05\x06");
        buf.extend_from_slice(&0u16.to_le_bytes()); // disk number
        buf.extend_from_slice(&0u16.to_le_bytes()); // disk with CD
        buf.extend_from_slice(&count.to_le_bytes()); // entries on disk
        buf.extend_from_slice(&count.to_le_bytes()); // total entries
        buf.extend_from_slice(&cd_size.to_le_bytes()); // CD size
        buf.extend_from_slice(&cd_offset.to_le_bytes()); // CD offset
        buf.extend_from_slice(&0u16.to_le_bytes()); // comment len
        buf
    }

    #[test]
    fn detects_x4_zip_with_stored_riff_data_cdr() {
        let mut inner_riff = Vec::new();
        inner_riff.extend_from_slice(b"RIFF");
        inner_riff.extend_from_slice(&4u32.to_le_bytes());
        inner_riff.extend_from_slice(b"CDRE"); // X4 (v14)

        let zip_bytes = synth_zip(&[("mimetype", 0, b"application/x-vnd.corel.zcf.draw.document+zip"), ("content/riffData.cdr", 0, &inner_riff)]);

        let res = inspect_zip_cdr(&zip_bytes).expect("ZIP with content/riffData.cdr should succeed");
        assert_eq!(res.primary_entry, "content/riffData.cdr");
        assert_eq!(res.role, ZipCdrRole::RiffDataCdr);
        assert!(!res.has_data_file_list);
        assert_eq!(res.entry_count, 2);
        let inner = res.inner_riff.expect("Stored entry should parse inner RIFF");
        assert_eq!(&inner.form, b"CDRE");
    }

    #[test]
    fn detects_x6_zip_with_root_dat_and_data_file_list() {
        let mut inner_riff = Vec::new();
        inner_riff.extend_from_slice(b"RIFF");
        inner_riff.extend_from_slice(&4u32.to_le_bytes());
        inner_riff.extend_from_slice(b"CDRG"); // X6 (v16)

        let zip_bytes = synth_zip(&[("content/root.dat", 0, &inner_riff), ("content/dataFileList.dat", 0, b"page1.dat\n")]);

        let res = inspect_zip_cdr(&zip_bytes).expect("ZIP with content/root.dat should succeed");
        assert_eq!(res.role, ZipCdrRole::RootDat);
        assert!(res.has_data_file_list);
        assert_eq!(res.inner_riff.expect("should parse inner RIFF").form, *b"CDRG");
    }

    #[test]
    fn reports_warning_when_zip_inner_entry_is_deflated() {
        let zip_bytes = synth_zip(&[("content/riffData.cdr", 8, b"\x01\x02\x03\x04")]);
        let res = inspect_zip_cdr(&zip_bytes).expect("Deflate-compressed CDR ZIP should still identify container during probe");
        assert_eq!(res.role, ZipCdrRole::RiffDataCdr);
        assert!(res.inner_riff.is_none());
        assert!(res.warnings.contains(&ProbeWarning::ZipInnerCompressed { entry: "content/riffData.cdr".to_string(), compression_method: 8 }));
    }

    #[test]
    fn rejects_non_cdr_zip_archive_as_not_cdr() {
        let docx_zip = synth_zip(&[("[Content_Types].xml", 0, b"<Types/>"), ("word/document.xml", 0, b"<w:document/>")]);
        let err = inspect_zip_cdr(&docx_zip).expect_err("non-CDR .docx/.zip must not be misidentified as CDR");
        assert!(matches!(err, ProbeError::NotCdr { .. }));
    }

    #[test]
    fn rejects_truncated_zip_without_eocd() {
        let err = inspect_zip_cdr(b"PK\x03\x04truncated_garbage_data_here").expect_err("truncated ZIP without EOCD must fail");
        assert!(matches!(err, ProbeError::Truncated { .. } | ProbeError::Invalid { .. }));
    }
}
