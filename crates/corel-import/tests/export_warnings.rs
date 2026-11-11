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
    assert_eq!(warning_count(&warnings, DROPPED_MASK_CATEGORY), 0, "{warnings:?}");
    for (_, with_warnings) in cdr_cmx_writers() {
        let (_, warnings) = with_warnings(&unchanged).unwrap();
        assert_eq!(warning_count(&warnings, DROPPED_MASK_CATEGORY), 0, "{warnings:?}");
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
