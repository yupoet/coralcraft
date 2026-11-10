//! CoralCraft: the CorelDRAW fill and outline daily workflow over the shared commands (P1 Task 7)
//! — fills and outlines (weight, join, dash, arrows, colour) stay editable through a native
//! `.vectorcraft` save and reopen, the exported SVG shows them as attributes of its own text, and
//! a CDR7 export/reopen keeps the outline's width within the format's coordinate tolerance.

use serde_json::json;
use vectorcraft_engine::Session;

fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", &json!({ "width": 300, "height": 200 })).unwrap();
    s
}

/// A stroked, dashed, arrowed path with a solid fill, selected.
fn outlined(s: &mut Session) -> u64 {
    let id = s.execute("path.create", &json!({ "d": "M 20 20 L 200 20 L 200 110", "closed": true })).unwrap()["id"].as_u64().unwrap();
    s.execute("select.set", &json!({ "ids": [id] })).unwrap();
    s.execute("paint.setFill", &json!({ "color": "#e8b04b" })).unwrap();
    s.execute("stroke.set", &json!({ "weight": 4.0, "join": "round", "dash": [5.0, 3.0] })).unwrap();
    id
}

#[test]
fn fills_and_outlines_survive_reopen_and_show_in_the_svg() {
    let mut s = session();
    outlined(&mut s);
    // The exported SVG shows the fill, the dash and the join as its own attributes.
    let svg = s.execute("document.serialize", &json!({ "format": "svg" })).unwrap()["text"].as_str().unwrap().to_string();
    assert!(svg.contains("fill=\"#e8b04b\""), "the fill: {svg}");
    assert!(svg.contains("stroke-dasharray"), "the dash: {svg}");
    assert!(svg.contains("stroke-linejoin=\"round\""), "the join: {svg}");
    // With an arrowhead, the exporter draws the dashed stroke and the head as geometry (each dash
    // a closed subpath, the head a triangle).
    s.execute("stroke.set", &json!({ "startArrow": "Triangle" })).unwrap();
    let svg = s.execute("document.serialize", &json!({ "format": "svg" })).unwrap()["text"].as_str().unwrap().to_string();
    assert!(svg.matches(" Z").count() > 10, "the dashes draw as geometry: {svg}");
    assert!(svg.contains("L28 28 Z"), "the arrowhead's triangle: {svg}");
    // Native reopen keeps them editable: the same commands act on the reopened path.
    let data = s.execute("document.serialize", &json!({ "format": "vectorcraft" })).unwrap()["dataBase64"].as_str().unwrap().to_string();
    s.execute("document.open", &json!({ "dataBase64": data })).unwrap();
    let paths = s.execute("document.inspect", &json!({ "kind": "Path" })).unwrap();
    let id = paths["nodes"].as_array().expect("the reopened path")[0]["id"].as_u64().unwrap();
    let node = s.execute("document.node", &json!({ "id": id, "summary": true })).unwrap();
    assert_eq!(node["strokeWidth"].as_f64(), Some(4.0), "the outline's width reopened");
    assert_eq!(node["fill"].as_str(), Some("#e8b04b"), "the fill reopened");
    let so = node["strokeOptions"].clone();
    assert_eq!(so["dash"].as_array().map(|d| d.len()), Some(2), "the dash reopened: {so}");
    assert_eq!(so["startArrow"].as_str(), Some("Triangle"), "the arrowhead reopened");
    // Still editable after the reopen.
    s.execute("select.set", &json!({ "ids": [id] })).unwrap();
    s.execute("stroke.set", &json!({ "weight": 7.0 })).unwrap();
    let node = s.execute("document.node", &json!({ "id": id, "summary": true })).unwrap();
    assert_eq!(node["strokeWidth"].as_f64(), Some(7.0), "the reopened outline edits");

    // CDR7: the outline's width survives the round trip within the 1/100 mm coordinate
    // quantization (the `outl` chunk corel-import writes).
    let (mut s2, _id) = (session(), 0);
    outlined(&mut s2);
    let cdr = s2.execute("document.serialize", &json!({ "format": "cdr" })).unwrap()["dataBase64"].as_str().unwrap().to_string();
    s2.execute("document.open", &json!({ "name": "outlined.cdr", "dataBase64": cdr })).unwrap();
    let paths = s2.execute("document.inspect", &json!({ "kind": "Path" })).unwrap();
    let id = paths["nodes"].as_array().expect("the reopened path")[0]["id"].as_u64().unwrap();
    let node = s2.execute("document.node", &json!({ "id": id, "summary": true })).unwrap();
    let w = node["strokeWidth"].as_f64().unwrap_or_default();
    assert!((w - 4.0).abs() < 0.02, "the outline width reopened from CDR7 within tolerance: {w}");
}
