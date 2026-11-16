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

/// P1–P5: create, apply, reuse, edit propagation and undo.
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

    // P4: the exported tiles carry the definition's colours.
    let svg_before = s.execute("document.serialize", &json!({ "format": "svg" })).unwrap()["text"].as_str().unwrap().to_string();
    assert!(svg_before.contains("#c02020") && svg_before.contains("#204080"), "both texture colours rendered: {svg_before}");

    // P4: editing the definition reaches every user's rendered tiles.
    s.execute("texture.setProps", &json!({ "name": "Coral Stipple", "foreground": "#20a040" })).unwrap();
    let svg_after = s.execute("document.serialize", &json!({ "format": "svg" })).unwrap()["text"].as_str().unwrap().to_string();
    assert!(!svg_after.contains("#c02020") && svg_after.contains("#20a040"), "the edited colour replaced the old one everywhere: {svg_after}");

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
    // The definition still edits after reopen; the export propagation regression is below.
    s.execute("texture.setProps", &json!({ "name": "Coral Stipple", "foreground": "#20a040" })).unwrap();
    let list = s.execute("texture.list", &json!({})).unwrap();
    assert_eq!(list["textures"][0]["foreground"].as_str(), Some("#20a040"), "the post-reopen edit took");
}

/// P7/P8: independent SVG and PDF evidence for the rendered tiles.
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
    // Independent usvg read: the connected pattern contains actual vector tiles in BOTH colours.
    let svg = exported_svg(&mut s);
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).expect("independent usvg parse");
    let mut patterns = vec![];
    pattern_paints(tree.root(), &mut patterns);
    let [pattern] = patterns.as_slice() else { panic!("one actual texture pattern: {svg}") };
    let tile = pattern.rect();
    assert_eq!((tile.x(), tile.y(), tile.width(), tile.height()), (0.0, 0.0, 24.0, 24.0), "the actual texture period");
    for rgb in [[192, 32, 32], [32, 64, 128]] {
        assert!(has_solid_vector(pattern.root(), rgb), "the pattern contains non-empty {rgb:?} vector tiles: {svg}");
    }
    // Independent Hayro-backed PDF read: the reconstructed definition and its target stay connected.
    let imported = imported_pdf(&mut s);
    let definition = imported
        .patterns
        .iter()
        .find(|definition| art_has_colours(&definition.art, &["#c02020", "#204080"]))
        .expect("Hayro reconstructs the two-colour texture definition");
    assert!((definition.tile.width() - 24.0).abs() < 1e-4 && (definition.tile.height() - 24.0).abs() < 1e-4, "{:?}", definition.tile);
    let mut connected = false;
    imported.walk(|node| {
        connected |= matches!(node.appearance.fill_paint(), vectorcraft_color::Paint::Pattern { pattern, .. } if *pattern == definition.name);
    });
    assert!(connected, "the reconstructed target references the reconstructed tile art");
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

/// P4: an edit of the definition reaches every user's rendered tiles.
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

fn pattern_paints(group: &usvg::Group, out: &mut Vec<std::sync::Arc<usvg::Pattern>>) {
    for node in group.children() {
        match node {
            usvg::Node::Group(child) => pattern_paints(child, out),
            usvg::Node::Path(path) if !path.data().is_empty() => {
                if let Some(fill) = path.fill()
                    && let usvg::Paint::Pattern(pattern) = fill.paint()
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

fn create_texture(s: &mut Session, density: f64) {
    s.execute(
        "texture.new",
        &json!({ "name": "Coral Stipple", "kind": "stipple", "density": density, "foreground": "#c02020", "background": "#204080" }),
    )
    .unwrap();
}

fn apply_texture(s: &mut Session, id: u64) {
    s.execute("select.set", &json!({ "ids": [id] })).unwrap();
    s.execute("paint.setStroke", &json!({ "none": true })).unwrap();
    s.execute("paint.setTexture", &json!({ "texture": "Coral Stipple" })).unwrap();
}

fn exported_svg(s: &mut Session) -> String {
    s.execute("document.serialize", &json!({ "format": "svg" })).unwrap()["text"].as_str().unwrap().to_string()
}

fn imported_pdf(s: &mut Session) -> vectorcraft_doc::Document {
    let bytes =
        vectorcraft_format::base64_decode(s.execute("document.serialize", &json!({ "format": "pdf" })).unwrap()["dataBase64"].as_str().unwrap())
            .unwrap();
    vectorcraft_pdf::import(&bytes).expect("independent Hayro-backed PDF read")
}

#[test]
fn texture_new_returns_the_definition_name() {
    let mut s = session();
    let created = s
        .execute(
            "texture.new",
            &json!({ "name": "Coral Stipple", "kind": "stipple", "density": 0.4, "foreground": "#c02020", "background": "#204080" }),
        )
        .unwrap();
    assert_eq!(created["name"].as_str(), Some("Coral Stipple"));
}

#[test]
fn texture_rasterizes_with_the_objects_opacity() {
    let mut s = session();
    let a = s.execute("shape.rectangle", &json!({ "x": 20, "y": 20, "width": 120, "height": 80 })).unwrap()["id"].as_u64().unwrap();
    create_texture(&mut s, 1.0);
    s.execute("select.set", &json!({ "ids": [a] })).unwrap();
    s.execute("transparency.set", &json!({ "opacity": 50 })).unwrap();
    apply_texture(&mut s, a);
    let image = vectorcraft_render::Renderer::new().render(
        &s.doc().unwrap().doc,
        400,
        300,
        vectorcraft_geom::Affine::IDENTITY,
        &vectorcraft_render::RenderOptions::default(),
    );
    let pixel = image.pixel(50, 50);
    assert!((120..=136).contains(&pixel[3]), "the texture respects 50% object opacity: {pixel:?}");
    assert!(pixel[0] > 180 && pixel[1] < 50 && pixel[2] < 50, "the foreground tile actually rasterizes: {pixel:?}");
}

#[test]
fn texture_edit_reaches_both_users_in_independent_exports() {
    let mut s = session();
    let a = s.execute("shape.rectangle", &json!({ "x": 20, "y": 20, "width": 90, "height": 60 })).unwrap()["id"].as_u64().unwrap();
    let b = s.execute("shape.rectangle", &json!({ "x": 200, "y": 20, "width": 90, "height": 60 })).unwrap()["id"].as_u64().unwrap();
    create_texture(&mut s, 0.4);
    for id in [a, b] {
        apply_texture(&mut s, id);
    }
    s.execute("texture.setProps", &json!({ "name": "Coral Stipple", "foreground": "#20a040" })).unwrap();

    let svg = exported_svg(&mut s);
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).expect("independent usvg parse");
    let mut patterns = vec![];
    pattern_paints(tree.root(), &mut patterns);
    assert_eq!(patterns.len(), 2, "both users reference the rendered texture: {svg}");
    assert!(std::sync::Arc::ptr_eq(&patterns[0], &patterns[1]), "both users share one definition");
    assert!(has_solid_vector(patterns[0].root(), [32, 160, 64]) && has_solid_vector(patterns[0].root(), [32, 64, 128]), "{svg}");
    assert!(!has_solid_vector(patterns[0].root(), [192, 32, 32]), "the old colour is gone: {svg}");

    let imported = imported_pdf(&mut s);
    let definition = imported
        .patterns
        .iter()
        .find(|definition| art_has_colours(&definition.art, &["#20a040", "#204080"]))
        .expect("Hayro reconstructs the edited two-colour texture definition");
    let mut users = 0;
    imported.walk(|node| {
        users +=
            usize::from(matches!(node.appearance.fill_paint(), vectorcraft_color::Paint::Pattern { pattern, .. } if *pattern == definition.name));
    });
    assert_eq!(users, 2, "both reconstructed targets reference the texture definition");
}

#[test]
fn texture_reopen_edit_reaches_the_independent_export() {
    let mut s = session();
    let a = s.execute("shape.rectangle", &json!({ "x": 20, "y": 20, "width": 90, "height": 60 })).unwrap()["id"].as_u64().unwrap();
    create_texture(&mut s, 0.4);
    apply_texture(&mut s, a);
    let data = s.execute("document.serialize", &json!({ "format": "vectorcraft" })).unwrap()["dataBase64"].as_str().unwrap().to_string();
    s.execute("document.open", &json!({ "dataBase64": data })).unwrap();
    s.execute("texture.setProps", &json!({ "name": "Coral Stipple", "foreground": "#20a040" })).unwrap();

    let svg = exported_svg(&mut s);
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).expect("independent usvg parse after native reopen");
    let mut patterns = vec![];
    pattern_paints(tree.root(), &mut patterns);
    let [pattern] = patterns.as_slice() else { panic!("one reopened texture user: {svg}") };
    assert!(has_solid_vector(pattern.root(), [32, 160, 64]) && !has_solid_vector(pattern.root(), [192, 32, 32]), "{svg}");
}

#[test]
fn texture_compound_preserves_even_odd_and_exports_tiles() {
    let mut s = Session::new();
    s.execute("file.new", &json!({ "width": 100, "height": 100 })).unwrap();
    let outer = s.execute("shape.rectangle", &json!({ "x": 10, "y": 10, "width": 80, "height": 80 })).unwrap()["id"].as_u64().unwrap();
    let inner = s.execute("shape.rectangle", &json!({ "x": 30, "y": 30, "width": 40, "height": 40 })).unwrap()["id"].as_u64().unwrap();
    s.execute("select.set", &json!({ "ids": [inner, outer] })).unwrap();
    let compound = s.execute("object.compoundPath.make", &json!({})).unwrap()["id"].as_u64().unwrap();
    s.execute("select.set", &json!({ "ids": [inner] })).unwrap();
    s.execute("path.reverse", &json!({})).unwrap();
    s.execute("select.set", &json!({ "ids": [compound] })).unwrap();
    s.execute("path.setFillRule", &json!({ "rule": "evenOdd" })).unwrap();
    create_texture(&mut s, 1.0);
    apply_texture(&mut s, compound);

    let svg = exported_svg(&mut s);
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).expect("independent usvg compound parse");
    let mut patterns = vec![];
    pattern_paints(tree.root(), &mut patterns);
    let [pattern] = patterns.as_slice() else { panic!("the compound has a texture pattern: {svg}") };
    assert!(has_solid_vector(pattern.root(), [192, 32, 32]), "{svg}");
    fn has_even_odd_pattern(group: &usvg::Group) -> bool {
        group.children().iter().any(|node| match node {
            usvg::Node::Group(child) => has_even_odd_pattern(child),
            usvg::Node::Path(path) => {
                path.fill().is_some_and(|fill| fill.rule() == usvg::FillRule::EvenOdd && matches!(fill.paint(), usvg::Paint::Pattern(_)))
            }
            _ => false,
        })
    }
    assert!(has_even_odd_pattern(tree.root()), "the SVG compound stays even-odd: {svg}");

    let imported = imported_pdf(&mut s);
    let definition = imported
        .patterns
        .iter()
        .find(|definition| art_has_colours(&definition.art, &["#c02020"]))
        .expect("Hayro reconstructs the compound texture definition");
    let mut even_odd = false;
    imported.walk(|node| {
        let rule = match &node.kind {
            vectorcraft_doc::NodeKind::Path { rule, .. } | vectorcraft_doc::NodeKind::Compound { rule, .. } => Some(*rule),
            _ => None,
        };
        even_odd |= rule == Some(vectorcraft_geom::FillRule::EvenOdd)
            && matches!(node.appearance.fill_paint(), vectorcraft_color::Paint::Pattern { pattern, .. } if *pattern == definition.name);
    });
    assert!(even_odd, "the reconstructed PDF compound stays even-odd");
}

#[test]
fn texture_svg_pattern_keeps_the_targets_opacity() {
    let mut s = session();
    let a = s.execute("shape.rectangle", &json!({ "x": 20, "y": 20, "width": 120, "height": 80 })).unwrap()["id"].as_u64().unwrap();
    create_texture(&mut s, 1.0);
    s.execute("select.set", &json!({ "ids": [a] })).unwrap();
    s.execute("transparency.set", &json!({ "opacity": 50 })).unwrap();
    apply_texture(&mut s, a);
    let svg = exported_svg(&mut s);
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).expect("independent usvg opacity parse");
    fn opacities(group: &usvg::Group, inherited: f32, out: &mut Vec<f32>) {
        let inherited = inherited * group.opacity().get();
        for node in group.children() {
            match node {
                usvg::Node::Group(child) => opacities(child, inherited, out),
                usvg::Node::Path(path) => {
                    if let Some(fill) = path.fill()
                        && matches!(fill.paint(), usvg::Paint::Pattern(_))
                    {
                        out.push(inherited * fill.opacity().get());
                    }
                }
                _ => {}
            }
        }
    }
    let mut found = vec![];
    opacities(tree.root(), 1.0, &mut found);
    assert_eq!(found, vec![0.5], "the whole textured target is half opaque: {svg}");
}

#[test]
fn texture_and_pattern_cache_keys_do_not_collide() {
    let mut s = session();
    s.execute("artboard.setProps", &json!({ "index": 0, "x": 100, "y": 100 })).unwrap();
    let tile = s.execute("shape.rectangle", &json!({ "x": 320, "y": 20, "width": 20, "height": 20 })).unwrap()["id"].as_u64().unwrap();
    s.execute("paint.setFill", &json!({ "id": tile, "color": "#336699" })).unwrap();
    s.execute("paint.setStroke", &json!({ "id": tile, "none": true })).unwrap();
    s.execute("object.pattern.make", &json!({ "id": tile, "name": "texture|Coral Stipple", "edit": false })).unwrap();
    let pattern_target = s.execute("shape.rectangle", &json!({ "x": 200, "y": 20, "width": 90, "height": 60 })).unwrap()["id"].as_u64().unwrap();
    s.execute("paint.setFill", &json!({ "id": pattern_target, "swatch": "texture|Coral Stipple" })).unwrap();
    s.execute("paint.setStroke", &json!({ "id": pattern_target, "none": true })).unwrap();
    let texture_target = s.execute("shape.rectangle", &json!({ "x": 20, "y": 20, "width": 90, "height": 60 })).unwrap()["id"].as_u64().unwrap();
    create_texture(&mut s, 0.4);
    apply_texture(&mut s, texture_target);

    let svg = exported_svg(&mut s);
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).expect("independent usvg collision parse");
    fn positioned_patterns(group: &usvg::Group, out: &mut Vec<(f32, std::sync::Arc<usvg::Pattern>)>) {
        for node in group.children() {
            match node {
                usvg::Node::Group(child) => positioned_patterns(child, out),
                usvg::Node::Path(path) => {
                    if let Some(fill) = path.fill()
                        && let usvg::Paint::Pattern(pattern) = fill.paint()
                    {
                        out.push((path.abs_bounding_box().left(), pattern.clone()));
                    }
                }
                _ => {}
            }
        }
    }
    let mut paints = vec![];
    positioned_patterns(tree.root(), &mut paints);
    paints.sort_by(|a, b| a.0.total_cmp(&b.0));
    let [(texture_x, texture), (pattern_x, pattern)] = paints.as_slice() else { panic!("two connected paint definitions: {svg}") };
    assert_eq!((*texture_x, *pattern_x), (-80.0, 100.0), "the targets retain their own positions: {svg}");
    assert!(!std::sync::Arc::ptr_eq(texture, pattern), "texture and pattern targets must use distinct definitions: {svg}");
    assert!(
        has_solid_vector(texture.root(), [192, 32, 32]) && has_solid_vector(texture.root(), [32, 64, 128]),
        "the texture target keeps its own tile colours: {svg}"
    );
    assert!(!has_solid_vector(texture.root(), [51, 102, 153]), "the pattern art does not replace the texture: {svg}");
    assert!(has_solid_vector(pattern.root(), [51, 102, 153]), "the pattern target keeps its pattern art: {svg}");
}
