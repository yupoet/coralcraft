//! Clean-room ISO 32000-1 PDF 1.4 multi-page vector exporter (`export_pdf`) for `coralcraft-render`.
//!
//! Preserves native CMYK (`k`/`K`) and RGB (`rg`/`RG`) color operators, per-page `/MediaBox`,
//! `/TrimBox`, and `/BleedBox`, cubic Bézier geometry, `behind_fill` stroke ordering,
//! `scale_with_object`, Hairline (`0.25pt`), Fountain Fill shading bands, and PowerClip (`W`/`W* n`) clipping.

use std::fmt::Write as _;

use coralcraft_color::{Color, interpolate_stops};
use coralcraft_doc::{Document, FillStyle, FountainGeometry, LineCap, LineJoin, ObjectKind, ObjectNode, OutlineStyle};
use coralcraft_geom::{Affine2, FillRule, Path, PathElement, Point, Rect, RectSpec};

use crate::surface::RenderError;
use crate::svg::fmt_num;
use coralcraft_geom::F64;
use serde::{Deserialize, Serialize};

/// Prepress printer's marks options for print-ready PDF 1.4 and SVG 1.1 export.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrepressOptions {
    /// Emit corner crop marks indicating the page trim boundary.
    pub crop_marks: bool,
    /// Emit registration bullseye targets in Registration color (`CMYK 100/100/100/100`).
    pub registration_marks: bool,
    /// Optional bleed override in typographic points (`0.0` = use `page.bleed`).
    pub bleed_pt: F64,
}

/// N-Up sheet imposition specification (`File -> Print Preview -> Imposition Layout Tool`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImpositionSpec {
    /// Press sheet width in typographic points.
    pub sheet_width_pt: F64,
    /// Press sheet height in typographic points.
    pub sheet_height_pt: F64,
    /// Number of rows on the sheet (`>= 1`).
    pub rows: u32,
    /// Number of columns on the sheet (`>= 1`).
    pub cols: u32,
    /// Outer sheet margin in typographic points.
    pub margin_pt: F64,
    /// Gutter spacing between adjacent cells in typographic points.
    pub gutter_pt: F64,
    /// Emit corner crop marks around each imposed cell.
    pub crop_marks: bool,
}

/// Exports `doc` imposed onto an N-Up (`rows × cols`) press sheet in ISO 32000-1 PDF 1.4 format.
/// If `doc.pages.len() == 1`, repeats Page 1 across all `rows * cols` cells (step-and-repeat imposition);
/// otherwise places sequential document pages into cells across the sheet.
pub fn export_pdf_imposed(doc: &Document, spec: &ImpositionSpec) -> Result<Vec<u8>, RenderError> {
    if doc.pages.is_empty() {
        return Err(RenderError::Doc(coralcraft_doc::DocError::PageOutOfRange { index: 0, count: 0 }));
    }
    let rows = spec.rows.clamp(1, 32);
    let cols = spec.cols.clamp(1, 32);
    let sheet_w = spec.sheet_width_pt.get().max(72.0);
    let sheet_h = spec.sheet_height_pt.get().max(72.0);
    let margin = spec.margin_pt.get().max(0.0);
    let gutter = spec.gutter_pt.get().max(0.0);

    let usable_w = (sheet_w - 2.0 * margin - f64::from(cols - 1) * gutter).max(10.0);
    let usable_h = (sheet_h - 2.0 * margin - f64::from(rows - 1) * gutter).max(10.0);
    let cell_w = usable_w / f64::from(cols);
    let cell_h = usable_h / f64::from(rows);

    let mut s = String::new();
    s.push_str("q\n");
    let _ = writeln!(s, "1 0 0 -1 0 {} cm", fmt_num(sheet_h));

    for r in 0..rows {
        for c in 0..cols {
            let cell_idx = (r * cols + c) as usize;
            let page_idx = if doc.pages.len() == 1 { 0 } else { cell_idx % doc.pages.len() };
            let page = &doc.pages[page_idx];
            let pw = page.size.width.get().max(1.0);
            let ph = page.size.height.get().max(1.0);
            let scale = (cell_w / pw).min(cell_h / ph);
            let cell_x = margin + f64::from(c) * (cell_w + gutter) + 0.5 * (cell_w - pw * scale);
            let cell_y = margin + f64::from(r) * (cell_h + gutter) + 0.5 * (cell_h - ph * scale);

            s.push_str("q\n");
            let _ = writeln!(s, "{} 0 0 {} {} {} cm", fmt_num(scale), fmt_num(scale), fmt_num(cell_x), fmt_num(cell_y));
            let layers = doc.effective_layers_for_page(page_idx, true)?;
            for layer in layers {
                if layer.opacity.get() <= 0.0 {
                    continue;
                }
                for node in &layer.objects {
                    emit_pdf_node(&mut s, node)?;
                }
            }
            if spec.crop_marks {
                let off = 2.0;
                let len = 8.0;
                s.push_str("q\n1 1 1 1 K\n0.25 w\n");
                let _ = writeln!(s, "{} 0 m {} 0 l 0 {} m 0 {} l S", fmt_num(-len), fmt_num(-off), fmt_num(-len), fmt_num(-off));
                let _ = writeln!(
                    s,
                    "{} 0 m {} 0 l {} {} m {} {} l S",
                    fmt_num(pw + off),
                    fmt_num(pw + len),
                    fmt_num(pw),
                    fmt_num(-len),
                    fmt_num(pw),
                    fmt_num(-off)
                );
                let _ = writeln!(
                    s,
                    "{} {} m {} {} l 0 {} m 0 {} l S",
                    fmt_num(-len),
                    fmt_num(ph),
                    fmt_num(-off),
                    fmt_num(ph),
                    fmt_num(ph + off),
                    fmt_num(ph + len)
                );
                let _ = writeln!(
                    s,
                    "{} {} m {} {} l {} {} m {} {} l S",
                    fmt_num(pw + off),
                    fmt_num(ph),
                    fmt_num(pw + len),
                    fmt_num(ph),
                    fmt_num(pw),
                    fmt_num(ph + off),
                    fmt_num(pw),
                    fmt_num(ph + len)
                );
                s.push_str("Q\n");
            }
            s.push_str("Q\n");
        }
    }
    s.push_str("Q\n");

    let mut objects: Vec<Vec<u8>> = Vec::with_capacity(5);
    objects.push(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    objects.push(b"<< /Type /Pages /Kids [4 0 R] /Count 1 >>".to_vec());
    objects.push(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec());
    objects.push(
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Resources << /Font << /F1 3 0 R >> >> /Contents 5 0 R >>",
            fmt_num(sheet_w),
            fmt_num(sheet_h)
        )
        .into_bytes(),
    );
    let stream_bytes = s.as_bytes();
    let mut content_obj = format!("<< /Length {} >>\nstream\n", stream_bytes.len()).into_bytes();
    content_obj.extend_from_slice(stream_bytes);
    content_obj.extend_from_slice(b"endstream");
    objects.push(content_obj);

    let mut out = Vec::with_capacity(4096);
    out.extend_from_slice(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n");
    let mut offsets = Vec::with_capacity(objects.len() + 1);
    offsets.push(0usize);
    for (idx, obj_bytes) in objects.iter().enumerate() {
        let obj_num = idx + 1;
        offsets.push(out.len());
        out.extend_from_slice(format!("{obj_num} 0 obj\n").as_bytes());
        out.extend_from_slice(obj_bytes);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref_offset = out.len();
    let size = offsets.len();
    out.extend_from_slice(format!("xref\n0 {size}\n0000000000 65535 f \n").as_bytes());
    for &off in &offsets[1..] {
        out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {size} /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n").as_bytes());
    Ok(out)
}

/// Exports all pages of `doc` into a self-contained ISO 32000-1 PDF 1.4 byte vector.
pub fn export_pdf(doc: &Document) -> Result<Vec<u8>, RenderError> {
    export_pdf_with_prepress(doc, &PrepressOptions::default())
}

/// Exports all pages of `doc` into an ISO 32000-1 PDF 1.4 byte vector with optional prepress marks.
pub fn export_pdf_with_prepress(doc: &Document, prepress: &PrepressOptions) -> Result<Vec<u8>, RenderError> {
    if doc.pages.is_empty() {
        return Err(RenderError::Doc(coralcraft_doc::DocError::PageOutOfRange { index: 0, count: 0 }));
    }

    // Object numbering plan:
    // 1 0 obj: Catalog
    // 2 0 obj: Pages
    // 3 0 obj: Standard Type1 Font (/Helvetica)
    // For page i in 0..N:
    //   page_obj = 4 + 2 * i
    //   content_obj = 5 + 2 * i
    let page_count = doc.pages.len();
    let total_objects = 3 + 2 * page_count;
    let mut objects: Vec<Vec<u8>> = Vec::with_capacity(total_objects);

    let mut kids_refs = String::new();
    for i in 0..page_count {
        if !kids_refs.is_empty() {
            kids_refs.push(' ');
        }
        let _ = write!(kids_refs, "{} 0 R", 4 + 2 * i);
    }

    // 1: Catalog
    objects.push(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    // 2: Pages
    objects.push(format!("<< /Type /Pages /Kids [{kids_refs}] /Count {page_count} >>").into_bytes());
    // 3: Font /F1 (Helvetica)
    objects.push(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec());

    for page_idx in 0..page_count {
        let page = &doc.pages[page_idx];
        let w = page.size.width.get();
        let h = page.size.height.get();
        let raw_bleed = if prepress.bleed_pt.get() > 0.0 { prepress.bleed_pt.get() } else { page.bleed.get().max(0.0) };
        let bleed = if (prepress.crop_marks || prepress.registration_marks) && raw_bleed < 18.0 { 18.0 } else { raw_bleed };
        let media_w = w + 2.0 * bleed;
        let media_h = h + 2.0 * bleed;

        let mut page_dict = format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}]", fmt_num(media_w), fmt_num(media_h));
        if bleed > 0.0 {
            let _ = write!(
                page_dict,
                " /TrimBox [{} {} {} {}] /BleedBox [0 0 {} {}]",
                fmt_num(bleed),
                fmt_num(bleed),
                fmt_num(bleed + w),
                fmt_num(bleed + h),
                fmt_num(media_w),
                fmt_num(media_h)
            );
        }
        let content_obj_num = 5 + 2 * page_idx;
        let _ = write!(page_dict, " /Resources << /Font << /F1 3 0 R >> >> /Contents {content_obj_num} 0 R >>");
        objects.push(page_dict.into_bytes());

        // Build content stream for `page_idx`
        let stream_text = build_page_content_stream(doc, page_idx, w, h, bleed, prepress)?;
        let stream_bytes = stream_text.as_bytes();
        let mut content_obj = format!("<< /Length {} >>\nstream\n", stream_bytes.len()).into_bytes();
        content_obj.extend_from_slice(stream_bytes);
        if !stream_bytes.ends_with(b"\n") {
            content_obj.push(b'\n');
        }
        content_obj.extend_from_slice(b"endstream");
        objects.push(content_obj);
    }

    // Serialize PDF file with exact byte-offset xref table
    let mut out = Vec::with_capacity(4096);
    out.extend_from_slice(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n");

    let mut offsets = Vec::with_capacity(objects.len() + 1);
    offsets.push(0usize); // Object 0 free entry

    for (idx, obj_bytes) in objects.iter().enumerate() {
        let obj_num = idx + 1;
        offsets.push(out.len());
        out.extend_from_slice(format!("{obj_num} 0 obj\n").as_bytes());
        out.extend_from_slice(obj_bytes);
        out.extend_from_slice(b"\nendobj\n");
    }

    let xref_offset = out.len();
    let size = offsets.len();
    out.extend_from_slice(format!("xref\n0 {size}\n").as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for &off in &offsets[1..] {
        // Exactly 20 bytes per entry: `%010d %05d n \n`
        out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }

    out.extend_from_slice(format!("trailer\n<< /Size {size} /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n").as_bytes());

    Ok(out)
}

fn build_page_content_stream(
    doc: &Document,
    page_index: usize,
    w: f64,
    h: f64,
    bleed: f64,
    prepress: &PrepressOptions,
) -> Result<String, RenderError> {
    let layers = doc.effective_layers_for_page(page_index, true)?;
    let page = &doc.pages[page_index];
    let media_h = h + 2.0 * bleed;

    let mut s = String::new();
    s.push_str("q\n");
    // Flip Y-axis so model space (origin top-left, +Y down) maps to PDF user space (origin bottom-left, +Y up)
    let _ = writeln!(s, "1 0 0 -1 0 {} cm", fmt_num(media_h));
    if bleed > 0.0 {
        let _ = writeln!(s, "1 0 0 1 {} {} cm", fmt_num(bleed), fmt_num(bleed));
    }

    // Optional page background fill
    if let Some(bg) = &page.background {
        let page_bbox = Rect::new(0.0, 0.0, w, h)?;
        let page_path = RectSpec::sharp(page_bbox).to_path()?;
        emit_pdf_fill(&mut s, &page_path, page_bbox, bg)?;
    }

    for layer in layers {
        if layer.opacity.get() <= 0.0 {
            continue;
        }
        for node in &layer.objects {
            emit_pdf_node(&mut s, node)?;
        }
    }

    if prepress.crop_marks || prepress.registration_marks {
        s.push_str("q\n1 1 1 1 K\n0.25 w\n");
        if prepress.crop_marks {
            let off = 3.0;
            let len = 12.0;
            // Top-left corner
            let _ = writeln!(s, "{} 0 m {} 0 l 0 {} m 0 {} l S", fmt_num(-len), fmt_num(-off), fmt_num(-len), fmt_num(-off));
            // Top-right corner
            let _ = writeln!(
                s,
                "{} 0 m {} 0 l {} {} m {} {} l S",
                fmt_num(w + off),
                fmt_num(w + len),
                fmt_num(w),
                fmt_num(-len),
                fmt_num(w),
                fmt_num(-off)
            );
            // Bottom-left corner
            let _ = writeln!(
                s,
                "{} {} m {} {} l 0 {} m 0 {} l S",
                fmt_num(-len),
                fmt_num(h),
                fmt_num(-off),
                fmt_num(h),
                fmt_num(h + off),
                fmt_num(h + len)
            );
            // Bottom-right corner
            let _ = writeln!(
                s,
                "{} {} m {} {} l {} {} m {} {} l S",
                fmt_num(w + off),
                fmt_num(h),
                fmt_num(w + len),
                fmt_num(h),
                fmt_num(w),
                fmt_num(h + off),
                fmt_num(w),
                fmt_num(h + len)
            );
        }
        if prepress.registration_marks {
            let targets = [(w * 0.5, -9.0), (w * 0.5, h + 9.0), (-9.0, h * 0.5), (w + 9.0, h * 0.5)];
            for (tx, ty) in targets {
                let r = 3.5;
                let _ = writeln!(
                    s,
                    "{} {} m {} {} l {} {} m {} {} l {} {} {} {} re S",
                    fmt_num(tx - 5.0),
                    fmt_num(ty),
                    fmt_num(tx + 5.0),
                    fmt_num(ty),
                    fmt_num(tx),
                    fmt_num(ty - 5.0),
                    fmt_num(tx),
                    fmt_num(ty + 5.0),
                    fmt_num(tx - r),
                    fmt_num(ty - r),
                    fmt_num(2.0 * r),
                    fmt_num(2.0 * r)
                );
            }
        }
        s.push_str("Q\n");
    }

    s.push_str("Q\n");
    Ok(s)
}

fn emit_pdf_color(s: &mut String, color: &Color, is_stroke: bool) {
    match *color {
        Color::Cmyk { c, m, y, k } => {
            let op = if is_stroke { "K" } else { "k" };
            let _ = writeln!(
                s,
                "{} {} {} {} {op}",
                fmt_num(c.get().clamp(0.0, 1.0)),
                fmt_num(m.get().clamp(0.0, 1.0)),
                fmt_num(y.get().clamp(0.0, 1.0)),
                fmt_num(k.get().clamp(0.0, 1.0))
            );
        }
        Color::Registration => {
            let op = if is_stroke { "K" } else { "k" };
            let _ = writeln!(s, "1 1 1 1 {op}");
        }
        _ => {
            let [r, g, b] = color.to_srgb8();
            let op = if is_stroke { "RG" } else { "rg" };
            let _ = writeln!(s, "{} {} {} {op}", fmt_num(f64::from(r) / 255.0), fmt_num(f64::from(g) / 255.0), fmt_num(f64::from(b) / 255.0));
        }
    }
}

fn emit_pdf_path_ops(s: &mut String, path: &Path) {
    let mut cur = Point::default();
    let mut subpath_start = Point::default();
    for el in &path.elements {
        match *el {
            PathElement::MoveTo(p) => {
                let _ = writeln!(s, "{} {} m", fmt_num(p.x.get()), fmt_num(p.y.get()));
                cur = p;
                subpath_start = p;
            }
            PathElement::LineTo(p) => {
                let _ = writeln!(s, "{} {} l", fmt_num(p.x.get()), fmt_num(p.y.get()));
                cur = p;
            }
            PathElement::QuadTo { ctrl, end } => {
                // Elevate quadratic Bézier (cur, ctrl, end) to exact cubic Bézier (cur, c1, c2, end)
                let c1x = cur.x.get() + (2.0 / 3.0) * (ctrl.x.get() - cur.x.get());
                let c1y = cur.y.get() + (2.0 / 3.0) * (ctrl.y.get() - cur.y.get());
                let c2x = end.x.get() + (2.0 / 3.0) * (ctrl.x.get() - end.x.get());
                let c2y = end.y.get() + (2.0 / 3.0) * (ctrl.y.get() - end.y.get());
                let _ = writeln!(
                    s,
                    "{} {} {} {} {} {} c",
                    fmt_num(c1x),
                    fmt_num(c1y),
                    fmt_num(c2x),
                    fmt_num(c2y),
                    fmt_num(end.x.get()),
                    fmt_num(end.y.get())
                );
                cur = end;
            }
            PathElement::CubicTo { c1, c2, end } => {
                let _ = writeln!(
                    s,
                    "{} {} {} {} {} {} c",
                    fmt_num(c1.x.get()),
                    fmt_num(c1.y.get()),
                    fmt_num(c2.x.get()),
                    fmt_num(c2.y.get()),
                    fmt_num(end.x.get()),
                    fmt_num(end.y.get())
                );
                cur = end;
            }
            PathElement::Close => {
                s.push_str("h\n");
                cur = subpath_start;
            }
        }
    }
}

fn emit_pdf_fill(s: &mut String, path: &Path, bbox: Rect, fill: &FillStyle) -> Result<(), RenderError> {
    let fill_op = if path.fill_rule == FillRule::EvenOdd { "f*" } else { "f" };
    let clip_op = if path.fill_rule == FillRule::EvenOdd { "W*" } else { "W" };

    match fill {
        FillStyle::Uniform { color } => {
            emit_pdf_color(s, color, false);
            emit_pdf_path_ops(s, path);
            let _ = writeln!(s, "{fill_op}");
        }
        FillStyle::Fountain { geometry, blend_mode, stops } => {
            if stops.is_empty() {
                return Ok(());
            }
            // Clip to the object's path and emit 32 deterministic shading bands
            s.push_str("q\n");
            emit_pdf_path_ops(s, path);
            let _ = writeln!(s, "{clip_op} n");

            let bands = 32usize;
            let x0 = bbox.x0.get();
            let y0 = bbox.y0.get();
            let bw = bbox.width().max(1e-6);
            let bh = bbox.height().max(1e-6);

            match *geometry {
                FountainGeometry::Linear { angle_deg, edge_pad } => {
                    let cx = x0 + 0.5 * bw;
                    let cy = y0 + 0.5 * bh;
                    let diag = bw.hypot(bh).max(1e-6);
                    let rad = angle_deg.get().to_radians();
                    let (sin_a, cos_a) = rad.sin_cos();
                    let pad = edge_pad.get().clamp(0.0, 0.49);
                    let half_span = ((0.5 * bw * cos_a.abs() + 0.5 * bh * sin_a.abs()) * (1.0 - 2.0 * pad)).max(1e-6);
                    let span = 2.0 * half_span;

                    s.push_str("q\n");
                    let _ = writeln!(
                        s,
                        "{} {} {} {} {} {} cm",
                        fmt_num(cos_a),
                        fmt_num(sin_a),
                        fmt_num(-sin_a),
                        fmt_num(cos_a),
                        fmt_num(cx),
                        fmt_num(cy)
                    );
                    let half_diag = 0.5 * diag;
                    let step = span / (bands as f64);
                    for i in 0..bands {
                        let t = ((i as f64) + 0.5) / (bands as f64);
                        let rgb = interpolate_stops(stops, *blend_mode, t)?;
                        emit_pdf_color(s, &Color::Rgb { r: rgb[0], g: rgb[1], b: rgb[2] }, false);
                        let bx = -half_span + (i as f64) * step;
                        let _ = writeln!(s, "{} {} {} {} re f", fmt_num(bx), fmt_num(-half_diag), fmt_num(step + 0.05), fmt_num(diag));
                    }
                    s.push_str("Q\n");
                }
                FountainGeometry::Radial { center_x, center_y, radius, .. } | FountainGeometry::Square { center_x, center_y, radius, .. } => {
                    let cx = x0 + center_x.get() * bw;
                    let cy = y0 + center_y.get() * bh;
                    let max_r = (radius.get() * bw.hypot(bh)).max(1e-6);
                    for i in (0..bands).rev() {
                        let t = ((i as f64) + 0.5) / (bands as f64);
                        let r = max_r * ((i + 1) as f64) / (bands as f64);
                        let rgb = interpolate_stops(stops, *blend_mode, t)?;
                        emit_pdf_color(s, &Color::Rgb { r: rgb[0], g: rgb[1], b: rgb[2] }, false);
                        let _ = writeln!(s, "{} {} {} {} re f", fmt_num(cx - r), fmt_num(cy - r), fmt_num(2.0 * r), fmt_num(2.0 * r));
                    }
                }
                FountainGeometry::Conical { center_x, center_y, angle_deg } => {
                    let cx = x0 + center_x.get() * bw;
                    let cy = y0 + center_y.get() * bh;
                    let max_r = bw.hypot(bh).max(1e-6);
                    let base_rad = angle_deg.get().to_radians();
                    for i in 0..bands {
                        let norm0 = (i as f64) / (bands as f64);
                        let norm1 = ((i + 1) as f64) / (bands as f64);
                        let norm_mid = 0.5 * (norm0 + norm1);
                        let t = (1.0 - (2.0 * norm_mid - 1.0).abs()).clamp(0.0, 1.0);
                        let rgb = interpolate_stops(stops, *blend_mode, t)?;
                        emit_pdf_color(s, &Color::Rgb { r: rgb[0], g: rgb[1], b: rgb[2] }, false);
                        let a0 = base_rad + norm0 * std::f64::consts::TAU;
                        let a1 = base_rad + norm1 * std::f64::consts::TAU;
                        let _ = writeln!(
                            s,
                            "{} {} m {} {} l {} {} l h f",
                            fmt_num(cx),
                            fmt_num(cy),
                            fmt_num(cx + max_r * a0.cos()),
                            fmt_num(cy + max_r * a0.sin()),
                            fmt_num(cx + max_r * a1.cos()),
                            fmt_num(cy + max_r * a1.sin())
                        );
                    }
                }
            }
            s.push_str("Q\n");
        }
        FillStyle::TwoColorPattern { bg_color, fg_color, tile_size_pt, .. } => {
            s.push_str("q\n");
            emit_pdf_path_ops(s, path);
            let _ = writeln!(s, "{clip_op} n");
            emit_pdf_color(s, bg_color, false);
            let _ = writeln!(s, "{} {} {} {} re f", fmt_num(bbox.x0.get()), fmt_num(bbox.y0.get()), fmt_num(bbox.width()), fmt_num(bbox.height()));
            emit_pdf_color(s, fg_color, false);
            let ts = tile_size_pt.get().max(2.0);
            let half = ts * 0.5;
            let mut ty = bbox.y0.get();
            while ty < bbox.y1.get() {
                let mut tx = bbox.x0.get();
                while tx < bbox.x1.get() {
                    let _ = writeln!(s, "{} {} {} {} re f", fmt_num(tx), fmt_num(ty), fmt_num(half), fmt_num(half));
                    tx += ts;
                }
                ty += ts;
            }
            s.push_str("Q\n");
        }
        FillStyle::Unknown { .. } => {}
    }
    Ok(())
}

fn emit_pdf_stroke(s: &mut String, path: &Path, outline: &OutlineStyle, obj_transform: Affine2) {
    let mut w = if outline.is_hairline() { 0.25 } else { outline.width_pt.get() };
    if !outline.scale_with_object {
        let scale = obj_transform.determinant().abs().sqrt();
        if scale > 1e-6 {
            w /= scale;
        }
    }
    let cap_code = match outline.cap {
        LineCap::Butt => 0,
        LineCap::Round => 1,
        LineCap::Square => 2,
    };
    let join_code = match outline.join {
        LineJoin::Miter => 0,
        LineJoin::Round => 1,
        LineJoin::Bevel => 2,
    };
    emit_pdf_color(s, &outline.color, true);
    let _ = writeln!(s, "{} w {cap_code} J {join_code} j {} M", fmt_num(w), fmt_num(outline.miter_limit.get()));
    if outline.dash_pattern.is_empty() {
        s.push_str("[] 0 d\n");
    } else {
        let joined: Vec<String> = outline.dash_pattern.iter().map(|d| fmt_num(d.get())).collect();
        let _ = writeln!(s, "[{}] 0 d", joined.join(" "));
    }
    emit_pdf_path_ops(s, path);
    s.push_str("S\n");
}

fn escape_pdf_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for b in text.bytes() {
        match b {
            b'(' => out.push_str("\\("),
            b')' => out.push_str("\\)"),
            b'\\' => out.push_str("\\\\"),
            0x20..=0x7E => out.push(b as char),
            _ => out.push('?'),
        }
    }
    out
}

fn emit_pdf_node(s: &mut String, node: &ObjectNode) -> Result<(), RenderError> {
    if !node.visible || node.opacity.get() <= 0.0 {
        return Ok(());
    }

    s.push_str("q\n");
    if node.transform != Affine2::IDENTITY {
        let _ = writeln!(
            s,
            "{} {} {} {} {} {} cm",
            fmt_num(node.transform.a.get()),
            fmt_num(node.transform.b.get()),
            fmt_num(node.transform.c.get()),
            fmt_num(node.transform.d.get()),
            fmt_num(node.transform.tx.get()),
            fmt_num(node.transform.ty.get())
        );
    }

    match &node.kind {
        ObjectKind::Group { children } => {
            for ch in children {
                emit_pdf_node(s, ch)?;
            }
            s.push_str("Q\n");
            return Ok(());
        }
        ObjectKind::ArtisticText { text, size_pt, position, .. } if node.effects.is_empty() => {
            if !text.is_empty() {
                if let Some(FillStyle::Uniform { color }) = &node.fill {
                    emit_pdf_color(s, color, false);
                } else {
                    s.push_str("0 0 0 rg\n");
                }
                // Because our page CTM flipped Y (`[1 0 0 -1 0 H]`), flip text matrix `[1 0 0 -1 x y] Tm` so glyphs are upright
                let _ = writeln!(
                    s,
                    "BT /F1 {} Tf 1 0 0 -1 {} {} Tm ({}) Tj ET",
                    fmt_num(size_pt.get()),
                    fmt_num(position.x.get()),
                    fmt_num(position.y.get()),
                    escape_pdf_string(text)
                );
            }
            s.push_str("Q\n");
            return Ok(());
        }
        ObjectKind::ParagraphText(pt) => {
            let layout = coralcraft_doc::layout_paragraph_text(pt);
            if !layout.lines.is_empty() {
                if let Some(FillStyle::Uniform { color }) = &node.fill {
                    emit_pdf_color(s, color, false);
                } else {
                    s.push_str("0 0 0 rg\n");
                }
                for line in &layout.lines {
                    let _ = writeln!(
                        s,
                        "BT /F1 {} Tf {} Tw 1 0 0 -1 {} {} Tm ({}) Tj ET",
                        fmt_num(pt.font_size_pt.get()),
                        fmt_num(line.word_spacing_pt),
                        fmt_num(line.origin_x),
                        fmt_num(line.origin_y),
                        escape_pdf_string(&line.text)
                    );
                }
            }
            s.push_str("Q\n");
            return Ok(());
        }
        ObjectKind::Bitmap { pixel_width, pixel_height, dpi_x, dpi_y, rgba_pixels } => {
            let w = (*pixel_width as f64 * 72.0 / dpi_x.get().max(1.0)).max(1.0);
            let h = (*pixel_height as f64 * 72.0 / dpi_y.get().max(1.0)).max(1.0);
            s.push_str("q\n");
            let _ = writeln!(s, "{} 0 0 {} 0 {} cm", fmt_num(w), fmt_num(-h), fmt_num(h));
            s.push_str("BI\n");
            let _ = writeln!(s, "/W {pixel_width}\n/H {pixel_height}\n/CS /DeviceRGB\n/BPC 8\n/F /AHx\nID");
            let total_pixels = (*pixel_width as usize) * (*pixel_height as usize);
            let mut hex = String::with_capacity(total_pixels * 6 + total_pixels / 16);
            for i in 0..total_pixels {
                let off = i * 4;
                if off + 3 <= rgba_pixels.len() {
                    let r = rgba_pixels[off];
                    let g = rgba_pixels[off + 1];
                    let b = rgba_pixels[off + 2];
                    let _ = write!(hex, "{r:02X}{g:02X}{b:02X}");
                } else {
                    hex.push_str("000000");
                }
                if i % 16 == 15 {
                    hex.push('\n');
                }
            }
            hex.push('>');
            s.push_str(&hex);
            s.push_str("\nEI\nQ\n");
            s.push_str("Q\n");
            return Ok(());
        }
        ObjectKind::Unsupported(u) => {
            if let Some(bbox) = u.fallback_bounds {
                let path = RectSpec::sharp(bbox).to_path()?;
                emit_pdf_geometry_and_powerclip(s, node, &path, bbox)?;
            }
            for ch in &u.children {
                emit_pdf_node(s, ch)?;
            }
            s.push_str("Q\n");
            return Ok(());
        }
        _ => {}
    }

    if let Some((mut local_path, mut local_bbox)) = crate::extract_local_path_and_bbox(&node.kind)? {
        crate::eps::apply_deforming_effects(node, &mut local_path, &mut local_bbox);
        emit_pdf_geometry_and_powerclip(s, node, &local_path, local_bbox)?;
    }

    s.push_str("Q\n");
    Ok(())
}

fn emit_pdf_geometry_and_powerclip(s: &mut String, node: &ObjectNode, local_path: &Path, local_bbox: Rect) -> Result<(), RenderError> {
    let behind_fill = node.outline.as_ref().is_some_and(|o| o.behind_fill);

    // 1. Outline behind fill
    if behind_fill && let Some(outline) = &node.outline {
        emit_pdf_stroke(s, local_path, outline, node.transform);
    }

    // 2. Fill
    if let Some(fill) = &node.fill {
        emit_pdf_fill(s, local_path, local_bbox, fill)?;
    }

    // 3. PowerClip children clipped by container path (`W n` / `W* n`)
    if !node.powerclip_children.is_empty() {
        let clip_op = if local_path.fill_rule == FillRule::EvenOdd { "W*" } else { "W" };
        s.push_str("q\n");
        emit_pdf_path_ops(s, local_path);
        let _ = writeln!(s, "{clip_op} n");
        for pc in &node.powerclip_children {
            emit_pdf_node(s, pc)?;
        }
        s.push_str("Q\n");
    }

    // 4. Outline in front of fill/PowerClip
    if !behind_fill && let Some(outline) = &node.outline {
        emit_pdf_stroke(s, local_path, outline, node.transform);
    }

    Ok(())
}
