//! 2D affine transformation matrix (`Affine2`).

use serde::{Deserialize, Serialize};

use crate::f64_canon::{EPS, F64, GeomError, Point, Rect};

/// 2×3 2D affine transformation matrix using the PostScript/SVG convention:
/// `(x', y') = (a*x + c*y + tx, b*x + d*y + ty)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Affine2 {
    /// Scale X (`m11`).
    pub a: F64,
    /// Shear Y (`m12`).
    pub b: F64,
    /// Shear X (`m21`).
    pub c: F64,
    /// Scale Y (`m22`).
    pub d: F64,
    /// Translation X (`dx`).
    pub tx: F64,
    /// Translation Y (`dy`).
    pub ty: F64,
}

impl Default for Affine2 {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Affine2 {
    /// Identity transform (`[1, 0, 0, 1, 0, 0]`).
    pub const IDENTITY: Self = Self { a: F64::ONE, b: F64::ZERO, c: F64::ZERO, d: F64::ONE, tx: F64::ZERO, ty: F64::ZERO };

    /// Constructs a checked `Affine2` from six `f64` coefficients.
    pub fn new(a: f64, b: f64, c: f64, d: f64, tx: f64, ty: f64) -> Result<Self, GeomError> {
        Ok(Self { a: F64::new(a)?, b: F64::new(b)?, c: F64::new(c)?, d: F64::new(d)?, tx: F64::new(tx)?, ty: F64::new(ty)? })
    }

    /// Translation by `(tx, ty)`.
    pub fn translation(tx: f64, ty: f64) -> Result<Self, GeomError> {
        Self::new(1.0, 0.0, 0.0, 1.0, tx, ty)
    }

    /// Non-uniform scaling by `(sx, sy)` about the origin.
    pub fn scaling(sx: f64, sy: f64) -> Result<Self, GeomError> {
        Self::new(sx, 0.0, 0.0, sy, 0.0, 0.0)
    }

    /// Rotation by `radians` about the origin.
    pub fn rotation_rad(radians: f64) -> Result<Self, GeomError> {
        if !radians.is_finite() {
            return Err(GeomError::NonFiniteCoordinate);
        }
        let (s, c) = radians.sin_cos();
        Self::new(c, s, -s, c, 0.0, 0.0)
    }

    /// Rotation by `degrees` about the origin.
    pub fn rotation_deg(degrees: f64) -> Result<Self, GeomError> {
        if !degrees.is_finite() {
            return Err(GeomError::NonFiniteCoordinate);
        }
        Self::rotation_rad(degrees.to_radians())
    }

    /// Skew along the X and Y axes in degrees.
    pub fn skew_deg(skew_x_deg: f64, skew_y_deg: f64) -> Result<Self, GeomError> {
        if !skew_x_deg.is_finite() || !skew_y_deg.is_finite() {
            return Err(GeomError::NonFiniteCoordinate);
        }
        let tx = skew_x_deg.to_radians().tan();
        let ty = skew_y_deg.to_radians().tan();
        Self::new(1.0, ty, tx, 1.0, 0.0, 0.0)
    }

    /// Returns `self` followed by `next` (`next * self`), so applying the returned matrix to `p`
    /// is equivalent to `next.apply_point(self.apply_point(p))`.
    pub fn then(self, next: Self) -> Result<Self, GeomError> {
        let a1 = self.a.get();
        let b1 = self.b.get();
        let c1 = self.c.get();
        let d1 = self.d.get();
        let tx1 = self.tx.get();
        let ty1 = self.ty.get();

        let a2 = next.a.get();
        let b2 = next.b.get();
        let c2 = next.c.get();
        let d2 = next.d.get();
        let tx2 = next.tx.get();
        let ty2 = next.ty.get();

        Self::new(a2 * a1 + c2 * b1, b2 * a1 + d2 * b1, a2 * c1 + c2 * d1, b2 * c1 + d2 * d1, a2 * tx1 + c2 * ty1 + tx2, b2 * tx1 + d2 * ty1 + ty2)
    }

    /// Pre-composes `prior` before `self` (`self * prior`).
    pub fn pre(self, prior: Self) -> Result<Self, GeomError> {
        prior.then(self)
    }

    /// Applies `self` to a [`Point`].
    pub fn apply_point(self, p: Point) -> Result<Point, GeomError> {
        let x = p.x.get();
        let y = p.y.get();
        Point::new(self.a.get() * x + self.c.get() * y + self.tx.get(), self.b.get() * x + self.d.get() * y + self.ty.get())
    }

    /// Transforms an axis-aligned [`Rect`] and returns the conservative axis-aligned bounding box of its 4 transformed corners.
    pub fn apply_rect_bbox(self, r: Rect) -> Result<Rect, GeomError> {
        let c0 = self.apply_point(Point::from_f64(r.x0, r.y0))?;
        let c1 = self.apply_point(Point::from_f64(r.x1, r.y0))?;
        let c2 = self.apply_point(Point::from_f64(r.x1, r.y1))?;
        let c3 = self.apply_point(Point::from_f64(r.x0, r.y1))?;
        let mut bbox = Rect::from_points(c0, c1)?;
        bbox = bbox.union(Rect::from_points(c2, c3)?)?;
        Ok(bbox)
    }

    /// Returns the 2×2 linear determinant `a * d - b * c`.
    #[must_use]
    pub fn determinant(self) -> f64 {
        self.a.get() * self.d.get() - self.b.get() * self.c.get()
    }

    /// Returns `true` when the transform flips winding orientation (`determinant < 0`).
    #[must_use]
    pub fn flips_orientation(self) -> bool {
        self.determinant() < 0.0
    }

    /// Computes the inverse affine transformation, or returns [`GeomError::SingularTransform`]
    /// if the matrix is singular (`|det| <= EPS`).
    pub fn invert(self) -> Result<Self, GeomError> {
        let det = self.determinant();
        if !det.is_finite() || det.abs() <= EPS {
            return Err(GeomError::SingularTransform);
        }
        let inv_det = 1.0 / det;
        let a = self.d.get() * inv_det;
        let b = -self.b.get() * inv_det;
        let c = -self.c.get() * inv_det;
        let d = self.a.get() * inv_det;
        let tx = -(a * self.tx.get() + c * self.ty.get());
        let ty = -(b * self.tx.get() + d * self.ty.get());
        Self::new(a, b, c, d, tx, ty)
    }
}
