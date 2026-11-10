//! CoralCraft: the CorelDRAW transform and Align & Distribute behaviours over the shared commands
//! (P1 Task 6) — the distribute modes on a three-object row verified by an independent parse of the
//! exported SVG (the x positions read from the text, not from our model), and a transformed
//! composition (rotated, mirrored, distributed) that survives a native `.vectorcraft` save and
//! reopen with its geometry.

use serde_json::json;
use vectorcraft_engine::Session;

fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", &json!({ "width": 400, "height": 300 })).unwrap();
    s
}

/// Three rectangles of the same size at x = 10, 120, 260 (uneven gaps 60 and 80).
fn row(s: &mut Session) -> Vec<u64> {
    [10u64, 120, 260]
        .into_iter()
        .map(|x| s.execute("shape.rectangle", &json!({ "x": x, "y": 50, "width": 50, "height": 30 })).unwrap()["id"].as_u64().unwrap())
        .collect()
}

/// The left edges of the SVG's `<rect>` shapes, in order.
fn svg_x_positions(s: &mut Session) -> Vec<f64> {
    let text = s.execute("document.serialize", &json!({ "format": "svg" })).unwrap()["text"].as_str().unwrap().to_string();
    let tree = usvg::Tree::from_str(&text, &usvg::Options::default()).expect("independent usvg parse");
    fn visit(node: &usvg::Node, xs: &mut Vec<f64>) {
        match node {
            usvg::Node::Group(group) => group.children().iter().for_each(|child| visit(child, xs)),
            _ => xs.push(f64::from(node.abs_bounding_box().left())),
        }
    }
    let mut xs = vec![];
    tree.root().children().iter().for_each(|node| visit(node, &mut xs));
    xs
}

/// `object.distribute` spaces the row evenly (the public guide's Distribute: the centres evenly
/// apart), each mode landing its own edge; `object.distributeSpacing` spaces by gaps; undo restores.
#[test]
fn distribute_modes_space_the_row_evenly() {
    let mut s = session();
    let ids = row(&mut s);
    s.execute("select.set", &json!({ "ids": ids })).unwrap();
    // Distribute the centres: 10..310 spans 300, so the centres sit 150 apart — the left edges
    // 10, 130, 250 (each rect 50 wide, centres 35/155/275… evenly spaced means edges 10,130,250).
    s.execute("object.distribute", &json!({ "horizontal": "center" })).unwrap();
    let mut xs = svg_x_positions(&mut s);
    xs.sort_by(|a, b| a.total_cmp(b));
    let gaps: Vec<f64> = xs.windows(2).map(|w| w[1] - w[0]).collect();
    assert!((gaps[0] - gaps[1]).abs() < 1e-6, "even gaps: {xs:?} {gaps:?}");
    // Distribute the left edges: the extremes stay (10 and 260) and the middle lands midway.
    s.execute("edit.undo", &json!({})).unwrap();
    s.execute("object.distribute", &json!({ "horizontal": "left" })).unwrap();
    let mut xs = svg_x_positions(&mut s);
    xs.sort_by(|a, b| a.total_cmp(b));
    assert!((xs[0] - 10.0).abs() < 1e-6 && (xs[2] - 260.0).abs() < 1e-6 && (xs[1] - 135.0).abs() < 1e-6, "left edges evenly spaced: {xs:?}");
    // Distribute Spacing: the gaps between the objects are even (60, 60).
    s.execute("edit.undo", &json!({})).unwrap();
    s.execute("object.distributeSpacing", &json!({ "axis": "horizontal" })).unwrap();
    let mut xs = svg_x_positions(&mut s);
    xs.sort_by(|a, b| a.total_cmp(b));
    let gaps: Vec<f64> = xs.windows(2).map(|w| w[1] - w[0]).collect();
    assert!((gaps[0] - gaps[1]).abs() < 1e-6, "even gaps after Distribute Spacing: {xs:?}");
}

/// A transformed composition — one object rotated, one mirrored, the row distributed — keeps its
/// geometry through a native `.vectorcraft` save and reopen, and the SVG export shows the rotation.
#[test]
fn transformed_composition_survives_a_native_reopen() {
    let mut s = session();
    let ids = row(&mut s);
    s.execute("select.set", &json!({ "ids": [ids[0]] })).unwrap();
    s.execute("object.rotate", &json!({ "angle": 30.0 })).unwrap();
    s.execute("select.set", &json!({ "ids": [ids[1]] })).unwrap();
    s.execute("object.reflect", &json!({ "axis": "vertical" })).unwrap();
    s.execute("select.set", &json!({ "ids": ids })).unwrap();
    s.execute("object.distribute", &json!({ "horizontal": "center" })).unwrap();
    // The SVG shows the rotation (a turned rect is a <polygon>, not a <rect>).
    let svg = s.execute("document.serialize", &json!({ "format": "svg" })).unwrap()["text"].as_str().unwrap().to_string();
    assert!(svg.contains("<polygon") || svg.contains("<path"), "the rotated rect is path geometry: {svg}");
    // Native reopen keeps the transformed geometry.
    let before: Vec<_> = svg_x_positions(&mut s);
    let data = s.execute("document.serialize", &json!({ "format": "vectorcraft" })).unwrap()["dataBase64"].as_str().unwrap().to_string();
    s.execute("document.open", &json!({ "dataBase64": data })).unwrap();
    let reopened = s.execute("document.serialize", &json!({ "format": "svg" })).unwrap()["text"].as_str().unwrap().to_string();
    assert!(reopened.contains("<polygon"), "the rotated rect reopened as path geometry: {reopened}");
    assert_eq!(reopened.matches("<rect").count(), 2, "the two unrotated rects reopen: {reopened}");
    let mut after = svg_x_positions(&mut s);
    after.sort_by(|a, b| a.total_cmp(b));
    let mut before = before;
    before.sort_by(|a, b| a.total_cmp(b));
    // The two rects keep their distributed x positions through the reopen.
    assert_eq!(before, after, "the rects' x positions survive the reopen");
}
