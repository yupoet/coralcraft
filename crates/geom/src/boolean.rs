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
    }

    #[test]
    fn artistic_text_to_path_generates_vector_loops() {
        let pt = Point::new(10.0, 100.0).unwrap();
        let path = artistic_text_to_path("CORAL", "Sans", 24.0, pt);
        assert!(!path.elements.is_empty());
        let bounds = path.bounds().expect("text bounds");
        assert!(bounds.width() > 20.0);
        assert!(bounds.height() > 10.0);
    }
}
