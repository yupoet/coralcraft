//! CorelDRAW-style right vertical Color Palette bar.
//!
//! Renders a scrollable column of color swatches along the right edge of the canvas:
//! - Top cell (`[X]` / `Paint::None`): Left-click clears fill (`paint.setFill {none: true}`),
//!   Right-click clears outline (`paint.setStroke {none: true}`).
//! - Swatch cells from the active document's swatches and color groups:
//!   Left-click applies the swatch to the selection's fill (`paint.setFill {swatch}`),
//!   Right-click applies the swatch to the selection's outline (`paint.setStroke {swatch}`).

use egui::{Pos2, Sense, Stroke, Ui, vec2};
use serde_json::json;
use vectorcraft_color::Paint;

use crate::theme::Tokens;
use crate::{VectorcraftApp, widgets};

/// Width of the right vertical Color Palette bar in screen points.
pub const PALETTE_BAR_WIDTH: f32 = 26.0;

/// Size of each square color chip in the vertical Color Palette bar.
const CHIP_SIZE: f32 = 18.0;

/// Renders the right-hand CorelDRAW vertical Color Palette bar when a document is open.
pub fn show(app: &mut VectorcraftApp, ui: &mut Ui) {
    if app.ui.home.is_some() || app.session.active().is_none() {
        return;
    }
    let t = Tokens::get(ui.ctx());
    egui::Panel::right("coralcraft_palette_bar")
        .exact_size(PALETTE_BAR_WIDTH)
        .frame(egui::Frame::NONE.fill(t.panel).inner_margin(egui::Margin::symmetric(3, 4)).stroke(Stroke::new(1.0, t.border)))
        .show(ui, |ui| {
            draw_contents(app, ui);
        });
}

/// Applies a palette bar cell (`None` for the top `[X]` No-Color cell, `Some(name)` for a named
/// swatch) to either the Fill (`is_stroke == false`, Left-click) or the Stroke/Outline
/// (`is_stroke == true`, Right-click).
pub fn apply_cell(app: &mut VectorcraftApp, swatch_name: Option<&str>, is_stroke: bool) {
    let cmd = if is_stroke { "paint.setStroke" } else { "paint.setFill" };
    let params = match swatch_name {
        Some(name) => json!({ "swatch": name }),
        None => json!({ "none": true }),
    };
    if let Err(e) = app.run(cmd, params) {
        app.status(e);
    }
}

fn draw_contents(app: &mut VectorcraftApp, ui: &mut Ui) {
    let Some(st) = app.session.active() else { return };
    let mut entries: Vec<(String, Paint)> = Vec::with_capacity(st.doc.swatches.len() + 16);
    for sw in &st.doc.swatches {
        if !matches!(sw.paint, Paint::None) {
            entries.push((sw.name.clone(), sw.paint.clone()));
        }
    }
    for grp in &st.doc.swatch_groups {
        for sw in &grp.swatches {
            if !matches!(sw.paint, Paint::None) {
                entries.push((sw.name.clone(), sw.paint.clone()));
            }
        }
    }

    let mut pending: Option<(Option<String>, bool)> = None;

    egui::ScrollArea::vertical()
        .id_salt("coralcraft_palette_bar_scroll")
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;

                // 1. Top `[X]` No-Color cell
                let (none_rect, none_resp) = ui.allocate_exact_size(vec2(CHIP_SIZE, CHIP_SIZE), Sense::click());
                let none_resp = none_resp.on_hover_text(crate::i18n::t("No Color (Left-click: clear fill, Right-click: clear outline)"));
                none_resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "No Color"));
                widgets::swatch_tile(ui, none_rect, &Paint::None, false, none_resp.hovered());
                if none_resp.clicked() {
                    pending = Some((None, false));
                } else if none_resp.secondary_clicked() {
                    pending = Some((None, true));
                }

                // 2. Document & Color-Group swatches
                for (name, paint) in &entries {
                    let (rect, resp) = ui.allocate_exact_size(vec2(CHIP_SIZE, CHIP_SIZE), Sense::click());
                    let tip = format!("{name} (Left-click: set fill, Right-click: set outline)");
                    let resp = resp.on_hover_text(tip);
                    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, name));
                    widgets::swatch_tile(ui, rect, paint, false, resp.hovered());
                    if resp.clicked() {
                        pending = Some((Some(name.clone()), false));
                    } else if resp.secondary_clicked() {
                        pending = Some((Some(name.clone()), true));
                    }
                }
                let _ = Pos2::ZERO;
            });
        });

    if let Some((swatch, is_stroke)) = pending {
        apply_cell(app, swatch.as_deref(), is_stroke);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vectorcraft_engine::Session;

    #[test]
    fn palette_bar_renders_swatches_and_applies_fill_on_left_click_and_stroke_on_right_click_v4_1() {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        app.run("file.new", json!({ "width": 200.0, "height": 200.0 })).unwrap();
        let id = app.run("shape.rectangle", json!({ "x": 10.0, "y": 10.0, "width": 80.0, "height": 60.0 })).unwrap()["id"].as_u64().unwrap();

        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let screen = egui::Rect::from_min_size(Pos2::ZERO, vec2(400.0, 600.0));
        let mut out = ctx.run_ui(egui::RawInput { screen_rect: Some(screen), ..Default::default() }, |ui| {
            show(&mut app, ui);
        });
        out.textures_delta.clear();

        let first_swatch = app.session.active().unwrap().doc.swatches.iter().find(|s| matches!(s.paint, Paint::Solid { .. })).unwrap().name.clone();

        // Left-click sets Fill to `first_swatch`
        apply_cell(&mut app, Some(&first_swatch), false);
        let node = app.session.active().unwrap().doc.node(vectorcraft_doc::NodeId(id)).unwrap();
        assert!(matches!(node.appearance.fill().map(|f| &f.paint), Some(Paint::Solid { .. })));

        // Right-click sets Stroke to `first_swatch`
        apply_cell(&mut app, Some(&first_swatch), true);
        let node = app.session.active().unwrap().doc.node(vectorcraft_doc::NodeId(id)).unwrap();
        assert!(matches!(node.appearance.stroke().map(|s| &s.paint), Some(Paint::Solid { .. })));

        // Left-click on `[X]` (`None`) clears Fill
        apply_cell(&mut app, None, false);
        let node = app.session.active().unwrap().doc.node(vectorcraft_doc::NodeId(id)).unwrap();
        assert!(matches!(node.appearance.fill().map(|f| &f.paint), None | Some(Paint::None)));
    }
}
