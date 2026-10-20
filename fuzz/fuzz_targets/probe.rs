use coralcraft_fuzz::run_fuzz_target;

fn main() {
    let seeds: &[&[u8]] = &[
        b"RIFF\x14\x00\x00\x00CDR7vrsn\x02\x00\x00\x00\xbc\x02",
        b"RIFF\x0c\x00\x00\x00CMX1cont\x00\x00\x00\x00",
        b"PK\x03\x04\x14\x00\x00\x00\x00\x00",
        b"WL\x00\x00\x00\x00",
    ];
    run_fuzz_target(seeds, |data| {
        let _ = coralcraft_cdr::probe(data);
    });
}
