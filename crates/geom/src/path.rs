//! Editable path model (`Path`, `PathElement`, `FillRule`, `NodeKind`).

use serde::{Deserialize, Serialize};

use crate::affine::Affine2;
use crate::f64_canon::{GeomError, Point, Rect};

/// Maximum number of elements allowed in a single [`Path`] to guard against hostile inputs.
pub const MAX_PATH_ELEMENTS: usize = 1_000_000;

/// Winding fill rule for closed subpaths and compound paths.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FillRule {
    /// Non-zero winding rule (CorelDRAW default for standard curves).
    #[default]
    NonZero,
    /// Even-odd alternating fill rule.
    EvenOdd,
}

/// Curve node continuity type in CorelDRAW's Shape Tool (`F10`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    /// Sharp corner (in/out control handles move independently).
    #[default]
    Cusp,
    /// Smooth tangent (in/out control handles remain collinear with independent lengths).
    Smooth,
    /// Symmetrical tangent (in/out control handles remain collinear with equal lengths).
    Symmetrical,
}

/// A single drawing verb in a [`Path`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum PathElement {
    /// Starts a new subpath at `Point`.
    MoveTo(Point),
    /// Straight line segment to `Point`.
    LineTo(Point),
    /// Quadratic Bézier curve with control point `ctrl` and endpoint `end`.
    QuadTo {
        /// Quadratic control point.
        ctrl: Point,
        /// Segment endpoint.
        end: Point,
    },
    /// Cubic Bézier curve with control points `c1`, `c2` and endpoint `end`.
    CubicTo {
        /// First cubic control point.
        c1: Point,
        /// Second cubic control point.
        c2: Point,
        /// Segment endpoint.
        end: Point,
    },
    /// Closes the current subpath back to its initial `MoveTo` point.
    Close,
}

/// Vector path composed of one or more subpaths and a [`FillRule`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Path {
    /// Ordered sequence of path elements.
    pub elements: Vec<PathElement>,
    /// Winding fill rule.
    #[serde(default)]
    pub fill_rule: FillRule,
}

impl Path {
    /// Creates an empty path with the default [`FillRule::NonZero`].
    #[must_use]
    pub fn new() -> Self {
        Self { elements: Vec::new(), fill_rule: FillRule::NonZero }
    }

    /// Creates a path from a checked element list and [`FillRule`].
    pub fn from_elements(elements: Vec<PathElement>, fill_rule: FillRule) -> Result<Self, GeomError> {
        if elements.len() > MAX_PATH_ELEMENTS {
            return Err(GeomError::PathTooLarge { max: MAX_PATH_ELEMENTS });
        }
        Ok(Self { elements, fill_rule })
    }

    /// Appends a [`PathElement`], returning [`GeomError::PathTooLarge`] if [`MAX_PATH_ELEMENTS`] is exceeded.
    pub fn push(&mut self, el: PathElement) -> Result<(), GeomError> {
        if self.elements.len() >= MAX_PATH_ELEMENTS {
            return Err(GeomError::PathTooLarge { max: MAX_PATH_ELEMENTS });
        }
        self.elements.push(el);
        Ok(())
    }

    /// Appends `MoveTo(p)`.
    pub fn move_to(&mut self, p: Point) -> Result<(), GeomError> {
        self.push(PathElement::MoveTo(p))
    }

    /// Appends `LineTo(p)`.
    pub fn line_to(&mut self, p: Point) -> Result<(), GeomError> {
        self.push(PathElement::LineTo(p))
    }

    /// Appends `QuadTo { ctrl, end }`.
    pub fn quad_to(&mut self, ctrl: Point, end: Point) -> Result<(), GeomError> {
        self.push(PathElement::QuadTo { ctrl, end })
    }

    /// Appends `CubicTo { c1, c2, end }`.
    pub fn cubic_to(&mut self, c1: Point, c2: Point, end: Point) -> Result<(), GeomError> {
        self.push(PathElement::CubicTo { c1, c2, end })
    }

    /// Appends `Close`.
    pub fn close(&mut self) -> Result<(), GeomError> {
        self.push(PathElement::Close)
    }

    /// Returns `true` if the path contains no elements.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    /// Computes the conservative control-point bounding box of the path, or `None` if no points exist.
    #[must_use]
    pub fn bounds(&self) -> Option<Rect> {
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        let mut any = false;

        let mut include = |p: Point| {
            let x = p.x.get();
            let y = p.y.get();
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
            any = true;
        };

        for el in &self.elements {
            match *el {
                PathElement::MoveTo(p) | PathElement::LineTo(p) => include(p),
                PathElement::QuadTo { ctrl, end } => {
                    include(ctrl);
                    include(end);
                }
                PathElement::CubicTo { c1, c2, end } => {
                    include(c1);
                    include(c2);
                    include(end);
                }
                PathElement::Close => {}
            }
        }

        if any { Rect::new(min_x, min_y, max_x, max_y).ok() } else { None }
    }

    /// Applies an [`Affine2`] transform to every point and control point in the path.
    ///
    /// Because Bézier curves are affine-invariant, transforming their control points
    /// transforms the curve identically.
    pub fn transform(&self, m: Affine2) -> Result<Self, GeomError> {
        let mut out = Vec::with_capacity(self.elements.len());
        for &el in &self.elements {
            let transformed = match el {
                PathElement::MoveTo(p) => PathElement::MoveTo(m.apply_point(p)?),
                PathElement::LineTo(p) => PathElement::LineTo(m.apply_point(p)?),
                PathElement::QuadTo { ctrl, end } => PathElement::QuadTo { ctrl: m.apply_point(ctrl)?, end: m.apply_point(end)? },
                PathElement::CubicTo { c1, c2, end } => {
                    PathElement::CubicTo { c1: m.apply_point(c1)?, c2: m.apply_point(c2)?, end: m.apply_point(end)? }
                }
                PathElement::Close => PathElement::Close,
            };
            out.push(transformed);
        }
        Ok(Self { elements: out, fill_rule: self.fill_rule })
    }
}
