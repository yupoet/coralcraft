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
        let color = match fill {
            FillStyle::Uniform { color } => Some(color),
            FillStyle::Fountain { stops, .. } => stops.first().map(|s| &s.color),
            FillStyle::TwoColorPattern { fg_color, .. } => Some(fg_color),
            FillStyle::Unknown { .. } => None,
        };
        if let Some(col) = color {
            emit_ps_color(out, col);
            emit_ps_path(out);
            if world_path.fill_rule == FillRule::EvenOdd {
                out.push_str("eofill\n");
            } else {
                out.push_str("fill\n");
            }
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
}
