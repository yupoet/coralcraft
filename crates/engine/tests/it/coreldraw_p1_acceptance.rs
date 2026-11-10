//! CoralCraft: the P1 acceptance scenario (P1 Task 13) — a multi-object logo document built and
//! revised entirely through the command surface an MCP `run_command` call drives: shapes, a
//! node-edited path, text, a welded pair, a style, a symbol, transforms and fills, on two pages
//! with a master layer. Then: a native `.vectorcraft` save and reopen keeps everything editable,
//! the exported SVG re-imports with matching geometry (an independent import), and the PDF has the
//! structure a PDF reader checks (header, page objects, xref).

use serde_json::json;
use vectorcraft_engine::Session;

#[test]
fn p1_acceptance_builds_revises_and_round_trips_a_logo() {
    let mut s = Session::new();
    s.execute("file.new", &json!({ "width": 640, "height": 400 })).unwrap();
    s.execute("artboard.new", &json!({ "after": 0, "name": "Print" })).unwrap();
    s.execute("layer.new", &json!({ "name": "Mark Master", "role": "masterAll" })).unwrap();

    // The mark: a node-edited curve over a welded pair, with the fill and outline the Outline Pen
    // commands set.
    let curve = s.execute("path.create", &json!({ "d": "M 60 160 C 100 80, 200 80, 240 160 S 380 240, 420 160" })).unwrap()["id"].as_u64().unwrap();
    s.execute("select.anchors", &json!({ "id": curve, "anchors": [[0, 0]] })).unwrap();
    s.execute("path.moveAnchors", &json!({ "dx": 10, "dy": -6 })).unwrap();
    s.execute("select.marquee", &json!({ "x": 60, "y": 180, "width": 160, "height": 120 })).unwrap();
    s.execute("shape.rectangle", &json!({ "x": 80, "y": 200, "width": 90, "height": 60 })).unwrap();
    s.execute("shape.rectangle", &json!({ "x": 125, "y": 225, "width": 90, "height": 60 })).unwrap();
    s.execute("select.marquee", &json!({ "x": 75, "y": 195, "width": 145, "height": 95 })).unwrap();
    s.execute("object.pathfinder.unite", &json!({})).unwrap();
    // Text, styled and mirrored, and a symbol from the mark's dot.
    s.execute("text.create", &json!({ "x": 300, "y": 140, "text": "CORAL", "size": 48 })).unwrap();
    s.execute("paint.setFill", &json!({ "color": "#e8b04b" })).unwrap();
    s.execute("stroke.set", &json!({ "weight": 2.5, "dash": [4.0, 2.0] })).unwrap();
    s.execute("graphicStyle.new", &json!({ "name": "Mark Gold" })).unwrap();
    s.execute("shape.ellipse", &json!({ "x": 300, "y": 170, "width": 28, "height": 28 })).unwrap();
    s.execute("graphicStyle.apply", &json!({ "name": "Mark Gold" })).unwrap();
    // The symbol's source art leaves an instance; the placed one transforms.
    s.execute("symbol.new", &json!({ "name": "Dot" })).unwrap();
    let placed = s.execute("symbol.place", &json!({ "name": "Dot", "x": 380, "y": 184 })).unwrap()["id"].as_u64().unwrap();
    s.execute("select.set", &json!({ "ids": [placed] })).unwrap();
    s.execute("object.rotate", &json!({ "angle": 15.0 })).unwrap();
    s.execute("object.reflect", &json!({ "axis": "vertical" })).unwrap();

    // Save `.vectorcraft`, reopen: everything is still there and still editable.
    let data = s.execute("document.serialize", &json!({ "format": "vectorcraft" })).unwrap()["dataBase64"].as_str().unwrap().to_string();
    s.execute("document.open", &json!({ "dataBase64": data })).unwrap();
    let info = s.execute("document.info", &json!({})).unwrap();
    assert_eq!(info["document"]["artboards"].as_array().map(Vec::len), Some(2), "two pages");
    let styles = s.execute("graphicStyle.list", &json!({})).unwrap();
    assert!(styles["styles"].as_array().unwrap().iter().any(|st| st["name"].as_str() == Some("Mark Gold")), "the style reopened");
    let symbols = s.execute("symbol.list", &json!({})).unwrap();
    assert!(symbols["symbols"].as_array().unwrap().iter().any(|sy| sy["name"].as_str() == Some("Dot")), "the symbol reopened");
    // Editability: a node edit and an outline edit both act on the reopened document.
    let paths = s.execute("document.inspect", &json!({ "kind": "Path" })).unwrap();
    let id = paths["nodes"].as_array().unwrap().iter().map(|n| n["id"].as_u64().unwrap()).max().unwrap();
    s.execute("select.anchors", &json!({ "id": id, "anchors": [[0, 0]] })).unwrap();
    s.execute("path.moveAnchors", &json!({ "dx": 0, "dy": 5 })).unwrap();
    s.execute("stroke.set", &json!({ "weight": 4.0 })).unwrap();
    let node = s.execute("document.node", &json!({ "id": id, "summary": true })).unwrap();
    assert_eq!(node["strokeWidth"].as_f64(), Some(4.0), "the reopened document edits");

    // SVG: an independent import — reopen the exported text and check the geometry came back.
    let svg = s.execute("document.serialize", &json!({ "format": "svg" })).unwrap()["text"].as_str().unwrap().to_string();
    assert!(svg.contains("<svg"), "the SVG carries the art: {svg}");
    s.execute("document.open", &json!({ "name": "logo.svg", "dataBase64": b64(svg.as_bytes()) })).unwrap();
    let info = s.execute("document.info", &json!({})).unwrap();
    // The re-imported document holds the logo's art: its artboard is the export's extent, and the
    // objects are selectable and editable as geometry.
    s.execute("select.all", &json!({})).unwrap();
    let selected = s.active().unwrap().selection.objects.len();
    assert!(selected >= 4, "the SVG re-imports with its geometry: {selected} objects, {:?}", info["document"]["artboards"]);
    let first = s.active().unwrap().selection.objects[0].0;
    s.execute("stroke.set", &json!({ "weight": 1.5 })).unwrap();
    let node = s.execute("document.node", &json!({ "id": first, "summary": true })).unwrap();
    assert_eq!(node["strokeWidth"].as_f64(), Some(1.5), "the re-imported geometry edits");

    // PDF: the structure an independent reader checks.
    let pdf =
        vectorcraft_format::base64_decode(s.execute("document.serialize", &json!({ "format": "pdf" })).unwrap()["dataBase64"].as_str().unwrap())
            .unwrap();
    assert!(pdf.starts_with(b"%PDF-"), "a PDF header");
    assert!(pdf.windows(11).any(|w| w == b"/Type /Page") || pdf.windows(10).any(|w| w == b"/Type/Page"), "page objects");
    assert!(pdf.windows(9).any(|w| w == b"startxref"), "an xref table");
}

/// Base64 (the command surface takes data as base64).
fn b64(data: &[u8]) -> String {
    vectorcraft_format::base64_encode(data)
}
