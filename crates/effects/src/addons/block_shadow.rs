//! CoralCraft (P3 Task E2): the Block Shadow effect — the object's silhouette swept solid along
//! (x, y), drawn behind the object. The sweep is the union of the outline, its translation and
//! the side walls between them (the edges the sweep runs over), as one filled path in the
//! effect's colour; the object itself draws unchanged on top. Paths and compound paths only (the
//! validation names the limit), like the solids.

use serde_json::{Value, json};
use vectorcraft_doc::color::{Color, Paint};
use vectorcraft_doc::{Appearance, Node};
use vectorcraft_geom::{Affine, BezPath, FillRule, PathData, Point, Vec2};
use vectorcraft_pathops::unite_all;

use crate::catalog::{EffectInfo, Lengths};

pub const BLOCK_SHADOW: &str = "stylize.blockShadow";

/// Most flattened outline points one shadow sweeps (past it the object draws as it is).
pub const MAX_OUTLINE_POINTS: usize = 10_000;

/// The effect's catalogue entry.
pub(crate) fn catalog() -> Vec<EffectInfo> {
    vec![EffectInfo {
        id: BLOCK_SHADOW,
        label: "Block Shadow…",
        menu: &["Effect", "Stylize"],
        params: "{x: pt (7), y: pt (7), color: colour (\"#000000\")} draws the object's silhouette swept solid along (x, y) behind it, as CorelDRAW's Block Shadow does (paths and compound paths only)",
        defaults: json!({"x": 7.0, "y": 7.0, "color": "#000000"}),
        raster: false,
        lengths: Lengths { always: &["x", "y"], absolute: &[] },
    }]
}

/// Does `n` carry a visible Block Shadow effect?
pub fn has_block_shadow(n: &Node) -> bool {
    n.appearance.effects.iter().any(|e| e.visible && e.id == BLOCK_SHADOW)
}

fn num(v: &Value, key: &str, default: f64) -> f64 {
    v.get(key).and_then(Value::as_f64).unwrap_or(default)
}

/// The effect's parameters on `n`: the offset and the colour, when they are usable.
fn params_of(n: &Node) -> Option<(f64, f64, Color)> {
    let fx = n.appearance.effects.iter().find(|e| e.visible && e.id == BLOCK_SHADOW)?;
    let merged = crate::merged_params(BLOCK_SHADOW, &fx.params);
    let (x, y) = (num(&merged, "x", 7.0), num(&merged, "y", 7.0));
    if !x.is_finite() || !y.is_finite() {
        return None;
    }
    let color = merged.get("color").and_then(Value::as_str).and_then(Color::from_hex).unwrap_or(Color::BLACK);
    Some((x, y, color))
}

/// Validate before a command changes the document.
pub fn validate(n: &Node, p: &Value) -> Result<(), String> {
    for key in ["x", "y"] {
        match p.get(key) {
            Some(Value::Null) | None => {}
            Some(v) if v.as_f64().is_some_and(f64::is_finite) => {}
            _ => return Err(format!("Block Shadow's {key} must be a number")),
        }
    }
    let (x, y) = (num(p, "x", 7.0), num(p, "y", 7.0));
    let _ = (x, y);
    if let Some(c) = p.get("color")
        && !c.is_null()
        && c.as_str().and_then(Color::from_hex).is_none()
    {
        return Err("Block Shadow's color must be a colour".into());
    }
    if crate::solids::profile(n).is_none() {
        return Err("Block Shadow currently requires a path or compound path. Convert type, images and groups to paths first.".into());
    }
    Ok(())
}

/// The node's art with its Block Shadow: the swept silhouette behind the unchanged object. This
/// replaces the node wherever its art is asked for; the object keeps every effect but this one.
pub fn block_shadow_art(n: &Node) -> Option<Node> {
    if !has_block_shadow(n) {
        return None;
    }
    let (x, y, color) = params_of(n)?;
    let (path, rule) = crate::solids::profile(n)?;
    let swept = sweep(&path, rule, x, y)?;
    let shadow = Node::path(n.id, swept, Appearance::basic(Paint::solid(color), Paint::None, 0.0));
    let mut above = n.clone();
    above.id = n.id;
    above.appearance.effects.retain(|e| e.id != BLOCK_SHADOW);
    Some(Node::group(n.id, vec![std::sync::Arc::new(shadow), std::sync::Arc::new(above)]))
}

/// The silhouette of `path` swept along `(x, y)`: the union of the outline, its translation and
/// the side walls between them. None past [`MAX_OUTLINE_POINTS`] points (the caller draws the
/// object as it is) or when the union fails.
fn sweep(path: &PathData, rule: FillRule, x: f64, y: f64) -> Option<PathData> {
    if x == 0.0 && y == 0.0 {
        return Some(path.clone());
    }
    let polys = flatten_polys(path);
    if polys.iter().map(|p| p.len()).sum::<usize>() > MAX_OUTLINE_POINTS {
        return None;
    }
    let v = Vec2::new(x, y);
    let mut moved_bp = path.to_bezpath();
    moved_bp.apply_affine(Affine::translate(v));
    let moved = PathData::from_bezpath(&moved_bp);
    let mut shapes: Vec<(PathData, FillRule)> = vec![(path.clone(), rule), (moved, rule)];
    for poly in &polys {
        // A CCW polygon's edge sees the sweep when its outward normal (e.y, −e.x) points along it.
        let ccw = signed_area(poly) >= 0.0;
        for a in 0..poly.len() {
            let (a, b) = (poly[a], poly[(a + 1) % poly.len()]);
            let e = b - a;
            let outward = e.y * v.y - e.x * v.x;
            if (ccw && outward > 0.0) || (!ccw && -outward > 0.0) {
                shapes.push((quad(a, b, b + v, a + v), FillRule::NonZero));
            }
        }
    }
    let refs: Vec<(&PathData, FillRule)> = shapes.iter().map(|(p, r)| (p, *r)).collect();
    Some(unite_all(&refs))
}

/// `path`'s subpaths as flattened polygons.
fn flatten_polys(path: &PathData) -> Vec<Vec<Point>> {
    let mut polys = vec![];
    let mut cur: Vec<Point> = vec![];
    vectorcraft_geom::flatten(path.to_bezpath(), 0.1, |el| match el {
        kurbo::PathEl::MoveTo(p) => {
            if cur.len() > 2 {
                polys.push(std::mem::take(&mut cur));
            }
            cur = vec![p];
        }
        kurbo::PathEl::LineTo(p) => cur.push(p),
        _ => {}
    });
    if cur.len() > 2 {
        polys.push(cur);
    }
    polys
}

fn signed_area(poly: &[Point]) -> f64 {
    poly.iter().zip(poly.iter().cycle().skip(1)).map(|(a, b)| a.x * b.y - b.x * a.y).sum::<f64>() / 2.0
}

fn quad(a: Point, b: Point, c: Point, d: Point) -> PathData {
    let mut bp = BezPath::new();
    bp.move_to(a);
    bp.line_to(b);
    bp.line_to(c);
    bp.line_to(d);
    bp.close_path();
    PathData::from_bezpath(&bp)
}
