//! CoralCraft (P3 Task E4): the colour lenses over the shared paint + merge-mode architecture.
//! The matrix in `docs/p3-inventory.md` §27 freezes this slice's semantics.

use serde_json::json;
use vectorcraft_color::{BlendMode, Paint};
use vectorcraft_doc::NodeId;
use vectorcraft_engine::Session;

fn session_with_lens_underlay() -> (Session, u64, u64) {
    let mut s = Session::new();
    s.execute("file.new", &json!({ "width": 400, "height": 300 })).unwrap();
    let under = s.execute("shape.rectangle", &json!({ "x": 20, "y": 20, "width": 160, "height": 100 })).unwrap()["id"].as_u64().unwrap();
    s.execute("paint.setFill", &json!({ "id": under, "color": "#804020" })).unwrap();
    let lens = s.execute("shape.ellipse", &json!({ "x": 60, "y": 40, "width": 80, "height": 60 })).unwrap()["id"].as_u64().unwrap();
    (s, under, lens)
}

fn lens_of(s: &Session, id: u64) -> (String, BlendMode, f32) {
    let node = s.doc().unwrap().doc.node(NodeId(id)).unwrap();
    let fill = node.appearance.fill().unwrap();
    let colour = fill.paint.color().map(|c| c.to_hex()).unwrap_or_default();
    (colour, fill.blend, fill.opacity)
}

/// P1 + P3: every colour lens maps onto its paint/blend/opacity; edits are one undo step.
#[test]
fn every_colour_lens_maps_onto_paint_blend_and_opacity() {
    for (kind, want_colour, want_blend) in [
        ("invert", "#ffffff", BlendMode::Difference),
        ("brighten", "#ffffff", BlendMode::Lighten),
        ("darken", "#000000", BlendMode::Darken),
        ("lighten", "#ffffff", BlendMode::Screen),
        ("tintedGrayscale", "#204080", BlendMode::Color),
    ] {
        let (mut s, _, lens) = session_with_lens_underlay();
        let params = if kind == "tintedGrayscale" {
            json!({ "id": lens, "kind": kind, "amount": 60, "color": "#204080" })
        } else {
            json!({ "id": lens, "kind": kind, "amount": 60 })
        };
        s.execute("object.lens", &params).unwrap();
        let (colour, blend, opacity) = lens_of(&s, lens);
        assert_eq!(colour, want_colour, "{kind}: {colour}");
        assert_eq!(blend, want_blend, "{kind}");
        assert_eq!(opacity, 0.6, "{kind}: {opacity}");
        // P3: re-applying edits in place, one undo step.
        let undo_count = s.active().unwrap().history.undo.len();
        let edit = if kind == "tintedGrayscale" {
            json!({ "id": lens, "kind": kind, "amount": 30, "color": "#204080" })
        } else {
            json!({ "id": lens, "kind": kind, "amount": 30 })
        };
        s.execute("object.lens", &edit).unwrap();
        assert_eq!(s.active().unwrap().history.undo.len(), undo_count + 1, "{kind}: one step");
        assert_eq!(lens_of(&s, lens).2, 0.3, "{kind}: the amount follows");
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(lens_of(&s, lens).2, 0.6, "{kind}: undo restores the amount");
        s.execute("edit.redo", &json!({})).unwrap();
        // P4: native reopen keeps the lens as the ordinary paint object it is.
        let data = s.execute("document.serialize", &json!({ "format": "vectorcraft" })).unwrap()["dataBase64"].as_str().unwrap().to_string();
        s.execute("document.open", &json!({ "dataBase64": data })).unwrap();
        let (colour, blend, _) = lens_of(&s, lens);
        assert_eq!((colour.as_str(), blend), (want_colour, want_blend), "{kind}: reopen keeps the lens");
        let _ = Paint::None;
    }
}

/// P2 + P6: the independent reads and the named CDR/CMX degradation.
#[test]
fn a_lens_reads_back_and_names_its_cdr_degradation() {
    let (mut s, _, lens) = session_with_lens_underlay();
    s.execute("object.lens", &json!({ "id": lens, "kind": "invert" })).unwrap();
    // usvg reads the lens's own blend and opacity.
    let svg = s.execute("document.serialize", &json!({ "format": "svg" })).unwrap()["text"].as_str().unwrap().to_string();
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).expect("independent usvg parse");
    let mut blends = vec![];
    collect_blend_names(tree.root(), &mut blends);
    assert!(blends.contains(&"difference"), "the lens shape carries its blend: {blends:?}");
    // The PDF read returns the blend on the imported node.
    let pdf =
        vectorcraft_format::base64_decode(s.execute("document.serialize", &json!({ "format": "pdf" })).unwrap()["dataBase64"].as_str().unwrap())
            .unwrap();
    let imported = vectorcraft_pdf::import(&pdf).expect("independent Hayro-backed PDF read");
    let mut found = false;
    imported.walk(|n| {
        if n.blend == BlendMode::Difference || n.appearance.fill().is_some_and(|fill| fill.blend == BlendMode::Difference) {
            found = true;
        }
    });
    assert!(found, "the PDF read returns the lens's blend");
    // CDR/CMX: the shape and colour survive; the dropped merge mode warns once by name.
    for format in ["cdr", "cmx"] {
        let out = s.execute("document.serialize", &json!({ "format": format })).unwrap();
        let warnings = out["warnings"].as_array().cloned().unwrap_or_default();
        let count = warnings
            .iter()
            .filter(|w| w.as_str().is_some_and(|w| w == "merge modes other than Normal are dropped in CDR/CMX; native blend modes remain editable"))
            .count();
        assert_eq!(count, 1, "the named merge-mode degradation ({format}): {warnings:?}");
    }
}

/// N1: refusals change nothing.
#[test]
fn lens_refusals_change_nothing() {
    let (mut s, _, lens) = session_with_lens_underlay();
    let before = s.execute("document.serialize", &json!({ "format": "vectorcraft" })).unwrap()["dataBase64"].as_str().unwrap().to_string();
    for bad in [
        json!({ "id": lens, "kind": "magnify" }),
        json!({ "id": lens, "kind": "fishEye" }),
        json!({ "id": lens, "kind": "heatMap" }),
        json!({ "id": lens, "kind": "posterize" }),
        json!({ "id": lens, "kind": "findEdges" }),
        json!({ "id": lens, "kind": "teleport" }),
        json!({ "id": lens, "kind": "invert", "amount": "lots" }),
    ] {
        assert!(s.execute("object.lens", &bad).is_err(), "accepted {bad:?}");
    }
    let after = s.execute("document.serialize", &json!({ "format": "vectorcraft" })).unwrap()["dataBase64"].as_str().unwrap().to_string();
    assert_eq!(before, after, "the refused calls changed nothing");
}

fn collect_blend_names(group: &usvg::Group, out: &mut Vec<&'static str>) {
    use usvg::BlendMode as U;
    let name = |b: U| match b {
        U::Normal => "normal",
        U::Multiply => "multiply",
        U::Screen => "screen",
        U::Overlay => "overlay",
        U::Darken => "darken",
        U::Lighten => "lighten",
        U::ColorDodge => "color-dodge",
        U::ColorBurn => "color-burn",
        U::HardLight => "hard-light",
        U::SoftLight => "soft-light",
        U::Difference => "difference",
        U::Exclusion => "exclusion",
        U::Hue => "hue",
        U::Saturation => "saturation",
        U::Color => "color",
        U::Luminosity => "luminosity",
    };
    if group.blend_mode() != U::Normal {
        out.push(name(group.blend_mode()));
    }
    for node in group.children() {
        if let usvg::Node::Group(child) = node {
            collect_blend_names(child, out);
        }
    }
}
