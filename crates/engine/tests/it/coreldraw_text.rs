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

#[test]
fn coreldraw_linked_frames_reflow_break_and_reopen() {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 600, "height": 300})).unwrap();
    let story = "The brochure introduction flows from one frame into the next without losing editable text. ".repeat(8);
    let first =
        s.execute("text.create", &json!({"x": 10, "y": 10, "text": story, "area": {"width": 150, "height": 60}})).unwrap()["id"].as_u64().unwrap();
    let second =
        s.execute("text.create", &json!({"x": 200, "y": 10, "text": "", "area": {"width": 150, "height": 120}})).unwrap()["id"].as_u64().unwrap();
    s.execute("select.set", &json!({"ids": [first, second]})).unwrap();
    s.execute("text.thread.create", &json!({})).unwrap();
    assert_eq!(s.execute("text.thread.info", &json!({})).unwrap(), json!([[first, second]]));
    let shown = |s: &Session| [first, second].map(|id| text(s, id).plain_text());
    assert!(shown(&s).iter().all(|part| !part.is_empty()), "both frames show part of the story");
    assert_eq!(shown(&s).concat(), story);

    s.execute("text.editRange", &json!({"id": first, "start": 0, "end": 0, "insert": "Revised introduction. "})).unwrap();
    assert_eq!(shown(&s).concat(), format!("Revised introduction. {story}"), "editing the first frame reflows the story");
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(shown(&s).concat(), story);
    s.execute("edit.redo", &json!({})).unwrap();
    let revised = format!("Revised introduction. {story}");
    assert_eq!(shown(&s).concat(), revised);

    s.execute("text.thread.break", &json!({"id": first, "port": "out"})).unwrap();
    assert!(s.execute("text.thread.info", &json!({})).unwrap().as_array().unwrap().is_empty());
    assert_eq!(text(&s, first).plain_text(), revised);
    assert_eq!(text(&s, second).plain_text(), "");
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(s.execute("text.thread.info", &json!({})).unwrap(), json!([[first, second]]));
    assert_eq!(shown(&s).concat(), revised);

    let data = s.execute("document.serialize", &json!({"format": "vectorcraft"})).unwrap()["dataBase64"].as_str().unwrap().to_string();
    s.execute("document.open", &json!({"dataBase64": data})).unwrap();
    assert_eq!(s.execute("text.thread.info", &json!({})).unwrap(), json!([[first, second]]));
    assert_eq!(shown(&s).concat(), revised);
    s.execute("text.editRange", &json!({"id": first, "start": 0, "end": 0, "insert": "After reopen. "})).unwrap();
    assert_eq!(shown(&s).concat(), format!("After reopen. {revised}"), "the reopened thread remains editable");
}

#[test]
fn coreldraw_character_styles_keep_local_overrides_and_reopen() {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 400, "height": 200})).unwrap();
    let id = s.execute("text.create", &json!({"x": 10, "y": 30, "text": "Hello world"})).unwrap()["id"].as_u64().unwrap();
    s.execute("charStyle.new", &json!({"name": "Brochure Emphasis", "attrs": {"size": 24.0, "underline": true, "features": ["dlig"]}})).unwrap();
    s.execute("charStyle.apply", &json!({"name": "Brochure Emphasis", "id": id, "start": 6, "end": 11})).unwrap();
    s.execute("text.setRangeStyle", &json!({"id": id, "start": 6, "end": 11, "size": 30.0, "featureChanges": ["tnum"]})).unwrap();
    s.execute("charStyle.setAttrs", &json!({"name": "Brochure Emphasis", "attrs": {"size": 18.0, "underline": false, "tracking": 50.0}})).unwrap();
    let styled = text(&s, id).runs.into_iter().find(|run| run.text == "world").expect("the styled range");
    assert_eq!((styled.style.size, styled.style.underline, styled.style.tracking), (30.0, false, 50.0), "redefinition keeps the local size override");
    assert_eq!(styled.style.style_name.as_deref(), Some("Brochure Emphasis"));
    assert!(styled.style.features.iter().any(|feature| feature == "tnum"));
    s.execute("edit.undo", &json!({})).unwrap();
    let before = text(&s, id).runs.into_iter().find(|run| run.text == "world").unwrap();
    assert!(
        before.style.underline && before.style.features.iter().any(|feature| feature == "tnum"),
        "undo restores the previous style and feature state"
    );
    s.execute("edit.redo", &json!({})).unwrap();

    s.execute("paraStyle.new", &json!({"name": "Brochure Lead", "attrs": {"justify": "Center", "space_after": 8.0}})).unwrap();
    s.execute("paraStyle.apply", &json!({"name": "Brochure Lead", "id": id})).unwrap();
    s.execute("paraStyle.setAttrs", &json!({"name": "Brochure Lead", "attrs": {"justify": "Right", "space_after": 12.0}})).unwrap();
    assert_eq!((text(&s, id).para.justify, text(&s, id).para.space_after), (Justify::Right, 12.0));

    let data = s.execute("document.serialize", &json!({"format": "vectorcraft"})).unwrap()["dataBase64"].as_str().unwrap().to_string();
    s.execute("document.open", &json!({"dataBase64": data})).unwrap();
    let reopened_text = text(&s, id);
    assert_eq!((reopened_text.para.justify, reopened_text.para.space_after), (Justify::Right, 12.0));
    let reopened = reopened_text.runs.into_iter().find(|run| run.text == "world").expect("the reopened styled range");
    assert_eq!((reopened.style.size, reopened.style.underline, reopened.style.tracking), (30.0, false, 50.0));
    assert!(reopened.style.features.iter().any(|feature| feature == "tnum"));
}

#[test]
fn coreldraw_glyphs_variable_axes_and_missing_fonts_reopen() {
    use vectorcraft_text::test_fonts::{TWO_AXIS_FAMILY, alternates_family, alternates_font, two_axis_font, unencoded_glyphs};
    let db = vectorcraft_text::FontDb::global();
    db.add_font(alternates_font(true).expect("the synthetic alternates font builds"));
    db.add_font(two_axis_font().expect("the synthetic variable font builds"));
    let [alternate, ..] = unencoded_glyphs().expect("the synthetic unencoded glyph");

    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 500, "height": 300})).unwrap();
    let glyphs =
        s.execute("text.create", &json!({"x": 10, "y": 40, "text": "xaay", "font": alternates_family(true)})).unwrap()["id"].as_u64().unwrap();
    let listed = s.execute("text.glyphAlternates", &json!({"id": glyphs, "start": 2})).unwrap();
    assert!(listed["alternates"].as_array().unwrap().iter().any(|item| item["gid"] == json!(alternate)));
    s.execute("text.setGlyphAlternate", &json!({"id": glyphs, "start": 2, "gid": alternate})).unwrap();
    assert_eq!(s.execute("text.glyphAlternates", &json!({"id": glyphs, "start": 2})).unwrap()["chosen"], json!(alternate));
    s.execute("edit.undo", &json!({})).unwrap();
    assert_ne!(s.execute("text.glyphAlternates", &json!({"id": glyphs, "start": 2})).unwrap()["chosen"], json!(alternate));
    s.execute("edit.redo", &json!({})).unwrap();
    assert_eq!(s.execute("text.glyphAlternates", &json!({"id": glyphs, "start": 2})).unwrap()["chosen"], json!(alternate));

    let variable = s.execute("text.create", &json!({"x": 10, "y": 100, "text": "lIn", "font": TWO_AXIS_FAMILY})).unwrap()["id"].as_u64().unwrap();
    s.execute("text.setRangeStyle", &json!({"id": variable, "start": 0, "end": 3, "fontAxes": {"wght": 650}})).unwrap();
    assert_eq!(text(&s, variable).runs[0].style.font_axes.get("wght"), Some(&650.0));
    let missing = s.execute("text.create", &json!({"x": 10, "y": 160, "text": "Missing", "font": "CoralCraft P2 Missing 6F28C9"})).unwrap()["id"]
        .as_u64()
        .unwrap();
    let report = s.execute("text.missingFonts", &json!({})).unwrap();
    assert!(
        report["fonts"].as_array().unwrap().iter().any(|font| font["family"] == "CoralCraft P2 Missing 6F28C9" && font["status"] == "missing"),
        "{report}"
    );

    let data = s.execute("document.serialize", &json!({"format": "vectorcraft"})).unwrap()["dataBase64"].as_str().unwrap().to_string();
    s.execute("document.open", &json!({"dataBase64": data})).unwrap();
    assert_eq!(s.execute("text.glyphAlternates", &json!({"id": glyphs, "start": 2})).unwrap()["chosen"], json!(alternate));
    assert_eq!(text(&s, variable).runs[0].style.font_axes.get("wght"), Some(&650.0));
    assert_eq!(text(&s, missing).runs[0].style.font_family, "CoralCraft P2 Missing 6F28C9");
}
