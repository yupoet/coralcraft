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

// CoralCraft (P3 Task E6): Lock Contents to PowerClip. The matrix in `docs/p3-inventory.md` §29
// freezes this slice's semantics.
#[test]
fn lock_contents_to_frame_controls_whether_contents_follow_the_frame() {
    let mut s = session();
    let contents = rectangle(&mut s, 0.0, 0.0, 100.0, 80.0);
    let frame = rectangle(&mut s, 25.0, 25.0, 60.0, 50.0);
    let clip = make_clip(&mut s, contents, frame);
    let bounds = |s: &Session, id| {
        let NodeKind::Path { path, .. } = &s.doc().unwrap().doc.node(NodeId(id)).unwrap().kind else { panic!("a path") };
        path.bounds().unwrap()
    };
    // P2: locked by default — a move of the clip group carries frame and contents.
    s.execute("select.set", &json!({ "ids": [clip] })).unwrap();
    s.execute("object.move", &json!({ "dx": 10.0, "dy": 0.0 })).unwrap();
    assert_eq!(bounds(&s, contents).x0, 10.0, "locked: the contents follow");
    s.execute("edit.undo", &json!({})).unwrap();
    // P1 + P3: unlock, then the same move leaves the contents where the page put them.
    s.execute("object.clippingMask.setContentsLocked", &json!({ "ids": [clip], "lock": false })).unwrap();
    let undo_count = s.active().unwrap().history.undo.len();
    s.execute("object.move", &json!({ "dx": 10.0, "dy": 0.0 })).unwrap();
    assert_eq!(bounds(&s, frame).x0, 35.0, "unlocked: the frame moved");
    assert_eq!(bounds(&s, contents).x0, 0.0, "unlocked: the contents stay");
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(bounds(&s, frame).x0, 25.0, "undo restores the frame");
    assert_eq!(s.active().unwrap().history.undo.len(), undo_count, "one step for the move");
    // The toggle itself is one undo step and round-trips: undo relocks (a following move
    // carries the contents), redo unlocks again (the next move leaves them).
    s.execute("edit.undo", &json!({})).unwrap();
    s.execute("object.move", &json!({ "dx": 10.0, "dy": 0.0 })).unwrap();
    assert_eq!(bounds(&s, contents).x0, 10.0, "undo of the toggle relocks the contents");
    s.execute("edit.undo", &json!({})).unwrap();
    s.execute("edit.redo", &json!({})).unwrap();
    s.execute("object.clippingMask.setContentsLocked", &json!({ "lock": false })).unwrap();
    s.execute("object.move", &json!({ "dx": 10.0, "dy": 0.0 })).unwrap();
    assert_eq!(bounds(&s, contents).x0, 10.0, "unlocked again: the contents stay");
    // P4: native reopen keeps the flag, it still toggles, and the behaviour follows.
    native_reopen(&mut s);
    s.execute("select.set", &json!({ "ids": [clip] })).unwrap();
    s.execute("object.move", &json!({ "dx": 5.0, "dy": 0.0 })).unwrap();
    assert_eq!(bounds(&s, contents).x0, 10.0, "reopen keeps the unlock");
    s.execute("object.clippingMask.setContentsLocked", &json!({ "lock": true })).unwrap();
    s.execute("object.move", &json!({ "dx": 5.0, "dy": 0.0 })).unwrap();
    assert_eq!(bounds(&s, contents).x0, 15.0, "still toggles after reopen: relocked");
    // P6: the named relock warning, once, and only while unlocked.
    for (unlocked, want) in [(false, 0), (true, 1)] {
        if unlocked {
            s.execute("object.clippingMask.setContentsLocked", &json!({ "lock": false })).unwrap();
        }
        let out = s.execute("document.serialize", &json!({ "format": "cdr" })).unwrap();
        let warnings = out["warnings"].as_array().cloned().unwrap_or_default();
        let count = warnings
            .iter()
            .filter(|w| {
                w.as_str().is_some_and(|w| {
                    w == "unlocked PowerClip contents (Lock Contents to Frame off) relock in CDR/CMX; native documents keep the flag"
                })
            })
            .count();
        assert_eq!(count, want, "the named relock warning (unlocked={unlocked}): {warnings:?}");
    }
}

/// N1: refusals change nothing.
#[test]
fn lock_contents_refusals_change_nothing() {
    let mut s = session();
    let plain = rectangle(&mut s, 0.0, 0.0, 50.0, 50.0);
    let before = s.execute("document.serialize", &json!({ "format": "vectorcraft" })).unwrap()["dataBase64"].as_str().unwrap().to_string();
    assert!(s.execute("object.clippingMask.setContentsLocked", &json!({ "ids": [plain], "lock": false })).is_err(), "an ordinary object");
    assert!(s.execute("object.clippingMask.setContentsLocked", &json!({ "lock": "yes" })).is_err(), "a non-bool lock");
    let after = s.execute("document.serialize", &json!({ "format": "vectorcraft" })).unwrap()["dataBase64"].as_str().unwrap().to_string();
    assert_eq!(before, after, "the refused calls changed nothing");
}
