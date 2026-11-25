//! Soxu 核心守护进程入口：持久化状态机、WebSocket 游标事件流与移动配对鉴权服务。

use anyhow::Context;
use soxu_daemon::{DaemonState, build_router};
use soxu_drivers::{DriverError, NativeCliKind, NativeDriver};
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
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

fn lock_data_dir(dir: &Path) -> anyhow::Result<File> {
    std::fs::create_dir_all(dir)?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(dir.join("daemon.lock"))?;
    lock.try_lock()
        .context("无法独占 Soxu 数据目录，请检查其他守护进程是否正在使用")?;
    Ok(lock)
}

fn discover_codex() -> Result<NativeDriver, DriverError> {
    let driver = match std::env::var_os("SOXU_CODEX_EXE") {
        Some(program) => NativeDriver::with_program(NativeCliKind::Codex, program.into(), []),
        None => NativeDriver::discover(NativeCliKind::Codex),
    }?;
    if cfg!(windows) && driver.program().with_extension("shim").is_file() {
        return Err(DriverError::Unsupported(
            "Windows Codex 入口为 Scoop 包装器",
        ));
    }
    Ok(driver)
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
    let local_addr = listener.local_addr()?;
    let endpoint_url = format!("http://{local_addr}");
    let require_auth_for_all = !local_addr.ip().is_loopback();
    let data_dir = default_data_dir();
    let _data_lock = data_dir.as_deref().map(lock_data_dir).transpose()?;
    let state = DaemonState::open(data_dir.as_deref(), &endpoint_url, require_auth_for_all)?;
    let quick_chat_root = data_dir.unwrap_or_else(|| {
        std::env::temp_dir().join(format!("soxu-{}", state.daemon_fingerprint()))
    });
    let state = Arc::new(state.with_native_turns(
        std::path::absolute(quick_chat_root)?.join("quick-chats"),
        discover_codex(),
    )?);
    info!(
        address = %local_addr,
        magpie_detected = state.magpie_detected(),
        fingerprint = %state.daemon_fingerprint(),
        "Soxu daemon started"
    );

    axum::serve(listener, build_router(state)).await?;
    Ok(())
}
