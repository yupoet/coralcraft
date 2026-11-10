//! CoralCraft: P3 native/CDR/CMX persistence and fallback audit (P3 Task 10).

use serde_json::json;
use vectorcraft_doc::{Document, NodeKind};
use vectorcraft_engine::Session;

fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 300, "height": 200})).unwrap();
    s
}

fn rectangle(s: &mut Session, x: f64) -> u64 {
    s.execute("shape.rectangle", &json!({"x": x, "y": 30, "width": 60, "height": 50})).unwrap()["id"].as_u64().unwrap()
}

fn reopen(s: &mut Session, format: &str) -> serde_json::Value {
    let exported = s.execute("document.serialize", &json!({"format": format})).unwrap();
    s.execute("document.open", &json!({"name": format!("p3.{format}"), "dataBase64": exported["dataBase64"].as_str().unwrap()})).unwrap();
    exported
}

fn inspect(doc: &Document) -> (usize, Vec<String>, usize) {
    let mut clips = 0;
    let mut effects = Vec::new();
    let mut nonempty_paths = 0;
    for layer in &doc.layers {
        layer.walk(&mut |node| {
            if matches!(node.kind, NodeKind::Group { clip: true, .. }) {
                clips += 1;
            }
            if matches!(node.kind, NodeKind::Path { .. }) && node.path_data().is_some_and(|path| !path.subpaths.is_empty()) {
                nonempty_paths += 1;
            }
            effects.extend(node.appearance.effects.iter().map(|effect| effect.id.clone()));
        });
    }
    (clips, effects, nonempty_paths)
}

#[test]
fn cdr_and_cmx_reopen_powerclip_with_supported_live_effects() {
    for format in ["cdr", "cmx"] {
        let mut s = session();
        let contents = rectangle(&mut s, 20.0);
        s.execute("effect.apply", &json!({"id": contents, "effect": "threeD.extrude", "params": {"depth": 35}})).unwrap();
        s.execute("effect.apply", &json!({"id": contents, "effect": "stylize.dropShadow", "params": {"x": 6, "y": 4}})).unwrap();
        let frame = s.execute("shape.ellipse", &json!({"x": 30, "y": 35, "width": 40, "height": 35})).unwrap()["id"].as_u64().unwrap();
        s.execute("select.set", &json!({"ids": [contents, frame]})).unwrap();
        s.execute("object.clippingMask.make", &json!({})).unwrap();

        reopen(&mut s, format);
        let (clips, effects, _) = inspect(&s.doc().unwrap().doc);
        assert_eq!(clips, 1, "{format} keeps the PowerClip structure");
        assert!(effects.contains(&"threeD.extrude".to_owned()), "{format} effects: {effects:?}");
        assert!(effects.contains(&"stylize.dropShadow".to_owned()), "{format} effects: {effects:?}");
    }
}

#[test]
fn cdr_live_blend_export_warns_and_keeps_visible_vector_geometry() {
    let mut s = session();
    let a = rectangle(&mut s, 20.0);
    let b = rectangle(&mut s, 180.0);
    s.execute("object.blend.make", &json!({"ids": [a, b], "steps": 3})).unwrap();
    let exported = s.execute("document.serialize", &json!({"format": "cdr"})).unwrap();
    let warnings = exported["warnings"].as_array().unwrap();
    assert!(
        warnings.iter().any(|warning| warning.as_str().is_some_and(|text| text.contains("live") && text.contains("Blend"))),
        "explicit live-effect fallback warning: {warnings:?}"
    );
    s.execute("document.open", &json!({"name": "blend.cdr", "dataBase64": exported["dataBase64"].as_str().unwrap()})).unwrap();
    let (_, _, paths) = inspect(&s.doc().unwrap().doc);
    assert!(paths > 0, "the fallback retains visible vector paths rather than an empty path");
}
