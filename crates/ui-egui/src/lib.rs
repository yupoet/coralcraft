//! Classic CorelDRAW desktop workspace frontend model, presenters, and `UiAction`
//! dispatcher (`coralcraft-ui-egui`, L6).
//!
//! Layout hierarchy (pixel-aligned to classic CorelDRAW desktop workspace):
//! ```text
//! Menu Bar: File | Edit | View | Layout | Object | Effects | Bitmaps | Text | Tools | Window | Help
//! Standard Toolbar: New | Open | Save | Print | Cut | Copy | Paste | Undo | Redo | Import | Export | Zoom | Snap
//! Context-Sensitive Property Bar (driven by PropertyBarSchema)
//! +----------+-----------------------------------+----------+----------------+
//! | Left     | Central Multi-Page Pasteboard     | Right    | Right Dockers  |
//! | Vertical | Canvas (with page drop shadow &   | Vertical | (Objects tree  |
//! | Toolbox  | 8-handle selection box)           | Palette  | & Properties)  |
//! +----------+-----------------------------------+----------+----------------+
//! | Bottom Page Navigator: [+] [Page 1] [Page 2] ... [Master Page] [Desktop] |
//! | Status Bar: (x, y) | Selection Summary | Fill Chip | Outline Chip        |
//! +--------------------------------------------------------------------------+
//! ```

use coralcraft_color::Color;
use coralcraft_doc::{Document, LayerRole, ObjectId};
use coralcraft_engine::{ActivePageTarget, EditorEngine, EngineCommand, PaletteChannel, StatusBarSnapshot};
use coralcraft_geom::{CornerKind, EllipseMode, F64, Point, Rect, Unit};
use coralcraft_render::{RenderError, RenderOpts, RenderOutput, render_page};
use coralcraft_tools::{KeyShortcut, PolygonSubMode, PropertyBarSchema, ToolKind, combined_selection_bounds};
use serde::{Deserialize, Serialize};

/// Top Menu Bar items in classic CorelDRAW order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MenuBarCategory {
    File,
    Edit,
    View,
    Layout,
    Object,
    Effects,
    Bitmaps,
    Text,
    Tools,
    Window,
    Help,
}

impl MenuBarCategory {
    /// Fixed left-to-right CorelDRAW top menu bar order.
    pub const ALL: [Self; 11] = [
        Self::File,
        Self::Edit,
        Self::View,
        Self::Layout,
        Self::Object,
        Self::Effects,
        Self::Bitmaps,
        Self::Text,
        Self::Tools,
        Self::Window,
        Self::Help,
    ];

    /// Menu label displayed in the top menu bar.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::File => "File",
            Self::Edit => "Edit",
            Self::View => "View",
            Self::Layout => "Layout",
            Self::Object => "Object",
            Self::Effects => "Effects",
            Self::Bitmaps => "Bitmaps",
            Self::Text => "Text",
            Self::Tools => "Tools",
            Self::Window => "Window",
            Self::Help => "Help",
        }
    }
}

/// Standard Toolbar controls from left to right.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StandardToolbarControl {
    New,
    Open,
    Save,
    Print,
    Cut,
    Copy,
    Paste,
    Undo,
    Redo,
    Import,
    Export,
    ZoomPercent,
    SnapToObjects,
    SnapToPage,
}

impl StandardToolbarControl {
    /// Fixed left-to-right CorelDRAW Standard Toolbar order.
    pub const ALL: [Self; 14] = [
        Self::New,
        Self::Open,
        Self::Save,
        Self::Print,
        Self::Cut,
        Self::Copy,
        Self::Paste,
        Self::Undo,
        Self::Redo,
        Self::Import,
        Self::Export,
        Self::ZoomPercent,
        Self::SnapToObjects,
        Self::SnapToPage,
    ];
}

/// Swatch entry in the right vertical CorelDRAW Color Palette Bar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaletteCell {
    /// Top `[X]` No-Color swatch that clears Fill on left-click or Outline on right-click.
    NoColor,
    /// Named or unnamed color swatch.
    Color { name: Option<String>, color: Color },
}

/// Tab in the bottom CorelDRAW Page Navigator Bar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageNavigatorTab {
    pub target: ActivePageTarget,
    pub label: String,
    pub active: bool,
    pub can_delete: bool,
}

/// Right collapsible Docker tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum DockerTab {
    /// CorelDRAW `Objects` (Object Manager) hierarchical page/layer/object tree.
    #[default]
    Objects,
    /// CorelDRAW `Properties` Docker (Fill, Outline, and geometry properties).
    Properties,
}

/// Node in the `Objects` (Object Manager) Docker tree for the active page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectTreeEntry {
    pub layer_name: String,
    pub is_master: bool,
    pub is_desktop: bool,
    pub object_id: ObjectId,
    pub object_label: String,
    pub selected: bool,
    pub visible: bool,
    pub locked: bool,
}

/// Computed layout rectangles (in window pixels) of the classic CorelDRAW desktop workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceLayoutRects {
    pub window_width: u32,
    pub window_height: u32,
    pub menu_bar: Rect,
    pub standard_toolbar: Rect,
    pub property_bar: Rect,
    pub left_toolbox: Rect,
    pub canvas_viewport: Rect,
    pub right_palette_bar: Rect,
    pub right_docker: Option<Rect>,
    pub page_navigator_bar: Rect,
    pub status_bar: Rect,
}

/// Complete declarative presenter snapshot of the CorelDRAW desktop workspace for a single frame.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceFrameSnapshot {
    pub layout: WorkspaceLayoutRects,
    pub menu_bar: Vec<String>,
    pub active_tool: ToolKind,
    pub can_undo: bool,
    pub can_redo: bool,
    pub property_bar: PropertyBarSchema,
    pub palette_cells: Vec<PaletteCell>,
    pub page_tabs: Vec<PageNavigatorTab>,
    pub status_bar: StatusBarSnapshot,
    pub docker_open: bool,
    pub active_docker_tab: DockerTab,
    pub object_tree: Vec<ObjectTreeEntry>,
    pub selection_bounds: Option<Rect>,
}

/// User interface action emitted by widgets and routed by [`DesktopWorkspace::dispatch`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiAction {
    /// Selects a tool in the left Toolbox.
    SelectTool(ToolKind),
    /// Presses a single-key or function-key shortcut while canvas is focused.
    Shortcut(KeyShortcut),
    /// Primary pointer down on the canvas in page coordinates.
    CanvasPointerDown { pt: Point, shift: bool, alt: bool },
    /// Pointer move on the canvas in page coordinates.
    CanvasPointerMove { pt: Point, shift: bool, alt: bool },
    /// Primary pointer release on the canvas in page coordinates.
    CanvasPointerUp { pt: Point, shift: bool, alt: bool },
    /// Left-clicks a cell in the right vertical Color Palette Bar (`0` = `[X]` No-Color) -> sets Fill.
    PaletteLeftClick { cell_index: usize },
    /// Right-clicks a cell in the right vertical Color Palette Bar (`0` = `[X]` No-Color) -> sets Outline.
    PaletteRightClick { cell_index: usize },
    /// Clicks a tab in the bottom Page Navigator Bar.
    ActivatePageTarget(ActivePageTarget),
    /// Clicks the `+` button in the bottom Page Navigator Bar.
    AddPage,
    /// Context-menu `Duplicate` on a numbered page tab.
    DuplicatePage { page_index: usize },
    /// Context-menu `Rename` on a numbered page tab.
    RenamePage { page_index: usize, name: String },
    /// Context-menu `Delete` on a numbered page tab.
    DeletePage { page_index: usize },
    /// Commits a Page Setup edit from the Property Bar.
    CommitPageSetup { width_pt: F64, height_pt: F64, units: Unit },
    /// Commits a Rectangle corner edit from the Property Bar.
    CommitRectCorners { corner_kind: CornerKind, radii: [F64; 4], corners_locked: bool },
    /// Commits an Ellipse mode/angle edit from the Property Bar.
    CommitEllipseParameters { mode: EllipseMode, start_deg: F64, end_deg: F64 },
    /// Commits a Polygon/Star point/radius edit from the Property Bar.
    CommitPolygonParameters { mode: PolygonSubMode, points: u32, inner_radius_pt: Option<F64> },
    /// Selects an object directly from the `Objects` Docker tree.
    SelectObjectFromTree { object_id: ObjectId, additive: bool },
    /// Toggles whether the right Docker column (`Objects` / `Properties`) is expanded or collapsed.
    SetDockerOpen(bool),
    /// Switches the active right Docker tab (`Objects` vs `Properties`).
    SelectDockerTab(DockerTab),
    /// Scrolls the right vertical Color Palette Bar independently of the canvas.
    ScrollPalette { offset_px: i32 },
    /// Triggers Undo (`Ctrl+Z`).
    Undo,
    /// Triggers Redo (`Ctrl+Y` / `Ctrl+Shift+Z`).
    Redo,
    /// Deletes the current selection (`Delete` / `Backspace`).
    DeleteSelection,
}

/// Stateful desktop workspace controller (`coralcraft-ui-egui`, L6) wrapping [`EditorEngine`].
#[derive(Debug, Clone)]
pub struct DesktopWorkspace {
    pub engine: EditorEngine,
    pub window_width: u32,
    pub window_height: u32,
    pub docker_open: bool,
    pub active_docker_tab: DockerTab,
    pub palette_scroll_px: i32,
}

impl Default for DesktopWorkspace {
    fn default() -> Self {
        Self::new(Document::new_a4_portrait())
    }
}

impl DesktopWorkspace {
    /// Creates a new CorelDRAW desktop workspace for `document` at `1440×900`.
    #[must_use]
    pub fn new(document: Document) -> Self {
        Self {
            engine: EditorEngine::new(document),
            window_width: 1440,
            window_height: 900,
            docker_open: true,
            active_docker_tab: DockerTab::Objects,
            palette_scroll_px: 0,
        }
    }

    /// Computes pixel-accurate classic CorelDRAW panel rectangles for `(window_width, window_height)`.
    ///
    /// When `docker_open == false`, the 260 px right docker column is returned to `canvas_viewport`.
    #[must_use]
    pub fn compute_layout(&self) -> WorkspaceLayoutRects {
        let w = f64::from(self.window_width.max(640));
        let h = f64::from(self.window_height.max(480));
        let rect = |x0: f64, y0: f64, x1: f64, y1: f64| -> Rect {
            Rect::new(x0, y0, x1, y1).unwrap_or(Rect { x0: F64::ZERO, y0: F64::ZERO, x1: F64::ZERO, y1: F64::ZERO })
        };

        let menu_h = 24.0;
        let std_tb_h = 32.0;
        let prop_bar_h = 34.0;
        let top_chrome_h = menu_h + std_tb_h + prop_bar_h;

        let status_h = 24.0;
        let page_nav_h = 26.0;
        let bottom_chrome_h = status_h + page_nav_h;

        let toolbox_w = 38.0;
        let palette_w = 24.0;
        let docker_w = if self.docker_open { 260.0 } else { 0.0 };

        let body_y0 = top_chrome_h;
        let body_y1 = (h - bottom_chrome_h).max(body_y0 + 100.0);

        let canvas_x0 = toolbox_w;
        let canvas_x1 = (w - palette_w - docker_w).max(canvas_x0 + 100.0);
        let palette_x0 = canvas_x1;
        let palette_x1 = palette_x0 + palette_w;

        let right_docker = if self.docker_open { Some(rect(palette_x1, body_y0, w, body_y1)) } else { None };

        WorkspaceLayoutRects {
            window_width: self.window_width,
            window_height: self.window_height,
            menu_bar: rect(0.0, 0.0, w, menu_h),
            standard_toolbar: rect(0.0, menu_h, w, menu_h + std_tb_h),
            property_bar: rect(0.0, menu_h + std_tb_h, w, top_chrome_h),
            left_toolbox: rect(0.0, body_y0, toolbox_w, body_y1),
            canvas_viewport: rect(canvas_x0, body_y0, canvas_x1, body_y1),
            right_palette_bar: rect(palette_x0, body_y0, palette_x1, body_y1),
            right_docker,
            page_navigator_bar: rect(0.0, body_y1, w, body_y1 + page_nav_h),
            status_bar: rect(0.0, body_y1 + page_nav_h, w, h),
        }
    }

    /// Builds the complete declarative [`WorkspaceFrameSnapshot`] for the current frame.
    #[must_use]
    pub fn snapshot(&self) -> WorkspaceFrameSnapshot {
        let layout = self.compute_layout();
        let menu_bar = MenuBarCategory::ALL.iter().map(|m| m.label().to_owned()).collect();
        let property_bar = self.engine.property_bar_schema();

        let mut palette_cells = Vec::with_capacity(self.engine.document.palette.swatches.len() + 1);
        palette_cells.push(PaletteCell::NoColor);
        for sw in &self.engine.document.palette.swatches {
            palette_cells.push(PaletteCell::Color { name: Some(sw.name.clone()), color: sw.color.clone() });
        }

        let can_delete_page = self.engine.document.pages.len() > 1;
        let mut page_tabs = Vec::with_capacity(self.engine.document.pages.len() + 2);
        for (idx, page) in self.engine.document.pages.iter().enumerate() {
            page_tabs.push(PageNavigatorTab {
                target: ActivePageTarget::Page(idx),
                label: page.name.clone(),
                active: self.engine.active_target == ActivePageTarget::Page(idx),
                can_delete: can_delete_page,
            });
        }
        page_tabs.push(PageNavigatorTab {
            target: ActivePageTarget::MasterPage,
            label: "Master Page".to_owned(),
            active: self.engine.active_target == ActivePageTarget::MasterPage,
            can_delete: false,
        });
        page_tabs.push(PageNavigatorTab {
            target: ActivePageTarget::Desktop,
            label: "Desktop".to_owned(),
            active: self.engine.active_target == ActivePageTarget::Desktop,
            can_delete: false,
        });

        let mut object_tree = Vec::new();
        if let Ok(layers) = self.engine.document.effective_layers_for_page(self.engine.current_page_index(), false) {
            for layer in layers {
                let is_master = matches!(layer.role, LayerRole::MasterAllPages | LayerRole::MasterOddPages | LayerRole::MasterEvenPages);
                let is_desktop = layer.role == LayerRole::Desktop;
                for obj in &layer.objects {
                    let object_label = obj.name.clone().unwrap_or_else(|| {
                        match &obj.kind {
                            coralcraft_doc::ObjectKind::Rect(_) => "Rectangle",
                            coralcraft_doc::ObjectKind::Ellipse(_) => "Ellipse",
                            coralcraft_doc::ObjectKind::Polygon(p) if p.inner_radius.is_some() => "Star",
                            coralcraft_doc::ObjectKind::Polygon(_) => "Polygon",
                            coralcraft_doc::ObjectKind::Path(_) => "Curve",
                            coralcraft_doc::ObjectKind::ArtisticText { .. } => "Artistic Text",
                            coralcraft_doc::ObjectKind::Group { .. } => "Group",
                            coralcraft_doc::ObjectKind::Unsupported(_) => "Unsupported Object",
                        }
                        .to_owned()
                    });
                    object_tree.push(ObjectTreeEntry {
                        layer_name: layer.name.clone(),
                        is_master,
                        is_desktop,
                        object_id: obj.id,
                        object_label,
                        selected: self.engine.selection.contains(&obj.id),
                        visible: obj.visible,
                        locked: obj.locked,
                    });
                }
            }
        }

        let selected_nodes = self.engine.selected_nodes();
        let selection_bounds = if selected_nodes.is_empty() { None } else { Some(combined_selection_bounds(&selected_nodes)) };

        WorkspaceFrameSnapshot {
            layout,
            menu_bar,
            active_tool: self.engine.tool_state.active_tool,
            can_undo: self.engine.can_undo(),
            can_redo: self.engine.can_redo(),
            property_bar,
            palette_cells,
            page_tabs,
            status_bar: self.engine.status_bar_snapshot(),
            docker_open: self.docker_open,
            active_docker_tab: self.active_docker_tab,
            object_tree,
            selection_bounds,
        }
    }

    /// Dispatches a [`UiAction`] through `coralcraft-tools` and `coralcraft-engine`.
    pub fn dispatch(&mut self, action: UiAction) {
        match action {
            UiAction::SelectTool(tool) => {
                self.engine.tool_state.active_tool = tool;
                self.engine.tool_state.gesture = None;
            }
            UiAction::Shortcut(shortcut) => {
                self.engine.handle_shortcut(shortcut);
            }
            UiAction::CanvasPointerDown { pt, shift, alt } => {
                self.engine.pointer_down(pt, shift, alt);
            }
            UiAction::CanvasPointerMove { pt, shift, alt } => {
                self.engine.pointer_move(pt, shift, alt);
            }
            UiAction::CanvasPointerUp { pt, shift, alt } => {
                self.engine.pointer_up(pt, shift, alt);
            }
            UiAction::PaletteLeftClick { cell_index } => {
                if let Some(color) = self.palette_color_at(cell_index) {
                    self.engine.execute(EngineCommand::ApplyPaletteSwatch { channel: PaletteChannel::Fill, color });
                }
            }
            UiAction::PaletteRightClick { cell_index } => {
                if let Some(color) = self.palette_color_at(cell_index) {
                    self.engine.execute(EngineCommand::ApplyPaletteSwatch { channel: PaletteChannel::Outline, color });
                }
            }
            UiAction::ActivatePageTarget(target) => {
                self.engine.set_active_target(target);
            }
            UiAction::AddPage => {
                self.engine.execute(EngineCommand::AddPageAfterActive);
            }
            UiAction::DuplicatePage { page_index } => {
                self.engine.execute(EngineCommand::DuplicatePage { page_index });
            }
            UiAction::RenamePage { page_index, name } => {
                self.engine.execute(EngineCommand::RenamePage { page_index, name });
            }
            UiAction::DeletePage { page_index } => {
                self.engine.execute(EngineCommand::DeletePage { page_index });
            }
            UiAction::CommitPageSetup { width_pt, height_pt, units } => {
                self.engine.execute(EngineCommand::SetPageSetup { width_pt, height_pt, units });
            }
            UiAction::CommitRectCorners { corner_kind, radii, corners_locked } => {
                self.engine.execute(EngineCommand::UpdateRectCorners { corner_kind, radii, corners_locked });
            }
            UiAction::CommitEllipseParameters { mode, start_deg, end_deg } => {
                self.engine.execute(EngineCommand::UpdateEllipseParameters { mode, start_deg, end_deg });
            }
            UiAction::CommitPolygonParameters { mode, points, inner_radius_pt } => {
                self.engine.execute(EngineCommand::UpdatePolygonParameters { mode, points, inner_radius_pt });
            }
            UiAction::SelectObjectFromTree { object_id, additive } => {
                if additive {
                    if let Some(pos) = self.engine.selection.iter().position(|&id| id == object_id) {
                        self.engine.selection.remove(pos);
                    } else {
                        self.engine.selection.push(object_id);
                    }
                } else {
                    self.engine.selection = vec![object_id];
                }
            }
            UiAction::SetDockerOpen(open) => {
                self.docker_open = open;
            }
            UiAction::SelectDockerTab(tab) => {
                self.active_docker_tab = tab;
                self.docker_open = true;
            }
            UiAction::ScrollPalette { offset_px } => {
                self.palette_scroll_px = offset_px.max(0);
            }
            UiAction::Undo => {
                self.engine.undo();
            }
            UiAction::Redo => {
                self.engine.redo();
            }
            UiAction::DeleteSelection => {
                self.engine.execute(EngineCommand::DeleteSelection);
            }
        }
    }

    /// Renders the currently active page through `coralcraft-render` into a [`RenderOutput`].
    pub fn render_active_page(&self, dpi: f64) -> Result<RenderOutput, RenderError> {
        render_page(
            &self.engine.document,
            self.engine.current_page_index(),
            &RenderOpts { scale: dpi / 72.0, transparent_background: false, export_mode: false },
        )
    }

    fn palette_color_at(&self, cell_index: usize) -> Option<Option<Color>> {
        if cell_index == 0 { Some(None) } else { self.engine.document.palette.swatches.get(cell_index - 1).map(|s| Some(s.color.clone())) }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use coralcraft_tools::PropertyContext;

    #[test]
    fn classic_coreldraw_workspace_layout_and_action_pipeline_acceptance() {
        let mut ws = DesktopWorkspace::default();
        let snap0 = ws.snapshot();

        // 1. Fixed 11-item top Menu Bar order
        assert_eq!(snap0.menu_bar, vec!["File", "Edit", "View", "Layout", "Object", "Effects", "Bitmaps", "Text", "Tools", "Window", "Help"]);
        // Empty selection shows Page Setup on Property Bar
        assert_eq!(snap0.property_bar.context, PropertyContext::PageSetup);
        // Top cell of right vertical palette is `[X]` NoColor
        assert_eq!(snap0.palette_cells[0], PaletteCell::NoColor);
        // Bottom Page Navigator contains Page 1, Master Page, Desktop
        assert_eq!(snap0.page_tabs.len(), 3);
        assert_eq!(snap0.page_tabs[0].label, "Page 1");
        assert!(!snap0.page_tabs[0].can_delete);
        assert_eq!(snap0.page_tabs[1].label, "Master Page");
        assert_eq!(snap0.page_tabs[2].label, "Desktop");

        // 2. Collapsing the right docker column returns its 260 px width to the central canvas
        let open_canvas_w = snap0.layout.canvas_viewport.width();
        ws.dispatch(UiAction::SetDockerOpen(false));
        let closed_snap = ws.snapshot();
        assert!(closed_snap.layout.right_docker.is_none());
        assert!((closed_snap.layout.canvas_viewport.width() - (open_canvas_w + 260.0)).abs() < 1e-6);
        ws.dispatch(UiAction::SetDockerOpen(true));

        // 3. Arming F6 (Rectangle), dragging on canvas, and clicking palette swatches
        ws.dispatch(UiAction::Shortcut(KeyShortcut::F6));
        assert_eq!(ws.snapshot().property_bar.context, PropertyContext::Rectangle);

        ws.dispatch(UiAction::CanvasPointerDown { pt: Point::new(40.0, 50.0).unwrap(), shift: false, alt: false });
        ws.dispatch(UiAction::CanvasPointerUp { pt: Point::new(140.0, 120.0).unwrap(), shift: false, alt: false });

        let snap1 = ws.snapshot();
        assert_eq!(snap1.object_tree.len(), 1);
        assert!(snap1.object_tree[0].selected);
        assert_eq!(snap1.status_bar.selection_summary, "Rectangle on Layer");

        // Left-click swatch #1 sets Fill; right-click swatch #2 sets Outline; left-click #0 `[X]` clears Fill
        ws.dispatch(UiAction::PaletteLeftClick { cell_index: 1 });
        assert!(ws.snapshot().status_bar.fill_chip.is_some());
        ws.dispatch(UiAction::PaletteRightClick { cell_index: 2 });
        assert!(ws.snapshot().status_bar.outline_chip.is_some());
        ws.dispatch(UiAction::PaletteLeftClick { cell_index: 0 });
        assert!(ws.snapshot().status_bar.fill_chip.is_none());

        // 4. Add a page (`+`), verify `can_delete` becomes true on numbered pages
        ws.dispatch(UiAction::AddPage);
        let snap2 = ws.snapshot();
        assert_eq!(snap2.page_tabs.len(), 4);
        assert!(snap2.page_tabs[0].can_delete);
        assert!(snap2.page_tabs[1].can_delete);

        // 5. Render active page frame
        let rendered = ws.render_active_page(72.0).unwrap();
        assert!(rendered.surface.width > 0 && rendered.surface.height > 0);
        assert!(!rendered.png_bytes.is_empty());
    }
}
