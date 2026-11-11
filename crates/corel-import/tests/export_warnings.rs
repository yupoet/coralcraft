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
    assert!(matches!(converted.pages[0].layers[0].objects[0].fill, Some(corel_import::coralcraft_doc::FillStyle::Uniform { .. })));
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
