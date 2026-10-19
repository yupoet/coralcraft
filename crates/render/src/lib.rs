//! `coralcraft-render`: Clean-room 4x4 supersampled software rasterizer, Fountain Fill shader,
//! PowerClip mask compositor, and ISO/IEC 15948 PNG encoder for CoralCraft.

pub mod pdf;
pub mod png;
pub mod raster;
pub mod surface;
pub mod svg;

use coralcraft_color::Color;
use coralcraft_doc::{Document, FillStyle, ObjectKind, ObjectNode, OutlineStyle, VectorEffect};
use coralcraft_geom::{Affine2, FillRule, FlattenOpts, Path, Point, Rect, RectSpec, artistic_text_to_path, flatten_path};

pub use pdf::export_pdf;
pub use png::{adler32, decode_stored_png_rgba, encode_png};
pub use raster::{SS, evaluate_fountain_t, expand_stroke_polylines, rasterize_coverage, shade_coverage};
pub use surface::{
    MAX_CLIP_DEPTH, MAX_SURFACE_PIXELS, MAX_SURFACE_SIDE, RenderError, RenderWarning, Surface, premul_to_straight, src_over_premul,
    straight_to_premul,
};
pub use svg::{export_svg, path_to_svg_d};

/// Options controlling document page rasterization.
#[derive(Clone, Debug, PartialEq)]
pub struct RenderOpts {
    /// Device pixels per typographic point (`1.0` = 72 DPI, `2.0` = 144 DPI).
    pub scale: f64,
    /// When `true`, omits the default paper-white page background and starts from transparent RGBA `(0,0,0,0)`.
    pub transparent_background: bool,
    /// When `true`, skips non-printable layers (`printable == false`, such as `Guides` and `Desktop`).
    pub export_mode: bool,
}

impl Default for RenderOpts {
    fn default() -> Self {
        Self { scale: 1.0, transparent_background: false, export_mode: true }
    }
}

/// Output of [`render_page`]: the rasterized [`Surface`], encoded PNG bytes, and warning ledger.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderOutput {
    /// Premultiplied RGBA8 surface.
    pub surface: Surface,
    /// Deterministic encoded PNG bytes.
    pub png_bytes: Vec<u8>,
    /// Non-fatal warnings encountered during rendering.
    pub warnings: Vec<RenderWarning>,
}

/// Rasterizes `page_index` (0-based) of `doc` into a [`RenderOutput`].
pub fn render_page(doc: &Document, page_index: usize, opts: &RenderOpts) -> Result<RenderOutput, RenderError> {
    if !opts.scale.is_finite() || opts.scale <= 0.0 {
        return Err(RenderError::InvalidScale);
    }

    let layers = doc.effective_layers_for_page(page_index, opts.export_mode)?;
    let page = &doc.pages[page_index];

    let w_px = (page.size.width.get() * opts.scale).round() as u32;
    let h_px = (page.size.height.get() * opts.scale).round() as u32;

    let bg_rgba = if opts.transparent_background {
        [0, 0, 0, 0]
    } else if let Some(FillStyle::Uniform { color }) = &page.background {
        color.to_rgba8(1.0)
    } else {
        [255, 255, 255, 255]
    };

    let mut surface = Surface::new(w_px, h_px, bg_rgba)?;
    let mut warnings = Vec::new();
    let page_to_dev = Affine2::scaling(opts.scale, opts.scale)?;

    // If page has a non-uniform (Fountain) background and !transparent_background, shade it over the page rect
    if !opts.transparent_background
        && let Some(bg_fill @ FillStyle::Fountain { .. }) = &page.background
    {
        let page_rect = Rect::new(0.0, 0.0, page.size.width.get(), page.size.height.get())?;
        let full_cov = vec![16_u8; (w_px as usize) * (h_px as usize)];
        shade_coverage(&mut surface, &full_cov, bg_fill, page_rect, page_to_dev, 1.0, &mut warnings, 0)?;
    }

    for layer in layers {
        let layer_opacity = layer.opacity.get().clamp(0.0, 1.0);
        if layer_opacity <= 0.0 {
            continue;
        }
        for obj in &layer.objects {
            render_object_node(&mut surface, obj, page_to_dev, opts.scale, layer_opacity, 0, &mut warnings)?;
        }
    }

    let png_bytes = encode_png(&surface)?;
    Ok(RenderOutput { surface, png_bytes, warnings })
}

fn render_object_node(
    surface: &mut Surface,
    node: &ObjectNode,
    parent_to_dev: Affine2,
    page_scale: f64,
    inherited_opacity: f64,
    clip_depth: u8,
    warnings: &mut Vec<RenderWarning>,
) -> Result<(), RenderError> {
    if !node.visible {
        return Ok(());
    }
    let eff_opacity = (inherited_opacity * node.opacity.get()).clamp(0.0, 1.0);
    if eff_opacity <= 0.0 {
        return Ok(());
    }

    let local_to_dev = node.transform.then(parent_to_dev)?;

    match &node.kind {
        ObjectKind::Group { children } => {
            for ch in children {
                render_object_node(surface, ch, local_to_dev, page_scale, eff_opacity, clip_depth, warnings)?;
            }
            return Ok(());
        }
        ObjectKind::Unsupported(u) => {
            warnings.push(RenderWarning::UnsupportedObjectFallback { object_id: node.id.0, tag: u.tag.display_ascii() });
            if let Some(bbox) = u.fallback_bounds {
                let path = RectSpec::sharp(bbox).to_path()?;
                paint_geometry_and_powerclip(surface, node, &path, bbox, local_to_dev, page_scale, eff_opacity, clip_depth, warnings)?;
            }
            for ch in &u.children {
                render_object_node(surface, ch, local_to_dev, page_scale, eff_opacity, clip_depth, warnings)?;
            }
            return Ok(());
        }
        _ => {}
    }

    let Some((local_path, local_bbox)) = extract_local_path_and_bbox(&node.kind)? else {
        return Ok(());
    };

    paint_geometry_and_powerclip(surface, node, &local_path, local_bbox, local_to_dev, page_scale, eff_opacity, clip_depth, warnings)
}

#[allow(clippy::too_many_arguments)]
fn paint_geometry_and_powerclip(
    surface: &mut Surface,
    node: &ObjectNode,
    local_path: &Path,
    local_bbox: Rect,
    local_to_dev: Affine2,
    page_scale: f64,
    eff_opacity: f64,
    clip_depth: u8,
    warnings: &mut Vec<RenderWarning>,
) -> Result<(), RenderError> {
    let dev_path = local_path.transform(local_to_dev)?;
    let flatten_opts = FlattenOpts::default();
    let dev_polys = flatten_path(&dev_path, &flatten_opts)?;

    // Determine effective FillRule after potential orientation-flipping mirror transform
    let fill_rule = dev_path.fill_rule;

    // Precompute container coverage if needed for fill or PowerClip
    let need_container_cov = node.fill.is_some() || !node.powerclip_children.is_empty();
    let container_cov = if need_container_cov { Some(rasterize_coverage(&dev_polys, fill_rule, surface.width, surface.height)) } else { None };

    let behind_fill = node.outline.as_ref().is_some_and(|o| o.behind_fill);

    // 0. Paint DropShadow and Extrude vector effects behind the object
    for effect in &node.effects {
        match effect {
            VectorEffect::DropShadow { offset_x, offset_y, opacity, color } => {
                let dx = offset_x.get() * page_scale;
                let dy = offset_y.get() * page_scale;
                let mut shadow_polys = dev_polys.clone();
                for poly in &mut shadow_polys {
                    for pt in &mut poly.points {
                        if let Ok(shifted) = Point::new(pt.x.get() + dx, pt.y.get() + dy) {
                            *pt = shifted;
                        }
                    }
                }
                let shadow_cov = rasterize_coverage(&shadow_polys, fill_rule, surface.width, surface.height);
                let shadow_rgb = color.to_srgb8();
                let shadow_a = eff_opacity * opacity.get().clamp(0.0, 1.0);
                let w = surface.width as usize;
                let h = surface.height as usize;
                for py in 0..h {
                    for px in 0..w {
                        let cov = shadow_cov[py * w + px];
                        if cov > 0 {
                            let a = shadow_a * (f64::from(cov) / 16.0);
                            surface.blend_straight(px as u32, py as u32, shadow_rgb, a);
                        }
                    }
                }
            }
            VectorEffect::Extrude { depth_pt, angle_deg } => {
                let rad = angle_deg.get().to_radians();
                let dx = depth_pt.get() * page_scale * rad.cos();
                let dy = depth_pt.get() * page_scale * rad.sin();
                let mut extrude_polys = dev_polys.clone();
                for poly in &mut extrude_polys {
                    for pt in &mut poly.points {
                        if let Ok(shifted) = Point::new(pt.x.get() + dx, pt.y.get() + dy) {
                            *pt = shifted;
                        }
                    }
                }
                let ext_cov = rasterize_coverage(&extrude_polys, fill_rule, surface.width, surface.height);
                let ext_rgb = [100, 100, 100];
                let w = surface.width as usize;
                let h = surface.height as usize;
                for py in 0..h {
                    for px in 0..w {
                        let cov = ext_cov[py * w + px];
                        if cov > 0 {
                            let a = eff_opacity * 0.7 * (f64::from(cov) / 16.0);
                            surface.blend_straight(px as u32, py as u32, ext_rgb, a);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    // 1. If CorelDRAW `behind_fill == true`, draw outline BEFORE fill
    if behind_fill && let Some(outline) = &node.outline {
        paint_outline(surface, &dev_polys, outline, node.transform, page_scale, eff_opacity)?;
    }

    // 2. Draw object Fill
    if let (Some(fill), Some(cov)) = (&node.fill, &container_cov) {
        shade_coverage(surface, cov, fill, local_bbox, local_to_dev, eff_opacity, warnings, node.id.0)?;
    }

    // 3. Draw CorelDRAW PowerClip children clipped to container coverage
    if !node.powerclip_children.is_empty()
        && let Some(cov) = &container_cov
    {
        if clip_depth >= MAX_CLIP_DEPTH {
            warnings.push(RenderWarning::ClipDepthTruncated { object_id: node.id.0, max_depth: MAX_CLIP_DEPTH });
            for pc in &node.powerclip_children {
                render_object_node(surface, pc, local_to_dev, page_scale, eff_opacity, clip_depth, warnings)?;
            }
        } else {
            let mut offscreen = Surface::new(surface.width, surface.height, [0, 0, 0, 0])?;
            for pc in &node.powerclip_children {
                render_object_node(&mut offscreen, pc, local_to_dev, page_scale, eff_opacity, clip_depth + 1, warnings)?;
            }
            surface.composite_masked(&offscreen, cov);
        }
    }

    // 4. If `behind_fill == false` (default), draw outline AFTER fill and PowerClip contents
    if !behind_fill && let Some(outline) = &node.outline {
        paint_outline(surface, &dev_polys, outline, node.transform, page_scale, eff_opacity)?;
    }

    // 5. Paint Contour vector effect around the object
    for effect in &node.effects {
        if let VectorEffect::Contour { steps, offset_pt, inside } = effect {
            let step_count = (*steps).min(20);
            for s in 1..=step_count {
                let dist = offset_pt.get() * (s as f64) * page_scale;
                let width_dev = dist * 2.0;
                let stroke_polys =
                    expand_stroke_polylines(&dev_polys, 0.5 * width_dev, coralcraft_doc::LineCap::Round, coralcraft_doc::LineJoin::Round, 4.0)?;
                let cov = rasterize_coverage(&stroke_polys, FillRule::NonZero, surface.width, surface.height);
                let w = surface.width as usize;
                let h = surface.height as usize;
                let rgb = if *inside { [80, 80, 80] } else { [180, 180, 180] };
                for py in 0..h {
                    for px in 0..w {
                        let c = cov[py * w + px];
                        if c > 0 {
                            let a = eff_opacity * 0.4 * (f64::from(c) / 16.0);
                            surface.blend_straight(px as u32, py as u32, rgb, a);
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

fn paint_outline(
    surface: &mut Surface,
    dev_polys: &[coralcraft_geom::Polyline],
    outline: &OutlineStyle,
    obj_transform: Affine2,
    page_scale: f64,
    eff_opacity: f64,
) -> Result<(), RenderError> {
    // CorelDRAW Hairline (`width_pt == 0.0`) always renders as 1.0 device pixel width;
    // otherwise width scales by `page_scale` and optionally `sqrt(|det(obj_transform)|)` when `scale_with_object == true`.
    let width_dev = if outline.is_hairline() {
        1.0
    } else {
        let obj_scale = if outline.scale_with_object { obj_transform.determinant().abs().sqrt() } else { 1.0 };
        outline.width_pt.get() * page_scale * obj_scale
    };

    if width_dev <= 1e-6 {
        return Ok(());
    }

    let stroke_polys = expand_stroke_polylines(dev_polys, 0.5 * width_dev, outline.cap, outline.join, outline.miter_limit.get())?;
    if stroke_polys.is_empty() {
        return Ok(());
    }

    let stroke_cov = rasterize_coverage(&stroke_polys, FillRule::NonZero, surface.width, surface.height);
    let rgb = outline.color.to_srgb8();
    let w = surface.width as usize;
    let h = surface.height as usize;
    for py in 0..h {
        for px in 0..w {
            let cov = stroke_cov[py * w + px];
            if cov > 0 {
                let a = eff_opacity * (f64::from(cov) / 16.0);
                surface.blend_straight(px as u32, py as u32, rgb, a);
            }
        }
    }

    Ok(())
}

fn extract_local_path_and_bbox(kind: &ObjectKind) -> Result<Option<(Path, Rect)>, RenderError> {
    match kind {
        ObjectKind::Rect(r) => {
            let p = r.to_path()?;
            Ok(Some((p, r.rect)))
        }
        ObjectKind::Ellipse(e) => {
            let p = e.to_path()?;
            Ok(Some((p, e.rect)))
        }
        ObjectKind::Polygon(poly) => {
            let p = poly.to_path()?;
            let bbox = p.bounds().unwrap_or_else(|| Rect::new(0.0, 0.0, 1.0, 1.0).unwrap_or_default());
            Ok(Some((p, bbox)))
        }
        ObjectKind::Path(p) => {
            let Some(bbox) = p.bounds() else {
                return Ok(None);
            };
            Ok(Some((p.clone(), bbox)))
        }
        ObjectKind::ArtisticText { text, font_family, size_pt, position } => {
            if text.is_empty() {
                return Ok(None);
            }
            let path = artistic_text_to_path(text, font_family, size_pt.get(), *position);
            let h = size_pt.get().max(4.0);
            let w = (text.chars().count() as f64) * h * 0.6;
            let bbox = path
                .bounds()
                .unwrap_or_else(|| Rect::new(position.x.get(), position.y.get() - h, position.x.get() + w, position.y.get()).unwrap_or_default());
            Ok(Some((path, bbox)))
        }
        ObjectKind::Group { .. } | ObjectKind::Unsupported(_) => Ok(None),
    }
}

/// Renders a deterministic `size_px × size_px` PNG swatch preview with a 16 px checkerboard border.
pub fn render_swatch_png(color: &Color, size_px: u32) -> Result<Vec<u8>, RenderError> {
    let side = size_px.clamp(32, 2048);
    let mut surface = Surface::new(side, side, [255, 255, 255, 255])?;
    let rgb = color.to_srgb8();
    let border = 16_u32.min(side / 4);

    for y in 0..side {
        for x in 0..side {
            let in_border = x < border || x >= side - border || y < border || y >= side - border;
            if in_border {
                let cell = ((x / 8) + (y / 8)) % 2;
                let g = if cell == 0 { 220 } else { 180 };
                surface.blend_straight(x, y, [g, g, g], 1.0);
            } else {
                surface.blend_straight(x, y, rgb, 1.0);
            }
        }
    }

    encode_png(&surface)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use coralcraft_color::{FountainBlendMode, FountainStop};
    use coralcraft_doc::{ChunkTag, FountainGeometry, ObjectId, RawPayload, Residual, UnsupportedObject};
    use coralcraft_geom::{CornerKind, F64, PathElement, Point, Size};

    use super::*;

    #[test]
    fn axis_aligned_rect_coverage_and_png_roundtrip() {
        let mut doc = Document::new_a4_portrait();
        doc.pages[0].size = Size::new(20.0, 20.0).unwrap();

        let rect = RectSpec::sharp(Rect::new(5.0, 5.0, 15.0, 15.0).unwrap());
        doc.pages[0].layers[0].objects.push(ObjectNode::new(
            ObjectId(10),
            Some("Red Box".to_owned()),
            ObjectKind::Rect(rect),
            Some(FillStyle::uniform(Color::rgb8(255, 0, 0))),
            None,
        ));

        let opts = RenderOpts { scale: 1.0, transparent_background: true, export_mode: true };
        let out = render_page(&doc, 0, &opts).unwrap();
        assert_eq!(out.surface.width, 20);
        assert_eq!(out.surface.height, 20);

        // Exactly 10 x 10 = 100 interior pixels must have alpha == 255 and red == 255, while exterior is transparent
        let mut covered = 0;
        for y in 0..20 {
            for x in 0..20 {
                let px = out.surface.get_straight_rgba(x, y);
                if (5..15).contains(&x) && (5..15).contains(&y) {
                    assert_eq!(px, [255, 0, 0, 255]);
                    covered += 1;
                } else {
                    assert_eq!(px, [0, 0, 0, 0]);
                }
            }
        }
        assert_eq!(covered, 100);

        // Verify our PNG encoder is byte-deterministic and decodes back to the exact pixels
        let out2 = render_page(&doc, 0, &opts).unwrap();
        assert_eq!(out.png_bytes, out2.png_bytes);
        let (dw, dh, decoded_px) = decode_stored_png_rgba(&out.png_bytes).unwrap();
        assert_eq!((dw, dh), (20, 20));
        assert_eq!(decoded_px[5 * 20 + 5], [255, 0, 0, 255]);
        assert_eq!(decoded_px[0], [0, 0, 0, 0]);
    }

    #[test]
    fn diagonal_antialiasing_and_fountain_fill_shading() {
        // Two complementary 45-degree right triangles sharing the diagonal `x == y`:
        // Because subpixel spans use half-open `[x0, x1)`, their diagonal pixel coverages sum to 16 (10 + 6 == 16) with zero seam!
        let mut tri_upper = Path::new();
        tri_upper.move_to(Point::new(0.0, 0.0).unwrap()).unwrap();
        tri_upper.line_to(Point::new(10.0, 0.0).unwrap()).unwrap();
        tri_upper.line_to(Point::new(10.0, 10.0).unwrap()).unwrap();
        tri_upper.close().unwrap();

        let mut tri_lower = Path::new();
        tri_lower.move_to(Point::new(0.0, 0.0).unwrap()).unwrap();
        tri_lower.line_to(Point::new(10.0, 10.0).unwrap()).unwrap();
        tri_lower.line_to(Point::new(0.0, 10.0).unwrap()).unwrap();
        tri_lower.close().unwrap();

        let cov_upper = rasterize_coverage(&flatten_path(&tri_upper, &FlattenOpts::default()).unwrap(), FillRule::NonZero, 10, 10);
        let cov_lower = rasterize_coverage(&flatten_path(&tri_lower, &FlattenOpts::default()).unwrap(), FillRule::NonZero, 10, 10);
        assert_eq!(cov_upper[4 * 10 + 4], 10);
        assert_eq!(cov_lower[4 * 10 + 4], 6);
        assert_eq!(cov_upper[4 * 10 + 4] + cov_lower[4 * 10 + 4], 16);

        // Linear Fountain Fill (0° horizontal: Red -> Blue) across a 20x10 rect
        let mut doc = Document::new_a4_portrait();
        doc.pages[0].size = Size::new(20.0, 10.0).unwrap();
        let rect = RectSpec::sharp(Rect::new(0.0, 0.0, 20.0, 10.0).unwrap());
        doc.pages[0].layers[0].objects.push(ObjectNode::new(
            ObjectId(11),
            None,
            ObjectKind::Rect(rect),
            Some(FillStyle::Fountain {
                geometry: FountainGeometry::Linear { angle_deg: F64::ZERO, edge_pad: F64::ZERO },
                blend_mode: FountainBlendMode::Direct,
                stops: vec![FountainStop::new(0.0, Color::rgb8(255, 0, 0)).unwrap(), FountainStop::new(1.0, Color::rgb8(0, 0, 255)).unwrap()],
            }),
            None,
        ));

        let out = render_page(&doc, 0, &RenderOpts::default()).unwrap();
        let left_px = out.surface.get_straight_rgba(0, 5);
        let mid_px = out.surface.get_straight_rgba(10, 5);
        let right_px = out.surface.get_straight_rgba(19, 5);
        assert!(left_px[0] > 240 && left_px[2] < 15);
        assert!(mid_px[0] > 110 && mid_px[0] < 145 && mid_px[2] > 110 && mid_px[2] < 145);
        assert!(right_px[2] > 240 && right_px[0] < 15);
    }

    #[test]
    fn powerclip_masking_and_depth_truncation_warning() {
        let mut doc = Document::new_a4_portrait();
        doc.pages[0].size = Size::new(20.0, 20.0).unwrap();

        // Container is a 10x10 rect at (5..15, 5..15)
        let container_rect = RectSpec::sharp(Rect::new(5.0, 5.0, 15.0, 15.0).unwrap());
        let mut container = ObjectNode::new(ObjectId(20), Some("PowerClip Frame".to_owned()), ObjectKind::Rect(container_rect), None, None);

        // Child inside PowerClip is a full 20x20 green rect (0..20, 0..20)
        let child_rect = RectSpec::sharp(Rect::new(0.0, 0.0, 20.0, 20.0).unwrap());
        container.powerclip_children.push(ObjectNode::new(
            ObjectId(21),
            Some("Clipped Green Fill".to_owned()),
            ObjectKind::Rect(child_rect),
            Some(FillStyle::uniform(Color::rgb8(0, 255, 0))),
            None,
        ));

        doc.pages[0].layers[0].objects.push(container);

        let out = render_page(&doc, 0, &RenderOpts { scale: 1.0, transparent_background: true, export_mode: true }).unwrap();

        // Outside the PowerClip container (e.g. (2, 2)) must be 100% transparent; inside (10, 10) must be green!
        assert_eq!(out.surface.get_straight_rgba(2, 2), [0, 0, 0, 0]);
        assert_eq!(out.surface.get_straight_rgba(10, 10), [0, 255, 0, 255]);

        // Nesting 10 levels deep (> MAX_CLIP_DEPTH = 8) must emit ClipDepthTruncated and never panic
        let mut deep_node =
            ObjectNode::new(ObjectId(100), None, ObjectKind::Rect(container_rect), Some(FillStyle::uniform(Color::rgb8(0, 0, 255))), None);
        for d in 0..10 {
            let mut wrapper = ObjectNode::new(ObjectId(200 + d), None, ObjectKind::Rect(container_rect), None, None);
            wrapper.powerclip_children.push(deep_node);
            deep_node = wrapper;
        }
        doc.pages[0].layers[0].objects = vec![deep_node];
        let deep_out = render_page(&doc, 0, &RenderOpts::default()).unwrap();
        assert!(deep_out.warnings.iter().any(|w| matches!(w, RenderWarning::ClipDepthTruncated { .. })));
    }

    #[test]
    fn outline_behind_fill_and_hairline_semantics() {
        let mut doc = Document::new_a4_portrait();
        doc.pages[0].size = Size::new(30.0, 30.0).unwrap();

        let rect = RectSpec::sharp(Rect::new(10.0, 10.0, 20.0, 20.0).unwrap());
        let mut outline = OutlineStyle::with_width(Color::rgb8(0, 0, 255), F64::new(6.0).unwrap());
        outline.behind_fill = true;

        doc.pages[0].layers[0].objects.push(ObjectNode::new(
            ObjectId(30),
            None,
            ObjectKind::Rect(rect),
            Some(FillStyle::uniform(Color::rgb8(255, 0, 0))),
            Some(outline),
        ));

        let out = render_page(&doc, 0, &RenderOpts::default()).unwrap();
        // Because `behind_fill == true`, pixel (11, 11) inside the rect near the border is covered by the red fill on top of the blue stroke!
        assert_eq!(out.surface.get_straight_rgba(11, 11), [255, 0, 0, 255]);
        // And pixel (8, 15) outside the rect but inside the 6pt stroke half-width (3pt) is blue!
        assert_eq!(out.surface.get_straight_rgba(8, 15), [0, 0, 255, 255]);

        // Unsupported object fallback emits warning and never panics
        let u = UnsupportedObject {
            tag: ChunkTag(*b"mesh"),
            class_hint: None,
            payload: RawPayload(vec![1, 2, 3]),
            fallback_bounds: Some(Rect::new(0.0, 0.0, 5.0, 5.0).unwrap()),
            absolute_geometry: false,
            children: Vec::new(),
            residual: Residual::default(),
        };
        doc.pages[0].layers[0].objects.push(ObjectNode::new(
            ObjectId(31),
            None,
            ObjectKind::Unsupported(u),
            Some(FillStyle::uniform(Color::rgb8(100, 100, 100))),
            Some(OutlineStyle::hairline(Color::BLACK)),
        ));
        let out_u = render_page(&doc, 0, &RenderOpts::default()).unwrap();
        assert!(out_u.warnings.iter().any(|w| matches!(w, RenderWarning::UnsupportedObjectFallback { .. })));
        let _ = PathElement::Close;
        let _ = CornerKind::Round;
    }

    #[test]
    fn export_svg_and_pdf_preserve_fountain_powerclip_cmyk_and_exact_xref() {
        let mut doc = Document::new_a4_portrait();
        doc.pages[0].size = Size::new(200.0, 100.0).unwrap();
        doc.pages[0].bleed = F64::new(9.0).unwrap();

        let rect = RectSpec::sharp(Rect::new(10.0, 10.0, 100.0, 80.0).unwrap());
        let mut outline = OutlineStyle::with_width(Color::cmyk(1.0, 0.0, 0.5, 0.0).unwrap(), F64::new(4.0).unwrap());
        outline.behind_fill = true;

        let mut container = ObjectNode::new(
            ObjectId(50),
            Some("Fountain Frame".to_owned()),
            ObjectKind::Rect(rect),
            Some(FillStyle::Fountain {
                geometry: FountainGeometry::Linear { angle_deg: F64::new(45.0).unwrap(), edge_pad: F64::ZERO },
                blend_mode: FountainBlendMode::Direct,
                stops: vec![FountainStop::new(0.0, Color::rgb8(255, 0, 0)).unwrap(), FountainStop::new(1.0, Color::rgb8(0, 0, 255)).unwrap()],
            }),
            Some(outline),
        );
        container.powerclip_children.push(ObjectNode::new(
            ObjectId(51),
            Some("Inner Text".to_owned()),
            ObjectKind::ArtisticText {
                text: "CoralCraft & <SVG>".to_owned(),
                font_family: "Arial".to_owned(),
                size_pt: F64::new(14.0).unwrap(),
                position: Point::new(20.0, 40.0).unwrap(),
            },
            Some(FillStyle::uniform(Color::cmyk(0.0, 1.0, 1.0, 0.0).unwrap())),
            None,
        ));
        doc.pages[0].layers[0].objects.push(container);

        // 1. SVG 1.1 export check
        let svg = export_svg(&doc, 0).unwrap();
        assert!(
            svg.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\" version=\"1.1\" width=\"200pt\" height=\"100pt\" viewBox=\"0 0 200 100\">")
        );
        assert!(svg.contains("<linearGradient id=\"grad-obj-50\""));
        assert!(svg.contains("<clipPath id=\"clip-50\""));
        assert!(svg.contains("clip-path=\"url(#clip-50)\""));
        assert!(svg.contains("CoralCraft &amp; &lt;SVG&gt;"));

        // 2. ISO 32000-1 PDF 1.4 export check
        let pdf_bytes = export_pdf(&doc).unwrap();
        assert!(pdf_bytes.starts_with(b"%PDF-1.4\n"));
        assert!(pdf_bytes.ends_with(b"%%EOF\n"));
        let pdf_str = String::from_utf8_lossy(&pdf_bytes);
        assert!(pdf_str.contains("/MediaBox [0 0 218 118]"));
        assert!(pdf_str.contains("/TrimBox [9 9 209 109]"));
        assert!(pdf_str.contains("/BleedBox [0 0 218 118]"));
        // Verify native CMYK stroke (`K`) and fill (`k`) operators in content stream
        assert!(pdf_str.contains("1 0 0.5 0 K"));
        assert!(pdf_str.contains("0 1 1 0 k"));
        // Verify exact byte offset of `1 0 obj` in the xref table
        let obj1_pos = pdf_bytes.windows(8).position(|w| w == b"1 0 obj\n").unwrap();
        let expected_xref_entry = format!("{obj1_pos:010} 00000 n \n");
        assert!(pdf_str.contains(&expected_xref_entry));
    }
}
