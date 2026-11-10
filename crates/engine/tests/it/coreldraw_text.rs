//! CoralCraft: P2 professional text workflows through the public command surface. The CorelDRAW
//! workspace uses these shared commands; these tests cover their model edits, history, and native
//! persistence without introducing a second paragraph formatter.

use serde_json::json;
use vectorcraft_doc::text::{Bullet, NumberStyle, ParaList};
use vectorcraft_doc::{Justify, NodeId, NodeKind, TextObject};
use vectorcraft_engine::Session;

fn text(s: &Session, id: u64) -> TextObject {
    match &s.doc().unwrap().doc.node(NodeId(id)).unwrap().kind {
        NodeKind::Text(text) => (**text).clone(),
        _ => panic!("not text"),
    }
}

#[test]
fn coreldraw_paragraph_controls_edit_history_and_reopen() {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 400, "height": 300})).unwrap();
    let id = s
        .execute("text.create", &json!({"x": 20, "y": 20, "text": "First paragraph\nSecond paragraph", "area": {"width": 240, "height": 180}}))
        .unwrap()["id"]
        .as_u64()
        .unwrap();

    let mut previous = Justify::Auto;
    for (command, expected) in [("center", Justify::Center), ("right", Justify::Right), ("justify", Justify::JustifyAll), ("left", Justify::Left)] {
        s.execute("text.setStyle", &json!({"id": id, "justify": command})).unwrap();
        assert!(text(&s, id).paragraph_styles().iter().all(|para| para.justify == expected), "{command} applies to every paragraph");
        s.execute("edit.undo", &json!({})).unwrap();
        let actual = text(&s, id).paragraph_styles().iter().map(|para| para.justify).collect::<Vec<_>>();
        assert!(actual.iter().all(|justify| *justify == previous), "after {command}: undo should restore {previous:?}, got {actual:?}");
        s.execute("edit.redo", &json!({})).unwrap();
        let actual = text(&s, id).paragraph_styles().iter().map(|para| para.justify).collect::<Vec<_>>();
        assert!(actual.iter().all(|justify| *justify == expected), "after {command}: redo should restore {expected:?}, got {actual:?}");
        previous = expected;
    }

    s.execute("text.setFormat", &json!({"ids": [id], "list": "bullet", "bullet": "square"})).unwrap();
    let square = Some(ParaList::bulleted(Bullet::Square));
    assert!(text(&s, id).paragraph_styles().iter().all(|para| para.list == square), "square bullets apply to the selected paragraphs");
    s.execute("edit.undo", &json!({})).unwrap();
    assert!(text(&s, id).paragraph_styles().iter().all(|para| para.list.is_none()));
    s.execute("edit.redo", &json!({})).unwrap();
    assert!(text(&s, id).paragraph_styles().iter().all(|para| para.list == square));

    s.execute("text.setFormat", &json!({"ids": [id], "list": "number", "number": "upperRoman"})).unwrap();
    let roman = Some(ParaList { bullet: Bullet::Square, number: Some(NumberStyle::UpperRoman) });
    assert!(text(&s, id).paragraph_styles().iter().all(|para| para.list == roman), "upper-Roman numbering applies to every paragraph");
    let markers = vectorcraft_text::layout(vectorcraft_text::FontDb::global(), &text(&s, id))
        .markers
        .iter()
        .map(|marker| marker.text.clone())
        .collect::<Vec<_>>();
    assert_eq!(markers, ["I.", "II."]);

    let data = s.execute("document.serialize", &json!({"format": "vectorcraft"})).unwrap()["dataBase64"].as_str().unwrap().to_string();
    s.execute("document.open", &json!({"dataBase64": data})).unwrap();
    let reopened = text(&s, id);
    assert!(reopened.paragraph_styles().iter().all(|para| para.justify == Justify::Left && para.list == roman));
    s.execute("text.setStyle", &json!({"id": id, "justify": "center"})).unwrap();
    assert!(text(&s, id).paragraph_styles().iter().all(|para| para.justify == Justify::Center), "the reopened text remains editable");
}
