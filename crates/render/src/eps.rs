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
        emit_eps_stroke(out, &emit_ps_path, outline, world_tf);
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
            FillStyle::Uniform { color } | FillStyle::TwoColorPattern { fg_color: color, .. } => {
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
        emit_eps_stroke(out, &emit_ps_path, outline, world_tf);
    }

    Ok(())
}

fn emit_eps_stroke(out: &mut String, emit_ps_path: &dyn Fn(&mut String), outline: &OutlineStyle, world_tf: Affine2) {
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
    let layers = doc.effective_layers_for_page(page_index, true)?;
    let page = doc.pages.get(page_index).ok_or(coralcraft_doc::DocError::PageOutOfRange { index: page_index, count: doc.pages.len() })?;
    let page_h = page.size.height.get();

    let mut out = String::new();
    out.push_str("0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1009\n0\nENDSEC\n");
    out.push_str("0\nSECTION\n2\nENTITIES\n");

    for layer in layers {
        if !layer.visible || layer.opacity.get() <= 0.0 {
            continue;
        }
        let safe_layer_name = sanitize_dxf_name(&layer.name);
        for node in &layer.objects {
            emit_dxf_node(&mut out, node, Affine2::IDENTITY, &safe_layer_name, page_h)?;
        }
    }

    out.push_str("0\nENDSEC\n0\nEOF\n");
    if out.len() > crate::pdf::MAX_PDF_EXPORT_BYTES {
        return Err(RenderError::ExportOutputTooLarge { bytes: out.len(), max: crate::pdf::MAX_PDF_EXPORT_BYTES });
    }
    Ok(out)
}

fn emit_dxf_node(out: &mut String, node: &ObjectNode, parent_tf: Affine2, layer_name: &str, page_h: f64) -> Result<(), RenderError> {
    if !node.visible || node.opacity.get() <= 0.0 {
        return Ok(());
    }
    let world_tf = node.transform.then(parent_tf)?;
    if let ObjectKind::Group { children } = &node.kind {
        for ch in children {
            emit_dxf_node(out, ch, world_tf, layer_name, page_h)?;
        }
        return Ok(());
    }
    if node.effects.is_empty() {
        match &node.kind {
            ObjectKind::ArtisticText { text, size_pt, position, .. } => {
                let clean = sanitize_dxf_text(text);
                if !clean.is_empty() {
                    let world_pt = world_tf.apply_point(*position)?;
                    let h_pt = (size_pt.get() * transform_scale(world_tf)).max(0.1);
                    let _ = writeln!(
                        out,
                        "0\nTEXT\n8\n{layer_name}\n10\n{}\n20\n{}\n40\n{}\n1\n{clean}",
                        fmt_num(world_pt.x.get()),
                        fmt_num(page_h - world_pt.y.get()),
                        fmt_num(h_pt)
                    );
                }
                return Ok(());
            }
            ObjectKind::ParagraphText(pt) => {
                let layout = coralcraft_doc::layout_paragraph_text(pt);
                let h_pt = (pt.font_size_pt.get() * transform_scale(world_tf)).max(0.1);
                for line in &layout.lines {
                    let clean = sanitize_dxf_text(&line.text);
                    if !clean.is_empty()
                        && let Ok(local_pt) = Point::new(line.origin_x, line.origin_y)
                        && let Ok(world_pt) = world_tf.apply_point(local_pt)
                    {
                        let _ = writeln!(
                            out,
                            "0\nTEXT\n8\n{layer_name}\n10\n{}\n20\n{}\n40\n{}\n1\n{clean}",
                            fmt_num(world_pt.x.get()),
                            fmt_num(page_h - world_pt.y.get()),
                            fmt_num(h_pt)
                        );
                    }
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
    let polys = flatten_path(&world_path, &FlattenOpts::default())?;

    for poly in &polys {
        if poly.points.len() < 2 {
            continue;
        }
        let closed_flag = if poly.closed { 1 } else { 0 };
        let _ = writeln!(out, "0\nPOLYLINE\n8\n{layer_name}\n66\n1\n70\n{closed_flag}");
        for &pt in &poly.points {
            let _ = writeln!(out, "0\nVERTEX\n8\n{layer_name}\n10\n{}\n20\n{}", fmt_num(pt.x.get()), fmt_num(page_h - pt.y.get()));
        }
        let _ = writeln!(out, "0\nSEQEND\n8\n{layer_name}");
    }

    for pc in &node.powerclip_children {
        emit_dxf_node(out, pc, world_tf, layer_name, page_h)?;
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
        assert!(dxf.contains("\n1\nFirst line\n") && dxf.contains("\n1\nSecond line\n"), "DXF should include ParagraphText lines: {dxf}");

        let reimported = coralcraft_doc::import_dxf(&dxf).expect("import_dxf");
        let re_objs = &reimported.pages.first().unwrap().layers.first().unwrap().objects;
        assert_eq!(re_objs.len(), 3, "import_dxf should reconstruct 3 ArtisticText objects from DXF TEXT entities");
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
}
