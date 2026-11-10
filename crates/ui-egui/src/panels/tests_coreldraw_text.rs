//! CoralCraft: CorelDRAW text Property Bar evidence (P2 Task 2). The workspace reuses the
//! Character controls and their shared text commands rather than adding another text engine.

use std::cell::Cell;

use serde_json::json;
use vectorcraft_doc::{CharStyle, NodeKind};

use super::tests_appearance::{app_with_rect, click, frame_events, run, text_rect};
use super::*;

fn style(app: &VectorcraftApp, id: u64) -> CharStyle {
    match &app.session.active().unwrap().doc.node(vectorcraft_doc::NodeId(id)).unwrap().kind {
        NodeKind::Text(text) => text.first_style(),
        _ => panic!("not text"),
    }
}

#[test]
fn text_bar_edits_the_selected_font_and_size() {
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut app = app_with_rect();
    crate::workspaces::switch(&mut app.ui, crate::workspaces::CORELDRAW).unwrap();
    let id = run(&mut app, "text.create", json!({"x": 10, "y": 150, "text": "Brochure", "size": 12}))["id"].as_u64().unwrap();
    run(&mut app, "select.set", json!({"ids": [id]}));

    let texts = frame_events(&ctx, &mut app, vec![], crate::chrome::control_bar);
    assert!(texts.iter().any(|(text, _)| text == "Character:"), "CorelDRAW shows the Character controls: {texts:?}");
    let size = text_rect(&texts, "12 pt");
    let arrow = texts.iter().filter(|(_, rect)| rect.min.x >= size.max.x).min_by(|a, b| a.1.min.x.total_cmp(&b.1.min.x)).unwrap().1;
    click(&ctx, &mut app, arrow.center(), crate::chrome::control_bar);
    let texts = frame_events(&ctx, &mut app, vec![], crate::chrome::control_bar);
    click(&ctx, &mut app, text_rect(&texts, "24 pt").center(), crate::chrome::control_bar);
    assert_eq!(style(&app, id).size, 24.0, "the visible size preset edits the selected text");
    run(&mut app, "edit.undo", json!({}));
    assert_eq!(style(&app, id).size, 12.0);
    run(&mut app, "edit.redo", json!({}));
    assert_eq!(style(&app, id).size, 24.0);

    run(&mut app, "text.setStyle", json!({"id": id, "style": "Bold"}));
    assert_eq!(style(&app, id).font_style, "Bold", "the style menu's shared command edits the font style");
    let data = run(&mut app, "document.serialize", json!({"format": "vectorcraft"}))["dataBase64"].as_str().unwrap().to_string();
    run(&mut app, "document.open", json!({"dataBase64": data}));
    assert_eq!(style(&app, id).size, 24.0);
    assert_eq!(style(&app, id).font_style, "Bold");

    run(&mut app, "select.set", json!({"ids": []}));
    let texts = frame_events(&ctx, &mut app, vec![], crate::chrome::control_bar);
    assert!(!texts.iter().any(|(text, _)| text == "Character:"), "no text selection, no text Property Bar");
}

#[test]
fn coreldraw_font_size_preset_uses_the_shared_combo_geometry() {
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut app = app_with_rect();
    crate::workspaces::switch(&mut app.ui, crate::workspaces::CORELDRAW).unwrap();
    let id = run(&mut app, "text.create", json!({"x": 10, "y": 150, "text": "Text", "size": 12}))["id"].as_u64().unwrap();
    run(&mut app, "select.set", json!({"ids": [id]}));
    let at = Cell::new(egui::Pos2::ZERO);
    let mut combo = |app: &mut VectorcraftApp, ui: &mut Ui| {
        at.set(ui.max_rect().min);
        ui.horizontal(|ui| character::size_field(app, ui, "cb-font-size", 96.0));
    };
    frame_events(&ctx, &mut app, vec![], &mut combo);
    click(&ctx, &mut app, at.get() + egui::vec2(86.0, 13.0), &mut combo);
    let texts = frame_events(&ctx, &mut app, vec![], &mut combo);
    click(&ctx, &mut app, text_rect(&texts, "36 pt").center(), &mut combo);
    assert_eq!(style(&app, id).size, 36.0);
}
