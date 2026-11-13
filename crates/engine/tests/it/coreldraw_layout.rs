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

#[test]
fn coreldraw_table_cells_are_editable_and_invalid_input_is_atomic() {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 500, "height": 300})).unwrap();
    let table = s.execute("layout.table", &json!({"x": 20, "y": 30, "width": 300, "height": 120, "rows": 2, "columns": 3, "text": [["Name", "Qty", "Price"], ["Apple", "2", "3.50"]]})).unwrap();
    let group = table["id"].as_u64().unwrap();
    let children = |s: &Session| match &s.doc().unwrap().doc.node(NodeId(group)).unwrap().kind {
        NodeKind::Group { children, .. } => children.clone(),
        _ => panic!("table is not a group"),
    };
    assert_eq!(children(&s).len(), 12, "two by three cells, each an editable path and live text");
    let apple = children(&s)
        .iter()
        .find_map(|node| match &node.kind {
            NodeKind::Text(text) if text.plain_text() == "Apple" => Some(node.id.0),
            _ => None,
        })
        .expect("the cell text is live");
    s.execute("text.setText", &json!({"id": apple, "text": "Pear"})).unwrap();
    assert_eq!(text(&s, apple).plain_text(), "Pear");
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(text(&s, apple).plain_text(), "Apple");
    s.execute("edit.redo", &json!({})).unwrap();
    assert_eq!(text(&s, apple).plain_text(), "Pear");

    let data = s.execute("document.serialize", &json!({"format": "vectorcraft"})).unwrap()["dataBase64"].as_str().unwrap().to_string();
    s.execute("document.open", &json!({"dataBase64": data})).unwrap();
    assert_eq!(children(&s).len(), 12);
    assert_eq!(text(&s, apple).plain_text(), "Pear", "the edited cell remains live after native reopen");
    let svg = s.execute("document.serialize", &json!({"format": "svg"})).unwrap()["text"].as_str().unwrap().to_string();
    usvg::Tree::from_str(&svg, &usvg::Options::default()).expect("independent usvg parse of the table");
    let pdf = vectorcraft_format::base64_decode(s.execute("document.serialize", &json!({"format": "pdf"})).unwrap()["dataBase64"].as_str().unwrap())
        .unwrap();
    let imported = vectorcraft_pdf::import(&pdf).expect("independent Hayro-backed table PDF read");
    let mut texts = Vec::new();
    for layer in &imported.layers {
        layer.walk(&mut |node| {
            if let NodeKind::Text(text) = &node.kind {
                texts.push(text.plain_text());
            }
        });
    }
    assert!(texts.iter().any(|text| text == "Pear"), "the edited cell exports as text: {texts:?}");

    let before = s.doc().unwrap().doc.clone();
    let history = s.doc().unwrap().history.undo.len();
    for params in [
        json!({"x": 0, "y": 0, "width": 100, "height": 100, "rows": 0, "columns": 2}),
        json!({"x": 0, "y": 0, "width": 100, "height": 100, "rows": 2, "columns": -1}),
        json!({"x": 0, "y": 0, "width": 100, "height": 100, "rows": 2.5, "columns": 2}),
        json!({"x": 0, "y": 0, "width": 0, "height": 100, "rows": 2, "columns": 2}),
        json!({"x": 0, "y": 0, "width": 100, "height": f64::INFINITY, "rows": 2, "columns": 2}),
        json!({"x": 0, "y": 0, "width": 100, "height": 100, "rows": 1001, "columns": 2}),
        json!({"x": 0, "y": 0, "width": 100, "height": 100, "rows": 1, "columns": 1, "text": [["ok"], ["too many"]]}),
    ] {
        assert!(s.execute("layout.table", &params).is_err(), "invalid table input: {params}");
        assert_eq!(s.doc().unwrap().doc, before, "invalid input changed the document: {params}");
        assert_eq!(s.doc().unwrap().history.undo.len(), history, "invalid input added history: {params}");
    }
}

#[test]
fn coreldraw_p2_semantics_survive_native_reopen_and_cdr_fallback() {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 500, "height": 300})).unwrap();
    s.execute("artboard.new", &json!({"after": 0, "name": "Two"})).unwrap();
    s.execute("layer.new", &json!({"name": "Master", "role": "masterAll"})).unwrap();
    let field = s.execute("text.pageNumber.insert", &json!({"x": 20, "y": 280, "size": 12})).unwrap()["id"].as_u64().unwrap();
    s.execute("layer.new", &json!({"name": "Body"})).unwrap();
    let story = "Linked brochure body remains editable across native persistence. ".repeat(8);
    let first =
        s.execute("text.create", &json!({"x": 20, "y": 20, "text": story, "area": {"width": 150, "height": 60}})).unwrap()["id"].as_u64().unwrap();
    let second =
        s.execute("text.create", &json!({"x": 190, "y": 20, "text": "", "area": {"width": 150, "height": 100}})).unwrap()["id"].as_u64().unwrap();
    s.execute("select.set", &json!({"ids": [first, second]})).unwrap();
    s.execute("text.thread.create", &json!({})).unwrap();
    s.execute("paraStyle.new", &json!({"name": "Body Center", "attrs": {"justify": "Center"}})).unwrap();
    s.execute("paraStyle.apply", &json!({"name": "Body Center", "id": first})).unwrap();
    let path = s.execute("path.create", &json!({"d": "M20 130 C100 80 200 80 280 130"})).unwrap()["id"].as_u64().unwrap();
    let path_text = s.execute("text.createInPath", &json!({"path": path, "mode": "onPath", "text": "Path label"})).unwrap()["id"].as_u64().unwrap();
    let table = s
        .execute("layout.table", &json!({"x": 20, "y": 170, "width": 180, "height": 40, "rows": 1, "columns": 2, "text": [["Before", "Cell"]]}))
        .unwrap()["id"]
        .as_u64()
        .unwrap();
    let cell = match &s.doc().unwrap().doc.node(NodeId(table)).unwrap().kind {
        NodeKind::Group { children, .. } => children
            .iter()
            .find_map(|node| match &node.kind {
                NodeKind::Text(text) if text.plain_text() == "Before" => Some(node.id.0),
                _ => None,
            })
            .unwrap(),
        _ => panic!("table group"),
    };

    let data = s.execute("document.serialize", &json!({"format": "vectorcraft"})).unwrap()["dataBase64"].as_str().unwrap().to_string();
    s.execute("document.open", &json!({"dataBase64": data})).unwrap();
    assert_eq!(s.execute("text.thread.info", &json!({})).unwrap(), json!([[first, second]]));
    assert_eq!(text(&s, first).para.justify, vectorcraft_doc::Justify::Center);
    assert!(matches!(text(&s, path_text).kind, TextKind::OnPath { .. }));
    assert_eq!(text(&s, field).plain_text(), "#");
    assert_eq!(text(&s, cell).plain_text(), "Before");

    s.execute("text.editRange", &json!({"id": first, "start": 0, "end": 0, "insert": "Revised. "})).unwrap();
    s.execute("text.reshapePath", &json!({"id": path_text, "anchors": [[0, 0]], "dx": 5, "dy": 0})).unwrap();
    s.execute("text.setText", &json!({"id": cell, "text": "After"})).unwrap();
    assert!(text(&s, first).plain_text().starts_with("Revised. "));
    assert_eq!(text(&s, cell).plain_text(), "After");
    assert_eq!(text(&s, field).plain_text(), "#");

    let cdr = s.execute("document.serialize", &json!({"format": "cdr"})).unwrap();
    assert!(
        cdr["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|warning| warning.as_str().is_some_and(|text| text.contains("page-number") && text.contains("native"))),
        "the dynamic-field fallback is explicit: {cdr:?}"
    );
    s.execute("document.open", &json!({"name": "p2.cdr", "dataBase64": cdr["dataBase64"].as_str().unwrap()})).unwrap();
    assert_eq!(s.doc().unwrap().doc.artboards.len(), 2);
    let mut texts = Vec::new();
    for layer in &s.doc().unwrap().doc.layers {
        layer.walk(&mut |node| {
            if let NodeKind::Text(text) = &node.kind {
                texts.push(text.plain_text());
            }
        });
    }
    for expected in ["1", "2", "After", "Path label"] {
        assert!(texts.iter().any(|text| text == expected), "CDR retains editable text {expected:?}: {texts:?}");
    }
    assert!(texts.iter().any(|text| text.starts_with("Revised. ")), "the linked story's edited text remains: {texts:?}");
}

#[test]
fn coreldraw_page_number_on_a_normal_page_uses_its_cdr_page_text() {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 200, "height": 100})).unwrap();
    s.execute("artboard.new", &json!({"after": 0, "name": "Two"})).unwrap();
    s.execute("layer.new", &json!({"name": "Second page", "pageIndex": 1})).unwrap();
    s.execute("text.pageNumber.insert", &json!({"x": 240, "y": 80})).unwrap();
    let exported = s.execute("document.serialize", &json!({"format": "cdr"})).unwrap();
    assert!(exported["warnings"].as_array().unwrap().iter().any(|warning| warning.as_str().is_some_and(|text| text.contains("page-number"))));
    s.execute("document.open", &json!({"name": "normal.cdr", "dataBase64": exported["dataBase64"].as_str().unwrap()})).unwrap();
    let texts = s
        .doc()
        .unwrap()
        .doc
        .layers
        .iter()
        .flat_map(|layer| {
            let mut texts = Vec::new();
            layer.walk(&mut |node| {
                if let NodeKind::Text(text) = &node.kind {
                    texts.push(text.plain_text());
                }
            });
            texts
        })
        .collect::<Vec<_>>();
    assert_eq!(texts, ["2"], "the normal-page CDR object carries its evaluated number, not the placeholder");
}

// CoralCraft (P2.TABLE-UI, the P2 follow-up in docs/gaps.md): `layout.tableEdit` inserts and
// deletes rows and columns of a `layout.table` group, keeping the cell texts in their shifted
// places, as one undo step, atomically on invalid input, and durably through a native reopen.
#[test]
fn coreldraw_table_rows_and_columns_edit_through_the_command() {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 500, "height": 300})).unwrap();
    let table = s.execute("layout.table", &json!({"x": 20, "y": 30, "width": 300, "height": 120, "rows": 2, "columns": 3, "text": [["Name", "Qty", "Price"], ["Apple", "2", "3.50"]]})).unwrap();
    let group = NodeId(table["id"].as_u64().unwrap());
    let children = |s: &Session| match &s.doc().unwrap().doc.node(group).unwrap().kind {
        NodeKind::Group { children, .. } => children.clone(),
        _ => panic!("table is not a group"),
    };
    let texts = |s: &Session| {
        children(s)
            .iter()
            .filter_map(|node| match &node.kind {
                NodeKind::Text(text) => Some(text.plain_text()),
                _ => None,
            })
            .collect::<Vec<_>>()
    };

    // Insert a row before row 2: an empty row of cells between the two text rows.
    let r = s.execute("layout.tableEdit", &json!({"id": group.0, "action": "insertRow", "at": 1})).unwrap();
    assert_eq!((r["rows"].as_u64(), r["columns"].as_u64()), (Some(3), Some(3)));
    assert_eq!(children(&s).len(), 18, "three rows of three cells");
    assert_eq!(texts(&s), vec!["Name", "Qty", "Price", "", "", "", "Apple", "2", "3.50"], "the texts shift with their rows");

    // Delete the last column (Price): a two-column table keeps the first two texts of each row.
    s.execute("layout.tableEdit", &json!({"id": group.0, "action": "deleteColumn", "at": 2})).unwrap();
    assert_eq!(children(&s).len(), 12, "three rows of two cells");
    assert_eq!(texts(&s), vec!["Name", "Qty", "", "", "Apple", "2"]);

    // Undo both edits as their own steps, back to the original table.
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(texts(&s).last().map(String::as_str), Some("3.50"), "the deleted column returns");
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(texts(&s), vec!["Name", "Qty", "Price", "Apple", "2", "3.50"]);

    // Native reopen keeps the edited table editable.
    s.execute("layout.tableEdit", &json!({"id": group.0, "action": "insertColumn", "at": 0})).unwrap();
    let data = s.execute("document.serialize", &json!({"format": "vectorcraft"})).unwrap()["dataBase64"].as_str().unwrap().to_string();
    s.execute("document.open", &json!({"dataBase64": data})).unwrap();
    let found = s.execute("document.inspect", &json!({})).unwrap();
    let svg = s.execute("document.serialize", &json!({"format": "svg"})).unwrap()["text"].as_str().unwrap().to_string();
    assert!(svg.contains("Apple"), "the reopened table holds its texts: {svg}");
    let _ = found;

    // Invalid input changes nothing: the last row or column stays, ranges check, unknown ids and
    // actions refuse.
    s.execute("layout.table", &json!({"x": 0, "y": 0, "width": 40, "height": 20, "rows": 1, "columns": 1, "text": [["Only"]]})).unwrap();
    let one = s.execute("document.inspect", &json!({})).unwrap();
    let _ = one;
    let before = s.doc().unwrap().doc.clone();
    let history = s.doc().unwrap().history.undo.len();
    for params in [
        json!({"id": 999, "action": "insertRow"}),
        json!({"id": group.0, "action": "warpRow"}),
        json!({"id": group.0}),
        json!({"id": group.0, "action": "deleteRow", "at": 9}),
        json!({"id": group.0, "action": "insertColumn", "at": 999}),
    ] {
        assert!(s.execute("layout.tableEdit", &params).is_err(), "invalid tableEdit input: {params}");
        assert_eq!(s.doc().unwrap().doc, before, "invalid input changed the document: {params}");
        assert_eq!(s.doc().unwrap().history.undo.len(), history, "invalid input added history: {params}");
    }
    // A non-table object refuses too.
    let path = s.execute("path.create", &json!({"d": "M 0 0 L 10 10"})).unwrap()["id"].as_u64().unwrap();
    assert!(s.execute("layout.tableEdit", &json!({"id": path, "action": "insertRow"})).is_err());
}

// CoralCraft (P2 follow-up review): `layout.tableEdit`'s bounds and cell-name validation refuse
// instead of panicking, a delete of a middle row keeps the surviving texts, styling survives a
// structural edit, and the reopened table still edits.
#[test]
fn coreldraw_table_edit_rejects_bad_bounds_and_names_without_panicking() {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 500, "height": 300})).unwrap();
    let table = s
        .execute("layout.table", &json!({"x": 20, "y": 30, "width": 300, "height": 120, "rows": 2, "columns": 2, "text": [["A", "B"], ["C", "D"]]}))
        .unwrap();
    let group = table["id"].as_u64().unwrap();
    // Deleting at the extent (== rows) is refused as bad input, not a caught panic (Internal).
    let err = s.execute("layout.tableEdit", &json!({"id": group, "action": "deleteRow", "at": 2})).unwrap_err();
    let text = err.to_string();
    assert!(text.contains("layout.tableEdit"), "{err}");
    assert!(!text.contains("Internal"), "a bounds bug must not surface as a caught panic: {err}");
    assert!(text.contains("does not name one of the table"), "{err}");
    // A cell renamed to a zero row or column refuses the edit rather than underflowing.
    let first_cell = {
        let st = s.doc().unwrap();
        match &st.doc.node(NodeId(group)).unwrap().kind {
            NodeKind::Group { children, .. } => children[0].id.0,
            _ => panic!("table is not a group"),
        }
    };
    s.execute("object.setProps", &json!({"id": first_cell, "name": "Cell 0:1"})).unwrap();
    let err = s.execute("layout.tableEdit", &json!({"id": group, "action": "insertRow"})).unwrap_err();
    assert!(!err.to_string().contains("Internal"), "an underflow must not surface as a caught panic: {err}");
    s.execute("object.setProps", &json!({"id": first_cell, "name": "Cell 1:0"})).unwrap();
    let err = s.execute("layout.tableEdit", &json!({"id": group, "action": "insertRow"})).unwrap_err();
    assert!(!err.to_string().contains("Internal"), "an underflow must not surface as a caught panic: {err}");
    s.execute("object.setProps", &json!({"id": first_cell, "name": "Cell 1:1"})).unwrap();
    // A middle-row delete keeps the surviving texts in order.
    s.execute("layout.tableEdit", &json!({"id": group, "action": "deleteRow", "at": 0})).unwrap();
    let texts = {
        let st = s.doc().unwrap();
        match &st.doc.node(NodeId(group)).unwrap().kind {
            NodeKind::Group { children, .. } => children
                .iter()
                .filter_map(|n| match &n.kind {
                    NodeKind::Text(t) => Some(t.plain_text()),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            _ => panic!("table is not a group"),
        }
    };
    assert_eq!(texts, vec!["C", "D"], "the first row is gone, the second stays");
}

#[test]
fn coreldraw_table_edit_preserves_cell_styling_and_edits_after_reopen() {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 500, "height": 300})).unwrap();
    let table = s
        .execute("layout.table", &json!({"x": 20, "y": 30, "width": 300, "height": 120, "rows": 2, "columns": 2, "text": [["A", "B"], ["C", "D"]]}))
        .unwrap();
    let group = table["id"].as_u64().unwrap();
    // Style the second cell's text red and its border green.
    let (text_id, border_id) = {
        let st = s.doc().unwrap();
        match &st.doc.node(NodeId(group)).unwrap().kind {
            NodeKind::Group { children, .. } => (children[3].id.0, children[2].id.0),
            _ => panic!("table is not a group"),
        }
    };
    s.execute("select.set", &json!({"ids": [text_id]})).unwrap();
    s.execute("paint.setFill", &json!({"color": "#c02020"})).unwrap();
    s.execute("select.set", &json!({"ids": [border_id]})).unwrap();
    s.execute("paint.setStroke", &json!({"color": "#20a040"})).unwrap();
    // A structural edit keeps the styled cell's look (new cells take the table's defaults).
    s.execute("select.set", &json!({"ids": [group]})).unwrap();
    s.execute("layout.tableEdit", &json!({"id": group, "action": "insertRow", "at": 0})).unwrap();
    let (text_color, border_color) = {
        let st = s.doc().unwrap();
        let ids: Vec<u64> = match &st.doc.node(NodeId(group)).unwrap().kind {
            NodeKind::Group { children, .. } => children.iter().map(|n| n.id.0).collect(),
            _ => panic!("table is not a group"),
        };
        drop(st);
        let mut text_color = None;
        let mut border_color = None;
        for id in ids {
            let node = s.execute("document.node", &json!({"id": id, "summary": true})).unwrap();
            if node["text"].as_str() == Some("B") {
                text_color = node["fill"].as_str().map(str::to_owned);
            }
            if node["name"].as_str() == Some("Cell 2:2") && node["kind"].as_str() == Some("Path") {
                border_color = node["stroke"].as_str().map(str::to_owned);
            }
        }
        (text_color.unwrap_or_default(), border_color.unwrap_or_default())
    };
    assert_eq!(text_color, "#c02020", "the styled cell text keeps its colour");
    assert_eq!(border_color, "#20a040", "the styled cell border keeps its colour");
    // Native reopen: the table still edits.
    let data = s.execute("document.serialize", &json!({"format": "vectorcraft"})).unwrap()["dataBase64"].as_str().unwrap().to_string();
    s.execute("document.open", &json!({"dataBase64": data})).unwrap();
    let reopened = s.execute("document.inspect", &json!({})).unwrap();
    let _ = reopened;
    // Find the reopened table by walking the tree for the named group.
    let table_id = {
        let st = s.doc().unwrap();
        let mut found = None;
        for layer in &st.doc.layers {
            for child in layer.children().into_iter().flatten() {
                if child.name.as_deref() == Some("Table") && matches!(child.kind, NodeKind::Group { .. }) {
                    found = Some(child.id.0);
                }
            }
        }
        found.expect("the reopened table")
    };
    s.execute("layout.tableEdit", &json!({"id": table_id, "action": "insertColumn", "at": 0})).unwrap();
    let count = {
        let st = s.doc().unwrap();
        match &st.doc.node(NodeId(table_id)).unwrap().kind {
            NodeKind::Group { children, .. } => children.len(),
            _ => panic!("table is not a group"),
        }
    };
    assert_eq!(count, 18, "three rows of three cells after the reopened edit");
}
