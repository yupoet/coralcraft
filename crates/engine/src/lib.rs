//! Core application engine (`EditorEngine`) combining multi-page document state,
//! tool gestures, undo/redo command history, hit-testing, and Property Bar snapshots (L5).

use coralcraft_color::Color;
use coralcraft_doc::{Document, FillStyle, Layer, LayerId, LayerRole, ObjectId, ObjectKind, ObjectNode, OutlineStyle, Page, PageId, Residual};
use coralcraft_geom::{Affine2, CornerKind, EllipseMode, F64, Point, Rect, Size, Unit};
use coralcraft_tools::{
    KeyShortcut, PageOrientation, PolygonSubMode, PropertyBarSchema, ToolKind, ToolState, build_property_bar_schema, combined_selection_bounds,
    object_local_bounds,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Active target selected in the bottom CorelDRAW Page Navigator Bar
/// (`+`, `Page 1`, `Page 2`, ..., `Master Page`, `Desktop`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ActivePageTarget {
    /// Zero-based index into `Document::pages`.
    Page(usize),
    /// Shared Master layer (`MasterAllPages`) repeated on every page.
    MasterPage,
    /// Shared pasteboard (`Desktop`) layer where objects belong to no single page.
    Desktop,
}

impl Default for ActivePageTarget {
    fn default() -> Self {
        Self::Page(0)
    }
}

/// Target channel of a click on the right vertical CorelDRAW Color Palette Bar
/// or the Status Bar swatch chips.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PaletteChannel {
    /// Left click sets Fill (`None` when clicking the top `[X]` No-Color swatch).
    Fill,
    /// Right click sets Outline (`None` when clicking the top `[X]` No-Color swatch).
    Outline,
}

/// Atomic, undoable editor command executed by [`EditorEngine::execute`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EngineCommand {
    /// Adds a newly created object to the active target layer and selects it.
    AddObject { name: Option<String>, kind: ObjectKind },
    /// Deletes all currently selected objects.
    DeleteSelection,
    /// Translates all currently selected objects by `(dx_pt, dy_pt)`.
    TranslateSelection { dx_pt: F64, dy_pt: F64 },
    /// Applies a color swatch (or `[X]` No-Color when `color == None`) to `channel`.
    /// If selection is non-empty, updates selected objects; otherwise updates creation defaults.
    ApplyPaletteSwatch { channel: PaletteChannel, color: Option<Color> },
    /// Sets explicit fill style on selected objects (or default fill if selection is empty).
    SetFill { fill: Option<FillStyle> },
    /// Sets explicit outline style on selected objects (or default outline if selection is empty).
    SetOutline { outline: Option<OutlineStyle> },
    /// Updates page dimensions, units, and orientation.
    SetPageSetup { width_pt: F64, height_pt: F64, units: Unit },
    /// Updates corner kind and radii on selected rectangles (or default rectangle tool state).
    UpdateRectCorners { corner_kind: CornerKind, radii: [F64; 4], corners_locked: bool },
    /// Updates ellipse mode and start/end angles on selected ellipses (or default ellipse tool state).
    UpdateEllipseParameters { mode: EllipseMode, start_deg: F64, end_deg: F64 },
    /// Updates polygon/star mode, point/side count, and optional inner radius on selected polygons.
    UpdatePolygonParameters { mode: PolygonSubMode, points: u32, inner_radius_pt: Option<F64> },
    /// Inserts a new blank page immediately after the active numbered page and activates it.
    AddPageAfterActive,
    /// Duplicates the specified 0-based page (including its layers and objects) and activates the copy.
    DuplicatePage { page_index: usize },
    /// Renames the specified 0-based page tab.
    RenamePage { page_index: usize, name: String },
    /// Deletes the specified 0-based page when `document.pages.len() > 1`.
    DeletePage { page_index: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct UndoEntry {
    document: Document,
    active_target: ActivePageTarget,
    selection: Vec<ObjectId>,
    tool_state: ToolState,
    default_fill: Option<FillStyle>,
    default_outline: Option<OutlineStyle>,
}

/// Summary of the Status Bar state at the bottom of the CorelDRAW workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusBarSnapshot {
    pub cursor_pt: Point,
    pub units: Unit,
    pub selection_summary: String,
    pub fill_chip: Option<Color>,
    pub outline_chip: Option<Color>,
}

/// Core stateful application engine (`coralcraft-engine`, L5).
#[derive(Debug, Clone)]
pub struct EditorEngine {
    pub document: Document,
    pub active_target: ActivePageTarget,
    pub selection: Vec<ObjectId>,
    pub tool_state: ToolState,
    pub default_fill: Option<FillStyle>,
    pub default_outline: Option<OutlineStyle>,
    pub cursor_pt: Point,
    pub zoom: F64,
    pub pan_pt: Point,
    pub snap_to_objects: bool,
    pub snap_to_page: bool,
    undo_stack: Vec<UndoEntry>,
    redo_stack: Vec<UndoEntry>,
}

impl Default for EditorEngine {
    fn default() -> Self {
        Self::new(Document::new_a4_portrait())
    }
}

impl EditorEngine {
    /// Creates an engine initialized with `document`, `Page 1` active, and standard CorelDRAW
    /// creation defaults (No Fill, Black Hairline Outline).
    #[must_use]
    pub fn new(document: Document) -> Self {
        Self {
            document,
            active_target: ActivePageTarget::Page(0),
            selection: Vec::new(),
            tool_state: ToolState::default(),
            default_fill: None,
            default_outline: Some(OutlineStyle::hairline(Color::BLACK)),
            cursor_pt: Point::ZERO,
            zoom: F64::ONE,
            pan_pt: Point::ZERO,
            snap_to_objects: true,
            snap_to_page: true,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    /// Returns the 0-based index of the currently viewed page (`0` when viewing `MasterPage` or `Desktop`).
    #[must_use]
    pub fn current_page_index(&self) -> usize {
        match self.active_target {
            ActivePageTarget::Page(idx) => idx.min(self.document.pages.len().saturating_sub(1)),
            ActivePageTarget::MasterPage | ActivePageTarget::Desktop => 0,
        }
    }

    /// Returns a reference to the currently viewed [`Page`].
    #[must_use]
    pub fn current_page(&self) -> &Page {
        let idx = self.current_page_index();
        &self.document.pages[idx]
    }

    /// Switches the active tab in the bottom Page Navigator Bar (`Page 1..N`, `Master Page`, `Desktop`).
    pub fn set_active_target(&mut self, target: ActivePageTarget) {
        let valid = match target {
            ActivePageTarget::Page(idx) if idx < self.document.pages.len() => ActivePageTarget::Page(idx),
            ActivePageTarget::Page(_) => ActivePageTarget::Page(self.document.pages.len().saturating_sub(1)),
            other => other,
        };
        if self.active_target != valid {
            self.active_target = valid;
            self.selection.clear();
        }
    }

    /// Collects references to all currently selected [`ObjectNode`]s in z-order.
    #[must_use]
    pub fn selected_nodes(&self) -> Vec<&ObjectNode> {
        if self.selection.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        for page in &self.document.pages {
            for layer in &page.layers {
                for obj in &layer.objects {
                    if self.selection.contains(&obj.id) {
                        out.push(obj);
                    }
                }
            }
        }
        for layer in &self.document.master_layers {
            for obj in &layer.objects {
                if self.selection.contains(&obj.id) {
                    out.push(obj);
                }
            }
        }
        out
    }

    /// Computes the context-sensitive CorelDRAW Property Bar schema for the current state.
    #[must_use]
    pub fn property_bar_schema(&self) -> PropertyBarSchema {
        let selected = self.selected_nodes();
        build_property_bar_schema(&selected, self.current_page(), self.document.units, &self.tool_state)
    }

    /// Computes the bottom CorelDRAW Status Bar snapshot (`cursor x, y`, selection summary, Fill/Outline chips).
    #[must_use]
    pub fn status_bar_snapshot(&self) -> StatusBarSnapshot {
        let selected = self.selected_nodes();
        let selection_summary = match selected.as_slice() {
            [] => "No selection".to_owned(),
            [single] => match &single.kind {
                ObjectKind::Rect(_) => "Rectangle on Layer".to_owned(),
                ObjectKind::Ellipse(_) => "Ellipse on Layer".to_owned(),
                ObjectKind::Polygon(p) if p.inner_radius.is_some() => "Star on Layer".to_owned(),
                ObjectKind::Polygon(_) => "Polygon on Layer".to_owned(),
                ObjectKind::Path(_) => "Curve on Layer".to_owned(),
                ObjectKind::ArtisticText { .. } => "Artistic Text".to_owned(),
                ObjectKind::Group { children } => format!("Group of {} objects", children.len()),
                ObjectKind::Unsupported(_) => "Unsupported CDR Object".to_owned(),
            },
            many => format!("{} objects selected", many.len()),
        };

        let fill_source = selected.first().map_or(self.default_fill.as_ref(), |n| n.fill.as_ref());
        let outline_source = selected.first().map_or(self.default_outline.as_ref(), |n| n.outline.as_ref());

        let fill_chip = match fill_source {
            Some(FillStyle::Uniform { color }) => Some(color.clone()),
            Some(FillStyle::Fountain { stops, .. }) => stops.first().map(|s| s.color.clone()),
            Some(FillStyle::Unknown { .. }) | None => None,
        };
        let outline_chip = outline_source.map(|o| o.color.clone());

        StatusBarSnapshot { cursor_pt: self.cursor_pt, units: self.document.units, selection_summary, fill_chip, outline_chip }
    }

    /// Applies a keyboard shortcut (`F5`, `F6`, `F7`, `F8`, `F10`, `Y`, `Z`, `G`, `Escape`).
    pub fn handle_shortcut(&mut self, shortcut: KeyShortcut) -> bool {
        self.tool_state.apply_shortcut(shortcut)
    }

    /// Performs top-to-bottom hit-testing at `pt` (in page coordinates) against effective visible,
    /// unlocked layers of the current page.
    #[must_use]
    pub fn hit_test_point(&self, pt: Point) -> Option<ObjectId> {
        let layers = self.document.effective_layers_for_page(self.current_page_index(), false).ok()?;
        for layer in layers.into_iter().rev() {
            if layer.locked || !layer.visible {
                continue;
            }
            for obj in layer.objects.iter().rev() {
                if obj.locked || !obj.visible {
                    continue;
                }
                let bbox = combined_selection_bounds(&[obj]);
                let pad = 2.0;
                if pt.x.get() >= bbox.x0.get() - pad
                    && pt.x.get() <= bbox.x1.get() + pad
                    && pt.y.get() >= bbox.y0.get() - pad
                    && pt.y.get() <= bbox.y1.get() + pad
                {
                    return Some(obj.id);
                }
            }
        }
        None
    }

    /// Handles a primary pointer down on the canvas at `pt` (in page coordinates).
    pub fn pointer_down(&mut self, pt: Point, shift_modifier: bool, alt_modifier: bool) {
        self.cursor_pt = pt;
        match self.tool_state.active_tool {
            ToolKind::Pick => {
                if let Some(hit_id) = self.hit_test_point(pt) {
                    if shift_modifier {
                        if let Some(pos) = self.selection.iter().position(|&id| id == hit_id) {
                            self.selection.remove(pos);
                        } else {
                            self.selection.push(hit_id);
                        }
                    } else if !self.selection.contains(&hit_id) {
                        self.selection = vec![hit_id];
                    }
                    self.tool_state.begin_drag(pt, shift_modifier, alt_modifier);
                } else if !shift_modifier {
                    self.selection.clear();
                }
            }
            ToolKind::ArtisticText => {
                let size_pt = self.tool_state.default_font_size_pt;
                let font_family = self.tool_state.default_font_family.clone();
                self.execute(EngineCommand::AddObject {
                    name: Some("Artistic Text".to_owned()),
                    kind: ObjectKind::ArtisticText { text: "Text".to_owned(), font_family, size_pt, position: pt },
                });
            }
            ToolKind::Zoom => {
                let factor = if alt_modifier { 0.5 } else { 2.0 };
                if let Ok(new_zoom) = F64::new((self.zoom.get() * factor).clamp(0.05, 64.0)) {
                    self.zoom = new_zoom;
                }
            }
            _ => {
                self.tool_state.begin_drag(pt, shift_modifier, alt_modifier);
            }
        }
    }

    /// Updates pointer coordinates during a canvas move/drag.
    pub fn pointer_move(&mut self, pt: Point, shift_modifier: bool, alt_modifier: bool) {
        self.cursor_pt = pt;
        self.tool_state.update_drag(pt, shift_modifier, alt_modifier);
    }

    /// Completes an active canvas drag gesture, committing shape creation or object movement
    /// as a single undo step.
    pub fn pointer_up(&mut self, pt: Point, shift_modifier: bool, alt_modifier: bool) {
        self.cursor_pt = pt;
        self.tool_state.update_drag(pt, shift_modifier, alt_modifier);

        if let Some(coralcraft_tools::GestureState::DragMoveSelection { start, current }) = self.tool_state.gesture.clone() {
            self.tool_state.gesture = None;
            let dx = current.x.get() - start.x.get();
            let dy = current.y.get() - start.y.get();
            if (dx.abs() > 1e-6 || dy.abs() > 1e-6)
                && !self.selection.is_empty()
                && let (Ok(dx_pt), Ok(dy_pt)) = (F64::new(dx), F64::new(dy))
            {
                self.execute(EngineCommand::TranslateSelection { dx_pt, dy_pt });
            }
            return;
        }

        if let Some(kind) = self.tool_state.finish_create_shape() {
            let name = match &kind {
                ObjectKind::Rect(_) => Some("Rectangle".to_owned()),
                ObjectKind::Ellipse(_) => Some("Ellipse".to_owned()),
                ObjectKind::Polygon(p) if p.inner_radius.is_some() => Some("Star".to_owned()),
                ObjectKind::Polygon(_) => Some("Polygon".to_owned()),
                _ => None,
            };
            self.execute(EngineCommand::AddObject { name, kind });
        }
    }

    /// Executes an atomic [`EngineCommand`] and pushes the previous state onto the undo stack.
    pub fn execute(&mut self, cmd: EngineCommand) -> bool {
        match &cmd {
            EngineCommand::DeletePage { page_index } if self.document.pages.len() <= 1 || *page_index >= self.document.pages.len() => {
                return false;
            }
            EngineCommand::RenamePage { page_index, .. } | EngineCommand::DuplicatePage { page_index }
                if *page_index >= self.document.pages.len() =>
            {
                return false;
            }
            EngineCommand::DeleteSelection if self.selection.is_empty() => {
                return false;
            }
            EngineCommand::TranslateSelection { dx_pt, dy_pt }
                if self.selection.is_empty() || Affine2::translation(dx_pt.get(), dy_pt.get()).is_err() =>
            {
                return false;
            }
            EngineCommand::SetPageSetup { width_pt, height_pt, .. } if Size::new(width_pt.get(), height_pt.get()).is_err() => {
                return false;
            }
            _ => {}
        }

        self.push_undo_snapshot();

        match cmd {
            EngineCommand::AddObject { name, kind } => {
                let id = self.document.allocate_object_id();
                let node = ObjectNode::new(id, name, kind, self.default_fill.clone(), self.default_outline.clone());
                self.target_layer_mut().objects.push(node);
                self.selection = vec![id];
            }
            EngineCommand::DeleteSelection => {
                let sel = self.selection.clone();
                for page in &mut self.document.pages {
                    for layer in &mut page.layers {
                        if !layer.locked {
                            layer.objects.retain(|o| o.locked || !sel.contains(&o.id));
                        }
                    }
                }
                for layer in &mut self.document.master_layers {
                    if !layer.locked {
                        layer.objects.retain(|o| o.locked || !sel.contains(&o.id));
                    }
                }
                self.selection.clear();
            }
            EngineCommand::TranslateSelection { dx_pt, dy_pt } => {
                let Ok(t) = Affine2::translation(dx_pt.get(), dy_pt.get()) else {
                    return false;
                };
                self.for_each_selected_mut(|obj| {
                    if let Ok(composed) = obj.transform.then(t) {
                        obj.transform = composed;
                    }
                });
            }
            EngineCommand::ApplyPaletteSwatch { channel, color } => match channel {
                PaletteChannel::Fill => {
                    let fill = color.map(FillStyle::uniform);
                    if self.selection.is_empty() {
                        self.default_fill = fill;
                    } else {
                        self.for_each_selected_mut(|obj| {
                            obj.fill = fill.clone();
                        });
                    }
                }
                PaletteChannel::Outline => {
                    if self.selection.is_empty() {
                        self.default_outline = color.map(|c| {
                            let mut style = self.default_outline.clone().unwrap_or_else(|| OutlineStyle::hairline(c.clone()));
                            style.color = c;
                            style
                        });
                    } else {
                        self.for_each_selected_mut(|obj| {
                            obj.outline = color.clone().map(|c| {
                                let mut style = obj.outline.clone().unwrap_or_else(|| OutlineStyle::hairline(c.clone()));
                                style.color = c;
                                style
                            });
                        });
                    }
                }
            },
            EngineCommand::SetFill { fill } => {
                if self.selection.is_empty() {
                    self.default_fill = fill;
                } else {
                    self.for_each_selected_mut(|obj| obj.fill = fill.clone());
                }
            }
            EngineCommand::SetOutline { outline } => {
                if self.selection.is_empty() {
                    self.default_outline = outline;
                } else {
                    self.for_each_selected_mut(|obj| obj.outline = outline.clone());
                }
            }
            EngineCommand::SetPageSetup { width_pt, height_pt, units } => {
                self.document.units = units;
                let idx = self.current_page_index();
                if let Some(page) = self.document.pages.get_mut(idx)
                    && let Ok(size) = Size::new(width_pt.get(), height_pt.get())
                {
                    page.size = size;
                }
            }
            EngineCommand::UpdateRectCorners { corner_kind, radii, corners_locked } => {
                self.tool_state.default_rect_corner_kind = corner_kind;
                self.tool_state.default_rect_uniform_radius_pt = radii[0];
                self.tool_state.default_rect_corners_locked = corners_locked;
                self.for_each_selected_mut(|obj| {
                    if let ObjectKind::Rect(spec) = &mut obj.kind {
                        spec.corner_kinds = [corner_kind; 4];
                        spec.radii = if corners_locked { [radii[0]; 4] } else { radii };
                    }
                });
            }
            EngineCommand::UpdateEllipseParameters { mode, start_deg, end_deg } => {
                self.tool_state.default_ellipse_mode = mode;
                self.tool_state.default_ellipse_start_deg = start_deg;
                self.tool_state.default_ellipse_end_deg = end_deg;
                self.for_each_selected_mut(|obj| {
                    if let ObjectKind::Ellipse(spec) = &mut obj.kind {
                        spec.mode = mode;
                        spec.start_deg = start_deg;
                        spec.end_deg = end_deg;
                    }
                });
            }
            EngineCommand::UpdatePolygonParameters { mode, points, inner_radius_pt } => {
                let pts = points.max(3);
                self.tool_state.polygon_mode = mode;
                self.tool_state.default_polygon_points = pts;
                self.for_each_selected_mut(|obj| {
                    if let ObjectKind::Polygon(spec) = &mut obj.kind {
                        spec.points = pts;
                        spec.inner_radius = match mode {
                            PolygonSubMode::Polygon => None,
                            PolygonSubMode::Star => inner_radius_pt.or_else(|| F64::new(spec.radius.get() * 0.5).ok()),
                        };
                    }
                });
            }
            EngineCommand::AddPageAfterActive => {
                let insert_after = self.current_page_index();
                let template_size = self.current_page().size;
                let page_id = PageId(self.document.allocate_object_id().0);
                let layer_id = LayerId(self.document.allocate_object_id().0);
                let new_num = self.document.pages.len() + 1;
                let new_page = Page {
                    id: page_id,
                    name: format!("Page {new_num}"),
                    size: template_size,
                    bleed: F64::ZERO,
                    background: Some(FillStyle::uniform(Color::WHITE)),
                    layers: vec![Layer::new_normal(layer_id, "Layer 1")],
                    residual: Residual::default(),
                    extra: BTreeMap::new(),
                };
                let new_idx = (insert_after + 1).min(self.document.pages.len());
                self.document.pages.insert(new_idx, new_page);
                self.active_target = ActivePageTarget::Page(new_idx);
                self.selection.clear();
            }
            EngineCommand::DuplicatePage { page_index } => {
                let idx = page_index.min(self.document.pages.len().saturating_sub(1));
                let mut cloned = self.document.pages[idx].clone();
                cloned.id = PageId(self.document.allocate_object_id().0);
                cloned.name = format!("{} Copy", cloned.name);
                for layer in &mut cloned.layers {
                    layer.id = LayerId(self.document.allocate_object_id().0);
                    for obj in &mut layer.objects {
                        reallocate_object_ids_recursive(obj, &mut self.document);
                    }
                }
                let new_idx = idx + 1;
                self.document.pages.insert(new_idx, cloned);
                self.active_target = ActivePageTarget::Page(new_idx);
                self.selection.clear();
            }
            EngineCommand::RenamePage { page_index, name } => {
                if let Some(page) = self.document.pages.get_mut(page_index) {
                    page.name = name;
                }
            }
            EngineCommand::DeletePage { page_index } => {
                if self.document.pages.len() > 1 && page_index < self.document.pages.len() {
                    self.document.pages.remove(page_index);
                    let clamped = page_index.min(self.document.pages.len() - 1);
                    self.active_target = ActivePageTarget::Page(clamped);
                    self.selection.clear();
                }
            }
        }
        true
    }

    /// Returns `true` if at least one command can be undone.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Returns `true` if at least one command can be redone.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Undoes the most recent committed [`EngineCommand`].
    pub fn undo(&mut self) -> bool {
        let Some(prev) = self.undo_stack.pop() else {
            return false;
        };
        let current = self.capture_snapshot();
        self.redo_stack.push(current);
        self.restore_snapshot(prev);
        true
    }

    /// Redoes the most recently undone [`EngineCommand`].
    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo_stack.pop() else {
            return false;
        };
        let current = self.capture_snapshot();
        self.undo_stack.push(current);
        self.restore_snapshot(next);
        true
    }

    /// Convenience helper returning orientation of the active page.
    #[must_use]
    pub fn current_page_orientation(&self) -> PageOrientation {
        let p = self.current_page();
        if p.size.width.get() >= p.size.height.get() { PageOrientation::Landscape } else { PageOrientation::Portrait }
    }

    /// Computes local bounds of an object kind (`coralcraft_tools::object_local_bounds` re-export).
    #[must_use]
    pub fn local_bounds(kind: &ObjectKind) -> Rect {
        object_local_bounds(kind)
    }

    fn capture_snapshot(&self) -> UndoEntry {
        UndoEntry {
            document: self.document.clone(),
            active_target: self.active_target,
            selection: self.selection.clone(),
            tool_state: self.tool_state.clone(),
            default_fill: self.default_fill.clone(),
            default_outline: self.default_outline.clone(),
        }
    }

    fn push_undo_snapshot(&mut self) {
        let snap = self.capture_snapshot();
        self.undo_stack.push(snap);
        self.redo_stack.clear();
    }

    fn restore_snapshot(&mut self, entry: UndoEntry) {
        self.document = entry.document;
        self.active_target = entry.active_target;
        self.selection = entry.selection;
        self.tool_state = entry.tool_state;
        self.default_fill = entry.default_fill;
        self.default_outline = entry.default_outline;
    }

    fn target_layer_mut(&mut self) -> &mut Layer {
        match self.active_target {
            ActivePageTarget::Page(idx) => {
                let page_idx = idx.min(self.document.pages.len().saturating_sub(1));
                if self.document.pages[page_idx].layers.is_empty() {
                    let layer_id = LayerId(self.document.allocate_object_id().0);
                    self.document.pages[page_idx].layers.push(Layer::new_normal(layer_id, "Layer 1"));
                }
                self.document.pages[page_idx].layers.last_mut().unwrap_or_else(|| unreachable!())
            }
            ActivePageTarget::MasterPage => {
                if let Some(pos) = self.document.master_layers.iter().position(|l| l.role == LayerRole::MasterAllPages) {
                    &mut self.document.master_layers[pos]
                } else {
                    let id = LayerId(self.document.allocate_object_id().0);
                    let mut master = Layer::new_normal(id, "Master Page");
                    master.role = LayerRole::MasterAllPages;
                    self.document.master_layers.push(master);
                    self.document.master_layers.last_mut().unwrap_or_else(|| unreachable!())
                }
            }
            ActivePageTarget::Desktop => {
                if let Some(pos) = self.document.master_layers.iter().position(|l| l.role == LayerRole::Desktop) {
                    &mut self.document.master_layers[pos]
                } else {
                    let id = LayerId(self.document.allocate_object_id().0);
                    self.document.master_layers.push(Layer::new_desktop(id));
                    self.document.master_layers.last_mut().unwrap_or_else(|| unreachable!())
                }
            }
        }
    }

    fn for_each_selected_mut(&mut self, mut f: impl FnMut(&mut ObjectNode)) {
        let sel = self.selection.clone();
        for page in &mut self.document.pages {
            for layer in &mut page.layers {
                for obj in &mut layer.objects {
                    if sel.contains(&obj.id) {
                        f(obj);
                    }
                }
            }
        }
        for layer in &mut self.document.master_layers {
            for obj in &mut layer.objects {
                if sel.contains(&obj.id) {
                    f(obj);
                }
            }
        }
    }
}

fn reallocate_object_ids_recursive(node: &mut ObjectNode, doc: &mut Document) {
    node.id = doc.allocate_object_id();
    for child in &mut node.powerclip_children {
        reallocate_object_ids_recursive(child, doc);
    }
    match &mut node.kind {
        ObjectKind::Group { children } => {
            for child in children {
                reallocate_object_ids_recursive(child, doc);
            }
        }
        ObjectKind::Unsupported(u) => {
            for child in &mut u.children {
                reallocate_object_ids_recursive(child, doc);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use coralcraft_tools::PropertyContext;

    #[test]
    fn palette_left_right_click_and_no_color_swatch_update_selection_and_defaults_with_undo() {
        let mut engine = EditorEngine::default();
        assert_eq!(engine.status_bar_snapshot().selection_summary, "No selection");

        // 1. With empty selection, left-click on Red swatch sets default fill for next object
        let red = Color::rgb8(230, 40, 40);
        let blue = Color::rgb8(30, 80, 220);
        engine.execute(EngineCommand::ApplyPaletteSwatch { channel: PaletteChannel::Fill, color: Some(red.clone()) });
        assert_eq!(engine.status_bar_snapshot().fill_chip, Some(red.clone()));

        // 2. Drag-create a rectangle with F6 -> inherits default Red fill
        engine.handle_shortcut(KeyShortcut::F6);
        engine.pointer_down(Point::new(50.0, 60.0).unwrap(), false, false);
        engine.pointer_move(Point::new(150.0, 120.0).unwrap(), false, false);
        engine.pointer_up(Point::new(150.0, 120.0).unwrap(), false, false);
        assert_eq!(engine.selection.len(), 1);
        assert_eq!(engine.selected_nodes()[0].fill, Some(FillStyle::uniform(red.clone())));
        assert_eq!(engine.property_bar_schema().context, PropertyContext::Rectangle);

        // 3. Right-click on Blue swatch sets Outline on selected rectangle
        engine.execute(EngineCommand::ApplyPaletteSwatch { channel: PaletteChannel::Outline, color: Some(blue.clone()) });
        assert_eq!(engine.status_bar_snapshot().outline_chip, Some(blue));

        // 4. Left-click on `[X]` No-Color swatch clears Fill on selected rectangle
        engine.execute(EngineCommand::ApplyPaletteSwatch { channel: PaletteChannel::Fill, color: None });
        assert_eq!(engine.selected_nodes()[0].fill, None);

        // 5. Undo restores the Red fill, Redo clears it again
        assert!(engine.undo());
        assert_eq!(engine.selected_nodes()[0].fill, Some(FillStyle::uniform(red)));
        assert!(engine.redo());
        assert_eq!(engine.selected_nodes()[0].fill, None);
    }

    #[test]
    fn page_navigator_add_duplicate_rename_delete_and_master_desktop_targets() {
        let mut engine = EditorEngine::default();
        assert_eq!(engine.document.pages.len(), 1);

        // Cannot delete the sole remaining numbered page
        assert!(!engine.execute(EngineCommand::DeletePage { page_index: 0 }));
        assert_eq!(engine.document.pages.len(), 1);

        // `+` adds Page 2 after Page 1
        assert!(engine.execute(EngineCommand::AddPageAfterActive));
        assert_eq!(engine.document.pages.len(), 2);
        assert_eq!(engine.active_target, ActivePageTarget::Page(1));
        assert_eq!(engine.current_page().name, "Page 2");

        // Rename Page 2 -> "Back Cover"
        assert!(engine.execute(EngineCommand::RenamePage { page_index: 1, name: "Back Cover".to_owned() }));
        assert_eq!(engine.document.pages[1].name, "Back Cover");

        // Switch to Master Page and place an object -> visible across all pages
        engine.set_active_target(ActivePageTarget::MasterPage);
        engine.handle_shortcut(KeyShortcut::F7);
        engine.pointer_down(Point::new(20.0, 20.0).unwrap(), false, false);
        engine.pointer_up(Point::new(80.0, 80.0).unwrap(), false, false);
        let eff_p1 = engine.document.effective_layers_for_page(0, true).unwrap();
        let eff_p2 = engine.document.effective_layers_for_page(1, true).unwrap();
        assert!(eff_p1.iter().any(|l| l.role == LayerRole::MasterAllPages && !l.objects.is_empty()));
        assert!(eff_p2.iter().any(|l| l.role == LayerRole::MasterAllPages && !l.objects.is_empty()));

        // Switch to Desktop pasteboard and place an object -> visible on screen, non-printing in export
        engine.set_active_target(ActivePageTarget::Desktop);
        engine.handle_shortcut(KeyShortcut::F6);
        engine.pointer_down(Point::new(-100.0, 50.0).unwrap(), false, false);
        engine.pointer_up(Point::new(-20.0, 120.0).unwrap(), false, false);
        let screen_layers = engine.document.effective_layers_for_page(0, false).unwrap();
        let print_layers = engine.document.effective_layers_for_page(0, true).unwrap();
        assert!(screen_layers.iter().any(|l| l.role == LayerRole::Desktop && !l.objects.is_empty()));
        assert!(!print_layers.iter().any(|l| l.role == LayerRole::Desktop));
    }

    #[test]
    fn pick_tool_hit_testing_and_drag_move_selection() {
        let mut engine = EditorEngine::default();
        engine.handle_shortcut(KeyShortcut::F6);
        engine.pointer_down(Point::new(100.0, 100.0).unwrap(), false, false);
        engine.pointer_up(Point::new(200.0, 180.0).unwrap(), false, false);
        let id = engine.selection[0];

        // Return to Pick tool (`Escape`), click inside rectangle, and drag by (+30, +40)
        engine.handle_shortcut(KeyShortcut::Escape);
        assert_eq!(engine.hit_test_point(Point::new(150.0, 140.0).unwrap()), Some(id));
        engine.pointer_down(Point::new(150.0, 140.0).unwrap(), false, false);
        engine.pointer_move(Point::new(180.0, 180.0).unwrap(), false, false);
        engine.pointer_up(Point::new(180.0, 180.0).unwrap(), false, false);

        let moved = engine.selected_nodes()[0];
        assert!((moved.transform.tx.get() - 30.0).abs() < 1e-9);
        assert!((moved.transform.ty.get() - 40.0).abs() < 1e-9);

        // Undo restores identity transform
        assert!(engine.undo());
        let restored = engine.selected_nodes()[0];
        assert!((restored.transform.tx.get()).abs() < 1e-9);
        assert!((restored.transform.ty.get()).abs() < 1e-9);
    }
}
