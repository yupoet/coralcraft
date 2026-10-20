//! Clean-room Encapsulated PostScript (`EPSF-3.0`) and AutoCAD `DXF` (`AC1009`) vector exporters
//! for `coralcraft-render`.

use std::fmt::Write as _;

use coralcraft_color::Color;
use coralcraft_doc::{Document, FillStyle, LineCap, LineJoin, ObjectKind, ObjectNode, OutlineStyle, VectorEffect};
use coralcraft_geom::{Affine2, FillRule, FlattenOpts, Path, PathElement, Point, Rect, RectSpec, flatten_path};

use crate::extract_local_path_and_bbox;
use crate::surface::RenderError;
use crate::svg::fmt_num;

fn sanitize_ps_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '(' => out.push_str("\\("),
            ')' => out.push_str("\\)"),
            '\\' => out.push_str("\\\\"),
            '\n' | '\r' | '\t' => out.push(' '),
            c if c.is_ascii_graphic() || c == ' ' => out.push(c),
            _ => out.push('?'),
        }
    }
    out
}

fn sanitize_dxf_name(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-') {
            out.push(ch);
        } else if !matches!(ch, '\n' | '\r') {
            out.push('_');
        }
    }
    if out.is_empty() { "0".to_owned() } else { out }
}

fn sanitize_dxf_text(s: &str) -> String {
    s.chars().filter(|&ch| ch != '\n' && ch != '\r').collect()
}

fn ps_font_name(font_family: &str) -> &'static str {
    let lower = font_family.to_ascii_lowercase();
    if lower.contains("times") || lower.contains("serif") {
        "Times-Roman"
    } else if lower.contains("courier") || lower.contains("mono") {
        "Courier"
    } else {
        "Helvetica"
    }
}

fn transform_scale(tf: Affine2) -> f64 {
    let a = tf.a.get();
    let b = tf.b.get();
    let c = tf.c.get();
    let d = tf.d.get();
    let s = ((a * a + b * b + c * c + d * d) * 0.5).sqrt();
    if s.is_finite() && s > 0.0 { s } else { 1.0 }
}

fn node_fill_color(node: &ObjectNode) -> Option<&Color> {
    node.fill.as_ref().and_then(|fill| match fill {
        FillStyle::Uniform { color } => Some(color),
        FillStyle::Fountain { stops, .. } => stops.first().map(|s| &s.color),
        FillStyle::TwoColorPattern { fg_color, .. } => Some(fg_color),
        FillStyle::Unknown { .. } => None,
    })
}

/// Exports `page_index` (0-based) of `doc` as a clean-room Encapsulated PostScript (`%!PS-Adobe-3.0 EPSF-3.0`) document.
pub fn export_eps(doc: &Document, page_index: usize) -> Result<String, RenderError> {
    let layers = doc.effective_layers_for_page(page_index, true)?;
    let page = doc.pages.get(page_index).ok_or(coralcraft_doc::DocError::PageOutOfRange { index: page_index, count: doc.pages.len() })?;
    let w = page.size.width.get();
    let h = page.size.height.get();

    let mut out = String::new();
    out.push_str("%!PS-Adobe-3.0 EPSF-3.0\n");
    let _ = writeln!(out, "%%Title: ({})", sanitize_ps_text(&page.name));
    out.push_str("%%Creator: (CoralCraft Clean-Room EPS Exporter)\n");
    let _ = writeln!(out, "%%BoundingBox: 0 0 {} {}", w.ceil() as i64, h.ceil() as i64);
    let _ = writeln!(out, "%%HiResBoundingBox: 0 0 {} {}", fmt_num(w), fmt_num(h));
    out.push_str("%%LanguageLevel: 2\n");
    out.push_str("%%EndComments\n");
    out.push_str("gsave\n");

    for layer in layers {
        if !layer.visible || layer.opacity.get() <= 0.0 {
            continue;
        }
        let _ = writeln!(out, "% Layer: {}", sanitize_ps_text(&layer.name));
        for node in &layer.objects {
            emit_eps_node(&mut out, node, Affine2::IDENTITY, h)?;
        }
    }

    out.push_str("grestore\n");
    out.push_str("showpage\n");
    out.push_str("%%EOF\n");
    if out.len() > crate::pdf::MAX_PDF_EXPORT_BYTES {
        return Err(RenderError::ExportOutputTooLarge { bytes: out.len(), max: crate::pdf::MAX_PDF_EXPORT_BYTES });
    }
    Ok(out)
}

fn emit_eps_node(out: &mut String, node: &ObjectNode, parent_tf: Affine2, page_h: f64) -> Result<(), RenderError> {
    if !node.visible || node.opacity.get() <= 0.0 {
        return Ok(());
    }
    let world_tf = node.transform.then(parent_tf)?;
    if let ObjectKind::Group { children } = &node.kind {
        for ch in children {
            emit_eps_node(out, ch, world_tf, page_h)?;
        }
        return Ok(());
    }
    if node.effects.is_empty() {
        match &node.kind {
            ObjectKind::ArtisticText { text, font_family, size_pt, position } => {
                if !text.is_empty() {
                    let col = node_fill_color(node).unwrap_or(&Color::BLACK);
                    emit_ps_color(out, col);
                    let scaled_sz = (size_pt.get() * transform_scale(world_tf)).max(0.1);
                    let _ = writeln!(out, "/{} findfont {} scalefont setfont", ps_font_name(font_family), fmt_num(scaled_sz));
                    let world_pt = world_tf.apply_point(*position)?;
                    let _ = writeln!(
                        out,
                        "{} {} moveto\n({}) show",
                        fmt_num(world_pt.x.get()),
                        fmt_num(page_h - world_pt.y.get()),
                        sanitize_ps_text(text)
                    );
                }
                return Ok(());
            }
            ObjectKind::ParagraphText(pt) => {
                let layout = coralcraft_doc::layout_paragraph_text(pt);
                if !layout.lines.is_empty() {
                    let col = node_fill_color(node).unwrap_or(&Color::BLACK);
                    emit_ps_color(out, col);
                    let scaled_sz = (pt.font_size_pt.get() * transform_scale(world_tf)).max(0.1);
                    let _ = writeln!(out, "/{} findfont {} scalefont setfont", ps_font_name(&pt.font_family), fmt_num(scaled_sz));
                    for line in &layout.lines {
                        if let Ok(local_pt) = Point::new(line.origin_x, line.origin_y)
                            && let Ok(world_pt) = world_tf.apply_point(local_pt)
                        {
                            let _ = writeln!(
                                out,
                                "{} {} moveto\n({}) show",
                                fmt_num(world_pt.x.get()),
                                fmt_num(page_h - world_pt.y.get()),
                                sanitize_ps_text(&line.text)
                            );
                        }
                    }
                }
                return Ok(());
            }
            ObjectKind::Bitmap { pixel_width, pixel_height, dpi_x, dpi_y, rgba_pixels } => {
                let pw = *pixel_width as usize;
                let ph = *pixel_height as usize;
                let needed = pw.saturating_mul(ph).saturating_mul(4);
                if pw > 0
                    && ph > 0
                    && rgba_pixels.len() >= needed
                    && let Some(pixel_slice) = rgba_pixels.get(..needed)
                {
                    let w_pt = (*pixel_width as f64 * 72.0 / dpi_x.get().max(1.0)).max(1.0);
                    let h_pt = (*pixel_height as f64 * 72.0 / dpi_y.get().max(1.0)).max(1.0);
                    let local_rect = coralcraft_geom::Rect::new(0.0, 0.0, w_pt, h_pt)?;
                    let wb = world_tf.apply_rect_bbox(local_rect).unwrap_or(local_rect);
                    let x0 = wb.x0.get();
                    let y0_ps = page_h - wb.y1.get();
                    let bw = wb.width().max(1.0);
                    let bh = wb.height().max(1.0);
                    let op = node.opacity.get().clamp(0.0, 1.0);
                    let _ = writeln!(
                        out,
                        "gsave\n% Bitmap\n{} {} translate\n{} {} scale\n{pw} {ph} 8 [{pw} 0 0 -{ph} 0 {ph}]\ncurrentfile /ASCIIHexDecode filter false 3 colorimage",
                        fmt_num(x0),
                        fmt_num(y0_ps),
                        fmt_num(bw),
                        fmt_num(bh)
                    );
                    let (chunks, _) = pixel_slice.as_chunks::<4>();
                    for &[r, g, b, a] in chunks {
                        let eff_a = (f64::from(a) * op).round().clamp(0.0, 255.0) as u32;
                        let inv_a = 255u32.saturating_sub(eff_a);
                        let ro = (u32::from(r).saturating_mul(eff_a).saturating_add(255u32.saturating_mul(inv_a)).saturating_add(127) / 255) as u8;
                        let go = (u32::from(g).saturating_mul(eff_a).saturating_add(255u32.saturating_mul(inv_a)).saturating_add(127) / 255) as u8;
                        let bo = (u32::from(b).saturating_mul(eff_a).saturating_add(255u32.saturating_mul(inv_a)).saturating_add(127) / 255) as u8;
                        let _ = write!(out, "{ro:02x}{go:02x}{bo:02x}");
                    }
                    out.push_str(">\ngrestore\n");
                }
                return Ok(());
            }
            _ => {}
        }
    }
    let Some((mut local_path, mut local_bbox)) = extract_local_path_and_bbox(&node.kind)? else {
        return Ok(());
    };
    apply_deforming_effects(node, &mut local_path, &mut local_bbox);
    let world_path = local_path.transform(world_tf)?;

    let emit_ps_path = |buf: &mut String| {
        buf.push_str("newpath\n");
        let mut cur = Point::ZERO;
        for &el in &world_path.elements {
            match el {
                PathElement::MoveTo(p) => {
                    let _ = writeln!(buf, "{} {} moveto", fmt_num(p.x.get()), fmt_num(page_h - p.y.get()));
                    cur = p;
                }
                PathElement::LineTo(p) => {
                    let _ = writeln!(buf, "{} {} lineto", fmt_num(p.x.get()), fmt_num(page_h - p.y.get()));
                    cur = p;
                }
                PathElement::QuadTo { ctrl, end } => {
                    let (c1, c2) = coralcraft_geom::elevate_quad_to_cubic(cur, ctrl, end).unwrap_or((ctrl, ctrl));
                    let _ = writeln!(
                        buf,
                        "{} {} {} {} {} {} curveto",
                        fmt_num(c1.x.get()),
                        fmt_num(page_h - c1.y.get()),
                        fmt_num(c2.x.get()),
                        fmt_num(page_h - c2.y.get()),
                        fmt_num(end.x.get()),
                        fmt_num(page_h - end.y.get())
                    );
                    cur = end;
                }
                PathElement::CubicTo { c1, c2, end } => {
                    let _ = writeln!(
                        buf,
                        "{} {} {} {} {} {} curveto",
                        fmt_num(c1.x.get()),
                        fmt_num(page_h - c1.y.get()),
                        fmt_num(c2.x.get()),
                        fmt_num(page_h - c2.y.get()),
                        fmt_num(end.x.get()),
                        fmt_num(page_h - end.y.get())
                    );
                    cur = end;
                }
                PathElement::Close => {
                    buf.push_str("closepath\n");
                }
            }
        }
    };

    let behind_fill = node.outline.as_ref().is_some_and(|o| o.behind_fill);

    if behind_fill && let Some(outline) = &node.outline {
        emit_eps_stroke(out, &emit_ps_path, &world_path, outline, world_tf, page_h);
    }

    if let Some(fill) = &node.fill {
        match fill {
            FillStyle::Fountain { geometry, blend_mode, stops } if !stops.is_empty() => {
                out.push_str("gsave\n");
                emit_ps_path(out);
                if world_path.fill_rule == FillRule::EvenOdd {
                    out.push_str("eoclip\nnewpath\n");
                } else {
                    out.push_str("clip\nnewpath\n");
                }
                let wb = world_path.bounds().unwrap_or(local_bbox);
                let x0 = wb.x0.get();
                let y0_ps = page_h - wb.y1.get();
                let bw = wb.width().max(1e-6);
                let bh = wb.height().max(1e-6);
                let bands = 16usize;
                match *geometry {
                    coralcraft_doc::FountainGeometry::Linear { angle_deg, edge_pad } => {
                        let cx = x0 + 0.5 * bw;
                        let cy_ps = y0_ps + 0.5 * bh;
                        let diag = bw.hypot(bh).max(1e-6);
                        let rad = -angle_deg.get().to_radians();
                        let (sin_a, cos_a) = rad.sin_cos();
                        let pad = edge_pad.get().clamp(0.0, 0.49);
                        let half_span = ((0.5 * bw * cos_a.abs() + 0.5 * bh * sin_a.abs()) * (1.0 - 2.0 * pad)).max(1e-6);
                        let span = 2.0 * half_span;
                        let step = span / (bands as f64);
                        let _ = writeln!(out, "{} {} translate\n{} rotate", fmt_num(cx), fmt_num(cy_ps), fmt_num(-angle_deg.get()));
                        for i in 0..bands {
                            let t = ((i as f64) + 0.5) / (bands as f64);
                            let rgb = coralcraft_color::interpolate_stops(stops, *blend_mode, t)?;
                            emit_ps_color(out, &Color::Rgb { r: rgb[0], g: rgb[1], b: rgb[2] });
                            let bx = -half_span + (i as f64) * step;
                            let _ = writeln!(out, "{} {} {} {} rectfill", fmt_num(bx), fmt_num(-0.5 * diag), fmt_num(step + 0.05), fmt_num(diag));
                        }
                    }
                    coralcraft_doc::FountainGeometry::Radial { center_x, center_y, radius, .. }
                    | coralcraft_doc::FountainGeometry::Square { center_x, center_y, radius, .. }
                    | coralcraft_doc::FountainGeometry::Conical { center_x, center_y, angle_deg: radius } => {
                        let cx = x0 + center_x.get() * bw;
                        let cy_ps = page_h - (wb.y0.get() + center_y.get() * bh);
                        let max_r = (radius.get().abs().max(0.5) * bw.hypot(bh)).max(1e-6);
                        for i in (0..bands).rev() {
                            let t = ((i as f64) + 0.5) / (bands as f64);
                            let r = max_r * (i.saturating_add(1) as f64) / (bands as f64);
                            let rgb = coralcraft_color::interpolate_stops(stops, *blend_mode, t)?;
                            emit_ps_color(out, &Color::Rgb { r: rgb[0], g: rgb[1], b: rgb[2] });
                            let _ = writeln!(out, "{} {} {} {} rectfill", fmt_num(cx - r), fmt_num(cy_ps - r), fmt_num(2.0 * r), fmt_num(2.0 * r));
                        }
                    }
                }
                out.push_str("grestore\n");
            }
            FillStyle::TwoColorPattern { kind, fg_color, bg_color, tile_size_pt } => {
                out.push_str("gsave\n% TwoColorPattern\n");
                emit_ps_path(out);
                if world_path.fill_rule == FillRule::EvenOdd {
                    out.push_str("eoclip\nnewpath\n");
                } else {
                    out.push_str("clip\nnewpath\n");
                }
                let wb = world_path.bounds().unwrap_or(local_bbox);
                let x0 = wb.x0.get();
                let y0_ps = page_h - wb.y1.get();
                let bw = wb.width().max(1e-6);
                let bh = wb.height().max(1e-6);
                emit_ps_color(out, bg_color);
                let _ = writeln!(out, "{} {} {} {} rectfill", fmt_num(x0), fmt_num(y0_ps), fmt_num(bw), fmt_num(bh));
                emit_ps_color(out, fg_color);
                let tile = tile_size_pt.get().max(2.0);
                let cols = ((bw / tile).ceil() as usize).clamp(1, 32);
                let rows = ((bh / tile).ceil() as usize).clamp(1, 32);
                let half = 0.5 * tile;
                for r in 0..rows {
                    let ty = y0_ps + (r as f64) * tile;
                    for c in 0..cols {
                        let tx = x0 + (c as f64) * tile;
                        match kind {
                            coralcraft_doc::PatternKind::Checkerboard | coralcraft_doc::PatternKind::Unknown => {
                                let _ = writeln!(out, "{} {} {} {} rectfill", fmt_num(tx), fmt_num(ty), fmt_num(half), fmt_num(half));
                                let _ = writeln!(out, "{} {} {} {} rectfill", fmt_num(tx + half), fmt_num(ty + half), fmt_num(half), fmt_num(half));
                            }
                            coralcraft_doc::PatternKind::PolkaDot => {
                                let q = 0.25 * tile;
                                let _ = writeln!(out, "{} {} {} {} rectfill", fmt_num(tx + q), fmt_num(ty + q), fmt_num(half), fmt_num(half));
                            }
                            coralcraft_doc::PatternKind::HorizontalStripe => {
                                let _ = writeln!(out, "{} {} {} {} rectfill", fmt_num(tx), fmt_num(ty), fmt_num(tile), fmt_num(half));
                            }
                            coralcraft_doc::PatternKind::VerticalStripe => {
                                let _ = writeln!(out, "{} {} {} {} rectfill", fmt_num(tx), fmt_num(ty), fmt_num(half), fmt_num(tile));
                            }
                            coralcraft_doc::PatternKind::DiagonalCrosshatch => {
                                let _ = writeln!(
                                    out,
                                    "{} setlinewidth\nnewpath\n{} {} moveto\n{} {} lineto\n{} {} moveto\n{} {} lineto\nstroke",
                                    fmt_num((0.12 * tile).max(0.24)),
                                    fmt_num(tx),
                                    fmt_num(ty),
                                    fmt_num(tx + tile),
                                    fmt_num(ty + tile),
                                    fmt_num(tx),
                                    fmt_num(ty + tile),
                                    fmt_num(tx + tile),
                                    fmt_num(ty)
                                );
                            }
                        }
                    }
                }
                out.push_str("grestore\n");
            }
            FillStyle::Uniform { color } => {
                emit_ps_color(out, color);
                emit_ps_path(out);
                if world_path.fill_rule == FillRule::EvenOdd {
                    out.push_str("eofill\n");
                } else {
                    out.push_str("fill\n");
                }
            }
            FillStyle::Fountain { .. } | FillStyle::Unknown { .. } => {}
        }
    }

    if !node.powerclip_children.is_empty() {
        out.push_str("gsave\n");
        emit_ps_path(out);
        if world_path.fill_rule == FillRule::EvenOdd {
            out.push_str("eoclip\nnewpath\n");
        } else {
            out.push_str("clip\nnewpath\n");
        }
        for pc in &node.powerclip_children {
            emit_eps_node(out, pc, world_tf, page_h)?;
        }
        out.push_str("grestore\n");
    }

    if !behind_fill && let Some(outline) = &node.outline {
        emit_eps_stroke(out, &emit_ps_path, &world_path, outline, world_tf, page_h);
    }

    Ok(())
}

fn emit_eps_stroke(out: &mut String, emit_ps_path: &dyn Fn(&mut String), world_path: &Path, outline: &OutlineStyle, world_tf: Affine2, page_h: f64) {
    let mut w = if outline.is_hairline() { 0.24 } else { outline.width_pt.get().max(0.1) };
    if outline.scale_with_object && !outline.is_hairline() {
        let a = world_tf.a.get();
        let b = world_tf.b.get();
        let c = world_tf.c.get();
        let d = world_tf.d.get();
        let scale = ((a * a + b * b + c * c + d * d) * 0.5).sqrt();
        if scale.is_finite() && scale > 0.0 {
            w *= scale;
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
    emit_ps_color(out, &outline.color);
    let _ = writeln!(out, "{} setlinewidth\n{cap_code} setlinecap\n{join_code} setlinejoin", fmt_num(w));
    if outline.dash_pattern.is_empty() {
        out.push_str("[] 0 setdash\n");
    } else {
        let joined: Vec<String> = outline.dash_pattern.iter().map(|d| fmt_num(d.get())).collect();
        let _ = writeln!(out, "[{}] 0 setdash", joined.join(" "));
    }
    emit_ps_path(out);
    out.push_str("stroke\n");

    if (outline.start_arrow != coralcraft_doc::ArrowheadKind::None || outline.end_arrow != coralcraft_doc::ArrowheadKind::None)
        && let Ok(polys) = flatten_path(world_path, &FlattenOpts::default())
        && let Ok(arrow_polys) = crate::build_arrowhead_polylines(&polys, 0.5 * w, outline.start_arrow, outline.end_arrow)
        && !arrow_polys.is_empty()
    {
        out.push_str("% Arrowheads\n");
        for ap in &arrow_polys {
            if let Some(&first) = ap.points.first() {
                let _ = writeln!(out, "newpath\n{} {} moveto", fmt_num(first.x.get()), fmt_num(page_h - first.y.get()));
                for &pt in ap.points.iter().skip(1) {
                    let _ = writeln!(out, "{} {} lineto", fmt_num(pt.x.get()), fmt_num(page_h - pt.y.get()));
                }
                out.push_str("closepath\nfill\n");
            }
        }
    }
}

fn emit_ps_color(out: &mut String, color: &Color) {
    match *color {
        Color::Cmyk { c, m, y, k } => {
            let _ = writeln!(out, "{} {} {} {} setcmykcolor", fmt_num(c.get()), fmt_num(m.get()), fmt_num(y.get()), fmt_num(k.get()));
        }
        Color::Registration => {
            out.push_str("1 1 1 1 setcmykcolor\n");
        }
        _ => {
            let [r, g, b] = color.to_srgb8();
            let _ =
                writeln!(out, "{} {} {} setrgbcolor", fmt_num(f64::from(r) / 255.0), fmt_num(f64::from(g) / 255.0), fmt_num(f64::from(b) / 255.0));
        }
    }
}

/// Exports `page_index` (0-based) of `doc` as a clean-room AutoCAD ASCII DXF (`AC1009` / R12) string.
pub fn export_dxf(doc: &Document, page_index: usize) -> Result<String, RenderError> {
    let layers: Vec<&coralcraft_doc::Layer> = doc
        .effective_layers_for_page(page_index, false)?
        .into_iter()
        .filter(|l| l.printable || l.role != coralcraft_doc::LayerRole::Desktop || !l.objects.is_empty())
        .collect();
    let page = doc.pages.get(page_index).ok_or(coralcraft_doc::DocError::PageOutOfRange { index: page_index, count: doc.pages.len() })?;
    let page_w = page.size.width.get();
    let page_h = page.size.height.get();

    let mut ltypes: Vec<Vec<coralcraft_geom::F64>> = Vec::new();
    for layer in &layers {
        if !layer.visible || layer.opacity.get() <= 0.0 {
            continue;
        }
        for node in &layer.objects {
            collect_dxf_ltypes_recursive(node, &mut ltypes);
        }
    }

    let mut blocks_body = String::new();
    for layer in &layers {
        if !layer.visible || layer.opacity.get() <= 0.0 {
            continue;
        }
        let safe_layer_name = sanitize_dxf_name(&layer.name);
        for node in &layer.objects {
            collect_dxf_blocks_recursive(&mut blocks_body, node, &safe_layer_name, page_h, &ltypes)?;
        }
    }

    let mut out = String::new();
    let _ = write!(
        out,
        "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1009\n9\n$LIMMIN\n10\n0\n20\n0\n9\n$LIMMAX\n10\n{}\n20\n{}\n",
        fmt_num(page_w),
        fmt_num(page_h)
    );
    let doc_meta =
        coralcraft_doc::DxfDocMeta { title: doc.title.clone(), units: doc.units, palette: doc.palette.clone(), residual: doc.residual.clone() };
    if let Some(b64) = coralcraft_doc::encode_interchange_meta(&doc_meta) {
        emit_dxf_header_meta(&mut out, "$CORALCRAFT_DOC", &b64);
    }
    let page_meta = coralcraft_doc::DxfPageMeta {
        name: page.name.clone(),
        bleed: page.bleed,
        background: page.background.clone(),
        residual: page.residual.clone(),
    };
    if let Some(b64) = coralcraft_doc::encode_interchange_meta(&page_meta) {
        emit_dxf_header_meta(&mut out, "$CORALCRAFT_PAGE", &b64);
    }
    out.push_str("0\nENDSEC\n0\nSECTION\n2\nTABLES\n");
    if !ltypes.is_empty() {
        let _ = writeln!(out, "0\nTABLE\n2\nLTYPE\n70\n{}", ltypes.len());
        for (idx, pat) in ltypes.iter().enumerate() {
            let name = format!("DASH_{}", idx.saturating_add(1));
            let total_len: f64 = pat.iter().map(|d| d.get().abs()).sum();
            let _ = writeln!(out, "0\nLTYPE\n2\n{name}\n70\n0\n3\nDashed line\n72\n65\n73\n{}\n40\n{}", pat.len(), fmt_num(total_len));
            for (di, d) in pat.iter().enumerate() {
                let signed = if di.is_multiple_of(2) { d.get().abs() } else { -d.get().abs() };
                let _ = writeln!(out, "49\n{}", fmt_num(signed));
            }
        }
        out.push_str("0\nENDTAB\n");
    }
    let _ = writeln!(out, "0\nTABLE\n2\nLAYER\n70\n{}", layers.len());
    for layer in &layers {
        let safe_layer_name = sanitize_dxf_name(&layer.name);
        let flags = if layer.locked { 4 } else { 0 };
        let is_master = doc.master_layers.iter().any(|ml| std::ptr::eq(ml, *layer));
        let _ = writeln!(out, "0\nLAYER\n2\n{safe_layer_name}\n70\n{flags}\n62\n7\n6\nCONTINUOUS");
        let lmeta = coralcraft_doc::DxfLayerMeta {
            name: layer.name.clone(),
            role: layer.role,
            visible: layer.visible,
            printable: layer.printable,
            locked: layer.locked,
            master_in_front: layer.master_in_front,
            opacity: layer.opacity,
            residual: layer.residual.clone(),
            is_master,
        };
        if let Some(b64) = coralcraft_doc::encode_interchange_meta(&lmeta) {
            emit_dxf_xdata(&mut out, "L:", &b64);
        }
    }
    out.push_str("0\nENDTAB\n0\nENDSEC\n");
    if !blocks_body.is_empty() {
        out.push_str("0\nSECTION\n2\nBLOCKS\n");
        out.push_str(&blocks_body);
        out.push_str("0\nENDSEC\n");
    }
    out.push_str("0\nSECTION\n2\nENTITIES\n");

    for layer in layers {
        if !layer.visible || layer.opacity.get() <= 0.0 {
            continue;
        }
        let safe_layer_name = sanitize_dxf_name(&layer.name);
        for node in &layer.objects {
            if !node.visible || node.opacity.get() <= 0.0 {
                continue;
            }
            let mut node_dxf = String::new();
            emit_dxf_node(&mut node_dxf, node, Affine2::IDENTITY, &safe_layer_name, page_h, &ltypes)?;
            if let Some(node_b64) = coralcraft_doc::encode_interchange_meta(node) {
                emit_dxf_node_with_xdata(&mut out, &node_dxf, &safe_layer_name, &node_b64);
            } else {
                out.push_str(&node_dxf);
            }
        }
    }

    out.push_str("0\nENDSEC\n0\nEOF\n");
    if out.len() > crate::pdf::MAX_PDF_EXPORT_BYTES {
        return Err(RenderError::ExportOutputTooLarge { bytes: out.len(), max: crate::pdf::MAX_PDF_EXPORT_BYTES });
    }
    Ok(out)
}

fn emit_dxf_header_meta(out: &mut String, var_name: &str, b64: &str) {
    let _ = writeln!(out, "9\n{var_name}");
    for chunk in b64.as_bytes().chunks(200) {
        if let Ok(s) = std::str::from_utf8(chunk) {
            let _ = writeln!(out, "1\n{s}");
        }
    }
}

fn emit_dxf_xdata(out: &mut String, prefix: &str, b64: &str) {
    out.push_str("1001\nCORALCRAFT\n");
    let full = format!("{prefix}{b64}");
    for chunk in full.as_bytes().chunks(200) {
        if let Ok(s) = std::str::from_utf8(chunk) {
            let _ = writeln!(out, "1000\n{s}");
        }
    }
}

fn emit_dxf_node_with_xdata(out: &mut String, node_dxf: &str, safe_layer_name: &str, node_b64: &str) {
    if node_dxf.is_empty() {
        let _ = writeln!(out, "0\nPOINT\n8\n{safe_layer_name}\n10\n0\n20\n0");
        emit_dxf_xdata(out, "N:", node_b64);
        return;
    }
    let lines: Vec<&str> = node_dxf.lines().collect();
    let mut in_entity_header = false;
    let mut is_first_entity = true;
    let mut idx = 0usize;
    while let (Some(&code_line), Some(&val_line)) = (lines.get(idx), lines.get(idx.saturating_add(1))) {
        if code_line.trim() == "0" {
            if in_entity_header {
                if is_first_entity {
                    emit_dxf_xdata(out, "N:", node_b64);
                    is_first_entity = false;
                } else {
                    emit_dxf_xdata(out, "", "SKIP");
                }
                in_entity_header = false;
            }
            let v = val_line.trim();
            if v != "VERTEX" && v != "SEQEND" {
                in_entity_header = true;
            }
        }
        let _ = writeln!(out, "{code_line}\n{val_line}");
        idx = idx.saturating_add(2);
    }
    if in_entity_header {
        if is_first_entity {
            emit_dxf_xdata(out, "N:", node_b64);
        } else {
            emit_dxf_xdata(out, "", "SKIP");
        }
    }
}

fn try_decompose_dxf_insert_transform(t: Affine2) -> Option<(f64, f64, f64, f64, f64)> {
    let sx = t.a.get().hypot(t.b.get());
    if sx <= 1e-6 {
        return None;
    }
    let theta = t.b.get().atan2(t.a.get());
    let (sin_t, cos_t) = theta.sin_cos();
    let shear = t.c.get() * cos_t + t.d.get() * sin_t;
    if shear.abs() > 1e-4 {
        return None;
    }
    let sy = t.d.get() * cos_t - t.c.get() * sin_t;
    if sy.abs() <= 1e-6 {
        return None;
    }
    let screen_deg = theta.to_degrees();
    let dxf_rot_deg = (-screen_deg).rem_euclid(360.0);
    Some((sx, sy, dxf_rot_deg, t.tx.get(), t.ty.get()))
}

fn collect_dxf_blocks_recursive(
    blocks_out: &mut String,
    node: &ObjectNode,
    layer_name: &str,
    page_h: f64,
    ltypes: &[Vec<coralcraft_geom::F64>],
) -> Result<(), RenderError> {
    if !node.visible || node.opacity.get() <= 0.0 {
        return Ok(());
    }
    if node.effects.is_empty()
        && let ObjectKind::Group { children } = &node.kind
        && !children.is_empty()
        && try_decompose_dxf_insert_transform(node.transform).is_some()
    {
        for ch in children {
            collect_dxf_blocks_recursive(blocks_out, ch, layer_name, page_h, ltypes)?;
        }
        let bname = format!("GRP_{}", node.id.0);
        let _ = writeln!(blocks_out, "0\nBLOCK\n8\n{layer_name}\n2\n{bname}\n70\n0\n10\n0\n20\n{}", crate::svg::fmt_num_exact(page_h));
        for ch in children {
            emit_dxf_node(blocks_out, ch, Affine2::IDENTITY, layer_name, page_h, ltypes)?;
        }
        let _ = writeln!(blocks_out, "0\nENDBLK\n8\n{layer_name}");
    }
    Ok(())
}

fn try_extract_cubic_spline_pts(path: &Path) -> Option<(Vec<Point>, bool)> {
    let Some(PathElement::MoveTo(start)) = path.elements.first().copied() else {
        return None;
    };
    let mut ctrl_pts = vec![start];
    let mut cur = start;
    let mut closed = false;
    let mut saw_curve = false;
    for (idx, el) in path.elements.iter().enumerate().skip(1) {
        if closed {
            return None;
        }
        match *el {
            PathElement::MoveTo(_) => return None,
            PathElement::LineTo(end) => {
                let c1 = Point::new(cur.x.get() + (end.x.get() - cur.x.get()) / 3.0, cur.y.get() + (end.y.get() - cur.y.get()) / 3.0).ok()?;
                let c2 =
                    Point::new(cur.x.get() + 2.0 * (end.x.get() - cur.x.get()) / 3.0, cur.y.get() + 2.0 * (end.y.get() - cur.y.get()) / 3.0).ok()?;
                ctrl_pts.push(c1);
                ctrl_pts.push(c2);
                ctrl_pts.push(end);
                cur = end;
            }
            PathElement::QuadTo { ctrl, end } => {
                saw_curve = true;
                let c1 = Point::new(cur.x.get() + 2.0 * (ctrl.x.get() - cur.x.get()) / 3.0, cur.y.get() + 2.0 * (ctrl.y.get() - cur.y.get()) / 3.0)
                    .ok()?;
                let c2 = Point::new(end.x.get() + 2.0 * (ctrl.x.get() - end.x.get()) / 3.0, end.y.get() + 2.0 * (ctrl.y.get() - end.y.get()) / 3.0)
                    .ok()?;
                ctrl_pts.push(c1);
                ctrl_pts.push(c2);
                ctrl_pts.push(end);
                cur = end;
            }
            PathElement::CubicTo { c1, c2, end } => {
                saw_curve = true;
                ctrl_pts.push(c1);
                ctrl_pts.push(c2);
                ctrl_pts.push(end);
                cur = end;
            }
            PathElement::Close => {
                if idx.saturating_add(1) != path.elements.len() {
                    return None;
                }
                closed = true;
            }
        }
    }
    if saw_curve && ctrl_pts.len() >= 4 { Some((ctrl_pts, closed)) } else { None }
}

fn collect_dxf_ltypes_recursive(node: &ObjectNode, ltypes: &mut Vec<Vec<coralcraft_geom::F64>>) {
    if !node.visible || node.opacity.get() <= 0.0 {
        return;
    }
    if let Some(outline) = &node.outline
        && !outline.dash_pattern.is_empty()
        && !ltypes.contains(&outline.dash_pattern)
    {
        ltypes.push(outline.dash_pattern.clone());
    }
    if let ObjectKind::Group { children } = &node.kind {
        for ch in children {
            collect_dxf_ltypes_recursive(ch, ltypes);
        }
    }
    for pc in &node.powerclip_children {
        collect_dxf_ltypes_recursive(pc, ltypes);
    }
}

fn emit_dxf_node(
    out: &mut String,
    node: &ObjectNode,
    parent_tf: Affine2,
    layer_name: &str,
    page_h: f64,
    ltypes: &[Vec<coralcraft_geom::F64>],
) -> Result<(), RenderError> {
    if !node.visible || node.opacity.get() <= 0.0 {
        return Ok(());
    }
    if parent_tf == Affine2::IDENTITY
        && node.effects.is_empty()
        && let ObjectKind::Group { children } = &node.kind
        && !children.is_empty()
        && let Some((sx, sy, dxf_rot_deg, tx, ty)) = try_decompose_dxf_insert_transform(node.transform)
    {
        let bname = format!("GRP_{}", node.id.0);
        let _ = writeln!(
            out,
            "0\nINSERT\n8\n{layer_name}\n2\n{bname}\n10\n{}\n20\n{}\n41\n{}\n42\n{}\n50\n{}",
            crate::svg::fmt_num_exact(tx),
            crate::svg::fmt_num_exact(page_h - ty),
            crate::svg::fmt_num_exact(sx),
            crate::svg::fmt_num_exact(sy),
            crate::svg::fmt_num_exact(dxf_rot_deg)
        );
        if let Some(name) = &node.name {
            let clean_name = sanitize_dxf_text(name);
            if !clean_name.is_empty() {
                let _ = writeln!(out, "1\n{clean_name}");
            }
        }
        return Ok(());
    }
    let world_tf = node.transform.then(parent_tf)?;
    if let ObjectKind::Group { children } = &node.kind {
        for ch in children {
            emit_dxf_node(out, ch, world_tf, layer_name, page_h, ltypes)?;
        }
        return Ok(());
    }
    let dxf_truecolor_attr = |col: &Color| -> String {
        let [r, g, b] = col.to_srgb8();
        let tc = ((i32::from(r) & 0xFF) << 16) | ((i32::from(g) & 0xFF) << 8) | (i32::from(b) & 0xFF);
        format!("420\n{tc}\n")
    };
    let text_col_attr = match &node.fill {
        Some(FillStyle::Uniform { color }) => dxf_truecolor_attr(color),
        _ => String::new(),
    };
    let mut poly_style_attrs = String::new();
    if let Some(outline) = &node.outline {
        poly_style_attrs.push_str(&dxf_truecolor_attr(&outline.color));
        if !outline.is_hairline() && outline.width_pt.get() > 0.0 {
            let lw_100mm = ((outline.width_pt.get() * 25.4 / 72.0) * 100.0).round().clamp(1.0, 2110.0) as i32;
            let _ = writeln!(poly_style_attrs, "370\n{lw_100mm}");
        }
        if !outline.dash_pattern.is_empty()
            && let Some(lt_idx) = ltypes.iter().position(|p| p == &outline.dash_pattern)
        {
            let _ = writeln!(poly_style_attrs, "6\nDASH_{}", lt_idx.saturating_add(1));
        }
    } else if let Some(FillStyle::Uniform { color }) = &node.fill {
        poly_style_attrs.push_str(&dxf_truecolor_attr(color));
    }
    if node.effects.is_empty() {
        match &node.kind {
            ObjectKind::Rect(spec)
                if world_tf == Affine2::IDENTITY
                    && spec.radii[0].get() > 1e-6
                    && spec.corner_kinds == [coralcraft_geom::CornerKind::Round; 4]
                    && (spec.radii[0].get() - spec.radii[1].get()).abs() <= 1e-6
                    && (spec.radii[0].get() - spec.radii[2].get()).abs() <= 1e-6
                    && (spec.radii[0].get() - spec.radii[3].get()).abs() <= 1e-6
                    && node.outline.as_ref().is_none_or(|o| {
                        o.start_arrow == coralcraft_doc::ArrowheadKind::None && o.end_arrow == coralcraft_doc::ArrowheadKind::None
                    }) =>
            {
                let w = spec.rect.width();
                let h = spec.rect.height();
                let r = spec.radii[0].get().clamp(0.0, 0.5 * w.min(h));
                if r > 1e-6 {
                    let x0 = spec.rect.x0.get();
                    let y0 = spec.rect.y0.get();
                    let x1 = spec.rect.x1.get();
                    let y1 = spec.rect.y1.get();
                    let bulge = fmt_num(-(std::f64::consts::SQRT_2 - 1.0));
                    let _ = writeln!(
                        out,
                        "0\nLWPOLYLINE\n8\n{layer_name}\n{poly_style_attrs}90\n8\n70\n1\n10\n{}\n20\n{}\n10\n{}\n20\n{}\n42\n{bulge}\n10\n{}\n20\n{}\n10\n{}\n20\n{}\n42\n{bulge}\n10\n{}\n20\n{}\n10\n{}\n20\n{}\n42\n{bulge}\n10\n{}\n20\n{}\n10\n{}\n20\n{}\n42\n{bulge}",
                        fmt_num(x0 + r),
                        fmt_num(page_h - y0),
                        fmt_num(x1 - r),
                        fmt_num(page_h - y0),
                        fmt_num(x1),
                        fmt_num(page_h - (y0 + r)),
                        fmt_num(x1),
                        fmt_num(page_h - (y1 - r)),
                        fmt_num(x1 - r),
                        fmt_num(page_h - y1),
                        fmt_num(x0 + r),
                        fmt_num(page_h - y1),
                        fmt_num(x0),
                        fmt_num(page_h - (y1 - r)),
                        fmt_num(x0),
                        fmt_num(page_h - (y0 + r)),
                    );
                    for pc in &node.powerclip_children {
                        emit_dxf_node(out, pc, world_tf, layer_name, page_h, ltypes)?;
                    }
                    return Ok(());
                }
            }
            ObjectKind::Ellipse(spec) if world_tf == Affine2::IDENTITY => {
                let rx = spec.rect.width() * 0.5;
                let ry = spec.rect.height() * 0.5;
                let cx = spec.rect.x0.get() + rx;
                let cy_dxf = page_h - (spec.rect.y0.get() + ry);
                if (rx - ry).abs() <= 1e-6 && rx > 0.0 {
                    if spec.mode == coralcraft_geom::EllipseMode::Full {
                        let _ = writeln!(
                            out,
                            "0\nCIRCLE\n8\n{layer_name}\n{poly_style_attrs}10\n{}\n20\n{}\n40\n{}",
                            fmt_num(cx),
                            fmt_num(cy_dxf),
                            fmt_num(rx)
                        );
                        return Ok(());
                    } else if spec.mode == coralcraft_geom::EllipseMode::Arc {
                        let a0 = (-spec.end_deg.get()).rem_euclid(360.0);
                        let a1 = (-spec.start_deg.get()).rem_euclid(360.0);
                        let _ = writeln!(
                            out,
                            "0\nARC\n8\n{layer_name}\n{poly_style_attrs}10\n{}\n20\n{}\n40\n{}\n50\n{}\n51\n{}",
                            fmt_num(cx),
                            fmt_num(cy_dxf),
                            fmt_num(rx),
                            fmt_num(a0),
                            fmt_num(a1)
                        );
                        return Ok(());
                    }
                } else if spec.mode == coralcraft_geom::EllipseMode::Full && rx > 0.0 && ry > 0.0 {
                    let ratio = (ry / rx).clamp(1e-3, 100.0);
                    let _ = writeln!(
                        out,
                        "0\nELLIPSE\n8\n{layer_name}\n{poly_style_attrs}10\n{}\n20\n{}\n11\n{}\n21\n0\n40\n{}\n41\n0\n42\n6.283185",
                        fmt_num(cx),
                        fmt_num(cy_dxf),
                        fmt_num(rx),
                        fmt_num(ratio)
                    );
                    return Ok(());
                }
            }
            ObjectKind::ArtisticText { text, font_family, size_pt, position } => {
                let clean = sanitize_dxf_text(text);
                if !clean.is_empty() {
                    let world_pt = world_tf.apply_point(*position)?;
                    let h_pt = (size_pt.get() * transform_scale(world_tf)).max(0.1);
                    let _ = writeln!(
                        out,
                        "0\nTEXT\n8\n{layer_name}\n{text_col_attr}10\n{}\n20\n{}\n40\n{}\n1\n{clean}",
                        fmt_num(world_pt.x.get()),
                        fmt_num(page_h - world_pt.y.get()),
                        fmt_num(h_pt)
                    );
                    if !font_family.is_empty() && font_family != "Sans" {
                        let _ = writeln!(out, "7\n{}", sanitize_dxf_text(font_family));
                    }
                    let screen_deg = world_tf.b.get().atan2(world_tf.a.get()).to_degrees();
                    if screen_deg.abs() > 1e-6 {
                        let dxf_ccw = (-screen_deg).rem_euclid(360.0);
                        let _ = writeln!(out, "50\n{}", fmt_num(dxf_ccw));
                    }
                }
                return Ok(());
            }
            ObjectKind::ParagraphText(pt) => {
                let raw_with_breaks = pt.text.replace('\r', "").replace('\n', "\\P");
                let clean = sanitize_dxf_text(&raw_with_breaks);
                if !clean.is_empty()
                    && let Ok(world_pt) = world_tf.apply_point(pt.position)
                {
                    let scale = transform_scale(world_tf);
                    let h_pt = (pt.font_size_pt.get() * scale).max(0.1);
                    let fw_pt = (pt.frame_width_pt.get() * scale).max(1.0);
                    let fh_pt = (pt.frame_height_pt.get() * scale).max(1.0);
                    let attach_code = match pt.alignment {
                        coralcraft_doc::TextAlignment::Left => 1,
                        coralcraft_doc::TextAlignment::Center => 2,
                        coralcraft_doc::TextAlignment::Right => 3,
                        coralcraft_doc::TextAlignment::Justify => 4,
                    };
                    let _ = writeln!(
                        out,
                        "0\nMTEXT\n8\n{layer_name}\n{text_col_attr}10\n{}\n20\n{}\n40\n{}\n41\n{}\n43\n{}\n71\n{attach_code}\n44\n{}\n75\n{}\n48\n{}",
                        crate::svg::fmt_num_exact(world_pt.x.get()),
                        crate::svg::fmt_num_exact(page_h - world_pt.y.get()),
                        crate::svg::fmt_num_exact(h_pt),
                        crate::svg::fmt_num_exact(fw_pt),
                        crate::svg::fmt_num_exact(fh_pt),
                        crate::svg::fmt_num_exact(pt.line_spacing_multiplier.get()),
                        pt.columns,
                        crate::svg::fmt_num_exact(pt.gutter_pt.get())
                    );
                    if !pt.font_family.is_empty() && pt.font_family != "Sans" {
                        let _ = writeln!(out, "7\n{}", sanitize_dxf_text(&pt.font_family));
                    }
                    let _ = writeln!(out, "1\n{clean}");
                }
                return Ok(());
            }
            _ => {}
        }
    }
    let Some((mut local_path, mut local_bbox)) = extract_local_path_and_bbox(&node.kind)? else {
        return Ok(());
    };
    apply_deforming_effects(node, &mut local_path, &mut local_bbox);
    let world_path = local_path.transform(world_tf)?;

    let has_arrows = node
        .outline
        .as_ref()
        .is_some_and(|o| o.start_arrow != coralcraft_doc::ArrowheadKind::None || o.end_arrow != coralcraft_doc::ArrowheadKind::None);
    if node.effects.is_empty()
        && !has_arrows
        && matches!(&node.kind, ObjectKind::Path(_))
        && let Some((ctrl_pts, closed)) = try_extract_cubic_spline_pts(&world_path)
    {
        let span_count = ctrl_pts.len().saturating_sub(1) / 3;
        let knot_count = ctrl_pts.len().saturating_add(4);
        let flag = if closed { 9 } else { 8 };
        let _ = writeln!(out, "0\nSPLINE\n8\n{layer_name}\n{poly_style_attrs}70\n{flag}\n71\n3\n72\n{knot_count}\n73\n{}", ctrl_pts.len());
        for _ in 0..4 {
            out.push_str("40\n0\n");
        }
        for s in 1..span_count {
            for _ in 0..3 {
                let _ = writeln!(out, "40\n{s}");
            }
        }
        for _ in 0..4 {
            let _ = writeln!(out, "40\n{span_count}");
        }
        for pt in &ctrl_pts {
            let _ = writeln!(out, "10\n{}\n20\n{}", crate::svg::fmt_num_exact(pt.x.get()), crate::svg::fmt_num_exact(page_h - pt.y.get()));
        }
        for pc in &node.powerclip_children {
            emit_dxf_node(out, pc, world_tf, layer_name, page_h, ltypes)?;
        }
        return Ok(());
    }

    let polys = flatten_path(&world_path, &FlattenOpts::default())?;

    for poly in &polys {
        if poly.points.len() < 2 {
            continue;
        }
        let closed_flag = if poly.closed { 1 } else { 0 };
        let _ = writeln!(out, "0\nPOLYLINE\n8\n{layer_name}\n{poly_style_attrs}66\n1\n70\n{closed_flag}");
        for &pt in &poly.points {
            let _ = writeln!(out, "0\nVERTEX\n8\n{layer_name}\n10\n{}\n20\n{}", fmt_num(pt.x.get()), fmt_num(page_h - pt.y.get()));
        }
        let _ = writeln!(out, "0\nSEQEND\n8\n{layer_name}");
    }

    if let Some(outline) = &node.outline
        && (outline.start_arrow != coralcraft_doc::ArrowheadKind::None || outline.end_arrow != coralcraft_doc::ArrowheadKind::None)
    {
        let w = if outline.is_hairline() { 0.24 } else { outline.width_pt.get().max(0.1) };
        if let Ok(arrow_polys) = crate::build_arrowhead_polylines(&polys, 0.5 * w, outline.start_arrow, outline.end_arrow) {
            for ap in &arrow_polys {
                if ap.points.len() < 2 {
                    continue;
                }
                let _ = writeln!(out, "0\nPOLYLINE\n8\n{layer_name}\n66\n1\n70\n1");
                for &pt in &ap.points {
                    let _ = writeln!(out, "0\nVERTEX\n8\n{layer_name}\n10\n{}\n20\n{}", fmt_num(pt.x.get()), fmt_num(page_h - pt.y.get()));
                }
                let _ = writeln!(out, "0\nSEQEND\n8\n{layer_name}");
            }
        }
    }

    for pc in &node.powerclip_children {
        emit_dxf_node(out, pc, world_tf, layer_name, page_h, ltypes)?;
    }

    Ok(())
}

pub(crate) fn apply_deforming_effects(node: &ObjectNode, local_path: &mut Path, local_bbox: &mut Rect) {
    for effect in &node.effects {
        match effect {
            VectorEffect::ArtisticMediaBrush { start_width_pt, end_width_pt } => {
                if let Ok(brush_path) = coralcraft_geom::expand_artistic_brush_stroke(local_path, start_width_pt.get(), end_width_pt.get())
                    && let Some(bb) = brush_path.bounds()
                {
                    *local_path = brush_path;
                    *local_bbox = bb;
                }
            }
            VectorEffect::Envelope { corners } => {
                if let Ok(warped) = coralcraft_geom::warp_path_bilinear(local_path, *local_bbox, *corners)
                    && let Some(bb) = warped.bounds()
                {
                    *local_path = warped;
                    *local_bbox = bb;
                }
            }
            VectorEffect::TextOnPath { spine, offset_pt } => {
                if let ObjectKind::ArtisticText { text, font_family, size_pt, .. } = &node.kind
                    && let Ok(curved) = coralcraft_geom::fit_text_to_path(text, font_family, size_pt.get(), spine, offset_pt.get())
                    && let Some(bb) = curved.bounds()
                {
                    *local_path = curved;
                    *local_bbox = bb;
                }
            }
            VectorEffect::Perspective { corners } => {
                if let Ok(persp) = coralcraft_geom::warp_path_perspective(local_path, *local_bbox, *corners)
                    && let Some(bb) = persp.bounds()
                {
                    *local_path = persp;
                    *local_bbox = bb;
                }
            }
            VectorEffect::Distortion { kind, amplitude, frequency } => {
                if let Ok(dist) = coralcraft_geom::distort_path(local_path, *local_bbox, *kind, amplitude.get(), frequency.get())
                    && let Some(bb) = dist.bounds()
                {
                    *local_path = dist;
                    *local_bbox = bb;
                }
            }
            _ => {}
        }
    }
    let _ = RectSpec::sharp(*local_bbox);
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use coralcraft_doc::ObjectId;
    use coralcraft_geom::F64;

    #[test]
    fn export_eps_powerclip_behind_fill_and_scale_with_object_m67() {
        let mut doc = Document::new_a4_portrait();
        let mut outl = OutlineStyle::with_width(Color::BLACK, F64::new(2.0).unwrap());
        outl.behind_fill = true;
        outl.scale_with_object = true;
        outl.cap = LineCap::Round;
        outl.join = LineJoin::Bevel;
        let mut parent = ObjectNode::new(
            ObjectId(10),
            Some("Parent".to_owned()),
            ObjectKind::Rect(RectSpec::sharp(Rect::new(10.0, 10.0, 90.0, 90.0).unwrap())),
            Some(FillStyle::uniform(Color::rgb8(200, 50, 50))),
            Some(outl),
        );
        parent.transform = Affine2::scaling(3.0, 3.0).unwrap();
        let child = ObjectNode::new(
            ObjectId(11),
            Some("ClippedChild".to_owned()),
            ObjectKind::Rect(RectSpec::sharp(Rect::new(20.0, 20.0, 60.0, 60.0).unwrap())),
            Some(FillStyle::uniform(Color::rgb8(50, 50, 200))),
            None,
        );
        parent.powerclip_children.push(child);
        doc.pages.first_mut().unwrap().layers.first_mut().unwrap().objects.push(parent);

        let eps = export_eps(&doc, 0).expect("export_eps");
        let stroke_pos = eps.find("6 setlinewidth\n1 setlinecap\n2 setlinejoin").expect("scaled behind_fill stroke");
        let fill_pos = eps.find("\nfill\n").expect("fill");
        let clip_pos = eps.find("\nclip\nnewpath\n").expect("PowerClip clip scope");
        assert!(stroke_pos < fill_pos, "behind_fill stroke must precede fill in EPS output");
        assert!(fill_pos < clip_pos, "PowerClip clip scope must follow fill");
    }

    #[test]
    fn export_eps_and_dxf_emit_native_artistic_and_paragraph_text_m68() {
        let mut doc = Document::new_a4_portrait();
        let atxt = ObjectNode::new(
            ObjectId(20),
            Some("Headline".to_owned()),
            ObjectKind::ArtisticText {
                text: "Hello (EPS)".to_owned(),
                font_family: "Helvetica".to_owned(),
                size_pt: F64::new(18.0).unwrap(),
                position: Point::new(50.0, 100.0).unwrap(),
            },
            Some(FillStyle::uniform(Color::rgb8(20, 40, 200))),
            None,
        );
        let ptxt_spec = coralcraft_doc::ParagraphTextSpec::new(
            Point::new(50.0, 200.0).unwrap(),
            160.0,
            80.0,
            "First line\nSecond line".to_owned(),
            "Times".to_owned(),
            12.0,
            1.2,
            coralcraft_doc::TextAlignment::Left,
            1,
            12.0,
        )
        .unwrap();
        let ptxt = ObjectNode::new(
            ObjectId(21),
            Some("BodyFrame".to_owned()),
            ObjectKind::ParagraphText(Box::new(ptxt_spec)),
            Some(FillStyle::uniform(Color::BLACK)),
            None,
        );
        let objs = &mut doc.pages.first_mut().unwrap().layers.first_mut().unwrap().objects;
        objs.push(atxt);
        objs.push(ptxt);

        let eps = export_eps(&doc, 0).expect("export_eps");
        assert!(eps.contains("/Helvetica findfont 18 scalefont setfont"), "EPS should select Helvetica 18pt: {eps}");
        assert!(eps.contains("(Hello \\(EPS\\)) show"), "EPS should emit escaped PostScript show operator: {eps}");
        assert!(eps.contains("/Times-Roman findfont 12 scalefont setfont"), "EPS should select Times-Roman 12pt: {eps}");
        assert!(eps.contains("(First line) show") && eps.contains("(Second line) show"), "EPS should emit paragraph lines: {eps}");

        let dxf = export_dxf(&doc, 0).expect("export_dxf");
        assert!(dxf.contains("0\nTEXT\n8\nLayer_1\n"), "DXF should emit TEXT entities: {dxf}");
        assert!(dxf.contains("\n40\n18\n1\nHello (EPS)\n"), "DXF should include 18pt ArtisticText: {dxf}");
        assert!(
            dxf.contains("0\nMTEXT\n") && dxf.contains("\n1\nFirst line\\PSecond line\n"),
            "DXF should include ParagraphText as native MTEXT with \\P line break: {dxf}"
        );

        let reimported = coralcraft_doc::import_dxf(&dxf).expect("import_dxf");
        let re_objs = &reimported.pages.first().unwrap().layers.first().unwrap().objects;
        assert_eq!(re_objs.len(), 2, "import_dxf should reconstruct ArtisticText and ParagraphText objects from DXF TEXT/MTEXT entities");
    }

    #[test]
    fn export_eps_fountain_fill_emits_clipped_shading_bands_m69() {
        let mut doc = Document::new_a4_portrait();
        let stops = vec![
            coralcraft_color::FountainStop::new(0.0, Color::rgb8(255, 0, 0)).unwrap(),
            coralcraft_color::FountainStop::new(1.0, Color::rgb8(0, 0, 255)).unwrap(),
        ];
        let fountain = FillStyle::Fountain {
            geometry: coralcraft_doc::FountainGeometry::Linear { angle_deg: F64::ZERO, edge_pad: F64::ZERO },
            blend_mode: coralcraft_color::FountainBlendMode::Direct,
            stops,
        };
        let node = ObjectNode::new(
            ObjectId(30),
            Some("FountainRect".to_owned()),
            ObjectKind::Rect(RectSpec::sharp(Rect::new(20.0, 40.0, 180.0, 120.0).unwrap())),
            Some(fountain),
            None,
        );
        doc.pages.first_mut().unwrap().layers.first_mut().unwrap().objects.push(node);

        let eps = export_eps(&doc, 0).expect("export_eps");
        assert!(eps.contains("clip\nnewpath\n"), "Fountain fill in EPS should clip to object path: {eps}");
        assert!(eps.contains("rectfill"), "Fountain fill in EPS should emit PostScript shading bands via rectfill: {eps}");
    }

    #[test]
    fn import_svg_groups_and_transforms_roundtrip_m69() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="400pt" height="300pt">
  <g id="layer-1" data-name="Layer 1">
    <g id="badge-group" transform="translate(30, 45)">
      <rect x="10" y="20" width="50" height="40" fill="#ff0000" transform="matrix(2 0 0 2 5 10)"/>
      <circle cx="80" cy="60" r="15" fill="#00ff00"/>
    </g>
  </g>
</svg>"##;
        let doc = coralcraft_doc::import_svg(svg).expect("import_svg");
        let objs = &doc.pages.first().unwrap().layers.first().unwrap().objects;
        assert_eq!(objs.len(), 1, "top-level layer should contain 1 Group object");
        let grp = objs.first().unwrap();
        assert!((grp.transform.tx.get() - 30.0).abs() < 1e-6);
        assert!((grp.transform.ty.get() - 45.0).abs() < 1e-6);
        let ObjectKind::Group { children } = &grp.kind else {
            panic!("expected ObjectKind::Group, got {:?}", grp.kind);
        };
        assert_eq!(children.len(), 2);
        let first_child = children.first().unwrap();
        assert!((first_child.transform.a.get() - 2.0).abs() < 1e-6);
        assert!((first_child.transform.tx.get() - 5.0).abs() < 1e-6);
    }

    #[test]
    fn import_svg_lines_polylines_polygons_and_stroke_attributes_m70() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="300pt" height="200pt">
  <line x1="10" y1="20" x2="110" y2="80" stroke="#112233" stroke-width="3.5" stroke-linecap="round" stroke-linejoin="bevel" stroke-dasharray="6, 2, 1"/>
  <polyline points="20,30 60,70 100,30" fill="none" stroke="#445566" stroke-width="2" stroke-linecap="square"/>
  <polygon points="10,10 50,10 30,40" fill="#abcdef" stroke="#000000" stroke-width="1.25" stroke-linejoin="round" stroke-dasharray="4 2"/>
</svg>"##;
        let doc = coralcraft_doc::import_svg(svg).expect("import_svg");
        let objs = &doc.pages.first().unwrap().layers.first().unwrap().objects;
        assert_eq!(objs.len(), 3, "expected 3 imported SVG objects (line, polyline, polygon)");

        let line_obj = objs.first().unwrap();
        let line_outl = line_obj.outline.as_ref().expect("line outline");
        assert!((line_outl.width_pt.get() - 3.5).abs() < 1e-6);
        assert_eq!(line_outl.cap, LineCap::Round);
        assert_eq!(line_outl.join, LineJoin::Bevel);
        assert_eq!(line_outl.dash_pattern.len(), 6, "odd-length SVG stroke-dasharray must duplicate per SVG 1.1");

        let poly_open = objs.get(1).unwrap();
        let ObjectKind::Path(p_open) = &poly_open.kind else {
            panic!("expected Path for polyline");
        };
        assert!(!p_open.elements.iter().any(|e| matches!(e, PathElement::Close)), "polyline must remain open");
        assert_eq!(poly_open.outline.as_ref().unwrap().cap, LineCap::Square);

        let poly_closed = objs.get(2).unwrap();
        let ObjectKind::Path(p_closed) = &poly_closed.kind else {
            panic!("expected Path for polygon");
        };
        assert!(p_closed.elements.iter().any(|e| matches!(e, PathElement::Close)), "polygon must be closed");
        let closed_outl = poly_closed.outline.as_ref().unwrap();
        assert!((closed_outl.width_pt.get() - 1.25).abs() < 1e-6);
        assert_eq!(closed_outl.join, LineJoin::Round);
        assert_eq!(closed_outl.dash_pattern.len(), 2);
    }

    #[test]
    fn export_eps_two_color_pattern_and_eps_dxf_arrowhead_markers_m70() {
        let mut doc = Document::new_a4_portrait();
        let pat_fill = FillStyle::TwoColorPattern {
            kind: coralcraft_doc::PatternKind::Checkerboard,
            fg_color: Color::rgb8(200, 0, 0),
            bg_color: Color::rgb8(240, 240, 240),
            tile_size_pt: F64::new(20.0).unwrap(),
        };
        let rect_node = ObjectNode::new(
            ObjectId(40),
            Some("PatternRect".to_owned()),
            ObjectKind::Rect(RectSpec::sharp(Rect::new(20.0, 20.0, 60.0, 60.0).unwrap())),
            Some(pat_fill),
            None,
        );
        let mut arrow_path = Path::new();
        arrow_path.move_to(Point::new(50.0, 150.0).unwrap()).unwrap();
        arrow_path.line_to(Point::new(180.0, 150.0).unwrap()).unwrap();
        let mut arrow_outl = OutlineStyle::with_width(Color::rgb8(0, 100, 200), F64::new(2.0).unwrap());
        arrow_outl.start_arrow = coralcraft_doc::ArrowheadKind::Circle;
        arrow_outl.end_arrow = coralcraft_doc::ArrowheadKind::Triangle;
        let line_node = ObjectNode::new(ObjectId(41), Some("ArrowLine".to_owned()), ObjectKind::Path(arrow_path), None, Some(arrow_outl));

        let objs = &mut doc.pages.first_mut().unwrap().layers.first_mut().unwrap().objects;
        objs.push(rect_node);
        objs.push(line_node);

        let eps = export_eps(&doc, 0).expect("export_eps");
        assert!(eps.contains("% TwoColorPattern"), "EPS should emit TwoColorPattern clipped tiles: {eps}");
        assert!(eps.contains("% Arrowheads"), "EPS should emit arrowhead marker polygons: {eps}");

        let dxf = export_dxf(&doc, 0).expect("export_dxf");
        let polyline_count = dxf.matches("\nPOLYLINE\n").count();
        assert!(polyline_count >= 4, "DXF should emit main path plus start/end arrowhead POLYLINE entities (got {polyline_count})");
    }

    #[test]
    fn import_svg_relative_paths_rounded_rect_rotate_and_dxf_arc_ellipse_m71() {
        use coralcraft_geom::{EllipseMode, FillRule};

        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="300pt" height="200pt">
  <rect x="20" y="30" width="100" height="60" rx="12" fill="#224466" transform="rotate(90, 70, 60)"/>
  <path d="m 10 20 h 40 v 30 l -20 10 q 10 10 20 0 c 5 -5 15 -5 20 0 z" fill="#884422" fill-rule="evenodd"/>
</svg>"##;
        let doc = coralcraft_doc::import_svg(svg).expect("import_svg");
        let objs = &doc.pages.first().unwrap().layers.first().unwrap().objects;
        assert_eq!(objs.len(), 2);

        let rect_node = objs.first().unwrap();
        let ObjectKind::Rect(rspec) = &rect_node.kind else {
            panic!("expected ObjectKind::Rect");
        };
        assert!((rspec.radii[0].get() - 12.0).abs() < 1e-6, "expected rx=12 corner radius on SVG rect");
        let center_rot = rect_node.transform.apply_point(Point::new(70.0, 60.0).unwrap()).unwrap();
        assert!((center_rot.x.get() - 70.0).abs() < 1e-4 && (center_rot.y.get() - 60.0).abs() < 1e-4, "rotate(90,70,60) must fix (70,60)");
        let corner_rot = rect_node.transform.apply_point(Point::new(80.0, 60.0).unwrap()).unwrap();
        assert!((corner_rot.x.get() - 70.0).abs() < 1e-4 && (corner_rot.y.get() - 70.0).abs() < 1e-4);

        let path_node = objs.get(1).unwrap();
        let ObjectKind::Path(svg_path) = &path_node.kind else {
            panic!("expected ObjectKind::Path");
        };
        assert_eq!(svg_path.fill_rule, FillRule::EvenOdd);
        assert_eq!(svg_path.elements.len(), 7, "expected MoveTo, 3 LineTo (h,v,l), QuadTo, CubicTo, Close");
        assert_eq!(svg_path.elements.get(1), Some(&PathElement::LineTo(Point::new(50.0, 20.0).unwrap())));
        assert_eq!(svg_path.elements.get(2), Some(&PathElement::LineTo(Point::new(50.0, 50.0).unwrap())));
        assert_eq!(svg_path.elements.get(3), Some(&PathElement::LineTo(Point::new(30.0, 60.0).unwrap())));

        let dxf = "0\nSECTION\n2\nENTITIES\n0\nARC\n10\n100.0\n20\n200.0\n40\n50.0\n50\n0.0\n51\n90.0\n0\nELLIPSE\n10\n150.0\n20\n300.0\n11\n60.0\n21\n0.0\n40\n0.5\n0\nENDSEC\n0\nEOF\n";
        let dxf_doc = coralcraft_doc::import_dxf(dxf).expect("import_dxf");
        let dxf_objs = &dxf_doc.pages.first().unwrap().layers.first().unwrap().objects;
        assert_eq!(dxf_objs.len(), 2, "expected ARC and ELLIPSE imported from DXF");
        let ObjectKind::Ellipse(arc_spec) = &dxf_objs.first().unwrap().kind else {
            panic!("expected ObjectKind::Ellipse for DXF ARC");
        };
        assert_eq!(arc_spec.mode, EllipseMode::Arc);
        assert!((arc_spec.rect.width() - 100.0).abs() < 1e-4);
        let ObjectKind::Ellipse(ell_spec) = &dxf_objs.get(1).unwrap().kind else {
            panic!("expected ObjectKind::Ellipse for DXF ELLIPSE");
        };
        assert_eq!(ell_spec.mode, EllipseMode::Full);
        assert!((ell_spec.rect.width() - 120.0).abs() < 1e-4);
        assert!((ell_spec.rect.height() - 60.0).abs() < 1e-4);
    }

    #[test]
    fn export_eps_bitmap_colorimage_and_import_svg_smooth_curves_and_chained_transforms_m72() {
        let mut doc = Document::new_a4_portrait();
        let rgba = vec![
            255, 0, 0, 255, // red
            0, 255, 0, 255, // green
            0, 0, 255, 255, // blue
            255, 255, 0, 255, // yellow
        ];
        doc.pages.first_mut().unwrap().layers.first_mut().unwrap().objects.push(ObjectNode::new(
            ObjectId(50),
            Some("TestBmp".to_owned()),
            ObjectKind::Bitmap { pixel_width: 2, pixel_height: 2, dpi_x: F64::new(72.0).unwrap(), dpi_y: F64::new(72.0).unwrap(), rgba_pixels: rgba },
            None,
            None,
        ));
        let eps = export_eps(&doc, 0).expect("export_eps with Bitmap");
        assert!(eps.contains("false 3 colorimage"), "EPS should emit PostScript colorimage for ObjectKind::Bitmap: {eps}");
        assert!(eps.contains("ff000000ff000000ffffff00"), "EPS should contain 24-bit RGB hex data: {eps}");

        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="300pt" height="200pt">
  <path d="M 10 10 C 20 0 30 0 40 10 S 60 20 70 10 Q 80 0 90 10 T 110 10" fill="none" stroke="#112233" transform="translate(10, 20) scale(2) skewX(45)"/>
</svg>"##;
        let svg_doc = coralcraft_doc::import_svg(svg).expect("import_svg smooth curves & chained transforms");
        let objs = &svg_doc.pages.first().unwrap().layers.first().unwrap().objects;
        assert_eq!(objs.len(), 1);
        let obj = objs.first().unwrap();
        let ObjectKind::Path(p) = &obj.kind else {
            panic!("expected ObjectKind::Path");
        };
        assert_eq!(p.elements.len(), 5, "expected MoveTo, CubicTo (C), CubicTo (S), QuadTo (Q), QuadTo (T)");
        match p.elements.get(2) {
            Some(PathElement::CubicTo { c1, c2, end }) => {
                assert!(
                    (c1.x.get() - 50.0).abs() < 1e-6 && (c1.y.get() - 20.0).abs() < 1e-6,
                    "S must reflect previous c2 (30,0) across (40,10) to (50,20)"
                );
                assert!((c2.x.get() - 60.0).abs() < 1e-6 && (c2.y.get() - 20.0).abs() < 1e-6);
                assert!((end.x.get() - 70.0).abs() < 1e-6 && (end.y.get() - 10.0).abs() < 1e-6);
            }
            other => panic!("expected CubicTo from S command, got {other:?}"),
        }
        match p.elements.get(4) {
            Some(PathElement::QuadTo { ctrl, end }) => {
                assert!(
                    (ctrl.x.get() - 100.0).abs() < 1e-6 && (ctrl.y.get() - 20.0).abs() < 1e-6,
                    "T must reflect previous ctrl (80,0) across (90,10) to (100,20)"
                );
                assert!((end.x.get() - 110.0).abs() < 1e-6 && (end.y.get() - 10.0).abs() < 1e-6);
            }
            other => panic!("expected QuadTo from T command, got {other:?}"),
        }
        let mapped = obj.transform.apply_point(Point::new(1.0, 1.0).unwrap()).unwrap();
        assert!(
            (mapped.x.get() - 14.0).abs() < 1e-4 && (mapped.y.get() - 22.0).abs() < 1e-4,
            "translate(10,20)*scale(2)*skewX(45) on (1,1) -> (14,22), got {mapped:?}"
        );
    }

    #[test]
    fn import_svg_arc_command_inline_css_style_and_dxf_solid_mtext_m73() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="300pt" height="200pt">
  <path d="M 10 50 A 40 40 0 0 1 90 50 a 20 20 0 0 0 40 0" style="fill:#336699;stroke:#993300;stroke-width:3pt;stroke-linecap:round;stroke-linejoin:bevel;stroke-dasharray:5,3;fill-rule:evenodd"/>
</svg>"##;
        let svg_doc = coralcraft_doc::import_svg(svg).expect("import_svg A/a arc and inline CSS style");
        let objs = &svg_doc.pages.first().unwrap().layers.first().unwrap().objects;
        assert_eq!(objs.len(), 1);
        let obj = objs.first().unwrap();
        assert_eq!(obj.fill, Some(FillStyle::uniform(Color::rgb8(0x33, 0x66, 0x99))));
        let outl = obj.outline.as_ref().expect("inline style stroke");
        assert_eq!(outl.color, Color::rgb8(0x99, 0x33, 0x00));
        assert!((outl.width_pt.get() - 3.0).abs() < 1e-6);
        assert_eq!(outl.cap, LineCap::Round);
        assert_eq!(outl.join, LineJoin::Bevel);
        assert_eq!(outl.dash_pattern.len(), 2);
        let ObjectKind::Path(p) = &obj.kind else {
            panic!("expected ObjectKind::Path");
        };
        assert_eq!(p.fill_rule, FillRule::EvenOdd);
        assert!(p.elements.iter().any(|e| matches!(e, PathElement::CubicTo { .. })), "SVG A/a arc must produce CubicTo segments");
        let Some(PathElement::CubicTo { end, .. }) = p.elements.last() else {
            panic!("expected last element to be CubicTo ending at (130, 50)");
        };
        assert!((end.x.get() - 130.0).abs() < 1e-4 && (end.y.get() - 50.0).abs() < 1e-4);

        let dxf = "0\nSECTION\n2\nENTITIES\n0\nSOLID\n10\n10.0\n20\n100.0\n11\n50.0\n21\n100.0\n12\n10.0\n22\n140.0\n13\n50.0\n23\n140.0\n0\nMTEXT\n10\n25.0\n20\n200.0\n40\n14.0\n1\nHello\\PWorld\n0\nENDSEC\n0\nEOF\n";
        let dxf_doc = coralcraft_doc::import_dxf(dxf).expect("import_dxf SOLID and MTEXT");
        let dxf_objs = &dxf_doc.pages.first().unwrap().layers.first().unwrap().objects;
        assert_eq!(dxf_objs.len(), 2, "expected SOLID and MTEXT imported from DXF");
        let solid_obj = dxf_objs.first().unwrap();
        assert_eq!(solid_obj.fill, Some(FillStyle::uniform(Color::BLACK)));
        let ObjectKind::Path(solid_path) = &solid_obj.kind else {
            panic!("expected ObjectKind::Path for DXF SOLID");
        };
        // DXF SOLID orders vertices 1(10,20) -> 2(11,21) -> 4(13,23) -> 3(12,22) -> Close
        assert_eq!(solid_path.elements.len(), 5);
        let ObjectKind::ArtisticText { text, size_pt, .. } = &dxf_objs.get(1).unwrap().kind else {
            panic!("expected ObjectKind::ArtisticText for DXF MTEXT");
        };
        assert_eq!(text, "Hello World");
        assert!((size_pt.get() - 14.0).abs() < 1e-6);
    }

    #[test]
    fn import_svg_colors_opacity_implicit_coords_and_dxf_bulge_spline_aci_m74() {
        use coralcraft_doc::TransparencyStyle;

        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 500 400">
  <path d="M 10 20 50 20 50 60-40 0 Z" fill="#f80" stroke="rgb(10%, 50%, 90%)" opacity="0.65" stroke-miterlimit="6"/>
  <rect x="60" y="20" width="40" height="30" fill="teal" stroke="transparent"/>
</svg>"##;
        let svg_doc = coralcraft_doc::import_svg(svg).expect("import_svg M74");
        let page = svg_doc.pages.first().unwrap();
        assert!((page.size.width.get() - 500.0).abs() < 1e-4 && (page.size.height.get() - 400.0).abs() < 1e-4);
        let objs = &page.layers.first().unwrap().objects;
        assert_eq!(objs.len(), 2);
        let path_obj = objs.first().unwrap();
        assert_eq!(path_obj.fill, Some(FillStyle::uniform(Color::rgb8(0xFF, 0x88, 0x00))));
        let outl = path_obj.outline.as_ref().expect("rgb(%) stroke");
        assert_eq!(outl.color, Color::rgb8(26, 128, 230));
        assert!((outl.miter_limit.get() - 6.0).abs() < 1e-6);
        assert_eq!(path_obj.transparency, Some(TransparencyStyle::Uniform { opacity: F64::new(0.65).unwrap() }));
        let ObjectKind::Path(p) = &path_obj.kind else {
            panic!("expected ObjectKind::Path");
        };
        // M(10,20) -> implicit L(50,20) -> implicit L(50,60) -> implicit L(-40,0) -> Close
        assert_eq!(p.elements.len(), 5);
        assert_eq!(objs.get(1).unwrap().fill, Some(FillStyle::uniform(Color::rgb8(0, 128, 128))));
        assert!(objs.get(1).unwrap().outline.is_none());

        let dxf = "0\nSECTION\n2\nENTITIES\n0\nLWPOLYLINE\n62\n1\n70\n1\n10\n20.0\n20\n100.0\n42\n1.0\n10\n100.0\n20\n100.0\n0\nSPLINE\n420\n33023\n71\n3\n10\n10.0\n20\n200.0\n10\n30.0\n20\n250.0\n10\n70.0\n20\n250.0\n10\n90.0\n20\n200.0\n0\nENDSEC\n0\nEOF\n";
        let dxf_doc = coralcraft_doc::import_dxf(dxf).expect("import_dxf M74");
        let dxf_objs = &dxf_doc.pages.first().unwrap().layers.first().unwrap().objects;
        assert_eq!(dxf_objs.len(), 2, "expected LWPOLYLINE with bulge and SPLINE");
        let lw_obj = dxf_objs.first().unwrap();
        assert_eq!(lw_obj.outline.as_ref().unwrap().color, Color::rgb8(255, 0, 0));
        let ObjectKind::Path(lw_path) = &lw_obj.kind else {
            panic!("expected ObjectKind::Path for LWPOLYLINE");
        };
        assert!(lw_path.elements.iter().any(|e| matches!(e, PathElement::CubicTo { .. })), "bulge=1.0 must emit cubic arc segments");

        let sp_obj = dxf_objs.get(1).unwrap();
        assert_eq!(sp_obj.outline.as_ref().unwrap().color, Color::rgb8(0x00, 0x80, 0xFF));
        let ObjectKind::Path(sp_path) = &sp_obj.kind else {
            panic!("expected ObjectKind::Path for SPLINE");
        };
        assert!(sp_path.elements.iter().any(|e| matches!(e, PathElement::CubicTo { .. })), "SPLINE must emit cubic segments");
    }

    #[test]
    fn export_svg_and_pdf_arrowheads_and_pdf_two_color_pattern_kinds_m75() {
        use coralcraft_doc::{ArrowheadKind, PatternKind};

        let mut doc = Document::new_a4_portrait();
        let mut arrow_path = Path::new();
        arrow_path.move_to(Point::new(30.0, 100.0).unwrap()).unwrap();
        arrow_path.line_to(Point::new(180.0, 100.0).unwrap()).unwrap();
        let mut outl = OutlineStyle::with_width(Color::rgb8(200, 30, 50), F64::new(3.0).unwrap());
        outl.start_arrow = ArrowheadKind::Triangle;
        outl.end_arrow = ArrowheadKind::Diamond;
        doc.pages.first_mut().unwrap().layers.first_mut().unwrap().objects.push(ObjectNode::new(
            ObjectId(1),
            Some("ArrowLine".to_owned()),
            ObjectKind::Path(arrow_path),
            None,
            Some(outl),
        ));

        let pat_fill = FillStyle::TwoColorPattern {
            kind: PatternKind::PolkaDot,
            fg_color: Color::rgb8(20, 100, 180),
            bg_color: Color::rgb8(240, 240, 250),
            tile_size_pt: F64::new(20.0).unwrap(),
        };
        doc.pages.first_mut().unwrap().layers.first_mut().unwrap().objects.push(ObjectNode::new(
            ObjectId(2),
            Some("PolkaBox".to_owned()),
            ObjectKind::Rect(RectSpec::sharp(Rect::new(30.0, 140.0, 130.0, 220.0).unwrap())),
            Some(pat_fill),
            None,
        ));

        let svg = crate::export_svg(&doc, 0).expect("export_svg with arrowheads");
        assert!(svg.contains("class=\"arrowhead-marker\""), "SVG must emit arrowhead-marker polygon elements");

        let pdf_bytes = crate::export_pdf(&doc).expect("export_pdf with arrowheads");
        let pdf_str = String::from_utf8_lossy(&pdf_bytes);
        assert!(pdf_str.contains("h f\n"), "PDF must emit closed filled arrowhead polygons (h f)");

        let imposed_spec = crate::ImpositionSpec {
            sheet_width_pt: F64::new(841.89).unwrap(),
            sheet_height_pt: F64::new(595.276).unwrap(),
            rows: 1,
            cols: 1,
            margin_pt: F64::new(10.0).unwrap(),
            gutter_pt: F64::ZERO,
            crop_marks: false,
        };
        let imposed_bytes = crate::export_pdf_imposed(&doc, &imposed_spec).expect("export_pdf_imposed with PolkaDot");
        let imposed_str = String::from_utf8_lossy(&imposed_bytes);
        assert!(imposed_str.contains(" c\n"), "PolkaDot pattern in emit_pdf_fill must emit cubic circle arcs");
    }

    #[test]
    fn export_svg_with_prepress_roundtrips_multilayer_and_page_background_via_import_svg_m79() {
        use coralcraft_doc::{ArrowheadKind, Layer, LayerId};

        let mut doc = Document::new_a4_portrait();
        let page = doc.pages.first_mut().unwrap();
        page.background = Some(FillStyle::uniform(Color::rgb8(0xF4, 0xF8, 0xFF)));
        page.layers.first_mut().unwrap().name = "Base Layer".to_owned();
        page.layers.first_mut().unwrap().opacity = F64::new(0.8).unwrap();

        let mut arrow_path = Path::new();
        arrow_path.move_to(Point::new(20.0, 50.0).unwrap()).unwrap();
        arrow_path.line_to(Point::new(140.0, 50.0).unwrap()).unwrap();
        let mut outl = OutlineStyle::with_width(Color::rgb8(180, 20, 40), F64::new(2.0).unwrap());
        outl.end_arrow = ArrowheadKind::Triangle;
        page.layers.first_mut().unwrap().objects.push(ObjectNode::new(
            ObjectId(1),
            Some("ArrowPath".to_owned()),
            ObjectKind::Path(arrow_path),
            None,
            Some(outl),
        ));

        let mut layer2 = Layer::new_normal(LayerId(2), "Overlay Layer");
        layer2.opacity = F64::new(0.6).unwrap();
        layer2.objects.push(ObjectNode::new(
            ObjectId(2),
            Some("OverlayRect".to_owned()),
            ObjectKind::Rect(RectSpec::sharp(Rect::new(40.0, 80.0, 120.0, 140.0).unwrap())),
            Some(FillStyle::uniform(Color::rgb8(0x10, 0x90, 0x50))),
            None,
        ));
        page.layers.push(layer2);

        let prepress = crate::PrepressOptions {
            bleed_pt: F64::new(9.0).unwrap(),
            crop_marks: true,
            registration_marks: true,
            ..crate::PrepressOptions::default()
        };
        let svg = crate::export_svg_with_prepress(&doc, 0, &prepress).expect("export_svg_with_prepress");
        assert!(svg.contains("id=\"page-background\""), "export_svg must tag page-background rect");

        let reimported = coralcraft_doc::import_svg(&svg).expect("import_svg from export_svg_with_prepress");
        let rpage = reimported.pages.first().unwrap();
        assert_eq!(rpage.background, Some(FillStyle::uniform(Color::rgb8(0xF4, 0xF8, 0xFF))));
        assert_eq!(rpage.layers.len(), 2);
        assert_eq!(rpage.layers.first().unwrap().name, "Base Layer");
        assert!((rpage.layers.first().unwrap().opacity.get() - 0.8).abs() < 1e-6);
        assert_eq!(rpage.layers.first().unwrap().objects.len(), 1);
        assert_eq!(rpage.layers.get(1).unwrap().name, "Overlay Layer");
        assert!((rpage.layers.get(1).unwrap().opacity.get() - 0.6).abs() < 1e-6);
        assert_eq!(rpage.layers.get(1).unwrap().objects.len(), 1);
    }

    #[test]
    fn export_dxf_roundtrips_multilayer_truecolor_and_lineweight_via_import_dxf_m80() {
        use coralcraft_doc::{Layer, LayerId};

        let mut doc = Document::new_a4_portrait();
        let page = doc.pages.first_mut().unwrap();
        page.layers.first_mut().unwrap().name = "Walls".to_owned();
        page.layers.first_mut().unwrap().objects.push(ObjectNode::new(
            ObjectId(1),
            Some("WallRect".to_owned()),
            ObjectKind::Rect(RectSpec::sharp(Rect::new(20.0, 20.0, 120.0, 80.0).unwrap())),
            None,
            Some(OutlineStyle::with_width(Color::rgb8(0x22, 0x66, 0xCC), F64::new(2.8346).unwrap())),
        ));

        let mut layer2 = Layer::new_normal(LayerId(2), "Labels");
        layer2.objects.push(ObjectNode::new(
            ObjectId(2),
            Some("LabelText".to_owned()),
            ObjectKind::ArtisticText {
                text: "Room A".to_owned(),
                font_family: "Sans".to_owned(),
                size_pt: F64::new(14.0).unwrap(),
                position: Point::new(40.0, 50.0).unwrap(),
            },
            Some(FillStyle::uniform(Color::rgb8(0xDD, 0x44, 0x11))),
            None,
        ));
        page.layers.push(layer2);

        let dxf = crate::export_dxf(&doc, 0).expect("export_dxf M80");
        let reimported = coralcraft_doc::import_dxf(&dxf).expect("import_dxf from export_dxf M80");
        let rpage = reimported.pages.first().unwrap();
        assert_eq!(rpage.layers.len(), 2);
        assert_eq!(rpage.layers.first().unwrap().name, "Walls");
        assert_eq!(rpage.layers.first().unwrap().objects.len(), 1);
        let wall_outl = rpage.layers.first().unwrap().objects.first().unwrap().outline.as_ref().unwrap();
        assert_eq!(wall_outl.color, Color::rgb8(0x22, 0x66, 0xCC));
        assert!((wall_outl.width_pt.get() - 2.8346).abs() < 0.05);

        assert_eq!(rpage.layers.get(1).unwrap().name, "Labels");
        assert_eq!(rpage.layers.get(1).unwrap().objects.len(), 1);
        let lbl = rpage.layers.get(1).unwrap().objects.first().unwrap();
        assert_eq!(lbl.fill, Some(FillStyle::uniform(Color::rgb8(0xDD, 0x44, 0x11))));
    }

    #[test]
    fn export_dxf_and_import_dxf_roundtrip_page_size_circle_arc_and_ellipse_m81() {
        use coralcraft_geom::{EllipseMode, EllipseSpec, Size};

        let mut doc = Document::new_a4_portrait();
        let page = doc.pages.first_mut().unwrap();
        page.size = Size::new(792.0, 612.0).unwrap();
        let objs = &mut page.layers.first_mut().unwrap().objects;

        // 1. Circle (rx == ry == 40, mode == Full)
        objs.push(ObjectNode::new(
            ObjectId(1),
            Some("MyCircle".to_owned()),
            ObjectKind::Ellipse(EllipseSpec::full(Rect::new(60.0, 80.0, 140.0, 160.0).unwrap())),
            None,
            Some(OutlineStyle::with_width(Color::rgb8(0x11, 0x88, 0x33), F64::new(1.5).unwrap())),
        ));

        // 2. Circular Arc (rx == ry == 50, mode == Arc, 0..90 deg)
        objs.push(ObjectNode::new(
            ObjectId(2),
            Some("MyArc".to_owned()),
            ObjectKind::Ellipse(EllipseSpec::arc(Rect::new(200.0, 100.0, 300.0, 200.0).unwrap(), 0.0, 90.0).unwrap()),
            None,
            Some(OutlineStyle::hairline(Color::rgb8(0xCC, 0x22, 0x44))),
        ));

        // 3. Non-circular Full Ellipse (rx = 80, ry = 40)
        objs.push(ObjectNode::new(
            ObjectId(3),
            Some("MyEllipse".to_owned()),
            ObjectKind::Ellipse(EllipseSpec::full(Rect::new(360.0, 200.0, 520.0, 280.0).unwrap())),
            None,
            Some(OutlineStyle::hairline(Color::rgb8(0x22, 0x44, 0xCC))),
        ));

        let dxf = crate::export_dxf(&doc, 0).expect("export_dxf M81");
        assert!(dxf.contains("0\nCIRCLE\n"), "export_dxf should emit native CIRCLE entity: {dxf}");
        assert!(dxf.contains("0\nARC\n"), "export_dxf should emit native ARC entity: {dxf}");
        assert!(dxf.contains("0\nELLIPSE\n"), "export_dxf should emit native ELLIPSE entity: {dxf}");

        let reimported = coralcraft_doc::import_dxf(&dxf).expect("import_dxf M81");
        let rpage = reimported.pages.first().unwrap();
        assert!((rpage.size.width.get() - 792.0).abs() < 1e-4 && (rpage.size.height.get() - 612.0).abs() < 1e-4);
        let robjs = &rpage.layers.first().unwrap().objects;
        assert_eq!(robjs.len(), 3);

        let ObjectKind::Ellipse(c_spec) = &robjs.first().unwrap().kind else {
            panic!("expected ObjectKind::Ellipse for CIRCLE, got {:?}", robjs.first().unwrap().kind);
        };
        assert_eq!(c_spec.mode, EllipseMode::Full);
        assert!((c_spec.rect.x0.get() - 60.0).abs() < 1e-3 && (c_spec.rect.y0.get() - 80.0).abs() < 1e-3);
        assert!((c_spec.rect.width() - 80.0).abs() < 1e-3 && (c_spec.rect.height() - 80.0).abs() < 1e-3);

        let ObjectKind::Ellipse(a_spec) = &robjs.get(1).unwrap().kind else {
            panic!("expected ObjectKind::Ellipse for ARC, got {:?}", robjs.get(1).unwrap().kind);
        };
        assert_eq!(a_spec.mode, EllipseMode::Arc);
        assert!((a_spec.rect.x0.get() - 200.0).abs() < 1e-3 && (a_spec.rect.y0.get() - 100.0).abs() < 1e-3);
        assert!((a_spec.start_deg.get() - 0.0).abs() < 1e-3 && (a_spec.end_deg.get() - 90.0).abs() < 1e-3);

        let ObjectKind::Ellipse(e_spec) = &robjs.get(2).unwrap().kind else {
            panic!("expected ObjectKind::Ellipse for ELLIPSE, got {:?}", robjs.get(2).unwrap().kind);
        };
        assert_eq!(e_spec.mode, EllipseMode::Full);
        assert!((e_spec.rect.x0.get() - 360.0).abs() < 1e-3 && (e_spec.rect.y0.get() - 200.0).abs() < 1e-3);
        assert!((e_spec.rect.width() - 160.0).abs() < 1e-3 && (e_spec.rect.height() - 80.0).abs() < 1e-3);
    }

    #[test]
    fn export_svg_and_import_svg_roundtrip_paragraph_text_frame_and_group_metadata_m82() {
        use coralcraft_doc::{ParagraphTextSpec, TextAlignment};

        let mut doc = Document::new_a4_portrait();
        let objs = &mut doc.pages.first_mut().unwrap().layers.first_mut().unwrap().objects;

        let ptxt_spec = ParagraphTextSpec::new(
            Point::new(36.0, 48.0).unwrap(),
            220.0,
            120.0,
            "First paragraph with soft wrapping words inside frame.\nSecond paragraph line.",
            "Helvetica",
            11.0,
            1.35,
            TextAlignment::Justify,
            2,
            12.0,
        )
        .unwrap();
        let ptxt_node = ObjectNode::new(
            ObjectId(1),
            Some("ArticleColumnFrame".to_owned()),
            ObjectKind::ParagraphText(Box::new(ptxt_spec.clone())),
            Some(FillStyle::uniform(Color::rgb8(0x22, 0x33, 0x44))),
            None,
        );

        let badge_rect = ObjectNode::new(
            ObjectId(2),
            Some("BadgeRect".to_owned()),
            ObjectKind::Rect(RectSpec::sharp(Rect::new(36.0, 200.0, 136.0, 250.0).unwrap())),
            Some(FillStyle::uniform(Color::rgb8(0xEE, 0x55, 0x22))),
            None,
        );

        let mut grp = ObjectNode::new(
            ObjectId(3),
            Some("Editorial Group & Header".to_owned()),
            ObjectKind::Group { children: vec![ptxt_node, badge_rect] },
            None,
            None,
        );
        grp.locked = true;
        grp.opacity = F64::new(0.85).unwrap();
        grp.transform = Affine2::translation(15.0, 25.0).unwrap();
        objs.push(grp);

        let svg = crate::export_svg(&doc, 0).expect("export_svg M82");
        assert!(svg.contains("data-frame-width=\"220\""), "export_svg must emit ParagraphText frame metadata: {svg}");
        assert!(svg.contains("data-name=\"Editorial Group &amp; Header\""), "export_svg must emit Group data-name: {svg}");

        let reimported = coralcraft_doc::import_svg(&svg).expect("import_svg M82");
        let robjs = &reimported.pages.first().unwrap().layers.first().unwrap().objects;
        assert_eq!(robjs.len(), 1);
        let rgrp = robjs.first().unwrap();
        assert_eq!(rgrp.name.as_deref(), Some("Editorial Group & Header"));
        assert!(rgrp.locked);
        assert!((rgrp.opacity.get() - 0.85).abs() < 1e-6);
        assert_eq!(rgrp.transform, Affine2::translation(15.0, 25.0).unwrap());

        let ObjectKind::Group { children } = &rgrp.kind else {
            panic!("expected ObjectKind::Group, got {:?}", rgrp.kind);
        };
        assert_eq!(children.len(), 2);
        let rptxt = children.first().unwrap();
        assert_eq!(rptxt.name.as_deref(), Some("ArticleColumnFrame"));
        assert_eq!(rptxt.fill, Some(FillStyle::uniform(Color::rgb8(0x22, 0x33, 0x44))));
        let ObjectKind::ParagraphText(r_spec) = &rptxt.kind else {
            panic!("expected ObjectKind::ParagraphText, got {:?}", rptxt.kind);
        };
        assert!((r_spec.position.x.get() - 36.0).abs() < 1e-4 && (r_spec.position.y.get() - 48.0).abs() < 1e-4);
        assert!((r_spec.frame_width_pt.get() - 220.0).abs() < 1e-4 && (r_spec.frame_height_pt.get() - 120.0).abs() < 1e-4);
        assert_eq!(r_spec.text, ptxt_spec.text);
        assert_eq!(r_spec.font_family, "Helvetica");
        assert!((r_spec.font_size_pt.get() - 11.0).abs() < 1e-4);
        assert!((r_spec.line_spacing_multiplier.get() - 1.35).abs() < 1e-4);
        assert_eq!(r_spec.alignment, TextAlignment::Justify);
        assert_eq!(r_spec.columns, 2);
        assert!((r_spec.gutter_pt.get() - 12.0).abs() < 1e-4);
    }

    #[test]
    fn export_svg_and_import_svg_roundtrip_object_wrapper_powerclip_behind_fill_and_arrowheads_m83() {
        use coralcraft_doc::{ArrowheadKind, LineCap, LineJoin};
        use coralcraft_geom::EllipseSpec;

        let mut doc = Document::new_a4_portrait();
        let objs = &mut doc.pages.first_mut().unwrap().layers.first_mut().unwrap().objects;

        let mut banner_path = Path::new();
        banner_path.move_to(Point::new(40.0, 50.0).unwrap()).unwrap();
        banner_path.line_to(Point::new(220.0, 50.0).unwrap()).unwrap();
        banner_path.line_to(Point::new(220.0, 140.0).unwrap()).unwrap();
        banner_path.line_to(Point::new(40.0, 140.0).unwrap()).unwrap();
        banner_path.close().unwrap();

        let mut outline = OutlineStyle::with_width(Color::rgb8(0x14, 0x28, 0x50), F64::new(3.0).unwrap());
        outline.cap = LineCap::Round;
        outline.join = LineJoin::Bevel;
        outline.miter_limit = F64::new(6.0).unwrap();
        outline.behind_fill = true;
        outline.scale_with_object = false;
        outline.dash_pattern = vec![F64::new(6.0).unwrap(), F64::new(3.0).unwrap()];
        outline.start_arrow = ArrowheadKind::Triangle;
        outline.end_arrow = ArrowheadKind::Diamond;

        let pc_child = ObjectNode::new(
            ObjectId(1),
            Some("ClippedBadge".to_owned()),
            ObjectKind::Ellipse(EllipseSpec::full(Rect::new(70.0, 65.0, 190.0, 125.0).unwrap())),
            Some(FillStyle::uniform(Color::rgb8(0x30, 0xA0, 0xE0))),
            None,
        );

        let mut banner = ObjectNode::new(
            ObjectId(2),
            Some("Clipped Banner".to_owned()),
            ObjectKind::Path(banner_path),
            Some(FillStyle::uniform(Color::rgb8(0xF0, 0x64, 0x28))),
            Some(outline),
        );
        banner.locked = true;
        banner.opacity = F64::new(0.8).unwrap();
        banner.transform = Affine2::new(2.0, 0.0, 0.0, 2.0, 15.0, 25.0).unwrap();
        banner.powerclip_children.push(pc_child);
        objs.push(banner);

        let svg = crate::export_svg(&doc, 0).expect("export_svg M83");
        assert!(svg.contains("data-object-wrapper=\"1\""), "export_svg must mark wrapped object group: {svg}");
        assert!(svg.contains("paint-order=\"stroke fill\""), "export_svg must emit paint-order for behind_fill: {svg}");
        assert!(svg.contains("vector-effect=\"non-scaling-stroke\""), "export_svg must emit non-scaling-stroke: {svg}");
        assert!(svg.contains("data-start-arrow=\"triangle\""), "export_svg must emit data-start-arrow: {svg}");
        assert!(svg.contains("data-end-arrow=\"diamond\""), "export_svg must emit data-end-arrow: {svg}");

        let reimported = coralcraft_doc::import_svg(&svg).expect("import_svg M83");
        let robjs = &reimported.pages.first().unwrap().layers.first().unwrap().objects;
        assert_eq!(robjs.len(), 1, "wrapped object must unwrap to a single top-level ObjectNode");
        let rnode = robjs.first().unwrap();
        assert_eq!(rnode.name.as_deref(), Some("Clipped Banner"));
        assert!(rnode.locked);
        assert!((rnode.opacity.get() - 0.8).abs() < 1e-6);
        assert_eq!(rnode.transform, Affine2::new(2.0, 0.0, 0.0, 2.0, 15.0, 25.0).unwrap());
        assert_eq!(rnode.fill, Some(FillStyle::uniform(Color::rgb8(0xF0, 0x64, 0x28))));
        assert!(matches!(rnode.kind, ObjectKind::Path(_)));

        let rout = rnode.outline.as_ref().expect("outline on reimported banner");
        assert_eq!(rout.color, Color::rgb8(0x14, 0x28, 0x50));
        assert!((rout.width_pt.get() - 3.0).abs() < 1e-4, "expected original 3.0pt stroke width, got {}", rout.width_pt.get());
        assert_eq!(rout.cap, LineCap::Round);
        assert_eq!(rout.join, LineJoin::Bevel);
        assert!((rout.miter_limit.get() - 6.0).abs() < 1e-4);
        assert!(rout.behind_fill);
        assert!(!rout.scale_with_object);
        assert_eq!(rout.dash_pattern, vec![F64::new(6.0).unwrap(), F64::new(3.0).unwrap()]);
        assert_eq!(rout.start_arrow, ArrowheadKind::Triangle);
        assert_eq!(rout.end_arrow, ArrowheadKind::Diamond);

        assert_eq!(rnode.powerclip_children.len(), 1);
        let rpc = rnode.powerclip_children.first().unwrap();
        assert_eq!(rpc.name.as_deref(), Some("ClippedBadge"));
        assert_eq!(rpc.fill, Some(FillStyle::uniform(Color::rgb8(0x30, 0xA0, 0xE0))));
        assert!(matches!(rpc.kind, ObjectKind::Ellipse(_)));
    }

    #[test]
    fn export_dxf_and_import_dxf_roundtrip_ltype_dashes_rotated_text_and_rounded_lwpolyline_m84() {
        let mut doc = Document::new_a4_portrait();
        let objs = &mut doc.pages.first_mut().unwrap().layers.first_mut().unwrap().objects;

        // 1. Dashed open line path
        let mut line_path = Path::new();
        line_path.move_to(Point::new(30.0, 40.0).unwrap()).unwrap();
        line_path.line_to(Point::new(230.0, 40.0).unwrap()).unwrap();
        let mut line_outl = OutlineStyle::with_width(Color::rgb8(0x10, 0x50, 0x90), F64::new(2.0).unwrap());
        line_outl.dash_pattern = vec![F64::new(8.0).unwrap(), F64::new(4.0).unwrap(), F64::new(2.0).unwrap(), F64::new(4.0).unwrap()];
        objs.push(ObjectNode::new(ObjectId(1), Some("DashedLine".to_owned()), ObjectKind::Path(line_path), None, Some(line_outl)));

        // 2. Parametric rounded rectangle with dashed outline
        let rspec = RectSpec::uniform(Rect::new(40.0, 80.0, 200.0, 180.0).unwrap(), 15.0, coralcraft_geom::CornerKind::Round).unwrap();
        let mut rect_outl = OutlineStyle::with_width(Color::rgb8(0xCC, 0x33, 0x22), F64::new(1.5).unwrap());
        rect_outl.dash_pattern = vec![F64::new(6.0).unwrap(), F64::new(3.0).unwrap()];
        objs.push(ObjectNode::new(ObjectId(2), Some("RoundedRect".to_owned()), ObjectKind::Rect(rspec), None, Some(rect_outl)));

        // 3. Rotated ArtisticText with custom font_family
        let pos = Point::new(120.0, 260.0).unwrap();
        let mut txt_node = ObjectNode::new(
            ObjectId(3),
            Some("RotatedLabel".to_owned()),
            ObjectKind::ArtisticText {
                text: "CAD Callout".to_owned(),
                font_family: "Courier".to_owned(),
                size_pt: F64::new(16.0).unwrap(),
                position: pos,
            },
            Some(FillStyle::uniform(Color::rgb8(0x22, 0x88, 0x44))),
            None,
        );
        let t0 = Affine2::translation(-pos.x.get(), -pos.y.get()).unwrap();
        let rot = Affine2::rotation_deg(30.0).unwrap();
        let t1 = Affine2::translation(pos.x.get(), pos.y.get()).unwrap();
        txt_node.transform = t0.then(rot).unwrap().then(t1).unwrap();
        objs.push(txt_node);

        let dxf = crate::export_dxf(&doc, 0).expect("export_dxf M84");
        assert!(dxf.contains("0\nTABLE\n2\nLTYPE\n"), "export_dxf must emit TABLES LTYPE section for dashed strokes: {dxf}");
        assert!(dxf.contains("0\nLWPOLYLINE\n"), "export_dxf must emit native LWPOLYLINE with bulge for rounded RectSpec: {dxf}");
        assert!(dxf.contains("\n7\nCourier\n"), "export_dxf must emit group code 7 font style for ArtisticText: {dxf}");
        assert!(dxf.contains("\n50\n330\n"), "export_dxf must emit group code 50 CCW rotation angle for ArtisticText: {dxf}");

        let reimported = coralcraft_doc::import_dxf(&dxf).expect("import_dxf M84");
        let robjs = &reimported.pages.first().unwrap().layers.first().unwrap().objects;
        assert_eq!(robjs.len(), 3);

        let r0_outl = robjs.first().unwrap().outline.as_ref().expect("dashed line outline");
        assert_eq!(r0_outl.dash_pattern, vec![F64::new(8.0).unwrap(), F64::new(4.0).unwrap(), F64::new(2.0).unwrap(), F64::new(4.0).unwrap()]);

        let r1 = robjs.get(1).unwrap();
        let ObjectKind::Rect(r_rect) = &r1.kind else {
            panic!("expected rounded LWPOLYLINE to round-trip to ObjectKind::Rect, got {:?}", r1.kind);
        };
        assert!((r_rect.rect.x0.get() - 40.0).abs() < 1e-3 && (r_rect.rect.y0.get() - 80.0).abs() < 1e-3);
        assert!((r_rect.rect.x1.get() - 200.0).abs() < 1e-3 && (r_rect.rect.y1.get() - 180.0).abs() < 1e-3);
        for rad in &r_rect.radii {
            assert!((rad.get() - 15.0).abs() < 1e-2, "expected corner radius 15.0, got {}", rad.get());
        }
        let r1_outl = r1.outline.as_ref().expect("rounded rect outline");
        assert_eq!(r1_outl.dash_pattern, vec![F64::new(6.0).unwrap(), F64::new(3.0).unwrap()]);

        let r2 = robjs.get(2).unwrap();
        let ObjectKind::ArtisticText { text, font_family, size_pt, position } = &r2.kind else {
            panic!("expected ObjectKind::ArtisticText, got {:?}", r2.kind);
        };
        assert_eq!(text, "CAD Callout");
        assert_eq!(font_family, "Courier");
        assert!((size_pt.get() - 16.0).abs() < 1e-4);
        assert!((position.x.get() - 120.0).abs() < 1e-3 && (position.y.get() - 260.0).abs() < 1e-3);
        let probe_pt = r2.transform.apply_point(Point::new(140.0, 260.0).unwrap()).unwrap();
        let orig_pt = t0.then(rot).unwrap().then(t1).unwrap().apply_point(Point::new(140.0, 260.0).unwrap()).unwrap();
        assert!((probe_pt.x.get() - orig_pt.x.get()).abs() < 1e-3 && (probe_pt.y.get() - orig_pt.y.get()).abs() < 1e-3);
    }

    #[test]
    fn export_svg_and_import_svg_roundtrip_polygon_fountain_midpoint_blend_and_bitmap_dpi_m85() {
        use coralcraft_color::{Color, FountainBlendMode, FountainStop};
        use coralcraft_doc::{Document, FillStyle, FountainGeometry, ObjectId, ObjectKind, ObjectNode, TransparencyStyle};
        use coralcraft_geom::{Affine2, F64, Point, PolygonSpec, Rect, RectSpec, Size};

        let mut doc = Document::new_a4_portrait();
        doc.pages.first_mut().unwrap().size = Size::new(500.0, 400.0).unwrap();
        let objs = &mut doc.pages.first_mut().unwrap().layers.first_mut().unwrap().objects;

        // 1. Star PolygonSpec with non-zero rotation and radial fountain fill (custom midpoint, focal offset, RainbowCw blend)
        let star = PolygonSpec::star(Point::new(150.0, 140.0).unwrap(), 75.25, 32.5, 6, 22.5).unwrap();
        let fountain_fill = FillStyle::Fountain {
            geometry: FountainGeometry::Radial {
                center_x: F64::new(0.4).unwrap(),
                center_y: F64::new(0.6).unwrap(),
                radius: F64::new(0.75).unwrap(),
                focal_x: F64::new(0.25).unwrap(),
                focal_y: F64::new(0.35).unwrap(),
            },
            blend_mode: FountainBlendMode::RainbowCw,
            stops: vec![
                FountainStop::with_midpoint(0.0, 0.3, Color::rgb8(0xFF, 0x20, 0x40)).unwrap(),
                FountainStop::with_midpoint(0.65, 0.7, Color::rgb8(0x20, 0xFF, 0x80)).unwrap(),
                FountainStop::with_midpoint(1.0, 0.5, Color::rgb8(0x20, 0x40, 0xFF)).unwrap(),
            ],
        };
        let mut star_node =
            ObjectNode::new(ObjectId(1), Some("RainbowStar".to_owned()), ObjectKind::Polygon(star), Some(fountain_fill.clone()), None);
        star_node.transparency = Some(TransparencyStyle::Fountain {
            geometry: FountainGeometry::Linear { angle_deg: F64::new(37.5).unwrap(), edge_pad: F64::new(0.15).unwrap() },
            blend_mode: FountainBlendMode::RainbowCcw,
            stops: vec![
                FountainStop::with_midpoint(0.0, 0.25, Color::rgb8(255, 255, 255)).unwrap(),
                FountainStop::with_midpoint(1.0, 0.5, Color::rgb8(0, 0, 0)).unwrap(),
            ],
        });
        objs.push(star_node);

        // 2. Regular PolygonSpec with Square fountain fill
        let reg_poly = PolygonSpec::regular(Point::new(340.0, 140.0).unwrap(), 60.0, 5, 0.0).unwrap();
        let sq_fill = FillStyle::Fountain {
            geometry: FountainGeometry::Square {
                center_x: F64::new(0.35).unwrap(),
                center_y: F64::new(0.65).unwrap(),
                radius: F64::new(0.8).unwrap(),
                angle_deg: F64::new(15.0).unwrap(),
            },
            blend_mode: FountainBlendMode::Direct,
            stops: vec![
                FountainStop::new(0.0, Color::rgb8(0xAA, 0x33, 0x11)).unwrap(),
                FountainStop::new(1.0, Color::rgb8(0x11, 0x88, 0xCC)).unwrap(),
            ],
        };
        objs.push(ObjectNode::new(ObjectId(2), Some("RegularPentagon".to_owned()), ObjectKind::Polygon(reg_poly), Some(sq_fill.clone()), None));

        // 3. High-DPI Bitmap (300x150 DPI) with name, locked=true, and translation transform
        let mut bmp_node = ObjectNode::new(
            ObjectId(3),
            Some("HiDpiPhoto".to_owned()),
            ObjectKind::Bitmap {
                pixel_width: 4,
                pixel_height: 2,
                dpi_x: F64::new(300.0).unwrap(),
                dpi_y: F64::new(150.0).unwrap(),
                rgba_pixels: [255, 0, 0, 255].repeat(8),
            },
            None,
            None,
        );
        bmp_node.locked = true;
        bmp_node.transform = Affine2::translation(60.0, 260.0).unwrap();
        let _ = RectSpec::sharp(Rect::new(0.0, 0.0, 10.0, 10.0).unwrap());
        objs.push(bmp_node);

        let svg = crate::export_svg(&doc, 0).expect("export_svg M85");
        assert!(svg.contains("data-polygon="), "export_svg must emit data-polygon metadata: {svg}");
        assert!(svg.contains("data-fountain-geom="), "export_svg must emit data-fountain-geom metadata: {svg}");
        assert!(svg.contains("data-blend-mode=\"rainbow-cw\""), "export_svg must emit data-blend-mode: {svg}");
        assert!(svg.contains("data-midpoint=\"0.3\""), "export_svg must emit stop data-midpoint: {svg}");
        assert!(svg.contains("data-dpi-x=\"300\""), "export_svg must emit bitmap data-dpi-x: {svg}");

        let reimported = coralcraft_doc::import_svg(&svg).expect("import_svg M85");
        let robjs = &reimported.pages.first().unwrap().layers.first().unwrap().objects;
        assert_eq!(robjs.len(), 3);

        let r0 = robjs.first().unwrap();
        assert_eq!(r0.name.as_deref(), Some("RainbowStar"));
        assert_eq!(r0.kind, ObjectKind::Polygon(star));
        assert_eq!(r0.fill, Some(fountain_fill));
        assert_eq!(
            r0.transparency,
            Some(TransparencyStyle::Fountain {
                geometry: FountainGeometry::Linear { angle_deg: F64::new(37.5).unwrap(), edge_pad: F64::new(0.15).unwrap() },
                blend_mode: FountainBlendMode::RainbowCcw,
                stops: vec![
                    FountainStop::with_midpoint(0.0, 0.25, Color::rgb8(255, 255, 255)).unwrap(),
                    FountainStop::with_midpoint(1.0, 0.5, Color::rgb8(0, 0, 0)).unwrap(),
                ],
            })
        );

        let r1 = robjs.get(1).unwrap();
        assert_eq!(r1.name.as_deref(), Some("RegularPentagon"));
        assert_eq!(r1.kind, ObjectKind::Polygon(reg_poly));
        assert_eq!(r1.fill, Some(sq_fill));

        let r2 = robjs.get(2).unwrap();
        assert_eq!(r2.name.as_deref(), Some("HiDpiPhoto"));
        assert!(r2.locked);
        assert_eq!(r2.transform, Affine2::translation(60.0, 260.0).unwrap());
        assert_eq!(
            r2.kind,
            ObjectKind::Bitmap {
                pixel_width: 4,
                pixel_height: 2,
                dpi_x: F64::new(300.0).unwrap(),
                dpi_y: F64::new(150.0).unwrap(),
                rgba_pixels: [255, 0, 0, 255].repeat(8),
            }
        );
    }

    #[test]
    fn export_svg_and_import_svg_roundtrip_doc_layer_meta_lens_and_cmyk_spot_colors_m86() {
        use coralcraft_color::{Color, FountainBlendMode, FountainStop};
        use coralcraft_doc::{
            Document, FillStyle, FountainGeometry, Layer, LayerId, LayerRole, LensKind, ObjectId, ObjectKind, ObjectNode, OutlineStyle, PatternKind,
            TransparencyStyle,
        };
        use coralcraft_geom::{F64, Rect, RectSpec, Size, Unit};

        let mut doc = Document::new_a4_portrait();
        doc.title = "Prepress & Spot Spec".to_owned();
        doc.units = Unit::Inch;
        let bg_cmyk = Color::cmyk(0.05, 0.02, 0.0, 0.0).unwrap();
        {
            let page = doc.pages.first_mut().unwrap();
            page.name = "Cover <Sheet>".to_owned();
            page.size = Size::new(500.0, 400.0).unwrap();
            page.bleed = F64::new(9.5).unwrap();
            page.background = Some(FillStyle::uniform(bg_cmyk.clone()));
            let layer1 = page.layers.first_mut().unwrap();
            layer1.name = "Plate & Spot Layer".to_owned();
            layer1.locked = true;
            layer1.opacity = F64::new(0.9).unwrap();
        }

        // Object 1: Spot fill + Registration outline + separate node.opacity (0.8) & TransparencyStyle::Uniform (0.5)
        let spot_col = Color::spot("PANTONE 186 C, Special", 0.75, [0.0, 1.0, 0.81, 0.04]).unwrap();
        let mut obj1 = ObjectNode::new(
            ObjectId(1),
            Some("SpotCard".to_owned()),
            ObjectKind::Rect(RectSpec::sharp(Rect::new(20.0, 30.0, 140.0, 110.0).unwrap())),
            Some(FillStyle::uniform(spot_col.clone())),
            Some(OutlineStyle::with_width(Color::Registration, F64::new(2.0).unwrap())),
        );
        obj1.opacity = F64::new(0.8).unwrap();
        obj1.transparency = Some(TransparencyStyle::Uniform { opacity: F64::new(0.5).unwrap() });

        // Object 2: Fountain fill with CMYK & HSB stops + Tinted Lens transparency
        let cmyk_stop = Color::cmyk(0.8, 0.1, 0.0, 0.05).unwrap();
        let hsb_stop = Color::hsb(210.0, 0.75, 0.9).unwrap();
        let ftn_fill = FillStyle::Fountain {
            geometry: FountainGeometry::Linear { angle_deg: F64::new(30.0).unwrap(), edge_pad: F64::ZERO },
            blend_mode: FountainBlendMode::Direct,
            stops: vec![FountainStop::new(0.0, cmyk_stop.clone()).unwrap(), FountainStop::new(1.0, hsb_stop.clone()).unwrap()],
        };
        let lens_tint_col = Color::cmyk(0.0, 0.85, 0.7, 0.0).unwrap();
        let mut obj2 = ObjectNode::new(
            ObjectId(2),
            Some("CmykHsbFountain".to_owned()),
            ObjectKind::Rect(RectSpec::sharp(Rect::new(160.0, 30.0, 280.0, 110.0).unwrap())),
            Some(ftn_fill.clone()),
            None,
        );
        obj2.transparency = Some(TransparencyStyle::Lens { kind: LensKind::Tinted { color: lens_tint_col.clone(), rate: F64::new(0.6).unwrap() } });

        // Object 3: TwoColorPattern with Gray fg and LabD50 bg + Magnify Lens
        let gray_fg = Color::gray(0.25).unwrap();
        let lab_bg = Color::lab_d50(85.0, -5.0, 12.0).unwrap();
        let pat_fill =
            FillStyle::TwoColorPattern { kind: PatternKind::PolkaDot, fg_color: gray_fg, bg_color: lab_bg, tile_size_pt: F64::new(16.0).unwrap() };
        let mut obj3 = ObjectNode::new(
            ObjectId(3),
            Some("PatternLens".to_owned()),
            ObjectKind::Rect(RectSpec::sharp(Rect::new(300.0, 30.0, 420.0, 110.0).unwrap())),
            Some(pat_fill.clone()),
            None,
        );
        obj3.transparency = Some(TransparencyStyle::Lens { kind: LensKind::Magnify { factor: F64::new(2.25).unwrap() } });

        {
            let objs = &mut doc.pages.first_mut().unwrap().layers.first_mut().unwrap().objects;
            objs.push(obj1);
            objs.push(obj2);
            objs.push(obj3);
        }

        // Foreground Master Layer in doc.master_layers
        let mut master_fg = Layer::new_normal(LayerId(10), "Watermark Master");
        master_fg.role = LayerRole::MasterAllPages;
        master_fg.master_in_front = true;
        master_fg.locked = true;
        let mut watermark = ObjectNode::new(
            ObjectId(4),
            Some("InvertLensWatermark".to_owned()),
            ObjectKind::Rect(RectSpec::sharp(Rect::new(50.0, 200.0, 250.0, 300.0).unwrap())),
            None,
            Some(OutlineStyle::hairline(Color::cmyk(0.0, 0.0, 0.0, 1.0).unwrap())),
        );
        watermark.transparency = Some(TransparencyStyle::Lens { kind: LensKind::Invert });
        master_fg.objects.push(watermark);
        doc.master_layers.push(master_fg);

        let svg = crate::export_svg(&doc, 0).expect("export_svg M86");
        assert!(svg.contains("data-title=\"Prepress &amp; Spot Spec\""), "SVG must emit data-title: {svg}");
        assert!(svg.contains("data-units=\"inch\""), "SVG must emit data-units: {svg}");
        assert!(svg.contains("data-page-bleed=\"9.5\""), "SVG must emit data-page-bleed: {svg}");
        assert!(svg.contains("data-lens="), "SVG must emit data-lens metadata: {svg}");
        assert!(svg.contains("data-fill-color="), "SVG must emit data-fill-color metadata: {svg}");

        let reimported = coralcraft_doc::import_svg(&svg).expect("import_svg M86");
        assert_eq!(reimported.title, "Prepress & Spot Spec");
        assert_eq!(reimported.units, Unit::Inch);
        let rpage = reimported.pages.first().unwrap();
        assert_eq!(rpage.name, "Cover <Sheet>");
        assert_eq!(rpage.bleed, F64::new(9.5).unwrap());
        assert_eq!(rpage.background, Some(FillStyle::uniform(bg_cmyk)));

        assert_eq!(rpage.layers.len(), 1);
        let rl1 = rpage.layers.first().unwrap();
        assert_eq!(rl1.name, "Plate & Spot Layer");
        assert!(rl1.locked);
        assert_eq!(rl1.opacity, F64::new(0.9).unwrap());
        assert_eq!(rl1.objects.len(), 3);

        let r1 = rl1.objects.first().unwrap();
        assert_eq!(r1.fill, Some(FillStyle::uniform(spot_col)));
        assert_eq!(r1.outline.as_ref().unwrap().color, Color::Registration);
        assert_eq!(r1.opacity, F64::new(0.8).unwrap());
        assert_eq!(r1.transparency, Some(TransparencyStyle::Uniform { opacity: F64::new(0.5).unwrap() }));

        let r2 = rl1.objects.get(1).unwrap();
        assert_eq!(r2.fill, Some(ftn_fill));
        assert_eq!(r2.transparency, Some(TransparencyStyle::Lens { kind: LensKind::Tinted { color: lens_tint_col, rate: F64::new(0.6).unwrap() } }));

        let r3 = rl1.objects.get(2).unwrap();
        assert_eq!(r3.fill, Some(pat_fill));
        assert_eq!(r3.transparency, Some(TransparencyStyle::Lens { kind: LensKind::Magnify { factor: F64::new(2.25).unwrap() } }));

        let rmaster = reimported.master_layers.iter().find(|l| l.name == "Watermark Master").expect("Watermark Master layer in doc.master_layers");
        assert_eq!(rmaster.role, LayerRole::MasterAllPages);
        assert!(rmaster.master_in_front);
        assert!(rmaster.locked);
        assert_eq!(rmaster.objects.len(), 1);
        assert_eq!(rmaster.objects.first().unwrap().transparency, Some(TransparencyStyle::Lens { kind: LensKind::Invert }));
    }

    #[test]
    fn export_dxf_and_import_dxf_roundtrip_group_block_insert_cubic_spline_and_mtext_m86() {
        use coralcraft_color::Color;
        use coralcraft_doc::{Document, FillStyle, ObjectId, ObjectKind, ObjectNode, OutlineStyle, ParagraphTextSpec, TextAlignment};
        use coralcraft_geom::{Affine2, F64, Path, PathElement, Point, Rect, Size};

        let mut doc = Document::new_a4_portrait();
        doc.pages.first_mut().unwrap().size = Size::new(600.0, 400.0).unwrap();
        let objs = &mut doc.pages.first_mut().unwrap().layers.first_mut().unwrap().objects;

        // 1. Group with scale(1.5, 1.5) + rotation(30 deg) + translation(120, 80) containing a Circle and a Line
        let grp_circle = ObjectNode::new(
            ObjectId(1),
            Some("PinCircle".to_owned()),
            ObjectKind::Ellipse(coralcraft_geom::EllipseSpec::full(Rect::new(10.0, 10.0, 50.0, 50.0).unwrap())),
            None,
            Some(OutlineStyle::with_width(Color::rgb8(0x20, 0x60, 0xC0), F64::new(1.5).unwrap())),
        );
        let mut grp = ObjectNode::new(ObjectId(2), Some("SensorAssembly".to_owned()), ObjectKind::Group { children: vec![grp_circle] }, None, None);
        let scale_tf = Affine2::scaling(1.5, 1.5).unwrap();
        let rot_tf = Affine2::rotation_deg(30.0).unwrap();
        let tr_tf = Affine2::translation(120.0, 80.0).unwrap();
        grp.transform = scale_tf.then(rot_tf).unwrap().then(tr_tf).unwrap();
        objs.push(grp);

        // 2. Curved Cubic Bezier Path emitted as native DXF SPLINE
        let mut curve_path = Path::new();
        curve_path.move_to(Point::new(40.0, 220.0).unwrap()).unwrap();
        curve_path.cubic_to(Point::new(80.0, 160.0).unwrap(), Point::new(140.0, 280.0).unwrap(), Point::new(180.0, 220.0).unwrap()).unwrap();
        curve_path.cubic_to(Point::new(210.0, 180.0).unwrap(), Point::new(250.0, 200.0).unwrap(), Point::new(280.0, 240.0).unwrap()).unwrap();
        objs.push(ObjectNode::new(
            ObjectId(3),
            Some("WaveSpline".to_owned()),
            ObjectKind::Path(curve_path.clone()),
            None,
            Some(OutlineStyle::with_width(Color::rgb8(0xCC, 0x20, 0x50), F64::new(2.0).unwrap())),
        ));

        // 3. ParagraphText frame emitted as native DXF MTEXT
        let ptxt_spec = ParagraphTextSpec::new(
            Point::new(320.0, 180.0).unwrap(),
            200.0,
            90.0,
            "First CAD paragraph line\nSecond CAD paragraph line".to_owned(),
            "Courier",
            14.0,
            1.3,
            TextAlignment::Center,
            2,
            12.0,
        )
        .unwrap();
        objs.push(ObjectNode::new(
            ObjectId(4),
            Some("SpecNotes".to_owned()),
            ObjectKind::ParagraphText(Box::new(ptxt_spec.clone())),
            Some(FillStyle::uniform(Color::rgb8(0x10, 0x70, 0x30))),
            None,
        ));

        let dxf = crate::export_dxf(&doc, 0).expect("export_dxf M86");
        assert!(dxf.contains("0\nSECTION\n2\nBLOCKS\n"), "export_dxf must emit BLOCKS section for ObjectKind::Group: {dxf}");
        assert!(dxf.contains("0\nINSERT\n"), "export_dxf must emit INSERT entity for ObjectKind::Group: {dxf}");
        assert!(dxf.contains("0\nSPLINE\n"), "export_dxf must emit native SPLINE entity for cubic Bezier Path: {dxf}");
        assert!(dxf.contains("0\nMTEXT\n"), "export_dxf must emit native MTEXT entity for ParagraphText: {dxf}");

        let reimported = coralcraft_doc::import_dxf(&dxf).expect("import_dxf M86");
        let robjs = &reimported.pages.first().unwrap().layers.first().unwrap().objects;
        assert_eq!(robjs.len(), 3);

        // 1. Group round-trip
        let r0 = robjs.first().unwrap();
        assert_eq!(r0.name.as_deref(), Some("SensorAssembly"));
        let ObjectKind::Group { children: r_children } = &r0.kind else {
            panic!("expected ObjectKind::Group from DXF INSERT, got {:?}", r0.kind);
        };
        assert_eq!(r_children.len(), 1);
        let orig_pt = grp_transform_probe(scale_tf, rot_tf, tr_tf, Point::new(30.0, 30.0).unwrap());
        let reimp_pt = r0.transform.apply_point(Point::new(30.0, 30.0).unwrap()).unwrap();
        assert!((orig_pt.x.get() - reimp_pt.x.get()).abs() < 1e-2 && (orig_pt.y.get() - reimp_pt.y.get()).abs() < 1e-2);

        // 2. Cubic Bezier SPLINE round-trip
        let r1 = robjs.get(1).unwrap();
        let ObjectKind::Path(r_path) = &r1.kind else {
            panic!("expected ObjectKind::Path from DXF SPLINE, got {:?}", r1.kind);
        };
        assert_eq!(r_path.elements.len(), 3);
        assert!(matches!(r_path.elements.get(1), Some(PathElement::CubicTo { .. })));
        assert_eq!(r_path, &curve_path);

        // 3. ParagraphText MTEXT round-trip
        let r2 = robjs.get(2).unwrap();
        let ObjectKind::ParagraphText(r_ptxt) = &r2.kind else {
            panic!("expected ObjectKind::ParagraphText from DXF MTEXT, got {:?}", r2.kind);
        };
        assert_eq!(r_ptxt.text, "First CAD paragraph line\nSecond CAD paragraph line");
        assert_eq!(r_ptxt.font_family, "Courier");
        assert!((r_ptxt.font_size_pt.get() - 14.0).abs() < 1e-4);
        assert!((r_ptxt.frame_width_pt.get() - 200.0).abs() < 1e-3);
        assert!((r_ptxt.frame_height_pt.get() - 90.0).abs() < 1e-3);
        assert_eq!(r_ptxt.alignment, TextAlignment::Center);
        assert_eq!(r_ptxt.columns, 2);
        assert!((r_ptxt.gutter_pt.get() - 12.0).abs() < 1e-3);
        assert!((r_ptxt.line_spacing_multiplier.get() - 1.3).abs() < 1e-3);
    }

    fn grp_transform_probe(s: Affine2, r: Affine2, t: Affine2, pt: coralcraft_geom::Point) -> coralcraft_geom::Point {
        s.then(r).unwrap().then(t).unwrap().apply_point(pt).unwrap()
    }

    #[test]
    fn export_svg_and_import_svg_roundtrip_live_effects_hairline_exact_transform_palette_residuals_and_unsupported_m87() {
        use coralcraft_color::{Color, Palette, PaletteSwatch};
        use coralcraft_doc::{
            ChunkTag, Document, FillStyle, ObjectId, ObjectKind, ObjectNode, OutlineStyle, RawChunk, RawPayload, UnsupportedObject, VectorEffect,
        };
        use coralcraft_geom::{Affine2, CornerKind, DistortionKind, F64, Path, Point, Rect, RectSpec, Size};

        let mut doc = Document::new_a4_portrait();
        doc.palette = Palette {
            name: "Custom Studio Palette".to_owned(),
            guid: Some("00112233-4455-6677-8899-aabbccddeeff".to_owned()),
            swatches: vec![
                PaletteSwatch { name: "Brand Spot".to_owned(), color: Color::spot("PANTONE 300 C", 0.85, [0.99, 0.5, 0.0, 0.0]).unwrap() },
                PaletteSwatch { name: "Process Gold".to_owned(), color: Color::cmyk(0.05, 0.2, 0.9, 0.0).unwrap() },
            ],
        };
        doc.residual.chunks.push(RawChunk {
            tag: ChunkTag(*b"dres"),
            flags: 1,
            slot: 7,
            payload: RawPayload(vec![0x10, 0x20, 0x30]),
            src_offset: 64,
        });
        {
            let page = doc.pages.first_mut().unwrap();
            page.size = Size::new(600.0, 450.0).unwrap();
            page.background = Some(FillStyle::Unknown { tag: ChunkTag(*b"bguk"), payload: RawPayload(vec![1, 2, 3, 4]) });
            page.residual.chunks.push(RawChunk {
                tag: ChunkTag(*b"pres"),
                flags: 2,
                slot: 0,
                payload: RawPayload(vec![0xAA, 0xBB]),
                src_offset: 128,
            });
            page.layers.first_mut().unwrap().residual.chunks.push(RawChunk {
                tag: ChunkTag(*b"lres"),
                flags: 3,
                slot: 2,
                payload: RawPayload(vec![0xCC, 0xDD, 0xEE]),
                src_offset: 192,
            });
        }

        // 1. Parametric RectSpec with live VectorEffect stack (Envelope + Distortion + Contour + DropShadow + Extrude),
        //    hairline stroke, non-dyadic exact Affine2 transform, and object residual
        let orig_rect = RectSpec::uniform(Rect::new(40.0, 50.0, 160.0, 130.0).unwrap(), 10.0, CornerKind::Chamfered).unwrap();
        let exact_tf = Affine2::new(1.0 / 3.0, 0.125, -0.25, 2.0 / 3.0, 12.3456789012345, 34.5678901234567).unwrap();
        let mut obj1 = ObjectNode::new(
            ObjectId(1),
            Some("LiveEffectRect".to_owned()),
            ObjectKind::Rect(orig_rect),
            Some(FillStyle::uniform(Color::rgb8(0x33, 0x77, 0xCC))),
            Some(OutlineStyle::hairline(Color::rgb8(0x11, 0x22, 0x33))),
        );
        obj1.transform = exact_tf;
        obj1.effects = vec![
            VectorEffect::Envelope {
                corners: [
                    Point::new(35.0, 45.0).unwrap(),
                    Point::new(165.0, 40.0).unwrap(),
                    Point::new(170.0, 135.0).unwrap(),
                    Point::new(30.0, 140.0).unwrap(),
                ],
            },
            VectorEffect::Distortion { kind: DistortionKind::PushPull, amplitude: F64::new(6.5).unwrap(), frequency: F64::new(2.0).unwrap() },
            VectorEffect::Contour { steps: 3, offset_pt: F64::new(4.25).unwrap(), inside: false },
            VectorEffect::DropShadow {
                offset_x: F64::new(5.0).unwrap(),
                offset_y: F64::new(6.0).unwrap(),
                opacity: F64::new(0.65).unwrap(),
                color: Color::cmyk(0.0, 0.0, 0.0, 0.8).unwrap(),
            },
            VectorEffect::Extrude { depth_pt: F64::new(12.0).unwrap(), angle_deg: F64::new(45.0).unwrap() },
        ];
        obj1.residual.chunks.push(RawChunk { tag: ChunkTag(*b"ores"), flags: 0, slot: 9, payload: RawPayload(vec![9, 8, 7]), src_offset: 256 });

        // 2. ArtisticText with TextOnPath live effect + FillStyle::Unknown
        let mut spine = Path::new();
        spine.move_to(Point::new(50.0, 220.0).unwrap()).unwrap();
        spine.cubic_to(Point::new(120.0, 180.0).unwrap(), Point::new(200.0, 260.0).unwrap(), Point::new(280.0, 220.0).unwrap()).unwrap();
        let mut obj2 = ObjectNode::new(
            ObjectId(2),
            Some("CurvedHeadline".to_owned()),
            ObjectKind::ArtisticText {
                text: "Curved Banner".to_owned(),
                font_family: "Helvetica".to_owned(),
                size_pt: F64::new(18.0).unwrap(),
                position: Point::new(50.0, 220.0).unwrap(),
            },
            Some(FillStyle::Unknown { tag: ChunkTag(*b"mesh"), payload: RawPayload(vec![0x42, 0x43, 0x44]) }),
            None,
        );
        obj2.effects = vec![VectorEffect::TextOnPath { spine: spine.clone(), offset_pt: F64::new(3.5).unwrap() }];

        // 3. ObjectKind::Unsupported with fallback_bounds, child node, and nested residual
        let unsup_child = ObjectNode::new(
            ObjectId(3),
            Some("InnerUnsupChild".to_owned()),
            ObjectKind::Rect(RectSpec::sharp(Rect::new(310.0, 60.0, 350.0, 100.0).unwrap())),
            Some(FillStyle::uniform(Color::rgb8(200, 100, 50))),
            None,
        );
        let mut unsup_res = coralcraft_doc::Residual::default();
        unsup_res.chunks.push(RawChunk { tag: ChunkTag(*b"ures"), flags: 4, slot: 0, payload: RawPayload(vec![0x55, 0x66]), src_offset: 512 });
        let unsup = UnsupportedObject {
            tag: ChunkTag(*b"smsh"),
            class_hint: Some("SmartMesh".to_owned()),
            payload: RawPayload(vec![0xDE, 0xAD, 0xBE, 0xEF]),
            fallback_bounds: Some(Rect::new(300.0, 50.0, 420.0, 140.0).unwrap()),
            absolute_geometry: true,
            children: vec![unsup_child],
            residual: unsup_res,
        };
        let obj3 = ObjectNode::new(ObjectId(4), Some("OpaqueMeshNode".to_owned()), ObjectKind::Unsupported(unsup.clone()), None, None);

        {
            let objs = &mut doc.pages.first_mut().unwrap().layers.first_mut().unwrap().objects;
            objs.push(obj1.clone());
            objs.push(obj2.clone());
            objs.push(obj3.clone());
        }

        let svg = crate::export_svg(&doc, 0).expect("export_svg M87");
        assert!(svg.contains("data-effects="), "export_svg must emit data-effects: {svg}");
        assert!(svg.contains("data-hairline=\"1\""), "export_svg must emit data-hairline: {svg}");
        assert!(svg.contains("data-palette="), "export_svg must emit data-palette: {svg}");
        assert!(svg.contains("data-unsupported="), "export_svg must emit data-unsupported: {svg}");

        let reimported = coralcraft_doc::import_svg(&svg).expect("import_svg M87");
        assert_eq!(reimported.palette, doc.palette);
        assert_eq!(reimported.residual, doc.residual);

        let rpage = reimported.pages.first().unwrap();
        assert_eq!(rpage.background, doc.pages.first().unwrap().background);
        assert_eq!(rpage.residual, doc.pages.first().unwrap().residual);

        let rlayer = rpage.layers.first().unwrap();
        assert_eq!(rlayer.residual, doc.pages.first().unwrap().layers.first().unwrap().residual);
        assert_eq!(rlayer.objects.len(), 3);

        let r1 = rlayer.objects.first().unwrap();
        assert_eq!(r1.name.as_deref(), Some("LiveEffectRect"));
        assert_eq!(r1.kind, ObjectKind::Rect(orig_rect));
        assert_eq!(r1.transform, exact_tf, "exact Affine2 matrix must round-trip bit-exactly");
        assert!(r1.outline.as_ref().unwrap().is_hairline(), "hairline stroke width_pt == 0.0 must be preserved");
        assert_eq!(r1.effects, obj1.effects);
        assert_eq!(r1.residual, obj1.residual);

        let r2 = rlayer.objects.get(1).unwrap();
        assert_eq!(r2.name.as_deref(), Some("CurvedHeadline"));
        assert_eq!(r2.kind, obj2.kind);
        assert_eq!(r2.fill, obj2.fill);
        assert!(r2.outline.is_none(), "FillStyle::Unknown must not synthesize a spurious hairline outline");
        assert_eq!(r2.effects, obj2.effects);

        let r3 = rlayer.objects.get(2).unwrap();
        assert_eq!(r3.name.as_deref(), Some("OpaqueMeshNode"));
        assert_eq!(r3.kind, ObjectKind::Unsupported(unsup));
    }

    #[test]
    fn export_dxf_and_import_dxf_roundtrip_layer_table_xdata_effects_powerclip_palette_residuals_and_unsupported_m87() {
        use coralcraft_color::{Color, Palette, PaletteSwatch};
        use coralcraft_doc::{
            ArrowheadKind, ChunkTag, Document, FillStyle, Layer, LayerId, LayerRole, LensKind, LineCap, LineJoin, ObjectId, ObjectKind, ObjectNode,
            OutlineStyle, PatternKind, RawChunk, RawPayload, TransparencyStyle, UnsupportedObject, VectorEffect,
        };
        use coralcraft_geom::{Affine2, CornerKind, EllipseSpec, F64, Point, PolygonSpec, Rect, RectSpec, Size, Unit};

        let mut doc = Document::new_a4_portrait();
        doc.title = "DXF Industrial Spec".to_owned();
        doc.units = Unit::Inch;
        doc.palette = Palette {
            name: "CAD Spot Swatches".to_owned(),
            guid: None,
            swatches: vec![PaletteSwatch { name: "Laser Red".to_owned(), color: Color::spot("LASER CUT", 1.0, [0.0, 1.0, 1.0, 0.0]).unwrap() }],
        };
        doc.residual.chunks.push(RawChunk { tag: ChunkTag(*b"ddxf"), flags: 1, slot: 3, payload: RawPayload(vec![0x11, 0x22]), src_offset: 32 });

        let page_bg = FillStyle::TwoColorPattern {
            kind: PatternKind::DiagonalCrosshatch,
            fg_color: Color::cmyk(0.1, 0.0, 0.0, 0.05).unwrap(),
            bg_color: Color::WHITE,
            tile_size_pt: F64::new(24.0).unwrap(),
        };
        {
            let page = doc.pages.first_mut().unwrap();
            page.name = "Sheet 1 - Chassis".to_owned();
            page.size = Size::new(720.0, 540.0).unwrap();
            page.bleed = F64::new(6.0).unwrap();
            page.background = Some(page_bg.clone());
            page.residual.chunks.push(RawChunk { tag: ChunkTag(*b"pdxf"), flags: 2, slot: 0, payload: RawPayload(vec![0x33, 0x44]), src_offset: 96 });

            let l1 = page.layers.first_mut().unwrap();
            l1.name = "Cut & Etch Layer".to_owned();
            l1.locked = true;
            l1.opacity = F64::new(0.85).unwrap();
            l1.residual.chunks.push(RawChunk { tag: ChunkTag(*b"ldxf"), flags: 5, slot: 1, payload: RawPayload(vec![0x77]), src_offset: 160 });
        }

        // Object 1: Parametric Star PolygonSpec with live Envelope + Contour effects, PowerClip child, Lens transparency, and arrowheads
        let star_spec = PolygonSpec::star(Point::new(150.0, 160.0).unwrap(), 80.0, 35.0, 5, 18.0).unwrap();
        let mut outl1 = OutlineStyle::with_width(Color::spot("CUT LINE", 1.0, [0.0, 1.0, 0.0, 0.0]).unwrap(), F64::new(2.5).unwrap());
        outl1.cap = LineCap::Round;
        outl1.join = LineJoin::Bevel;
        outl1.behind_fill = true;
        outl1.scale_with_object = false;
        outl1.start_arrow = ArrowheadKind::Circle;
        outl1.end_arrow = ArrowheadKind::Triangle;

        let pc_inner = ObjectNode::new(
            ObjectId(1),
            Some("ClippedPie".to_owned()),
            ObjectKind::Ellipse(EllipseSpec::pie(Rect::new(110.0, 120.0, 190.0, 200.0).unwrap(), 15.0, 240.0).unwrap()),
            Some(FillStyle::uniform(Color::cmyk(0.7, 0.1, 0.0, 0.0).unwrap())),
            None,
        );

        let mut obj1 = ObjectNode::new(
            ObjectId(2),
            Some("PowerClippedStar".to_owned()),
            ObjectKind::Polygon(star_spec),
            Some(FillStyle::uniform(Color::hsb(45.0, 0.9, 0.95).unwrap())),
            Some(outl1.clone()),
        );
        obj1.locked = true;
        obj1.opacity = F64::new(0.9).unwrap();
        obj1.transform = Affine2::new(1.1, 0.15, -0.1, 0.95, 25.0, 15.0).unwrap();
        obj1.transparency = Some(TransparencyStyle::Lens { kind: LensKind::Brighten { rate: F64::new(0.35).unwrap() } });
        obj1.effects = vec![VectorEffect::Contour { steps: 2, offset_pt: F64::new(3.0).unwrap(), inside: true }];
        obj1.powerclip_children = vec![pc_inner];
        obj1.residual.chunks.push(RawChunk { tag: ChunkTag(*b"odxf"), flags: 7, slot: 4, payload: RawPayload(vec![0x99, 0xAA]), src_offset: 300 });

        // Object 2: Embedded Bitmap + Object 3: UnsupportedObject on a second layer
        let obj2 = ObjectNode::new(
            ObjectId(3),
            Some("CalibrationBitmap".to_owned()),
            ObjectKind::Bitmap {
                pixel_width: 2,
                pixel_height: 2,
                dpi_x: F64::new(144.0).unwrap(),
                dpi_y: F64::new(144.0).unwrap(),
                rgba_pixels: vec![255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255],
            },
            None,
            None,
        );

        let unsup = UnsupportedObject {
            tag: ChunkTag(*b"cadx"),
            class_hint: Some("CustomCadEntity".to_owned()),
            payload: RawPayload(vec![10, 20, 30, 40, 50]),
            fallback_bounds: Some(Rect::new(400.0, 100.0, 500.0, 180.0).unwrap()),
            absolute_geometry: false,
            children: Vec::new(),
            residual: coralcraft_doc::Residual::default(),
        };
        let obj3 = ObjectNode::new(
            ObjectId(4),
            Some("UnsupportedCadNode".to_owned()),
            ObjectKind::Unsupported(unsup.clone()),
            Some(FillStyle::Unknown { tag: ChunkTag(*b"ufdx"), payload: RawPayload(vec![7, 7, 7]) }),
            Some(OutlineStyle::hairline(Color::Registration)),
        );

        {
            let page = doc.pages.first_mut().unwrap();
            page.layers.first_mut().unwrap().objects.push(obj1.clone());
            let mut layer2 = Layer::new_normal(LayerId(5), "Bitmaps & Custom");
            layer2.printable = false;
            layer2.objects.push(obj2.clone());
            layer2.objects.push(obj3.clone());
            page.layers.push(layer2);
        }

        // Master layer in doc.master_layers
        let mut master = Layer::new_normal(LayerId(10), "Border Master");
        master.role = LayerRole::MasterAllPages;
        master.master_in_front = true;
        master.objects.push(ObjectNode::new(
            ObjectId(5),
            Some("MasterFrame".to_owned()),
            ObjectKind::Rect(RectSpec::uniform(Rect::new(10.0, 10.0, 710.0, 530.0).unwrap(), 8.0, CornerKind::Scalloped).unwrap()),
            None,
            Some(OutlineStyle::hairline(Color::BLACK)),
        ));
        doc.master_layers.push(master);

        let dxf = crate::export_dxf(&doc, 0).expect("export_dxf M87");
        assert!(dxf.contains("0\nTABLE\n2\nLAYER\n"), "export_dxf must emit TABLES LAYER table: {dxf}");
        assert!(dxf.contains("1001\nCORALCRAFT\n"), "export_dxf must emit 1001 CORALCRAFT XDATA: {dxf}");

        let reimported = coralcraft_doc::import_dxf(&dxf).expect("import_dxf M87");
        assert_eq!(reimported.title, "DXF Industrial Spec");
        assert_eq!(reimported.units, Unit::Inch);
        assert_eq!(reimported.palette, doc.palette);
        assert_eq!(reimported.residual, doc.residual);

        let rpage = reimported.pages.first().unwrap();
        assert_eq!(rpage.name, "Sheet 1 - Chassis");
        assert_eq!(rpage.bleed, F64::new(6.0).unwrap());
        assert_eq!(rpage.background, Some(page_bg));
        assert_eq!(rpage.residual, doc.pages.first().unwrap().residual);
        assert_eq!(rpage.layers.len(), 2);

        let rl1 = rpage.layers.first().unwrap();
        assert_eq!(rl1.name, "Cut & Etch Layer");
        assert!(rl1.locked);
        assert_eq!(rl1.opacity, F64::new(0.85).unwrap());
        assert_eq!(rl1.residual, doc.pages.first().unwrap().layers.first().unwrap().residual);
        assert_eq!(rl1.objects.len(), 1);

        let robj1 = rl1.objects.first().unwrap();
        assert_eq!(robj1.name.as_deref(), Some("PowerClippedStar"));
        assert_eq!(robj1.kind, ObjectKind::Polygon(star_spec));
        assert_eq!(robj1.fill, obj1.fill);
        assert_eq!(robj1.outline, obj1.outline);
        assert_eq!(robj1.transform, obj1.transform);
        assert_eq!(robj1.transparency, obj1.transparency);
        assert_eq!(robj1.effects, obj1.effects);
        assert_eq!(robj1.powerclip_children.len(), 1);
        assert_eq!(robj1.powerclip_children.first().unwrap().kind, obj1.powerclip_children.first().unwrap().kind);
        assert_eq!(robj1.residual, obj1.residual);

        let rl2 = rpage.layers.get(1).unwrap();
        assert_eq!(rl2.name, "Bitmaps & Custom");
        assert!(!rl2.printable);
        assert_eq!(rl2.objects.len(), 2);
        assert_eq!(rl2.objects.first().unwrap().kind, obj2.kind);
        assert_eq!(rl2.objects.get(1).unwrap().kind, ObjectKind::Unsupported(unsup));
        assert_eq!(rl2.objects.get(1).unwrap().fill, obj3.fill);
        assert!(rl2.objects.get(1).unwrap().outline.as_ref().unwrap().is_hairline());

        let rmaster = reimported.master_layers.iter().find(|l| l.name == "Border Master").expect("Border Master layer in doc.master_layers");
        assert_eq!(rmaster.role, LayerRole::MasterAllPages);
        assert!(rmaster.master_in_front);
        assert_eq!(rmaster.objects.len(), 1);
    }
}
