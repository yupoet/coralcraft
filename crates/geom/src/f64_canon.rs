//! Canonical finite `f64` wrapper and 2D primitive types (`Point`, `Size`, `Rect`).

use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

/// Tolerance used for geometric comparisons and degenerate chord checks.
pub const EPS: f64 = 1e-9;

/// Errors returned by `coralcraft-geom` constructors and operations.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum GeomError {
    /// Coordinate or scalar is `NaN` or `±Infinity`.
    #[error("coordinate or scalar is not finite (NaN or Infinity)")]
    NonFiniteCoordinate,
    /// Coordinate exceeds the representable `i64` tick range after unit scaling.
    #[error("coordinate exceeds representable i64 CDR tick range")]
    CoordinateOutOfRange,
    /// Affine transformation matrix is singular (determinant near zero) and cannot be inverted.
    #[error("affine transform is singular and cannot be inverted")]
    SingularTransform,
    /// Path exceeds `MAX_PATH_ELEMENTS`.
    #[error("path exceeds maximum element count ({max})")]
    PathTooLarge {
        /// Maximum allowed path elements.
        max: usize,
    },
    /// Invalid shape parameter (e.g. negative size or fewer than 3 polygon sides).
    #[error("invalid shape parameter: {0}")]
    InvalidShapeParameter(&'static str),
}

/// Canonical finite 64-bit floating-point scalar.
///
/// Guarantees:
/// - Never `NaN` or `±Infinity`.
/// - `-0.0` is canonicalized to `+0.0` (`0.0_f64.to_bits() == 0`).
/// - Implements [`Eq`], [`Ord`], and [`Hash`] via canonical IEEE-754 bit patterns.
#[derive(Clone, Copy, Debug)]
pub struct F64(f64);

impl Default for F64 {
    fn default() -> Self {
        Self::ZERO
    }
}

impl F64 {
    /// Canonical `+0.0`.
    pub const ZERO: Self = Self(0.0);
    /// Canonical `0.5`.
    pub const HALF: Self = Self(0.5);
    /// Canonical `1.0`.
    pub const ONE: Self = Self(1.0);

    /// Constructs a canonical finite `F64`, rejecting `NaN` and `±Infinity` and normalizing `-0.0` to `+0.0`.
    pub fn new(v: f64) -> Result<Self, GeomError> {
        if !v.is_finite() {
            return Err(GeomError::NonFiniteCoordinate);
        }
        Ok(Self(if v == 0.0 { 0.0 } else { v }))
    }

    /// Re-canonicalizes `-0.0` to `+0.0`.
    #[must_use]
    pub const fn canon(self) -> Self {
        Self(if self.0 == 0.0 { 0.0 } else { self.0 })
    }

    /// Returns the underlying `f64` value.
    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }

    /// Quantizes onto an exact binary grid (`2^-20`, ~0.95 ppm) so `q(q(x)) == q(x)`.
    #[must_use]
    pub fn quantize_2_20(self) -> Self {
        const SCALE: f64 = 1_048_576.0;
        let v = (self.0 * SCALE).round() / SCALE;
        if v.is_finite() { Self(if v == 0.0 { 0.0 } else { v }) } else { self.canon() }
    }
}

impl PartialEq for F64 {
    fn eq(&self, other: &Self) -> bool {
        self.0.to_bits() == other.0.to_bits()
    }
}

impl Eq for F64 {}

impl Hash for F64 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.to_bits().hash(state);
    }
}

impl PartialOrd for F64 {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for F64 {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.total_cmp(&other.0)
    }
}

impl fmt::Display for F64 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for F64 {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_f64(self.canon().0)
    }
}

impl<'de> Deserialize<'de> for F64 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = f64::deserialize(deserializer)?;
        Self::new(raw).map_err(serde::de::Error::custom)
    }
}

/// 2D point in document coordinates (points, `1/72 inch`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Point {
    /// Horizontal coordinate.
    pub x: F64,
    /// Vertical coordinate.
    pub y: F64,
}

impl Point {
    /// Origin `(0, 0)`.
    pub const ZERO: Self = Self { x: F64::ZERO, y: F64::ZERO };

    /// Creates a checked `Point` from raw `f64` coordinates.
    pub fn new(x: f64, y: f64) -> Result<Self, GeomError> {
        Ok(Self { x: F64::new(x)?, y: F64::new(y)? })
    }

    /// Creates a `Point` from validated [`F64`] coordinates.
    #[must_use]
    pub const fn from_f64(x: F64, y: F64) -> Self {
        Self { x: x.canon(), y: y.canon() }
    }

    /// Euclidean distance to `other`.
    #[must_use]
    pub fn distance(self, other: Self) -> f64 {
        (self.x.get() - other.x.get()).hypot(self.y.get() - other.y.get())
    }

    /// Midpoint between `self` and `other`.
    pub fn midpoint(self, other: Self) -> Result<Self, GeomError> {
        Self::new(0.5 * (self.x.get() + other.x.get()), 0.5 * (self.y.get() + other.y.get()))
    }
}

/// 2D size (`width`, `height`) in document coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Size {
    /// Width (must be finite and non-negative for valid page/shape dimensions).
    pub width: F64,
    /// Height (must be finite and non-negative for valid page/shape dimensions).
    pub height: F64,
}

impl Size {
    /// Creates a checked `Size` from raw `f64` dimensions, rejecting negative or non-finite values.
    pub fn new(width: f64, height: f64) -> Result<Self, GeomError> {
        let w = F64::new(width)?;
        let h = F64::new(height)?;
        if w.get() < 0.0 || h.get() < 0.0 {
            return Err(GeomError::InvalidShapeParameter("size dimensions must be non-negative"));
        }
        Ok(Self { width: w, height: h })
    }
}

/// Axis-aligned bounding rectangle `(x0, y0) .. (x1, y1)`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Rect {
    /// Minimum X coordinate.
    pub x0: F64,
    /// Minimum Y coordinate.
    pub y0: F64,
    /// Maximum X coordinate.
    pub x1: F64,
    /// Maximum Y coordinate.
    pub y1: F64,
}

/// Alias for [`Rect`] used for bounding boxes in document nodes.
pub type BBox = Rect;

impl Rect {
    /// Creates a normalized axis-aligned rectangle (`x0 <= x1`, `y0 <= y1`) from two corner coordinates.
    pub fn new(x0: f64, y0: f64, x1: f64, y1: f64) -> Result<Self, GeomError> {
        let ax0 = F64::new(x0.min(x1))?;
        let ay0 = F64::new(y0.min(y1))?;
        let ax1 = F64::new(x0.max(x1))?;
        let ay1 = F64::new(y0.max(y1))?;
        Ok(Self { x0: ax0, y0: ay0, x1: ax1, y1: ay1 })
    }

    /// Creates a normalized rectangle from two [`Point`] corners.
    pub fn from_points(a: Point, b: Point) -> Result<Self, GeomError> {
        Self::new(a.x.get(), a.y.get(), b.x.get(), b.y.get())
    }

    /// Width of the rectangle (`x1 - x0`).
    #[must_use]
    pub fn width(self) -> f64 {
        (self.x1.get() - self.x0.get()).max(0.0)
    }

    /// Height of the rectangle (`y1 - y0`).
    #[must_use]
    pub fn height(self) -> f64 {
        (self.y1.get() - self.y0.get()).max(0.0)
    }

    /// Center point of the rectangle.
    pub fn center(self) -> Result<Point, GeomError> {
        Point::new(0.5 * (self.x0.get() + self.x1.get()), 0.5 * (self.y0.get() + self.y1.get()))
    }

    /// Returns `true` if `p` lies inside or on the boundary of `self`.
    #[must_use]
    pub fn contains(self, p: Point) -> bool {
        let px = p.x.get();
        let py = p.y.get();
        px >= self.x0.get() && px <= self.x1.get() && py >= self.y0.get() && py <= self.y1.get()
    }

    /// Returns `true` if `self` and `other` overlap.
    #[must_use]
    pub fn intersects(self, other: Self) -> bool {
        self.x0.get() <= other.x1.get() && self.x1.get() >= other.x0.get() && self.y0.get() <= other.y1.get() && self.y1.get() >= other.y0.get()
    }

    /// Returns the smallest rectangle containing both `self` and `other`.
    pub fn union(self, other: Self) -> Result<Self, GeomError> {
        Self::new(
            self.x0.get().min(other.x0.get()),
            self.y0.get().min(other.y0.get()),
            self.x1.get().max(other.x1.get()),
            self.y1.get().max(other.y1.get()),
        )
    }
}

/// Union of two optional bounding rectangles (adapted from `vectorcraft-geom`).
pub fn union_opt(a: Option<Rect>, b: Option<Rect>) -> Option<Rect> {
    match (a, b) {
        (Some(ra), Some(rb)) => ra.union(rb).ok(),
        (Some(ra), None) => Some(ra),
        (None, Some(rb)) => Some(rb),
        (None, None) => None,
    }
}

/// The 9 reference points of a rectangle (CorelDRAW 9-point Object Origin locator),
/// ordered row-major from top-left (`0`) through center (`4`) to bottom-right (`8`).
pub fn reference_point(r: Rect, index: usize) -> Result<Point, GeomError> {
    let c = r.center()?;
    let xs = [r.x0.get(), c.x.get(), r.x1.get()];
    let ys = [r.y0.get(), c.y.get(), r.y1.get()];
    Point::new(xs.get(index % 3).copied().unwrap_or(0.0), ys.get((index / 3).min(2)).copied().unwrap_or(0.0))
}

/// Normalizes an angle in degrees into `(-180.0, 180.0]`.
#[must_use]
pub fn normalize_deg(mut angle_deg: f64) -> f64 {
    angle_deg %= 360.0;
    if angle_deg <= -180.0 {
        angle_deg += 360.0;
    } else if angle_deg > 180.0 {
        angle_deg -= 360.0;
    }
    if angle_deg == 0.0 { 0.0 } else { angle_deg }
}

/// Snaps a 2D displacement vector `(dx, dy)` to the nearest multiple of `step_deg`
/// (used for `Ctrl`-constrained angle dragging in CorelDRAW, default `15.0` degrees).
#[must_use]
pub fn constrain_angle(dx: f64, dy: f64, step_deg: f64) -> (f64, f64) {
    constrain_angle_from(dx, dy, step_deg, 0.0)
}

/// Snaps `(dx, dy)` in steps of `step_deg` measured from `base_deg`.
#[must_use]
pub fn constrain_angle_from(dx: f64, dy: f64, step_deg: f64, base_deg: f64) -> (f64, f64) {
    let len = dx.hypot(dy);
    if len < EPS || !step_deg.is_finite() || step_deg.abs() < EPS {
        return (dx, dy);
    }
    let step = step_deg.to_radians();
    let base = -base_deg.to_radians();
    let angle = base + ((dy.atan2(dx) - base) / step).round() * step;
    (angle.cos() * len, angle.sin() * len)
}
