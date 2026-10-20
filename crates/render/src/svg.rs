//! Clean-room SVG 1.1 vector exporter (`export_svg`) for `coralcraft-render`.

use std::fmt::Write as _;

use coralcraft_color::Color;
use coralcraft_doc::{
    Document, FillStyle, FountainGeometry, LineCap, LineJoin, ObjectKind, ObjectNode, OutlineStyle, PatternKind, TransparencyStyle,
};
use coralcraft_geom::{Affine2, FillRule, Path, PathElement, Rect, RectSpec};

use crate::surface::RenderError;

/// Formats a floating-point coordinate/number cleanly (up to 3 decimal places, trimming trailing zeros, `-0` -> `0`).
pub(crate) fn fmt_num(v: f64) -> String {
    let v = if !v.is_finite() || v.abs() < 1e-9 { 0.0 } else { v };
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-0" { "0".to_owned() } else { s.to_owned() }
}

/// Escapes special XML characters (`&`, `<`, `>`, `"`, `'`) and strips illegal XML 1.0 C0 control characters.
fn escape_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c if (c < '\x20' && c != '\t' && c != '\n' && c != '\r') => {}
            _ => out.push(ch),
        }
    }
    out
}

/// Formats a [`Color`] as `#rrggbb`.
fn svg_hex_color(color: &Color) -> String {
    let [r, g, b] = color.to_srgb8();
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// Serializes a [`Path`] into an SVG `d="..."` path data string.
pub fn path_to_svg_d(path: &Path) -> String {
    let mut d = String::new();
    for el in &path.elements {
        if !d.is_empty() {
            d.push(' ');
        }
        match *el {
            PathElement::MoveTo(p) => {
                let _ = write!(d, "M {} {}", fmt_num(p.x.get()), fmt_num(p.y.get()));
            }
            PathElement::LineTo(p) => {
                let _ = write!(d, "L {} {}", fmt_num(p.x.get()), fmt_num(p.y.get()));
            }
            PathElement::QuadTo { ctrl, end } => {
                let _ = write!(d, "Q {} {} {} {}", fmt_num(ctrl.x.get()), fmt_num(ctrl.y.get()), fmt_num(end.x.get()), fmt_num(end.y.get()));
            }
            PathElement::CubicTo { c1, c2, end } => {
                let _ = write!(
                    d,
                    "C {} {} {} {} {} {}",
                    fmt_num(c1.x.get()),
                    fmt_num(c1.y.get()),
                    fmt_num(c2.x.get()),
                    fmt_num(c2.y.get()),
                    fmt_num(end.x.get()),
                    fmt_num(end.y.get())
                );
            }
            PathElement::Close => {
                d.push('Z');
            }
        }
    }
    d
}

/// Exports `page_index` (0-based) of `doc` as a self-contained SVG 1.1 document string.
pub fn export_svg(doc: &Document, page_index: usize) -> Result<String, RenderError> {
    export_svg_with_prepress(doc, page_index, &crate::pdf::PrepressOptions::default())
}

/// Exports `page_index` (0-based) of `doc` as an SVG 1.1 document string with optional prepress marks.
pub fn export_svg_with_prepress(doc: &Document, page_index: usize, prepress: &crate::pdf::PrepressOptions) -> Result<String, RenderError> {
    let layers = doc.effective_layers_for_page(page_index, true)?;
    let page = doc.pages.get(page_index).ok_or(coralcraft_doc::DocError::PageOutOfRange { index: page_index, count: doc.pages.len() })?;
    let w = page.size.width.get();
    let h = page.size.height.get();

    let mut defs = String::new();
    let mut body = String::new();

    // Optional page background
    let bg_fill_attr = match &page.background {
        Some(FillStyle::Uniform { color }) => svg_hex_color(color),
        Some(fill @ FillStyle::Fountain { .. }) => {
            let page_bbox = Rect::new(0.0, 0.0, w, h)?;
            emit_fountain_def(&mut defs, "page-bg", fill, page_bbox);
            "url(#grad-page-bg)".to_owned()
        }
        Some(fill @ FillStyle::TwoColorPattern { .. }) => {
            emit_pattern_def(&mut defs, "page-bg", fill);
            "url(#pat-page-bg)".to_owned()
        }
        Some(FillStyle::Unknown { .. }) | None => "#ffffff".to_owned(),
    };

    for layer in layers {
        let layer_opacity = layer.opacity.get().clamp(0.0, 1.0);
        if layer_opacity <= 0.0 {
            continue;
        }
        let opacity_attr = if (layer_opacity - 1.0).abs() > 1e-4 { format!(" opacity=\"{}\"", fmt_num(layer_opacity)) } else { String::new() };
        let _ = writeln!(body, "  <g id=\"layer-{}\" data-name=\"{}\"{opacity_attr}>", layer.id.0, escape_xml(&layer.name));
        for node in &layer.objects {
            emit_svg_node(&mut defs, &mut body, node, 2)?;
        }
        body.push_str("  </g>\n");
    }

    if prepress.crop_marks || prepress.registration_marks {
        body.push_str(
            "  <g id=\"prepress-marks\" fill=\"none\" stroke=\"#000000\" stroke-width=\"0.25\" data-color=\"Registration(CMYK 100,100,100,100)\">\n",
        );
        if prepress.crop_marks {
            let mut d = String::new();
            for (idx, ((x0, y0), (x1, y1))) in crate::pdf::crop_mark_segments(w, h, 3.0, 12.0).into_iter().enumerate() {
                if idx > 0 {
                    d.push(' ');
                }
                let _ = write!(d, "M {} {} L {} {}", fmt_num(x0), fmt_num(y0), fmt_num(x1), fmt_num(y1));
            }
            let _ = writeln!(body, "    <path class=\"crop-marks\" d=\"{d}\"/>");
        }
        if prepress.registration_marks {
            for (tx, ty) in crate::pdf::registration_target_centers(w, h) {
                let _ = writeln!(body, "    <circle class=\"registration-target\" cx=\"{}\" cy=\"{}\" r=\"3.5\"/>", fmt_num(tx), fmt_num(ty));
                let _ = writeln!(
                    body,
                    "    <path class=\"registration-crosshair\" d=\"M {} {} L {} {} M {} {} L {} {}\"/>",
                    fmt_num(tx - 5.0),
                    fmt_num(ty),
                    fmt_num(tx + 5.0),
                    fmt_num(ty),
                    fmt_num(tx),
                    fmt_num(ty - 5.0),
                    fmt_num(tx),
                    fmt_num(ty + 5.0)
                );
            }
        }
        body.push_str("  </g>\n");
    }

    let mut svg = String::new();
    let _ = writeln!(
        svg,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" version=\"1.1\" width=\"{}pt\" height=\"{}pt\" viewBox=\"0 0 {} {}\">",
        fmt_num(w),
        fmt_num(h),
        fmt_num(w),
        fmt_num(h)
    );
    if !defs.is_empty() {
        svg.push_str("  <defs>\n");
        svg.push_str(&defs);
        svg.push_str("  </defs>\n");
    }
    let _ =
        writeln!(svg, "  <rect id=\"page-background\" x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" fill=\"{bg_fill_attr}\"/>", fmt_num(w), fmt_num(h));
    svg.push_str(&body);
    svg.push_str("</svg>\n");

    if svg.len() > crate::pdf::MAX_PDF_EXPORT_BYTES {
        return Err(RenderError::ExportOutputTooLarge { bytes: svg.len(), max: crate::pdf::MAX_PDF_EXPORT_BYTES });
    }
    Ok(svg)
}

fn emit_pattern_def(defs: &mut String, id_suffix: &str, fill: &FillStyle) {
    let FillStyle::TwoColorPattern { kind, fg_color, bg_color, tile_size_pt } = fill else {
        return;
    };
    let ts = tile_size_pt.get().max(1.0);
    let half = ts * 0.5;
    let fg = svg_hex_color(fg_color);
    let bg = svg_hex_color(bg_color);
    let _ =
        writeln!(defs, "    <pattern id=\"pat-{id_suffix}\" patternUnits=\"userSpaceOnUse\" width=\"{}\" height=\"{}\">", fmt_num(ts), fmt_num(ts));
    let _ = writeln!(defs, "      <rect width=\"{}\" height=\"{}\" fill=\"{bg}\"/>", fmt_num(ts), fmt_num(ts));
    match kind {
        PatternKind::Checkerboard | PatternKind::Unknown => {
            let _ = writeln!(defs, "      <rect width=\"{}\" height=\"{}\" fill=\"{fg}\"/>", fmt_num(half), fmt_num(half));
            let _ = writeln!(
                defs,
                "      <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{fg}\"/>",
                fmt_num(half),
                fmt_num(half),
                fmt_num(half),
                fmt_num(half)
            );
        }
        PatternKind::PolkaDot => {
            let _ = writeln!(defs, "      <circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{fg}\"/>", fmt_num(half), fmt_num(half), fmt_num(ts * 0.35));
        }
        PatternKind::HorizontalStripe => {
            let _ = writeln!(defs, "      <rect width=\"{}\" height=\"{}\" fill=\"{fg}\"/>", fmt_num(ts), fmt_num(half));
        }
        PatternKind::VerticalStripe => {
            let _ = writeln!(defs, "      <rect width=\"{}\" height=\"{}\" fill=\"{fg}\"/>", fmt_num(half), fmt_num(ts));
        }
        PatternKind::DiagonalCrosshatch => {
            let _ = writeln!(
                defs,
                "      <path d=\"M 0 0 L {0} {0} M 0 {0} L {0} 0\" stroke=\"{fg}\" stroke-width=\"{1}\"/>",
                fmt_num(ts),
                fmt_num(ts * 0.15)
            );
        }
    }
    defs.push_str("    </pattern>\n");
}

fn emit_fountain_def(defs: &mut String, id_suffix: &str, fill: &FillStyle, bbox: Rect) {
    let FillStyle::Fountain { geometry, stops, .. } = fill else {
        return;
    };
    let cx = 0.5 * (bbox.x0.get() + bbox.x1.get());
    let cy = 0.5 * (bbox.y0.get() + bbox.y1.get());
    let bw = bbox.width().max(1e-6);
    let bh = bbox.height().max(1e-6);
    let hw = 0.5 * bw;
    let hh = 0.5 * bh;

    match *geometry {
        FountainGeometry::Linear { angle_deg, edge_pad } => {
            let rad = angle_deg.get().to_radians();
            let (sin_a, cos_a) = rad.sin_cos();
            let pad = edge_pad.get().clamp(0.0, 0.49);
            let half_span = ((hw * cos_a.abs() + hh * sin_a.abs()) * (1.0 - 2.0 * pad)).max(1e-6);
            let x1 = cx - cos_a * half_span;
            let y1 = cy - sin_a * half_span;
            let x2 = cx + cos_a * half_span;
            let y2 = cy + sin_a * half_span;
            let _ = writeln!(
                defs,
                "    <linearGradient id=\"grad-{id_suffix}\" gradientUnits=\"userSpaceOnUse\" x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\">",
                fmt_num(x1),
                fmt_num(y1),
                fmt_num(x2),
                fmt_num(y2)
            );
            for stop in stops {
                let _ = writeln!(defs, "      <stop offset=\"{}\" stop-color=\"{}\"/>", fmt_num(stop.offset.get()), svg_hex_color(&stop.color));
            }
            defs.push_str("    </linearGradient>\n");
        }
        FountainGeometry::Radial { center_x, center_y, radius, focal_x, focal_y } => {
            let rcx = bbox.x0.get() + center_x.get() * bw;
            let rcy = bbox.y0.get() + center_y.get() * bh;
            let rfx = bbox.x0.get() + focal_x.get() * bw;
            let rfy = bbox.y0.get() + focal_y.get() * bh;
            let r = (radius.get() * bw.hypot(bh)).max(1e-6);
            let _ = writeln!(
                defs,
                "    <radialGradient id=\"grad-{id_suffix}\" gradientUnits=\"userSpaceOnUse\" cx=\"{}\" cy=\"{}\" r=\"{}\" fx=\"{}\" fy=\"{}\">",
                fmt_num(rcx),
                fmt_num(rcy),
                fmt_num(r),
                fmt_num(rfx),
                fmt_num(rfy)
            );
            for stop in stops {
                let _ = writeln!(defs, "      <stop offset=\"{}\" stop-color=\"{}\"/>", fmt_num(stop.offset.get()), svg_hex_color(&stop.color));
            }
            defs.push_str("    </radialGradient>\n");
        }
        FountainGeometry::Conical { center_x, center_y, angle_deg } => {
            let rcx = bbox.x0.get() + center_x.get() * bw;
            let rcy = bbox.y0.get() + center_y.get() * bh;
            let r = (0.5 * bw.hypot(bh)).max(1e-6);
            let _ = writeln!(
                defs,
                "    <radialGradient id=\"grad-{id_suffix}\" data-fountain-type=\"conical\" data-angle=\"{}\" gradientUnits=\"userSpaceOnUse\" cx=\"{}\" cy=\"{}\" r=\"{}\">",
                fmt_num(angle_deg.get()),
                fmt_num(rcx),
                fmt_num(rcy),
                fmt_num(r),
            );
            for stop in stops {
                let _ = writeln!(defs, "      <stop offset=\"{}\" stop-color=\"{}\"/>", fmt_num(stop.offset.get()), svg_hex_color(&stop.color));
            }
            defs.push_str("    </radialGradient>\n");
        }
        FountainGeometry::Square { center_x, center_y, radius, angle_deg } => {
            let rcx = bbox.x0.get() + center_x.get() * bw;
            let rcy = bbox.y0.get() + center_y.get() * bh;
            let r = (radius.get() * bw.hypot(bh)).max(1e-6);
            let _ = writeln!(
                defs,
                "    <radialGradient id=\"grad-{id_suffix}\" data-fountain-type=\"square\" data-angle=\"{}\" gradientUnits=\"userSpaceOnUse\" cx=\"{}\" cy=\"{}\" r=\"{}\">",
                fmt_num(angle_deg.get()),
                fmt_num(rcx),
                fmt_num(rcy),
                fmt_num(r),
            );
            for stop in stops {
                let _ = writeln!(defs, "      <stop offset=\"{}\" stop-color=\"{}\"/>", fmt_num(stop.offset.get()), svg_hex_color(&stop.color));
            }
            defs.push_str("    </radialGradient>\n");
        }
    }
}

fn format_transform_attr(t: Affine2) -> String {
    if t == Affine2::IDENTITY {
        String::new()
    } else {
        format!(
            " transform=\"matrix({} {} {} {} {} {})\"",
            fmt_num(t.a.get()),
            fmt_num(t.b.get()),
            fmt_num(t.c.get()),
            fmt_num(t.d.get()),
            fmt_num(t.tx.get()),
            fmt_num(t.ty.get())
        )
    }
}

fn format_stroke_attrs(outline: &OutlineStyle, obj_transform: Affine2) -> String {
    let mut w = if outline.is_hairline() { 0.25 } else { outline.width_pt.get() };
    if !outline.scale_with_object {
        let scale = obj_transform.determinant().abs().sqrt();
        if scale > 1e-6 {
            w /= scale;
        }
    }
    let cap = match outline.cap {
        LineCap::Butt => "butt",
        LineCap::Round => "round",
        LineCap::Square => "square",
    };
    let join = match outline.join {
        LineJoin::Miter => "miter",
        LineJoin::Round => "round",
        LineJoin::Bevel => "bevel",
    };
    let dash_attr = if outline.dash_pattern.is_empty() {
        String::new()
    } else {
        let joined: Vec<String> = outline.dash_pattern.iter().map(|d| fmt_num(d.get())).collect();
        format!(" stroke-dasharray=\"{}\"", joined.join(","))
    };
    format!(
        "stroke=\"{}\" stroke-width=\"{}\" stroke-linecap=\"{cap}\" stroke-linejoin=\"{join}\" stroke-miterlimit=\"{}\"{dash_attr}",
        svg_hex_color(&outline.color),
        fmt_num(w),
        fmt_num(outline.miter_limit.get())
    )
}

fn emit_svg_node(defs: &mut String, body: &mut String, node: &ObjectNode, indent_level: usize) -> Result<(), RenderError> {
    if !node.visible {
        return Ok(());
    }
    let opacity = node.opacity.get().clamp(0.0, 1.0);
    if opacity <= 0.0 {
        return Ok(());
    }
    let indent = "  ".repeat(indent_level);
    let transform_attr = format_transform_attr(node.transform);
    let opacity_attr = if (opacity - 1.0).abs() > 1e-4 { format!(" opacity=\"{}\"", fmt_num(opacity)) } else { String::new() };

    match &node.kind {
        ObjectKind::Group { children } => {
            let name_attr = node.name.as_deref().map(|n| format!(" data-name=\"{}\"", escape_xml(n))).unwrap_or_default();
            let locked_attr = if node.locked { " data-locked=\"1\"" } else { "" };
            let _ = writeln!(body, "{indent}<g id=\"obj-{}\"{name_attr}{locked_attr}{transform_attr}{opacity_attr}>", node.id.0);
            for ch in children {
                emit_svg_node(defs, body, ch, indent_level.saturating_add(1))?;
            }
            let _ = writeln!(body, "{indent}</g>");
            return Ok(());
        }
        ObjectKind::ArtisticText { text, font_family, size_pt, position } if node.effects.is_empty() => {
            if text.is_empty() {
                return Ok(());
            }
            let fill_str = match &node.fill {
                Some(FillStyle::Uniform { color }) => svg_hex_color(color),
                Some(fill @ FillStyle::Fountain { .. }) => {
                    let h = size_pt.get().max(4.0);
                    let w = (text.chars().count() as f64) * h * 0.6;
                    let bbox = Rect::new(position.x.get(), position.y.get() - h, position.x.get() + w, position.y.get())?;
                    let grad_id = format!("obj-{}", node.id.0);
                    emit_fountain_def(defs, &grad_id, fill, bbox);
                    format!("url(#grad-{grad_id})")
                }
                Some(fill @ FillStyle::TwoColorPattern { .. }) => {
                    let pat_id = format!("obj-{}", node.id.0);
                    emit_pattern_def(defs, &pat_id, fill);
                    format!("url(#pat-{pat_id})")
                }
                Some(FillStyle::Unknown { .. }) | None => "none".to_owned(),
            };
            let name_attr = node.name.as_deref().map(|n| format!(" data-name=\"{}\"", escape_xml(n))).unwrap_or_default();
            let stroke_str = node.outline.as_ref().map(|o| format!(" {}", format_stroke_attrs(o, node.transform))).unwrap_or_default();
            let _ = writeln!(
                body,
                "{indent}<text id=\"obj-{}\"{name_attr} x=\"{}\" y=\"{}\" font-family=\"{}\" font-size=\"{}\" fill=\"{fill_str}\"{stroke_str}{transform_attr}{opacity_attr} xml:space=\"preserve\">{}</text>",
                node.id.0,
                fmt_num(position.x.get()),
                fmt_num(position.y.get()),
                escape_xml(font_family),
                fmt_num(size_pt.get()),
                escape_xml(text)
            );
            return Ok(());
        }
        ObjectKind::ParagraphText(pt) => {
            let layout = coralcraft_doc::layout_paragraph_text(pt);
            if layout.lines.is_empty() {
                return Ok(());
            }
            let px = pt.position.x.get();
            let py = pt.position.y.get();
            let bbox = Rect::new(px, py, px + pt.frame_width_pt.get().max(1.0), py + pt.frame_height_pt.get().max(1.0))?;
            let fill_str = match &node.fill {
                Some(FillStyle::Uniform { color }) => svg_hex_color(color),
                Some(fill @ FillStyle::Fountain { .. }) => {
                    let grad_id = format!("obj-{}", node.id.0);
                    emit_fountain_def(defs, &grad_id, fill, bbox);
                    format!("url(#grad-{grad_id})")
                }
                Some(fill @ FillStyle::TwoColorPattern { .. }) => {
                    let pat_id = format!("obj-{}", node.id.0);
                    emit_pattern_def(defs, &pat_id, fill);
                    format!("url(#pat-{pat_id})")
                }
                Some(FillStyle::Unknown { .. }) | None => "none".to_owned(),
            };
            let align_str = match pt.alignment {
                coralcraft_doc::TextAlignment::Left => "left",
                coralcraft_doc::TextAlignment::Center => "center",
                coralcraft_doc::TextAlignment::Right => "right",
                coralcraft_doc::TextAlignment::Justify => "justify",
            };
            let name_attr = node.name.as_deref().map(|n| format!(" data-name=\"{}\"", escape_xml(n))).unwrap_or_default();
            let escaped_raw_text = escape_xml(&pt.text).replace('\n', "&#10;").replace('\r', "&#13;");
            let stroke_str = node.outline.as_ref().map(|o| format!(" {}", format_stroke_attrs(o, node.transform))).unwrap_or_default();
            let _ = writeln!(
                body,
                "{indent}<text id=\"obj-{}\"{name_attr} x=\"{}\" y=\"{}\" data-frame-x=\"{}\" data-frame-y=\"{}\" data-frame-width=\"{}\" data-frame-height=\"{}\" data-line-spacing=\"{}\" data-align=\"{align_str}\" data-columns=\"{}\" data-gutter=\"{}\" data-raw-text=\"{escaped_raw_text}\" font-family=\"{}\" font-size=\"{}\" fill=\"{fill_str}\"{stroke_str}{transform_attr}{opacity_attr} xml:space=\"preserve\">",
                node.id.0,
                fmt_num(px),
                fmt_num(py),
                fmt_num(px),
                fmt_num(py),
                fmt_num(pt.frame_width_pt.get()),
                fmt_num(pt.frame_height_pt.get()),
                fmt_num(pt.line_spacing_multiplier.get()),
                pt.columns,
                fmt_num(pt.gutter_pt.get()),
                escape_xml(&pt.font_family),
                fmt_num(pt.font_size_pt.get())
            );
            for line in &layout.lines {
                let ws_attr =
                    if line.word_spacing_pt > 1e-4 { format!(" word-spacing=\"{}\"", fmt_num(line.word_spacing_pt)) } else { String::new() };
                let _ = writeln!(
                    body,
                    "{indent}  <tspan x=\"{}\" y=\"{}\"{ws_attr}>{}</tspan>",
                    fmt_num(line.origin_x),
                    fmt_num(line.origin_y),
                    escape_xml(&line.text)
                );
            }
            let _ = writeln!(body, "{indent}</text>");
            return Ok(());
        }
        ObjectKind::Bitmap { pixel_width, pixel_height, dpi_x, dpi_y, rgba_pixels } => {
            let w = (*pixel_width as f64 * 72.0 / dpi_x.get().max(1.0)).max(1.0);
            let h = (*pixel_height as f64 * 72.0 / dpi_y.get().max(1.0)).max(1.0);
            let png_bytes = crate::png::encode_raw_png_rgba(*pixel_width, *pixel_height, rgba_pixels)?;
            let b64 = crate::png::base64_encode(&png_bytes);
            let _ = writeln!(
                body,
                "{indent}<image id=\"obj-{}\" x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" preserveAspectRatio=\"none\" href=\"data:image/png;base64,{}\"{transform_attr}{opacity_attr}/>",
                node.id.0,
                fmt_num(w),
                fmt_num(h),
                b64
            );
            return Ok(());
        }
        ObjectKind::Unsupported(u) => {
            if let Some(bbox) = u.fallback_bounds {
                let path = RectSpec::sharp(bbox).to_path()?;
                emit_path_node(defs, body, node, &path, bbox, indent_level)?;
            }
            for ch in &u.children {
                emit_svg_node(defs, body, ch, indent_level)?;
            }
            return Ok(());
        }
        _ => {}
    }

    let Some((mut local_path, mut local_bbox)) = crate::extract_local_path_and_bbox(&node.kind)? else {
        return Ok(());
    };
    crate::eps::apply_deforming_effects(node, &mut local_path, &mut local_bbox);
    emit_path_node(defs, body, node, &local_path, local_bbox, indent_level)
}

fn emit_path_node(
    defs: &mut String,
    body: &mut String,
    node: &ObjectNode,
    local_path: &Path,
    local_bbox: Rect,
    indent_level: usize,
) -> Result<(), RenderError> {
    let indent = "  ".repeat(indent_level);
    let d = path_to_svg_d(local_path);
    if d.is_empty() {
        return Ok(());
    }

    let fill_rule_attr = if local_path.fill_rule == FillRule::EvenOdd { " fill-rule=\"evenodd\"" } else { "" };
    let fill_attr = match &node.fill {
        Some(FillStyle::Uniform { color }) => svg_hex_color(color),
        Some(fill @ FillStyle::Fountain { .. }) => {
            let grad_id = format!("obj-{}", node.id.0);
            emit_fountain_def(defs, &grad_id, fill, local_bbox);
            format!("url(#grad-{grad_id})")
        }
        Some(fill @ FillStyle::TwoColorPattern { .. }) => {
            let pat_id = format!("obj-{}", node.id.0);
            emit_pattern_def(defs, &pat_id, fill);
            format!("url(#pat-{pat_id})")
        }
        Some(FillStyle::Unknown { .. }) | None => "none".to_owned(),
    };

    let has_powerclip = !node.powerclip_children.is_empty();
    let behind_fill = node.outline.as_ref().is_some_and(|o| o.behind_fill);
    let transform_attr = format_transform_attr(node.transform);
    let mut opacity = node.opacity.get().clamp(0.0, 1.0);
    if let Some(TransparencyStyle::Uniform { opacity: tr_op }) = &node.transparency {
        opacity = (opacity * tr_op.get()).clamp(0.0, 1.0);
    }
    let opacity_attr = if (opacity - 1.0).abs() > 1e-4 { format!(" opacity=\"{}\"", fmt_num(opacity)) } else { String::new() };
    let mask_attr = if let Some(TransparencyStyle::Fountain { geometry, blend_mode, stops }) = &node.transparency {
        let mask_grad_id = format!("trsp-obj-{}", node.id.0);
        let mask_fill = FillStyle::Fountain { geometry: *geometry, blend_mode: *blend_mode, stops: stops.clone() };
        emit_fountain_def(defs, &mask_grad_id, &mask_fill, local_bbox);
        let _ = writeln!(defs, "    <mask id=\"mask-obj-{}\" maskUnits=\"userSpaceOnUse\">", node.id.0);
        let _ = writeln!(
            defs,
            "      <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"url(#grad-{mask_grad_id})\"/>",
            fmt_num(local_bbox.x0.get()),
            fmt_num(local_bbox.y0.get()),
            fmt_num(local_bbox.width()),
            fmt_num(local_bbox.height())
        );
        defs.push_str("    </mask>\n");
        format!(" mask=\"url(#mask-obj-{})\"", node.id.0)
    } else {
        String::new()
    };

    if has_powerclip {
        let clip_rule_attr = if local_path.fill_rule == FillRule::EvenOdd { " clip-rule=\"evenodd\"" } else { "" };
        let _ = writeln!(defs, "    <clipPath id=\"clip-{}\" clipPathUnits=\"userSpaceOnUse\">", node.id.0);
        let _ = writeln!(defs, "      <path d=\"{d}\"{clip_rule_attr}/>");
        defs.push_str("    </clipPath>\n");
    }

    let has_arrows = node
        .outline
        .as_ref()
        .is_some_and(|o| o.start_arrow != coralcraft_doc::ArrowheadKind::None || o.end_arrow != coralcraft_doc::ArrowheadKind::None);
    let needs_wrapper = has_powerclip || behind_fill || has_arrows || !transform_attr.is_empty() || !opacity_attr.is_empty() || !mask_attr.is_empty();
    if needs_wrapper {
        let _ = writeln!(body, "{indent}<g id=\"obj-{}\"{transform_attr}{opacity_attr}{mask_attr}>", node.id.0);
        let child_indent = "  ".repeat(indent_level.saturating_add(1));

        // 1. Outline behind fill
        if behind_fill && let Some(outline) = &node.outline {
            let stroke_attrs = format_stroke_attrs(outline, node.transform);
            let _ = writeln!(body, "{child_indent}<path d=\"{d}\" fill=\"none\" {stroke_attrs}/>");
            emit_svg_arrowheads(body, local_path, outline, node.transform, &child_indent);
        }

        // 2. Fill
        if node.fill.is_some() {
            if !behind_fill
                && !has_powerclip
                && let Some(outline) = &node.outline
            {
                let stroke_attrs = format_stroke_attrs(outline, node.transform);
                let _ = writeln!(body, "{child_indent}<path d=\"{d}\" fill=\"{fill_attr}\"{fill_rule_attr} {stroke_attrs}/>");
                emit_svg_arrowheads(body, local_path, outline, node.transform, &child_indent);
            } else {
                let _ = writeln!(body, "{child_indent}<path d=\"{d}\" fill=\"{fill_attr}\"{fill_rule_attr}/>");
            }
        }

        // 3. PowerClip children clipped by container path
        if has_powerclip {
            let _ = writeln!(body, "{child_indent}<g clip-path=\"url(#clip-{})\">", node.id.0);
            for pc in &node.powerclip_children {
                emit_svg_node(defs, body, pc, indent_level.saturating_add(2))?;
            }
            let _ = writeln!(body, "{child_indent}</g>");
        }

        // 4. Outline in front of fill/PowerClip
        if !behind_fill
            && (has_powerclip || node.fill.is_none())
            && let Some(outline) = &node.outline
        {
            let stroke_attrs = format_stroke_attrs(outline, node.transform);
            let _ = writeln!(body, "{child_indent}<path d=\"{d}\" fill=\"none\" {stroke_attrs}/>");
            emit_svg_arrowheads(body, local_path, outline, node.transform, &child_indent);
        }

        let _ = writeln!(body, "{indent}</g>");
    } else {
        let stroke_part = node.outline.as_ref().map(|o| format!(" {}", format_stroke_attrs(o, node.transform))).unwrap_or_default();
        let _ = writeln!(body, "{indent}<path id=\"obj-{}\" d=\"{d}\" fill=\"{fill_attr}\"{fill_rule_attr}{stroke_part}/>", node.id.0);
    }

    Ok(())
}

fn emit_svg_arrowheads(body: &mut String, local_path: &Path, outline: &OutlineStyle, obj_transform: Affine2, indent: &str) {
    if outline.start_arrow == coralcraft_doc::ArrowheadKind::None && outline.end_arrow == coralcraft_doc::ArrowheadKind::None {
        return;
    }
    let mut w = if outline.is_hairline() { 0.25 } else { outline.width_pt.get() };
    if !outline.scale_with_object {
        let scale = obj_transform.determinant().abs().sqrt();
        if scale > 1e-6 {
            w /= scale;
        }
    }
    if let Ok(polys) = coralcraft_geom::flatten_path(local_path, &coralcraft_geom::FlattenOpts::default())
        && let Ok(arrow_polys) = crate::build_arrowhead_polylines(&polys, 0.5 * w, outline.start_arrow, outline.end_arrow)
    {
        let col = svg_hex_color(&outline.color);
        for ap in &arrow_polys {
            if ap.points.len() >= 3 {
                let pts_str: Vec<String> = ap.points.iter().map(|p| format!("{},{}", fmt_num(p.x.get()), fmt_num(p.y.get()))).collect();
                let _ = writeln!(body, "{indent}<polygon class=\"arrowhead-marker\" points=\"{}\" fill=\"{col}\"/>", pts_str.join(" "));
            }
        }
    }
}
