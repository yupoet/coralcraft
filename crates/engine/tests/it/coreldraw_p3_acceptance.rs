//! CoralCraft: P3 poster + traced-image acceptance with native revision and independent readers.

use serde_json::json;
use vectorcraft_color::BlendMode;
use vectorcraft_doc::{NodeId, NodeKind};
use vectorcraft_engine::Session;

fn native_reopen(s: &mut Session) {
    let data = s.execute("document.serialize", &json!({"format": "vectorcraft"})).unwrap()["dataBase64"].as_str().unwrap().to_string();
    s.execute("document.open", &json!({"dataBase64": data})).unwrap();
}

fn ordinary_vector_count(doc: &vectorcraft_doc::Document) -> usize {
    let mut paths = 0;
    for layer in &doc.layers {
        layer.walk(&mut |node| {
            if matches!(node.kind, NodeKind::Path { .. }) && node.path_data().is_some_and(|path| !path.subpaths.is_empty()) {
                paths += 1;
            }
        });
    }
    paths
}

/// Groups the independent `usvg` read finds with a blend mode other than Normal.
fn blended_group_count(svg: &str) -> usize {
    let tree = usvg::Tree::from_str(svg, &usvg::Options::default()).expect("independent usvg parse");
    fn walk(group: &usvg::Group, out: &mut usize) {
        if group.blend_mode() != usvg::BlendMode::Normal {
            *out += 1;
        }
        for node in group.children() {
            if let usvg::Node::Group(child) = node {
                walk(child, out);
            }
        }
    }
    let mut out = 0;
    walk(tree.root(), &mut out);
    out
}

#[test]
fn p3_acceptance_revises_a_poster_and_traced_composition() {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 420, "height": 300})).unwrap();
    let contents = s.execute("shape.rectangle", &json!({"x": 25, "y": 60, "width": 150, "height": 100})).unwrap()["id"].as_u64().unwrap();
    s.execute("effect.apply", &json!({"id": contents, "effect": "threeD.extrude", "params": {"depth": 30}})).unwrap();
    s.execute("effect.apply", &json!({"id": contents, "effect": "stylize.dropShadow", "params": {"x": 6, "y": 5}})).unwrap();
    let frame = s.execute("shape.ellipse", &json!({"x": 40, "y": 70, "width": 100, "height": 80})).unwrap()["id"].as_u64().unwrap();
    s.execute("select.set", &json!({"ids": [contents, frame]})).unwrap();
    let clip = s.execute("object.clippingMask.make", &json!({})).unwrap()["id"].as_u64().unwrap();
    // CoralCraft (P3 final acceptance): the scenario's PowerClip starts unlocked (the E6 channel).
    s.execute("object.clippingMask.setContentsLocked", &json!({"ids": [clip], "lock": false})).unwrap();
    let title = s.execute("text.create", &json!({"x": 25, "y": 30, "text": "Editable poster"})).unwrap()["id"].as_u64().unwrap();

    let a = s.execute("shape.ellipse", &json!({"x": 220, "y": 55, "width": 24, "height": 24})).unwrap()["id"].as_u64().unwrap();
    let b = s.execute("shape.ellipse", &json!({"x": 340, "y": 100, "width": 40, "height": 40})).unwrap()["id"].as_u64().unwrap();
    let blend = s.execute("object.blend.make", &json!({"ids": [a, b], "steps": 3})).unwrap()["id"].as_u64().unwrap();

    // CoralCraft (P3 final acceptance): the E1–E5 surface in the same document — radial Symmetry,
    // Block Shadow, Perspective, an E5 handle-style Zig Zag edit, and a colour lens over the
    // poster art.
    let sym_source = s.execute("shape.ellipse", &json!({"x": 95, "y": 195, "width": 40, "height": 40})).unwrap()["id"].as_u64().unwrap();
    s.execute("select.set", &json!({"ids": [sym_source]})).unwrap();
    let symmetry = s.execute("object.symmetry.create", &json!({"kind": "radial", "lines": 5, "angle": 30.0})).unwrap()["id"].as_u64().unwrap();

    let shadowed = s.execute("shape.rectangle", &json!({"x": 250, "y": 15, "width": 90, "height": 45})).unwrap()["id"].as_u64().unwrap();
    s.execute("effect.apply", &json!({"id": shadowed, "effect": "stylize.blockShadow", "params": {"x": 8, "y": 6}})).unwrap();

    let persp = s.execute("shape.rectangle", &json!({"x": 300, "y": 205, "width": 80, "height": 55})).unwrap()["id"].as_u64().unwrap();
    s.execute(
        "effect.apply",
        &json!({"id": persp, "effect": "distort.perspective", "params": {"corners": [[0.1, 0.0], [0.95, 0.05], [1.0, 1.0], [0.05, 0.95]]}}),
    )
    .unwrap();

    let zig = s.execute("shape.ellipse", &json!({"x": 170, "y": 210, "width": 60, "height": 40})).unwrap()["id"].as_u64().unwrap();
    s.execute("effect.apply", &json!({"id": zig, "effect": "distort.zigZag", "params": {"size": 10.0, "ridges": 4}})).unwrap();

    let lens = s.execute("shape.ellipse", &json!({"x": 60, "y": 85, "width": 70, "height": 55})).unwrap()["id"].as_u64().unwrap();
    s.execute("object.lens", &json!({"id": lens, "kind": "invert", "amount": 60})).unwrap();

    let mut png = std::io::Cursor::new(Vec::new());
    image::RgbaImage::from_fn(24, 24, |x, y| {
        if (5..19).contains(&x) && (4..20).contains(&y) { image::Rgba([30, 120, 180, 255]) } else { image::Rgba([255; 4]) }
    })
    .write_to(&mut png, image::ImageFormat::Png)
    .unwrap();
    let bitmap = s
        .execute(
            "file.place",
            &json!({"name": "synthetic.png", "dataBase64": vectorcraft_format::base64_encode(&png.into_inner()), "link": false, "at": [220, 180]}),
        )
        .unwrap()["ids"][0]
        .as_u64()
        .unwrap();
    let trace = s.execute("imageTrace.make", &json!({"id": bitmap, "params": {"ignoreWhite": true}})).unwrap()["id"].as_u64().unwrap();
    assert!(s.execute("imageTrace.makeAndExpand", &json!({"id": trace})).unwrap()["paths"].as_u64().unwrap() > 0);

    // Two native reopens with source and parameter revisions between them.
    native_reopen(&mut s);
    s.execute("effect.setParams", &json!({"id": contents, "index": 0, "params": {"depth": 55}})).unwrap();
    s.execute("paint.setFill", &json!({"id": contents, "color": "#ff0000"})).unwrap();
    s.execute("text.setText", &json!({"id": title, "text": "Editable poster — revised"})).unwrap();
    s.execute("object.symmetry.setProps", &json!({"id": symmetry, "angle": 45.0})).unwrap();
    s.execute("effect.setParams", &json!({"id": zig, "index": 0, "params": {"size": 9.0, "ridges": 6}})).unwrap();
    native_reopen(&mut s);

    // Native editability of every scenario object after both reopens.
    let st = s.doc().unwrap();
    assert_eq!(st.doc.node(NodeId(clip)).unwrap().kind_label(), "Clip Group");
    assert!(st.doc.node(NodeId(clip)).unwrap().extras().contents_unlocked, "the unlock flag survives both reopens");
    assert!(matches!(st.doc.node(NodeId(blend)).unwrap().kind, NodeKind::Blend { .. }));
    assert_eq!(st.doc.node(NodeId(contents)).unwrap().appearance.effects[0].params["depth"].as_f64(), Some(55.0));
    let NodeKind::Text(text) = &st.doc.node(NodeId(title)).unwrap().kind else { panic!("live title") };
    assert_eq!(text.plain_text(), "Editable poster — revised");
    let NodeKind::Repeat(repeat) = &st.doc.node(NodeId(symmetry)).unwrap().kind else { panic!("live symmetry") };
    match &repeat.kind {
        vectorcraft_doc::RepeatKind::Radial { instances, start_angle, .. } => {
            assert_eq!(*instances, 5, "radial instances survive: {:?}", repeat.kind);
            assert_eq!(*start_angle, 45.0, "the revised angle survives: {:?}", repeat.kind);
        }
        other => panic!("radial symmetry: {other:?}"),
    }
    assert_eq!(st.doc.node(NodeId(zig)).unwrap().appearance.effects[0].params["size"].as_f64(), Some(9.0));
    assert_eq!(st.doc.node(NodeId(zig)).unwrap().appearance.effects[0].params["ridges"].as_f64(), Some(6.0));
    assert!(st.doc.node(NodeId(persp)).unwrap().appearance.effects.iter().any(|e| e.id == "distort.perspective"));
    assert!(st.doc.node(NodeId(shadowed)).unwrap().appearance.effects.iter().any(|e| e.id == "stylize.blockShadow"));
    let lens_fill = st.doc.node(NodeId(lens)).unwrap().appearance.fill().expect("lens fill");
    assert_eq!(lens_fill.blend, BlendMode::Difference, "the lens keeps its blend mode");
    assert_eq!(lens_fill.opacity, 0.6, "the lens keeps its amount as opacity");

    // Independent usvg structure: the clip path and exactly the lens's non-Normal blend group.
    let svg = s.execute("document.serialize", &json!({"format": "svg"})).unwrap()["text"].as_str().unwrap().to_string();
    usvg::Tree::from_str(&svg, &usvg::Options::default()).expect("independent usvg parse of the P3 poster");
    assert!(svg.contains("<clipPath"), "PowerClip remains visible in the SVG");
    assert_eq!(blended_group_count(&svg), 1, "exactly the lens blends in the evaluated SVG");
    // Independent Hayro-backed PDF structure.
    let pdf = vectorcraft_format::base64_decode(s.execute("document.serialize", &json!({"format": "pdf"})).unwrap()["dataBase64"].as_str().unwrap())
        .unwrap();
    assert_eq!(vectorcraft_pdf::import(&pdf).expect("independent Hayro-backed P3 PDF read").artboards.len(), 1);

    // CDR/CMX: non-empty ordinary vectors, and every actual fallback named exactly once. Each
    // leg restores the native document directly: reimporting a CDR/CMX flattens it.
    let native = s.execute("document.serialize", &json!({"format": "vectorcraft"})).unwrap()["dataBase64"].as_str().unwrap().to_owned();
    for format in ["cdr", "cmx"] {
        let out = s.execute("document.serialize", &json!({"format": format})).unwrap();
        assert_exact_cdr_cmx_fallbacks(&out, format);

        let bytes = vectorcraft_format::base64_decode(out["dataBase64"].as_str().unwrap()).unwrap();
        let imported = corel_import::import_cdr_bytes(&bytes).unwrap_or_else(|error| panic!("{format} import failed: {error}"));
        assert!(ordinary_vector_count(&imported.document) > 0, "{format} reopens non-empty ordinary vectors");
        s.execute("document.open", &json!({"name": format!("p3.{format}"), "dataBase64": out["dataBase64"].as_str().unwrap()})).unwrap();
        let doc = &s.doc().unwrap().doc;
        let mut live = 0;
        for layer in &doc.layers {
            layer.walk(&mut |node| live += usize::from(matches!(node.kind, NodeKind::Blend { .. } | NodeKind::Repeat(_))));
        }
        assert_eq!(live, 0, "{format} carries no live blend or symmetry");
        s.execute("document.open", &json!({"name": "p3.vectorcraft", "dataBase64": native})).unwrap();
    }

    // Export never mutated the native live sources.
    assert!(
        matches!(s.doc().unwrap().doc.node(NodeId(blend)).unwrap().kind, NodeKind::Blend { .. }),
        "export does not mutate the native live source"
    );
}

/// The scenario's exact CDR/CMX fallback warnings: the live Blend/Radial-Repeat expansion, the
/// dropped lens merge mode, the unsupported perspective effect, and the PowerClip relock each
/// named exactly once — and nothing else. The scenario's trace was expanded (makeAndExpand), so
/// it is ordinary objects with no live-trace fallback to name.
fn assert_exact_cdr_cmx_fallbacks(out: &serde_json::Value, format: &str) {
    let warnings = out["warnings"].as_array().unwrap();
    let count = |pred: &dyn Fn(&str) -> bool| warnings.iter().filter(|w| w.as_str().is_some_and(pred)).count();
    let live_expansion = count(&|text| {
        text.starts_with("live ")
            && text.ends_with("objects are expanded to plain vector groups in CDR/CMX; the native document keeps the editable live sources")
    });
    let merge_modes = count(&|text| text == "merge modes other than Normal are dropped in CDR/CMX; native blend modes remain editable");
    let unsupported = count(&|text| text.starts_with("unsupported bitmap effects are dropped in CDR/CMX:") && text.contains("distort.perspective"));
    let live_trace =
        count(&|text| text.contains("live trace") && text.contains("parameters and source relationships") && text.contains("ordinary objects"));
    let relock = count(&|text| text == "unlocked PowerClip contents (Lock Contents to Frame off) relock in CDR/CMX; native documents keep the flag");
    assert_eq!(live_expansion, 1, "{format} names the live expansion once: {warnings:?}");
    assert_eq!(merge_modes, 1, "{format} names the dropped lens merge mode once: {warnings:?}");
    assert_eq!(unsupported, 1, "{format} names the unsupported perspective effect once: {warnings:?}");
    assert_eq!(relock, 1, "{format} names the relock once: {warnings:?}");
    assert_eq!(live_trace, 0, "{format}: the expanded trace carries no live warning: {warnings:?}");
    assert_eq!(warnings.len(), 4, "{format} has no unexplained warnings: {warnings:?}");
}
