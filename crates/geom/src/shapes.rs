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
