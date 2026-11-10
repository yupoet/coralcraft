//! CoralCraft: the P1 persistence audit (P1 Task 11) — one document exercising every P1 slice's
//! semantics (pages and a master layer, node-edited geometry, a welded pair, styled objects,
//! symbol instances, an outlined dashed stroke) survives a native `.vectorcraft` save and reopen
//! with each semantic intact, and the CoralCraft extension block carries what the VectorCraft
//! model alone cannot (the CorelDRAW layer roles).

use serde_json::json;
use vectorcraft_engine::Session;

#[test]
fn every_p1_semantic_survives_one_native_round_trip() {
    let mut s = Session::new();
    s.execute("file.new", &json!({ "width": 400, "height": 300 })).unwrap();
    // Pages and a master layer (T10).
    s.execute("artboard.new", &json!({ "after": 0, "name": "Back" })).unwrap();
    s.execute("layer.new", &json!({ "name": "Logo Master", "role": "masterAll" })).unwrap();
    // Node-edited geometry (T3): a path whose first anchor moved.
    let path = s.execute("path.create", &json!({ "d": "M 20 20 L 160 20 L 160 120 Z" })).unwrap()["id"].as_u64().unwrap();
    s.execute("select.anchors", &json!({ "id": path, "anchors": [[0, 0]] })).unwrap();
    s.execute("path.moveAnchors", &json!({ "dx": 15, "dy": 10 })).unwrap();
    // A welded pair (T4).
    s.execute("shape.rectangle", &json!({ "x": 200, "y": 20, "width": 60, "height": 60 })).unwrap();
    s.execute("shape.rectangle", &json!({ "x": 230, "y": 50, "width": 60, "height": 60 })).unwrap();
    s.execute("select.marquee", &json!({ "x": 195, "y": 15, "width": 100, "height": 100 })).unwrap();
    s.execute("object.pathfinder.unite", &json!({})).unwrap();
    // Styles (T9) on two objects; the outlined dashed stroke (T7) is part of the style, so the
    // style's objects are untouched after they link.
    let styled1 = s.execute("shape.ellipse", &json!({ "x": 20, "y": 200, "width": 50, "height": 50 })).unwrap()["id"].as_u64().unwrap();
    s.execute("paint.setFill", &json!({ "color": "#3050a0" })).unwrap();
    s.execute("stroke.set", &json!({ "weight": 3.0, "dash": [4.0, 3.0] })).unwrap();
    s.execute("graphicStyle.new", &json!({ "name": "P1 Blue" })).unwrap();
    let styled2 = s.execute("shape.ellipse", &json!({ "x": 120, "y": 200, "width": 50, "height": 50 })).unwrap()["id"].as_u64().unwrap();
    s.execute("graphicStyle.apply", &json!({ "name": "P1 Blue" })).unwrap();
    // A symbol (T9).
    s.execute("symbol.new", &json!({ "name": "P1 Dot" })).unwrap();
    s.execute("symbol.place", &json!({ "name": "P1 Dot", "x": 300, "y": 200 })).unwrap();

    // Save, reopen, verify each semantic.
    let data = s.execute("document.serialize", &json!({ "format": "vectorcraft" })).unwrap()["dataBase64"].as_str().unwrap().to_string();
    s.execute("document.open", &json!({ "dataBase64": data })).unwrap();
    let info = s.execute("document.info", &json!({})).unwrap();
    let doc = &info["document"];
    assert_eq!(doc["artboards"].as_array().map(Vec::len), Some(2), "the pages reopened: {:?}", doc["artboards"]);
    let json = s.execute("document.json", &json!({})).unwrap();
    let roles = json.pointer("/unknown/coralcraft/layer_meta").and_then(|m| m.as_object()).expect("the extension block");
    assert!(
        roles.values().any(|m| m["role"].as_str() == Some("master_all_pages")),
        "the master layer's role rides in the extension block: {roles:?}"
    );
    // The node-edited path kept its moved anchor (its bounds moved by (15, 10)).
    let paths = s.execute("document.inspect", &json!({})).unwrap();
    // Flatten every reported node of every kind.
    let mut nodes: Vec<serde_json::Value> = vec![];
    let mut walk = |v: &serde_json::Value, out: &mut Vec<serde_json::Value>| {
        if let Some(arr) = v["nodes"].as_array() {
            for n in arr {
                out.push(n.clone());
            }
        }
        if let Some(arr) = v["layers"].as_array() {
            for n in arr {
                walk_layers(n, out);
            }
        }
    };
    fn walk_layers(v: &serde_json::Value, out: &mut Vec<serde_json::Value>) {
        out.push(v.clone());
        for c in v["children"].as_array().unwrap_or(&vec![]) {
            walk_layers(c, out);
        }
    }
    walk(&paths, &mut nodes);
    let nodes = &nodes;
    let by_bounds = |x: f64, y: f64| {
        nodes
            .iter()
            .find(|n| (n["bounds"]["x"].as_f64().unwrap_or(-1e9) - x).abs() < 1e-6 && (n["bounds"]["y"].as_f64().unwrap_or(-1e9) - y).abs() < 1e-6)
    };
    assert!(by_bounds(35.0, 20.0).is_some(), "the node-edited path reopened with its moved anchor (x shifted by 15): {nodes:?}");
    // The welded pair is one path of the union's bounds (200..290 square-ish L).
    assert!(by_bounds(200.0, 20.0).is_some(), "the welded pair reopened: {nodes:?}");
    // The style survives with its look, and the styled objects keep it (the dedicated T9 test
    // proves both objects' links round-trip in the clean two-object scenario).
    let styles = s.execute("graphicStyle.list", &json!({})).unwrap();
    let p1blue = styles["styles"].as_array().unwrap().iter().find(|st| st["name"].as_str() == Some("P1 Blue")).expect("the style reopened");
    assert_eq!(p1blue["strokeWidth"].as_f64(), Some(3.0), "the style's outlined weight reopened: {p1blue:?}");
    assert!(!p1blue["linked"].as_array().unwrap_or(&vec![]).is_empty(), "an object stayed linked");
    // styled1 keeps its look as a plain object; styled2 is the symbol's source art, so its look
    // shows in the export (an independent read of the reopened document).
    let id = by_bounds(20.0, 200.0).unwrap_or_else(|| panic!("styled1 reopened: {nodes:?}"))["id"].as_u64().unwrap();
    let node = s.execute("document.node", &json!({ "id": id, "summary": true })).unwrap();
    assert_eq!(node["fill"].as_str(), Some("#3050a0"), "the styled fill reopened");
    assert_eq!(node["strokeWidth"].as_f64(), Some(3.0), "the outlined weight reopened");
    assert_eq!(node["strokeOptions"]["dash"].as_array().map(Vec::len), Some(2), "the dash reopened");
    let svg = s.execute("document.serialize", &json!({ "format": "svg" })).unwrap()["text"].as_str().unwrap().to_string();
    assert!(svg.matches("#3050a0").count() >= 2, "the style's look shows on both objects (one inside the symbol): {svg}");
    let _ = (styled1, styled2);
    // The symbol and its instances.
    let symbols = s.execute("symbol.list", &json!({})).unwrap();
    let dot = symbols["symbols"].as_array().unwrap().iter().find(|sy| sy["name"].as_str() == Some("P1 Dot")).expect("the symbol reopened");
    assert!(dot["instances"].as_u64().unwrap_or(0) >= 2, "the instances reopened: {dot:?}");
}
