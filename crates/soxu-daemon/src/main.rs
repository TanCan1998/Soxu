//! Soxu 核心守护进程：统一管理本机原生 Agent CLI 会话、RPC/WebSocket 事件流与内嵌移动端 Web/PWA。

use axum::{Json, Router, routing::get};
use soxu_magpie::{EnvironmentMode, detect_mode_from_config_dir};
use soxu_protocol::{HealthSnapshot, PROTOCOL_VERSION};
use std::path::PathBuf;
use tracing::info;

fn default_magpie_config_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(|home| PathBuf::from(home).join(".config").join("magpie"))
}

async fn health() -> Json<HealthSnapshot> {
    let magpie_detected = default_magpie_config_dir()
        .is_some_and(|dir| detect_mode_from_config_dir(&dir) == EnvironmentMode::MagpieManaged);

    Json(HealthSnapshot {
        status: "ok".to_owned(),
        daemon_version: env!("CARGO_PKG_VERSION").to_owned(),
        protocol_version: PROTOCOL_VERSION,
        magpie_detected,
    })
}

pub fn build_router() -> Router {
    Router::new().route("/api/health", get(health))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let bind_addr = std::env::var("SOXU_LISTEN").unwrap_or_else(|_| "127.0.0.1:7698".to_owned());
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    info!(address = %bind_addr, "Soxu daemon started");

    axum::serve(listener, build_router()).await?;
    Ok(())
}
