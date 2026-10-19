//! Interactive tool state machines, CorelDRAW shortcut bindings, and context-sensitive
//! Property Bar schema generation for CoralCraft (L4).

use coralcraft_doc::{ObjectKind, ObjectNode, Page};
use coralcraft_geom::{CornerKind, EllipseMode, EllipseSpec, F64, FlattenOpts, Point, PolygonSpec, Rect, RectSpec, Unit, flatten_path};
use serde::{Deserialize, Serialize};

/// The 10 classic tools in the left vertical CorelDRAW Toolbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum ToolKind {
    /// Pick tool (select, move, scale, rotate, skew).
    #[default]
    Pick,
    /// Shape tool (`F10` — node and corner handle editing).
    Shape,
    /// Crop tool (crop marquee on bitmaps or page).
    Crop,
    /// Zoom tool (`Z` — click zoom in, Alt-click zoom out, drag marquee zoom).
    Zoom,
    /// Freehand / Bézier curve tool (`F5`).
    FreehandBezier,
    /// Rectangle tool (`F6` — drag rectangle, Shift for square).
    Rectangle,
    /// Ellipse tool (`F7` — drag ellipse, Shift for circle).
    Ellipse,
    /// Polygon / Star tool (`Y`).
    PolygonStar,
    /// Artistic Text tool (`F8`).
    ArtisticText,
    /// Interactive Fill tool (`G`).
    InteractiveFill,
}

impl ToolKind {
    /// Ordered list of tools in the left vertical Toolbox from top to bottom.
    pub const ALL: [Self; 10] = [
        Self::Pick,
        Self::Shape,
        Self::Crop,
        Self::Zoom,
        Self::FreehandBezier,
        Self::Rectangle,
        Self::Ellipse,
        Self::PolygonStar,
        Self::ArtisticText,
        Self::InteractiveFill,
    ];

    /// Human-readable tool name.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Pick => "Pick",
            Self::Shape => "Shape",
            Self::Crop => "Crop",
            Self::Zoom => "Zoom",
            Self::FreehandBezier => "Freehand / Bezier",
            Self::Rectangle => "Rectangle",
            Self::Ellipse => "Ellipse",
            Self::PolygonStar => "Polygon / Star",
            Self::ArtisticText => "Artistic Text",
            Self::InteractiveFill => "Interactive Fill",
        }
    }

    /// Classic CorelDRAW keyboard shortcut label, if assigned.
    #[must_use]
    pub const fn shortcut_hint(self) -> Option<&'static str> {
        match self {
            Self::Pick | Self::Crop => None,
            Self::Shape => Some("F10"),
            Self::Zoom => Some("Z"),
            Self::FreehandBezier => Some("F5"),
            Self::Rectangle => Some("F6"),
            Self::Ellipse => Some("F7"),
            Self::PolygonStar => Some("Y"),
            Self::ArtisticText => Some("F8"),
            Self::InteractiveFill => Some("G"),
        }
    }

    /// Tooltip string combining the tool name and shortcut hint.
    #[must_use]
    pub fn tooltip(self) -> String {
        match self.shortcut_hint() {
            Some(key) => format!("{} ({key})", self.label()),
            None => self.label().to_owned(),
        }
    }

    /// Returns `true` if arming this tool immediately switches the context-sensitive
    /// Property Bar to its creation parameters before the first drag (`F5`, `F6`, `F7`, `Y`, `F8`).
    #[must_use]
    pub const fn arms_property_bar(self) -> bool {
        matches!(self, Self::FreehandBezier | Self::Rectangle | Self::Ellipse | Self::PolygonStar | Self::ArtisticText)
    }
}

/// Logical key press used for classic CorelDRAW single-key and function-key tool switching.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KeyShortcut {
    F5,
    F6,
    F7,
    F8,
    F10,
    Char(char),
    Escape,
}

/// Maps a [`KeyShortcut`] to a [`ToolKind`] following CorelDRAW's classic shortcut table.
#[must_use]
pub const fn tool_for_shortcut(key: KeyShortcut) -> Option<ToolKind> {
    match key {
        KeyShortcut::Escape => Some(ToolKind::Pick),
        KeyShortcut::F10 => Some(ToolKind::Shape),
        KeyShortcut::F5 => Some(ToolKind::FreehandBezier),
        KeyShortcut::F6 => Some(ToolKind::Rectangle),
        KeyShortcut::F7 => Some(ToolKind::Ellipse),
        KeyShortcut::F8 => Some(ToolKind::ArtisticText),
        KeyShortcut::Char('z' | 'Z') => Some(ToolKind::Zoom),
        KeyShortcut::Char('y' | 'Y') => Some(ToolKind::PolygonStar),
        KeyShortcut::Char('g' | 'G') => Some(ToolKind::InteractiveFill),
        KeyShortcut::Char(_) => None,
    }
}

/// Sub-mode for the `Freehand / Bezier` (`F5`) tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum CurveSubMode {
    #[default]
    Freehand,
    Bezier,
}

/// Sub-mode for the `Polygon / Star` (`Y`) tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum PolygonSubMode {
    #[default]
    Polygon,
    Star,
}

/// Page orientation shown on the Page Setup Property Bar when selection is empty.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PageOrientation {
    Portrait,
    Landscape,
}

/// CorelDRAW 9-point Object Origin selector (`0..=8`, where `4` is `Center`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum OriginPoint {
    TopLeft,
    TopCenter,
    TopRight,
    MiddleLeft,
    #[default]
    Center,
    MiddleRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

impl OriginPoint {
    /// Converts the 9-point origin enum to a `0..=8` index compatible with `coralcraft_geom::reference_point`.
    #[must_use]
    pub const fn as_index(self) -> u8 {
        match self {
            Self::TopLeft => 0,
            Self::TopCenter => 1,
            Self::TopRight => 2,
            Self::MiddleLeft => 3,
            Self::Center => 4,
            Self::MiddleRight => 5,
            Self::BottomLeft => 6,
            Self::BottomCenter => 7,
            Self::BottomRight => 8,
        }
    }
}

/// Active canvas drag or creation gesture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GestureState {
    /// Dragging to create a parametric shape (`Rectangle`, `Ellipse`, `PolygonStar`).
    DragCreateShape { tool: ToolKind, start: Point, current: Point, shift_constrain: bool, alt_from_center: bool },
    /// Dragging selected objects with the `Pick` tool.
    DragMoveSelection { start: Point, current: Point },
    /// Dragging a zoom marquee with the `Zoom` tool.
    DragZoomMarquee { start: Point, current: Point },
    /// Dragging an interactive gradient vector with the `InteractiveFill` tool (`G`).
    DragInteractiveFill { start: Point, current: Point },
}

impl GestureState {
    /// Computes the constrained bounding box (`Rect { x0, y0, x1, y1 }`) for a shape-creation or marquee gesture.
    ///
    /// - `shift_constrain`: constrains width and height to equal magnitude (square / circle).
    /// - `alt_from_center`: treats `start` as the center point rather than a corner.
    #[must_use]
    pub fn compute_drag_rect(start: Point, current: Point, shift_constrain: bool, alt_from_center: bool) -> Option<Rect> {
        let mut dx = current.x.get() - start.x.get();
        let mut dy = current.y.get() - start.y.get();
        if shift_constrain {
            let side = dx.abs().max(dy.abs());
            dx = if dx < 0.0 { -side } else { side };
            dy = if dy < 0.0 { -side } else { side };
        }
        if alt_from_center {
            let rx = dx.abs();
            let ry = dy.abs();
            Rect::new(start.x.get() - rx, start.y.get() - ry, start.x.get() + rx, start.y.get() + ry).ok()
        } else {
            let end_x = start.x.get() + dx;
            let end_y = start.y.get() + dy;
            Rect::new(start.x.get().min(end_x), start.y.get().min(end_y), start.x.get().max(end_x), start.y.get().max(end_y)).ok()
        }
    }
}

/// Mutable tool controller state owned by the workspace/engine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolState {
    pub active_tool: ToolKind,
    pub curve_mode: CurveSubMode,
    pub polygon_mode: PolygonSubMode,
    pub origin_point: OriginPoint,
    pub default_rect_corner_kind: CornerKind,
    pub default_rect_uniform_radius_pt: F64,
    pub default_rect_corners_locked: bool,
    pub default_ellipse_mode: EllipseMode,
    pub default_ellipse_start_deg: F64,
    pub default_ellipse_end_deg: F64,
    pub default_polygon_points: u32,
    pub default_star_sharpness: F64,
    pub default_font_family: String,
    pub default_font_size_pt: F64,
    pub gesture: Option<GestureState>,
}

impl Default for ToolState {
    fn default() -> Self {
        Self {
            active_tool: ToolKind::Pick,
            curve_mode: CurveSubMode::Freehand,
            polygon_mode: PolygonSubMode::Polygon,
            origin_point: OriginPoint::Center,
            default_rect_corner_kind: CornerKind::Round,
            default_rect_uniform_radius_pt: F64::ZERO,
            default_rect_corners_locked: true,
            default_ellipse_mode: EllipseMode::Full,
            default_ellipse_start_deg: F64::ZERO,
            default_ellipse_end_deg: F64::new(360.0).unwrap_or(F64::ZERO),
            default_polygon_points: 5,
            default_star_sharpness: F64::new(0.5).unwrap_or(F64::ZERO),
            default_font_family: "Sans".to_owned(),
            default_font_size_pt: F64::new(24.0).unwrap_or(F64::ONE),
            gesture: None,
        }
    }
}

impl ToolState {
    /// Applies a keyboard shortcut if it maps to a CorelDRAW tool.
    pub fn apply_shortcut(&mut self, shortcut: KeyShortcut) -> bool {
        if let Some(tool) = tool_for_shortcut(shortcut) {
            self.active_tool = tool;
            self.gesture = None;
            true
        } else {
            false
        }
    }

    /// Starts a canvas drag gesture with the currently active tool.
    pub fn begin_drag(&mut self, start: Point, shift_constrain: bool, alt_from_center: bool) -> Option<&GestureState> {
        self.gesture = match self.active_tool {
            ToolKind::Rectangle | ToolKind::Ellipse | ToolKind::PolygonStar => {
                Some(GestureState::DragCreateShape { tool: self.active_tool, start, current: start, shift_constrain, alt_from_center })
            }
            ToolKind::Pick => Some(GestureState::DragMoveSelection { start, current: start }),
            ToolKind::Zoom => Some(GestureState::DragZoomMarquee { start, current: start }),
            ToolKind::InteractiveFill => Some(GestureState::DragInteractiveFill { start, current: start }),
            ToolKind::Shape | ToolKind::Crop | ToolKind::FreehandBezier | ToolKind::ArtisticText => None,
        };
        self.gesture.as_ref()
    }

    /// Updates an in-progress drag gesture.
    pub fn update_drag(&mut self, current: Point, shift_constrain: bool, alt_from_center: bool) {
        match &mut self.gesture {
            Some(GestureState::DragCreateShape { current: cur, shift_constrain: sc, alt_from_center: ac, .. }) => {
                *cur = current;
                *sc = shift_constrain;
                *ac = alt_from_center;
            }
            Some(
                GestureState::DragMoveSelection { current: cur, .. }
                | GestureState::DragZoomMarquee { current: cur, .. }
                | GestureState::DragInteractiveFill { current: cur, .. },
            ) => {
                *cur = current;
            }
            None => {}
        }
    }

    /// Finishes a shape-creation drag gesture and returns the constructed [`ObjectKind`]
    /// if the dragged rectangle has non-zero area.
    pub fn finish_create_shape(&mut self) -> Option<ObjectKind> {
        if !matches!(self.gesture, Some(GestureState::DragCreateShape { .. })) {
            return None;
        }
        let gesture = self.gesture.take()?;
        let GestureState::DragCreateShape { tool, start, current, shift_constrain, alt_from_center } = gesture else {
            return None;
        };
        let rect = GestureState::compute_drag_rect(start, current, shift_constrain, alt_from_center)?;
        if rect.width() < 0.5 || rect.height() < 0.5 {
            return None;
        }
        match tool {
            ToolKind::Rectangle => {
                let spec = RectSpec::uniform(rect, self.default_rect_uniform_radius_pt.get(), self.default_rect_corner_kind).ok()?;
                Some(ObjectKind::Rect(spec))
            }
            ToolKind::Ellipse => Some(ObjectKind::Ellipse(EllipseSpec {
                rect,
                start_deg: self.default_ellipse_start_deg,
                end_deg: self.default_ellipse_end_deg,
                mode: self.default_ellipse_mode,
            })),
            ToolKind::PolygonStar => {
                let center = rect.center().ok()?;
                let radius = 0.5 * rect.width().min(rect.height());
                let points = self.default_polygon_points.max(3);
                let spec = match self.polygon_mode {
                    PolygonSubMode::Polygon => PolygonSpec::regular(center, radius, points, 0.0).ok()?,
                    PolygonSubMode::Star => {
                        let sharpness = self.default_star_sharpness.get().clamp(0.05, 0.95);
                        PolygonSpec::star(center, radius, radius * (1.0 - sharpness), points, 0.0).ok()?
                    }
                };
                Some(ObjectKind::Polygon(spec))
            }
            _ => None,
        }
    }
}

/// High-level mode of the context-sensitive Property Bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PropertyContext {
    /// Empty selection -> Page width, height, units, Portrait/Landscape.
    PageSetup,
    /// Single or homogeneous multi-selection of Rectangles (or armed `F6` Rectangle tool).
    Rectangle,
    /// Single or homogeneous multi-selection of Ellipses (or armed `F7` Ellipse tool).
    Ellipse,
    /// Single or homogeneous multi-selection of Polygons (or armed `Y` Polygon mode).
    Polygon,
    /// Single or homogeneous multi-selection of Stars (or armed `Y` Star mode).
    Star,
    /// Armed `F5` Freehand / Bézier curve tool.
    FreehandBezier,
    /// Single Artistic Text object or armed `F8` Artistic Text tool.
    ArtisticText,
    /// Curve, Group, or heterogeneous multi-selection.
    GenericSelection,
}

/// Declarative control descriptor emitted by [`build_property_bar_schema`] and rendered
/// by `coralcraft-ui-egui` without direct document mutation inside widgets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PropertyControl {
    /// Page width & height in document points and unit selector.
    PageDimensions { width_pt: F64, height_pt: F64, units: Unit, orientation: PageOrientation },
    /// 9-point Object Origin selector (`TopLeft` .. `BottomRight`, `Center`).
    OriginSelector { active: OriginPoint },
    /// Numeric transform fields (`x`, `y`, `width`, `height`, `rotation_deg`).
    TransformBox { x_pt: F64, y_pt: F64, width_pt: F64, height_pt: F64, rotation_deg: F64 },
    /// Rectangle corner style (`Round | Scalloped | Chamfered`), uniform radius, lock toggle, and per-corner quad.
    RectangleCorners { corner_kind: CornerKind, uniform_radius_pt: F64, corners_locked: bool, radii: [F64; 4] },
    /// Ellipse mode (`Full | Pie | Arc`) and start/end angles in degrees.
    EllipseParameters { mode: EllipseMode, start_deg: F64, end_deg: F64 },
    /// Polygon / Star mode toggle and point/side count (`>= 3`).
    PolygonParameters { mode: PolygonSubMode, points: u32, inner_radius_pt: Option<F64> },
    /// Freehand vs Bézier sub-mode selector (`F5`).
    CurveModeSelector { mode: CurveSubMode },
    /// Artistic text font family, font size in points, and text string.
    ArtisticTextParameters { font_family: String, font_size_pt: F64, text: String },
    /// Read-only or clickable summary of selection Fill and Outline.
    StyleSummary { selection_count: usize, has_fill: bool, has_outline: bool },
}

/// Context-sensitive Property Bar schema computed as a pure function of selection and tool state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PropertyBarSchema {
    pub context: PropertyContext,
    pub controls: Vec<PropertyControl>,
}

/// Pure function that computes the CorelDRAW context-sensitive Property Bar schema
/// from the current selection, active page, document unit system, and [`ToolState`].
///
/// Rules (per classic CorelDRAW workspace specification):
/// 1. While a shape/curve/text creation tool is armed (`F5`, `F6`, `F7`, `Y`, `F8`) or actively
///    dragging a creation gesture, the Property Bar displays that tool's live parameters.
/// 2. Otherwise, when `selection` is empty, the Property Bar displays Page Setup controls
///    (Page width, height, units, Portrait/Landscape).
/// 3. When 1 or more objects of the same parametric kind (`Rect`, `Ellipse`, `Polygon`/`Star`, `ArtisticText`)
///    are selected, the Property Bar displays shared controls for that kind plus transform controls.
/// 4. When selected objects have differing kinds or are generic curves/groups, the Property Bar
///    displays the generic transform row (`OriginSelector`, `TransformBox`, `StyleSummary`).
#[must_use]
pub fn build_property_bar_schema(selection: &[&ObjectNode], page: &Page, units: Unit, tool_state: &ToolState) -> PropertyBarSchema {
    let active_create_tool = match &tool_state.gesture {
        Some(GestureState::DragCreateShape { tool, .. }) => Some(*tool),
        _ if tool_state.active_tool.arms_property_bar() && selection.is_empty() => Some(tool_state.active_tool),
        _ => None,
    };

    if let Some(tool) = active_create_tool {
        return schema_for_armed_tool(tool, tool_state);
    }

    if selection.is_empty() {
        let orientation = if page.size.width.get() >= page.size.height.get() { PageOrientation::Landscape } else { PageOrientation::Portrait };
        return PropertyBarSchema {
            context: PropertyContext::PageSetup,
            controls: vec![PropertyControl::PageDimensions { width_pt: page.size.width, height_pt: page.size.height, units, orientation }],
        };
    }

    let first = selection[0];
    let bbox = combined_selection_bounds(selection);
    let ref_pt = coralcraft_geom::reference_point(bbox, usize::from(tool_state.origin_point.as_index())).unwrap_or(Point { x: bbox.x0, y: bbox.y0 });
    let origin_ctrl = PropertyControl::OriginSelector { active: tool_state.origin_point };
    let transform_ctrl = PropertyControl::TransformBox {
        x_pt: ref_pt.x,
        y_pt: ref_pt.y,
        width_pt: F64::new(bbox.width()).unwrap_or(F64::ZERO),
        height_pt: F64::new(bbox.height()).unwrap_or(F64::ZERO),
        rotation_deg: F64::ZERO,
    };
    let style_ctrl =
        PropertyControl::StyleSummary { selection_count: selection.len(), has_fill: first.fill.is_some(), has_outline: first.outline.is_some() };

    match &first.kind {
        ObjectKind::Rect(spec) if selection.iter().all(|n| matches!(n.kind, ObjectKind::Rect(_))) => {
            let uniform = spec.radii[0];
            let locked = spec.radii[0] == spec.radii[1] && spec.radii[0] == spec.radii[2] && spec.radii[0] == spec.radii[3];
            PropertyBarSchema {
                context: PropertyContext::Rectangle,
                controls: vec![
                    origin_ctrl,
                    transform_ctrl,
                    PropertyControl::RectangleCorners {
                        corner_kind: spec.corner_kinds[0],
                        uniform_radius_pt: uniform,
                        corners_locked: locked,
                        radii: spec.radii,
                    },
                    style_ctrl,
                ],
            }
        }
        ObjectKind::Ellipse(spec) if selection.iter().all(|n| matches!(n.kind, ObjectKind::Ellipse(_))) => PropertyBarSchema {
            context: PropertyContext::Ellipse,
            controls: vec![
                origin_ctrl,
                transform_ctrl,
                PropertyControl::EllipseParameters { mode: spec.mode, start_deg: spec.start_deg, end_deg: spec.end_deg },
                style_ctrl,
            ],
        },
        ObjectKind::Polygon(spec)
            if selection.iter().all(|n| {
                matches!(
                    &n.kind,
                    ObjectKind::Polygon(p) if p.inner_radius.is_some() == spec.inner_radius.is_some()
                )
            }) =>
        {
            let (context, mode) = if spec.inner_radius.is_some() {
                (PropertyContext::Star, PolygonSubMode::Star)
            } else {
                (PropertyContext::Polygon, PolygonSubMode::Polygon)
            };
            PropertyBarSchema {
                context,
                controls: vec![
                    origin_ctrl,
                    transform_ctrl,
                    PropertyControl::PolygonParameters { mode, points: spec.points, inner_radius_pt: spec.inner_radius },
                    style_ctrl,
                ],
            }
        }
        ObjectKind::ArtisticText { text, font_family, size_pt, .. } if selection.len() == 1 => PropertyBarSchema {
            context: PropertyContext::ArtisticText,
            controls: vec![
                origin_ctrl,
                transform_ctrl,
                PropertyControl::ArtisticTextParameters { font_family: font_family.clone(), font_size_pt: *size_pt, text: text.clone() },
                style_ctrl,
            ],
        },
        _ => PropertyBarSchema { context: PropertyContext::GenericSelection, controls: vec![origin_ctrl, transform_ctrl, style_ctrl] },
    }
}

fn schema_for_armed_tool(tool: ToolKind, tool_state: &ToolState) -> PropertyBarSchema {
    match tool {
        ToolKind::Rectangle => PropertyBarSchema {
            context: PropertyContext::Rectangle,
            controls: vec![PropertyControl::RectangleCorners {
                corner_kind: tool_state.default_rect_corner_kind,
                uniform_radius_pt: tool_state.default_rect_uniform_radius_pt,
                corners_locked: tool_state.default_rect_corners_locked,
                radii: [tool_state.default_rect_uniform_radius_pt; 4],
            }],
        },
        ToolKind::Ellipse => PropertyBarSchema {
            context: PropertyContext::Ellipse,
            controls: vec![PropertyControl::EllipseParameters {
                mode: tool_state.default_ellipse_mode,
                start_deg: tool_state.default_ellipse_start_deg,
                end_deg: tool_state.default_ellipse_end_deg,
            }],
        },
        ToolKind::PolygonStar => {
            let (context, inner_radius_pt) = match tool_state.polygon_mode {
                PolygonSubMode::Polygon => (PropertyContext::Polygon, None),
                PolygonSubMode::Star => (PropertyContext::Star, Some(tool_state.default_star_sharpness)),
            };
            PropertyBarSchema {
                context,
                controls: vec![PropertyControl::PolygonParameters {
                    mode: tool_state.polygon_mode,
                    points: tool_state.default_polygon_points,
                    inner_radius_pt,
                }],
            }
        }
        ToolKind::FreehandBezier => PropertyBarSchema {
            context: PropertyContext::FreehandBezier,
            controls: vec![PropertyControl::CurveModeSelector { mode: tool_state.curve_mode }],
        },
        ToolKind::ArtisticText => PropertyBarSchema {
            context: PropertyContext::ArtisticText,
            controls: vec![PropertyControl::ArtisticTextParameters {
                font_family: tool_state.default_font_family.clone(),
                font_size_pt: tool_state.default_font_size_pt,
                text: String::new(),
            }],
        },
        _ => PropertyBarSchema { context: PropertyContext::GenericSelection, controls: Vec::new() },
    }
}

/// Computes the world-space axis-aligned bounding box of a slice of object nodes.
#[must_use]
pub fn combined_selection_bounds(selection: &[&ObjectNode]) -> Rect {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for node in selection {
        let r = object_local_bounds(&node.kind);
        let corners = [(r.x0.get(), r.y0.get()), (r.x1.get(), r.y0.get()), (r.x1.get(), r.y1.get()), (r.x0.get(), r.y1.get())];
        for (cx, cy) in corners {
            if let Ok(pt) = Point::new(cx, cy)
                && let Ok(p) = node.transform.apply_point(pt)
            {
                min_x = min_x.min(p.x.get());
                min_y = min_y.min(p.y.get());
                max_x = max_x.max(p.x.get());
                max_y = max_y.max(p.y.get());
            }
        }
    }

    let zero = Rect { x0: F64::ZERO, y0: F64::ZERO, x1: F64::ZERO, y1: F64::ZERO };
    if !min_x.is_finite() {
        return zero;
    }
    Rect::new(min_x, min_y, max_x, max_y).unwrap_or(zero)
}

/// Computes local-space bounding box of an [`ObjectKind`].
#[must_use]
pub fn object_local_bounds(kind: &ObjectKind) -> Rect {
    let zero_rect = || Rect { x0: F64::ZERO, y0: F64::ZERO, x1: F64::ZERO, y1: F64::ZERO };
    match kind {
        ObjectKind::Rect(spec) => spec.rect,
        ObjectKind::Ellipse(spec) => spec.rect,
        ObjectKind::Polygon(spec) => {
            let cx = spec.center.x.get();
            let cy = spec.center.y.get();
            let r = spec.radius.get().abs();
            Rect::new(cx - r, cy - r, cx + r, cy + r).unwrap_or_else(|_| zero_rect())
        }
        ObjectKind::Path(path) => {
            let Ok(polys) = flatten_path(path, &FlattenOpts::default()) else {
                return zero_rect();
            };
            let mut min_x = f64::INFINITY;
            let mut min_y = f64::INFINITY;
            let mut max_x = f64::NEG_INFINITY;
            let mut max_y = f64::NEG_INFINITY;
            for poly in &polys {
                for p in &poly.points {
                    min_x = min_x.min(p.x.get());
                    min_y = min_y.min(p.y.get());
                    max_x = max_x.max(p.x.get());
                    max_y = max_y.max(p.y.get());
                }
            }
            if min_x.is_finite() { Rect::new(min_x, min_y, max_x, max_y).unwrap_or_else(|_| zero_rect()) } else { zero_rect() }
        }
        ObjectKind::ArtisticText { text, size_pt, position, .. } => {
            let w = ((text.chars().count() as f64) * size_pt.get() * 0.6).max(10.0);
            let h = size_pt.get().max(10.0);
            let x0 = position.x.get();
            let y0 = position.y.get();
            Rect::new(x0, y0, x0 + w, y0 + h).unwrap_or_else(|_| zero_rect())
        }
        ObjectKind::Group { children } => {
            let refs: Vec<&ObjectNode> = children.iter().collect();
            if refs.is_empty() { zero_rect() } else { combined_selection_bounds(&refs) }
        }
        ObjectKind::Unsupported(u) => u.fallback_bounds.unwrap_or_else(zero_rect),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use coralcraft_doc::{Document, ObjectId};

    #[test]
    fn shortcuts_map_to_classic_coreldraw_tools() {
        let mut state = ToolState::default();
        assert_eq!(state.active_tool, ToolKind::Pick);

        for (key, expected) in [
            (KeyShortcut::F10, ToolKind::Shape),
            (KeyShortcut::Char('z'), ToolKind::Zoom),
            (KeyShortcut::F5, ToolKind::FreehandBezier),
            (KeyShortcut::F6, ToolKind::Rectangle),
            (KeyShortcut::F7, ToolKind::Ellipse),
            (KeyShortcut::Char('Y'), ToolKind::PolygonStar),
            (KeyShortcut::F8, ToolKind::ArtisticText),
            (KeyShortcut::Char('g'), ToolKind::InteractiveFill),
            (KeyShortcut::Escape, ToolKind::Pick),
        ] {
            assert!(state.apply_shortcut(key));
            assert_eq!(state.active_tool, expected);
        }
    }

    #[test]
    fn property_bar_schema_follows_selection_and_armed_tool_contexts() {
        let doc = Document::new_a4_portrait();
        let page = &doc.pages[0];
        let mut tool_state = ToolState::default();

        // 1. Empty selection with Pick tool -> PageSetup
        let schema = build_property_bar_schema(&[], page, Unit::Mm, &tool_state);
        assert_eq!(schema.context, PropertyContext::PageSetup);
        assert!(matches!(schema.controls[0], PropertyControl::PageDimensions { orientation: PageOrientation::Portrait, units: Unit::Mm, .. }));

        // 2. Arming F6 (Rectangle) before dragging -> Rectangle property bar immediately
        tool_state.apply_shortcut(KeyShortcut::F6);
        let schema = build_property_bar_schema(&[], page, Unit::Mm, &tool_state);
        assert_eq!(schema.context, PropertyContext::Rectangle);

        // 3. Selecting a Rectangle -> Rectangle property bar with radii & Round/Scalloped/Chamfered
        tool_state.apply_shortcut(KeyShortcut::Escape);
        let r = Rect::new(10.0, 20.0, 110.0, 70.0).unwrap();
        let rect_node = ObjectNode::new(
            ObjectId(10),
            Some("Box".to_owned()),
            ObjectKind::Rect(RectSpec::uniform(r, 8.0, CornerKind::Chamfered).unwrap()),
            None,
            None,
        );
        let schema = build_property_bar_schema(&[&rect_node], page, Unit::Mm, &tool_state);
        assert_eq!(schema.context, PropertyContext::Rectangle);
        assert!(
            schema
                .controls
                .iter()
                .any(|c| matches!(c, PropertyControl::RectangleCorners { corner_kind: CornerKind::Chamfered, corners_locked: true, .. }))
        );

        // 4. Selecting an Ellipse -> Ellipse property bar with mode & angles
        let ellipse_node =
            ObjectNode::new(ObjectId(11), Some("Pie".to_owned()), ObjectKind::Ellipse(EllipseSpec::pie(r, 15.0, 270.0).unwrap()), None, None);
        let schema = build_property_bar_schema(&[&ellipse_node], page, Unit::Mm, &tool_state);
        assert_eq!(schema.context, PropertyContext::Ellipse);
        assert!(schema.controls.iter().any(|c| matches!(c, PropertyControl::EllipseParameters { mode: EllipseMode::Pie, .. })));

        // 5. Selecting a Star -> Star property bar with sides & inner radius
        let star_node = ObjectNode::new(
            ObjectId(12),
            Some("Star".to_owned()),
            ObjectKind::Polygon(PolygonSpec::star(Point::new(100.0, 100.0).unwrap(), 40.0, 18.0, 6, 0.0).unwrap()),
            None,
            None,
        );
        let schema = build_property_bar_schema(&[&star_node], page, Unit::Mm, &tool_state);
        assert_eq!(schema.context, PropertyContext::Star);

        // 6. Heterogeneous multi-selection (Rect + Ellipse) -> GenericSelection
        let schema = build_property_bar_schema(&[&rect_node, &ellipse_node], page, Unit::Mm, &tool_state);
        assert_eq!(schema.context, PropertyContext::GenericSelection);

        // 7. Single Artistic Text selection -> ArtisticText context
        let text_node = ObjectNode::new(
            ObjectId(13),
            Some("Headline".to_owned()),
            ObjectKind::ArtisticText {
                text: "CoralCraft".to_owned(),
                font_family: "Sans".to_owned(),
                size_pt: F64::new(36.0).unwrap(),
                position: Point::new(20.0, 40.0).unwrap(),
            },
            None,
            None,
        );
        let schema = build_property_bar_schema(&[&text_node], page, Unit::Mm, &tool_state);
        assert_eq!(schema.context, PropertyContext::ArtisticText);
    }

    #[test]
    fn drag_create_shape_supports_shift_square_and_alt_center_constraints() {
        let mut tool_state = ToolState::default();
        tool_state.apply_shortcut(KeyShortcut::F6);
        tool_state.default_rect_corner_kind = CornerKind::Scalloped;
        tool_state.default_rect_uniform_radius_pt = F64::new(6.0).unwrap();

        tool_state.begin_drag(Point::new(100.0, 100.0).unwrap(), true, false);
        tool_state.update_drag(Point::new(180.0, 140.0).unwrap(), true, false);
        let kind = tool_state.finish_create_shape().unwrap();
        match kind {
            ObjectKind::Rect(spec) => {
                assert!((spec.rect.width() - 80.0).abs() < 1e-9);
                assert!((spec.rect.height() - 80.0).abs() < 1e-9);
                assert_eq!(spec.corner_kinds, [CornerKind::Scalloped; 4]);
                assert_eq!(spec.radii[0].get(), 6.0);
            }
            other => panic!("expected Rect, got {other:?}"),
        }
    }
}
