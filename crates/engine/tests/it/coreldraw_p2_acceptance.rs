//! CoralCraft: P2 brochure acceptance. Build, natively reopen, revise, reorder, and export a
//! three-page document, then use independent SVG and Hayro-backed PDF readers.

use serde_json::json;
use vectorcraft_doc::{Document, NodeId, NodeKind};
use vectorcraft_engine::Session;

fn texts(doc: &Document) -> Vec<String> {
    let mut out = Vec::new();
    for layer in &doc.layers {
        layer.walk(&mut |node| {
            if let NodeKind::Text(text) = &node.kind {
                out.push(text.plain_text());
            }
        });
    }
    out
}

fn find_text(s: &Session, wanted: &str) -> u64 {
    let mut found = None;
    for layer in &s.doc().unwrap().doc.layers {
        layer.walk(&mut |node| {
            if matches!(&node.kind, NodeKind::Text(text) if text.plain_text() == wanted) {
                found = Some(node.id.0);
            }
        });
    }
    found.unwrap_or_else(|| panic!("no text {wanted:?}"))
}

fn native_reopen(s: &mut Session) {
    let data = s.execute("document.serialize", &json!({"format": "vectorcraft"})).unwrap()["dataBase64"].as_str().unwrap().to_string();
    s.execute("document.open", &json!({"dataBase64": data})).unwrap();
}

#[test]
fn p2_acceptance_builds_revises_and_reads_a_three_page_brochure() {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 400, "height": 300})).unwrap();
    s.execute("artboard.new", &json!({"after": 0, "name": "Inside"})).unwrap();
    s.execute("artboard.new", &json!({"after": 1, "name": "Back"})).unwrap();
    s.execute("layer.new", &json!({"name": "Folio", "role": "masterAll"})).unwrap();
    let field = s.execute("text.pageNumber.insert", &json!({"x": 20, "y": 280, "size": 12})).unwrap()["id"].as_u64().unwrap();

    s.execute("layer.new", &json!({"name": "Brochure"})).unwrap();
    let title = s.execute("text.create", &json!({"x": 20, "y": 40, "text": "Coral"})).unwrap()["id"].as_u64().unwrap();
    s.execute("charStyle.new", &json!({"name": "Brochure Title", "attrs": {"size": 32.0, "tracking": 20.0}})).unwrap();
    s.execute("charStyle.apply", &json!({"name": "Brochure Title", "id": title})).unwrap();
    let story = "Editable linked body copy carries the brochure introduction through both frames. ".repeat(8);
    let first =
        s.execute("text.create", &json!({"x": 20, "y": 65, "text": story, "area": {"width": 130, "height": 55}})).unwrap()["id"].as_u64().unwrap();
    let second =
        s.execute("text.create", &json!({"x": 170, "y": 65, "text": "", "area": {"width": 130, "height": 100}})).unwrap()["id"].as_u64().unwrap();
    s.execute("select.set", &json!({"ids": [first, second]})).unwrap();
    s.execute("text.thread.create", &json!({})).unwrap();
    let wrap = s.execute("shape.ellipse", &json!({"x": 45, "y": 90, "width": 55, "height": 45})).unwrap()["id"].as_u64().unwrap();
    s.execute("object.textWrap.make", &json!({"ids": [wrap], "offset": 6})).unwrap();
    let path = s.execute("path.create", &json!({"d": "M20 180 C100 120 220 120 300 180"})).unwrap()["id"].as_u64().unwrap();
    let path_text =
        s.execute("text.createInPath", &json!({"path": path, "mode": "onPath", "text": "Editable path label"})).unwrap()["id"].as_u64().unwrap();
    let table = s
        .execute(
            "layout.table",
            &json!({"x": 210, "y": 200, "width": 150, "height": 60, "rows": 2, "columns": 2, "text": [["Cell", "B"], ["C", "D"]]}),
        )
        .unwrap()["id"]
        .as_u64()
        .unwrap();

    let mut png = std::io::Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(4, 4, image::Rgba([30, 120, 180, 255])).write_to(&mut png, image::ImageFormat::Png).unwrap();
    let placed = s
        .execute(
            "file.place",
            &json!({"name": "synthetic.png", "dataBase64": vectorcraft_format::base64_encode(&png.into_inner()), "link": false, "at": [320, 230]}),
        )
        .unwrap();
    assert!(placed["ids"].as_array().is_some_and(|ids| !ids.is_empty()), "the synthetic bitmap is embedded");

    native_reopen(&mut s);
    s.execute("text.setText", &json!({"id": title, "text": "Coral Revised"})).unwrap();
    s.execute("text.editRange", &json!({"id": first, "start": 0, "end": 0, "insert": "Revised body. "})).unwrap();
    let cell = find_text(&s, "Cell");
    s.execute("text.setText", &json!({"id": cell, "text": "Cell Revised"})).unwrap();
    s.execute("text.reshapePath", &json!({"id": path_text, "anchors": [[0, 0]], "dx": 5, "dy": 0})).unwrap();
    assert!(matches!(s.doc().unwrap().doc.node(NodeId(path_text)).unwrap().kind, NodeKind::Text(_)));
    assert_eq!(s.execute("text.thread.info", &json!({})).unwrap(), json!([[first, second]]));
    assert!(s.doc().unwrap().doc.node(NodeId(table)).is_some());

    s.execute("artboard.reorder", &json!({"index": 0, "to": 2, "reflowPages": true})).unwrap();
    native_reopen(&mut s);
    assert_eq!(s.doc().unwrap().doc.artboards.len(), 3);
    let NodeKind::Text(field_text) = &s.doc().unwrap().doc.node(NodeId(field)).unwrap().kind else { panic!("page field is text") };
    assert_eq!(field_text.plain_text(), "#", "the page field stayed dynamic");
    assert!(texts(&s.doc().unwrap().doc).iter().any(|text| text == "Cell Revised"));

    let pdf = vectorcraft_format::base64_decode(s.execute("document.serialize", &json!({"format": "pdf"})).unwrap()["dataBase64"].as_str().unwrap())
        .unwrap();
    let imported = vectorcraft_pdf::import(&pdf).expect("independent Hayro-backed brochure PDF read");
    assert_eq!(imported.artboards.len(), 3, "the PDF has all three pages");
    let imported_text = texts(&imported);
    for expected in ["Coral Revised", "Cell Revised", "Editable path label"] {
        assert!(imported_text.iter().any(|text| text == expected), "PDF retains editable {expected:?}: {imported_text:?}");
    }
    assert!(imported_text.iter().any(|text| text.starts_with("Revised body. ")), "PDF retains the revised linked story");
    for page in ["1", "2", "3"] {
        assert!(imported_text.iter().any(|text| text == page), "PDF contains evaluated page {page}");
    }
    let mut images = 0;
    for layer in &imported.layers {
        layer.walk(&mut |node| images += usize::from(matches!(node.kind, NodeKind::Image(_))));
    }
    assert!(images >= 1, "the PDF reader reconstructed the embedded bitmap");

    let svg = s.execute("document.serialize", &json!({"format": "svg", "artboard": 2})).unwrap()["text"].as_str().unwrap().to_string();
    usvg::Tree::from_str(&svg, &usvg::Options::default()).expect("independent usvg parse of the reordered brochure page");
}
