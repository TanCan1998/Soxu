//! 配置 Daemon 私有的 Antigravity 只读快聊启动环境。

use crate::NativeAssetGuard;
use crate::native_assets::checked_directory;
use anyhow::{Context, bail, ensure};
use soxu_drivers::{NativeCliKind, NativeDriver};
#[cfg(windows)]
use std::ffi::OsString;
use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

pub(crate) fn canonical_agent_id(agent: String) -> String {
    if agent == "antigravity" {
        "agy".to_owned()
    } else {
        agent
    }
}

const PRIVATE_DIRECTORIES: &[&str] = &[
    "home",
    "home/.gemini",
    "home/.gemini/config",
    "home/.gemini/config/agents",
    "home/.gemini/config/projects",
    "appdata",
    "local-appdata",
    "tmp",
    "xdg-config",
    "xdg-cache",
    "xdg-data",
    "xdg-state",
    "quick-chats",
];
const AGENT: &str = "soxu-no-tools";
const AGENT_CONTENT: &str = "---
name: soxu-no-tools
description: Soxu 只读快速对话
tools: []
mainAgent: true
subagent: false
commandExecutionPolicy: off
mcpServers: []
skills: []
plugins: []
---
仅回复文本，不执行工具。
";
const HOOK_CONTENT: &str =
    r#"console.log(JSON.stringify({ decision: "deny", reason: "SOXU_HOOK_DENY" }));"#;

/// 使用宿主提供的可信原生入口。配置和会话数据留在 Daemon 私有目录。
pub fn configure(
    root: &Path,
    program: &Path,
    node: &Path,
) -> anyhow::Result<(NativeDriver, NativeAssetGuard)> {
    let program = trusted_program(program).context("Antigravity 原生入口不可用")?;
    let node = trusted_program(node).context("Antigravity 钩子所需的原生 Node 入口不可用")?;
    let parent = root.parent().context("Antigravity 隔离目录缺少父目录")?;
    checked_directory(parent)?;
    private_directory(root)?;
    #[cfg(windows)]
    restrict_windows_root(root)?;
    let root = root.canonicalize()?;
    for path in PRIVATE_DIRECTORIES {
        private_directory(&root.join(path))?;
    }

    let hook_path = root.join("deny-tool.cjs");
    let hook_command = format!("{} {}", hook_argument(&node)?, hook_argument(&hook_path)?);
    let hooks = serde_json::to_vec(&serde_json::json!({
        "soxu-deny-tools": {
            "enabled": true,
            "PreToolUse": [{
                "matcher": "*",
                "hooks": [{"type": "command", "command": hook_command, "timeout": 5}]
            }]
        }
    }))?;
    let assets = [
        (
            "home/.gemini/config/agents/soxu-no-tools.md",
            AGENT_CONTENT.as_bytes().to_vec(),
        ),
        ("home/.gemini/config/hooks.json", hooks),
        ("home/.gemini/config/settings.json", b"{}".to_vec()),
        (
            "home/.gemini/config/config.json",
            br#"{"plugins":{"swarm":{"enabled":false}}}"#.to_vec(),
        ),
        (
            "home/.gemini/config/mcp_config.json",
            br#"{"mcpServers":{}}"#.to_vec(),
        ),
        ("home/.gemini/config/.migrated", Vec::new()),
        ("deny-tool.cjs", HOOK_CONTENT.as_bytes().to_vec()),
    ];
    for (relative, content) in &assets {
        write_new(&root.join(relative), content)?;
    }
    let guard = NativeAssetGuard::new(
        root.clone(),
        assets
            .into_iter()
            .map(|(path, bytes)| (PathBuf::from(path), bytes)),
    )?
    .protect()?;
    verify_config_layout(&guard)?;
    guard.verify_launch_directory(&root.join("quick-chats"))?;

    let mut driver = NativeDriver::with_program(
        NativeCliKind::Antigravity,
        program,
        ["--agent".into(), AGENT.into()],
    )?
    .without_inherited_env();
    for (key, relative) in [
        ("HOME", "home"),
        ("USERPROFILE", "home"),
        ("APPDATA", "appdata"),
        ("LOCALAPPDATA", "local-appdata"),
        ("TEMP", "tmp"),
        ("TMP", "tmp"),
        ("TMPDIR", "tmp"),
        ("XDG_CONFIG_HOME", "xdg-config"),
        ("XDG_CACHE_HOME", "xdg-cache"),
        ("XDG_DATA_HOME", "xdg-data"),
        ("XDG_STATE_HOME", "xdg-state"),
    ] {
        driver = driver.with_env(key, Some(root.join(relative).into_os_string()));
    }
    #[cfg(windows)]
    {
        let windows = windows_root()?;
        let system32 = windows.join("System32");
        let drive = windows
            .ancestors()
            .last()
            .context("无法确定 Windows 系统盘")?;
        let drive = drive.as_os_str().to_string_lossy();
        for (key, value) in [
            ("SystemRoot", windows.clone().into_os_string()),
            ("WINDIR", windows.clone().into_os_string()),
            ("SystemDrive", OsString::from(drive.trim_end_matches('\\'))),
            ("COMSPEC", system32.join("cmd.exe").into_os_string()),
            ("PATH", system32.into_os_string()),
        ] {
            driver = driver.with_env(key, Some(value));
        }
    }
    #[cfg(unix)]
    {
        driver = driver.with_env("PATH", Some("/usr/bin:/bin".into()));
    }
    Ok((driver, guard))
}

pub(crate) fn verify_config_layout(guard: &NativeAssetGuard) -> anyhow::Result<()> {
    for path in PRIVATE_DIRECTORIES {
        guard.verify_private_directory(&guard.root().join(path))?;
    }
    for (directory, known) in [
        (
            "home/.gemini/config",
            &[
                "agents",
                "projects",
                "settings.json",
                "config.json",
                "mcp_config.json",
                "hooks.json",
                ".migrated",
            ][..],
        ),
        ("home/.gemini/config/agents", &["soxu-no-tools.md"][..]),
    ] {
        for entry in fs::read_dir(guard.root().join(directory))? {
            let entry = entry?;
            ensure!(
                known.iter().any(|name| entry.file_name() == *name),
                "Antigravity 隔离配置出现未经声明的资产"
            );
        }
    }
    Ok(())
}

pub(crate) fn quick_chat_directory(
    guard: &NativeAssetGuard,
    chat_id: &str,
) -> anyhow::Result<PathBuf> {
    let directory = guard.root().join("quick-chats");
    guard.verify_private_directory(&directory)?;
    let directory = directory.join(chat_id);
    private_directory(&directory)?;
    Ok(directory)
}

pub fn node_program() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("SOXU_AGY_NODE_EXE") {
        return Some(path.into());
    }
    let search = std::env::var_os("PATH")?;
    let executable = if cfg!(windows) { "node.exe" } else { "node" };
    std::env::split_paths(&search)
        .map(|dir| dir.join(executable))
        .find_map(|path| trusted_program(&path).ok())
}

fn trusted_program(path: &Path) -> anyhow::Result<PathBuf> {
    ensure!(path.is_absolute(), "须指定原生程序的绝对路径");
    let program = path.canonicalize()?;
    ensure!(program.is_file(), "原生入口须为普通文件");
    let mut magic = [0; 4];
    File::open(&program)?.read_exact(&mut magic)?;
    #[cfg(windows)]
    ensure!(
        magic.starts_with(b"MZ")
            && [path, &program].into_iter().all(|candidate| {
                candidate
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
                    && !candidate.with_extension("shim").exists()
            }),
        "Windows 入口须为无 shim 的原生 EXE"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        ensure!(
            program.metadata()?.permissions().mode() & 0o111 != 0
                && matches!(
                    u32::from_be_bytes(magic),
                    0x7f454c46
                        | 0xfeedface
                        | 0xcefaedfe
                        | 0xfeedfacf
                        | 0xcffaedfe
                        | 0xcafebabe
                        | 0xbebafeca
                        | 0xcafebabf
                        | 0xbfbafeca
                ),
            "Unix 入口须为可执行的原生程序"
        );
    }
    Ok(program)
}

fn private_directory(path: &Path) -> anyhow::Result<()> {
    let builder = &mut DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt as _;
        builder.mode(0o700);
    }
    match builder.create(path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error.into()),
    }
    checked_directory(path)?;
    Ok(())
}

fn write_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    match options.open(path) {
        Ok(mut file) => {
            file.write_all(bytes)?;
            file.sync_all()
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(error),
    }
}

fn hook_argument(path: &Path) -> anyhow::Result<String> {
    let path = path.to_str().context("钩子路径必须是有效文本")?;
    #[cfg(windows)]
    let path = path.strip_prefix(r"\\?\").unwrap_or(path);
    if path.chars().any(char::is_control)
        || path.contains(['"', '$', '%', '!', '^', '&', '|', '<', '>'])
    {
        bail!("钩子路径包含命令解释器特殊字符");
    }
    Ok(format!(r#""{path}""#))
}

#[cfg(windows)]
fn windows_root() -> anyhow::Result<PathBuf> {
    let root = PathBuf::from(std::env::var_os("SystemRoot").context("缺少 Windows 系统目录")?);
    checked_directory(&root.join("System32"))?;
    Ok(root)
}

#[cfg(windows)]
fn restrict_windows_root(root: &Path) -> anyhow::Result<()> {
    use std::process::{Command, Stdio};
    let icacls = windows_root()?.join("System32/icacls.exe");
    let status = Command::new(icacls)
        .arg(root)
        .args(["/inheritance:r", "/grant:r", "*S-1-3-4:(OI)(CI)F"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    ensure!(status.success(), "无法限制 Antigravity 隔离目录的访问权限");
    Ok(())
}
