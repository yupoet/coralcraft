use std::io::Cursor;

use serde_json::{Value, json};
use vectorcraft_doc::NodeKind;

use super::*;
use crate::cmd::parse_range;

fn session(width: f64, height: f64, artboards: usize) -> Session {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": width, "height": height, "artboards": artboards})).unwrap();
    s
}

fn tmp_dir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("vc-fileio-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn b64(v: &Value) -> Vec<u8> {
    vectorcraft_format::base64_decode(v["dataBase64"].as_str().expect("dataBase64")).unwrap()
}

fn open(s: &mut Session, name: &str, bytes: &[u8]) -> Value {
    s.execute("document.open", &json!({"name": name, "dataBase64": vectorcraft_format::base64_encode(bytes)}))
        .unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// A `w`×`h` image of one colour in `format`.
fn image_bytes(w: u32, h: u32, format: image::ImageFormat) -> Vec<u8> {
    let rgba = image::RgbaImage::from_pixel(w, h, image::Rgba([200, 30, 40, 255]));
    let img = if format == image::ImageFormat::Jpeg {
        image::DynamicImage::ImageRgb8(image::DynamicImage::ImageRgba8(rgba).to_rgb8())
    } else {
        rgba.into()
    };
    let mut out = Vec::new();
    img.write_to(&mut Cursor::new(&mut out), format).unwrap();
    out
}

fn image_of(s: &Session) -> vectorcraft_doc::ImageObject {
    let doc = &s.doc().unwrap().doc;
    match &doc.layers[0].children().unwrap()[0].kind {
        NodeKind::Image(im) => (**im).clone(),
        other => panic!("expected an image, got {other:?}"),
    }
}

#[test]
fn opens_every_readable_format() {
    let mut s = session(200.0, 100.0, 1);
    s.execute("shape.rectangle", &json!({"x": 10, "y": 10, "width": 50, "height": 40})).unwrap();
    let native = b64(&s.execute("document.serialize", &json!({})).unwrap());
    let svg = s.execute("document.serialize", &json!({"format": "svg"})).unwrap()["text"].as_str().unwrap().to_string();
    let pdf = b64(&s.execute("document.serialize", &json!({"format": "pdf"})).unwrap());
    let svgz = vectorcraft_svg::compress(&svg);

    for (name, bytes, format) in [
        ("a.vectorcraft", native.clone(), "vectorcraft"),
        ("a.drawcraft", native, "vectorcraft"),
        ("a.svg", svg.into_bytes(), "svg"),
        ("a.svgz", svgz, "svgz"),
        ("a.pdf", pdf.clone(), "pdf"),
        ("a.ai", pdf, "ai"),
    ] {
        let r = open(&mut s, name, &bytes);
        assert_eq!(r["format"], format, "{name}");
        assert_eq!(r["title"], if format == "vectorcraft" { "Untitled-1" } else { name }, "{name}");
        assert!(s.doc().unwrap().doc.node_count() >= 2, "{name}: the rectangle came through");
    }
    for (name, kind) in [
        ("p.png", image::ImageFormat::Png),
        ("p.jpg", image::ImageFormat::Jpeg),
        ("p.gif", image::ImageFormat::Gif),
        ("p.webp", image::ImageFormat::WebP),
        ("p.tif", image::ImageFormat::Tiff),
        ("p.bmp", image::ImageFormat::Bmp),
    ] {
        let r = open(&mut s, name, &image_bytes(5, 4, kind));
        assert_eq!(r["format"], format(name.rsplit('.').next().unwrap()).unwrap().id, "{name}");
        let im = image_of(&s);
        assert_eq!((im.width, im.height), (5, 4), "{name}");
        let doc = &s.doc().unwrap().doc;
        assert_eq!((doc.artboards[0].rect.width(), doc.artboards[0].rect.height()), (5.0, 4.0));
        // Browser-safe formats keep their bytes; TIFF and BMP are stored as PNG.
        let mime = &doc.images[&im.key].mime;
        assert_eq!(mime, if name.ends_with("tif") || name.ends_with("bmp") { "image/png" } else { format_for_name(name).unwrap().mime });
    }
    assert!(s.execute("document.open", &json!({"name": "x.txt", "dataBase64": "aGVsbG8="})).is_err());
}

#[test]
fn webp_opens_at_its_pixel_size() {
    let mut s = Session::new();
    open(&mut s, "tiny.webp", &image_bytes(3, 2, image::ImageFormat::WebP));
    let im = image_of(&s);
    assert_eq!((im.width, im.height), (3, 2));
    let ab = s.doc().unwrap().doc.artboards[0].rect;
    assert_eq!((ab.width(), ab.height()), (3.0, 2.0));
    let r = raster_image(&image_bytes(3, 2, image::ImageFormat::WebP)).unwrap();
    assert_eq!((r.width, r.height, r.blob.mime.as_str()), (3, 2, "image/webp"));
}

#[test]
fn cmyk_tiffs_keep_their_inks_and_other_tiffs_become_png() {
    let inks = vectorcraft_doc::cmyk::Inks::new(2, 1, vec![0, 102, 255, 0, 10, 20, 30, 40]).unwrap();
    let tiff = vectorcraft_doc::ImageBlob::cmyk_tiff(&inks).unwrap();
    let r = raster_image(&tiff.bytes).unwrap();
    assert_eq!((r.width, r.height, r.blob.mime.as_str()), (2, 1, "image/tiff"));
    assert_eq!(r.blob.cmyk(), Some(inks));
    let rgb = raster_image(&image_bytes(2, 1, image::ImageFormat::Tiff)).unwrap();
    assert_eq!(rgb.blob.mime, "image/png");
}

/// A 3 × 2 TIFF at 150 ppi of `channels` (`extra`: their ExtraSamples values), 8 or 16 `bits` a
/// sample, chunky or `planar`.
fn hand_tiff(photometric: u16, bits: u16, extra: &[u16], planar: bool, channels: &[Vec<u16>]) -> Vec<u8> {
    let t = vectorcraft_testkit::tiff::Tiff { width: 3, height: 2, photometric, bits, extra, planar, ppi: Some(150), channels };
    vectorcraft_testkit::tiff::tiff(&t)
}

/// What Place keeps of TIFF `bytes`: the ink amounts, when it keeps them, and the pixels any image
/// reader decodes from the stored image, as RGBA.
fn kept(what: &str, bytes: &[u8]) -> (Option<Vec<u8>>, Vec<u8>) {
    let r = raster_image(bytes).unwrap_or_else(|e| panic!("{what}: {e}"));
    assert_eq!((r.width, r.height, r.ppi), (3, 2, Some((150.0, 150.0))), "{what}");
    assert_eq!(super::ppi::resolution(&r.blob.bytes), Some((150.0, 150.0)), "{what}: the stored image keeps the resolution");
    let rgba = image::load_from_memory(&r.blob.bytes).unwrap_or_else(|e| panic!("{what}: {e}")).to_rgba8().into_raw();
    (r.blob.cmyk().map(|i| i.data), rgba)
}

/// TIFFs that store one plane per channel (PlanarConfiguration 2) read as their chunky versions do:
/// grayscale and RGB with extra channels, and CMYK with and without them (#1466).
#[test]
fn planar_tiffs_read_as_their_chunky_versions() {
    let ch = |v: [u16; 6]| v.to_vec();
    let (gray, alpha, spot) = (ch([0, 51, 102, 153, 204, 255]), ch([255, 128, 0, 255, 64, 255]), ch([9, 8, 7, 6, 5, 4]));
    let (r, g, b) = (ch([255, 0, 0, 10, 20, 30]), ch([0, 255, 0, 40, 50, 60]), ch([0, 0, 255, 70, 80, 90]));
    let (c, m, y, k) = (ch([255, 0, 0, 0, 12, 200]), ch([0, 255, 0, 0, 34, 100]), ch([0, 0, 255, 0, 56, 50]), ch([0, 0, 0, 255, 78, 0]));
    // Pixel by pixel, the samples of `px`.
    let interleave = |px: &[&Vec<u16>]| -> Vec<u8> { (0..6).flat_map(|i| px.iter().map(move |v| v[i] as u8)).collect() };
    let opaque = ch([255; 6]);
    let inks = interleave(&[&c, &m, &y, &k]);
    // (photometric, ExtraSamples, channels, the RGBA pixels when the color is gray or RGB, the inks when it is CMYK)
    type Case = (u16, Vec<u16>, Vec<Vec<u16>>, Option<Vec<u8>>, Option<Vec<u8>>);
    let cases: Vec<Case> = vec![
        (1, vec![2], vec![gray.clone(), alpha.clone()], Some(interleave(&[&gray, &gray, &gray, &alpha])), None),
        (1, vec![2, 0], vec![gray.clone(), alpha.clone(), spot.clone()], Some(interleave(&[&gray, &gray, &gray, &alpha])), None),
        (2, vec![0], vec![r.clone(), g.clone(), b.clone(), spot.clone()], Some(interleave(&[&r, &g, &b, &opaque])), None),
        (2, vec![2, 0], vec![r.clone(), g.clone(), b.clone(), alpha.clone(), spot.clone()], Some(interleave(&[&r, &g, &b, &alpha])), None),
        (5, vec![], vec![c.clone(), m.clone(), y.clone(), k.clone()], None, Some(inks.clone())),
        (5, vec![0], vec![c.clone(), m.clone(), y.clone(), k.clone(), spot.clone()], None, Some(inks.clone())),
        (5, vec![2], vec![c.clone(), m.clone(), y.clone(), k.clone(), alpha.clone()], None, None),
    ];
    for (photometric, extra, channels, want_rgba, want_inks) in cases {
        for bits in [8, 16] {
            let wide: Vec<Vec<u16>> = channels.iter().map(|ch| ch.iter().map(|v| if bits == 16 { v * 257 } else { *v }).collect()).collect();
            let what = format!("photometric {photometric}, extra samples {extra:?}, {bits} bits");
            let chunky = kept(&format!("{what}, chunky"), &hand_tiff(photometric, bits, &extra, false, &wide));
            let planar = kept(&format!("{what}, planar"), &hand_tiff(photometric, bits, &extra, true, &wide));
            assert_eq!(planar.0, chunky.0, "{what}: planar keeps the inks chunky keeps");
            assert_eq!(planar.0, want_inks, "{what}: the inks");
            // A 16-bit chunky CMYK file is stored as it is, and a planar one is written again with
            // its 8-bit inks: their RGB pixels may be a level apart.
            let close = planar.1.len() == chunky.1.len() && planar.1.iter().zip(&chunky.1).all(|(a, b)| a.abs_diff(*b) <= 1);
            assert!(close, "{what}: planar reads as chunky: {:?} against {:?}", planar.1, chunky.1);
            if let Some(want) = &want_rgba {
                assert_eq!(&planar.1, want, "{what}: the pixels");
            }
        }
    }
}

/// A planar TIFF whose header claims more strips than the decoder can count (here 2^31 + 1 rows
/// a strip each, times 5 planes) is refused as too large to draw, without a panic.
#[test]
fn planar_tiffs_with_more_strips_than_can_be_counted_are_too_large() {
    let plane = |v: u16| vec![v; 6];
    let mut file = hand_tiff(5, 8, &[0], true, &[plane(200), plane(40), plane(10), plane(0), plane(90)]);
    // ImageLength (the directory's second entry, its value at offset 30) and RowsPerStrip (the
    // eighth, at offset 102).
    file[30..34].copy_from_slice(&(1u32 << 31 | 1).to_le_bytes());
    file[102..106].copy_from_slice(&1u32.to_le_bytes());
    let e = raster_image(&file).err().expect("refused");
    assert!(e.to_string().contains("an image of 3 × 2147483649 pixels is too large to draw"), "{e}");
    assert!(too_large_note(&file).is_some());
    assert_eq!(vectorcraft_doc::cmyk::oversized_tiff(&file), Some((3, 1 << 31 | 1)));
}

/// A 16-bit RGB TIFF whose samples need more than the allowance (6 bytes a pixel), though its RGBA
/// pixels would fit, is refused as too large to draw before it is decoded.
#[test]
fn tiffs_whose_samples_pass_the_allowance_are_too_large() {
    let plane = |v: u16| vec![v; 6];
    let mut file = hand_tiff(2, 16, &[], false, &[plane(9000), plane(30_000), plane(60_000)]);
    // The loop sets ImageWidth, ImageLength and RowsPerStrip (the first, second and eighth entries) to `side`.
    let side = 20_000u32;
    assert!(u64::from(side * side) * 6 > MAX_RASTER_ALLOC && u64::from(side * side) * 4 <= MAX_RASTER_ALLOC);
    for at in [18, 30, 102] {
        file[at..at + 4].copy_from_slice(&side.to_le_bytes());
    }
    let e = raster_image(&file).err().expect("refused");
    assert_eq!(e.to_string(), "an image of 20000 × 20000 pixels is too large to draw");
}

/// A planar CMYK TIFF opens and places with its ink amounts, stored as a TIFF the image decoder
/// reads (#1466).
#[test]
fn planar_cmyk_tiffs_open_and_place_with_their_inks() {
    let inks = [vec![255u16, 0, 12, 0, 40, 9], vec![0, 255, 34, 0, 50, 8], vec![0, 0, 56, 0, 60, 7], vec![0, 0, 78, 255, 70, 6]];
    let file = hand_tiff(5, 8, &[], true, &inks);
    let want: Vec<u8> = (0..6).flat_map(|i| inks.iter().map(move |v| v[i] as u8)).collect();
    assert!(image::load_from_memory(&file).is_err(), "the image decoder reads planar CMYK now");
    let mut s = Session::new();
    open(&mut s, "planar.tif", &file);
    let blob = &s.doc().unwrap().doc.images[&image_of(&s).key];
    assert_eq!(blob.cmyk().map(|i| i.data), Some(want.clone()), "Open keeps the inks");
    let mut s = session(100.0, 100.0, 1);
    s.execute("file.place", &json!({"name": "planar.tif", "dataBase64": vectorcraft_format::base64_encode(&file)})).unwrap();
    let blob = &s.doc().unwrap().doc.images[&image_of(&s).key];
    assert_eq!(blob.cmyk().map(|i| i.data), Some(want), "Place keeps the inks");
    assert!(image::load_from_memory(&blob.bytes).is_ok(), "the image decoder reads the stored image");
}

/// A 70 × 7 picture, `true` for black: a white row and a black row (runs past 63 take a make-up
/// code), then runs of several lengths.
fn fax_picture() -> Vec<Vec<bool>> {
    (0..7).map(|y| (0..70).map(|x| if y < 2 { y == 1 } else { (x * (y + 2) / 5 + y) % 3 == 0 }).collect()).collect()
}

/// The first `rows` rows of [`fax_picture`] as a TIFF compressed with CCITT Group 3 in one strip:
/// `compression` 2 or 3 with `t4_options`, white is zero, the highest bit of each byte first.
fn group3_tiff(compression: u16, t4_options: u32, rows: usize) -> Vec<u8> {
    let rows = &fax_picture()[..rows];
    let g =
        vectorcraft_testkit::tiff::Group3 { rows, compression, t4_options, photometric: 0, fill_order: 1, rows_per_strip: 7, tile: None, rtc: false };
    vectorcraft_testkit::tiff::group3(&g)
}

/// 1-bit TIFFs compressed with CCITT Group 3 open and place with their pixels (#1466): Compression
/// 2 (modified Huffman) and Compression 3 (ITU-T T.4) with one-dimensional rows or two-dimensional
/// ones (T4Options bit 0), with and without fill bits (bit 2) and six EOLs at the end, in one strip,
/// three strips or three 32 × 16 tiles, each byte's highest or lowest bit first (FillOrder 1 and 2),
/// white or black as zero.
#[test]
fn ccitt_group3_tiffs_open_and_place_with_their_pixels() {
    let rows = fax_picture();
    let mut s = Session::new();
    for (compression, t4_options, rtc) in [(2, 0, false), (3, 0, false), (3, 4, true), (3, 1, false), (3, 5, true)] {
        for (rows_per_strip, tile) in [(7, None), (3, None), (7, Some((32, 16)))] {
            for fill_order in [1, 2] {
                for photometric in [0, 1] {
                    let what = format!(
                        "Compression {compression}, T4Options {t4_options}, {rows_per_strip} rows a strip, tiles {tile:?}, FillOrder {fill_order}, photometric {photometric}"
                    );
                    let g = vectorcraft_testkit::tiff::Group3 {
                        rows: &rows,
                        compression,
                        t4_options,
                        photometric,
                        fill_order,
                        rows_per_strip,
                        tile,
                        rtc,
                    };
                    let file = vectorcraft_testkit::tiff::group3(&g);
                    assert!(image::load_from_memory(&file).is_err(), "{what}: the image decoder reads Group 3 now");
                    // A black run's pixels are 1 bits, which show as black when white is zero.
                    let want: Vec<u8> = rows.concat().iter().map(|&black| if black == (photometric == 0) { 0 } else { 255 }).collect();
                    let pixels = |s: &Session| {
                        let blob = &s.doc().unwrap().doc.images[&image_of(s).key];
                        image::load_from_memory(&blob.bytes).unwrap_or_else(|e| panic!("{what}: {e}")).to_luma8().into_raw()
                    };
                    open(&mut s, "fax.tif", &file);
                    assert_eq!(pixels(&s), want, "{what}: Open");
                    let mut placed = session(100.0, 100.0, 1);
                    placed.execute("file.place", &json!({"name": "fax.tif", "dataBase64": vectorcraft_format::base64_encode(&file)})).unwrap();
                    assert_eq!(pixels(&placed), want, "{what}: Place");
                }
            }
        }
    }
}

/// A Group 3 TIFF whose pixels would take more than the decoding allowance as RGBA is refused as
/// too large to draw, without a panic at 2^31 × 2^31 pixels too. One whose data ends inside a row,
/// or after a whole row before the strip's last, is refused as damaged, and so is one whose tiles
/// are wider than the allowance.
#[test]
fn ccitt_group3_tiffs_too_large_or_cut_short_are_refused() {
    // ImageWidth (the directory's first entry, its value at offset 18), ImageLength (the second, at
    // 30) and RowsPerStrip (the eighth, at 102).
    for (width, height) in [(70u32, 1u32 << 30), (1 << 31, 1 << 31)] {
        let mut big = group3_tiff(3, 1, 7);
        big[18..22].copy_from_slice(&width.to_le_bytes());
        big[30..34].copy_from_slice(&height.to_le_bytes());
        big[102..106].copy_from_slice(&height.to_le_bytes());
        let e = raster_image(&big).err().expect("refused");
        assert!(e.to_string().contains(&format!("an image of {width} × {height} pixels is too large to draw")), "{e}");
    }
    let mut short = group3_tiff(3, 0, 7);
    short.truncate(short.len() - 6);
    let e = raster_image(&short).err().expect("refused");
    assert!(e.to_string().contains("could not decode image: unexpected end of input"), "{e}");
    // Compression 2 rows start at a byte, so the strip of the first six rows is where the strip of
    // seven starts. StripByteCounts (the ninth entry, its value at offset 114) gives the six rows'.
    let (mut seven, six) = (group3_tiff(2, 0, 7), group3_tiff(2, 0, 6));
    seven[114..118].copy_from_slice(&six[114..118]);
    assert_eq!(seven[30..34], 7u32.to_le_bytes(), "ImageLength");
    assert_eq!(seven[78..82], six[78..82], "StripOffsets (the sixth entry, at offset 78)");
    let e = raster_image(&seven).err().expect("refused");
    assert!(e.to_string().contains("could not decode image: unexpected end of input"), "{e}");
    // A tile wider than the allowance: TileWidth (the eighth entry, its value at offset 102).
    let rows = fax_picture();
    let g = vectorcraft_testkit::tiff::Group3 {
        rows: &rows,
        compression: 3,
        t4_options: 1,
        photometric: 0,
        fill_order: 1,
        rows_per_strip: 7,
        tile: Some((80, 16)),
        rtc: false,
    };
    let mut wide = vectorcraft_testkit::tiff::group3(&g);
    assert_eq!(wide[102..106], 80u32.to_le_bytes(), "TileWidth");
    wide[102..106].copy_from_slice(&u32::MAX.to_le_bytes());
    let e = raster_image(&wide).err().expect("refused");
    assert!(e.to_string().contains("could not decode image: arithmetic overflow"), "{e}");
}

#[test]
fn content_beats_a_wrong_extension() {
    let png = image_bytes(2, 2, image::ImageFormat::Png);
    assert_eq!(detect("photo.jpg", &png).unwrap().id, "png");
    assert_eq!(detect("noext", b"<?xml version=\"1.0\"?><svg xmlns=\"http://www.w3.org/2000/svg\"/>").unwrap().id, "svg");
    assert_eq!(detect("x.ait", b"%PDF-1.7").unwrap().id, "ait");
    assert_eq!(detect("x.bin", b"%PDF-1.7").unwrap().id, "pdf");
    assert!(detect("x.bin", b"nothing").is_none());
}

#[test]
fn templates_open_untitled() {
    let mut s = session(100.0, 100.0, 1);
    let dir = tmp_dir("tpl");
    let path = dir.join("t.vectorcraft").to_string_lossy().to_string();
    // A template under a native file name still opens untitled.
    std::fs::write(&path, b64(&s.execute("file.saveAsTemplate", &json!({})).unwrap())).unwrap();
    let r = s.execute("document.open", &json!({"path": path})).unwrap();
    assert!(r["title"].as_str().unwrap().starts_with("Untitled-"), "{r}");
    assert_eq!(s.doc().unwrap().path, None);
    assert!(!s.doc().unwrap().doc.template);

    let pdf = b64(&s.execute("document.serialize", &json!({"format": "pdf"})).unwrap());
    let ait = dir.join("t.ait").to_string_lossy().to_string();
    std::fs::write(&ait, pdf).unwrap();
    let r = s.execute("document.open", &json!({"path": ait})).unwrap();
    assert_eq!(r["format"], "ait");
    assert!(r["title"].as_str().unwrap().starts_with("Untitled-"), "{r}");
    assert_eq!(s.doc().unwrap().path, None);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn open_exts_cover_every_readable_format() {
    for f in FORMATS.iter().filter(|f| f.read) {
        for e in f.extensions {
            assert!(OPEN_EXTS.contains(e), "OPEN_EXTS lacks .{e} ({})", f.label);
        }
    }
    for e in OPEN_EXTS {
        assert!(format(e).is_some_and(|f| f.read), ".{e} in OPEN_EXTS is no readable format");
    }
    let filters: Vec<_> = open_filters().collect();
    assert_eq!(filters[0], ("All readable files", OPEN_EXTS));
    assert_eq!(
        filters.len(),
        9 + FORMATS.iter().filter(|f| f.read).count(),
        "and swatch libraries, libraries, flattener, PDF and print presets, shortcut sets, scripts, plug-ins"
    );
    assert!(filters.contains(&("Scripts", crate::cmd::scripts::EXTS)), "File › Open checks scripts");
    assert_eq!(filters.last(), Some(&("Plug-ins", crate::cmd::plugin::EXTS)), "File › Open installs plug-ins");
}

/// The extensions usage texts list for export: one per writable format, each picking that format
/// (`.png` picks PNG; PNG-8 is asked for by `format`).
#[test]
fn export_extensions_name_every_writable_format() {
    let exts = export_extensions();
    assert_eq!(
        exts,
        [
            "vectorcraft",
            "svg",
            "svgz",
            "pdf",
            "ai",
            "png",
            "jpg",
            "gif",
            "webp",
            "tif",
            "bmp",
            "vctemplate",
            "txt",
            "dxf",
            "eps",
            "emf",
            "wmf",
            "tga",
            "psd",
            "cdr",
            "cmx"
        ]
    );
    for e in exts {
        assert!(format_for_name(&format!("x.{e}")).is_some_and(|f| f.write), "{e}");
    }
}

#[test]
fn formats_query_lists_readers_writers_and_options() {
    let mut s = Session::new();
    let r = s.execute("document.formats", &json!({})).unwrap();
    let ids = |k: &str| r[k].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect::<Vec<_>>();
    assert_eq!(
        ids("writable"),
        [
            "vectorcraft",
            "svg",
            "svgz",
            "pdf",
            "ai",
            "png",
            "jpg",
            "gif",
            "webp",
            "tiff",
            "bmp",
            "template",
            "png8",
            "txt",
            "dxf",
            "eps",
            "emf",
            "wmf",
            "tga",
            "psd",
            "cdr",
            "cmx"
        ]
    );
    assert!(ids("readable").contains(&"tiff".to_string()) && ids("readable").contains(&"ait".to_string()));
    let png = r["formats"].as_array().unwrap().iter().find(|f| f["id"] == "png").unwrap();
    assert_eq!(png["options"]["scale"]["default"], 1);
    assert_eq!(r["openExtensions"].as_array().unwrap().len(), OPEN_EXTS.len());
}

/// The format ids `command`'s params text lists for `format` (`a|b (default)|c (…)`, up to the
/// first comma), sorted.
fn listed_formats(command: &str) -> Vec<String> {
    let params = crate::cmd::find_command(command).unwrap().params;
    let (_, rest) = params.split_once("format?: ").unwrap_or_else(|| panic!("{command} lists no formats"));
    let list = rest.split(',').next().unwrap_or_default();
    let mut ids: Vec<String> = list.split('|').filter_map(|f| f.split_whitespace().next()).map(str::to_string).collect();
    ids.sort();
    ids
}

/// The params texts of the commands that take any writable format list exactly the formats each
/// command writes (.ai among them).
#[test]
fn export_params_list_every_format_the_command_writes() {
    let mut s = session(100.0, 80.0, 1);
    s.execute("shape.rectangle", &json!({"x": 10, "y": 10, "width": 50, "height": 40})).unwrap();
    s.execute("select.all", &json!({})).unwrap();
    for command in ["document.serialize", "document.export", "document.exportSelection"] {
        let mut refused = vec![];
        let mut written: Vec<String> = FORMATS
            .iter()
            .filter(|f| s.execute(command, &json!({"format": f.id})).map_err(|e| refused.push(format!("{}: {e}", f.id))).is_ok())
            .map(|f| f.id.to_string())
            .collect();
        written.sort();
        assert_eq!(listed_formats(command), written, "{command}; refused: {refused:?}");
    }
}

#[test]
fn ranges_parse_one_based() {
    assert_eq!(parse_range("1-3, 5", 5).unwrap(), [0, 1, 2, 4]);
    assert_eq!(parse_range("3-, 1", 4).unwrap(), [2, 3, 0]);
    assert_eq!(parse_range("-2", 4).unwrap(), [0, 1]);
    assert_eq!(parse_range("2,2,1\u{2013}2", 4).unwrap(), [1, 0]);
    for bad in ["", "0", "6", "3-1", "a", "-", "1-9"] {
        assert!(parse_range(bad, 5).is_err(), "{bad:?}");
    }
}

/// Pages in a PDF, counted by importing it (one artboard per page).
fn pdf_pages(bytes: &[u8]) -> usize {
    vectorcraft_pdf::import(bytes).unwrap().artboards.len()
}

#[test]
fn pdf_export_honours_artboard_and_range() {
    let mut s = session(100.0, 80.0, 3);
    let pages = |s: &mut Session, p: Value| pdf_pages(&b64(&s.execute("document.export", &p).unwrap()));
    assert_eq!(pages(&mut s, json!({"format": "pdf"})), 3);
    assert_eq!(pages(&mut s, json!({"format": "pdf", "artboard": 1})), 1);
    assert_eq!(pages(&mut s, json!({"format": "pdf", "artboards": [0, 2]})), 2);
    assert_eq!(pages(&mut s, json!({"format": "pdf", "range": "2-3"})), 2);
    assert_eq!(pdf_pages(&b64(&s.execute("document.serialize", &json!({"format": "pdf", "artboard": 2})).unwrap())), 1);
    assert!(s.execute("document.export", &json!({"format": "pdf", "artboard": 3})).is_err());
    assert!(s.execute("document.export", &json!({"format": "pdf", "range": "1-4"})).is_err());
    // Single-image formats take one artboard.
    assert!(s.execute("document.export", &json!({"format": "png", "range": "1-2"})).is_err());
    let png = b64(&s.execute("document.export", &json!({"format": "png", "range": "2", "scale": 0.5})).unwrap());
    assert_eq!(image::load_from_memory(&png).unwrap().width(), 50);
    assert!(s.execute("document.export", &json!({"format": "png", "scale": "big"})).is_err(), "typed options reject a bad type");
}

#[test]
fn export_without_path_returns_bytes_and_never_retargets() {
    let mut s = session(100.0, 100.0, 1);
    let dir = tmp_dir("retarget");
    let doc_path = dir.join("doc.vectorcraft").to_string_lossy().to_string();
    s.execute("document.save", &json!({"path": doc_path})).unwrap();
    let copy = dir.join("copy.vectorcraft").to_string_lossy().to_string();
    let r = s.execute("document.export", &json!({"path": copy})).unwrap();
    assert_eq!(r["format"], "vectorcraft");
    assert!(vectorcraft_format::sniff(&std::fs::read(&copy).unwrap()));
    assert_eq!(s.doc().unwrap().path.as_deref(), Some(doc_path.as_str()), "exporting the native format keeps the document's path");
    let r = s.execute("document.export", &json!({"format": "jpg"})).unwrap();
    assert_eq!(&b64(&r)[..2], [0xFF, 0xD8]);
    assert!(r.get("path").is_none());
    assert!(s.execute("document.export", &json!({"path": dir.join("x.dwg").to_string_lossy()})).is_err(), "DWG can't be written");
    // A never-saved document hands its native bytes back.
    let mut s = session(10.0, 10.0, 1);
    let r = s.execute("document.save", &json!({})).unwrap();
    assert!(vectorcraft_format::sniff(&b64(&r)));
    assert_eq!(s.doc().unwrap().path, None);
    let _ = std::fs::remove_dir_all(dir);
}

/// A red square on a template layer and a blue one on a normal layer above it.
fn template_doc() -> (Session, u64, u64) {
    let mut s = session(100.0, 100.0, 1);
    let template = s.doc().unwrap().doc.layers[0].id.0;
    s.execute("paint.setFill", &json!({"color": "#ff0000"})).unwrap();
    let red = s.execute("shape.rectangle", &json!({"x": 0, "y": 0, "width": 50, "height": 100})).unwrap()["id"].as_u64().unwrap();
    s.execute("layer.new", &json!({})).unwrap();
    s.execute("paint.setFill", &json!({"color": "#0000ff"})).unwrap();
    let blue = s.execute("shape.rectangle", &json!({"x": 50, "y": 0, "width": 50, "height": 100})).unwrap()["id"].as_u64().unwrap();
    s.execute("layer.setProps", &json!({"id": template, "template": true})).unwrap();
    (s, red, blue)
}

fn has_red(png: &[u8]) -> bool {
    image::load_from_memory(png).unwrap().to_rgba8().pixels().any(|p| p[0] > 200 && p[2] < 80 && p[3] > 0)
}

#[test]
fn raster_exports_leave_template_layers_out() {
    let (mut s, red, blue) = template_doc();
    let png = b64(&s.execute("document.export", &json!({"format": "png"})).unwrap());
    assert!(!has_red(&png), "no red pixels from the template layer");
    assert!(image::load_from_memory(&png).unwrap().to_rgba8().get_pixel(75, 50)[2] > 200, "the normal layer is exported");
    s.execute("select.set", &json!({"ids": [red, blue]})).unwrap();
    let r = s.execute("document.exportSelection", &json!({"format": "png"})).unwrap();
    assert!(!has_red(&b64(&r)));
    assert!(r["bounds"][0].as_f64().unwrap() > 40.0, "only the blue square (and its stroke) is exported: {}", r["bounds"]);
}

#[test]
fn export_for_screens_one_page_per_pdf_unique_names() {
    let mut s = session(100.0, 80.0, 3);
    s.execute("artboard.setProps", &json!({"index": 0, "name": "Icon"})).unwrap();
    s.execute("artboard.setProps", &json!({"index": 1, "name": "Icon"})).unwrap();
    let dir = tmp_dir("screens");
    let folder = dir.to_string_lossy().to_string();
    let r = s
        .execute(
            "document.exportForScreens",
            &json!({"folder": folder, "formats": [{"format": "pdf"}, {"format": "svg", "scale": 2}, {"format": "svg", "scale": 3}, {"format": "png", "scale": 2}]}),
        )
        .unwrap();
    let files: Vec<String> = r["files"].as_array().unwrap().iter().map(|f| f.as_str().unwrap().to_string()).collect();
    let names: Vec<&str> = files.iter().map(|f| f.rsplit('/').next().unwrap()).collect();
    assert_eq!(
        names,
        ["Icon.pdf", "Icon.svg", "Icon@2x.png", "Icon-2.pdf", "Icon-2.svg", "Icon-2@2x.png", "Artboard-3.pdf", "Artboard-3.svg", "Artboard-3@2x.png"]
    );
    for f in files.iter().filter(|f| f.ends_with(".pdf")) {
        assert_eq!(pdf_pages(&std::fs::read(f).unwrap()), 1, "{f}");
    }
    // No folder: the files come back as bytes.
    let r = s.execute("document.exportForScreens", &json!({"range": "3", "formats": [{"format": "jpg", "scale": 0.5}]})).unwrap();
    assert_eq!(r["files"][0]["name"], "Artboard-3@0.5x.jpg");
    assert_eq!(image::load_from_memory(&b64(&r["files"][0])).unwrap().width(), 50);
    // A native row: each artboard as a document of its own to edit.
    let r = s.execute("document.exportForScreens", &json!({"range": "1", "formats": [{"format": "vectorcraft"}]})).unwrap();
    assert!(r["files"][0]["name"].as_str().unwrap().ends_with(".vectorcraft"), "{r}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn template_sublayers_are_left_out_too() {
    let mut s = session(100.0, 100.0, 1);
    let sub = s.execute("layer.newSublayer", &json!({})).unwrap()["id"].as_u64().unwrap();
    s.execute("layer.setCurrent", &json!({"id": sub})).unwrap();
    s.execute("paint.setFill", &json!({"color": "#ff0000"})).unwrap();
    let red = s.execute("shape.rectangle", &json!({"x": 0, "y": 0, "width": 50, "height": 100})).unwrap()["id"].as_u64().unwrap();
    assert_eq!(s.doc().unwrap().doc.ancestry(vectorcraft_doc::NodeId(red)).unwrap().len(), 3, "the square is on a sublayer");
    assert!(has_red(&b64(&s.execute("document.export", &json!({"format": "png"})).unwrap())));
    s.execute("layer.setProps", &json!({"id": sub, "template": true})).unwrap();
    assert!(!has_red(&b64(&s.execute("document.export", &json!({"format": "png"})).unwrap())), "a template sublayer is left out");
    s.execute("select.set", &json!({"ids": [red]})).unwrap();
    assert!(s.execute("document.exportSelection", &json!({"format": "png"})).is_err(), "nothing but template art is selected");
}

#[test]
fn export_for_screens_vector_rows_drop_scale_suffixes_and_names_ignore_case() {
    let mut s = session(100.0, 80.0, 3);
    s.execute("artboard.setProps", &json!({"index": 0, "name": "Icon"})).unwrap();
    s.execute("artboard.setProps", &json!({"index": 1, "name": "icon"})).unwrap();
    std::sync::Arc::make_mut(&mut s.doc_mut().unwrap().doc).artboards[2].name.clear();
    // The dialog's second row starts as PNG @2x; switched to PDF it keeps that suffix text.
    let formats =
        json!([{"format": "png", "scale": 1, "suffix": ""}, {"format": "pdf", "scale": 2, "suffix": "@2x"}, {"format": "svg", "suffix": "-web"}]);
    let r = s.execute("document.exportForScreens", &json!({"formats": formats})).unwrap();
    let names: Vec<&str> = r["files"].as_array().unwrap().iter().map(|f| f["name"].as_str().unwrap()).collect();
    assert_eq!(
        names,
        [
            "Icon.png",
            "Icon.pdf",
            "Icon-web.svg",
            "icon-2.png",
            "icon-2.pdf",
            "icon-2-web.svg",
            "Artboard-3.png",
            "Artboard-3.pdf",
            "Artboard-3-web.svg"
        ]
    );
}

#[test]
fn oversized_raster_exports_are_refused() {
    let mut s = session(2000.0, 2000.0, 1);
    let e = s.execute("document.export", &json!({"format": "webp", "scale": 10})).unwrap_err().to_string();
    assert!(e.contains("too large"), "{e}");
    assert!(s.execute("document.export", &json!({"format": "png", "scale": 40})).is_err(), "80000 px a side");
}

#[test]
fn new_files_never_replace_a_file() {
    let d = tmp_dir("new-file");
    let path = d.join("Brand.ase").to_string_lossy().to_string();
    let _ = std::fs::remove_file(&path);
    write_new_file(&path, b"first").unwrap();
    let e = write_new_file(&path, b"second").unwrap_err().to_string();
    assert!(e.starts_with(&format!("{path}: ")), "{e}");
    assert_eq!(std::fs::read(&path).unwrap(), b"first");
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn a_native_save_streams_the_bytes_the_encoder_makes() {
    let d = tmp_dir("stream");
    let mut s = session(200.0, 100.0, 1);
    s.execute("shape.rectangle", &json!({"x": 10, "y": 10, "width": 50, "height": 20})).unwrap();
    for (name, compress) in [("a.vectorcraft", false), ("b.vectorcraft", true), ("c.vctemplate", true)] {
        let path = d.join(name).to_string_lossy().to_string();
        let p = json!({"path": path, "compress": compress});
        let r = s.execute("file.saveCopy", &p).unwrap();
        let written = std::fs::read(&path).unwrap();
        assert_eq!(r["bytes"], written.len(), "{r}");
        let plan = save_plan(&s, SaveMode::Copy, &p).unwrap();
        let job = save_job(&mut s, plan).unwrap();
        assert!(job.streams());
        assert_eq!(written, job.encode().unwrap().files[0].1, "{name}");
        assert_eq!(vectorcraft_format::load(&written).unwrap().template, name.ends_with("vctemplate"));
    }
    // A writer that fails: the save fails with its error.
    let plan = save_plan(&s, SaveMode::Copy, &json!({"path": "/x/y.vectorcraft"})).unwrap();
    let job = save_job(&mut s, plan).unwrap();
    let e = job
        .write_streamed(&mut |_, fill| {
            fill(&mut std::io::sink()).unwrap();
            Err(EngineError::Other("disk full".into()))
        })
        .unwrap_err();
    assert_eq!(e.to_string(), "disk full");
    let _ = std::fs::remove_dir_all(&d);
}

/// A native save whose file write fails partway (a full disk) leaves the file as it was, and no
/// temporary file behind.
#[test]
fn a_save_that_fails_while_streaming_leaves_the_file_as_it_was() {
    /// Takes `left` bytes, then fails as a full disk does.
    struct FailAfter<'a> {
        inner: &'a mut dyn std::io::Write,
        left: usize,
    }
    impl std::io::Write for FailAfter<'_> {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            if self.left == 0 {
                return Err(std::io::Error::other("disk full"));
            }
            let n = b.len().min(self.left);
            self.left -= n;
            self.inner.write(&b[..n])
        }
        fn flush(&mut self) -> std::io::Result<()> {
            self.inner.flush()
        }
    }
    let d = tmp_dir("stream-fail");
    let mut s = session(2000.0, 2000.0, 1);
    // Enough objects that the file is written in several pieces, the failure in the middle.
    for i in 0..1500 {
        s.execute("shape.rectangle", &json!({"x": i % 100 * 20, "y": i / 100 * 20, "width": 10, "height": 10})).unwrap();
    }
    for compress in [false, true] {
        let path = d.join("a.vectorcraft").to_string_lossy().to_string();
        s.execute("file.saveCopy", &json!({"path": path, "compress": compress})).unwrap();
        let before = std::fs::read(&path).unwrap();
        s.execute("shape.ellipse", &json!({"x": 0, "y": 0, "width": 50, "height": 50})).unwrap();
        let plan = save_plan(&s, SaveMode::Copy, &json!({"path": path, "compress": compress})).unwrap();
        let job = save_job(&mut s, plan).unwrap();
        let e = job.write_streamed(&mut |p, fill| super::write_file_with(p, &mut |w| fill(&mut FailAfter { inner: w, left: 10_000 }))).unwrap_err();
        assert!(e.to_string().contains("disk full") || e.to_string().contains("couldn't be encoded"), "{e}");
        assert_eq!(std::fs::read(&path).unwrap(), before, "compress: {compress}");
        assert_eq!(std::fs::read_dir(&d).unwrap().count(), 1, "no temporary file is left");
    }
    let _ = std::fs::remove_dir_all(&d);
}

/// A save that fails leaves File Info's dates and the saved state alone; one that works stamps them.
#[test]
fn a_failed_save_does_not_stamp_the_documents_dates() {
    let mut s = session(200.0, 100.0, 1);
    s.execute("shape.rectangle", &json!({"x": 10, "y": 10, "width": 50, "height": 20})).unwrap();
    let before = s.doc().unwrap().doc.metadata.clone();
    let dirty = s.doc().unwrap().is_dirty();
    let plan = save_plan(&s, SaveMode::SaveAs, &json!({"path": "/x/y.vectorcraft", "modified": 12345})).unwrap();
    let e = save_with(&mut s, plan, |_, _| Err(EngineError::Other("disk full".into()))).unwrap_err();
    assert_eq!(e.to_string(), "disk full");
    let st = s.doc().unwrap();
    assert_eq!(st.doc.metadata, before);
    assert_eq!(st.is_dirty(), dirty);
    // Options the encoder refuses: the same.
    assert!(s.execute("file.saveAs", &json!({"path": "/x/y.vectorcraft", "version": 99, "modified": 12345})).is_err());
    let st = s.doc().unwrap();
    assert_eq!(st.doc.metadata, before);
    assert_eq!(st.is_dirty(), dirty);
    let plan = save_plan(&s, SaveMode::SaveAs, &json!({"path": "/x/y.vectorcraft", "modified": 12345})).unwrap();
    save_with(&mut s, plan, |_, _| Ok(())).unwrap();
    let st = s.doc().unwrap();
    assert_eq!(st.doc.metadata.modified, Some(12345));
    assert!(!st.is_dirty());
    // Edited while the file is written (Background Save): the dates go on once it is, and the
    // edit keeps the document modified.
    let plan = save_plan(&s, SaveMode::SaveAs, &json!({"path": "/x/z.vectorcraft", "modified": 777})).unwrap();
    let job = save_job(&mut s, plan).unwrap();
    let uid = s.doc().unwrap().uid;
    assert_eq!(s.doc().unwrap().doc.metadata.modified, Some(12345));
    s.execute("shape.ellipse", &json!({"x": 0, "y": 0, "width": 50, "height": 50})).unwrap();
    job.write(|_, _| Ok(())).unwrap();
    job.complete(&mut s, uid);
    let st = s.doc().unwrap();
    assert_eq!(st.doc.metadata.modified, Some(777));
    assert_eq!(st.path.as_deref(), Some("/x/z.vectorcraft"));
    assert!(st.is_dirty());
    // Undoing the edit is back at what the file holds.
    s.execute("edit.undo", &json!({})).unwrap();
    assert!(!s.doc().unwrap().is_dirty());
}

// CoralCraft: V1.2 tests for .cdr and .cmx in FORMATS, OPEN_EXTS, PLACE_EXTS, document.open, and file.place.
#[test]
fn cdr_and_cmx_are_in_format_tables_and_open_and_place() {
    assert!(OPEN_EXTS.contains(&"cdr") && OPEN_EXTS.contains(&"cmx"));
    assert!(PLACE_EXTS.contains(&"cdr") && PLACE_EXTS.contains(&"cmx"));
    assert!(FORMATS.iter().any(|f| f.id == "cdr" && f.read && f.extensions.contains(&"cdr")));
    assert!(FORMATS.iter().any(|f| f.id == "cmx" && f.read && f.extensions.contains(&"cmx")));

    let mut authored = session(200.0, 100.0, 1);
    authored.execute("shape.rectangle", &json!({"x": 20, "y": 15, "width": 80, "height": 50})).unwrap();
    let vdoc = &authored.doc().unwrap().doc;
    let cdr7_bytes = corel_import::write_cdr7_document(vdoc).unwrap();
    let cdrx_bytes = corel_import::write_cdrx_zip_document(vdoc).unwrap();
    let cmx1_bytes = corel_import::write_cmx1_document(vdoc).unwrap();

    for (name, bytes, expected_fmt) in
        [("sample.cdr", cdr7_bytes.clone(), "cdr"), ("modern.cdr", cdrx_bytes.clone(), "cdr"), ("exchange.cmx", cmx1_bytes.clone(), "cmx")]
    {
        let mut s = Session::new();
        let r = open(&mut s, name, &bytes);
        assert_eq!(r["format"], expected_fmt, "{name}");
        assert_eq!(r["title"], name, "{name}");
        assert!(s.doc().unwrap().doc.node_count() >= 2, "{name}: rectangle imported");

        // Place into an active document
        let mut target = session(400.0, 300.0, 1);
        let pr = target
            .execute("file.place", &json!({"name": name, "dataBase64": vectorcraft_format::base64_encode(&bytes), "at": [200.0, 150.0]}))
            .unwrap();
        assert_eq!(pr["format"], expected_fmt, "file.place {name}");
        assert!(target.doc().unwrap().doc.node_count() >= 2, "file.place {name} added artwork");
    }
}

// CoralCraft: V1.3 tests for CDR7 / CDRX / CMX1 writers via document.export, document.serialize, file.saveAs, and document.save.
#[test]
fn cdr_and_cmx_writers_export_serialize_and_save_as() {
    assert!(SAVE_FORMATS.contains(&"cdr") && SAVE_FORMATS.contains(&"cmx"));
    assert!(FORMATS.iter().any(|f| f.id == "cdr" && f.write));
    assert!(FORMATS.iter().any(|f| f.id == "cmx" && f.write));

    let mut s = session(200.0, 100.0, 2);
    s.execute("shape.rectangle", &json!({"x": 20, "y": 15, "width": 80, "height": 50})).unwrap();
    s.execute("shape.ellipse", &json!({"x": 250, "y": 20, "width": 60, "height": 40})).unwrap();

    // 1. Export CDR7 (default)
    let cdr7_exp = s.execute("document.export", &json!({"format": "cdr"})).unwrap();
    assert_eq!(cdr7_exp["format"], "cdr");
    let cdr7_bytes = b64(&cdr7_exp);
    assert!(cdr7_bytes.starts_with(b"RIFF"), "default CDR export is CDR7 RIFF");

    // 2. Export CDRX (modern X4+ ZIP+RIFF)
    let cdrx_exp = s.execute("document.export", &json!({"format": "cdr", "cdrVersion": "cdrx"})).unwrap();
    let cdrx_bytes = b64(&cdrx_exp);
    assert!(cdrx_bytes.starts_with(b"PK\x03\x04"), "cdrx version exports ZIP+RIFF container");

    // 3. Invalid cdrVersion is rejected
    assert!(s.execute("document.export", &json!({"format": "cdr", "cdrVersion": "cdr3"})).is_err());

    // 4. Serialize CMX1
    let cmx_ser = s.execute("document.serialize", &json!({"format": "cmx"})).unwrap();
    let cmx_bytes = b64(&cmx_ser);
    assert!(cmx_bytes.starts_with(b"RIFF") && &cmx_bytes[8..12] == b"CMX1");

    // 5. Save As .cdr and .cmx, then Save and reopen
    let dir = std::env::temp_dir().join(format!("vc-cdr-save-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let cdr_path = dir.join("layout.cdr").to_string_lossy().into_owned();
    let saved_cdr = s.execute("file.saveAs", &json!({"path": cdr_path, "options": {"cdrVersion": "cdrx"}})).unwrap();
    assert_eq!(saved_cdr["format"], "cdr");
    assert_eq!(s.doc().unwrap().format, "cdr");
    assert!(std::fs::read(&cdr_path).unwrap().starts_with(b"PK\x03\x04"));

    // Subsequent document.save reuses remembered cdrVersion option
    s.execute("shape.rectangle", &json!({"x": 40, "y": 30, "width": 25, "height": 25})).unwrap();
    let resaved = s.execute("document.save", &json!({})).unwrap();
    assert_eq!(resaved["format"], "cdr");
    assert!(std::fs::read(&cdr_path).unwrap().starts_with(b"PK\x03\x04"));

    let mut reopened = Session::new();
    reopened.execute("document.open", &json!({"path": cdr_path})).unwrap();
    assert_eq!(reopened.doc().unwrap().format, "cdr");
    assert_eq!(reopened.doc().unwrap().doc.artboards.len(), 2);

    let cmx_path = dir.join("drawing.cmx").to_string_lossy().into_owned();
    let saved_cmx = s.execute("file.saveAs", &json!({"path": cmx_path})).unwrap();
    assert_eq!(saved_cmx["format"], "cmx");
    assert_eq!(s.doc().unwrap().format, "cmx");
    assert!(&std::fs::read(&cmx_path).unwrap()[8..12] == b"CMX1");

    let _ = std::fs::remove_dir_all(&dir);
}

// CoralCraft: V1.4 differential libcdr oracle and real-sample tests on the VectorCraft engine pipeline.
#[test]
fn cdr_and_cmx_engine_pipeline_passes_libcdr_oracle_and_real_samples() {
    use std::process::Command;

    let find_tool = |name: &str| -> Option<String> {
        [name.to_owned(), format!("/home/paris/.local/bin/{name}")]
            .into_iter()
            .find(|candidate| Command::new(candidate).arg("--version").output().is_ok())
    };

    let mut s = session(400.0, 300.0, 1);
    s.execute("shape.rectangle", &json!({"x": 20, "y": 20, "width": 100, "height": 60})).unwrap();
    s.execute("shape.ellipse", &json!({"x": 160, "y": 30, "width": 80, "height": 80})).unwrap();

    let cdr_bytes = b64(&s.execute("document.export", &json!({"format": "cdr"})).unwrap());
    let cmx_bytes = b64(&s.execute("document.export", &json!({"format": "cmx"})).unwrap());

    if let Some(cdr2raw) = find_tool("cdr2raw") {
        let tmp = std::env::temp_dir().join(format!("vc_engine_oracle_{}.cdr", std::process::id()));
        std::fs::write(&tmp, &cdr_bytes).unwrap();
        let out = Command::new(&cdr2raw).arg(&tmp).output().unwrap();
        let _ = std::fs::remove_file(&tmp);
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(out.status.success(), "cdr2raw failed: {}", String::from_utf8_lossy(&out.stderr));
        assert!(stdout.contains("startDocument") && stdout.matches("drawPath").count() >= 2);
    }

    if let Some(cmx2raw) = find_tool("cmx2raw") {
        let tmp = std::env::temp_dir().join(format!("vc_engine_oracle_{}.cmx", std::process::id()));
        std::fs::write(&tmp, &cmx_bytes).unwrap();
        let out = Command::new(&cmx2raw).arg(&tmp).output().unwrap();
        let _ = std::fs::remove_file(&tmp);
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(out.status.success(), "cmx2raw failed: {}", String::from_utf8_lossy(&out.stderr));
        assert!(stdout.contains("startDocument") && stdout.matches("drawPath").count() >= 2);
    }

    let samples_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../refs/cdr-samples");
    if samples_dir.is_dir() {
        let mut stack = vec![samples_dir];
        let mut opened = 0usize;
        let mut with_objects = 0usize;
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("cdr")) {
                    let bytes = std::fs::read(&path).unwrap();
                    if coralcraft_cdr::probe(&bytes).is_err() {
                        continue;
                    }
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("sample.cdr");
                    let loaded = super::load::load(name, &bytes).unwrap_or_else(|e| panic!("engine load failed on {}: {e}", path.display()));
                    assert_eq!(loaded.format.id, "cdr");
                    let metrics = corel_import::document_metrics(&loaded.doc);
                    assert!(metrics.page_count >= 1 && metrics.layer_count >= 1);
                    if metrics.object_count > 0 {
                        with_objects += 1;
                    }
                    opened += 1;
                }
            }
        }
        assert!(opened >= 64, "expected at least 64 real-world .cdr samples opened through engine, got {opened}");
        assert!(with_objects >= 20, "expected >= 20 real-world .cdr samples with imported objects, got {with_objects}");
    }
}

// CoralCraft: V2.1 compatibility matrix test over every entry in VectorCraft's FORMATS table.
#[test]
fn vectorcraft_format_compatibility_matrix_covers_every_format_v2_1() {
    // 1. CI fails if any upstream VectorCraft format or CoralCraft format is missing from FORMATS.
    let required_formats: &[(&str, bool, bool, &[&str])] = &[
        ("vectorcraft", true, true, &["vectorcraft", "drawcraft"]),
        ("svg", true, true, &["svg"]),
        ("svgz", true, true, &["svgz"]),
        ("pdf", true, true, &["pdf"]),
        ("ai", true, true, &["ai"]),
        ("ait", true, false, &["ait"]),
        ("png", true, true, &["png"]),
        ("jpg", true, true, &["jpg", "jpeg"]),
        ("gif", true, true, &["gif"]),
        ("webp", true, true, &["webp"]),
        ("tiff", true, true, &["tif", "tiff"]),
        ("bmp", true, true, &["bmp"]),
        ("template", true, true, &["vctemplate"]),
        ("png8", false, true, &["png"]),
        ("txt", false, true, &["txt"]),
        ("dxf", true, true, &["dxf"]),
        ("eps", true, true, &["eps", "epsf"]),
        ("emf", true, true, &["emf"]),
        ("wmf", true, true, &["wmf"]),
        ("tga", true, true, &["tga", "vda", "icb", "vst"]),
        ("psd", true, true, &["psd"]),
        ("affinity", true, false, &["af", "afdesign", "afpub"]),
        ("psb", true, false, &["psb"]),
        ("ps", true, false, &["ps"]),
        ("cdr", true, true, &["cdr"]),
        ("cmx", true, true, &["cmx"]),
    ];

    for &(id, want_read, want_write, want_exts) in required_formats {
        let fmt = FORMATS.iter().find(|f| f.id == id).unwrap_or_else(|| panic!("missing required format `{id}` in FORMATS"));
        assert_eq!(fmt.read, want_read, "format `{id}` read flag mismatch");
        assert_eq!(fmt.write, want_write, "format `{id}` write flag mismatch");
        assert_eq!(fmt.extensions, want_exts, "format `{id}` extensions mismatch");
    }
    assert_eq!(FORMATS.len(), required_formats.len(), "every FORMATS entry must be covered by the V2.1 compatibility matrix");

    // 2. Author a document with both vector geometry and point type so every writable format (including txt) exports non-empty output.
    let mut authored = session(200.0, 120.0, 1);
    authored.execute("shape.rectangle", &json!({"x": 20, "y": 15, "width": 80, "height": 50})).unwrap();
    authored.execute("text.create", &json!({"x": 30, "y": 90, "text": "CoralCraft Matrix"})).unwrap();

    // Helper for read-only PSB (2x1 8-bit RGB PSB header + empty sections + raw pixels)
    let synth_psb = || -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(b"8BPS");
        b.extend_from_slice(&2u16.to_be_bytes()); // version 2 = PSB
        b.extend_from_slice(&[0u8; 6]);
        b.extend_from_slice(&3u16.to_be_bytes()); // 3 channels
        b.extend_from_slice(&1u32.to_be_bytes()); // height = 1
        b.extend_from_slice(&2u32.to_be_bytes()); // width = 2
        b.extend_from_slice(&8u16.to_be_bytes()); // depth = 8
        b.extend_from_slice(&3u16.to_be_bytes()); // mode = 3 (RGB)
        b.extend_from_slice(&0u32.to_be_bytes()); // color mode data length (4B even in PSB)
        b.extend_from_slice(&0u32.to_be_bytes()); // image resources length (4B even in PSB)
        b.extend_from_slice(&0u64.to_be_bytes()); // layer/mask section length (8B in PSB)
        b.extend_from_slice(&0u16.to_be_bytes()); // compression 0 = Raw
        b.extend_from_slice(&[220, 40, 30, 200, 10, 180]); // R, G, B planes for 2x1
        b
    };

    // Helper for read-only Affinity container with embedded PNG preview
    let synth_affinity = || -> Vec<u8> {
        let png = image_bytes(2, 2, image::ImageFormat::Png);
        let mut b = vec![0; 72];
        b[..4].copy_from_slice(vectorcraft_affinity::MAGIC);
        b[4..6].copy_from_slice(&12u16.to_le_bytes());
        b[8..12].copy_from_slice(b"nsrP");
        b[12..16].copy_from_slice(b"#Inf");
        b[24..32].copy_from_slice(&72u64.to_le_bytes());
        b[64..68].copy_from_slice(b"Prot");
        b.extend_from_slice(b"\xff\xff\xff\xffThmb");
        b.extend_from_slice(&1u32.to_le_bytes());
        b.extend_from_slice(&(png.len() as u32 + 13).to_le_bytes());
        b.extend_from_slice(&29u32.to_le_bytes());
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend_from_slice(&(png.len() as u32).to_le_bytes());
        b.push(1);
        b.extend_from_slice(&png);
        b
    };

    // 3. Exercise every entry in FORMATS through export/serialize (when writable) and document.open (when readable).
    for fmt in FORMATS {
        let exported_bytes = if fmt.write {
            let exp = authored
                .execute("document.export", &json!({"format": fmt.id}))
                .unwrap_or_else(|e| panic!("document.export failed for `{}`: {e}", fmt.id));
            assert_eq!(exp["format"], fmt.id);
            let bytes = b64(&exp);
            assert!(!bytes.is_empty(), "document.export produced empty bytes for `{}`", fmt.id);

            let ser = authored
                .execute("document.serialize", &json!({"format": fmt.id}))
                .unwrap_or_else(|e| panic!("document.serialize failed for `{}`: {e}", fmt.id));
            if fmt.id == "svg" {
                assert!(!ser["text"].as_str().unwrap_or_default().is_empty(), "svg serialize text empty");
            } else {
                assert!(!b64(&ser).is_empty(), "serialize dataBase64 empty for `{}`", fmt.id);
            }
            Some(bytes)
        } else {
            None
        };

        if fmt.read {
            let sample_bytes = match fmt.id {
                "ait" => {
                    let dir = tmp_dir("matrix-ait");
                    let ait_path = dir.join("matrix.ait").to_string_lossy().into_owned();
                    authored.execute("file.saveAsTemplate", &json!({"path": ait_path})).unwrap();
                    let bytes = std::fs::read(&ait_path).unwrap();
                    let _ = std::fs::remove_dir_all(&dir);
                    bytes
                }
                "ps" => b64(&authored.execute("document.export", &json!({"format": "eps"})).unwrap()),
                "psb" => synth_psb(),
                "affinity" => synth_affinity(),
                _ => exported_bytes.unwrap_or_else(|| panic!("no sample bytes for readable format `{}`", fmt.id)),
            };

            for &ext in fmt.extensions {
                let file_name = format!("matrix_test.{ext}");
                let mut reader_session = Session::new();
                let opened = open(&mut reader_session, &file_name, &sample_bytes);
                assert_eq!(opened["format"], fmt.id, "opening `{file_name}` must report format `{}`", fmt.id);
                let doc = &reader_session.doc().unwrap().doc;
                assert!(!doc.artboards.is_empty(), "`{file_name}` must have at least 1 artboard");
                assert!(!doc.layers.is_empty(), "`{file_name}` must have at least 1 layer");
            }
        }
    }
}

// CoralCraft: V2.2 lossless round-trip for .vectorcraft / .drawcraft / .vctemplate with namespaced "coralcraft" extension block.
#[test]
fn vectorcraft_drawcraft_and_vctemplate_preserve_coralcraft_extension_block_losslessly_v2_2() {
    use corel_import::coralcraft_color::{Color as CdrColor, PaletteSwatch};
    use corel_import::coralcraft_doc::{
        ChunkTag, Document as CdrDocument, FillStyle, Layer as CdrLayer, LayerId, LayerRole, ObjectId, ObjectKind, ObjectNode, RawChunk, RawPayload,
        UnsupportedObject,
    };
    use corel_import::coralcraft_geom::{F64, Rect as CdrRect, RectSpec};

    // 1. Pure VectorCraft document saves and opens without injecting "coralcraft" into unknown.
    let mut pure = session(300.0, 200.0, 1);
    pure.execute("shape.rectangle", &json!({"x": 10, "y": 10, "width": 100, "height": 80})).unwrap();
    let pure_bytes = b64(&pure.execute("document.serialize", &json!({"format": "vectorcraft"})).unwrap());
    let pure_loaded = vectorcraft_format::load(&pure_bytes).unwrap();
    assert!(!pure_loaded.unknown.contains_key(corel_import::CORALCRAFT_UNKNOWN_KEY));

    // 2. Construct a rich CorelDRAW document with pages, master layers, CDR residuals, and an unsupported object.
    let mut cdr_doc = CdrDocument::new_a4_portrait();
    cdr_doc.title = "CorelExtensionRoundtrip".to_owned();
    cdr_doc.palette.swatches.push(PaletteSwatch { name: "Coral Red".to_owned(), color: CdrColor::rgb8(230, 50, 40) });
    cdr_doc.residual.chunks.push(RawChunk {
        tag: ChunkTag(*b"cdoc"),
        flags: 0,
        slot: 0,
        payload: RawPayload(vec![0xCA, 0xFE, 0xBA, 0xBE]),
        src_offset: 0,
    });

    let mut master_odd = CdrLayer::new_normal(LayerId(77), "Odd Page Header");
    master_odd.role = LayerRole::MasterOddPages;
    master_odd.master_in_front = true;
    master_odd.residual.chunks.push(RawChunk { tag: ChunkTag(*b"modd"), flags: 0, slot: 0, payload: RawPayload(vec![1, 2, 3]), src_offset: 0 });
    cdr_doc.master_layers.push(master_odd);

    cdr_doc.pages[0].name = "Cover Page".to_owned();
    cdr_doc.pages[0].bleed = F64::new(9.0).unwrap();
    cdr_doc.pages[0].background = Some(FillStyle::uniform(CdrColor::rgb8(250, 248, 240)));
    cdr_doc.pages[0].residual.chunks.push(RawChunk {
        tag: ChunkTag(*b"cpag"),
        flags: 0,
        slot: 0,
        payload: RawPayload(vec![0x10, 0x20, 0x30]),
        src_offset: 0,
    });

    let unsup = ObjectNode::new(
        ObjectId(88),
        Some("CustomEffectPlugin".to_owned()),
        ObjectKind::Unsupported(UnsupportedObject {
            tag: ChunkTag(*b"fx99"),
            class_hint: Some("CustomPlugin".to_owned()),
            payload: RawPayload(vec![9, 8, 7, 6, 5]),
            fallback_bounds: Some(CdrRect::new(40.0, 50.0, 140.0, 120.0).unwrap()),
            absolute_geometry: true,
            children: Vec::new(),
            residual: Default::default(),
        }),
        None,
        None,
    );
    cdr_doc.pages[0].layers[0].objects.push(unsup);
    cdr_doc.pages[0].layers[0].objects.push(ObjectNode::new(
        ObjectId(89),
        Some("CoverBox".to_owned()),
        ObjectKind::Rect(RectSpec::sharp(CdrRect::new(20.0, 30.0, 120.0, 90.0).unwrap())),
        Some(FillStyle::uniform(CdrColor::rgb8(230, 50, 40))),
        None,
    ));

    let cdr_bytes = coralcraft_cdr::write_cdr7(&cdr_doc).unwrap();
    let mut s = Session::new();
    open(&mut s, "brochure.cdr", &cdr_bytes);

    // Also insert a non-CoralCraft foreign key into `document.unknown` to verify coexistence.
    std::sync::Arc::make_mut(&mut s.doc_mut().unwrap().doc).unknown.insert("thirdPartyPlugin".to_owned(), json!({"enabled": true}));

    // 3. Round-trip through .vectorcraft (uncompressed & compressed, versions 1..=4), .drawcraft, and .vctemplate.
    let dir = tmp_dir("v2-2-ext");
    for (file_name, fmt_id, compress, version) in [
        ("doc_v4.vectorcraft", "vectorcraft", false, 4u32),
        ("doc_v4_gz.vectorcraft", "vectorcraft", true, 4u32),
        ("doc_v3.vectorcraft", "vectorcraft", true, 3u32),
        ("doc_v2.drawcraft", "vectorcraft", false, 2u32),
        ("doc_v1.drawcraft", "vectorcraft", false, 1u32),
        ("tpl.vctemplate", "template", true, 4u32),
    ] {
        let path = dir.join(file_name).to_string_lossy().into_owned();
        s.execute("document.export", &json!({"path": path, "format": fmt_id, "compress": compress, "version": version}))
            .unwrap_or_else(|e| panic!("export {file_name} failed: {e}"));
        let raw_bytes = std::fs::read(&path).unwrap();

        // Plain VectorCraft format loader reads the file cleanly and keeps both unknown blocks intact.
        let vdoc = vectorcraft_format::load(&raw_bytes).unwrap_or_else(|e| panic!("vectorcraft_format::load({file_name}) failed: {e}"));
        assert_eq!(vdoc.unknown.get("thirdPartyPlugin"), Some(&json!({"enabled": true})), "{file_name} preserved third-party unknown key");
        let ext = corel_import::read_coralcraft_data(&vdoc).unwrap_or_else(|| panic!("{file_name} missing coralcraft extension block"));
        assert_eq!(ext.pages.len(), 1);
        assert_eq!(ext.pages[0].name, "Cover Page");
        assert!((ext.pages[0].bleed.get() - 9.0).abs() < 1e-6);
        assert!(ext.pages[0].residual.chunks.iter().any(|c| &c.tag.0 == b"cpag"));
        assert!(ext.residual.chunks.iter().any(|c| &c.tag.0 == b"cdoc"));
        assert!(ext.master_layers.iter().any(|ml| ml.name == "Odd Page Header" && ml.role == LayerRole::MasterOddPages));

        // Open through engine and export back to CDR: all CorelDRAW residuals and unsupported objects survive.
        let mut r_sess = Session::new();
        let opened = open(&mut r_sess, file_name, &raw_bytes);
        assert_eq!(opened["format"], fmt_id, "{file_name}");
        let exported_cdr = corel_import::export_document(&r_sess.doc().unwrap().doc).unwrap();
        assert_eq!(exported_cdr.pages[0].name, "Cover Page");
        assert_eq!(exported_cdr.pages[0].background, Some(FillStyle::uniform(CdrColor::rgb8(250, 248, 240))));
        assert!(exported_cdr.residual.chunks.iter().any(|c| &c.tag.0 == b"cdoc" && c.payload.0 == vec![0xCA, 0xFE, 0xBA, 0xBE]));
        assert!(exported_cdr.pages[0].residual.chunks.iter().any(|c| &c.tag.0 == b"cpag"));
        assert!(exported_cdr.master_layers.iter().any(|ml| ml.role == LayerRole::MasterOddPages));
        assert!(
            exported_cdr.pages[0].layers[0]
                .objects
                .iter()
                .any(|o| matches!(&o.kind, ObjectKind::Unsupported(u) if &u.tag.0 == b"fx99" && u.payload.0 == vec![9, 8, 7, 6, 5]))
        );
    }

    // 4. Editing artboards in engine and saving updates the "coralcraft" extension block's page list while keeping residuals.
    s.execute("artboard.setProps", &json!({"index": 0, "name": "Renamed Cover"})).unwrap();
    s.execute("artboard.new", &json!({"name": "Back Cover", "width": 595, "height": 842})).unwrap();
    let edited_bytes = b64(&s.execute("document.serialize", &json!({"format": "vectorcraft"})).unwrap());
    let edited_vdoc = vectorcraft_format::load(&edited_bytes).unwrap();
    let edited_ext = corel_import::read_coralcraft_data(&edited_vdoc).unwrap();
    assert_eq!(edited_ext.pages.len(), 2);
    assert_eq!(edited_ext.pages[0].name, "Renamed Cover");
    assert!((edited_ext.pages[0].bleed.get() - 9.0).abs() < 1e-6, "page 0 bleed preserved across artboard rename");
    assert_eq!(edited_ext.pages[1].name, "Back Cover");

    let _ = std::fs::remove_dir_all(&dir);
}
