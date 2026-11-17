//! CoralCraft (P3 Task E1): CorelDRAW Symmetry over the synced Repeat architecture — a symmetry
//! object is a live Repeat (line = Mirror, grid = mirrored 2×2 Grid, radial = Mirrored Radial),
//! created, edited and broken through Corel-facing commands, proven by native persistence,
//! independent SVG/PDF reads, the named CDR/CMX expansion warning, and the real rendered
//! controls (Object menu + Property Bar row). The matrix in `docs/p3-inventory.md` §24 freezes
//! this slice's semantics.

use serde_json::json;
use vectorcraft_doc::{NodeId, NodeKind};
use vectorcraft_engine::Session;

/// An asymmetric source (all art on one side of x = 50), so a mirrored copy is tellable from a
/// rotated one by its bounds.
fn asymmetric_source(s: &mut Session) -> u64 {
    s.execute("shape.rectangle", &json!({ "x": 60, "y": 40, "width": 30, "height": 20 })).unwrap()["id"].as_u64().unwrap()
}

fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", &json!({ "width": 400, "height": 300 })).unwrap();
    s
}

fn is_repeat(s: &Session, id: u64) -> bool {
    matches!(s.doc().unwrap().doc.node(NodeId(id)).map(|n| &n.kind), Some(NodeKind::Repeat(_)))
}

fn instances(s: &Session, id: u64) -> usize {
    match s.doc().unwrap().doc.node(NodeId(id)).map(|n| &n.kind) {
        Some(NodeKind::Repeat(r)) => r.instances(Some(&s.doc().unwrap().doc)),
        _ => 0,
    }
}

/// P1–P2: every kind creates its mapped arrangement; radial alternates chirality.
#[test]
fn every_symmetry_kind_creates_its_mapped_arrangement() {
    // Line: one mirror line → 2 instances.
    let mut s = session();
    let art = asymmetric_source(&mut s);
    s.execute("select.set", &json!({ "ids": [art] })).unwrap();
    let made = s.execute("object.symmetry.create", &json!({ "kind": "line", "angle": 90.0 })).unwrap();
    let id = made["id"].as_u64().unwrap();
    assert!(is_repeat(&s, id), "a live Repeat");
    assert_eq!(instances(&s, id), 2, "the source and its mirror");
    let info = s.execute("object.symmetry.info", &json!({ "id": id })).unwrap();
    assert_eq!(info["kind"].as_str(), Some("line"), "the info names the kind: {info:?}");
    assert_eq!(info["angle"].as_f64(), Some(90.0));

    // Grid: mirrored 2×2 → 4 instances.
    let mut s = session();
    let art = asymmetric_source(&mut s);
    s.execute("select.set", &json!({ "ids": [art] })).unwrap();
    let made = s.execute("object.symmetry.create", &json!({ "kind": "grid" })).unwrap();
    let id = made["id"].as_u64().unwrap();
    assert_eq!(instances(&s, id), 4, "four mirrored quadrants");
    let info = s.execute("object.symmetry.info", &json!({ "id": id })).unwrap();
    assert_eq!(info["kind"].as_str(), Some("grid"), "{info:?}");

    // Radial: N lines → N instances with alternating chirality (P2).
    let mut s = session();
    let art = asymmetric_source(&mut s);
    s.execute("select.set", &json!({ "ids": [art] })).unwrap();
    let made = s.execute("object.symmetry.create", &json!({ "kind": "radial", "lines": 5, "center": [50.0, 50.0] })).unwrap();
    let id = made["id"].as_u64().unwrap();
    assert_eq!(instances(&s, id), 5, "one instance per line");
    let NodeKind::Repeat(r) = &s.doc().unwrap().doc.node(NodeId(id)).unwrap().kind else { panic!("repeat") };
    let transforms = r.transforms(Some(&s.doc().unwrap().doc));
    assert_eq!(transforms.len(), 5);
    // Chirality by signed area: a rotation keeps the winding of the source's rectangle, a
    // reflection flips it — odd instances must flip (alternating kaleidoscope chirality), even
    // ones must not.
    let source = r.source_bounds().unwrap();
    let corners = [source.origin(), (source.x1, source.y0).into(), (source.x1, source.y1).into(), (source.x0, source.y1).into()];
    let signed = |pts: [vectorcraft_geom::Point; 4]| pts.iter().zip(pts.iter().cycle().skip(1)).map(|(a, b)| a.x * b.y - b.x * a.y).sum::<f64>();
    for (k, m) in transforms.iter().enumerate() {
        let wound = signed(corners.map(|p| *m * p));
        assert_eq!(
            wound.signum() >= 0.0,
            (k % 2 == 0) == (signed(corners).signum() >= 0.0),
            "instance {k} has {} chirality (winding {wound})",
            if k % 2 == 0 { "even" } else { "flipped" }
        );
    }
    let info = s.execute("object.symmetry.info", &json!({ "id": id })).unwrap();
    assert_eq!((info["kind"].as_str(), info["lines"].as_u64()), (Some("radial"), Some(5)), "{info:?}");
}

/// P3–P5: edits, undo/redo, native reopen, and the source keeps driving every instance.
#[test]
fn symmetry_edits_undo_reopens_and_the_source_drives_the_instances() {
    let mut s = session();
    let art = asymmetric_source(&mut s);
    s.execute("select.set", &json!({ "ids": [art] })).unwrap();
    let id = s.execute("object.symmetry.create", &json!({ "kind": "radial", "lines": 4, "center": [50.0, 50.0] })).unwrap()["id"].as_u64().unwrap();
    // P3: each parameter edit is one undo step and round-trips.
    let undo_count = s.active().unwrap().history.undo.len();
    s.execute("object.symmetry.setProps", &json!({ "id": id, "lines": 6 })).unwrap();
    assert_eq!(instances(&s, id), 6, "lines edits the arrangement");
    assert_eq!(s.active().unwrap().history.undo.len(), undo_count + 1, "one step");
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(instances(&s, id), 4, "undo restores the lines");
    s.execute("edit.redo", &json!({})).unwrap();
    assert_eq!(instances(&s, id), 6);
    s.execute("object.symmetry.setProps", &json!({ "id": id, "center": [120.0, 50.0] })).unwrap();
    let info = s.execute("object.symmetry.info", &json!({ "id": id })).unwrap();
    assert_eq!(info["center"].as_array().and_then(|c| Some((c[0].as_f64()?, c[1].as_f64()?))), Some((120.0, 50.0)), "{info:?}");
    s.execute("edit.undo", &json!({})).unwrap();
    let info = s.execute("object.symmetry.info", &json!({ "id": id })).unwrap();
    assert_ne!(info["center"].as_array().and_then(|c| c[0].as_f64()), Some(120.0), "undo restored the centre");
    // P4: native reopen keeps it, and it still edits after.
    let data = s.execute("document.serialize", &json!({ "format": "vectorcraft" })).unwrap()["dataBase64"].as_str().unwrap().to_string();
    s.execute("document.open", &json!({ "dataBase64": data })).unwrap();
    assert_eq!(instances(&s, id), 6, "reopen keeps the symmetry");
    s.execute("object.symmetry.setProps", &json!({ "id": id, "angle": 45.0 })).unwrap();
    let info = s.execute("object.symmetry.info", &json!({ "id": id })).unwrap();
    assert_eq!(info["angle"].as_f64(), Some(45.0), "still edits after reopen");
    // P5: editing the source reaches every instance.
    let NodeKind::Repeat(r) = &s.doc().unwrap().doc.node(NodeId(id)).unwrap().kind else { panic!("repeat") };
    let source_id = r.source[0].id;
    let paint = s.execute("paint.setFill", &json!({ "id": source_id.0, "color": "#20a040" })).unwrap();
    assert!(paint.is_object(), "the source is editable in place");
    let svg = s.execute("document.serialize", &json!({ "format": "svg" })).unwrap()["text"].as_str().unwrap().to_string();
    assert!(svg.matches("#20a040").count() >= 4, "the source edit reaches every instance: {svg}");
}

/// P6: Break dissolves to plain copies.
#[test]
fn breaking_a_symmetry_dissolves_to_plain_copies() {
    let mut s = session();
    let art = asymmetric_source(&mut s);
    s.execute("select.set", &json!({ "ids": [art] })).unwrap();
    let id = s.execute("object.symmetry.create", &json!({ "kind": "grid" })).unwrap()["id"].as_u64().unwrap();
    s.execute("object.symmetry.break", &json!({ "id": id })).unwrap();
    assert!(!is_repeat(&s, id), "no Repeat left");
    let node = s.doc().unwrap().doc.node(NodeId(id)).unwrap();
    let children = node.children().map(|c| c.len()).unwrap_or(0);
    assert_eq!(children, 4, "the four instances as plain copies");
}

/// P7 + P9: independent reads and the named CDR/CMX degradation.
#[test]
fn symmetry_reads_back_independently_and_names_its_cdr_degradation() {
    let mut s = session();
    let art = asymmetric_source(&mut s);
    s.execute("select.set", &json!({ "ids": [art] })).unwrap();
    s.execute("object.symmetry.create", &json!({ "kind": "radial", "lines": 5, "center": [50.0, 50.0] })).unwrap();
    // Independent usvg: five instance rectangles.
    let svg = s.execute("document.serialize", &json!({ "format": "svg" })).unwrap()["text"].as_str().unwrap().to_string();
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).expect("independent usvg parse");
    let mut rects = 0usize;
    count_paths(tree.root(), &mut rects);
    assert!(rects >= 5, "the five instances export as real geometry: {rects}");
    // Independent Hayro-backed PDF read.
    let pdf =
        vectorcraft_format::base64_decode(s.execute("document.serialize", &json!({ "format": "pdf" })).unwrap()["dataBase64"].as_str().unwrap())
            .unwrap();
    let imported = vectorcraft_pdf::import(&pdf).expect("independent Hayro-backed PDF read");
    let mut paths = 0;
    imported.walk(|n| {
        if n.path_data().is_some_and(|p| !p.is_empty()) {
            paths += 1;
        }
    });
    assert!(paths >= 5, "the PDF read returns the instances: {paths}");
    // P9: exactly one named live-Repeat expansion warning in CDR and CMX.
    for format in ["cdr", "cmx"] {
        let out = s.execute("document.serialize", &json!({ "format": format })).unwrap();
        let warnings = out["warnings"].as_array().cloned().unwrap_or_default();
        let count = warnings
            .iter()
            .filter(|w| {
                w.as_str().is_some_and(|w| {
                    w.ends_with("Repeat objects are expanded to plain vector groups in CDR/CMX; the native document keeps the editable live sources")
                })
            })
            .count();
        assert_eq!(count, 1, "one named expansion warning ({format}): {warnings:?}");
    }
}

/// N1 + N2: refusals change nothing.
#[test]
fn symmetry_refusals_change_nothing() {
    let mut s = session();
    let art = asymmetric_source(&mut s);
    s.execute("select.set", &json!({ "ids": [art] })).unwrap();
    let before = s.execute("document.serialize", &json!({ "format": "vectorcraft" })).unwrap()["dataBase64"].as_str().unwrap().to_string();
    for bad in [
        json!({ "kind": "kaleidoscope" }),
        json!({ "kind": "radial", "lines": 1 }),
        json!({ "kind": "radial", "lines": 1001 }),
        json!({ "kind": "line", "angle": "high" }),
    ] {
        assert!(s.execute("object.symmetry.create", &bad).is_err(), "accepted {bad:?}");
    }
    assert!(s.execute("object.symmetry.setProps", &json!({ "id": art, "lines": 5 })).is_err(), "setProps on an ordinary object");
    assert!(s.execute("object.symmetry.break", &json!({ "id": art })).is_err(), "break on an ordinary object");
    let after = s.execute("document.serialize", &json!({ "format": "vectorcraft" })).unwrap()["dataBase64"].as_str().unwrap().to_string();
    assert_eq!(before, after, "the refused calls changed nothing");
}

fn count_paths(group: &usvg::Group, out: &mut usize) {
    for node in group.children() {
        match node {
            usvg::Node::Group(child) => count_paths(child, out),
            usvg::Node::Path(path) if !path.data().is_empty() => *out += 1,
            _ => {}
        }
    }
}
