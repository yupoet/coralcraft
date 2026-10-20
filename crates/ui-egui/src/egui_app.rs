//! Native `egui` 0.36 desktop & Web application shell (`CoralCraftApp`, L6).
//!
//! Renders the classic 7-panel CorelDRAW desktop workspace:
//! 1. Top Menu Bar (`File`, `Edit`, `View`, `Layout`, `Object`, `Effects`, `Bitmaps`, `Text`, `Tools`, `Window`, `Help`)
//! 2. Standard Toolbar (`New`, `Open`, `Save`, `Undo`, `Redo`, `Cut`, `Copy`, `Paste`, `Delete`, `Zoom`, `Snap`, `Docker`)
//! 3. Context-Sensitive Property Bar (driven by [`PropertyBarSchema`](coralcraft_tools::PropertyBarSchema))
//! 4. Left Vertical Toolbox (10 classic tools: `Pick`, `Shape`, `Crop`, `Zoom`, `Freehand / Bezier`, `Rectangle`, `Ellipse`, `Polygon / Star`, `Artistic Text`, `Interactive Fill`)
//! 5. Central Multi-Page Pasteboard & Interactive Vector Canvas (cached page raster texture + 60 fps `egui::Painter` vector overlays)
//! 6. Right Vertical Color Palette Strip (`[X]` No-Color + document swatches; left-click = Fill, right-click = Outline)
//! 7. Right Collapsible Docker (`Objects` hierarchical layer/object tree & `Properties` inspector)
//! 8. Bottom Page Navigator (`+ Page`, `Page 1..N`, `Master Page`, `Desktop`) & Status Bar (`(x, y)`, selection summary, Fill/Outline chips)

use std::path::{Path, PathBuf};

use coralcraft_cdr::parse_document;
use coralcraft_color::{Color, FountainBlendMode, FountainStop};
use coralcraft_doc::{Document, FillStyle, FountainGeometry, OutlineStyle, TextCaseKind};
use coralcraft_engine::{ActivePageTarget, EditorEngine, EngineCommand};
use coralcraft_geom::{BooleanOp, CornerKind, EllipseMode, F64, Point};
use coralcraft_render::{export_dxf, export_eps, export_pdf, export_svg};
use coralcraft_tools::{
    AlignMode, CurveSubMode, GestureState, KeyShortcut, OriginPoint, PageOrientation, PolygonSubMode, PropertyControl, ToolKind, ZOrderOp,
};
use egui::{Color32, CornerRadius, Key, PointerButton, Pos2, Sense, Stroke, TextureHandle, TextureOptions, Ui, pos2, vec2};

use crate::{DesktopWorkspace, DockerTab, MenuBarCategory, PaletteCell, UiAction};

/// Callback for native Open File dialog (`rfd`).
pub type PickOpenFn = Box<dyn FnMut() -> Option<PathBuf>>;
/// Callback for native Save File dialog (`rfd`), taking a suggested file name.
pub type PickSaveFn = Box<dyn FnMut(&str) -> Option<PathBuf>>;
/// Callback for reading text from the OS clipboard (`arboard`).
pub type ClipboardGetFn = Box<dyn FnMut() -> Option<String>>;
/// Callback for writing text to the OS clipboard (`arboard`).
pub type ClipboardSetFn = Box<dyn FnMut(&str) -> Result<(), String>>;

/// Host platform services injected by the desktop (`apps/coralcraft`) or test harness.
#[derive(Default)]
pub struct Services {
    pub pick_open: Option<PickOpenFn>,
    pub pick_save: Option<PickSaveFn>,
    pub clipboard_get_text: Option<ClipboardGetFn>,
    pub clipboard_set_text: Option<ClipboardSetFn>,
}

/// Cached rasterized page texture keyed by `(doc_revision, active_target, dpi_bucket)`.
struct CachedPageTexture {
    revision: u64,
    target: ActivePageTarget,
    dpi_bucket: u32,
    texture: TextureHandle,
}

/// Native `egui` 0.36 application state for CoralCraft.
pub struct CoralCraftApp {
    pub workspace: DesktopWorkspace,
    pub services: Services,
    pub current_file_path: Option<PathBuf>,
    pub status_message: String,
    pub show_about_modal: bool,
    pub show_guidelines: bool,
    pub cursor_page_pt: Option<Point>,
    pub doc_revision: u64,
    pub quit_requested: bool,
    cached_page: Option<CachedPageTexture>,
    last_pan_pos: Option<Pos2>,
}

impl Default for CoralCraftApp {
    fn default() -> Self {
        Self::new_with_welcome_artwork(Services::default())
    }
}

impl CoralCraftApp {
    /// Creates a new `CoralCraftApp` for `document` with the provided platform `services`.
    #[must_use]
    pub fn new(document: Document, services: Services) -> Self {
        Self {
            workspace: DesktopWorkspace::new(document),
            services,
            current_file_path: None,
            status_message: "Ready".to_owned(),
            show_about_modal: false,
            show_guidelines: true,
            cursor_page_pt: None,
            doc_revision: 1,
            quit_requested: false,
            cached_page: None,
            last_pan_pos: None,
        }
    }

    /// Creates a `CoralCraftApp` pre-populated with the clean-room starter vector illustration.
    #[must_use]
    pub fn new_with_welcome_artwork(services: Services) -> Self {
        let session = crate::InteractiveGuiSession::new_with_welcome_artwork();
        Self {
            workspace: session.workspace,
            services,
            current_file_path: None,
            status_message: "Welcome to CoralCraft Studio".to_owned(),
            show_about_modal: false,
            show_guidelines: true,
            cursor_page_pt: None,
            doc_revision: 1,
            quit_requested: false,
            cached_page: None,
            last_pan_pos: None,
        }
    }

    /// Dispatches a [`UiAction`] and increments `doc_revision` so the canvas texture refreshes when needed.
    pub fn dispatch(&mut self, action: UiAction) {
        self.workspace.dispatch(action);
        self.doc_revision = self.doc_revision.wrapping_add(1);
    }

    /// Executes an [`EngineCommand`] transactionally and updates `status_message` on error.
    pub fn execute_cmd(&mut self, cmd: EngineCommand) {
        match self.workspace.engine.try_execute(cmd) {
            Ok(()) => {
                self.doc_revision = self.doc_revision.wrapping_add(1);
            }
            Err(e) => {
                self.status_message = format!("Command error: {e:?}");
            }
        }
    }

    /// Loads a document from `path` (`.coralcraft`, `.cdr`, `.cmx`, `.svg`, `.dxf`).
    pub fn open_from_path(&mut self, path: &Path) -> Result<(), String> {
        let bytes = std::fs::read(path).map_err(|e| format!("failed to read `{}`: {e}", path.display()))?;
        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
        let doc = if ext == "svg" {
            let text = std::str::from_utf8(&bytes).map_err(|e| format!("invalid UTF-8 in SVG: {e}"))?;
            coralcraft_doc::import_svg(text).map_err(|e| format!("SVG import error: {e}"))?
        } else if ext == "dxf" {
            let text = std::str::from_utf8(&bytes).map_err(|e| format!("invalid UTF-8 in DXF: {e}"))?;
            coralcraft_doc::import_dxf(text).map_err(|e| format!("DXF import error: {e}"))?
        } else if let Ok(doc) = coralcraft_doc::load_bytes(&bytes) {
            doc
        } else {
            let outcome = parse_document(&bytes).map_err(|e| format!("CDR/CMX parse error: {e}"))?;
            outcome.document
        };
        self.workspace.engine = EditorEngine::new(doc);
        self.current_file_path = Some(path.to_path_buf());
        self.doc_revision = self.doc_revision.wrapping_add(1);
        self.status_message = format!("Opened {}", path.display());
        Ok(())
    }

    /// Saves or exports the current document to `path` based on its file extension.
    pub fn save_to_path(&mut self, path: &Path) -> Result<(), String> {
        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("coralcraft").to_ascii_lowercase();
        let bytes: Vec<u8> = match ext.as_str() {
            "svg" => export_svg(&self.workspace.engine.document, self.workspace.engine.current_page_index()).map_err(|e| e.to_string())?.into_bytes(),
            "pdf" => export_pdf(&self.workspace.engine.document).map_err(|e| e.to_string())?,
            "eps" => export_eps(&self.workspace.engine.document, self.workspace.engine.current_page_index()).map_err(|e| e.to_string())?.into_bytes(),
            "dxf" => export_dxf(&self.workspace.engine.document, self.workspace.engine.current_page_index()).map_err(|e| e.to_string())?.into_bytes(),
            "png" => self.workspace.render_active_page(96.0).map_err(|e| e.to_string())?.png_bytes,
            "cdr" => coralcraft_cdr::write_cdr7(&self.workspace.engine.document).map_err(|e| e.to_string())?,
            "cmx" => coralcraft_cdr::write_cmx1(&self.workspace.engine.document).map_err(|e| e.to_string())?,
            "json" => coralcraft_doc::save_json(&self.workspace.engine.document).map_err(|e| e.to_string())?,
            _ => coralcraft_doc::save_zip(&self.workspace.engine.document).map_err(|e| e.to_string())?,
        };
        std::fs::write(path, &bytes).map_err(|e| format!("failed to write `{}`: {e}", path.display()))?;
        if matches!(ext.as_str(), "coralcraft" | "json") {
            self.current_file_path = Some(path.to_path_buf());
        }
        self.status_message = format!("Saved {} ({} bytes)", path.display(), bytes.len());
        Ok(())
    }

    /// Copies the currently selected objects to the system clipboard as JSON.
    pub fn copy_selection(&mut self) {
        let selected: Vec<coralcraft_doc::ObjectNode> = self.workspace.engine.selected_nodes().into_iter().cloned().collect();
        if selected.is_empty() {
            "Nothing selected to copy".clone_into(&mut self.status_message);
            return;
        }
        if let Ok(json) = serde_json::to_string(&selected) {
            if let Some(set_cb) = self.services.clipboard_set_text.as_mut() {
                match set_cb(&json) {
                    Ok(()) => self.status_message = format!("Copied {} object(s)", selected.len()),
                    Err(e) => self.status_message = format!("Clipboard error: {e}"),
                }
            } else {
                self.status_message = format!("Copied {} object(s)", selected.len());
            }
        }
    }

    /// Pastes objects from the system clipboard JSON into the active page.
    pub fn paste_clipboard(&mut self) {
        let Some(get_cb) = self.services.clipboard_get_text.as_mut() else {
            return;
        };
        let Some(text) = get_cb() else {
            return;
        };
        if let Ok(nodes) = serde_json::from_str::<Vec<coralcraft_doc::ObjectNode>>(&text) {
            let mut count = 0usize;
            for node in nodes {
                if self.workspace.engine.try_execute(EngineCommand::AddObject { name: node.name, kind: node.kind }).is_ok() {
                    if node.fill.is_some() {
                        let _ = self.workspace.engine.try_execute(EngineCommand::SetFill { fill: node.fill });
                    }
                    if node.outline.is_some() {
                        let _ = self.workspace.engine.try_execute(EngineCommand::SetOutline { outline: node.outline });
                    }
                    count = count.saturating_add(1);
                }
            }
            if count > 0 {
                self.doc_revision = self.doc_revision.wrapping_add(1);
                self.status_message = format!("Pasted {count} object(s)");
            }
        }
    }

    /// Renders the complete CoralCraft workspace into `ui`.
    pub fn ui(&mut self, ui: &mut Ui) {
        let rect = ui.max_rect();
        if rect.width() >= 320.0 && rect.height() >= 240.0 {
            self.workspace.window_width = rect.width().round() as u32;
            self.workspace.window_height = rect.height().round() as u32;
        }

        self.handle_keyboard_shortcuts(ui);

        let snap = self.workspace.snapshot();

        self.render_menu_bar(ui, &snap);
        self.render_standard_toolbar(ui, &snap);
        self.render_property_bar(ui, &snap);
        self.render_status_bar(ui, &snap);
        self.render_page_navigator_bar(ui, &snap);
        self.render_left_toolbox(ui, &snap);
        if self.workspace.docker_open {
            self.render_right_docker(ui, &snap);
        }
        self.render_right_palette_bar(ui, &snap);
        self.render_central_canvas(ui, &snap);

        if self.show_about_modal {
            egui::Window::new("About CoralCraft").collapsible(false).resizable(false).show(ui.ctx(), |ui| {
                ui.heading("CoralCraft Vector Studio");
                ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
                ui.label("Clean-room Rust vector illustration and multi-page layout application.");
                ui.label("Dual-licensed under MIT OR Apache-2.0.");
                ui.add_space(8.0);
                if ui.button("Close").clicked() {
                    self.show_about_modal = false;
                }
            });
        }
    }

    fn handle_keyboard_shortcuts(&mut self, ui: &Ui) {
        if ui.ctx().egui_wants_keyboard_input() {
            return;
        }
        let events = ui.input(|i| i.events.clone());
        for ev in events {
            let egui::Event::Key { key, pressed: true, modifiers, .. } = ev else {
                continue;
            };
            let cmd = modifiers.command || modifiers.ctrl;
            if cmd {
                match key {
                    Key::Z if modifiers.shift => self.dispatch(UiAction::Redo),
                    Key::Z => self.dispatch(UiAction::Undo),
                    Key::Y => self.dispatch(UiAction::Redo),
                    Key::G => self.dispatch(UiAction::GroupSelection),
                    Key::U => self.dispatch(UiAction::UngroupSelection),
                    Key::Q => self.dispatch(UiAction::ConvertToCurves),
                    Key::L => self.execute_cmd(EngineCommand::CombineSelection),
                    Key::K => self.execute_cmd(EngineCommand::BreakApartSelection),
                    Key::D => self.execute_cmd(EngineCommand::DuplicateSelection {
                        offset_x_pt: F64::new(12.0).unwrap_or(F64::ZERO),
                        offset_y_pt: F64::new(12.0).unwrap_or(F64::ZERO),
                    }),
                    Key::A => {
                        self.workspace.engine.select_all();
                        self.doc_revision = self.doc_revision.wrapping_add(1);
                    }
                    Key::C => self.copy_selection(),
                    Key::X => {
                        self.copy_selection();
                        self.dispatch(UiAction::DeleteSelection);
                    }
                    Key::V => self.paste_clipboard(),
                    Key::N => {
                        self.workspace = DesktopWorkspace::new(Document::new_a4_portrait());
                        self.current_file_path = None;
                        self.doc_revision = self.doc_revision.wrapping_add(1);
                        "Created new A4 document".clone_into(&mut self.status_message);
                    }
                    Key::O => self.trigger_open_dialog(),
                    Key::S => self.trigger_save_dialog(false),
                    _ => {}
                }
                continue;
            }

            match key {
                Key::Escape => self.dispatch(UiAction::Shortcut(KeyShortcut::Escape)),
                Key::F5 => self.dispatch(UiAction::Shortcut(KeyShortcut::F5)),
                Key::F6 => self.dispatch(UiAction::Shortcut(KeyShortcut::F6)),
                Key::F7 => self.dispatch(UiAction::Shortcut(KeyShortcut::F7)),
                Key::F8 => self.dispatch(UiAction::Shortcut(KeyShortcut::F8)),
                Key::F10 => self.dispatch(UiAction::Shortcut(KeyShortcut::F10)),
                Key::Z => self.dispatch(UiAction::Shortcut(KeyShortcut::Char('z'))),
                Key::Y => self.dispatch(UiAction::Shortcut(KeyShortcut::Char('y'))),
                Key::G => self.dispatch(UiAction::Shortcut(KeyShortcut::Char('g'))),
                Key::P => self.execute_cmd(EngineCommand::CenterSelectionToPage { horizontal: true, vertical: true }),
                Key::Delete | Key::Backspace => self.dispatch(UiAction::DeleteSelection),
                Key::PageUp if modifiers.shift => self.dispatch(UiAction::ReorderSelection { op: ZOrderOp::ToFront }),
                Key::PageDown if modifiers.shift => self.dispatch(UiAction::ReorderSelection { op: ZOrderOp::ToBack }),
                Key::PageUp => self.dispatch(UiAction::ReorderSelection { op: ZOrderOp::ForwardOne }),
                Key::PageDown => self.dispatch(UiAction::ReorderSelection { op: ZOrderOp::BackwardOne }),
                _ => {}
            }
        }
    }

    fn trigger_open_dialog(&mut self) {
        let picked = self.services.pick_open.as_mut().and_then(|pick| pick());
        if let Some(path) = picked
            && let Err(e) = self.open_from_path(&path)
        {
            self.status_message = e;
        }
    }

    fn trigger_save_dialog(&mut self, save_as: bool) {
        if !save_as && let Some(path) = self.current_file_path.clone() {
            if let Err(e) = self.save_to_path(&path) {
                self.status_message = e;
            }
            return;
        }
        let suggested = self.current_file_path.as_ref().and_then(|p| p.file_name()).and_then(|s| s.to_str()).unwrap_or("drawing.coralcraft");
        let picked = self.services.pick_save.as_mut().and_then(|pick| pick(suggested));
        if let Some(path) = picked
            && let Err(e) = self.save_to_path(&path)
        {
            self.status_message = e;
        }
    }

    fn trigger_export_dialog(&mut self, suggested: &str) {
        let picked = self.services.pick_save.as_mut().and_then(|pick| pick(suggested));
        if let Some(path) = picked
            && let Err(e) = self.save_to_path(&path)
        {
            self.status_message = e;
        }
    }

    fn render_menu_bar(&mut self, ui: &mut Ui, snap: &crate::WorkspaceFrameSnapshot) {
        egui::Panel::top("menu_bar").exact_size(24.0).show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                for category in MenuBarCategory::ALL {
                    ui.menu_button(category.label(), |ui| match category {
                        MenuBarCategory::File => {
                            if ui.button("New Document (Ctrl+N)").clicked() {
                                self.workspace = DesktopWorkspace::new(Document::new_a4_portrait());
                                self.current_file_path = None;
                                self.doc_revision = self.doc_revision.wrapping_add(1);
                                "Created new A4 document".clone_into(&mut self.status_message);
                                ui.close();
                            }
                            if ui.button("Open... (Ctrl+O)").clicked() {
                                self.trigger_open_dialog();
                                ui.close();
                            }
                            if ui.button("Save (Ctrl+S)").clicked() {
                                self.trigger_save_dialog(false);
                                ui.close();
                            }
                            if ui.button("Save As...").clicked() {
                                self.trigger_save_dialog(true);
                                ui.close();
                            }
                            ui.separator();
                            if ui.button("Export SVG...").clicked() {
                                self.trigger_export_dialog("export.svg");
                                ui.close();
                            }
                            if ui.button("Export PDF...").clicked() {
                                self.trigger_export_dialog("export.pdf");
                                ui.close();
                            }
                            if ui.button("Export EPS...").clicked() {
                                self.trigger_export_dialog("export.eps");
                                ui.close();
                            }
                            if ui.button("Export DXF...").clicked() {
                                self.trigger_export_dialog("export.dxf");
                                ui.close();
                            }
                            if ui.button("Export PNG...").clicked() {
                                self.trigger_export_dialog("export.png");
                                ui.close();
                            }
                            if ui.button("Export CorelDRAW CDR7...").clicked() {
                                self.trigger_export_dialog("export.cdr");
                                ui.close();
                            }
                            if ui.button("Export Corel CMX1...").clicked() {
                                self.trigger_export_dialog("export.cmx");
                                ui.close();
                            }
                            ui.separator();
                            if ui.button("Quit").clicked() {
                                self.quit_requested = true;
                                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                                ui.close();
                            }
                        }
                        MenuBarCategory::Edit => {
                            if ui.add_enabled(snap.can_undo, egui::Button::new("Undo (Ctrl+Z)")).clicked() {
                                self.dispatch(UiAction::Undo);
                                ui.close();
                            }
                            if ui.add_enabled(snap.can_redo, egui::Button::new("Redo (Ctrl+Y)")).clicked() {
                                self.dispatch(UiAction::Redo);
                                ui.close();
                            }
                            ui.separator();
                            if ui.button("Cut (Ctrl+X)").clicked() {
                                self.copy_selection();
                                self.dispatch(UiAction::DeleteSelection);
                                ui.close();
                            }
                            if ui.button("Copy (Ctrl+C)").clicked() {
                                self.copy_selection();
                                ui.close();
                            }
                            if ui.button("Paste (Ctrl+V)").clicked() {
                                self.paste_clipboard();
                                ui.close();
                            }
                            if ui.button("Duplicate (Ctrl+D)").clicked() {
                                self.execute_cmd(EngineCommand::DuplicateSelection {
                                    offset_x_pt: F64::new(12.0).unwrap_or(F64::ZERO),
                                    offset_y_pt: F64::new(12.0).unwrap_or(F64::ZERO),
                                });
                                ui.close();
                            }
                            if ui.button("Step and Repeat (3x)...").clicked() {
                                self.execute_cmd(EngineCommand::StepAndRepeat {
                                    count: 3,
                                    offset_x_pt: F64::new(24.0).unwrap_or(F64::ZERO),
                                    offset_y_pt: F64::new(0.0).unwrap_or(F64::ZERO),
                                });
                                ui.close();
                            }
                            ui.separator();
                            if ui.button("Select All (Ctrl+A)").clicked() {
                                self.workspace.engine.select_all();
                                self.doc_revision = self.doc_revision.wrapping_add(1);
                                ui.close();
                            }
                            if ui.button("Delete (Del)").clicked() {
                                self.dispatch(UiAction::DeleteSelection);
                                ui.close();
                            }
                        }
                        MenuBarCategory::View => {
                            if ui.button("Zoom In (150%)").clicked() {
                                let z = (self.workspace.engine.zoom.get() * 1.5).clamp(0.1, 16.0);
                                if let Ok(zoom) = F64::new(z) {
                                    self.workspace.engine.zoom = zoom;
                                    self.doc_revision = self.doc_revision.wrapping_add(1);
                                }
                                ui.close();
                            }
                            if ui.button("Zoom Out (67%)").clicked() {
                                let z = (self.workspace.engine.zoom.get() / 1.5).clamp(0.1, 16.0);
                                if let Ok(zoom) = F64::new(z) {
                                    self.workspace.engine.zoom = zoom;
                                    self.doc_revision = self.doc_revision.wrapping_add(1);
                                }
                                ui.close();
                            }
                            if ui.button("Zoom 100%").clicked() {
                                self.workspace.engine.zoom = F64::ONE;
                                self.workspace.engine.pan_pt = Point::ZERO;
                                self.doc_revision = self.doc_revision.wrapping_add(1);
                                ui.close();
                            }
                            ui.separator();
                            ui.checkbox(&mut self.show_guidelines, "Show Guidelines");
                            if ui.button("Clear Guidelines").clicked() {
                                self.dispatch(UiAction::ClearGuidelines);
                                ui.close();
                            }
                        }
                        MenuBarCategory::Layout => {
                            if ui.button("Insert Page After Active").clicked() {
                                self.dispatch(UiAction::AddPage);
                                ui.close();
                            }
                            if ui.button("Duplicate Active Page").clicked() {
                                let page_index = self.workspace.engine.current_page_index();
                                self.dispatch(UiAction::DuplicatePage { page_index });
                                ui.close();
                            }
                            if ui.add_enabled(self.workspace.engine.document.pages.len() > 1, egui::Button::new("Delete Active Page")).clicked() {
                                let page_index = self.workspace.engine.current_page_index();
                                self.dispatch(UiAction::DeletePage { page_index });
                                ui.close();
                            }
                            ui.separator();
                            if ui.button("Switch to Master Page").clicked() {
                                self.dispatch(UiAction::ActivatePageTarget(ActivePageTarget::MasterPage));
                                ui.close();
                            }
                            if ui.button("Switch to Desktop Layer").clicked() {
                                self.dispatch(UiAction::ActivatePageTarget(ActivePageTarget::Desktop));
                                ui.close();
                            }
                        }
                        MenuBarCategory::Object => {
                            if ui.button("Group (Ctrl+G)").clicked() {
                                self.dispatch(UiAction::GroupSelection);
                                ui.close();
                            }
                            if ui.button("Ungroup (Ctrl+U)").clicked() {
                                self.dispatch(UiAction::UngroupSelection);
                                ui.close();
                            }
                            if ui.button("Ungroup All").clicked() {
                                self.execute_cmd(EngineCommand::UngroupAllSelection);
                                ui.close();
                            }
                            ui.separator();
                            if ui.button("Convert to Curves (Ctrl+Q)").clicked() {
                                self.dispatch(UiAction::ConvertToCurves);
                                ui.close();
                            }
                            if ui.button("Convert Outline to Object (Ctrl+Shift+Q)").clicked() {
                                self.execute_cmd(EngineCommand::ConvertOutlineToObject);
                                ui.close();
                            }
                            if ui.button("Combine (Ctrl+L)").clicked() {
                                self.execute_cmd(EngineCommand::CombineSelection);
                                ui.close();
                            }
                            if ui.button("Break Apart (Ctrl+K)").clicked() {
                                self.execute_cmd(EngineCommand::BreakApartSelection);
                                ui.close();
                            }
                            ui.separator();
                            ui.menu_button("Shaping (Boolean)", |ui| {
                                if ui.button("Weld (Union)").clicked() {
                                    self.execute_cmd(EngineCommand::BooleanSelection { op: BooleanOp::Weld });
                                    ui.close();
                                }
                                if ui.button("Trim (Subtract)").clicked() {
                                    self.execute_cmd(EngineCommand::BooleanSelection { op: BooleanOp::Trim });
                                    ui.close();
                                }
                                if ui.button("Intersect").clicked() {
                                    self.execute_cmd(EngineCommand::BooleanSelection { op: BooleanOp::Intersect });
                                    ui.close();
                                }
                                if ui.button("Exclude (XOR)").clicked() {
                                    self.execute_cmd(EngineCommand::ExcludeSelection);
                                    ui.close();
                                }
                                if ui.button("Divide").clicked() {
                                    self.execute_cmd(EngineCommand::DivideSelection);
                                    ui.close();
                                }
                                if ui.button("Create Boundary").clicked() {
                                    self.execute_cmd(EngineCommand::CreateBoundary);
                                    ui.close();
                                }
                            });
                            ui.menu_button("Order", |ui| {
                                if ui.button("To Front of Layer (Shift+PgUp)").clicked() {
                                    self.dispatch(UiAction::ReorderSelection { op: ZOrderOp::ToFront });
                                    ui.close();
                                }
                                if ui.button("To Back of Layer (Shift+PgDn)").clicked() {
                                    self.dispatch(UiAction::ReorderSelection { op: ZOrderOp::ToBack });
                                    ui.close();
                                }
                                if ui.button("Forward One (PgUp)").clicked() {
                                    self.dispatch(UiAction::ReorderSelection { op: ZOrderOp::ForwardOne });
                                    ui.close();
                                }
                                if ui.button("Back One (PgDn)").clicked() {
                                    self.dispatch(UiAction::ReorderSelection { op: ZOrderOp::BackwardOne });
                                    ui.close();
                                }
                            });
                            ui.menu_button("Align and Distribute", |ui| {
                                for (label, mode) in [
                                    ("Align Left (L)", AlignMode::Left),
                                    ("Align Center Horizontally (C)", AlignMode::CenterHorizontal),
                                    ("Align Right (R)", AlignMode::Right),
                                    ("Align Top (T)", AlignMode::Top),
                                    ("Align Center Vertically (E)", AlignMode::CenterVertical),
                                    ("Align Bottom (B)", AlignMode::Bottom),
                                    ("Center to Page (P)", AlignMode::CenterToPage),
                                ] {
                                    if ui.button(label).clicked() {
                                        self.dispatch(UiAction::AlignSelection { mode });
                                        ui.close();
                                    }
                                }
                            });
                            ui.separator();
                            if ui.button("Unlock All Objects").clicked() {
                                self.execute_cmd(EngineCommand::UnlockAllObjects);
                                ui.close();
                            }
                        }
                        MenuBarCategory::Effects => {
                            if ui.button("Add Outside Contour (6 pt)").clicked() {
                                if let (Some(&object_id), Ok(offset_pt)) = (self.workspace.engine.selection.first(), F64::new(6.0)) {
                                    self.execute_cmd(EngineCommand::ApplyContour { object_id, steps: 1, offset_pt, inside: false });
                                }
                                ui.close();
                            }
                            if ui.button("Add Extrude / Drop Shadow").clicked() {
                                if let (Some(&object_id), Ok(depth_pt), Ok(angle_deg)) =
                                    (self.workspace.engine.selection.first(), F64::new(12.0), F64::new(45.0))
                                {
                                    self.execute_cmd(EngineCommand::AddExtrude { object_id, depth_pt, angle_deg });
                                }
                                ui.close();
                            }
                            if ui.button("Clear Effects").clicked() {
                                if let Some(&object_id) = self.workspace.engine.selection.first() {
                                    self.execute_cmd(EngineCommand::ClearEffects { object_id });
                                }
                                ui.close();
                            }
                        }
                        MenuBarCategory::Bitmaps => {
                            if ui.button("Invert Selection Colors").clicked() {
                                self.execute_cmd(EngineCommand::ConvertSelectionColors { invert: true, grayscale: false });
                                ui.close();
                            }
                            if ui.button("Convert Selection to Grayscale").clicked() {
                                self.execute_cmd(EngineCommand::ConvertSelectionColors { invert: false, grayscale: true });
                                ui.close();
                            }
                        }
                        MenuBarCategory::Text => {
                            if ui.button("Convert Artistic <-> Paragraph Text (Ctrl+F8)").clicked() {
                                if let Some(&object_id) = self.workspace.engine.selection.first() {
                                    self.execute_cmd(EngineCommand::ToggleTextKind { object_id });
                                }
                                ui.close();
                            }
                            if ui.button("UPPERCASE").clicked() {
                                if let Some(&object_id) = self.workspace.engine.selection.first() {
                                    self.execute_cmd(EngineCommand::ChangeTextCase { object_id, case_kind: TextCaseKind::Uppercase });
                                }
                                ui.close();
                            }
                            if ui.button("lowercase").clicked() {
                                if let Some(&object_id) = self.workspace.engine.selection.first() {
                                    self.execute_cmd(EngineCommand::ChangeTextCase { object_id, case_kind: TextCaseKind::Lowercase });
                                }
                                ui.close();
                            }
                            if ui.button("Title Case").clicked() {
                                if let Some(&object_id) = self.workspace.engine.selection.first() {
                                    self.execute_cmd(EngineCommand::ChangeTextCase { object_id, case_kind: TextCaseKind::TitleCase });
                                }
                                ui.close();
                            }
                        }
                        MenuBarCategory::Tools => {
                            if ui.button("Objects Docker").clicked() {
                                self.dispatch(UiAction::SelectDockerTab(DockerTab::Objects));
                                ui.close();
                            }
                            if ui.button("Properties Docker").clicked() {
                                self.dispatch(UiAction::SelectDockerTab(DockerTab::Properties));
                                ui.close();
                            }
                        }
                        MenuBarCategory::Window => {
                            if ui.button(if self.workspace.docker_open { "Hide Right Docker" } else { "Show Right Docker" }).clicked() {
                                self.dispatch(UiAction::SetDockerOpen(!self.workspace.docker_open));
                                ui.close();
                            }
                            if ui.button("Reset Workspace").clicked() {
                                self.workspace.docker_open = true;
                                self.workspace.active_docker_tab = DockerTab::Objects;
                                self.workspace.engine.zoom = F64::ONE;
                                self.workspace.engine.pan_pt = Point::ZERO;
                                self.doc_revision = self.doc_revision.wrapping_add(1);
                                ui.close();
                            }
                        }
                        MenuBarCategory::Help => {
                            if ui.button("About CoralCraft").clicked() {
                                self.show_about_modal = true;
                                ui.close();
                            }
                        }
                    });
                }
            });
        });
    }

    fn render_standard_toolbar(&mut self, ui: &mut Ui, snap: &crate::WorkspaceFrameSnapshot) {
        egui::Panel::top("standard_toolbar").exact_size(32.0).show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                if ui.button("New").on_hover_text("New Document (Ctrl+N)").clicked() {
                    self.workspace = DesktopWorkspace::new(Document::new_a4_portrait());
                    self.current_file_path = None;
                    self.doc_revision = self.doc_revision.wrapping_add(1);
                }
                if ui.button("Open").on_hover_text("Open Document (Ctrl+O)").clicked() {
                    self.trigger_open_dialog();
                }
                if ui.button("Save").on_hover_text("Save Document (Ctrl+S)").clicked() {
                    self.trigger_save_dialog(false);
                }
                ui.separator();
                if ui.add_enabled(snap.can_undo, egui::Button::new("Undo")).on_hover_text("Undo (Ctrl+Z)").clicked() {
                    self.dispatch(UiAction::Undo);
                }
                if ui.add_enabled(snap.can_redo, egui::Button::new("Redo")).on_hover_text("Redo (Ctrl+Y)").clicked() {
                    self.dispatch(UiAction::Redo);
                }
                ui.separator();
                if ui.button("Cut").on_hover_text("Cut Selection (Ctrl+X)").clicked() {
                    self.copy_selection();
                    self.dispatch(UiAction::DeleteSelection);
                }
                if ui.button("Copy").on_hover_text("Copy Selection (Ctrl+C)").clicked() {
                    self.copy_selection();
                }
                if ui.button("Paste").on_hover_text("Paste Clipboard (Ctrl+V)").clicked() {
                    self.paste_clipboard();
                }
                if ui.button("Delete").on_hover_text("Delete Selection (Del)").clicked() {
                    self.dispatch(UiAction::DeleteSelection);
                }
                ui.separator();
                let zoom_pct = (self.workspace.engine.zoom.get() * 100.0).round() as i32;
                ui.label(format!("Zoom: {zoom_pct}%"));
                if ui.small_button("-").on_hover_text("Zoom Out").clicked() {
                    let z = (self.workspace.engine.zoom.get() / 1.25).clamp(0.1, 16.0);
                    if let Ok(zoom) = F64::new(z) {
                        self.workspace.engine.zoom = zoom;
                        self.doc_revision = self.doc_revision.wrapping_add(1);
                    }
                }
                if ui.small_button("100%").on_hover_text("Reset Zoom to 100%").clicked() {
                    self.workspace.engine.zoom = F64::ONE;
                    self.workspace.engine.pan_pt = Point::ZERO;
                    self.doc_revision = self.doc_revision.wrapping_add(1);
                }
                if ui.small_button("+").on_hover_text("Zoom In").clicked() {
                    let z = (self.workspace.engine.zoom.get() * 1.25).clamp(0.1, 16.0);
                    if let Ok(zoom) = F64::new(z) {
                        self.workspace.engine.zoom = zoom;
                        self.doc_revision = self.doc_revision.wrapping_add(1);
                    }
                }
                ui.separator();
                let mut snap_obj = self.workspace.engine.snap_to_objects;
                if ui.toggle_value(&mut snap_obj, "Snap Objects").changed() {
                    let snap_page = self.workspace.engine.snap_to_page;
                    self.execute_cmd(EngineCommand::SetSnapSettings { snap_to_objects: Some(snap_obj), snap_to_page: Some(snap_page) });
                }
                let mut snap_page = self.workspace.engine.snap_to_page;
                if ui.toggle_value(&mut snap_page, "Snap Page").changed() {
                    let snap_obj = self.workspace.engine.snap_to_objects;
                    self.execute_cmd(EngineCommand::SetSnapSettings { snap_to_objects: Some(snap_obj), snap_to_page: Some(snap_page) });
                }
                let mut docker_open = self.workspace.docker_open;
                if ui.toggle_value(&mut docker_open, "Docker").changed() {
                    self.dispatch(UiAction::SetDockerOpen(docker_open));
                }
            });
        });
    }

    fn render_property_bar(&mut self, ui: &mut Ui, snap: &crate::WorkspaceFrameSnapshot) {
        egui::Panel::top("property_bar").exact_size(34.0).show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.strong(format!("{:?}:", snap.property_bar.context));
                for ctrl in &snap.property_bar.controls {
                    match ctrl {
                        PropertyControl::PageDimensions { width_pt, height_pt, units, orientation } => {
                            let mut w = width_pt.get();
                            let mut h = height_pt.get();
                            ui.label("W:");
                            let w_changed = ui.add(egui::DragValue::new(&mut w).range(36.0..=5000.0).speed(1.0).suffix(" pt")).changed();
                            ui.label("H:");
                            let h_changed = ui.add(egui::DragValue::new(&mut h).range(36.0..=5000.0).speed(1.0).suffix(" pt")).changed();
                            if (w_changed || h_changed)
                                && let (Ok(w_pt), Ok(h_pt)) = (F64::new(w), F64::new(h))
                            {
                                self.dispatch(UiAction::CommitPageSetup { width_pt: w_pt, height_pt: h_pt, units: *units });
                            }
                            if ui.selectable_label(*orientation == PageOrientation::Portrait, "Portrait").clicked() && w > h {
                                self.dispatch(UiAction::CommitPageSetup { width_pt: *height_pt, height_pt: *width_pt, units: *units });
                            }
                            if ui.selectable_label(*orientation == PageOrientation::Landscape, "Landscape").clicked() && h > w {
                                self.dispatch(UiAction::CommitPageSetup { width_pt: *height_pt, height_pt: *width_pt, units: *units });
                            }
                        }
                        PropertyControl::OriginSelector { active } => {
                            let label = match active {
                                OriginPoint::Center => "Origin: Center",
                                OriginPoint::TopLeft => "Origin: Top-Left",
                                _ => "Origin: Custom",
                            };
                            if ui.button(label).clicked() {
                                self.workspace.engine.tool_state.origin_point = match active {
                                    OriginPoint::Center => OriginPoint::TopLeft,
                                    _ => OriginPoint::Center,
                                };
                            }
                        }
                        PropertyControl::TransformBox { x_pt, y_pt, width_pt, height_pt, .. } => {
                            let mut x = x_pt.get();
                            let mut y = y_pt.get();
                            let mut w = width_pt.get();
                            let mut h = height_pt.get();
                            ui.label("X:");
                            let x_ch = ui.add(egui::DragValue::new(&mut x).speed(1.0)).changed();
                            ui.label("Y:");
                            let y_ch = ui.add(egui::DragValue::new(&mut y).speed(1.0)).changed();
                            ui.label("W:");
                            let w_ch = ui.add(egui::DragValue::new(&mut w).range(1.0..=5000.0).speed(1.0)).changed();
                            ui.label("H:");
                            let h_ch = ui.add(egui::DragValue::new(&mut h).range(1.0..=5000.0).speed(1.0)).changed();
                            if (x_ch || y_ch || w_ch || h_ch)
                                && let (Ok(nx), Ok(ny), Ok(nw), Ok(nh)) = (F64::new(x), F64::new(y), F64::new(w), F64::new(h))
                            {
                                self.execute_cmd(EngineCommand::SetSelectionBounds {
                                    x_pt: Some(nx),
                                    y_pt: Some(ny),
                                    width_pt: Some(nw),
                                    height_pt: Some(nh),
                                    origin_index: self.workspace.engine.tool_state.origin_point.as_index(),
                                });
                            }
                        }
                        PropertyControl::RectangleCorners { corner_kind, uniform_radius_pt, corners_locked, radii } => {
                            let mut rad = uniform_radius_pt.get();
                            ui.label("Corner:");
                            for (label, kind) in
                                [("Round", CornerKind::Round), ("Scallop", CornerKind::Scalloped), ("Chamfer", CornerKind::Chamfered)]
                            {
                                if ui.selectable_label(*corner_kind == kind, label).clicked() {
                                    self.dispatch(UiAction::CommitRectCorners { corner_kind: kind, radii: *radii, corners_locked: *corners_locked });
                                }
                            }
                            ui.label("Radius:");
                            if ui.add(egui::DragValue::new(&mut rad).range(0.0..=500.0).speed(0.5).suffix(" pt")).changed()
                                && let Ok(r_f64) = F64::new(rad)
                            {
                                self.dispatch(UiAction::CommitRectCorners { corner_kind: *corner_kind, radii: [r_f64; 4], corners_locked: true });
                            }
                        }
                        PropertyControl::EllipseParameters { mode, start_deg, end_deg } => {
                            for (label, m) in [("Ellipse", EllipseMode::Full), ("Pie", EllipseMode::Pie), ("Arc", EllipseMode::Arc)] {
                                if ui.selectable_label(*mode == m, label).clicked() {
                                    self.dispatch(UiAction::CommitEllipseParameters { mode: m, start_deg: *start_deg, end_deg: *end_deg });
                                }
                            }
                            let mut s = start_deg.get();
                            let mut e = end_deg.get();
                            ui.label("Start:");
                            let sc = ui.add(egui::DragValue::new(&mut s).range(0.0..=360.0).speed(1.0).suffix("°")).changed();
                            ui.label("End:");
                            let ec = ui.add(egui::DragValue::new(&mut e).range(0.0..=360.0).speed(1.0).suffix("°")).changed();
                            if (sc || ec)
                                && let (Ok(sf), Ok(ef)) = (F64::new(s), F64::new(e))
                            {
                                self.dispatch(UiAction::CommitEllipseParameters { mode: *mode, start_deg: sf, end_deg: ef });
                            }
                        }
                        PropertyControl::PolygonParameters { mode, points, inner_radius_pt } => {
                            if ui.selectable_label(*mode == PolygonSubMode::Polygon, "Polygon").clicked() {
                                self.dispatch(UiAction::CommitPolygonParameters {
                                    mode: PolygonSubMode::Polygon,
                                    points: *points,
                                    inner_radius_pt: None,
                                });
                            }
                            if ui.selectable_label(*mode == PolygonSubMode::Star, "Star").clicked() {
                                let ir = inner_radius_pt.or_else(|| F64::new(25.0).ok());
                                self.dispatch(UiAction::CommitPolygonParameters { mode: PolygonSubMode::Star, points: *points, inner_radius_pt: ir });
                            }
                            let mut pts = *points;
                            ui.label("Points:");
                            if ui.add(egui::DragValue::new(&mut pts).range(3..=64)).changed() {
                                self.dispatch(UiAction::CommitPolygonParameters { mode: *mode, points: pts, inner_radius_pt: *inner_radius_pt });
                            }
                        }
                        PropertyControl::CurveModeSelector { mode } => {
                            if ui.selectable_label(*mode == CurveSubMode::Freehand, "Freehand").clicked() {
                                self.workspace.engine.tool_state.curve_mode = CurveSubMode::Freehand;
                            }
                            if ui.selectable_label(*mode == CurveSubMode::Bezier, "Bezier").clicked() {
                                self.workspace.engine.tool_state.curve_mode = CurveSubMode::Bezier;
                            }
                        }
                        PropertyControl::ArtisticTextParameters { font_family, font_size_pt, text } => {
                            let mut size = font_size_pt.get();
                            let mut txt = text.clone();
                            ui.label(format!("Font: {font_family}"));
                            if ui.add(egui::DragValue::new(&mut size).range(4.0..=288.0).speed(0.5).suffix(" pt")).changed()
                                && let (Some(&object_id), Ok(sz)) = (self.workspace.engine.selection.first(), F64::new(size))
                            {
                                self.execute_cmd(EngineCommand::SetArtisticTextProperties {
                                    object_id,
                                    text: None,
                                    font_family: None,
                                    size_pt: Some(sz),
                                });
                            }
                            if ui.text_edit_singleline(&mut txt).changed()
                                && let Some(&object_id) = self.workspace.engine.selection.first()
                            {
                                self.execute_cmd(EngineCommand::SetArtisticTextProperties {
                                    object_id,
                                    text: Some(txt),
                                    font_family: None,
                                    size_pt: None,
                                });
                            }
                        }
                        PropertyControl::StyleSummary { selection_count, has_fill, has_outline } => {
                            ui.label(format!(
                                "({selection_count} sel | Fill: {} | Outline: {})",
                                if *has_fill { "Yes" } else { "None" },
                                if *has_outline { "Yes" } else { "None" }
                            ));
                        }
                    }
                }
            });
        });
    }

    fn render_status_bar(&mut self, ui: &mut Ui, snap: &crate::WorkspaceFrameSnapshot) {
        egui::Panel::bottom("status_bar").exact_size(24.0).show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                if let Some(pt) = self.cursor_page_pt {
                    ui.label(format!("({:.1}, {:.1}) pt", pt.x.get(), pt.y.get()));
                } else {
                    ui.label("(0.0, 0.0) pt");
                }
                ui.separator();
                ui.strong(&snap.status_bar.selection_summary);
                ui.separator();
                let fill_desc = match &snap.status_bar.fill_chip {
                    Some(c) => {
                        let [r, g, b] = c.to_srgb8();
                        format!("Fill: #{r:02X}{g:02X}{b:02X}")
                    }
                    None => "Fill: None".to_owned(),
                };
                ui.label(fill_desc);
                ui.separator();
                let outline_desc = match &snap.status_bar.outline_chip {
                    Some(c) => {
                        let [r, g, b] = c.to_srgb8();
                        format!("Outline: #{r:02X}{g:02X}{b:02X}")
                    }
                    None => "Outline: None".to_owned(),
                };
                ui.label(outline_desc);
                ui.separator();
                ui.label(&self.status_message);
            });
        });
    }

    fn render_page_navigator_bar(&mut self, ui: &mut Ui, snap: &crate::WorkspaceFrameSnapshot) {
        egui::Panel::bottom("page_navigator_bar").exact_size(26.0).show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                if ui.button("+ Page").on_hover_text("Add Page After Active").clicked() {
                    self.dispatch(UiAction::AddPage);
                }
                ui.separator();
                for tab in &snap.page_tabs {
                    let resp = ui.selectable_label(tab.active, &tab.label);
                    if resp.clicked() {
                        self.dispatch(UiAction::ActivatePageTarget(tab.target));
                    }
                    if let ActivePageTarget::Page(page_index) = tab.target {
                        resp.context_menu(|ui| {
                            if ui.button("Duplicate Page").clicked() {
                                self.dispatch(UiAction::DuplicatePage { page_index });
                                ui.close();
                            }
                            if ui.add_enabled(tab.can_delete, egui::Button::new("Delete Page")).clicked() {
                                self.dispatch(UiAction::DeletePage { page_index });
                                ui.close();
                            }
                        });
                    }
                }
            });
        });
    }

    fn render_left_toolbox(&mut self, ui: &mut Ui, snap: &crate::WorkspaceFrameSnapshot) {
        egui::Panel::left("left_toolbox").exact_size(44.0).show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(4.0);
                for tool in ToolKind::ALL {
                    let short = match tool {
                        ToolKind::Pick => "Pick",
                        ToolKind::Shape => "F10",
                        ToolKind::Crop => "Crop",
                        ToolKind::Zoom => "Zoom",
                        ToolKind::FreehandBezier => "F5",
                        ToolKind::Rectangle => "F6",
                        ToolKind::Ellipse => "F7",
                        ToolKind::PolygonStar => "Poly",
                        ToolKind::ArtisticText => "F8",
                        ToolKind::InteractiveFill => "Fill",
                    };
                    let resp =
                        ui.add_sized(vec2(36.0, 26.0), egui::Button::selectable(snap.active_tool == tool, short)).on_hover_text(tool.tooltip());
                    if resp.clicked() {
                        self.dispatch(UiAction::SelectTool(tool));
                    }
                }
            });
        });
    }

    fn render_right_palette_bar(&mut self, ui: &mut Ui, snap: &crate::WorkspaceFrameSnapshot) {
        egui::Panel::right("right_palette_bar").exact_size(28.0).show(ui, |ui| {
            egui::ScrollArea::vertical().id_salt("palette_scroll").show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    for (idx, cell) in snap.palette_cells.iter().enumerate() {
                        let (rect, resp) = ui.allocate_exact_size(vec2(20.0, 18.0), Sense::click());
                        match cell {
                            PaletteCell::NoColor => {
                                ui.painter().rect_filled(rect, CornerRadius::ZERO, Color32::WHITE);
                                ui.painter().rect_stroke(rect, CornerRadius::ZERO, Stroke::new(1.0, Color32::DARK_GRAY), egui::StrokeKind::Inside);
                                ui.painter().line_segment([rect.left_top(), rect.right_bottom()], Stroke::new(1.2, Color32::RED));
                                ui.painter().line_segment([rect.right_top(), rect.left_bottom()], Stroke::new(1.2, Color32::RED));
                            }
                            PaletteCell::Color { color, .. } => {
                                let [r, g, b] = color.to_srgb8();
                                ui.painter().rect_filled(rect, CornerRadius::ZERO, Color32::from_rgb(r, g, b));
                                ui.painter().rect_stroke(
                                    rect,
                                    CornerRadius::ZERO,
                                    Stroke::new(1.0, Color32::from_gray(60)),
                                    egui::StrokeKind::Inside,
                                );
                            }
                        }
                        let tooltip = match cell {
                            PaletteCell::NoColor => "[X] No Color (Left-click: Clear Fill, Right-click: Clear Outline)".to_owned(),
                            PaletteCell::Color { name, color } => {
                                let [r, g, b] = color.to_srgb8();
                                format!("{} (#{r:02X}{g:02X}{b:02X})\nLeft-click: Fill | Right-click: Outline", name.as_deref().unwrap_or("Swatch"))
                            }
                        };
                        let resp = resp.on_hover_text(tooltip);
                        if resp.clicked() {
                            self.dispatch(UiAction::PaletteLeftClick { cell_index: idx });
                        }
                        if resp.secondary_clicked() {
                            self.dispatch(UiAction::PaletteRightClick { cell_index: idx });
                        }
                    }
                });
            });
        });
    }

    fn render_right_docker(&mut self, ui: &mut Ui, snap: &crate::WorkspaceFrameSnapshot) {
        egui::Panel::right("right_docker").exact_size(260.0).show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.selectable_label(snap.active_docker_tab == DockerTab::Objects, "Objects").clicked() {
                    self.dispatch(UiAction::SelectDockerTab(DockerTab::Objects));
                }
                if ui.selectable_label(snap.active_docker_tab == DockerTab::Properties, "Properties").clicked() {
                    self.dispatch(UiAction::SelectDockerTab(DockerTab::Properties));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("×").on_hover_text("Close Docker").clicked() {
                        self.dispatch(UiAction::SetDockerOpen(false));
                    }
                });
            });
            ui.separator();

            match snap.active_docker_tab {
                DockerTab::Objects => {
                    ui.horizontal(|ui| {
                        if ui.small_button("+ Layer").clicked() {
                            let next_num = self.workspace.engine.current_page().layers.len().saturating_add(1);
                            self.execute_cmd(EngineCommand::AddLayer { name: format!("Layer {next_num}") });
                        }
                        ui.label(format!("{} object(s)", snap.object_tree.len()));
                    });
                    ui.separator();
                    egui::ScrollArea::vertical().id_salt("objects_tree_scroll").show(ui, |ui| {
                        let mut current_layer = String::new();
                        for entry in &snap.object_tree {
                            if entry.layer_name != current_layer {
                                current_layer.clone_from(&entry.layer_name);
                                ui.strong(format!("▾ {}", entry.layer_name));
                            }
                            ui.horizontal(|ui| {
                                let vis_label = if entry.visible { "👁" } else { "·" };
                                if ui.small_button(vis_label).on_hover_text("Toggle Visibility").clicked() {
                                    self.execute_cmd(EngineCommand::SetObjectVisible { object_id: entry.object_id, visible: !entry.visible });
                                }
                                let lock_label = if entry.locked { "🔒" } else { "·" };
                                if ui.small_button(lock_label).on_hover_text("Toggle Lock").clicked() {
                                    self.execute_cmd(EngineCommand::SetObjectLocked { object_id: entry.object_id, locked: !entry.locked });
                                }
                                let row_label = format!("{} {}", entry.type_badge, entry.object_label);
                                let shift = ui.input(|i| i.modifiers.shift);
                                if ui.selectable_label(entry.selected, row_label).clicked() {
                                    self.dispatch(UiAction::SelectObjectFromTree { object_id: entry.object_id, additive: shift });
                                }
                            });
                        }
                    });
                }
                DockerTab::Properties => {
                    egui::ScrollArea::vertical().id_salt("properties_docker_scroll").show(ui, |ui| {
                        ui.strong("Fill");
                        ui.horizontal(|ui| {
                            if ui.button("No Fill").clicked() {
                                self.execute_cmd(EngineCommand::SetFill { fill: None });
                            }
                            if ui.button("Linear Gradient").clicked()
                                && let (Ok(ang), Ok(s0), Ok(s1)) = (
                                    F64::new(45.0),
                                    FountainStop::new(0.0, Color::rgb8(0, 120, 215)),
                                    FountainStop::new(1.0, Color::rgb8(255, 200, 40)),
                                )
                            {
                                self.execute_cmd(EngineCommand::SetFill {
                                    fill: Some(FillStyle::Fountain {
                                        geometry: FountainGeometry::Linear { angle_deg: ang, edge_pad: F64::ZERO },
                                        blend_mode: FountainBlendMode::Direct,
                                        stops: vec![s0, s1],
                                    }),
                                });
                            }
                        });
                        ui.separator();
                        ui.strong("Outline");
                        ui.horizontal(|ui| {
                            if ui.button("None").clicked() {
                                self.execute_cmd(EngineCommand::SetOutline { outline: None });
                            }
                            if ui.button("Hairline").clicked() {
                                self.execute_cmd(EngineCommand::SetOutline { outline: Some(OutlineStyle::hairline(Color::BLACK)) });
                            }
                            if ui.button("2 pt").clicked()
                                && let Ok(w) = F64::new(2.0)
                            {
                                let mut out = OutlineStyle::hairline(Color::BLACK);
                                out.width_pt = w;
                                self.execute_cmd(EngineCommand::SetOutline { outline: Some(out) });
                            }
                        });
                        ui.separator();
                        ui.strong("Vector Effects");
                        if ui.button("Add Outside Contour").clicked()
                            && let (Some(&object_id), Ok(offset_pt)) = (self.workspace.engine.selection.first(), F64::new(5.0))
                        {
                            self.execute_cmd(EngineCommand::ApplyContour { object_id, steps: 1, offset_pt, inside: false });
                        }
                        if ui.button("Add Extrude / Shadow").clicked()
                            && let (Some(&object_id), Ok(depth_pt), Ok(angle_deg)) =
                                (self.workspace.engine.selection.first(), F64::new(10.0), F64::new(45.0))
                        {
                            self.execute_cmd(EngineCommand::AddExtrude { object_id, depth_pt, angle_deg });
                        }
                        if ui.button("Clear Effects").clicked()
                            && let Some(&object_id) = self.workspace.engine.selection.first()
                        {
                            self.execute_cmd(EngineCommand::ClearEffects { object_id });
                        }
                    });
                }
            }
        });
    }

    fn render_central_canvas(&mut self, ui: &mut Ui, snap: &crate::WorkspaceFrameSnapshot) {
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(Color32::from_rgb(0x80, 0x80, 0x80))).show(ui, |ui| {
            let avail = ui.available_rect_before_wrap();
            let (canvas_rect, resp) = ui.allocate_exact_size(avail.size(), Sense::click_and_drag());
            let painter = ui.painter_at(canvas_rect);

            let ruler_thick = 20.0f32;
            let pb_rect = egui::Rect::from_min_max(
                pos2((canvas_rect.left() + ruler_thick).min(canvas_rect.right()), (canvas_rect.top() + ruler_thick).min(canvas_rect.bottom())),
                canvas_rect.max,
            );

            // Compute page screen rectangle using current zoom & pan
            let page = self.workspace.engine.current_page();
            let page_w_pt = page.size.width.get().max(1.0) as f32;
            let page_h_pt = page.size.height.get().max(1.0) as f32;
            let avail_w = (pb_rect.width() - 48.0).max(32.0);
            let avail_h = (pb_rect.height() - 48.0).max(32.0);
            let base_fit = (avail_w / page_w_pt).min(avail_h / page_h_pt).clamp(0.1, 4.0);
            let zoom = (self.workspace.engine.zoom.get() as f32).clamp(0.1, 16.0);
            let scale = base_fit * zoom;
            let pan_x = self.workspace.engine.pan_pt.x.get() as f32;
            let pan_y = self.workspace.engine.pan_pt.y.get() as f32;

            let pw = (page_w_pt * scale).max(4.0);
            let ph = (page_h_pt * scale).max(4.0);
            let page_x0 = pb_rect.left() + (pb_rect.width() - pw) * 0.5 + pan_x;
            let page_y0 = pb_rect.top() + (pb_rect.height() - ph) * 0.5 + pan_y;
            let page_rect = egui::Rect::from_min_size(pos2(page_x0, page_y0), vec2(pw, ph));

            // Draw page drop shadow and white page background
            let shadow_rect = page_rect.translate(vec2(5.0, 5.0));
            painter.rect_filled(shadow_rect, CornerRadius::ZERO, Color32::from_rgba_unmultiplied(30, 30, 30, 140));
            painter.rect_filled(page_rect, CornerRadius::ZERO, Color32::WHITE);

            // Update and draw cached rasterized page texture
            let dpi = (72.0 * f64::from(scale)).clamp(36.0, 216.0);
            let dpi_bucket = ((dpi / 18.0).round() as u32).saturating_mul(18);
            let target = self.workspace.engine.active_target;
            let needs_re_render = match &self.cached_page {
                Some(c) => c.revision != self.doc_revision || c.target != target || c.dpi_bucket != dpi_bucket,
                None => true,
            };
            if needs_re_render && let Ok(rendered) = self.workspace.render_active_page(f64::from(dpi_bucket.max(36))) {
                let w = rendered.surface.width as usize;
                let h = rendered.surface.height as usize;
                let mut rgba = Vec::with_capacity(w.saturating_mul(h).saturating_mul(4));
                for px in &rendered.surface.pixels {
                    rgba.extend_from_slice(px);
                }
                let color_img = egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba);
                let tex = ui.ctx().load_texture("active_page_surface", color_img, TextureOptions::LINEAR);
                self.cached_page = Some(CachedPageTexture { revision: self.doc_revision, target, dpi_bucket, texture: tex });
            }
            if let Some(c) = &self.cached_page {
                let uv = egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
                painter.image(c.texture.id(), page_rect, uv, Color32::WHITE);
            }
            painter.rect_stroke(page_rect, CornerRadius::ZERO, Stroke::new(1.0, Color32::BLACK), egui::StrokeKind::Outside);

            let page_to_screen =
                |pt: Point| -> Pos2 { pos2(page_rect.left() + (pt.x.get() as f32) * scale, page_rect.top() + (pt.y.get() as f32) * scale) };
            let screen_to_page = |pos: Pos2| -> Point {
                let px = f64::from((pos.x - page_rect.left()) / scale);
                let py = f64::from((pos.y - page_rect.top()) / scale);
                Point::new(px, py).unwrap_or(Point::ZERO)
            };

            // Draw guidelines
            if self.show_guidelines {
                let guide_stroke = Stroke::new(1.0, Color32::from_rgb(0x00, 0x55, 0xE5));
                for y in &snap.horizontal_guidelines {
                    let sy = page_rect.top() + (y.get() as f32) * scale;
                    painter.line_segment([pos2(pb_rect.left(), sy), pos2(pb_rect.right(), sy)], guide_stroke);
                }
                for x in &snap.vertical_guidelines {
                    let sx = page_rect.left() + (x.get() as f32) * scale;
                    painter.line_segment([pos2(sx, pb_rect.top()), pos2(sx, pb_rect.bottom())], guide_stroke);
                }
            }

            // Draw selection bounding box and 8 resize handles
            if let Some(sel_bounds) = snap.selection_bounds {
                let p0 = page_to_screen(Point { x: sel_bounds.x0, y: sel_bounds.y0 });
                let p1 = page_to_screen(Point { x: sel_bounds.x1, y: sel_bounds.y1 });
                let srect = egui::Rect::from_two_pos(p0, p1);
                painter.rect_stroke(srect, CornerRadius::ZERO, Stroke::new(1.0, Color32::from_rgb(0x00, 0x66, 0xCC)), egui::StrokeKind::Outside);
                let handles = [
                    srect.left_top(),
                    pos2(srect.center().x, srect.top()),
                    srect.right_top(),
                    pos2(srect.left(), srect.center().y),
                    pos2(srect.right(), srect.center().y),
                    srect.left_bottom(),
                    pos2(srect.center().x, srect.bottom()),
                    srect.right_bottom(),
                ];
                for hp in handles {
                    let hr = egui::Rect::from_center_size(hp, vec2(6.0, 6.0));
                    painter.rect_filled(hr, CornerRadius::ZERO, Color32::BLACK);
                    painter.rect_stroke(hr, CornerRadius::ZERO, Stroke::new(1.0, Color32::WHITE), egui::StrokeKind::Outside);
                }
            }

            // Draw live gesture rubberband preview
            if let Some(gesture) = &self.workspace.engine.tool_state.gesture {
                match gesture {
                    GestureState::DragCreateShape { start, current, shift_constrain, alt_from_center, .. } => {
                        if let Some(r) = GestureState::compute_drag_rect(*start, *current, *shift_constrain, *alt_from_center) {
                            let p0 = page_to_screen(Point { x: r.x0, y: r.y0 });
                            let p1 = page_to_screen(Point { x: r.x1, y: r.y1 });
                            painter.rect_stroke(
                                egui::Rect::from_two_pos(p0, p1),
                                CornerRadius::ZERO,
                                Stroke::new(1.2, Color32::from_rgb(0x00, 0x55, 0xE5)),
                                egui::StrokeKind::Outside,
                            );
                        }
                    }
                    GestureState::DragInteractiveFill { start, current } => {
                        let p0 = page_to_screen(*start);
                        let p1 = page_to_screen(*current);
                        painter.line_segment([p0, p1], Stroke::new(1.5, Color32::from_rgb(0x00, 0x55, 0xE5)));
                        painter.circle_filled(p0, 4.0, Color32::WHITE);
                        painter.circle_filled(p1, 4.0, Color32::from_rgb(0x00, 0x55, 0xE5));
                    }
                    _ => {}
                }
            }

            // Draw top and left rulers
            let top_ruler = egui::Rect::from_min_max(canvas_rect.min, pos2(canvas_rect.right(), canvas_rect.top() + ruler_thick));
            let left_ruler = egui::Rect::from_min_max(canvas_rect.min, pos2(canvas_rect.left() + ruler_thick, canvas_rect.bottom()));
            painter.rect_filled(top_ruler, CornerRadius::ZERO, Color32::from_rgb(0xEC, 0xE9, 0xD8));
            painter.rect_filled(left_ruler, CornerRadius::ZERO, Color32::from_rgb(0xEC, 0xE9, 0xD8));
            painter.line_segment(
                [pos2(canvas_rect.left(), top_ruler.bottom()), pos2(canvas_rect.right(), top_ruler.bottom())],
                Stroke::new(1.0, Color32::GRAY),
            );
            painter.line_segment(
                [pos2(left_ruler.right(), canvas_rect.top()), pos2(left_ruler.right(), canvas_rect.bottom())],
                Stroke::new(1.0, Color32::GRAY),
            );

            // Pointer interaction on canvas
            if let Some(hover_pos) = resp.hover_pos() {
                self.cursor_page_pt = Some(screen_to_page(hover_pos));
                let scroll_y = ui.input(|i| i.smooth_scroll_delta.y);
                if scroll_y.abs() > 0.5 {
                    let factor = if scroll_y > 0.0 { 1.1 } else { 1.0 / 1.1 };
                    let z = (self.workspace.engine.zoom.get() * factor).clamp(0.1, 16.0);
                    if let Ok(zoom) = F64::new(z) {
                        self.workspace.engine.zoom = zoom;
                        self.doc_revision = self.doc_revision.wrapping_add(1);
                    }
                }
            }

            // Middle-mouse panning
            if resp.dragged_by(PointerButton::Middle) {
                let delta = resp.drag_delta();
                let nx = self.workspace.engine.pan_pt.x.get() + f64::from(delta.x);
                let ny = self.workspace.engine.pan_pt.y.get() + f64::from(delta.y);
                if let Ok(pan_pt) = Point::new(nx, ny) {
                    self.workspace.engine.pan_pt = pan_pt;
                }
            }

            let modifiers = ui.input(|i| i.modifiers);
            if resp.drag_started_by(PointerButton::Primary)
                && let Some(pos) = resp.interact_pointer_pos()
            {
                let pt = screen_to_page(pos);
                self.last_pan_pos = Some(pos);
                self.dispatch(UiAction::CanvasPointerDown { pt, shift: modifiers.shift, alt: modifiers.alt });
            } else if resp.dragged_by(PointerButton::Primary)
                && let Some(pos) = resp.interact_pointer_pos()
            {
                let pt = screen_to_page(pos);
                self.dispatch(UiAction::CanvasPointerMove { pt, shift: modifiers.shift, alt: modifiers.alt });
            } else if resp.drag_stopped_by(PointerButton::Primary) {
                let pos = resp.interact_pointer_pos().or(self.last_pan_pos).unwrap_or(page_rect.center());
                let pt = screen_to_page(pos);
                self.dispatch(UiAction::CanvasPointerUp { pt, shift: modifiers.shift, alt: modifiers.alt });
            } else if resp.clicked_by(PointerButton::Primary)
                && let Some(pos) = resp.interact_pointer_pos()
            {
                let pt = screen_to_page(pos);
                self.dispatch(UiAction::CanvasPointerDown { pt, shift: modifiers.shift, alt: modifiers.alt });
                self.dispatch(UiAction::CanvasPointerUp { pt, shift: modifiers.shift, alt: modifiers.alt });
            }
        });
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing, clippy::arithmetic_side_effects)]
mod tests {
    use super::*;
    use coralcraft_tools::PropertyContext;
    use egui_kittest::{Harness, kittest::Queryable};

    #[test]
    fn egui_app_shell_renders_panels_switches_tools_and_draws_on_canvas_r2_1() {
        let app = CoralCraftApp::new(Document::new_a4_portrait(), Services::default());
        let mut harness = Harness::builder().with_size(vec2(1440.0, 900.0)).build_ui_state(
            |ui, app: &mut CoralCraftApp| {
                app.ui(ui);
            },
            app,
        );

        // 1. Verify top menu bar items are queryable in the accessibility tree
        for menu_label in ["File", "Edit", "View", "Layout", "Object", "Effects", "Bitmaps", "Text", "Tools", "Window", "Help"] {
            let _ = harness.get_by_label(menu_label);
        }

        // 2. Click the F6 (Rectangle) tool button in the left toolbox
        harness.get_by_label("F6").click();
        harness.run();
        assert_eq!(harness.state().workspace.engine.tool_state.active_tool, ToolKind::Rectangle);
        assert_eq!(harness.state().workspace.snapshot().property_bar.context, PropertyContext::Rectangle);

        // 3. Dispatch a canvas rectangle creation gesture and verify the Objects docker updates
        harness.state_mut().dispatch(UiAction::CanvasPointerDown { pt: Point::new(50.0, 60.0).unwrap(), shift: false, alt: false });
        harness.state_mut().dispatch(UiAction::CanvasPointerUp { pt: Point::new(180.0, 160.0).unwrap(), shift: false, alt: false });
        harness.run();
        assert_eq!(harness.state().workspace.engine.current_page().layers[0].objects.len(), 1);
        let _ = harness.get_by_label("[Rec] Rectangle");

        // 4. Click `+ Page` in the bottom Page Navigator Bar and verify Page 2 is added
        harness.get_by_label("+ Page").click();
        harness.run();
        assert_eq!(harness.state().workspace.engine.document.pages.len(), 2);
        let _ = harness.get_by_label("Page 2");
    }

    #[test]
    fn egui_app_file_open_save_and_clipboard_services_roundtrip_r2_1() {
        let tmp_dir = std::env::temp_dir().join(format!("coralcraft_egui_r2_1_{}", std::process::id()));
        std::fs::create_dir_all(&tmp_dir).unwrap();
        let save_path = tmp_dir.join("test_doc.coralcraft");
        let svg_path = tmp_dir.join("test_export.svg");

        let clipboard_buf = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let cb_get = clipboard_buf.clone();
        let cb_set = clipboard_buf.clone();

        let save_path_clone = save_path.clone();
        let svg_path_clone = svg_path.clone();
        let services = Services {
            pick_open: Some(Box::new(move || Some(save_path_clone.clone()))),
            pick_save: Some(Box::new({
                let save_path = save_path.clone();
                move |suggested| {
                    if std::path::Path::new(suggested).extension().is_some_and(|ext| ext.eq_ignore_ascii_case("svg")) {
                        Some(svg_path_clone.clone())
                    } else {
                        Some(save_path.clone())
                    }
                }
            })),
            clipboard_get_text: Some(Box::new(move || Some(cb_get.lock().unwrap().clone()))),
            clipboard_set_text: Some(Box::new(move |text| {
                *cb_set.lock().unwrap() = text.to_owned();
                Ok(())
            })),
        };

        let mut app = CoralCraftApp::new_with_welcome_artwork(services);
        // Select all and copy to clipboard
        app.workspace.engine.select_all();
        app.copy_selection();
        assert!(!clipboard_buf.lock().unwrap().is_empty());

        // Save native .coralcraft and export SVG
        app.trigger_save_dialog(true);
        assert!(save_path.exists());
        app.trigger_export_dialog("export.svg");
        assert!(svg_path.exists());

        // Reset to empty doc, then open via service callback
        app.workspace = DesktopWorkspace::new(Document::new_a4_portrait());
        assert_eq!(app.workspace.engine.current_page().layers[0].objects.len(), 0);
        app.trigger_open_dialog();
        assert!(app.workspace.engine.current_page().layers[0].objects.len() >= 4);

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }
}
