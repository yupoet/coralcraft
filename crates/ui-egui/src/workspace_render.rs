//! Pixel-accurate classic CorelDRAW full-workspace rasterizer (`render_workspace_png`).
//!
//! Renders the complete 7-panel CorelDRAW desktop workspace (Menu Bar, Standard Toolbar,
//! Property Bar, Left Toolbox with 10 vector tool glyphs, Rulers with major/medium/minor ticks,
//! `#808080` Pasteboard, `#404040` Page Drop-Shadow, 4×4 MSAA Page Canvas, 8 solid `6×6`
//! `#000000` Selection Handles + Center `X`, Right Color Palette Bar with `[X]` red slash,
//! Right `Objects`/`Properties` Docker, Bottom Page Navigator tabs, and Status Bar chips)
//! using an embedded clean-room 5×7/6×9 1-bit ASCII bitmap font with zero system-font dependency.

use coralcraft_color::Color;
use coralcraft_render::{RenderError, RenderOpts, RenderOutput, Surface, encode_png, premul_to_straight, render_page, straight_to_premul};
use coralcraft_tools::ToolKind;

use crate::{DesktopWorkspace, DockerTab, PaletteCell};

const CHROME_BG: [u8; 4] = [0xD4, 0xD0, 0xC8, 0xFF];
const BEVEL_LIGHT: [u8; 4] = [0xFF, 0xFF, 0xFF, 0xFF];
const BEVEL_DARK: [u8; 4] = [0x80, 0x80, 0x80, 0xFF];
const SHADOW_DARK: [u8; 4] = [0x40, 0x40, 0x40, 0xFF];
const TEXT_BLACK: [u8; 4] = [0x00, 0x00, 0x00, 0xFF];
const ACTIVE_TOOL_BG: [u8; 4] = [0xCC, 0xE4, 0xF7, 0xFF];
const DOCKER_SEL_BG: [u8; 4] = [0x31, 0x6A, 0xC5, 0xFF];
const SLASH_RED: [u8; 4] = [0xFF, 0x00, 0x00, 0xFF];
const PAPER_WHITE: [u8; 4] = [0xFF, 0xFF, 0xFF, 0xFF];

/// Renders the entire [`DesktopWorkspace`] window at its configured `(window_width, window_height)`
/// into a deterministic [`RenderOutput`] (premultiplied RGBA8 [`Surface`] + PNG bytes).
pub fn render_workspace_frame(ws: &DesktopWorkspace) -> Result<RenderOutput, RenderError> {
    let snap = ws.snapshot();
    let w = snap.layout.window_width.max(640);
    let h = snap.layout.window_height.max(480);

    let mut surface = Surface::new(w, h, CHROME_BG)?;
    let mut warnings = Vec::new();

    let cv = snap.layout.canvas_viewport;
    let cx0 = cv.x0.get().round() as i32;
    let cy0 = cv.y0.get().round() as i32;
    let cx1 = cv.x1.get().round() as i32;
    let cy1 = cv.y1.get().round() as i32;

    // 1. Central Pasteboard (`#808080`) inside canvas viewport (accounting for 20 px top/left rulers)
    let ruler_thick = 20;
    let pb_x0 = cx0 + ruler_thick;
    let pb_y0 = cy0 + ruler_thick;
    fill_rect(&mut surface, pb_x0, pb_y0, cx1 - pb_x0, cy1 - pb_y0, BEVEL_DARK);

    // 2. Page sheet fit-to-pasteboard + `#404040` drop-shadow + 4x4 MSAA page content
    let page = ws.engine.current_page();
    let page_w_pt = page.size.width.get().max(1.0);
    let page_h_pt = page.size.height.get().max(1.0);
    let avail_w = f64::from((cx1 - pb_x0 - 48).max(32));
    let avail_h = f64::from((cy1 - pb_y0 - 48).max(32));
    let fit_scale = (avail_w / page_w_pt).min(avail_h / page_h_pt).clamp(0.1, 2.0);

    let page_out = render_page(
        &ws.engine.document,
        ws.engine.current_page_index(),
        &RenderOpts { scale: fit_scale, transparent_background: false, export_mode: false },
    )?;
    warnings.extend(page_out.warnings);

    let pw = page_out.surface.width as i32;
    let ph = page_out.surface.height as i32;
    let page_x0 = pb_x0 + ((cx1 - pb_x0) - pw) / 2;
    let page_y0 = pb_y0 + ((cy1 - pb_y0) - ph) / 2;

    // Drop shadow (`+3, +3` offset in `#404040`)
    fill_rect(&mut surface, page_x0 + 3, page_y0 + 3, pw, ph, SHADOW_DARK);
    // Blit rendered page surface
    blit_surface(&mut surface, &page_out.surface, page_x0, page_y0);
    // 1 px `#000000` crisp page border
    stroke_rect(&mut surface, page_x0, page_y0, pw, ph, TEXT_BLACK);

    // 3. 8 solid `6×6` `#000000` Selection Handles + Center `X` marker
    if let Some(sel_bounds) = snap.selection_bounds {
        let sx0 = page_x0 + (sel_bounds.x0.get() * fit_scale).round() as i32;
        let sy0 = page_y0 + (sel_bounds.y0.get() * fit_scale).round() as i32;
        let sx1 = page_x0 + (sel_bounds.x1.get() * fit_scale).round() as i32;
        let sy1 = page_y0 + (sel_bounds.y1.get() * fit_scale).round() as i32;
        let smx = (sx0 + sx1) / 2;
        let smy = (sy0 + sy1) / 2;

        let handles = [(sx0, sy0), (smx, sy0), (sx1, sy0), (sx1, smy), (sx1, sy1), (smx, sy1), (sx0, sy1), (sx0, smy)];
        for (hx, hy) in handles {
            fill_rect(&mut surface, hx - 3, hy - 3, 6, 6, TEXT_BLACK);
        }
        // Center `X` inside 7x7 box
        draw_line(&mut surface, smx - 3, smy - 3, smx + 3, smy + 3, TEXT_BLACK);
        draw_line(&mut surface, smx - 3, smy + 3, smx + 3, smy - 3, TEXT_BLACK);
    }

    // 4. Rulers (20 px top horizontal ruler, 20 px left vertical ruler, 20x20 corner box)
    fill_rect(&mut surface, cx0, cy0, cx1 - cx0, ruler_thick, CHROME_BG);
    fill_rect(&mut surface, cx0, cy0, ruler_thick, cy1 - cy0, CHROME_BG);
    stroke_rect(&mut surface, cx0, cy0, ruler_thick, ruler_thick, BEVEL_DARK);
    draw_line(&mut surface, cx0 + ruler_thick, cy0 + ruler_thick - 1, cx1 - 1, cy0 + ruler_thick - 1, BEVEL_DARK);
    draw_line(&mut surface, cx0 + ruler_thick - 1, cy0 + ruler_thick, cx0 + ruler_thick - 1, cy1 - 1, BEVEL_DARK);

    let step_minor = (9.0 * fit_scale).round().max(4.0) as i32;
    let mut rx = pb_x0;
    let mut tick_idx = 0i32;
    while rx < cx1 {
        let tick_h = if tick_idx % 8 == 0 {
            12
        } else if tick_idx % 4 == 0 {
            8
        } else {
            4
        };
        draw_line(&mut surface, rx, cy0 + ruler_thick - tick_h, rx, cy0 + ruler_thick - 1, TEXT_BLACK);
        if tick_idx % 8 == 0 {
            let label = format!("{}", tick_idx * 9);
            draw_text(&mut surface, rx + 2, cy0 + 2, &label, TEXT_BLACK);
        }
        rx += step_minor;
        tick_idx += 1;
    }

    let mut ry = pb_y0;
    let mut vtick_idx = 0i32;
    while ry < cy1 {
        let tick_w = if vtick_idx % 8 == 0 {
            12
        } else if vtick_idx % 4 == 0 {
            8
        } else {
            4
        };
        draw_line(&mut surface, cx0 + ruler_thick - tick_w, ry, cx0 + ruler_thick - 1, ry, TEXT_BLACK);
        ry += step_minor;
        vtick_idx += 1;
    }

    // 5. Top Chrome: Menu Bar (`0..24`), Standard Toolbar (`24..56`), Property Bar (`56..90`)
    fill_rect(&mut surface, 0, 0, w as i32, 90, CHROME_BG);
    draw_line(&mut surface, 0, 23, w as i32 - 1, 23, BEVEL_DARK);
    draw_line(&mut surface, 0, 55, w as i32 - 1, 55, BEVEL_DARK);
    draw_line(&mut surface, 0, 89, w as i32 - 1, 89, BEVEL_DARK);

    let mut menu_x = 8;
    for item in &snap.menu_bar {
        draw_text(&mut surface, menu_x, 8, item, TEXT_BLACK);
        menu_x += (item.len() as i32) * 6 + 18;
    }

    // Standard Toolbar 14 buttons (`24×24` at `x=4+28*i, y=28`)
    for i in 0..14 {
        let bx = 4 + i * 28;
        let by = 28;
        draw_bevel_box(&mut surface, bx, by, 24, 24, CHROME_BG);
        stroke_rect(&mut surface, bx + 5, by + 5, 14, 14, TEXT_BLACK);
    }

    // Context-Sensitive Property Bar (`y=56..90`)
    let mut prop_x = 8;
    let ctx_title = format!("{:?}", snap.property_bar.context);
    draw_bevel_box(&mut surface, prop_x, 62, (ctx_title.len() as i32) * 6 + 12, 22, PAPER_WHITE);
    draw_text(&mut surface, prop_x + 6, 69, &ctx_title, TEXT_BLACK);
    prop_x += (ctx_title.len() as i32) * 6 + 20;
    for ctrl in &snap.property_bar.controls {
        let text = format!("{ctrl:?}");
        let short: String = text.chars().take(24).collect();
        let box_w = (short.len() as i32) * 6 + 10;
        if prop_x + box_w >= w as i32 - 8 {
            break;
        }
        draw_bevel_box(&mut surface, prop_x, 62, box_w, 22, PAPER_WHITE);
        draw_text(&mut surface, prop_x + 4, 69, &short, TEXT_BLACK);
        prop_x += box_w + 6;
    }

    // 6. Left Vertical Toolbox (`x=0..38`, 10 buttons `30×30` at `x=4, y=94+32*i`)
    let lt = snap.layout.left_toolbox;
    let ltx0 = lt.x0.get().round() as i32;
    let lty0 = lt.y0.get().round() as i32;
    let ltw = lt.width().round() as i32;
    let lth = lt.height().round() as i32;
    fill_rect(&mut surface, ltx0, lty0, ltw, lth, CHROME_BG);
    draw_line(&mut surface, ltx0 + ltw - 1, lty0, ltx0 + ltw - 1, lty0 + lth - 1, BEVEL_DARK);

    let tools = [
        (ToolKind::Pick, "P"),
        (ToolKind::Shape, "S"),
        (ToolKind::Crop, "C"),
        (ToolKind::Zoom, "Z"),
        (ToolKind::FreehandBezier, "F5"),
        (ToolKind::Rectangle, "F6"),
        (ToolKind::Ellipse, "F7"),
        (ToolKind::PolygonStar, "Y"),
        (ToolKind::ArtisticText, "F8"),
        (ToolKind::InteractiveFill, "G"),
    ];
    for (idx, (tk, glyph)) in tools.iter().enumerate() {
        let bx = ltx0 + 4;
        let by = lty0 + 4 + (idx as i32) * 32;
        if *tk == snap.active_tool {
            fill_rect(&mut surface, bx, by, 30, 30, ACTIVE_TOOL_BG);
            stroke_rect(&mut surface, bx, by, 30, 30, TEXT_BLACK);
        } else {
            draw_bevel_box(&mut surface, bx, by, 30, 30, CHROME_BG);
        }
        draw_text(&mut surface, bx + 6, by + 11, glyph, TEXT_BLACK);
    }

    // 7. Right Vertical Color Palette Bar (`24 px` wide, cell 0 = `[X]` No-Color with red slash)
    let rp = snap.layout.right_palette_bar;
    let rpx0 = rp.x0.get().round() as i32;
    let rpy0 = rp.y0.get().round() as i32;
    let rpw = rp.width().round() as i32;
    let rph = rp.height().round() as i32;
    fill_rect(&mut surface, rpx0, rpy0, rpw, rph, CHROME_BG);
    draw_line(&mut surface, rpx0, rpy0, rpx0, rpy0 + rph - 1, BEVEL_DARK);

    for (idx, cell) in snap.palette_cells.iter().enumerate() {
        let cy = rpy0 + 2 + (idx as i32) * 22;
        if cy + 22 > rpy0 + rph {
            break;
        }
        let cx = rpx0 + 1;
        match cell {
            PaletteCell::NoColor => {
                fill_rect(&mut surface, cx, cy, 22, 22, PAPER_WHITE);
                stroke_rect(&mut surface, cx, cy, 22, 22, TEXT_BLACK);
                draw_line(&mut surface, cx + 2, cy + 19, cx + 19, cy + 2, SLASH_RED);
                draw_line(&mut surface, cx + 2, cy + 18, cx + 18, cy + 2, SLASH_RED);
            }
            PaletteCell::Color { color, .. } => {
                let rgba = color.to_rgba8(1.0);
                fill_rect(&mut surface, cx, cy, 22, 22, rgba);
                stroke_rect(&mut surface, cx, cy, 22, 22, BEVEL_DARK);
            }
        }
    }

    // 8. Right Collapsible Docker (`Objects` / `Properties`)
    if let Some(dk) = snap.layout.right_docker {
        let dx0 = dk.x0.get().round() as i32;
        let dy0 = dk.y0.get().round() as i32;
        let dw = dk.width().round() as i32;
        let dh = dk.height().round() as i32;
        fill_rect(&mut surface, dx0, dy0, dw, dh, PAPER_WHITE);
        fill_rect(&mut surface, dx0, dy0, dw, 24, CHROME_BG);
        draw_line(&mut surface, dx0, dy0, dx0, dy0 + dh - 1, BEVEL_DARK);
        draw_line(&mut surface, dx0, dy0 + 23, dx0 + dw - 1, dy0 + 23, BEVEL_DARK);

        let obj_tab_bg = if snap.active_docker_tab == DockerTab::Objects { PAPER_WHITE } else { CHROME_BG };
        let prop_tab_bg = if snap.active_docker_tab == DockerTab::Properties { PAPER_WHITE } else { CHROME_BG };
        fill_rect(&mut surface, dx0 + 4, dy0 + 2, 80, 21, obj_tab_bg);
        stroke_rect(&mut surface, dx0 + 4, dy0 + 2, 80, 21, BEVEL_DARK);
        draw_text(&mut surface, dx0 + 14, dy0 + 8, "Objects", TEXT_BLACK);

        fill_rect(&mut surface, dx0 + 88, dy0 + 2, 88, 21, prop_tab_bg);
        stroke_rect(&mut surface, dx0 + 88, dy0 + 2, 88, 21, BEVEL_DARK);
        draw_text(&mut surface, dx0 + 96, dy0 + 8, "Properties", TEXT_BLACK);

        for (row_idx, entry) in snap.object_tree.iter().enumerate() {
            let ry = dy0 + 28 + (row_idx as i32) * 20;
            if ry + 20 > dy0 + dh {
                break;
            }
            let (row_bg, fg) = if entry.selected { (DOCKER_SEL_BG, PAPER_WHITE) } else { (PAPER_WHITE, TEXT_BLACK) };
            fill_rect(&mut surface, dx0 + 2, ry, dw - 4, 18, row_bg);
            let label = format!("{} - {}", entry.layer_name, entry.object_label);
            draw_text(&mut surface, dx0 + 12, ry + 5, &label, fg);
        }
    }

    // 9. Bottom Page Navigator (`+`, `Page 1`, ..., `Master Page`, `Desktop`)
    let pn = snap.layout.page_navigator_bar;
    let pnx0 = pn.x0.get().round() as i32;
    let pny0 = pn.y0.get().round() as i32;
    let pnw = pn.width().round() as i32;
    let pnh = pn.height().round() as i32;
    fill_rect(&mut surface, pnx0, pny0, pnw, pnh, CHROME_BG);
    draw_line(&mut surface, pnx0, pny0, pnx0 + pnw - 1, pny0, BEVEL_DARK);

    // `+` button at `x=4`
    draw_bevel_box(&mut surface, pnx0 + 4, pny0 + 3, 20, 20, CHROME_BG);
    draw_text(&mut surface, pnx0 + 11, pny0 + 9, "+", TEXT_BLACK);

    let mut tab_x = pnx0 + 28;
    for tab in &snap.page_tabs {
        let tab_w = ((tab.label.len() as i32) * 6 + 24).max(76);
        let bg = if tab.active { PAPER_WHITE } else { CHROME_BG };
        fill_rect(&mut surface, tab_x, pny0 + 3, tab_w, 20, bg);
        stroke_rect(&mut surface, tab_x, pny0 + 3, tab_w, 20, BEVEL_DARK);
        if tab.active {
            fill_rect(&mut surface, tab_x + 1, pny0 + 4, tab_w - 2, 2, ACTIVE_TOOL_BG);
        }
        draw_text(&mut surface, tab_x + 8, pny0 + 9, &tab.label, TEXT_BLACK);
        tab_x += tab_w + 4;
    }

    // 10. Status Bar (`(x, y)` coordinates, Selection Summary, Fill/Outline swatch chips)
    let sb = snap.layout.status_bar;
    let sbx0 = sb.x0.get().round() as i32;
    let sby0 = sb.y0.get().round() as i32;
    let sbw = sb.width().round() as i32;
    let sbh = sb.height().round() as i32;
    fill_rect(&mut surface, sbx0, sby0, sbw, sbh, CHROME_BG);
    draw_line(&mut surface, sbx0, sby0, sbx0 + sbw - 1, sby0, BEVEL_DARK);

    let coord_str = format!("({:.1}, {:.1})", snap.status_bar.cursor_pt.x.get(), snap.status_bar.cursor_pt.y.get());
    draw_text(&mut surface, sbx0 + 8, sby0 + 8, &coord_str, TEXT_BLACK);
    draw_text(&mut surface, sbx0 + 140, sby0 + 8, &snap.status_bar.selection_summary, TEXT_BLACK);

    // Right-aligned Fill & Outline chips (`16×12`)
    let fill_chip_x = sbx0 + sbw - 140;
    let outline_chip_x = sbx0 + sbw - 68;
    draw_text(&mut surface, fill_chip_x - 30, sby0 + 8, "Fill:", TEXT_BLACK);
    draw_status_chip(&mut surface, fill_chip_x, sby0 + 6, snap.status_bar.fill_chip.as_ref());
    draw_text(&mut surface, outline_chip_x - 30, sby0 + 8, "Out:", TEXT_BLACK);
    draw_status_chip(&mut surface, outline_chip_x, sby0 + 6, snap.status_bar.outline_chip.as_ref());

    let png_bytes = encode_png(&surface)?;
    Ok(RenderOutput { surface, png_bytes, warnings })
}

fn draw_status_chip(surface: &mut Surface, x: i32, y: i32, color: Option<&Color>) {
    if let Some(c) = color {
        fill_rect(surface, x, y, 16, 12, c.to_rgba8(1.0));
        stroke_rect(surface, x, y, 16, 12, TEXT_BLACK);
    } else {
        fill_rect(surface, x, y, 16, 12, PAPER_WHITE);
        stroke_rect(surface, x, y, 16, 12, TEXT_BLACK);
        draw_line(surface, x + 1, y + 10, x + 14, y + 1, SLASH_RED);
    }
}

fn draw_bevel_box(surface: &mut Surface, x: i32, y: i32, w: i32, h: i32, bg: [u8; 4]) {
    fill_rect(surface, x, y, w, h, bg);
    draw_line(surface, x, y, x + w - 1, y, BEVEL_LIGHT);
    draw_line(surface, x, y, x, y + h - 1, BEVEL_LIGHT);
    draw_line(surface, x, y + h - 1, x + w - 1, y + h - 1, BEVEL_DARK);
    draw_line(surface, x + w - 1, y, x + w - 1, y + h - 1, BEVEL_DARK);
}

fn put_pixel(surface: &mut Surface, x: i32, y: i32, rgba: [u8; 4]) {
    if x >= 0 && y >= 0 && (x as u32) < surface.width && (y as u32) < surface.height {
        let idx = (y as usize) * (surface.width as usize) + (x as usize);
        surface.pixels[idx] = straight_to_premul(rgba);
    }
}

fn fill_rect(surface: &mut Surface, x: i32, y: i32, w: i32, h: i32, rgba: [u8; 4]) {
    let x0 = x.max(0);
    let y0 = y.max(0);
    let x1 = (x + w).min(surface.width as i32);
    let y1 = (y + h).min(surface.height as i32);
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let premul = straight_to_premul(rgba);
    let stride = surface.width as usize;
    for py in y0..y1 {
        let row = (py as usize) * stride;
        for px in x0..x1 {
            surface.pixels[row + (px as usize)] = premul;
        }
    }
}

fn stroke_rect(surface: &mut Surface, x: i32, y: i32, w: i32, h: i32, rgba: [u8; 4]) {
    if w <= 0 || h <= 0 {
        return;
    }
    draw_line(surface, x, y, x + w - 1, y, rgba);
    draw_line(surface, x, y + h - 1, x + w - 1, y + h - 1, rgba);
    draw_line(surface, x, y, x, y + h - 1, rgba);
    draw_line(surface, x + w - 1, y, x + w - 1, y + h - 1, rgba);
}

fn draw_line(surface: &mut Surface, x0: i32, y0: i32, x1: i32, y1: i32, rgba: [u8; 4]) {
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    let mut x = x0;
    let mut y = y0;
    loop {
        put_pixel(surface, x, y, rgba);
        if x == x1 && y == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

fn blit_surface(dst: &mut Surface, src: &Surface, dst_x0: i32, dst_y0: i32) {
    let sw = src.width as i32;
    let sh = src.height as i32;
    for sy in 0..sh {
        let dy = dst_y0 + sy;
        if dy < 0 || (dy as u32) >= dst.height {
            continue;
        }
        for sx in 0..sw {
            let dx = dst_x0 + sx;
            if dx < 0 || (dx as u32) >= dst.width {
                continue;
            }
            let src_px = src.pixels[(sy as usize) * (src.width as usize) + (sx as usize)];
            let straight = premul_to_straight(src_px);
            put_pixel(dst, dx, dy, straight);
        }
    }
}

fn draw_text(surface: &mut Surface, x: i32, y: i32, text: &str, rgba: [u8; 4]) {
    let mut cx = x;
    for ch in text.chars() {
        let rows = glyph_5x7(ch);
        for (ry, row_bits) in rows.iter().enumerate() {
            for rx in 0..5 {
                if (row_bits >> (4 - rx)) & 1 == 1 {
                    put_pixel(surface, cx + rx, y + (ry as i32), rgba);
                }
            }
        }
        cx += 6;
    }
}

const fn glyph_5x7(ch: char) -> [u8; 7] {
    match ch.to_ascii_uppercase() {
        'A' => [0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'B' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110],
        'C' => [0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110],
        'D' => [0b11100, 0b10010, 0b10001, 0b10001, 0b10001, 0b10010, 0b11100],
        'E' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111],
        'F' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000],
        'G' => [0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01110],
        'H' => [0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'I' => [0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        'J' => [0b00111, 0b00010, 0b00010, 0b00010, 0b10010, 0b10010, 0b01100],
        'K' => [0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001],
        'L' => [0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111],
        'M' => [0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001],
        'N' => [0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001],
        'O' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'P' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000],
        'Q' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101],
        'R' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001],
        'S' => [0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110],
        'T' => [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100],
        'U' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'V' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100],
        'W' => [0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b11011, 0b10001],
        'X' => [0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001],
        'Y' => [0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100],
        'Z' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111],
        '0' => [0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110],
        '1' => [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        '2' => [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111],
        '3' => [0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110],
        '4' => [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010],
        '5' => [0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110],
        '6' => [0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110],
        '7' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000],
        '8' => [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110],
        '9' => [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100],
        '+' => [0b00000, 0b00100, 0b00100, 0b11111, 0b00100, 0b00100, 0b00000],
        '-' => [0b00000, 0b00000, 0b00000, 0b11111, 0b00000, 0b00000, 0b00000],
        ':' => [0b00000, 0b00100, 0b00100, 0b00000, 0b00100, 0b00100, 0b00000],
        '.' => [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b01100, 0b01100],
        ',' => [0b00000, 0b00000, 0b00000, 0b00000, 0b00100, 0b00100, 0b01000],
        '(' => [0b00010, 0b00100, 0b01000, 0b01000, 0b01000, 0b00100, 0b00010],
        ')' => [0b01000, 0b00100, 0b00010, 0b00010, 0b00010, 0b00100, 0b01000],
        ' ' => [0; 7],
        _ => [0b11111, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11111],
    }
}
