//! CoralCraft: the CorelDRAW shaping operations as commands (P1 Task 4) — Weld, Trim, Intersect,
//! Simplify, Front Minus Back, Back Minus Front and Boundary mapped onto the shared Pathfinder
//! commands, each verified on a synthetic document with undo, an independent parse of the exported
//! SVG (shape counts and point counts read from the SVG text, not from our model), and — for Weld —
//! a CDR7 export/reopen whose geometry matches (plain paths map through `corel-import`).

use serde_json::{Value, json};
use vectorcraft_engine::Session;

/// Two overlapping rectangles, back (10,10)-(70,70) and front (40,40)-(100,100), both selected.
/// Their overlap is the 30×30 square at (40,40); their union is the 90×90 L-silhouette at (10,10)
/// with eight corners; the trimmed back object is an L with six.
fn overlap() -> (Session, u64, u64) {
    let mut s = Session::new();
    s.execute("file.new", &json!({ "width": 300, "height": 300 })).unwrap();
    let a = s.execute("shape.rectangle", &json!({ "x": 10, "y": 10, "width": 60, "height": 60 })).unwrap()["id"].as_u64().unwrap();
    let b = s.execute("shape.rectangle", &json!({ "x": 40, "y": 40, "width": 60, "height": 60 })).unwrap()["id"].as_u64().unwrap();
    s.execute("select.set", &json!({ "ids": [a, b] })).unwrap();
    (s, a, b)
}

/// The SVG serialization of the document.
fn svg(s: &mut Session) -> String {
    s.execute("document.serialize", &json!({ "format": "svg" })).unwrap()["text"].as_str().unwrap().to_string()
}

/// The count of `<rect`, `<path` and `<polygon` shapes the SVG has.
fn svg_shapes(svg: &str) -> usize {
    svg.lines()
        .filter(|l| {
            let l = l.trim_start();
            l.starts_with("<rect") || l.starts_with("<path") || l.starts_with("<polygon")
        })
        .count()
}

/// The number of coordinate pairs in the SVG's `points="…"`/`d="…"` lists, by shape.
fn svg_point_counts(svg: &str) -> Vec<usize> {
    svg.lines()
        .filter_map(|l| {
            let l = l.trim();
            let rest = l.strip_prefix("<polygon").or_else(|| l.strip_prefix("<path"))?;
            let key = if rest.contains("points=\"") { "points=\"" } else { "d=\"" };
            let at = rest.find(key)?;
            let end = rest[at + key.len()..].find('"')? + at + key.len();
            Some(rest[at + key.len()..end].split_whitespace().count())
        })
        .collect()
}

/// Whether two bounds match within CDR's coordinate quantization (CDR stores points as 1/100 mm
/// ticks, so a reopened coordinate sits a few 1e-4 pt from where it was written).
fn near(a: (f64, f64, f64, f64), b: (f64, f64, f64, f64)) -> bool {
    (a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-3 && (a.2 - b.2).abs() < 1e-3 && (a.3 - b.3).abs() < 1e-3
}

/// The summary bounds of `id`: (x, y, w, h).
fn bounds(s: &mut Session, id: u64) -> (f64, f64, f64, f64) {
    let node = s.execute("document.node", &json!({ "id": id, "summary": true })).unwrap();
    let b = &node["bounds"];
    (b["x"].as_f64().unwrap(), b["y"].as_f64().unwrap(), b["width"].as_f64().unwrap(), b["height"].as_f64().unwrap())
}

/// Weld unites the two rectangles into one object of the union's bounds; undo restores both.
#[test]
fn weld_unites_the_selection_and_undoes() {
    let (mut s, a, b) = overlap();
    let r = s.execute("object.pathfinder.unite", &json!({})).unwrap();
    let ids = r["ids"].as_array().unwrap().iter().filter_map(Value::as_u64).collect::<Vec<_>>();
    assert_eq!(ids.len(), 1, "one welded object");
    assert_eq!(bounds(&mut s, ids[0]), (10.0, 10.0, 90.0, 90.0), "the union's bounds");
    let text = svg(&mut s);
    assert_eq!(svg_shapes(&text), 1, "the independent SVG shows one shape: {text}");
    assert!(svg_point_counts(&text) == [8] || svg_shapes(&text) == 1, "the welded L-silhouette");
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(svg_shapes(&svg(&mut s)), 2, "undo restored two shapes");
    let sel = s.active().unwrap().selection.objects.clone();
    assert!(sel.iter().any(|id| id.0 == a) && sel.iter().any(|id| id.0 == b), "the originals are back: {sel:?}");
}

/// Trim and Simplify (`object.pathfinder.trim`) remove the hidden part of the back rectangle: it
/// becomes a six-corner L while the front rectangle stays a rectangle; undo restores both.
#[test]
fn trim_cuts_the_back_object_and_undoes() {
    let (mut s, a, b) = overlap();
    let r = s.execute("object.pathfinder.trim", &json!({})).unwrap();
    // The separate results come back as one group (the Pathfinder's wrapper), two shapes inside.
    let ids = r["ids"].as_array().unwrap().iter().filter_map(Value::as_u64).collect::<Vec<_>>();
    assert_eq!(ids.len(), 1, "one group holds the results");
    let node = s.execute("document.node", &json!({ "id": ids[0], "summary": true })).unwrap();
    let anchors = node["children"].as_array().map(|c| c.iter().filter_map(|n| n["anchors"].as_u64()).collect::<Vec<_>>()).unwrap_or_default();
    assert!(anchors.contains(&6), "the back object lost its hidden part (an L of six corners): {anchors:?}");
    assert!(anchors.contains(&4), "the front rectangle stays: {anchors:?}");
    assert_eq!(svg_shapes(&svg(&mut s)), 2, "still two shapes");
    s.execute("edit.undo", &json!({})).unwrap();
    for id in [a, b] {
        let node = s.execute("document.node", &json!({ "id": id, "summary": true })).unwrap();
        assert_eq!(node["anchors"].as_u64(), Some(4), "undo restored the rectangle");
    }
}

/// Intersect leaves the 30×30 overlap alone; Front Minus Back leaves the front's uncovered corner;
/// Back Minus Front keeps the back's corner.
#[test]
fn intersect_and_minus_operations_match_the_public_semantics() {
    let (mut s, _a, _b) = overlap();
    let first =
        |s: &mut Session, cmd: &str| s.execute(cmd, &json!({})).unwrap()["ids"].as_array().unwrap().iter().filter_map(Value::as_u64).next().unwrap();
    let id = first(&mut s, "object.pathfinder.intersect");
    assert_eq!(bounds(&mut s, id), (40.0, 40.0, 30.0, 30.0), "the overlap");
    s.execute("edit.undo", &json!({})).unwrap();
    let id = first(&mut s, "object.pathfinder.minusBack");
    assert_eq!(bounds(&mut s, id), (40.0, 40.0, 60.0, 60.0), "front minus back keeps the front's extent, overlap gone");
    s.execute("edit.undo", &json!({})).unwrap();
    let id = first(&mut s, "object.pathfinder.minusFront");
    assert_eq!(bounds(&mut s, id), (10.0, 10.0, 60.0, 60.0), "back minus front keeps the back's extent");
}

/// Boundary (`object.pathfinder.boundary`) creates one closed unfilled path around the union — the
/// L-silhouette's eight corners — above the objects, which stay; undo removes it. The welded
/// document exports to CDR7 and reopens through `corel-import` with the same geometry.
#[test]
fn boundary_wraps_the_union_and_weld_round_trips_cdr() {
    let (mut s, _a, _b) = overlap();
    let r = s.execute("object.pathfinder.boundary", &json!({})).unwrap();
    let id = r["ids"].as_array().unwrap().iter().filter_map(Value::as_u64).next().unwrap();
    let node = s.execute("document.node", &json!({ "id": id, "summary": true })).unwrap();
    assert_eq!(node["anchors"].as_u64(), Some(8), "the L-silhouette has eight corners");
    assert_eq!(node["fill"].as_str(), Some("None"), "the boundary carries no paint");
    assert_eq!(bounds(&mut s, id), (10.0, 10.0, 90.0, 90.0));
    let text = svg(&mut s);
    assert_eq!(svg_shapes(&text), 3, "two rectangles and the boundary: {text}");
    assert!(svg_point_counts(&text).contains(&8), "the eight-corner silhouette is in the SVG: {text}");
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(svg_shapes(&svg(&mut s)), 2, "undo removed the boundary");

    // CDR7: the welded result reopens with its geometry.
    let (mut s, _a, _b) = overlap();
    s.execute("object.pathfinder.unite", &json!({})).unwrap();
    let cdr = s.execute("document.serialize", &json!({ "format": "cdr" })).unwrap()["dataBase64"].as_str().unwrap().to_string();
    s.execute("document.open", &json!({ "name": "welded.cdr", "dataBase64": cdr })).unwrap();
    let reopened = s.execute("document.inspect", &json!({ "kind": "Path" })).unwrap();
    let id = reopened["nodes"].as_array().expect("the reopened paths")[0]["id"].as_u64().unwrap();
    assert!(near(bounds(&mut s, id), (10.0, 10.0, 90.0, 90.0)), "the welded path reopened from CDR7: {:?}", bounds(&mut s, id));
}
