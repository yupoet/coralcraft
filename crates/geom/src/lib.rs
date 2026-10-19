//! `coralcraft-geom`: 2D geometry kernel for CoralCraft.
//!
//! Provides:
//! - [`F64`]: Canonical finite floating-point scalar (`-0.0 -> +0.0`, `Eq`, `Ord`, `Hash`).
//! - [`CdrUnit`] & [`Pt`]: Exact `0.1 µm` (`1/254,000 inch`) integer ticks and typographic points (`1/72 inch`).
//! - [`Point`], [`Size`], [`Rect`], and [`Affine2`]: 2D primitives and checked affine transformations.
//! - [`Path`], [`PathElement`], and [`flatten_path`]: Editable paths and bounded adaptive de Casteljau subdivision.
//! - [`RectSpec`], [`EllipseSpec`], and [`PolygonSpec`]: CorelDRAW parametric shapes (`Round`/`Scalloped`/`Chamfered` corners, `Pie`/`Arc` ellipses, polygons, and stars).

pub mod affine;
pub mod f64_canon;
pub mod flatten;
pub mod path;
pub mod shapes;
pub mod units;

pub use affine::Affine2;
pub use f64_canon::{BBox, EPS, F64, GeomError, Point, Rect, Size, constrain_angle, constrain_angle_from, normalize_deg, reference_point, union_opt};
pub use flatten::{
    DEFAULT_FLATTEN_TOLERANCE, DEFAULT_MAX_FLATTEN_DEPTH, FlattenOpts, Polyline, eval_cubic, flatten_cubic, flatten_path, flatten_quad,
};
pub use path::{FillRule, MAX_PATH_ELEMENTS, NodeKind, Path, PathElement};
pub use shapes::{CornerKind, EllipseMode, EllipseSpec, KAPPA, PolygonSpec, RectSpec};
pub use units::{CDR_TICKS_PER_INCH, CDR_TICKS_PER_MM, CDR_TICKS_PER_PT_DEN, CDR_TICKS_PER_PT_NUM, CdrUnit, PTS_PER_INCH, Pt, Unit};

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// Deterministic SplitMix64 generator for property testing without external test dependencies.
    fn splitmix64(state: &mut u64) -> u64 {
        *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = *state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    #[test]
    fn f64_canonicalizes_neg_zero_and_rejects_non_finite() {
        let neg_zero = F64::new(-0.0).unwrap();
        assert_eq!(neg_zero, F64::ZERO);
        assert_eq!(neg_zero.get().to_bits(), 0.0_f64.to_bits());
        assert_eq!(neg_zero.canon(), neg_zero);

        assert!(matches!(F64::new(f64::NAN), Err(GeomError::NonFiniteCoordinate)));
        assert!(matches!(F64::new(f64::INFINITY), Err(GeomError::NonFiniteCoordinate)));
        assert!(matches!(F64::new(f64::NEG_INFINITY), Err(GeomError::NonFiniteCoordinate)));

        let json = serde_json::to_string(&F64::new(123.456_789).unwrap()).unwrap();
        let back: F64 = serde_json::from_str(&json).unwrap();
        assert_eq!(back, F64::new(123.456_789).unwrap());
        assert!(serde_json::from_str::<F64>("1e999").is_err());

        // Property: random finite bit patterns round-trip bit-identically through serde_json
        let mut seed = 0xC0DE_CAFE_2026_0001_u64;
        for _ in 0..1_000 {
            let bits = splitmix64(&mut seed);
            let raw = f64::from_bits(bits);
            if raw.is_finite() {
                let canon = F64::new(raw).unwrap();
                assert_eq!(canon.canon(), canon);
                let encoded = serde_json::to_vec(&canon).unwrap();
                let decoded: F64 = serde_json::from_slice(&encoded).unwrap();
                assert_eq!(decoded, canon);
            } else {
                assert!(F64::new(raw).is_err());
            }
        }
    }

    #[test]
    fn cdr_unit_and_pt_exact_rational_and_roundtrip_property() {
        // 1 inch = 254,000 CDR ticks = 72 pt = 25.4 mm
        let one_inch_ticks = CdrUnit(CDR_TICKS_PER_INCH);
        let one_inch_pt = one_inch_ticks.to_pt();
        assert!((one_inch_pt.get() - 72.0).abs() < 1e-12);
        assert_eq!(one_inch_pt.try_to_cdr_ticks().unwrap(), one_inch_ticks);
        assert!((one_inch_ticks.to_mm() - 25.4).abs() < 1e-12);
        assert_eq!(CdrUnit::from_mm(25.4).unwrap(), one_inch_ticks);

        // Property: 2,000 random tick positions across ±100 inches round-trip within <= 1 tick
        let mut seed = 0xC0DE_CAFE_2026_0002_u64;
        let max_ticks = 100 * CDR_TICKS_PER_INCH;
        for _ in 0..2_000 {
            let r = (splitmix64(&mut seed) % ((2 * max_ticks) as u64)) as i64 - max_ticks;
            let ticks = CdrUnit(r);
            let back = ticks.to_pt().try_to_cdr_ticks().unwrap();
            assert!((back.0 - ticks.0).abs() <= 1, "drift too large: {} vs {}", back.0, ticks.0);
        }

        assert!(Pt(F64::new(1e300).unwrap()).try_to_cdr_ticks().is_err());
    }

    #[test]
    fn affine2_compose_invert_and_singular_detection() {
        let t = Affine2::translation(10.0, -20.0).unwrap();
        let s = Affine2::scaling(2.0, -3.0).unwrap();
        let r = Affine2::rotation_deg(35.0).unwrap();
        let m = t.then(s).unwrap().then(r).unwrap();
        assert!(m.flips_orientation());

        let inv = m.invert().unwrap();
        let round = m.then(inv).unwrap();
        assert!((round.a.get() - 1.0).abs() < 1e-9);
        assert!(round.b.get().abs() < 1e-9);
        assert!(round.c.get().abs() < 1e-9);
        assert!((round.d.get() - 1.0).abs() < 1e-9);
        assert!(round.tx.get().abs() < 1e-9);
        assert!(round.ty.get().abs() < 1e-9);

        let p = Point::new(17.5, -42.25).unwrap();
        let p_back = inv.apply_point(m.apply_point(p).unwrap()).unwrap();
        assert!(p.distance(p_back) < 1e-9);

        let singular = Affine2::scaling(0.0, 5.0).unwrap();
        assert_eq!(singular.invert(), Err(GeomError::SingularTransform));
    }

    #[test]
    fn adaptive_cubic_flattening_accuracy_and_hostile_depth_cap() {
        let p0 = Point::new(0.0, 0.0).unwrap();
        let c1 = Point::new(25.0, 80.0).unwrap();
        let c2 = Point::new(75.0, -40.0).unwrap();
        let p3 = Point::new(100.0, 20.0).unwrap();

        let mut path = Path::new();
        path.move_to(p0).unwrap();
        path.cubic_to(c1, c2, p3).unwrap();

        let opts = FlattenOpts { tolerance: 0.1, max_depth: 12 };
        let polys = flatten_path(&path, &opts).unwrap();
        assert_eq!(polys.len(), 1);
        let poly = &polys[0];
        assert_eq!(poly.points.first().copied(), Some(p0));
        assert_eq!(poly.points.last().copied(), Some(p3));
        assert!(poly.points.len() > 4 && poly.points.len() <= 4097);

        // Dense parametric samples on the true cubic must lie close to the flattened polyline
        for step in 0..=100 {
            let t = f64::from(step) / 100.0;
            let sample = eval_cubic(p0, c1, c2, p3, t).unwrap();
            let min_dist = poly
                .points
                .windows(2)
                .map(|w| {
                    let ax = w[0].x.get();
                    let ay = w[0].y.get();
                    let dx = w[1].x.get() - ax;
                    let dy = w[1].y.get() - ay;
                    let l2 = dx * dx + dy * dy;
                    let u = if l2 > 1e-12 { (((sample.x.get() - ax) * dx + (sample.y.get() - ay) * dy) / l2).clamp(0.0, 1.0) } else { 0.0 };
                    sample.distance(Point::new(ax + u * dx, ay + u * dy).unwrap())
                })
                .fold(f64::INFINITY, f64::min);
            assert!(min_dist <= opts.tolerance * 1.05, "min_dist {min_dist} > tolerance");
        }

        // Hostile extreme control coordinates must terminate at depth cap (<= 4096 segments)
        let mut hostile_pts = Vec::new();
        flatten_cubic(
            Point::new(-1e100, -1e100).unwrap(),
            Point::new(1e100, -1e100).unwrap(),
            Point::new(-1e100, 1e100).unwrap(),
            Point::new(1e100, 1e100).unwrap(),
            &FlattenOpts { tolerance: 0.001, max_depth: 12 },
            &mut |pt| hostile_pts.push(pt),
        );
        assert!(hostile_pts.len() <= 4096);
        assert!(!hostile_pts.is_empty());
    }

    #[test]
    fn parametric_shapes_round_scalloped_chamfered_pie_arc_star() {
        let r = Rect::new(0.0, 0.0, 100.0, 60.0).unwrap();
        for kind in [CornerKind::Round, CornerKind::Scalloped, CornerKind::Chamfered] {
            let spec = RectSpec::uniform(r, 12.0, kind).unwrap();
            let path = spec.to_path().unwrap();
            let polys = flatten_path(&path, &FlattenOpts::default()).unwrap();
            assert_eq!(polys.len(), 1);
            assert!(polys[0].closed);
            assert!(polys[0].points.len() >= 8);
        }

        // Full, Pie, and Arc ellipses
        let full = EllipseSpec::full(r).to_path().unwrap();
        assert!(flatten_path(&full, &FlattenOpts::default()).unwrap()[0].closed);

        let pie = EllipseSpec::pie(r, 0.0, 135.0).unwrap().to_path().unwrap();
        let pie_poly = &flatten_path(&pie, &FlattenOpts::default()).unwrap()[0];
        assert!(pie_poly.closed);
        assert_eq!(pie_poly.points[0], r.center().unwrap());

        let arc = EllipseSpec::arc(r, 30.0, 210.0).unwrap().to_path().unwrap();
        let arc_poly = &flatten_path(&arc, &FlattenOpts::default()).unwrap()[0];
        assert!(!arc_poly.closed);

        // Regular Polygon and 5-pointed Star
        let center = Point::new(50.0, 50.0).unwrap();
        let hex = PolygonSpec::regular(center, 40.0, 6, 0.0).unwrap().to_path().unwrap();
        assert_eq!(flatten_path(&hex, &FlattenOpts::default()).unwrap()[0].points.len(), 6);

        let star = PolygonSpec::star(center, 40.0, 18.0, 5, 0.0).unwrap().to_path().unwrap();
        assert_eq!(flatten_path(&star, &FlattenOpts::default()).unwrap()[0].points.len(), 10);
    }

    #[test]
    fn reference_points_and_angle_constraints() {
        let r = Rect::new(10.0, 20.0, 110.0, 80.0).unwrap();
        assert_eq!(reference_point(r, 0).unwrap(), Point::new(10.0, 20.0).unwrap());
        assert_eq!(reference_point(r, 4).unwrap(), Point::new(60.0, 50.0).unwrap());
        assert_eq!(reference_point(r, 8).unwrap(), Point::new(110.0, 80.0).unwrap());

        assert_eq!(normalize_deg(195.0), -165.0);
        assert_eq!(normalize_deg(-195.0), 165.0);

        let (cx, cy) = constrain_angle(10.0, 0.5, 15.0);
        assert!(cy.abs() < 1e-9 && (cx - 10.0).abs() < 0.05);
    }
}
