//! CoralCraft: bitmap and PowerTRACE workflows over the shared tracer (P3 Tasks 8–9).

use serde_json::json;
use vectorcraft_doc::{NodeId, NodeKind, TraceView};
use vectorcraft_engine::Session;

fn native_reopen(s: &mut Session) {
    let data = s.execute("document.serialize", &json!({"format": "vectorcraft"})).unwrap()["dataBase64"].as_str().unwrap().to_string();
    s.execute("document.open", &json!({"dataBase64": data})).unwrap();
}

#[test]
fn powertrace_reopens_live_and_expands_to_editable_paths() {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 200, "height": 200})).unwrap();
    let mut png = std::io::Cursor::new(Vec::new());
    image::RgbaImage::from_fn(24, 24, |x, y| {
        if (5..19).contains(&x) && (4..20).contains(&y) { image::Rgba([20, 40, 80, 255]) } else { image::Rgba([255; 4]) }
    })
    .write_to(&mut png, image::ImageFormat::Png)
    .unwrap();
    let placed = s
        .execute(
            "file.place",
            &json!({"name": "synthetic.png", "dataBase64": vectorcraft_format::base64_encode(&png.into_inner()), "link": false, "at": [20, 20]}),
        )
        .unwrap();
    let bitmap = placed["ids"][0].as_u64().unwrap();
    assert!(matches!(s.doc().unwrap().doc.node(NodeId(bitmap)).unwrap().kind, NodeKind::Image(_)));

    let made = s.execute("imageTrace.make", &json!({"id": bitmap, "params": {"mode": "color", "ignoreWhite": true}})).unwrap();
    let trace = made["id"].as_u64().unwrap();
    assert!(made["paths"].as_u64().unwrap() > 0, "{made}");
    assert!(matches!(s.doc().unwrap().doc.node(NodeId(trace)).unwrap().kind, NodeKind::Group { .. }));
    s.execute("imageTrace.setView", &json!({"id": trace, "view": "outlines"})).unwrap();
    native_reopen(&mut s);
    assert_eq!(s.doc().unwrap().doc.node(NodeId(trace)).unwrap().trace_view(), TraceView::Outlines);
    assert!(s.doc().unwrap().doc.node(NodeId(trace)).unwrap().extras().trace.is_some());

    let expanded = s.execute("imageTrace.makeAndExpand", &json!({"id": trace})).unwrap();
    assert!(expanded["paths"].as_u64().unwrap() > 0, "{expanded}");
    native_reopen(&mut s);
    let paths = s.execute("document.inspect", &json!({"kind": "Path"})).unwrap();
    assert!(!paths["nodes"].as_array().unwrap().is_empty(), "expanded paths survive native reopen");
    let first = paths["nodes"][0]["id"].as_u64().unwrap();
    s.execute("select.set", &json!({"ids": [first]})).unwrap();
    s.execute("paint.setFill", &json!({"color": "#ff0000"})).unwrap();
    assert_eq!(s.doc().unwrap().doc.node(NodeId(first)).unwrap().appearance.fill_paint().color().unwrap().to_hex(), "#ff0000");
    let svg = s.execute("document.serialize", &json!({"format": "svg"})).unwrap()["text"].as_str().unwrap().to_string();
    usvg::Tree::from_str(&svg, &usvg::Options::default()).expect("independent usvg parse of expanded PowerTRACE paths");
    assert!(svg.contains("fill=\"#ff0000\""), "the edited traced path is exported: {svg}");
}

#[test]
fn bitmap_crop_and_colour_adjustment_survive_undo_and_native_reopen() {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 200, "height": 200})).unwrap();
    let mut png = std::io::Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(24, 24, image::Rgba([30, 120, 180, 255])).write_to(&mut png, image::ImageFormat::Png).unwrap();
    let bitmap = s
        .execute(
            "file.place",
            &json!({"name": "synthetic.png", "dataBase64": vectorcraft_format::base64_encode(&png.into_inner()), "link": false, "at": [40, 40]}),
        )
        .unwrap()["ids"][0]
        .as_u64()
        .unwrap();
    let cropped = s.execute("object.cropImage", &json!({"id": bitmap, "rect": [30, 30, 12, 12]})).unwrap();
    assert_eq!((cropped["width"].as_u64(), cropped["height"].as_u64()), (Some(12), Some(12)));
    s.execute("edit.undo", &json!({})).unwrap();
    s.execute("edit.redo", &json!({})).unwrap();
    s.execute("effect.apply", &json!({"id": bitmap, "effect": "adjust.brightnessContrast", "params": {"brightness": 10, "contrast": 20}})).unwrap();
    native_reopen(&mut s);
    let node = s.doc().unwrap().doc.node(NodeId(bitmap)).unwrap();
    assert!(matches!(node.kind, NodeKind::Image(_)));
    assert_eq!(node.appearance.effects[0].id, "adjust.brightnessContrast");
    assert_eq!(node.appearance.effects[0].params["contrast"].as_f64(), Some(20.0));
    let svg = s.execute("document.serialize", &json!({"format": "svg"})).unwrap()["text"].as_str().unwrap().to_string();
    usvg::Tree::from_str(&svg, &usvg::Options::default()).expect("independent usvg parse of the adjusted bitmap");
}
