//! CoralCraft: public warning-returning export API compatibility.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use corel_import::coralcraft_color::Color as CdrColor;
use corel_import::coralcraft_doc::{Document as CdrDocument, ObjectId, ObjectKind, ObjectNode};
use corel_import::coralcraft_geom::{F64, Point as CdrPoint, Rect as CdrRect, RectSpec};
use vectorcraft_color::Paint;
use vectorcraft_doc::{Appearance, Document, Node, NodeId, NodeKind, TextRun};
use vectorcraft_geom::{Rect, shapes};

type ExistingWriter = fn(&Document) -> Result<Vec<u8>, corel_import::CorelImportError>;
type WarningWriter = fn(&Document) -> Result<(Vec<u8>, Vec<String>), corel_import::CorelImportError>;

fn patterned_document() -> Document {
    let mut doc = Document::new(120.0, 120.0);
    let id = doc.alloc_id();
    let pattern = Paint::Pattern { pattern: "API Tile".into(), xf: Default::default() };
    let path = shapes::rectangle(Rect::new(10.0, 10.0, 100.0, 100.0));
    doc.insert(doc.default_layer(), 0, Node::path(id, path, Appearance::basic(pattern, Paint::None, 0.0))).unwrap();
    doc
}

#[test]
fn warning_returning_apis_match_legacy_wrappers() {
    let doc = patterned_document();
    let (converted, warnings) = corel_import::export_document_with_warnings(&doc).unwrap();
    assert_eq!(corel_import::export_document(&doc).unwrap(), converted);
    assert_eq!(warnings.iter().filter(|warning| warning.contains("generic Pattern")).count(), 1, "{warnings:?}");

    let writers: [(ExistingWriter, WarningWriter); 3] = [
        (corel_import::write_cdr7_document, corel_import::write_cdr7_document_with_warnings),
        (corel_import::write_cdrx_zip_document, corel_import::write_cdrx_zip_document_with_warnings),
        (corel_import::write_cmx1_document, corel_import::write_cmx1_document_with_warnings),
    ];
    for (existing, with_warnings) in writers {
        let expected = existing(&doc).unwrap();
        let (actual, warnings) = with_warnings(&doc).unwrap();
        assert_eq!(actual, expected);
        assert_eq!(warnings.iter().filter(|warning| warning.contains("generic Pattern")).count(), 1, "{warnings:?}");
    }
}

fn import_restorable(object: ObjectNode) -> Document {
    let mut cdr = CdrDocument::new_a4_portrait();
    cdr.pages[0].layers[0].objects.push(object);
    let (doc, warnings) = corel_import::import_document(&cdr).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    doc
}

fn restorable_id(doc: &Document, name: &str) -> NodeId {
    doc.layers
        .iter()
        .flat_map(|layer| layer.children().into_iter().flatten())
        .find(|node| node.name.as_deref() == Some(name))
        .map(|node| node.id)
        .unwrap()
}

fn pattern() -> Paint {
    Paint::Pattern { pattern: "Restored Tile".into(), xf: Default::default() }
}

#[test]
fn metadata_restoration_still_warns_for_new_pattern_and_live_trace() {
    let object = ObjectNode::new(
        ObjectId(41),
        Some("Restorable Path".into()),
        ObjectKind::Rect(RectSpec::sharp(CdrRect::new(10.0, 10.0, 100.0, 100.0).unwrap())),
        Some(corel_import::coralcraft_doc::FillStyle::uniform(CdrColor::BLACK)),
        None,
    );
    let mut doc = import_restorable(object);
    let id = restorable_id(&doc, "Restorable Path");
    let node = doc.node_mut(id).unwrap();
    node.appearance.set_fill(pattern());
    node.extras_mut().trace = Some(Box::new(serde_json::json!({"preset": "Custom", "params": {}, "view": "outlines"})));

    let (converted, warnings) = corel_import::export_document_with_warnings(&doc).unwrap();
    assert_eq!(warnings.iter().filter(|warning| warning.contains("generic Pattern")).count(), 1, "{warnings:?}");
    assert_eq!(warnings.iter().filter(|warning| warning.contains("live trace")).count(), 1, "{warnings:?}");
    assert!(warnings.iter().any(|warning| warning.contains("or dropped")));
    assert!(matches!(converted.pages[0].layers[0].objects[0].fill, Some(corel_import::coralcraft_doc::FillStyle::TwoColorPattern { .. })));
    assert_eq!(corel_import::export_document(&doc).unwrap(), converted);
}

#[test]
fn metadata_restoration_still_warns_for_later_text_run_paints() {
    let object = ObjectNode::new(
        ObjectId(42),
        Some("Restorable Text".into()),
        ObjectKind::ArtisticText {
            text: "AB".into(),
            font_family: "Source Sans 3".into(),
            size_pt: F64::new(24.0).unwrap(),
            position: CdrPoint::new(20.0, 60.0).unwrap(),
        },
        None,
        None,
    );
    let mut doc = import_restorable(object);
    let id = restorable_id(&doc, "Restorable Text");
    let NodeKind::Text(object) = &mut doc.node_mut(id).unwrap().kind else { panic!("text object") };
    object.runs[0].text = "A".into();
    let mut later = object.runs[0].style.clone();
    later.fill = pattern();
    later.stroke = pattern();
    later.stroke_width = 1.5;
    object.runs.push(TextRun::new("B", later));

    let (_, warnings) = corel_import::export_document_with_warnings(&doc).unwrap();
    assert_eq!(warnings.iter().filter(|warning| warning.contains("generic Pattern")).count(), 1, "{warnings:?}");
}

#[test]
fn metadata_restoration_still_warns_for_image_colorize() {
    let object = ObjectNode::new(
        ObjectId(43),
        Some("Restorable Image".into()),
        ObjectKind::Bitmap {
            pixel_width: 1,
            pixel_height: 1,
            dpi_x: F64::new(72.0).unwrap(),
            dpi_y: F64::new(72.0).unwrap(),
            rgba_pixels: vec![128, 128, 128, 255],
        },
        None,
        None,
    );
    let mut doc = import_restorable(object);
    let id = restorable_id(&doc, "Restorable Image");
    doc.node_mut(id).unwrap().extras_mut().colorize = Some(Box::new(pattern()));

    let (_, warnings) = corel_import::export_document_with_warnings(&doc).unwrap();
    assert_eq!(warnings.iter().filter(|warning| warning.contains("generic Pattern")).count(), 1, "{warnings:?}");
}

fn rect_object(id: u64, name: &str, rect: [f64; 4]) -> ObjectNode {
    ObjectNode::new(
        ObjectId(id),
        Some(name.to_owned()),
        ObjectKind::Rect(RectSpec::sharp(CdrRect::new(rect[0], rect[1], rect[2], rect[3]).unwrap())),
        Some(corel_import::coralcraft_doc::FillStyle::uniform(CdrColor::BLACK)),
        None,
    )
}

#[test]
fn restored_group_with_changed_child_count_still_warns_for_nested_pattern() {
    let group = ObjectNode::new(
        ObjectId(50),
        Some("Restorable Group".into()),
        ObjectKind::Group { children: vec![rect_object(51, "Big", [10.0, 10.0, 100.0, 100.0]), rect_object(52, "Small", [20.0, 20.0, 30.0, 30.0])] },
        None,
        None,
    );
    let mut doc = import_restorable(group);
    let id = restorable_id(&doc, "Restorable Group");
    let NodeKind::Group { children, .. } = &mut doc.node_mut(id).unwrap().kind else { panic!("group") };
    children.retain(|child| child.name.as_deref() != Some("Small"));
    assert_eq!(children.len(), 1);
    std::sync::Arc::make_mut(&mut children[0]).appearance.set_fill(pattern());

    for (existing, with_warnings) in [
        (corel_import::write_cdr7_document as ExistingWriter, corel_import::write_cdr7_document_with_warnings as WarningWriter),
        (corel_import::write_cmx1_document as ExistingWriter, corel_import::write_cmx1_document_with_warnings as WarningWriter),
    ] {
        let (bytes, warnings) = with_warnings(&doc).unwrap();
        assert_eq!(warnings.iter().filter(|warning| warning.contains("generic Pattern")).count(), 1, "{warnings:?}");
        assert_eq!(existing(&doc).unwrap(), bytes);
    }
}

#[test]
fn restored_powerclip_frame_pattern_and_trace_warn() {
    let mut frame = rect_object(60, "Frame", [10.0, 10.0, 100.0, 100.0]);
    frame.powerclip_children.push(rect_object(61, "Contents", [20.0, 20.0, 30.0, 30.0]));
    let mut doc = import_restorable(frame);
    let id = restorable_id(&doc, "Frame");
    let NodeKind::Group { children, .. } = &mut doc.node_mut(id).unwrap().kind else { panic!("PowerClip") };
    let frame = std::sync::Arc::make_mut(&mut children[0]);
    frame.appearance.set_fill(pattern());
    frame.extras_mut().trace = Some(Box::new(serde_json::json!({"preset": "Custom", "params": {}, "view": "outlines"})));

    for (existing, with_warnings) in [
        (corel_import::write_cdr7_document as ExistingWriter, corel_import::write_cdr7_document_with_warnings as WarningWriter),
        (corel_import::write_cmx1_document as ExistingWriter, corel_import::write_cmx1_document_with_warnings as WarningWriter),
    ] {
        let (bytes, warnings) = with_warnings(&doc).unwrap();
        assert_eq!(warnings.iter().filter(|warning| warning.contains("generic Pattern")).count(), 1, "{warnings:?}");
        assert_eq!(warnings.iter().filter(|warning| warning.contains("live trace")).count(), 1, "{warnings:?}");
        assert_eq!(existing(&doc).unwrap(), bytes);
    }
}

const GRADIENT_ALPHA_CATEGORY: &str = "gradient stop opacity";
const DROPPED_MASK_CATEGORY: &str = "enabled opacity masks";
const BITMAP_EFFECT_CATEGORY: &str = "unsupported bitmap effects";
const MISSING_IMAGE_CATEGORY: &str = "missing image pixels";
const DECODE_IMAGE_CATEGORY: &str = "undecodable or empty image pixels";

fn warning_count(warnings: &[String], category: &str) -> usize {
    warnings.iter().filter(|warning| warning.contains(category)).count()
}

fn cdr_cmx_writers() -> [(ExistingWriter, WarningWriter); 2] {
    [
        (corel_import::write_cdr7_document, corel_import::write_cdr7_document_with_warnings),
        (corel_import::write_cmx1_document, corel_import::write_cmx1_document_with_warnings),
    ]
}

fn gradient_paint(opacity: f32) -> Paint {
    let stops = vec![
        vectorcraft_color::GradientStop { opacity, ..vectorcraft_color::GradientStop::new(0.0, vectorcraft_color::Color::BLACK) },
        vectorcraft_color::GradientStop::new(1.0, vectorcraft_color::Color::WHITE),
    ];
    Paint::Gradient(Box::new(vectorcraft_color::GradientPaint::new(vectorcraft_color::Gradient::new(vectorcraft_color::GradientKind::Linear, stops))))
}

fn gradient_document(opacity: f32) -> Document {
    let mut doc = Document::new(120.0, 120.0);
    let id = doc.alloc_id();
    let path = shapes::rectangle(Rect::new(10.0, 10.0, 100.0, 100.0));
    doc.insert(doc.default_layer(), 0, Node::path(id, path, Appearance::basic(gradient_paint(opacity), gradient_paint(opacity), 1.0))).unwrap();
    doc
}

#[test]
fn visible_gradient_alpha_warns_once_and_all_opaque_paints_stay_silent() {
    let doc = gradient_document(0.25);
    for (_, with_warnings) in cdr_cmx_writers() {
        let (_, warnings) = with_warnings(&doc).unwrap();
        assert_eq!(warning_count(&warnings, GRADIENT_ALPHA_CATEGORY), 1, "{warnings:?}");
    }
    let Paint::Gradient(gradient) = doc.layers[0].children().unwrap()[0].as_ref().appearance.fill_paint() else { panic!("native gradient") };
    assert_eq!(gradient.gradient.stops[0].opacity, 0.25);

    let opaque = gradient_document(1.0);
    for (_, with_warnings) in cdr_cmx_writers() {
        let (_, warnings) = with_warnings(&opaque).unwrap();
        assert_eq!(warning_count(&warnings, GRADIENT_ALPHA_CATEGORY), 0, "{warnings:?}");
    }
}

fn text_object(id: u64, name: &str) -> ObjectNode {
    ObjectNode::new(
        ObjectId(id),
        Some(name.to_owned()),
        ObjectKind::ArtisticText {
            text: "AB".into(),
            font_family: "Source Sans 3".into(),
            size_pt: F64::new(24.0).unwrap(),
            position: CdrPoint::new(20.0, 60.0).unwrap(),
        },
        Some(corel_import::coralcraft_doc::FillStyle::uniform(CdrColor::BLACK)),
        None,
    )
}

#[test]
fn nested_restored_container_and_later_text_run_gradient_alpha_warn() {
    let group = ObjectNode::new(
        ObjectId(80),
        Some("Gradient Group".into()),
        ObjectKind::Group { children: vec![text_object(81, "Nested Text"), rect_object(82, "Deleted Sibling", [10.0, 10.0, 30.0, 30.0])] },
        None,
        None,
    );
    let mut doc = import_restorable(group);
    let group_id = restorable_id(&doc, "Gradient Group");
    let NodeKind::Group { children, .. } = &mut doc.node_mut(group_id).unwrap().kind else { panic!("group") };
    children.retain(|child| child.id != NodeId(82));
    let NodeKind::Text(text) = &mut doc.node_mut(NodeId(81)).unwrap().kind else { panic!("text") };
    let mut later = text.runs[0].style.clone();
    later.fill = gradient_paint(0.4);
    later.stroke = gradient_paint(0.6);
    text.runs.push(TextRun::new("B", later));

    for (_, with_warnings) in cdr_cmx_writers() {
        let (_, warnings) = with_warnings(&doc).unwrap();
        assert_eq!(warning_count(&warnings, GRADIENT_ALPHA_CATEGORY), 1, "{warnings:?}");
    }
}

fn only_object(doc: &CdrDocument) -> &ObjectNode {
    &doc.pages[0].layers[0].objects[0]
}

#[test]
fn unchanged_fountain_mask_restores_exactly_but_edited_mask_warns() {
    let mut original = rect_object(90, "Fountain Mask", [10.0, 10.0, 100.0, 100.0]);
    original.transparency = Some(corel_import::coralcraft_doc::TransparencyStyle::fountain_linear(45.0, 1.0, 0.2));
    let expected = original.transparency.clone();
    let mut unchanged = import_restorable(original.clone());
    let (converted, warnings) = corel_import::export_document_with_warnings(&unchanged).unwrap();
    assert_eq!(only_object(&converted).transparency, expected);
    assert!(warnings.is_empty(), "{warnings:?}");
    for (_, with_warnings) in cdr_cmx_writers() {
        let (_, warnings) = with_warnings(&unchanged).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
    }

    let id = restorable_id(&unchanged, "Fountain Mask");
    let node = unchanged.node_mut(id).unwrap();
    let mut extras = node.extras_mut();
    let mask = extras.mask.as_deref_mut().unwrap();
    std::sync::Arc::make_mut(&mut mask.art).appearance.set_fill(Paint::solid(vectorcraft_color::Color::rgb(1.0, 0.0, 0.0)));
    drop(extras);
    let (converted, warnings) = corel_import::export_document_with_warnings(&unchanged).unwrap();
    assert_eq!(warning_count(&warnings, DROPPED_MASK_CATEGORY), 1, "{warnings:?}");
    assert!(only_object(&converted).transparency.is_none());
    for (_, with_warnings) in cdr_cmx_writers() {
        let (_, warnings) = with_warnings(&unchanged).unwrap();
        assert_eq!(warning_count(&warnings, DROPPED_MASK_CATEGORY), 1, "{warnings:?}");
    }

    let mut disabled = import_restorable(original);
    let id = restorable_id(&disabled, "Fountain Mask");
    disabled.node_mut(id).unwrap().extras_mut().mask.as_deref_mut().unwrap().disabled = true;
    for (_, with_warnings) in cdr_cmx_writers() {
        let (_, warnings) = with_warnings(&disabled).unwrap();
        assert_eq!(warning_count(&warnings, DROPPED_MASK_CATEGORY), 0, "{warnings:?}");
    }
}

fn effect_document(ids: &[&str]) -> Document {
    let mut doc = Document::new(120.0, 120.0);
    let id = doc.alloc_id();
    let path = shapes::rectangle(Rect::new(10.0, 10.0, 100.0, 100.0));
    let mut appearance = Appearance::basic(Paint::solid(vectorcraft_color::Color::BLACK), Paint::None, 0.0);
    appearance.effects =
        ids.iter().map(|id| vectorcraft_doc::Effect { id: (*id).to_owned(), params: serde_json::json!({}), visible: true }).collect();
    appearance.effects.push(vectorcraft_doc::Effect { id: "blur.gaussian".to_owned(), params: serde_json::json!({}), visible: false });
    doc.insert(doc.default_layer(), 0, Node::path(id, path, appearance)).unwrap();
    doc
}

#[test]
fn unsupported_bitmap_effects_warn_once_with_sorted_deduplicated_ids() {
    let doc = effect_document(&["blur.gaussian", "adjust.brightnessContrast", "blur.gaussian"]);
    let native_effects = doc.layers[0].children().unwrap()[0].as_ref().appearance.effects.clone();
    for (_, with_warnings) in cdr_cmx_writers() {
        let (_, warnings) = with_warnings(&doc).unwrap();
        assert_eq!(warning_count(&warnings, BITMAP_EFFECT_CATEGORY), 1, "{warnings:?}");
        assert!(warnings.iter().any(|warning| warning.contains("adjust.brightnessContrast, blur.gaussian")), "{warnings:?}");
    }
    assert_eq!(doc.layers[0].children().unwrap()[0].as_ref().appearance.effects, native_effects);

    let supported =
        effect_document(&["stylize.dropShadow", "path.offsetPath", "threeD.extrude", "distort.puckerBloat", "distort.zigZag", "distort.twist"]);
    for (_, with_warnings) in cdr_cmx_writers() {
        let (_, warnings) = with_warnings(&supported).unwrap();
        assert_eq!(warning_count(&warnings, BITMAP_EFFECT_CATEGORY), 0, "{warnings:?}");
    }
}

#[test]
fn metadata_restoration_warns_for_edited_unsupported_bitmap_effects() {
    let mut doc = import_restorable(rect_object(91, "Edited Effects", [10.0, 10.0, 100.0, 100.0]));
    let id = restorable_id(&doc, "Edited Effects");
    doc.node_mut(id).unwrap().appearance.effects = vec![
        vectorcraft_doc::Effect { id: "blur.gaussian".into(), params: serde_json::json!({}), visible: true },
        vectorcraft_doc::Effect { id: "adjust.brightnessContrast".into(), params: serde_json::json!({}), visible: true },
    ];
    for (_, with_warnings) in cdr_cmx_writers() {
        let (_, warnings) = with_warnings(&doc).unwrap();
        assert_eq!(warning_count(&warnings, BITMAP_EFFECT_CATEGORY), 1, "{warnings:?}");
        assert!(warnings.iter().any(|warning| warning.contains("adjust.brightnessContrast, blur.gaussian")), "{warnings:?}");
    }
}

fn white_png() -> Vec<u8> {
    let image = image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 255, 255, 255]));
    let mut bytes = Vec::new();
    image.write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png).unwrap();
    bytes
}

fn image_document(bytes: Option<Vec<u8>>, width: u32, height: u32) -> Document {
    let mut doc = Document::new(120.0, 120.0);
    let id = doc.alloc_id();
    let key = "pixels".to_owned();
    if let Some(bytes) = bytes {
        doc.images.insert(key.clone(), vectorcraft_doc::ImageBlob::new("image/png", bytes));
    }
    let image = vectorcraft_doc::ImageObject { key, width, height, xf: Default::default(), link: None, placement: Default::default(), pixels: None };
    doc.insert(doc.default_layer(), 0, Node::new(id, NodeKind::Image(Box::new(image)))).unwrap();
    doc
}

fn assert_native_image_unchanged(doc: &Document, expected: Option<&[u8]>) {
    let NodeKind::Image(image) = &doc.layers[0].children().unwrap()[0].as_ref().kind else { panic!("image") };
    assert_eq!(image.key, "pixels");
    assert_eq!(doc.images.get("pixels").map(|blob| blob.bytes.as_slice()), expected);
}

#[test]
fn missing_corrupt_and_valid_white_images_have_independent_warning_categories() {
    let missing = image_document(None, 2, 2);
    let (converted, warnings) = corel_import::export_document_with_warnings(&missing).unwrap();
    assert_eq!(warning_count(&warnings, MISSING_IMAGE_CATEGORY), 1, "{warnings:?}");
    assert_eq!(warning_count(&warnings, DECODE_IMAGE_CATEGORY), 0, "{warnings:?}");
    let ObjectKind::Bitmap { rgba_pixels, .. } = &only_object(&converted).kind else { panic!("bitmap") };
    assert_eq!(rgba_pixels, &[255; 16]);
    assert_native_image_unchanged(&missing, None);

    let corrupt_bytes = vec![1, 2, 3, 4];
    let corrupt = image_document(Some(corrupt_bytes.clone()), 2, 2);
    let (converted, warnings) = corel_import::export_document_with_warnings(&corrupt).unwrap();
    assert_eq!(warning_count(&warnings, MISSING_IMAGE_CATEGORY), 0, "{warnings:?}");
    assert_eq!(warning_count(&warnings, DECODE_IMAGE_CATEGORY), 1, "{warnings:?}");
    let ObjectKind::Bitmap { rgba_pixels, .. } = &only_object(&converted).kind else { panic!("bitmap") };
    assert_eq!(rgba_pixels, &[255; 16]);
    assert_native_image_unchanged(&corrupt, Some(&corrupt_bytes));

    let valid_bytes = white_png();
    let valid = image_document(Some(valid_bytes.clone()), 2, 2);
    let (converted, warnings) = corel_import::export_document_with_warnings(&valid).unwrap();
    assert_eq!(warning_count(&warnings, MISSING_IMAGE_CATEGORY), 0, "{warnings:?}");
    assert_eq!(warning_count(&warnings, DECODE_IMAGE_CATEGORY), 0, "{warnings:?}");
    let ObjectKind::Bitmap { rgba_pixels, .. } = &only_object(&converted).kind else { panic!("bitmap") };
    assert_eq!(rgba_pixels, &[255; 16]);
    assert_native_image_unchanged(&valid, Some(&valid_bytes));

    for (_, with_warnings) in cdr_cmx_writers() {
        assert_eq!(warning_count(&with_warnings(&missing).unwrap().1, MISSING_IMAGE_CATEGORY), 1);
        assert_eq!(warning_count(&with_warnings(&missing).unwrap().1, DECODE_IMAGE_CATEGORY), 0);
        assert_eq!(warning_count(&with_warnings(&corrupt).unwrap().1, MISSING_IMAGE_CATEGORY), 0);
        assert_eq!(warning_count(&with_warnings(&corrupt).unwrap().1, DECODE_IMAGE_CATEGORY), 1);
        assert_eq!(warning_count(&with_warnings(&valid).unwrap().1, MISSING_IMAGE_CATEGORY), 0);
        assert_eq!(warning_count(&with_warnings(&valid).unwrap().1, DECODE_IMAGE_CATEGORY), 0);
    }
}

#[test]
fn huge_missing_image_returns_conversion_error_instead_of_allocating() {
    let doc = image_document(None, u32::MAX, u32::MAX);
    let error = corel_import::export_document_with_warnings(&doc).unwrap_err();
    assert!(matches!(error, corel_import::CorelImportError::Conversion(message) if message.contains("bitmap") && message.contains("64")));
    assert_native_image_unchanged(&doc, None);
}

#[test]
fn huge_original_bitmap_returns_conversion_error_instead_of_allocating() {
    let mut original = ObjectNode::new(
        ObjectId(92),
        Some("Huge Original".into()),
        ObjectKind::Bitmap {
            pixel_width: 1,
            pixel_height: 1,
            dpi_x: F64::new(72.0).unwrap(),
            dpi_y: F64::new(72.0).unwrap(),
            rgba_pixels: vec![255, 255, 255, 255],
        },
        None,
        None,
    );
    let mut doc = import_restorable(original.clone());
    let ObjectKind::Bitmap { pixel_width, pixel_height, rgba_pixels, .. } = &mut original.kind else { panic!("bitmap") };
    *pixel_width = u32::MAX;
    *pixel_height = u32::MAX;
    rgba_pixels.clear();
    let mut value = doc.unknown.get(corel_import::CORALCRAFT_UNKNOWN_KEY).unwrap().clone();
    let metadata = value["object_meta"].as_object_mut().unwrap().values_mut().next().unwrap();
    metadata["original"] = serde_json::to_value(&original).unwrap();
    doc.unknown.insert(corel_import::CORALCRAFT_UNKNOWN_KEY.to_owned(), value);

    let error = corel_import::export_document_with_warnings(&doc).unwrap_err();
    assert!(matches!(error, corel_import::CorelImportError::Conversion(message) if message.contains("bitmap") && message.contains("64")));
}

#[test]
fn huge_original_bitmap_legacy_reconstruction_returns_conversion_error_instead_of_allocating() {
    let mut original = ObjectNode::new(
        ObjectId(93),
        Some("Huge Legacy Original".into()),
        ObjectKind::Bitmap {
            pixel_width: 1,
            pixel_height: 1,
            dpi_x: F64::new(72.0).unwrap(),
            dpi_y: F64::new(72.0).unwrap(),
            rgba_pixels: vec![255, 255, 255, 255],
        },
        None,
        None,
    );
    let mut doc = import_restorable(original.clone());
    let ObjectKind::Bitmap { pixel_width, pixel_height, rgba_pixels, .. } = &mut original.kind else { panic!("bitmap") };
    *pixel_width = u32::MAX;
    *pixel_height = u32::MAX;
    rgba_pixels.clear();
    let mut value = doc.unknown.get(corel_import::CORALCRAFT_UNKNOWN_KEY).unwrap().clone();
    for metadata in value["object_meta"].as_object_mut().unwrap().values_mut() {
        metadata["original"] = serde_json::to_value(&original).unwrap();
        metadata.as_object_mut().unwrap().remove("original_vc");
    }
    value["extra"].as_object_mut().unwrap().retain(|key, _| !key.starts_with("corel.import.original_vc.v1."));
    doc.unknown.insert(corel_import::CORALCRAFT_UNKNOWN_KEY.to_owned(), value);

    let error = corel_import::export_document_with_warnings(&doc).unwrap_err();
    assert!(matches!(error, corel_import::CorelImportError::Conversion(message) if message.contains("bitmap") && message.contains("64")));
}

fn small_bitmap_object(id: u64, name: &str) -> ObjectNode {
    ObjectNode::new(
        ObjectId(id),
        Some(name.to_owned()),
        ObjectKind::Bitmap {
            pixel_width: 1,
            pixel_height: 1,
            dpi_x: F64::new(72.0).unwrap(),
            dpi_y: F64::new(72.0).unwrap(),
            rgba_pixels: vec![255, 255, 255, 255],
        },
        None,
        None,
    )
}

fn make_bitmap_huge(object: &mut ObjectNode) {
    let ObjectKind::Bitmap { pixel_width, pixel_height, rgba_pixels, .. } = &mut object.kind else { panic!("bitmap") };
    *pixel_width = u32::MAX;
    *pixel_height = u32::MAX;
    rgba_pixels.clear();
}

fn replace_original_tree_and_clear_sidecar(doc: &mut Document, original: &ObjectNode) {
    let mut value = doc.unknown.get(corel_import::CORALCRAFT_UNKNOWN_KEY).unwrap().clone();
    for metadata in value["object_meta"].as_object_mut().unwrap().values_mut() {
        metadata["original"] = serde_json::to_value(original).unwrap();
        metadata.as_object_mut().unwrap().remove("original_vc");
    }
    value["extra"].as_object_mut().unwrap().retain(|key, _| !key.starts_with("corel.import.original_vc.v1."));
    doc.unknown.insert(corel_import::CORALCRAFT_UNKNOWN_KEY.to_owned(), value);
}

fn assert_huge_nested_bitmap_error(doc: &Document) {
    let error = corel_import::export_document_with_warnings(doc).unwrap_err();
    assert!(matches!(error, corel_import::CorelImportError::Conversion(message) if message.contains("bitmap") && message.contains("64")));
}

#[test]
fn huge_nested_group_bitmap_legacy_reconstruction_returns_conversion_error_instead_of_allocating() {
    let mut original = ObjectNode::new(
        ObjectId(100),
        Some("Legacy Group".into()),
        ObjectKind::Group {
            children: vec![ObjectNode::new(
                ObjectId(101),
                Some("Nested Group".into()),
                ObjectKind::Group { children: vec![small_bitmap_object(102, "Deep Bitmap")] },
                None,
                None,
            )],
        },
        None,
        None,
    );
    let mut doc = import_restorable(original.clone());
    let ObjectKind::Group { children } = &mut original.kind else { panic!("outer group") };
    let ObjectKind::Group { children: nested } = &mut children[0].kind else { panic!("nested group") };
    make_bitmap_huge(&mut nested[0]);
    replace_original_tree_and_clear_sidecar(&mut doc, &original);

    assert_huge_nested_bitmap_error(&doc);
}

#[test]
fn huge_nested_powerclip_bitmap_legacy_reconstruction_returns_conversion_error_instead_of_allocating() {
    let mut original = rect_object(110, "Legacy PowerClip", [10.0, 10.0, 100.0, 100.0]);
    original.powerclip_children.push(ObjectNode::new(
        ObjectId(111),
        Some("PowerClip Group".into()),
        ObjectKind::Group { children: vec![small_bitmap_object(112, "PowerClip Bitmap")] },
        None,
        None,
    ));
    let mut doc = import_restorable(original.clone());
    let ObjectKind::Group { children } = &mut original.powerclip_children[0].kind else { panic!("PowerClip group") };
    make_bitmap_huge(&mut children[0]);
    replace_original_tree_and_clear_sidecar(&mut doc, &original);

    assert_huge_nested_bitmap_error(&doc);
}

const MESH_OPACITY_WARNING: &str = "mesh point opacity is dropped in CDR/CMX, so expanded patches are opaque; tessellated patch colours and positions are retained, and native mesh points remain editable";

fn mesh_document(opacity: f32) -> Document {
    let mut doc = Document::new(120.0, 120.0);
    let id = doc.alloc_id();
    let mut mesh =
        vectorcraft_doc::GradientMesh::from_surface(1, 1, &|u, v| vectorcraft_geom::Point::new(10.0 + 100.0 * u, 20.0 + 60.0 * v), &|_, _| {
            vectorcraft_color::Color::rgb8(40, 170, 90)
        });
    mesh.points[2].opacity = opacity;
    doc.insert(doc.default_layer(), 0, Node::new(id, NodeKind::Mesh(mesh))).unwrap();
    doc
}

fn path_points(path: &corel_import::coralcraft_geom::Path) -> Vec<CdrPoint> {
    let mut points = Vec::new();
    for element in &path.elements {
        match *element {
            corel_import::coralcraft_geom::PathElement::MoveTo(point) | corel_import::coralcraft_geom::PathElement::LineTo(point) => {
                points.push(point)
            }
            corel_import::coralcraft_geom::PathElement::QuadTo { ctrl, end } => points.extend([ctrl, end]),
            corel_import::coralcraft_geom::PathElement::CubicTo { c1, c2, end } => points.extend([c1, c2, end]),
            corel_import::coralcraft_geom::PathElement::Close => {}
        }
    }
    points
}

fn assert_opaque_tessellated_mesh(doc: &CdrDocument) {
    let root = only_object(doc);
    assert_eq!(root.opacity.get(), 1.0);
    let ObjectKind::Group { children } = &root.kind else { panic!("expanded mesh group") };
    assert!(!children.is_empty());
    let expected_fill = Some(corel_import::coralcraft_doc::FillStyle::uniform(CdrColor::rgb8(40, 170, 90)));
    let mut points = Vec::new();
    for child in children {
        assert_eq!(child.opacity.get(), 1.0);
        assert_eq!(child.fill, expected_fill);
        let ObjectKind::Path(path) = &child.kind else { panic!("ordinary tessellated path") };
        assert!(!path.elements.is_empty());
        points.extend(path_points(path));
    }
    assert!(!points.is_empty());
    let x0 = points.iter().map(|point| point.x.get()).fold(f64::INFINITY, f64::min);
    let x1 = points.iter().map(|point| point.x.get()).fold(f64::NEG_INFINITY, f64::max);
    let y0 = points.iter().map(|point| point.y.get()).fold(f64::INFINITY, f64::min);
    let y1 = points.iter().map(|point| point.y.get()).fold(f64::NEG_INFINITY, f64::max);
    assert!((x0 - 10.0).abs() < 1e-6 && (x1 - 110.0).abs() < 1e-6, "retained x coordinates: {x0}..{x1}");
    assert!((y0 - 20.0).abs() < 1e-6 && (y1 - 80.0).abs() < 1e-6, "retained y coordinates: {y0}..{y1}");
}

#[test]
fn mesh_point_opacity_warns_once_and_exports_opaque_tessellated_paths() {
    let doc = mesh_document(0.25);
    let native = doc.layers[0].children().unwrap()[0].clone();
    let (converted, warnings) = corel_import::export_document_with_warnings(&doc).unwrap();
    assert_eq!(warnings.iter().filter(|warning| warning.as_str() == MESH_OPACITY_WARNING).count(), 1, "{warnings:?}");
    assert_opaque_tessellated_mesh(&converted);
    assert_eq!(doc.layers[0].children().unwrap()[0], native);

    for (existing, with_warnings) in cdr_cmx_writers() {
        let (bytes, warnings) = with_warnings(&doc).unwrap();
        assert_eq!(warnings.iter().filter(|warning| warning.as_str() == MESH_OPACITY_WARNING).count(), 1, "{warnings:?}");
        assert_eq!(existing(&doc).unwrap(), bytes);
    }
}

#[test]
fn all_opaque_mesh_stays_silent() {
    let doc = mesh_document(1.0);
    let (converted, warnings) = corel_import::export_document_with_warnings(&doc).unwrap();
    assert!(!warnings.iter().any(|warning| warning.as_str() == MESH_OPACITY_WARNING), "{warnings:?}");
    assert_opaque_tessellated_mesh(&converted);
    for (_, with_warnings) in cdr_cmx_writers() {
        let (_, warnings) = with_warnings(&doc).unwrap();
        assert!(!warnings.iter().any(|warning| warning.as_str() == MESH_OPACITY_WARNING), "{warnings:?}");
    }
}

#[test]
fn exact_mesh_restoration_stays_silent() {
    let original = ObjectNode::new(
        ObjectId(120),
        Some("Restored Mesh Fallback".into()),
        ObjectKind::Group { children: vec![rect_object(121, "Original Patch", [10.0, 20.0, 110.0, 80.0])] },
        None,
        None,
    );
    let mut doc = import_restorable(original);
    let id = restorable_id(&doc, "Restored Mesh Fallback");
    let mesh_source = mesh_document(0.25);
    let NodeKind::Mesh(mesh) = &mesh_source.layers[0].children().unwrap()[0].kind else { panic!("mesh") };
    doc.node_mut(id).unwrap().kind = NodeKind::Mesh(mesh.clone());
    let expanded = vectorcraft_doc::live::expand_deep(doc.node(id).unwrap(), None);
    let mut value = doc.unknown.get(corel_import::CORALCRAFT_UNKNOWN_KEY).unwrap().clone();
    value["extra"].as_object_mut().unwrap().insert(format!("corel.import.original_vc.v1.{}", id.0), serde_json::to_value(expanded).unwrap());
    doc.unknown.insert(corel_import::CORALCRAFT_UNKNOWN_KEY.to_owned(), value);

    let (_, warnings) = corel_import::export_document_with_warnings(&doc).unwrap();
    assert!(!warnings.iter().any(|warning| warning.as_str() == MESH_OPACITY_WARNING), "{warnings:?}");
}

fn invalid_mesh(id: u64) -> Node {
    Node::new(NodeId(id), NodeKind::Mesh(vectorcraft_doc::GradientMesh { rows: 1, cols: 1, points: Vec::new() }))
}

fn invalid_mesh_document(nested: bool) -> Document {
    let mut doc = Document::new(120.0, 120.0);
    let id = doc.alloc_id();
    let child = invalid_mesh(doc.alloc_id().0);
    let root = if nested {
        Node::new(id, NodeKind::Blend { children: vec![std::sync::Arc::new(child)], spec: Default::default() })
    } else {
        Node::new(id, child.kind)
    };
    doc.insert(doc.default_layer(), 0, root).unwrap();
    doc
}

fn assert_empty_live_conversion_error<T>(result: Result<T, corel_import::CorelImportError>) {
    assert!(matches!(result, Err(corel_import::CorelImportError::Conversion(message)) if message.contains("live") && message.contains("empty")));
}

#[test]
fn empty_live_expansion_fails_closed_through_every_public_export_and_writer() {
    let doc = invalid_mesh_document(false);
    assert_empty_live_conversion_error(corel_import::export_document(&doc));
    assert_empty_live_conversion_error(corel_import::export_document_with_warnings(&doc));
    assert_empty_live_conversion_error(corel_import::write_cdr7_document(&doc));
    assert_empty_live_conversion_error(corel_import::write_cdr7_document_with_warnings(&doc));
    assert_empty_live_conversion_error(corel_import::write_cdrx_zip_document(&doc));
    assert_empty_live_conversion_error(corel_import::write_cdrx_zip_document_with_warnings(&doc));
    assert_empty_live_conversion_error(corel_import::write_cmx1_document(&doc));
    assert_empty_live_conversion_error(corel_import::write_cmx1_document_with_warnings(&doc));
}

#[test]
fn nested_empty_live_child_fails_closed_instead_of_exporting_an_empty_object() {
    let doc = invalid_mesh_document(true);
    assert_empty_live_conversion_error(corel_import::export_document(&doc));
    for (existing, with_warnings) in cdr_cmx_writers() {
        assert_empty_live_conversion_error(existing(&doc));
        assert_empty_live_conversion_error(with_warnings(&doc));
    }
}

#[test]
fn nested_exact_mesh_restoration_stays_silent_when_parent_restoration_fails() {
    let mut nested_original = ObjectNode::new(
        ObjectId(121),
        Some("Nested Restored Mesh".into()),
        ObjectKind::Group { children: vec![rect_object(122, "Original Nested Patch", [10.0, 20.0, 110.0, 80.0])] },
        None,
        None,
    );
    nested_original.opacity = F64::new(0.37).unwrap();
    let parent = ObjectNode::new(
        ObjectId(120),
        Some("Original Parent".into()),
        ObjectKind::Group { children: vec![nested_original.clone(), rect_object(123, "Sibling", [120.0, 20.0, 130.0, 80.0])] },
        None,
        None,
    );
    let mut doc = import_restorable(parent);
    let parent_id = restorable_id(&doc, "Original Parent");
    let nested_id =
        doc.node(parent_id).unwrap().children().unwrap().iter().find(|child| child.name.as_deref() == Some("Nested Restored Mesh")).unwrap().id;
    doc.node_mut(parent_id).unwrap().name = Some("Changed Parent".into());

    let mesh_source = mesh_document(0.25);
    let NodeKind::Mesh(mesh) = &mesh_source.layers[0].children().unwrap()[0].kind else { panic!("mesh") };
    let nested = doc.node_mut(nested_id).unwrap();
    nested.kind = NodeKind::Mesh(mesh.clone());
    nested.opacity = 1.0;
    let expanded = vectorcraft_doc::live::expand_deep(doc.node(nested_id).unwrap(), None);
    let key = format!("corel.import.original_vc.v1.{}", parent_id.0);
    let mut value = doc.unknown.get(corel_import::CORALCRAFT_UNKNOWN_KEY).unwrap().clone();
    let mut projection: Node = serde_json::from_value(value["extra"][&key].clone()).unwrap();
    let NodeKind::Group { children, .. } = &mut projection.kind else { panic!("parent projection") };
    let nested_projection = children.iter_mut().find(|child| child.id == nested_id).unwrap();
    *nested_projection = expanded.into();
    value["extra"].as_object_mut().unwrap().insert(key, serde_json::to_value(projection).unwrap());
    doc.unknown.insert(corel_import::CORALCRAFT_UNKNOWN_KEY.to_owned(), value);

    let (converted, warnings) = corel_import::export_document_with_warnings(&doc).unwrap();
    let ObjectKind::Group { children } = &only_object(&converted).kind else { panic!("converted parent group") };
    let nested = children.iter().find(|child| child.name.as_deref() == Some("Nested Restored Mesh")).unwrap();
    assert_eq!(nested.opacity.get(), 0.37, "the nested original shell restored instead of using current opacity 1.0");
    assert!(matches!(&nested.kind, ObjectKind::Group { children } if !children.is_empty()));
    assert!(!warnings.iter().any(|warning| warning.as_str() == MESH_OPACITY_WARNING), "{warnings:?}");
}

#[test]
fn nested_exact_mesh_restoration_stays_silent_when_live_parent_restoration_fails() {
    let mut nested_original = ObjectNode::new(
        ObjectId(131),
        Some("Nested Live Restored Mesh".into()),
        ObjectKind::Group { children: vec![rect_object(132, "Original Live Nested Patch", [10.0, 20.0, 110.0, 80.0])] },
        None,
        None,
    );
    nested_original.opacity = F64::new(0.37).unwrap();
    let parent = ObjectNode::new(
        ObjectId(130),
        Some("Original Live Parent".into()),
        ObjectKind::Group { children: vec![nested_original.clone()] },
        None,
        None,
    );
    let mut doc = import_restorable(parent);
    let parent_id = restorable_id(&doc, "Original Live Parent");
    let nested_id =
        doc.node(parent_id).unwrap().children().unwrap().iter().find(|child| child.name.as_deref() == Some("Nested Live Restored Mesh")).unwrap().id;

    let mesh_source = mesh_document(0.25);
    let NodeKind::Mesh(mesh) = &mesh_source.layers[0].children().unwrap()[0].kind else { panic!("mesh") };
    let nested = doc.node_mut(nested_id).unwrap();
    nested.kind = NodeKind::Mesh(mesh.clone());
    nested.opacity = 1.0;
    let nested = doc.node(nested_id).unwrap().clone();
    doc.node_mut(parent_id).unwrap().kind = NodeKind::Blend { children: vec![std::sync::Arc::new(nested)], spec: Default::default() };
    let expanded = vectorcraft_doc::live::expand_deep(doc.node(parent_id).unwrap(), None);
    let key = format!("corel.import.original_vc.v1.{}", parent_id.0);
    let mut value = doc.unknown.get(corel_import::CORALCRAFT_UNKNOWN_KEY).unwrap().clone();
    value["extra"].as_object_mut().unwrap().insert(key, serde_json::to_value(expanded).unwrap());
    doc.unknown.insert(corel_import::CORALCRAFT_UNKNOWN_KEY.to_owned(), value);
    doc.node_mut(parent_id).unwrap().name = Some("Changed Live Parent".into());

    let (converted, warnings) = corel_import::export_document_with_warnings(&doc).unwrap();
    let ObjectKind::Group { children } = &only_object(&converted).kind else { panic!("converted live parent group") };
    let nested = children.iter().find(|child| child.name.as_deref() == Some("Nested Live Restored Mesh")).unwrap();
    assert_eq!(nested.opacity.get(), 0.37, "the nested original shell restored instead of using current opacity 1.0");
    assert!(matches!(&nested.kind, ObjectKind::Group { children } if !children.is_empty()));
    assert!(!warnings.iter().any(|warning| warning.as_str() == MESH_OPACITY_WARNING), "{warnings:?}");
}

fn mesh_mask_document(name: &str, force_fallback: bool) -> (Document, corel_import::coralcraft_doc::TransparencyStyle) {
    let mut original = rect_object(140, name, [10.0, 10.0, 100.0, 100.0]);
    original.transparency = Some(corel_import::coralcraft_doc::TransparencyStyle::fountain_linear(45.0, 1.0, 0.2));
    let expected = original.transparency.clone().unwrap();
    let mut doc = import_restorable(original);
    let id = restorable_id(&doc, name);
    let mesh_source = mesh_document(0.25);
    let NodeKind::Mesh(mesh) = &mesh_source.layers[0].children().unwrap()[0].kind else { panic!("mesh") };
    let owner = doc.node_mut(id).unwrap();
    let mut extras = owner.extras_mut();
    let mask = extras.mask.as_deref_mut().unwrap();
    std::sync::Arc::make_mut(&mut mask.art).kind = NodeKind::Mesh(mesh.clone());
    drop(extras);

    let key = format!("corel.import.original_vc.v1.{}", id.0);
    let projection = doc.node(id).unwrap().clone();
    let mut value = doc.unknown.get(corel_import::CORALCRAFT_UNKNOWN_KEY).unwrap().clone();
    value["extra"].as_object_mut().unwrap().insert(key, serde_json::to_value(projection).unwrap());
    doc.unknown.insert(corel_import::CORALCRAFT_UNKNOWN_KEY.to_owned(), value);
    if force_fallback {
        doc.node_mut(id).unwrap().name = Some("Changed Mask Owner".into());
    }
    (doc, expected)
}

#[test]
fn exact_restored_owner_with_translucent_mesh_mask_stays_silent() {
    let (doc, expected) = mesh_mask_document("Restored Mesh Mask", false);
    let (converted, warnings) = corel_import::export_document_with_warnings(&doc).unwrap();
    assert_eq!(only_object(&converted).transparency, Some(expected));
    assert!(!warnings.iter().any(|warning| warning.as_str() == MESH_OPACITY_WARNING), "{warnings:?}");
}

#[test]
fn fallback_owner_with_translucent_mesh_mask_warns_once() {
    let (doc, _) = mesh_mask_document("Fallback Mesh Mask", true);
    let (converted, warnings) = corel_import::export_document_with_warnings(&doc).unwrap();
    assert!(only_object(&converted).transparency.is_none());
    assert_eq!(warnings.iter().filter(|warning| warning.as_str() == MESH_OPACITY_WARNING).count(), 1, "{warnings:?}");
    assert_eq!(warning_count(&warnings, DROPPED_MASK_CATEGORY), 1, "{warnings:?}");
}

// CoralCraft (P3 mesh review): generated meshes — the copies a Repeat makes and the intermediates
// a multi-source Blend lerps — carry their point opacity into the CDR/CMX fallback too, and the
// node whose expansion generated them owns the warning. Attribution comes from the live tree and
// the expansion outcome together, never from generated node ids alone.
fn translucent_mesh_node(id: u64) -> Node {
    let mut mesh =
        vectorcraft_doc::GradientMesh::from_surface(1, 1, &|u, v| vectorcraft_geom::Point::new(10.0 + 100.0 * u, 20.0 + 60.0 * v), &|_, _| {
            vectorcraft_color::Color::rgb8(40, 170, 90)
        });
    mesh.points[2].opacity = 0.25;
    Node::new(NodeId(id), NodeKind::Mesh(mesh))
}

fn mesh_warning_count(warnings: &[String]) -> usize {
    warnings.iter().filter(|warning| warning.as_str() == MESH_OPACITY_WARNING).count()
}

/// A Repeat of a translucent mesh: every generated copy tessellates opaque, so the Repeat's own
/// fallback warns exactly once (the generated copies carry `NodeId(0)`, so original-id attribution
/// alone can never see them).
#[test]
fn translucent_mesh_repeat_warns_exactly_once() {
    let mut doc = Document::new(400.0, 200.0);
    let id = doc.alloc_id();
    let mesh_id = doc.alloc_id().0;
    let repeat = vectorcraft_doc::RepeatSpec::mirror(vec![std::sync::Arc::new(translucent_mesh_node(mesh_id))], 90.0, 20.0);
    doc.insert(doc.default_layer(), 0, Node::new(id, NodeKind::Repeat(repeat))).unwrap();
    let (converted, warnings) = corel_import::export_document_with_warnings(&doc).unwrap();
    assert_eq!(mesh_warning_count(&warnings), 1, "{warnings:?}");
    // The exported art is the expanded, opaque tessellation of the mirrored copies.
    let root = only_object(&converted);
    assert_eq!(root.opacity.get(), 1.0);
    let ObjectKind::Group { children } = &root.kind else { panic!("expanded repeat group") };
    assert!(children.len() > 1, "the mirrored copies exported: {}", children.len());
}

/// A multi-source Blend of two meshes with lerped translucent intermediates: the blend's fallback
/// warns exactly once even though neither source id is the blend's own.
#[test]
fn multi_source_blend_with_translucent_intermediates_warns_once() {
    let mut doc = Document::new(400.0, 200.0);
    let id = doc.alloc_id();
    let a = translucent_mesh_node(doc.alloc_id().0);
    let mut b_node = translucent_mesh_node(doc.alloc_id().0);
    if let NodeKind::Mesh(mesh) = &mut b_node.kind {
        for point in mesh.points.iter_mut() {
            point.p.x += 160.0;
        }
    }
    doc.insert(
        doc.default_layer(),
        0,
        Node::new(id, NodeKind::Blend { children: vec![std::sync::Arc::new(a), std::sync::Arc::new(b_node)], spec: Default::default() }),
    )
    .unwrap();
    let (converted, warnings) = corel_import::export_document_with_warnings(&doc).unwrap();
    assert_eq!(mesh_warning_count(&warnings), 1, "{warnings:?}");
    let root = only_object(&converted);
    let ObjectKind::Group { children } = &root.kind else { panic!("expanded blend group") };
    assert!(!children.is_empty());
}

/// A plain group around a translucent mesh adds no warning of its own: the child's own conversion
/// owns the single warning (the control that keeps per-node attribution from regressing into
/// parent proxy warnings).
#[test]
fn plain_group_over_translucent_mesh_leaves_the_single_child_warning() {
    let mut doc = Document::new(200.0, 200.0);
    let group = doc.alloc_id();
    let mesh_id = doc.alloc_id();
    doc.insert(
        doc.default_layer(),
        0,
        Node::new(group, NodeKind::Group { children: vec![std::sync::Arc::new(translucent_mesh_node(mesh_id.0))], clip: false }),
    )
    .unwrap();
    let (converted, warnings) = corel_import::export_document_with_warnings(&doc).unwrap();
    assert_eq!(mesh_warning_count(&warnings), 1, "{warnings:?}");
    assert!(matches!(&only_object(&converted).kind, ObjectKind::Group { .. }), "the group exported");
}

/// CoralCraft (P3 mesh review): the review's exact Blend scenario — the two source objects
/// restore exactly (their own conversions stay silent), while the generated intermediate meshes
/// have no original to restore, fall back, and the blend's own evaluation warns exactly once.
#[test]
fn blend_with_restored_sources_still_warns_for_generated_intermediates() {
    let mut doc = Document::new(400.0, 200.0);
    let blend_id = doc.alloc_id();
    let a_id = doc.alloc_id().0;
    let b_id = doc.alloc_id().0;
    let a = translucent_mesh_node(a_id);
    let mut b_node = translucent_mesh_node(b_id);
    if let NodeKind::Mesh(mesh) = &mut b_node.kind {
        for point in mesh.points.iter_mut() {
            point.p.x += 160.0;
        }
    }
    let blend =
        Node::new(blend_id, NodeKind::Blend { children: vec![std::sync::Arc::new(a), std::sync::Arc::new(b_node)], spec: Default::default() });
    doc.insert(doc.default_layer(), 0, blend).unwrap();
    // Store the current expansion as the expected original, exactly as an imported document
    // would: the sources' ids match, so they restore; the generated intermediates have none.
    let projection = vectorcraft_doc::live::expand_deep(doc.node(blend_id).unwrap(), None);
    let key = format!("corel.import.original_vc.v1.{}", blend_id.0);
    let mut value = doc.unknown.get(corel_import::CORALCRAFT_UNKNOWN_KEY).cloned().unwrap_or_else(|| serde_json::json!({}));
    value["extra"] = serde_json::json!({});
    value["extra"].as_object_mut().unwrap().insert(key, serde_json::to_value(projection).unwrap());
    doc.unknown.insert(corel_import::CORALCRAFT_UNKNOWN_KEY.to_owned(), value);

    let (converted, warnings) = corel_import::export_document_with_warnings(&doc).unwrap();
    assert_eq!(mesh_warning_count(&warnings), 1, "the generated intermediates fell back and dropped opacity: {warnings:?}");
    let root = only_object(&converted);
    let ObjectKind::Group { children } = &root.kind else { panic!("expanded blend group") };
    assert!(!children.is_empty(), "the blend exported its art");
}
