//! CoralCraft: fail-closed CDR metadata restoration after native edits.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::io::Cursor;
use std::sync::Arc;

use corel_import::coralcraft_color::{Color as CdrColor, FountainBlendMode, FountainStop};
use corel_import::coralcraft_doc::{
    ChunkTag, Document as CdrDocument, FillStyle, FountainGeometry, ObjectId, ObjectKind, ObjectNode, RawChunk, RawPayload, Residual,
    TransparencyStyle, UnsupportedObject, VectorEffect,
};
use corel_import::coralcraft_geom::{F64, Point as CdrPoint, Rect as CdrRect, RectSpec};
use vectorcraft_color::{BlendMode, Color as VcColor, Paint};
use vectorcraft_doc::{Document, ImageBlob, NodeId, NodeKind, TextRun};
use vectorcraft_geom::{Point, Rect, shapes};

fn import_restorable(object: ObjectNode) -> Document {
    let mut cdr = CdrDocument::new_a4_portrait();
    cdr.pages[0].layers[0].objects.push(object);
    import_cdr(cdr)
}

fn import_cdr(cdr: CdrDocument) -> Document {
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

fn rect_object(id: u64, name: &str, rect: [f64; 4]) -> ObjectNode {
    ObjectNode::new(
        ObjectId(id),
        Some(name.to_owned()),
        ObjectKind::Rect(RectSpec::sharp(CdrRect::new(rect[0], rect[1], rect[2], rect[3]).unwrap())),
        Some(FillStyle::uniform(CdrColor::BLACK)),
        None,
    )
}

fn text_object(id: u64, name: &str, text: &str) -> ObjectNode {
    ObjectNode::new(
        ObjectId(id),
        Some(name.to_owned()),
        ObjectKind::ArtisticText {
            text: text.to_owned(),
            font_family: "Source Sans 3".into(),
            size_pt: F64::new(24.0).unwrap(),
            position: CdrPoint::new(20.0, 60.0).unwrap(),
        },
        Some(FillStyle::uniform(CdrColor::BLACK)),
        None,
    )
}

fn bitmap_object(id: u64, name: &str, rgba: [u8; 4]) -> ObjectNode {
    ObjectNode::new(
        ObjectId(id),
        Some(name.to_owned()),
        ObjectKind::Bitmap {
            pixel_width: 1,
            pixel_height: 1,
            dpi_x: F64::new(72.0).unwrap(),
            dpi_y: F64::new(72.0).unwrap(),
            rgba_pixels: rgba.to_vec(),
        },
        None,
        None,
    )
}

fn only_object(doc: &CdrDocument) -> &ObjectNode {
    &doc.pages[0].layers[0].objects[0]
}

fn red_fill() -> Paint {
    Paint::solid(VcColor::rgb(1.0, 0.0, 0.0))
}

fn assert_red(object: &ObjectNode) {
    assert_eq!(object.fill, Some(FillStyle::uniform(CdrColor::rgb8(255, 0, 0))));
}

fn png(rgba: [u8; 4]) -> Vec<u8> {
    let image = image::RgbaImage::from_pixel(1, 1, image::Rgba(rgba));
    let mut bytes = Vec::new();
    image.write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png).unwrap();
    bytes
}

fn clear_original_vc(doc: &mut Document) {
    let mut value = doc.unknown.get(corel_import::CORALCRAFT_UNKNOWN_KEY).unwrap().clone();
    for metadata in value["object_meta"].as_object_mut().unwrap().values_mut() {
        metadata.as_object_mut().unwrap().remove("original_vc");
    }
    doc.unknown.insert(corel_import::CORALCRAFT_UNKNOWN_KEY.to_owned(), value);
}

fn group_children(object: &ObjectNode) -> &[ObjectNode] {
    match &object.kind {
        ObjectKind::Group { children } => children,
        ObjectKind::Unsupported(unsupported) => &unsupported.children,
        other => panic!("expected container, got {other:?}"),
    }
}

#[test]
fn same_bounds_solid_fill_edit_exports_the_current_color() {
    let mut doc = import_restorable(rect_object(41, "Rectangle", [10.0, 10.0, 100.0, 100.0]));
    let id = restorable_id(&doc, "Rectangle");
    doc.node_mut(id).unwrap().appearance.set_fill(red_fill());

    let exported = corel_import::export_document(&doc).unwrap();
    assert_red(only_object(&exported));
}

#[test]
fn same_bounds_path_geometry_edit_exports_the_current_path() {
    let mut doc = import_restorable(rect_object(42, "Rectangle", [10.0, 10.0, 100.0, 100.0]));
    let id = restorable_id(&doc, "Rectangle");
    let NodeKind::Path { path, live, .. } = &mut doc.node_mut(id).unwrap().kind else { panic!("path") };
    *path = shapes::ellipse(Rect::new(10.0, 10.0, 100.0, 100.0));
    *live = None;

    let exported = corel_import::export_document(&doc).unwrap();
    assert!(matches!(only_object(&exported).kind, ObjectKind::Path(_)));
}

#[test]
fn gradient_stop_opacity_edit_does_not_restore_old_colors_or_claim_alpha() {
    let mut object = rect_object(43, "Gradient", [10.0, 10.0, 100.0, 100.0]);
    object.fill = Some(FillStyle::Fountain {
        geometry: FountainGeometry::Linear { angle_deg: F64::ZERO, edge_pad: F64::ZERO },
        blend_mode: FountainBlendMode::Direct,
        stops: vec![FountainStop::new(0.0, CdrColor::BLACK).unwrap(), FountainStop::new(1.0, CdrColor::WHITE).unwrap()],
    });
    let mut doc = import_restorable(object);
    let id = restorable_id(&doc, "Gradient");
    let node = doc.node_mut(id).unwrap();
    let gradient = node
        .appearance
        .items
        .iter_mut()
        .find_map(|item| match item.paint_mut() {
            Paint::Gradient(gradient) => Some(gradient),
            _ => None,
        })
        .unwrap();
    gradient.gradient.stops[0].color = VcColor::rgb(1.0, 0.0, 0.0);
    gradient.gradient.stops[0].opacity = 0.25;

    let exported = corel_import::export_document(&doc).unwrap();
    let Some(FillStyle::Fountain { stops, .. }) = &only_object(&exported).fill else { panic!("fountain") };
    assert_eq!(stops[0].color, CdrColor::rgb8(255, 0, 0));
}

#[test]
fn text_content_and_first_run_style_edits_export_current_text() {
    let mut doc = import_restorable(text_object(44, "Text", "AB"));
    let id = restorable_id(&doc, "Text");
    let NodeKind::Text(text) = &mut doc.node_mut(id).unwrap().kind else { panic!("text") };
    text.runs[0].text = "XY".into();
    text.runs[0].style.fill = red_fill();

    let exported = corel_import::export_document(&doc).unwrap();
    let ObjectKind::ArtisticText { text, .. } = &only_object(&exported).kind else { panic!("artistic text") };
    assert_eq!(text, "XY");
    assert_red(only_object(&exported));
}

#[test]
fn later_run_style_edit_does_not_restore_unknown_original_fill() {
    let mut original = text_object(45, "Text", "AB");
    original.fill = Some(FillStyle::Unknown { tag: ChunkTag(*b"unkn"), payload: RawPayload(vec![1, 2, 3, 4]) });
    let mut doc = import_restorable(original);
    let id = restorable_id(&doc, "Text");
    let NodeKind::Text(text) = &mut doc.node_mut(id).unwrap().kind else { panic!("text") };
    let mut later = text.runs[0].style.clone();
    later.fill = red_fill();
    later.stroke = red_fill();
    text.runs.push(TextRun::new("", later));

    let exported = corel_import::export_document(&doc).unwrap();
    let ObjectKind::ArtisticText { text, .. } = &only_object(&exported).kind else { panic!("artistic text") };
    assert_eq!(text, "AB");
    assert_eq!(only_object(&exported).fill, Some(FillStyle::uniform(CdrColor::gray(0.5).unwrap())));
}

#[test]
fn text_layout_caches_alone_keep_exact_native_restoration() {
    let mut original = text_object(46, "Text", "AB");
    original.fill = Some(FillStyle::Unknown { tag: ChunkTag(*b"unkn"), payload: RawPayload(vec![1, 2, 3, 4]) });
    let expected = original.clone();
    let mut doc = import_restorable(original);
    let id = restorable_id(&doc, "Text");
    let NodeKind::Text(text) = &mut doc.node_mut(id).unwrap().kind else { panic!("text") };
    text.cached_bounds = Some(Rect::new(0.0, 0.0, 50.0, 20.0));
    text.cached_baselines = vec![(Point::new(0.0, 0.0), Point::new(50.0, 0.0))];

    let exported = corel_import::export_document(&doc).unwrap();
    assert_eq!(only_object(&exported), &expected);
}

#[test]
fn image_replacement_with_new_bytes_and_key_exports_current_pixels() {
    let mut doc = import_restorable(bitmap_object(47, "Image", [128, 128, 128, 255]));
    let id = restorable_id(&doc, "Image");
    let NodeKind::Image(image) = &mut doc.node_mut(id).unwrap().kind else { panic!("image") };
    image.key = "replacement".into();
    doc.images.insert("replacement".into(), ImageBlob::png(png([255, 0, 0, 255])));

    let exported = corel_import::export_document(&doc).unwrap();
    let ObjectKind::Bitmap { rgba_pixels, .. } = &only_object(&exported).kind else { panic!("bitmap") };
    assert_eq!(rgba_pixels, &[255, 0, 0, 255]);
}

#[test]
fn same_key_image_byte_mutation_fails_closed() {
    let mut doc = import_restorable(bitmap_object(48, "Image", [128, 128, 128, 255]));
    let id = restorable_id(&doc, "Image");
    let NodeKind::Image(image) = &doc.node(id).unwrap().kind else { panic!("image") };
    let key = image.key.clone();
    Arc::make_mut(&mut doc.images.get_mut(&key).unwrap().bytes).copy_from_slice(&png([255, 0, 0, 255]));

    let exported = corel_import::export_document(&doc).unwrap();
    let ObjectKind::Bitmap { rgba_pixels, .. } = &only_object(&exported).kind else { panic!("bitmap") };
    assert_eq!(rgba_pixels, &[255, 0, 0, 255]);
}

#[test]
fn equivalent_image_bytes_under_a_new_key_may_restore() {
    let expected = bitmap_object(49, "Image", [128, 128, 128, 255]);
    let mut doc = import_restorable(expected.clone());
    let id = restorable_id(&doc, "Image");
    let NodeKind::Image(image) = &doc.node(id).unwrap().kind else { panic!("image") };
    let key = image.key.clone();
    let bytes = doc.images.get(&key).unwrap().clone();
    let NodeKind::Image(image) = &mut doc.node_mut(id).unwrap().kind else { panic!("image") };
    image.key = "same-bytes-new-key".into();
    doc.images.insert("same-bytes-new-key".into(), bytes);

    let exported = corel_import::export_document(&doc).unwrap();
    assert_eq!(only_object(&exported), &expected);
}

#[test]
fn missing_image_bytes_fail_closed_instead_of_restoring_original_pixels() {
    let mut doc = import_restorable(bitmap_object(50, "Image", [128, 128, 128, 255]));
    let id = restorable_id(&doc, "Image");
    let NodeKind::Image(image) = &doc.node(id).unwrap().kind else { panic!("image") };
    let key = image.key.clone();
    doc.images.remove(&key);

    let exported = corel_import::export_document(&doc).unwrap();
    let ObjectKind::Bitmap { rgba_pixels, .. } = &only_object(&exported).kind else { panic!("bitmap") };
    assert_eq!(rgba_pixels, &[255, 255, 255, 255]);
}

fn shadowed_document() -> Document {
    let mut object = rect_object(51, "Shadow", [10.0, 10.0, 100.0, 100.0]);
    object.effects.push(VectorEffect::DropShadow {
        offset_x: F64::new(4.0).unwrap(),
        offset_y: F64::new(5.0).unwrap(),
        opacity: F64::new(0.6).unwrap(),
        color: CdrColor::BLACK,
    });
    import_restorable(object)
}

#[test]
fn supported_effect_edit_removal_and_visibility_do_not_restore_old_effect() {
    let mut edited = shadowed_document();
    let id = restorable_id(&edited, "Shadow");
    edited.node_mut(id).unwrap().appearance.effects[0].params["x"] = serde_json::json!(19.0);
    let exported = corel_import::export_document(&edited).unwrap();
    assert!(matches!(only_object(&exported).effects.as_slice(), [VectorEffect::DropShadow { offset_x, .. }] if offset_x.get() == 19.0));

    let mut removed = shadowed_document();
    removed.node_mut(id).unwrap().appearance.effects.clear();
    assert!(only_object(&corel_import::export_document(&removed).unwrap()).effects.is_empty());

    let mut hidden = shadowed_document();
    hidden.node_mut(id).unwrap().appearance.effects[0].visible = false;
    assert!(only_object(&corel_import::export_document(&hidden).unwrap()).effects.is_empty());
}

#[test]
fn opacity_mask_and_blend_edits_do_not_restore_old_transparency() {
    let mut object = rect_object(52, "Mask", [10.0, 10.0, 100.0, 100.0]);
    object.transparency = Some(TransparencyStyle::fountain_linear(45.0, 1.0, 0.2));
    let mut doc = import_restorable(object);
    let id = restorable_id(&doc, "Mask");
    let node = doc.node_mut(id).unwrap();
    node.opacity = 0.4;
    node.blend = BlendMode::Multiply;
    let mut extras = node.extras_mut();
    let mask = extras.mask.as_mut().unwrap();
    Arc::make_mut(&mut mask.art).appearance.set_fill(red_fill());
    drop(extras);

    let exported = corel_import::export_document(&doc).unwrap();
    assert!((only_object(&exported).opacity.get() - 0.4).abs() < 1e-6);
    assert!(only_object(&exported).transparency.is_none());
}

fn nested_group() -> ObjectNode {
    let mut unchanged = rect_object(62, "Unchanged", [20.0, 20.0, 30.0, 30.0]);
    unchanged.fill = Some(FillStyle::Unknown { tag: ChunkTag(*b"unkn"), payload: RawPayload(vec![9, 8, 7]) });
    unchanged.residual.chunks.push(RawChunk { tag: ChunkTag(*b"keep"), flags: 0, slot: 0, payload: RawPayload(vec![6, 5]), src_offset: 12 });
    ObjectNode::new(
        ObjectId(60),
        Some("Group".into()),
        ObjectKind::Group { children: vec![rect_object(61, "Edited", [10.0, 10.0, 100.0, 100.0]), unchanged] },
        None,
        None,
    )
}

#[test]
fn nested_group_child_edit_keeps_current_paint_and_unchanged_sibling_residual() {
    let mut doc = import_restorable(nested_group());
    let edited = doc.node(NodeId(61)).unwrap().id;
    doc.node_mut(edited).unwrap().appearance.set_fill(red_fill());

    let exported = corel_import::export_document(&doc).unwrap();
    let children = group_children(only_object(&exported));
    assert_red(&children[0]);
    assert_eq!(children[1].fill, Some(FillStyle::Unknown { tag: ChunkTag(*b"unkn"), payload: RawPayload(vec![9, 8, 7]) }));
    assert_eq!(children[1].residual.chunks[0].tag, ChunkTag(*b"keep"));
}

#[test]
fn nested_group_delete_and_reorder_use_current_children() {
    let mut deleted = import_restorable(nested_group());
    let id = restorable_id(&deleted, "Group");
    let NodeKind::Group { children, .. } = &mut deleted.node_mut(id).unwrap().kind else { panic!("group") };
    children.remove(1);
    let exported = corel_import::export_document(&deleted).unwrap();
    assert_eq!(group_children(only_object(&exported)).len(), 1);
    assert_eq!(group_children(only_object(&exported))[0].name.as_deref(), Some("Edited"));

    let mut reordered = import_restorable(nested_group());
    let NodeKind::Group { children, .. } = &mut reordered.node_mut(id).unwrap().kind else { panic!("group") };
    children.reverse();
    let exported = corel_import::export_document(&reordered).unwrap();
    let children = group_children(only_object(&exported));
    assert_eq!(children[0].name.as_deref(), Some("Unchanged"));
    assert_eq!(children[0].residual.chunks[0].tag, ChunkTag(*b"keep"));
    assert_eq!(children[1].name.as_deref(), Some("Edited"));
}

#[test]
fn unsupported_container_always_converts_current_children_and_keeps_shell_residual() {
    let unsupported = UnsupportedObject {
        tag: ChunkTag(*b"mesh"),
        class_hint: Some("Opaque Container".into()),
        payload: RawPayload(vec![1, 2, 3]),
        fallback_bounds: None,
        absolute_geometry: false,
        children: vec![rect_object(71, "Child", [10.0, 10.0, 100.0, 100.0])],
        residual: Residual { chunks: vec![RawChunk { tag: ChunkTag(*b"keep"), flags: 0, slot: 0, payload: RawPayload(vec![4]), src_offset: 0 }] },
    };
    let object = ObjectNode::new(ObjectId(70), None, ObjectKind::Unsupported(unsupported), None, None);
    let mut doc = import_restorable(object);
    let edited = doc.node(NodeId(71)).unwrap().id;
    doc.node_mut(edited).unwrap().appearance.set_fill(red_fill());

    let exported = corel_import::export_document(&doc).unwrap();
    let ObjectKind::Unsupported(unsupported) = &only_object(&exported).kind else { panic!("unsupported") };
    assert_red(&unsupported.children[0]);
    assert_eq!(unsupported.payload, RawPayload(vec![1, 2, 3]));
    assert_eq!(unsupported.residual.chunks[0].tag, ChunkTag(*b"keep"));
}

fn powerclip() -> ObjectNode {
    let mut frame = rect_object(80, "PowerClip", [10.0, 10.0, 100.0, 100.0]);
    frame.residual.chunks.push(RawChunk { tag: ChunkTag(*b"keep"), flags: 0, slot: 0, payload: RawPayload(vec![4]), src_offset: 0 });
    frame.powerclip_children.push(rect_object(81, "Contents", [20.0, 20.0, 30.0, 30.0]));
    frame
}

#[test]
fn powerclip_frame_fill_geometry_and_opacity_edits_change_the_exported_container() {
    let mut filled = import_restorable(powerclip());
    let frame_id = filled.node(filled.node(NodeId(80)).unwrap().children().unwrap()[0].id).unwrap().id;
    filled.node_mut(frame_id).unwrap().appearance.set_fill(red_fill());
    let exported = corel_import::export_document(&filled).unwrap();
    assert_red(only_object(&exported));
    assert_eq!(only_object(&exported).residual.chunks[0].tag, ChunkTag(*b"keep"));

    let mut geometry = import_restorable(powerclip());
    let NodeKind::Path { path, live, .. } = &mut geometry.node_mut(frame_id).unwrap().kind else { panic!("frame") };
    *path = shapes::ellipse(Rect::new(10.0, 10.0, 100.0, 100.0));
    *live = None;
    assert!(matches!(only_object(&corel_import::export_document(&geometry).unwrap()).kind, ObjectKind::Path(_)));

    let mut opacity = import_restorable(powerclip());
    opacity.node_mut(frame_id).unwrap().opacity = 0.35;
    assert!((only_object(&corel_import::export_document(&opacity).unwrap()).opacity.get() - 0.35).abs() < 1e-6);
}

#[test]
fn powerclip_contents_add_and_remove_use_the_current_count_and_paint() {
    let mut removed = import_restorable(powerclip());
    let id = restorable_id(&removed, "PowerClip");
    let NodeKind::Group { children, .. } = &mut removed.node_mut(id).unwrap().kind else { panic!("clip") };
    children.remove(1);
    assert!(only_object(&corel_import::export_document(&removed).unwrap()).powerclip_children.is_empty());

    let mut added = import_restorable(powerclip());
    let second = added.node(NodeId(81)).unwrap().clone();
    let second_id = NodeId(82);
    let mut second = second;
    second.id = second_id;
    second.name = Some("Second".into());
    second.appearance.set_fill(red_fill());
    let NodeKind::Group { children, .. } = &mut added.node_mut(id).unwrap().kind else { panic!("clip") };
    children.push(Arc::new(second));
    let exported = corel_import::export_document(&added).unwrap();
    assert_eq!(only_object(&exported).powerclip_children.len(), 2);
    assert_red(&only_object(&exported).powerclip_children[1]);
}

#[test]
fn unchanged_native_fills_residuals_and_source_document_remain_exact() {
    let mut pattern = rect_object(90, "Pattern", [10.0, 10.0, 100.0, 100.0]);
    pattern.fill = Some(FillStyle::TwoColorPattern {
        kind: corel_import::coralcraft_doc::PatternKind::Checkerboard,
        fg_color: CdrColor::BLACK,
        bg_color: CdrColor::WHITE,
        tile_size_pt: F64::new(10.0).unwrap(),
    });
    pattern.residual.chunks.push(RawChunk { tag: ChunkTag(*b"keep"), flags: 0, slot: 0, payload: RawPayload(vec![4]), src_offset: 0 });
    let mut unknown = rect_object(91, "Unknown", [120.0, 10.0, 200.0, 100.0]);
    unknown.fill = Some(FillStyle::Unknown { tag: ChunkTag(*b"unkn"), payload: RawPayload(vec![1, 2, 3]) });
    let mut cdr = CdrDocument::new_a4_portrait();
    cdr.pages[0].layers[0].objects = vec![pattern, unknown, powerclip()];
    cdr.next_id = 92;
    let expected = cdr.clone();
    let doc = import_cdr(cdr);

    let exported = corel_import::export_document(&doc).unwrap();
    assert_eq!(exported, expected);
}

#[test]
fn snapshot_persists_natively_only_on_top_level_imported_objects() {
    let doc = import_restorable(nested_group());
    let metadata = doc.unknown.get(corel_import::CORALCRAFT_UNKNOWN_KEY).unwrap();
    assert!(metadata["object_meta"]["60"].get("original_vc").is_some());
    assert!(metadata["object_meta"]["61"].get("original_vc").is_none());
    assert!(metadata["object_meta"]["62"].get("original_vc").is_none());

    let bytes = vectorcraft_format::save(&doc, false);
    let reopened = vectorcraft_format::load(&bytes).unwrap();
    let metadata = reopened.unknown.get(corel_import::CORALCRAFT_UNKNOWN_KEY).unwrap();
    assert!(metadata["object_meta"]["60"].get("original_vc").is_some());
    assert!(metadata["object_meta"]["61"].get("original_vc").is_none());
    assert!(metadata["object_meta"]["62"].get("original_vc").is_none());
}

#[test]
fn legacy_fallback_reconstructs_powerclip_and_generated_mask_ids_without_mutating_metadata() {
    let mut mask = rect_object(100, "Legacy Mask", [10.0, 10.0, 100.0, 100.0]);
    mask.transparency = Some(TransparencyStyle::fountain_linear(45.0, 1.0, 0.2));
    let mut cdr = CdrDocument::new_a4_portrait();
    cdr.pages[0].layers[0].objects = vec![mask, powerclip()];
    cdr.next_id = 101;
    let expected = cdr.clone();
    let mut doc = import_cdr(cdr);
    clear_original_vc(&mut doc);
    let before = doc.unknown.get(corel_import::CORALCRAFT_UNKNOWN_KEY).unwrap().clone();

    let exported = corel_import::export_document(&doc).unwrap();
    assert_eq!(exported, expected);
    assert_eq!(doc.unknown.get(corel_import::CORALCRAFT_UNKNOWN_KEY), Some(&before));
}

#[test]
fn legacy_fallback_also_fails_closed_for_current_edits() {
    let mut doc = import_restorable(powerclip());
    clear_original_vc(&mut doc);
    let frame_id = doc.node(doc.node(NodeId(80)).unwrap().children().unwrap()[0].id).unwrap().id;
    doc.node_mut(frame_id).unwrap().appearance.set_fill(red_fill());

    let exported = corel_import::export_document(&doc).unwrap();
    assert_red(only_object(&exported));
}

fn transparent_powerclip() -> ObjectNode {
    let mut frame = powerclip();
    frame.transparency = Some(TransparencyStyle::fountain_linear(45.0, 1.0, 0.2));
    frame
}

fn clip_frame_id(doc: &Document) -> NodeId {
    doc.node(doc.node(NodeId(80)).unwrap().children().unwrap()[0].id).unwrap().id
}

#[test]
fn powerclip_frame_transparency_edit_uses_current_conversion() {
    let mut doc = import_restorable(transparent_powerclip());
    let frame_id = clip_frame_id(&doc);
    let frame = doc.node_mut(frame_id).unwrap();
    let mut extras = frame.extras_mut();
    let mask = extras.mask.as_mut().unwrap();
    Arc::make_mut(&mut mask.art).appearance.set_fill(red_fill());
    drop(extras);

    let exported = corel_import::export_document(&doc).unwrap();
    assert!(only_object(&exported).transparency.is_none());
}

#[test]
fn powerclip_frame_transparency_removal_uses_current_conversion_in_restore_and_fallback() {
    let mut restored = import_restorable(transparent_powerclip());
    let frame_id = clip_frame_id(&restored);
    restored.node_mut(frame_id).unwrap().extras_mut().mask = None;
    assert!(only_object(&corel_import::export_document(&restored).unwrap()).transparency.is_none());

    let mut fallback = import_restorable(transparent_powerclip());
    let frame_id = clip_frame_id(&fallback);
    fallback.node_mut(frame_id).unwrap().extras_mut().mask = None;
    fallback.node_mut(NodeId(80)).unwrap().name = Some("Changed outer shell".into());
    let exported = corel_import::export_document(&fallback).unwrap();
    assert!(only_object(&exported).transparency.is_none());
}

#[test]
fn unchanged_transparent_powerclip_still_restores_original_transparency() {
    let original = transparent_powerclip();
    let expected = original.transparency.clone();
    let doc = import_restorable(original);
    let exported = corel_import::export_document(&doc).unwrap();
    assert_eq!(only_object(&exported).transparency, expected);
}

#[test]
fn missing_current_bounds_fail_closed_for_original_unknown_fill() {
    let unsupported = UnsupportedObject {
        tag: ChunkTag(*b"mesh"),
        class_hint: Some("No Bounds".into()),
        payload: RawPayload(vec![1, 2, 3]),
        fallback_bounds: None,
        absolute_geometry: false,
        children: Vec::new(),
        residual: Residual::default(),
    };
    let mut object = ObjectNode::new(ObjectId(110), None, ObjectKind::Unsupported(unsupported), None, None);
    object.fill = Some(FillStyle::Unknown { tag: ChunkTag(*b"unkn"), payload: RawPayload(vec![4, 5, 6]) });
    let doc = import_restorable(object);

    let exported = corel_import::export_document(&doc).unwrap();
    assert_eq!(only_object(&exported).fill, Some(FillStyle::uniform(CdrColor::gray(0.5).unwrap())));
    assert!(matches!(only_object(&exported).kind, ObjectKind::Path(_)));
}
