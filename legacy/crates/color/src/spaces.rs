//! Color spaces (`Rgb`, `Cmyk`, `Hsb`, `Gray`, `LabD50`, `LabD65`, `Spot`, `Registration`)
//! and CIE Bradford D50 <-> D65 chromatic adaptation.

use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

/// Errors returned by `coralcraft-color` operations.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ColorError {
    /// Color channel scalar is `NaN` or `±Infinity`.
    #[error("color component is not finite (NaN or Infinity)")]
    NonFiniteComponent,
    /// Color channel is outside its valid domain.
    #[error("color component out of range: {0}")]
    ComponentOutOfRange(&'static str),
    /// Invalid hexadecimal color string.
    #[error("invalid hex color string: {0}")]
    InvalidHexColor(String),
    /// Malformed XML palette syntax or missing required attributes.
    #[error("XML palette parse error: {0}")]
    XmlParseError(String),
    /// Binary `.cpl` palette header was detected, which is detection-only in M2.
    #[error("binary .cpl palette body decoding is not supported in M2 (header detected)")]
    UnsupportedBinaryCpl,
    /// Input bytes are neither a valid XML palette nor a recognized `.cpl` binary header.
    #[error("unrecognized palette file format")]
    UnknownPaletteFormat,
    /// Requested swatch name or index was not found in the palette.
    #[error("swatch not found in palette: {0}")]
    SwatchNotFound(String),
    /// Invalid Fountain Fill stop list.
    #[error("invalid fountain fill stops: {0}")]
    InvalidFountainStops(&'static str),
}

/// Canonical finite 64-bit float for color components (`-0.0 -> +0.0`, `Eq`, `Ord`, `Hash`).
#[derive(Clone, Copy, Debug, Default)]
pub struct CVal(f64);

impl CVal {
    /// Canonical `0.0`.
    pub const ZERO: Self = Self(0.0);
    /// Canonical `0.5`.
    pub const HALF: Self = Self(0.5);
    /// Canonical `1.0`.
    pub const ONE: Self = Self(1.0);

    /// Constructs a canonical finite `CVal`.
    pub fn new(v: f64) -> Result<Self, ColorError> {
        if !v.is_finite() {
            return Err(ColorError::NonFiniteComponent);
        }
        Ok(Self(if v == 0.0 { 0.0 } else { v }))
    }

    /// Constructs a `CVal` verified to lie within `[min, max]`.
    pub fn in_range(v: f64, min: f64, max: f64, err_msg: &'static str) -> Result<Self, ColorError> {
        let c = Self::new(v)?;
        if c.0 < min || c.0 > max {
            return Err(ColorError::ComponentOutOfRange(err_msg));
        }
        Ok(c)
    }

    /// Returns the underlying `f64`.
    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }
}

impl PartialEq for CVal {
    fn eq(&self, other: &Self) -> bool {
        self.0.to_bits() == other.0.to_bits()
    }
}

impl Eq for CVal {}

impl Hash for CVal {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.to_bits().hash(state);
    }
}

impl PartialOrd for CVal {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for CVal {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.total_cmp(&other.0)
    }
}

impl fmt::Display for CVal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for CVal {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let v = if self.0 == 0.0 { 0.0 } else { self.0 };
        serializer.serialize_f64(v)
    }
}

impl<'de> Deserialize<'de> for CVal {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = f64::deserialize(deserializer)?;
        Self::new(raw).map_err(serde::de::Error::custom)
    }
}

/// Policy for previewing Spot color tints on screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpotBlend {
    /// Standard CorelDRAW screen preview: scales CMYK ink channels by `tint` against paper white.
    #[default]
    White,
    /// Linearly blends in sRGB between paper white and the 100% fallback RGB color.
    Fallback,
}

/// Authoring color value in CoralCraft, preserving the exact color model chosen by the user or CDR file.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "space", rename_all = "snake_case")]
pub enum Color {
    /// 8-bit gamma-encoded sRGB (`0..=255`).
    Rgb {
        /// Red channel (`0..=255`).
        r: u8,
        /// Green channel (`0..=255`).
        g: u8,
        /// Blue channel (`0..=255`).
        b: u8,
    },
    /// Process CMYK ink fractions (`0.0..=1.0`, corresponding to `0..=100%`).
    Cmyk {
        /// Cyan (`0.0..=1.0`).
        c: CVal,
        /// Magenta (`0.0..=1.0`).
        m: CVal,
        /// Yellow (`0.0..=1.0`).
        y: CVal,
        /// Black / Key (`0.0..=1.0`).
        k: CVal,
    },
    /// Hue-Saturation-Brightness (`h in [0, 360)`, `s, b in [0, 1]`).
    Hsb {
        /// Hue in degrees (`0.0..360.0`).
        h: CVal,
        /// Saturation (`0.0..=1.0`).
        s: CVal,
        /// Brightness / Value (`0.0..=1.0`).
        b: CVal,
    },
    /// Grayscale ink fraction (`0.0` = white, `1.0` = black).
    Gray {
        /// Black ink fraction (`0.0..=1.0`).
        k: CVal,
    },
    /// CIE L*a*b* under ICC D50 illuminant (`L in [0, 100]`, `a, b in [-128, 127]`).
    LabD50 {
        /// Lightness `L*` (`0.0..=100.0`).
        l: CVal,
        /// Green-red axis `a*`.
        a: CVal,
        /// Blue-yellow axis `b*`.
        b: CVal,
    },
    /// CIE L*a*b* under sRGB D65 illuminant (`L in [0, 100]`, `a, b in [-128, 127]`).
    LabD65 {
        /// Lightness `L*` (`0.0..=100.0`).
        l: CVal,
        /// Green-red axis `a*`.
        a: CVal,
        /// Blue-yellow axis `b*`.
        b: CVal,
    },
    /// Named Spot ink with tint (`0.0..=1.0`) and CMYK fallback (`0.0..=1.0` per plate).
    Spot {
        /// Spot ink name (e.g. user-imported custom or Pantone swatch name).
        name: String,
        /// Tint fraction (`0.0..=1.0`).
        tint: CVal,
        /// Fallback CMYK `[c, m, y, k]` at 100% tint.
        fallback_cmyk: [CVal; 4],
    },
    /// Printer registration mark ink (100% on all plates; renders as solid black on screen).
    Registration,
}

impl Default for Color {
    fn default() -> Self {
        Self::BLACK
    }
}

impl Color {
    /// Solid sRGB black `(0, 0, 0)`.
    pub const BLACK: Self = Self::Rgb { r: 0, g: 0, b: 0 };
    /// Solid sRGB white `(255, 255, 255)`.
    pub const WHITE: Self = Self::Rgb { r: 255, g: 255, b: 255 };

    /// Creates an 8-bit sRGB color.
    #[must_use]
    pub const fn rgb8(r: u8, g: u8, b: u8) -> Self {
        Self::Rgb { r, g, b }
    }

    /// Creates a validated CMYK color (`c, m, y, k in 0.0..=1.0`).
    pub fn cmyk(c: f64, m: f64, y: f64, k: f64) -> Result<Self, ColorError> {
        Ok(Self::Cmyk {
            c: CVal::in_range(c, 0.0, 1.0, "CMYK c must be in 0.0..=1.0")?,
            m: CVal::in_range(m, 0.0, 1.0, "CMYK m must be in 0.0..=1.0")?,
            y: CVal::in_range(y, 0.0, 1.0, "CMYK y must be in 0.0..=1.0")?,
            k: CVal::in_range(k, 0.0, 1.0, "CMYK k must be in 0.0..=1.0")?,
        })
    }

    /// Creates a validated HSB color (`h in 0.0..=360.0` normalized to `[0.0, 360.0)`, `s, b in 0.0..=1.0`).
    pub fn hsb(h: f64, s: f64, b: f64) -> Result<Self, ColorError> {
        let h_val = CVal::in_range(h, 0.0, 360.0, "HSB h must be in 0.0..=360.0")?;
        let norm_h = if (h_val.get() - 360.0).abs() < 1e-12 { CVal::ZERO } else { CVal::new(h_val.get().rem_euclid(360.0))? };
        Ok(Self::Hsb {
            h: norm_h,
            s: CVal::in_range(s, 0.0, 1.0, "HSB s must be in 0.0..=1.0")?,
            b: CVal::in_range(b, 0.0, 1.0, "HSB b must be in 0.0..=1.0")?,
        })
    }

    /// Creates a validated Grayscale color (`k in 0.0..=1.0`).
    pub fn gray(k: f64) -> Result<Self, ColorError> {
        Ok(Self::Gray { k: CVal::in_range(k, 0.0, 1.0, "Gray k must be in 0.0..=1.0")? })
    }

    /// Creates a validated CIE Lab (D50) color (`l in 0.0..=100.0`, `a, b in -128.0..=127.0`).
    pub fn lab_d50(l: f64, a: f64, b: f64) -> Result<Self, ColorError> {
        Ok(Self::LabD50 {
            l: CVal::in_range(l, 0.0, 100.0, "Lab L* must be in 0.0..=100.0")?,
            a: CVal::in_range(a, -128.0, 127.0, "Lab a* must be in -128.0..=127.0")?,
            b: CVal::in_range(b, -128.0, 127.0, "Lab b* must be in -128.0..=127.0")?,
        })
    }

    /// Creates a validated CIE Lab (D65) color (`l in 0.0..=100.0`, `a, b in -128.0..=127.0`).
    pub fn lab_d65(l: f64, a: f64, b: f64) -> Result<Self, ColorError> {
        Ok(Self::LabD65 {
            l: CVal::in_range(l, 0.0, 100.0, "Lab L* must be in 0.0..=100.0")?,
            a: CVal::in_range(a, -128.0, 127.0, "Lab a* must be in -128.0..=127.0")?,
            b: CVal::in_range(b, -128.0, 127.0, "Lab b* must be in -128.0..=127.0")?,
        })
    }

    /// Creates a validated Spot color (`tint in 0.0..=1.0`, `fallback_cmyk in 0.0..=1.0`).
    /// If `name` matches `"REGISTRATION"` (case-insensitive), returns [`Color::Registration`].
    pub fn spot(name: impl Into<String>, tint: f64, fallback_cmyk: [f64; 4]) -> Result<Self, ColorError> {
        let name = name.into();
        if Self::is_registration_name(&name) {
            return Ok(Self::Registration);
        }
        Ok(Self::Spot {
            name,
            tint: CVal::in_range(tint, 0.0, 1.0, "Spot tint must be in 0.0..=1.0")?,
            fallback_cmyk: [
                CVal::in_range(fallback_cmyk[0], 0.0, 1.0, "Spot fallback C must be in 0.0..=1.0")?,
                CVal::in_range(fallback_cmyk[1], 0.0, 1.0, "Spot fallback M must be in 0.0..=1.0")?,
                CVal::in_range(fallback_cmyk[2], 0.0, 1.0, "Spot fallback Y must be in 0.0..=1.0")?,
                CVal::in_range(fallback_cmyk[3], 0.0, 1.0, "Spot fallback K must be in 0.0..=1.0")?,
            ],
        })
    }

    /// Returns `true` if `name` is the reserved registration ink name (`"REGISTRATION"` or `"[Registration]"`).
    #[must_use]
    pub fn is_registration_name(name: &str) -> bool {
        let trimmed = name.trim().trim_start_matches('[').trim_end_matches(']');
        trimmed.eq_ignore_ascii_case("registration")
    }

    /// Parses a `#RRGGBB` or `RRGGBB` hex string into [`Color::Rgb`].
    pub fn from_hex(hex: &str) -> Result<Self, ColorError> {
        let s = hex.trim().strip_prefix('#').unwrap_or_else(|| hex.trim());
        let (Some(r_str), Some(g_str), Some(b_str)) = (s.get(0..2), s.get(2..4), s.get(4..6)) else {
            return Err(ColorError::InvalidHexColor(hex.to_owned()));
        };
        if s.len() != 6 || !s.is_ascii() {
            return Err(ColorError::InvalidHexColor(hex.to_owned()));
        }
        let r = u8::from_str_radix(r_str, 16).map_err(|_| ColorError::InvalidHexColor(hex.to_owned()))?;
        let g = u8::from_str_radix(g_str, 16).map_err(|_| ColorError::InvalidHexColor(hex.to_owned()))?;
        let b = u8::from_str_radix(b_str, 16).map_err(|_| ColorError::InvalidHexColor(hex.to_owned()))?;
        Ok(Self::Rgb { r, g, b })
    }

    /// Formats the resolved 8-bit sRGB color as lowercase `#rrggbb`.
    #[must_use]
    pub fn to_hex(&self) -> String {
        let [r, g, b] = self.to_srgb8();
        format!("#{r:02x}{g:02x}{b:02x}")
    }

    /// Resolves any [`Color`] variant to 8-bit gamma-encoded sRGB `[r, g, b]`.
    #[must_use]
    pub fn to_srgb8(&self) -> [u8; 3] {
        self.to_srgb8_with_spot_policy(SpotBlend::White)
    }

    /// Resolves any [`Color`] variant to 8-bit sRGB using the specified [`SpotBlend`] policy.
    #[must_use]
    pub fn to_srgb8_with_spot_policy(&self, policy: SpotBlend) -> [u8; 3] {
        match self {
            &Self::Rgb { r, g, b } => [r, g, b],
            &Self::Cmyk { c, m, y, k } => cmyk_to_srgb8(c.get(), m.get(), y.get(), k.get()),
            &Self::Hsb { h, s, b } => hsb_to_srgb8(h.get(), s.get(), b.get()),
            &Self::Gray { k } => {
                let v = quantize_u8(1.0 - k.get());
                [v, v, v]
            }
            &Self::LabD50 { l, a, b } => lab_d50_to_srgb8(l.get(), a.get(), b.get()),
            &Self::LabD65 { l, a, b } => lab_d65_to_srgb8(l.get(), a.get(), b.get()),
            Self::Spot { name, tint, fallback_cmyk } => {
                if Self::is_registration_name(name) {
                    return [0, 0, 0];
                }
                let t = tint.get().clamp(0.0, 1.0);
                let [c, m, y, k] = [fallback_cmyk[0].get(), fallback_cmyk[1].get(), fallback_cmyk[2].get(), fallback_cmyk[3].get()];
                match policy {
                    SpotBlend::White => cmyk_to_srgb8(c * t, m * t, y * t, k * t),
                    SpotBlend::Fallback => {
                        let [fr, fg, fb] = cmyk_to_srgb8(c, m, y, k);
                        [
                            ((1.0 - t) * 255.0 + t * f64::from(fr)).round().clamp(0.0, 255.0) as u8,
                            ((1.0 - t) * 255.0 + t * f64::from(fg)).round().clamp(0.0, 255.0) as u8,
                            ((1.0 - t) * 255.0 + t * f64::from(fb)).round().clamp(0.0, 255.0) as u8,
                        ]
                    }
                }
            }
            Self::Registration => [0, 0, 0],
        }
    }

    /// Resolves to 8-bit straight RGBA `[r, g, b, a]` with `opacity in 0.0..=1.0`.
    #[must_use]
    pub fn to_rgba8(&self, opacity: f64) -> [u8; 4] {
        let [r, g, b] = self.to_srgb8();
        [r, g, b, quantize_u8(opacity)]
    }
}

#[inline]
pub(crate) fn quantize_u8(unit: f64) -> u8 {
    (unit.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Converts device CMYK (`0.0..=1.0`) to 8-bit gamma-encoded sRGB.
#[must_use]
pub fn cmyk_to_srgb8(c: f64, m: f64, y: f64, k: f64) -> [u8; 3] {
    let c = c.clamp(0.0, 1.0);
    let m = m.clamp(0.0, 1.0);
    let y = y.clamp(0.0, 1.0);
    let k = k.clamp(0.0, 1.0);
    let r = (1.0 - c) * (1.0 - k);
    let g = (1.0 - m) * (1.0 - k);
    let b = (1.0 - y) * (1.0 - k);
    [quantize_u8(r), quantize_u8(g), quantize_u8(b)]
}

/// Converts 8-bit sRGB to naive device CMYK (`0.0..=1.0`).
#[must_use]
pub fn srgb8_to_cmyk(r: u8, g: u8, b: u8) -> [f64; 4] {
    let rf = f64::from(r) / 255.0;
    let gf = f64::from(g) / 255.0;
    let bf = f64::from(b) / 255.0;
    let max_rgb = rf.max(gf).max(bf);
    let k = 1.0 - max_rgb;
    if max_rgb <= 1e-12 {
        return [0.0, 0.0, 0.0, 1.0];
    }
    let c = (max_rgb - rf) / max_rgb;
    let m = (max_rgb - gf) / max_rgb;
    let y = (max_rgb - bf) / max_rgb;
    [c, m, y, k]
}

/// Converts HSB (`h in 0..360`, `s, b in 0..=1`) to 8-bit sRGB.
#[must_use]
pub fn hsb_to_srgb8(h: f64, s: f64, b: f64) -> [u8; 3] {
    let s = s.clamp(0.0, 1.0);
    let b = b.clamp(0.0, 1.0);
    let h_norm = if (h - 360.0).abs() < 1e-12 { 0.0 } else { h.rem_euclid(360.0) };
    let c = b * s;
    let h_prime = h_norm / 60.0;
    let x = c * (1.0 - ((h_prime % 2.0) - 1.0).abs());
    let m = b - c;
    let (r1, g1, b1) = if h_prime < 1.0 {
        (c, x, 0.0)
    } else if h_prime < 2.0 {
        (x, c, 0.0)
    } else if h_prime < 3.0 {
        (0.0, c, x)
    } else if h_prime < 4.0 {
        (0.0, x, c)
    } else if h_prime < 5.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    [quantize_u8(r1 + m), quantize_u8(g1 + m), quantize_u8(b1 + m)]
}

/// Converts 8-bit sRGB to HSB `[h_deg, s, b]` (`h in [0, 360)`, `s, b in [0, 1]`).
#[must_use]
pub fn srgb8_to_hsb(r: u8, g: u8, b: u8) -> [f64; 3] {
    let rf = f64::from(r) / 255.0;
    let gf = f64::from(g) / 255.0;
    let bf = f64::from(b) / 255.0;
    let max_c = rf.max(gf).max(bf);
    let min_c = rf.min(gf).min(bf);
    let d = max_c - min_c;
    let v = max_c;
    let s = if max_c <= 1e-12 { 0.0 } else { d / max_c };
    let h = if d <= 1e-12 {
        0.0
    } else if (max_c - rf).abs() <= 1e-12 {
        60.0 * (((gf - bf) / d).rem_euclid(6.0))
    } else if (max_c - gf).abs() <= 1e-12 {
        60.0 * (((bf - rf) / d) + 2.0)
    } else {
        60.0 * (((rf - gf) / d) + 4.0)
    };
    [h.rem_euclid(360.0), s, v]
}

// Reference whites (CIE 1931 2-degree observer)
const D50_WHITE: [f64; 3] = [0.96422, 1.00000, 0.82521];
const D65_WHITE: [f64; 3] = [0.95047, 1.00000, 1.08883];

// Bradford cone response matrix M_A and its exact 3x3 inverse
const M_BRADFORD: [[f64; 3]; 3] = [[0.8951, 0.2664, -0.1614], [-0.7502, 1.7135, 0.0367], [0.0389, -0.0685, 1.0296]];

const M_BRADFORD_INV: [[f64; 3]; 3] = [
    [0.9869929054667123, -0.14705425642099013, 0.15996265166373125],
    [0.43230526972339456, 0.5183602715367776, 0.0492912282128556],
    [-0.008528664575177328, 0.04004282165408487, 0.9684866957875502],
];

fn mul3(m: [[f64; 3]; 3], v: [f64; 3]) -> [f64; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

/// Adapts CIE XYZ from `src_white` to `dst_white` using the Bradford chromatic adaptation transform.
#[must_use]
pub fn bradford_adapt_xyz(xyz: [f64; 3], src_white: [f64; 3], dst_white: [f64; 3]) -> [f64; 3] {
    let src_lms = mul3(M_BRADFORD, src_white);
    let dst_lms = mul3(M_BRADFORD, dst_white);
    let lms = mul3(M_BRADFORD, xyz);
    let adapted_lms = [lms[0] * (dst_lms[0] / src_lms[0]), lms[1] * (dst_lms[1] / src_lms[1]), lms[2] * (dst_lms[2] / src_lms[2])];
    mul3(M_BRADFORD_INV, adapted_lms)
}

fn lab_to_xyz(l: f64, a: f64, b: f64, white: [f64; 3]) -> [f64; 3] {
    const DELTA: f64 = 6.0 / 29.0;
    let fy = (l + 16.0) / 116.0;
    let fx = fy + a / 500.0;
    let fz = fy - b / 200.0;
    let f_inv = |t: f64| -> f64 { if t > DELTA { t * t * t } else { 3.0 * DELTA * DELTA * (t - 4.0 / 29.0) } };
    [white[0] * f_inv(fx), white[1] * f_inv(fy), white[2] * f_inv(fz)]
}

fn xyz_to_lab(xyz: [f64; 3], white: [f64; 3]) -> [f64; 3] {
    const DELTA: f64 = 6.0 / 29.0;
    const EPSILON: f64 = DELTA * DELTA * DELTA;
    let f = |t: f64| -> f64 { if t > EPSILON { t.cbrt() } else { t / (3.0 * DELTA * DELTA) + 4.0 / 29.0 } };
    let fx = f(xyz[0] / white[0]);
    let fy = f(xyz[1] / white[1]);
    let fz = f(xyz[2] / white[2]);
    let l = (116.0 * fy - 16.0).clamp(0.0, 100.0);
    let a = 500.0 * (fx - fy);
    let b = 200.0 * (fy - fz);
    [l, a, b]
}

/// Converts CIE Lab (D50) to CIE Lab (D65) via Bradford chromatic adaptation.
#[must_use]
pub fn lab_d50_to_d65(l: f64, a: f64, b: f64) -> [f64; 3] {
    let xyz_d50 = lab_to_xyz(l, a, b, D50_WHITE);
    let xyz_d65 = bradford_adapt_xyz(xyz_d50, D50_WHITE, D65_WHITE);
    xyz_to_lab(xyz_d65, D65_WHITE)
}

/// Converts CIE Lab (D65) to CIE Lab (D50) via Bradford chromatic adaptation.
#[must_use]
pub fn lab_d65_to_d50(l: f64, a: f64, b: f64) -> [f64; 3] {
    let xyz_d65 = lab_to_xyz(l, a, b, D65_WHITE);
    let xyz_d50 = bradford_adapt_xyz(xyz_d65, D65_WHITE, D50_WHITE);
    xyz_to_lab(xyz_d50, D50_WHITE)
}

fn xyz_d65_to_srgb8(xyz: [f64; 3]) -> [u8; 3] {
    const M_XYZ_TO_SRGB: [[f64; 3]; 3] = [[3.2406, -1.5372, -0.4986], [-0.9689, 1.8758, 0.0415], [0.0557, -0.2040, 1.0570]];
    let lin = mul3(M_XYZ_TO_SRGB, xyz);
    let gamma = |u: f64| -> f64 {
        let u = u.clamp(0.0, 1.0);
        if u <= 0.003_130_8 { 12.92 * u } else { 1.055 * u.powf(1.0 / 2.4) - 0.055 }
    };
    [quantize_u8(gamma(lin[0])), quantize_u8(gamma(lin[1])), quantize_u8(gamma(lin[2]))]
}

/// Converts CIE Lab (D50) to 8-bit sRGB via Bradford D50 -> D65 chromatic adaptation.
#[must_use]
pub fn lab_d50_to_srgb8(l: f64, a: f64, b: f64) -> [u8; 3] {
    let xyz_d50 = lab_to_xyz(l, a, b, D50_WHITE);
    let xyz_d65 = bradford_adapt_xyz(xyz_d50, D50_WHITE, D65_WHITE);
    xyz_d65_to_srgb8(xyz_d65)
}

/// Converts CIE Lab (D65) directly to 8-bit sRGB.
#[must_use]
pub fn lab_d65_to_srgb8(l: f64, a: f64, b: f64) -> [u8; 3] {
    let xyz_d65 = lab_to_xyz(l, a, b, D65_WHITE);
    xyz_d65_to_srgb8(xyz_d65)
}
