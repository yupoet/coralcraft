//! CoralCraft: PowerClip lifecycle evidence over the shared clip-group commands (P3 Task 2).

use serde_json::json;
use vectorcraft_doc::{NodeId, NodeKind};
use vectorcraft_engine::Session;

fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 300, "height": 200})).unwrap();
    s
}

fn rectangle(s: &mut Session, x: f64, y: f64, width: f64, height: f64) -> u64 {
    s.execute("shape.rectangle", &json!({"x": x, "y": y, "width": width, "height": height})).unwrap()["id"].as_u64().unwrap()
}

fn make_clip(s: &mut Session, contents: u64, frame: u64) -> u64 {
    s.execute("select.set", &json!({"ids": [contents, frame]})).unwrap();
    s.execute("object.clippingMask.make", &json!({})).unwrap()["id"].as_u64().unwrap()
}

fn native_reopen(s: &mut Session) {
    let data = s.execute("document.serialize", &json!({"format": "vectorcraft"})).unwrap()["dataBase64"].as_str().unwrap().to_string();
    s.execute("document.open", &json!({"dataBase64": data})).unwrap();
}

#[test]
fn powerclip_contents_and_frame_stay_editable_through_reopen_and_release() {
    let mut s = session();
    let contents = rectangle(&mut s, 0.0, 0.0, 160.0, 120.0);
    let frame = rectangle(&mut s, 25.0, 25.0, 60.0, 50.0);
    let clip = make_clip(&mut s, contents, frame);
    assert_eq!(s.doc().unwrap().doc.node(NodeId(clip)).unwrap().kind_label(), "Clip Group");

    s.execute("object.clippingMask.editContents", &json!({})).unwrap();
    assert_eq!(s.doc().unwrap().selection.objects, vec![NodeId(contents)]);
    s.execute("object.move", &json!({"dx": 12.0, "dy": 4.0})).unwrap();
    let NodeKind::Path { path, .. } = &s.doc().unwrap().doc.node(NodeId(contents)).unwrap().kind else { panic!("contents stay a path") };
    assert_eq!((path.bounds().unwrap().x0, path.bounds().unwrap().y0), (12.0, 4.0));
    s.execute("edit.undo", &json!({})).unwrap();
    let NodeKind::Path { path, .. } = &s.doc().unwrap().doc.node(NodeId(contents)).unwrap().kind else { panic!("undo keeps contents editable") };
    assert_eq!((path.bounds().unwrap().x0, path.bounds().unwrap().y0), (0.0, 0.0));
    s.execute("edit.redo", &json!({})).unwrap();

    s.execute("select.set", &json!({"ids": [clip]})).unwrap();
    s.execute("object.clippingMask.editMask", &json!({})).unwrap();
    assert_eq!(s.doc().unwrap().selection.objects, vec![NodeId(frame)]);

    native_reopen(&mut s);
    assert_eq!(s.doc().unwrap().doc.node(NodeId(clip)).unwrap().kind_label(), "Clip Group");
    s.execute("select.set", &json!({"ids": [clip]})).unwrap();
    s.execute("object.clippingMask.editContents", &json!({})).unwrap();
    assert_eq!(s.doc().unwrap().selection.objects, vec![NodeId(contents)]);
    s.execute("object.move", &json!({"dx": 1.0, "dy": 2.0})).unwrap();
    let NodeKind::Path { path, .. } = &s.doc().unwrap().doc.node(NodeId(contents)).unwrap().kind else { panic!("reopened contents stay a path") };
    assert_eq!((path.bounds().unwrap().x0, path.bounds().unwrap().y0), (13.0, 6.0));

    let svg = s.execute("document.serialize", &json!({"format": "svg"})).unwrap()["text"].as_str().unwrap().to_string();
    assert!(svg.contains("<clipPath"), "the exported clipping geometry is present: {svg}");
    usvg::Tree::from_str(&svg, &usvg::Options::default()).expect("independent usvg parse of PowerClip");
    let pdf = vectorcraft_format::base64_decode(s.execute("document.serialize", &json!({"format": "pdf"})).unwrap()["dataBase64"].as_str().unwrap())
        .unwrap();
    let imported = vectorcraft_pdf::import(&pdf).expect("independent Hayro-backed PowerClip PDF read");
    assert_eq!(imported.artboards.len(), 1);

    s.execute("select.set", &json!({"ids": [clip]})).unwrap();
    s.execute("object.clippingMask.release", &json!({})).unwrap();
    assert!(matches!(s.doc().unwrap().doc.node(NodeId(clip)).unwrap().kind, NodeKind::Group { clip: false, .. }));
    assert!(matches!(s.doc().unwrap().doc.node(NodeId(contents)).unwrap().kind, NodeKind::Path { .. }));
    assert!(matches!(s.doc().unwrap().doc.node(NodeId(frame)).unwrap().kind, NodeKind::Path { .. }));
}

#[test]
fn powerclip_nesting_preserves_the_inner_clip_and_its_sources() {
    let mut s = session();
    let contents = rectangle(&mut s, 0.0, 0.0, 180.0, 120.0);
    let inner_frame = rectangle(&mut s, 20.0, 20.0, 100.0, 80.0);
    let inner = make_clip(&mut s, contents, inner_frame);
    let outer_frame = rectangle(&mut s, 40.0, 30.0, 50.0, 45.0);
    let outer = make_clip(&mut s, inner, outer_frame);

    native_reopen(&mut s);
    assert_eq!(s.doc().unwrap().doc.node(NodeId(outer)).unwrap().kind_label(), "Clip Group");
    assert_eq!(s.doc().unwrap().doc.node(NodeId(inner)).unwrap().kind_label(), "Clip Group");
    s.execute("select.set", &json!({"ids": [outer]})).unwrap();
    s.execute("object.clippingMask.editContents", &json!({})).unwrap();
    assert_eq!(s.doc().unwrap().selection.objects, vec![NodeId(inner)]);
    s.execute("object.clippingMask.editContents", &json!({})).unwrap();
    assert_eq!(s.doc().unwrap().selection.objects, vec![NodeId(contents)]);

    s.execute("select.set", &json!({"ids": [outer]})).unwrap();
    s.execute("object.clippingMask.release", &json!({})).unwrap();
    assert!(matches!(s.doc().unwrap().doc.node(NodeId(outer)).unwrap().kind, NodeKind::Group { clip: false, .. }));
    assert_eq!(s.doc().unwrap().doc.node(NodeId(inner)).unwrap().kind_label(), "Clip Group");
    assert!(matches!(s.doc().unwrap().doc.node(NodeId(contents)).unwrap().kind, NodeKind::Path { .. }));
}
