//! Interactive pixel-coordinate GUI session controller (`InteractiveGuiSession`)
//! that maps window pointer/keyboard/resize events to [`DesktopWorkspace`] actions
//! and renders RGBA8 workspace frames for native OS windowing (Linux X11, Windows Win32,
//! macOS AppKit/CoreGraphics, and Web HTML5 Canvas).

#![allow(clippy::arithmetic_side_effects)]

use coralcraft_color::Color;
use coralcraft_doc::{Document, FillStyle, ObjectKind, OutlineStyle};
use coralcraft_engine::EngineCommand;
use coralcraft_geom::{CornerKind, EllipseSpec, F64, Point, PolygonSpec, Rect, RectSpec};
use coralcraft_render::{RenderError, RenderOutput, export_pdf, export_svg};
use coralcraft_tools::{KeyShortcut, ToolKind};

use crate::{DesktopWorkspace, DockerTab, StandardToolbarControl, UiAction};

/// Mouse button identifier for window pointer events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// Interactive desktop GUI session wrapping [`DesktopWorkspace`] with pixel-coordinate
/// hit-testing across all 7 classic CorelDRAW panels.
#[derive(Debug, Clone)]
pub struct InteractiveGuiSession {
    pub workspace: DesktopWorkspace,
    pub dragging_canvas: bool,
    pub dirty: bool,
    pub last_status_note: Option<String>,
}

impl Default for InteractiveGuiSession {
    fn default() -> Self {
        Self::new_with_welcome_artwork()
    }
}

impl InteractiveGuiSession {
    /// Creates an interactive GUI session for `document`.
    #[must_use]
    pub fn new(document: Document) -> Self {
        Self { workspace: DesktopWorkspace::new(document), dragging_canvas: false, dirty: true, last_status_note: None }
    }

    /// Creates an interactive GUI session populated with a clean-room starter vector illustration
    /// so the user immediately sees rich vector objects, gradients, and selection handles upon launch.
    #[must_use]
    pub fn new_with_welcome_artwork() -> Self {
        let mut session = Self::new(Document::new_a4_portrait());
        session.populate_starter_scene();
        session
    }

    /// Populates a clean-room vector starter scene on the active page.
    pub fn populate_starter_scene(&mut self) {
        if let Ok(r) = Rect::new(60.0, 70.0, 260.0, 200.0)
            && let Ok(spec) = RectSpec::uniform(r, 14.0, CornerKind::Round)
        {
            self.workspace.engine.execute(EngineCommand::AddObject { name: Some("Hero Card".to_owned()), kind: ObjectKind::Rect(spec) });
            self.workspace.engine.execute(EngineCommand::SetFill { fill: Some(FillStyle::Uniform { color: Color::rgb8(0, 120, 215) }) });
        }
        if let Ok(r) = Rect::new(210.0, 140.0, 370.0, 300.0) {
            let spec = EllipseSpec::full(r);
            self.workspace.engine.execute(EngineCommand::AddObject { name: Some("Accent Circle".to_owned()), kind: ObjectKind::Ellipse(spec) });
            self.workspace.engine.execute(EngineCommand::SetFill { fill: Some(FillStyle::Uniform { color: Color::rgb8(240, 90, 50) }) });
        }
        if let Ok(center) = Point::new(430.0, 180.0)
            && let Ok(star) = PolygonSpec::star(center, 72.0, 32.0, 7, 0.0)
        {
            self.workspace.engine.execute(EngineCommand::AddObject { name: Some("Burst Star".to_owned()), kind: ObjectKind::Polygon(star) });
            self.workspace.engine.execute(EngineCommand::SetFill { fill: Some(FillStyle::Uniform { color: Color::rgb8(250, 200, 30) }) });
            self.workspace.engine.execute(EngineCommand::SetOutline { outline: Some(OutlineStyle::hairline(Color::BLACK)) });
        }
        if let Ok(pos) = Point::new(80.0, 360.0)
            && let Ok(size_pt) = F64::new(28.0)
        {
            self.workspace.engine.execute(EngineCommand::AddObject {
                name: Some("Headline".to_owned()),
                kind: ObjectKind::ArtisticText { text: "CORALCRAFT STUDIO".to_owned(), font_family: "Sans".to_owned(), size_pt, position: pos },
            });
        }
        // Arm the Pick tool so the user can immediately select/drag or switch tools
        self.workspace.dispatch(UiAction::SelectTool(ToolKind::Pick));
        self.dirty = true;
    }

    /// Resizes the workspace viewport to `(width, height)`.
    pub fn resize(&mut self, width: u32, height: u32) {
        let w = width.clamp(640, 3840);
        let h = height.clamp(480, 2160);
        if self.workspace.window_width != w || self.workspace.window_height != h {
            self.workspace.window_width = w;
            self.workspace.window_height = h;
            self.dirty = true;
        }
    }

    /// Converts window pixel coordinates `(x_px, y_px)` into page typographic points `(pt_x, pt_y)`.
    #[must_use]
    pub fn window_pixel_to_page_point(&self, x_px: i32, y_px: i32) -> Point {
        let layout = self.workspace.compute_layout();
        let cv = layout.canvas_viewport;
        let cx0 = cv.x0.get().round() as i32;
        let cy0 = cv.y0.get().round() as i32;
        let cx1 = cv.x1.get().round() as i32;
        let cy1 = cv.y1.get().round() as i32;
        let ruler_thick = 20;
        let pb_x0 = cx0 + ruler_thick;
        let pb_y0 = cy0 + ruler_thick;

        let page = self.workspace.engine.current_page();
        let page_w_pt = page.size.width.get().max(1.0);
        let page_h_pt = page.size.height.get().max(1.0);
        let avail_w = f64::from((cx1 - pb_x0 - 48).max(32));
        let avail_h = f64::from((cy1 - pb_y0 - 48).max(32));
        let fit_scale = (avail_w / page_w_pt).min(avail_h / page_h_pt).clamp(0.1, 2.0);

        let pw = (page_w_pt * fit_scale).max(1.0).round() as i32;
        let ph = (page_h_pt * fit_scale).max(1.0).round() as i32;
        let page_x0 = pb_x0 + ((cx1 - pb_x0) - pw) / 2;
        let page_y0 = pb_y0 + ((cy1 - pb_y0) - ph) / 2;

        let pt_x = f64::from(x_px - page_x0) / fit_scale;
        let pt_y = f64::from(y_px - page_y0) / fit_scale;
        Point::new(pt_x, pt_y).unwrap_or(Point::ZERO)
    }

    /// Handles a mouse button press at window pixel `(x_px, y_px)`.
    pub fn pointer_down(&mut self, x_px: i32, y_px: i32, button: MouseButton, shift: bool, alt: bool) {
        let snap = self.workspace.snapshot();
        let layout = snap.layout;

        // 1. Menu Bar (`y in 0..24`)
        if (0..24).contains(&y_px) {
            let mut menu_x = 8;
            for (idx, item) in snap.menu_bar.iter().enumerate() {
                let item_w = (item.len() as i32) * 6 + 18;
                if x_px >= menu_x && x_px < menu_x + item_w {
                    self.handle_menu_click(idx, shift);
                    self.dirty = true;
                    return;
                }
                menu_x += item_w;
            }
            return;
        }

        // 2. Standard Toolbar (`y in 24..56`)
        if (24..56).contains(&y_px) {
            for (i, ctrl) in StandardToolbarControl::ALL.iter().enumerate() {
                let bx = 4 + (i as i32) * 28;
                if x_px >= bx && x_px < bx + 24 && (28..52).contains(&y_px) {
                    self.handle_toolbar_click(*ctrl);
                    self.dirty = true;
                    return;
                }
            }
            return;
        }

        // 3. Property Bar (`y in 56..90`)
        if (56..90).contains(&y_px) {
            return;
        }

        // 4. Bottom Page Navigator Bar
        let pn = layout.page_navigator_bar;
        let pnx0 = pn.x0.get().round() as i32;
        let pny0 = pn.y0.get().round() as i32;
        let pnh = pn.height().round() as i32;
        if y_px >= pny0 && y_px < pny0 + pnh {
            if x_px >= pnx0 + 4 && x_px < pnx0 + 24 {
                self.workspace.dispatch(UiAction::AddPage);
                self.dirty = true;
                return;
            }
            let mut tab_x = pnx0 + 28;
            for tab in &snap.page_tabs {
                let tab_w = ((tab.label.len() as i32) * 6 + 24).max(76);
                if x_px >= tab_x && x_px < tab_x + tab_w {
                    if button == MouseButton::Right
                        && let coralcraft_engine::ActivePageTarget::Page(page_index) = tab.target
                    {
                        self.workspace.dispatch(UiAction::DuplicatePage { page_index });
                    } else {
                        self.workspace.dispatch(UiAction::ActivatePageTarget(tab.target));
                    }
                    self.dirty = true;
                    return;
                }
                tab_x += tab_w + 4;
            }
            return;
        }

        // 5. Status Bar
        let sb = layout.status_bar;
        if y_px >= sb.y0.get().round() as i32 {
            return;
        }

        // 6. Left Vertical Toolbox (`x < 38`)
        let lt = layout.left_toolbox;
        let ltx0 = lt.x0.get().round() as i32;
        let lty0 = lt.y0.get().round() as i32;
        let ltw = lt.width().round() as i32;
        if x_px >= ltx0 && x_px < ltx0 + ltw {
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
                let by = lty0 + 4 + (idx as i32) * 32;
                if y_px >= by && y_px < by + 30 {
                    self.workspace.dispatch(UiAction::SelectTool(*tk));
                    self.dirty = true;
                    return;
                }
            }
            return;
        }

        // 7. Right Vertical Color Palette Bar
        let rp = layout.right_palette_bar;
        let rpx0 = rp.x0.get().round() as i32;
        let rpy0 = rp.y0.get().round() as i32;
        let rpw = rp.width().round() as i32;
        if x_px >= rpx0 && x_px < rpx0 + rpw {
            let rel_y = y_px - (rpy0 + 2);
            if rel_y >= 0 {
                let idx = (rel_y / 22) as usize;
                if idx < snap.palette_cells.len() {
                    match button {
                        MouseButton::Right => self.workspace.dispatch(UiAction::PaletteRightClick { cell_index: idx }),
                        _ => self.workspace.dispatch(UiAction::PaletteLeftClick { cell_index: idx }),
                    }
                    self.dirty = true;
                }
            }
            return;
        }

        // 8. Right Collapsible Docker (`Objects` / `Properties`)
        if let Some(dk) = layout.right_docker {
            let dx0 = dk.x0.get().round() as i32;
            let dy0 = dk.y0.get().round() as i32;
            let dw = dk.width().round() as i32;
            if x_px >= dx0 && x_px < dx0 + dw {
                if y_px >= dy0 && y_px < dy0 + 24 {
                    if x_px >= dx0 + 4 && x_px < dx0 + 84 {
                        self.workspace.dispatch(UiAction::SelectDockerTab(DockerTab::Objects));
                        self.dirty = true;
                    } else if x_px >= dx0 + 88 && x_px < dx0 + 176 {
                        self.workspace.dispatch(UiAction::SelectDockerTab(DockerTab::Properties));
                        self.dirty = true;
                    }
                    return;
                }
                let rel_y = y_px - (dy0 + 28);
                if rel_y >= 0 {
                    let row_idx = (rel_y / 20) as usize;
                    if let Some(entry) = snap.object_tree.get(row_idx) {
                        if x_px >= dx0 + dw - 28 {
                            self.workspace.engine.execute(EngineCommand::SetObjectVisible { object_id: entry.object_id, visible: !entry.visible });
                        } else {
                            self.workspace.dispatch(UiAction::SelectObjectFromTree { object_id: entry.object_id, additive: shift });
                        }
                        self.dirty = true;
                    }
                }
                return;
            }
        }

        // 9. Central Canvas & Pasteboard
        let cv = layout.canvas_viewport;
        let cx0 = cv.x0.get().round() as i32;
        let cy0 = cv.y0.get().round() as i32;
        let cx1 = cv.x1.get().round() as i32;
        let cy1 = cv.y1.get().round() as i32;
        if x_px >= cx0 && x_px < cx1 && y_px >= cy0 && y_px < cy1 {
            let pt = self.window_pixel_to_page_point(x_px, y_px);
            self.dragging_canvas = true;
            self.workspace.dispatch(UiAction::CanvasPointerDown { pt, shift, alt });
            self.dirty = true;
        }
    }

    /// Handles pointer motion at window pixel `(x_px, y_px)`.
    pub fn pointer_move(&mut self, x_px: i32, y_px: i32, shift: bool, alt: bool) {
        let pt = self.window_pixel_to_page_point(x_px, y_px);
        self.workspace.engine.cursor_pt = pt;
        if self.dragging_canvas {
            self.workspace.dispatch(UiAction::CanvasPointerMove { pt, shift, alt });
            self.dirty = true;
        }
    }

    /// Handles mouse button release at window pixel `(x_px, y_px)`.
    pub fn pointer_up(&mut self, x_px: i32, y_px: i32, _button: MouseButton, shift: bool, alt: bool) {
        if self.dragging_canvas {
            let pt = self.window_pixel_to_page_point(x_px, y_px);
            self.dragging_canvas = false;
            self.workspace.dispatch(UiAction::CanvasPointerUp { pt, shift, alt });
            self.dirty = true;
        }
    }

    /// Handles a keyboard shortcut or key press.
    pub fn key_down(&mut self, key: &str, ctrl: bool, shift: bool) {
        if ctrl {
            match key.to_ascii_lowercase().as_str() {
                "z" => {
                    if shift {
                        self.workspace.dispatch(UiAction::Redo);
                    } else {
                        self.workspace.dispatch(UiAction::Undo);
                    }
                    self.dirty = true;
                    return;
                }
                "y" => {
                    self.workspace.dispatch(UiAction::Redo);
                    self.dirty = true;
                    return;
                }
                "d" => {
                    if let Ok(ten) = F64::new(10.0) {
                        self.workspace.engine.execute(EngineCommand::DuplicateSelection { offset_x_pt: ten, offset_y_pt: ten });
                    }
                    self.dirty = true;
                    return;
                }
                "g" => {
                    self.workspace.dispatch(UiAction::GroupSelection);
                    self.dirty = true;
                    return;
                }
                "u" => {
                    self.workspace.dispatch(UiAction::UngroupSelection);
                    self.dirty = true;
                    return;
                }
                "q" => {
                    self.workspace.dispatch(UiAction::ConvertToCurves);
                    self.dirty = true;
                    return;
                }
                _ => {}
            }
        }

        match key {
            "F5" | "f5" => self.workspace.dispatch(UiAction::Shortcut(KeyShortcut::F5)),
            "F6" | "f6" => self.workspace.dispatch(UiAction::Shortcut(KeyShortcut::F6)),
            "F7" | "f7" => self.workspace.dispatch(UiAction::Shortcut(KeyShortcut::F7)),
            "F8" | "f8" => self.workspace.dispatch(UiAction::Shortcut(KeyShortcut::F8)),
            "F10" | "f10" => self.workspace.dispatch(UiAction::Shortcut(KeyShortcut::F10)),
            "Delete" | "BackSpace" | "Backspace" => self.workspace.dispatch(UiAction::DeleteSelection),
            " " | "Space" => self.workspace.dispatch(UiAction::SelectTool(ToolKind::Pick)),
            s if s.len() == 1 => {
                if let Some(ch) = s.chars().next() {
                    self.workspace.dispatch(UiAction::Shortcut(KeyShortcut::Char(ch)));
                }
            }
            _ => return,
        }
        self.dirty = true;
    }

    /// Renders the current workspace frame to an RGBA8 [`RenderOutput`].
    pub fn render_frame(&mut self) -> Result<RenderOutput, RenderError> {
        self.dirty = false;
        self.workspace.render_workspace_png()
    }

    fn handle_menu_click(&mut self, menu_idx: usize, shift: bool) {
        match menu_idx {
            0 => self.handle_toolbar_click(StandardToolbarControl::Save),
            1 => {
                if shift {
                    self.workspace.dispatch(UiAction::Redo);
                } else {
                    self.workspace.dispatch(UiAction::Undo);
                }
            }
            2 => {
                if self.workspace.horizontal_guidelines.is_empty() {
                    if let Ok(v) = F64::new(200.0) {
                        self.workspace.dispatch(UiAction::AddHorizontalGuideline(v));
                        self.workspace.dispatch(UiAction::AddVerticalGuideline(v));
                    }
                } else {
                    self.workspace.dispatch(UiAction::ClearGuidelines);
                }
            }
            3 => self.workspace.dispatch(UiAction::AddPage),
            4 => self.workspace.dispatch(UiAction::ConvertToCurves),
            5 => {
                if let Some(&id) = self.workspace.engine.selection.first()
                    && let Ok(off) = F64::new(5.0)
                {
                    self.workspace.engine.execute(EngineCommand::ApplyContour { object_id: id, steps: 2, offset_pt: off, inside: false });
                }
            }
            9 => self.workspace.dispatch(UiAction::SetDockerOpen(!self.workspace.docker_open)),
            _ => {}
        }
    }

    fn handle_toolbar_click(&mut self, ctrl: StandardToolbarControl) {
        match ctrl {
            StandardToolbarControl::New => {
                self.workspace = DesktopWorkspace::new(Document::new_a4_portrait());
            }
            StandardToolbarControl::Open => {
                self.populate_starter_scene();
            }
            StandardToolbarControl::Save => {
                if let Ok(bytes) = coralcraft_doc::save_zip(&self.workspace.engine.document) {
                    let _ = std::fs::write("coralcraft-workspace.coralcraft", bytes);
                    self.last_status_note = Some("Saved coralcraft-workspace.coralcraft".to_owned());
                }
            }
            StandardToolbarControl::Print => {
                if let Ok(pdf) = export_pdf(&self.workspace.engine.document) {
                    let _ = std::fs::write("coralcraft-export.pdf", pdf);
                    self.last_status_note = Some("Exported coralcraft-export.pdf".to_owned());
                }
            }
            StandardToolbarControl::Cut => {
                self.workspace.dispatch(UiAction::DeleteSelection);
            }
            StandardToolbarControl::Copy | StandardToolbarControl::Paste => {
                if let Ok(ten) = F64::new(10.0) {
                    self.workspace.engine.execute(EngineCommand::DuplicateSelection { offset_x_pt: ten, offset_y_pt: ten });
                }
            }
            StandardToolbarControl::Undo => {
                self.workspace.dispatch(UiAction::Undo);
            }
            StandardToolbarControl::Redo => {
                self.workspace.dispatch(UiAction::Redo);
            }
            StandardToolbarControl::Import => {
                if let Ok(center) = Point::new(280.0, 260.0)
                    && let Ok(path) = coralcraft_geom::build_koch_snowflake_path(center, 55.0, 3)
                {
                    self.workspace.engine.execute(EngineCommand::AddObject { name: Some("Koch Snowflake".to_owned()), kind: ObjectKind::Path(path) });
                }
            }
            StandardToolbarControl::Export => {
                if let Ok(svg) = export_svg(&self.workspace.engine.document, self.workspace.engine.current_page_index()) {
                    let _ = std::fs::write("coralcraft-export.svg", svg);
                }
                if let Ok(pdf) = export_pdf(&self.workspace.engine.document) {
                    let _ = std::fs::write("coralcraft-export.pdf", pdf);
                }
                self.last_status_note = Some("Exported coralcraft-export.svg & .pdf".to_owned());
            }
            StandardToolbarControl::ZoomPercent => {
                self.workspace.engine.zoom = F64::ONE;
            }
            StandardToolbarControl::SnapToObjects => {
                self.workspace.engine.snap_to_objects = !self.workspace.engine.snap_to_objects;
            }
            StandardToolbarControl::SnapToPage => {
                self.workspace.engine.snap_to_page = !self.workspace.engine.snap_to_page;
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn interactive_gui_session_routes_toolbox_canvas_palette_and_page_tabs() {
        let mut session = InteractiveGuiSession::new_with_welcome_artwork();
        assert_eq!(session.workspace.snapshot().object_tree.len(), 4);

        // Click Rectangle tool (index 5 in left toolbox: x=16, y=94 + 5*32 + 10 = 264)
        session.pointer_down(16, 264, MouseButton::Left, false, false);
        assert_eq!(session.workspace.snapshot().active_tool, ToolKind::Rectangle);

        // Drag on canvas to draw a new rectangle
        session.pointer_down(400, 350, MouseButton::Left, false, false);
        session.pointer_move(520, 440, false, false);
        session.pointer_up(520, 440, MouseButton::Left, false, false);
        assert_eq!(session.workspace.snapshot().object_tree.len(), 5);

        // Click '+' button on bottom Page Navigator bar (x=12, y=858)
        let pny0 = session.workspace.snapshot().layout.page_navigator_bar.y0.get().round() as i32;
        session.pointer_down(12, pny0 + 10, MouseButton::Left, false, false);
        assert_eq!(session.workspace.engine.document.pages.len(), 2);

        // Render frame
        let out = session.render_frame().unwrap();
        assert_eq!(out.surface.width, 1440);
        assert_eq!(out.surface.height, 900);
    }
}
