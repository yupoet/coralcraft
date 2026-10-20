//! Differential oracle and truncation-safety tests for `write_cdr7` and `write_cmx1` (`R7.1`).
//!
//! Validates that files produced by `write_cdr7` and `write_cmx1` conform to the public CDR/CMX
//! specifications, round-trip through `parse_document`, are accepted by `libcdr`'s `cdr2raw` and
//! `cmx2raw` tools when available on `$PATH`, and return `Err` instead of silently truncating
//! oversized paths.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing, clippy::arithmetic_side_effects)]

use std::process::Command;

use coralcraft_cdr::{ProbeError, parse_document, probe, write_cdr7, write_cdrx_zip, write_cmx1};
use coralcraft_color::Color;
use coralcraft_doc::{Document, FillStyle, ObjectId, ObjectKind, ObjectNode, OutlineStyle};
use coralcraft_geom::{EllipseSpec, F64, Path, Point, PolygonSpec, Rect, RectSpec};

fn build_interop_test_document() -> Document {
    let mut doc = Document::new_a4_portrait();
    let layer = &mut doc.pages[0].layers[0];

    // 1. Sharp rectangle with uniform RGB fill and outline
    let rect = Rect::new(50.0, 60.0, 200.0, 160.0).unwrap();
    layer.objects.push(ObjectNode::new(
        ObjectId(10),
        Some("RedRect".to_owned()),
        ObjectKind::Rect(RectSpec::sharp(rect)),
        Some(FillStyle::uniform(Color::rgb8(220, 40, 30))),
        Some(OutlineStyle::with_width(Color::rgb8(20, 20, 20), F64::new(2.0).unwrap())),
    ));

    // 2. Full ellipse with uniform CMYK fill
    let ell = Rect::new(240.0, 60.0, 380.0, 180.0).unwrap();
    layer.objects.push(ObjectNode::new(
        ObjectId(11),
        Some("CyanEllipse".to_owned()),
        ObjectKind::Ellipse(EllipseSpec::full(ell)),
        Some(FillStyle::uniform(Color::cmyk(0.8, 0.1, 0.0, 0.0).unwrap())),
        Some(OutlineStyle::hairline(Color::BLACK)),
    ));

    // 3. Bezier + line path
    let mut path = Path::new();
    path.move_to(Point::new(60.0, 220.0).unwrap()).unwrap();
    path.line_to(Point::new(180.0, 220.0).unwrap()).unwrap();
    path.cubic_to(Point::new(210.0, 250.0).unwrap(), Point::new(190.0, 310.0).unwrap(), Point::new(120.0, 320.0).unwrap()).unwrap();
    path.close().unwrap();
    layer.objects.push(ObjectNode::new(
        ObjectId(12),
        Some("BezierCurve".to_owned()),
        ObjectKind::Path(path),
        Some(FillStyle::uniform(Color::rgb8(40, 170, 90))),
        Some(OutlineStyle::with_width(Color::BLACK, F64::new(1.5).unwrap())),
    ));

    // 4. 5-pointed star polygon
    let poly = PolygonSpec::star(Point::new(310.0, 280.0).unwrap(), 55.0, 25.0, 5, 0.0).unwrap();
    layer.objects.push(ObjectNode::new(
        ObjectId(13),
        Some("GoldStar".to_owned()),
        ObjectKind::Polygon(poly),
        Some(FillStyle::uniform(Color::rgb8(240, 190, 20))),
        Some(OutlineStyle::hairline(Color::BLACK)),
    ));

    doc.next_id = 50;
    doc
}

fn find_tool(name: &str) -> Option<String> {
    [name.to_owned(), format!("/home/paris/.local/bin/{name}")]
        .into_iter()
        .find(|candidate| Command::new(candidate).arg("--version").output().is_ok())
}

#[test]
fn write_cdr7_and_write_cmx1_pass_libcdr_cdr2raw_and_cmx2raw_oracle_r7_1() {
    let doc = build_interop_test_document();

    let cdr_bytes = write_cdr7(&doc).expect("write_cdr7 should succeed");
    let _cdr_probe = probe(&cdr_bytes).expect("probe should accept write_cdr7 output");
    let cdr_roundtrip = parse_document(&cdr_bytes).expect("parse_document should round-trip write_cdr7");
    assert_eq!(cdr_roundtrip.document.pages[0].layers[0].objects.len(), 4);

    let cdrx_bytes = write_cdrx_zip(&doc).expect("write_cdrx_zip should succeed");
    let cdrx_roundtrip = parse_document(&cdrx_bytes).expect("parse_document should round-trip write_cdrx_zip");
    assert_eq!(cdrx_roundtrip.document.pages[0].layers[0].objects.len(), 4);

    let cmx_bytes = write_cmx1(&doc).expect("write_cmx1 should succeed");
    let _cmx_probe = probe(&cmx_bytes).expect("probe should accept write_cmx1 output");
    let cmx_roundtrip = parse_document(&cmx_bytes).expect("parse_document should round-trip write_cmx1");
    assert_eq!(cmx_roundtrip.document.pages[0].layers[0].objects.len(), 4);

    // If `cdr2raw` is installed, run it as an independent differential oracle
    if let Some(cdr2raw) = find_tool("cdr2raw") {
        let tmp_cdr = std::env::temp_dir().join(format!("coralcraft_oracle_{}.cdr", std::process::id()));
        std::fs::write(&tmp_cdr, &cdr_bytes).unwrap();
        let out = Command::new(&cdr2raw).arg(&tmp_cdr).output().unwrap();
        let _ = std::fs::remove_file(&tmp_cdr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(out.status.success(), "cdr2raw failed with status {:?}: {}", out.status, String::from_utf8_lossy(&out.stderr));
        assert!(stdout.contains("startDocument"), "cdr2raw output missing startDocument:\n{stdout}");
        assert!(stdout.contains("startPage"), "cdr2raw output missing startPage:\n{stdout}");
        let draw_path_count = stdout.matches("drawPath").count();
        assert!(draw_path_count >= 4, "expected at least 4 drawPath calls in cdr2raw output, got {draw_path_count}:\n{stdout}");
    }

    // If `cmx2raw` is installed, run it as an independent differential oracle
    if let Some(cmx2raw) = find_tool("cmx2raw") {
        let tmp_cmx = std::env::temp_dir().join(format!("coralcraft_oracle_{}.cmx", std::process::id()));
        std::fs::write(&tmp_cmx, &cmx_bytes).unwrap();
        let out = Command::new(&cmx2raw).arg(&tmp_cmx).output().unwrap();
        let _ = std::fs::remove_file(&tmp_cmx);
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(out.status.success(), "cmx2raw failed with status {:?}: {}", out.status, String::from_utf8_lossy(&out.stderr));
        assert!(stdout.contains("startDocument"), "cmx2raw output missing startDocument:\n{stdout}");
        assert!(stdout.contains("startPage"), "cmx2raw output missing startPage:\n{stdout}");
        let draw_path_count = stdout.matches("drawPath").count();
        assert!(draw_path_count >= 4, "expected at least 4 drawPath calls in cmx2raw output, got {draw_path_count}:\n{stdout}");
    }
}

#[test]
fn write_cdr7_and_write_cmx1_return_err_instead_of_truncating_oversized_paths_r7_1() {
    let mut doc = Document::new_a4_portrait();
    let mut huge_path = Path::new();
    huge_path.move_to(Point::ZERO).unwrap();
    // 25_000 CubicTo segments = 75_001 CDR curve points > u16::MAX (65_535)
    for i in 1..=25_000 {
        let x = f64::from(i % 200);
        let y = f64::from((i / 200) % 200);
        let pt = Point::new(x, y).unwrap();
        huge_path.cubic_to(pt, pt, pt).unwrap();
    }
    doc.pages[0].layers[0].objects.push(ObjectNode::new(
        ObjectId(10),
        None,
        ObjectKind::Path(huge_path),
        None,
        Some(OutlineStyle::hairline(Color::BLACK)),
    ));

    let cdr_err = write_cdr7(&doc).expect_err("write_cdr7 must reject paths exceeding u16::MAX CDR points");
    assert!(matches!(cdr_err, ProbeError::LimitExceeded { .. }));

    let cmx_err = write_cmx1(&doc).expect_err("write_cmx1 must reject paths exceeding u16::MAX CMX points");
    assert!(matches!(cmx_err, ProbeError::LimitExceeded { .. }));
}
