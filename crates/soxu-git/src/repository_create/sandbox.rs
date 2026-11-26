//! 受信任 Git 二进制的受控运行环境。
//!
//! 每个请求在协调域内得到独立的工作目录、HOME、XDG、临时目录与空钩子目录；
//! 继承的 `GIT_*`、代理与凭据相关变量全部清除，网络协议默认关闭，只放行本次操作声明的协议。
//!
//! 该环境有两种打开方式：`prepare` 会建立（或复用）请求沙箱，仅用于确实要执行 Git 写入的路径；
//! `prepare_readonly` 不创建任何目录与文件，供重复请求回查与失败后的只读复核使用。

use std::ffi::OsStr;

use std::io;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;

use super::RepositoryCreateError;
use super::platform::AuthorizedDirectory;

/// 单个受控 Git 调用的输出上限。
pub(super) const OUTPUT_LIMIT: u64 = 16 * 1024 * 1024;
/// 单个受控 Git 调用的墙钟上限。
pub(super) const COMMAND_TIMEOUT: Duration = Duration::from_secs(120);

const SANDBOX_DIR: &str = "sandbox";
const WORK_DIR: &str = "work";
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
    work: AuthorizedDirectory,
    home: PathBuf,
    temporary: PathBuf,
    hooks: PathBuf,
    /// 只读沙箱可能没有可写的全局配置文件，此时 `GIT_CONFIG_GLOBAL` 指向空设备。
    gitconfig: Option<PathBuf>,
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
        let work_path = root.join(WORK_DIR);
        let gitconfig = root.join("gitconfig");
        ensure_plain_directory(&home)?;
        ensure_plain_directory(&temporary)?;
        ensure_plain_directory(&hooks)?;
        ensure_plain_directory(&work_path)?;
        let work = AuthorizedDirectory::open(&work_path)?;
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
            work,
            home,
            temporary,
            hooks,
            gitconfig: Some(gitconfig),
        })
    }

    /// 只读打开既有沙箱：不创建目录、不写配置文件。
    ///
    /// 沙箱缺失时把工作目录退到协调域本身，并把 `GIT_CONFIG_GLOBAL` 指向空设备；
    /// 该模式只允许运行 `ls-tree`、`cat-file`、`symbolic-ref`、`rev-parse`、`status` 一类只读命令。
    pub(super) fn prepare_readonly(
        state_root: &Path,
        request_id: &str,
    ) -> Result<Self, RepositoryCreateError> {
        let root = state_root.join(SANDBOX_DIR).join(request_id);
        let root = if plain_directory_exists(&root)? {
            root
        } else {
            state_root.to_owned()
        };
        let home = existing_child(&root, "home")?;
        let temporary = existing_child(&root, "tmp")?;
        let hooks = existing_child(&root, "hooks")?;
        let work = AuthorizedDirectory::open(&existing_child(&root, WORK_DIR)?)?;
        let gitconfig = root.join("gitconfig");
        let gitconfig = match gitconfig.symlink_metadata() {
            Ok(metadata) if metadata.is_file() && !metadata.is_symlink() => Some(gitconfig),
            Ok(_) => {
                return Err(RepositoryCreateError::Unauthorized(
                    "沙箱配置必须是普通文件",
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(RepositoryCreateError::Io(error)),
        };
        Ok(Self {
            root,
            work,
            home,
            temporary,
            hooks,
            gitconfig,
        })
    }

    pub(super) fn root(&self) -> &Path {
        &self.root
    }

    /// 宿主私有的 Git 写入暂存目录：Git 只经该句柄锚定的路径执行，绝不直接写最终目标。
    pub(super) fn work(&self) -> &AuthorizedDirectory {
        &self.work
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
        let global = self
            .gitconfig
            .clone()
            .unwrap_or_else(|| PathBuf::from(null_device()));
        command
            .env("LC_ALL", "C")
            .env("LANG", "C")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_SYSTEM", null_device())
            .env("GIT_CONFIG_GLOBAL", global)
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

fn plain_directory_exists(path: &Path) -> Result<bool, RepositoryCreateError> {
    match path.symlink_metadata() {
        Ok(metadata) => {
            if !super::platform::plain_directory(&metadata) {
                return Err(RepositoryCreateError::Unauthorized(
                    "沙箱目录必须是普通目录",
                ));
            }
            Ok(true)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(RepositoryCreateError::Io(error)),
    }
}

fn existing_child(root: &Path, name: &str) -> Result<PathBuf, RepositoryCreateError> {
    let candidate = root.join(name);
    Ok(if plain_directory_exists(&candidate)? {
        candidate
    } else {
        root.to_owned()
    })
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
    harden(path)
}

/// 沙箱目录由宿主独占：Unix 上收敛为 0700，避免第三主体访问暂存。
#[cfg(unix)]
fn harden(path: &Path) -> Result<(), RepositoryCreateError> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = std::fs::metadata(path)
        .map_err(RepositoryCreateError::Io)?
        .permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(path, permissions).map_err(RepositoryCreateError::Io)
}

#[cfg(not(unix))]
fn harden(_path: &Path) -> Result<(), RepositoryCreateError> {
    Ok(())
}

/// 一次受控 Git 调用的完整结果。
#[derive(Debug)]
pub(super) struct RunOutput {
    pub status: std::process::ExitStatus,
    pub stdout: Vec<u8>,
    /// 保留供失败诊断，不进入留痕与对外输出。
    #[allow(dead_code)]
    pub stderr: Vec<u8>,
    /// 只证明本次直接子进程已退出并被回收，**不**覆盖其派生孙进程（例如本地克隆的 upload-pack）。
    pub direct_child_reaped: bool,
}

/// 一次受控 Git 调用的失败，携带与失败原因对应的直接子进程回收事实。
#[derive(Debug)]
pub(super) struct RunFailure {
    pub error: RepositoryCreateError,
    /// 与 `RunOutput::direct_child_reaped` 同义，只覆盖直接子进程。
    pub direct_child_reaped: bool,
}

impl From<RunFailure> for RepositoryCreateError {
    fn from(failure: RunFailure) -> Self {
        failure.error
    }
}

/// 有界执行一次受控 Git 调用：超时或输出超限时先关闭管道再终止并回收直接子进程。
pub(super) async fn run(
    command: &mut Command,
    limit: u64,
    timeout: Duration,
) -> Result<RunOutput, RunFailure> {
    let mut child = command.spawn().map_err(|error| RunFailure {
        direct_child_reaped: false,
        error: RepositoryCreateError::Io(error),
    })?;
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
        Ok::<_, RepositoryCreateError>((status, stdout, stderr))
    })
    .await;
    match result {
        Ok(Ok((status, stdout, stderr))) => Ok(RunOutput {
            status,
            stdout,
            stderr,
            direct_child_reaped: direct_child_reaped(&mut child).await,
        }),
        Ok(Err(error)) => Err(RunFailure {
            direct_child_reaped: reap(&mut child).await,
            error,
        }),
        Err(_) => Err(RunFailure {
            direct_child_reaped: reap(&mut child).await,
            error: RepositoryCreateError::GitTimeout,
        }),
    }
}

/// 终止并回收直接子进程；只有 `wait` 成功且 `try_wait` 确证已回收才返回 `true`。
async fn reap(child: &mut tokio::process::Child) -> bool {
    let _ = child.start_kill();
    if child.wait().await.is_err() {
        return false;
    }
    direct_child_reaped(child).await
}

async fn direct_child_reaped(child: &mut tokio::process::Child) -> bool {
    matches!(child.try_wait(), Ok(Some(_)))
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
