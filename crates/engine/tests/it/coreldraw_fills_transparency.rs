//! CoralCraft: editable mesh fills and transparency over the shared object model (P3 Task 3).

use serde_json::json;
use vectorcraft_color::BlendMode;
use vectorcraft_doc::{NodeId, NodeKind};
use vectorcraft_engine::Session;

fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 240, "height": 160})).unwrap();
    s
}

fn rectangle(s: &mut Session) -> u64 {
    s.execute("shape.rectangle", &json!({"x": 20, "y": 20, "width": 120, "height": 90})).unwrap()["id"].as_u64().unwrap()
}

fn native_reopen(s: &mut Session) {
    let data = s.execute("document.serialize", &json!({"format": "vectorcraft"})).unwrap()["dataBase64"].as_str().unwrap().to_string();
    s.execute("document.open", &json!({"dataBase64": data})).unwrap();
}

fn independent_export_reads(s: &mut Session) {
    let svg = s.execute("document.serialize", &json!({"format": "svg"})).unwrap()["text"].as_str().unwrap().to_string();
    usvg::Tree::from_str(&svg, &usvg::Options::default()).expect("independent usvg parse of the fill/transparency output");
    let pdf = vectorcraft_format::base64_decode(s.execute("document.serialize", &json!({"format": "pdf"})).unwrap()["dataBase64"].as_str().unwrap())
        .unwrap();
    let imported = vectorcraft_pdf::import(&pdf).expect("independent Hayro-backed fill/transparency PDF read");
    assert_eq!(imported.artboards.len(), 1);
}

#[test]
fn mesh_fill_points_survive_native_reopen_and_remain_editable() {
    let mut s = session();
    let shape = rectangle(&mut s);
    s.execute("object.mesh.create", &json!({"id": shape, "rows": 1, "cols": 1})).unwrap();
    s.execute("object.mesh.setPointColor", &json!({"id": shape, "index": 3, "color": "#00ff00", "opacity": 0.5})).unwrap();
    let NodeKind::Mesh(mesh) = &s.doc().unwrap().doc.node(NodeId(shape)).unwrap().kind else { panic!("mesh fill") };
    assert_eq!(mesh.points[3].color.to_hex(), "#00ff00");
    assert_eq!(mesh.points[3].opacity, 0.5);
    s.execute("edit.undo", &json!({})).unwrap();
    let NodeKind::Mesh(mesh) = &s.doc().unwrap().doc.node(NodeId(shape)).unwrap().kind else { panic!("undo keeps the mesh") };
    assert_eq!(mesh.points[3].color.to_hex(), "#ffffff");
    s.execute("edit.redo", &json!({})).unwrap();

    native_reopen(&mut s);
    let NodeKind::Mesh(mesh) = &s.doc().unwrap().doc.node(NodeId(shape)).unwrap().kind else { panic!("native reopen keeps the mesh") };
    assert_eq!(mesh.points[3].color.to_hex(), "#00ff00");
    s.execute("object.mesh.setPointColor", &json!({"id": shape, "index": 0, "color": "#ff0000"})).unwrap();
    s.execute("object.mesh.movePoint", &json!({"id": shape, "index": 1, "x": 90.0, "y": 30.0})).unwrap();
    let NodeKind::Mesh(mesh) = &s.doc().unwrap().doc.node(NodeId(shape)).unwrap().kind else { panic!("edited mesh") };
    assert_eq!(mesh.points[0].color.to_hex(), "#ff0000");
    assert_eq!((mesh.points[1].p.x, mesh.points[1].p.y), (90.0, 30.0));
    independent_export_reads(&mut s);
}

#[test]
fn object_transparency_and_blend_mode_survive_reopen_and_undo() {
    let mut s = session();
    let shape = rectangle(&mut s);
    s.execute("transparency.set", &json!({"id": shape, "opacity": 50, "blend": "multiply"})).unwrap();
    let node = s.doc().unwrap().doc.node(NodeId(shape)).unwrap();
    assert_eq!((node.opacity, node.blend), (0.5, BlendMode::Multiply));
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(s.doc().unwrap().doc.node(NodeId(shape)).unwrap().opacity, 1.0);
    s.execute("edit.redo", &json!({})).unwrap();

    native_reopen(&mut s);
    let node = s.doc().unwrap().doc.node(NodeId(shape)).unwrap();
    assert_eq!((node.opacity, node.blend), (0.5, BlendMode::Multiply));
    s.execute("transparency.set", &json!({"id": shape, "opacity": 65})).unwrap();
    assert_eq!(s.doc().unwrap().doc.node(NodeId(shape)).unwrap().opacity, 0.65);
    independent_export_reads(&mut s);
}

#[test]
fn fountain_fill_stops_survive_native_reopen_and_editing() {
    let mut s = session();
    let shape = rectangle(&mut s);
    let stops = json!([{"offset": 0, "color": "#ffffff"}, {"offset": 1, "color": "#333333"}]);
    s.execute("paint.setFill", &json!({"id": shape, "gradient": {"kind": "linear", "stops": stops}})).unwrap();
    native_reopen(&mut s);
    let vectorcraft_color::Paint::Gradient(gradient) = &s.doc().unwrap().doc.node(NodeId(shape)).unwrap().appearance.fill_paint() else {
        panic!("fountain fill")
    };
    assert_eq!(gradient.gradient.stops[1].color.to_hex(), "#333333");
    s.execute("paint.editGradient", &json!({"id": shape, "stops": [{"offset": 0, "color": "#ff0000"}, {"offset": 1, "color": "#0000ff"}]})).unwrap();
    let vectorcraft_color::Paint::Gradient(gradient) = &s.doc().unwrap().doc.node(NodeId(shape)).unwrap().appearance.fill_paint() else {
        panic!("edited fountain fill")
    };
    assert_eq!(gradient.gradient.stops[0].color.to_hex(), "#ff0000");
    assert_eq!(gradient.gradient.stops[1].color.to_hex(), "#0000ff");
    independent_export_reads(&mut s);
}

#[test]
fn vector_pattern_fill_keeps_its_definition_and_editable_target() {
    let mut s = session();
    let tile = rectangle(&mut s);
    s.execute("paint.setFill", &json!({"id": tile, "color": "#808080"})).unwrap();
    s.execute("object.pattern.make", &json!({"id": tile, "name": "Grey Tile", "edit": false})).unwrap();
    let target = rectangle(&mut s);
    s.execute("paint.setFill", &json!({"id": target, "swatch": "Grey Tile"})).unwrap();
    assert!(matches!(s.doc().unwrap().doc.node(NodeId(target)).unwrap().appearance.fill_paint(), vectorcraft_color::Paint::Pattern { .. }));

    native_reopen(&mut s);
    assert!(s.doc().unwrap().doc.pattern("Grey Tile").is_some());
    assert!(matches!(s.doc().unwrap().doc.node(NodeId(target)).unwrap().appearance.fill_paint(), vectorcraft_color::Paint::Pattern { .. }));
    s.execute("paint.setFill", &json!({"id": target, "color": "#204080"})).unwrap();
    assert_eq!(s.doc().unwrap().doc.node(NodeId(target)).unwrap().appearance.fill_paint().color().unwrap().to_hex(), "#204080");
    independent_export_reads(&mut s);
}
