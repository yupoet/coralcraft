//! CoralCraft: P3 poster + traced-image acceptance with native revision and independent readers.

use serde_json::json;
use vectorcraft_doc::{NodeId, NodeKind};
use vectorcraft_engine::Session;

fn native_reopen(s: &mut Session) {
    let data = s.execute("document.serialize", &json!({"format": "vectorcraft"})).unwrap()["dataBase64"].as_str().unwrap().to_string();
    s.execute("document.open", &json!({"dataBase64": data})).unwrap();
}

#[test]
fn p3_acceptance_revises_a_poster_and_traced_composition() {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 420, "height": 300})).unwrap();
    let contents = s.execute("shape.rectangle", &json!({"x": 25, "y": 60, "width": 150, "height": 100})).unwrap()["id"].as_u64().unwrap();
    s.execute("effect.apply", &json!({"id": contents, "effect": "threeD.extrude", "params": {"depth": 30}})).unwrap();
    s.execute("effect.apply", &json!({"id": contents, "effect": "stylize.dropShadow", "params": {"x": 6, "y": 5}})).unwrap();
    let frame = s.execute("shape.ellipse", &json!({"x": 40, "y": 70, "width": 100, "height": 80})).unwrap()["id"].as_u64().unwrap();
    s.execute("select.set", &json!({"ids": [contents, frame]})).unwrap();
    let clip = s.execute("object.clippingMask.make", &json!({})).unwrap()["id"].as_u64().unwrap();
    let title = s.execute("text.create", &json!({"x": 25, "y": 30, "text": "Editable poster"})).unwrap()["id"].as_u64().unwrap();

    let a = s.execute("shape.ellipse", &json!({"x": 220, "y": 55, "width": 24, "height": 24})).unwrap()["id"].as_u64().unwrap();
    let b = s.execute("shape.ellipse", &json!({"x": 340, "y": 100, "width": 40, "height": 40})).unwrap()["id"].as_u64().unwrap();
    let blend = s.execute("object.blend.make", &json!({"ids": [a, b], "steps": 3})).unwrap()["id"].as_u64().unwrap();

    let mut png = std::io::Cursor::new(Vec::new());
    image::RgbaImage::from_fn(24, 24, |x, y| {
        if (5..19).contains(&x) && (4..20).contains(&y) { image::Rgba([30, 120, 180, 255]) } else { image::Rgba([255; 4]) }
    })
    .write_to(&mut png, image::ImageFormat::Png)
    .unwrap();
    let bitmap = s
        .execute(
            "file.place",
            &json!({"name": "synthetic.png", "dataBase64": vectorcraft_format::base64_encode(&png.into_inner()), "link": false, "at": [220, 180]}),
        )
        .unwrap()["ids"][0]
        .as_u64()
        .unwrap();
    let trace = s.execute("imageTrace.make", &json!({"id": bitmap, "params": {"ignoreWhite": true}})).unwrap()["id"].as_u64().unwrap();
    assert!(s.execute("imageTrace.makeAndExpand", &json!({"id": trace})).unwrap()["paths"].as_u64().unwrap() > 0);

    native_reopen(&mut s);
    s.execute("effect.setParams", &json!({"id": contents, "index": 0, "params": {"depth": 55}})).unwrap();
    s.execute("paint.setFill", &json!({"id": contents, "color": "#ff0000"})).unwrap();
    s.execute("text.setText", &json!({"id": title, "text": "Editable poster — revised"})).unwrap();
    native_reopen(&mut s);

    assert_eq!(s.doc().unwrap().doc.node(NodeId(clip)).unwrap().kind_label(), "Clip Group");
    assert!(matches!(s.doc().unwrap().doc.node(NodeId(blend)).unwrap().kind, NodeKind::Blend { .. }));
    assert_eq!(s.doc().unwrap().doc.node(NodeId(contents)).unwrap().appearance.effects[0].params["depth"].as_f64(), Some(55.0));
    let NodeKind::Text(text) = &s.doc().unwrap().doc.node(NodeId(title)).unwrap().kind else { panic!("live title") };
    assert_eq!(text.plain_text(), "Editable poster — revised");

    let svg = s.execute("document.serialize", &json!({"format": "svg"})).unwrap()["text"].as_str().unwrap().to_string();
    usvg::Tree::from_str(&svg, &usvg::Options::default()).expect("independent usvg parse of the P3 poster");
    assert!(svg.contains("<clipPath"), "PowerClip remains visible in the SVG");
    let pdf = vectorcraft_format::base64_decode(s.execute("document.serialize", &json!({"format": "pdf"})).unwrap()["dataBase64"].as_str().unwrap())
        .unwrap();
    assert_eq!(vectorcraft_pdf::import(&pdf).expect("independent Hayro-backed P3 PDF read").artboards.len(), 1);

    let cdr = s.execute("document.serialize", &json!({"format": "cdr"})).unwrap();
    assert!(
        cdr["warnings"].as_array().unwrap().iter().any(|warning| warning.as_str().is_some_and(|text| text.contains("live Blend"))),
        "CDR fallback is explicit: {cdr:?}"
    );
    assert!(
        matches!(s.doc().unwrap().doc.node(NodeId(blend)).unwrap().kind, NodeKind::Blend { .. }),
        "export does not mutate the native live source"
    );
}
