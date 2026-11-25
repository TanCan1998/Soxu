use std::ffi::{OsStr, OsString};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use tokio::io::AsyncReadExt;
use tokio::process::Command;

use crate::{ParseError, StatusSnapshot, parse_status};

const OUTPUT_LIMIT: u64 = 16 * 1024 * 1024;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("Git 入口和仓库目录必须是绝对路径")]
    RelativePath,
    #[error("Git 入口必须是普通原生可执行文件，不能是脚本 shim")]
    InvalidProgram,
    #[error("未找到本机 Git 原生入口")]
    GitNotFound,
    #[error("无法读取指定路径或运行 Git：{0}")]
    Io(#[from] io::Error),
    #[error("指定路径不是目录")]
    NotDirectory,
    #[error("指定目录不属于 Git 仓库")]
    NotRepository,
    #[error("当前底座不支持 bare 仓库或 Git 元数据目录")]
    NoWorktree,
    #[error("仓库配置了外部内容过滤器，当前只读底座拒绝执行")]
    ExternalFilter,
    #[error("仓库含子模块，当前底座不递归执行其配置；可独立探测子模块目录")]
    SubmodulesUnsupported,
    #[error("Git {operation} 失败（退出码 {code:?}），原始 stderr 已保留")]
    CommandFailed {
        operation: &'static str,
        code: Option<i32>,
        stderr: Vec<u8>,
    },
    #[error("Git 命令超时")]
    Timeout,
    #[error("Git 输出超过 16 MiB，未返回截断状态")]
    OutputLimit,
    #[error("无法表示 Git 返回的路径，原始字节已保留")]
    UnsupportedPathEncoding(Vec<u8>),
    #[error("非法 Git 元数据：{0}")]
    InvalidMetadata(&'static str),
    #[error(transparent)]
    Parse(#[from] ParseError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeadState {
    Attached { branch: Vec<u8>, oid: String },
    Detached { oid: String },
    Unborn { branch: Vec<u8> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositorySnapshot {
    pub root: PathBuf,
    pub git_dir: PathBuf,
    pub common_dir: PathBuf,
    pub head: HeadState,
    pub status: StatusSnapshot,
}

#[derive(Debug, Clone)]
pub struct GitProbe {
    program: PathBuf,
}

impl GitProbe {
    pub fn new(program: impl AsRef<Path>) -> Result<Self, GitError> {
        let program = program.as_ref();
        if !program.is_absolute() {
            return Err(GitError::RelativePath);
        }
        if !program.is_file()
            || (cfg!(windows)
                && !program
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("exe")))
        {
            return Err(GitError::InvalidProgram);
        }
        Ok(Self {
            program: program.canonicalize()?,
        })
    }

    pub fn discover() -> Result<Self, GitError> {
        if let Some(search_path) = std::env::var_os("PATH") {
            for directory in std::env::split_paths(&search_path).filter(|path| path.is_absolute()) {
                let candidate = directory.join(if cfg!(windows) { "git.exe" } else { "git" });
                if candidate.is_file() {
                    return Self::new(candidate);
                }
            }
        }
        Err(GitError::GitNotFound)
    }

    pub fn program(&self) -> &Path {
        &self.program
    }

    pub async fn probe(&self, directory: impl AsRef<Path>) -> Result<RepositorySnapshot, GitError> {
        let directory = directory.as_ref();
        if !directory.is_absolute() {
            return Err(GitError::RelativePath);
        }
        let directory = directory.canonicalize()?;
        if !directory.is_dir() {
            return Err(GitError::NotDirectory);
        }
        let inside = self
            .run(&directory, "rev-parse", &["--is-inside-work-tree"])
            .await?;
        if inside != b"true\n" {
            return Err(GitError::NoWorktree);
        }
        let root = self.read_path(&directory, "--show-toplevel").await?;
        let git_dir = self.read_path(&root, "--absolute-git-dir").await?;
        let common_dir = self.read_path(&root, "--git-common-dir").await?;
        let filters = self
            .run(
                &root,
                "config",
                &[
                    "--null",
                    "--name-only",
                    "--get-regexp",
                    "^filter\\..*\\.(clean|process)$",
                ],
            )
            .await?;
        if !filters.is_empty() {
            return Err(GitError::ExternalFilter);
        }
        let index = self.run(&root, "ls-files", &["--stage", "-z"]).await?;
        if index
            .split(|byte| *byte == 0)
            .any(|record| record.starts_with(b"160000 "))
        {
            return Err(GitError::SubmodulesUnsupported);
        }
        let output = self
            .run(
                &root,
                "status",
                &[
                    "--porcelain=v2",
                    "-z",
                    "--branch",
                    "--untracked-files=all",
                    "--ignore-submodules=none",
                    "--renames",
                ],
            )
            .await?;
        let status = parse_status(&output)?;
        let branch = status
            .header(b"branch.head")
            .ok_or(GitError::InvalidMetadata("缺少 branch.head"))?;
        let head_oid = status
            .header(b"branch.oid")
            .ok_or(GitError::InvalidMetadata("缺少 branch.oid"))?;
        let head = match (branch, head_oid) {
            (b"(detached)", b"(initial)") => {
                return Err(GitError::InvalidMetadata("detached 与 unborn 同时出现"));
            }
            (b"(detached)", head_oid) => HeadState::Detached {
                oid: crate::status::oid(head_oid).map_err(GitError::InvalidMetadata)?,
            },
            (branch, b"(initial)") => HeadState::Unborn {
                branch: branch.to_vec(),
            },
            (branch, head_oid) => HeadState::Attached {
                branch: branch.to_vec(),
                oid: crate::status::oid(head_oid).map_err(GitError::InvalidMetadata)?,
            },
        };
        Ok(RepositorySnapshot {
            root,
            git_dir,
            common_dir,
            head,
            status,
        })
    }

    async fn read_path(&self, directory: &Path, option: &'static str) -> Result<PathBuf, GitError> {
        let output = self
            .run(directory, "rev-parse", &["--path-format=absolute", option])
            .await?;
        let bytes = output
            .strip_suffix(b"\n")
            .ok_or(GitError::InvalidMetadata("路径缺少行终止符"))?;
        let path = native_path(bytes)?;
        if !path.is_absolute() {
            return Err(GitError::InvalidMetadata("Git 未返回绝对路径"));
        }
        Ok(path.canonicalize()?)
    }

    async fn run(
        &self,
        directory: &Path,
        operation: &'static str,
        arguments: &[&str],
    ) -> Result<Vec<u8>, GitError> {
        let mut command = self.command(directory);
        command.arg(operation).args(arguments);
        let mut child = command.spawn()?;
        let stdout = child
            .stdout
            .take()
            .ok_or(GitError::InvalidMetadata("缺少 stdout 管道"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or(GitError::InvalidMetadata("缺少 stderr 管道"))?;
        let result = tokio::time::timeout(COMMAND_TIMEOUT, async {
            tokio::try_join!(
                async {
                    let mut bytes = Vec::new();
                    stdout
                        .take(OUTPUT_LIMIT + 1)
                        .read_to_end(&mut bytes)
                        .await?;
                    Ok::<_, io::Error>(bytes)
                },
                async {
                    let mut bytes = Vec::new();
                    stderr
                        .take(OUTPUT_LIMIT + 1)
                        .read_to_end(&mut bytes)
                        .await?;
                    Ok::<_, io::Error>(bytes)
                },
                child.wait(),
            )
        })
        .await
        .map_err(|_| GitError::Timeout)??;
        let (stdout, stderr, status) = result;
        if stdout.len() as u64 > OUTPUT_LIMIT || stderr.len() as u64 > OUTPUT_LIMIT {
            return Err(GitError::OutputLimit);
        }
        if !status.success() {
            if operation == "config" && status.code() == Some(1) && stderr.is_empty() {
                return Ok(Vec::new());
            }
            if operation == "rev-parse" && stderr.starts_with(b"fatal: not a git repository") {
                return Err(GitError::NotRepository);
            }
            return Err(GitError::CommandFailed {
                operation,
                code: status.code(),
                stderr,
            });
        }
        Ok(stdout)
    }

    fn command(&self, directory: &Path) -> Command {
        let mut command = Command::new(&self.program);
        command
            .current_dir(directory)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        for (name, _) in std::env::vars_os() {
            if name
                .to_string_lossy()
                .to_ascii_uppercase()
                .starts_with("GIT_")
            {
                command.env_remove(name);
            }
        }
        command
            .env("LC_ALL", "C")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", null_device())
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("GIT_NO_LAZY_FETCH", "1")
            .env("GIT_TERMINAL_PROMPT", "0")
            .args([
                "--no-optional-locks",
                "-c",
                "core.fsmonitor=false",
                "-c",
                "core.untrackedCache=false",
                "-c",
                "core.hooksPath=",
                "-c",
                "maintenance.auto=false",
                "-c",
                "gc.auto=0",
                "-c",
                "status.submoduleSummary=false",
                "-c",
                "core.quotePath=false",
            ]);
        command
    }
}

fn null_device() -> &'static OsStr {
    OsStr::new(if cfg!(windows) { "NUL" } else { "/dev/null" })
}

fn native_path(bytes: &[u8]) -> Result<PathBuf, GitError> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        Ok(PathBuf::from(OsString::from_vec(bytes.to_vec())))
    }
    #[cfg(not(unix))]
    {
        let text = std::str::from_utf8(bytes)
            .map_err(|_| GitError::UnsupportedPathEncoding(bytes.to_vec()))?;
        Ok(PathBuf::from(OsString::from(text)))
    }
}
