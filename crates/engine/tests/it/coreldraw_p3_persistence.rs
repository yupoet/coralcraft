//! CoralCraft: P3 native/CDR/CMX persistence and fallback audit (P3 Task 10).

use corel_import::coralcraft_color::Color as CdrColor;
use corel_import::coralcraft_doc::{FillStyle, PatternKind};
use serde_json::json;
use vectorcraft_color::Paint;
use vectorcraft_doc::{AppearanceItem, Document, Node, NodeId, NodeKind};
use vectorcraft_engine::Session;

fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 300, "height": 200})).unwrap();
    s
}

fn rectangle(s: &mut Session, x: f64) -> u64 {
    s.execute("shape.rectangle", &json!({"x": x, "y": 30, "width": 60, "height": 50})).unwrap()["id"].as_u64().unwrap()
}

fn reopen(s: &mut Session, format: &str) -> serde_json::Value {
    let exported = s.execute("document.serialize", &json!({"format": format})).unwrap();
    s.execute("document.open", &json!({"name": format!("p3.{format}"), "dataBase64": exported["dataBase64"].as_str().unwrap()})).unwrap();
    exported
}

fn inspect(doc: &Document) -> (usize, Vec<String>, usize) {
    let mut clips = 0;
    let mut effects = Vec::new();
    let mut nonempty_paths = 0;
    for layer in &doc.layers {
        layer.walk(&mut |node| {
            if matches!(node.kind, NodeKind::Group { clip: true, .. }) {
                clips += 1;
            }
            if matches!(node.kind, NodeKind::Path { .. }) && node.path_data().is_some_and(|path| !path.subpaths.is_empty()) {
                nonempty_paths += 1;
            }
            effects.extend(node.appearance.effects.iter().map(|effect| effect.id.clone()));
        });
    }
    (clips, effects, nonempty_paths)
}

#[test]
fn cdr_and_cmx_reopen_powerclip_with_supported_live_effects() {
    for format in ["cdr", "cmx"] {
        let mut s = session();
        let contents = rectangle(&mut s, 20.0);
        s.execute("effect.apply", &json!({"id": contents, "effect": "threeD.extrude", "params": {"depth": 35}})).unwrap();
        s.execute("effect.apply", &json!({"id": contents, "effect": "stylize.dropShadow", "params": {"x": 6, "y": 4}})).unwrap();
        let frame = s.execute("shape.ellipse", &json!({"x": 30, "y": 35, "width": 40, "height": 35})).unwrap()["id"].as_u64().unwrap();
        s.execute("select.set", &json!({"ids": [contents, frame]})).unwrap();
        s.execute("object.clippingMask.make", &json!({})).unwrap();

        reopen(&mut s, format);
        let (clips, effects, _) = inspect(&s.doc().unwrap().doc);
        assert_eq!(clips, 1, "{format} keeps the PowerClip structure");
        assert!(effects.contains(&"threeD.extrude".to_owned()), "{format} effects: {effects:?}");
        assert!(effects.contains(&"stylize.dropShadow".to_owned()), "{format} effects: {effects:?}");
    }
}

#[test]
fn cdr_live_blend_export_warns_and_keeps_visible_vector_geometry() {
    let mut s = session();
    let a = rectangle(&mut s, 20.0);
    let b = rectangle(&mut s, 180.0);
    s.execute("object.blend.make", &json!({"ids": [a, b], "steps": 3})).unwrap();
    let exported = s.execute("document.serialize", &json!({"format": "cdr"})).unwrap();
    let warnings = exported["warnings"].as_array().unwrap();
    assert!(
        warnings.iter().any(|warning| warning.as_str().is_some_and(|text| text.contains("live") && text.contains("Blend"))),
        "explicit live-effect fallback warning: {warnings:?}"
    );
    s.execute("document.open", &json!({"name": "blend.cdr", "dataBase64": exported["dataBase64"].as_str().unwrap()})).unwrap();
    let (_, _, paths) = inspect(&s.doc().unwrap().doc);
    assert!(paths > 0, "the fallback retains visible vector paths rather than an empty path");
}

fn native_bytes(s: &mut Session) -> String {
    s.execute("document.serialize", &json!({"format": "vectorcraft"})).unwrap()["dataBase64"].as_str().unwrap().to_owned()
}

fn open_native(s: &mut Session, data: &str) {
    s.execute("document.open", &json!({"name": "source.vectorcraft", "dataBase64": data})).unwrap();
}

fn ordinary_paths(doc: &Document) -> Vec<&Node> {
    let mut paths = vec![];
    for layer in &doc.layers {
        layer.walk(&mut |node| {
            if matches!(node.kind, NodeKind::Path { .. }) && node.path_data().is_some_and(|path| !path.subpaths.is_empty()) {
                paths.push(node);
            }
        });
    }
    paths
}

fn trace_and_image_counts(doc: &Document) -> (usize, usize) {
    let mut traces = 0;
    let mut images = 0;
    for layer in &doc.layers {
        layer.walk(&mut |node| {
            traces += usize::from(node.extras().trace.is_some());
            images += usize::from(matches!(node.kind, NodeKind::Image(_)));
        });
    }
    (traces, images)
}

fn has_live_trace_warning(warnings: &[serde_json::Value]) -> bool {
    warnings.iter().any(|warning| {
        warning.as_str().is_some_and(|text| {
            text.contains("live trace") && text.contains("parameters and source relationships") && text.contains("ordinary objects")
        })
    })
}

fn session_with_trace() -> (Session, u64) {
    let image = image::RgbaImage::from_fn(60, 60, |x, y| {
        if (12..48).contains(&x) && (12..48).contains(&y) { image::Rgba([20, 40, 80, 255]) } else { image::Rgba([255; 4]) }
    });
    let mut png = std::io::Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png).unwrap();
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 120, "height": 120})).unwrap();
    let placed = s
        .execute(
            "file.place",
            &json!({"name": "trace-source.png", "dataBase64": vectorcraft_format::base64_encode(&png.into_inner()), "link": false, "at": [20, 20]}),
        )
        .unwrap();
    let bitmap = placed["ids"][0].as_u64().unwrap();
    let made = s
        .execute(
            "imageTrace.make",
            &json!({"id": bitmap, "preset": "Default", "params": {"mode": "color", "ignoreWhite": true, "fills": true, "strokes": false}}),
        )
        .unwrap();
    assert!(made["paths"].as_u64().is_some_and(|paths| paths > 0));
    (s, made["id"].as_u64().unwrap())
}

#[test]
fn generic_pattern_export_warns_without_mutating_the_editable_native_pattern() {
    let mut s = session();
    let tile = rectangle(&mut s, 10.0);
    s.execute("paint.setFill", &json!({"id": tile, "color": "#336699"})).unwrap();
    s.execute("object.pattern.make", &json!({"id": tile, "name": "Persistence Tile", "edit": false})).unwrap();
    let target = rectangle(&mut s, 120.0);
    s.execute("paint.setFill", &json!({"id": target, "swatch": "Persistence Tile"})).unwrap();
    s.execute("paint.setStroke", &json!({"id": target, "swatch": "Persistence Tile"})).unwrap();

    let native_source = native_bytes(&mut s);
    open_native(&mut s, &native_source);
    assert!(s.doc().unwrap().doc.pattern("Persistence Tile").is_some());
    assert!(matches!(
        s.doc().unwrap().doc.node(NodeId(target)).unwrap().appearance.fill_paint(),
        Paint::Pattern { pattern, .. } if pattern == "Persistence Tile"
    ));

    let exported_cdr = corel_import::export_document(&s.doc().unwrap().doc).unwrap();
    let default_checker = exported_cdr.pages.iter().flat_map(|page| &page.layers).flat_map(|layer| &layer.objects).any(|object| {
        matches!(
            &object.fill,
            Some(FillStyle::TwoColorPattern { kind: PatternKind::Checkerboard, fg_color, bg_color, tile_size_pt })
                if fg_color == &CdrColor::BLACK && bg_color == &CdrColor::WHITE && (tile_size_pt.get() - 12.0).abs() < 1e-9
        )
    });
    assert!(default_checker, "the measured fallback is the default 12 pt black/white checkerboard");

    let native = native_bytes(&mut s);
    for format in ["cdr", "cmx"] {
        open_native(&mut s, &native);
        let exported = s.execute("document.serialize", &json!({"format": format})).unwrap();
        let warnings = exported["warnings"].as_array().unwrap();
        assert!(
            warnings.iter().any(|warning| {
                warning.as_str().is_some_and(|text| {
                    text.contains("generic Pattern")
                        && text.contains("default black/white checkerboard")
                        && text.contains("native")
                        && text.contains("editable")
                })
            }),
            "{format} names the generic-pattern substitution and editable native source: {warnings:?}"
        );
        assert!(s.doc().unwrap().doc.pattern("Persistence Tile").is_some(), "export does not consume the native definition");
        assert!(matches!(
            s.doc().unwrap().doc.node(NodeId(target)).unwrap().appearance.fill_paint(),
            Paint::Pattern { pattern, .. } if pattern == "Persistence Tile"
        ));

        s.execute("document.open", &json!({"name": format!("pattern.{format}"), "dataBase64": exported["dataBase64"].as_str().unwrap()})).unwrap();
        let doc = &s.doc().unwrap().doc;
        let paths = ordinary_paths(doc);
        assert!(!paths.is_empty());
        assert!(paths.iter().any(|node| matches!(node.appearance.fill_paint(), Paint::Solid { .. })));
        assert!(doc.pattern("Persistence Tile").is_none(), "self-reimport is substituted appearance, not native pattern semantics");
    }

    let mut unused = session();
    let tile = rectangle(&mut unused, 10.0);
    unused.execute("object.pattern.make", &json!({"id": tile, "name": "Unused Tile", "edit": false})).unwrap();
    for format in ["cdr", "cmx"] {
        let exported = unused.execute("document.serialize", &json!({"format": format})).unwrap();
        let warnings = exported["warnings"].as_array().unwrap();
        assert!(!warnings.iter().any(|warning| warning.as_str().is_some_and(|text| text.contains("generic Pattern"))));
    }
}

#[test]
fn live_trace_export_warns_and_reopens_only_as_ordinary_vectors_and_source_image() {
    let (mut s, trace) = session_with_trace();
    s.execute("imageTrace.setView", &json!({"id": trace, "view": "outlines"})).unwrap();
    let native = native_bytes(&mut s);
    open_native(&mut s, &native);
    let stored = s.doc().unwrap().doc.node(NodeId(trace)).unwrap().extras().trace.clone().unwrap();
    assert_eq!(stored["preset"], "Custom");
    assert_eq!(stored["params"]["mode"], "color");
    assert_eq!(stored["view"], "outlines");

    let retraced = s.execute("imageTrace.make", &json!({"id": trace, "preset": "Default", "params": {"paths": 30}})).unwrap()["id"].as_u64().unwrap();
    assert_ne!(retraced, trace, "native reopen permits a real retrace");
    s.execute("imageTrace.setView", &json!({"id": retraced, "view": "outlines"})).unwrap();
    let stored = s.doc().unwrap().doc.node(NodeId(retraced)).unwrap().extras().trace.clone().unwrap();
    assert_eq!(stored["params"]["paths"].as_f64(), Some(30.0));

    let native = native_bytes(&mut s);
    open_native(&mut s, &native);
    assert_eq!(s.doc().unwrap().doc.node(NodeId(retraced)).unwrap().extras().trace.as_ref(), Some(&stored));
    for format in ["cdr", "cmx"] {
        open_native(&mut s, &native);
        let exported = s.execute("document.serialize", &json!({"format": format})).unwrap();
        let warnings = exported["warnings"].as_array().unwrap();
        assert!(has_live_trace_warning(warnings), "{format} explicitly drops live tracing relationships: {warnings:?}");
        assert_eq!(s.doc().unwrap().doc.node(NodeId(retraced)).unwrap().extras().trace.as_ref(), Some(&stored));

        s.execute("document.open", &json!({"name": format!("live-trace.{format}"), "dataBase64": exported["dataBase64"].as_str().unwrap()})).unwrap();
        let doc = &s.doc().unwrap().doc;
        assert!(!ordinary_paths(doc).is_empty(), "{format} retains non-empty ordinary vector results");
        let (traces, images) = trace_and_image_counts(doc);
        assert_eq!(traces, 0, "{format} has no live trace metadata");
        assert!(images > 0, "{format} retains the hidden source as an ordinary image object");
    }
}

fn assert_edited_expanded_path(node: &Node, edited: &vectorcraft_geom::PathData) {
    let actual = node.path_data().expect("ordinary edited path");
    assert_eq!(actual.subpaths.len(), edited.subpaths.len());
    for (actual_subpath, edited_subpath) in actual.subpaths.iter().zip(&edited.subpaths) {
        assert_eq!(actual_subpath.closed, edited_subpath.closed);
        assert_eq!(actual_subpath.anchors.len(), edited_subpath.anchors.len());
        for (actual_anchor, edited_anchor) in actual_subpath.anchors.iter().zip(&edited_subpath.anchors) {
            for (actual_point, edited_point) in
                [(actual_anchor.p, edited_anchor.p), (actual_anchor.h_in, edited_anchor.h_in), (actual_anchor.h_out, edited_anchor.h_out)]
            {
                assert!((actual_point.x - edited_point.x).abs() < 0.001 && (actual_point.y - edited_point.y).abs() < 0.001);
            }
        }
    }
    let stroke = node.appearance.stroke().expect("edited centreline stroke");
    assert!(matches!(&stroke.paint, Paint::Solid { color, .. } if color.to_hex() == "#e04838"));
}

#[test]
fn expanded_trace_edits_round_trip_without_a_live_trace_warning() {
    let image = image::RgbaImage::from_fn(100, 100, |x, y| {
        if (10..90).contains(&x) && (49..52).contains(&y) { image::Rgba([0, 0, 0, 255]) } else { image::Rgba([255; 4]) }
    });
    let mut png = std::io::Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png).unwrap();
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 240, "height": 240})).unwrap();
    let placed = s
        .execute(
            "file.place",
            &json!({"name": "centreline.png", "dataBase64": vectorcraft_format::base64_encode(&png.into_inner()), "link": false, "at": [20, 20]}),
        )
        .unwrap();
    let bitmap = placed["ids"][0].as_u64().unwrap();
    let params = json!({"mode": "blackAndWhite", "ignoreWhite": true, "fills": true, "strokes": true, "strokeWidth": 5});
    let trace = s.execute("imageTrace.make", &json!({"id": bitmap, "preset": "Default", "params": params})).unwrap()["id"].as_u64().unwrap();
    let expanded =
        s.execute("imageTrace.makeAndExpand", &json!({"id": trace, "preset": "Default", "params": params})).unwrap()["id"].as_u64().unwrap();
    assert!(s.doc().unwrap().doc.node(NodeId(expanded)).unwrap().extras().trace.is_none());
    let path = ordinary_paths(&s.doc().unwrap().doc)[0].id;
    s.execute("select.set", &json!({"ids": [path.0]})).unwrap();
    s.execute("paint.setStroke", &json!({"color": "#e04838"})).unwrap();
    s.execute("path.moveAnchor", &json!({"id": path.0, "subpath": 0, "anchor": 0, "x": 37, "y": 43})).unwrap();
    let edited = s.doc().unwrap().doc.node(path).unwrap().path_data().unwrap().clone();
    assert_eq!(edited.subpaths[0].anchors[0].p.x, 37.0);
    assert_eq!(edited.subpaths[0].anchors[0].p.y, 43.0);
    assert_edited_expanded_path(s.doc().unwrap().doc.node(path).unwrap(), &edited);

    let native_source = native_bytes(&mut s);
    open_native(&mut s, &native_source);
    assert_edited_expanded_path(s.doc().unwrap().doc.node(path).unwrap(), &edited);

    let native = native_bytes(&mut s);
    for format in ["cdr", "cmx"] {
        open_native(&mut s, &native);
        let exported = s.execute("document.serialize", &json!({"format": format})).unwrap();
        let warnings = exported["warnings"].as_array().unwrap();
        assert!(!has_live_trace_warning(warnings), "expanded traces are already ordinary paths: {warnings:?}");
        s.execute("document.open", &json!({"name": format!("expanded-trace.{format}"), "dataBase64": exported["dataBase64"].as_str().unwrap()}))
            .unwrap();
        let doc = &s.doc().unwrap().doc;
        let paths = ordinary_paths(doc);
        assert_eq!(paths.len(), 1);
        assert_edited_expanded_path(paths[0], &edited);
        assert_eq!(trace_and_image_counts(doc).0, 0);
    }
}

fn session_with_pattern(name: &str) -> (Session, u64) {
    let mut s = session();
    let tile = rectangle(&mut s, 5.0);
    s.execute("paint.setFill", &json!({"id": tile, "color": "#336699"})).unwrap();
    s.execute("object.pattern.make", &json!({"id": tile, "name": name, "edit": false})).unwrap();
    let target = rectangle(&mut s, 70.0);
    s.execute("paint.setFill", &json!({"id": target, "swatch": name})).unwrap();
    (s, target)
}

fn fallback_warning_count(exported: &serde_json::Value, category: &str) -> usize {
    exported["warnings"].as_array().unwrap().iter().filter(|warning| warning.as_str().is_some_and(|text| text.contains(category))).count()
}

#[test]
fn linked_placed_pattern_and_live_trace_warn_once_per_fallback_category() {
    let (mut source, trace) = session_with_trace();
    add_nested_pattern(&mut source);
    assert!(source.doc().unwrap().doc.node(NodeId(trace)).unwrap().extras().trace.is_some());
    let linked = native_bytes(&mut source);
    let path = std::env::temp_dir().join(format!("coralcraft-p3-linked-{}.vectorcraft", std::process::id()));
    std::fs::write(&path, vectorcraft_format::base64_decode(&linked).unwrap()).unwrap();
    let mut parent = session();
    let placed = parent.execute("file.place", &json!({"path": path, "link": true, "at": [20, 20]})).unwrap();
    assert_eq!(placed["linked"], true);

    for format in ["cdr", "cmx"] {
        let exported = parent.execute("document.serialize", &json!({"format": format})).unwrap();
        assert_eq!(fallback_warning_count(&exported, "generic Pattern"), 1, "{format} pattern warnings: {exported}");
        assert_eq!(fallback_warning_count(&exported, "live trace"), 1, "{format} trace warnings: {exported}");
    }
    std::fs::remove_file(path).unwrap();
}

fn add_nested_pattern(s: &mut Session) {
    let tile = rectangle(s, 5.0);
    s.execute("paint.setFill", &json!({"id": tile, "color": "#336699"})).unwrap();
    s.execute("object.pattern.make", &json!({"id": tile, "name": "Nested Tile", "edit": false})).unwrap();
    let target = rectangle(s, 70.0);
    s.execute("paint.setFill", &json!({"id": target, "swatch": "Nested Tile"})).unwrap();
}

#[test]
fn invisible_and_shadowed_pattern_items_do_not_warn() {
    let (mut invisible, invisible_id) = session_with_pattern("Invisible Tile");
    invisible.execute("appearance.setItem", &json!({"id": invisible_id, "index": 0, "visible": false})).unwrap();

    let (mut shadowed, shadowed_id) = session_with_pattern("Shadowed Tile");
    shadowed.execute("appearance.addFill", &json!({"id": shadowed_id})).unwrap();
    shadowed.execute("appearance.setItem", &json!({"id": shadowed_id, "index": 2, "color": "#112233"})).unwrap();
    let items = &shadowed.doc().unwrap().doc.node(NodeId(shadowed_id)).unwrap().appearance.items;
    assert!(
        matches!(items.last(), Some(AppearanceItem::Fill(fill)) if matches!(fill.paint, Paint::Solid { .. })),
        "last visible fill is solid: {items:?}"
    );

    for (name, s) in [("invisible", &mut invisible), ("shadowed", &mut shadowed)] {
        for format in ["cdr", "cmx"] {
            let exported = s.execute("document.serialize", &json!({"format": format})).unwrap();
            assert_eq!(fallback_warning_count(&exported, "generic Pattern"), 0, "{name} pattern must not warn in {format}: {exported}");
        }
    }
}

#[test]
fn temporary_symbol_and_mask_editing_layers_do_not_warn() {
    let (mut symbol, symbol_id) = session_with_pattern("Symbol Edit Tile");
    symbol.execute("select.set", &json!({"ids": [symbol_id]})).unwrap();
    symbol.execute("symbol.new", &json!({"name": "Temporary Symbol"})).unwrap();
    symbol.execute("symbol.edit", &json!({"name": "Temporary Symbol"})).unwrap();
    assert!(symbol.doc().unwrap().doc.symbol_edit.is_some());

    let mut mask = session();
    let target = rectangle(&mut mask, 150.0);
    mask.execute("paint.setFill", &json!({"id": target, "color": "#ff0000"})).unwrap();
    let tile = rectangle(&mut mask, 5.0);
    mask.execute("paint.setFill", &json!({"id": tile, "color": "#336699"})).unwrap();
    mask.execute("object.pattern.make", &json!({"id": tile, "name": "Mask Edit Tile", "edit": false})).unwrap();
    let mask_id = rectangle(&mut mask, 70.0);
    mask.execute("paint.setFill", &json!({"id": mask_id, "swatch": "Mask Edit Tile"})).unwrap();
    mask.execute("select.set", &json!({"ids": [target, mask_id]})).unwrap();
    let made = mask.execute("transparency.makeOpacityMask", &json!({"clip": true, "invert": false})).unwrap();
    let masked = made["id"].as_u64().unwrap();
    assert!(mask.doc().unwrap().doc.node(NodeId(masked)).unwrap().extras().mask.is_some());
    mask.execute("transparency.editOpacityMask", &json!({"id": masked})).unwrap();
    assert!(mask.doc().unwrap().doc.mask_edit.is_some());

    for (name, s) in [("symbol", &mut symbol), ("mask", &mut mask)] {
        for format in ["cdr", "cmx"] {
            let exported = s.execute("document.serialize", &json!({"format": format})).unwrap();
            assert_eq!(fallback_warning_count(&exported, "generic Pattern"), 0, "{name} editing-only pattern must not warn in {format}: {exported}");
        }
    }
}
