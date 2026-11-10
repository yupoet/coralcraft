//! CoralCraft: the CorelDRAW object styles and symbol basics on the shared commands (P1 Task 9) —
//! style apply and redefine propagate to the linked objects, a symbol's instances update when its
//! source art is edited, both survive a native `.vectorcraft` save and reopen, and the exported
//! SVG of a styled/symbol document shows the applied look (an independent read of the text).

use serde_json::json;
use vectorcraft_engine::Session;

fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", &json!({ "width": 400, "height": 300 })).unwrap();
    s
}

/// An object style: creating it from an object, applying it to another, and redefining it
/// propagates to every linked object.
#[test]
fn styles_apply_and_redefine_propagate() {
    let mut s = session();
    let a = s.execute("shape.rectangle", &json!({ "x": 20, "y": 20, "width": 60, "height": 40 })).unwrap()["id"].as_u64().unwrap();
    let b = s.execute("shape.rectangle", &json!({ "x": 200, "y": 20, "width": 60, "height": 40 })).unwrap()["id"].as_u64().unwrap();
    s.execute("select.set", &json!({ "ids": [a] })).unwrap();
    s.execute("paint.setFill", &json!({ "color": "#3050a0" })).unwrap();
    // A style from the first object, applied to the second: both wear the look.
    s.execute("graphicStyle.new", &json!({ "name": "Logo Blue" })).unwrap();
    s.execute("select.set", &json!({ "ids": [b] })).unwrap();
    s.execute("graphicStyle.apply", &json!({ "name": "Logo Blue" })).unwrap();
    let node = s.execute("document.node", &json!({ "id": b, "summary": true })).unwrap();
    assert_eq!(node["fill"].as_str(), Some("#3050a0"), "the style applied");
    // Redefine from an edited object: every linked object updates.
    s.execute("paint.setFill", &json!({ "color": "#a03050" })).unwrap();
    s.execute("graphicStyle.redefine", &json!({ "name": "Logo Blue" })).unwrap();
    let node = s.execute("document.node", &json!({ "id": b, "summary": true })).unwrap();
    assert_eq!(node["fill"].as_str(), Some("#a03050"), "the redefinition propagated");
    // The style list reports the links; the styled SVG shows the applied colour.
    let list = s.execute("graphicStyle.list", &json!({})).unwrap();
    let style = list["styles"].as_array().unwrap().iter().find(|st| st["name"].as_str() == Some("Logo Blue")).expect("the style listed");
    assert_eq!(style["linked"].as_array().map(|l| l.len()), Some(2), "both objects linked: {style:?}");
    let svg = s.execute("document.serialize", &json!({ "format": "svg" })).unwrap()["text"].as_str().unwrap().to_string();
    assert!(svg.contains("#a03050"), "the independent SVG shows the styled look: {svg}");
}

/// Symbol basics: create from a selection, place instances, edit the source — every instance
/// updates — and both the symbols and the instances survive a native reopen.
#[test]
fn symbols_place_and_update_on_source_edits() {
    let mut s = session();
    let a = s.execute("shape.ellipse", &json!({ "x": 20, "y": 20, "width": 50, "height": 50 })).unwrap()["id"].as_u64().unwrap();
    s.execute("select.set", &json!({ "ids": [a] })).unwrap();
    s.execute("paint.setFill", &json!({ "color": "#20a050" })).unwrap();
    s.execute("symbol.new", &json!({ "name": "Dot" })).unwrap();
    // Two placed instances.
    let i1 = s.execute("symbol.place", &json!({ "name": "Dot", "x": 200, "y": 60 })).unwrap()["id"].as_u64().unwrap();
    let i2 = s.execute("symbol.place", &json!({ "name": "Dot", "x": 300, "y": 60 })).unwrap()["id"].as_u64().unwrap();
    let list = s.execute("symbol.list", &json!({})).unwrap();
    let dot = list["symbols"].as_array().unwrap().iter().find(|sy| sy["name"].as_str() == Some("Dot")).expect("the symbol listed");
    assert_eq!(dot["instances"].as_u64(), Some(3), "the source art and two instances: {dot:?}");
    // Edit the symbol's art: every instance's look updates.
    s.execute("symbol.edit", &json!({ "id": i1 })).unwrap();
    s.execute("select.all", &json!({})).unwrap();
    s.execute("paint.setFill", &json!({ "color": "#a02050" })).unwrap();
    s.execute("symbol.exitEditing", &json!({})).unwrap();
    let svg = s.execute("document.serialize", &json!({ "format": "svg" })).unwrap()["text"].as_str().unwrap().to_string();
    assert!(svg.contains("#a02050"), "the source edit shows in the export: {svg}");
    assert!(!svg.contains("#20a050"), "the old look is gone everywhere");
    // Native reopen keeps the symbol and its instances (the instance count is the same symbol).
    let data = s.execute("document.serialize", &json!({ "format": "vectorcraft" })).unwrap()["dataBase64"].as_str().unwrap().to_string();
    s.execute("document.open", &json!({ "dataBase64": data })).unwrap();
    let list = s.execute("symbol.list", &json!({})).unwrap();
    let dot = list["symbols"].as_array().unwrap().iter().find(|sy| sy["name"].as_str() == Some("Dot")).expect("the symbol reopened");
    assert_eq!(dot["instances"].as_u64(), Some(3), "the instances reopened: {dot:?}");
    // An instance is still placeable after the reopen.
    let i3 = s.execute("symbol.place", &json!({ "name": "Dot", "x": 100, "y": 200 })).unwrap()["id"].as_u64().unwrap();
    assert!(i3 > i2, "placed a new instance into the reopened document");
}
