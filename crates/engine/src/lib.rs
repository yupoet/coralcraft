//! Core application engine (`EditorEngine`) combining multi-page document state,
//! tool gestures, undo/redo command history, hit-testing, and Property Bar snapshots (L5).

use coralcraft_color::Color;
use coralcraft_doc::{
    Document, FillStyle, Layer, LayerId, LayerRole, LineCap, LineJoin, ObjectId, ObjectKind, ObjectNode, OutlineStyle, Page, PageId,
    ParagraphTextSpec, Residual, TextAlignment, TextCaseKind, TransparencyStyle, VectorEffect, apply_text_case,
};
use coralcraft_geom::{
    Affine2, ArcLengthTable, AxonometricPlane, BooleanOp, CornerKind, DistortionKind, EllipseMode, F64, FillRule, Path, PathElement, Point, Rect,
    RectSpec, Size, Unit, artistic_text_to_path, boolean_paths, break_path_at_node, chaikin_smooth_path, close_open_subpaths,
    convert_segment_to_curve, convert_segment_to_line, delete_curve_node_at, delete_segment_at_index, divide_overlapping_paths,
    erase_path_within_circle, expand_tapered_stroke_profile, hatch_path_region, insert_curve_node_at, join_open_subpaths, knife_cut_path_by_line,
    measure_path_length_and_area, morph_paths_linear, offset_open_or_closed_path, project_path_axonometric, radial_push_path_around_point,
    reference_point, reverse_path_direction, roughen_path_segments, simplify_path_nodes, smear_path_from_point, smooth_curve_node_at,
    smooth_path_around_point, subdivide_all_path_segments, trace_bitmap_to_path, transform_curve_nodes, twirl_path_around_point,
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
    /// Simplifies `object_id`'s path using Ramer-Douglas-Peucker node reduction (`Shape Tool -> Reduce Nodes`).
    ReducePathNodes { object_id: ObjectId, tolerance_pt: F64 },
    /// Sets exact position `(x_pt, y_pt)` and/or dimensions `(width_pt, height_pt)` of the current selection relative to the 9-point Object Origin (`0..=8`).
    SetSelectionBounds { x_pt: Option<F64>, y_pt: Option<F64>, width_pt: Option<F64>, height_pt: Option<F64>, origin_index: u8 },
    /// Adds a new normal layer with `name` on the active page (`Object -> Layer -> New Layer`).
    AddLayer { name: String },
    /// Updates `visible`, `locked`, `printable`, and/or `opacity` on `layer_id` (`Objects` Docker).
    SetLayerProperties { layer_id: LayerId, visible: Option<bool>, locked: Option<bool>, printable: Option<bool>, opacity: Option<F64> },
    /// Resets the affine transformation matrix of all selected unlocked objects to `Affine2::IDENTITY` (`Object -> Transformations -> Clear Transformations`).
    ClearTransformations,
    /// Converts `object_id` between `ObjectKind::ArtisticText` and `ObjectKind::ParagraphText` (`Text -> Convert to Paragraph/Artistic Text` / `Ctrl+F8`).
    ToggleTextKind { object_id: ObjectId },
    /// Replaces all occurrences of `find` with `replace` across all unlocked text objects (`Edit -> Find and Replace -> Replace Text`).
    FindAndReplaceText { find: String, replace: String },
    /// Changes the letter case of `object_id` (`Text -> Change Case` / `Shift+F3`).
    ChangeTextCase { object_id: ObjectId, case_kind: TextCaseKind },
    /// Subdivides the path segment at `element_index` on `object_id`, inserting a new node at `t = 0.5` (`Shape Tool -> Add Nodes` / `+`).
    InsertCurveNode { object_id: ObjectId, element_index: usize },
    /// Deletes the path node at `element_index` on `object_id` (`Shape Tool -> Delete Nodes` / `-`).
    DeleteCurveNode { object_id: ObjectId, element_index: usize },
    /// Converts the path segment at `element_index` on `object_id` into a straight `LineTo` (`Shape Tool -> To Line`).
    ConvertSegmentToLine { object_id: ObjectId, element_index: usize },
    /// Converts the path segment at `element_index` on `object_id` into a `CubicTo` Bézier curve (`Shape Tool -> To Curve`).
    ConvertSegmentToCurve { object_id: ObjectId, element_index: usize },
    /// Sets or clears the canvas background fill on `page_index` (`Layout -> Page Background`).
    SetPageBackground { page_index: usize, background: Option<FillStyle> },
    /// Sets the print bleed margin (in points) on `page_index` (`Layout -> Page Setup -> Bleed`).
    SetPageBleed { page_index: usize, bleed_pt: F64 },
    /// Updates the opacity (`0.0..=1.0`) of `object_id` (`Properties` Docker).
    SetObjectOpacity { object_id: ObjectId, opacity: F64 },
    /// Toggles visibility (`Object -> Hide / Show Object`) of `object_id`.
    SetObjectVisible { object_id: ObjectId, visible: bool },
    /// Updates the user-assigned label of `object_id` in the `Objects` Docker.
    RenameObject { object_id: ObjectId, name: Option<String> },
    /// Deletes `layer_id` from its page when the page has more than one layer (`Object -> Layer -> Delete Layer`).
    DeleteLayer { layer_id: LayerId },
    /// Reorders the layer at `from_index` to `to_index` on `page_index` (`Objects` Docker).
    MoveLayer { page_index: usize, from_index: usize, to_index: usize },
    /// Inverts the selection among unlocked, visible objects on the active target layer (`Edit -> Select All -> Invert Selection`).
    InvertSelection,
    /// Updates `width_pt`, `behind_fill`, and/or `scale_with_object` on selected objects' `OutlineStyle` (`F12` Outline Pen).
    SetOutlineProperties { width_pt: Option<F64>, behind_fill: Option<bool>, scale_with_object: Option<bool> },
    /// Selects all unlocked, visible objects on the active layer matching `kind` (`"rect"`, `"ellipse"`, `"polygon"`, `"path"`, `"text"`, `"bitmap"`, `"group"`).
    SelectByKind { kind: String },
    /// Updates the document title metadata (`File -> Document Properties`).
    SetDocumentTitle { title: String },
    /// Applies a localized rotational swirl around `center` within `radius_pt` on `object_id` (`Twirl Tool`).
    TwirlObjectPath { object_id: ObjectId, center: Point, radius_pt: F64, angle_deg: F64 },
    /// Removes all non-destructive vector effects from `object_id` (`Object -> Clear Effect`).
    ClearEffects { object_id: ObjectId },
    /// Copies the non-destructive vector effect stack from `source_id` to all selected unlocked objects (`Effects -> Clone Effect`).
    CloneEffectsFrom { source_id: ObjectId },
    /// Radially attracts (`strength > 0`) or repels (`strength < 0`) vertices within `radius_pt` of `center` on `object_id` (`Attract and Repel Tool`).
    AttractRepelObjectPath { object_id: ObjectId, center: Point, radius_pt: F64, strength: F64 },
    /// Directionally drags vertices within `radius_pt` of `origin` along `(dx_pt, dy_pt)` on `object_id` (`Smear Tool`).
    SmearObjectPath { object_id: ObjectId, origin: Point, radius_pt: F64, dx_pt: F64, dy_pt: F64 },
    /// Updates multi-column layout (`columns`, `gutter_pt`, `line_spacing`, `alignment`) on a `ParagraphText` frame (`Text -> Columns / Paragraph Formatting`).
    SetParagraphTextLayout {
        object_id: ObjectId,
        columns: Option<u32>,
        gutter_pt: Option<F64>,
        line_spacing: Option<F64>,
        alignment: Option<TextAlignment>,
    },
    /// Subdivides segments of `object_id` with serrated zigzag normal teeth (`Roughen Tool`).
    RoughenObjectPath { object_id: ObjectId, tooth_height_pt: F64, frequency: u32 },
    /// Updates `text`, `font_family`, and/or `size_pt` on an `ArtisticText` or `ParagraphText` object (`Text -> Edit Text` / `Ctrl+Shift+T`).
    SetArtisticTextProperties { object_id: ObjectId, text: Option<String>, font_family: Option<String>, size_pt: Option<F64> },
    /// Applies localized Jacobi Laplacian vertex smoothing within `radius_pt` around `center` on `object_id` (`Smooth Tool`).
    SmoothObjectPath { object_id: ObjectId, center: Point, radius_pt: F64, strength: F64, iterations: u32 },
    /// Slices `object_id` along the cut line `(line_start, line_end)` with kerf `gap_pt` (`Knife Tool`).
    KnifeCutObject { object_id: ObjectId, line_start: Point, line_end: Point, gap_pt: F64 },
    /// Crops unlocked selected objects (or all unlocked objects on the active layer when selection is empty) to `rect` (`Crop Tool`).
    CropToRect { rect: Rect },
    /// Moves `object_id` to `target_layer_id` on the active page (`Objects` Docker).
    MoveObjectToLayer { object_id: ObjectId, target_layer_id: LayerId },
    /// Subtracts a circular eraser nib of `radius_pt` centered at `center` from `object_id` (`Eraser Tool X`).
    EraseObjectCircle { object_id: ObjectId, center: Point, radius_pt: F64 },
    /// Updates `snap_to_objects` and/or `snap_to_page` preferences (`View -> Snap To`).
    SetSnapSettings { snap_to_objects: Option<bool>, snap_to_page: Option<bool> },
    /// Removes the path segment at `element_index` on `object_id` (`Virtual Segment Delete Tool`).
    VirtualSegmentDelete { object_id: ObjectId, element_index: usize },
    /// Creates a new filled `ObjectKind::Path` from the enclosed region of overlapping closed shapes containing `point` (`Smart Fill Tool`).
    SmartFillAtPoint { point: Point, fill: FillStyle, outline: Option<OutlineStyle> },
    /// Offsets `object_id`'s path by `distance_pt` (`Parallel Drawing`), either replacing in place or creating a parallel copy.
    OffsetObjectPath { object_id: ObjectId, distance_pt: F64, copy: bool },
    /// Updates the winding fill rule (`FillRule::NonZero` or `FillRule::EvenOdd`) of `object_id` (`Properties` Docker).
    SetPathFillRule { object_id: ObjectId, fill_rule: FillRule },
    /// Swaps the bounding-box centers of `first_id` and `second_id` (`Object -> Align and Distribute -> Swap`).
    SwapObjectPositions { first_id: ObjectId, second_id: ObjectId },
    /// Centers the current selection horizontally and/or vertically on the active page (`Object -> Align and Distribute -> Center to Page`).
    CenterSelectionToPage { horizontal: bool, vertical: bool },
    /// Converts the node at the end of `element_index` on `object_id` into a smooth symmetrical cubic Bézier node (`Shape Tool F10 -> Smooth / Symmetrical Node`).
    SmoothCurveNode { object_id: ObjectId, element_index: usize, tension: F64 },
    /// Duplicates the current selection in a `rows × cols` 2D grid with `(spacing_x_pt, spacing_y_pt)` step offsets (`Object -> Transformations -> Array`).
    ArrayDuplicateSelection { rows: u32, cols: u32, spacing_x_pt: F64, spacing_y_pt: F64 },
    /// Adds a horizontal or vertical ruler guideline at `position_pt` on the active page's `Guides` layer (`View -> Guidelines`).
    AddGuideline { horizontal: bool, position_pt: F64 },
    /// Removes all guidelines from the active page's `Guides` layer (`View -> Guidelines -> Clear Guidelines`).
    ClearGuidelines,
    /// Duplicates the current selection `count` times in a polar/radial circle around `center` over `total_angle_deg` (`Object -> Transformations -> Rotate`).
    CircularDuplicateSelection { count: u32, center: Point, total_angle_deg: F64, rotate_objects: bool },
    /// Resizes the active page and translates page objects so the selection (or all unlocked visible objects) is framed with `margin_pt` (`Layout -> Fit Page to Drawing`).
    FitPageToSelection { margin_pt: F64 },
    /// Updates `cap`, `join`, and/or `miter_limit` on selected objects' `OutlineStyle` or creation defaults (`F12` Outline Pen).
    SetOutlineCapAndJoin { cap: Option<LineCap>, join: Option<LineJoin>, miter_limit: Option<F64> },
    /// Aligns the endpoints of `element_indices` on `object_id` horizontally (same Y) and/or vertically (same X) to the first index (`Shape Tool F10 -> Align Nodes`).
    AlignCurveNodes { object_id: ObjectId, element_indices: Vec<usize>, horizontal: bool, vertical: bool },
    /// Shifts HSB hue (`hue_delta_deg`), saturation (`saturation_delta`), and brightness (`brightness_delta`) across all fills, fountain stops, patterns, and outlines of the selection (`Effects -> Adjust -> Hue/Saturation/Lightness`).
    ShiftSelectionColors { hue_delta_deg: F64, saturation_delta: F64, brightness_delta: F64 },
    /// Subdivides every segment of `object_id` at its midpoint $t = 0.5$ (`Shape Tool F10 -> Select All Nodes -> Add Nodes (+)`).
    SubdividePathSegments { object_id: ObjectId },
    /// Converts all fills, fountain stops, two-color patterns, and outlines of the selection via sRGB inversion (`invert`) and/or BT.709 luminance grayscale (`grayscale`) (`Effects -> Transform -> Invert Colors` / `Edit -> Find and Replace -> Convert to Grayscale`).
    ConvertSelectionColors { invert: bool, grayscale: bool },
    /// Scales (`scale_x`, `scale_y`) and rotates (`rotate_deg`) the curve nodes at `element_indices` on `object_id` around their centroid (`Shape Tool F10 -> Stretch/Scale & Rotate Nodes`).
    TransformCurveNodes { object_id: ObjectId, element_indices: Vec<usize>, scale_x: F64, scale_y: F64, rotate_deg: F64 },
    /// Selects all unlocked visible objects on the active layer whose fill (`match_fill`) and/or outline (`match_outline`) color is within `tolerance_u8` of `color` (`Edit -> Find and Replace -> Find Objects by Color`).
    SelectByColor { color: Color, match_fill: bool, match_outline: bool, tolerance_u8: u8 },
    /// Breaks a curve at `element_index` on `object_id`, opening a closed subpath or splitting an open subpath into two (`Shape Tool F10 -> Break Curve`).
    BreakPathAtNode { object_id: ObjectId, element_index: usize },
    /// Scales all selected unlocked objects around their centers to match the width (`match_width`) and/or height (`match_height`) of the last selected key object (`Object -> Align and Distribute -> Equalize Size`).
    EqualizeSelectionSize { match_width: bool, match_height: bool },
    /// Topologically morphs the world-space contour of `start_id` toward `end_id` at factor `t` (`0.0..=1.0`) with `sample_count` vertices and adds the resulting [`ObjectKind::Path`] (`Effects -> Blend -> Morph Paths`).
    MorphBetweenObjects { start_id: ObjectId, end_id: ObjectId, t: F64, sample_count: usize },
    /// Spaces all selected unlocked objects (`>= 2`) along the horizontal (`horizontal = true`) or vertical (`horizontal = false`) axis with an exact gap `gap_pt` between adjacent bounding boxes (`Object -> Align and Distribute -> Space with Exact Gap`).
    DistributeExactGap { horizontal: bool, gap_pt: F64 },
    /// Splits two selected overlapping closed objects into non-overlapping regions (`A \ B`, `A ∩ B`, `B \ A`) (`Object -> Shaping -> Divide`).
    DivideSelection,
    /// Positions all selected unlocked objects other than `spine_object_id` evenly along the normalized arc length of `spine_object_id` (`Object -> Align and Distribute -> Distribute Along Path`).
    DistributeSelectionAlongPath { spine_object_id: ObjectId, rotate_to_tangent: bool },
    /// Globally replaces colors matching `from_color` within `tolerance_u8` with `to_color` across fills, fountain stops, two-color patterns, and outlines (`Edit -> Find and Replace -> Replace Objects -> Replace a Color`).
    RecolorArtwork { from_color: Color, to_color: Color, tolerance_u8: u8, selection_only: bool },
    /// Replaces `object_id` with parallel/cross-hatched vector engraving lines at `angle_deg` and `spacing_pt` (`Object -> Hatch / Engraving Fill`).
    HatchObjectPath { object_id: ObjectId, angle_deg: F64, spacing_pt: F64, cross_hatch: bool },
    /// Generates `steps` (`1..=32`) standalone intermediate morphed vector [`ObjectKind::Path`] objects between `start_id` and `end_id` with interpolated fill and outline colors (`Effects -> Blend -> Break Blend Apart`).
    CreateIntermediateBlendPaths { start_id: ObjectId, end_id: ObjectId, steps: u32, sample_count: usize },
    /// Projects all selected unlocked objects onto one of the three $30^\circ$ axonometric/isometric planes (`Object -> Transformations -> Isometric Projection`).
    ProjectSelectionAxonometric { plane: AxonometricPlane },
    /// Packs all selected unlocked objects into a multi-column grid starting at `origin` with `cols` columns and `gap_pt` horizontal and vertical spacing (`Object -> Align and Distribute -> Pack in Grid`).
    PackSelectionInGrid { origin: Point, cols: u32, gap_pt: F64 },
    /// Computes the symmetric difference (XOR / Exclude) of `>= 2` selected unlocked objects (`Object -> Shaping -> Exclude`).
    ExcludeSelection,
    /// Iteratively separates overlapping selected objects by repulsion so adjacent bounding boxes maintain at least `min_gap_pt` clearance (`Object -> Align and Distribute -> Resolve Overlaps`).
    ResolveSelectionOverlaps { min_gap_pt: F64, iterations: u32 },
    /// Expands the spine of `object_id` using a piecewise-linear variable-width profile `widths_pt` (`>= 2` stops) into a closed filled ribbon path (`Artistic Media / Variable Width Tool`).
    ApplyVariableWidthStroke { object_id: ObjectId, widths_pt: Vec<F64> },
    /// Smooths the polygonal/curve vertices of `object_id` via Chaikin corner-cutting (`1/4` and `3/4` subdivision) for `iterations` (`1..=6`) (`Object -> Shaping -> Smooth / Corner-Cut Curve`).
    ChaikinSmoothObjectPath { object_id: ObjectId, iterations: u32 },
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
            EngineCommand::ReducePathNodes { object_id, tolerance_pt } => {
                if tolerance_pt.get() < 0.0 {
                    return false;
                }
                let valid = self.selected_or_all_node(*object_id).is_some_and(|o| !o.locked && object_to_path(o).is_some());
                if !valid {
                    return false;
                }
            }
            EngineCommand::SetSelectionBounds { x_pt, y_pt, width_pt, height_pt, origin_index } => {
                if *origin_index > 8 || (x_pt.is_none() && y_pt.is_none() && width_pt.is_none() && height_pt.is_none()) {
                    return false;
                }
                if width_pt.is_some_and(|w| w.get() <= 1e-6) || height_pt.is_some_and(|h| h.get() <= 1e-6) {
                    return false;
                }
                let sel = &self.selection;
                let has_unlocked = self.current_target_objects().iter().any(|o| !o.locked && sel.contains(&o.id));
                if !has_unlocked {
                    return false;
                }
            }
            EngineCommand::AddLayer { name } => {
                if name.trim().is_empty() || self.document.pages.is_empty() {
                    return false;
                }
            }
            EngineCommand::SetLayerProperties { layer_id, visible, locked, printable, opacity } => {
                if visible.is_none() && locked.is_none() && printable.is_none() && opacity.is_none() {
                    return false;
                }
                let exists = self.document.pages.iter().flat_map(|p| &p.layers).chain(&self.document.master_layers).any(|l| l.id == *layer_id);
                if !exists {
                    return false;
                }
            }
            EngineCommand::ClearTransformations => {
                let has_unlocked = self.selected_nodes().iter().any(|o| !o.locked);
                if !has_unlocked {
                    return false;
                }
            }
            EngineCommand::ToggleTextKind { object_id } => {
                let valid = self
                    .selected_or_all_node(*object_id)
                    .is_some_and(|o| !o.locked && matches!(o.kind, ObjectKind::ArtisticText { .. } | ObjectKind::ParagraphText(_)));
                if !valid {
                    return false;
                }
            }
            EngineCommand::FindAndReplaceText { find, .. } => {
                if find.is_empty() {
                    return false;
                }
            }
            EngineCommand::ChangeTextCase { object_id, .. } => {
                let valid = self
                    .selected_or_all_node(*object_id)
                    .is_some_and(|o| !o.locked && matches!(o.kind, ObjectKind::ArtisticText { .. } | ObjectKind::ParagraphText(_)));
                if !valid {
                    return false;
                }
            }
            EngineCommand::InsertCurveNode { object_id, element_index } => {
                let valid = self
                    .selected_or_all_node(*object_id)
                    .is_some_and(|o| !o.locked && object_to_path(o).is_some_and(|p| insert_curve_node_at(&p, *element_index).is_ok()));
                if !valid {
                    return false;
                }
            }
            EngineCommand::DeleteCurveNode { object_id, element_index } => {
                let valid = self
                    .selected_or_all_node(*object_id)
                    .is_some_and(|o| !o.locked && object_to_path(o).is_some_and(|p| delete_curve_node_at(&p, *element_index).is_ok()));
                if !valid {
                    return false;
                }
            }
            EngineCommand::ConvertSegmentToLine { object_id, element_index } => {
                let valid = self
                    .selected_or_all_node(*object_id)
                    .is_some_and(|o| !o.locked && object_to_path(o).is_some_and(|p| convert_segment_to_line(&p, *element_index).is_ok()));
                if !valid {
                    return false;
                }
            }
            EngineCommand::ConvertSegmentToCurve { object_id, element_index } => {
                let valid = self
                    .selected_or_all_node(*object_id)
                    .is_some_and(|o| !o.locked && object_to_path(o).is_some_and(|p| convert_segment_to_curve(&p, *element_index).is_ok()));
                if !valid {
                    return false;
                }
            }
            EngineCommand::SetPageBackground { page_index, .. } | EngineCommand::SetPageBleed { page_index, .. } => {
                if *page_index >= self.document.pages.len() {
                    return false;
                }
            }
            EngineCommand::DeleteLayer { layer_id } => {
                let can_delete = self.document.pages.iter().any(|p| p.layers.len() > 1 && p.layers.iter().any(|l| l.id == *layer_id));
                if !can_delete {
                    return false;
                }
            }
            EngineCommand::MoveLayer { page_index, from_index, to_index } => {
                let valid = self
                    .document
                    .pages
                    .get(*page_index)
                    .is_some_and(|p| *from_index < p.layers.len() && *to_index < p.layers.len() && *from_index != *to_index);
                if !valid {
                    return false;
                }
            }
            EngineCommand::SetOutlineProperties { width_pt, behind_fill, scale_with_object } => {
                if width_pt.is_none() && behind_fill.is_none() && scale_with_object.is_none() {
                    return false;
                }
            }
            EngineCommand::SetDocumentTitle { title } => {
                if title.trim().is_empty() {
                    return false;
                }
            }
            EngineCommand::TwirlObjectPath { object_id, center, radius_pt, angle_deg } => {
                let valid = self.selected_or_all_node(*object_id).is_some_and(|o| {
                    !o.locked && object_to_path(o).is_some_and(|p| twirl_path_around_point(&p, *center, radius_pt.get(), angle_deg.get()).is_ok())
                });
                if !valid {
                    return false;
                }
            }
            EngineCommand::AttractRepelObjectPath { object_id, center, radius_pt, strength } => {
                let valid = self.selected_or_all_node(*object_id).is_some_and(|o| {
                    !o.locked
                        && object_to_path(o).is_some_and(|p| radial_push_path_around_point(&p, *center, radius_pt.get(), strength.get()).is_ok())
                });
                if !valid {
                    return false;
                }
            }
            EngineCommand::SmearObjectPath { object_id, origin, radius_pt, dx_pt, dy_pt } => {
                let valid = self.selected_or_all_node(*object_id).is_some_and(|o| {
                    !o.locked
                        && object_to_path(o).is_some_and(|p| smear_path_from_point(&p, *origin, radius_pt.get(), dx_pt.get(), dy_pt.get()).is_ok())
                });
                if !valid {
                    return false;
                }
            }
            EngineCommand::SetParagraphTextLayout { object_id, .. } => {
                let valid = self.selected_or_all_node(*object_id).is_some_and(|o| !o.locked && matches!(o.kind, ObjectKind::ParagraphText(_)));
                if !valid {
                    return false;
                }
            }
            EngineCommand::RoughenObjectPath { object_id, tooth_height_pt, frequency } => {
                let valid = self.selected_or_all_node(*object_id).is_some_and(|o| {
                    !o.locked && object_to_path(o).is_some_and(|p| roughen_path_segments(&p, tooth_height_pt.get(), *frequency).is_ok())
                });
                if !valid {
                    return false;
                }
            }
            EngineCommand::SetArtisticTextProperties { object_id, .. } => {
                let valid = self
                    .selected_or_all_node(*object_id)
                    .is_some_and(|o| !o.locked && matches!(o.kind, ObjectKind::ArtisticText { .. } | ObjectKind::ParagraphText(_)));
                if !valid {
                    return false;
                }
            }
            EngineCommand::SmoothObjectPath { object_id, center, radius_pt, strength, iterations } => {
                let valid = self.selected_or_all_node(*object_id).is_some_and(|o| {
                    !o.locked
                        && object_to_path(o)
                            .is_some_and(|p| smooth_path_around_point(&p, *center, radius_pt.get(), strength.get(), *iterations).is_ok())
                });
                if !valid {
                    return false;
                }
            }
            EngineCommand::KnifeCutObject { object_id, line_start, line_end, gap_pt } => {
                let valid = self.selected_or_all_node(*object_id).is_some_and(|o| {
                    !o.locked && object_to_path(o).is_some_and(|p| knife_cut_path_by_line(&p, *line_start, *line_end, gap_pt.get()).is_ok())
                });
                if !valid {
                    return false;
                }
            }
            EngineCommand::CropToRect { rect } => {
                if rect.width() <= 0.0 || rect.height() <= 0.0 {
                    return false;
                }
                let has_unlocked = self.current_target_objects().iter().any(|o| !o.locked && o.visible);
                if !has_unlocked {
                    return false;
                }
            }
            EngineCommand::MoveObjectToLayer { object_id, target_layer_id } => {
                let page = self.current_page();
                let dst_ok = page.layers.iter().any(|l| l.id == *target_layer_id && !l.locked);
                let src_ok =
                    page.layers.iter().any(|l| l.id != *target_layer_id && !l.locked && l.objects.iter().any(|o| o.id == *object_id && !o.locked));
                if !dst_ok || !src_ok {
                    return false;
                }
            }
            EngineCommand::EraseObjectCircle { object_id, center, radius_pt } => {
                let valid = self
                    .selected_or_all_node(*object_id)
                    .is_some_and(|o| !o.locked && object_to_path(o).is_some_and(|p| erase_path_within_circle(&p, *center, radius_pt.get()).is_ok()));
                if !valid {
                    return false;
                }
            }
            EngineCommand::SetSnapSettings { snap_to_objects, snap_to_page } => {
                if snap_to_objects.is_none() && snap_to_page.is_none() {
                    return false;
                }
            }
            EngineCommand::VirtualSegmentDelete { object_id, element_index } => {
                let valid = self
                    .selected_or_all_node(*object_id)
                    .is_some_and(|o| !o.locked && object_to_path(o).is_some_and(|p| delete_segment_at_index(&p, *element_index).is_ok()));
                if !valid {
                    return false;
                }
            }
            EngineCommand::SmartFillAtPoint { point, .. } => {
                let px = point.x.get();
                let py = point.y.get();
                let has_enclosing = self.current_target_objects().iter().any(|o| {
                    !o.locked
                        && o.visible
                        && object_to_path(o)
                            .and_then(|p| p.transform(o.transform).ok())
                            .and_then(|wp| wp.bounds())
                            .is_some_and(|b| px >= b.x0.get() && px <= b.x1.get() && py >= b.y0.get() && py <= b.y1.get())
                });
                if !has_enclosing {
                    return false;
                }
            }
            EngineCommand::OffsetObjectPath { object_id, distance_pt, .. } => {
                let valid = self
                    .selected_or_all_node(*object_id)
                    .is_some_and(|o| !o.locked && object_to_path(o).is_some_and(|p| offset_open_or_closed_path(&p, distance_pt.get()).is_ok()));
                if !valid {
                    return false;
                }
            }
            EngineCommand::SetPathFillRule { object_id, .. } => {
                let valid = self.selected_or_all_node(*object_id).is_some_and(|o| !o.locked && object_to_path(o).is_some());
                if !valid {
                    return false;
                }
            }
            EngineCommand::SwapObjectPositions { first_id, second_id } => {
                if first_id == second_id {
                    return false;
                }
                let ok1 = self.selected_or_all_node(*first_id).is_some_and(|o| !o.locked);
                let ok2 = self.selected_or_all_node(*second_id).is_some_and(|o| !o.locked);
                if !ok1 || !ok2 {
                    return false;
                }
            }
            EngineCommand::CenterSelectionToPage { horizontal, vertical } => {
                if !horizontal && !vertical {
                    return false;
                }
                let sel_nodes: Vec<&ObjectNode> = self.selected_nodes().into_iter().filter(|o| !o.locked).collect();
                if sel_nodes.is_empty() {
                    return false;
                }
            }
            EngineCommand::SmoothCurveNode { object_id, element_index, tension } => {
                let valid = self
                    .selected_or_all_node(*object_id)
                    .is_some_and(|o| !o.locked && object_to_path(o).is_some_and(|p| smooth_curve_node_at(&p, *element_index, tension.get()).is_ok()));
                if !valid {
                    return false;
                }
            }
            EngineCommand::ArrayDuplicateSelection { rows, cols, .. } => {
                if *rows == 0 || *cols == 0 || (*rows == 1 && *cols == 1) {
                    return false;
                }
                let has_unlocked = self.selected_nodes().iter().any(|o| !o.locked);
                if !has_unlocked {
                    return false;
                }
            }
            EngineCommand::AddGuideline { position_pt, .. } => {
                if !position_pt.get().is_finite() {
                    return false;
                }
            }
            EngineCommand::ClearGuidelines => {
                let has_guides = self.current_page().layers.iter().any(|l| l.role == LayerRole::Guides && !l.objects.is_empty());
                if !has_guides {
                    return false;
                }
            }
            EngineCommand::CircularDuplicateSelection { count, total_angle_deg, .. } => {
                if *count == 0 || total_angle_deg.get().abs() <= 1e-6 {
                    return false;
                }
                let has_unlocked = self.selected_nodes().iter().any(|o| !o.locked);
                if !has_unlocked {
                    return false;
                }
            }
            EngineCommand::FitPageToSelection { margin_pt } => {
                if margin_pt.get() < 0.0 || !margin_pt.get().is_finite() {
                    return false;
                }
                let has_targets = if self.selection.is_empty() {
                    self.current_target_objects().iter().any(|o| !o.locked && o.visible)
                } else {
                    self.selected_nodes().iter().any(|o| !o.locked)
                };
                if !has_targets {
                    return false;
                }
            }
            EngineCommand::SetOutlineCapAndJoin { cap, join, miter_limit } => {
                if cap.is_none() && join.is_none() && miter_limit.is_none() {
                    return false;
                }
            }
            EngineCommand::AlignCurveNodes { object_id, element_indices, horizontal, vertical } => {
                if element_indices.len() < 2 || (!horizontal && !vertical) {
                    return false;
                }
                let valid = self.selected_or_all_node(*object_id).is_some_and(|o| {
                    !o.locked
                        && object_to_path(o).is_some_and(|p| {
                            element_indices.iter().all(|&idx| idx < p.elements.len() && !matches!(p.elements[idx], PathElement::Close))
                        })
                });
                if !valid {
                    return false;
                }
            }
            EngineCommand::ShiftSelectionColors { hue_delta_deg, saturation_delta, brightness_delta } => {
                if hue_delta_deg.get().abs() <= 1e-6 && saturation_delta.get().abs() <= 1e-6 && brightness_delta.get().abs() <= 1e-6 {
                    return false;
                }
                let has_unlocked = self.selected_nodes().iter().any(|o| !o.locked);
                if !has_unlocked {
                    return false;
                }
            }
            EngineCommand::SubdividePathSegments { object_id } => {
                let valid = self
                    .selected_or_all_node(*object_id)
                    .is_some_and(|o| !o.locked && object_to_path(o).is_some_and(|p| subdivide_all_path_segments(&p).is_ok()));
                if !valid {
                    return false;
                }
            }
            EngineCommand::ConvertSelectionColors { invert, grayscale } => {
                if !invert && !grayscale {
                    return false;
                }
                let has_unlocked = self.selected_nodes().iter().any(|o| !o.locked);
                if !has_unlocked {
                    return false;
                }
            }
            EngineCommand::TransformCurveNodes { object_id, element_indices, scale_x, scale_y, rotate_deg } => {
                let valid = self.selected_or_all_node(*object_id).is_some_and(|o| {
                    !o.locked
                        && object_to_path(o)
                            .is_some_and(|p| transform_curve_nodes(&p, element_indices, scale_x.get(), scale_y.get(), rotate_deg.get()).is_ok())
                });
                if !valid {
                    return false;
                }
            }
            EngineCommand::SelectByColor { match_fill, match_outline, .. } => {
                if !match_fill && !match_outline {
                    return false;
                }
            }
            EngineCommand::BreakPathAtNode { object_id, element_index } => {
                let valid = self
                    .selected_or_all_node(*object_id)
                    .is_some_and(|o| !o.locked && object_to_path(o).is_some_and(|p| break_path_at_node(&p, *element_index).is_ok()));
                if !valid {
                    return false;
                }
            }
            EngineCommand::EqualizeSelectionSize { match_width, match_height } => {
                if !match_width && !match_height {
                    return false;
                }
                let unlocked_count = self.selected_nodes().iter().filter(|o| !o.locked).count();
                if unlocked_count < 2 {
                    return false;
                }
            }
            EngineCommand::MorphBetweenObjects { start_id, end_id, t, sample_count } => {
                if start_id == end_id || !(0.0..=1.0).contains(&t.get()) || *sample_count < 3 {
                    return false;
                }
                let valid_a = self.selected_or_all_node(*start_id).and_then(object_to_path).is_some();
                let valid_b = self.selected_or_all_node(*end_id).and_then(object_to_path).is_some();
                if !valid_a || !valid_b {
                    return false;
                }
            }
            EngineCommand::DistributeExactGap { gap_pt, .. } => {
                if !gap_pt.get().is_finite() {
                    return false;
                }
                let unlocked_count = self.selected_nodes().iter().filter(|o| !o.locked).count();
                if unlocked_count < 2 {
                    return false;
                }
            }
            EngineCommand::DivideSelection => {
                let sel = &self.selection;
                let count = self.current_target_objects().iter().filter(|o| !o.locked && sel.contains(&o.id) && object_to_path(o).is_some()).count();
                if count < 2 {
                    return false;
                }
            }
            EngineCommand::DistributeSelectionAlongPath { spine_object_id, .. } => {
                let valid_spine = self
                    .selected_or_all_node(*spine_object_id)
                    .and_then(|o| object_to_path(o).and_then(|p| p.transform(o.transform).ok()))
                    .and_then(|p| ArcLengthTable::from_path(&p).ok())
                    .is_some_and(|lut| lut.total_length() > 1e-6);
                let has_targets = self.selected_nodes().iter().any(|o| !o.locked && o.id != *spine_object_id);
                if !valid_spine || !has_targets {
                    return false;
                }
            }
            EngineCommand::RecolorArtwork { selection_only, .. } => {
                if *selection_only && self.selected_nodes().iter().all(|o| o.locked) {
                    return false;
                }
            }
            EngineCommand::HatchObjectPath { object_id, angle_deg, spacing_pt, cross_hatch } => {
                let valid = self.selected_or_all_node(*object_id).is_some_and(|o| {
                    !o.locked && object_to_path(o).is_some_and(|p| hatch_path_region(&p, angle_deg.get(), spacing_pt.get(), *cross_hatch).is_ok())
                });
                if !valid {
                    return false;
                }
            }
            EngineCommand::CreateIntermediateBlendPaths { start_id, end_id, steps, sample_count } => {
                if start_id == end_id || *steps == 0 || *sample_count < 3 {
                    return false;
                }
                let valid_a = self.selected_or_all_node(*start_id).and_then(object_to_path).is_some();
                let valid_b = self.selected_or_all_node(*end_id).and_then(object_to_path).is_some();
                if !valid_a || !valid_b {
                    return false;
                }
            }
            EngineCommand::ProjectSelectionAxonometric { .. } => {
                let has_valid = self.selected_nodes().iter().any(|o| !o.locked && object_to_path(o).is_some());
                if !has_valid {
                    return false;
                }
            }
            EngineCommand::PackSelectionInGrid { cols, gap_pt, .. } => {
                if *cols == 0 || !gap_pt.get().is_finite() {
                    return false;
                }
                let has_unlocked = self.selected_nodes().iter().any(|o| !o.locked);
                if !has_unlocked {
                    return false;
                }
            }
            EngineCommand::ExcludeSelection => {
                let sel = &self.selection;
                let count = self.current_target_objects().iter().filter(|o| !o.locked && sel.contains(&o.id) && object_to_path(o).is_some()).count();
                if count < 2 {
                    return false;
                }
            }
            EngineCommand::ResolveSelectionOverlaps { min_gap_pt, iterations } => {
                if *iterations == 0 || min_gap_pt.get() < 0.0 || !min_gap_pt.get().is_finite() {
                    return false;
                }
                let unlocked_count = self.selected_nodes().iter().filter(|o| !o.locked).count();
                if unlocked_count < 2 {
                    return false;
                }
            }
            EngineCommand::ClearEffects { object_id } => {
                let valid = self.selected_or_all_node(*object_id).is_some_and(|o| !o.locked);
                if !valid {
                    return false;
                }
            }
            EngineCommand::CloneEffectsFrom { source_id } => {
                let has_src = self.selected_or_all_node(*source_id).is_some();
                let has_target = self.selected_nodes().iter().any(|o| !o.locked && o.id != *source_id);
                if !has_src || !has_target {
                    return false;
                }
            }
            EngineCommand::SetObjectLocked { object_id, .. }
            | EngineCommand::SetObjectOpacity { object_id, .. }
            | EngineCommand::SetObjectVisible { object_id, .. }
            | EngineCommand::RenameObject { object_id, .. } => {
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
            EngineCommand::ReducePathNodes { object_id, tolerance_pt } => {
                let tol = tolerance_pt.get();
                self.for_each_node_mut(|obj| {
                    if obj.id == object_id
                        && !obj.locked
                        && let Some(base_path) = object_to_path(obj)
                        && let Ok(reduced) = simplify_path_nodes(&base_path, tol)
                    {
                        obj.kind = ObjectKind::Path(reduced);
                    }
                });
            }
            EngineCommand::SetSelectionBounds { x_pt, y_pt, width_pt, height_pt, origin_index } => {
                let sel_nodes = self.selected_nodes();
                let bounds = combined_selection_bounds(&sel_nodes);
                let Ok(pivot) = reference_point(bounds, usize::from(origin_index)) else {
                    return false;
                };
                let cur_w = bounds.width().max(1e-6);
                let cur_h = bounds.height().max(1e-6);
                let sx = width_pt.map_or(1.0, |w| (w.get() / cur_w).max(1e-6));
                let sy = height_pt.map_or(1.0, |h| (h.get() / cur_h).max(1e-6));
                let target_x = x_pt.map_or(pivot.x.get(), F64::get);
                let target_y = y_pt.map_or(pivot.y.get(), F64::get);

                let Ok(t_to_origin) = Affine2::translation(-pivot.x.get(), -pivot.y.get()) else {
                    return false;
                };
                let Ok(t_scale) = Affine2::scaling(sx, sy) else {
                    return false;
                };
                let Ok(t_to_target) = Affine2::translation(target_x, target_y) else {
                    return false;
                };
                let Ok(tf) = t_to_origin.then(t_scale).and_then(|m| m.then(t_to_target)) else {
                    return false;
                };
                self.apply_selection_transform(tf, false);
            }
            EngineCommand::AddLayer { name } => {
                let id_raw = self.document.allocate_object_id().0;
                let page_idx = self.current_page_index();
                if let Some(page) = self.document.pages.get_mut(page_idx) {
                    page.layers.push(Layer::new_normal(LayerId(id_raw), name.trim()));
                }
            }
            EngineCommand::SetLayerProperties { layer_id, visible, locked, printable, opacity } => {
                for page in &mut self.document.pages {
                    for layer in &mut page.layers {
                        if layer.id == layer_id {
                            if let Some(v) = visible {
                                layer.visible = v;
                            }
                            if let Some(lk) = locked {
                                layer.locked = lk;
                            }
                            if let Some(pr) = printable {
                                layer.printable = pr;
                            }
                            if let Some(op) = opacity
                                && let Ok(clamped) = F64::new(op.get().clamp(0.0, 1.0))
                            {
                                layer.opacity = clamped;
                            }
                        }
                    }
                }
                for layer in &mut self.document.master_layers {
                    if layer.id == layer_id {
                        if let Some(v) = visible {
                            layer.visible = v;
                        }
                        if let Some(lk) = locked {
                            layer.locked = lk;
                        }
                        if let Some(pr) = printable {
                            layer.printable = pr;
                        }
                        if let Some(op) = opacity
                            && let Ok(clamped) = F64::new(op.get().clamp(0.0, 1.0))
                        {
                            layer.opacity = clamped;
                        }
                    }
                }
            }
            EngineCommand::ClearTransformations => {
                self.for_each_selected_mut(|obj| {
                    obj.transform = Affine2::IDENTITY;
                });
            }
            EngineCommand::ToggleTextKind { object_id } => {
                self.for_each_node_mut(|obj| {
                    if obj.id != object_id || obj.locked {
                        return;
                    }
                    match &obj.kind {
                        ObjectKind::ArtisticText { text, font_family, size_pt, position } => {
                            let w = (text.chars().count().max(4) as f64) * size_pt.get() * 0.65;
                            let h = size_pt.get() * 3.0;
                            if let Ok(spec) = ParagraphTextSpec::new(
                                *position,
                                w.max(72.0),
                                h.max(36.0),
                                text.clone(),
                                font_family.clone(),
                                size_pt.get(),
                                1.2,
                                TextAlignment::Left,
                                1,
                                12.0,
                            ) {
                                obj.kind = ObjectKind::ParagraphText(Box::new(spec));
                            }
                        }
                        ObjectKind::ParagraphText(spec) => {
                            obj.kind = ObjectKind::ArtisticText {
                                text: spec.text.clone(),
                                font_family: spec.font_family.clone(),
                                size_pt: spec.font_size_pt,
                                position: spec.position,
                            };
                        }
                        _ => {}
                    }
                });
            }
            EngineCommand::FindAndReplaceText { find, replace } => {
                self.for_each_node_mut(|obj| {
                    if obj.locked {
                        return;
                    }
                    match &mut obj.kind {
                        ObjectKind::ArtisticText { text, .. } if text.contains(&find) => {
                            *text = text.replace(&find, &replace);
                        }
                        ObjectKind::ParagraphText(spec) if spec.text.contains(&find) => {
                            spec.text = spec.text.replace(&find, &replace);
                        }
                        _ => {}
                    }
                });
            }
            EngineCommand::ChangeTextCase { object_id, case_kind } => {
                self.for_each_node_mut(|obj| {
                    if obj.id != object_id || obj.locked {
                        return;
                    }
                    match &mut obj.kind {
                        ObjectKind::ArtisticText { text, .. } => {
                            *text = apply_text_case(text, case_kind);
                        }
                        ObjectKind::ParagraphText(spec) => {
                            spec.text = apply_text_case(&spec.text, case_kind);
                        }
                        _ => {}
                    }
                });
            }
            EngineCommand::InsertCurveNode { object_id, element_index } => {
                self.for_each_node_mut(|obj| {
                    if obj.id == object_id
                        && !obj.locked
                        && let Some(base_path) = object_to_path(obj)
                        && let Ok(updated) = insert_curve_node_at(&base_path, element_index)
                    {
                        obj.kind = ObjectKind::Path(updated);
                    }
                });
            }
            EngineCommand::DeleteCurveNode { object_id, element_index } => {
                self.for_each_node_mut(|obj| {
                    if obj.id == object_id
                        && !obj.locked
                        && let Some(base_path) = object_to_path(obj)
                        && let Ok(updated) = delete_curve_node_at(&base_path, element_index)
                    {
                        obj.kind = ObjectKind::Path(updated);
                    }
                });
            }
            EngineCommand::ConvertSegmentToLine { object_id, element_index } => {
                self.for_each_node_mut(|obj| {
                    if obj.id == object_id
                        && !obj.locked
                        && let Some(base_path) = object_to_path(obj)
                        && let Ok(updated) = convert_segment_to_line(&base_path, element_index)
                    {
                        obj.kind = ObjectKind::Path(updated);
                    }
                });
            }
            EngineCommand::ConvertSegmentToCurve { object_id, element_index } => {
                self.for_each_node_mut(|obj| {
                    if obj.id == object_id
                        && !obj.locked
                        && let Some(base_path) = object_to_path(obj)
                        && let Ok(updated) = convert_segment_to_curve(&base_path, element_index)
                    {
                        obj.kind = ObjectKind::Path(updated);
                    }
                });
            }
            EngineCommand::SetPageBackground { page_index, background } => {
                if let Some(page) = self.document.pages.get_mut(page_index) {
                    page.background = background;
                }
            }
            EngineCommand::SetPageBleed { page_index, bleed_pt } => {
                if let Some(page) = self.document.pages.get_mut(page_index)
                    && let Ok(clamped) = F64::new(bleed_pt.get().max(0.0))
                {
                    page.bleed = clamped;
                }
            }
            EngineCommand::SetObjectOpacity { object_id, opacity } => {
                if let Ok(clamped) = F64::new(opacity.get().clamp(0.0, 1.0)) {
                    self.for_each_node_mut(|obj| {
                        if obj.id == object_id && !obj.locked {
                            obj.opacity = clamped;
                        }
                    });
                }
            }
            EngineCommand::SetObjectVisible { object_id, visible } => {
                self.for_each_node_mut(|obj| {
                    if obj.id == object_id {
                        obj.visible = visible;
                    }
                });
            }
            EngineCommand::RenameObject { object_id, name } => {
                let normalized = name.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty());
                self.for_each_node_mut(|obj| {
                    if obj.id == object_id {
                        obj.name = normalized.clone();
                    }
                });
            }
            EngineCommand::DeleteLayer { layer_id } => {
                let mut removed_ids = Vec::new();
                for page in &mut self.document.pages {
                    if page.layers.len() > 1
                        && let Some(pos) = page.layers.iter().position(|l| l.id == layer_id)
                    {
                        let removed = page.layers.remove(pos);
                        for obj in removed.objects {
                            removed_ids.push(obj.id);
                        }
                    }
                }
                self.selection.retain(|id| !removed_ids.contains(id));
            }
            EngineCommand::MoveLayer { page_index, from_index, to_index } => {
                if let Some(page) = self.document.pages.get_mut(page_index) {
                    let layer = page.layers.remove(from_index);
                    page.layers.insert(to_index, layer);
                }
            }
            EngineCommand::InvertSelection => {
                let cur = self.selection.clone();
                self.selection =
                    self.current_target_objects().iter().filter(|o| !o.locked && o.visible && !cur.contains(&o.id)).map(|o| o.id).collect();
            }
            EngineCommand::SetOutlineProperties { width_pt, behind_fill, scale_with_object } => {
                let update_outline = |opt: &mut Option<OutlineStyle>| {
                    let mut st = opt.clone().unwrap_or_else(|| OutlineStyle::hairline(Color::BLACK));
                    if let Some(w) = width_pt
                        && let Ok(clamped) = F64::new(w.get().max(0.0))
                    {
                        st.width_pt = clamped;
                    }
                    if let Some(bf) = behind_fill {
                        st.behind_fill = bf;
                    }
                    if let Some(swo) = scale_with_object {
                        st.scale_with_object = swo;
                    }
                    *opt = Some(st);
                };
                if self.selection.is_empty() {
                    update_outline(&mut self.default_outline);
                } else {
                    self.for_each_selected_mut(|obj| {
                        if !obj.locked {
                            update_outline(&mut obj.outline);
                        }
                    });
                }
            }
            EngineCommand::SelectByKind { kind } => {
                let k = kind.trim().to_ascii_lowercase();
                self.selection = self
                    .current_target_objects()
                    .iter()
                    .filter(|o| {
                        !o.locked
                            && o.visible
                            && match k.as_str() {
                                "rect" | "rectangle" => matches!(o.kind, ObjectKind::Rect(_)),
                                "ellipse" => matches!(o.kind, ObjectKind::Ellipse(_)),
                                "polygon" | "star" => matches!(o.kind, ObjectKind::Polygon(_)),
                                "path" | "curve" => matches!(o.kind, ObjectKind::Path(_)),
                                "text" => matches!(o.kind, ObjectKind::ArtisticText { .. } | ObjectKind::ParagraphText(_)),
                                "bitmap" | "image" => matches!(o.kind, ObjectKind::Bitmap { .. }),
                                "group" => matches!(o.kind, ObjectKind::Group { .. }),
                                _ => false,
                            }
                    })
                    .map(|o| o.id)
                    .collect();
            }
            EngineCommand::SetDocumentTitle { title } => {
                self.document.title = title.trim().to_owned();
            }
            EngineCommand::TwirlObjectPath { object_id, center, radius_pt, angle_deg } => {
                let r = radius_pt.get();
                let deg = angle_deg.get();
                self.for_each_node_mut(|obj| {
                    if obj.id == object_id
                        && !obj.locked
                        && let Some(base_path) = object_to_path(obj)
                        && let Ok(updated) = twirl_path_around_point(&base_path, center, r, deg)
                    {
                        obj.kind = ObjectKind::Path(updated);
                    }
                });
            }
            EngineCommand::ClearEffects { object_id } => {
                self.for_each_node_mut(|obj| {
                    if obj.id == object_id && !obj.locked {
                        obj.effects.clear();
                    }
                });
            }
            EngineCommand::CloneEffectsFrom { source_id } => {
                let Some(src_effects) = self.selected_or_all_node(source_id).map(|o| o.effects.clone()) else {
                    return false;
                };
                self.for_each_selected_mut(|obj| {
                    if !obj.locked && obj.id != source_id {
                        obj.effects = src_effects.clone();
                    }
                });
            }
            EngineCommand::AttractRepelObjectPath { object_id, center, radius_pt, strength } => {
                let r = radius_pt.get();
                let s = strength.get();
                self.for_each_node_mut(|obj| {
                    if obj.id == object_id
                        && !obj.locked
                        && let Some(base_path) = object_to_path(obj)
                        && let Ok(updated) = radial_push_path_around_point(&base_path, center, r, s)
                    {
                        obj.kind = ObjectKind::Path(updated);
                    }
                });
            }
            EngineCommand::SmearObjectPath { object_id, origin, radius_pt, dx_pt, dy_pt } => {
                let r = radius_pt.get();
                let dx = dx_pt.get();
                let dy = dy_pt.get();
                self.for_each_node_mut(|obj| {
                    if obj.id == object_id
                        && !obj.locked
                        && let Some(base_path) = object_to_path(obj)
                        && let Ok(updated) = smear_path_from_point(&base_path, origin, r, dx, dy)
                    {
                        obj.kind = ObjectKind::Path(updated);
                    }
                });
            }
            EngineCommand::SetParagraphTextLayout { object_id, columns, gutter_pt, line_spacing, alignment } => {
                self.for_each_node_mut(|obj| {
                    if obj.id == object_id
                        && !obj.locked
                        && let ObjectKind::ParagraphText(spec) = &mut obj.kind
                    {
                        if let Some(cols) = columns {
                            spec.columns = cols.clamp(1, 8);
                        }
                        if let Some(g) = gutter_pt
                            && let Ok(clamped) = F64::new(g.get().max(0.0))
                        {
                            spec.gutter_pt = clamped;
                        }
                        if let Some(ls) = line_spacing
                            && let Ok(clamped) = F64::new(ls.get().clamp(0.5, 5.0))
                        {
                            spec.line_spacing_multiplier = clamped;
                        }
                        if let Some(align) = alignment {
                            spec.alignment = align;
                        }
                    }
                });
            }
            EngineCommand::RoughenObjectPath { object_id, tooth_height_pt, frequency } => {
                let h = tooth_height_pt.get();
                self.for_each_node_mut(|obj| {
                    if obj.id == object_id
                        && !obj.locked
                        && let Some(base_path) = object_to_path(obj)
                        && let Ok(updated) = roughen_path_segments(&base_path, h, frequency)
                    {
                        obj.kind = ObjectKind::Path(updated);
                    }
                });
            }
            EngineCommand::SetArtisticTextProperties { object_id, text, font_family, size_pt } => {
                self.for_each_node_mut(|obj| {
                    if obj.id != object_id || obj.locked {
                        return;
                    }
                    match &mut obj.kind {
                        ObjectKind::ArtisticText { text: t, font_family: ff, size_pt: sz, .. } => {
                            if let Some(new_t) = &text {
                                *t = new_t.clone();
                            }
                            if let Some(new_ff) = &font_family {
                                *ff = new_ff.clone();
                            }
                            if let Some(new_sz) = size_pt
                                && let Ok(clamped) = F64::new(new_sz.get().clamp(1.0, 2000.0))
                            {
                                *sz = clamped;
                            }
                        }
                        ObjectKind::ParagraphText(spec) => {
                            if let Some(new_t) = &text {
                                spec.text = new_t.clone();
                            }
                            if let Some(new_ff) = &font_family {
                                spec.font_family = new_ff.clone();
                            }
                            if let Some(new_sz) = size_pt
                                && let Ok(clamped) = F64::new(new_sz.get().clamp(1.0, 2000.0))
                            {
                                spec.font_size_pt = clamped;
                            }
                        }
                        _ => {}
                    }
                });
            }
            EngineCommand::SmoothObjectPath { object_id, center, radius_pt, strength, iterations } => {
                let r = radius_pt.get();
                let s = strength.get();
                self.for_each_node_mut(|obj| {
                    if obj.id == object_id
                        && !obj.locked
                        && let Some(base_path) = object_to_path(obj)
                        && let Ok(updated) = smooth_path_around_point(&base_path, center, r, s, iterations)
                    {
                        obj.kind = ObjectKind::Path(updated);
                    }
                });
            }
            EngineCommand::KnifeCutObject { object_id, line_start, line_end, gap_pt } => {
                let g = gap_pt.get();
                self.for_each_node_mut(|obj| {
                    if obj.id == object_id
                        && !obj.locked
                        && let Some(base_path) = object_to_path(obj)
                        && let Ok(updated) = knife_cut_path_by_line(&base_path, line_start, line_end, g)
                    {
                        obj.kind = ObjectKind::Path(updated);
                    }
                });
            }
            EngineCommand::CropToRect { rect } => {
                let Ok(clip_path) = RectSpec::sharp(rect).to_path() else {
                    return false;
                };
                let sel = self.selection.clone();
                let filter_sel = !sel.is_empty();
                let layer = self.target_layer_mut();
                let mut cropped_objs = Vec::with_capacity(layer.objects.len());
                for mut obj in layer.objects.drain(..) {
                    if obj.locked || !obj.visible || (filter_sel && !sel.contains(&obj.id)) {
                        cropped_objs.push(obj);
                        continue;
                    }
                    if let Some(world_path) = object_to_path(&obj).and_then(|p| p.transform(obj.transform).ok())
                        && let Ok(inter) = boolean_paths(&world_path, &clip_path, BooleanOp::Intersect)
                    {
                        if !inter.elements.is_empty() {
                            obj.kind = ObjectKind::Path(inter);
                            obj.transform = Affine2::IDENTITY;
                            cropped_objs.push(obj);
                        }
                    } else {
                        cropped_objs.push(obj);
                    }
                }
                layer.objects = cropped_objs;
                let remaining_ids: Vec<ObjectId> = layer.objects.iter().map(|o| o.id).collect();
                self.selection.retain(|id| remaining_ids.contains(id));
            }
            EngineCommand::MoveObjectToLayer { object_id, target_layer_id } => {
                let page_idx = self.current_page_index();
                if let Some(page) = self.document.pages.get_mut(page_idx) {
                    let mut extracted: Option<ObjectNode> = None;
                    for layer in &mut page.layers {
                        if let Some(pos) = layer.objects.iter().position(|o| o.id == object_id && !o.locked) {
                            extracted = Some(layer.objects.remove(pos));
                            break;
                        }
                    }
                    if let Some(node) = extracted
                        && let Some(dst) = page.layers.iter_mut().find(|l| l.id == target_layer_id)
                    {
                        dst.objects.push(node);
                    }
                }
            }
            EngineCommand::EraseObjectCircle { object_id, center, radius_pt } => {
                let r = radius_pt.get();
                self.for_each_node_mut(|obj| {
                    if obj.id == object_id
                        && !obj.locked
                        && let Some(base_path) = object_to_path(obj)
                        && let Ok(updated) = erase_path_within_circle(&base_path, center, r)
                    {
                        obj.kind = ObjectKind::Path(updated);
                    }
                });
            }
            EngineCommand::SetSnapSettings { snap_to_objects, snap_to_page } => {
                if let Some(so) = snap_to_objects {
                    self.snap_to_objects = so;
                }
                if let Some(sp) = snap_to_page {
                    self.snap_to_page = sp;
                }
            }
            EngineCommand::VirtualSegmentDelete { object_id, element_index } => {
                self.for_each_node_mut(|obj| {
                    if obj.id == object_id
                        && !obj.locked
                        && let Some(base_path) = object_to_path(obj)
                        && let Ok(updated) = delete_segment_at_index(&base_path, element_index)
                    {
                        obj.kind = ObjectKind::Path(updated);
                    }
                });
            }
            EngineCommand::SmartFillAtPoint { point, fill, outline } => {
                let px = point.x.get();
                let py = point.y.get();
                let hits: Vec<Path> = self
                    .current_target_objects()
                    .iter()
                    .filter(|o| !o.locked && o.visible)
                    .filter_map(|o| object_to_path(o).and_then(|p| p.transform(o.transform).ok()))
                    .filter(|wp| wp.bounds().is_some_and(|b| px >= b.x0.get() && px <= b.x1.get() && py >= b.y0.get() && py <= b.y1.get()))
                    .collect();
                if let Some(first) = hits.first() {
                    let mut region = first.clone();
                    for other in hits.iter().skip(1) {
                        if let Ok(inter) = boolean_paths(&region, other, BooleanOp::Intersect)
                            && !inter.elements.is_empty()
                        {
                            region = inter;
                        }
                    }
                    let id = self.document.allocate_object_id();
                    let node = ObjectNode::new(id, Some("Smart Fill".to_owned()), ObjectKind::Path(region), Some(fill), outline);
                    self.target_layer_mut().objects.push(node);
                    self.selection = vec![id];
                }
            }
            EngineCommand::OffsetObjectPath { object_id, distance_pt, copy } => {
                let d = distance_pt.get();
                if copy {
                    if let Some(src) = self.selected_or_all_node(object_id).cloned()
                        && let Some(base_path) = object_to_path(&src)
                        && let Ok(updated) = offset_open_or_closed_path(&base_path, d)
                    {
                        let mut dup = src;
                        reallocate_object_ids_recursive(&mut dup, &mut self.document);
                        dup.kind = ObjectKind::Path(updated);
                        let new_id = dup.id;
                        self.target_layer_mut().objects.push(dup);
                        self.selection = vec![new_id];
                    }
                } else {
                    self.for_each_node_mut(|obj| {
                        if obj.id == object_id
                            && !obj.locked
                            && let Some(base_path) = object_to_path(obj)
                            && let Ok(updated) = offset_open_or_closed_path(&base_path, d)
                        {
                            obj.kind = ObjectKind::Path(updated);
                        }
                    });
                }
            }
            EngineCommand::SetPathFillRule { object_id, fill_rule } => {
                self.for_each_node_mut(|obj| {
                    if obj.id == object_id
                        && !obj.locked
                        && let Some(mut base_path) = object_to_path(obj)
                    {
                        base_path.fill_rule = fill_rule;
                        obj.kind = ObjectKind::Path(base_path);
                    }
                });
            }
            EngineCommand::SwapObjectPositions { first_id, second_id } => {
                let c1 = self.selected_or_all_node(first_id).and_then(|o| combined_selection_bounds(&[o]).center().ok());
                let c2 = self.selected_or_all_node(second_id).and_then(|o| combined_selection_bounds(&[o]).center().ok());
                if let (Some(p1), Some(p2)) = (c1, c2) {
                    let dx = p2.x.get() - p1.x.get();
                    let dy = p2.y.get() - p1.y.get();
                    let tf1 = Affine2::translation(dx, dy).unwrap_or(Affine2::IDENTITY);
                    let tf2 = Affine2::translation(-dx, -dy).unwrap_or(Affine2::IDENTITY);
                    self.for_each_node_mut(|obj| {
                        if !obj.locked {
                            if obj.id == first_id {
                                if let Ok(composed) = obj.transform.then(tf1) {
                                    obj.transform = composed;
                                }
                            } else if obj.id == second_id
                                && let Ok(composed) = obj.transform.then(tf2)
                            {
                                obj.transform = composed;
                            }
                        }
                    });
                }
            }
            EngineCommand::CenterSelectionToPage { horizontal, vertical } => {
                let sel_nodes: Vec<&ObjectNode> = self.selected_nodes().into_iter().filter(|o| !o.locked).collect();
                if let Ok(sel_center) = combined_selection_bounds(&sel_nodes).center() {
                    let page_size = self.current_page().size;
                    let dx = if horizontal { 0.5 * page_size.width.get() - sel_center.x.get() } else { 0.0 };
                    let dy = if vertical { 0.5 * page_size.height.get() - sel_center.y.get() } else { 0.0 };
                    if let Ok(tf) = Affine2::translation(dx, dy) {
                        self.for_each_selected_mut(|obj| {
                            if !obj.locked
                                && let Ok(composed) = obj.transform.then(tf)
                            {
                                obj.transform = composed;
                            }
                        });
                    }
                }
            }
            EngineCommand::SmoothCurveNode { object_id, element_index, tension } => {
                let t = tension.get();
                self.for_each_node_mut(|obj| {
                    if obj.id == object_id
                        && !obj.locked
                        && let Some(base_path) = object_to_path(obj)
                        && let Ok(updated) = smooth_curve_node_at(&base_path, element_index, t)
                    {
                        obj.kind = ObjectKind::Path(updated);
                    }
                });
            }
            EngineCommand::ArrayDuplicateSelection { rows, cols, spacing_x_pt, spacing_y_pt } => {
                let r_count = rows.clamp(1, 20);
                let c_count = cols.clamp(1, 20);
                let sx = spacing_x_pt.get();
                let sy = spacing_y_pt.get();
                let sel = self.selection.clone();
                let templates: Vec<ObjectNode> = self.current_target_objects().iter().filter(|o| !o.locked && sel.contains(&o.id)).cloned().collect();
                let mut new_selection = Vec::new();
                for r in 0..r_count {
                    for c in 0..c_count {
                        if r == 0 && c == 0 {
                            continue;
                        }
                        let dx = f64::from(c) * sx;
                        let dy = f64::from(r) * sy;
                        let Ok(tf) = Affine2::translation(dx, dy) else {
                            continue;
                        };
                        for mut dup in templates.clone() {
                            reallocate_object_ids_recursive(&mut dup, &mut self.document);
                            if let Ok(composed) = dup.transform.then(tf) {
                                dup.transform = composed;
                            }
                            new_selection.push(dup.id);
                            self.target_layer_mut().objects.push(dup);
                        }
                    }
                }
                if !new_selection.is_empty() {
                    self.selection = new_selection;
                }
            }
            EngineCommand::AddGuideline { horizontal, position_pt } => {
                let page_idx = self.current_page_index();
                let page_size = self.document.pages[page_idx].size;
                let pos = position_pt.get();
                let (p0, p1, label) = if horizontal {
                    (Point::new(0.0, pos), Point::new(page_size.width.get(), pos), format!("Horizontal Guide ({pos:.1} pt)"))
                } else {
                    (Point::new(pos, 0.0), Point::new(pos, page_size.height.get()), format!("Vertical Guide ({pos:.1} pt)"))
                };
                if let (Ok(a), Ok(b)) = (p0, p1)
                    && let Ok(path) = Path::from_elements(vec![PathElement::MoveTo(a), PathElement::LineTo(b)], FillRule::NonZero)
                {
                    let obj_id = self.document.allocate_object_id();
                    let mut outline = OutlineStyle::hairline(Color::rgb8(0, 85, 229));
                    if let (Ok(d1), Ok(d2)) = (F64::new(4.0), F64::new(3.0)) {
                        outline.dash_pattern = vec![d1, d2];
                    }
                    let node = ObjectNode::new(obj_id, Some(label), ObjectKind::Path(path), None, Some(outline));
                    if let Some(guide_layer) = self.document.pages[page_idx].layers.iter_mut().find(|l| l.role == LayerRole::Guides) {
                        guide_layer.objects.push(node);
                    } else {
                        let layer_id = LayerId(self.document.allocate_object_id().0);
                        let page = &mut self.document.pages[page_idx];
                        page.layers.push(Layer {
                            id: layer_id,
                            name: "Guides".to_owned(),
                            role: LayerRole::Guides,
                            visible: true,
                            printable: false,
                            locked: false,
                            master_in_front: true,
                            opacity: F64::ONE,
                            objects: vec![node],
                            residual: Residual::default(),
                            extra: BTreeMap::new(),
                        });
                    }
                }
            }
            EngineCommand::ClearGuidelines => {
                let page_idx = self.current_page_index();
                let mut removed_ids = Vec::new();
                for layer in &mut self.document.pages[page_idx].layers {
                    if layer.role == LayerRole::Guides {
                        for obj in layer.objects.drain(..) {
                            removed_ids.push(obj.id);
                        }
                    }
                }
                self.selection.retain(|id| !removed_ids.contains(id));
            }
            EngineCommand::CircularDuplicateSelection { count, center, total_angle_deg, rotate_objects } => {
                let n = count.clamp(1, 72);
                let step_deg = total_angle_deg.get() / f64::from(n);
                let cx = center.x.get();
                let cy = center.y.get();
                let sel = self.selection.clone();
                let templates: Vec<ObjectNode> = self.current_target_objects().iter().filter(|o| !o.locked && sel.contains(&o.id)).cloned().collect();
                let mut new_selection = Vec::new();
                for k in 1..=n {
                    let angle_rad = (f64::from(k) * step_deg).to_radians();
                    for mut dup in templates.clone() {
                        reallocate_object_ids_recursive(&mut dup, &mut self.document);
                        if rotate_objects {
                            if let Ok(t1) = Affine2::translation(-cx, -cy)
                                && let Ok(rot) = Affine2::rotation_rad(angle_rad)
                                && let Ok(t2) = Affine2::translation(cx, cy)
                                && let Ok(around_center) = t1.then(rot).and_then(|m| m.then(t2))
                                && let Ok(composed) = dup.transform.then(around_center)
                            {
                                dup.transform = composed;
                            }
                        } else if let Ok(obj_center) = combined_selection_bounds(&[&dup]).center() {
                            let vx = obj_center.x.get() - cx;
                            let vy = obj_center.y.get() - cy;
                            let cos_a = angle_rad.cos();
                            let sin_a = angle_rad.sin();
                            let nx = cx + vx * cos_a - vy * sin_a;
                            let ny = cy + vx * sin_a + vy * cos_a;
                            if let Ok(tf) = Affine2::translation(nx - obj_center.x.get(), ny - obj_center.y.get())
                                && let Ok(composed) = dup.transform.then(tf)
                            {
                                dup.transform = composed;
                            }
                        }
                        new_selection.push(dup.id);
                        self.target_layer_mut().objects.push(dup);
                    }
                }
                if !new_selection.is_empty() {
                    self.selection = new_selection;
                }
            }
            EngineCommand::FitPageToSelection { margin_pt } => {
                let m = margin_pt.get().max(0.0);
                let bounds = if self.selection.is_empty() {
                    let refs: Vec<&ObjectNode> = self.current_target_objects().iter().filter(|o| !o.locked && o.visible).collect();
                    combined_selection_bounds(&refs)
                } else {
                    let refs: Vec<&ObjectNode> = self.selected_nodes().into_iter().filter(|o| !o.locked).collect();
                    combined_selection_bounds(&refs)
                };
                let new_w = (bounds.width() + 2.0 * m).max(1.0);
                let new_h = (bounds.height() + 2.0 * m).max(1.0);
                let dx = m - bounds.x0.get();
                let dy = m - bounds.y0.get();
                let page_idx = self.current_page_index();
                if let Ok(sz) = Size::new(new_w, new_h)
                    && let Ok(tf) = Affine2::translation(dx, dy)
                {
                    self.document.pages[page_idx].size = sz;
                    for layer in &mut self.document.pages[page_idx].layers {
                        if !layer.locked {
                            for obj in &mut layer.objects {
                                if !obj.locked
                                    && let Ok(composed) = obj.transform.then(tf)
                                {
                                    obj.transform = composed;
                                }
                            }
                        }
                    }
                }
            }
            EngineCommand::SetOutlineCapAndJoin { cap, join, miter_limit } => {
                let update_outline = |opt: &mut Option<OutlineStyle>| {
                    let mut st = opt.clone().unwrap_or_else(|| OutlineStyle::hairline(Color::BLACK));
                    if let Some(c) = cap {
                        st.cap = c;
                    }
                    if let Some(j) = join {
                        st.join = j;
                    }
                    if let Some(ml) = miter_limit
                        && let Ok(clamped) = F64::new(ml.get().clamp(1.0, 100.0))
                    {
                        st.miter_limit = clamped;
                    }
                    *opt = Some(st);
                };
                if self.selection.is_empty() {
                    update_outline(&mut self.default_outline);
                } else {
                    self.for_each_selected_mut(|obj| {
                        if !obj.locked {
                            update_outline(&mut obj.outline);
                        }
                    });
                }
            }
            EngineCommand::AlignCurveNodes { object_id, element_indices, horizontal, vertical } => {
                self.for_each_node_mut(|obj| {
                    if obj.id != object_id || obj.locked {
                        return;
                    }
                    let Some(mut path) = object_to_path(obj) else {
                        return;
                    };
                    let elem_end = |el: &PathElement| -> Option<Point> {
                        match *el {
                            PathElement::MoveTo(p)
                            | PathElement::LineTo(p)
                            | PathElement::QuadTo { end: p, .. }
                            | PathElement::CubicTo { end: p, .. } => Some(p),
                            PathElement::Close => None,
                        }
                    };
                    let Some(ref_pt) = element_indices.first().and_then(|&i| path.elements.get(i)).and_then(elem_end) else {
                        return;
                    };
                    for &idx in element_indices.iter().skip(1) {
                        let Some(cur_pt) = path.elements.get(idx).and_then(elem_end) else {
                            continue;
                        };
                        let nx = if vertical { ref_pt.x.get() } else { cur_pt.x.get() };
                        let ny = if horizontal { ref_pt.y.get() } else { cur_pt.y.get() };
                        let dx = nx - cur_pt.x.get();
                        let dy = ny - cur_pt.y.get();
                        let Ok(new_end) = Point::new(nx, ny) else {
                            continue;
                        };
                        match &mut path.elements[idx] {
                            PathElement::MoveTo(p) | PathElement::LineTo(p) => *p = new_end,
                            PathElement::QuadTo { ctrl, end } => {
                                if let Ok(nc) = Point::new(ctrl.x.get() + dx, ctrl.y.get() + dy) {
                                    *ctrl = nc;
                                }
                                *end = new_end;
                            }
                            PathElement::CubicTo { c2, end, .. } => {
                                if let Ok(nc2) = Point::new(c2.x.get() + dx, c2.y.get() + dy) {
                                    *c2 = nc2;
                                }
                                *end = new_end;
                            }
                            PathElement::Close => {}
                        }
                        if idx + 1 < path.elements.len()
                            && let PathElement::CubicTo { c1, .. } = &mut path.elements[idx + 1]
                            && let Ok(nc1) = Point::new(c1.x.get() + dx, c1.y.get() + dy)
                        {
                            *c1 = nc1;
                        }
                    }
                    obj.kind = ObjectKind::Path(path);
                });
            }
            EngineCommand::ShiftSelectionColors { hue_delta_deg, saturation_delta, brightness_delta } => {
                let dh = hue_delta_deg.get();
                let ds = saturation_delta.get();
                let db = brightness_delta.get();
                let shift_color = |c: &Color| -> Color {
                    let [r, g, b] = c.to_srgb8();
                    let [h, s, v] = coralcraft_color::srgb8_to_hsb(r, g, b);
                    let [nr, ng, nb] = coralcraft_color::hsb_to_srgb8((h + dh).rem_euclid(360.0), (s + ds).clamp(0.0, 1.0), (v + db).clamp(0.0, 1.0));
                    Color::rgb8(nr, ng, nb)
                };
                self.for_each_selected_mut(|obj| {
                    if obj.locked {
                        return;
                    }
                    if let Some(fill) = &mut obj.fill {
                        match fill {
                            FillStyle::Uniform { color } => *color = shift_color(color),
                            FillStyle::Fountain { stops, .. } => {
                                for stop in stops {
                                    stop.color = shift_color(&stop.color);
                                }
                            }
                            FillStyle::TwoColorPattern { fg_color, bg_color, .. } => {
                                *fg_color = shift_color(fg_color);
                                *bg_color = shift_color(bg_color);
                            }
                            FillStyle::Unknown { .. } => {}
                        }
                    }
                    if let Some(outline) = &mut obj.outline {
                        outline.color = shift_color(&outline.color);
                    }
                });
            }
            EngineCommand::SubdividePathSegments { object_id } => {
                self.for_each_node_mut(|obj| {
                    if obj.id == object_id
                        && !obj.locked
                        && let Some(base_path) = object_to_path(obj)
                        && let Ok(updated) = subdivide_all_path_segments(&base_path)
                    {
                        obj.kind = ObjectKind::Path(updated);
                    }
                });
            }
            EngineCommand::ConvertSelectionColors { invert, grayscale } => {
                let convert_color = |c: &Color| -> Color {
                    let [mut r, mut g, mut b] = c.to_srgb8();
                    if invert {
                        r = 255 - r;
                        g = 255 - g;
                        b = 255 - b;
                    }
                    if grayscale {
                        let lum = (0.2126 * f64::from(r) + 0.7152 * f64::from(g) + 0.0722 * f64::from(b)).round().clamp(0.0, 255.0) as u8;
                        r = lum;
                        g = lum;
                        b = lum;
                    }
                    Color::rgb8(r, g, b)
                };
                self.for_each_selected_mut(|obj| {
                    if obj.locked {
                        return;
                    }
                    if let Some(fill) = &mut obj.fill {
                        match fill {
                            FillStyle::Uniform { color } => *color = convert_color(color),
                            FillStyle::Fountain { stops, .. } => {
                                for stop in stops {
                                    stop.color = convert_color(&stop.color);
                                }
                            }
                            FillStyle::TwoColorPattern { fg_color, bg_color, .. } => {
                                *fg_color = convert_color(fg_color);
                                *bg_color = convert_color(bg_color);
                            }
                            FillStyle::Unknown { .. } => {}
                        }
                    }
                    if let Some(outline) = &mut obj.outline {
                        outline.color = convert_color(&outline.color);
                    }
                });
            }
            EngineCommand::TransformCurveNodes { object_id, element_indices, scale_x, scale_y, rotate_deg } => {
                let sx = scale_x.get();
                let sy = scale_y.get();
                let rot = rotate_deg.get();
                self.for_each_node_mut(|obj| {
                    if obj.id == object_id
                        && !obj.locked
                        && let Some(base_path) = object_to_path(obj)
                        && let Ok(updated) = transform_curve_nodes(&base_path, &element_indices, sx, sy, rot)
                    {
                        obj.kind = ObjectKind::Path(updated);
                    }
                });
            }
            EngineCommand::SelectByColor { color, match_fill, match_outline, tolerance_u8 } => {
                let [tr, tg, tb] = color.to_srgb8();
                let tol = i32::from(tolerance_u8);
                let matches_c = |c: &Color| -> bool {
                    let [r, g, b] = c.to_srgb8();
                    (i32::from(r) - i32::from(tr)).abs() <= tol
                        && (i32::from(g) - i32::from(tg)).abs() <= tol
                        && (i32::from(b) - i32::from(tb)).abs() <= tol
                };
                self.selection = self
                    .current_target_objects()
                    .iter()
                    .filter(|o| {
                        if o.locked || !o.visible {
                            return false;
                        }
                        let fill_ok = match_fill
                            && o.fill.as_ref().is_some_and(|f| match f {
                                FillStyle::Uniform { color: fc } => matches_c(fc),
                                FillStyle::Fountain { stops, .. } => stops.iter().any(|s| matches_c(&s.color)),
                                FillStyle::TwoColorPattern { fg_color, bg_color, .. } => matches_c(fg_color) || matches_c(bg_color),
                                FillStyle::Unknown { .. } => false,
                            });
                        let outline_ok = match_outline && o.outline.as_ref().is_some_and(|ol| matches_c(&ol.color));
                        fill_ok || outline_ok
                    })
                    .map(|o| o.id)
                    .collect();
            }
            EngineCommand::BreakPathAtNode { object_id, element_index } => {
                self.for_each_node_mut(|obj| {
                    if obj.id == object_id
                        && !obj.locked
                        && let Some(base_path) = object_to_path(obj)
                        && let Ok(updated) = break_path_at_node(&base_path, element_index)
                    {
                        obj.kind = ObjectKind::Path(updated);
                    }
                });
            }
            EngineCommand::EqualizeSelectionSize { match_width, match_height } => {
                if let Some(&anchor_id) = self.selection.last()
                    && let Some(anchor_node) = self.selected_or_all_node(anchor_id)
                {
                    let anchor_bbox = combined_selection_bounds(&[anchor_node]);
                    let target_w = anchor_bbox.width();
                    let target_h = anchor_bbox.height();
                    if target_w > 1e-6 && target_h > 1e-6 {
                        self.for_each_selected_mut(|obj| {
                            if obj.locked || obj.id == anchor_id {
                                return;
                            }
                            let bbox = combined_selection_bounds(&[obj]);
                            let cur_w = bbox.width();
                            let cur_h = bbox.height();
                            if cur_w <= 1e-6 || cur_h <= 1e-6 {
                                return;
                            }
                            let sx = if match_width { target_w / cur_w } else { 1.0 };
                            let sy = if match_height { target_h / cur_h } else { 1.0 };
                            let cx = 0.5 * (bbox.x0.get() + bbox.x1.get());
                            let cy = 0.5 * (bbox.y0.get() + bbox.y1.get());
                            if let Ok(t1) = Affine2::translation(-cx, -cy)
                                && let Ok(sc) = Affine2::scaling(sx, sy)
                                && let Ok(t2) = Affine2::translation(cx, cy)
                                && let Ok(step1) = t1.then(sc)
                                && let Ok(tf) = step1.then(t2)
                                && let Ok(composed) = obj.transform.then(tf)
                            {
                                obj.transform = composed;
                            }
                        });
                    }
                }
            }
            EngineCommand::MorphBetweenObjects { start_id, end_id, t, sample_count } => {
                let src_a = self.selected_or_all_node(start_id).cloned();
                let src_b = self.selected_or_all_node(end_id).cloned();
                if let (Some(a), Some(b)) = (src_a, src_b)
                    && let Some(pa) = object_to_path(&a).and_then(|p| p.transform(a.transform).ok())
                    && let Some(pb) = object_to_path(&b).and_then(|p| p.transform(b.transform).ok())
                    && let Ok(morphed) = morph_paths_linear(&pa, &pb, t.get(), sample_count)
                {
                    let new_id = self.document.allocate_object_id();
                    let node =
                        ObjectNode::new(new_id, Some("Morphed Curve".to_owned()), ObjectKind::Path(morphed), a.fill.clone(), a.outline.clone());
                    self.target_layer_mut().objects.push(node);
                    self.selection = vec![new_id];
                }
            }
            EngineCommand::DistributeExactGap { horizontal, gap_pt } => {
                let gap = gap_pt.get();
                let mut items: Vec<(ObjectId, Rect)> =
                    self.selected_nodes().into_iter().filter(|o| !o.locked).map(|o| (o.id, combined_selection_bounds(&[o]))).collect();
                if items.len() >= 2 {
                    if horizontal {
                        items.sort_by_key(|a| a.1.x0);
                    } else {
                        items.sort_by_key(|a| a.1.y0);
                    }
                    let mut deltas = BTreeMap::new();
                    if horizontal {
                        let mut cur_x = items[0].1.x1.get() + gap;
                        for (id, b) in items.iter().skip(1) {
                            deltas.insert(*id, (cur_x - b.x0.get(), 0.0));
                            cur_x += b.width() + gap;
                        }
                    } else {
                        let mut cur_y = items[0].1.y1.get() + gap;
                        for (id, b) in items.iter().skip(1) {
                            deltas.insert(*id, (0.0, cur_y - b.y0.get()));
                            cur_y += b.height() + gap;
                        }
                    }
                    self.for_each_selected_mut(|obj| {
                        if !obj.locked
                            && let Some(&(dx, dy)) = deltas.get(&obj.id)
                            && let Ok(tf) = Affine2::translation(dx, dy)
                            && let Ok(composed) = obj.transform.then(tf)
                        {
                            obj.transform = composed;
                        }
                    });
                }
            }
            EngineCommand::DivideSelection => {
                let sel = self.selection.clone();
                let candidates: Vec<ObjectNode> =
                    self.current_target_objects().iter().filter(|o| !o.locked && sel.contains(&o.id)).cloned().collect();
                if candidates.len() >= 2
                    && let Some(pa) = object_to_path(&candidates[0]).and_then(|p| p.transform(candidates[0].transform).ok())
                    && let Some(pb) = object_to_path(&candidates[1]).and_then(|p| p.transform(candidates[1].transform).ok())
                    && let Ok(regions) = divide_overlapping_paths(&pa, &pb)
                {
                    let id_a = candidates[0].id;
                    let id_b = candidates[1].id;
                    let mut new_ids = Vec::with_capacity(regions.len());
                    let mut new_nodes = Vec::with_capacity(regions.len());
                    for (idx, reg) in regions.into_iter().enumerate() {
                        let new_id = self.document.allocate_object_id();
                        let src = if idx == 2 { &candidates[1] } else { &candidates[0] };
                        let node = ObjectNode::new(
                            new_id,
                            Some(format!("Divided Region {}", idx + 1)),
                            ObjectKind::Path(reg),
                            src.fill.clone(),
                            src.outline.clone(),
                        );
                        new_ids.push(new_id);
                        new_nodes.push(node);
                    }
                    let layer = self.target_layer_mut();
                    layer.objects.retain(|o| o.id != id_a && o.id != id_b);
                    layer.objects.extend(new_nodes);
                    self.selection = new_ids;
                }
            }
            EngineCommand::DistributeSelectionAlongPath { spine_object_id, rotate_to_tangent } => {
                if let Some(spine_obj) = self.selected_or_all_node(spine_object_id).cloned()
                    && let Some(spine_path) = object_to_path(&spine_obj).and_then(|p| p.transform(spine_obj.transform).ok())
                    && let Ok(lut) = ArcLengthTable::from_path(&spine_path)
                {
                    let targets: Vec<(ObjectId, Point)> = self
                        .selected_nodes()
                        .into_iter()
                        .filter(|o| !o.locked && o.id != spine_object_id)
                        .filter_map(|o| combined_selection_bounds(&[o]).center().ok().map(|c| (o.id, c)))
                        .collect();
                    let n = targets.len();
                    let mut transforms = BTreeMap::new();
                    for (idx, (id, cur_c)) in targets.into_iter().enumerate() {
                        let u = if n <= 1 { 0.5 } else { (idx as f64) / ((n - 1) as f64) };
                        if let Some(sample) = lut.sample_normalized(u) {
                            let dx = sample.point.x.get() - cur_c.x.get();
                            let dy = sample.point.y.get() - cur_c.y.get();
                            if rotate_to_tangent {
                                if let Ok(t1) = Affine2::translation(-cur_c.x.get(), -cur_c.y.get())
                                    && let Ok(rot) = Affine2::rotation_rad(sample.angle_deg.to_radians())
                                    && let Ok(t2) = Affine2::translation(sample.point.x.get(), sample.point.y.get())
                                    && let Ok(s1) = t1.then(rot)
                                    && let Ok(tf) = s1.then(t2)
                                {
                                    transforms.insert(id, tf);
                                }
                            } else if let Ok(tf) = Affine2::translation(dx, dy) {
                                transforms.insert(id, tf);
                            }
                        }
                    }
                    self.for_each_selected_mut(|obj| {
                        if !obj.locked
                            && let Some(&tf) = transforms.get(&obj.id)
                            && let Ok(composed) = obj.transform.then(tf)
                        {
                            obj.transform = composed;
                        }
                    });
                }
            }
            EngineCommand::RecolorArtwork { from_color, to_color, tolerance_u8, selection_only } => {
                let [tr, tg, tb] = from_color.to_srgb8();
                let tol = i32::from(tolerance_u8);
                let matches_c = |c: &Color| -> bool {
                    let [r, g, b] = c.to_srgb8();
                    (i32::from(r) - i32::from(tr)).abs() <= tol
                        && (i32::from(g) - i32::from(tg)).abs() <= tol
                        && (i32::from(b) - i32::from(tb)).abs() <= tol
                };
                let recolor_node = |obj: &mut ObjectNode| {
                    if obj.locked {
                        return;
                    }
                    if let Some(fill) = &mut obj.fill {
                        match fill {
                            FillStyle::Uniform { color } => {
                                if matches_c(color) {
                                    *color = to_color.clone();
                                }
                            }
                            FillStyle::Fountain { stops, .. } => {
                                for s in stops {
                                    if matches_c(&s.color) {
                                        s.color = to_color.clone();
                                    }
                                }
                            }
                            FillStyle::TwoColorPattern { fg_color, bg_color, .. } => {
                                if matches_c(fg_color) {
                                    *fg_color = to_color.clone();
                                }
                                if matches_c(bg_color) {
                                    *bg_color = to_color.clone();
                                }
                            }
                            FillStyle::Unknown { .. } => {}
                        }
                    }
                    if let Some(ol) = &mut obj.outline
                        && matches_c(&ol.color)
                    {
                        ol.color = to_color.clone();
                    }
                };
                if selection_only {
                    self.for_each_selected_mut(recolor_node);
                } else {
                    self.for_each_node_mut(recolor_node);
                }
            }
            EngineCommand::HatchObjectPath { object_id, angle_deg, spacing_pt, cross_hatch } => {
                let ang = angle_deg.get();
                let sp = spacing_pt.get();
                self.for_each_node_mut(|obj| {
                    if obj.id == object_id
                        && !obj.locked
                        && let Some(base_path) = object_to_path(obj)
                        && let Ok(hatched) = hatch_path_region(&base_path, ang, sp, cross_hatch)
                    {
                        obj.kind = ObjectKind::Path(hatched);
                        if obj.outline.is_none() {
                            obj.outline = Some(OutlineStyle::hairline(Color::BLACK));
                        }
                    }
                });
            }
            EngineCommand::CreateIntermediateBlendPaths { start_id, end_id, steps, sample_count } => {
                let n_steps = steps.clamp(1, 32);
                let src_a = self.selected_or_all_node(start_id).cloned();
                let src_b = self.selected_or_all_node(end_id).cloned();
                if let (Some(a), Some(b)) = (src_a, src_b)
                    && let Some(pa) = object_to_path(&a).and_then(|p| p.transform(a.transform).ok())
                    && let Some(pb) = object_to_path(&b).and_then(|p| p.transform(b.transform).ok())
                {
                    let ca = a.fill.as_ref().and_then(|f| match f {
                        FillStyle::Uniform { color } => Some(color.to_srgb8()),
                        _ => None,
                    });
                    let cb = b.fill.as_ref().and_then(|f| match f {
                        FillStyle::Uniform { color } => Some(color.to_srgb8()),
                        _ => None,
                    });
                    let mut new_ids = Vec::with_capacity(n_steps as usize);
                    for k in 1..=n_steps {
                        let t = f64::from(k) / f64::from(n_steps + 1);
                        if let Ok(morphed) = morph_paths_linear(&pa, &pb, t, sample_count) {
                            let new_id = self.document.allocate_object_id();
                            let fill = if let (Some([r0, g0, b0]), Some([r1, g1, b1])) = (ca, cb) {
                                let r = ((1.0 - t) * f64::from(r0) + t * f64::from(r1)).round().clamp(0.0, 255.0) as u8;
                                let g = ((1.0 - t) * f64::from(g0) + t * f64::from(g1)).round().clamp(0.0, 255.0) as u8;
                                let bl = ((1.0 - t) * f64::from(b0) + t * f64::from(b1)).round().clamp(0.0, 255.0) as u8;
                                Some(FillStyle::uniform(Color::rgb8(r, g, bl)))
                            } else {
                                a.fill.clone()
                            };
                            let node = ObjectNode::new(new_id, Some(format!("Blend Step {k}")), ObjectKind::Path(morphed), fill, a.outline.clone());
                            new_ids.push(new_id);
                            self.target_layer_mut().objects.push(node);
                        }
                    }
                    if !new_ids.is_empty() {
                        self.selection = new_ids;
                    }
                }
            }
            EngineCommand::ProjectSelectionAxonometric { plane } => {
                self.for_each_selected_mut(|obj| {
                    if !obj.locked
                        && let Some(world_path) = object_to_path(obj).and_then(|p| p.transform(obj.transform).ok())
                        && let Ok(projected) = project_path_axonometric(&world_path, plane)
                    {
                        obj.kind = ObjectKind::Path(projected);
                        obj.transform = Affine2::IDENTITY;
                    }
                });
            }
            EngineCommand::PackSelectionInGrid { origin, cols, gap_pt } => {
                let c_count = cols.clamp(1, 32) as usize;
                let gap = gap_pt.get();
                let items: Vec<(ObjectId, Rect)> =
                    self.selected_nodes().into_iter().filter(|o| !o.locked).map(|o| (o.id, combined_selection_bounds(&[o]))).collect();
                let mut deltas = BTreeMap::new();
                let mut cur_x = origin.x.get();
                let mut cur_y = origin.y.get();
                let mut row_h = 0.0_f64;
                for (idx, (id, b)) in items.into_iter().enumerate() {
                    if idx > 0 && idx % c_count == 0 {
                        cur_x = origin.x.get();
                        cur_y += row_h + gap;
                        row_h = 0.0;
                    }
                    deltas.insert(id, (cur_x - b.x0.get(), cur_y - b.y0.get()));
                    cur_x += b.width() + gap;
                    row_h = row_h.max(b.height());
                }
                self.for_each_selected_mut(|obj| {
                    if !obj.locked
                        && let Some(&(dx, dy)) = deltas.get(&obj.id)
                        && let Ok(tf) = Affine2::translation(dx, dy)
                        && let Ok(composed) = obj.transform.then(tf)
                    {
                        obj.transform = composed;
                    }
                });
            }
            EngineCommand::ExcludeSelection => {
                let sel = self.selection.clone();
                let candidates: Vec<ObjectNode> =
                    self.current_target_objects().iter().filter(|o| !o.locked && sel.contains(&o.id)).cloned().collect();
                if candidates.len() >= 2
                    && let Some(first_path) = object_to_path(&candidates[0]).and_then(|p| p.transform(candidates[0].transform).ok())
                {
                    let mut acc = first_path;
                    for cand in candidates.iter().skip(1) {
                        if let Some(wp) = object_to_path(cand).and_then(|p| p.transform(cand.transform).ok())
                            && let Ok(excluded) = boolean_paths(&acc, &wp, BooleanOp::Exclude)
                        {
                            acc = excluded;
                        }
                    }
                    let anchor = candidates.last().unwrap_or(&candidates[0]);
                    let new_id = self.document.allocate_object_id();
                    let new_node =
                        ObjectNode::new(new_id, Some("Excluded Path".to_owned()), ObjectKind::Path(acc), anchor.fill.clone(), anchor.outline.clone());
                    let remove_ids: Vec<ObjectId> = candidates.iter().map(|c| c.id).collect();
                    let layer = self.target_layer_mut();
                    layer.objects.retain(|o| !remove_ids.contains(&o.id));
                    layer.objects.push(new_node);
                    self.selection = vec![new_id];
                }
            }
            EngineCommand::ResolveSelectionOverlaps { min_gap_pt, iterations } => {
                let gap = min_gap_pt.get().max(0.0);
                let iters = iterations.clamp(1, 200);
                let mut boxes: Vec<(ObjectId, f64, f64, f64, f64, f64, f64)> = self
                    .selected_nodes()
                    .into_iter()
                    .filter(|o| !o.locked)
                    .map(|o| {
                        let b = combined_selection_bounds(&[o]);
                        (o.id, b.x0.get(), b.y0.get(), b.width(), b.height(), b.x0.get(), b.y0.get())
                    })
                    .collect();
                let n = boxes.len();
                for _ in 0..iters {
                    let mut any_moved = false;
                    for i in 0..n {
                        for j in (i + 1)..n {
                            let cxi = boxes[i].1 + 0.5 * boxes[i].3;
                            let cyi = boxes[i].2 + 0.5 * boxes[i].4;
                            let cxj = boxes[j].1 + 0.5 * boxes[j].3;
                            let cyj = boxes[j].2 + 0.5 * boxes[j].4;
                            let req_dx = 0.5 * (boxes[i].3 + boxes[j].3) + gap;
                            let req_dy = 0.5 * (boxes[i].4 + boxes[j].4) + gap;
                            let dx = cxj - cxi;
                            let dy = cyj - cyi;
                            let pen_x = req_dx - dx.abs();
                            let pen_y = req_dy - dy.abs();
                            if pen_x > 1e-6 && pen_y > 1e-6 {
                                any_moved = true;
                                if pen_x <= pen_y {
                                    let dir = if dx.abs() > 1e-6 { dx.signum() } else { 1.0 };
                                    let push = 0.5 * pen_x * dir;
                                    boxes[i].1 -= push;
                                    boxes[j].1 += push;
                                } else {
                                    let dir = if dy.abs() > 1e-6 { dy.signum() } else { 1.0 };
                                    let push = 0.5 * pen_y * dir;
                                    boxes[i].2 -= push;
                                    boxes[j].2 += push;
                                }
                            }
                        }
                    }
                    if !any_moved {
                        break;
                    }
                }
                let mut deltas = BTreeMap::new();
                for (id, x, y, _, _, orig_x, orig_y) in boxes {
                    deltas.insert(id, (x - orig_x, y - orig_y));
                }
                self.for_each_selected_mut(|obj| {
                    if !obj.locked
                        && let Some(&(dx, dy)) = deltas.get(&obj.id)
                        && let Ok(tf) = Affine2::translation(dx, dy)
                        && let Ok(composed) = obj.transform.then(tf)
                    {
                        obj.transform = composed;
                    }
                });
            }
            EngineCommand::ApplyVariableWidthStroke { object_id, widths_pt } => {
                if widths_pt.len() < 2 {
                    return false;
                }
                let raw_widths: Vec<f64> = widths_pt.iter().map(|w| w.get()).collect();
                let mut changed = false;
                self.for_each_node_mut(|obj| {
                    if obj.id == object_id
                        && !obj.locked
                        && let Some(base_path) = object_to_path(obj)
                        && let Ok(ribbon) = expand_tapered_stroke_profile(&base_path, &raw_widths)
                    {
                        if obj.fill.is_none() {
                            let color = obj.outline.as_ref().map_or_else(|| Color::rgb8(31, 41, 55), |o| o.color.clone());
                            obj.fill = Some(FillStyle::Uniform { color });
                        }
                        obj.kind = ObjectKind::Path(ribbon);
                        changed = true;
                    }
                });
                if !changed {
                    return false;
                }
            }
            EngineCommand::ChaikinSmoothObjectPath { object_id, iterations } => {
                let mut changed = false;
                self.for_each_node_mut(|obj| {
                    if obj.id == object_id
                        && !obj.locked
                        && let Some(base_path) = object_to_path(obj)
                        && let Ok(smoothed) = chaikin_smooth_path(&base_path, iterations)
                    {
                        obj.kind = ObjectKind::Path(smoothed);
                        changed = true;
                    }
                });
                if !changed {
                    return false;
                }
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

    /// Computes `(length_pt, signed_area_sq_pt)` of the world-space contour of `object_id` (`Object Properties -> Curve`).
    pub fn measure_object_geometry(&self, object_id: ObjectId) -> Option<(f64, f64)> {
        let obj = self.selected_or_all_node(object_id)?;
        let local_path = object_to_path(obj)?;
        let world_path = local_path.transform(obj.transform).ok()?;
        measure_path_length_and_area(&world_path).ok()
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
                self.document.pages[page_idx]
                    .layers
                    .iter()
                    .rev()
                    .find(|l| l.role == LayerRole::Normal)
                    .or_else(|| self.document.pages[page_idx].layers.last())
                    .map_or(&[], |l| l.objects.as_slice())
            }
            ActivePageTarget::MasterPage => {
                self.document.master_layers.iter().find(|l| l.role == LayerRole::MasterAllPages).map_or(&[], |l| l.objects.as_slice())
            }
            ActivePageTarget::Desktop => {
                self.document.master_layers.iter().find(|l| l.role == LayerRole::Desktop).map_or(&[], |l| l.objects.as_slice())
            }
        }
    }

    /// Looks up an [`ObjectNode`] by `id` across all numbered pages and master/desktop layers.
    #[must_use]
    pub fn inspect_object(&self, id: ObjectId) -> Option<&ObjectNode> {
        self.selected_or_all_node(id)
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
                if !self.document.pages[page_idx].layers.iter().any(|l| l.role == LayerRole::Normal) {
                    let layer_id = LayerId(self.document.allocate_object_id().0);
                    self.document.pages[page_idx].layers.push(Layer::new_normal(layer_id, "Layer 1"));
                }
                self.document.pages[page_idx].layers.iter_mut().rev().find(|l| l.role == LayerRole::Normal).unwrap_or_else(|| unreachable!())
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

        // 17. ReducePathNodes, SetSelectionBounds, AddLayer, and SetLayerProperties
        engine.set_active_target(ActivePageTarget::Page(1));
        assert!(engine.execute(EngineCommand::ReducePathNodes { object_id: boundary_id, tolerance_pt: F64::new(1.5).unwrap() }));
        engine.set_selection(vec![boundary_id]);
        assert!(engine.execute(EngineCommand::SetSelectionBounds {
            x_pt: Some(F64::new(100.0).unwrap()),
            y_pt: Some(F64::new(100.0).unwrap()),
            width_pt: Some(F64::new(80.0).unwrap()),
            height_pt: Some(F64::new(50.0).unwrap()),
            origin_index: 4,
        }));
        let layers_before = engine.document.pages[1].layers.len();
        assert!(engine.execute(EngineCommand::AddLayer { name: "Layer 2".to_owned() }));
        assert_eq!(engine.document.pages[1].layers.len(), layers_before + 1);
        let new_layer_id = engine.document.pages[1].layers.last().unwrap().id;
        assert!(engine.execute(EngineCommand::SetLayerProperties {
            layer_id: new_layer_id,
            visible: Some(false),
            locked: Some(true),
            printable: Some(false),
            opacity: Some(F64::new(0.5).unwrap()),
        }));
        let updated_layer = engine.document.pages[1].layers.last().unwrap();
        assert!(!updated_layer.visible && updated_layer.locked && !updated_layer.printable);
        assert!(engine.undo());
        assert!(engine.undo());

        // 18. ClearTransformations, ToggleTextKind (Ctrl+F8), and FindAndReplaceText
        assert!(engine.execute(EngineCommand::ClearTransformations));
        let art_id = engine.document.allocate_object_id();
        engine.target_layer_mut().objects.push(ObjectNode::new(
            art_id,
            Some("Headline".to_owned()),
            ObjectKind::ArtisticText {
                text: "Hello Corel".to_owned(),
                font_family: "Sans".to_owned(),
                size_pt: F64::new(18.0).unwrap(),
                position: Point::new(20.0, 40.0).unwrap(),
            },
            Some(FillStyle::uniform(Color::BLACK)),
            None,
        ));
        assert!(engine.execute(EngineCommand::FindAndReplaceText { find: "Corel".to_owned(), replace: "CoralCraft".to_owned() }));
        assert!(engine.execute(EngineCommand::ToggleTextKind { object_id: art_id }));
        if let ObjectKind::ParagraphText(pt) = &engine.selected_or_all_node(art_id).unwrap().kind {
            assert_eq!(pt.text, "Hello CoralCraft");
        } else {
            panic!("expected ParagraphText after ToggleTextKind");
        }

        // 19. ChangeTextCase (Shift+F3), InsertCurveNode (+), and DeleteCurveNode (-)
        assert!(engine.execute(EngineCommand::ChangeTextCase { object_id: art_id, case_kind: TextCaseKind::Uppercase }));
        if let ObjectKind::ParagraphText(pt) = &engine.selected_or_all_node(art_id).unwrap().kind {
            assert_eq!(pt.text, "HELLO CORALCRAFT");
        } else {
            panic!("expected ParagraphText after ChangeTextCase");
        }
        assert!(engine.execute(EngineCommand::InsertCurveNode { object_id: boundary_id, element_index: 1 }));
        assert!(engine.execute(EngineCommand::DeleteCurveNode { object_id: boundary_id, element_index: 1 }));

        // 20. ConvertSegmentToCurve, ConvertSegmentToLine, SetPageBackground, and SetPageBleed
        assert!(engine.execute(EngineCommand::ConvertSegmentToCurve { object_id: boundary_id, element_index: 1 }));
        assert!(engine.execute(EngineCommand::ConvertSegmentToLine { object_id: boundary_id, element_index: 1 }));
        assert!(engine.execute(EngineCommand::SetPageBackground { page_index: 0, background: Some(FillStyle::uniform(Color::rgb8(245, 245, 240))) }));
        assert!(engine.execute(EngineCommand::SetPageBleed { page_index: 0, bleed_pt: F64::new(9.0).unwrap() }));
        assert_eq!(engine.document.pages[0].bleed, F64::new(9.0).unwrap());

        // 21. SetObjectOpacity, SetObjectVisible, and RenameObject
        assert!(engine.execute(EngineCommand::SetObjectOpacity { object_id: boundary_id, opacity: F64::new(0.75).unwrap() }));
        assert_eq!(engine.selected_or_all_node(boundary_id).unwrap().opacity, F64::new(0.75).unwrap());
        assert!(engine.execute(EngineCommand::SetObjectVisible { object_id: boundary_id, visible: false }));
        assert!(!engine.selected_or_all_node(boundary_id).unwrap().visible);
        assert!(engine.execute(EngineCommand::SetObjectVisible { object_id: boundary_id, visible: true }));
        assert!(engine.execute(EngineCommand::RenameObject { object_id: boundary_id, name: Some("Custom Silhouette".to_owned()) }));
        assert_eq!(engine.selected_or_all_node(boundary_id).unwrap().name.as_deref(), Some("Custom Silhouette"));

        // 22. InvertSelection, AddLayer + MoveLayer + DeleteLayer
        engine.set_selection(vec![boundary_id]);
        assert!(engine.execute(EngineCommand::InvertSelection));
        assert!(!engine.selection.contains(&boundary_id));
        assert!(engine.execute(EngineCommand::AddLayer { name: "Layer 3".to_owned() }));
        let l3_id = engine.document.pages[1].layers.last().unwrap().id;
        assert!(engine.execute(EngineCommand::MoveLayer { page_index: 1, from_index: 1, to_index: 0 }));
        assert!(engine.execute(EngineCommand::DeleteLayer { layer_id: l3_id }));
        assert_eq!(engine.document.pages[1].layers.len(), 1);

        // 23. SetOutlineProperties, SelectByKind, and SetDocumentTitle
        engine.set_selection(vec![boundary_id]);
        assert!(engine.execute(EngineCommand::SetOutlineProperties {
            width_pt: Some(F64::new(3.0).unwrap()),
            behind_fill: Some(true),
            scale_with_object: Some(true),
        }));
        let outl = engine.selected_or_all_node(boundary_id).unwrap().outline.as_ref().unwrap();
        assert!(outl.behind_fill && outl.scale_with_object);
        assert!(engine.execute(EngineCommand::SelectByKind { kind: "path".to_owned() }));
        assert!(engine.selection.contains(&boundary_id));
        assert!(engine.execute(EngineCommand::SetDocumentTitle { title: "CoralCraft Masterpiece".to_owned() }));
        assert_eq!(engine.document.title, "CoralCraft Masterpiece");

        // 24. TwirlObjectPath, CloneEffectsFrom, and ClearEffects
        assert!(engine.execute(EngineCommand::TwirlObjectPath {
            object_id: boundary_id,
            center: Point::new(100.0, 100.0).unwrap(),
            radius_pt: F64::new(60.0).unwrap(),
            angle_deg: F64::new(45.0).unwrap(),
        }));
        assert!(engine.execute(EngineCommand::ApplyContour { object_id: art_id, steps: 1, offset_pt: F64::new(2.0).unwrap(), inside: false }));
        engine.set_selection(vec![boundary_id]);
        assert!(engine.execute(EngineCommand::CloneEffectsFrom { source_id: art_id }));
        assert!(!engine.selected_or_all_node(boundary_id).unwrap().effects.is_empty());
        assert!(engine.execute(EngineCommand::ClearEffects { object_id: boundary_id }));
        assert!(engine.selected_or_all_node(boundary_id).unwrap().effects.is_empty());

        // 25. AttractRepelObjectPath, SmearObjectPath, and SetParagraphTextLayout
        assert!(engine.execute(EngineCommand::AttractRepelObjectPath {
            object_id: boundary_id,
            center: Point::new(100.0, 100.0).unwrap(),
            radius_pt: F64::new(60.0).unwrap(),
            strength: F64::new(0.4).unwrap(),
        }));
        assert!(engine.execute(EngineCommand::SmearObjectPath {
            object_id: boundary_id,
            origin: Point::new(100.0, 100.0).unwrap(),
            radius_pt: F64::new(60.0).unwrap(),
            dx_pt: F64::new(12.0).unwrap(),
            dy_pt: F64::new(-8.0).unwrap(),
        }));
        assert!(engine.execute(EngineCommand::SetParagraphTextLayout {
            object_id: art_id,
            columns: Some(2),
            gutter_pt: Some(F64::new(14.0).unwrap()),
            line_spacing: Some(F64::new(1.5).unwrap()),
            alignment: Some(TextAlignment::Justify),
        }));
        if let ObjectKind::ParagraphText(pt) = &engine.inspect_object(art_id).unwrap().kind {
            assert_eq!(pt.columns, 2);
            assert_eq!(pt.alignment, TextAlignment::Justify);
        } else {
            panic!("expected ParagraphText");
        }

        // 26. RoughenObjectPath and SetArtisticTextProperties
        assert!(engine.execute(EngineCommand::RoughenObjectPath { object_id: boundary_id, tooth_height_pt: F64::new(2.0).unwrap(), frequency: 3 }));
        assert!(engine.execute(EngineCommand::SetArtisticTextProperties {
            object_id: art_id,
            text: Some("UPDATED CORALCRAFT".to_owned()),
            font_family: Some("Helvetica".to_owned()),
            size_pt: Some(F64::new(20.0).unwrap()),
        }));
        if let ObjectKind::ParagraphText(pt) = &engine.inspect_object(art_id).unwrap().kind {
            assert_eq!(pt.text, "UPDATED CORALCRAFT");
            assert_eq!(pt.font_family, "Helvetica");
        } else {
            panic!("expected ParagraphText");
        }
        assert!(engine.execute(EngineCommand::SmoothObjectPath {
            object_id: boundary_id,
            center: Point::new(100.0, 100.0).unwrap(),
            radius_pt: F64::new(60.0).unwrap(),
            strength: F64::new(0.5).unwrap(),
            iterations: 3,
        }));
        assert!(engine.execute(EngineCommand::KnifeCutObject {
            object_id: boundary_id,
            line_start: Point::new(0.0, 80.0).unwrap(),
            line_end: Point::new(200.0, 80.0).unwrap(),
            gap_pt: F64::new(2.0).unwrap(),
        }));
        engine.set_selection(vec![boundary_id]);
        assert!(engine.execute(EngineCommand::CropToRect { rect: Rect::new(0.0, 0.0, 300.0, 300.0).unwrap() }));
        assert!(engine.execute(EngineCommand::EraseObjectCircle {
            object_id: boundary_id,
            center: Point::new(100.0, 100.0).unwrap(),
            radius_pt: F64::new(15.0).unwrap(),
        }));
        assert!(engine.execute(EngineCommand::SetSnapSettings { snap_to_objects: Some(false), snap_to_page: Some(true) }));
        assert!(!engine.snap_to_objects && engine.snap_to_page);
        assert!(engine.execute(EngineCommand::VirtualSegmentDelete { object_id: boundary_id, element_index: 1 }));
        assert!(engine.execute(EngineCommand::SmartFillAtPoint {
            point: Point::new(100.0, 100.0).unwrap(),
            fill: FillStyle::uniform(Color::rgb8(0, 180, 120)),
            outline: None,
        }));
        let sf_id = engine.selection[0];
        assert!(engine.execute(EngineCommand::OffsetObjectPath { object_id: sf_id, distance_pt: F64::new(5.0).unwrap(), copy: true }));
        let off_id = engine.selection[0];
        assert!(engine.execute(EngineCommand::SetPathFillRule { object_id: sf_id, fill_rule: FillRule::EvenOdd }));
        assert!(engine.execute(EngineCommand::SwapObjectPositions { first_id: sf_id, second_id: off_id }));
        engine.set_selection(vec![sf_id]);
        assert!(engine.execute(EngineCommand::CenterSelectionToPage { horizontal: true, vertical: true }));
        assert!(engine.measure_object_geometry(sf_id).is_some());
        assert!(engine.execute(EngineCommand::SmoothCurveNode { object_id: sf_id, element_index: 1, tension: F64::new(0.35).unwrap() }));
        assert!(engine.execute(EngineCommand::ArrayDuplicateSelection {
            rows: 2,
            cols: 2,
            spacing_x_pt: F64::new(20.0).unwrap(),
            spacing_y_pt: F64::new(20.0).unwrap(),
        }));
        assert_eq!(engine.selection.len(), 3);
        assert!(engine.execute(EngineCommand::AddGuideline { horizontal: true, position_pt: F64::new(120.0).unwrap() }));
        assert!(engine.execute(EngineCommand::ClearGuidelines));
        assert!(engine.execute(EngineCommand::CircularDuplicateSelection {
            count: 3,
            center: Point::new(100.0, 100.0).unwrap(),
            total_angle_deg: F64::new(360.0).unwrap(),
            rotate_objects: true,
        }));
        assert!(engine.execute(EngineCommand::SetOutlineCapAndJoin {
            cap: Some(LineCap::Round),
            join: Some(LineJoin::Round),
            miter_limit: Some(F64::new(8.0).unwrap()),
        }));
        assert!(engine.execute(EngineCommand::FitPageToSelection { margin_pt: F64::new(10.0).unwrap() }));
        assert!(engine.execute(EngineCommand::AlignCurveNodes { object_id: sf_id, element_indices: vec![0, 1], horizontal: true, vertical: false }));
        assert!(engine.execute(EngineCommand::ShiftSelectionColors {
            hue_delta_deg: F64::new(45.0).unwrap(),
            saturation_delta: F64::new(0.1).unwrap(),
            brightness_delta: F64::new(-0.05).unwrap(),
        }));
        assert!(engine.execute(EngineCommand::SubdividePathSegments { object_id: sf_id }));
        assert!(engine.execute(EngineCommand::ConvertSelectionColors { invert: true, grayscale: true }));
        assert!(engine.execute(EngineCommand::TransformCurveNodes {
            object_id: sf_id,
            element_indices: vec![0, 1],
            scale_x: F64::new(1.2).unwrap(),
            scale_y: F64::new(0.8).unwrap(),
            rotate_deg: F64::new(15.0).unwrap(),
        }));
        assert!(engine.execute(EngineCommand::SelectByColor {
            color: Color::rgb8(128, 128, 128),
            match_fill: true,
            match_outline: true,
            tolerance_u8: 255,
        }));
        assert!(!engine.selection.is_empty());
        assert!(engine.execute(EngineCommand::BreakPathAtNode { object_id: sf_id, element_index: 1 }));
        assert!(engine.execute(EngineCommand::EqualizeSelectionSize { match_width: true, match_height: true }));
        assert!(engine.execute(EngineCommand::DistributeExactGap { horizontal: true, gap_pt: F64::new(12.0).unwrap() }));
        assert!(engine.execute(EngineCommand::MorphBetweenObjects { start_id: sf_id, end_id: off_id, t: F64::new(0.5).unwrap(), sample_count: 16 }));
        let morph_id = engine.selection[0];
        engine.set_selection(vec![sf_id, off_id]);
        assert!(engine.execute(EngineCommand::DistributeSelectionAlongPath { spine_object_id: morph_id, rotate_to_tangent: true }));
        assert!(engine.execute(EngineCommand::RecolorArtwork {
            from_color: Color::rgb8(128, 128, 128),
            to_color: Color::rgb8(20, 140, 220),
            tolerance_u8: 255,
            selection_only: false,
        }));
        engine.set_selection(vec![sf_id, off_id]);
        assert!(engine.execute(EngineCommand::DivideSelection));
        let div_ids = engine.selection.clone();
        assert!(engine.execute(EngineCommand::CreateIntermediateBlendPaths { start_id: div_ids[0], end_id: div_ids[1], steps: 2, sample_count: 16 }));
        assert_eq!(engine.selection.len(), 2);
        assert!(engine.execute(EngineCommand::HatchObjectPath {
            object_id: div_ids[0],
            angle_deg: F64::new(45.0).unwrap(),
            spacing_pt: F64::new(6.0).unwrap(),
            cross_hatch: true,
        }));
        assert!(engine.execute(EngineCommand::ProjectSelectionAxonometric { plane: AxonometricPlane::IsometricTop }));
        assert!(engine.execute(EngineCommand::PackSelectionInGrid {
            origin: Point::new(30.0, 30.0).unwrap(),
            cols: 2,
            gap_pt: F64::new(10.0).unwrap(),
        }));
        assert!(engine.execute(EngineCommand::ResolveSelectionOverlaps { min_gap_pt: F64::new(16.0).unwrap(), iterations: 10 }));
        assert!(engine.execute(EngineCommand::ExcludeSelection));
        let xor_id = engine.selection[0];
        assert!(engine.execute(EngineCommand::ChaikinSmoothObjectPath { object_id: xor_id, iterations: 2 }));
        assert!(engine.execute(EngineCommand::ApplyVariableWidthStroke {
            object_id: xor_id,
            widths_pt: vec![F64::new(2.0).unwrap(), F64::new(10.0).unwrap(), F64::new(4.0).unwrap()],
        }));
    }
}
