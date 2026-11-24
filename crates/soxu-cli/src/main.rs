//! Soxu 命令行控制与环境诊断工具。

use clap::{Parser, Subcommand};
use soxu_magpie::{EnvironmentMode, detect_mode_from_config_dir};
use soxu_protocol::PROTOCOL_VERSION;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "soxu", version, about = "Soxu 多 Agent 协同工作台命令行工具")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// 查看 CLI 与协议版本信息
    Version,
    /// 诊断本机 Magpie 接管状态与原生 Agent CLI 环境
    Doctor,
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Version => {
            println!(
                "soxu v{} (protocol v{})",
                env!("CARGO_PKG_VERSION"),
                PROTOCOL_VERSION
            );
        }
        Commands::Doctor => {
            let mode = std::env::var_os("USERPROFILE")
                .or_else(|| std::env::var_os("HOME"))
                .map(|h| PathBuf::from(h).join(".config").join("magpie"))
                .map_or(EnvironmentMode::StandaloneNative, |dir| {
                    detect_mode_from_config_dir(&dir)
                });
            println!("环境发现模式: {mode:?}");
        }
    }
}
