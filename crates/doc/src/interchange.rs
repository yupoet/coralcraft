//! Clean-room SVG 1.1 (`import_svg`) and AutoCAD ASCII DXF (`import_dxf`) vector importers
//! for `coralcraft-doc`.

use coralcraft_color::Color;
use coralcraft_geom::{EllipseSpec, F64, Path, Point, Rect, RectSpec, Size};

use crate::model::{DocError, Document, ObjectKind, ObjectNode};
use crate::style::{FillStyle, OutlineStyle};

/// Imports a clean-room SVG 1.1 document string into a [`Document`].
pub fn import_svg(svg_str: &str) -> Result<Document, DocError> {
    if !svg_str.contains("<svg") {
        return Err(DocError::InvalidSvg("missing <svg> root element".to_owned()));
    }
    let mut doc = Document::new_a4_portrait();
    if let Some(svg_open) = extract_tag_slice(svg_str, "<svg") {
        let w = parse_attr_f64(svg_open, "width").unwrap_or(595.276);
        let h = parse_attr_f64(svg_open, "height").unwrap_or(841.89);
        if let Ok(sz) = Size::new(w.max(1.0), h.max(1.0)) {
            doc.pages[0].size = sz;
        }
    }

    let mut pos = 0usize;
    while pos < svg_str.len() {
        let Some(lt_rel) = svg_str[pos..].find('<') else {
            break;
        };
        let start = pos + lt_rel;
        let Some(gt_rel) = svg_str[start..].find('>') else {
            break;
        };
        let end = start + gt_rel + 1;
        let tag_slice = &svg_str[start..end];
        pos = end;

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
                let (fill, outline) = parse_svg_style_attrs(tag_slice);
                let obj_id = doc.allocate_object_id();
                doc.pages[0].layers[0].objects.push(ObjectNode::new(
                    obj_id,
                    Some("SVG Rect".to_owned()),
                    ObjectKind::Rect(RectSpec::sharp(r)),
                    fill,
                    outline,
                ));
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
                doc.pages[0].layers[0].objects.push(ObjectNode::new(
                    obj_id,
                    Some("SVG Ellipse".to_owned()),
                    ObjectKind::Ellipse(EllipseSpec::full(r)),
                    fill,
                    outline,
                ));
            }
        } else if tag_slice.starts_with("<path ") {
            let class_str = extract_attr_str(tag_slice, "class").unwrap_or_default();
            if class_str == "crop-marks" {
                continue;
            }
            if let Some(d_str) = extract_attr_str(tag_slice, "d")
                && let Ok(path) = parse_svg_path_d(d_str)
                && !path.is_empty()
            {
                let (fill, outline) = parse_svg_style_attrs(tag_slice);
                let obj_id = doc.allocate_object_id();
                doc.pages[0].layers[0].objects.push(ObjectNode::new(obj_id, Some("SVG Path".to_owned()), ObjectKind::Path(path), fill, outline));
            }
        } else if tag_slice.starts_with("<text ")
            && let Some(close_rel) = svg_str[pos..].find("</text>")
        {
            let text_content = svg_str[pos..pos + close_rel].trim().to_owned();
            pos += close_rel + 7;
            let x = parse_attr_f64(tag_slice, "x").unwrap_or(0.0);
            let y = parse_attr_f64(tag_slice, "y").unwrap_or(0.0);
            let font_family = extract_attr_str(tag_slice, "font-family").unwrap_or("Arial").to_owned();
            let size_val = parse_attr_f64(tag_slice, "font-size").unwrap_or(12.0).max(1.0);
            if let (Ok(position), Ok(size_pt)) = (Point::new(x, y), F64::new(size_val)) {
                let (fill, outline) = parse_svg_style_attrs(tag_slice);
                let obj_id = doc.allocate_object_id();
                doc.pages[0].layers[0].objects.push(ObjectNode::new(
                    obj_id,
                    Some("SVG Text".to_owned()),
                    ObjectKind::ArtisticText { text: text_content, font_family, size_pt, position },
                    fill,
                    outline,
                ));
            }
        }
    }

    Ok(doc)
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
    while i < tokens.len() {
        let cmd = tokens[i].as_str();
        i += 1;
        match cmd {
            "M" if i + 1 < tokens.len() => {
                if let (Ok(x), Ok(y)) = (tokens[i].parse::<f64>(), tokens[i + 1].parse::<f64>()) {
                    let pt = Point::new(x, y)?;
                    path.move_to(pt)?;
                }
                i += 2;
            }
            "L" if i + 1 < tokens.len() => {
                if let (Ok(x), Ok(y)) = (tokens[i].parse::<f64>(), tokens[i + 1].parse::<f64>()) {
                    let pt = Point::new(x, y)?;
                    path.line_to(pt)?;
                }
                i += 2;
            }
            "Q" if i + 3 < tokens.len() => {
                if let (Ok(cx), Ok(cy), Ok(ex), Ok(ey)) =
                    (tokens[i].parse::<f64>(), tokens[i + 1].parse::<f64>(), tokens[i + 2].parse::<f64>(), tokens[i + 3].parse::<f64>())
                {
                    let ctrl = Point::new(cx, cy)?;
                    let pt = Point::new(ex, ey)?;
                    path.quad_to(ctrl, pt)?;
                }
                i += 4;
            }
            "C" if i + 5 < tokens.len() => {
                if let (Ok(c1x), Ok(c1y), Ok(c2x), Ok(c2y), Ok(ex), Ok(ey)) = (
                    tokens[i].parse::<f64>(),
                    tokens[i + 1].parse::<f64>(),
                    tokens[i + 2].parse::<f64>(),
                    tokens[i + 3].parse::<f64>(),
                    tokens[i + 4].parse::<f64>(),
                    tokens[i + 5].parse::<f64>(),
                ) {
                    let c1 = Point::new(c1x, c1y)?;
                    let c2 = Point::new(c2x, c2y)?;
                    let pt = Point::new(ex, ey)?;
                    path.cubic_to(c1, c2, pt)?;
                }
                i += 6;
            }
            "Z" | "z" => {
                path.close()?;
            }
            _ => {}
        }
    }
    Ok(path)
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
    let outline = if fill.is_none() && outline.is_none() { Some(OutlineStyle::hairline(Color::BLACK)) } else { outline };
    (fill, outline)
}

fn parse_hex_color(s: &str) -> Option<Color> {
    let hex = s.strip_prefix('#')?;
    // Byte-wise parsing: `len()` counts bytes, so slicing a non-ASCII string could split a char.
    if let [r1, r2, g1, g2, b1, b2] = hex.as_bytes() {
        let digit = |c: &u8| char::from(*c).to_digit(16).and_then(|d| u8::try_from(d).ok());
        let byte = |hi: &u8, lo: &u8| Some(digit(hi)? * 16 + digit(lo)?);
        Some(Color::rgb8(byte(r1, r2)?, byte(g1, g2)?, byte(b1, b2)?))
    } else {
        None
    }
}

fn extract_tag_slice<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    let start = s.find(prefix)?;
    let end = s[start..].find('>')?;
    Some(&s[start..start + end + 1])
}

fn extract_attr_str<'a>(tag: &'a str, attr: &str) -> Option<&'a str> {
    let needle = format!("{attr}=\"");
    let start = tag.find(&needle)? + needle.len();
    let end = tag[start..].find('"')?;
    Some(&tag[start..start + end])
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
    let page_h = doc.pages[0].size.height.get();
    let lines: Vec<&str> = dxf_str.lines().map(str::trim).collect();

    let mut pairs = Vec::with_capacity(lines.len() / 2);
    let mut idx = 0usize;
    while idx + 1 < lines.len() {
        if let Ok(code) = lines[idx].parse::<i32>() {
            pairs.push((code, lines[idx + 1]));
            idx += 2;
        } else {
            idx += 1;
        }
    }

    let mut p = 0usize;
    while p < pairs.len() {
        let (code, val) = pairs[p];
        if code == 0 && val == "LINE" {
            p += 1;
            let mut x0 = 0.0;
            let mut y0 = 0.0;
            let mut x1 = 0.0;
            let mut y1 = 0.0;
            while p < pairs.len() && pairs[p].0 != 0 {
                match pairs[p].0 {
                    10 => x0 = pairs[p].1.parse::<f64>().unwrap_or(0.0),
                    20 => y0 = pairs[p].1.parse::<f64>().unwrap_or(0.0),
                    11 => x1 = pairs[p].1.parse::<f64>().unwrap_or(0.0),
                    21 => y1 = pairs[p].1.parse::<f64>().unwrap_or(0.0),
                    _ => {}
                }
                p += 1;
            }
            let mut path = Path::new();
            if let (Ok(pt0), Ok(pt1)) = (Point::new(x0, page_h - y0), Point::new(x1, page_h - y1)) {
                let _ = path.move_to(pt0);
                let _ = path.line_to(pt1);
                let id = doc.allocate_object_id();
                doc.pages[0].layers[0].objects.push(ObjectNode::new(
                    id,
                    Some("DXF Line".to_owned()),
                    ObjectKind::Path(path),
                    None,
                    Some(OutlineStyle::hairline(Color::BLACK)),
                ));
            }
        } else if code == 0 && val == "POLYLINE" {
            p += 1;
            let mut closed = false;
            while p < pairs.len() && pairs[p].0 != 0 {
                if pairs[p].0 == 70 && pairs[p].1.parse::<i32>().unwrap_or(0) & 1 != 0 {
                    closed = true;
                }
                p += 1;
            }
            let mut pts = Vec::new();
            while p < pairs.len() {
                if pairs[p].0 == 0 && pairs[p].1 == "VERTEX" {
                    p += 1;
                    let mut vx = 0.0;
                    let mut vy = 0.0;
                    while p < pairs.len() && pairs[p].0 != 0 {
                        match pairs[p].0 {
                            10 => vx = pairs[p].1.parse::<f64>().unwrap_or(0.0),
                            20 => vy = pairs[p].1.parse::<f64>().unwrap_or(0.0),
                            _ => {}
                        }
                        p += 1;
                    }
                    if let Ok(pt) = Point::new(vx, page_h - vy) {
                        pts.push(pt);
                    }
                } else if pairs[p].0 == 0 && pairs[p].1 == "SEQEND" {
                    p += 1;
                    while p < pairs.len() && pairs[p].0 != 0 {
                        p += 1;
                    }
                    break;
                } else {
                    break;
                }
            }
            if pts.len() >= 2 {
                let mut path = Path::new();
                let _ = path.move_to(pts[0]);
                for &pt in &pts[1..] {
                    let _ = path.line_to(pt);
                }
                if closed {
                    let _ = path.close();
                }
                let id = doc.allocate_object_id();
                doc.pages[0].layers[0].objects.push(ObjectNode::new(
                    id,
                    Some("DXF Polyline".to_owned()),
                    ObjectKind::Path(path),
                    None,
                    Some(OutlineStyle::hairline(Color::BLACK)),
                ));
            }
        } else if code == 0 && val == "CIRCLE" {
            p += 1;
            let mut cx = 0.0;
            let mut cy = 0.0;
            let mut r = 0.0;
            while p < pairs.len() && pairs[p].0 != 0 {
                match pairs[p].0 {
                    10 => cx = pairs[p].1.parse::<f64>().unwrap_or(0.0),
                    20 => cy = pairs[p].1.parse::<f64>().unwrap_or(0.0),
                    40 => r = pairs[p].1.parse::<f64>().unwrap_or(0.0),
                    _ => {}
                }
                p += 1;
            }
            let sy = page_h - cy;
            if r > 0.0
                && let Ok(rect) = Rect::new(cx - r, sy - r, cx + r, sy + r)
            {
                let id = doc.allocate_object_id();
                doc.pages[0].layers[0].objects.push(ObjectNode::new(
                    id,
                    Some("DXF Circle".to_owned()),
                    ObjectKind::Ellipse(EllipseSpec::full(rect)),
                    None,
                    Some(OutlineStyle::hairline(Color::BLACK)),
                ));
            }
        } else if code == 0 && val == "LWPOLYLINE" {
            p += 1;
            let mut closed = false;
            let mut xs = Vec::new();
            let mut ys = Vec::new();
            while p < pairs.len() && pairs[p].0 != 0 {
                match pairs[p].0 {
                    70 if pairs[p].1.parse::<i32>().unwrap_or(0) & 1 != 0 => closed = true,
                    10 => xs.push(pairs[p].1.parse::<f64>().unwrap_or(0.0)),
                    20 => ys.push(pairs[p].1.parse::<f64>().unwrap_or(0.0)),
                    _ => {}
                }
                p += 1;
            }
            let n = xs.len().min(ys.len());
            if n >= 2 {
                let mut path = Path::new();
                if let Ok(first) = Point::new(xs[0], page_h - ys[0]) {
                    let _ = path.move_to(first);
                    for k in 1..n {
                        if let Ok(pt) = Point::new(xs[k], page_h - ys[k]) {
                            let _ = path.line_to(pt);
                        }
                    }
                    if closed {
                        let _ = path.close();
                    }
                    let id = doc.allocate_object_id();
                    doc.pages[0].layers[0].objects.push(ObjectNode::new(
                        id,
                        Some("DXF LwPolyline".to_owned()),
                        ObjectKind::Path(path),
                        None,
                        Some(OutlineStyle::hairline(Color::BLACK)),
                    ));
                }
            }
        } else {
            p += 1;
        }
    }

    Ok(doc)
}
