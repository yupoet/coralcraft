// CoralCraft: the CorelDRAW Outline Pen dialog (P1 Task 7) — the public user guide's outline
// fields (<https://product.corel.com/help/CorelDRAW/Documentation-Windows/index.html>, "Outline
// Pen") over the shared stroke commands: Width, Miter limit, Corner, Line caps, Arrows and Style
// run `stroke.set` (which, with nothing selected, sets up the next object drawn), the Colour runs
// `paint.setStroke`. The VectorCraft Stroke panel keeps its own behaviour.

use serde_json::{Value, json};

use super::{DialogSpec, form};
use crate::VectorcraftApp;
use crate::state::Dialog;

pub const KIND: &str = "outlinePen";

pub(super) const SPEC: DialogSpec = DialogSpec { heading: |_| tl!("Outline Pen").into(), body, confirm, ..DialogSpec::FORM };

const LABEL: f32 = 96.0;

/// (command value, label) of the Corner choices — `stroke.set`'s joins.
const JOINS: &[(&str, &str)] = &[("miter", "Miter"), ("round", "Round"), ("bevel", "Bevel")];
/// The Line caps choices.
const CAPS: &[(&str, &str)] = &[("butt", "Flat"), ("round", "Rounded"), ("square", "Square")];
/// The Arrowhead choices (a daily subset of `stroke.set`'s arrow ids, the guide's names).
const ARROWS: &[(&str, &str)] = &[
    ("", "None"),
    ("Arrow", "Arrow"),
    ("Triangle", "Triangle"),
    ("Concave", "Concave"),
    ("DoubleArrow", "Double Arrow"),
    ("Circle", "Circle"),
    ("Square", "Square"),
    ("Diamond", "Diamond"),
    ("Bar", "Bar"),
];
/// The Style choices — Solid and the guide's dotted and dashed patterns.
const STYLES: &[(&str, Option<&[f64]>)] = &[("solid", None), ("dotted", Some(&[0.0, 4.0])), ("dashed", Some(&[4.0, 4.0]))];

/// Open on the selection's outline (the first selected object's stroke, as `document.node`
/// reports it), or the defaults a new outline starts with when nothing is selected.
pub fn open(app: &mut VectorcraftApp) -> Result<Value, String> {
    let mut fields = json!({
        "weight": 2.0,
        "miterLimit": 10.0,
        "join": "miter",
        "cap": "butt",
        "startArrow": "",
        "endArrow": "",
        "style": "solid",
        "color": "#000000",
    });
    if let Some(id) = app.session.active().and_then(|st| st.selection.objects.first().map(|id| id.0))
        && let Ok(node) = app.run("document.node", json!({ "id": id, "summary": true }))
        && let Some(o) = node.get("strokeOptions").and_then(Value::as_object)
    {
        for (k, dst) in [("miterLimit", "miterLimit"), ("join", "join"), ("cap", "cap")] {
            if let Some(v) = o.get(k) {
                fields[dst] = v.clone();
            }
        }
        if let Some(arrow) = o.get("startArrow").and_then(Value::as_str) {
            fields["startArrow"] = json!(arrow);
        }
        if let Some(arrow) = o.get("endArrow").and_then(Value::as_str) {
            fields["endArrow"] = json!(arrow);
        }
        if let Some(dash) = o.get("dash").and_then(Value::as_array) {
            fields["style"] = json!(match dash.iter().filter_map(Value::as_f64).collect::<Vec<_>>()[..] {
                [0.0, ..] => "dotted",
                _ => "dashed",
            });
        }
        // The summary reports the weight beside the stroke options.
        if let Some(w) = node.get("strokeWidth").and_then(Value::as_f64) {
            fields["weight"] = json!(w);
        }
        if let Some(c) = node.get("stroke").and_then(Value::as_str).filter(|s| s.starts_with('#')) {
            fields["color"] = json!(c);
        }
    }
    app.ui.dialog = Some(Dialog::new(KIND, fields));
    Ok(Value::Null)
}

fn body(app: &mut VectorcraftApp, ui: &mut egui::Ui, d: &mut Dialog) -> bool {
    let unit = app.session.general_unit();
    form::grid_then(ui, d, unit, |ui, d| {
        form::length_field(ui, d, "weight", tl!("Width:"), unit);
        form::field_w(ui, d, "miterLimit", tl!("Miter limit:"), 72.0);
        choice(ui, d, "join", tl!("Corner:"), JOINS);
        choice(ui, d, "cap", tl!("Line caps:"), CAPS);
        choice(ui, d, "startArrow", tl!("Start arrow:"), ARROWS);
        choice(ui, d, "endArrow", tl!("End arrow:"), ARROWS);
        choice(ui, d, "style", tl!("Style:"), &[("solid", "Solid"), ("dotted", "Dotted"), ("dashed", "Dashed")]);
        crate::widgets::label_row(ui, tl!("Colour:"), LABEL, |ui| {
            form::color_text(ui, d, "color", 160.0);
        });
    });
    false
}

/// A labelled dropdown writing the chosen command value into `key`.
fn choice(ui: &mut egui::Ui, d: &mut Dialog, key: &str, label: &str, options: &[(&str, &str)]) {
    let cur = d.str(key);
    let shown = options.iter().find(|(v, _)| *v == cur).map_or(options[0].1, |(_, l)| *l);
    let labels: Vec<&str> = options.iter().map(|(_, l)| *l).collect();
    crate::widgets::label_row(ui, tl!(label), LABEL, |ui| {
        if let Some((value, _)) = crate::widgets::dropdown(ui, ("outline-pen", key), shown, &labels, 150.0).and_then(|i| options.get(i)) {
            d.fields.insert(key.into(), json!(value));
        }
    });
}

fn confirm(app: &mut VectorcraftApp, d: &Dialog) -> Result<Value, String> {
    let style = STYLES.iter().find(|(v, _)| *v == d.str("style")).and_then(|(_, dash)| *dash);
    let mut p = json!({
        "weight": d.f64("weight", 2.0).max(0.0),
        "miterLimit": d.f64("miterLimit", 10.0).max(1.0),
        "join": d.str("join"),
        "cap": d.str("cap"),
    });
    if let Some(dash) = style {
        p["dash"] = json!(dash);
    }
    for (key, dst) in [("startArrow", "startArrow"), ("endArrow", "endArrow")] {
        let v = d.str(key);
        if v.is_empty() {
            p[dst] = Value::Null;
        } else {
            p[dst] = json!(v);
        }
    }
    // The stroke first, then the colour (each is its own undo step, as the panels' are), then the
    // dialog closes whatever they did.
    let r = app.run("stroke.set", p).map_err(|e| e.to_string())?;
    if let Some(c) = d.fields.get("color").and_then(Value::as_str).filter(|c| c.starts_with('#') && c.len() == 7) {
        app.run("paint.setStroke", json!({ "color": c })).ok();
    }
    app.ui.dialog = None;
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use vectorcraft_engine::Session;

    fn app_with_stroked_path() -> (crate::VectorcraftApp, u64) {
        let mut app = crate::VectorcraftApp::new(Session::new(), Default::default());
        app.run("file.new", json!({ "width": 300, "height": 200 })).unwrap();
        let id = app.run("path.create", json!({ "d": "M 20 20 L 160 20 L 160 90" })).unwrap()["id"].as_u64().unwrap();
        app.run("select.set", json!({ "ids": [id] })).unwrap();
        app.run("stroke.set", json!({ "weight": 3.0, "join": "bevel", "cap": "round" })).unwrap();
        (app, id)
    }

    /// The dialog opens on the selection's outline — its fields show the object's weight, join and
    /// cap — and OK applies the fields as one `stroke.set` (plus `paint.setStroke` for a colour).
    #[test]
    fn opens_on_the_selection_and_applies_the_outline() {
        let (mut app, id) = app_with_stroked_path();
        open(&mut app).unwrap();
        let d = app.ui.dialog.as_ref().expect("the dialog is open");
        assert_eq!(app.ui.dialog.as_ref().unwrap().kind, KIND);
        assert_eq!(
            (d.f64("weight", 0.0), d.str("join").to_string(), d.str("cap").to_string()),
            (3.0, "bevel".to_string(), "round".to_string()),
            "the fields show the selection's outline"
        );
        // Edit the fields and confirm: the object's outline takes them.
        let mut d = app.ui.dialog.take().unwrap();
        d.fields.insert("weight".into(), json!(5.0));
        d.fields.insert("join".into(), json!("round"));
        d.fields.insert("cap".into(), json!("square"));
        d.fields.insert("style".into(), json!("dashed"));
        d.fields.insert("startArrow".into(), json!("Triangle"));
        d.fields.insert("color".into(), json!("#336699"));
        let r = confirm(&mut app, &d).unwrap();
        assert!(app.ui.dialog.is_none(), "the dialog closed");
        assert!(r.get("objects").is_some(), "stroke.set reported: {r}");
        let node = app.run("document.node", json!({ "id": id, "summary": true })).unwrap();
        let so = &node["strokeOptions"];
        assert_eq!((node["strokeWidth"].as_f64(), so["join"].as_str(), so["cap"].as_str()), (Some(5.0), Some("round"), Some("square")));
        assert_eq!(so["dash"].as_array().map(|d| d.len()), Some(2), "the dashed style: {so}");
        assert_eq!(so["startArrow"].as_str(), Some("Triangle"));
        assert_eq!(node["stroke"].as_str(), Some("#336699"), "the colour applied");
        // With nothing selected the dialog sets up the next object drawn (stroke.set's behaviour).
        app.run("select.none", json!({})).unwrap();
        open(&mut app).unwrap();
        let d = app.ui.dialog.take().unwrap();
        confirm(&mut app, &d).unwrap();
    }
}
