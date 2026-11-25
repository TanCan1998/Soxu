//! Soxu 核心守护进程入口：持久化状态机、WebSocket 游标事件流与移动配对鉴权服务。

use soxu_daemon::{DaemonState, build_router};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::info;

fn default_data_dir() -> Option<PathBuf> {
    if let Ok(explicit) = std::env::var("SOXU_DATA_DIR")
        && !explicit.trim().is_empty()
    {
        return Some(PathBuf::from(explicit));
    }
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(|home| PathBuf::from(home).join(".soxu"))
}

fn is_loopback_bind(addr: &str) -> bool {
    addr.starts_with("127.0.0.1:") || addr.starts_with("localhost:") || addr.starts_with("[::1]:")
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
    let endpoint_url = format!("http://{bind_addr}");
    let require_auth_for_all = !is_loopback_bind(&bind_addr);
    let data_dir = default_data_dir();

    let state = Arc::new(DaemonState::open(
        data_dir.as_deref(),
        &endpoint_url,
        require_auth_for_all,
    )?);

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    info!(
        address = %bind_addr,
        magpie_detected = state.magpie_detected(),
        fingerprint = %state.daemon_fingerprint(),
        "Soxu daemon started"
    );

    axum::serve(listener, build_router(state)).await?;
    Ok(())
}
