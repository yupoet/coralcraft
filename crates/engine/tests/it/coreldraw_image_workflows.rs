//! CoralCraft: bitmap and PowerTRACE workflows over the shared tracer (P3 Tasks 8–9).

use serde_json::json;
use vectorcraft_doc::{NodeId, NodeKind, TraceView};
use vectorcraft_engine::Session;

fn native_reopen(s: &mut Session) {
    let data = s.execute("document.serialize", &json!({"format": "vectorcraft"})).unwrap()["dataBase64"].as_str().unwrap().to_string();
    s.execute("document.open", &json!({"dataBase64": data})).unwrap();
}

#[test]
fn powertrace_reopens_live_and_expands_to_editable_paths() {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 200, "height": 200})).unwrap();
    let mut png = std::io::Cursor::new(Vec::new());
    image::RgbaImage::from_fn(24, 24, |x, y| {
        if (5..19).contains(&x) && (4..20).contains(&y) { image::Rgba([20, 40, 80, 255]) } else { image::Rgba([255; 4]) }
    })
    .write_to(&mut png, image::ImageFormat::Png)
    .unwrap();
    let placed = s
        .execute(
            "file.place",
            &json!({"name": "synthetic.png", "dataBase64": vectorcraft_format::base64_encode(&png.into_inner()), "link": false, "at": [20, 20]}),
        )
        .unwrap();
    let bitmap = placed["ids"][0].as_u64().unwrap();
    assert!(matches!(s.doc().unwrap().doc.node(NodeId(bitmap)).unwrap().kind, NodeKind::Image(_)));

    let made = s.execute("imageTrace.make", &json!({"id": bitmap, "params": {"mode": "color", "ignoreWhite": true}})).unwrap();
    let trace = made["id"].as_u64().unwrap();
    assert!(made["paths"].as_u64().unwrap() > 0, "{made}");
    assert!(matches!(s.doc().unwrap().doc.node(NodeId(trace)).unwrap().kind, NodeKind::Group { .. }));
    s.execute("imageTrace.setView", &json!({"id": trace, "view": "outlines"})).unwrap();
    native_reopen(&mut s);
    assert_eq!(s.doc().unwrap().doc.node(NodeId(trace)).unwrap().trace_view(), TraceView::Outlines);
    assert!(s.doc().unwrap().doc.node(NodeId(trace)).unwrap().extras().trace.is_some());

    let expanded = s.execute("imageTrace.makeAndExpand", &json!({"id": trace})).unwrap();
    assert!(expanded["paths"].as_u64().unwrap() > 0, "{expanded}");
    native_reopen(&mut s);
    let paths = s.execute("document.inspect", &json!({"kind": "Path"})).unwrap();
    assert!(!paths["nodes"].as_array().unwrap().is_empty(), "expanded paths survive native reopen");
    let first = paths["nodes"][0]["id"].as_u64().unwrap();
    s.execute("select.set", &json!({"ids": [first]})).unwrap();
    s.execute("paint.setFill", &json!({"color": "#ff0000"})).unwrap();
    assert_eq!(s.doc().unwrap().doc.node(NodeId(first)).unwrap().appearance.fill_paint().color().unwrap().to_hex(), "#ff0000");
    let svg = s.execute("document.serialize", &json!({"format": "svg"})).unwrap()["text"].as_str().unwrap().to_string();
    usvg::Tree::from_str(&svg, &usvg::Options::default()).expect("independent usvg parse of expanded PowerTRACE paths");
    assert!(svg.contains("fill=\"#ff0000\""), "the edited traced path is exported: {svg}");
}

#[test]
fn bitmap_crop_and_colour_adjustment_survive_undo_and_native_reopen() {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 200, "height": 200})).unwrap();
    let mut png = std::io::Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(24, 24, image::Rgba([30, 120, 180, 255])).write_to(&mut png, image::ImageFormat::Png).unwrap();
    let bitmap = s
        .execute(
            "file.place",
            &json!({"name": "synthetic.png", "dataBase64": vectorcraft_format::base64_encode(&png.into_inner()), "link": false, "at": [40, 40]}),
        )
        .unwrap()["ids"][0]
        .as_u64()
        .unwrap();
    let cropped = s.execute("object.cropImage", &json!({"id": bitmap, "rect": [30, 30, 12, 12]})).unwrap();
    assert_eq!((cropped["width"].as_u64(), cropped["height"].as_u64()), (Some(12), Some(12)));
    s.execute("edit.undo", &json!({})).unwrap();
    s.execute("edit.redo", &json!({})).unwrap();
    s.execute("effect.apply", &json!({"id": bitmap, "effect": "adjust.brightnessContrast", "params": {"brightness": 10, "contrast": 20}})).unwrap();
    native_reopen(&mut s);
    let node = s.doc().unwrap().doc.node(NodeId(bitmap)).unwrap();
    assert!(matches!(node.kind, NodeKind::Image(_)));
    assert_eq!(node.appearance.effects[0].id, "adjust.brightnessContrast");
    assert_eq!(node.appearance.effects[0].params["contrast"].as_f64(), Some(20.0));
    let svg = s.execute("document.serialize", &json!({"format": "svg"})).unwrap()["text"].as_str().unwrap().to_string();
    usvg::Tree::from_str(&svg, &usvg::Options::default()).expect("independent usvg parse of the adjusted bitmap");
}

#[test]
fn invalid_bitmap_crop_is_refused_without_document_or_history_changes() {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 200, "height": 200})).unwrap();
    let mut png = std::io::Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(8, 8, image::Rgba([30, 120, 180, 255])).write_to(&mut png, image::ImageFormat::Png).unwrap();
    let bitmap = s
        .execute(
            "file.place",
            &json!({"name": "synthetic.png", "dataBase64": vectorcraft_format::base64_encode(&png.into_inner()), "link": false, "at": [40, 40]}),
        )
        .unwrap()["ids"][0]
        .as_u64()
        .unwrap();
    let before = s.doc().unwrap().doc.clone();
    let history = s.doc().unwrap().history.undo.len();
    assert!(s.execute("object.cropImage", &json!({"id": bitmap, "rect": [500, 500, 10, 10]})).is_err());
    assert!(s.execute("object.cropImage", &json!({"id": bitmap, "rect": "12"})).is_err());
    assert_eq!(s.doc().unwrap().doc, before);
    assert_eq!(s.doc().unwrap().history.undo.len(), history);
}

fn synthetic_rgba_png(width: u32, height: u32) -> (Vec<u8>, image::RgbaImage) {
    let source =
        image::RgbaImage::from_fn(width, height, |x, y| image::Rgba([(x * 31 + 10) as u8, (y * 47 + 20) as u8, (x * 13 + y * 19 + 30) as u8, 255]));
    let mut png = std::io::Cursor::new(Vec::new());
    source.write_to(&mut png, image::ImageFormat::Png).unwrap();
    (png.into_inner(), source)
}

#[test]
fn bitmap_resample_preserves_identity_bounds_appearance_and_native_history() {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 200, "height": 200})).unwrap();
    let (png, source) = synthetic_rgba_png(4, 2);
    let bitmap = s
        .execute(
            "file.place",
            &json!({"name": "synthetic.png", "dataBase64": vectorcraft_format::base64_encode(&png), "link": false, "at": [40, 50]}),
        )
        .unwrap()["ids"][0]
        .as_u64()
        .unwrap();
    assert_eq!(image::load_from_memory(&png).unwrap().to_rgba8(), source);
    s.execute("effect.apply", &json!({"id": bitmap, "effect": "adjust.brightnessContrast", "params": {"brightness": 7, "contrast": 11}})).unwrap();
    let before_bounds = s.doc().unwrap().doc.node(NodeId(bitmap)).unwrap().geometric_bounds().unwrap();
    let before_xf = match &s.doc().unwrap().doc.node(NodeId(bitmap)).unwrap().kind {
        NodeKind::Image(im) => im.xf,
        _ => panic!("placed source is an image"),
    };
    s.execute("select.none", &json!({})).unwrap();

    let result = s.execute("object.resampleImage", &json!({"id": bitmap, "width": 12, "height": 9, "filter": "lanczos3"})).unwrap();
    assert_eq!((result["id"].as_u64(), result["width"].as_u64(), result["height"].as_u64()), (Some(bitmap), Some(12), Some(9)));
    assert_eq!(result["filter"].as_str(), Some("lanczos3"));
    let node = s.doc().unwrap().doc.node(NodeId(bitmap)).unwrap();
    assert_eq!(node.geometric_bounds().unwrap(), before_bounds);
    assert_eq!(node.appearance.effects[0].params["contrast"].as_f64(), Some(11.0));
    let NodeKind::Image(im) = &node.kind else { panic!("resampled node stays an image") };
    assert_eq!((im.width, im.height), (12, 9));
    let expected_xf = before_xf * vectorcraft_geom::Affine::scale_non_uniform(4.0 / 12.0, 2.0 / 9.0);
    assert_eq!(im.xf, expected_xf);
    assert!(im.link.is_none());
    let blob = &s.doc().unwrap().doc.images[&im.key];
    assert_eq!(blob.mime, "image/png");
    let decoded = image::load_from_memory(&blob.bytes).unwrap().to_rgba8();
    assert_eq!(decoded.dimensions(), (12, 9));
    assert_eq!(decoded, image::imageops::resize(&source, 12, 9, image::imageops::FilterType::Lanczos3));

    s.execute("edit.undo", &json!({})).unwrap();
    let NodeKind::Image(im) = &s.doc().unwrap().doc.node(NodeId(bitmap)).unwrap().kind else { panic!("undo restores the image") };
    assert_eq!((im.width, im.height), (4, 2));
    assert_eq!(image::load_from_memory(&s.doc().unwrap().doc.images[&im.key].bytes).unwrap().to_rgba8(), source);
    s.execute("edit.redo", &json!({})).unwrap();
    native_reopen(&mut s);
    let node = s.doc().unwrap().doc.node(NodeId(bitmap)).unwrap();
    assert_eq!(node.geometric_bounds().unwrap(), before_bounds);
    assert_eq!(node.appearance.effects[0].params["brightness"].as_f64(), Some(7.0));
    let NodeKind::Image(im) = &node.kind else { panic!("native reopen keeps the image") };
    assert_eq!((im.width, im.height), (12, 9));
    let decoded = image::load_from_memory(&s.doc().unwrap().doc.images[&im.key].bytes).unwrap();
    assert_eq!((decoded.width(), decoded.height()), (12, 9));
    let svg = s.execute("document.serialize", &json!({"format": "svg"})).unwrap()["text"].as_str().unwrap().to_string();
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).expect("independent usvg parse of the resampled bitmap");
    fn image_node(node: &usvg::Node) -> Option<&usvg::Image> {
        match node {
            usvg::Node::Group(group) => group.children().iter().find_map(image_node),
            usvg::Node::Image(image) => Some(image),
            _ => None,
        }
    }
    let image = tree.root().children().iter().find_map(image_node).expect("an independently parsed image node");
    assert_eq!((image.size().width(), image.size().height()), (12.0, 9.0));
    let actual = image.abs_transform();
    let actual = [actual.sx, actual.ky, actual.kx, actual.sy, actual.tx, actual.ty];
    for (actual, expected) in actual.into_iter().zip(expected_xf.as_coeffs()) {
        assert!((actual - expected as f32).abs() < 5e-4, "independent image transform: {actual} != {expected}");
    }
}

#[test]
fn selected_bitmap_resample_defaults_to_lanczos3() {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 200, "height": 200})).unwrap();
    let (png, _) = synthetic_rgba_png(4, 2);
    let bitmap = s
        .execute(
            "file.place",
            &json!({"name": "synthetic.png", "dataBase64": vectorcraft_format::base64_encode(&png), "link": false, "at": [40, 50]}),
        )
        .unwrap()["ids"][0]
        .as_u64()
        .unwrap();
    let before = s.doc().unwrap().doc.node(NodeId(bitmap)).unwrap().geometric_bounds().unwrap();
    let result = s.execute("object.resampleImage", &json!({"width": 2, "height": 2})).unwrap();
    assert_eq!((result["id"].as_u64(), result["filter"].as_str()), (Some(bitmap), Some("lanczos3")));
    assert_eq!(s.doc().unwrap().doc.node(NodeId(bitmap)).unwrap().geometric_bounds().unwrap(), before);
    let NodeKind::Image(im) = &s.doc().unwrap().doc.node(NodeId(bitmap)).unwrap().kind else { panic!("selected source is an image") };
    let decoded = image::load_from_memory(&s.doc().unwrap().doc.images[&im.key].bytes).unwrap();
    assert_eq!((decoded.width(), decoded.height()), (2, 2));
}

#[test]
fn invalid_bitmap_resample_is_refused_without_document_or_history_changes() {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 200, "height": 200})).unwrap();
    let (png, _) = synthetic_rgba_png(8, 8);
    let bitmap = s
        .execute(
            "file.place",
            &json!({"name": "synthetic.png", "dataBase64": vectorcraft_format::base64_encode(&png), "link": false, "at": [40, 40]}),
        )
        .unwrap()["ids"][0]
        .as_u64()
        .unwrap();
    let shape = s.execute("shape.rectangle", &json!({"x": 100, "y": 100, "width": 20, "height": 20})).unwrap()["id"].as_u64().unwrap();
    s.execute("select.none", &json!({})).unwrap();
    let before = s.doc().unwrap().doc.clone();
    let history = s.doc().unwrap().history.undo.len();
    let invalid = [
        ("missing width", json!({"id": bitmap, "height": 4})),
        ("missing height", json!({"id": bitmap, "width": 4})),
        ("zero width", json!({"id": bitmap, "width": 0, "height": 4})),
        ("zero height", json!({"id": bitmap, "width": 4, "height": 0})),
        ("negative width", json!({"id": bitmap, "width": -1, "height": 4})),
        ("fractional height", json!({"id": bitmap, "width": 4, "height": 2.5})),
        ("non-numeric width", json!({"id": bitmap, "width": "4", "height": 4})),
        ("excessive output", json!({"id": bitmap, "width": 1_000_000_000_u64, "height": 1_000_000_000_u64})),
        ("unsupported filter", json!({"id": bitmap, "width": 4, "height": 4, "filter": "cubic"})),
        ("non-string filter", json!({"id": bitmap, "width": 4, "height": 4, "filter": 3})),
        ("non-image", json!({"id": shape, "width": 4, "height": 4})),
        ("unknown image", json!({"id": u64::MAX, "width": 4, "height": 4})),
        ("malformed id", json!({"id": "image", "width": 4, "height": 4})),
        ("no selected image", json!({"width": 4, "height": 4})),
    ];
    for (name, params) in invalid {
        let error = s.execute("object.resampleImage", &params).expect_err(name).to_string();
        assert!(!error.contains("unknown command"), "{name} reaches input validation: {error}");
        assert_eq!(s.doc().unwrap().doc, before, "{name} leaves the document unchanged");
        assert_eq!(s.doc().unwrap().history.undo.len(), history, "{name} leaves history unchanged");
    }

    let key = match &s.doc().unwrap().doc.node(NodeId(bitmap)).unwrap().kind {
        NodeKind::Image(im) => im.key.clone(),
        _ => panic!("source is an image"),
    };
    std::sync::Arc::make_mut(&mut s.active_mut().unwrap().doc).images.get_mut(&key).unwrap().bytes = std::sync::Arc::new(b"not an image".to_vec());
    let before = s.doc().unwrap().doc.clone();
    assert!(s.execute("object.resampleImage", &json!({"id": bitmap, "width": 4, "height": 4})).is_err());
    assert_eq!(s.doc().unwrap().doc, before);
    assert_eq!(s.doc().unwrap().history.undo.len(), history);
}

#[test]
fn cmyk_bitmap_resample_is_rejected_atomically() {
    let inks = vectorcraft_doc::cmyk::Inks::new(4, 2, [255, 0, 0, 0].repeat(8)).unwrap();
    let tiff = vectorcraft_doc::ImageBlob::cmyk_tiff(&inks).unwrap();
    let mut s = Session::new();
    s.execute("document.open", &json!({"name": "inks.tif", "dataBase64": vectorcraft_format::base64_encode(&tiff.bytes)})).unwrap();
    s.execute("select.all", &json!({})).unwrap();
    let bitmap = s.doc().unwrap().selection.objects.first().unwrap().0;
    let before = s.doc().unwrap().doc.clone();
    let history = s.doc().unwrap().history.undo.len();
    let error = s.execute("object.resampleImage", &json!({"id": bitmap, "width": 8, "height": 4})).unwrap_err().to_string();
    assert!(error.contains("CMYK"), "{error}");
    assert_eq!(s.doc().unwrap().doc, before);
    assert_eq!(s.doc().unwrap().history.undo.len(), history);
}

#[test]
fn extreme_aspect_ratio_resample_is_rejected_before_float_intermediate_allocation() {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 200, "height": 200})).unwrap();
    let (png, _) = synthetic_rgba_png(32_768, 1);
    let bitmap = s
        .execute("file.place", &json!({"name": "wide.png", "dataBase64": vectorcraft_format::base64_encode(&png), "link": false, "at": [40, 40]}))
        .unwrap()["ids"][0]
        .as_u64()
        .unwrap();
    let before = s.doc().unwrap().doc.clone();
    let history = s.doc().unwrap().history.undo.len();
    let error = s.execute("object.resampleImage", &json!({"id": bitmap, "width": 1, "height": 16_384, "filter": "nearest"})).unwrap_err().to_string();
    assert!(error.contains("intermediate"), "{error}");
    assert_eq!(s.doc().unwrap().doc, before);
    assert_eq!(s.doc().unwrap().history.undo.len(), history);
}

#[test]
fn every_resample_filter_mapping_produces_the_library_pixels() {
    for (name, filter) in [
        ("nearest", image::imageops::FilterType::Nearest),
        ("triangle", image::imageops::FilterType::Triangle),
        ("catmullRom", image::imageops::FilterType::CatmullRom),
    ] {
        let mut s = Session::new();
        s.execute("file.new", &json!({"width": 200, "height": 200})).unwrap();
        let (png, source) = synthetic_rgba_png(4, 2);
        let bitmap = s
            .execute(
                "file.place",
                &json!({"name": "synthetic.png", "dataBase64": vectorcraft_format::base64_encode(&png), "link": false, "at": [40, 50]}),
            )
            .unwrap()["ids"][0]
            .as_u64()
            .unwrap();
        let result = s.execute("object.resampleImage", &json!({"id": bitmap, "width": 3, "height": 3, "filter": name})).unwrap();
        assert_eq!(result["filter"].as_str(), Some(name));
        let NodeKind::Image(im) = &s.doc().unwrap().doc.node(NodeId(bitmap)).unwrap().kind else { panic!("resampled image") };
        let decoded = image::load_from_memory(&s.doc().unwrap().doc.images[&im.key].bytes).unwrap().to_rgba8();
        assert_eq!(decoded, image::imageops::resize(&source, 3, 3, filter), "{name}");
    }
}

#[test]
fn linked_bitmap_resample_embeds_a_derived_png_without_touching_the_source_file() {
    let dir = vectorcraft_testkit::temp_dir("coreldraw-linked-resample");
    let path = dir.join("linked.png");
    let (png, _) = synthetic_rgba_png(4, 2);
    std::fs::write(&path, &png).unwrap();
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 200, "height": 200})).unwrap();
    let bitmap = s.execute("file.place", &json!({"path": path.to_string_lossy(), "link": true, "at": [40, 50]})).unwrap()["ids"][0].as_u64().unwrap();
    let NodeKind::Image(linked) = &s.doc().unwrap().doc.node(NodeId(bitmap)).unwrap().kind else { panic!("linked source") };
    assert!(linked.link.is_some());
    let source_before = std::fs::read(&path).unwrap();
    s.execute("object.resampleImage", &json!({"id": bitmap, "width": 8, "height": 4, "filter": "nearest"})).unwrap();
    let NodeKind::Image(im) = &s.doc().unwrap().doc.node(NodeId(bitmap)).unwrap().kind else { panic!("derived image") };
    assert!(im.link.is_none());
    let blob = &s.doc().unwrap().doc.images[&im.key];
    assert_eq!(blob.mime, "image/png");
    let decoded = image::load_from_memory(&blob.bytes).unwrap();
    assert_eq!((decoded.width(), decoded.height()), (8, 4));
    assert_eq!(std::fs::read(&path).unwrap(), source_before);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn bitmap_powerclip_gaussian_blur_stays_live_and_exports_connected_structure() {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 160, "height": 120})).unwrap();
    let (png, source) = synthetic_rgba_png(24, 18);
    let bitmap = s
        .execute(
            "file.place",
            &json!({"name": "powerclip-source.png", "dataBase64": vectorcraft_format::base64_encode(&png), "link": false, "at": [20, 30]}),
        )
        .unwrap()["ids"][0]
        .as_u64()
        .unwrap();
    let frame = s.execute("shape.rectangle", &json!({"x": 28, "y": 34, "width": 42, "height": 32})).unwrap()["id"].as_u64().unwrap();
    s.execute("select.set", &json!({"ids": [bitmap, frame]})).unwrap();
    let clip = s.execute("object.clippingMask.make", &json!({})).unwrap()["id"].as_u64().unwrap();

    s.execute("effect.apply", &json!({"id": bitmap, "effect": "adjust.brightnessContrast", "params": {"brightness": 3, "contrast": 5}})).unwrap();
    s.execute("effect.apply", &json!({"id": bitmap, "effect": "blur.gaussian", "params": {"radius": 2}})).unwrap();
    s.execute("effect.setParams", &json!({"id": bitmap, "index": 1, "params": {"radius": 8}})).unwrap();
    assert_eq!(s.doc().unwrap().doc.node(NodeId(bitmap)).unwrap().appearance.effects[1].params["radius"].as_f64(), Some(8.0));
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(s.doc().unwrap().doc.node(NodeId(bitmap)).unwrap().appearance.effects[1].params["radius"].as_f64(), Some(2.0));
    s.execute("edit.redo", &json!({})).unwrap();
    assert_eq!(s.doc().unwrap().doc.node(NodeId(bitmap)).unwrap().appearance.effects[1].params["radius"].as_f64(), Some(8.0));

    native_reopen(&mut s);
    let doc = &s.doc().unwrap().doc;
    let NodeKind::Group { clip: true, children } = &doc.node(NodeId(clip)).unwrap().kind else { panic!("native reopen keeps a live clip group") };
    assert_eq!(children.iter().map(|child| child.id).collect::<Vec<_>>(), [NodeId(frame), NodeId(bitmap)]);
    let NodeKind::Path { path, .. } = &doc.node(NodeId(frame)).unwrap().kind else { panic!("the PowerClip frame remains a vector path") };
    assert!(!path.is_empty());
    let bitmap_node = doc.node(NodeId(bitmap)).unwrap();
    let NodeKind::Image(im) = &bitmap_node.kind else { panic!("the clipped source remains an editable bitmap") };
    assert!(im.link.is_none());
    assert_eq!((im.width, im.height), (24, 18));
    let blob = &doc.images[&im.key];
    assert_eq!(blob.mime, "image/png");
    assert_eq!(blob.bytes.as_slice(), png.as_slice(), "native reopen retains the original embedded PNG bytes");
    assert_eq!(image::load_from_memory(&blob.bytes).unwrap().to_rgba8(), source);
    let effects = &bitmap_node.appearance.effects;
    assert_eq!(effects.iter().map(|effect| effect.id.as_str()).collect::<Vec<_>>(), ["adjust.brightnessContrast", "blur.gaussian"]);
    assert_eq!((effects[0].params["brightness"].as_f64(), effects[0].params["contrast"].as_f64()), (Some(3.0), Some(5.0)));
    assert_eq!(effects[1].params["radius"].as_f64(), Some(8.0));
    assert!(doc.is_editable(NodeId(bitmap)) && doc.is_editable(NodeId(frame)));
    s.execute("select.set", &json!({"ids": [clip]})).unwrap();
    s.execute("object.clippingMask.editContents", &json!({})).unwrap();
    assert_eq!(s.doc().unwrap().selection.objects, vec![NodeId(bitmap)]);

    let expected_doc = vectorcraft_render::effects::bake_document(&s.doc().unwrap().doc).expect("export bakes the colour adjustment");
    let NodeKind::Image(expected_image) = &expected_doc.node(NodeId(bitmap)).unwrap().kind else { panic!("export keeps the bitmap as an image") };
    let expected_pixels = image::load_from_memory(&expected_doc.image_blob(expected_image).unwrap().bytes).unwrap().to_rgba8();
    assert_ne!(expected_pixels, source, "the expected export pixels include the colour adjustment");

    let svg = s.execute("document.serialize", &json!({"format": "svg"})).unwrap()["text"].as_str().unwrap().to_string();
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).expect("independent usvg parse of the clipped blurred bitmap");
    fn image_matches(image: &usvg::Image, expected: &image::RgbaImage) -> bool {
        if (image.size().width() as u32, image.size().height() as u32) != expected.dimensions() {
            return false;
        }
        match image.kind() {
            usvg::ImageKind::PNG(bytes) => {
                let decoded = image::load_from_memory(bytes.as_slice()).expect("the independently parsed PNG decodes").to_rgba8();
                assert_eq!(decoded.dimensions(), expected.dimensions());
                assert_eq!(decoded, *expected, "the exported PNG has the colour-adjusted RGBA pixels");
                true
            }
            _ => false,
        }
    }
    fn contains_source_image(group: &usvg::Group, source: &image::RgbaImage) -> bool {
        group.children().iter().any(|node| match node {
            usvg::Node::Group(child) => contains_source_image(child, source),
            usvg::Node::Image(image) => image_matches(image, source),
            _ => false,
        })
    }
    fn blurred_source(group: &usvg::Group, source: &image::RgbaImage) -> bool {
        let gaussian = group.filters().iter().any(|filter| {
            filter.primitives().iter().any(|primitive| match primitive.kind() {
                usvg::filter::Kind::GaussianBlur(blur) => {
                    matches!(blur.input(), usvg::filter::Input::SourceGraphic)
                        && (blur.std_dev_x().get() - 4.0).abs() < 1e-4
                        && (blur.std_dev_y().get() - 4.0).abs() < 1e-4
                }
                _ => false,
            })
        });
        (gaussian && contains_source_image(group, source))
            || group.children().iter().any(|node| match node {
                usvg::Node::Group(child) => blurred_source(child, source),
                _ => false,
            })
    }
    fn connected_clip(group: &usvg::Group, source: &image::RgbaImage) -> bool {
        let vector_clip = group
            .clip_path()
            .is_some_and(|clip| clip.root().children().iter().any(|node| matches!(node, usvg::Node::Path(path) if !path.data().is_empty())));
        (vector_clip && blurred_source(group, source))
            || group.children().iter().any(|node| match node {
                usvg::Node::Group(child) => connected_clip(child, source),
                _ => false,
            })
    }
    assert!(
        connected_clip(tree.root(), &expected_pixels),
        "a non-empty vector clip contains the expected adjusted image under a Gaussian filter: {svg}"
    );

    let pdf = vectorcraft_format::base64_decode(s.execute("document.serialize", &json!({"format": "pdf"})).unwrap()["dataBase64"].as_str().unwrap())
        .unwrap();
    let imported = vectorcraft_pdf::import(&pdf).expect("independent Hayro-backed clipped-bitmap PDF read");
    assert_eq!(imported.artboards.len(), 1);
    let mut pdf_images = 0;
    imported.walk(|node| {
        if matches!(node.kind, NodeKind::Image(_)) {
            pdf_images += 1;
        }
    });
    assert!(pdf_images > 0, "the PDF reader reconstructs a real page and rendered raster composition, not live clip/effect semantics");
}
