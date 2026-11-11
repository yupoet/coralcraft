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

#[test]
fn coreldraw_image_trace_panel_runs_the_shared_powertrace_workflow() {
    let mut app = app_with_rect();
    crate::workspaces::switch(&mut app.ui, crate::workspaces::CORELDRAW).unwrap();
    let mut png = std::io::Cursor::new(Vec::new());
    image::RgbaImage::from_fn(16, 16, |x, y| {
        if (3..13).contains(&x) && (3..13).contains(&y) { image::Rgba([20, 40, 80, 255]) } else { image::Rgba([255; 4]) }
    })
    .write_to(&mut png, image::ImageFormat::Png)
    .unwrap();
    let placed = run(
        &mut app,
        "file.place",
        json!({"name": "synthetic.png", "dataBase64": vectorcraft_format::base64_encode(&png.into_inner()), "link": false, "at": [20, 20]}),
    );
    let bitmap = placed["ids"][0].as_u64().unwrap();
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let texts = super::tests_appearance::frame_events(&ctx, &mut app, vec![], super::image_trace::show);
    for label in ["Preset", "Mode", "View"] {
        assert!(texts.iter().any(|(text, _)| text.contains(label)), "{label} in {texts:?}");
    }
    let trace = NodeId(run(&mut app, "imageTrace.make", json!({"id": bitmap, "params": {"ignoreWhite": true}}))["id"].as_u64().unwrap());
    assert!(app.session.active().unwrap().doc.node(trace).unwrap().extras().trace.is_some());
    let expanded = run(&mut app, "imageTrace.makeAndExpand", json!({"id": trace.0}));
    assert!(expanded["paths"].as_u64().unwrap() > 0, "{expanded}");
}

#[test]
fn coreldraw_gaussian_dialog_edits_a_selected_bitmaps_shared_effect_stack() {
    let mut app = app_with_rect();
    crate::workspaces::switch(&mut app.ui, crate::workspaces::CORELDRAW).unwrap();
    let mut png = std::io::Cursor::new(Vec::new());
    image::RgbaImage::from_fn(12, 8, |x, y| image::Rgba([(x * 17) as u8, (y * 29) as u8, 90, 255]))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let placed = run(
        &mut app,
        "file.place",
        json!({"name": "dialog-source.png", "dataBase64": vectorcraft_format::base64_encode(&png.into_inner()), "link": false, "at": [30, 40]}),
    );
    let bitmap = NodeId(placed["ids"][0].as_u64().unwrap());
    run(&mut app, "select.set", json!({"ids": [bitmap.0]}));

    let entry = crate::menus::menu_entries(&app)
        .into_iter()
        .find(|entry| entry.path == ["Effect", "Blur"] && entry.label == "Gaussian Blur…")
        .expect("Effect > Blur > Gaussian Blur is listed");
    assert!(entry.enabled);
    let (command, params) = crate::menus::click_target(&entry.label, entry.command.as_deref().unwrap(), &entry.params);
    crate::menus::invoke(&mut app, &command, params);
    let dialog = app.ui.dialog.as_ref().expect("the generic Gaussian Blur dialog opens");
    assert_eq!(dialog.kind, "effect");
    assert_eq!(dialog.fields.get("__effect").and_then(serde_json::Value::as_str), Some("blur.gaussian"));
    assert_eq!(dialog.fields.get("__label").and_then(serde_json::Value::as_str), Some("Gaussian Blur"));

    use egui_kittest::kittest::Queryable;
    let before = app.session.doc().unwrap().history.undo.len();
    let mut harness = egui_kittest::Harness::builder()
        .with_size(egui::vec2(900.0, 700.0))
        .with_context_setup(|ctx| {
            crate::theme::install_fonts(ctx);
            crate::theme::apply(ctx, Default::default());
        })
        .build_ui(|ui| crate::dialogs::show(&mut app, ui.ctx()));
    harness.run_steps(6);
    let radius = harness.get_by_role_and_label(egui::accesskit::Role::TextInput, "Radius:");
    assert_eq!(radius.value().as_deref(), Some("5 pt"));
    radius.click();
    harness.key_combination_modifiers(egui::Modifiers::COMMAND, &[egui::Key::A]);
    radius.type_text("8");
    harness.run_steps(4);
    harness.key_press(egui::Key::Tab);
    harness.run_steps(4);
    let radius = harness.get_by_role_and_label(egui::accesskit::Role::TextInput, "Radius:");
    assert_eq!(radius.value().as_deref(), Some("8 pt"));
    harness.get_by_role_and_label(egui::accesskit::Role::Button, "OK").click();
    harness.run_steps(4);
    drop(harness);

    assert!(app.ui.dialog.is_none());
    assert_eq!(app.session.doc().unwrap().history.undo.len(), before + 1);
    let node = app.session.active().unwrap().doc.node(bitmap).unwrap();
    assert!(matches!(node.kind, NodeKind::Image(_)));
    let effects = &node.appearance.effects;
    assert_eq!(effects.len(), 1);
    assert_eq!(effects[0].id, "blur.gaussian");
    assert_eq!(effects[0].params["radius"].as_f64(), Some(8.0));
}

#[test]
fn coreldraw_transparency_panel_makes_options_and_releases_an_opacity_mask() {
    let mut app = app_with_rect();
    crate::workspaces::switch(&mut app.ui, crate::workspaces::CORELDRAW).unwrap();
    let target = app.session.active().unwrap().selection.objects[0];
    let mask = NodeId(run(&mut app, "shape.rectangle", json!({"x": 30, "y": 20, "width": 35, "height": 40}))["id"].as_u64().unwrap());
    run(&mut app, "select.set", json!({"ids": [target.0, mask.0]}));

    use egui_kittest::kittest::Queryable;
    {
        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(520.0, 420.0))
            .with_accessibility_check(false)
            .with_context_setup(|ctx| {
                crate::theme::install_fonts(ctx);
                crate::theme::apply(ctx, Default::default());
            })
            .build_ui(|ui| super::transparency::show(&mut app, ui));
        harness.run_steps(6);
        harness.get_by_role_and_label(egui::accesskit::Role::Button, "Make Mask").click();
        harness.run_steps(4);
    }
    let model =
        app.session.active().unwrap().doc.node(target).unwrap().extras().mask.as_deref().expect("the visible Make Mask button creates a mask");
    assert_eq!(model.art.id, mask);
    assert!(model.art.path_data().is_some());
    assert!(app.session.active().unwrap().doc.node(mask).is_none());

    {
        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(520.0, 420.0))
            .with_accessibility_check(false)
            .with_context_setup(|ctx| {
                crate::theme::install_fonts(ctx);
                crate::theme::apply(ctx, Default::default());
            })
            .build_ui(|ui| super::transparency::show(&mut app, ui));
        harness.run_steps(4);
        harness.get_by_role_and_label(egui::accesskit::Role::CheckBox, "Invert Mask").click();
        harness.run_steps(4);
    }
    assert!(app.session.active().unwrap().doc.node(target).unwrap().extras().mask.as_deref().unwrap().invert);

    {
        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(520.0, 420.0))
            .with_accessibility_check(false)
            .with_context_setup(|ctx| {
                crate::theme::install_fonts(ctx);
                crate::theme::apply(ctx, Default::default());
            })
            .build_ui(|ui| super::transparency::show(&mut app, ui));
        harness.run_steps(4);
        harness.get_by_role_and_label(egui::accesskit::Role::Button, "Release").click();
        harness.run_steps(4);
    }

    let st = app.session.active().unwrap();
    assert!(st.doc.node(target).unwrap().extras().mask.is_none());
    assert!(st.doc.node(target).unwrap().path_data().is_some());
    assert!(st.selection.objects.iter().any(|id| st.doc.node(*id).is_some_and(|node| node.path_data().is_some() && node.id != target)));
}

#[test]
fn coreldraw_pattern_menu_and_options_run_a_real_edit_done_lifecycle() {
    fn menu_item(app: &mut crate::VectorcraftApp, label: &str) {
        fn show(app: &mut crate::VectorcraftApp, ui: &mut egui::Ui) {
            crate::menus::menu_bar(app, ui);
        }
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        crate::theme::apply(&ctx, Default::default());
        let shown = super::tests_appearance::frame_events(&ctx, app, vec![], show);
        let object = super::tests_appearance::text_rect(&shown, "Object").center();
        super::tests_appearance::click(&ctx, app, object, show);
        let shown = super::tests_appearance::frame_events(&ctx, app, vec![], show);
        let pattern = super::tests_appearance::text_rect(&shown, "Pattern").center();
        super::tests_appearance::click(&ctx, app, pattern, show);
        let shown = super::tests_appearance::frame_events(&ctx, app, vec![], show);
        let item = super::tests_appearance::text_rect(&shown, label).center();
        super::tests_appearance::click(&ctx, app, item, show);
    }
    fn click_done(app: &mut crate::VectorcraftApp) {
        use egui_kittest::kittest::Queryable;
        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(560.0, 700.0))
            .with_accessibility_check(false)
            .with_context_setup(|ctx| {
                crate::theme::install_fonts(ctx);
                crate::theme::apply(ctx, Default::default());
            })
            .build_ui(|ui| super::pattern_options::show(app, ui));
        harness.run_steps(6);
        harness.get_by_role_and_label(egui::accesskit::Role::Button, "Done").click();
        harness.run_steps(4);
    }
    let definition_color = |app: &crate::VectorcraftApp, name: &str| {
        app.session.active().unwrap().doc.pattern(name).unwrap().art.iter().find_map(|node| node.appearance.fill_paint().color()).unwrap().to_hex()
    };

    let mut app = app_with_rect();
    crate::workspaces::switch(&mut app.ui, crate::workspaces::CORELDRAW).unwrap();
    menu_item(&mut app, "Make");
    assert_eq!(app.ui.open_panel.as_deref(), Some("patternOptions"), "Make opens the existing Pattern Options panel");
    let name = app.session.active().unwrap().doc.pattern_edit.as_ref().expect("Make enters pattern editing").pattern.clone();
    let tile = app.session.active().unwrap().selection.objects[0];
    run(&mut app, "paint.setFill", json!({"id": tile.0, "color": "#123456"}));
    click_done(&mut app);
    assert!(app.session.active().unwrap().doc.pattern_edit.is_none(), "the visible Done button leaves pattern editing");
    assert_eq!(definition_color(&app, &name), "#123456", "Done saves the edited tile art");

    let target = run(&mut app, "shape.rectangle", json!({"x": 120, "y": 30, "width": 60, "height": 50}))["id"].as_u64().unwrap();
    run(&mut app, "paint.setFill", json!({"id": target, "swatch": name}));
    run(&mut app, "select.set", json!({"ids": [target]}));
    menu_item(&mut app, "Edit Pattern");
    assert_eq!(app.ui.open_panel.as_deref(), Some("patternOptions"), "Edit keeps the existing Pattern Options panel active");
    assert_eq!(app.session.active().unwrap().doc.pattern_edit.as_ref().map(|edit| edit.pattern.as_str()), Some(name.as_str()));
    let tile = app.session.active().unwrap().selection.objects[0];
    run(&mut app, "paint.setFill", json!({"id": tile.0, "color": "#abcdef"}));
    click_done(&mut app);
    assert!(app.session.active().unwrap().doc.pattern_edit.is_none());
    assert_eq!(definition_color(&app, &name), "#abcdef", "the second edit/Done lifecycle saves the ordinary colour edit");
}
