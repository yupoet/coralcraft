//! CoralCraft: the CorelDRAW page and master-layer daily use (P1 Task 10) — a three-page document
//! with an All-pages master layer: pages reorder by name, the master layer and its role survive a
//! native `.vectorcraft` save and reopen and a CDR7 round trip (pages and master layers map through
//! `corel-import`), and the exported SVG carries the master art.

use serde_json::json;
use vectorcraft_engine::Session;

fn pages_doc() -> Session {
    let mut s = Session::new();
    s.execute("file.new", &json!({ "width": 200, "height": 200 })).unwrap();
    s.execute("artboard.new", &json!({ "after": 0, "name": "Two" })).unwrap();
    s.execute("artboard.new", &json!({ "after": 1, "name": "Three" })).unwrap();
    s.execute("layer.new", &json!({ "name": "Logo Master", "role": "masterAll" })).unwrap();
    // Art on the master layer: it belongs to every page.
    let r = s.execute("shape.rectangle", &json!({ "x": 20, "y": 20, "width": 40, "height": 20 })).unwrap();
    let _ = r;
    s
}

/// The layers as (name, role) pairs — the names from the document's layer tree, the CorelDRAW
/// roles from the CoralCraft extension block (`unknown.coralcraft.layer_meta`, the block the
/// native format and `corel-import` save).
fn layers(s: &mut Session) -> Vec<(String, String)> {
    let doc = s.execute("document.json", &json!({})).unwrap();
    let mut out = vec![];
    let mut walk = |nodes: &Vec<serde_json::Value>, out: &mut Vec<(String, String)>| {
        for n in nodes {
            if n["kind"]["type"].as_str() == Some("layer") {
                let id = n["id"].as_u64().unwrap_or_default();
                let role = doc
                    .pointer("/unknown/coralcraft/layer_meta")
                    .and_then(|m| m.get(id.to_string()))
                    .and_then(|m| m["role"].as_str())
                    .unwrap_or("")
                    .to_string();
                out.push((n["name"].as_str().unwrap_or("").to_string(), role));
            }
        }
    };
    walk(doc["layers"].as_array().unwrap_or(&vec![]), &mut out);
    out
}

#[test]
fn pages_and_master_layers_survive_the_daily_round_trips() {
    let mut s = pages_doc();
    // Pages: three, by name; the reorder moves a name with its page.
    let names = |s: &mut Session| -> Vec<String> {
        let info = s.execute("document.info", &json!({})).unwrap();
        let doc = &info["document"];
        let arr = doc["artboards"].as_array().or_else(|| info["artboards"].as_array()).expect("the page list");
        arr.iter().filter_map(|p| p["name"].as_str().map(str::to_string)).collect()
    };
    let before = names(&mut s);
    assert_eq!(before.len(), 3, "three pages: {before:?}");
    s.execute("artboard.reorder", &json!({ "index": 1, "to": 2, "reflowPages": true })).unwrap();
    let after = names(&mut s);
    assert_ne!(before, after, "the reorder moved a page");
    assert_eq!(before.len(), after.len());
    // The master layer reports its role; the SVG carries the master art.
    assert!(
        layers(&mut s).iter().any(|(name, role)| name == "Logo Master" && role.starts_with("master_all")),
        "the master layer and its role: {:?}",
        layers(&mut s)
    );
    let svg = s.execute("document.serialize", &json!({ "format": "svg" })).unwrap()["text"].as_str().unwrap().to_string();
    assert!(svg.matches("<rect").count() >= 1, "the master art exports: {svg}");

    // Native reopen: pages, their order and the master layer's role all stay.
    let data = s.execute("document.serialize", &json!({ "format": "vectorcraft" })).unwrap()["dataBase64"].as_str().unwrap().to_string();
    s.execute("document.open", &json!({ "dataBase64": data })).unwrap();
    assert_eq!(names(&mut s), after, "the pages and their order reopened");
    assert!(
        layers(&mut s).iter().any(|(name, role)| name == "Logo Master" && role.starts_with("master_all")),
        "the master layer's role reopened: {:?}",
        layers(&mut s)
    );

    // CDR7 round trip: every page and the master layer survive `corel-import`'s mapping (the
    // reader's master-page heuristic no longer eats a named first page; P1 Task 11).
    let mut s = pages_doc();
    let cdr = s.execute("document.serialize", &json!({ "format": "cdr" })).unwrap()["dataBase64"].as_str().unwrap().to_string();
    s.execute("document.open", &json!({ "name": "pages.cdr", "dataBase64": cdr })).unwrap();
    assert_eq!(names(&mut s).len(), 3, "three pages reopened from CDR7: {:?}", names(&mut s));
    assert!(layers(&mut s).iter().any(|(_, role)| role.starts_with("master_all")), "the master layer reopened from CDR7: {:?}", layers(&mut s));
}
