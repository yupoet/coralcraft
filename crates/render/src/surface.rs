//! Premultiplied RGBA8 [`Surface`], [`RenderError`], and [`RenderWarning`] types.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use coralcraft_color::ColorError;
use coralcraft_doc::DocError;
use coralcraft_geom::GeomError;

/// Maximum pixel dimension (`width` or `height`) allowed for a [`Surface`].
pub const MAX_SURFACE_SIDE: u32 = 32_767;

/// Maximum total pixel count (`width * height <= 67,108,864` = 256 MiB RGBA8) to guard against unchecked allocation.
pub const MAX_SURFACE_PIXELS: u64 = 1 << 26;

/// Maximum recursive PowerClip nesting depth before truncation with a warning.
pub const MAX_CLIP_DEPTH: u8 = 8;

/// Errors returned by `coralcraft-render`.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RenderError {
    /// Requested surface dimensions are zero or exceed [`MAX_SURFACE_SIDE`] / [`MAX_SURFACE_PIXELS`].
    #[error("invalid or oversized surface dimensions ({width}x{height})")]
    SurfaceTooLarge {
        /// Requested width in pixels.
        width: u32,
        /// Requested height in pixels.
        height: u32,
    },
    /// Render scale factor is non-finite or `<= 0.0`.
    #[error("invalid render scale factor")]
    InvalidScale,
    /// Document error during page or layer resolution.
    #[error("document error: {0}")]
    Doc(#[from] DocError),
    /// Geometry error during path construction or flattening.
    #[error("geometry error: {0}")]
    Geom(#[from] GeomError),
    /// Color error during gradient interpolation.
    #[error("color error: {0}")]
    Color(#[from] ColorError),
    /// PNG payload exceeds 32-bit chunk length ceiling.
    #[error("PNG image payload exceeds u32 chunk limit")]
    PngTooLarge,
}

/// Non-fatal warnings recorded during page rendering.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RenderWarning {
    /// PowerClip nesting exceeded [`MAX_CLIP_DEPTH`] (`8`); deeper clip frames were flattened into the parent clip.
    ClipDepthTruncated {
        /// Object ID where clip depth was capped.
        object_id: u64,
        /// Maximum supported clip depth.
        max_depth: u8,
    },
    /// An `ObjectKind::Unsupported` node was rendered using its `fallback_bounds` placeholder.
    UnsupportedObjectFallback {
        /// Object ID.
        object_id: u64,
        /// 4CC chunk tag of the unsupported object.
        tag: String,
    },
    /// An unimplemented `FillStyle::Unknown` chunk fell back to neutral gray.
    UnknownFillFallback {
        /// Object ID.
        object_id: u64,
        /// 4CC chunk tag of the unknown fill.
        tag: String,
    },
}

/// 8-bit premultiplied RGBA pixel buffer (`[r_premul, g_premul, b_premul, a]`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Surface {
    /// Width in device pixels.
    pub width: u32,
    /// Height in device pixels.
    pub height: u32,
    /// Row-major premultiplied RGBA8 pixels (`len == width * height`).
    pub pixels: Vec<[u8; 4]>,
}

impl Surface {
    /// Allocates a new [`Surface`] initialized to straight-RGBA background `bg_straight_rgba`.
    pub fn new(width: u32, height: u32, bg_straight_rgba: [u8; 4]) -> Result<Self, RenderError> {
        let total = u64::from(width) * u64::from(height);
        if width == 0 || height == 0 || width > MAX_SURFACE_SIDE || height > MAX_SURFACE_SIDE || total > MAX_SURFACE_PIXELS {
            return Err(RenderError::SurfaceTooLarge { width, height });
        }
        let premul = straight_to_premul(bg_straight_rgba);
        Ok(Self { width, height, pixels: vec![premul; total as usize] })
    }

    /// Returns the premultiplied RGBA8 pixel at `(x, y)`, or `[0, 0, 0, 0]` if out of bounds.
    #[must_use]
    pub fn get_premul(&self, x: u32, y: u32) -> [u8; 4] {
        if x < self.width && y < self.height {
            let idx = (y as usize) * (self.width as usize) + (x as usize);
            self.pixels[idx]
        } else {
            [0, 0, 0, 0]
        }
    }

    /// Returns the unpremultiplied (straight-alpha) RGBA8 pixel at `(x, y)`.
    #[must_use]
    pub fn get_straight_rgba(&self, x: u32, y: u32) -> [u8; 4] {
        premul_to_straight(self.get_premul(x, y))
    }

    /// Blends a straight-alpha color `[r, g, b]` with coverage/opacity `alpha in 0.0..=1.0`
    /// onto pixel `(x, y)` using Porter-Duff `src-over`.
    #[inline]
    pub fn blend_straight(&mut self, x: u32, y: u32, rgb: [u8; 3], alpha: f64) {
        if x >= self.width || y >= self.height || alpha <= 0.0 {
            return;
        }
        let a_u8 = (alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
        if a_u8 == 0 {
            return;
        }
        let src = straight_to_premul([rgb[0], rgb[1], rgb[2], a_u8]);
        let idx = (y as usize) * (self.width as usize) + (x as usize);
        self.pixels[idx] = src_over_premul(src, self.pixels[idx]);
    }

    /// Multiplies `child`'s premultiplied pixels by the 16-level coverage mask `mask_16` (`0..=16`)
    /// and composites the result onto `self` via Porter-Duff `src-over` (used for CorelDRAW PowerClip).
    pub fn composite_masked(&mut self, child: &Self, mask_16: &[u8]) {
        for ((dst, &sp), &m) in self.pixels.iter_mut().zip(&child.pixels).zip(mask_16) {
            let cov = u32::from(m.min(16));
            if cov == 0 || sp[3] == 0 {
                continue;
            }
            let masked_src = if cov == 16 {
                sp
            } else {
                [
                    ((u32::from(sp[0]) * cov + 8) / 16) as u8,
                    ((u32::from(sp[1]) * cov + 8) / 16) as u8,
                    ((u32::from(sp[2]) * cov + 8) / 16) as u8,
                    ((u32::from(sp[3]) * cov + 8) / 16) as u8,
                ]
            };
            *dst = src_over_premul(masked_src, *dst);
        }
    }
}

/// Converts straight RGBA8 `[r, g, b, a]` to premultiplied RGBA8.
#[inline]
#[must_use]
pub const fn straight_to_premul(rgba: [u8; 4]) -> [u8; 4] {
    let a = rgba[3] as u32;
    if a == 255 {
        return rgba;
    }
    if a == 0 {
        return [0, 0, 0, 0];
    }
    [((rgba[0] as u32 * a + 127) / 255) as u8, ((rgba[1] as u32 * a + 127) / 255) as u8, ((rgba[2] as u32 * a + 127) / 255) as u8, rgba[3]]
}

/// Converts premultiplied RGBA8 to straight RGBA8.
#[inline]
#[must_use]
pub const fn premul_to_straight(premul: [u8; 4]) -> [u8; 4] {
    let a = premul[3] as u32;
    if a == 0 {
        return [0, 0, 0, 0];
    }
    if a == 255 {
        return premul;
    }
    const fn unpremul(c: u8, a: u32) -> u8 {
        let v = (c as u32 * 255 + (a / 2)) / a;
        if v > 255 { 255 } else { v as u8 }
    }
    [unpremul(premul[0], a), unpremul(premul[1], a), unpremul(premul[2], a), premul[3]]
}

/// Porter-Duff `src-over` on premultiplied RGBA8 pixels: `out = src + dst * (1 - src.a)`.
#[inline]
#[must_use]
pub const fn src_over_premul(src: [u8; 4], dst: [u8; 4]) -> [u8; 4] {
    let sa = src[3] as u32;
    if sa == 255 {
        return src;
    }
    if sa == 0 {
        return dst;
    }
    let inv_sa = 255 - sa;
    const fn blend(s: u8, d: u8, inv_sa: u32) -> u8 {
        let v = (s as u32) + ((d as u32 * inv_sa + 127) / 255);
        if v > 255 { 255 } else { v as u8 }
    }
    [blend(src[0], dst[0], inv_sa), blend(src[1], dst[1], inv_sa), blend(src[2], dst[2], inv_sa), blend(src[3], dst[3], inv_sa)]
}
