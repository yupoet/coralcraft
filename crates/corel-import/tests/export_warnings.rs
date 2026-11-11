//! CoralCraft: public warning-returning export API compatibility.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use vectorcraft_color::Paint;
use vectorcraft_doc::{Appearance, Document, Node};
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
fn warning_returning_export_apis_preserve_existing_results() {
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
