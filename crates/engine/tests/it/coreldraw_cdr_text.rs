//! CoralCraft: legacy CDR text mapping evidence. The synthetic residual is handcrafted; it is not
//! a proprietary fixture and does not close the real-sample S2 verification gate.

use vectorcraft_doc::{NodeKind, TextObject};

#[test]
fn unknown_text_encoding_residual_survives_native_and_cdr_mapping() {
    use corel_import::coralcraft_doc::{ChunkTag, Document, ObjectId, ObjectKind, ObjectNode, RawChunk, RawPayload};
    let mut cdr = Document::new_a4_portrait();
    let mut object = ObjectNode::new(
        ObjectId(42),
        Some("Legacy text".into()),
        ObjectKind::ArtisticText {
            text: "Text".into(),
            font_family: "Arial".into(),
            size_pt: corel_import::coralcraft_geom::F64::new(12.0).unwrap(),
            position: corel_import::coralcraft_geom::Point::ZERO,
        },
        None,
        None,
    );
    object.residual.chunks.push(RawChunk { tag: ChunkTag(*b"txtd"), flags: 0, slot: 0, payload: RawPayload(vec![0x7f, b'A']), src_offset: 0 });
    cdr.pages[0].layers[0].objects.push(object);

    let (document, _) = corel_import::import_document(&cdr).unwrap();
    let native = vectorcraft_format::save(&document, false);
    let reopened = vectorcraft_format::load(&native).unwrap();
    let text = reopened
        .layers
        .iter()
        .find_map(|layer| {
            let mut found = None;
            layer.walk(&mut |node| {
                if matches!(node.kind, NodeKind::Text(_)) {
                    found = Some(node.clone());
                }
            });
            found
        })
        .expect("the imported text remains a live text object");
    let NodeKind::Text(text) = &text.kind else { panic!("expected live text") };
    assert_eq!(text.as_ref(), &TextObject::point(text.xf * vectorcraft_geom::Point::ZERO, "Text", text.first_style()));

    let exported = corel_import::export_document(&reopened).unwrap();
    let written = &exported.pages[0].layers[0].objects[0];
    assert!(
        written.residual.chunks.iter().any(|chunk| chunk.tag == ChunkTag(*b"txtd") && chunk.payload.0 == [0x7f, b'A']),
        "the writer preserves the unknown original bytes"
    );
}
