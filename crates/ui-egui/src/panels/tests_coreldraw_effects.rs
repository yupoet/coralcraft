//! CoralCraft: signature-effect workflows over the shared panels and menu commands (P3).

use serde_json::json;
use vectorcraft_doc::{NodeId, NodeKind};

use super::tests_appearance::{app_with_rect, run};

#[test]
fn coreldraw_object_menu_runs_the_shared_powerclip_lifecycle() {
    let mut app = app_with_rect();
    crate::workspaces::switch(&mut app.ui, crate::workspaces::CORELDRAW).unwrap();
    let contents = app.session.active().unwrap().selection.objects[0];
    let frame = NodeId(run(&mut app, "shape.ellipse", json!({"x": 30, "y": 20, "width": 45, "height": 40}))["id"].as_u64().unwrap());
    run(&mut app, "select.set", json!({"ids": [contents.0, frame.0]}));

    let tree = crate::menus::menu_tree();
    let object = tree.iter().find(|(title, _)| *title == "Object").map(|(_, items)| items).unwrap();
    let clip = object
        .iter()
        .find_map(|item| match item {
            crate::menus::Item::Sub("Clipping Mask", items) => Some(items),
            _ => None,
        })
        .expect("Object menu has the clipping submenu");
    for command in ["object.clippingMask.make", "object.clippingMask.release", "object.clippingMask.editContents", "object.clippingMask.editMask"] {
        assert!(clip.iter().any(|item| matches!(item, crate::menus::Item::Cmd(_, id, _) if *id == command)), "{command} in {clip:?}");
    }

    crate::menus::invoke(&mut app, "object.clippingMask.make", json!({}));
    let group = app.session.active().unwrap().selection.objects[0];
    assert!(matches!(app.session.active().unwrap().doc.node(group).unwrap().kind, NodeKind::Group { clip: true, .. }));
    crate::menus::invoke(&mut app, "object.clippingMask.editContents", json!({}));
    assert_eq!(app.session.active().unwrap().selection.objects, vec![contents]);
    crate::menus::invoke(&mut app, "object.clippingMask.editMask", json!({}));
    assert_eq!(app.session.active().unwrap().selection.objects, vec![frame]);
    run(&mut app, "select.set", json!({"ids": [group.0]}));
    crate::menus::invoke(&mut app, "object.clippingMask.release", json!({}));
    assert!(matches!(app.session.active().unwrap().doc.node(group).unwrap().kind, NodeKind::Group { clip: false, .. }));
    assert!(app.session.active().unwrap().doc.node(contents).is_some());
    assert!(app.session.active().unwrap().doc.node(frame).is_some());
}

#[test]
fn coreldraw_transparency_panel_and_mesh_command_keep_live_models() {
    let mut app = app_with_rect();
    crate::workspaces::switch(&mut app.ui, crate::workspaces::CORELDRAW).unwrap();
    let shape = app.session.active().unwrap().selection.objects[0];
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let texts = super::tests_appearance::frame_events(&ctx, &mut app, vec![], super::transparency::show);
    assert!(texts.iter().any(|(text, _)| text.contains("Opacity")), "Transparency panel rows: {texts:?}");

    run(&mut app, "transparency.set", json!({"id": shape.0, "opacity": 55, "blend": "multiply"}));
    let node = app.session.active().unwrap().doc.node(shape).unwrap();
    assert_eq!(node.opacity, 0.55);
    assert_eq!(node.blend, vectorcraft_color::BlendMode::Multiply);
    run(&mut app, "object.mesh.create", json!({"id": shape.0, "rows": 1, "cols": 1}));
    assert!(matches!(app.session.active().unwrap().doc.node(shape).unwrap().kind, NodeKind::Mesh(_)));
}

#[test]
fn coreldraw_blend_panel_reflects_and_edits_the_live_blend() {
    let mut app = app_with_rect();
    crate::workspaces::switch(&mut app.ui, crate::workspaces::CORELDRAW).unwrap();
    let a = app.session.active().unwrap().selection.objects[0];
    let b = NodeId(run(&mut app, "shape.rectangle", json!({"x": 140, "y": 30, "width": 30, "height": 30}))["id"].as_u64().unwrap());
    run(&mut app, "select.set", json!({"ids": [a.0, b.0]}));
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let texts = super::tests_appearance::frame_events(&ctx, &mut app, vec![], super::blend::show);
    for label in ["Make", "Spacing:", "Orientation:"] {
        assert!(texts.iter().any(|(text, _)| text.contains(label)), "{label} in {texts:?}");
    }

    let blend = NodeId(run(&mut app, "object.blend.make", json!({"steps": 4}))["id"].as_u64().unwrap());
    let texts = super::tests_appearance::frame_events(&ctx, &mut app, vec![], super::blend::show);
    for label in ["Direction:", "Reverse Spine", "Expand", "Release"] {
        assert!(texts.iter().any(|(text, _)| text.contains(label)), "{label} in {texts:?}");
    }
    run(&mut app, "object.blend.options", json!({"id": blend.0, "steps": 7}));
    assert_eq!(app.session.execute("object.blend.info", &json!({})).unwrap()["steps"], json!(7));
    assert!(matches!(app.session.active().unwrap().doc.node(blend).unwrap().kind, NodeKind::Blend { .. }));
}

#[test]
fn coreldraw_envelope_menu_and_distort_dialog_use_shared_models() {
    let mut app = app_with_rect();
    crate::workspaces::switch(&mut app.ui, crate::workspaces::CORELDRAW).unwrap();
    let shape = app.session.active().unwrap().selection.objects[0];
    let tree = crate::menus::menu_tree();
    let object = tree.iter().find(|(title, _)| *title == "Object").map(|(_, items)| items).unwrap();
    let envelope = object
        .iter()
        .find_map(|item| match item {
            crate::menus::Item::Sub("Envelope Distort", items) => Some(items),
            _ => None,
        })
        .expect("Object menu has Envelope Distort");
    assert!(envelope.iter().any(|item| matches!(item, crate::menus::Item::Cmd(_, "object.envelope.makeWithWarp", _))));
    assert!(envelope.iter().any(|item| matches!(item, crate::menus::Item::Cmd(_, "object.envelope.editContents", _))));

    let envelope = NodeId(run(&mut app, "object.envelope.makeWithWarp", json!({"id": shape.0, "style": "arc", "bend": 25}))["id"].as_u64().unwrap());
    assert!(matches!(app.session.active().unwrap().doc.node(envelope).unwrap().kind, NodeKind::Envelope { .. }));
    run(&mut app, "effect.apply", json!({"id": shape.0, "effect": "distort.twist", "params": {"angle": 35}}));
    assert_eq!(app.session.active().unwrap().doc.node(shape).unwrap().appearance.effects[0].params["angle"].as_f64(), Some(35.0));
    crate::menus::invoke(&mut app, "effect.dialog", json!({"effect": "distort.twist"}));
    let dialog = app.ui.dialog.as_ref().expect("the Distort dialog opens");
    assert_eq!(dialog.fields.get("__effect").and_then(serde_json::Value::as_str), Some("distort.twist"));
}

#[test]
fn coreldraw_extrude_and_drop_shadow_dialogs_open_on_the_shared_effect_stack() {
    let mut app = app_with_rect();
    crate::workspaces::switch(&mut app.ui, crate::workspaces::CORELDRAW).unwrap();
    let shape = app.session.active().unwrap().selection.objects[0];
    for effect in ["threeD.extrude", "stylize.dropShadow"] {
        run(&mut app, "effect.apply", json!({"id": shape.0, "effect": effect}));
        crate::menus::invoke(&mut app, "effect.dialog", json!({"effect": effect}));
        let dialog = app.ui.dialog.as_ref().expect("the effect dialog opens");
        assert_eq!(dialog.fields.get("__effect").and_then(serde_json::Value::as_str), Some(effect));
        app.ui.dialog = None;
    }
    let effects = &app.session.active().unwrap().doc.node(shape).unwrap().appearance.effects;
    assert_eq!(effects.iter().map(|effect| effect.id.as_str()).collect::<Vec<_>>(), ["threeD.extrude", "stylize.dropShadow"]);
}

#[test]
fn coreldraw_appearance_docker_shows_and_edits_the_ordered_effect_stack() {
    let mut app = app_with_rect();
    crate::workspaces::switch(&mut app.ui, crate::workspaces::CORELDRAW).unwrap();
    let shape = app.session.active().unwrap().selection.objects[0];
    run(&mut app, "effect.apply", json!({"id": shape.0, "effect": "stylize.dropShadow"}));
    run(&mut app, "effect.apply", json!({"id": shape.0, "effect": "distort.twist"}));
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let texts = super::tests_appearance::frame_events(&ctx, &mut app, vec![], super::appearance::show);
    for label in ["Drop Shadow", "Twist"] {
        assert!(texts.iter().any(|(text, _)| text.contains(label)), "{label} in {texts:?}");
    }
    run(&mut app, "effect.move", json!({"id": shape.0, "from": 1, "to": 0}));
    run(&mut app, "effect.duplicate", json!({"id": shape.0, "index": 0}));
    run(&mut app, "effect.setParams", json!({"id": shape.0, "index": 0, "visible": false}));
    let effects = &app.session.active().unwrap().doc.node(shape).unwrap().appearance.effects;
    assert_eq!(effects.iter().map(|effect| effect.id.as_str()).collect::<Vec<_>>(), ["distort.twist", "distort.twist", "stylize.dropShadow"]);
    assert!(!effects[0].visible);
}
