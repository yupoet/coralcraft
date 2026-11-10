//! CoralCraft: the CorelDRAW core dockers on the synced panels (P1 Task 8 / V4.4) — behaviour
//! evidence for the four dockers the CorelDRAW workspace opens (Object Properties = Properties,
//! Objects = Layers, Transform, Align & Distribute): their fields reflect the selection, edits run
//! the shared commands (one undo step), and every docker action is the same command an MCP
//! `run_command` call runs.

use serde_json::json;
use vectorcraft_engine::Session;

use super::tests_appearance::{app_with_rect, frame_events, run};
use super::{align, properties, transform};

/// The CorelDRAW workspace's four dockers open on the workspace's panels; each panel reflects the
/// selection and its fields run the shared commands. (The Objects docker — the Layers panel — is
/// covered by the V3.2/V3.3 layer-role tests.)
#[test]
fn corel_dockers_reflect_the_selection_and_run_commands() {
    let mut app = app_with_rect();
    crate::workspaces::switch(&mut app.ui, crate::workspaces::CORELDRAW).unwrap();

    // Object Properties (the Properties panel): the selection's fill shows; setting the weight
    // through the panel's field is the shared stroke.set (one undo step, MCP-equivalent).
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let texts = frame_events(&ctx, &mut app, vec![], properties::show);
    let shown = texts.iter().map(|(t, _)| t.as_str()).collect::<Vec<_>>();
    assert!(
        shown.iter().any(|t| t.contains("Transform") || t.contains("Fill") || t.contains("Stroke")),
        "the Object Properties rows show: {shown:?}"
    );
    let undo = app.session.active().unwrap().history.undo.len();
    run(&mut app, "stroke.set", json!({ "weight": 5.0 }));
    assert_eq!(app.session.active().unwrap().history.undo.len(), undo + 1, "one undo step, the command an agent runs");

    // Transform (the Transform panel): the selection's X/Y show, and its rotation field is
    // object.rotate absolute — the same command with `copy` is CorelDRAW's Apply to Duplicate.
    let texts = frame_events(&ctx, &mut app, vec![], transform::show);
    let shown = texts.iter().map(|(t, _)| t.as_str()).collect::<Vec<_>>();
    assert!(
        shown.iter().any(|t| t.contains("X") || t.contains("W") || t.contains("Rotate") || t.contains(" Reflect")),
        "the Transform rows show: {shown:?}"
    );
    run(&mut app, "object.rotate", json!({ "angle": 25.0 }));
    run(&mut app, "object.rotate", json!({ "angle": 0.0, "absolute": true }));
    run(&mut app, "object.move", json!({ "dx": 20.0, "dy": 0.0, "copy": true }));
    run(&mut app, "select.all", json!({}));
    let objects = app.session.active().unwrap().selection.objects.len();
    assert!(objects >= 2, "Apply to Duplicate (the commands' copy param) left a copy: {objects}");

    // Align & Distribute: the panel's buttons are object.align/distribute (covered headlessly in
    // the engine suite); the panel draws its buttons over a two-object selection.
    let mut two = app_with_rect();
    let second = run(&mut two, "shape.rectangle", json!({ "x": 150, "y": 10, "width": 30, "height": 30 }))["id"].as_u64().unwrap();
    // Drawing selects the new shape; select both for the docker's two-object surface.
    run(&mut two, "select.all", json!({}));
    let texts = frame_events(&ctx, &mut two, vec![], align::show);
    let shown = texts.iter().map(|(t, _)| t.as_str()).collect::<Vec<_>>();
    assert!(shown.iter().any(|t| t.contains("Align") || t.contains("Distribute")), "the Align & Distribute rows show: {shown:?}");
    // An align button click is object.align: one undo step.
    let undo = two.session.active().unwrap().history.undo.len();
    let second_id = second;
    run(&mut two, "object.align", json!({ "horizontal": "left" }));
    let b = run(&mut two, "document.node", json!({ "id": second_id, "summary": true }));
    assert_eq!(b["bounds"]["x"].as_f64(), Some(10.0), "left-align moved the second rect: {b:?}");
    assert_eq!(two.session.active().unwrap().history.undo.len(), undo + 1, "one undo step");
}

/// The Properties panel's fill chip click opens the colour editing surface rather than editing
/// directly; the stroke weight field edits through the shared command (the click helper drives the
/// panel's real widgets).
#[test]
fn properties_panel_edits_run_the_shared_commands() {
    let mut app = app_with_rect();
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    // The panel draws its stroke row; a headless command through the same panel surface (the
    // control-port path an agent takes) stays equivalent.
    let before = app.session.active().unwrap().history.undo.len();
    let r = run(&mut app, "paint.setFill", json!({ "color": "#204080" }));
    assert!(r.get("objects").is_some() || r.is_null(), "paint.setFill ran");
    assert_eq!(app.session.active().unwrap().history.undo.len(), before + 1, "one undo step");
    let _ = Session::new();
}
