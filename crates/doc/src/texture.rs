//! CoralCraft (P3 Task T1): reusable texture-fill definitions. A texture is **not** an ordinary
//! pattern relabelled: it is a named, document-level definition with a procedural kind and its
//! parameters plus the two colours CorelDRAW's texture fills paint with, shared by every object
//! whose paint names it, and synthesized deterministically for render and export.

use crate::color::Color;
use crate::{Node, Rect};

/// The procedural families a texture definition can be. `Stipple` is CorelDRAW's two-colour
/// stipple family: a deterministic cell grid whose cells hold the foreground colour with
/// probability `density`, else the background.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TextureKind {
    #[serde(rename = "stipple")]
    Stipple,
}

impl TextureKind {
    /// The parameter name the engine commands accept.
    pub fn id(self) -> &'static str {
        match self {
            TextureKind::Stipple => "stipple",
        }
    }

    /// The kind an engine command's `kind` parameter names, if it is one of ours.
    pub fn of_id(id: &str) -> Option<Self> {
        match id {
            "stipple" => Some(TextureKind::Stipple),
            _ => None,
        }
    }
}

/// A named, reusable texture definition (P3 Task T1).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TextureDef {
    pub name: String,
    pub kind: TextureKind,
    /// 0..=1: the share of cells holding the foreground colour.
    pub density: f64,
    pub foreground: Color,
    pub background: Color,
}

/// The cell edge of the stipple synthesis, in points.
const CELL: f64 = 12.0;

/// A cell (i, j) holds the foreground when this says so — a pure function of the definition and
/// the cell coordinates, so every renderer synthesizes byte-identical tiles.
fn holds_foreground(def: &TextureDef, i: i64, j: i64) -> bool {
    let mut h = 0x811c_9dc5u64;
    let density_bits = (def.density * 65536.0) as u64;
    for byte in def.name.bytes().chain(density_bits.to_le_bytes()) {
        h = (h ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
    }
    h ^= (i as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    h = h.rotate_left(17) ^ (j as u64).wrapping_mul(0xc2b2_ae3d_27d4_ebf4);
    (f64::from((h % 1000) as u32)) < def.density * 1000.0
}

impl TextureDef {
    /// The definition's tiles over `bounds`: one rectangle per cell, in its colour. Deterministic
    /// and bounded (the cell grid covers at most the bounds), so independent readers see the same
    /// geometry every export.
    pub fn tiles(&self, bounds: Rect) -> Vec<(Rect, Color)> {
        let x0 = (bounds.x0 / CELL).floor() as i64;
        let x1 = (bounds.x1 / CELL).ceil() as i64;
        let y0 = (bounds.y0 / CELL).floor() as i64;
        let y1 = (bounds.y1 / CELL).ceil() as i64;
        let mut out = Vec::new();
        for i in x0..x1 {
            for j in y0..y1 {
                let left = f64::from(i as i32) * CELL;
                let top = f64::from(j as i32) * CELL;
                let rect = Rect::new(left, top, left + CELL, top + CELL);
                let color = if holds_foreground(self, i, j) { self.foreground } else { self.background };
                out.push((rect, color));
            }
        }
        out
    }

    /// The tile nodes of `bounds`, as ordinary filled rectangles (the renderers' and exporters'
    /// shared synthesis).
    pub fn tile_nodes(&self, bounds: Rect) -> Vec<Node> {
        let mut n = 0u64;
        self.tiles(bounds)
            .into_iter()
            .map(|(rect, color)| {
                n += 1;
                let mut node = crate::Node::path(crate::NodeId(0), crate::geom::shapes::rectangle(rect), Default::default());
                node.appearance = crate::Shared::new(crate::Appearance::basic(crate::color::Paint::solid(color), crate::color::Paint::None, 0.0));
                node
            })
            .collect()
    }

    /// Where a texture-filled object's bounds sit (helpers may not need this; kept for symmetry
    /// with the pattern model).
    pub fn period(&self) -> (f64, f64) {
        (CELL * 2.0, CELL * 2.0)
    }
}
