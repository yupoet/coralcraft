//! Clean-room Encapsulated PostScript (`EPSF-3.0`) and AutoCAD `DXF` (`AC1009`) vector exporters
//! for `coralcraft-render`.

use std::fmt::Write as _;

use coralcraft_color::Color;
use coralcraft_doc::{Document, FillStyle, ObjectKind, ObjectNode, VectorEffect};
use coralcraft_geom::{FlattenOpts, Path, PathElement, Point, Rect, RectSpec, flatten_path};

use crate::extract_local_path_and_bbox;
use crate::surface::RenderError;
use crate::svg::fmt_num;

/// Exports `page_index` (0-based) of `doc` as a clean-room Encapsulated PostScript (`%!PS-Adobe-3.0 EPSF-3.0`) document.
pub fn export_eps(doc: &Document, page_index: usize) -> Result<String, RenderError> {
    let layers = doc.effective_layers_for_page(page_index, true)?;
    let page = doc.pages.get(page_index).ok_or(coralcraft_doc::DocError::PageOutOfRange { index: page_index, count: doc.pages.len() })?;
    let w = page.size.width.get();
    let h = page.size.height.get();

    let mut out = String::new();
    out.push_str("%!PS-Adobe-3.0 EPSF-3.0\n");
    let _ = writeln!(out, "%%Title: ({})", page.name);
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
        let _ = writeln!(out, "% Layer: {}", layer.name);
        for node in &layer.objects {
            emit_eps_node(&mut out, node, h)?;
        }
    }

    out.push_str("grestore\n");
    out.push_str("showpage\n");
    out.push_str("%%EOF\n");
    Ok(out)
}

fn emit_eps_node(out: &mut String, node: &ObjectNode, page_h: f64) -> Result<(), RenderError> {
    if !node.visible || node.opacity.get() <= 0.0 {
        return Ok(());
    }
    if let ObjectKind::Group { children } = &node.kind {
        for ch in children {
            emit_eps_node(out, ch, page_h)?;
        }
        return Ok(());
    }
    let Some((mut local_path, mut local_bbox)) = extract_local_path_and_bbox(&node.kind)? else {
        return Ok(());
    };
    apply_deforming_effects(node, &mut local_path, &mut local_bbox);
    let world_path = local_path.transform(node.transform)?;

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
                    let c1x = cur.x.get() + (2.0 / 3.0) * (ctrl.x.get() - cur.x.get());
                    let c1y = cur.y.get() + (2.0 / 3.0) * (ctrl.y.get() - cur.y.get());
                    let c2x = end.x.get() + (2.0 / 3.0) * (ctrl.x.get() - end.x.get());
                    let c2y = end.y.get() + (2.0 / 3.0) * (ctrl.y.get() - end.y.get());
                    let _ = writeln!(
                        buf,
                        "{} {} {} {} {} {} curveto",
                        fmt_num(c1x),
                        fmt_num(page_h - c1y),
                        fmt_num(c2x),
                        fmt_num(page_h - c2y),
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
            out.push_str("fill\n");
        }
    }

    if let Some(outline) = &node.outline {
        let w = if outline.is_hairline() { 0.24 } else { outline.width_pt.get().max(0.1) };
        emit_ps_color(out, &outline.color);
        let _ = writeln!(out, "{} setlinewidth", fmt_num(w));
        if outline.dash_pattern.is_empty() {
            out.push_str("[] 0 setdash\n");
        } else {
            let joined: Vec<String> = outline.dash_pattern.iter().map(|d| fmt_num(d.get())).collect();
            let _ = writeln!(out, "[{}] 0 setdash", joined.join(" "));
        }
        emit_ps_path(out);
        out.push_str("stroke\n");
    }

    Ok(())
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
        for node in &layer.objects {
            emit_dxf_node(&mut out, node, &layer.name, page_h)?;
        }
    }

    out.push_str("0\nENDSEC\n0\nEOF\n");
    Ok(out)
}

fn emit_dxf_node(out: &mut String, node: &ObjectNode, layer_name: &str, page_h: f64) -> Result<(), RenderError> {
    if !node.visible || node.opacity.get() <= 0.0 {
        return Ok(());
    }
    if let ObjectKind::Group { children } = &node.kind {
        for ch in children {
            emit_dxf_node(out, ch, layer_name, page_h)?;
        }
        return Ok(());
    }
    let Some((mut local_path, mut local_bbox)) = extract_local_path_and_bbox(&node.kind)? else {
        return Ok(());
    };
    apply_deforming_effects(node, &mut local_path, &mut local_bbox);
    let world_path = local_path.transform(node.transform)?;
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

    Ok(())
}

fn apply_deforming_effects(node: &ObjectNode, local_path: &mut Path, local_bbox: &mut Rect) {
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
