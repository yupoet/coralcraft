//! CoralCraft (P3 Task E1): the CorelDRAW Symmetry commands. A symmetry object is a live
//! `NodeKind::Repeat` — Corel's **line** kind is `Mirror` (one mirror line), **grid** is the
//! mirrored 2×2 `Grid`, and **radial** is `Radial` with its `mirror` flag (N instances at 2π/N,
//! alternating chirality). The source stays editable; Break dissolves the arrangement to plain
//! copies through the repeat expander.

use serde_json::{Value, json};
use vectorcraft_doc::pattern::{RepeatKind, RepeatSpec};
use vectorcraft_doc::{Document, Node, NodeId, NodeKind};

use super::patterncmds::{expand_repeat, make_repeat};
use super::{CommandSpec, EngineError, Result, Session, bad, cmd, has_doc};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "object.symmetry.create",
            "Create Symmetry",
            ["Object", "Symmetry"],
            None,
            "{ids?, kind: line|grid|radial, lines?: 2..1000 (radial, 5), angle?: deg (line/grid axis, radial start; 90), center?: [x, y] (default: the selection's centre)} wrap the selection in a live symmetry of Corel's kind → {id}",
            has_doc,
            symmetry_create
        ),
        cmd!(
            "object.symmetry.setProps",
            "Symmetry Properties",
            ["Object", "Symmetry"],
            None,
            "{id, angle?, center?, lines?: 2..1000 (radial)} edit the symmetry's arrangement in place, one undo step → {id}",
            has_doc,
            symmetry_set_props
        ),
        cmd!(
            "object.symmetry.break",
            "Break Symmetry",
            ["Object", "Symmetry"],
            None,
            "{id} dissolve the symmetry into its instances as plain editable copies",
            has_doc,
            symmetry_break
        ),
        cmd!(query "object.symmetry.info", "Symmetry Info", [], None, "{id} → {kind: line|grid|radial|repeat, lines?, angle?, center?} (repeat: a repeat that is none of the symmetry kinds)", has_doc, symmetry_info),
    ]
}

/// The centre of the symmetry: the given one, else the selection's bounds centre.
fn center_param(s: &Session, p: &Value) -> Result<vectorcraft_geom::Point> {
    match p.get("center") {
        Some(v) => {
            let c = v.as_array().filter(|a| a.len() == 2).ok_or_else(|| bad("object.symmetry", "center must be [x, y]"))?;
            let (x, y) = (c[0].as_f64(), c[1].as_f64());
            match (x, y) {
                (Some(x), Some(y)) if x.is_finite() && y.is_finite() => Ok(vectorcraft_geom::Point::new(x, y)),
                _ => Err(bad("object.symmetry", "center must be two finite numbers")),
            }
        }
        None => {
            let st = s.doc()?;
            let mut bounds = None;
            for id in &st.selection.objects {
                if let Some(b) = st.doc.node(*id).and_then(|n| n.visual_bounds()) {
                    bounds = Some(bounds.map_or(b, |acc: vectorcraft_geom::Rect| acc.union(b)));
                }
            }
            Ok(bounds.map_or(vectorcraft_geom::Point::new(0.0, 0.0), |b| b.center()))
        }
    }
}

fn symmetry_create(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "object.symmetry.create";
    let kind = p.get("kind").and_then(Value::as_str).ok_or_else(|| bad(C, "missing kind (line|grid|radial)"))?;
    let angle = match p.get("angle") {
        Some(v) => v.as_f64().filter(|a| a.is_finite()).ok_or_else(|| bad(C, "angle must be a number"))?,
        None => 90.0,
    };
    let center = center_param(s, p)?;
    fn spec(lines: Option<u32>, kind: &str, angle: f64, center: vectorcraft_geom::Point) -> impl Fn(Vec<std::sync::Arc<Node>>) -> RepeatSpec {
        move |src| {
            let kind = kind;
            let mut spec = match kind {
                "line" => RepeatSpec { kind: RepeatKind::Mirror { angle, center }, source: src },
                "grid" => RepeatSpec {
                    kind: RepeatKind::Grid {
                        h_spacing: 0.0,
                        v_spacing: 0.0,
                        rows: 2,
                        cols: 2,
                        grid_type: vectorcraft_doc::pattern::TileType::Grid,
                        flip_rows: true,
                        flip_cols: true,
                    },
                    source: src,
                },
                "radial" => {
                    let mut spec = RepeatSpec::radial(src, lines.unwrap_or(5), None);
                    if let RepeatKind::Radial { instances, radius, center: c, reverse_overlap, start_angle, end_angle, mirror } = &mut spec.kind {
                        *instances = lines.unwrap_or(5);
                        let _ = radius;
                        *c = center;
                        *reverse_overlap = false;
                        *start_angle = angle;
                        *end_angle = 360.0;
                        *mirror = true;
                    }
                    spec
                }
                _ => RepeatSpec::radial(src, 5, None),
            };
            if let RepeatKind::Mirror { center: c, .. } = &mut spec.kind {
                *c = center;
            }
            spec
        }
    }
    match kind {
        "line" => make_repeat(s, p, "Create Line Symmetry", spec(None, kind, angle, center)),
        "grid" => make_repeat(s, p, "Create Grid Symmetry", spec(None, kind, angle, center)),
        "radial" => {
            let lines = match p.get("lines") {
                Some(v) => v
                    .as_f64()
                    .and_then(|n| (n.fract() == 0.0).then_some(n as u32))
                    .filter(|n| (2..=1000).contains(n))
                    .ok_or_else(|| bad(C, "lines must be a whole number from 2 to 1000"))?,
                None => 5,
            };
            make_repeat(s, p, "Create Radial Symmetry", spec(Some(lines), kind, angle, center))
        }
        _ => Err(bad(C, format!("unknown symmetry kind {kind:?} (line|grid|radial)"))),
    }
}

fn symmetry_set_props(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "object.symmetry.setProps";
    let id = p.get("id").and_then(Value::as_u64).ok_or_else(|| bad(C, "missing id"))?;
    let id = NodeId(id);
    let angle = match p.get("angle") {
        Some(v) => Some(v.as_f64().filter(|a| a.is_finite()).ok_or_else(|| bad(C, "angle must be a number"))?),
        None => None,
    };
    let center = match p.get("center") {
        Some(_) => Some(center_param(s, p)?),
        None => None,
    };
    let lines = match p.get("lines") {
        Some(v) => Some(
            v.as_f64()
                .and_then(|n| (n.fract() == 0.0).then_some(n as u32))
                .filter(|n| (2..=1000).contains(n))
                .ok_or_else(|| bad(C, "lines must be a whole number from 2 to 1000"))?,
        ),
        None => None,
    };
    s.edit("Symmetry Properties", move |d, _| {
        let n = d.node_mut(id).ok_or(EngineError::NoNode(id))?;
        let NodeKind::Repeat(spec) = &mut n.kind else {
            return Err(EngineError::Other(format!("{C}: {id:?} is not a symmetry object")));
        };
        match &mut spec.kind {
            RepeatKind::Mirror { angle: a, center: c } => {
                if lines.is_some() {
                    return Err(EngineError::Other(format!("{C}: a line symmetry has no lines")));
                }
                if let Some(v) = angle {
                    *a = v;
                }
                if let Some(v) = center {
                    *c = v;
                }
            }
            RepeatKind::Radial { instances, start_angle, center: c, .. } => {
                if let Some(v) = lines {
                    *instances = v;
                }
                if let Some(v) = angle {
                    *start_angle = v;
                }
                if let Some(v) = center {
                    *c = v;
                }
            }
            RepeatKind::Grid { .. } => {
                return Err(EngineError::Other(format!("{C}: a grid symmetry edits through object.repeat.options")));
            }
        }
        Ok(())
    })?;
    Ok(json!({ "id": id.0 }))
}

fn symmetry_break(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "object.symmetry.break";
    let id = p.get("id").and_then(Value::as_u64).ok_or_else(|| bad(C, "missing id"))?;
    let id = NodeId(id);
    s.edit("Break Symmetry", move |d, _| {
        if !matches!(d.node(id).map(|n| &n.kind), Some(NodeKind::Repeat(_))) {
            return Err(EngineError::Other(format!("{C}: {id:?} is not a symmetry object")));
        }
        expand_repeat(d, id)
    })?;
    Ok(json!({ "id": id.0 }))
}

fn symmetry_info(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "object.symmetry.info";
    let id = p.get("id").and_then(Value::as_u64).ok_or_else(|| bad(C, "missing id"))?;
    let st = s.doc()?;
    let Some(node) = st.doc.node(NodeId(id)) else { return Err(EngineError::NoNode(NodeId(id))) };
    let NodeKind::Repeat(spec) = &node.kind else {
        return Err(EngineError::Other(format!("{C}: {id:?} is not a symmetry object")));
    };
    let (kind, lines, angle, center) = match &spec.kind {
        RepeatKind::Mirror { angle, center } => ("line", None, Some(*angle), Some([center.x, center.y])),
        RepeatKind::Grid { flip_rows, flip_cols, .. } if *flip_rows && *flip_cols => ("grid", None, None, None),
        RepeatKind::Radial { instances, mirror: true, start_angle, center, .. } => {
            ("radial", Some(*instances), Some(*start_angle), Some([center.x, center.y]))
        }
        _ => ("repeat", None, None, None),
    };
    Ok(json!({ "kind": kind, "lines": lines, "angle": angle, "center": center }))
}

/// A plain `Document` import silence (the helper above uses it through `s.edit`).
#[allow(unused)]
fn _doc(_: &Document) {}
