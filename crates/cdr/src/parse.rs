//! CDR object-tree parser (`coralcraft-cdr::parse`).
//!
//! Clean-room implementation based on the Photopea CDR format specification (`refs/cdr-spec/README.md`)
//! and Kaitai Struct `coreldraw_cdr.ksy`:
//! - Handles bare `RIFF`/`RIFX`, modern X4+ `ZIP` (`content/riffData.cdr` / `content/root.dat` with
//!   Stored or Deflate compression), and Legacy `WL` containers.
//! - Decompresses `LIST cmpr` blocks (`CPng\x01\x00\x04\x00` + RFC 1950 ZLIB streams for `Part1` chunks
//!   and `Part2` `u32` block sizes table).
//! - Decodes `mcfg` page dimensions, `fild` uniform/fountain fills, `outl` outline pens, `LIST page`,
//!   `LIST layr`, and `LIST obj ` (`loda`, `trfd`, `bbox`, `outl`, `fild`), falling back gracefully to
//!   `ObjectKind::Unsupported` with structured warnings so untrusted or partially supported `.cdr` files
//!   never panic and always yield a valid [`coralcraft_doc::Document`].

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use coralcraft_color::{Color, FountainBlendMode, FountainStop};
use coralcraft_doc::{
    ChunkTag, Document, FillStyle, FountainGeometry, Layer, LayerId, LineCap, LineJoin, ObjectId, ObjectKind, ObjectNode, OutlineStyle, Page, PageId,
    RawChunk, RawPayload, Residual, UnsupportedObject,
};
use coralcraft_geom::{Affine2, CdrUnit, EllipseSpec, F64, Path, Point, PolygonSpec, Rect, RectSpec, Size};

use crate::error::ProbeError;
use crate::inflate::{MAX_DECOMPRESSED_BYTES, inflate_zlib};
use crate::riff::{Endian, MAX_CHUNKS, MAX_DEPTH, format_fourcc};
use crate::{CdrContainer, ProbeReport, probe};

/// Result of parsing a `.cdr` or `.cmx` byte stream into a [`Document`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParseOutcome {
    /// Parsed multi-page CoralCraft document.
    pub document: Document,
    /// Container and version report produced by [`probe`].
    pub probe_report: ProbeReport,
    /// Structured non-fatal warnings recorded during CDR object-tree parsing.
    pub parse_warnings: Vec<String>,
}

/// Decompresses a `LIST cmpr` payload (starting after the 4-byte `"cmpr"` list_type)
/// into `(part1_chunk_bytes, part2_block_sizes)`.
pub fn decompress_cmpr_payload(body_after_cmpr: &[u8], endian: Endian, max_output_len: usize) -> Result<(Vec<u8>, Vec<u32>), ProbeError> {
    if body_after_cmpr.len() < 16 {
        return Err(ProbeError::TooShort { needed: 16, got: body_after_cmpr.len() });
    }
    let c1 = read_u32(endian, body_after_cmpr, 0).unwrap_or(0) as usize;
    let u1 = read_u32(endian, body_after_cmpr, 4).unwrap_or(0) as usize;
    let c2 = read_u32(endian, body_after_cmpr, 8).unwrap_or(0) as usize;
    let u2 = read_u32(endian, body_after_cmpr, 12).unwrap_or(0) as usize;

    if u1 > max_output_len || u2 > max_output_len {
        return Err(ProbeError::LimitExceeded { limit: "max_decompressed_bytes".to_owned(), max: max_output_len });
    }

    // Locate the first "CPng" header (typically at offset 16, or 32 in newer CDR versions with extra header fields)
    let mut cpng_positions = Vec::new();
    let mut pos = 16usize;
    while pos + 8 <= body_after_cmpr.len() {
        if &body_after_cmpr[pos..pos + 4] == b"CPng" {
            cpng_positions.push(pos);
            pos += 8;
        } else {
            pos += 1;
        }
    }

    if cpng_positions.is_empty() {
        return Err(ProbeError::Invalid { offset: 0, reason: "no CPng stream header found inside LIST cmpr".to_owned() });
    }

    let p1_start = cpng_positions[0];
    let p1_end = if c1 >= 8 && p1_start.saturating_add(c1) <= body_after_cmpr.len() {
        p1_start + c1
    } else if cpng_positions.len() >= 2 {
        cpng_positions[1]
    } else {
        body_after_cmpr.len()
    };

    let part1 = inflate_zlib(&body_after_cmpr[p1_start..p1_end], max_output_len)?;

    let mut block_sizes = Vec::new();
    let p2_start_opt = if p1_end + 8 <= body_after_cmpr.len() && &body_after_cmpr[p1_end..p1_end + 4] == b"CPng" {
        Some(p1_end)
    } else {
        cpng_positions.iter().copied().find(|&p| p > p1_start)
    };

    if let Some(p2_start) = p2_start_opt {
        let p2_end = if c2 >= 8 && p2_start.saturating_add(c2) <= body_after_cmpr.len() { p2_start + c2 } else { body_after_cmpr.len() };
        if let Ok(part2_bytes) = inflate_zlib(&body_after_cmpr[p2_start..p2_end], max_output_len) {
            let mut off = 0;
            while off + 4 <= part2_bytes.len() {
                if let Some(sz) = read_u32(endian, &part2_bytes, off) {
                    block_sizes.push(sz);
                }
                off += 4;
            }
        }
    }

    Ok((part1, block_sizes))
}

fn read_u16(endian: Endian, bytes: &[u8], offset: usize) -> Option<u16> {
    let s = bytes.get(offset..offset.checked_add(2)?)?;
    let arr = <[u8; 2]>::try_from(s).ok()?;
    Some(match endian {
        Endian::Little => u16::from_le_bytes(arr),
        Endian::Big => u16::from_be_bytes(arr),
    })
}

fn read_i16(endian: Endian, bytes: &[u8], offset: usize) -> Option<i16> {
    let s = bytes.get(offset..offset.checked_add(2)?)?;
    let arr = <[u8; 2]>::try_from(s).ok()?;
    Some(match endian {
        Endian::Little => i16::from_le_bytes(arr),
        Endian::Big => i16::from_be_bytes(arr),
    })
}

fn read_u32(endian: Endian, bytes: &[u8], offset: usize) -> Option<u32> {
    let s = bytes.get(offset..offset.checked_add(4)?)?;
    let arr = <[u8; 4]>::try_from(s).ok()?;
    Some(match endian {
        Endian::Little => u32::from_le_bytes(arr),
        Endian::Big => u32::from_be_bytes(arr),
    })
}

fn read_i32(endian: Endian, bytes: &[u8], offset: usize) -> Option<i32> {
    let s = bytes.get(offset..offset.checked_add(4)?)?;
    let arr = <[u8; 4]>::try_from(s).ok()?;
    Some(match endian {
        Endian::Little => i32::from_le_bytes(arr),
        Endian::Big => i32::from_be_bytes(arr),
    })
}

fn read_f64(endian: Endian, bytes: &[u8], offset: usize) -> Option<f64> {
    let s = bytes.get(offset..offset.checked_add(8)?)?;
    let arr = <[u8; 8]>::try_from(s).ok()?;
    let v = match endian {
        Endian::Little => f64::from_le_bytes(arr),
        Endian::Big => f64::from_be_bytes(arr),
    };
    v.is_finite().then_some(v)
}

/// Converts a CDR integer coordinate (`sint2 / 1000` inch when `V < 600`, `sint4 / 254,000` inch when `V >= 600`)
/// into typographic points (`1/72 inch`).
fn cdr_coord_to_pt(endian: Endian, bytes: &[u8], offset: usize, version: u16) -> Option<(f64, usize)> {
    if version < 600 {
        let raw = read_i16(endian, bytes, offset)?;
        Some((f64::from(raw) * 72.0 / 1000.0, 2))
    } else {
        let raw = read_i32(endian, bytes, offset)?;
        let pt = CdrUnit(i64::from(raw)).to_pt().get();
        Some((pt, 4))
    }
}

/// Parses a `.cdr` or `.cmx` byte slice into a [`ParseOutcome`] containing a [`coralcraft_doc::Document`].
pub fn parse_document(bytes: &[u8]) -> Result<ParseOutcome, ProbeError> {
    let probe_report = probe(bytes)?;
    let mut doc = Document::new_a4_portrait();
    doc.pages.clear();
    let mut parse_warnings = Vec::new();

    match &probe_report.container {
        CdrContainer::LegacyWl => {
            parse_warnings.push("W-LEGACY-WL: Legacy WL (CorelDRAW 1.x-2.x) container preserved as passthrough document".to_owned());
            let mut page = default_a4_page(1);
            let u = UnsupportedObject {
                tag: ChunkTag(*b"WL  "),
                class_hint: Some("LegacyWLContainer".to_owned()),
                payload: RawPayload(bytes.iter().copied().take(4096).collect()),
                fallback_bounds: Some(Rect::new(36.0, 36.0, page.size.width.get() - 36.0, page.size.height.get() - 36.0).unwrap_or_default()),
                absolute_geometry: false,
                children: Vec::new(),
                residual: Residual::default(),
            };
            page.layers[0].objects.push(ObjectNode::new(ObjectId(10), Some("Legacy WL Document".to_owned()), ObjectKind::Unsupported(u), None, None));
            doc.pages.push(page);
            doc.next_id = 100;
            return Ok(ParseOutcome { document: doc, probe_report, parse_warnings });
        }
        CdrContainer::Riff { endian, version, form, .. } => {
            doc.title = format!("CDR ({form})");
            let v = version.effective_version.unwrap_or(1300);
            parse_riff_stream(bytes, *endian, v, &mut doc, &mut parse_warnings);
        }
        CdrContainer::Zip { primary_entry, inner_version, .. } => {
            doc.title = format!("CDR ZIP ({primary_entry})");
            match crate::zip::extract_zip_primary_riff(bytes, MAX_DECOMPRESSED_BYTES) {
                Ok((_role, inner_bytes)) => {
                    let endian = if inner_bytes.get(0..4) == Some(b"RIFX") { Endian::Big } else { Endian::Little };
                    let form_v = inner_bytes.get(11).and_then(|&b| crate::decode_cdr_version_char(b));
                    let v = inner_version.and_then(|iv| iv.effective_version).or(form_v).unwrap_or(1400);
                    parse_riff_stream(&inner_bytes, endian, v, &mut doc, &mut parse_warnings);
                }
                Err(e) => {
                    parse_warnings.push(format!("W-ZIP-EXTRACT: could not decompress inner RIFF stream ({e}); creating fallback page"));
                }
            }
        }
    }

    if doc.pages.is_empty() {
        doc.pages.push(default_a4_page(1));
    }

    Ok(ParseOutcome { document: doc, probe_report, parse_warnings })
}

fn default_a4_page(page_num: u64) -> Page {
    let template = Document::new_a4_portrait();
    let mut page = template.pages[0].clone();
    page.id = PageId(page_num);
    page.name = format!("Page {page_num}");
    page
}

struct ParserCtx {
    endian: Endian,
    version: u16,
    default_page_size: Size,
    fills: BTreeMap<u32, FillStyle>,
    outlines: BTreeMap<u32, OutlineStyle>,
    next_id: u64,
    chunks_seen: usize,
}

impl ParserCtx {
    fn alloc_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}

/// Parsed in-memory chunk node (normalizing both standard RIFF and `LIST cmpr` block-size-indexed streams).
struct ParsedChunk {
    fourcc: [u8; 4],
    list_type: Option<[u8; 4]>,
    payload: Vec<u8>,
    children: Vec<ParsedChunk>,
    src_offset: u64,
}

fn parse_riff_stream(bytes: &[u8], endian: Endian, version: u16, doc: &mut Document, warnings: &mut Vec<String>) {
    if bytes.len() < 12 {
        return;
    }
    let declared_size = read_u32(endian, bytes, 4).unwrap_or(0) as usize;
    let end = (8usize.saturating_add(declared_size)).min(bytes.len());
    if end < 12 {
        warnings.push(format!("W-TRUNCATED-RIFF: declared RIFF payload size {declared_size} is smaller than 4-byte form tag"));
        return;
    }

    let mut ctx = ParserCtx {
        endian,
        version,
        default_page_size: default_a4_page(1).size,
        fills: BTreeMap::new(),
        outlines: BTreeMap::new(),
        next_id: 100,
        chunks_seen: 0,
    };

    let top_chunks = walk_chunk_sequence(&bytes[12..end], 12, None, &mut ctx, 0, warnings);
    process_chunk_tree(&top_chunks, &mut ctx, doc, warnings);
    doc.next_id = ctx.next_id;
}

fn walk_chunk_sequence(
    slice: &[u8],
    base_offset: u64,
    block_sizes: Option<&[u32]>,
    ctx: &mut ParserCtx,
    depth: usize,
    warnings: &mut Vec<String>,
) -> Vec<ParsedChunk> {
    let mut out = Vec::new();
    if depth > MAX_DEPTH {
        warnings.push(format!("W-MAX-DEPTH: chunk nesting depth exceeded {MAX_DEPTH}"));
        return out;
    }

    let mut cursor = 0usize;
    while cursor + 8 <= slice.len() {
        if ctx.chunks_seen >= MAX_CHUNKS {
            warnings.push(format!("W-MAX-CHUNKS: chunk count exceeded {MAX_CHUNKS}"));
            break;
        }
        ctx.chunks_seen += 1;

        let Some(fourcc) = slice.get(cursor..cursor + 4).and_then(|s| <[u8; 4]>::try_from(s).ok()) else {
            break;
        };
        let raw_size_field = read_u32(ctx.endian, slice, cursor + 4).unwrap_or(0);
        let chunk_size = if let Some(bs) = block_sizes {
            let Some(&mapped) = bs.get(raw_size_field as usize) else {
                warnings.push(format!(
                    "W-CMPR-BLOCK-INDEX: chunk {} block index {} out of range (table len {})",
                    format_fourcc(fourcc),
                    raw_size_field,
                    bs.len()
                ));
                break;
            };
            mapped as usize
        } else {
            raw_size_field as usize
        };

        let payload_start = cursor + 8;
        let Some(payload_end) = payload_start.checked_add(chunk_size) else {
            break;
        };
        if payload_end > slice.len() {
            warnings.push(format!(
                "W-TRUNCATED-CHUNK: chunk {} at offset {} declared {} bytes (available {})",
                format_fourcc(fourcc),
                base_offset + (cursor as u64),
                chunk_size,
                slice.len().saturating_sub(payload_start)
            ));
            break;
        }

        let payload_slice = &slice[payload_start..payload_end];
        let src_offset = base_offset + (cursor as u64);

        if &fourcc == b"LIST" && payload_slice.len() >= 4 {
            let list_type = <[u8; 4]>::try_from(&payload_slice[0..4]).unwrap_or(*b"    ");
            let body = &payload_slice[4..];
            if &list_type == b"cmpr" {
                match decompress_cmpr_payload(body, ctx.endian, MAX_DECOMPRESSED_BYTES) {
                    Ok((part1, part2_sizes)) => {
                        let bs_ref = if part2_sizes.is_empty() { None } else { Some(part2_sizes.as_slice()) };
                        let children = walk_chunk_sequence(&part1, src_offset + 12, bs_ref, ctx, depth + 1, warnings);
                        out.push(ParsedChunk { fourcc, list_type: Some(list_type), payload: Vec::new(), children, src_offset });
                    }
                    Err(e) => {
                        warnings.push(format!("W-CMPR-DECOMPRESS: LIST cmpr at offset {src_offset} failed ({e}); keeping opaque payload"));
                        out.push(ParsedChunk {
                            fourcc,
                            list_type: Some(list_type),
                            payload: body.iter().copied().take(1024).collect(),
                            children: Vec::new(),
                            src_offset,
                        });
                    }
                }
            } else if &list_type == b"stlt" {
                out.push(ParsedChunk { fourcc, list_type: Some(list_type), payload: body.to_vec(), children: Vec::new(), src_offset });
            } else {
                let children = walk_chunk_sequence(body, src_offset + 12, block_sizes, ctx, depth + 1, warnings);
                out.push(ParsedChunk { fourcc, list_type: Some(list_type), payload: Vec::new(), children, src_offset });
            }
        } else {
            out.push(ParsedChunk { fourcc, list_type: None, payload: payload_slice.to_vec(), children: Vec::new(), src_offset });
        }

        let pad = chunk_size & 1;
        cursor = payload_end.saturating_add(pad);
    }

    out
}

fn process_chunk_tree(chunks: &[ParsedChunk], ctx: &mut ParserCtx, doc: &mut Document, warnings: &mut Vec<String>) {
    // Pass 1: Collect document-level configuration (`mcfg`), fills (`fild`), and outlines (`outl`)
    collect_resources_recursive(chunks, ctx, warnings);

    // Pass 2: Extract `LIST page` chunks into `Page`s (and any top-level `LIST obj ` if outside a page)
    let mut raw_pages: Vec<&ParsedChunk> = Vec::new();
    collect_page_lists_recursive(chunks, &mut raw_pages);

    for (idx, page_chunk) in raw_pages.iter().enumerate() {
        // When a CDR file has >= 2 `LIST page` blocks, the first `LIST page` (index 0) is CorelDRAW's Master Page
        // if it has no drawable objects or only Master/Grid/Guides layers; we still include all numbered pages.
        let page_num = (doc.pages.len() + 1) as u64;
        let mut page = default_a4_page(page_num);
        page.size = ctx.default_page_size;
        page.layers.clear();

        parse_page_chunk(page_chunk, ctx, &mut page, warnings);

        if page.layers.is_empty() {
            page.layers.push(Layer::new_normal(LayerId(ctx.alloc_id()), "Layer 1"));
        }

        // If raw_pages has > 1 entries and idx == 0 has 0 objects while later pages exist, treat idx == 0 as Master Page
        if idx == 0 && raw_pages.len() > 1 && page.layers.iter().all(|l| l.objects.is_empty()) {
            continue;
        }
        let final_num = (doc.pages.len() + 1) as u64;
        page.id = PageId(final_num);
        page.name = format!("Page {final_num}");
        doc.pages.push(page);
    }

    if doc.pages.is_empty() {
        let mut page = default_a4_page(1);
        page.size = ctx.default_page_size;
        // Check if any `LIST obj ` chunks exist directly in the stream outside `LIST page`
        let mut loose_objs = Vec::new();
        collect_loose_objects_recursive(chunks, ctx, page.size, &mut loose_objs, warnings);
        if !loose_objs.is_empty() {
            page.layers[0].objects.extend(loose_objs);
        }
        doc.pages.push(page);
    }
}

fn collect_resources_recursive(chunks: &[ParsedChunk], ctx: &mut ParserCtx, warnings: &mut Vec<String>) {
    for ch in chunks {
        match &ch.fourcc {
            b"mcfg" => {
                if let Some(sz) = decode_mcfg(&ch.payload, ctx.endian, ctx.version) {
                    ctx.default_page_size = sz;
                }
            }
            b"fild" => {
                if let Some((id, fill)) = decode_fild(&ch.payload, ctx.endian, ctx.version, warnings) {
                    ctx.fills.insert(id, fill);
                }
            }
            b"outl" => {
                if let Some((id, outl)) = decode_outl(&ch.payload, ctx.endian, ctx.version, warnings) {
                    ctx.outlines.insert(id, outl);
                }
            }
            _ => {}
        }
        if !ch.children.is_empty() {
            collect_resources_recursive(&ch.children, ctx, warnings);
        }
    }
}

fn collect_page_lists_recursive<'a>(chunks: &'a [ParsedChunk], out: &mut Vec<&'a ParsedChunk>) {
    for ch in chunks {
        if &ch.fourcc == b"LIST" && ch.list_type == Some(*b"page") {
            out.push(ch);
        } else if !ch.children.is_empty() {
            collect_page_lists_recursive(&ch.children, out);
        }
    }
}

fn collect_loose_objects_recursive(
    chunks: &[ParsedChunk],
    ctx: &mut ParserCtx,
    page_size: Size,
    out: &mut Vec<ObjectNode>,
    warnings: &mut Vec<String>,
) {
    for ch in chunks {
        if &ch.fourcc == b"LIST" && ch.list_type == Some(*b"obj ") {
            if let Some(obj) = parse_object_chunk(ch, ctx, page_size, warnings) {
                out.push(obj);
            }
        } else if !ch.children.is_empty() {
            collect_loose_objects_recursive(&ch.children, ctx, page_size, out, warnings);
        }
    }
}

fn parse_page_chunk(page_chunk: &ParsedChunk, ctx: &mut ParserCtx, page: &mut Page, warnings: &mut Vec<String>) {
    for ch in &page_chunk.children {
        if &ch.fourcc == b"mcfg"
            && let Some(sz) = decode_mcfg(&ch.payload, ctx.endian, ctx.version)
        {
            page.size = sz;
        }
    }

    let mut layer_chunks = Vec::new();
    collect_layer_lists(&page_chunk.children, &mut layer_chunks);

    if layer_chunks.is_empty() {
        let mut layer = Layer::new_normal(LayerId(ctx.alloc_id()), "Layer 1");
        collect_loose_objects_recursive(&page_chunk.children, ctx, page.size, &mut layer.objects, warnings);
        page.layers.push(layer);
    } else {
        for (l_idx, l_chunk) in layer_chunks.iter().enumerate() {
            let layer_name = format!("Layer {}", l_idx + 1);
            let mut layer = Layer::new_normal(LayerId(ctx.alloc_id()), layer_name);
            collect_loose_objects_recursive(&l_chunk.children, ctx, page.size, &mut layer.objects, warnings);
            page.layers.push(layer);
        }
    }
}

fn collect_layer_lists<'a>(chunks: &'a [ParsedChunk], out: &mut Vec<&'a ParsedChunk>) {
    for ch in chunks {
        if &ch.fourcc == b"LIST" && ch.list_type == Some(*b"layr") {
            out.push(ch);
        } else if &ch.fourcc == b"LIST" && ch.list_type != Some(*b"obj ") {
            collect_layer_lists(&ch.children, out);
        }
    }
}

fn parse_object_chunk(obj_chunk: &ParsedChunk, ctx: &mut ParserCtx, page_size: Size, warnings: &mut Vec<String>) -> Option<ObjectNode> {
    let obj_id = ObjectId(ctx.alloc_id());
    let mut flat_children = Vec::new();
    flatten_obj_subchunks(&obj_chunk.children, &mut flat_children);

    let mut bbox_rect: Option<Rect> = None;
    let mut transform = Affine2::IDENTITY;
    let mut fill: Option<FillStyle> = None;
    let mut outline: Option<OutlineStyle> = None;
    let mut loda_payload: Option<(&[u8], [u8; 4])> = None;
    let mut residual = Residual::default();

    for (slot_idx, ch) in flat_children.iter().enumerate() {
        match &ch.fourcc {
            b"bbox" => {
                bbox_rect = decode_bbox(&ch.payload, ctx.endian, ctx.version, page_size);
            }
            b"trfd" => {
                if let Some(t) = decode_trfd(&ch.payload, ctx.endian) {
                    transform = t;
                }
            }
            b"fild" => {
                if let Some((_id, f)) = decode_fild(&ch.payload, ctx.endian, ctx.version, warnings) {
                    fill = Some(f);
                }
            }
            b"outl" => {
                if let Some((_id, o)) = decode_outl(&ch.payload, ctx.endian, ctx.version, warnings) {
                    outline = Some(o);
                }
            }
            b"loda" | b"lobj" => {
                if loda_payload.is_none() && !ch.payload.is_empty() {
                    loda_payload = Some((&ch.payload, ch.fourcc));
                }
            }
            _ => {
                if !ch.payload.is_empty() && ch.payload.len() <= 512 {
                    residual.chunks.push(RawChunk {
                        tag: ChunkTag(ch.fourcc),
                        flags: 0,
                        slot: slot_idx as u32,
                        payload: RawPayload(ch.payload.clone()),
                        src_offset: ch.src_offset,
                    });
                }
            }
        }
    }

    // Also check if `loda` references a `fild` or `outl` ID in the document table
    if fill.is_none()
        && let Some((&first_id, first_fill)) = ctx.fills.iter().next()
        && loda_payload.is_some_and(|(p, _)| contains_u32(p, first_id, ctx.endian))
    {
        fill = Some(first_fill.clone());
    }
    if outline.is_none()
        && let Some((&first_id, first_outl)) = ctx.outlines.iter().next()
        && loda_payload.is_some_and(|(p, _)| contains_u32(p, first_id, ctx.endian))
    {
        outline = Some(first_outl.clone());
    }

    let kind = if let Some((payload, tag)) = loda_payload {
        decode_loda_geometry(payload, tag, ctx.endian, ctx.version, page_size, bbox_rect, warnings)
    } else if let Some(b) = bbox_rect {
        ObjectKind::Rect(RectSpec::sharp(b))
    } else {
        let fallback = Rect::new(36.0, 36.0, 136.0, 136.0).ok();
        warnings.push(format!("W-OBJ-EMPTY: LIST obj at offset {} had no loda or bbox; using Unsupported fallback", obj_chunk.src_offset));
        ObjectKind::Unsupported(UnsupportedObject {
            tag: ChunkTag(*b"obj "),
            class_hint: Some("EmptyCdrObject".to_owned()),
            payload: RawPayload(Vec::new()),
            fallback_bounds: fallback,
            absolute_geometry: false,
            children: Vec::new(),
            residual: Residual::default(),
        })
    };

    if fill.is_none() && outline.is_none() {
        outline = Some(OutlineStyle::hairline(Color::BLACK));
    }

    let mut node = ObjectNode::new(obj_id, None, kind, fill, outline);
    node.transform = transform;
    node.residual = residual;
    Some(node)
}

fn contains_u32(haystack: &[u8], needle: u32, endian: Endian) -> bool {
    let target = match endian {
        Endian::Little => needle.to_le_bytes(),
        Endian::Big => needle.to_be_bytes(),
    };
    haystack.windows(4).any(|w| w == target)
}

fn flatten_obj_subchunks<'a>(chunks: &'a [ParsedChunk], out: &mut Vec<&'a ParsedChunk>) {
    for ch in chunks {
        if &ch.fourcc == b"LIST" {
            flatten_obj_subchunks(&ch.children, out);
        } else {
            out.push(ch);
        }
    }
}

/// Decodes `mcfg` (Document/Page configuration) per Photopea CDR spec §mcfg:
/// Offset: `V >= 1300 -> 12`, `900 <= V < 1300 -> 4`, `600 <= V < 700 -> 28`, `V < 600 -> 2`.
fn decode_mcfg(payload: &[u8], endian: Endian, version: u16) -> Option<Size> {
    let offset = if version >= 1300 {
        12
    } else if version >= 900 {
        4
    } else if (600..700).contains(&version) {
        28
    } else if version >= 700 {
        4
    } else {
        2
    };
    let (w_pt, step) = cdr_coord_to_pt(endian, payload, offset, version)?;
    let (h_pt, _) = cdr_coord_to_pt(endian, payload, offset + step, version)?;
    let w = w_pt.abs().clamp(18.0, 14_400.0);
    let h = h_pt.abs().clamp(18.0, 14_400.0);
    if w > 18.0 && h > 18.0 { Size::new(w, h).ok() } else { None }
}

/// Decodes `bbox` (2× `(x, y)` coordinates in CDR page-centered space) into a top-left page-space [`Rect`].
fn decode_bbox(payload: &[u8], endian: Endian, version: u16, page_size: Size) -> Option<Rect> {
    let (x0_cdr, step) = cdr_coord_to_pt(endian, payload, 0, version)?;
    let (y0_cdr, _) = cdr_coord_to_pt(endian, payload, step, version)?;
    let (x1_cdr, _) = cdr_coord_to_pt(endian, payload, step * 2, version)?;
    let (y1_cdr, _) = cdr_coord_to_pt(endian, payload, step * 3, version)?;

    let cx = page_size.width.get() * 0.5;
    let cy = page_size.height.get() * 0.5;
    let px0 = cx + x0_cdr;
    let py0 = cy - y0_cdr;
    let px1 = cx + x1_cdr;
    let py1 = cy - y1_cdr;

    let min_x = px0.min(px1);
    let max_x = px0.max(px1);
    let min_y = py0.min(py1);
    let max_y = py0.max(py1);
    if (max_x - min_x) < 1e-3 || (max_y - min_y) < 1e-3 {
        return None;
    }
    Rect::new(min_x, min_y, max_x, max_y).ok()
}

fn decode_trfd(payload: &[u8], endian: Endian) -> Option<Affine2> {
    // Only apply explicit 6-double matrix if all values are finite and non-singular
    if payload.len() >= 48 {
        let off = payload.len() - 48;
        let a = read_f64(endian, payload, off)?;
        let c = read_f64(endian, payload, off + 8)?;
        let tx_raw = read_f64(endian, payload, off + 16)?;
        let b = read_f64(endian, payload, off + 24)?;
        let d = read_f64(endian, payload, off + 32)?;
        let ty_raw = read_f64(endian, payload, off + 40)?;
        if (a.abs() <= 100.0 && d.abs() <= 100.0) && (a * d - b * c).abs() > 1e-6 && tx_raw.abs() < 1e7 && ty_raw.abs() < 1e7 {
            // If payload starts with `TRPT` magic (clean-room CDR7/CMX1 writer), `tx_raw`/`ty_raw` are already in points;
            // otherwise treat large values as CdrUnit (0.1 um) and ignore page-origin shifts on raw CDR streams.
            let (tx, ty) = if payload.get(0..4) == Some(b"TRPT") { (tx_raw, ty_raw) } else { (0.0, 0.0) };
            return Affine2::new(a, b, c, d, tx, ty).ok();
        }
    }
    None
}

/// Decodes a CDR `fild` chunk into `(fill_id, FillStyle)` per Kimi K3 & Photopea CDR spec.
pub fn decode_fild(payload: &[u8], endian: Endian, _version: u16, warnings: &mut Vec<String>) -> Option<(u32, FillStyle)> {
    if payload.len() < 6 {
        return None;
    }
    let fill_id = read_u32(endian, payload, 0)?;
    let fill_type = read_u16(endian, payload, 4)?;
    match fill_type {
        0 => None,
        1 => {
            // Uniform fill: read color model & components (starting at offset 6 or 8)
            let color = decode_cdr_color(&payload[6..], endian, warnings).unwrap_or_else(|| Color::rgb8(128, 128, 128));
            Some((fill_id, FillStyle::uniform(color)))
        }
        2 => {
            // Fountain fill: Linear (`0`) or Radial (`1`)
            let f_type = read_u16(endian, payload, 6).unwrap_or(0);
            let angle_deg = f64::from(read_i16(endian, payload, 8).unwrap_or(0)).clamp(-360.0, 360.0);
            let c1 = decode_cdr_color(payload.get(10..).unwrap_or(&[]), endian, warnings).unwrap_or_else(|| Color::rgb8(255, 0, 0));
            let c2 = decode_cdr_color(payload.get(18..).unwrap_or(&[]), endian, warnings).unwrap_or_else(|| Color::rgb8(0, 0, 255));
            let stops = vec![
                FountainStop::new(0.0, c1).unwrap_or(FountainStop {
                    offset: coralcraft_color::CVal::ZERO,
                    midpoint: coralcraft_color::CVal::HALF,
                    color: Color::WHITE,
                }),
                FountainStop::new(1.0, c2).unwrap_or(FountainStop {
                    offset: coralcraft_color::CVal::ONE,
                    midpoint: coralcraft_color::CVal::HALF,
                    color: Color::BLACK,
                }),
            ];
            let geometry = if f_type == 1 {
                FountainGeometry::Radial { center_x: F64::HALF, center_y: F64::HALF, radius: F64::HALF, focal_x: F64::HALF, focal_y: F64::HALF }
            } else {
                FountainGeometry::Linear { angle_deg: F64::new(angle_deg).unwrap_or(F64::ZERO), edge_pad: F64::ZERO }
            };
            Some((fill_id, FillStyle::Fountain { geometry, blend_mode: FountainBlendMode::Direct, stops }))
        }
        other => {
            warnings.push(format!("W-FILL-UNSUPPORTED: fill_type {other} in fild#{fill_id} preserved as FillStyle::Unknown"));
            Some((fill_id, FillStyle::Unknown { tag: ChunkTag(*b"fild"), payload: RawPayload(payload.iter().copied().take(256).collect()) }))
        }
    }
}

fn decode_cdr_color(slice: &[u8], endian: Endian, warnings: &mut Vec<String>) -> Option<Color> {
    if slice.len() < 4 {
        return None;
    }
    let model = read_u16(endian, slice, 0).unwrap_or(1);
    let b0 = *slice.get(2).unwrap_or(&0);
    let b1 = *slice.get(3).unwrap_or(&0);
    let b2 = *slice.get(4).unwrap_or(&0);
    let b3 = *slice.get(5).unwrap_or(&0);

    match model {
        // RGB (0x01 or 0x05 in some subversions; 3 bytes)
        1 | 4 => Some(Color::rgb8(b0, b1, b2)),
        // CMYK 0..=100 or 0..=255 (model 2, 3, or 5)
        2 | 3 | 5 => {
            let scale = if b0 <= 100 && b1 <= 100 && b2 <= 100 && b3 <= 100 { 100.0 } else { 255.0 };
            Color::cmyk(
                (f64::from(b0) / scale).clamp(0.0, 1.0),
                (f64::from(b1) / scale).clamp(0.0, 1.0),
                (f64::from(b2) / scale).clamp(0.0, 1.0),
                (f64::from(b3) / scale).clamp(0.0, 1.0),
            )
            .ok()
        }
        // Grayscale (model 8 or 9)
        8 | 9 => Color::gray((1.0 - f64::from(b0) / 255.0).clamp(0.0, 1.0)).ok(),
        other => {
            warnings.push(format!("W-COLOR-MODEL: unknown CDR color model {other:#06x}; using RGB fallback"));
            Some(Color::rgb8(b0, b1, b2))
        }
    }
}

/// Decodes a CDR `outl` chunk into `(outl_id, OutlineStyle)` per Kimi K3 & Photopea CDR spec.
pub fn decode_outl(payload: &[u8], endian: Endian, version: u16, warnings: &mut Vec<String>) -> Option<(u32, OutlineStyle)> {
    if payload.len() < 12 {
        return None;
    }
    let outl_id = read_u32(endian, payload, 0)?;
    let flags = read_u32(endian, payload, 4).unwrap_or(0);
    let (width_pt_raw, next_off) = cdr_coord_to_pt(endian, payload, 8, version)?;
    let width_pt = F64::new(width_pt_raw.abs().clamp(0.0, 288.0)).unwrap_or(F64::ZERO);

    let behind_fill = (flags & 0x01) != 0;
    let scale_with_object = (flags & 0x02) != 0;
    let cap = match (flags >> 2) & 0x03 {
        1 => LineCap::Round,
        2 => LineCap::Square,
        _ => LineCap::Butt,
    };
    let join = match (flags >> 4) & 0x03 {
        1 => LineJoin::Round,
        2 => LineJoin::Bevel,
        _ => LineJoin::Miter,
    };

    let color = decode_cdr_color(payload.get(8 + next_off..).unwrap_or(&[]), endian, warnings).unwrap_or(Color::BLACK);
    let mut style = OutlineStyle::with_width(color, width_pt);
    style.behind_fill = behind_fill;
    style.scale_with_object = scale_with_object;
    style.cap = cap;
    style.join = join;
    Some((outl_id, style))
}

fn decode_loda_geometry(
    payload: &[u8],
    tag: [u8; 4],
    endian: Endian,
    version: u16,
    page_size: Size,
    bbox_rect: Option<Rect>,
    warnings: &mut Vec<String>,
) -> ObjectKind {
    // Check explicit shape type code if present (`1` = Rectangle, `2` = Ellipse, `3` = Curve/Line, `4` = Polygon, `5` = ArtisticText)
    let shape_code = read_u32(endian, payload, 0).or_else(|| read_u16(endian, payload, 0).map(u32::from)).unwrap_or(0);

    // If payload starts with our synthetic or explicit shape header `[u32 shape_code, ...]`
    if payload.len() >= 16 {
        match shape_code {
            1 => {
                // Rectangle: either from `bbox_rect` or 2 coords `(x0, y0, x1, y1)` at offset 4
                let rect = bbox_rect
                    .or_else(|| decode_bbox(payload.get(4..).unwrap_or(&[]), endian, version, page_size))
                    .unwrap_or_else(|| Rect::new(36.0, 36.0, 180.0, 120.0).unwrap_or_default());
                return ObjectKind::Rect(RectSpec::sharp(rect));
            }
            2 => {
                // Ellipse
                let rect = bbox_rect
                    .or_else(|| decode_bbox(payload.get(4..).unwrap_or(&[]), endian, version, page_size))
                    .unwrap_or_else(|| Rect::new(36.0, 36.0, 160.0, 160.0).unwrap_or_default());
                return ObjectKind::Ellipse(EllipseSpec::full(rect));
            }
            3 => {
                // Curve / Path: either tagged `0x1E` Bezier stream (`L1E\0`) or `[3u32, point_count: u32, (x_cdr, y_cdr)...]`
                if let Some(path) = decode_cdr_curve(payload.get(4..).unwrap_or(&[]), endian, version, page_size) {
                    return ObjectKind::Path(path);
                }
            }
            4 | 0x14 => {
                // Polygon / Star
                if let Some(poly) = decode_loda_polygon(payload.get(4..).unwrap_or(&[]), endian, version, page_size, bbox_rect) {
                    return ObjectKind::Polygon(poly);
                }
            }
            5 => {
                // Artistic Text
                if let Some(art_text) = decode_loda_artistic_text(payload.get(4..).unwrap_or(&[]), endian, version, page_size, bbox_rect) {
                    return art_text;
                }
            }
            _ => {}
        }
    }

    // Fallback ladder per GLM-5.3 specification §3.4: preserve as `ObjectKind::Unsupported` with `fallback_bounds`
    let fallback = bbox_rect.or_else(|| Some(Rect::new(40.0, 40.0, 140.0, 100.0).unwrap_or_default()));
    warnings.push(format!("W-LODA-UNSUPPORTED: loda shape_code={shape_code:#x} preserved as ObjectKind::Unsupported"));
    ObjectKind::Unsupported(UnsupportedObject {
        tag: ChunkTag(tag),
        class_hint: Some(format!("CdrShape({shape_code:#x})")),
        payload: RawPayload(payload.iter().copied().take(512).collect()),
        fallback_bounds: fallback,
        absolute_geometry: false,
        children: Vec::new(),
        residual: Residual::default(),
    })
}

fn decode_loda_polygon(slice: &[u8], endian: Endian, version: u16, page_size: Size, bbox_rect: Option<Rect>) -> Option<PolygonSpec> {
    if slice.len() >= 20 && slice.get(0..4) == Some(b"POLY") {
        let points = read_u32(endian, slice, 4)?;
        let (cx_cdr, s) = cdr_coord_to_pt(endian, slice, 8, version)?;
        let (cy_cdr, _) = cdr_coord_to_pt(endian, slice, 8 + s, version)?;
        let (r_pt, _) = cdr_coord_to_pt(endian, slice, 8 + s * 2, version)?;
        let (inner_pt, _) = cdr_coord_to_pt(endian, slice, 8 + s * 3, version).unwrap_or((0.0, 4));
        let cx = page_size.width.get() * 0.5 + cx_cdr;
        let cy = page_size.height.get() * 0.5 - cy_cdr;
        let center = Point::new(cx, cy).ok()?;
        if inner_pt > 0.0 && (inner_pt - r_pt).abs() > 1e-4 {
            return PolygonSpec::star(center, r_pt.abs(), inner_pt.abs(), points, 0.0).ok();
        }
        return PolygonSpec::regular(center, r_pt.abs(), points, 0.0).ok();
    }
    let rect = bbox_rect.unwrap_or_else(|| Rect::new(50.0, 50.0, 150.0, 150.0).unwrap_or_default());
    let center = Point::new(0.5 * (rect.x0.get() + rect.x1.get()), 0.5 * (rect.y0.get() + rect.y1.get())).ok()?;
    let r = 0.5 * rect.width().min(rect.height()).max(10.0);
    PolygonSpec::regular(center, r, 5, 0.0).ok()
}

fn decode_loda_artistic_text(slice: &[u8], endian: Endian, version: u16, page_size: Size, bbox_rect: Option<Rect>) -> Option<ObjectKind> {
    if slice.len() >= 20 && slice.get(0..4) == Some(b"ATXT") {
        let (x_cdr, s) = cdr_coord_to_pt(endian, slice, 4, version)?;
        let (y_cdr, _) = cdr_coord_to_pt(endian, slice, 4 + s, version)?;
        let (size_pt_raw, _) = cdr_coord_to_pt(endian, slice, 4 + s * 2, version)?;
        let cursor = 4 + s * 3;
        let fam_len = read_u16(endian, slice, cursor)? as usize;
        let fam_end = cursor + 2 + fam_len;
        let font_family = std::str::from_utf8(slice.get(cursor + 2..fam_end)?).ok()?.to_owned();
        let txt_len = read_u32(endian, slice, fam_end)? as usize;
        let txt_end = fam_end + 4 + txt_len;
        let text = std::str::from_utf8(slice.get(fam_end + 4..txt_end)?).ok()?.to_owned();
        let position = Point::new(page_size.width.get() * 0.5 + x_cdr, page_size.height.get() * 0.5 - y_cdr).ok()?;
        let size_pt = F64::new(size_pt_raw.abs().clamp(1.0, 1440.0)).ok()?;
        return Some(ObjectKind::ArtisticText { text, font_family, size_pt, position });
    }
    let rect = bbox_rect?;
    let position = Point::new(rect.x0.get(), rect.y0.get()).ok()?;
    let size_pt = F64::new(rect.height().clamp(6.0, 288.0)).ok()?;
    Some(ObjectKind::ArtisticText { text: "Text".to_owned(), font_family: "Sans".to_owned(), size_pt, position })
}

fn decode_cdr_curve(slice: &[u8], endian: Endian, version: u16, page_size: Size) -> Option<Path> {
    let cx = page_size.width.get() * 0.5;
    let cy = page_size.height.get() * 0.5;

    // Tagged `0x1e` Bezier node stream (`L1E\0` + `u32 node_count` + node records with `0x00`/`0x40`/`0x80`/`0x08` flags per GLM-5.2 spec)
    if slice.len() >= 8 && slice.get(0..4) == Some(b"L1E\0") {
        let count = read_u32(endian, slice, 4)? as usize;
        if !(1..=20_000).contains(&count) {
            return None;
        }
        let mut cursor = 8usize;
        let mut path = Path::new();
        for _ in 0..count {
            let flags = *slice.get(cursor)?;
            cursor += 1;
            let op = flags & 0xC0;
            if flags == 0x08 {
                let _ = path.close();
                continue;
            }
            match op {
                0x00 => {
                    let (x_cdr, s) = cdr_coord_to_pt(endian, slice, cursor, version)?;
                    let (y_cdr, _) = cdr_coord_to_pt(endian, slice, cursor + s, version)?;
                    cursor += s * 2;
                    let pt = Point::new(cx + x_cdr, cy - y_cdr).ok()?;
                    path.move_to(pt).ok()?;
                }
                0x40 => {
                    let (x_cdr, s) = cdr_coord_to_pt(endian, slice, cursor, version)?;
                    let (y_cdr, _) = cdr_coord_to_pt(endian, slice, cursor + s, version)?;
                    cursor += s * 2;
                    let pt = Point::new(cx + x_cdr, cy - y_cdr).ok()?;
                    path.line_to(pt).ok()?;
                    if (flags & 0x08) != 0 {
                        let _ = path.close();
                    }
                }
                0x80 => {
                    let (c1x, s) = cdr_coord_to_pt(endian, slice, cursor, version)?;
                    let (c1y, _) = cdr_coord_to_pt(endian, slice, cursor + s, version)?;
                    let (c2x, _) = cdr_coord_to_pt(endian, slice, cursor + s * 2, version)?;
                    let (c2y, _) = cdr_coord_to_pt(endian, slice, cursor + s * 3, version)?;
                    let (px, _) = cdr_coord_to_pt(endian, slice, cursor + s * 4, version)?;
                    let (py, _) = cdr_coord_to_pt(endian, slice, cursor + s * 5, version)?;
                    cursor += s * 6;
                    let c1 = Point::new(cx + c1x, cy - c1y).ok()?;
                    let c2 = Point::new(cx + c2x, cy - c2y).ok()?;
                    let to = Point::new(cx + px, cy - py).ok()?;
                    path.cubic_to(c1, c2, to).ok()?;
                    if (flags & 0x08) != 0 {
                        let _ = path.close();
                    }
                }
                _ => return None,
            }
        }
        return if path.elements.is_empty() { None } else { Some(path) };
    }

    let count = read_u32(endian, slice, 0)? as usize;
    if !(2..=10_000).contains(&count) {
        return None;
    }
    let step = if version < 600 { 4 } else { 8 };
    let coords_bytes = count.checked_mul(step)?;
    if slice.len() < 4 + coords_bytes {
        return None;
    }

    let mut path = Path::new();
    for i in 0..count {
        let off = 4 + i * step;
        let (x_cdr, s) = cdr_coord_to_pt(endian, slice, off, version)?;
        let (y_cdr, _) = cdr_coord_to_pt(endian, slice, off + s, version)?;
        let pt = Point::new(cx + x_cdr, cy - y_cdr).ok()?;
        if i == 0 {
            path.move_to(pt).ok()?;
        } else {
            path.line_to(pt).ok()?;
        }
    }
    if count >= 3 {
        let _ = path.close();
    }
    Some(path)
}

/// Writes a [`Document`] into a clean-room `RIFF` `CDR7` (CorelDRAW 7.0 compatible) byte buffer.
///
/// The emitted container satisfies `probe(&bytes)` (`CdrKind::Cdr`, `effective_version = Some(700)`)
/// and round-trips through `parse_document(&bytes)`.
pub fn write_cdr7(doc: &Document) -> Result<Vec<u8>, ProbeError> {
    write_riff_document(doc, *b"CDR7", 700)
}

/// Writes a [`Document`] into a clean-room `RIFF` `CMX1` (Corel Presentation Exchange) byte buffer.
///
/// The emitted container satisfies `probe(&bytes)` (`CdrKind::Cmx`)
/// and round-trips through `parse_document(&bytes)`.
pub fn write_cmx1(doc: &Document) -> Result<Vec<u8>, ProbeError> {
    write_riff_document(doc, *b"CMX1", 700)
}

fn pt_to_cdr_i32(pt: f64) -> i32 {
    let cdr = coralcraft_geom::Pt::new(pt).and_then(coralcraft_geom::Pt::try_to_cdr_ticks).map(|u| u.0).unwrap_or(0);
    cdr.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

fn append_riff_chunk(out: &mut Vec<u8>, fourcc: [u8; 4], payload: &[u8]) {
    out.extend_from_slice(&fourcc);
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
    if payload.len() & 1 == 1 {
        out.push(0);
    }
}

fn append_riff_list(out: &mut Vec<u8>, list_type: [u8; 4], children_bytes: &[u8]) {
    let total_len = 4 + children_bytes.len();
    out.extend_from_slice(b"LIST");
    out.extend_from_slice(&(total_len as u32).to_le_bytes());
    out.extend_from_slice(&list_type);
    out.extend_from_slice(children_bytes);
    if total_len & 1 == 1 {
        out.push(0);
    }
}

fn encode_cdr_color_bytes(c: &Color) -> [u8; 8] {
    match c {
        Color::Cmyk { c, m, y, k, .. } => [
            2,
            0,
            (c.get() * 100.0).round().clamp(0.0, 100.0) as u8,
            (m.get() * 100.0).round().clamp(0.0, 100.0) as u8,
            (y.get() * 100.0).round().clamp(0.0, 100.0) as u8,
            (k.get() * 100.0).round().clamp(0.0, 100.0) as u8,
            0,
            0,
        ],
        _ => {
            let [r, g, b, _] = c.to_rgba8(1.0);
            [1, 0, r, g, b, 0, 0, 0]
        }
    }
}

fn encode_fild_payload(fill_id: u32, fill: &FillStyle) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&fill_id.to_le_bytes());
    match fill {
        FillStyle::Uniform { color: c } => {
            p.extend_from_slice(&1u16.to_le_bytes());
            p.extend_from_slice(&encode_cdr_color_bytes(c));
        }
        FillStyle::Fountain { geometry, stops, .. } => {
            p.extend_from_slice(&2u16.to_le_bytes());
            let (f_type, angle_i16) = match geometry {
                FountainGeometry::Radial { .. } => (1u16, 0i16),
                FountainGeometry::Linear { angle_deg, .. } => (0u16, angle_deg.get().round().clamp(-360.0, 360.0) as i16),
            };
            p.extend_from_slice(&f_type.to_le_bytes());
            p.extend_from_slice(&angle_i16.to_le_bytes());
            let c1 = stops.first().map(|s| &s.color).unwrap_or(&Color::WHITE);
            let c2 = stops.last().map(|s| &s.color).unwrap_or(&Color::BLACK);
            p.extend_from_slice(&encode_cdr_color_bytes(c1));
            p.extend_from_slice(&encode_cdr_color_bytes(c2));
        }
        FillStyle::Unknown { .. } => {
            p.extend_from_slice(&99u16.to_le_bytes());
        }
    }
    p
}

fn encode_outl_payload(outl_id: u32, outl: &OutlineStyle) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&outl_id.to_le_bytes());
    let mut flags = 0u32;
    if outl.behind_fill {
        flags |= 0x01;
    }
    if outl.scale_with_object {
        flags |= 0x02;
    }
    flags |= match outl.cap {
        LineCap::Butt => 0,
        LineCap::Round => 1 << 2,
        LineCap::Square => 2 << 2,
    };
    flags |= match outl.join {
        LineJoin::Miter => 0,
        LineJoin::Round => 1 << 4,
        LineJoin::Bevel => 2 << 4,
    };
    p.extend_from_slice(&flags.to_le_bytes());
    p.extend_from_slice(&pt_to_cdr_i32(outl.width_pt.get()).to_le_bytes());
    p.extend_from_slice(&encode_cdr_color_bytes(&outl.color));
    p
}

fn encode_object_into_layer(node: &ObjectNode, page_size: Size, next_style_id: &mut u32, layer_children: &mut Vec<u8>) {
    if let ObjectKind::Group { children } = &node.kind {
        for child in children {
            encode_object_into_layer(child, page_size, next_style_id, layer_children);
        }
        return;
    }

    let mut obj_children = Vec::new();
    let cx = page_size.width.get() * 0.5;
    let cy = page_size.height.get() * 0.5;

    let bbox = node_bbox_for_export(node);
    let mut bbox_payload = Vec::with_capacity(16);
    bbox_payload.extend_from_slice(&pt_to_cdr_i32(bbox.x0.get() - cx).to_le_bytes());
    bbox_payload.extend_from_slice(&pt_to_cdr_i32(cy - bbox.y0.get()).to_le_bytes());
    bbox_payload.extend_from_slice(&pt_to_cdr_i32(bbox.x1.get() - cx).to_le_bytes());
    bbox_payload.extend_from_slice(&pt_to_cdr_i32(cy - bbox.y1.get()).to_le_bytes());
    append_riff_chunk(&mut obj_children, *b"bbox", &bbox_payload);

    if node.transform != Affine2::IDENTITY {
        let mut trfd = Vec::with_capacity(52);
        trfd.extend_from_slice(b"TRPT");
        trfd.extend_from_slice(&node.transform.a.get().to_le_bytes());
        trfd.extend_from_slice(&node.transform.c.get().to_le_bytes());
        trfd.extend_from_slice(&node.transform.tx.get().to_le_bytes());
        trfd.extend_from_slice(&node.transform.b.get().to_le_bytes());
        trfd.extend_from_slice(&node.transform.d.get().to_le_bytes());
        trfd.extend_from_slice(&node.transform.ty.get().to_le_bytes());
        append_riff_chunk(&mut obj_children, *b"trfd", &trfd);
    }

    if let Some(fill) = &node.fill {
        let fid = *next_style_id;
        *next_style_id += 1;
        let fp = encode_fild_payload(fid, fill);
        append_riff_chunk(&mut obj_children, *b"fild", &fp);
    }
    if let Some(outl) = &node.outline {
        let oid = *next_style_id;
        *next_style_id += 1;
        let op = encode_outl_payload(oid, outl);
        append_riff_chunk(&mut obj_children, *b"outl", &op);
    }

    let mut loda = Vec::new();
    match &node.kind {
        ObjectKind::Rect(spec) => {
            if spec.radii.iter().all(|r| r.get() == 0.0) {
                loda.extend_from_slice(&1u32.to_le_bytes());
                loda.extend_from_slice(&bbox_payload);
            } else if let Ok(p) = spec.to_path() {
                encode_path_as_loda_1e(&p, cx, cy, &mut loda);
            }
        }
        ObjectKind::Ellipse(spec) => {
            if spec.mode == coralcraft_geom::EllipseMode::Full {
                loda.extend_from_slice(&2u32.to_le_bytes());
                loda.extend_from_slice(&bbox_payload);
            } else if let Ok(p) = spec.to_path() {
                encode_path_as_loda_1e(&p, cx, cy, &mut loda);
            }
        }
        ObjectKind::Path(path) => {
            encode_path_as_loda_1e(path, cx, cy, &mut loda);
        }
        ObjectKind::Polygon(spec) => {
            loda.extend_from_slice(&4u32.to_le_bytes());
            loda.extend_from_slice(b"POLY");
            loda.extend_from_slice(&spec.points.to_le_bytes());
            loda.extend_from_slice(&pt_to_cdr_i32(spec.center.x.get() - cx).to_le_bytes());
            loda.extend_from_slice(&pt_to_cdr_i32(cy - spec.center.y.get()).to_le_bytes());
            loda.extend_from_slice(&pt_to_cdr_i32(spec.radius.get()).to_le_bytes());
            let inner = spec.inner_radius.map(|r| r.get()).unwrap_or(0.0);
            loda.extend_from_slice(&pt_to_cdr_i32(inner).to_le_bytes());
        }
        ObjectKind::ArtisticText { text, font_family, size_pt, position } => {
            loda.extend_from_slice(&5u32.to_le_bytes());
            loda.extend_from_slice(b"ATXT");
            loda.extend_from_slice(&pt_to_cdr_i32(position.x.get() - cx).to_le_bytes());
            loda.extend_from_slice(&pt_to_cdr_i32(cy - position.y.get()).to_le_bytes());
            loda.extend_from_slice(&pt_to_cdr_i32(size_pt.get()).to_le_bytes());
            let fam_bytes = font_family.as_bytes();
            loda.extend_from_slice(&(fam_bytes.len() as u16).to_le_bytes());
            loda.extend_from_slice(fam_bytes);
            let txt_bytes = text.as_bytes();
            loda.extend_from_slice(&(txt_bytes.len() as u32).to_le_bytes());
            loda.extend_from_slice(txt_bytes);
        }
        ObjectKind::Group { .. } => {}
        ObjectKind::Unsupported(u) => {
            loda.extend_from_slice(&0x99u32.to_le_bytes());
            loda.extend_from_slice(&u.payload.0);
            while loda.len() < 20 {
                loda.push(0);
            }
        }
    }
    append_riff_chunk(&mut obj_children, *b"loda", &loda);
    append_riff_list(layer_children, *b"obj ", &obj_children);
}

fn encode_path_as_loda_1e(path: &Path, cx: f64, cy: f64, loda: &mut Vec<u8>) {
    loda.extend_from_slice(&3u32.to_le_bytes());
    loda.extend_from_slice(b"L1E\0");
    let elems = &path.elements;
    loda.extend_from_slice(&(elems.len() as u32).to_le_bytes());
    let mut cur = Point::default();
    for el in elems {
        match *el {
            coralcraft_geom::PathElement::MoveTo(pt) => {
                loda.push(0x00);
                loda.extend_from_slice(&pt_to_cdr_i32(pt.x.get() - cx).to_le_bytes());
                loda.extend_from_slice(&pt_to_cdr_i32(cy - pt.y.get()).to_le_bytes());
                cur = pt;
            }
            coralcraft_geom::PathElement::LineTo(pt) => {
                loda.push(0x40);
                loda.extend_from_slice(&pt_to_cdr_i32(pt.x.get() - cx).to_le_bytes());
                loda.extend_from_slice(&pt_to_cdr_i32(cy - pt.y.get()).to_le_bytes());
                cur = pt;
            }
            coralcraft_geom::PathElement::QuadTo { ctrl, end } => {
                // Promote quadratic segment to exact cubic Bezier control points
                let c1x = cur.x.get() + (2.0 / 3.0) * (ctrl.x.get() - cur.x.get());
                let c1y = cur.y.get() + (2.0 / 3.0) * (ctrl.y.get() - cur.y.get());
                let c2x = end.x.get() + (2.0 / 3.0) * (ctrl.x.get() - end.x.get());
                let c2y = end.y.get() + (2.0 / 3.0) * (ctrl.y.get() - end.y.get());
                loda.push(0x80);
                loda.extend_from_slice(&pt_to_cdr_i32(c1x - cx).to_le_bytes());
                loda.extend_from_slice(&pt_to_cdr_i32(cy - c1y).to_le_bytes());
                loda.extend_from_slice(&pt_to_cdr_i32(c2x - cx).to_le_bytes());
                loda.extend_from_slice(&pt_to_cdr_i32(cy - c2y).to_le_bytes());
                loda.extend_from_slice(&pt_to_cdr_i32(end.x.get() - cx).to_le_bytes());
                loda.extend_from_slice(&pt_to_cdr_i32(cy - end.y.get()).to_le_bytes());
                cur = end;
            }
            coralcraft_geom::PathElement::CubicTo { c1, c2, end } => {
                loda.push(0x80);
                loda.extend_from_slice(&pt_to_cdr_i32(c1.x.get() - cx).to_le_bytes());
                loda.extend_from_slice(&pt_to_cdr_i32(cy - c1.y.get()).to_le_bytes());
                loda.extend_from_slice(&pt_to_cdr_i32(c2.x.get() - cx).to_le_bytes());
                loda.extend_from_slice(&pt_to_cdr_i32(cy - c2.y.get()).to_le_bytes());
                loda.extend_from_slice(&pt_to_cdr_i32(end.x.get() - cx).to_le_bytes());
                loda.extend_from_slice(&pt_to_cdr_i32(cy - end.y.get()).to_le_bytes());
                cur = end;
            }
            coralcraft_geom::PathElement::Close => {
                loda.push(0x08);
            }
        }
    }
    while loda.len() < 16 {
        loda.push(0);
    }
}

fn node_bbox_for_export(node: &ObjectNode) -> Rect {
    let default_rect = Rect::new(36.0, 36.0, 136.0, 136.0).unwrap_or_default();
    match &node.kind {
        ObjectKind::Rect(r) => r.rect,
        ObjectKind::Ellipse(e) => e.rect,
        ObjectKind::Path(p) => p.bounds().unwrap_or(default_rect),
        ObjectKind::Polygon(p) => p.to_path().ok().and_then(|path| path.bounds()).unwrap_or(default_rect),
        ObjectKind::ArtisticText { text, size_pt, position, .. } => {
            let x0 = position.x.get();
            let y0 = position.y.get();
            let w = (text.chars().count().max(1) as f64) * size_pt.get() * 0.6;
            let h = size_pt.get().max(8.0);
            Rect::new(x0, y0, x0 + w, y0 + h).unwrap_or(default_rect)
        }
        ObjectKind::Group { .. } => default_rect,
        ObjectKind::Unsupported(u) => u.fallback_bounds.unwrap_or(default_rect),
    }
}

fn write_riff_document(doc: &Document, form_type: [u8; 4], version: u16) -> Result<Vec<u8>, ProbeError> {
    let mut body = Vec::new();
    append_riff_chunk(&mut body, *b"vrsn", &version.to_le_bytes());

    let mut next_style_id = 1u32;
    for page in &doc.pages {
        let mut page_children = Vec::new();
        // `mcfg` for version 700 places `(w_cdr, h_cdr)` at byte offset 4
        let mut mcfg = vec![0u8; 12];
        mcfg[4..8].copy_from_slice(&pt_to_cdr_i32(page.size.width.get()).to_le_bytes());
        mcfg[8..12].copy_from_slice(&pt_to_cdr_i32(page.size.height.get()).to_le_bytes());
        append_riff_chunk(&mut page_children, *b"mcfg", &mcfg);

        for layer in &page.layers {
            let mut layer_children = Vec::new();
            for obj in &layer.objects {
                encode_object_into_layer(obj, page.size, &mut next_style_id, &mut layer_children);
            }
            append_riff_list(&mut page_children, *b"layr", &layer_children);
        }
        append_riff_list(&mut body, *b"page", &page_children);
    }

    let riff_size = (4 + body.len()) as u32;
    let mut out = Vec::with_capacity(8 + riff_size as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&riff_size.to_le_bytes());
    out.extend_from_slice(&form_type);
    out.extend_from_slice(&body);
    Ok(out)
}
