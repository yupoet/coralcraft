//! Clean-room SVG 1.1 (`import_svg`) and AutoCAD ASCII DXF (`import_dxf`) vector importers
//! for `coralcraft-doc`.

use std::collections::BTreeMap;

use coralcraft_color::{Color, FountainBlendMode, FountainStop};
use coralcraft_geom::{Affine2, CornerKind, EllipseSpec, F64, FillRule, Path, Point, Rect, RectSpec, Size};

use crate::model::{DocError, Document, Layer, LayerId, ObjectKind, ObjectNode, ParagraphTextSpec, TextAlignment, TransparencyStyle};
use crate::style::{ArrowheadKind, FillStyle, FountainGeometry, LineCap, LineJoin, OutlineStyle, PatternKind};

fn push_to_active_layer(doc: &mut Document, node: ObjectNode) {
    if let Some(page) = doc.pages.first_mut()
        && let Some(layer) = page.layers.last_mut()
    {
        layer.objects.push(node);
    }
}

type SvgGroupFrame = (Option<String>, bool, bool, F64, Affine2, Option<String>, Option<TransparencyStyle>, Vec<ObjectNode>);

fn push_svg_node(doc: &mut Document, group_stack: &mut [SvgGroupFrame], node: ObjectNode) {
    if let Some((_, _, _, _, _, _, _, children)) = group_stack.last_mut() {
        children.push(node);
    } else {
        push_to_active_layer(doc, node);
    }
}

fn last_svg_sibling_mut<'a>(doc: &'a mut Document, group_stack: &'a mut [SvgGroupFrame]) -> Option<&'a mut ObjectNode> {
    if let Some((_, _, _, _, _, _, _, children)) = group_stack.last_mut() {
        children.last_mut()
    } else {
        doc.pages.first_mut()?.layers.last_mut()?.objects.last_mut()
    }
}

/// Imports a clean-room SVG 1.1 document string into a [`Document`].
pub fn import_svg(svg_str: &str) -> Result<Document, DocError> {
    if !svg_str.contains("<svg") {
        return Err(DocError::InvalidSvg("missing <svg> root element".to_owned()));
    }
    let mut doc = Document::new_a4_portrait();
    if let Some(svg_open) = extract_tag_slice(svg_str, "<svg") {
        let vb_dims = extract_attr_str(svg_open, "viewBox").and_then(|vb| {
            let nums: Vec<f64> =
                vb.split(|c: char| c.is_whitespace() || c == ',').filter(|t| !t.is_empty()).filter_map(|t| t.parse::<f64>().ok()).collect();
            if let [_, _, vw, vh] = nums.as_slice() { Some((*vw, *vh)) } else { None }
        });
        let w = parse_attr_f64(svg_open, "width").or_else(|| vb_dims.map(|(vw, _)| vw)).unwrap_or(595.276);
        let h = parse_attr_f64(svg_open, "height").or_else(|| vb_dims.map(|(_, vh)| vh)).unwrap_or(841.89);
        if let Ok(sz) = Size::new(w.max(1.0), h.max(1.0))
            && let Some(page) = doc.pages.first_mut()
        {
            page.size = sz;
        }
    }

    let mut pos = 0usize;
    let mut in_defs = false;
    let mut svg_gradients: BTreeMap<String, FillStyle> = BTreeMap::new();
    let mut svg_masks: BTreeMap<String, TransparencyStyle> = BTreeMap::new();
    let mut svg_clip_paths: BTreeMap<String, Path> = BTreeMap::new();
    let mut active_grad: Option<(String, FountainGeometry, Vec<FountainStop>)> = None;
    let mut passthrough_g_stack: Vec<bool> = Vec::new();
    let mut group_stack: Vec<SvgGroupFrame> = Vec::new();
    let mut saw_layer_group = false;

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

        if tag_slice.starts_with("<linearGradient ") || tag_slice.starts_with("<linearGradient\t") {
            if let Some(id) = extract_attr_str(tag_slice, "id") {
                let x1 = parse_svg_coord_or_pct(tag_slice, "x1").unwrap_or(0.0);
                let y1 = parse_svg_coord_or_pct(tag_slice, "y1").unwrap_or(0.0);
                let x2 = parse_svg_coord_or_pct(tag_slice, "x2").unwrap_or(1.0);
                let y2 = parse_svg_coord_or_pct(tag_slice, "y2").unwrap_or(0.0);
                let dx = x2 - x1;
                let dy = y2 - y1;
                let ang = if dx.abs() < 1e-9 && dy.abs() < 1e-9 { 0.0 } else { dy.atan2(dx).to_degrees() };
                let geom = FountainGeometry::Linear { angle_deg: F64::new(ang).unwrap_or(F64::ZERO), edge_pad: F64::ZERO };
                if !tag_slice.ends_with("/>") {
                    active_grad = Some((id.to_owned(), geom, Vec::new()));
                }
            }
            continue;
        } else if tag_slice.starts_with("<radialGradient ") || tag_slice.starts_with("<radialGradient\t") {
            if let Some(id) = extract_attr_str(tag_slice, "id") {
                let ftype = extract_attr_str(tag_slice, "data-fountain-type").unwrap_or("radial");
                let ang = F64::new(parse_attr_f64(tag_slice, "data-angle").unwrap_or(0.0)).unwrap_or(F64::ZERO);
                let geom = match ftype {
                    "conical" => FountainGeometry::Conical { center_x: F64::HALF, center_y: F64::HALF, angle_deg: ang },
                    "square" => FountainGeometry::Square { center_x: F64::HALF, center_y: F64::HALF, radius: F64::HALF, angle_deg: ang },
                    _ => FountainGeometry::Radial {
                        center_x: F64::HALF,
                        center_y: F64::HALF,
                        radius: F64::HALF,
                        focal_x: F64::HALF,
                        focal_y: F64::HALF,
                    },
                };
                if !tag_slice.ends_with("/>") {
                    active_grad = Some((id.to_owned(), geom, Vec::new()));
                }
            }
            continue;
        } else if (tag_slice.starts_with("<stop ") || tag_slice.starts_with("<stop\t"))
            && let Some((_, _, stops)) = &mut active_grad
        {
            let off_raw = extract_style_or_attr_str(tag_slice, "offset").unwrap_or("0");
            let off_t = off_raw.trim();
            let off_val = if let Some(pct) = off_t.strip_suffix('%') {
                pct.trim().parse::<f64>().map(|v| v / 100.0).unwrap_or(0.0)
            } else {
                off_t.parse::<f64>().unwrap_or(0.0)
            }
            .clamp(0.0, 1.0);
            let col = extract_style_or_attr_str(tag_slice, "stop-color").and_then(|s| parse_svg_color(s.trim())).unwrap_or(Color::BLACK);
            if let Ok(st) = FountainStop::new(off_val, col) {
                stops.push(st);
            }
            continue;
        } else if tag_slice.starts_with("</linearGradient>") || tag_slice.starts_with("</radialGradient>") {
            if let Some((id, geometry, mut stops)) = active_grad.take() {
                if stops.len() == 1
                    && let Some(first) = stops.first().cloned()
                    && let Ok(dup) = FountainStop::new(1.0, first.color)
                {
                    stops.push(dup);
                }
                if stops.len() >= 2 {
                    svg_gradients.insert(id, FillStyle::Fountain { geometry, blend_mode: FountainBlendMode::Direct, stops });
                }
            }
            continue;
        } else if (tag_slice.starts_with("<pattern ") || tag_slice.starts_with("<pattern\t"))
            && !tag_slice.ends_with("/>")
            && let Some(after_open) = svg_str.get(pos..)
            && let Some(close_rel) = after_open.find("</pattern>")
        {
            let pat_body = after_open.get(..close_rel).unwrap_or_default();
            pos = pos.saturating_add(close_rel).saturating_add(10);
            if let Some((id, pat_fill)) = parse_svg_pattern(tag_slice, pat_body) {
                svg_gradients.insert(id, pat_fill);
            }
            continue;
        } else if (tag_slice.starts_with("<mask ") || tag_slice.starts_with("<mask\t"))
            && !tag_slice.ends_with("/>")
            && let Some(after_open) = svg_str.get(pos..)
            && let Some(close_rel) = after_open.find("</mask>")
        {
            let mask_body = after_open.get(..close_rel).unwrap_or_default();
            pos = pos.saturating_add(close_rel).saturating_add(7);
            if let Some(mask_id) = extract_attr_str(tag_slice, "id")
                && let Some(tr) = parse_svg_mask_body(mask_body, &svg_gradients)
            {
                svg_masks.insert(mask_id.to_owned(), tr);
            }
            continue;
        } else if (tag_slice.starts_with("<clipPath ") || tag_slice.starts_with("<clipPath\t"))
            && !tag_slice.ends_with("/>")
            && let Some(after_open) = svg_str.get(pos..)
            && let Some(close_rel) = after_open.find("</clipPath>")
        {
            let clip_body = after_open.get(..close_rel).unwrap_or_default();
            pos = pos.saturating_add(close_rel).saturating_add(11);
            if let Some(clip_id) = extract_attr_str(tag_slice, "id")
                && let Some(clip_path) = parse_svg_clip_path_body(clip_body)
            {
                svg_clip_paths.insert(clip_id.to_owned(), clip_path);
            }
            continue;
        }

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
            if id_str.starts_with("layer-") {
                passthrough_g_stack.push(true);
                let layer_name = extract_attr_str(tag_slice, "data-name").unwrap_or(id_str).to_owned();
                let layer_opacity = parse_attr_f64(tag_slice, "opacity").and_then(|v| F64::new(v.clamp(0.0, 1.0)).ok()).unwrap_or(F64::ONE);
                if let Some(page) = doc.pages.first_mut() {
                    if !saw_layer_group
                        && let Some(first_layer) = page.layers.first_mut()
                        && first_layer.objects.is_empty()
                    {
                        first_layer.name = layer_name;
                        first_layer.opacity = layer_opacity;
                    } else {
                        let next_lid = LayerId(page.layers.len().saturating_add(1) as u64);
                        let mut new_layer = Layer::new_normal(next_lid, layer_name);
                        new_layer.opacity = layer_opacity;
                        page.layers.push(new_layer);
                    }
                    saw_layer_group = true;
                }
            } else if id_str == "prepress-marks" {
                passthrough_g_stack.push(true);
            } else {
                passthrough_g_stack.push(false);
                let is_obj_wrapper = extract_attr_str(tag_slice, "data-object-wrapper") == Some("1");
                let name = extract_attr_str(tag_slice, "data-name")
                    .map(unescape_svg_xml)
                    .or_else(|| if id_str.is_empty() || is_obj_wrapper { None } else { Some(id_str.to_owned()) });
                let locked = extract_attr_str(tag_slice, "data-locked") == Some("1");
                let opacity = parse_style_or_attr_f64(tag_slice, "opacity").and_then(|v| F64::new(v.clamp(0.0, 1.0)).ok()).unwrap_or(F64::ONE);
                let tf = parse_svg_transform(tag_slice);
                let clip_id = extract_url_ref_id(tag_slice, "clip-path");
                let tr = parse_svg_transparency(tag_slice, &svg_masks);
                group_stack.push((name, is_obj_wrapper, locked, opacity, tf, clip_id, tr, Vec::new()));
            }
            continue;
        } else if tag_slice.starts_with("</g>") {
            if passthrough_g_stack.pop() == Some(false)
                && let Some((name, is_obj_wrapper, locked, opacity, tf, clip_id_opt, tr, mut children)) = group_stack.pop()
            {
                if let Some(clip_id) = clip_id_opt {
                    if let Some(prev) = last_svg_sibling_mut(&mut doc, &mut group_stack)
                        && prev.powerclip_children.is_empty()
                    {
                        prev.powerclip_children = children;
                    } else if let Some(clip_path) = svg_clip_paths.get(&clip_id).cloned() {
                        let obj_id = doc.allocate_object_id();
                        let mut container = ObjectNode::new(obj_id, Some("SVG PowerClip".to_owned()), ObjectKind::Path(clip_path), None, None);
                        container.transform = tf;
                        container.transparency = tr;
                        container.powerclip_children = children;
                        push_svg_node(&mut doc, &mut group_stack, container);
                    }
                } else if is_obj_wrapper && !children.is_empty() {
                    let mut merged = children.remove(0);
                    for ch in children {
                        if merged.fill.is_none() && ch.fill.is_some() {
                            merged.fill = ch.fill;
                            merged.kind = ch.kind;
                        }
                        if merged.outline.is_none() && ch.outline.is_some() {
                            merged.outline = ch.outline;
                        }
                        if merged.powerclip_children.is_empty() && !ch.powerclip_children.is_empty() {
                            merged.powerclip_children = ch.powerclip_children;
                        }
                    }
                    if name.is_some() {
                        merged.name = name;
                    }
                    if locked {
                        merged.locked = true;
                    }
                    merged.opacity = opacity;
                    merged.transform = tf;
                    if tr.is_some() {
                        merged.transparency = tr;
                    }
                    push_svg_node(&mut doc, &mut group_stack, merged);
                } else {
                    let grp_name = name.or_else(|| Some("SVG Group".to_owned()));
                    let obj_id = doc.allocate_object_id();
                    let mut grp = ObjectNode::new(obj_id, grp_name, ObjectKind::Group { children }, None, None);
                    grp.locked = locked;
                    grp.opacity = opacity;
                    grp.transform = tf;
                    grp.transparency = tr;
                    push_svg_node(&mut doc, &mut group_stack, grp);
                }
            }
            continue;
        }

        if tag_slice.starts_with("<rect ") || tag_slice.starts_with("<rect\t") {
            let id_str = extract_attr_str(tag_slice, "id").unwrap_or_default();
            if id_str == "page-background" {
                let (fill, _) = parse_svg_style_attrs(tag_slice, &svg_gradients);
                if let Some(page) = doc.pages.first_mut() {
                    page.background = fill;
                }
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
                let (fill, outline) = parse_svg_style_attrs(tag_slice, &svg_gradients);
                let obj_id = doc.allocate_object_id();
                let mut node = ObjectNode::new(obj_id, Some("SVG Rect".to_owned()), ObjectKind::Rect(rspec), fill, outline);
                node.transform = parse_svg_transform(tag_slice);
                node.transparency = parse_svg_transparency(tag_slice, &svg_masks);
                push_svg_node(&mut doc, &mut group_stack, node);
            }
        } else if tag_slice.starts_with("<ellipse ") || tag_slice.starts_with("<circle ") {
            let class_str = extract_attr_str(tag_slice, "class").unwrap_or_default();
            if class_str == "registration-target" {
                continue;
            }
            let cx = parse_attr_f64(tag_slice, "cx").unwrap_or(0.0);
            let cy = parse_attr_f64(tag_slice, "cy").unwrap_or(0.0);
            let rx = parse_attr_f64(tag_slice, "rx").or_else(|| parse_attr_f64(tag_slice, "r")).unwrap_or(0.0);
            let ry = parse_attr_f64(tag_slice, "ry").or_else(|| parse_attr_f64(tag_slice, "r")).unwrap_or(0.0);
            if rx > 0.0
                && ry > 0.0
                && let Ok(r) = Rect::new(cx - rx, cy - ry, cx + rx, cy + ry)
            {
                let (fill, outline) = parse_svg_style_attrs(tag_slice, &svg_gradients);
                let obj_id = doc.allocate_object_id();
                let mut node = ObjectNode::new(obj_id, Some("SVG Ellipse".to_owned()), ObjectKind::Ellipse(EllipseSpec::full(r)), fill, outline);
                node.transform = parse_svg_transform(tag_slice);
                node.transparency = parse_svg_transparency(tag_slice, &svg_masks);
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
                    let (fill, outline) = parse_svg_style_attrs(tag_slice, &svg_gradients);
                    let obj_id = doc.allocate_object_id();
                    let mut node = ObjectNode::new(obj_id, Some("SVG Line".to_owned()), ObjectKind::Path(path), fill, outline);
                    node.transform = parse_svg_transform(tag_slice);
                    node.transparency = parse_svg_transparency(tag_slice, &svg_masks);
                    push_svg_node(&mut doc, &mut group_stack, node);
                }
            }
        } else if tag_slice.starts_with("<polyline ")
            || tag_slice.starts_with("<polyline\t")
            || tag_slice.starts_with("<polygon ")
            || tag_slice.starts_with("<polygon\t")
        {
            let class_str = extract_attr_str(tag_slice, "class").unwrap_or_default();
            if class_str == "arrowhead-marker" {
                continue;
            }
            let is_polygon = tag_slice.starts_with("<polygon");
            if let Some(pts_str) = extract_attr_str(tag_slice, "points")
                && let Some(mut path) = parse_svg_points_path(pts_str, is_polygon)
            {
                if extract_style_or_attr_str(tag_slice, "fill-rule").map(str::trim) == Some("evenodd") {
                    path.fill_rule = FillRule::EvenOdd;
                }
                let (fill, outline) = parse_svg_style_attrs(tag_slice, &svg_gradients);
                let obj_id = doc.allocate_object_id();
                let label = if is_polygon { "SVG Polygon" } else { "SVG Polyline" };
                let mut node = ObjectNode::new(obj_id, Some(label.to_owned()), ObjectKind::Path(path), fill, outline);
                node.transform = parse_svg_transform(tag_slice);
                node.transparency = parse_svg_transparency(tag_slice, &svg_masks);
                push_svg_node(&mut doc, &mut group_stack, node);
            }
        } else if tag_slice.starts_with("<path ") {
            let class_str = extract_attr_str(tag_slice, "class").unwrap_or_default();
            if class_str == "crop-marks" || class_str == "registration-crosshair" || class_str == "arrowhead-marker" {
                continue;
            }
            if let Some(d_str) = extract_attr_str(tag_slice, "d")
                && let Ok(mut path) = parse_svg_path_d(d_str)
                && !path.is_empty()
            {
                if extract_style_or_attr_str(tag_slice, "fill-rule").map(str::trim) == Some("evenodd") {
                    path.fill_rule = FillRule::EvenOdd;
                }
                let kind = parse_svg_shape_meta_kind(tag_slice).unwrap_or(ObjectKind::Path(path));
                let (fill, outline) = parse_svg_style_attrs(tag_slice, &svg_gradients);
                let name = extract_attr_str(tag_slice, "data-name").map(unescape_svg_xml).or_else(|| Some("SVG Path".to_owned()));
                let obj_id = doc.allocate_object_id();
                let mut node = ObjectNode::new(obj_id, name, kind, fill, outline);
                node.locked = extract_attr_str(tag_slice, "data-locked") == Some("1");
                if let Some(op) = parse_style_or_attr_f64(tag_slice, "opacity").and_then(|v| F64::new(v.clamp(0.0, 1.0)).ok()) {
                    node.opacity = op;
                }
                node.transform = parse_svg_transform(tag_slice);
                node.transparency = parse_svg_transparency(tag_slice, &svg_masks);
                push_svg_node(&mut doc, &mut group_stack, node);
            }
        } else if (tag_slice.starts_with("<image ") || tag_slice.starts_with("<image\t"))
            && let Some((node_kind, img_tf)) = parse_svg_image_tag(tag_slice)
        {
            let obj_id = doc.allocate_object_id();
            let mut node = ObjectNode::new(obj_id, Some("SVG Image".to_owned()), node_kind, None, None);
            node.transform = img_tf;
            node.transparency = parse_svg_transparency(tag_slice, &svg_masks);
            push_svg_node(&mut doc, &mut group_stack, node);
        } else if (tag_slice.starts_with("<text ") || tag_slice.starts_with("<text\t") || tag_slice == "<text>")
            && let Some(after_open) = svg_str.get(pos..)
            && let Some(close_rel) = after_open.find("</text>")
        {
            let raw_inner = after_open.get(..close_rel).unwrap_or_default();
            pos = pos.saturating_add(close_rel).saturating_add(7);
            let (text_content, x, y) = parse_svg_text_inner(tag_slice, raw_inner);
            let font_family = extract_attr_str(tag_slice, "font-family").map(unescape_svg_xml).unwrap_or_else(|| "Arial".to_owned());
            let size_val = parse_attr_f64(tag_slice, "font-size").unwrap_or(12.0).max(1.0);
            let node_name = extract_attr_str(tag_slice, "data-name").map(unescape_svg_xml).unwrap_or_else(|| "SVG Text".to_owned());
            let frame_w_opt = parse_attr_f64(tag_slice, "data-frame-width").filter(|&v| v > 0.0);
            let frame_h_opt = parse_attr_f64(tag_slice, "data-frame-height").filter(|&v| v > 0.0);
            let kind_opt = if let (Some(fw), Some(fh)) = (frame_w_opt, frame_h_opt) {
                let fx = parse_attr_f64(tag_slice, "data-frame-x").unwrap_or(x);
                let fy = parse_attr_f64(tag_slice, "data-frame-y").unwrap_or(y);
                let line_spacing = parse_attr_f64(tag_slice, "data-line-spacing").unwrap_or(1.2).clamp(0.1, 10.0);
                let align =
                    match extract_attr_str(tag_slice, "data-align").or_else(|| extract_style_or_attr_str(tag_slice, "text-anchor")).map(str::trim) {
                        Some("center" | "middle") => TextAlignment::Center,
                        Some("right" | "end") => TextAlignment::Right,
                        Some("justify") => TextAlignment::Justify,
                        _ => TextAlignment::Left,
                    };
                let columns = parse_attr_f64(tag_slice, "data-columns").map(|v| (v.round() as u32).clamp(1, 64)).unwrap_or(1);
                let gutter = parse_attr_f64(tag_slice, "data-gutter").unwrap_or(0.0).max(0.0);
                let para_text = extract_attr_str(tag_slice, "data-raw-text").map(unescape_svg_xml).unwrap_or(text_content);
                Point::new(fx, fy)
                    .and_then(|pt_pos| ParagraphTextSpec::new(pt_pos, fw, fh, para_text, font_family, size_val, line_spacing, align, columns, gutter))
                    .ok()
                    .map(|spec| ObjectKind::ParagraphText(Box::new(spec)))
            } else if let (Ok(position), Ok(size_pt)) = (Point::new(x, y), F64::new(size_val)) {
                Some(ObjectKind::ArtisticText { text: text_content, font_family, size_pt, position })
            } else {
                None
            };
            if let Some(kind) = kind_opt {
                let (fill, outline) = parse_svg_style_attrs(tag_slice, &svg_gradients);
                let obj_id = doc.allocate_object_id();
                let mut node = ObjectNode::new(obj_id, Some(node_name), kind, fill, outline);
                node.transform = parse_svg_transform(tag_slice);
                node.transparency = parse_svg_transparency(tag_slice, &svg_masks);
                push_svg_node(&mut doc, &mut group_stack, node);
            }
        }
    }

    while let Some((name, _, locked, opacity, tf, _, tr, children)) = group_stack.pop() {
        let grp_name = name.or_else(|| Some("SVG Group".to_owned()));
        let obj_id = doc.allocate_object_id();
        let mut grp = ObjectNode::new(obj_id, grp_name, ObjectKind::Group { children }, None, None);
        grp.locked = locked;
        grp.opacity = opacity;
        grp.transform = tf;
        grp.transparency = tr;
        push_svg_node(&mut doc, &mut group_stack, grp);
    }

    Ok(doc)
}

fn parse_svg_transform(tag_slice: &str) -> Affine2 {
    let Some(raw) = extract_attr_str(tag_slice, "transform") else {
        return Affine2::IDENTITY;
    };
    let mut acc = Affine2::IDENTITY;
    let mut rem = raw.trim();
    while let Some(close_idx) = rem.find(')') {
        let end = close_idx.saturating_add(1);
        let Some(part) = rem.get(..end) else {
            break;
        };
        let single = parse_single_svg_transform(part.trim_start_matches(|c: char| c.is_whitespace() || c == ','));
        acc = single.then(acc).unwrap_or(acc);
        rem = rem.get(end..).unwrap_or_default().trim();
    }
    acc
}

fn parse_single_svg_transform(s: &str) -> Affine2 {
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
    } else if let Some(rest) = s.strip_prefix("skewX(")
        && let Some(inner) = rest.strip_suffix(')')
        && let Ok(deg) = inner.trim().parse::<f64>()
    {
        return Affine2::skew_deg(deg, 0.0).unwrap_or(Affine2::IDENTITY);
    } else if let Some(rest) = s.strip_prefix("skewY(")
        && let Some(inner) = rest.strip_suffix(')')
        && let Ok(deg) = inner.trim().parse::<f64>()
    {
        return Affine2::skew_deg(0.0, deg).unwrap_or(Affine2::IDENTITY);
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
        if ch.is_ascii_alphabetic() && ch != 'e' && ch != 'E' {
            if !cur.is_empty() {
                tokens.push(std::mem::take(&mut cur));
            }
            tokens.push(ch.to_string());
        } else if ch.is_whitespace() || ch == ',' {
            if !cur.is_empty() {
                tokens.push(std::mem::take(&mut cur));
            }
        } else if (ch == '-' || ch == '+') && !cur.is_empty() && !cur.ends_with('e') && !cur.ends_with('E') {
            tokens.push(std::mem::take(&mut cur));
            cur.push(ch);
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
    let mut last_cubic_c2: Option<(f64, f64)> = None;
    let mut last_quad_ctrl: Option<(f64, f64)> = None;
    let mut active_cmd = "";
    while i < tokens.len() {
        let tok = tokens.get(i).map(String::as_str).unwrap_or_default();
        if matches!(tok, "M" | "m" | "L" | "l" | "H" | "h" | "V" | "v" | "Q" | "q" | "T" | "t" | "C" | "c" | "S" | "s" | "A" | "a" | "Z" | "z") {
            active_cmd = tok;
            i = i.saturating_add(1);
        } else if active_cmd.is_empty() {
            i = i.saturating_add(1);
            continue;
        }
        let cmd = active_cmd;
        let prev_i = i;
        if !matches!(cmd, "C" | "c" | "S" | "s") {
            last_cubic_c2 = None;
        }
        if !matches!(cmd, "Q" | "q" | "T" | "t") {
            last_quad_ctrl = None;
        }
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
                active_cmd = if cmd == "m" { "l" } else { "L" };
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
                    last_quad_ctrl = Some((acx, acy));
                    cur_x = aex;
                    cur_y = aey;
                }
                i = i.saturating_add(4);
            }
            "T" | "t" if i.saturating_add(1) < tokens.len() => {
                if let (Some(ex), Some(ey)) = (token_f64(&tokens, i), token_f64(&tokens, i.saturating_add(1))) {
                    let (acx, acy) = match last_quad_ctrl {
                        Some((px, py)) => (2.0 * cur_x - px, 2.0 * cur_y - py),
                        None => (cur_x, cur_y),
                    };
                    let (aex, aey) = if cmd == "t" { (cur_x + ex, cur_y + ey) } else { (ex, ey) };
                    let ctrl = Point::new(acx, acy)?;
                    let pt = Point::new(aex, aey)?;
                    path.quad_to(ctrl, pt)?;
                    last_quad_ctrl = Some((acx, acy));
                    cur_x = aex;
                    cur_y = aey;
                }
                i = i.saturating_add(2);
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
                    last_cubic_c2 = Some((ac2x, ac2y));
                    cur_x = aex;
                    cur_y = aey;
                }
                i = i.saturating_add(6);
            }
            "S" | "s" if i.saturating_add(3) < tokens.len() => {
                if let (Some(c2x), Some(c2y), Some(ex), Some(ey)) = (
                    token_f64(&tokens, i),
                    token_f64(&tokens, i.saturating_add(1)),
                    token_f64(&tokens, i.saturating_add(2)),
                    token_f64(&tokens, i.saturating_add(3)),
                ) {
                    let (ac1x, ac1y) = match last_cubic_c2 {
                        Some((px, py)) => (2.0 * cur_x - px, 2.0 * cur_y - py),
                        None => (cur_x, cur_y),
                    };
                    let (ac2x, ac2y, aex, aey) = if cmd == "s" { (cur_x + c2x, cur_y + c2y, cur_x + ex, cur_y + ey) } else { (c2x, c2y, ex, ey) };
                    let c1 = Point::new(ac1x, ac1y)?;
                    let c2 = Point::new(ac2x, ac2y)?;
                    let pt = Point::new(aex, aey)?;
                    path.cubic_to(c1, c2, pt)?;
                    last_cubic_c2 = Some((ac2x, ac2y));
                    cur_x = aex;
                    cur_y = aey;
                }
                i = i.saturating_add(4);
            }
            "A" | "a" if i.saturating_add(6) < tokens.len() => {
                if let (Some(rx), Some(ry), Some(x_rot), Some(large_f), Some(sweep_f), Some(ex), Some(ey)) = (
                    token_f64(&tokens, i),
                    token_f64(&tokens, i.saturating_add(1)),
                    token_f64(&tokens, i.saturating_add(2)),
                    token_f64(&tokens, i.saturating_add(3)),
                    token_f64(&tokens, i.saturating_add(4)),
                    token_f64(&tokens, i.saturating_add(5)),
                    token_f64(&tokens, i.saturating_add(6)),
                ) {
                    let (aex, aey) = if cmd == "a" { (cur_x + ex, cur_y + ey) } else { (ex, ey) };
                    emit_svg_elliptical_arc(&mut path, cur_x, cur_y, rx, ry, x_rot, large_f.abs() > 0.5, sweep_f.abs() > 0.5, aex, aey)?;
                    cur_x = aex;
                    cur_y = aey;
                }
                i = i.saturating_add(7);
            }
            "Z" | "z" => {
                path.close()?;
                cur_x = sub_x;
                cur_y = sub_y;
                active_cmd = "";
            }
            _ => {
                i = i.saturating_add(1);
            }
        }
        if i == prev_i && !active_cmd.is_empty() {
            i = i.saturating_add(1);
        }
    }
    Ok(path)
}

#[allow(clippy::too_many_arguments)]
fn emit_svg_elliptical_arc(
    path: &mut Path,
    x1: f64,
    y1: f64,
    rx_in: f64,
    ry_in: f64,
    x_axis_rot_deg: f64,
    large_arc: bool,
    sweep: bool,
    x2: f64,
    y2: f64,
) -> Result<(), coralcraft_geom::GeomError> {
    if (x1 - x2).hypot(y1 - y2) < 1e-9 {
        return Ok(());
    }
    if rx_in.abs() < 1e-9 || ry_in.abs() < 1e-9 {
        return path.line_to(Point::new(x2, y2)?);
    }
    let mut rx = rx_in.abs();
    let mut ry = ry_in.abs();
    let (sin_phi, cos_phi) = x_axis_rot_deg.to_radians().sin_cos();
    let dx2 = 0.5 * (x1 - x2);
    let dy2 = 0.5 * (y1 - y2);
    let x1p = cos_phi * dx2 + sin_phi * dy2;
    let y1p = -sin_phi * dx2 + cos_phi * dy2;
    let lambda = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
    if lambda > 1.0 {
        let s = lambda.sqrt();
        rx *= s;
        ry *= s;
    }
    let rx2 = rx * rx;
    let ry2 = ry * ry;
    let x1p2 = x1p * x1p;
    let y1p2 = y1p * y1p;
    let num = (rx2 * ry2 - rx2 * y1p2 - ry2 * x1p2).max(0.0);
    let den = (rx2 * y1p2 + ry2 * x1p2).max(1e-12);
    let coef = (if large_arc == sweep { -1.0 } else { 1.0 }) * (num / den).sqrt();
    let cxp = coef * (rx * y1p / ry);
    let cyp = coef * (-ry * x1p / rx);
    let cx = cos_phi * cxp - sin_phi * cyp + 0.5 * (x1 + x2);
    let cy = sin_phi * cxp + cos_phi * cyp + 0.5 * (y1 + y2);
    let ux = (x1p - cxp) / rx;
    let uy = (y1p - cyp) / ry;
    let vx = (-x1p - cxp) / rx;
    let vy = (-y1p - cyp) / ry;
    let theta1 = uy.atan2(ux);
    let mut dtheta = (ux * vy - uy * vx).atan2(ux * vx + uy * vy);
    if !sweep && dtheta > 0.0 {
        dtheta -= std::f64::consts::TAU;
    } else if sweep && dtheta < 0.0 {
        dtheta += std::f64::consts::TAU;
    }
    let n_segs = ((dtheta.abs() / (std::f64::consts::FRAC_PI_2 + 1e-6)).ceil() as usize).clamp(1, 4);
    let step = dtheta / (n_segs as f64);
    let alpha = (4.0 / 3.0) * (0.25 * step).tan();
    for seg in 0..n_segs {
        let t0 = theta1 + (seg as f64) * step;
        let t1 = theta1 + (seg.saturating_add(1) as f64) * step;
        let (s0, c0) = t0.sin_cos();
        let (s1, c1) = t1.sin_cos();
        let p0x = cx + rx * cos_phi * c0 - ry * sin_phi * s0;
        let p0y = cy + rx * sin_phi * c0 + ry * cos_phi * s0;
        let dp0x = -rx * cos_phi * s0 - ry * sin_phi * c0;
        let dp0y = -rx * sin_phi * s0 + ry * cos_phi * c0;
        let p1x = cx + rx * cos_phi * c1 - ry * sin_phi * s1;
        let p1y = cy + rx * sin_phi * c1 + ry * cos_phi * s1;
        let dp1x = -rx * cos_phi * s1 - ry * sin_phi * c1;
        let dp1y = -rx * sin_phi * s1 + ry * cos_phi * c1;
        let ctrl1 = Point::new(p0x + alpha * dp0x, p0y + alpha * dp0y)?;
        let ctrl2 = Point::new(p1x - alpha * dp1x, p1y - alpha * dp1y)?;
        let end = Point::new(p1x, p1y)?;
        path.cubic_to(ctrl1, ctrl2, end)?;
    }
    Ok(())
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

fn parse_svg_pattern(open_tag: &str, pat_body: &str) -> Option<(String, FillStyle)> {
    let id = extract_attr_str(open_tag, "id")?.to_owned();
    let tile_w = parse_attr_f64(open_tag, "width").or_else(|| parse_attr_f64(open_tag, "height")).unwrap_or(16.0).max(1.0);
    let tile_size_pt = F64::new(tile_w).ok()?;
    let mut rects: Vec<(f64, f64, Color)> = Vec::new();
    let mut circle_color: Option<Color> = None;
    let mut path_stroke_color: Option<Color> = None;

    let mut p = 0usize;
    while let Some(rem) = pat_body.get(p..) {
        let Some(lt) = rem.find('<') else { break };
        let s = p.saturating_add(lt);
        let Some(from_s) = pat_body.get(s..) else { break };
        let Some(gt) = from_s.find('>') else { break };
        let e = s.saturating_add(gt).saturating_add(1);
        let Some(sub_tag) = pat_body.get(s..e) else { break };
        p = e;

        if sub_tag.starts_with("<rect ") || sub_tag.starts_with("<rect\t") {
            let w = parse_attr_f64(sub_tag, "width").unwrap_or(tile_w);
            let h = parse_attr_f64(sub_tag, "height").unwrap_or(tile_w);
            if let Some(c) = extract_style_or_attr_str(sub_tag, "fill").and_then(|v| parse_svg_color(v.trim())) {
                rects.push((w, h, c));
            }
        } else if sub_tag.starts_with("<circle ") || sub_tag.starts_with("<ellipse ") {
            if let Some(c) = extract_style_or_attr_str(sub_tag, "fill").and_then(|v| parse_svg_color(v.trim())) {
                circle_color = Some(c);
            }
        } else if (sub_tag.starts_with("<path ") || sub_tag.starts_with("<line "))
            && let Some(c) = extract_style_or_attr_str(sub_tag, "stroke")
                .or_else(|| extract_style_or_attr_str(sub_tag, "fill"))
                .and_then(|v| parse_svg_color(v.trim()))
        {
            path_stroke_color = Some(c);
        }
    }

    let bg_color = rects.first().map(|r| r.2.clone()).unwrap_or(Color::WHITE);
    let (kind, fg_color) = if let Some(fg) = circle_color {
        (PatternKind::PolkaDot, fg)
    } else if let Some(fg) = path_stroke_color {
        (PatternKind::DiagonalCrosshatch, fg)
    } else if rects.len() >= 3 {
        let fg = rects.get(1).map(|r| r.2.clone()).unwrap_or(Color::BLACK);
        (PatternKind::Checkerboard, fg)
    } else if let [(bw, bh, _), (fw, fh, fg)] = rects.as_slice() {
        let k = if (*fw - *bw).abs() < 1e-3 && *fh < *bh {
            PatternKind::HorizontalStripe
        } else if (*fh - *bh).abs() < 1e-3 && *fw < *bw {
            PatternKind::VerticalStripe
        } else {
            PatternKind::Checkerboard
        };
        (k, fg.clone())
    } else {
        return None;
    };

    Some((id, FillStyle::TwoColorPattern { kind, fg_color, bg_color, tile_size_pt }))
}

fn unescape_svg_xml(s: &str) -> String {
    s.replace("&#10;", "\n")
        .replace("&#13;", "\r")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn parse_svg_text_inner(open_tag: &str, raw_inner: &str) -> (String, f64, f64) {
    let mut x_opt = parse_attr_f64(open_tag, "x");
    let mut y_opt = parse_attr_f64(open_tag, "y");
    if raw_inner.contains("<tspan") {
        let mut lines = Vec::new();
        let mut p = 0usize;
        while let Some(rem) = raw_inner.get(p..) {
            let Some(ts_rel) = rem.find("<tspan") else { break };
            let ts_start = p.saturating_add(ts_rel);
            let Some(from_ts) = raw_inner.get(ts_start..) else { break };
            let Some(gt_rel) = from_ts.find('>') else { break };
            let open_end = ts_start.saturating_add(gt_rel).saturating_add(1);
            let Some(tspan_open) = raw_inner.get(ts_start..open_end) else { break };
            if x_opt.is_none() {
                x_opt = parse_attr_f64(tspan_open, "x");
            }
            if y_opt.is_none() {
                y_opt = parse_attr_f64(tspan_open, "y");
            }
            if tspan_open.ends_with("/>") {
                p = open_end;
                continue;
            }
            let Some(after_tspan) = raw_inner.get(open_end..) else { break };
            let Some(close_rel) = after_tspan.find("</tspan>") else { break };
            let inner_txt = after_tspan.get(..close_rel).unwrap_or_default();
            lines.push(unescape_svg_xml(inner_txt));
            p = open_end.saturating_add(close_rel).saturating_add(8);
        }
        return (lines.join("\n"), x_opt.unwrap_or(0.0), y_opt.unwrap_or(0.0));
    }
    (unescape_svg_xml(raw_inner.trim()), x_opt.unwrap_or(0.0), y_opt.unwrap_or(0.0))
}

fn parse_svg_coord_or_pct(tag_slice: &str, attr_name: &str) -> Option<f64> {
    let raw = extract_attr_str(tag_slice, attr_name)?.trim();
    if let Some(pct) = raw.strip_suffix('%') {
        pct.trim().parse::<f64>().ok().map(|v| v / 100.0)
    } else {
        raw.trim_end_matches("pt").trim_end_matches("px").parse::<f64>().ok()
    }
}

fn parse_svg_style_attrs(tag_slice: &str, svg_gradients: &BTreeMap<String, FillStyle>) -> (Option<FillStyle>, Option<OutlineStyle>) {
    let fill = match extract_style_or_attr_str(tag_slice, "fill").map(str::trim) {
        Some("none") | Some("transparent") | None => None,
        Some(raw) => {
            if let Some(rest) = raw.strip_prefix("url(")
                && let Some(inner) = rest.strip_suffix(')')
            {
                let grad_id = inner.trim().trim_matches(|c| c == '\'' || c == '"').strip_prefix('#').unwrap_or_default();
                svg_gradients.get(grad_id).cloned()
            } else {
                parse_svg_color(raw).map(FillStyle::uniform)
            }
        }
    };
    let outline = match extract_style_or_attr_str(tag_slice, "stroke") {
        Some("none") | Some("transparent") | None => None,
        Some(raw) => parse_svg_color(raw.trim()).map(OutlineStyle::hairline),
    };
    let has_explicit_stroke_none = matches!(extract_style_or_attr_str(tag_slice, "stroke").map(str::trim), Some("none" | "transparent"));
    let mut outline =
        if fill.is_none() && outline.is_none() && !has_explicit_stroke_none { Some(OutlineStyle::hairline(Color::BLACK)) } else { outline };
    if let Some(st) = &mut outline {
        if let Some(w) = parse_attr_f64(tag_slice, "data-stroke-width-pt").or_else(|| parse_style_or_attr_f64(tag_slice, "stroke-width"))
            && w > 0.0
            && let Ok(w_f64) = F64::new(w)
        {
            st.width_pt = w_f64;
        }
        if extract_attr_str(tag_slice, "data-behind-fill") == Some("1")
            || extract_style_or_attr_str(tag_slice, "paint-order").is_some_and(|v| v.contains("stroke") && v.contains("fill"))
        {
            st.behind_fill = true;
        }
        if extract_attr_str(tag_slice, "data-scale-with-object") == Some("0")
            || extract_style_or_attr_str(tag_slice, "vector-effect").map(str::trim) == Some("non-scaling-stroke")
        {
            st.scale_with_object = false;
        }
        if let Some(sa) = extract_attr_str(tag_slice, "data-start-arrow").and_then(parse_svg_arrow_kind) {
            st.start_arrow = sa;
        }
        if let Some(ea) = extract_attr_str(tag_slice, "data-end-arrow").and_then(parse_svg_arrow_kind) {
            st.end_arrow = ea;
        }
        if let Some(ml) = parse_style_or_attr_f64(tag_slice, "stroke-miterlimit")
            && ml >= 1.0
            && let Ok(ml_f64) = F64::new(ml)
        {
            st.miter_limit = ml_f64;
        }
        if let Some(cap_str) = extract_style_or_attr_str(tag_slice, "stroke-linecap") {
            st.cap = match cap_str.trim() {
                "round" => LineCap::Round,
                "square" => LineCap::Square,
                _ => LineCap::Butt,
            };
        }
        if let Some(join_str) = extract_style_or_attr_str(tag_slice, "stroke-linejoin") {
            st.join = match join_str.trim() {
                "round" => LineJoin::Round,
                "bevel" => LineJoin::Bevel,
                _ => LineJoin::Miter,
            };
        }
        if let Some(dash_str) = extract_style_or_attr_str(tag_slice, "stroke-dasharray") {
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

fn parse_svg_arrow_kind(s: &str) -> Option<ArrowheadKind> {
    match s.trim() {
        "none" => Some(ArrowheadKind::None),
        "triangle" => Some(ArrowheadKind::Triangle),
        "open-arrow" => Some(ArrowheadKind::OpenArrow),
        "circle" => Some(ArrowheadKind::Circle),
        "diamond" => Some(ArrowheadKind::Diamond),
        "unknown" => Some(ArrowheadKind::Unknown),
        _ => None,
    }
}

fn parse_svg_shape_meta_kind(tag_slice: &str) -> Option<ObjectKind> {
    if let Some(el_str) = extract_attr_str(tag_slice, "data-ellipse") {
        let nums: Vec<f64> = el_str.split(',').filter_map(|t| t.trim().parse::<f64>().ok()).collect();
        if let [x0, y0, x1, y1, s_deg, e_deg, m_code] = nums.as_slice()
            && let Ok(rect) = Rect::new(*x0, *y0, *x1, *y1)
        {
            let mode = match *m_code as u8 {
                1 => coralcraft_geom::EllipseMode::Pie,
                2 => coralcraft_geom::EllipseMode::Arc,
                _ => coralcraft_geom::EllipseMode::Full,
            };
            return Some(ObjectKind::Ellipse(EllipseSpec {
                rect,
                start_deg: F64::new(*s_deg).unwrap_or(F64::ZERO),
                end_deg: F64::new(*e_deg).unwrap_or(F64::ZERO),
                mode,
            }));
        }
    }
    if let Some(rc_str) = extract_attr_str(tag_slice, "data-rect") {
        let nums: Vec<f64> = rc_str.split(',').filter_map(|t| t.trim().parse::<f64>().ok()).collect();
        if let [x0, y0, x1, y1, r0, r1, r2, r3, k0, k1, k2, k3] = nums.as_slice()
            && let Ok(rect) = Rect::new(*x0, *y0, *x1, *y1)
        {
            let map_ck = |v: f64| match v as u8 {
                1 => CornerKind::Scalloped,
                2 => CornerKind::Chamfered,
                _ => CornerKind::Round,
            };
            return Some(ObjectKind::Rect(RectSpec {
                rect,
                radii: [
                    F64::new(*r0).unwrap_or(F64::ZERO),
                    F64::new(*r1).unwrap_or(F64::ZERO),
                    F64::new(*r2).unwrap_or(F64::ZERO),
                    F64::new(*r3).unwrap_or(F64::ZERO),
                ],
                corner_kinds: [map_ck(*k0), map_ck(*k1), map_ck(*k2), map_ck(*k3)],
            }));
        }
    }
    None
}

fn extract_url_ref_id(tag_slice: &str, prop: &str) -> Option<String> {
    let raw = extract_style_or_attr_str(tag_slice, prop)?.trim();
    let inner = raw.strip_prefix("url(")?.strip_suffix(')')?.trim();
    let id = inner.trim_matches(|c| c == '\'' || c == '"').strip_prefix('#')?;
    if id.is_empty() { None } else { Some(id.to_owned()) }
}

fn parse_svg_mask_body(mask_body: &str, svg_gradients: &BTreeMap<String, FillStyle>) -> Option<TransparencyStyle> {
    let mut p = 0usize;
    while let Some(rem) = mask_body.get(p..) {
        let lt = rem.find('<')?;
        let s = p.saturating_add(lt);
        let gt = mask_body.get(s..)?.find('>')?;
        let e = s.saturating_add(gt).saturating_add(1);
        let sub_tag = mask_body.get(s..e)?;
        p = e;
        if let Some(grad_id) = extract_url_ref_id(sub_tag, "fill")
            && let Some(FillStyle::Fountain { geometry, blend_mode, stops }) = svg_gradients.get(&grad_id)
        {
            return Some(TransparencyStyle::Fountain { geometry: *geometry, blend_mode: *blend_mode, stops: stops.clone() });
        }
    }
    None
}

fn parse_svg_clip_path_body(clip_body: &str) -> Option<Path> {
    let mut p = 0usize;
    while let Some(rem) = clip_body.get(p..) {
        let lt = rem.find('<')?;
        let s = p.saturating_add(lt);
        let gt = clip_body.get(s..)?.find('>')?;
        let e = s.saturating_add(gt).saturating_add(1);
        let sub_tag = clip_body.get(s..e)?;
        p = e;
        if sub_tag.starts_with("<path ")
            && let Some(d_str) = extract_attr_str(sub_tag, "d")
            && let Ok(mut path) = parse_svg_path_d(d_str)
            && !path.is_empty()
        {
            if extract_style_or_attr_str(sub_tag, "clip-rule").or_else(|| extract_style_or_attr_str(sub_tag, "fill-rule")).map(str::trim)
                == Some("evenodd")
            {
                path.fill_rule = FillRule::EvenOdd;
            }
            return Some(path);
        } else if sub_tag.starts_with("<rect ") {
            let x = parse_attr_f64(sub_tag, "x").unwrap_or(0.0);
            let y = parse_attr_f64(sub_tag, "y").unwrap_or(0.0);
            let w = parse_attr_f64(sub_tag, "width").unwrap_or(0.0);
            let h = parse_attr_f64(sub_tag, "height").unwrap_or(0.0);
            if w > 0.0
                && h > 0.0
                && let Ok(r) = Rect::new(x, y, x + w, y + h)
                && let Ok(path) = RectSpec::sharp(r).to_path()
            {
                return Some(path);
            }
        }
    }
    None
}

fn decode_svg_base64(input: &str) -> Option<Vec<u8>> {
    let clean: Vec<u8> = input.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if clean.is_empty() || !clean.len().is_multiple_of(4) {
        return None;
    }
    let decode_val = |b: u8| -> Option<u8> {
        match b {
            b'A'..=b'Z' => Some(b.saturating_sub(b'A')),
            b'a'..=b'z' => Some(b.saturating_sub(b'a').saturating_add(26)),
            b'0'..=b'9' => Some(b.saturating_sub(b'0').saturating_add(52)),
            b'+' | b'-' => Some(62),
            b'/' | b'_' => Some(63),
            _ => None,
        }
    };
    let mut out = Vec::with_capacity((clean.len() / 4).saturating_mul(3));
    for &[c0, c1, c2, c3] in clean.as_chunks::<4>().0 {
        let v0 = decode_val(c0)?;
        let v1 = decode_val(c1)?;
        let pad2 = c2 == b'=';
        let pad3 = c3 == b'=';
        let v2 = if pad2 { 0 } else { decode_val(c2)? };
        let v3 = if pad3 { 0 } else { decode_val(c3)? };
        let triple = (u32::from(v0) << 18) | (u32::from(v1) << 12) | (u32::from(v2) << 6) | u32::from(v3);
        out.push(((triple >> 16) & 0xFF) as u8);
        if !pad2 {
            out.push(((triple >> 8) & 0xFF) as u8);
        }
        if !pad2 && !pad3 {
            out.push((triple & 0xFF) as u8);
        }
    }
    Some(out)
}

fn decode_stored_png_rgba_bytes(png: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    const PNG_SIG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    if png.get(0..8) != Some(&PNG_SIG) {
        return None;
    }
    let mut pos = 8usize;
    let mut width = 0u32;
    let mut height = 0u32;
    let mut idat = Vec::new();
    while pos.saturating_add(12) <= png.len() {
        let len = u32::from_be_bytes(<[u8; 4]>::try_from(png.get(pos..pos.saturating_add(4))?).ok()?) as usize;
        let ctype = <[u8; 4]>::try_from(png.get(pos.saturating_add(4)..pos.saturating_add(8))?).ok()?;
        let data_start = pos.saturating_add(8);
        let data_end = data_start.checked_add(len)?;
        let data = png.get(data_start..data_end)?;
        pos = data_end.checked_add(4)?;
        match &ctype {
            b"IHDR" if data.len() == 13 => {
                width = u32::from_be_bytes(<[u8; 4]>::try_from(data.get(0..4)?).ok()?);
                height = u32::from_be_bytes(<[u8; 4]>::try_from(data.get(4..8)?).ok()?);
                if data.get(8..13) != Some(&[8, 6, 0, 0, 0]) || width == 0 || height == 0 {
                    return None;
                }
            }
            b"IDAT" => idat.extend_from_slice(data),
            b"IEND" => break,
            _ => {}
        }
    }
    if width == 0 || height == 0 || idat.len() < 7 {
        return None;
    }
    let mut zpos = 2usize;
    let payload_end = idat.len().saturating_sub(4);
    let mut raw = Vec::new();
    while zpos < payload_end {
        let hdr = *idat.get(zpos)?;
        if (hdr & 0b0000_0110) != 0 {
            return None;
        }
        let blen = u16::from_le_bytes(<[u8; 2]>::try_from(idat.get(zpos.saturating_add(1)..zpos.saturating_add(3))?).ok()?);
        let nlen = u16::from_le_bytes(<[u8; 2]>::try_from(idat.get(zpos.saturating_add(3)..zpos.saturating_add(5))?).ok()?);
        if blen != !nlen {
            return None;
        }
        let bstart = zpos.saturating_add(5);
        let bend = bstart.checked_add(usize::from(blen))?;
        raw.extend_from_slice(idat.get(bstart..bend)?);
        zpos = bend;
        if (hdr & 1) != 0 {
            break;
        }
    }
    let row_bytes = (width as usize).checked_mul(4)?;
    let row_stride = row_bytes.checked_add(1)?;
    if raw.len() != row_stride.checked_mul(height as usize)? {
        return None;
    }
    let mut rgba = Vec::with_capacity(row_bytes.saturating_mul(height as usize));
    for y in 0..(height as usize) {
        let rstart = y.saturating_mul(row_stride);
        if *raw.get(rstart)? != 0 {
            return None;
        }
        rgba.extend_from_slice(raw.get(rstart.saturating_add(1)..rstart.saturating_add(row_stride))?);
    }
    Some((width, height, rgba))
}

fn parse_svg_image_tag(tag_slice: &str) -> Option<(ObjectKind, Affine2)> {
    let w = parse_attr_f64(tag_slice, "width").unwrap_or(0.0).max(1.0);
    let h = parse_attr_f64(tag_slice, "height").unwrap_or(0.0).max(1.0);
    let href = extract_attr_str(tag_slice, "href").or_else(|| extract_attr_str(tag_slice, "xlink:href"))?;
    let b64_str = href.trim().strip_prefix("data:image/png;base64,")?;
    let png_bytes = decode_svg_base64(b64_str)?;
    let (pixel_width, pixel_height, rgba_pixels) = decode_stored_png_rgba_bytes(&png_bytes)?;
    let dpi_x = F64::new(((f64::from(pixel_width) * 72.0) / w).max(1.0)).ok()?;
    let dpi_y = F64::new(((f64::from(pixel_height) * 72.0) / h).max(1.0)).ok()?;
    let x = parse_attr_f64(tag_slice, "x").unwrap_or(0.0);
    let y = parse_attr_f64(tag_slice, "y").unwrap_or(0.0);
    let base_tf = parse_svg_transform(tag_slice);
    let tf = if x.abs() > 1e-9 || y.abs() > 1e-9 {
        let tr = Affine2::translation(x, y).unwrap_or(Affine2::IDENTITY);
        tr.then(base_tf).unwrap_or(base_tf)
    } else {
        base_tf
    };
    Some((ObjectKind::Bitmap { pixel_width, pixel_height, dpi_x, dpi_y, rgba_pixels }, tf))
}

fn parse_svg_transparency(tag_slice: &str, svg_masks: &BTreeMap<String, TransparencyStyle>) -> Option<TransparencyStyle> {
    if let Some(mask_id) = extract_url_ref_id(tag_slice, "mask")
        && let Some(tr) = svg_masks.get(&mask_id)
    {
        return Some(tr.clone());
    }
    let parse_op = |s: &str| -> Option<f64> {
        let t = s.trim();
        if let Some(pct) = t.strip_suffix('%') {
            Some((pct.trim().parse::<f64>().ok()? / 100.0).clamp(0.0, 1.0))
        } else {
            Some(t.parse::<f64>().ok()?.clamp(0.0, 1.0))
        }
    };
    let op1 = extract_style_or_attr_str(tag_slice, "opacity").and_then(parse_op);
    let op2 = extract_style_or_attr_str(tag_slice, "fill-opacity").and_then(parse_op);
    let combined = match (op1, op2) {
        (Some(a), Some(b)) => Some(a * b),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }?;
    if combined < 1.0 - 1e-6 { Some(TransparencyStyle::Uniform { opacity: F64::new(combined).ok()? }) } else { None }
}

fn extract_style_or_attr_str<'a>(tag: &'a str, prop: &str) -> Option<&'a str> {
    if let Some(v) = extract_attr_str(tag, prop) {
        return Some(v);
    }
    let style = extract_attr_str(tag, "style")?;
    for decl in style.split(';') {
        if let Some((k, v)) = decl.split_once(':')
            && k.trim() == prop
        {
            return Some(v.trim());
        }
    }
    None
}

fn parse_style_or_attr_f64(tag: &str, prop: &str) -> Option<f64> {
    let raw = extract_style_or_attr_str(tag, prop)?;
    let trimmed = raw.trim_end_matches("pt").trim_end_matches("px").trim();
    trimmed.parse::<f64>().ok()
}

fn parse_svg_color(s: &str) -> Option<Color> {
    if let Some(hex) = s.strip_prefix('#') {
        let digit = |c: &u8| char::from(*c).to_digit(16).and_then(|d| u8::try_from(d).ok());
        // Byte-wise parsing: `len()` counts bytes, so slicing a non-ASCII string could split a char.
        if let [r1, r2, g1, g2, b1, b2] = hex.as_bytes() {
            let byte = |hi: &u8, lo: &u8| Some((digit(hi)? << 4) | digit(lo)?);
            return Some(Color::rgb8(byte(r1, r2)?, byte(g1, g2)?, byte(b1, b2)?));
        } else if let [r, g, b] = hex.as_bytes() {
            let dbl = |n: &u8| {
                let d = digit(n)?;
                Some((d << 4) | d)
            };
            return Some(Color::rgb8(dbl(r)?, dbl(g)?, dbl(b)?));
        }
        return None;
    }
    let lower = s.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("rgb(")
        && let Some(inner) = rest.strip_suffix(')')
    {
        let parse_ch = |tok: &str| -> Option<u8> {
            let t = tok.trim();
            if let Some(pct) = t.strip_suffix('%') {
                let p = pct.trim().parse::<f64>().ok()?.clamp(0.0, 100.0);
                Some((p * 255.0 / 100.0).round() as u8)
            } else {
                let v = t.parse::<f64>().ok()?.round().clamp(0.0, 255.0);
                Some(v as u8)
            }
        };
        let chs: Vec<u8> = inner.split(|c: char| c.is_whitespace() || c == ',').filter(|t| !t.is_empty()).filter_map(parse_ch).collect();
        if let [r, g, b, ..] = chs.as_slice() {
            return Some(Color::rgb8(*r, *g, *b));
        }
        return None;
    }
    match lower.as_str() {
        "black" => Some(Color::rgb8(0, 0, 0)),
        "white" => Some(Color::rgb8(255, 255, 255)),
        "red" => Some(Color::rgb8(255, 0, 0)),
        "lime" => Some(Color::rgb8(0, 255, 0)),
        "green" => Some(Color::rgb8(0, 128, 0)),
        "blue" => Some(Color::rgb8(0, 0, 255)),
        "yellow" => Some(Color::rgb8(255, 255, 0)),
        "cyan" | "aqua" => Some(Color::rgb8(0, 255, 255)),
        "magenta" | "fuchsia" => Some(Color::rgb8(255, 0, 255)),
        "silver" => Some(Color::rgb8(192, 192, 192)),
        "gray" | "grey" => Some(Color::rgb8(128, 128, 128)),
        "maroon" => Some(Color::rgb8(128, 0, 0)),
        "olive" => Some(Color::rgb8(128, 128, 0)),
        "purple" => Some(Color::rgb8(128, 0, 128)),
        "teal" => Some(Color::rgb8(0, 128, 128)),
        "navy" => Some(Color::rgb8(0, 0, 128)),
        "orange" => Some(Color::rgb8(255, 165, 0)),
        _ => None,
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
    let mut search_from = 0usize;
    while let Some(rel) = tag.get(search_from..)?.find(&needle) {
        let idx = search_from.saturating_add(rel);
        let prev_ok = idx > 0 && tag.as_bytes().get(idx.saturating_sub(1)).is_some_and(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r'));
        let val_start = idx.saturating_add(needle.len());
        if prev_ok {
            let from_start = tag.get(val_start..)?;
            let end = from_start.find('"')?;
            return from_start.get(..end);
        }
        search_from = val_start;
    }
    None
}

fn parse_attr_f64(tag: &str, attr: &str) -> Option<f64> {
    let raw = extract_attr_str(tag, attr)?;
    let trimmed = raw.trim_end_matches("pt").trim_end_matches("px").trim();
    trimmed.parse::<f64>().ok()
}

fn push_dxf_node(
    doc: &mut Document,
    saw_custom_layer: &mut bool,
    active_block: &mut Option<(String, f64, f64, Vec<ObjectNode>)>,
    layer_name: Option<&str>,
    node: ObjectNode,
) {
    if let Some((_, _, _, blk_objs)) = active_block.as_mut() {
        blk_objs.push(node);
        return;
    }
    let Some(page) = doc.pages.first_mut() else {
        return;
    };
    let clean_name = layer_name.map(str::trim).filter(|s| !s.is_empty() && *s != "0");
    if let Some(lname) = clean_name {
        if let Some(existing) = page.layers.iter_mut().find(|l| l.name == lname) {
            existing.objects.push(node);
            *saw_custom_layer = true;
            return;
        }
        if !*saw_custom_layer
            && let Some(first_layer) = page.layers.first_mut()
            && first_layer.objects.is_empty()
        {
            first_layer.name = lname.to_owned();
            first_layer.objects.push(node);
            *saw_custom_layer = true;
            return;
        }
        let next_lid = LayerId(page.layers.len().saturating_add(1) as u64);
        let mut new_layer = Layer::new_normal(next_lid, lname.to_owned());
        new_layer.objects.push(node);
        page.layers.push(new_layer);
        *saw_custom_layer = true;
    } else if let Some(first_layer) = page.layers.first_mut() {
        first_layer.objects.push(node);
    }
}

fn dxf_make_outline(
    aci: Option<i32>,
    truecolor: Option<i32>,
    lw_100mm: Option<i32>,
    ltype_name: Option<&str>,
    ltypes: &BTreeMap<String, Vec<F64>>,
) -> OutlineStyle {
    let col = dxf_aci_to_color(aci, truecolor);
    let mut outl = if let Some(lw) = lw_100mm
        && lw > 0
        && let Ok(w_pt) = F64::new((f64::from(lw) * 0.01 * 72.0 / 25.4).clamp(0.05, 288.0))
    {
        OutlineStyle::with_width(col, w_pt)
    } else {
        OutlineStyle::hairline(col)
    };
    if let Some(lt) = ltype_name.map(str::trim)
        && !lt.is_empty()
        && !lt.eq_ignore_ascii_case("CONTINUOUS")
        && !lt.eq_ignore_ascii_case("BYLAYER")
        && !lt.eq_ignore_ascii_case("BYBLOCK")
        && let Some(pat) = ltypes.get(&lt.to_ascii_uppercase())
    {
        outl.dash_pattern = pat.clone();
    }
    outl
}

fn try_decode_dxf_rounded_rect(verts: &[(f64, f64, f64)], bx_off: f64, page_h: f64) -> Option<RectSpec> {
    if verts.len() != 8 {
        return None;
    }
    let s_verts: Vec<(f64, f64, f64)> = verts.iter().map(|&(vx, vy, b)| (vx - bx_off, page_h - vy, b)).collect();
    let v0 = *s_verts.first()?;
    let v1 = *s_verts.get(1)?;
    let v2 = *s_verts.get(2)?;
    let v3 = *s_verts.get(3)?;
    let v4 = *s_verts.get(4)?;
    let v5 = *s_verts.get(5)?;
    let v6 = *s_verts.get(6)?;
    let v7 = *s_verts.get(7)?;
    let q_bulge = std::f64::consts::SQRT_2 - 1.0;
    if v0.2.abs() > 1e-3
        || v2.2.abs() > 1e-3
        || v4.2.abs() > 1e-3
        || v6.2.abs() > 1e-3
        || (v1.2.abs() - q_bulge).abs() > 1e-2
        || (v3.2.abs() - q_bulge).abs() > 1e-2
        || (v5.2.abs() - q_bulge).abs() > 1e-2
        || (v7.2.abs() - q_bulge).abs() > 1e-2
    {
        return None;
    }
    let min_x = s_verts.iter().map(|v| v.0).fold(f64::INFINITY, f64::min);
    let max_x = s_verts.iter().map(|v| v.0).fold(f64::NEG_INFINITY, f64::max);
    let min_y = s_verts.iter().map(|v| v.1).fold(f64::INFINITY, f64::min);
    let max_y = s_verts.iter().map(|v| v.1).fold(f64::NEG_INFINITY, f64::max);
    let r = (v0.0 - min_x).abs();
    if r <= 1e-4
        || (v0.1 - min_y).abs() > 1e-2
        || (v1.1 - min_y).abs() > 1e-2
        || (v1.0 - (max_x - r)).abs() > 1e-2
        || (v2.0 - max_x).abs() > 1e-2
        || (v2.1 - (min_y + r)).abs() > 1e-2
        || (v3.0 - max_x).abs() > 1e-2
        || (v3.1 - (max_y - r)).abs() > 1e-2
        || (v4.1 - max_y).abs() > 1e-2
        || (v4.0 - (max_x - r)).abs() > 1e-2
        || (v5.1 - max_y).abs() > 1e-2
        || (v5.0 - (min_x + r)).abs() > 1e-2
        || (v6.0 - min_x).abs() > 1e-2
        || (v6.1 - (max_y - r)).abs() > 1e-2
        || (v7.0 - min_x).abs() > 1e-2
        || (v7.1 - (min_y + r)).abs() > 1e-2
    {
        return None;
    }
    let rect = Rect::new(min_x, min_y, max_x, max_y).ok()?;
    RectSpec::uniform(rect, r, CornerKind::Round).ok()
}

/// Imports an AutoCAD ASCII DXF (`SECTION` / `BLOCKS` / `ENTITIES`) string into a [`Document`].
pub fn import_dxf(dxf_str: &str) -> Result<Document, DocError> {
    if !dxf_str.contains("ENTITIES") {
        return Err(DocError::InvalidDxf("missing ENTITIES section".to_owned()));
    }
    let mut doc = Document::new_a4_portrait();
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

    for (hp_idx, &(c, v)) in pairs.iter().enumerate() {
        if c == 9 && v == "$LIMMAX" {
            let mut lim_w = 0.0;
            let mut lim_h = 0.0;
            let mut sub = hp_idx.saturating_add(1);
            while let Some(&(sc, sv)) = pairs.get(sub) {
                if sc == 9 || sc == 0 {
                    break;
                }
                match sc {
                    10 => lim_w = sv.parse::<f64>().unwrap_or(0.0),
                    20 => lim_h = sv.parse::<f64>().unwrap_or(0.0),
                    _ => {}
                }
                sub = sub.saturating_add(1);
            }
            if lim_w > 1.0
                && lim_h > 1.0
                && let Ok(sz) = Size::new(lim_w, lim_h)
                && let Some(first_page) = doc.pages.first_mut()
            {
                first_page.size = sz;
            }
            break;
        }
    }
    let doc_page_h = doc.pages.first().map_or(841.89, |p| p.size.height.get());

    let mut saw_custom_layer = false;
    let mut blocks: BTreeMap<String, Vec<ObjectNode>> = BTreeMap::new();
    let mut active_block: Option<(String, f64, f64, Vec<ObjectNode>)> = None;
    let mut ltypes: BTreeMap<String, Vec<F64>> = BTreeMap::new();

    let mut p = 0usize;
    while let Some(&(code, val)) = pairs.get(p) {
        let (bx_off, page_h) = if let Some((_, bx, by, _)) = &active_block { (*bx, *by) } else { (0.0, doc_page_h) };
        if code == 0 && val == "LTYPE" {
            p = p.saturating_add(1);
            let mut lt_name = String::new();
            let mut dashes = Vec::new();
            while let Some(&(c, v)) = pairs.get(p) {
                if c == 0 {
                    break;
                }
                match c {
                    2 if lt_name.is_empty() => lt_name = v.trim().to_ascii_uppercase(),
                    49 => {
                        if let Ok(dv) = v.parse::<f64>()
                            && dv.abs() > 0.0
                            && let Ok(f) = F64::new(dv.abs())
                        {
                            dashes.push(f);
                        }
                    }
                    _ => {}
                }
                p = p.saturating_add(1);
            }
            if !lt_name.is_empty() && !dashes.is_empty() && dashes.len() <= 32 {
                ltypes.insert(lt_name, dashes);
            }
        } else if code == 0 && val == "BLOCK" {
            p = p.saturating_add(1);
            let mut bname = String::new();
            let mut bx = 0.0;
            let mut by = 0.0;
            while let Some(&(c, v)) = pairs.get(p) {
                if c == 0 {
                    break;
                }
                match c {
                    2 | 3 if bname.is_empty() => bname = v.to_owned(),
                    10 => bx = v.parse::<f64>().unwrap_or(0.0),
                    20 => by = v.parse::<f64>().unwrap_or(0.0),
                    _ => {}
                }
                p = p.saturating_add(1);
            }
            if !bname.is_empty() {
                active_block = Some((bname, bx, by, Vec::new()));
            }
        } else if code == 0 && val == "ENDBLK" {
            p = p.saturating_add(1);
            while let Some(&(c, _)) = pairs.get(p) {
                if c == 0 {
                    break;
                }
                p = p.saturating_add(1);
            }
            if let Some((bname, _, _, objs)) = active_block.take() {
                blocks.insert(bname, objs);
            }
        } else if code == 0 && val == "INSERT" {
            p = p.saturating_add(1);
            let mut layer_name: Option<&str> = None;
            let mut bname = "";
            let mut ix = 0.0;
            let mut iy = 0.0;
            let mut sx = 1.0_f64;
            let mut sy: Option<f64> = None;
            let mut rot_deg = 0.0_f64;
            while let Some(&(c, v)) = pairs.get(p) {
                if c == 0 {
                    break;
                }
                match c {
                    8 => layer_name = Some(v),
                    2 => bname = v,
                    10 => ix = v.parse::<f64>().unwrap_or(0.0),
                    20 => iy = v.parse::<f64>().unwrap_or(0.0),
                    41 => sx = v.parse::<f64>().unwrap_or(1.0),
                    42 => sy = v.parse::<f64>().ok(),
                    50 => rot_deg = v.parse::<f64>().unwrap_or(0.0),
                    _ => {}
                }
                p = p.saturating_add(1);
            }
            if let Some(template_objs) = blocks.get(bname).cloned()
                && !template_objs.is_empty()
            {
                let mut children = Vec::with_capacity(template_objs.len());
                for mut ch in template_objs {
                    ch.id = doc.allocate_object_id();
                    children.push(ch);
                }
                let grp_id = doc.allocate_object_id();
                let mut grp = ObjectNode::new(grp_id, Some(format!("DXF Insert {bname}")), ObjectKind::Group { children }, None, None);
                let sy_val = sy.unwrap_or(sx);
                let scale_tf = Affine2::scaling(sx, sy_val).unwrap_or(Affine2::IDENTITY);
                let rot_tf = Affine2::rotation_deg(-rot_deg).unwrap_or(Affine2::IDENTITY);
                let tr_tf = Affine2::translation(ix - bx_off, page_h - iy).unwrap_or(Affine2::IDENTITY);
                grp.transform = scale_tf.then(rot_tf).and_then(|m| m.then(tr_tf)).unwrap_or(tr_tf);
                push_dxf_node(&mut doc, &mut saw_custom_layer, &mut active_block, layer_name, grp);
            }
        } else if code == 0 && val == "LINE" {
            p = p.saturating_add(1);
            let mut layer_name: Option<&str> = None;
            let mut ltype_name: Option<&str> = None;
            let mut aci: Option<i32> = None;
            let mut truecolor: Option<i32> = None;
            let mut lw_100mm: Option<i32> = None;
            let mut x0 = 0.0;
            let mut y0 = 0.0;
            let mut x1 = 0.0;
            let mut y1 = 0.0;
            while let Some(&(c, v)) = pairs.get(p) {
                if c == 0 {
                    break;
                }
                match c {
                    8 => layer_name = Some(v),
                    6 => ltype_name = Some(v),
                    62 => aci = v.parse::<i32>().ok(),
                    420 => truecolor = v.parse::<i32>().ok(),
                    370 => lw_100mm = v.parse::<i32>().ok(),
                    10 => x0 = v.parse::<f64>().unwrap_or(0.0),
                    20 => y0 = v.parse::<f64>().unwrap_or(0.0),
                    11 => x1 = v.parse::<f64>().unwrap_or(0.0),
                    21 => y1 = v.parse::<f64>().unwrap_or(0.0),
                    _ => {}
                }
                p = p.saturating_add(1);
            }
            let mut path = Path::new();
            if let (Ok(pt0), Ok(pt1)) = (Point::new(x0 - bx_off, page_h - y0), Point::new(x1 - bx_off, page_h - y1)) {
                let _ = path.move_to(pt0);
                let _ = path.line_to(pt1);
                let id = doc.allocate_object_id();
                push_dxf_node(
                    &mut doc,
                    &mut saw_custom_layer,
                    &mut active_block,
                    layer_name,
                    ObjectNode::new(
                        id,
                        Some("DXF Line".to_owned()),
                        ObjectKind::Path(path),
                        None,
                        Some(dxf_make_outline(aci, truecolor, lw_100mm, ltype_name, &ltypes)),
                    ),
                );
            }
        } else if code == 0 && val == "POLYLINE" {
            p = p.saturating_add(1);
            let mut layer_name: Option<&str> = None;
            let mut ltype_name: Option<&str> = None;
            let mut aci: Option<i32> = None;
            let mut truecolor: Option<i32> = None;
            let mut lw_100mm: Option<i32> = None;
            let mut closed = false;
            while let Some(&(c, v)) = pairs.get(p) {
                if c == 0 {
                    break;
                }
                match c {
                    8 => layer_name = Some(v),
                    6 => ltype_name = Some(v),
                    62 => aci = v.parse::<i32>().ok(),
                    420 => truecolor = v.parse::<i32>().ok(),
                    370 => lw_100mm = v.parse::<i32>().ok(),
                    70 if v.parse::<i32>().unwrap_or(0) & 1 != 0 => closed = true,
                    _ => {}
                }
                p = p.saturating_add(1);
            }
            let mut verts: Vec<(f64, f64, f64)> = Vec::new();
            while let Some(&(c, v)) = pairs.get(p) {
                if c == 0 && v == "VERTEX" {
                    p = p.saturating_add(1);
                    let mut vx = 0.0;
                    let mut vy = 0.0;
                    let mut vbulge = 0.0;
                    while let Some(&(vc, vv)) = pairs.get(p) {
                        if vc == 0 {
                            break;
                        }
                        match vc {
                            10 => vx = vv.parse::<f64>().unwrap_or(0.0),
                            20 => vy = vv.parse::<f64>().unwrap_or(0.0),
                            42 => vbulge = vv.parse::<f64>().unwrap_or(0.0),
                            _ => {}
                        }
                        p = p.saturating_add(1);
                    }
                    verts.push((vx, vy, vbulge));
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
            if verts.len() >= 2
                && let Some(&(fx, fy, _)) = verts.first()
                && let Ok(first) = Point::new(fx - bx_off, page_h - fy)
            {
                let mut path = Path::new();
                let _ = path.move_to(first);
                let seg_count = if closed { verts.len() } else { verts.len().saturating_sub(1) };
                for idx_v in 0..seg_count {
                    let Some(&(x1_dxf, y1_dxf, bulge_dxf)) = verts.get(idx_v) else {
                        break;
                    };
                    let next_idx = if idx_v.saturating_add(1) < verts.len() { idx_v.saturating_add(1) } else { 0 };
                    let Some(&(x2_dxf, y2_dxf, _)) = verts.get(next_idx) else {
                        break;
                    };
                    let x1 = x1_dxf - bx_off;
                    let y1 = page_h - y1_dxf;
                    let x2 = x2_dxf - bx_off;
                    let y2 = page_h - y2_dxf;
                    if bulge_dxf.abs() > 1e-6 {
                        let b = -bulge_dxf;
                        let d = (x2 - x1).hypot(y2 - y1);
                        let r = d * (1.0 + b * b) / (4.0 * b.abs()).max(1e-9);
                        let _ = emit_svg_elliptical_arc(&mut path, x1, y1, r, r, 0.0, b.abs() > 1.0, b > 0.0, x2, y2);
                    } else if let Ok(pt) = Point::new(x2, y2) {
                        let _ = path.line_to(pt);
                    }
                }
                if closed {
                    let _ = path.close();
                }
                let id = doc.allocate_object_id();
                push_dxf_node(
                    &mut doc,
                    &mut saw_custom_layer,
                    &mut active_block,
                    layer_name,
                    ObjectNode::new(
                        id,
                        Some("DXF Polyline".to_owned()),
                        ObjectKind::Path(path),
                        None,
                        Some(dxf_make_outline(aci, truecolor, lw_100mm, ltype_name, &ltypes)),
                    ),
                );
            }
        } else if code == 0 && val == "CIRCLE" {
            p = p.saturating_add(1);
            let mut layer_name: Option<&str> = None;
            let mut ltype_name: Option<&str> = None;
            let mut aci: Option<i32> = None;
            let mut truecolor: Option<i32> = None;
            let mut lw_100mm: Option<i32> = None;
            let mut cx = 0.0;
            let mut cy = 0.0;
            let mut r = 0.0;
            while let Some(&(c, v)) = pairs.get(p) {
                if c == 0 {
                    break;
                }
                match c {
                    8 => layer_name = Some(v),
                    6 => ltype_name = Some(v),
                    62 => aci = v.parse::<i32>().ok(),
                    420 => truecolor = v.parse::<i32>().ok(),
                    370 => lw_100mm = v.parse::<i32>().ok(),
                    10 => cx = v.parse::<f64>().unwrap_or(0.0),
                    20 => cy = v.parse::<f64>().unwrap_or(0.0),
                    40 => r = v.parse::<f64>().unwrap_or(0.0),
                    _ => {}
                }
                p = p.saturating_add(1);
            }
            let sx = cx - bx_off;
            let sy = page_h - cy;
            if r > 0.0
                && let Ok(rect) = Rect::new(sx - r, sy - r, sx + r, sy + r)
            {
                let id = doc.allocate_object_id();
                push_dxf_node(
                    &mut doc,
                    &mut saw_custom_layer,
                    &mut active_block,
                    layer_name,
                    ObjectNode::new(
                        id,
                        Some("DXF Circle".to_owned()),
                        ObjectKind::Ellipse(EllipseSpec::full(rect)),
                        None,
                        Some(dxf_make_outline(aci, truecolor, lw_100mm, ltype_name, &ltypes)),
                    ),
                );
            }
        } else if code == 0 && val == "ARC" {
            p = p.saturating_add(1);
            let mut layer_name: Option<&str> = None;
            let mut ltype_name: Option<&str> = None;
            let mut aci: Option<i32> = None;
            let mut truecolor: Option<i32> = None;
            let mut lw_100mm: Option<i32> = None;
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
                    8 => layer_name = Some(v),
                    6 => ltype_name = Some(v),
                    62 => aci = v.parse::<i32>().ok(),
                    420 => truecolor = v.parse::<i32>().ok(),
                    370 => lw_100mm = v.parse::<i32>().ok(),
                    10 => cx = v.parse::<f64>().unwrap_or(0.0),
                    20 => cy = v.parse::<f64>().unwrap_or(0.0),
                    40 => r = v.parse::<f64>().unwrap_or(0.0),
                    50 => a0_deg = v.parse::<f64>().unwrap_or(0.0),
                    51 => a1_deg = v.parse::<f64>().unwrap_or(360.0),
                    _ => {}
                }
                p = p.saturating_add(1);
            }
            let sx = cx - bx_off;
            let sy = page_h - cy;
            if r > 0.0
                && let Ok(rect) = Rect::new(sx - r, sy - r, sx + r, sy + r)
                && let Ok(spec) = EllipseSpec::arc(rect, (-a1_deg).rem_euclid(360.0), (-a0_deg).rem_euclid(360.0))
            {
                let id = doc.allocate_object_id();
                push_dxf_node(
                    &mut doc,
                    &mut saw_custom_layer,
                    &mut active_block,
                    layer_name,
                    ObjectNode::new(
                        id,
                        Some("DXF Arc".to_owned()),
                        ObjectKind::Ellipse(spec),
                        None,
                        Some(dxf_make_outline(aci, truecolor, lw_100mm, ltype_name, &ltypes)),
                    ),
                );
            }
        } else if code == 0 && val == "ELLIPSE" {
            p = p.saturating_add(1);
            let mut layer_name: Option<&str> = None;
            let mut ltype_name: Option<&str> = None;
            let mut aci: Option<i32> = None;
            let mut truecolor: Option<i32> = None;
            let mut lw_100mm: Option<i32> = None;
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
                    8 => layer_name = Some(v),
                    6 => ltype_name = Some(v),
                    62 => aci = v.parse::<i32>().ok(),
                    420 => truecolor = v.parse::<i32>().ok(),
                    370 => lw_100mm = v.parse::<i32>().ok(),
                    10 => cx = v.parse::<f64>().unwrap_or(0.0),
                    20 => cy = v.parse::<f64>().unwrap_or(0.0),
                    11 => mx = v.parse::<f64>().unwrap_or(0.0),
                    21 => my = v.parse::<f64>().unwrap_or(0.0),
                    40 => ratio = v.parse::<f64>().unwrap_or(1.0),
                    _ => {}
                }
                p = p.saturating_add(1);
            }
            let sx = cx - bx_off;
            let sy = page_h - cy;
            let rx = mx.hypot(my);
            let ry = rx * ratio.clamp(1e-3, 100.0);
            if rx > 0.0
                && ry > 0.0
                && let Ok(rect) = Rect::new(sx - rx, sy - ry, sx + rx, sy + ry)
            {
                let id = doc.allocate_object_id();
                let mut node = ObjectNode::new(
                    id,
                    Some("DXF Ellipse".to_owned()),
                    ObjectKind::Ellipse(EllipseSpec::full(rect)),
                    None,
                    Some(dxf_make_outline(aci, truecolor, lw_100mm, ltype_name, &ltypes)),
                );
                let ang_deg = (-my).atan2(mx).to_degrees();
                if ang_deg.abs() > 1e-6
                    && let (Ok(t0), Ok(rot), Ok(t1)) = (Affine2::translation(-sx, -sy), Affine2::rotation_deg(ang_deg), Affine2::translation(sx, sy))
                    && let Ok(tr) = t0.then(rot).and_then(|m| m.then(t1))
                {
                    node.transform = tr;
                }
                push_dxf_node(&mut doc, &mut saw_custom_layer, &mut active_block, layer_name, node);
            }
        } else if code == 0 && val == "LWPOLYLINE" {
            p = p.saturating_add(1);
            let mut layer_name: Option<&str> = None;
            let mut ltype_name: Option<&str> = None;
            let mut closed = false;
            let mut aci: Option<i32> = None;
            let mut truecolor: Option<i32> = None;
            let mut lw_100mm: Option<i32> = None;
            let mut verts: Vec<(f64, f64, f64)> = Vec::new();
            while let Some(&(c, v)) = pairs.get(p) {
                if c == 0 {
                    break;
                }
                match c {
                    8 => layer_name = Some(v),
                    6 => ltype_name = Some(v),
                    70 if v.parse::<i32>().unwrap_or(0) & 1 != 0 => closed = true,
                    62 => aci = v.parse::<i32>().ok(),
                    420 => truecolor = v.parse::<i32>().ok(),
                    370 => lw_100mm = v.parse::<i32>().ok(),
                    10 => verts.push((v.parse::<f64>().unwrap_or(0.0), 0.0, 0.0)),
                    20 => {
                        if let Some(last) = verts.last_mut() {
                            last.1 = v.parse::<f64>().unwrap_or(0.0);
                        }
                    }
                    42 => {
                        if let Some(last) = verts.last_mut() {
                            last.2 = v.parse::<f64>().unwrap_or(0.0);
                        }
                    }
                    _ => {}
                }
                p = p.saturating_add(1);
            }
            if closed && let Some(rspec) = try_decode_dxf_rounded_rect(&verts, bx_off, page_h) {
                let id = doc.allocate_object_id();
                push_dxf_node(
                    &mut doc,
                    &mut saw_custom_layer,
                    &mut active_block,
                    layer_name,
                    ObjectNode::new(
                        id,
                        Some("DXF RoundedRect".to_owned()),
                        ObjectKind::Rect(rspec),
                        None,
                        Some(dxf_make_outline(aci, truecolor, lw_100mm, ltype_name, &ltypes)),
                    ),
                );
            } else if verts.len() >= 2
                && let Some(&(fx, fy, _)) = verts.first()
                && let Ok(first) = Point::new(fx - bx_off, page_h - fy)
            {
                let mut path = Path::new();
                let _ = path.move_to(first);
                let seg_count = if closed { verts.len() } else { verts.len().saturating_sub(1) };
                for idx_v in 0..seg_count {
                    let Some(&(x1_dxf, y1_dxf, bulge_dxf)) = verts.get(idx_v) else {
                        break;
                    };
                    let next_idx = if idx_v.saturating_add(1) < verts.len() { idx_v.saturating_add(1) } else { 0 };
                    let Some(&(x2_dxf, y2_dxf, _)) = verts.get(next_idx) else {
                        break;
                    };
                    let x1 = x1_dxf - bx_off;
                    let y1 = page_h - y1_dxf;
                    let x2 = x2_dxf - bx_off;
                    let y2 = page_h - y2_dxf;
                    if bulge_dxf.abs() > 1e-6 {
                        let b = -bulge_dxf;
                        let d = (x2 - x1).hypot(y2 - y1);
                        let r = d * (1.0 + b * b) / (4.0 * b.abs()).max(1e-9);
                        let _ = emit_svg_elliptical_arc(&mut path, x1, y1, r, r, 0.0, b.abs() > 1.0, b > 0.0, x2, y2);
                    } else if let Ok(pt) = Point::new(x2, y2) {
                        let _ = path.line_to(pt);
                    }
                }
                if closed {
                    let _ = path.close();
                }
                let id = doc.allocate_object_id();
                push_dxf_node(
                    &mut doc,
                    &mut saw_custom_layer,
                    &mut active_block,
                    layer_name,
                    ObjectNode::new(
                        id,
                        Some("DXF LwPolyline".to_owned()),
                        ObjectKind::Path(path),
                        None,
                        Some(dxf_make_outline(aci, truecolor, lw_100mm, ltype_name, &ltypes)),
                    ),
                );
            }
        } else if code == 0 && val == "SPLINE" {
            p = p.saturating_add(1);
            let mut layer_name: Option<&str> = None;
            let mut ltype_name: Option<&str> = None;
            let mut closed = false;
            let mut aci: Option<i32> = None;
            let mut truecolor: Option<i32> = None;
            let mut lw_100mm: Option<i32> = None;
            let mut ctrl_pts: Vec<(f64, f64)> = Vec::new();
            let mut fit_pts: Vec<(f64, f64)> = Vec::new();
            while let Some(&(c, v)) = pairs.get(p) {
                if c == 0 {
                    break;
                }
                match c {
                    8 => layer_name = Some(v),
                    6 => ltype_name = Some(v),
                    70 if v.parse::<i32>().unwrap_or(0) & 1 != 0 => closed = true,
                    62 => aci = v.parse::<i32>().ok(),
                    420 => truecolor = v.parse::<i32>().ok(),
                    370 => lw_100mm = v.parse::<i32>().ok(),
                    10 => ctrl_pts.push((v.parse::<f64>().unwrap_or(0.0), 0.0)),
                    20 => {
                        if let Some(last) = ctrl_pts.last_mut() {
                            last.1 = v.parse::<f64>().unwrap_or(0.0);
                        }
                    }
                    11 => fit_pts.push((v.parse::<f64>().unwrap_or(0.0), 0.0)),
                    21 => {
                        if let Some(last) = fit_pts.last_mut() {
                            last.1 = v.parse::<f64>().unwrap_or(0.0);
                        }
                    }
                    _ => {}
                }
                p = p.saturating_add(1);
            }
            let src = if ctrl_pts.len() >= 2 { &ctrl_pts } else { &fit_pts };
            let pts: Vec<Point> = src.iter().filter_map(|&(x, y)| Point::new(x - bx_off, page_h - y).ok()).collect();
            if let Some(&first) = pts.first()
                && pts.len() >= 2
            {
                let mut path = Path::new();
                let _ = path.move_to(first);
                if pts.len() >= 4 && (pts.len().saturating_sub(1)).is_multiple_of(3) {
                    let mut k = 1usize;
                    while let (Some(&c1), Some(&c2), Some(&end)) = (pts.get(k), pts.get(k.saturating_add(1)), pts.get(k.saturating_add(2))) {
                        let _ = path.cubic_to(c1, c2, end);
                        k = k.saturating_add(3);
                    }
                } else {
                    for idx_p in 0..pts.len().saturating_sub(1) {
                        if let (Some(&p0), Some(&p1)) = (pts.get(idx_p), pts.get(idx_p.saturating_add(1))) {
                            let c1 =
                                Point::new(p0.x.get() + (p1.x.get() - p0.x.get()) / 3.0, p0.y.get() + (p1.y.get() - p0.y.get()) / 3.0).unwrap_or(p0);
                            let c2 =
                                Point::new(p0.x.get() + 2.0 * (p1.x.get() - p0.x.get()) / 3.0, p0.y.get() + 2.0 * (p1.y.get() - p0.y.get()) / 3.0)
                                    .unwrap_or(p1);
                            let _ = path.cubic_to(c1, c2, p1);
                        }
                    }
                }
                if closed {
                    let _ = path.close();
                }
                let id = doc.allocate_object_id();
                push_dxf_node(
                    &mut doc,
                    &mut saw_custom_layer,
                    &mut active_block,
                    layer_name,
                    ObjectNode::new(
                        id,
                        Some("DXF Spline".to_owned()),
                        ObjectKind::Path(path),
                        None,
                        Some(dxf_make_outline(aci, truecolor, lw_100mm, ltype_name, &ltypes)),
                    ),
                );
            }
        } else if code == 0 && val == "SOLID" {
            p = p.saturating_add(1);
            let mut layer_name: Option<&str> = None;
            let mut aci: Option<i32> = None;
            let mut truecolor: Option<i32> = None;
            let mut x0 = 0.0;
            let mut y0 = 0.0;
            let mut x1 = 0.0;
            let mut y1 = 0.0;
            let mut x2 = 0.0;
            let mut y2 = 0.0;
            let mut x3: Option<f64> = None;
            let mut y3: Option<f64> = None;
            while let Some(&(c, v)) = pairs.get(p) {
                if c == 0 {
                    break;
                }
                match c {
                    8 => layer_name = Some(v),
                    62 => aci = v.parse::<i32>().ok(),
                    420 => truecolor = v.parse::<i32>().ok(),
                    10 => x0 = v.parse::<f64>().unwrap_or(0.0),
                    20 => y0 = v.parse::<f64>().unwrap_or(0.0),
                    11 => x1 = v.parse::<f64>().unwrap_or(0.0),
                    21 => y1 = v.parse::<f64>().unwrap_or(0.0),
                    12 => x2 = v.parse::<f64>().unwrap_or(0.0),
                    22 => y2 = v.parse::<f64>().unwrap_or(0.0),
                    13 => x3 = v.parse::<f64>().ok(),
                    23 => y3 = v.parse::<f64>().ok(),
                    _ => {}
                }
                p = p.saturating_add(1);
            }
            let x3_v = x3.unwrap_or(x2);
            let y3_v = y3.unwrap_or(y2);
            if let (Ok(p0), Ok(p1), Ok(p2), Ok(p3)) = (
                Point::new(x0 - bx_off, page_h - y0),
                Point::new(x1 - bx_off, page_h - y1),
                Point::new(x2 - bx_off, page_h - y2),
                Point::new(x3_v - bx_off, page_h - y3_v),
            ) {
                let mut path = Path::new();
                let _ = path.move_to(p0);
                let _ = path.line_to(p1);
                if p3 != p2 {
                    let _ = path.line_to(p3);
                }
                let _ = path.line_to(p2);
                let _ = path.close();
                let id = doc.allocate_object_id();
                let fill_col = dxf_aci_to_color(aci, truecolor);
                push_dxf_node(
                    &mut doc,
                    &mut saw_custom_layer,
                    &mut active_block,
                    layer_name,
                    ObjectNode::new(id, Some("DXF Solid".to_owned()), ObjectKind::Path(path), Some(FillStyle::uniform(fill_col)), None),
                );
            }
        } else if code == 0 && (val == "TEXT" || val == "MTEXT") {
            let is_mtext = val == "MTEXT";
            p = p.saturating_add(1);
            let mut layer_name: Option<&str> = None;
            let mut style_font: Option<&str> = None;
            let mut aci: Option<i32> = None;
            let mut truecolor: Option<i32> = None;
            let mut tx = 0.0;
            let mut ty = 0.0;
            let mut height = 12.0;
            let mut rot_deg = 0.0_f64;
            let mut text_val = String::new();
            while let Some(&(c, v)) = pairs.get(p) {
                if c == 0 {
                    break;
                }
                match c {
                    8 => layer_name = Some(v),
                    7 => style_font = Some(v),
                    62 => aci = v.parse::<i32>().ok(),
                    420 => truecolor = v.parse::<i32>().ok(),
                    10 => tx = v.parse::<f64>().unwrap_or(0.0),
                    20 => ty = v.parse::<f64>().unwrap_or(0.0),
                    40 => height = v.parse::<f64>().unwrap_or(12.0),
                    50 => rot_deg = v.parse::<f64>().unwrap_or(0.0),
                    1 | 3 => text_val.push_str(v),
                    _ => {}
                }
                p = p.saturating_add(1);
            }
            if is_mtext {
                text_val = text_val.replace("\\P", " ").replace("\\p", " ");
            }
            if !text_val.is_empty()
                && let Ok(pos) = Point::new(tx - bx_off, page_h - ty)
                && let Ok(size_pt) = F64::new(height.max(1.0))
            {
                let id = doc.allocate_object_id();
                let label = if is_mtext { "DXF MText" } else { "DXF Text" };
                let fill_col = dxf_aci_to_color(aci, truecolor);
                let font_family =
                    style_font.map(str::trim).filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("STANDARD")).unwrap_or("Sans").to_owned();
                let mut node = ObjectNode::new(
                    id,
                    Some(label.to_owned()),
                    ObjectKind::ArtisticText { text: text_val, font_family, size_pt, position: pos },
                    Some(FillStyle::uniform(fill_col)),
                    None,
                );
                if !is_mtext && rot_deg.abs() > 1e-6 {
                    let screen_deg = (-rot_deg).rem_euclid(360.0);
                    if let (Ok(t0), Ok(rot), Ok(t1)) = (
                        Affine2::translation(-pos.x.get(), -pos.y.get()),
                        Affine2::rotation_deg(screen_deg),
                        Affine2::translation(pos.x.get(), pos.y.get()),
                    ) && let Ok(tr) = t0.then(rot).and_then(|m| m.then(t1))
                    {
                        node.transform = tr;
                    }
                }
                push_dxf_node(&mut doc, &mut saw_custom_layer, &mut active_block, layer_name, node);
            }
        } else {
            p = p.saturating_add(1);
        }
    }

    Ok(doc)
}

fn dxf_aci_to_color(aci: Option<i32>, truecolor: Option<i32>) -> Color {
    if let Some(rgb) = truecolor {
        let r = ((rgb >> 16) & 0xFF) as u8;
        let g = ((rgb >> 8) & 0xFF) as u8;
        let b = (rgb & 0xFF) as u8;
        return Color::rgb8(r, g, b);
    }
    match aci.unwrap_or(7) {
        1 => Color::rgb8(255, 0, 0),
        2 => Color::rgb8(255, 255, 0),
        3 => Color::rgb8(0, 255, 0),
        4 => Color::rgb8(0, 255, 255),
        5 => Color::rgb8(0, 0, 255),
        6 => Color::rgb8(255, 0, 255),
        8 => Color::rgb8(128, 128, 128),
        9 => Color::rgb8(192, 192, 192),
        _ => Color::BLACK,
    }
}
