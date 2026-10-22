//! `corel-import`: Map CorelDRAW (`.cdr` / `.cmx`) documents between `coralcraft-cdr` (`coralcraft-doc`)
//! and `vectorcraft-doc` (`vectorcraft_doc::Document`), preserving lossless CorelDRAW residuals under
//! `Document::unknown["coralcraft"]`.

use std::collections::BTreeMap;
use std::io::Cursor;
use std::sync::Arc;

use coralcraft_color::{Color as CdrColor, FountainBlendMode, FountainStop, Palette, PaletteSwatch};
use coralcraft_doc::{
    ArrowheadKind, Document as CdrDocument, FillStyle, FountainGeometry, Layer as CdrLayer, LayerId, LayerRole, LineCap as CdrLineCap,
    LineJoin as CdrLineJoin, ObjectId, ObjectKind, ObjectNode, OutlineStyle, Page as CdrPage, PageId, ParagraphTextSpec, PatternKind, Residual,
    SchemaVersion, TextAlignment, TransparencyStyle, VectorEffect,
};
use coralcraft_geom::{
    Affine2, CornerKind as CdrCornerKind, DistortionKind, EllipseMode, EllipseSpec, F64, FillRule as CdrFillRule, Path as CdrPath, PathElement,
    Point as CdrPoint, PolygonSpec, Rect as CdrRect, RectSpec, Size as CdrSize, Unit as CdrUnit,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use thiserror::Error;
use vectorcraft_color::{
    Color as VcColor, Gradient, GradientGeom, GradientKind, GradientPaint, GradientStop, Paint, Swatch, default_swatches, swatch::REGISTRATION,
};
use vectorcraft_doc::{
    Appearance, AppearanceItem, AreaOptions, Arrowhead, Artboard, CharStyle, ColorMode, Dash, Document as VcDocument, Effect, FillLayer, ImageBlob,
    ImageObject, Justify, LayerColor, LineCap as VcLineCap, LineJoin as VcLineJoin, LiveShape, Node, NodeId, NodeKind, OpacityMask, ParaStyle,
    StrokeLayer, TextKind, TextObject, TextRun, Unit as VcUnit,
};
use vectorcraft_geom::{
    Affine, BezPath, FillRule as VcFillRule, PathData, PathEl, Point as VcPoint, Rect as VcRect, Vec2,
    shapes::{self, CornerKind as VcCornerKind},
};

pub use coralcraft_color;
pub use coralcraft_doc;
pub use coralcraft_geom;

/// Horizontal spacing in typographic points between consecutive pages laid out as VectorCraft artboards.
pub const PAGE_GAP_PT: f64 = 40.0;

/// Key in `vectorcraft_doc::Document::unknown` storing lossless CorelDRAW metadata and residuals.
pub const CORALCRAFT_UNKNOWN_KEY: &str = "coralcraft";

/// Stroke width in points used in `vectorcraft-doc` to represent a CorelDRAW Hairline (`0.0 pt`).
pub const HAIRLINE_WIDTH_PT: f64 = 0.25;

/// Errors returned when importing or exporting CorelDRAW documents.
#[derive(Debug, Error)]
pub enum CorelImportError {
    /// Underlying CDR/CMX container or parser error.
    #[error("CDR/CMX parse error: {0}")]
    Cdr(#[from] coralcraft_cdr::ProbeError),
    /// Document model validation or conversion error.
    #[error("CorelDRAW document conversion error: {0}")]
    Conversion(String),
}

/// Outcome of importing a `.cdr` or `.cmx` byte stream into a [`vectorcraft_doc::Document`].
#[derive(Clone, Debug)]
pub struct CorelImportOutcome {
    /// Converted VectorCraft document.
    pub document: VcDocument,
    /// Detected container and version report.
    pub report: coralcraft_cdr::ProbeReport,
    /// Non-fatal parser or conversion warnings.
    pub warnings: Vec<String>,
}

/// Structural counts (`page_count`, `layer_count`, `object_count`) for a [`vectorcraft_doc::Document`]
/// imported from or prepared for CorelDRAW (`.cdr` / `.cmx`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CdrImportMetrics {
    /// Number of artboards (pages) in the VectorCraft document.
    pub page_count: usize,
    /// Number of top-level layers in the VectorCraft document.
    pub layer_count: usize,
    /// Total number of non-layer drawing objects across all layers (including group and PowerClip contents).
    pub object_count: usize,
}

fn count_vc_node_objects(node: &Node) -> usize {
    match &node.kind {
        NodeKind::Layer { children, .. } => children.iter().map(|c| count_vc_node_objects(c)).fold(0usize, usize::saturating_add),
        NodeKind::Group { children, clip: true } => {
            // A clip group represents one PowerClip container object (its first child is the frame)
            // plus its inner PowerClip children (`children.iter().skip(1)`).
            let inner = children.iter().skip(1).map(|c| count_vc_node_objects(c)).fold(0usize, usize::saturating_add);
            inner.saturating_add(1)
        }
        NodeKind::Group { children, clip: false } => {
            let inner = children.iter().map(|c| count_vc_node_objects(c)).fold(0usize, usize::saturating_add);
            inner.saturating_add(1)
        }
        _ => 1,
    }
}

/// Compute structural page, layer, and object counts for a [`vectorcraft_doc::Document`].
#[must_use]
pub fn document_metrics(doc: &VcDocument) -> CdrImportMetrics {
    let page_count = doc.artboards.len();
    let layer_count = doc.layers.len();
    let object_count = doc.layers.iter().map(|l| count_vc_node_objects(l)).fold(0usize, usize::saturating_add);
    CdrImportMetrics { page_count, layer_count, object_count }
}

/// Per-page CorelDRAW metadata stored under `Document::unknown["coralcraft"]`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoralCraftPageMeta {
    pub id: PageId,
    pub name: String,
    pub size: CdrSize,
    pub bleed: F64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<FillStyle>,
    #[serde(default, skip_serializing_if = "Residual::is_empty")]
    pub residual: Residual,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// Per-layer CorelDRAW metadata keyed by `vectorcraft_doc::NodeId` (`u64`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoralCraftLayerMeta {
    pub layer_id: LayerId,
    /// `Some(page_idx)` for a per-page layer, `None` for a document-wide master/desktop/guide/grid layer.
    pub page_index: Option<usize>,
    pub role: LayerRole,
    #[serde(default)]
    pub master_in_front: bool,
    #[serde(default, skip_serializing_if = "Residual::is_empty")]
    pub residual: Residual,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// Per-object lossless CorelDRAW snapshot keyed by `vectorcraft_doc::NodeId` (`u64`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoralCraftObjectMeta {
    /// Original CorelDRAW `ObjectNode` snapshot prior to any interactive edits in VectorCraft.
    pub original: ObjectNode,
    /// Page index (`Some(i)`) whose artboard offset was applied to this node, or `None` for local/master coordinates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_index: Option<usize>,
}

/// Top-level CorelDRAW metadata and lossless residuals stored in `Document::unknown["coralcraft"]`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoralCraftDocData {
    pub schema: SchemaVersion,
    pub doc_id: u64,
    pub next_id: u64,
    pub units: CdrUnit,
    pub palette: Palette,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pages: Vec<CoralCraftPageMeta>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub master_layers: Vec<CdrLayer>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub layer_meta: BTreeMap<u64, CoralCraftLayerMeta>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub object_meta: BTreeMap<u64, CoralCraftObjectMeta>,
    #[serde(default, skip_serializing_if = "Residual::is_empty")]
    pub residual: Residual,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// Extracts [`CoralCraftDocData`] from `doc.unknown["coralcraft"]`, if present.
#[must_use]
pub fn read_coralcraft_data(doc: &VcDocument) -> Option<CoralCraftDocData> {
    let val = doc.unknown.get(CORALCRAFT_UNKNOWN_KEY)?;
    serde_json::from_value(val.clone()).ok()
}

/// Stores [`CoralCraftDocData`] into `doc.unknown["coralcraft"]`.
pub fn write_coralcraft_data(doc: &mut VcDocument, data: &CoralCraftDocData) -> Result<(), CorelImportError> {
    let val = serde_json::to_value(data).map_err(|e| CorelImportError::Conversion(e.to_string()))?;
    doc.unknown.insert(CORALCRAFT_UNKNOWN_KEY.to_owned(), val);
    Ok(())
}

/// Synchronizes an existing `doc.unknown["coralcraft"]` extension block with the current
/// [`vectorcraft_doc::Document`] state (artboard names/dimensions, units, and master layers)
/// while preserving CorelDRAW-only data (per-page bleed/background/residuals, master layers,
/// object/layer/document residuals, and palette). Does nothing if `doc.unknown["coralcraft"]`
/// is absent so pure VectorCraft documents remain untouched.
pub fn sync_coralcraft_extension(doc: &mut VcDocument) -> Result<(), CorelImportError> {
    let Some(mut data) = read_coralcraft_data(doc) else {
        return Ok(());
    };
    data.units = map_unit_to_cdr(doc.units);
    let default_bleed = F64::new(doc.setup.bleed.first().copied().unwrap_or(0.0).max(0.0)).unwrap_or(F64::ZERO);
    let old_pages = std::mem::take(&mut data.pages);
    let mut new_pages = Vec::with_capacity(doc.artboards.len());
    for (idx, ab) in doc.artboards.iter().enumerate() {
        let w = ab.rect.width().max(1.0);
        let h = ab.rect.height().max(1.0);
        let size = CdrSize::new(w, h).unwrap_or_default();
        if let Some(prev) = old_pages.get(idx) {
            let p_size = if (prev.size.width.get() - w).abs() < 1e-6 && (prev.size.height.get() - h).abs() < 1e-6 { prev.size } else { size };
            new_pages.push(CoralCraftPageMeta {
                id: prev.id,
                name: ab.name.clone(),
                size: p_size,
                bleed: prev.bleed,
                background: prev.background.clone(),
                residual: prev.residual.clone(),
                extra: prev.extra.clone(),
            });
        } else {
            new_pages.push(CoralCraftPageMeta {
                id: PageId(u64::from(ab.id.max(1))),
                name: ab.name.clone(),
                size,
                bleed: default_bleed,
                background: Some(FillStyle::uniform(CdrColor::WHITE)),
                residual: Residual::default(),
                extra: BTreeMap::new(),
            });
        }
    }
    data.pages = new_pages;
    write_coralcraft_data(doc, &data)
}

/// Parses `.cdr` or `.cmx` bytes and maps the resulting document onto a [`vectorcraft_doc::Document`].
pub fn import_cdr_bytes(bytes: &[u8]) -> Result<CorelImportOutcome, CorelImportError> {
    let parsed = coralcraft_cdr::parse_document(bytes)?;
    let (document, conv_warnings) = import_document(&parsed.document)?;
    let mut warnings: Vec<String> = parsed.probe_report.warnings.iter().map(|w| format!("{w:?}")).collect();
    warnings.extend(parsed.parse_warnings);
    warnings.extend(conv_warnings);
    Ok(CorelImportOutcome { document, report: parsed.probe_report, warnings })
}

/// Encodes a [`vectorcraft_doc::Document`] as a CorelDRAW v7 RIFF `.cdr` byte stream.
pub fn write_cdr7_document(doc: &VcDocument) -> Result<Vec<u8>, CorelImportError> {
    let cdr_doc = export_document(doc)?;
    Ok(coralcraft_cdr::write_cdr7(&cdr_doc)?)
}

/// Encodes a [`vectorcraft_doc::Document`] as a CorelDRAW X4+ ZIP+RIFF `.cdr` byte stream.
pub fn write_cdrx_zip_document(doc: &VcDocument) -> Result<Vec<u8>, CorelImportError> {
    let cdr_doc = export_document(doc)?;
    Ok(coralcraft_cdr::write_cdrx_zip(&cdr_doc)?)
}

/// Encodes a [`vectorcraft_doc::Document`] as a Corel Presentation Exchange `.cmx` (`RIFF CMX1`) byte stream.
pub fn write_cmx1_document(doc: &VcDocument) -> Result<Vec<u8>, CorelImportError> {
    let cdr_doc = export_document(doc)?;
    Ok(coralcraft_cdr::write_cmx1(&cdr_doc)?)
}

/// Maps a [`coralcraft_doc::Document`] onto a [`vectorcraft_doc::Document`], preserving lossless residuals
/// in `Document::unknown["coralcraft"]`.
pub fn import_document(cdr_doc: &CdrDocument) -> Result<(VcDocument, Vec<String>), CorelImportError> {
    let mut warnings = Vec::new();
    let mut doc = VcDocument::new(595.275_590_551_181_2, 841.889_763_779_527_6);
    doc.title.clone_from(&cdr_doc.title);
    doc.units = map_unit_to_vc(cdr_doc.units);
    doc.color_mode = infer_color_mode(cdr_doc);

    // Populate swatches: start from VectorCraft defaults for the color mode, then append custom Corel swatches.
    let (mut swatches, groups) = default_swatches(doc.color_mode.model());
    for sw in &cdr_doc.palette.swatches {
        if !swatches.iter().any(|existing| existing.name == sw.name) {
            swatches.push(map_palette_swatch_to_vc(sw));
        }
    }
    doc.swatches = swatches.into();
    doc.swatch_groups = groups.into();

    if let Some(first_page) = cdr_doc.pages.first() {
        let b = first_page.bleed.get().max(0.0);
        doc.setup.bleed = [b, b, b, b];
    }

    // Build artboards horizontally with PAGE_GAP_PT spacing.
    doc.artboards.clear();
    let mut page_offsets: Vec<Vec2> = Vec::with_capacity(cdr_doc.pages.len());
    let mut cursor_x = 0.0_f64;
    let mut page_metas = Vec::with_capacity(cdr_doc.pages.len());

    for (idx, page) in cdr_doc.pages.iter().enumerate() {
        let w = page.size.width.get().max(1.0);
        let h = page.size.height.get().max(1.0);
        let rect = VcRect::new(cursor_x, 0.0, cursor_x + w, h);
        let ab_id = u32::try_from(idx.saturating_add(1)).unwrap_or(u32::MAX);
        doc.artboards.push(Artboard { id: ab_id, name: page.name.clone(), rect, show_center_mark: false, show_cross_hairs: false });
        page_offsets.push(Vec2::new(cursor_x, 0.0));
        cursor_x += w + PAGE_GAP_PT;

        page_metas.push(CoralCraftPageMeta {
            id: page.id,
            name: page.name.clone(),
            size: page.size,
            bleed: page.bleed,
            background: page.background.clone(),
            residual: page.residual.clone(),
            extra: page.extra.clone(),
        });
    }

    if doc.artboards.is_empty() {
        doc.artboards.push(Artboard {
            id: 1,
            name: "Page 1".to_owned(),
            rect: VcRect::new(0.0, 0.0, 595.275_590_551_181_2, 841.889_763_779_527_6),
            show_center_mark: false,
            show_cross_hairs: false,
        });
        page_offsets.push(Vec2::ZERO);
    }

    let mut layer_meta_map: BTreeMap<u64, CoralCraftLayerMeta> = BTreeMap::new();
    let mut object_meta_map: BTreeMap<u64, CoralCraftObjectMeta> = BTreeMap::new();
    let mut vc_layers: Vec<Arc<Node>> = Vec::new();
    let mut used_node_ids = std::collections::BTreeSet::new();

    let alloc_unique_id = |preferred: u64, used: &mut std::collections::BTreeSet<u64>, doc_ref: &mut VcDocument| -> NodeId {
        if preferred > 0 && used.insert(preferred) {
            let _ = doc_ref.alloc_id();
            NodeId(preferred)
        } else {
            loop {
                let candidate = doc_ref.alloc_id().0;
                if used.insert(candidate) {
                    return NodeId(candidate);
                }
            }
        }
    };

    // Convert background master layers that carry objects first so they sit behind page layers on canvas.
    for (m_idx, m_layer) in cdr_doc.master_layers.iter().enumerate() {
        if !m_layer.master_in_front
            && !matches!(m_layer.role, LayerRole::Desktop | LayerRole::Guides | LayerRole::Grid)
            && !m_layer.objects.is_empty()
        {
            let lid = alloc_unique_id(m_layer.id.0, &mut used_node_ids, &mut doc);
            let preset_idx = u8::try_from(m_idx % 27).unwrap_or(0);
            let mut lnode = Node::layer(lid, &m_layer.name, LayerColor::Preset(preset_idx));
            lnode.visible = m_layer.visible;
            lnode.locked = m_layer.locked;
            lnode.opacity = m_layer.opacity.get().clamp(0.0, 1.0) as f32;
            if let NodeKind::Layer { printable, children, .. } = &mut lnode.kind {
                *printable = m_layer.printable;
                for obj in &m_layer.objects {
                    let cnode = convert_cdr_node_to_vc(obj, Vec2::ZERO, None, &mut doc, &mut used_node_ids, &mut object_meta_map, &mut warnings);
                    children.push(Arc::new(cnode));
                }
            }
            layer_meta_map.insert(
                lid.0,
                CoralCraftLayerMeta {
                    layer_id: m_layer.id,
                    page_index: None,
                    role: m_layer.role,
                    master_in_front: m_layer.master_in_front,
                    residual: m_layer.residual.clone(),
                    extra: m_layer.extra.clone(),
                },
            );
            vc_layers.push(Arc::new(lnode));
        }
    }

    // Convert per-page layers in page order.
    for (p_idx, page) in cdr_doc.pages.iter().enumerate() {
        let offset = page_offsets.get(p_idx).copied().unwrap_or(Vec2::ZERO);
        for (l_idx, layer) in page.layers.iter().enumerate() {
            let lid = alloc_unique_id(layer.id.0, &mut used_node_ids, &mut doc);
            let color_idx = u8::try_from(p_idx.saturating_add(l_idx) % 27).unwrap_or(0);
            let mut lnode = Node::layer(lid, &layer.name, LayerColor::Preset(color_idx));
            lnode.visible = layer.visible;
            lnode.locked = layer.locked;
            lnode.opacity = layer.opacity.get().clamp(0.0, 1.0) as f32;
            if let NodeKind::Layer { printable, children, .. } = &mut lnode.kind {
                *printable = layer.printable;
                for obj in &layer.objects {
                    let cnode = convert_cdr_node_to_vc(obj, offset, Some(p_idx), &mut doc, &mut used_node_ids, &mut object_meta_map, &mut warnings);
                    children.push(Arc::new(cnode));
                }
            }
            layer_meta_map.insert(
                lid.0,
                CoralCraftLayerMeta {
                    layer_id: layer.id,
                    page_index: Some(p_idx),
                    role: layer.role,
                    master_in_front: layer.master_in_front,
                    residual: layer.residual.clone(),
                    extra: layer.extra.clone(),
                },
            );
            vc_layers.push(Arc::new(lnode));
        }
    }

    // Convert foreground master / desktop layers that carry objects.
    for (m_idx, m_layer) in cdr_doc.master_layers.iter().enumerate() {
        let is_fg_or_desktop = m_layer.master_in_front || matches!(m_layer.role, LayerRole::Desktop | LayerRole::Guides | LayerRole::Grid);
        if is_fg_or_desktop && !m_layer.objects.is_empty() {
            let lid = alloc_unique_id(m_layer.id.0, &mut used_node_ids, &mut doc);
            let preset_idx = u8::try_from(m_idx % 27).unwrap_or(0);
            let mut lnode = Node::layer(lid, &m_layer.name, LayerColor::Preset(preset_idx));
            lnode.visible = m_layer.visible;
            lnode.locked = m_layer.locked;
            lnode.opacity = m_layer.opacity.get().clamp(0.0, 1.0) as f32;
            if let NodeKind::Layer { printable, children, .. } = &mut lnode.kind {
                *printable = m_layer.printable;
                for obj in &m_layer.objects {
                    let cnode = convert_cdr_node_to_vc(obj, Vec2::ZERO, None, &mut doc, &mut used_node_ids, &mut object_meta_map, &mut warnings);
                    children.push(Arc::new(cnode));
                }
            }
            layer_meta_map.insert(
                lid.0,
                CoralCraftLayerMeta {
                    layer_id: m_layer.id,
                    page_index: None,
                    role: m_layer.role,
                    master_in_front: m_layer.master_in_front,
                    residual: m_layer.residual.clone(),
                    extra: m_layer.extra.clone(),
                },
            );
            vc_layers.push(Arc::new(lnode));
        }
    }

    if vc_layers.is_empty() {
        let lid = doc.alloc_id();
        vc_layers.push(Arc::new(Node::layer(lid, "Layer 1", LayerColor::Preset(0))));
    }
    doc.layers = vc_layers;
    doc.fix_next_id();

    let cc_data = CoralCraftDocData {
        schema: cdr_doc.schema,
        doc_id: cdr_doc.id,
        next_id: cdr_doc.next_id,
        units: cdr_doc.units,
        palette: cdr_doc.palette.clone(),
        pages: page_metas,
        master_layers: cdr_doc.master_layers.clone(),
        layer_meta: layer_meta_map,
        object_meta: object_meta_map,
        residual: cdr_doc.residual.clone(),
        extra: cdr_doc.extra.clone(),
    };
    write_coralcraft_data(&mut doc, &cc_data)?;

    Ok((doc, warnings))
}

/// Maps a [`vectorcraft_doc::Document`] back to a [`coralcraft_doc::Document`], restoring any lossless
/// CorelDRAW metadata and residuals stored in `Document::unknown["coralcraft"]`.
pub fn export_document(doc: &VcDocument) -> Result<CdrDocument, CorelImportError> {
    let maybe_meta = read_coralcraft_data(doc);
    let mut cdr_doc = CdrDocument::new_a4_portrait();
    cdr_doc.title.clone_from(&doc.title);
    cdr_doc.units = map_unit_to_cdr(doc.units);

    if let Some(meta) = &maybe_meta {
        cdr_doc.schema = meta.schema;
        cdr_doc.id = meta.doc_id;
        cdr_doc.next_id = meta.next_id;
        cdr_doc.units = meta.units;
        cdr_doc.palette = meta.palette.clone();
        cdr_doc.residual = meta.residual.clone();
        cdr_doc.extra = meta.extra.clone();
    } else {
        // Synthesize Corel palette swatches from non-reserved VectorCraft swatches
        let mut custom_swatches = Vec::new();
        for sw in doc.swatches.iter() {
            if let Some(ps) = map_vc_swatch_to_palette(sw) {
                custom_swatches.push(ps);
            }
        }
        if !custom_swatches.is_empty() {
            cdr_doc.palette = Palette { name: "Document Palette".to_owned(), guid: None, swatches: custom_swatches };
        }
    }

    // Build Pages from doc.artboards (merged with CoralCraftPageMeta when available).
    cdr_doc.pages.clear();
    let mut artboard_offsets: Vec<Vec2> = Vec::with_capacity(doc.artboards.len().max(1));
    let bleed_pt = F64::new(doc.setup.bleed[0].max(0.0)).unwrap_or(F64::ZERO);

    if doc.artboards.is_empty() {
        let default_p = CdrDocument::new_a4_portrait();
        if let Some(p0) = default_p.pages.into_iter().next() {
            let mut p = p0;
            p.layers.clear();
            cdr_doc.pages.push(p);
        }
        artboard_offsets.push(Vec2::ZERO);
    } else {
        for (idx, ab) in doc.artboards.iter().enumerate() {
            let w = ab.rect.width().max(1.0);
            let h = ab.rect.height().max(1.0);
            let size = CdrSize::new(w, h).unwrap_or_default();
            artboard_offsets.push(Vec2::new(ab.rect.x0, ab.rect.y0));
            if let Some(pmeta) = maybe_meta.as_ref().and_then(|m| m.pages.get(idx)) {
                let p_size = if (pmeta.size.width.get() - w).abs() < 1e-6 && (pmeta.size.height.get() - h).abs() < 1e-6 { pmeta.size } else { size };
                cdr_doc.pages.push(CdrPage {
                    id: pmeta.id,
                    name: ab.name.clone(),
                    size: p_size,
                    bleed: pmeta.bleed,
                    background: pmeta.background.clone(),
                    layers: Vec::new(),
                    residual: pmeta.residual.clone(),
                    extra: pmeta.extra.clone(),
                });
            } else {
                cdr_doc.pages.push(CdrPage {
                    id: PageId(u64::from(ab.id.max(1))),
                    name: ab.name.clone(),
                    size,
                    bleed: bleed_pt,
                    background: Some(FillStyle::uniform(CdrColor::WHITE)),
                    layers: Vec::new(),
                    residual: Residual::default(),
                    extra: BTreeMap::new(),
                });
            }
        }
    }

    // Reconstruct layers and objects.
    if let Some(meta) = &maybe_meta {
        cdr_doc.master_layers = meta.master_layers.clone();
        // Update any master layers that were represented as active layer nodes in doc.layers
        for lnode in &doc.layers {
            if let Some(lmeta) = meta.layer_meta.get(&lnode.id.0) {
                match lmeta.page_index {
                    Some(p_idx) => {
                        let offset = artboard_offsets.get(p_idx).copied().unwrap_or(Vec2::ZERO);
                        let clayer = convert_vc_layer_to_cdr(lnode, Some(lmeta), offset, doc, Some(meta));
                        if let Some(page) = cdr_doc.pages.get_mut(p_idx) {
                            page.layers.push(clayer);
                        }
                    }
                    None => {
                        let clayer = convert_vc_layer_to_cdr(lnode, Some(lmeta), Vec2::ZERO, doc, Some(meta));
                        if let Some(existing) = cdr_doc.master_layers.iter_mut().find(|ml| ml.id == lmeta.layer_id) {
                            *existing = clayer;
                        } else {
                            cdr_doc.master_layers.push(clayer);
                        }
                    }
                }
            } else {
                // Newly added layer in VectorCraft: place on Page 0 (or partition by artboard)
                let offset = artboard_offsets.first().copied().unwrap_or(Vec2::ZERO);
                let clayer = convert_vc_layer_to_cdr(lnode, None, offset, doc, Some(meta));
                if let Some(page0) = cdr_doc.pages.first_mut() {
                    page0.layers.push(clayer);
                }
            }
        }
    } else {
        // Pure VectorCraft document without prior CorelDRAW metadata:
        // If single artboard, map each doc.layers entry directly onto Page 0.
        // If multi-artboard, create matching layers on each page and assign objects to the artboard containing their center.
        if cdr_doc.pages.len() <= 1 {
            let offset = artboard_offsets.first().copied().unwrap_or(Vec2::ZERO);
            if let Some(page0) = cdr_doc.pages.first_mut() {
                for lnode in &doc.layers {
                    page0.layers.push(convert_vc_layer_to_cdr(lnode, None, offset, doc, None));
                }
            }
        } else {
            for (p_idx, page) in cdr_doc.pages.iter_mut().enumerate() {
                let offset = artboard_offsets.get(p_idx).copied().unwrap_or(Vec2::ZERO);
                for (l_idx, lnode) in doc.layers.iter().enumerate() {
                    let lid = LayerId(lnode.id.0.saturating_mul(100).saturating_add(p_idx as u64).saturating_add(l_idx as u64).max(1));
                    let lname = lnode.name.clone().unwrap_or_else(|| format!("Layer {}", l_idx.saturating_add(1)));
                    let mut clayer = CdrLayer::new_normal(lid, lname);
                    clayer.visible = lnode.visible;
                    clayer.locked = lnode.locked;
                    clayer.opacity = F64::new(f64::from(lnode.opacity.clamp(0.0, 1.0))).unwrap_or(F64::ONE);
                    if let NodeKind::Layer { printable, children, .. } = &lnode.kind {
                        clayer.printable = *printable;
                        for child in children {
                            let target_ab = best_artboard_for_node(child, &doc.artboards);
                            if target_ab == p_idx {
                                clayer.objects.push(convert_vc_node_to_cdr(child, offset, doc, None));
                            }
                        }
                    }
                    page.layers.push(clayer);
                }
            }
        }
    }

    for (idx, page) in cdr_doc.pages.iter_mut().enumerate() {
        if page.layers.is_empty() {
            let lid = LayerId((idx as u64).saturating_add(1000));
            page.layers.push(CdrLayer::new_normal(lid, "Layer 1"));
        }
    }

    let _ = cdr_doc.validate();
    Ok(cdr_doc)
}

fn best_artboard_for_node(node: &Node, artboards: &[Artboard]) -> usize {
    let Some(bounds) = node.geometric_bounds() else { return 0 };
    let center = bounds.center();
    for (idx, ab) in artboards.iter().enumerate() {
        if ab.rect.contains(center) {
            return idx;
        }
    }
    0
}

fn map_unit_to_vc(u: CdrUnit) -> VcUnit {
    match u {
        CdrUnit::Pt | CdrUnit::CdrTick => VcUnit::Points,
        CdrUnit::Mm => VcUnit::Millimeters,
        CdrUnit::Inch => VcUnit::Inches,
        CdrUnit::Px => VcUnit::Pixels,
    }
}

fn map_unit_to_cdr(u: VcUnit) -> CdrUnit {
    match u {
        VcUnit::Points | VcUnit::Picas => CdrUnit::Pt,
        VcUnit::Millimeters | VcUnit::Centimeters | VcUnit::Meters => CdrUnit::Mm,
        VcUnit::Inches | VcUnit::Feet | VcUnit::FeetInches | VcUnit::Yards => CdrUnit::Inch,
        VcUnit::Pixels => CdrUnit::Px,
    }
}

fn infer_color_mode(cdr_doc: &CdrDocument) -> ColorMode {
    for page in &cdr_doc.pages {
        for layer in &page.layers {
            for obj in &layer.objects {
                if let Some(FillStyle::Uniform { color: CdrColor::Cmyk { .. } | CdrColor::Spot { .. } | CdrColor::Registration }) = &obj.fill {
                    return ColorMode::Cmyk;
                }
                if let Some(OutlineStyle { color: CdrColor::Cmyk { .. } | CdrColor::Spot { .. } | CdrColor::Registration, .. }) = &obj.outline {
                    return ColorMode::Cmyk;
                }
            }
        }
    }
    ColorMode::Rgb
}

fn map_cdr_color_to_vc(c: &CdrColor) -> VcColor {
    match c {
        CdrColor::Rgb { r, g, b } => VcColor::rgb(f32::from(*r) / 255.0, f32::from(*g) / 255.0, f32::from(*b) / 255.0),
        CdrColor::Cmyk { c, m, y, k } => VcColor::cmyk(c.get() as f32, m.get() as f32, y.get() as f32, k.get() as f32),
        CdrColor::Gray { k } => VcColor::gray(k.get() as f32),
        CdrColor::LabD50 { l, a, b } => VcColor::lab(l.get() as f32, a.get() as f32, b.get() as f32),
        CdrColor::LabD65 { l, a, b } => {
            let [l50, a50, b50] = coralcraft_color::lab_d65_to_d50(l.get(), a.get(), b.get());
            VcColor::lab(l50 as f32, a50 as f32, b50 as f32)
        }
        CdrColor::Hsb { h, s, b } => VcColor::from_hsb(h.get() as f32, s.get() as f32, b.get() as f32),
        CdrColor::Spot { tint, fallback_cmyk, .. } => {
            VcColor::cmyk(fallback_cmyk[0].get() as f32, fallback_cmyk[1].get() as f32, fallback_cmyk[2].get() as f32, fallback_cmyk[3].get() as f32)
                .tinted(tint.get() as f32)
        }
        CdrColor::Registration => VcColor::cmyk(1.0, 1.0, 1.0, 1.0),
    }
}

fn map_cdr_color_to_paint(c: &CdrColor) -> Paint {
    match c {
        CdrColor::Registration => Paint::registration(),
        CdrColor::Spot { name, tint, fallback_cmyk } => {
            let base = VcColor::cmyk(
                fallback_cmyk[0].get() as f32,
                fallback_cmyk[1].get() as f32,
                fallback_cmyk[2].get() as f32,
                fallback_cmyk[3].get() as f32,
            );
            let t = (tint.get() as f32).clamp(0.0, 1.0);
            Paint::Solid { color: base.tinted(t), swatch: Some(name.clone()), tint: t }
        }
        other => Paint::solid(map_cdr_color_to_vc(other)),
    }
}

fn map_vc_color_to_cdr(c: &VcColor) -> CdrColor {
    match *c {
        VcColor::Rgb { r, g, b } => CdrColor::rgb8(
            (r.clamp(0.0, 1.0) * 255.0).round() as u8,
            (g.clamp(0.0, 1.0) * 255.0).round() as u8,
            (b.clamp(0.0, 1.0) * 255.0).round() as u8,
        ),
        VcColor::Cmyk { c, m, y, k } => CdrColor::cmyk(f64::from(c), f64::from(m), f64::from(y), f64::from(k)).unwrap_or(CdrColor::BLACK),
        VcColor::Gray { k } => CdrColor::gray(f64::from(k)).unwrap_or(CdrColor::BLACK),
        VcColor::Lab { l, a, b } => CdrColor::lab_d50(f64::from(l), f64::from(a), f64::from(b)).unwrap_or(CdrColor::BLACK),
    }
}

fn map_vc_paint_to_cdr_color(p: &Paint) -> Option<CdrColor> {
    match p {
        Paint::None => None,
        Paint::Solid { swatch: Some(sw), .. } if sw == REGISTRATION => Some(CdrColor::Registration),
        Paint::Solid { color, swatch: Some(sw), tint } if *tint < 1.0 => {
            let [c, m, y, k] = color.to_cmyk();
            CdrColor::spot(sw.clone(), f64::from(*tint), [f64::from(c), f64::from(m), f64::from(y), f64::from(k)])
                .ok()
                .or_else(|| Some(map_vc_color_to_cdr(color)))
        }
        Paint::Solid { color, .. } => Some(map_vc_color_to_cdr(color)),
        Paint::Gradient(gp) => gp.gradient.stops.first().map(|s| map_vc_color_to_cdr(&s.color)),
        Paint::Pattern { .. } => Some(CdrColor::BLACK),
    }
}

fn map_palette_swatch_to_vc(sw: &PaletteSwatch) -> Swatch {
    match &sw.color {
        CdrColor::Registration => vectorcraft_color::swatch::registration().clone(),
        CdrColor::Spot { fallback_cmyk, .. } => {
            let base = VcColor::cmyk(
                fallback_cmyk[0].get() as f32,
                fallback_cmyk[1].get() as f32,
                fallback_cmyk[2].get() as f32,
                fallback_cmyk[3].get() as f32,
            );
            Swatch { name: sw.name.clone(), paint: Paint::solid(base), global: true, spot: true }
        }
        other => Swatch { name: sw.name.clone(), paint: Paint::solid(map_cdr_color_to_vc(other)), global: false, spot: false },
    }
}

fn map_vc_swatch_to_palette(sw: &Swatch) -> Option<PaletteSwatch> {
    if sw.paint.is_none() {
        return None;
    }
    if sw.name == REGISTRATION {
        return Some(PaletteSwatch { name: "Registration".to_owned(), color: CdrColor::Registration });
    }
    let color = match &sw.paint {
        Paint::Solid { color, tint, .. } if sw.spot => {
            let [c, m, y, k] = color.to_cmyk();
            CdrColor::spot(sw.name.clone(), f64::from(*tint), [f64::from(c), f64::from(m), f64::from(y), f64::from(k)]).ok()?
        }
        Paint::Solid { color, .. } => map_vc_color_to_cdr(color),
        _ => return None,
    };
    Some(PaletteSwatch { name: sw.name.clone(), color })
}

#[allow(clippy::arithmetic_side_effects)]
fn affine_mul(a: Affine, b: Affine) -> Affine {
    a * b
}

#[allow(clippy::arithmetic_side_effects)]
fn affine_apply_point(a: Affine, p: VcPoint) -> VcPoint {
    a * p
}

fn map_affine_to_vc(a: Affine2, offset: Vec2) -> Affine {
    affine_mul(Affine::translate(offset), Affine::new([a.a.get(), a.b.get(), a.c.get(), a.d.get(), a.tx.get(), a.ty.get()]))
}

fn map_cdr_corner_kind_to_vc(k: CdrCornerKind) -> VcCornerKind {
    match k {
        CdrCornerKind::Round => VcCornerKind::Round,
        CdrCornerKind::Scalloped => VcCornerKind::InvertedRound,
        CdrCornerKind::Chamfered => VcCornerKind::Chamfer,
    }
}

fn map_vc_corner_kind_to_cdr(k: VcCornerKind) -> CdrCornerKind {
    match k {
        VcCornerKind::Round => CdrCornerKind::Round,
        VcCornerKind::InvertedRound => CdrCornerKind::Scalloped,
        VcCornerKind::Chamfer => CdrCornerKind::Chamfered,
    }
}

fn map_cdr_path_to_vc(path: &CdrPath, xf: Affine) -> (PathData, VcFillRule) {
    let mut bp = BezPath::new();
    for el in &path.elements {
        match *el {
            PathElement::MoveTo(p) => bp.move_to(VcPoint::new(p.x.get(), p.y.get())),
            PathElement::LineTo(p) => bp.line_to(VcPoint::new(p.x.get(), p.y.get())),
            PathElement::QuadTo { ctrl, end } => bp.quad_to(VcPoint::new(ctrl.x.get(), ctrl.y.get()), VcPoint::new(end.x.get(), end.y.get())),
            PathElement::CubicTo { c1, c2, end } => {
                bp.curve_to(VcPoint::new(c1.x.get(), c1.y.get()), VcPoint::new(c2.x.get(), c2.y.get()), VcPoint::new(end.x.get(), end.y.get()))
            }
            PathElement::Close => bp.close_path(),
        }
    }
    let mut pd = PathData::from_bezpath(&bp);
    if xf != Affine::IDENTITY {
        pd.transform(xf);
    }
    let rule = match path.fill_rule {
        CdrFillRule::NonZero => VcFillRule::NonZero,
        CdrFillRule::EvenOdd => VcFillRule::EvenOdd,
    };
    (pd, rule)
}

fn map_vc_path_to_cdr(path: &PathData, rule: VcFillRule, offset: Vec2) -> CdrPath {
    let mut bp = path.to_bezpath();
    if offset != Vec2::ZERO {
        bp.apply_affine(Affine::translate(Vec2::new(-offset.x, -offset.y)));
    }
    let mut cpath = CdrPath::new();
    cpath.fill_rule = match rule {
        VcFillRule::NonZero => CdrFillRule::NonZero,
        VcFillRule::EvenOdd => CdrFillRule::EvenOdd,
    };
    for el in bp.elements() {
        match *el {
            PathEl::MoveTo(p) => {
                if let Ok(pt) = CdrPoint::new(p.x, p.y) {
                    let _ = cpath.move_to(pt);
                }
            }
            PathEl::LineTo(p) => {
                if let Ok(pt) = CdrPoint::new(p.x, p.y) {
                    let _ = cpath.line_to(pt);
                }
            }
            PathEl::QuadTo(c, p) => {
                if let (Ok(ctrl), Ok(to)) = (CdrPoint::new(c.x, c.y), CdrPoint::new(p.x, p.y)) {
                    let _ = cpath.quad_to(ctrl, to);
                }
            }
            PathEl::CurveTo(c1, c2, p) => {
                if let (Ok(p1), Ok(p2), Ok(to)) = (CdrPoint::new(c1.x, c1.y), CdrPoint::new(c2.x, c2.y), CdrPoint::new(p.x, p.y)) {
                    let _ = cpath.cubic_to(p1, p2, to);
                }
            }
            PathEl::ClosePath => {
                let _ = cpath.close();
            }
        }
    }
    cpath
}

fn map_cdr_fill_to_paint(fill: &FillStyle, bounds: Option<VcRect>) -> Paint {
    match fill {
        FillStyle::Uniform { color } => map_cdr_color_to_paint(color),
        FillStyle::Fountain { geometry, stops, .. } => {
            let mut vc_stops: Vec<GradientStop> = stops
                .iter()
                .map(|st| {
                    let mut gs = GradientStop::new(st.offset.get() as f32, map_cdr_color_to_vc(&st.color));
                    gs.midpoint = (st.midpoint.get() as f32).clamp(0.13, 0.87);
                    gs
                })
                .collect();
            if vc_stops.len() < 2 {
                let c = vc_stops.first().map_or(VcColor::WHITE, |s| s.color);
                vc_stops = vec![GradientStop::new(0.0, c), GradientStop::new(1.0, c)];
            }
            let (kind, angle, geom) = match *geometry {
                FountainGeometry::Linear { angle_deg, .. } => (GradientKind::Linear, angle_deg.get(), None),
                FountainGeometry::Radial { center_x, center_y, radius, focal_x, focal_y } => {
                    let g = bounds.map(|b| {
                        let start = VcPoint::new(b.x0 + b.width() * center_x.get(), b.y0 + b.height() * center_y.get());
                        let r = b.width().max(b.height()) * radius.get().max(0.05);
                        let end = VcPoint::new(start.x + r, start.y);
                        let focal = VcPoint::new(b.x0 + b.width() * focal_x.get(), b.y0 + b.height() * focal_y.get());
                        let mut gg = GradientGeom { start, end, aspect: 1.0, focal: None };
                        if (focal.x - start.x).hypot(focal.y - start.y) > 1e-4 {
                            gg.set_focal(Some(focal));
                        }
                        gg
                    });
                    (GradientKind::Radial, 0.0, g)
                }
                FountainGeometry::Conical { angle_deg, .. } => (GradientKind::Radial, angle_deg.get(), None),
                FountainGeometry::Square { angle_deg, .. } => (GradientKind::Radial, angle_deg.get(), None),
            };
            let mut gp = GradientPaint::new(Gradient { kind, stops: vc_stops });
            gp.angle = angle;
            gp.geom = geom;
            Paint::Gradient(Box::new(gp))
        }
        FillStyle::TwoColorPattern { fg_color, bg_color, .. } => {
            // Represent two-color pattern as a 2-stop gradient or solid blend for visual fidelity;
            // exact TwoColorPattern is preserved in CoralCraftObjectMeta.
            let c = map_cdr_color_to_vc(fg_color).lerp(&map_cdr_color_to_vc(bg_color), 0.25);
            Paint::solid(c)
        }
        FillStyle::Unknown { .. } => Paint::solid(VcColor::gray(0.5)),
    }
}

fn map_cdr_outline_to_stroke(outl: &OutlineStyle) -> StrokeLayer {
    let width = if outl.is_hairline() { HAIRLINE_WIDTH_PT } else { outl.width_pt.get().max(HAIRLINE_WIDTH_PT) };
    let mut sl = StrokeLayer::new(map_cdr_color_to_paint(&outl.color), width);
    sl.cap = match outl.cap {
        CdrLineCap::Butt => VcLineCap::Butt,
        CdrLineCap::Round => VcLineCap::Round,
        CdrLineCap::Square => VcLineCap::Square,
    };
    sl.join = match outl.join {
        CdrLineJoin::Miter => VcLineJoin::Miter,
        CdrLineJoin::Round => VcLineJoin::Round,
        CdrLineJoin::Bevel => VcLineJoin::Bevel,
    };
    sl.miter_limit = outl.miter_limit.get().max(1.0);
    if !outl.dash_pattern.is_empty() {
        sl.dash = Some(Dash { pattern: outl.dash_pattern.iter().map(|d| d.get().max(0.1)).collect(), offset: 0.0, align_corners: false });
    }
    sl.start_arrow = map_cdr_arrow_to_vc(outl.start_arrow);
    sl.end_arrow = map_cdr_arrow_to_vc(outl.end_arrow);
    sl
}

fn map_cdr_arrow_to_vc(a: ArrowheadKind) -> Option<Arrowhead> {
    match a {
        ArrowheadKind::None => None,
        ArrowheadKind::Triangle => Some(Arrowhead::Triangle),
        ArrowheadKind::OpenArrow => Some(Arrowhead::ArrowOpen),
        ArrowheadKind::Circle => Some(Arrowhead::Circle),
        ArrowheadKind::Diamond => Some(Arrowhead::Diamond),
        ArrowheadKind::Unknown => Some(Arrowhead::Arrow),
    }
}

fn map_vc_arrow_to_cdr(a: Option<Arrowhead>) -> ArrowheadKind {
    match a {
        None => ArrowheadKind::None,
        Some(Arrowhead::Triangle | Arrowhead::TriangleReverse | Arrowhead::TriangleBar) => ArrowheadKind::Triangle,
        Some(Arrowhead::ArrowOpen | Arrowhead::TriangleOpen | Arrowhead::Chevron) => ArrowheadKind::OpenArrow,
        Some(Arrowhead::Circle | Arrowhead::CircleOpen | Arrowhead::Oval | Arrowhead::Target) => ArrowheadKind::Circle,
        Some(Arrowhead::Diamond | Arrowhead::DiamondOpen) => ArrowheadKind::Diamond,
        Some(_) => ArrowheadKind::Triangle,
    }
}

fn build_appearance(fill: Option<&FillStyle>, outline: Option<&OutlineStyle>, effects: &[VectorEffect], bounds: Option<VcRect>) -> Appearance {
    let mut items = Vec::with_capacity(2);
    let fill_item = fill.map(|f| AppearanceItem::Fill(FillLayer::new(map_cdr_fill_to_paint(f, bounds))));
    let stroke_item = outline.map(|o| AppearanceItem::Stroke(map_cdr_outline_to_stroke(o)));
    let behind_fill = outline.is_some_and(|o| o.behind_fill);

    if behind_fill {
        if let Some(s) = stroke_item {
            items.push(s);
        }
        if let Some(f) = fill_item {
            items.push(f);
        }
    } else {
        if let Some(f) = fill_item {
            items.push(f);
        }
        if let Some(s) = stroke_item {
            items.push(s);
        }
    }

    let mut vc_effects = Vec::new();
    for eff in effects {
        match eff {
            VectorEffect::DropShadow { offset_x, offset_y, opacity, color } => {
                vc_effects.push(Effect {
                    id: "stylize.dropShadow".to_owned(),
                    params: json!({
                        "mode": "multiply",
                        "opacity": opacity.get().clamp(0.0, 1.0) * 100.0,
                        "x": offset_x.get(),
                        "y": offset_y.get(),
                        "blur": 5.0,
                        "shade": "color",
                        "color": color.to_hex(),
                        "darkness": 100.0
                    }),
                    visible: true,
                });
            }
            VectorEffect::Contour { steps, offset_pt, inside } => {
                let signed = if *inside { -offset_pt.get() } else { offset_pt.get() };
                vc_effects.push(Effect {
                    id: "path.offsetPath".to_owned(),
                    params: json!({
                        "offset": signed,
                        "joins": "miter",
                        "miterLimit": 4.0,
                        "steps": *steps
                    }),
                    visible: true,
                });
            }
            VectorEffect::Extrude { depth_pt, angle_deg } => {
                vc_effects.push(Effect {
                    id: "threeD.extrude".to_owned(),
                    params: json!({
                        "depth": depth_pt.get(),
                        "rotationZ": angle_deg.get()
                    }),
                    visible: true,
                });
            }
            VectorEffect::Distortion { kind, amplitude, frequency } => match kind {
                DistortionKind::PushPull => {
                    vc_effects.push(Effect { id: "distort.puckerBloat".to_owned(), params: json!({ "amount": amplitude.get() }), visible: true });
                }
                DistortionKind::Zipper => {
                    vc_effects.push(Effect {
                        id: "distort.zigZag".to_owned(),
                        params: json!({
                            "size": amplitude.get().abs(),
                            "relative": false,
                            "ridges": frequency.get().round().clamp(1.0, 100.0) as i64,
                            "points": "corner"
                        }),
                        visible: true,
                    });
                }
                DistortionKind::Twister => {
                    vc_effects.push(Effect { id: "distort.twist".to_owned(), params: json!({ "angle": amplitude.get() }), visible: true });
                }
            },
            _ => {}
        }
    }

    Appearance { items, effects: vc_effects, contents_index: None }
}

fn build_fountain_opacity_mask(mask_id: NodeId, bounds: VcRect, geometry: FountainGeometry, stops: &[FountainStop]) -> OpacityMask {
    let rect = if bounds.width() > 0.0 && bounds.height() > 0.0 { bounds } else { VcRect::new(0.0, 0.0, 100.0, 100.0) };
    let path = shapes::rectangle(rect);
    let fill = FillStyle::Fountain { geometry, blend_mode: FountainBlendMode::Direct, stops: stops.to_vec() };
    let paint = map_cdr_fill_to_paint(&fill, Some(rect));
    let ap = Appearance { items: vec![AppearanceItem::Fill(FillLayer::new(paint))], effects: vec![], contents_index: None };
    let mask_node = Node::path(mask_id, path, ap);
    OpacityMask::new(mask_node, false)
}

#[allow(clippy::too_many_arguments)]
fn convert_cdr_node_to_vc(
    cdr_node: &ObjectNode,
    offset: Vec2,
    page_index: Option<usize>,
    doc: &mut VcDocument,
    used_ids: &mut std::collections::BTreeSet<u64>,
    object_meta: &mut BTreeMap<u64, CoralCraftObjectMeta>,
    warnings: &mut Vec<String>,
) -> Node {
    let nid = if cdr_node.id.0 > 0 && used_ids.insert(cdr_node.id.0) {
        let _ = doc.alloc_id();
        NodeId(cdr_node.id.0)
    } else {
        loop {
            let candidate = doc.alloc_id().0;
            if used_ids.insert(candidate) {
                break NodeId(candidate);
            }
        }
    };

    object_meta.insert(nid.0, CoralCraftObjectMeta { original: cdr_node.clone(), page_index });

    let xf = map_affine_to_vc(cdr_node.transform, offset);
    let local_bounds = cdr_node.kind.local_bounds().map(|r| {
        let vr = VcRect::new(r.x0.get(), r.y0.get(), r.x1.get(), r.y1.get());
        xf.transform_rect_bbox(vr)
    });

    let appearance = build_appearance(cdr_node.fill.as_ref(), cdr_node.outline.as_ref(), &cdr_node.effects, local_bounds);

    let mut base_node = match &cdr_node.kind {
        ObjectKind::Rect(rspec) => {
            let w = rspec.rect.width();
            let h = rspec.rect.height();
            let radii = [rspec.radii[0].get(), rspec.radii[1].get(), rspec.radii[2].get(), rspec.radii[3].get()];
            let kinds = [
                map_cdr_corner_kind_to_vc(rspec.corner_kinds[0]),
                map_cdr_corner_kind_to_vc(rspec.corner_kinds[1]),
                map_cdr_corner_kind_to_vc(rspec.corner_kinds[2]),
                map_cdr_corner_kind_to_vc(rspec.corner_kinds[3]),
            ];
            let shape_xf = affine_mul(xf, Affine::translate((rspec.rect.x0.get(), rspec.rect.y0.get())));
            let live = LiveShape::Rectangle { w, h, radii, kinds, xf: shape_xf };
            let path = live.to_path();
            let mut n = Node::new(nid, NodeKind::Path { path, rule: VcFillRule::NonZero, live: Some(Box::new(live)), clipping: false, guide: false });
            n.appearance = appearance.clone().into();
            n
        }
        ObjectKind::Ellipse(espec) => {
            let w = espec.rect.width();
            let h = espec.rect.height();
            let shape_xf = affine_mul(xf, Affine::translate((espec.rect.x0.get(), espec.rect.y0.get())));
            match espec.mode {
                EllipseMode::Full => {
                    let live = LiveShape::Ellipse { w, h, pie: (0.0, 360.0), xf: shape_xf };
                    let path = live.to_path();
                    let mut n =
                        Node::new(nid, NodeKind::Path { path, rule: VcFillRule::NonZero, live: Some(Box::new(live)), clipping: false, guide: false });
                    n.appearance = appearance.clone().into();
                    n
                }
                EllipseMode::Pie => {
                    let live = LiveShape::Ellipse { w, h, pie: (espec.start_deg.get(), espec.end_deg.get()), xf: shape_xf };
                    let path = live.to_path();
                    let mut n =
                        Node::new(nid, NodeKind::Path { path, rule: VcFillRule::NonZero, live: Some(Box::new(live)), clipping: false, guide: false });
                    n.appearance = appearance.clone().into();
                    n
                }
                EllipseMode::Arc => {
                    let cpath = espec.to_path().unwrap_or_default();
                    let (path, rule) = map_cdr_path_to_vc(&cpath, xf);
                    let mut n = Node::new(nid, NodeKind::Path { path, rule, live: None, clipping: false, guide: false });
                    n.appearance = appearance.clone().into();
                    n
                }
            }
        }
        ObjectKind::Polygon(pspec) => {
            let center = VcPoint::new(pspec.center.x.get(), pspec.center.y.get());
            if let Some(inner) = pspec.inner_radius {
                let path = shapes::star(center, pspec.radius.get(), inner.get(), pspec.points, pspec.rotation_deg.get()).transformed(xf);
                let mut n = Node::new(nid, NodeKind::Path { path, rule: VcFillRule::NonZero, live: None, clipping: false, guide: false });
                n.appearance = appearance.clone().into();
                n
            } else {
                let poly_xf = affine_mul(affine_mul(xf, Affine::translate(center.to_vec2())), Affine::rotate(pspec.rotation_deg.get().to_radians()));
                let live =
                    LiveShape::Polygon { radius: pspec.radius.get(), sides: pspec.points.max(3), xf: poly_xf, radii: Vec::new(), kinds: Vec::new() };
                let path = live.to_path();
                let mut n =
                    Node::new(nid, NodeKind::Path { path, rule: VcFillRule::NonZero, live: Some(Box::new(live)), clipping: false, guide: false });
                n.appearance = appearance.clone().into();
                n
            }
        }
        ObjectKind::Path(cpath) => {
            let (path, rule) = map_cdr_path_to_vc(cpath, xf);
            let mut n = Node::new(nid, NodeKind::Path { path, rule, live: None, clipping: false, guide: false });
            n.appearance = appearance.clone().into();
            n
        }
        ObjectKind::ArtisticText { text, font_family, size_pt, position } => {
            let mut style = CharStyle {
                font_family: font_family.clone(),
                size: size_pt.get().max(0.5),
                fill: cdr_node.fill.as_ref().map_or_else(|| Paint::solid(VcColor::BLACK), |f| map_cdr_fill_to_paint(f, local_bounds)),
                stroke: cdr_node.outline.as_ref().map_or(Paint::None, |o| map_cdr_color_to_paint(&o.color)),
                stroke_width: cdr_node.outline.as_ref().map_or(0.0, |o| o.width_pt.get()),
                ..CharStyle::default()
            };
            if style.font_family.trim().is_empty() {
                style.font_family = "Arial".to_owned();
            }
            let origin = VcPoint::new(position.x.get(), position.y.get());
            let mut tobj = TextObject::point(origin, text, style);
            tobj.xf = affine_mul(xf, tobj.xf);
            if let Some(VectorEffect::TextOnPath { spine, offset_pt }) =
                cdr_node.effects.iter().find(|e| matches!(e, VectorEffect::TextOnPath { .. }))
            {
                let (spine_pd, _) = map_cdr_path_to_vc(spine, xf);
                tobj.kind = TextKind::OnPath { path: spine_pd, start: 0.0, end: None };
                tobj.xf = Affine::IDENTITY;
                tobj.path_spacing = offset_pt.get();
            }
            let mut n = Node::new(nid, NodeKind::Text(Box::new(tobj)));
            if !appearance.effects.is_empty() {
                n.appearance = Appearance { items: vec![], effects: appearance.effects.clone(), contents_index: None }.into();
            }
            n
        }
        ObjectKind::ParagraphText(pspec) => {
            let mut style = CharStyle {
                font_family: pspec.font_family.clone(),
                size: pspec.font_size_pt.get().max(0.5),
                leading: Some(pspec.font_size_pt.get().max(0.5) * pspec.line_spacing_multiplier.get().max(0.5)),
                fill: cdr_node.fill.as_ref().map_or_else(|| Paint::solid(VcColor::BLACK), |f| map_cdr_fill_to_paint(f, local_bounds)),
                stroke: cdr_node.outline.as_ref().map_or(Paint::None, |o| map_cdr_color_to_paint(&o.color)),
                stroke_width: cdr_node.outline.as_ref().map_or(0.0, |o| o.width_pt.get()),
                ..CharStyle::default()
            };
            if style.font_family.trim().is_empty() {
                style.font_family = "Arial".to_owned();
            }
            let frame_rect = VcRect::new(
                pspec.position.x.get(),
                pspec.position.y.get(),
                pspec.position.x.get() + pspec.frame_width_pt.get().max(1.0),
                pspec.position.y.get() + pspec.frame_height_pt.get().max(1.0),
            );
            let frame = shapes::rectangle(frame_rect);
            let para = ParaStyle {
                justify: match pspec.alignment {
                    TextAlignment::Left => Justify::Left,
                    TextAlignment::Center => Justify::Center,
                    TextAlignment::Right => Justify::Right,
                    TextAlignment::Justify => Justify::JustifyLeft,
                },
                ..ParaStyle::default()
            };
            let area = AreaOptions {
                rows: 1,
                columns: (pspec.columns as usize).clamp(1, AreaOptions::MAX_ROWS_COLUMNS),
                gutter: pspec.gutter_pt.get().max(0.0),
                ..AreaOptions::default()
            };
            let tobj = TextObject {
                vertical: false,
                kind: TextKind::Area { frame },
                xf,
                runs: vec![TextRun::new(pspec.text.clone(), style)],
                para,
                paras: Vec::new(),
                area,
                path_effect: Default::default(),
                path_align: Default::default(),
                path_spacing: 0.0,
                wrap: Vec::new(),
                optical_margin: false,
                cached_bounds: None,
                cached_baselines: Vec::new(),
            };
            let mut n = Node::new(nid, NodeKind::Text(Box::new(tobj)));
            if !appearance.effects.is_empty() {
                n.appearance = Appearance { items: vec![], effects: appearance.effects.clone(), contents_index: None }.into();
            }
            n
        }
        ObjectKind::Group { children } => {
            let mut vc_children = Vec::with_capacity(children.len());
            for ch in children {
                let cn = convert_cdr_node_to_vc(ch, offset, page_index, doc, used_ids, object_meta, warnings);
                vc_children.push(Arc::new(cn));
            }
            let mut n = Node::group(nid, vc_children);
            if !appearance.effects.is_empty() {
                n.appearance = Appearance { items: vec![], effects: appearance.effects.clone(), contents_index: None }.into();
            }
            n
        }
        ObjectKind::Bitmap { pixel_width, pixel_height, dpi_x, dpi_y, rgba_pixels } => {
            let w_px = (*pixel_width).max(1);
            let h_px = (*pixel_height).max(1);
            let expected_len = (w_px as usize).saturating_mul(h_px as usize).saturating_mul(4);
            let mut raw = rgba_pixels.clone();
            if raw.len() < expected_len {
                raw.resize(expected_len, 255);
            } else if raw.len() > expected_len {
                raw.truncate(expected_len);
            }
            let mut png_bytes = Vec::new();
            if let Some(rgba_img) = image::RgbaImage::from_raw(w_px, h_px, raw)
                && let Err(e) = rgba_img.write_to(&mut Cursor::new(&mut png_bytes), image::ImageFormat::Png)
            {
                warnings.push(format!("W-IMPORT-BITMAP: PNG encoding failed for object {}: {e}", cdr_node.id.0));
            }
            let blob = ImageBlob::new("image/png", png_bytes);
            let key = blob.content_key();
            doc.images.entry(key.clone()).or_insert(blob);
            let sx = 72.0 / dpi_x.get().max(1.0);
            let sy = 72.0 / dpi_y.get().max(1.0);
            let img_xf = affine_mul(xf, Affine::scale_non_uniform(sx, sy));
            let im = ImageObject { key, width: w_px, height: h_px, xf: img_xf, link: None, placement: Default::default() };
            let mut n = Node::new(nid, NodeKind::Image(Box::new(im)));
            if !appearance.effects.is_empty() {
                n.appearance = Appearance { items: vec![], effects: appearance.effects.clone(), contents_index: None }.into();
            }
            n
        }
        ObjectKind::Unsupported(u) => {
            if !u.children.is_empty() {
                let mut vc_children = Vec::with_capacity(u.children.len());
                for ch in &u.children {
                    let cn = convert_cdr_node_to_vc(ch, offset, page_index, doc, used_ids, object_meta, warnings);
                    vc_children.push(Arc::new(cn));
                }
                Node::group(nid, vc_children)
            } else {
                let path = match u.fallback_bounds {
                    Some(b) => {
                        let r = VcRect::new(b.x0.get(), b.y0.get(), b.x1.get(), b.y1.get());
                        shapes::rectangle(r).transformed(xf)
                    }
                    None => PathData::default(),
                };
                let mut n = Node::new(nid, NodeKind::Path { path, rule: VcFillRule::NonZero, live: None, clipping: false, guide: false });
                n.appearance = appearance.clone().into();
                n
            }
        }
    };

    base_node.name = cdr_node.name.clone().or_else(|| {
        if let ObjectKind::Unsupported(u) = &cdr_node.kind {
            u.class_hint.clone().or_else(|| Some(format!("<CDR {}>", u.tag.display_ascii())))
        } else {
            None
        }
    });
    base_node.visible = cdr_node.visible;
    base_node.locked = cdr_node.locked;

    let mut opacity = cdr_node.opacity.get().clamp(0.0, 1.0) as f32;
    if let Some(trns) = &cdr_node.transparency {
        match trns {
            TransparencyStyle::Uniform { opacity: top } => {
                opacity *= top.get().clamp(0.0, 1.0) as f32;
            }
            TransparencyStyle::Fountain { geometry, stops, .. } => {
                let mask_id = loop {
                    let candidate = doc.alloc_id().0;
                    if used_ids.insert(candidate) {
                        break NodeId(candidate);
                    }
                };
                let b = base_node.geometric_bounds().unwrap_or_else(|| VcRect::new(0.0, 0.0, 100.0, 100.0));
                let mask = build_fountain_opacity_mask(mask_id, b, *geometry, stops);
                base_node.extras_mut().mask = Some(Box::new(mask));
            }
            TransparencyStyle::Lens { .. } | TransparencyStyle::Unknown => {}
        }
    }
    base_node.opacity = opacity;

    // Wrap in a Clip Group if PowerClip children are present.
    if !cdr_node.powerclip_children.is_empty() {
        let clip_child_id = loop {
            let candidate = doc.alloc_id().0;
            if used_ids.insert(candidate) {
                break NodeId(candidate);
            }
        };
        let mut frame_child = base_node.clone();
        frame_child.id = clip_child_id;
        frame_child.name = None;
        if let NodeKind::Path { clipping, .. } = &mut frame_child.kind {
            *clipping = true;
        }
        let mut group_children = Vec::with_capacity(cdr_node.powerclip_children.len().saturating_add(1));
        group_children.push(Arc::new(frame_child));
        for pc in &cdr_node.powerclip_children {
            let pcn = convert_cdr_node_to_vc(pc, offset, page_index, doc, used_ids, object_meta, warnings);
            group_children.push(Arc::new(pcn));
        }
        let mut clip_group = Node::new(nid, NodeKind::Group { children: group_children, clip: true });
        clip_group.name.clone_from(&cdr_node.name);
        clip_group.visible = cdr_node.visible;
        clip_group.locked = cdr_node.locked;
        clip_group.opacity = base_node.opacity;
        return clip_group;
    }

    base_node
}

fn convert_vc_layer_to_cdr(
    lnode: &Node,
    lmeta: Option<&CoralCraftLayerMeta>,
    offset: Vec2,
    doc: &VcDocument,
    meta: Option<&CoralCraftDocData>,
) -> CdrLayer {
    let lid = lmeta.map_or(LayerId(lnode.id.0.max(1)), |m| m.layer_id);
    let name = lnode.name.clone().unwrap_or_else(|| "Layer 1".to_owned());
    let mut clayer = CdrLayer::new_normal(lid, name);
    clayer.visible = lnode.visible;
    clayer.locked = lnode.locked;
    clayer.opacity = F64::new(f64::from(lnode.opacity.clamp(0.0, 1.0))).unwrap_or(F64::ONE);
    if let Some(m) = lmeta {
        clayer.role = m.role;
        clayer.master_in_front = m.master_in_front;
        clayer.residual = m.residual.clone();
        clayer.extra = m.extra.clone();
    }
    if let NodeKind::Layer { printable, children, .. } = &lnode.kind {
        clayer.printable = *printable;
        for ch in children {
            clayer.objects.push(convert_vc_node_to_cdr(ch, offset, doc, meta));
        }
    }
    clayer
}

fn convert_vc_node_to_cdr(node: &Node, offset: Vec2, doc: &VcDocument, meta: Option<&CoralCraftDocData>) -> ObjectNode {
    if let Some(ometa) = meta.and_then(|m| m.object_meta.get(&node.id.0)) {
        // Check if the node's basic properties were untouched in VectorCraft; if so, restore the exact original
        // ObjectNode (updating any recursive children if it is a Group or PowerClip).
        let name_matches =
            node.name == ometa.original.name || (ometa.original.name.is_none() && matches!(&ometa.original.kind, ObjectKind::Unsupported(_)));
        if name_matches && node.visible == ometa.original.visible && node.locked == ometa.original.locked {
            let mut restored = ometa.original.clone();
            match (&node.kind, &mut restored.kind) {
                (NodeKind::Group { children: vc_ch, clip: false }, ObjectKind::Group { children: cdr_ch }) if vc_ch.len() == cdr_ch.len() => {
                    *cdr_ch = vc_ch.iter().map(|c| convert_vc_node_to_cdr(c, offset, doc, meta)).collect();
                    return restored;
                }
                (NodeKind::Group { children: vc_ch, clip: true }, _)
                    if !restored.powerclip_children.is_empty() && vc_ch.len() == restored.powerclip_children.len().saturating_add(1) =>
                {
                    restored.powerclip_children = vc_ch.iter().skip(1).map(|c| convert_vc_node_to_cdr(c, offset, doc, meta)).collect();
                    return restored;
                }
                _ => return restored,
            }
        }
    }

    let oid = ObjectId(node.id.0.max(1));
    let (fill, outline) = convert_vc_appearance_to_cdr(&node.appearance);
    let effects = convert_vc_effects_to_cdr(&node.appearance.effects);

    let mut powerclip_children = Vec::new();
    let neg_offset = Vec2::new(-offset.x, -offset.y);
    let kind = match &node.kind {
        NodeKind::Path { path, rule, live, .. } => match live.as_deref() {
            Some(LiveShape::Rectangle { w, h, radii, kinds, xf }) => {
                let local_xf = affine_mul(Affine::translate(neg_offset), *xf);
                let [a0, a1, a2, a3, tx, ty] = local_xf.as_coeffs();
                let is_pure_translate = (a0 - 1.0).abs() < 1e-9 && a1.abs() < 1e-9 && a2.abs() < 1e-9 && (a3 - 1.0).abs() < 1e-9;
                if is_pure_translate
                    && let Ok(r) = CdrRect::new(tx, ty, tx + w.max(0.0), ty + h.max(0.0))
                    && let Ok(mut spec) = RectSpec::uniform(r, radii[0].max(0.0), map_vc_corner_kind_to_cdr(kinds[0]))
                {
                    for i in 0..4 {
                        if let (Some(r_dst), Some(&rad)) = (spec.radii.get_mut(i), radii.get(i)) {
                            *r_dst = F64::new(rad.max(0.0)).unwrap_or(F64::ZERO);
                        }
                        if let (Some(k_dst), Some(&k)) = (spec.corner_kinds.get_mut(i), kinds.get(i)) {
                            *k_dst = map_vc_corner_kind_to_cdr(k);
                        }
                    }
                    ObjectKind::Rect(spec)
                } else {
                    ObjectKind::Path(map_vc_path_to_cdr(path, *rule, offset))
                }
            }
            Some(LiveShape::Ellipse { w, h, pie, xf }) => {
                let local_xf = affine_mul(Affine::translate(neg_offset), *xf);
                let [a0, a1, a2, a3, tx, ty] = local_xf.as_coeffs();
                let is_pure_translate = (a0 - 1.0).abs() < 1e-9 && a1.abs() < 1e-9 && a2.abs() < 1e-9 && (a3 - 1.0).abs() < 1e-9;
                if is_pure_translate && let Ok(r) = CdrRect::new(tx, ty, tx + w.max(0.0), ty + h.max(0.0)) {
                    let span = (pie.1 - pie.0).rem_euclid(360.0);
                    if span < 1e-6 || (360.0 - span) < 1e-6 {
                        ObjectKind::Ellipse(EllipseSpec::full(r))
                    } else if let Ok(espec) = EllipseSpec::pie(r, pie.0, pie.1) {
                        ObjectKind::Ellipse(espec)
                    } else {
                        ObjectKind::Ellipse(EllipseSpec::full(r))
                    }
                } else {
                    ObjectKind::Path(map_vc_path_to_cdr(path, *rule, offset))
                }
            }
            Some(LiveShape::Polygon { radius, sides, xf, .. }) => {
                let local_xf = affine_mul(Affine::translate(neg_offset), *xf);
                let center_vc = affine_apply_point(local_xf, VcPoint::ZERO);
                let [a0, a1, ..] = local_xf.as_coeffs();
                let rot_deg = a1.atan2(a0).to_degrees();
                if let Ok(center) = CdrPoint::new(center_vc.x, center_vc.y)
                    && let Ok(pspec) = PolygonSpec::regular(center, (*radius).max(1.0), (*sides).max(3), rot_deg)
                {
                    ObjectKind::Polygon(pspec)
                } else {
                    ObjectKind::Path(map_vc_path_to_cdr(path, *rule, offset))
                }
            }
            _ => ObjectKind::Path(map_vc_path_to_cdr(path, *rule, offset)),
        },
        NodeKind::Compound { children, rule } => {
            let mut bp = BezPath::new();
            for ch in children {
                if let Some(pd) = ch.path_data() {
                    bp.extend(pd.to_bezpath());
                }
            }
            let pd = PathData::from_bezpath(&bp);
            ObjectKind::Path(map_vc_path_to_cdr(&pd, *rule, offset))
        }
        NodeKind::Group { children, clip } => {
            if *clip && !children.is_empty() {
                if let Some(first) = children.first() {
                    let frame_cdr = convert_vc_node_to_cdr(first, offset, doc, meta);
                    for ch in children.iter().skip(1) {
                        powerclip_children.push(convert_vc_node_to_cdr(ch, offset, doc, meta));
                    }
                    frame_cdr.kind
                } else {
                    ObjectKind::Group { children: Vec::new() }
                }
            } else {
                let cdr_children = children.iter().map(|c| convert_vc_node_to_cdr(c, offset, doc, meta)).collect();
                ObjectKind::Group { children: cdr_children }
            }
        }
        NodeKind::Text(tobj) => {
            let text = tobj.plain_text();
            let st = tobj.first_style();
            let local_xf = affine_mul(Affine::translate(neg_offset), tobj.xf);
            match &tobj.kind {
                TextKind::Area { frame } => {
                    let b = frame.bounds().map(|r| local_xf.transform_rect_bbox(r)).unwrap_or_else(|| VcRect::new(0.0, 0.0, 200.0, 100.0));
                    let pos = CdrPoint::new(b.x0, b.y0).unwrap_or(CdrPoint::ZERO);
                    let align = match tobj.para.justify {
                        Justify::Left | Justify::Auto => TextAlignment::Left,
                        Justify::Center | Justify::JustifyCenter => TextAlignment::Center,
                        Justify::Right | Justify::JustifyRight => TextAlignment::Right,
                        Justify::JustifyLeft | Justify::JustifyAll => TextAlignment::Justify,
                    };
                    let leading_mult = st.leading.map_or(1.2, |l| (l / st.size.max(1.0)).clamp(0.5, 5.0));
                    if let Ok(pspec) = ParagraphTextSpec::new(
                        pos,
                        b.width().max(1.0),
                        b.height().max(1.0),
                        text.clone(),
                        st.font_family.clone(),
                        st.size.max(1.0),
                        leading_mult,
                        align,
                        tobj.area.columns as u32,
                        tobj.area.gutter,
                    ) {
                        ObjectKind::ParagraphText(Box::new(pspec))
                    } else {
                        ObjectKind::ArtisticText {
                            text,
                            font_family: st.font_family,
                            size_pt: F64::new(st.size.max(1.0)).unwrap_or(F64::ONE),
                            position: pos,
                        }
                    }
                }
                TextKind::Point | TextKind::OnPath { .. } => {
                    let orig_vc = affine_apply_point(local_xf, VcPoint::ZERO);
                    let position = CdrPoint::new(orig_vc.x, orig_vc.y).unwrap_or(CdrPoint::ZERO);
                    ObjectKind::ArtisticText { text, font_family: st.font_family, size_pt: F64::new(st.size.max(1.0)).unwrap_or(F64::ONE), position }
                }
            }
        }
        NodeKind::Image(im) => {
            let mut rgba_pixels = Vec::new();
            let mut pw = im.width.max(1);
            let mut ph = im.height.max(1);
            if let Some(blob) = doc.images.get(&im.key)
                && let Ok(decoded) = image::load_from_memory(&blob.bytes)
            {
                let rgba = decoded.to_rgba8();
                pw = rgba.width().max(1);
                ph = rgba.height().max(1);
                rgba_pixels = rgba.into_raw();
            }
            if rgba_pixels.is_empty() {
                rgba_pixels = vec![255; (pw as usize).saturating_mul(ph as usize).saturating_mul(4)];
            }
            let [a0, a1, a2, a3, ..] = im.xf.as_coeffs();
            let sx = a0.hypot(a1).max(1e-3);
            let sy = a2.hypot(a3).max(1e-3);
            let dpi_x = F64::new((72.0 / sx).clamp(1.0, 9600.0)).unwrap_or_else(|_| F64::new(72.0).unwrap_or(F64::ONE));
            let dpi_y = F64::new((72.0 / sy).clamp(1.0, 9600.0)).unwrap_or_else(|_| F64::new(72.0).unwrap_or(F64::ONE));
            ObjectKind::Bitmap { pixel_width: pw, pixel_height: ph, dpi_x, dpi_y, rgba_pixels }
        }
        _ => {
            let pd = node.path_data().cloned().unwrap_or_default();
            ObjectKind::Path(map_vc_path_to_cdr(&pd, VcFillRule::NonZero, offset))
        }
    };

    let mut cnode = ObjectNode::new(oid, node.name.clone(), kind, fill, outline);
    if let NodeKind::Text(tobj) = &node.kind {
        let st = tobj.first_style();
        if cnode.fill.is_none()
            && let Some(col) = map_vc_paint_to_cdr_color(&st.fill)
        {
            cnode.fill = Some(FillStyle::uniform(col));
        }
    }
    if let NodeKind::Image(im) = &node.kind {
        let local_xf = affine_mul(Affine::translate(neg_offset), im.xf);
        let [_, _, _, _, tx, ty] = local_xf.as_coeffs();
        cnode.transform = Affine2::translation(tx, ty).unwrap_or(Affine2::IDENTITY);
    }
    cnode.visible = node.visible;
    cnode.locked = node.locked;
    cnode.opacity = F64::new(f64::from(node.opacity.clamp(0.0, 1.0))).unwrap_or(F64::ONE);
    cnode.effects = effects;
    cnode.powerclip_children = powerclip_children;
    if let Some(ometa) = meta.and_then(|m| m.object_meta.get(&node.id.0)) {
        cnode.residual = ometa.original.residual.clone();
        cnode.extra = ometa.original.extra.clone();
    }
    cnode
}

fn convert_vc_appearance_to_cdr(ap: &Appearance) -> (Option<FillStyle>, Option<OutlineStyle>) {
    let mut fill = None;
    let mut outline = None;
    let mut first_fill_idx = None;
    let mut first_stroke_idx = None;

    for (idx, item) in ap.items.iter().enumerate() {
        match item {
            AppearanceItem::Fill(fl) if fl.visible && !fl.paint.is_none() => {
                first_fill_idx.get_or_insert(idx);
                fill = convert_vc_paint_to_fill(&fl.paint);
            }
            AppearanceItem::Stroke(sl) if sl.visible && !sl.paint.is_none() => {
                first_stroke_idx.get_or_insert(idx);
                outline = convert_vc_stroke_to_outline(sl);
            }
            _ => {}
        }
    }

    if let (Some(s_idx), Some(f_idx), Some(outl)) = (first_stroke_idx, first_fill_idx, outline.as_mut())
        && s_idx < f_idx
    {
        outl.behind_fill = true;
    }

    (fill, outline)
}

fn convert_vc_paint_to_fill(paint: &Paint) -> Option<FillStyle> {
    match paint {
        Paint::None => None,
        Paint::Solid { .. } => map_vc_paint_to_cdr_color(paint).map(FillStyle::uniform),
        Paint::Gradient(gp) => {
            let mut stops = Vec::with_capacity(gp.gradient.stops.len());
            for st in &gp.gradient.stops {
                if let Ok(fs) = FountainStop::with_midpoint(f64::from(st.offset), f64::from(st.midpoint), map_vc_color_to_cdr(&st.color)) {
                    stops.push(fs);
                }
            }
            let geometry = match gp.gradient.kind {
                GradientKind::Linear | GradientKind::Freeform => {
                    FountainGeometry::Linear { angle_deg: F64::new(gp.angle).unwrap_or(F64::ZERO), edge_pad: F64::ZERO }
                }
                GradientKind::Radial => FountainGeometry::Radial {
                    center_x: F64::new(0.5).unwrap_or(F64::ZERO),
                    center_y: F64::new(0.5).unwrap_or(F64::ZERO),
                    radius: F64::new(0.5).unwrap_or(F64::ZERO),
                    focal_x: F64::new(0.5).unwrap_or(F64::ZERO),
                    focal_y: F64::new(0.5).unwrap_or(F64::ZERO),
                },
            };
            Some(FillStyle::Fountain { geometry, blend_mode: FountainBlendMode::Direct, stops })
        }
        Paint::Pattern { .. } => Some(FillStyle::TwoColorPattern {
            kind: PatternKind::Checkerboard,
            fg_color: CdrColor::BLACK,
            bg_color: CdrColor::WHITE,
            tile_size_pt: F64::new(12.0).unwrap_or(F64::ONE),
        }),
    }
}

fn convert_vc_stroke_to_outline(sl: &StrokeLayer) -> Option<OutlineStyle> {
    let color = map_vc_paint_to_cdr_color(&sl.paint)?;
    let width_pt = if (sl.width - HAIRLINE_WIDTH_PT).abs() < 1e-6 { F64::ZERO } else { F64::new(sl.width.max(0.0)).unwrap_or(F64::ZERO) };
    let mut outl = OutlineStyle::with_width(color, width_pt);
    outl.cap = match sl.cap {
        VcLineCap::Butt => CdrLineCap::Butt,
        VcLineCap::Round => CdrLineCap::Round,
        VcLineCap::Square => CdrLineCap::Square,
    };
    outl.join = match sl.join {
        VcLineJoin::Miter => CdrLineJoin::Miter,
        VcLineJoin::Round => CdrLineJoin::Round,
        VcLineJoin::Bevel => CdrLineJoin::Bevel,
    };
    outl.miter_limit = F64::new(sl.miter_limit.max(1.0)).unwrap_or(F64::ONE);
    if let Some(d) = &sl.dash {
        outl.dash_pattern = d.pattern.iter().filter_map(|&v| F64::new(v.max(0.1)).ok()).collect();
    }
    outl.start_arrow = map_vc_arrow_to_cdr(sl.start_arrow);
    outl.end_arrow = map_vc_arrow_to_cdr(sl.end_arrow);
    Some(outl)
}

fn convert_vc_effects_to_cdr(effects: &[Effect]) -> Vec<VectorEffect> {
    let mut out = Vec::new();
    for eff in effects {
        if !eff.visible {
            continue;
        }
        match eff.id.as_str() {
            "stylize.dropShadow" => {
                let ox = eff.params.get("x").and_then(serde_json::Value::as_f64).unwrap_or(7.0);
                let oy = eff.params.get("y").and_then(serde_json::Value::as_f64).unwrap_or(7.0);
                let op = eff.params.get("opacity").and_then(serde_json::Value::as_f64).unwrap_or(75.0) / 100.0;
                let col_hex = eff.params.get("color").and_then(serde_json::Value::as_str).unwrap_or("#000000");
                let color = CdrColor::from_hex(col_hex).unwrap_or(CdrColor::BLACK);
                if let (Ok(offset_x), Ok(offset_y), Ok(opacity)) = (F64::new(ox), F64::new(oy), F64::new(op.clamp(0.0, 1.0))) {
                    out.push(VectorEffect::DropShadow { offset_x, offset_y, opacity, color });
                }
            }
            "path.offsetPath" => {
                let off = eff.params.get("offset").and_then(serde_json::Value::as_f64).unwrap_or(10.0);
                let steps = eff.params.get("steps").and_then(serde_json::Value::as_u64).unwrap_or(1) as u32;
                if let Ok(offset_pt) = F64::new(off.abs().max(0.1)) {
                    out.push(VectorEffect::Contour { steps: steps.max(1), offset_pt, inside: off < 0.0 });
                }
            }
            "threeD.extrude" => {
                let depth = eff.params.get("depth").and_then(serde_json::Value::as_f64).unwrap_or(50.0);
                let angle = eff.params.get("rotationZ").and_then(serde_json::Value::as_f64).unwrap_or(0.0);
                if let (Ok(depth_pt), Ok(angle_deg)) = (F64::new(depth.max(0.0)), F64::new(angle)) {
                    out.push(VectorEffect::Extrude { depth_pt, angle_deg });
                }
            }
            "distort.puckerBloat" => {
                let amt = eff.params.get("amount").and_then(serde_json::Value::as_f64).unwrap_or(0.0);
                if let Ok(amplitude) = F64::new(amt) {
                    out.push(VectorEffect::Distortion { kind: DistortionKind::PushPull, amplitude, frequency: F64::ONE });
                }
            }
            "distort.zigZag" => {
                let sz = eff.params.get("size").and_then(serde_json::Value::as_f64).unwrap_or(10.0);
                let ridges = eff.params.get("ridges").and_then(serde_json::Value::as_f64).unwrap_or(4.0);
                if let (Ok(amplitude), Ok(frequency)) = (F64::new(sz), F64::new(ridges.max(1.0))) {
                    out.push(VectorEffect::Distortion { kind: DistortionKind::Zipper, amplitude, frequency });
                }
            }
            "distort.twist" => {
                let ang = eff.params.get("angle").and_then(serde_json::Value::as_f64).unwrap_or(10.0);
                if let Ok(amplitude) = F64::new(ang) {
                    out.push(VectorEffect::Distortion { kind: DistortionKind::Twister, amplitude, frequency: F64::ONE });
                }
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing, clippy::arithmetic_side_effects)]
mod tests {
    use super::*;
    use coralcraft_doc::{ChunkTag, LensKind, RawChunk, RawPayload, UnsupportedObject};

    #[test]
    fn multipage_cdr_document_maps_pages_to_artboards_and_layers_to_layer_nodes() {
        let mut cdr_doc = CdrDocument::new_a4_portrait();
        cdr_doc.title = "Brochure Layout".to_owned();
        cdr_doc.units = CdrUnit::Mm;
        cdr_doc.pages[0].bleed = F64::new(9.0).unwrap();
        cdr_doc.palette.swatches.push(PaletteSwatch {
            name: "Custom Ruby Spot".to_owned(),
            color: CdrColor::spot("Custom Ruby Spot", 0.75, [0.0, 0.9, 0.6, 0.1]).unwrap(),
        });

        // Add Page 2
        let p2_size = CdrSize::new(400.0, 500.0).unwrap();
        let mut p2_layer = CdrLayer::new_normal(LayerId(20), "Page 2 Artwork");
        p2_layer.locked = true;
        p2_layer.printable = false;
        p2_layer.opacity = F64::new(0.8).unwrap();
        let p2_rect = ObjectNode::new(
            ObjectId(21),
            Some("Page 2 Box".to_owned()),
            ObjectKind::Rect(RectSpec::sharp(CdrRect::new(10.0, 20.0, 110.0, 80.0).unwrap())),
            Some(FillStyle::uniform(CdrColor::rgb8(20, 40, 200))),
            None,
        );
        p2_layer.objects.push(p2_rect);
        cdr_doc.pages.push(CdrPage {
            id: PageId(2),
            name: "Inside Spread".to_owned(),
            size: p2_size,
            bleed: F64::new(9.0).unwrap(),
            background: Some(FillStyle::uniform(CdrColor::rgb8(250, 250, 245))),
            layers: vec![p2_layer],
            residual: Residual::default(),
            extra: BTreeMap::new(),
        });

        let (vdoc, warnings) = import_document(&cdr_doc).unwrap();
        assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
        assert_eq!(vdoc.title, "Brochure Layout");
        assert_eq!(vdoc.units, VcUnit::Millimeters);
        assert_eq!(vdoc.setup.bleed, [9.0, 9.0, 9.0, 9.0]);
        assert_eq!(vdoc.artboards.len(), 2);
        assert_eq!(vdoc.artboards[0].name, "Page 1");
        assert_eq!(vdoc.artboards[1].name, "Inside Spread");
        let p1_w = cdr_doc.pages[0].size.width.get();
        assert!((vdoc.artboards[1].rect.x0 - (p1_w + PAGE_GAP_PT)).abs() < 1e-6);
        assert!((vdoc.artboards[1].rect.width() - 400.0).abs() < 1e-6);
        assert!((vdoc.artboards[1].rect.height() - 500.0).abs() < 1e-6);

        // Spot swatch must be imported into vdoc.swatches
        assert!(vdoc.swatches.iter().any(|s| s.name == "Custom Ruby Spot" && s.spot));

        // Page 2's rectangle must be offset onto Artboard 2
        let p2_layer_node = vdoc.layers.iter().find(|l| l.name.as_deref() == Some("Page 2 Artwork")).unwrap();
        assert!(p2_layer_node.locked);
        assert!((p2_layer_node.opacity - 0.8).abs() < 1e-5);
        let p2_children = p2_layer_node.children().unwrap();
        assert_eq!(p2_children.len(), 1);
        let p2_bounds = p2_children[0].geometric_bounds().unwrap();
        assert!((p2_bounds.x0 - (vdoc.artboards[1].rect.x0 + 10.0)).abs() < 1e-5);
        assert!((p2_bounds.y0 - 20.0).abs() < 1e-5);
    }

    #[test]
    fn shapes_paths_text_bitmaps_groups_powerclip_and_appearances_map_to_vectorcraft_nodes() {
        let mut cdr_doc = CdrDocument::new_a4_portrait();
        let layer = &mut cdr_doc.pages[0].layers[0];

        // 1. Rounded/chamfered rectangle with behind_fill outline, dashes, and arrowheads
        let rect_spec = RectSpec::uniform(CdrRect::new(10.0, 15.0, 130.0, 95.0).unwrap(), 12.0, CdrCornerKind::Chamfered).unwrap();
        let mut outline = OutlineStyle::with_width(CdrColor::cmyk(0.0, 1.0, 1.0, 0.0).unwrap(), F64::new(3.0).unwrap());
        outline.behind_fill = true;
        outline.cap = CdrLineCap::Round;
        outline.join = CdrLineJoin::Bevel;
        outline.dash_pattern = vec![F64::new(6.0).unwrap(), F64::new(3.0).unwrap()];
        outline.start_arrow = ArrowheadKind::Triangle;
        outline.end_arrow = ArrowheadKind::Circle;
        let mut rect_node = ObjectNode::new(
            ObjectId(100),
            Some("Chamfered Card".to_owned()),
            ObjectKind::Rect(rect_spec),
            Some(FillStyle::Fountain {
                geometry: FountainGeometry::Linear { angle_deg: F64::new(30.0).unwrap(), edge_pad: F64::ZERO },
                blend_mode: FountainBlendMode::Direct,
                stops: vec![
                    FountainStop::with_midpoint(0.0, 0.35, CdrColor::rgb8(255, 100, 0)).unwrap(),
                    FountainStop::new(1.0, CdrColor::rgb8(0, 50, 200)).unwrap(),
                ],
            }),
            Some(outline),
        );
        rect_node.effects.push(VectorEffect::DropShadow {
            offset_x: F64::new(4.0).unwrap(),
            offset_y: F64::new(5.0).unwrap(),
            opacity: F64::new(0.6).unwrap(),
            color: CdrColor::BLACK,
        });
        layer.objects.push(rect_node);

        // 2. Pie ellipse with radial fountain transparency
        let pie_spec = EllipseSpec::pie(CdrRect::new(150.0, 20.0, 250.0, 120.0).unwrap(), 15.0, 240.0).unwrap();
        let mut pie_node = ObjectNode::new(
            ObjectId(101),
            Some("Pie Chart".to_owned()),
            ObjectKind::Ellipse(pie_spec),
            Some(FillStyle::Fountain {
                geometry: FountainGeometry::Radial {
                    center_x: F64::new(0.5).unwrap(),
                    center_y: F64::new(0.5).unwrap(),
                    radius: F64::new(0.5).unwrap(),
                    focal_x: F64::new(0.3).unwrap(),
                    focal_y: F64::new(0.3).unwrap(),
                },
                blend_mode: FountainBlendMode::Direct,
                stops: vec![FountainStop::new(0.0, CdrColor::WHITE).unwrap(), FountainStop::new(1.0, CdrColor::rgb8(0, 180, 90)).unwrap()],
            }),
            Some(OutlineStyle::hairline(CdrColor::BLACK)),
        );
        pie_node.transparency = Some(TransparencyStyle::fountain_linear(45.0, 1.0, 0.2));
        layer.objects.push(pie_node);

        // 3. Regular polygon + Cubic Bezier path
        let poly_spec = PolygonSpec::regular(CdrPoint::new(80.0, 200.0).unwrap(), 40.0, 6, 15.0).unwrap();
        layer.objects.push(ObjectNode::new(
            ObjectId(102),
            Some("Hex".to_owned()),
            ObjectKind::Polygon(poly_spec),
            Some(FillStyle::uniform(CdrColor::gray(0.3).unwrap())),
            None,
        ));

        let mut custom_path = CdrPath::new();
        custom_path.fill_rule = CdrFillRule::EvenOdd;
        custom_path.move_to(CdrPoint::new(10.0, 300.0).unwrap()).unwrap();
        custom_path.line_to(CdrPoint::new(60.0, 300.0).unwrap()).unwrap();
        custom_path.cubic_to(CdrPoint::new(80.0, 320.0).unwrap(), CdrPoint::new(80.0, 350.0).unwrap(), CdrPoint::new(60.0, 370.0).unwrap()).unwrap();
        custom_path.close().unwrap();
        layer.objects.push(ObjectNode::new(
            ObjectId(103),
            Some("Custom Curve".to_owned()),
            ObjectKind::Path(custom_path),
            Some(FillStyle::TwoColorPattern {
                kind: PatternKind::Checkerboard,
                fg_color: CdrColor::BLACK,
                bg_color: CdrColor::WHITE,
                tile_size_pt: F64::new(10.0).unwrap(),
            }),
            None,
        ));

        // 4. Artistic text & Paragraph text
        layer.objects.push(ObjectNode::new(
            ObjectId(104),
            Some("Headline".to_owned()),
            ObjectKind::ArtisticText {
                text: "CoralCraft Headline".to_owned(),
                font_family: "Arial".to_owned(),
                size_pt: F64::new(24.0).unwrap(),
                position: CdrPoint::new(20.0, 420.0).unwrap(),
            },
            Some(FillStyle::uniform(CdrColor::rgb8(30, 30, 30))),
            None,
        ));

        let para_spec = ParagraphTextSpec::new(
            CdrPoint::new(20.0, 450.0).unwrap(),
            200.0,
            120.0,
            "Paragraph one.\nParagraph two.",
            "Arial",
            11.0,
            1.25,
            TextAlignment::Justify,
            2,
            12.0,
        )
        .unwrap();
        layer.objects.push(ObjectNode::new(
            ObjectId(105),
            Some("Body Copy".to_owned()),
            ObjectKind::ParagraphText(Box::new(para_spec)),
            Some(FillStyle::uniform(CdrColor::BLACK)),
            None,
        ));

        // 5. Bitmap (2x2 RGBA8 @ 144 DPI)
        layer.objects.push(ObjectNode::new(
            ObjectId(106),
            Some("Logo Bitmap".to_owned()),
            ObjectKind::Bitmap {
                pixel_width: 2,
                pixel_height: 2,
                dpi_x: F64::new(144.0).unwrap(),
                dpi_y: F64::new(144.0).unwrap(),
                rgba_pixels: vec![
                    255, 0, 0, 255, 0, 255, 0, 255, // row 0
                    0, 0, 255, 255, 255, 255, 0, 128, // row 1
                ],
            },
            None,
            None,
        ));

        // 6. PowerClip container (frame rect clipping a child ellipse)
        let mut frame_node = ObjectNode::new(
            ObjectId(107),
            Some("PowerClip Frame".to_owned()),
            ObjectKind::Rect(RectSpec::sharp(CdrRect::new(300.0, 300.0, 420.0, 420.0).unwrap())),
            Some(FillStyle::uniform(CdrColor::WHITE)),
            Some(OutlineStyle::hairline(CdrColor::BLACK)),
        );
        frame_node.powerclip_children.push(ObjectNode::new(
            ObjectId(108),
            Some("Clipped Circle".to_owned()),
            ObjectKind::Ellipse(EllipseSpec::full(CdrRect::new(320.0, 320.0, 400.0, 400.0).unwrap())),
            Some(FillStyle::uniform(CdrColor::rgb8(220, 20, 60))),
            None,
        ));
        layer.objects.push(frame_node);

        let (vdoc, warnings) = import_document(&cdr_doc).unwrap();
        assert!(warnings.is_empty());
        let children = vdoc.layers[0].children().unwrap();
        assert_eq!(children.len(), 8);

        // Verify 1: Chamfered rectangle with behind_fill stroke before fill + dropShadow effect
        let n_rect = &children[0];
        assert!(matches!(n_rect.live_shape(), Some(LiveShape::Rectangle { .. })));
        assert!(matches!(n_rect.appearance.items[0], AppearanceItem::Stroke(_)), "behind_fill must place stroke before fill");
        assert!(matches!(n_rect.appearance.items[1], AppearanceItem::Fill(_)));
        assert!(n_rect.appearance.effects.iter().any(|e| e.id == "stylize.dropShadow"));

        // Verify 2: Pie ellipse with OpacityMask from Fountain Transparency
        let n_pie = &children[1];
        assert!(matches!(n_pie.live_shape(), Some(LiveShape::Ellipse { pie: (s, e), .. }) if (*s - 15.0).abs() < 1e-6 && (*e - 240.0).abs() < 1e-6));
        assert!(n_pie.extras().mask.is_some());

        // Verify 3: Regular polygon live shape
        let n_poly = &children[2];
        assert!(matches!(n_poly.live_shape(), Some(LiveShape::Polygon { sides: 6, .. })));

        // Verify 4: ArtisticText -> Point text, ParagraphText -> Area text
        let NodeKind::Text(t_point) = &children[4].kind else { panic!("expected Point text") };
        assert!(matches!(t_point.kind, TextKind::Point));
        assert_eq!(t_point.plain_text(), "CoralCraft Headline");

        let NodeKind::Text(t_area) = &children[5].kind else { panic!("expected Area text") };
        assert!(matches!(t_area.kind, TextKind::Area { .. }));
        assert_eq!(t_area.area.columns, 2);
        assert!((t_area.area.gutter - 12.0).abs() < 1e-6);

        // Verify 5: Bitmap -> NodeKind::Image + PNG blob in vdoc.images
        let NodeKind::Image(im) = &children[6].kind else { panic!("expected Image") };
        assert_eq!((im.width, im.height), (2, 2));
        assert!(vdoc.images.contains_key(&im.key));

        // Verify 6: PowerClip -> Clip Group with clipping first child
        let NodeKind::Group { children: pc_children, clip: true } = &children[7].kind else {
            panic!("expected Clip Group for PowerClip");
        };
        assert_eq!(pc_children.len(), 2);
        assert!(matches!(pc_children[0].kind, NodeKind::Path { clipping: true, .. }));
    }

    #[test]
    fn residuals_and_unsupported_objects_roundtrip_losslessly_through_vectorcraft_format() {
        let mut cdr_doc = CdrDocument::new_a4_portrait();
        cdr_doc.title = "Lossless Roundtrip Spec".to_owned();
        cdr_doc.residual.chunks.push(RawChunk {
            tag: ChunkTag(*b"dunk"),
            flags: 1,
            slot: 0,
            payload: RawPayload(vec![0xDE, 0xAD, 0xBE, 0xEF]),
            src_offset: 64,
        });
        cdr_doc.pages[0].residual.chunks.push(RawChunk {
            tag: ChunkTag(*b"punk"),
            flags: 2,
            slot: 1,
            payload: RawPayload(vec![1, 2, 3]),
            src_offset: 128,
        });
        cdr_doc.pages[0].layers[0].residual.chunks.push(RawChunk {
            tag: ChunkTag(*b"lunk"),
            flags: 0,
            slot: 0,
            payload: RawPayload(vec![9, 8, 7]),
            src_offset: 256,
        });

        // Master layer with an object
        let mut master = CdrLayer::new_normal(LayerId(50), "Master All BG");
        master.role = LayerRole::MasterAllPages;
        master.objects.push(ObjectNode::new(
            ObjectId(51),
            Some("Watermark".to_owned()),
            ObjectKind::Rect(RectSpec::sharp(CdrRect::new(0.0, 0.0, 100.0, 50.0).unwrap())),
            Some(FillStyle::uniform(CdrColor::gray(0.1).unwrap())),
            None,
        ));
        cdr_doc.master_layers.insert(0, master);

        // Unsupported object with residual and child
        let unsup = UnsupportedObject {
            tag: ChunkTag(*b"mesh"),
            class_hint: Some("Mesh Fill Object".to_owned()),
            payload: RawPayload(vec![0x11, 0x22, 0x33, 0x44, 0x55]),
            fallback_bounds: Some(CdrRect::new(40.0, 50.0, 140.0, 150.0).unwrap()),
            absolute_geometry: true,
            children: Vec::new(),
            residual: Residual {
                chunks: vec![RawChunk { tag: ChunkTag(*b"mchn"), flags: 3, slot: 0, payload: RawPayload(vec![0xAA, 0xBB]), src_offset: 512 }],
            },
        };
        let mut unsup_node = ObjectNode::new(ObjectId(200), Some("Opaque Mesh".to_owned()), ObjectKind::Unsupported(unsup), None, None);
        unsup_node.transparency = Some(TransparencyStyle::Lens { kind: LensKind::Magnify { factor: F64::new(2.0).unwrap() } });
        unsup_node.effects.push(VectorEffect::Distortion {
            kind: DistortionKind::Twister,
            amplitude: F64::new(45.0).unwrap(),
            frequency: F64::new(3.0).unwrap(),
        });
        cdr_doc.pages[0].layers[0].objects.push(unsup_node);
        cdr_doc.next_id = 201;

        // Import -> save to .vectorcraft -> load from .vectorcraft -> export back to CdrDocument
        let (vdoc, _) = import_document(&cdr_doc).unwrap();
        assert!(vdoc.unknown.contains_key(CORALCRAFT_UNKNOWN_KEY));

        let vc_bytes = vectorcraft_format::save(&vdoc, false);
        let vdoc_reloaded = vectorcraft_format::load(&vc_bytes).unwrap();
        let exported = export_document(&vdoc_reloaded).unwrap();
        assert_eq!(exported, cdr_doc);
    }

    #[test]
    fn import_cdr_bytes_parses_cdr7_cdrx_zip_and_cmx1_containers() {
        let mut cdr_doc = CdrDocument::new_a4_portrait();
        cdr_doc.pages[0].layers[0].objects.push(ObjectNode::new(
            ObjectId(10),
            Some("Sample Rect".to_owned()),
            ObjectKind::Rect(RectSpec::sharp(CdrRect::new(25.0, 35.0, 125.0, 95.0).unwrap())),
            Some(FillStyle::uniform(CdrColor::rgb8(200, 30, 50))),
            Some(OutlineStyle::hairline(CdrColor::BLACK)),
        ));

        for bytes in [
            coralcraft_cdr::write_cdr7(&cdr_doc).unwrap(),
            coralcraft_cdr::write_cdrx_zip(&cdr_doc).unwrap(),
            coralcraft_cdr::write_cmx1(&cdr_doc).unwrap(),
        ] {
            let outcome = import_cdr_bytes(&bytes).unwrap();
            assert_eq!(outcome.document.artboards.len(), 1);
            assert!(!outcome.document.layers.is_empty());
            let total_objs: usize = outcome.document.layers.iter().map(|l| l.children().map_or(0, Vec::len)).sum();
            assert!(total_objs >= 1);
        }
    }

    #[test]
    fn authored_vectorcraft_document_exports_cleanly_to_cdr_and_cmx() {
        let mut vdoc = VcDocument::new(612.0, 792.0);
        vdoc.title = "Fresh Poster".to_owned();
        let nid = vdoc.alloc_id();
        let live =
            LiveShape::Rectangle { w: 120.0, h: 80.0, radii: [8.0; 4], kinds: [VcCornerKind::Chamfer; 4], xf: Affine::translate((40.0, 60.0)) };
        let path = live.to_path();
        let mut node = Node::new(nid, NodeKind::Path { path, rule: VcFillRule::NonZero, live: Some(Box::new(live)), clipping: false, guide: false });
        node.name = Some("Chamfer Box".to_owned());
        node.appearance = Appearance::basic(Paint::solid(VcColor::rgb(0.2, 0.6, 0.9)), Paint::solid(VcColor::BLACK), 2.0).into();
        if let Some(layer) = vdoc.layers.first_mut().map(Arc::make_mut)
            && let Some(children) = layer.children_mut()
        {
            children.push(Arc::new(node));
        }
        vdoc.fix_next_id();

        let cdr7 = write_cdr7_document(&vdoc).unwrap();
        let cdrx = write_cdrx_zip_document(&vdoc).unwrap();
        let cmx1 = write_cmx1_document(&vdoc).unwrap();
        assert_eq!(import_cdr_bytes(&cdr7).unwrap().document.artboards.len(), 1);
        assert_eq!(import_cdr_bytes(&cdrx).unwrap().document.artboards.len(), 1);
        assert_eq!(import_cdr_bytes(&cmx1).unwrap().document.artboards.len(), 1);
    }
}
