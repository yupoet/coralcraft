//! cargo xtask：CoralCraft 质量门与工程工具（对齐 vectorcraft xtask 约定）。
//!
//! - `ci`        = fmt --check + clippy -D warnings + test + cleanroom + assets + layers
//! - `cleanroom` = 扫描 Corel 版权样串与 `.cdr`/`.cmx` fixture（净室铁律的机器门）
//! - `assets`    = 校验 assets/ 内文件都在 ASSETS.md 登记（当前无资产目录，恒过）
//! - `layers`    = 校验 workspace crate 分层依赖规则（L0→L7，L6 以下禁止依赖 UI 库）
//! - `stats`     = 统计各 crate 的 Rust 文件数、代码行数与 `#[test]` 数量
//!
//! 有意只用标准库：质量门自身不引第三方依赖（对齐家族惯例）。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const USAGE: &str = "\
usage: cargo xtask <command>

commands:
  ci              fmt --check, clippy -D warnings, test, cleanroom, assets, layers
  cleanroom       scan for prohibited copyright strings and binary .cdr/.cmx fixtures
  assets          check that every file in assets/ is attributed in ASSETS.md
  layers          enforce workspace crate dependency layering (L0..L7)
  stats           count Rust files, lines, and #[test] attributes per crate
";

fn main() -> ExitCode {
    let task = std::env::args().nth(1);
    let res = match task.as_deref() {
        Some("ci") => ci(),
        Some("cleanroom") => cleanroom(&repo_root()),
        Some("assets") => assets(&repo_root()),
        Some("layers") => layers(&repo_root()),
        Some("stats") => stats(&repo_root()),
        Some("-h" | "--help" | "help") | None => {
            print!("{USAGE}");
            Ok(())
        }
        Some(other) => Err(format!("未知子命令 `{other}`\n\n{USAGE}")),
    };
    match res {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(label: &str, cmd: &mut Command) -> Result<(), String> {
    print!("== {label} … ");
    let status = cmd.status().map_err(|e| format!("{label} 启动失败: {e}"))?;
    if !status.success() {
        return Err(format!("{label} 失败（{status}）"));
    }
    println!("ok");
    Ok(())
}

fn ci() -> Result<(), String> {
    let root = repo_root();
    run("fmt --check", Command::new("cargo").current_dir(&root).args(["fmt", "--all", "--", "--check"]))?;
    run("clippy", Command::new("cargo").current_dir(&root).args(["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"]))?;
    run("test", Command::new("cargo").current_dir(&root).args(["test", "--workspace"]))?;
    cleanroom(&root)?;
    assets(&root)?;
    layers(&root)?;
    Ok(())
}

/// 净室扫描：Corel 版权样串 + Corel 软件生成的 fixture（.cdr/.cmx 二进制）。
/// 只扫本仓源码树；refs/（第三方参考，git-ignored）、target/ 与 xtask 自身
/// （扫描器源码天然含有样串定义）不在辖内。
fn cleanroom(root: &Path) -> Result<(), String> {
    let mut offenders: Vec<String> = Vec::new();

    let mut stack = vec![root.join("crates"), root.join("apps"), root.join("docs")];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if name == "target" || name == "refs" || name == ".git" || name == "plan" {
                continue;
            }
            let is_real_dir = entry.file_type().is_ok_and(|ft| ft.is_dir() && !ft.is_symlink());
            if is_real_dir {
                stack.push(path);
                continue;
            }
            if name.ends_with(".cdr") || name.ends_with(".cmx") {
                offenders.push(format!("fixture 疑似 Corel 文件: {}", path.display()));
                continue;
            }
            if let Ok(text) = fs::read_to_string(&path) {
                for needle in ["Copyright (c) Corel", "Corel Corporation", "© Corel"] {
                    if text.contains(needle) {
                        offenders.push(format!("Corel 版权样串: {} （命中 {needle:?}）", path.display()));
                    }
                }
            }
        }
    }

    if offenders.is_empty() {
        println!("cleanroom: 通过（无 Corel 版权样串、无 .cdr/.cmx fixture）");
        Ok(())
    } else {
        for o in &offenders {
            eprintln!("cleanroom 违规: {o}");
        }
        Err(format!("cleanroom 检出 {} 处违规", offenders.len()))
    }
}

fn assets(root: &Path) -> Result<(), String> {
    let dir = root.join("assets");
    if !dir.exists() {
        println!("assets: 无 assets/ 目录，通过（首个资产落地时建目录并逐行登记 ASSETS.md）");
        return Ok(());
    }
    let manifest = fs::read_to_string(root.join("ASSETS.md")).unwrap_or_default();
    let mut missing = Vec::new();
    collect_files(&dir, &mut |p| {
        let rel = p.strip_prefix(root).unwrap_or(p.as_path());
        let rel = rel.to_string_lossy();
        if !manifest.contains(rel.as_ref()) {
            missing.push(rel.into_owned());
        }
    });
    if missing.is_empty() {
        println!("assets: 通过");
        Ok(())
    } else {
        for m in &missing {
            eprintln!("assets 未登记: {m}");
        }
        Err(format!("assets 检出 {} 个未登记文件", missing.len()))
    }
}

/// 分层表（对齐 vectorcraft L0–L7 架构）。
const LAYER_TABLE: &[(&str, u8)] = &[
    ("coralcraft-geom", 0),
    ("coralcraft-color", 0),
    ("coralcraft-doc", 1),
    ("coralcraft-cdr", 3),
    ("coralcraft-render", 3),
    ("coralcraft-tools", 4),
    ("coralcraft-engine", 5),
    ("coralcraft-ui-egui", 6),
    ("coralcraft-mcp", 6),
    ("coralcraft", 7),
    ("coralcraft-cli", 7),
    ("xtask", 7),
];

const UI_CRATES: &[&str] = &["egui", "eframe", "winit", "egui_kittest", "craft-ui", "rfd"];
const UI_MIN_LAYER: u8 = 6;

fn layer_of(pkg: &str) -> Option<u8> {
    LAYER_TABLE.iter().find(|(n, _)| *n == pkg).map(|(_, l)| *l)
}

fn layers(root: &Path) -> Result<(), String> {
    let manifests = workspace_manifests(root);
    let mut violations = Vec::new();
    println!("{:<24} {:<8} deps", "crate", "layer");
    println!("{}", "-".repeat(60));
    for (pkg, layer, deps) in &manifests {
        let layer_s = layer.map_or_else(|| "?".to_string(), |l| format!("L{l}"));
        println!("{pkg:<24} {layer_s:<8} {}", deps.join(", "));
        let Some(from_l) = *layer else {
            violations.push(format!("{pkg}: 未在 LAYER_TABLE 登记"));
            continue;
        };
        for dep in deps {
            if from_l < UI_MIN_LAYER && UI_CRATES.iter().any(|u| dep == u || dep.starts_with("egui_")) {
                violations.push(format!("{pkg} (L{from_l}) 禁止依赖 UI 库 `{dep}`（仅 L{UI_MIN_LAYER}+ 允许）"));
            }
            if let Some(to_l) = layer_of(dep)
                && from_l < 7
                && to_l >= from_l
            {
                violations.push(format!("{pkg} (L{from_l}) 不可依赖同层或高层 crate `{dep}` (L{to_l})"));
            }
        }
    }
    if violations.is_empty() {
        println!("\nlayers: 通过（{} 个 crate，无分层违规）", manifests.len());
        Ok(())
    } else {
        for v in &violations {
            eprintln!("layers 违规: {v}");
        }
        Err(format!("layers 检出 {} 处违规", violations.len()))
    }
}

fn workspace_manifests(root: &Path) -> Vec<(String, Option<u8>, Vec<String>)> {
    let mut dirs = Vec::new();
    for group in ["crates", "apps"] {
        if let Ok(rd) = fs::read_dir(root.join(group)) {
            for e in rd.flatten() {
                if e.path().is_dir() {
                    dirs.push(e.path());
                }
            }
        }
    }
    dirs.push(root.join("xtask"));
    let mut out = Vec::new();
    for d in dirs {
        let toml_path = d.join("Cargo.toml");
        let Ok(text) = fs::read_to_string(&toml_path) else {
            continue;
        };
        let Some(name) = parse_package_name(&text) else {
            continue;
        };
        let deps = parse_dep_names(&text);
        out.push((name.clone(), layer_of(&name), deps));
    }
    out.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
    out
}

fn parse_package_name(toml_text: &str) -> Option<String> {
    let mut in_pkg = false;
    for line in toml_text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_pkg = t == "[package]";
            continue;
        }
        if in_pkg
            && let Some(rest) = t.strip_prefix("name")
            && let Some((_, val)) = rest.split_once('=')
        {
            return Some(val.trim().trim_matches('"').to_owned());
        }
    }
    None
}

fn parse_dep_names(toml_text: &str) -> Vec<String> {
    let mut in_deps = false;
    let mut deps = Vec::new();
    for line in toml_text.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        if t.starts_with('[') {
            in_deps = t == "[dependencies]";
            continue;
        }
        if in_deps && let Some((k, _)) = t.split_once('=') {
            deps.push(k.trim().to_owned());
        }
    }
    deps
}

#[derive(Default, Clone, Copy)]
struct Counts {
    files: usize,
    lines: usize,
    tests: usize,
}

fn walk_rs(dir: &Path, c: &mut Counts) {
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        let is_real_dir = e.file_type().is_ok_and(|ft| ft.is_dir() && !ft.is_symlink());
        if is_real_dir {
            if name != "target" && name != "fuzz" && !name.starts_with('.') {
                walk_rs(&p, c);
            }
        } else if name.ends_with(".rs")
            && let Ok(s) = fs::read_to_string(&p)
        {
            c.files += 1;
            c.lines += s.lines().count();
            c.tests += count_tests(&s);
        }
    }
}

fn count_tests(src: &str) -> usize {
    src.lines().map(str::trim_start).filter(|l| !l.starts_with("//")).map(|l| l.matches("#[test]").count()).sum()
}

fn stats(root: &Path) -> Result<(), String> {
    let mut dirs = Vec::new();
    for group in ["crates", "apps"] {
        if let Ok(rd) = fs::read_dir(root.join(group)) {
            for e in rd.flatten() {
                if e.path().is_dir() {
                    dirs.push(e.path());
                }
            }
        }
    }
    dirs.push(root.join("xtask"));

    let mut rows = Vec::new();
    for d in dirs {
        let Ok(text) = fs::read_to_string(d.join("Cargo.toml")) else {
            continue;
        };
        let Some(name) = parse_package_name(&text) else {
            continue;
        };
        let mut c = Counts::default();
        walk_rs(&d, &mut c);
        rows.push((name, c));
    }
    rows.sort_by(|a, b| a.0.cmp(&b.0));

    println!("{:<24} {:>6} {:>8} {:>8}", "crate", "files", "lines", "#[test]");
    println!("{}", "-".repeat(50));
    let mut total = Counts::default();
    for (name, c) in &rows {
        println!("{name:<24} {:>6} {:>8} {:>8}", c.files, c.lines, c.tests);
        total.files += c.files;
        total.lines += c.lines;
        total.tests += c.tests;
    }
    println!("{}", "-".repeat(50));
    println!("{:<24} {:>6} {:>8} {:>8}", "TOTAL", total.files, total.lines, total.tests);
    Ok(())
}

fn collect_files(dir: &Path, f: &mut impl FnMut(&PathBuf)) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let is_real_dir = entry.file_type().is_ok_and(|ft| ft.is_dir() && !ft.is_symlink());
        if is_real_dir {
            collect_files(&path, f);
        } else if entry.file_type().is_ok_and(|ft| ft.is_file()) {
            f(&path);
        }
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap_or_else(|| Path::new(".")).to_path_buf()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TempRoot(PathBuf);

    impl TempRoot {
        fn new(tag: &str) -> Self {
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!("coralcraft_xtask_{tag}_{}_{n}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).expect("create temp root");
            Self(dir)
        }
    }

    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn repo_root_exists() {
        assert!(repo_root().join("Cargo.toml").exists());
    }

    #[test]
    fn counts_test_attributes_ignoring_comments() {
        let src = "#[test]\nfn a() {}\n// #[test]\n  #[test]\nfn b() {}\n";
        assert_eq!(count_tests(src), 2);
    }

    #[test]
    fn workspace_layers_pass() {
        assert!(layers(&repo_root()).is_ok());
    }

    #[test]
    fn cleanroom_passes_on_clean_tree_and_fails_on_forbidden_string_or_cdr_fixture() {
        let tmp = TempRoot::new("cleanroom");
        let crates_dir = tmp.0.join("crates/foo/src");
        let docs_dir = tmp.0.join("docs");
        fs::create_dir_all(&crates_dir).expect("mkdir crates");
        fs::create_dir_all(&docs_dir).expect("mkdir docs");
        fs::write(crates_dir.join("lib.rs"), "// clean rust file\n").expect("write clean file");

        assert!(cleanroom(&tmp.0).is_ok());

        // Construct the forbidden string at runtime so source remains clean.
        let forbidden = format!("{} (c) {}", "Copyright", "Corel");
        fs::write(docs_dir.join("bad.md"), format!("bad line: {forbidden}\n")).expect("write bad doc");
        let err = cleanroom(&tmp.0).expect_err("must reject forbidden string");
        assert!(err.contains("1 处违规"));

        // Remove bad doc, add a .cdr fixture inside crates/
        fs::remove_file(docs_dir.join("bad.md")).expect("rm bad doc");
        fs::write(crates_dir.join("sample.cdr"), b"RIFF\x04\x00\x00\x00CDRD").expect("write .cdr fixture");
        let err2 = cleanroom(&tmp.0).expect_err("must reject .cdr fixture");
        assert!(err2.contains("1 处违规"));
    }

    #[test]
    fn assets_gate_catches_unregistered_file_and_passes_when_registered() {
        let tmp = TempRoot::new("assets");
        // No assets/ dir -> passes
        assert!(assets(&tmp.0).is_ok());

        let icons_dir = tmp.0.join("assets/icons");
        fs::create_dir_all(&icons_dir).expect("mkdir assets/icons");
        fs::write(icons_dir.join("pen.svg"), "<svg/>").expect("write pen.svg");
        fs::write(tmp.0.join("ASSETS.md"), "# Assets\n").expect("write empty ASSETS.md");

        let err = assets(&tmp.0).expect_err("unregistered asset must fail");
        assert!(err.contains("1 个未登记文件"));

        fs::write(tmp.0.join("ASSETS.md"), "# Assets\n\n| assets/icons/pen.svg | Author | URL | MIT | Icon |\n").expect("register asset");
        assert!(assets(&tmp.0).is_ok());
    }
}
