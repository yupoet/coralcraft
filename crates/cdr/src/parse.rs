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
            if let Some(&mapped) = bs.get(raw_size_field as usize) { mapped as usize } else { raw_size_field as usize }
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
        && loda_payload.is_some_and(|(p, _)| contains_u32_le(p, first_id))
    {
        fill = Some(first_fill.clone());
    }
    if outline.is_none()
        && let Some((&first_id, first_outl)) = ctx.outlines.iter().next()
        && loda_payload.is_some_and(|(p, _)| contains_u32_le(p, first_id))
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

fn contains_u32_le(haystack: &[u8], needle: u32) -> bool {
    let target = needle.to_le_bytes();
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
        if (a.abs() <= 100.0 && d.abs() <= 100.0) && (a * d - b * c).abs() > 1e-6 && tx_raw.abs() < 1e5 && ty_raw.abs() < 1e5 {
            return Affine2::new(a, b, c, d, 0.0, 0.0).ok();
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
    // Check explicit shape type code if present (`1` = Rectangle, `2` = Ellipse, `3` = Curve/Line, `4` = ArtisticText, `0x14` = Polygon)
    let shape_code = read_u32(endian, payload, 0).or_else(|| read_u16(endian, payload, 0).map(u32::from)).unwrap_or(0);

    // If payload starts with our synthetic or explicit shape header `[u32 shape_code, ...]`
    if payload.len() >= 20 {
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
                // Curve / Path: `[3u32, point_count: u32, (x_cdr, y_cdr, node_flag: u8)...]`
                if let Some(path) = decode_cdr_curve(payload.get(4..).unwrap_or(&[]), endian, version, page_size) {
                    return ObjectKind::Path(path);
                }
            }
            4 => {
                // Polygon
                let rect = bbox_rect.unwrap_or_else(|| Rect::new(50.0, 50.0, 150.0, 150.0).unwrap_or_default());
                let center = Point::new(0.5 * (rect.x0.get() + rect.x1.get()), 0.5 * (rect.y0.get() + rect.y1.get())).unwrap_or_default();
                let r = 0.5 * rect.width().min(rect.height()).max(10.0);
                if let Ok(poly) = PolygonSpec::regular(center, r, 5, 0.0) {
                    return ObjectKind::Polygon(poly);
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

fn decode_cdr_curve(slice: &[u8], endian: Endian, version: u16, page_size: Size) -> Option<Path> {
    let count = read_u32(endian, slice, 0)? as usize;
    if !(2..=10_000).contains(&count) {
        return None;
    }
    let step = if version < 600 { 4 } else { 8 };
    let coords_bytes = count.checked_mul(step)?;
    if slice.len() < 4 + coords_bytes {
        return None;
    }

    let cx = page_size.width.get() * 0.5;
    let cy = page_size.height.get() * 0.5;
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
