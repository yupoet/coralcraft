//! Atomic editor commands, targets, and error types for `coralcraft-engine` (`R8.1`).

use coralcraft_color::Color;
use coralcraft_doc::{
    Document, FillStyle, LayerId, LineCap, LineJoin, ObjectId, ObjectKind, OutlineStyle, TextAlignment, TextCaseKind, TransparencyStyle,
};
use coralcraft_geom::{AxonometricPlane, BooleanOp, CornerKind, DistortionKind, EllipseMode, F64, FillRule, Point, Rect, Unit};
use coralcraft_tools::{AlignMode, DistributeMode, PolygonSubMode, ToolState, ZOrderOp};
use serde::{Deserialize, Serialize};

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
    /// Applies a sinusoidal wave distortion along the local normals of `object_id` (`Effects -> Distort -> Wave Distortion`).
    WaveDistortObjectPath { object_id: ObjectId, amplitude_pt: F64, frequency: F64, phase_deg: F64 },
    /// Scales the radial distance of each selected unlocked object's center from `center` by `radial_scale` and rotates its angular position around `center` by `twist_deg` (`Object -> Transformations -> Polar Scatter / Radial Scale`).
    RadialScaleSelectionPositions { center: Point, radial_scale: F64, twist_deg: F64 },
    /// Replaces `object_id` with a cyclic ($C_n$) or dihedral ($D_n$) radial kaleidoscope symmetry compound path around `center` (`Object -> Symmetry -> Create New Symmetry`).
    ApplyDihedralSymmetry { object_id: ObjectId, center: Point, folds: u32, reflect: bool },
    /// Scales each selected unlocked object around its own center based on its radial distance from `center` within `falloff_radius_pt` (`Object -> Transformations -> Falloff Scale by Distance`).
    ScaleSelectionByDistance { center: Point, min_scale: F64, max_scale: F64, falloff_radius_pt: F64 },
    /// Splits the contour of `object_id` along its arc length into discrete open dashed subpaths according to `dash_pattern_pt` (`Object -> Shaping -> Split Curve into Dashes`).
    DashObjectPath { object_id: ObjectId, dash_pattern_pt: Vec<F64> },
    /// Snaps each selected unlocked object's top-left world bounding corner to the nearest `(grid_x_pt, grid_y_pt)` grid multiple (`Object -> Align and Distribute -> Snap Selection to Grid`).
    QuantizeSelectionPositions { grid_x_pt: F64, grid_y_pt: F64 },
    /// Creates a new closed convex-hull [`ObjectKind::Path`] enclosing all world-space vertices of the current selection (`Object -> Shaping -> Convex Hull`).
    ConvexHullSelection,
    /// Places all selected unlocked objects at equal angular intervals around a circle of `radius_pt` centered at `center` starting at `start_angle_deg`, optionally rotating each object to align with the circle tangent (`Object -> Align and Distribute -> Arrange on Circle`).
    ArrangeSelectionOnCircle { center: Point, radius_pt: F64, start_angle_deg: F64, rotate_to_tangent: bool },
    /// Replaces `object_id` with its internal medial-chord centerline skeleton sampled at `sample_steps` (`Object -> Shaping -> Centerline Skeleton`).
    ExtractPathSkeleton { object_id: ObjectId, sample_steps: u32 },
    /// Applies a deterministic SplitMix64 pseudo-random translation, rotation, and uniform scale around each selected unlocked object's center (`Object -> Transformations -> Jitter / Randomize Selection`).
    JitterSelectionTransforms { dx_max_pt: F64, dy_max_pt: F64, angle_max_deg: F64, scale_max_pct: F64, seed: u64 },
    /// Densifies the contour of `object_id` by subdividing every segment longer than `max_len_pt` (`Shape Tool F10 -> Densify / Subdivide by Max Length`).
    DensifyObjectPath { object_id: ObjectId, max_len_pt: F64 },
    /// Aligns all selected unlocked objects to a stationary `key_object_id` according to `mode` (`Object -> Align and Distribute -> Align to Key Object`).
    AlignSelectionToKeyObject { key_object_id: ObjectId, mode: AlignMode },
    /// Applies a CorelDRAW Fish-Eye / Spherical Lens radial warp to `object_id` within `radius_pt` of `center` at `magnification` (`Effects -> Lens -> Fish Eye Warp`).
    SphericalWarpObjectPath { object_id: ObjectId, center: Point, radius_pt: F64, magnification: F64 },
    /// Linearly interpolates fill (`apply_to_fill`) and/or outline (`apply_to_outline`) colors across all selected unlocked objects in selection order from `start_color` to `end_color` (`Effects -> Adjust -> Colorize Selection by Gradient`).
    ColorizeSelectionByGradient { start_color: Color, end_color: Color, apply_to_fill: bool, apply_to_outline: bool },
    /// Globally relaxes all subpaths of `object_id` using shrinkage-compensated Taubin/Laplacian vertex smoothing over `iterations` (`1..=16`) with factor `lambda` (`Shape Tool F10 -> Global Curve Relaxation`).
    RelaxObjectPath { object_id: ObjectId, iterations: u32, lambda: F64 },
    /// Cycles `colors` across all selected unlocked objects in selection order, updating fill (`apply_to_fill`) and/or outline (`apply_to_outline`) (`Object -> Color -> Apply Palette Swatches to Selection`).
    ApplyPaletteSwatchesToSelection { colors: Vec<Color>, apply_to_fill: bool, apply_to_outline: bool },
    /// Replaces `object_id` with a least-squares Kåsa algebraic circle fitted to its world-space path points (`Object -> Shaping -> Fit Circle to Curve`).
    FitCircleToObjectPath { object_id: ObjectId },
    /// Translates each selected unlocked object radially from `center` so its center lies on the circle of `radius_pt` (`Object -> Align and Distribute -> Snap Selection to Circle Radius`).
    SnapSelectionToCircleRadius { center: Point, radius_pt: F64 },
    /// Converts each subpath of `object_id` into a $C^1$-continuous Catmull-Rom cubic Bézier curve with `tension` (`Shape Tool F10 -> Convert Polyline to Smooth Cubic Curves`).
    SmoothPolylineToCubicBeziers { object_id: ObjectId, tension: F64 },
    /// Rotates each selected unlocked object around its own center to face radially away from (or toward) `center` plus `angle_offset_deg` (`Object -> Transformations -> Orient Selection to Center`).
    OrientSelectionToCenter { center: Point, angle_offset_deg: F64 },
    /// Applies a radial Pinch (`strength < 0`) or Punch (`strength > 0`) warp to `object_id` within `radius_pt` around `center` (`Effects -> Distort -> Pinch / Punch Warp`).
    PinchPunchObjectPath { object_id: ObjectId, center: Point, radius_pt: F64, strength: F64 },
    /// Packs all selected unlocked objects edge-to-edge horizontally (`horizontal = true`) or vertically (`horizontal = false`) with `gap_pt` separation (`Object -> Align and Distribute -> Stack Selection`).
    StackSelection { horizontal: bool, gap_pt: F64 },
    /// Flattens all curves of `object_id` into linear polyline subpaths within flatness `tolerance_pt` (`Shape Tool F10 -> Polygonize / Convert Curves to Lines`).
    FlattenObjectPathToPolylines { object_id: ObjectId, tolerance_pt: F64 },
    /// Scales each selected unlocked object around its own center by a linearly interpolated scale factor from `start_scale` to `end_scale` in selection order (`Object -> Transformations -> Progressive Scale`).
    ScaleSelectionProgressively { start_scale: F64, end_scale: F64 },
    /// Applies a bounding-box-normalized oblique shear warp (`shear_x_pt`, `shear_y_pt`) to `object_id`'s path (`Object -> Envelope / Shear Warp`).
    ShearWarpObjectPath { object_id: ObjectId, shear_x_pt: F64, shear_y_pt: F64 },
    /// Rotates each selected unlocked object around its own center by a linearly interpolated angle from `start_angle_deg` to `end_angle_deg` in selection order (`Object -> Transformations -> Progressive Rotate`).
    RotateSelectionProgressively { start_angle_deg: F64, end_angle_deg: F64 },
}

/// Maximum number of undo history entries retained per [`EditorEngine`].
pub const MAX_UNDO_HISTORY: usize = 128;

/// Maximum number of [`ObjectNode`]s allowed across a document (including group and PowerClip children).
pub const MAX_DOCUMENT_OBJECTS: usize = 8_192;

/// Error returned when [`EditorEngine::try_execute`] rejects an [`EngineCommand`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EngineError {
    /// The command preconditions failed or the command produced no valid change.
    CommandRejected,
    /// Executing the command would exceed [`MAX_DOCUMENT_OBJECTS`].
    ObjectBudgetExceeded { max: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UndoEntry {
    pub(crate) document: Document,
    pub(crate) active_target: ActivePageTarget,
    pub(crate) selection: Vec<ObjectId>,
    pub(crate) tool_state: ToolState,
    pub(crate) default_fill: Option<FillStyle>,
    pub(crate) default_outline: Option<OutlineStyle>,
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
