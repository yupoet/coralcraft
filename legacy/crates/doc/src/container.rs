//! Dual-mode `.coralcraft` container (`save_json`, `save_zip`, `load_bytes`) with
//! content-addressed SHA-256 `blobs/<id>.bin` deduplication and deterministic ZIP output.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::model::{CURRENT_SCHEMA, DocError, Document, SchemaVersion};
use crate::style::{decode_hex, encode_hex};

/// Payloads of at least 64 bytes are externalized into `blobs/<sha256>.bin` in ZIP mode.
pub const BLOB_THRESHOLD: usize = 64;

/// Maximum uncompressed size of a single ZIP entry (`256 MiB`).
const MAX_ZIP_ENTRY_BYTES: usize = 256 * 1024 * 1024;

/// Maximum total uncompressed size across all entries in a `.coralcraft` ZIP (`512 MiB`).
const MAX_ZIP_TOTAL_BYTES: usize = 512 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Manifest {
    format: String,
    version: SchemaVersion,
    document: String,
    payload_threshold: usize,
    blobs: Vec<ManifestBlob>,
    document_sha256: String,
    #[serde(default)]
    extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct ManifestBlob {
    id: String,
    len: u64,
}

/// Serializes `doc` in single-file compact JSON mode.
///
/// Pure and deterministic: does not mutate `doc`, regenerate IDs, or reorder fields.
pub fn save_json(doc: &Document) -> Result<Vec<u8>, DocError> {
    validate_no_reserved_extra_keys(doc)?;
    serde_json::to_vec(doc).map_err(|e| DocError::Json(e.to_string()))
}

/// Serializes `doc` in deterministic ZIP container mode (`manifest.json` + `document.json` + sorted `blobs/<sha256>.bin`).
///
/// Any `RawPayload` of length `>= 64` bytes is stored once in `blobs/<sha256>.bin` and referenced via
/// `{"$payload": {"enc": "blob", "id": "<64-char sha256>", "len": N}}`.
pub fn save_zip(doc: &Document) -> Result<Vec<u8>, DocError> {
    validate_no_reserved_extra_keys(doc)?;
    let mut tree = serde_json::to_value(doc).map_err(|e| DocError::Json(e.to_string()))?;
    let mut blobs: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    externalize_payloads(&mut tree, &mut blobs)?;

    let doc_json_bytes = serde_json::to_vec(&tree).map_err(|e| DocError::Json(e.to_string()))?;
    let doc_sha256 = sha256_hex(&doc_json_bytes);

    let manifest = Manifest {
        format: "coralcraft".to_owned(),
        version: doc.schema,
        document: "document.json".to_owned(),
        payload_threshold: BLOB_THRESHOLD,
        blobs: blobs.iter().map(|(id, data)| ManifestBlob { id: id.clone(), len: data.len() as u64 }).collect(),
        document_sha256: doc_sha256,
        extra: BTreeMap::new(),
    };
    let manifest_bytes = serde_json::to_vec(&manifest).map_err(|e| DocError::Json(e.to_string()))?;

    let mut entries: Vec<(String, Vec<u8>)> = Vec::with_capacity(2usize.saturating_add(blobs.len()));
    entries.push(("manifest.json".to_owned(), manifest_bytes));
    entries.push(("document.json".to_owned(), doc_json_bytes));
    for (id, data) in blobs {
        entries.push((format!("blobs/{id}.bin"), data));
    }

    Ok(write_deterministic_stored_zip(&entries))
}

/// Loads a [`Document`] from `.coralcraft` bytes, automatically detecting single-file JSON (`{`)
/// or ZIP container (`PK\x03\x04`) mode.
pub fn load_bytes(bytes: &[u8]) -> Result<Document, DocError> {
    if bytes.starts_with(b"PK\x03\x04") {
        return load_zip_bytes(bytes);
    }
    let trimmed = strip_bom_and_ws(bytes);
    if trimmed.starts_with(b"{") {
        let raw_tree: Value = serde_json::from_slice(trimmed).map_err(|e| DocError::Json(e.to_string()))?;
        check_tree_no_unexpected_payload_keys(&raw_tree)?;
        let mut doc: Document = serde_json::from_value(raw_tree).map_err(|e| DocError::Json(e.to_string()))?;
        if doc.schema.major != CURRENT_SCHEMA.major {
            return Err(DocError::UnsupportedVersion { found: doc.schema.major, expected: CURRENT_SCHEMA.major });
        }
        validate_no_reserved_extra_keys(&doc)?;
        doc.validate()?;
        return Ok(doc);
    }
    Err(DocError::BadMagic)
}

/// Lists the sorted `blobs/<id>.bin` entry names inside a `.coralcraft` ZIP archive (for testing deduplication).
pub fn list_zip_blobs(bytes: &[u8]) -> Result<Vec<String>, DocError> {
    let entries = read_stored_zip(bytes)?;
    Ok(entries.keys().filter(|k| k.starts_with("blobs/")).cloned().collect())
}

fn strip_bom_and_ws(bytes: &[u8]) -> &[u8] {
    let b = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    let idx = b.iter().position(|c| !c.is_ascii_whitespace()).unwrap_or(b.len());
    b.get(idx..).unwrap_or_default()
}

fn validate_no_reserved_extra_keys(doc: &Document) -> Result<(), DocError> {
    if doc.extra.contains_key("$payload") {
        return Err(DocError::ReservedPayloadKey);
    }
    for page in &doc.pages {
        if page.extra.contains_key("$payload") {
            return Err(DocError::ReservedPayloadKey);
        }
        for layer in &page.layers {
            check_layer_extra(layer)?;
        }
    }
    for layer in &doc.master_layers {
        check_layer_extra(layer)?;
    }
    Ok(())
}

fn check_layer_extra(layer: &crate::model::Layer) -> Result<(), DocError> {
    if layer.extra.contains_key("$payload") {
        return Err(DocError::ReservedPayloadKey);
    }
    for obj in &layer.objects {
        check_object_extra(obj)?;
    }
    Ok(())
}

fn check_object_extra(obj: &crate::model::ObjectNode) -> Result<(), DocError> {
    if obj.extra.contains_key("$payload") {
        return Err(DocError::ReservedPayloadKey);
    }
    for pc in &obj.powerclip_children {
        check_object_extra(pc)?;
    }
    match &obj.kind {
        crate::model::ObjectKind::Group { children } => {
            for ch in children {
                check_object_extra(ch)?;
            }
        }
        crate::model::ObjectKind::Unsupported(u) => {
            for ch in &u.children {
                check_object_extra(ch)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn check_tree_no_unexpected_payload_keys(val: &Value) -> Result<(), DocError> {
    match val {
        Value::Object(map) => {
            if map.contains_key("$payload") && map.len() > 1 {
                return Err(DocError::ReservedPayloadKey);
            }
            for v in map.values() {
                check_tree_no_unexpected_payload_keys(v)?;
            }
        }
        Value::Array(arr) => {
            for v in arr {
                check_tree_no_unexpected_payload_keys(v)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn externalize_payloads(val: &mut Value, blobs: &mut BTreeMap<String, Vec<u8>>) -> Result<(), DocError> {
    match val {
        Value::Object(map) => {
            if map.len() == 1
                && let Some(Value::Object(inner)) = map.get_mut("$payload")
                && inner.get("enc").and_then(Value::as_str) == Some("hex")
                && let Some(hex_str) = inner.get("data").and_then(Value::as_str)
            {
                let raw = decode_hex(hex_str).map_err(DocError::Json)?;
                if raw.len() >= BLOB_THRESHOLD {
                    let id = sha256_hex(&raw);
                    let len = raw.len() as u64;
                    blobs.entry(id.clone()).or_insert(raw);
                    inner.clear();
                    inner.insert("enc".to_owned(), Value::String("blob".to_owned()));
                    inner.insert("id".to_owned(), Value::String(id));
                    inner.insert("len".to_owned(), Value::Number(len.into()));
                }
                return Ok(());
            }
            for v in map.values_mut() {
                externalize_payloads(v, blobs)?;
            }
        }
        Value::Array(arr) => {
            for v in arr {
                externalize_payloads(v, blobs)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn internalize_payloads(val: &mut Value, entries: &BTreeMap<String, Vec<u8>>) -> Result<(), DocError> {
    match val {
        Value::Object(map) => {
            if map.len() == 1
                && let Some(Value::Object(inner)) = map.get_mut("$payload")
                && inner.get("enc").and_then(Value::as_str) == Some("blob")
            {
                let id = inner
                    .get("id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| DocError::InvalidZipContainer("blob ref missing id".to_owned()))?
                    .to_owned();
                let expected_len =
                    inner.get("len").and_then(Value::as_u64).ok_or_else(|| DocError::InvalidZipContainer("blob ref missing len".to_owned()))?;

                if id.len() != 64 || !id.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) {
                    return Err(DocError::InvalidZipContainer(format!("invalid blob id `{id}`")));
                }

                let blob_path = format!("blobs/{id}.bin");
                let Some(blob_bytes) = entries.get(&blob_path) else {
                    return Err(DocError::CorruptBlob { id, reason: "referenced blob entry missing from ZIP".to_owned() });
                };

                if (blob_bytes.len() as u64) != expected_len {
                    return Err(DocError::CorruptBlob {
                        id,
                        reason: format!("blob length mismatch: expected {expected_len}, found {}", blob_bytes.len()),
                    });
                }

                let actual_hash = sha256_hex(blob_bytes);
                if actual_hash != id {
                    return Err(DocError::CorruptBlob { id, reason: format!("SHA-256 mismatch (computed {actual_hash})") });
                }

                let hex_data = encode_hex(blob_bytes);
                inner.clear();
                inner.insert("enc".to_owned(), Value::String("hex".to_owned()));
                inner.insert("data".to_owned(), Value::String(hex_data));
                return Ok(());
            }
            for v in map.values_mut() {
                internalize_payloads(v, entries)?;
            }
        }
        Value::Array(arr) => {
            for v in arr {
                internalize_payloads(v, entries)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn load_zip_bytes(bytes: &[u8]) -> Result<Document, DocError> {
    let entries = read_stored_zip(bytes)?;
    let Some(manifest_bytes) = entries.get("manifest.json") else {
        return Err(DocError::InvalidZipContainer("missing manifest.json".to_owned()));
    };
    let manifest: Manifest = serde_json::from_slice(manifest_bytes).map_err(|e| DocError::Json(e.to_string()))?;
    if manifest.format != "coralcraft" {
        return Err(DocError::InvalidZipContainer(format!("unexpected manifest format `{}`", manifest.format)));
    }
    if manifest.version.major != CURRENT_SCHEMA.major {
        return Err(DocError::UnsupportedVersion { found: manifest.version.major, expected: CURRENT_SCHEMA.major });
    }

    let Some(doc_bytes) = entries.get(&manifest.document) else {
        return Err(DocError::InvalidZipContainer(format!("missing document entry `{}`", manifest.document)));
    };
    if sha256_hex(doc_bytes) != manifest.document_sha256 {
        return Err(DocError::InvalidZipContainer("document.json SHA-256 digest mismatch".to_owned()));
    }

    // Verify every listed manifest blob exists and has matching hash & length
    for b in &manifest.blobs {
        let path = format!("blobs/{}.bin", b.id);
        let Some(data) = entries.get(&path) else {
            return Err(DocError::CorruptBlob { id: b.id.clone(), reason: "blob listed in manifest is missing from ZIP".to_owned() });
        };
        if (data.len() as u64) != b.len || sha256_hex(data) != b.id {
            return Err(DocError::CorruptBlob { id: b.id.clone(), reason: "manifest blob failed SHA-256 or length verification".to_owned() });
        }
    }

    let mut tree: Value = serde_json::from_slice(doc_bytes).map_err(|e| DocError::Json(e.to_string()))?;
    check_tree_no_unexpected_payload_keys(&tree)?;
    internalize_payloads(&mut tree, &entries)?;

    let mut doc: Document = serde_json::from_value(tree).map_err(|e| DocError::Json(e.to_string()))?;
    if doc.schema.major != CURRENT_SCHEMA.major {
        return Err(DocError::UnsupportedVersion { found: doc.schema.major, expected: CURRENT_SCHEMA.major });
    }
    validate_no_reserved_extra_keys(&doc)?;
    doc.validate()?;
    Ok(doc)
}

/// Writes a deterministic Stored (`method = 0`) ZIP archive with fixed MS-DOS timestamps (`1980-01-01 00:00:00`).
fn write_deterministic_stored_zip(entries: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central_dir = Vec::new();

    for (name, data) in entries {
        let local_offset = out.len() as u32;
        let name_bytes = name.as_bytes();
        let crc = crc32(data);
        let size = data.len() as u32;

        // Local File Header (PK\x03\x04, 30 bytes + filename)
        out.extend_from_slice(b"PK\x03\x04");
        out.extend_from_slice(&20_u16.to_le_bytes()); // version needed (2.0)
        out.extend_from_slice(&0_u16.to_le_bytes()); // flags
        out.extend_from_slice(&0_u16.to_le_bytes()); // method 0 = Stored
        out.extend_from_slice(&0_u16.to_le_bytes()); // mod time 00:00:00
        out.extend_from_slice(&0x0021_u16.to_le_bytes()); // mod date 1980-01-01
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes()); // extra len
        out.extend_from_slice(name_bytes);
        out.extend_from_slice(data);

        // Central Directory Header (PK\x01\x02, 46 bytes + filename)
        central_dir.extend_from_slice(b"PK\x01\x02");
        central_dir.extend_from_slice(&20_u16.to_le_bytes()); // version made by
        central_dir.extend_from_slice(&20_u16.to_le_bytes()); // version needed
        central_dir.extend_from_slice(&0_u16.to_le_bytes()); // flags
        central_dir.extend_from_slice(&0_u16.to_le_bytes()); // method 0 = Stored
        central_dir.extend_from_slice(&0_u16.to_le_bytes()); // mod time
        central_dir.extend_from_slice(&0x0021_u16.to_le_bytes()); // mod date
        central_dir.extend_from_slice(&crc.to_le_bytes());
        central_dir.extend_from_slice(&size.to_le_bytes());
        central_dir.extend_from_slice(&size.to_le_bytes());
        central_dir.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        central_dir.extend_from_slice(&0_u16.to_le_bytes()); // extra len
        central_dir.extend_from_slice(&0_u16.to_le_bytes()); // comment len
        central_dir.extend_from_slice(&0_u16.to_le_bytes()); // disk number
        central_dir.extend_from_slice(&0_u16.to_le_bytes()); // internal attrs
        central_dir.extend_from_slice(&0_u32.to_le_bytes()); // external attrs
        central_dir.extend_from_slice(&local_offset.to_le_bytes());
        central_dir.extend_from_slice(name_bytes);
    }

    let cd_offset = out.len() as u32;
    let cd_size = central_dir.len() as u32;
    let count = entries.len() as u16;
    out.extend_from_slice(&central_dir);

    // End of Central Directory Record (PK\x05\x06, 22 bytes)
    out.extend_from_slice(b"PK\x05\x06");
    out.extend_from_slice(&0_u16.to_le_bytes()); // disk
    out.extend_from_slice(&0_u16.to_le_bytes()); // cd start disk
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&cd_size.to_le_bytes());
    out.extend_from_slice(&cd_offset.to_le_bytes());
    out.extend_from_slice(&0_u16.to_le_bytes()); // comment len

    out
}

fn zip_u16_le(bytes: &[u8], offset: usize) -> Option<u16> {
    let s = bytes.get(offset..offset.checked_add(2)?)?;
    Some(u16::from_le_bytes(<[u8; 2]>::try_from(s).ok()?))
}

fn zip_u32_le(bytes: &[u8], offset: usize) -> Option<u32> {
    let s = bytes.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_le_bytes(<[u8; 4]>::try_from(s).ok()?))
}

fn read_stored_zip(bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, DocError> {
    if bytes.len() < 22 {
        return Err(DocError::InvalidZipContainer("ZIP shorter than 22 bytes".to_owned()));
    }
    let min_eocd = bytes.len().saturating_sub(65_557);
    let mut eocd_pos = None;
    let mut i = bytes.len().saturating_sub(22);
    loop {
        if bytes.get(i..i.saturating_add(4)) == Some(b"PK\x05\x06") {
            let comment_len = usize::from(zip_u16_le(bytes, i.saturating_add(20)).unwrap_or(0));
            if i.saturating_add(22).saturating_add(comment_len) == bytes.len() {
                eocd_pos = Some(i);
                break;
            }
        }
        if i <= min_eocd {
            break;
        }
        i = i.saturating_sub(1);
    }

    let Some(eocd) = eocd_pos else {
        return Err(DocError::InvalidZipContainer("missing EOCD record".to_owned()));
    };

    let total_entries = usize::from(zip_u16_le(bytes, eocd.saturating_add(10)).unwrap_or(0));
    let cd_size = zip_u32_le(bytes, eocd.saturating_add(12)).unwrap_or(0) as usize;
    let cd_offset = zip_u32_le(bytes, eocd.saturating_add(16)).unwrap_or(0) as usize;

    let Some(cd_end) = cd_offset.checked_add(cd_size) else {
        return Err(DocError::InvalidZipContainer("central directory offset overflow".to_owned()));
    };
    if cd_end > eocd {
        return Err(DocError::InvalidZipContainer("central directory exceeds EOCD".to_owned()));
    }

    let mut entries = BTreeMap::new();
    let mut total_bytes: usize = 0;
    let mut pos = cd_offset;

    for _ in 0..total_entries {
        if pos.saturating_add(46) > cd_end || bytes.get(pos..pos.saturating_add(4)) != Some(b"PK\x01\x02") {
            return Err(DocError::InvalidZipContainer("corrupt central directory entry".to_owned()));
        }
        let method = zip_u16_le(bytes, pos.saturating_add(10)).unwrap_or(u16::MAX);
        if method != 0 {
            return Err(DocError::InvalidZipContainer(format!("unsupported ZIP method {method}")));
        }
        let expected_crc = zip_u32_le(bytes, pos.saturating_add(16)).unwrap_or(0);
        let comp_size = zip_u32_le(bytes, pos.saturating_add(20)).unwrap_or(0) as usize;
        let uncomp_size = zip_u32_le(bytes, pos.saturating_add(24)).unwrap_or(0) as usize;
        let name_len = usize::from(zip_u16_le(bytes, pos.saturating_add(28)).unwrap_or(0));
        let extra_len = usize::from(zip_u16_le(bytes, pos.saturating_add(30)).unwrap_or(0));
        let comment_len = usize::from(zip_u16_le(bytes, pos.saturating_add(32)).unwrap_or(0));
        let local_offset = zip_u32_le(bytes, pos.saturating_add(42)).unwrap_or(0) as usize;

        if comp_size != uncomp_size || uncomp_size > MAX_ZIP_ENTRY_BYTES {
            return Err(DocError::InvalidZipContainer("entry size invalid or exceeds limit".to_owned()));
        }
        total_bytes = total_bytes.checked_add(uncomp_size).ok_or_else(|| DocError::InvalidZipContainer("total size overflow".to_owned()))?;
        if total_bytes > MAX_ZIP_TOTAL_BYTES {
            return Err(DocError::InvalidZipContainer("archive exceeds total size limit".to_owned()));
        }

        let name_start = pos.saturating_add(46);
        let name_end = name_start.saturating_add(name_len);
        let next_pos = name_end.saturating_add(extra_len).saturating_add(comment_len);
        if next_pos > cd_end {
            return Err(DocError::InvalidZipContainer("central directory name truncated".to_owned()));
        }
        let name_slice =
            bytes.get(name_start..name_end).ok_or_else(|| DocError::InvalidZipContainer("central directory name truncated".to_owned()))?;
        let name = std::str::from_utf8(name_slice).map_err(|_| DocError::InvalidZipContainer("non-UTF8 entry name".to_owned()))?.to_owned();

        // Reject path traversal
        if name.starts_with('/') || name.contains("..") || name.contains('\\') {
            return Err(DocError::InvalidZipContainer(format!("unsafe entry path `{name}`")));
        }

        // Read local file header & data
        // Checked arithmetic: offsets come from the file and `usize` is 32 bits on wasm32.
        let local_header = local_offset.checked_add(30).filter(|end| *end <= cd_offset).and_then(|end| bytes.get(local_offset..end));
        let Some(local_header) = local_header else {
            return Err(DocError::InvalidZipContainer("invalid local file header".to_owned()));
        };
        let Some((l_name_len, l_extra_len)) = local_header
            .starts_with(b"PK\x03\x04")
            .then(|| (usize::from(zip_u16_le(local_header, 26).unwrap_or(0)), usize::from(zip_u16_le(local_header, 28).unwrap_or(0))))
        else {
            return Err(DocError::InvalidZipContainer("invalid local file header".to_owned()));
        };
        let Some(data_start) = local_offset.checked_add(30).and_then(|v| v.checked_add(l_name_len)).and_then(|v| v.checked_add(l_extra_len)) else {
            return Err(DocError::InvalidZipContainer("local header range overflow".to_owned()));
        };
        let Some(data_end) = data_start.checked_add(uncomp_size) else {
            return Err(DocError::InvalidZipContainer("local data range overflow".to_owned()));
        };
        if data_end > cd_offset {
            return Err(DocError::InvalidZipContainer("local data exceeds central directory".to_owned()));
        }

        let Some(slice) = bytes.get(data_start..data_end) else {
            return Err(DocError::InvalidZipContainer("local data out of range".to_owned()));
        };
        if crc32(slice) != expected_crc {
            if let Some(id) = name.strip_prefix("blobs/").and_then(|s| s.strip_suffix(".bin")) {
                return Err(DocError::CorruptBlob { id: id.to_owned(), reason: "ZIP CRC-32 mismatch".to_owned() });
            }
            return Err(DocError::InvalidZipContainer(format!("CRC-32 mismatch on `{name}`")));
        }

        if entries.insert(name.clone(), slice.to_vec()).is_some() {
            return Err(DocError::InvalidZipContainer(format!("duplicate ZIP entry `{name}`")));
        }

        pos = next_pos;
    }

    Ok(entries)
}

/// IEEE 802.3 CRC-32 checksum (`0xEDB88320` reflected polynomial).
#[must_use]
pub fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFF_u32;
    for &b in bytes {
        crc ^= u32::from(b);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

/// Clean-room FIPS 180-4 SHA-256 digest formatted as a 64-character lowercase hex string.
#[must_use]
#[allow(clippy::indexing_slicing)]
pub fn sha256_hex(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be,
        0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa,
        0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85,
        0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3,
        0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f,
        0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];

    let mut h: [u32; 8] = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];

    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut padded = Vec::with_capacity((data.len().saturating_add(72)) & !63);
    padded.extend_from_slice(data);
    padded.push(0x80);
    while (padded.len() % 64) != 56 {
        padded.push(0x00);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in padded.as_chunks::<64>().0 {
        let mut w = [0_u32; 64];
        for (slot, word) in w.iter_mut().zip(chunk.as_chunks::<4>().0) {
            *slot = u32::from_be_bytes(*word);
        }
        for i in 16..64usize {
            let w15 = w[i.saturating_sub(15)];
            let w2 = w[i.saturating_sub(2)];
            let s0 = w15.rotate_right(7) ^ w15.rotate_right(18) ^ (w15 >> 3);
            let s1 = w2.rotate_right(17) ^ w2.rotate_right(19) ^ (w2 >> 10);
            w[i] = w[i.saturating_sub(16)].wrapping_add(s0).wrapping_add(w[i.saturating_sub(7)]).wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64usize {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    let mut out = String::with_capacity(64);
    for word in h {
        out.push_str(&format!("{word:08x}"));
    }
    out
}
