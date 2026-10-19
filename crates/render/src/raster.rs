//! 4×4 supersampled scanline filler, stroke outline expander, and Linear/Radial Fountain shader.

use std::f64::consts::PI;

use coralcraft_color::{Color, interpolate_stops};
use coralcraft_doc::{FillStyle, FountainGeometry, LineCap, LineJoin};
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
    let mut cov = vec![0_u8; w * h];
    if w == 0 || h == 0 || polys.is_empty() {
        return cov;
    }

    let mut edges = Vec::new();
    for poly in polys {
        if poly.points.len() < 2 {
            continue;
        }
        for seg in poly.points.windows(2) {
            push_edge(&mut edges, seg[0], seg[1]);
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

    let mut crossings: Vec<(f64, i32)> = Vec::with_capacity(32);

    for py in 0..h {
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
                winding += w_delta;
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
                    accumulate_subpixel_span(&mut cov[py * w..(py + 1) * w], w, x0, x);
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
    let max_sub = (w * SS) as isize;
    // Subpixel center `(k + 0.5) / 4 >= x0` <=> `k >= ceil(4 * x0 - 0.5)`
    let k_start = ((x0 * (SS as f64) - 0.5).ceil() as isize).clamp(0, max_sub);
    let k_end = ((x1 * (SS as f64) - 0.5).ceil() as isize).clamp(0, max_sub);
    if k_start >= k_end {
        return;
    }
    for k in (k_start as usize)..(k_end as usize) {
        let px = k / SS;
        row_cov[px] = row_cov[px].saturating_add(1);
    }
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
            let mut left_side = Vec::with_capacity(n * 2);
            let mut right_side = Vec::with_capacity(n * 2);
            for i in 0..n {
                let p_prev = pts[(i + n - 1) % n];
                let p_curr = pts[i];
                let p_next = pts[(i + 1) % n];
                emit_join(&mut left_side, p_prev, p_curr, p_next, half_w, join, miter_lim);
                emit_join(&mut right_side, p_next, p_curr, p_prev, half_w, join, miter_lim);
            }
            right_side.reverse();
            out.push(to_closed_polyline(&left_side)?);
            out.push(to_closed_polyline(&right_side)?);
        } else {
            let n = pts.len();
            let mut contour = Vec::with_capacity(n * 4 + 8);
            // Forward pass along left side
            let (nx0, ny0) = left_normal(pts[0], pts[1]);
            contour.push((pts[0].0 + nx0 * half_w, pts[0].1 + ny0 * half_w));
            for i in 1..(n - 1) {
                emit_join(&mut contour, pts[i - 1], pts[i], pts[i + 1], half_w, join, miter_lim);
            }
            let (nx_last, ny_last) = left_normal(pts[n - 2], pts[n - 1]);
            contour.push((pts[n - 1].0 + nx_last * half_w, pts[n - 1].1 + ny_last * half_w));

            // End cap at `pts[n - 1]`
            emit_cap(&mut contour, pts[n - 2], pts[n - 1], half_w, cap);

            // Backward pass along right side
            contour.push((pts[n - 1].0 - nx_last * half_w, pts[n - 1].1 - ny_last * half_w));
            for i in (1..(n - 1)).rev() {
                emit_join(&mut contour, pts[i + 1], pts[i], pts[i - 1], half_w, join, miter_lim);
            }
            contour.push((pts[0].0 - nx0 * half_w, pts[0].1 - ny0 * half_w));

            // Start cap at `pts[0]`
            emit_cap(&mut contour, pts[1], pts[0], half_w, cap);

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
                    let cov = coverage_16[py * w + px];
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
                    let cov = coverage_16[py * w + px];
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
        FillStyle::Unknown { tag, .. } => {
            warnings.push(RenderWarning::UnknownFillFallback { object_id, tag: tag.display_ascii() });
            let fallback_rgb = Color::rgb8(160, 160, 160).to_srgb8();
            for py in 0..h {
                for px in 0..w {
                    let cov = coverage_16[py * w + px];
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
    }
}
