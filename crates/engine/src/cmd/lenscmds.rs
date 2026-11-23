//! CoralCraft (P3 Task E4): the colour lenses. A lens is a filled shape whose fill, blend mode
//! and opacity carry Corel's kind over the shared paint + merge-mode architecture — invert is
//! white with Difference, brighten white with Lighten, darken black with Darken, lighten white
//! with Screen, tintedGrayscale the tint colour with Color — at the lens's amount (its opacity
//! share). The backdrop-distorting lenses (Magnify, Fish Eye, Heat Map, Posterize, Find Edges)
//! have no channel in the formats we write and are refused by name.

use serde_json::{Value, json};
use vectorcraft_color::{BlendMode, Color, Paint};
use vectorcraft_doc::AppearanceItem;

use super::appearance::{appearance_targets, item_target};
use super::{CommandSpec, EngineError, Result, Session, bad, cmd, has_doc};

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        "object.lens",
        "Lens",
        ["Object", "Lens"],
        None,
        "{ids?|id?, kind: invert|brighten|darken|lighten|tintedGrayscale, amount?: % 0..100 (50), color?: colour (the kind's own: white, black, white, white, black)} make the shape a colour lens: its fill, blend and opacity carry the kind over what is under it, one undo step; magnify, fishEye, heatMap, posterize and findEdges are refused (no channel in the formats we write)",
        has_doc,
        object_lens
    )]
}

/// The kind's mapping: the fill colour's default and the blend mode that carries the lens.
fn mapping(kind: &str) -> Option<(Color, BlendMode)> {
    Some(match kind {
        "invert" => (Color::WHITE, BlendMode::Difference),
        "brighten" => (Color::WHITE, BlendMode::Lighten),
        "darken" => (Color::BLACK, BlendMode::Darken),
        "lighten" => (Color::WHITE, BlendMode::Screen),
        "tintedGrayscale" => (Color::BLACK, BlendMode::Color),
        _ => return None,
    })
}

fn object_lens(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "object.lens";
    let kind = p.get("kind").and_then(Value::as_str).ok_or_else(|| bad(C, "missing kind"))?;
    let Some((default_color, blend)) = mapping(kind) else {
        let known = if matches!(kind, "magnify" | "fishEye" | "heatMap" | "posterize" | "findEdges") {
            format!(
                "{kind:?} distorts or remaps the art under the lens, which no format we write carries; the colour lenses are invert, brighten, darken, lighten and tintedGrayscale"
            )
        } else {
            format!("unknown lens kind {kind:?} (invert, brighten, darken, lighten, tintedGrayscale)")
        };
        return Err(bad(C, known));
    };
    let amount = match p.get("amount") {
        Some(v) => v.as_f64().filter(|a| (0.0..=100.0).contains(a)).ok_or_else(|| bad(C, "amount must be a number from 0 to 100"))?,
        None => 50.0,
    };
    let color = match p.get("color").filter(|v| !v.is_null()) {
        Some(v) => v.as_str().and_then(Color::from_hex).ok_or_else(|| bad(C, "color must be a colour"))?,
        None => default_color,
    };
    let opacity = (amount / 100.0) as f32;
    let ids = appearance_targets(s, p)?;
    if ids.is_empty() {
        return Err(EngineError::Other(format!("{C}: select the shape to make a lens")));
    }
    let item = item_target(s, p, C)?;
    super::appearance::edit_items(s, &ids, item, C, "Lens", true, |n, item| {
        let items = &mut n.appearance.items;
        let slot = match item {
            Some(index) => Some(&mut items[index]),
            None => {
                items.push(AppearanceItem::Fill(vectorcraft_doc::FillLayer::new(Paint::solid(color))));
                items.last_mut()
            }
        };
        let Some(vectorcraft_doc::AppearanceItem::Fill(fill)) = slot else {
            return Err(EngineError::Other(format!("{C}: the target has no fill to make a lens of")));
        };
        fill.paint = Paint::solid(color);
        fill.opacity = opacity;
        fill.blend = blend;
        Ok(())
    })?;
    Ok(json!({ "objects": ids.len() }))
}
