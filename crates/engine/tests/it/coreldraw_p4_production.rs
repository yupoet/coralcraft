//! CoralCraft (P4 Task 2): a production document's colour pipeline proven end to end — process
//! CMYK, a named spot ink, overprint, proof/separation rendering, the separations print PDF,
//! and the refusals that leave document and history untouched.
//!
//! The process-wide colour *settings* (working spaces, intent, BPC) are not touched here: every
//! colour goes through the defaults or per-call intents, because those settings are process-wide
//! and their dedicated coverage lives in `crates/engine/tests/color_settings.rs` (its own test
//! binary, extended by P4 with the intent × BPC matrix).

use serde_json::{Value, json};
use vectorcraft_doc::Document;
use vectorcraft_engine::Session;
use vectorcraft_geom::Affine;
use vectorcraft_render::proof::ProofSetup;
use vectorcraft_render::{RenderOptions, Renderer};

const SPOT: &str = "Coral Spot Orange";

fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 200, "height": 150})).unwrap();
    s.execute("file.documentColorMode", &json!({"mode": "cmyk"})).unwrap();
    s
}

fn rect(s: &mut Session, x: f64, y: f64, w: f64, h: f64, color: Value) -> u64 {
    let id = s.execute("shape.rectangle", &json!({"x": x, "y": y, "width": w, "height": h})).unwrap()["id"].as_u64().unwrap();
    s.execute("paint.setFill", &json!({"ids": [id], "color": color})).unwrap();
    s.execute("paint.setStroke", &json!({"ids": [id], "none": true})).unwrap();
    id
}

/// The production fixture: a CMYK document with a process-ink object, a named spot object, an
/// overprinting object over a knockout base, and one plain normal object.
fn production_document() -> Session {
    let mut s = session();
    // A process-CMYK shape.
    rect(&mut s, 0.0, 0.0, 60.0, 60.0, json!({"c": 0, "m": 100, "y": 0, "k": 0}));
    // A named spot swatch and an object printed in it.
    s.execute("swatch.new", &json!({"name": SPOT, "color": {"c": 0, "m": 35, "y": 95, "k": 0}, "global": true})).unwrap();
    s.execute("swatch.setSpot", &json!({"name": SPOT})).unwrap();
    let spot = s.execute("shape.rectangle", &json!({"x": 70, "y": 0, "width": 60, "height": 60})).unwrap()["id"].as_u64().unwrap();
    s.execute("paint.setFill", &json!({"ids": [spot], "swatch": SPOT})).unwrap();
    s.execute("paint.setStroke", &json!({"ids": [spot], "none": true})).unwrap();
    // An overprinting yellow over a cyan base (overprint shows the inks underneath).
    rect(&mut s, 0.0, 70.0, 60.0, 60.0, json!({"c": 100, "m": 0, "y": 0, "k": 0}));
    let over = s.execute("shape.rectangle", &json!({"x": 20, "y": 90, "width": 60, "height": 40})).unwrap()["id"].as_u64().unwrap();
    s.execute("paint.setFill", &json!({"ids": [over], "color": {"c": 0, "m": 0, "y": 100, "k": 0}})).unwrap();
    s.execute("paint.setStroke", &json!({"ids": [over], "none": true})).unwrap();
    s.execute("object.setOverprint", &json!({"ids": [over], "fill": true})).unwrap();
    // A plain normal object.
    rect(&mut s, 140.0, 90.0, 50.0, 50.0, json!({"c": 0, "m": 0, "y": 0, "k": 100}));
    s
}

fn render(doc: &Document, opts: RenderOptions) -> vectorcraft_render::Rendered {
    let mut r = Renderer::new();
    r.threads = 0;
    r.render(doc, 200, 150, Affine::IDENTITY, &RenderOptions { background: Some([255, 255, 255, 255]), ..opts })
}

fn reopen_native(s: &mut Session) {
    let data = s.execute("document.serialize", &json!({"format": "vectorcraft"})).unwrap()["dataBase64"].as_str().unwrap().to_owned();
    s.execute("document.open", &json!({"name": "p4.vectorcraft", "dataBase64": data})).unwrap();
}

/// The document's plates as `(name, spot)` pairs.
fn plates(s: &mut Session) -> Vec<(String, bool)> {
    s.execute("color.plates", &json!({})).unwrap()["plates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p["name"].as_str().unwrap().to_owned(), p["spot"].as_bool().unwrap()))
        .collect()
}

/// The end-to-end production fixture: plates, spot identity, overprint preview pixels, and every
/// setting surviving undo/redo and a native reopen.
#[test]
fn production_document_names_its_plates_and_keeps_spot_and_overprint_native() {
    let mut s = production_document();

    // Plate identity: the four process plates plus the named spot, and nothing else.
    let plate_list = plates(&mut s);
    let names: Vec<&str> = plate_list.iter().map(|(n, _)| n.as_str()).collect();
    for process in ["Cyan", "Magenta", "Yellow", "Black"] {
        assert!(names.contains(&process), "{process} plate: {names:?}");
    }
    let (spot_name, spot_flag) =
        plate_list.iter().find(|(n, _)| n == SPOT).map(|(n, s)| (n.as_str(), *s)).unwrap_or_else(|| panic!("spot plate: {names:?}"));
    assert!(spot_flag, "the swatch prints on its own spot plate");
    assert_eq!(plate_list.len(), 5, "exactly the four process plates and the spot: {names:?}");
    let _ = spot_name;

    // Separations Preview: solo the spot plate, then hide one process plate.
    let solo = s.execute("view.separationsPreview", &json!({"on": true, "only": SPOT})).unwrap();
    {
        let visible: Vec<&str> =
            solo["plates"].as_array().unwrap().iter().filter(|p| p["visible"].as_bool().unwrap()).map(|p| p["name"].as_str().unwrap()).collect();
        assert_eq!(visible, [SPOT]);
    }
    let toggled =
        s.execute("view.separationsPreview", &json!({"plates": ["Cyan", "Magenta", "Yellow", "Black", SPOT], "toggle": "Magenta"})).unwrap();
    {
        let visible: Vec<&str> =
            toggled["plates"].as_array().unwrap().iter().filter(|p| p["visible"].as_bool().unwrap()).map(|p| p["name"].as_str().unwrap()).collect();
        assert_eq!(visible, ["Cyan", "Yellow", "Black", SPOT]);
    }
    s.execute("view.separationsPreview", &json!({"on": false})).unwrap();

    // Pixel evidence through the render proof path (what the canvas shows):
    // soloing the Yellow plate renders the yellow overprint shape at its ink coverage and the
    // cyan base white; the spot object carries no process yellow, so it is white there.
    let doc = s.doc().unwrap().doc.clone();
    let solo_yellow = render(
        &doc,
        RenderOptions { proof: Some(ProofSetup { separations: Some(vec!["Yellow".to_owned()]), ..ProofSetup::default() }), ..Default::default() },
    );
    let grey = |x: usize, y: usize| solo_yellow.pixel(x as u32, y as u32)[0];
    assert!(grey(35, 110) < 200, "the overprinting yellow prints its ink: {}", grey(35, 110));
    assert!(grey(15, 80) > 240, "the cyan base has no yellow ink: {}", grey(15, 80));
    assert!(grey(100, 30) > 240, "the spot object has no process yellow: {}", grey(100, 30));
    // Overprint Preview: the yellow over the cyan base shows both inks (darkened), while the
    // same yellow without overprint knocks the cyan out.
    let overprinting = render(&doc, RenderOptions { overprint_preview: true, ..Default::default() });
    let knockout = render(&doc, RenderOptions::default());
    assert_ne!(overprinting.pixel(35, 110), knockout.pixel(35, 110), "overprint preview changes the composite");
    let op = overprinting.pixel(35, 110);
    assert!(op[1] > 40 && op[2] > 40, "both the cyan and the yellow ink show under the overprint: {op:?}");

    // Undo/redo and reopen keep the settings exactly.
    let undo_len = s.active().unwrap().history.undo.len();
    let over = find_overprinting(&s);
    s.execute("object.setOverprint", &json!({"ids": [over], "fill": false})).unwrap();
    s.execute("edit.undo", &json!({})).unwrap();
    let info = s.execute("attributes.info", &json!({ "ids": [over] })).unwrap();
    assert_eq!(info["overprintFill"], true, "undo restores the overprint: {info}");
    s.execute("edit.redo", &json!({})).unwrap();
    let info = s.execute("attributes.info", &json!({ "ids": [over] })).unwrap();
    assert_eq!(info["overprintFill"], false, "redo applies the knockout: {info}");
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(s.active().unwrap().history.undo.len(), undo_len, "no stray history steps");

    reopen_native(&mut s);
    let reopened = plates(&mut s);
    assert_eq!(reopened, plate_list, "the plate set survives the native reopen");
    let swatch = s.doc().unwrap().doc.swatches.iter().find(|w| w.name == SPOT).unwrap_or_else(|| panic!("spot swatch reopened"));
    assert!(swatch.spot && swatch.global, "the swatch stays a global spot: {swatch:?}");
    let info = s.execute("attributes.info", &json!({ "ids": [find_overprinting(&s)] })).unwrap();
    assert_eq!(info["overprintFill"], true, "the overprint flag survives the reopen: {info}");
}

/// The id of the fixture's overprinting object (found by its bounds, ids being an
/// implementation detail of the fixture).
fn find_overprinting(s: &Session) -> u64 {
    let doc = &s.doc().unwrap().doc;
    let mut found = None;
    for layer in &doc.layers {
        layer.walk(&mut |node| {
            let over = node.appearance.fill().is_some_and(|f| f.overprint);
            if over {
                found = Some(node.id.0);
            }
        });
    }
    found.unwrap_or_else(|| panic!("the overprinting object"))
}

/// `print.preview` reports one sheet per printing ink with its separation fields, and the
/// separations print PDF carries the spot as a `/Separation` colour space.
#[test]
fn separations_print_reports_one_sheet_per_ink_and_writes_separation_colour_spaces() {
    let mut s = production_document();
    let settings = json!({"output": {"mode": "separations"}});

    let preview = s.execute("print.preview", &json!({"settings": settings.clone()})).unwrap();
    let inks: Vec<Value> = preview["inks"].as_array().unwrap().clone();
    assert_eq!(inks.len(), 5, "four process inks and the spot: {inks:?}");
    let by_name = |n: &str| inks.iter().find(|i| i["name"].as_str() == Some(n)).unwrap_or_else(|| panic!("{n}: {inks:?}"));
    for process in ["Cyan", "Magenta", "Yellow", "Black"] {
        let ink = by_name(process);
        assert_eq!(ink["spot"], false, "{process} is a process ink: {ink}");
        assert_eq!(ink["print"], true, "{process} prints: {ink}");
        assert!(ink["frequency"].is_number() && ink["angle"].is_number(), "{process} screen fields: {ink}");
    }
    let spot = by_name(SPOT);
    assert_eq!(spot["spot"], true, "the named ink is the spot: {spot}");
    // One sheet per ink that prints.
    let sheets: Vec<&str> = preview["sheets"].as_array().unwrap().iter().map(|sh| sh["ink"].as_str().unwrap()).collect();
    assert_eq!(sheets, ["Cyan", "Magenta", "Yellow", "Black", SPOT], "one sheet per printing ink: {sheets:?}");

    // The separations print PDF: one grey page per printing ink, overprints honoured in the
    // separated output (the Darken blend the preview proves).
    let printed = s.execute("file.print", &json!({"settings": settings})).unwrap();
    let pdf = vectorcraft_format::base64_decode(printed["dataBase64"].as_str().unwrap()).unwrap();
    assert_eq!(printed["pages"].as_u64(), Some(5), "five separated pages: {printed:?}");
    let hay = String::from_utf8_lossy(&pdf).into_owned();
    assert!(hay.contains("/Type/Pages/Count 5"), "a page per ink: {:?}", &hay[..hay.len().min(120)]);
    assert!(hay.contains("/BM/Darken"), "separations keep the overprint behaviour");
    // The same document as a composite PDF/X-1a file keeps the spot ink as a /Separation colour
    // space with its colourant name and a CMYK alternate, and names its standard through the
    // /GTS_PDFX output intent.
    let doc = s.doc().unwrap().doc.clone();
    let pdfx = vectorcraft_pdf::export(
        &doc,
        &vectorcraft_pdf::PdfOptions {
            created: Some(0),
            settings: vectorcraft_pdf::PdfSettings {
                standard: vectorcraft_pdf::Standard::PdfX1a,
                compatibility: vectorcraft_pdf::Compatibility::Pdf13,
                ..Default::default()
            },
            ..vectorcraft_pdf::PdfOptions::uncompressed()
        },
    )
    .unwrap();
    let hay = String::from_utf8_lossy(&pdfx).into_owned();
    assert!(hay.contains("/Separation"), "the spot keeps its own colour space");
    assert!(hay.contains("Coral#20Spot#20Orange"), "the colourant name: {:?}", &hay[..hay.len().min(200)]);
    assert!(hay.contains("/GTS_PDFX"), "the PDF/X output intent");

    // A composite print with preserved overprints writes the /OP keys.
    let composite = s.execute("file.print", &json!({"settings": {"output": {"mode": "composite"}, "advanced": {"overprints": "preserve"}}})).unwrap();
    let op_pdf = String::from_utf8_lossy(&vectorcraft_format::base64_decode(composite["dataBase64"].as_str().unwrap()).unwrap()).into_owned();
    assert!(op_pdf.contains("/OP"), "overprint keys: {:?}", &op_pdf[..op_pdf.len().min(120)]);
}

/// Invalid colour and print inputs are refused with the document and its history untouched.
#[test]
fn invalid_colour_and_print_inputs_are_refused_atomically() {
    let mut s = production_document();
    let before = s.execute("document.serialize", &json!({"format": "vectorcraft"})).unwrap()["dataBase64"].as_str().unwrap().to_owned();
    let undo_before = s.active().unwrap().history.undo.len();

    // Unknown intent (per-call, and in the proof setup).
    assert!(s.execute("color.convert", &json!({"color": "#00ff00", "to": "cmyk", "intent": "vivid"})).is_err());
    assert!(s.execute("view.proofSetup", &json!({"target": "cmyk:NoSuchProfile.icc", "proof": true})).is_err());
    // An unknown profile in the working spaces is refused without changing them.
    assert!(s.execute("edit.colorSettings", &json!({"cmyk": "Missing.icc"})).is_err());
    // Non-finite/unparseable colour values (JSON cannot carry NaN; the wrong type stands in).
    assert!(s.execute("paint.setFill", &json!({"ids": [find_overprinting(&s)], "color": {"c": "lots", "m": 0, "y": 0, "k": 0}})).is_err());
    assert!(s.execute("paint.setFill", &json!({"ids": [find_overprinting(&s)], "color": {"c": null, "m": 0, "y": 0, "k": 0}})).is_err());
    // An ink entry the settings cannot parse is refused; an ink name unknown to this document is
    // a documented degradation (a reused preset may name inks it lacks): the preview warns, and
    // the query itself changes nothing.
    assert!(s.execute("print.setup", &json!({"settings": {"output": {"inks": [{"name": 42}]}}})).is_err());
    let preview = s.execute("print.preview", &json!({"settings": {"output": {"mode": "separations", "inks": [{"name": "Not An Ink"}]}}})).unwrap();
    let warnings = preview["warnings"].as_array().unwrap();
    assert!(
        warnings.iter().any(|w| w.as_str().is_some_and(|t| t.contains("no ink named") && t.contains("Not An Ink"))),
        "the unknown ink is named: {warnings:?}"
    );
    // Nothing above changed the document or its history.
    let after = s.execute("document.serialize", &json!({"format": "vectorcraft"})).unwrap()["dataBase64"].as_str().unwrap().to_owned();
    assert_eq!(after, before, "the refused calls mutated nothing");
    assert_eq!(s.active().unwrap().history.undo.len(), undo_before, "the refused calls added no history");
}

/// The PDF/X rule matrix over the live production document: every standard's file reimports
/// through the independent Hayro reader with the blend evaluated and the text real, and no
/// export mutates the native live sources.
#[test]
fn pdfx_matrix_over_the_live_document_reads_back_independently() {
    let mut s = production_document();
    let a = s.execute("shape.ellipse", &json!({"x": 140, "y": 0, "width": 24, "height": 24})).unwrap()["id"].as_u64().unwrap();
    let b = s.execute("shape.ellipse", &json!({"x": 170, "y": 40, "width": 24, "height": 24})).unwrap()["id"].as_u64().unwrap();
    let blend = s.execute("object.blend.make", &json!({"ids": [a, b], "steps": 3})).unwrap()["id"].as_u64().unwrap();
    s.execute("text.create", &json!({"x": 8, "y": 138, "text": "Proof"})).unwrap();
    s.execute("document.setup", &json!({"bleed": 9.0})).unwrap();
    let native = s.execute("document.serialize", &json!({"format": "vectorcraft"})).unwrap()["dataBase64"].as_str().unwrap().to_owned();

    for (standard, header) in [("pdfX1a", "%PDF-1.3"), ("pdfX3", "%PDF-1.3"), ("pdfX4", "%PDF-1.6")] {
        let out = s
            .execute(
                "document.exportPdf",
                &json!({"standard": standard, "advanced": {"outlineText": false}, "bleed": {"useDocument": true}, "compression": {"compressText": false}}),
            )
            .unwrap();
        let bytes = vectorcraft_format::base64_decode(out["dataBase64"].as_str().unwrap()).unwrap();
        assert!(String::from_utf8_lossy(&bytes[..20]).starts_with(header), "{standard}");
        let read = vectorcraft_pdf::import(&bytes).unwrap_or_else(|e| panic!("{standard} reimports: {e}"));
        assert_eq!(read.artboards.len(), 1, "{standard}");
        // The blend is evaluated output: its keys and intermediate shapes all reimport as
        // ordinary painted paths (more than the document's own five shapes plus two keys).
        let mut paths = 0;
        for layer in &read.layers {
            layer.walk(&mut |node| paths += usize::from(node.path_data().is_some_and(|p| !p.subpaths.is_empty())));
        }
        assert!(paths >= 9, "{standard} carries the evaluated blend: {paths} paths");
        // The text is real (not outlines) and reads back.
        let mut texts = vec![];
        for layer in &read.layers {
            layer.walk(&mut |node| {
                if let vectorcraft_doc::NodeKind::Text(t) = &node.kind {
                    texts.push(t.plain_text());
                }
            });
        }
        assert!(texts.iter().any(|t| t.contains("Proof")), "{standard} keeps real text: {texts:?}");
        // The export never touched the native live sources.
        assert!(
            matches!(s.doc().unwrap().doc.node(vectorcraft_doc::NodeId(blend)).map(|n| &n.kind), Some(vectorcraft_doc::NodeKind::Blend { .. })),
            "{standard} keeps the blend live"
        );
        assert_eq!(
            s.execute("document.serialize", &json!({"format": "vectorcraft"})).unwrap()["dataBase64"],
            native,
            "{standard} leaves the native bytes unchanged"
        );
    }
}
