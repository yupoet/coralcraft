//! `coralcraft-render`: Clean-room 4x4 supersampled software rasterizer, Fountain Fill shader,
//! PowerClip mask compositor, and ISO/IEC 15948 PNG encoder for CoralCraft.

pub mod eps;
pub mod pdf;
pub mod png;
pub mod raster;
pub mod surface;
pub mod svg;

use coralcraft_color::{Color, interpolate_stops};
use coralcraft_doc::{Document, FillStyle, LensKind, ObjectKind, ObjectNode, OutlineStyle, TransparencyStyle, VectorEffect};
use coralcraft_geom::{Affine2, FillRule, FlattenOpts, Path, Point, Rect, RectSpec, artistic_text_to_path, flatten_path};

pub use eps::{export_dxf, export_eps};
pub use pdf::{ImpositionSpec, PrepressOptions, export_pdf, export_pdf_imposed, export_pdf_with_prepress};
pub use png::{adler32, base64_encode, decode_stored_png_rgba, encode_png, encode_raw_png_rgba};
pub use raster::{SS, build_arrowhead_polylines, dash_polylines, evaluate_fountain_t, expand_stroke_polylines, rasterize_coverage, shade_coverage};
pub use surface::{
    MAX_CLIP_DEPTH, MAX_SURFACE_PIXELS, MAX_SURFACE_SIDE, RenderError, RenderWarning, Surface, premul_to_straight, src_over_premul,
    straight_to_premul,
};
pub use svg::{export_svg, export_svg_with_prepress, path_to_svg_d};

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
            for effect in &obj.effects {
                if let VectorEffect::Blend { target_id, steps, spine } = effect
                    && *steps > 0
                    && let Some(target_obj) = layer.objects.iter().find(|o| o.id == *target_id)
                {
                    render_blend_steps(&mut surface, obj, target_obj, *steps, spine.as_ref(), page_to_dev, opts.scale, layer_opacity, &mut warnings)?;
                }
            }
        }
    }

    let png_bytes = encode_png(&surface)?;
    Ok(RenderOutput { surface, png_bytes, warnings })
}

#[allow(clippy::too_many_arguments)]
fn render_blend_steps(
    surface: &mut Surface,
    start_obj: &ObjectNode,
    end_obj: &ObjectNode,
    steps: u32,
    spine: Option<&Path>,
    page_to_dev: Affine2,
    page_scale: f64,
    layer_opacity: f64,
    warnings: &mut Vec<RenderWarning>,
) -> Result<(), RenderError> {
    let Some((_, b0)) = extract_local_path_and_bbox(&start_obj.kind)? else {
        return Ok(());
    };
    let Some((_, b1)) = extract_local_path_and_bbox(&end_obj.kind)? else {
        return Ok(());
    };
    let c0 = start_obj.transform.apply_point(b0.center()?)?;
    let c1 = end_obj.transform.apply_point(b1.center()?)?;
    let spine_lut = spine.and_then(|sp| coralcraft_geom::ArcLengthTable::from_path(sp).ok()).filter(|lut| lut.total_length() > 1e-6);

    let clamped_steps = steps.min(64);
    for k in 1..=clamped_steps {
        let u = f64::from(k) / f64::from(clamped_steps + 1);
        let target_center = if let Some(lut) = &spine_lut
            && let Some(sample) = lut.sample_normalized(u)
        {
            sample.point
        } else {
            let x = c0.x.get() + u * (c1.x.get() - c0.x.get());
            let y = c0.y.get() + u * (c1.y.get() - c0.y.get());
            Point::new(x, y)?
        };
        let dx = target_center.x.get() - c0.x.get();
        let dy = target_center.y.get() - c0.y.get();
        let mut step_node = start_obj.clone();
        step_node.effects.clear();
        step_node.transform = start_obj.transform.then(Affine2::translation(dx, dy)?)?;
        if let (Some(FillStyle::Uniform { color: col0 }), Some(FillStyle::Uniform { color: col1 })) = (&start_obj.fill, &end_obj.fill) {
            let rgb0 = col0.to_srgb8();
            let rgb1 = col1.to_srgb8();
            let r = (f64::from(rgb0[0]) + u * (f64::from(rgb1[0]) - f64::from(rgb0[0]))).round() as u8;
            let g = (f64::from(rgb0[1]) + u * (f64::from(rgb1[1]) - f64::from(rgb0[1]))).round() as u8;
            let b = (f64::from(rgb0[2]) + u * (f64::from(rgb1[2]) - f64::from(rgb0[2]))).round() as u8;
            step_node.fill = Some(FillStyle::uniform(Color::rgb8(r, g, b)));
        }
        render_object_node(surface, &step_node, page_to_dev, page_scale, layer_opacity, 0, warnings)?;
    }
    Ok(())
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
    let mut eff_opacity = (inherited_opacity * node.opacity.get()).clamp(0.0, 1.0);
    if let Some(TransparencyStyle::Uniform { opacity }) = &node.transparency {
        eff_opacity = (eff_opacity * opacity.get()).clamp(0.0, 1.0);
    }
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

    let Some((mut local_path, mut local_bbox)) = extract_local_path_and_bbox(&node.kind)? else {
        return Ok(());
    };

    for effect in &node.effects {
        match effect {
            VectorEffect::ArtisticMediaBrush { start_width_pt, end_width_pt } => {
                if let Ok(brush_path) = coralcraft_geom::expand_artistic_brush_stroke(&local_path, start_width_pt.get(), end_width_pt.get())
                    && let Some(bb) = brush_path.bounds()
                {
                    local_path = brush_path;
                    local_bbox = bb;
                }
            }
            VectorEffect::Envelope { corners } => {
                if let Ok(warped) = coralcraft_geom::warp_path_bilinear(&local_path, local_bbox, *corners)
                    && let Some(bb) = warped.bounds()
                {
                    local_path = warped;
                    local_bbox = bb;
                }
            }
            VectorEffect::TextOnPath { spine, offset_pt } => {
                if let ObjectKind::ArtisticText { text, font_family, size_pt, .. } = &node.kind
                    && let Ok(curved) = coralcraft_geom::fit_text_to_path(text, font_family, size_pt.get(), spine, offset_pt.get())
                    && let Some(bb) = curved.bounds()
                {
                    local_path = curved;
                    local_bbox = bb;
                }
            }
            VectorEffect::Perspective { corners } => {
                if let Ok(persp) = coralcraft_geom::warp_path_perspective(&local_path, local_bbox, *corners)
                    && let Some(bb) = persp.bounds()
                {
                    local_path = persp;
                    local_bbox = bb;
                }
            }
            VectorEffect::Distortion { kind, amplitude, frequency } => {
                if let Ok(dist) = coralcraft_geom::distort_path(&local_path, local_bbox, *kind, amplitude.get(), frequency.get())
                    && let Some(bb) = dist.bounds()
                {
                    local_path = dist;
                    local_bbox = bb;
                }
            }
            _ => {}
        }
    }

    if let Some(TransparencyStyle::Fountain { geometry, blend_mode, stops }) = &node.transparency {
        let mut offscreen = Surface::new(surface.width, surface.height, [0, 0, 0, 0])?;
        paint_geometry_and_powerclip(&mut offscreen, node, &local_path, local_bbox, local_to_dev, page_scale, eff_opacity, clip_depth, warnings)?;
        let dev_to_local = local_to_dev.invert().unwrap_or(Affine2::IDENTITY);
        let bx0 = local_bbox.x0.get();
        let by0 = local_bbox.y0.get();
        let bw = local_bbox.width().max(1e-6);
        let bh = local_bbox.height().max(1e-6);
        let w = surface.width as usize;
        let h = surface.height as usize;

        for py in 0..h {
            for px in 0..w {
                let sp = offscreen.pixels[py * w + px];
                if sp[3] == 0 {
                    continue;
                }
                let dev_pt = Point::new((px as f64) + 0.5, (py as f64) + 0.5)?;
                let loc_pt = dev_to_local.apply_point(dev_pt)?;
                let u = (loc_pt.x.get() - bx0) / bw;
                let v = (loc_pt.y.get() - by0) / bh;
                let t = evaluate_fountain_t(*geometry, u, v);
                let rgb = interpolate_stops(stops, *blend_mode, t)?;
                let lum = (0.2126 * f64::from(rgb[0]) + 0.7152 * f64::from(rgb[1]) + 0.0722 * f64::from(rgb[2])) / 255.0;
                let alpha_mult = lum.clamp(0.0, 1.0);
                if alpha_mult <= 0.0 {
                    continue;
                }
                let masked_src = [
                    (f64::from(sp[0]) * alpha_mult).round() as u8,
                    (f64::from(sp[1]) * alpha_mult).round() as u8,
                    (f64::from(sp[2]) * alpha_mult).round() as u8,
                    (f64::from(sp[3]) * alpha_mult).round() as u8,
                ];
                let dst = &mut surface.pixels[py * w + px];
                *dst = src_over_premul(masked_src, *dst);
            }
        }
        return Ok(());
    }

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

    // Precompute container coverage if needed for fill, Lens, or PowerClip
    let need_container_cov =
        node.fill.is_some() || !node.powerclip_children.is_empty() || matches!(node.transparency, Some(TransparencyStyle::Lens { .. }));
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

    // 2. Draw object Fill, Lens, or Bitmap
    if let (Some(TransparencyStyle::Lens { kind }), Some(cov)) = (&node.transparency, &container_cov) {
        apply_lens_coverage(surface, cov, kind, local_bbox, local_to_dev, eff_opacity)?;
    } else if let ObjectKind::Bitmap { pixel_width, pixel_height, rgba_pixels, .. } = &node.kind {
        paint_bitmap(surface, *pixel_width, *pixel_height, rgba_pixels, local_bbox, local_to_dev, eff_opacity, container_cov.as_deref());
    } else if let (Some(fill), Some(cov)) = (&node.fill, &container_cov) {
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

fn apply_lens_coverage(
    surface: &mut Surface,
    cov: &[u8],
    kind: &LensKind,
    local_bbox: Rect,
    local_to_dev: Affine2,
    eff_opacity: f64,
) -> Result<(), RenderError> {
    let w = surface.width as usize;
    let h = surface.height as usize;
    if w == 0 || h == 0 {
        return Ok(());
    }
    let snapshot = surface.clone();
    let dev_center = local_to_dev.apply_point(local_bbox.center().unwrap_or(Point::ZERO))?;
    let cx = dev_center.x.get();
    let cy = dev_center.y.get();

    for py in 0..h {
        for px in 0..w {
            let c = cov[py * w + px];
            if c == 0 {
                continue;
            }
            let alpha = eff_opacity * (f64::from(c) / 16.0);
            let rgb = match kind {
                LensKind::Invert => {
                    let [sr, sg, sb, _] = snapshot.get_straight_rgba(px as u32, py as u32);
                    [255 - sr, 255 - sg, 255 - sb]
                }
                LensKind::Brighten { rate } => {
                    let [sr, sg, sb, _] = snapshot.get_straight_rgba(px as u32, py as u32);
                    let r_adj = rate.get().clamp(-1.0, 1.0);
                    let map_ch = |ch: u8| -> u8 {
                        let v = f64::from(ch) / 255.0;
                        let out = if r_adj >= 0.0 { v + (1.0 - v) * r_adj } else { v * (1.0 + r_adj) };
                        (out.clamp(0.0, 1.0) * 255.0).round() as u8
                    };
                    [map_ch(sr), map_ch(sg), map_ch(sb)]
                }
                LensKind::Tinted { color, rate } => {
                    let [sr, sg, sb, _] = snapshot.get_straight_rgba(px as u32, py as u32);
                    let lum = (0.2126 * f64::from(sr) + 0.7152 * f64::from(sg) + 0.0722 * f64::from(sb)) / 255.0;
                    let [tr, tg, tb] = color.to_srgb8();
                    let t = rate.get().clamp(0.0, 1.0);
                    let mix = |orig: u8, tint: u8| -> u8 {
                        let tinted = lum * f64::from(tint);
                        ((1.0 - t) * f64::from(orig) + t * tinted).round().clamp(0.0, 255.0) as u8
                    };
                    [mix(sr, tr), mix(sg, tg), mix(sb, tb)]
                }
                LensKind::Magnify { factor } => {
                    let f = factor.get().max(1.0);
                    let sx = (cx + ((px as f64) - cx) / f).round().clamp(0.0, (w - 1) as f64) as u32;
                    let sy = (cy + ((py as f64) - cy) / f).round().clamp(0.0, (h - 1) as f64) as u32;
                    let [sr, sg, sb, _] = snapshot.get_straight_rgba(sx, sy);
                    [sr, sg, sb]
                }
            };
            surface.blend_straight(px as u32, py as u32, rgb, alpha);
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
    let obj_scale = if outline.scale_with_object { obj_transform.determinant().abs().sqrt() } else { 1.0 };
    let width_dev = if outline.is_hairline() { 1.0 } else { outline.width_pt.get() * page_scale * obj_scale };

    if width_dev <= 1e-6 {
        return Ok(());
    }

    let dashed_storage;
    let effective_polys = if outline.dash_pattern.is_empty() {
        dev_polys
    } else {
        let scaled_dashes: Vec<f64> = outline.dash_pattern.iter().map(|d| (d.get() * page_scale * obj_scale).max(1.0)).collect();
        dashed_storage = dash_polylines(dev_polys, &scaled_dashes)?;
        &dashed_storage
    };

    let mut stroke_polys = expand_stroke_polylines(effective_polys, 0.5 * width_dev, outline.cap, outline.join, outline.miter_limit.get())?;
    let arrow_polys = build_arrowhead_polylines(dev_polys, 0.5 * width_dev, outline.start_arrow, outline.end_arrow)?;
    stroke_polys.extend(arrow_polys);
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

#[allow(clippy::too_many_arguments)]
fn paint_bitmap(
    surface: &mut Surface,
    pixel_width: u32,
    pixel_height: u32,
    rgba_pixels: &[u8],
    local_bbox: Rect,
    local_to_dev: Affine2,
    eff_opacity: f64,
    container_cov: Option<&[u8]>,
) {
    if pixel_width == 0 || pixel_height == 0 || surface.width == 0 || surface.height == 0 {
        return;
    }
    let expected_len = (pixel_width as usize) * (pixel_height as usize) * 4;
    if rgba_pixels.len() < expected_len {
        return;
    }
    let Ok(dev_to_local) = local_to_dev.invert() else {
        return;
    };

    let w = surface.width as usize;
    let h = surface.height as usize;
    let pw_f = pixel_width as f64;
    let ph_f = pixel_height as f64;
    let bw = local_bbox.width();
    let bh = local_bbox.height();
    if bw <= 0.0 || bh <= 0.0 {
        return;
    }

    let sample_texel = |ix: i32, iy: i32| -> [f64; 4] {
        if ix < 0 || ix >= pixel_width as i32 || iy < 0 || iy >= pixel_height as i32 {
            [0.0, 0.0, 0.0, 0.0]
        } else {
            let offset = ((iy as usize) * (pixel_width as usize) + (ix as usize)) * 4;
            if offset + 4 <= rgba_pixels.len() {
                let a = f64::from(rgba_pixels[offset + 3]) / 255.0;
                let r = (f64::from(rgba_pixels[offset]) / 255.0) * a;
                let g = (f64::from(rgba_pixels[offset + 1]) / 255.0) * a;
                let b = (f64::from(rgba_pixels[offset + 2]) / 255.0) * a;
                [r, g, b, a]
            } else {
                [0.0, 0.0, 0.0, 0.0]
            }
        }
    };

    for py in 0..h {
        let y_center = (py as f64) + 0.5;
        for px in 0..w {
            let cov = container_cov.map_or(16, |c| c[py * w + px]);
            if cov == 0 {
                continue;
            }
            let x_center = (px as f64) + 0.5;
            let Ok(dev_pt) = Point::new(x_center, y_center) else {
                continue;
            };
            let Ok(loc) = dev_to_local.apply_point(dev_pt) else {
                continue;
            };

            let lx = loc.x.get();
            let ly = loc.y.get();
            if lx < 0.0 || lx >= bw || ly < 0.0 || ly >= bh {
                continue;
            }

            let tx = (lx / bw) * pw_f - 0.5;
            let ty = (ly / bh) * ph_f - 0.5;

            let x0 = tx.floor() as i32;
            let y0 = ty.floor() as i32;
            let x1 = x0 + 1;
            let y1 = y0 + 1;

            let fx = (tx - tx.floor()).clamp(0.0, 1.0);
            let fy = (ty - ty.floor()).clamp(0.0, 1.0);

            let s00 = sample_texel(x0, y0);
            let s10 = sample_texel(x1, y0);
            let s01 = sample_texel(x0, y1);
            let s11 = sample_texel(x1, y1);

            let mut blended = [0.0; 4];
            for c in 0..4 {
                let top = s00[c] + fx * (s10[c] - s00[c]);
                let bot = s01[c] + fx * (s11[c] - s01[c]);
                blended[c] = top + fy * (bot - top);
            }

            let a = blended[3];
            if a <= 1e-4 {
                continue;
            }

            let r = ((blended[0] / a).clamp(0.0, 1.0) * 255.0).round() as u8;
            let g = ((blended[1] / a).clamp(0.0, 1.0) * 255.0).round() as u8;
            let b = ((blended[2] / a).clamp(0.0, 1.0) * 255.0).round() as u8;

            let final_alpha = a * eff_opacity * (f64::from(cov) / 16.0);
            surface.blend_straight(px as u32, py as u32, [r, g, b], final_alpha);
        }
    }
}

pub(crate) fn extract_local_path_and_bbox(kind: &ObjectKind) -> Result<Option<(Path, Rect)>, RenderError> {
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
        ObjectKind::ParagraphText(pt) => {
            let layout = coralcraft_doc::layout_paragraph_text(pt);
            let mut combined = Vec::new();
            for line in &layout.lines {
                if let Ok(origin) = Point::new(line.origin_x, line.origin_y) {
                    let line_path = artistic_text_to_path(&line.text, &pt.font_family, pt.font_size_pt.get(), origin);
                    combined.extend(line_path.elements);
                }
            }
            let px = pt.position.x.get();
            let py = pt.position.y.get();
            let bbox = Rect::new(px, py, px + pt.frame_width_pt.get().max(1.0), py + pt.frame_height_pt.get().max(1.0))?;
            if combined.is_empty() {
                return Ok(None);
            }
            let path = Path::from_elements(combined, FillRule::NonZero)?;
            Ok(Some((path, bbox)))
        }
        ObjectKind::Bitmap { pixel_width, pixel_height, dpi_x, dpi_y, .. } => {
            let w = (*pixel_width as f64 * 72.0 / dpi_x.get().max(1.0)).max(1.0);
            let h = (*pixel_height as f64 * 72.0 / dpi_y.get().max(1.0)).max(1.0);
            let bbox = Rect::new(0.0, 0.0, w, h)?;
            let p = RectSpec::sharp(bbox).to_path()?;
            Ok(Some((p, bbox)))
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

    #[test]
    fn bitmap_raster_rendering_and_svg_pdf_export() {
        let mut doc = Document::new_a4_portrait();
        doc.pages[0].size = Size::new(100.0, 100.0).unwrap();

        // 2x2 RGBA checkerboard: Top-left Red, Top-right Green, Bottom-left Blue, Bottom-right Yellow
        #[rustfmt::skip]
        let rgba = vec![
            255, 0, 0, 255,     0, 255, 0, 255,
            0, 0, 255, 255,     255, 255, 0, 255,
        ];

        let bmp_node = ObjectNode::new(
            ObjectId(99),
            Some("Checker Bitmap".to_owned()),
            ObjectKind::Bitmap { pixel_width: 2, pixel_height: 2, dpi_x: F64::new(72.0).unwrap(), dpi_y: F64::new(72.0).unwrap(), rgba_pixels: rgba },
            None,
            None,
        );
        doc.pages[0].layers[0].objects.push(bmp_node);

        // 1. Render raster surface at 1.0 scale
        let render_out = render_page(&doc, 0, &RenderOpts { scale: 1.0, transparent_background: true, export_mode: true }).unwrap();
        let p00 = render_out.surface.get_straight_rgba(0, 0);
        assert_eq!(p00[0], 255);
        assert_eq!(render_out.surface.get_straight_rgba(50, 50), [0, 0, 0, 0]);

        // 2. Export SVG and verify <image> element
        let svg = export_svg(&doc, 0).unwrap();
        assert!(svg.contains("<image id=\"obj-99\""));
        assert!(svg.contains("href=\"data:image/png;base64,"));
        assert!(svg.contains("preserveAspectRatio=\"none\""));

        // 3. Export PDF and verify inline image BI ... ID ... EI
        let pdf_bytes = export_pdf(&doc).unwrap();
        let pdf_str = String::from_utf8_lossy(&pdf_bytes);
        assert!(pdf_str.contains("BI\n/W 2\n/H 2\n/CS /DeviceRGB\n/BPC 8\n/F /AHx\nID\n"));
        assert!(pdf_str.contains("EI"));
    }

    #[test]
    fn fountain_transparency_raster_and_svg_mask() {
        let mut doc = Document::new_a4_portrait();
        doc.pages[0].size = Size::new(100.0, 40.0).unwrap();

        let rect = RectSpec::sharp(Rect::new(0.0, 0.0, 100.0, 40.0).unwrap());
        let mut node = ObjectNode::new(
            ObjectId(120),
            Some("Fountain Transparent Rect".to_owned()),
            ObjectKind::Rect(rect),
            Some(FillStyle::uniform(Color::rgb8(255, 0, 0))),
            None,
        );
        // Left (x=0) opaque (1.0), Right (x=99) transparent (0.0)
        node.transparency = Some(TransparencyStyle::fountain_linear(0.0, 1.0, 0.0));
        doc.pages[0].layers[0].objects.push(node);

        let out = render_page(&doc, 0, &RenderOpts { scale: 1.0, transparent_background: true, export_mode: true }).unwrap();
        let left_rgba = out.surface.get_straight_rgba(5, 20);
        let right_rgba = out.surface.get_straight_rgba(95, 20);
        assert!(left_rgba[3] > 220, "left side should be nearly opaque: {left_rgba:?}");
        assert!(right_rgba[3] < 40, "right side should be nearly transparent: {right_rgba:?}");

        let svg = export_svg(&doc, 0).unwrap();
        assert!(svg.contains("<mask id=\"mask-obj-120\" maskUnits=\"userSpaceOnUse\">"));
        assert!(svg.contains("mask=\"url(#mask-obj-120)\""));
    }

    #[test]
    fn paragraph_text_raster_svg_tspan_and_pdf_export() {
        use coralcraft_doc::{ParagraphTextSpec, TextAlignment};

        let mut doc = Document::new_a4_portrait();
        doc.pages[0].size = Size::new(200.0, 120.0).unwrap();
        let spec = ParagraphTextSpec::new(
            Point::new(10.0, 10.0).unwrap(),
            160.0,
            90.0,
            "CoralCraft clean-room paragraph text wrapping across multiple lines in a frame.\nSecond paragraph line.",
            "Arial",
            12.0,
            1.2,
            TextAlignment::Justify,
            1,
            0.0,
        )
        .unwrap();
        doc.pages[0].layers[0].objects.push(ObjectNode::new(
            ObjectId(140),
            Some("Body Copy".to_owned()),
            ObjectKind::ParagraphText(Box::new(spec)),
            Some(FillStyle::uniform(Color::rgb8(20, 30, 40))),
            None,
        ));

        let out = render_page(&doc, 0, &RenderOpts { scale: 1.0, transparent_background: true, export_mode: true }).unwrap();
        assert_eq!(out.surface.width, 200);
        assert_eq!(out.surface.height, 120);

        let svg = export_svg(&doc, 0).unwrap();
        assert!(svg.contains("<text id=\"obj-140\""));
        assert!(svg.contains("<tspan x=\""));
        assert!(svg.contains("word-spacing=\""));

        let pdf_bytes = export_pdf(&doc).unwrap();
        let pdf_str = String::from_utf8_lossy(&pdf_bytes);
        assert!(pdf_str.contains("Tw 1 0 0 -1"));
        assert!(pdf_str.contains("Tj ET"));
    }

    #[test]
    fn blend_spine_and_prepress_marks_pdf_svg_export() {
        let mut doc = Document::new_a4_portrait();
        doc.pages[0].size = Size::new(200.0, 200.0).unwrap();

        // Start object at (10, 10..30, 30), End object at (170, 170..190, 190)
        // Custom L-shaped spine going through (100, 20)
        let mut spine = Path::new();
        spine.move_to(Point::new(20.0, 20.0).unwrap()).unwrap();
        spine.line_to(Point::new(100.0, 20.0).unwrap()).unwrap();
        spine.line_to(Point::new(100.0, 100.0).unwrap()).unwrap();

        let mut start_node = ObjectNode::new(
            ObjectId(201),
            Some("Blend Start".to_owned()),
            ObjectKind::Rect(RectSpec::sharp(Rect::new(10.0, 10.0, 30.0, 30.0).unwrap())),
            Some(FillStyle::uniform(Color::rgb8(255, 0, 0))),
            None,
        );
        start_node.effects.push(VectorEffect::Blend { target_id: ObjectId(202), steps: 1, spine: Some(spine) });
        let end_node = ObjectNode::new(
            ObjectId(202),
            Some("Blend End".to_owned()),
            ObjectKind::Rect(RectSpec::sharp(Rect::new(90.0, 90.0, 110.0, 110.0).unwrap())),
            Some(FillStyle::uniform(Color::rgb8(0, 0, 255))),
            None,
        );
        doc.pages[0].layers[0].objects.push(start_node);
        doc.pages[0].layers[0].objects.push(end_node);

        // At u = 0.5 along the L-shaped spine (20,20)->(100,20)->(100,100), the midpoint is at (100, 20)!
        let out = render_page(&doc, 0, &RenderOpts { scale: 1.0, transparent_background: true, export_mode: true }).unwrap();
        let mid_px = out.surface.get_straight_rgba(100, 20);
        assert!(mid_px[3] > 200, "intermediate blend step along spine at (100, 20) should be painted: {mid_px:?}");

        // Prepress marks export
        let prepress = PrepressOptions { crop_marks: true, registration_marks: true, bleed_pt: F64::new(9.0).unwrap() };
        let svg_prepress = export_svg_with_prepress(&doc, 0, &prepress).unwrap();
        assert!(svg_prepress.contains("<g id=\"prepress-marks\""));
        assert!(svg_prepress.contains("class=\"crop-marks\""));
        assert!(svg_prepress.contains("class=\"registration-target\""));

        let pdf_prepress = export_pdf_with_prepress(&doc, &prepress).unwrap();
        let pdf_str = String::from_utf8_lossy(&pdf_prepress);
        assert!(pdf_str.contains("1 1 1 1 K"));
        assert!(pdf_str.contains("/TrimBox"));
        assert!(pdf_str.contains("/BleedBox"));
    }

    #[test]
    fn envelope_and_artistic_media_brush_effects_render_deformed_geometry() {
        let mut doc = Document::new_a4_portrait();
        doc.pages[0].size = Size::new(160.0, 120.0).unwrap();

        // 1. Envelope warped rectangle shifted into (80..140, 10..50)
        let mut env_rect = ObjectNode::new(
            ObjectId(301),
            Some("Warped Rect".to_owned()),
            ObjectKind::Rect(RectSpec::sharp(Rect::new(10.0, 10.0, 50.0, 50.0).unwrap())),
            Some(FillStyle::uniform(Color::rgb8(220, 40, 40))),
            None,
        );
        env_rect.effects.push(VectorEffect::Envelope {
            corners: [
                Point::new(80.0, 10.0).unwrap(),
                Point::new(140.0, 10.0).unwrap(),
                Point::new(140.0, 50.0).unwrap(),
                Point::new(80.0, 50.0).unwrap(),
            ],
        });
        doc.pages[0].layers[0].objects.push(env_rect);

        // 2. Artistic Media Brush stroke along y = 90 with start_width = 24 pt
        let mut spine = Path::new();
        spine.move_to(Point::new(20.0, 90.0).unwrap()).unwrap();
        spine.line_to(Point::new(140.0, 90.0).unwrap()).unwrap();
        let mut brush_obj = ObjectNode::new(
            ObjectId(302),
            Some("Calligraphic Stroke".to_owned()),
            ObjectKind::Path(spine),
            Some(FillStyle::uniform(Color::rgb8(20, 160, 80))),
            None,
        );
        brush_obj.effects.push(VectorEffect::ArtisticMediaBrush { start_width_pt: F64::new(24.0).unwrap(), end_width_pt: F64::new(4.0).unwrap() });
        doc.pages[0].layers[0].objects.push(brush_obj);

        let out = render_page(&doc, 0, &RenderOpts { scale: 1.0, transparent_background: true, export_mode: true }).unwrap();
        // Original rect center (30, 30) should be transparent, warped center (110, 30) should be red
        assert_eq!(out.surface.get_straight_rgba(30, 30)[3], 0);
        assert!(out.surface.get_straight_rgba(110, 30)[0] > 200);
        // Brush at (30, 82) is inside the thick start ribbon (90 ± 12)
        assert!(out.surface.get_straight_rgba(30, 82)[1] > 120);

        let eps = export_eps(&doc, 0).unwrap();
        assert!(eps.starts_with("%!PS-Adobe-3.0 EPSF-3.0"));
        assert!(eps.contains("%%BoundingBox: 0 0 160 120"));
        assert!(eps.contains("moveto"));
        assert!(eps.contains("fill"));

        let dxf = export_dxf(&doc, 0).unwrap();
        assert!(dxf.contains("$ACADVER\n1\nAC1009"));
        assert!(dxf.contains("POLYLINE"));
        assert!(dxf.contains("VERTEX"));
        assert!(dxf.contains("SEQEND"));

        let imp_spec = ImpositionSpec {
            sheet_width_pt: F64::new(612.0).unwrap(),
            sheet_height_pt: F64::new(792.0).unwrap(),
            rows: 2,
            cols: 2,
            margin_pt: F64::new(18.0).unwrap(),
            gutter_pt: F64::new(12.0).unwrap(),
            crop_marks: true,
        };
        let pdf_imp = export_pdf_imposed(&doc, &imp_spec).unwrap();
        assert!(pdf_imp.starts_with(b"%PDF-1.4"));
        assert!(pdf_imp.len() > 400);

        // Conical & Square fountain evaluation and dashed outline export across SVG, PDF, and EPS
        let t_conical = evaluate_fountain_t(FountainGeometry::Conical { center_x: F64::HALF, center_y: F64::HALF, angle_deg: F64::ZERO }, 0.0, 0.5);
        assert!((t_conical - 1.0).abs() < 1e-6);
        let t_square = evaluate_fountain_t(
            FountainGeometry::Square { center_x: F64::HALF, center_y: F64::HALF, radius: F64::HALF, angle_deg: F64::ZERO },
            0.75,
            0.5,
        );
        assert!((t_square - 0.5).abs() < 1e-6);

        let mut dashed_outl = OutlineStyle::with_width(Color::BLACK, F64::new(2.0).unwrap());
        dashed_outl.dash_pattern = vec![F64::new(6.0).unwrap(), F64::new(4.0).unwrap()];
        doc.pages[0].layers[0].objects.push(ObjectNode::new(
            ObjectId(303),
            Some("Dashed Box".to_owned()),
            ObjectKind::Rect(RectSpec::sharp(Rect::new(20.0, 20.0, 80.0, 60.0).unwrap())),
            None,
            Some(dashed_outl),
        ));
        let svg_dashed = export_svg(&doc, 0).unwrap();
        assert!(svg_dashed.contains("stroke-dasharray=\"6,4\""));
        let eps_dashed = export_eps(&doc, 0).unwrap();
        assert!(eps_dashed.contains("[6 4] 0 setdash"));
        let pdf_dashed = String::from_utf8_lossy(&export_pdf(&doc).unwrap()).into_owned();
        assert!(pdf_dashed.contains("[6 4] 0 d"));

        // Lens (Alt+F3) Invert & Magnify over warped red rectangle at (110, 30)
        let mut lens_obj = ObjectNode::new(
            ObjectId(304),
            Some("Invert Lens".to_owned()),
            ObjectKind::Rect(RectSpec::sharp(Rect::new(100.0, 20.0, 120.0, 40.0).unwrap())),
            None,
            None,
        );
        lens_obj.transparency = Some(TransparencyStyle::Lens { kind: LensKind::Invert });
        doc.pages[0].layers[0].objects.push(lens_obj);
        let out_lens = render_page(&doc, 0, &RenderOpts { scale: 1.0, transparent_background: true, export_mode: true }).unwrap();
        let inv_px = out_lens.surface.get_straight_rgba(110, 30);
        assert!(inv_px[1] > 180 && inv_px[2] > 180, "Invert lens over red should produce cyan: {inv_px:?}");

        // Two-Color Pattern Fill (Checkerboard) raster & SVG <pattern> export
        let pat_obj = ObjectNode::new(
            ObjectId(305),
            Some("Pattern Box".to_owned()),
            ObjectKind::Rect(RectSpec::sharp(Rect::new(0.0, 0.0, 32.0, 32.0).unwrap())),
            Some(FillStyle::TwoColorPattern {
                kind: coralcraft_doc::PatternKind::Checkerboard,
                fg_color: Color::rgb8(10, 20, 200),
                bg_color: Color::rgb8(240, 240, 240),
                tile_size_pt: F64::new(16.0).unwrap(),
            }),
            None,
        );
        doc.pages[0].layers[0].objects.push(pat_obj);
        let out_pat = render_page(&doc, 0, &RenderOpts { scale: 1.0, transparent_background: true, export_mode: true }).unwrap();
        // (4, 4) is u=0.25, v=0.25 -> bg; (12, 4) is u=0.75, v=0.25 -> fg
        assert_eq!(out_pat.surface.get_straight_rgba(4, 4)[0], 240);
        assert_eq!(out_pat.surface.get_straight_rgba(12, 4)[2], 200);
        let svg_pat = export_svg(&doc, 0).unwrap();
        assert!(svg_pat.contains("<pattern id=\"pat-obj-305\""));
        assert!(svg_pat.contains("fill=\"url(#pat-obj-305)\""));
    }
}
