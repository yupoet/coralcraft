//! `coralcraft-color`: Color spaces, CMYK/HSB/Lab/Spot colors, Fountain Fill stop interpolation,
//! and `.xml`/`.cpl` palette support for CoralCraft.

pub mod fountain;
pub mod palette;
pub mod spaces;

pub use fountain::{FountainBlendMode, FountainStop, interpolate_stops};
pub use palette::{
    CplDetection, MAX_PALETTE_SWATCHES, Palette, PaletteSwatch, detect_cpl, load_palette_bytes, parse_binary_cpl, parse_xml_palette,
    serialize_binary_cpl, serialize_xml_palette,
};
pub use spaces::{
    CVal, Color, ColorError, SpotBlend, bradford_adapt_xyz, cmyk_to_srgb8, hsb_to_srgb8, lab_d50_to_d65, lab_d50_to_srgb8, lab_d65_to_d50,
    lab_d65_to_srgb8, srgb8_to_cmyk, srgb8_to_hsb,
};

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing, clippy::arithmetic_side_effects)]
mod tests {
    use super::*;

    #[test]
    fn cmyk_hsb_lab_spot_and_registration_conversions() {
        let cyan = Color::cmyk(1.0, 0.0, 0.0, 0.0).unwrap();
        assert_eq!(cyan.to_srgb8(), [0, 255, 255]);
        assert_eq!(cyan.to_hex(), "#00ffff");

        let reg = Color::Registration;
        assert_eq!(reg.to_srgb8(), [0, 0, 0]);
        assert_eq!(Color::spot("REGISTRATION", 0.3, [0.1, 0.2, 0.3, 0.4]).unwrap(), Color::Registration);

        // Spot tint 50% of 100% Magenta against white -> 50% Magenta -> (255, 128, 255)
        let spot_mag = Color::spot("Custom Ruby", 0.5, [0.0, 1.0, 0.0, 0.0]).unwrap();
        assert_eq!(spot_mag.to_srgb8_with_spot_policy(SpotBlend::White), [255, 128, 255]);
        assert_eq!(spot_mag.to_srgb8_with_spot_policy(SpotBlend::Fallback), [255, 128, 255]);

        // Hex round-trip
        let parsed = Color::from_hex("#ed1c24").unwrap();
        assert_eq!(parsed, Color::rgb8(237, 28, 36));
        assert_eq!(parsed.to_hex(), "#ed1c24");
    }

    #[test]
    fn hsb_and_cmyk_11_cubed_lattice_roundtrip_within_1_lsb() {
        // 11^3 = 1,331 RGB lattice points -> HSB -> sRGB8 and -> CMYK -> sRGB8 within ±1 LSB
        for ri in 0..=10 {
            for gi in 0..=10 {
                for bi in 0..=10 {
                    let r = ((ri * 255 + 5) / 10) as u8;
                    let g = ((gi * 255 + 5) / 10) as u8;
                    let b = ((bi * 255 + 5) / 10) as u8;

                    let [h, s, v] = srgb8_to_hsb(r, g, b);
                    let [r2, g2, b2] = hsb_to_srgb8(h, s, v);
                    assert!(r.abs_diff(r2) <= 1 && g.abs_diff(g2) <= 1 && b.abs_diff(b2) <= 1);

                    let [c, m, y, k] = srgb8_to_cmyk(r, g, b);
                    let [r3, g3, b3] = cmyk_to_srgb8(c, m, y, k);
                    assert!(r.abs_diff(r3) <= 1 && g.abs_diff(g3) <= 1 && b.abs_diff(b3) <= 1);
                }
            }
        }
    }

    #[test]
    fn lab_d50_d65_bradford_roundtrip_and_white_point_accuracy() {
        // White point L*=100, a*=0, b*=0 in both D50 and D65 maps to pure white (255, 255, 255)
        assert_eq!(lab_d50_to_srgb8(100.0, 0.0, 0.0), [255, 255, 255]);
        assert_eq!(lab_d65_to_srgb8(100.0, 0.0, 0.0), [255, 255, 255]);
        assert_eq!(lab_d50_to_srgb8(0.0, 0.0, 0.0), [0, 0, 0]);

        // D50 -> D65 -> D50 Bradford round-trip error < 0.01 delta-E
        for &(l, a, b) in &[(100.0, 0.0, 0.0), (50.0, 25.0, -30.0), (75.0, -40.0, 60.0), (20.0, 10.0, 15.0)] {
            let [l65, a65, b65] = lab_d50_to_d65(l, a, b);
            let [l50, a50, b50] = lab_d65_to_d50(l65, a65, b65);
            let de = ((l - l50).powi(2) + (a - a50).powi(2) + (b - b50).powi(2)).sqrt();
            assert!(de < 0.01, "Bradford round-trip drift {de} for ({l}, {a}, {b})");
        }
    }

    #[test]
    fn fountain_fill_direct_rainbow_cw_ccw_and_midpoint_skew() {
        let red = Color::rgb8(255, 0, 0); // H = 0
        let blue = Color::rgb8(0, 0, 255); // H = 240

        // Direct mode with midpoint 0.5 at t = 0.5 -> (128, 0, 128)
        let stops = vec![FountainStop::with_midpoint(0.0, 0.5, red.clone()).unwrap(), FountainStop::with_midpoint(1.0, 0.5, blue.clone()).unwrap()];
        assert_eq!(interpolate_stops(&stops, FountainBlendMode::Direct, 0.0).unwrap(), [255, 0, 0]);
        assert_eq!(interpolate_stops(&stops, FountainBlendMode::Direct, 0.5).unwrap(), [128, 0, 128]);
        assert_eq!(interpolate_stops(&stops, FountainBlendMode::Direct, 1.0).unwrap(), [0, 0, 255]);

        // Midpoint skew m = 0.25 reaches the 50/50 blend at t = 0.25!
        let skewed = vec![FountainStop::with_midpoint(0.0, 0.25, red.clone()).unwrap(), FountainStop::with_midpoint(1.0, 0.5, blue.clone()).unwrap()];
        assert_eq!(interpolate_stops(&skewed, FountainBlendMode::Direct, 0.25).unwrap(), [128, 0, 128]);

        // Rainbow CCW from Red (0°) to Blue (240°) at t = 0.5 passes through Green (120°) -> (0, 255, 0)
        assert_eq!(interpolate_stops(&stops, FountainBlendMode::RainbowCcw, 0.5).unwrap(), [0, 255, 0]);

        // Rainbow CW from Red (0°/360°) to Blue (240°) at t = 0.5 passes through Magenta (300°) -> (255, 0, 255)
        assert_eq!(interpolate_stops(&stops, FountainBlendMode::RainbowCw, 0.5).unwrap(), [255, 0, 255]);
    }

    #[test]
    fn xml_palette_roundtrip_xxe_guard_and_cpl_detection() {
        let xml = r#"<palette name="Test Prepress" guid="1234-5678">
            <colors>
                <cs type="1" name="Pure Red" r="255" g="0" b="0"/>
                <cs type="2" name="Process Cyan" c="100" m="0" y="0" k="0"/>
                <cs type="3" name="Bright Green" h="120" s="100" b="100"/>
                <cs type="4" name="Lab Sample" l="60" a="20" b="-30"/>
                <cs type="5" name="Custom Spot 186" c="0" m="100" y="80" k="5" tint="50"/>
                <cs type="6" name="Registration"/>
            </colors>
        </palette>"#;

        let pal1 = parse_xml_palette(xml).unwrap();
        assert_eq!(pal1.name, "Test Prepress");
        assert_eq!(pal1.swatches.len(), 6);

        let serialized = serialize_xml_palette(&pal1);
        let pal2 = parse_xml_palette(&serialized).unwrap();
        assert_eq!(pal1, pal2);

        // XXE/DTD must be rejected
        assert!(parse_xml_palette("<!DOCTYPE foo><palette name=\"x\"/>").is_err());

        // .cpl binary header detection: 0 false positive on XML, true positive on 0xCDCD header
        assert_eq!(detect_cpl(xml.as_bytes()), CplDetection::NotCpl);
        let cpl_bytes = [0xCD, 0xCD, 0x0C, 0x00, 0x00, 0x00, 0x00, 0x00];
        assert_eq!(detect_cpl(&cpl_bytes), CplDetection::Cpl { version: 0xCDCD, color_count: 12 });
        assert_eq!(load_palette_bytes(&cpl_bytes), Err(ColorError::UnsupportedBinaryCpl));

        // Full binary .cpl round-trip (`0xCDDC` magic)
        let default_pal = Palette::default_workspace_palette();
        let bin_cpl = serialize_binary_cpl(&default_pal);
        assert_eq!(detect_cpl(&bin_cpl), CplDetection::Cpl { version: 0xCDDC, color_count: default_pal.swatches.len() as u16 });
        let decoded_cpl = load_palette_bytes(&bin_cpl).unwrap();
        assert_eq!(decoded_cpl, default_pal);
    }

    #[test]
    fn xml_palette_1_percent_and_fractional_percent_roundtrip_exactly() {
        let pal = Palette {
            name: "Low Tint Test".to_owned(),
            guid: None,
            swatches: vec![
                PaletteSwatch { name: "1% Cyan".to_owned(), color: Color::cmyk(0.01, 0.0, 0.0, 0.0).unwrap() },
                PaletteSwatch { name: "0.5% Magenta".to_owned(), color: Color::cmyk(0.0, 0.005, 0.0, 0.0).unwrap() },
                PaletteSwatch { name: "1% HSB".to_owned(), color: Color::hsb(180.0, 0.01, 0.01).unwrap() },
                PaletteSwatch { name: "1% Spot".to_owned(), color: Color::spot("Spot 1%", 0.01, [0.01, 0.0, 0.0, 0.0]).unwrap() },
                PaletteSwatch { name: "1% Gray".to_owned(), color: Color::gray(0.01).unwrap() },
            ],
        };
        let xml = serialize_xml_palette(&pal);
        let roundtripped = parse_xml_palette(&xml).unwrap();
        assert_eq!(roundtripped, pal);
    }
}
