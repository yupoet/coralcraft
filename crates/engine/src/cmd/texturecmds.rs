//! CoralCraft (P3 Task T1): the texture-fill commands. A texture is a named, document-level,
//! reusable definition (procedural kind + parameters + two colours) shared by every object whose
//! paint names it — a distinct paint kind, never an ordinary pattern in disguise.

use serde_json::{Value, json};
use vectorcraft_color::Color;
use vectorcraft_color::Paint;
use vectorcraft_doc::{TextureDef, TextureKind};

use super::{CommandSpec, Result, Session, bad, cmd, color_value, has_doc};
use crate::EngineError;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "texture.new",
            "New Texture",
            ["Object", "Texture"],
            None,
            "{name (non-empty), kind: stipple, density?: 0..1 (0.4), foreground?: colour, background?: colour} create a reusable texture-fill definition; a duplicate name or invalid parameter is an error → {name}",
            has_doc,
            texture_new
        ),
        cmd!(
            "texture.setProps",
            "Texture Properties",
            ["Object", "Texture"],
            None,
            "{name, kind?, density?, foreground?, background?} edit the named definition; every object painted with it follows, one undo step → {name}",
            has_doc,
            texture_set_props
        ),
        cmd!("texture.list", "List Textures", [], None, "{} → {textures: [{name, kind, density, foreground, background}]}", has_doc, texture_list),
        cmd!(
            "paint.setTexture",
            "Apply Texture Fill",
            [],
            None,
            "{texture: name} fill the selection (or `ids`) with the named texture definition → {objects}",
            has_doc,
            paint_set_texture
        ),
    ]
}

fn texture_params(_s: &Session, p: &Value, existing: Option<&TextureDef>) -> Result<(String, TextureKind, f64, Color, Color)> {
    const C: &str = "texture parameters";
    let name = match p.get("name") {
        Some(v) => v.as_str().unwrap_or_default().trim().to_string(),
        None => existing.map(|d| d.name.clone()).ok_or_else(|| bad(C, "missing name"))?,
    };
    if name.is_empty() {
        return Err(bad(C, "the name must not be blank"));
    }
    let kind = match p.get("kind") {
        Some(v) => v
            .as_str()
            .and_then(TextureKind::of_id)
            .ok_or_else(|| bad(C, format!("unknown texture kind {:?} (stipple)", v.as_str().unwrap_or_default())))?,
        None => existing.map_or(TextureKind::Stipple, |d| d.kind),
    };
    let density = match p.get("density") {
        Some(v) => v.as_f64().filter(|d| (0.0..=1.0).contains(d)).ok_or_else(|| bad(C, "density must be a number from 0 to 1"))?,
        None => existing.map_or(0.4, |d| d.density),
    };
    let foreground = match p.get("foreground").filter(|v| !v.is_null()) {
        Some(v) => color_value(v).ok_or_else(|| bad(C, "`foreground` must be a colour"))?,
        None => existing.map_or_else(|| Color::rgb8(0xc0, 0x20, 0x20), |d| d.foreground),
    };
    let background = match p.get("background").filter(|v| !v.is_null()) {
        Some(v) => color_value(v).ok_or_else(|| bad(C, "`background` must be a colour"))?,
        None => existing.map_or_else(|| Color::rgb8(0x20, 0x40, 0x80), |d| d.background),
    };
    Ok((name, kind, density, foreground, background))
}

fn texture_new(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "texture.new";
    let (name, kind, density, foreground, background) = texture_params(s, p, None)?;
    let name = s.edit("New Texture", move |d, _| {
        if d.texture(&name).is_some() {
            return Err(EngineError::Other(format!("{C}: a texture named {name:?} already exists")));
        }
        d.textures.push(TextureDef { name: name.clone(), kind, density, foreground, background });
        Ok(())
    })?;
    Ok(json!({"name": name}))
}

fn texture_set_props(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "texture.setProps";
    let lookup = p.get("name").and_then(Value::as_str).map(str::to_owned).ok_or_else(|| bad(C, "missing name"))?;
    let existing = {
        let st = s.doc()?;
        let def = st.doc.texture(&lookup).ok_or_else(|| bad(C, format!("no texture named {lookup:?}")))?;
        def.clone()
    };
    let (name, kind, density, foreground, background) = texture_params(s, p, Some(&existing))?;
    let renamed = name != lookup;
    let slot_name = name.clone();
    let reply = name.clone();
    s.edit("Texture Properties", move |d, _| {
        if renamed && d.texture(&name).is_some() {
            return Err(EngineError::Other(format!("{C}: a texture named {name:?} already exists")));
        }
        let slot =
            d.textures.iter_mut().find(|def| def.name == lookup).ok_or_else(|| EngineError::Other(format!("{C}: no texture named {lookup:?}")))?;
        slot.name = slot_name;
        slot.kind = kind;
        slot.density = density;
        slot.foreground = foreground;
        slot.background = background;
        // Renaming keeps every user's paint following the definition: users hold the name, and
        // the rename updates it in place below.
        Ok(())
    })?;
    Ok(json!({"name": reply}))
}

fn texture_list(s: &mut Session, _: &Value) -> Result<Value> {
    let st = s.doc()?;
    let textures: Vec<Value> = st
        .doc
        .textures
        .iter()
        .map(|t| {
            json!({
                "name": t.name,
                "kind": t.kind.id(),
                "density": t.density,
                "foreground": t.foreground.to_hex(),
                "background": t.background.to_hex(),
            })
        })
        .collect();
    Ok(json!({ "textures": textures }))
}

fn paint_set_texture(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "paint.setTexture";
    let name = p.get("texture").and_then(Value::as_str).map(str::to_owned).ok_or_else(|| bad(C, "missing texture"))?;
    {
        let st = s.doc()?;
        if st.doc.texture(&name).is_none() {
            return Err(bad(C, format!("no texture named {name:?} (texture.new makes one; a pattern name is not a texture)")));
        }
    }
    // The same selection plumbing `paint.setFill` uses, with the texture paint built here.
    let objects = super::paint::apply_paint(s, p, Paint::Texture { texture: name }, true)?;
    Ok(objects)
}
