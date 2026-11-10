// CoralCraft: the CorelDRAW Property Bar modes for the mapped core tools (P1 Task 2 / V4.3).
//
// The public user guide's property bar is context-sensitive: with a drawing tool active it shows
// that tool's fields — the defaults for the next draw with nothing selected, the selected object's
// parameters with a matching live shape selected
// (<https://product.corel.com/help/CorelDRAW/Documentation-Windows/index.html>, "Property bar").
// The shape tools' rows (Rectangle's radius, Polygon's sides, Ellipse's modes) are the synced
// add-on shape controls already drawn by [`crate::toolbar::control_bar_options`]; the rows here
// add what no bar exposes — the Spiral tool's real defaults (`segments`/`decay`, the `shape.spiral`
// command's) and the Freehand tool's smoothing — over `tool.setOption`, with headless tests. They
// show in the CorelDRAW workspace preset (V4.1) alone, so the upstream Control bar keeps its
// behaviour.

use egui::Ui;

use serde_json::json;
use vectorcraft_tools::coreldraw::{self, CorelTool};

use crate::theme::Tokens;
use crate::{VectorcraftApp, widgets};

/// Whether the CorelDRAW workspace preset is the active workspace.
pub fn active(app: &VectorcraftApp) -> bool {
    app.ui.workspace == crate::workspaces::CORELDRAW
}

/// The mapped core tool that is active, if the CorelDRAW workspace is on and the active VectorCraft
/// tool maps onto one.
pub fn active_tool(app: &VectorcraftApp) -> Option<&'static CorelTool> {
    active(app).then(|| coreldraw::corel_for_vc(app.session.tool_id())).flatten()
}

/// A field's label, in the Control bar's inline style.
fn label(t: &Tokens, text: &str) -> egui::RichText {
    egui::RichText::new(tl!(text)).size(12.0).color(t.text)
}

/// The active core tool's Property Bar row, when it has one of its own: the Spiral tool's
/// Revolutions and Decay (its real option keys, which the add-on shape controls' turns/divergence
/// don't drive) and the Freehand tool's Freehand Smoothing. Returns whether a row was drawn; the
/// Spiral row replaces the add-on shape controls, so the bar never shows two spiral
/// parameterizations at once.
pub fn tool_options(app: &mut VectorcraftApp, ui: &mut Ui) -> bool {
    let Some(tool) = active_tool(app) else { return false };
    let t = Tokens::get(ui.ctx());
    let mut run = None;
    match tool.vc {
        "spiral" => {
            let opts = app.session.tool_options();
            widgets::field_label(ui, label(&t, "Revolutions:"));
            if let Some(v) = widgets::plain_field(ui, "cb-corel-revolutions", opts["segments"].as_f64().unwrap_or(10.0), "", 0, 48.0)
                .map(|n| n.round().clamp(2.0, 1000.0) as u64)
            {
                run = Some(json!({ "tool": "spiral", "key": "segments", "value": v }));
            }
            widgets::field_label(ui, label(&t, "Decay:"));
            if let Some(v) = widgets::plain_field(ui, "cb-corel-decay", opts["decay"].as_f64().unwrap_or(80.0), "%", 0, 56.0) {
                run = Some(json!({ "tool": "spiral", "key": "decay", "value": v }));
            }
        }
        "pencil" => {
            let shown = app.session.tool_options()["fidelity"].as_f64().unwrap_or(50.0);
            widgets::field_label(ui, label(&t, "Freehand Smoothing:"));
            if let Some(v) = widgets::plain_field(ui, "cb-corel-smoothing", shown, "%", 0, 56.0) {
                run = Some(json!({ "tool": "pencil", "key": "fidelity", "value": v }));
            }
        }
        _ => return false,
    }
    if let Some(p) = run {
        app.run("tool.setOption", p).ok();
    }
    ui.add_space(4.0);
    ui.separator();
    true
}

/// The mapped core tool's label with its public shortcut, as the toolbox shows it (`None`: the
/// VectorCraft tool is not one of the mapped core tools, or the CorelDRAW workspace is off).
pub fn tool_tip(app: &VectorcraftApp, vc: &str) -> Option<String> {
    let c = active(app).then(|| coreldraw::corel_for_vc(vc)).flatten()?;
    Some(match c.shortcut {
        Some(s) => format!("{} ({})", tl!(c.label), crate::shortcut_editor::as_typed(s)),
        None => tl!(c.label).to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::vec2;
    use serde_json::json;

    fn app() -> VectorcraftApp {
        let mut app = VectorcraftApp::new(vectorcraft_engine::Session::new(), Default::default());
        app.run("file.new", json!({ "width": 800, "height": 400 })).unwrap();
        app
    }

    fn corel(app: &mut VectorcraftApp) {
        crate::workspaces::switch(&mut app.ui, crate::workspaces::CORELDRAW).unwrap();
        assert!(active(app));
    }

    /// Every mapped core tool selects its VectorCraft tool through `tool.select`, and each tool
    /// with Property Bar fields of its own round-trips them through `tool.setOption` (the
    /// Rectangle's radius is the option of its next draw, not a persisted one; see
    /// `vectorcraft_tools::settings`).
    #[test]
    fn mapped_tools_select_and_round_trip_their_options() {
        let mut app = app();
        for c in coreldraw::COREL_TOOLS {
            app.run("tool.select", json!({ "tool": c.vc })).unwrap();
            assert_eq!(app.session.tool_id(), c.vc, "{} maps onto {}", c.id, c.vc);
        }
        app.run("tool.select", json!({ "tool": "rectangle" })).unwrap();
        app.run("tool.setOption", json!({ "key": "cornerRadius", "value": 25.0 })).unwrap();
        assert_eq!(app.session.tool_options()["cornerRadius"], json!(25.0));
        for (tool, key, value) in
            [("polygon", "sides", json!(7)), ("spiral", "segments", json!(12)), ("spiral", "decay", json!(95.0)), ("pencil", "fidelity", json!(75.0))]
        {
            app.run("tool.setOption", json!({ "tool": tool, "key": key, "value": value.clone() })).unwrap();
            let back = app.session.tool_options_of(tool)[key].clone();
            assert_eq!(back, value, "{tool}.{key} round-trips");
        }
    }

    /// The Property Bar rows show only in the CorelDRAW workspace: the Spiral row replaces the
    /// add-on shape controls (one spiral parameterization at a time), the Freehand row adds the
    /// tool's smoothing, and the shape tools keep the add-on controls every workspace has. A
    /// selected live shape edits through the shared commands with one undo step.
    #[test]
    fn property_bar_rows_show_per_core_tool() {
        let mut app = app();
        corel(&mut app);
        // Spiral: the CorelDRAW row shows, the add-on's Segments doesn't.
        app.run("tool.select", json!({ "tool": "spiral" })).unwrap();
        let nodes = control_bar_nodes(&mut app, 1440.0);
        assert!(node_text(&nodes, "Revolutions:").is_some(), "the Spiral row shows");
        assert!(node_text(&nodes, "Decay:").is_some());
        assert!(node_text(&nodes, "Segments").is_none(), "the add-on's spiral controls are replaced");
        // Freehand: the smoothing row shows.
        app.run("tool.select", json!({ "tool": "pencil" })).unwrap();
        let nodes = control_bar_nodes(&mut app, 1440.0);
        assert!(node_text(&nodes, "Freehand Smoothing:").is_some(), "the Freehand row shows");
        // Rectangle: the add-on shape controls the workspaces share keep showing.
        app.run("tool.select", json!({ "tool": "rectangle" })).unwrap();
        let nodes = control_bar_nodes(&mut app, 1440.0);
        assert!(node_text(&nodes, "Radius:").is_some(), "the Rectangle's shape controls show");
        // A selected rectangle and polygon edit through the live-shape commands: one undo step
        // each, the live shape's parameter changes (the fields' commands).
        let rect = app.run("shape.rectangle", json!({ "x": 10, "y": 10, "width": 90, "height": 60 })).unwrap()["id"].clone();
        app.run("select.set", json!({ "ids": [rect] })).unwrap();
        let undo = app.session.active().unwrap().history.undo.len();
        app.run("object.setLiveShape", json!({ "radius": 18.0 })).unwrap();
        assert_eq!(app.session.active().unwrap().history.undo.len(), undo + 1, "one undo step");
        let poly = app.run("shape.polygon", json!({ "cx": 200, "cy": 100, "radius": 60, "sides": 5 })).unwrap()["id"].clone();
        app.run("select.set", json!({ "ids": [poly] })).unwrap();
        app.run("object.setLiveShape", json!({ "sides": 8 })).unwrap();
        let shown = app.session.active().unwrap().doc.node(vectorcraft_doc::NodeId(poly.as_u64().unwrap())).map(|n| &n.kind);
        assert!(
            matches!(shown, Some(vectorcraft_doc::NodeKind::Path { live: Some(live), .. }) if matches!(**live, vectorcraft_doc::LiveShape::Polygon { sides: 8, .. })),
            "the polygon's points changed"
        );
        // Outside the CorelDRAW workspace the upstream bar is unchanged.
        crate::workspaces::switch(&mut app.ui, crate::workspaces::ESSENTIALS).unwrap();
        app.run("tool.select", json!({ "tool": "spiral" })).unwrap();
        let nodes = control_bar_nodes(&mut app, 1440.0);
        assert!(node_text(&nodes, "Revolutions:").is_none(), "no CorelDRAW row outside the workspace");
        assert!(node_text(&nodes, "Segments").is_some(), "the add-on controls return");
    }

    /// The mapped core tools show the CorelDRAW workspace's toolbox tips.
    #[test]
    fn mapped_tool_tips_show_corel_names() {
        let mut app = app();
        corel(&mut app);
        assert_eq!(tool_tip(&app, "selection").unwrap(), "Pick Tool");
        assert_eq!(tool_tip(&app, "pencil").unwrap(), "Freehand Tool (F5)");
        assert!(tool_tip(&app, "shaper").is_none());
        crate::workspaces::switch(&mut app.ui, crate::workspaces::ESSENTIALS).unwrap();
        assert!(tool_tip(&app, "selection").is_none());
    }

    /// An English context for headless Control bar frames, with AccessKit on to find its controls
    /// (as the Paragraph panel's Control bar tests set up).
    fn ctx() -> egui::Context {
        crate::i18n::set_current(crate::i18n::Lang::EN);
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        crate::theme::install_fonts(&ctx);
        ctx
    }

    /// Two headless frames of the whole Control bar, `width` wide (the first lays it out).
    fn control_bar_nodes(app: &mut VectorcraftApp, width: f32) -> Vec<(egui::accesskit::NodeId, egui::accesskit::Node)> {
        let ctx = ctx();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(width, 300.0));
        let mut nodes = Vec::new();
        for _ in 0..2 {
            let mut out = ctx.run_ui(egui::RawInput { screen_rect: Some(screen), ..Default::default() }, |ui| crate::chrome::control_bar(app, ui));
            out.textures_delta.clear();
            nodes = out.platform_output.accesskit_update.map(|u| u.nodes).unwrap_or_default();
        }
        nodes
    }

    fn node_text(nodes: &[(egui::accesskit::NodeId, egui::accesskit::Node)], text: &str) -> Option<egui::accesskit::Node> {
        nodes.iter().find(|(_, n)| n.label() == Some(text) || n.value() == Some(text)).map(|(_, n)| n.clone())
    }
}
