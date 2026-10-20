//! Clean-room CorelDRAW XML palette (`.xml`) parser/serializer, `.cpl` binary header detector,
//! and default workspace palette.

use serde::{Deserialize, Serialize};

use crate::spaces::{Color, ColorError};

/// Maximum number of swatches parsed from an untrusted palette file.
pub const MAX_PALETTE_SWATCHES: usize = 16_384;

/// A named color swatch in a [`Palette`].
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PaletteSwatch {
    /// Display name of the swatch.
    pub name: String,
    /// Authoring color value.
    pub color: Color,
}

/// A color palette (loaded from `.xml` or constructed as the default workspace palette).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Palette {
    /// Human-readable palette name.
    pub name: String,
    /// Optional palette GUID from XML attributes.
    pub guid: Option<String>,
    /// Ordered list of swatches.
    pub swatches: Vec<PaletteSwatch>,
}

impl Palette {
    /// Constructs CoralCraft's clean-room default CMYK/RGB workspace palette for the right-hand
    /// vertical Color Palette Bar.
    #[must_use]
    pub fn default_workspace_palette() -> Self {
        let entries: &[(&str, Color)] = &[
            ("Black", Color::Rgb { r: 0, g: 0, b: 0 }),
            ("Dark Gray", Color::Rgb { r: 64, g: 64, b: 64 }),
            ("Medium Gray", Color::Rgb { r: 128, g: 128, b: 128 }),
            ("Light Gray", Color::Rgb { r: 192, g: 192, b: 192 }),
            ("White", Color::Rgb { r: 255, g: 255, b: 255 }),
            ("Cyan", Color::Rgb { r: 0, g: 174, b: 239 }),
            ("Magenta", Color::Rgb { r: 236, g: 0, b: 140 }),
            ("Yellow", Color::Rgb { r: 255, g: 242, b: 0 }),
            ("Red", Color::Rgb { r: 237, g: 28, b: 36 }),
            ("Green", Color::Rgb { r: 0, g: 166, b: 81 }),
            ("Blue", Color::Rgb { r: 46, g: 49, b: 146 }),
            ("Orange", Color::Rgb { r: 247, g: 148, b: 29 }),
            ("Purple", Color::Rgb { r: 102, g: 45, b: 145 }),
            ("Teal", Color::Rgb { r: 0, g: 169, b: 157 }),
            ("Registration", Color::Registration),
        ];
        Self {
            name: "CoralCraft Default CMYK/RGB".to_owned(),
            guid: None,
            swatches: entries.iter().map(|(n, c)| PaletteSwatch { name: (*n).to_owned(), color: c.clone() }).collect(),
        }
    }

    /// Finds a swatch by 0-based index string or by case-insensitive swatch name.
    pub fn resolve_swatch(&self, query: &str) -> Result<(usize, &PaletteSwatch), ColorError> {
        if let Ok(idx) = query.parse::<usize>()
            && let Some(sw) = self.swatches.get(idx)
        {
            return Ok((idx, sw));
        }
        for (i, sw) in self.swatches.iter().enumerate() {
            if sw.name.eq_ignore_ascii_case(query) {
                return Ok((i, sw));
            }
        }
        Err(ColorError::SwatchNotFound(query.to_owned()))
    }
}

/// Result of inspecting a candidate binary `.cpl` palette header.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CplDetection {
    /// Recognized binary `.cpl` header with version marker and declared swatch count.
    Cpl {
        /// Header version marker (e.g. `0xCDCD`, `0xDCDC`, `0xCC80`, `0xCC90`).
        version: u16,
        /// Declared number of color records in the palette.
        color_count: u16,
    },
    /// Not a binary `.cpl` file.
    NotCpl,
}

/// Detects whether `bytes` starts with a known CorelDRAW binary `.cpl` palette header.
///
/// Never triggers false positives on XML (`<`) or JSON (`{`) text files.
#[must_use]
pub fn detect_cpl(bytes: &[u8]) -> CplDetection {
    let [b0, b1, b2, b3, _, _, _, _, ..] = *bytes else {
        return CplDetection::NotCpl;
    };
    // Reject XML / JSON / UTF-8 BOM text files immediately
    let trimmed = strip_bom_and_ascii_ws(bytes);
    if trimmed.starts_with(b"<") || trimmed.starts_with(b"{") {
        return CplDetection::NotCpl;
    }

    let magic = u16::from_le_bytes([b0, b1]);
    let known_magics: &[u16] = &[0xCDCD, 0xDCDC, 0xCDDC, 0xCCDC, 0xCC80, 0xCC90, 0xCDD0, 0xCD01];
    if known_magics.contains(&magic) {
        let count = u16::from_le_bytes([b2, b3]);
        if count <= 4096 {
            return CplDetection::Cpl { version: magic, color_count: count };
        }
    }

    CplDetection::NotCpl
}

/// Loads a palette from raw bytes, automatically dispatching XML palettes vs binary `.cpl` files.
pub fn load_palette_bytes(bytes: &[u8]) -> Result<Palette, ColorError> {
    if let CplDetection::Cpl { .. } = detect_cpl(bytes) {
        return parse_binary_cpl(bytes);
    }
    let text = std::str::from_utf8(strip_bom_and_ascii_ws(bytes)).map_err(|_| ColorError::UnknownPaletteFormat)?;
    if !text.starts_with('<') {
        return Err(ColorError::UnknownPaletteFormat);
    }
    parse_xml_palette(text)
}

/// Parses a binary CorelDRAW `.cpl` palette (`0xCDDC` / `0xCCDC` / `0xCDCD` header + Pascal palette name +
/// `color_count` records of `[model: u16 LE, pad: u16, c0: u8, c1: u8, c2: u8, c3: u8, name_len: u8, name_bytes...]`).
///
/// Returns [`ColorError::UnsupportedBinaryCpl`] when the binary payload is truncated or uses an unknown sub-layout.
pub fn parse_binary_cpl(bytes: &[u8]) -> Result<Palette, ColorError> {
    let CplDetection::Cpl { color_count, .. } = detect_cpl(bytes) else {
        return Err(ColorError::UnknownPaletteFormat);
    };
    let count = usize::from(color_count);
    if count > MAX_PALETTE_SWATCHES || bytes.len() < 9 {
        return Err(ColorError::UnsupportedBinaryCpl);
    }
    let after_magic = bytes.get(4..).ok_or(ColorError::UnsupportedBinaryCpl)?;
    let (&pal_name_len_u8, after_len) = after_magic.split_first().ok_or(ColorError::UnsupportedBinaryCpl)?;
    let pal_name_len = usize::from(pal_name_len_u8);
    let pal_name_bytes = after_len.get(..pal_name_len).ok_or(ColorError::UnsupportedBinaryCpl)?;
    let mut rest = after_len.get(pal_name_len..).ok_or(ColorError::UnsupportedBinaryCpl)?;
    let pal_name_raw = std::str::from_utf8(pal_name_bytes).map_err(|_| ColorError::UnsupportedBinaryCpl)?;
    let palette_name = if pal_name_raw.is_empty() { "Binary CPL Palette".to_owned() } else { pal_name_raw.to_owned() };

    if count > 0 && rest.is_empty() {
        return Err(ColorError::UnsupportedBinaryCpl);
    }

    let mut swatches = Vec::with_capacity(count);
    for idx in 0..count {
        let [m0, m1, _, _, c0, c1, c2, c3, name_len_u8, ..] = *rest else {
            return Err(ColorError::UnsupportedBinaryCpl);
        };
        let model = u16::from_le_bytes([m0, m1]);
        let name_len = usize::from(name_len_u8);
        let after_hdr = rest.get(9..).ok_or(ColorError::UnsupportedBinaryCpl)?;
        let name_bytes = after_hdr.get(..name_len).ok_or(ColorError::UnsupportedBinaryCpl)?;
        rest = after_hdr.get(name_len..).ok_or(ColorError::UnsupportedBinaryCpl)?;
        let name_str = std::str::from_utf8(name_bytes).map_err(|_| ColorError::UnsupportedBinaryCpl)?;
        let name = if name_str.is_empty() { format!("Swatch {}", idx.saturating_add(1)) } else { name_str.to_owned() };

        let color = match model {
            1 | 5 => Color::rgb8(c0, c1, c2),
            2..=4 => Color::cmyk(
                (f64::from(c0) / 100.0).clamp(0.0, 1.0),
                (f64::from(c1) / 100.0).clamp(0.0, 1.0),
                (f64::from(c2) / 100.0).clamp(0.0, 1.0),
                (f64::from(c3) / 100.0).clamp(0.0, 1.0),
            )?,
            6 => Color::Registration,
            _ => return Err(ColorError::UnsupportedBinaryCpl),
        };
        swatches.push(PaletteSwatch { name, color });
    }

    Ok(Palette { name: palette_name, guid: None, swatches })
}

/// Serializes a [`Palette`] into a clean-room binary `.cpl` palette (`0xCDDC` magic).
#[must_use]
pub fn serialize_binary_cpl(palette: &Palette) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&0xCDDCu16.to_le_bytes());
    let count = u16::try_from(palette.swatches.len()).unwrap_or(u16::MAX);
    out.extend_from_slice(&count.to_le_bytes());
    let pal_name_bytes = palette.name.as_bytes();
    let pal_name_slice = pal_name_bytes.get(..255).unwrap_or(pal_name_bytes);
    out.push(u8::try_from(pal_name_slice.len()).unwrap_or(255));
    out.extend_from_slice(pal_name_slice);

    for sw in palette.swatches.iter().take(usize::from(count)) {
        match &sw.color {
            Color::Rgb { r, g, b } => {
                out.extend_from_slice(&5u16.to_le_bytes());
                out.extend_from_slice(&0u16.to_le_bytes());
                out.extend_from_slice(&[*r, *g, *b, 0]);
            }
            Color::Cmyk { c, m, y, k } => {
                out.extend_from_slice(&2u16.to_le_bytes());
                out.extend_from_slice(&0u16.to_le_bytes());
                out.extend_from_slice(&[
                    (c.get() * 100.0).round().clamp(0.0, 100.0) as u8,
                    (m.get() * 100.0).round().clamp(0.0, 100.0) as u8,
                    (y.get() * 100.0).round().clamp(0.0, 100.0) as u8,
                    (k.get() * 100.0).round().clamp(0.0, 100.0) as u8,
                ]);
            }
            Color::Registration => {
                out.extend_from_slice(&6u16.to_le_bytes());
                out.extend_from_slice(&0u16.to_le_bytes());
                out.extend_from_slice(&[100, 100, 100, 100]);
            }
            other => {
                let [r, g, b] = other.to_srgb8();
                out.extend_from_slice(&5u16.to_le_bytes());
                out.extend_from_slice(&0u16.to_le_bytes());
                out.extend_from_slice(&[r, g, b, 0]);
            }
        }
        let sw_name = sw.name.as_bytes();
        let sw_slice = sw_name.get(..255).unwrap_or(sw_name);
        out.push(u8::try_from(sw_slice.len()).unwrap_or(255));
        out.extend_from_slice(sw_slice);
    }
    out
}

fn strip_bom_and_ascii_ws(bytes: &[u8]) -> &[u8] {
    let b = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    b.trim_ascii_start()
}

/// Parses a clean-room XML palette (`<palette name="..."><colors><cs .../></colors></palette>`).
///
/// Streaming tag scanner with zero DTD/entity expansion (XXE-safe).
pub fn parse_xml_palette(xml: &str) -> Result<Palette, ColorError> {
    if xml.contains("<!DOCTYPE") || xml.contains("<!ENTITY") {
        return Err(ColorError::XmlParseError("DTD and ENTITY declarations are forbidden".to_owned()));
    }

    let mut palette_name = String::from("Untitled Palette");
    let mut palette_guid = None;
    let mut found_palette_tag = false;
    let mut swatches = Vec::new();

    let mut rest = xml;
    while let Some((_, after_lt)) = rest.split_once('<') {
        rest = after_lt;
        if let Some(comment_body) = rest.strip_prefix("!--") {
            let Some((_, after_comment)) = comment_body.split_once("-->") else {
                return Err(ColorError::XmlParseError("unclosed XML comment".to_owned()));
            };
            rest = after_comment;
            continue;
        }
        if let Some(pi_body) = rest.strip_prefix('?') {
            let Some((_, after_pi)) = pi_body.split_once("?>") else {
                return Err(ColorError::XmlParseError("unclosed XML processing instruction".to_owned()));
            };
            rest = after_pi;
            continue;
        }

        let Some((tag_raw, after_gt)) = rest.split_once('>') else {
            return Err(ColorError::XmlParseError("unclosed XML tag".to_owned()));
        };
        let tag_body = tag_raw.trim();
        rest = after_gt;

        if tag_body.starts_with('/') {
            continue;
        }

        let tag_body = tag_body.strip_suffix('/').map_or(tag_body, str::trim);
        let (tag_name, attrs_str) = split_tag_name(tag_body);
        match tag_name {
            "palette" => {
                found_palette_tag = true;
                let attrs = parse_attributes(attrs_str)?;
                for (k, v) in attrs {
                    if k == "name" {
                        palette_name = v;
                    } else if k == "guid" {
                        palette_guid = Some(v);
                    }
                }
            }
            "cs" | "color" => {
                if swatches.len() >= MAX_PALETTE_SWATCHES {
                    return Err(ColorError::XmlParseError("palette exceeds maximum swatch count".to_owned()));
                }
                let attrs = parse_attributes(attrs_str)?;
                let swatch = swatch_from_attrs(&attrs)?;
                swatches.push(swatch);
            }
            _ => {}
        }
    }

    if !found_palette_tag {
        return Err(ColorError::XmlParseError("missing <palette> root element".to_owned()));
    }

    Ok(Palette { name: palette_name, guid: palette_guid, swatches })
}

/// Serializes a [`Palette`] into canonical, deterministic UTF-8 XML (`<palette><colors><cs .../></colors></palette>`).
#[must_use]
pub fn serialize_xml_palette(palette: &Palette) -> String {
    let mut out = String::new();
    out.push_str("<palette name=\"");
    push_xml_escaped(&mut out, &palette.name);
    out.push('"');
    if let Some(guid) = &palette.guid {
        out.push_str(" guid=\"");
        push_xml_escaped(&mut out, guid);
        out.push('"');
    }
    out.push_str("><colors>");

    for sw in &palette.swatches {
        out.push_str("<cs ");
        match &sw.color {
            &Color::Rgb { r, g, b } => {
                out.push_str("type=\"1\" name=\"");
                push_xml_escaped(&mut out, &sw.name);
                out.push_str(&format!("\" r=\"{r}\" g=\"{g}\" b=\"{b}\"/>"));
            }
            &Color::Cmyk { c, m, y, k } => {
                out.push_str("type=\"2\" name=\"");
                push_xml_escaped(&mut out, &sw.name);
                out.push_str(&format!(
                    "\" c=\"{}\" m=\"{}\" y=\"{}\" k=\"{}\"/>",
                    c.get() * 100.0,
                    m.get() * 100.0,
                    y.get() * 100.0,
                    k.get() * 100.0
                ));
            }
            &Color::Hsb { h, s, b } => {
                out.push_str("type=\"3\" name=\"");
                push_xml_escaped(&mut out, &sw.name);
                out.push_str(&format!("\" h=\"{}\" s=\"{}\" b=\"{}\"/>", h.get(), s.get() * 100.0, b.get() * 100.0));
            }
            &Color::LabD50 { l, a, b } => {
                out.push_str("type=\"4\" name=\"");
                push_xml_escaped(&mut out, &sw.name);
                out.push_str(&format!("\" l=\"{}\" a=\"{}\" b=\"{}\"/>", l.get(), a.get(), b.get()));
            }
            &Color::LabD65 { l, a, b } => {
                out.push_str("type=\"4\" illuminant=\"D65\" name=\"");
                push_xml_escaped(&mut out, &sw.name);
                out.push_str(&format!("\" l=\"{}\" a=\"{}\" b=\"{}\"/>", l.get(), a.get(), b.get()));
            }
            Color::Spot { name: spot_name, tint, fallback_cmyk } => {
                out.push_str("type=\"5\" name=\"");
                push_xml_escaped(&mut out, &sw.name);
                if spot_name != &sw.name {
                    out.push_str("\" ink=\"");
                    push_xml_escaped(&mut out, spot_name);
                }
                out.push_str(&format!(
                    "\" c=\"{}\" m=\"{}\" y=\"{}\" k=\"{}\" tint=\"{}\"/>",
                    fallback_cmyk[0].get() * 100.0,
                    fallback_cmyk[1].get() * 100.0,
                    fallback_cmyk[2].get() * 100.0,
                    fallback_cmyk[3].get() * 100.0,
                    tint.get() * 100.0
                ));
            }
            Color::Registration => {
                out.push_str("type=\"6\" name=\"");
                push_xml_escaped(&mut out, &sw.name);
                out.push_str("\"/>");
            }
            &Color::Gray { k } => {
                out.push_str("type=\"7\" name=\"");
                push_xml_escaped(&mut out, &sw.name);
                out.push_str(&format!("\" k=\"{}\"/>", k.get() * 100.0));
            }
        }
    }

    out.push_str("</colors></palette>");
    out
}

fn split_tag_name(s: &str) -> (&str, &str) {
    if let Some((name, rest)) = s.split_once(char::is_whitespace) { (name, rest.trim()) } else { (s, "") }
}

fn parse_attributes(mut s: &str) -> Result<Vec<(String, String)>, ColorError> {
    let mut attrs = Vec::new();
    while !s.is_empty() {
        s = s.trim_start();
        if s.is_empty() {
            break;
        }
        let Some((key_raw, after_eq)) = s.split_once('=') else {
            return Err(ColorError::XmlParseError(format!("malformed attribute in `{s}`")));
        };
        let key = key_raw.trim().to_owned();
        s = after_eq.trim_start();
        let (quote, after_open_q) = if let Some(rest_q) = s.strip_prefix('"') {
            ('"', rest_q)
        } else if let Some(rest_q) = s.strip_prefix('\'') {
            ('\'', rest_q)
        } else {
            return Err(ColorError::XmlParseError(format!("missing quote for attribute `{key}`")));
        };
        let Some((val_raw, after_close_q)) = after_open_q.split_once(quote) else {
            return Err(ColorError::XmlParseError(format!("unclosed quote for attribute `{key}`")));
        };
        let val = unescape_xml(val_raw);
        attrs.push((key, val));
        s = after_close_q;
    }
    Ok(attrs)
}

fn swatch_from_attrs(attrs: &[(String, String)]) -> Result<PaletteSwatch, ColorError> {
    let get = |k: &str| -> Option<&str> { attrs.iter().find(|(ak, _)| ak.eq_ignore_ascii_case(k)).map(|(_, v)| v.as_str()) };
    let get_f64 = |k: &str| -> Result<Option<f64>, ColorError> {
        let Some(s) = get(k) else {
            return Ok(None);
        };
        let v = s.parse::<f64>().map_err(|_| ColorError::XmlParseError(format!("invalid numeric attribute `{k}=\"{s}\"`")))?;
        if !v.is_finite() {
            return Err(ColorError::NonFiniteComponent);
        }
        Ok(Some(v))
    };

    let name = get("name").unwrap_or("Unnamed").to_owned();
    let cs_type = get("type").unwrap_or("");

    let norm_pct = |v: f64| -> f64 {
        let scale = if v > 100.0 { 255.0 } else { 100.0 };
        (v / scale).clamp(0.0, 1.0)
    };

    let color = match cs_type {
        "1" => {
            let r = get_f64("r")?.unwrap_or(0.0).round().clamp(0.0, 255.0) as u8;
            let g = get_f64("g")?.unwrap_or(0.0).round().clamp(0.0, 255.0) as u8;
            let b = get_f64("b")?.unwrap_or(0.0).round().clamp(0.0, 255.0) as u8;
            Color::Rgb { r, g, b }
        }
        "2" => {
            let raw_c = get_f64("c")?.unwrap_or(0.0);
            let raw_m = get_f64("m")?.unwrap_or(0.0);
            let raw_y = get_f64("y")?.unwrap_or(0.0);
            let raw_k = get_f64("k")?.unwrap_or(0.0);
            let max_v = raw_c.max(raw_m).max(raw_y).max(raw_k);
            let scale = if max_v > 100.0 { 255.0 } else { 100.0 };
            Color::cmyk(
                (raw_c / scale).clamp(0.0, 1.0),
                (raw_m / scale).clamp(0.0, 1.0),
                (raw_y / scale).clamp(0.0, 1.0),
                (raw_k / scale).clamp(0.0, 1.0),
            )?
        }
        "3" => {
            let h = get_f64("h")?.unwrap_or(0.0);
            let s = norm_pct(get_f64("s")?.unwrap_or(0.0));
            let b = norm_pct(get_f64("b")?.unwrap_or(0.0));
            Color::hsb(h, s, b)?
        }
        "4" => {
            let l = get_f64("l")?.unwrap_or(0.0);
            let a = get_f64("a")?.unwrap_or(0.0);
            let b = get_f64("b")?.unwrap_or(0.0);
            if get("illuminant").is_some_and(|ill| ill.eq_ignore_ascii_case("D65")) { Color::lab_d65(l, a, b)? } else { Color::lab_d50(l, a, b)? }
        }
        "5" => {
            let c = norm_pct(get_f64("c")?.unwrap_or(0.0));
            let m = norm_pct(get_f64("m")?.unwrap_or(0.0));
            let y = norm_pct(get_f64("y")?.unwrap_or(0.0));
            let k = norm_pct(get_f64("k")?.unwrap_or(0.0));
            let tint = norm_pct(get_f64("tint")?.unwrap_or(100.0));
            let ink_name = get("ink").unwrap_or(&name).to_owned();
            Color::spot(ink_name, tint, [c, m, y, k])?
        }
        "6" => Color::Registration,
        "7" => {
            let k = norm_pct(get_f64("k")?.unwrap_or(0.0));
            Color::gray(k)?
        }
        _ => {
            if get("c").is_some() || get("m").is_some() || get("y").is_some() {
                let c = norm_pct(get_f64("c")?.unwrap_or(0.0));
                let m = norm_pct(get_f64("m")?.unwrap_or(0.0));
                let y = norm_pct(get_f64("y")?.unwrap_or(0.0));
                let k = norm_pct(get_f64("k")?.unwrap_or(0.0));
                Color::cmyk(c, m, y, k)?
            } else {
                let r = get_f64("r")?.unwrap_or(0.0).round().clamp(0.0, 255.0) as u8;
                let g = get_f64("g")?.unwrap_or(0.0).round().clamp(0.0, 255.0) as u8;
                let b = get_f64("b")?.unwrap_or(0.0).round().clamp(0.0, 255.0) as u8;
                Color::Rgb { r, g, b }
            }
        }
    };

    Ok(PaletteSwatch { name, color })
}

fn push_xml_escaped(out: &mut String, s: &str) {
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
}

fn unescape_xml(s: &str) -> String {
    s.replace("&quot;", "\"").replace("&apos;", "'").replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&")
}
