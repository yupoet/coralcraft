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

fn control_nodes(ctx: &egui::Context, app: &mut VectorcraftApp) -> Vec<egui::accesskit::Node> {
    let mut out = ctx.run_ui(egui::RawInput::default(), |ui| crate::chrome::control_bar(app, ui));
    out.textures_delta.clear();
    out.platform_output.accesskit_update.expect("an accessibility tree").nodes.into_iter().map(|(_, node)| node).collect()
}

fn click_control(ctx: &egui::Context, app: &mut VectorcraftApp, name: &str) {
    let nodes = control_nodes(ctx, app);
    let node = nodes
        .iter()
        .filter(|node| node.label() == Some(name))
        .max_by(|a, b| a.bounds().map(|r| r.x1 - r.x0).unwrap_or(0.0).total_cmp(&b.bounds().map(|r| r.x1 - r.x0).unwrap_or(0.0)))
        .unwrap_or_else(|| panic!("no control named {name}: {nodes:?}"));
    let bounds = node.bounds().expect("control bounds");
    let at = egui::pos2(((bounds.x0 + bounds.x1) * 0.5) as f32, ((bounds.y0 + bounds.y1) * 0.5) as f32);
    click(ctx, app, at, crate::chrome::control_bar);
}

#[test]
fn coreldraw_paragraph_popover_edits_alignment_and_lists() {
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    ctx.enable_accesskit();
    let mut app = app_with_rect();
    crate::workspaces::switch(&mut app.ui, crate::workspaces::CORELDRAW).unwrap();
    let id =
        run(&mut app, "text.create", json!({"x": 10, "y": 40, "text": "One\nTwo", "area": {"width": 200, "height": 120}}))["id"].as_u64().unwrap();
    run(&mut app, "select.set", json!({"ids": [id]}));
    let paragraph = |app: &VectorcraftApp| match &app.session.active().unwrap().doc.node(vectorcraft_doc::NodeId(id)).unwrap().kind {
        NodeKind::Text(text) => text.para.clone(),
        _ => panic!("not text"),
    };

    click_control(&ctx, &mut app, "Paragraph:");
    assert!(
        control_nodes(&ctx, &mut app).iter().any(|node| node.label() == Some("Justify all lines")),
        "the CorelDRAW Property Bar opens the shared Paragraph controls"
    );
    click_control(&ctx, &mut app, "Justify all lines");
    assert_eq!(paragraph(&app).justify, vectorcraft_doc::Justify::JustifyAll);
    click_control(&ctx, &mut app, "Bullets");
    assert_eq!(paragraph(&app).list, Some(vectorcraft_doc::text::ParaList::bulleted(vectorcraft_doc::text::Bullet::Dot)));
    click_control(&ctx, &mut app, "Numbered List");
    assert_eq!(paragraph(&app).list, Some(vectorcraft_doc::text::ParaList::numbered(vectorcraft_doc::text::NumberStyle::Arabic)));
    run(&mut app, "edit.undo", json!({}));
    assert_eq!(paragraph(&app).list, Some(vectorcraft_doc::text::ParaList::bulleted(vectorcraft_doc::text::Bullet::Dot)));
    run(&mut app, "edit.redo", json!({}));
    assert_eq!(paragraph(&app).list, Some(vectorcraft_doc::text::ParaList::numbered(vectorcraft_doc::text::NumberStyle::Arabic)));
}

#[test]
fn coreldraw_thread_menu_uses_the_shared_linking_commands() {
    let mut app = app_with_rect();
    crate::workspaces::switch(&mut app.ui, crate::workspaces::CORELDRAW).unwrap();
    let story = "Linked brochure text flows through both frames. ".repeat(12);
    let first = run(&mut app, "text.create", json!({"x": 10, "y": 10, "text": story, "area": {"width": 150, "height": 60}}))["id"].as_u64().unwrap();
    let second = run(&mut app, "text.create", json!({"x": 200, "y": 10, "text": "", "area": {"width": 150, "height": 120}}))["id"].as_u64().unwrap();
    run(&mut app, "select.set", json!({"ids": [first, second]}));
    assert!(crate::menus::enabled(&app, "text.thread.create"));
    crate::menus::invoke(&mut app, "text.thread.create", json!({}));
    assert_eq!(run(&mut app, "text.thread.info", json!({})), json!([[first, second]]));
    run(&mut app, "select.set", json!({"ids": [second]}));
    crate::menus::invoke(&mut app, "text.thread.remove", json!({}));
    assert_eq!(run(&mut app, "text.thread.info", json!({})), json!([]));
    run(&mut app, "edit.undo", json!({}));
    assert_eq!(run(&mut app, "text.thread.info", json!({})), json!([[first, second]]));
}

#[test]
fn coreldraw_workspace_opens_the_shared_typography_panels() {
    let mut app = app_with_rect();
    crate::workspaces::switch(&mut app.ui, crate::workspaces::CORELDRAW).unwrap();
    for panel in ["character", "paragraph", "openType", "glyphs", "charStyles", "paraStyles"] {
        app.run("window.panel", json!({"panel": panel})).unwrap();
        let docked = app.ui.docking.as_ref().is_some_and(|layout| layout.contains(&panel.to_string()));
        assert!(docked || app.ui.open_panel.as_deref() == Some(panel), "CorelDRAW shows the shared {panel} panel");
    }
}
