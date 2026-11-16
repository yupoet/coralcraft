//! CoralCraft (P3 Task T2): the Corel two-colour pattern parameters — a document-level pattern
//! definition carrying the geometric kind, the foreground and background colours and the tile
//! size, mapped onto the synced pattern architecture (an ordinary `Paint::Pattern` naming it),
//! edited per parameter through commands, persisted natively, proven by independent SVG/PDF
//! evidence and exact CDR/CMX import/export mappings. The matrix in `docs/p3-inventory.md` §22
//! freezes this slice's semantics; generic `PatternDef.art` workflows satisfy none of it.

use serde_json::json;
use vectorcraft_engine::Session;

const GENERIC_PATTERN_WARNING: &str = "generic Pattern paints are substituted with a default black/white checkerboard/black or dropped in CDR/CMX; native pattern definitions and paints remain editable";
const UNKNOWN_PATTERN_KIND_WARNING: &str =
    "unknown Corel pattern kinds import as a solid blend of their two colours; the original fill stays in the residual";

fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", &json!({ "width": 400, "height": 300 })).unwrap();
    s
}

/// P1–P3: every kind creates and lists, the paint applies, and one definition is reused.
#[test]
fn two_color_patterns_create_list_apply_and_reuse() {
    let mut s = session();
    let a = s.execute("shape.rectangle", &json!({ "x": 20, "y": 20, "width": 90, "height": 60 })).unwrap()["id"].as_u64().unwrap();
    let b = s.execute("shape.rectangle", &json!({ "x": 200, "y": 20, "width": 90, "height": 60 })).unwrap()["id"].as_u64().unwrap();
    // P1: each of the five kinds creates a named definition and lists its parameters.
    for (i, kind) in ["checkerboard", "polkaDot", "horizontalStripe", "verticalStripe", "diagonalCrosshatch"].into_iter().enumerate() {
        let name = format!("Coral {kind}");
        let created = s
            .execute("pattern.twoColor", &json!({ "name": name, "kind": kind, "foreground": "#c02020", "background": "#204080", "tileSize": 18.0 }))
            .unwrap();
        assert_eq!(created["name"].as_str(), Some(name.as_str()), "the created name comes back");
        let list = s.execute("pattern.list", &json!({})).unwrap();
        let defs = list["patterns"].as_array().expect("the pattern list");
        let def = defs.iter().find(|d| d["name"].as_str() == Some(name.as_str())).unwrap_or_else(|| panic!("{kind} listed: {defs:?}"));
        assert_eq!(def["twoColor"]["kind"].as_str(), Some(kind), "the source kind is listed");
        assert_eq!(def["twoColor"]["foreground"].as_str(), Some("#c02020"));
        assert_eq!(def["twoColor"]["background"].as_str(), Some("#204080"));
        assert_eq!(def["twoColor"]["tileSize"].as_f64(), Some(18.0));
        assert_eq!(i + 1, defs.len(), "one definition per kind: {defs:?}");
    }
    // P2: applying it paints the object with the pattern (not a texture, not a solid).
    s.execute("select.set", &json!({ "ids": [a] })).unwrap();
    s.execute("paint.setPattern", &json!({ "pattern": "Coral polkaDot" })).unwrap();
    let node = s.execute("document.node", &json!({ "id": a, "summary": true })).unwrap();
    assert_eq!(node["fill"].as_str(), Some("pattern Coral polkaDot"), "the paint is the pattern: {node:?}");
    // P3: the second object reuses the ONE definition.
    s.execute("select.set", &json!({ "ids": [b] })).unwrap();
    s.execute("paint.setPattern", &json!({ "pattern": "Coral polkaDot" })).unwrap();
    let list = s.execute("pattern.list", &json!({})).unwrap();
    assert_eq!(list["patterns"].as_array().map(Vec::len), Some(5), "no definition duplicate");
}

/// P4: each parameter edits through `pattern.setTwoColorProps`, reaches every user's exported
/// tiles, and undo/redo round-trips exactly.
#[test]
fn each_two_color_parameter_edits_and_undo_redoes() {
    let mut s = session();
    let a = s.execute("shape.rectangle", &json!({ "x": 20, "y": 20, "width": 90, "height": 60 })).unwrap()["id"].as_u64().unwrap();
    let b = s.execute("shape.rectangle", &json!({ "x": 200, "y": 20, "width": 90, "height": 60 })).unwrap()["id"].as_u64().unwrap();
    s.execute(
        "pattern.twoColor",
        &json!({ "name": "Coral Checks", "kind": "checkerboard", "foreground": "#c02020", "background": "#204080", "tileSize": 12.0 }),
    )
    .unwrap();
    for id in [a, b] {
        s.execute("select.set", &json!({ "ids": [id] })).unwrap();
        s.execute("paint.setPattern", &json!({ "pattern": "Coral Checks" })).unwrap();
    }
    let listed = |s: &mut Session| {
        let list = s.execute("pattern.list", &json!({})).unwrap();
        list["patterns"].as_array().unwrap().iter().find(|d| d["name"].as_str() == Some("Coral Checks")).cloned().unwrap()
    };
    let svg_colours = |s: &mut Session| {
        let svg = s.execute("document.serialize", &json!({ "format": "svg" })).unwrap()["text"].as_str().unwrap().to_string();
        (svg.contains("#c02020"), svg.contains("#20a040"), svg.contains("#204080"))
    };
    // fg: the exported colour changes everywhere.
    s.execute("pattern.setTwoColorProps", &json!({ "name": "Coral Checks", "foreground": "#20a040" })).unwrap();
    assert_eq!(listed(&mut s)["twoColor"]["foreground"].as_str(), Some("#20a040"));
    assert_eq!(svg_colours(&mut s), (false, true, true), "the edited foreground replaced the old one in the export");
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(listed(&mut s)["twoColor"]["foreground"].as_str(), Some("#c02020"), "undo restored the foreground");
    assert_eq!(svg_colours(&mut s), (true, false, true));
    s.execute("edit.redo", &json!({})).unwrap();
    assert_eq!(listed(&mut s)["twoColor"]["foreground"].as_str(), Some("#20a040"), "redo re-applied the edit");
    // bg: same propagation.
    s.execute("pattern.setTwoColorProps", &json!({ "name": "Coral Checks", "background": "#f0e0d0" })).unwrap();
    assert_eq!(listed(&mut s)["twoColor"]["background"].as_str(), Some("#f0e0d0"));
    let svg = s.execute("document.serialize", &json!({ "format": "svg" })).unwrap()["text"].as_str().unwrap().to_string();
    assert!(svg.contains("#f0e0d0") && !svg.contains("#204080"), "the edited background replaced the old one: {svg}");
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(listed(&mut s)["twoColor"]["background"].as_str(), Some("#204080"), "undo restored the background");
    s.execute("edit.redo", &json!({})).unwrap();
    // kind: the tile art's geometry changes (a polka-dot tile holds a curved dot; a checkerboard
    // tile holds straight-edged quadrant rectangles).
    let curved = |s: &mut Session| -> usize {
        let svg = s.execute("document.serialize", &json!({ "format": "svg" })).unwrap()["text"].as_str().unwrap().to_string();
        let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).expect("independent usvg parse");
        let mut patterns = vec![];
        pattern_paints(tree.root(), &mut patterns);
        let [pattern] = patterns.as_slice() else { panic!("one pattern: {svg}") };
        pattern_paths(pattern.root()).iter().filter(|(_, curved)| *curved).count()
    };
    let straight_before = curved(&mut s);
    s.execute("pattern.setTwoColorProps", &json!({ "name": "Coral Checks", "kind": "polkaDot" })).unwrap();
    assert_eq!(listed(&mut s)["twoColor"]["kind"].as_str(), Some("polkaDot"));
    assert!(curved(&mut s) > straight_before, "the polka-dot tile adds curved geometry the checkerboard tile lacks");
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(listed(&mut s)["twoColor"]["kind"].as_str(), Some("checkerboard"), "undo restored the kind");
    assert_eq!(curved(&mut s), straight_before);
    s.execute("edit.redo", &json!({})).unwrap();
    // tileSize: the exported pattern period changes.
    let tile_of = |s: &mut Session| -> (f32, f32) {
        let svg = s.execute("document.serialize", &json!({ "format": "svg" })).unwrap()["text"].as_str().unwrap().to_string();
        let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).expect("independent usvg parse");
        let mut patterns = vec![];
        pattern_paints(tree.root(), &mut patterns);
        let [pattern] = patterns.as_slice() else { panic!("one pattern: {svg}") };
        (pattern.rect().width(), pattern.rect().height())
    };
    s.execute("pattern.setTwoColorProps", &json!({ "name": "Coral Checks", "tileSize": 30.0 })).unwrap();
    assert_eq!(listed(&mut s)["twoColor"]["tileSize"].as_f64(), Some(30.0));
    assert_eq!(tile_of(&mut s), (30.0, 30.0), "the exported tile follows the edited size");
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(tile_of(&mut s), (12.0, 12.0), "undo restored the tile size");
    s.execute("edit.redo", &json!({})).unwrap();
    // Each parameter edit is one undo step.
    let undo_count = s.active().unwrap().history.undo.len();
    s.execute("pattern.setTwoColorProps", &json!({ "name": "Coral Checks", "foreground": "#101010" })).unwrap();
    assert_eq!(s.active().unwrap().history.undo.len(), undo_count + 1, "one edit is one step");
}

/// P5: native reopen keeps the definition, its parameters and its users; each parameter still
/// edits after reopen.
#[test]
fn two_color_definitions_survive_native_reopen_and_still_edit() {
    let mut s = session();
    let a = s.execute("shape.rectangle", &json!({ "x": 20, "y": 20, "width": 90, "height": 60 })).unwrap()["id"].as_u64().unwrap();
    s.execute(
        "pattern.twoColor",
        &json!({ "name": "Coral Hatch", "kind": "diagonalCrosshatch", "foreground": "#c02020", "background": "#204080", "tileSize": 15.0 }),
    )
    .unwrap();
    s.execute("select.set", &json!({ "ids": [a] })).unwrap();
    s.execute("paint.setPattern", &json!({ "pattern": "Coral Hatch" })).unwrap();
    let data = s.execute("document.serialize", &json!({ "format": "vectorcraft" })).unwrap()["dataBase64"].as_str().unwrap().to_string();
    s.execute("document.open", &json!({ "dataBase64": data })).unwrap();
    let list = s.execute("pattern.list", &json!({})).unwrap();
    let def = list["patterns"].as_array().unwrap().iter().find(|d| d["name"].as_str() == Some("Coral Hatch")).expect("the definition reopened");
    assert_eq!(def["twoColor"]["kind"].as_str(), Some("diagonalCrosshatch"));
    assert_eq!(def["twoColor"]["foreground"].as_str(), Some("#c02020"));
    assert_eq!(def["twoColor"]["background"].as_str(), Some("#204080"));
    assert_eq!(def["twoColor"]["tileSize"].as_f64(), Some(15.0));
    let node = s.execute("document.node", &json!({ "id": a, "summary": true })).unwrap();
    assert_eq!(node["fill"].as_str(), Some("pattern Coral Hatch"), "the user reopened with its paint");
    // Each parameter still edits after reopen, and the edit reaches the export.
    for (key, value) in
        [("foreground", json!("#20a040")), ("background", json!("#f0e0d0")), ("kind", json!("verticalStripe")), ("tileSize", json!(24.0))]
    {
        let mut params = json!({ "name": "Coral Hatch" });
        params[key] = value;
        s.execute("pattern.setTwoColorProps", &params).unwrap();
    }
    let list = s.execute("pattern.list", &json!({})).unwrap();
    let def = list["patterns"].as_array().unwrap().iter().find(|d| d["name"].as_str() == Some("Coral Hatch")).unwrap();
    assert_eq!(def["twoColor"]["kind"].as_str(), Some("verticalStripe"));
    assert_eq!(def["twoColor"]["foreground"].as_str(), Some("#20a040"));
    assert_eq!(def["twoColor"]["background"].as_str(), Some("#f0e0d0"));
    assert_eq!(def["twoColor"]["tileSize"].as_f64(), Some(24.0));
    let svg = s.execute("document.serialize", &json!({ "format": "svg" })).unwrap()["text"].as_str().unwrap().to_string();
    assert!(svg.contains("#20a040") && svg.contains("#f0e0d0"), "the post-reopen edits reached the export: {svg}");
}

/// P6: independent `usvg` reads the connected pattern's tile rect, both colours, and kind-specific
/// geometry (a polka-dot tile holds a curved dot where a checkerboard tile holds straight edges).
#[test]
fn two_color_tiles_are_independently_readable_in_svg() {
    let mut s = session();
    let a = s.execute("shape.rectangle", &json!({ "x": 20, "y": 20, "width": 120, "height": 80 })).unwrap()["id"].as_u64().unwrap();
    s.execute(
        "pattern.twoColor",
        &json!({ "name": "Coral Dots", "kind": "polkaDot", "foreground": "#c02020", "background": "#204080", "tileSize": 18.0 }),
    )
    .unwrap();
    s.execute("select.set", &json!({ "ids": [a] })).unwrap();
    s.execute("paint.setStroke", &json!({ "none": true })).unwrap();
    s.execute("paint.setPattern", &json!({ "pattern": "Coral Dots" })).unwrap();
    let svg = s.execute("document.serialize", &json!({ "format": "svg" })).unwrap()["text"].as_str().unwrap().to_string();
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).expect("independent usvg parse");
    let mut patterns = vec![];
    pattern_paints(tree.root(), &mut patterns);
    let [pattern] = patterns.as_slice() else { panic!("one actual two-colour pattern: {svg}") };
    let tile = pattern.rect();
    assert_eq!((tile.x(), tile.y(), tile.width(), tile.height()), (0.0, 0.0, 18.0, 18.0), "the actual tile size");
    for rgb in [[192, 32, 32], [32, 64, 128]] {
        assert!(has_solid_vector(pattern.root(), rgb), "the tile contains non-empty {rgb:?} vector art: {svg}");
    }
    let paths = pattern_paths(pattern.root());
    assert!(paths.iter().any(|(_, curved)| *curved), "a polka-dot tile holds a curved dot (the kind maps to real geometry): {svg}");
}

/// P7: independent Hayro-backed PDF read reconstructs the tile art as a connected pattern
/// definition in both colours (structure, not live-PDF claims).
#[test]
fn two_color_tiles_are_independently_readable_in_pdf() {
    let mut s = session();
    let a = s.execute("shape.rectangle", &json!({ "x": 20, "y": 20, "width": 120, "height": 80 })).unwrap()["id"].as_u64().unwrap();
    s.execute(
        "pattern.twoColor",
        &json!({ "name": "Coral Dots", "kind": "polkaDot", "foreground": "#c02020", "background": "#204080", "tileSize": 18.0 }),
    )
    .unwrap();
    s.execute("select.set", &json!({ "ids": [a] })).unwrap();
    s.execute("paint.setStroke", &json!({ "none": true })).unwrap();
    s.execute("paint.setPattern", &json!({ "pattern": "Coral Dots" })).unwrap();
    let imported = imported_pdf(&mut s);
    let definition = imported
        .patterns
        .iter()
        .find(|definition| art_has_colours(&definition.art, &["#c02020", "#204080"]))
        .expect("Hayro reconstructs the two-colour pattern definition");
    assert!((definition.tile.width() - 18.0).abs() < 1e-4 && (definition.tile.height() - 18.0).abs() < 1e-4, "{:?}", definition.tile);
    let mut connected = false;
    imported.walk(|node| {
        connected |= matches!(node.appearance.fill_paint(), vectorcraft_color::Paint::Pattern { pattern, .. } if *pattern == definition.name);
    });
    assert!(connected, "the reconstructed target references the reconstructed tile art");
}

/// P8 + N5: CDR import maps `FillStyle::TwoColorPattern` to a definition with exactly those
/// parameters and a pattern paint naming it; an unknown kind degrades with a named warning.
#[test]
fn cdr_import_maps_two_color_fills_exactly() {
    use corel_import::coralcraft_color::Color as CdrColor;
    use corel_import::coralcraft_doc::{Document, FillStyle, ObjectId, ObjectKind, ObjectNode, PatternKind};
    use corel_import::coralcraft_geom::{F64, Rect, RectSpec};
    use vectorcraft_color::Paint;

    let mut cdr = Document::new_a4_portrait();
    for (i, kind) in [PatternKind::Checkerboard, PatternKind::PolkaDot, PatternKind::Unknown].into_iter().enumerate() {
        cdr.pages[0].layers[0].objects.push(ObjectNode::new(
            ObjectId(101 + i as u64),
            Some(format!("Pattern {i}")),
            ObjectKind::Rect(RectSpec::sharp(Rect::new(30.0, 30.0 + i as f64 * 80.0, 150.0, 90.0 + i as f64 * 80.0).unwrap())),
            Some(FillStyle::TwoColorPattern {
                kind,
                fg_color: CdrColor::rgb8(180, 20, 40),
                bg_color: CdrColor::rgb8(240, 245, 250),
                tile_size_pt: F64::new(18.0).unwrap(),
            }),
            None,
        ));
    }
    let (doc, warnings) = corel_import::import_document(&cdr).expect("import");
    // The two known kinds map to definitions with exactly those parameters (read through the
    // serialized definition, so the assertion names the persisted mapping, not a helper's word).
    let defs = serde_json::to_value(&doc.patterns).expect("the definitions serialize");
    let defs = defs.as_array().expect("the patterns are a list");
    let with_source: Vec<&serde_json::Value> = defs.iter().filter(|d| d.get("twoColor").is_some_and(|s| !s.is_null())).collect();
    assert_eq!(with_source.len(), 2, "one definition per known kind: {defs:?}");
    for def in &with_source {
        let source = def["twoColor"].as_object().expect("the source is an object");
        let fg = serde_json::to_value(vectorcraft_color::Color::rgb8(180, 20, 40)).unwrap();
        let bg = serde_json::to_value(vectorcraft_color::Color::rgb8(240, 245, 250)).unwrap();
        assert_eq!(source.get("foreground"), Some(&fg), "the foreground colour maps exactly");
        assert_eq!(source.get("background"), Some(&bg), "the background colour maps exactly");
        let tile = def["tile"].as_object().expect("the tile is a rect");
        let (x0, x1, y0, y1) =
            (tile["x0"].as_f64().unwrap(), tile["x1"].as_f64().unwrap(), tile["y0"].as_f64().unwrap(), tile["y1"].as_f64().unwrap());
        assert!((x1 - x0 - 18.0).abs() < 1e-9 && (y1 - y0 - 18.0).abs() < 1e-9, "the tile size maps exactly: {tile:?}");
    }
    let kinds: Vec<&str> = with_source.iter().filter_map(|d| d["twoColor"]["kind"].as_str()).collect();
    assert!(kinds.contains(&"checkerboard") && kinds.contains(&"polkaDot"), "the kinds map: {kinds:?}");
    // A user's fill is the pattern paint naming its definition.
    let mut painted = 0;
    doc.walk(|node| {
        if let Paint::Pattern { pattern, .. } = node.appearance.fill_paint() {
            assert!(doc.patterns.iter().any(|d| d.name == *pattern), "the paint names a real definition: {pattern}");
            painted += 1;
        }
    });
    assert_eq!(painted, 2, "both known-kind objects carry the pattern paint");
    // N5: the unknown kind degrades to the solid blend with a named warning, never silently.
    assert!(warnings.iter().any(|w| w == UNKNOWN_PATTERN_KIND_WARNING), "the unknown kind warns by name: {warnings:?}");
    let mut solids = 0;
    let blend = vectorcraft_color::Color::rgb8(180, 20, 40).lerp(&vectorcraft_color::Color::rgb8(240, 245, 250), 0.25).to_hex();
    doc.walk(|node| {
        let paint = node.appearance.fill_paint();
        if paint.color().is_some_and(|c| c.to_hex() == blend) {
            solids += 1;
        }
    });
    assert_eq!(solids, 1, "the unknown kind's object takes the solid blend");
}

/// P9: CDR export writes the exact `FillStyle::TwoColorPattern` parameters for a two-colour
/// pattern (with no generic warning), and a generic art pattern still warns exactly once.
#[test]
fn cdr_export_maps_two_color_definitions_exactly_and_generic_patterns_still_warn() {
    use corel_import::coralcraft_doc::FillStyle;

    let mut s = session();
    let a = s.execute("shape.rectangle", &json!({ "x": 20, "y": 20, "width": 90, "height": 60 })).unwrap()["id"].as_u64().unwrap();
    s.execute(
        "pattern.twoColor",
        &json!({ "name": "Coral Hatch", "kind": "diagonalCrosshatch", "foreground": "#b41428", "background": "#f0f5fa", "tileSize": 18.0 }),
    )
    .unwrap();
    s.execute("select.set", &json!({ "ids": [a] })).unwrap();
    s.execute("paint.setPattern", &json!({ "pattern": "Coral Hatch" })).unwrap();
    let (cdr_doc, warnings) = {
        let st = s.doc().unwrap();
        corel_import::export_document_with_warnings(&st.doc).expect("export")
    };
    assert!(
        !warnings.iter().any(|w| w == GENERIC_PATTERN_WARNING),
        "a two-colour pattern is a real mapping, not the generic substitution: {warnings:?}"
    );
    let mut exact = 0;
    for page in &cdr_doc.pages {
        for layer in &page.layers {
            for object in &layer.objects {
                if let Some(FillStyle::TwoColorPattern { kind, fg_color, bg_color, tile_size_pt }) = &object.fill {
                    assert_eq!(kind, &corel_import::coralcraft_doc::PatternKind::DiagonalCrosshatch, "the kind maps exactly");
                    assert_eq!(fg_color.to_hex(), "#b41428", "the foreground maps exactly");
                    assert_eq!(bg_color.to_hex(), "#f0f5fa", "the background maps exactly");
                    assert!((tile_size_pt.get() - 18.0).abs() < 1e-9, "the tile size maps exactly");
                    exact += 1;
                }
            }
        }
    }
    assert_eq!(exact, 1, "the pattern-filled object exports as TwoColorPattern");
    // The engine-level CDR/CMX serializations carry no generic warning either.
    for format in ["cdr", "cmx"] {
        let out = s.execute("document.serialize", &json!({ "format": format })).unwrap();
        let warnings = out["warnings"].as_array().cloned().unwrap_or_default();
        assert!(
            !warnings.iter().any(|w| w.as_str() == Some(GENERIC_PATTERN_WARNING)),
            "no generic substitution warning for the real mapping ({format}): {warnings:?}"
        );
    }
    // A generic art pattern keeps exactly one named generic warning.
    s.execute("object.pattern.make", &json!({ "ids": [a], "name": "Plain Pattern", "edit": false })).unwrap();
    s.execute("select.set", &json!({ "ids": [a] })).unwrap();
    s.execute("paint.setPattern", &json!({ "pattern": "Plain Pattern" })).unwrap();
    let out = s.execute("document.serialize", &json!({ "format": "cdr" })).unwrap();
    let warnings = out["warnings"].as_array().cloned().unwrap_or_default();
    let count = warnings.iter().filter(|w| w.as_str() == Some(GENERIC_PATTERN_WARNING)).count();
    assert_eq!(count, 1, "exactly one generic-pattern warning: {warnings:?}");
}

/// N1–N4: no silent fallbacks and no pattern/texture confusion.
#[test]
fn two_color_negatives_refuse_instead_of_falling_back() {
    let mut s = session();
    let a = s.execute("shape.rectangle", &json!({ "x": 20, "y": 20, "width": 90, "height": 60 })).unwrap()["id"].as_u64().unwrap();
    s.execute(
        "pattern.twoColor",
        &json!({ "name": "Coral Checks", "kind": "checkerboard", "foreground": "#c02020", "background": "#204080", "tileSize": 12.0 }),
    )
    .unwrap();
    // N1: invalid definitions and edits are refused without document change.
    let listed = |s: &mut Session| s.execute("pattern.list", &json!({})).unwrap()["patterns"].as_array().unwrap().clone();
    let before = listed(&mut s);
    for bad in [
        json!({ "name": "Bad Kind", "kind": "marble" }),
        json!({ "name": "Bad Colour", "kind": "polkaDot", "foreground": "nope" }),
        json!({ "name": "Bad Tile", "kind": "polkaDot", "tileSize": 0.0 }),
        json!({ "name": "Bad Tile 2", "kind": "polkaDot", "tileSize": -3.0 }),
        json!({ "name": "", "kind": "polkaDot" }),
    ] {
        assert!(s.execute("pattern.twoColor", &bad).is_err(), "invalid definition accepted: {bad}");
    }
    for bad in [
        json!({ "name": "Coral Checks", "kind": "marble" }),
        json!({ "name": "Coral Checks", "tileSize": 0.0 }),
        json!({ "name": "Coral Checks", "background": "nope" }),
        json!({ "name": "No Such Pattern", "kind": "polkaDot" }),
    ] {
        assert!(s.execute("pattern.setTwoColorProps", &bad).is_err(), "invalid edit accepted: {bad}");
    }
    assert_eq!(listed(&mut s), before, "the refused calls changed nothing");
    // N2: an art pattern has no two-colour source to edit.
    s.execute("object.pattern.make", &json!({ "ids": [a], "name": "Plain Pattern", "edit": false })).unwrap();
    let err =
        s.execute("pattern.setTwoColorProps", &json!({ "name": "Plain Pattern", "foreground": "#101010" })).expect_err("an art pattern is refused");
    assert!(format!("{err:?}").contains("art pattern"), "the error names it an art pattern: {err:?}");
    // N3: unknown and texture names are refused for the paint.
    s.execute("texture.new", &json!({ "name": "Coral Stipple", "kind": "stipple" })).unwrap();
    s.execute("select.set", &json!({ "ids": [a] })).unwrap();
    let before = s.execute("document.node", &json!({ "id": a, "summary": true })).unwrap();
    for bad in ["No Such Pattern", "Coral Stipple"] {
        assert!(s.execute("paint.setPattern", &json!({ "pattern": bad })).is_err(), "accepted {bad:?}");
    }
    let after = s.execute("document.node", &json!({ "id": a, "summary": true })).unwrap();
    assert_eq!(before["fill"], after["fill"], "the failed paints changed nothing");
    // N4: the two-colour pattern is not a texture, and the texture command refuses its name.
    let textures = s.execute("texture.list", &json!({})).unwrap();
    assert!(
        textures["textures"].as_array().is_none_or(|t| t.iter().all(|d| d["name"].as_str() != Some("Coral Checks"))),
        "the pattern does not appear among textures: {textures:?}"
    );
    assert!(s.execute("paint.setTexture", &json!({ "texture": "Coral Checks" })).is_err(), "a pattern name is not a texture");
}

fn pattern_paints(group: &usvg::Group, out: &mut Vec<std::sync::Arc<usvg::Pattern>>) {
    for node in group.children() {
        match node {
            usvg::Node::Group(child) => pattern_paints(child, out),
            usvg::Node::Path(path) if !path.data().is_empty() => {
                if let Some(fill) = path.fill()
                    && let usvg::Paint::Pattern(pattern) = fill.paint()
                    && !out.iter().any(|p| p.id() == pattern.id())
                {
                    out.push(pattern.clone());
                }
            }
            _ => {}
        }
    }
}

fn has_solid_vector(group: &usvg::Group, rgb: [u8; 3]) -> bool {
    group.children().iter().any(|node| match node {
        usvg::Node::Group(child) => has_solid_vector(child, rgb),
        usvg::Node::Path(path) if !path.data().is_empty() => {
            path.fill().is_some_and(|fill| matches!(fill.paint(), usvg::Paint::Color(color) if [color.red, color.green, color.blue] == rgb))
        }
        _ => false,
    })
}

/// The non-empty paths of `group` with whether each holds any curve.
fn pattern_paths(group: &usvg::Group) -> Vec<(usize, bool)> {
    let mut out = vec![];
    for node in group.children() {
        match node {
            usvg::Node::Group(child) => out.extend(pattern_paths(child)),
            usvg::Node::Path(path) if !path.data().is_empty() => {
                let curved = path
                    .data()
                    .segments()
                    .any(|seg| matches!(seg, usvg::tiny_skia_path::PathSegment::CubicTo(..) | usvg::tiny_skia_path::PathSegment::QuadTo(..)));
                out.push((path.data().segments().count(), curved));
            }
            _ => {}
        }
    }
    out
}

fn art_has_colours(art: &[std::sync::Arc<vectorcraft_doc::Node>], expected: &[&str]) -> bool {
    expected.iter().all(|expected_color| {
        art.iter().any(|node| {
            let mut found = false;
            node.walk(&mut |child| {
                if child.path_data().is_some_and(|path| !path.is_empty())
                    && child.appearance.fill_paint().color().is_some_and(|color| color.to_hex().as_str() == *expected_color)
                {
                    found = true;
                }
            });
            found
        })
    })
}

fn imported_pdf(s: &mut Session) -> vectorcraft_doc::Document {
    let bytes =
        vectorcraft_format::base64_decode(s.execute("document.serialize", &json!({ "format": "pdf" })).unwrap()["dataBase64"].as_str().unwrap())
            .unwrap();
    vectorcraft_pdf::import(&bytes).expect("independent Hayro-backed PDF read")
}
