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
            if poly.points.is_empty() {
                continue;
            }
            let mut prev = poly.points[0];
            if samples.is_empty() {
                let first_tan = if poly.points.len() >= 2 {
                    let dx = poly.points[1].x.get() - prev.x.get();
                    let dy = poly.points[1].y.get() - prev.y.get();
                    let d = dx.hypot(dy);
                    if d > 1e-9 { (dx / d, dy / d) } else { (1.0, 0.0) }
                } else {
                    (1.0, 0.0)
                };
                samples.push(PathSample { point: prev, tangent: first_tan, angle_deg: first_tan.1.atan2(first_tan.0).to_degrees(), s_pt: 0.0 });
            }
            for &pt in &poly.points[1..] {
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
            if poly.closed && poly.points.len() >= 2 {
                let first = poly.points[0];
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
        let s0 = self.samples[idx - 1];
        let s1 = self.samples[idx];
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
            if let Some(&first) = pts.first() {
                out.move_to(first)?;
                for &p in &pts[1..] {
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
            if !poly.closed && (i == 0 || i == m - 1) {
                if !started {
                    out.move_to(pts[i])?;
                    started = true;
                } else {
                    out.line_to(pts[i])?;
                }
                continue;
            }
            let prev = pts[(i + m - 1) % m];
            let curr = pts[i];
            let next = pts[(i + 1) % m];
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
    let mut left_pts = Vec::with_capacity(steps + 1);
    let mut right_pts = Vec::with_capacity(steps + 1);

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
    if let Some(&first) = left_pts.first() {
        out.move_to(first)?;
        for &pt in &left_pts[1..] {
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
        let mut dense_pts = Vec::with_capacity(poly.points.len() * 4);
        for seg in poly.points.windows(2) {
            let p0 = seg[0];
            let p1 = seg[1];
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
            if idx > 0 {
                cum_len += dense_pts[idx - 1].distance(pt);
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
                    let prev = if idx > 0 { dense_pts[idx - 1] } else { pt };
                    let next = if idx + 1 < dense_pts.len() { dense_pts[idx + 1] } else { pt };
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

    let mut path = Path { elements: Vec::new(), fill_rule: FillRule::NonZero };
    for y in 0..pixel_height {
        let mut x = 0u32;
        while x < pixel_width {
            let idx = ((y * pixel_width + x) as usize) * 4;
            let r = u16::from(rgba_pixels[idx]);
            let g = u16::from(rgba_pixels[idx + 1]);
            let b = u16::from(rgba_pixels[idx + 2]);
            let a = rgba_pixels[idx + 3];
            let lum = ((54 * r + 183 * g + 19 * b) >> 8) as u8;
            if a >= 128 && lum <= threshold {
                let start_x = x;
                x += 1;
                while x < pixel_width {
                    let idx2 = ((y * pixel_width + x) as usize) * 4;
                    let r2 = u16::from(rgba_pixels[idx2]);
                    let g2 = u16::from(rgba_pixels[idx2 + 1]);
                    let b2 = u16::from(rgba_pixels[idx2 + 2]);
                    let a2 = rgba_pixels[idx2 + 3];
                    let lum2 = ((54 * r2 + 183 * g2 + 19 * b2) >> 8) as u8;
                    if a2 >= 128 && lum2 <= threshold {
                        x += 1;
                    } else {
                        break;
                    }
                }
                let x0 = f64::from(start_x) * scale_x;
                let x1 = f64::from(x) * scale_x;
                let y0 = f64::from(y) * scale_y;
                let y1 = f64::from(y + 1) * scale_y;
                if path.elements.len() + 5 <= MAX_PATH_ELEMENTS {
                    path.move_to(Point::new(x0, y0)?)?;
                    path.line_to(Point::new(x1, y0)?)?;
                    path.line_to(Point::new(x1, y1)?)?;
                    path.line_to(Point::new(x0, y1)?)?;
                    path.close()?;
                }
            } else {
                x += 1;
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
                match segs[idx - 1] {
                    Seg::Line(p) | Seg::Quad(_, p) | Seg::Cubic(_, _, p) => p,
                }
            };
            match segs[idx] {
                Seg::Line(_) => out.line_to(prev_pt)?,
                Seg::Quad(ctrl, _) => out.quad_to(ctrl, prev_pt)?,
                Seg::Cubic(c1, c2, _) => out.cubic_to(c2, c1, prev_pt)?,
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
    let mut out = Path { elements: Vec::with_capacity(path.elements.len() + 4), fill_rule: path.fill_rule };
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
        let Some(&PathElement::MoveTo(start)) = sub.first() else {
            return None;
        };
        let mut end = start;
        let mut closed = false;
        for &el in &sub[1..] {
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
                    prev.extend_from_slice(&sub[1..]);
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
                        prev.extend_from_slice(&tmp.elements[1..]);
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
        if pts.is_empty() {
            continue;
        }
        if pts.len() <= 2 || tolerance_pt <= 1e-9 {
            out.move_to(pts[0])?;
            for &p in &pts[1..] {
                out.line_to(p)?;
            }
            if poly.closed {
                out.close()?;
            }
            continue;
        }

        let mut keep = vec![false; pts.len()];
        keep[0] = true;
        keep[pts.len() - 1] = true;
        if poly.closed && pts.len() >= 4 {
            keep[pts.len() / 2] = true;
        }

        let mut stack = Vec::new();
        if poly.closed && pts.len() >= 4 {
            stack.push((0usize, pts.len() / 2));
            stack.push((pts.len() / 2, pts.len() - 1));
        } else {
            stack.push((0usize, pts.len() - 1));
        }

        while let Some((start_idx, end_idx)) = stack.pop() {
            if end_idx <= start_idx + 1 {
                continue;
            }
            let a = pts[start_idx];
            let b = pts[end_idx];
            let dx = b.x.get() - a.x.get();
            let dy = b.y.get() - a.y.get();
            let len_sq = dx * dx + dy * dy;

            let mut max_dist = 0.0;
            let mut max_idx = start_idx;
            for (idx, p) in pts.iter().enumerate().take(end_idx).skip(start_idx + 1) {
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
                keep[max_idx] = true;
                stack.push((start_idx, max_idx));
                stack.push((max_idx, end_idx));
            }
        }

        let mut first = true;
        for (i, &p) in pts.iter().enumerate() {
            if keep[i] {
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
    for &el in &path.elements[..element_index] {
        match el {
            PathElement::MoveTo(p) | PathElement::LineTo(p) | PathElement::QuadTo { end: p, .. } | PathElement::CubicTo { end: p, .. } => {
                prev_pt = p;
            }
            PathElement::Close => {}
        }
    }

    let mut out = Path { elements: Vec::with_capacity(path.elements.len() + 1), fill_rule: path.fill_rule };
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
    if matches!(path.elements[element_index], PathElement::MoveTo(_) | PathElement::Close) {
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
    if element_index == 0 || element_index >= path.elements.len() {
        return Err(GeomError::InvalidShapeParameter("element_index out of bounds for convert_segment_to_line"));
    }
    let target = match path.elements[element_index] {
        PathElement::LineTo(end) | PathElement::QuadTo { end, .. } | PathElement::CubicTo { end, .. } => PathElement::LineTo(end),
        PathElement::MoveTo(_) | PathElement::Close => {
            return Err(GeomError::InvalidShapeParameter("cannot convert MoveTo or Close to line"));
        }
    };
    let mut elements = path.elements.clone();
    elements[element_index] = target;
    Path::from_elements(elements, path.fill_rule)
}

/// Converts the segment at `element_index` into a `CubicTo` Bézier segment with collinear `1/3` and `2/3` handles (`Shape Tool -> To Curve`).
pub fn convert_segment_to_curve(path: &Path, element_index: usize) -> Result<Path, GeomError> {
    if element_index == 0 || element_index >= path.elements.len() {
        return Err(GeomError::InvalidShapeParameter("element_index out of bounds for convert_segment_to_curve"));
    }
    let mut start = Point::ZERO;
    let mut has_start = false;
    for &el in &path.elements[..element_index] {
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
    let target = match path.elements[element_index] {
        PathElement::LineTo(end) => {
            let c1 = lerp(start, end, 1.0 / 3.0)?;
            let c2 = lerp(start, end, 2.0 / 3.0)?;
            PathElement::CubicTo { c1, c2, end }
        }
        PathElement::QuadTo { ctrl, end } => {
            let c1 = lerp(start, ctrl, 2.0 / 3.0)?;
            let c2 = lerp(end, ctrl, 2.0 / 3.0)?;
            PathElement::CubicTo { c1, c2, end }
        }
        PathElement::CubicTo { c1, c2, end } => PathElement::CubicTo { c1, c2, end },
        PathElement::MoveTo(_) | PathElement::Close => {
            return Err(GeomError::InvalidShapeParameter("cannot convert MoveTo or Close to curve"));
        }
    };
    let mut elements = path.elements.clone();
    elements[element_index] = target;
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
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
    }
}
