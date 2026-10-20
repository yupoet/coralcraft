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
    ParagraphTextSpec, RawChunk, RawPayload, Residual, TextAlignment, UnsupportedObject,
};
use coralcraft_geom::{Affine2, CdrUnit, EllipseSpec, F64, Path, Point, PolygonSpec, Rect, RectSpec, Size, Unit};

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
    while pos.saturating_add(8) <= body_after_cmpr.len() {
        if body_after_cmpr.get(pos..pos.saturating_add(4)) == Some(b"CPng") {
            cpng_positions.push(pos);
            pos = pos.saturating_add(8);
        } else {
            pos = pos.saturating_add(1);
        }
    }

    let Some(&p1_start) = cpng_positions.first() else {
        return Err(ProbeError::Invalid { offset: 0, reason: "no CPng stream header found inside LIST cmpr".to_owned() });
    };
    let p1_end = if c1 >= 8 && p1_start.saturating_add(c1) <= body_after_cmpr.len() {
        p1_start.saturating_add(c1)
    } else if let Some(&second) = cpng_positions.get(1) {
        second
    } else {
        body_after_cmpr.len()
    };

    let part1 = inflate_zlib(body_after_cmpr.get(p1_start..p1_end).unwrap_or_default(), max_output_len)?;

    let mut block_sizes = Vec::new();
    let p2_start_opt = if p1_end.saturating_add(8) <= body_after_cmpr.len() && body_after_cmpr.get(p1_end..p1_end.saturating_add(4)) == Some(b"CPng")
    {
        Some(p1_end)
    } else {
        cpng_positions.iter().copied().find(|&p| p > p1_start)
    };

    if let Some(p2_start) = p2_start_opt {
        let p2_end =
            if c2 >= 8 && p2_start.saturating_add(c2) <= body_after_cmpr.len() { p2_start.saturating_add(c2) } else { body_after_cmpr.len() };
        if let Ok(part2_bytes) = inflate_zlib(body_after_cmpr.get(p2_start..p2_end).unwrap_or_default(), max_output_len) {
            let mut off = 0usize;
            while off.saturating_add(4) <= part2_bytes.len() {
                if let Some(sz) = read_u32(endian, &part2_bytes, off) {
                    block_sizes.push(sz);
                }
                off = off.saturating_add(4);
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
            if let Some(layer) = page.layers.first_mut() {
                layer.objects.push(ObjectNode::new(ObjectId(10), Some("Legacy WL Document".to_owned()), ObjectKind::Unsupported(u), None, None));
            }
            doc.pages.push(page);
            doc.next_id = 100;
            return Ok(ParseOutcome { document: doc, probe_report, parse_warnings });
        }
        CdrContainer::Riff { endian, version, form, .. } => {
            doc.title = format!("CDR ({form})");
            let v = version.effective_version.unwrap_or(1300);
            parse_riff_stream(bytes, *endian, v, &mut doc, &mut parse_warnings);
        }
        CdrContainer::Zip { primary_entry, inner_version, has_data_file_list, .. } => {
            doc.title = format!("CDR ZIP ({primary_entry})");
            match crate::zip::extract_zip_primary_riff(bytes, MAX_DECOMPRESSED_BYTES) {
                Ok((_role, inner_bytes)) => {
                    let endian = if inner_bytes.get(0..4) == Some(b"RIFX") { Endian::Big } else { Endian::Little };
                    let form_v = inner_bytes.get(11).and_then(|&b| crate::decode_cdr_version_char(b));
                    let v = inner_version.and_then(|iv| iv.effective_version).or(form_v).unwrap_or(1400);
                    parse_riff_stream(&inner_bytes, endian, v, &mut doc, &mut parse_warnings);
                    if *has_data_file_list && let Ok(aux_streams) = crate::zip::extract_zip_data_file_streams(bytes, MAX_DECOMPRESSED_BYTES) {
                        for (_name, stream_bytes) in aux_streams {
                            if stream_bytes.get(0..4) == Some(b"RIFF") || stream_bytes.get(0..4) == Some(b"RIFX") {
                                let aux_endian = if stream_bytes.get(0..4) == Some(b"RIFX") { Endian::Big } else { Endian::Little };
                                parse_riff_stream(&stream_bytes, aux_endian, v, &mut doc, &mut parse_warnings);
                            }
                        }
                    }
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

/// Total decompressed bytes allowed across every `LIST cmpr` stream in one file. Each stream is
/// also capped by [`MAX_DECOMPRESSED_BYTES`]; this shared budget stops many small compressed lists
/// from expanding into an unbounded amount of retained memory.
pub const MAX_FILE_DECOMPRESSED_BYTES: usize = 256 * 1024 * 1024;

/// Largest embedded bitmap accepted, in pixels (64 Mpx, 256 MiB of RGBA).
pub const MAX_BITMAP_PIXELS: usize = 64 * 1024 * 1024;

fn default_a4_page(page_num: u64) -> Page {
    let size = Size::new(595.275_590_551_181_2, 841.889_763_779_527_6).unwrap_or_default();
    Page {
        id: PageId(page_num),
        name: format!("Page {page_num}"),
        size,
        bleed: F64::ZERO,
        background: Some(FillStyle::uniform(Color::WHITE)),
        layers: vec![Layer::new_normal(LayerId(2), "Layer 1")],
        residual: Residual::default(),
        extra: BTreeMap::new(),
    }
}

struct ParserCtx {
    endian: Endian,
    version: u16,
    default_page_size: Size,
    fills: BTreeMap<u32, FillStyle>,
    outlines: BTreeMap<u32, OutlineStyle>,
    next_id: u64,
    chunks_seen: usize,
    /// Remaining shared decompression budget for this file.
    decompress_budget: usize,
    /// True while walking the decompressed contents of a `LIST cmpr` (nested `cmpr` stays opaque).
    inside_cmpr: bool,
}

impl ParserCtx {
    fn alloc_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
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
    parse_riff_stream_with_budget(bytes, endian, version, doc, warnings, MAX_FILE_DECOMPRESSED_BYTES);
}

fn parse_riff_stream_with_budget(
    bytes: &[u8],
    endian: Endian,
    version: u16,
    doc: &mut Document,
    warnings: &mut Vec<String>,
    decompress_budget: usize,
) {
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
        decompress_budget,
        inside_cmpr: false,
    };

    let top_chunks = walk_chunk_sequence(bytes.get(12..end).unwrap_or_default(), 12, None, &mut ctx, 0, warnings);
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
    while cursor.saturating_add(8) <= slice.len() {
        if ctx.chunks_seen >= MAX_CHUNKS {
            warnings.push(format!("W-MAX-CHUNKS: chunk count exceeded {MAX_CHUNKS}"));
            break;
        }
        ctx.chunks_seen = ctx.chunks_seen.saturating_add(1);

        let Some(fourcc) = slice.get(cursor..cursor.saturating_add(4)).and_then(|s| <[u8; 4]>::try_from(s).ok()) else {
            break;
        };
        let raw_size_field = read_u32(ctx.endian, slice, cursor.saturating_add(4)).unwrap_or(0);
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

        let payload_start = cursor.saturating_add(8);
        let Some(payload_end) = payload_start.checked_add(chunk_size) else {
            break;
        };
        if payload_end > slice.len() {
            warnings.push(format!(
                "W-TRUNCATED-CHUNK: chunk {} at offset {} declared {} bytes (available {})",
                format_fourcc(fourcc),
                base_offset.saturating_add(cursor as u64),
                chunk_size,
                slice.len().saturating_sub(payload_start)
            ));
            break;
        }

        let payload_slice = slice.get(payload_start..payload_end).unwrap_or_default();
        let src_offset = base_offset.saturating_add(cursor as u64);

        if &fourcc == b"LIST" && payload_slice.len() >= 4 {
            let list_type = payload_slice.get(0..4).and_then(|s| <[u8; 4]>::try_from(s).ok()).unwrap_or(*b"    ");
            let body = payload_slice.get(4..).unwrap_or_default();
            if &list_type == b"cmpr" && ctx.inside_cmpr {
                warnings.push(format!("W-CMPR-NESTED: LIST cmpr at offset {src_offset} inside another LIST cmpr kept opaque"));
                out.push(ParsedChunk {
                    fourcc,
                    list_type: Some(list_type),
                    payload: body.iter().copied().take(1024).collect(),
                    children: Vec::new(),
                    src_offset,
                });
            } else if &list_type == b"cmpr" && ctx.decompress_budget == 0 {
                warnings.push(format!("W-CMPR-BUDGET: LIST cmpr at offset {src_offset} skipped; per-file decompression budget exhausted"));
                out.push(ParsedChunk {
                    fourcc,
                    list_type: Some(list_type),
                    payload: body.iter().copied().take(1024).collect(),
                    children: Vec::new(),
                    src_offset,
                });
            } else if &list_type == b"cmpr" {
                let limit = ctx.decompress_budget.min(MAX_DECOMPRESSED_BYTES);
                match decompress_cmpr_payload(body, ctx.endian, limit) {
                    Ok((part1, part2_sizes)) => {
                        let used = part1.len().saturating_add(part2_sizes.len().saturating_mul(4));
                        ctx.decompress_budget = ctx.decompress_budget.saturating_sub(used);
                        let bs_ref = if part2_sizes.is_empty() { None } else { Some(part2_sizes.as_slice()) };
                        ctx.inside_cmpr = true;
                        let children = walk_chunk_sequence(&part1, src_offset.saturating_add(12), bs_ref, ctx, depth.saturating_add(1), warnings);
                        ctx.inside_cmpr = false;
                        out.push(ParsedChunk { fourcc, list_type: Some(list_type), payload: Vec::new(), children, src_offset });
                    }
                    Err(ProbeError::LimitExceeded { .. }) if limit < MAX_DECOMPRESSED_BYTES => {
                        ctx.decompress_budget = 0;
                        warnings.push(format!("W-CMPR-BUDGET: LIST cmpr at offset {src_offset} exceeds the remaining per-file decompression budget ({limit} bytes); kept opaque"));
                        out.push(ParsedChunk {
                            fourcc,
                            list_type: Some(list_type),
                            payload: body.iter().copied().take(1024).collect(),
                            children: Vec::new(),
                            src_offset,
                        });
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
                let children = walk_chunk_sequence(body, src_offset.saturating_add(12), block_sizes, ctx, depth.saturating_add(1), warnings);
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

    for (slot_idx, ch) in chunks.iter().enumerate() {
        if &ch.fourcc == b"dmta" {
            decode_dmta(&ch.payload, ctx.endian, doc);
        } else if &ch.fourcc == b"plte" {
            if let Ok(pal) = coralcraft_color::load_palette_bytes(&ch.payload) {
                doc.palette = pal;
            }
        } else if &ch.fourcc == b"LIST" && ch.list_type == Some(*b"mstr") {
            doc.master_layers.clear();
            let mut m_layers = Vec::new();
            collect_layer_lists(&ch.children, &mut m_layers);
            for (l_idx, l_chunk) in m_layers.iter().enumerate() {
                let mut layer = Layer::new_normal(LayerId(ctx.alloc_id()), format!("Master Layer {}", l_idx.saturating_add(1)));
                for (slot_idx, sub) in l_chunk.children.iter().enumerate() {
                    if &sub.fourcc == b"lmta" {
                        decode_lmta(&sub.payload, ctx.endian, &mut layer);
                    } else if sub.list_type.is_none() && !sub.payload.is_empty() && sub.payload.len() <= 512 {
                        layer.residual.chunks.push(RawChunk {
                            tag: ChunkTag(sub.fourcc),
                            flags: 0,
                            slot: slot_idx as u32,
                            payload: RawPayload(sub.payload.clone()),
                            src_offset: sub.src_offset,
                        });
                    }
                }
                collect_loose_objects_recursive(&l_chunk.children, ctx, ctx.default_page_size, &mut layer.objects, warnings);
                doc.master_layers.push(layer);
            }
        } else if ch.list_type.is_none()
            && !matches!(
                &ch.fourcc,
                b"vrsn"
                    | b"dmta"
                    | b"plte"
                    | b"mcfg"
                    | b"fild"
                    | b"outl"
                    | b"page"
                    | b"rclr"
                    | b"rpen"
                    | b"rott"
                    | b"rdot"
                    | b"rotl"
                    | b"ccmm"
                    | b"cont"
                    | b"indx"
            )
            && !ch.payload.is_empty()
            && ch.payload.len() <= 512
        {
            doc.residual.chunks.push(RawChunk {
                tag: ChunkTag(ch.fourcc),
                flags: 0,
                slot: slot_idx as u32,
                payload: RawPayload(ch.payload.clone()),
                src_offset: ch.src_offset,
            });
        }
    }

    // Pass 2: For CMX containers (`cont` + `page` instruction streams), decode CMX `page` chunks directly;
    // otherwise extract `LIST page` chunks into `Page`s.
    let is_cmx_stream = chunks.iter().any(|ch| &ch.fourcc == b"cont") && chunks.iter().any(|ch| &ch.fourcc == b"page" && ch.payload.len() >= 8);
    if is_cmx_stream {
        let style_tables = parse_cmx_style_tables(chunks, ctx.endian);
        for (idx, ch) in chunks.iter().filter(|ch| &ch.fourcc == b"page" && ch.payload.len() >= 8).enumerate() {
            let pnum = (idx.saturating_add(1)) as u64;
            let mut cpage = default_a4_page(pnum);
            cpage.size = ctx.default_page_size;
            parse_single_cmx_page_chunk(&ch.payload, &style_tables, ctx, &mut cpage);
            doc.pages.push(cpage);
        }
    }

    let mut raw_pages: Vec<&ParsedChunk> = Vec::new();
    if !is_cmx_stream {
        collect_page_lists_recursive(chunks, &mut raw_pages);
    }

    for (idx, page_chunk) in raw_pages.iter().enumerate() {
        // When a CDR file has >= 2 `LIST page` blocks, the first `LIST page` (index 0) is CorelDRAW's Master Page
        // if it has no drawable objects or only Master/Grid/Guides layers; we still include all numbered pages.
        let page_num = (doc.pages.len().saturating_add(1)) as u64;
        let mut page = default_a4_page(page_num);
        page.name.clear();
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
        let final_num = (doc.pages.len().saturating_add(1)) as u64;
        page.id = PageId(final_num);
        if page.name.is_empty() {
            page.name = format!("Page {final_num}");
        }
        doc.pages.push(page);
    }

    if doc.pages.is_empty() {
        let mut page = default_a4_page(1);
        page.size = ctx.default_page_size;
        // Check if any `LIST obj ` chunks exist directly in the stream outside `LIST page`
        let mut loose_objs = Vec::new();
        collect_loose_objects_recursive(chunks, ctx, page.size, &mut loose_objs, warnings);
        if !loose_objs.is_empty() {
            if let Some(layer) = page.layers.first_mut() {
                layer.objects.extend(loose_objs);
            }
            doc.pages.push(page);
        } else {
            // Also check for standalone CMX `page` instruction streams when no `LIST page` is present
            let style_tables = parse_cmx_style_tables(chunks, ctx.endian);
            let cmx_page_chunks: Vec<&ParsedChunk> = chunks.iter().filter(|ch| &ch.fourcc == b"page" && ch.payload.len() >= 8).collect();
            if cmx_page_chunks.is_empty() {
                doc.pages.push(page);
            } else {
                for (idx, ch) in cmx_page_chunks.iter().enumerate() {
                    let pnum = (idx.saturating_add(1)) as u64;
                    let mut cpage = default_a4_page(pnum);
                    cpage.size = ctx.default_page_size;
                    parse_single_cmx_page_chunk(&ch.payload, &style_tables, ctx, &mut cpage);
                    doc.pages.push(cpage);
                }
            }
        }
    }
}

type CmxStyleTables = (Vec<Color>, Vec<f64>, Vec<(u8, u8)>, Vec<Vec<u16>>, Vec<(u16, u16, u16, u16)>);

fn parse_cmx_style_tables(chunks: &[ParsedChunk], endian: Endian) -> CmxStyleTables {
    let mut colors = Vec::new();
    let mut pens_pt = Vec::new();
    let mut line_styles = Vec::new();
    let mut dots = Vec::new();
    let mut outlines = Vec::new();

    for ch in chunks {
        match &ch.fourcc {
            b"rclr" if ch.payload.len() >= 2 => {
                let count = (read_u16(endian, &ch.payload, 0).unwrap_or(0) as usize).min(4096);
                let mut pos = 2usize;
                for _ in 0..count {
                    let mut model = 5u8;
                    let mut col = Color::BLACK;
                    while pos < ch.payload.len() {
                        let Some(&tag_id) = ch.payload.get(pos) else { break };
                        if tag_id == 255 {
                            pos = pos.saturating_add(1);
                            break;
                        }
                        if pos.saturating_add(3) > ch.payload.len() {
                            break;
                        }
                        let tag_len = (read_u16(endian, &ch.payload, pos.saturating_add(1)).unwrap_or(3) as usize).max(3);
                        let tag_end = pos.saturating_add(tag_len).min(ch.payload.len());
                        if tag_id == 1 && pos.saturating_add(4) <= tag_end {
                            model = ch.payload.get(pos.saturating_add(3)).copied().unwrap_or(5);
                        } else if tag_id == 2 && pos.saturating_add(7) <= tag_end {
                            let c0 = ch.payload.get(pos.saturating_add(3)).copied().unwrap_or(0);
                            let c1 = ch.payload.get(pos.saturating_add(4)).copied().unwrap_or(0);
                            let c2 = ch.payload.get(pos.saturating_add(5)).copied().unwrap_or(0);
                            let c3 = ch.payload.get(pos.saturating_add(6)).copied().unwrap_or(0);
                            match model {
                                1 => {
                                    let density = read_u16(endian, &ch.payload, pos.saturating_add(5)).unwrap_or(100);
                                    let tint = (f64::from(density) / 100.0).clamp(0.0, 1.0);
                                    if let Ok(sp) = Color::spot("Spot", tint, [0.0, 0.0, 0.0, 1.0]) {
                                        col = sp;
                                    }
                                }
                                2 => {
                                    if let Ok(cmyk) = Color::cmyk(
                                        (f64::from(c0) / 100.0).clamp(0.0, 1.0),
                                        (f64::from(c1) / 100.0).clamp(0.0, 1.0),
                                        (f64::from(c2) / 100.0).clamp(0.0, 1.0),
                                        (f64::from(c3) / 100.0).clamp(0.0, 1.0),
                                    ) {
                                        col = cmyk;
                                    }
                                }
                                3 => {
                                    if let Ok(cmyk) = Color::cmyk(
                                        (f64::from(c0) / 255.0).clamp(0.0, 1.0),
                                        (f64::from(c1) / 255.0).clamp(0.0, 1.0),
                                        (f64::from(c2) / 255.0).clamp(0.0, 1.0),
                                        (f64::from(c3) / 255.0).clamp(0.0, 1.0),
                                    ) {
                                        col = cmyk;
                                    }
                                }
                                6 => {
                                    let h_u16 = read_u16(endian, &ch.payload, pos.saturating_add(3)).unwrap_or(0);
                                    if let Ok(hsb) = Color::hsb(
                                        f64::from(h_u16).clamp(0.0, 360.0),
                                        (f64::from(c2) / 255.0).clamp(0.0, 1.0),
                                        (f64::from(c3) / 255.0).clamp(0.0, 1.0),
                                    ) {
                                        col = hsb;
                                    }
                                }
                                8 => {
                                    col = Color::Registration;
                                }
                                9 => {
                                    if let Ok(g) = Color::gray((1.0 - f64::from(c0) / 255.0).clamp(0.0, 1.0)) {
                                        col = g;
                                    }
                                }
                                _ => {
                                    col = Color::rgb8(c0, c1, c2);
                                }
                            }
                        } else if tag_id == 10 && pos.saturating_add(7) <= tag_end {
                            let body = ch.payload.get(pos.saturating_add(3)..tag_end).unwrap_or_default();
                            match body.get(0..4) {
                                Some(b"CMYK") if body.len() >= 36 => {
                                    let c = read_f64(endian, body, 4).unwrap_or(0.0);
                                    let m = read_f64(endian, body, 12).unwrap_or(0.0);
                                    let y = read_f64(endian, body, 20).unwrap_or(0.0);
                                    let k = read_f64(endian, body, 28).unwrap_or(0.0);
                                    if let Ok(cmyk) = Color::cmyk(c, m, y, k) {
                                        col = cmyk;
                                    }
                                }
                                Some(b"HSB\0") if body.len() >= 28 => {
                                    let h = read_f64(endian, body, 4).unwrap_or(0.0);
                                    let s = read_f64(endian, body, 12).unwrap_or(0.0);
                                    let b = read_f64(endian, body, 20).unwrap_or(0.0);
                                    if let Ok(hsb) = Color::hsb(h, s, b) {
                                        col = hsb;
                                    }
                                }
                                Some(b"GRAY") if body.len() >= 12 => {
                                    let k = read_f64(endian, body, 4).unwrap_or(0.0);
                                    if let Ok(g) = Color::gray(k) {
                                        col = g;
                                    }
                                }
                                Some(b"REGI") => {
                                    col = Color::Registration;
                                }
                                Some(b"SPOT") if body.len() >= 46 => {
                                    let tint = read_f64(endian, body, 4).unwrap_or(1.0);
                                    let fc = read_f64(endian, body, 12).unwrap_or(0.0);
                                    let fm = read_f64(endian, body, 20).unwrap_or(0.0);
                                    let fy = read_f64(endian, body, 28).unwrap_or(0.0);
                                    let fk = read_f64(endian, body, 36).unwrap_or(0.0);
                                    let nlen = usize::from(read_u16(endian, body, 44).unwrap_or(0));
                                    let nend = 46usize.saturating_add(nlen).min(body.len());
                                    if let Some(nslice) = body.get(46..nend)
                                        && let Ok(nstr) = std::str::from_utf8(nslice)
                                        && let Ok(sp) = Color::spot(nstr, tint, [fc, fm, fy, fk])
                                    {
                                        col = sp;
                                    }
                                }
                                _ => {}
                            }
                        }
                        pos = pos.saturating_add(tag_len);
                    }
                    colors.push(col);
                }
            }
            b"rott" if ch.payload.len() >= 2 => {
                let count = (read_u16(endian, &ch.payload, 0).unwrap_or(0) as usize).min(4096);
                let mut pos = 2usize;
                for _ in 0..count {
                    let mut spec = 0x02u8;
                    let mut cap_and_join = 0x00u8;
                    while pos < ch.payload.len() {
                        let Some(&tag_id) = ch.payload.get(pos) else { break };
                        if tag_id == 255 {
                            pos = pos.saturating_add(1);
                            break;
                        }
                        if pos.saturating_add(3) > ch.payload.len() {
                            break;
                        }
                        let tag_len = (read_u16(endian, &ch.payload, pos.saturating_add(1)).unwrap_or(3) as usize).max(3);
                        if tag_id == 1 && pos.saturating_add(5) <= ch.payload.len() {
                            spec = ch.payload.get(pos.saturating_add(3)).copied().unwrap_or(0x02);
                            cap_and_join = ch.payload.get(pos.saturating_add(4)).copied().unwrap_or(0x00);
                        }
                        pos = pos.saturating_add(tag_len);
                    }
                    line_styles.push((spec, cap_and_join));
                }
            }
            b"rdot" if ch.payload.len() >= 2 => {
                let count = (read_u16(endian, &ch.payload, 0).unwrap_or(0) as usize).min(4096);
                let mut pos = 2usize;
                for _ in 0..count {
                    let mut arr = Vec::new();
                    while pos < ch.payload.len() {
                        let Some(&tag_id) = ch.payload.get(pos) else { break };
                        if tag_id == 255 {
                            pos = pos.saturating_add(1);
                            break;
                        }
                        if pos.saturating_add(3) > ch.payload.len() {
                            break;
                        }
                        let tag_len = (read_u16(endian, &ch.payload, pos.saturating_add(1)).unwrap_or(3) as usize).max(3);
                        if tag_id == 1 && pos.saturating_add(5) <= ch.payload.len() {
                            let dot_count = (read_u16(endian, &ch.payload, pos.saturating_add(3)).unwrap_or(0) as usize).min(11);
                            for idx in 0..dot_count {
                                let off = pos.saturating_add(5).saturating_add(idx.saturating_mul(2));
                                if let Some(v) = read_u16(endian, &ch.payload, off)
                                    && v > 0
                                {
                                    arr.push(v);
                                }
                            }
                        }
                        pos = pos.saturating_add(tag_len);
                    }
                    dots.push(arr);
                }
            }
            b"rpen" if ch.payload.len() >= 2 => {
                let count = (read_u16(endian, &ch.payload, 0).unwrap_or(0) as usize).min(4096);
                let mut pos = 2usize;
                for _ in 0..count {
                    let mut w_pt = 1.0_f64;
                    while pos < ch.payload.len() {
                        let Some(&tag_id) = ch.payload.get(pos) else { break };
                        if tag_id == 255 {
                            pos = pos.saturating_add(1);
                            break;
                        }
                        if pos.saturating_add(3) > ch.payload.len() {
                            break;
                        }
                        let tag_len = (read_u16(endian, &ch.payload, pos.saturating_add(1)).unwrap_or(3) as usize).max(3);
                        if tag_id == 1 && pos.saturating_add(7) <= ch.payload.len() {
                            let w_cdr = read_i32(endian, &ch.payload, pos.saturating_add(3)).unwrap_or(0);
                            let raw_pt = CdrUnit(i64::from(w_cdr)).to_pt().get().abs().clamp(0.1, 288.0);
                            w_pt = (raw_pt * 1000.0).round() / 1000.0;
                        }
                        pos = pos.saturating_add(tag_len);
                    }
                    pens_pt.push(w_pt);
                }
            }
            b"rotl" if ch.payload.len() >= 2 => {
                let count = (read_u16(endian, &ch.payload, 0).unwrap_or(0) as usize).min(4096);
                let mut pos = 2usize;
                for _ in 0..count {
                    let mut ls_idx = 1u16;
                    let mut c_idx = 1u16;
                    let mut p_idx = 1u16;
                    let mut dash_idx = 0u16;
                    while pos < ch.payload.len() {
                        let Some(&tag_id) = ch.payload.get(pos) else { break };
                        if tag_id == 255 {
                            pos = pos.saturating_add(1);
                            break;
                        }
                        if pos.saturating_add(3) > ch.payload.len() {
                            break;
                        }
                        let tag_len = (read_u16(endian, &ch.payload, pos.saturating_add(1)).unwrap_or(3) as usize).max(3);
                        if tag_id == 1 && pos.saturating_add(15) <= ch.payload.len() {
                            ls_idx = read_u16(endian, &ch.payload, pos.saturating_add(3)).unwrap_or(1);
                            c_idx = read_u16(endian, &ch.payload, pos.saturating_add(7)).unwrap_or(1);
                            p_idx = read_u16(endian, &ch.payload, pos.saturating_add(11)).unwrap_or(1);
                            dash_idx = read_u16(endian, &ch.payload, pos.saturating_add(13)).unwrap_or(0);
                        }
                        pos = pos.saturating_add(tag_len);
                    }
                    outlines.push((ls_idx, c_idx, p_idx, dash_idx));
                }
            }
            _ => {}
        }
    }
    (colors, pens_pt, line_styles, dots, outlines)
}

type CmxParsedMeta = (Option<(Option<String>, bool, bool, F64)>, Option<Affine2>, Option<coralcraft_doc::TransparencyStyle>, Vec<u8>);

fn decode_cmx_meta_tag(slice: &[u8], endian: Endian, version: u16, warnings: &mut Vec<String>) -> Option<CmxParsedMeta> {
    if slice.len() < 8 || slice.get(0..4) != Some(b"CCMX") {
        return None;
    }
    let omta_len = usize::from(read_u16(endian, slice, 4)?);
    let omta_end = 6usize.checked_add(omta_len)?;
    let omta = slice.get(6..omta_end).and_then(|s| decode_omta(s, endian));
    let has_trfd = *slice.get(omta_end)?;
    let mut cur = omta_end.saturating_add(1);
    let trfd = if has_trfd == 1 && cur.saturating_add(48) <= slice.len() {
        let a = read_f64(endian, slice, cur)?;
        let b = read_f64(endian, slice, cur.saturating_add(8))?;
        let c = read_f64(endian, slice, cur.saturating_add(16))?;
        let d = read_f64(endian, slice, cur.saturating_add(24))?;
        let tx = read_f64(endian, slice, cur.saturating_add(32))?;
        let ty = read_f64(endian, slice, cur.saturating_add(40))?;
        cur = cur.saturating_add(48);
        Affine2::new(a, b, c, d, tx, ty).ok()
    } else {
        None
    };
    let has_trns = *slice.get(cur)?;
    cur = cur.saturating_add(1);
    let trns = if has_trns == 1 {
        let trns_len = usize::from(read_u16(endian, slice, cur)?);
        let trns_start = cur.saturating_add(2);
        let trns_end = trns_start.checked_add(trns_len)?;
        cur = trns_end;
        slice.get(trns_start..trns_end).and_then(|s| decode_trns(s, endian, version, warnings))
    } else {
        None
    };
    let shape_len = usize::from(read_u16(endian, slice, cur)?);
    let shape_start = cur.saturating_add(2);
    let shape_end = shape_start.checked_add(shape_len)?.min(slice.len());
    let shape_meta = slice.get(shape_start..shape_end).unwrap_or_default().to_vec();
    Some((omta, trfd, trns, shape_meta))
}

fn parse_single_cmx_page_chunk(payload: &[u8], tables: &CmxStyleTables, ctx: &mut ParserCtx, page: &mut Page) {
    let (cmx_colors, cmx_pens, cmx_line_styles, cmx_dots, cmx_outlines) = tables;
    let mut pos = 0usize;
    let mut group_stack: Vec<(Vec<ObjectNode>, Option<CmxParsedMeta>)> = Vec::new();
    let mut has_explicit_layers = false;
    while pos.saturating_add(4) <= payload.len() {
        let sz_i16 = read_i16(ctx.endian, payload, pos).unwrap_or(0);
        let (inst_size, header_len) = if sz_i16 < 0 {
            let sz_i32 = read_i32(ctx.endian, payload, pos.saturating_add(2)).unwrap_or(0);
            (sz_i32.unsigned_abs() as usize, 8usize)
        } else {
            (sz_i16 as usize, 4usize)
        };
        if inst_size < header_len || pos.saturating_add(inst_size) > payload.len() {
            break;
        }
        let code_off = if sz_i16 < 0 { pos.saturating_add(6) } else { pos.saturating_add(2) };
        let code = read_i16(ctx.endian, payload, code_off).unwrap_or(0).unsigned_abs();
        if code == 9 {
            // CMX_Command_BeginPage
            let tag_start = pos.saturating_add(header_len);
            if tag_start.saturating_add(25) <= pos.saturating_add(inst_size) && payload.get(tag_start) == Some(&1) {
                let x0 = read_i32(ctx.endian, payload, tag_start.saturating_add(9)).unwrap_or(0);
                let y0 = read_i32(ctx.endian, payload, tag_start.saturating_add(13)).unwrap_or(0);
                let x1 = read_i32(ctx.endian, payload, tag_start.saturating_add(17)).unwrap_or(0);
                let y1 = read_i32(ctx.endian, payload, tag_start.saturating_add(21)).unwrap_or(0);
                let w_pt = ((CdrUnit(i64::from(x1).saturating_sub(i64::from(x0))).to_pt().get().abs()) * 1000.0).round() / 1000.0;
                let h_pt = ((CdrUnit(i64::from(y0).saturating_sub(i64::from(y1))).to_pt().get().abs()) * 1000.0).round() / 1000.0;
                if w_pt > 1.0
                    && h_pt > 1.0
                    && let Ok(sz) = Size::new(w_pt, h_pt)
                {
                    page.size = sz;
                }
            }
        } else if code == 11 {
            // CMX_Command_BeginLayer
            let tag_start = pos.saturating_add(header_len);
            if tag_start.saturating_add(17) <= pos.saturating_add(inst_size) && payload.get(tag_start) == Some(&1) {
                let flags = read_u32(ctx.endian, payload, tag_start.saturating_add(7)).unwrap_or(0x03);
                let name_len = usize::from(read_u16(ctx.endian, payload, tag_start.saturating_add(15)).unwrap_or(0));
                let name_start = tag_start.saturating_add(17);
                let name_end = name_start.saturating_add(name_len).min(pos.saturating_add(inst_size));
                let layer_name = payload
                    .get(name_start..name_end)
                    .and_then(|s| std::str::from_utf8(s).ok())
                    .filter(|s| !s.is_empty())
                    .map_or_else(|| format!("Layer {}", page.layers.len().saturating_add(1)), str::to_owned);
                if !has_explicit_layers {
                    has_explicit_layers = true;
                    page.layers.clear();
                }
                let mut lyr = Layer::new_normal(LayerId(ctx.alloc_id()), layer_name);
                lyr.visible = (flags & 0x01) != 0;
                lyr.printable = (flags & 0x02) != 0;
                lyr.locked = (flags & 0x04) != 0;
                page.layers.push(lyr);
            }
        } else if (code == 13 || code == 14) && inst_size >= 10 {
            // CMX_Command_BeginGroup
            let mut tag_pos = pos.saturating_add(header_len);
            let inst_end = pos.saturating_add(inst_size);
            let mut grp_meta: Option<CmxParsedMeta> = None;
            while tag_pos < inst_end {
                let Some(&tag_id) = payload.get(tag_pos) else { break };
                if tag_id == 255 || tag_pos.saturating_add(3) > inst_end {
                    break;
                }
                let tag_len = (read_u16(ctx.endian, payload, tag_pos.saturating_add(1)).unwrap_or(3) as usize).max(3);
                if tag_id == 99 && tag_pos.saturating_add(tag_len) <= inst_end {
                    let s = payload.get(tag_pos.saturating_add(3)..tag_pos.saturating_add(tag_len)).unwrap_or_default();
                    grp_meta = decode_cmx_meta_tag(s, ctx.endian, 700, &mut Vec::new());
                }
                tag_pos = tag_pos.saturating_add(tag_len);
            }
            if group_stack.len() < 64 {
                group_stack.push((Vec::new(), grp_meta));
            }
        } else if code == 15 || (code == 14 && inst_size < 10) {
            // CMX_Command_EndGroup
            if let Some((children, grp_meta)) = group_stack.pop()
                && !children.is_empty()
            {
                let grp_id = ObjectId(ctx.alloc_id());
                let mut grp_node = ObjectNode::new(grp_id, Some("Group".to_owned()), ObjectKind::Group { children }, None, None);
                if let Some((omta, trfd, trns, _)) = grp_meta {
                    if let Some((name, visible, locked, opacity)) = omta {
                        if name.is_some() {
                            grp_node.name = name;
                        }
                        grp_node.visible = visible;
                        grp_node.locked = locked;
                        grp_node.opacity = opacity;
                    }
                    if let Some(tf) = trfd {
                        grp_node.transform = tf;
                    }
                    if let Some(t) = trns {
                        grp_node.transparency = Some(t);
                    }
                }
                if let Some((parent, _)) = group_stack.last_mut() {
                    parent.push(grp_node);
                } else if let Some(layer) = page.layers.last_mut() {
                    layer.objects.push(grp_node);
                }
            }
        } else if matches!(code, 66..=68) {
            // CMX_Command_Ellipse (66), CMX_Command_PolyCurve (67), CMX_Command_Rectangle (68)
            let mut tag_pos = pos.saturating_add(header_len);
            let inst_end = pos.saturating_add(inst_size);
            let mut decoded_path: Option<Path> = None;
            let mut decoded_kind: Option<ObjectKind> = None;
            let mut decoded_fill: Option<FillStyle> = None;
            let mut decoded_outline: Option<OutlineStyle> = None;
            let mut decoded_transparency: Option<coralcraft_doc::TransparencyStyle> = None;
            let mut decoded_meta: Option<CmxParsedMeta> = None;
            let mut decoded_bbox: Option<Rect> = None;
            let mut has_render_attr = false;
            while tag_pos < inst_end {
                let Some(&tag_id) = payload.get(tag_pos) else {
                    break;
                };
                if tag_id == 255 || tag_pos.saturating_add(3) > inst_end {
                    break;
                }
                let tag_len = (read_u16(ctx.endian, payload, tag_pos.saturating_add(1)).unwrap_or(3) as usize).max(3);
                if tag_id == 1 && tag_pos.saturating_add(tag_len) <= inst_end {
                    has_render_attr = true;
                    let attr_slice = payload.get(tag_pos.saturating_add(3)..tag_pos.saturating_add(tag_len)).unwrap_or_default();
                    if let Some(&mask) = attr_slice.first() {
                        let mut sub_pos = 1usize;
                        if (mask & 0x01) != 0 && sub_pos.saturating_add(3) <= attr_slice.len() {
                            let fill_spec_len = (read_u16(ctx.endian, attr_slice, sub_pos.saturating_add(1)).unwrap_or(3) as usize).max(3);
                            let fill_type = read_u16(ctx.endian, attr_slice, sub_pos.saturating_add(3)).unwrap_or(0);
                            if fill_type == 1 && sub_pos.saturating_add(10) <= attr_slice.len() {
                                let c_idx = read_u16(ctx.endian, attr_slice, sub_pos.saturating_add(8)).unwrap_or(1) as usize;
                                if let Some(col) = cmx_colors.get(c_idx.saturating_sub(1)) {
                                    decoded_fill = Some(FillStyle::uniform(col.clone()));
                                }
                            } else if fill_type == 2 && sub_pos.saturating_add(fill_spec_len) <= attr_slice.len() {
                                let ftn_end = sub_pos.saturating_add(fill_spec_len);
                                let mut ftn_pos = sub_pos.saturating_add(5);
                                let mut f_type = 0u16;
                                let mut pad_pct = 0u16;
                                let mut angle_micro = 0i32;
                                let mut x_off = 0i32;
                                let mut y_off = 0i32;
                                let mut mode_raw = 0u16;
                                let mut stops = Vec::new();
                                while ftn_pos < ftn_end {
                                    let Some(&ftn_tag) = attr_slice.get(ftn_pos) else {
                                        break;
                                    };
                                    if ftn_tag == 255 || ftn_pos.saturating_add(3) > ftn_end {
                                        break;
                                    }
                                    let ftn_len = (read_u16(ctx.endian, attr_slice, ftn_pos.saturating_add(1)).unwrap_or(3) as usize).max(3);
                                    if ftn_tag == 1 && ftn_pos.saturating_add(25) <= ftn_end {
                                        f_type = read_u16(ctx.endian, attr_slice, ftn_pos.saturating_add(3)).unwrap_or(0);
                                        pad_pct = read_u16(ctx.endian, attr_slice, ftn_pos.saturating_add(7)).unwrap_or(0);
                                        angle_micro = read_i32(ctx.endian, attr_slice, ftn_pos.saturating_add(9)).unwrap_or(0);
                                        x_off = read_i32(ctx.endian, attr_slice, ftn_pos.saturating_add(13)).unwrap_or(0);
                                        y_off = read_i32(ctx.endian, attr_slice, ftn_pos.saturating_add(17)).unwrap_or(0);
                                        mode_raw = read_u16(ctx.endian, attr_slice, ftn_pos.saturating_add(23)).unwrap_or(0);
                                    } else if ftn_tag == 2 && ftn_pos.saturating_add(5) <= ftn_end {
                                        let color_count = (read_u16(ctx.endian, attr_slice, ftn_pos.saturating_add(3)).unwrap_or(0) as usize).min(64);
                                        for idx in 0..color_count {
                                            let base = ftn_pos.saturating_add(5).saturating_add(idx.saturating_mul(4));
                                            if base.saturating_add(4) > ftn_end {
                                                break;
                                            }
                                            let c_idx = read_u16(ctx.endian, attr_slice, base).unwrap_or(1) as usize;
                                            let pos_pct = read_u16(ctx.endian, attr_slice, base.saturating_add(2)).unwrap_or(0);
                                            let col = cmx_colors.get(c_idx.saturating_sub(1)).cloned().unwrap_or(Color::BLACK);
                                            let off = (f64::from(pos_pct) / 100.0).clamp(0.0, 1.0);
                                            if let Ok(st) = FountainStop::new(off, col) {
                                                stops.push(st);
                                            }
                                        }
                                    }
                                    ftn_pos = ftn_pos.saturating_add(ftn_len);
                                }
                                if stops.len() >= 2 {
                                    let angle_deg = (f64::from(angle_micro) / 1_000_000.0).clamp(-360.0, 360.0);
                                    let cx_f = F64::new((0.5 + f64::from(x_off) / 100.0).clamp(0.0, 1.0)).unwrap_or(F64::HALF);
                                    let cy_f = F64::new((0.5 - f64::from(y_off) / 100.0).clamp(0.0, 1.0)).unwrap_or(F64::HALF);
                                    let geometry = match f_type {
                                        1 => FountainGeometry::Radial {
                                            center_x: cx_f,
                                            center_y: cy_f,
                                            radius: F64::HALF,
                                            focal_x: cx_f,
                                            focal_y: cy_f,
                                        },
                                        2 => FountainGeometry::Conical {
                                            center_x: cx_f,
                                            center_y: cy_f,
                                            angle_deg: F64::new(angle_deg).unwrap_or(F64::ZERO),
                                        },
                                        3 => FountainGeometry::Square {
                                            center_x: cx_f,
                                            center_y: cy_f,
                                            radius: F64::HALF,
                                            angle_deg: F64::new(angle_deg).unwrap_or(F64::ZERO),
                                        },
                                        _ => FountainGeometry::Linear {
                                            angle_deg: F64::new(angle_deg).unwrap_or(F64::ZERO),
                                            edge_pad: F64::new(f64::from(pad_pct.min(49))).unwrap_or(F64::ZERO),
                                        },
                                    };
                                    let blend_mode = match mode_raw {
                                        1 => FountainBlendMode::RainbowCw,
                                        2 => FountainBlendMode::RainbowCcw,
                                        _ => FountainBlendMode::Direct,
                                    };
                                    decoded_fill = Some(FillStyle::Fountain { geometry, blend_mode, stops });
                                }
                            } else if fill_type == 7 && sub_pos.saturating_add(fill_spec_len) <= attr_slice.len() {
                                let mono_end = sub_pos.saturating_add(fill_spec_len);
                                let mut mono_pos = sub_pos.saturating_add(5);
                                while mono_pos < mono_end {
                                    let Some(&mono_tag) = attr_slice.get(mono_pos) else {
                                        break;
                                    };
                                    if mono_tag == 255 || mono_pos.saturating_add(3) > mono_end {
                                        break;
                                    }
                                    let mono_len = (read_u16(ctx.endian, attr_slice, mono_pos.saturating_add(1)).unwrap_or(3) as usize).max(3);
                                    if mono_tag == 1 && mono_pos.saturating_add(25) <= mono_end {
                                        let bm_idx = read_u16(ctx.endian, attr_slice, mono_pos.saturating_add(3)).unwrap_or(1);
                                        let w_cdr = read_i32(ctx.endian, attr_slice, mono_pos.saturating_add(5)).unwrap_or(254_000);
                                        let fg_idx = read_u16(ctx.endian, attr_slice, mono_pos.saturating_add(21)).unwrap_or(1) as usize;
                                        let bg_idx = read_u16(ctx.endian, attr_slice, mono_pos.saturating_add(23)).unwrap_or(1) as usize;
                                        let kind = match bm_idx {
                                            2 => coralcraft_doc::PatternKind::PolkaDot,
                                            3 => coralcraft_doc::PatternKind::HorizontalStripe,
                                            4 => coralcraft_doc::PatternKind::VerticalStripe,
                                            5 => coralcraft_doc::PatternKind::DiagonalCrosshatch,
                                            _ => coralcraft_doc::PatternKind::Checkerboard,
                                        };
                                        let fg_color = cmx_colors.get(fg_idx.saturating_sub(1)).cloned().unwrap_or(Color::BLACK);
                                        let bg_color = cmx_colors.get(bg_idx.saturating_sub(1)).cloned().unwrap_or(Color::WHITE);
                                        let raw_pt = CdrUnit(i64::from(w_cdr)).to_pt().get().abs().clamp(1.0, 1000.0);
                                        let tile_size_pt = F64::new((raw_pt * 1000.0).round() / 1000.0).unwrap_or(F64::ONE);
                                        decoded_fill = Some(FillStyle::TwoColorPattern { kind, fg_color, bg_color, tile_size_pt });
                                    }
                                    mono_pos = mono_pos.saturating_add(mono_len);
                                }
                            }
                            sub_pos = sub_pos.saturating_add(fill_spec_len);
                            if attr_slice.get(sub_pos) == Some(&255) {
                                sub_pos = sub_pos.saturating_add(1);
                            }
                        }
                        if (mask & 0x02) != 0 && sub_pos.saturating_add(5) <= attr_slice.len() {
                            let outl_spec_len = (read_u16(ctx.endian, attr_slice, sub_pos.saturating_add(1)).unwrap_or(5) as usize).max(3);
                            let o_idx = read_u16(ctx.endian, attr_slice, sub_pos.saturating_add(3)).unwrap_or(1) as usize;
                            if let Some(&(ls_idx, c_idx, p_idx, dash_idx)) = cmx_outlines.get(o_idx.saturating_sub(1)) {
                                let col = cmx_colors.get(usize::from(c_idx).saturating_sub(1)).cloned().unwrap_or(Color::BLACK);
                                let w_pt = cmx_pens.get(usize::from(p_idx).saturating_sub(1)).copied().unwrap_or(1.0);
                                let mut outl = OutlineStyle::with_width(col, F64::new(w_pt).unwrap_or(F64::ONE));
                                if let Some(&(spec, cap_and_join)) = cmx_line_styles.get(usize::from(ls_idx).saturating_sub(1)) {
                                    outl.behind_fill = (spec & 0x08) != 0;
                                    outl.scale_with_object = (spec & 0x20) != 0;
                                    outl.cap = match cap_and_join & 0x0f {
                                        1 => LineCap::Round,
                                        2 => LineCap::Square,
                                        _ => LineCap::Butt,
                                    };
                                    outl.join = match (cap_and_join >> 4) & 0x0f {
                                        1 => LineJoin::Round,
                                        2 => LineJoin::Bevel,
                                        _ => LineJoin::Miter,
                                    };
                                    if (spec & 0x04) != 0
                                        && dash_idx > 0
                                        && let Some(arr) = cmx_dots.get(usize::from(dash_idx).saturating_sub(1))
                                    {
                                        let base_w = w_pt.max(0.25);
                                        outl.dash_pattern =
                                            arr.iter().filter_map(|&m| F64::new(((f64::from(m) * base_w) * 1000.0).round() / 1000.0).ok()).collect();
                                    }
                                }
                                decoded_outline = Some(outl);
                            } else {
                                decoded_outline = Some(OutlineStyle::hairline(Color::BLACK));
                            }
                            sub_pos = sub_pos.saturating_add(outl_spec_len);
                            if attr_slice.get(sub_pos) == Some(&255) {
                                sub_pos = sub_pos.saturating_add(1);
                            }
                        }
                        if (mask & 0x04) != 0 {
                            while sub_pos < attr_slice.len() {
                                let Some(&lens_tag) = attr_slice.get(sub_pos) else { break };
                                if lens_tag == 255 {
                                    break;
                                }
                                if sub_pos.saturating_add(3) > attr_slice.len() {
                                    break;
                                }
                                let lens_len = (read_u16(ctx.endian, attr_slice, sub_pos.saturating_add(1)).unwrap_or(3) as usize).max(3);
                                let lens_end = sub_pos.saturating_add(lens_len).min(attr_slice.len());
                                if lens_tag == 10 && sub_pos.saturating_add(3) < lens_end {
                                    let trns_slice = attr_slice.get(sub_pos.saturating_add(3)..lens_end).unwrap_or_default();
                                    decoded_transparency = decode_trns(trns_slice, ctx.endian, 700, &mut Vec::new());
                                } else if lens_tag == 1 && decoded_transparency.is_none() && sub_pos.saturating_add(8) <= lens_end {
                                    let l_type = attr_slice.get(sub_pos.saturating_add(3)).copied().unwrap_or(0);
                                    if l_type == 1 && sub_pos.saturating_add(11) <= lens_end {
                                        let tint_method = attr_slice.get(sub_pos.saturating_add(4)).copied().unwrap_or(0);
                                        let rate_milli = read_u16(ctx.endian, attr_slice, sub_pos.saturating_add(5)).unwrap_or(500);
                                        let c_ref = read_u16(ctx.endian, attr_slice, sub_pos.saturating_add(7)).unwrap_or(1) as usize;
                                        let rate = (f64::from(rate_milli) / 1000.0).clamp(0.0, 1.0);
                                        if tint_method == 0 {
                                            if let Ok(op) = F64::new(rate) {
                                                decoded_transparency = Some(coralcraft_doc::TransparencyStyle::Uniform { opacity: op });
                                            }
                                        } else if let Ok(r_f64) = F64::new(rate) {
                                            let col = cmx_colors.get(c_ref.saturating_sub(1)).cloned().unwrap_or(Color::BLACK);
                                            decoded_transparency = Some(coralcraft_doc::TransparencyStyle::Lens {
                                                kind: coralcraft_doc::LensKind::Tinted { color: col, rate: r_f64 },
                                            });
                                        }
                                    } else if l_type == 2 {
                                        let rate_centi = read_u16(ctx.endian, attr_slice, sub_pos.saturating_add(4)).unwrap_or(200);
                                        if let Ok(fac) = F64::new((f64::from(rate_centi) / 100.0).max(1.0)) {
                                            decoded_transparency = Some(coralcraft_doc::TransparencyStyle::Lens {
                                                kind: coralcraft_doc::LensKind::Magnify { factor: fac },
                                            });
                                        }
                                    }
                                }
                                sub_pos = sub_pos.saturating_add(lens_len);
                            }
                        }
                    }
                } else if tag_id == 2 && tag_pos.saturating_add(tag_len) <= inst_end {
                    let pt_slice = payload.get(tag_pos.saturating_add(3)..tag_pos.saturating_add(tag_len)).unwrap_or_default();
                    if code == 67 && pt_slice.len() >= 2 {
                        let count = read_u16(ctx.endian, pt_slice, 0).unwrap_or(0) as usize;
                        let mut with_pad = Vec::with_capacity(4usize.saturating_add(pt_slice.len()));
                        with_pad.extend_from_slice(&(count as u16).to_le_bytes());
                        with_pad.extend_from_slice(&0u16.to_le_bytes());
                        with_pad.extend_from_slice(pt_slice.get(2..).unwrap_or_default());
                        decoded_path = decode_cdr_point_types_array(&with_pad, ctx.endian, 700, page.size);
                    } else if code == 68 && pt_slice.len() >= 20 {
                        let cx_cdr = CdrUnit(i64::from(read_i32(ctx.endian, pt_slice, 0).unwrap_or(0))).to_pt().get();
                        let cy_cdr = CdrUnit(i64::from(read_i32(ctx.endian, pt_slice, 4).unwrap_or(0))).to_pt().get();
                        let w_pt = CdrUnit(i64::from(read_i32(ctx.endian, pt_slice, 8).unwrap_or(0))).to_pt().get().abs().max(1.0);
                        let h_pt = CdrUnit(i64::from(read_i32(ctx.endian, pt_slice, 12).unwrap_or(0))).to_pt().get().abs().max(1.0);
                        let r_pt = CdrUnit(i64::from(read_i32(ctx.endian, pt_slice, 16).unwrap_or(0))).to_pt().get().abs().max(0.0);
                        let cx = page.size.width.get() * 0.5 + cx_cdr;
                        let cy = page.size.height.get() * 0.5 - cy_cdr;
                        if let Ok(rect) = Rect::new(cx - 0.5 * w_pt, cy - 0.5 * h_pt, cx + 0.5 * w_pt, cy + 0.5 * h_pt) {
                            decoded_bbox = Some(rect);
                            let r_f64 = F64::new(r_pt).unwrap_or(F64::ZERO);
                            decoded_kind =
                                Some(ObjectKind::Rect(RectSpec { rect, radii: [r_f64; 4], corner_kinds: [coralcraft_geom::CornerKind::Round; 4] }));
                        }
                    } else if code == 66 && pt_slice.len() >= 29 {
                        let cx_cdr = CdrUnit(i64::from(read_i32(ctx.endian, pt_slice, 0).unwrap_or(0))).to_pt().get();
                        let cy_cdr = CdrUnit(i64::from(read_i32(ctx.endian, pt_slice, 4).unwrap_or(0))).to_pt().get();
                        let w_pt = CdrUnit(i64::from(read_i32(ctx.endian, pt_slice, 8).unwrap_or(0))).to_pt().get().abs().max(1.0);
                        let h_pt = CdrUnit(i64::from(read_i32(ctx.endian, pt_slice, 12).unwrap_or(0))).to_pt().get().abs().max(1.0);
                        let a1_deg = f64::from(read_i32(ctx.endian, pt_slice, 16).unwrap_or(0)) / 1_000_000.0;
                        let a2_deg = f64::from(read_i32(ctx.endian, pt_slice, 20).unwrap_or(0)) / 1_000_000.0;
                        let pie = pt_slice.get(28).copied().unwrap_or(0) != 0;
                        let cx = page.size.width.get() * 0.5 + cx_cdr;
                        let cy = page.size.height.get() * 0.5 - cy_cdr;
                        if let Ok(rect) = Rect::new(cx - 0.5 * w_pt, cy - 0.5 * h_pt, cx + 0.5 * w_pt, cy + 0.5 * h_pt) {
                            decoded_bbox = Some(rect);
                            let mode = if (a1_deg - a2_deg).abs() < 1e-4 {
                                coralcraft_geom::EllipseMode::Full
                            } else if pie {
                                coralcraft_geom::EllipseMode::Pie
                            } else {
                                coralcraft_geom::EllipseMode::Arc
                            };
                            decoded_kind = Some(ObjectKind::Ellipse(EllipseSpec {
                                rect,
                                start_deg: F64::new(a1_deg.rem_euclid(360.0)).unwrap_or(F64::ZERO),
                                end_deg: F64::new(a2_deg.rem_euclid(360.0)).unwrap_or(F64::ZERO),
                                mode,
                            }));
                        }
                    }
                } else if tag_id == 3 && code == 67 && tag_pos.saturating_add(19) <= inst_end {
                    let bbox_slice = payload.get(tag_pos.saturating_add(3)..tag_pos.saturating_add(19)).unwrap_or_default();
                    decoded_bbox = decode_bbox(bbox_slice, ctx.endian, 700, page.size);
                } else if tag_id == 99 && tag_pos.saturating_add(tag_len) <= inst_end {
                    let meta_slice = payload.get(tag_pos.saturating_add(3)..tag_pos.saturating_add(tag_len)).unwrap_or_default();
                    decoded_meta = decode_cmx_meta_tag(meta_slice, ctx.endian, 700, &mut Vec::new());
                }
                tag_pos = tag_pos.saturating_add(tag_len);
            }
            if let Some((_, _, _, ref shape_meta)) = decoded_meta
                && !shape_meta.is_empty()
            {
                decoded_kind = Some(decode_explicit_shape_payload(shape_meta, *b"loda", ctx.endian, 700, page.size, decoded_bbox, &mut Vec::new()));
            }
            if let Some(kind) = decoded_kind.or_else(|| decoded_path.map(ObjectKind::Path)) {
                if !has_render_attr || (decoded_fill.is_none() && decoded_outline.is_none()) {
                    decoded_outline = Some(OutlineStyle::hairline(Color::BLACK));
                }
                let obj_id = ObjectId(ctx.alloc_id());
                let mut obj_node = ObjectNode::new(obj_id, None, kind, decoded_fill, decoded_outline);
                if let Some((omta, trfd, trns, _)) = decoded_meta {
                    if let Some((name, visible, locked, opacity)) = omta {
                        obj_node.name = name;
                        obj_node.visible = visible;
                        obj_node.locked = locked;
                        obj_node.opacity = opacity;
                    }
                    if let Some(tf) = trfd {
                        obj_node.transform = tf;
                    }
                    if let Some(t) = trns {
                        decoded_transparency = Some(t);
                    }
                }
                if let Some(t) = decoded_transparency {
                    obj_node.transparency = Some(t);
                }
                if let Some((parent, _)) = group_stack.last_mut() {
                    parent.push(obj_node);
                } else if let Some(layer) = page.layers.last_mut() {
                    layer.objects.push(obj_node);
                }
            }
        }
        pos = pos.saturating_add(inst_size);
    }
    while let Some((children, grp_meta)) = group_stack.pop() {
        if !children.is_empty() {
            let grp_id = ObjectId(ctx.alloc_id());
            let mut grp_node = ObjectNode::new(grp_id, Some("Group".to_owned()), ObjectKind::Group { children }, None, None);
            if let Some((omta, trfd, trns, _)) = grp_meta {
                if let Some((name, visible, locked, opacity)) = omta {
                    if name.is_some() {
                        grp_node.name = name;
                    }
                    grp_node.visible = visible;
                    grp_node.locked = locked;
                    grp_node.opacity = opacity;
                }
                if let Some(tf) = trfd {
                    grp_node.transform = tf;
                }
                if let Some(t) = trns {
                    grp_node.transparency = Some(t);
                }
            }
            if let Some((parent, _)) = group_stack.last_mut() {
                parent.push(grp_node);
            } else if let Some(layer) = page.layers.last_mut() {
                layer.objects.push(grp_node);
            }
        }
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
        } else if &ch.fourcc == b"LIST" && ch.list_type == Some(*b"grp ") {
            let mut group_children = Vec::new();
            collect_loose_objects_recursive(&ch.children, ctx, page_size, &mut group_children, warnings);
            if !group_children.is_empty() {
                let grp_id = ObjectId(ctx.alloc_id());
                let mut grp_node = ObjectNode::new(grp_id, Some("Group".to_owned()), ObjectKind::Group { children: group_children }, None, None);
                for (slot_idx, sub) in ch.children.iter().enumerate() {
                    if &sub.fourcc == b"omta"
                        && let Some((name, visible, locked, opacity)) = decode_omta(&sub.payload, ctx.endian)
                    {
                        grp_node.name = name;
                        grp_node.visible = visible;
                        grp_node.locked = locked;
                        grp_node.opacity = opacity;
                    } else if &sub.fourcc == b"trfd"
                        && let Some(tf) = decode_trfd(&sub.payload, ctx.endian)
                    {
                        grp_node.transform = tf;
                    } else if &sub.fourcc == b"trns"
                        && let Some(tr) = decode_trns(&sub.payload, ctx.endian, ctx.version, warnings)
                    {
                        grp_node.transparency = Some(tr);
                    } else if &sub.fourcc == b"enve"
                        && let Some(corners) = decode_enve(&sub.payload, ctx.endian, ctx.version, page_size)
                    {
                        grp_node.effects.push(coralcraft_doc::VectorEffect::Envelope { corners });
                    } else if &sub.fourcc == b"vefx"
                        && let Some(fx) = decode_vefx(&sub.payload, ctx.endian, ctx.version, page_size, warnings)
                    {
                        grp_node.effects.push(fx);
                    } else if &sub.fourcc == b"LIST" && sub.list_type == Some(*b"pclp") {
                        collect_loose_objects_recursive(&sub.children, ctx, page_size, &mut grp_node.powerclip_children, warnings);
                    } else if sub.list_type.is_none()
                        && !matches!(&sub.fourcc, b"omta" | b"trfd" | b"trns" | b"enve" | b"vefx")
                        && !sub.payload.is_empty()
                        && sub.payload.len() <= 512
                    {
                        grp_node.residual.chunks.push(RawChunk {
                            tag: ChunkTag(sub.fourcc),
                            flags: 0,
                            slot: slot_idx as u32,
                            payload: RawPayload(sub.payload.clone()),
                            src_offset: sub.src_offset,
                        });
                    }
                }
                out.push(grp_node);
            }
        } else if &ch.fourcc == b"LIST" && matches!(ch.list_type.as_ref(), Some(b"mstr" | b"pclp" | b"uchl" | b"ures")) {
            // Specialized container lists are parsed separately by their owners
        } else if !ch.children.is_empty() {
            collect_loose_objects_recursive(&ch.children, ctx, page_size, out, warnings);
        }
    }
}

fn parse_page_chunk(page_chunk: &ParsedChunk, ctx: &mut ParserCtx, page: &mut Page, warnings: &mut Vec<String>) {
    for (slot_idx, ch) in page_chunk.children.iter().enumerate() {
        if &ch.fourcc == b"mcfg"
            && let Some(sz) = decode_mcfg(&ch.payload, ctx.endian, ctx.version)
        {
            page.size = sz;
        } else if &ch.fourcc == b"pmta" {
            decode_pmta(&ch.payload, ctx.endian, ctx.version, page, warnings);
        } else if ch.list_type.is_none() && !matches!(&ch.fourcc, b"mcfg" | b"pmta") && !ch.payload.is_empty() && ch.payload.len() <= 512 {
            page.residual.chunks.push(RawChunk {
                tag: ChunkTag(ch.fourcc),
                flags: 0,
                slot: slot_idx as u32,
                payload: RawPayload(ch.payload.clone()),
                src_offset: ch.src_offset,
            });
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
            let layer_name = format!("Layer {}", l_idx.saturating_add(1));
            let mut layer = Layer::new_normal(LayerId(ctx.alloc_id()), layer_name);
            for (slot_idx, ch) in l_chunk.children.iter().enumerate() {
                if &ch.fourcc == b"lmta" {
                    decode_lmta(&ch.payload, ctx.endian, &mut layer);
                } else if ch.list_type.is_none() && !ch.payload.is_empty() && ch.payload.len() <= 512 {
                    layer.residual.chunks.push(RawChunk {
                        tag: ChunkTag(ch.fourcc),
                        flags: 0,
                        slot: slot_idx as u32,
                        payload: RawPayload(ch.payload.clone()),
                        src_offset: ch.src_offset,
                    });
                }
            }
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
    let mut transparency: Option<coralcraft_doc::TransparencyStyle> = None;
    let mut obj_meta: Option<(Option<String>, bool, bool, F64)> = None;
    let mut powerclip_children: Vec<ObjectNode> = Vec::new();
    let mut unsup_children: Vec<ObjectNode> = Vec::new();
    let mut unsup_residual = Residual::default();
    let mut loda_payload: Option<(&[u8], [u8; 4])> = None;
    let mut bmp_payload: Option<&[u8]> = None;
    let mut txtd_payload: Option<&[u8]> = None;
    let mut effects = Vec::new();
    let mut residual = Residual::default();

    for ch in &obj_chunk.children {
        if &ch.fourcc == b"LIST" && ch.list_type == Some(*b"pclp") {
            collect_loose_objects_recursive(&ch.children, ctx, page_size, &mut powerclip_children, warnings);
        } else if &ch.fourcc == b"LIST" && ch.list_type == Some(*b"uchl") {
            collect_loose_objects_recursive(&ch.children, ctx, page_size, &mut unsup_children, warnings);
        } else if &ch.fourcc == b"LIST" && ch.list_type == Some(*b"ures") {
            for (slot_idx, sub) in ch.children.iter().enumerate() {
                if sub.list_type.is_none() && !sub.payload.is_empty() && sub.payload.len() <= 512 {
                    unsup_residual.chunks.push(RawChunk {
                        tag: ChunkTag(sub.fourcc),
                        flags: 0,
                        slot: slot_idx as u32,
                        payload: RawPayload(sub.payload.clone()),
                        src_offset: sub.src_offset,
                    });
                }
            }
        }
    }

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
            b"trns" => {
                if let Some(tr) = decode_trns(&ch.payload, ctx.endian, ctx.version, warnings) {
                    transparency = Some(tr);
                }
            }
            b"omta" => {
                obj_meta = decode_omta(&ch.payload, ctx.endian);
            }
            b"vefx" => {
                if let Some(fx) = decode_vefx(&ch.payload, ctx.endian, ctx.version, page_size, warnings) {
                    effects.push(fx);
                }
            }
            b"bmp " | b"CDIB" => {
                if bmp_payload.is_none() && !ch.payload.is_empty() {
                    bmp_payload = Some(&ch.payload);
                }
            }
            b"txtd" | b"txts" => {
                if txtd_payload.is_none() && !ch.payload.is_empty() {
                    txtd_payload = Some(&ch.payload);
                }
            }
            b"enve" => {
                if let Some(corners) = decode_enve(&ch.payload, ctx.endian, ctx.version, page_size) {
                    effects.push(coralcraft_doc::VectorEffect::Envelope { corners });
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

    // Check if `loda` references `fild` (`0x14`) or `outl` (`0x0a`) IDs in the document style tables
    if let Some((payload, _)) = loda_payload {
        let (loda_fill_id, loda_outl_id) = extract_loda_style_ids(payload, ctx.endian);
        if fill.is_none() {
            if let Some(fid) = loda_fill_id
                && let Some(f) = ctx.fills.get(&fid)
            {
                fill = Some(f.clone());
            } else if let Some((&first_id, first_fill)) = ctx.fills.iter().next()
                && contains_u32(payload, first_id, ctx.endian)
            {
                fill = Some(first_fill.clone());
            }
        }
        if outline.is_none() {
            if let Some(oid) = loda_outl_id
                && let Some(o) = ctx.outlines.get(&oid)
            {
                outline = Some(o.clone());
            } else if let Some((&first_id, first_outl)) = ctx.outlines.iter().next()
                && contains_u32(payload, first_id, ctx.endian)
            {
                outline = Some(first_outl.clone());
            }
        }
    }

    let mut kind = if let Some(bmp_data) = bmp_payload {
        if let Some(bmp) = decode_bmp_or_cdib_chunk(bmp_data, ctx.endian) {
            bmp
        } else if let Some((payload, tag)) = loda_payload {
            decode_loda_geometry(payload, tag, ctx.endian, ctx.version, page_size, bbox_rect, warnings)
        } else {
            let fallback = bbox_rect.or_else(|| Rect::new(36.0, 36.0, 136.0, 136.0).ok());
            ObjectKind::Unsupported(UnsupportedObject {
                tag: ChunkTag(*b"CDIB"),
                class_hint: Some("CorruptBitmap".to_owned()),
                payload: RawPayload(bmp_data.iter().copied().take(512).collect()),
                fallback_bounds: fallback,
                absolute_geometry: false,
                children: Vec::new(),
                residual: Residual::default(),
            })
        }
    } else if let Some((payload, tag)) = loda_payload {
        decode_loda_geometry(payload, tag, ctx.endian, ctx.version, page_size, bbox_rect, warnings)
    } else if let Some(txtd) = txtd_payload {
        decode_txtd_chunk(txtd, bbox_rect)
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

    if let ObjectKind::Unsupported(u) = &mut kind {
        if !unsup_children.is_empty() {
            u.children = unsup_children;
        }
        if !unsup_residual.chunks.is_empty() {
            u.residual = unsup_residual;
        }
    }

    if fill.is_none() && outline.is_none() && !matches!(kind, ObjectKind::Bitmap { .. } | ObjectKind::Unsupported(_)) {
        outline = Some(OutlineStyle::hairline(Color::BLACK));
    }

    let mut node = ObjectNode::new(obj_id, None, kind, fill, outline);
    if let Some((name, visible, locked, opacity)) = obj_meta {
        node.name = name;
        node.visible = visible;
        node.locked = locked;
        node.opacity = opacity;
    }
    node.transform = transform;
    node.transparency = transparency;
    node.powerclip_children = powerclip_children;
    node.effects = effects;
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
            if matches!(ch.list_type.as_ref(), Some(b"pclp" | b"uchl" | b"ures")) {
                continue;
            }
            flatten_obj_subchunks(&ch.children, out);
        } else {
            out.push(ch);
        }
    }
}

/// Decodes `mcfg` (Document/Page configuration) per Photopea CDR spec §mcfg (`https://github.com/photopea/cdr-spec`):
/// Offset: `V >= 1300 -> 12`, `900 <= V < 1300 -> 4`, `700 <= V < 900 -> 0`, `600 <= V < 700 -> 28`, `V < 600 -> 2`.
fn decode_mcfg(payload: &[u8], endian: Endian, version: u16) -> Option<Size> {
    let offset = if version >= 1300 {
        12usize
    } else if version >= 900 {
        4usize
    } else if (700..900).contains(&version) {
        if payload.len() >= 12 && read_u32(endian, payload, 0) == Some(0) { 4usize } else { 0usize }
    } else if (600..700).contains(&version) {
        28usize
    } else {
        2usize
    };
    let (w_pt, step) = cdr_coord_to_pt(endian, payload, offset, version)?;
    let (h_pt, _) = cdr_coord_to_pt(endian, payload, offset.saturating_add(step), version)?;
    let w = w_pt.abs().clamp(18.0, 14_400.0);
    let h = h_pt.abs().clamp(18.0, 14_400.0);
    if w > 18.0 && h > 18.0 { Size::new(w, h).ok() } else { None }
}

/// Decodes `bbox` (2× `(x, y)` coordinates in CDR page-centered space) into a top-left page-space [`Rect`].
fn decode_bbox(payload: &[u8], endian: Endian, version: u16, page_size: Size) -> Option<Rect> {
    let (x0_cdr, step) = cdr_coord_to_pt(endian, payload, 0, version)?;
    let (y0_cdr, _) = cdr_coord_to_pt(endian, payload, step, version)?;
    let (x1_cdr, _) = cdr_coord_to_pt(endian, payload, step.saturating_mul(2), version)?;
    let (y1_cdr, _) = cdr_coord_to_pt(endian, payload, step.saturating_mul(3), version)?;

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

/// Decodes a 32-byte `enve` 4-corner Envelope cage chunk (`[TL, TR, BR, BL]` in `CdrUnit` ticks).
fn decode_enve(payload: &[u8], endian: Endian, version: u16, page_size: Size) -> Option<[Point; 4]> {
    if payload.len() < 32 {
        return None;
    }
    let cx = page_size.width.get() * 0.5;
    let cy = page_size.height.get() * 0.5;
    let mut pts = [Point::ZERO; 4];
    for (i, slot) in pts.iter_mut().enumerate() {
        let base = i.saturating_mul(8);
        let (x_cdr, step) = cdr_coord_to_pt(endian, payload, base, version)?;
        let (y_cdr, _) = cdr_coord_to_pt(endian, payload, base.saturating_add(step), version)?;
        *slot = Point::new(cx + x_cdr, cy - y_cdr).ok()?;
    }
    Some(pts)
}

fn decode_trns(payload: &[u8], endian: Endian, version: u16, warnings: &mut Vec<String>) -> Option<coralcraft_doc::TransparencyStyle> {
    let mode = *payload.first()?;
    match mode {
        1 if payload.len() >= 12 => {
            let op = read_f64(endian, payload, 4)?.clamp(0.0, 1.0);
            Some(coralcraft_doc::TransparencyStyle::Uniform { opacity: F64::new(op).ok()? })
        }
        2 if payload.len() >= 10 => {
            let (_id, fill) = decode_fild(payload.get(4..)?, endian, version, warnings)?;
            if let FillStyle::Fountain { geometry, blend_mode, stops } = fill {
                Some(coralcraft_doc::TransparencyStyle::Fountain { geometry, blend_mode, stops })
            } else {
                None
            }
        }
        3 if payload.len() >= 4 => {
            let l_code = *payload.get(1)?;
            let kind = match l_code {
                1 => coralcraft_doc::LensKind::Invert,
                2 if payload.len() >= 12 => {
                    let rate = F64::new(read_f64(endian, payload, 4)?.clamp(-1.0, 1.0)).ok()?;
                    coralcraft_doc::LensKind::Brighten { rate }
                }
                3 if payload.len() >= 24 => {
                    let rate = F64::new(read_f64(endian, payload, 4)?.clamp(0.0, 1.0)).ok()?;
                    let color = decode_cdr_color_v500(payload.get(12..24)?, endian, warnings).unwrap_or(Color::BLACK);
                    coralcraft_doc::LensKind::Tinted { color, rate }
                }
                4 if payload.len() >= 12 => {
                    let factor = F64::new(read_f64(endian, payload, 4)?.max(1.0)).ok()?;
                    coralcraft_doc::LensKind::Magnify { factor }
                }
                _ => coralcraft_doc::LensKind::Unknown,
            };
            Some(coralcraft_doc::TransparencyStyle::Lens { kind })
        }
        _ => None,
    }
}

fn encode_trns_payload(trns: &coralcraft_doc::TransparencyStyle) -> Vec<u8> {
    let mut p = Vec::new();
    match trns {
        coralcraft_doc::TransparencyStyle::Uniform { opacity } => {
            p.extend_from_slice(&[1u8, 0, 0, 0]);
            p.extend_from_slice(&opacity.get().to_le_bytes());
        }
        coralcraft_doc::TransparencyStyle::Fountain { geometry, blend_mode, stops } => {
            p.extend_from_slice(&[2u8, 0, 0, 0]);
            let fild = encode_fild_payload(0, &FillStyle::Fountain { geometry: *geometry, blend_mode: *blend_mode, stops: stops.clone() });
            p.extend_from_slice(&fild);
        }
        coralcraft_doc::TransparencyStyle::Lens { kind } => match kind {
            coralcraft_doc::LensKind::Invert => {
                p.extend_from_slice(&[3u8, 1u8, 0, 0]);
            }
            coralcraft_doc::LensKind::Brighten { rate } => {
                p.extend_from_slice(&[3u8, 2u8, 0, 0]);
                p.extend_from_slice(&rate.get().to_le_bytes());
            }
            coralcraft_doc::LensKind::Tinted { color, rate } => {
                p.extend_from_slice(&[3u8, 3u8, 0, 0]);
                p.extend_from_slice(&rate.get().to_le_bytes());
                p.extend_from_slice(&encode_cdr_color_v500(color));
            }
            coralcraft_doc::LensKind::Magnify { factor } => {
                p.extend_from_slice(&[3u8, 4u8, 0, 0]);
                p.extend_from_slice(&factor.get().to_le_bytes());
            }
            coralcraft_doc::LensKind::Unknown => {
                p.extend_from_slice(&[3u8, 0u8, 0, 0]);
            }
        },
        coralcraft_doc::TransparencyStyle::Unknown => {
            p.extend_from_slice(&[0u8, 0, 0, 0]);
        }
    }
    p
}

fn decode_dmta(payload: &[u8], endian: Endian, doc: &mut Document) {
    if payload.len() < 6 {
        return;
    }
    doc.units = match payload.first().copied().unwrap_or(0) {
        1 => Unit::Pt,
        2 => Unit::Inch,
        3 => Unit::Px,
        4 => Unit::CdrTick,
        _ => Unit::Mm,
    };
    if let Some(title_len_u16) = read_u16(endian, payload, 4) {
        let title_len = usize::from(title_len_u16);
        if title_len > 0
            && let Some(title_end) = 6usize.checked_add(title_len)
            && let Some(title_slice) = payload.get(6..title_end)
            && let Ok(title_str) = std::str::from_utf8(title_slice)
            && !title_str.is_empty()
        {
            doc.title = title_str.to_owned();
        }
    }
}

fn encode_dmta_payload(doc: &Document) -> Vec<u8> {
    let unit_code: u8 = match doc.units {
        Unit::Mm => 0,
        Unit::Pt => 1,
        Unit::Inch => 2,
        Unit::Px => 3,
        Unit::CdrTick => 4,
    };
    let mut p = Vec::with_capacity(32);
    p.extend_from_slice(&[unit_code, 0, 0, 0]);
    let is_default_title = doc.title == "Untitled-1" || doc.title.starts_with("CDR (") || doc.title.starts_with("CDR ZIP (");
    let title_bytes = if is_default_title { b"".as_slice() } else { doc.title.as_bytes() };
    let title_len = title_bytes.len().min(4096);
    p.extend_from_slice(&(title_len as u16).to_le_bytes());
    p.extend_from_slice(title_bytes.get(..title_len).unwrap_or_default());
    p
}

fn decode_pmta(payload: &[u8], endian: Endian, version: u16, page: &mut Page, warnings: &mut Vec<String>) {
    if payload.len() < 11 {
        return;
    }
    if let Some(bleed_raw) = read_f64(endian, payload, 0)
        && let Ok(bleed) = F64::new(bleed_raw.max(0.0))
    {
        page.bleed = bleed;
    }
    let Some(name_len_u16) = read_u16(endian, payload, 8) else {
        return;
    };
    let name_len = usize::from(name_len_u16);
    let Some(name_end) = 10usize.checked_add(name_len) else {
        return;
    };
    if let Some(name_slice) = payload.get(10..name_end)
        && let Ok(name_str) = std::str::from_utf8(name_slice)
        && !name_str.is_empty()
    {
        page.name = name_str.to_owned();
    }
    if let Some(&has_bg) = payload.get(name_end) {
        if has_bg == 1 {
            if let Some(fild_slice) = payload.get(name_end.saturating_add(1)..)
                && let Some((_id, bg_fill)) = decode_fild(fild_slice, endian, version, warnings)
            {
                page.background = Some(bg_fill);
            }
        } else {
            page.background = None;
        }
    }
}

fn encode_pmta_payload(page: &Page) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&page.bleed.get().to_le_bytes());
    let name_bytes = page.name.as_bytes();
    let name_len = name_bytes.len().min(4096);
    p.extend_from_slice(&(name_len as u16).to_le_bytes());
    p.extend_from_slice(name_bytes.get(..name_len).unwrap_or_default());
    if let Some(bg) = &page.background {
        p.push(1u8);
        p.extend_from_slice(&encode_fild_payload(0, bg));
    } else {
        p.push(0u8);
    }
    p
}

fn decode_lmta(payload: &[u8], endian: Endian, layer: &mut Layer) {
    if payload.len() < 14 {
        return;
    }
    let flags = payload.first().copied().unwrap_or(0x03);
    layer.visible = (flags & 0x01) != 0;
    layer.printable = (flags & 0x02) != 0;
    layer.locked = (flags & 0x04) != 0;
    layer.master_in_front = (flags & 0x08) != 0;
    layer.role = match payload.get(1).copied().unwrap_or(0) {
        1 => coralcraft_doc::LayerRole::MasterAllPages,
        2 => coralcraft_doc::LayerRole::MasterEvenPages,
        3 => coralcraft_doc::LayerRole::MasterOddPages,
        4 => coralcraft_doc::LayerRole::Desktop,
        5 => coralcraft_doc::LayerRole::Guides,
        6 => coralcraft_doc::LayerRole::Grid,
        _ => coralcraft_doc::LayerRole::Normal,
    };
    if let Some(op_raw) = read_f64(endian, payload, 4)
        && let Ok(op) = F64::new(op_raw.clamp(0.0, 1.0))
    {
        layer.opacity = op;
    }
    if let Some(name_len_u16) = read_u16(endian, payload, 12) {
        let name_len = usize::from(name_len_u16);
        if let Some(name_end) = 14usize.checked_add(name_len)
            && let Some(name_slice) = payload.get(14..name_end)
            && let Ok(name_str) = std::str::from_utf8(name_slice)
            && !name_str.is_empty()
        {
            layer.name = name_str.to_owned();
        }
    }
}

fn encode_lmta_payload(layer: &Layer) -> Vec<u8> {
    let mut flags = 0u8;
    if layer.visible {
        flags |= 0x01;
    }
    if layer.printable {
        flags |= 0x02;
    }
    if layer.locked {
        flags |= 0x04;
    }
    if layer.master_in_front {
        flags |= 0x08;
    }
    let role_code: u8 = match layer.role {
        coralcraft_doc::LayerRole::Normal | coralcraft_doc::LayerRole::Unknown => 0,
        coralcraft_doc::LayerRole::MasterAllPages => 1,
        coralcraft_doc::LayerRole::MasterEvenPages => 2,
        coralcraft_doc::LayerRole::MasterOddPages => 3,
        coralcraft_doc::LayerRole::Desktop => 4,
        coralcraft_doc::LayerRole::Guides => 5,
        coralcraft_doc::LayerRole::Grid => 6,
    };
    let mut p = Vec::with_capacity(32);
    p.push(flags);
    p.push(role_code);
    p.extend_from_slice(&0u16.to_le_bytes());
    p.extend_from_slice(&layer.opacity.get().to_le_bytes());
    let name_bytes = layer.name.as_bytes();
    let name_len = name_bytes.len().min(4096);
    p.extend_from_slice(&(name_len as u16).to_le_bytes());
    p.extend_from_slice(name_bytes.get(..name_len).unwrap_or_default());
    p
}

fn decode_omta(payload: &[u8], endian: Endian) -> Option<(Option<String>, bool, bool, F64)> {
    if payload.len() < 14 {
        return None;
    }
    let flags = *payload.first()?;
    let visible = (flags & 0x01) != 0;
    let locked = (flags & 0x02) != 0;
    let opacity = F64::new(read_f64(endian, payload, 4)?.clamp(0.0, 1.0)).ok()?;
    let name_len = usize::from(read_u16(endian, payload, 12)?);
    let name_end = 14usize.checked_add(name_len)?;
    let name = if name_len > 0 {
        let s = std::str::from_utf8(payload.get(14..name_end)?).ok()?;
        Some(s.to_owned())
    } else {
        None
    };
    Some((name, visible, locked, opacity))
}

fn encode_omta_payload(node: &ObjectNode) -> Vec<u8> {
    let mut flags = 0u8;
    if node.visible {
        flags |= 0x01;
    }
    if node.locked {
        flags |= 0x02;
    }
    let mut p = Vec::with_capacity(32);
    p.extend_from_slice(&[flags, 0, 0, 0]);
    p.extend_from_slice(&node.opacity.get().to_le_bytes());
    let name_bytes = node.name.as_deref().unwrap_or_default().as_bytes();
    let name_len = name_bytes.len().min(4096);
    p.extend_from_slice(&(name_len as u16).to_le_bytes());
    p.extend_from_slice(name_bytes.get(..name_len).unwrap_or_default());
    p
}

fn decode_vefx(payload: &[u8], endian: Endian, version: u16, page_size: Size, warnings: &mut Vec<String>) -> Option<coralcraft_doc::VectorEffect> {
    let tag = *payload.first()?;
    match tag {
        1 if payload.len() >= 16 => {
            let inside = payload.get(1).copied().unwrap_or(0) != 0;
            let steps = read_u32(endian, payload, 4)?;
            let offset_pt = F64::new(read_f64(endian, payload, 8)?).ok()?;
            Some(coralcraft_doc::VectorEffect::Contour { steps, offset_pt, inside })
        }
        2 if payload.len() >= 40 => {
            let offset_x = F64::new(read_f64(endian, payload, 4)?).ok()?;
            let offset_y = F64::new(read_f64(endian, payload, 12)?).ok()?;
            let opacity = F64::new(read_f64(endian, payload, 20)?.clamp(0.0, 1.0)).ok()?;
            let color = decode_cdr_color_v500(payload.get(28..40)?, endian, warnings).unwrap_or(Color::BLACK);
            Some(coralcraft_doc::VectorEffect::DropShadow { offset_x, offset_y, opacity, color })
        }
        3 if payload.len() >= 20 => {
            let depth_pt = F64::new(read_f64(endian, payload, 4)?).ok()?;
            let angle_deg = F64::new(read_f64(endian, payload, 12)?).ok()?;
            Some(coralcraft_doc::VectorEffect::Extrude { depth_pt, angle_deg })
        }
        4 if payload.len() >= 16 => {
            let has_spine = payload.get(1).copied().unwrap_or(0) != 0;
            let target_low = u64::from(read_u32(endian, payload, 4)?);
            let target_high = u64::from(read_u32(endian, payload, 8)?);
            let target_id = ObjectId(target_low | (target_high << 32));
            let steps = read_u32(endian, payload, 12)?;
            let spine = if has_spine && payload.len() > 20 { decode_cdr_curve(payload.get(20..)?, endian, version, page_size) } else { None };
            Some(coralcraft_doc::VectorEffect::Blend { target_id, steps, spine })
        }
        5 if payload.len() >= 20 => {
            let start_width_pt = F64::new(read_f64(endian, payload, 4)?).ok()?;
            let end_width_pt = F64::new(read_f64(endian, payload, 12)?).ok()?;
            Some(coralcraft_doc::VectorEffect::ArtisticMediaBrush { start_width_pt, end_width_pt })
        }
        6 if payload.len() > 16 => {
            let offset_pt = F64::new(read_f64(endian, payload, 4)?).ok()?;
            let spine = decode_cdr_curve(payload.get(16..)?, endian, version, page_size)?;
            Some(coralcraft_doc::VectorEffect::TextOnPath { spine, offset_pt })
        }
        7 if payload.len() >= 36 => {
            let corners = decode_enve(payload.get(4..)?, endian, version, page_size)?;
            Some(coralcraft_doc::VectorEffect::Perspective { corners })
        }
        8 if payload.len() >= 20 => {
            let kind = match payload.get(1).copied().unwrap_or(0) {
                1 => coralcraft_geom::DistortionKind::Zipper,
                2 => coralcraft_geom::DistortionKind::Twister,
                _ => coralcraft_geom::DistortionKind::PushPull,
            };
            let amplitude = F64::new(read_f64(endian, payload, 4)?).ok()?;
            let frequency = F64::new(read_f64(endian, payload, 12)?).ok()?;
            Some(coralcraft_doc::VectorEffect::Distortion { kind, amplitude, frequency })
        }
        _ => None,
    }
}

fn encode_vefx_payload(effect: &coralcraft_doc::VectorEffect, cx: f64, cy: f64) -> Option<Vec<u8>> {
    let mut p = Vec::new();
    match effect {
        coralcraft_doc::VectorEffect::Contour { steps, offset_pt, inside } => {
            p.extend_from_slice(&[1u8, u8::from(*inside), 0, 0]);
            p.extend_from_slice(&steps.to_le_bytes());
            p.extend_from_slice(&offset_pt.get().to_le_bytes());
        }
        coralcraft_doc::VectorEffect::DropShadow { offset_x, offset_y, opacity, color } => {
            p.extend_from_slice(&[2u8, 0, 0, 0]);
            p.extend_from_slice(&offset_x.get().to_le_bytes());
            p.extend_from_slice(&offset_y.get().to_le_bytes());
            p.extend_from_slice(&opacity.get().to_le_bytes());
            p.extend_from_slice(&encode_cdr_color_v500(color));
        }
        coralcraft_doc::VectorEffect::Extrude { depth_pt, angle_deg } => {
            p.extend_from_slice(&[3u8, 0, 0, 0]);
            p.extend_from_slice(&depth_pt.get().to_le_bytes());
            p.extend_from_slice(&angle_deg.get().to_le_bytes());
        }
        coralcraft_doc::VectorEffect::Blend { target_id, steps, spine } => {
            p.extend_from_slice(&[4u8, u8::from(spine.is_some()), 0, 0]);
            p.extend_from_slice(&target_id.0.to_le_bytes());
            p.extend_from_slice(&steps.to_le_bytes());
            if let Some(sp) = spine {
                encode_path_as_loda_1e(sp, cx, cy, &mut p);
            }
        }
        coralcraft_doc::VectorEffect::ArtisticMediaBrush { start_width_pt, end_width_pt } => {
            p.extend_from_slice(&[5u8, 0, 0, 0]);
            p.extend_from_slice(&start_width_pt.get().to_le_bytes());
            p.extend_from_slice(&end_width_pt.get().to_le_bytes());
        }
        coralcraft_doc::VectorEffect::TextOnPath { spine, offset_pt } => {
            p.extend_from_slice(&[6u8, 0, 0, 0]);
            p.extend_from_slice(&offset_pt.get().to_le_bytes());
            encode_path_as_loda_1e(spine, cx, cy, &mut p);
        }
        coralcraft_doc::VectorEffect::Perspective { corners } => {
            p.extend_from_slice(&[7u8, 0, 0, 0]);
            for pt in corners {
                p.extend_from_slice(&pt_to_cdr_i32(pt.x.get() - cx).to_le_bytes());
                p.extend_from_slice(&pt_to_cdr_i32(cy - pt.y.get()).to_le_bytes());
            }
        }
        coralcraft_doc::VectorEffect::Distortion { kind, amplitude, frequency } => {
            let k_code = match kind {
                coralcraft_geom::DistortionKind::PushPull => 0u8,
                coralcraft_geom::DistortionKind::Zipper => 1u8,
                coralcraft_geom::DistortionKind::Twister => 2u8,
            };
            p.extend_from_slice(&[8u8, k_code, 0, 0]);
            p.extend_from_slice(&amplitude.get().to_le_bytes());
            p.extend_from_slice(&frequency.get().to_le_bytes());
        }
        coralcraft_doc::VectorEffect::Envelope { .. } | coralcraft_doc::VectorEffect::Unknown => return None,
    }
    Some(p)
}

fn decode_trfd(payload: &[u8], endian: Endian) -> Option<Affine2> {
    if payload.len() >= 48 {
        let off = payload.len().saturating_sub(48);
        let a = read_f64(endian, payload, off)?;
        let c = read_f64(endian, payload, off.saturating_add(8))?;
        let tx_raw = read_f64(endian, payload, off.saturating_add(16))?;
        let b = read_f64(endian, payload, off.saturating_add(24))?;
        let d = read_f64(endian, payload, off.saturating_add(32))?;
        let ty_raw = read_f64(endian, payload, off.saturating_add(40))?;
        if (a.abs() <= 100.0 && d.abs() <= 100.0) && (a * d - b * c).abs() > 1e-6 && tx_raw.abs() < 1e9 && ty_raw.abs() < 1e9 {
            let (tx, ty) = if payload.len() == 72 && payload.get(18..22) == Some(b"CCPT") {
                (tx_raw * 72.0 / 254_000.0, -ty_raw * 72.0 / 254_000.0)
            } else if payload.get(0..4) == Some(b"TRPT") {
                (tx_raw, ty_raw)
            } else {
                (0.0, 0.0)
            };
            return Affine2::new(a, b, c, d, tx, ty).ok();
        }
    }
    None
}

/// Decodes a CDR `fild` chunk into `(fill_id, FillStyle)` per the Photopea CDR format specification
/// (`https://github.com/photopea/cdr-spec`, §`fild`) and Kaitai Struct `coreldraw_cdr.ksy`
/// (`https://formats.kaitai.io/coreldraw_cdr/`).
pub fn decode_fild(payload: &[u8], endian: Endian, _version: u16, warnings: &mut Vec<String>) -> Option<(u32, FillStyle)> {
    if payload.len() < 6 {
        return None;
    }
    let fill_id = read_u32(endian, payload, 0)?;
    let fill_type = read_u16(endian, payload, 4)?;
    match fill_type {
        0 => None,
        1 => {
            // Uniform fill: standard V>=500 has 2B pad + 12B CDRColor (20B total); legacy compact test has 8B at offset 6
            let color = if let Some(s) = payload.get(8..20) {
                decode_cdr_color_v500(s, endian, warnings)
            } else {
                decode_cdr_color(payload.get(6..).unwrap_or_default(), endian, warnings)
            }
            .unwrap_or_else(|| Color::rgb8(128, 128, 128));
            Some((fill_id, FillStyle::uniform(color)))
        }
        2 => {
            if payload.len() >= 56 {
                let f_type = u16::from(*payload.get(8).unwrap_or(&0));
                let edge_off = read_i32(endian, payload, 28).unwrap_or(0);
                let angle_raw = read_i32(endian, payload, 32).unwrap_or(0);
                let cx_off = read_i32(endian, payload, 36).unwrap_or(0);
                let cy_off = read_i32(endian, payload, 40).unwrap_or(0);
                let mode_raw = read_u32(endian, payload, 46).unwrap_or(0);
                let angle_deg = (f64::from(angle_raw) / 1_000_000.0).clamp(-360.0, 360.0);
                let cx_f = F64::new((0.5 + f64::from(cx_off) / 100.0).clamp(0.0, 1.0)).unwrap_or(F64::HALF);
                let cy_f = F64::new((0.5 - f64::from(cy_off) / 100.0).clamp(0.0, 1.0)).unwrap_or(F64::HALF);
                let edge_pad_f = F64::new(f64::from(edge_off.clamp(0, 49))).unwrap_or(F64::ZERO);
                let num_stops = (read_u32(endian, payload, 52).unwrap_or(0) as usize).min(64);
                let mut stops = Vec::with_capacity(num_stops.max(2));
                for i in 0..num_stops {
                    let base = 56usize.saturating_add(i.saturating_mul(16));
                    if base.saturating_add(16) > payload.len() {
                        break;
                    }
                    let c = decode_cdr_color_v500(payload.get(base..base.saturating_add(12)).unwrap_or_default(), endian, warnings)
                        .unwrap_or(Color::BLACK);
                    let off_pct = read_u32(endian, payload, base.saturating_add(12)).unwrap_or(0);
                    let off = (f64::from(off_pct) / 100.0).clamp(0.0, 1.0);
                    if let Ok(st) = FountainStop::new(off, c) {
                        stops.push(st);
                    }
                }
                if stops.len() < 2 {
                    stops = vec![
                        FountainStop::new(0.0, Color::WHITE).unwrap_or(FountainStop {
                            offset: coralcraft_color::CVal::ZERO,
                            midpoint: coralcraft_color::CVal::HALF,
                            color: Color::WHITE,
                        }),
                        FountainStop::new(1.0, Color::BLACK).unwrap_or(FountainStop {
                            offset: coralcraft_color::CVal::ONE,
                            midpoint: coralcraft_color::CVal::HALF,
                            color: Color::BLACK,
                        }),
                    ];
                }
                let geometry = match f_type {
                    2 => FountainGeometry::Radial { center_x: cx_f, center_y: cy_f, radius: F64::HALF, focal_x: cx_f, focal_y: cy_f },
                    3 => FountainGeometry::Conical { center_x: cx_f, center_y: cy_f, angle_deg: F64::new(angle_deg).unwrap_or(F64::ZERO) },
                    4 => FountainGeometry::Square {
                        center_x: cx_f,
                        center_y: cy_f,
                        radius: F64::HALF,
                        angle_deg: F64::new(angle_deg).unwrap_or(F64::ZERO),
                    },
                    _ => FountainGeometry::Linear { angle_deg: F64::new(angle_deg).unwrap_or(F64::ZERO), edge_pad: edge_pad_f },
                };
                let blend_mode = match mode_raw {
                    1 => FountainBlendMode::RainbowCw,
                    2 => FountainBlendMode::RainbowCcw,
                    _ => FountainBlendMode::Direct,
                };
                return Some((fill_id, FillStyle::Fountain { geometry, blend_mode, stops }));
            }
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
            let geometry = match f_type {
                1 => FountainGeometry::Radial { center_x: F64::HALF, center_y: F64::HALF, radius: F64::HALF, focal_x: F64::HALF, focal_y: F64::HALF },
                2 => FountainGeometry::Conical { center_x: F64::HALF, center_y: F64::HALF, angle_deg: F64::new(angle_deg).unwrap_or(F64::ZERO) },
                3 => FountainGeometry::Square {
                    center_x: F64::HALF,
                    center_y: F64::HALF,
                    radius: F64::HALF,
                    angle_deg: F64::new(angle_deg).unwrap_or(F64::ZERO),
                },
                _ => FountainGeometry::Linear { angle_deg: F64::new(angle_deg).unwrap_or(F64::ZERO), edge_pad: F64::ZERO },
            };
            Some((fill_id, FillStyle::Fountain { geometry, blend_mode: FountainBlendMode::Direct, stops }))
        }
        7 => {
            if payload.len() >= 52 {
                let pat_id = read_u32(endian, payload, 8).unwrap_or(1);
                let kind = match pat_id {
                    2 => coralcraft_doc::PatternKind::PolkaDot,
                    3 => coralcraft_doc::PatternKind::HorizontalStripe,
                    4 => coralcraft_doc::PatternKind::VerticalStripe,
                    5 => coralcraft_doc::PatternKind::DiagonalCrosshatch,
                    _ => coralcraft_doc::PatternKind::Checkerboard,
                };
                let w_cdr = read_i32(endian, payload, 12).unwrap_or(254_000);
                let tile_pt = CdrUnit(i64::from(w_cdr)).to_pt().get().abs().clamp(1.0, 1000.0);
                let fg_color = decode_cdr_color_v500(payload.get(28..40).unwrap_or_default(), endian, warnings).unwrap_or(Color::BLACK);
                let bg_color = decode_cdr_color_v500(payload.get(40..52).unwrap_or_default(), endian, warnings).unwrap_or(Color::WHITE);
                let tile_size_pt = F64::new(tile_pt).unwrap_or(F64::ONE);
                return Some((fill_id, FillStyle::TwoColorPattern { kind, fg_color, bg_color, tile_size_pt }));
            }
            warnings.push(format!("W-FILL-UNSUPPORTED: fill_type 7 in fild#{fill_id} preserved as FillStyle::Unknown"));
            Some((fill_id, FillStyle::Unknown { tag: ChunkTag(*b"fild"), payload: RawPayload(payload.iter().copied().take(256).collect()) }))
        }
        99 if payload.len() >= 20 && payload.get(8..12) == Some(b"UKFL") => {
            let mut tag_bytes = [0u8; 4];
            if let Some(t) = payload.get(12..16) {
                tag_bytes.copy_from_slice(t);
            }
            let len = (read_u32(endian, payload, 16).unwrap_or(0) as usize).min(65535);
            let raw = payload.get(20..20usize.saturating_add(len).min(payload.len())).unwrap_or_default().to_vec();
            Some((fill_id, FillStyle::Unknown { tag: ChunkTag(tag_bytes), payload: RawPayload(raw) }))
        }
        other => {
            warnings.push(format!("W-FILL-UNSUPPORTED: fill_type {other} in fild#{fill_id} preserved as FillStyle::Unknown"));
            Some((fill_id, FillStyle::Unknown { tag: ChunkTag(*b"fild"), payload: RawPayload(payload.iter().copied().take(256).collect()) }))
        }
    }
}

fn decode_cdr_color_v500(slice: &[u8], endian: Endian, warnings: &mut Vec<String>) -> Option<Color> {
    if slice.len() < 12 {
        return decode_cdr_color(slice, endian, warnings);
    }
    let model = read_u16(endian, slice, 0).unwrap_or(5);
    let col0 = *slice.get(8).unwrap_or(&0);
    let col1 = *slice.get(9).unwrap_or(&0);
    let col2 = *slice.get(10).unwrap_or(&0);
    let col3 = *slice.get(11).unwrap_or(&0);
    match model {
        // Standard V>=500 RGB (`0x05`: col2 = R, col1 = G, col0 = B)
        5 => Some(Color::rgb8(col2, col1, col0)),
        // CMYK 100 (`0x01`, `0x02`, `0x15`) or CMYK 255 (`0x03`, `0x11`)
        1..=3 | 0x11 | 0x15 => {
            let scale = if matches!(model, 3 | 0x11) { 255.0 } else { 100.0 };
            Color::cmyk(
                (f64::from(col0) / scale).clamp(0.0, 1.0),
                (f64::from(col1) / scale).clamp(0.0, 1.0),
                (f64::from(col2) / scale).clamp(0.0, 1.0),
                (f64::from(col3) / scale).clamp(0.0, 1.0),
            )
            .ok()
        }
        // HSB (`0x06`)
        6 => {
            let h_u16 = read_u16(endian, slice, 8).unwrap_or(0);
            let (s, b) = if slice.get(2..4) == Some(b"CF") {
                (f64::from(read_u16(endian, slice, 4).unwrap_or(0)) / 10000.0, f64::from(read_u16(endian, slice, 6).unwrap_or(0)) / 10000.0)
            } else {
                (f64::from(col2) / 255.0, f64::from(col3) / 255.0)
            };
            Color::hsb(f64::from(h_u16).clamp(0.0, 360.0), s.clamp(0.0, 1.0), b.clamp(0.0, 1.0)).ok()
        }
        // Grayscale (`0x08` or `0x09`)
        8 | 9 => {
            let k =
                if slice.get(2..4) == Some(b"CF") { f64::from(read_u16(endian, slice, 4).unwrap_or(0)) / 10000.0 } else { f64::from(col0) / 255.0 };
            Color::gray(k.clamp(0.0, 1.0)).ok()
        }
        // Registration (`0x14 = 20`)
        20 => Some(Color::Registration),
        other => {
            warnings.push(format!("W-COLOR-MODEL: unknown CDR color model {other:#06x}; using RGB fallback"));
            Some(Color::rgb8(col2, col1, col0))
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
        1 | 4 => Some(Color::rgb8(b0, b1, b2)),
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
        8 | 9 => Color::gray((1.0 - f64::from(b0) / 255.0).clamp(0.0, 1.0)).ok(),
        other => {
            warnings.push(format!("W-COLOR-MODEL: unknown CDR color model {other:#06x}; using RGB fallback"));
            Some(Color::rgb8(b0, b1, b2))
        }
    }
}

/// Decodes a CDR `outl` chunk into `(outl_id, OutlineStyle)` per the Photopea CDR format specification
/// (`https://github.com/photopea/cdr-spec`, §`outl`) and Kaitai Struct `coreldraw_cdr.ksy`.
pub fn decode_outl(payload: &[u8], endian: Endian, version: u16, warnings: &mut Vec<String>) -> Option<(u32, OutlineStyle)> {
    if payload.len() >= 128 {
        let outl_id = read_u32(endian, payload, 0)?;
        let line_type = read_u16(endian, payload, 4)?;
        let caps = read_u16(endian, payload, 6).unwrap_or(0);
        let join = read_u16(endian, payload, 8).unwrap_or(0);
        let (width_pt_raw, _) = cdr_coord_to_pt(endian, payload, 12, version)?;
        let width_rounded = (width_pt_raw.abs().clamp(0.0, 288.0) * 1000.0).round() / 1000.0;
        let width_pt = F64::new(width_rounded).unwrap_or(F64::ZERO);
        let color = decode_cdr_color_v500(payload.get(76..88).unwrap_or_default(), endian, warnings).unwrap_or(Color::BLACK);
        let mut style = OutlineStyle::with_width(color, width_pt);
        style.behind_fill = (line_type & 0x08) != 0;
        style.scale_with_object = (line_type & 0x20) != 0;
        style.cap = match caps {
            1 => LineCap::Round,
            2 => LineCap::Square,
            _ => LineCap::Butt,
        };
        style.join = match join {
            1 => LineJoin::Round,
            2 => LineJoin::Bevel,
            _ => LineJoin::Miter,
        };
        let num_dash = (read_u16(endian, payload, 104).unwrap_or(0) as usize).min(11);
        if (line_type & 0x04) != 0 && num_dash > 0 {
            let base_w = if width_pt.get() > 0.0 { width_pt.get() } else { 1.0 };
            for i in 0..num_dash {
                let off = 106usize.saturating_add(i.saturating_mul(2));
                if let Some(mult) = read_u16(endian, payload, off)
                    && mult > 0
                    && let Ok(val) = F64::new(((f64::from(mult) * base_w) * 1000.0).round() / 1000.0)
                {
                    style.dash_pattern.push(val);
                }
            }
        }
        if payload.len() >= 136 {
            let decode_arrow = |id: u32| match id {
                0 => coralcraft_doc::ArrowheadKind::None,
                1 => coralcraft_doc::ArrowheadKind::Triangle,
                2 => coralcraft_doc::ArrowheadKind::OpenArrow,
                3 => coralcraft_doc::ArrowheadKind::Circle,
                4 => coralcraft_doc::ArrowheadKind::Diamond,
                _ => coralcraft_doc::ArrowheadKind::Unknown,
            };
            style.start_arrow = decode_arrow(read_u32(endian, payload, 128).unwrap_or(0));
            style.end_arrow = decode_arrow(read_u32(endian, payload, 132).unwrap_or(0));
        }
        return Some((outl_id, style));
    }
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

    let color = decode_cdr_color(payload.get(8usize.saturating_add(next_off)..).unwrap_or(&[]), endian, warnings).unwrap_or(Color::BLACK);
    let mut style = OutlineStyle::with_width(color, width_pt);
    style.behind_fill = behind_fill;
    style.scale_with_object = scale_with_object;
    style.cap = cap;
    style.join = join;
    Some((outl_id, style))
}

fn extract_loda_style_ids(payload: &[u8], endian: Endian) -> (Option<u32>, Option<u32>) {
    if payload.len() < 20 {
        return (None, None);
    }
    let num_args = (read_u32(endian, payload, 4).unwrap_or(0) as usize).min(64);
    let start_of_args = read_u32(endian, payload, 8).unwrap_or(0) as usize;
    let start_of_arg_types = read_u32(endian, payload, 12).unwrap_or(0) as usize;
    let Some(args_end) = start_of_args.checked_add(num_args.saturating_mul(4)) else {
        return (None, None);
    };
    let Some(types_end) = start_of_arg_types.checked_add(num_args.saturating_mul(4)) else {
        return (None, None);
    };
    if start_of_args < 16 || args_end > payload.len() || types_end > payload.len() {
        return (None, None);
    }
    let mut fill_id = None;
    let mut outl_id = None;
    for i in 0..num_args {
        let arg_off = read_u32(endian, payload, start_of_args.saturating_add(i.saturating_mul(4))).unwrap_or(0) as usize;
        let rev_idx = num_args.saturating_sub(1).saturating_sub(i);
        let arg_type = read_u32(endian, payload, start_of_arg_types.saturating_add(rev_idx.saturating_mul(4))).unwrap_or(0);
        if arg_type == 0x14 {
            fill_id = read_u32(endian, payload, arg_off).filter(|&v| v != 0);
        } else if arg_type == 0x0a {
            outl_id = read_u32(endian, payload, arg_off).filter(|&v| v != 0);
        }
    }
    (fill_id, outl_id)
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
    // Check standard `loda` header (`chunk_length`, `num_args`, `start_of_args`, `start_of_arg_types`, `chunk_type`)
    // per Photopea CDR spec (`https://github.com/photopea/cdr-spec`, §`loda`) and Kaitai Struct `coreldraw_cdr.ksy`.
    if payload.len() >= 28 {
        let num_args = (read_u32(endian, payload, 4).unwrap_or(0) as usize).min(64);
        let start_of_args = read_u32(endian, payload, 8).unwrap_or(0) as usize;
        let start_of_arg_types = read_u32(endian, payload, 12).unwrap_or(0) as usize;
        let chunk_type = read_u32(endian, payload, 16).unwrap_or(0);
        if (1..=64).contains(&num_args)
            && start_of_args >= 20
            && start_of_args.saturating_add(num_args.saturating_mul(4)) <= payload.len()
            && start_of_arg_types.saturating_add(num_args.saturating_mul(4)) <= payload.len()
        {
            for i in 0..num_args {
                let arg_off = read_u32(endian, payload, start_of_args.saturating_add(i.saturating_mul(4))).unwrap_or(0) as usize;
                let rev_idx = num_args.saturating_sub(1).saturating_sub(i);
                let arg_type = read_u32(endian, payload, start_of_arg_types.saturating_add(rev_idx.saturating_mul(4))).unwrap_or(0);
                if arg_type == 0x1e && arg_off < payload.len() {
                    let next_off = if i.saturating_add(1) < num_args {
                        (read_u32(endian, payload, start_of_args.saturating_add(i.saturating_add(1).saturating_mul(4)))
                            .unwrap_or(payload.len() as u32) as usize)
                            .min(payload.len())
                    } else {
                        payload.len()
                    };
                    let slice = payload.get(arg_off..next_off.max(arg_off)).unwrap_or_default();
                    if let Some(meta_pos) = slice.windows(6).position(|w| w == b"CCMETA") {
                        let meta_slice = slice.get(meta_pos.saturating_add(6)..).unwrap_or_default();
                        return decode_explicit_shape_payload(meta_slice, tag, endian, version, page_size, bbox_rect, warnings);
                    }
                    match chunk_type {
                        0x01 => {
                            let rect = bbox_rect.unwrap_or_else(|| Rect::new(36.0, 36.0, 180.0, 120.0).unwrap_or_default());
                            return ObjectKind::Rect(RectSpec::sharp(rect));
                        }
                        0x02 => {
                            let rect = bbox_rect.unwrap_or_else(|| Rect::new(36.0, 36.0, 160.0, 160.0).unwrap_or_default());
                            return ObjectKind::Ellipse(EllipseSpec::full(rect));
                        }
                        0x03 | 0x14 | 0x25 => {
                            if let Some(path) = decode_cdr_point_types_array(slice, endian, version, page_size) {
                                return ObjectKind::Path(path);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    decode_explicit_shape_payload(payload, tag, endian, version, page_size, bbox_rect, warnings)
}

fn decode_explicit_shape_payload(
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
                if payload.len() >= 60 && payload.get(20..24) == Some(b"RECT") {
                    let mut corner_kinds = [coralcraft_geom::CornerKind::Round; 4];
                    for (i, slot) in corner_kinds.iter_mut().enumerate() {
                        *slot = match *payload.get(24usize.saturating_add(i)).unwrap_or(&0) {
                            1 => coralcraft_geom::CornerKind::Scalloped,
                            2 => coralcraft_geom::CornerKind::Chamfered,
                            _ => coralcraft_geom::CornerKind::Round,
                        };
                    }
                    let mut radii = [F64::ZERO; 4];
                    for (i, slot) in radii.iter_mut().enumerate() {
                        let off = 28usize.saturating_add(i.saturating_mul(8));
                        let val = read_f64(endian, payload, off).unwrap_or(0.0).max(0.0);
                        *slot = F64::new(val).unwrap_or(F64::ZERO);
                    }
                    return ObjectKind::Rect(RectSpec { rect, radii, corner_kinds });
                }
                return ObjectKind::Rect(RectSpec::sharp(rect));
            }
            2 => {
                // Ellipse
                let rect = bbox_rect
                    .or_else(|| decode_bbox(payload.get(4..).unwrap_or(&[]), endian, version, page_size))
                    .unwrap_or_else(|| Rect::new(36.0, 36.0, 160.0, 160.0).unwrap_or_default());
                if payload.len() >= 44 && payload.get(20..24) == Some(b"ELLP") {
                    let mode = match *payload.get(24).unwrap_or(&0) {
                        1 => coralcraft_geom::EllipseMode::Pie,
                        2 => coralcraft_geom::EllipseMode::Arc,
                        _ => coralcraft_geom::EllipseMode::Full,
                    };
                    let start_deg = F64::new(read_f64(endian, payload, 28).unwrap_or(0.0)).unwrap_or(F64::ZERO);
                    let end_deg = F64::new(read_f64(endian, payload, 36).unwrap_or(0.0)).unwrap_or(F64::ZERO);
                    return ObjectKind::Ellipse(EllipseSpec { rect, start_deg, end_deg, mode });
                }
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
            6 => {
                // Bitmap
                if let Some(bmp) = decode_loda_bitmap(payload.get(4..).unwrap_or(&[]), endian, version, page_size, bbox_rect) {
                    return bmp;
                }
            }
            7 => {
                // Paragraph Text Frame
                if let Some(para) = decode_loda_paragraph_text(payload.get(4..).unwrap_or(&[]), endian, version, page_size, bbox_rect) {
                    return para;
                }
            }
            0x99 if payload.len() >= 51 && payload.get(4..8) == Some(b"UNSP") => {
                let mut u_tag = [0u8; 4];
                if let Some(tb) = payload.get(8..12) {
                    u_tag.copy_from_slice(tb);
                }
                let flags = payload.get(12).copied().unwrap_or(0);
                let absolute_geometry = (flags & 0x01) != 0;
                let has_fb = (flags & 0x02) != 0;
                let fallback_bounds = if has_fb {
                    let x0 = read_f64(endian, payload, 13).unwrap_or(0.0);
                    let y0 = read_f64(endian, payload, 21).unwrap_or(0.0);
                    let x1 = read_f64(endian, payload, 29).unwrap_or(0.0);
                    let y1 = read_f64(endian, payload, 37).unwrap_or(0.0);
                    Rect::new(x0, y0, x1, y1).ok().or(bbox_rect)
                } else {
                    bbox_rect
                };
                let hint_len = usize::from(read_u16(endian, payload, 45).unwrap_or(0));
                let hint_end = 47usize.saturating_add(hint_len).min(payload.len());
                let class_hint = payload.get(47..hint_end).and_then(|s| std::str::from_utf8(s).ok()).filter(|s| !s.is_empty()).map(str::to_owned);
                let pay_len = (read_u32(endian, payload, hint_end).unwrap_or(0) as usize).min(65535);
                let pay_start = hint_end.saturating_add(4);
                let pay_end = pay_start.saturating_add(pay_len).min(payload.len());
                let raw_pay = payload.get(pay_start..pay_end).unwrap_or_default().to_vec();
                return ObjectKind::Unsupported(UnsupportedObject {
                    tag: ChunkTag(u_tag),
                    class_hint,
                    payload: RawPayload(raw_pay),
                    fallback_bounds,
                    absolute_geometry,
                    children: Vec::new(),
                    residual: Residual::default(),
                });
            }
            _ => {}
        }
    }

    // Fallback ladder per Photopea CDR spec (`https://github.com/photopea/cdr-spec`, §`loda`): preserve as `ObjectKind::Unsupported` with `fallback_bounds`
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
        let (cy_cdr, _) = cdr_coord_to_pt(endian, slice, 8usize.saturating_add(s), version)?;
        let (r_pt, _) = cdr_coord_to_pt(endian, slice, 8usize.saturating_add(s.saturating_mul(2)), version)?;
        let (inner_pt, _) = cdr_coord_to_pt(endian, slice, 8usize.saturating_add(s.saturating_mul(3)), version).unwrap_or((0.0, 4));
        let rot_deg = read_f64(endian, slice, 8usize.saturating_add(s.saturating_mul(4))).unwrap_or(0.0);
        let cx = page_size.width.get() * 0.5 + cx_cdr;
        let cy = page_size.height.get() * 0.5 - cy_cdr;
        let center = Point::new(cx, cy).ok()?;
        if inner_pt > 0.0 && (inner_pt - r_pt).abs() > 1e-4 {
            return PolygonSpec::star(center, r_pt.abs(), inner_pt.abs(), points, rot_deg).ok();
        }
        return PolygonSpec::regular(center, r_pt.abs(), points, rot_deg).ok();
    }
    let rect = bbox_rect.unwrap_or_else(|| Rect::new(50.0, 50.0, 150.0, 150.0).unwrap_or_default());
    let center = Point::new(0.5 * (rect.x0.get() + rect.x1.get()), 0.5 * (rect.y0.get() + rect.y1.get())).ok()?;
    let r = 0.5 * rect.width().min(rect.height()).max(10.0);
    PolygonSpec::regular(center, r, 5, 0.0).ok()
}

fn decode_loda_artistic_text(slice: &[u8], endian: Endian, version: u16, page_size: Size, bbox_rect: Option<Rect>) -> Option<ObjectKind> {
    if slice.len() >= 20 && slice.get(0..4) == Some(b"ATXT") {
        let (x_cdr, s) = cdr_coord_to_pt(endian, slice, 4, version)?;
        let (y_cdr, _) = cdr_coord_to_pt(endian, slice, 4usize.saturating_add(s), version)?;
        let (size_pt_raw, _) = cdr_coord_to_pt(endian, slice, 4usize.saturating_add(s.saturating_mul(2)), version)?;
        let cursor = 4usize.saturating_add(s.saturating_mul(3));
        let fam_len = read_u16(endian, slice, cursor)? as usize;
        let fam_end = cursor.checked_add(2)?.checked_add(fam_len)?;
        let font_family = std::str::from_utf8(slice.get(cursor.saturating_add(2)..fam_end)?).ok()?.to_owned();
        let txt_len = read_u32(endian, slice, fam_end)? as usize;
        let txt_end = fam_end.checked_add(4)?.checked_add(txt_len)?;
        let text = std::str::from_utf8(slice.get(fam_end.saturating_add(4)..txt_end)?).ok()?.to_owned();
        let position = Point::new(page_size.width.get() * 0.5 + x_cdr, page_size.height.get() * 0.5 - y_cdr).ok()?;
        let size_pt = F64::new(size_pt_raw.abs().clamp(1.0, 1440.0)).ok()?;
        return Some(ObjectKind::ArtisticText { text, font_family, size_pt, position });
    }
    let rect = bbox_rect?;
    let position = Point::new(rect.x0.get(), rect.y0.get()).ok()?;
    let size_pt = F64::new(rect.height().clamp(6.0, 288.0)).ok()?;
    Some(ObjectKind::ArtisticText { text: "Text".to_owned(), font_family: "Sans".to_owned(), size_pt, position })
}

fn decode_loda_paragraph_text(slice: &[u8], endian: Endian, version: u16, page_size: Size, bbox_rect: Option<Rect>) -> Option<ObjectKind> {
    if slice.len() >= 28 && slice.get(0..4) == Some(b"PTXT") {
        let (x_cdr, s) = cdr_coord_to_pt(endian, slice, 4, version)?;
        let (y_cdr, _) = cdr_coord_to_pt(endian, slice, 4usize.saturating_add(s), version)?;
        let (w_pt, _) = cdr_coord_to_pt(endian, slice, 4usize.saturating_add(s.saturating_mul(2)), version)?;
        let (h_pt, _) = cdr_coord_to_pt(endian, slice, 4usize.saturating_add(s.saturating_mul(3)), version)?;
        let (size_pt_raw, _) = cdr_coord_to_pt(endian, slice, 4usize.saturating_add(s.saturating_mul(4)), version)?;
        let cursor = 4usize.saturating_add(s.saturating_mul(5));
        let align_code = *slice.get(cursor)?;
        let columns = u32::from(*slice.get(cursor.saturating_add(1))?).max(1);
        let gutter_tenths = f64::from(read_u16(endian, slice, cursor.saturating_add(2))?) / 10.0;
        let spacing_pct = f64::from(read_u16(endian, slice, cursor.saturating_add(4))?) / 100.0;
        let fam_len = read_u16(endian, slice, cursor.saturating_add(6))? as usize;
        let fam_end = cursor.checked_add(8)?.checked_add(fam_len)?;
        let font_family = std::str::from_utf8(slice.get(cursor.saturating_add(8)..fam_end)?).ok()?.to_owned();
        let txt_len = read_u32(endian, slice, fam_end)? as usize;
        let txt_end = fam_end.checked_add(4)?.checked_add(txt_len)?;
        let text = std::str::from_utf8(slice.get(fam_end.saturating_add(4)..txt_end)?).ok()?.to_owned();
        let position = Point::new(page_size.width.get() * 0.5 + x_cdr, page_size.height.get() * 0.5 - y_cdr).ok()?;
        let alignment = match align_code {
            1 => TextAlignment::Center,
            2 => TextAlignment::Right,
            3 => TextAlignment::Justify,
            _ => TextAlignment::Left,
        };
        let spec = ParagraphTextSpec::new(
            position,
            w_pt.abs().max(1.0),
            h_pt.abs().max(1.0),
            text,
            font_family,
            size_pt_raw.abs().clamp(1.0, 1440.0),
            spacing_pct.clamp(0.5, 5.0),
            alignment,
            columns,
            gutter_tenths,
        )
        .ok()?;
        return Some(ObjectKind::ParagraphText(Box::new(spec)));
    }
    let rect = bbox_rect?;
    let spec = ParagraphTextSpec::new(
        Point::new(rect.x0.get(), rect.y0.get()).ok()?,
        rect.width().max(10.0),
        rect.height().max(10.0),
        "Paragraph Text",
        "Sans",
        12.0,
        1.2,
        TextAlignment::Left,
        1,
        0.0,
    )
    .ok()?;
    Some(ObjectKind::ParagraphText(Box::new(spec)))
}

fn decode_txtd_chunk(payload: &[u8], bbox_rect: Option<Rect>) -> ObjectKind {
    // Byte 0 is text encoding tag (`0x00` = ASCII, `0x01` = UTF-8, `0x02` = UTF-16LE) per Photopea CDR spec (`https://github.com/photopea/cdr-spec`, §Text)
    let text = if payload.first() == Some(&0x02) && payload.len() >= 3 {
        let u16s: Vec<u16> =
            payload.get(1..).unwrap_or_default().as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).take_while(|&ch| ch != 0).collect();
        String::from_utf16_lossy(&u16s)
    } else if payload.first().is_some_and(|&b| b <= 0x01) && payload.len() >= 2 {
        String::from_utf8_lossy(payload.get(1..).unwrap_or_default()).trim_end_matches('\0').to_owned()
    } else {
        String::from_utf8_lossy(payload).trim_end_matches('\0').to_owned()
    };
    let rect = bbox_rect.unwrap_or_else(|| Rect::new(36.0, 36.0, 236.0, 136.0).unwrap_or_default());
    if let Ok(pt) = Point::new(rect.x0.get(), rect.y0.get())
        && let Ok(spec) = ParagraphTextSpec::new(
            pt,
            rect.width().max(20.0),
            rect.height().max(20.0),
            if text.is_empty() { "Text".to_owned() } else { text },
            "Arial",
            12.0,
            1.2,
            TextAlignment::Left,
            1,
            0.0,
        )
    {
        ObjectKind::ParagraphText(Box::new(spec))
    } else {
        ObjectKind::Rect(RectSpec::sharp(rect))
    }
}

fn decode_loda_bitmap(slice: &[u8], endian: Endian, _version: u16, _page_size: Size, bbox_rect: Option<Rect>) -> Option<ObjectKind> {
    let dpi_fallback = F64::new(72.0).unwrap_or(F64::ONE);
    if slice.len() >= 24 && slice.get(0..4) == Some(b"BMP ") {
        let pixel_width = read_u32(endian, slice, 4)?;
        let pixel_height = read_u32(endian, slice, 8)?;
        let dpi_x_raw = read_u32(endian, slice, 12)?;
        let dpi_y_raw = read_u32(endian, slice, 16)?;
        let dpi_x = F64::new(f64::from(f32::from_bits(dpi_x_raw))).unwrap_or(dpi_fallback);
        let dpi_y = F64::new(f64::from(f32::from_bits(dpi_y_raw))).unwrap_or(dpi_fallback);
        let byte_len = usize::try_from(read_u32(endian, slice, 20)?).ok()?;
        let pixels = usize::try_from(pixel_width).ok()?.checked_mul(usize::try_from(pixel_height).ok()?)?;
        if pixels == 0 || pixels > MAX_BITMAP_PIXELS {
            return None;
        }
        let expected = pixels.checked_mul(4)?;
        // The pixel data must be present in full: never pad or invent pixels from header fields.
        if byte_len != expected {
            return None;
        }
        let pixel_bytes = slice.get(24..24usize.checked_add(byte_len)?)?;
        Some(ObjectKind::Bitmap { pixel_width, pixel_height, dpi_x, dpi_y, rgba_pixels: pixel_bytes.to_vec() })
    } else {
        // Without decodable pixel data the caller falls back to an `Unsupported` object that keeps
        // the bounding box; placeholder pixels are never invented.
        let _ = bbox_rect;
        None
    }
}

fn decode_bmp_or_cdib_chunk(payload: &[u8], endian: Endian) -> Option<ObjectKind> {
    if payload.len() < 24 {
        return None;
    }
    if payload.get(0..4) == Some(b"BMP ") {
        return decode_loda_bitmap(payload, endian, 1400, Size::default(), None);
    }
    // Check BITMAPINFOHEADER (biSize = 40)
    let bi_size = read_u32(Endian::Little, payload, 0)?;
    if bi_size == 40 && payload.len() >= 40 {
        let w = read_i32(Endian::Little, payload, 4)?.unsigned_abs();
        let h = read_i32(Endian::Little, payload, 8)?.unsigned_abs();
        let bpp = read_u16(Endian::Little, payload, 14)?;
        let xpels = read_i32(Endian::Little, payload, 24).unwrap_or(0);
        let ypels = read_i32(Endian::Little, payload, 28).unwrap_or(0);
        let dpi_x = if xpels > 0 { (f64::from(xpels) * 0.0254).max(1.0) } else { 72.0 };
        let dpi_y = if ypels > 0 { (f64::from(ypels) * 0.0254).max(1.0) } else { 72.0 };

        if w == 0 || h == 0 || w > 16384 || h > 16384 {
            return None;
        }
        let pixels = (w as usize).checked_mul(h as usize)?;
        if pixels > MAX_BITMAP_PIXELS {
            return None;
        }

        let raw_pixels = payload.get(40..)?;
        // Check that the pixel data is present before allocating the RGBA buffer.
        let needed = match bpp {
            32 => pixels.checked_mul(4)?,
            24 => (w as usize).checked_mul(3)?.div_ceil(4).checked_mul(4)?.checked_mul(h as usize)?,
            _ => return None,
        };
        if raw_pixels.len() < needed {
            return None;
        }
        let expected_rgba_len = pixels.checked_mul(4)?;
        let mut rgba = Vec::with_capacity(expected_rgba_len);
        if bpp == 32 {
            for &[b, g, r, a] in raw_pixels.as_chunks::<4>().0.iter().take(pixels) {
                rgba.push(r);
                rgba.push(g);
                rgba.push(b);
                rgba.push(a);
            }
        } else if bpp == 24 {
            let row_stride = (w as usize).saturating_mul(3).div_ceil(4).saturating_mul(4);
            for row in 0..h as usize {
                let row_start = row.saturating_mul(row_stride);
                if row_start.saturating_add((w as usize).saturating_mul(3)) <= raw_pixels.len() {
                    for x in 0..w as usize {
                        let px = row_start.saturating_add(x.saturating_mul(3));
                        rgba.push(*raw_pixels.get(px.saturating_add(2)).unwrap_or(&0));
                        rgba.push(*raw_pixels.get(px.saturating_add(1)).unwrap_or(&0));
                        rgba.push(*raw_pixels.get(px).unwrap_or(&0));
                        rgba.push(255);
                    }
                }
            }
        }
        if rgba.len() == expected_rgba_len {
            let dpi_fallback = F64::new(72.0).unwrap_or(F64::ONE);
            return Some(ObjectKind::Bitmap {
                pixel_width: w,
                pixel_height: h,
                dpi_x: F64::new(dpi_x).unwrap_or(dpi_fallback),
                dpi_y: F64::new(dpi_y).unwrap_or(dpi_fallback),
                rgba_pixels: rgba,
            });
        }
    }
    None
}

/// Decodes a standard CDR `0x1e` `LineAndCurve` / `Polygon` point + point-type array:
/// `point_num: u16`, `pad: u16`, `points: [(coord, coord); point_num]`, `point_types: [u8; point_num]`
/// per Photopea CDR spec (`https://github.com/photopea/cdr-spec`, §Curve) and Kaitai Struct `coreldraw_cdr.ksy`.
fn decode_cdr_point_types_array(slice: &[u8], endian: Endian, version: u16, page_size: Size) -> Option<Path> {
    if slice.len() < 4 {
        return None;
    }
    let point_num = read_u16(endian, slice, 0)? as usize;
    if point_num == 0 {
        return None;
    }
    let coord_step = if version < 600 { 4usize } else { 8usize };
    let coords_bytes = point_num.checked_mul(coord_step)?;
    let types_start = 4usize.checked_add(coords_bytes)?;
    let types_end = types_start.checked_add(point_num)?;
    if types_end > slice.len() {
        return None;
    }

    let cx = page_size.width.get() * 0.5;
    let cy = page_size.height.get() * 0.5;
    let mut pts = Vec::with_capacity(point_num);
    for i in 0..point_num {
        let base = 4usize.saturating_add(i.saturating_mul(coord_step));
        let (x_cdr, s) = cdr_coord_to_pt(endian, slice, base, version)?;
        let (y_cdr, _) = cdr_coord_to_pt(endian, slice, base.saturating_add(s), version)?;
        pts.push(Point::new(cx + x_cdr, cy - y_cdr).ok()?);
    }
    let types = slice.get(types_start..types_end)?;
    let mut path = Path::new();
    let mut ctrl_pts: Vec<Point> = Vec::new();
    for (pt, &t) in pts.into_iter().zip(types.iter()) {
        let is_closed = (t & 0x08) != 0;
        match t & 0xC0 {
            0x00 => {
                if is_closed {
                    let _ = path.close();
                }
                ctrl_pts.clear();
                path.move_to(pt).ok()?;
            }
            0x40 => {
                ctrl_pts.clear();
                path.line_to(pt).ok()?;
                if is_closed {
                    let _ = path.close();
                }
            }
            0x80 => {
                if let (Some(&c1), Some(&c2)) = (ctrl_pts.first(), ctrl_pts.get(1)) {
                    path.cubic_to(c1, c2, pt).ok()?;
                } else {
                    path.line_to(pt).ok()?;
                }
                if is_closed {
                    let _ = path.close();
                }
                ctrl_pts.clear();
            }
            0xC0 => {
                ctrl_pts.push(pt);
            }
            _ => {}
        }
    }
    (!path.elements.is_empty()).then_some(path)
}

fn decode_cdr_curve(slice: &[u8], endian: Endian, version: u16, page_size: Size) -> Option<Path> {
    let cx = page_size.width.get() * 0.5;
    let cy = page_size.height.get() * 0.5;

    // Tagged `0x1e` Bezier node stream (`L1E\0` + `u32 node_count` + node records with `0x00`/`0x40`/`0x80`/`0x08` flags per Photopea CDR spec §Curve)
    if slice.len() >= 8 && slice.get(0..4) == Some(b"L1E\0") {
        let count = read_u32(endian, slice, 4)? as usize;
        if !(1..=20_000).contains(&count) {
            return None;
        }
        let mut cursor = 8usize;
        let mut path = Path::new();
        for _ in 0..count {
            let flags = *slice.get(cursor)?;
            cursor = cursor.saturating_add(1);
            let op = flags & 0xC0;
            if flags == 0x08 {
                let _ = path.close();
                continue;
            }
            match op {
                0x00 => {
                    let (x_cdr, s) = cdr_coord_to_pt(endian, slice, cursor, version)?;
                    let (y_cdr, _) = cdr_coord_to_pt(endian, slice, cursor.saturating_add(s), version)?;
                    cursor = cursor.saturating_add(s.saturating_mul(2));
                    let pt = Point::new(cx + x_cdr, cy - y_cdr).ok()?;
                    path.move_to(pt).ok()?;
                }
                0x40 => {
                    let (x_cdr, s) = cdr_coord_to_pt(endian, slice, cursor, version)?;
                    let (y_cdr, _) = cdr_coord_to_pt(endian, slice, cursor.saturating_add(s), version)?;
                    cursor = cursor.saturating_add(s.saturating_mul(2));
                    let pt = Point::new(cx + x_cdr, cy - y_cdr).ok()?;
                    path.line_to(pt).ok()?;
                    if (flags & 0x08) != 0 {
                        let _ = path.close();
                    }
                }
                0x80 => {
                    let (c1x, s) = cdr_coord_to_pt(endian, slice, cursor, version)?;
                    let (c1y, _) = cdr_coord_to_pt(endian, slice, cursor.saturating_add(s), version)?;
                    let (c2x, _) = cdr_coord_to_pt(endian, slice, cursor.saturating_add(s.saturating_mul(2)), version)?;
                    let (c2y, _) = cdr_coord_to_pt(endian, slice, cursor.saturating_add(s.saturating_mul(3)), version)?;
                    let (px, _) = cdr_coord_to_pt(endian, slice, cursor.saturating_add(s.saturating_mul(4)), version)?;
                    let (py, _) = cdr_coord_to_pt(endian, slice, cursor.saturating_add(s.saturating_mul(5)), version)?;
                    cursor = cursor.saturating_add(s.saturating_mul(6));
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

    if let Some(path) = decode_cdr_point_types_array(slice, endian, version, page_size) {
        return Some(path);
    }

    let count = read_u32(endian, slice, 0)? as usize;
    if !(2..=10_000).contains(&count) {
        return None;
    }
    let step = if version < 600 { 4usize } else { 8usize };
    let coords_bytes = count.checked_mul(step)?;
    if slice.len() < 4usize.checked_add(coords_bytes)? {
        return None;
    }

    let mut path = Path::new();
    for i in 0..count {
        let off = 4usize.saturating_add(i.saturating_mul(step));
        let (x_cdr, s) = cdr_coord_to_pt(endian, slice, off, version)?;
        let (y_cdr, _) = cdr_coord_to_pt(endian, slice, off.saturating_add(s), version)?;
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
/// The emitted container satisfies `probe(&bytes)` (`CdrKind::Cdr`, `effective_version = Some(700)`),
/// conforms to the Photopea CDR / Kaitai Struct `coreldraw_cdr.ksy` specification (validated against
/// `libcdr`'s `cdr2raw`), and round-trips through `parse_document(&bytes)`.
pub fn write_cdr7(doc: &Document) -> Result<Vec<u8>, ProbeError> {
    write_riff_document(doc, *b"CDR7", 700)
}

/// Writes a [`Document`] into a clean-room `RIFF` `CMX1` (Corel Presentation Exchange 32-bit) byte buffer.
///
/// The emitted container satisfies `probe(&bytes)` (`CdrKind::Cmx`), emits standard CMX1 `cont`,
/// `page` (`BeginPage`, `PolyCurve`, `EndPage`), `rclr`, `rott`, `rpen`, `rotl`, `ixpg`, and `ixmr`
/// sections accepted by `libcdr`'s `cmx2raw`, and round-trips through `parse_document(&bytes)`.
pub fn write_cmx1(doc: &Document) -> Result<Vec<u8>, ProbeError> {
    write_cmx1_document(doc)
}

/// Writes a [`Document`] into a clean-room X4+ ZIP-wrapped `.cdr` container (`PK\x03\x04` containing
/// `mimetype`, `content/riffData.cdr` with `RIFF....CDRE` v1400, and `META-INF/metadata.xml`).
///
/// The emitted container satisfies `probe(&bytes)` (`CdrContainer::Zip`, `ZipCdrRole::RiffDataCdr`,
/// `effective_version = Some(1400)`) and round-trips through `parse_document(&bytes)`.
pub fn write_cdrx_zip(doc: &Document) -> Result<Vec<u8>, ProbeError> {
    let inner_riff = write_riff_document(doc, *b"CDRE", 1400)?;
    let mimetype = b"application/x-vnd.corel.zcf.draw.document+zip";
    let metadata = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<metadata title=\"{}\" pages=\"{}\"/>\n",
        doc.title.replace('"', "&quot;").replace('<', "&lt;").replace('>', "&gt;"),
        doc.pages.len()
    );
    Ok(crate::zip::build_stored_zip(&[("mimetype", mimetype), ("content/riffData.cdr", &inner_riff), ("META-INF/metadata.xml", metadata.as_bytes())]))
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
    let total_len = 4usize.saturating_add(children_bytes.len());
    out.extend_from_slice(b"LIST");
    out.extend_from_slice(&(total_len as u32).to_le_bytes());
    out.extend_from_slice(&list_type);
    out.extend_from_slice(children_bytes);
    if total_len & 1 == 1 {
        out.push(0);
    }
}

/// Encodes a 12-byte `CDRColor` (`V >= 500`) per Photopea CDR spec (`https://github.com/photopea/cdr-spec`, §Color):
/// `color_model: u16`, `color_palette: u16 = 0`, `reserved: u32 = 0`, `color_value: u32`.
fn encode_cdr_color_v500(c: &Color) -> [u8; 12] {
    let mut out = [0u8; 12];
    match c {
        Color::Cmyk { c, m, y, k } => {
            out[0..2].copy_from_slice(&2u16.to_le_bytes());
            out[8] = (c.get() * 100.0).round().clamp(0.0, 100.0) as u8;
            out[9] = (m.get() * 100.0).round().clamp(0.0, 100.0) as u8;
            out[10] = (y.get() * 100.0).round().clamp(0.0, 100.0) as u8;
            out[11] = (k.get() * 100.0).round().clamp(0.0, 100.0) as u8;
        }
        Color::Hsb { h, s, b } => {
            out[0..2].copy_from_slice(&6u16.to_le_bytes());
            out[2..4].copy_from_slice(b"CF");
            out[4..6].copy_from_slice(&((s.get() * 10000.0).round().clamp(0.0, 10000.0) as u16).to_le_bytes());
            out[6..8].copy_from_slice(&((b.get() * 10000.0).round().clamp(0.0, 10000.0) as u16).to_le_bytes());
            out[8..10].copy_from_slice(&(h.get().round().clamp(0.0, 360.0) as u16).to_le_bytes());
            out[10] = (s.get() * 255.0).round().clamp(0.0, 255.0) as u8;
            out[11] = (b.get() * 255.0).round().clamp(0.0, 255.0) as u8;
        }
        Color::Gray { k } => {
            out[0..2].copy_from_slice(&9u16.to_le_bytes());
            out[2..4].copy_from_slice(b"CF");
            out[4..6].copy_from_slice(&((k.get() * 10000.0).round().clamp(0.0, 10000.0) as u16).to_le_bytes());
            out[8] = (k.get() * 255.0).round().clamp(0.0, 255.0) as u8;
        }
        Color::Registration => {
            out[0..2].copy_from_slice(&20u16.to_le_bytes());
            out[8..12].copy_from_slice(&[100, 100, 100, 100]);
        }
        _ => {
            let [r, g, b, _] = c.to_rgba8(1.0);
            out[0..2].copy_from_slice(&5u16.to_le_bytes());
            out[8] = b;
            out[9] = g;
            out[10] = r;
            out[11] = 0;
        }
    }
    out
}

fn encode_fild_payload(fill_id: u32, fill: &FillStyle) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&fill_id.to_le_bytes());
    match fill {
        FillStyle::Uniform { color: c } => {
            p.extend_from_slice(&1u16.to_le_bytes());
            p.extend_from_slice(&0u16.to_le_bytes());
            p.extend_from_slice(&encode_cdr_color_v500(c));
        }
        FillStyle::Fountain { geometry, blend_mode, stops } => {
            p.extend_from_slice(&2u16.to_le_bytes());
            p.extend_from_slice(&0u16.to_le_bytes());
            let (sub_type, angle_deg, edge_off, cx_off, cy_off) = match geometry {
                FountainGeometry::Radial { center_x, center_y, .. } => {
                    (2u8, 0.0, 0i32, ((center_x.get() - 0.5) * 100.0).round() as i32, ((0.5 - center_y.get()) * 100.0).round() as i32)
                }
                FountainGeometry::Conical { center_x, center_y, angle_deg } => {
                    (3u8, angle_deg.get(), 0i32, ((center_x.get() - 0.5) * 100.0).round() as i32, ((0.5 - center_y.get()) * 100.0).round() as i32)
                }
                FountainGeometry::Square { center_x, center_y, angle_deg, .. } => {
                    (4u8, angle_deg.get(), 0i32, ((center_x.get() - 0.5) * 100.0).round() as i32, ((0.5 - center_y.get()) * 100.0).round() as i32)
                }
                FountainGeometry::Linear { angle_deg, edge_pad } => {
                    (1u8, angle_deg.get(), edge_pad.get().round().clamp(0.0, 49.0) as i32, 0i32, 0i32)
                }
            };
            let mode_raw: u32 = match blend_mode {
                FountainBlendMode::Direct => 0,
                FountainBlendMode::RainbowCw => 1,
                FountainBlendMode::RainbowCcw => 2,
            };
            p.push(sub_type);
            p.extend_from_slice(&[0u8; 19]);
            p.extend_from_slice(&edge_off.to_le_bytes()); // edge_offset
            let angle_raw = (angle_deg.clamp(-360.0, 360.0) * 1_000_000.0).round() as i32;
            p.extend_from_slice(&angle_raw.to_le_bytes());
            p.extend_from_slice(&cx_off.to_le_bytes()); // center_x_offset
            p.extend_from_slice(&cy_off.to_le_bytes()); // center_y_offset
            p.extend_from_slice(&0u16.to_le_bytes()); // pad
            p.extend_from_slice(&mode_raw.to_le_bytes()); // mode
            p.push(50u8); // mid_point
            p.push(0u8); // pad
            let stop_list: Vec<(Color, u32)> = if stops.is_empty() {
                vec![(Color::WHITE, 0), (Color::BLACK, 100)]
            } else {
                stops.iter().map(|s| (s.color.clone(), (s.offset.get() * 100.0).round().clamp(0.0, 100.0) as u32)).collect()
            };
            p.extend_from_slice(&(stop_list.len() as u32).to_le_bytes());
            for (col, off_pct) in stop_list {
                p.extend_from_slice(&encode_cdr_color_v500(&col));
                p.extend_from_slice(&off_pct.to_le_bytes());
            }
        }
        FillStyle::TwoColorPattern { kind, fg_color, bg_color, tile_size_pt } => {
            p.extend_from_slice(&7u16.to_le_bytes());
            p.extend_from_slice(&0u16.to_le_bytes());
            let pat_id: u32 = match kind {
                coralcraft_doc::PatternKind::PolkaDot => 2,
                coralcraft_doc::PatternKind::HorizontalStripe => 3,
                coralcraft_doc::PatternKind::VerticalStripe => 4,
                coralcraft_doc::PatternKind::DiagonalCrosshatch => 5,
                coralcraft_doc::PatternKind::Checkerboard | coralcraft_doc::PatternKind::Unknown => 1,
            };
            p.extend_from_slice(&pat_id.to_le_bytes()); // pattern_id
            let w_cdr = pt_to_cdr_i32(tile_size_pt.get().clamp(1.0, 1000.0));
            p.extend_from_slice(&w_cdr.to_le_bytes());
            p.extend_from_slice(&w_cdr.to_le_bytes());
            p.extend_from_slice(&0u16.to_le_bytes()); // tile_offset_x
            p.extend_from_slice(&0u16.to_le_bytes()); // tile_offset_y
            p.extend_from_slice(&0u16.to_le_bytes()); // rcp_offset
            p.push(0u8); // flags
            p.push(0u8); // pad
            p.extend_from_slice(&encode_cdr_color_v500(fg_color));
            p.extend_from_slice(&encode_cdr_color_v500(bg_color));
        }
        FillStyle::Unknown { tag, payload } => {
            p.extend_from_slice(&99u16.to_le_bytes());
            p.extend_from_slice(&0u16.to_le_bytes());
            p.extend_from_slice(b"UKFL");
            p.extend_from_slice(&tag.0);
            let len = payload.0.len().min(65535);
            p.extend_from_slice(&(len as u32).to_le_bytes());
            p.extend_from_slice(payload.0.get(..len).unwrap_or_default());
        }
    }
    p
}

fn encode_outl_payload(outl_id: u32, outl: &OutlineStyle) -> Vec<u8> {
    let mut p = Vec::with_capacity(136);
    p.extend_from_slice(&outl_id.to_le_bytes());
    let mut line_type = 0x0002u16; // Solid stroke enabled
    if !outl.dash_pattern.is_empty() {
        line_type |= 0x0004; // Dashed stroke enabled
    }
    if outl.behind_fill {
        line_type |= 0x0008;
    }
    if outl.scale_with_object {
        line_type |= 0x0020;
    }
    let caps: u16 = match outl.cap {
        LineCap::Butt => 0,
        LineCap::Round => 1,
        LineCap::Square => 2,
    };
    let join: u16 = match outl.join {
        LineJoin::Miter => 0,
        LineJoin::Round => 1,
        LineJoin::Bevel => 2,
    };
    p.extend_from_slice(&line_type.to_le_bytes());
    p.extend_from_slice(&caps.to_le_bytes());
    p.extend_from_slice(&join.to_le_bytes());
    p.extend_from_slice(&0u16.to_le_bytes()); // pad (600 <= V < 1300)
    p.extend_from_slice(&pt_to_cdr_i32(outl.width_pt.get()).to_le_bytes());
    p.extend_from_slice(&100u16.to_le_bytes()); // stretch = 100%
    p.extend_from_slice(&0u16.to_le_bytes()); // pad
    p.extend_from_slice(&0i32.to_le_bytes()); // angle
    p.extend_from_slice(&[0u8; 52]); // pad (600 <= V < 1300)
    p.extend_from_slice(&encode_cdr_color_v500(&outl.color)); // 12B at offset 76..88
    p.extend_from_slice(&[0u8; 16]); // pad (V >= 600)
    let num_dash = outl.dash_pattern.len().min(11);
    p.extend_from_slice(&(num_dash as u16).to_le_bytes()); // num_dash at 104..106
    let base_w = if outl.width_pt.get() > 0.0 { outl.width_pt.get() } else { 1.0 };
    let mut dash_table = [0u8; 22];
    for (i, d) in outl.dash_pattern.iter().take(11).enumerate() {
        let mult = (d.get() / base_w).round().clamp(1.0, f64::from(u16::MAX)) as u16;
        let off = i.saturating_mul(2);
        if let Some(slot) = dash_table.get_mut(off..off.saturating_add(2)) {
            slot.copy_from_slice(&mult.to_le_bytes());
        }
    }
    p.extend_from_slice(&dash_table); // fixed dash table (22B for V >= 600) at 106..128
    let encode_arrow = |a: coralcraft_doc::ArrowheadKind| -> u32 {
        match a {
            coralcraft_doc::ArrowheadKind::None => 0,
            coralcraft_doc::ArrowheadKind::Triangle => 1,
            coralcraft_doc::ArrowheadKind::OpenArrow => 2,
            coralcraft_doc::ArrowheadKind::Circle => 3,
            coralcraft_doc::ArrowheadKind::Diamond => 4,
            coralcraft_doc::ArrowheadKind::Unknown => 99,
        }
    };
    p.extend_from_slice(&encode_arrow(outl.start_arrow).to_le_bytes()); // start_marker_id at 128..132
    p.extend_from_slice(&encode_arrow(outl.end_arrow).to_le_bytes()); // end_marker_id at 132..136
    p
}

fn node_to_export_path(node: &ObjectNode) -> Path {
    match &node.kind {
        ObjectKind::Rect(r) => r.to_path().unwrap_or_default(),
        ObjectKind::Ellipse(e) => e.to_path().unwrap_or_default(),
        ObjectKind::Path(p) => p.clone(),
        ObjectKind::Polygon(p) => p.to_path().unwrap_or_default(),
        _ => RectSpec::sharp(node_bbox_for_export(node)).to_path().unwrap_or_default(),
    }
}

type CdrCurveEncoded = (Vec<(i32, i32)>, Vec<u8>);

fn encode_path_to_cdr_points_types(path: &Path, cx: f64, cy: f64) -> Result<CdrCurveEncoded, ProbeError> {
    let mut pts = Vec::new();
    let mut types = Vec::new();
    let mut cur = Point::default();
    for el in &path.elements {
        match *el {
            coralcraft_geom::PathElement::MoveTo(pt) => {
                pts.push((pt_to_cdr_i32(pt.x.get() - cx), pt_to_cdr_i32(cy - pt.y.get())));
                types.push(0x00u8);
                cur = pt;
            }
            coralcraft_geom::PathElement::LineTo(pt) => {
                pts.push((pt_to_cdr_i32(pt.x.get() - cx), pt_to_cdr_i32(cy - pt.y.get())));
                types.push(0x40u8);
                cur = pt;
            }
            coralcraft_geom::PathElement::QuadTo { ctrl, end } => {
                let (c1, c2) = coralcraft_geom::elevate_quad_to_cubic(cur, ctrl, end).unwrap_or((ctrl, ctrl));
                pts.push((pt_to_cdr_i32(c1.x.get() - cx), pt_to_cdr_i32(cy - c1.y.get())));
                types.push(0xC0u8);
                pts.push((pt_to_cdr_i32(c2.x.get() - cx), pt_to_cdr_i32(cy - c2.y.get())));
                types.push(0xC0u8);
                pts.push((pt_to_cdr_i32(end.x.get() - cx), pt_to_cdr_i32(cy - end.y.get())));
                types.push(0x80u8);
                cur = end;
            }
            coralcraft_geom::PathElement::CubicTo { c1, c2, end } => {
                pts.push((pt_to_cdr_i32(c1.x.get() - cx), pt_to_cdr_i32(cy - c1.y.get())));
                types.push(0xC0u8);
                pts.push((pt_to_cdr_i32(c2.x.get() - cx), pt_to_cdr_i32(cy - c2.y.get())));
                types.push(0xC0u8);
                pts.push((pt_to_cdr_i32(end.x.get() - cx), pt_to_cdr_i32(cy - end.y.get())));
                types.push(0x80u8);
                cur = end;
            }
            coralcraft_geom::PathElement::Close => {
                if let Some(last_t) = types.last_mut() {
                    *last_t |= 0x08;
                }
            }
        }
        if pts.len() > usize::from(u16::MAX) {
            return Err(ProbeError::LimitExceeded { limit: "cdr_curve_points".to_owned(), max: usize::from(u16::MAX) });
        }
    }
    Ok((pts, types))
}

fn encode_explicit_shape_meta(node: &ObjectNode, cx: f64, cy: f64, bbox_payload: &[u8], meta: &mut Vec<u8>) {
    match &node.kind {
        ObjectKind::Rect(spec) => {
            meta.extend_from_slice(&1u32.to_le_bytes());
            meta.extend_from_slice(bbox_payload);
            meta.extend_from_slice(b"RECT");
            for &ck in &spec.corner_kinds {
                let code = match ck {
                    coralcraft_geom::CornerKind::Round => 0u8,
                    coralcraft_geom::CornerKind::Scalloped => 1u8,
                    coralcraft_geom::CornerKind::Chamfered => 2u8,
                };
                meta.push(code);
            }
            for &r in &spec.radii {
                meta.extend_from_slice(&r.get().to_le_bytes());
            }
        }
        ObjectKind::Ellipse(spec) => {
            meta.extend_from_slice(&2u32.to_le_bytes());
            meta.extend_from_slice(bbox_payload);
            meta.extend_from_slice(b"ELLP");
            let mode_code = match spec.mode {
                coralcraft_geom::EllipseMode::Full => 0u8,
                coralcraft_geom::EllipseMode::Pie => 1u8,
                coralcraft_geom::EllipseMode::Arc => 2u8,
            };
            meta.extend_from_slice(&[mode_code, 0, 0, 0]);
            meta.extend_from_slice(&spec.start_deg.get().to_le_bytes());
            meta.extend_from_slice(&spec.end_deg.get().to_le_bytes());
        }
        ObjectKind::Path(path) => {
            encode_path_as_loda_1e(path, cx, cy, meta);
        }
        ObjectKind::Polygon(spec) => {
            meta.extend_from_slice(&4u32.to_le_bytes());
            meta.extend_from_slice(b"POLY");
            meta.extend_from_slice(&spec.points.to_le_bytes());
            meta.extend_from_slice(&pt_to_cdr_i32(spec.center.x.get() - cx).to_le_bytes());
            meta.extend_from_slice(&pt_to_cdr_i32(cy - spec.center.y.get()).to_le_bytes());
            meta.extend_from_slice(&pt_to_cdr_i32(spec.radius.get()).to_le_bytes());
            let inner = spec.inner_radius.map(|r| r.get()).unwrap_or(0.0);
            meta.extend_from_slice(&pt_to_cdr_i32(inner).to_le_bytes());
            meta.extend_from_slice(&spec.rotation_deg.get().to_le_bytes());
        }
        ObjectKind::ArtisticText { text, font_family, size_pt, position } => {
            meta.extend_from_slice(&5u32.to_le_bytes());
            meta.extend_from_slice(b"ATXT");
            meta.extend_from_slice(&pt_to_cdr_i32(position.x.get() - cx).to_le_bytes());
            meta.extend_from_slice(&pt_to_cdr_i32(cy - position.y.get()).to_le_bytes());
            meta.extend_from_slice(&pt_to_cdr_i32(size_pt.get()).to_le_bytes());
            let fam_bytes = font_family.as_bytes();
            meta.extend_from_slice(&(fam_bytes.len() as u16).to_le_bytes());
            meta.extend_from_slice(fam_bytes);
            let txt_bytes = text.as_bytes();
            meta.extend_from_slice(&(txt_bytes.len() as u32).to_le_bytes());
            meta.extend_from_slice(txt_bytes);
        }
        ObjectKind::ParagraphText(pt) => {
            meta.extend_from_slice(&7u32.to_le_bytes());
            meta.extend_from_slice(b"PTXT");
            meta.extend_from_slice(&pt_to_cdr_i32(pt.position.x.get() - cx).to_le_bytes());
            meta.extend_from_slice(&pt_to_cdr_i32(cy - pt.position.y.get()).to_le_bytes());
            meta.extend_from_slice(&pt_to_cdr_i32(pt.frame_width_pt.get()).to_le_bytes());
            meta.extend_from_slice(&pt_to_cdr_i32(pt.frame_height_pt.get()).to_le_bytes());
            meta.extend_from_slice(&pt_to_cdr_i32(pt.font_size_pt.get()).to_le_bytes());
            let align_code: u8 = match pt.alignment {
                TextAlignment::Left => 0,
                TextAlignment::Center => 1,
                TextAlignment::Right => 2,
                TextAlignment::Justify => 3,
            };
            meta.push(align_code);
            meta.push((pt.columns.clamp(1, 64)) as u8);
            let gutter_tenths = (pt.gutter_pt.get().clamp(0.0, 6000.0) * 10.0).round() as u16;
            let spacing_pct = (pt.line_spacing_multiplier.get().clamp(0.5, 5.0) * 100.0).round() as u16;
            meta.extend_from_slice(&gutter_tenths.to_le_bytes());
            meta.extend_from_slice(&spacing_pct.to_le_bytes());
            let fam_bytes = pt.font_family.as_bytes();
            meta.extend_from_slice(&(fam_bytes.len() as u16).to_le_bytes());
            meta.extend_from_slice(fam_bytes);
            let txt_bytes = pt.text.as_bytes();
            meta.extend_from_slice(&(txt_bytes.len() as u32).to_le_bytes());
            meta.extend_from_slice(txt_bytes);
        }
        ObjectKind::Bitmap { pixel_width, pixel_height, dpi_x, dpi_y, rgba_pixels } => {
            meta.extend_from_slice(&6u32.to_le_bytes());
            meta.extend_from_slice(b"BMP ");
            meta.extend_from_slice(&pixel_width.to_le_bytes());
            meta.extend_from_slice(&pixel_height.to_le_bytes());
            meta.extend_from_slice(&(dpi_x.get() as f32).to_le_bytes());
            meta.extend_from_slice(&(dpi_y.get() as f32).to_le_bytes());
            meta.extend_from_slice(&(rgba_pixels.len() as u32).to_le_bytes());
            meta.extend_from_slice(rgba_pixels);
        }
        ObjectKind::Group { .. } => {}
        ObjectKind::Unsupported(u) => {
            meta.extend_from_slice(&0x99u32.to_le_bytes());
            meta.extend_from_slice(b"UNSP");
            meta.extend_from_slice(&u.tag.0);
            let mut flags = 0u8;
            if u.absolute_geometry {
                flags |= 0x01;
            }
            if u.fallback_bounds.is_some() {
                flags |= 0x02;
            }
            meta.push(flags);
            let fb = u.fallback_bounds.unwrap_or_default();
            meta.extend_from_slice(&fb.x0.get().to_le_bytes());
            meta.extend_from_slice(&fb.y0.get().to_le_bytes());
            meta.extend_from_slice(&fb.x1.get().to_le_bytes());
            meta.extend_from_slice(&fb.y1.get().to_le_bytes());
            let hint_bytes = u.class_hint.as_deref().unwrap_or("").as_bytes();
            let hint_len = hint_bytes.len().min(1024);
            meta.extend_from_slice(&(hint_len as u16).to_le_bytes());
            meta.extend_from_slice(hint_bytes.get(..hint_len).unwrap_or_default());
            let pay_len = u.payload.0.len().min(65535);
            meta.extend_from_slice(&(pay_len as u32).to_le_bytes());
            meta.extend_from_slice(u.payload.0.get(..pay_len).unwrap_or_default());
        }
    }
}

fn encode_object_into_layer(node: &ObjectNode, page_size: Size, next_style_id: &mut u32, layer_children: &mut Vec<u8>) -> Result<(), ProbeError> {
    if let ObjectKind::Group { children } = &node.kind {
        let mut grp_children = Vec::new();
        let cx = page_size.width.get() * 0.5;
        let cy = page_size.height.get() * 0.5;
        if node.transform != Affine2::IDENTITY {
            let mut trfd = Vec::with_capacity(72);
            trfd.extend_from_slice(&72u32.to_le_bytes());
            trfd.extend_from_slice(&1u32.to_le_bytes());
            trfd.extend_from_slice(&12u32.to_le_bytes());
            trfd.extend_from_slice(&16u32.to_le_bytes());
            trfd.extend_from_slice(&0x0008u16.to_le_bytes());
            trfd.extend_from_slice(b"CCPT\0\0");
            let x0_cdr = node.transform.tx.get() * 254_000.0 / 72.0;
            let y0_cdr = -node.transform.ty.get() * 254_000.0 / 72.0;
            trfd.extend_from_slice(&node.transform.a.get().to_le_bytes());
            trfd.extend_from_slice(&node.transform.c.get().to_le_bytes());
            trfd.extend_from_slice(&x0_cdr.to_le_bytes());
            trfd.extend_from_slice(&node.transform.b.get().to_le_bytes());
            trfd.extend_from_slice(&node.transform.d.get().to_le_bytes());
            trfd.extend_from_slice(&y0_cdr.to_le_bytes());
            append_riff_chunk(&mut grp_children, *b"trfd", &trfd);
        }
        if let Some(trns) = &node.transparency {
            let tp = encode_trns_payload(trns);
            append_riff_chunk(&mut grp_children, *b"trns", &tp);
        }
        if node.name.is_some() || !node.visible || node.locked || (node.opacity.get() - 1.0).abs() > 1e-6 {
            let omta = encode_omta_payload(node);
            append_riff_chunk(&mut grp_children, *b"omta", &omta);
        }
        for effect in &node.effects {
            if let coralcraft_doc::VectorEffect::Envelope { corners } = effect {
                let mut enve = Vec::with_capacity(32);
                for pt in corners {
                    enve.extend_from_slice(&pt_to_cdr_i32(pt.x.get() - cx).to_le_bytes());
                    enve.extend_from_slice(&pt_to_cdr_i32(cy - pt.y.get()).to_le_bytes());
                }
                append_riff_chunk(&mut grp_children, *b"enve", &enve);
            } else if let Some(vefx) = encode_vefx_payload(effect, cx, cy) {
                append_riff_chunk(&mut grp_children, *b"vefx", &vefx);
            }
        }
        if !node.powerclip_children.is_empty() {
            let mut pclp_children = Vec::new();
            for pc in &node.powerclip_children {
                encode_object_into_layer(pc, page_size, next_style_id, &mut pclp_children)?;
            }
            append_riff_list(&mut grp_children, *b"pclp", &pclp_children);
        }
        for rc in &node.residual.chunks {
            if !matches!(&rc.tag.0, b"trfd" | b"trns" | b"omta" | b"enve" | b"vefx" | b"LIST") && !rc.payload.0.is_empty() {
                append_riff_chunk(&mut grp_children, rc.tag.0, &rc.payload.0);
            }
        }
        for child in children {
            encode_object_into_layer(child, page_size, next_style_id, &mut grp_children)?;
        }
        append_riff_list(layer_children, *b"grp ", &grp_children);
        return Ok(());
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
        // Standard 72-byte `trfd` chunk (`V=700`) per Photopea CDR spec (`https://github.com/photopea/cdr-spec`, §`trfd`)
        let mut trfd = Vec::with_capacity(72);
        trfd.extend_from_slice(&72u32.to_le_bytes()); // chunk_length
        trfd.extend_from_slice(&1u32.to_le_bytes()); // num_args = 1
        trfd.extend_from_slice(&12u32.to_le_bytes()); // start_of_args = 12
        trfd.extend_from_slice(&16u32.to_le_bytes()); // arg_offset_0 = 16
        trfd.extend_from_slice(&0x0008u16.to_le_bytes()); // tmp_type = 0x08 (trafo)
        trfd.extend_from_slice(b"CCPT\0\0"); // 6-byte pad (V >= 600)
        let x0_cdr = node.transform.tx.get() * 254_000.0 / 72.0;
        let y0_cdr = -node.transform.ty.get() * 254_000.0 / 72.0;
        trfd.extend_from_slice(&node.transform.a.get().to_le_bytes());
        trfd.extend_from_slice(&node.transform.c.get().to_le_bytes());
        trfd.extend_from_slice(&x0_cdr.to_le_bytes());
        trfd.extend_from_slice(&node.transform.b.get().to_le_bytes());
        trfd.extend_from_slice(&node.transform.d.get().to_le_bytes());
        trfd.extend_from_slice(&y0_cdr.to_le_bytes());
        append_riff_chunk(&mut obj_children, *b"trfd", &trfd);
    }

    let mut fill_id = 0u32;
    if let Some(fill) = &node.fill {
        fill_id = *next_style_id;
        *next_style_id = next_style_id.saturating_add(1);
        let fp = encode_fild_payload(fill_id, fill);
        append_riff_chunk(&mut obj_children, *b"fild", &fp);
    }
    let mut outl_id = 0u32;
    if let Some(outl) = &node.outline {
        outl_id = *next_style_id;
        *next_style_id = next_style_id.saturating_add(1);
        let op = encode_outl_payload(outl_id, outl);
        append_riff_chunk(&mut obj_children, *b"outl", &op);
    }

    if let Some(trns) = &node.transparency {
        let tp = encode_trns_payload(trns);
        append_riff_chunk(&mut obj_children, *b"trns", &tp);
    }

    if node.name.is_some() || !node.visible || node.locked || (node.opacity.get() - 1.0).abs() > 1e-6 {
        let omta = encode_omta_payload(node);
        append_riff_chunk(&mut obj_children, *b"omta", &omta);
    }

    if !node.powerclip_children.is_empty() {
        let mut pclp_children = Vec::new();
        for child in &node.powerclip_children {
            encode_object_into_layer(child, page_size, next_style_id, &mut pclp_children)?;
        }
        append_riff_list(&mut obj_children, *b"pclp", &pclp_children);
    }

    for effect in &node.effects {
        if let coralcraft_doc::VectorEffect::Envelope { corners } = effect {
            let mut enve = Vec::with_capacity(32);
            for pt in corners {
                enve.extend_from_slice(&pt_to_cdr_i32(pt.x.get() - cx).to_le_bytes());
                enve.extend_from_slice(&pt_to_cdr_i32(cy - pt.y.get()).to_le_bytes());
            }
            append_riff_chunk(&mut obj_children, *b"enve", &enve);
        } else if let Some(vefx) = encode_vefx_payload(effect, cx, cy) {
            append_riff_chunk(&mut obj_children, *b"vefx", &vefx);
        }
    }

    if let ObjectKind::ParagraphText(pt) = &node.kind {
        let mut txtd = Vec::with_capacity(1usize.saturating_add(pt.text.len()));
        txtd.push(0x01); // UTF-8 encoding tag per Photopea CDR spec §Text
        txtd.extend_from_slice(pt.text.as_bytes());
        append_riff_chunk(&mut obj_children, *b"txtd", &txtd);
    }

    let export_path = node_to_export_path(node);
    let (pts, types) = encode_path_to_cdr_points_types(&export_path, cx, cy)?;
    let point_num =
        u16::try_from(pts.len()).map_err(|_| ProbeError::LimitExceeded { limit: "cdr_curve_points".to_owned(), max: usize::from(u16::MAX) })?;

    let mut arg_1e = Vec::new();
    arg_1e.extend_from_slice(&point_num.to_le_bytes());
    arg_1e.extend_from_slice(&0u16.to_le_bytes());
    for (x_cdr, y_cdr) in &pts {
        arg_1e.extend_from_slice(&x_cdr.to_le_bytes());
        arg_1e.extend_from_slice(&y_cdr.to_le_bytes());
    }
    arg_1e.extend_from_slice(&types);
    arg_1e.extend_from_slice(b"CCMETA");
    encode_explicit_shape_meta(node, cx, cy, &bbox_payload, &mut arg_1e);
    while arg_1e.len() & 3 != 0 {
        arg_1e.push(0);
    }

    // Build standard `loda` chunk (`chunk_type = 0x03` LineAndCurve, with `0x1e` coords, `0x14` fill_id, `0x0a` outl_id)
    let mut args: Vec<(u32, Vec<u8>)> = vec![(0x1e, arg_1e)];
    if fill_id != 0 {
        args.push((0x14, fill_id.to_le_bytes().to_vec()));
    }
    if outl_id != 0 {
        args.push((0x0a, outl_id.to_le_bytes().to_vec()));
    }
    let num_args = args.len() as u32;
    let start_of_args = 20u32;
    let start_of_arg_types = 20u32.saturating_add(4u32.saturating_mul(num_args));
    let mut running_off = start_of_arg_types.saturating_add(4u32.saturating_mul(num_args));
    let mut arg_offsets = Vec::with_capacity(args.len());
    for (_, data) in &args {
        arg_offsets.push(running_off);
        running_off = running_off.saturating_add(data.len() as u32);
    }
    let chunk_length = running_off;

    let mut loda = Vec::with_capacity(chunk_length as usize);
    loda.extend_from_slice(&chunk_length.to_le_bytes());
    loda.extend_from_slice(&num_args.to_le_bytes());
    loda.extend_from_slice(&start_of_args.to_le_bytes());
    loda.extend_from_slice(&start_of_arg_types.to_le_bytes());
    loda.extend_from_slice(&0x03u32.to_le_bytes()); // chunk_type = 0x03 (LineAndCurve)
    for off in &arg_offsets {
        loda.extend_from_slice(&off.to_le_bytes());
    }
    // `arg_types` are stored in reverse order in CDR `loda` headers
    for (arg_type, _) in args.iter().rev() {
        loda.extend_from_slice(&arg_type.to_le_bytes());
    }
    for (_, data) in args {
        loda.extend_from_slice(&data);
    }

    append_riff_chunk(&mut obj_children, *b"loda", &loda);
    if let ObjectKind::Unsupported(u) = &node.kind {
        if !u.children.is_empty() {
            let mut uchl_children = Vec::new();
            for uch in &u.children {
                encode_object_into_layer(uch, page_size, next_style_id, &mut uchl_children)?;
            }
            append_riff_list(&mut obj_children, *b"uchl", &uchl_children);
        }
        if !u.residual.chunks.is_empty() {
            let mut ures_children = Vec::new();
            for rc in &u.residual.chunks {
                if !rc.payload.0.is_empty() {
                    append_riff_chunk(&mut ures_children, rc.tag.0, &rc.payload.0);
                }
            }
            append_riff_list(&mut obj_children, *b"ures", &ures_children);
        }
    }
    for rc in &node.residual.chunks {
        if !matches!(
            &rc.tag.0,
            b"bbox"
                | b"trfd"
                | b"fild"
                | b"outl"
                | b"trns"
                | b"omta"
                | b"vefx"
                | b"enve"
                | b"txtd"
                | b"txts"
                | b"loda"
                | b"lobj"
                | b"bmp "
                | b"CDIB"
                | b"LIST"
        ) && !rc.payload.0.is_empty()
        {
            append_riff_chunk(&mut obj_children, rc.tag.0, &rc.payload.0);
        }
    }
    append_riff_list(layer_children, *b"obj ", &obj_children);
    Ok(())
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
                let (c1, c2) = coralcraft_geom::elevate_quad_to_cubic(cur, ctrl, end).unwrap_or((ctrl, ctrl));
                loda.push(0x80);
                loda.extend_from_slice(&pt_to_cdr_i32(c1.x.get() - cx).to_le_bytes());
                loda.extend_from_slice(&pt_to_cdr_i32(cy - c1.y.get()).to_le_bytes());
                loda.extend_from_slice(&pt_to_cdr_i32(c2.x.get() - cx).to_le_bytes());
                loda.extend_from_slice(&pt_to_cdr_i32(cy - c2.y.get()).to_le_bytes());
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
        ObjectKind::ParagraphText(pt) => {
            let x0 = pt.position.x.get();
            let y0 = pt.position.y.get();
            let w = pt.frame_width_pt.get().max(1.0);
            let h = pt.frame_height_pt.get().max(1.0);
            Rect::new(x0, y0, x0 + w, y0 + h).unwrap_or(default_rect)
        }
        ObjectKind::Bitmap { pixel_width, pixel_height, dpi_x, dpi_y, .. } => {
            let w = (f64::from(*pixel_width) * 72.0 / dpi_x.get().max(1.0)).max(1.0);
            let h = (f64::from(*pixel_height) * 72.0 / dpi_y.get().max(1.0)).max(1.0);
            Rect::new(0.0, 0.0, w, h).unwrap_or(default_rect)
        }
        ObjectKind::Group { .. } => default_rect,
        ObjectKind::Unsupported(u) => u.fallback_bounds.unwrap_or(default_rect),
    }
}

fn write_riff_document(doc: &Document, form_type: [u8; 4], version: u16) -> Result<Vec<u8>, ProbeError> {
    let mut body = Vec::new();
    append_riff_chunk(&mut body, *b"vrsn", &version.to_le_bytes());
    let dmta = encode_dmta_payload(doc);
    append_riff_chunk(&mut body, *b"dmta", &dmta);
    if doc.palette != coralcraft_color::Palette::default_workspace_palette() {
        let xml_pal = coralcraft_color::serialize_xml_palette(&doc.palette);
        append_riff_chunk(&mut body, *b"plte", xml_pal.as_bytes());
    }
    for rc in &doc.residual.chunks {
        if !matches!(&rc.tag.0, b"vrsn" | b"dmta" | b"plte" | b"mcfg" | b"fild" | b"outl" | b"LIST") && !rc.payload.0.is_empty() {
            append_riff_chunk(&mut body, rc.tag.0, &rc.payload.0);
        }
    }

    let mcfg_off = if version >= 1300 {
        12usize
    } else if version >= 900 {
        4usize
    } else {
        0usize
    };
    let mut next_style_id = 1u32;
    if !doc.master_layers.is_empty() {
        let master_page_size = doc.pages.first().map(|p| p.size).unwrap_or_else(|| default_a4_page(1).size);
        let mut mstr_children = Vec::new();
        for mlayer in &doc.master_layers {
            let mut layer_children = Vec::new();
            let lmta = encode_lmta_payload(mlayer);
            append_riff_chunk(&mut layer_children, *b"lmta", &lmta);
            for rc in &mlayer.residual.chunks {
                if !matches!(&rc.tag.0, b"lmta" | b"LIST") && !rc.payload.0.is_empty() {
                    append_riff_chunk(&mut layer_children, rc.tag.0, &rc.payload.0);
                }
            }
            for obj in &mlayer.objects {
                encode_object_into_layer(obj, master_page_size, &mut next_style_id, &mut layer_children)?;
            }
            append_riff_list(&mut mstr_children, *b"layr", &layer_children);
        }
        append_riff_list(&mut body, *b"mstr", &mstr_children);
    }
    for page in &doc.pages {
        let mut page_children = Vec::new();
        let mut mcfg = vec![0u8; mcfg_off];
        mcfg.extend_from_slice(&pt_to_cdr_i32(page.size.width.get()).to_le_bytes());
        mcfg.extend_from_slice(&pt_to_cdr_i32(page.size.height.get()).to_le_bytes());
        append_riff_chunk(&mut page_children, *b"mcfg", &mcfg);
        let pmta = encode_pmta_payload(page);
        append_riff_chunk(&mut page_children, *b"pmta", &pmta);
        for rc in &page.residual.chunks {
            if !matches!(&rc.tag.0, b"mcfg" | b"pmta" | b"LIST") && !rc.payload.0.is_empty() {
                append_riff_chunk(&mut page_children, rc.tag.0, &rc.payload.0);
            }
        }

        for layer in &page.layers {
            let mut layer_children = Vec::new();
            let lmta = encode_lmta_payload(layer);
            append_riff_chunk(&mut layer_children, *b"lmta", &lmta);
            for rc in &layer.residual.chunks {
                if !matches!(&rc.tag.0, b"lmta" | b"LIST") && !rc.payload.0.is_empty() {
                    append_riff_chunk(&mut layer_children, rc.tag.0, &rc.payload.0);
                }
            }
            for obj in &layer.objects {
                encode_object_into_layer(obj, page.size, &mut next_style_id, &mut layer_children)?;
            }
            append_riff_list(&mut page_children, *b"layr", &layer_children);
        }
        append_riff_list(&mut body, *b"page", &page_children);
    }

    let riff_size = 4u32.saturating_add(body.len() as u32);
    let mut out = Vec::with_capacity(8usize.saturating_add(riff_size as usize));
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&riff_size.to_le_bytes());
    out.extend_from_slice(&form_type);
    out.extend_from_slice(&body);
    Ok(out)
}

struct CmxWriteTables {
    colors: Vec<Color>,
    pens: Vec<i32>,
    line_styles: Vec<(u8, u8)>,
    dots: Vec<Vec<u16>>,
    outlines: Vec<(u16, u16, u16, u16)>,
}

struct CmxFountainSpec {
    f_type: u16,
    pad_pct: u16,
    angle_micro: i32,
    x_off: i32,
    y_off: i32,
    mode_raw: u16,
    stop_entries: Vec<(u16, u16)>,
}

fn encode_cmx_meta_tag(node: &ObjectNode, cx: f64, cy: f64, bbox_payload: &[u8]) -> Option<Vec<u8>> {
    let has_omta = node.name.is_some() || !node.visible || node.locked || (node.opacity.get() - 1.0).abs() > 1e-6;
    let has_trfd = node.transform != Affine2::IDENTITY;
    let has_trns = node.transparency.is_some();
    let has_shape = !matches!(node.kind, ObjectKind::Path(_) | ObjectKind::Group { .. });
    if !has_omta && !has_trfd && !has_trns && !has_shape {
        return None;
    }
    let mut meta = Vec::new();
    meta.extend_from_slice(b"CCMX");
    if has_omta {
        let omta = encode_omta_payload(node);
        let omta_len = omta.len().min(usize::from(u16::MAX));
        meta.extend_from_slice(&(omta_len as u16).to_le_bytes());
        meta.extend_from_slice(omta.get(..omta_len).unwrap_or_default());
    } else {
        meta.extend_from_slice(&0u16.to_le_bytes());
    }
    if has_trfd {
        meta.push(1u8);
        meta.extend_from_slice(&node.transform.a.get().to_le_bytes());
        meta.extend_from_slice(&node.transform.b.get().to_le_bytes());
        meta.extend_from_slice(&node.transform.c.get().to_le_bytes());
        meta.extend_from_slice(&node.transform.d.get().to_le_bytes());
        meta.extend_from_slice(&node.transform.tx.get().to_le_bytes());
        meta.extend_from_slice(&node.transform.ty.get().to_le_bytes());
    } else {
        meta.push(0u8);
    }
    if let Some(trns) = &node.transparency {
        let tp = encode_trns_payload(trns);
        let tp_len = tp.len().min(usize::from(u16::MAX));
        meta.push(1u8);
        meta.extend_from_slice(&(tp_len as u16).to_le_bytes());
        meta.extend_from_slice(tp.get(..tp_len).unwrap_or_default());
    } else {
        meta.push(0u8);
    }
    if has_shape {
        let mut shape_meta = Vec::new();
        encode_explicit_shape_meta(node, cx, cy, bbox_payload, &mut shape_meta);
        if shape_meta.len() <= 60_000 {
            meta.extend_from_slice(&(shape_meta.len() as u16).to_le_bytes());
            meta.extend_from_slice(&shape_meta);
        } else {
            meta.extend_from_slice(&0u16.to_le_bytes());
        }
    } else {
        meta.extend_from_slice(&0u16.to_le_bytes());
    }
    if meta.len().saturating_add(3) <= usize::from(u16::MAX) { Some(meta) } else { None }
}

fn encode_cmx_node(node: &ObjectNode, cx: f64, cy: f64, tables: &mut CmxWriteTables, page_cmds: &mut Vec<u8>) -> Result<(), ProbeError> {
    if let ObjectKind::Group { children } = &node.kind {
        // CMX_Command_BeginGroup (code = 14)
        let bbox = node_bbox_for_export(node);
        let mut bbox_payload = Vec::with_capacity(16);
        bbox_payload.extend_from_slice(&pt_to_cdr_i32(bbox.x0.get() - cx).to_le_bytes());
        bbox_payload.extend_from_slice(&pt_to_cdr_i32(cy - bbox.y0.get()).to_le_bytes());
        bbox_payload.extend_from_slice(&pt_to_cdr_i32(bbox.x1.get() - cx).to_le_bytes());
        bbox_payload.extend_from_slice(&pt_to_cdr_i32(cy - bbox.y1.get()).to_le_bytes());
        let mut grp_body = Vec::with_capacity(30);
        grp_body.push(1u8); // CMX_Tag_BeginGroup_GroupSpecification
        grp_body.extend_from_slice(&29u16.to_le_bytes()); // tagLength = 29
        grp_body.extend_from_slice(&bbox_payload);
        grp_body.extend_from_slice(&0u16.to_le_bytes()); // groupCount = 0
        grp_body.extend_from_slice(&(children.len().min(usize::from(u16::MAX)) as u16).to_le_bytes()); // commandCount
        grp_body.extend_from_slice(&0u32.to_le_bytes()); // commandSize = 0
        grp_body.extend_from_slice(&0u16.to_le_bytes()); // flags = 0
        if let Some(meta) = encode_cmx_meta_tag(node, cx, cy, &bbox_payload) {
            let tag_len = (3usize.saturating_add(meta.len())) as u16;
            grp_body.push(99u8);
            grp_body.extend_from_slice(&tag_len.to_le_bytes());
            grp_body.extend_from_slice(&meta);
        }
        grp_body.push(255u8); // CMX_Tag_EndTag
        if let Ok(inst_sz) = i16::try_from(4usize.saturating_add(grp_body.len())) {
            page_cmds.extend_from_slice(&inst_sz.to_le_bytes());
            page_cmds.extend_from_slice(&14i16.to_le_bytes()); // CMX_Command_BeginGroup
            page_cmds.extend_from_slice(&grp_body);
        } else {
            let inst_sz = (8usize.saturating_add(grp_body.len())) as i32;
            page_cmds.extend_from_slice(&(-1i16).to_le_bytes());
            page_cmds.extend_from_slice(&inst_sz.to_le_bytes());
            page_cmds.extend_from_slice(&14i16.to_le_bytes());
            page_cmds.extend_from_slice(&grp_body);
        }

        for child in children {
            encode_cmx_node(child, cx, cy, tables, page_cmds)?;
        }

        // CMX_Command_EndGroup (code = 15)
        page_cmds.extend_from_slice(&4i16.to_le_bytes());
        page_cmds.extend_from_slice(&15i16.to_le_bytes());
        return Ok(());
    }

    let export_path = node_to_export_path(node);
    let (pts, types) = encode_path_to_cdr_points_types(&export_path, cx, cy)?;
    let point_num =
        u16::try_from(pts.len()).map_err(|_| ProbeError::LimitExceeded { limit: "cmx_polycurve_points".to_owned(), max: usize::from(u16::MAX) })?;
    let point_tag_len_usize = 5usize.saturating_add(pts.len().saturating_mul(9));
    let point_tag_len = u16::try_from(point_tag_len_usize)
        .map_err(|_| ProbeError::LimitExceeded { limit: "cmx_polycurve_tag_bytes".to_owned(), max: usize::from(u16::MAX) })?;

    let mut render_attr = Vec::new();
    let mut mask = 0u8;
    let mut fill_color_idx = 1u16;
    let mut cmx_fountain: Option<CmxFountainSpec> = None;
    let mut cmx_mono_bm: Option<(u16, i32, u16, u16)> = None;
    if let Some(fill) = &node.fill {
        mask |= 0x01;
        match fill {
            FillStyle::Fountain { geometry, blend_mode, stops } if stops.len() >= 2 => {
                let (f_type, pad_pct, angle_micro, x_off, y_off) = match geometry {
                    FountainGeometry::Linear { angle_deg, edge_pad } => {
                        (0u16, edge_pad.get().round().clamp(0.0, 49.0) as u16, (angle_deg.get() * 1_000_000.0).round() as i32, 0i32, 0i32)
                    }
                    FountainGeometry::Radial { center_x, center_y, .. } => {
                        (1u16, 0u16, 0i32, ((center_x.get() - 0.5) * 100.0).round() as i32, ((0.5 - center_y.get()) * 100.0).round() as i32)
                    }
                    FountainGeometry::Conical { center_x, center_y, angle_deg } => (
                        2u16,
                        0u16,
                        (angle_deg.get() * 1_000_000.0).round() as i32,
                        ((center_x.get() - 0.5) * 100.0).round() as i32,
                        ((0.5 - center_y.get()) * 100.0).round() as i32,
                    ),
                    FountainGeometry::Square { center_x, center_y, angle_deg, .. } => (
                        3u16,
                        0u16,
                        (angle_deg.get() * 1_000_000.0).round() as i32,
                        ((center_x.get() - 0.5) * 100.0).round() as i32,
                        ((0.5 - center_y.get()) * 100.0).round() as i32,
                    ),
                };
                let mode_raw: u16 = match blend_mode {
                    FountainBlendMode::Direct => 0,
                    FountainBlendMode::RainbowCw => 1,
                    FountainBlendMode::RainbowCcw => 2,
                };
                let mut stop_entries = Vec::with_capacity(stops.len().min(64));
                for st in stops.iter().take(64) {
                    tables.colors.push(st.color.clone());
                    let c_idx = tables.colors.len() as u16;
                    let pos_pct = (st.offset.get() * 100.0).round().clamp(0.0, 100.0) as u16;
                    stop_entries.push((c_idx, pos_pct));
                }
                cmx_fountain = Some(CmxFountainSpec { f_type, pad_pct, angle_micro, x_off, y_off, mode_raw, stop_entries });
            }
            FillStyle::TwoColorPattern { kind, fg_color, bg_color, tile_size_pt } => {
                let bm_idx: u16 = match kind {
                    coralcraft_doc::PatternKind::PolkaDot => 2,
                    coralcraft_doc::PatternKind::HorizontalStripe => 3,
                    coralcraft_doc::PatternKind::VerticalStripe => 4,
                    coralcraft_doc::PatternKind::DiagonalCrosshatch => 5,
                    coralcraft_doc::PatternKind::Checkerboard | coralcraft_doc::PatternKind::Unknown => 1,
                };
                let w_cdr = pt_to_cdr_i32(tile_size_pt.get().clamp(1.0, 1000.0));
                tables.colors.push(fg_color.clone());
                let fg_idx = tables.colors.len() as u16;
                tables.colors.push(bg_color.clone());
                let bg_idx = tables.colors.len() as u16;
                cmx_mono_bm = Some((bm_idx, w_cdr, fg_idx, bg_idx));
            }
            _ => {
                let c = match fill {
                    FillStyle::Uniform { color } => color.clone(),
                    FillStyle::Fountain { stops, .. } => stops.first().map(|s| s.color.clone()).unwrap_or(Color::WHITE),
                    FillStyle::TwoColorPattern { fg_color, .. } => fg_color.clone(),
                    FillStyle::Unknown { .. } => Color::rgb8(128, 128, 128),
                };
                tables.colors.push(c);
                fill_color_idx = tables.colors.len() as u16;
            }
        }
    }
    let mut outl_idx = 1u16;
    if let Some(outl) = &node.outline {
        mask |= 0x02;
        tables.colors.push(outl.color.clone());
        let c_idx = tables.colors.len() as u16;
        let base_w = outl.width_pt.get().max(0.25);
        tables.pens.push(pt_to_cdr_i32(base_w));
        let p_idx = tables.pens.len() as u16;
        let mut spec = 0x02u8;
        if !outl.dash_pattern.is_empty() {
            spec |= 0x04;
        }
        if outl.behind_fill {
            spec |= 0x08;
        }
        if outl.scale_with_object {
            spec |= 0x20;
        }
        let cap_code: u8 = match outl.cap {
            LineCap::Butt => 0,
            LineCap::Round => 1,
            LineCap::Square => 2,
        };
        let join_code: u8 = match outl.join {
            LineJoin::Miter => 0,
            LineJoin::Round => 1,
            LineJoin::Bevel => 2,
        };
        tables.line_styles.push((spec, (join_code << 4) | cap_code));
        let ls_idx = tables.line_styles.len() as u16;
        let dash_idx = if outl.dash_pattern.is_empty() {
            0u16
        } else {
            let arr: Vec<u16> =
                outl.dash_pattern.iter().take(11).map(|d| (d.get() / base_w).round().clamp(1.0, f64::from(u16::MAX)) as u16).collect();
            tables.dots.push(arr);
            tables.dots.len() as u16
        };
        tables.outlines.push((ls_idx, c_idx, p_idx, dash_idx));
        outl_idx = tables.outlines.len() as u16;
    }
    if node.transparency.is_some() {
        mask |= 0x04;
    }
    render_attr.push(mask);
    if (mask & 0x01) != 0 {
        if let Some(ftn) = &cmx_fountain {
            let color_sub_len = (5usize.saturating_add(ftn.stop_entries.len().saturating_mul(4))) as u16;
            let fill_spec_len = (40usize.saturating_add(ftn.stop_entries.len().saturating_mul(4))) as u16;
            render_attr.push(1u8); // CMX_Tag_RenderAttr_FillSpec
            render_attr.extend_from_slice(&fill_spec_len.to_le_bytes());
            render_attr.extend_from_slice(&2u16.to_le_bytes()); // fillType = 2 (Fountain)
            render_attr.push(1u8); // CMX_Tag_RenderAttr_FillSpec_Fountain_Base
            render_attr.extend_from_slice(&29u16.to_le_bytes()); // subTagLength = 29
            render_attr.extend_from_slice(&ftn.f_type.to_le_bytes());
            render_attr.extend_from_slice(&100u16.to_le_bytes()); // screen = 100
            render_attr.extend_from_slice(&ftn.pad_pct.to_le_bytes());
            render_attr.extend_from_slice(&ftn.angle_micro.to_le_bytes());
            render_attr.extend_from_slice(&ftn.x_off.to_le_bytes());
            render_attr.extend_from_slice(&ftn.y_off.to_le_bytes());
            render_attr.extend_from_slice(&256u16.to_le_bytes()); // steps = 256
            render_attr.extend_from_slice(&ftn.mode_raw.to_le_bytes());
            render_attr.extend_from_slice(&0u16.to_le_bytes()); // rateMethod = 0
            render_attr.extend_from_slice(&50u16.to_le_bytes()); // rateValue = 50
            render_attr.push(2u8); // CMX_Tag_RenderAttr_FillSpec_Fountain_Color
            render_attr.extend_from_slice(&color_sub_len.to_le_bytes());
            render_attr.extend_from_slice(&(ftn.stop_entries.len() as u16).to_le_bytes());
            for &(c_idx, pos_pct) in &ftn.stop_entries {
                render_attr.extend_from_slice(&c_idx.to_le_bytes());
                render_attr.extend_from_slice(&pos_pct.to_le_bytes());
            }
            render_attr.push(255u8); // EndTag (Fountain)
            render_attr.push(255u8); // EndTag (FillSpec)
        } else if let Some(&(bm_idx, w_cdr, fg_idx, bg_idx)) = cmx_mono_bm.as_ref() {
            render_attr.push(1u8); // CMX_Tag_RenderAttr_FillSpec
            render_attr.extend_from_slice(&33u16.to_le_bytes()); // tagLength = 33
            render_attr.extend_from_slice(&7u16.to_le_bytes()); // fillType = 7 (MonoBM)
            render_attr.push(1u8); // CMX_Tag_RenderAttr_FillSpec_MonoBM
            render_attr.extend_from_slice(&27u16.to_le_bytes()); // subTagLength = 27
            render_attr.extend_from_slice(&bm_idx.to_le_bytes());
            render_attr.extend_from_slice(&w_cdr.to_le_bytes());
            render_attr.extend_from_slice(&w_cdr.to_le_bytes());
            render_attr.extend_from_slice(&0u16.to_le_bytes()); // xOff = 0
            render_attr.extend_from_slice(&0u16.to_le_bytes()); // yOff = 0
            render_attr.extend_from_slice(&0u16.to_le_bytes()); // interOff = 0
            render_attr.extend_from_slice(&0u16.to_le_bytes()); // flags = 0
            render_attr.extend_from_slice(&fg_idx.to_le_bytes());
            render_attr.extend_from_slice(&bg_idx.to_le_bytes());
            render_attr.extend_from_slice(&100u16.to_le_bytes()); // screen = 100
            render_attr.push(255u8); // EndTag (MonoBM)
            render_attr.push(255u8); // EndTag (FillSpec)
        } else {
            render_attr.push(1u8); // CMX_Tag_RenderAttr_FillSpec
            render_attr.extend_from_slice(&13u16.to_le_bytes()); // tagLength = 13
            render_attr.extend_from_slice(&1u16.to_le_bytes()); // fillType = 1 (Uniform)
            render_attr.push(1u8); // CMX_Tag_RenderAttr_FillSpec_Uniform
            render_attr.extend_from_slice(&7u16.to_le_bytes()); // subTagLength = 7
            render_attr.extend_from_slice(&fill_color_idx.to_le_bytes());
            render_attr.extend_from_slice(&100u16.to_le_bytes()); // screen = 100
            render_attr.push(255u8); // EndTag (Uniform)
            render_attr.push(255u8); // EndTag (FillSpec)
        }
    }
    if (mask & 0x02) != 0 {
        render_attr.push(1u8); // CMX_Tag_RenderAttr_OutlineSpec
        render_attr.extend_from_slice(&5u16.to_le_bytes()); // tagLength = 5
        render_attr.extend_from_slice(&outl_idx.to_le_bytes());
        render_attr.push(255u8); // EndTag (OutlineSpec)
    }
    if let Some(trns) = &node.transparency {
        let rate_pct = match trns {
            coralcraft_doc::TransparencyStyle::Uniform { opacity } => {
                ((1.0 - opacity.get().clamp(0.0, 1.0)) * 100.0).round().clamp(0.0, 100.0) as u16
            }
            _ => 50u16,
        };
        render_attr.push(1u8); // CMX_Tag_RenderAttr_LensSpec_Base
        render_attr.extend_from_slice(&7u16.to_le_bytes()); // tagLength = 7
        render_attr.push(1u8); // lensType = 1 (Glass/Transparency)
        render_attr.push(0u8); // tintMethod = 0 (Average/Rate)
        render_attr.extend_from_slice(&rate_pct.to_le_bytes());
        let tp = encode_trns_payload(trns);
        if tp.len().saturating_add(3) <= usize::from(u16::MAX) {
            let tp_tag_len = (3usize.saturating_add(tp.len())) as u16;
            render_attr.push(10u8);
            render_attr.extend_from_slice(&tp_tag_len.to_le_bytes());
            render_attr.extend_from_slice(&tp);
        }
        render_attr.push(255u8); // EndTag (LensSpec)
    }
    let render_tag_len = (3usize.saturating_add(render_attr.len())) as u16;

    let bbox = node_bbox_for_export(node);
    let mut bbox_payload = Vec::with_capacity(16);
    bbox_payload.extend_from_slice(&pt_to_cdr_i32(bbox.x0.get() - cx).to_le_bytes());
    bbox_payload.extend_from_slice(&pt_to_cdr_i32(cy - bbox.y0.get()).to_le_bytes());
    bbox_payload.extend_from_slice(&pt_to_cdr_i32(bbox.x1.get() - cx).to_le_bytes());
    bbox_payload.extend_from_slice(&pt_to_cdr_i32(cy - bbox.y1.get()).to_le_bytes());

    let mut body = Vec::new();
    // Tag 1: CMX_Tag_PolyCurve_RenderingAttr
    body.push(1u8);
    body.extend_from_slice(&render_tag_len.to_le_bytes());
    body.extend_from_slice(&render_attr);
    // Tag 2: CMX_Tag_PolyCurve_PointList
    body.push(2u8);
    body.extend_from_slice(&point_tag_len.to_le_bytes());
    body.extend_from_slice(&point_num.to_le_bytes());
    for (x_cdr, y_cdr) in &pts {
        body.extend_from_slice(&x_cdr.to_le_bytes());
        body.extend_from_slice(&y_cdr.to_le_bytes());
    }
    body.extend_from_slice(&types);
    // Tag 3: CMX_Tag_Polycurve_BoundingBox
    body.push(3u8);
    body.extend_from_slice(&19u16.to_le_bytes());
    body.extend_from_slice(&bbox_payload);
    if let Some(meta) = encode_cmx_meta_tag(node, cx, cy, &bbox_payload) {
        let meta_tag_len = (3usize.saturating_add(meta.len())) as u16;
        body.push(99u8);
        body.extend_from_slice(&meta_tag_len.to_le_bytes());
        body.extend_from_slice(&meta);
    }
    // EndTag
    body.push(255u8);

    if let Ok(inst_sz) = i16::try_from(4usize.saturating_add(body.len())) {
        page_cmds.extend_from_slice(&inst_sz.to_le_bytes());
        page_cmds.extend_from_slice(&67i16.to_le_bytes()); // CMX_Command_PolyCurve
        page_cmds.extend_from_slice(&body);
    } else {
        let inst_sz = (8usize.saturating_add(body.len())) as i32;
        page_cmds.extend_from_slice(&(-1i16).to_le_bytes());
        page_cmds.extend_from_slice(&inst_sz.to_le_bytes());
        page_cmds.extend_from_slice(&67i16.to_le_bytes());
        page_cmds.extend_from_slice(&body);
    }
    Ok(())
}

fn write_cmx1_document(doc: &Document) -> Result<Vec<u8>, ProbeError> {
    let default_page = default_a4_page(1);
    let pages_slice = if doc.pages.is_empty() { std::slice::from_ref(&default_page) } else { doc.pages.as_slice() };
    let first_page = pages_slice.first().unwrap_or(&default_page);
    let first_cx = first_page.size.width.get() * 0.5;
    let first_cy = first_page.size.height.get() * 0.5;
    let first_half_w = pt_to_cdr_i32(first_cx);
    let first_half_h = pt_to_cdr_i32(first_cy);
    let first_neg_half_w = first_half_w.saturating_neg();
    let first_neg_half_h = first_half_h.saturating_neg();

    let mut tables = CmxWriteTables {
        colors: vec![Color::BLACK, Color::WHITE],
        pens: vec![pt_to_cdr_i32(1.0)],
        line_styles: vec![(0x02, 0x00)],
        dots: vec![vec![1, 1]],
        outlines: vec![(1, 1, 1, 0)],
    };

    let mut page_cmd_chunks = Vec::with_capacity(pages_slice.len());
    for (p_idx, page) in pages_slice.iter().enumerate() {
        let cx = page.size.width.get() * 0.5;
        let cy = page.size.height.get() * 0.5;
        let half_w = pt_to_cdr_i32(cx);
        let half_h = pt_to_cdr_i32(cy);
        let neg_half_w = half_w.saturating_neg();
        let neg_half_h = half_h.saturating_neg();
        let page_num = (p_idx.saturating_add(1).min(usize::from(u16::MAX))) as u16;

        let mut page_cmds = Vec::new();
        // 1. CMX_Command_BeginPage (code = 9)
        {
            let mut cmd = Vec::with_capacity(30);
            cmd.extend_from_slice(&30i16.to_le_bytes()); // instructionSize
            cmd.extend_from_slice(&9i16.to_le_bytes()); // CMX_Command_BeginPage
            cmd.push(1u8); // CMX_Tag_BeginPage_PageSpecification
            cmd.extend_from_slice(&25u16.to_le_bytes()); // tagLength = 25
            cmd.extend_from_slice(&page_num.to_le_bytes());
            cmd.extend_from_slice(&0u32.to_le_bytes()); // flags = 0
            cmd.extend_from_slice(&neg_half_w.to_le_bytes());
            cmd.extend_from_slice(&half_h.to_le_bytes());
            cmd.extend_from_slice(&half_w.to_le_bytes());
            cmd.extend_from_slice(&neg_half_h.to_le_bytes());
            cmd.push(255u8); // CMX_Tag_EndTag
            page_cmds.extend_from_slice(&cmd);
        }

        for (l_idx, layer) in page.layers.iter().enumerate() {
            let layer_num = (l_idx.saturating_add(1).min(usize::from(u16::MAX))) as u16;
            let mut l_flags = 0u32;
            if layer.visible {
                l_flags |= 0x01;
            }
            if layer.printable {
                l_flags |= 0x02;
            }
            if layer.locked {
                l_flags |= 0x04;
            }
            let name_bytes = layer.name.as_bytes();
            let name_len = name_bytes.len().min(256);
            let tag_len = (17usize.saturating_add(name_len)) as u16;
            let inst_len = (22usize.saturating_add(name_len)) as i16;
            page_cmds.extend_from_slice(&inst_len.to_le_bytes());
            page_cmds.extend_from_slice(&11i16.to_le_bytes()); // CMX_Command_BeginLayer
            page_cmds.push(1u8); // CMX_Tag_BeginLayer_LayerSpecification
            page_cmds.extend_from_slice(&tag_len.to_le_bytes());
            page_cmds.extend_from_slice(&page_num.to_le_bytes());
            page_cmds.extend_from_slice(&layer_num.to_le_bytes());
            page_cmds.extend_from_slice(&l_flags.to_le_bytes());
            page_cmds.extend_from_slice(&(layer.objects.len() as u32).to_le_bytes()); // tally
            page_cmds.extend_from_slice(&(name_len as u16).to_le_bytes());
            page_cmds.extend_from_slice(name_bytes.get(..name_len).unwrap_or_default());
            page_cmds.push(255u8); // CMX_Tag_EndTag

            for node in &layer.objects {
                encode_cmx_node(node, cx, cy, &mut tables, &mut page_cmds)?;
            }

            // CMX_Command_EndLayer (code = 12)
            page_cmds.extend_from_slice(&4i16.to_le_bytes());
            page_cmds.extend_from_slice(&12i16.to_le_bytes());
        }

        // CMX_Command_EndPage (code = 10)
        page_cmds.extend_from_slice(&4i16.to_le_bytes());
        page_cmds.extend_from_slice(&10i16.to_le_bytes());
        page_cmd_chunks.push(page_cmds);
    }

    // Build `rclr` payload (CMX_COLOR_DESCRIPTION_SECTION = 21)
    let mut rclr = Vec::new();
    rclr.extend_from_slice(&(tables.colors.len() as u16).to_le_bytes());
    for col in &tables.colors {
        rclr.push(1u8); // CMX_Tag_DescrSection_Color_Base
        rclr.extend_from_slice(&5u16.to_le_bytes());
        match col {
            Color::Cmyk { c, m, y, k } => {
                rclr.push(2u8); // CMYK100
                rclr.push(0u8);
                rclr.push(2u8); // CMX_Tag_DescrSection_Color_ColorDescr
                rclr.extend_from_slice(&7u16.to_le_bytes());
                rclr.push((c.get() * 100.0).round().clamp(0.0, 100.0) as u8);
                rclr.push((m.get() * 100.0).round().clamp(0.0, 100.0) as u8);
                rclr.push((y.get() * 100.0).round().clamp(0.0, 100.0) as u8);
                rclr.push((k.get() * 100.0).round().clamp(0.0, 100.0) as u8);
                rclr.push(10u8);
                rclr.extend_from_slice(&39u16.to_le_bytes());
                rclr.extend_from_slice(b"CMYK");
                rclr.extend_from_slice(&c.get().to_le_bytes());
                rclr.extend_from_slice(&m.get().to_le_bytes());
                rclr.extend_from_slice(&y.get().to_le_bytes());
                rclr.extend_from_slice(&k.get().to_le_bytes());
            }
            Color::Hsb { h, s, b } => {
                let h_u16 = h.get().round().clamp(0.0, 360.0) as u16;
                rclr.push(6u8); // HSB
                rclr.push(0u8);
                rclr.push(2u8); // CMX_Tag_DescrSection_Color_ColorDescr
                rclr.extend_from_slice(&7u16.to_le_bytes());
                rclr.extend_from_slice(&h_u16.to_le_bytes());
                rclr.push((s.get() * 255.0).round().clamp(0.0, 255.0) as u8);
                rclr.push((b.get() * 255.0).round().clamp(0.0, 255.0) as u8);
                rclr.push(10u8);
                rclr.extend_from_slice(&31u16.to_le_bytes());
                rclr.extend_from_slice(b"HSB\0");
                rclr.extend_from_slice(&h.get().to_le_bytes());
                rclr.extend_from_slice(&s.get().to_le_bytes());
                rclr.extend_from_slice(&b.get().to_le_bytes());
            }
            Color::Gray { k } => {
                rclr.push(9u8); // Gray
                rclr.push(0u8);
                rclr.push(2u8); // CMX_Tag_DescrSection_Color_ColorDescr
                rclr.extend_from_slice(&4u16.to_le_bytes());
                rclr.push(((1.0 - k.get()) * 255.0).round().clamp(0.0, 255.0) as u8);
                rclr.push(10u8);
                rclr.extend_from_slice(&15u16.to_le_bytes());
                rclr.extend_from_slice(b"GRAY");
                rclr.extend_from_slice(&k.get().to_le_bytes());
            }
            Color::Registration => {
                rclr.push(8u8); // Registration
                rclr.push(0u8);
                rclr.push(2u8); // CMX_Tag_DescrSection_Color_ColorDescr
                rclr.extend_from_slice(&7u16.to_le_bytes());
                rclr.extend_from_slice(&[0u8; 4]);
                rclr.push(10u8);
                rclr.extend_from_slice(&7u16.to_le_bytes());
                rclr.extend_from_slice(b"REGI");
            }
            Color::Spot { name, tint, fallback_cmyk } => {
                rclr.push(2u8); // CMYK100 fallback for libcdr
                rclr.push(0u8);
                rclr.push(2u8); // CMX_Tag_DescrSection_Color_ColorDescr
                rclr.extend_from_slice(&7u16.to_le_bytes());
                rclr.push((fallback_cmyk[0].get() * 100.0).round().clamp(0.0, 100.0) as u8);
                rclr.push((fallback_cmyk[1].get() * 100.0).round().clamp(0.0, 100.0) as u8);
                rclr.push((fallback_cmyk[2].get() * 100.0).round().clamp(0.0, 100.0) as u8);
                rclr.push((fallback_cmyk[3].get() * 100.0).round().clamp(0.0, 100.0) as u8);
                let name_bytes = name.as_bytes();
                let name_len = name_bytes.len().min(256);
                let tag_len = (49usize.saturating_add(name_len)) as u16;
                rclr.push(10u8);
                rclr.extend_from_slice(&tag_len.to_le_bytes());
                rclr.extend_from_slice(b"SPOT");
                rclr.extend_from_slice(&tint.get().to_le_bytes());
                rclr.extend_from_slice(&fallback_cmyk[0].get().to_le_bytes());
                rclr.extend_from_slice(&fallback_cmyk[1].get().to_le_bytes());
                rclr.extend_from_slice(&fallback_cmyk[2].get().to_le_bytes());
                rclr.extend_from_slice(&fallback_cmyk[3].get().to_le_bytes());
                rclr.extend_from_slice(&(name_len as u16).to_le_bytes());
                rclr.extend_from_slice(name_bytes.get(..name_len).unwrap_or_default());
            }
            _ => {
                let [r, g, b, _] = col.to_rgba8(1.0);
                rclr.push(5u8); // RGB
                rclr.push(0u8);
                rclr.push(2u8); // CMX_Tag_DescrSection_Color_ColorDescr
                rclr.extend_from_slice(&7u16.to_le_bytes());
                rclr.push(r);
                rclr.push(g);
                rclr.push(b);
                rclr.push(0);
            }
        }
        rclr.push(255u8);
    }

    // Build `rott` payload (CMX_LINE_STYLE_DESCRIPTION_SECTION = 16)
    let mut rott = Vec::new();
    rott.extend_from_slice(&(tables.line_styles.len() as u16).to_le_bytes());
    for &(spec, cap_and_join) in &tables.line_styles {
        rott.push(1u8); // CMX_Tag_DescrSection_LineStyle
        rott.extend_from_slice(&5u16.to_le_bytes());
        rott.push(spec);
        rott.push(cap_and_join);
        rott.push(255u8);
    }

    // Build `rdot` payload (CMX_DOT_DASH_DESCRIPTION_SECTION = 17)
    let mut rdot = Vec::new();
    rdot.extend_from_slice(&(tables.dots.len() as u16).to_le_bytes());
    for arr in &tables.dots {
        let tag_len = (5usize.saturating_add(arr.len().saturating_mul(2))) as u16;
        rdot.push(1u8); // CMX_Tag_DescrSection_Dash
        rdot.extend_from_slice(&tag_len.to_le_bytes());
        rdot.extend_from_slice(&(arr.len() as u16).to_le_bytes());
        for &m in arr {
            rdot.extend_from_slice(&m.to_le_bytes());
        }
        rdot.push(255u8);
    }

    // Build `rpen` payload (CMX_PEN_DESCRIPTION_SECTION = 19)
    let mut rpen = Vec::new();
    rpen.extend_from_slice(&(tables.pens.len() as u16).to_le_bytes());
    for &w_cdr in &tables.pens {
        rpen.push(1u8); // CMX_Tag_DescrSection_Pen
        rpen.extend_from_slice(&15u16.to_le_bytes());
        rpen.extend_from_slice(&w_cdr.to_le_bytes());
        rpen.extend_from_slice(&100u16.to_le_bytes()); // aspect = 100%
        rpen.extend_from_slice(&0i32.to_le_bytes()); // angle = 0
        rpen.extend_from_slice(&1u16.to_le_bytes()); // matrix type = 1 (identity)
        rpen.push(255u8);
    }

    // Build `rotl` payload (CMX_OUTLINE_DESCRIPTION_SECTION = 15)
    let mut rotl = Vec::new();
    rotl.extend_from_slice(&(tables.outlines.len() as u16).to_le_bytes());
    for &(ls_idx, c_idx, p_idx, dash_idx) in &tables.outlines {
        rotl.push(1u8); // CMX_Tag_DescrSection_Outline
        rotl.extend_from_slice(&15u16.to_le_bytes());
        rotl.extend_from_slice(&ls_idx.to_le_bytes()); // lineStyle
        rotl.extend_from_slice(&100u16.to_le_bytes()); // screen = 100
        rotl.extend_from_slice(&c_idx.to_le_bytes()); // color
        rotl.extend_from_slice(&0u16.to_le_bytes()); // arrowHeads
        rotl.extend_from_slice(&p_idx.to_le_bytes()); // pen
        rotl.extend_from_slice(&dash_idx.to_le_bytes()); // dashArray
        rotl.push(255u8);
    }

    // Compute exact RIFF stream offsets:
    // 0..12: `RIFF` + size + `CMX1`
    // 12..148: `cont` chunk (8B header + 128B payload = 136B -> ends at 148)
    let mut cur_offset = 148u32;
    let mut page_offsets = Vec::with_capacity(page_cmd_chunks.len());
    for p_cmds in &page_cmd_chunks {
        page_offsets.push(cur_offset);
        let p_total = 8usize.saturating_add(p_cmds.len()).saturating_add(p_cmds.len() & 1);
        cur_offset = cur_offset.saturating_add(p_total as u32);
    }
    let rclr_offset = cur_offset;
    let rclr_chunk_total = 8usize.saturating_add(rclr.len()).saturating_add(rclr.len() & 1);
    let rott_offset = rclr_offset.saturating_add(rclr_chunk_total as u32);
    let rott_chunk_total = 8usize.saturating_add(rott.len()).saturating_add(rott.len() & 1);
    let rdot_offset = rott_offset.saturating_add(rott_chunk_total as u32);
    let rdot_chunk_total = 8usize.saturating_add(rdot.len()).saturating_add(rdot.len() & 1);
    let rpen_offset = rdot_offset.saturating_add(rdot_chunk_total as u32);
    let rpen_chunk_total = 8usize.saturating_add(rpen.len()).saturating_add(rpen.len() & 1);
    let rotl_offset = rpen_offset.saturating_add(rpen_chunk_total as u32);
    let rotl_chunk_total = 8usize.saturating_add(rotl.len()).saturating_add(rotl.len() & 1);
    let ixpg_offset = rotl_offset.saturating_add(rotl_chunk_total as u32);

    // Build `ixpg` payload (CMX_PAGE_INDEX_TABLE = 2)
    let mut ixpg = Vec::with_capacity(4usize.saturating_add(page_offsets.len().saturating_mul(16)));
    ixpg.extend_from_slice(&(page_offsets.len() as u16).to_le_bytes()); // numRecords
    ixpg.extend_from_slice(&16u16.to_le_bytes()); // sizeInFile = 16
    for &p_off in &page_offsets {
        ixpg.extend_from_slice(&p_off.to_le_bytes());
        ixpg.extend_from_slice(&u32::MAX.to_le_bytes()); // layerTableOffset
        ixpg.extend_from_slice(&u32::MAX.to_le_bytes()); // thumbnailOffset
        ixpg.extend_from_slice(&u32::MAX.to_le_bytes()); // refListOffset
    }
    let ixpg_chunk_total = 8usize.saturating_add(ixpg.len());
    let ixmr_offset = ixpg_offset.saturating_add(ixpg_chunk_total as u32);

    // Build `ixmr` payload (master index table)
    let mut ixmr = Vec::with_capacity(42);
    ixmr.extend_from_slice(&1u16.to_le_bytes()); // master ID = 1
    ixmr.extend_from_slice(&6u16.to_le_bytes()); // size = 6
    ixmr.extend_from_slice(&6u16.to_le_bytes()); // recordCount = 6
    for (sec_id, sec_off) in
        [(2u16, ixpg_offset), (15u16, rotl_offset), (16u16, rott_offset), (17u16, rdot_offset), (19u16, rpen_offset), (21u16, rclr_offset)]
    {
        ixmr.extend_from_slice(&sec_id.to_le_bytes());
        ixmr.extend_from_slice(&sec_off.to_le_bytes());
    }

    // Build `cont` 128-byte CMX header payload
    let mut cont = Vec::with_capacity(128);
    cont.extend_from_slice(b"Corel Binary Meta File\0"); // 0..23
    cont.resize(32, 0);
    cont.extend_from_slice(b"Windows"); // 32..39
    cont.resize(48, 0);
    cont.extend_from_slice(b"2   "); // 48..52: Little-endian byte order
    cont.extend_from_slice(b"4 "); // 52..54: 32-bit coordinates (`PRECISION_32BIT`)
    cont.extend_from_slice(b"2   "); // 54..58: Major version 2
    cont.extend_from_slice(b"0   "); // 58..62: Minor version 0
    cont.extend_from_slice(&2u16.to_le_bytes()); // 62..64: Base units: inch
    cont.extend_from_slice(&0.000_001f64.to_le_bytes()); // 64..72: Scale
    cont.resize(84, 0);
    cont.extend_from_slice(&ixmr_offset.to_le_bytes()); // 84..88: indexSectionOffset
    cont.extend_from_slice(&u32::MAX.to_le_bytes()); // 88..92: infoSectionOffset
    cont.extend_from_slice(&u32::MAX.to_le_bytes()); // 92..96: thumbnailOffset
    cont.extend_from_slice(&first_neg_half_w.to_le_bytes()); // 96..100
    cont.extend_from_slice(&first_half_h.to_le_bytes()); // 100..104
    cont.extend_from_slice(&first_half_w.to_le_bytes()); // 104..108
    cont.extend_from_slice(&first_neg_half_h.to_le_bytes()); // 108..112
    cont.resize(128, 0);

    let mut body = Vec::new();
    append_riff_chunk(&mut body, *b"cont", &cont);
    for p_cmds in &page_cmd_chunks {
        append_riff_chunk(&mut body, *b"page", p_cmds);
    }
    append_riff_chunk(&mut body, *b"rclr", &rclr);
    append_riff_chunk(&mut body, *b"rott", &rott);
    append_riff_chunk(&mut body, *b"rdot", &rdot);
    append_riff_chunk(&mut body, *b"rpen", &rpen);
    append_riff_chunk(&mut body, *b"rotl", &rotl);
    append_riff_chunk(&mut body, *b"ixpg", &ixpg);
    append_riff_chunk(&mut body, *b"ixmr", &ixmr);

    // Also include `LIST page` chunks so `parse_document` round-trips all parametric primitives and effects losslessly
    let mut next_style_id = 1u32;
    for page in &doc.pages {
        let mut page_children = Vec::new();
        let mut mcfg = Vec::with_capacity(8);
        mcfg.extend_from_slice(&pt_to_cdr_i32(page.size.width.get()).to_le_bytes());
        mcfg.extend_from_slice(&pt_to_cdr_i32(page.size.height.get()).to_le_bytes());
        append_riff_chunk(&mut page_children, *b"mcfg", &mcfg);
        for layer in &page.layers {
            let mut layer_children = Vec::new();
            for obj in &layer.objects {
                encode_object_into_layer(obj, page.size, &mut next_style_id, &mut layer_children)?;
            }
            append_riff_list(&mut page_children, *b"layr", &layer_children);
        }
        append_riff_list(&mut body, *b"page", &page_children);
    }

    let riff_size = 4u32.saturating_add(body.len() as u32);
    let mut out = Vec::with_capacity(8usize.saturating_add(riff_size as usize));
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&riff_size.to_le_bytes());
    out.extend_from_slice(b"CMX1");
    out.extend_from_slice(&body);
    Ok(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing, clippy::arithmetic_side_effects)]
mod budget_tests {
    use super::*;

    /// `CPng` header + zlib header + stored deflate blocks (each at most 65,535 bytes).
    fn cpng_stored(raw: &[u8]) -> Vec<u8> {
        let mut out = b"CPng\x01\x00\x04\x00".to_vec();
        out.extend_from_slice(&[0x78, 0x01]);
        let mut chunks = raw.chunks(65_535).peekable();
        if chunks.peek().is_none() {
            out.extend_from_slice(&[0x01, 0x00, 0x00, 0xFF, 0xFF]);
        }
        while let Some(c) = chunks.next() {
            out.push(u8::from(chunks.peek().is_none()));
            let len = c.len() as u16;
            out.extend_from_slice(&len.to_le_bytes());
            out.extend_from_slice(&(!len).to_le_bytes());
            out.extend_from_slice(c);
        }
        out.extend_from_slice(&crate::inflate::adler32(raw).to_be_bytes());
        out
    }

    /// Body of a `LIST cmpr` (starting with the `cmpr` list type) for `part1` / `part2` streams.
    fn cmpr_body(part1: &[u8], part2: &[u8]) -> Vec<u8> {
        let c1 = cpng_stored(part1);
        let c2 = cpng_stored(part2);
        let mut body = b"cmpr".to_vec();
        for v in [c1.len(), part1.len(), c2.len(), part2.len()] {
            body.extend_from_slice(&(v as u32).to_le_bytes());
        }
        body.extend_from_slice(&c1);
        body.extend_from_slice(&c2);
        body
    }

    fn riff_with_top_level(chunks: &[Vec<u8>]) -> Vec<u8> {
        let mut inner = Vec::new();
        for body in chunks {
            inner.extend_from_slice(b"LIST");
            inner.extend_from_slice(&(body.len() as u32).to_le_bytes());
            inner.extend_from_slice(body);
            if body.len() % 2 == 1 {
                inner.push(0);
            }
        }
        let mut riff = b"RIFF".to_vec();
        riff.extend_from_slice(&((inner.len() + 4) as u32).to_le_bytes());
        riff.extend_from_slice(b"CDRD");
        riff.extend_from_slice(&inner);
        riff
    }

    /// Part1 holding one opaque `junk` chunk whose size is block-size index 0, plus its Part2 table.
    fn junk_cmpr(payload_len: usize) -> Vec<u8> {
        let mut part1 = b"junk".to_vec();
        part1.extend_from_slice(&0u32.to_le_bytes());
        part1.extend(std::iter::repeat_n(0xAB_u8, payload_len));
        cmpr_body(&part1, &(payload_len as u32).to_le_bytes())
    }

    #[test]
    fn cmpr_decompression_budget_is_shared_across_all_lists_in_a_file() {
        let riff = riff_with_top_level(&[junk_cmpr(40_000), junk_cmpr(40_000), junk_cmpr(40_000)]);
        let mut doc = Document::new_a4_portrait();
        let mut warnings = Vec::new();
        parse_riff_stream_with_budget(&riff, Endian::Little, 1300, &mut doc, &mut warnings, 100_000);
        let budget_hits = warnings.iter().filter(|w| w.starts_with("W-CMPR-BUDGET")).count();
        assert_eq!(budget_hits, 1, "the third 40 KB list must exceed a shared 100 KB budget: {warnings:?}");
    }

    #[test]
    fn nested_cmpr_inside_cmpr_is_kept_opaque() {
        let inner = junk_cmpr(16);
        let mut part1 = b"LIST".to_vec();
        part1.extend_from_slice(&0u32.to_le_bytes());
        part1.extend_from_slice(&inner);
        let outer = cmpr_body(&part1, &(inner.len() as u32).to_le_bytes());
        let outcome = parse_document(&riff_with_top_level(&[outer])).unwrap();
        assert!(
            outcome.parse_warnings.iter().any(|w| w.starts_with("W-CMPR-NESTED")),
            "nested cmpr must not be decompressed: {:?}",
            outcome.parse_warnings
        );
    }

    #[test]
    fn hostile_bitmap_headers_are_rejected_without_huge_allocations() {
        // `BMP ` header claiming u32::MAX x u32::MAX pixels with a 16-byte body.
        let mut bmp = b"BMP ".to_vec();
        for v in [u32::MAX, u32::MAX, 0, 0, 16] {
            bmp.extend_from_slice(&v.to_le_bytes());
        }
        bmp.extend_from_slice(&[0u8; 16]);
        assert!(decode_loda_bitmap(&bmp, Endian::Little, 1400, Size::default(), None).is_none());

        // 2x2 bitmap whose pixel data is short: must be rejected, not padded.
        let mut short = b"BMP ".to_vec();
        for v in [2u32, 2, 0, 0, 8] {
            short.extend_from_slice(&v.to_le_bytes());
        }
        short.extend_from_slice(&[0u8; 8]);
        assert!(decode_loda_bitmap(&short, Endian::Little, 1400, Size::default(), None).is_none());

        // No `BMP ` magic: no invented placeholder pixels, even with a bounding box.
        let bbox = Rect::new(0.0, 0.0, 4000.0, 4000.0).ok();
        assert!(decode_loda_bitmap(&[0u8; 32], Endian::Little, 1400, Size::default(), bbox).is_none());

        // BITMAPINFOHEADER 16384 x 16384 x 24 bpp with no pixel data: rejected before allocating.
        let mut dib = Vec::new();
        dib.extend_from_slice(&40u32.to_le_bytes());
        dib.extend_from_slice(&16_384i32.to_le_bytes());
        dib.extend_from_slice(&16_384i32.to_le_bytes());
        dib.extend_from_slice(&1u16.to_le_bytes());
        dib.extend_from_slice(&24u16.to_le_bytes());
        dib.resize(40, 0);
        assert!(decode_bmp_or_cdib_chunk(&dib, Endian::Little).is_none());
    }
}
