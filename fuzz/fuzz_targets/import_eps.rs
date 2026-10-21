use coralcraft_fuzz::run_fuzz_target;

fn main() {
    let seeds: &[&[u8]] = &[
        b"%!PS-Adobe-3.0 EPSF-3.0\n%%BoundingBox: 0 0 200 100\ngsave\n1 0 0 setrgbcolor\n10 10 80 40 rectfill\nnewpath 100 50 20 0 180 arc closepath stroke\n/Helvetica findfont 12 scalefont setfont 20 20 moveto (Hi \\101) show\ngrestore\nshowpage\n%%EOF\n",
    ];
    run_fuzz_target(seeds, |data| {
        if let Ok(text) = std::str::from_utf8(data) {
            let _ = coralcraft_doc::import_eps(text);
        }
    });
}
