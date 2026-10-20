use coralcraft_fuzz::run_fuzz_target;

fn main() {
    let seeds: &[&[u8]] = &[
        br##"<svg width="200" height="100"><rect x="10" y="10" width="80" height="40" fill="#ff00aa" stroke="#112233"/><path d="M 0 0 L 50 50 C 60 60 70 40 90 10 Z"/></svg>"##,
        br#"<svg viewBox="0 0 100 100"><circle cx="50" cy="50" r="25" fill="rgb(10,20,30)"/><polygon points="10,10 90,10 50,80"/></svg>"#,
    ];
    run_fuzz_target(seeds, |data| {
        if let Ok(text) = std::str::from_utf8(data) {
            let _ = coralcraft_doc::import_svg(text);
        }
    });
}
