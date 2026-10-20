//! Hostile-input tests for the document importers and the `.coralcraft` loader.
//!
//! Every input is generated in code (no fixture files). The mutation loops are deterministic
//! (SplitMix64 with fixed seeds) so a failure always reproduces.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing, clippy::arithmetic_side_effects)]

use coralcraft_doc::{Document, import_dxf, import_svg, load_bytes, save_json, save_zip};

struct SplitMix64(u64);

impl SplitMix64 {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % (n.max(1) as u64)) as usize
    }
}

/// Applies 1-4 random byte edits (overwrite, insert, delete, truncate) to `seed`.
fn mutate(rng: &mut SplitMix64, seed: &[u8]) -> Vec<u8> {
    let mut v = seed.to_vec();
    for _ in 0..=rng.below(4) {
        match rng.below(4) {
            0 if !v.is_empty() => {
                let i = rng.below(v.len());
                v[i] = rng.next() as u8;
            }
            1 => {
                let i = rng.below(v.len() + 1);
                v.insert(i, rng.next() as u8);
            }
            2 if !v.is_empty() => {
                let i = rng.below(v.len());
                v.remove(i);
            }
            _ => {
                let n = rng.below(v.len() + 1);
                v.truncate(n);
            }
        }
    }
    v
}

#[test]
fn svg_import_never_panics_on_multibyte_hex_colors() {
    // "#aé€" is 6 bytes long but not 6 ASCII hex digits; byte slicing used to split a UTF-8 char.
    for color in ["#aé€", "#€€", "#ééé", "#\u{1F600}aa"] {
        let svg =
            format!(r#"<svg xmlns="http://www.w3.org/2000/svg"><rect x="1" y="2" width="3" height="4" fill="{color}" stroke="{color}"/></svg>"#);
        let _ = import_svg(&svg);
    }
}

#[test]
fn svg_and_dxf_import_survive_deterministic_mutations() {
    let svg_seed = r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100"><rect x="10" y="10" width="50" height="30" rx="4" fill="#ff0000" stroke="#0000ff"/><ellipse cx="100" cy="50" rx="20" ry="10" fill="#00ff00"/><path d="M 10 10 L 50 50 C 60 60 70 70 80 80 Z" fill="none" stroke="#000000"/><text x="5" y="90">Hi é</text></svg>"##;
    let dxf_seed = "0\nSECTION\n2\nENTITIES\n0\nLINE\n8\n0\n10\n0\n20\n0\n11\n100\n21\n50\n0\nCIRCLE\n8\n0\n10\n50\n20\n50\n40\n25\n0\nLWPOLYLINE\n8\n0\n90\n3\n10\n0\n20\n0\n10\n10\n20\n0\n10\n10\n20\n10\n0\nENDSEC\n0\nEOF\n";
    let mut rng = SplitMix64(0xC0FA_1C4A_F7D0_0001);
    for _ in 0..3_000 {
        let m = mutate(&mut rng, svg_seed.as_bytes());
        let _ = import_svg(&String::from_utf8_lossy(&m));
        let m = mutate(&mut rng, dxf_seed.as_bytes());
        let _ = import_dxf(&String::from_utf8_lossy(&m));
    }
}

#[test]
fn coralcraft_loader_survives_deterministic_mutations_of_json_and_zip() {
    let doc = Document::new_a4_portrait();
    let json = save_json(&doc).unwrap();
    let zip = save_zip(&doc).unwrap();
    let mut rng = SplitMix64(0xC0FA_1C4A_F7D0_0002);
    for _ in 0..2_000 {
        let _ = load_bytes(&mutate(&mut rng, &json));
        let _ = load_bytes(&mutate(&mut rng, &zip));
    }
}

fn doc_json_with(edit: impl FnOnce(&mut serde_json::Value)) -> Vec<u8> {
    let mut doc = Document::new_a4_portrait();
    let a = doc.allocate_object_id();
    let b = doc.allocate_object_id();
    let rect = |id| {
        coralcraft_doc::ObjectNode::new(
            id,
            None,
            coralcraft_doc::ObjectKind::Rect(coralcraft_geom::RectSpec::sharp(coralcraft_geom::Rect::new(0.0, 0.0, 10.0, 10.0).unwrap())),
            None,
            None,
        )
    };
    doc.pages[0].layers[0].objects.push(rect(a));
    doc.pages[0].layers[0].objects.push(rect(b));
    let mut v: serde_json::Value = serde_json::from_slice(&save_json(&doc).unwrap()).unwrap();
    edit(&mut v);
    serde_json::to_vec(&v).unwrap()
}

#[test]
fn loader_rejects_documents_without_pages() {
    let bytes = doc_json_with(|v| v["pages"] = serde_json::json!([]));
    assert!(matches!(load_bytes(&bytes), Err(coralcraft_doc::DocError::InvalidDocument(_))));
}

#[test]
fn loader_rejects_duplicate_object_ids() {
    let bytes = doc_json_with(|v| {
        let id = v["pages"][0]["layers"][0]["objects"][0]["id"].clone();
        v["pages"][0]["layers"][0]["objects"][1]["id"] = id;
    });
    assert!(matches!(load_bytes(&bytes), Err(coralcraft_doc::DocError::InvalidDocument(_))));
}

#[test]
fn loader_recomputes_a_stale_next_id_so_new_ids_never_collide() {
    let bytes = doc_json_with(|v| v["next_id"] = serde_json::json!(1));
    let mut doc = load_bytes(&bytes).unwrap();
    let fresh = doc.allocate_object_id();
    let mut ids = Vec::new();
    for layer in &doc.pages[0].layers {
        ids.push(layer.id.0);
        ids.extend(layer.objects.iter().map(|o| o.id.0));
    }
    ids.push(doc.pages[0].id.0);
    assert!(ids.iter().all(|&id| id < fresh.0), "fresh id {} collides with existing ids {ids:?}", fresh.0);
}

#[test]
fn loader_rejects_ids_at_the_top_of_the_id_space() {
    let bytes = doc_json_with(|v| v["pages"][0]["layers"][0]["objects"][0]["id"] = serde_json::json!(u64::MAX));
    assert!(matches!(load_bytes(&bytes), Err(coralcraft_doc::DocError::InvalidDocument(_))));
}
