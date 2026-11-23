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

/// Most cells one bounded synthesis may make, matching the pattern instance budget.
const MAX_TILES: i64 = 250_000;

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
    /// The definition's tiles over `bounds`: one rectangle per cell, in its colour. Non-finite,
    /// inverted or integer-unrepresentable bounds, and grids over [`MAX_TILES`], produce none.
    pub fn tiles(&self, bounds: Rect) -> Vec<(Rect, Color)> {
        let index = |v: f64| (v.abs() < 9.0e18).then_some(v as i64);
        let (Some(x0), Some(x1), Some(y0), Some(y1)) = (
            index((bounds.x0 / CELL).floor()),
            index((bounds.x1 / CELL).ceil()),
            index((bounds.y0 / CELL).floor()),
            index((bounds.y1 / CELL).ceil()),
        ) else {
            return vec![];
        };
        let (nx, ny) = (x1.saturating_sub(x0), y1.saturating_sub(y0));
        if nx < 1 || ny < 1 || nx.saturating_mul(ny) > MAX_TILES {
            return vec![];
        }
        let mut out = Vec::with_capacity((nx * ny) as usize);
        for i in x0..x1 {
            for j in y0..y1 {
                let left = i as f64 * CELL;
                let top = j as f64 * CELL;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn definition() -> TextureDef {
        TextureDef {
            name: "Coral Stipple".into(),
            kind: TextureKind::Stipple,
            density: 0.4,
            foreground: Color::rgb8(0xc0, 0x20, 0x20),
            background: Color::rgb8(0x20, 0x40, 0x80),
        }
    }

    #[test]
    fn tile_synthesis_refuses_more_than_the_instance_budget() {
        let bounds = Rect::new(0.0, 0.0, CELL * 501.0, CELL * 500.0);
        assert!(definition().tiles(bounds).is_empty());
    }

    #[test]
    fn tile_synthesis_refuses_non_finite_bounds() {
        let bounds = Rect::new(f64::NAN, 0.0, CELL, CELL);
        assert!(definition().tiles(bounds).is_empty());
    }
}
