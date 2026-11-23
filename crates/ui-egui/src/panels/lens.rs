//! CoralCraft (P3 Task E4): the Lens docker (CorelDRAW's Lens window). With a shape selected,
//! the kind list, Amount and Tint pick a colour lens and Apply runs `object.lens` on the
//! selection as one undo step; the backdrop-distorting kinds are absent by design (the command
//! refuses them by name). The picker's state lives in the panel (egui memory), like a dialog's
//! fields.

use egui::Ui;
use serde_json::json;

use super::selection_len;
use crate::VectorcraftApp;
use crate::widgets;

pub const ID: &str = "lens";

/// The colour lenses the docker lists, with the labels the dropdown shows.
const KINDS: [(&str, &str); 5] =
    [("invert", "Invert"), ("brighten", "Brighten"), ("darken", "Darken"), ("lighten", "Lighten"), ("tintedGrayscale", "Tinted Grayscale")];

pub fn show(app: &mut VectorcraftApp, ui: &mut Ui) {
    let kind = ui.data_mut(|d| *d.get_temp_mut_or_insert_with(ui.scope_id().with("lens-kind"), || 0usize));
    let amount = ui.data_mut(|d| *d.get_temp_mut_or_insert_with(ui.scope_id().with("lens-amount"), || 50.0f64));
    let tint = ui.data_mut(|d| d.get_temp_mut_or_insert_with(ui.scope_id().with("lens-tint"), || "#204080".to_owned()).clone());
    let mut apply = None;

    widgets::label_row(ui, crate::tl!("Lens Type:"), 84.0, |ui| {
        if let Some(i) = widgets::dropdown(ui, "lens-kind", KINDS[kind.min(4)].1, &KINDS.map(|(_, l)| l), ui.available_width()) {
            ui.data_mut(|d| d.insert_temp(ui.scope_id().with("lens-kind"), i));
        }
    });
    ui.horizontal(|ui| {
        widgets::field_label(ui, crate::tl!("Amount:"));
        if let Some(v) = widgets::plain_field(ui, "lens-amount", amount, "%", 0, 64.0) {
            ui.data_mut(|d| d.insert_temp(ui.scope_id().with("lens-amount"), v));
        }
    });
    ui.horizontal(|ui| {
        widgets::field_label(ui, crate::tl!("Tint:"));
        if let Some(v) = widgets::plain_field(ui, "lens-tint-field", 0.0, &tint, 0, 84.0) {
            let _ = v;
        }
    });
    let enabled = selection_len(app) > 0;
    ui.add_enabled_ui(enabled, |ui| {
        if widgets::flat_button(ui, crate::tl!("Apply"), 96.0).clicked() {
            // The tint is the tinted-grayscale lens's own colour; the others carry their own.
            let id = KINDS[kind.min(4)].0;
            let mut params = json!({ "kind": id, "amount": amount.round() });
            if id == "tintedGrayscale" {
                params["color"] = json!(tint);
            }
            apply = Some(params);
        }
    });
    if let Some(p) = apply {
        app.run("object.lens", p).ok();
    }
}
