//! CoralCraft: editable Blend and Contour workflows over VectorCraft live effects (P3 Task 4).

use serde_json::json;
use vectorcraft_doc::live::{self, BlendSpacing};
use vectorcraft_doc::{NodeId, NodeKind};
use vectorcraft_engine::Session;

fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 320, "height": 200})).unwrap();
    s
}

fn rectangle(s: &mut Session, x: f64, color: &str) -> u64 {
    let id = s.execute("shape.rectangle", &json!({"x": x, "y": 40, "width": 30, "height": 30})).unwrap()["id"].as_u64().unwrap();
    s.execute("paint.setFill", &json!({"id": id, "color": color})).unwrap();
    id
}

fn native_reopen(s: &mut Session) {
    let data = s.execute("document.serialize", &json!({"format": "vectorcraft"})).unwrap()["dataBase64"].as_str().unwrap().to_string();
    s.execute("document.open", &json!({"dataBase64": data})).unwrap();
}

fn independent_svg_read(s: &mut Session) -> String {
    let svg = s.execute("document.serialize", &json!({"format": "svg"})).unwrap()["text"].as_str().unwrap().to_string();
    usvg::Tree::from_str(&svg, &usvg::Options::default()).expect("independent usvg parse of the live-effect output");
    svg
}

#[test]
fn blend_steps_spine_and_sources_remain_editable_after_native_reopen() {
    let mut s = session();
    let a = rectangle(&mut s, 20.0, "#000000");
    let b = rectangle(&mut s, 220.0, "#ffffff");
    let blend = s.execute("object.blend.make", &json!({"ids": [a, b], "steps": 4})).unwrap()["id"].as_u64().unwrap();
    s.execute("object.blend.options", &json!({"id": blend, "steps": 7})).unwrap();
    s.execute("select.set", &json!({"ids": [blend]})).unwrap();
    assert_eq!(s.execute("object.blend.info", &json!({})).unwrap()["steps"], json!(7));
    let NodeKind::Blend { spec, .. } = &s.doc().unwrap().doc.node(NodeId(blend)).unwrap().kind else { panic!("live blend") };
    assert_eq!(spec.spacing, BlendSpacing::Steps(7));
    assert_eq!(live::expand_live(s.doc().unwrap().doc.node(NodeId(blend)).unwrap()).len(), 9);

    s.execute("object.blend.spine.moveAnchor", &json!({"id": blend, "anchor": 0, "x": 20, "y": 60})).unwrap();
    s.execute("object.blend.reverseSpine", &json!({"id": blend})).unwrap();
    s.execute("object.blend.reverseFrontToBack", &json!({"id": blend})).unwrap();
    s.execute("edit.undo", &json!({})).unwrap();
    s.execute("edit.redo", &json!({})).unwrap();

    native_reopen(&mut s);
    let NodeKind::Blend { spec, .. } = &s.doc().unwrap().doc.node(NodeId(blend)).unwrap().kind else { panic!("native reopen keeps the live blend") };
    assert_eq!(spec.spacing, BlendSpacing::Steps(7));
    s.execute("paint.setFill", &json!({"id": a, "color": "#ff0000"})).unwrap();
    assert_eq!(s.doc().unwrap().doc.node(NodeId(a)).unwrap().appearance.fill_paint().color().unwrap().to_hex(), "#ff0000");
    let svg = independent_svg_read(&mut s);
    assert_eq!(svg.matches("<rect").count(), 9, "expanded blend geometry is visible in SVG: {svg}");

    s.execute("select.set", &json!({"ids": [blend]})).unwrap();
    let released = s.execute("object.blend.release", &json!({})).unwrap();
    assert_eq!(released["ids"], json!([b, a]), "release follows the reversed front-to-back order");
    assert!(s.doc().unwrap().doc.node(NodeId(blend)).is_none());
}

#[test]
fn contour_updates_one_live_offset_effect_and_expands_only_when_asked() {
    let mut s = session();
    let source = rectangle(&mut s, 40.0, "#204080");
    s.execute("select.set", &json!({"ids": [source]})).unwrap();
    let made = s.execute("addon.contour.set", &json!({"offset": 10, "corners": "round"})).unwrap();
    let contour = made["created"][0].as_u64().unwrap();
    let node = s.doc().unwrap().doc.node(NodeId(contour)).unwrap();
    assert_eq!(node.appearance.effects.len(), 1);
    assert_eq!(node.appearance.effects[0].id, "path.offsetPath");
    assert_eq!(node.appearance.effects[0].params["joins"], json!("round"));

    s.execute("effect.setParams", &json!({"id": contour, "index": 0, "params": {"offset": 4}})).unwrap();
    native_reopen(&mut s);
    let node = s.doc().unwrap().doc.node(NodeId(contour)).unwrap();
    assert_eq!(node.appearance.effects[0].params["offset"].as_f64(), Some(4.0));
    assert_eq!(node.appearance.effects[0].params["joins"], json!("round"));
    assert!(matches!(node.kind, NodeKind::Path { .. }));
    independent_svg_read(&mut s);

    s.execute("select.set", &json!({"ids": [contour]})).unwrap();
    let expanded = s.execute("effect.expandAppearance", &json!({})).unwrap();
    let expanded_id = expanded["ids"][0].as_u64().unwrap();
    assert!(s.doc().unwrap().doc.node(NodeId(expanded_id)).unwrap().appearance.effects.is_empty());
    assert!(matches!(s.doc().unwrap().doc.node(NodeId(source)).unwrap().kind, NodeKind::Path { .. }));
}

#[test]
fn envelope_mesh_contents_and_warp_parameters_remain_live_after_reopen() {
    let mut s = session();
    let source = rectangle(&mut s, 40.0, "#204080");
    s.execute("select.set", &json!({"ids": [source]})).unwrap();
    let envelope = s.execute("object.envelope.makeWithWarp", &json!({"style": "arc", "bend": 25})).unwrap()["id"].as_u64().unwrap();
    s.execute("object.envelope.options", &json!({"id": envelope, "fidelity": 90, "bend": -30})).unwrap();
    let editing = s.execute("object.envelope.editContents", &json!({"id": envelope})).unwrap();
    assert_eq!(editing["editing"], json!(true));
    assert_eq!(s.doc().unwrap().selection.objects, vec![NodeId(source)]);
    s.execute("paint.setFill", &json!({"id": source, "color": "#ff0000"})).unwrap();
    s.execute("object.envelope.editContents", &json!({"editing": false})).unwrap();

    native_reopen(&mut s);
    let NodeKind::Envelope { fidelity, kind, .. } = &s.doc().unwrap().doc.node(NodeId(envelope)).unwrap().kind else { panic!("live envelope") };
    assert_eq!(*fidelity, 90.0);
    let vectorcraft_doc::live::EnvelopeKind::Warp { bend, .. } = kind.as_ref() else { panic!("warp envelope") };
    assert_eq!(*bend, -30.0);
    assert_eq!(s.doc().unwrap().doc.node(NodeId(source)).unwrap().appearance.fill_paint().color().unwrap().to_hex(), "#ff0000");

    s.execute("select.set", &json!({"ids": [envelope]})).unwrap();
    s.execute("object.envelope.resetWithMesh", &json!({"id": envelope, "rows": 2, "cols": 2})).unwrap();
    s.execute("object.envelope.setMeshPoint", &json!({"id": envelope, "index": 4, "x": 85, "y": 45})).unwrap();
    native_reopen(&mut s);
    let NodeKind::Envelope { kind, .. } = &s.doc().unwrap().doc.node(NodeId(envelope)).unwrap().kind else { panic!("live mesh envelope") };
    assert!(matches!(kind.as_ref(), vectorcraft_doc::live::EnvelopeKind::Mesh { .. }));
    independent_svg_read(&mut s);
    s.execute("select.set", &json!({"ids": [envelope]})).unwrap();
    s.execute("object.envelope.expand", &json!({})).unwrap();
    assert!(matches!(s.doc().unwrap().doc.node(NodeId(envelope)).unwrap().kind, NodeKind::Group { .. }));
}

#[test]
fn distort_twist_parameters_survive_native_reopen_and_export() {
    let mut s = session();
    let source = rectangle(&mut s, 40.0, "#204080");
    let applied = s.execute("effect.apply", &json!({"id": source, "effect": "distort.twist", "params": {"angle": 10}})).unwrap();
    assert_eq!(applied["index"], json!(0));
    s.execute("effect.setParams", &json!({"id": source, "index": 0, "params": {"angle": 35}})).unwrap();
    native_reopen(&mut s);
    let node = s.doc().unwrap().doc.node(NodeId(source)).unwrap();
    assert_eq!(node.appearance.effects[0].id, "distort.twist");
    assert_eq!(node.appearance.effects[0].params["angle"].as_f64(), Some(35.0));
    assert!(matches!(node.kind, NodeKind::Path { .. }));
    independent_svg_read(&mut s);
}

#[test]
fn extrude_bevel_and_drop_shadow_keep_ordered_editable_parameters() {
    let mut s = session();
    let source = rectangle(&mut s, 40.0, "#204080");
    s.execute("effect.apply", &json!({"id": source, "effect": "threeD.extrude", "params": {"depth": 40, "bevel": "classic", "bevelWidth": 3}}))
        .unwrap();
    s.execute("effect.apply", &json!({"id": source, "effect": "stylize.dropShadow", "params": {"x": 8, "y": 6, "blur": 2, "opacity": 60}})).unwrap();
    s.execute("effect.setParams", &json!({"id": source, "index": 0, "params": {"depth": 55}})).unwrap();
    s.execute("effect.setParams", &json!({"id": source, "index": 1, "params": {"x": 10}})).unwrap();

    native_reopen(&mut s);
    let effects = &s.doc().unwrap().doc.node(NodeId(source)).unwrap().appearance.effects;
    assert_eq!(effects.iter().map(|effect| effect.id.as_str()).collect::<Vec<_>>(), ["threeD.extrude", "stylize.dropShadow"]);
    assert_eq!(effects[0].params["depth"].as_f64(), Some(55.0));
    assert_eq!(effects[0].params["bevel"], json!("classic"));
    assert_eq!(effects[1].params["x"].as_f64(), Some(10.0));
    assert_eq!(effects[1].params["opacity"].as_f64(), Some(60.0));
    assert!(matches!(s.doc().unwrap().doc.node(NodeId(source)).unwrap().kind, NodeKind::Path { .. }));
    independent_svg_read(&mut s);
    let pdf = vectorcraft_format::base64_decode(s.execute("document.serialize", &json!({"format": "pdf"})).unwrap()["dataBase64"].as_str().unwrap())
        .unwrap();
    assert_eq!(vectorcraft_pdf::import(&pdf).expect("independent Hayro-backed effect PDF read").artboards.len(), 1);
}

#[test]
fn effect_stack_reorders_duplicates_hides_removes_and_expands_atomically() {
    let mut s = session();
    let source = rectangle(&mut s, 40.0, "#204080");
    s.execute("effect.apply", &json!({"id": source, "effect": "stylize.dropShadow", "params": {"x": 8}})).unwrap();
    s.execute("effect.apply", &json!({"id": source, "effect": "distort.twist", "params": {"angle": 20}})).unwrap();
    s.execute("effect.move", &json!({"id": source, "from": 1, "to": 0})).unwrap();
    s.execute("effect.duplicate", &json!({"id": source, "index": 0})).unwrap();
    s.execute("effect.setParams", &json!({"id": source, "index": 0, "visible": false})).unwrap();
    let ids = |s: &Session| {
        s.doc().unwrap().doc.node(NodeId(source)).unwrap().appearance.effects.iter().map(|effect| effect.id.clone()).collect::<Vec<_>>()
    };
    assert_eq!(ids(&s), ["distort.twist", "distort.twist", "stylize.dropShadow"]);
    assert!(!s.doc().unwrap().doc.node(NodeId(source)).unwrap().appearance.effects[0].visible);

    let before = s.doc().unwrap().history.undo.len();
    assert!(s.execute("effect.move", &json!({"id": source, "from": 99, "to": 0})).is_err());
    assert_eq!(s.doc().unwrap().history.undo.len(), before);
    native_reopen(&mut s);
    assert_eq!(ids(&s), ["distort.twist", "distort.twist", "stylize.dropShadow"]);
    assert!(!s.doc().unwrap().doc.node(NodeId(source)).unwrap().appearance.effects[0].visible);

    s.execute("effect.remove", &json!({"id": source, "index": 1})).unwrap();
    assert_eq!(ids(&s), ["distort.twist", "stylize.dropShadow"]);
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(ids(&s), ["distort.twist", "distort.twist", "stylize.dropShadow"]);
    s.execute("edit.redo", &json!({})).unwrap();
    assert_eq!(ids(&s), ["distort.twist", "stylize.dropShadow"]);
    independent_svg_read(&mut s);

    s.execute("select.set", &json!({"ids": [source]})).unwrap();
    let expanded = s.execute("effect.expandAppearance", &json!({})).unwrap();
    for id in expanded["ids"].as_array().unwrap().iter().filter_map(|id| id.as_u64()) {
        assert!(s.doc().unwrap().doc.node(NodeId(id)).unwrap().appearance.effects.is_empty());
    }
}
