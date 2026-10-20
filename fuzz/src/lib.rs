//! Clean-room deterministic and corpus-driven fuzzing harness for CoralCraft untrusted-input parsers (`R7.3`).

/// Runs a fuzz target closure against initial seed inputs plus deterministic PRNG mutations.
/// If a directory or file path is passed on the command line, also runs against every file in that path.
pub fn run_fuzz_target<F>(seeds: &[&[u8]], mut target: F)
where
    F: FnMut(&[u8]),
{
    for seed in seeds {
        target(seed);
    }

    for arg in std::env::args().skip(1) {
        let path = std::path::Path::new(&arg);
        if path.is_file() {
            if let Ok(bytes) = std::fs::read(path) {
                target(&bytes);
            }
        } else if path.is_dir()
            && let Ok(entries) = std::fs::read_dir(path)
        {
            for entry in entries.flatten() {
                if let Ok(bytes) = std::fs::read(entry.path()) {
                    target(&bytes);
                }
            }
        }
    }

    let iterations: usize = std::env::var("CORALCRAFT_FUZZ_ITERS").ok().and_then(|s| s.parse().ok()).unwrap_or(1_000);

    let mut state: u64 = 0x5EED_C0DE_2026_1021;
    let mut next_u64 = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };

    for i in 0..iterations {
        let base = if seeds.is_empty() { &[][..] } else { seeds[i % seeds.len()] };
        let mut buf = base.to_vec();
        let ops = (next_u64() % 8) + 1;
        for _ in 0..ops {
            match next_u64() % 4 {
                0 if !buf.is_empty() => {
                    let idx = (next_u64() as usize) % buf.len();
                    buf[idx] ^= (next_u64() & 0xFF) as u8;
                }
                1 if !buf.is_empty() => {
                    let len = (next_u64() as usize) % buf.len();
                    buf.truncate(len);
                }
                2 => {
                    let extra = ((next_u64() % 16) + 1) as usize;
                    for _ in 0..extra {
                        buf.push((next_u64() & 0xFF) as u8);
                    }
                }
                _ if buf.len() >= 4 => {
                    let idx = (next_u64() as usize) % (buf.len() - 3);
                    let pat = match next_u64() % 4 {
                        0 => [0xFF, 0xFF, 0xFF, 0x7F],
                        1 => [0xFF, 0xFF, 0xFF, 0xFF],
                        2 => [0x00, 0x00, 0x00, 0x80],
                        _ => [0x00, 0x00, 0x00, 0x00],
                    };
                    buf[idx..idx + 4].copy_from_slice(&pat);
                }
                _ => {}
            }
        }
        target(&buf);
    }
}
