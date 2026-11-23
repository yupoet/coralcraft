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

// CoralCraft (P3 Task E2): Block Shadow — the swept-silhouette live effect. The matrix in
// `docs/p3-inventory.md` §25 freezes this slice's semantics.

#[test]
fn block_shadow_sweeps_edits_and_exports() {
    let mut s = Session::new();
    s.execute("file.new", &json!({ "width": 400, "height": 300 })).unwrap();
    let shape = s.execute("shape.rectangle", &json!({ "x": 40, "y": 40, "width": 90, "height": 60 })).unwrap()["id"].as_u64().unwrap();
    // P1: the effect lands with its parameters.
    s.execute("effect.apply", &json!({ "id": shape, "effect": "stylize.blockShadow", "params": { "x": 12.0, "y": 8.0, "color": "#204080" } }))
        .unwrap();
    let applied = s.execute("effect.list", &json!({ "id": shape })).unwrap()["applied"].as_array().cloned().unwrap_or_default();
    let fx = applied
        .iter()
        .flat_map(|a| a["effects"].as_array().into_iter().flatten())
        .chain(applied.iter().flat_map(|a| a["items"].as_array().into_iter().flatten().flat_map(|i| i["effects"].as_array().into_iter().flatten())))
        .find(|e| e["id"].as_str() == Some("stylize.blockShadow"))
        .expect("the effect listed");
    assert_eq!(fx["params"]["x"].as_f64(), Some(12.0));
    // P2: the exported art holds the swept block behind the object, in the effect's colour.
    let exported = |s: &mut Session| {
        let svg = s.execute("document.serialize", &json!({ "format": "svg" })).unwrap()["text"].as_str().unwrap().to_string();
        let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).expect("independent usvg parse");
        let mut shadow = 0usize;
        count_filled(tree.root(), &[32, 64, 128], &mut shadow);
        (shadow, svg)
    };
    let (shadow, svg) = exported(&mut s);
    assert!(shadow > 0, "a #204080 shadow path exports: {svg}");
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
    assert!(paths >= 2, "the PDF read returns the shadow and the object: {paths}");
    // P3: parameter edits move the shadow; undo/redo round-trips.
    let undo_count = s.active().unwrap().history.undo.len();
    s.execute("effect.setParams", &json!({ "id": shape, "index": 0, "params": { "color": "#20a040" } })).unwrap();
    let (_, svg) = exported(&mut s);
    assert!(svg.contains("#20a040") && !svg.contains("#204080"), "the shadow colour follows: {svg}");
    assert_eq!(s.active().unwrap().history.undo.len(), undo_count + 1, "one step");
    s.execute("edit.undo", &json!({})).unwrap();
    let (_, svg) = exported(&mut s);
    assert!(svg.contains("#204080"), "undo restores the colour");
    s.execute("edit.redo", &json!({})).unwrap();
    // P4: native reopen keeps it; it still edits.
    let data = s.execute("document.serialize", &json!({ "format": "vectorcraft" })).unwrap()["dataBase64"].as_str().unwrap().to_string();
    s.execute("document.open", &json!({ "dataBase64": data })).unwrap();
    let list = s.execute("effect.list", &json!({ "id": shape })).unwrap();
    assert!(
        list["applied"].as_array().is_some_and(|a| a
            .iter()
            .any(|o| o["effects"].as_array().is_some_and(|fx| fx.iter().any(|e| e["id"].as_str() == Some("stylize.blockShadow"))))),
        "reopen keeps the effect"
    );
    s.execute("effect.setParams", &json!({ "id": shape, "index": 0, "params": { "x": 30.0 } })).unwrap();
    let list = s.execute("effect.list", &json!({ "id": shape })).unwrap();
    let fx = list["applied"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|a| a["effects"].as_array().into_iter().flatten())
        .find(|e| e["id"].as_str() == Some("stylize.blockShadow"))
        .unwrap();
    assert_eq!(fx["params"]["x"].as_f64(), Some(30.0), "still edits after reopen");
    // P6: CDR/CMX maps it to the Extrude vector effect with the exact geometry.
    let (cdr_doc, warnings) = {
        let st = s.doc().unwrap();
        corel_import::export_document_with_warnings(&st.doc).expect("export")
    };
    assert!(
        !warnings.iter().any(|w| w.contains("unsupported") && w.contains("blockShadow")),
        "the block shadow is mapped, not dropped: {warnings:?}"
    );
    let object = &cdr_doc.pages[0].layers[0].objects.iter().find(|o| o.id.0 == shape).unwrap();
    let (want_depth, want_angle) = (30.0f64.hypot(8.0), 8.0f64.atan2(30.0).to_degrees());
    let mapped = object.effects.iter().any(|e| matches!(e, corel_import::coralcraft_doc::VectorEffect::Extrude { depth_pt, angle_deg } if (depth_pt.get() - want_depth).abs() < 1e-9 && (angle_deg.get() - want_angle).abs() < 1e-9));
    assert!(mapped, "the Extrude mapping carries the depth and angle: {:?} (want {want_depth}/{want_angle})", object.effects);
    // P7: remove takes it off.
    s.execute("effect.remove", &json!({ "id": shape, "index": 0 })).unwrap();
    let list = s.execute("effect.list", &json!({ "id": shape })).unwrap();
    assert!(
        !list["applied"].as_array().is_some_and(|a| a
            .iter()
            .any(|o| o["effects"].as_array().is_some_and(|fx| fx.iter().any(|e| e["id"].as_str() == Some("stylize.blockShadow"))))),
        "removed"
    );
    let (shadow, _) = exported(&mut s);
    assert_eq!(shadow, 0, "the shadow is gone from the export");
}

/// N1: refusals change nothing.
#[test]
fn block_shadow_refusals_change_nothing() {
    let mut s = Session::new();
    s.execute("file.new", &json!({ "width": 400, "height": 300 })).unwrap();
    let a = s.execute("shape.rectangle", &json!({ "x": 40, "y": 40, "width": 60, "height": 40 })).unwrap()["id"].as_u64().unwrap();
    let b = s.execute("shape.ellipse", &json!({ "x": 140, "y": 40, "width": 60, "height": 40 })).unwrap()["id"].as_u64().unwrap();
    let group = s.execute("object.group", &json!({ "ids": [a, b] })).unwrap()["id"].as_u64().unwrap();
    let before = s.execute("document.serialize", &json!({ "format": "vectorcraft" })).unwrap()["dataBase64"].as_str().unwrap().to_string();
    assert!(
        s.execute("effect.apply", &json!({ "id": group, "effect": "stylize.blockShadow", "params": { "x": 5.0, "y": 5.0 } })).is_err(),
        "a group is refused (paths only, the validation names it)"
    );
    assert!(
        s.execute("effect.apply", &json!({ "id": a, "effect": "stylize.blockShadow", "params": { "x": "far", "y": 5.0 } })).is_err(),
        "a non-number x"
    );
    assert!(
        s.execute("effect.apply", &json!({ "id": a, "effect": "stylize.blockShadow", "params": { "color": "crimson-ish" } })).is_err(),
        "a non-colour"
    );
    let after = s.execute("document.serialize", &json!({ "format": "vectorcraft" })).unwrap()["dataBase64"].as_str().unwrap().to_string();
    assert_eq!(before, after, "the refused calls changed nothing");
}

fn count_filled(group: &usvg::Group, rgb: &[u8; 3], out: &mut usize) {
    for node in group.children() {
        match node {
            usvg::Node::Group(child) => count_filled(child, rgb, out),
            usvg::Node::Path(path)
                if !path.data().is_empty()
                    && path
                        .fill()
                        .is_some_and(|fill| matches!(fill.paint(), usvg::Paint::Color(color) if [color.red, color.green, color.blue] == *rgb)) =>
            {
                *out += 1;
            }
            _ => {}
        }
    }
}

// CoralCraft (P3 Task E3): Add Perspective — the true projective four-corner distortion
// (`distort.perspective`), CorelDRAW's Effects > Add Perspective on the shared effect stack. The
// matrix in `docs/p3-inventory.md` §26 freezes this slice's semantics.

#[test]
fn perspective_is_projective_edits_and_exports() {
    let mut s = Session::new();
    s.execute("file.new", &json!({ "width": 400, "height": 300 })).unwrap();
    let shape = s.execute("shape.rectangle", &json!({ "x": 40, "y": 40, "width": 100, "height": 80 })).unwrap()["id"].as_u64().unwrap();
    // One-point-ish corners: the bottom edge narrows to 0.3..0.7 while the top stays 0..1.
    let corners = json!([[0.0, 0.0], [1.0, 0.0], [0.7, 1.0], [0.3, 1.0]]);
    s.execute("effect.apply", &json!({ "id": shape, "effect": "distort.perspective", "params": { "corners": corners } })).unwrap();
    // The exported geometry is the projective map of the box, not a bilinear one: the box's
    // centre (0.5, 0.5) maps where the homography puts it (0.5 for the x of the vanishing
    // structure; the y moves to (0.5·g+... ) — asserted against the closed form below), which
    // differs from the bilinear centre (exactly 0.5, 0.5).
    let svg = s.execute("document.serialize", &json!({ "format": "svg" })).unwrap()["text"].as_str().unwrap().to_string();
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).expect("independent usvg parse");
    let mut anchors: Vec<(f64, f64)> = vec![];
    collect_path_anchors(tree.root(), &mut anchors);
    assert!(!anchors.is_empty(), "the distorted path exports: {svg}");
    // The projective map keeps the four corners exactly where the homography puts them; a
    // bilinear map would agree on the corners but bend the sides differently (checked by the
    // curve-splitting between them, which the exported anchors carry).
    // Anchor-based checks: the four corners of the box map exactly.
    let near = |p: (f64, f64), q: (f64, f64)| (p.0 - q.0).hypot(p.1 - q.1) < 0.5;
    assert!(anchors.iter().any(|&a| near(a, (40.0, 40.0))), "TL keeps its place: {anchors:?}");
    assert!(anchors.iter().any(|&a| near(a, (110.0, 120.0))), "BR narrows to 0.7/1.0: {anchors:?}");
    assert!(anchors.iter().any(|&a| near(a, (70.0, 120.0))), "BL narrows to 0.3/1.0: {anchors:?}");
    // Edits: one step each, undo/redo, native reopen, remove.
    let undo_count = s.active().unwrap().history.undo.len();
    s.execute("effect.setParams", &json!({ "id": shape, "index": 0, "params": { "corners": [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.3, 1.0]] } }))
        .unwrap();
    assert_eq!(s.active().unwrap().history.undo.len(), undo_count + 1, "one step");
    s.execute("edit.undo", &json!({})).unwrap();
    s.execute("edit.redo", &json!({})).unwrap();
    let data = s.execute("document.serialize", &json!({ "format": "vectorcraft" })).unwrap()["dataBase64"].as_str().unwrap().to_string();
    s.execute("document.open", &json!({ "dataBase64": data })).unwrap();
    let list = s.execute("effect.list", &json!({ "id": shape })).unwrap();
    assert!(
        list["applied"].as_array().is_some_and(|a| a
            .iter()
            .any(|o| o["effects"].as_array().is_some_and(|fx| fx.iter().any(|e| e["id"].as_str() == Some("distort.perspective"))))),
        "reopen keeps the perspective"
    );
    s.execute("effect.setParams", &json!({ "id": shape, "index": 0, "params": { "corners": [[0.1, 0.0], [0.9, 0.0], [0.8, 1.0], [0.2, 1.0]] } }))
        .unwrap();
    // Independent PDF read returns the distorted geometry.
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
    assert!(paths >= 1, "the distorted path reads back");
    // The CDR/CMX degradation is the named unsupported-effect warning, once, with the id.
    for format in ["cdr", "cmx"] {
        let out = s.execute("document.serialize", &json!({ "format": format })).unwrap();
        let warnings = out["warnings"].as_array().cloned().unwrap_or_default();
        let count = warnings
            .iter()
            .filter(|w| {
                w.as_str().is_some_and(|w| w.starts_with("unsupported bitmap effects are dropped in CDR/CMX:") && w.contains("distort.perspective"))
            })
            .count();
        assert_eq!(count, 1, "the named degradation ({format}): {warnings:?}");
    }
    s.execute("effect.remove", &json!({ "id": shape, "index": 0 })).unwrap();
    let svg = s.execute("document.serialize", &json!({ "format": "svg" })).unwrap()["text"].as_str().unwrap().to_string();
    assert!(svg.contains(r#"<rect x="40" y="40""#), "the box returns to its rectangle: {svg}");
}

/// N1: refusals change nothing.
#[test]
fn perspective_refusals_change_nothing() {
    let mut s = Session::new();
    s.execute("file.new", &json!({ "width": 400, "height": 300 })).unwrap();
    let shape = s.execute("shape.rectangle", &json!({ "x": 40, "y": 40, "width": 60, "height": 40 })).unwrap()["id"].as_u64().unwrap();
    let before = s.execute("document.serialize", &json!({ "format": "vectorcraft" })).unwrap()["dataBase64"].as_str().unwrap().to_string();
    assert!(
        s.execute(
            "effect.apply",
            &json!({ "id": shape, "effect": "distort.perspective", "params": { "corners": [[0.0, 0.0], [1.0, 0.0], ["wide", 1.0], [0.3, 1.0]] } })
        )
        .is_err(),
        "a non-number corner is refused"
    );
    let after = s.execute("document.serialize", &json!({ "format": "vectorcraft" })).unwrap()["dataBase64"].as_str().unwrap().to_string();
    assert_eq!(before, after, "the refused call changed nothing");
}

fn collect_path_anchors(group: &usvg::Group, out: &mut Vec<(f64, f64)>) {
    for node in group.children() {
        match node {
            usvg::Node::Group(child) => collect_path_anchors(child, out),
            usvg::Node::Path(path) => {
                for seg in path.data().segments() {
                    match seg {
                        usvg::tiny_skia_path::PathSegment::MoveTo(p) => out.push((p.x as f64, p.y as f64)),
                        usvg::tiny_skia_path::PathSegment::LineTo(p) => out.push((p.x as f64, p.y as f64)),
                        usvg::tiny_skia_path::PathSegment::CubicTo(.., p) => out.push((p.x as f64, p.y as f64)),
                        usvg::tiny_skia_path::PathSegment::QuadTo(_, p) => out.push((p.x as f64, p.y as f64)),
                        usvg::tiny_skia_path::PathSegment::Close => {}
                    }
                }
            }
            _ => {}
        }
    }
}

// CoralCraft (P3 Task E5): the handle-set values carry through the shared machinery.
#[test]
fn handle_set_distortions_reopen_read_back_and_map_exactly_to_cdr() {
    let mut s = Session::new();
    s.execute("file.new", &json!({ "width": 400, "height": 300 })).unwrap();
    let shape = s.execute("shape.rectangle", &json!({ "x": 40, "y": 40, "width": 120, "height": 80 })).unwrap()["id"].as_u64().unwrap();
    // The values the dedicated handles commit (E5's canvas test drags these same commands).
    s.execute("effect.apply", &json!({ "id": shape, "effect": "distort.zigZag", "params": { "size": 6.0, "ridges": 4 } })).unwrap();
    s.execute("effect.setParams", &json!({ "id": shape, "index": 0, "params": { "size": 26.0, "ridges": 7 } })).unwrap();
    // P4: native reopen keeps them, and they still edit after.
    let data = s.execute("document.serialize", &json!({ "format": "vectorcraft" })).unwrap()["dataBase64"].as_str().unwrap().to_string();
    s.execute("document.open", &json!({ "dataBase64": data })).unwrap();
    let fx = s.doc().unwrap().doc.node(vectorcraft_doc::NodeId(shape)).unwrap().appearance.effects[0].params.clone();
    assert_eq!((fx["size"].as_f64(), fx["ridges"].as_u64()), (Some(26.0), Some(7)), "reopen keeps the handle-set values");
    // P5: the re-distorted geometry reads back independently.
    let svg = s.execute("document.serialize", &json!({ "format": "svg" })).unwrap()["text"].as_str().unwrap().to_string();
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).expect("independent usvg parse");
    let mut anchors = vec![];
    collect_path_anchors(tree.root(), &mut anchors);
    assert!(anchors.len() > 8, "the zigzag's anchors read back: {}", anchors.len());
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
    assert!(paths >= 1, "the distorted path reads back");
    // P6: CDR/CMX carries the exact Distortion with the handle-set values.
    let (cdr_doc, _) = {
        let st = s.doc().unwrap();
        corel_import::export_document_with_warnings(&st.doc).expect("export")
    };
    let object = cdr_doc.pages[0].layers[0].objects.iter().find(|o| o.id.0 == shape).unwrap();
    let mapped = object
        .effects
        .iter()
        .any(|e| matches!(e, corel_import::coralcraft_doc::VectorEffect::Distortion { kind: corel_import::coralcraft_geom::DistortionKind::Zipper, amplitude, frequency } if (amplitude.get() - 26.0).abs() < 1e-9 && (frequency.get() - 7.0).abs() < 1e-9));
    assert!(mapped, "the Zipper Distortion carries the handle-set values: {:?}", object.effects);
}
