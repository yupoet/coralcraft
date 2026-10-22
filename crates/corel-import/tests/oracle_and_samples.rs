//! Differential `libcdr` (`cdr2raw` / `cmx2raw`) oracle and real-sample (`refs/cdr-samples`)
//! verification on the `corel-import` + `vectorcraft-doc` pipeline (Phase V task V1.4).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing, clippy::arithmetic_side_effects)]

use std::process::Command;
use std::sync::Arc;

use coralcraft_cdr::{parse_document, probe};
use corel_import::{document_metrics, import_cdr_bytes, write_cdr7_document, write_cdrx_zip_document, write_cmx1_document};
use vectorcraft_color::{Color, Paint};
use vectorcraft_doc::{Appearance, AppearanceItem, Artboard, Document, FillLayer, LayerColor, LiveShape, Node, NodeId, NodeKind, StrokeLayer};
use vectorcraft_geom::{
    Affine, BezPath, FillRule, PathData, Point, Rect,
    shapes::{self, CornerKind},
};

fn find_tool(name: &str) -> Option<String> {
    [name.to_owned(), format!("/home/paris/.local/bin/{name}")]
        .into_iter()
        .find(|candidate| Command::new(candidate).arg("--version").output().is_ok())
}

fn solid_appearance(fill: Color, stroke: Color, stroke_width: f64) -> Appearance {
    Appearance {
        items: vec![
            AppearanceItem::Fill(FillLayer::new(Paint::solid(fill))),
            AppearanceItem::Stroke(StrokeLayer::new(Paint::solid(stroke), stroke_width)),
        ],
        effects: Vec::new(),
        contents_index: None,
    }
}

fn build_vectorcraft_interop_document() -> Document {
    let mut doc = Document::new(595.275_59, 841.889_76);
    doc.artboards = vec![Artboard {
        id: 1,
        name: "Page 1".to_owned(),
        rect: Rect::new(0.0, 0.0, 595.275_59, 841.889_76),
        show_center_mark: false,
        show_cross_hairs: false,
    }];

    // 1. Sharp rectangle with uniform RGB fill and stroke
    let rect_live = LiveShape::Rectangle { w: 150.0, h: 100.0, radii: [0.0; 4], kinds: [CornerKind::Round; 4], xf: Affine::translate((50.0, 60.0)) };
    let rect_path = rect_live.to_path();
    let mut rect_node = Node::new(
        NodeId(10),
        NodeKind::Path { path: rect_path, rule: FillRule::NonZero, live: Some(Box::new(rect_live)), clipping: false, guide: false },
    );
    rect_node.name = Some("RedRect".to_owned());
    rect_node.appearance = solid_appearance(Color::rgb8(220, 40, 30), Color::rgb8(20, 20, 20), 2.0).into();

    // 2. Full ellipse with uniform CMYK fill and hairline stroke
    let ell_live = LiveShape::Ellipse { w: 140.0, h: 120.0, pie: (0.0, 360.0), xf: Affine::translate((240.0, 60.0)) };
    let ell_path = ell_live.to_path();
    let mut ell_node = Node::new(
        NodeId(11),
        NodeKind::Path { path: ell_path, rule: FillRule::NonZero, live: Some(Box::new(ell_live)), clipping: false, guide: false },
    );
    ell_node.name = Some("CyanEllipse".to_owned());
    ell_node.appearance = solid_appearance(Color::cmyk(0.8, 0.1, 0.0, 0.0), Color::BLACK, 0.2).into();

    // 3. Bezier + line path
    let mut bez = BezPath::new();
    bez.move_to(Point::new(60.0, 220.0));
    bez.line_to(Point::new(180.0, 220.0));
    bez.curve_to(Point::new(210.0, 250.0), Point::new(190.0, 310.0), Point::new(120.0, 320.0));
    bez.close_path();
    let mut path_node = Node::path(NodeId(12), PathData::from_bezpath(&bez), solid_appearance(Color::rgb8(40, 170, 90), Color::BLACK, 1.5));
    path_node.name = Some("BezierCurve".to_owned());

    // 4. 5-pointed star polygon
    let star_path = shapes::star(Point::new(310.0, 280.0), 55.0, 25.0, 5, 0.0);
    let mut star_node = Node::path(NodeId(13), star_path, solid_appearance(Color::rgb8(240, 190, 20), Color::BLACK, 0.2));
    star_node.name = Some("GoldStar".to_owned());

    let mut layer = Node::layer(NodeId(1), "Layer 1", LayerColor::Preset(0));
    if let NodeKind::Layer { children, .. } = &mut layer.kind {
        *children = vec![Arc::new(rect_node), Arc::new(ell_node), Arc::new(path_node), Arc::new(star_node)];
    }
    doc.layers = vec![Arc::new(layer)];
    doc
}

fn count_cdr_objects(objects: &[coralcraft_doc::ObjectNode]) -> usize {
    let mut total = 0usize;
    for obj in objects {
        total += 1;
        match &obj.kind {
            coralcraft_doc::ObjectKind::Group { children } => {
                total += count_cdr_objects(children);
            }
            coralcraft_doc::ObjectKind::Unsupported(u) if !u.children.is_empty() => {
                total += count_cdr_objects(&u.children);
            }
            _ => {}
        }
        if !obj.powerclip_children.is_empty() {
            total += count_cdr_objects(&obj.powerclip_children);
        }
    }
    total
}

#[test]
fn write_cdr7_cdrx_and_cmx1_pass_libcdr_oracle_on_vectorcraft_document() {
    let vdoc = build_vectorcraft_interop_document();

    let cdr7_bytes = write_cdr7_document(&vdoc).expect("write_cdr7_document should succeed");
    let cdrx_bytes = write_cdrx_zip_document(&vdoc).expect("write_cdrx_zip_document should succeed");
    let cmx1_bytes = write_cmx1_document(&vdoc).expect("write_cmx1_document should succeed");

    for (label, bytes) in [("cdr7", &cdr7_bytes), ("cdrx", &cdrx_bytes), ("cmx1", &cmx1_bytes)] {
        let _rep = probe(bytes).unwrap_or_else(|e| panic!("probe failed on {label}: {e}"));
        let imported = import_cdr_bytes(bytes).unwrap_or_else(|e| panic!("import_cdr_bytes failed on {label}: {e}"));
        let metrics = document_metrics(&imported.document);
        assert_eq!(metrics.page_count, 1, "{label} page_count");
        assert_eq!(metrics.layer_count, 1, "{label} layer_count");
        assert_eq!(metrics.object_count, 4, "{label} object_count");
    }

    if let Some(cdr2raw) = find_tool("cdr2raw") {
        let tmp = std::env::temp_dir().join(format!("corel_import_oracle_cdr7_{}.cdr", std::process::id()));
        std::fs::write(&tmp, &cdr7_bytes).unwrap();
        let out = Command::new(&cdr2raw).arg(&tmp).output().unwrap();
        let _ = std::fs::remove_file(&tmp);
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(out.status.success(), "cdr2raw failed on cdr7: {}", String::from_utf8_lossy(&out.stderr));
        assert!(stdout.contains("startDocument"), "cdr2raw cdr7 missing startDocument");
        assert_eq!(stdout.matches("startPage").count(), 1, "cdr2raw cdr7 page count");
        let draw_path_count = stdout.matches("drawPath").count();
        assert!(draw_path_count >= 4, "expected >= 4 drawPath calls in cdr2raw cdr7, got {draw_path_count}");
    }

    if let Some(cmx2raw) = find_tool("cmx2raw") {
        let tmp = std::env::temp_dir().join(format!("corel_import_oracle_cmx1_{}.cmx", std::process::id()));
        std::fs::write(&tmp, &cmx1_bytes).unwrap();
        let out = Command::new(&cmx2raw).arg(&tmp).output().unwrap();
        let _ = std::fs::remove_file(&tmp);
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(out.status.success(), "cmx2raw failed: {}", String::from_utf8_lossy(&out.stderr));
        assert!(stdout.contains("startDocument"), "cmx2raw missing startDocument");
        assert_eq!(stdout.matches("startPage").count(), 1, "cmx2raw page count");
        let draw_path_count = stdout.matches("drawPath").count();
        assert!(draw_path_count >= 4, "expected >= 4 drawPath calls in cmx2raw, got {draw_path_count}");
    }
}

#[test]
fn real_world_cdr_samples_open_into_vectorcraft_doc_with_exact_counts() {
    let samples_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../refs/cdr-samples");
    if !samples_dir.is_dir() {
        return;
    }
    let mut stack = vec![samples_dir];
    let mut sample_count = 0usize;
    let mut non_empty_object_samples = 0usize;

    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|s| s.to_str()).is_some_and(|ext| ext.eq_ignore_ascii_case("cdr")) {
                let bytes = std::fs::read(&path).expect("read sample");
                if probe(&bytes).is_err() {
                    continue;
                }
                let parsed = parse_document(&bytes).unwrap_or_else(|e| panic!("parse_document failed on {}: {e}", path.display()));
                let imported = import_cdr_bytes(&bytes).unwrap_or_else(|e| panic!("import_cdr_bytes failed on {}: {e}", path.display()));

                let expected_pages = parsed.document.pages.len();
                let master_layers_with_objs = parsed.document.master_layers.iter().filter(|l| !l.objects.is_empty()).count();
                let page_layers: usize = parsed.document.pages.iter().map(|p| p.layers.len()).sum();
                let expected_layers = (page_layers + master_layers_with_objs).max(1);
                let expected_objects: usize = parsed
                    .document
                    .pages
                    .iter()
                    .flat_map(|p| &p.layers)
                    .chain(parsed.document.master_layers.iter().filter(|l| !l.objects.is_empty()))
                    .map(|l| count_cdr_objects(&l.objects))
                    .sum();

                let metrics = document_metrics(&imported.document);
                assert!(metrics.page_count >= 1, "{} must have at least 1 page", path.display());
                assert!(metrics.layer_count >= 1, "{} must have at least 1 layer", path.display());
                assert_eq!(metrics.page_count, expected_pages, "{} page_count mismatch", path.display());
                assert_eq!(metrics.layer_count, expected_layers, "{} layer_count mismatch", path.display());
                assert_eq!(metrics.object_count, expected_objects, "{} object_count mismatch", path.display());

                if metrics.object_count > 0 {
                    non_empty_object_samples += 1;
                }
                sample_count += 1;
            }
        }
    }

    assert!(sample_count >= 64, "expected at least 64 valid .cdr samples, got {sample_count}");
    assert!(
        non_empty_object_samples >= 20,
        "expected at least 20 real-world .cdr samples to yield non-zero imported objects, got {non_empty_object_samples}/{sample_count}"
    );
}
