//! 4×4 supersampled scanline filler, stroke outline expander, and Linear/Radial Fountain shader.

use std::f64::consts::PI;

use coralcraft_color::{Color, interpolate_stops};
use coralcraft_doc::{ArrowheadKind, FillStyle, FountainGeometry, LineCap, LineJoin, PatternKind};
use coralcraft_geom::{Affine2, EPS, FillRule, GeomError, Point, Polyline, Rect};

use crate::surface::{RenderError, RenderWarning, Surface};

/// Supersampling factor per axis (`4×4 = 16` subpixel samples per device pixel).
pub const SS: usize = 4;

#[derive(Clone, Copy, Debug)]
struct Edge {
    y_min: f64,
    y_max: f64,
    x_at_ymin: f64,
    inv_slope: f64,
    winding: i32,
}

/// Rasterizes a slice of device-space [`Polyline`]s into a `width * height` coverage buffer
/// where each byte is in `0..=16` (`covered_subpixel_centers` out of `4×4`).
#[must_use]
pub fn rasterize_coverage(polys: &[Polyline], fill_rule: FillRule, width: u32, height: u32) -> Vec<u8> {
    let w = width as usize;
    let h = height as usize;
    let mut cov = vec![0_u8; w.saturating_mul(h)];
    if w == 0 || h == 0 || polys.is_empty() {
        return cov;
    }

    let mut edges = Vec::new();
    for poly in polys {
        if poly.points.len() < 2 {
            continue;
        }
        for seg in poly.points.windows(2) {
            if let [p0, p1] = *seg {
                push_edge(&mut edges, p0, p1);
            }
        }
        // Implicitly close the contour for fill coverage
        if let (Some(&last), Some(&first)) = (poly.points.last(), poly.points.first())
            && last != first
        {
            push_edge(&mut edges, last, first);
        }
    }

    if edges.is_empty() {
        return cov;
    }

    let mut global_y_min = f64::INFINITY;
    let mut global_y_max = f64::NEG_INFINITY;
    for e in &edges {
        global_y_min = global_y_min.min(e.y_min);
        global_y_max = global_y_max.max(e.y_max);
    }
    let py_start = (global_y_min.floor().max(0.0) as usize).min(h);
    let py_end = (global_y_max.ceil().max(0.0) as usize).min(h);

    let mut crossings: Vec<(f64, i32)> = Vec::with_capacity(32);

    for py in py_start..py_end {
        let row_y_min = py as f64;

        for sy in 0..SS {
            let y_sub = row_y_min + ((sy as f64) + 0.5) / (SS as f64);
            crossings.clear();

            for e in &edges {
                // Half-open interval `[y_min, y_max)` avoids double-counting shared vertices
                if y_sub >= e.y_min && y_sub < e.y_max {
                    let x = e.x_at_ymin + (y_sub - e.y_min) * e.inv_slope;
                    if x.is_finite() {
                        crossings.push((x, e.winding));
                    }
                }
            }

            if crossings.len() < 2 {
                continue;
            }
            crossings.sort_by(|a, b| a.0.total_cmp(&b.0));

            let mut winding = 0_i32;
            let mut span_start: Option<f64> = None;

            for &(x, w_delta) in &crossings {
                let was_inside = match fill_rule {
                    FillRule::NonZero => winding != 0,
                    FillRule::EvenOdd => (winding & 1) != 0,
                };
                winding = winding.saturating_add(w_delta);
                let is_inside = match fill_rule {
                    FillRule::NonZero => winding != 0,
                    FillRule::EvenOdd => (winding & 1) != 0,
                };

                if !was_inside && is_inside {
                    span_start = Some(x);
                } else if was_inside
                    && !is_inside
                    && let Some(x0) = span_start.take()
                {
                    let row_start = py.saturating_mul(w);
                    let row_end = py.saturating_add(1).saturating_mul(w);
                    if let Some(row_cov) = cov.get_mut(row_start..row_end) {
                        accumulate_subpixel_span(row_cov, w, x0, x);
                    }
                }
            }
        }
    }

    cov
}

fn push_edge(edges: &mut Vec<Edge>, p0: Point, p1: Point) {
    let x0 = p0.x.get();
    let y0 = p0.y.get();
    let x1 = p1.x.get();
    let y1 = p1.y.get();
    let dy = y1 - y0;
    if dy.abs() <= 1e-12 {
        return;
    }
    let (y_min, y_max, x_at_ymin, winding) = if y0 < y1 { (y0, y1, x0, 1) } else { (y1, y0, x1, -1) };
    let inv_slope = (x1 - x0) / dy;
    edges.push(Edge { y_min, y_max, x_at_ymin, inv_slope, winding });
}

/// Accumulates subpixel coverage across `w` pixels on one subpixel scanline for interval `[x0, x1)`.
/// Subpixel `k` (`0 .. 4*w`) has center at `(k + 0.5) / 4.0`.
fn accumulate_subpixel_span(row_cov: &mut [u8], w: usize, x0: f64, x1: f64) {
    let max_sub = w.saturating_mul(SS) as isize;
    // Subpixel center `(k + 0.5) / 4 >= x0` <=> `k >= ceil(4 * x0 - 0.5)`
    let k_start = ((x0 * (SS as f64) - 0.5).ceil() as isize).clamp(0, max_sub);
    let k_end = ((x1 * (SS as f64) - 0.5).ceil() as isize).clamp(0, max_sub);
    if k_start >= k_end {
        return;
    }
    for k in (k_start as usize)..(k_end as usize) {
        let px = k / SS;
        if let Some(slot) = row_cov.get_mut(px) {
            *slot = slot.saturating_add(1);
        }
    }
}

/// Splits device-space or local-space polylines into open dashed sub-polylines according to `dash_pattern`.
pub fn dash_polylines(polys: &[Polyline], dash_pattern: &[f64]) -> Result<Vec<Polyline>, GeomError> {
    let mut pat: Vec<f64> = dash_pattern.iter().copied().filter(|d| d.is_finite() && *d > 1e-6).take(32).collect();
    if pat.is_empty() {
        return Ok(polys.to_vec());
    }
    if pat.len() % 2 == 1 {
        let copy = pat.clone();
        pat.extend(copy);
    }

    let mut out = Vec::new();
    for poly in polys {
        if poly.points.len() < 2 {
            continue;
        }
        let mut pts = poly.points.clone();
        if poly.closed
            && let (Some(&first), Some(&last)) = (pts.first(), pts.last())
            && first.distance(last) > EPS
        {
            pts.push(first);
        }

        let mut pat_idx = 0usize;
        let mut rem = pat.first().copied().unwrap_or(1.0);
        let mut pen_down = true;
        let mut current_dash: Vec<Point> = pts.first().copied().into_iter().collect();

        for seg in pts.windows(2) {
            let [p0, p1] = *seg else { continue };
            let dx = p1.x.get() - p0.x.get();
            let dy = p1.y.get() - p0.y.get();
            let seg_len = dx.hypot(dy);
            if seg_len <= EPS {
                continue;
            }
            let ux = dx / seg_len;
            let uy = dy / seg_len;
            let mut dist = 0.0;

            while dist + rem < seg_len - EPS {
                dist += rem;
                let cut = Point::new(p0.x.get() + ux * dist, p0.y.get() + uy * dist)?;
                if pen_down {
                    current_dash.push(cut);
                    if current_dash.len() >= 2 {
                        out.push(Polyline { points: std::mem::take(&mut current_dash), closed: false });
                    } else {
                        current_dash.clear();
                    }
                }
                pat_idx = pat_idx.saturating_add(1).checked_rem(pat.len()).unwrap_or(0);
                rem = pat.get(pat_idx).copied().unwrap_or(1.0);
                pen_down = pat_idx.is_multiple_of(2);
                if pen_down {
                    current_dash.push(cut);
                }
            }

            let left_on_seg = seg_len - dist;
            rem -= left_on_seg;
            if pen_down {
                current_dash.push(p1);
            }
        }

        if pen_down && current_dash.len() >= 2 {
            out.push(Polyline { points: current_dash, closed: false });
        }
    }

    Ok(out)
}

/// Expands device-space or local-space polylines into closed stroke-outline polylines
/// with half-width `half_w`, `cap`, `join`, and `miter_limit`.
pub fn expand_stroke_polylines(polys: &[Polyline], half_w: f64, cap: LineCap, join: LineJoin, miter_limit: f64) -> Result<Vec<Polyline>, GeomError> {
    if !half_w.is_finite() || half_w <= 1e-9 {
        return Ok(Vec::new());
    }
    let miter_lim = miter_limit.max(1.0);
    let mut out = Vec::new();

    for poly in polys {
        let mut pts: Vec<(f64, f64)> = Vec::with_capacity(poly.points.len());
        for p in &poly.points {
            let xy = (p.x.get(), p.y.get());
            if pts.last().is_none_or(|prev| (prev.0 - xy.0).hypot(prev.1 - xy.1) > EPS) {
                pts.push(xy);
            }
        }
        if poly.closed
            && pts.len() >= 2
            && let (Some(&first), Some(&last)) = (pts.first(), pts.last())
            && (first.0 - last.0).hypot(first.1 - last.1) <= EPS
        {
            pts.pop();
        }
        if pts.len() < 2 {
            continue;
        }

        if poly.closed && pts.len() >= 3 {
            let n = pts.len();
            let mut left_side = Vec::with_capacity(n.saturating_mul(2));
            let mut right_side = Vec::with_capacity(n.saturating_mul(2));
            for i in 0..n {
                let prev_idx = i.saturating_add(n.saturating_sub(1)).checked_rem(n).unwrap_or(0);
                let next_idx = i.saturating_add(1).checked_rem(n).unwrap_or(0);
                if let (Some(&p_prev), Some(&p_curr), Some(&p_next)) = (pts.get(prev_idx), pts.get(i), pts.get(next_idx)) {
                    emit_join(&mut left_side, p_prev, p_curr, p_next, half_w, join, miter_lim);
                    emit_join(&mut right_side, p_next, p_curr, p_prev, half_w, join, miter_lim);
                }
            }
            right_side.reverse();
            out.push(to_closed_polyline(&left_side)?);
            out.push(to_closed_polyline(&right_side)?);
        } else {
            let n = pts.len();
            let mut contour = Vec::with_capacity(n.saturating_mul(4).saturating_add(8));
            let (Some(&pt0), Some(&pt1)) = (pts.first(), pts.get(1)) else {
                continue;
            };
            let (Some(&pt_penult), Some(&pt_last)) = (pts.get(n.saturating_sub(2)), pts.last()) else {
                continue;
            };
            // Forward pass along left side
            let (nx0, ny0) = left_normal(pt0, pt1);
            contour.push((pt0.0 + nx0 * half_w, pt0.1 + ny0 * half_w));
            for win in pts.windows(3) {
                if let [p_prev, p_curr, p_next] = *win {
                    emit_join(&mut contour, p_prev, p_curr, p_next, half_w, join, miter_lim);
                }
            }
            let (nx_last, ny_last) = left_normal(pt_penult, pt_last);
            contour.push((pt_last.0 + nx_last * half_w, pt_last.1 + ny_last * half_w));

            // End cap at `pts[n - 1]`
            emit_cap(&mut contour, pt_penult, pt_last, half_w, cap);

            // Backward pass along right side
            contour.push((pt_last.0 - nx_last * half_w, pt_last.1 - ny_last * half_w));
            for win in pts.windows(3).rev() {
                if let [p_prev, p_curr, p_next] = *win {
                    emit_join(&mut contour, p_next, p_curr, p_prev, half_w, join, miter_lim);
                }
            }
            contour.push((pt0.0 - nx0 * half_w, pt0.1 - ny0 * half_w));

            // Start cap at `pts[0]`
            emit_cap(&mut contour, pt1, pt0, half_w, cap);

            out.push(to_closed_polyline(&contour)?);
        }
    }

    Ok(out)
}

fn left_normal(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let len = dx.hypot(dy);
    if len <= EPS { (0.0, -1.0) } else { (-dy / len, dx / len) }
}

fn emit_cap(out: &mut Vec<(f64, f64)>, from: (f64, f64), at: (f64, f64), half_w: f64, cap: LineCap) {
    let dx = at.0 - from.0;
    let dy = at.1 - from.1;
    let len = dx.hypot(dy);
    if len <= EPS {
        return;
    }
    let ux = dx / len;
    let uy = dy / len;
    let nx = -uy;
    let ny = ux;

    match cap {
        LineCap::Butt => {}
        LineCap::Square => {
            out.push((at.0 + (nx + ux) * half_w, at.1 + (ny + uy) * half_w));
            out.push((at.0 + (-nx + ux) * half_w, at.1 + (-ny + uy) * half_w));
        }
        LineCap::Round => {
            let base_angle = ny.atan2(nx);
            let steps = 8;
            for step in 1..steps {
                let a = base_angle - PI * (f64::from(step) / f64::from(steps));
                out.push((at.0 + a.cos() * half_w, at.1 + a.sin() * half_w));
            }
        }
    }
}

fn emit_join(out: &mut Vec<(f64, f64)>, prev: (f64, f64), curr: (f64, f64), next: (f64, f64), half_w: f64, join: LineJoin, miter_limit: f64) {
    let n0 = left_normal(prev, curr);
    let n1 = left_normal(curr, next);
    let p0 = (curr.0 + n0.0 * half_w, curr.1 + n0.1 * half_w);
    let p1 = (curr.0 + n1.0 * half_w, curr.1 + n1.1 * half_w);

    let bx = n0.0 + n1.0;
    let by = n0.1 + n1.1;
    let b_len_sq = bx * bx + by * by;

    match join {
        LineJoin::Bevel => {
            out.push(p0);
            out.push(p1);
        }
        LineJoin::Round => {
            out.push(p0);
            let a0 = n0.1.atan2(n0.0);
            let mut da = n1.1.atan2(n1.0) - a0;
            while da <= -PI {
                da += 2.0 * PI;
            }
            while da > PI {
                da -= 2.0 * PI;
            }
            let steps = 6;
            for s in 1..steps {
                let a = a0 + da * (f64::from(s) / f64::from(steps));
                out.push((curr.0 + a.cos() * half_w, curr.1 + a.sin() * half_w));
            }
            out.push(p1);
        }
        LineJoin::Miter => {
            if b_len_sq > 1e-9 {
                let miter_scale = 2.0 / b_len_sq;
                let mx = bx * miter_scale;
                let my = by * miter_scale;
                let miter_ratio = mx.hypot(my);
                if miter_ratio <= miter_limit {
                    out.push((curr.0 + mx * half_w, curr.1 + my * half_w));
                    return;
                }
            }
            out.push(p0);
            out.push(p1);
        }
    }
}

fn to_closed_polyline(pts: &[(f64, f64)]) -> Result<Polyline, GeomError> {
    let mut points = Vec::with_capacity(pts.len());
    for &(x, y) in pts {
        points.push(Point::new(x, y)?);
    }
    Ok(Polyline { points, closed: true })
}

/// Builds closed arrowhead marker polylines at the start and/or end of open polylines.
pub fn build_arrowhead_polylines(
    polys: &[Polyline],
    half_w: f64,
    start_arrow: ArrowheadKind,
    end_arrow: ArrowheadKind,
) -> Result<Vec<Polyline>, GeomError> {
    if start_arrow == ArrowheadKind::None && end_arrow == ArrowheadKind::None {
        return Ok(Vec::new());
    }
    let scale = (half_w * 2.0).max(2.0);
    let mut out = Vec::new();
    for poly in polys {
        if poly.closed || poly.points.len() < 2 {
            continue;
        }
        if start_arrow != ArrowheadKind::None
            && let (Some(&p_tip), Some(&p_from)) = (poly.points.first(), poly.points.get(1))
            && let Some(marker) = single_arrowhead(p_from, p_tip, scale, start_arrow)?
        {
            out.push(marker);
        }
        if end_arrow != ArrowheadKind::None {
            let n = poly.points.len();
            if let (Some(&p_tip), Some(&p_from)) = (poly.points.last(), poly.points.get(n.saturating_sub(2)))
                && let Some(marker) = single_arrowhead(p_from, p_tip, scale, end_arrow)?
            {
                out.push(marker);
            }
        }
    }
    Ok(out)
}

fn single_arrowhead(from: Point, tip: Point, stroke_w: f64, kind: ArrowheadKind) -> Result<Option<Polyline>, GeomError> {
    let dx = tip.x.get() - from.x.get();
    let dy = tip.y.get() - from.y.get();
    let len = dx.hypot(dy);
    if len <= EPS {
        return Ok(None);
    }
    let ux = dx / len;
    let uy = dy / len;
    let nx = -uy;
    let ny = ux;
    let aw = stroke_w * 3.0;
    let ah = stroke_w * 1.5;
    let tx = tip.x.get();
    let ty = tip.y.get();

    match kind {
        ArrowheadKind::None => Ok(None),
        ArrowheadKind::Triangle | ArrowheadKind::OpenArrow => {
            let pts = [(tx, ty), (tx - aw * ux + ah * nx, ty - aw * uy + ah * ny), (tx - aw * ux - ah * nx, ty - aw * uy - ah * ny)];
            Ok(Some(to_closed_polyline(&pts)?))
        }
        ArrowheadKind::Diamond => {
            let half_l = aw * 0.6;
            let pts = [
                (tx, ty),
                (tx - half_l * ux + ah * nx, ty - half_l * uy + ah * ny),
                (tx - 2.0 * half_l * ux, ty - 2.0 * half_l * uy),
                (tx - half_l * ux - ah * nx, ty - half_l * uy - ah * ny),
            ];
            Ok(Some(to_closed_polyline(&pts)?))
        }
        ArrowheadKind::Circle => {
            let r = ah;
            let mut pts = Vec::with_capacity(12);
            for k in 0..12 {
                let ang = (f64::from(k) / 12.0) * std::f64::consts::TAU;
                pts.push((tx + r * ang.cos(), ty + r * ang.sin()));
            }
            Ok(Some(to_closed_polyline(&pts)?))
        }
    }
}

/// Shades a 16-level coverage buffer onto `surface` using `fill` (Uniform or Linear/Radial Fountain Fill).
#[allow(clippy::too_many_arguments)]
pub fn shade_coverage(
    surface: &mut Surface,
    coverage_16: &[u8],
    fill: &FillStyle,
    local_bbox: Rect,
    local_to_dev: Affine2,
    opacity: f64,
    warnings: &mut Vec<RenderWarning>,
    object_id: u64,
) -> Result<(), RenderError> {
    let w = surface.width as usize;
    let h = surface.height as usize;
    let opacity = opacity.clamp(0.0, 1.0);
    if opacity <= 0.0 {
        return Ok(());
    }

    match fill {
        FillStyle::Uniform { color } => {
            let rgb = color.to_srgb8();
            for py in 0..h {
                for px in 0..w {
                    let cov = coverage_16.get(py.saturating_mul(w).saturating_add(px)).copied().unwrap_or(0);
                    if cov > 0 {
                        let a = opacity * (f64::from(cov) / 16.0);
                        surface.blend_straight(px as u32, py as u32, rgb, a);
                    }
                }
            }
        }
        FillStyle::Fountain { geometry, blend_mode, stops } => {
            let dev_to_local = local_to_dev.invert().unwrap_or(Affine2::IDENTITY);
            let bx0 = local_bbox.x0.get();
            let by0 = local_bbox.y0.get();
            let bw = local_bbox.width().max(1e-6);
            let bh = local_bbox.height().max(1e-6);

            for py in 0..h {
                for px in 0..w {
                    let cov = coverage_16.get(py.saturating_mul(w).saturating_add(px)).copied().unwrap_or(0);
                    if cov == 0 {
                        continue;
                    }
                    let dev_pt = Point::new((px as f64) + 0.5, (py as f64) + 0.5)?;
                    let loc_pt = dev_to_local.apply_point(dev_pt)?;
                    let u = (loc_pt.x.get() - bx0) / bw;
                    let v = (loc_pt.y.get() - by0) / bh;

                    let t = evaluate_fountain_t(*geometry, u, v);
                    let rgb = interpolate_stops(stops, *blend_mode, t)?;
                    let a = opacity * (f64::from(cov) / 16.0);
                    surface.blend_straight(px as u32, py as u32, rgb, a);
                }
            }
        }
        FillStyle::TwoColorPattern { kind, fg_color, bg_color, tile_size_pt } => {
            let dev_to_local = local_to_dev.invert().unwrap_or(Affine2::IDENTITY);
            let ts = tile_size_pt.get().max(1.0);
            let fg_rgb = fg_color.to_srgb8();
            let bg_rgb = bg_color.to_srgb8();
            for py in 0..h {
                for px in 0..w {
                    let cov = coverage_16.get(py.saturating_mul(w).saturating_add(px)).copied().unwrap_or(0);
                    if cov == 0 {
                        continue;
                    }
                    let dev_pt = Point::new((px as f64) + 0.5, (py as f64) + 0.5)?;
                    let loc_pt = dev_to_local.apply_point(dev_pt)?;
                    let u = (loc_pt.x.get() / ts).rem_euclid(1.0);
                    let v = (loc_pt.y.get() / ts).rem_euclid(1.0);
                    let is_fg = match kind {
                        PatternKind::Checkerboard => (u < 0.5) ^ (v < 0.5),
                        PatternKind::PolkaDot => (u - 0.5).hypot(v - 0.5) <= 0.35,
                        PatternKind::HorizontalStripe => v < 0.5,
                        PatternKind::VerticalStripe => u < 0.5,
                        PatternKind::DiagonalCrosshatch => (u - v).abs() < 0.15 || (u + v - 1.0).abs() < 0.15,
                    };
                    let rgb = if is_fg { fg_rgb } else { bg_rgb };
                    let a = opacity * (f64::from(cov) / 16.0);
                    surface.blend_straight(px as u32, py as u32, rgb, a);
                }
            }
        }
        FillStyle::Unknown { tag, .. } => {
            warnings.push(RenderWarning::UnknownFillFallback { object_id, tag: tag.display_ascii() });
            let fallback_rgb = Color::rgb8(160, 160, 160).to_srgb8();
            for py in 0..h {
                for px in 0..w {
                    let cov = coverage_16.get(py.saturating_mul(w).saturating_add(px)).copied().unwrap_or(0);
                    if cov > 0 {
                        let a = opacity * (f64::from(cov) / 16.0);
                        surface.blend_straight(px as u32, py as u32, fallback_rgb, a);
                    }
                }
            }
        }
    }

    Ok(())
}

/// Evaluates normalized gradient parameter `t in [0.0, 1.0]` for Linear or Radial Fountain geometry
/// given unit-square coordinates `(u, v)`.
#[must_use]
pub fn evaluate_fountain_t(geometry: FountainGeometry, u: f64, v: f64) -> f64 {
    match geometry {
        FountainGeometry::Linear { angle_deg, edge_pad } => {
            let rad = angle_deg.get().to_radians();
            let (s, c) = rad.sin_cos();
            // Project `(u - 0.5, v - 0.5)` onto the rotated gradient axis
            let proj = (u - 0.5) * c + (v - 0.5) * s + 0.5;
            let pad = edge_pad.get().clamp(0.0, 0.49);
            let span = (1.0 - 2.0 * pad).max(1e-6);
            ((proj - pad) / span).clamp(0.0, 1.0)
        }
        FountainGeometry::Radial { center_x, center_y, radius, focal_x, focal_y } => {
            // Distance from focal point (or center) normalized by radius
            let fx = if (focal_x.get() - center_x.get()).abs() > 1e-9 || (focal_y.get() - center_y.get()).abs() > 1e-9 {
                focal_x.get()
            } else {
                center_x.get()
            };
            let fy = if (focal_x.get() - center_x.get()).abs() > 1e-9 || (focal_y.get() - center_y.get()).abs() > 1e-9 {
                focal_y.get()
            } else {
                center_y.get()
            };
            let r = radius.get().max(1e-6);
            let dist = (u - fx).hypot(v - fy);
            (dist / r).clamp(0.0, 1.0)
        }
        FountainGeometry::Conical { center_x, center_y, angle_deg } => {
            let dx = u - center_x.get();
            let dy = v - center_y.get();
            let theta = (dy.atan2(dx) - angle_deg.get().to_radians()).rem_euclid(std::f64::consts::TAU);
            let norm = theta / std::f64::consts::TAU;
            (1.0 - (2.0 * norm - 1.0).abs()).clamp(0.0, 1.0)
        }
        FountainGeometry::Square { center_x, center_y, radius, angle_deg } => {
            let dx = u - center_x.get();
            let dy = v - center_y.get();
            let (s, c) = (-angle_deg.get().to_radians()).sin_cos();
            let rx = dx * c - dy * s;
            let ry = dx * s + dy * c;
            let r = radius.get().max(1e-6);
            (rx.abs().max(ry.abs()) / r).clamp(0.0, 1.0)
        }
    }
}
