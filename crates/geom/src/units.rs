//! Exact CorelDRAW unit conversions (`CdrUnit` = `0.1 µm` = `1/254,000 inch` <-> `Pt` = `1/72 inch`).

use serde::{Deserialize, Serialize};

use crate::f64_canon::{F64, GeomError};

/// Exact CDR ticks per inch (`25.4 mm / 0.1 µm = 254,000`).
pub const CDR_TICKS_PER_INCH: i64 = 254_000;

/// Exact CDR ticks per millimeter (`1 mm / 0.1 µm = 10,000`).
pub const CDR_TICKS_PER_MM: i64 = 10_000;

/// Typographic points per inch (`72.0`).
pub const PTS_PER_INCH: f64 = 72.0;

/// Reduced rational numerator for `1 pt = (31_750 / 9) CDR ticks` (`gcd(254_000, 72) = 8`).
pub const CDR_TICKS_PER_PT_NUM: i64 = 31_750;

/// Reduced rational denominator for `1 pt = (31_750 / 9) CDR ticks`.
pub const CDR_TICKS_PER_PT_DEN: i64 = 9;

/// Native CorelDRAW integer coordinate tick (`1 tick = 0.1 µm = 1/254,000 inch` exactly).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CdrUnit(pub i64);

/// Typographic point (`1 pt = 1/72 inch`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Pt(pub F64);

impl CdrUnit {
    /// Converts CDR integer ticks to typographic points (`ticks * 9 / 31_750`).
    #[must_use]
    pub fn to_pt(self) -> Pt {
        let raw = (self.0 as f64) * (CDR_TICKS_PER_PT_DEN as f64) / (CDR_TICKS_PER_PT_NUM as f64);
        // `self.0` is a finite i64, so `raw` is always finite.
        Pt(F64::new(raw).unwrap_or(F64::ZERO))
    }

    /// Converts CDR integer ticks to millimeters (`ticks / 10,000`).
    #[must_use]
    pub fn to_mm(self) -> f64 {
        (self.0 as f64) / (CDR_TICKS_PER_MM as f64)
    }

    /// Converts millimeters to nearest integer CDR ticks (`mm * 10,000`).
    pub fn from_mm(mm: f64) -> Result<Self, GeomError> {
        if !mm.is_finite() {
            return Err(GeomError::NonFiniteCoordinate);
        }
        let scaled = (mm * (CDR_TICKS_PER_MM as f64)).round();
        if !(scaled >= (i64::MIN as f64) && scaled < (i64::MAX as f64)) {
            return Err(GeomError::CoordinateOutOfRange);
        }
        Ok(Self(scaled as i64))
    }
}

impl Pt {
    /// Constructs a checked `Pt` from a raw `f64`.
    pub fn new(pt: f64) -> Result<Self, GeomError> {
        Ok(Self(F64::new(pt)?))
    }

    /// Returns the underlying `f64` value in points.
    #[must_use]
    pub const fn get(self) -> f64 {
        self.0.get()
    }

    /// Converts typographic points to nearest CDR integer ticks (`pt * 31_750 / 9`).
    pub fn try_to_cdr_ticks(self) -> Result<CdrUnit, GeomError> {
        let v = self.0.get();
        if !v.is_finite() {
            return Err(GeomError::NonFiniteCoordinate);
        }
        let scaled = (v * (CDR_TICKS_PER_PT_NUM as f64) / (CDR_TICKS_PER_PT_DEN as f64)).round();
        if !(scaled >= (i64::MIN as f64) && scaled < (i64::MAX as f64)) {
            return Err(GeomError::CoordinateOutOfRange);
        }
        Ok(CdrUnit(scaled as i64))
    }
}

/// Document display and authoring measurement units supported in CorelDRAW's Property Bar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    /// Millimeters (`1 mm = 72 / 25.4 pt`).
    #[default]
    Mm,
    /// Typographic points (`1/72 inch`).
    Pt,
    /// Inches (`1 inch = 72 pt`).
    Inch,
    /// Pixels at 96 DPI (`1 px = 72 / 96 pt = 0.75 pt`).
    Px,
    /// Native CDR integer ticks (`0.1 µm`).
    CdrTick,
}

impl Unit {
    /// Converts a value in `self` units into typographic points (`Pt`).
    pub fn to_pt(self, value: f64) -> Result<Pt, GeomError> {
        if !value.is_finite() {
            return Err(GeomError::NonFiniteCoordinate);
        }
        let pts = match self {
            Self::Mm => value * (PTS_PER_INCH / 25.4),
            Self::Pt => value,
            Self::Inch => value * PTS_PER_INCH,
            Self::Px => value * (PTS_PER_INCH / 96.0),
            Self::CdrTick => value * (CDR_TICKS_PER_PT_DEN as f64) / (CDR_TICKS_PER_PT_NUM as f64),
        };
        Pt::new(pts)
    }

    /// Converts typographic points (`Pt`) into a value in `self` units.
    #[must_use]
    pub fn from_pt(self, pt: Pt) -> f64 {
        let v = pt.get();
        match self {
            Self::Mm => v * (25.4 / PTS_PER_INCH),
            Self::Pt => v,
            Self::Inch => v / PTS_PER_INCH,
            Self::Px => v * (96.0 / PTS_PER_INCH),
            Self::CdrTick => v * (CDR_TICKS_PER_PT_NUM as f64) / (CDR_TICKS_PER_PT_DEN as f64),
        }
    }
}
