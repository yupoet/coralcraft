//! Integration tests for `coralcraft-cli probe` (M1 Task 11).
//!
//! Clean-room note: all synthetic test byte streams are written to the system temporary
//! directory (`std::env::temp_dir()`) and removed on drop; no binary test files are ever
//! stored inside the repository tree.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(1);

struct TempFixture {
    path: PathBuf,
}

impl TempFixture {
    fn new(tag: &str, bytes: &[u8]) -> Self {
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let path = std::env::temp_dir().join(format!("coralcraft_cli_test_{pid}_{id}_{tag}.bin"));
        fs::write(&path, bytes).expect("failed to write temporary test fixture");
        Self { path }
    }
}

impl Drop for TempFixture {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn cli_bin() -> &'static str {
    env!("CARGO_BIN_EXE_coralcraft-cli")
}

fn synth_riff_cdr7_with_vrsn() -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(b"RIFF");
    v.extend_from_slice(&14u32.to_le_bytes()); // form(4) + vrsn chunk(10)
    v.extend_from_slice(b"CDR7");
    v.extend_from_slice(b"vrsn");
    v.extend_from_slice(&2u32.to_le_bytes());
    v.extend_from_slice(&700u16.to_le_bytes());
    v
}

#[test]
fn cli_probe_human_readable_riff_cdr7() {
    let f = TempFixture::new("riff7", &synth_riff_cdr7_with_vrsn());
    let out = Command::new(cli_bin()).arg("probe").arg(&f.path).output().expect("failed to run coralcraft-cli");

    assert_eq!(out.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("RIFF"), "stdout should contain RIFF: {stdout}");
    assert!(stdout.contains("CDR7"), "stdout should contain CDR7: {stdout}");
    assert!(stdout.contains("700"), "stdout should contain version 700: {stdout}");
    assert!(stdout.contains("vrsn"), "stdout should contain chunk vrsn: {stdout}");
}

#[test]
fn cli_probe_json_roundtrip() {
    let f = TempFixture::new("json7", &synth_riff_cdr7_with_vrsn());
    let out = Command::new(cli_bin()).args(["probe", "--json"]).arg(&f.path).output().expect("failed to run coralcraft-cli");

    assert_eq!(out.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let val: serde_json::Value = serde_json::from_slice(&out.stdout).expect("--json output should be valid JSON");
    assert_eq!(val["schema_version"], 1);
    assert_eq!(val["container"]["container_kind"], "riff");
    assert_eq!(val["container"]["form"], "CDR7");
    assert_eq!(val["container"]["version"]["effective_version"], 700);
    assert_eq!(val["chunks"][0]["fourcc"], "vrsn");
}

#[test]
fn cli_probe_non_cdr_exits_with_code_3() {
    let f = TempFixture::new("not_cdr", b"%PDF-1.7 not a coreldraw file");
    let out = Command::new(cli_bin()).arg("probe").arg(&f.path).output().expect("failed to run coralcraft-cli");

    assert_eq!(out.status.code(), Some(3), "non-CDR file should exit with code 3 (distinct from clap usage error 2)");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("not a CDR"), "stderr: {stderr}");
}

#[test]
fn cli_probe_truncated_exits_with_code_5() {
    let mut broken = Vec::new();
    broken.extend_from_slice(b"RIFF");
    broken.extend_from_slice(&100u32.to_le_bytes()); // declares 100 bytes when only 12 bytes exist
    broken.extend_from_slice(b"CDR7");
    let f = TempFixture::new("truncated", &broken);

    let out = Command::new(cli_bin()).arg("probe").arg(&f.path).output().expect("failed to run coralcraft-cli");
    assert_eq!(out.status.code(), Some(5), "truncated/corrupted container should exit with code 5");
}

#[test]
fn cli_render_and_inspect_palette_subcommands() {
    let mut doc = coralcraft_doc::Document::new_a4_portrait();
    doc.pages[0].size = coralcraft_geom::Size::new(32.0, 24.0).unwrap();
    let zip_bytes = coralcraft_doc::save_zip(&doc).unwrap();
    let in_doc = TempFixture::new("sample.coralcraft", &zip_bytes);
    let out_png = TempFixture::new("rendered.png", b"");

    let render_out = Command::new(cli_bin())
        .args(["render", "--json", "-o"])
        .arg(&out_png.path)
        .arg(&in_doc.path)
        .output()
        .expect("failed to run coralcraft-cli render");
    assert_eq!(render_out.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&render_out.stderr));
    let r_json: serde_json::Value = serde_json::from_slice(&render_out.stdout).unwrap();
    assert_eq!(r_json["width"], 32);
    assert_eq!(r_json["height"], 24);
    let png_on_disk = fs::read(&out_png.path).unwrap();
    assert!(png_on_disk.starts_with(b"\x89PNG\r\n\x1a\n"));

    // Test inspect-palette --json
    let xml_pal = b"<palette name=\"CLI Test\"><colors><cs type=\"1\" name=\"Red\" r=\"255\" g=\"0\" b=\"0\"/></colors></palette>";
    let pal_file = TempFixture::new("pal.xml", xml_pal);
    let pal_out = Command::new(cli_bin())
        .args(["inspect-palette", "--json"])
        .arg(&pal_file.path)
        .output()
        .expect("failed to run coralcraft-cli inspect-palette");
    assert_eq!(pal_out.status.code(), Some(0));
    let p_json: serde_json::Value = serde_json::from_slice(&pal_out.stdout).unwrap();
    assert_eq!(p_json["name"], "CLI Test");
    assert_eq!(p_json["swatch_count"], 1);
    assert_eq!(p_json["swatches"][0]["srgb_hex"], "#ff0000");
}

#[test]
fn cli_export_cdr_to_svg_pdf_png_and_coralcraft() {
    let cdr_file = TempFixture::new("input.cdr", &synth_riff_cdr7_with_vrsn());
    let out_svg = std::env::temp_dir().join(format!("coralcraft_cli_export_{}.svg", std::process::id()));
    let out_pdf = std::env::temp_dir().join(format!("coralcraft_cli_export_{}.pdf", std::process::id()));
    let out_cc = std::env::temp_dir().join(format!("coralcraft_cli_export_{}.coralcraft", std::process::id()));

    // 1. Export .cdr -> .svg
    let res_svg = Command::new(cli_bin()).args(["export", "--json", "-o"]).arg(&out_svg).arg(&cdr_file.path).output().unwrap();
    assert_eq!(res_svg.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&res_svg.stderr));
    let svg_text = fs::read_to_string(&out_svg).unwrap();
    assert!(svg_text.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\""));
    let _ = fs::remove_file(&out_svg);

    // 2. Export .cdr -> .pdf
    let res_pdf = Command::new(cli_bin()).args(["export", "-o"]).arg(&out_pdf).arg(&cdr_file.path).output().unwrap();
    assert_eq!(res_pdf.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&res_pdf.stderr));
    let pdf_bytes = fs::read(&out_pdf).unwrap();
    assert!(pdf_bytes.starts_with(b"%PDF-1.4\n"));
    let _ = fs::remove_file(&out_pdf);

    // 3. Export .cdr -> .coralcraft and reload
    let res_cc = Command::new(cli_bin()).args(["export", "-o"]).arg(&out_cc).arg(&cdr_file.path).output().unwrap();
    assert_eq!(res_cc.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&res_cc.stderr));
    let cc_bytes = fs::read(&out_cc).unwrap();
    let loaded = coralcraft_doc::load_bytes(&cc_bytes).expect("saved .coralcraft must load cleanly");
    assert_eq!(loaded.pages.len(), 1);
    let _ = fs::remove_file(&out_cc);
}
