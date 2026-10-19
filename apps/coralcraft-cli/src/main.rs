//! CoralCraft CLI（`coralcraft-cli`）。
//!
//! 退出码约定：
//! - `0`：成功
//! - `2`：命令行参数错误（`clap` 默认）
//! - `3`：非 CDR/CMX 文件（`NotCdr` / `TooShort`）
//! - `4`：文件读取 I/O 错误
//! - `5`：容器结构损坏、截断或超出安全解析上限（`Truncated` / `Invalid` / `LimitExceeded`）

use std::fs;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use coralcraft_cdr::{CdrContainer, ProbeError, ProbeReport};

#[derive(Parser)]
#[command(name = "coralcraft-cli", version, about = "CoralCraft 命令行工具")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// 识别 CDR/CMX 文件的容器、版本与顶层 chunk 清单
    Probe {
        /// 待识别的文件路径
        file: String,
        /// 以结构化 JSON 输出（含 schema_version = 1）
        #[arg(long)]
        json: bool,
    },
}

fn print_human_report(file: &str, report: &ProbeReport) {
    println!("file: {file} ({} bytes)", report.file_size);
    match &report.container {
        CdrContainer::Riff { kind, endian, form, version, .. } => {
            println!(
                "container: RIFF ({endian:?}), kind={kind:?}, form={form}, effective_version={:?} (form={:?}, vrsn={:?})",
                version.effective_version, version.form_version, version.vrsn_chunk
            );
        }
        CdrContainer::Zip { primary_entry, role, has_data_file_list, entry_count, inner_version } => {
            println!(
                "container: ZIP (role={role:?}, primary={primary_entry}, entries={entry_count}, data_file_list={has_data_file_list}), inner_version={:?}",
                inner_version.and_then(|v| v.effective_version)
            );
        }
        CdrContainer::LegacyWl => {
            println!("container: Legacy WL (pre-1992 CorelDRAW v1/v2)");
        }
        _ => {
            println!("container: {:?}", report.container);
        }
    }

    println!("chunks: {}", report.chunks.len());
    for chunk in &report.chunks {
        let indent = "  ".repeat(chunk.depth + 1);
        if let Some(lt) = &chunk.list_type {
            println!("{indent}{} ({lt}) offset={} size={} opaque={}", chunk.fourcc, chunk.offset, chunk.size, chunk.opaque_payload);
        } else {
            println!("{indent}{} offset={} size={}", chunk.fourcc, chunk.offset, chunk.size);
        }
    }

    if !report.warnings.is_empty() {
        println!("warnings ({}):", report.warnings.len());
        for w in &report.warnings {
            println!("  - {w:?}");
        }
    }
}

/// CLI 单次读入内存探测的文件大小上限（512 MiB），防止超大敌意文件引发 OOM。
const MAX_PROBE_FILE_BYTES: u64 = 512 * 1024 * 1024;

fn run_probe(file: &str, json: bool) -> ExitCode {
    let meta = match fs::metadata(file) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("coralcraft-cli probe: 无法读取文件元数据 '{file}': {e}");
            return ExitCode::from(4);
        }
    };
    if meta.len() > MAX_PROBE_FILE_BYTES {
        eprintln!("coralcraft-cli probe: 文件 '{file}' 大小 ({} bytes) 超出安全探测上限 ({} bytes)", meta.len(), MAX_PROBE_FILE_BYTES);
        return ExitCode::from(4);
    }

    let bytes = match fs::read(file) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("coralcraft-cli probe: 无法读取文件 '{file}': {e}");
            return ExitCode::from(4);
        }
    };

    match coralcraft_cdr::probe(&bytes) {
        Ok(report) => {
            if json {
                match serde_json::to_string_pretty(&report) {
                    Ok(s) => println!("{s}"),
                    Err(e) => {
                        eprintln!("coralcraft-cli probe: JSON 序列化失败: {e}");
                        return ExitCode::from(5);
                    }
                }
            } else {
                print_human_report(file, &report);
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            let code = match &err {
                ProbeError::NotCdr { .. } | ProbeError::TooShort { .. } => 3,
                ProbeError::Truncated { .. } | ProbeError::Invalid { .. } | ProbeError::LimitExceeded { .. } => 5,
                _ => 5,
            };
            eprintln!("coralcraft-cli probe: not a CDR/CMX or invalid container ({file}): {err}");
            ExitCode::from(code)
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Probe { file, json } => run_probe(&file, json),
    }
}
