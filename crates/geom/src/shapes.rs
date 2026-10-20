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

/// Builds a 3-Point Curve [`Path`] starting at `start`, ending at `end`, and passing exactly
/// through `through` at parameter `t = 0.5` (`3-Point Curve Tool`).
pub fn build_three_point_curve_path(start: Point, end: Point, through: Point) -> Result<Path, GeomError> {
    let qx = 2.0 * through.x.get() - 0.5 * (start.x.get() + end.x.get());
    let qy = 2.0 * through.y.get() - 0.5 * (start.y.get() + end.y.get());
    let c1 = Point::new(start.x.get() + (2.0 / 3.0) * (qx - start.x.get()), start.y.get() + (2.0 / 3.0) * (qy - start.y.get()))?;
    let c2 = Point::new(end.x.get() + (2.0 / 3.0) * (qx - end.x.get()), end.y.get() + (2.0 / 3.0) * (qy - end.y.get()))?;
    let mut path = Path::new();
    path.move_to(start)?;
    path.cubic_to(c1, c2, end)?;
    Ok(path)
}

/// Builds a compound vector halftone dot grid [`Path`] inside `bounds` (`Effects -> Pointillizer`).
pub fn build_pointillizer_path(bounds: Rect, columns: u32, rows: u32, dot_scale: f64) -> Result<Path, GeomError> {
    let _ = F64::new(dot_scale)?;
    let cols = columns.clamp(1, 64);
    let rws = rows.clamp(1, 64);
    let scale = dot_scale.clamp(0.05, 1.0);
    let cell_w = bounds.width() / f64::from(cols);
    let cell_h = bounds.height() / f64::from(rws);
    let r = (0.5 * cell_w.min(cell_h) * scale).max(0.25);
    let k = r * KAPPA;
    let mut path = Path::new();

    for row in 0..rws {
        let cy = bounds.y0.get() + (f64::from(row) + 0.5) * cell_h;
        for col in 0..cols {
            let cx = bounds.x0.get() + (f64::from(col) + 0.5) * cell_w;
            path.move_to(Point::new(cx + r, cy)?)?;
            path.cubic_to(Point::new(cx + r, cy + k)?, Point::new(cx + k, cy + r)?, Point::new(cx, cy + r)?)?;
            path.cubic_to(Point::new(cx - k, cy + r)?, Point::new(cx - r, cy + k)?, Point::new(cx - r, cy)?)?;
            path.cubic_to(Point::new(cx - r, cy - k)?, Point::new(cx - k, cy - r)?, Point::new(cx, cy - r)?)?;
            path.cubic_to(Point::new(cx + k, cy - r)?, Point::new(cx + r, cy - k)?, Point::new(cx + r, cy)?)?;
            path.close()?;
        }
    }
    Ok(path)
}

/// Builds a compound radial burst rays [`Path`] around `center` from `inner_radius` to `outer_radius`
/// (`Impact Tool`).
pub fn build_impact_rays_path(center: Point, inner_radius: f64, outer_radius: f64, ray_count: u32) -> Result<Path, GeomError> {
    let r_in = F64::new(inner_radius)?.get().max(0.0);
    let r_out = F64::new(outer_radius)?.get();
    if r_out <= r_in || r_out <= 0.0 {
        return Err(GeomError::InvalidShapeParameter("outer_radius must exceed inner_radius and 0"));
    }
    let count = ray_count.clamp(3, 128);
    let step = std::f64::consts::TAU / f64::from(count);
    let half_wedge = 0.25 * step;
    let cx = center.x.get();
    let cy = center.y.get();
    let mut path = Path::new();

    for i in 0..count {
        let theta = f64::from(i) * step;
        let a0 = theta - half_wedge;
        let a1 = theta + half_wedge;
        let p_in = Point::new(cx + r_in * theta.cos(), cy + r_in * theta.sin())?;
        let p_out0 = Point::new(cx + r_out * a0.cos(), cy + r_out * a0.sin())?;
        let p_out1 = Point::new(cx + r_out * a1.cos(), cy + r_out * a1.sin())?;
        path.move_to(p_in)?;
        path.line_to(p_out0)?;
        path.line_to(p_out1)?;
        path.close()?;
    }
    Ok(path)
}

fn gcd_u32(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a.max(1)
}

/// Builds a Complex Star (`{n/k}` Schläfli star polygon) [`Path`] around `center` (`Complex Star Tool`).
pub fn build_complex_star_path(center: Point, radius: f64, points: u32, sharpness: u32) -> Result<Path, GeomError> {
    let r = F64::new(radius)?.get();
    if r <= 0.0 {
        return Err(GeomError::InvalidShapeParameter("radius must be positive"));
    }
    let n = points.clamp(5, 256);
    let max_k = (n - 1) / 2;
    let k = sharpness.clamp(2, max_k.max(2));
    let cycles = gcd_u32(n, k);
    let steps_per_cycle = n / cycles;
    let cx = center.x.get();
    let cy = center.y.get();
    let mut path = Path { elements: Vec::new(), fill_rule: crate::path::FillRule::EvenOdd };

    for c in 0..cycles {
        for s in 0..steps_per_cycle {
            let v_idx = (c + s * k) % n;
            let angle = -std::f64::consts::FRAC_PI_2 + std::f64::consts::TAU * (f64::from(v_idx) / f64::from(n));
            let pt = Point::new(cx + r * angle.cos(), cy + r * angle.sin())?;
            if s == 0 {
                path.move_to(pt)?;
            } else {
                path.line_to(pt)?;
            }
        }
        path.close()?;
    }
    Ok(path)
}

/// Builds an open or closed polyline [`Path`] from `points` (`Freehand / Polyline Tool F5`).
pub fn build_polyline_path(points: &[Point], closed: bool) -> Result<Path, GeomError> {
    if points.len() < 2 {
        return Err(GeomError::InvalidShapeParameter("polyline requires at least 2 points"));
    }
    let mut path = Path::new();
    path.move_to(points[0])?;
    for &pt in &points[1..] {
        path.line_to(pt)?;
    }
    if closed {
        path.close()?;
    }
    Ok(path)
}

/// Builds a parametric mechanical gear / cogwheel [`Path`] with `teeth` trapezoidal cogs and a
/// concentric axle bore hole (`Basic Shapes -> Gear Wheel`).
pub fn build_gear_wheel_path(center: Point, outer_radius: f64, inner_radius: f64, teeth: u32) -> Result<Path, GeomError> {
    let r_out = F64::new(outer_radius)?.get();
    let r_in = F64::new(inner_radius)?.get();
    if r_in <= 0.0 || r_out <= r_in {
        return Err(GeomError::InvalidShapeParameter("outer_radius must exceed inner_radius and 0"));
    }
    let n = teeth.clamp(3, 128);
    let pitch = std::f64::consts::TAU / f64::from(n);
    let cx = center.x.get();
    let cy = center.y.get();
    let mut path = Path { elements: Vec::new(), fill_rule: crate::path::FillRule::EvenOdd };

    for i in 0..n {
        let base = -std::f64::consts::FRAC_PI_2 + f64::from(i) * pitch;
        let samples = [(0.0, r_in), (0.15, r_out), (0.45, r_out), (0.60, r_in)];
        for (k, &(frac, r)) in samples.iter().enumerate() {
            let a = base + frac * pitch;
            let pt = Point::new(cx + r * a.cos(), cy + r * a.sin())?;
            if i == 0 && k == 0 {
                path.move_to(pt)?;
            } else {
                path.line_to(pt)?;
            }
        }
    }
    path.close()?;

    // Add concentric circular bore hole subpath at 0.4 * inner_radius
    let bore_r = 0.4 * r_in;
    let k = bore_r * KAPPA;
    path.move_to(Point::new(cx + bore_r, cy)?)?;
    path.cubic_to(Point::new(cx + bore_r, cy + k)?, Point::new(cx + k, cy + bore_r)?, Point::new(cx, cy + bore_r)?)?;
    path.cubic_to(Point::new(cx - k, cy + bore_r)?, Point::new(cx - bore_r, cy + k)?, Point::new(cx - bore_r, cy)?)?;
    path.cubic_to(Point::new(cx - bore_r, cy - k)?, Point::new(cx - k, cy - bore_r)?, Point::new(cx, cy - bore_r)?)?;
    path.cubic_to(Point::new(cx + k, cy - bore_r)?, Point::new(cx + bore_r, cy - k)?, Point::new(cx + bore_r, cy)?)?;
    path.close()?;
    Ok(path)
}

/// Builds a closed 7-vertex Block Arrow [`Path`] from `start` to `end` (`Polygon Flyout -> Arrow Shapes`).
pub fn build_block_arrow_path(start: Point, end: Point, shaft_width_pt: f64, head_width_pt: f64, head_length_pt: f64) -> Result<Path, GeomError> {
    let shaft_w = F64::new(shaft_width_pt)?.get();
    let head_w = F64::new(head_width_pt)?.get();
    let head_l = F64::new(head_length_pt)?.get();
    if shaft_w <= 0.0 || head_w < shaft_w || head_l <= 0.0 {
        return Err(GeomError::InvalidShapeParameter("invalid block arrow dimensions"));
    }
    let dx = end.x.get() - start.x.get();
    let dy = end.y.get() - start.y.get();
    let len = dx.hypot(dy);
    if len <= 1e-6 {
        return Err(GeomError::InvalidShapeParameter("block arrow length must be positive"));
    }
    let ux = dx / len;
    let uy = dy / len;
    let nx = -uy;
    let ny = ux;
    let eff_head_l = head_l.min(0.8 * len);
    let neck_x = start.x.get() + (len - eff_head_l) * ux;
    let neck_y = start.y.get() + (len - eff_head_l) * uy;
    let hs = 0.5 * shaft_w;
    let hh = 0.5 * head_w;

    let mut path = Path::new();
    path.move_to(Point::new(start.x.get() + hs * nx, start.y.get() + hs * ny)?)?;
    path.line_to(Point::new(neck_x + hs * nx, neck_y + hs * ny)?)?;
    path.line_to(Point::new(neck_x + hh * nx, neck_y + hh * ny)?)?;
    path.line_to(end)?;
    path.line_to(Point::new(neck_x - hh * nx, neck_y - hh * ny)?)?;
    path.line_to(Point::new(neck_x - hs * nx, neck_y - hs * ny)?)?;
    path.line_to(Point::new(start.x.get() - hs * nx, start.y.get() - hs * ny)?)?;
    path.close()?;
    Ok(path)
}

/// Builds a compound 3-part heraldic Banner / Ribbon [`Path`] inside `bounds` with recessed
/// side tails (`fold_depth_pt`) and V-shaped swallowtail notches (`swallowtail_depth_pt`)
/// (`Polygon Flyout -> Banner Shapes`).
pub fn build_banner_ribbon_path(bounds: Rect, fold_depth_pt: f64, swallowtail_depth_pt: f64) -> Result<Path, GeomError> {
    let w = bounds.width();
    let h = bounds.height();
    let fold_raw = F64::new(fold_depth_pt)?.get();
    let tail_raw = F64::new(swallowtail_depth_pt)?.get();
    if w <= 0.0 || h <= 0.0 || fold_raw < 0.0 || tail_raw < 0.0 {
        return Err(GeomError::InvalidShapeParameter("invalid banner ribbon dimensions"));
    }
    let fold = fold_raw.clamp(0.05 * w, 0.35 * w);
    let tail = tail_raw.clamp(0.0, 0.8 * fold);
    let x0 = bounds.x0.get();
    let y0 = bounds.y0.get();
    let x1 = bounds.x1.get();
    let y1 = bounds.y1.get();
    let drop_y = 0.15 * h;
    let tail_mid_y = y0 + drop_y + 0.5 * (h - drop_y);

    let mut path = Path::new();
    // 1. Left swallowtail wing
    path.move_to(Point::new(x0, y0 + drop_y)?)?;
    path.line_to(Point::new(x0 + fold, y0 + drop_y)?)?;
    path.line_to(Point::new(x0 + fold, y1)?)?;
    path.line_to(Point::new(x0, y1)?)?;
    path.line_to(Point::new(x0 + tail, tail_mid_y)?)?;
    path.close()?;

    // 2. Right swallowtail wing
    path.move_to(Point::new(x1 - fold, y0 + drop_y)?)?;
    path.line_to(Point::new(x1, y0 + drop_y)?)?;
    path.line_to(Point::new(x1 - tail, tail_mid_y)?)?;
    path.line_to(Point::new(x1, y1)?)?;
    path.line_to(Point::new(x1 - fold, y1)?)?;
    path.close()?;

    // 3. Central foreground banner panel
    let inset = 0.6 * fold;
    path.move_to(Point::new(x0 + inset, y0)?)?;
    path.line_to(Point::new(x1 - inset, y0)?)?;
    path.line_to(Point::new(x1 - inset, y1 - drop_y)?)?;
    path.line_to(Point::new(x0 + inset, y1 - drop_y)?)?;
    path.close()?;
    Ok(path)
}

/// Standard ISO 5807 / CorelDRAW Flowchart Symbol kinds (`Polygon Flyout -> Flowchart Shapes`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FlowchartSymbolKind {
    /// Rectangular process step.
    Process,
    /// Diamond decision branch.
    Decision,
    /// Stadium / capsule start-or-end terminator.
    Terminator,
    /// Slanted parallelogram input/output data block.
    Data,
    /// Rectangle with a sinusoidal wavy bottom edge representing a document.
    Document,
}

/// Builds a closed Flowchart Symbol [`Path`] inside `bounds` (`Polygon Flyout -> Flowchart Shapes`).
pub fn build_flowchart_symbol_path(bounds: Rect, kind: FlowchartSymbolKind) -> Result<Path, GeomError> {
    let w = bounds.width();
    let h = bounds.height();
    if w <= 0.0 || h <= 0.0 {
        return Err(GeomError::InvalidShapeParameter("flowchart symbol bounds must have positive area"));
    }
    let x0 = bounds.x0.get();
    let y0 = bounds.y0.get();
    let x1 = bounds.x1.get();
    let y1 = bounds.y1.get();
    let xm = 0.5 * (x0 + x1);
    let ym = 0.5 * (y0 + y1);

    match kind {
        FlowchartSymbolKind::Process => RectSpec::sharp(bounds).to_path(),
        FlowchartSymbolKind::Decision => {
            let mut path = Path::new();
            path.move_to(Point::new(xm, y0)?)?;
            path.line_to(Point::new(x1, ym)?)?;
            path.line_to(Point::new(xm, y1)?)?;
            path.line_to(Point::new(x0, ym)?)?;
            path.close()?;
            Ok(path)
        }
        FlowchartSymbolKind::Terminator => {
            let r = 0.5 * w.min(h);
            RectSpec::uniform(bounds, r, CornerKind::Round)?.to_path()
        }
        FlowchartSymbolKind::Data => {
            let slant = 0.2 * w;
            let mut path = Path::new();
            path.move_to(Point::new(x0 + slant, y0)?)?;
            path.line_to(Point::new(x1, y0)?)?;
            path.line_to(Point::new(x1 - slant, y1)?)?;
            path.line_to(Point::new(x0, y1)?)?;
            path.close()?;
            Ok(path)
        }
        FlowchartSymbolKind::Document => {
            let wave = 0.12 * h;
            let wy = y1 - wave;
            let mut path = Path::new();
            path.move_to(Point::new(x0, y0)?)?;
            path.line_to(Point::new(x1, y0)?)?;
            path.line_to(Point::new(x1, wy)?)?;
            path.cubic_to(Point::new(x0 + 0.75 * w, wy - 1.5 * wave)?, Point::new(x0 + 0.25 * w, wy + 2.5 * wave)?, Point::new(x0, wy)?)?;
            path.close()?;
            Ok(path)
        }
    }
}

/// Builds a closed Rhodonea / Guilloche Rose Curve [`Path`] ($r(\theta) = R \cos(k \theta)$) centered
/// at `center` with outer `radius`, petal frequency `petals_k` (`1..=36`), and `points_per_lobe` (`4..=64`).
pub fn build_rose_guilloche_path(center: Point, radius: f64, petals_k: u32, points_per_lobe: u32) -> Result<Path, GeomError> {
    let r_max = F64::new(radius)?.get();
    if r_max <= 0.0 || petals_k == 0 || points_per_lobe < 4 {
        return Err(GeomError::InvalidShapeParameter("invalid rose guilloche parameters"));
    }
    let k = petals_k.clamp(1, 36);
    let lobes = if k.is_multiple_of(2) { 2 * k } else { k };
    let max_theta = if k.is_multiple_of(2) { std::f64::consts::TAU } else { std::f64::consts::PI };
    let total_steps = (lobes * points_per_lobe.clamp(4, 64)).min(1024);
    let cx = center.x.get();
    let cy = center.y.get();

    let mut path = Path::new();
    for step in 0..total_steps {
        let theta = max_theta * (f64::from(step) / f64::from(total_steps));
        let r = r_max * (f64::from(k) * theta).cos();
        let (s, c) = theta.sin_cos();
        let pt = Point::new(cx + r * c, cy + r * s)?;
        if step == 0 {
            path.move_to(pt)?;
        } else {
            path.line_to(pt)?;
        }
    }
    path.close()?;
    Ok(path)
}

/// Builds a compound vector Barcode [`Path`] inside `bounds` encoding `digits` (`Object -> Insert -> Barcode`).
/// Uses a clean-room 7-module L-code digit encoding with start (`101`), center (`01010`), and end (`101`) guard bars.
pub fn build_barcode_path(bounds: Rect, digits: &str) -> Result<Path, GeomError> {
    let w = bounds.width();
    let h = bounds.height();
    let clean: Vec<u8> = digits.bytes().filter(|b| b.is_ascii_digit()).collect();
    if w <= 0.0 || h <= 0.0 || clean.is_empty() {
        return Err(GeomError::InvalidShapeParameter("barcode requires positive bounds and at least one digit"));
    }
    const L_PATTERNS: [u8; 10] = [
        0b000_1101, // 0
        0b001_1001, // 1
        0b001_0011, // 2
        0b011_1101, // 3
        0b010_0011, // 4
        0b011_0001, // 5
        0b010_1111, // 6
        0b011_1011, // 7
        0b011_0111, // 8
        0b000_1011, // 9
    ];
    let mut modules = Vec::with_capacity(clean.len() * 7 + 11);
    modules.extend_from_slice(&[true, false, true]); // start guard
    let half = clean.len() / 2;
    for (idx, &d) in clean.iter().take(24).enumerate() {
        if idx == half && half > 0 {
            modules.extend_from_slice(&[false, true, false, true, false]); // center guard
        }
        let pat = L_PATTERNS[(d - b'0') as usize];
        for bit in (0..7).rev() {
            modules.push(((pat >> bit) & 1) != 0);
        }
    }
    modules.extend_from_slice(&[true, false, true]); // end guard

    let quiet = 4usize;
    let total_mods = modules.len() + 2 * quiet;
    let mod_w = w / (total_mods as f64);
    let x0 = bounds.x0.get();
    let y0 = bounds.y0.get();
    let y1 = bounds.y1.get();

    let mut path = Path::new();
    let mut i = 0usize;
    while i < modules.len() {
        if !modules[i] {
            i += 1;
            continue;
        }
        let run_start = i;
        while i < modules.len() && modules[i] {
            i += 1;
        }
        let bx0 = x0 + ((quiet + run_start) as f64) * mod_w;
        let bx1 = x0 + ((quiet + i) as f64) * mod_w;
        path.move_to(Point::new(bx0, y0)?)?;
        path.line_to(Point::new(bx1, y0)?)?;
        path.line_to(Point::new(bx1, y1)?)?;
        path.line_to(Point::new(bx0, y1)?)?;
        path.close()?;
    }
    Ok(path)
}

/// Builds a 2-Point Perspective Grid [`Path`] inside `bounds` with left and right vanishing points
/// at `(bounds.x0, horizon_y_pt)` and `(bounds.x1, horizon_y_pt)` and `subdivisions` (`2..=32`) rays
/// (`Object -> Perspective -> Draw in Perspective Grid`).
pub fn build_perspective_grid_path(bounds: Rect, horizon_y_pt: f64, subdivisions: u32) -> Result<Path, GeomError> {
    let w = bounds.width();
    let h = bounds.height();
    let hy = F64::new(horizon_y_pt)?.get();
    if w <= 0.0 || h <= 0.0 || subdivisions < 2 {
        return Err(GeomError::InvalidShapeParameter("invalid perspective grid parameters"));
    }
    let divs = subdivisions.clamp(2, 32);
    let x0 = bounds.x0.get();
    let y0 = bounds.y0.get();
    let x1 = bounds.x1.get();
    let y1 = bounds.y1.get();
    let vp_left = Point::new(x0, hy)?;
    let vp_right = Point::new(x1, hy)?;

    let mut path = Path::new();
    // Horizon line
    path.move_to(vp_left)?;
    path.line_to(vp_right)?;

    // Converging rays from left and right vanishing points to bottom & top edges
    for i in 0..=divs {
        let u = f64::from(i) / f64::from(divs);
        let tx = x0 + u * w;
        let bot_pt = Point::new(tx, y1)?;
        let top_pt = Point::new(tx, y0)?;
        path.move_to(vp_left)?;
        path.line_to(bot_pt)?;
        path.move_to(vp_right)?;
        path.line_to(bot_pt)?;
        path.move_to(vp_left)?;
        path.line_to(top_pt)?;
        path.move_to(vp_right)?;
        path.line_to(top_pt)?;
    }
    Ok(path)
}

/// Builds a closed Gielis Superformula [`Path`]
/// ($r(\theta) = \left(|\cos(m\theta/4)|^{n_2} + |\sin(m\theta/4)|^{n_3}\right)^{-1/n_1}$)
/// centered at `center` with outer `radius`, symmetry `m`, exponents `(n1, n2, n3)`, and `steps` (`12..=1024`).
pub fn build_superformula_path(center: Point, radius: f64, m: f64, n1: f64, n2: f64, n3: f64, steps: u32) -> Result<Path, GeomError> {
    let r_scale = F64::new(radius)?.get();
    if r_scale <= 0.0 || !m.is_finite() || !n1.is_finite() || !n2.is_finite() || !n3.is_finite() || n1.abs() < 1e-3 || steps < 12 {
        return Err(GeomError::InvalidShapeParameter("invalid superformula parameters"));
    }
    let count = steps.clamp(12, 1024);
    let cx = center.x.get();
    let cy = center.y.get();
    let mut raw_pts = Vec::with_capacity(count as usize);
    let mut max_r = 0.0_f64;

    for i in 0..count {
        let theta = std::f64::consts::TAU * (f64::from(i) / f64::from(count));
        let phi = 0.25 * m * theta;
        let t1 = phi.cos().abs().powf(n2);
        let t2 = phi.sin().abs().powf(n3);
        let sum = t1 + t2;
        let r = if sum > 1e-12 { sum.powf(-1.0 / n1) } else { 0.0 };
        if r.is_finite() {
            max_r = max_r.max(r);
            raw_pts.push((theta, r));
        }
    }
    if max_r <= 1e-12 || raw_pts.len() < 3 {
        return Err(GeomError::InvalidShapeParameter("degenerate superformula curve"));
    }
    let norm = r_scale / max_r;
    let mut path = Path::new();
    for (idx, (theta, r)) in raw_pts.into_iter().enumerate() {
        let (s, c) = theta.sin_cos();
        let pt = Point::new(cx + r * norm * c, cy + r * norm * s)?;
        if idx == 0 {
            path.move_to(pt)?;
        } else {
            path.line_to(pt)?;
        }
    }
    path.close()?;
    Ok(path)
}

/// CorelDRAW Basic Shapes flyout family (`Object -> Insert -> Basic Shapes`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BasicShapeKind {
    /// Symmetric dual-lobe heart shape.
    Heart,
    /// Orthogonal 12-vertex cross / plus sign.
    Cross,
    /// Jagged 6-vertex lightning bolt.
    Lightning,
    /// Concave crescent moon silhouette.
    CrescentMoon,
    /// Pointed-apex circular-bulb teardrop.
    Teardrop,
    /// Rhombic 4-vertex diamond.
    Diamond,
}

/// Builds a closed CorelDRAW Basic Shape [`Path`] inside `bounds` (`Basic Shapes Tool`).
pub fn build_basic_shape_path(bounds: Rect, kind: BasicShapeKind) -> Result<Path, GeomError> {
    let w = bounds.width();
    let h = bounds.height();
    if w <= 0.0 || h <= 0.0 {
        return Err(GeomError::InvalidShapeParameter("basic shape bounds must have positive width and height"));
    }
    let x0 = bounds.x0.get();
    let y0 = bounds.y0.get();
    let x1 = bounds.x1.get();
    let y1 = bounds.y1.get();
    let cx = 0.5 * (x0 + x1);
    let cy = 0.5 * (y0 + y1);

    let mut path = Path::new();
    match kind {
        BasicShapeKind::Diamond => {
            path.move_to(Point::new(cx, y0)?)?;
            path.line_to(Point::new(x1, cy)?)?;
            path.line_to(Point::new(cx, y1)?)?;
            path.line_to(Point::new(x0, cy)?)?;
            path.close()?;
        }
        BasicShapeKind::Cross => {
            let ax0 = x0 + 0.32 * w;
            let ax1 = x0 + 0.68 * w;
            let ay0 = y0 + 0.32 * h;
            let ay1 = y0 + 0.68 * h;
            for (idx, (px, py)) in [
                (ax0, y0),
                (ax1, y0),
                (ax1, ay0),
                (x1, ay0),
                (x1, ay1),
                (ax1, ay1),
                (ax1, y1),
                (ax0, y1),
                (ax0, ay1),
                (x0, ay1),
                (x0, ay0),
                (ax0, ay0),
            ]
            .into_iter()
            .enumerate()
            {
                let pt = Point::new(px, py)?;
                if idx == 0 {
                    path.move_to(pt)?;
                } else {
                    path.line_to(pt)?;
                }
            }
            path.close()?;
        }
        BasicShapeKind::Lightning => {
            for (idx, (u, v)) in [(0.58, 0.0), (0.15, 0.52), (0.48, 0.52), (0.32, 1.0), (0.85, 0.42), (0.50, 0.42)].into_iter().enumerate() {
                let pt = Point::new(x0 + u * w, y0 + v * h)?;
                if idx == 0 {
                    path.move_to(pt)?;
                } else {
                    path.line_to(pt)?;
                }
            }
            path.close()?;
        }
        BasicShapeKind::Heart => {
            let top_cleft = Point::new(cx, y0 + 0.25 * h)?;
            let bot_tip = Point::new(cx, y1)?;
            let left_mid = Point::new(x0, y0 + 0.35 * h)?;
            let right_mid = Point::new(x1, y0 + 0.35 * h)?;
            path.move_to(top_cleft)?;
            path.cubic_to(Point::new(cx - 0.15 * w, y0)?, Point::new(x0, y0)?, left_mid)?;
            path.cubic_to(Point::new(x0, y0 + 0.65 * h)?, Point::new(cx - 0.15 * w, y0 + 0.85 * h)?, bot_tip)?;
            path.cubic_to(Point::new(cx + 0.15 * w, y0 + 0.85 * h)?, Point::new(x1, y0 + 0.65 * h)?, right_mid)?;
            path.cubic_to(Point::new(x1, y0)?, Point::new(cx + 0.15 * w, y0)?, top_cleft)?;
            path.close()?;
        }
        BasicShapeKind::Teardrop => {
            let apex = Point::new(cx, y0)?;
            let bot = Point::new(cx, y1)?;
            let left = Point::new(x0, y0 + 0.62 * h)?;
            let right = Point::new(x1, y0 + 0.62 * h)?;
            path.move_to(apex)?;
            path.cubic_to(Point::new(x1, y0 + 0.30 * h)?, Point::new(x1, y0 + 0.45 * h)?, right)?;
            path.cubic_to(Point::new(x1, y0 + 0.85 * h)?, Point::new(cx + 0.28 * w, y1)?, bot)?;
            path.cubic_to(Point::new(cx - 0.28 * w, y1)?, Point::new(x0, y0 + 0.85 * h)?, left)?;
            path.cubic_to(Point::new(x0, y0 + 0.45 * h)?, Point::new(x0, y0 + 0.30 * h)?, apex)?;
            path.close()?;
        }
        BasicShapeKind::CrescentMoon => {
            let top_tip = Point::new(x0 + 0.68 * w, y0)?;
            let bot_tip = Point::new(x0 + 0.68 * w, y1)?;
            path.move_to(top_tip)?;
            path.cubic_to(Point::new(x0 - 0.12 * w, y0 + 0.12 * h)?, Point::new(x0 - 0.12 * w, y0 + 0.88 * h)?, bot_tip)?;
            path.cubic_to(Point::new(x0 + 0.26 * w, y0 + 0.72 * h)?, Point::new(x0 + 0.26 * w, y0 + 0.28 * h)?, top_tip)?;
            path.close()?;
        }
    }
    Ok(path)
}

/// Builds a procedural Lindenmayer System (L-System) turtle graphics [`Path`] starting at `origin`
/// with initial `axiom`, production `rules` (`&[(symbol, replacement)]`), `turn_angle_deg`,
/// forward `step_pt`, and `iterations` (`0..=7`).
///
/// Supported turtle commands:
/// - `'F' | 'G' | 'A' | 'B' | '0' | '1'`: Move forward by `step_pt` while drawing (`LineTo`).
/// - `'f'`: Move forward by `step_pt` without drawing (`MoveTo`).
/// - `'+'`: Turn right by `+turn_angle_deg`.
/// - `'-'`: Turn left by `-turn_angle_deg`.
/// - `'['`: Push current `(position, heading)` onto the branch stack.
/// - `']'`: Pop `(position, heading)` from the branch stack.
pub fn build_lsystem_path(
    origin: Point,
    axiom: &str,
    rules: &[(&str, &str)],
    turn_angle_deg: f64,
    step_pt: f64,
    iterations: u32,
) -> Result<Path, GeomError> {
    let turn_rad = F64::new(turn_angle_deg)?.get().to_radians();
    let step = F64::new(step_pt)?.get();
    if axiom.trim().is_empty() || step <= 0.0 || iterations > 7 {
        return Err(GeomError::InvalidShapeParameter("invalid L-system parameters"));
    }

    let mut current_str = axiom.to_owned();
    let max_symbols = 8_192usize;
    for _ in 0..iterations {
        let mut next_str = String::with_capacity(current_str.len().saturating_mul(2).min(max_symbols));
        for ch in current_str.chars() {
            let mut matched = None;
            for &(lhs, rhs) in rules {
                if lhs.starts_with(ch) && lhs.chars().count() == 1 {
                    matched = Some(rhs);
                    break;
                }
            }
            if let Some(rep) = matched {
                if next_str.len() + rep.len() > max_symbols {
                    break;
                }
                next_str.push_str(rep);
            } else {
                if next_str.len() + ch.len_utf8() > max_symbols {
                    break;
                }
                next_str.push(ch);
            }
        }
        current_str = next_str;
    }

    let mut path = Path::new();
    let mut cur_pt = origin;
    let mut heading_rad = -std::f64::consts::FRAC_PI_2;
    let mut stack: Vec<(Point, f64)> = Vec::new();
    let mut pen_down_at_cur = false;

    for ch in current_str.chars() {
        match ch {
            'F' | 'G' | 'A' | 'B' | '0' | '1' => {
                let (s, c) = heading_rad.sin_cos();
                let next_pt = Point::new(cur_pt.x.get() + step * c, cur_pt.y.get() + step * s)?;
                if !pen_down_at_cur {
                    path.move_to(cur_pt)?;
                    pen_down_at_cur = true;
                }
                path.line_to(next_pt)?;
                cur_pt = next_pt;
            }
            'f' => {
                let (s, c) = heading_rad.sin_cos();
                cur_pt = Point::new(cur_pt.x.get() + step * c, cur_pt.y.get() + step * s)?;
                pen_down_at_cur = false;
            }
            '+' => {
                heading_rad += turn_rad;
            }
            '-' => {
                heading_rad -= turn_rad;
            }
            '[' => {
                if stack.len() < 256 {
                    stack.push((cur_pt, heading_rad));
                }
            }
            ']' => {
                if let Some((saved_pt, saved_heading)) = stack.pop() {
                    cur_pt = saved_pt;
                    heading_rad = saved_heading;
                    pen_down_at_cur = false;
                }
            }
            _ => {}
        }
    }

    if path.elements.is_empty() {
        return Err(GeomError::InvalidShapeParameter("L-system produced no drawable segments"));
    }
    Ok(path)
}

/// Builds a compound Voronoi cellular mosaic [`Path`] inside `bounds` from `seeds` (`>= 2`)
/// with inset cell separation `cell_gap_pt >= 0.0` (`Effects -> Creative -> Voronoi Mosaic`).
pub fn build_voronoi_mosaic_path(bounds: Rect, seeds: &[Point], cell_gap_pt: f64) -> Result<Path, GeomError> {
    let w = bounds.width();
    let h = bounds.height();
    let gap = F64::new(cell_gap_pt)?.get();
    if w <= 0.0 || h <= 0.0 || seeds.len() < 2 || gap < 0.0 {
        return Err(GeomError::InvalidShapeParameter("invalid Voronoi mosaic parameters"));
    }
    let x0 = bounds.x0.get();
    let y0 = bounds.y0.get();
    let x1 = bounds.x1.get();
    let y1 = bounds.y1.get();
    let base_cell = vec![Point::new(x0, y0)?, Point::new(x1, y0)?, Point::new(x1, y1)?, Point::new(x0, y1)?];

    let clip_half_plane = |poly: &[Point], sx: f64, sy: f64, nx: f64, ny: f64, max_proj: f64| -> Result<Vec<Point>, GeomError> {
        if poly.len() < 3 {
            return Ok(Vec::new());
        }
        let eval = |p: Point| -> f64 { (p.x.get() - sx) * nx + (p.y.get() - sy) * ny - max_proj };
        let mut out = Vec::with_capacity(poly.len() + 2);
        let mut prev = poly[poly.len() - 1];
        let mut d_prev = eval(prev);
        for &curr in poly {
            let d_curr = eval(curr);
            let prev_in = d_prev <= 1e-9;
            let curr_in = d_curr <= 1e-9;
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
    };

    let mut path = Path::new();
    for (i, &si) in seeds.iter().enumerate() {
        let mut cell = base_cell.clone();
        let six = si.x.get();
        let siy = si.y.get();
        for (j, &sj) in seeds.iter().enumerate() {
            if i == j {
                continue;
            }
            let dx = sj.x.get() - six;
            let dy = sj.y.get() - siy;
            let dist = dx.hypot(dy);
            if dist <= 1e-6 {
                continue;
            }
            let nx = dx / dist;
            let ny = dy / dist;
            let max_proj = 0.5 * dist - 0.5 * gap;
            if max_proj <= 1e-6 {
                cell.clear();
                break;
            }
            cell = clip_half_plane(&cell, six, siy, nx, ny, max_proj)?;
            if cell.len() < 3 {
                break;
            }
        }
        if cell.len() >= 3 {
            path.move_to(cell[0])?;
            for &pt in &cell[1..] {
                path.line_to(pt)?;
            }
            path.close()?;
        }
    }
    if path.elements.is_empty() {
        return Err(GeomError::InvalidShapeParameter("Voronoi mosaic produced no non-empty cells"));
    }
    Ok(path)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod m27_tests {
    use super::*;
    use crate::path::PathElement;

    #[test]
    fn build_pointillizer_and_impact_rays_paths_generate_closed_subpaths() {
        let rect = Rect::new(0.0, 0.0, 100.0, 80.0).unwrap();
        let path = build_pointillizer_path(rect, 4, 3, 0.75).unwrap();
        let closes = path.elements.iter().filter(|e| matches!(e, PathElement::Close)).count();
        assert_eq!(closes, 12);

        let rays = build_impact_rays_path(Point::new(50.0, 50.0).unwrap(), 10.0, 60.0, 8).unwrap();
        let ray_closes = rays.elements.iter().filter(|e| matches!(e, PathElement::Close)).count();
        assert_eq!(ray_closes, 8);

        let star = build_complex_star_path(Point::new(50.0, 50.0).unwrap(), 40.0, 7, 3).unwrap();
        assert_eq!(star.elements.len(), 8);

        let poly =
            build_polyline_path(&[Point::new(0.0, 0.0).unwrap(), Point::new(40.0, 0.0).unwrap(), Point::new(20.0, 30.0).unwrap()], true).unwrap();
        assert_eq!(poly.elements.len(), 4);

        let gear = build_gear_wheel_path(Point::new(50.0, 50.0).unwrap(), 40.0, 30.0, 8).unwrap();
        let gear_closes = gear.elements.iter().filter(|e| matches!(e, PathElement::Close)).count();
        assert_eq!(gear_closes, 2);

        let arrow = build_block_arrow_path(Point::new(0.0, 50.0).unwrap(), Point::new(100.0, 50.0).unwrap(), 20.0, 40.0, 30.0).unwrap();
        assert_eq!(arrow.elements.len(), 8);

        let banner = build_banner_ribbon_path(Rect::new(0.0, 0.0, 200.0, 60.0).unwrap(), 30.0, 12.0).unwrap();
        let banner_closes = banner.elements.iter().filter(|e| matches!(e, PathElement::Close)).count();
        assert_eq!(banner_closes, 3);

        for kind in [
            FlowchartSymbolKind::Process,
            FlowchartSymbolKind::Decision,
            FlowchartSymbolKind::Terminator,
            FlowchartSymbolKind::Data,
            FlowchartSymbolKind::Document,
        ] {
            let fc = build_flowchart_symbol_path(Rect::new(0.0, 0.0, 120.0, 60.0).unwrap(), kind).unwrap();
            assert!(fc.elements.iter().any(|e| matches!(e, PathElement::Close)));
        }

        let rose = build_rose_guilloche_path(Point::new(100.0, 100.0).unwrap(), 50.0, 5, 12).unwrap();
        assert!(rose.elements.iter().any(|e| matches!(e, PathElement::Close)));

        let barcode = build_barcode_path(Rect::new(0.0, 0.0, 180.0, 60.0).unwrap(), "012345678905").unwrap();
        let bar_closes = barcode.elements.iter().filter(|e| matches!(e, PathElement::Close)).count();
        assert!(bar_closes >= 10);

        let pgrid = build_perspective_grid_path(Rect::new(0.0, 0.0, 200.0, 120.0).unwrap(), 50.0, 4).unwrap();
        assert!(pgrid.elements.len() > 20);

        let sf = build_superformula_path(Point::new(100.0, 100.0).unwrap(), 50.0, 6.0, 1.0, 7.0, 8.0, 64).unwrap();
        assert!(sf.elements.iter().any(|e| matches!(e, PathElement::Close)));

        for kind in [
            BasicShapeKind::Heart,
            BasicShapeKind::Cross,
            BasicShapeKind::Lightning,
            BasicShapeKind::CrescentMoon,
            BasicShapeKind::Teardrop,
            BasicShapeKind::Diamond,
        ] {
            let bs = build_basic_shape_path(Rect::new(10.0, 20.0, 110.0, 100.0).unwrap(), kind).unwrap();
            assert!(bs.elements.iter().any(|e| matches!(e, PathElement::Close)));
        }

        let lsys = build_lsystem_path(Point::new(50.0, 50.0).unwrap(), "F", &[("F", "F+F-F-F+F")], 90.0, 10.0, 2).unwrap();
        assert!(lsys.elements.len() >= 10);

        let voronoi = build_voronoi_mosaic_path(
            Rect::new(0.0, 0.0, 120.0, 100.0).unwrap(),
            &[Point::new(30.0, 30.0).unwrap(), Point::new(90.0, 30.0).unwrap(), Point::new(60.0, 75.0).unwrap()],
            2.0,
        )
        .unwrap();
        let vor_closes = voronoi.elements.iter().filter(|e| matches!(e, PathElement::Close)).count();
        assert_eq!(vor_closes, 3);
    }
}
