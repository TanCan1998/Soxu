//! Soxu 命令行控制与环境诊断工具。

mod client;

use anyhow::{Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use client::DaemonHttpClient;
use soxu_magpie::{EnvironmentMode, detect_mode_from_config_dir};
use soxu_protocol::{
    HealthSnapshot, PROTOCOL_VERSION, PairingTransportMode, RpcCommand, RpcRequest, RpcResponse,
    RpcResultData, StateSnapshot,
};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "soxu", version, about = "Soxu 多 Agent 协同工作台命令行工具")]
struct Cli {
    /// 指定守护进程监听地址（默认读取 `SOXU_LISTEN` 或 `127.0.0.1:7698`）
    #[arg(long, global = true)]
    addr: Option<String>,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// 查看 CLI 与协议版本信息
    Version,
    /// 诊断本机 Magpie 接管状态、原生环境与守护进程连通性
    Doctor {
        #[arg(long)]
        json: bool,
    },
    /// 查看当前守护进程健康态、工作区与会话快照
    Status {
        #[arg(long)]
        json: bool,
    },
    /// 无项目快速对话（Quick Chat）管理
    Chat {
        #[command(subcommand)]
        subcommand: ChatCommands,
    },
    /// 移动遥控设备配对码生成与设备管理
    Pair {
        #[command(subcommand)]
        subcommand: PairCommands,
    },
    /// 查询守护进程增量事件流
    Events {
        #[arg(long, default_value_t = 0)]
        after_seq: u64,
        #[arg(long, default_value_t = 50)]
        limit: u32,
    },
}

#[derive(Debug, Subcommand)]
enum ChatCommands {
    /// 新建无项目快速对话
    New {
        #[arg(long, default_value = "codex")]
        agent: String,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        title: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum TransportArg {
    LanDirect,
    EncryptedRelay,
}

impl From<TransportArg> for PairingTransportMode {
    fn from(value: TransportArg) -> Self {
        match value {
            TransportArg::LanDirect => Self::LanDirect,
            TransportArg::EncryptedRelay => Self::EncryptedRelay,
        }
    }
}

#[derive(Debug, Subcommand)]
enum PairCommands {
    /// 生成一次性扫码链接与 6 位数字配对码
    Create {
        #[arg(long, value_enum, default_value_t = TransportArg::LanDirect)]
        transport: TransportArg,
    },
    /// 列出已配对移动设备
    Devices,
    /// 吊销指定设备访问令牌
    Revoke { device_id: String },
}

fn resolve_daemon_addr(explicit: Option<&str>) -> String {
    if let Some(addr) = explicit
        && !addr.trim().is_empty()
    {
        return addr.trim().to_owned();
    }
    std::env::var("SOXU_LISTEN")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "127.0.0.1:7698".to_owned())
}

fn detect_magpie_mode() -> EnvironmentMode {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(|h| PathBuf::from(h).join(".config").join("magpie"))
        .map_or(EnvironmentMode::StandaloneNative, |dir| {
            detect_mode_from_config_dir(&dir)
        })
}

async fn run_cli(cli: Cli) -> Result<()> {
    let addr = resolve_daemon_addr(cli.addr.as_deref());
    let client = DaemonHttpClient::new(addr.clone());

    match cli.command {
        Commands::Version => {
            println!(
                "soxu v{} (protocol v{})",
                env!("CARGO_PKG_VERSION"),
                PROTOCOL_VERSION
            );
        }
        Commands::Doctor { json } => {
            let mode = detect_magpie_mode();
            let daemon_health = client.get_json::<HealthSnapshot>("/api/health").await.ok();
            if json {
                let out = serde_json::json!({
                    "cli_version": env!("CARGO_PKG_VERSION"),
                    "protocol_version": PROTOCOL_VERSION,
                    "environment_mode": mode,
                    "daemon_addr": addr,
                    "daemon_online": daemon_health.is_some(),
                    "daemon_health": daemon_health,
                });
                println!("{}", serde_json::to_string_pretty(&out)?);
            } else {
                println!("环境发现模式: {mode:?}");
                match daemon_health {
                    Some(h) => println!(
                        "守护进程状态: 在线 ({addr} / v{} / protocol v{})",
                        h.daemon_version, h.protocol_version
                    ),
                    None => println!(
                        "守护进程状态: 离线 ({addr}，可执行 cargo run -p soxu-daemon 启动)"
                    ),
                }
            }
        }
        Commands::Status { json } => {
            let health: HealthSnapshot = client.get_json("/api/health").await?;
            let snap: StateSnapshot = client.get_json("/api/snapshot").await?;
            if json {
                let out = serde_json::json!({
                    "health": health,
                    "snapshot": snap,
                });
                println!("{}", serde_json::to_string_pretty(&out)?);
            } else {
                println!(
                    "Soxu Daemon v{} (protocol v{}, magpie={})",
                    health.daemon_version, health.protocol_version, health.magpie_detected
                );
                println!(
                    "项目: {} | 工作区: {} | 快速对话: {} | 活跃会话: {} | 待审批: {} | 已配对设备: {}",
                    snap.projects.len(),
                    snap.workspaces.len(),
                    snap.quick_chats.len(),
                    snap.sessions.len(),
                    snap.pending_approvals.len(),
                    snap.paired_devices.len()
                );
            }
        }
        Commands::Chat {
            subcommand:
                ChatCommands::New {
                    agent,
                    model,
                    title,
                },
        } => {
            let req = RpcRequest {
                request_id: "cli-chat-new".to_owned(),
                command: RpcCommand::CreateQuickChat {
                    title,
                    agent_id: agent,
                    model_id: model,
                    permission: None,
                    thinking: None,
                },
            };
            let resp: RpcResponse = client.post_json("/api/rpc", &req).await?;
            if !resp.ok {
                let err = resp
                    .error
                    .map_or_else(|| "未知错误".to_owned(), |e| e.to_string());
                bail!("{err}");
            }
            if let Some(RpcResultData::QuickChatCreated(qc)) = resp.result {
                println!(
                    "已创建快速对话 {}「{}」(agent={}, model={})",
                    qc.chat_id, qc.title, qc.agent_id, qc.model_id
                );
            }
        }
        Commands::Pair { subcommand } => match subcommand {
            PairCommands::Create { transport } => {
                let req = RpcRequest {
                    request_id: "cli-pair-create".to_owned(),
                    command: RpcCommand::CreatePairingOffer {
                        transport: transport.into(),
                    },
                };
                let resp: RpcResponse = client.post_json("/api/rpc", &req).await?;
                if !resp.ok {
                    let err = resp
                        .error
                        .map_or_else(|| "未知错误".to_owned(), |e| e.to_string());
                    bail!("{err}");
                }
                if let Some(RpcResultData::PairingOfferCreated { code, .. }) = resp.result {
                    println!("6 位配对码: {}", code.six_digit_code);
                    println!("扫码链接: {}", code.offer_url);
                    println!("有效期: {} 秒", code.ttl_seconds);
                }
            }
            PairCommands::Devices => {
                let snap: StateSnapshot = client.get_json("/api/snapshot").await?;
                if snap.paired_devices.is_empty() {
                    println!("当前暂无已配对移动设备");
                } else {
                    for dev in snap.paired_devices {
                        println!(
                            "{} | {} ({}) | revoked={} | online={}",
                            dev.device_id, dev.device_name, dev.platform, dev.revoked, dev.online
                        );
                    }
                }
            }
            PairCommands::Revoke { device_id } => {
                let req = RpcRequest {
                    request_id: "cli-pair-revoke".to_owned(),
                    command: RpcCommand::RevokePairedDevice {
                        device_id: device_id.clone(),
                    },
                };
                let resp: RpcResponse = client.post_json("/api/rpc", &req).await?;
                if !resp.ok {
                    let err = resp
                        .error
                        .map_or_else(|| "未知错误".to_owned(), |e| e.to_string());
                    bail!("{err}");
                }
                println!("已吊销设备访问令牌: {device_id}");
            }
        },
        Commands::Events { after_seq, limit } => {
            let path = format!("/api/events?after_seq={after_seq}&limit={limit}");
            let result: RpcResultData = client.get_json(&path).await?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    run_cli(cli).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use client::parse_http_response;

    #[test]
    fn parses_subcommands_cleanly() {
        let cli = Cli::try_parse_from([
            "soxu",
            "--addr",
            "127.0.0.1:9000",
            "pair",
            "create",
            "--transport",
            "encrypted-relay",
        ])
        .expect("parse pair create command");
        assert_eq!(resolve_daemon_addr(cli.addr.as_deref()), "127.0.0.1:9000");

        let chat_cli = Cli::try_parse_from([
            "soxu",
            "chat",
            "new",
            "--agent",
            "pi",
            "--title",
            "设计评审",
        ])
        .expect("parse chat new");
        assert!(matches!(chat_cli.command, Commands::Chat { .. }));
    }

    #[test]
    fn parses_plain_and_chunked_http_responses() {
        let plain = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{\"ok\":true}";
        let (st, body) = parse_http_response(plain).expect("parse plain http");
        assert_eq!(st, 200);
        assert_eq!(body, "{\"ok\":true}");

        let chunked =
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nB\r\n{\"ok\":true}\r\n0\r\n\r\n";
        let (st2, body2) = parse_http_response(chunked).expect("parse chunked http");
        assert_eq!(st2, 200);
        assert_eq!(body2, "{\"ok\":true}");
    }
}
