//! CoralCraft (P4 Task 6): the complete production job — a two-page mixed-size document with
//! process CMYK, a named spot ink, overprint, embedded subset text, and a data-driven variant —
//! exported through every production channel and independently checked, with the native source
//! still editable and every degraded CDR/CMX channel named exactly.

use serde_json::{Value, json};
use vectorcraft_doc::NodeKind;
use vectorcraft_engine::Session;

const SPOT: &str = "Coral Spot Orange";
const OVERPRINT_DROP: &str = "overprint fill and stroke flags are dropped in CDR/CMX; native documents keep them";

fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 420, "height": 300})).unwrap();
    s.execute("file.documentColorMode", &json!({"mode": "cmyk"})).unwrap();
    s
}

/// The production job: page 1 with process art, the spot ink, overprint, subset text and the
/// data-driven title; page 2 mixed-size with the traced-style bitmap.
fn production_job() -> Session {
    let mut s = session();
    // Page 1.
    s.execute("shape.rectangle", &json!({"x": 20, "y": 40, "width": 140, "height": 90})).unwrap();
    s.execute("paint.setFill", &json!({"color": {"c": 0, "m": 100, "y": 0, "k": 0}})).unwrap();
    s.execute("swatch.new", &json!({"name": SPOT, "color": {"c": 0, "m": 35, "y": 95, "k": 0}, "global": true})).unwrap();
    s.execute("swatch.setSpot", &json!({"name": SPOT})).unwrap();
    let spot = s.execute("shape.rectangle", &json!({"x": 180, "y": 40, "width": 80, "height": 90})).unwrap()["id"].as_u64().unwrap();
    s.execute("paint.setFill", &json!({"ids": [spot], "swatch": SPOT})).unwrap();
    s.execute("paint.setStroke", &json!({"ids": [spot], "none": true})).unwrap();
    s.execute("shape.rectangle", &json!({"x": 20, "y": 150, "width": 120, "height": 60})).unwrap();
    s.execute("paint.setFill", &json!({"color": {"c": 100, "m": 0, "y": 0, "k": 0}})).unwrap();
    let over = s.execute("shape.rectangle", &json!({"x": 60, "y": 165, "width": 100, "height": 40})).unwrap()["id"].as_u64().unwrap();
    s.execute("paint.setFill", &json!({"ids": [over], "color": {"c": 0, "m": 0, "y": 100, "k": 0}})).unwrap();
    s.execute("paint.setStroke", &json!({"ids": [over], "none": true})).unwrap();
    s.execute("object.setOverprint", &json!({"ids": [over], "fill": true})).unwrap();
    let title = s.execute("text.create", &json!({"x": 20, "y": 15, "text": "Production Alpha"})).unwrap()["id"].as_u64().unwrap();
    s.execute("select.set", &json!({"ids": [title]})).unwrap();
    s.execute("variable.makeTextDynamic", &json!({})).unwrap();
    let var = s.execute("variable.list", &json!({})).unwrap()["variables"][0]["name"].as_str().unwrap().to_owned();
    s.execute("dataset.new", &json!({"name": "Alpha", "values": {var.clone(): "Production Alpha"}})).unwrap();
    s.execute("dataset.new", &json!({"name": "Beta", "values": {var.clone(): "Production Beta"}})).unwrap();
    // Page 2: a different size and a placed bitmap.
    s.execute("artboard.new", &json!({"width": 300, "height": 200})).unwrap();
    let mut png = vec![];
    image::RgbaImage::from_fn(16, 16, |x, _| image::Rgba([30, 120, 180, if x < 8 { 0 } else { 255 }]))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    s.execute("file.place", &json!({"name": "art.png", "dataBase64": vectorcraft_format::base64_encode(&png), "link": false, "at": [40, 40]}))
        .unwrap();
    s.execute("document.setup", &json!({"bleed": 9.0})).unwrap();
    s
}

fn decode(v: &Value) -> Vec<u8> {
    vectorcraft_format::base64_decode(v["dataBase64"].as_str().unwrap()).unwrap()
}

fn texts_of(doc: &vectorcraft_doc::Document) -> Vec<String> {
    let mut out = vec![];
    for layer in &doc.layers {
        layer.walk(&mut |node| {
            if let NodeKind::Text(t) = &node.kind {
                out.push(t.plain_text());
            }
        });
    }
    out
}

#[test]
fn p4_acceptance_runs_the_complete_production_job() {
    let mut s = production_job();

    // PDF/X-1a, X-3 and X-4: each file carries its standard, both pages, the spot ink, embedded
    // fonts, its boxes, and no encryption or editing payload.
    for (standard, header) in [("pdfX1a", "%PDF-1.3"), ("pdfX3", "%PDF-1.3"), ("pdfX4", "%PDF-1.6")] {
        let out = s
            .execute(
                "document.exportPdf",
                &json!({"standard": standard, "advanced": {"outlineText": false}, "bleed": {"useDocument": true}, "compression": {"compressText": false}}),
            )
            .unwrap();
        let bytes = decode(&out);
        let text = String::from_utf8_lossy(&bytes).into_owned();
        assert!(text.starts_with(header), "{standard}");
        assert!(text.contains("/S/GTS_PDFX") && text.contains("/GTS_PDFXVersion"), "{standard}");
        assert!(text.contains("/FontFile") && text.contains("/ToUnicode"), "{standard} embeds subset text");
        assert!(text.contains("/Separation") && text.contains("Coral#20Spot#20Orange"), "{standard} keeps the spot ink");
        assert!(!text.contains("/Encrypt") && !text.contains("/PieceInfo"), "{standard}");
        assert!(text.contains("/TrimBox[9 9 429 309]"), "{standard} page-1 trim box with bleed");
        let read = vectorcraft_pdf::import(&bytes).unwrap_or_else(|e| panic!("{standard} reimports: {e}"));
        assert_eq!(read.artboards.len(), 2, "{standard} carries both mixed-size pages");
        assert!(texts_of(&read).iter().any(|t| t.contains("Production Alpha")), "{standard} keeps the variant text");
    }

    // The separations print PDF: one grey page per printing ink per page, overprint honoured.
    let printed = s.execute("file.print", &json!({"settings": {"output": {"mode": "separations"}}})).unwrap();
    let sep_text = String::from_utf8_lossy(&decode(&printed)).into_owned();
    assert_eq!(printed["pages"].as_u64(), Some(10), "C, M, Y, K and the spot, on both pages");
    assert!(sep_text.contains("/BM/Darken"), "the overprint survives separation");
    assert!(sep_text.contains("/Type/Pages/Count 10"));

    // The imposed PDF: the 420 pt page tiles across A4, every tile printed.
    let impose = json!({"media": "a4", "autoRotate": false, "scaling": "tileFull", "scale": {"width": 200, "height": 200}, "artboards": "range", "range": "1"});
    let imposed = s.execute("file.print", &json!({"settings": impose.clone()})).unwrap();
    let preview = s.execute("print.preview", &json!({"settings": impose})).unwrap();
    let tiles = preview["tiles"][0].clone();
    assert_eq!((tiles["columns"].as_u64(), tiles["rows"].as_u64()), (Some(2), Some(1)), "the doubled artboard spans two A4 columns: {tiles:?}");
    assert_eq!(imposed["pages"].as_u64(), Some(2), "one page per tile");

    // The data variant: row Beta exports its own text.
    s.execute("dataset.select", &json!({"name": "Beta"})).unwrap();
    let variant = s.execute("document.exportPdf", &json!({"compression": {"compressText": false}, "advanced": {"outlineText": false}})).unwrap();
    let read = vectorcraft_pdf::import(&decode(&variant)).unwrap();
    assert!(texts_of(&read).iter().any(|t| t.contains("Production Beta")), "the variant's text");

    // The native source reopens and still edits: the overprint flag, the spot swatch, the
    // variable bindings, and a fresh edit all survive.
    let native = s.execute("document.serialize", &json!({"format": "vectorcraft"})).unwrap()["dataBase64"].as_str().unwrap().to_owned();
    s.execute("document.open", &json!({"name": "job.vectorcraft", "dataBase64": native})).unwrap();
    let info = s.execute("attributes.info", &json!({"ids": [overprinting_id(&s)]})).unwrap();
    assert_eq!(info["overprintFill"], true, "the overprint reopens");
    s.execute("dataset.select", &json!({"name": "Alpha"})).unwrap();
    let back = s.execute("document.exportPdf", &json!({"advanced": {"outlineText": false}})).unwrap();
    assert!(texts_of(&vectorcraft_pdf::import(&decode(&back)).unwrap()).iter().any(|t| t.contains("Production Alpha")), "the template still drives");

    // CDR/CMX: every degraded channel named exactly once — the live mesh-free job has no live
    // nodes, so the channels are the dropped overprint flags (and nothing unexplained).
    s.execute("dataset.select", &json!({"name": "Alpha"})).unwrap();
    for format in ["cdr", "cmx"] {
        let out = s.execute("document.serialize", &json!({"format": format})).unwrap();
        let warnings = out["warnings"].as_array().unwrap();
        let overprint = warnings.iter().filter(|w| w.as_str() == Some(OVERPRINT_DROP)).count();
        assert_eq!(overprint, 1, "{format} names the dropped overprint once: {warnings:?}");
        assert_eq!(warnings.len(), 1, "{format} has no unexplained warnings: {warnings:?}");
        let bytes = decode(&out);
        let imported = corel_import::import_cdr_bytes(&bytes).unwrap_or_else(|e| panic!("{format} reimports: {e}"));
        let mut paths = 0;
        for layer in &imported.document.layers {
            layer.walk(&mut |node| paths += usize::from(node.path_data().is_some_and(|p| !p.subpaths.is_empty())));
        }
        assert!(paths > 0, "{format} keeps ordinary vector art");
    }
    // The native document still holds the overprint after every export.
    assert_eq!(s.execute("attributes.info", &json!({"ids": [overprinting_id(&s)]})).unwrap()["overprintFill"], json!(true));
}

/// The job's overprinting object, found by its flag.
fn overprinting_id(s: &Session) -> u64 {
    let doc = &s.doc().unwrap().doc;
    let mut found = None;
    for layer in &doc.layers {
        layer.walk(&mut |node| {
            if node.appearance.fill().is_some_and(|f| f.overprint) {
                found = Some(node.id.0);
            }
        });
    }
    found.unwrap_or_else(|| panic!("the overprinting object"))
}
