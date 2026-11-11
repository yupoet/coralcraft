//! CoralCraft: editable mesh fills and transparency over the shared object model (P3 Task 3).

use serde_json::json;
use vectorcraft_color::{BlendMode, Paint};
use vectorcraft_doc::{NodeId, NodeKind};
use vectorcraft_engine::Session;

fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 240, "height": 160})).unwrap();
    s
}

fn rectangle(s: &mut Session) -> u64 {
    s.execute("shape.rectangle", &json!({"x": 20, "y": 20, "width": 120, "height": 90})).unwrap()["id"].as_u64().unwrap()
}

fn native_reopen(s: &mut Session) {
    let data = s.execute("document.serialize", &json!({"format": "vectorcraft"})).unwrap()["dataBase64"].as_str().unwrap().to_string();
    s.execute("document.open", &json!({"dataBase64": data})).unwrap();
}

fn independent_export_reads(s: &mut Session) {
    let svg = s.execute("document.serialize", &json!({"format": "svg"})).unwrap()["text"].as_str().unwrap().to_string();
    usvg::Tree::from_str(&svg, &usvg::Options::default()).expect("independent usvg parse of the fill/transparency output");
    let pdf = vectorcraft_format::base64_decode(s.execute("document.serialize", &json!({"format": "pdf"})).unwrap()["dataBase64"].as_str().unwrap())
        .unwrap();
    let imported = vectorcraft_pdf::import(&pdf).expect("independent Hayro-backed fill/transparency PDF read");
    assert_eq!(imported.artboards.len(), 1);
}

#[test]
fn mesh_fill_points_survive_native_reopen_and_remain_editable() {
    let mut s = session();
    let shape = rectangle(&mut s);
    s.execute("object.mesh.create", &json!({"id": shape, "rows": 1, "cols": 1})).unwrap();
    s.execute("object.mesh.setPointColor", &json!({"id": shape, "index": 3, "color": "#00ff00", "opacity": 0.5})).unwrap();
    let NodeKind::Mesh(mesh) = &s.doc().unwrap().doc.node(NodeId(shape)).unwrap().kind else { panic!("mesh fill") };
    assert_eq!(mesh.points[3].color.to_hex(), "#00ff00");
    assert_eq!(mesh.points[3].opacity, 0.5);
    s.execute("edit.undo", &json!({})).unwrap();
    let NodeKind::Mesh(mesh) = &s.doc().unwrap().doc.node(NodeId(shape)).unwrap().kind else { panic!("undo keeps the mesh") };
    assert_eq!(mesh.points[3].color.to_hex(), "#ffffff");
    s.execute("edit.redo", &json!({})).unwrap();

    native_reopen(&mut s);
    let NodeKind::Mesh(mesh) = &s.doc().unwrap().doc.node(NodeId(shape)).unwrap().kind else { panic!("native reopen keeps the mesh") };
    assert_eq!(mesh.points[3].color.to_hex(), "#00ff00");
    s.execute("object.mesh.setPointColor", &json!({"id": shape, "index": 0, "color": "#ff0000"})).unwrap();
    s.execute("object.mesh.movePoint", &json!({"id": shape, "index": 1, "x": 90.0, "y": 30.0})).unwrap();
    let NodeKind::Mesh(mesh) = &s.doc().unwrap().doc.node(NodeId(shape)).unwrap().kind else { panic!("edited mesh") };
    assert_eq!(mesh.points[0].color.to_hex(), "#ff0000");
    assert_eq!((mesh.points[1].p.x, mesh.points[1].p.y), (90.0, 30.0));
    independent_export_reads(&mut s);
}

#[test]
fn object_transparency_and_blend_mode_survive_reopen_and_undo() {
    let mut s = session();
    let shape = rectangle(&mut s);
    s.execute("transparency.set", &json!({"id": shape, "opacity": 50, "blend": "multiply"})).unwrap();
    let node = s.doc().unwrap().doc.node(NodeId(shape)).unwrap();
    assert_eq!((node.opacity, node.blend), (0.5, BlendMode::Multiply));
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(s.doc().unwrap().doc.node(NodeId(shape)).unwrap().opacity, 1.0);
    s.execute("edit.redo", &json!({})).unwrap();

    native_reopen(&mut s);
    let node = s.doc().unwrap().doc.node(NodeId(shape)).unwrap();
    assert_eq!((node.opacity, node.blend), (0.5, BlendMode::Multiply));
    s.execute("transparency.set", &json!({"id": shape, "opacity": 65})).unwrap();
    assert_eq!(s.doc().unwrap().doc.node(NodeId(shape)).unwrap().opacity, 0.65);
    independent_export_reads(&mut s);
}

#[test]
fn fountain_fill_stops_survive_native_reopen_and_editing() {
    let mut s = session();
    let shape = rectangle(&mut s);
    let stops = json!([{"offset": 0, "color": "#ffffff"}, {"offset": 1, "color": "#333333"}]);
    s.execute("paint.setFill", &json!({"id": shape, "gradient": {"kind": "linear", "stops": stops}})).unwrap();
    native_reopen(&mut s);
    let vectorcraft_color::Paint::Gradient(gradient) = &s.doc().unwrap().doc.node(NodeId(shape)).unwrap().appearance.fill_paint() else {
        panic!("fountain fill")
    };
    assert_eq!(gradient.gradient.stops[1].color.to_hex(), "#333333");
    s.execute("paint.editGradient", &json!({"id": shape, "stops": [{"offset": 0, "color": "#ff0000"}, {"offset": 1, "color": "#0000ff"}]})).unwrap();
    let vectorcraft_color::Paint::Gradient(gradient) = &s.doc().unwrap().doc.node(NodeId(shape)).unwrap().appearance.fill_paint() else {
        panic!("edited fountain fill")
    };
    assert_eq!(gradient.gradient.stops[0].color.to_hex(), "#ff0000");
    assert_eq!(gradient.gradient.stops[1].color.to_hex(), "#0000ff");
    independent_export_reads(&mut s);
}

#[test]
fn vector_pattern_fill_keeps_its_definition_and_editable_target() {
    let mut s = session();
    let tile = rectangle(&mut s);
    s.execute("paint.setFill", &json!({"id": tile, "color": "#808080"})).unwrap();
    s.execute("object.pattern.make", &json!({"id": tile, "name": "Grey Tile", "edit": false})).unwrap();
    let target = rectangle(&mut s);
    s.execute("paint.setFill", &json!({"id": target, "swatch": "Grey Tile"})).unwrap();
    assert!(matches!(s.doc().unwrap().doc.node(NodeId(target)).unwrap().appearance.fill_paint(), vectorcraft_color::Paint::Pattern { .. }));

    native_reopen(&mut s);
    assert!(s.doc().unwrap().doc.pattern("Grey Tile").is_some());
    assert!(matches!(s.doc().unwrap().doc.node(NodeId(target)).unwrap().appearance.fill_paint(), vectorcraft_color::Paint::Pattern { .. }));
    s.execute("paint.setFill", &json!({"id": target, "color": "#204080"})).unwrap();
    assert_eq!(s.doc().unwrap().doc.node(NodeId(target)).unwrap().appearance.fill_paint().color().unwrap().to_hex(), "#204080");
    independent_export_reads(&mut s);
}

#[test]
fn graded_transparency_stops_edit_and_export_as_real_gradient_alpha() {
    let mut s = session();
    let shape = rectangle(&mut s);
    let stops = |opacity: f64| {
        json!([
            {"offset": 0, "color": "#ff0000", "opacity": 1},
            {"offset": 1, "color": "#0000ff", "opacity": opacity}
        ])
    };
    s.execute("paint.setFill", &json!({"id": shape, "gradient": {"kind": "linear", "stops": stops(0.25)}})).unwrap();
    s.execute("paint.editGradient", &json!({"id": shape, "stops": stops(0.6)})).unwrap();
    let opacity = |s: &Session| {
        let vectorcraft_color::Paint::Gradient(gradient) = &s.doc().unwrap().doc.node(NodeId(shape)).unwrap().appearance.fill_paint() else {
            panic!("editable vector gradient fill")
        };
        gradient.gradient.stops[1].opacity
    };
    assert_eq!(opacity(&s), 0.6);
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(opacity(&s), 0.25);
    s.execute("edit.redo", &json!({})).unwrap();
    assert_eq!(opacity(&s), 0.6);

    native_reopen(&mut s);
    assert!(matches!(s.doc().unwrap().doc.node(NodeId(shape)).unwrap().kind, NodeKind::Path { .. }));
    assert_eq!(opacity(&s), 0.6);
    s.execute("paint.editGradient", &json!({"id": shape, "stops": stops(0.4)})).unwrap();
    assert_eq!(opacity(&s), 0.4);

    let svg = s.execute("document.serialize", &json!({"format": "svg"})).unwrap()["text"].as_str().unwrap().to_string();
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).expect("independent usvg parse of graded transparency");
    fn gradients(group: &usvg::Group, out: &mut Vec<Vec<(f32, f32, [u8; 3])>>) {
        for node in group.children() {
            match node {
                usvg::Node::Group(child) => gradients(child, out),
                usvg::Node::Path(path) => {
                    let Some(fill) = path.fill() else { continue };
                    let usvg::Paint::LinearGradient(gradient) = fill.paint() else { continue };
                    out.push(
                        gradient
                            .stops()
                            .iter()
                            .map(|stop| {
                                let color = stop.color();
                                (stop.offset().get(), stop.opacity().get(), [color.red, color.green, color.blue])
                            })
                            .collect(),
                    );
                }
                _ => {}
            }
        }
    }
    let mut exported = vec![];
    gradients(tree.root(), &mut exported);
    let [stops] = exported.as_slice() else { panic!("one actual linear gradient in the independent SVG tree: {svg}") };
    assert_eq!(stops.len(), 2, "{stops:?}");
    assert!((stops[0].0 - 0.0).abs() < 1e-5 && (stops[1].0 - 1.0).abs() < 1e-5, "{stops:?}");
    assert!((stops[0].1 - 1.0).abs() < 1e-5 && (stops[1].1 - 0.4).abs() < 1e-5, "actual stop alpha: {stops:?}");
    assert_eq!((stops[0].2, stops[1].2), ([255, 0, 0], [0, 0, 255]), "actual stop colours: {stops:?}");

    let pdf = vectorcraft_format::base64_decode(s.execute("document.serialize", &json!({"format": "pdf"})).unwrap()["dataBase64"].as_str().unwrap())
        .unwrap();
    let imported = vectorcraft_pdf::import(&pdf).expect("independent Hayro-backed graded-transparency PDF read");
    assert_eq!(imported.artboards.len(), 1);
    let mut vector_paths = 0;
    imported.walk(|node| vector_paths += usize::from(node.path_data().is_some()));
    assert!(vector_paths > 0, "Hayro reconstructs one page with vector geometry; this is not live PDF gradient-editability evidence");
}

#[test]
fn opacity_mask_art_edits_exports_and_releases_without_flattening_sources() {
    let mut s = session();
    let target = rectangle(&mut s);
    s.execute("paint.setFill", &json!({"id": target, "color": "#ff0000"})).unwrap();
    let mask = s.execute("shape.rectangle", &json!({"x": 20, "y": 20, "width": 60, "height": 90})).unwrap()["id"].as_u64().unwrap();
    s.execute("paint.setFill", &json!({"id": mask, "color": "#ffffff"})).unwrap();
    s.execute("select.set", &json!({"ids": [target, mask]})).unwrap();
    let made = s.execute("transparency.makeOpacityMask", &json!({"clip": true, "invert": false})).unwrap();
    assert_eq!(made["id"].as_u64(), Some(target));
    assert!(s.doc().unwrap().doc.node(NodeId(mask)).is_none(), "the top vector object moved out of the layer tree");
    let model = s.doc().unwrap().doc.node(NodeId(target)).unwrap().extras().mask.as_deref().unwrap();
    assert_eq!(model.art.id, NodeId(mask));
    assert!(model.art.path_data().is_some(), "real vector mask art lives in the mask model");

    s.execute("edit.undo", &json!({})).unwrap();
    assert!(s.doc().unwrap().doc.node(NodeId(target)).unwrap().extras().mask.is_none());
    assert!(s.doc().unwrap().doc.node(NodeId(mask)).is_some(), "undo restores the top object");
    s.execute("edit.redo", &json!({})).unwrap();
    assert!(s.doc().unwrap().doc.node(NodeId(mask)).is_none());
    s.execute("transparency.setOpacityMask", &json!({"id": target, "clip": false, "invert": true})).unwrap();
    s.execute("transparency.disableOpacityMask", &json!({"id": target})).unwrap();
    assert!(s.doc().unwrap().doc.node(NodeId(target)).unwrap().extras().mask.as_deref().unwrap().disabled);
    s.execute("transparency.enableOpacityMask", &json!({"id": target})).unwrap();
    let model = s.doc().unwrap().doc.node(NodeId(target)).unwrap().extras().mask.as_deref().unwrap();
    assert!((model.clip, model.invert, model.disabled, model.linked) == (false, true, false, true));

    s.execute("transparency.editOpacityMask", &json!({"id": target})).unwrap();
    let editing = s.doc().unwrap().selection.objects[0];
    s.execute("paint.setFill", &json!({"id": editing.0, "color": "#808080"})).unwrap();
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(s.doc().unwrap().doc.node(editing).unwrap().appearance.fill_paint().color().unwrap().to_hex(), "#ffffff");
    s.execute("edit.redo", &json!({})).unwrap();
    assert_eq!(s.doc().unwrap().doc.node(editing).unwrap().appearance.fill_paint().color().unwrap().to_hex(), "#808080");
    s.execute("transparency.stopEditingOpacityMask", &json!({})).unwrap();

    native_reopen(&mut s);
    let model = s.doc().unwrap().doc.node(NodeId(target)).unwrap().extras().mask.as_deref().unwrap();
    assert!((model.clip, model.invert, model.disabled) == (false, true, false));
    assert!(model.art.path_data().is_some(), "native reopen retains vector mask art");
    s.execute("transparency.editOpacityMask", &json!({"id": target})).unwrap();
    let editing = s.doc().unwrap().selection.objects[0];
    s.execute("paint.setFill", &json!({"id": editing.0, "color": "#404040"})).unwrap();
    s.execute("transparency.stopEditingOpacityMask", &json!({})).unwrap();
    assert_eq!(
        s.doc().unwrap().doc.node(NodeId(target)).unwrap().extras().mask.as_deref().unwrap().art.appearance.fill_paint().color().unwrap().to_hex(),
        "#404040",
        "mask art remains editable after native reopen"
    );
    s.execute("select.set", &json!({"ids": [target]})).unwrap();
    s.execute("object.move", &json!({"dx": 4, "dy": 0})).unwrap();
    let NodeKind::Path { path, .. } = &s.doc().unwrap().doc.node(NodeId(target)).unwrap().kind else { panic!("target stays a vector path") };
    assert_eq!(path.bounds().unwrap().x0, 24.0);
    s.execute("edit.undo", &json!({})).unwrap();
    s.execute("edit.redo", &json!({})).unwrap();

    let svg = s.execute("document.serialize", &json!({"format": "svg"})).unwrap()["text"].as_str().unwrap().to_string();
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).expect("independent usvg parse of the opacity mask");
    fn has_solid_path(group: &usvg::Group, rgb: [u8; 3]) -> bool {
        group.children().iter().any(|node| match node {
            usvg::Node::Group(child) => has_solid_path(child, rgb),
            usvg::Node::Path(path) if !path.data().is_empty() => {
                path.fill().is_some_and(|fill| matches!(fill.paint(), usvg::Paint::Color(color) if [color.red, color.green, color.blue] == rgb))
            }
            _ => false,
        })
    }
    fn connected_mask(group: &usvg::Group) -> bool {
        let connected = group.mask().is_some_and(|mask| {
            mask.kind() == usvg::MaskType::Luminance && has_solid_path(mask.root(), [64, 64, 64]) && has_solid_path(group, [255, 0, 0])
        });
        connected
            || group.children().iter().any(|node| match node {
                usvg::Node::Group(child) => connected_mask(child),
                _ => false,
            })
    }
    assert!(connected_mask(tree.root()), "a luminance mask containing the edited vector art is connected to the red target: {svg}");

    let pdf = vectorcraft_format::base64_decode(s.execute("document.serialize", &json!({"format": "pdf"})).unwrap()["dataBase64"].as_str().unwrap())
        .unwrap();
    let imported = vectorcraft_pdf::import(&pdf).expect("independent Hayro-backed opacity-mask PDF read");
    assert_eq!(imported.artboards.len(), 1);
    let mut reconstructed_masks = 0;
    imported.walk(|node| {
        reconstructed_masks += usize::from(node.extras().mask.as_deref().is_some_and(|mask| mask.art.geometric_bounds().is_some()));
    });
    assert!(reconstructed_masks > 0, "Hayro reconstructs a page and vector soft-mask structure, not the original native editing relationships");

    s.execute("select.set", &json!({"ids": [target]})).unwrap();
    s.execute("transparency.releaseOpacityMask", &json!({})).unwrap();
    assert!(s.doc().unwrap().doc.node(NodeId(target)).unwrap().extras().mask.is_none());
    let art = &s.doc().unwrap().doc.layers[0];
    let restored_grey_art = art
        .children()
        .unwrap()
        .iter()
        .any(|node| node.path_data().is_some() && node.appearance.fill_paint().color().is_some_and(|color| color.to_hex() == "#404040"));
    assert!(restored_grey_art, "release restores the edited mask art as an ordinary editable path");
    s.execute("edit.undo", &json!({})).unwrap();
    assert!(s.doc().unwrap().doc.node(NodeId(target)).unwrap().extras().mask.is_some());
    s.execute("edit.redo", &json!({})).unwrap();
    assert!(s.doc().unwrap().doc.node(NodeId(target)).unwrap().extras().mask.is_none());
}

#[test]
fn two_colour_generic_pattern_art_survives_editing_reopen_and_independent_exports() {
    let mut s = session();
    let background = s.execute("shape.rectangle", &json!({"x": 0, "y": 0, "width": 20, "height": 20})).unwrap()["id"].as_u64().unwrap();
    let foreground = s.execute("shape.rectangle", &json!({"x": 0, "y": 0, "width": 10, "height": 20})).unwrap()["id"].as_u64().unwrap();
    for (id, color) in [(background, "#f4d35e"), (foreground, "#e21a2c")] {
        s.execute("paint.setFill", &json!({"id": id, "color": color})).unwrap();
        s.execute("paint.setStroke", &json!({"id": id, "none": true})).unwrap();
    }
    s.execute("select.set", &json!({"ids": [background, foreground]})).unwrap();
    s.execute("object.pattern.make", &json!({"name": "Two Colour Generic", "width": 20, "height": 20, "edit": false})).unwrap();
    let target = s.execute("shape.rectangle", &json!({"x": 100, "y": 40, "width": 90, "height": 70})).unwrap()["id"].as_u64().unwrap();
    s.execute("paint.setFill", &json!({"id": target, "swatch": "Two Colour Generic"})).unwrap();
    s.execute("select.set", &json!({"ids": [target]})).unwrap();
    s.execute("object.pattern.edit", &json!({})).unwrap();
    let editing = s.doc().unwrap().selection.objects.clone();
    assert_eq!(editing.len(), 2, "both ordinary vector objects are editable in the pattern layer");
    s.execute("paint.setFill", &json!({"id": editing[0].0, "color": "#123456"})).unwrap();
    s.execute("paint.setFill", &json!({"id": editing[1].0, "color": "#abcdef"})).unwrap();
    let editing_color =
        |s: &Session, index: usize| s.doc().unwrap().doc.node(editing[index]).unwrap().appearance.fill_paint().color().unwrap().to_hex();
    assert_eq!((editing_color(&s, 0), editing_color(&s, 1)), ("#123456".into(), "#abcdef".into()));
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!((editing_color(&s, 0), editing_color(&s, 1)), ("#123456".into(), "#e21a2c".into()));
    s.execute("edit.redo", &json!({})).unwrap();
    assert_eq!(editing_color(&s, 1), "#abcdef");
    s.execute("object.pattern.done", &json!({})).unwrap();

    let art_has_colours = |art: &[std::sync::Arc<vectorcraft_doc::Node>], expected: &[&str]| {
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
    };
    let target_uses = |s: &Session, name: &str| {
        let node = s.doc().unwrap().doc.node(NodeId(target)).unwrap();
        node.path_data().is_some_and(|path| !path.is_empty())
            && matches!(node.appearance.fill_paint(), Paint::Pattern { pattern, .. } if pattern == name)
    };
    let first_edit = ["#123456", "#abcdef"];
    assert!(art_has_colours(&s.doc().unwrap().doc.pattern("Two Colour Generic").unwrap().art, &first_edit));
    assert!(target_uses(&s, "Two Colour Generic"));

    native_reopen(&mut s);
    assert!(art_has_colours(&s.doc().unwrap().doc.pattern("Two Colour Generic").unwrap().art, &first_edit));
    assert!(target_uses(&s, "Two Colour Generic"));
    s.execute("object.pattern.edit", &json!({"name": "Two Colour Generic"})).unwrap();
    let reopened = s.doc().unwrap().selection.objects.clone();
    assert_eq!(reopened.len(), 2, "native reopen keeps two separately editable vector objects");
    s.execute("paint.setFill", &json!({"id": reopened[0].0, "color": "#0f1e2d"})).unwrap();
    s.execute("object.pattern.done", &json!({})).unwrap();
    let expected = ["#0f1e2d", "#abcdef"];
    native_reopen(&mut s);
    assert!(art_has_colours(&s.doc().unwrap().doc.pattern("Two Colour Generic").unwrap().art, &expected));
    assert!(target_uses(&s, "Two Colour Generic"));

    let svg = s.execute("document.serialize", &json!({"format": "svg"})).unwrap()["text"].as_str().unwrap().to_string();
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).expect("independent usvg parse of the two-colour generic pattern");
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
    let mut patterns = vec![];
    pattern_paints(tree.root(), &mut patterns);
    let [pattern] = patterns.as_slice() else { panic!("the target has one actual pattern paint in the independent SVG tree: {svg}") };
    let tile = pattern.rect();
    assert_eq!((tile.x(), tile.y(), tile.width(), tile.height()), (0.0, 0.0, 20.0, 20.0), "the target's actual tile rectangle");
    for rgb in [[15, 30, 45], [171, 205, 239]] {
        assert!(has_solid_vector(pattern.root(), rgb), "the connected tile recursively contains non-empty {rgb:?} vector art: {svg}");
    }

    let pdf = vectorcraft_format::base64_decode(s.execute("document.serialize", &json!({"format": "pdf"})).unwrap()["dataBase64"].as_str().unwrap())
        .unwrap();
    let imported = vectorcraft_pdf::import(&pdf).expect("independent Hayro-backed two-colour pattern PDF read");
    assert_eq!(imported.artboards.len(), 1);
    let definition = imported
        .patterns
        .iter()
        .find(|definition| art_has_colours(&definition.art, &expected))
        .expect("Hayro reconstructs a real pattern definition containing both edited vector colours");
    assert!((definition.tile.width() - 20.0).abs() < 1e-4 && (definition.tile.height() - 20.0).abs() < 1e-4, "{:?}", definition.tile);
    let mut connected = false;
    imported.walk(|node| {
        connected |= matches!(node.appearance.fill_paint(), Paint::Pattern { pattern, .. } if *pattern == definition.name);
    });
    assert!(connected, "the reconstructed target paint references the reconstructed definition; this is not live PDF editing evidence");
}

#[test]
fn bitmap_generic_pattern_art_replacement_survives_undo_reopen_and_independent_exports() {
    fn png(pixels: &image::RgbaImage) -> Vec<u8> {
        let mut out = std::io::Cursor::new(Vec::new());
        pixels.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }
    let original = image::RgbaImage::from_raw(2, 1, vec![255, 0, 0, 255, 0, 255, 0, 255]).unwrap();
    let replacement = image::RgbaImage::from_raw(
        3,
        2,
        vec![1, 2, 3, 255, 10, 20, 30, 255, 40, 50, 60, 255, 70, 80, 90, 255, 100, 110, 120, 255, 130, 140, 150, 255],
    )
    .unwrap();
    let mut s = session();
    let placed = s
        .execute(
            "file.place",
            &json!({"name": "original-tile.png", "dataBase64": vectorcraft_format::base64_encode(&png(&original)), "link": false, "at": [0, 0]}),
        )
        .unwrap();
    let bitmap = placed["ids"][0].as_u64().unwrap();
    s.execute("object.pattern.make", &json!({"ids": [bitmap], "name": "Bitmap Generic", "width": 12, "height": 8, "edit": false})).unwrap();
    let target = s.execute("shape.rectangle", &json!({"x": 100, "y": 40, "width": 90, "height": 70})).unwrap()["id"].as_u64().unwrap();
    s.execute("paint.setFill", &json!({"id": target, "swatch": "Bitmap Generic"})).unwrap();
    s.execute("object.pattern.edit", &json!({"name": "Bitmap Generic"})).unwrap();
    assert_eq!(s.doc().unwrap().selection.objects.len(), 1, "the embedded bitmap is editable in the pattern layer");
    let replaced = s
        .execute(
            "file.place",
            &json!({"name": "replacement-tile.png", "dataBase64": vectorcraft_format::base64_encode(&png(&replacement)), "link": false, "replace": true}),
        )
        .unwrap();
    assert_eq!(replaced["ids"].as_array().unwrap().len(), 1);

    let selected_image_matches = |s: &Session, expected: &image::RgbaImage| {
        let st = s.doc().unwrap();
        let id = st.selection.objects[0];
        let NodeKind::Image(im) = &st.doc.node(id).unwrap().kind else { return false };
        let Some(blob) = st.doc.images.get(&im.key) else { return false };
        image::load_from_memory(&blob.bytes).is_ok_and(|decoded| {
            let decoded = decoded.to_rgba8();
            decoded.dimensions() == expected.dimensions() && decoded.as_raw() == expected.as_raw()
        })
    };
    assert!(selected_image_matches(&s, &replacement));
    s.execute("edit.undo", &json!({})).unwrap();
    assert!(selected_image_matches(&s, &original), "undo restores the original embedded tile bitmap");
    s.execute("edit.redo", &json!({})).unwrap();
    assert!(selected_image_matches(&s, &replacement), "redo restores the replacement embedded tile bitmap");
    s.execute("object.pattern.done", &json!({})).unwrap();

    let definition_has_image = |document: &vectorcraft_doc::Document, expected: &image::RgbaImage| {
        document.pattern("Bitmap Generic").is_some_and(|definition| {
            definition.art.iter().any(|node| {
                let mut found = false;
                node.walk(&mut |child| {
                    let NodeKind::Image(im) = &child.kind else { return };
                    let Some(blob) = document.images.get(&im.key) else { return };
                    if image::load_from_memory(&blob.bytes).is_ok_and(|decoded| {
                        let decoded = decoded.to_rgba8();
                        decoded.dimensions() == expected.dimensions() && decoded.as_raw() == expected.as_raw()
                    }) {
                        found = true;
                    }
                });
                found
            })
        })
    };
    let target_is_vector_pattern = |s: &Session| {
        let node = s.doc().unwrap().doc.node(NodeId(target)).unwrap();
        node.path_data().is_some_and(|path| !path.is_empty())
            && matches!(node.appearance.fill_paint(), Paint::Pattern { pattern, .. } if pattern == "Bitmap Generic")
    };
    assert!(definition_has_image(&s.doc().unwrap().doc, &replacement));
    assert!(target_is_vector_pattern(&s), "the vector target is not flattened");
    native_reopen(&mut s);
    assert!(definition_has_image(&s.doc().unwrap().doc, &replacement), "native reopen retains the replacement pixels as editable image art");
    assert!(target_is_vector_pattern(&s));

    let svg = s.execute("document.serialize", &json!({"format": "svg"})).unwrap()["text"].as_str().unwrap().to_string();
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).expect("independent usvg parse of the bitmap generic pattern");
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
    fn has_png(group: &usvg::Group, expected: &image::RgbaImage) -> bool {
        group.children().iter().any(|node| match node {
            usvg::Node::Group(child) => has_png(child, expected),
            usvg::Node::Image(image) => match image.kind() {
                usvg::ImageKind::PNG(bytes) => image::load_from_memory(bytes.as_slice()).is_ok_and(|decoded| {
                    let decoded = decoded.to_rgba8();
                    decoded.dimensions() == expected.dimensions() && decoded.as_raw() == expected.as_raw()
                }),
                _ => false,
            },
            _ => false,
        })
    }
    let mut patterns = vec![];
    pattern_paints(tree.root(), &mut patterns);
    let [pattern] = patterns.as_slice() else { panic!("the target has one actual bitmap pattern paint in the independent SVG tree: {svg}") };
    let tile = pattern.rect();
    assert_eq!((tile.x(), tile.y(), tile.width(), tile.height()), (0.0, 0.0, 12.0, 8.0), "the target's actual bitmap tile rectangle");
    assert!(has_png(pattern.root(), &replacement), "the connected tile contains the exact replacement PNG dimensions and pixels: {svg}");

    let pdf = vectorcraft_format::base64_decode(s.execute("document.serialize", &json!({"format": "pdf"})).unwrap()["dataBase64"].as_str().unwrap())
        .unwrap();
    let imported = vectorcraft_pdf::import(&pdf).expect("independent Hayro-backed bitmap pattern PDF read");
    assert_eq!(imported.artboards.len(), 1);
    let definition = imported
        .patterns
        .iter()
        .find(|definition| {
            definition.art.iter().any(|node| {
                let mut found = false;
                node.walk(&mut |child| {
                    let NodeKind::Image(im) = &child.kind else { return };
                    let Some(blob) = imported.images.get(&im.key) else { return };
                    if image::load_from_memory(&blob.bytes).is_ok_and(|decoded| {
                        let decoded = decoded.to_rgba8();
                        decoded.dimensions() == replacement.dimensions() && decoded.as_raw() == replacement.as_raw()
                    }) {
                        found = true;
                    }
                });
                found
            })
        })
        .expect("Hayro reconstructs a real bitmap pattern definition containing the replacement pixels");
    assert!((definition.tile.width() - 12.0).abs() < 1e-4 && (definition.tile.height() - 8.0).abs() < 1e-4, "{:?}", definition.tile);
    let mut connected = false;
    imported.walk(|node| {
        connected |= matches!(node.appearance.fill_paint(), Paint::Pattern { pattern, .. } if *pattern == definition.name);
    });
    assert!(connected, "the reconstructed target paint references the bitmap tile; this is not live PDF editing evidence");
}
