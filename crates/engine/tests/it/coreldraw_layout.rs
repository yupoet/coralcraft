//! CoralCraft: P2 path text, text wrapping, frame fitting, native editability, and independent
//! SVG/PDF reader evidence. These workflows reuse the VectorCraft text and layout engine.

use serde_json::json;
use vectorcraft_doc::{AreaFit, NodeId, NodeKind, TextKind, TextObject};
use vectorcraft_engine::Session;

fn text(s: &Session, id: u64) -> TextObject {
    match &s.doc().unwrap().doc.node(NodeId(id)).unwrap().kind {
        NodeKind::Text(text) => (**text).clone(),
        _ => panic!("not text"),
    }
}

#[test]
fn coreldraw_path_text_wrap_and_fit_remain_editable() {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 500, "height": 400})).unwrap();
    let path = s.execute("path.create", &json!({"d": "M20 100 C100 20 200 20 280 100"})).unwrap()["id"].as_u64().unwrap();
    let path_text =
        s.execute("text.createInPath", &json!({"path": path, "mode": "onPath", "text": "Editable path text"})).unwrap()["id"].as_u64().unwrap();
    s.execute("text.reshapePath", &json!({"id": path_text, "anchors": [[0, 0]], "dx": 0, "dy": 20})).unwrap();
    let TextKind::OnPath { path, .. } = &text(&s, path_text).kind else { panic!("path text") };
    assert_eq!(path.subpaths[0].anchors[0].p.y, 120.0, "the text baseline path itself edited");

    let made = s
        .execute(
            "text.create",
            &json!({"x": 20, "y": 180, "text": "wrap around this brochure copy ".repeat(100), "size": 12, "area": {"width": 240, "height": 50}}),
        )
        .unwrap();
    let area = made["id"].as_u64().unwrap();
    assert!(made["overflow"].as_array().is_some_and(|ids| ids.contains(&json!(area))), "the small frame reports overflow: {made}");
    let available = |s: &Session| {
        vectorcraft_text::layout(vectorcraft_text::FontDb::global(), &text(s, area)).lines.iter().map(|line| line.avail).collect::<Vec<_>>()
    };
    let wrap = s.execute("shape.rectangle", &json!({"x": 20, "y": 190, "width": 100, "height": 35})).unwrap()["id"].as_u64().unwrap();
    s.execute("object.textWrap.make", &json!({"ids": [wrap], "offset": 6})).unwrap();
    assert!(available(&s).iter().any(|(left, _)| *left >= 105.0), "the object above displaces text in frame coordinates: {:?}", available(&s));
    s.execute("select.set", &json!({"ids": [wrap]})).unwrap();
    s.execute("object.move", &json!({"dx": 400, "dy": 0})).unwrap();
    assert!(available(&s).iter().all(|(left, _)| *left < 1.0), "moving the wrap object away reflows the text");

    let options = s.execute("text.areaOptions", &json!({"ids": [area], "fit": "shrinkText", "fitMinPercent": 30})).unwrap();
    assert_eq!(text(&s, area).area.fit, AreaFit::ShrinkText { min_percent: 30.0 });
    assert_eq!(options["overflow"], json!(true), "text beyond the 30% minimum still reports overflow: {options}");
    assert_eq!(options["fitScale"], json!(0.3), "shrink-to-fit respects its minimum scale");

    let data = s.execute("document.serialize", &json!({"format": "vectorcraft"})).unwrap()["dataBase64"].as_str().unwrap().to_string();
    s.execute("document.open", &json!({"dataBase64": data})).unwrap();
    assert_eq!(text(&s, path_text).plain_text(), "Editable path text");
    assert!(matches!(text(&s, path_text).kind, TextKind::OnPath { .. }));
    assert_eq!(text(&s, area).area.fit, AreaFit::ShrinkText { min_percent: 30.0 });
    s.execute("text.reshapePath", &json!({"id": path_text, "anchors": [[0, 0]], "dx": 5, "dy": 0})).unwrap();
    let TextKind::OnPath { path, .. } = &text(&s, path_text).kind else { panic!("path text after reopen") };
    assert_eq!(path.subpaths[0].anchors[0].p.x, 25.0, "the reopened path text remains editable");
    s.execute("text.editRange", &json!({"id": area, "start": 0, "end": 0, "insert": "Reopened. "})).unwrap();
    assert!(text(&s, area).plain_text().starts_with("Reopened. "), "the reopened frame remains editable");

    let svg = s.execute("document.serialize", &json!({"format": "svg"})).unwrap()["text"].as_str().unwrap().to_string();
    usvg::Tree::from_str(&svg, &usvg::Options::default()).expect("independent usvg parse of path and area text");
    let pdf = vectorcraft_format::base64_decode(s.execute("document.serialize", &json!({"format": "pdf"})).unwrap()["dataBase64"].as_str().unwrap())
        .unwrap();
    let reopened = vectorcraft_pdf::import(&pdf).expect("independent Hayro-backed PDF read");
    assert_eq!(reopened.artboards.len(), 1);
}

fn exported_page_text(s: &mut Session, page: usize) -> String {
    let svg = s.execute("document.serialize", &json!({"format": "svg", "artboard": page})).unwrap()["text"].as_str().unwrap().to_string();
    usvg::Tree::from_str(&svg, &usvg::Options::default()).expect("independent usvg parse of the page");
    let xml = usvg::roxmltree::Document::parse(&svg).expect("XML text evidence");
    xml.descendants()
        .filter(|node| node.is_element() && matches!(node.tag_name().name(), "text" | "tspan"))
        .filter_map(|node| node.text())
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .collect()
}

#[test]
fn coreldraw_dynamic_page_numbers_follow_pages_and_native_reopen() {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 200, "height": 100})).unwrap();
    s.execute("artboard.new", &json!({"after": 0, "name": "Two"})).unwrap();
    s.execute("artboard.new", &json!({"after": 1, "name": "Three"})).unwrap();
    s.execute("layer.new", &json!({"name": "Page Numbers", "role": "masterAll"})).unwrap();
    let field = s.execute("text.pageNumber.insert", &json!({"x": 20, "y": 80, "size": 12})).unwrap()["id"].as_u64().unwrap();
    assert_eq!(exported_page_text(&mut s, 0), "1");
    assert_eq!(exported_page_text(&mut s, 1), "2");
    assert_eq!(exported_page_text(&mut s, 2), "3");
    assert_eq!(text(&s, field).plain_text(), "#", "the stored field is not flattened to the current number");
    let (before_first, before_second) = {
        let doc = &s.doc().unwrap().doc;
        let mut renderer = vectorcraft_render::Renderer::new();
        let first = renderer.render_region(doc, doc.artboards[0].rect, 1.0, true).pixels;
        let second = renderer.render_region(doc, doc.artboards[1].rect, 1.0, true).pixels;
        (first, second)
    };
    assert!(before_first != before_second, "the initial master pages evaluate different numbers");

    s.execute("artboard.reorder", &json!({"index": 0, "to": 2, "reflowPages": true})).unwrap();
    assert_eq!(exported_page_text(&mut s, 2), "3", "reordered pages still evaluate by their current position");
    let data = s.execute("document.serialize", &json!({"format": "vectorcraft"})).unwrap()["dataBase64"].as_str().unwrap().to_string();
    s.execute("document.open", &json!({"dataBase64": data})).unwrap();
    assert_eq!(text(&s, field).plain_text(), "#");
    assert_eq!(exported_page_text(&mut s, 1), "2", "the native field remains dynamic after reopen");

    let (first, second) = {
        let doc = &s.doc().unwrap().doc;
        let mut renderer = vectorcraft_render::Renderer::new();
        let first = renderer.render_region(doc, doc.artboards[0].rect, 1.0, true).pixels;
        let second = renderer.render_region(doc, doc.artboards[1].rect, 1.0, true).pixels;
        (first, second)
    };
    assert!(
        first != second,
        "the canvas renderer evaluates each page's number; pages: {:?}; ink: {} / {}",
        s.doc().unwrap().doc.artboards.iter().map(|page| page.rect).collect::<Vec<_>>(),
        first.iter().filter(|&&channel| channel != 255).count(),
        second.iter().filter(|&&channel| channel != 255).count()
    );

    for page in 0..3 {
        let bytes =
            vectorcraft_pdf::export(&s.doc().unwrap().doc, &vectorcraft_pdf::PdfOptions { artboards: Some(vec![page]), ..Default::default() })
                .unwrap();
        let imported = vectorcraft_pdf::import(&bytes).expect("independent Hayro-backed page-number PDF read");
        let mut texts = Vec::new();
        for layer in &imported.layers {
            layer.walk(&mut |node| {
                if let NodeKind::Text(text) = &node.kind {
                    texts.push(text.plain_text());
                }
            });
        }
        assert!(texts.iter().any(|text| text == &(page + 1).to_string()), "page {} exports its evaluated number: {texts:?}", page + 1);
    }
}

#[test]
fn coreldraw_page_number_cdr_fallback_warns_and_writes_page_text() {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 200, "height": 100})).unwrap();
    s.execute("artboard.new", &json!({"after": 0, "name": "Two"})).unwrap();
    s.execute("layer.new", &json!({"name": "Page Numbers", "role": "masterAll"})).unwrap();
    s.execute("text.pageNumber.insert", &json!({"x": 20, "y": 80, "size": 12})).unwrap();
    let exported = s.execute("document.serialize", &json!({"format": "cdr"})).unwrap();
    assert!(
        exported["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|warning| warning.as_str().is_some_and(|text| text.contains("page-number") && text.contains("native"))),
        "the CDR fallback is explicit: {exported:?}"
    );
    s.execute("document.open", &json!({"name": "pages.cdr", "dataBase64": exported["dataBase64"].as_str().unwrap()})).unwrap();
    let mut texts = Vec::new();
    for layer in &s.doc().unwrap().doc.layers {
        layer.walk(&mut |node| {
            if let NodeKind::Text(text) = &node.kind {
                texts.push(text.plain_text());
            }
        });
    }
    assert!(
        texts.iter().any(|text| text == "1") && texts.iter().any(|text| text == "2"),
        "each CDR page carries its evaluated editable text: {texts:?}"
    );
    assert!(!texts.iter().any(|text| text == "#"), "the dynamic placeholder is not written as visible content");
}
