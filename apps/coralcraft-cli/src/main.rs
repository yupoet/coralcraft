//! CoralCraft CLI。M0 骨架：仅 `probe` 占位与 `--version`。
//! M1 Task 11 接线真实探测（见计划文档）。

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "coralcraft-cli", version, about = "CoralCraft 命令行工具")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// 识别 CDR/CMX 文件的容器与版本（M1 交付）
    Probe {
        /// 待识别的文件路径
        file: String,
        /// 以 JSON 输出
        #[arg(long)]
        json: bool,
    },
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Command::Probe { .. } => {
            // M1 Task 11 接线 coralcraft_cdr::probe；当前为骨架占位。
            eprintln!("probe 将在 M1 Task 11 接线（见 docs/superpowers/plans）");
            std::process::exit(2);
        }
    }
}
