//! 受信任 Git 二进制的受控运行环境。
//!
//! 每个请求在协调域内得到独立的工作目录、HOME、XDG、临时目录与空钩子目录；
//! 继承的 `GIT_*`、代理与凭据相关变量全部清除，网络协议默认关闭，只放行本次操作声明的协议。

use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;

use super::RepositoryCreateError;

/// 单个受控 Git 调用的输出上限。
pub(super) const OUTPUT_LIMIT: u64 = 16 * 1024 * 1024;
/// 单个受控 Git 调用的墙钟上限。
pub(super) const COMMAND_TIMEOUT: Duration = Duration::from_secs(120);

const SANDBOX_DIR: &str = "sandbox";
const ENV_DENY_PREFIXES: [&str; 5] = ["GIT_", "SSH_", "GPG_", "LC_", "XDG_"];
const ENV_DENY_EXACT: [&str; 14] = [
    "HOME",
    "USERPROFILE",
    "HOMEDRIVE",
    "HOMEPATH",
    "TMP",
    "TEMP",
    "TMPDIR",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "NO_PROXY",
    "http_proxy",
    "https_proxy",
    "all_proxy",
];

/// 请求独占的 Git 运行沙箱。
#[derive(Debug)]
pub(super) struct Sandbox {
    root: PathBuf,
    home: PathBuf,
    temporary: PathBuf,
    hooks: PathBuf,
    gitconfig: PathBuf,
}

impl Sandbox {
    /// 在协调域内建立沙箱目录。失败时不触碰目标与源。
    pub(super) fn prepare(
        state_root: &Path,
        request_id: &str,
    ) -> Result<Self, RepositoryCreateError> {
        let root = state_root.join(SANDBOX_DIR).join(request_id);
        ensure_plain_directory(&state_root.join(SANDBOX_DIR))?;
        ensure_plain_directory(&root)?;
        let home = root.join("home");
        let temporary = root.join("tmp");
        let hooks = root.join("hooks");
        let gitconfig = root.join("gitconfig");
        ensure_plain_directory(&home)?;
        ensure_plain_directory(&temporary)?;
        ensure_plain_directory(&hooks)?;
        if !hooks
            .read_dir()
            .map_err(RepositoryCreateError::Io)?
            .next()
            .is_none()
        {
            return Err(RepositoryCreateError::Unauthorized("沙箱钩子目录必须为空"));
        }
        match gitconfig.symlink_metadata() {
            Ok(metadata) => {
                if !metadata.is_file() || metadata.is_symlink() {
                    return Err(RepositoryCreateError::Unauthorized(
                        "沙箱配置必须是普通文件",
                    ));
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                std::fs::write(&gitconfig, b"").map_err(RepositoryCreateError::Io)?;
            }
            Err(error) => return Err(RepositoryCreateError::Io(error)),
        }
        Ok(Self {
            root,
            home,
            temporary,
            hooks,
            gitconfig,
        })
    }

    pub(super) fn root(&self) -> &Path {
        &self.root
    }

    /// 建立已清理环境与固定 `-c` 前缀的 Git 命令；调用方只追加子命令参数。
    pub(super) fn command(&self, program: &Path, workdir: &Path) -> Command {
        let mut command = Command::new(program);
        command
            .current_dir(workdir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        for (name, _) in std::env::vars_os() {
            if denied(&name) {
                command.env_remove(name);
            }
        }
        command
            .env("LC_ALL", "C")
            .env("LANG", "C")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_SYSTEM", null_device())
            .env("GIT_CONFIG_GLOBAL", &self.gitconfig)
            .env("GIT_ATTR_NOSYSTEM", "1")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_ASKPASS", "")
            .env("SSH_ASKPASS", "")
            .env("GIT_PROTOCOL_FROM_USER", "0")
            .env("GIT_ALLOW_PROTOCOL", "file")
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("GIT_NO_LAZY_FETCH", "1")
            .env("GIT_LFS_SKIP_SMUDGE", "1")
            .env("GIT_PAGER", "cat")
            .env("GIT_EDITOR", ":")
            .env("HOME", &self.home)
            .env("USERPROFILE", &self.home)
            .env("XDG_CONFIG_HOME", self.home.join(".config"))
            .env("XDG_DATA_HOME", self.home.join(".local").join("share"))
            .env("XDG_CACHE_HOME", self.home.join(".cache"))
            .env("TMPDIR", &self.temporary)
            .env("TMP", &self.temporary)
            .env("TEMP", &self.temporary)
            .args(["--no-optional-locks"]);
        for (key, value) in self.fixed_config() {
            command.arg("-c").arg(format!("{key}={value}"));
        }
        command
    }

    fn fixed_config(&self) -> Vec<(&'static str, String)> {
        let hooks = self.hooks.to_string_lossy().into_owned();
        vec![
            ("core.hooksPath", hooks),
            ("core.fsmonitor", "false".to_owned()),
            ("core.untrackedCache", "false".to_owned()),
            ("core.askPass", String::new()),
            ("core.editor", ":".to_owned()),
            ("core.pager", "cat".to_owned()),
            ("core.autocrlf", "false".to_owned()),
            ("credential.helper", String::new()),
            ("maintenance.auto", "false".to_owned()),
            ("gc.auto", "0".to_owned()),
            ("protocol.allow", "never".to_owned()),
            ("protocol.file.allow", "always".to_owned()),
            ("init.templateDir", String::new()),
            ("init.defaultBranch", "main".to_owned()),
            ("submodule.recurse", "false".to_owned()),
            ("index.threads", "1".to_owned()),
        ]
    }
}

fn denied(name: &OsStr) -> bool {
    let Some(name) = name.to_str() else {
        return true;
    };
    let upper = name.to_ascii_uppercase();
    ENV_DENY_PREFIXES
        .iter()
        .any(|prefix| upper.starts_with(prefix))
        || ENV_DENY_EXACT.contains(&upper.as_str())
}

fn null_device() -> &'static OsStr {
    OsStr::new(if cfg!(windows) { "NUL" } else { "/dev/null" })
}

fn ensure_plain_directory(path: &Path) -> Result<(), RepositoryCreateError> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => {
            if !super::platform::plain_directory(&metadata) {
                return Err(RepositoryCreateError::Unauthorized(
                    "沙箱目录必须是普通目录",
                ));
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            std::fs::create_dir(path).map_err(RepositoryCreateError::Io)?;
        }
        Err(error) => return Err(RepositoryCreateError::Io(error)),
    }
    Ok(())
}

/// 有界执行一次受控 Git 调用：超时或输出超限时先关闭管道再终止并回收直接子进程。
pub(super) async fn run(
    command: &mut Command,
    limit: u64,
    timeout: Duration,
) -> Result<std::process::Output, RepositoryCreateError> {
    let mut child = command.spawn().map_err(RepositoryCreateError::Io)?;
    let result = tokio::time::timeout(timeout, async {
        let stdout = child
            .stdout
            .take()
            .ok_or(RepositoryCreateError::InvalidInput("缺少 stdout 管道"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or(RepositoryCreateError::InvalidInput("缺少 stderr 管道"))?;
        let stdout = read_bounded(stdout, limit);
        let stderr = read_bounded(stderr, limit);
        let status = async { child.wait().await.map_err(RepositoryCreateError::Io) };
        let (stdout, stderr, status) = tokio::try_join!(stdout, stderr, status)?;
        Ok::<_, RepositoryCreateError>(std::process::Output {
            status,
            stdout,
            stderr,
        })
    })
    .await;
    match result {
        Ok(Ok(output)) => Ok(output),
        Ok(Err(error)) => {
            let _ = child.start_kill();
            let _ = child.wait().await;
            Err(error)
        }
        Err(_) => {
            let _ = child.start_kill();
            let _ = child.wait().await;
            Err(RepositoryCreateError::GitTimeout)
        }
    }
}

async fn read_bounded(
    reader: impl AsyncRead + Unpin,
    limit: u64,
) -> Result<Vec<u8>, RepositoryCreateError> {
    let mut bytes = Vec::new();
    reader
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(RepositoryCreateError::Io)?;
    if bytes.len() as u64 > limit {
        return Err(RepositoryCreateError::OutputLimit);
    }
    Ok(bytes)
}
