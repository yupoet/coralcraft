//! Adaptive de Casteljau Bézier flattening with bounded recursion depth.

use serde::{Deserialize, Serialize};

use crate::f64_canon::{EPS, GeomError, Point};
use crate::path::{MAX_PATH_ELEMENTS, Path, PathElement};

/// Default flatness tolerance in points (`0.1 pt`).
pub const DEFAULT_FLATTEN_TOLERANCE: f64 = 0.1;

/// Default maximum recursion depth (`12` -> at most `4,096` segments per cubic curve).
pub const DEFAULT_MAX_FLATTEN_DEPTH: u8 = 12;

/// Options controlling adaptive Bézier curve flattening.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlattenOpts {
    /// Maximum perpendicular deviation from chord in coordinate units (must be `> 0.0`).
    pub tolerance: f64,
    /// Unconditional recursion depth ceiling (capped at `16`).
    pub max_depth: u8,
}

impl Default for FlattenOpts {
    fn default() -> Self {
        Self { tolerance: DEFAULT_FLATTEN_TOLERANCE, max_depth: DEFAULT_MAX_FLATTEN_DEPTH }
    }
}

/// Flattened polyline representing one subpath.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Polyline {
    /// Ordered vertices of the subpath.
    pub points: Vec<Point>,
    /// Whether the subpath is closed (`Close` verb was present).
    pub closed: bool,
}

/// Evaluates a cubic Bézier curve at parameter `t in [0.0, 1.0]`.
pub fn eval_cubic(p0: Point, c1: Point, c2: Point, p3: Point, t: f64) -> Result<Point, GeomError> {
    let u = 1.0 - t;
    let b0 = u * u * u;
    let b1 = 3.0 * u * u * t;
    let b2 = 3.0 * u * t * t;
    let b3 = t * t * t;
    let x = b0 * p0.x.get() + b1 * c1.x.get() + b2 * c2.x.get() + b3 * p3.x.get();
    let y = b0 * p0.y.get() + b1 * c1.y.get() + b2 * c2.y.get() + b3 * p3.y.get();
    Point::new(x, y)
}

/// Flattens a single cubic Bézier segment `p0 .. p3` with control points `c1, c2`,
/// invoking `sink` for each subdivided vertex after `p0` (ending with `p3`).
///
/// Guarantees termination within `2^min(opts.max_depth, 16)` segments even for extreme coordinates.
pub fn flatten_cubic(p0: Point, c1: Point, c2: Point, p3: Point, opts: &FlattenOpts, sink: &mut dyn FnMut(Point)) {
    let tol = if opts.tolerance.is_finite() && opts.tolerance > 1e-6 { opts.tolerance } else { DEFAULT_FLATTEN_TOLERANCE };
    let max_depth = opts.max_depth.min(16);

    fn subdivide(pts: [f64; 8], tol_sq: f64, depth: u8, max_depth: u8, sink: &mut dyn FnMut(Point)) {
        let [x0, y0, x1, y1, x2, y2, x3, y3] = pts;
        if depth >= max_depth || is_cubic_flat(pts, tol_sq) {
            if let Ok(pt) = Point::new(x3, y3) {
                sink(pt);
            }
            return;
        }

        // de Casteljau midpoint subdivision
        let x01 = 0.5 * (x0 + x1);
        let y01 = 0.5 * (y0 + y1);
        let x12 = 0.5 * (x1 + x2);
        let y12 = 0.5 * (y1 + y2);
        let x23 = 0.5 * (x2 + x3);
        let y23 = 0.5 * (y2 + y3);

        let x012 = 0.5 * (x01 + x12);
        let y012 = 0.5 * (y01 + y12);
        let x123 = 0.5 * (x12 + x23);
        let y123 = 0.5 * (y12 + y23);

        let x0123 = 0.5 * (x012 + x123);
        let y0123 = 0.5 * (y012 + y123);

        if !(x0123.is_finite() && y0123.is_finite()) {
            if let Ok(pt) = Point::new(x3, y3) {
                sink(pt);
            }
            return;
        }

        subdivide([x0, y0, x01, y01, x012, y012, x0123, y0123], tol_sq, depth + 1, max_depth, sink);
        subdivide([x0123, y0123, x123, y123, x23, y23, x3, y3], tol_sq, depth + 1, max_depth, sink);
    }

    subdivide([p0.x.get(), p0.y.get(), c1.x.get(), c1.y.get(), c2.x.get(), c2.y.get(), p3.x.get(), p3.y.get()], tol * tol, 0, max_depth, sink);
}

/// Checks whether a cubic Bézier `(p0, c1, c2, p3)` is within `tol_sq` of the straight chord `p0 -> p3`.
///
/// For a cubic curve, the maximum distance from the curve to the chord is bounded by `3/4 * max(d(c1), d(c2))`.
/// We conservatively require `max(d(c1)^2, d(c2)^2) <= tol_sq`.
fn is_cubic_flat(pts: [f64; 8], tol_sq: f64) -> bool {
    let [x0, y0, x1, y1, x2, y2, x3, y3] = pts;
    let dx = x3 - x0;
    let dy = y3 - y0;
    let len_sq = dx * dx + dy * dy;

    let d1_sq = point_to_segment_dist_sq(x1, y1, x0, y0, dx, dy, len_sq);
    let d2_sq = point_to_segment_dist_sq(x2, y2, x0, y0, dx, dy, len_sq);
    d1_sq.max(d2_sq) <= tol_sq
}

fn point_to_segment_dist_sq(px: f64, py: f64, x0: f64, y0: f64, dx: f64, dy: f64, len_sq: f64) -> f64 {
    if !len_sq.is_finite() || len_sq <= EPS * EPS {
        let vx = px - x0;
        let vy = py - y0;
        return vx * vx + vy * vy;
    }
    let t = (((px - x0) * dx + (py - y0) * dy) / len_sq).clamp(0.0, 1.0);
    let proj_x = x0 + t * dx;
    let proj_y = y0 + t * dy;
    let ex = px - proj_x;
    let ey = py - proj_y;
    ex * ex + ey * ey
}

/// Elevates a quadratic Bézier `(p0, ctrl, end)` to an exact cubic Bézier and flattens it.
pub fn flatten_quad(p0: Point, ctrl: Point, end: Point, opts: &FlattenOpts, sink: &mut dyn FnMut(Point)) {
    let c1x = p0.x.get() + (2.0 / 3.0) * (ctrl.x.get() - p0.x.get());
    let c1y = p0.y.get() + (2.0 / 3.0) * (ctrl.y.get() - p0.y.get());
    let c2x = end.x.get() + (2.0 / 3.0) * (ctrl.x.get() - end.x.get());
    let c2y = end.y.get() + (2.0 / 3.0) * (ctrl.y.get() - end.y.get());
    if let (Ok(c1), Ok(c2)) = (Point::new(c1x, c1y), Point::new(c2x, c2y)) {
        flatten_cubic(p0, c1, c2, end, opts, sink);
    } else {
        sink(end);
    }
}

/// Flattens a [`Path`] into one [`Polyline`] per subpath.
pub fn flatten_path(path: &Path, opts: &FlattenOpts) -> Result<Vec<Polyline>, GeomError> {
    if path.elements.len() > MAX_PATH_ELEMENTS {
        return Err(GeomError::PathTooLarge { max: MAX_PATH_ELEMENTS });
    }

    let mut subpaths: Vec<Polyline> = Vec::new();
    let mut current: Option<Polyline> = None;
    let mut cursor = Point::ZERO;

    for &el in &path.elements {
        match el {
            PathElement::MoveTo(p) => {
                if let Some(poly) = current.take()
                    && !poly.points.is_empty()
                {
                    subpaths.push(poly);
                }
                cursor = p;
                current = Some(Polyline { points: vec![p], closed: false });
            }
            PathElement::LineTo(p) => {
                let poly = current.get_or_insert_with(|| Polyline { points: vec![cursor], closed: false });
                poly.points.push(p);
                cursor = p;
            }
            PathElement::QuadTo { ctrl, end } => {
                let poly = current.get_or_insert_with(|| Polyline { points: vec![cursor], closed: false });
                flatten_quad(cursor, ctrl, end, opts, &mut |pt| {
                    poly.points.push(pt);
                });
                cursor = end;
            }
            PathElement::CubicTo { c1, c2, end } => {
                let poly = current.get_or_insert_with(|| Polyline { points: vec![cursor], closed: false });
                flatten_cubic(cursor, c1, c2, end, opts, &mut |pt| {
                    poly.points.push(pt);
                });
                cursor = end;
            }
            PathElement::Close => {
                if let Some(mut poly) = current.take() {
                    poly.closed = true;
                    if let Some(&first) = poly.points.first() {
                        cursor = first;
                    }
                    subpaths.push(poly);
                }
            }
        }
    }

    if let Some(poly) = current.take()
        && !poly.points.is_empty()
    {
        subpaths.push(poly);
    }

    Ok(subpaths)
}
