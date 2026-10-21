//! Integration tests for `coralcraft-cli probe`: chunk-level inspection of `.cdr`/`.cmx`
//! containers (ported from the pre-fork CLI, V0.4).
//!
//! Clean-room note: all synthetic test byte streams are written to the system temporary
//! directory and removed on drop; no binary test files are stored in the repository tree.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing, clippy::arithmetic_side_effects)]

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

    assert_eq!(out.status.code(), Some(3), "non-CDR file should exit with code 3");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("not a CDR"), "stderr: {stderr}");
}

#[test]
fn cli_probe_truncated_exits_with_code_5() {
    let mut broken = Vec::new();
    broken.extend_from_slice(b"RIFF");
    broken.extend_from_slice(&100u32.to_le_bytes()); // declares 100 bytes when only 12 exist
    broken.extend_from_slice(b"CDR7");
    let f = TempFixture::new("truncated", &broken);

    let out = Command::new(cli_bin()).arg("probe").arg(&f.path).output().expect("failed to run coralcraft-cli");
    assert_eq!(out.status.code(), Some(5), "truncated/corrupted container should exit with code 5");
}
