use coralcraft_fuzz::run_fuzz_target;

fn main() {
    let seeds: &[&[u8]] = &[
        b"0\nSECTION\n2\nENTITIES\n0\nLINE\n10\n0.0\n20\n0.0\n11\n100.0\n21\n50.0\n0\nCIRCLE\n10\n50.0\n20\n50.0\n40\n25.0\n0\nENDSEC\n0\nEOF\n",
        b"0\nSECTION\n2\nENTITIES\n0\nLWPOLYLINE\n70\n1\n10\n0.0\n20\n0.0\n10\n80.0\n20\n0.0\n10\n40.0\n20\n60.0\n0\nENDSEC\n0\nEOF\n",
    ];
    run_fuzz_target(seeds, |data| {
        if let Ok(text) = std::str::from_utf8(data) {
            let _ = coralcraft_doc::import_dxf(text);
        }
    });
}
