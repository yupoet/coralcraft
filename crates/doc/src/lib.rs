//! `coralcraft-doc`: Multi-page CorelDRAW document model (`Document`, `Page`, `Layer`, `ObjectNode`,
//! `ObjectKind::Unsupported`) and dual-mode `.coralcraft` container (`save_json`, `save_zip`, `load_bytes`).

pub mod container;
pub mod interchange;
pub mod model;
pub mod style;
pub mod text_layout;

pub use container::{BLOB_THRESHOLD, crc32, list_zip_blobs, load_bytes, save_json, save_zip, sha256_hex};
pub use interchange::{import_dxf, import_svg};
pub use model::{
    CURRENT_SCHEMA, DocError, Document, Layer, LayerId, LayerRole, LensKind, ObjectId, ObjectKind, ObjectNode, Page, PageId, ParagraphTextSpec,
    SchemaVersion, TextAlignment, TransparencyStyle, UnsupportedObject, VectorEffect,
};
pub use style::{
    ArrowheadKind, ChunkTag, FillStyle, FountainGeometry, LineCap, LineJoin, OutlineStyle, PatternKind, RawChunk, RawPayload, Residual, decode_hex,
    encode_hex,
};
pub use text_layout::{ParagraphLayout, PositionedTextLine, TextCaseKind, advance_pt, apply_text_case, layout_paragraph_text};

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing, clippy::arithmetic_side_effects)]
mod tests {
    use std::collections::BTreeMap;

    use coralcraft_color::{Color, FountainBlendMode, FountainStop};
    use coralcraft_geom::{CornerKind, F64, Rect, RectSpec, Size};

    use super::*;

    fn splitmix64(state: &mut u64) -> u64 {
        *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = *state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    #[test]
    fn sha256_matches_nist_fips_180_4_vectors() {
        assert_eq!(sha256_hex(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    #[test]
    fn document_dual_mode_json_and_zip_roundtrip_equality() {
        let mut doc = Document::new_a4_portrait();
        let rect_id = doc.allocate_object_id();
        let rect = RectSpec::uniform(Rect::new(20.0, 30.0, 220.0, 150.0).unwrap(), 8.0, CornerKind::Round).unwrap();
        let node = ObjectNode::new(
            rect_id,
            Some("Hero Card".to_owned()),
            ObjectKind::Rect(rect),
            Some(FillStyle::Fountain {
                geometry: FountainGeometry::Linear { angle_deg: F64::new(45.0).unwrap(), edge_pad: F64::ZERO },
                blend_mode: FountainBlendMode::Direct,
                stops: vec![FountainStop::new(0.0, Color::rgb8(255, 0, 0)).unwrap(), FountainStop::new(1.0, Color::rgb8(0, 0, 255)).unwrap()],
            }),
            Some(OutlineStyle::hairline(Color::BLACK)),
        );
        doc.pages[0].layers[0].objects.push(node);

        let json_bytes = save_json(&doc).unwrap();
        let zip_bytes = save_zip(&doc).unwrap();

        let from_json = load_bytes(&json_bytes).unwrap();
        let from_zip = load_bytes(&zip_bytes).unwrap();
        assert_eq!(from_json, doc);
        assert_eq!(from_zip, doc);
        assert_eq!(from_zip, from_json);

        // Byte idempotence of save_zip
        let zip_bytes_2 = save_zip(&from_zip).unwrap();
        assert_eq!(zip_bytes_2, zip_bytes);
    }

    #[test]
    fn unsupported_objects_blob_threshold_dedupe_and_tamper_detection() {
        let mut doc = Document::new_a4_portrait();

        // Payload sizes at boundary: 0, 1, 63 (inline hex in ZIP), 64, 65 (externalized to blobs/ in ZIP)
        let payload_63 = vec![0xAA_u8; 63];
        let payload_64 = (0..64_u8).collect::<Vec<u8>>();
        let payload_65 = (0..65_u8).collect::<Vec<u8>>();

        for (i, payload) in [
            Vec::new(),
            vec![0x42],
            payload_63,
            payload_64.clone(),
            payload_64.clone(), // duplicate 64-byte payload must deduplicate to 1 blob entry!
            payload_65,
        ]
        .into_iter()
        .enumerate()
        {
            let id = doc.allocate_object_id();
            let u = UnsupportedObject {
                tag: ChunkTag(*b"lens"),
                class_hint: Some(format!("Lens #{i}")),
                payload: RawPayload(payload),
                fallback_bounds: Some(Rect::new(0.0, 0.0, 50.0, 50.0).unwrap()),
                absolute_geometry: i % 2 == 0,
                children: Vec::new(),
                residual: Residual {
                    chunks: vec![RawChunk {
                        tag: ChunkTag(*b"unkn"),
                        flags: 0x0001,
                        slot: 0,
                        payload: RawPayload(vec![1, 2, 3, 4]),
                        src_offset: 128,
                    }],
                },
            };
            doc.pages[0].layers[0].objects.push(ObjectNode::new(id, Some(format!("Unsupported {i}")), ObjectKind::Unsupported(u), None, None));
        }

        let json_bytes = save_json(&doc).unwrap();
        let zip_bytes = save_zip(&doc).unwrap();

        // Only payload_64 (deduped) and payload_65 are >= 64 bytes -> exactly 2 blobs in ZIP!
        let blobs = list_zip_blobs(&zip_bytes).unwrap();
        assert_eq!(blobs.len(), 2);

        // Both modes round-trip to the exact same Document with all 6 Unsupported nodes intact
        assert_eq!(load_bytes(&json_bytes).unwrap(), doc);
        assert_eq!(load_bytes(&zip_bytes).unwrap(), doc);

        // Tampering a byte inside a blob payload in the ZIP archive must return Err(DocError::CorruptBlob)
        let mut corrupted = zip_bytes.clone();
        // Flip the last byte before the Central Directory (which lies inside the last blob's data)
        let needle = (0..65_u8).collect::<Vec<u8>>();
        let pos = corrupted.windows(needle.len()).position(|w| w == needle.as_slice()).unwrap();
        corrupted[pos + 10] ^= 0xFF;
        assert!(matches!(load_bytes(&corrupted), Err(DocError::CorruptBlob { .. })));
    }

    #[test]
    fn property_random_documents_roundtrip_json_and_zip_identically() {
        let mut seed = 0xC0DE_CAFE_2026_0003_u64;
        for case_idx in 0..100 {
            let mut doc = Document::new_a4_portrait();
            doc.title = format!("PropDoc-{case_idx}-🦀");
            let page_count = (splitmix64(&mut seed) % 3) as usize + 1;
            while doc.pages.len() < page_count {
                let pid = PageId(doc.allocate_object_id().0);
                let lid = LayerId(doc.allocate_object_id().0);
                doc.pages.push(Page {
                    id: pid,
                    name: format!("Page {}", doc.pages.len() + 1),
                    size: Size::new(400.0 + (case_idx as f64), 600.0).unwrap(),
                    bleed: F64::new(9.0).unwrap(),
                    background: None,
                    layers: vec![Layer::new_normal(lid, "Layer 1")],
                    residual: Residual::default(),
                    extra: BTreeMap::new(),
                });
            }

            for p in 0..doc.pages.len() {
                let obj_count = (splitmix64(&mut seed) % 4) as usize + 1;
                for j in 0..obj_count {
                    let oid = doc.allocate_object_id();
                    let len_choices = [0_usize, 1, 63, 64, 65, 128];
                    let plen = len_choices[(splitmix64(&mut seed) as usize) % len_choices.len()];
                    let mut bytes = vec![0_u8; plen];
                    for b in &mut bytes {
                        *b = (splitmix64(&mut seed) & 0xFF) as u8;
                    }
                    let u = UnsupportedObject {
                        tag: ChunkTag(*b"test"),
                        class_hint: Some(format!("p{p}_o{j}")),
                        payload: RawPayload(bytes),
                        fallback_bounds: Some(Rect::new(0.0, 0.0, 10.0, 10.0).unwrap()),
                        absolute_geometry: false,
                        children: Vec::new(),
                        residual: Residual::default(),
                    };
                    doc.pages[p].layers[0].objects.push(ObjectNode::new(oid, None, ObjectKind::Unsupported(u), None, None));
                }
            }

            let j = save_json(&doc).unwrap();
            let z = save_zip(&doc).unwrap();
            let dj = load_bytes(&j).unwrap();
            let dz = load_bytes(&z).unwrap();
            assert_eq!(dj, doc);
            assert_eq!(dz, doc);
            assert_eq!(save_zip(&dz).unwrap(), z);
        }
    }

    #[test]
    fn multipage_master_and_desktop_layer_z_order_semantics() {
        let mut doc = Document::new_a4_portrait();
        // Add Page 2
        doc.pages.push(Page {
            id: PageId(10),
            name: "Page 2".to_owned(),
            size: doc.pages[0].size,
            bleed: F64::ZERO,
            background: None,
            layers: vec![Layer::new_normal(LayerId(11), "P2 Layer 1")],
            residual: Residual::default(),
            extra: BTreeMap::new(),
        });

        // Add Master Odd (behind), Master Even (behind), Master All (in front)
        doc.master_layers.insert(
            0,
            Layer {
                id: LayerId(20),
                name: "Master Odd BG".to_owned(),
                role: LayerRole::MasterOddPages,
                visible: true,
                printable: true,
                locked: false,
                master_in_front: false,
                opacity: F64::ONE,
                objects: Vec::new(),
                residual: Residual::default(),
                extra: BTreeMap::new(),
            },
        );
        doc.master_layers.insert(
            1,
            Layer {
                id: LayerId(21),
                name: "Master Even BG".to_owned(),
                role: LayerRole::MasterEvenPages,
                visible: true,
                printable: true,
                locked: false,
                master_in_front: false,
                opacity: F64::ONE,
                objects: Vec::new(),
                residual: Residual::default(),
                extra: BTreeMap::new(),
            },
        );
        doc.master_layers.insert(
            2,
            Layer {
                id: LayerId(22),
                name: "Master All FG".to_owned(),
                role: LayerRole::MasterAllPages,
                visible: true,
                printable: true,
                locked: false,
                master_in_front: true,
                opacity: F64::ONE,
                objects: Vec::new(),
                residual: Residual::default(),
                extra: BTreeMap::new(),
            },
        );

        // Page 1 (odd, index 0) screen view: Master Odd BG -> Layer 1 -> Master All FG -> Desktop
        let p1_screen: Vec<&str> = doc.effective_layers_for_page(0, false).unwrap().into_iter().map(|l| l.name.as_str()).collect();
        assert_eq!(p1_screen, vec!["Master Odd BG", "Layer 1", "Master All FG", "Desktop"]);

        // Page 2 (even, index 1) print/export view (Desktop has printable == false):
        // Master Even BG -> P2 Layer 1 -> Master All FG
        let p2_export: Vec<&str> = doc.effective_layers_for_page(1, true).unwrap().into_iter().map(|l| l.name.as_str()).collect();
        assert_eq!(p2_export, vec!["Master Even BG", "P2 Layer 1", "Master All FG"]);
    }

    #[test]
    fn paragraph_text_wrapping_alignment_multicol_and_serde() {
        use coralcraft_geom::Point;

        // 1. Single column word wrapping and justification
        let spec_justified = ParagraphTextSpec::new(
            Point::new(10.0, 20.0).unwrap(),
            120.0,
            100.0,
            "The quick brown fox jumps over the lazy dog\nSecond paragraph text line.",
            "Arial",
            12.0,
            1.2,
            TextAlignment::Justify,
            1,
            0.0,
        )
        .unwrap();

        let layout_j = layout_paragraph_text(&spec_justified);
        assert!(!layout_j.overflow);
        assert_eq!(layout_j.column_count, 1);
        assert!(layout_j.lines.len() >= 3);
        // Non-final lines of a paragraph in Justify mode should have word_spacing_pt > 0
        assert!(layout_j.lines[0].word_spacing_pt > 0.0);
        // Final lines of paragraph should have word_spacing_pt == 0
        assert_eq!(layout_j.lines.last().unwrap().word_spacing_pt, 0.0);

        // 2. Multi-column flow
        let spec_multicol = ParagraphTextSpec::new(
            Point::ZERO,
            200.0,
            40.0, // Short height forces flow into column 2
            "Word1 Word2 Word3 Word4 Word5 Word6 Word7 Word8 Word9 Word10",
            "Helvetica",
            10.0,
            1.2,
            TextAlignment::Left,
            2,
            10.0, // 10pt gutter
        )
        .unwrap();

        let layout_multi = layout_paragraph_text(&spec_multicol);
        assert_eq!(layout_multi.column_count, 2);
        assert!((layout_multi.column_width_pt - 95.0).abs() < 1e-4); // (200 - 10) / 2 = 95
        let has_col0 = layout_multi.lines.iter().any(|l| l.column == 0);
        let has_col1 = layout_multi.lines.iter().any(|l| l.column == 1);
        assert!(has_col0 && has_col1);

        // 3. Overflow detection
        let spec_overflow = ParagraphTextSpec::new(
            Point::ZERO,
            50.0,
            20.0, // Only fits ~1 line
            "This text is far too long to fit in this tiny frame and must overflow",
            "Arial",
            12.0,
            1.2,
            TextAlignment::Left,
            1,
            0.0,
        )
        .unwrap();

        let layout_ovf = layout_paragraph_text(&spec_overflow);
        assert!(layout_ovf.overflow);

        // 4. Serde roundtrip
        let node = ObjectNode::new(
            ObjectId(100),
            Some("Body Text".to_owned()),
            ObjectKind::ParagraphText(Box::new(spec_justified.clone())),
            Some(FillStyle::uniform(Color::rgb8(0, 0, 0))),
            None,
        );

        let json = serde_json::to_string(&node).unwrap();
        assert!(json.contains(r#""kind":"paragraph_text""#));
        let decoded: ObjectNode = serde_json::from_str(&json).unwrap();
        assert_eq!(node, decoded);
    }

    #[test]
    fn import_svg_and_import_dxf_produce_valid_vector_documents() {
        let svg_sample = r##"<svg width="400" height="300" xmlns="http://www.w3.org/2000/svg">
            <rect x="10" y="20" width="120" height="80" rx="6" fill="#E03040" stroke="#112233" stroke-width="2" />
            <circle cx="200" cy="150" r="45" fill="rgb(20, 140, 220)" />
            <path d="M 10 10 L 90 10 C 90 50 50 90 10 90 Z" fill="#00AA55" />
            <text x="30" y="260" font-family="Arial" font-size="18" fill="#222222">Imported SVG</text>
        </svg>"##;
        let doc_svg = import_svg(svg_sample).unwrap();
        assert_eq!(doc_svg.pages[0].size.width.get(), 400.0);
        assert_eq!(doc_svg.pages[0].size.height.get(), 300.0);
        assert_eq!(doc_svg.pages[0].layers[0].objects.len(), 4);

        let dxf_sample = "0\nSECTION\n2\nENTITIES\n0\nLINE\n8\n0\n10\n10.0\n20\n20.0\n11\n110.0\n21\n70.0\n0\nCIRCLE\n8\n0\n10\n150.0\n20\n150.0\n40\n35.0\n0\nLWPOLYLINE\n70\n1\n10\n0.0\n20\n0.0\n10\n80.0\n20\n0.0\n10\n80.0\n20\n60.0\n0\nENDSEC\n0\nEOF\n";
        let doc_dxf = import_dxf(dxf_sample).unwrap();
        assert_eq!(doc_dxf.pages[0].layers[0].objects.len(), 3);
    }

    #[test]
    fn forward_compatible_enum_variants_deserialize_unknown_tags_m65() {
        let mut doc = Document::new_a4_portrait();
        let rect = Rect::new(10.0, 10.0, 100.0, 80.0).unwrap();
        let node = ObjectNode::new(
            ObjectId(42),
            Some("ForwardCompat".to_owned()),
            ObjectKind::Rect(RectSpec::sharp(rect)),
            Some(FillStyle::TwoColorPattern {
                kind: PatternKind::Checkerboard,
                fg_color: Color::BLACK,
                bg_color: Color::WHITE,
                tile_size_pt: F64::new(12.0).unwrap(),
            }),
            Some(OutlineStyle::hairline(Color::BLACK)),
        );
        doc.pages[0].layers[0].objects.push(node);

        let mut json = String::from_utf8(save_json(&doc).unwrap()).unwrap();
        json = json.replace(r#""role":"normal""#, r#""role":"future_ai_guide_layer""#);
        json = json.replace(r#""kind":"checkerboard""#, r#""kind":"future_hex_pattern""#);
        json = json.replace(r#""start_arrow":"none""#, r#""start_arrow":"future_feathered_arrow""#);

        let loaded = load_bytes(json.as_bytes()).expect("unknown enum tags from newer minor schema must deserialize gracefully");
        assert_eq!(loaded.pages[0].layers[0].role, LayerRole::Unknown);
        let obj = &loaded.pages[0].layers[0].objects[0];
        assert!(matches!(obj.fill, Some(FillStyle::TwoColorPattern { kind: PatternKind::Unknown, .. })));
        assert_eq!(obj.outline.as_ref().unwrap().start_arrow, ArrowheadKind::Unknown);

        let effect_unknown: VectorEffect = serde_json::from_str(r#"{"type":"future_bevel_3d","depth":12.0}"#).unwrap();
        assert_eq!(effect_unknown, VectorEffect::Unknown);
        let trans_unknown: TransparencyStyle = serde_json::from_str(r#"{"type":"future_mesh_opacity"}"#).unwrap();
        assert_eq!(trans_unknown, TransparencyStyle::Unknown);
        let lens_unknown: LensKind = serde_json::from_str(r#"{"lens_type":"future_chromatic_aberration"}"#).unwrap();
        assert_eq!(lens_unknown, LensKind::Unknown);
    }

    #[test]
    fn import_svg_defs_linear_and_radial_gradients_roundtrip_m76() {
        let svg = r##"<svg width="400" height="300" xmlns="http://www.w3.org/2000/svg">
          <defs>
            <linearGradient id="lin1" x1="0" y1="0" x2="100" y2="100">
              <stop offset="0%" stop-color="#ff0000"/>
              <stop offset="50%" style="stop-color: #00ff00"/>
              <stop offset="1" stop-color="#0000ff"/>
            </linearGradient>
            <radialGradient id="rad1" cx="50" cy="50" r="40">
              <stop offset="0" stop-color="#ffffff"/>
              <stop offset="100%" stop-color="#112233"/>
            </radialGradient>
            <radialGradient id="con1" data-fountain-type="conical" data-angle="30" cx="50" cy="50" r="40">
              <stop offset="0" stop-color="#aa0000"/>
              <stop offset="1" stop-color="#0000aa"/>
            </radialGradient>
          </defs>
          <rect x="10" y="10" width="100" height="80" fill="url(#lin1)"/>
          <circle cx="200" cy="60" r="40" style="fill: url(#rad1)"/>
          <path d="M 20 150 L 120 150 L 70 230 Z" fill="url('#con1')"/>
        </svg>"##;

        let doc = import_svg(svg).expect("import_svg with gradients");
        let objs = &doc.pages[0].layers[0].objects;
        assert_eq!(objs.len(), 3);

        match &objs[0].fill {
            Some(FillStyle::Fountain { geometry: FountainGeometry::Linear { angle_deg, .. }, stops, .. }) => {
                assert!((angle_deg.get() - 45.0).abs() < 1e-3);
                assert_eq!(stops.len(), 3);
                assert_eq!(stops[0].color, Color::rgb8(255, 0, 0));
                assert!((stops[1].offset.get() - 0.5).abs() < 1e-6);
                assert_eq!(stops[1].color, Color::rgb8(0, 255, 0));
                assert_eq!(stops[2].color, Color::rgb8(0, 0, 255));
            }
            other => panic!("expected Linear Fountain fill on rect, got {other:?}"),
        }

        match &objs[1].fill {
            Some(FillStyle::Fountain { geometry: FountainGeometry::Radial { .. }, stops, .. }) => {
                assert_eq!(stops.len(), 2);
                assert_eq!(stops[0].color, Color::rgb8(255, 255, 255));
                assert_eq!(stops[1].color, Color::rgb8(0x11, 0x22, 0x33));
            }
            other => panic!("expected Radial Fountain fill on circle, got {other:?}"),
        }

        match &objs[2].fill {
            Some(FillStyle::Fountain { geometry: FountainGeometry::Conical { angle_deg, .. }, stops, .. }) => {
                assert!((angle_deg.get() - 30.0).abs() < 1e-3);
                assert_eq!(stops.len(), 2);
            }
            other => panic!("expected Conical Fountain fill on path, got {other:?}"),
        }
    }

    #[test]
    fn import_svg_defs_patterns_and_tspan_text_roundtrip_m77() {
        let svg = r##"<svg width="500" height="300" xmlns="http://www.w3.org/2000/svg">
          <defs>
            <pattern id="pat-chk" patternUnits="userSpaceOnUse" width="16" height="16">
              <rect width="16" height="16" fill="#ffffff"/>
              <rect width="8" height="8" fill="#112233"/>
              <rect x="8" y="8" width="8" height="8" fill="#112233"/>
            </pattern>
            <pattern id="pat-dot" patternUnits="userSpaceOnUse" width="20" height="20">
              <rect width="20" height="20" fill="#f0f0f0"/>
              <circle cx="10" cy="10" r="7" fill="#cc2244"/>
            </pattern>
            <pattern id="pat-h" patternUnits="userSpaceOnUse" width="12" height="12">
              <rect width="12" height="12" fill="#eeeeee"/>
              <rect width="12" height="6" fill="#228844"/>
            </pattern>
            <pattern id="pat-v" patternUnits="userSpaceOnUse" width="14" height="14">
              <rect width="14" height="14" fill="#dddddd"/>
              <rect width="7" height="14" fill="#3344aa"/>
            </pattern>
            <pattern id="pat-x" patternUnits="userSpaceOnUse" width="18" height="18">
              <rect width="18" height="18" fill="#fafafa"/>
              <path d="M 0 0 L 18 18 M 0 18 L 18 0" stroke="#881122" stroke-width="2.7"/>
            </pattern>
          </defs>
          <rect x="10" y="10" width="50" height="50" fill="url(#pat-chk)"/>
          <rect x="70" y="10" width="50" height="50" fill="url(#pat-dot)"/>
          <rect x="130" y="10" width="50" height="50" fill="url(#pat-h)"/>
          <rect x="190" y="10" width="50" height="50" fill="url(#pat-v)"/>
          <rect x="250" y="10" width="50" height="50" fill="url(#pat-x)"/>
          <text font-family="Helvetica" font-size="14" fill="#222222">
            <tspan x="20" y="140">First &amp; Line</tspan>
            <tspan x="20" y="158">Second &lt;Line&gt;</tspan>
          </text>
        </svg>"##;

        let doc = import_svg(svg).expect("import_svg with patterns and tspan");
        let objs = &doc.pages[0].layers[0].objects;
        assert_eq!(objs.len(), 6);

        let expected = [
            (PatternKind::Checkerboard, Color::rgb8(0x11, 0x22, 0x33), Color::WHITE, 16.0),
            (PatternKind::PolkaDot, Color::rgb8(0xcc, 0x22, 0x44), Color::rgb8(0xf0, 0xf0, 0xf0), 20.0),
            (PatternKind::HorizontalStripe, Color::rgb8(0x22, 0x88, 0x44), Color::rgb8(0xee, 0xee, 0xee), 12.0),
            (PatternKind::VerticalStripe, Color::rgb8(0x33, 0x44, 0xaa), Color::rgb8(0xdd, 0xdd, 0xdd), 14.0),
            (PatternKind::DiagonalCrosshatch, Color::rgb8(0x88, 0x11, 0x22), Color::rgb8(0xfa, 0xfa, 0xfa), 18.0),
        ];
        for (idx, (exp_kind, exp_fg, exp_bg, exp_ts)) in expected.iter().enumerate() {
            match &objs[idx].fill {
                Some(FillStyle::TwoColorPattern { kind, fg_color, bg_color, tile_size_pt }) => {
                    assert_eq!(*kind, *exp_kind);
                    assert_eq!(*fg_color, *exp_fg);
                    assert_eq!(*bg_color, *exp_bg);
                    assert!((tile_size_pt.get() - *exp_ts).abs() < 1e-6);
                }
                other => panic!("expected TwoColorPattern at {idx}, got {other:?}"),
            }
        }

        match &objs[5].kind {
            ObjectKind::ArtisticText { text, font_family, size_pt, position } => {
                assert_eq!(text, "First & Line\nSecond <Line>");
                assert_eq!(font_family, "Helvetica");
                assert!((size_pt.get() - 14.0).abs() < 1e-6);
                assert!((position.x.get() - 20.0).abs() < 1e-6);
                assert!((position.y.get() - 140.0).abs() < 1e-6);
            }
            other => panic!("expected ArtisticText from <text><tspan>, got {other:?}"),
        }
    }

    #[test]
    fn import_svg_mask_fountain_transparency_clip_path_powerclip_and_base64_png_image_m78() {
        // 2x1 stored-deflate RGBA8 PNG: pixel(0,0)=[255,0,0,255], pixel(1,0)=[0,255,0,128]
        // PNG signature + IHDR(2x1, 8-bit RGBA) + IDAT(zlib stored 9 bytes: [0, 255,0,0,255, 0,255,0,128]) + IEND
        let b64_png = "iVBORw0KGgoAAAANSUhEUgAAAAIAAAABCAYAAAD0In+KAAAAFElEQVR4AWP4z8DwH4gZGBj+AwA5cQf82/7j1AAAAABJRU5ErkJggg==";
        let _ = b64_png;
        // Build a deterministic stored-deflate PNG in-memory for the test
        let raw_scanline: [u8; 9] = [0, 255, 0, 0, 255, 0, 255, 0, 128];
        let mut zlib = vec![0x78, 0x01, 0x01, 9, 0, !9u8, 0xFF];
        zlib.extend_from_slice(&raw_scanline);
        // Adler-32 of [0, 255, 0, 0, 255, 0, 255, 0, 128]
        let mut s1 = 1u32;
        let mut s2 = 0u32;
        for &b in &raw_scanline {
            s1 = (s1 + u32::from(b)) % 65_521;
            s2 = (s2 + s1) % 65_521;
        }
        zlib.extend_from_slice(&((s2 << 16) | s1).to_be_bytes());
        let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        let ihdr: [u8; 13] = [0, 0, 0, 2, 0, 0, 0, 1, 8, 6, 0, 0, 0];
        for (ctype, data) in [(*b"IHDR", ihdr.as_slice()), (*b"IDAT", zlib.as_slice()), (*b"IEND", &[][..])] {
            png.extend_from_slice(&(data.len() as u32).to_be_bytes());
            let cs = png.len();
            png.extend_from_slice(&ctype);
            png.extend_from_slice(data);
            let crc = crc32(&png[cs..]);
            png.extend_from_slice(&crc.to_be_bytes());
        }
        const B64_CHARS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut b64 = String::new();
        for chunk in png.chunks(3) {
            let b0 = chunk[0];
            let b1 = *chunk.get(1).unwrap_or(&0);
            let b2 = *chunk.get(2).unwrap_or(&0);
            let n = (u32::from(b0) << 16) | (u32::from(b1) << 8) | u32::from(b2);
            b64.push(B64_CHARS[((n >> 18) & 63) as usize] as char);
            b64.push(B64_CHARS[((n >> 12) & 63) as usize] as char);
            b64.push(if chunk.len() > 1 { B64_CHARS[((n >> 6) & 63) as usize] as char } else { '=' });
            b64.push(if chunk.len() > 2 { B64_CHARS[(n & 63) as usize] as char } else { '=' });
        }

        let svg = format!(
            r##"<svg width="400" height="300" xmlns="http://www.w3.org/2000/svg">
          <defs>
            <linearGradient id="grad-trsp-1" x1="0" y1="0" x2="100" y2="0">
              <stop offset="0" stop-color="#ffffff"/>
              <stop offset="1" stop-color="#000000"/>
            </linearGradient>
            <mask id="mask-obj-1" maskUnits="userSpaceOnUse">
              <rect x="0" y="0" width="100" height="80" fill="url(#grad-trsp-1)"/>
            </mask>
            <clipPath id="clip-1" clipPathUnits="userSpaceOnUse">
              <path d="M 10 10 L 110 10 L 110 90 L 10 90 Z"/>
            </clipPath>
          </defs>
          <rect x="0" y="0" width="100" height="80" fill="#ff0000" mask="url(#mask-obj-1)"/>
          <path d="M 10 10 L 110 10 L 110 90 L 10 90 Z" fill="#eeeeee"/>
          <g clip-path="url(#clip-1)">
            <circle cx="60" cy="50" r="30" fill="#00aa44"/>
          </g>
          <image x="20" y="30" width="40" height="20" href="data:image/png;base64,{b64}"/>
        </svg>"##
        );

        let doc = import_svg(&svg).expect("import_svg with mask, clipPath, and image");
        let objs = &doc.pages[0].layers[0].objects;
        assert_eq!(objs.len(), 3);

        assert!(matches!(objs[0].transparency, Some(TransparencyStyle::Fountain { geometry: FountainGeometry::Linear { .. }, .. })));
        assert_eq!(objs[1].powerclip_children.len(), 1);
        assert!(matches!(objs[1].powerclip_children[0].kind, ObjectKind::Ellipse(_)));

        match &objs[2].kind {
            ObjectKind::Bitmap { pixel_width, pixel_height, rgba_pixels, .. } => {
                assert_eq!((*pixel_width, *pixel_height), (2, 1));
                assert_eq!(rgba_pixels.as_slice(), &[255, 0, 0, 255, 0, 255, 0, 128]);
            }
            other => panic!("expected Bitmap from <image>, got {other:?}"),
        }
        assert!((objs[2].transform.tx.get() - 20.0).abs() < 1e-6);
        assert!((objs[2].transform.ty.get() - 30.0).abs() < 1e-6);
    }

    #[test]
    fn import_svg_multilayer_page_background_and_decorative_marker_filtering_m79() {
        let svg = r##"<svg width="500" height="400" xmlns="http://www.w3.org/2000/svg">
          <rect id="page-background" x="0" y="0" width="500" height="400" fill="#f4f8ff"/>
          <g id="layer-2" data-name="Base Layer">
            <rect x="20" y="20" width="100" height="60" fill="#114488"/>
            <polygon class="arrowhead-marker" points="10,10 20,15 10,20" fill="#000000"/>
          </g>
          <g id="layer-3" data-name="Overlay Layer" opacity="0.75">
            <circle cx="200" cy="150" r="40" fill="#ee5522"/>
          </g>
          <g id="prepress-marks">
            <path class="crop-marks" d="M 0 0 L 10 0"/>
            <circle class="registration-target" cx="-10" cy="200" r="3.5"/>
            <path class="registration-crosshair" d="M -15 200 L -5 200"/>
          </g>
        </svg>"##;

        let doc = import_svg(svg).expect("import_svg with multilayer and page-background");
        assert_eq!(doc.pages[0].background, Some(FillStyle::uniform(Color::rgb8(0xf4, 0xf8, 0xff))));
        assert_eq!(doc.pages[0].layers.len(), 2);
        assert_eq!(doc.pages[0].layers[0].name, "Base Layer");
        assert_eq!(doc.pages[0].layers[0].objects.len(), 1);
        assert_eq!(doc.pages[0].layers[1].name, "Overlay Layer");
        assert!((doc.pages[0].layers[1].opacity.get() - 0.75).abs() < 1e-6);
        assert_eq!(doc.pages[0].layers[1].objects.len(), 1);
    }

    #[test]
    fn import_dxf_multilayer_block_insert_polyline_bulge_and_lineweight_m80() {
        use coralcraft_geom::PathElement;

        let dxf = concat!(
            "0\nSECTION\n2\nBLOCKS\n",
            "0\nBLOCK\n2\nSTAMP\n10\n0.0\n20\n0.0\n",
            "0\nLINE\n8\n0\n62\n1\n370\n50\n10\n0.0\n20\n0.0\n11\n40.0\n21\n0.0\n",
            "0\nENDBLK\n",
            "0\nENDSEC\n",
            "0\nSECTION\n2\nENTITIES\n",
            "0\nPOLYLINE\n8\nBorderLayer\n62\n5\n370\n100\n70\n1\n",
            "0\nVERTEX\n10\n10.0\n20\n100.0\n42\n1.0\n",
            "0\nVERTEX\n10\n90.0\n20\n100.0\n",
            "0\nSEQEND\n",
            "0\nINSERT\n8\nStampLayer\n2\nSTAMP\n10\n50.0\n20\n200.0\n41\n2.0\n42\n2.0\n50\n90.0\n",
            "0\nENDSEC\n0\nEOF\n"
        );

        let doc = import_dxf(dxf).expect("import_dxf M80");
        assert_eq!(doc.pages[0].layers.len(), 2);
        assert_eq!(doc.pages[0].layers[0].name, "BorderLayer");
        assert_eq!(doc.pages[0].layers[0].objects.len(), 1);
        let poly_node = &doc.pages[0].layers[0].objects[0];
        let poly_outl = poly_node.outline.as_ref().expect("POLYLINE outline");
        assert_eq!(poly_outl.color, Color::rgb8(0, 0, 255));
        assert!((poly_outl.width_pt.get() - (1.0 * 72.0 / 25.4)).abs() < 1e-2);
        let ObjectKind::Path(poly_path) = &poly_node.kind else {
            panic!("expected Path for POLYLINE");
        };
        assert!(poly_path.elements.iter().any(|e| matches!(e, PathElement::CubicTo { .. })), "POLYLINE VERTEX 42 bulge must emit cubic arcs");

        assert_eq!(doc.pages[0].layers[1].name, "StampLayer");
        assert_eq!(doc.pages[0].layers[1].objects.len(), 1);
        let ins_node = &doc.pages[0].layers[1].objects[0];
        let ObjectKind::Group { children } = &ins_node.kind else {
            panic!("expected Group for INSERT block instance");
        };
        assert_eq!(children.len(), 1);
        let child_outl = children[0].outline.as_ref().expect("BLOCK LINE outline");
        assert_eq!(child_outl.color, Color::rgb8(255, 0, 0));
        assert!((ins_node.transform.tx.get() - 50.0).abs() < 1e-3);
    }
}
