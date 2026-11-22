//! `coralcraft-cli perf`: the performance budgets (plan §4) measured on a synthetic document.
//!
//! Each row is the median of several runs. Timings are wall-clock: on a busy machine (a load
//! average above three quarters of the cores, or on Windows more than three quarters of the
//! cores busy over a second) they are noise, and the report says so.

use std::time::Instant;

use serde_json::json;
use vectorcraft_color::{Color, Paint};
use vectorcraft_doc::{Appearance, Document, Node, hit};
use vectorcraft_geom::{Affine, Point, Rect, shapes};
use vectorcraft_render::{RenderOptions, Renderer};

/// Deterministic xorshift in 0..1.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % 1_000_000) as f64 / 1_000_000.0
    }
}

/// The synthetic document's page, in points.
const SYNTHETIC_PAGE: (f64, f64) = (1600.0, 1200.0);

/// `n` mixed paths (ellipses with strokes, translucent rectangles, stroked stars) on a 1600×1200 page.
pub fn synthetic(n: usize) -> Result<Document, String> {
    let mut d = Document::new(SYNTHETIC_PAGE.0, SYNTHETIC_PAGE.1);
    let l = d.layers.first().map(|l| l.id);
    let mut r = Rng(42);
    for i in 0..n {
        let (x, y, s) = (r.next() * SYNTHETIC_PAGE.0, r.next() * SYNTHETIC_PAGE.1, 3.0 + r.next() * 22.0);
        let c = Color::rgb(r.next() as f32, r.next() as f32, r.next() as f32);
        let id = d.alloc_id();
        let mut node = match i % 3 {
            0 => Node::path(
                id,
                shapes::ellipse(Rect::from_center_size(Point::new(x, y), (2.0 * s, 2.0 * s))),
                Appearance::basic(Paint::solid(c), Paint::solid(Color::BLACK), 0.5),
            ),
            1 => Node::path(id, shapes::rectangle(Rect::new(x, y, x + 2.0 * s, y + s)), Appearance::basic(Paint::solid(c), Paint::None, 0.0)),
            _ => Node::path(id, shapes::star(Point::new(x, y), s, s / 2.0, 5, 0.0), Appearance::basic(Paint::None, Paint::solid(c), 2.0)),
        };
        if i % 3 == 1 {
            node.opacity = 0.8;
        }
        d.insert(l, usize::MAX, node).map_err(|e| format!("building the synthetic document: {e}"))?;
    }
    Ok(d)
}

fn median_ms(runs: usize, mut f: impl FnMut()) -> f64 {
    let mut v: Vec<f64> = (0..runs)
        .map(|_| {
            let t = Instant::now();
            f();
            t.elapsed().as_secs_f64() * 1000.0
        })
        .collect();
    v.sort_by(f64::total_cmp);
    v.get(v.len() / 2).copied().unwrap_or(0.0)
}

/// 1-minute load average (macOS / Linux), if available.
#[cfg(not(windows))]
fn load_average() -> Option<f64> {
    let out = std::process::Command::new("sysctl").args(["-n", "vm.loadavg"]).output().ok().filter(|o| o.status.success());
    let text = match out {
        Some(o) => String::from_utf8_lossy(&o.stdout).to_string(),
        None => std::fs::read_to_string("/proc/loadavg").ok()?,
    };
    text.split_whitespace().map(|t| t.trim_matches(|c| c == '{' || c == '}')).find_map(|t| t.parse().ok())
}

/// The cores busy on average over one second (Windows), if available: the share of the
/// processors' time that `GetSystemTimes` reports as kernel or user time but not as idle time,
/// times `cores`. A load average also counts the threads waiting for a core.
#[cfg(windows)]
fn busy_cores(cores: usize) -> Option<f64> {
    let times = || winsafe::GetSystemTimes().ok().map(|(idle, kernel, user)| [idle, kernel, user].map(u64::from));
    let before = times()?;
    std::thread::sleep(std::time::Duration::from_secs(1));
    busy_share(before, times()?).map(|share| share * cores as f64)
}

/// The share of the processors' time between two `[idle, kernel, user]` readings of
/// `GetSystemTimes` that was busy (kernel time includes idle time). It is None when no time
/// passed, a time went back, or the idle time exceeds the kernel and user time.
#[cfg(any(windows, test))]
fn busy_share([idle0, kernel0, user0]: [u64; 3], [idle1, kernel1, user1]: [u64; 3]) -> Option<f64> {
    let total = kernel1.checked_sub(kernel0)?.checked_add(user1.checked_sub(user0)?)?;
    let busy = total.checked_sub(idle1.checked_sub(idle0)?)?;
    (total > 0).then(|| busy as f64 / total as f64)
}

/// Milliseconds per pointer move of a Direct Selection drag of a handle, then of a segment, of a
/// 5,000-anchor wave of smooth anchors added on top of the synthetic `n`-path document.
fn edit_drags(n: usize) -> Result<(f64, f64), String> {
    use vectorcraft_engine::tools::{Mods, PointerEvent, PointerKind};
    use vectorcraft_geom::{Anchor, PathData, SubPath, Vec2};
    let mut doc = synthetic(n)?;
    let l = doc.layers.first().map(|l| l.id);
    let id = doc.alloc_id();
    // Anchors 2 pt apart, up and down by 20 pt, their handles 0.6 pt long.
    let anchor = |i: usize| {
        let p = Point::new(i as f64 * 2.0, if i.is_multiple_of(2) { 500.0 } else { 520.0 });
        Anchor::smooth(p, p + Vec2::new(0.6, 0.0))
    };
    let path = PathData::single(SubPath::new((0..5000).map(anchor).collect(), false));
    doc.insert(l, usize::MAX, Node::path(id, path, Appearance::default_art())).map_err(|e| e.to_string())?;
    let mut s = vectorcraft_engine::Session::new();
    s.add_document(doc, None);
    let view = vectorcraft_engine::ViewInfo { zoom: 8.0, ..Default::default() };
    let err = |e: vectorcraft_engine::EngineError| e.to_string();
    let mut drag = |down: Point| -> Result<f64, String> {
        let ev =
            |kind, y: f64| PointerEvent { kind, pos: down + Vec2::new(0.0, y), mods: Mods::default(), backquote: false, pressure: 1.0, pen: None };
        s.execute("select.anchors", &json!({"id": id.0, "anchors": [[0, 400]]})).map_err(err)?;
        s.select_tool("directSelection", view).map_err(err)?;
        let undo = s.doc().map_err(err)?.history.undo.len();
        s.pointer(&ev(PointerKind::Down, 0.0), view).map_err(err)?;
        let mut y = 0.0;
        let ms = median_ms(9, || {
            y += 3.0;
            // A failed move shows as the missing undo step below.
            let _ = s.pointer(&ev(PointerKind::Drag, y), view);
        });
        s.pointer(&ev(PointerKind::Up, y), view).map_err(err)?;
        if s.doc().map_err(err)?.history.undo.len() != undo + 1 {
            return Err(format!("the drag from {down:?} made no undo step"));
        }
        Ok(ms)
    };
    // Anchor 400 is at (800, 500): its outgoing handle's end, then its segment to anchor 401.
    let handle = drag(Point::new(800.6, 500.0))?;
    let segment = drag(Point::new(801.0, 510.0))?;
    Ok((handle, segment))
}

/// Resident memory of this process in MB (`ps`), if available, with the most it has held so far
/// on Linux (`VmHWM` in `/proc/self/status`). The peak is None on macOS, which has no `/proc`.
#[cfg(not(windows))]
fn rss_mb() -> Option<(f64, Option<f64>)> {
    let pid = std::process::id().to_string();
    let out = std::process::Command::new("ps").args(["-o", "rss=", "-p", &pid]).output().ok().filter(|o| o.status.success())?;
    let peak = std::fs::read_to_string("/proc/self/status").ok().and_then(|status| vm_hwm_mb(&status));
    String::from_utf8_lossy(&out.stdout).trim().parse::<f64>().ok().map(|kb| (kb / 1024.0, peak))
}

/// The peak resident set size in MB from the `VmHWM` line of a `/proc/<pid>/status` text, which
/// gives it in kB. It is None when the text has no `VmHWM` line in kB.
#[cfg(any(not(windows), test))]
fn vm_hwm_mb(status: &str) -> Option<f64> {
    let kb = status.lines().find_map(|line| line.strip_prefix("VmHWM:"))?.trim().strip_suffix("kB")?.trim().parse::<f64>().ok()?;
    Some(kb / 1024.0)
}

/// The working set of this process in MB (`GetProcessMemoryInfo`), if available: its pages that
/// are in physical memory, the figure `ps` reports as resident memory elsewhere. The same call
/// gives the peak working set, the most the working set has held so far.
#[cfg(windows)]
fn rss_mb() -> Option<(f64, Option<f64>)> {
    let counters = winsafe::HPROCESS::GetCurrentProcess().GetProcessMemoryInfo().ok()?;
    let mb = |bytes: usize| bytes as f64 / (1024.0 * 1024.0);
    Some((mb(counters.WorkingSetSize), Some(mb(counters.PeakWorkingSetSize))))
}

/// A tracked row of `--deep`: the work's name and its median in ms. Names are padded to 54
/// characters.
fn tracked_row(name: &str, ms: f64) -> String {
    format!("  {name:<54} {ms:>10.2} ms")
}

/// A memory row of `--deep`: the reading's name and value in MB, then the peak so far when
/// there is one. Names are padded to 56 characters.
fn memory_row(name: &str, mb: f64, peak: Option<f64>) -> String {
    let peak = peak.map_or_else(String::new, |peak| format!("  peak {peak:>8.0} MB"));
    format!("  {name:<56} {mb:>8.0} MB{peak}")
}

/// A row of the budget table: the budget's name, the median and the budget in ms, then "ok" or
/// "OVER". Names are padded to 42 characters, as in the table's header.
fn budget_row(name: &str, ms: f64, budget: f64, ok: bool) -> String {
    format!("  {name:<42} {ms:>8.2} ms {budget:>7.0} ms  {}", if ok { "ok" } else { "OVER" })
}

/// Tracked rows for pointing at things in an n-path document: what hovering, clicking, marquee
/// selection and snapping cost when nothing narrows the search.
fn pointing(n: usize, rows: &mut Vec<(String, f64)>) -> Result<(), String> {
    use vectorcraft_engine::tools::{Mods, PointerEvent, PointerKind};
    let err = |e: vectorcraft_engine::EngineError| e.to_string();
    let mut s = vectorcraft_engine::Session::new();
    s.add_document(synthetic(n)?, None);
    let view = vectorcraft_engine::ViewInfo { zoom: 1.0, ..Default::default() };
    s.select_tool("selection", view).map_err(err)?;
    // Off the art (nothing under the pointer), the case every hover over empty canvas pays.
    let off = Point::new(-200.0, -200.0);
    let hover = |s: &mut vectorcraft_engine::Session| {
        let _ = s.pointer(&PointerEvent::new(PointerKind::Move, off.x, off.y), view);
        let _ = s.cursor(off, Mods::default(), view);
    };
    rows.push((format!("hover over empty canvas ({n} paths)"), median_ms(5, || hover(&mut s))));
    s.execute("select.all", &json!({})).map_err(err)?;
    rows.push((format!("hover over empty canvas, all selected ({n} paths)"), median_ms(5, || hover(&mut s))));
    let doc = s.doc().map_err(err)?.doc.clone();
    rows.push((format!("hit test, a miss ({n} paths)"), median_ms(5, || drop(hit::hit_test(&doc, off, hit::HitOptions::default())))));
    let r = Rect::new(600.0, 450.0, 1000.0, 750.0);
    rows.push((format!("marquee 400×300 ({n} paths)"), median_ms(5, || drop(hit::marquee(&doc, r, None, false)))));
    let view_rect = Rect::new(0.0, 0.0, 1600.0, 1200.0);
    rows.push((
        format!("smart guide targets in view ({n} paths)"),
        median_ms(5, || drop(vectorcraft_engine::tools::guides::Targets::collect(&doc, &[], Some(view_rect)))),
    ));
    // Adding one more object to a selection of half the document: the command a shift-click ends
    // in, without the pointer handling and hit test before it.
    let ids: Vec<u64> = doc.layers[0].children().map(|c| c.iter().map(|n| n.id.0).collect()).unwrap_or_default();
    s.execute("select.set", &json!({"ids": ids[..ids.len() / 2]})).map_err(err)?;
    let mut k = ids.len() / 2;
    rows.push((
        format!("select.toggle in a {}-object selection", ids.len() / 2),
        median_ms(5, || {
            k += 1;
            let _ = s.execute("select.toggle", &json!({"id": ids[k % ids.len()]}));
        }),
    ));
    Ok(())
}

/// `--deep`: tracked rows (no budget yet) for work the budgets above don't reach: editing a whole
/// selection, live effects, every Pathfinder operation, typing in a long story, and memory, with
/// the peak so far where `rss_mb` reads one.
fn deep(n: usize) -> Result<(), String> {
    let err = |e: vectorcraft_engine::EngineError| e.to_string();
    let mut rows: Vec<(String, f64)> = vec![];
    let mut mem: Vec<(String, f64, Option<f64>)> = vec![];
    let mut note = |what: &str| {
        if let Some((m, peak)) = rss_mb() {
            mem.push((what.to_string(), m, peak));
        }
    };
    note("start");
    // Editing every object of an n-path document.
    let mut s = vectorcraft_engine::Session::new();
    s.add_document(synthetic(n)?, None);
    note("document built");
    rows.push((format!("select all, {n} paths"), median_ms(3, || drop(s.execute("select.all", &json!({}))))));
    rows.push((format!("nudge the selection ({n} paths)"), median_ms(5, || drop(s.execute("object.move", &json!({"dx": 1, "dy": 0}))))));
    rows.push((format!("undo a nudge ({n} paths)"), median_ms(5, || drop(s.execute("edit.undo", &json!({}))))));
    rows.push((format!("redo a nudge ({n} paths)"), median_ms(5, || drop(s.execute("edit.redo", &json!({}))))));
    // Dragging the whole selection with the Selection tool: one pointer move (the preview).
    {
        use vectorcraft_engine::tools::{Mods, PointerEvent, PointerKind};
        use vectorcraft_geom::Vec2;
        let view = vectorcraft_engine::ViewInfo { zoom: 1.0, ..Default::default() };
        let first = s.doc().map_err(err)?.selection.objects.first().copied();
        let at = first.and_then(|id| s.doc().ok()?.doc.node(id)?.geometric_bounds()).map(|b| b.center());
        if let Some(down) = at {
            let ev = |kind, dx: f64| PointerEvent {
                kind,
                pos: down + Vec2::new(dx, 0.0),
                mods: Mods::default(),
                backquote: false,
                pressure: 1.0,
                pen: None,
            };
            s.select_tool("selection", view).map_err(err)?;
            s.pointer(&ev(PointerKind::Down, 0.0), view).map_err(err)?;
            let mut dx = 0.0;
            rows.push((
                format!("drag the selection, per move ({n} paths)"),
                median_ms(5, || {
                    dx += 2.0;
                    let _ = s.pointer(&ev(PointerKind::Drag, dx), view);
                }),
            ));
            s.pointer(&ev(PointerKind::Up, dx), view).map_err(err)?;
        }
    }
    for _ in 0..40 {
        s.execute("object.move", &json!({"dx": 1, "dy": 0})).map_err(err)?;
    }
    note("after 45 whole-selection edits");
    drop(s);
    // Live effects on 2,000 paths: render a page whose every object has the effect.
    let opts = RenderOptions::default();
    for (fx, params) in [("path.offsetPath", json!({"offset": 3})), ("distort.roughen", json!({"size": 5, "relative": true}))] {
        let mut s = vectorcraft_engine::Session::new();
        s.add_document(synthetic(2000)?, None);
        s.execute("select.all", &json!({})).map_err(err)?;
        s.execute("effect.apply", &json!({"effect": fx, "params": params})).map_err(err)?;
        let doc = s.doc().map_err(err)?.doc.clone();
        let mut r = Renderer::new();
        r.render(&doc, 2880, 1800, Affine::scale(1.6), &opts);
        rows.push((format!("render, 2,000 paths with {fx} (warm)"), median_ms(5, || drop(r.render(&doc, 2880, 1800, Affine::scale(1.6), &opts)))));
    }
    note("after live effects");
    // Every Pathfinder operation on 300 overlapping paths.
    for op in ["unite", "intersect", "exclude", "minusFront", "minusBack", "divide", "trim", "merge", "crop", "outline"] {
        let ms = median_ms(1, || {
            let mut s = vectorcraft_engine::Session::new();
            if let Ok(d) = synthetic(300) {
                s.add_document(d, None);
                let _ = s.execute("select.all", &json!({})).and_then(|_| s.execute(&format!("object.pathfinder.{op}"), &json!({})));
            }
        });
        rows.push((format!("Pathfinder {op}, 300 paths"), ms));
    }
    note("after Pathfinder");
    // Typing one character in the middle of a 20,000-character area-type story.
    let mut s = vectorcraft_engine::Session::new();
    s.add_document(Document::new(1600.0, 1200.0), None);
    let text: String = (0..2000).map(|i| if i % 12 == 11 { "paragraph.\n" } else { "lorem ips " }).collect();
    let id = s.execute("text.create", &json!({"x": 20, "y": 20, "text": text, "size": 10, "area": {"width": 1500, "height": 1100}})).map_err(err)?;
    let id = id.get("id").cloned().unwrap_or(json!(0));
    let mut at = 10_000;
    rows.push((
        "type one character, 20,000-character area text".into(),
        median_ms(9, || {
            at += 1;
            let _ = s.execute("text.editRange", &json!({"id": id, "start": at, "end": at, "insert": "x"}));
        }),
    ));
    note("after typing");
    pointing(n, &mut rows)?;
    note("after pointing");
    // Four embedded 24 MP photos drawn at fit page by three renderers, as the app's canvas, render
    // worker and a panel each draw them.
    {
        let art = synthetic(3000)?;
        let mut d = Document::new(1600.0, 1200.0);
        let l = d.layers.first().map(|l| l.id);
        for i in 0..4u32 {
            let photo = Renderer::new().render(&art, 6000, 4000, Affine::scale(3.75 + f64::from(i) * 0.1), &opts).to_png()?;
            let key = format!("photo {i}");
            d.images.insert(key.clone(), vectorcraft_doc::ImageBlob::new("image/png", photo));
            let xf = Affine::translate((f64::from(i % 2) * 800.0, f64::from(i / 2) * 600.0)) * Affine::scale(800.0 / 6000.0);
            let image = vectorcraft_doc::ImageObject { key, width: 6000, height: 4000, xf, link: None, placement: Default::default(), pixels: None };
            let id = d.alloc_id();
            d.insert(l, usize::MAX, Node::new(id, vectorcraft_doc::NodeKind::Image(Box::new(image)))).map_err(|e| e.to_string())?;
        }
        note("4 embedded 24 MP photos");
        let mut renderers = [Renderer::new(), Renderer::new(), Renderer::new()];
        let ms = median_ms(1, || {
            for r in &mut renderers {
                r.render(&d, 2880, 1800, Affine::scale(1.6), &opts);
            }
        });
        rows.push(("render 4 embedded 24 MP photos with 3 renderers (cold)".into(), ms));
        note("after rendering the photos with 3 renderers");
    }
    // An SVG with one embedded picture opened from its text, as picture-heavy SVGs come: the
    // importer's passes over the text before usvg reads it, most of which is the data URL. The
    // picture is the synthetic page drawn at 1.25x (about 2 MB as a PNG; the row names its size);
    // the hidden rectangle is a marker, so the prepass walks the tree as it does for a file that
    // has one, instead of returning at its first check.
    {
        let k = 1.25;
        let (w, h) = ((SYNTHETIC_PAGE.0 * k).round() as u32, (SYNTHETIC_PAGE.1 * k).round() as u32);
        let photo = Renderer::new().render(&synthetic(3000)?, w, h, Affine::scale(k), &opts).to_png()?;
        let data = vectorcraft_svg::base64_encode(&photo);
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}"><image width="{w}" height="{h}" href="data:image/png;base64,{data}"/><rect width="100" height="100" style="display:none"/></svg>"#
        );
        // One import checked first, so a failure is reported rather than timed; the same text
        // imports the same way each time. Its document is kept for the memory reading.
        let imported = vectorcraft_svg::import_with_report(&svg).map_err(|e| e.to_string())?;
        let ms = median_ms(5, || drop(vectorcraft_svg::import_with_report(&svg)));
        rows.push((format!("import SVG, one embedded {} KB PNG", photo.len() / 1024), ms));
        note("after the import of an embedded picture");
        drop(imported);
    }
    // Output hashes, to compare builds: the SVG export and a save → open → save round trip.
    let fnv = |b: &[u8]| b.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, &x| (h ^ u64::from(x)).wrapping_mul(0x100_0000_01b3));
    let doc = synthetic(n)?;
    let svg = vectorcraft_svg::export(&doc, &vectorcraft_svg::ExportOptions::default());
    let saved = vectorcraft_format::save_file(&doc);
    let reopened = vectorcraft_format::load(&saved).map_err(|e| e.to_string())?;
    outln!("  output hash: SVG export {:016x}, save→open→save {:016x}", fnv(svg.as_bytes()), fnv(&vectorcraft_format::save_file(&reopened)));
    note("after the SVG export and save→open→save");
    outln!("  tracked (no budget yet)");
    for (name, ms) in &rows {
        outln!("{}", tracked_row(name, *ms));
    }
    // `rss_mb` reads the working set on Windows.
    let memory = if cfg!(windows) { "working set" } else { "memory" };
    for (what, mb, peak) in &mem {
        outln!("{}", memory_row(&format!("{memory} {what}"), *mb, *peak));
    }
    Ok(())
}

pub fn run(args: &[String]) -> Result<(), String> {
    if crate::help_requested(args, &["--paths"]) {
        return crate::help();
    }
    let mut n = 50_000usize;
    let mut want_deep = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--deep" => want_deep = true,
            "--bench" => {
                let which = it.next().cloned().ok_or("--bench needs zoom")?;
                let ms = it.next().and_then(|v| v.parse().ok()).unwrap_or(3000);
                return match which.as_str() {
                    "zoom" => bench_zoom(ms),
                    "save" => bench_save(ms),
                    other => Err(format!("unknown bench `{other}` (zoom|save)")),
                };
            }
            "--class" => {
                let which = it.next().cloned().ok_or("--class needs illustration|brochure|poster")?;
                return class_report(&which);
            }
            "--paths" => n = it.next().and_then(|v| v.parse().ok()).ok_or("--paths needs a number")?,
            other => return Err(format!("unknown perf option `{other}`")),
        }
    }
    let cores = std::thread::available_parallelism().map_or(1, |c| c.get());
    // The figure is the 1-minute load average, or on Windows the cores busy over one second.
    #[cfg(not(windows))]
    let (load, label) = (load_average(), "load average");
    #[cfg(windows)]
    let (load, label) = (busy_cores(cores), "cores busy");
    outln!("CoralCraft performance budgets — {n} paths, {cores} cores, {label} {}", load.map_or("?".into(), |l| format!("{l:.1}")));
    let noisy = load.is_some_and(|l| l > cores as f64 * 0.75);
    if noisy {
        outln!("WARNING: the machine is busy; wall-clock timings below are not trustworthy.");
    }
    let t = Instant::now();
    let doc = synthetic(n)?;
    outln!("  (built in {:.0} ms)", t.elapsed().as_secs_f64() * 1000.0);

    let mut rows: Vec<(&str, f64, f64)> = vec![];
    let opts = RenderOptions::default();
    let mut r = Renderer::new();
    let fit = Affine::scale(1.6);
    r.render(&doc, 2880, 1800, fit, &opts);
    // The canvas never waits for these: while panning/zooming it shows the last frame reprojected
    // and re-renders in the background, so the budget is for that refresh to land quickly.
    rows.push(("render: fit page, 2880×1800", median_ms(5, || drop(r.render(&doc, 2880, 1800, fit, &opts))), 100.0));
    let mut dx = 0.0;
    rows.push((
        "render: background refresh after a pan",
        median_ms(9, || {
            dx += 7.0;
            drop(r.render(&doc, 2880, 1800, Affine::translate((dx, 0.0)) * fit, &opts));
        }),
        100.0,
    ));
    let zoom = Affine::scale(6.4) * Affine::translate((-600.0, -400.0));
    r.render(&doc, 2880, 1800, zoom, &opts);
    // Zoomed in, culling leaves few objects: this one should fit a frame.
    rows.push(("render: 400% zoom, 2880×1800", median_ms(5, || drop(r.render(&doc, 2880, 1800, zoom, &opts))), 16.0));

    let mut rng = Rng(7);
    let points: Vec<Point> = (0..200).map(|_| Point::new(rng.next() * 1600.0, rng.next() * 1200.0)).collect();
    let hits = median_ms(3, || {
        for p in &points {
            drop(hit::hit_test(&doc, *p, hit::HitOptions::default()));
        }
    }) / points.len() as f64;
    rows.push(("hit test (per click)", hits, 2.0));

    let mut bytes = vec![];
    rows.push(("save .vectorcraft", median_ms(3, || bytes = vectorcraft_format::save_file(&doc)), 300.0));
    let mut failed = None;
    let open = median_ms(3, || {
        if let Err(e) = vectorcraft_format::load(&bytes) {
            failed = Some(format!("open .vectorcraft: {e}"));
        }
    });
    rows.push(("open .vectorcraft", open, 300.0));
    let svg_opts = vectorcraft_svg::ExportOptions::default();
    rows.push(("export SVG", median_ms(3, || drop(vectorcraft_svg::export(&doc, &svg_opts))), 500.0));

    // Pathfinder Unite on 1,000 overlapping paths.
    let unite = median_ms(3, || {
        let mut s = vectorcraft_engine::Session::new();
        let r = synthetic(1000).and_then(|d| {
            s.add_document(d, None);
            s.execute("select.all", &json!({})).and_then(|_| s.execute("object.pathfinder.unite", &json!({}))).map_err(|e| e.to_string())
        });
        if let Err(e) = r {
            failed = Some(format!("Pathfinder Unite: {e}"));
        }
    });
    rows.push(("Pathfinder Unite, 1,000 paths", unite, 150.0));

    // Path editing: one pointer move of a Direct Selection drag (the tool event and the preview it
    // applies), on a 5,000-anchor path among the synthetic paths.
    let (handle, segment) = edit_drags(n).unwrap_or_else(|e| {
        failed = Some(e);
        (0.0, 0.0)
    });
    rows.push(("handle drag, 5,000-anchor path (per move)", handle, 8.0));
    rows.push(("segment drag, 5,000-anchor path (per move)", segment, 8.0));
    if let Some(e) = failed {
        return Err(e);
    }
    if want_deep {
        deep(n)?;
    }

    let mut over = 0;
    outln!("  {:<42} {:>10} {:>10}", "", "measured", "budget");
    for (name, ms, budget) in &rows {
        let ok = ms <= budget;
        over += usize::from(!ok);
        outln!("{}", budget_row(name, *ms, *budget, ok));
    }
    match (over, noisy) {
        (0, _) => Ok(()),
        (_, true) => {
            outln!("{over} over budget, but the machine is busy: re-run when idle.");
            Ok(())
        }
        _ => Err(format!("{over} budget(s) exceeded")),
    }
}

// CoralCraft (P5 Task 1): the three representative project classes, built deterministically
// through real commands so every run replays the same document (defined and measured in
// docs/p5-inventory.md; synthetic only — no operator document is committed).

/// Runs one command, mapping errors to the harness's string errors.
fn ex(s: &mut vectorcraft_engine::Session, c: &str, p: serde_json::Value) -> Result<serde_json::Value, String> {
    s.execute(c, &p).map_err(|e| format!("{c}: {e}"))
}

/// A synthetic PNG: a rendered gradient page, so the classes need no external fixture.
fn png_bytes(size: u32) -> Vec<u8> {
    let mut doc = Document::new(f64::from(size), f64::from(size));
    let l = doc.layers.first().map(|l| l.id);
    for i in 0..8 {
        let x = f64::from(i) * f64::from(size) / 8.0;
        let node = Node::path(
            doc.alloc_id(),
            shapes::rectangle(Rect::new(x, 0.0, x + f64::from(size) / 8.0, f64::from(size))),
            Appearance::basic(Paint::solid(Color::rgb(f64::from(i) as f32 / 8.0, 0.4, 1.0 - f64::from(i) as f32 / 8.0)), Paint::None, 0.0),
        );
        doc.insert(l, usize::MAX, node).ok();
    }
    let mut r = Renderer::new();
    r.threads = 0;
    r.render(&doc, size, size, Affine::IDENTITY, &RenderOptions::default()).to_png().unwrap_or_default()
}

/// Class builders. Every position and colour comes from the index, never chance.
fn class_session(which: &str) -> Result<vectorcraft_engine::Session, String> {
    let mut s = vectorcraft_engine::Session::new();
    let colour = |i: usize| format!("#{:06x}", (i.wrapping_mul(2654435761) % 0x1000000));
    match which {
        // A many-object vector illustration: one page, 2,400 shapes, 40 guides, 5 symbols × 20
        // instances, 30 text objects, and one edited 5,000-anchor path.
        "illustration" => {
            ex(&mut s, "file.new", json!({"width": 420, "height": 300}))?;
            for i in 0..2400 {
                let (x, y) = ((i % 80) as f64 * 5.25, (i / 80) as f64 * 10.0);
                let id = match i % 3 {
                    0 => ex(&mut s, "shape.rectangle", json!({"x": x, "y": y, "width": 4.0, "height": 4.0}))?["id"].as_u64().unwrap_or_default(),
                    1 => ex(&mut s, "shape.ellipse", json!({"x": x, "y": y, "width": 4.0, "height": 4.0}))?["id"].as_u64().unwrap_or_default(),
                    _ => ex(&mut s, "shape.star", json!({"cx": x + 2.0, "cy": y + 2.0, "radius1": 2.4, "radius2": 1.2, "points": 5}))?["id"]
                        .as_u64()
                        .unwrap_or_default(),
                };
                ex(&mut s, "paint.setFill", json!({"ids": [id], "color": colour(i)}))?;
                if i % 5 == 0 {
                    ex(&mut s, "object.setProps", json!({"ids": [id], "opacity": 80}))?;
                }
            }
            for i in 0..40 {
                ex(&mut s, "guide.add", json!({"vertical": i % 2 == 0, "pos": (i as f64 + 1.0) * 10.0}))?;
            }
            for sym in 0..5 {
                let id = ex(&mut s, "shape.ellipse", json!({"x": 200.0 + sym as f64 * 8.0, "y": 20.0, "width": 12.0, "height": 12.0}))?["id"]
                    .as_u64()
                    .unwrap_or_default();
                ex(&mut s, "paint.setFill", json!({"ids": [id], "color": colour(sym + 900)}))?;
                ex(&mut s, "select.set", json!({"ids": [id]}))?;
                ex(&mut s, "symbol.new", json!({"name": format!("Mark {sym}")}))?;
                for at in 0..20 {
                    ex(&mut s, "symbol.place", json!({"name": format!("Mark {sym}"), "x": 200.0 + at as f64 * 3.0, "y": 260.0 + sym as f64 * 6.0}))?;
                }
            }
            for t in 0..30 {
                ex(&mut s, "text.create", json!({"x": 10.0 + t as f64 * 12.0, "y": 280.0 + (t % 3) as f64 * 6.0, "text": format!("Label {t}")}))?;
            }
        }
        // A multipage brochure: six mixed-size pages, a master layer with the page-number field,
        // threaded area text, two tables, four placed 512×512 bitmaps, and CDR residuals from one
        // export/import round trip.
        "brochure" => {
            ex(&mut s, "file.new", json!({"width": 420, "height": 300}))?;
            for (i, (w, h)) in [(300.0, 400.0), (420.0, 300.0), (300.0, 300.0)].iter().enumerate() {
                ex(&mut s, "artboard.new", json!({"width": w, "height": h}))?;
                let page = i + 1;
                for o in 0..12 {
                    let id = ex(
                        &mut s,
                        "shape.rectangle",
                        json!({"x": 10.0 + o as f64 * 22.0, "y": 10.0 + o as f64 * 16.0, "width": 18.0, "height": 12.0}),
                    )?["id"]
                        .as_u64()
                        .unwrap_or_default();
                    ex(&mut s, "paint.setFill", json!({"ids": [id], "color": colour(o + page * 40)}))?;
                }
                ex(&mut s, "text.create", json!({"x": 12.0, "y": 240.0, "text": format!("Page {page} body text with several words.")}))?;
            }
            ex(&mut s, "layer.new", json!({"name": "Master", "role": "masterAll", "top": false}))?;
            ex(&mut s, "text.create", json!({"x": 200.0, "y": 12.0, "text": "Brochure header"}))?;
            let png = vectorcraft_format::base64_encode(&png_bytes(512));
            for b in 0..4 {
                ex(
                    &mut s,
                    "file.place",
                    json!({"name": format!("photo-{b}.png"), "dataBase64": png, "link": false, "at": [30.0 + b as f64 * 90.0, 60.0]}),
                )?;
            }
            ex(
                &mut s,
                "layout.table",
                json!({"x": 40.0, "y": 150.0, "width": 200.0, "height": 60.0, "rows": 3, "columns": 4, "text": [["A", "1", "2", "3"], ["B", "4", "5", "6"], ["C", "7", "8", "9"]]}),
            )?;
            // One CDR round trip leaves the CoralCraft residual block in the reopened document.
            let cdr = ex(&mut s, "document.serialize", json!({"format": "cdr"}))?;
            ex(&mut s, "document.open", json!({"name": "brochure.cdr", "dataBase64": cdr["dataBase64"]}))?;
        }
        // A P3/P4 feature-dense poster: nested PowerClip, blend, mesh with point opacity,
        // two-colour pattern, texture, gradient with an opacity mask, live effects, a traced
        // bitmap, and the production settings (spot ink, overprint, separations print setup,
        // document bleed).
        "poster" => {
            ex(&mut s, "file.new", json!({"width": 420, "height": 300}))?;
            ex(&mut s, "file.documentColorMode", json!({"mode": "cmyk"}))?;
            let base =
                ex(&mut s, "shape.rectangle", json!({"x": 20.0, "y": 20.0, "width": 150.0, "height": 100.0}))?["id"].as_u64().unwrap_or_default();
            ex(&mut s, "paint.setFill", json!({"ids": [base], "color": {"c": 0, "m": 100, "y": 0, "k": 0}}))?;
            ex(&mut s, "swatch.new", json!({"name": "Coral Spot Orange", "color": {"c": 0, "m": 35, "y": 95, "k": 0}, "global": true}))?;
            ex(&mut s, "swatch.setSpot", json!({"name": "Coral Spot Orange"}))?;
            let spot =
                ex(&mut s, "shape.rectangle", json!({"x": 190.0, "y": 20.0, "width": 80.0, "height": 100.0}))?["id"].as_u64().unwrap_or_default();
            ex(&mut s, "paint.setFill", json!({"ids": [spot], "swatch": "Coral Spot Orange"}))?;
            let over =
                ex(&mut s, "shape.rectangle", json!({"x": 60.0, "y": 80.0, "width": 100.0, "height": 40.0}))?["id"].as_u64().unwrap_or_default();
            ex(&mut s, "paint.setFill", json!({"ids": [over], "color": {"c": 0, "m": 0, "y": 100, "k": 0}}))?;
            ex(&mut s, "object.setOverprint", json!({"ids": [over], "fill": true}))?;
            let inner =
                ex(&mut s, "shape.rectangle", json!({"x": 30.0, "y": 130.0, "width": 60.0, "height": 40.0}))?["id"].as_u64().unwrap_or_default();
            let inner_frame =
                ex(&mut s, "shape.rectangle", json!({"x": 25.0, "y": 125.0, "width": 70.0, "height": 50.0}))?["id"].as_u64().unwrap_or_default();
            ex(&mut s, "select.set", json!({"ids": [inner, inner_frame]}))?;
            let clip = ex(&mut s, "object.clippingMask.make", json!({}))?["id"].as_u64().unwrap_or_default();
            let outer_frame =
                ex(&mut s, "shape.rectangle", json!({"x": 15.0, "y": 115.0, "width": 95.0, "height": 70.0}))?["id"].as_u64().unwrap_or_default();
            ex(&mut s, "select.set", json!({"ids": [clip, outer_frame]}))?;
            ex(&mut s, "object.clippingMask.make", json!({}))?;
            let a = ex(&mut s, "shape.ellipse", json!({"x": 300.0, "y": 20.0, "width": 24.0, "height": 24.0}))?["id"].as_u64().unwrap_or_default();
            let b = ex(&mut s, "shape.ellipse", json!({"x": 370.0, "y": 90.0, "width": 40.0, "height": 40.0}))?["id"].as_u64().unwrap_or_default();
            ex(&mut s, "object.blend.make", json!({"ids": [a, b], "steps": 10}))?;
            let mesh =
                ex(&mut s, "shape.rectangle", json!({"x": 300.0, "y": 150.0, "width": 90.0, "height": 60.0}))?["id"].as_u64().unwrap_or_default();
            ex(&mut s, "object.mesh.create", json!({"id": mesh, "rows": 4, "cols": 4}))?;
            ex(&mut s, "object.mesh.setPointColor", json!({"id": mesh, "index": 5, "color": "#28aa5a", "opacity": 0.5}))?;
            ex(&mut s, "pattern.twoColor", json!({"name": "Coral Checks", "kind": "checkerboard"}))?;
            let patterned =
                ex(&mut s, "shape.rectangle", json!({"x": 20.0, "y": 200.0, "width": 80.0, "height": 60.0}))?["id"].as_u64().unwrap_or_default();
            ex(&mut s, "select.set", json!({"ids": [patterned]}))?;
            ex(&mut s, "paint.setPattern", json!({"pattern": "Coral Checks"}))?;
            ex(&mut s, "texture.new", json!({"name": "Coral Stipple", "kind": "stipple", "density": 0.5}))?;
            let textured =
                ex(&mut s, "shape.rectangle", json!({"x": 110.0, "y": 200.0, "width": 80.0, "height": 60.0}))?["id"].as_u64().unwrap_or_default();
            ex(&mut s, "select.set", json!({"ids": [textured]}))?;
            ex(&mut s, "paint.setTexture", json!({"texture": "Coral Stipple"}))?;
            let faded =
                ex(&mut s, "shape.ellipse", json!({"x": 200.0, "y": 200.0, "width": 70.0, "height": 60.0}))?["id"].as_u64().unwrap_or_default();
            ex(&mut s, "select.set", json!({"ids": [faded]}))?;
            ex(&mut s, "transparency.set", json!({"opacity": 50}))?;
            let shadowed =
                ex(&mut s, "shape.rectangle", json!({"x": 290.0, "y": 230.0, "width": 100.0, "height": 40.0}))?["id"].as_u64().unwrap_or_default();
            ex(&mut s, "effect.apply", json!({"id": shadowed, "effect": "stylize.dropShadow", "params": {"x": 6, "y": 5}}))?;
            ex(&mut s, "effect.apply", json!({"id": shadowed, "effect": "threeD.extrude", "params": {"depth": 12}}))?;
            let zig = ex(&mut s, "shape.ellipse", json!({"x": 30.0, "y": 30.0, "width": 60.0, "height": 40.0}))?["id"].as_u64().unwrap_or_default();
            ex(&mut s, "effect.apply", json!({"id": zig, "effect": "distort.zigZag", "params": {"size": 6, "ridges": 8}}))?;
            let png = vectorcraft_format::base64_encode(&png_bytes(96));
            let bitmap = ex(&mut s, "file.place", json!({"name": "trace-me.png", "dataBase64": png, "link": false, "at": [300.0, 130.0]}))?["ids"][0]
                .as_u64()
                .unwrap_or_default();
            ex(&mut s, "imageTrace.make", json!({"id": bitmap, "params": {"ignoreWhite": true}}))?;
            ex(&mut s, "text.create", json!({"x": 20.0, "y": 12.0, "text": "Feature poster"}))?;
            ex(&mut s, "document.setup", json!({"bleed": 9.0}))?;
            ex(&mut s, "print.setup", json!({"settings": {"output": {"mode": "separations"}, "marks": {"trim": true}}}))?;
        }
        other => return Err(format!("unknown class `{other}` (illustration | brochure | poster)")),
    }
    Ok(s)
}

/// Counts one class's document: objects, anchors, images, effects, texts, artboards, layers.
fn class_counts(doc: &Document) -> (usize, usize, usize, usize, usize, usize, usize) {
    let (mut objects, mut anchors, mut images, mut effects, mut texts) = (0, 0, 0, 0, 0);
    for layer in &doc.layers {
        layer.walk(&mut |node| {
            use vectorcraft_doc::NodeKind;
            match &node.kind {
                NodeKind::Layer { .. } => {}
                NodeKind::Text(_) => {
                    texts += 1;
                    objects += 1;
                }
                NodeKind::Image(im) => {
                    images += 1;
                    objects += 1;
                    let _ = im;
                }
                _ => objects += 1,
            }
            if let Some(path) = node.path_data() {
                anchors += path.subpaths.iter().map(|s| s.anchors.len()).sum::<usize>();
            }
            effects += node.appearance.effects.len();
        });
    }
    (objects, anchors, images, effects, texts, doc.artboards.len(), doc.layers.len())
}

/// Builds one class, prints its shape (counts, saved bytes, SHA-256), and measures the P5
/// scenario rows: open, idle render, hit test, selection edit/undo/redo, pointer drag, save,
/// reopen, and the SVG/PDF/CDR exports, with resident memory before and after.
fn class_report(which: &str) -> Result<(), String> {
    use sha2::{Digest, Sha256};
    let t = Instant::now();
    let s = class_session(which)?;
    let build_ms = t.elapsed().as_secs_f64() * 1000.0;
    let doc = s.doc().map_err(|e| e.to_string())?.doc.clone();
    let (objects, anchors, images, effects, texts, artboards, layers) = class_counts(&doc);
    let bytes = vectorcraft_format::save_file(&doc);
    let hash = Sha256::digest(&bytes);
    let hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();
    outln!("CoralCraft class `{which}` — built in {build_ms:.0} ms");
    outln!(
        "  {objects} objects ({texts} text, {images} images), {anchors} anchors, {effects} effect entries, {artboards} artboards, {layers} layers"
    );
    outln!("  saved .vectorcraft: {} bytes, sha256 {}", bytes.len(), &hex[..16]);

    let mut rows: Vec<(String, f64)> = vec![];
    rows.push(("open (load saved bytes)".into(), median_ms(3, || drop(vectorcraft_format::load(&bytes)))));
    {
        let opts = RenderOptions::default();
        let mut r = Renderer::new();
        let fit = Affine::scale(1.6);
        r.render(&doc, 2880, 1800, fit, &opts);
        rows.push(("idle render, 2880×1800 (warm)".into(), median_ms(5, || drop(r.render(&doc, 2880, 1800, fit, &opts)))));
    }
    {
        use vectorcraft_doc::hit::{self, HitOptions};
        let points: Vec<Point> = (0..200).map(|i| Point::new(f64::from(i % 50) * 8.4, f64::from(i / 50) * 4.0)).collect();
        let ms = median_ms(3, || {
            for p in &points {
                drop(hit::hit_test(&doc, *p, HitOptions::default()));
            }
        }) / points.len() as f64;
        rows.push(("hit test (per click)".into(), ms));
    }
    // Selection editing on the reopened document: move, undo, redo, and a pointer drag.
    {
        let mut s2 = vectorcraft_engine::Session::new();
        s2.add_document(vectorcraft_format::load(&bytes).map_err(|e| e.to_string())?, None);
        let err = |e: vectorcraft_engine::EngineError| e.to_string();
        s2.execute("select.all", &json!({})).map_err(err)?;
        rows.push(("select.all".into(), 0.0)); // replaced below with the timed value
        rows.pop();
        rows.push((
            "select.all".into(),
            median_ms(3, || {
                drop(s2.execute("select.all", &json!({})));
            }),
        ));
        rows.push(("move the selection".into(), median_ms(5, || drop(s2.execute("object.move", &json!({"dx": 1, "dy": 0}))))));
        rows.push(("undo the move".into(), median_ms(5, || drop(s2.execute("edit.undo", &json!({}))))));
        rows.push(("redo the move".into(), median_ms(5, || drop(s2.execute("edit.redo", &json!({}))))));
        use vectorcraft_engine::tools::{Mods, PointerEvent, PointerKind};
        let view = vectorcraft_engine::ViewInfo { zoom: 1.0, ..Default::default() };
        let first = s2.doc().map_err(err)?.selection.objects.first().copied();
        let at = first.and_then(|id| s2.doc().ok()?.doc.node(id)?.geometric_bounds()).map(|b| b.center());
        if let Some(down) = at {
            let ev = |kind, dx: f64| PointerEvent {
                kind,
                pos: down + vectorcraft_geom::Vec2::new(dx, 0.0),
                mods: Mods::default(),
                backquote: false,
                pressure: 1.0,
                pen: None,
            };
            s2.select_tool("selection", view).map_err(err)?;
            s2.pointer(&ev(PointerKind::Down, 0.0), view).map_err(err)?;
            let mut dx = 0.0;
            let drag = median_ms(9, || {
                dx += 2.0;
                let _ = s2.pointer(&ev(PointerKind::Drag, dx), view);
            });
            s2.pointer(&ev(PointerKind::Up, dx), view).map_err(err)?;
            rows.push(("pointer drag, per move".into(), drag));
        }
        rows.push(("save .vectorcraft".into(), median_ms(3, || drop(s2.execute("document.serialize", &json!({"format": "vectorcraft"}))))));
        for format in ["svg", "pdf", "cdr"] {
            rows.push((format!("export {format}"), median_ms(3, || drop(s2.execute("document.serialize", &json!({"format": format}))))));
        }
    }
    for (name, ms) in &rows {
        outln!("{}", tracked_row(name, *ms));
    }
    if let Some((m, peak)) = rss_mb() {
        outln!("{}", memory_row("now", m, peak));
    }
    Ok(())
}

// CoralCraft (P5 Task 2): a deterministic, sampleable benchmark for the measured hot path — the
// zoomed render of a 50,000-path document (the deep run's exceeded 16 ms budget; baseline
// 28.70 ms median). Runs the zoomed viewport for ~`ms` milliseconds and reports the median, so a
// profiler can be attached to exactly this loop. The output pixels are hashed: an optimization
// must leave them unchanged (the first frame's bytes, FNV-1a, printed every run).
fn bench_zoom(ms: u64) -> Result<(), String> {
    let n = 50_000usize;
    let doc = synthetic(n)?;
    let opts = RenderOptions::default();
    let mut r = Renderer::new();
    let fit = Affine::scale(1.6);
    r.render(&doc, 2880, 1800, fit, &opts);
    let zoom = Affine::scale(6.4) * Affine::translate((-600.0, -400.0));
    let first = r.render(&doc, 2880, 1800, zoom, &opts);
    let mut fnv: u64 = 0xcbf29ce484222325;
    for b in &first.pixels {
        fnv = (fnv ^ u64::from(*b)).wrapping_mul(0x100000001b3);
    }
    let t = Instant::now();
    let mut runs = 0u64;
    while (t.elapsed().as_millis() as u64) < ms {
        drop(r.render(&doc, 2880, 1800, zoom, &opts));
        runs += 1;
    }
    let per = t.elapsed().as_secs_f64() / runs as f64 * 1000.0;
    outln!("zoom render, 50,000 paths, 2880×1800: {per:.2} ms median over {runs} runs ({ms} ms loop); pixels fnv1a {fnv:016x}");
    Ok(())
}

// CoralCraft (P5 Task 2): the same shape of benchmark for the other measured hot path — saving
// the image-heavy brochure class natively (baseline ~900 ms median; the deep run's exceeded open
// budget is the same format layer). The saved bytes' length is printed every run: an
// optimization must not change the file.
fn bench_save(ms: u64) -> Result<(), String> {
    let s = class_session("brochure")?;
    let doc = s.doc().map_err(|e| e.to_string())?.doc.clone();
    let bytes = vectorcraft_format::save_file(&doc);
    outln!("brochure saved bytes: {}", bytes.len());
    if let Some(ext) = doc.unknown.get("coralcraft") {
        outln!("  coralcraft block: {} chars of JSON Value", serde_json::to_string(ext).map(|s| s.len()).unwrap_or_default());
        if let Some(obj) = ext.as_object() {
            let mut keys: Vec<(&String, usize)> =
                obj.iter().map(|(k, v)| (k, serde_json::to_string(v).map(|s| s.len()).unwrap_or_default())).collect();
            keys.sort_by_key(|&(_, len)| std::cmp::Reverse(len));
            for (k, len) in keys.iter().take(6) {
                outln!("    {k}: {len} chars");
            }
        }
    }
    let t = Instant::now();
    let mut runs = 0u64;
    while (t.elapsed().as_millis() as u64) < ms {
        drop(vectorcraft_format::save_file(&doc));
        runs += 1;
    }
    let per = t.elapsed().as_secs_f64() / runs as f64 * 1000.0;
    outln!("save .vectorcraft (format layer), brochure class: {per:.2} ms median over {runs} runs ({ms} ms loop)");
    // The engine's document.serialize over the same document (what the app saves with).
    let mut s2 = vectorcraft_engine::Session::new();
    s2.add_document((*doc).clone(), None);
    let t = Instant::now();
    let mut runs = 0u64;
    while (t.elapsed().as_millis() as u64) < ms {
        drop(s2.execute("document.serialize", &json!({"format": "vectorcraft"})));
        runs += 1;
    }
    let per = t.elapsed().as_secs_f64() / runs as f64 * 1000.0;
    outln!("document.serialize vectorcraft (engine), brochure class: {per:.2} ms median over {runs} runs ({ms} ms loop)");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn busy_share_leaves_out_idle_time() {
        // In one second four processors log 40,000,000 ticks of 100 ns of kernel and user time,
        // 10,000,000 of them idle.
        assert_eq!(busy_share([7, 100, 50], [10_000_007, 25_000_100, 15_000_050]), Some(0.75));
        // There is no share when no time passed, a time went back, or the idle time exceeds the
        // kernel and user time.
        assert_eq!(busy_share([7, 100, 50], [7, 100, 50]), None);
        assert_eq!(busy_share([7, 100, 50], [8, 99, 60]), None);
        assert_eq!(busy_share([0, 0, 0], [30, 10, 10]), None);
    }

    /// On Windows `busy_cores` gives a number between 0 and the cores.
    #[cfg(windows)]
    #[test]
    fn windows_reads_how_many_cores_are_busy() {
        let cores = std::thread::available_parallelism().map_or(1, |c| c.get());
        let busy = busy_cores(cores);
        assert!(busy.is_some_and(|b| (0.0..=cores as f64).contains(&b)), "{busy:?} of {cores}");
    }

    /// On Windows `rss_mb` gives this process's working set: more than 0 MB and less than
    /// 10,000 MB.
    #[cfg(windows)]
    #[test]
    fn windows_reads_the_working_set() {
        let mb = rss_mb().map(|(mb, _)| mb);
        assert!(mb.is_some_and(|mb| mb > 0.0 && mb < 10_000.0), "{mb:?}");
    }

    /// On Windows and Linux `rss_mb` also gives the peak: after this test writes 64 MB and frees
    /// it, the peak is no more than 16 MB below the resident memory read while it held the 64 MB.
    #[cfg(any(windows, target_os = "linux"))]
    #[test]
    fn freed_memory_stays_in_the_peak() {
        let buffer = std::hint::black_box(vec![1u8; 64 << 20]);
        let held = rss_mb();
        drop(buffer);
        let (Some((held, _)), Some((now, peak))) = (held, rss_mb()) else {
            eprintln!("no memory reading (Linux without `ps`): nothing to check");
            return;
        };
        assert!(peak.is_some_and(|peak| peak > held - 16.0), "holding 64 MB: {held:.1} MB, after freeing it: {now:.1} MB, peak {peak:?}");
    }

    /// `vm_hwm_mb` reads the peak resident set size from the `VmHWM` line of a status text that
    /// also has a `VmRSS` line.
    #[test]
    fn vm_hwm_is_the_peak_resident_set_size() {
        let status = "Name:\tcoralcraft-cli\nVmPeak:\t 2097152 kB\nVmSize:\t 1572864 kB\nVmHWM:\t  819200 kB\nVmRSS:\t  204800 kB\n";
        assert_eq!(vm_hwm_mb(status), Some(800.0));
        // A text without a `VmHWM` line gives no peak.
        assert_eq!(vm_hwm_mb("Name:\tkthreadd\nState:\tS (sleeping)\n"), None);
    }

    /// A memory row ends with the peak when there is one; without a peak it ends with the value
    /// in MB. With the longest name on Windows, the value and the peak line up with the other rows.
    #[test]
    fn memory_rows_end_with_the_peak_when_there_is_one() {
        let row = memory_row("working set start", 346.4, Some(470.2));
        assert!(row.starts_with("  working set start ") && row.ends_with(" 346 MB  peak      470 MB"), "{row}");
        let longest = memory_row("working set after rendering the photos with 3 renderers", 2514.0, Some(2541.0));
        assert_eq!(longest.chars().count(), row.chars().count(), "{longest}\n{row}");
        let row = memory_row("memory start", 346.4, None);
        assert!(row.starts_with("  memory start ") && row.ends_with(" 346 MB") && !row.contains("peak"), "{row}");
    }

    /// With the longest names, the values of a tracked row and of the budget table's rows line
    /// up with the rows that have short names.
    #[test]
    fn the_longest_tracked_and_budget_names_fit_their_columns() {
        let longest = tracked_row("render 4 embedded 24 MP photos with 3 renderers (cold)", 774.61);
        let short = tracked_row("Pathfinder crop, 300 paths", 30.94);
        assert_eq!(longest.chars().count(), short.chars().count(), "{longest}\n{short}");
        for name in ["handle drag, 5,000-anchor path (per move)", "segment drag, 5,000-anchor path (per move)"] {
            let (longest, short) = (budget_row(name, 0.12, 8.0, true), budget_row("export SVG", 7.81, 500.0, true));
            assert_eq!(longest.chars().count(), short.chars().count(), "{longest}\n{short}");
        }
    }
}
