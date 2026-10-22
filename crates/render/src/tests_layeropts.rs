//! Layer Options on screen: Preview off draws a layer in outline, Dim Images fades its images;
//! exports (which leave templates out) draw both as they are.

use vectorcraft_color::{Color, Paint};
use vectorcraft_doc::{Appearance, ImageBlob, ImageObject, Node, NodeId, NodeKind};
use vectorcraft_geom::shapes;

use super::*;

/// A red square (10..40) and a 30×30 red image at (50, 10), on one layer set up by `layer`.
fn doc(layer: impl FnOnce(&mut NodeKind)) -> Document {
    let mut d = Document::new(100.0, 100.0);
    let img = image::RgbaImage::from_pixel(30, 30, image::Rgba([255, 0, 0, 255]));
    let mut png = vec![];
    img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
    d.images.insert("red".into(), ImageBlob::new("image/png", png));
    let l = d.layers[0].id;
    let red = Appearance::basic(Paint::solid(Color::rgb(1.0, 0.0, 0.0)), Paint::None, 0.0);
    let sq = Node::path(d.alloc_id(), shapes::rectangle(Rect::new(10.0, 10.0, 40.0, 40.0)), red);
    d.insert(Some(l), 0, sq).unwrap();
    let mut im = Node::new(
        NodeId(0),
        NodeKind::Image(Box::new(ImageObject {
            key: "red".into(),
            width: 30,
            height: 30,
            xf: Affine::translate((50.0, 10.0)),
            link: None,
            placement: Default::default(),
        })),
    );
    im.id = d.alloc_id();
    d.insert(Some(l), 9, im).unwrap();
    layer(&mut d.node_mut(l).unwrap().kind);
    d
}

fn render(d: &Document, export: bool) -> Rendered {
    let opts = RenderOptions { background: Some([255, 255, 255, 255]), skip_templates: export, ..Default::default() };
    Renderer::new().render(d, 100, 100, Affine::IDENTITY, &opts)
}

#[test]
fn preview_off_draws_the_layer_in_outline_on_screen_only() {
    let d = doc(|k| {
        if let NodeKind::Layer { preview, .. } = k {
            *preview = false;
        }
    });
    assert_eq!(render(&d, false).pixel(25, 25), [255, 255, 255, 255], "no fill in outline");
    assert_eq!(render(&d, true).pixel(25, 25), [255, 0, 0, 255], "exports draw it as it is");
}

#[test]
fn dim_images_fades_only_the_images() {
    let plain = render(&doc(|_| {}), false);
    assert_eq!(plain.pixel(65, 25), [255, 0, 0, 255]);
    let d = doc(|k| {
        if let NodeKind::Layer { dim_images, .. } = k {
            *dim_images = Some(50);
        }
    });
    let img = render(&d, false);
    let [r, g, b, _] = img.pixel(65, 25);
    assert!(r == 255 && (120..=135).contains(&g) && g == b, "half way to white: {:?}", img.pixel(65, 25));
    assert_eq!(img.pixel(25, 25), [255, 0, 0, 255], "vector art stays");
    assert_eq!(render(&d, true).pixel(65, 25), [255, 0, 0, 255], "exports are not dimmed");
}

/// #1683: a template layer dims its images by its Dim Images percentage and nothing else: with
/// the option off they show at full strength, and the layer's paths are never faded.
#[test]
fn a_template_layer_dims_its_images_by_dim_images() {
    let template = |dim: Option<u8>| {
        let d = doc(|k| {
            if let NodeKind::Layer { template, dim_images, .. } = k {
                (*template, *dim_images) = (true, dim);
            }
        });
        let img = render(&d, false);
        (img.pixel(65, 25), img.pixel(25, 25))
    };
    let red = [255, 0, 0, 255];
    assert_eq!(template(None), (red, red), "Dim Images off: as it is");
    let (image, path) = template(Some(50));
    assert!(image[0] == 255 && (120..=135).contains(&image[1]) && image[1] == image[2], "half way to white: {image:?}");
    assert_eq!(path, red, "vector art stays");
    let (image, _) = template(Some(20));
    assert!((198..=210).contains(&image[1]) && image[1] == image[2], "a fifth of its strength: {image:?}");
    assert_eq!(template(Some(100)), (red, red));
}

/// In Outline view a template layer still draws in Preview, dimmed, so its picture can be traced;
/// the layer above it draws in outline (#1486).
#[test]
fn a_template_layer_stays_in_preview_in_outline_view() {
    let mut d = doc(|k| {
        if let NodeKind::Layer { template, preview, dim_images, .. } = k {
            // As Layer Options makes a template: its images dimmed to 50 %.
            (*template, *dim_images) = (true, Some(50));
            // Preview off doesn't put a template in outline either.
            *preview = false;
        }
    });
    let top = Node::layer(d.alloc_id(), "Drawing", vectorcraft_doc::LayerColor::Preset(0));
    let top_id = d.insert(None, 9, top).unwrap();
    let red = Appearance::basic(Paint::solid(Color::rgb(1.0, 0.0, 0.0)), Paint::None, 0.0);
    let sq = Node::path(d.alloc_id(), shapes::rectangle(Rect::new(10.0, 60.0, 40.0, 90.0)), red);
    d.insert(Some(top_id), 0, sq).unwrap();
    let draw = |outline: bool| {
        let opts = RenderOptions { background: Some([255, 255, 255, 255]), outline, ..Default::default() };
        Renderer::new().render(&d, 100, 100, Affine::IDENTITY, &opts)
    };
    let (preview, outline) = (draw(false), draw(true));
    let [r, g, b, _] = outline.pixel(65, 25);
    assert!(r == 255 && (120..=135).contains(&g) && g == b, "the template's image, dimmed: {:?}", outline.pixel(65, 25));
    assert_eq!(outline.pixel(65, 25), preview.pixel(65, 25), "as in Preview");
    assert_eq!(outline.pixel(25, 75), [255, 255, 255, 255], "the drawing layer has no fill in outline");
    assert_eq!(preview.pixel(25, 75), [255, 0, 0, 255]);
}

/// View › Hide Template (#2000) leaves template layers out of the screen view and nothing else:
/// a guide path on another layer still draws, unlike in an export.
#[test]
fn hide_template_leaves_only_the_template_layers_out() {
    let mut d = doc(|k| {
        if let NodeKind::Layer { template, .. } = k {
            *template = true;
        }
    });
    let top = d.add_layer(Some("Guides"));
    let mut g = Node::path(d.alloc_id(), shapes::rectangle(Rect::new(70.0, 70.0, 90.0, 90.0)), Appearance::default());
    if let NodeKind::Path { guide, .. } = &mut g.kind {
        *guide = true;
    }
    d.insert(Some(top), 0, g).unwrap();
    let render = |hide: bool| {
        let opts = RenderOptions { background: Some([255, 255, 255, 255]), hide_templates: hide, ..Default::default() };
        Renderer::new().render(&d, 100, 100, Affine::IDENTITY, &opts)
    };
    let guide_drawn = |r: &Rendered| (60..100).flat_map(|y| (60..100).map(move |x| (x, y))).any(|(x, y)| r.pixel(x, y) != [255, 255, 255, 255]);
    let shown = render(false);
    assert_ne!(shown.pixel(25, 25), [255, 255, 255, 255], "the template layer's square shows (dimmed)");
    assert!(guide_drawn(&shown));
    let hidden = render(true);
    assert_eq!(hidden.pixel(25, 25), [255, 255, 255, 255], "Hide Template: the template layer is left out");
    assert_eq!(hidden.pixel(65, 25), [255, 255, 255, 255], "its image too");
    assert!(guide_drawn(&hidden), "the guide on the other layer still draws");
}

/// A guide path draws on screen only: exports leave it out, as they leave templates out (and as
/// the SVG, PSD and PDF exports and printing already do).
#[test]
fn guides_draw_on_screen_only() {
    let mut d = Document::new(100.0, 100.0);
    let l = d.layers[0].id;
    let mut g = Node::path(d.alloc_id(), shapes::rectangle(Rect::new(70.0, 70.0, 90.0, 90.0)), Appearance::default());
    if let NodeKind::Path { guide, .. } = &mut g.kind {
        *guide = true;
    }
    d.insert(Some(l), 0, g).unwrap();
    let drawn = |export: bool| {
        let r = render(&d, export);
        (60..100).flat_map(|y| (60..100).map(move |x| (x, y))).filter(|&(x, y)| r.pixel(x, y) != [255, 255, 255, 255]).count()
    };
    assert!(drawn(false) > 0, "on screen the guide shows");
    assert_eq!(drawn(true), 0, "exports leave it out");
}

#[test]
fn coreldraw_master_layers_all_odd_even_and_desktop_render_order_v3_2() {
    use vectorcraft_doc::{Artboard, CorelLayerRole, LayerColor};

    let mut d = Document::new(100.0, 100.0);
    d.artboards = vec![
        Artboard { id: 1, name: "Page 1".into(), rect: Rect::new(0.0, 0.0, 100.0, 100.0), show_center_mark: false, show_cross_hairs: false },
        Artboard { id: 2, name: "Page 2".into(), rect: Rect::new(140.0, 0.0, 240.0, 100.0), show_center_mark: false, show_cross_hairs: false },
        Artboard { id: 3, name: "Page 3".into(), rect: Rect::new(280.0, 0.0, 380.0, 100.0), show_center_mark: false, show_cross_hairs: false },
    ];
    d.layers.clear();

    let solid = |r: u8, g: u8, b: u8| Appearance::basic(Paint::solid(Color::rgb8(r, g, b)), Paint::None, 0.0);

    // Insert layers in deliberately scrambled order in `d.layers` to verify CorelDRAW bucket ordering:
    // 1. Desktop layer (non-printable by default, renders on top on screen, skipped in page exports)
    let mut l_desk = Node::layer(d.alloc_id(), "Desktop", LayerColor::Preset(0));
    if let NodeKind::Layer { printable, .. } = &mut l_desk.kind {
        *printable = false;
    }
    let desk_id = d.insert(None, usize::MAX, l_desk).unwrap();
    d.set_corel_layer_info(desk_id, Some(CorelLayerRole::Desktop), Some(false), None);
    let sq_desk_p1 = Node::path(d.alloc_id(), shapes::rectangle(Rect::new(10.0, 10.0, 30.0, 30.0)), solid(255, 0, 255));
    let sq_desk_gap = Node::path(d.alloc_id(), shapes::rectangle(Rect::new(105.0, 10.0, 135.0, 40.0)), solid(255, 0, 255));
    d.insert(Some(desk_id), usize::MAX, sq_desk_p1).unwrap();
    d.insert(Some(desk_id), usize::MAX, sq_desk_gap).unwrap();

    // 2. Foreground Master (Odd Pages) — must render in front of normal page layers on Pages 1 and 3 only
    let l_fg_odd = Node::layer(d.alloc_id(), "Master Odd FG", LayerColor::Preset(1));
    let fg_odd_id = d.insert(None, usize::MAX, l_fg_odd).unwrap();
    d.set_corel_layer_info(fg_odd_id, Some(CorelLayerRole::MasterOddPages), Some(true), None);
    let sq_fg_odd = Node::path(d.alloc_id(), shapes::rectangle(Rect::new(40.0, 40.0, 60.0, 60.0)), solid(0, 0, 255));
    d.insert(Some(fg_odd_id), usize::MAX, sq_fg_odd).unwrap();

    // 3. Normal Layer — page objects on Pages 1, 2, 3
    let l_norm = Node::layer(d.alloc_id(), "Layer 1", LayerColor::Preset(2));
    let norm_id = d.insert(None, usize::MAX, l_norm).unwrap();
    d.set_corel_layer_info(norm_id, Some(CorelLayerRole::Normal), Some(false), None);
    for x_off in [0.0, 140.0, 280.0] {
        let sq = Node::path(d.alloc_id(), shapes::rectangle(Rect::new(x_off + 35.0, 35.0, x_off + 65.0, 65.0)), solid(0, 255, 0));
        d.insert(Some(norm_id), usize::MAX, sq).unwrap();
    }
    let sq_yellow_p1 = Node::path(d.alloc_id(), shapes::rectangle(Rect::new(10.0, 10.0, 30.0, 30.0)), solid(255, 255, 0));
    d.insert(Some(norm_id), usize::MAX, sq_yellow_p1).unwrap();

    // 4. Background Master (All Pages) — must render behind normal layers on Pages 1, 2, 3
    let l_bg_all = Node::layer(d.alloc_id(), "Master All BG", LayerColor::Preset(3));
    let bg_all_id = d.insert(None, usize::MAX, l_bg_all).unwrap();
    d.set_corel_layer_info(bg_all_id, Some(CorelLayerRole::MasterAllPages), Some(false), None);
    let sq_bg_all = Node::path(d.alloc_id(), shapes::rectangle(Rect::new(5.0, 5.0, 95.0, 95.0)), solid(255, 0, 0));
    d.insert(Some(bg_all_id), usize::MAX, sq_bg_all).unwrap();

    // 5. Background Master (Even Pages) — must render on Page 2 only, above bg_all and behind normal
    let l_bg_even = Node::layer(d.alloc_id(), "Master Even BG", LayerColor::Preset(4));
    let bg_even_id = d.insert(None, usize::MAX, l_bg_even).unwrap();
    d.set_corel_layer_info(bg_even_id, Some(CorelLayerRole::MasterEvenPages), Some(false), None);
    let sq_bg_even = Node::path(d.alloc_id(), shapes::rectangle(Rect::new(70.0, 10.0, 90.0, 30.0)), solid(0, 255, 255));
    d.insert(Some(bg_even_id), usize::MAX, sq_bg_even).unwrap();

    let mut renderer = Renderer::new();
    let opts = RenderOptions { background: Some([255, 255, 255, 255]), ..Default::default() };
    let screen = renderer.render(&d, 400, 100, Affine::IDENTITY, &opts);

    // MasterAllPages (Red) draws on all 3 pages at local (80, 80)
    assert_eq!(screen.pixel(80, 80), [255, 0, 0, 255], "Page 1 MasterAllPages background");
    assert_eq!(screen.pixel(140 + 80, 80), [255, 0, 0, 255], "Page 2 MasterAllPages background");
    assert_eq!(screen.pixel(280 + 80, 80), [255, 0, 0, 255], "Page 3 MasterAllPages background");

    // Normal layer (Green) draws in front of MasterAllPages (Red) at local (38, 38)
    assert_eq!(screen.pixel(38, 38), [0, 255, 0, 255], "Page 1 Normal above BG Master");
    assert_eq!(screen.pixel(140 + 38, 38), [0, 255, 0, 255], "Page 2 Normal above BG Master");
    assert_eq!(screen.pixel(280 + 38, 38), [0, 255, 0, 255], "Page 3 Normal above BG Master");

    // MasterEvenPages (Cyan) draws at local (80, 20) ONLY on Page 2 (Even)
    assert_eq!(screen.pixel(80, 20), [255, 0, 0, 255], "Page 1 (Odd) does not show MasterEvenPages");
    assert_eq!(screen.pixel(140 + 80, 20), [0, 255, 255, 255], "Page 2 (Even) shows MasterEvenPages");
    assert_eq!(screen.pixel(280 + 80, 20), [255, 0, 0, 255], "Page 3 (Odd) does not show MasterEvenPages");

    // Foreground MasterOddPages (Blue) draws in front of Normal (Green) at local (50, 50) on Pages 1 & 3 only
    assert_eq!(screen.pixel(50, 50), [0, 0, 255, 255], "Page 1 (Odd) shows FG MasterOddPages in front of Normal");
    assert_eq!(screen.pixel(140 + 50, 50), [0, 255, 0, 255], "Page 2 (Even) shows Normal Green without MasterOddPages");
    assert_eq!(screen.pixel(280 + 50, 50), [0, 0, 255, 255], "Page 3 (Odd) shows FG MasterOddPages in front of Normal");

    // Desktop layer (Magenta) draws on screen over Page 1 at (20, 20) and in the pasteboard gap at (120, 20)
    assert_eq!(screen.pixel(20, 20), [255, 0, 255, 255], "Desktop layer draws on top on screen");
    assert_eq!(screen.pixel(120, 20), [255, 0, 255, 255], "Desktop layer draws in cross-page pasteboard gap");

    // Exporting Page 1 via `render_region` leaves out the non-printable Desktop layer, revealing Yellow (255, 255, 0)
    let exported_p1 = renderer.render_region(&d, d.artboards[0].rect, 1.0, true);
    assert_eq!(exported_p1.pixel(20, 20), [255, 255, 0, 255], "non-printable Desktop layer is excluded from page export");
}
