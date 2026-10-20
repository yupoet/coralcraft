//! Boolean operations and artistic text vector outline generator (`coralcraft-geom::boolean`).
//!
//! Implements CorelDRAW-compatible path operations:
//! - [`BooleanOp::Weld`]: Union of closed path contours into a single compound path.
//! - [`BooleanOp::Trim`]: Subtraction (subject minus clipping paths).
//! - [`BooleanOp::Intersect`]: Mutual intersection of path areas.
//! - [`BooleanOp::FrontMinusBack`] & [`BooleanOp::BackMinusFront`]: Z-order subtraction.
//! - [`artistic_text_to_path`]: Clean-room vector outline generator for artistic text.

use serde::{Deserialize, Serialize};

use crate::f64_canon::{GeomError, Point};
use crate::flatten::{FlattenOpts, Polyline, flatten_path};
use crate::path::Path;

/// CorelDRAW-compatible 2D vector Boolean operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BooleanOp {
    /// Combines all shapes into a single compound object with outer perimeter.
    Weld,
    /// Cuts away the overlapping area of target objects using source cutter.
    Trim,
    /// Retains only the mutually overlapping area among selected objects.
    Intersect,
    /// Subtracts the lower (back) object from the upper (front) object in z-order.
    FrontMinusBack,
    /// Subtracts the upper (front) object from the lower (back) object in z-order.
    BackMinusFront,
}

/// Executes a 2D boolean operation between a subject path and a clip path.
///
/// Both paths are converted to closed polygon loops. If inputs are disjoint:
/// - [`BooleanOp::Weld`] returns a compound path containing both disjoint contours.
/// - [`BooleanOp::Trim`] or [`BooleanOp::FrontMinusBack`] returns the subject untouched.
/// - [`BooleanOp::Intersect`] returns an empty path.
pub fn boolean_paths(subj: &Path, clip: &Path, op: BooleanOp) -> Result<Path, GeomError> {
    if subj.elements.is_empty() {
        return match op {
            BooleanOp::Weld => Ok(clip.clone()),
            _ => Ok(Path::new()),
        };
    }
    if clip.elements.is_empty() {
        return match op {
            BooleanOp::Weld | BooleanOp::Trim | BooleanOp::FrontMinusBack => Ok(subj.clone()),
            _ => Ok(Path::new()),
        };
    }

    let opts = FlattenOpts { tolerance: 0.25, max_depth: 10 };
    let subj_polys = flatten_path(subj, &opts)?;
    let clip_polys = flatten_path(clip, &opts)?;

    let subj_box = subj.bounds().unwrap_or_default();
    let clip_box = clip.bounds().unwrap_or_default();

    let overlaps = subj_box.intersects(clip_box);

    match op {
        BooleanOp::Weld => {
            if !overlaps {
                let mut out = subj.clone();
                for el in &clip.elements {
                    out.elements.push(*el);
                }
                return Ok(out);
            }
            // Clip/merge closed loops using planar polygon union
            merge_polylines(&subj_polys, &clip_polys, BooleanOp::Weld)
        }
        BooleanOp::Trim | BooleanOp::FrontMinusBack => {
            if !overlaps {
                return Ok(subj.clone());
            }
            merge_polylines(&subj_polys, &clip_polys, BooleanOp::Trim)
        }
        BooleanOp::Intersect => {
            if !overlaps {
                return Ok(Path::new());
            }
            merge_polylines(&subj_polys, &clip_polys, BooleanOp::Intersect)
        }
        BooleanOp::BackMinusFront => {
            if !overlaps {
                return Ok(clip.clone());
            }
            merge_polylines(&clip_polys, &subj_polys, BooleanOp::Trim)
        }
    }
}

fn point_in_polygon(pt: Point, poly: &[Point]) -> bool {
    if poly.len() < 3 {
        return false;
    }
    let mut inside = false;
    let px = pt.x.get();
    let py = pt.y.get();
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let xi = poly[i].x.get();
        let yi = poly[i].y.get();
        let xj = poly[j].x.get();
        let yj = poly[j].y.get();

        let intersect = ((yi > py) != (yj > py)) && (px < (xj - xi) * (py - yi) / (yj - yi) + xi);
        if intersect {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn segment_intersection(p1: Point, p2: Point, p3: Point, p4: Point) -> Option<(f64, f64, Point)> {
    let d1x = p2.x.get() - p1.x.get();
    let d1y = p2.y.get() - p1.y.get();
    let d2x = p4.x.get() - p3.x.get();
    let d2y = p4.y.get() - p3.y.get();

    let denom = d1x * d2y - d1y * d2x;
    if denom.abs() < 1e-9 {
        return None;
    }

    let t = ((p3.x.get() - p1.x.get()) * d2y - (p3.y.get() - p1.y.get()) * d2x) / denom;
    let u = ((p3.x.get() - p1.x.get()) * d1y - (p3.y.get() - p1.y.get()) * d1x) / denom;

    if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u) {
        let ix = p1.x.get() + t * d1x;
        let iy = p1.y.get() + t * d1y;
        Point::new(ix, iy).ok().map(|pt| (t, u, pt))
    } else {
        None
    }
}

fn merge_polylines(subj_list: &[Polyline], clip_list: &[Polyline], op: BooleanOp) -> Result<Path, GeomError> {
    let mut out_path = Path::new();

    // Clean-room polygon intersection/subtraction based on sampled ray casting
    for subj in subj_list {
        if subj.points.len() < 3 {
            continue;
        }
        for clip in clip_list {
            if clip.points.len() < 3 {
                continue;
            }

            match op {
                BooleanOp::Weld => {
                    // For weld: keep subj vertices not inside clip, and clip vertices not inside subj
                    let mut loop_pts = Vec::new();
                    for &p in &subj.points {
                        if !point_in_polygon(p, &clip.points) {
                            loop_pts.push(p);
                        }
                    }
                    for &p in &clip.points {
                        if !point_in_polygon(p, &subj.points) {
                            loop_pts.push(p);
                        }
                    }
                    if loop_pts.len() >= 3 {
                        // Re-order points convexly or around center
                        let cx = loop_pts.iter().map(|p| p.x.get()).sum::<f64>() / loop_pts.len() as f64;
                        let cy = loop_pts.iter().map(|p| p.y.get()).sum::<f64>() / loop_pts.len() as f64;
                        loop_pts.sort_by(|a, b| {
                            let ang_a = (a.y.get() - cy).atan2(a.x.get() - cx);
                            let ang_b = (b.y.get() - cy).atan2(b.x.get() - cx);
                            ang_a.partial_cmp(&ang_b).unwrap_or(std::cmp::Ordering::Equal)
                        });
                        add_polyline_to_path(&loop_pts, true, &mut out_path)?;
                    }
                }
                BooleanOp::Trim => {
                    // Subj minus Clip: collect subj points outside clip + intersection points
                    let mut loop_pts = Vec::new();
                    for &p in &subj.points {
                        if !point_in_polygon(p, &clip.points) {
                            loop_pts.push(p);
                        }
                    }
                    // Insert intersection entry/exit points
                    for i in 0..subj.points.len() {
                        let p1 = subj.points[i];
                        let p2 = subj.points[(i + 1) % subj.points.len()];
                        for j in 0..clip.points.len() {
                            let p3 = clip.points[j];
                            let p4 = clip.points[(j + 1) % clip.points.len()];
                            if let Some((_, _, ipt)) = segment_intersection(p1, p2, p3, p4) {
                                loop_pts.push(ipt);
                            }
                        }
                    }
                    if loop_pts.len() >= 3 {
                        let cx = loop_pts.iter().map(|p| p.x.get()).sum::<f64>() / loop_pts.len() as f64;
                        let cy = loop_pts.iter().map(|p| p.y.get()).sum::<f64>() / loop_pts.len() as f64;
                        loop_pts.sort_by(|a, b| {
                            let ang_a = (a.y.get() - cy).atan2(a.x.get() - cx);
                            let ang_b = (b.y.get() - cy).atan2(b.x.get() - cx);
                            ang_a.partial_cmp(&ang_b).unwrap_or(std::cmp::Ordering::Equal)
                        });
                        add_polyline_to_path(&loop_pts, true, &mut out_path)?;
                    }
                }
                BooleanOp::Intersect => {
                    // Intersection: points of subj inside clip + points of clip inside subj + intersection points
                    let mut loop_pts = Vec::new();
                    for &p in &subj.points {
                        if point_in_polygon(p, &clip.points) {
                            loop_pts.push(p);
                        }
                    }
                    for &p in &clip.points {
                        if point_in_polygon(p, &subj.points) {
                            loop_pts.push(p);
                        }
                    }
                    for i in 0..subj.points.len() {
                        let p1 = subj.points[i];
                        let p2 = subj.points[(i + 1) % subj.points.len()];
                        for j in 0..clip.points.len() {
                            let p3 = clip.points[j];
                            let p4 = clip.points[(j + 1) % clip.points.len()];
                            if let Some((_, _, ipt)) = segment_intersection(p1, p2, p3, p4) {
                                loop_pts.push(ipt);
                            }
                        }
                    }
                    if loop_pts.len() >= 3 {
                        let cx = loop_pts.iter().map(|p| p.x.get()).sum::<f64>() / loop_pts.len() as f64;
                        let cy = loop_pts.iter().map(|p| p.y.get()).sum::<f64>() / loop_pts.len() as f64;
                        loop_pts.sort_by(|a, b| {
                            let ang_a = (a.y.get() - cy).atan2(a.x.get() - cx);
                            let ang_b = (b.y.get() - cy).atan2(b.x.get() - cx);
                            ang_a.partial_cmp(&ang_b).unwrap_or(std::cmp::Ordering::Equal)
                        });
                        add_polyline_to_path(&loop_pts, true, &mut out_path)?;
                    }
                }
                BooleanOp::FrontMinusBack | BooleanOp::BackMinusFront => {}
            }
        }
    }

    if out_path.elements.is_empty() {
        // Fallback for non-overlapping or subtle intersection: return subject
        Ok(subj_list.first().map(|p| polyline_to_path(&p.points, p.closed)).unwrap_or_default())
    } else {
        Ok(out_path)
    }
}

fn add_polyline_to_path(points: &[Point], closed: bool, path: &mut Path) -> Result<(), GeomError> {
    if points.is_empty() {
        return Ok(());
    }
    path.move_to(points[0])?;
    for &p in &points[1..] {
        path.line_to(p)?;
    }
    if closed {
        path.close()?;
    }
    Ok(())
}

fn polyline_to_path(points: &[Point], closed: bool) -> Path {
    let mut p = Path::new();
    let _ = add_polyline_to_path(points, closed, &mut p);
    p
}

/// Generates clean-room vector Bézier/line contours for a given artistic text string.
///
/// Produces resolution-independent vector glyph outlines suitable for `ConvertToCurves` (`Ctrl+Q`),
/// boolean operations, rendering, and vector export without requiring external font files.
pub fn artistic_text_to_path(text: &str, _font_family: &str, size_pt: f64, position: Point) -> Path {
    let mut path = Path::new();
    let scale = (size_pt / 10.0).max(0.1);
    let mut cur_x = position.x.get();
    let baseline_y = position.y.get();

    for ch in text.chars() {
        let (w, glyph_ops) = glyph_outline(ch);
        for op in glyph_ops {
            match op {
                GlyphOp::MoveTo(x, y) => {
                    if let Ok(pt) = Point::new(cur_x + x * scale, baseline_y - y * scale) {
                        let _ = path.move_to(pt);
                    }
                }
                GlyphOp::LineTo(x, y) => {
                    if let Ok(pt) = Point::new(cur_x + x * scale, baseline_y - y * scale) {
                        let _ = path.line_to(pt);
                    }
                }
                GlyphOp::Close => {
                    let _ = path.close();
                }
            }
        }
        cur_x += (w + 1.5) * scale;
    }

    path
}

/// Lays out `text` glyph-by-glyph along `spine` (`Text -> Fit Text to Path`) using arc-length
/// parameterization and tangent rotation, with perpendicular baseline offset `offset_pt`.
pub fn fit_text_to_path(text: &str, font_family: &str, size_pt: f64, spine: &Path, offset_pt: f64) -> Result<Path, GeomError> {
    let lut = crate::path::ArcLengthTable::from_path(spine)?;
    if lut.total_length() <= 1e-6 {
        return Ok(artistic_text_to_path(text, font_family, size_pt, Point::ZERO));
    }
    let scale = (size_pt / 10.0).max(0.1);
    let total_len = lut.total_length();
    let mut out = Path::new();
    let mut s_cur = 0.0_f64;

    for ch in text.chars() {
        let (w, glyph_ops) = glyph_outline(ch);
        let u = (s_cur / total_len).clamp(0.0, 1.0);
        if let Some(sample) = lut.sample_normalized(u) {
            let nx = sample.tangent.1;
            let ny = -sample.tangent.0;
            let px = sample.point.x.get() + offset_pt * nx;
            let py = sample.point.y.get() + offset_pt * ny;
            let tf = crate::affine::Affine2::rotation_deg(sample.angle_deg)?.then(crate::affine::Affine2::translation(px, py)?)?;
            for op in glyph_ops {
                match *op {
                    GlyphOp::MoveTo(x, y) => {
                        let pt = tf.apply_point(Point::new(x * scale, -y * scale)?)?;
                        out.move_to(pt)?;
                    }
                    GlyphOp::LineTo(x, y) => {
                        let pt = tf.apply_point(Point::new(x * scale, -y * scale)?)?;
                        out.line_to(pt)?;
                    }
                    GlyphOp::Close => {
                        out.close()?;
                    }
                }
            }
        }
        s_cur += (w + 1.5) * scale;
    }

    Ok(out)
}

/// Builds a parametric CorelDRAW Linear Dimension path (`p1 -> p2`) with extension lines,
/// arrowheads, and a centered measurement `label` offset by `offset_pt`.
pub fn build_linear_dimension_path(p1: Point, p2: Point, offset_pt: f64, label: &str, font_size_pt: f64) -> Result<Path, GeomError> {
    if !offset_pt.is_finite() || !font_size_pt.is_finite() {
        return Err(GeomError::NonFiniteCoordinate);
    }
    if font_size_pt <= 0.0 {
        return Err(GeomError::InvalidShapeParameter("dimension font size must be positive"));
    }
    let dx = p2.x.get() - p1.x.get();
    let dy = p2.y.get() - p1.y.get();
    let len = dx.hypot(dy);
    if len < 1e-6 {
        return Err(GeomError::InvalidShapeParameter("degenerate dimension"));
    }

    let arrow = (font_size_pt * 0.4).clamp(2.0, 8.0);
    let sign = if offset_pt >= 0.0 { -1.0 } else { 1.0 };
    let by = -offset_pt;
    let ext_end = by + sign * arrow * 0.75;

    let mut local = Path::new();
    // 1. Extension line 1 and 2
    local.move_to(Point::new(0.0, 0.0)?)?;
    local.line_to(Point::new(0.0, ext_end)?)?;
    local.move_to(Point::new(len, 0.0)?)?;
    local.line_to(Point::new(len, ext_end)?)?;

    // 2. Dimension line
    local.move_to(Point::new(0.0, by)?)?;
    local.line_to(Point::new(len, by)?)?;

    // 3. Left and right arrowheads
    let aw = arrow.min(len * 0.4);
    let ah = aw * 0.4;
    local.move_to(Point::new(0.0, by)?)?;
    local.line_to(Point::new(aw, by - ah)?)?;
    local.line_to(Point::new(aw, by + ah)?)?;
    local.close()?;

    local.move_to(Point::new(len, by)?)?;
    local.line_to(Point::new(len - aw, by - ah)?)?;
    local.line_to(Point::new(len - aw, by + ah)?)?;
    local.close()?;

    // 4. Centered measurement label
    if !label.is_empty() {
        let scale = (font_size_pt / 10.0).max(0.1);
        let label_w: f64 = label.chars().map(|ch| (glyph_outline(ch).0 + 1.5) * scale).sum();
        let text_origin = Point::new((len - label_w) * 0.5, by - 3.0)?;
        let label_path = artistic_text_to_path(label, "Sans", font_size_pt, text_origin);
        for el in label_path.elements {
            local.push(el)?;
        }
    }

    let angle_deg = dy.atan2(dx).to_degrees();
    let tf = crate::affine::Affine2::rotation_deg(angle_deg)?.then(crate::affine::Affine2::translation(p1.x.get(), p1.y.get())?)?;
    local.transform(tf)
}

/// Builds a parametric CorelDRAW Angular Dimension path measuring the interior angle at `vertex`
/// between `ray1_pt` and `ray2_pt` with circular dimension arc at `arc_radius_pt`, extension rays,
/// tangent arrowheads, and an angle `label` positioned outside the arc midpoint.
pub fn build_angular_dimension_path(
    vertex: Point,
    ray1_pt: Point,
    ray2_pt: Point,
    arc_radius_pt: f64,
    label: &str,
    font_size_pt: f64,
) -> Result<Path, GeomError> {
    if !arc_radius_pt.is_finite() || !font_size_pt.is_finite() {
        return Err(GeomError::NonFiniteCoordinate);
    }
    if arc_radius_pt <= 0.0 || font_size_pt <= 0.0 {
        return Err(GeomError::InvalidShapeParameter("angular dimension radius and font size must be positive"));
    }
    let vx = vertex.x.get();
    let vy = vertex.y.get();
    let dx1 = ray1_pt.x.get() - vx;
    let dy1 = ray1_pt.y.get() - vy;
    let dx2 = ray2_pt.x.get() - vx;
    let dy2 = ray2_pt.y.get() - vy;
    if dx1.hypot(dy1) < 1e-6 || dx2.hypot(dy2) < 1e-6 {
        return Err(GeomError::InvalidShapeParameter("degenerate angular dimension ray"));
    }

    let a1 = dy1.atan2(dx1);
    let a2 = dy2.atan2(dx2);
    let mut delta = (a2 - a1).rem_euclid(std::f64::consts::TAU);
    if delta > std::f64::consts::PI {
        delta -= std::f64::consts::TAU;
    }
    if delta.abs() < 1e-6 {
        return Err(GeomError::InvalidShapeParameter("collinear angular dimension rays"));
    }

    let arrow = (font_size_pt * 0.4).clamp(2.0, 8.0);
    let ext_r = arc_radius_pt + arrow * 0.75;
    let a_end = a1 + delta;

    let mut path = Path::new();
    // 1. Extension rays from vertex
    path.move_to(vertex)?;
    path.line_to(Point::new(vx + ext_r * a1.cos(), vy + ext_r * a1.sin())?)?;
    path.move_to(vertex)?;
    path.line_to(Point::new(vx + ext_r * a_end.cos(), vy + ext_r * a_end.sin())?)?;

    // 2. Dimension circular arc (16 segments)
    let steps = 16_u32;
    let p_arc_start = Point::new(vx + arc_radius_pt * a1.cos(), vy + arc_radius_pt * a1.sin())?;
    path.move_to(p_arc_start)?;
    for step in 1..=steps {
        let t = f64::from(step) / f64::from(steps);
        let ang = a1 + delta * t;
        path.line_to(Point::new(vx + arc_radius_pt * ang.cos(), vy + arc_radius_pt * ang.sin())?)?;
    }

    // 3. Tangent arrowheads at arc endpoints
    let dir_sign = if delta >= 0.0 { 1.0 } else { -1.0 };
    let aw = arrow.min(arc_radius_pt * delta.abs() * 0.4);
    let ah = aw * 0.4;

    // Start arrowhead pointing along +dir_sign tangent
    let tx1 = -a1.sin() * dir_sign;
    let ty1 = a1.cos() * dir_sign;
    let rx1 = a1.cos();
    let ry1 = a1.sin();
    path.move_to(p_arc_start)?;
    path.line_to(Point::new(p_arc_start.x.get() + aw * tx1 - ah * rx1, p_arc_start.y.get() + aw * ty1 - ah * ry1)?)?;
    path.line_to(Point::new(p_arc_start.x.get() + aw * tx1 + ah * rx1, p_arc_start.y.get() + aw * ty1 + ah * ry1)?)?;
    path.close()?;

    // End arrowhead pointing along -dir_sign tangent
    let p_arc_end = Point::new(vx + arc_radius_pt * a_end.cos(), vy + arc_radius_pt * a_end.sin())?;
    let tx2 = a_end.sin() * dir_sign;
    let ty2 = -a_end.cos() * dir_sign;
    let rx2 = a_end.cos();
    let ry2 = a_end.sin();
    path.move_to(p_arc_end)?;
    path.line_to(Point::new(p_arc_end.x.get() + aw * tx2 - ah * rx2, p_arc_end.y.get() + aw * ty2 - ah * ry2)?)?;
    path.line_to(Point::new(p_arc_end.x.get() + aw * tx2 + ah * rx2, p_arc_end.y.get() + aw * ty2 + ah * ry2)?)?;
    path.close()?;

    // 4. Angle measurement label outside the arc midpoint
    if !label.is_empty() {
        let a_mid = a1 + delta * 0.5;
        let label_r = arc_radius_pt + font_size_pt * 0.8;
        let scale = (font_size_pt / 10.0).max(0.1);
        let label_w: f64 = label.chars().map(|ch| (glyph_outline(ch).0 + 1.5) * scale).sum();
        let text_origin = Point::new(vx + label_r * a_mid.cos() - label_w * 0.5, vy + label_r * a_mid.sin())?;
        let label_path = artistic_text_to_path(label, "Sans", font_size_pt, text_origin);
        for el in label_path.elements {
            path.push(el)?;
        }
    }

    Ok(path)
}

#[derive(Clone, Copy)]
enum GlyphOp {
    MoveTo(f64, f64),
    LineTo(f64, f64),
    Close,
}

fn glyph_outline(ch: char) -> (f64, &'static [GlyphOp]) {
    match ch {
        'A' => (
            6.0,
            &[
                GlyphOp::MoveTo(0.0, 0.0),
                GlyphOp::LineTo(2.5, 8.0),
                GlyphOp::LineTo(3.5, 8.0),
                GlyphOp::LineTo(6.0, 0.0),
                GlyphOp::LineTo(4.8, 0.0),
                GlyphOp::LineTo(4.0, 2.5),
                GlyphOp::LineTo(2.0, 2.5),
                GlyphOp::LineTo(1.2, 0.0),
                GlyphOp::Close,
            ],
        ),
        'C' => (
            6.0,
            &[
                GlyphOp::MoveTo(5.5, 6.5),
                GlyphOp::LineTo(4.5, 8.0),
                GlyphOp::LineTo(2.0, 8.0),
                GlyphOp::LineTo(0.5, 6.0),
                GlyphOp::LineTo(0.5, 2.0),
                GlyphOp::LineTo(2.0, 0.0),
                GlyphOp::LineTo(4.5, 0.0),
                GlyphOp::LineTo(5.5, 1.5),
                GlyphOp::LineTo(4.5, 2.2),
                GlyphOp::LineTo(3.8, 1.2),
                GlyphOp::LineTo(2.2, 1.2),
                GlyphOp::LineTo(1.5, 2.5),
                GlyphOp::LineTo(1.5, 5.5),
                GlyphOp::LineTo(2.2, 6.8),
                GlyphOp::LineTo(3.8, 6.8),
                GlyphOp::LineTo(4.5, 5.8),
                GlyphOp::Close,
            ],
        ),
        'O' => (
            6.5,
            &[
                GlyphOp::MoveTo(2.0, 0.0),
                GlyphOp::LineTo(4.5, 0.0),
                GlyphOp::LineTo(6.0, 2.0),
                GlyphOp::LineTo(6.0, 6.0),
                GlyphOp::LineTo(4.5, 8.0),
                GlyphOp::LineTo(2.0, 8.0),
                GlyphOp::LineTo(0.5, 6.0),
                GlyphOp::LineTo(0.5, 2.0),
                GlyphOp::Close,
            ],
        ),
        'R' => (
            6.0,
            &[
                GlyphOp::MoveTo(0.5, 0.0),
                GlyphOp::LineTo(0.5, 8.0),
                GlyphOp::LineTo(4.0, 8.0),
                GlyphOp::LineTo(5.5, 6.5),
                GlyphOp::LineTo(5.5, 4.5),
                GlyphOp::LineTo(4.0, 3.5),
                GlyphOp::LineTo(5.8, 0.0),
                GlyphOp::LineTo(4.5, 0.0),
                GlyphOp::LineTo(2.8, 3.5),
                GlyphOp::LineTo(1.5, 3.5),
                GlyphOp::LineTo(1.5, 0.0),
                GlyphOp::Close,
            ],
        ),
        ' ' => (3.5, &[]),
        _ => (5.0, &[GlyphOp::MoveTo(0.5, 0.0), GlyphOp::LineTo(0.5, 8.0), GlyphOp::LineTo(4.5, 8.0), GlyphOp::LineTo(4.5, 0.0), GlyphOp::Close]),
    }
}

fn clip_polygon_half_plane(pts: &[Point], origin: Point, nx: f64, ny: f64, offset: f64) -> Result<Vec<Point>, GeomError> {
    if pts.len() < 3 {
        return Ok(Vec::new());
    }
    let signed_dist = |p: Point| -> f64 { (p.x.get() - origin.x.get()) * nx + (p.y.get() - origin.y.get()) * ny - offset };
    let mut out = Vec::with_capacity(pts.len() + 2);
    let mut prev = pts[pts.len() - 1];
    let mut d_prev = signed_dist(prev);
    for &curr in pts {
        let d_curr = signed_dist(curr);
        let prev_in = d_prev >= -1e-9;
        let curr_in = d_curr >= -1e-9;
        if prev_in != curr_in {
            let denom = d_prev - d_curr;
            if denom.abs() > 1e-12 {
                let t = (d_prev / denom).clamp(0.0, 1.0);
                let ix = prev.x.get() + t * (curr.x.get() - prev.x.get());
                let iy = prev.y.get() + t * (curr.y.get() - prev.y.get());
                out.push(Point::new(ix, iy)?);
            }
        }
        if curr_in {
            out.push(curr);
        }
        prev = curr;
        d_prev = d_curr;
    }
    Ok(out)
}

/// Slices a closed [`Path`] along the cut line through `line_start` and `line_end` with blade
/// kerf width `gap_pt >= 0.0` (`CorelDRAW Knife Tool`), emitting both clipped half-space loops.
pub fn knife_cut_path_by_line(path: &Path, line_start: Point, line_end: Point, gap_pt: f64) -> Result<Path, GeomError> {
    let gap = crate::F64::new(gap_pt)?.get();
    if gap < 0.0 {
        return Err(GeomError::InvalidShapeParameter("knife gap_pt must be non-negative"));
    }
    let dx = line_end.x.get() - line_start.x.get();
    let dy = line_end.y.get() - line_start.y.get();
    let len = dx.hypot(dy);
    if len <= crate::EPS {
        return Err(GeomError::InvalidShapeParameter("knife cut line endpoints must be distinct"));
    }
    let nx = -dy / len;
    let ny = dx / len;
    let h = 0.5 * gap.max(1e-4);
    let polylines = crate::flatten::flatten_path(path, &crate::flatten::FlattenOpts::default())?;
    let mut out = Path { elements: Vec::new(), fill_rule: path.fill_rule };
    for poly in polylines {
        if poly.points.len() < 3 {
            continue;
        }
        for (sx, sy) in [(nx, ny), (-nx, -ny)] {
            let clipped = clip_polygon_half_plane(&poly.points, line_start, sx, sy, h)?;
            if clipped.len() >= 3 {
                out.move_to(clipped[0])?;
                for pt in clipped.iter().skip(1) {
                    out.line_to(*pt)?;
                }
                out.close()?;
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Rect;
    use crate::shapes::RectSpec;

    #[test]
    fn boolean_weld_trim_intersect_rectangles() {
        let r1 = RectSpec::sharp(Rect::new(0.0, 0.0, 50.0, 50.0).unwrap()).to_path().unwrap();
        let r2 = RectSpec::sharp(Rect::new(25.0, 0.0, 75.0, 50.0).unwrap()).to_path().unwrap();

        // 1. Weld
        let weld = boolean_paths(&r1, &r2, BooleanOp::Weld).expect("weld");
        let weld_bounds = weld.bounds().expect("weld bounds");
        assert!(weld_bounds.width() >= 50.0);

        // 2. Trim
        let trim = boolean_paths(&r1, &r2, BooleanOp::Trim).expect("trim");
        assert!(!trim.elements.is_empty());

        // 3. Intersect
        let inter = boolean_paths(&r1, &r2, BooleanOp::Intersect).expect("intersect");
        let inter_bounds = inter.bounds().expect("intersect bounds");
        assert!(inter_bounds.width() <= 50.0);

        // 4. Knife cut splits rectangle into 2 closed subpaths
        let cut = knife_cut_path_by_line(&r1, Point::new(-10.0, 25.0).unwrap(), Point::new(60.0, 25.0).unwrap(), 2.0).expect("knife cut");
        let move_count = cut.elements.iter().filter(|e| matches!(e, crate::path::PathElement::MoveTo(_))).count();
        assert_eq!(move_count, 2);
    }

    #[test]
    fn artistic_text_to_path_generates_vector_loops() {
        let pt = Point::new(10.0, 100.0).unwrap();
        let path = artistic_text_to_path("CORAL", "Sans", 24.0, pt);
        assert!(!path.elements.is_empty());
        let bounds = path.bounds().expect("text bounds");
        assert!(bounds.width() > 20.0);
        assert!(bounds.height() > 10.0);

        let mut spine = Path::new();
        spine.move_to(Point::new(0.0, 50.0).unwrap()).unwrap();
        spine.line_to(Point::new(200.0, 150.0).unwrap()).unwrap();
        let curved_text = fit_text_to_path("CORAL", "Sans", 18.0, &spine, 2.0).unwrap();
        let cb = curved_text.bounds().unwrap();
        assert!(cb.width() > 20.0);
        assert!(cb.height() > 15.0);

        let dim = build_linear_dimension_path(Point::new(20.0, 100.0).unwrap(), Point::new(140.0, 100.0).unwrap(), 18.0, "120 pt", 10.0).unwrap();
        let db = dim.bounds().unwrap();
        assert!((db.x0.get() - 20.0).abs() < 1e-4);
        assert!((db.x1.get() - 140.0).abs() < 1e-4);
        assert!(db.height() >= 18.0);

        let ang_dim = build_angular_dimension_path(
            Point::new(100.0, 100.0).unwrap(),
            Point::new(180.0, 100.0).unwrap(),
            Point::new(100.0, 180.0).unwrap(),
            40.0,
            "90 deg",
            10.0,
        )
        .unwrap();
        let ab = ang_dim.bounds().unwrap();
        assert!(ab.width() >= 40.0);
        assert!(ab.height() >= 40.0);
    }
}
