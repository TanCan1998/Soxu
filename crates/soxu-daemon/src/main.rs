//! Soxu 核心守护进程入口：持久化状态机、WebSocket 游标事件流与移动配对鉴权服务。

use anyhow::Context;
use soxu_daemon::{DaemonState, build_router};
use soxu_drivers::{DriverError, NativeCliKind, NativeDriver};
use std::ffi::OsString;
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

fn configure_pi(session_dir: PathBuf) -> Result<NativeDriver, DriverError> {
    let program = std::env::var_os("SOXU_PI_EXE").ok_or(DriverError::BinaryNotFound)?;
    let mut prefix = Vec::new();
    if let Some(entry) = std::env::var_os("SOXU_PI_ENTRY") {
        prefix.push(validate_js_entry(entry, "SOXU_PI_ENTRY")?);
    }
    prefix.extend([
        OsString::from("--session-dir"),
        session_dir.into_os_string(),
    ]);
    let driver = NativeDriver::with_program(NativeCliKind::Pi, program.into(), prefix)?;
    if !driver.program().is_file() {
        return Err(DriverError::BinaryNotFound);
    }
    if cfg!(windows) && driver.program().with_extension("shim").is_file() {
        return Err(DriverError::Unsupported("Windows Pi 入口为 Scoop 包装器"));
    }
    Ok(driver)
}

fn validate_js_entry(entry: OsString, parameter: &'static str) -> Result<OsString, DriverError> {
    let path = Path::new(&entry);
    if path.is_absolute()
        && path.extension().is_some_and(|ext| {
            ["js", "mjs", "cjs"]
                .iter()
                .any(|allowed| ext.eq_ignore_ascii_case(allowed))
        })
        && path.is_file()
    {
        Ok(entry)
    } else {
        Err(DriverError::InvalidRequest(parameter))
    }
}

fn configure_claude(config_dir: PathBuf) -> Result<NativeDriver, DriverError> {
    let program = std::env::var_os("SOXU_CLAUDE_EXE").ok_or(DriverError::BinaryNotFound)?;
    let driver = NativeDriver::with_program(NativeCliKind::ClaudeCode, program.into(), [])?;
    if !driver.program().is_file() {
        return Err(DriverError::BinaryNotFound);
    }
    if cfg!(windows) && driver.program().with_extension("shim").is_file() {
        return Err(DriverError::Unsupported(
            "Windows Claude Code 入口为 Scoop 包装器",
        ));
    }
    Ok(driver
        .with_env("CLAUDE_CONFIG_DIR", Some(config_dir.into_os_string()))
        .with_env("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", Some("1".into())))
}

fn configure_qwen(config_dir: PathBuf) -> Result<NativeDriver, DriverError> {
    let program = std::env::var_os("SOXU_QWEN_NODE").ok_or(DriverError::BinaryNotFound)?;
    let entry = std::env::var_os("SOXU_QWEN_ENTRY")
        .ok_or(DriverError::InvalidRequest("SOXU_QWEN_ENTRY"))?;
    let entry = validate_js_entry(entry, "SOXU_QWEN_ENTRY")?;
    let driver = NativeDriver::with_program(
        NativeCliKind::QwenCode,
        program.into(),
        [
            "--expose-gc".into(),
            entry,
            "--auth-type".into(),
            "openai".into(),
        ],
    )?;
    if !driver.program().is_file() {
        return Err(DriverError::BinaryNotFound);
    }
    if cfg!(windows) && driver.program().with_extension("shim").is_file() {
        return Err(DriverError::Unsupported("Windows Node 入口为 Scoop 包装器"));
    }
    std::fs::create_dir_all(&config_dir).map_err(|_| DriverError::InvalidRequest("QWEN_HOME"))?;
    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(config_dir.join("settings.json"))
    {
        Ok(mut settings) => std::io::Write::write_all(&mut settings, b"{}\n")
            .map_err(|_| DriverError::InvalidRequest("QWEN_HOME"))?,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(_) => return Err(DriverError::InvalidRequest("QWEN_HOME")),
    }
    Ok(driver
        .with_env(
            "QWEN_RUNTIME_DIR",
            Some(config_dir.join("runtime").into_os_string()),
        )
        .with_env(
            "QWEN_CONVERSATIONS_DIR",
            Some(config_dir.join("conversations").into_os_string()),
        )
        .with_env("QWEN_HOME", Some(config_dir.into_os_string()))
        .with_env("QWEN_CODE_RELAUNCH_ARGS", None))
}

fn configure_copilot(config_dir: PathBuf) -> Result<NativeDriver, DriverError> {
    let program = std::env::var_os("SOXU_COPILOT_EXE").ok_or(DriverError::BinaryNotFound)?;
    let driver = NativeDriver::with_program(NativeCliKind::Copilot, program.into(), [])?;
    if !driver.program().is_file() {
        return Err(DriverError::BinaryNotFound);
    }
    if cfg!(windows) && driver.program().with_extension("shim").is_file() {
        return Err(DriverError::Unsupported(
            "Windows Copilot 入口为 Scoop 包装器",
        ));
    }
    Ok(driver
        .with_env(
            "COPILOT_HOME",
            Some(config_dir.join("state").into_os_string()),
        )
        .with_env(
            "COPILOT_CACHE_HOME",
            Some(config_dir.join("cache").into_os_string()),
        )
        .with_env("COPILOT_AUTO_UPDATE", Some("false".into()))
        .with_env("COPILOT_ALLOW_ALL", Some("false".into()))
        .with_env("COPILOT_ASSISTED_APPROVAL", Some("false".into()))
        .with_env("COPILOT_PLUGIN_DIR_ONLY", Some("true".into()))
        .with_env("COPILOT_DISABLE_RESUME_AUTO_CD", Some("1".into()))
        .with_env("COPILOT_PROVIDER_API_KEY_COMMAND", None)
        .with_env("COPILOT_SKILLS_DIRS", None))
}

async fn configure_opencode(config_dir: PathBuf) -> Result<NativeDriver, DriverError> {
    let program = std::env::var_os("SOXU_OPENCODE_EXE").ok_or(DriverError::BinaryNotFound)?;
    let mut driver =
        NativeDriver::with_program(NativeCliKind::OpenCode, program.into(), ["--pure".into()])?;
    if !driver.program().is_file() {
        return Err(DriverError::BinaryNotFound);
    }
    if cfg!(windows) && driver.program().with_extension("shim").is_file() {
        return Err(DriverError::Unsupported(
            "Windows OpenCode 入口为 Scoop 包装器",
        ));
    }
    let input = std::env::var("OPENCODE_CONFIG_CONTENT")
        .map_err(|_| DriverError::InvalidRequest("OPENCODE_CONFIG_CONTENT"))?;
    let mut config: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&input)
        .map_err(|_| DriverError::InvalidRequest("OPENCODE_CONFIG_CONTENT"))?;
    config.retain(|key, _| matches!(key.as_str(), "provider" | "model"));
    if !config
        .get("model")
        .is_some_and(serde_json::Value::is_string)
    {
        return Err(DriverError::InvalidRequest("OPENCODE_CONFIG_CONTENT.model"));
    }
    let deny = serde_json::json!({"*": "deny"});
    config.insert("permission".to_owned(), deny.clone());
    config.insert("autoupdate".to_owned(), false.into());
    config.insert("share".to_owned(), "disabled".into());

    let mut probe = tokio::process::Command::new(driver.program());
    probe
        .arg("--version")
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    for (key, subdir) in [
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_CACHE_HOME", "cache"),
        ("XDG_STATE_HOME", "state"),
        ("OPENCODE_TEST_HOME", "home"),
    ] {
        let path = config_dir.join(subdir);
        probe.env(key, &path);
        driver = driver.with_env(key, Some(path.into_os_string()));
    }
    let version = tokio::time::timeout(std::time::Duration::from_secs(5), probe.output())
        .await
        .ok()
        .and_then(Result::ok);
    if !version.is_some_and(|output| {
        output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == "1.19.6"
    }) {
        return Err(DriverError::Unsupported("OpenCode 只读已验证范围为 1.19.6"));
    }
    Ok(driver
        .with_env("OPENCODE_CONFIG", None)
        .with_env("OPENCODE_CONFIG_DIR", None)
        .with_env("OPENCODE_DB", None)
        .with_env("OPENCODE_WORKSPACE_ID", None)
        .with_env("OPENCODE_PERMISSION", Some(deny.to_string().into()))
        .with_env(
            "OPENCODE_CONFIG_CONTENT",
            Some(serde_json::Value::Object(config).to_string().into()),
        )
        .with_env("OPENCODE_DISABLE_PROJECT_CONFIG", Some("1".into()))
        .with_env("OPENCODE_DISABLE_AUTOUPDATE", Some("1".into()))
        .with_env("OPENCODE_DISABLE_MODELS_FETCH", Some("1".into())))
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
    let native_root = data_dir.unwrap_or_else(|| {
        std::env::temp_dir().join(format!("soxu-{}", state.daemon_fingerprint()))
    });
    let native_root = std::path::absolute(native_root)?;
    let (agy, agy_guard) = match std::env::var_os("SOXU_AGY_EXE") {
        Some(program) => {
            match soxu_daemon::antigravity::node_program()
                .context("缺少可信 Node 可执行文件")
                .and_then(|node| {
                    soxu_daemon::antigravity::configure(
                        &native_root.join("antigravity"),
                        Path::new(&program),
                        &node,
                    )
                }) {
                Ok((mut driver, guard)) => {
                    if let Some(key) =
                        std::env::var_os("SOXU_AGY_GEMINI_API_KEY").filter(|key| !key.is_empty())
                    {
                        driver = driver.with_env("GEMINI_API_KEY", Some(key));
                    }
                    (Ok(driver), Some(guard))
                }
                Err(error) => {
                    tracing::warn!(%error, "Antigravity 只读快聊未能配置");
                    (
                        Err(DriverError::Unsupported("Antigravity 隔离配置未通过校验")),
                        None,
                    )
                }
            }
        }
        None => (Err(DriverError::BinaryNotFound), None),
    };
    let mut state = state.with_native_driver_set(
        native_root.join("quick-chats"),
        [
            (NativeCliKind::Codex, discover_codex()),
            (
                NativeCliKind::Pi,
                configure_pi(native_root.join("pi-sessions")),
            ),
            (
                NativeCliKind::ClaudeCode,
                configure_claude(native_root.join("claude")),
            ),
            (
                NativeCliKind::QwenCode,
                configure_qwen(native_root.join("qwen")),
            ),
            (
                NativeCliKind::Copilot,
                configure_copilot(native_root.join("copilot")),
            ),
            (
                NativeCliKind::OpenCode,
                configure_opencode(native_root.join("opencode")).await,
            ),
            (NativeCliKind::Antigravity, agy),
        ],
    )?;
    if let Some(guard) = agy_guard {
        state = state.with_native_asset_guard(NativeCliKind::Antigravity, guard)?;
    }
    let state = Arc::new(state);
    info!(
        address = %local_addr,
        magpie_detected = state.magpie_detected(),
        fingerprint = %state.daemon_fingerprint(),
        "Soxu daemon started"
    );

    axum::serve(listener, build_router(state)).await?;
    Ok(())
}
