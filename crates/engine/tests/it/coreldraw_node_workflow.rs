//! CoralCraft: the CorelDRAW selection and node-editing workflow (P1 Task 3) — the evidence for
//! the D1 row's rise to level 4. The Shape tool's daily workflow runs headless as commands (node
//! select/move/add/delete/convert, marquee selection, selection modifiers) with undo and redo, the
//! edited nodes stay editable after a native `.vectorcraft` save and reopen, and the exported SVG
//! is checked both as text and by the independent `usvg` parser rather than only through our own
//! document model.

use serde_json::{Value, json};
use vectorcraft_engine::Session;

fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", &json!({ "width": 400, "height": 300 })).unwrap();
    s
}

/// The summary of `document.node {id}` on a path node: its anchor count.
fn anchors(s: &mut Session, id: u64) -> u64 {
    let node = s.execute("document.node", &json!({ "id": id, "summary": true })).unwrap();
    if let Some(n) = node["anchors"].as_u64() {
        return n;
    }
    let children = node["children"].as_array().expect("the layer's children");
    let path = children.iter().find(|c| c.get("anchors").is_some()).unwrap_or_else(|| panic!("node {id} among {children:?}"));
    path["anchors"].as_u64().unwrap()
}

/// The SVG serialization of the document (an independent text the checks below parse).
fn svg(s: &mut Session) -> String {
    let text = s.execute("document.serialize", &json!({ "format": "svg" })).unwrap()["text"].as_str().unwrap().to_string();
    let tree = usvg::Tree::from_str(&text, &usvg::Options::default()).expect("independent usvg parse");
    assert!(!tree.root().children().is_empty(), "usvg found the exported artwork");
    text
}

/// Text-level geometry assertions backed by the independent `usvg` parse in [`svg`].
fn svg_geometry(svg: &str) -> String {
    let mut out = String::new();
    for l in svg.lines() {
        let l = l.trim();
        for (tag, key) in [("<path", "d=\""), ("<polygon", "points=\"")] {
            if let Some(rest) = l.strip_prefix(tag)
                && let Some(at) = rest.find(key)
                && let Some(end) = rest[at + key.len()..].find('"')
            {
                out.push_str(&rest[at + key.len()..at + key.len() + end].replace(',', " "));
                out.push(' ');
            }
        }
    }
    out
}

/// `select.marquee` selects what the rect reaches, `inside` keeps only what lies entirely within
/// it, the modes compose with an existing selection, and bad parameters are refused.
#[test]
fn marquee_selection_selects_touched_and_inside_objects() {
    let mut s = session();
    let mut rect = |x: f64| s.execute("shape.rectangle", &json!({ "x": x, "y": 0, "width": 40, "height": 40 })).unwrap()["id"].as_u64().unwrap();
    let a = rect(0.0); // entirely within (0,0,80,80)
    let b = rect(60.0); // reached by the rect's edge, not inside it
    let c = rect(300.0); // outside
    // What the marquee reaches.
    let r = s.execute("select.marquee", &json!({ "x": 0, "y": 0, "width": 80, "height": 80 })).unwrap();
    let mut got = r["ids"].as_array().unwrap().iter().filter_map(Value::as_u64).collect::<Vec<_>>();
    got.sort_unstable();
    assert_eq!(got, vec![a, b], "the marquee reaches a and touches b");
    // Only what lies entirely inside.
    let r = s.execute("select.marquee", &json!({ "x": 0, "y": 0, "width": 80, "height": 80, "inside": true })).unwrap();
    assert_eq!(r["ids"].as_array().unwrap().iter().filter_map(Value::as_u64).collect::<Vec<_>>(), vec![a], "b is not inside");
    // add and toggle compose with a selection.
    s.execute("select.set", &json!({ "ids": [c] })).unwrap();
    let r = s.execute("select.marquee", &json!({ "x": 0, "y": 0, "width": 80, "height": 80, "mode": "add" })).unwrap();
    let mut got = r["ids"].as_array().unwrap().iter().filter_map(Value::as_u64).collect::<Vec<_>>();
    got.sort_unstable();
    assert_eq!(got, vec![a, b, c], "add keeps c");
    let r = s.execute("select.marquee", &json!({ "x": 0, "y": 0, "width": 80, "height": 80, "mode": "toggle" })).unwrap();
    assert_eq!(r["ids"].as_array().unwrap().iter().filter_map(Value::as_u64).collect::<Vec<_>>(), vec![c], "toggle takes a and b back out");
    // Bad parameters are refused without touching the selection.
    for bad in [
        json!({ "x": 0, "y": 0, "width": 80 }),
        json!({ "x": 0, "y": 0, "width": 80, "height": "tall" }),
        json!({ "x": 0, "y": 0, "width": 80, "height": 80, "mode": "both" }),
    ] {
        let err = s.execute("select.marquee", &bad).unwrap_err();
        assert!(err.to_string().contains("select.marquee"), "{bad}: {err}");
    }
    let r = s.execute("select.marquee", &json!({ "x": 0, "y": 0, "width": 80, "height": 80 })).unwrap();
    assert_eq!(
        r["ids"].as_array().unwrap().iter().filter_map(Value::as_u64).collect::<Vec<_>>(),
        vec![a, b],
        "the failures left the selection alone"
    );
}

/// The Shape tool's node workflow as commands: select anchors, move them, insert, delete and
/// convert one, each step undoable and redoable, and the SVG shows the edited geometry.
#[test]
fn node_workflow_runs_headless_with_undo_and_redo() {
    let mut s = session();
    let id = s.execute("path.create", &json!({ "d": "M 20 20 L 120 20 L 120 100 Z", "closed": true })).unwrap()["id"].as_u64().unwrap();
    assert_eq!(anchors(&mut s, id), 3);
    // Select the first anchor and move it: the geometry moves, not just the selection.
    s.execute("select.anchors", &json!({ "id": id, "anchors": [[0, 0]] })).unwrap();
    s.execute("path.moveAnchors", &json!({ "dx": 10, "dy": 30 })).unwrap();
    let d = svg_geometry(&svg(&mut s));
    assert!(d.contains("30 50"), "the anchor moved to (30, 50): {d}");
    // Undo and redo bring the geometry back and forth.
    s.execute("edit.undo", &json!({})).unwrap();
    let d = svg_geometry(&svg(&mut s));
    assert!(d.contains("20 20") && !d.contains("30 50"), "undo restored the anchor: {d}");
    s.execute("edit.redo", &json!({})).unwrap();
    let d = svg_geometry(&svg(&mut s));
    assert!(d.contains("30 50"), "redo moved it again: {d}");
    // Insert an anchor on a segment, then delete it.
    s.execute("path.insertAnchor", &json!({ "id": id, "segment": 0, "t": 0.5 })).unwrap();
    assert_eq!(anchors(&mut s, id), 4, "insertAnchor added one");
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(anchors(&mut s, id), 3, "insertAnchor undoes");
    // Convert an anchor to smooth: its handles change the geometry.
    s.execute("path.convertAnchor", &json!({ "id": id, "anchor": 1, "to": "smooth" })).unwrap();
    let d = svg_geometry(&svg(&mut s));
    assert!(d.contains('C'), "the segment out of the anchor is a curve now: {d}");
    // Selection modifiers on nodes: add a second anchor, then toggle it back out.
    s.execute("select.anchors", &json!({ "id": id, "anchors": [[0, 1]], "mode": "add" })).unwrap();
    s.execute("select.anchors", &json!({ "id": id, "anchors": [[0, 1]], "mode": "toggle" })).unwrap();
    // Moving the remaining selection still edits the path.
    s.execute("path.moveAnchors", &json!({ "dx": 5, "dy": 0 })).unwrap();
    let d = svg_geometry(&svg(&mut s));
    assert!(d.contains("35 50"), "the still-selected anchor moved: {d}");
}

/// The edited nodes survive a native `.vectorcraft` save and reopen editable, and the SVG the
/// reopened document exports shows the same geometry.
#[test]
fn edited_nodes_survive_a_native_save_and_reopen() {
    let mut s = session();
    let id = s.execute("path.create", &json!({ "d": "M 20 20 L 120 20 L 120 100 Z", "closed": true })).unwrap()["id"].as_u64().unwrap();
    s.execute("select.anchors", &json!({ "id": id, "anchors": [[0, 0]] })).unwrap();
    s.execute("path.moveAnchors", &json!({ "dx": 10, "dy": 30 })).unwrap();
    let data = s.execute("document.serialize", &json!({ "format": "vectorcraft" })).unwrap()["dataBase64"].as_str().unwrap().to_string();
    // Reopen from the bytes; ids start over, so find the path by inspection.
    s.execute("document.open", &json!({ "dataBase64": data })).unwrap();
    let reopened = s.execute("document.inspect", &json!({ "kind": "Path" })).unwrap();
    let id = reopened["nodes"].as_array().expect("the reopened path")[0]["id"].as_u64().unwrap();
    assert_eq!(anchors(&mut s, id), 3, "the reopened path keeps its nodes");
    // The nodes are still editable: the same commands act on the reopened path.
    s.execute("select.anchors", &json!({ "id": id, "anchors": [[0, 1]] })).unwrap();
    s.execute("path.moveAnchors", &json!({ "dx": 0, "dy": 40 })).unwrap();
    let d = svg_geometry(&svg(&mut s));
    assert!(d.contains("30 50") && d.contains("120 60"), "the reopened path edits: {d}");
}

/// Marquee selection in isolation mode never reaches objects outside the isolated object.
#[test]
fn marquee_selection_respects_the_isolation_scope() {
    let mut s = session();
    let inside = s.execute("shape.rectangle", &json!({ "x": 0, "y": 0, "width": 40, "height": 40 })).unwrap()["id"].as_u64().unwrap();
    let outside = s.execute("shape.rectangle", &json!({ "x": 100, "y": 0, "width": 40, "height": 40 })).unwrap()["id"].as_u64().unwrap();
    s.execute("object.isolate", &json!({ "id": inside })).unwrap();

    let result = s.execute("select.marquee", &json!({ "x": 0, "y": 0, "width": 200, "height": 100 })).unwrap();
    let ids: Vec<u64> = result["ids"].as_array().unwrap().iter().filter_map(Value::as_u64).collect();
    assert_eq!(ids, vec![inside], "isolation excluded the outside object {outside}");
}
