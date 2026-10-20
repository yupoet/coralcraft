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
    /// Symmetric difference (XOR): retains non-overlapping areas $(A \setminus B) \cup (B \setminus A)$ with `FillRule::EvenOdd`.
    Exclude,
}

/// Executes a 2D boolean operation between a subject path and a clip path.
///
/// Both paths are converted to closed polygon loops. If inputs are disjoint:
/// - [`BooleanOp::Weld`] returns a compound path containing both disjoint contours.
/// - [`BooleanOp::Trim`] or [`BooleanOp::FrontMinusBack`] returns the subject untouched.
/// - [`BooleanOp::Intersect`] returns an empty path.
/// - [`BooleanOp::Exclude`] returns the symmetric difference with [`crate::FillRule::EvenOdd`].
pub fn boolean_paths(subj: &Path, clip: &Path, op: BooleanOp) -> Result<Path, GeomError> {
    if subj.elements.is_empty() {
        return match op {
            BooleanOp::Weld | BooleanOp::Exclude => Ok(clip.clone()),
            _ => Ok(Path::new()),
        };
    }
    if clip.elements.is_empty() {
        return match op {
            BooleanOp::Weld | BooleanOp::Trim | BooleanOp::FrontMinusBack | BooleanOp::Exclude => Ok(subj.clone()),
            _ => Ok(Path::new()),
        };
    }

    let opts = FlattenOpts::default();
    let subj_polys = flatten_path(subj, &opts)?;
    let clip_polys = flatten_path(clip, &opts)?;

    let subj_box = subj.bounds().unwrap_or_default();
    let clip_box = clip.bounds().unwrap_or_default();

    let overlaps = subj_box.intersects(clip_box);

    match op {
        BooleanOp::Weld => {
            if !overlaps {
                let mut rings = extract_normalized_rings(&subj_polys);
                rings.extend(extract_normalized_rings(&clip_polys));
                return rings_to_path(&rings, subj.fill_rule);
            }
            clip_polylines(&subj_polys, &clip_polys, BooleanOp::Weld, subj.fill_rule)
        }
        BooleanOp::Trim | BooleanOp::FrontMinusBack => {
            if !overlaps {
                return Ok(subj.clone());
            }
            clip_polylines(&subj_polys, &clip_polys, BooleanOp::Trim, subj.fill_rule)
        }
        BooleanOp::Intersect => {
            if !overlaps {
                return Ok(Path::new());
            }
            clip_polylines(&subj_polys, &clip_polys, BooleanOp::Intersect, subj.fill_rule)
        }
        BooleanOp::BackMinusFront => {
            if !overlaps {
                return Ok(clip.clone());
            }
            clip_polylines(&subj_polys, &clip_polys, BooleanOp::BackMinusFront, clip.fill_rule)
        }
        BooleanOp::Exclude => {
            if !overlaps {
                let mut rings = extract_normalized_rings(&subj_polys);
                rings.extend(extract_normalized_rings(&clip_polys));
                return rings_to_path(&rings, crate::FillRule::EvenOdd);
            }
            let a_minus_b = clip_polylines(&subj_polys, &clip_polys, BooleanOp::Trim, crate::FillRule::EvenOdd)?;
            let b_minus_a = clip_polylines(&subj_polys, &clip_polys, BooleanOp::BackMinusFront, crate::FillRule::EvenOdd)?;
            let mut out = Path {
                elements: Vec::with_capacity(a_minus_b.elements.len().saturating_add(b_minus_a.elements.len())),
                fill_rule: crate::FillRule::EvenOdd,
            };
            out.elements.extend(a_minus_b.elements);
            out.elements.extend(b_minus_a.elements);
            Ok(out)
        }
    }
}

/// Tests whether `pt` lies inside `path` according to `path.fill_rule` (`EvenOdd` or `NonZero`).
pub fn path_contains_point(path: &Path, pt: Point) -> bool {
    let Ok(polys) = flatten_path(path, &FlattenOpts::default()) else {
        return false;
    };
    match path.fill_rule {
        crate::FillRule::EvenOdd => {
            let mut inside = false;
            for poly in &polys {
                if poly.points.len() >= 3 && point_in_polygon(pt, &poly.points) {
                    inside = !inside;
                }
            }
            inside
        }
        crate::FillRule::NonZero => {
            let mut winding = 0i32;
            for poly in &polys {
                if poly.points.len() >= 3 {
                    winding = winding.saturating_add(polygon_winding_number(pt, &poly.points));
                }
            }
            winding != 0
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
    let mut j = poly.len().saturating_sub(1);
    for i in 0..poly.len() {
        let (Some(&pi), Some(&pj)) = (poly.get(i), poly.get(j)) else {
            continue;
        };
        let xi = pi.x.get();
        let yi = pi.y.get();
        let xj = pj.x.get();
        let yj = pj.y.get();

        let intersect = ((yi > py) != (yj > py)) && (px < (xj - xi) * (py - yi) / (yj - yi) + xi);
        if intersect {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn polygon_winding_number(pt: Point, poly: &[Point]) -> i32 {
    if poly.len() < 3 {
        return 0;
    }
    let px = pt.x.get();
    let py = pt.y.get();
    let mut winding = 0i32;
    let n = poly.len();
    for i in 0..n {
        let next_i = i.saturating_add(1).checked_rem(n).unwrap_or(0);
        let (Some(&p0), Some(&p1)) = (poly.get(i), poly.get(next_i)) else {
            continue;
        };
        let x0 = p0.x.get();
        let y0 = p0.y.get();
        let x1 = p1.x.get();
        let y1 = p1.y.get();
        if y0 <= py {
            if y1 > py && (x1 - x0) * (py - y0) - (px - x0) * (y1 - y0) > 0.0 {
                winding = winding.saturating_add(1);
            }
        } else if y1 <= py && (x1 - x0) * (py - y0) - (px - x0) * (y1 - y0) < 0.0 {
            winding = winding.saturating_sub(1);
        }
    }
    winding
}

fn polygon_signed_area2(poly: &[Point]) -> f64 {
    let n = poly.len();
    if n < 3 {
        return 0.0;
    }
    let mut area2 = 0.0;
    for i in 0..n {
        let next_i = i.saturating_add(1).checked_rem(n).unwrap_or(0);
        if let (Some(&p0), Some(&p1)) = (poly.get(i), poly.get(next_i)) {
            area2 += p0.x.get() * p1.y.get() - p1.x.get() * p0.y.get();
        }
    }
    area2
}

fn extract_normalized_rings(polys: &[Polyline]) -> Vec<Vec<Point>> {
    let mut raw_rings: Vec<Vec<Point>> = Vec::new();
    for poly in polys {
        let mut pts: Vec<Point> = Vec::with_capacity(poly.points.len());
        for &p in &poly.points {
            if pts.last().is_none_or(|last| last.distance(p) > 1e-6) {
                pts.push(p);
            }
        }
        while pts.len() >= 2 && pts.first().zip(pts.last()).is_some_and(|(a, b)| a.distance(*b) <= 1e-6) {
            pts.pop();
        }
        if pts.len() >= 3 && polygon_signed_area2(&pts).abs() > 1e-5 {
            raw_rings.push(pts);
        }
    }

    // Orient outer rings with positive signed area (> 0) and hole rings with negative signed area (< 0)
    let count = raw_rings.len();
    let mut normalized = Vec::with_capacity(count);
    for (i, raw_ring) in raw_rings.iter().enumerate() {
        let Some(&test_pt) = raw_ring.first() else {
            continue;
        };
        let mut depth = 0usize;
        for (j, other) in raw_rings.iter().enumerate() {
            if i != j && point_in_polygon(test_pt, other) {
                depth = depth.saturating_add(1);
            }
        }
        let mut ring = raw_ring.clone();
        let area2 = polygon_signed_area2(&ring);
        let is_hole = depth % 2 == 1;
        if (!is_hole && area2 < 0.0) || (is_hole && area2 > 0.0) {
            ring.reverse();
        }
        normalized.push(ring);
    }
    normalized
}

fn point_in_rings(pt: Point, rings: &[Vec<Point>]) -> bool {
    let mut inside = false;
    for ring in rings {
        if point_in_polygon(pt, ring) {
            inside = !inside;
        }
    }
    inside
}

fn segment_proper_or_touch_intersection(p1: Point, p2: Point, p3: Point, p4: Point) -> Option<Point> {
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

    if (-1e-7..=1.0 + 1e-7).contains(&t) && (-1e-7..=1.0 + 1e-7).contains(&u) {
        let tc = t.clamp(0.0, 1.0);
        let ix = p1.x.get() + tc * d1x;
        let iy = p1.y.get() + tc * d1y;
        Point::new(ix, iy).ok()
    } else {
        None
    }
}

struct VertexPool {
    pts: Vec<Point>,
}

impl VertexPool {
    fn new() -> Self {
        Self { pts: Vec::new() }
    }

    fn intern(&mut self, pt: Point) -> usize {
        const SNAP_EPS: f64 = 1e-5;
        for (idx, &existing) in self.pts.iter().enumerate() {
            if existing.distance(pt) <= SNAP_EPS {
                return idx;
            }
        }
        let idx = self.pts.len();
        self.pts.push(pt);
        idx
    }
}

fn split_ring_edges_against_pool(rings: &[Vec<Point>], pool: &VertexPool) -> Vec<(usize, usize)> {
    const ON_SEG_EPS: f64 = 1e-5;
    let mut directed_edges = Vec::new();
    for ring in rings {
        let n = ring.len();
        for i in 0..n {
            let next_i = i.saturating_add(1).checked_rem(n).unwrap_or(0);
            let (Some(&a), Some(&b)) = (ring.get(i), ring.get(next_i)) else {
                continue;
            };
            let dx = b.x.get() - a.x.get();
            let dy = b.y.get() - a.y.get();
            let len_sq = dx * dx + dy * dy;
            if len_sq <= 1e-12 {
                continue;
            }
            let mut splits: Vec<(f64, usize)> = Vec::new();
            for (vid, &vpt) in pool.pts.iter().enumerate() {
                let vx = vpt.x.get() - a.x.get();
                let vy = vpt.y.get() - a.y.get();
                let t = (vx * dx + vy * dy) / len_sq;
                if (-1e-6..=1.0 + 1e-6).contains(&t) {
                    let tc = t.clamp(0.0, 1.0);
                    let proj_x = a.x.get() + tc * dx;
                    let proj_y = a.y.get() + tc * dy;
                    let dist = (vpt.x.get() - proj_x).hypot(vpt.y.get() - proj_y);
                    if dist <= ON_SEG_EPS {
                        splits.push((tc, vid));
                    }
                }
            }
            splits.sort_by(|x, y| x.0.total_cmp(&y.0));
            let mut chain: Vec<usize> = Vec::with_capacity(splits.len());
            for (_, vid) in splits {
                if chain.last().copied() != Some(vid) {
                    chain.push(vid);
                }
            }
            for pair in chain.windows(2) {
                if let [u, v] = *pair
                    && u != v
                {
                    directed_edges.push((u, v));
                }
            }
        }
    }
    directed_edges
}

fn clip_polylines(subj_list: &[Polyline], clip_list: &[Polyline], op: BooleanOp, fill_rule: crate::FillRule) -> Result<Path, GeomError> {
    let rings_a = extract_normalized_rings(subj_list);
    let rings_b = extract_normalized_rings(clip_list);
    if rings_a.is_empty() || rings_b.is_empty() {
        return match op {
            BooleanOp::Weld | BooleanOp::Exclude => {
                let mut all = rings_a;
                all.extend(rings_b);
                rings_to_path(&all, fill_rule)
            }
            BooleanOp::Trim | BooleanOp::FrontMinusBack => rings_to_path(&rings_a, fill_rule),
            BooleanOp::BackMinusFront => rings_to_path(&rings_b, fill_rule),
            BooleanOp::Intersect => Ok(Path::new()),
        };
    }

    // 1. Populate canonical VertexPool with all ring vertices and all pairwise intersection points
    let mut pool = VertexPool::new();
    for ring in rings_a.iter().chain(rings_b.iter()) {
        for &pt in ring {
            pool.intern(pt);
        }
    }
    for ra in &rings_a {
        let na = ra.len();
        for i in 0..na {
            let next_i = i.saturating_add(1).checked_rem(na).unwrap_or(0);
            let (Some(&a0), Some(&a1)) = (ra.get(i), ra.get(next_i)) else {
                continue;
            };
            for rb in &rings_b {
                let nb = rb.len();
                for j in 0..nb {
                    let next_j = j.saturating_add(1).checked_rem(nb).unwrap_or(0);
                    let (Some(&b0), Some(&b1)) = (rb.get(j), rb.get(next_j)) else {
                        continue;
                    };
                    if let Some(ipt) = segment_proper_or_touch_intersection(a0, a1, b0, b1) {
                        pool.intern(ipt);
                    }
                }
            }
        }
    }

    // 2. Split every ring edge against the canonical VertexPool
    let edges_a = split_ring_edges_against_pool(&rings_a, &pool);
    let edges_b = split_ring_edges_against_pool(&rings_b, &pool);

    // 3. Classify elementary edges
    let mut selected: Vec<(usize, usize)> = Vec::new();
    let mut used_b = vec![false; edges_b.len()];

    for &(ua, va) in &edges_a {
        // Check if this undirected elementary edge is shared with Clip
        let mut shared_b_idx: Option<usize> = None;
        for (bi, &(ub, vb)) in edges_b.iter().enumerate() {
            if !used_b.get(bi).copied().unwrap_or(true) && ((ua == ub && va == vb) || (ua == vb && va == ub)) {
                shared_b_idx = Some(bi);
                break;
            }
        }

        if let Some(bi) = shared_b_idx {
            if let Some(slot) = used_b.get_mut(bi) {
                *slot = true;
            }
            let Some(&(ub, vb)) = edges_b.get(bi) else {
                continue;
            };
            let same_dir = ua == ub && va == vb;
            match op {
                BooleanOp::Weld | BooleanOp::Intersect => {
                    if same_dir {
                        selected.push((ua, va));
                    }
                }
                BooleanOp::Trim | BooleanOp::FrontMinusBack => {
                    if !same_dir {
                        selected.push((ua, va));
                    }
                }
                BooleanOp::BackMinusFront => {
                    if !same_dir {
                        selected.push((ub, vb));
                    }
                }
                BooleanOp::Exclude => {}
            }
        } else {
            let (Some(&pa), Some(&pb)) = (pool.pts.get(ua), pool.pts.get(va)) else {
                continue;
            };
            let mid = Point::new(0.5 * (pa.x.get() + pb.x.get()), 0.5 * (pa.y.get() + pb.y.get()))?;
            let in_b = point_in_rings(mid, &rings_b);
            match op {
                BooleanOp::Weld | BooleanOp::Trim | BooleanOp::FrontMinusBack => {
                    if !in_b {
                        selected.push((ua, va));
                    }
                }
                BooleanOp::Intersect => {
                    if in_b {
                        selected.push((ua, va));
                    }
                }
                BooleanOp::BackMinusFront => {
                    if in_b {
                        selected.push((va, ua));
                    }
                }
                BooleanOp::Exclude => {}
            }
        }
    }

    for (bi, &(ub, vb)) in edges_b.iter().enumerate() {
        if used_b.get(bi).copied().unwrap_or(true) {
            continue;
        }
        let (Some(&pa), Some(&pb)) = (pool.pts.get(ub), pool.pts.get(vb)) else {
            continue;
        };
        let mid = Point::new(0.5 * (pa.x.get() + pb.x.get()), 0.5 * (pa.y.get() + pb.y.get()))?;
        let in_a = point_in_rings(mid, &rings_a);
        match op {
            BooleanOp::Weld | BooleanOp::BackMinusFront => {
                if !in_a {
                    selected.push((ub, vb));
                }
            }
            BooleanOp::Intersect => {
                if in_a {
                    selected.push((ub, vb));
                }
            }
            BooleanOp::Trim | BooleanOp::FrontMinusBack => {
                if in_a {
                    selected.push((vb, ub));
                }
            }
            BooleanOp::Exclude => {}
        }
    }

    // 4. Stitch directed elementary edges into closed polygon loops
    let loops = stitch_directed_edges(&selected, &pool.pts);
    rings_to_path(&loops, fill_rule)
}

fn stitch_directed_edges(edges: &[(usize, usize)], pts: &[Point]) -> Vec<Vec<Point>> {
    let mut used = vec![false; edges.len()];
    let mut loops: Vec<Vec<Point>> = Vec::new();

    for (start_idx, &(u0, v0)) in edges.iter().enumerate() {
        if used.get(start_idx).copied().unwrap_or(true) {
            continue;
        }
        if let Some(slot) = used.get_mut(start_idx) {
            *slot = true;
        }
        let mut chain: Vec<usize> = vec![u0, v0];

        loop {
            let curr = *chain.last().unwrap_or(&u0);
            if curr == u0 && chain.len() > 2 {
                chain.pop();
                if let Some(clean) = simplify_loop_indices(&chain, pts) {
                    loops.push(clean);
                }
                break;
            }

            // If `curr` already appeared earlier in `chain` (at index k > 0), extract the simple sub-cycle
            let prefix_len = chain.len().saturating_sub(1);
            if let Some(prefix) = chain.get(..prefix_len)
                && let Some(pos) = prefix.iter().position(|&v| v == curr)
                && pos > 0
                && let Some(sub_slice) = chain.get(pos..prefix_len)
            {
                let sub_cycle = sub_slice.to_vec();
                chain.truncate(pos.saturating_add(1));
                if let Some(clean) = simplify_loop_indices(&sub_cycle, pts) {
                    loops.push(clean);
                }
            }

            let prev = chain.get(chain.len().saturating_sub(2)).copied().unwrap_or(u0);
            let (Some(&p_prev), Some(&p_curr)) = (pts.get(prev), pts.get(curr)) else {
                break;
            };
            let in_angle = (p_prev.y.get() - p_curr.y.get()).atan2(p_prev.x.get() - p_curr.x.get());

            let mut best_edge: Option<(usize, f64)> = None;
            for (ei, &(u, v)) in edges.iter().enumerate() {
                if !used.get(ei).copied().unwrap_or(true)
                    && u == curr
                    && let Some(&p_next) = pts.get(v)
                {
                    let out_angle = (p_next.y.get() - p_curr.y.get()).atan2(p_next.x.get() - p_curr.x.get());
                    let mut turn = (out_angle - in_angle).rem_euclid(std::f64::consts::TAU);
                    if turn < 1e-9 {
                        turn = std::f64::consts::TAU;
                    }
                    if best_edge.is_none_or(|(_, best_turn)| turn < best_turn) {
                        best_edge = Some((ei, turn));
                    }
                }
            }

            if let Some((next_ei, _)) = best_edge
                && let Some(&(_, next_v)) = edges.get(next_ei)
            {
                if let Some(slot) = used.get_mut(next_ei) {
                    *slot = true;
                }
                chain.push(next_v);
            } else {
                break;
            }
        }
    }

    loops
}

fn simplify_loop_indices(indices: &[usize], pts: &[Point]) -> Option<Vec<Point>> {
    if indices.len() < 3 {
        return None;
    }
    let mut ring: Vec<Point> = indices.iter().filter_map(|&idx| pts.get(idx).copied()).collect();
    // Remove collinear intermediate vertices along straight split segments
    let mut changed = true;
    while changed && ring.len() >= 3 {
        changed = false;
        let n = ring.len();
        for i in 0..n {
            let prev_i = i.saturating_add(n.saturating_sub(1)).checked_rem(n).unwrap_or(0);
            let next_i = i.saturating_add(1).checked_rem(n).unwrap_or(0);
            let (Some(&p_prev), Some(&p_curr), Some(&p_next)) = (ring.get(prev_i), ring.get(i), ring.get(next_i)) else {
                continue;
            };
            let ax = p_curr.x.get() - p_prev.x.get();
            let ay = p_curr.y.get() - p_prev.y.get();
            let bx = p_next.x.get() - p_curr.x.get();
            let by = p_next.y.get() - p_curr.y.get();
            let la = ax.hypot(ay);
            let lb = bx.hypot(by);
            if la <= 1e-6 || lb <= 1e-6 {
                ring.remove(i);
                changed = true;
                break;
            }
            let dot = ax * bx + ay * by;
            let cross = (ax * by - ay * bx).abs();
            if dot > 0.0 && cross <= 1e-6 * la * lb {
                ring.remove(i);
                changed = true;
                break;
            }
        }
    }
    if ring.len() >= 3 && polygon_signed_area2(&ring).abs() > 1e-4 { Some(ring) } else { None }
}

fn rings_to_path(rings: &[Vec<Point>], fill_rule: crate::FillRule) -> Result<Path, GeomError> {
    let mut out = Path { elements: Vec::new(), fill_rule };
    for ring in rings {
        if ring.len() >= 3 {
            add_polyline_to_path(ring, true, &mut out)?;
        }
    }
    Ok(out)
}

fn add_polyline_to_path(points: &[Point], closed: bool, path: &mut Path) -> Result<(), GeomError> {
    let Some((&first, rest)) = points.split_first() else {
        return Ok(());
    };
    path.move_to(first)?;
    for &p in rest {
        path.line_to(p)?;
    }
    if closed {
        path.close()?;
    }
    Ok(())
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

/// Lays out `text` glyph-by-glyph along `spine` (`Text -> Fit Text to Path`) using arc-length
/// parameterization and tangent rotation, with perpendicular baseline offset `offset_pt`.
pub fn fit_text_to_path(text: &str, font_family: &str, size_pt: f64, spine: &Path, offset_pt: f64) -> Result<Path, GeomError> {
    let lut = crate::path::ArcLengthTable::from_path(spine)?;
    if lut.total_length() <= 1e-6 {
        return Ok(artistic_text_to_path(text, font_family, size_pt, Point::ZERO));
    }
    let scale = (size_pt / 10.0).max(0.1);
    let total_len = lut.total_length();
    let mut out = Path::new();
    let mut s_cur = 0.0_f64;

    for ch in text.chars() {
        let (w, glyph_ops) = glyph_outline(ch);
        let u = (s_cur / total_len).clamp(0.0, 1.0);
        if let Some(sample) = lut.sample_normalized(u) {
            let nx = sample.tangent.1;
            let ny = -sample.tangent.0;
            let px = sample.point.x.get() + offset_pt * nx;
            let py = sample.point.y.get() + offset_pt * ny;
            let tf = crate::affine::Affine2::rotation_deg(sample.angle_deg)?.then(crate::affine::Affine2::translation(px, py)?)?;
            for op in glyph_ops {
                match *op {
                    GlyphOp::MoveTo(x, y) => {
                        let pt = tf.apply_point(Point::new(x * scale, -y * scale)?)?;
                        out.move_to(pt)?;
                    }
                    GlyphOp::LineTo(x, y) => {
                        let pt = tf.apply_point(Point::new(x * scale, -y * scale)?)?;
                        out.line_to(pt)?;
                    }
                    GlyphOp::Close => {
                        out.close()?;
                    }
                }
            }
        }
        s_cur += (w + 1.5) * scale;
    }

    Ok(out)
}

/// Builds a parametric CorelDRAW Linear Dimension path (`p1 -> p2`) with extension lines,
/// arrowheads, and a centered measurement `label` offset by `offset_pt`.
pub fn build_linear_dimension_path(p1: Point, p2: Point, offset_pt: f64, label: &str, font_size_pt: f64) -> Result<Path, GeomError> {
    if !offset_pt.is_finite() || !font_size_pt.is_finite() {
        return Err(GeomError::NonFiniteCoordinate);
    }
    if font_size_pt <= 0.0 {
        return Err(GeomError::InvalidShapeParameter("dimension font size must be positive"));
    }
    let dx = p2.x.get() - p1.x.get();
    let dy = p2.y.get() - p1.y.get();
    let len = dx.hypot(dy);
    if len < 1e-6 {
        return Err(GeomError::InvalidShapeParameter("degenerate dimension"));
    }

    let arrow = (font_size_pt * 0.4).clamp(2.0, 8.0);
    let sign = if offset_pt >= 0.0 { -1.0 } else { 1.0 };
    let by = -offset_pt;
    let ext_end = by + sign * arrow * 0.75;

    let mut local = Path::new();
    // 1. Extension line 1 and 2
    local.move_to(Point::new(0.0, 0.0)?)?;
    local.line_to(Point::new(0.0, ext_end)?)?;
    local.move_to(Point::new(len, 0.0)?)?;
    local.line_to(Point::new(len, ext_end)?)?;

    // 2. Dimension line
    local.move_to(Point::new(0.0, by)?)?;
    local.line_to(Point::new(len, by)?)?;

    // 3. Left and right arrowheads
    let aw = arrow.min(len * 0.4);
    let ah = aw * 0.4;
    local.move_to(Point::new(0.0, by)?)?;
    local.line_to(Point::new(aw, by - ah)?)?;
    local.line_to(Point::new(aw, by + ah)?)?;
    local.close()?;

    local.move_to(Point::new(len, by)?)?;
    local.line_to(Point::new(len - aw, by - ah)?)?;
    local.line_to(Point::new(len - aw, by + ah)?)?;
    local.close()?;

    // 4. Centered measurement label
    if !label.is_empty() {
        let scale = (font_size_pt / 10.0).max(0.1);
        let label_w: f64 = label.chars().map(|ch| (glyph_outline(ch).0 + 1.5) * scale).sum();
        let text_origin = Point::new((len - label_w) * 0.5, by - 3.0)?;
        let label_path = artistic_text_to_path(label, "Sans", font_size_pt, text_origin);
        for el in label_path.elements {
            local.push(el)?;
        }
    }

    let angle_deg = dy.atan2(dx).to_degrees();
    let tf = crate::affine::Affine2::rotation_deg(angle_deg)?.then(crate::affine::Affine2::translation(p1.x.get(), p1.y.get())?)?;
    local.transform(tf)
}

/// Builds a parametric CorelDRAW Angular Dimension path measuring the interior angle at `vertex`
/// between `ray1_pt` and `ray2_pt` with circular dimension arc at `arc_radius_pt`, extension rays,
/// tangent arrowheads, and an angle `label` positioned outside the arc midpoint.
pub fn build_angular_dimension_path(
    vertex: Point,
    ray1_pt: Point,
    ray2_pt: Point,
    arc_radius_pt: f64,
    label: &str,
    font_size_pt: f64,
) -> Result<Path, GeomError> {
    if !arc_radius_pt.is_finite() || !font_size_pt.is_finite() {
        return Err(GeomError::NonFiniteCoordinate);
    }
    if arc_radius_pt <= 0.0 || font_size_pt <= 0.0 {
        return Err(GeomError::InvalidShapeParameter("angular dimension radius and font size must be positive"));
    }
    let vx = vertex.x.get();
    let vy = vertex.y.get();
    let dx1 = ray1_pt.x.get() - vx;
    let dy1 = ray1_pt.y.get() - vy;
    let dx2 = ray2_pt.x.get() - vx;
    let dy2 = ray2_pt.y.get() - vy;
    if dx1.hypot(dy1) < 1e-6 || dx2.hypot(dy2) < 1e-6 {
        return Err(GeomError::InvalidShapeParameter("degenerate angular dimension ray"));
    }

    let a1 = dy1.atan2(dx1);
    let a2 = dy2.atan2(dx2);
    let mut delta = (a2 - a1).rem_euclid(std::f64::consts::TAU);
    if delta > std::f64::consts::PI {
        delta -= std::f64::consts::TAU;
    }
    if delta.abs() < 1e-6 {
        return Err(GeomError::InvalidShapeParameter("collinear angular dimension rays"));
    }

    let arrow = (font_size_pt * 0.4).clamp(2.0, 8.0);
    let ext_r = arc_radius_pt + arrow * 0.75;
    let a_end = a1 + delta;

    let mut path = Path::new();
    // 1. Extension rays from vertex
    path.move_to(vertex)?;
    path.line_to(Point::new(vx + ext_r * a1.cos(), vy + ext_r * a1.sin())?)?;
    path.move_to(vertex)?;
    path.line_to(Point::new(vx + ext_r * a_end.cos(), vy + ext_r * a_end.sin())?)?;

    // 2. Dimension circular arc (16 segments)
    let steps = 16_u32;
    let p_arc_start = Point::new(vx + arc_radius_pt * a1.cos(), vy + arc_radius_pt * a1.sin())?;
    path.move_to(p_arc_start)?;
    for step in 1..=steps {
        let t = f64::from(step) / f64::from(steps);
        let ang = a1 + delta * t;
        path.line_to(Point::new(vx + arc_radius_pt * ang.cos(), vy + arc_radius_pt * ang.sin())?)?;
    }

    // 3. Tangent arrowheads at arc endpoints
    let dir_sign = if delta >= 0.0 { 1.0 } else { -1.0 };
    let aw = arrow.min(arc_radius_pt * delta.abs() * 0.4);
    let ah = aw * 0.4;

    // Start arrowhead pointing along +dir_sign tangent
    let tx1 = -a1.sin() * dir_sign;
    let ty1 = a1.cos() * dir_sign;
    let rx1 = a1.cos();
    let ry1 = a1.sin();
    path.move_to(p_arc_start)?;
    path.line_to(Point::new(p_arc_start.x.get() + aw * tx1 - ah * rx1, p_arc_start.y.get() + aw * ty1 - ah * ry1)?)?;
    path.line_to(Point::new(p_arc_start.x.get() + aw * tx1 + ah * rx1, p_arc_start.y.get() + aw * ty1 + ah * ry1)?)?;
    path.close()?;

    // End arrowhead pointing along -dir_sign tangent
    let p_arc_end = Point::new(vx + arc_radius_pt * a_end.cos(), vy + arc_radius_pt * a_end.sin())?;
    let tx2 = a_end.sin() * dir_sign;
    let ty2 = -a_end.cos() * dir_sign;
    let rx2 = a_end.cos();
    let ry2 = a_end.sin();
    path.move_to(p_arc_end)?;
    path.line_to(Point::new(p_arc_end.x.get() + aw * tx2 - ah * rx2, p_arc_end.y.get() + aw * ty2 - ah * ry2)?)?;
    path.line_to(Point::new(p_arc_end.x.get() + aw * tx2 + ah * rx2, p_arc_end.y.get() + aw * ty2 + ah * ry2)?)?;
    path.close()?;

    // 4. Angle measurement label outside the arc midpoint
    if !label.is_empty() {
        let a_mid = a1 + delta * 0.5;
        let label_r = arc_radius_pt + font_size_pt * 0.8;
        let scale = (font_size_pt / 10.0).max(0.1);
        let label_w: f64 = label.chars().map(|ch| (glyph_outline(ch).0 + 1.5) * scale).sum();
        let text_origin = Point::new(vx + label_r * a_mid.cos() - label_w * 0.5, vy + label_r * a_mid.sin())?;
        let label_path = artistic_text_to_path(label, "Sans", font_size_pt, text_origin);
        for el in label_path.elements {
            path.push(el)?;
        }
    }

    Ok(path)
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

/// Clips a closed polygon `pts` against the half-plane `(p - origin) · (nx, ny) >= offset`
/// using Sutherland–Hodgman polygon clipping.
pub fn clip_polygon_half_plane(pts: &[Point], origin: Point, nx: f64, ny: f64, offset: f64) -> Result<Vec<Point>, GeomError> {
    if pts.len() < 3 {
        return Ok(Vec::new());
    }
    let Some(&last_pt) = pts.last() else {
        return Ok(Vec::new());
    };
    let signed_dist = |p: Point| -> f64 { (p.x.get() - origin.x.get()) * nx + (p.y.get() - origin.y.get()) * ny - offset };
    let mut out = Vec::with_capacity(pts.len().saturating_add(2));
    let mut prev = last_pt;
    let mut d_prev = signed_dist(prev);
    for &curr in pts {
        let d_curr = signed_dist(curr);
        let prev_in = d_prev >= -1e-9;
        let curr_in = d_curr >= -1e-9;
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
}

/// Slices a closed [`Path`] along the cut line through `line_start` and `line_end` with blade
/// kerf width `gap_pt >= 0.0` (`CorelDRAW Knife Tool`), emitting both clipped half-space loops.
pub fn knife_cut_path_by_line(path: &Path, line_start: Point, line_end: Point, gap_pt: f64) -> Result<Path, GeomError> {
    let gap = crate::F64::new(gap_pt)?.get();
    if gap < 0.0 {
        return Err(GeomError::InvalidShapeParameter("knife gap_pt must be non-negative"));
    }
    let dx = line_end.x.get() - line_start.x.get();
    let dy = line_end.y.get() - line_start.y.get();
    let len = dx.hypot(dy);
    if len <= crate::EPS {
        return Err(GeomError::InvalidShapeParameter("knife cut line endpoints must be distinct"));
    }
    let nx = -dy / len;
    let ny = dx / len;
    let h = 0.5 * gap.max(1e-4);
    let polylines = crate::flatten::flatten_path(path, &crate::flatten::FlattenOpts::default())?;
    let mut out = Path { elements: Vec::new(), fill_rule: path.fill_rule };
    for poly in polylines {
        if poly.points.len() < 3 {
            continue;
        }
        for (sx, sy) in [(nx, ny), (-nx, -ny)] {
            let clipped = clip_polygon_half_plane(&poly.points, line_start, sx, sy, h)?;
            if clipped.len() >= 3
                && let Some((&first, rest)) = clipped.split_first()
            {
                out.move_to(first)?;
                for &pt in rest {
                    out.line_to(pt)?;
                }
                out.close()?;
            }
        }
    }
    Ok(out)
}

/// Erases (subtracts) a circular nib of `radius_pt` centered at `center` from `path`
/// (`CorelDRAW Eraser Tool X`).
///
/// - When the circle is completely disjoint from `path.bounds()`, returns `path` unchanged.
/// - When the circle lies strictly inside a closed polygon subpath, punches a 24-gon circular
///   counter-wound hole with `FillRule::EvenOdd`.
/// - When the circle intersects open or closed segments, clips out portions inside `radius_pt`
///   at exact segment-circle intersection parameters $t \in [0, 1]$.
pub fn erase_path_within_circle(path: &Path, center: Point, radius_pt: f64) -> Result<Path, GeomError> {
    let r = crate::F64::new(radius_pt)?.get();
    if r <= crate::EPS {
        return Err(GeomError::InvalidShapeParameter("eraser radius must be positive"));
    }
    let cx = center.x.get();
    let cy = center.y.get();
    if let Some(b) = path.bounds()
        && (cx + r < b.x0.get() || cx - r > b.x1.get() || cy + r < b.y0.get() || cy - r > b.y1.get())
    {
        return Ok(path.clone());
    }
    let polylines = crate::flatten::flatten_path(path, &crate::flatten::FlattenOpts::default())?;
    let mut out = Path { elements: Vec::new(), fill_rule: crate::FillRule::EvenOdd };

    let seg_circle_ts = |a: Point, b: Point| -> Vec<f64> {
        let dx = b.x.get() - a.x.get();
        let dy = b.y.get() - a.y.get();
        let fx = a.x.get() - cx;
        let fy = a.y.get() - cy;
        let qa = dx * dx + dy * dy;
        if qa <= 1e-12 {
            return Vec::new();
        }
        let qb = 2.0 * (fx * dx + fy * dy);
        let qc = fx * fx + fy * fy - r * r;
        let disc = qb * qb - 4.0 * qa * qc;
        if disc < 0.0 {
            return Vec::new();
        }
        let s = disc.sqrt();
        let mut ts = Vec::new();
        for t in [(-qb - s) / (2.0 * qa), (-qb + s) / (2.0 * qa)] {
            if (0.0..=1.0).contains(&t) {
                ts.push(t);
            }
        }
        ts.sort_by(|x, y| x.total_cmp(y));
        ts
    };

    for poly in polylines {
        if poly.points.len() < 2 {
            continue;
        }
        if !poly.closed {
            let mut current_run: Vec<Point> = Vec::new();
            for seg in poly.points.windows(2) {
                let [a, b] = *seg else { continue };
                let mut cuts = vec![0.0];
                cuts.extend(seg_circle_ts(a, b));
                cuts.push(1.0);
                for pair in cuts.windows(2) {
                    let [t0, t1] = *pair else { continue };
                    if t1 - t0 <= 1e-7 {
                        continue;
                    }
                    let tm = 0.5 * (t0 + t1);
                    let mx = a.x.get() + tm * (b.x.get() - a.x.get());
                    let my = a.y.get() + tm * (b.y.get() - a.y.get());
                    let inside = (mx - cx).hypot(my - cy) < r - 1e-7;
                    if inside {
                        if current_run.len() >= 2
                            && let Some((&first, rest)) = current_run.split_first()
                        {
                            out.move_to(first)?;
                            for &pt in rest {
                                out.line_to(pt)?;
                            }
                        }
                        current_run.clear();
                    } else {
                        let p0 = Point::new(a.x.get() + t0 * (b.x.get() - a.x.get()), a.y.get() + t0 * (b.y.get() - a.y.get()))?;
                        let p1 = Point::new(a.x.get() + t1 * (b.x.get() - a.x.get()), a.y.get() + t1 * (b.y.get() - a.y.get()))?;
                        if current_run.is_empty() {
                            current_run.push(p0);
                        }
                        current_run.push(p1);
                    }
                }
            }
            if current_run.len() >= 2
                && let Some((&first, rest)) = current_run.split_first()
            {
                out.move_to(first)?;
                for &pt in rest {
                    out.line_to(pt)?;
                }
            }
            continue;
        }

        // Closed polygon subpath
        let all_outside = poly.points.iter().all(|p| (p.x.get() - cx).hypot(p.y.get() - cy) >= r);
        let n = poly.points.len();
        let any_seg_hit = (0..n).any(|i| {
            let next_i = i.saturating_add(1).checked_rem(n).unwrap_or(0);
            if let (Some(&pa), Some(&pb)) = (poly.points.get(i), poly.points.get(next_i)) { !seg_circle_ts(pa, pb).is_empty() } else { false }
        });
        if all_outside
            && !any_seg_hit
            && point_in_polygon(center, &poly.points)
            && let Some((&first, rest)) = poly.points.split_first()
        {
            out.move_to(first)?;
            for &pt in rest {
                out.line_to(pt)?;
            }
            out.close()?;
            let hole_steps = 24u32;
            for i in 0..hole_steps {
                let ang = -std::f64::consts::TAU * f64::from(i) / f64::from(hole_steps);
                let hp = Point::new(cx + r * ang.cos(), cy + r * ang.sin())?;
                if i == 0 {
                    out.move_to(hp)?;
                } else {
                    out.line_to(hp)?;
                }
            }
            out.close()?;
            continue;
        }

        let mut ring: Vec<Point> = Vec::new();
        for i in 0..n {
            let next_i = i.saturating_add(1).checked_rem(n).unwrap_or(0);
            let (Some(&a), Some(&b)) = (poly.points.get(i), poly.points.get(next_i)) else {
                continue;
            };
            let mut cuts = vec![0.0];
            cuts.extend(seg_circle_ts(a, b));
            cuts.push(1.0);
            for pair in cuts.windows(2) {
                let [t0, t1] = *pair else { continue };
                if t1 - t0 <= 1e-7 {
                    continue;
                }
                let tm = 0.5 * (t0 + t1);
                let mx = a.x.get() + tm * (b.x.get() - a.x.get());
                let my = a.y.get() + tm * (b.y.get() - a.y.get());
                if (mx - cx).hypot(my - cy) >= r - 1e-7 {
                    let p0 = Point::new(a.x.get() + t0 * (b.x.get() - a.x.get()), a.y.get() + t0 * (b.y.get() - a.y.get()))?;
                    let p1 = Point::new(a.x.get() + t1 * (b.x.get() - a.x.get()), a.y.get() + t1 * (b.y.get() - a.y.get()))?;
                    if ring.last().is_none_or(|last| (last.x.get() - p0.x.get()).hypot(last.y.get() - p0.y.get()) > 1e-6) {
                        ring.push(p0);
                    }
                    ring.push(p1);
                }
            }
        }
        if ring.len() >= 3
            && let Some((&first, rest)) = ring.split_first()
        {
            out.move_to(first)?;
            for &pt in rest {
                out.line_to(pt)?;
            }
            out.close()?;
        }
    }
    Ok(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing, clippy::arithmetic_side_effects)]
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

        // 3b. Exclude (XOR)
        let excl = boolean_paths(&r1, &r2, BooleanOp::Exclude).expect("exclude");
        assert_eq!(excl.fill_rule, crate::path::FillRule::EvenOdd);
        let excl_closes = excl.elements.iter().filter(|e| matches!(e, crate::path::PathElement::Close)).count();
        assert_eq!(excl_closes, 2);

        // 4. Knife cut splits rectangle into 2 closed subpaths
        let cut = knife_cut_path_by_line(&r1, Point::new(-10.0, 25.0).unwrap(), Point::new(60.0, 25.0).unwrap(), 2.0).expect("knife cut");
        let move_count = cut.elements.iter().filter(|e| matches!(e, crate::path::PathElement::MoveTo(_))).count();
        assert_eq!(move_count, 2);

        // 5. Eraser nib subtraction: interior hole punches 2 subpaths, open line splits into 2 open subpaths
        let hole_erased = erase_path_within_circle(&r1, Point::new(25.0, 25.0).unwrap(), 8.0).expect("eraser interior hole");
        let hole_moves = hole_erased.elements.iter().filter(|e| matches!(e, crate::path::PathElement::MoveTo(_))).count();
        assert_eq!(hole_moves, 2);

        let mut open_line = Path::new();
        open_line.move_to(Point::new(0.0, 25.0).unwrap()).unwrap();
        open_line.line_to(Point::new(100.0, 25.0).unwrap()).unwrap();
        let split_open = erase_path_within_circle(&open_line, Point::new(50.0, 25.0).unwrap(), 10.0).expect("eraser open split");
        let open_moves = split_open.elements.iter().filter(|e| matches!(e, crate::path::PathElement::MoveTo(_))).count();
        assert_eq!(open_moves, 2);
    }

    #[test]
    fn artistic_text_to_path_generates_vector_loops() {
        let pt = Point::new(10.0, 100.0).unwrap();
        let path = artistic_text_to_path("CORAL", "Sans", 24.0, pt);
        assert!(!path.elements.is_empty());
        let bounds = path.bounds().expect("text bounds");
        assert!(bounds.width() > 20.0);
        assert!(bounds.height() > 10.0);

        let mut spine = Path::new();
        spine.move_to(Point::new(0.0, 50.0).unwrap()).unwrap();
        spine.line_to(Point::new(200.0, 150.0).unwrap()).unwrap();
        let curved_text = fit_text_to_path("CORAL", "Sans", 18.0, &spine, 2.0).unwrap();
        let cb = curved_text.bounds().unwrap();
        assert!(cb.width() > 20.0);
        assert!(cb.height() > 15.0);

        let dim = build_linear_dimension_path(Point::new(20.0, 100.0).unwrap(), Point::new(140.0, 100.0).unwrap(), 18.0, "120 pt", 10.0).unwrap();
        let db = dim.bounds().unwrap();
        assert!((db.x0.get() - 20.0).abs() < 1e-4);
        assert!((db.x1.get() - 140.0).abs() < 1e-4);
        assert!(db.height() >= 18.0);

        let ang_dim = build_angular_dimension_path(
            Point::new(100.0, 100.0).unwrap(),
            Point::new(180.0, 100.0).unwrap(),
            Point::new(100.0, 180.0).unwrap(),
            40.0,
            "90 deg",
            10.0,
        )
        .unwrap();
        let ab = ang_dim.bounds().unwrap();
        assert!(ab.width() >= 40.0);
        assert!(ab.height() >= 40.0);
    }

    #[test]
    fn boolean_planar_clipper_area_identities_holes_splits_and_grid_oracle_r5_1() {
        use crate::path::{PathElement, measure_path_length_and_area};
        use crate::shapes::{EllipseSpec, PolygonSpec};

        // 1. Overlapping rectangles with shared top/bottom collinear edges:
        // r1 = [0,0,50,50] (area 2500), r2 = [25,0,75,50] (area 2500)
        // weld = 3750, inter = 1250, trim = 1250, back_minus_front = 1250, exclude = 2500
        let r1 = RectSpec::sharp(Rect::new(0.0, 0.0, 50.0, 50.0).unwrap()).to_path().unwrap();
        let r2 = RectSpec::sharp(Rect::new(25.0, 0.0, 75.0, 50.0).unwrap()).to_path().unwrap();
        let (_, area_weld) = measure_path_length_and_area(&boolean_paths(&r1, &r2, BooleanOp::Weld).unwrap()).unwrap();
        let (_, area_inter) = measure_path_length_and_area(&boolean_paths(&r1, &r2, BooleanOp::Intersect).unwrap()).unwrap();
        let (_, area_trim) = measure_path_length_and_area(&boolean_paths(&r1, &r2, BooleanOp::Trim).unwrap()).unwrap();
        let (_, area_bmf) = measure_path_length_and_area(&boolean_paths(&r1, &r2, BooleanOp::BackMinusFront).unwrap()).unwrap();
        let (_, area_excl) = measure_path_length_and_area(&boolean_paths(&r1, &r2, BooleanOp::Exclude).unwrap()).unwrap();
        assert!((area_weld - 3750.0).abs() < 1.0, "expected weld area 3750, got {area_weld}");
        assert!((area_inter - 1250.0).abs() < 1.0, "expected inter area 1250, got {area_inter}");
        assert!((area_trim - 1250.0).abs() < 1.0, "expected trim area 1250, got {area_trim}");
        assert!((area_bmf - 1250.0).abs() < 1.0, "expected bmf area 1250, got {area_bmf}");
        assert!((area_excl - 2500.0).abs() < 1.0, "expected exclude area 2500, got {area_excl}");

        // 2. Crossed bars: horizontal [0, 15, 60, 45] (area 1800) and vertical [15, 0, 45, 60] (area 1800)
        // Overlap is [15, 15, 45, 45] (area 900).
        // Trim (horiz \ vert) must split horizontal bar into 2 disjoint rectangles of total area 900.
        let horiz = RectSpec::sharp(Rect::new(0.0, 15.0, 60.0, 45.0).unwrap()).to_path().unwrap();
        let vert = RectSpec::sharp(Rect::new(15.0, 0.0, 45.0, 60.0).unwrap()).to_path().unwrap();
        let cross_weld = boolean_paths(&horiz, &vert, BooleanOp::Weld).unwrap();
        let cross_trim = boolean_paths(&horiz, &vert, BooleanOp::Trim).unwrap();
        let cross_inter = boolean_paths(&horiz, &vert, BooleanOp::Intersect).unwrap();
        let (_, cw_area) = measure_path_length_and_area(&cross_weld).unwrap();
        let (_, ct_area) = measure_path_length_and_area(&cross_trim).unwrap();
        let (_, ci_area) = measure_path_length_and_area(&cross_inter).unwrap();
        assert!((cw_area - 2700.0).abs() < 1.0, "cross weld area {cw_area} != 2700");
        assert!((ct_area - 900.0).abs() < 1.0, "cross trim area {ct_area} != 900");
        assert!((ci_area - 900.0).abs() < 1.0, "cross inter area {ci_area} != 900");
        let ct_closes = cross_trim.elements.iter().filter(|e| matches!(e, PathElement::Close)).count();
        assert_eq!(ct_closes, 2, "cross trim should produce 2 disjoint closed loops");

        // 3. Interior hole punch: outer [0,0,60,60] (area 3600) minus inner [15,15,45,45] (area 900) -> net area 2700, 2 loops
        let outer = RectSpec::sharp(Rect::new(0.0, 0.0, 60.0, 60.0).unwrap()).to_path().unwrap();
        let inner = RectSpec::sharp(Rect::new(15.0, 15.0, 45.0, 45.0).unwrap()).to_path().unwrap();
        let punched = boolean_paths(&outer, &inner, BooleanOp::Trim).unwrap();
        let (_, punched_area) = measure_path_length_and_area(&punched).unwrap();
        let punched_closes = punched.elements.iter().filter(|e| matches!(e, PathElement::Close)).count();
        assert_eq!(punched_closes, 2, "hole punch must have outer + inner contour");
        assert!((punched_area - 2700.0).abs() < 1.0, "hole punch net area {punched_area} != 2700");

        // Reverse: inner minus outer must be completely empty!
        let swallowed = boolean_paths(&inner, &outer, BooleanOp::Trim).unwrap();
        assert!(swallowed.elements.is_empty(), "inner minus outer must be empty");

        // 4. Disjoint circles with overlapping bounding boxes: Intersect must be empty!
        let c1 = EllipseSpec::full(Rect::new(0.0, 0.0, 40.0, 40.0).unwrap()).to_path().unwrap();
        let c2 = EllipseSpec::full(Rect::new(35.0, 35.0, 75.0, 75.0).unwrap()).to_path().unwrap();
        let c_inter = boolean_paths(&c1, &c2, BooleanOp::Intersect).unwrap();
        assert!(c_inter.elements.is_empty(), "disjoint circles with overlapping bboxes must have empty intersection");

        // 5. Star and circle area identity + grid point-membership oracle
        let star = PolygonSpec::star(Point::new(30.0, 30.0).unwrap(), 28.0, 12.0, 5, 0.0).unwrap().to_path().unwrap();
        let disc = EllipseSpec::full(Rect::new(15.0, 10.0, 65.0, 55.0).unwrap()).to_path().unwrap();
        let (_, a_star) = measure_path_length_and_area(&star).unwrap();
        let (_, a_disc) = measure_path_length_and_area(&disc).unwrap();
        let u_sd = boolean_paths(&star, &disc, BooleanOp::Weld).unwrap();
        let i_sd = boolean_paths(&star, &disc, BooleanOp::Intersect).unwrap();
        let t_sd = boolean_paths(&star, &disc, BooleanOp::Trim).unwrap();
        let b_sd = boolean_paths(&star, &disc, BooleanOp::BackMinusFront).unwrap();
        let x_sd = boolean_paths(&star, &disc, BooleanOp::Exclude).unwrap();
        let (_, a_u) = measure_path_length_and_area(&u_sd).unwrap();
        let (_, a_i) = measure_path_length_and_area(&i_sd).unwrap();
        let (_, a_t) = measure_path_length_and_area(&t_sd).unwrap();
        let (_, a_b) = measure_path_length_and_area(&b_sd).unwrap();
        let (_, a_x) = measure_path_length_and_area(&x_sd).unwrap();
        assert!(
            ((a_u + a_i) - (a_star + a_disc)).abs() < 2.0,
            "area(A u B) + area(A n B) != area(A) + area(B): {a_u} + {a_i} vs {a_star} + {a_disc}"
        );
        assert!(((a_t + a_i) - a_star).abs() < 2.0, "area(A \\ B) + area(A n B) != area(A): {a_t} + {a_i} vs {a_star}");
        assert!(((a_b + a_i) - a_disc).abs() < 2.0, "area(B \\ A) + area(A n B) != area(B): {a_b} + {a_i} vs {a_disc}");
        assert!((a_x - (a_t + a_b)).abs() < 2.0, "area(A ^ B) != area(A \\ B) + area(B \\ A): {a_x} vs {a_t} + {a_b}");

        // Grid membership oracle on cross_weld, cross_trim, cross_inter, and exclude
        let cross_excl = boolean_paths(&horiz, &vert, BooleanOp::Exclude).unwrap();
        let cross_bmf = boolean_paths(&horiz, &vert, BooleanOp::BackMinusFront).unwrap();
        for gx in 0..20 {
            for gy in 0..20 {
                let x = 1.5 + f64::from(gx) * 3.0;
                let y = 1.5 + f64::from(gy) * 3.0;
                // Skip points close to the rectangle boundaries x/y in {0, 15, 45, 60}
                if [0.0, 15.0, 45.0, 60.0].iter().any(|&b| (x - b).abs() < 0.8 || (y - b).abs() < 0.8) {
                    continue;
                }
                let pt = Point::new(x, y).unwrap();
                let in_a = path_contains_point(&horiz, pt);
                let in_b = path_contains_point(&vert, pt);
                assert_eq!(path_contains_point(&cross_weld, pt), in_a || in_b, "weld mismatch at ({x}, {y})");
                assert_eq!(path_contains_point(&cross_inter, pt), in_a && in_b, "inter mismatch at ({x}, {y})");
                assert_eq!(path_contains_point(&cross_trim, pt), in_a && !in_b, "trim mismatch at ({x}, {y})");
                assert_eq!(path_contains_point(&cross_bmf, pt), in_b && !in_a, "bmf mismatch at ({x}, {y})");
                assert_eq!(path_contains_point(&cross_excl, pt), in_a ^ in_b, "excl mismatch at ({x}, {y})");
            }
        }
    }
}
