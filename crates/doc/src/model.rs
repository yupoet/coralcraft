//! Multi-page CorelDRAW document model (`Document`, `Page`, `Layer`, `ObjectNode`, `ObjectKind`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use coralcraft_color::{Color, Palette};
use coralcraft_geom::{Affine2, BBox, EllipseSpec, F64, Path, Point, PolygonSpec, RectSpec, Size, Unit};

use crate::style::{ChunkTag, FillStyle, OutlineStyle, RawPayload, Residual};

/// Current `.coralcraft` native document schema version (`1.0`).
pub const CURRENT_SCHEMA: SchemaVersion = SchemaVersion { major: 1, minor: 0 };

/// Errors returned when constructing, loading, or saving a [`Document`].
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DocError {
    /// Input bytes do not start with `{` (JSON) or `PK\x03\x04` (ZIP).
    #[error("unrecognized .coralcraft file header (expected JSON '{{' or ZIP 'PK\\x03\\x04')")]
    BadMagic,
    /// Document or manifest major version is incompatible with this reader.
    #[error("unsupported .coralcraft schema major version {found} (expected {expected})")]
    UnsupportedVersion {
        /// Major version found in the file.
        found: u16,
        /// Supported major version.
        expected: u16,
    },
    /// Reserved key `$payload` appeared in an `extra` forward-compatibility map.
    #[error("reserved key `$payload` is forbidden in extra fields")]
    ReservedPayloadKey,
    /// JSON serialization or deserialization error.
    #[error("JSON format error: {0}")]
    Json(String),
    /// Malformed or unsafe `.coralcraft` ZIP archive structure.
    #[error("corrupt or unsafe .coralcraft ZIP container: {0}")]
    InvalidZipContainer(String),
    /// Content-addressed blob failed SHA-256 or byte-length verification.
    #[error("corrupt blob `{id}` in .coralcraft ZIP container: {reason}")]
    CorruptBlob {
        /// Content-addressed SHA-256 hex ID.
        id: String,
        /// Verification failure reason.
        reason: String,
    },
    /// Requested page index does not exist in the document.
    #[error("page index {index} out of range (document has {count} pages)")]
    PageOutOfRange {
        /// Requested 0-based page index.
        index: usize,
        /// Total page count.
        count: usize,
    },
}

/// Semantic version of the `.coralcraft` document schema.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SchemaVersion {
    /// Incompatible major version.
    pub major: u16,
    /// Backward-compatible minor version.
    pub minor: u16,
}

impl Default for SchemaVersion {
    fn default() -> Self {
        CURRENT_SCHEMA
    }
}

/// Stable identifier for a [`Page`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PageId(pub u64);

/// Stable identifier for a [`Layer`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct LayerId(pub u64);

/// Stable identifier for an [`ObjectNode`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ObjectId(pub u64);

/// Role of a layer in CorelDRAW's Object Manager (`Objects` Docker).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayerRole {
    /// Standard per-page drawing layer.
    #[default]
    Normal,
    /// Master layer repeated on all pages.
    MasterAllPages,
    /// Master layer repeated on even-numbered pages (`2, 4, 6, ...`).
    MasterEvenPages,
    /// Master layer repeated on odd-numbered pages (`1, 3, 5, ...`).
    MasterOddPages,
    /// Shared cross-page pasteboard (`Desktop`) layer where objects placed outside page bounds live.
    Desktop,
    /// Guidelines layer (non-printing by default).
    Guides,
    /// Document grid layer (non-printing).
    Grid,
}

/// First-class passthrough node for unimplemented or opaque CDR object records.
///
/// Never dropped during load, save, grouping, or z-order edits.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnsupportedObject {
    /// Original 4CC chunk tag (e.g. `b"lens"`, `b"mesh"`, `b"enve"`).
    pub tag: ChunkTag,
    /// Optional human-readable class hint for diagnostics and the Object Manager UI.
    pub class_hint: Option<String>,
    /// Verbatim binary payload bytes (never decoded and re-encoded).
    pub payload: RawPayload,
    /// Optional local-space bounding box for selection, hit-testing, and wireframe fallback rendering.
    pub fallback_bounds: Option<BBox>,
    /// When `true`, `payload` embeds absolute page-space coordinates.
    pub absolute_geometry: bool,
    /// Child objects if the unsupported record is a container.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<ObjectNode>,
    /// Nested unknown chunks.
    #[serde(default)]
    pub residual: Residual,
}

/// Variant payload of an [`ObjectNode`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ObjectKind {
    /// Parametric CorelDRAW rectangle (with per-corner `Round`, `Scalloped`, or `Chamfered` radii).
    Rect(RectSpec),
    /// Parametric CorelDRAW ellipse (`Full`, `Pie`, or `Arc`).
    Ellipse(EllipseSpec),
    /// Parametric CorelDRAW polygon or star.
    Polygon(PolygonSpec),
    /// Arbitrary editable Bézier curve path.
    Path(Path),
    /// CorelDRAW Artistic Text object (`F8`).
    ArtisticText {
        /// UTF-8 text content.
        text: String,
        /// Font family name.
        font_family: String,
        /// Font size in typographic points.
        size_pt: F64,
        /// Baseline origin in local coordinates.
        position: Point,
    },
    /// Group of child objects.
    Group {
        /// Ordered child nodes (`0` = bottom, `len - 1` = top).
        children: Vec<ObjectNode>,
    },
    /// First-class passthrough node for unsupported CDR records.
    Unsupported(UnsupportedObject),
}

/// A scene-graph object on a [`Layer`] or inside a `Group` / `PowerClip` container.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectNode {
    /// Stable object identifier.
    pub id: ObjectId,
    /// Optional user-assigned object name in the Object Manager.
    pub name: Option<String>,
    /// Local-to-parent affine transformation matrix.
    pub transform: Affine2,
    /// Whether the object is visible on canvas and in exports.
    pub visible: bool,
    /// Whether the object is locked against interactive selection/editing.
    pub locked: bool,
    /// Object opacity (`0.0..=1.0`).
    pub opacity: F64,
    /// Fill style (`None` = unfilled / transparent interior).
    pub fill: Option<FillStyle>,
    /// Outline Pen style (`None` = no outline).
    pub outline: Option<OutlineStyle>,
    /// Objects clipped inside this object via CorelDRAW **PowerClip** (`Object -> PowerClip -> Place Inside Frame`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub powerclip_children: Vec<ObjectNode>,
    /// Geometric or container kind of this object.
    pub kind: ObjectKind,
    /// Object-scoped unknown CDR chunks.
    #[serde(default)]
    pub residual: Residual,
    /// Forward-compatible unknown JSON fields.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl ObjectNode {
    /// Constructs a visible, unlocked `ObjectNode` with identity transform and `1.0` opacity.
    #[must_use]
    pub fn new(id: ObjectId, name: Option<String>, kind: ObjectKind, fill: Option<FillStyle>, outline: Option<OutlineStyle>) -> Self {
        Self {
            id,
            name,
            transform: Affine2::IDENTITY,
            visible: true,
            locked: false,
            opacity: F64::ONE,
            fill,
            outline,
            powerclip_children: Vec::new(),
            kind,
            residual: Residual::default(),
            extra: BTreeMap::new(),
        }
    }
}

/// A drawing, master, desktop, or guide layer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Layer {
    /// Stable layer identifier.
    pub id: LayerId,
    /// Layer name (e.g. `"Layer 1"`, `"Desktop"`, `"Guides"`).
    pub name: String,
    /// Role of this layer in the multi-page publication model.
    pub role: LayerRole,
    /// Whether objects on this layer are rendered.
    pub visible: bool,
    /// Whether objects on this layer are included when printing or exporting (`false` for Guides/Grid by default).
    pub printable: bool,
    /// Whether objects on this layer are locked against editing.
    pub locked: bool,
    /// For master layers: when `true`, renders in front of page layers; when `false`, renders behind page layers.
    #[serde(default)]
    pub master_in_front: bool,
    /// Layer opacity (`0.0..=1.0`).
    pub opacity: F64,
    /// Objects on this layer ordered from bottom (`0`) to top (`len - 1`).
    pub objects: Vec<ObjectNode>,
    /// Layer-scoped unknown CDR chunks.
    #[serde(default)]
    pub residual: Residual,
    /// Forward-compatible unknown JSON fields.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl Layer {
    /// Creates a standard visible, printable drawing layer.
    #[must_use]
    pub fn new_normal(id: LayerId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            role: LayerRole::Normal,
            visible: true,
            printable: true,
            locked: false,
            master_in_front: false,
            opacity: F64::ONE,
            objects: Vec::new(),
            residual: Residual::default(),
            extra: BTreeMap::new(),
        }
    }

    /// Creates the shared CorelDRAW `Desktop` pasteboard layer.
    #[must_use]
    pub fn new_desktop(id: LayerId) -> Self {
        Self {
            id,
            name: "Desktop".to_owned(),
            role: LayerRole::Desktop,
            visible: true,
            printable: false,
            locked: false,
            master_in_front: true,
            opacity: F64::ONE,
            objects: Vec::new(),
            residual: Residual::default(),
            extra: BTreeMap::new(),
        }
    }
}

/// A single page in a multi-page CorelDRAW document, with independent dimensions and bleed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Page {
    /// Stable page identifier.
    pub id: PageId,
    /// Page tab label (e.g. `"Page 1"`).
    pub name: String,
    /// Page dimensions `(width, height)` in typographic points (`1/72 inch`).
    pub size: Size,
    /// Bleed margin outside the page trim box in typographic points.
    pub bleed: F64,
    /// Optional page background fill (`None` = default paper white or transparent export).
    pub background: Option<FillStyle>,
    /// Per-page layers ordered from bottom (`0`) to top (`len - 1`).
    pub layers: Vec<Layer>,
    /// Page-scoped unknown CDR chunks.
    #[serde(default)]
    pub residual: Residual,
    /// Forward-compatible unknown JSON fields.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// Top-level multi-page CorelDRAW document (`Document`).
///
/// Self-contained and deterministic: derives [`PartialEq`] and [`Eq`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Document {
    /// Native `.coralcraft` schema version.
    pub schema: SchemaVersion,
    /// Document identifier.
    pub id: u64,
    /// Monotonic ID allocator counter persisted for deterministic ID assignment.
    pub next_id: u64,
    /// Authoring measurement unit displayed in the Property Bar and Rulers.
    pub units: Unit,
    /// Document title.
    pub title: String,
    /// Ordered pages in the publication (`0` = Page 1).
    pub pages: Vec<Page>,
    /// Document-wide Master, Desktop, Guides, and Grid layers.
    pub master_layers: Vec<Layer>,
    /// Document color palette.
    pub palette: Palette,
    /// Document-scoped unknown CDR chunks.
    #[serde(default)]
    pub residual: Residual,
    /// Forward-compatible unknown JSON fields.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl Default for Document {
    fn default() -> Self {
        Self::new_a4_portrait()
    }
}

impl Document {
    /// Creates a new 1-page A4 Portrait (`210 mm × 297 mm` = `595.27559 pt × 841.88976 pt`) CorelDRAW document
    /// with `"Layer 1"`, a shared `"Desktop"` layer, and the default CMYK/RGB workspace palette.
    #[must_use]
    pub fn new_a4_portrait() -> Self {
        let width_pt = Unit::Mm.to_pt(210.0).map_or(595.275_590_551_181_2, |p| p.get());
        let height_pt = Unit::Mm.to_pt(297.0).map_or(841.889_763_779_527_6, |p| p.get());
        let size = Size::new(width_pt, height_pt).unwrap_or_default();

        let page1 = Page {
            id: PageId(1),
            name: "Page 1".to_owned(),
            size,
            bleed: F64::ZERO,
            background: Some(FillStyle::uniform(Color::WHITE)),
            layers: vec![Layer::new_normal(LayerId(2), "Layer 1")],
            residual: Residual::default(),
            extra: BTreeMap::new(),
        };

        let desktop = Layer::new_desktop(LayerId(3));

        Self {
            schema: CURRENT_SCHEMA,
            id: 1,
            next_id: 4,
            units: Unit::Mm,
            title: "Untitled-1".to_owned(),
            pages: vec![page1],
            master_layers: vec![desktop],
            palette: Palette::default_workspace_palette(),
            residual: Residual::default(),
            extra: BTreeMap::new(),
        }
    }

    /// Allocates the next deterministic monotonic ID in this document.
    pub fn allocate_object_id(&mut self) -> ObjectId {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        ObjectId(id)
    }

    /// Returns the ordered sequence of layers that should be rendered for `page_index` (0-based),
    /// implementing CorelDRAW's multi-page + Master + Desktop layer compositing order:
    /// 1. Matching Master background layers (`!master_in_front`)
    /// 2. Page's own layers (`bottom -> top`)
    /// 3. Matching Master foreground layers (`master_in_front`)
    /// 4. Shared `Desktop` layer (included on screen; included in print/export only if `printable == true`).
    pub fn effective_layers_for_page(&self, page_index: usize, for_print_or_export: bool) -> Result<Vec<&Layer>, DocError> {
        let Some(page) = self.pages.get(page_index) else {
            return Err(DocError::PageOutOfRange { index: page_index, count: self.pages.len() });
        };

        let page_num_1based = page_index + 1;
        let is_odd = page_num_1based % 2 == 1;

        let should_draw = |layer: &Layer| -> bool {
            if !layer.visible {
                return false;
            }
            if for_print_or_export && !layer.printable {
                return false;
            }
            true
        };

        let matches_master_parity = |role: LayerRole| -> bool {
            match role {
                LayerRole::MasterAllPages => true,
                LayerRole::MasterOddPages => is_odd,
                LayerRole::MasterEvenPages => !is_odd,
                _ => false,
            }
        };

        let mut ordered = Vec::new();

        // 1. Background Master layers
        for l in &self.master_layers {
            if matches_master_parity(l.role) && !l.master_in_front && should_draw(l) {
                ordered.push(l);
            }
        }

        // 2. Page-local layers
        for l in &page.layers {
            if should_draw(l) {
                ordered.push(l);
            }
        }

        // 3. Foreground Master layers
        for l in &self.master_layers {
            if matches_master_parity(l.role) && l.master_in_front && should_draw(l) {
                ordered.push(l);
            }
        }

        // 4. Shared Desktop / Guides / Grid layers
        for l in &self.master_layers {
            if matches!(l.role, LayerRole::Desktop | LayerRole::Guides | LayerRole::Grid) && should_draw(l) {
                ordered.push(l);
            }
        }

        Ok(ordered)
    }
}
