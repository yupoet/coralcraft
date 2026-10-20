//! Clean-room SVG 1.1 (`import_svg`) and AutoCAD ASCII DXF (`import_dxf`) vector importers
//! for `coralcraft-doc`.

use coralcraft_color::Color;
use coralcraft_geom::{Affine2, CornerKind, EllipseSpec, F64, FillRule, Path, Point, Rect, RectSpec, Size};

use crate::model::{DocError, Document, ObjectKind, ObjectNode};
use crate::style::{FillStyle, LineCap, LineJoin, OutlineStyle};

fn push_to_first_layer(doc: &mut Document, node: ObjectNode) {
    if let Some(page) = doc.pages.first_mut()
        && let Some(layer) = page.layers.first_mut()
    {
        layer.objects.push(node);
    }
}

fn push_svg_node(doc: &mut Document, group_stack: &mut [(Option<String>, Affine2, Vec<ObjectNode>)], node: ObjectNode) {
    if let Some((_, _, children)) = group_stack.last_mut() {
        children.push(node);
    } else {
        push_to_first_layer(doc, node);
    }
}

/// Imports a clean-room SVG 1.1 document string into a [`Document`].
pub fn import_svg(svg_str: &str) -> Result<Document, DocError> {
    if !svg_str.contains("<svg") {
        return Err(DocError::InvalidSvg("missing <svg> root element".to_owned()));
    }
    let mut doc = Document::new_a4_portrait();
    if let Some(svg_open) = extract_tag_slice(svg_str, "<svg") {
        let w = parse_attr_f64(svg_open, "width").unwrap_or(595.276);
        let h = parse_attr_f64(svg_open, "height").unwrap_or(841.89);
        if let Ok(sz) = Size::new(w.max(1.0), h.max(1.0))
            && let Some(page) = doc.pages.first_mut()
        {
            page.size = sz;
        }
    }

    let mut pos = 0usize;
    let mut in_defs = false;
    let mut passthrough_g_stack: Vec<bool> = Vec::new();
    let mut group_stack: Vec<(Option<String>, Affine2, Vec<ObjectNode>)> = Vec::new();

    while let Some(rem) = svg_str.get(pos..) {
        let Some(lt_rel) = rem.find('<') else {
            break;
        };
        let start = pos.saturating_add(lt_rel);
        let Some(from_start) = svg_str.get(start..) else {
            break;
        };
        let Some(gt_rel) = from_start.find('>') else {
            break;
        };
        let end = start.saturating_add(gt_rel).saturating_add(1);
        let Some(tag_slice) = svg_str.get(start..end) else {
            break;
        };
        pos = end;

        if tag_slice.starts_with("<defs") && !tag_slice.ends_with("/>") {
            in_defs = true;
            continue;
        } else if tag_slice.starts_with("</defs>") {
            in_defs = false;
            continue;
        }
        if in_defs {
            continue;
        }

        if (tag_slice.starts_with("<g ") || tag_slice.starts_with("<g\t") || tag_slice == "<g>") && !tag_slice.ends_with("/>") {
            let id_str = extract_attr_str(tag_slice, "id").unwrap_or_default();
            if id_str.starts_with("layer-") || id_str == "prepress-marks" {
                passthrough_g_stack.push(true);
            } else {
                passthrough_g_stack.push(false);
                let name = if id_str.is_empty() { Some("SVG Group".to_owned()) } else { Some(id_str.to_owned()) };
                let tf = parse_svg_transform(tag_slice);
                group_stack.push((name, tf, Vec::new()));
            }
            continue;
        } else if tag_slice.starts_with("</g>") {
            if passthrough_g_stack.pop() == Some(false)
                && let Some((name, tf, children)) = group_stack.pop()
            {
                let obj_id = doc.allocate_object_id();
                let mut grp = ObjectNode::new(obj_id, name, ObjectKind::Group { children }, None, None);
                grp.transform = tf;
                push_svg_node(&mut doc, &mut group_stack, grp);
            }
            continue;
        }

        if tag_slice.starts_with("<rect ") || tag_slice.starts_with("<rect\t") {
            let id_str = extract_attr_str(tag_slice, "id").unwrap_or_default();
            if id_str == "page-background" {
                continue;
            }
            let x = parse_attr_f64(tag_slice, "x").unwrap_or(0.0);
            let y = parse_attr_f64(tag_slice, "y").unwrap_or(0.0);
            let w = parse_attr_f64(tag_slice, "width").unwrap_or(0.0);
            let h = parse_attr_f64(tag_slice, "height").unwrap_or(0.0);
            if w > 0.0
                && h > 0.0
                && let Ok(r) = Rect::new(x, y, x + w, y + h)
            {
                let rx_opt = parse_attr_f64(tag_slice, "rx").filter(|&v| v > 0.0);
                let ry_opt = parse_attr_f64(tag_slice, "ry").filter(|&v| v > 0.0);
                let rad = match (rx_opt, ry_opt) {
                    (Some(rx), Some(ry)) => rx.max(ry),
                    (Some(rx), None) => rx,
                    (None, Some(ry)) => ry,
                    (None, None) => 0.0,
                }
                .clamp(0.0, 0.5 * w.min(h));
                let rspec =
                    if rad > 0.0 { RectSpec::uniform(r, rad, CornerKind::Round).unwrap_or_else(|_| RectSpec::sharp(r)) } else { RectSpec::sharp(r) };
                let (fill, outline) = parse_svg_style_attrs(tag_slice);
                let obj_id = doc.allocate_object_id();
                let mut node = ObjectNode::new(obj_id, Some("SVG Rect".to_owned()), ObjectKind::Rect(rspec), fill, outline);
                node.transform = parse_svg_transform(tag_slice);
                push_svg_node(&mut doc, &mut group_stack, node);
            }
        } else if tag_slice.starts_with("<ellipse ") || tag_slice.starts_with("<circle ") {
            let cx = parse_attr_f64(tag_slice, "cx").unwrap_or(0.0);
            let cy = parse_attr_f64(tag_slice, "cy").unwrap_or(0.0);
            let rx = parse_attr_f64(tag_slice, "rx").or_else(|| parse_attr_f64(tag_slice, "r")).unwrap_or(0.0);
            let ry = parse_attr_f64(tag_slice, "ry").or_else(|| parse_attr_f64(tag_slice, "r")).unwrap_or(0.0);
            if rx > 0.0
                && ry > 0.0
                && let Ok(r) = Rect::new(cx - rx, cy - ry, cx + rx, cy + ry)
            {
                let (fill, outline) = parse_svg_style_attrs(tag_slice);
                let obj_id = doc.allocate_object_id();
                let mut node = ObjectNode::new(obj_id, Some("SVG Ellipse".to_owned()), ObjectKind::Ellipse(EllipseSpec::full(r)), fill, outline);
                node.transform = parse_svg_transform(tag_slice);
                push_svg_node(&mut doc, &mut group_stack, node);
            }
        } else if tag_slice.starts_with("<line ") || tag_slice.starts_with("<line\t") {
            let x1 = parse_attr_f64(tag_slice, "x1").unwrap_or(0.0);
            let y1 = parse_attr_f64(tag_slice, "y1").unwrap_or(0.0);
            let x2 = parse_attr_f64(tag_slice, "x2").unwrap_or(0.0);
            let y2 = parse_attr_f64(tag_slice, "y2").unwrap_or(0.0);
            if let (Ok(p1), Ok(p2)) = (Point::new(x1, y1), Point::new(x2, y2)) {
                let mut path = Path::new();
                if path.move_to(p1).is_ok() && path.line_to(p2).is_ok() {
                    let (fill, outline) = parse_svg_style_attrs(tag_slice);
                    let obj_id = doc.allocate_object_id();
                    let mut node = ObjectNode::new(obj_id, Some("SVG Line".to_owned()), ObjectKind::Path(path), fill, outline);
                    node.transform = parse_svg_transform(tag_slice);
                    push_svg_node(&mut doc, &mut group_stack, node);
                }
            }
        } else if tag_slice.starts_with("<polyline ")
            || tag_slice.starts_with("<polyline\t")
            || tag_slice.starts_with("<polygon ")
            || tag_slice.starts_with("<polygon\t")
        {
            let is_polygon = tag_slice.starts_with("<polygon");
            if let Some(pts_str) = extract_attr_str(tag_slice, "points")
                && let Some(mut path) = parse_svg_points_path(pts_str, is_polygon)
            {
                if extract_attr_str(tag_slice, "fill-rule").map(str::trim) == Some("evenodd") {
                    path.fill_rule = FillRule::EvenOdd;
                }
                let (fill, outline) = parse_svg_style_attrs(tag_slice);
                let obj_id = doc.allocate_object_id();
                let label = if is_polygon { "SVG Polygon" } else { "SVG Polyline" };
                let mut node = ObjectNode::new(obj_id, Some(label.to_owned()), ObjectKind::Path(path), fill, outline);
                node.transform = parse_svg_transform(tag_slice);
                push_svg_node(&mut doc, &mut group_stack, node);
            }
        } else if tag_slice.starts_with("<path ") {
            let class_str = extract_attr_str(tag_slice, "class").unwrap_or_default();
            if class_str == "crop-marks" {
                continue;
            }
            if let Some(d_str) = extract_attr_str(tag_slice, "d")
                && let Ok(mut path) = parse_svg_path_d(d_str)
                && !path.is_empty()
            {
                if extract_attr_str(tag_slice, "fill-rule").map(str::trim) == Some("evenodd") {
                    path.fill_rule = FillRule::EvenOdd;
                }
                let (fill, outline) = parse_svg_style_attrs(tag_slice);
                let obj_id = doc.allocate_object_id();
                let mut node = ObjectNode::new(obj_id, Some("SVG Path".to_owned()), ObjectKind::Path(path), fill, outline);
                node.transform = parse_svg_transform(tag_slice);
                push_svg_node(&mut doc, &mut group_stack, node);
            }
        } else if tag_slice.starts_with("<text ")
            && let Some(after_open) = svg_str.get(pos..)
            && let Some(close_rel) = after_open.find("</text>")
        {
            let text_content = after_open.get(..close_rel).unwrap_or_default().trim().to_owned();
            pos = pos.saturating_add(close_rel).saturating_add(7);
            let x = parse_attr_f64(tag_slice, "x").unwrap_or(0.0);
            let y = parse_attr_f64(tag_slice, "y").unwrap_or(0.0);
            let font_family = extract_attr_str(tag_slice, "font-family").unwrap_or("Arial").to_owned();
            let size_val = parse_attr_f64(tag_slice, "font-size").unwrap_or(12.0).max(1.0);
            if let (Ok(position), Ok(size_pt)) = (Point::new(x, y), F64::new(size_val)) {
                let (fill, outline) = parse_svg_style_attrs(tag_slice);
                let obj_id = doc.allocate_object_id();
                let mut node = ObjectNode::new(
                    obj_id,
                    Some("SVG Text".to_owned()),
                    ObjectKind::ArtisticText { text: text_content, font_family, size_pt, position },
                    fill,
                    outline,
                );
                node.transform = parse_svg_transform(tag_slice);
                push_svg_node(&mut doc, &mut group_stack, node);
            }
        }
    }

    while let Some((name, tf, children)) = group_stack.pop() {
        let obj_id = doc.allocate_object_id();
        let mut grp = ObjectNode::new(obj_id, name, ObjectKind::Group { children }, None, None);
        grp.transform = tf;
        push_svg_node(&mut doc, &mut group_stack, grp);
    }

    Ok(doc)
}

fn parse_svg_transform(tag_slice: &str) -> Affine2 {
    let Some(raw) = extract_attr_str(tag_slice, "transform") else {
        return Affine2::IDENTITY;
    };
    let s = raw.trim();
    if let Some(rest) = s.strip_prefix("matrix(")
        && let Some(inner) = rest.strip_suffix(')')
    {
        let nums: Vec<f64> =
            inner.split(|c: char| c.is_whitespace() || c == ',').filter(|t| !t.is_empty()).filter_map(|t| t.parse::<f64>().ok()).collect();
        if let [a, b, c, d, tx, ty] = nums.as_slice() {
            return Affine2::new(*a, *b, *c, *d, *tx, *ty).unwrap_or(Affine2::IDENTITY);
        }
    } else if let Some(rest) = s.strip_prefix("translate(")
        && let Some(inner) = rest.strip_suffix(')')
    {
        let nums: Vec<f64> =
            inner.split(|c: char| c.is_whitespace() || c == ',').filter(|t| !t.is_empty()).filter_map(|t| t.parse::<f64>().ok()).collect();
        match nums.as_slice() {
            [tx, ty] => return Affine2::translation(*tx, *ty).unwrap_or(Affine2::IDENTITY),
            [tx] => return Affine2::translation(*tx, 0.0).unwrap_or(Affine2::IDENTITY),
            _ => {}
        }
    } else if let Some(rest) = s.strip_prefix("scale(")
        && let Some(inner) = rest.strip_suffix(')')
    {
        let nums: Vec<f64> =
            inner.split(|c: char| c.is_whitespace() || c == ',').filter(|t| !t.is_empty()).filter_map(|t| t.parse::<f64>().ok()).collect();
        match nums.as_slice() {
            [sx, sy] => return Affine2::scaling(*sx, *sy).unwrap_or(Affine2::IDENTITY),
            [sx] => return Affine2::scaling(*sx, *sx).unwrap_or(Affine2::IDENTITY),
            _ => {}
        }
    } else if let Some(rest) = s.strip_prefix("rotate(")
        && let Some(inner) = rest.strip_suffix(')')
    {
        let nums: Vec<f64> =
            inner.split(|c: char| c.is_whitespace() || c == ',').filter(|t| !t.is_empty()).filter_map(|t| t.parse::<f64>().ok()).collect();
        match nums.as_slice() {
            [ang, cx, cy] => {
                if let (Ok(t0), Ok(rot), Ok(t1)) = (Affine2::translation(-*cx, -*cy), Affine2::rotation_deg(*ang), Affine2::translation(*cx, *cy))
                    && let Ok(tr) = t0.then(rot).and_then(|m| m.then(t1))
                {
                    return tr;
                }
            }
            [ang] => return Affine2::rotation_deg(*ang).unwrap_or(Affine2::IDENTITY),
            _ => {}
        }
    }
    Affine2::IDENTITY
}

fn token_f64(tokens: &[String], idx: usize) -> Option<f64> {
    tokens.get(idx)?.parse::<f64>().ok()
}

fn parse_svg_path_d(d_str: &str) -> Result<Path, coralcraft_geom::GeomError> {
    let mut path = Path::new();
    let mut tokens = Vec::new();
    let mut cur = String::new();
    for ch in d_str.chars() {
        if ch.is_ascii_alphabetic() {
            if !cur.is_empty() {
                tokens.push(std::mem::take(&mut cur));
            }
            tokens.push(ch.to_string());
        } else if ch.is_whitespace() || ch == ',' {
            if !cur.is_empty() {
                tokens.push(std::mem::take(&mut cur));
            }
        } else {
            cur.push(ch);
        }
    }
    if !cur.is_empty() {
        tokens.push(cur);
    }

    let mut i = 0usize;
    let mut cur_x = 0.0_f64;
    let mut cur_y = 0.0_f64;
    let mut sub_x = 0.0_f64;
    let mut sub_y = 0.0_f64;
    while let Some(cmd) = tokens.get(i).map(String::as_str) {
        i = i.saturating_add(1);
        match cmd {
            "M" | "m" if i.saturating_add(1) < tokens.len() => {
                if let (Some(dx), Some(dy)) = (token_f64(&tokens, i), token_f64(&tokens, i.saturating_add(1))) {
                    let (nx, ny) = if cmd == "m" { (cur_x + dx, cur_y + dy) } else { (dx, dy) };
                    let pt = Point::new(nx, ny)?;
                    path.move_to(pt)?;
                    cur_x = nx;
                    cur_y = ny;
                    sub_x = nx;
                    sub_y = ny;
                }
                i = i.saturating_add(2);
            }
            "L" | "l" if i.saturating_add(1) < tokens.len() => {
                if let (Some(dx), Some(dy)) = (token_f64(&tokens, i), token_f64(&tokens, i.saturating_add(1))) {
                    let (nx, ny) = if cmd == "l" { (cur_x + dx, cur_y + dy) } else { (dx, dy) };
                    let pt = Point::new(nx, ny)?;
                    path.line_to(pt)?;
                    cur_x = nx;
                    cur_y = ny;
                }
                i = i.saturating_add(2);
            }
            "H" | "h" if i < tokens.len() => {
                if let Some(dx) = token_f64(&tokens, i) {
                    let nx = if cmd == "h" { cur_x + dx } else { dx };
                    let pt = Point::new(nx, cur_y)?;
                    path.line_to(pt)?;
                    cur_x = nx;
                }
                i = i.saturating_add(1);
            }
            "V" | "v" if i < tokens.len() => {
                if let Some(dy) = token_f64(&tokens, i) {
                    let ny = if cmd == "v" { cur_y + dy } else { dy };
                    let pt = Point::new(cur_x, ny)?;
                    path.line_to(pt)?;
                    cur_y = ny;
                }
                i = i.saturating_add(1);
            }
            "Q" | "q" if i.saturating_add(3) < tokens.len() => {
                if let (Some(cx), Some(cy), Some(ex), Some(ey)) = (
                    token_f64(&tokens, i),
                    token_f64(&tokens, i.saturating_add(1)),
                    token_f64(&tokens, i.saturating_add(2)),
                    token_f64(&tokens, i.saturating_add(3)),
                ) {
                    let (acx, acy, aex, aey) = if cmd == "q" { (cur_x + cx, cur_y + cy, cur_x + ex, cur_y + ey) } else { (cx, cy, ex, ey) };
                    let ctrl = Point::new(acx, acy)?;
                    let pt = Point::new(aex, aey)?;
                    path.quad_to(ctrl, pt)?;
                    cur_x = aex;
                    cur_y = aey;
                }
                i = i.saturating_add(4);
            }
            "C" | "c" if i.saturating_add(5) < tokens.len() => {
                if let (Some(c1x), Some(c1y), Some(c2x), Some(c2y), Some(ex), Some(ey)) = (
                    token_f64(&tokens, i),
                    token_f64(&tokens, i.saturating_add(1)),
                    token_f64(&tokens, i.saturating_add(2)),
                    token_f64(&tokens, i.saturating_add(3)),
                    token_f64(&tokens, i.saturating_add(4)),
                    token_f64(&tokens, i.saturating_add(5)),
                ) {
                    let (ac1x, ac1y, ac2x, ac2y, aex, aey) = if cmd == "c" {
                        (cur_x + c1x, cur_y + c1y, cur_x + c2x, cur_y + c2y, cur_x + ex, cur_y + ey)
                    } else {
                        (c1x, c1y, c2x, c2y, ex, ey)
                    };
                    let c1 = Point::new(ac1x, ac1y)?;
                    let c2 = Point::new(ac2x, ac2y)?;
                    let pt = Point::new(aex, aey)?;
                    path.cubic_to(c1, c2, pt)?;
                    cur_x = aex;
                    cur_y = aey;
                }
                i = i.saturating_add(6);
            }
            "Z" | "z" => {
                path.close()?;
                cur_x = sub_x;
                cur_y = sub_y;
            }
            _ => {}
        }
    }
    Ok(path)
}

fn parse_svg_points_path(pts_str: &str, close: bool) -> Option<Path> {
    let coords: Vec<f64> =
        pts_str.split(|c: char| c.is_whitespace() || c == ',').filter(|t| !t.is_empty()).filter_map(|t| t.parse::<f64>().ok()).collect();
    let (pairs, rem) = coords.as_chunks::<2>();
    if !rem.is_empty() || pairs.len() < 2 {
        return None;
    }
    let mut path = Path::new();
    for (idx, &[x, y]) in pairs.iter().enumerate() {
        let pt = Point::new(x, y).ok()?;
        if idx == 0 {
            path.move_to(pt).ok()?;
        } else {
            path.line_to(pt).ok()?;
        }
    }
    if close {
        path.close().ok()?;
    }
    Some(path)
}

fn parse_svg_style_attrs(tag_slice: &str) -> (Option<FillStyle>, Option<OutlineStyle>) {
    let fill = match extract_attr_str(tag_slice, "fill") {
        Some("none") | None => None,
        Some(hex) => parse_hex_color(hex).map(FillStyle::uniform),
    };
    let outline = match extract_attr_str(tag_slice, "stroke") {
        Some("none") | None => None,
        Some(hex) => parse_hex_color(hex).map(OutlineStyle::hairline),
    };
    let mut outline = if fill.is_none() && outline.is_none() { Some(OutlineStyle::hairline(Color::BLACK)) } else { outline };
    if let Some(st) = &mut outline {
        if let Some(w) = parse_attr_f64(tag_slice, "stroke-width")
            && w > 0.0
            && let Ok(w_f64) = F64::new(w)
        {
            st.width_pt = w_f64;
        }
        if let Some(cap_str) = extract_attr_str(tag_slice, "stroke-linecap") {
            st.cap = match cap_str.trim() {
                "round" => LineCap::Round,
                "square" => LineCap::Square,
                _ => LineCap::Butt,
            };
        }
        if let Some(join_str) = extract_attr_str(tag_slice, "stroke-linejoin") {
            st.join = match join_str.trim() {
                "round" => LineJoin::Round,
                "bevel" => LineJoin::Bevel,
                _ => LineJoin::Miter,
            };
        }
        if let Some(dash_str) = extract_attr_str(tag_slice, "stroke-dasharray") {
            let trimmed = dash_str.trim();
            if trimmed != "none" && !trimmed.is_empty() {
                let mut dashes: Vec<F64> = trimmed
                    .split(|c: char| c.is_whitespace() || c == ',')
                    .filter(|t| !t.is_empty())
                    .filter_map(|t| t.trim_end_matches("pt").trim_end_matches("px").parse::<f64>().ok())
                    .filter(|&v| v > 0.0)
                    .filter_map(|v| F64::new(v).ok())
                    .collect();
                if !dashes.is_empty() && !dashes.len().is_multiple_of(2) {
                    let dup = dashes.clone();
                    dashes.extend(dup);
                }
                if dashes.len() <= 32 {
                    st.dash_pattern = dashes;
                }
            }
        }
    }
    (fill, outline)
}

fn parse_hex_color(s: &str) -> Option<Color> {
    let hex = s.strip_prefix('#')?;
    // Byte-wise parsing: `len()` counts bytes, so slicing a non-ASCII string could split a char.
    if let [r1, r2, g1, g2, b1, b2] = hex.as_bytes() {
        let digit = |c: &u8| char::from(*c).to_digit(16).and_then(|d| u8::try_from(d).ok());
        let byte = |hi: &u8, lo: &u8| Some((digit(hi)? << 4) | digit(lo)?);
        Some(Color::rgb8(byte(r1, r2)?, byte(g1, g2)?, byte(b1, b2)?))
    } else {
        None
    }
}

fn extract_tag_slice<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    let start = s.find(prefix)?;
    let from_start = s.get(start..)?;
    let end = from_start.find('>')?;
    from_start.get(..end.saturating_add(1))
}

fn extract_attr_str<'a>(tag: &'a str, attr: &str) -> Option<&'a str> {
    let needle = format!("{attr}=\"");
    let start = tag.find(&needle)?.saturating_add(needle.len());
    let from_start = tag.get(start..)?;
    let end = from_start.find('"')?;
    from_start.get(..end)
}

fn parse_attr_f64(tag: &str, attr: &str) -> Option<f64> {
    let raw = extract_attr_str(tag, attr)?;
    let trimmed = raw.trim_end_matches("pt").trim_end_matches("px").trim();
    trimmed.parse::<f64>().ok()
}

/// Imports an AutoCAD ASCII DXF (`SECTION` / `ENTITIES` with `LINE` and `POLYLINE`/`VERTEX`/`SEQEND`) string into a [`Document`].
pub fn import_dxf(dxf_str: &str) -> Result<Document, DocError> {
    if !dxf_str.contains("ENTITIES") {
        return Err(DocError::InvalidDxf("missing ENTITIES section".to_owned()));
    }
    let mut doc = Document::new_a4_portrait();
    let page_h = doc.pages.first().map_or(841.89, |p| p.size.height.get());
    let lines: Vec<&str> = dxf_str.lines().map(str::trim).collect();

    let mut pairs = Vec::with_capacity(lines.len() / 2);
    let mut idx = 0usize;
    while let (Some(&code_line), Some(&val_line)) = (lines.get(idx), lines.get(idx.saturating_add(1))) {
        if let Ok(code) = code_line.parse::<i32>() {
            pairs.push((code, val_line));
            idx = idx.saturating_add(2);
        } else {
            idx = idx.saturating_add(1);
        }
    }

    let mut p = 0usize;
    while let Some(&(code, val)) = pairs.get(p) {
        if code == 0 && val == "LINE" {
            p = p.saturating_add(1);
            let mut x0 = 0.0;
            let mut y0 = 0.0;
            let mut x1 = 0.0;
            let mut y1 = 0.0;
            while let Some(&(c, v)) = pairs.get(p) {
                if c == 0 {
                    break;
                }
                match c {
                    10 => x0 = v.parse::<f64>().unwrap_or(0.0),
                    20 => y0 = v.parse::<f64>().unwrap_or(0.0),
                    11 => x1 = v.parse::<f64>().unwrap_or(0.0),
                    21 => y1 = v.parse::<f64>().unwrap_or(0.0),
                    _ => {}
                }
                p = p.saturating_add(1);
            }
            let mut path = Path::new();
            if let (Ok(pt0), Ok(pt1)) = (Point::new(x0, page_h - y0), Point::new(x1, page_h - y1)) {
                let _ = path.move_to(pt0);
                let _ = path.line_to(pt1);
                let id = doc.allocate_object_id();
                push_to_first_layer(
                    &mut doc,
                    ObjectNode::new(id, Some("DXF Line".to_owned()), ObjectKind::Path(path), None, Some(OutlineStyle::hairline(Color::BLACK))),
                );
            }
        } else if code == 0 && val == "POLYLINE" {
            p = p.saturating_add(1);
            let mut closed = false;
            while let Some(&(c, v)) = pairs.get(p) {
                if c == 0 {
                    break;
                }
                if c == 70 && v.parse::<i32>().unwrap_or(0) & 1 != 0 {
                    closed = true;
                }
                p = p.saturating_add(1);
            }
            let mut pts = Vec::new();
            while let Some(&(c, v)) = pairs.get(p) {
                if c == 0 && v == "VERTEX" {
                    p = p.saturating_add(1);
                    let mut vx = 0.0;
                    let mut vy = 0.0;
                    while let Some(&(vc, vv)) = pairs.get(p) {
                        if vc == 0 {
                            break;
                        }
                        match vc {
                            10 => vx = vv.parse::<f64>().unwrap_or(0.0),
                            20 => vy = vv.parse::<f64>().unwrap_or(0.0),
                            _ => {}
                        }
                        p = p.saturating_add(1);
                    }
                    if let Ok(pt) = Point::new(vx, page_h - vy) {
                        pts.push(pt);
                    }
                } else if c == 0 && v == "SEQEND" {
                    p = p.saturating_add(1);
                    while let Some(&(sc, _)) = pairs.get(p) {
                        if sc == 0 {
                            break;
                        }
                        p = p.saturating_add(1);
                    }
                    break;
                } else {
                    break;
                }
            }
            if let (Some(&first), Some(rest)) = (pts.first(), pts.get(1..))
                && !rest.is_empty()
            {
                let mut path = Path::new();
                let _ = path.move_to(first);
                for &pt in rest {
                    let _ = path.line_to(pt);
                }
                if closed {
                    let _ = path.close();
                }
                let id = doc.allocate_object_id();
                push_to_first_layer(
                    &mut doc,
                    ObjectNode::new(id, Some("DXF Polyline".to_owned()), ObjectKind::Path(path), None, Some(OutlineStyle::hairline(Color::BLACK))),
                );
            }
        } else if code == 0 && val == "CIRCLE" {
            p = p.saturating_add(1);
            let mut cx = 0.0;
            let mut cy = 0.0;
            let mut r = 0.0;
            while let Some(&(c, v)) = pairs.get(p) {
                if c == 0 {
                    break;
                }
                match c {
                    10 => cx = v.parse::<f64>().unwrap_or(0.0),
                    20 => cy = v.parse::<f64>().unwrap_or(0.0),
                    40 => r = v.parse::<f64>().unwrap_or(0.0),
                    _ => {}
                }
                p = p.saturating_add(1);
            }
            let sy = page_h - cy;
            if r > 0.0
                && let Ok(rect) = Rect::new(cx - r, sy - r, cx + r, sy + r)
            {
                let id = doc.allocate_object_id();
                push_to_first_layer(
                    &mut doc,
                    ObjectNode::new(
                        id,
                        Some("DXF Circle".to_owned()),
                        ObjectKind::Ellipse(EllipseSpec::full(rect)),
                        None,
                        Some(OutlineStyle::hairline(Color::BLACK)),
                    ),
                );
            }
        } else if code == 0 && val == "ARC" {
            p = p.saturating_add(1);
            let mut cx = 0.0;
            let mut cy = 0.0;
            let mut r = 0.0;
            let mut a0_deg = 0.0;
            let mut a1_deg = 360.0;
            while let Some(&(c, v)) = pairs.get(p) {
                if c == 0 {
                    break;
                }
                match c {
                    10 => cx = v.parse::<f64>().unwrap_or(0.0),
                    20 => cy = v.parse::<f64>().unwrap_or(0.0),
                    40 => r = v.parse::<f64>().unwrap_or(0.0),
                    50 => a0_deg = v.parse::<f64>().unwrap_or(0.0),
                    51 => a1_deg = v.parse::<f64>().unwrap_or(360.0),
                    _ => {}
                }
                p = p.saturating_add(1);
            }
            let sy = page_h - cy;
            if r > 0.0
                && let Ok(rect) = Rect::new(cx - r, sy - r, cx + r, sy + r)
                && let Ok(spec) = EllipseSpec::arc(rect, (-a1_deg).rem_euclid(360.0), (-a0_deg).rem_euclid(360.0))
            {
                let id = doc.allocate_object_id();
                push_to_first_layer(
                    &mut doc,
                    ObjectNode::new(id, Some("DXF Arc".to_owned()), ObjectKind::Ellipse(spec), None, Some(OutlineStyle::hairline(Color::BLACK))),
                );
            }
        } else if code == 0 && val == "ELLIPSE" {
            p = p.saturating_add(1);
            let mut cx = 0.0;
            let mut cy = 0.0;
            let mut mx = 0.0;
            let mut my = 0.0;
            let mut ratio = 1.0_f64;
            while let Some(&(c, v)) = pairs.get(p) {
                if c == 0 {
                    break;
                }
                match c {
                    10 => cx = v.parse::<f64>().unwrap_or(0.0),
                    20 => cy = v.parse::<f64>().unwrap_or(0.0),
                    11 => mx = v.parse::<f64>().unwrap_or(0.0),
                    21 => my = v.parse::<f64>().unwrap_or(0.0),
                    40 => ratio = v.parse::<f64>().unwrap_or(1.0),
                    _ => {}
                }
                p = p.saturating_add(1);
            }
            let sy = page_h - cy;
            let rx = mx.hypot(my);
            let ry = rx * ratio.clamp(1e-3, 1.0);
            if rx > 0.0
                && ry > 0.0
                && let Ok(rect) = Rect::new(cx - rx, sy - ry, cx + rx, sy + ry)
            {
                let id = doc.allocate_object_id();
                let mut node = ObjectNode::new(
                    id,
                    Some("DXF Ellipse".to_owned()),
                    ObjectKind::Ellipse(EllipseSpec::full(rect)),
                    None,
                    Some(OutlineStyle::hairline(Color::BLACK)),
                );
                let ang_deg = (-my).atan2(mx).to_degrees();
                if ang_deg.abs() > 1e-6
                    && let (Ok(t0), Ok(rot), Ok(t1)) = (Affine2::translation(-cx, -sy), Affine2::rotation_deg(ang_deg), Affine2::translation(cx, sy))
                    && let Ok(tr) = t0.then(rot).and_then(|m| m.then(t1))
                {
                    node.transform = tr;
                }
                push_to_first_layer(&mut doc, node);
            }
        } else if code == 0 && val == "LWPOLYLINE" {
            p = p.saturating_add(1);
            let mut closed = false;
            let mut xs = Vec::new();
            let mut ys = Vec::new();
            while let Some(&(c, v)) = pairs.get(p) {
                if c == 0 {
                    break;
                }
                match c {
                    70 if v.parse::<i32>().unwrap_or(0) & 1 != 0 => closed = true,
                    10 => xs.push(v.parse::<f64>().unwrap_or(0.0)),
                    20 => ys.push(v.parse::<f64>().unwrap_or(0.0)),
                    _ => {}
                }
                p = p.saturating_add(1);
            }
            let mut coords = xs.into_iter().zip(ys);
            if let Some((fx, fy)) = coords.next()
                && let Ok(first) = Point::new(fx, page_h - fy)
            {
                let mut path = Path::new();
                let _ = path.move_to(first);
                let mut count = 1usize;
                for (kx, ky) in coords {
                    if let Ok(pt) = Point::new(kx, page_h - ky) {
                        let _ = path.line_to(pt);
                        count = count.saturating_add(1);
                    }
                }
                if count >= 2 {
                    if closed {
                        let _ = path.close();
                    }
                    let id = doc.allocate_object_id();
                    push_to_first_layer(
                        &mut doc,
                        ObjectNode::new(
                            id,
                            Some("DXF LwPolyline".to_owned()),
                            ObjectKind::Path(path),
                            None,
                            Some(OutlineStyle::hairline(Color::BLACK)),
                        ),
                    );
                }
            }
        } else if code == 0 && val == "TEXT" {
            p = p.saturating_add(1);
            let mut tx = 0.0;
            let mut ty = 0.0;
            let mut height = 12.0;
            let mut text_val = String::new();
            while let Some(&(c, v)) = pairs.get(p) {
                if c == 0 {
                    break;
                }
                match c {
                    10 => tx = v.parse::<f64>().unwrap_or(0.0),
                    20 => ty = v.parse::<f64>().unwrap_or(0.0),
                    40 => height = v.parse::<f64>().unwrap_or(12.0),
                    1 => text_val = v.to_owned(),
                    _ => {}
                }
                p = p.saturating_add(1);
            }
            if !text_val.is_empty()
                && let Ok(pos) = Point::new(tx, page_h - ty)
                && let Ok(size_pt) = F64::new(height.max(1.0))
            {
                let id = doc.allocate_object_id();
                push_to_first_layer(
                    &mut doc,
                    ObjectNode::new(
                        id,
                        Some("DXF Text".to_owned()),
                        ObjectKind::ArtisticText { text: text_val, font_family: "Sans".to_owned(), size_pt, position: pos },
                        Some(FillStyle::uniform(Color::BLACK)),
                        None,
                    ),
                );
            }
        } else {
            p = p.saturating_add(1);
        }
    }

    Ok(doc)
}
