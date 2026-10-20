//! Core application engine (`EditorEngine`) combining multi-page document state,
//! tool gestures, undo/redo command history, hit-testing, and Property Bar snapshots (L5).

use coralcraft_color::Color;
use coralcraft_doc::{
    Document, FillStyle, Layer, LayerId, LayerRole, ObjectId, ObjectKind, ObjectNode, OutlineStyle, Page, PageId, Residual, TransparencyStyle,
    VectorEffect,
};
use coralcraft_geom::{
    Affine2, BooleanOp, CornerKind, DistortionKind, EllipseMode, F64, FillRule, Path, PathElement, Point, Rect, Size, Unit, artistic_text_to_path,
    boolean_paths, close_open_subpaths, join_open_subpaths, reference_point, reverse_path_direction, trace_bitmap_to_path,
};
use coralcraft_tools::{
    AlignMode, DistributeMode, KeyShortcut, PageOrientation, PolygonSubMode, PropertyBarSchema, ToolKind, ToolState, ZOrderOp,
    build_property_bar_schema, combined_selection_bounds, object_local_bounds,
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
    /// Groups `>= 2` unlocked selected objects on the active layer (`Ctrl+G`).
    GroupSelection,
    /// Ungroups selected `ObjectKind::Group` nodes one level back into the active layer (`Ctrl+U`).
    UngroupSelection,
    /// Converts selected parametric shapes (`Rect`, `Ellipse`, `Polygon`) into editable Bezier paths (`Ctrl+Q`).
    ConvertToCurves,
    /// Reorders selected objects within their layer (`Shift+PageUp`, `Shift+PageDown`, `PageUp`, `PageDown`).
    ReorderSelection { op: ZOrderOp },
    /// Aligns selected objects relative to the anchor object (last selected) or the active page (`L/R/T/B/C/E/P`).
    AlignSelection { mode: AlignMode },
    /// Distributes `>= 3` selected objects evenly along the horizontal or vertical axis.
    DistributeSelection { mode: DistributeMode },
    /// Places selected unlocked objects inside `container_id` as PowerClip children (up to nesting depth 3).
    PlaceInsidePowerClip { container_id: ObjectId },
    /// Extracts all PowerClip children from `container_id` back into the parent layer above `container_id`.
    ExtractPowerClip { container_id: ObjectId },
    /// Moves a single on-curve anchor point of an `ObjectKind::Path` (`F10` Shape Tool).
    MoveBezierNode { object_id: ObjectId, element_index: usize, new_pt: Point },
    /// Executes a Boolean operation between selected objects (Weld, Trim, Intersect, FrontMinusBack, BackMinusFront).
    BooleanSelection { op: BooleanOp },
    /// Applies a non-destructive Contour vector effect to the specified object.
    ApplyContour { object_id: ObjectId, steps: u32, offset_pt: F64, inside: bool },
    /// Applies a non-destructive Drop Shadow vector effect to the specified object.
    AddDropShadow { object_id: ObjectId, offset_x: F64, offset_y: F64, opacity: F64, color: Color },
    /// Applies a non-destructive Extrude vector effect to the specified object.
    AddExtrude { object_id: ObjectId, depth_pt: F64, angle_deg: F64 },
    /// Creates a non-destructive Blend vector effect between two objects.
    CreateBlend { start_id: ObjectId, end_id: ObjectId, steps: u32 },
    /// Attaches a custom Bézier path spine object (`New Path`) to an existing Blend effect.
    AttachBlendSpine { blend_object_id: ObjectId, spine_object_id: ObjectId },
    /// Applies a non-destructive 4-corner bilinear Envelope warp (`corners = [TL, TR, BR, BL]`) to `object_id`.
    ApplyEnvelope { object_id: ObjectId, corners: [Point; 4] },
    /// Applies a non-destructive 2-point vanishing projective Perspective homography (`Effects -> Add Perspective`) to `object_id`.
    ApplyPerspective { object_id: ObjectId, corners: [Point; 4] },
    /// Applies Fillet (`Round`), Scallop (`Scalloped`), or Chamfer (`Chamfered`) corner modification to `object_id`.
    FilletPathCorners { object_id: ObjectId, radius_pt: F64, corner_kind: CornerKind },
    /// Applies a non-destructive Artistic Media variable-width calligraphic brush stroke to `object_id`.
    ApplyArtisticBrush { object_id: ObjectId, start_width_pt: F64, end_width_pt: F64 },
    /// Flows an Artistic Text object (`text_object_id`) along a spine path object (`spine_object_id`) (`Text -> Fit Text to Path`).
    FitTextToPath { text_object_id: ObjectId, spine_object_id: ObjectId, offset_pt: F64 },
    /// Parametric array duplication of selected objects (`Edit -> Step and Repeat` / `Ctrl+Shift+D`).
    StepAndRepeat { count: u32, offset_x_pt: F64, offset_y_pt: F64 },
    /// Bakes (expands) all live vector effects on the specified object into static geometry.
    ExpandEffects { object_id: ObjectId },
    /// Sets or removes object-level transparency on selected objects.
    SetTransparency { transparency: Option<TransparencyStyle> },
    /// Copies Fill, Outline, and/or Vector Effects from `source_id` to all selected objects (`Edit -> Copy Properties From...`).
    CopyPropertiesFrom { source_id: ObjectId, copy_fill: bool, copy_outline: bool, copy_effects: bool },
    /// Combines `>= 2` selected objects into a single compound `ObjectKind::Path` with `FillRule::EvenOdd` (`Object -> Combine` / `Ctrl+L`).
    CombineSelection,
    /// Splits compound `ObjectKind::Path` objects at each `MoveTo` boundary into independent `ObjectNode`s (`Object -> Break Apart` / `Ctrl+K`).
    BreakApartSelection,
    /// Applies non-destructive/undoable per-pixel brightness, contrast, BT.709 grayscale, and color inversion to an `ObjectKind::Bitmap` (`Bitmaps -> Mode / Effects -> Adjust`).
    AdjustBitmap { object_id: ObjectId, brightness: F64, contrast: F64, grayscale: bool, invert: bool },
    /// Applies a non-destructive Distortion effect (`PushPull`, `Zipper`, or `Twister`) to `object_id` (`Effects -> Distort`).
    ApplyDistortion { object_id: ObjectId, kind: DistortionKind, amplitude: F64, frequency: F64 },
    /// Traces an `ObjectKind::Bitmap` into a closed vector `ObjectKind::Path` (`Bitmaps -> Outline Trace`).
    TraceBitmap { object_id: ObjectId, threshold: u8 },
    /// Mirrors selected objects horizontally and/or vertically around their combined center, optionally duplicating (`Object -> Transformations -> Scale and Mirror`).
    MirrorSelection { horizontal: bool, vertical: bool, copy: bool },
    /// Creates a new closed union silhouette `ObjectNode` around selected objects without deleting originals (`Object -> Shaping -> Boundary`).
    CreateBoundary,
    /// Converts each selected object's `OutlineStyle` stroke into a closed filled `ObjectKind::Path` (`Object -> Convert Outline to Object` / `Ctrl+Shift+Q`).
    ConvertOutlineToObject,
    /// Rotates selected objects by `angle_deg` around the 9-point Object Origin (`origin_index` in `0..=8`), optionally duplicating (`Alt+F8`).
    RotateSelection { angle_deg: F64, origin_index: u8, copy: bool },
    /// Scales selected objects by `(scale_x, scale_y)` around the 9-point Object Origin (`origin_index` in `0..=8`), optionally duplicating (`Alt+F9`).
    ScaleSelection { scale_x: F64, scale_y: F64, origin_index: u8, copy: bool },
    /// Skews selected objects by `(skew_x_deg, skew_y_deg)` around the 9-point Object Origin (`origin_index` in `0..=8`), optionally duplicating (`Alt+F11`).
    SkewSelection { skew_x_deg: F64, skew_y_deg: F64, origin_index: u8, copy: bool },
    /// Duplicates selected objects with offset `(offset_x_pt, offset_y_pt)` and selects the duplicates (`Edit -> Duplicate` / `Ctrl+D`).
    DuplicateSelection { offset_x_pt: F64, offset_y_pt: F64 },
    /// Reverses the winding direction of all subpaths in `object_id` (`Shape Tool -> Reverse Subpaths`).
    ReversePathDirection { object_id: ObjectId },
    /// Closes all open subpaths in `object_id` (`Object -> Join Curves -> Auto-Close Curve`).
    CloseOpenSubpaths { object_id: ObjectId },
    /// Locks or unlocks `object_id` (`Object -> Lock -> Lock / Unlock`).
    SetObjectLocked { object_id: ObjectId, locked: bool },
    /// Unlocks all locked objects across all layers (`Object -> Lock -> Unlock All Objects`).
    UnlockAllObjects,
    /// Joins open subpaths in `object_id` whose endpoints lie within `tolerance_pt` (`Object -> Join Curves` / `Ctrl+Shift+J`).
    JoinOpenSubpaths { object_id: ObjectId, tolerance_pt: F64 },
    /// Recursively ungroups all selected groups and nested subgroups down to leaf objects (`Object -> Group -> Ungroup All`).
    UngroupAllSelection,
    /// Moves the 0-based page at `from_index` to `to_index` and activates `to_index`.
    MovePage { from_index: usize, to_index: usize },
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
                ObjectKind::ParagraphText(_) => "Paragraph Text on Layer".to_owned(),
                ObjectKind::Group { children } => format!("Group of {} objects", children.len()),
                ObjectKind::Bitmap { pixel_width, pixel_height, .. } => format!("Bitmap ({pixel_width}x{pixel_height}) on Layer"),
                ObjectKind::Unsupported(_) => "Unsupported CDR Object".to_owned(),
            },
            many => format!("{} objects selected", many.len()),
        };

        let fill_source = selected.first().map_or(self.default_fill.as_ref(), |n| n.fill.as_ref());
        let outline_source = selected.first().map_or(self.default_outline.as_ref(), |n| n.outline.as_ref());

        let fill_chip = match fill_source {
            Some(FillStyle::Uniform { color }) => Some(color.clone()),
            Some(FillStyle::Fountain { stops, .. }) => stops.first().map(|s| s.color.clone()),
            Some(FillStyle::TwoColorPattern { fg_color, .. }) => Some(fg_color.clone()),
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
            ToolKind::InteractiveFill => {
                if self.selection.is_empty()
                    && let Some(hit_id) = self.hit_test_point(pt)
                {
                    self.selection = vec![hit_id];
                }
                self.tool_state.begin_drag(pt, shift_modifier, alt_modifier);
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

        if let Some(coralcraft_tools::GestureState::DragInteractiveFill { start, current }) = self.tool_state.gesture.clone() {
            self.tool_state.gesture = None;
            let dx = current.x.get() - start.x.get();
            let dy = current.y.get() - start.y.get();
            if (dx.abs() > 1e-6 || dy.abs() > 1e-6) && !self.selection.is_empty() {
                let angle_deg = dy.atan2(dx).to_degrees();
                let (c0, c1) = self.current_target_objects().iter().find(|o| self.selection.contains(&o.id)).map_or(
                    (Color::rgb8(0, 120, 215), Color::WHITE),
                    |o| match &o.fill {
                        Some(FillStyle::Fountain { stops, .. }) if stops.len() >= 2 => (stops[0].color.clone(), stops[stops.len() - 1].color.clone()),
                        Some(FillStyle::Uniform { color }) => (color.clone(), Color::WHITE),
                        _ => (Color::rgb8(0, 120, 215), Color::WHITE),
                    },
                );
                let mut stops = Vec::with_capacity(2);
                if let Ok(st0) = coralcraft_color::FountainStop::new(0.0, c0) {
                    stops.push(st0);
                }
                if let Ok(st1) = coralcraft_color::FountainStop::new(1.0, c1) {
                    stops.push(st1);
                }
                let fill = FillStyle::Fountain {
                    geometry: coralcraft_doc::FountainGeometry::Linear { angle_deg: F64::new(angle_deg).unwrap_or(F64::ZERO), edge_pad: F64::ZERO },
                    blend_mode: coralcraft_color::FountainBlendMode::Direct,
                    stops,
                };
                self.execute(EngineCommand::SetFill { fill: Some(fill) });
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
            EngineCommand::GroupSelection => {
                let sel = &self.selection;
                let count = self.current_target_objects().iter().filter(|o| !o.locked && sel.contains(&o.id)).count();
                if count < 2 {
                    return false;
                }
            }
            EngineCommand::UngroupSelection => {
                let sel = &self.selection;
                let has_group =
                    self.current_target_objects().iter().any(|o| !o.locked && sel.contains(&o.id) && matches!(o.kind, ObjectKind::Group { .. }));
                if !has_group {
                    return false;
                }
            }
            EngineCommand::ConvertToCurves => {
                let has_convertible = self.selected_nodes().into_iter().any(|o| {
                    !o.locked
                        && matches!(
                            o.kind,
                            ObjectKind::Rect(_)
                                | ObjectKind::Ellipse(_)
                                | ObjectKind::Polygon(_)
                                | ObjectKind::ArtisticText { .. }
                                | ObjectKind::ParagraphText(_)
                        )
                });
                if !has_convertible {
                    return false;
                }
            }
            EngineCommand::BooleanSelection { .. } => {
                let sel = &self.selection;
                let count = self.current_target_objects().iter().filter(|o| !o.locked && sel.contains(&o.id)).count();
                if count < 2 {
                    return false;
                }
            }
            EngineCommand::ApplyContour { object_id, .. }
            | EngineCommand::AddDropShadow { object_id, .. }
            | EngineCommand::AddExtrude { object_id, .. }
            | EngineCommand::ApplyEnvelope { object_id, .. }
            | EngineCommand::ApplyPerspective { object_id, .. }
            | EngineCommand::ApplyDistortion { object_id, .. }
            | EngineCommand::FilletPathCorners { object_id, .. }
            | EngineCommand::ApplyArtisticBrush { object_id, .. }
            | EngineCommand::ExpandEffects { object_id } => {
                if self.selected_or_all_node(*object_id).is_none_or(|o| o.locked) {
                    return false;
                }
            }
            EngineCommand::CreateBlend { start_id, end_id, .. } => {
                let has_start = self.selected_or_all_node(*start_id).is_some_and(|o| !o.locked);
                let has_end = self.selected_or_all_node(*end_id).is_some_and(|o| !o.locked);
                if !has_start || !has_end || start_id == end_id {
                    return false;
                }
            }
            EngineCommand::AttachBlendSpine { blend_object_id, spine_object_id } => {
                let has_blend = self
                    .selected_or_all_node(*blend_object_id)
                    .is_some_and(|o| !o.locked && o.effects.iter().any(|e| matches!(e, VectorEffect::Blend { .. })));
                let has_spine = self.selected_or_all_node(*spine_object_id).and_then(object_to_path).is_some();
                if !has_blend || !has_spine || blend_object_id == spine_object_id {
                    return false;
                }
            }
            EngineCommand::FitTextToPath { text_object_id, spine_object_id, .. } => {
                let has_text =
                    self.selected_or_all_node(*text_object_id).is_some_and(|o| !o.locked && matches!(o.kind, ObjectKind::ArtisticText { .. }));
                let has_spine = self.selected_or_all_node(*spine_object_id).and_then(object_to_path).is_some();
                if !has_text || !has_spine || text_object_id == spine_object_id {
                    return false;
                }
            }
            EngineCommand::StepAndRepeat { count, .. } => {
                let sel = &self.selection;
                let has_unlocked = self.current_target_objects().iter().any(|o| !o.locked && sel.contains(&o.id));
                if *count == 0 || !has_unlocked {
                    return false;
                }
            }
            EngineCommand::CopyPropertiesFrom { source_id, copy_fill, copy_outline, copy_effects } => {
                if (!*copy_fill && !*copy_outline && !*copy_effects) || self.selected_or_all_node(*source_id).is_none() {
                    return false;
                }
                let sel = &self.selection;
                let has_target = self.current_target_objects().iter().any(|o| !o.locked && o.id != *source_id && sel.contains(&o.id));
                if !has_target {
                    return false;
                }
            }
            EngineCommand::CombineSelection => {
                let sel = &self.selection;
                let count = self.current_target_objects().iter().filter(|o| !o.locked && sel.contains(&o.id) && object_to_path(o).is_some()).count();
                if count < 2 {
                    return false;
                }
            }
            EngineCommand::BreakApartSelection => {
                let sel = &self.selection;
                let has_compound = self.current_target_objects().iter().any(|o| {
                    !o.locked
                        && sel.contains(&o.id)
                        && matches!(&o.kind, ObjectKind::Path(p) if p.elements.iter().filter(|e| matches!(e, PathElement::MoveTo(_))).count() >= 2)
                });
                if !has_compound {
                    return false;
                }
            }
            EngineCommand::AdjustBitmap { object_id, .. } | EngineCommand::TraceBitmap { object_id, .. } => {
                let is_bmp = self.selected_or_all_node(*object_id).is_some_and(|o| !o.locked && matches!(o.kind, ObjectKind::Bitmap { .. }));
                if !is_bmp {
                    return false;
                }
            }
            EngineCommand::MirrorSelection { horizontal, vertical, .. } => {
                if !*horizontal && !*vertical {
                    return false;
                }
                let sel = &self.selection;
                let has_unlocked = self.current_target_objects().iter().any(|o| !o.locked && sel.contains(&o.id));
                if !has_unlocked {
                    return false;
                }
            }
            EngineCommand::CreateBoundary => {
                let sel = &self.selection;
                let has_path = self.current_target_objects().iter().any(|o| !o.locked && sel.contains(&o.id) && object_to_path(o).is_some());
                if !has_path {
                    return false;
                }
            }
            EngineCommand::ConvertOutlineToObject => {
                let sel = &self.selection;
                let has_outlined = self
                    .current_target_objects()
                    .iter()
                    .any(|o| !o.locked && sel.contains(&o.id) && o.outline.is_some() && object_to_path(o).is_some());
                if !has_outlined {
                    return false;
                }
            }
            EngineCommand::RotateSelection { angle_deg, origin_index, .. } => {
                if *origin_index > 8 || angle_deg.get().abs() <= 1e-9 {
                    return false;
                }
                let sel = &self.selection;
                let has_unlocked = self.current_target_objects().iter().any(|o| !o.locked && sel.contains(&o.id));
                if !has_unlocked {
                    return false;
                }
            }
            EngineCommand::ScaleSelection { scale_x, scale_y, origin_index, .. } => {
                if *origin_index > 8 || scale_x.get().abs() <= 1e-6 || scale_y.get().abs() <= 1e-6 {
                    return false;
                }
                let sel = &self.selection;
                let has_unlocked = self.current_target_objects().iter().any(|o| !o.locked && sel.contains(&o.id));
                if !has_unlocked {
                    return false;
                }
            }
            EngineCommand::SkewSelection { skew_x_deg, skew_y_deg, origin_index, .. } => {
                let sx = skew_x_deg.get();
                let sy = skew_y_deg.get();
                if *origin_index > 8
                    || sx.abs() >= 85.0
                    || sy.abs() >= 85.0
                    || (sx.abs() <= 1e-9 && sy.abs() <= 1e-9)
                    || Affine2::skew_deg(sx, sy).is_err_and(|_| true)
                {
                    return false;
                }
                let sel = &self.selection;
                let has_unlocked = self.current_target_objects().iter().any(|o| !o.locked && sel.contains(&o.id));
                if !has_unlocked {
                    return false;
                }
            }
            EngineCommand::DuplicateSelection { offset_x_pt, offset_y_pt } => {
                if Affine2::translation(offset_x_pt.get(), offset_y_pt.get()).is_err() {
                    return false;
                }
                let sel = &self.selection;
                let has_unlocked = self.current_target_objects().iter().any(|o| !o.locked && sel.contains(&o.id));
                if !has_unlocked {
                    return false;
                }
            }
            EngineCommand::ReversePathDirection { object_id } | EngineCommand::CloseOpenSubpaths { object_id } => {
                let valid = self.selected_or_all_node(*object_id).is_some_and(|o| !o.locked && object_to_path(o).is_some());
                if !valid {
                    return false;
                }
            }
            EngineCommand::JoinOpenSubpaths { object_id, tolerance_pt } => {
                if tolerance_pt.get() < 0.0 {
                    return false;
                }
                let valid = self.selected_or_all_node(*object_id).is_some_and(|o| !o.locked && object_to_path(o).is_some());
                if !valid {
                    return false;
                }
            }
            EngineCommand::UngroupAllSelection => {
                let sel = &self.selection;
                let has_group =
                    self.current_target_objects().iter().any(|o| !o.locked && sel.contains(&o.id) && matches!(o.kind, ObjectKind::Group { .. }));
                if !has_group {
                    return false;
                }
            }
            EngineCommand::MovePage { from_index, to_index } => {
                let len = self.document.pages.len();
                if *from_index >= len || *to_index >= len || *from_index == *to_index {
                    return false;
                }
            }
            EngineCommand::SetObjectLocked { object_id, .. } => {
                if self.selected_or_all_node(*object_id).is_none() {
                    return false;
                }
            }
            EngineCommand::UnlockAllObjects => {
                let mut any_locked = false;
                for page in &self.document.pages {
                    for layer in &page.layers {
                        if layer.objects.iter().any(|o| o.locked) {
                            any_locked = true;
                        }
                    }
                }
                for layer in &self.document.master_layers {
                    if layer.objects.iter().any(|o| o.locked) {
                        any_locked = true;
                    }
                }
                if !any_locked {
                    return false;
                }
            }
            EngineCommand::ReorderSelection { .. } if self.selection.is_empty() => {
                return false;
            }
            EngineCommand::AlignSelection { mode } => {
                let min_required = if *mode == AlignMode::CenterToPage { 1 } else { 2 };
                if self.selection.len() < min_required {
                    return false;
                }
            }
            EngineCommand::DistributeSelection { .. } => {
                let sel = &self.selection;
                let count = self.current_target_objects().iter().filter(|o| !o.locked && sel.contains(&o.id)).count();
                if count < 3 {
                    return false;
                }
            }
            EngineCommand::PlaceInsidePowerClip { container_id } => {
                let sel = &self.selection;
                let objs = self.current_target_objects();
                let Some(container) = objs.iter().find(|o| o.id == *container_id && !o.locked) else {
                    return false;
                };
                let candidates: Vec<&ObjectNode> = objs.iter().filter(|o| !o.locked && o.id != *container_id && sel.contains(&o.id)).collect();
                if candidates.is_empty() {
                    return false;
                }
                for c in candidates {
                    if powerclip_subtree_depth(container).max(powerclip_subtree_depth(c) + 1) > 3 {
                        return false;
                    }
                }
            }
            EngineCommand::ExtractPowerClip { container_id } => {
                let objs = self.current_target_objects();
                let Some(container) = objs.iter().find(|o| o.id == *container_id && !o.locked) else {
                    return false;
                };
                if container.powerclip_children.is_empty() {
                    return false;
                }
            }
            EngineCommand::MoveBezierNode { object_id, element_index, .. } => {
                let valid = self.selected_or_all_node(*object_id).is_some_and(|o| {
                    !o.locked
                        && match &o.kind {
                            ObjectKind::Path(p) => {
                                p.elements.get(*element_index).is_some_and(|el| !matches!(el, coralcraft_geom::PathElement::Close))
                            }
                            _ => false,
                        }
                });
                if !valid {
                    return false;
                }
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
            EngineCommand::GroupSelection => {
                let sel = self.selection.clone();
                let group_id = self.document.allocate_object_id();
                let layer = self.target_layer_mut();
                let mut children = Vec::new();
                let mut remaining = Vec::with_capacity(layer.objects.len());
                let mut insert_idx = 0usize;
                for obj in layer.objects.drain(..) {
                    if !obj.locked && sel.contains(&obj.id) {
                        insert_idx = remaining.len();
                        children.push(obj);
                    } else {
                        remaining.push(obj);
                    }
                }
                let group_node = ObjectNode::new(group_id, Some("Group".to_owned()), ObjectKind::Group { children }, None, None);
                remaining.insert(insert_idx.min(remaining.len()), group_node);
                layer.objects = remaining;
                self.selection = vec![group_id];
            }
            EngineCommand::UngroupSelection => {
                let sel = self.selection.clone();
                let layer = self.target_layer_mut();
                let mut new_objects = Vec::with_capacity(layer.objects.len());
                let mut new_selection = Vec::new();
                for obj in layer.objects.drain(..) {
                    if !obj.locked && sel.contains(&obj.id) {
                        if let ObjectKind::Group { children } = obj.kind {
                            let group_tf = obj.transform;
                            for mut child in children {
                                if let Ok(composed) = child.transform.then(group_tf) {
                                    child.transform = composed;
                                }
                                new_selection.push(child.id);
                                new_objects.push(child);
                            }
                        } else {
                            new_objects.push(obj);
                        }
                    } else {
                        new_objects.push(obj);
                    }
                }
                layer.objects = new_objects;
                self.selection = new_selection;
            }
            EngineCommand::ConvertToCurves => {
                self.for_each_selected_mut(|obj| {
                    if obj.locked {
                        return;
                    }
                    if let Some(path) = object_to_path(obj) {
                        obj.kind = ObjectKind::Path(path);
                    }
                });
            }
            EngineCommand::ReorderSelection { op } => {
                let sel = self.selection.clone();
                let layer = self.target_layer_mut();
                match op {
                    ZOrderOp::ToFront => {
                        let mut unselected = Vec::with_capacity(layer.objects.len());
                        let mut selected = Vec::new();
                        for obj in layer.objects.drain(..) {
                            if !obj.locked && sel.contains(&obj.id) {
                                selected.push(obj);
                            } else {
                                unselected.push(obj);
                            }
                        }
                        unselected.extend(selected);
                        layer.objects = unselected;
                    }
                    ZOrderOp::ToBack => {
                        let mut selected = Vec::new();
                        let mut unselected = Vec::with_capacity(layer.objects.len());
                        for obj in layer.objects.drain(..) {
                            if !obj.locked && sel.contains(&obj.id) {
                                selected.push(obj);
                            } else {
                                unselected.push(obj);
                            }
                        }
                        selected.extend(unselected);
                        layer.objects = selected;
                    }
                    ZOrderOp::ForwardOne => {
                        let len = layer.objects.len();
                        if len >= 2 {
                            for i in (0..len - 1).rev() {
                                let is_sel = !layer.objects[i].locked && sel.contains(&layer.objects[i].id);
                                let next_sel = sel.contains(&layer.objects[i + 1].id);
                                if is_sel && !next_sel {
                                    layer.objects.swap(i, i + 1);
                                }
                            }
                        }
                    }
                    ZOrderOp::BackwardOne => {
                        let len = layer.objects.len();
                        if len >= 2 {
                            for i in 1..len {
                                let is_sel = !layer.objects[i].locked && sel.contains(&layer.objects[i].id);
                                let prev_sel = sel.contains(&layer.objects[i - 1].id);
                                if is_sel && !prev_sel {
                                    layer.objects.swap(i - 1, i);
                                }
                            }
                        }
                    }
                }
            }
            EngineCommand::AlignSelection { mode } => {
                if mode == AlignMode::CenterToPage {
                    let ps = self.current_page().size;
                    let nodes = self.selected_nodes();
                    let sb = combined_selection_bounds(&nodes);
                    let dx = ps.width.get() * 0.5 - (sb.x0.get() + sb.x1.get()) * 0.5;
                    let dy = ps.height.get() * 0.5 - (sb.y0.get() + sb.y1.get()) * 0.5;
                    if let Ok(t) = Affine2::translation(dx, dy) {
                        self.for_each_selected_mut(|obj| {
                            if !obj.locked
                                && let Ok(composed) = obj.transform.then(t)
                            {
                                obj.transform = composed;
                            }
                        });
                    }
                } else if let Some(&anchor_id) = self.selection.last()
                    && let Some(anchor_node) = self.selected_or_all_node(anchor_id)
                {
                    let anchor_bbox = combined_selection_bounds(&[anchor_node]);
                    let ax_mid = (anchor_bbox.x0.get() + anchor_bbox.x1.get()) * 0.5;
                    let ay_mid = (anchor_bbox.y0.get() + anchor_bbox.y1.get()) * 0.5;
                    self.for_each_selected_mut(|obj| {
                        if obj.locked || obj.id == anchor_id {
                            return;
                        }
                        let bbox = combined_selection_bounds(&[obj]);
                        let bx_mid = (bbox.x0.get() + bbox.x1.get()) * 0.5;
                        let by_mid = (bbox.y0.get() + bbox.y1.get()) * 0.5;
                        let (dx, dy) = match mode {
                            AlignMode::Left => (anchor_bbox.x0.get() - bbox.x0.get(), 0.0),
                            AlignMode::Right => (anchor_bbox.x1.get() - bbox.x1.get(), 0.0),
                            AlignMode::Top => (0.0, anchor_bbox.y0.get() - bbox.y0.get()),
                            AlignMode::Bottom => (0.0, anchor_bbox.y1.get() - bbox.y1.get()),
                            AlignMode::CenterHorizontal => (ax_mid - bx_mid, 0.0),
                            AlignMode::CenterVertical => (0.0, ay_mid - by_mid),
                            AlignMode::CenterToPage => (0.0, 0.0),
                        };
                        if let Ok(t) = Affine2::translation(dx, dy)
                            && let Ok(composed) = obj.transform.then(t)
                        {
                            obj.transform = composed;
                        }
                    });
                }
            }
            EngineCommand::DistributeSelection { mode } => {
                let sel = &self.selection;
                let mut items: Vec<(ObjectId, Rect)> = self
                    .current_target_objects()
                    .iter()
                    .filter(|o| !o.locked && sel.contains(&o.id))
                    .map(|o| (o.id, combined_selection_bounds(&[o])))
                    .collect();
                let n = items.len();
                if n >= 3 {
                    let is_horizontal = matches!(
                        mode,
                        DistributeMode::Left | DistributeMode::CenterHorizontal | DistributeMode::Right | DistributeMode::SpacingHorizontal
                    );
                    if is_horizontal {
                        items.sort_by_key(|a| a.1.x0);
                    } else {
                        items.sort_by_key(|a| a.1.y0);
                    }
                    let mut deltas = BTreeMap::new();
                    let denom = (n - 1) as f64;
                    match mode {
                        DistributeMode::Left => {
                            let start = items[0].1.x0.get();
                            let span = items[n - 1].1.x0.get() - start;
                            for (k, (id, b)) in items.iter().enumerate().take(n - 1).skip(1) {
                                deltas.insert(*id, (start + (k as f64 / denom) * span - b.x0.get(), 0.0));
                            }
                        }
                        DistributeMode::CenterHorizontal => {
                            let start = (items[0].1.x0.get() + items[0].1.x1.get()) * 0.5;
                            let end = (items[n - 1].1.x0.get() + items[n - 1].1.x1.get()) * 0.5;
                            let span = end - start;
                            for (k, (id, b)) in items.iter().enumerate().take(n - 1).skip(1) {
                                let cur = (b.x0.get() + b.x1.get()) * 0.5;
                                deltas.insert(*id, (start + (k as f64 / denom) * span - cur, 0.0));
                            }
                        }
                        DistributeMode::Right => {
                            let start = items[0].1.x1.get();
                            let span = items[n - 1].1.x1.get() - start;
                            for (k, (id, b)) in items.iter().enumerate().take(n - 1).skip(1) {
                                deltas.insert(*id, (start + (k as f64 / denom) * span - b.x1.get(), 0.0));
                            }
                        }
                        DistributeMode::SpacingHorizontal => {
                            let min_x = items[0].1.x0.get();
                            let max_x = items[n - 1].1.x1.get();
                            let total_w: f64 = items.iter().map(|(_, b)| b.width()).sum();
                            let gap = (max_x - min_x - total_w) / denom;
                            let mut cur_x = min_x + items[0].1.width() + gap;
                            for (id, b) in items.iter().take(n - 1).skip(1) {
                                deltas.insert(*id, (cur_x - b.x0.get(), 0.0));
                                cur_x += b.width() + gap;
                            }
                        }
                        DistributeMode::Top => {
                            let start = items[0].1.y0.get();
                            let span = items[n - 1].1.y0.get() - start;
                            for (k, (id, b)) in items.iter().enumerate().take(n - 1).skip(1) {
                                deltas.insert(*id, (0.0, start + (k as f64 / denom) * span - b.y0.get()));
                            }
                        }
                        DistributeMode::CenterVertical => {
                            let start = (items[0].1.y0.get() + items[0].1.y1.get()) * 0.5;
                            let end = (items[n - 1].1.y0.get() + items[n - 1].1.y1.get()) * 0.5;
                            let span = end - start;
                            for (k, (id, b)) in items.iter().enumerate().take(n - 1).skip(1) {
                                let cur = (b.y0.get() + b.y1.get()) * 0.5;
                                deltas.insert(*id, (0.0, start + (k as f64 / denom) * span - cur));
                            }
                        }
                        DistributeMode::Bottom => {
                            let start = items[0].1.y1.get();
                            let span = items[n - 1].1.y1.get() - start;
                            for (k, (id, b)) in items.iter().enumerate().take(n - 1).skip(1) {
                                deltas.insert(*id, (0.0, start + (k as f64 / denom) * span - b.y1.get()));
                            }
                        }
                        DistributeMode::SpacingVertical => {
                            let min_y = items[0].1.y0.get();
                            let max_y = items[n - 1].1.y1.get();
                            let total_h: f64 = items.iter().map(|(_, b)| b.height()).sum();
                            let gap = (max_y - min_y - total_h) / denom;
                            let mut cur_y = min_y + items[0].1.height() + gap;
                            for (id, b) in items.iter().take(n - 1).skip(1) {
                                deltas.insert(*id, (0.0, cur_y - b.y0.get()));
                                cur_y += b.height() + gap;
                            }
                        }
                    }
                    self.for_each_selected_mut(|obj| {
                        if let Some(&(dx, dy)) = deltas.get(&obj.id)
                            && let Ok(t) = Affine2::translation(dx, dy)
                            && let Ok(composed) = obj.transform.then(t)
                        {
                            obj.transform = composed;
                        }
                    });
                }
            }
            EngineCommand::PlaceInsidePowerClip { container_id } => {
                let sel = self.selection.clone();
                let layer = self.target_layer_mut();
                let mut extracted = Vec::new();
                let mut remaining = Vec::with_capacity(layer.objects.len());
                for obj in layer.objects.drain(..) {
                    if !obj.locked && obj.id != container_id && sel.contains(&obj.id) {
                        extracted.push(obj);
                    } else {
                        remaining.push(obj);
                    }
                }
                if let Some(container) = remaining.iter_mut().find(|o| o.id == container_id) {
                    container.powerclip_children.extend(extracted);
                }
                layer.objects = remaining;
                self.selection = vec![container_id];
            }
            EngineCommand::ExtractPowerClip { container_id } => {
                let layer = self.target_layer_mut();
                let mut new_objects = Vec::with_capacity(layer.objects.len());
                let mut extracted_ids = Vec::new();
                for mut obj in layer.objects.drain(..) {
                    if obj.id == container_id && !obj.locked {
                        let children: Vec<ObjectNode> = obj.powerclip_children.drain(..).collect();
                        new_objects.push(obj);
                        for child in children {
                            extracted_ids.push(child.id);
                            new_objects.push(child);
                        }
                    } else {
                        new_objects.push(obj);
                    }
                }
                layer.objects = new_objects;
                self.selection = extracted_ids;
            }
            EngineCommand::MoveBezierNode { object_id, element_index, new_pt } => {
                self.for_each_node_mut(|obj| {
                    if obj.id != object_id || obj.locked {
                        return;
                    }
                    let inv = obj.transform.invert().unwrap_or(Affine2::IDENTITY);
                    let Ok(local_pt) = inv.apply_point(new_pt) else {
                        return;
                    };
                    if let ObjectKind::Path(path) = &mut obj.kind
                        && let Some(el) = path.elements.get_mut(element_index)
                    {
                        match el {
                            coralcraft_geom::PathElement::MoveTo(pt) | coralcraft_geom::PathElement::LineTo(pt) => {
                                *pt = local_pt;
                            }
                            coralcraft_geom::PathElement::QuadTo { ctrl, end } => {
                                let dx = local_pt.x.get() - end.x.get();
                                let dy = local_pt.y.get() - end.y.get();
                                if let Ok(new_ctrl) = Point::new(ctrl.x.get() + dx, ctrl.y.get() + dy) {
                                    *ctrl = new_ctrl;
                                }
                                *end = local_pt;
                            }
                            coralcraft_geom::PathElement::CubicTo { c2, end, .. } => {
                                let dx = local_pt.x.get() - end.x.get();
                                let dy = local_pt.y.get() - end.y.get();
                                if let Ok(new_c2) = Point::new(c2.x.get() + dx, c2.y.get() + dy) {
                                    *c2 = new_c2;
                                }
                                *end = local_pt;
                            }
                            coralcraft_geom::PathElement::Close => {}
                        }
                    }
                });
            }
            EngineCommand::BooleanSelection { op } => {
                let new_id = self.document.allocate_object_id();
                let sel = self.selection.clone();
                let layer = self.target_layer_mut();
                let mut sel_indices: Vec<usize> = Vec::new();
                for (idx, obj) in layer.objects.iter().enumerate() {
                    if !obj.locked && sel.contains(&obj.id) {
                        sel_indices.push(idx);
                    }
                }
                if sel_indices.len() < 2 {
                    return false;
                }

                let first_idx = sel_indices[0];
                let anchor_idx = *sel_indices.last().unwrap_or(&first_idx);
                let anchor_style_fill = layer.objects[anchor_idx].fill.clone();
                let anchor_style_outline = layer.objects[anchor_idx].outline.clone();

                let mut combined_path = match object_to_path(&layer.objects[first_idx]) {
                    Some(p) => p,
                    None => return false,
                };

                for &idx in &sel_indices[1..] {
                    if let Some(p) = object_to_path(&layer.objects[idx])
                        && let Ok(merged) = boolean_paths(&combined_path, &p, op)
                    {
                        combined_path = merged;
                    }
                }

                let new_node = ObjectNode::new(
                    new_id,
                    Some(format!("{:?} Result", op)),
                    ObjectKind::Path(combined_path),
                    anchor_style_fill,
                    anchor_style_outline,
                );

                match op {
                    BooleanOp::Trim => {
                        layer.objects[anchor_idx] = new_node;
                        self.selection = vec![new_id];
                    }
                    _ => {
                        let mut remaining = Vec::with_capacity(layer.objects.len());
                        let mut inserted = false;
                        for (idx, obj) in layer.objects.drain(..).enumerate() {
                            if sel_indices.contains(&idx) {
                                if !inserted {
                                    remaining.push(new_node.clone());
                                    inserted = true;
                                }
                            } else {
                                remaining.push(obj);
                            }
                        }
                        layer.objects = remaining;
                        self.selection = vec![new_id];
                    }
                }
            }
            EngineCommand::ApplyContour { object_id, steps, offset_pt, inside } => {
                let effect = VectorEffect::Contour { steps, offset_pt, inside };
                self.for_each_node_mut(|obj| {
                    if obj.id == object_id && !obj.locked {
                        obj.effects.push(effect.clone());
                    }
                });
            }
            EngineCommand::AddDropShadow { object_id, offset_x, offset_y, opacity, color } => {
                let effect = VectorEffect::DropShadow { offset_x, offset_y, opacity, color };
                self.for_each_node_mut(|obj| {
                    if obj.id == object_id && !obj.locked {
                        obj.effects.push(effect.clone());
                    }
                });
            }
            EngineCommand::AddExtrude { object_id, depth_pt, angle_deg } => {
                let effect = VectorEffect::Extrude { depth_pt, angle_deg };
                self.for_each_node_mut(|obj| {
                    if obj.id == object_id && !obj.locked {
                        obj.effects.push(effect.clone());
                    }
                });
            }
            EngineCommand::CreateBlend { start_id, end_id, steps } => {
                let effect = VectorEffect::Blend { target_id: end_id, steps, spine: None };
                self.for_each_node_mut(|obj| {
                    if obj.id == start_id && !obj.locked {
                        obj.effects.push(effect.clone());
                    }
                });
            }
            EngineCommand::AttachBlendSpine { blend_object_id, spine_object_id } => {
                let Some(spine_path) =
                    self.selected_or_all_node(spine_object_id).and_then(|o| object_to_path(o).and_then(|p| p.transform(o.transform).ok()))
                else {
                    return false;
                };
                self.for_each_node_mut(|obj| {
                    if obj.id == blend_object_id && !obj.locked {
                        for effect in &mut obj.effects {
                            if let VectorEffect::Blend { spine, .. } = effect {
                                *spine = Some(spine_path.clone());
                            }
                        }
                    }
                });
            }
            EngineCommand::ApplyEnvelope { object_id, corners } => {
                let effect = VectorEffect::Envelope { corners };
                self.for_each_node_mut(|obj| {
                    if obj.id == object_id && !obj.locked {
                        obj.effects.push(effect.clone());
                    }
                });
            }
            EngineCommand::ApplyPerspective { object_id, corners } => {
                let effect = VectorEffect::Perspective { corners };
                self.for_each_node_mut(|obj| {
                    if obj.id == object_id && !obj.locked {
                        obj.effects.push(effect.clone());
                    }
                });
            }
            EngineCommand::FilletPathCorners { object_id, radius_pt, corner_kind } => {
                self.for_each_node_mut(|obj| {
                    if obj.id == object_id
                        && !obj.locked
                        && let Some(base_path) = object_to_path(obj)
                        && let Ok(modified) = coralcraft_geom::fillet_path_corners(&base_path, radius_pt.get(), corner_kind)
                    {
                        obj.kind = ObjectKind::Path(modified);
                    }
                });
            }
            EngineCommand::ApplyArtisticBrush { object_id, start_width_pt, end_width_pt } => {
                let effect = VectorEffect::ArtisticMediaBrush { start_width_pt, end_width_pt };
                self.for_each_node_mut(|obj| {
                    if obj.id == object_id && !obj.locked {
                        obj.effects.push(effect.clone());
                    }
                });
            }
            EngineCommand::FitTextToPath { text_object_id, spine_object_id, offset_pt } => {
                let Some(spine_path) =
                    self.selected_or_all_node(spine_object_id).and_then(|o| object_to_path(o).and_then(|p| p.transform(o.transform).ok()))
                else {
                    return false;
                };
                let effect = VectorEffect::TextOnPath { spine: spine_path, offset_pt };
                self.for_each_node_mut(|obj| {
                    if obj.id == text_object_id && !obj.locked {
                        obj.effects.retain(|e| !matches!(e, VectorEffect::TextOnPath { .. }));
                        obj.effects.push(effect.clone());
                    }
                });
            }
            EngineCommand::StepAndRepeat { count, offset_x_pt, offset_y_pt } => {
                let sel = self.selection.clone();
                let clamped_count = count.min(100);
                let templates: Vec<ObjectNode> = self.current_target_objects().iter().filter(|o| !o.locked && sel.contains(&o.id)).cloned().collect();
                let mut new_selection = Vec::new();
                for step in 1..=clamped_count {
                    let dx = f64::from(step) * offset_x_pt.get();
                    let dy = f64::from(step) * offset_y_pt.get();
                    let Ok(step_tf) = Affine2::translation(dx, dy) else {
                        continue;
                    };
                    for orig in &templates {
                        let mut copy = orig.clone();
                        reallocate_object_ids_recursive(&mut copy, &mut self.document);
                        if let Ok(composed) = copy.transform.then(step_tf) {
                            copy.transform = composed;
                        }
                        new_selection.push(copy.id);
                        self.target_layer_mut().objects.push(copy);
                    }
                }
                if !new_selection.is_empty() {
                    self.selection = new_selection;
                }
            }
            EngineCommand::ExpandEffects { object_id } => {
                self.for_each_node_mut(|obj| {
                    if obj.id == object_id && !obj.locked && !obj.effects.is_empty() {
                        if let Some(mut base_path) = object_to_path(obj) {
                            for effect in &obj.effects {
                                match effect {
                                    VectorEffect::ArtisticMediaBrush { start_width_pt, end_width_pt } => {
                                        if let Ok(p) =
                                            coralcraft_geom::expand_artistic_brush_stroke(&base_path, start_width_pt.get(), end_width_pt.get())
                                        {
                                            base_path = p;
                                        }
                                    }
                                    VectorEffect::Envelope { corners } => {
                                        if let Some(bbox) = base_path.bounds()
                                            && let Ok(p) = coralcraft_geom::warp_path_bilinear(&base_path, bbox, *corners)
                                        {
                                            base_path = p;
                                        }
                                    }
                                    VectorEffect::Perspective { corners } => {
                                        if let Some(bbox) = base_path.bounds()
                                            && let Ok(p) = coralcraft_geom::warp_path_perspective(&base_path, bbox, *corners)
                                        {
                                            base_path = p;
                                        }
                                    }
                                    VectorEffect::Distortion { kind, amplitude, frequency } => {
                                        if let Some(bbox) = base_path.bounds()
                                            && let Ok(p) = coralcraft_geom::distort_path(&base_path, bbox, *kind, amplitude.get(), frequency.get())
                                        {
                                            base_path = p;
                                        }
                                    }
                                    VectorEffect::TextOnPath { spine, offset_pt } => {
                                        if let ObjectKind::ArtisticText { text, font_family, size_pt, .. } = &obj.kind
                                            && let Ok(p) = coralcraft_geom::fit_text_to_path(text, font_family, size_pt.get(), spine, offset_pt.get())
                                        {
                                            base_path = p;
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            obj.kind = ObjectKind::Path(base_path);
                        }
                        obj.effects.clear();
                    }
                });
            }
            EngineCommand::SetTransparency { transparency } => {
                self.for_each_selected_mut(|obj| {
                    obj.transparency = transparency.clone();
                });
            }
            EngineCommand::CopyPropertiesFrom { source_id, copy_fill, copy_outline, copy_effects } => {
                let Some(src) = self.selected_or_all_node(source_id).cloned() else {
                    return false;
                };
                self.for_each_selected_mut(|obj| {
                    if obj.id != source_id {
                        if copy_fill {
                            obj.fill = src.fill.clone();
                        }
                        if copy_outline {
                            obj.outline = src.outline.clone();
                        }
                        if copy_effects {
                            obj.effects = src.effects.clone();
                        }
                    }
                });
            }
            EngineCommand::CombineSelection => {
                let new_id = self.document.allocate_object_id();
                let sel = self.selection.clone();
                let layer = self.target_layer_mut();
                let mut combined_elements = Vec::new();
                let mut anchor_fill = None;
                let mut anchor_outline = None;
                let mut sel_indices = Vec::new();
                for (idx, obj) in layer.objects.iter().enumerate() {
                    if !obj.locked
                        && sel.contains(&obj.id)
                        && let Some(local_p) = object_to_path(obj)
                        && let Ok(world_p) = local_p.transform(obj.transform)
                    {
                        combined_elements.extend(world_p.elements);
                        anchor_fill = obj.fill.clone();
                        anchor_outline = obj.outline.clone();
                        sel_indices.push(idx);
                    }
                }
                if sel_indices.len() < 2 {
                    return false;
                }
                let combined_path = Path { elements: combined_elements, fill_rule: FillRule::EvenOdd };
                let new_node =
                    ObjectNode::new(new_id, Some("Combined Path".to_owned()), ObjectKind::Path(combined_path), anchor_fill, anchor_outline);
                let mut remaining = Vec::with_capacity(layer.objects.len());
                let mut inserted = false;
                for (idx, obj) in layer.objects.drain(..).enumerate() {
                    if sel_indices.contains(&idx) {
                        if !inserted {
                            remaining.push(new_node.clone());
                            inserted = true;
                        }
                    } else {
                        remaining.push(obj);
                    }
                }
                layer.objects = remaining;
                self.selection = vec![new_id];
            }
            EngineCommand::BreakApartSelection => {
                let sel = self.selection.clone();
                let mut next_id_val = self.document.next_id;
                let layer = self.target_layer_mut();
                let mut new_objects = Vec::with_capacity(layer.objects.len() + 4);
                let mut new_sel = Vec::new();
                for obj in layer.objects.drain(..) {
                    if !obj.locked
                        && sel.contains(&obj.id)
                        && let ObjectKind::Path(ref path) = obj.kind
                    {
                        let mut subpaths: Vec<Vec<PathElement>> = Vec::new();
                        for &el in &path.elements {
                            if matches!(el, PathElement::MoveTo(_)) || subpaths.is_empty() {
                                subpaths.push(vec![el]);
                            } else if let Some(last) = subpaths.last_mut() {
                                last.push(el);
                            }
                        }
                        if subpaths.len() >= 2 {
                            for (k, elems) in subpaths.into_iter().enumerate() {
                                let part_id = ObjectId(next_id_val);
                                next_id_val += 1;
                                let mut part_node = obj.clone();
                                part_node.id = part_id;
                                part_node.name = Some(format!("Subpath {}", k + 1));
                                part_node.kind = ObjectKind::Path(Path { elements: elems, fill_rule: FillRule::NonZero });
                                new_objects.push(part_node);
                                new_sel.push(part_id);
                            }
                            continue;
                        }
                    }
                    new_objects.push(obj);
                }
                layer.objects = new_objects;
                self.document.next_id = next_id_val;
                if !new_sel.is_empty() {
                    self.selection = new_sel;
                }
            }
            EngineCommand::AdjustBitmap { object_id, brightness, contrast, grayscale, invert } => {
                let b_adj = brightness.get().clamp(-1.0, 1.0);
                let c_adj = contrast.get().clamp(-0.99, 0.99);
                let c_factor = (1.0 + c_adj) / (1.0 - c_adj);
                self.for_each_node_mut(|obj| {
                    if obj.id == object_id
                        && !obj.locked
                        && let ObjectKind::Bitmap { rgba_pixels, .. } = &mut obj.kind
                    {
                        for px in rgba_pixels.chunks_exact_mut(4) {
                            let mut r = f64::from(px[0]) / 255.0;
                            let mut g = f64::from(px[1]) / 255.0;
                            let mut b = f64::from(px[2]) / 255.0;
                            if grayscale {
                                let lum = 0.2126 * r + 0.7152 * g + 0.0722 * b;
                                r = lum;
                                g = lum;
                                b = lum;
                            }
                            r = (c_factor * (r + b_adj - 0.5) + 0.5).clamp(0.0, 1.0);
                            g = (c_factor * (g + b_adj - 0.5) + 0.5).clamp(0.0, 1.0);
                            b = (c_factor * (b + b_adj - 0.5) + 0.5).clamp(0.0, 1.0);
                            if invert {
                                r = 1.0 - r;
                                g = 1.0 - g;
                                b = 1.0 - b;
                            }
                            px[0] = (r * 255.0).round().clamp(0.0, 255.0) as u8;
                            px[1] = (g * 255.0).round().clamp(0.0, 255.0) as u8;
                            px[2] = (b * 255.0).round().clamp(0.0, 255.0) as u8;
                        }
                    }
                });
            }
            EngineCommand::ApplyDistortion { object_id, kind, amplitude, frequency } => {
                let effect = VectorEffect::Distortion { kind, amplitude, frequency };
                self.for_each_node_mut(|obj| {
                    if obj.id == object_id && !obj.locked {
                        obj.effects.retain(|e| !matches!(e, VectorEffect::Distortion { .. }));
                        obj.effects.push(effect.clone());
                    }
                });
            }
            EngineCommand::TraceBitmap { object_id, threshold } => {
                self.for_each_node_mut(|obj| {
                    if obj.id == object_id
                        && !obj.locked
                        && let ObjectKind::Bitmap { pixel_width, pixel_height, dpi_x, dpi_y, rgba_pixels } = &obj.kind
                        && let Ok(traced_path) = trace_bitmap_to_path(*pixel_width, *pixel_height, dpi_x.get(), dpi_y.get(), rgba_pixels, threshold)
                    {
                        obj.kind = ObjectKind::Path(traced_path);
                        if obj.fill.is_none() {
                            obj.fill = Some(FillStyle::uniform(Color::BLACK));
                        }
                    }
                });
            }
            EngineCommand::MirrorSelection { horizontal, vertical, copy } => {
                let sel_nodes = self.selected_nodes();
                let bounds = combined_selection_bounds(&sel_nodes);
                let Ok(center) = bounds.center() else {
                    return false;
                };
                let sx = if horizontal { -1.0 } else { 1.0 };
                let sy = if vertical { -1.0 } else { 1.0 };
                let Ok(t_to_origin) = Affine2::translation(-center.x.get(), -center.y.get()) else {
                    return false;
                };
                let Ok(t_scale) = Affine2::scaling(sx, sy) else {
                    return false;
                };
                let Ok(t_back) = Affine2::translation(center.x.get(), center.y.get()) else {
                    return false;
                };
                let Ok(mirror_tf) = t_to_origin.then(t_scale).and_then(|m| m.then(t_back)) else {
                    return false;
                };

                if copy {
                    let sel = self.selection.clone();
                    let templates: Vec<ObjectNode> =
                        self.current_target_objects().iter().filter(|o| !o.locked && sel.contains(&o.id)).cloned().collect();
                    let mut new_selection = Vec::new();
                    for mut dup in templates {
                        reallocate_object_ids_recursive(&mut dup, &mut self.document);
                        if let Ok(composed) = dup.transform.then(mirror_tf) {
                            dup.transform = composed;
                        }
                        new_selection.push(dup.id);
                        self.target_layer_mut().objects.push(dup);
                    }
                    if !new_selection.is_empty() {
                        self.selection = new_selection;
                    }
                } else {
                    self.for_each_selected_mut(|obj| {
                        if let Ok(composed) = obj.transform.then(mirror_tf) {
                            obj.transform = composed;
                        }
                    });
                }
            }
            EngineCommand::CreateBoundary => {
                let new_id = self.document.allocate_object_id();
                let sel = self.selection.clone();
                let default_outline = self.default_outline.clone();
                let layer = self.target_layer_mut();
                let mut boundary_path: Option<Path> = None;
                let mut boundary_outline = None;
                for obj in &layer.objects {
                    if !obj.locked
                        && sel.contains(&obj.id)
                        && let Some(local_p) = object_to_path(obj)
                        && let Ok(world_p) = local_p.transform(obj.transform)
                    {
                        boundary_outline = obj.outline.clone();
                        boundary_path = Some(match boundary_path {
                            None => world_p,
                            Some(acc) => boolean_paths(&acc, &world_p, BooleanOp::Weld).unwrap_or(acc),
                        });
                    }
                }
                let Some(final_path) = boundary_path else {
                    return false;
                };
                let outline = boundary_outline.or(default_outline).or_else(|| Some(OutlineStyle::hairline(Color::BLACK)));
                let new_node = ObjectNode::new(new_id, Some("Boundary".to_owned()), ObjectKind::Path(final_path), None, outline);
                layer.objects.push(new_node);
                self.selection = vec![new_id];
            }
            EngineCommand::ConvertOutlineToObject => {
                self.for_each_selected_mut(|obj| {
                    if !obj.locked
                        && let Some(outline) = obj.outline.clone()
                        && let Some(base_path) = object_to_path(obj)
                    {
                        let w = outline.width_pt.get().max(0.25);
                        if let Ok(stroke_path) = coralcraft_geom::expand_artistic_brush_stroke(&base_path, w, w) {
                            obj.kind = ObjectKind::Path(stroke_path);
                            obj.fill = Some(FillStyle::uniform(outline.color));
                            obj.outline = None;
                        }
                    }
                });
            }
            EngineCommand::RotateSelection { angle_deg, origin_index, copy } => {
                let sel_nodes = self.selected_nodes();
                let bounds = combined_selection_bounds(&sel_nodes);
                let Ok(pivot) = reference_point(bounds, usize::from(origin_index)) else {
                    return false;
                };
                let Ok(t_to_origin) = Affine2::translation(-pivot.x.get(), -pivot.y.get()) else {
                    return false;
                };
                let Ok(t_rot) = Affine2::rotation_deg(angle_deg.get()) else {
                    return false;
                };
                let Ok(t_back) = Affine2::translation(pivot.x.get(), pivot.y.get()) else {
                    return false;
                };
                let Ok(rot_tf) = t_to_origin.then(t_rot).and_then(|m| m.then(t_back)) else {
                    return false;
                };
                self.apply_selection_transform(rot_tf, copy);
            }
            EngineCommand::ScaleSelection { scale_x, scale_y, origin_index, copy } => {
                let sel_nodes = self.selected_nodes();
                let bounds = combined_selection_bounds(&sel_nodes);
                let Ok(pivot) = reference_point(bounds, usize::from(origin_index)) else {
                    return false;
                };
                let Ok(t_to_origin) = Affine2::translation(-pivot.x.get(), -pivot.y.get()) else {
                    return false;
                };
                let Ok(t_scale) = Affine2::scaling(scale_x.get(), scale_y.get()) else {
                    return false;
                };
                let Ok(t_back) = Affine2::translation(pivot.x.get(), pivot.y.get()) else {
                    return false;
                };
                let Ok(scale_tf) = t_to_origin.then(t_scale).and_then(|m| m.then(t_back)) else {
                    return false;
                };
                self.apply_selection_transform(scale_tf, copy);
            }
            EngineCommand::SkewSelection { skew_x_deg, skew_y_deg, origin_index, copy } => {
                let sel_nodes = self.selected_nodes();
                let bounds = combined_selection_bounds(&sel_nodes);
                let Ok(pivot) = reference_point(bounds, usize::from(origin_index)) else {
                    return false;
                };
                let Ok(t_to_origin) = Affine2::translation(-pivot.x.get(), -pivot.y.get()) else {
                    return false;
                };
                let Ok(t_skew) = Affine2::skew_deg(skew_x_deg.get(), skew_y_deg.get()) else {
                    return false;
                };
                if t_skew.determinant().abs() <= 1e-6 {
                    return false;
                }
                let Ok(t_back) = Affine2::translation(pivot.x.get(), pivot.y.get()) else {
                    return false;
                };
                let Ok(skew_tf) = t_to_origin.then(t_skew).and_then(|m| m.then(t_back)) else {
                    return false;
                };
                self.apply_selection_transform(skew_tf, copy);
            }
            EngineCommand::DuplicateSelection { offset_x_pt, offset_y_pt } => {
                let Ok(tf) = Affine2::translation(offset_x_pt.get(), offset_y_pt.get()) else {
                    return false;
                };
                self.apply_selection_transform(tf, true);
            }
            EngineCommand::ReversePathDirection { object_id } => {
                self.for_each_node_mut(|obj| {
                    if obj.id == object_id
                        && !obj.locked
                        && let Some(base_path) = object_to_path(obj)
                        && let Ok(rev) = reverse_path_direction(&base_path)
                    {
                        obj.kind = ObjectKind::Path(rev);
                    }
                });
            }
            EngineCommand::CloseOpenSubpaths { object_id } => {
                self.for_each_node_mut(|obj| {
                    if obj.id == object_id
                        && !obj.locked
                        && let Some(base_path) = object_to_path(obj)
                        && let Ok(closed) = close_open_subpaths(&base_path)
                    {
                        obj.kind = ObjectKind::Path(closed);
                    }
                });
            }
            EngineCommand::SetObjectLocked { object_id, locked } => {
                self.for_each_node_mut(|obj| {
                    if obj.id == object_id {
                        obj.locked = locked;
                    }
                });
            }
            EngineCommand::UnlockAllObjects => {
                self.for_each_node_mut(|obj| {
                    obj.locked = false;
                });
            }
            EngineCommand::JoinOpenSubpaths { object_id, tolerance_pt } => {
                let tol = tolerance_pt.get();
                self.for_each_node_mut(|obj| {
                    if obj.id == object_id
                        && !obj.locked
                        && let Some(base_path) = object_to_path(obj)
                        && let Ok(joined) = join_open_subpaths(&base_path, tol)
                    {
                        obj.kind = ObjectKind::Path(joined);
                    }
                });
            }
            EngineCommand::UngroupAllSelection => {
                let sel = self.selection.clone();
                let layer = self.target_layer_mut();
                let mut new_objects = Vec::with_capacity(layer.objects.len());
                let mut new_selection = Vec::new();
                for obj in layer.objects.drain(..) {
                    if !obj.locked && sel.contains(&obj.id) && matches!(obj.kind, ObjectKind::Group { .. }) {
                        let mut leaves = Vec::new();
                        flatten_group_recursive(obj, &mut leaves);
                        for leaf in leaves {
                            new_selection.push(leaf.id);
                            new_objects.push(leaf);
                        }
                    } else {
                        new_objects.push(obj);
                    }
                }
                layer.objects = new_objects;
                self.selection = new_selection;
            }
            EngineCommand::MovePage { from_index, to_index } => {
                let page = self.document.pages.remove(from_index);
                self.document.pages.insert(to_index, page);
                self.active_target = ActivePageTarget::Page(to_index);
                self.selection.clear();
            }
        }
        true
    }

    fn apply_selection_transform(&mut self, tf: Affine2, copy: bool) {
        if copy {
            let sel = self.selection.clone();
            let templates: Vec<ObjectNode> = self.current_target_objects().iter().filter(|o| !o.locked && sel.contains(&o.id)).cloned().collect();
            let mut new_selection = Vec::new();
            for mut dup in templates {
                reallocate_object_ids_recursive(&mut dup, &mut self.document);
                if let Ok(composed) = dup.transform.then(tf) {
                    dup.transform = composed;
                }
                new_selection.push(dup.id);
                self.target_layer_mut().objects.push(dup);
            }
            if !new_selection.is_empty() {
                self.selection = new_selection;
            }
        } else {
            self.for_each_selected_mut(|obj| {
                if let Ok(composed) = obj.transform.then(tf) {
                    obj.transform = composed;
                }
            });
        }
    }

    /// Replaces the current selection list.
    pub fn set_selection(&mut self, selection: Vec<ObjectId>) {
        self.selection = selection;
    }

    /// Selects all unlocked, visible objects on the active target layer (`Edit -> Select All -> Objects` / `Ctrl+A`).
    pub fn select_all(&mut self) {
        self.selection = self.current_target_objects().iter().filter(|o| !o.locked && o.visible).map(|o| o.id).collect();
    }
}

fn flatten_group_recursive(obj: ObjectNode, out: &mut Vec<ObjectNode>) {
    if let ObjectKind::Group { children } = obj.kind {
        let group_tf = obj.transform;
        for mut child in children {
            if let Ok(composed) = child.transform.then(group_tf) {
                child.transform = composed;
            }
            flatten_group_recursive(child, out);
        }
    } else {
        out.push(obj);
    }
}

impl EditorEngine {
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

    #[must_use]
    pub fn current_target_objects(&self) -> &[ObjectNode] {
        match self.active_target {
            ActivePageTarget::Page(idx) => {
                let page_idx = idx.min(self.document.pages.len().saturating_sub(1));
                self.document.pages[page_idx].layers.last().map_or(&[], |l| l.objects.as_slice())
            }
            ActivePageTarget::MasterPage => {
                self.document.master_layers.iter().find(|l| l.role == LayerRole::MasterAllPages).map_or(&[], |l| l.objects.as_slice())
            }
            ActivePageTarget::Desktop => {
                self.document.master_layers.iter().find(|l| l.role == LayerRole::Desktop).map_or(&[], |l| l.objects.as_slice())
            }
        }
    }

    fn selected_or_all_node(&self, id: ObjectId) -> Option<&ObjectNode> {
        for page in &self.document.pages {
            for layer in &page.layers {
                for obj in &layer.objects {
                    if obj.id == id {
                        return Some(obj);
                    }
                }
            }
        }
        for layer in &self.document.master_layers {
            for obj in &layer.objects {
                if obj.id == id {
                    return Some(obj);
                }
            }
        }
        None
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
        self.for_each_node_mut(|obj| {
            if sel.contains(&obj.id) {
                f(obj);
            }
        });
    }

    fn for_each_node_mut(&mut self, mut f: impl FnMut(&mut ObjectNode)) {
        for page in &mut self.document.pages {
            for layer in &mut page.layers {
                for obj in &mut layer.objects {
                    f(obj);
                }
            }
        }
        for layer in &mut self.document.master_layers {
            for obj in &mut layer.objects {
                f(obj);
            }
        }
    }
}

fn powerclip_subtree_depth(node: &ObjectNode) -> usize {
    node.powerclip_children.iter().map(|c| 1 + powerclip_subtree_depth(c)).max().unwrap_or(0)
}

fn object_to_path(obj: &ObjectNode) -> Option<coralcraft_geom::Path> {
    match &obj.kind {
        ObjectKind::Rect(r) => r.to_path().ok(),
        ObjectKind::Ellipse(e) => e.to_path().ok(),
        ObjectKind::Polygon(p) => p.to_path().ok(),
        ObjectKind::Path(p) => Some(p.clone()),
        ObjectKind::ArtisticText { text, font_family, size_pt, position } => Some(artistic_text_to_path(text, font_family, size_pt.get(), *position)),
        ObjectKind::ParagraphText(pt) => {
            let layout = coralcraft_doc::layout_paragraph_text(pt);
            let mut combined = Vec::new();
            for line in &layout.lines {
                if let Ok(origin) = Point::new(line.origin_x, line.origin_y) {
                    let line_path = artistic_text_to_path(&line.text, &pt.font_family, pt.font_size_pt.get(), origin);
                    combined.extend(line_path.elements);
                }
            }
            coralcraft_geom::Path::from_elements(combined, coralcraft_geom::FillRule::NonZero).ok()
        }
        _ => None,
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

    #[test]
    fn group_ungroup_convert_to_curves_zorder_align_powerclip_and_node_edit() {
        let mut engine = EditorEngine::default();

        // Create two rectangles A and B
        engine.handle_shortcut(KeyShortcut::F6);
        engine.pointer_down(Point::new(10.0, 20.0).unwrap(), false, false);
        engine.pointer_up(Point::new(50.0, 60.0).unwrap(), false, false);
        let id_a = engine.selection[0];

        engine.pointer_down(Point::new(100.0, 120.0).unwrap(), false, false);
        engine.pointer_up(Point::new(160.0, 180.0).unwrap(), false, false);
        let id_b = engine.selection[0];

        // 1. Align A's left edge to B (anchor = last selected = id_b)
        engine.set_selection(vec![id_a, id_b]);
        assert!(engine.execute(EngineCommand::AlignSelection { mode: AlignMode::Left }));
        let node_a = engine.selected_or_all_node(id_a).unwrap();
        assert!((node_a.transform.tx.get() - 90.0).abs() < 1e-9);

        // 2. Group A and B -> single Group node, then translate group by (+10, +5), then Ungroup
        assert!(engine.execute(EngineCommand::GroupSelection));
        assert_eq!(engine.selection.len(), 1);
        let group_id = engine.selection[0];
        assert!(matches!(engine.selected_nodes()[0].kind, ObjectKind::Group { .. }));
        assert!(engine.execute(EngineCommand::TranslateSelection { dx_pt: F64::new(10.0).unwrap(), dy_pt: F64::new(5.0).unwrap() }));
        assert_ne!(group_id, id_a);
        assert!(engine.execute(EngineCommand::UngroupSelection));
        assert_eq!(engine.selection, vec![id_a, id_b]);
        let node_a_after = engine.selected_or_all_node(id_a).unwrap();
        assert!((node_a_after.transform.tx.get() - 100.0).abs() < 1e-9);
        assert!((node_a_after.transform.ty.get() - 5.0).abs() < 1e-9);

        // 3. Reorder selection: move id_b to back, then id_b to front
        engine.set_selection(vec![id_b]);
        assert!(engine.execute(EngineCommand::ReorderSelection { op: ZOrderOp::ToBack }));
        assert_eq!(engine.document.pages[0].layers[0].objects[0].id, id_b);
        assert!(engine.execute(EngineCommand::ReorderSelection { op: ZOrderOp::ToFront }));
        assert_eq!(engine.document.pages[0].layers[0].objects[1].id, id_b);

        // 4. PowerClip: place id_a inside container id_b, then extract
        engine.set_selection(vec![id_a]);
        assert!(engine.execute(EngineCommand::PlaceInsidePowerClip { container_id: id_b }));
        assert_eq!(engine.document.pages[0].layers[0].objects.len(), 1);
        assert_eq!(engine.document.pages[0].layers[0].objects[0].powerclip_children.len(), 1);
        assert!(engine.execute(EngineCommand::ExtractPowerClip { container_id: id_b }));
        assert_eq!(engine.document.pages[0].layers[0].objects.len(), 2);
        assert_eq!(engine.selection, vec![id_a]);

        // 5. ConvertToCurves (`Ctrl+Q`) on id_a and MoveBezierNode (`F10` Shape Tool)
        assert!(engine.execute(EngineCommand::ConvertToCurves));
        assert!(matches!(engine.selected_or_all_node(id_a).unwrap().kind, ObjectKind::Path(_)));
        assert!(engine.execute(EngineCommand::MoveBezierNode { object_id: id_a, element_index: 0, new_pt: Point::new(125.0, 35.0).unwrap() }));
        if let ObjectKind::Path(p) = &engine.selected_or_all_node(id_a).unwrap().kind {
            assert_eq!(p.elements[0], coralcraft_geom::PathElement::MoveTo(Point::new(25.0, 30.0).unwrap()));
        } else {
            panic!("expected Path after ConvertToCurves");
        }
    }

    #[test]
    fn boolean_selection_and_vector_effects_with_undo() {
        let mut engine = EditorEngine::default();

        // 1. Create two overlapping rectangles
        engine.handle_shortcut(KeyShortcut::F6);
        engine.pointer_down(Point::new(0.0, 0.0).unwrap(), false, false);
        engine.pointer_up(Point::new(60.0, 60.0).unwrap(), false, false);
        let id1 = engine.selection[0];

        engine.pointer_down(Point::new(40.0, 0.0).unwrap(), false, false);
        engine.pointer_up(Point::new(100.0, 60.0).unwrap(), false, false);
        let id2 = engine.selection[0];

        // 2. Boolean Weld
        engine.set_selection(vec![id1, id2]);
        assert!(engine.execute(EngineCommand::BooleanSelection { op: BooleanOp::Weld }));
        assert_eq!(engine.selection.len(), 1);
        let welded_id = engine.selection[0];
        assert!(matches!(engine.selected_nodes()[0].kind, ObjectKind::Path(_)));

        // 3. Undo Weld restores both original rectangles
        assert!(engine.undo());
        assert_eq!(engine.current_target_objects().len(), 2);
        assert!(engine.redo());
        assert_eq!(engine.current_target_objects().len(), 1);

        // 4. Non-destructive Contour effect on the welded object
        assert!(engine.execute(EngineCommand::ApplyContour { object_id: welded_id, steps: 3, offset_pt: F64::new(4.0).unwrap(), inside: false }));
        let obj = engine.selected_or_all_node(welded_id).unwrap();
        assert_eq!(obj.effects.len(), 1);
        assert!(matches!(obj.effects[0], VectorEffect::Contour { steps: 3, .. }));

        // 5. Non-destructive DropShadow effect
        assert!(engine.execute(EngineCommand::AddDropShadow {
            object_id: welded_id,
            offset_x: F64::new(5.0).unwrap(),
            offset_y: F64::new(5.0).unwrap(),
            opacity: F64::new(0.6).unwrap(),
            color: Color::BLACK,
        }));
        assert_eq!(engine.selected_or_all_node(welded_id).unwrap().effects.len(), 2);

        // 6. ExpandEffects bakes effects into static geometry and clears live effect stack
        assert!(engine.execute(EngineCommand::ExpandEffects { object_id: welded_id }));
        assert!(engine.selected_or_all_node(welded_id).unwrap().effects.is_empty());

        // 7. Undo restores both live effects
        assert!(engine.undo());
        assert_eq!(engine.selected_or_all_node(welded_id).unwrap().effects.len(), 2);

        // 8. Convert ArtisticText to curves
        let text_id = engine.document.allocate_object_id();
        let text_node = ObjectNode::new(
            text_id,
            Some("Text".to_owned()),
            ObjectKind::ArtisticText {
                text: "CORAL".to_owned(),
                font_family: "Sans".to_owned(),
                size_pt: F64::new(24.0).unwrap(),
                position: Point::new(10.0, 100.0).unwrap(),
            },
            Some(FillStyle::uniform(Color::BLACK)),
            None,
        );
        engine.target_layer_mut().objects.push(text_node);
        engine.set_selection(vec![text_id]);
        assert!(engine.execute(EngineCommand::ConvertToCurves));
        assert!(matches!(engine.selected_or_all_node(text_id).unwrap().kind, ObjectKind::Path(_)));

        // 9. ApplyEnvelope and ExpandEffects bake warped corner coordinates on welded_id
        let corners =
            [Point::new(10.0, 10.0).unwrap(), Point::new(150.0, 20.0).unwrap(), Point::new(160.0, 90.0).unwrap(), Point::new(0.0, 80.0).unwrap()];
        assert!(engine.execute(EngineCommand::ApplyEnvelope { object_id: welded_id, corners }));
        assert!(engine.execute(EngineCommand::ExpandEffects { object_id: welded_id }));
        if let ObjectKind::Path(warped_path) = &engine.selected_or_all_node(welded_id).unwrap().kind {
            let wb = warped_path.bounds().unwrap();
            assert!((wb.x0.get() - 0.0).abs() < 1e-3);
            assert!((wb.x1.get() - 160.0).abs() < 1e-3);
        } else {
            panic!("expected baked Path");
        }
        assert!(engine.execute(EngineCommand::ApplyArtisticBrush {
            object_id: text_id,
            start_width_pt: F64::new(12.0).unwrap(),
            end_width_pt: F64::new(2.0).unwrap(),
        }));
        assert_eq!(engine.selected_or_all_node(text_id).unwrap().effects.len(), 1);

        // 10. Step and Repeat creates 3 offset duplicates and supports undo
        engine.set_selection(vec![welded_id]);
        let before_count = engine.current_target_objects().len();
        assert!(engine.execute(EngineCommand::StepAndRepeat {
            count: 3,
            offset_x_pt: F64::new(25.0).unwrap(),
            offset_y_pt: F64::new(15.0).unwrap(),
        }));
        assert_eq!(engine.current_target_objects().len(), before_count + 3);
        assert_eq!(engine.selection.len(), 3);
        let dup_ids = engine.selection.clone();
        assert!(engine.execute(EngineCommand::DistributeSelection { mode: DistributeMode::SpacingHorizontal }));
        assert_eq!(engine.selection, dup_ids);
        assert!(engine.undo());
        assert!(engine.undo());
        assert_eq!(engine.current_target_objects().len(), before_count);

        // 11. CopyPropertiesFrom copies fill, outline, and effects from text_id to welded_id
        engine.set_selection(vec![welded_id]);
        assert!(engine.execute(EngineCommand::CopyPropertiesFrom { source_id: text_id, copy_fill: true, copy_outline: true, copy_effects: true }));
        assert_eq!(engine.selected_or_all_node(welded_id).unwrap().effects.len(), 1);

        // 12. CombineSelection (Ctrl+L) and BreakApartSelection (Ctrl+K)
        engine.set_selection(vec![welded_id, text_id]);
        assert!(engine.execute(EngineCommand::CombineSelection));
        assert!(engine.execute(EngineCommand::BreakApartSelection));
        assert!(engine.selection.len() >= 2);

        // 13. CreateBoundary (non-destructive union silhouette) & ConvertOutlineToObject (Ctrl+Shift+Q)
        let count_before_boundary = engine.current_target_objects().len();
        assert!(engine.execute(EngineCommand::CreateBoundary));
        assert_eq!(engine.current_target_objects().len(), count_before_boundary + 1);
        let boundary_id = engine.selection[0];
        assert!(engine.selected_or_all_node(boundary_id).unwrap().outline.is_some());
        assert!(engine.execute(EngineCommand::ConvertOutlineToObject));
        let converted = engine.selected_or_all_node(boundary_id).unwrap();
        assert!(converted.outline.is_none());
        assert!(converted.fill.is_some());

        // 14. RotateSelection & ScaleSelection around 9-point Object Origin (origin_index = 4 center)
        assert!(engine.execute(EngineCommand::RotateSelection { angle_deg: F64::new(45.0).unwrap(), origin_index: 4, copy: false }));
        assert!(engine.execute(EngineCommand::ScaleSelection {
            scale_x: F64::new(1.5).unwrap(),
            scale_y: F64::new(1.5).unwrap(),
            origin_index: 0,
            copy: true,
        }));
        assert!(engine.execute(EngineCommand::SkewSelection {
            skew_x_deg: F64::new(15.0).unwrap(),
            skew_y_deg: F64::ZERO,
            origin_index: 4,
            copy: false,
        }));
        assert!(engine.execute(EngineCommand::DuplicateSelection { offset_x_pt: F64::new(18.0).unwrap(), offset_y_pt: F64::new(18.0).unwrap() }));
        engine.select_all();
        assert!(engine.selection.len() >= 3);

        // 15. ReversePathDirection, CloseOpenSubpaths, SetObjectLocked, and UnlockAllObjects
        assert!(engine.execute(EngineCommand::ReversePathDirection { object_id: boundary_id }));
        assert!(engine.execute(EngineCommand::CloseOpenSubpaths { object_id: boundary_id }));
        assert!(engine.execute(EngineCommand::SetObjectLocked { object_id: boundary_id, locked: true }));
        assert!(engine.selected_or_all_node(boundary_id).unwrap().locked);
        assert!(engine.execute(EngineCommand::UnlockAllObjects));
        assert!(!engine.selected_or_all_node(boundary_id).unwrap().locked);

        // 16. JoinOpenSubpaths, UngroupAllSelection, and MovePage
        assert!(engine.execute(EngineCommand::JoinOpenSubpaths { object_id: boundary_id, tolerance_pt: F64::new(5.0).unwrap() }));
        engine.select_all();
        assert!(engine.execute(EngineCommand::GroupSelection));
        assert!(engine.execute(EngineCommand::UngroupAllSelection));
        assert!(engine.selection.len() >= 2);
        assert!(engine.execute(EngineCommand::AddPageAfterActive));
        assert_eq!(engine.document.pages.len(), 2);
        assert!(engine.execute(EngineCommand::MovePage { from_index: 1, to_index: 0 }));
        assert_eq!(engine.current_page_index(), 0);
    }
}
