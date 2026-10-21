//! Clean-room Encapsulated PostScript (`EPSF-3.0`, PLRM 3rd Edition) and ISO 32000-1 PDF 1.4
//! vector importers (`import_eps`, `import_pdf`) for `coralcraft-doc`.

use std::collections::BTreeMap;

use coralcraft_color::Color;
use coralcraft_geom::{Affine2, F64, FillRule, Path, Point, Rect, RectSpec, Size};

use crate::interchange::{DxfDocMeta, DxfLayerMeta, DxfPageMeta, decode_interchange_meta};
use crate::model::{DocError, Document, Layer, LayerId, LayerRole, ObjectId, ObjectKind, ObjectNode, Page, PageId};
use crate::style::{FillStyle, LineCap, LineJoin, OutlineStyle, Residual};

/// Maximum input byte length (`64 MiB`) accepted by `import_eps` and `import_pdf`.
const MAX_IMPORT_BYTES: usize = 64 * 1024 * 1024;
/// Maximum graphics state stack depth (`gsave`/`grestore` or `q`/`Q`).
const MAX_STATE_DEPTH: usize = 64;
/// Maximum operand stack size in the PostScript/PDF state machine.
const MAX_OPERAND_STACK: usize = 256;
/// Maximum number of vector objects/elements imported from a single file.
const MAX_IMPORTED_OBJECTS: usize = 100_000;

#[allow(clippy::arithmetic_side_effects)]
fn f64_sub(a: f64, b: f64) -> f64 {
    a - b
}

#[allow(clippy::arithmetic_side_effects)]
fn f64_add(a: f64, b: f64) -> f64 {
    a + b
}

#[allow(clippy::arithmetic_side_effects)]
fn f64_mul(a: f64, b: f64) -> f64 {
    a * b
}

#[allow(clippy::arithmetic_side_effects)]
fn f64_neg(a: f64) -> f64 {
    -a
}

fn remap_imported_node_ids(doc: &mut Document, node: &mut ObjectNode) {
    node.id = doc.allocate_object_id();
    if let ObjectKind::Group { children } = &mut node.kind {
        for ch in children {
            remap_imported_node_ids(doc, ch);
        }
    }
    if let ObjectKind::Unsupported(u) = &mut node.kind {
        for ch in &mut u.children {
            remap_imported_node_ids(doc, ch);
        }
    }
    for pc in &mut node.powerclip_children {
        remap_imported_node_ids(doc, pc);
    }
}

fn unescape_ps_pdf_string(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some('(') => out.push('('),
                Some(')') => out.push(')'),
                Some('\\') => out.push('\\'),
                Some('\n') | Some('\r') => {}
                Some(d0 @ '0'..='7') => {
                    let mut val = u32::from(d0 as u8).saturating_sub(u32::from(b'0'));
                    for _ in 0..2 {
                        if let Some(&dnext) = chars.peek()
                            && matches!(dnext, '0'..='7')
                        {
                            let _ = chars.next();
                            val = val.saturating_mul(8).saturating_add(u32::from(dnext as u8).saturating_sub(u32::from(b'0')));
                        } else {
                            break;
                        }
                    }
                    if let Some(c) = char::from_u32(val & 0xFF) {
                        out.push(c);
                    }
                }
                Some(other) => out.push(other),
                None => break,
            }
        } else {
            out.push(ch);
        }
    }
    out
}

fn decode_pdf_hex_string(hex_inner: &str) -> String {
    let mut bytes_out = Vec::with_capacity(hex_inner.len() / 2);
    let mut high: Option<u8> = None;
    for &b in hex_inner.as_bytes() {
        if let Some(nib) = hex_nibble(b) {
            if let Some(h) = high.take() {
                bytes_out.push((h << 4) | nib);
            } else {
                high = Some(nib);
            }
        }
    }
    if let Some(h) = high {
        bytes_out.push(h << 4);
    }
    String::from_utf8_lossy(&bytes_out).into_owned()
}

fn parse_four_floats(s: &str) -> Option<(f64, f64, f64, f64)> {
    let mut it = s.split_whitespace().filter_map(|tok| tok.parse::<f64>().ok().filter(|v| v.is_finite()));
    let a = it.next()?;
    let b = it.next()?;
    let c = it.next()?;
    let d = it.next()?;
    Some((a, b, c, d))
}

fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b.saturating_sub(b'0')),
        b'a'..=b'f' => Some(b.saturating_sub(b'a').saturating_add(10)),
        b'A'..=b'F' => Some(b.saturating_sub(b'A').saturating_add(10)),
        _ => None,
    }
}

fn decode_ascii_hex_rgb_to_rgba(hex_str: &str, pw: u32, ph: u32) -> Option<Vec<u8>> {
    let total_pixels = usize::try_from(pw).ok()?.checked_mul(usize::try_from(ph).ok()?)?;
    if total_pixels == 0 || total_pixels > 4_000_000 {
        return None;
    }
    let mut rgb_bytes = Vec::with_capacity(total_pixels.saturating_mul(3));
    let mut high: Option<u8> = None;
    for &b in hex_str.as_bytes() {
        if b == b'>' {
            break;
        }
        if let Some(nib) = hex_nibble(b) {
            if let Some(h) = high.take() {
                rgb_bytes.push((h << 4) | nib);
                if rgb_bytes.len() >= total_pixels.saturating_mul(3) {
                    break;
                }
            } else {
                high = Some(nib);
            }
        }
    }
    if let Some(h) = high {
        rgb_bytes.push(h << 4);
    }
    let mut rgba = Vec::with_capacity(total_pixels.saturating_mul(4));
    for px in 0..total_pixels {
        let base = px.saturating_mul(3);
        let r = rgb_bytes.get(base).copied().unwrap_or(0);
        let g = rgb_bytes.get(base.saturating_add(1)).copied().unwrap_or(0);
        let b = rgb_bytes.get(base.saturating_add(2)).copied().unwrap_or(0);
        rgba.extend_from_slice(&[r, g, b, 255]);
    }
    Some(rgba)
}

#[derive(Clone, Debug)]
enum PsToken {
    Num(f64),
    Name(String),
    Str(String),
    Array(Vec<PsToken>),
    Op(String),
}

fn tokenize_ps_pdf_line(line: &str) -> Vec<PsToken> {
    let mut tokens = Vec::new();
    let bytes = line.as_bytes();
    let mut i = 0usize;
    while let Some(&b) = bytes.get(i) {
        if b.is_ascii_whitespace() {
            i = i.saturating_add(1);
            continue;
        }
        if b == b'%' {
            break;
        }
        if b == b'(' {
            i = i.saturating_add(1);
            let mut depth = 1usize;
            let start = i;
            while let Some(&c) = bytes.get(i) {
                if c == b'\\' {
                    i = i.saturating_add(2);
                    continue;
                }
                if c == b'(' {
                    depth = depth.saturating_add(1);
                } else if c == b')' {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        break;
                    }
                }
                i = i.saturating_add(1);
            }
            if let Some(slice) = line.get(start..i) {
                tokens.push(PsToken::Str(unescape_ps_pdf_string(slice)));
            }
            if bytes.get(i) == Some(&b')') {
                i = i.saturating_add(1);
            }
            continue;
        }
        if b == b'[' {
            i = i.saturating_add(1);
            let start = i;
            let mut depth = 1usize;
            while let Some(&c) = bytes.get(i) {
                if c == b'[' {
                    depth = depth.saturating_add(1);
                } else if c == b']' {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        break;
                    }
                }
                i = i.saturating_add(1);
            }
            if let Some(inner) = line.get(start..i) {
                tokens.push(PsToken::Array(tokenize_ps_pdf_line(inner)));
            }
            if bytes.get(i) == Some(&b']') {
                i = i.saturating_add(1);
            }
            continue;
        }
        if b == b'<' {
            if bytes.get(i.saturating_add(1)) == Some(&b'<') {
                i = i.saturating_add(2);
                continue;
            }
            i = i.saturating_add(1);
            let start = i;
            while let Some(&c) = bytes.get(i) {
                if c == b'>' {
                    break;
                }
                i = i.saturating_add(1);
            }
            if let Some(inner) = line.get(start..i) {
                tokens.push(PsToken::Str(decode_pdf_hex_string(inner)));
            }
            if bytes.get(i) == Some(&b'>') {
                i = i.saturating_add(1);
            }
            continue;
        }
        if matches!(b, b')' | b']' | b'>') {
            i = i.saturating_add(1);
            continue;
        }
        if b == b'/' {
            i = i.saturating_add(1);
            let start = i;
            while let Some(&c) = bytes.get(i) {
                if c.is_ascii_whitespace() || matches!(c, b'/' | b'(' | b')' | b'[' | b']' | b'<' | b'>' | b'%') {
                    break;
                }
                i = i.saturating_add(1);
            }
            if let Some(name) = line.get(start..i) {
                tokens.push(PsToken::Name(name.to_owned()));
            }
            continue;
        }
        let start = i;
        while let Some(&c) = bytes.get(i) {
            if c.is_ascii_whitespace() || matches!(c, b'/' | b'(' | b')' | b'[' | b']' | b'<' | b'>' | b'%') {
                break;
            }
            i = i.saturating_add(1);
        }
        if let Some(word) = line.get(start..i) {
            if let Ok(num) = word.parse::<f64>()
                && num.is_finite()
            {
                tokens.push(PsToken::Num(num));
            } else {
                tokens.push(PsToken::Op(word.to_owned()));
            }
        }
    }
    tokens
}

#[derive(Clone, Debug)]
struct PsGraphicsState {
    ctm: Affine2,
    color: Color,
    stroke_color: Color,
    line_width: f64,
    cap: LineCap,
    join: LineJoin,
    miter_limit: f64,
    dash_pattern: Vec<F64>,
    font_family: String,
    font_size: f64,
}

impl Default for PsGraphicsState {
    fn default() -> Self {
        Self {
            ctm: Affine2::IDENTITY,
            color: Color::BLACK,
            stroke_color: Color::BLACK,
            line_width: 1.0,
            cap: LineCap::Butt,
            join: LineJoin::Miter,
            miter_limit: 4.0,
            dash_pattern: Vec::new(),
            font_family: "Helvetica".to_owned(),
            font_size: 12.0,
        }
    }
}

fn pop_num(stack: &mut Vec<PsToken>) -> Option<f64> {
    match stack.pop()? {
        PsToken::Num(n) => Some(n),
        _ => None,
    }
}

fn push_bounded(stack: &mut Vec<PsToken>, tok: PsToken) {
    if stack.len() >= MAX_OPERAND_STACK {
        stack.remove(0);
    }
    stack.push(tok);
}

fn push_node_to_target(
    doc: &mut Document,
    page_index: usize,
    saw_custom_layer: &mut bool,
    target: &Option<(LayerRole, String, bool)>,
    node: ObjectNode,
) {
    if let Some((role, name, is_master)) = target {
        if *is_master {
            if let Some(layer) = doc.master_layers.iter_mut().find(|l| l.name == *name && l.role == *role) {
                layer.objects.push(node);
                return;
            }
        } else if let Some(page) = doc.pages.get_mut(page_index)
            && let Some(layer) = page.layers.iter_mut().find(|l| l.name == *name)
        {
            layer.objects.push(node);
            return;
        }
    }
    let needs_default_layer = doc.pages.get(page_index).is_some_and(|p| p.layers.is_empty());
    if needs_default_layer {
        let lid = LayerId(doc.allocate_object_id().0);
        if let Some(page) = doc.pages.get_mut(page_index) {
            page.layers.push(Layer::new_normal(lid, "Layer 1"));
            *saw_custom_layer = true;
        }
    }
    if let Some(page) = doc.pages.get_mut(page_index)
        && let Some(layer) = page.layers.last_mut()
    {
        layer.objects.push(node);
    }
}

fn apply_layer_meta(doc: &mut Document, page_index: usize, saw_custom_layer: &mut bool, lmeta: DxfLayerMeta) -> (LayerRole, String, bool) {
    let is_master = lmeta.is_master || lmeta.role != LayerRole::Normal;
    if is_master {
        if let Some(existing) = doc.master_layers.iter_mut().find(|l| l.name == lmeta.name && l.role == lmeta.role) {
            existing.visible = lmeta.visible;
            existing.printable = lmeta.printable;
            existing.locked = lmeta.locked;
            existing.master_in_front = lmeta.master_in_front;
            existing.opacity = lmeta.opacity;
            existing.residual = lmeta.residual;
        } else {
            let lid = LayerId(doc.allocate_object_id().0);
            let mut layer = Layer::new_normal(lid, lmeta.name.clone());
            layer.role = lmeta.role;
            layer.visible = lmeta.visible;
            layer.printable = lmeta.printable;
            layer.locked = lmeta.locked;
            layer.master_in_front = lmeta.master_in_front;
            layer.opacity = lmeta.opacity;
            layer.residual = lmeta.residual;
            doc.master_layers.push(layer);
        }
        (lmeta.role, lmeta.name, true)
    } else {
        if !*saw_custom_layer
            && let Some(page) = doc.pages.get_mut(page_index)
            && let Some(first) = page.layers.first_mut()
        {
            *saw_custom_layer = true;
            first.name.clone_from(&lmeta.name);
            first.role = lmeta.role;
            first.visible = lmeta.visible;
            first.printable = lmeta.printable;
            first.locked = lmeta.locked;
            first.master_in_front = lmeta.master_in_front;
            first.opacity = lmeta.opacity;
            first.residual = lmeta.residual;
            return (lmeta.role, lmeta.name, false);
        }
        if let Some(page) = doc.pages.get_mut(page_index)
            && let Some(existing) = page.layers.iter_mut().find(|l| l.name == lmeta.name)
        {
            existing.visible = lmeta.visible;
            existing.printable = lmeta.printable;
            existing.locked = lmeta.locked;
            existing.master_in_front = lmeta.master_in_front;
            existing.opacity = lmeta.opacity;
            existing.residual = lmeta.residual;
            return (lmeta.role, lmeta.name, false);
        }
        let lid = LayerId(doc.allocate_object_id().0);
        if let Some(page) = doc.pages.get_mut(page_index) {
            let mut layer = Layer::new_normal(lid, lmeta.name.clone());
            layer.role = lmeta.role;
            layer.visible = lmeta.visible;
            layer.printable = lmeta.printable;
            layer.locked = lmeta.locked;
            layer.master_in_front = lmeta.master_in_front;
            layer.opacity = lmeta.opacity;
            layer.residual = lmeta.residual;
            page.layers.push(layer);
        }
        (lmeta.role, lmeta.name, false)
    }
}

/// Imports an Encapsulated PostScript (`EPSF-3.0`) string into a [`Document`].
pub fn import_eps(eps_text: &str) -> Result<Document, DocError> {
    if eps_text.len() > MAX_IMPORT_BYTES {
        return Err(DocError::InvalidDocument(format!("EPS input size {} exceeds maximum {}", eps_text.len(), MAX_IMPORT_BYTES)));
    }
    let trimmed = eps_text.trim_start();
    if !trimmed.starts_with("%!PS") {
        return Err(DocError::BadMagic);
    }

    let mut title: Option<String> = None;
    let mut int_bbox: Option<(f64, f64, f64, f64)> = None;
    let mut hires_bbox: Option<(f64, f64, f64, f64)> = None;
    let mut doc_meta_b64 = String::new();
    let mut page_meta_b64 = String::new();
    let mut active_header_cont: u8 = 0; // 0 = none, 1 = doc, 2 = page

    for raw_line in eps_text.lines() {
        let line = raw_line.trim();
        if let Some(rest) = line.strip_prefix("%%Title:") {
            active_header_cont = 0;
            let t = rest.trim();
            let stripped = t.strip_prefix('(').and_then(|s| s.strip_suffix(')')).unwrap_or(t);
            title = Some(unescape_ps_pdf_string(stripped));
        } else if let Some(rest) = line.strip_prefix("%%HiResBoundingBox:") {
            active_header_cont = 0;
            if let Some(bb) = parse_four_floats(rest) {
                hires_bbox = Some(bb);
            }
        } else if let Some(rest) = line.strip_prefix("%%BoundingBox:") {
            active_header_cont = 0;
            if let Some(bb) = parse_four_floats(rest) {
                int_bbox = Some(bb);
            }
        } else if let Some(rest) = line.strip_prefix("%%CoralCraftDoc:") {
            doc_meta_b64.push_str(rest.trim());
            active_header_cont = 1;
        } else if let Some(rest) = line.strip_prefix("%%CoralCraftPage:") {
            page_meta_b64.push_str(rest.trim());
            active_header_cont = 2;
        } else if let Some(rest) = line.strip_prefix("%%+") {
            if active_header_cont == 1 {
                doc_meta_b64.push_str(rest.trim());
            } else if active_header_cont == 2 {
                page_meta_b64.push_str(rest.trim());
            }
        } else {
            active_header_cont = 0;
        }
    }

    let mut doc = Document::new_a4_portrait();
    let (llx, lly, urx, ury) = hires_bbox.or(int_bbox).unwrap_or((0.0, 0.0, 595.275_590_551_181_2, 841.889_763_779_527_6));
    let page_w = f64_sub(urx, llx).clamp(1.0, 1_000_000.0);
    let page_h = f64_sub(ury, lly).clamp(1.0, 1_000_000.0);
    if let Some(page) = doc.pages.first_mut() {
        if let Ok(sz) = Size::new(page_w, page_h) {
            page.size = sz;
        }
        if let Some(t) = title {
            page.name = t;
        }
    }

    if !doc_meta_b64.is_empty()
        && let Some(dmeta) = decode_interchange_meta::<DxfDocMeta>(&doc_meta_b64)
    {
        doc.title = dmeta.title;
        doc.units = dmeta.units;
        doc.palette = dmeta.palette;
        doc.residual = dmeta.residual;
    }
    if !page_meta_b64.is_empty()
        && let Some(pmeta) = decode_interchange_meta::<DxfPageMeta>(&page_meta_b64)
        && let Some(page) = doc.pages.first_mut()
    {
        page.name = pmeta.name;
        page.bleed = pmeta.bleed;
        page.background = pmeta.background;
        page.residual = pmeta.residual;
    }

    let mut saw_custom_layer = false;
    let mut current_target: Option<(LayerRole, String, bool)> = None;
    let mut pending_layer_b64 = String::new();
    let mut pending_node_b64 = String::new();
    let mut skip_until_end_node = false;
    let mut reading_hex_bitmap: Option<(u32, u32, Affine2, String)> = None;

    let mut state = PsGraphicsState::default();
    let mut state_stack: Vec<PsGraphicsState> = Vec::new();
    let mut op_stack: Vec<PsToken> = Vec::new();
    let mut cur_path = Path::new();
    let mut cur_pt = Point::ZERO;
    let mut pending_image_dims: Option<(u32, u32)> = None;
    let mut imported_count = 0usize;

    let map_ps_pt = |p: Point, ctm: Affine2| -> Result<Point, DocError> {
        let wp = ctm.apply_point(p).map_err(|e| DocError::InvalidDocument(e.to_string()))?;
        Point::new(f64_sub(wp.x.get(), llx), f64_sub(ury, wp.y.get())).map_err(|e| DocError::InvalidDocument(e.to_string()))
    };

    for raw_line in eps_text.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(rest) = line.strip_prefix("% CoralCraftLayer:") {
            if !pending_layer_b64.is_empty()
                && let Some(lmeta) = decode_interchange_meta::<DxfLayerMeta>(&pending_layer_b64)
            {
                current_target = Some(apply_layer_meta(&mut doc, 0, &mut saw_custom_layer, lmeta));
            }
            pending_layer_b64.clear();
            pending_layer_b64.push_str(rest.trim());
            continue;
        }
        if let Some(rest) = line.strip_prefix("% CoralCraftLayer+:") {
            pending_layer_b64.push_str(rest.trim());
            continue;
        }
        if !pending_layer_b64.is_empty() {
            if let Some(lmeta) = decode_interchange_meta::<DxfLayerMeta>(&pending_layer_b64) {
                current_target = Some(apply_layer_meta(&mut doc, 0, &mut saw_custom_layer, lmeta));
            }
            pending_layer_b64.clear();
        }

        if let Some(rest) = line.strip_prefix("% CoralCraftNode:") {
            pending_node_b64.clear();
            pending_node_b64.push_str(rest.trim());
            continue;
        }
        if let Some(rest) = line.strip_prefix("% CoralCraftNode+:") {
            pending_node_b64.push_str(rest.trim());
            continue;
        }
        if !pending_node_b64.is_empty() {
            if let Some(mut node) = decode_interchange_meta::<ObjectNode>(&pending_node_b64) {
                imported_count = imported_count.saturating_add(1);
                if imported_count <= MAX_IMPORTED_OBJECTS {
                    remap_imported_node_ids(&mut doc, &mut node);
                    push_node_to_target(&mut doc, 0, &mut saw_custom_layer, &current_target, node);
                }
                skip_until_end_node = true;
            }
            pending_node_b64.clear();
        }

        if line == "% CoralCraftEndNode" {
            skip_until_end_node = false;
            continue;
        }
        if skip_until_end_node {
            continue;
        }

        if let Some((pw, ph, img_ctm, ref mut hex_buf)) = reading_hex_bitmap {
            hex_buf.push_str(line);
            if line.contains('>') {
                if let Some(rgba) = decode_ascii_hex_rgb_to_rgba(hex_buf, pw, ph) {
                    let id = doc.allocate_object_id();
                    let w_pt = img_ctm.a.get().abs().max(1.0);
                    let h_pt = img_ctm.d.get().abs().max(1.0);
                    let dpi_x = F64::new(f64_mul(f64::from(pw), 72.0) / w_pt).unwrap_or(F64::new(72.0).unwrap_or(F64::ONE));
                    let dpi_y = F64::new(f64_mul(f64::from(ph), 72.0) / h_pt).unwrap_or(F64::new(72.0).unwrap_or(F64::ONE));
                    let mut node = ObjectNode::new(
                        id,
                        Some("EPS Bitmap".to_owned()),
                        ObjectKind::Bitmap { pixel_width: pw, pixel_height: ph, dpi_x, dpi_y, rgba_pixels: rgba },
                        None,
                        None,
                    );
                    let tx_doc = f64_sub(img_ctm.tx.get(), llx);
                    let ty_doc = f64_sub(ury, f64_add(img_ctm.ty.get(), h_pt));
                    if let Ok(tf) = Affine2::translation(tx_doc, ty_doc) {
                        node.transform = tf;
                    }
                    push_node_to_target(&mut doc, 0, &mut saw_custom_layer, &current_target, node);
                }
                reading_hex_bitmap = None;
            }
            continue;
        }

        if let Some(rest) = line.strip_prefix("% Layer:") {
            let lname = rest.trim().to_owned();
            if !lname.is_empty() && !current_target.as_ref().is_some_and(|(_, n, _)| n == &lname) {
                let lmeta = DxfLayerMeta {
                    name: lname,
                    role: LayerRole::Normal,
                    visible: true,
                    printable: true,
                    locked: false,
                    master_in_front: false,
                    opacity: F64::ONE,
                    residual: Residual::default(),
                    is_master: false,
                };
                current_target = Some(apply_layer_meta(&mut doc, 0, &mut saw_custom_layer, lmeta));
            }
            continue;
        }
        if line.starts_with('%') {
            continue;
        }

        for tok in tokenize_ps_pdf_line(line) {
            match tok {
                PsToken::Num(_) | PsToken::Name(_) | PsToken::Str(_) => {
                    push_bounded(&mut op_stack, tok);
                }
                PsToken::Array(arr) => {
                    if op_stack.len() >= 3
                        && let (Some(PsToken::Num(bpc)), Some(PsToken::Num(ph)), Some(PsToken::Num(pw))) = (
                            op_stack.get(op_stack.len().saturating_sub(1)),
                            op_stack.get(op_stack.len().saturating_sub(2)),
                            op_stack.get(op_stack.len().saturating_sub(3)),
                        )
                        && (*bpc as u32) == 8
                        && *pw >= 1.0
                        && *ph >= 1.0
                    {
                        pending_image_dims = Some((*pw as u32, *ph as u32));
                    }
                    push_bounded(&mut op_stack, PsToken::Array(arr));
                }
                PsToken::Op(op) => match op.as_str() {
                    "gsave" => {
                        if state_stack.len() < MAX_STATE_DEPTH {
                            state_stack.push(state.clone());
                        }
                    }
                    "grestore" => {
                        if let Some(prev) = state_stack.pop() {
                            state = prev;
                        }
                    }
                    "newpath" => {
                        cur_path = Path::new();
                    }
                    "closepath" => {
                        let _ = cur_path.close();
                    }
                    "moveto" => {
                        if let (Some(y), Some(x)) = (pop_num(&mut op_stack), pop_num(&mut op_stack))
                            && let Ok(p) = Point::new(x, y)
                            && let Ok(dp) = map_ps_pt(p, state.ctm)
                        {
                            let _ = cur_path.move_to(dp);
                            cur_pt = p;
                        }
                    }
                    "rmoveto" => {
                        if let (Some(dy), Some(dx)) = (pop_num(&mut op_stack), pop_num(&mut op_stack))
                            && let Ok(p) = Point::new(f64_add(cur_pt.x.get(), dx), f64_add(cur_pt.y.get(), dy))
                            && let Ok(dp) = map_ps_pt(p, state.ctm)
                        {
                            let _ = cur_path.move_to(dp);
                            cur_pt = p;
                        }
                    }
                    "lineto" => {
                        if let (Some(y), Some(x)) = (pop_num(&mut op_stack), pop_num(&mut op_stack))
                            && let Ok(p) = Point::new(x, y)
                            && let Ok(dp) = map_ps_pt(p, state.ctm)
                        {
                            let _ = cur_path.line_to(dp);
                            cur_pt = p;
                        }
                    }
                    "rlineto" => {
                        if let (Some(dy), Some(dx)) = (pop_num(&mut op_stack), pop_num(&mut op_stack))
                            && let Ok(p) = Point::new(f64_add(cur_pt.x.get(), dx), f64_add(cur_pt.y.get(), dy))
                            && let Ok(dp) = map_ps_pt(p, state.ctm)
                        {
                            let _ = cur_path.line_to(dp);
                            cur_pt = p;
                        }
                    }
                    "curveto" => {
                        if let (Some(y3), Some(x3), Some(y2), Some(x2), Some(y1), Some(x1)) = (
                            pop_num(&mut op_stack),
                            pop_num(&mut op_stack),
                            pop_num(&mut op_stack),
                            pop_num(&mut op_stack),
                            pop_num(&mut op_stack),
                            pop_num(&mut op_stack),
                        ) && let (Ok(p1), Ok(p2), Ok(p3)) = (Point::new(x1, y1), Point::new(x2, y2), Point::new(x3, y3))
                            && let (Ok(dp1), Ok(dp2), Ok(dp3)) = (map_ps_pt(p1, state.ctm), map_ps_pt(p2, state.ctm), map_ps_pt(p3, state.ctm))
                        {
                            let _ = cur_path.cubic_to(dp1, dp2, dp3);
                            cur_pt = p3;
                        }
                    }
                    "rcurveto" => {
                        if let (Some(dy3), Some(dx3), Some(dy2), Some(dx2), Some(dy1), Some(dx1)) = (
                            pop_num(&mut op_stack),
                            pop_num(&mut op_stack),
                            pop_num(&mut op_stack),
                            pop_num(&mut op_stack),
                            pop_num(&mut op_stack),
                            pop_num(&mut op_stack),
                        ) && let (Ok(p1), Ok(p2), Ok(p3)) = (
                            Point::new(f64_add(cur_pt.x.get(), dx1), f64_add(cur_pt.y.get(), dy1)),
                            Point::new(f64_add(cur_pt.x.get(), dx2), f64_add(cur_pt.y.get(), dy2)),
                            Point::new(f64_add(cur_pt.x.get(), dx3), f64_add(cur_pt.y.get(), dy3)),
                        ) && let (Ok(dp1), Ok(dp2), Ok(dp3)) = (map_ps_pt(p1, state.ctm), map_ps_pt(p2, state.ctm), map_ps_pt(p3, state.ctm))
                        {
                            let _ = cur_path.cubic_to(dp1, dp2, dp3);
                            cur_pt = p3;
                        }
                    }
                    "translate" => {
                        if let (Some(ty), Some(tx)) = (pop_num(&mut op_stack), pop_num(&mut op_stack))
                            && let Ok(tr) = Affine2::translation(tx, ty)
                            && let Ok(next) = tr.then(state.ctm)
                        {
                            state.ctm = next;
                        }
                    }
                    "scale" => {
                        if let (Some(sy), Some(sx)) = (pop_num(&mut op_stack), pop_num(&mut op_stack))
                            && let Ok(sc) = Affine2::scaling(sx, sy)
                            && let Ok(next) = sc.then(state.ctm)
                        {
                            state.ctm = next;
                        }
                    }
                    "rotate" => {
                        if let Some(deg) = pop_num(&mut op_stack)
                            && let Ok(rot) = Affine2::rotation_deg(deg)
                            && let Ok(next) = rot.then(state.ctm)
                        {
                            state.ctm = next;
                        }
                    }
                    "concat" => {
                        if let Some(PsToken::Array(arr)) = op_stack.pop()
                            && arr.len() >= 6
                            && let (
                                Some(PsToken::Num(a)),
                                Some(PsToken::Num(b)),
                                Some(PsToken::Num(c)),
                                Some(PsToken::Num(d)),
                                Some(PsToken::Num(tx)),
                                Some(PsToken::Num(ty)),
                            ) = (arr.first(), arr.get(1), arr.get(2), arr.get(3), arr.get(4), arr.get(5))
                            && let Ok(m) = Affine2::new(*a, *b, *c, *d, *tx, *ty)
                            && let Ok(next) = m.then(state.ctm)
                        {
                            state.ctm = next;
                        }
                    }
                    "setrgbcolor" => {
                        if let (Some(b), Some(g), Some(r)) = (pop_num(&mut op_stack), pop_num(&mut op_stack), pop_num(&mut op_stack)) {
                            let col = Color::rgb8(
                                f64_mul(r.clamp(0.0, 1.0), 255.0).round() as u8,
                                f64_mul(g.clamp(0.0, 1.0), 255.0).round() as u8,
                                f64_mul(b.clamp(0.0, 1.0), 255.0).round() as u8,
                            );
                            state.color = col.clone();
                            state.stroke_color = col;
                        }
                    }
                    "setcmykcolor" => {
                        if let (Some(k), Some(y), Some(m), Some(c)) =
                            (pop_num(&mut op_stack), pop_num(&mut op_stack), pop_num(&mut op_stack), pop_num(&mut op_stack))
                            && let Ok(col) = Color::cmyk(c.clamp(0.0, 1.0), m.clamp(0.0, 1.0), y.clamp(0.0, 1.0), k.clamp(0.0, 1.0))
                        {
                            state.color = col.clone();
                            state.stroke_color = col;
                        }
                    }
                    "setgray" => {
                        if let Some(g) = pop_num(&mut op_stack)
                            && let Ok(col) = Color::gray(g.clamp(0.0, 1.0))
                        {
                            state.color = col.clone();
                            state.stroke_color = col;
                        }
                    }
                    "setlinewidth" => {
                        if let Some(w) = pop_num(&mut op_stack) {
                            state.line_width = w.max(0.0);
                        }
                    }
                    "setlinecap" => {
                        if let Some(c) = pop_num(&mut op_stack) {
                            state.cap = match c.round() as i32 {
                                1 => LineCap::Round,
                                2 => LineCap::Square,
                                _ => LineCap::Butt,
                            };
                        }
                    }
                    "setlinejoin" => {
                        if let Some(j) = pop_num(&mut op_stack) {
                            state.join = match j.round() as i32 {
                                1 => LineJoin::Round,
                                2 => LineJoin::Bevel,
                                _ => LineJoin::Miter,
                            };
                        }
                    }
                    "setmiterlimit" => {
                        if let Some(m) = pop_num(&mut op_stack) {
                            state.miter_limit = m.max(1.0);
                        }
                    }
                    "setdash" => {
                        let _phase = pop_num(&mut op_stack);
                        if let Some(PsToken::Array(arr)) = op_stack.pop() {
                            state.dash_pattern = arr
                                .into_iter()
                                .filter_map(|t| match t {
                                    PsToken::Num(v) if v > 0.0 => F64::new(v).ok(),
                                    _ => None,
                                })
                                .collect();
                        }
                    }
                    "findfont" => {
                        if let Some(PsToken::Name(font)) = op_stack.pop() {
                            state.font_family = font;
                        }
                    }
                    "scalefont" => {
                        if let Some(sz) = pop_num(&mut op_stack) {
                            state.font_size = sz.max(1.0);
                        }
                    }
                    "setfont" => {}
                    "show" => {
                        if let Some(PsToken::Str(text)) = op_stack.pop()
                            && !text.is_empty()
                            && let Ok(pos) = map_ps_pt(cur_pt, state.ctm)
                            && let Ok(size_pt) = F64::new(state.font_size)
                        {
                            imported_count = imported_count.saturating_add(1);
                            if imported_count <= MAX_IMPORTED_OBJECTS {
                                let id = doc.allocate_object_id();
                                let node = ObjectNode::new(
                                    id,
                                    Some("EPS Text".to_owned()),
                                    ObjectKind::ArtisticText { text, font_family: state.font_family.clone(), size_pt, position: pos },
                                    Some(FillStyle::uniform(state.color.clone())),
                                    None,
                                );
                                push_node_to_target(&mut doc, 0, &mut saw_custom_layer, &current_target, node);
                            }
                        }
                    }
                    "rectfill" | "rectstroke" => {
                        if let (Some(rh), Some(rw), Some(ry), Some(rx)) =
                            (pop_num(&mut op_stack), pop_num(&mut op_stack), pop_num(&mut op_stack), pop_num(&mut op_stack))
                            && let (Ok(p0), Ok(p1)) = (Point::new(rx, ry), Point::new(f64_add(rx, rw), f64_add(ry, rh)))
                            && let (Ok(dp0), Ok(dp1)) = (map_ps_pt(p0, state.ctm), map_ps_pt(p1, state.ctm))
                            && let Ok(rect) = Rect::new(
                                dp0.x.get().min(dp1.x.get()),
                                dp0.y.get().min(dp1.y.get()),
                                dp0.x.get().max(dp1.x.get()),
                                dp0.y.get().max(dp1.y.get()),
                            )
                        {
                            imported_count = imported_count.saturating_add(1);
                            if imported_count <= MAX_IMPORTED_OBJECTS {
                                let id = doc.allocate_object_id();
                                let is_fill = op == "rectfill";
                                let fill = if is_fill { Some(FillStyle::uniform(state.color.clone())) } else { None };
                                let outline = if !is_fill {
                                    Some(OutlineStyle {
                                        width_pt: F64::new(state.line_width).unwrap_or(F64::ONE),
                                        color: state.stroke_color.clone(),
                                        cap: state.cap,
                                        join: state.join,
                                        miter_limit: F64::new(state.miter_limit).unwrap_or(F64::new(4.0).unwrap_or(F64::ONE)),
                                        behind_fill: false,
                                        scale_with_object: true,
                                        dash_pattern: state.dash_pattern.clone(),
                                        start_arrow: crate::style::ArrowheadKind::None,
                                        end_arrow: crate::style::ArrowheadKind::None,
                                    })
                                } else {
                                    None
                                };
                                let node = ObjectNode::new(id, Some("EPS Rect".to_owned()), ObjectKind::Rect(RectSpec::sharp(rect)), fill, outline);
                                push_node_to_target(&mut doc, 0, &mut saw_custom_layer, &current_target, node);
                            }
                        }
                    }
                    "fill" | "eofill" => {
                        if !cur_path.elements.is_empty() {
                            imported_count = imported_count.saturating_add(1);
                            if imported_count <= MAX_IMPORTED_OBJECTS {
                                let mut path = std::mem::take(&mut cur_path);
                                path.fill_rule = if op == "eofill" { FillRule::EvenOdd } else { FillRule::NonZero };
                                let id = doc.allocate_object_id();
                                let node = ObjectNode::new(
                                    id,
                                    Some("EPS Path".to_owned()),
                                    ObjectKind::Path(path),
                                    Some(FillStyle::uniform(state.color.clone())),
                                    None,
                                );
                                push_node_to_target(&mut doc, 0, &mut saw_custom_layer, &current_target, node);
                            }
                        }
                    }
                    "stroke" => {
                        if !cur_path.elements.is_empty() {
                            imported_count = imported_count.saturating_add(1);
                            if imported_count <= MAX_IMPORTED_OBJECTS {
                                let path = std::mem::take(&mut cur_path);
                                let id = doc.allocate_object_id();
                                let outl = OutlineStyle {
                                    width_pt: F64::new(state.line_width).unwrap_or(F64::ONE),
                                    color: state.stroke_color.clone(),
                                    cap: state.cap,
                                    join: state.join,
                                    miter_limit: F64::new(state.miter_limit).unwrap_or(F64::new(4.0).unwrap_or(F64::ONE)),
                                    behind_fill: false,
                                    scale_with_object: true,
                                    dash_pattern: state.dash_pattern.clone(),
                                    start_arrow: crate::style::ArrowheadKind::None,
                                    end_arrow: crate::style::ArrowheadKind::None,
                                };
                                let node = ObjectNode::new(id, Some("EPS Path".to_owned()), ObjectKind::Path(path), None, Some(outl));
                                push_node_to_target(&mut doc, 0, &mut saw_custom_layer, &current_target, node);
                            }
                        }
                    }
                    "clip" | "eoclip" | "rectclip" => {
                        if op == "rectclip" {
                            let _ = (pop_num(&mut op_stack), pop_num(&mut op_stack), pop_num(&mut op_stack), pop_num(&mut op_stack));
                        }
                        cur_path = Path::new();
                    }
                    "arc" | "arcn" => {
                        if let (Some(ang2), Some(ang1), Some(r), Some(cy), Some(cx)) =
                            (pop_num(&mut op_stack), pop_num(&mut op_stack), pop_num(&mut op_stack), pop_num(&mut op_stack), pop_num(&mut op_stack))
                            && r >= 0.0
                        {
                            let ccw = op == "arc";
                            let raw_diff = if ccw { f64_sub(ang2, ang1) } else { f64_sub(ang1, ang2) };
                            let sweep_deg = if raw_diff >= 360.0 {
                                360.0
                            } else {
                                let rem = raw_diff.rem_euclid(360.0);
                                if rem.abs() < 1e-9 && raw_diff > 0.0 { 360.0 } else { rem }
                            };
                            let signed_sweep_deg = if ccw { sweep_deg } else { f64_neg(sweep_deg) };
                            let a1_rad = ang1.to_radians();
                            let start_ps = Point::new(f64_add(cx, f64_mul(r, a1_rad.cos())), f64_add(cy, f64_mul(r, a1_rad.sin())));
                            if let Ok(sp) = start_ps
                                && let Ok(dsp) = map_ps_pt(sp, state.ctm)
                            {
                                if cur_path.elements.is_empty() {
                                    let _ = cur_path.move_to(dsp);
                                } else {
                                    let _ = cur_path.line_to(dsp);
                                }
                                cur_pt = sp;
                                if sweep_deg > 1e-6 && r > 1e-6 {
                                    let n_segs = ((sweep_deg / 90.0).ceil() as usize).clamp(1, 4);
                                    let d_theta = signed_sweep_deg.to_radians() / (n_segs as f64);
                                    let k = f64_mul(4.0 / 3.0, f64_mul(0.25, d_theta).tan());
                                    for seg_i in 0..n_segs {
                                        let th0 = f64_add(a1_rad, f64_mul(seg_i as f64, d_theta));
                                        let th1 = f64_add(th0, d_theta);
                                        let (sin0, cos0) = th0.sin_cos();
                                        let (sin1, cos1) = th1.sin_cos();
                                        let p0x = f64_add(cx, f64_mul(r, cos0));
                                        let p0y = f64_add(cy, f64_mul(r, sin0));
                                        let p3x = f64_add(cx, f64_mul(r, cos1));
                                        let p3y = f64_add(cy, f64_mul(r, sin1));
                                        let c1x = f64_sub(p0x, f64_mul(k, f64_mul(r, sin0)));
                                        let c1y = f64_add(p0y, f64_mul(k, f64_mul(r, cos0)));
                                        let c2x = f64_add(p3x, f64_mul(k, f64_mul(r, sin1)));
                                        let c2y = f64_sub(p3y, f64_mul(k, f64_mul(r, cos1)));
                                        if let (Ok(c1), Ok(c2), Ok(p3)) = (Point::new(c1x, c1y), Point::new(c2x, c2y), Point::new(p3x, p3y))
                                            && let (Ok(dc1), Ok(dc2), Ok(dp3)) =
                                                (map_ps_pt(c1, state.ctm), map_ps_pt(c2, state.ctm), map_ps_pt(p3, state.ctm))
                                        {
                                            let _ = cur_path.cubic_to(dc1, dc2, dp3);
                                            cur_pt = p3;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    "colorimage" => {
                        op_stack.clear();
                        if let Some((pw, ph)) = pending_image_dims.take() {
                            reading_hex_bitmap = Some((pw, ph, state.ctm, String::new()));
                        }
                    }
                    _ => {}
                },
            }
        }
    }

    Ok(doc)
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

type PdfBox4 = (f64, f64, f64, f64);

fn extract_pdf_box(dict_str: &str, key: &str) -> Option<PdfBox4> {
    let idx = dict_str.find(key)?;
    let after = dict_str.get(idx.saturating_add(key.len())..)?;
    let open = after.find('[')?;
    let close = after.get(open.saturating_add(1)..)?.find(']')?;
    let inside = after.get(open.saturating_add(1)..open.saturating_add(1).saturating_add(close))?;
    parse_four_floats(inside)
}

fn extract_pdf_contents_refs(dict_str: &str) -> Vec<u32> {
    let Some(idx) = dict_str.find("/Contents") else {
        return Vec::new();
    };
    let Some(after) = dict_str.get(idx.saturating_add("/Contents".len())..) else {
        return Vec::new();
    };
    let trimmed = after.trim_start();
    if let Some(after_open) = trimmed.strip_prefix('[') {
        let end_bracket = after_open.find(']').unwrap_or(after_open.len());
        let inside = after_open.get(..end_bracket).unwrap_or(after_open);
        let toks: Vec<&str> = inside.split_whitespace().collect();
        let mut refs = Vec::new();
        let mut i = 0usize;
        while let Some(&tok) = toks.get(i) {
            if toks.get(i.saturating_add(1)) == Some(&"0")
                && toks.get(i.saturating_add(2)) == Some(&"R")
                && let Ok(id) = tok.parse::<u32>()
            {
                refs.push(id);
                i = i.saturating_add(3);
            } else {
                i = i.saturating_add(1);
            }
        }
        refs
    } else {
        trimmed.split_whitespace().next().and_then(|s| s.parse::<u32>().ok()).into_iter().collect()
    }
}

fn extract_pdf_parent_ref(dict_str: &str) -> Option<u32> {
    let idx = dict_str.find("/Parent")?;
    let after = dict_str.get(idx.saturating_add("/Parent".len())..)?.trim_start();
    after.split_whitespace().next()?.parse::<u32>().ok()
}

fn resolve_pdf_media_box(dict_str: &str, objects: &BTreeMap<u32, &[u8]>) -> Option<PdfBox4> {
    if let Some(mb) = extract_pdf_box(dict_str, "/MediaBox") {
        return Some(mb);
    }
    let mut cur_parent = extract_pdf_parent_ref(dict_str);
    let mut visited = [0u32; 8];
    for hop in 0..8usize {
        let pid = cur_parent?;
        if visited.get(..hop).is_some_and(|v| v.contains(&pid)) {
            break;
        }
        if let Some(slot) = visited.get_mut(hop) {
            *slot = pid;
        }
        let &pbody = objects.get(&pid)?;
        let dict_end = find_subslice(pbody, b"stream").unwrap_or(pbody.len());
        let pdict = String::from_utf8_lossy(pbody.get(..dict_end).unwrap_or(pbody));
        if let Some(mb) = extract_pdf_box(&pdict, "/MediaBox") {
            return Some(mb);
        }
        cur_parent = extract_pdf_parent_ref(&pdict);
    }
    None
}

fn extract_stream_payload(obj_bytes: &[u8]) -> Option<&[u8]> {
    let stream_pos = find_subslice(obj_bytes, b"stream")?;
    let after_kw = obj_bytes.get(stream_pos.saturating_add(6)..)?;
    let body_start = if after_kw.starts_with(b"\r\n") {
        2usize
    } else if after_kw.starts_with(b"\n") || after_kw.starts_with(b"\r") {
        1usize
    } else {
        0usize
    };
    let stream_body = after_kw.get(body_start..)?;
    let end_pos = find_subslice(stream_body, b"endstream")?;
    stream_body.get(..end_pos)
}

/// Imports an ISO 32000-1 PDF 1.4 byte slice into a multi-page [`Document`].
pub fn import_pdf(pdf_bytes: &[u8]) -> Result<Document, DocError> {
    if pdf_bytes.len() > MAX_IMPORT_BYTES {
        return Err(DocError::InvalidDocument(format!("PDF input size {} exceeds maximum {}", pdf_bytes.len(), MAX_IMPORT_BYTES)));
    }
    if !pdf_bytes.starts_with(b"%PDF-") {
        return Err(DocError::BadMagic);
    }

    let mut objects: BTreeMap<u32, &[u8]> = BTreeMap::new();
    let mut cursor = 0usize;
    while cursor < pdf_bytes.len() {
        let Some(rem) = pdf_bytes.get(cursor..) else {
            break;
        };
        let Some(obj_rel) = find_subslice(rem, b" 0 obj") else {
            break;
        };
        let obj_kw_pos = cursor.saturating_add(obj_rel);
        let mut num_start = obj_kw_pos;
        while num_start > cursor {
            let prev = num_start.saturating_sub(1);
            if pdf_bytes.get(prev).is_some_and(u8::is_ascii_digit) {
                num_start = prev;
            } else {
                break;
            }
        }
        let obj_num = pdf_bytes.get(num_start..obj_kw_pos).and_then(|s| std::str::from_utf8(s).ok()).and_then(|s| s.parse::<u32>().ok());
        let body_start = obj_kw_pos.saturating_add(6);
        let Some(after_obj) = pdf_bytes.get(body_start..) else {
            break;
        };
        let Some(end_rel) = find_subslice(after_obj, b"endobj") else {
            break;
        };
        if let Some(n) = obj_num
            && let Some(body) = after_obj.get(..end_rel)
        {
            objects.insert(n, body);
        }
        cursor = body_start.saturating_add(end_rel).saturating_add(6);
    }

    let mut page_entries: Vec<(PdfBox4, Option<PdfBox4>, Vec<u32>)> = Vec::new();
    for (&_obj_num, &body) in &objects {
        let dict_end = find_subslice(body, b"stream").unwrap_or(body.len());
        let Some(dict_slice) = body.get(..dict_end) else {
            continue;
        };
        let dict_str = String::from_utf8_lossy(dict_slice);
        if (dict_str.contains("/Type /Page ") || dict_str.contains("/Type /Page\n") || dict_str.contains("/Type /Page/"))
            && !dict_str.contains("/Type /Pages")
        {
            let media = resolve_pdf_media_box(&dict_str, &objects).unwrap_or((0.0, 0.0, 595.275_590_551_181_2, 841.889_763_779_527_6));
            let trim = extract_pdf_box(&dict_str, "/TrimBox");
            let contents_ids = extract_pdf_contents_refs(&dict_str);
            if !contents_ids.is_empty() {
                page_entries.push((media, trim, contents_ids));
            }
        }
    }

    if page_entries.is_empty() {
        return Err(DocError::InvalidDocument("PDF contains no /Type /Page objects".to_owned()));
    }

    let mut doc = Document::new_a4_portrait();
    doc.pages.clear();
    let mut imported_count = 0usize;

    for (page_idx, (media, trim, contents_ids)) in page_entries.into_iter().enumerate() {
        let (m_llx, m_lly, m_urx, m_ury) = media;
        let (page_w, page_h, bleed_pt, origin_x, origin_y_top) = if let Some((tx0, ty0, tx1, ty1)) = trim {
            let pw = f64_sub(tx1, tx0).clamp(1.0, 1_000_000.0);
            let ph = f64_sub(ty1, ty0).clamp(1.0, 1_000_000.0);
            let bl = f64_sub(tx0, m_llx).max(0.0);
            (pw, ph, bl, tx0, f64_sub(m_ury, ty0))
        } else {
            let pw = f64_sub(m_urx, m_llx).clamp(1.0, 1_000_000.0);
            let ph = f64_sub(m_ury, m_lly).clamp(1.0, 1_000_000.0);
            (pw, ph, 0.0, m_llx, f64_sub(m_ury, m_lly))
        };

        let pid = PageId(u64::try_from(page_idx.saturating_add(1)).unwrap_or(1));
        let lid = LayerId(doc.allocate_object_id().0);
        let page = Page {
            id: pid,
            name: format!("Page {}", page_idx.saturating_add(1)),
            size: Size::new(page_w, page_h).unwrap_or_default(),
            bleed: F64::new(bleed_pt).unwrap_or(F64::ZERO),
            background: None,
            layers: vec![Layer::new_normal(lid, "Layer 1")],
            residual: Residual::default(),
            extra: BTreeMap::new(),
        };
        doc.pages.push(page);

        let mut combined_stream_bytes: Vec<u8> = Vec::new();
        for cid in contents_ids {
            if let Some(&content_obj) = objects.get(&cid) {
                let part = extract_stream_payload(content_obj).unwrap_or(content_obj);
                if !combined_stream_bytes.is_empty() {
                    combined_stream_bytes.push(b'\n');
                }
                combined_stream_bytes.extend_from_slice(part);
            }
        }
        if combined_stream_bytes.is_empty() {
            continue;
        }
        let stream_str = String::from_utf8_lossy(&combined_stream_bytes);

        let mut saw_custom_layer = false;
        let mut current_target: Option<(LayerRole, String, bool)> = None;
        let mut pending_doc_b64 = String::new();
        let mut pending_page_b64 = String::new();
        let mut pending_layer_b64 = String::new();
        let mut pending_node_b64 = String::new();
        let mut skip_until_end_node = false;
        let mut skip_bg_block = false;
        let mut in_inline_image = false;
        let mut inline_img_w = 0u32;
        let mut inline_img_h = 0u32;
        let mut inline_img_data = false;
        let mut inline_img_hex = String::new();

        // Base transform maps PDF bottom-left user space `(x_pdf, y_pdf)` to CoralCraft top-left `(x_pdf - origin_x, origin_y_top - y_pdf)`.
        let base_ctm = Affine2::new(1.0, 0.0, 0.0, -1.0, f64_neg(origin_x), origin_y_top).unwrap_or(Affine2::IDENTITY);
        let mut state = PsGraphicsState { ctm: base_ctm, ..PsGraphicsState::default() };
        let mut state_stack: Vec<PsGraphicsState> = Vec::new();
        let mut op_stack: Vec<PsToken> = Vec::new();
        let mut cur_path = Path::new();
        let mut cur_pt = Point::ZERO;
        let mut text_pos = Point::ZERO;

        for raw_line in stream_str.lines() {
            let line = raw_line.trim();
            if line.is_empty() {
                continue;
            }

            if let Some(rest) = line.strip_prefix("% CoralCraftDoc:") {
                pending_doc_b64.clear();
                pending_doc_b64.push_str(rest.trim());
                continue;
            }
            if let Some(rest) = line.strip_prefix("% CoralCraftDoc+:") {
                pending_doc_b64.push_str(rest.trim());
                continue;
            }
            if !pending_doc_b64.is_empty() {
                if let Some(dmeta) = decode_interchange_meta::<DxfDocMeta>(&pending_doc_b64) {
                    doc.title = dmeta.title;
                    doc.units = dmeta.units;
                    doc.palette = dmeta.palette;
                    doc.residual = dmeta.residual;
                }
                pending_doc_b64.clear();
            }

            if let Some(rest) = line.strip_prefix("% CoralCraftPage:") {
                pending_page_b64.clear();
                pending_page_b64.push_str(rest.trim());
                continue;
            }
            if let Some(rest) = line.strip_prefix("% CoralCraftPage+:") {
                pending_page_b64.push_str(rest.trim());
                continue;
            }
            if !pending_page_b64.is_empty() {
                if let Some(pmeta) = decode_interchange_meta::<DxfPageMeta>(&pending_page_b64)
                    && let Some(p) = doc.pages.get_mut(page_idx)
                {
                    p.name = pmeta.name;
                    p.bleed = pmeta.bleed;
                    p.background = pmeta.background;
                    p.residual = pmeta.residual;
                }
                pending_page_b64.clear();
            }

            if line == "% CoralCraftSkipBegin" {
                skip_bg_block = true;
                continue;
            }
            if line == "% CoralCraftSkipEnd" {
                skip_bg_block = false;
                continue;
            }
            if skip_bg_block {
                continue;
            }

            if let Some(rest) = line.strip_prefix("% CoralCraftLayer:") {
                if !pending_layer_b64.is_empty()
                    && let Some(lmeta) = decode_interchange_meta::<DxfLayerMeta>(&pending_layer_b64)
                {
                    current_target = Some(apply_layer_meta(&mut doc, page_idx, &mut saw_custom_layer, lmeta));
                }
                pending_layer_b64.clear();
                pending_layer_b64.push_str(rest.trim());
                continue;
            }
            if let Some(rest) = line.strip_prefix("% CoralCraftLayer+:") {
                pending_layer_b64.push_str(rest.trim());
                continue;
            }
            if !pending_layer_b64.is_empty() {
                if let Some(lmeta) = decode_interchange_meta::<DxfLayerMeta>(&pending_layer_b64) {
                    current_target = Some(apply_layer_meta(&mut doc, page_idx, &mut saw_custom_layer, lmeta));
                }
                pending_layer_b64.clear();
            }

            if let Some(rest) = line.strip_prefix("% CoralCraftNode:") {
                pending_node_b64.clear();
                pending_node_b64.push_str(rest.trim());
                continue;
            }
            if let Some(rest) = line.strip_prefix("% CoralCraftNode+:") {
                pending_node_b64.push_str(rest.trim());
                continue;
            }
            if !pending_node_b64.is_empty() {
                if let Some(mut node) = decode_interchange_meta::<ObjectNode>(&pending_node_b64) {
                    let skip_master_dup = current_target.as_ref().is_some_and(|(_, _, is_master)| *is_master && page_idx > 0);
                    if !skip_master_dup {
                        imported_count = imported_count.saturating_add(1);
                        if imported_count <= MAX_IMPORTED_OBJECTS {
                            remap_imported_node_ids(&mut doc, &mut node);
                            push_node_to_target(&mut doc, page_idx, &mut saw_custom_layer, &current_target, node);
                        }
                    }
                    skip_until_end_node = true;
                }
                pending_node_b64.clear();
            }

            if line == "% CoralCraftEndNode" {
                skip_until_end_node = false;
                continue;
            }
            if skip_until_end_node || line.starts_with('%') {
                continue;
            }

            if in_inline_image {
                if line == "EI" {
                    if let Some(rgba) = decode_ascii_hex_rgb_to_rgba(&inline_img_hex, inline_img_w, inline_img_h) {
                        imported_count = imported_count.saturating_add(1);
                        if imported_count <= MAX_IMPORTED_OBJECTS {
                            let id = doc.allocate_object_id();
                            let w_pt = state.ctm.a.get().abs().max(1.0);
                            let h_pt = state.ctm.d.get().abs().max(1.0);
                            let dpi_x = F64::new(f64_mul(f64::from(inline_img_w), 72.0) / w_pt).unwrap_or(F64::new(72.0).unwrap_or(F64::ONE));
                            let dpi_y = F64::new(f64_mul(f64::from(inline_img_h), 72.0) / h_pt).unwrap_or(F64::new(72.0).unwrap_or(F64::ONE));
                            let mut node = ObjectNode::new(
                                id,
                                Some("PDF Inline Image".to_owned()),
                                ObjectKind::Bitmap { pixel_width: inline_img_w, pixel_height: inline_img_h, dpi_x, dpi_y, rgba_pixels: rgba },
                                None,
                                None,
                            );
                            let p0 = state.ctm.apply_point(Point::ZERO).unwrap_or(Point::ZERO);
                            let p1 = state.ctm.apply_point(Point::new(1.0, 1.0).unwrap_or(Point::ZERO)).unwrap_or(Point::ZERO);
                            if let Ok(tf) = Affine2::translation(p0.x.get().min(p1.x.get()), p0.y.get().min(p1.y.get())) {
                                node.transform = tf;
                            }
                            push_node_to_target(&mut doc, page_idx, &mut saw_custom_layer, &current_target, node);
                        }
                    }
                    in_inline_image = false;
                    inline_img_data = false;
                    inline_img_hex.clear();
                    continue;
                }
                if inline_img_data {
                    inline_img_hex.push_str(line);
                } else if line == "ID" {
                    inline_img_data = true;
                } else if let Some(rest) = line.strip_prefix("/W") {
                    inline_img_w = rest.trim().parse::<u32>().unwrap_or(inline_img_w);
                } else if let Some(rest) = line.strip_prefix("/H") {
                    inline_img_h = rest.trim().parse::<u32>().unwrap_or(inline_img_h);
                }
                continue;
            }

            for tok in tokenize_ps_pdf_line(line) {
                match tok {
                    PsToken::Num(_) | PsToken::Name(_) | PsToken::Str(_) | PsToken::Array(_) => {
                        push_bounded(&mut op_stack, tok);
                    }
                    PsToken::Op(op) => match op.as_str() {
                        "q" => {
                            if state_stack.len() < MAX_STATE_DEPTH {
                                state_stack.push(state.clone());
                            }
                        }
                        "Q" => {
                            if let Some(prev) = state_stack.pop() {
                                state = prev;
                            }
                        }
                        "cm" => {
                            if let (Some(ty), Some(tx), Some(d), Some(c), Some(b), Some(a)) = (
                                pop_num(&mut op_stack),
                                pop_num(&mut op_stack),
                                pop_num(&mut op_stack),
                                pop_num(&mut op_stack),
                                pop_num(&mut op_stack),
                                pop_num(&mut op_stack),
                            ) && let Ok(m) = Affine2::new(a, b, c, d, tx, ty)
                                && let Ok(next) = m.then(state.ctm)
                            {
                                state.ctm = next;
                            }
                        }
                        "m" => {
                            if let (Some(y), Some(x)) = (pop_num(&mut op_stack), pop_num(&mut op_stack))
                                && let Ok(p) = Point::new(x, y)
                                && let Ok(dp) = state.ctm.apply_point(p)
                            {
                                let _ = cur_path.move_to(dp);
                                cur_pt = p;
                            }
                        }
                        "l" => {
                            if let (Some(y), Some(x)) = (pop_num(&mut op_stack), pop_num(&mut op_stack))
                                && let Ok(p) = Point::new(x, y)
                                && let Ok(dp) = state.ctm.apply_point(p)
                            {
                                let _ = cur_path.line_to(dp);
                                cur_pt = p;
                            }
                        }
                        "c" => {
                            if let (Some(y3), Some(x3), Some(y2), Some(x2), Some(y1), Some(x1)) = (
                                pop_num(&mut op_stack),
                                pop_num(&mut op_stack),
                                pop_num(&mut op_stack),
                                pop_num(&mut op_stack),
                                pop_num(&mut op_stack),
                                pop_num(&mut op_stack),
                            ) && let (Ok(p1), Ok(p2), Ok(p3)) = (Point::new(x1, y1), Point::new(x2, y2), Point::new(x3, y3))
                                && let (Ok(dp1), Ok(dp2), Ok(dp3)) = (state.ctm.apply_point(p1), state.ctm.apply_point(p2), state.ctm.apply_point(p3))
                            {
                                let _ = cur_path.cubic_to(dp1, dp2, dp3);
                                cur_pt = p3;
                            }
                        }
                        "v" => {
                            if let (Some(y3), Some(x3), Some(y2), Some(x2)) =
                                (pop_num(&mut op_stack), pop_num(&mut op_stack), pop_num(&mut op_stack), pop_num(&mut op_stack))
                                && let (Ok(p2), Ok(p3)) = (Point::new(x2, y2), Point::new(x3, y3))
                                && let (Ok(dp1), Ok(dp2), Ok(dp3)) =
                                    (state.ctm.apply_point(cur_pt), state.ctm.apply_point(p2), state.ctm.apply_point(p3))
                            {
                                let _ = cur_path.cubic_to(dp1, dp2, dp3);
                                cur_pt = p3;
                            }
                        }
                        "y" => {
                            if let (Some(y3), Some(x3), Some(y1), Some(x1)) =
                                (pop_num(&mut op_stack), pop_num(&mut op_stack), pop_num(&mut op_stack), pop_num(&mut op_stack))
                                && let (Ok(p1), Ok(p3)) = (Point::new(x1, y1), Point::new(x3, y3))
                                && let (Ok(dp1), Ok(dp3)) = (state.ctm.apply_point(p1), state.ctm.apply_point(p3))
                            {
                                let _ = cur_path.cubic_to(dp1, dp3, dp3);
                                cur_pt = p3;
                            }
                        }
                        "h" => {
                            let _ = cur_path.close();
                        }
                        "re" => {
                            if let (Some(rh), Some(rw), Some(ry), Some(rx)) =
                                (pop_num(&mut op_stack), pop_num(&mut op_stack), pop_num(&mut op_stack), pop_num(&mut op_stack))
                                && let (Ok(p0), Ok(p1), Ok(p2), Ok(p3)) = (
                                    Point::new(rx, ry),
                                    Point::new(f64_add(rx, rw), ry),
                                    Point::new(f64_add(rx, rw), f64_add(ry, rh)),
                                    Point::new(rx, f64_add(ry, rh)),
                                )
                                && let (Ok(dp0), Ok(dp1), Ok(dp2), Ok(dp3)) =
                                    (state.ctm.apply_point(p0), state.ctm.apply_point(p1), state.ctm.apply_point(p2), state.ctm.apply_point(p3))
                            {
                                let _ = cur_path.move_to(dp0);
                                let _ = cur_path.line_to(dp1);
                                let _ = cur_path.line_to(dp2);
                                let _ = cur_path.line_to(dp3);
                                let _ = cur_path.close();
                                cur_pt = p0;
                            }
                        }
                        "rg" | "RG" => {
                            if let (Some(b), Some(g), Some(r)) = (pop_num(&mut op_stack), pop_num(&mut op_stack), pop_num(&mut op_stack)) {
                                let col = Color::rgb8(
                                    f64_mul(r.clamp(0.0, 1.0), 255.0).round() as u8,
                                    f64_mul(g.clamp(0.0, 1.0), 255.0).round() as u8,
                                    f64_mul(b.clamp(0.0, 1.0), 255.0).round() as u8,
                                );
                                if op == "RG" {
                                    state.stroke_color = col;
                                } else {
                                    state.color = col;
                                }
                            }
                        }
                        "k" | "K" => {
                            if let (Some(k), Some(y), Some(m), Some(c)) =
                                (pop_num(&mut op_stack), pop_num(&mut op_stack), pop_num(&mut op_stack), pop_num(&mut op_stack))
                                && let Ok(col) = Color::cmyk(c.clamp(0.0, 1.0), m.clamp(0.0, 1.0), y.clamp(0.0, 1.0), k.clamp(0.0, 1.0))
                            {
                                if op == "K" {
                                    state.stroke_color = col;
                                } else {
                                    state.color = col;
                                }
                            }
                        }
                        "g" | "G" => {
                            if let Some(g) = pop_num(&mut op_stack)
                                && let Ok(col) = Color::gray(g.clamp(0.0, 1.0))
                            {
                                if op == "G" {
                                    state.stroke_color = col;
                                } else {
                                    state.color = col;
                                }
                            }
                        }
                        "w" => {
                            if let Some(w) = pop_num(&mut op_stack) {
                                state.line_width = w.max(0.0);
                            }
                        }
                        "J" => {
                            if let Some(c) = pop_num(&mut op_stack) {
                                state.cap = match c.round() as i32 {
                                    1 => LineCap::Round,
                                    2 => LineCap::Square,
                                    _ => LineCap::Butt,
                                };
                            }
                        }
                        "j" => {
                            if let Some(j) = pop_num(&mut op_stack) {
                                state.join = match j.round() as i32 {
                                    1 => LineJoin::Round,
                                    2 => LineJoin::Bevel,
                                    _ => LineJoin::Miter,
                                };
                            }
                        }
                        "M" => {
                            if let Some(m) = pop_num(&mut op_stack) {
                                state.miter_limit = m.max(1.0);
                            }
                        }
                        "d" => {
                            let _phase = pop_num(&mut op_stack);
                            if let Some(PsToken::Array(arr)) = op_stack.pop() {
                                state.dash_pattern = arr
                                    .into_iter()
                                    .filter_map(|t| match t {
                                        PsToken::Num(v) if v > 0.0 => F64::new(v).ok(),
                                        _ => None,
                                    })
                                    .collect();
                            }
                        }
                        "f" | "F" | "f*" | "S" | "s" | "B" | "B*" | "b" | "b*" => {
                            if matches!(op.as_str(), "s" | "b" | "b*") {
                                let _ = cur_path.close();
                            }
                            if !cur_path.elements.is_empty() {
                                imported_count = imported_count.saturating_add(1);
                                if imported_count <= MAX_IMPORTED_OBJECTS {
                                    let mut path = std::mem::take(&mut cur_path);
                                    if matches!(op.as_str(), "f*" | "B*" | "b*") {
                                        path.fill_rule = FillRule::EvenOdd;
                                    }
                                    let has_fill = matches!(op.as_str(), "f" | "F" | "f*" | "B" | "B*" | "b" | "b*");
                                    let has_stroke = matches!(op.as_str(), "S" | "s" | "B" | "B*" | "b" | "b*");
                                    let fill = if has_fill { Some(FillStyle::uniform(state.color.clone())) } else { None };
                                    let outline = if has_stroke {
                                        Some(OutlineStyle {
                                            width_pt: F64::new(state.line_width).unwrap_or(F64::ONE),
                                            color: state.stroke_color.clone(),
                                            cap: state.cap,
                                            join: state.join,
                                            miter_limit: F64::new(state.miter_limit).unwrap_or(F64::new(4.0).unwrap_or(F64::ONE)),
                                            behind_fill: false,
                                            scale_with_object: true,
                                            dash_pattern: state.dash_pattern.clone(),
                                            start_arrow: crate::style::ArrowheadKind::None,
                                            end_arrow: crate::style::ArrowheadKind::None,
                                        })
                                    } else {
                                        None
                                    };
                                    let id = doc.allocate_object_id();
                                    let node = ObjectNode::new(id, Some("PDF Path".to_owned()), ObjectKind::Path(path), fill, outline);
                                    push_node_to_target(&mut doc, page_idx, &mut saw_custom_layer, &current_target, node);
                                }
                            }
                        }
                        "n" | "W" | "W*" => {
                            if op == "n" {
                                cur_path = Path::new();
                            }
                        }
                        "BT" => {
                            text_pos = Point::ZERO;
                        }
                        "ET" => {}
                        "Tf" => {
                            if let Some(sz) = pop_num(&mut op_stack) {
                                state.font_size = sz.max(1.0);
                            }
                            let _font_name = op_stack.pop();
                        }
                        "Tm" => {
                            if let (Some(ty), Some(tx), Some(_d), Some(_c), Some(_b), Some(_a)) = (
                                pop_num(&mut op_stack),
                                pop_num(&mut op_stack),
                                pop_num(&mut op_stack),
                                pop_num(&mut op_stack),
                                pop_num(&mut op_stack),
                                pop_num(&mut op_stack),
                            ) && let Ok(p) = Point::new(tx, ty)
                            {
                                text_pos = p;
                            }
                        }
                        "Td" | "TD" => {
                            if let (Some(ty), Some(tx)) = (pop_num(&mut op_stack), pop_num(&mut op_stack))
                                && let Ok(p) = Point::new(f64_add(text_pos.x.get(), tx), f64_add(text_pos.y.get(), ty))
                            {
                                text_pos = p;
                            }
                        }
                        "Tj" => {
                            if let Some(PsToken::Str(text)) = op_stack.pop()
                                && !text.is_empty()
                                && let Ok(dp) = state.ctm.apply_point(text_pos)
                                && let Ok(size_pt) = F64::new(state.font_size)
                            {
                                imported_count = imported_count.saturating_add(1);
                                if imported_count <= MAX_IMPORTED_OBJECTS {
                                    let id = doc.allocate_object_id();
                                    let node = ObjectNode::new(
                                        id,
                                        Some("PDF Text".to_owned()),
                                        ObjectKind::ArtisticText { text, font_family: state.font_family.clone(), size_pt, position: dp },
                                        Some(FillStyle::uniform(state.color.clone())),
                                        None,
                                    );
                                    push_node_to_target(&mut doc, page_idx, &mut saw_custom_layer, &current_target, node);
                                }
                            }
                        }
                        "TJ" => {
                            if let Some(PsToken::Array(arr)) = op_stack.pop() {
                                let mut text = String::new();
                                for elem in arr {
                                    match elem {
                                        PsToken::Str(s) => text.push_str(&s),
                                        PsToken::Num(kern) if kern <= -180.0 && !text.ends_with(' ') => text.push(' '),
                                        _ => {}
                                    }
                                }
                                if !text.is_empty()
                                    && let Ok(dp) = state.ctm.apply_point(text_pos)
                                    && let Ok(size_pt) = F64::new(state.font_size)
                                {
                                    imported_count = imported_count.saturating_add(1);
                                    if imported_count <= MAX_IMPORTED_OBJECTS {
                                        let id = doc.allocate_object_id();
                                        let node = ObjectNode::new(
                                            id,
                                            Some("PDF Text".to_owned()),
                                            ObjectKind::ArtisticText { text, font_family: state.font_family.clone(), size_pt, position: dp },
                                            Some(FillStyle::uniform(state.color.clone())),
                                            None,
                                        );
                                        push_node_to_target(&mut doc, page_idx, &mut saw_custom_layer, &current_target, node);
                                    }
                                }
                            }
                        }
                        "BI" => {
                            in_inline_image = true;
                            inline_img_w = 1;
                            inline_img_h = 1;
                            inline_img_data = false;
                            inline_img_hex.clear();
                        }
                        _ => {}
                    },
                }
            }
        }
        let _ = ObjectId(1);
    }

    Ok(doc)
}
