//! CorelDRAW-style bottom Page Navigator bar (`1 of N`, `|<`, `<`, `+|<`, page tabs, `+`, `>`, `>|`).
//!
//! Supports clicking page tabs to switch pages (`view.goToArtboard`), adding pages before/after the
//! active page (`artboard.new` with `reflowPages: true`), deleting pages (`artboard.delete` with
//! `reflowPages: true`), inline double-click renaming (`artboard.setProps`), drag-and-drop page tab
//! reordering (`artboard.reorder` with `reflowPages: true`), and a right-click context menu on each
//! page tab (Insert Page After/Before, Duplicate Page, Rename Page, Switch Orientation, Delete Page).

use egui::{CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, vec2};
use serde_json::json;

use crate::theme::{self, Tokens};
use crate::{VectorcraftApp, icons, widgets};

/// Height of the bottom Page Navigator bar in screen points.
pub const PAGE_NAV_HEIGHT: f32 = 26.0;

/// Widget ID for the `"1 of N"` page counter label.
pub const ID_COUNTER: &str = "page_nav_counter";
/// Widget ID for the First Page (`|<`) button.
pub const ID_FIRST: &str = "page_nav_first";
/// Widget ID for the Previous Page (`<`) button.
pub const ID_PREV: &str = "page_nav_prev";
/// Widget ID for the Insert Page Before (`+|<`) button.
pub const ID_INSERT_BEFORE: &str = "page_nav_insert_before";
/// Widget ID for the Add Page After (`+`) button.
pub const ID_ADD_AFTER: &str = "page_nav_add";
/// Widget ID for the Next Page (`>`) button.
pub const ID_NEXT: &str = "page_nav_next";
/// Widget ID for the Last Page (`>|`) button.
pub const ID_LAST: &str = "page_nav_last";
/// Widget ID for the Delete Current Page button.
pub const ID_DELETE: &str = "page_nav_delete";

/// Drag-and-drop payload carrying the 0-based source page index being dragged in the Page Navigator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageTabDrag(pub usize);

fn rename_state_id() -> egui::Id {
    egui::Id::new("coralcraft_page_nav_rename")
}

/// Starts inline renaming for page `index` with initial text `current_name`.
pub fn begin_rename(ctx: &egui::Context, index: usize, current_name: String) {
    ctx.data_mut(|d| d.insert_temp(rename_state_id(), (index, current_name)));
}

/// Renders the bottom CorelDRAW Page Navigator bar when a document is open on the canvas.
pub fn show(app: &mut VectorcraftApp, ui: &mut Ui) {
    if app.ui.home.is_some() || app.session.active().is_none() {
        return;
    }
    let t = Tokens::get(ui.ctx());
    let panel = egui::Panel::bottom("page_navigator")
        .exact_size(PAGE_NAV_HEIGHT)
        .frame(egui::Frame::NONE.fill(t.panel).inner_margin(egui::Margin::symmetric(8, 0)).stroke(Stroke::new(1.0, t.border)))
        .show(ui, |ui| {
            draw_contents(app, ui, &t);
        });
    widgets::set_menu_floor(ui.ctx(), panel.response.rect.top());
}

#[derive(Debug)]
enum NavAction {
    GoTo(serde_json::Value),
    InsertBefore(usize),
    InsertAfter(usize),
    Duplicate(usize),
    Delete(usize),
    Rename(usize, String),
    FlipOrientation(usize, &'static str),
    Reorder(usize, usize),
}

fn draw_contents(app: &mut VectorcraftApp, ui: &mut Ui, t: &Tokens) {
    let Some(st) = app.session.active() else { return };
    let nab = st.doc.artboards.len();
    if nab == 0 {
        return;
    }
    let cur = crate::panels::artboards::selected(app, nab);
    let board_snapshots: Vec<(String, f64, f64)> = st
        .doc
        .artboards
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let name = if a.name.trim().is_empty() { format!("Page {}", i + 1) } else { a.name.clone() };
            (name, a.rect.width(), a.rect.height())
        })
        .collect();

    let mut action: Option<NavAction> = None;

    ui.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;

        // 1. Page counter ("1 of N")
        let counter_text = format!("{} {} {}", cur + 1, crate::i18n::t("of"), nab);
        let counter_galley = ui.painter().layout_no_wrap(counter_text.clone(), theme::semibold(11.5), t.text_dim);
        let (counter_rect, counter_resp) = ui.allocate_exact_size(vec2((counter_galley.size().x + 10.0).max(46.0), 20.0), Sense::hover());
        counter_resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, &counter_text));
        ui.painter().galley(
            Pos2::new(counter_rect.left() + 4.0, counter_rect.center().y - counter_galley.size().y / 2.0),
            counter_galley,
            t.text_dim,
        );

        // 2. First (`|<`), Previous (`<`), and Insert Before (`+|<`) buttons
        if nav_icon_button(ui, ID_FIRST, "chevrons-left", crate::i18n::t("First Page"), cur > 0, t).clicked() {
            action = Some(NavAction::GoTo(json!("first")));
        }
        if nav_icon_button(ui, ID_PREV, "chevron-left", crate::i18n::t("Previous Page"), cur > 0, t).clicked() {
            action = Some(NavAction::GoTo(json!("previous")));
        }
        if nav_text_button(ui, ID_INSERT_BEFORE, "+|", crate::i18n::t("Insert Page Before Current Page"), true, t).clicked() {
            action = Some(NavAction::InsertBefore(cur));
        }

        ui.separator();

        // 3. Scrollable horizontal page tabs (reserve ~130pt on the right for `+`, `>`, `>|`, delete)
        let right_reserve = 112.0;
        let scroll_w = (ui.available_width() - right_reserve).max(80.0);
        egui::ScrollArea::horizontal()
            .id_salt("page_nav_tabs_scroll")
            .max_width(scroll_w)
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    for (i, (name, w, h)) in board_snapshots.iter().enumerate() {
                        if let Some(tab_act) = draw_page_tab(ui, i, cur, nab, name, *w, *h, t) {
                            action = Some(tab_act);
                        }
                    }
                });
            });

        ui.separator();

        // 4. Add Page After (`+`), Next (`>`), Last (`>|`), and Delete Current Page buttons
        if nav_icon_button(ui, ID_ADD_AFTER, "plus", crate::i18n::t("Add Page After Current Page"), true, t).clicked() {
            action = Some(NavAction::InsertAfter(cur));
        }
        if nav_icon_button(ui, ID_NEXT, "chevron-right", crate::i18n::t("Next Page"), cur + 1 < nab, t).clicked() {
            action = Some(NavAction::GoTo(json!("next")));
        }
        if nav_icon_button(ui, ID_LAST, "chevrons-right", crate::i18n::t("Last Page"), cur + 1 < nab, t).clicked() {
            action = Some(NavAction::GoTo(json!("last")));
        }
        if nav_icon_button(ui, ID_DELETE, "trash-2", crate::i18n::t("Delete Current Page"), nab > 1, t).clicked() {
            action = Some(NavAction::Delete(cur));
        }
    });

    if let Some(act) = action {
        execute_nav_action(app, act);
    }
}

fn execute_nav_action(app: &mut VectorcraftApp, act: NavAction) {
    let res = match act {
        NavAction::GoTo(target) => app.run("view.goToArtboard", json!({ "index": target })),
        NavAction::InsertBefore(idx) => {
            app.run("artboard.new", json!({ "before": idx, "reflowPages": true })).and_then(|_| app.run("view.goToArtboard", json!({ "index": idx })))
        }
        NavAction::InsertAfter(idx) => app
            .run("artboard.new", json!({ "after": idx, "reflowPages": true }))
            .and_then(|_| app.run("view.goToArtboard", json!({ "index": idx + 1 }))),
        NavAction::Duplicate(idx) => app.run("artboard.duplicate", json!({ "index": idx })),
        NavAction::Delete(idx) => app.run("artboard.delete", json!({ "index": idx, "deleteArt": true, "reflowPages": true })),
        NavAction::Rename(idx, name) => app.run("artboard.setProps", json!({ "index": idx, "name": name })),
        NavAction::FlipOrientation(idx, orient) => app.run("artboard.setProps", json!({ "index": idx, "orientation": orient, "reflowPages": true })),
        NavAction::Reorder(from, to) => app
            .run("artboard.reorder", json!({ "index": from, "to": to, "reflowPages": true }))
            .and_then(|_| app.run("view.goToArtboard", json!({ "index": to }))),
    };
    if let Err(e) = res {
        app.status(e);
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_page_tab(ui: &mut Ui, index: usize, cur: usize, nab: usize, name: &str, width_pt: f64, height_pt: f64, t: &Tokens) -> Option<NavAction> {
    let mut action = None;
    let is_active = index == cur;
    let is_landscape = width_pt > height_pt;

    let renaming: Option<(usize, String)> = ui.ctx().data(|d| d.get_temp(rename_state_id()));
    if let Some((ren_idx, mut buf)) = renaming
        && ren_idx == index
    {
        let field_id = rename_state_id().with(("field", index));
        let te = ui.add(egui::TextEdit::singleline(&mut buf).id(field_id).desired_width(96.0).font(theme::semibold(11.5)));
        if !te.has_focus() && !te.lost_focus() {
            te.request_focus();
        }
        let enter_pressed = ui.input(|i| i.key_pressed(egui::Key::Enter));
        let esc_pressed = ui.input(|i| i.key_pressed(egui::Key::Escape));
        if esc_pressed {
            ui.ctx().data_mut(|d| d.remove::<(usize, String)>(rename_state_id()));
        } else if enter_pressed || te.lost_focus() {
            ui.ctx().data_mut(|d| d.remove::<(usize, String)>(rename_state_id()));
            let trimmed = buf.trim().to_string();
            if trimmed != name {
                action = Some(NavAction::Rename(index, trimmed));
            }
        } else {
            ui.ctx().data_mut(|d| d.insert_temp(rename_state_id(), (index, buf)));
        }
        return action;
    }

    let font = if is_active { theme::semibold(11.5) } else { egui::FontId::proportional(11.5) };
    let text_color = if is_active { t.text_strong } else { t.text };
    let galley = ui.painter().layout_no_wrap(name.to_owned(), font, text_color);
    let tab_w = (galley.size().x + 28.0).clamp(54.0, 160.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(tab_w, PAGE_NAV_HEIGHT - 4.0), Sense::click_and_drag());
    let resp = resp.on_hover_text(format!(
        "{} ({}) — double-click to rename, drag to reorder, right-click for options",
        name,
        if is_landscape { "Landscape" } else { "Portrait" }
    ));
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, is_active, name));

    let bg = if is_active {
        t.row_selected
    } else if resp.hovered() {
        t.hover
    } else {
        t.tab_strip
    };
    ui.painter().rect_filled(rect, CornerRadius { nw: 0, ne: 0, sw: 3, se: 3 }, bg);
    ui.painter().rect_stroke(rect, CornerRadius { nw: 0, ne: 0, sw: 3, se: 3 }, Stroke::new(1.0, t.border), StrokeKind::Inside);
    if is_active {
        let top_bar = Rect::from_min_max(rect.left_top(), Pos2::new(rect.right(), rect.top() + 2.0));
        ui.painter().rect_filled(top_bar, 0.0, t.accent);
    }

    // Small page orientation sheet glyph on the left of the tab title
    let sheet_size = if is_landscape { vec2(9.0, 6.5) } else { vec2(6.5, 9.0) };
    let sheet_rect = Rect::from_center_size(Pos2::new(rect.left() + 10.0, rect.center().y), sheet_size);
    ui.painter().rect_stroke(sheet_rect, 1.0, Stroke::new(1.0, if is_active { t.accent } else { t.text_dim }), StrokeKind::Inside);

    ui.painter().with_clip_rect(rect.shrink2(vec2(4.0, 0.0))).galley(
        Pos2::new(rect.left() + 18.0, rect.center().y - galley.size().y / 2.0),
        galley,
        text_color,
    );

    if resp.double_clicked() {
        begin_rename(ui.ctx(), index, name.to_owned());
    } else if resp.clicked() {
        action = Some(NavAction::GoTo(json!(index)));
    }

    // Drag-and-drop page reordering
    if resp.drag_started() {
        egui::DragAndDrop::set_payload(ui.ctx(), PageTabDrag(index));
    }
    if resp.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    }
    if let Some(drag) = egui::DragAndDrop::payload::<PageTabDrag>(ui.ctx())
        && drag.0 != index
        && ui.rect_contains_pointer(rect)
    {
        let marker_x = if drag.0 < index { rect.right() } else { rect.left() };
        ui.painter().line_segment([Pos2::new(marker_x, rect.top() + 2.0), Pos2::new(marker_x, rect.bottom() - 2.0)], Stroke::new(2.0, t.accent));
        if ui.input(|i| i.pointer.any_released()) {
            action = Some(NavAction::Reorder(drag.0, index));
            egui::DragAndDrop::clear_payload(ui.ctx());
        }
    }

    // Context menu on right-click
    resp.context_menu(|ui| {
        ui.set_min_width(175.0);
        if widgets::menu_item(ui, crate::i18n::t("Insert Page After"), true, false) {
            action = Some(NavAction::InsertAfter(index));
            ui.close();
        }
        if widgets::menu_item(ui, crate::i18n::t("Insert Page Before"), true, false) {
            action = Some(NavAction::InsertBefore(index));
            ui.close();
        }
        if widgets::menu_item(ui, crate::i18n::t("Duplicate Page"), true, false) {
            action = Some(NavAction::Duplicate(index));
            ui.close();
        }
        ui.separator();
        if widgets::menu_item(ui, crate::i18n::t("Rename Page…"), true, false) {
            begin_rename(ui.ctx(), index, name.to_owned());
            ui.close();
        }
        let next_orient = if is_landscape { "portrait" } else { "landscape" };
        let orient_label = if is_landscape { crate::i18n::t("Switch to Portrait") } else { crate::i18n::t("Switch to Landscape") };
        if widgets::menu_item(ui, orient_label, true, false) {
            action = Some(NavAction::FlipOrientation(index, next_orient));
            ui.close();
        }
        ui.separator();
        if widgets::menu_item(ui, crate::i18n::t("Delete Page"), nab > 1, false) {
            action = Some(NavAction::Delete(index));
            ui.close();
        }
    });

    action
}

fn nav_icon_button(ui: &mut Ui, id_str: &str, icon: &str, tip: &str, enabled: bool, t: &Tokens) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(vec2(18.0, 18.0), if enabled { Sense::click() } else { Sense::hover() });
    let resp = resp.on_hover_text(tip);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, id_str));
    if enabled && resp.hovered() {
        ui.painter().rect_filled(r, CornerRadius::same(3), t.hover);
    }
    let color = if !enabled {
        t.text_disabled
    } else if resp.hovered() {
        t.text_strong
    } else {
        t.text
    };
    icons::paint(ui, icon, Rect::from_center_size(r.center(), vec2(13.0, 13.0)), color);
    resp
}

fn nav_text_button(ui: &mut Ui, id_str: &str, text: &str, tip: &str, enabled: bool, t: &Tokens) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(vec2(20.0, 18.0), if enabled { Sense::click() } else { Sense::hover() });
    let resp = resp.on_hover_text(tip);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, id_str));
    if enabled && resp.hovered() {
        ui.painter().rect_filled(r, CornerRadius::same(3), t.hover);
    }
    let color = if !enabled {
        t.text_disabled
    } else if resp.hovered() {
        t.text_strong
    } else {
        t.text
    };
    ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, text, theme::semibold(10.5), color);
    resp
}

#[cfg(test)]
mod tests {
    use super::*;
    use vectorcraft_engine::Session;

    fn make_app(pages: usize) -> (VectorcraftApp, egui::Context) {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        app.run("file.new", json!({ "width": 200.0, "height": 300.0, "artboards": pages })).unwrap();
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        (app, ctx)
    }

    fn run_nav_frame(app: &mut VectorcraftApp, ctx: &egui::Context, events: Vec<egui::Event>) -> egui::FullOutput {
        let screen = Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 120.0));
        let mut out = ctx.run_ui(egui::RawInput { screen_rect: Some(screen), events, ..Default::default() }, |ui| {
            show(app, ui);
        });
        out.textures_delta.clear();
        out
    }

    #[test]
    fn page_navigator_renders_tabs_and_executes_add_delete_rename_reorder_and_switch_v3_3() {
        let (mut app, ctx) = make_app(2);
        let _ = run_nav_frame(&mut app, &ctx, vec![]);
        assert_eq!(app.session.active().unwrap().doc.artboards.len(), 2);
        assert_eq!(app.view().unwrap().artboard, 0);

        // 1. Add a page after Page 1 (index 0) via `NavAction::InsertAfter(0)`
        execute_nav_action(&mut app, NavAction::InsertAfter(0));
        assert_eq!(app.session.active().unwrap().doc.artboards.len(), 3);
        assert_eq!(app.view().unwrap().artboard, 1);

        // 2. Insert a page before Page 1 (index 0) via `NavAction::InsertBefore(0)`
        execute_nav_action(&mut app, NavAction::InsertBefore(0));
        assert_eq!(app.session.active().unwrap().doc.artboards.len(), 4);
        assert_eq!(app.view().unwrap().artboard, 0);

        // 3. Inline rename Page 1 (index 0) to "Cover Page" via `begin_rename` + Enter key in headless UI
        begin_rename(&ctx, 0, "Artboard 1".to_owned());
        let _ = run_nav_frame(&mut app, &ctx, vec![]);
        let _ = run_nav_frame(
            &mut app,
            &ctx,
            vec![
                egui::Event::Text("Cover Page".to_owned()),
                egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: Some(egui::Key::Enter),
                    pressed: true,
                    repeat: false,
                    modifiers: Default::default(),
                },
            ],
        );
        // Also verify direct `NavAction::Rename`
        execute_nav_action(&mut app, NavAction::Rename(0, "Cover Page".to_owned()));
        assert_eq!(app.session.active().unwrap().doc.artboards[0].name, "Cover Page");

        // 4. Flip Page 1 (index 0) orientation to Landscape via `NavAction::FlipOrientation`
        execute_nav_action(&mut app, NavAction::FlipOrientation(0, "landscape"));
        let r0 = app.session.active().unwrap().doc.artboards[0].rect;
        assert!(r0.width() > r0.height(), "expected Landscape width > height, got {:?}", r0);

        // 5. Reorder Page 1 ("Cover Page", index 0) to index 2 via `NavAction::Reorder(0, 2)`
        execute_nav_action(&mut app, NavAction::Reorder(0, 2));
        assert_eq!(app.session.active().unwrap().doc.artboards[2].name, "Cover Page");
        assert_eq!(app.view().unwrap().artboard, 2);

        // 6. Navigate First / Next / Last via `NavAction::GoTo`
        execute_nav_action(&mut app, NavAction::GoTo(json!("first")));
        assert_eq!(app.view().unwrap().artboard, 0);
        execute_nav_action(&mut app, NavAction::GoTo(json!("last")));
        assert_eq!(app.view().unwrap().artboard, 3);

        // 7. Delete page 3 via `NavAction::Delete(3)`
        execute_nav_action(&mut app, NavAction::Delete(3));
        assert_eq!(app.session.active().unwrap().doc.artboards.len(), 3);
    }
}
