//! CoralCraft (P3 Task T1): the texture fill's reusable model — a document-level definition
//! shared by every object painted with it, edited and re-edited through commands, persisted
//! natively, and exported with independent SVG/PDF evidence and an explicit CDR/CMX fallback
//! warning. The matrix in `docs/p3-inventory.md` §19 freezes this slice's semantics; an ordinary
//! pattern relabelled as texture satisfies none of it.

use serde_json::json;
use vectorcraft_engine::Session;

const TEXTURE_FALLBACK_WARNING: &str =
    "texture fill is synthesized to tiles in CDR/CMX; the texture definition and its parameters do not survive the round trip";

fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", &json!({ "width": 400, "height": 300 })).unwrap();
    s
}

/// P1–P3, P5: create, apply, reuse, undo. (P4 propagation needs the open tile synthesis below.)
#[test]
fn texture_definitions_are_reusable_and_undo_round_trips() {
    let mut s = session();
    let a = s.execute("shape.rectangle", &json!({ "x": 20, "y": 20, "width": 90, "height": 60 })).unwrap()["id"].as_u64().unwrap();
    let b = s.execute("shape.rectangle", &json!({ "x": 200, "y": 20, "width": 90, "height": 60 })).unwrap()["id"].as_u64().unwrap();
    // P1: a named definition with parameters and two colours.
    s.execute(
        "texture.new",
        &json!({ "name": "Coral Stipple", "kind": "stipple", "density": 0.4, "foreground": "#c02020", "background": "#204080" }),
    )
    .unwrap();
    let list = s.execute("texture.list", &json!({})).unwrap();
    let defs = list["textures"].as_array().expect("the definition listed");
    assert_eq!(defs.len(), 1, "one definition: {defs:?}");
    assert_eq!(defs[0]["name"].as_str(), Some("Coral Stipple"));
    assert_eq!(defs[0]["kind"].as_str(), Some("stipple"));
    assert_eq!(defs[0]["foreground"].as_str(), Some("#c02020"));

    // P2: applying it paints the object with the texture (not a pattern, not a solid).
    s.execute("select.set", &json!({ "ids": [a] })).unwrap();
    s.execute("paint.setTexture", &json!({ "texture": "Coral Stipple" })).unwrap();
    let node = s.execute("document.node", &json!({ "id": a, "summary": true })).unwrap();
    assert_eq!(node["fill"].as_str(), Some("texture Coral Stipple"), "the paint is the texture: {node:?}");

    // P3: the second object reuses the ONE definition.
    s.execute("select.set", &json!({ "ids": [b] })).unwrap();
    s.execute("paint.setTexture", &json!({ "texture": "Coral Stipple" })).unwrap();
    let list = s.execute("texture.list", &json!({})).unwrap();
    assert_eq!(list["textures"].as_array().map(Vec::len), Some(1), "no definition duplicate");

    s.execute("texture.setProps", &json!({ "name": "Coral Stipple", "foreground": "#20a040" })).unwrap();
    // P5: undo/redo of the definition edit and of the paint application.
    s.execute("edit.undo", &json!({})).unwrap();
    let list = s.execute("texture.list", &json!({})).unwrap();
    assert_eq!(list["textures"][0]["foreground"].as_str(), Some("#c02020"), "undo restored the definition's colour");
    s.execute("edit.redo", &json!({})).unwrap();
    let list = s.execute("texture.list", &json!({})).unwrap();
    assert_eq!(list["textures"][0]["foreground"].as_str(), Some("#20a040"), "redo re-applied the edit");
    s.execute("select.set", &json!({ "ids": [b] })).unwrap();
    let undo_count = s.active().unwrap().history.undo.len();
    s.execute("paint.setFill", &json!({ "color": "#ffffff" })).unwrap();
    s.execute("paint.setTexture", &json!({ "texture": "Coral Stipple" })).unwrap();
    assert_eq!(s.active().unwrap().history.undo.len(), undo_count + 2, "each application is a step");
    s.execute("edit.undo", &json!({})).unwrap();
    let node = s.execute("document.node", &json!({ "id": b, "summary": true })).unwrap();
    assert_eq!(node["fill"].as_str(), Some("#ffffff"), "undo of the texture application restores the solid");
}

/// P6: native reopen keeps the definition and its users editable.
#[test]
fn texture_definitions_survive_native_reopen_and_still_edit() {
    let mut s = session();
    let a = s.execute("shape.rectangle", &json!({ "x": 20, "y": 20, "width": 90, "height": 60 })).unwrap()["id"].as_u64().unwrap();
    s.execute(
        "texture.new",
        &json!({ "name": "Coral Stipple", "kind": "stipple", "density": 0.4, "foreground": "#c02020", "background": "#204080" }),
    )
    .unwrap();
    s.execute("select.set", &json!({ "ids": [a] })).unwrap();
    s.execute("paint.setTexture", &json!({ "texture": "Coral Stipple" })).unwrap();
    let data = s.execute("document.serialize", &json!({ "format": "vectorcraft" })).unwrap()["dataBase64"].as_str().unwrap().to_string();
    s.execute("document.open", &json!({ "dataBase64": data })).unwrap();
    let list = s.execute("texture.list", &json!({})).unwrap();
    assert_eq!(list["textures"].as_array().map(Vec::len), Some(1), "the definition reopened");
    // The definition still edits after reopen (tile rendering is the open remainder).
    s.execute("texture.setProps", &json!({ "name": "Coral Stipple", "foreground": "#20a040" })).unwrap();
    let list = s.execute("texture.list", &json!({})).unwrap();
    assert_eq!(list["textures"][0]["foreground"].as_str(), Some("#20a040"), "the post-reopen edit took");
}

/// The open T1 remainder, kept as visible anchors: P4 (edit propagation through rendered tiles),
/// P7 and P8 (independent SVG/PDF tile reads). Run with `--ignored`; they fail until tile
/// synthesis lands, and T1 is not closed while they do.
#[ignore = "P3 T1 open: independent SVG/PDF tile synthesis (P4, P7, P8)"]
#[test]
fn texture_tiles_are_independently_readable_in_svg_and_pdf() {
    let mut s = session();
    let a = s.execute("shape.rectangle", &json!({ "x": 20, "y": 20, "width": 120, "height": 80 })).unwrap()["id"].as_u64().unwrap();
    s.execute(
        "texture.new",
        &json!({ "name": "Coral Stipple", "kind": "stipple", "density": 0.4, "foreground": "#c02020", "background": "#204080" }),
    )
    .unwrap();
    s.execute("select.set", &json!({ "ids": [a] })).unwrap();
    s.execute("paint.setTexture", &json!({ "texture": "Coral Stipple" })).unwrap();
    // Independent usvg read: the tiles' geometry and BOTH colours are in the SVG.
    let svg = s.execute("document.serialize", &json!({ "format": "svg" })).unwrap()["text"].as_str().unwrap().to_string();
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).expect("independent usvg parse");
    let mut colors = Vec::new();
    let mut geometry = 0usize;
    collect(tree.root(), &mut colors, &mut geometry);
    assert!(geometry > 1, "more than one tile shape: {geometry}");
    assert!(colors.iter().any(|c| c == "#c02020") && colors.iter().any(|c| c == "#204080"), "both texture colours render: {colors:?}");
    // Independent Hayro-backed PDF read: a page with the tile geometry (structure only).
    let pdf =
        vectorcraft_format::base64_decode(s.execute("document.serialize", &json!({ "format": "pdf" })).unwrap()["dataBase64"].as_str().unwrap())
            .unwrap();
    let imported = vectorcraft_pdf::import(&pdf).expect("independent Hayro-backed PDF read");
    let mut paths = 0usize;
    for layer in &imported.layers {
        layer.walk(&mut |node| {
            if matches!(node.kind, vectorcraft_doc::NodeKind::Path { .. }) {
                paths += 1;
            }
        });
    }
    assert!(paths > 1, "the PDF reconstructs the tile art: {paths}");
}

/// P9/N3: CDR/CMX export carries exactly one explicit texture fallback warning.
#[test]
fn cdr_cmx_export_warns_once_about_the_texture_fallback() {
    let mut s = session();
    let a = s.execute("shape.rectangle", &json!({ "x": 20, "y": 20, "width": 90, "height": 60 })).unwrap()["id"].as_u64().unwrap();
    s.execute(
        "texture.new",
        &json!({ "name": "Coral Stipple", "kind": "stipple", "density": 0.4, "foreground": "#c02020", "background": "#204080" }),
    )
    .unwrap();
    s.execute("select.set", &json!({ "ids": [a] })).unwrap();
    s.execute("paint.setTexture", &json!({ "texture": "Coral Stipple" })).unwrap();
    let cdr = s.execute("document.serialize", &json!({ "format": "cdr" })).unwrap();
    let warnings = cdr["warnings"].as_array().cloned().unwrap_or_default();
    let count = warnings.iter().filter(|w| w.as_str() == Some(TEXTURE_FALLBACK_WARNING)).count();
    assert_eq!(count, 1, "exactly one explicit texture fallback warning: {warnings:?}");
    let cmx = s.execute("document.serialize", &json!({ "format": "cmx" })).unwrap();
    let warnings = cmx["warnings"].as_array().cloned().unwrap_or_default();
    let count = warnings.iter().filter(|w| w.as_str() == Some(TEXTURE_FALLBACK_WARNING)).count();
    assert_eq!(count, 1, "the CMX writer warns too: {warnings:?}");
}

/// N1/N2/N4: no silent fallbacks and no pattern/texture confusion.
#[test]
fn texture_negatives_refuse_instead_of_falling_back() {
    let mut s = session();
    let a = s.execute("shape.rectangle", &json!({ "x": 20, "y": 20, "width": 90, "height": 60 })).unwrap()["id"].as_u64().unwrap();
    s.execute(
        "texture.new",
        &json!({ "name": "Coral Stipple", "kind": "stipple", "density": 0.4, "foreground": "#c02020", "background": "#204080" }),
    )
    .unwrap();
    // N1: an unknown texture errors and paints nothing.
    s.execute("select.set", &json!({ "ids": [a] })).unwrap();
    let before = s.execute("document.node", &json!({ "id": a, "summary": true })).unwrap();
    assert!(s.execute("paint.setTexture", &json!({ "texture": "No Such Texture" })).is_err());
    let after = s.execute("document.node", &json!({ "id": a, "summary": true })).unwrap();
    assert_eq!(before["fill"], after["fill"], "the failed paint changed nothing");
    // N2: the texture is not listed by pattern queries, and a pattern name is refused.
    let patterns = s.execute("pattern.list", &json!({})).unwrap();
    assert!(
        patterns["patterns"].as_array().is_none_or(|p| p.iter().all(|def| def["name"].as_str() != Some("Coral Stipple"))),
        "the texture does not appear among patterns: {patterns:?}"
    );
    s.execute("object.pattern.make", &json!({ "ids": [a], "name": "Plain Pattern", "edit": false })).unwrap();
    assert!(s.execute("paint.setTexture", &json!({ "texture": "Plain Pattern" })).is_err(), "a pattern name is not a texture");
    // N4: invalid definitions are refused without document change.
    let count = s.execute("texture.list", &json!({})).unwrap()["textures"].as_array().map(Vec::len).unwrap_or(0);
    for bad in [
        json!({ "name": "Bad Kind", "kind": "marble", "foreground": "#000000", "background": "#ffffff" }),
        json!({ "name": "Bad Density", "kind": "stipple", "density": 7.0, "foreground": "#000000", "background": "#ffffff" }),
        json!({ "name": "", "kind": "stipple", "density": 0.4 }),
    ] {
        assert!(s.execute("texture.new", &bad).is_err(), "invalid definition accepted: {bad}");
        assert!(s.execute("texture.setProps", &json!({ "name": "Coral Stipple", "foreground": "#c02020" })).is_ok());
    }
    assert_eq!(
        s.execute("texture.list", &json!({})).unwrap()["textures"].as_array().map(Vec::len),
        Some(count),
        "refused definitions changed nothing"
    );
}

fn collect(group: &usvg::Group, colors: &mut Vec<String>, geometry: &mut usize) {
    for node in group.children() {
        match node {
            usvg::Node::Group(child) => collect(child, colors, geometry),
            usvg::Node::Path(path) => {
                *geometry += 1;
                if let Some(fill) = path.fill()
                    && let usvg::Paint::Color(color) = fill.paint()
                {
                    colors.push(format!("#{:02x}{:02x}{:02x}", color.red, color.green, color.blue));
                }
            }
            usvg::Node::Image(_) | usvg::Node::Text(_) => {}
        }
    }
}

#[ignore = "P3 T1 open: independent SVG/PDF tile synthesis (P4, P7, P8)"]
#[test]
fn texture_edit_propagates_to_rendered_tiles() {
    let mut s = session();
    let a = s.execute("shape.rectangle", &json!({ "x": 20, "y": 20, "width": 90, "height": 60 })).unwrap()["id"].as_u64().unwrap();
    let b = s.execute("shape.rectangle", &json!({ "x": 200, "y": 20, "width": 90, "height": 60 })).unwrap()["id"].as_u64().unwrap();
    s.execute(
        "texture.new",
        &json!({ "name": "Coral Stipple", "kind": "stipple", "density": 0.4, "foreground": "#c02020", "background": "#204080" }),
    )
    .unwrap();
    for id in [a, b] {
        s.execute("select.set", &json!({ "ids": [id] })).unwrap();
        s.execute("paint.setTexture", &json!({ "texture": "Coral Stipple" })).unwrap();
    }
    let before = s.execute("document.serialize", &json!({ "format": "svg" })).unwrap()["text"].as_str().unwrap().to_string();
    s.execute("texture.setProps", &json!({ "name": "Coral Stipple", "foreground": "#20a040" })).unwrap();
    let after = s.execute("document.serialize", &json!({ "format": "svg" })).unwrap()["text"].as_str().unwrap().to_string();
    assert!(before.contains("#c02020") && !before.contains("#20a040"), "the old colour was rendered: {before}");
    assert!(after.contains("#20a040"), "the edited colour reaches every tile: {after}");
}
