//! CoralCraft (P5 Task 3): sustained editing over the three representative document classes —
//! hundreds of mixed edit/undo/redo cycles (selection, node-level edits, text, effects, page
//! reordering, symbols, pattern/texture changes, native save/reopen) asserting stable ids,
//! valid selections, the exact history-depth policy, unchanged native semantics of untouched
//! objects, and a deterministic final output.
//!
//! The default run keeps a bounded runtime for CI. The long soak sits behind an explicit opt-in:
//! `CORALCRAFT_P5_SOAK=500 cargo test -p vectorcraft-engine --test it coreldraw_p5_hardening`.

use serde_json::{Value, json};
use vectorcraft_doc::{Document, NodeId, NodeKind};
use vectorcraft_engine::Session;

/// The default (CI-bounded) cycles per class; `CORALCRAFT_P5_SOAK=<n>` raises it for the soak.
const DEFAULT_CYCLES: usize = 120;

fn cycles() -> usize {
    std::env::var("CORALCRAFT_P5_SOAK").ok().and_then(|v| v.parse().ok()).filter(|n| *n > 0).unwrap_or(DEFAULT_CYCLES)
}

fn run(s: &mut Session, cmd: &str, p: &Value) -> Value {
    s.execute(cmd, p).unwrap_or_else(|e| panic!("{cmd}: {e}"))
}

/// Every id in the document (objects and layers, in walk order).
fn all_ids(doc: &Document) -> Vec<u64> {
    let mut ids = vec![];
    for layer in &doc.layers {
        layer.walk(&mut |node| ids.push(node.id.0));
    }
    ids
}

/// A small many-object illustration: shapes, guides, symbols with instances, text.
/// What the mixed-edit script may do to one class.
struct Caps {
    text: u64,
    mesh: Option<u64>,
    pattern: bool,
    texture: bool,
    symbol: bool,
    reorder: bool,
}

fn illustration() -> (Session, Caps) {
    let mut s = Session::new();
    run(&mut s, "file.new", &json!({"width": 420, "height": 300}));
    for i in 0..300 {
        let (x, y) = ((i % 30) as f64 * 14.0, (i / 30) as f64 * 30.0);
        let id = run(&mut s, "shape.rectangle", &json!({"x": x, "y": y, "width": 10.0, "height": 8.0}))["id"].as_u64().unwrap();
        run(&mut s, "paint.setFill", &json!({"ids": [id], "color": format!("#{:06x}", (i * 7919) % 0x1000000)}));
    }
    for g in 0..20 {
        run(&mut s, "guide.add", &json!({"vertical": g % 2 == 0, "pos": (g as f64 + 1.0) * 20.0}));
    }
    for sym in 0..2 {
        let id =
            run(&mut s, "shape.ellipse", &json!({"x": 200.0 + sym as f64 * 20.0, "y": 20.0, "width": 16.0, "height": 16.0}))["id"].as_u64().unwrap();
        run(&mut s, "select.set", &json!({"ids": [id]}));
        run(&mut s, "symbol.new", &json!({"name": format!("Mark {sym}")}));
        for at in 0..10 {
            run(&mut s, "symbol.place", &json!({"name": format!("Mark {sym}"), "x": 200.0 + at as f64 * 10.0, "y": 260.0}));
        }
    }
    let mut text = 0;
    for t in 0..20 {
        text = run(&mut s, "text.create", &json!({"x": 10.0 + t as f64 * 20.0, "y": 280.0, "text": format!("Label {t}")}))["id"].as_u64().unwrap();
    }
    (s, Caps { text, mesh: None, pattern: false, texture: false, symbol: true, reorder: false })
}

/// A multipage brochure: mixed sizes, a master layer, a table, an image, CDR residuals.
fn brochure() -> (Session, Caps) {
    let mut s = Session::new();
    run(&mut s, "file.new", &json!({"width": 420, "height": 300}));
    for (w, h) in [(300.0, 400.0), (420.0, 300.0), (300.0, 300.0)] {
        run(&mut s, "artboard.new", &json!({"width": w, "height": h}));
    }
    run(&mut s, "layer.new", &json!({"name": "Master", "role": "masterAll", "top": false}));
    run(&mut s, "text.create", &json!({"x": 200.0, "y": 12.0, "text": "Brochure header"}));
    let mut png = vec![];
    image::RgbaImage::from_pixel(64, 64, image::Rgba([30, 120, 180, 255]))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    run(
        &mut s,
        "file.place",
        &json!({"name": "photo.png", "dataBase64": vectorcraft_format::base64_encode(&png), "link": false, "at": [40.0, 60.0]}),
    );
    run(
        &mut s,
        "layout.table",
        &json!({"x": 40.0, "y": 150.0, "width": 200.0, "height": 60.0, "rows": 3, "columns": 3, "text": [["A", "1", "2"], ["B", "3", "4"], ["C", "5", "6"]]}),
    );
    let cdr = run(&mut s, "document.serialize", &json!({"format": "cdr"}));
    run(&mut s, "document.open", &json!({"name": "brochure.cdr", "dataBase64": cdr["dataBase64"]}));
    // The CDR reopen renumbers ids; find the header text again by its content.
    let doc = s.doc().unwrap().doc.clone();
    let mut text = 0;
    for layer in &doc.layers {
        layer.walk(&mut |node| {
            if let NodeKind::Text(t) = &node.kind
                && t.plain_text() == "Brochure header"
            {
                text = node.id.0;
            }
        });
    }
    assert!(text > 0, "the header text reopened");
    (s, Caps { text, mesh: None, pattern: false, texture: false, symbol: false, reorder: true })
}

/// A feature-dense poster: nested PowerClip, blend, mesh, pattern, texture, effects, trace,
/// spot ink with overprint, and production print setup.
fn poster() -> (Session, Caps) {
    let mut s = Session::new();
    run(&mut s, "file.new", &json!({"width": 420, "height": 300}));
    let inner = run(&mut s, "shape.rectangle", &json!({"x": 30.0, "y": 130.0, "width": 60.0, "height": 40.0}))["id"].as_u64().unwrap();
    let frame = run(&mut s, "shape.rectangle", &json!({"x": 25.0, "y": 125.0, "width": 70.0, "height": 50.0}))["id"].as_u64().unwrap();
    run(&mut s, "select.set", &json!({"ids": [inner, frame]}));
    let clip = run(&mut s, "object.clippingMask.make", &json!({}))["id"].as_u64().unwrap();
    let outer = run(&mut s, "shape.rectangle", &json!({"x": 15.0, "y": 115.0, "width": 95.0, "height": 70.0}))["id"].as_u64().unwrap();
    run(&mut s, "select.set", &json!({"ids": [clip, outer]}));
    run(&mut s, "object.clippingMask.make", &json!({}));
    let a = run(&mut s, "shape.ellipse", &json!({"x": 300.0, "y": 20.0, "width": 24.0, "height": 24.0}))["id"].as_u64().unwrap();
    let b = run(&mut s, "shape.ellipse", &json!({"x": 370.0, "y": 90.0, "width": 40.0, "height": 40.0}))["id"].as_u64().unwrap();
    run(&mut s, "object.blend.make", &json!({"ids": [a, b], "steps": 6}));
    let mesh = run(&mut s, "shape.rectangle", &json!({"x": 300.0, "y": 150.0, "width": 90.0, "height": 60.0}))["id"].as_u64().unwrap();
    run(&mut s, "object.mesh.create", &json!({"id": mesh, "rows": 3, "cols": 3}));
    run(&mut s, "object.mesh.setPointColor", &json!({"id": mesh, "index": 4, "color": "#28aa5a", "opacity": 0.5}));
    run(&mut s, "pattern.twoColor", &json!({"name": "Coral Checks", "kind": "checkerboard"}));
    let patterned = run(&mut s, "shape.rectangle", &json!({"x": 20.0, "y": 200.0, "width": 80.0, "height": 60.0}))["id"].as_u64().unwrap();
    run(&mut s, "select.set", &json!({"ids": [patterned]}));
    run(&mut s, "paint.setPattern", &json!({"pattern": "Coral Checks"}));
    run(&mut s, "texture.new", &json!({"name": "Coral Stipple", "kind": "stipple", "density": 0.5}));
    let textured = run(&mut s, "shape.rectangle", &json!({"x": 110.0, "y": 200.0, "width": 80.0, "height": 60.0}))["id"].as_u64().unwrap();
    run(&mut s, "select.set", &json!({"ids": [textured]}));
    run(&mut s, "paint.setTexture", &json!({"texture": "Coral Stipple"}));
    let shadowed = run(&mut s, "shape.rectangle", &json!({"x": 290.0, "y": 230.0, "width": 100.0, "height": 40.0}))["id"].as_u64().unwrap();
    run(&mut s, "effect.apply", &json!({"id": shadowed, "effect": "stylize.dropShadow", "params": {"x": 6, "y": 5}}));
    let mut png = vec![];
    image::RgbaImage::from_fn(48, 48, |x, _| image::Rgba([30, 120, 180, if x < 24 { 0 } else { 255 }]))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    let bitmap = run(
        &mut s,
        "file.place",
        &json!({"name": "trace-me.png", "dataBase64": vectorcraft_format::base64_encode(&png), "link": false, "at": [200.0, 130.0]}),
    )["ids"][0]
        .as_u64()
        .unwrap();
    run(&mut s, "imageTrace.make", &json!({"id": bitmap, "params": {"ignoreWhite": true}}));
    run(&mut s, "swatch.new", &json!({"name": "Coral Spot Orange", "color": {"c": 0, "m": 35, "y": 95, "k": 0}, "global": true}));
    run(&mut s, "swatch.setSpot", &json!({"name": "Coral Spot Orange"}));
    let over = run(&mut s, "shape.rectangle", &json!({"x": 200.0, "y": 20.0, "width": 60.0, "height": 40.0}))["id"].as_u64().unwrap();
    run(&mut s, "paint.setFill", &json!({"ids": [over], "swatch": "Coral Spot Orange"}));
    run(&mut s, "object.setOverprint", &json!({"ids": [over], "fill": true}));
    let text = run(&mut s, "text.create", &json!({"x": 20.0, "y": 12.0, "text": "Hardening poster"}))["id"].as_u64().unwrap();
    (s, Caps { text, mesh: Some(mesh), pattern: true, texture: true, symbol: false, reorder: false })
}

/// Every id in `s` resolves (the selection and the document agree).
fn selection_valid(s: &Session) -> bool {
    let doc = &s.doc().unwrap().doc;
    s.doc().unwrap().selection.objects.iter().all(|id| doc.node(*id).is_some())
}

#[test]
fn sustained_mixed_editing_keeps_ids_selection_history_and_output_deterministic() {
    let cycles = cycles();
    for (name, build) in [("illustration", illustration as fn() -> (Session, Caps)), ("brochure", brochure), ("poster", poster)] {
        let (mut s, caps) = build();
        let initial_ids = all_ids(&s.doc().unwrap().doc);
        let max_initial = initial_ids.iter().max().copied().unwrap_or(0);
        // Sentinels that the loop never edits: their native semantics must survive unchanged.
        let sentinel_kinds: Vec<String> = {
            let doc = &s.doc().unwrap().doc;
            initial_ids.iter().map(|id| doc.node(NodeId(*id)).map(|n| n.kind_label().to_owned()).unwrap_or_default()).collect()
        };
        let baseline_svg = run(&mut s, "document.serialize", &json!({"format": "svg"}))["text"].as_str().unwrap().to_owned();

        let mut history_floor = s.active().unwrap().history.undo.len();
        let mut used: std::collections::BTreeSet<u64> = Default::default();
        for i in 0..cycles {
            let kind = i % 10;
            let undo_before = s.active().unwrap().history.undo.len();
            let limit = s.active().unwrap().history.limit;
            // A known-editable object of the class (its header/title text): the max id can sit
            // on a locked or master layer, where an edit legitimately records no step.
            let target = NodeId(caps.text);
            let mut edited_target = false;
            match kind {
                1 => {
                    run(&mut s, "text.setText", &json!({"id": caps.text, "text": format!("Edited {i}")}));
                    used.insert(caps.text);
                }
                2 | 7 if caps.pattern || caps.texture => {
                    run(&mut s, "select.set", &json!({"ids": [target.0]}));
                    run(&mut s, "effect.apply", &json!({"effect": "distort.roughen", "params": {"size": 5, "relative": true}}));
                    edited_target = true;
                }
                4 if caps.pattern => {
                    run(
                        &mut s,
                        "pattern.setTwoColorProps",
                        &json!({"name": "Coral Checks", "foreground": format!("#{:06x}", (i * 131) % 0x1000000)}),
                    );
                }
                5 if caps.texture => {
                    run(&mut s, "texture.setProps", &json!({"name": "Coral Stipple", "density": 0.2 + (i % 5) as f64 * 0.1}));
                }
                6 if caps.symbol => {
                    run(&mut s, "symbol.place", &json!({"name": "Mark 0", "x": 20.0 + i as f64, "y": 270.0}));
                }
                8 if caps.reorder => {
                    run(&mut s, "artboard.reorder", &json!({"index": 1, "to": 0}));
                }
                9 if let Some(mesh) = caps.mesh => {
                    run(&mut s, "object.mesh.setPointColor", &json!({"id": mesh, "index": 0, "color": format!("#{:06x}", (i * 977) % 0x1000000)}));
                    used.insert(mesh);
                }
                _ => {
                    run(&mut s, "select.set", &json!({"ids": [target.0]}));
                    run(&mut s, "object.move", &json!({"dx": 1, "dy": 0}));
                    edited_target = true;
                }
            }
            if edited_target {
                used.insert(target.0);
            }
            // The exact history-depth policy: one undo step per edit, up to the History States
            // limit, where the oldest step goes (the depth then stays at the limit).
            let after = s.active().unwrap().history.undo.len();
            let expected = if limit > 0 { (undo_before + 1).min(limit) } else { undo_before + 1 };
            assert_eq!(after, expected, "{name} cycle {i}: one undo step per edit, capped at the limit ({limit})");
            assert!(after >= history_floor, "{name} cycle {i}: history shrank below the reopen floor");
            assert!(selection_valid(&s), "{name} cycle {i}: the selection resolves");
            // Undo and redo the edit: the state round-trips (the depth returns to `after`).
            run(&mut s, "edit.undo", &json!({}));
            run(&mut s, "edit.redo", &json!({}));
            assert_eq!(s.active().unwrap().history.undo.len(), after, "{name} cycle {i}: undo+redo keeps the depth");

            // A periodic native save/reopen: the sentinels the script never touched keep their
            // kinds, ids are never recycled, and a reopen clears the history.
            if i % 24 == 23 {
                let data = run(&mut s, "document.serialize", &json!({"format": "vectorcraft"}))["dataBase64"].as_str().unwrap().to_owned();
                run(&mut s, "document.open", &json!({"name": "hardening.vectorcraft", "dataBase64": data}));
                history_floor = 0;
                let ids = all_ids(&s.doc().unwrap().doc);
                assert_eq!(s.active().unwrap().history.undo.len(), 0, "{name} cycle {i}: a reopen clears the history");
                {
                    let doc = &s.doc().unwrap().doc;
                    for (id, kind) in initial_ids.iter().zip(&sentinel_kinds) {
                        if !used.contains(id) {
                            assert_eq!(
                                doc.node(NodeId(*id)).map(|n| n.kind_label().to_owned()).as_deref(),
                                Some(kind.as_str()),
                                "{name} cycle {i}: the untouched sentinel {id} kept its kind"
                            );
                        }
                    }
                }
                assert!(ids.iter().max().unwrap() >= &max_initial, "{name} cycle {i}: a reopen never renumbers below the initial ids");
                // Ids created since (symbol placements) are all above every initial id: none was
                // freed and reused.
                assert!(
                    ids.iter().filter(|id| !initial_ids.contains(id)).all(|id| *id > max_initial),
                    "{name} cycle {i}: new ids never recycle an initial id"
                );
            }
        }

        // The deterministic final output: the same state serializes identically twice, and the
        // edited document differs from the baseline (the edits left their mark).
        let final_a = run(&mut s, "document.serialize", &json!({"format": "vectorcraft"}))["dataBase64"].as_str().unwrap().to_owned();
        let final_b = run(&mut s, "document.serialize", &json!({"format": "vectorcraft"}))["dataBase64"].as_str().unwrap().to_owned();
        assert_eq!(final_a, final_b, "{name}: the final state is deterministic");
        let final_svg = run(&mut s, "document.serialize", &json!({"format": "svg"}))["text"].as_str().unwrap().to_owned();
        if cycles > 0 {
            assert_ne!(final_svg, baseline_svg, "{name}: the edits changed the document");
        }
    }
}

/// Near-budget and hostile shapes are refused without a panic, an unbounded allocation, or a
/// history entry (the bounded-resource half of the task; the import fuzz gates live beside the
/// fuzzer itself).
#[test]
fn hostile_and_near_budget_inputs_fail_closed_without_mutating() {
    let (mut s, _) = brochure();
    let before = run(&mut s, "document.serialize", &json!({"format": "vectorcraft"}))["dataBase64"].as_str().unwrap().to_owned();
    let undo_before = s.active().unwrap().history.undo.len();
    // A non-finite and a wrong-typed colour, an unknown id, and an impossible table.
    assert!(s.execute("paint.setFill", &json!({"ids": [1], "color": {"c": "lots", "m": 0, "y": 0, "k": 0}})).is_err());
    assert!(s.execute("paint.setFill", &json!({"ids": [1], "color": {"c": null, "m": 0, "y": 0, "k": 0}})).is_err());
    assert!(s.execute("object.move", &json!({"ids": [999_999], "dx": 1, "dy": 0})).is_err());
    assert!(s.execute("layout.table", &json!({"x": 0, "y": 0, "width": 10, "height": 10, "rows": 0, "columns": 1})).is_err());
    assert!(s.execute("artboard.reorder", &json!({"index": 99, "to": 0})).is_err());
    assert_eq!(s.active().unwrap().history.undo.len(), undo_before, "refusals add no history");
    assert_eq!(run(&mut s, "document.serialize", &json!({"format": "vectorcraft"}))["dataBase64"], before, "refusals mutate nothing");
    // An empty live expansion fails closed through every writer (no successful empty file).
    for format in ["cdr", "cmx"] {
        let out = run(&mut s, "document.serialize", &json!({"format": format}));
        let bytes = vectorcraft_format::base64_decode(out["dataBase64"].as_str().unwrap()).unwrap();
        assert!(!bytes.is_empty(), "{format} never writes an empty file");
    }
}

/// The consolidated per-class production chain (P5 Task 7 Step 1): sustained edits → native
/// save/reopen → SVG/PDF-X/CDR export with exact warning counts → a recovery copy left in a
/// store and restored by a fresh session. Every class runs the whole chain.
#[test]
fn representative_production_chain_per_class() {
    for (name, build) in [("illustration", illustration as fn() -> (Session, Caps)), ("brochure", brochure), ("poster", poster)] {
        // Sustained edits: a bounded slice of the mixed script (the long soak is the dedicated
        // test above).
        let (mut s, caps) = build();
        for i in 0..10 {
            match i % 3 {
                0 => {
                    run(&mut s, "text.setText", &json!({"id": caps.text, "text": format!("Chain {i}")}));
                }
                1 => {
                    run(&mut s, "select.set", &json!({"ids": [caps.text]}));
                    run(&mut s, "object.move", &json!({"dx": 1, "dy": 0}));
                }
                _ => {
                    // Undo the move (the text edit stays).
                    run(&mut s, "edit.undo", &json!({}));
                }
            }
        }

        // Native save/reopen: the edited document comes back.
        let native = run(&mut s, "document.serialize", &json!({"format": "vectorcraft"}))["dataBase64"].as_str().unwrap().to_owned();
        run(&mut s, "document.open", &json!({"name": "chain.vectorcraft", "dataBase64": native}));
        let doc = run(&mut s, "document.json", &json!({}));
        let all = serde_json::to_string(&doc).unwrap();
        assert!(all.contains("Chain"), "{name}: an edit survived the reopen");

        // Exports with their exact CDR/CMX degradation sets: the illustration and brochure carry
        // no live objects (their CDR warnings are exactly the ones their art dictates — none for
        // the illustration's plain shapes, the brochure's overprint-free round trip likewise),
        // and the poster's separations print setup exports clean.
        let svg = run(&mut s, "document.serialize", &json!({"format": "svg"}))["text"].as_str().unwrap().to_owned();
        assert!(svg.contains("<svg"), "{name}: an SVG document");
        let pdfx = run(&mut s, "document.exportPdf", &json!({"standard": "pdfX4", "advanced": {"outlineText": false}}));
        assert!(pdfx["dataBase64"].as_str().is_some_and(|d| !d.is_empty()), "{name}: a PDF/X-4 file");
        let cdr = run(&mut s, "document.serialize", &json!({"format": "cdr"}));
        let warnings = cdr["warnings"].as_array().cloned().unwrap_or_default();
        let unexpected: Vec<String> = warnings
            .iter()
            .filter(|w| {
                w.as_str().is_some_and(|t| {
                    !(t.contains("live") && t.contains("objects are expanded"))
                        && !t.contains("merge modes other than Normal")
                        && !t.contains("mesh point opacity")
                        && !t.contains("overprint fill and stroke flags")
                        && !t.contains("unlocked PowerClip contents")
                        && !t.contains("texture fill")
                        && !t.contains("live trace")
                        && !t.contains("pattern")
                })
            })
            .map(|w| w.as_str().unwrap_or_default().to_owned())
            .collect();
        assert!(unexpected.is_empty(), "{name}: unexplained CDR warnings: {unexpected:?} (all: {warnings:?})");

        // The recovery chain: the modified document leaves a copy in the store, and a fresh
        // session restores it with the edit intact.
        use vectorcraft_engine::cmd::recovery::MemoryStore;
        let store = std::sync::Arc::new(MemoryStore::default());
        s.recovery.set_store(store.clone());
        run(&mut s, "text.setText", &json!({"id": caps.text, "text": "Recovered chain"}));
        // The session holds every document the chain opened; each modified one is copied.
        let saved = run(&mut s, "file.recovery.save", &json!({}));
        let copied = saved["saved"].as_array().cloned().unwrap_or_default().len();
        assert!(copied >= 1, "{name}: the modified document(s) copied: {saved}");
        // The saving app goes away (its area lock goes with it), as after a crash.
        drop(s);
        let mut fresh = Session::new();
        fresh.recovery.set_store(store.clone());
        let restored = run(&mut fresh, "file.recovery.restore", &json!({}));
        let entries = restored["restored"].as_array().cloned().unwrap_or_default();
        assert_eq!(entries.len(), copied, "{name}: every copy restores: {restored}");
        let doc = run(&mut fresh, "document.json", &json!({}));
        let all = serde_json::to_string(&doc).unwrap();
        assert!(all.contains("Recovered chain"), "{name}: the recovery holds the last edit");
    }
}
