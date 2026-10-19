//! cargo xtask：CoralCraft 质量门。
//!
//! - `ci`        = fmt --check + clippy -D warnings + test --workspace
//! - `cleanroom` = 扫描 Corel 版权样串与 `.cdr` fixture（净室铁律的机器门）
//! - `assets`    = 校验 assets/ 内文件都在 ASSETS.md 登记（当前无资产目录，恒过）
//!
//! 有意只用标准库：质量门自身不引第三方依赖（对齐家族惯例）。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let task = std::env::args().nth(1).unwrap_or_default();
    match task.as_str() {
        "ci" => ci(),
        "cleanroom" => cleanroom(),
        "assets" => assets(),
        other => {
            eprintln!("用法: cargo xtask <ci|cleanroom|assets>，收到: {other:?}");
            std::process::exit(2);
        }
    }
}

fn run(label: &str, cmd: &mut Command) {
    print!("== {label} … ");
    let status = cmd.status().unwrap_or_else(|e| {
        eprintln!("启动失败: {e}");
        std::process::exit(1);
    });
    if !status.success() {
        eprintln!("{label} 失败（{status}）");
        std::process::exit(1);
    }
    println!("ok");
}

fn ci() {
    run(
        "fmt --check",
        Command::new("cargo").args(["fmt", "--all", "--", "--check"]),
    );
    run(
        "clippy",
        Command::new("cargo").args([
            "clippy",
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ]),
    );
    run("test", Command::new("cargo").args(["test", "--workspace"]));
}

/// 净室扫描：Corel 版权样串 + Corel 软件生成的 fixture（.cdr 二进制）。
/// 只扫本仓源码树；refs/（第三方参考，git-ignored）、target/ 与 xtask 自身
/// （扫描器源码天然含有样串定义）不在辖内。
fn cleanroom() {
    let root = repo_root();
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
            if path.is_dir() {
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
                        offenders.push(format!(
                            "Corel 版权样串: {} （命中 {needle:?}）",
                            path.display()
                        ));
                    }
                }
            }
        }
    }

    if offenders.is_empty() {
        println!("cleanroom: 通过（无 Corel 版权样串、无 .cdr/.cmx fixture）");
    } else {
        for o in &offenders {
            eprintln!("cleanroom 违规: {o}");
        }
        std::process::exit(1);
    }
}

fn assets() {
    let root = repo_root();
    let dir = root.join("assets");
    if !dir.exists() {
        println!("assets: 无 assets/ 目录，通过（首个资产落地时建目录并逐行登记 ASSETS.md）");
        return;
    }
    let manifest = fs::read_to_string(root.join("ASSETS.md")).unwrap_or_default();
    let mut missing = Vec::new();
    collect_files(&dir, &mut |p| {
        let rel = p.strip_prefix(&root).unwrap_or(p.as_path());
        let rel = rel.to_string_lossy();
        if !manifest.contains(rel.as_ref()) {
            missing.push(rel.into_owned());
        }
    });
    if missing.is_empty() {
        println!("assets: 通过");
    } else {
        for m in missing {
            eprintln!("assets 未登记: {m}");
        }
        std::process::exit(1);
    }
}

fn collect_files(dir: &Path, f: &mut impl FnMut(&PathBuf)) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, f);
        } else {
            f(&path);
        }
    }
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask 必在仓库根的子目录")
        .to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_root_exists() {
        assert!(repo_root().join("Cargo.toml").exists());
    }
}
