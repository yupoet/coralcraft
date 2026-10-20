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

use crate::{DesktopWorkspace, DockerTab, PaletteCell, StandardToolbarControl};

const CHROME_BG: [u8; 4] = [0xD4, 0xD0, 0xC8, 0xFF];
const BEVEL_LIGHT: [u8; 4] = [0xFF, 0xFF, 0xFF, 0xFF];
const BEVEL_DARK: [u8; 4] = [0x80, 0x80, 0x80, 0xFF];
const SHADOW_DARK: [u8; 4] = [0x40, 0x40, 0x40, 0xFF];
const TEXT_BLACK: [u8; 4] = [0x00, 0x00, 0x00, 0xFF];
const ACTIVE_TOOL_BG: [u8; 4] = [0xCC, 0xE4, 0xF7, 0xFF];
const DOCKER_SEL_BG: [u8; 4] = [0x31, 0x6A, 0xC5, 0xFF];
const SLASH_RED: [u8; 4] = [0xFF, 0x00, 0x00, 0xFF];
const PAPER_WHITE: [u8; 4] = [0xFF, 0xFF, 0xFF, 0xFF];
const GUIDELINE_BLUE: [u8; 4] = [0x00, 0x55, 0xE5, 0xFF];
const FOLDER_YELLOW: [u8; 4] = [0xD8, 0xB0, 0x40, 0xFF];
const FLOPPY_BLUE: [u8; 4] = [0x28, 0x48, 0x88, 0xFF];
const ICON_GREEN: [u8; 4] = [0x20, 0x88, 0x30, 0xFF];
const STAR_GOLD: [u8; 4] = [0xF8, 0xC8, 0x20, 0xFF];

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

    // 2b. Dashed #0055E5 Corel Blue Ruler Guidelines across pasteboard & page
    for gy_pt in &snap.horizontal_guidelines {
        let gy = page_y0 + (gy_pt.get() * fit_scale).round() as i32;
        if gy >= pb_y0 && gy < cy1 {
            draw_dashed_hline(&mut surface, pb_x0, cx1 - 1, gy, 4, GUIDELINE_BLUE);
            fill_rect(&mut surface, cx0 + ruler_thick - 5, gy - 1, 5, 3, GUIDELINE_BLUE);
        }
    }
    for gx_pt in &snap.vertical_guidelines {
        let gx = page_x0 + (gx_pt.get() * fit_scale).round() as i32;
        if gx >= pb_x0 && gx < cx1 {
            draw_dashed_vline(&mut surface, gx, pb_y0, cy1 - 1, 4, GUIDELINE_BLUE);
            fill_rect(&mut surface, gx - 1, cy0 + ruler_thick - 5, 3, 5, GUIDELINE_BLUE);
        }
    }

    // 2c. Paragraph Text dashed frame outlines and red bottom-center overflow indicator handle
    if let Ok(eff_layers) = ws.engine.document.effective_layers_for_page(ws.engine.current_page_index(), false) {
        for layer in eff_layers {
            for obj in &layer.objects {
                if !obj.visible {
                    continue;
                }
                if let coralcraft_doc::ObjectKind::ParagraphText(pt) = &obj.kind {
                    let fx0 = page_x0 + ((pt.position.x.get() + obj.transform.tx.get()) * fit_scale).round() as i32;
                    let fy0 = page_y0 + ((pt.position.y.get() + obj.transform.ty.get()) * fit_scale).round() as i32;
                    let fx1 = page_x0 + ((pt.position.x.get() + pt.frame_width_pt.get() + obj.transform.tx.get()) * fit_scale).round() as i32;
                    let fy1 = page_y0 + ((pt.position.y.get() + pt.frame_height_pt.get() + obj.transform.ty.get()) * fit_scale).round() as i32;
                    draw_dashed_hline(&mut surface, fx0, fx1, fy0, 3, GUIDELINE_BLUE);
                    draw_dashed_hline(&mut surface, fx0, fx1, fy1, 3, GUIDELINE_BLUE);
                    draw_dashed_vline(&mut surface, fx0, fy0, fy1, 3, GUIDELINE_BLUE);
                    draw_dashed_vline(&mut surface, fx1, fy0, fy1, 3, GUIDELINE_BLUE);
                    let layout = coralcraft_doc::layout_paragraph_text(pt);
                    if layout.overflow {
                        let fmx = (fx0 + fx1) / 2;
                        fill_rect(&mut surface, fmx - 4, fy1 - 3, 9, 7, SLASH_RED);
                        stroke_rect(&mut surface, fmx - 4, fy1 - 3, 9, 7, PAPER_WHITE);
                    }
                }
            }
        }
    }

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

        // 3b. Interactive Fill (`G`) on-canvas vector handles (Start Swatch, Axis, Midpoint Slider, End Swatch)
        if snap.active_tool == ToolKind::InteractiveFill {
            let (angle_deg, start_rgba, end_rgba) = ws.engine.current_target_objects().iter().find(|o| ws.engine.selection.contains(&o.id)).map_or(
                (0.0, [0, 120, 215, 255], PAPER_WHITE),
                |o| match &o.fill {
                    Some(coralcraft_doc::FillStyle::Fountain { geometry, stops, .. }) => {
                        let ang = match geometry {
                            coralcraft_doc::FountainGeometry::Linear { angle_deg, .. }
                            | coralcraft_doc::FountainGeometry::Conical { angle_deg, .. }
                            | coralcraft_doc::FountainGeometry::Square { angle_deg, .. } => angle_deg.get(),
                            coralcraft_doc::FountainGeometry::Radial { .. } => 0.0,
                        };
                        let c0 = stops.first().map(|s| s.color.to_rgba8(1.0)).unwrap_or([0, 120, 215, 255]);
                        let c1 = stops.last().map(|s| s.color.to_rgba8(1.0)).unwrap_or(PAPER_WHITE);
                        (ang, c0, c1)
                    }
                    Some(coralcraft_doc::FillStyle::Uniform { color }) => (0.0, color.to_rgba8(1.0), PAPER_WHITE),
                    _ => (0.0, [0, 120, 215, 255], PAPER_WHITE),
                },
            );
            let half_len = ((sx1 - sx0).abs().max(24) as f64) * 0.4;
            let rad = angle_deg.to_radians();
            let dx = (half_len * rad.cos()).round() as i32;
            let dy = (half_len * rad.sin()).round() as i32;
            let vx0 = smx - dx;
            let vy0 = smy - dy;
            let vx1 = smx + dx;
            let vy1 = smy + dy;
            draw_line(&mut surface, vx0, vy0, vx1, vy1, GUIDELINE_BLUE);
            fill_rect(&mut surface, smx - 3, smy - 3, 6, 6, PAPER_WHITE);
            stroke_rect(&mut surface, smx - 3, smy - 3, 6, 6, GUIDELINE_BLUE);
            fill_rect(&mut surface, vx0 - 4, vy0 - 4, 8, 8, start_rgba);
            stroke_rect(&mut surface, vx0 - 4, vy0 - 4, 8, 8, TEXT_BLACK);
            fill_rect(&mut surface, vx1 - 4, vy1 - 4, 8, 8, end_rgba);
            stroke_rect(&mut surface, vx1 - 4, vy1 - 4, 8, 8, TEXT_BLACK);
        }
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
    for (i, ctrl) in StandardToolbarControl::ALL.iter().enumerate() {
        let bx = 4 + (i as i32) * 28;
        let by = 28;
        draw_bevel_box(&mut surface, bx, by, 24, 24, CHROME_BG);
        draw_toolbar_icon(&mut surface, *ctrl, bx, by);
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
        ToolKind::Pick,
        ToolKind::Shape,
        ToolKind::Crop,
        ToolKind::Zoom,
        ToolKind::FreehandBezier,
        ToolKind::Rectangle,
        ToolKind::Ellipse,
        ToolKind::PolygonStar,
        ToolKind::ArtisticText,
        ToolKind::InteractiveFill,
    ];
    for (idx, tk) in tools.iter().enumerate() {
        let bx = ltx0 + 4;
        let by = lty0 + 4 + (idx as i32) * 32;
        if *tk == snap.active_tool {
            fill_rect(&mut surface, bx, by, 30, 30, ACTIVE_TOOL_BG);
            stroke_rect(&mut surface, bx, by, 30, 30, TEXT_BLACK);
        } else {
            draw_bevel_box(&mut surface, bx, by, 30, 30, CHROME_BG);
        }
        draw_toolbox_glyph(&mut surface, *tk, bx, by, *tk == snap.active_tool);
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

            // Badge pill: [Grp], [Pwr], [Eff], [Rec], [Elp], etc.
            let badge_bg = match entry.type_badge.as_str() {
                "[Grp]" => [0x90, 0x60, 0x10, 0xFF],
                "[Pwr]" => [0x88, 0x20, 0x88, 0xFF],
                "[Eff]" => [0x20, 0x70, 0x30, 0xFF],
                "[Cur]" => [0x10, 0x60, 0x70, 0xFF],
                "[Txt]" => [0x20, 0x30, 0x80, 0xFF],
                "[Bmp]" => [0x10, 0x80, 0x80, 0xFF],
                _ => [0x40, 0x50, 0x70, 0xFF],
            };
            fill_rect(&mut surface, dx0 + 8, ry + 2, 28, 14, badge_bg);
            draw_text(&mut surface, dx0 + 10, ry + 5, &entry.type_badge, PAPER_WHITE);

            let prefix = if entry.is_master {
                "[M] "
            } else if entry.is_desktop {
                "[D] "
            } else {
                ""
            };
            let label = format!("{}{}: {}", prefix, entry.layer_name, entry.object_label);
            draw_text(&mut surface, dx0 + 40, ry + 5, &label, fg);

            let eye_x = dx0 + dw - 24;
            if entry.visible {
                draw_text(&mut surface, eye_x, ry + 5, "[o]", fg);
            } else {
                draw_text(&mut surface, eye_x, ry + 5, "[-]", fg);
            }
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

fn draw_dashed_hline(surface: &mut Surface, x0: i32, x1: i32, y: i32, dash_len: i32, rgba: [u8; 4]) {
    let mut x = x0;
    while x <= x1 {
        let seg_end = (x + dash_len).min(x1 + 1);
        for px in x..seg_end {
            put_pixel(surface, px, y, rgba);
        }
        x += dash_len * 2;
    }
}

fn draw_dashed_vline(surface: &mut Surface, x: i32, y0: i32, y1: i32, dash_len: i32, rgba: [u8; 4]) {
    let mut y = y0;
    while y <= y1 {
        let seg_end = (y + dash_len).min(y1 + 1);
        for py in y..seg_end {
            put_pixel(surface, x, py, rgba);
        }
        y += dash_len * 2;
    }
}

fn draw_toolbox_glyph(surface: &mut Surface, tool: ToolKind, bx: i32, by: i32, _active: bool) {
    // 1. Draw CorelDRAW tool-flyout black triangle at bottom-right corner
    for dy in 0..4 {
        for dx in (3 - dy)..4 {
            put_pixel(surface, bx + 24 + dx, by + 24 + dy, TEXT_BLACK);
        }
    }

    match tool {
        ToolKind::Pick => {
            // Classic NW pointer cursor arrow
            let pts = [
                (bx + 9, by + 7),
                (bx + 9, by + 21),
                (bx + 13, by + 17),
                (bx + 17, by + 22),
                (bx + 19, by + 20),
                (bx + 15, by + 15),
                (bx + 19, by + 15),
            ];
            for i in 0..pts.len() {
                let next = (i + 1) % pts.len();
                draw_line(surface, pts[i].0, pts[i].1, pts[next].0, pts[next].1, TEXT_BLACK);
            }
            fill_rect(surface, bx + 10, by + 10, 3, 5, PAPER_WHITE);
            fill_rect(surface, bx + 11, by + 15, 2, 2, PAPER_WHITE);
        }
        ToolKind::Shape => {
            // Node-editing curved path with handles
            draw_line(surface, bx + 8, by + 21, bx + 13, by + 12, TEXT_BLACK);
            draw_line(surface, bx + 13, by + 12, bx + 21, by + 13, TEXT_BLACK);
            // End node 1
            fill_rect(surface, bx + 6, by + 19, 4, 4, PAPER_WHITE);
            stroke_rect(surface, bx + 6, by + 19, 4, 4, TEXT_BLACK);
            // End node 2
            fill_rect(surface, bx + 19, by + 11, 4, 4, GUIDELINE_BLUE);
            stroke_rect(surface, bx + 19, by + 11, 4, 4, TEXT_BLACK);
            // Pointer touching node
            draw_line(surface, bx + 14, by + 7, bx + 14, by + 14, TEXT_BLACK);
            draw_line(surface, bx + 14, by + 14, bx + 18, by + 10, TEXT_BLACK);
        }
        ToolKind::Crop => {
            // Overlapping L-brackets
            draw_line(surface, bx + 7, by + 9, bx + 16, by + 9, TEXT_BLACK);
            draw_line(surface, bx + 9, by + 7, bx + 9, by + 16, TEXT_BLACK);
            draw_line(surface, bx + 12, by + 20, bx + 21, by + 20, TEXT_BLACK);
            draw_line(surface, bx + 19, by + 12, bx + 19, by + 21, TEXT_BLACK);
            stroke_rect(surface, bx + 11, by + 11, 7, 7, BEVEL_DARK);
        }
        ToolKind::Zoom => {
            // Magnifying glass with + inside
            stroke_rect(surface, bx + 8, by + 8, 10, 10, TEXT_BLACK);
            draw_line(surface, bx + 16, by + 16, bx + 22, by + 22, TEXT_BLACK);
            draw_line(surface, bx + 17, by + 16, bx + 22, by + 21, TEXT_BLACK);
            // Plus
            draw_line(surface, bx + 10, by + 13, bx + 15, by + 13, TEXT_BLACK);
            draw_line(surface, bx + 13, by + 10, bx + 13, by + 15, TEXT_BLACK);
        }
        ToolKind::FreehandBezier => {
            // Smooth freehand S-curve with node markers
            draw_line(surface, bx + 8, by + 20, bx + 11, by + 13, TEXT_BLACK);
            draw_line(surface, bx + 11, by + 13, bx + 16, by + 16, TEXT_BLACK);
            draw_line(surface, bx + 16, by + 16, bx + 21, by + 8, TEXT_BLACK);
            fill_rect(surface, bx + 7, by + 19, 3, 3, TEXT_BLACK);
            fill_rect(surface, bx + 20, by + 7, 3, 3, TEXT_BLACK);
        }
        ToolKind::Rectangle => {
            // Rectangle outline with 4 solid corner handles
            stroke_rect(surface, bx + 8, by + 10, 14, 10, TEXT_BLACK);
            fill_rect(surface, bx + 7, by + 9, 3, 3, TEXT_BLACK);
            fill_rect(surface, bx + 20, by + 9, 3, 3, TEXT_BLACK);
            fill_rect(surface, bx + 7, by + 18, 3, 3, TEXT_BLACK);
            fill_rect(surface, bx + 20, by + 18, 3, 3, TEXT_BLACK);
        }
        ToolKind::Ellipse => {
            // Ellipse outline with 4 cardinal node handles
            stroke_rect(surface, bx + 8, by + 10, 14, 10, TEXT_BLACK);
            put_pixel(surface, bx + 8, by + 10, CHROME_BG);
            put_pixel(surface, bx + 21, by + 10, CHROME_BG);
            put_pixel(surface, bx + 8, by + 19, CHROME_BG);
            put_pixel(surface, bx + 21, by + 19, CHROME_BG);
            // 4 cardinal handles
            fill_rect(surface, bx + 14, by + 9, 3, 3, TEXT_BLACK);
            fill_rect(surface, bx + 14, by + 18, 3, 3, TEXT_BLACK);
            fill_rect(surface, bx + 7, by + 14, 3, 3, TEXT_BLACK);
            fill_rect(surface, bx + 20, by + 14, 3, 3, TEXT_BLACK);
        }
        ToolKind::PolygonStar => {
            // 5-point star filled with gold
            let pts = [(bx + 15, by + 7), (bx + 22, by + 12), (bx + 19, by + 21), (bx + 11, by + 21), (bx + 8, by + 12)];
            draw_line(surface, pts[0].0, pts[0].1, pts[2].0, pts[2].1, TEXT_BLACK);
            draw_line(surface, pts[2].0, pts[2].1, pts[4].0, pts[4].1, TEXT_BLACK);
            draw_line(surface, pts[4].0, pts[4].1, pts[1].0, pts[1].1, TEXT_BLACK);
            draw_line(surface, pts[1].0, pts[1].1, pts[3].0, pts[3].1, TEXT_BLACK);
            draw_line(surface, pts[3].0, pts[3].1, pts[0].0, pts[0].1, TEXT_BLACK);
            fill_rect(surface, bx + 13, by + 13, 4, 4, STAR_GOLD);
        }
        ToolKind::ArtisticText => {
            // Bold serif 'A' with baseline
            draw_line(surface, bx + 14, by + 8, bx + 9, by + 20, TEXT_BLACK);
            draw_line(surface, bx + 15, by + 8, bx + 10, by + 20, TEXT_BLACK);
            draw_line(surface, bx + 15, by + 8, bx + 20, by + 20, TEXT_BLACK);
            draw_line(surface, bx + 12, by + 16, bx + 18, by + 16, TEXT_BLACK);
            // Serifs
            draw_line(surface, bx + 8, by + 20, bx + 12, by + 20, TEXT_BLACK);
            draw_line(surface, bx + 18, by + 20, bx + 22, by + 20, TEXT_BLACK);
        }
        ToolKind::InteractiveFill => {
            // Tilted paint bucket spilling paint drop
            fill_rect(surface, bx + 9, by + 11, 7, 7, [0x30, 0x90, 0xD0, 0xFF]);
            stroke_rect(surface, bx + 9, by + 11, 7, 7, TEXT_BLACK);
            draw_line(surface, bx + 15, by + 15, bx + 20, by + 20, [0x30, 0x90, 0xD0, 0xFF]);
            fill_rect(surface, bx + 19, by + 19, 3, 3, [0x30, 0x90, 0xD0, 0xFF]);
        }
    }
}

fn draw_toolbar_icon(surface: &mut Surface, ctrl: StandardToolbarControl, bx: i32, by: i32) {
    match ctrl {
        StandardToolbarControl::New => {
            fill_rect(surface, bx + 6, by + 4, 11, 15, PAPER_WHITE);
            stroke_rect(surface, bx + 6, by + 4, 11, 15, TEXT_BLACK);
            // Dog ear
            put_pixel(surface, bx + 15, by + 4, CHROME_BG);
            put_pixel(surface, bx + 16, by + 4, CHROME_BG);
            put_pixel(surface, bx + 16, by + 5, CHROME_BG);
            draw_line(surface, bx + 14, by + 4, bx + 16, by + 6, TEXT_BLACK);
            // Text lines
            draw_line(surface, bx + 8, by + 9, bx + 14, by + 9, BEVEL_DARK);
            draw_line(surface, bx + 8, by + 12, bx + 14, by + 12, BEVEL_DARK);
            draw_line(surface, bx + 8, by + 15, bx + 12, by + 15, BEVEL_DARK);
        }
        StandardToolbarControl::Open => {
            // Yellow folder
            fill_rect(surface, bx + 5, by + 6, 6, 3, FOLDER_YELLOW);
            stroke_rect(surface, bx + 5, by + 6, 6, 3, TEXT_BLACK);
            fill_rect(surface, bx + 4, by + 8, 15, 11, FOLDER_YELLOW);
            stroke_rect(surface, bx + 4, by + 8, 15, 11, TEXT_BLACK);
            draw_line(surface, bx + 4, by + 13, bx + 18, by + 13, [0xC0, 0x98, 0x20, 0xFF]);
        }
        StandardToolbarControl::Save => {
            // Blue 3.5" floppy disk
            fill_rect(surface, bx + 5, by + 5, 14, 14, FLOPPY_BLUE);
            stroke_rect(surface, bx + 5, by + 5, 14, 14, TEXT_BLACK);
            fill_rect(surface, bx + 8, by + 5, 7, 5, CHROME_BG);
            stroke_rect(surface, bx + 8, by + 5, 7, 5, TEXT_BLACK);
            fill_rect(surface, bx + 7, by + 12, 10, 6, PAPER_WHITE);
        }
        StandardToolbarControl::Print => {
            // Desktop printer
            fill_rect(surface, bx + 5, by + 10, 14, 8, CHROME_BG);
            stroke_rect(surface, bx + 5, by + 10, 14, 8, TEXT_BLACK);
            fill_rect(surface, bx + 8, by + 6, 8, 5, PAPER_WHITE);
            stroke_rect(surface, bx + 8, by + 6, 8, 5, TEXT_BLACK);
            fill_rect(surface, bx + 8, by + 14, 8, 4, PAPER_WHITE);
            stroke_rect(surface, bx + 8, by + 14, 8, 4, TEXT_BLACK);
            put_pixel(surface, bx + 16, by + 12, ICON_GREEN);
        }
        StandardToolbarControl::Cut => {
            // Scissors
            draw_line(surface, bx + 6, by + 6, bx + 16, by + 15, TEXT_BLACK);
            draw_line(surface, bx + 16, by + 6, bx + 6, by + 15, TEXT_BLACK);
            stroke_rect(surface, bx + 5, by + 14, 4, 4, TEXT_BLACK);
            stroke_rect(surface, bx + 14, by + 14, 4, 4, TEXT_BLACK);
        }
        StandardToolbarControl::Copy => {
            // Two overlapping sheets
            fill_rect(surface, bx + 9, by + 5, 9, 12, PAPER_WHITE);
            stroke_rect(surface, bx + 9, by + 5, 9, 12, TEXT_BLACK);
            fill_rect(surface, bx + 6, by + 8, 9, 12, PAPER_WHITE);
            stroke_rect(surface, bx + 6, by + 8, 9, 12, TEXT_BLACK);
        }
        StandardToolbarControl::Paste => {
            // Clipboard
            fill_rect(surface, bx + 5, by + 6, 13, 13, [0xA0, 0x70, 0x30, 0xFF]);
            stroke_rect(surface, bx + 5, by + 6, 13, 13, TEXT_BLACK);
            fill_rect(surface, bx + 8, by + 4, 7, 3, CHROME_BG);
            stroke_rect(surface, bx + 8, by + 4, 7, 3, TEXT_BLACK);
            fill_rect(surface, bx + 7, by + 8, 9, 10, PAPER_WHITE);
            stroke_rect(surface, bx + 7, by + 8, 9, 10, TEXT_BLACK);
        }
        StandardToolbarControl::Undo => {
            // Curved left blue arrow
            draw_line(surface, bx + 16, by + 16, bx + 12, by + 9, GUIDELINE_BLUE);
            draw_line(surface, bx + 12, by + 9, bx + 7, by + 11, GUIDELINE_BLUE);
            draw_line(surface, bx + 7, by + 11, bx + 10, by + 8, GUIDELINE_BLUE);
            draw_line(surface, bx + 7, by + 11, bx + 10, by + 14, GUIDELINE_BLUE);
        }
        StandardToolbarControl::Redo => {
            // Curved right blue arrow
            draw_line(surface, bx + 7, by + 16, bx + 11, by + 9, GUIDELINE_BLUE);
            draw_line(surface, bx + 11, by + 9, bx + 16, by + 11, GUIDELINE_BLUE);
            draw_line(surface, bx + 16, by + 11, bx + 13, by + 8, GUIDELINE_BLUE);
            draw_line(surface, bx + 16, by + 11, bx + 13, by + 14, GUIDELINE_BLUE);
        }
        StandardToolbarControl::Import => {
            // Document with green arrow pointing in
            fill_rect(surface, bx + 5, by + 6, 9, 12, PAPER_WHITE);
            stroke_rect(surface, bx + 5, by + 6, 9, 12, TEXT_BLACK);
            draw_line(surface, bx + 17, by + 6, bx + 11, by + 12, ICON_GREEN);
            draw_line(surface, bx + 11, by + 12, bx + 14, by + 12, ICON_GREEN);
            draw_line(surface, bx + 11, by + 12, bx + 11, by + 9, ICON_GREEN);
        }
        StandardToolbarControl::Export => {
            // Document with blue arrow pointing out
            fill_rect(surface, bx + 5, by + 6, 9, 12, PAPER_WHITE);
            stroke_rect(surface, bx + 5, by + 6, 9, 12, TEXT_BLACK);
            draw_line(surface, bx + 11, by + 12, bx + 17, by + 6, GUIDELINE_BLUE);
            draw_line(surface, bx + 17, by + 6, bx + 14, by + 6, GUIDELINE_BLUE);
            draw_line(surface, bx + 17, by + 6, bx + 17, by + 9, GUIDELINE_BLUE);
        }
        StandardToolbarControl::ZoomPercent => {
            // Magnifier + %
            stroke_rect(surface, bx + 5, by + 8, 8, 8, TEXT_BLACK);
            draw_line(surface, bx + 11, by + 14, bx + 14, by + 17, TEXT_BLACK);
            draw_text(surface, bx + 14, by + 8, "%", TEXT_BLACK);
        }
        StandardToolbarControl::SnapToObjects => {
            // Magnet icon (red & blue poles)
            fill_rect(surface, bx + 6, by + 6, 4, 8, [0xE0, 0x20, 0x20, 0xFF]);
            fill_rect(surface, bx + 13, by + 6, 4, 8, GUIDELINE_BLUE);
            draw_line(surface, bx + 6, by + 14, bx + 17, by + 14, BEVEL_DARK);
            fill_rect(surface, bx + 6, by + 5, 4, 2, CHROME_BG);
            fill_rect(surface, bx + 13, by + 5, 4, 2, CHROME_BG);
        }
        StandardToolbarControl::SnapToPage => {
            // Page with snap crosshairs
            stroke_rect(surface, bx + 7, by + 6, 10, 12, TEXT_BLACK);
            draw_line(surface, bx + 5, by + 12, bx + 19, by + 12, GUIDELINE_BLUE);
            draw_line(surface, bx + 12, by + 4, bx + 12, by + 20, GUIDELINE_BLUE);
        }
    }
}
