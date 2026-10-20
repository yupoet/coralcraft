//! Fill, Outline, and opaque `RawPayload` / `Residual` chunk capture types for `coralcraft-doc`.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use coralcraft_color::{Color, FountainBlendMode, FountainStop};
use coralcraft_geom::F64;

/// 4-byte RIFF/CDR chunk identifier (e.g. `b"outl"`, `b"fild"`, `b"loda"`),
/// serialized as an 8-character lowercase hex string so non-ASCII tags round-trip losslessly.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChunkTag(pub [u8; 4]);

impl ChunkTag {
    /// Creates a `ChunkTag` from a 4-byte ASCII/binary tag.
    #[must_use]
    pub const fn new(bytes: [u8; 4]) -> Self {
        Self(bytes)
    }

    /// Formats the tag as a printable 4-char ASCII string (or `0x...` hex if non-ASCII).
    #[must_use]
    pub fn display_ascii(self) -> String {
        if self.0.iter().all(|b| (0x20..=0x7E).contains(b)) {
            String::from_utf8_lossy(&self.0).into_owned()
        } else {
            format!("0x{:02x}{:02x}{:02x}{:02x}", self.0[0], self.0[1], self.0[2], self.0[3])
        }
    }
}

impl Serialize for ChunkTag {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let hex = format!("{:02x}{:02x}{:02x}{:02x}", self.0[0], self.0[1], self.0[2], self.0[3]);
        serializer.serialize_str(&hex)
    }
}

impl<'de> Deserialize<'de> for ChunkTag {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        let bytes = decode_hex(&s).map_err(serde::de::Error::custom)?;
        let Ok(arr) = <[u8; 4]>::try_from(bytes.as_slice()) else {
            return Err(serde::de::Error::custom("ChunkTag hex must be 8 hex digits (4 bytes)"));
        };
        Ok(Self(arr))
    }
}

/// In-memory opaque binary payload (`Vec<u8>`).
///
/// On disk, serialized inside the reserved `{"$payload": {"enc": "hex" | "blob", ...}}` envelope
/// so that `load_bytes(&save_zip(&doc)?)? == load_bytes(&save_json(&doc)?)? == doc` holds structurally.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct RawPayload(pub Vec<u8>);

#[derive(Serialize, Deserialize)]
struct PayloadEnvelope {
    #[serde(rename = "$payload")]
    inner: PayloadDisk,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "enc", rename_all = "snake_case")]
enum PayloadDisk {
    Hex { data: String },
    Blob { id: String, len: u64 },
}

impl Serialize for RawPayload {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let env = PayloadEnvelope { inner: PayloadDisk::Hex { data: encode_hex(&self.0) } };
        env.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for RawPayload {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let env = PayloadEnvelope::deserialize(deserializer)?;
        match env.inner {
            PayloadDisk::Hex { data } => {
                let bytes = decode_hex(&data).map_err(serde::de::Error::custom)?;
                Ok(Self(bytes))
            }
            PayloadDisk::Blob { .. } => Err(serde::de::Error::custom("unresolved blob reference in JSON mode (E_BLOB_IN_JSON)")),
        }
    }
}

/// Encodes `bytes` as lowercase hexadecimal.
#[must_use]
pub fn encode_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().saturating_mul(2));
    for &b in bytes {
        out.push(char::from_digit(u32::from(b >> 4), 16).unwrap_or('0'));
        out.push(char::from_digit(u32::from(b & 0x0F), 16).unwrap_or('0'));
    }
    out
}

/// Decodes a lowercase/uppercase hexadecimal string into bytes.
pub fn decode_hex(s: &str) -> Result<Vec<u8>, String> {
    if !s.len().is_multiple_of(2) {
        return Err(format!("odd hex string length {}", s.len()));
    }
    let mut out = Vec::with_capacity(s.len() / 2);
    let (pairs, _) = s.as_bytes().as_chunks::<2>();
    for (idx, &[b0, b1]) in pairs.iter().enumerate() {
        let i = idx.saturating_mul(2);
        let hi = hex_nibble(b0).ok_or_else(|| format!("invalid hex byte at {i}"))?;
        let lo = hex_nibble(b1).ok_or_else(|| format!("invalid hex byte at {}", i.saturating_add(1)))?;
        out.push((hi << 4) | lo);
    }
    Ok(out)
}

const fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b.saturating_sub(b'0')),
        b'a'..=b'f' => Some(b.saturating_sub(b'a').saturating_add(10)),
        b'A'..=b'F' => Some(b.saturating_sub(b'A').saturating_add(10)),
        _ => None,
    }
}

/// An unparsed or unknown CDR record preserved verbatim for lossless round-trip.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RawChunk {
    /// 4CC chunk tag.
    pub tag: ChunkTag,
    /// CDR record flags affecting interpretation.
    pub flags: u16,
    /// Number of known sibling records preceding this chunk in the source stream.
    pub slot: u32,
    /// Verbatim payload bytes.
    pub payload: RawPayload,
    /// Byte offset in the original source stream (`0` for natively created records).
    pub src_offset: u64,
}

/// Ordered collection of residual [`RawChunk`]s attached to a document, page, layer, or object.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Residual {
    /// Captured unknown chunks in stream order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chunks: Vec<RawChunk>,
}

impl Residual {
    /// Returns `true` if no residual chunks are present.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }
}

/// Geometric layout of a CorelDRAW Fountain Fill in normalized `[0, 1]` object-bounding-box space.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FountainGeometry {
    /// Linear fountain fill rotated by `angle_deg` with `edge_pad` (`0.0..=0.49`).
    Linear {
        /// Rotation angle in degrees (`0.0` = left-to-right).
        angle_deg: F64,
        /// Edge pad fraction (`0.0..=0.49`).
        edge_pad: F64,
    },
    /// Radial / elliptical fountain fill in unit-square `[0, 1]` object coordinates.
    Radial {
        /// Center X in `[0, 1]` unit square (`0.5` = center).
        center_x: F64,
        /// Center Y in `[0, 1]` unit square (`0.5` = center).
        center_y: F64,
        /// Outer radius in unit square (`0.5` = inscribed circle).
        radius: F64,
        /// Focal point X in `[0, 1]` unit square.
        focal_x: F64,
        /// Focal point Y in `[0, 1]` unit square.
        focal_y: F64,
    },
    /// Conical (angular sweep) fountain fill in unit-square `[0, 1]` object coordinates.
    Conical {
        /// Center X in `[0, 1]` unit square (`0.5` = center).
        center_x: F64,
        /// Center Y in `[0, 1]` unit square (`0.5` = center).
        center_y: F64,
        /// Starting sweep angle in degrees.
        angle_deg: F64,
    },
    /// Square (Chebyshev / diamond) fountain fill in unit-square `[0, 1]` object coordinates.
    Square {
        /// Center X in `[0, 1]` unit square (`0.5` = center).
        center_x: F64,
        /// Center Y in `[0, 1]` unit square (`0.5` = center).
        center_y: F64,
        /// Half-side extent in unit square (`0.5` = inscribed square).
        radius: F64,
        /// Rotation angle in degrees.
        angle_deg: F64,
    },
}

/// Preset two-color geometric pattern kind in CorelDRAW's Pattern Fill dialog.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatternKind {
    /// 2×2 alternating foreground/background checkerboard tile.
    #[default]
    Checkerboard,
    /// Foreground circular polka dot centered in the background tile.
    PolkaDot,
    /// Alternating horizontal foreground/background bands.
    HorizontalStripe,
    /// Alternating vertical foreground/background bands.
    VerticalStripe,
    /// Diagonal 45° and 135° crosshatch grid lines.
    DiagonalCrosshatch,
    /// Forward-compatible fallback for pattern kinds introduced in newer `.coralcraft` schemas.
    #[serde(other)]
    Unknown,
}

/// Object or page fill style.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "fill_type", rename_all = "snake_case")]
pub enum FillStyle {
    /// Solid uniform color fill.
    Uniform {
        /// Fill color.
        color: Color,
    },
    /// CorelDRAW Fountain Fill (linear or radial gradient).
    Fountain {
        /// Normalized geometric layout.
        geometry: FountainGeometry,
        /// Color blend mode (`Direct`, `RainbowCw`, `RainbowCcw`).
        blend_mode: FountainBlendMode,
        /// Ordered gradient stops.
        stops: Vec<FountainStop>,
    },
    /// CorelDRAW Two-Color Pattern Fill.
    TwoColorPattern {
        /// Geometric tile pattern kind.
        kind: PatternKind,
        /// Foreground pattern ink color.
        fg_color: Color,
        /// Background pattern tile color.
        bg_color: Color,
        /// Repeating square tile size in typographic points (`> 0.0`).
        tile_size_pt: F64,
    },
    /// Unimplemented CDR fill chunk preserved verbatim.
    Unknown {
        /// 4CC chunk tag.
        tag: ChunkTag,
        /// Verbatim fill chunk payload.
        payload: RawPayload,
    },
}

impl FillStyle {
    /// Creates a solid uniform fill.
    #[must_use]
    pub const fn uniform(color: Color) -> Self {
        Self::Uniform { color }
    }
}

/// Stroke endpoint cap style in CorelDRAW's Outline Pen dialog (`F12`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineCap {
    /// Flush square cap at the exact endpoint.
    #[default]
    Butt,
    /// Semicircular cap extending by half the stroke width.
    Round,
    /// Rectangular cap extending by half the stroke width.
    Square,
}

/// Stroke corner join style in CorelDRAW's Outline Pen dialog (`F12`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineJoin {
    /// Pointed miter join up to `miter_limit`.
    #[default]
    Miter,
    /// Circular arc join.
    Round,
    /// Flat beveled join.
    Bevel,
}

/// Endpoint arrowhead style in CorelDRAW's Outline Pen dialog (`F12`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArrowheadKind {
    /// No arrowhead marker.
    #[default]
    None,
    /// Solid closed triangular arrowhead.
    Triangle,
    /// Open V-shaped chevron arrowhead.
    OpenArrow,
    /// Solid circular marker at the endpoint.
    Circle,
    /// Solid diamond marker at the endpoint.
    Diamond,
    /// Forward-compatible fallback for arrowhead kinds introduced in newer `.coralcraft` schemas.
    #[serde(other)]
    Unknown,
}

/// CorelDRAW Outline Pen style (`F12`).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OutlineStyle {
    /// Outline ink color.
    pub color: Color,
    /// Outline width in typographic points (`0.0` = CorelDRAW **Hairline**, rendered as 1 device pixel).
    pub width_pt: F64,
    /// Endpoint cap style.
    pub cap: LineCap,
    /// Corner join style.
    pub join: LineJoin,
    /// Miter limit ratio (CorelDRAW default `10.0`).
    pub miter_limit: F64,
    /// CorelDRAW "Behind fill" option: when `true`, the outline is drawn underneath the object's fill.
    pub behind_fill: bool,
    /// CorelDRAW "Scale with object" option: when `true`, the stroke width scales with the object's transform.
    pub scale_with_object: bool,
    /// Optional alternating dash-and-gap lengths in typographic points (`F12` Outline Pen Style). Empty means solid line.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dash_pattern: Vec<F64>,
    /// Start endpoint arrowhead style (`F12` Arrowheads).
    #[serde(default)]
    pub start_arrow: ArrowheadKind,
    /// End endpoint arrowhead style (`F12` Arrowheads).
    #[serde(default)]
    pub end_arrow: ArrowheadKind,
}

impl OutlineStyle {
    /// Creates a standard black Hairline outline (`width_pt == 0.0`).
    #[must_use]
    pub fn hairline(color: Color) -> Self {
        Self {
            color,
            width_pt: F64::ZERO,
            cap: LineCap::Butt,
            join: LineJoin::Miter,
            miter_limit: F64::new(10.0).unwrap_or(F64::ONE),
            behind_fill: false,
            scale_with_object: false,
            dash_pattern: Vec::new(),
            start_arrow: ArrowheadKind::None,
            end_arrow: ArrowheadKind::None,
        }
    }

    /// Creates an outline with explicit `width_pt` in points.
    #[must_use]
    pub fn with_width(color: Color, width_pt: F64) -> Self {
        Self {
            color,
            width_pt,
            cap: LineCap::Butt,
            join: LineJoin::Miter,
            miter_limit: F64::new(10.0).unwrap_or(F64::ONE),
            behind_fill: false,
            scale_with_object: false,
            dash_pattern: Vec::new(),
            start_arrow: ArrowheadKind::None,
            end_arrow: ArrowheadKind::None,
        }
    }

    /// Returns `true` if this outline is a CorelDRAW Hairline (`width_pt == 0.0`).
    #[must_use]
    pub fn is_hairline(&self) -> bool {
        self.width_pt == F64::ZERO
    }
}
