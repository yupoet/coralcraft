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

/// The CorelDRAW transform controls of the Property Bar (P1 Task 6), shown with one or more
/// objects selected in the CorelDRAW workspace alone: Mirror Horizontal and Mirror Vertical over
/// `object.reflect` and the Rotation field over `object.rotate` (the angle of the selection's
/// bounding box, `absolute`), as the public user guide's property bar carries them. The numeric
/// transforms, Apply to Duplicate (`copy`) and the rest stay in the Transform docker.
pub fn transform_buttons(app: &mut VectorcraftApp, ui: &mut Ui) {
    if !active(app) || app.session.active().is_none_or(|st| st.selection.objects.is_empty()) {
        return;
    }
    let mut run = None;
    for (icon, tip, axis) in [("flip-horizontal-2", "Mirror Horizontal", "vertical"), ("flip-vertical-2", "Mirror Vertical", "horizontal")] {
        if widgets::icon_button(ui, icon, tl!(tip), false, 24.0).clicked() {
            run = Some(json!({ "axis": axis }));
        }
    }
    // The selection's current angle, shown as the amount another turn adds to.
    let angle = app.selection_box().map_or(0.0, |b| b.angle);
    widgets::field_label(ui, label(&Tokens::get(ui.ctx()), "Rotation:"));
    if let Some(v) = widgets::plain_field(ui, "cb-corel-rotation", angle, "°", 1, 56.0) {
        run = Some(json!({ "angle": v, "absolute": true }));
    }
    if let Some(p) = run {
        let cmd = if p.get("axis").is_some() { "object.reflect" } else { "object.rotate" };
        app.run(cmd, p).ok();
    }
    ui.add_space(4.0);
    ui.separator();
}

/// The CorelDRAW table controls of the Property Bar (P2.TABLE-UI): with one `layout.table` group
/// selected, Insert Row / Insert Column append a row or column (empty cells) and Delete Row /
/// Delete Column take the last one away, each one `layout.tableEdit` undo step. Cell-level
/// picking and drag-resizing stay with the Shape tool's group editing.
pub fn table_buttons(app: &mut VectorcraftApp, ui: &mut Ui) {
    let Some(table) = app.session.active().and_then(|st| match st.selection.objects[..] {
        [id] => st.doc.node(id).filter(|n| match &n.kind {
            vectorcraft_doc::NodeKind::Group { children, .. } => {
                n.name.as_deref() == Some("Table") && children.iter().any(|c| c.name.as_deref().is_some_and(|name| name.starts_with("Cell ")))
            }
            _ => false,
        }),
        _ => None,
    }) else {
        return;
    };
    if !active(app) {
        return;
    }
    let table = table.id.0;
    for (label, action) in
        [("Insert Row", "insertRow"), ("Delete Row", "deleteRow"), ("Insert Column", "insertColumn"), ("Delete Column", "deleteColumn")]
    {
        if widgets::flat_button(ui, tl!(label), 64.0).clicked() {
            app.run("layout.tableEdit", json!({ "id": table, "action": action })).ok();
        }
    }
    ui.add_space(4.0);
    ui.separator();
}

/// The CorelDRAW shaping buttons of the Property Bar (P1 Task 4), in the public user guide's
/// order, shown with two or more objects selected in the CorelDRAW workspace alone. Each runs the
/// shared Pathfinder command it maps onto; Simplify and Trim share `object.pathfinder.trim`
/// (each shape cut by everything above it), whose geometry is Simplify's, and identical to Trim
/// for the two-object case.
pub fn shaping_buttons(app: &mut VectorcraftApp, ui: &mut Ui) {
    let n = app.session.active().map_or(0, |st| st.selection.objects.len());
    if !active(app) || n < 2 {
        return;
    }
    for (label, cmd) in [
        ("Weld", "object.pathfinder.unite"),
        ("Trim", "object.pathfinder.trim"),
        ("Intersect", "object.pathfinder.intersect"),
        ("Simplify", "object.pathfinder.trim"),
        ("Front Minus Back", "object.pathfinder.minusBack"),
        ("Back Minus Front", "object.pathfinder.minusFront"),
        ("Boundary", "object.pathfinder.boundary"),
    ] {
        if widgets::flat_button(ui, tl!(label), 64.0).clicked() {
            app.run(cmd, json!({})).ok();
        }
    }
    ui.add_space(4.0);
    ui.separator();
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
        let ctx = ctx();
        let nodes = control_bar_nodes(&ctx, &mut app, 1440.0, vec![]);
        assert!(node_text(&nodes, "Revolutions:").is_some(), "the Spiral row shows");
        assert!(node_text(&nodes, "Decay:").is_some());
        assert!(node_text(&nodes, "Segments").is_none(), "the add-on's spiral controls are replaced");
        // Freehand: the smoothing row shows.
        app.run("tool.select", json!({ "tool": "pencil" })).unwrap();
        let nodes = control_bar_nodes(&ctx, &mut app, 1440.0, vec![]);
        assert!(node_text(&nodes, "Freehand Smoothing:").is_some(), "the Freehand row shows");
        // Rectangle: the add-on shape controls the workspaces share keep showing.
        app.run("tool.select", json!({ "tool": "rectangle" })).unwrap();
        let nodes = control_bar_nodes(&ctx, &mut app, 1440.0, vec![]);
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
        let nodes = control_bar_nodes(&ctx, &mut app, 1440.0, vec![]);
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

    /// The shaping buttons show with two or more objects selected in the CorelDRAW workspace alone,
    /// and a click runs its command (Weld unites into one selected object).
    #[test]
    fn shaping_buttons_show_and_weld_runs() {
        let mut app = app();
        corel(&mut app);
        let a = app.run("shape.rectangle", json!({ "x": 10, "y": 10, "width": 60, "height": 60 })).unwrap()["id"].clone();
        let b = app.run("shape.rectangle", json!({ "x": 40, "y": 40, "width": 60, "height": 60 })).unwrap()["id"].clone();
        let ctx = ctx();
        // One object: no shaping buttons.
        app.run("select.set", json!({ "ids": [a] })).unwrap();
        let nodes = control_bar_nodes(&ctx, &mut app, 1440.0, vec![]);
        assert!(node_text(&nodes, "Weld").is_none(), "one object is nothing to shape");
        // Two: the seven buttons show.
        app.run("select.set", json!({ "ids": [a, b] })).unwrap();
        let nodes = control_bar_nodes(&ctx, &mut app, 1440.0, vec![]);
        for label in ["Weld", "Trim", "Intersect", "Simplify", "Front Minus Back", "Back Minus Front", "Boundary"] {
            assert!(node_text(&nodes, label).is_some(), "{label} shows");
        }
        // A click on Weld unites the selection into one object.
        let center = |n: &egui::accesskit::Node| {
            let b = n.bounds().unwrap_or_else(|| panic!("{n:?}"));
            egui::pos2(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32)
        };
        let at = center(&node_text(&nodes, "Weld").expect("the Weld button"));
        let press = |pressed| egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
        control_bar_nodes(&ctx, &mut app, 1440.0, vec![egui::Event::PointerMoved(at), press(true)]);
        control_bar_nodes(&ctx, &mut app, 1440.0, vec![press(false)]);
        control_bar_nodes(&ctx, &mut app, 1440.0, vec![]);
        let selected = app.session.active().unwrap().selection.objects.clone();
        assert_eq!(selected.len(), 1, "Weld united the two rectangles");
        // Outside the CorelDRAW workspace the buttons never show.
        crate::workspaces::switch(&mut app.ui, crate::workspaces::ESSENTIALS).unwrap();
        app.run("edit.undo", json!({})).unwrap();
        app.run("select.set", json!({ "ids": [a, b] })).unwrap();
        let nodes = control_bar_nodes(&ctx, &mut app, 1440.0, vec![]);
        assert!(node_text(&nodes, "Weld").is_none(), "the upstream bar keeps its behaviour");
    }

    /// The CorelDRAW snap toggles: `view.snapToGuides` flips the state the tools receive, the
    /// CorelDRAW workspace's Snapping menu names the daily-use modes (Snap to Objects, Snap to
    /// Guidelines, Dynamic Guides) where the upstream menu keeps its own, and both the view flag
    /// and the Dynamic Guides preference persist.
    #[test]
    fn snapping_toggles_corel_names_and_persist() {
        let mut app = app();
        corel(&mut app);
        // The toggle command flips the state and the tools' ViewInfo with it.
        assert!(app.ui.view.snap_to_guides, "on by default");
        assert!(app.view_info().snap_to_guides);
        crate::menus::invoke(&mut app, "view.snapToGuides", json!({}));
        assert!(!app.ui.view.snap_to_guides && !app.view_info().snap_to_guides, "toggled off");
        crate::menus::invoke(&mut app, "view.snapToGuides", json!({}));
        assert!(app.ui.view.snap_to_guides, "toggled back on");
        // The CorelDRAW workspace's Snapping menu shows the CorelDRAW modes once opened.
        let ctx = ctx();
        let nodes = open_snapping_menu(&ctx, &mut app);
        for label in ["Snap to Objects", "Snap to Grid", "Snap to Guidelines", "Dynamic Guides"] {
            assert!(node_text(&nodes, label).is_some(), "{label} shows in the CorelDRAW workspace: {nodes:?}");
        }
        assert!(node_text(&nodes, "Smart Guides").is_none(), "the CorelDRAW names replace the upstream ones");
        // The upstream menu keeps its own names outside the workspace (a click away closes the
        // open menu first).
        let away = egui::pos2(1400.0, 16.0);
        let away_press =
            |pressed| egui::Event::PointerButton { pos: away, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
        control_bar_nodes(&ctx, &mut app, 1440.0, vec![egui::Event::PointerMoved(away), away_press(true)]);
        control_bar_nodes(&ctx, &mut app, 1440.0, vec![away_press(false)]);
        crate::workspaces::switch(&mut app.ui, crate::workspaces::ESSENTIALS).unwrap();
        let nodes = open_snapping_menu(&ctx, &mut app);
        assert!(node_text(&nodes, "Smart Guides").is_some() && node_text(&nodes, "Snap to Point").is_some(), "the upstream entries return");
        assert!(node_text(&nodes, "Snap to Objects").is_none(), "no CorelDRAW entry outside the workspace");
        // The view flag and the Dynamic Guides preference persist.
        app.ui.view.snap_to_guides = false;
        let back: crate::state::UiState = serde_json::from_str(&serde_json::to_string(&app.ui).unwrap()).unwrap();
        assert!(!back.view.snap_to_guides, "the flag round-trips");
        let old: crate::state::UiState = serde_json::from_str(&serde_json::to_string(&app.ui).unwrap().replace("\"snap_to_guides\":false", ""))
            .unwrap_or_else(|_| serde_json::from_str("{}").unwrap());
        assert!(old.view.snap_to_guides, "a state saved before the flag defaults on");
        app.run("prefs.set", json!({ "key": "constructionGuides", "value": false })).unwrap();
        assert!(!app.session.prefs.construction_guides, "Dynamic Guides off through its preference");
        assert_eq!(app.run("prefs.get", json!({ "key": "constructionGuides" })).unwrap(), json!(false), "the preference reads back");
    }

    /// The transform controls show with one object selected in the CorelDRAW workspace alone, and
    /// a click on Mirror Horizontal runs `object.reflect` (the geometry flips); the Rotation field
    /// shows the selection's angle.
    #[test]
    fn transform_buttons_show_and_mirror_runs() {
        let mut app = app();
        corel(&mut app);
        let ctx = ctx();
        // Nothing selected: no transform controls.
        let nodes = control_bar_nodes(&ctx, &mut app, 1440.0, vec![]);
        assert!(node_text(&nodes, "Mirror Horizontal").is_none() && node_text(&nodes, "Rotation:").is_none(), "nothing selected");
        let id = app.run("shape.rectangle", json!({ "x": 100, "y": 100, "width": 40, "height": 20 })).unwrap()["id"].clone();
        app.run("select.set", json!({ "ids": [id] })).unwrap();
        let nodes = control_bar_nodes(&ctx, &mut app, 1440.0, vec![]);
        assert!(node_text(&nodes, "Mirror Horizontal").is_some(), "the mirror buttons show");
        assert!(node_text(&nodes, "Rotation:").is_some(), "the rotation field shows");
        // A click flips the selection about its own axis (the box keeps its place, mirrored).
        let b = node_text(&nodes, "Mirror Horizontal").unwrap().bounds().expect("the button");
        let at = egui::pos2(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32);
        let press = |pressed| egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
        let undo = app.session.active().unwrap().history.undo.len();
        control_bar_nodes(&ctx, &mut app, 1440.0, vec![egui::Event::PointerMoved(at), press(true)]);
        control_bar_nodes(&ctx, &mut app, 1440.0, vec![press(false)]);
        control_bar_nodes(&ctx, &mut app, 1440.0, vec![]);
        let node = app.run("document.node", json!({ "id": id, "summary": true })).unwrap();
        let bb = &node["bounds"];
        assert_eq!((bb["x"].as_f64(), bb["width"].as_f64()), (Some(100.0), Some(40.0)), "a square-to-the-page box mirrors in place: {bb:?}");
        assert_eq!(app.session.active().unwrap().history.undo.len(), undo + 1, "one undo step");
        // Outside the CorelDRAW workspace the controls never show.
        crate::workspaces::switch(&mut app.ui, crate::workspaces::ESSENTIALS).unwrap();
        let nodes = control_bar_nodes(&ctx, &mut app, 1440.0, vec![]);
        assert!(node_text(&nodes, "Mirror Horizontal").is_none(), "the upstream bar keeps its behaviour");
    }

    /// The table controls show with a `layout.table` group selected in the CorelDRAW workspace
    /// alone, and Insert Row grows the table through `layout.tableEdit` (one undo step).
    #[test]
    fn table_buttons_show_and_insert_a_row() {
        let mut app = app();
        corel(&mut app);
        let ctx = ctx();
        // A plain rectangle: no table controls.
        let rect = app.run("shape.rectangle", json!({ "x": 10, "y": 10, "width": 40, "height": 30 })).unwrap()["id"].clone();
        app.run("select.set", json!({ "ids": [rect] })).unwrap();
        let nodes = control_bar_nodes(&ctx, &mut app, 1440.0, vec![]);
        assert!(node_text(&nodes, "Insert Row").is_none(), "a rectangle is not a table");
        // A table: the four controls show, and Insert Row appends a row.
        let table = app
            .run("layout.table", json!({ "x": 40, "y": 40, "width": 200, "height": 60, "rows": 2, "columns": 2, "text": [["A", "B"], ["C", "D"]] }))
            .unwrap()["id"]
            .clone();
        app.run("select.set", json!({ "ids": [table] })).unwrap();
        let nodes = control_bar_nodes(&ctx, &mut app, 1440.0, vec![]);
        for label in ["Insert Row", "Delete Row", "Insert Column", "Delete Column"] {
            assert!(node_text(&nodes, label).is_some(), "{label} shows");
        }
        let b = node_text(&nodes, "Insert Row").unwrap().bounds().expect("the button");
        let at = egui::pos2(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32);
        let press = |pressed| egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
        let undo = app.session.active().unwrap().history.undo.len();
        control_bar_nodes(&ctx, &mut app, 1440.0, vec![egui::Event::PointerMoved(at), press(true)]);
        control_bar_nodes(&ctx, &mut app, 1440.0, vec![press(false)]);
        control_bar_nodes(&ctx, &mut app, 1440.0, vec![]);
        let r = app.run("document.node", json!({ "id": table, "summary": true })).unwrap();
        assert_eq!(r["children"].as_array().map(Vec::len), Some(12), "three rows of two cells: {r:?}");
        assert_eq!(app.session.active().unwrap().history.undo.len(), undo + 1, "one undo step");
        // Outside the CorelDRAW workspace the controls never show.
        crate::workspaces::switch(&mut app.ui, crate::workspaces::ESSENTIALS).unwrap();
        let nodes = control_bar_nodes(&ctx, &mut app, 1440.0, vec![]);
        assert!(node_text(&nodes, "Insert Row").is_none(), "the upstream bar keeps its behaviour");
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

    /// Two headless frames of the whole Control bar, `width` wide (the first lays it out), with
    /// optional events, on the shared `ctx`.
    fn control_bar_nodes(
        ctx: &egui::Context,
        app: &mut VectorcraftApp,
        width: f32,
        events: Vec<egui::Event>,
    ) -> Vec<(egui::accesskit::NodeId, egui::accesskit::Node)> {
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(width, 300.0));
        let mut nodes = Vec::new();
        for events in [events, vec![]] {
            let mut out =
                ctx.run_ui(egui::RawInput { events, screen_rect: Some(screen), ..Default::default() }, |ui| crate::chrome::control_bar(app, ui));
            out.textures_delta.clear();
            nodes = out.platform_output.accesskit_update.map(|u| u.nodes).unwrap_or_default();
        }
        nodes
    }

    /// The Snapping menu opened (a click on its button), and the frames' accesskit nodes with its
    /// entries in.
    fn open_snapping_menu(ctx: &egui::Context, app: &mut VectorcraftApp) -> Vec<(egui::accesskit::NodeId, egui::accesskit::Node)> {
        let nodes = control_bar_nodes(ctx, app, 1440.0, vec![]);
        let button = node_text(&nodes, "Snapping").expect("the Snapping menu button");
        let b = button.bounds().expect("the button's bounds");
        let at = egui::pos2(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32);
        let press = |pressed| egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
        control_bar_nodes(ctx, app, 1440.0, vec![egui::Event::PointerMoved(at), press(true)]);
        control_bar_nodes(ctx, app, 1440.0, vec![press(false)]);
        control_bar_nodes(ctx, app, 1440.0, vec![])
    }

    fn node_text(nodes: &[(egui::accesskit::NodeId, egui::accesskit::Node)], text: &str) -> Option<egui::accesskit::Node> {
        nodes.iter().find(|(_, n)| n.label() == Some(text) || n.value() == Some(text)).map(|(_, n)| n.clone())
    }
}
