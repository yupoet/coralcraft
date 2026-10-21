//! `cargo xtask cleanroom`: no proprietary Corel material enters this repository (AGENTS.md ›
//! Clean-Room). CoralCraft is a clean-room, open-source CorelDRAW-compatible application: its
//! behavior is derived from public documentation and operator-owned sample files only, never from
//! Corel software, resources, or authored files.
//!
//! Scans every tracked text file for Corel copyright markers, and rejects binary `.cdr`/`.cmx`
//! fixtures: all CDR/CMX test inputs are handcrafted from public specifications (Photopea CDR
//! specification, Kaitai Struct `coreldraw_cdr.ksy`, RIFF, PKWARE APPNOTE, PDF/ISO 32000). The
//! git-ignored `refs/` directory (external reference code) never reaches this scan.

use std::path::Path;

/// Prohibited Corel copyright markers.
const NEEDLES: &[&str] = &["Copyright (c) Corel", "Corel Corporation", "© Corel"];

/// Extensions of real Corel document fixtures that must never be committed.
const FIXTURE_EXTS: &[&str] = &["cdr", "cmx"];

/// This file lists the markers it rejects.
const SELF: &str = "xtask/src/cleanroom.rs";

pub fn run(root: &Path) -> Result<(), String> {
    // Outside a git checkout there is no file list to check.
    let Ok(files) = crate::repo_files(root) else {
        println!("cleanroom: skipped (not a git checkout)");
        return Ok(());
    };
    let offenders = scan(root, &files);
    if offenders.is_empty() {
        println!("cleanroom: ok ({} files scanned, no Corel copyright markers or .cdr/.cmx fixtures)", files.len());
        Ok(())
    } else {
        Err(format!("{} clean-room violation(s) (AGENTS.md › Clean-Room):\n  {}", offenders.len(), offenders.join("\n  ")))
    }
}

/// Every violation in `files` (paths relative to `root`).
pub fn scan(root: &Path, files: &[String]) -> Vec<String> {
    let mut offenders = vec![];
    for path in files {
        if path == SELF {
            continue;
        }
        let ext = Path::new(path).extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();
        if FIXTURE_EXTS.contains(&ext.as_str()) {
            offenders.push(format!("{path}: binary .cdr/.cmx fixture (test inputs are handcrafted, never real Corel files)"));
            continue;
        }
        // Binary files fail UTF-8 reading and have nothing to scan.
        let Ok(text) = std::fs::read_to_string(root.join(path)) else { continue };
        for needle in NEEDLES {
            if text.contains(needle) {
                offenders.push(format!("{path}: Corel copyright marker {needle:?}"));
            }
        }
    }
    offenders
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_markers_and_binary_fixtures() {
        let dir = std::env::temp_dir().join(format!("coralcraft-cleanroom-{}", std::process::id()));
        let file = |p: &str, text: &str| {
            let path = dir.join(p);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        file("docs/clean.md", "a clean document\n");
        std::fs::write(dir.join("docs/binary.cdr"), b"RIFF\x04\x00\x00\x00CDRD").unwrap();
        // Construct the marker at runtime so this test source stays scannable itself.
        file("docs/bad.md", format!("bad: {} (c) {}", "Copyright", "Corel").as_str());
        let files: Vec<String> = ["docs/clean.md", "docs/binary.cdr", "docs/bad.md"].map(String::from).to_vec();
        let offenders = scan(&dir, &files);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(offenders.len(), 2, "{offenders:#?}");
        assert!(offenders.iter().any(|o| o.contains("binary .cdr/.cmx fixture")));
        assert!(offenders.iter().any(|o| o.contains("copyright marker")));
    }

    #[test]
    fn the_tree_is_clean() {
        let root = crate::root();
        let Ok(files) = crate::repo_files(&root) else { return };
        let offenders = scan(&root, &files);
        assert!(offenders.is_empty(), "{offenders:#?}");
    }
}
