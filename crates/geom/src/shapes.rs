//! CorelDRAW parametric shape generators (`RectSpec`, `EllipseSpec`, `PolygonSpec`).
//!
//! Adapted from `vectorcraft-geom::shapes` (`MIT OR Apache-2.0`) and extended with CorelDRAW's
//! per-corner `Round` / `Scalloped` / `Chamfered` rectangle cuts, `Full` / `Pie` / `Arc` ellipses,
//! and regular polygons / stars.

use std::f64::consts::{PI, TAU};

use serde::{Deserialize, Serialize};

use crate::f64_canon::{EPS, F64, GeomError, Point, Rect};
use crate::path::{FillRule, Path};

/// Cubic Bézier handle ratio for a 90° circular quarter-arc (`4 * (sqrt(2) - 1) / 3`).
pub const KAPPA: f64 = 0.552_284_749_830_793_6;

/// CorelDRAW rectangle corner style (Property Bar: Round, Scalloped, Chamfered).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CornerKind {
    /// Outward quarter-circle fillet (default).
    #[default]
    Round,
    /// Inward quarter-circle notch centered at the rectangle corner (CorelDRAW "Scalloped Corner").
    Scalloped,
    /// Straight diagonal bevel cut (CorelDRAW "Chamfered Corner").
    Chamfered,
}

impl CornerKind {
    /// Cycles to the next corner kind (`Round -> Scalloped -> Chamfered -> Round`).
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Round => Self::Scalloped,
            Self::Scalloped => Self::Chamfered,
            Self::Chamfered => Self::Round,
        }
    }
}

/// Parametric CorelDRAW rectangle with independent per-corner radii and corner styles
/// ordered clockwise from top-left: `[top_left, top_right, bottom_right, bottom_left]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RectSpec {
    /// Axis-aligned bounding rectangle.
    pub rect: Rect,
    /// Corner radii `[top_left, top_right, bottom_right, bottom_left]` in document points.
    pub radii: [F64; 4],
    /// Corner cut styles `[top_left, top_right, bottom_right, bottom_left]`.
    pub corner_kinds: [CornerKind; 4],
}

impl RectSpec {
    /// Creates a sharp-cornered rectangle.
    #[must_use]
    pub const fn sharp(rect: Rect) -> Self {
        Self { rect, radii: [F64::ZERO; 4], corner_kinds: [CornerKind::Round; 4] }
    }

    /// Creates a rectangle with uniform corner `radius` and `kind`.
    pub fn uniform(rect: Rect, radius: f64, kind: CornerKind) -> Result<Self, GeomError> {
        let r = F64::new(radius.max(0.0))?;
        Ok(Self { rect, radii: [r; 4], corner_kinds: [kind; 4] })
    }

    /// Clamps the 4 corner radii so no corner exceeds `min(width, height) / 2.0`.
    #[must_use]
    pub fn clamped_radii(&self) -> [f64; 4] {
        let max_r = 0.5 * self.rect.width().min(self.rect.height());
        [
            self.radii[0].get().clamp(0.0, max_r),
            self.radii[1].get().clamp(0.0, max_r),
            self.radii[2].get().clamp(0.0, max_r),
            self.radii[3].get().clamp(0.0, max_r),
        ]
    }

    /// Converts the parametric rectangle into a closed [`Path`] clockwise from the top edge.
    pub fn to_path(&self) -> Result<Path, GeomError> {
        let x0 = self.rect.x0.get();
        let y0 = self.rect.y0.get();
        let x1 = self.rect.x1.get();
        let y1 = self.rect.y1.get();
        let [r_tl, r_tr, r_br, r_bl] = self.clamped_radii();

        let mut path = Path::new();

        // Start at top edge after top-left corner cut
        path.move_to(Point::new(x0 + r_tl, y0)?)?;

        // Top edge -> Top-Right corner (corner at (x1, y0): enters at (x1 - r_tr, y0), exits at (x1, y0 + r_tr))
        path.line_to(Point::new(x1 - r_tr, y0)?)?;
        if r_tr > EPS {
            emit_corner(&mut path, self.corner_kinds[1], Point::new(x1 - r_tr, y0)?, Point::new(x1, y0 + r_tr)?, (1.0, 0.0), (0.0, 1.0), r_tr)?;
        }

        // Right edge -> Bottom-Right corner (corner at (x1, y1): enters at (x1, y1 - r_br), exits at (x1 - r_br, y1))
        path.line_to(Point::new(x1, y1 - r_br)?)?;
        if r_br > EPS {
            emit_corner(&mut path, self.corner_kinds[2], Point::new(x1, y1 - r_br)?, Point::new(x1 - r_br, y1)?, (0.0, 1.0), (-1.0, 0.0), r_br)?;
        }

        // Bottom edge -> Bottom-Left corner (corner at (x0, y1): enters at (x0 + r_bl, y1), exits at (x0, y1 - r_bl))
        path.line_to(Point::new(x0 + r_bl, y1)?)?;
        if r_bl > EPS {
            emit_corner(&mut path, self.corner_kinds[3], Point::new(x0 + r_bl, y1)?, Point::new(x0, y1 - r_bl)?, (-1.0, 0.0), (0.0, -1.0), r_bl)?;
        }

        // Left edge -> Top-Left corner (corner at (x0, y0): enters at (x0, y0 + r_tl), exits at (x0 + r_tl, y0))
        path.line_to(Point::new(x0, y0 + r_tl)?)?;
        if r_tl > EPS {
            emit_corner(&mut path, self.corner_kinds[0], Point::new(x0, y0 + r_tl)?, Point::new(x0 + r_tl, y0)?, (0.0, -1.0), (1.0, 0.0), r_tl)?;
        }

        path.close()?;
        Ok(path)
    }
}

/// Emits one rectangle corner transition from `enter` to `exit`:
/// - `Round`: outward quarter circle with tangents along `in_dir` and `out_dir`.
/// - `Scalloped`: inward quarter circle centered at the corner vertex (` tangents along `out_dir` at enter and `-in_dir` at exit).
/// - `Chamfered`: straight bevel line `enter -> exit`.
fn emit_corner(
    path: &mut Path,
    kind: CornerKind,
    enter: Point,
    exit: Point,
    in_dir: (f64, f64),
    out_dir: (f64, f64),
    r: f64,
) -> Result<(), GeomError> {
    let k = r * KAPPA;
    match kind {
        CornerKind::Round => {
            let c1 = Point::new(enter.x.get() + in_dir.0 * k, enter.y.get() + in_dir.1 * k)?;
            let c2 = Point::new(exit.x.get() - out_dir.0 * k, exit.y.get() - out_dir.1 * k)?;
            path.cubic_to(c1, c2, exit)
        }
        CornerKind::Scalloped => {
            let c1 = Point::new(enter.x.get() + out_dir.0 * k, enter.y.get() + out_dir.1 * k)?;
            let c2 = Point::new(exit.x.get() - in_dir.0 * k, exit.y.get() - in_dir.1 * k)?;
            path.cubic_to(c1, c2, exit)
        }
        CornerKind::Chamfered => path.line_to(exit),
    }
}

/// CorelDRAW Ellipse mode (Property Bar: Ellipse, Pie, Arc).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EllipseMode {
    /// Complete closed ellipse (4 cubic quarter-arcs).
    #[default]
    Full,
    /// Closed pie wedge connected through the ellipse center.
    Pie,
    /// Open elliptical arc between `start_deg` and `end_deg`.
    Arc,
}

/// Parametric CorelDRAW ellipse (`Full`, `Pie`, or `Arc`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EllipseSpec {
    /// Bounding rectangle of the full ellipse.
    pub rect: Rect,
    /// Start angle in degrees (`0.0` = 3 o'clock).
    pub start_deg: F64,
    /// End angle in degrees.
    pub end_deg: F64,
    /// Ellipse drawing mode (`Full`, `Pie`, or `Arc`).
    pub mode: EllipseMode,
}

impl EllipseSpec {
    /// Creates a full closed ellipse inscribed in `rect`.
    #[must_use]
    pub const fn full(rect: Rect) -> Self {
        Self { rect, start_deg: F64::ZERO, end_deg: F64::ZERO, mode: EllipseMode::Full }
    }

    /// Creates a pie wedge from `start_deg` to `end_deg`.
    pub fn pie(rect: Rect, start_deg: f64, end_deg: f64) -> Result<Self, GeomError> {
        Ok(Self { rect, start_deg: F64::new(start_deg)?, end_deg: F64::new(end_deg)?, mode: EllipseMode::Pie })
    }

    /// Creates an open arc from `start_deg` to `end_deg`.
    pub fn arc(rect: Rect, start_deg: f64, end_deg: f64) -> Result<Self, GeomError> {
        Ok(Self { rect, start_deg: F64::new(start_deg)?, end_deg: F64::new(end_deg)?, mode: EllipseMode::Arc })
    }

    /// Converts the ellipse specification into a [`Path`].
    pub fn to_path(&self) -> Result<Path, GeomError> {
        let c = self.rect.center()?;
        let cx = c.x.get();
        let cy = c.y.get();
        let rx = 0.5 * self.rect.width();
        let ry = 0.5 * self.rect.height();

        let span = (self.end_deg.get() - self.start_deg.get()).rem_euclid(360.0);
        if self.mode == EllipseMode::Full || span < EPS || (360.0 - span) < EPS {
            let kx = rx * KAPPA;
            let ky = ry * KAPPA;
            let mut path = Path::new();
            path.move_to(Point::new(cx + rx, cy)?)?;
            path.cubic_to(Point::new(cx + rx, cy + ky)?, Point::new(cx + kx, cy + ry)?, Point::new(cx, cy + ry)?)?;
            path.cubic_to(Point::new(cx - kx, cy + ry)?, Point::new(cx - rx, cy + ky)?, Point::new(cx - rx, cy)?)?;
            path.cubic_to(Point::new(cx - rx, cy - ky)?, Point::new(cx - kx, cy - ry)?, Point::new(cx, cy - ry)?)?;
            path.cubic_to(Point::new(cx + kx, cy - ry)?, Point::new(cx + rx, cy - ky)?, Point::new(cx + rx, cy)?)?;
            path.close()?;
            return Ok(path);
        }

        // Subdivide angular span into at most 4 cubic pieces (each <= 90 degrees).
        let pieces = ((span / 90.0).ceil() as u32).clamp(1, 4);
        let step_deg = span / f64::from(pieces);
        let k = (4.0 / 3.0) * (step_deg.to_radians() / 4.0).tan();

        let eval_pt = |deg: f64| -> Result<(Point, (f64, f64)), GeomError> {
            let rad = deg.to_radians();
            let (s, cos_t) = rad.sin_cos();
            let pt = Point::new(cx + rx * cos_t, cy + ry * s)?;
            let deriv = (-rx * s, ry * cos_t);
            Ok((pt, deriv))
        };

        let mut path = Path::new();
        let start = self.start_deg.get();
        let (p_first, _) = eval_pt(start)?;

        if self.mode == EllipseMode::Pie {
            path.move_to(c)?;
            path.line_to(p_first)?;
        } else {
            path.move_to(p_first)?;
        }

        for i in 0..pieces {
            let a0 = start + step_deg * f64::from(i);
            let a1 = start + step_deg * f64::from(i + 1);
            let (p0, d0) = eval_pt(a0)?;
            let (p1, d1) = eval_pt(a1)?;
            let c1 = Point::new(p0.x.get() + k * d0.0, p0.y.get() + k * d0.1)?;
            let c2 = Point::new(p1.x.get() - k * d1.0, p1.y.get() - k * d1.1)?;
            path.cubic_to(c1, c2, p1)?;
        }

        if self.mode == EllipseMode::Pie {
            path.close()?;
        }

        Ok(path)
    }
}

/// Parametric regular polygon or star (CorelDRAW Polygon / Star tool `Y`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PolygonSpec {
    /// Center of the polygon or star.
    pub center: Point,
    /// Outer vertex radius in document points.
    pub radius: F64,
    /// Optional inner vertex radius for stars (`None` = regular polygon).
    pub inner_radius: Option<F64>,
    /// Number of polygon sides or star points (`>= 3`).
    pub points: u32,
    /// Rotation angle in degrees (`0.0` places the first vertex pointing straight up).
    pub rotation_deg: F64,
}

impl PolygonSpec {
    /// Creates a regular polygon with `points >= 3` sides.
    pub fn regular(center: Point, radius: f64, points: u32, rotation_deg: f64) -> Result<Self, GeomError> {
        if points < 3 {
            return Err(GeomError::InvalidShapeParameter("polygon requires at least 3 points"));
        }
        let r = F64::new(radius)?;
        if r.get() <= 0.0 {
            return Err(GeomError::InvalidShapeParameter("polygon radius must be positive"));
        }
        Ok(Self { center, radius: r, inner_radius: None, points, rotation_deg: F64::new(rotation_deg)? })
    }

    /// Creates a star with `points >= 3` tips, outer `radius`, and `inner_radius`.
    pub fn star(center: Point, radius: f64, inner_radius: f64, points: u32, rotation_deg: f64) -> Result<Self, GeomError> {
        if points < 3 {
            return Err(GeomError::InvalidShapeParameter("star requires at least 3 points"));
        }
        let r_out = F64::new(radius)?;
        let r_in = F64::new(inner_radius)?;
        if r_out.get() <= 0.0 || r_in.get() <= 0.0 {
            return Err(GeomError::InvalidShapeParameter("star radii must be positive"));
        }
        Ok(Self { center, radius: r_out, inner_radius: Some(r_in), points, rotation_deg: F64::new(rotation_deg)? })
    }

    /// Converts the polygon or star into a closed [`Path`].
    pub fn to_path(&self) -> Result<Path, GeomError> {
        if self.points < 3 {
            return Err(GeomError::InvalidShapeParameter("polygon requires at least 3 points"));
        }
        let cx = self.center.x.get();
        let cy = self.center.y.get();
        let r_out = self.radius.get();
        let rot = self.rotation_deg.get().to_radians();

        let mut path = Path { elements: Vec::new(), fill_rule: FillRule::NonZero };

        if let Some(r_in_f64) = self.inner_radius {
            let r_in = r_in_f64.get();
            let total_vertices = self.points.saturating_mul(2);
            for i in 0..total_vertices {
                let r = if i % 2 == 0 { r_out } else { r_in };
                let angle = -PI / 2.0 + rot + TAU * f64::from(i) / f64::from(total_vertices);
                let pt = Point::new(cx + r * angle.cos(), cy + r * angle.sin())?;
                if i == 0 {
                    path.move_to(pt)?;
                } else {
                    path.line_to(pt)?;
                }
            }
        } else {
            for i in 0..self.points {
                let angle = -PI / 2.0 + rot + TAU * f64::from(i) / f64::from(self.points);
                let pt = Point::new(cx + r_out * angle.cos(), cy + r_out * angle.sin())?;
                if i == 0 {
                    path.move_to(pt)?;
                } else {
                    path.line_to(pt)?;
                }
            }
        }

        path.close()?;
        Ok(path)
    }
}

/// Builds a CorelDRAW Symmetrical (`expansion == 1.0`) or Logarithmic (`expansion > 1.0`) Spiral path (`A` tool)
/// using quarter-turn Hermite-to-cubic-Bézier segments (`4 * revolutions` segments).
pub fn build_spiral_path(center: Point, outer_radius: f64, revolutions: u32, expansion: f64) -> Result<Path, GeomError> {
    if !(1..=100).contains(&revolutions) {
        return Err(GeomError::InvalidShapeParameter("spiral revolutions must be in 1..=100"));
    }
    if !outer_radius.is_finite() || outer_radius <= 0.0 {
        return Err(GeomError::InvalidShapeParameter("spiral outer_radius must be positive"));
    }
    if !expansion.is_finite() || !(1.0..=100.0).contains(&expansion) {
        return Err(GeomError::InvalidShapeParameter("spiral expansion must be in 1.0..=100.0"));
    }

    let cx = center.x.get();
    let cy = center.y.get();
    let total_theta = TAU * f64::from(revolutions);
    let segments = revolutions * 4;
    let d_theta = total_theta / f64::from(segments);

    let is_logarithmic = (expansion - 1.0).abs() > 1e-6;
    let b_log = if is_logarithmic { (1.0 + (expansion - 1.0) * 0.1).ln() / TAU } else { 0.0 };
    let exp_total = if is_logarithmic { (b_log * total_theta).exp() - 1.0 } else { 1.0 };

    let eval_r_and_dr = |theta: f64| -> (f64, f64) {
        if is_logarithmic && exp_total > EPS {
            let e_bt = (b_log * theta).exp();
            let r = outer_radius * (e_bt - 1.0) / exp_total;
            let dr = outer_radius * b_log * e_bt / exp_total;
            (r, dr)
        } else {
            let r = outer_radius * (theta / total_theta);
            let dr = outer_radius / total_theta;
            (r, dr)
        }
    };

    let eval_pt_and_deriv = |theta: f64| -> Result<(Point, (f64, f64)), GeomError> {
        let (r, dr) = eval_r_and_dr(theta);
        let (sin_t, cos_t) = theta.sin_cos();
        let pt = Point::new(cx + r * cos_t, cy + r * sin_t)?;
        let dx = dr * cos_t - r * sin_t;
        let dy = dr * sin_t + r * cos_t;
        Ok((pt, (dx, dy)))
    };

    let mut path = Path::new();
    let (p_start, _) = eval_pt_and_deriv(0.0)?;
    path.move_to(p_start)?;

    for i in 0..segments {
        let t0 = f64::from(i) * d_theta;
        let t1 = f64::from(i + 1) * d_theta;
        let (p0, d0) = eval_pt_and_deriv(t0)?;
        let (p1, d1) = eval_pt_and_deriv(t1)?;
        let k = d_theta / 3.0;
        let c1 = Point::new(p0.x.get() + k * d0.0, p0.y.get() + k * d0.1)?;
        let c2 = Point::new(p1.x.get() - k * d1.0, p1.y.get() - k * d1.1)?;
        path.cubic_to(c1, c2, p1)?;
    }

    Ok(path)
}

/// Builds a CorelDRAW Graph Paper grid (`D` tool) consisting of `columns * rows` closed rectangular cells
/// inside `rect`.
pub fn build_graph_paper_path(rect: Rect, columns: u32, rows: u32) -> Result<Path, GeomError> {
    if !(1..=100).contains(&columns) || !(1..=100).contains(&rows) {
        return Err(GeomError::InvalidShapeParameter("graph paper columns and rows must be in 1..=100"));
    }
    let w = rect.width();
    let h = rect.height();
    if w <= EPS || h <= EPS {
        return Err(GeomError::InvalidShapeParameter("graph paper rect must have positive width and height"));
    }
    let cell_w = w / f64::from(columns);
    let cell_h = h / f64::from(rows);
    let x0 = rect.x0.get();
    let y0 = rect.y0.get();

    let mut path = Path::new();
    for r in 0..rows {
        for c in 0..columns {
            let cx0 = x0 + f64::from(c) * cell_w;
            let cy0 = y0 + f64::from(r) * cell_h;
            let cx1 = cx0 + cell_w;
            let cy1 = cy0 + cell_h;
            path.move_to(Point::new(cx0, cy0)?)?;
            path.line_to(Point::new(cx1, cy0)?)?;
            path.line_to(Point::new(cx1, cy1)?)?;
            path.line_to(Point::new(cx0, cy1)?)?;
            path.close()?;
        }
    }
    Ok(path)
}

/// Builds a straight or orthogonal (Manhattan Z-elbow) connector path between `p1` and `p2`,
/// optionally rounding the 90-degree elbows by `corner_radius`.
pub fn build_connector_path(p1: Point, p2: Point, orthogonal: bool, corner_radius: f64) -> Result<Path, GeomError> {
    if !corner_radius.is_finite() || corner_radius < 0.0 {
        return Err(GeomError::InvalidShapeParameter("connector corner_radius must be non-negative"));
    }
    let mut path = Path::new();
    path.move_to(p1)?;
    if !orthogonal || (p1.x.get() - p2.x.get()).abs() <= EPS || (p1.y.get() - p2.y.get()).abs() <= EPS {
        path.line_to(p2)?;
        return Ok(path);
    }
    let mid_x = 0.5 * (p1.x.get() + p2.x.get());
    path.line_to(Point::new(mid_x, p1.y.get())?)?;
    path.line_to(Point::new(mid_x, p2.y.get())?)?;
    path.line_to(p2)?;
    if corner_radius > EPS { crate::path::fillet_path_corners(&path, corner_radius, CornerKind::Round) } else { Ok(path) }
}

/// Builds a closed rectangular speech/annotation callout box (`box_rect`) with a triangular pointer tail
/// attached to the box edge closest to `target_pt` and pointing to `target_pt`.
pub fn build_callout_path(box_rect: Rect, target_pt: Point, tail_width: f64) -> Result<Path, GeomError> {
    let w = box_rect.width();
    let h = box_rect.height();
    if w <= EPS || h <= EPS || !tail_width.is_finite() || tail_width <= 0.0 {
        return Err(GeomError::InvalidShapeParameter("invalid callout rectangle or tail_width"));
    }
    let x0 = box_rect.x0.get();
    let y0 = box_rect.y0.get();
    let x1 = box_rect.x1.get();
    let y1 = box_rect.y1.get();
    let tx = target_pt.x.get();
    let ty = target_pt.y.get();

    // Determine which of the 4 edges (0=bottom, 1=top, 2=left, 3=right) is closest to target_pt
    let d_bottom = (ty - y1).abs();
    let d_top = (ty - y0).abs();
    let d_left = (tx - x0).abs();
    let d_right = (tx - x1).abs();
    let mut best_edge = 0u8;
    let mut best_d = d_bottom;
    if d_top < best_d {
        best_d = d_top;
        best_edge = 1;
    }
    if d_left < best_d {
        best_d = d_left;
        best_edge = 2;
    }
    if d_right < best_d {
        best_edge = 3;
    }

    let hw_x = (0.5 * tail_width).min(0.35 * w);
    let hw_y = (0.5 * tail_width).min(0.35 * h);
    let mid_x = tx.clamp(x0 + hw_x, x1 - hw_x);
    let mid_y = ty.clamp(y0 + hw_y, y1 - hw_y);

    let mut path = Path::new();
    path.move_to(Point::new(x0, y0)?)?;
    // Top edge: (x0, y0) -> (x1, y0)
    if best_edge == 1 {
        path.line_to(Point::new(mid_x - hw_x, y0)?)?;
        path.line_to(target_pt)?;
        path.line_to(Point::new(mid_x + hw_x, y0)?)?;
    }
    path.line_to(Point::new(x1, y0)?)?;
    // Right edge: (x1, y0) -> (x1, y1)
    if best_edge == 3 {
        path.line_to(Point::new(x1, mid_y - hw_y)?)?;
        path.line_to(target_pt)?;
        path.line_to(Point::new(x1, mid_y + hw_y)?)?;
    }
    path.line_to(Point::new(x1, y1)?)?;
    // Bottom edge: (x1, y1) -> (x0, y1)
    if best_edge == 0 {
        path.line_to(Point::new(mid_x + hw_x, y1)?)?;
        path.line_to(target_pt)?;
        path.line_to(Point::new(mid_x - hw_x, y1)?)?;
    }
    path.line_to(Point::new(x0, y1)?)?;
    // Left edge: (x0, y1) -> (x0, y0)
    if best_edge == 2 {
        path.line_to(Point::new(x0, mid_y + hw_y)?)?;
        path.line_to(target_pt)?;
        path.line_to(Point::new(x0, mid_y - hw_y)?)?;
    }
    path.close()?;
    Ok(path)
}

/// Builds a smooth Catmull-Rom / B-Spline cubic Bézier [`Path`] passing through `control_points`
/// (`B-Spline Tool`).
pub fn build_bspline_path(control_points: &[Point], closed: bool) -> Result<Path, GeomError> {
    if control_points.len() < 2 {
        return Err(GeomError::InvalidShapeParameter("build_bspline_path requires at least 2 control points"));
    }
    let n = control_points.len();
    let mut path = Path::new();
    path.move_to(control_points[0])?;

    if n == 2 {
        path.line_to(control_points[1])?;
        if closed {
            path.close()?;
        }
        return Ok(path);
    }

    let get_pt = |idx: isize| -> Point {
        if closed {
            let rem = idx.rem_euclid(n as isize) as usize;
            control_points[rem]
        } else {
            let clamped = idx.clamp(0, (n - 1) as isize) as usize;
            control_points[clamped]
        }
    };

    let seg_count = if closed { n } else { n - 1 };
    for i in 0..seg_count {
        let p_prev = get_pt((i as isize) - 1);
        let p0 = get_pt(i as isize);
        let p1 = get_pt((i + 1) as isize);
        let p_next = get_pt((i + 2) as isize);

        let c1 = Point::new(p0.x.get() + (p1.x.get() - p_prev.x.get()) / 6.0, p0.y.get() + (p1.y.get() - p_prev.y.get()) / 6.0)?;
        let c2 = Point::new(p1.x.get() - (p_next.x.get() - p0.x.get()) / 6.0, p1.y.get() - (p_next.y.get() - p0.y.get()) / 6.0)?;
        path.cubic_to(c1, c2, p1)?;
    }

    if closed {
        path.close()?;
    }
    Ok(path)
}
