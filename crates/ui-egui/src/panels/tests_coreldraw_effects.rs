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
    use egui_kittest::kittest::Queryable;

    fn choose(h: &mut egui_kittest::Harness<'_, crate::VectorcraftApp>, current: &str, item: &str) {
        let label = match current {
            "Default" => "Preset:",
            "Color" | "Grayscale" => "Mode:",
            "Tracing Result" => "View:",
            other => panic!("unexpected combo value {other}"),
        };
        let combo = h.get_by_role_and_label(egui::accesskit::Role::ComboBox, label);
        assert_eq!(combo.value().as_deref(), Some(current));
        combo.click();
        h.run_steps(2);
        h.get_by_label(item).click();
        h.run_steps(4);
    }

    fn release_slider(h: &mut egui_kittest::Harness<'_, crate::VectorcraftApp>, label: &str, high: bool) {
        h.run_steps(2);
        let rail =
            h.ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::unique(("trace-slider", label)))).unwrap_or_else(|| panic!("{label} slider rail"));
        let at = if high { rail.right_center() - egui::vec2(2.0, 0.0) } else { rail.left_center() + egui::vec2(2.0, 0.0) };
        let press = |pressed| egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
        h.input_mut().events.extend([egui::Event::PointerMoved(at), press(true)]);
        h.run_steps(2);
        h.input_mut().events.push(press(false));
        h.run_steps(4);
    }

    fn selected(app: &crate::VectorcraftApp) -> NodeId {
        app.session.active().unwrap().selection.objects[0]
    }

    fn fill_palette(app: &crate::VectorcraftApp, id: NodeId) -> std::collections::BTreeSet<String> {
        let node = app.session.active().unwrap().doc.node(id).unwrap();
        node.children().unwrap().iter().filter_map(|child| child.appearance.fill_paint().color().map(|color| color.to_hex())).collect()
    }

    fn assert_color_result(app: &crate::VectorcraftApp, id: NodeId, expected: &std::collections::BTreeSet<String>) {
        let node = app.session.active().unwrap().doc.node(id).unwrap();
        assert_eq!(node.extras().trace.as_ref().unwrap()["params"]["mode"], "color");
        assert_eq!(fill_palette(app, id), *expected, "Color restores the complete original palette, not stale grayscale geometry");
    }

    fn undo(app: &crate::VectorcraftApp) -> usize {
        app.session.active().unwrap().history.undo.len()
    }

    fn anchors(app: &crate::VectorcraftApp) -> usize {
        let node = app.session.active().unwrap().doc.node(selected(app)).unwrap();
        node.children().unwrap().iter().filter_map(|child| child.path_data().map(|path| path.anchor_count())).sum()
    }

    fn assert_grayscale_result(app: &crate::VectorcraftApp, id: NodeId) {
        let node = app.session.active().unwrap().doc.node(id).unwrap();
        assert_eq!(node.extras().trace.as_ref().unwrap()["params"]["mode"], "grayscale");
        let fills: Vec<_> = node.children().unwrap().iter().filter_map(|child| child.appearance.fill_paint().color()).collect();
        assert!(!fills.is_empty());
        assert!(
            fills.into_iter().all(|color| {
                let [r, g, b, _] = color.to_rgba8(1.0);
                r == g && g == b
            }),
            "Grayscale mode produces only neutral fills"
        );
    }

    let mut app = app_with_rect();
    crate::workspaces::switch(&mut app.ui, crate::workspaces::CORELDRAW).unwrap();
    let image = image::RgbaImage::from_fn(96, 96, |x, y| {
        let boundary = 48.0 + (f64::from(y) / 10.0).sin() * 12.0;
        let left = f64::from(x) < boundary;
        let top = y < 48;
        let rgba = match (left, top) {
            (true, true) => [180, 30, 30, 255],
            (true, false) => [30, 150, 50, 255],
            (false, true) => [30, 80, 190, 255],
            (false, false) => [220, 180, 30, 255],
        };
        image::Rgba(rgba)
    });
    let mut png = std::io::Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png).unwrap();
    let placed = run(
        &mut app,
        "file.place",
        json!({"name": "four-colour-sine.png", "dataBase64": vectorcraft_format::base64_encode(&png.into_inner()), "link": false, "at": [20, 20]}),
    );
    assert_eq!(placed["ids"].as_array().unwrap().len(), 1);

    let mut h = egui_kittest::Harness::builder()
        .with_size(egui::vec2(520.0, 820.0))
        .with_accessibility_check(false)
        .with_context_setup(|ctx| {
            crate::theme::install_fonts(ctx);
            crate::theme::apply(ctx, Default::default());
        })
        .build_ui_state(|ui, app: &mut crate::VectorcraftApp| super::image_trace::show(app, ui), app);
    h.run_steps(6);

    let preview = h.get_by_role_and_label(egui::accesskit::Role::CheckBox, "Preview");
    preview.click();
    h.run_steps(2);
    assert!(!h.state().ui.image_trace_preview);
    h.get_by_role_and_label(egui::accesskit::Role::CheckBox, "Preview").click();
    h.run_steps(2);
    assert!(h.state().ui.image_trace_preview);

    choose(&mut h, "Default", "6 Colors");
    let before = undo(h.state());
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Trace").click();
    h.run_steps(4);
    assert_eq!(undo(h.state()), before + 1);
    let first = selected(h.state());
    let node = h.state().session.active().unwrap().doc.node(first).unwrap();
    assert_eq!(node.extras().trace.as_ref().unwrap()["preset"], "6 Colors");
    let initial_palette = fill_palette(h.state(), first);
    assert!(!initial_palette.is_empty());

    choose(&mut h, "Color", "Grayscale");
    assert_eq!(undo(h.state()), before + 2, "changing the visible Mode control retraces once");
    let grayscale = selected(h.state());
    assert_ne!(grayscale, first, "panel retraces replace the target with a new group ID");
    assert_grayscale_result(h.state(), grayscale);

    choose(&mut h, "Grayscale", "Color");
    assert_eq!(undo(h.state()), before + 3, "changing Mode back to Color is another single retrace");
    let second = selected(h.state());
    assert_ne!(second, grayscale);
    assert_color_result(h.state(), second, &initial_palette);

    let before = undo(h.state());
    release_slider(&mut h, "Colors", false);
    assert_eq!(undo(h.state()), before + 1, "one slider release is one undo step");
    let reduced = selected(h.state());
    assert_ne!(reduced, second);
    let node = h.state().session.active().unwrap().doc.node(reduced).unwrap();
    assert_eq!(node.extras().trace.as_ref().unwrap()["params"]["colors"], 2);
    let fills: std::collections::BTreeSet<_> =
        node.children().unwrap().iter().filter_map(|child| child.appearance.fill_paint().color().map(|color| color.to_hex())).collect();
    assert_eq!(fills.len(), 2, "the visible Colors control produces two unique fills: {fills:?}");

    let before_anchors = anchors(h.state());
    let before = undo(h.state());
    release_slider(&mut h, "Paths", false);
    assert_eq!(undo(h.state()), before + 1);
    let smoothed = selected(h.state());
    assert_ne!(smoothed, reduced);
    let node = h.state().session.active().unwrap().doc.node(smoothed).unwrap();
    assert_eq!(node.extras().trace.as_ref().unwrap()["params"]["paths"], 0.0);
    assert!(anchors(h.state()) < before_anchors, "Paths reduces anchors on this fixed curved fixture");
    let smoothed_anchors = anchors(h.state());

    let before = undo(h.state());
    choose(&mut h, "Tracing Result", "Outlines");
    assert_eq!(selected(h.state()), smoothed, "View does not retrace or replace the group");
    assert_eq!(anchors(h.state()), smoothed_anchors, "the smoothed geometry is unchanged by View");
    assert_eq!(undo(h.state()), before + 1, "View is one document edit");
    let node = h.state().session.active().unwrap().doc.node(smoothed).unwrap();
    assert_eq!(node.trace_view(), vectorcraft_doc::TraceView::Outlines);

    h.get_by_role_and_label(egui::accesskit::Role::Button, "Expand").click();
    h.run_steps(4);
    assert_eq!(selected(h.state()), smoothed, "Expand retains the group ID");
    assert_eq!(undo(h.state()), before + 2);
    assert_eq!(anchors(h.state()), smoothed_anchors, "Expand retains the traced paths");
    let expanded = h.state().session.active().unwrap().doc.node(smoothed).unwrap();
    assert!(expanded.extras().trace.is_none());
    assert!(expanded.children().unwrap().iter().all(|child| !matches!(child.kind, NodeKind::Image(_))));
    assert!(expanded.children().unwrap().iter().any(|child| child.path_data().is_some()));
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
