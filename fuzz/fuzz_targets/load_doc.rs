use coralcraft_fuzz::run_fuzz_target;

fn main() {
    let seeds: &[&[u8]] = &[
        br#"{"schema_version":1,"title":"Fuzz","unit":"Millimeters","pages":[{"id":1,"name":"Page 1","width_pt":595.0,"height_pt":842.0,"layers":[]}],"master_layers":[],"palettes":[],"styles":[],"color_settings":{"working_rgb":"sRGB","working_cmyk":"ISO Coated v2","rendering_intent":"RelativeColorimetric"},"next_id":2}"#,
        b"PK\x03\x04\x14\x00\x00\x00\x00\x00",
    ];
    run_fuzz_target(seeds, |data| {
        let _ = coralcraft_doc::load_bytes(data);
    });
}
