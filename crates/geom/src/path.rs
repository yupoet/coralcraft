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

/// Sampled position and normalized tangent along a [`Path`] at a cumulative arc length.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PathSample {
    /// Sampled point in path coordinates.
    pub point: Point,
    /// Unit tangent vector `(dx, dy)` along the path direction.
    pub tangent: (f64, f64),
    /// Tangent angle in degrees (`atan2(dy, dx).to_degrees()`).
    pub angle_deg: f64,
    /// Cumulative arc length in points from the start of the path.
    pub s_pt: f64,
}

/// Cumulative arc-length lookup table (LUT) for uniform arc-length parameterization of a [`Path`].
#[derive(Clone, Debug, PartialEq)]
pub struct ArcLengthTable {
    samples: Vec<PathSample>,
    total_length: f64,
}

impl ArcLengthTable {
    /// Builds a cumulative arc-length lookup table from a [`Path`] using adaptive flattening.
    pub fn from_path(path: &Path) -> Result<Self, GeomError> {
        let polys = crate::flatten::flatten_path(path, &crate::flatten::FlattenOpts::default())?;
        let mut samples = Vec::new();
        let mut cumulative = 0.0;

        for poly in &polys {
            let Some((&first_pt, rest)) = poly.points.split_first() else {
                continue;
            };
            let mut prev = first_pt;
            if samples.is_empty() {
                let first_tan = if let Some(&second_pt) = rest.first() {
                    let dx = second_pt.x.get() - prev.x.get();
                    let dy = second_pt.y.get() - prev.y.get();
                    let d = dx.hypot(dy);
                    if d > 1e-9 { (dx / d, dy / d) } else { (1.0, 0.0) }
                } else {
                    (1.0, 0.0)
                };
                samples.push(PathSample { point: prev, tangent: first_tan, angle_deg: first_tan.1.atan2(first_tan.0).to_degrees(), s_pt: 0.0 });
            }
            for &pt in rest {
                let dx = pt.x.get() - prev.x.get();
                let dy = pt.y.get() - prev.y.get();
                let ds = dx.hypot(dy);
                if ds > 1e-9 {
                    cumulative += ds;
                    let tan = (dx / ds, dy / ds);
                    samples.push(PathSample { point: pt, tangent: tan, angle_deg: tan.1.atan2(tan.0).to_degrees(), s_pt: cumulative });
                    prev = pt;
                }
            }
            if poly.closed && !rest.is_empty() {
                let first = first_pt;
                let dx = first.x.get() - prev.x.get();
                let dy = first.y.get() - prev.y.get();
                let ds = dx.hypot(dy);
                if ds > 1e-9 {
                    cumulative += ds;
                    let tan = (dx / ds, dy / ds);
                    samples.push(PathSample { point: first, tangent: tan, angle_deg: tan.1.atan2(tan.0).to_degrees(), s_pt: cumulative });
                }
            }
        }

        Ok(Self { samples, total_length: cumulative })
    }

    /// Returns the total arc length of the path in typographic points.
    #[must_use]
    pub fn total_length(&self) -> f64 {
        self.total_length
    }

    /// Samples a point and tangent at normalized arc-length parameter `u in 0.0..=1.0`.
    #[must_use]
    pub fn sample_normalized(&self, u: f64) -> Option<PathSample> {
        if self.samples.is_empty() {
            return None;
        }
        if self.total_length <= 1e-9 || self.samples.len() == 1 {
            return self.samples.first().copied();
        }
        let target_s = u.clamp(0.0, 1.0) * self.total_length;
        let idx = self.samples.partition_point(|s| s.s_pt < target_s);
        if idx == 0 {
            return self.samples.first().copied();
        }
        if idx >= self.samples.len() {
            return self.samples.last().copied();
        }
        let s0 = *self.samples.get(idx.saturating_sub(1))?;
        let s1 = *self.samples.get(idx)?;
        let span = (s1.s_pt - s0.s_pt).max(1e-9);
        let alpha = ((target_s - s0.s_pt) / span).clamp(0.0, 1.0);
        let x = s0.point.x.get() + alpha * (s1.point.x.get() - s0.point.x.get());
        let y = s0.point.y.get() + alpha * (s1.point.y.get() - s0.point.y.get());
        let point = Point::new(x, y).unwrap_or(s1.point);
        Some(PathSample { point, tangent: s1.tangent, angle_deg: s1.angle_deg, s_pt: target_s })
    }
}

/// Applies a 4-corner bilinear Envelope warp (`corners = [TL, TR, BR, BL]`) to a [`Path`]
/// relative to `src_bbox`.
pub fn warp_path_bilinear(path: &Path, src_bbox: Rect, corners: [Point; 4]) -> Result<Path, GeomError> {
    let w = src_bbox.width().max(1e-9);
    let h = src_bbox.height().max(1e-9);
    let min_x = src_bbox.x0.get();
    let min_y = src_bbox.y0.get();
    let [p00, p10, p11, p01] = corners;

    let warp_pt = |pt: Point| -> Result<Point, GeomError> {
        let u = (pt.x.get() - min_x) / w;
        let v = (pt.y.get() - min_y) / h;
        let w00 = (1.0 - u) * (1.0 - v);
        let w10 = u * (1.0 - v);
        let w11 = u * v;
        let w01 = (1.0 - u) * v;
        let x = w00 * p00.x.get() + w10 * p10.x.get() + w11 * p11.x.get() + w01 * p01.x.get();
        let y = w00 * p00.y.get() + w10 * p10.y.get() + w11 * p11.y.get() + w01 * p01.y.get();
        Point::new(x, y)
    };

    let mut out = Path { elements: Vec::with_capacity(path.elements.len()), fill_rule: path.fill_rule };
    for &el in &path.elements {
        let mapped = match el {
            PathElement::MoveTo(p) => PathElement::MoveTo(warp_pt(p)?),
            PathElement::LineTo(p) => PathElement::LineTo(warp_pt(p)?),
            PathElement::QuadTo { ctrl, end } => PathElement::QuadTo { ctrl: warp_pt(ctrl)?, end: warp_pt(end)? },
            PathElement::CubicTo { c1, c2, end } => PathElement::CubicTo { c1: warp_pt(c1)?, c2: warp_pt(c2)?, end: warp_pt(end)? },
            PathElement::Close => PathElement::Close,
        };
        out.elements.push(mapped);
    }
    Ok(out)
}

/// Applies a 4-corner 2-point vanishing projective Perspective homography (`corners = [TL, TR, BR, BL]`,
/// Heckbert 1989 §3.2) to a [`Path`] relative to `src_bbox`.
pub fn warp_path_perspective(path: &Path, src_bbox: Rect, corners: [Point; 4]) -> Result<Path, GeomError> {
    let w = src_bbox.width().max(1e-9);
    let h = src_bbox.height().max(1e-9);
    let min_x = src_bbox.x0.get();
    let min_y = src_bbox.y0.get();
    let [p00, p10, p11, p01] = corners;

    let x00 = p00.x.get();
    let y00 = p00.y.get();
    let x10 = p10.x.get();
    let y10 = p10.y.get();
    let x11 = p11.x.get();
    let y11 = p11.y.get();
    let x01 = p01.x.get();
    let y01 = p01.y.get();

    let dx1 = x10 - x11;
    let dx2 = x01 - x11;
    let dy1 = y10 - y11;
    let dy2 = y01 - y11;
    let det = dx1 * dy2 - dx2 * dy1;
    if det.abs() <= 1e-12 {
        return Err(GeomError::SingularTransform);
    }

    let sx = x00 - x10 + x11 - x01;
    let sy = y00 - y10 + y11 - y01;
    let g = (sx * dy2 - sy * dx2) / det;
    let h_proj = (dx1 * sy - dy1 * sx) / det;
    let a = x10 - x00 + g * x10;
    let b = x01 - x00 + h_proj * x01;
    let c = x00;
    let d = y10 - y00 + g * y10;
    let e = y01 - y00 + h_proj * y01;
    let f = y00;

    let warp_pt = |pt: Point| -> Result<Point, GeomError> {
        let u = (pt.x.get() - min_x) / w;
        let v = (pt.y.get() - min_y) / h;
        let denom = g * u + h_proj * v + 1.0;
        if denom.abs() <= 1e-9 {
            return Err(GeomError::SingularTransform);
        }
        let x = (a * u + b * v + c) / denom;
        let y = (d * u + e * v + f) / denom;
        Point::new(x, y)
    };

    let mut out = Path { elements: Vec::with_capacity(path.elements.len()), fill_rule: path.fill_rule };
    for &el in &path.elements {
        let mapped = match el {
            PathElement::MoveTo(p) => PathElement::MoveTo(warp_pt(p)?),
            PathElement::LineTo(p) => PathElement::LineTo(warp_pt(p)?),
            PathElement::QuadTo { ctrl, end } => PathElement::QuadTo { ctrl: warp_pt(ctrl)?, end: warp_pt(end)? },
            PathElement::CubicTo { c1, c2, end } => PathElement::CubicTo { c1: warp_pt(c1)?, c2: warp_pt(c2)?, end: warp_pt(end)? },
            PathElement::Close => PathElement::Close,
        };
        out.elements.push(mapped);
    }
    Ok(out)
}

/// Applies CorelDRAW Fillet (`Round`), Scallop (`Scalloped`), or Chamfer (`Chamfered`) corner modification
/// with `radius_pt` to all sharp polygon/polyline vertices of a [`Path`].
pub fn fillet_path_corners(path: &Path, radius_pt: f64, kind: crate::shapes::CornerKind) -> Result<Path, GeomError> {
    if !radius_pt.is_finite() {
        return Err(GeomError::NonFiniteCoordinate);
    }
    if radius_pt <= 1e-6 {
        return Ok(path.clone());
    }
    let polys = crate::flatten::flatten_path(path, &crate::flatten::FlattenOpts::default())?;
    let mut out = Path { elements: Vec::new(), fill_rule: path.fill_rule };

    for poly in &polys {
        let mut pts = poly.points.clone();
        if poly.closed
            && pts.len() >= 2
            && let (Some(&first), Some(&last)) = (pts.first(), pts.last())
            && first.distance(last) < 1e-6
        {
            pts.pop();
        }
        let m = pts.len();
        if m < 3 {
            if let Some((&first, rest)) = pts.split_first() {
                out.move_to(first)?;
                for &p in rest {
                    out.line_to(p)?;
                }
                if poly.closed {
                    out.close()?;
                }
            }
            continue;
        }

        let mut started = false;
        for i in 0..m {
            if !poly.closed && (i == 0 || i == m.saturating_sub(1)) {
                if let Some(&pt_i) = pts.get(i) {
                    if !started {
                        out.move_to(pt_i)?;
                        started = true;
                    } else {
                        out.line_to(pt_i)?;
                    }
                }
                continue;
            }
            let prev_i = i.saturating_add(m.saturating_sub(1)).checked_rem(m).unwrap_or(0);
            let next_i = i.saturating_add(1).checked_rem(m).unwrap_or(0);
            let (Some(&prev), Some(&curr), Some(&next)) = (pts.get(prev_i), pts.get(i), pts.get(next_i)) else {
                continue;
            };
            let dx_in = prev.x.get() - curr.x.get();
            let dy_in = prev.y.get() - curr.y.get();
            let len_in = dx_in.hypot(dy_in);
            let dx_out = next.x.get() - curr.x.get();
            let dy_out = next.y.get() - curr.y.get();
            let len_out = dx_out.hypot(dy_out);
            if len_in <= 1e-6 || len_out <= 1e-6 {
                continue;
            }
            let ux_in = dx_in / len_in;
            let uy_in = dy_in / len_in;
            let ux_out = dx_out / len_out;
            let uy_out = dy_out / len_out;
            let cos_theta = (ux_in * ux_out + uy_in * uy_out).clamp(-1.0, 1.0);
            let theta = cos_theta.acos();
            let half_tan = (0.5 * theta).tan();
            if half_tan.abs() <= 1e-6 {
                if !started {
                    out.move_to(curr)?;
                    started = true;
                } else {
                    out.line_to(curr)?;
                }
                continue;
            }
            let t = (radius_pt / half_tan.abs()).min(0.45 * len_in).min(0.45 * len_out);
            let t_in = Point::new(curr.x.get() + t * ux_in, curr.y.get() + t * uy_in)?;
            let t_out = Point::new(curr.x.get() + t * ux_out, curr.y.get() + t * uy_out)?;
            if !started {
                out.move_to(t_in)?;
                started = true;
            } else {
                out.line_to(t_in)?;
            }
            match kind {
                crate::shapes::CornerKind::Chamfered => {
                    out.line_to(t_out)?;
                }
                crate::shapes::CornerKind::Round => {
                    out.quad_to(curr, t_out)?;
                }
                crate::shapes::CornerKind::Scalloped => {
                    let ctrl = Point::new(t_in.x.get() + t_out.x.get() - curr.x.get(), t_in.y.get() + t_out.y.get() - curr.y.get())?;
                    out.quad_to(ctrl, t_out)?;
                }
            }
        }
        if poly.closed && started {
            out.close()?;
        }
    }
    Ok(out)
}

/// Expands a spine [`Path`] into a closed variable-width Artistic Media calligraphic brush ribbon
/// tapering linearly from `start_width_pt` to `end_width_pt`.
pub fn expand_artistic_brush_stroke(spine: &Path, start_width_pt: f64, end_width_pt: f64) -> Result<Path, GeomError> {
    let lut = ArcLengthTable::from_path(spine)?;
    if lut.samples.is_empty() {
        return Ok(Path::new());
    }
    let steps = 24_usize;
    let mut left_pts = Vec::with_capacity(steps.saturating_add(1));
    let mut right_pts = Vec::with_capacity(steps.saturating_add(1));

    for i in 0..=steps {
        let u = (i as f64) / (steps as f64);
        let Some(sample) = lut.sample_normalized(u) else {
            continue;
        };
        let half_w = 0.5 * (start_width_pt + u * (end_width_pt - start_width_pt)).max(0.0);
        let nx = -sample.tangent.1;
        let ny = sample.tangent.0;
        let lx = sample.point.x.get() + half_w * nx;
        let ly = sample.point.y.get() + half_w * ny;
        let rx = sample.point.x.get() - half_w * nx;
        let ry = sample.point.y.get() - half_w * ny;
        left_pts.push(Point::new(lx, ly)?);
        right_pts.push(Point::new(rx, ry)?);
    }

    let mut out = Path::new();
    if let Some((&first, rest)) = left_pts.split_first() {
        out.move_to(first)?;
        for &pt in rest {
            out.line_to(pt)?;
        }
        for &pt in right_pts.iter().rev() {
            out.line_to(pt)?;
        }
        out.close()?;
    }
    Ok(out)
}

/// CorelDRAW interactive distortion mode (`Effects -> Distort`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DistortionKind {
    /// Radial push/pull of path segments relative to the bounding-box center (`Push and Pull Distortion`).
    #[default]
    PushPull,
    /// Sawtooth/sinusoidal normal oscillation along path edges (`Zipper Distortion`).
    Zipper,
    /// Vortex angular swirl around the bounding-box center (`Twister Distortion`).
    Twister,
}

/// Applies a CorelDRAW distortion (`PushPull`, `Zipper`, or `Twister`) to `path` within `bbox`.
pub fn distort_path(path: &Path, bbox: Rect, kind: DistortionKind, amplitude: f64, frequency: f64) -> Result<Path, GeomError> {
    if !amplitude.is_finite() || !frequency.is_finite() {
        return Err(GeomError::NonFiniteCoordinate);
    }
    let center = bbox.center()?;
    let cx = center.x.get();
    let cy = center.y.get();
    let max_r = (0.5 * bbox.width()).hypot(0.5 * bbox.height()).max(1.0);
    let freq = frequency.clamp(1.0, 64.0);

    let polylines = crate::flatten::flatten_path(path, &crate::flatten::FlattenOpts::default())?;
    let mut out = Path { elements: Vec::new(), fill_rule: path.fill_rule };

    for poly in polylines {
        if poly.points.len() < 2 {
            continue;
        }
        // Subdivide each segment into 4 sub-steps so straight edges can curve under distortion
        let mut dense_pts = Vec::with_capacity(poly.points.len().saturating_mul(4));
        for seg in poly.points.windows(2) {
            let [p0, p1] = *seg else { continue };
            for k in 0..4 {
                let t = f64::from(k) / 4.0;
                let x = p0.x.get() + t * (p1.x.get() - p0.x.get());
                let y = p0.y.get() + t * (p1.y.get() - p0.y.get());
                dense_pts.push(Point::new(x, y)?);
            }
        }
        if let Some(&last) = poly.points.last() {
            dense_pts.push(last);
        }

        let mut cum_len = 0.0_f64;
        for (idx, &pt) in dense_pts.iter().enumerate() {
            if idx > 0
                && let Some(&prev_pt) = dense_pts.get(idx.saturating_sub(1))
            {
                cum_len += prev_pt.distance(pt);
            }
            let dx = pt.x.get() - cx;
            let dy = pt.y.get() - cy;
            let r = dx.hypot(dy);
            let mapped = match kind {
                DistortionKind::PushPull => {
                    let angle = dy.atan2(dx);
                    let lobe = (freq * angle).cos();
                    let scale = (1.0 + (amplitude / max_r) * lobe).max(0.05);
                    Point::new(cx + dx * scale, cy + dy * scale)?
                }
                DistortionKind::Zipper => {
                    let prev = if idx > 0 { dense_pts.get(idx.saturating_sub(1)).copied().unwrap_or(pt) } else { pt };
                    let next = dense_pts.get(idx.saturating_add(1)).copied().unwrap_or(pt);
                    let tx = next.x.get() - prev.x.get();
                    let ty = next.y.get() - prev.y.get();
                    let tlen = tx.hypot(ty).max(1e-6);
                    let nx = -ty / tlen;
                    let ny = tx / tlen;
                    let wave = amplitude * (std::f64::consts::TAU * freq * cum_len / max_r.max(10.0)).sin();
                    Point::new(pt.x.get() + nx * wave, pt.y.get() + ny * wave)?
                }
                DistortionKind::Twister => {
                    let falloff = (1.0 - (r / max_r).clamp(0.0, 1.0)).powi(2);
                    let swirl_rad = amplitude.to_radians() * falloff;
                    let (sin_a, cos_a) = swirl_rad.sin_cos();
                    Point::new(cx + dx * cos_a - dy * sin_a, cy + dx * sin_a + dy * cos_a)?
                }
            };
            if idx == 0 {
                out.move_to(mapped)?;
            } else {
                out.line_to(mapped)?;
            }
        }
        if poly.closed {
            out.close()?;
        }
    }
    Ok(out)
}

/// Clean-room PowerTrace bitmap-to-vector contour tracer (`Bitmaps -> Outline Trace`):
/// thresholds an RGBA8 image (`alpha >= 128 && BT.709 luminance <= threshold`) and emits merged
/// scanline run rectangles as a closed vector [`Path`] in typographic points (`72 / dpi`).
pub fn trace_bitmap_to_path(
    pixel_width: u32,
    pixel_height: u32,
    dpi_x: f64,
    dpi_y: f64,
    rgba_pixels: &[u8],
    threshold: u8,
) -> Result<Path, GeomError> {
    if pixel_width == 0 || pixel_height == 0 {
        return Err(GeomError::InvalidShapeParameter("bitmap dimensions must be positive"));
    }
    let expected = (pixel_width as usize).saturating_mul(pixel_height as usize).saturating_mul(4);
    if rgba_pixels.len() < expected {
        return Err(GeomError::InvalidShapeParameter("rgba_pixels buffer shorter than pixel_width * pixel_height * 4"));
    }
    let scale_x = 72.0 / dpi_x.max(1.0);
    let scale_y = 72.0 / dpi_y.max(1.0);

    let is_fg_pixel = |px: u32, py: u32| -> bool {
        let idx = (py as usize).saturating_mul(pixel_width as usize).saturating_add(px as usize).saturating_mul(4);
        if let Some(&[r_u8, g_u8, b_u8, a_u8, ..]) = rgba_pixels.get(idx..) {
            let r = u16::from(r_u8);
            let g = u16::from(g_u8);
            let b = u16::from(b_u8);
            let lum = (r.saturating_mul(54).saturating_add(g.saturating_mul(183)).saturating_add(b.saturating_mul(19)) >> 8) as u8;
            a_u8 >= 128 && lum <= threshold
        } else {
            false
        }
    };

    let mut path = Path { elements: Vec::new(), fill_rule: FillRule::NonZero };
    for y in 0..pixel_height {
        let mut x = 0u32;
        while x < pixel_width {
            if is_fg_pixel(x, y) {
                let start_x = x;
                x = x.saturating_add(1);
                while x < pixel_width && is_fg_pixel(x, y) {
                    x = x.saturating_add(1);
                }
                let x0 = f64::from(start_x) * scale_x;
                let x1 = f64::from(x) * scale_x;
                let y0 = f64::from(y) * scale_y;
                let y1 = f64::from(y.saturating_add(1)) * scale_y;
                if path.elements.len().saturating_add(5) <= MAX_PATH_ELEMENTS {
                    path.move_to(Point::new(x0, y0)?)?;
                    path.line_to(Point::new(x1, y0)?)?;
                    path.line_to(Point::new(x1, y1)?)?;
                    path.line_to(Point::new(x0, y1)?)?;
                    path.close()?;
                }
            } else {
                x = x.saturating_add(1);
            }
        }
    }
    Ok(path)
}

/// Reverses the winding direction of every subpath in `path` (`Shape Tool -> Reverse Direction`),
/// swapping cubic control points `(c1, c2)` so that reversing twice is bit-exact.
pub fn reverse_path_direction(path: &Path) -> Result<Path, GeomError> {
    #[derive(Clone, Copy)]
    enum Seg {
        Line(Point),
        Quad(Point, Point),
        Cubic(Point, Point, Point),
    }

    let mut subpaths: Vec<(Point, Vec<Seg>, bool)> = Vec::new();
    for &el in &path.elements {
        match el {
            PathElement::MoveTo(p) => {
                subpaths.push((p, Vec::new(), false));
            }
            PathElement::LineTo(p) => {
                if let Some(last) = subpaths.last_mut() {
                    last.1.push(Seg::Line(p));
                }
            }
            PathElement::QuadTo { ctrl, end } => {
                if let Some(last) = subpaths.last_mut() {
                    last.1.push(Seg::Quad(ctrl, end));
                }
            }
            PathElement::CubicTo { c1, c2, end } => {
                if let Some(last) = subpaths.last_mut() {
                    last.1.push(Seg::Cubic(c1, c2, end));
                }
            }
            PathElement::Close => {
                if let Some(last) = subpaths.last_mut() {
                    last.2 = true;
                }
            }
        }
    }

    let mut out = Path { elements: Vec::with_capacity(path.elements.len()), fill_rule: path.fill_rule };
    for (start_pt, segs, closed) in subpaths {
        if segs.is_empty() {
            out.move_to(start_pt)?;
            if closed {
                out.close()?;
            }
            continue;
        }
        let end_pt = match *segs.last().unwrap_or(&Seg::Line(start_pt)) {
            Seg::Line(p) | Seg::Quad(_, p) | Seg::Cubic(_, _, p) => p,
        };
        out.move_to(end_pt)?;
        for idx in (0..segs.len()).rev() {
            let prev_pt = if idx == 0 {
                start_pt
            } else {
                match segs.get(idx.saturating_sub(1)).copied().unwrap_or(Seg::Line(start_pt)) {
                    Seg::Line(p) | Seg::Quad(_, p) | Seg::Cubic(_, _, p) => p,
                }
            };
            if let Some(&seg) = segs.get(idx) {
                match seg {
                    Seg::Line(_) => out.line_to(prev_pt)?,
                    Seg::Quad(ctrl, _) => out.quad_to(ctrl, prev_pt)?,
                    Seg::Cubic(c1, c2, _) => out.cubic_to(c2, c1, prev_pt)?,
                }
            }
        }
        if closed {
            out.close()?;
        }
    }
    Ok(out)
}

/// Closes all open subpaths in `path` that contain at least one segment (`Object -> Join Curves` / `Auto-Close Curve`).
pub fn close_open_subpaths(path: &Path) -> Result<Path, GeomError> {
    let mut out = Path { elements: Vec::with_capacity(path.elements.len().saturating_add(4)), fill_rule: path.fill_rule };
    let mut has_seg_in_subpath = false;
    let mut is_closed = false;

    for &el in &path.elements {
        match el {
            PathElement::MoveTo(p) => {
                if has_seg_in_subpath && !is_closed {
                    out.close()?;
                }
                out.move_to(p)?;
                has_seg_in_subpath = false;
                is_closed = false;
            }
            PathElement::LineTo(_) | PathElement::QuadTo { .. } | PathElement::CubicTo { .. } => {
                out.push(el)?;
                has_seg_in_subpath = true;
                is_closed = false;
            }
            PathElement::Close => {
                out.close()?;
                is_closed = true;
            }
        }
    }
    if has_seg_in_subpath && !is_closed {
        out.close()?;
    }
    Ok(out)
}

/// Joins open subpaths in `path` whose endpoints lie within `tolerance_pt` of each other,
/// and closes any resulting subpath whose start and end lie within `tolerance_pt` (`Object -> Join Curves` / `Ctrl+Shift+J`).
pub fn join_open_subpaths(path: &Path, tolerance_pt: f64) -> Result<Path, GeomError> {
    if !tolerance_pt.is_finite() {
        return Err(GeomError::NonFiniteCoordinate);
    }
    if tolerance_pt < 0.0 {
        return Err(GeomError::InvalidShapeParameter("tolerance_pt must be >= 0"));
    }
    let mut subpaths: Vec<Vec<PathElement>> = Vec::new();
    for &el in &path.elements {
        if matches!(el, PathElement::MoveTo(_)) || subpaths.is_empty() {
            subpaths.push(vec![el]);
        } else if let Some(last) = subpaths.last_mut() {
            last.push(el);
        }
    }

    let endpoint_of = |sub: &[PathElement]| -> Option<(Point, Point, bool)> {
        let Some((&PathElement::MoveTo(start), rest)) = sub.split_first() else {
            return None;
        };
        let mut end = start;
        let mut closed = false;
        for &el in rest {
            match el {
                PathElement::LineTo(p) | PathElement::QuadTo { end: p, .. } | PathElement::CubicTo { end: p, .. } => end = p,
                PathElement::Close => closed = true,
                PathElement::MoveTo(p) => end = p,
            }
        }
        Some((start, end, closed))
    };

    let dist = |a: Point, b: Point| -> f64 { (a.x.get() - b.x.get()).hypot(a.y.get() - b.y.get()) };

    let mut merged: Vec<Vec<PathElement>> = Vec::new();
    for sub in subpaths {
        let Some((s2, e2, c2)) = endpoint_of(&sub) else {
            continue;
        };
        if c2 {
            merged.push(sub);
            continue;
        }
        let mut attached = false;
        for prev in &mut merged {
            if let Some((s1, e1, c1)) = endpoint_of(prev)
                && !c1
            {
                if dist(e1, s2) <= tolerance_pt {
                    if dist(e1, s2) > 1e-6 {
                        prev.push(PathElement::LineTo(s2));
                    }
                    if let Some(tail) = sub.get(1..) {
                        prev.extend_from_slice(tail);
                    }
                    let (_, new_end, _) = endpoint_of(prev).unwrap_or((s1, e2, false));
                    if dist(s1, new_end) <= tolerance_pt {
                        prev.push(PathElement::Close);
                    }
                    attached = true;
                    break;
                } else if dist(e1, e2) <= tolerance_pt {
                    let mut tmp = Path { elements: sub.clone(), fill_rule: path.fill_rule };
                    if let Ok(rev) = reverse_path_direction(&tmp) {
                        tmp = rev;
                        if dist(e1, e2) > 1e-6 {
                            prev.push(PathElement::LineTo(e2));
                        }
                        if let Some(tail) = tmp.elements.get(1..) {
                            prev.extend_from_slice(tail);
                        }
                        let (_, new_end, _) = endpoint_of(prev).unwrap_or((s1, s2, false));
                        if dist(s1, new_end) <= tolerance_pt {
                            prev.push(PathElement::Close);
                        }
                        attached = true;
                        break;
                    }
                }
            }
        }
        if !attached {
            let mut sub_owned = sub;
            if sub_owned.len() >= 3 && dist(s2, e2) <= tolerance_pt {
                sub_owned.push(PathElement::Close);
            }
            merged.push(sub_owned);
        }
    }

    let mut out = Path { elements: Vec::new(), fill_rule: path.fill_rule };
    for sub in merged {
        for el in sub {
            out.push(el)?;
        }
    }
    Ok(out)
}

/// Simplifies each subpath in `path` using the Ramer-Douglas-Peucker algorithm with maximum perpendicular
/// error `tolerance_pt` (`Shape Tool -> Reduce Nodes`).
pub fn simplify_path_nodes(path: &Path, tolerance_pt: f64) -> Result<Path, GeomError> {
    if !tolerance_pt.is_finite() {
        return Err(GeomError::NonFiniteCoordinate);
    }
    if tolerance_pt < 0.0 {
        return Err(GeomError::InvalidShapeParameter("tolerance_pt must be >= 0"));
    }
    let polylines = crate::flatten::flatten_path(path, &crate::flatten::FlattenOpts::default())?;
    let mut out = Path { elements: Vec::new(), fill_rule: path.fill_rule };

    for poly in polylines {
        let pts = &poly.points;
        let Some((&first_pt, rest)) = pts.split_first() else {
            continue;
        };
        if pts.len() <= 2 || tolerance_pt <= 1e-9 {
            out.move_to(first_pt)?;
            for &p in rest {
                out.line_to(p)?;
            }
            if poly.closed {
                out.close()?;
            }
            continue;
        }

        let mut keep = vec![false; pts.len()];
        if let Some(slot) = keep.first_mut() {
            *slot = true;
        }
        if let Some(slot) = keep.last_mut() {
            *slot = true;
        }
        if poly.closed
            && pts.len() >= 4
            && let Some(slot) = keep.get_mut(pts.len() / 2)
        {
            *slot = true;
        }

        let mut stack = Vec::new();
        if poly.closed && pts.len() >= 4 {
            stack.push((0usize, pts.len() / 2));
            stack.push((pts.len() / 2, pts.len().saturating_sub(1)));
        } else {
            stack.push((0usize, pts.len().saturating_sub(1)));
        }

        while let Some((start_idx, end_idx)) = stack.pop() {
            if end_idx <= start_idx.saturating_add(1) {
                continue;
            }
            let (Some(&a), Some(&b)) = (pts.get(start_idx), pts.get(end_idx)) else {
                continue;
            };
            let dx = b.x.get() - a.x.get();
            let dy = b.y.get() - a.y.get();
            let len_sq = dx * dx + dy * dy;

            let mut max_dist = 0.0;
            let mut max_idx = start_idx;
            for (idx, p) in pts.iter().enumerate().take(end_idx).skip(start_idx.saturating_add(1)) {
                let d = if len_sq <= 1e-12 {
                    (p.x.get() - a.x.get()).hypot(p.y.get() - a.y.get())
                } else {
                    let t = (((p.x.get() - a.x.get()) * dx + (p.y.get() - a.y.get()) * dy) / len_sq).clamp(0.0, 1.0);
                    let proj_x = a.x.get() + t * dx;
                    let proj_y = a.y.get() + t * dy;
                    (p.x.get() - proj_x).hypot(p.y.get() - proj_y)
                };
                if d > max_dist {
                    max_dist = d;
                    max_idx = idx;
                }
            }

            if max_dist > tolerance_pt && max_idx > start_idx && max_idx < end_idx {
                if let Some(slot) = keep.get_mut(max_idx) {
                    *slot = true;
                }
                stack.push((start_idx, max_idx));
                stack.push((max_idx, end_idx));
            }
        }

        let mut first = true;
        for (i, &p) in pts.iter().enumerate() {
            if keep.get(i).copied().unwrap_or(false) {
                if first {
                    out.move_to(p)?;
                    first = false;
                } else {
                    out.line_to(p)?;
                }
            }
        }
        if poly.closed {
            out.close()?;
        }
    }
    Ok(out)
}

/// Subdivides the segment at `element_index` of `path` at its midpoint `t = 0.5` using exact
/// de Casteljau subdivision (`F10` Shape Tool `+` Add Node).
pub fn insert_curve_node_at(path: &Path, element_index: usize) -> Result<Path, GeomError> {
    if element_index == 0 || element_index >= path.elements.len() {
        return Err(GeomError::InvalidShapeParameter("element_index out of bounds for insert_curve_node_at"));
    }
    let mut prev_pt = Point::ZERO;
    for &el in path.elements.iter().take(element_index) {
        match el {
            PathElement::MoveTo(p) | PathElement::LineTo(p) | PathElement::QuadTo { end: p, .. } | PathElement::CubicTo { end: p, .. } => {
                prev_pt = p;
            }
            PathElement::Close => {}
        }
    }

    let mut out = Path { elements: Vec::with_capacity(path.elements.len().saturating_add(1)), fill_rule: path.fill_rule };
    for (idx, &el) in path.elements.iter().enumerate() {
        if idx != element_index {
            out.push(el)?;
            continue;
        }
        match el {
            PathElement::LineTo(end) => {
                let mid = Point::new(0.5 * (prev_pt.x.get() + end.x.get()), 0.5 * (prev_pt.y.get() + end.y.get()))?;
                out.line_to(mid)?;
                out.line_to(end)?;
            }
            PathElement::QuadTo { ctrl, end } => {
                let q0 = Point::new(0.5 * (prev_pt.x.get() + ctrl.x.get()), 0.5 * (prev_pt.y.get() + ctrl.y.get()))?;
                let q1 = Point::new(0.5 * (ctrl.x.get() + end.x.get()), 0.5 * (ctrl.y.get() + end.y.get()))?;
                let mid = Point::new(0.5 * (q0.x.get() + q1.x.get()), 0.5 * (q0.y.get() + q1.y.get()))?;
                out.quad_to(q0, mid)?;
                out.quad_to(q1, end)?;
            }
            PathElement::CubicTo { c1, c2, end } => {
                let p01 = Point::new(0.5 * (prev_pt.x.get() + c1.x.get()), 0.5 * (prev_pt.y.get() + c1.y.get()))?;
                let p12 = Point::new(0.5 * (c1.x.get() + c2.x.get()), 0.5 * (c1.y.get() + c2.y.get()))?;
                let p23 = Point::new(0.5 * (c2.x.get() + end.x.get()), 0.5 * (c2.y.get() + end.y.get()))?;
                let p012 = Point::new(0.5 * (p01.x.get() + p12.x.get()), 0.5 * (p01.y.get() + p12.y.get()))?;
                let p123 = Point::new(0.5 * (p12.x.get() + p23.x.get()), 0.5 * (p12.y.get() + p23.y.get()))?;
                let mid = Point::new(0.5 * (p012.x.get() + p123.x.get()), 0.5 * (p012.y.get() + p123.y.get()))?;
                out.cubic_to(p01, p012, mid)?;
                out.cubic_to(p123, p23, end)?;
            }
            _ => out.push(el)?,
        }
    }
    Ok(out)
}

/// Removes the segment node at `element_index` of `path` (`F10` Shape Tool `-` Delete Node).
pub fn delete_curve_node_at(path: &Path, element_index: usize) -> Result<Path, GeomError> {
    if element_index == 0 || element_index >= path.elements.len() {
        return Err(GeomError::InvalidShapeParameter("element_index out of bounds for delete_curve_node_at"));
    }
    if matches!(path.elements.get(element_index), Some(PathElement::MoveTo(_) | PathElement::Close) | None) {
        return Err(GeomError::InvalidShapeParameter("cannot delete MoveTo or Close element"));
    }
    let mut out = Path { elements: Vec::with_capacity(path.elements.len()), fill_rule: path.fill_rule };
    for (idx, &el) in path.elements.iter().enumerate() {
        if idx != element_index {
            out.push(el)?;
        }
    }
    Ok(out)
}

/// Converts the curve segment at `element_index` into a straight `LineTo` segment (`Shape Tool -> To Line`).
pub fn convert_segment_to_line(path: &Path, element_index: usize) -> Result<Path, GeomError> {
    if element_index == 0 {
        return Err(GeomError::InvalidShapeParameter("element_index out of bounds for convert_segment_to_line"));
    }
    let Some(&el) = path.elements.get(element_index) else {
        return Err(GeomError::InvalidShapeParameter("element_index out of bounds for convert_segment_to_line"));
    };
    let target = match el {
        PathElement::LineTo(end) | PathElement::QuadTo { end, .. } | PathElement::CubicTo { end, .. } => PathElement::LineTo(end),
        PathElement::MoveTo(_) | PathElement::Close => {
            return Err(GeomError::InvalidShapeParameter("cannot convert MoveTo or Close to line"));
        }
    };
    let mut elements = path.elements.clone();
    if let Some(slot) = elements.get_mut(element_index) {
        *slot = target;
    }
    Path::from_elements(elements, path.fill_rule)
}

/// Converts the segment at `element_index` into a `CubicTo` Bézier segment with collinear `1/3` and `2/3` handles (`Shape Tool -> To Curve`).
pub fn convert_segment_to_curve(path: &Path, element_index: usize) -> Result<Path, GeomError> {
    if element_index == 0 || element_index >= path.elements.len() {
        return Err(GeomError::InvalidShapeParameter("element_index out of bounds for convert_segment_to_curve"));
    }
    let mut start = Point::ZERO;
    let mut has_start = false;
    for &el in path.elements.iter().take(element_index) {
        match el {
            PathElement::MoveTo(p) | PathElement::LineTo(p) | PathElement::QuadTo { end: p, .. } | PathElement::CubicTo { end: p, .. } => {
                start = p;
                has_start = true;
            }
            PathElement::Close => {}
        }
    }
    if !has_start {
        return Err(GeomError::InvalidShapeParameter("missing preceding anchor for convert_segment_to_curve"));
    }
    let lerp = |a: Point, b: Point, t: f64| -> Result<Point, GeomError> {
        Point::new(a.x.get() + (b.x.get() - a.x.get()) * t, a.y.get() + (b.y.get() - a.y.get()) * t)
    };
    let Some(&el) = path.elements.get(element_index) else {
        return Err(GeomError::InvalidShapeParameter("element_index out of bounds for convert_segment_to_curve"));
    };
    let target = match el {
        PathElement::LineTo(end) => {
            let c1 = lerp(start, end, 1.0 / 3.0)?;
            let c2 = lerp(start, end, 2.0 / 3.0)?;
            PathElement::CubicTo { c1, c2, end }
        }
        PathElement::QuadTo { ctrl, end } => {
            let (c1, c2) = crate::flatten::elevate_quad_to_cubic(start, ctrl, end)?;
            PathElement::CubicTo { c1, c2, end }
        }
        PathElement::CubicTo { c1, c2, end } => PathElement::CubicTo { c1, c2, end },
        PathElement::MoveTo(_) | PathElement::Close => {
            return Err(GeomError::InvalidShapeParameter("cannot convert MoveTo or Close to curve"));
        }
    };
    let mut elements = path.elements.clone();
    if let Some(slot) = elements.get_mut(element_index) {
        *slot = target;
    }
    Path::from_elements(elements, path.fill_rule)
}

/// Applies a localized rotational swirl around `center` within `radius_pt` (`Twirl Tool`).
pub fn twirl_path_around_point(path: &Path, center: Point, radius_pt: f64, angle_deg: f64) -> Result<Path, GeomError> {
    let r = crate::F64::new(radius_pt)?.get();
    let max_rad = crate::F64::new(angle_deg)?.get().to_radians();
    if r <= 0.0 {
        return Err(GeomError::InvalidShapeParameter("radius_pt must be positive"));
    }
    let warp_pt = |p: Point| -> Result<Point, GeomError> {
        let dx = p.x.get() - center.x.get();
        let dy = p.y.get() - center.y.get();
        let dist = dx.hypot(dy);
        if dist >= r {
            return Ok(p);
        }
        let u = 1.0 - dist / r;
        let theta = max_rad * u * u;
        let (s, c) = theta.sin_cos();
        Point::new(center.x.get() + dx * c - dy * s, center.y.get() + dx * s + dy * c)
    };
    let mut out = Path { elements: Vec::with_capacity(path.elements.len()), fill_rule: path.fill_rule };
    for &el in &path.elements {
        match el {
            PathElement::MoveTo(p) => out.move_to(warp_pt(p)?)?,
            PathElement::LineTo(p) => out.line_to(warp_pt(p)?)?,
            PathElement::QuadTo { ctrl, end } => out.quad_to(warp_pt(ctrl)?, warp_pt(end)?)?,
            PathElement::CubicTo { c1, c2, end } => out.cubic_to(warp_pt(c1)?, warp_pt(c2)?, warp_pt(end)?)?,
            PathElement::Close => out.close()?,
        }
    }
    Ok(out)
}

/// Radially attracts (`strength > 0`) or repels (`strength < 0`) vertices within `radius_pt` of `center`
/// (`Attract and Repel Tool`).
pub fn radial_push_path_around_point(path: &Path, center: Point, radius_pt: f64, strength: f64) -> Result<Path, GeomError> {
    let r = crate::F64::new(radius_pt)?.get();
    let s = crate::F64::new(strength)?.get().clamp(-0.95, 0.95);
    if r <= 0.0 {
        return Err(GeomError::InvalidShapeParameter("radius_pt must be positive"));
    }
    let warp_pt = |p: Point| -> Result<Point, GeomError> {
        let dx = p.x.get() - center.x.get();
        let dy = p.y.get() - center.y.get();
        let dist = dx.hypot(dy);
        if dist <= 1e-9 || dist >= r {
            return Ok(p);
        }
        let t = dist / r;
        let u = 1.0 - t;
        let new_t = (t * (1.0 - s * u * u)).clamp(0.0, 1.0);
        let scale = new_t / t;
        Point::new(center.x.get() + dx * scale, center.y.get() + dy * scale)
    };
    let mut out = Path { elements: Vec::with_capacity(path.elements.len()), fill_rule: path.fill_rule };
    for &el in &path.elements {
        match el {
            PathElement::MoveTo(p) => out.move_to(warp_pt(p)?)?,
            PathElement::LineTo(p) => out.line_to(warp_pt(p)?)?,
            PathElement::QuadTo { ctrl, end } => out.quad_to(warp_pt(ctrl)?, warp_pt(end)?)?,
            PathElement::CubicTo { c1, c2, end } => out.cubic_to(warp_pt(c1)?, warp_pt(c2)?, warp_pt(end)?)?,
            PathElement::Close => out.close()?,
        }
    }
    Ok(out)
}

/// Directionally drags vertices within `radius_pt` of `origin` along `(dx_pt, dy_pt)` with smooth quadratic falloff (`Smear Tool`).
pub fn smear_path_from_point(path: &Path, origin: Point, radius_pt: f64, dx_pt: f64, dy_pt: f64) -> Result<Path, GeomError> {
    let r = crate::F64::new(radius_pt)?.get();
    let dx = crate::F64::new(dx_pt)?.get();
    let dy = crate::F64::new(dy_pt)?.get();
    if r <= 0.0 {
        return Err(GeomError::InvalidShapeParameter("radius_pt must be positive"));
    }
    let warp_pt = |p: Point| -> Result<Point, GeomError> {
        let dist = (p.x.get() - origin.x.get()).hypot(p.y.get() - origin.y.get());
        if dist >= r {
            return Ok(p);
        }
        let u = 1.0 - dist / r;
        let w = u * u;
        Point::new(p.x.get() + dx * w, p.y.get() + dy * w)
    };
    let mut out = Path { elements: Vec::with_capacity(path.elements.len()), fill_rule: path.fill_rule };
    for &el in &path.elements {
        match el {
            PathElement::MoveTo(p) => out.move_to(warp_pt(p)?)?,
            PathElement::LineTo(p) => out.line_to(warp_pt(p)?)?,
            PathElement::QuadTo { ctrl, end } => out.quad_to(warp_pt(ctrl)?, warp_pt(end)?)?,
            PathElement::CubicTo { c1, c2, end } => out.cubic_to(warp_pt(c1)?, warp_pt(c2)?, warp_pt(end)?)?,
            PathElement::Close => out.close()?,
        }
    }
    Ok(out)
}

/// Subdivides segments of `path` with serrated zigzag normal teeth of amplitude `tooth_height_pt`
/// and `frequency` teeth per segment (`Roughen Tool`).
pub fn roughen_path_segments(path: &Path, tooth_height_pt: f64, frequency: u32) -> Result<Path, GeomError> {
    let h = crate::F64::new(tooth_height_pt)?.get();
    if h < 0.0 || frequency == 0 {
        return Err(GeomError::InvalidShapeParameter("tooth_height_pt must be >= 0 and frequency >= 1"));
    }
    let steps = frequency.clamp(1, 32);
    let polylines = crate::flatten::flatten_path(path, &crate::flatten::FlattenOpts::default())?;
    let mut out = Path { elements: Vec::new(), fill_rule: path.fill_rule };
    for poly in polylines {
        let Some(&first_pt) = poly.points.first() else {
            continue;
        };
        if poly.points.len() < 2 {
            continue;
        }
        out.move_to(first_pt)?;
        for seg in poly.points.windows(2) {
            let [a, b] = *seg else { continue };
            let dx = b.x.get() - a.x.get();
            let dy = b.y.get() - a.y.get();
            let len = dx.hypot(dy);
            if len <= 1e-6 {
                out.line_to(b)?;
                continue;
            }
            let nx = -dy / len;
            let ny = dx / len;
            for k in 0..steps {
                let t_mid = (f64::from(k) + 0.5) / f64::from(steps);
                let sign = if k % 2 == 0 { 1.0 } else { -1.0 };
                let mx = a.x.get() + dx * t_mid + nx * h * sign;
                let my = a.y.get() + dy * t_mid + ny * h * sign;
                out.line_to(Point::new(mx, my)?)?;
                let t_end = f64::from(k.saturating_add(1)) / f64::from(steps);
                let ex = a.x.get() + dx * t_end;
                let ey = a.y.get() + dy * t_end;
                out.line_to(Point::new(ex, ey)?)?;
            }
        }
        if poly.closed {
            out.close()?;
        }
    }
    Ok(out)
}

/// Applies localized Jacobi Laplacian vertex smoothing within `radius_pt` around `center`
/// (`CorelDRAW Smooth Tool`).
///
/// Each pass moves interior vertices within `radius_pt` toward the midpoint of their adjacent
/// subpath neighbors weighted by `strength * (1 - d / radius_pt)^2`, preserving C1 boundary
/// continuity at `d == radius_pt` and pinning open-subpath endpoints.
pub fn smooth_path_around_point(path: &Path, center: Point, radius_pt: f64, strength: f64, iterations: u32) -> Result<Path, GeomError> {
    let r = crate::F64::new(radius_pt)?.get();
    let s = crate::F64::new(strength)?.get().clamp(0.0, 1.0);
    if r <= crate::EPS {
        return Err(GeomError::InvalidShapeParameter("smooth radius must be positive"));
    }
    let passes = iterations.min(64);
    if passes == 0 || s <= crate::EPS {
        return Ok(path.clone());
    }
    let cx = center.x.get();
    let cy = center.y.get();
    let polylines = crate::flatten::flatten_path(path, &crate::flatten::FlattenOpts::default())?;
    let mut out = Path { elements: Vec::new(), fill_rule: path.fill_rule };
    for poly in polylines {
        if poly.points.is_empty() {
            continue;
        }
        let mut pts = poly.points;
        let n = pts.len();
        if n >= 3 {
            let mut buf = pts.clone();
            for _ in 0..passes {
                for i in 0..n {
                    let Some(&cur_pt) = pts.get(i) else { continue };
                    if !poly.closed && (i == 0 || i.saturating_add(1) == n) {
                        if let Some(slot) = buf.get_mut(i) {
                            *slot = cur_pt;
                        }
                        continue;
                    }
                    let px = cur_pt.x.get();
                    let py = cur_pt.y.get();
                    let d = (px - cx).hypot(py - cy);
                    if d >= r {
                        if let Some(slot) = buf.get_mut(i) {
                            *slot = cur_pt;
                        }
                        continue;
                    }
                    let u = 1.0 - d / r;
                    let w = s * u * u;
                    let prev_i = i.saturating_add(n.saturating_sub(1)).checked_rem(n).unwrap_or(0);
                    let next_i = i.saturating_add(1).checked_rem(n).unwrap_or(0);
                    let (Some(&prev), Some(&next)) = (pts.get(prev_i), pts.get(next_i)) else {
                        continue;
                    };
                    let avg_x = 0.5 * (prev.x.get() + next.x.get());
                    let avg_y = 0.5 * (prev.y.get() + next.y.get());
                    if let Some(slot) = buf.get_mut(i) {
                        *slot = Point::new(px + w * (avg_x - px), py + w * (avg_y - py))?;
                    }
                }
                std::mem::swap(&mut pts, &mut buf);
            }
        }
        if let Some((&first, rest)) = pts.split_first() {
            out.move_to(first)?;
            for &pt in rest {
                out.line_to(pt)?;
            }
        }
        if poly.closed {
            out.close()?;
        }
    }
    Ok(out)
}

/// Removes the segment at `element_index` from `path` (`CorelDRAW Virtual Segment Delete Tool`),
/// replacing a curve/line segment with a `MoveTo` jump (and converting a trailing `Close` in the
/// same subpath into a `LineTo` back to the original subpath start so remaining segments stay intact).
pub fn delete_segment_at_index(path: &Path, element_index: usize) -> Result<Path, GeomError> {
    let Some(target_el) = path.elements.get(element_index).copied() else {
        return Err(GeomError::InvalidShapeParameter("element_index out of range"));
    };
    if matches!(target_el, PathElement::MoveTo(_)) {
        return Err(GeomError::InvalidShapeParameter("cannot delete MoveTo anchor with Virtual Segment Delete"));
    }
    if matches!(target_el, PathElement::Close) {
        let mut elems = path.elements.clone();
        elems.remove(element_index);
        return Path::from_elements(elems, path.fill_rule);
    }
    let seg_end = match target_el {
        PathElement::LineTo(p) | PathElement::QuadTo { end: p, .. } | PathElement::CubicTo { end: p, .. } => p,
        PathElement::MoveTo(_) | PathElement::Close => unreachable!(),
    };
    let mut subpath_start = seg_end;
    if let Some(prefix) = path.elements.get(..element_index) {
        for el in prefix.iter().rev() {
            if let PathElement::MoveTo(p) = *el {
                subpath_start = p;
                break;
            }
        }
    }
    let mut elems = Vec::with_capacity(path.elements.len());
    let mut convert_next_close = true;
    for (idx, &el) in path.elements.iter().enumerate() {
        if idx == element_index {
            elems.push(PathElement::MoveTo(seg_end));
        } else if idx > element_index && convert_next_close {
            match el {
                PathElement::MoveTo(_) => {
                    convert_next_close = false;
                    elems.push(el);
                }
                PathElement::Close => {
                    convert_next_close = false;
                    elems.push(PathElement::LineTo(subpath_start));
                }
                _ => elems.push(el),
            }
        } else {
            elems.push(el);
        }
    }
    Path::from_elements(elems, path.fill_rule)
}

/// Corner join style for parallel curve and contour offsetting (`Effects -> Contour`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OffsetJoinKind {
    /// Miter join with miter-limit clamping (`<= 2.5 * |distance_pt|`).
    #[default]
    Miter,
    /// Circular arc fan centered at the original vertex with radius `|distance_pt|`.
    Round,
    /// Flat chamfer segment connecting the incoming and outgoing offset edge endpoints.
    Bevel,
}

/// Computes a parallel offset path shifted by signed `distance_pt` (`CorelDRAW Parallel Drawing / Offset Curve`).
///
/// Closed subpaths orient `distance_pt > 0` outward using signed polygon area; open subpaths shift
/// along the left-hand unit normal with miter clamping at sharp joints.
pub fn offset_open_or_closed_path(path: &Path, distance_pt: f64) -> Result<Path, GeomError> {
    offset_path_with_join(path, distance_pt, OffsetJoinKind::Miter)
}

/// Computes a parallel offset path shifted by signed `distance_pt` with explicit corner join style
/// (`OffsetJoinKind::Miter`, `OffsetJoinKind::Round`, or `OffsetJoinKind::Bevel`).
pub fn offset_path_with_join(path: &Path, distance_pt: f64, join: OffsetJoinKind) -> Result<Path, GeomError> {
    let d = crate::F64::new(distance_pt)?.get();
    if d.abs() <= crate::EPS {
        return Ok(path.clone());
    }
    let polylines = crate::flatten::flatten_path(path, &crate::flatten::FlattenOpts::default())?;
    let mut out = Path { elements: Vec::new(), fill_rule: path.fill_rule };
    let seg_normal = |a: Point, b: Point| -> (f64, f64) {
        let dx = b.x.get() - a.x.get();
        let dy = b.y.get() - a.y.get();
        let len = dx.hypot(dy);
        if len <= 1e-9 { (0.0, 0.0) } else { (-dy / len, dx / len) }
    };
    for poly in polylines {
        let mut pts = poly.points;
        if poly.closed && pts.len() >= 3 && pts.first().zip(pts.last()).is_some_and(|(a, b)| a.distance(*b) < 1e-6) {
            pts.pop();
        }
        let n = pts.len();
        if n < 2 {
            continue;
        }
        let mut sign = 1.0;
        if poly.closed && n >= 3 {
            let mut area2 = 0.0;
            for i in 0..n {
                let next_i = i.saturating_add(1).checked_rem(n).unwrap_or(0);
                if let (Some(&p0), Some(&p1)) = (pts.get(i), pts.get(next_i)) {
                    area2 += p0.x.get() * p1.y.get() - p1.x.get() * p0.y.get();
                }
            }
            if area2 > 0.0 {
                sign = -1.0;
            }
        }
        let eff_d = d * sign;
        let mut shifted = Vec::with_capacity(n.saturating_mul(2));
        for i in 0..n {
            let Some(&cur_pt) = pts.get(i) else { continue };
            let is_open_endpoint = !poly.closed && (i == 0 || i.saturating_add(1) == n);
            let (n1, n2) = if poly.closed {
                let prev_i = i.saturating_add(n.saturating_sub(1)).checked_rem(n).unwrap_or(0);
                let next_i = i.saturating_add(1).checked_rem(n).unwrap_or(0);
                let prev = pts.get(prev_i).copied().unwrap_or(cur_pt);
                let next = pts.get(next_i).copied().unwrap_or(cur_pt);
                (seg_normal(prev, cur_pt), seg_normal(cur_pt, next))
            } else if i == 0 {
                let next = pts.get(1).copied().unwrap_or(cur_pt);
                let n0 = seg_normal(cur_pt, next);
                (n0, n0)
            } else if i.saturating_add(1) == n {
                let prev = pts.get(n.saturating_sub(2)).copied().unwrap_or(cur_pt);
                let nl = seg_normal(prev, cur_pt);
                (nl, nl)
            } else {
                let prev = pts.get(i.saturating_sub(1)).copied().unwrap_or(cur_pt);
                let next = pts.get(i.saturating_add(1)).copied().unwrap_or(cur_pt);
                (seg_normal(prev, cur_pt), seg_normal(cur_pt, next))
            };
            if is_open_endpoint {
                shifted.push(Point::new(cur_pt.x.get() + n1.0 * eff_d, cur_pt.y.get() + n1.1 * eff_d)?);
                continue;
            }
            let norm_diff = (n1.0 - n2.0).hypot(n1.1 - n2.1);
            match join {
                OffsetJoinKind::Miter => {
                    let bx = n1.0 + n2.0;
                    let by = n1.1 + n2.1;
                    let blen = bx.hypot(by);
                    let (ox, oy) = if blen <= 1e-6 {
                        (n1.0 * eff_d, n1.1 * eff_d)
                    } else {
                        let ux = bx / blen;
                        let uy = by / blen;
                        let dot = (ux * n1.0 + uy * n1.1).max(0.4);
                        let miter = (eff_d / dot).clamp(-2.5 * eff_d.abs(), 2.5 * eff_d.abs());
                        (ux * miter, uy * miter)
                    };
                    shifted.push(Point::new(cur_pt.x.get() + ox, cur_pt.y.get() + oy)?);
                }
                OffsetJoinKind::Bevel => {
                    shifted.push(Point::new(cur_pt.x.get() + n1.0 * eff_d, cur_pt.y.get() + n1.1 * eff_d)?);
                    if norm_diff > 1e-4 {
                        shifted.push(Point::new(cur_pt.x.get() + n2.0 * eff_d, cur_pt.y.get() + n2.1 * eff_d)?);
                    }
                }
                OffsetJoinKind::Round => {
                    if norm_diff <= 1e-4 {
                        shifted.push(Point::new(cur_pt.x.get() + n1.0 * eff_d, cur_pt.y.get() + n1.1 * eff_d)?);
                    } else {
                        let a1 = (n1.1 * eff_d).atan2(n1.0 * eff_d);
                        let a2 = (n2.1 * eff_d).atan2(n2.0 * eff_d);
                        let da = (a2 - a1 + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI;
                        let r = eff_d.abs();
                        let arc_steps = 6_u32;
                        for k in 0..=arc_steps {
                            let angle = a1 + da * (f64::from(k) / f64::from(arc_steps));
                            shifted.push(Point::new(cur_pt.x.get() + r * angle.cos(), cur_pt.y.get() + r * angle.sin())?);
                        }
                    }
                }
            }
        }
        if let Some((&first, rest)) = shifted.split_first() {
            out.move_to(first)?;
            for &pt in rest {
                out.line_to(pt)?;
            }
        }
        if poly.closed {
            out.close()?;
        }
    }
    Ok(out)
}

/// Computes `(total_arc_length_pt, enclosed_area_pt2)` for `path` (`CorelDRAW Curve Statistics`).
///
/// Open subpaths contribute to `total_arc_length_pt` and `0.0` to `enclosed_area_pt2`; closed
/// subpaths include their closing chord in `total_arc_length_pt` and accumulate signed Shoelace
/// area ($\frac{1}{2} \sum (x_i y_{i+1} - x_{i+1} y_i)$).
pub fn measure_path_length_and_area(path: &Path) -> Result<(f64, f64), GeomError> {
    let polylines = crate::flatten::flatten_path(path, &crate::flatten::FlattenOpts::default())?;
    let mut total_len = 0.0;
    let mut signed_area2 = 0.0;
    for poly in polylines {
        let n = poly.points.len();
        if n < 2 {
            continue;
        }
        for seg in poly.points.windows(2) {
            if let [p0, p1] = *seg {
                let dx = p1.x.get() - p0.x.get();
                let dy = p1.y.get() - p0.y.get();
                total_len += dx.hypot(dy);
            }
        }
        if poly.closed {
            if let (Some(&first), Some(&last)) = (poly.points.first(), poly.points.last()) {
                total_len += (first.x.get() - last.x.get()).hypot(first.y.get() - last.y.get());
            }
            if n >= 3 {
                let mut sub_area2 = 0.0;
                for i in 0..n {
                    let next_i = i.saturating_add(1).checked_rem(n).unwrap_or(0);
                    if let (Some(&p0), Some(&p1)) = (poly.points.get(i), poly.points.get(next_i)) {
                        sub_area2 += p0.x.get() * p1.y.get() - p1.x.get() * p0.y.get();
                    }
                }
                signed_area2 += sub_area2;
            }
        }
    }
    Ok((total_len, 0.5 * signed_area2.abs()))
}

/// Converts the node at the end of `element_index` into a smooth symmetrical cubic Bézier node
/// (`Shape Tool F10 -> Smooth / Symmetrical Node`), aligning incoming `c2` and outgoing `c1`
/// along the chord tangent between neighboring anchors.
pub fn smooth_curve_node_at(path: &Path, element_index: usize, tension: f64) -> Result<Path, GeomError> {
    if !tension.is_finite() {
        return Err(GeomError::NonFiniteCoordinate);
    }
    if element_index == 0 || element_index >= path.elements.len() {
        return Err(GeomError::InvalidShapeParameter("element_index out of range"));
    }
    let elem_end = |el: &PathElement| -> Option<Point> {
        match *el {
            PathElement::MoveTo(p) | PathElement::LineTo(p) | PathElement::QuadTo { end: p, .. } | PathElement::CubicTo { end: p, .. } => Some(p),
            PathElement::Close => None,
        }
    };
    let Some(p_prev) = path.elements.get(element_index.saturating_sub(1)).and_then(elem_end) else {
        return Err(GeomError::InvalidShapeParameter("previous element has no endpoint"));
    };
    let Some(p_cur) = path.elements.get(element_index).and_then(elem_end) else {
        return Err(GeomError::InvalidShapeParameter("target element has no endpoint"));
    };
    let next_idx = element_index.checked_add(1).filter(|&ni| path.elements.get(ni).and_then(elem_end).is_some());
    let p_next = next_idx.and_then(|idx| path.elements.get(idx).and_then(elem_end)).unwrap_or(p_cur);
    let tau = tension.clamp(0.05, 0.5);
    let tx = (p_next.x.get() - p_prev.x.get()) * 0.5 * tau;
    let ty = (p_next.y.get() - p_prev.y.get()) * 0.5 * tau;
    let in_c2 = Point::new(p_cur.x.get() - tx, p_cur.y.get() - ty)?;
    let out_c1 = Point::new(p_cur.x.get() + tx, p_cur.y.get() + ty)?;

    let mut elements = path.elements.clone();
    let in_c1 = match elements.get(element_index).copied() {
        Some(PathElement::CubicTo { c1, .. }) => c1,
        _ => Point::new(p_prev.x.get() + (p_cur.x.get() - p_prev.x.get()) / 3.0, p_prev.y.get() + (p_cur.y.get() - p_prev.y.get()) / 3.0)?,
    };
    if let Some(slot) = elements.get_mut(element_index) {
        *slot = PathElement::CubicTo { c1: in_c1, c2: in_c2, end: p_cur };
    }

    if let Some(ni) = next_idx {
        let out_c2 = match elements.get(ni).copied() {
            Some(PathElement::CubicTo { c2, .. }) => c2,
            _ => Point::new(p_next.x.get() - (p_next.x.get() - p_cur.x.get()) / 3.0, p_next.y.get() - (p_next.y.get() - p_cur.y.get()) / 3.0)?,
        };
        if let Some(slot) = elements.get_mut(ni) {
            *slot = PathElement::CubicTo { c1: out_c1, c2: out_c2, end: p_next };
        }
    }
    Path::from_elements(elements, path.fill_rule)
}

/// Subdivides every line, quadratic, and cubic segment of `path` at its midpoint $t = 0.5$
/// (`Shape Tool F10 -> Select All Nodes -> Add Nodes (+)`).
pub fn subdivide_all_path_segments(path: &Path) -> Result<Path, GeomError> {
    if path.elements.is_empty() {
        return Err(GeomError::InvalidShapeParameter("cannot subdivide empty path"));
    }
    let mid_pt = |a: Point, b: Point| -> Result<Point, GeomError> { Point::new(0.5 * (a.x.get() + b.x.get()), 0.5 * (a.y.get() + b.y.get())) };
    let mut out = Vec::with_capacity(path.elements.len().saturating_mul(2));
    let mut cur = Point::ZERO;
    for &el in &path.elements {
        match el {
            PathElement::MoveTo(p) => {
                cur = p;
                out.push(PathElement::MoveTo(p));
            }
            PathElement::LineTo(p) => {
                let m = mid_pt(cur, p)?;
                out.push(PathElement::LineTo(m));
                out.push(PathElement::LineTo(p));
                cur = p;
            }
            PathElement::QuadTo { ctrl, end } => {
                let q0 = mid_pt(cur, ctrl)?;
                let q1 = mid_pt(ctrl, end)?;
                let m = mid_pt(q0, q1)?;
                out.push(PathElement::QuadTo { ctrl: q0, end: m });
                out.push(PathElement::QuadTo { ctrl: q1, end });
                cur = end;
            }
            PathElement::CubicTo { c1, c2, end } => {
                let m01 = mid_pt(cur, c1)?;
                let m12 = mid_pt(c1, c2)?;
                let m23 = mid_pt(c2, end)?;
                let q0 = mid_pt(m01, m12)?;
                let q1 = mid_pt(m12, m23)?;
                let m = mid_pt(q0, q1)?;
                out.push(PathElement::CubicTo { c1: m01, c2: q0, end: m });
                out.push(PathElement::CubicTo { c1: q1, c2: m23, end });
                cur = end;
            }
            PathElement::Close => {
                out.push(PathElement::Close);
            }
        }
    }
    Path::from_elements(out, path.fill_rule)
}

/// Scales (`scale_x`, `scale_y`) and rotates (`rotate_deg`) the curve nodes at `element_indices`
/// around their collective centroid (`Shape Tool F10 -> Stretch/Scale & Rotate Nodes`).
pub fn transform_curve_nodes(path: &Path, element_indices: &[usize], scale_x: f64, scale_y: f64, rotate_deg: f64) -> Result<Path, GeomError> {
    if !scale_x.is_finite() || !scale_y.is_finite() || !rotate_deg.is_finite() {
        return Err(GeomError::NonFiniteCoordinate);
    }
    if scale_x.abs() <= 1e-6 || scale_y.abs() <= 1e-6 || element_indices.is_empty() {
        return Err(GeomError::InvalidShapeParameter("invalid node transform parameters"));
    }
    let elem_end = |el: &PathElement| -> Option<Point> {
        match *el {
            PathElement::MoveTo(p) | PathElement::LineTo(p) | PathElement::QuadTo { end: p, .. } | PathElement::CubicTo { end: p, .. } => Some(p),
            PathElement::Close => None,
        }
    };
    let mut sum_x = 0.0;
    let mut sum_y = 0.0;
    let mut count = 0usize;
    for &idx in element_indices {
        let Some(el) = path.elements.get(idx) else {
            return Err(GeomError::InvalidShapeParameter("element_index out of range"));
        };
        let Some(p) = elem_end(el) else {
            return Err(GeomError::InvalidShapeParameter("Close element has no endpoint"));
        };
        sum_x += p.x.get();
        sum_y += p.y.get();
        count = count.saturating_add(1);
    }
    let cx = sum_x / (count as f64);
    let cy = sum_y / (count as f64);
    let t1 = Affine2::translation(-cx, -cy)?;
    let sc = Affine2::scaling(scale_x, scale_y)?;
    let rot = Affine2::rotation_rad(rotate_deg.to_radians())?;
    let t2 = Affine2::translation(cx, cy)?;
    let tf = t1.then(sc)?.then(rot)?.then(t2)?;

    let mut elements = path.elements.clone();
    for &idx in element_indices {
        let Some(old_end) = elements.get(idx).and_then(elem_end) else {
            continue;
        };
        let new_end = tf.apply_point(old_end)?;
        let dx = new_end.x.get() - old_end.x.get();
        let dy = new_end.y.get() - old_end.y.get();
        if let Some(el_mut) = elements.get_mut(idx) {
            match el_mut {
                PathElement::MoveTo(p) | PathElement::LineTo(p) => *p = new_end,
                PathElement::QuadTo { ctrl, end } => {
                    *ctrl = Point::new(ctrl.x.get() + dx, ctrl.y.get() + dy)?;
                    *end = new_end;
                }
                PathElement::CubicTo { c2, end, .. } => {
                    *c2 = Point::new(c2.x.get() + dx, c2.y.get() + dy)?;
                    *end = new_end;
                }
                PathElement::Close => {}
            }
        }
        if let Some(PathElement::CubicTo { c1, .. }) = elements.get_mut(idx.saturating_add(1)) {
            *c1 = Point::new(c1.x.get() + dx, c1.y.get() + dy)?;
        }
    }
    Path::from_elements(elements, path.fill_rule)
}

/// Breaks a curve at `element_index` (`Shape Tool -> Break Curve`), splitting an open subpath into
/// two open subpaths at the selected node or opening a closed subpath at the selected node.
pub fn break_path_at_node(path: &Path, element_index: usize) -> Result<Path, GeomError> {
    let Some(el) = path.elements.get(element_index) else {
        return Err(GeomError::InvalidShapeParameter("element_index out of range"));
    };
    let elem_end = |e: &PathElement| -> Option<Point> {
        match *e {
            PathElement::MoveTo(p) | PathElement::LineTo(p) | PathElement::QuadTo { end: p, .. } | PathElement::CubicTo { end: p, .. } => Some(p),
            PathElement::Close => None,
        }
    };
    let Some(break_pt) = elem_end(el) else {
        return Err(GeomError::InvalidShapeParameter("Close element cannot be broken"));
    };

    // Locate the enclosing subpath [sub_start..=sub_end]
    let mut sub_start = element_index;
    while sub_start > 0 && !matches!(path.elements.get(sub_start), Some(PathElement::MoveTo(_))) {
        sub_start = sub_start.saturating_sub(1);
    }
    let Some(PathElement::MoveTo(start_pt)) = path.elements.get(sub_start).copied() else {
        return Err(GeomError::InvalidShapeParameter("subpath missing MoveTo"));
    };
    let mut sub_end = element_index;
    while sub_end.saturating_add(1) < path.elements.len() && !matches!(path.elements.get(sub_end.saturating_add(1)), Some(PathElement::MoveTo(_))) {
        sub_end = sub_end.saturating_add(1);
    }
    let is_closed = matches!(path.elements.get(sub_end), Some(PathElement::Close));
    if !is_closed && (element_index == sub_start || element_index == sub_end) {
        return Err(GeomError::InvalidShapeParameter("cannot break open subpath at its start or end node"));
    }

    let mut out = Vec::with_capacity(path.elements.len().saturating_add(2));
    for (i, item) in path.elements.iter().copied().enumerate() {
        if i == sub_end && is_closed {
            if start_pt != break_pt || element_index != sub_start {
                out.push(PathElement::LineTo(start_pt));
            }
            continue;
        }
        out.push(item);
        if i == element_index && element_index > sub_start {
            out.push(PathElement::MoveTo(break_pt));
        }
    }
    Path::from_elements(out, path.fill_rule)
}

/// Topologically morphs `start_path` toward `end_path` at interpolation factor `t` (`0.0..=1.0`)
/// by resampling both contours along their normalized arc lengths (`ArcLengthTable`) into `sample_count`
/// vertices (`Effects -> Blend -> Morph Paths`).
pub fn morph_paths_linear(start_path: &Path, end_path: &Path, t: f64, sample_count: usize) -> Result<Path, GeomError> {
    if !t.is_finite() || !(0.0..=1.0).contains(&t) || sample_count < 3 {
        return Err(GeomError::InvalidShapeParameter("invalid morph factor or sample_count"));
    }
    let lut_a = ArcLengthTable::from_path(start_path)?;
    let lut_b = ArcLengthTable::from_path(end_path)?;
    if lut_a.total_length() <= 1e-6 || lut_b.total_length() <= 1e-6 {
        return Err(GeomError::InvalidShapeParameter("cannot morph zero-length path"));
    }
    let n = sample_count.clamp(3, 512);
    let closed =
        start_path.elements.iter().any(|e| matches!(e, PathElement::Close)) && end_path.elements.iter().any(|e| matches!(e, PathElement::Close));
    let denom = if closed { n as f64 } else { n.saturating_sub(1) as f64 };

    let mut out = Path::new();
    out.fill_rule = start_path.fill_rule;
    for i in 0..n {
        let u = (i as f64) / denom;
        let Some(sa) = lut_a.sample_normalized(u) else {
            return Err(GeomError::InvalidShapeParameter("failed to sample start path"));
        };
        let Some(sb) = lut_b.sample_normalized(u) else {
            return Err(GeomError::InvalidShapeParameter("failed to sample end path"));
        };
        let mx = (1.0 - t) * sa.point.x.get() + t * sb.point.x.get();
        let my = (1.0 - t) * sa.point.y.get() + t * sb.point.y.get();
        let pt = Point::new(mx, my)?;
        if i == 0 {
            out.move_to(pt)?;
        } else {
            out.line_to(pt)?;
        }
    }
    if closed {
        out.close()?;
    }
    Ok(out)
}

/// Splits two overlapping closed paths `subject` and `clip` into up to three non-overlapping
/// regions (`subject \ clip`, `subject ∩ clip`, `clip \ subject`) (`Object -> Shaping -> Divide`).
pub fn divide_overlapping_paths(subject: &Path, clip: &Path) -> Result<Vec<Path>, GeomError> {
    use crate::boolean::{BooleanOp, boolean_paths};
    let mut regions = Vec::with_capacity(3);
    for op in [BooleanOp::FrontMinusBack, BooleanOp::Intersect, BooleanOp::BackMinusFront] {
        if let Ok(part) = boolean_paths(subject, clip, op)
            && !part.elements.is_empty()
        {
            regions.push(part);
        }
    }
    if regions.is_empty() {
        return Err(GeomError::InvalidShapeParameter("divide produced no non-empty regions"));
    }
    Ok(regions)
}

/// Generates parallel and optional cross-hatch engraving strokes inside `path` at `angle_deg`
/// and `spacing_pt` (`Object -> Hatch / Engraving Fill`).
pub fn hatch_path_region(path: &Path, angle_deg: f64, spacing_pt: f64, cross_hatch: bool) -> Result<Path, GeomError> {
    if !angle_deg.is_finite() || !spacing_pt.is_finite() || spacing_pt < 0.5 {
        return Err(GeomError::InvalidShapeParameter("invalid hatch angle or spacing"));
    }
    let polys = crate::flatten::flatten_path(path, &crate::flatten::FlattenOpts::default())?;
    let mut edges = Vec::new();
    for poly in &polys {
        if poly.points.len() < 2 {
            continue;
        }
        for w in poly.points.windows(2) {
            if let [p0, p1] = *w {
                edges.push((p0, p1));
            }
        }
        if let (Some(&first), Some(&last)) = (poly.points.first(), poly.points.last())
            && first != last
        {
            edges.push((last, first));
        }
    }
    if edges.is_empty() {
        return Err(GeomError::InvalidShapeParameter("cannot hatch empty path"));
    }

    let step = spacing_pt.max(0.5);
    let mut out = Path::new();
    let angles = if cross_hatch { vec![angle_deg, angle_deg + 90.0] } else { vec![angle_deg] };
    for ang in angles {
        let (s, c) = ang.to_radians().sin_cos();
        let mut v_min = f64::INFINITY;
        let mut v_max = f64::NEG_INFINITY;
        let mut rot_edges = Vec::with_capacity(edges.len());
        for &(p0, p1) in &edges {
            let u0 = p0.x.get() * c + p0.y.get() * s;
            let v0 = -p0.x.get() * s + p0.y.get() * c;
            let u1 = p1.x.get() * c + p1.y.get() * s;
            let v1 = -p1.x.get() * s + p1.y.get() * c;
            v_min = v_min.min(v0).min(v1);
            v_max = v_max.max(v0).max(v1);
            rot_edges.push((u0, v0, u1, v1));
        }
        let mut v = v_min + 0.5 * step;
        let mut line_count = 0usize;
        let prev_len = out.elements.len();
        let emit_scanline = |v_scan: f64, out: &mut Path| -> Result<(), GeomError> {
            let mut hits = Vec::new();
            for &(u0, v0, u1, v1) in &rot_edges {
                if (v0 <= v_scan && v1 > v_scan) || (v1 <= v_scan && v0 > v_scan) {
                    let t = (v_scan - v0) / (v1 - v0);
                    hits.push(u0 + t * (u1 - u0));
                }
            }
            hits.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            for &[ua, ub] in hits.as_chunks::<2>().0 {
                if (ub - ua).abs() > 1e-6 {
                    let pa = Point::new(ua * c - v_scan * s, ua * s + v_scan * c)?;
                    let pb = Point::new(ub * c - v_scan * s, ub * s + v_scan * c)?;
                    out.move_to(pa)?;
                    out.line_to(pb)?;
                }
            }
            Ok(())
        };
        while v < v_max && line_count < 512 {
            line_count = line_count.saturating_add(1);
            emit_scanline(v, &mut out)?;
            v += step;
        }
        if out.elements.len() == prev_len {
            for &(u0, v0, u1, v1) in &rot_edges {
                let _ = (u0, u1);
                if (v1 - v0).abs() > 1e-6 {
                    emit_scanline(0.5 * (v0 + v1), &mut out)?;
                    if out.elements.len() > prev_len {
                        break;
                    }
                }
            }
        }
    }
    if out.elements.is_empty() {
        return Err(GeomError::InvalidShapeParameter("hatch spacing too wide for path bounds"));
    }
    Ok(out)
}

/// Standard $30^\circ$ axonometric / isometric projection planes (`Object -> Transformations -> Isometric Projection`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AxonometricPlane {
    /// Top horizontal isometric plane (rotate $45^\circ$, scale Y by $\tan(30^\circ) \approx 0.57735$).
    IsometricTop,
    /// Left vertical isometric plane (scale X by $\cos(30^\circ) \approx 0.866025$, shear Y by $+30^\circ$).
    IsometricLeft,
    /// Right vertical isometric plane (scale X by $\cos(30^\circ) \approx 0.866025$, shear Y by $-30^\circ$).
    IsometricRight,
}

/// Projects `path` around its bounding-box center onto one of the three standard $30^\circ$
/// isometric planes (`AxonometricPlane`).
pub fn project_path_axonometric(path: &Path, plane: AxonometricPlane) -> Result<Path, GeomError> {
    let Some(bbox) = path.bounds() else {
        return Err(GeomError::InvalidShapeParameter("cannot project empty path"));
    };
    let cx = 0.5 * (bbox.x0.get() + bbox.x1.get());
    let cy = 0.5 * (bbox.y0.get() + bbox.y1.get());
    let t1 = Affine2::translation(-cx, -cy)?;
    let t2 = Affine2::translation(cx, cy)?;
    let cos30 = 30.0_f64.to_radians().cos();
    let sin30 = 0.5_f64;
    let tan30 = 30.0_f64.to_radians().tan();
    let iso = match plane {
        AxonometricPlane::IsometricTop => {
            let rot = Affine2::rotation_rad(45.0_f64.to_radians())?;
            let sc = Affine2::scaling(1.0, tan30)?;
            rot.then(sc)?
        }
        AxonometricPlane::IsometricLeft => Affine2::new(cos30, sin30, 0.0, 1.0, 0.0, 0.0)?,
        AxonometricPlane::IsometricRight => Affine2::new(cos30, -sin30, 0.0, 1.0, 0.0, 0.0)?,
    };
    let tf = t1.then(iso)?.then(t2)?;
    path.transform(tf)
}

/// Expands `spine` into a closed filled ribbon [`Path`] using a multi-stop width profile `widths_pt`
/// (`>= 2` non-negative width stops interpolated along normalized arc length $u \in [0, 1]$).
pub fn expand_tapered_stroke_profile(spine: &Path, widths_pt: &[f64]) -> Result<Path, GeomError> {
    if widths_pt.len() < 2 {
        return Err(GeomError::InvalidShapeParameter("tapered stroke profile requires at least 2 width stops"));
    }
    let mut any_positive = false;
    for &w in widths_pt {
        if !w.is_finite() {
            return Err(GeomError::NonFiniteCoordinate);
        }
        if w < 0.0 {
            return Err(GeomError::InvalidShapeParameter("tapered stroke width stops must be non-negative"));
        }
        if w > 1e-6 {
            any_positive = true;
        }
    }
    if !any_positive {
        return Err(GeomError::InvalidShapeParameter("tapered stroke profile requires at least one positive width stop"));
    }
    let lut = ArcLengthTable::from_path(spine)?;
    if lut.total_length() <= 1e-6 {
        return Err(GeomError::InvalidShapeParameter("cannot expand zero-length spine"));
    }

    let steps = 32usize;
    let max_idx = widths_pt.len().saturating_sub(1) as f64;
    let eval_width = |u: f64| -> f64 {
        let pos = (u.clamp(0.0, 1.0) * max_idx).clamp(0.0, max_idx);
        let idx = (pos.floor() as usize).min(widths_pt.len().saturating_sub(2));
        let frac = pos - (idx as f64);
        let w0 = widths_pt.get(idx).copied().unwrap_or(0.0);
        let w1 = widths_pt.get(idx.saturating_add(1)).copied().unwrap_or(w0);
        (1.0 - frac) * w0 + frac * w1
    };

    let mut left_pts = Vec::with_capacity(steps.saturating_add(1));
    let mut right_pts = Vec::with_capacity(steps.saturating_add(1));
    for i in 0..=steps {
        let u = (i as f64) / (steps as f64);
        if let Some(sample) = lut.sample_normalized(u) {
            let half_w = 0.5 * eval_width(u);
            let nx = -sample.tangent.1;
            let ny = sample.tangent.0;
            let px = sample.point.x.get();
            let py = sample.point.y.get();
            left_pts.push(Point::new(px + half_w * nx, py + half_w * ny)?);
            right_pts.push(Point::new(px - half_w * nx, py - half_w * ny)?);
        }
    }
    let Some((&first_left, rest_left)) = left_pts.split_first() else {
        return Err(GeomError::InvalidShapeParameter("insufficient spine samples"));
    };
    if rest_left.is_empty() {
        return Err(GeomError::InvalidShapeParameter("insufficient spine samples"));
    }
    let mut out = Path::new();
    out.move_to(first_left)?;
    for &p in rest_left {
        out.line_to(p)?;
    }
    for &p in right_pts.iter().rev() {
        out.line_to(p)?;
    }
    out.close()?;
    Ok(out)
}

/// Refines `path` using `iterations` (`1..=5`) of Chaikin's $(3/4, 1/4)$ corner-cutting
/// subdivision algorithm (`Shape Tool -> Smooth Curve Subdivision`).
pub fn chaikin_smooth_path(path: &Path, iterations: u32) -> Result<Path, GeomError> {
    if path.elements.is_empty() || iterations == 0 {
        return Err(GeomError::InvalidShapeParameter("invalid Chaikin smoothing parameters"));
    }
    let iters = iterations.clamp(1, 5);
    let polys = crate::flatten::flatten_path(path, &crate::flatten::FlattenOpts::default())?;
    let mut out = Path::new();
    out.fill_rule = path.fill_rule;

    for poly in polys {
        if poly.points.len() < 2 {
            continue;
        }
        let mut pts = poly.points;
        // Strip duplicate closing vertex if present so closed wrap-around doesn't double-count
        if poly.closed && pts.len() >= 3 && pts.first() == pts.last() {
            pts.pop();
        }
        for _ in 0..iters {
            let n = pts.len();
            if n < 2 {
                break;
            }
            if poly.closed {
                if !(3..=2048).contains(&n) {
                    break;
                }
                let mut next = Vec::with_capacity(n.saturating_mul(2));
                for i in 0..n {
                    let next_i = i.saturating_add(1).checked_rem(n).unwrap_or(0);
                    let (Some(&p0), Some(&p1)) = (pts.get(i), pts.get(next_i)) else {
                        continue;
                    };
                    next.push(Point::new(0.75 * p0.x.get() + 0.25 * p1.x.get(), 0.75 * p0.y.get() + 0.25 * p1.y.get())?);
                    next.push(Point::new(0.25 * p0.x.get() + 0.75 * p1.x.get(), 0.25 * p0.y.get() + 0.75 * p1.y.get())?);
                }
                pts = next;
            } else {
                if n > 2048 {
                    break;
                }
                let (Some(&first_pt), Some(&last_pt)) = (pts.first(), pts.last()) else {
                    break;
                };
                let mut next = Vec::with_capacity(n.saturating_mul(2));
                next.push(first_pt);
                for w in pts.windows(2) {
                    let [p0, p1] = *w else { continue };
                    next.push(Point::new(0.75 * p0.x.get() + 0.25 * p1.x.get(), 0.75 * p0.y.get() + 0.25 * p1.y.get())?);
                    next.push(Point::new(0.25 * p0.x.get() + 0.75 * p1.x.get(), 0.25 * p0.y.get() + 0.75 * p1.y.get())?);
                }
                next.push(last_pt);
                pts = next;
            }
        }
        if pts.len() >= 2
            && let Some((&first, rest)) = pts.split_first()
        {
            out.move_to(first)?;
            for &p in rest {
                out.line_to(p)?;
            }
            if poly.closed {
                out.close()?;
            }
        }
    }
    if out.elements.is_empty() {
        return Err(GeomError::InvalidShapeParameter("Chaikin subdivision produced empty path"));
    }
    Ok(out)
}

/// Applies a sinusoidal wave distortion along the local normals of `path`
/// (`Effects -> Distort -> Wave Distortion`).
///
/// Subdivides segments if needed and displaces each vertex along its local normal by
/// `amplitude_pt * sin(2 * PI * frequency * (s / total_len) + phase_rad)`.
pub fn wave_distort_path(path: &Path, amplitude_pt: f64, frequency: f64, phase_deg: f64) -> Result<Path, GeomError> {
    let amp = crate::F64::new(amplitude_pt)?.get();
    let freq = crate::F64::new(frequency)?.get();
    let phase_rad = crate::F64::new(phase_deg)?.get().to_radians();
    if freq <= 0.0 || freq > 128.0 {
        return Err(GeomError::InvalidShapeParameter("wave frequency must be in (0, 128]"));
    }
    // Subdivide twice so straight edges have enough vertices to carry the wave
    let dense = subdivide_all_path_segments(&subdivide_all_path_segments(path)?)?;
    let polys = crate::flatten::flatten_path(&dense, &crate::flatten::FlattenOpts::default())?;
    let mut out = Path::new();
    out.fill_rule = path.fill_rule;

    for poly in polys {
        let n = poly.points.len();
        if n < 2 {
            continue;
        }
        let mut cum_s = Vec::with_capacity(n);
        let mut total_s = 0.0;
        cum_s.push(0.0);
        for w in poly.points.windows(2) {
            if let [p0, p1] = *w {
                total_s += (p1.x.get() - p0.x.get()).hypot(p1.y.get() - p0.y.get());
                cum_s.push(total_s);
            }
        }
        let inv_len = if total_s > 1e-6 { 1.0 / total_s } else { 0.0 };
        let mut displaced = Vec::with_capacity(n);
        let (Some(&first_poly_pt), Some(&last_poly_pt)) = (poly.points.first(), poly.points.last()) else {
            continue;
        };
        for (i, &s_val) in cum_s.iter().enumerate() {
            let Some(&cur_pt) = poly.points.get(i) else { continue };
            let p_prev = if i > 0 { poly.points.get(i.saturating_sub(1)).copied().unwrap_or(first_poly_pt) } else { first_poly_pt };
            let p_next = poly.points.get(i.saturating_add(1)).copied().unwrap_or(last_poly_pt);
            let dx = p_next.x.get() - p_prev.x.get();
            let dy = p_next.y.get() - p_prev.y.get();
            let len = dx.hypot(dy);
            let (nx, ny) = if len > 1e-9 { (-dy / len, dx / len) } else { (0.0, -1.0) };
            let u = s_val * inv_len;
            let offset = amp * (std::f64::consts::TAU * freq * u + phase_rad).sin();
            displaced.push(Point::new(cur_pt.x.get() + nx * offset, cur_pt.y.get() + ny * offset)?);
        }
        if poly.closed
            && displaced.len() >= 3
            && let Some(&first_disp) = displaced.first()
            && let Some(last_slot) = displaced.last_mut()
        {
            // Ensure exact closure if first and last points coincided
            *last_slot = first_disp;
        }
        if let Some((&first, rest)) = displaced.split_first() {
            out.move_to(first)?;
            for &pt in rest {
                out.line_to(pt)?;
            }
        }
        if poly.closed {
            out.close()?;
        }
    }
    if out.elements.is_empty() {
        return Err(GeomError::InvalidShapeParameter("wave distortion produced empty path"));
    }
    Ok(out)
}

/// Builds a cyclic ($C_n$, `reflect = false`) or dihedral ($D_n$, `reflect = true`) radial
/// kaleidoscope symmetry compound [`Path`] around `center` with `folds` (`2..=32`) symmetry sectors
/// (`Object -> Symmetry -> Create New Symmetry`).
pub fn build_dihedral_symmetry_path(path: &Path, center: Point, folds: u32, reflect: bool) -> Result<Path, GeomError> {
    if !(2..=32).contains(&folds) || path.elements.is_empty() {
        return Err(GeomError::InvalidShapeParameter("symmetry folds must be in 2..=32 and path must be non-empty"));
    }
    let cx = center.x.get();
    let cy = center.y.get();
    let t_to_origin = Affine2::translation(-cx, -cy)?;
    let t_back = Affine2::translation(cx, cy)?;
    let mirror_y = Affine2::scaling(1.0, -1.0)?;

    let mut out = Path::new();
    out.fill_rule = path.fill_rule;

    for k in 0..folds {
        let angle_deg = 360.0 * (f64::from(k) / f64::from(folds));
        let rot = Affine2::rotation_deg(angle_deg)?;
        let tf_rot = t_to_origin.then(rot)?.then(t_back)?;
        let rotated = path.transform(tf_rot)?;
        for elem in rotated.elements {
            if out.elements.len() >= MAX_PATH_ELEMENTS {
                return Ok(out);
            }
            out.elements.push(elem);
        }
        if reflect {
            let tf_ref = t_to_origin.then(mirror_y)?.then(rot)?.then(t_back)?;
            let reflected = path.transform(tf_ref)?;
            for elem in reflected.elements {
                if out.elements.len() >= MAX_PATH_ELEMENTS {
                    return Ok(out);
                }
                out.elements.push(elem);
            }
        }
    }
    Ok(out)
}

/// Splits `path` along its flattened arc length into discrete open dashed subpaths
/// according to `dash_pattern_pt` (`Object -> Shaping -> Split Curve into Dashes`).
pub fn dash_path_to_open_subpaths(path: &Path, dash_pattern_pt: &[f64]) -> Result<Path, GeomError> {
    if dash_pattern_pt.is_empty() {
        return Err(GeomError::InvalidShapeParameter("dash pattern must not be empty"));
    }
    let mut pattern = Vec::with_capacity(dash_pattern_pt.len().saturating_mul(2));
    for &d in dash_pattern_pt {
        let val = crate::F64::new(d)?.get();
        if val <= 0.0 {
            return Err(GeomError::InvalidShapeParameter("dash pattern entries must be positive"));
        }
        pattern.push(val);
    }
    if pattern.len() % 2 == 1 {
        let clone = pattern.clone();
        pattern.extend_from_slice(&clone);
    }
    let polys = crate::flatten::flatten_path(path, &crate::flatten::FlattenOpts::default())?;
    let mut out = Path::new();
    out.fill_rule = path.fill_rule;

    for poly in polys {
        if poly.points.len() < 2 {
            continue;
        }
        let mut pat_idx = 0usize;
        let mut pat_rem = pattern.first().copied().unwrap_or(1.0);
        let mut draw_on = true;
        let mut in_dash = false;

        for seg in poly.points.windows(2) {
            let [p0, p1] = *seg else { continue };
            let dx = p1.x.get() - p0.x.get();
            let dy = p1.y.get() - p0.y.get();
            let seg_len = dx.hypot(dy);
            if seg_len <= 1e-9 {
                continue;
            }
            let mut dist = 0.0;
            while dist < seg_len - 1e-9 {
                let step = pat_rem.min(seg_len - dist);
                let t0 = dist / seg_len;
                let t1 = (dist + step) / seg_len;
                if draw_on {
                    let s_pt = Point::new(p0.x.get() + dx * t0, p0.y.get() + dy * t0)?;
                    let e_pt = Point::new(p0.x.get() + dx * t1, p0.y.get() + dy * t1)?;
                    if !in_dash {
                        out.move_to(s_pt)?;
                        in_dash = true;
                    }
                    out.line_to(e_pt)?;
                }
                dist += step;
                pat_rem -= step;
                if pat_rem <= 1e-9 {
                    pat_idx = pat_idx.saturating_add(1).checked_rem(pattern.len()).unwrap_or(0);
                    pat_rem = pattern.get(pat_idx).copied().unwrap_or(1.0);
                    draw_on = !draw_on;
                    if !draw_on {
                        in_dash = false;
                    }
                }
            }
        }
    }
    if out.elements.is_empty() {
        return Err(GeomError::InvalidShapeParameter("dash splitting produced empty path"));
    }
    Ok(out)
}

/// Computes the 2D Andrew monotone-chain convex hull polygon [`Path`] enclosing all flattened
/// vertices of `path` (`Object -> Shaping -> Convex Hull`).
pub fn convex_hull_of_path(path: &Path) -> Result<Path, GeomError> {
    let polys = crate::flatten::flatten_path(path, &crate::flatten::FlattenOpts::default())?;
    let mut pts: Vec<(f64, f64)> = Vec::new();
    for poly in polys {
        for p in poly.points {
            pts.push((p.x.get(), p.y.get()));
        }
    }
    if pts.len() < 3 {
        return Err(GeomError::InvalidShapeParameter("convex hull requires at least 3 vertices"));
    }
    pts.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.total_cmp(&b.1)));
    pts.dedup_by(|a, b| (a.0 - b.0).abs() <= 1e-9 && (a.1 - b.1).abs() <= 1e-9);
    if pts.len() < 3 {
        return Err(GeomError::InvalidShapeParameter("convex hull requires at least 3 non-coincident vertices"));
    }

    let cross = |o: (f64, f64), a: (f64, f64), b: (f64, f64)| -> f64 { (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0) };

    let mut lower: Vec<(f64, f64)> = Vec::with_capacity(pts.len());
    for &p in &pts {
        while let [.., o, a] = lower.as_slice()
            && cross(*o, *a, p) <= 1e-9
        {
            lower.pop();
        }
        lower.push(p);
    }

    let mut upper: Vec<(f64, f64)> = Vec::with_capacity(pts.len());
    for &p in pts.iter().rev() {
        while let [.., o, a] = upper.as_slice()
            && cross(*o, *a, p) <= 1e-9
        {
            upper.pop();
        }
        upper.push(p);
    }

    lower.pop();
    upper.pop();
    lower.extend(upper);

    if lower.len() < 3 {
        return Err(GeomError::InvalidShapeParameter("convex hull vertices are collinear"));
    }

    let mut out = Path::new();
    for (idx, (x, y)) in lower.into_iter().enumerate() {
        let pt = Point::new(x, y)?;
        if idx == 0 {
            out.move_to(pt)?;
        } else {
            out.line_to(pt)?;
        }
    }
    out.close()?;
    Ok(out)
}

/// Computes an internal medial-chord / centerline skeleton [`Path`] of `path` by sampling
/// `sample_steps` (`8..=128`) boundary points along its arc length and pairing each sample
/// with its closest non-local opposing boundary sample (`Object -> Shaping -> Centerline Skeleton`).
pub fn medial_chord_skeleton_of_path(path: &Path, sample_steps: u32) -> Result<Path, GeomError> {
    let lut = ArcLengthTable::from_path(path)?;
    let total_len = lut.total_length();
    if total_len <= 1e-6 {
        return Err(GeomError::InvalidShapeParameter("cannot skeletonize zero-length path"));
    }
    let steps = sample_steps.clamp(8, 128) as usize;
    let mut samples = Vec::with_capacity(steps);
    for i in 0..steps {
        let u = (i as f64) / (steps as f64);
        if let Some(s) = lut.sample_normalized(u) {
            samples.push(s);
        }
    }
    let n = samples.len();
    if n < 8 {
        return Err(GeomError::InvalidShapeParameter("insufficient boundary samples for skeleton"));
    }
    let min_idx_gap = (n / 6).max(2);
    let mut midpoints = Vec::new();
    for (i, si) in samples.iter().enumerate() {
        let pi = si.point;
        let ti = si.tangent;
        let mut best_dist = f64::INFINITY;
        let mut best_mid = None;
        for (j, sj) in samples.iter().enumerate() {
            let diff = i.abs_diff(j);
            let circ_gap = diff.min(n.saturating_sub(diff));
            if circ_gap < min_idx_gap {
                continue;
            }
            let pj = sj.point;
            let dx = pj.x.get() - pi.x.get();
            let dy = pj.y.get() - pi.y.get();
            let dist = dx.hypot(dy);
            if dist <= 1e-6 {
                continue;
            }
            // Prefer opposing chords roughly orthogonal to the local tangent
            let ortho_penalty = 1.0 + ((dx * ti.0 + dy * ti.1) / dist).abs();
            let score = dist * ortho_penalty;
            if score < best_dist {
                best_dist = score;
                if let Ok(m) = Point::new(0.5 * (pi.x.get() + pj.x.get()), 0.5 * (pi.y.get() + pj.y.get())) {
                    best_mid = Some(m);
                }
            }
        }
        if let Some(m) = best_mid {
            midpoints.push(m);
        }
    }
    if midpoints.len() < 2 {
        return Err(GeomError::InvalidShapeParameter("skeleton produced fewer than 2 medial points"));
    }
    let max_jump = (total_len / (n as f64)) * 4.0;
    let mut out = Path::new();
    let mut prev: Option<Point> = None;
    for m in midpoints {
        if let Some(p) = prev {
            let d = (m.x.get() - p.x.get()).hypot(m.y.get() - p.y.get());
            if d <= 1e-4 {
                continue;
            }
            if d <= max_jump {
                out.line_to(m)?;
            } else {
                out.move_to(m)?;
            }
        } else {
            out.move_to(m)?;
        }
        prev = Some(m);
    }
    if out.elements.len() < 2 {
        out.line_to(prev.unwrap_or(Point::ZERO))?;
    }
    Ok(out)
}

/// Densifies `path` by flattening curves and subdividing every segment longer than `max_len_pt`
/// into uniform sub-segments (`Shape Tool -> Densify / Subdivide by Max Length`).
pub fn subdivide_path_by_max_segment_length(path: &Path, max_len_pt: f64) -> Result<Path, GeomError> {
    let max_len = crate::f64_canon::F64::new(max_len_pt)?.get();
    if path.elements.is_empty() || max_len <= 0.1 {
        return Err(GeomError::InvalidShapeParameter("invalid max_len_pt for path densification"));
    }
    let polys = crate::flatten::flatten_path(path, &crate::flatten::FlattenOpts::default())?;
    let mut out = Path::new();
    out.fill_rule = path.fill_rule;
    for poly in polys {
        let Some(&first_pt) = poly.points.first() else {
            continue;
        };
        if poly.points.len() < 2 {
            continue;
        }
        out.move_to(first_pt)?;
        for seg in poly.points.windows(2) {
            let [p0, p1] = *seg else { continue };
            let dx = p1.x.get() - p0.x.get();
            let dy = p1.y.get() - p0.y.get();
            let dist = dx.hypot(dy);
            let n = ((dist / max_len).ceil() as usize).clamp(1, 128);
            for k in 1..=n {
                let t = (k as f64) / (n as f64);
                let pt = Point::new(p0.x.get() + dx * t, p0.y.get() + dy * t)?;
                out.line_to(pt)?;
            }
        }
        if poly.closed {
            out.close()?;
        }
    }
    if out.elements.is_empty() {
        return Err(GeomError::InvalidShapeParameter("densification produced empty path"));
    }
    Ok(out)
}

/// Applies a CorelDRAW Fish-Eye / Spherical Lens geometric warp to `path` within `radius_pt`
/// of `center` at `magnification` (`0.2..=5.0`) (`Effects -> Lens -> Fish Eye Warp`).
pub fn spherical_fisheye_warp_path(path: &Path, center: Point, radius_pt: f64, magnification: f64) -> Result<Path, GeomError> {
    let r_lens = crate::f64_canon::F64::new(radius_pt)?.get();
    let mag = crate::f64_canon::F64::new(magnification)?.get();
    if path.elements.is_empty() || r_lens <= 1.0 || !(0.2..=5.0).contains(&mag) {
        return Err(GeomError::InvalidShapeParameter("invalid spherical fisheye warp parameters"));
    }
    let dense = subdivide_path_by_max_segment_length(path, (r_lens * 0.15).max(2.0))?;
    let cx = center.x.get();
    let cy = center.y.get();
    let exp = 1.0 / mag;
    let warp_pt = |p: Point| -> Result<Point, GeomError> {
        let dx = p.x.get() - cx;
        let dy = p.y.get() - cy;
        let d = dx.hypot(dy);
        if d <= 1e-9 || d >= r_lens {
            return Ok(p);
        }
        let u = (d / r_lens).clamp(0.0, 1.0);
        let d_new = r_lens * u.powf(exp);
        let scale = d_new / d;
        Point::new(cx + dx * scale, cy + dy * scale)
    };
    let mut out_elems = Vec::with_capacity(dense.elements.len());
    for el in dense.elements {
        match el {
            PathElement::MoveTo(p) => out_elems.push(PathElement::MoveTo(warp_pt(p)?)),
            PathElement::LineTo(p) => out_elems.push(PathElement::LineTo(warp_pt(p)?)),
            PathElement::QuadTo { ctrl, end } => out_elems.push(PathElement::QuadTo { ctrl: warp_pt(ctrl)?, end: warp_pt(end)? }),
            PathElement::CubicTo { c1, c2, end } => {
                out_elems.push(PathElement::CubicTo { c1: warp_pt(c1)?, c2: warp_pt(c2)?, end: warp_pt(end)? });
            }
            PathElement::Close => out_elems.push(PathElement::Close),
        }
    }
    Path::from_elements(out_elems, path.fill_rule)
}

/// Applies global shrinkage-compensated Taubin ($\lambda / \mu$) Laplacian vertex relaxation
/// across all subpaths of `path` for `iterations` (`1..=16`) with factor `lambda` (`0.05..=0.8`)
/// (`Shape Tool F10 -> Global Curve Relaxation`).
pub fn laplacian_relax_closed_path(path: &Path, iterations: u32, lambda: f64) -> Result<Path, GeomError> {
    let lam = crate::f64_canon::F64::new(lambda)?.get();
    if path.elements.is_empty() || iterations == 0 || !(0.05..=0.8).contains(&lam) {
        return Err(GeomError::InvalidShapeParameter("invalid Laplacian relaxation parameters"));
    }
    let iters = iterations.clamp(1, 16);
    // Shrinkage-compensating negative pass factor (Taubin 1995: 1/lambda + 1/mu = 0.1)
    let mu = -lam / (1.0 - 0.1 * lam).max(0.2);
    let polys = crate::flatten::flatten_path(path, &crate::flatten::FlattenOpts::default())?;
    let mut out = Path::new();
    out.fill_rule = path.fill_rule;

    for poly in polys {
        let mut pts = poly.points;
        if pts.len() >= 2 && poly.closed && pts.first().zip(pts.last()).is_some_and(|(a, b)| a == b) {
            pts.pop();
        }
        let n = pts.len();
        if n < 3 {
            if let Some((&first, rest)) = pts.split_first() {
                out.move_to(first)?;
                for &p in rest {
                    out.line_to(p)?;
                }
                if poly.closed {
                    out.close()?;
                }
            }
            continue;
        }
        for _ in 0..iters {
            for factor in [lam, mu] {
                let prev_pts = pts.clone();
                for (i, slot) in pts.iter_mut().enumerate() {
                    if !poly.closed && (i == 0 || i.saturating_add(1) == n) {
                        continue;
                    }
                    let prev_idx = i.saturating_add(n).saturating_sub(1).checked_rem(n).unwrap_or(0);
                    let next_idx = i.saturating_add(1).checked_rem(n).unwrap_or(0);
                    if let (Some(&p_prev), Some(&p_curr), Some(&p_next)) = (prev_pts.get(prev_idx), prev_pts.get(i), prev_pts.get(next_idx)) {
                        let lx = 0.5 * (p_prev.x.get() + p_next.x.get()) - p_curr.x.get();
                        let ly = 0.5 * (p_prev.y.get() + p_next.y.get()) - p_curr.y.get();
                        *slot = Point::new(p_curr.x.get() + factor * lx, p_curr.y.get() + factor * ly)?;
                    }
                }
            }
        }
        if let Some((&first, rest)) = pts.split_first() {
            out.move_to(first)?;
            for &p in rest {
                out.line_to(p)?;
            }
        }
        if poly.closed {
            out.close()?;
        }
    }
    if out.elements.is_empty() {
        return Err(GeomError::InvalidShapeParameter("relaxation produced empty path"));
    }
    Ok(out)
}

/// Fits a least-squares Kåsa algebraic circle to the flattened vertices of `path`
/// and returns a closed 4-arc cubic Bézier circle [`Path`] (`Object -> Shaping -> Fit Circle to Curve`).
pub fn fit_circle_to_path_points(path: &Path) -> Result<Path, GeomError> {
    let polys = crate::flatten::flatten_path(path, &crate::flatten::FlattenOpts::default())?;
    let mut pts: Vec<(f64, f64)> = Vec::new();
    for poly in polys {
        let mut sub = poly.points;
        if poly.closed && sub.len() >= 2 && sub.first().zip(sub.last()).is_some_and(|(a, b)| a == b) {
            sub.pop();
        }
        for p in sub {
            pts.push((p.x.get(), p.y.get()));
        }
    }
    if pts.len() < 3 {
        return Err(GeomError::InvalidShapeParameter("circle fit requires at least 3 points"));
    }
    let n = pts.len() as f64;
    let mean_x = pts.iter().map(|p| p.0).sum::<f64>() / n;
    let mean_y = pts.iter().map(|p| p.1).sum::<f64>() / n;

    let mut suu = 0.0;
    let mut suv = 0.0;
    let mut svv = 0.0;
    let mut suz = 0.0;
    let mut svz = 0.0;
    for &(x, y) in &pts {
        let u = x - mean_x;
        let v = y - mean_y;
        let z = u * u + v * v;
        suu += u * u;
        suv += u * v;
        svv += v * v;
        suz += u * z;
        svz += v * z;
    }
    let det = suu * svv - suv * suv;
    if det.abs() <= 1e-9 {
        return Err(GeomError::InvalidShapeParameter("points are collinear; cannot fit circle"));
    }
    let rhs_u = 0.5 * suz;
    let rhs_v = 0.5 * svz;
    let uc = (rhs_u * svv - rhs_v * suv) / det;
    let vc = (suu * rhs_v - suv * rhs_u) / det;
    let cx = mean_x + uc;
    let cy = mean_y + vc;
    let r = (uc * uc + vc * vc + (suu + svv) / n).sqrt();
    if !r.is_finite() || r <= 1e-4 {
        return Err(GeomError::InvalidShapeParameter("fitted circle radius is degenerate"));
    }
    let rect = Rect::new(cx - r, cy - r, cx + r, cy + r)?;
    crate::shapes::EllipseSpec::full(rect).to_path()
}

/// Converts each subpath of `path` into a $C^1$-continuous Catmull-Rom cubic Bézier curve
/// with `tension` (`0.1..=2.0`, default `1.0`) (`Shape Tool F10 -> Convert Polyline to Smooth Cubic Curves`).
pub fn interpolate_smooth_cubic_path(path: &Path, tension: f64) -> Result<Path, GeomError> {
    let tau = crate::f64_canon::F64::new(tension)?.get();
    if path.elements.is_empty() || !(0.1..=2.0).contains(&tau) {
        return Err(GeomError::InvalidShapeParameter("invalid Catmull-Rom cubic interpolation parameters"));
    }
    let s = tau / 6.0;
    let polys = crate::flatten::flatten_path(path, &crate::flatten::FlattenOpts::default())?;
    let mut out = Path::new();
    out.fill_rule = path.fill_rule;

    for poly in polys {
        let mut pts = poly.points;
        if poly.closed && pts.len() >= 2 && pts.first().zip(pts.last()).is_some_and(|(a, b)| a == b) {
            pts.pop();
        }
        let n = pts.len();
        if n < 2 {
            continue;
        }
        let Some(&first) = pts.first() else {
            continue;
        };
        out.move_to(first)?;
        if poly.closed && n >= 3 {
            for i in 0..n {
                let idx0 = i.saturating_add(n).saturating_sub(1).checked_rem(n).unwrap_or(0);
                let idx2 = i.saturating_add(1).checked_rem(n).unwrap_or(0);
                let idx3 = i.saturating_add(2).checked_rem(n).unwrap_or(0);
                if let (Some(&p0), Some(&p1), Some(&p2), Some(&p3)) = (pts.get(idx0), pts.get(i), pts.get(idx2), pts.get(idx3)) {
                    let c1 = Point::new(p1.x.get() + (p2.x.get() - p0.x.get()) * s, p1.y.get() + (p2.y.get() - p0.y.get()) * s)?;
                    let c2 = Point::new(p2.x.get() - (p3.x.get() - p1.x.get()) * s, p2.y.get() - (p3.y.get() - p1.y.get()) * s)?;
                    out.cubic_to(c1, c2, p2)?;
                }
            }
            out.close()?;
        } else {
            let Some(&last_pt) = pts.last() else {
                continue;
            };
            for i in 0..n.saturating_sub(1) {
                let p0 = if i == 0 { first } else { pts.get(i.saturating_sub(1)).copied().unwrap_or(first) };
                let Some(&p1) = pts.get(i) else { continue };
                let Some(&p2) = pts.get(i.saturating_add(1)) else { continue };
                let p3 = pts.get(i.saturating_add(2)).copied().unwrap_or(last_pt);
                let c1 = Point::new(p1.x.get() + (p2.x.get() - p0.x.get()) * s, p1.y.get() + (p2.y.get() - p0.y.get()) * s)?;
                let c2 = Point::new(p2.x.get() - (p3.x.get() - p1.x.get()) * s, p2.y.get() - (p3.y.get() - p1.y.get()) * s)?;
                out.cubic_to(c1, c2, p2)?;
            }
        }
    }
    if out.elements.is_empty() {
        return Err(GeomError::InvalidShapeParameter("Catmull-Rom interpolation produced empty path"));
    }
    Ok(out)
}

/// Applies a radial Pinch (`strength < 0.0`) or Punch (`strength > 0.0`, `strength` in `-0.95..=2.0`)
/// warp to `path` within `radius_pt` around `center` (`Effects -> Distort -> Pinch / Punch Warp`).
pub fn pinch_punch_warp_path(path: &Path, center: Point, radius_pt: f64, strength: f64) -> Result<Path, GeomError> {
    let r_max = crate::f64_canon::F64::new(radius_pt)?.get();
    let s = crate::f64_canon::F64::new(strength)?.get();
    if path.elements.is_empty() || r_max <= 1.0 || !(-0.95..=2.0).contains(&s) {
        return Err(GeomError::InvalidShapeParameter("invalid pinch/punch warp parameters"));
    }
    let dense = subdivide_path_by_max_segment_length(path, (r_max * 0.15).max(2.0))?;
    let cx = center.x.get();
    let cy = center.y.get();
    let warp_pt = |p: Point| -> Result<Point, GeomError> {
        let dx = p.x.get() - cx;
        let dy = p.y.get() - cy;
        let d = dx.hypot(dy);
        if d <= 1e-9 || d >= r_max {
            return Ok(p);
        }
        let falloff = 1.0 - (d / r_max);
        let factor = (1.0 + s * falloff).max(0.05);
        Point::new(cx + dx * factor, cy + dy * factor)
    };
    let mut out_elems = Vec::with_capacity(dense.elements.len());
    for el in dense.elements {
        match el {
            PathElement::MoveTo(p) => out_elems.push(PathElement::MoveTo(warp_pt(p)?)),
            PathElement::LineTo(p) => out_elems.push(PathElement::LineTo(warp_pt(p)?)),
            PathElement::QuadTo { ctrl, end } => out_elems.push(PathElement::QuadTo { ctrl: warp_pt(ctrl)?, end: warp_pt(end)? }),
            PathElement::CubicTo { c1, c2, end } => {
                out_elems.push(PathElement::CubicTo { c1: warp_pt(c1)?, c2: warp_pt(c2)?, end: warp_pt(end)? });
            }
            PathElement::Close => out_elems.push(PathElement::Close),
        }
    }
    Path::from_elements(out_elems, path.fill_rule)
}

/// Flattens all quadratic and cubic Bézier curves in `path` into linear polyline subpaths
/// within flatness `tolerance_pt` (`0.05..=50.0`) (`Shape Tool F10 -> Polygonize / Convert Curves to Lines`).
pub fn flatten_path_to_polylines(path: &Path, tolerance_pt: f64) -> Result<Path, GeomError> {
    let tol = crate::f64_canon::F64::new(tolerance_pt)?.get();
    if path.elements.is_empty() || !(0.05..=50.0).contains(&tol) {
        return Err(GeomError::InvalidShapeParameter("invalid flatness tolerance_pt for path polygonization"));
    }
    let opts = crate::flatten::FlattenOpts { tolerance: tol, max_depth: crate::flatten::DEFAULT_MAX_FLATTEN_DEPTH };
    let polys = crate::flatten::flatten_path(path, &opts)?;
    let mut out = Path::new();
    out.fill_rule = path.fill_rule;
    for poly in polys {
        let mut pts = poly.points;
        if poly.closed && pts.len() >= 2 && pts.first().zip(pts.last()).is_some_and(|(a, b)| a == b) {
            pts.pop();
        }
        if pts.len() < 2 {
            continue;
        }
        if let Some((&first, rest)) = pts.split_first() {
            out.move_to(first)?;
            for &p in rest {
                out.line_to(p)?;
            }
        }
        if poly.closed {
            out.close()?;
        }
    }
    if out.elements.is_empty() {
        return Err(GeomError::InvalidShapeParameter("polygonization produced empty path"));
    }
    Ok(out)
}

/// Applies a center-anchored linear shear warp (`shear_x_pt`, `shear_y_pt`) to `path`
/// normalized across `bounds` (`Effects -> Envelope -> Linear Shear Warp`).
pub fn shear_warp_path_along_axis(path: &Path, bounds: Rect, shear_x_pt: f64, shear_y_pt: f64) -> Result<Path, GeomError> {
    let sx = crate::f64_canon::F64::new(shear_x_pt)?.get();
    let sy = crate::f64_canon::F64::new(shear_y_pt)?.get();
    let w = bounds.width();
    let h = bounds.height();
    if path.elements.is_empty() || w <= 1e-6 || h <= 1e-6 {
        return Err(GeomError::InvalidShapeParameter("invalid linear shear warp parameters"));
    }
    let center = bounds.center()?;
    let cx = center.x.get();
    let cy = center.y.get();
    let half_w = 0.5 * w;
    let half_h = 0.5 * h;
    let warp_pt = |p: Point| -> Result<Point, GeomError> {
        let u = (p.x.get() - cx) / half_w;
        let v = (p.y.get() - cy) / half_h;
        Point::new(p.x.get() + sx * v, p.y.get() + sy * u)
    };
    let mut out_elems = Vec::with_capacity(path.elements.len());
    for &el in &path.elements {
        match el {
            PathElement::MoveTo(p) => out_elems.push(PathElement::MoveTo(warp_pt(p)?)),
            PathElement::LineTo(p) => out_elems.push(PathElement::LineTo(warp_pt(p)?)),
            PathElement::QuadTo { ctrl, end } => out_elems.push(PathElement::QuadTo { ctrl: warp_pt(ctrl)?, end: warp_pt(end)? }),
            PathElement::CubicTo { c1, c2, end } => {
                out_elems.push(PathElement::CubicTo { c1: warp_pt(c1)?, c2: warp_pt(c2)?, end: warp_pt(end)? });
            }
            PathElement::Close => out_elems.push(PathElement::Close),
        }
    }
    Path::from_elements(out_elems, path.fill_rule)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::arithmetic_side_effects, clippy::panic)]
mod m26_tests {
    use super::*;

    #[test]
    fn convert_segment_to_curve_and_back_to_line() {
        let p0 = Point::new(0.0, 0.0).unwrap();
        let p1 = Point::new(90.0, 60.0).unwrap();
        let path = Path::from_elements(vec![PathElement::MoveTo(p0), PathElement::LineTo(p1)], FillRule::NonZero).unwrap();
        let curved = convert_segment_to_curve(&path, 1).unwrap();
        match curved.elements[1] {
            PathElement::CubicTo { c1, c2, end } => {
                assert!((c1.x.get() - 30.0).abs() < 1e-6);
                assert!((c2.x.get() - 60.0).abs() < 1e-6);
                assert_eq!(end, p1);
            }
            _ => unreachable!(),
        }
        let linear = convert_segment_to_line(&curved, 1).unwrap();
        assert_eq!(linear.elements[1], PathElement::LineTo(p1));

        let twirled = twirl_path_around_point(&curved, Point::new(45.0, 30.0).unwrap(), 60.0, 90.0).unwrap();
        assert_eq!(twirled.elements.len(), 2);
        let attracted = radial_push_path_around_point(&curved, Point::new(45.0, 30.0).unwrap(), 60.0, 0.5).unwrap();
        assert_eq!(attracted.elements.len(), 2);
        let smeared = smear_path_from_point(&curved, Point::new(45.0, 30.0).unwrap(), 60.0, 15.0, -10.0).unwrap();
        assert_eq!(smeared.elements.len(), 2);
        let roughened = roughen_path_segments(&linear, 3.0, 4).unwrap();
        assert!(roughened.elements.len() > 2);
        let smoothed = smooth_path_around_point(&roughened, Point::new(45.0, 30.0).unwrap(), 60.0, 0.6, 3).unwrap();
        assert_eq!(smoothed.elements.len(), roughened.elements.len());
        let vsd = delete_segment_at_index(&roughened, 1).unwrap();
        assert!(matches!(vsd.elements[1], PathElement::MoveTo(_)));
        let offset = offset_open_or_closed_path(&linear, 10.0).unwrap();
        assert_eq!(offset.elements.len(), 2);

        let rect_path = crate::shapes::RectSpec::sharp(crate::Rect::new(0.0, 0.0, 50.0, 40.0).unwrap()).to_path().unwrap();
        let (perim, area) = measure_path_length_and_area(&rect_path).unwrap();
        assert!((perim - 180.0).abs() < 1e-6);
        assert!((area - 2000.0).abs() < 1e-6);

        let poly = Path::from_elements(
            vec![
                PathElement::MoveTo(Point::new(0.0, 0.0).unwrap()),
                PathElement::LineTo(Point::new(50.0, 40.0).unwrap()),
                PathElement::LineTo(Point::new(100.0, 0.0).unwrap()),
            ],
            FillRule::NonZero,
        )
        .unwrap();
        let smoothed_node = smooth_curve_node_at(&poly, 1, 0.35).unwrap();
        assert!(matches!(smoothed_node.elements[1], PathElement::CubicTo { .. }));
        assert!(matches!(smoothed_node.elements[2], PathElement::CubicTo { .. }));

        let broken_open = break_path_at_node(&poly, 1).unwrap();
        let move_count = broken_open.elements.iter().filter(|e| matches!(e, PathElement::MoveTo(_))).count();
        assert_eq!(move_count, 2);

        let broken_closed = break_path_at_node(&rect_path, 2).unwrap();
        assert!(!broken_closed.elements.iter().any(|e| matches!(e, PathElement::Close)));

        let morphed = morph_paths_linear(&rect_path, &poly, 0.5, 16).unwrap();
        assert_eq!(morphed.elements.len(), 16);

        let rect2 = crate::shapes::RectSpec::sharp(crate::Rect::new(25.0, 15.0, 75.0, 55.0).unwrap()).to_path().unwrap();
        let divided = divide_overlapping_paths(&rect_path, &rect2).unwrap();
        assert_eq!(divided.len(), 3);

        let hatched = hatch_path_region(&rect_path, 45.0, 8.0, true).unwrap();
        assert!(hatched.elements.len() >= 10);

        for plane in [AxonometricPlane::IsometricTop, AxonometricPlane::IsometricLeft, AxonometricPlane::IsometricRight] {
            let proj = project_path_axonometric(&rect_path, plane).unwrap();
            assert_eq!(proj.elements.len(), rect_path.elements.len());
        }

        let tapered = expand_tapered_stroke_profile(&poly, &[2.0, 12.0, 4.0]).unwrap();
        assert!(tapered.elements.iter().any(|e| matches!(e, PathElement::Close)));

        let chaikin = chaikin_smooth_path(&rect_path, 2).unwrap();
        assert!(chaikin.elements.len() > rect_path.elements.len());

        let waved = wave_distort_path(&rect_path, 4.0, 3.0, 0.0).unwrap();
        assert!(waved.elements.len() > rect_path.elements.len());

        let sym = build_dihedral_symmetry_path(&rect_path, Point::new(50.0, 50.0).unwrap(), 4, true).unwrap();
        assert_eq!(sym.elements.len(), rect_path.elements.len() * 8);

        let dashed = dash_path_to_open_subpaths(&rect_path, &[10.0, 5.0]).unwrap();
        let dash_moves = dashed.elements.iter().filter(|e| matches!(e, PathElement::MoveTo(_))).count();
        assert!(dash_moves >= 8);

        let hull = convex_hull_of_path(&sym).unwrap();
        assert!(hull.elements.iter().any(|e| matches!(e, PathElement::Close)));

        let skel = medial_chord_skeleton_of_path(&rect_path, 24).unwrap();
        assert!(skel.elements.len() >= 2);

        let dense = subdivide_path_by_max_segment_length(&rect_path, 10.0).unwrap();
        assert!(dense.elements.len() >= 18);

        let fisheye = spherical_fisheye_warp_path(&rect_path, Point::new(25.0, 20.0).unwrap(), 40.0, 1.8).unwrap();
        assert!(fisheye.elements.len() >= 10);

        let relaxed = laplacian_relax_closed_path(&dense, 3, 0.45).unwrap();
        assert!(relaxed.elements.len() >= 18);

        let fitted_circle = fit_circle_to_path_points(&rect_path).unwrap();
        let fb = fitted_circle.bounds().unwrap();
        assert!((fb.center().unwrap().x.get() - 25.0).abs() < 1e-3);
        assert!((fb.center().unwrap().y.get() - 20.0).abs() < 1e-3);

        let smooth_cubic = interpolate_smooth_cubic_path(&rect_path, 1.0).unwrap();
        let cubic_count = smooth_cubic.elements.iter().filter(|e| matches!(e, PathElement::CubicTo { .. })).count();
        assert_eq!(cubic_count, 4);

        let pinched = pinch_punch_warp_path(&rect_path, Point::new(25.0, 20.0).unwrap(), 40.0, -0.5).unwrap();
        assert!(pinched.elements.len() >= 10);

        let poly_flat = flatten_path_to_polylines(&fitted_circle, 0.5).unwrap();
        assert!(!poly_flat.elements.iter().any(|e| matches!(e, PathElement::CubicTo { .. })));

        let sheared = shear_warp_path_along_axis(&rect_path, crate::Rect::new(0.0, 0.0, 50.0, 40.0).unwrap(), 10.0, 5.0).unwrap();
        assert_eq!(sheared.elements.len(), rect_path.elements.len());

        let miter_rect = offset_path_with_join(&rect_path, 10.0, OffsetJoinKind::Miter).unwrap();
        let bevel_rect = offset_path_with_join(&rect_path, 10.0, OffsetJoinKind::Bevel).unwrap();
        let round_rect = offset_path_with_join(&rect_path, 10.0, OffsetJoinKind::Round).unwrap();
        assert_eq!(miter_rect.elements.len(), 5);
        assert_eq!(bevel_rect.elements.len(), 9);
        assert!(round_rect.elements.len() > bevel_rect.elements.len());
        let (_, bevel_area) = measure_path_length_and_area(&bevel_rect).unwrap();
        let (_, round_area) = measure_path_length_and_area(&round_rect).unwrap();
        let (_, miter_area) = measure_path_length_and_area(&miter_rect).unwrap();
        assert!(bevel_area < round_area && round_area < miter_area);
    }
}
