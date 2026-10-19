//! CorelDRAW Fountain Fill stop interpolation (`Direct`, `RainbowCw`, `RainbowCcw` + midpoint skew).

use serde::{Deserialize, Serialize};

use crate::spaces::{CVal, Color, ColorError, cmyk_to_srgb8, hsb_to_srgb8, quantize_u8, srgb8_to_hsb};

/// Color transition path between adjacent Fountain Fill stops in CorelDRAW.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FountainBlendMode {
    /// Direct linear interpolation in the stop's color space (default).
    #[default]
    Direct,
    /// Clockwise hue traversal in HSB space (decreasing hue angle).
    RainbowCw,
    /// Counter-clockwise hue traversal in HSB space (increasing hue angle).
    RainbowCcw,
}

/// A single gradient stop in a CorelDRAW Fountain Fill.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FountainStop {
    /// Normalized stop position along the gradient axis (`0.0..=1.0`).
    pub offset: CVal,
    /// Midpoint skew between this stop and the next stop (`0.01..=0.99`, default `0.5`).
    pub midpoint: CVal,
    /// Stop color.
    pub color: Color,
}

impl FountainStop {
    /// Creates a validated [`FountainStop`] with default `midpoint = 0.5`.
    pub fn new(offset: f64, color: Color) -> Result<Self, ColorError> {
        Self::with_midpoint(offset, 0.5, color)
    }

    /// Creates a validated [`FountainStop`] with an explicit `midpoint in 0.01..=0.99`.
    pub fn with_midpoint(offset: f64, midpoint: f64, color: Color) -> Result<Self, ColorError> {
        Ok(Self {
            offset: CVal::in_range(offset, 0.0, 1.0, "fountain stop offset must be in 0.0..=1.0")?,
            midpoint: CVal::in_range(midpoint, 0.01, 0.99, "fountain stop midpoint must be in 0.01..=0.99")?,
            color,
        })
    }
}

/// Evaluates a list of sorted [`FountainStop`]s at position `t in 0.0..=1.0` using `mode`.
pub fn interpolate_stops(stops: &[FountainStop], mode: FountainBlendMode, t: f64) -> Result<[u8; 3], ColorError> {
    if stops.len() < 2 {
        return Err(ColorError::InvalidFountainStops("at least 2 stops are required"));
    }
    if !t.is_finite() {
        return Err(ColorError::NonFiniteComponent);
    }
    let p = t.clamp(0.0, 1.0);

    // Clamp to first/last stop when outside range
    if p <= stops[0].offset.get() {
        return Ok(stops[0].color.to_srgb8());
    }
    if let Some(last) = stops.last()
        && p >= last.offset.get()
    {
        return Ok(last.color.to_srgb8());
    }

    // Locate the active adjacent pair `(s0, s1)`
    let mut pair_idx = 0;
    for i in 0..(stops.len() - 1) {
        if p >= stops[i].offset.get() && p <= stops[i + 1].offset.get() {
            pair_idx = i;
            break;
        }
    }

    let s0 = &stops[pair_idx];
    let s1 = &stops[pair_idx + 1];
    let span = s1.offset.get() - s0.offset.get();
    if span <= 1e-12 {
        return Ok(s1.color.to_srgb8());
    }

    let t_raw = ((p - s0.offset.get()) / span).clamp(0.0, 1.0);
    let m = s0.midpoint.get().clamp(0.01, 0.99);

    // Piecewise-linear midpoint remap (M2 canonical form)
    let local_t = if t_raw <= m { 0.5 * (t_raw / m) } else { 0.5 + 0.5 * ((t_raw - m) / (1.0 - m)) };

    Ok(blend_pair(&s0.color, &s1.color, mode, local_t))
}

fn blend_pair(c0: &Color, c1: &Color, mode: FountainBlendMode, t: f64) -> [u8; 3] {
    match mode {
        FountainBlendMode::Direct => {
            if let (&Color::Cmyk { c: c0c, m: c0m, y: c0y, k: c0k }, &Color::Cmyk { c: c1c, m: c1m, y: c1y, k: c1k }) = (c0, c1) {
                let c = c0c.get() + t * (c1c.get() - c0c.get());
                let m = c0m.get() + t * (c1m.get() - c0m.get());
                let y = c0y.get() + t * (c1y.get() - c0y.get());
                let k = c0k.get() + t * (c1k.get() - c0k.get());
                return cmyk_to_srgb8(c, m, y, k);
            }
            let [r0, g0, b0] = c0.to_srgb8();
            let [r1, g1, b1] = c1.to_srgb8();
            [
                quantize_u8((f64::from(r0) + t * (f64::from(r1) - f64::from(r0))) / 255.0),
                quantize_u8((f64::from(g0) + t * (f64::from(g1) - f64::from(g0))) / 255.0),
                quantize_u8((f64::from(b0) + t * (f64::from(b1) - f64::from(b0))) / 255.0),
            ]
        }
        FountainBlendMode::RainbowCw | FountainBlendMode::RainbowCcw => {
            let [h0, s0, b0] = to_hsb_components(c0);
            let [h1, s1, b1] = to_hsb_components(c1);
            let s = s0 + t * (s1 - s0);
            let b = b0 + t * (b1 - b0);
            let h = if mode == FountainBlendMode::RainbowCw {
                (h0 - t * (h0 - h1).rem_euclid(360.0)).rem_euclid(360.0)
            } else {
                (h0 + t * (h1 - h0).rem_euclid(360.0)).rem_euclid(360.0)
            };
            hsb_to_srgb8(h, s, b)
        }
    }
}

fn to_hsb_components(c: &Color) -> [f64; 3] {
    if let &Color::Hsb { h, s, b } = c {
        [h.get(), s.get(), b.get()]
    } else {
        let [r, g, b] = c.to_srgb8();
        srgb8_to_hsb(r, g, b)
    }
}
