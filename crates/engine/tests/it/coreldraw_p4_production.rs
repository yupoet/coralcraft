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

/// A letter sheet's points.
const LETTER: (f64, f64) = (612.0, 792.0);

/// The print-layout matrix over mixed-size artboards with document bleed: orientation,
/// auto-rotation, transverse, scaling, margins, blank skipping, copies and reverse — the
/// preview's exact geometry, cross-checked against the printed PDF's pages.
#[test]
fn print_layout_matrix_places_mixed_artboards_exactly() {
    let mut s = production_document();
    s.execute("artboard.new", &json!({"width": 300, "height": 100})).unwrap();
    s.execute("document.setup", &json!({"bleed": 9.0})).unwrap();
    let preview = |s: &mut Session, settings: Value| s.execute("print.preview", &json!({"settings": settings})).unwrap();

    // Default: auto-rotation turns the paper to each artboard — both print landscape, centred.
    let r = preview(&mut s, json!({}));
    assert_eq!(r["pages"].as_u64(), Some(2), "one page per artboard");
    let [a, b] = [0, 1].map(|i| r["sheets"][i].clone()) else { panic!("two sheets") };
    for (sheet, trim_w, trim_h) in [(&a, 200.0, 150.0), (&b, 300.0, 100.0)] {
        assert_eq!((sheet["width"].as_f64(), sheet["height"].as_f64()), (Some(LETTER.1), Some(LETTER.0)), "landscape: {sheet}");
        assert_eq!(sheet["scale"].as_array().unwrap(), &vec![json!(100.0), json!(100.0)]);
        let trim = sheet["trim"].as_array().unwrap();
        let (x0, y0, x1, y1) = (trim[0].as_f64().unwrap(), trim[1].as_f64().unwrap(), trim[2].as_f64().unwrap(), trim[3].as_f64().unwrap());
        assert_eq!(x1 - x0, trim_w, "trim width");
        assert_eq!(y1 - y0, trim_h, "trim height");
        assert_eq!(((x0 + x1) / 2.0 - LETTER.1 / 2.0).abs() < 0.01 && ((y0 + y1) / 2.0 - LETTER.0 / 2.0).abs() < 0.01, true, "centred: {trim:?}");
    }

    // Forced portrait keeps the paper upright; transverse turns the page a quarter turn.
    let r = preview(&mut s, json!({"autoRotate": false}));
    assert_eq!((r["sheets"][0]["width"].as_f64(), r["sheets"][0]["height"].as_f64()), (Some(LETTER.0), Some(LETTER.1)), "portrait");
    let r = preview(&mut s, json!({"autoRotate": false, "transverse": true}));
    assert_eq!((r["sheets"][0]["width"].as_f64(), r["sheets"][0]["height"].as_f64()), (Some(LETTER.1), Some(LETTER.0)), "transverse");

    // Fit scaling shrinks art larger than the paper onto the sheet.
    let mut big = Session::new();
    big.execute("file.new", &json!({"width": 700, "height": 700})).unwrap();
    let r = big.execute("print.preview", &json!({"settings": {"autoRotate": false, "scaling": "fit"}})).unwrap();
    let scale = r["sheets"][0]["scale"].as_array().unwrap();
    assert!(scale[0].as_f64().unwrap() < 100.0 && scale[1].as_f64().unwrap() < 100.0, "the 700 pt artboard fits the 612 pt paper: {scale:?}");
    // A margin shrinks the imageable area the art centres in.
    let r = preview(&mut s, json!({"autoRotate": false, "margin": 36}));
    let trim = r["sheets"][0]["trim"].as_array().unwrap();
    assert!(trim[0].as_f64().unwrap() >= 36.0 && trim[1].as_f64().unwrap() >= 36.0, "inside the unprintable margin: {trim:?}");
    // Blank artboards drop out when asked.
    let r = preview(&mut s, json!({"skipBlank": true}));
    let printed: Vec<u64> = r["sheets"].as_array().unwrap().iter().map(|sh| sh["artboard"].as_u64().unwrap()).collect();
    assert_eq!(printed.len(), 1, "the art-less artboard is skipped: {printed:?}");
    // Copies multiply the pages (the sheet list stays one copy in order).
    assert_eq!(preview(&mut s, json!({"copies": 2}))["pages"].as_u64(), Some(4));
    let r = preview(&mut s, json!({"reverse": true}));
    assert_eq!(r["pages"].as_u64(), Some(2), "reverse prints the same pages back to front: {}", r["pages"]);

    // The printed PDF matches the preview: two landscape letter pages.
    let printed = s.execute("file.print", &json!({"settings": {}})).unwrap();
    let pdf = vectorcraft_format::base64_decode(printed["dataBase64"].as_str().unwrap()).unwrap();
    let info = vectorcraft_pdf::info(&pdf, None).unwrap();
    assert_eq!(info.pages.len(), 2);
    assert_eq!((info.pages[0].width, info.pages[0].height), (LETTER.1, LETTER.0), "the sheet size reaches the file");
}

/// Printer's marks reach the printed file: the paper keeps its size with the marks drawn on it
/// in the room around the art, the TrimBox stays the artboard, and the mark geometry imports as
/// real vector paths.
#[test]
fn print_marks_draw_on_the_paper_and_import_as_paths() {
    let mut s = production_document();
    let plain = s.execute("file.print", &json!({"settings": {"autoRotate": false}})).unwrap();
    let marked = s
        .execute(
            "file.print",
            &json!({"settings": {"autoRotate": false, "marks": {"trim": true, "registration": true, "colorBars": true, "pageInfo": true}}}),
        )
        .unwrap();
    let plain_pdf = vectorcraft_format::base64_decode(plain["dataBase64"].as_str().unwrap()).unwrap();
    let marked_pdf = vectorcraft_format::base64_decode(marked["dataBase64"].as_str().unwrap()).unwrap();
    let (plain_info, marked_info) = (vectorcraft_pdf::info(&plain_pdf, None).unwrap(), vectorcraft_pdf::info(&marked_pdf, None).unwrap());
    for info in [&plain_info, &marked_info] {
        assert_eq!((info.pages[0].width, info.pages[0].height), (LETTER.0, LETTER.1), "the paper is the letter sheet");
    }
    let count = |pdf: &[u8]| {
        let read = vectorcraft_pdf::import(pdf).unwrap();
        let mut paths = 0;
        for layer in &read.layers {
            layer.walk(&mut |node| paths += usize::from(node.path_data().is_some_and(|p| !p.subpaths.is_empty())));
        }
        paths
    };
    assert!(count(&marked_pdf) > count(&plain_pdf), "the marks import as painted paths");
    let text = String::from_utf8_lossy(&marked_pdf).into_owned();
    assert!(text.contains("/TrimBox"), "the artboard stays the TrimBox");
    let at = text.find("/TrimBox").unwrap();
    let box_text = &text[at..at + 40];
    assert!(box_text.contains("[206 326.6 406 476.6]"), "the centred 200×150 artboard on the letter sheet: {box_text}");
}

/// Print tiling imposes the art: tile counts and ranges, overlap, and every printed tile
/// non-empty in the independently read file.
#[test]
fn print_tiling_imposes_tiles_that_all_print() {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 1000, "height": 800})).unwrap();
    // Art across the whole sheet, so every tile prints something.
    s.execute("shape.rectangle", &json!({"x": 0, "y": 0, "width": 1000, "height": 800})).unwrap();
    s.execute("paint.setFill", &json!({"color": {"c": 0, "m": 0, "y": 0, "k": 10}})).unwrap();
    let settings =
        |overlap: f64, range: &str| json!({"media": "a4", "autoRotate": false, "scaling": "tileFull", "overlap": overlap, "tileRange": range});

    let r = s.execute("print.preview", &json!({"settings": settings(0.0, "")})).unwrap();
    let tiles = &r["tiles"][0];
    assert_eq!((tiles["columns"].as_u64(), tiles["rows"].as_u64()), (Some(2), Some(1)), "1000×800 pt on A4 portrait: {:?}", tiles);
    assert_eq!(r["sheets"].as_array().unwrap().len(), 2, "one sheet per tile");
    let tile_rects: Vec<Vec<f64>> =
        tiles["tiles"].as_array().unwrap().iter().map(|t| t.as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect()).collect();
    assert_eq!(tile_rects.len(), 2);
    assert!(tile_rects[0][2] <= 1000.0 && tile_rects[1][0] < 1000.0 && tile_rects[1][2] >= 995.0, "the tiles cover the art: {tile_rects:?}");

    // Overlap makes neighbouring tiles share `overlap` pt of art.
    let r = s.execute("print.preview", &json!({"settings": settings(24.0, "")})).unwrap();
    let tiles = &r["tiles"][0];
    let rects: Vec<Vec<f64>> =
        tiles["tiles"].as_array().unwrap().iter().map(|t| t.as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect()).collect();
    assert!((rects[1][0] - (rects[0][2] - 24.0)).abs() < 0.01, "24 pt of shared art: {rects:?}");

    // A tile range prints just those tiles, and every printed tile is non-empty.
    let r = s.execute("print.preview", &json!({"settings": settings(0.0, "1")})).unwrap();
    assert_eq!(r["sheets"].as_array().unwrap().len(), 1, "tileRange 1");
    let printed = s.execute("file.print", &json!({"settings": settings(0.0, "")})).unwrap();
    assert_eq!(printed["pages"].as_u64(), Some(2));
    let pdf = vectorcraft_format::base64_decode(printed["dataBase64"].as_str().unwrap()).unwrap();
    assert_eq!(vectorcraft_pdf::info(&pdf, None).unwrap().pages.len(), 2);
    let read = vectorcraft_pdf::import(&pdf).unwrap();
    let mut paths = 0;
    for layer in &read.layers {
        layer.walk(&mut |node| paths += usize::from(node.path_data().is_some_and(|p| !p.subpaths.is_empty())));
    }
    assert!(paths >= 2, "the imposed art reads back: {paths} paths");
}

/// A fresh temp folder for one test (removed when dropped).
struct TempDir(std::path::PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("p4-production-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn file(&self, name: &str) -> String {
        self.0.join(name).to_string_lossy().into_owned()
    }

    fn png(&self, name: &str, colour: [u8; 3]) -> String {
        let path = self.file(name);
        image::RgbaImage::from_pixel(12, 12, image::Rgba([colour[0], colour[1], colour[2], 255])).save(&path).unwrap();
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The data-driven production loop: one template (text, visibility, linked image, graph
/// variables), three data rows, one exported PDF per row verified through the independent
/// reader — one undo step per row, the template still bound after a native reopen, and a
/// missing row refused without mutation.
#[test]
fn data_merge_exports_one_variant_pdf_per_row_and_keeps_the_template() {
    let dir = TempDir::new("data-merge");
    let red = dir.png("red.png", [200, 30, 30]);
    let blue = dir.png("blue.png", [30, 30, 200]);

    let mut s = session();
    let title = s.execute("text.create", &json!({"x": 10, "y": 10, "text": "Hello Alpha"})).unwrap()["id"].as_u64().unwrap();
    let badge = s.execute("shape.rectangle", &json!({"x": 10, "y": 40, "width": 40, "height": 20})).unwrap()["id"].as_u64().unwrap();
    s.execute("paint.setFill", &json!({"ids": [badge], "color": {"c": 0, "m": 0, "y": 0, "k": 20}})).unwrap();
    let photo = s.execute("file.place", &json!({"path": red, "link": true, "at": [60, 10]})).unwrap()["ids"][0].as_u64().unwrap();
    let graph =
        s.execute("graph.create", &json!({"type": "column", "x": 10, "y": 70, "width": 100, "height": 60, "csv": ",a\nQ1,10\nQ2,5"})).unwrap()["id"]
            .as_u64()
            .unwrap();

    s.execute("select.set", &json!({"ids": [title]})).unwrap();
    s.execute("variable.makeTextDynamic", &json!({})).unwrap();
    s.execute("select.set", &json!({"ids": [badge]})).unwrap();
    s.execute("variable.makeVisibilityDynamic", &json!({})).unwrap();
    s.execute("select.set", &json!({"ids": [photo]})).unwrap();
    s.execute("variable.makeImageDynamic", &json!({})).unwrap();
    s.execute("select.set", &json!({"ids": [graph]})).unwrap();
    s.execute("variable.makeGraphDynamic", &json!({})).unwrap();
    let names = |s: &mut Session| -> Vec<String> {
        s.execute("variable.list", &json!({})).unwrap()["variables"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["name"].as_str().unwrap().to_owned())
            .collect()
    };
    let vars = names(&mut s);
    assert_eq!(vars.len(), 4, "text, visibility, image, graph: {vars:?}");
    let [text_var, vis_var, image_var, graph_var] = [vars[0].clone(), vars[1].clone(), vars[2].clone(), vars[3].clone()] else { unreachable!() };

    // Three rows.
    let rows = [
        ("Alpha", true, red.as_str(), ",a\nQ1,10\nQ2,5"),
        ("Beta", false, blue.as_str(), ",a\nQ1,8\nQ2,4\nQ3,2"),
        ("Gamma", true, red.as_str(), ",a\nQ1,12"),
    ];
    for (row, visible, image_path, csv) in rows {
        s.execute(
            "dataset.new",
            &json!({"name": row, "values": {text_var.clone(): format!("Hello {row}"), vis_var.clone(): visible, image_var.clone(): image_path, graph_var.clone(): csv}}),
        )
        .unwrap();
    }

    // One exported variant PDF per row, each verified through the independent reader.
    for (row, _visible, _, _) in rows {
        let undo_before = s.active().unwrap().history.undo.len();
        let applied = s.execute("dataset.select", &json!({"name": row})).unwrap();
        assert_eq!((applied["applied"].as_u64(), applied["skipped"].as_u64()), (Some(4), Some(0)), "{row}: {applied}");
        assert_eq!(s.active().unwrap().history.undo.len(), undo_before + 1, "{row}: one undo step");
        let out = s.execute("document.exportPdf", &json!({"compression": {"compressText": false}})).unwrap();
        let read = vectorcraft_pdf::import(&vectorcraft_format::base64_decode(out["dataBase64"].as_str().unwrap()).unwrap()).unwrap();
        let mut texts = vec![];
        let mut images = 0;
        let mut paths = 0;
        for layer in &read.layers {
            layer.walk(&mut |node| {
                if let vectorcraft_doc::NodeKind::Text(t) = &node.kind {
                    texts.push(t.plain_text());
                }
                images += usize::from(matches!(node.kind, vectorcraft_doc::NodeKind::Image(_)));
                paths += usize::from(node.path_data().is_some_and(|p| !p.subpaths.is_empty()));
            });
        }
        assert!(texts.iter().any(|t| t.contains(&format!("Hello {row}"))), "{row}: {texts:?}");
        assert_eq!(images, 1, "{row} keeps its photo");
    }
    // The visibility difference is real: the hidden row's PDF has one painted shape fewer.
    let variants: Vec<(String, usize)> = rows
        .iter()
        .map(|(row, _, _, _)| {
            s.execute("dataset.select", &json!({"name": row})).unwrap();
            let out = s.execute("document.exportPdf", &json!({})).unwrap();
            let read = vectorcraft_pdf::import(&vectorcraft_format::base64_decode(out["dataBase64"].as_str().unwrap()).unwrap()).unwrap();
            let mut paths = 0;
            for layer in &read.layers {
                layer.walk(&mut |node| paths += usize::from(node.path_data().is_some_and(|p| !p.subpaths.is_empty())));
            }
            (row.to_string(), paths)
        })
        .collect();
    let count = |row: &str| variants.iter().find(|(r, _)| r == row).unwrap().1;
    assert!(count("Alpha") > count("Beta"), "the hidden badge drops out: {variants:?}");
    assert!(count("Gamma") > count("Beta"), "the graph's third category adds geometry: {variants:?}");

    // The template survives a native reopen with every binding and row.
    let native = s.execute("document.serialize", &json!({"format": "vectorcraft"})).unwrap()["dataBase64"].as_str().unwrap().to_owned();
    s.execute("document.open", &json!({"name": "p4.vectorcraft", "dataBase64": native})).unwrap();
    assert_eq!(names(&mut s).len(), 4, "the variables reopen");
    let sets = s.execute("dataset.list", &json!({})).unwrap()["datasets"].as_array().unwrap().clone();
    assert_eq!(sets.len(), 3, "the rows reopen: {sets:?}");

    // A missing row is refused without touching the document.
    let before = s.execute("document.serialize", &json!({"format": "vectorcraft"})).unwrap()["dataBase64"].as_str().unwrap().to_owned();
    assert!(s.execute("dataset.select", &json!({"name": "No Such Row"})).is_err());
    assert_eq!(s.execute("document.serialize", &json!({"format": "vectorcraft"})).unwrap()["dataBase64"], before);
}

/// Batch export writes every variant in every format with unique names and per-format folders,
/// and the print/PDF presets save, apply, and move between documents.
#[test]
fn batch_export_writes_every_variant_and_presets_move_between_documents() {
    let mut s = session();
    s.execute("artboard.new", &json!({"width": 200, "height": 150})).unwrap();
    let title = s.execute("text.create", &json!({"x": 10, "y": 10, "text": "One"})).unwrap()["id"].as_u64().unwrap();
    s.execute("select.set", &json!({"ids": [title]})).unwrap();
    s.execute("variable.makeTextDynamic", &json!({})).unwrap();
    let name = s.execute("variable.list", &json!({})).unwrap()["variables"][0]["name"].as_str().unwrap().to_owned();
    s.execute("dataset.new", &json!({"name": "One", "values": {name.clone(): "One"}})).unwrap();
    s.execute("dataset.new", &json!({"name": "Two", "values": {name.clone(): "Two"}})).unwrap();

    // Every variant × {SVG, PNG, PDF}: unique names, per-format folders, nothing empty.
    let mut svgs = vec![];
    for row in ["One", "Two"] {
        s.execute("dataset.select", &json!({"name": row})).unwrap();
        let r = s
            .execute(
                "document.exportForScreens",
                &json!({"range": "1-2", "name": "variant-<filename>-####.ext", "folders": "format", "formats": [
                    {"format": "svg"},
                    {"format": "png", "scale": "1x"},
                    {"format": "pdf"}
                ]}),
            )
            .unwrap();
        let files = r["files"].as_array().unwrap();
        assert_eq!(files.len(), 6, "{row}: {files:?}");
        let mut names: Vec<&str> = files.iter().map(|f| f["name"].as_str().unwrap()).collect();
        names.sort();
        assert_eq!(names.len(), names.iter().collect::<std::collections::BTreeSet<_>>().len(), "{row}: unique names");
        for folder in ["svg/", "png/", "pdf/"] {
            assert!(names.iter().filter(|n| n.starts_with(folder)).count() == 2, "{row}: {names:?}");
        }
        for f in files {
            let size = f["bytes"]
                .as_u64()
                .or_else(|| f["dataBase64"].as_str().map(|d| d.len() as u64))
                .or_else(|| f["data"].as_str().map(|d| d.len() as u64))
                .unwrap_or(0);
            assert!(size > 0, "{row}: non-empty output: {f:?}");
        }
        let svg = files.iter().find(|f| f["name"].as_str().is_some_and(|n| n.starts_with("svg/"))).unwrap();
        let data = svg["dataBase64"].as_str().or_else(|| svg["data"].as_str()).unwrap_or_else(|| panic!("{svg:?}"));
        svgs.push(vectorcraft_format::base64_decode(data).unwrap());
    }
    assert_ne!(svgs[0], svgs[1], "the two variants' SVG outputs differ");

    // Print presets save, apply through print.setup, and move between documents.
    s.execute("print.presets.save", &json!({"name": "P4 Press", "settings": {"media": "a4", "marks": {"trim": true}}})).unwrap();
    let listed = s.execute("print.presets.list", &json!({})).unwrap()["presets"].as_array().unwrap().clone();
    assert!(listed.iter().any(|p| p["name"] == "P4 Press"), "{listed:?}");
    let applied = s.execute("print.setup", &json!({"preset": "P4 Press"})).unwrap()["settings"].clone();
    assert_eq!(applied["media"], "a4", "the preset applies: {applied}");
    let exported = s.execute("print.presets.export", &json!({})).unwrap();
    let moved = exported["data"].as_str().unwrap();
    let mut fresh = Session::new();
    fresh.execute("file.new", &json!({"width": 100, "height": 100})).unwrap();
    let imported = fresh.execute("print.presets.import", &json!({"data": moved})).unwrap();
    assert_eq!(imported["imported"].as_array().unwrap(), &vec![json!("P4 Press")], "the saved preset moves: {imported}");
    assert!(fresh.execute("print.setup", &json!({"preset": "P4 Press"})).is_ok(), "the moved preset applies");

    // PDF presets save and apply to an export.
    s.execute("pdf.preset.save", &json!({"name": "P4/X-1a", "standard": "pdfX1a"})).unwrap();
    let out = s.execute("document.exportPdf", &json!({"preset": "P4/X-1a"})).unwrap();
    assert!(
        String::from_utf8_lossy(&vectorcraft_format::base64_decode(out["dataBase64"].as_str().unwrap()).unwrap()).starts_with("%PDF-1.3"),
        "the saved preset's standard applies"
    );
}
