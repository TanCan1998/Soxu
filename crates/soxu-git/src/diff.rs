use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Output;
use std::time::{Duration, SystemTime};

use tokio::process::Command;
use tokio::time::Instant;

use crate::probe::{COMMAND_TIMEOUT, OUTPUT_LIMIT, null_device, run_bounded};
use crate::{GitError, GitProbe, WorktreeIdentity};

mod fixed;
mod format;
mod repository;

pub use fixed::{
    FixedDiffChange, FixedDiffCoverage, FixedDiffEndpoint, FixedDiffError, FixedDiffIdentity,
    FixedDiffIncomplete, FixedDiffKind, FixedDiffRequest, FixedDiffResult,
};

const MAX_PATHS: usize = 128;
const MAX_PATH_BYTES: usize = 8 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(45);
const REQUEST_OUTPUT_LIMIT: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffBaseline<'a> {
    Unstaged,
    Staged,
    Uncommitted,
    BaseToWorktree(&'a str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffOutput<'a> {
    Metadata,
    Review(&'a [u8]),
}

#[derive(Debug, Clone)]
pub struct DiffRequest<'a> {
    pub baseline: DiffBaseline<'a>,
    pub paths: &'a [&'a [u8]],
    pub output: DiffOutput<'a>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffFileMode {
    Regular,
    Executable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffKind {
    Added,
    Deleted,
    Modified,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffChange {
    pub path: Vec<u8>,
    pub kind: DiffKind,
    pub old_mode: Option<DiffFileMode>,
    pub new_mode: Option<DiffFileMode>,
    pub old_oid: Option<String>,
    pub new_oid: Option<String>,
    pub binary: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffReview {
    Unchanged,
    Binary,
    Text(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffObservation {
    pub worktree: WorktreeIdentity,
    pub requested_base_ref: Option<String>,
    pub resolved_commit_oid: Option<String>,
    pub observed_at: SystemTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DiffResult {
    pub observation: Option<DiffObservation>,
    pub changes: Vec<DiffChange>,
    pub review: Option<DiffReview>,
}

#[derive(Debug, thiserror::Error)]
pub enum DiffError {
    #[error("只读 Diff 需要显式的绝对工作树根")]
    InvalidWorktree,
    #[error("授权路径不是受支持的精确仓库相对文件路径")]
    InvalidPath,
    #[error("路径不属于所选端点的已跟踪文件集合")]
    UntrackedPath,
    #[error("审阅路径必须包含在本次精确授权中")]
    UnauthorizedReview,
    #[error("只读 Diff 不支持此条件：{0}")]
    Unsupported(&'static str),
    #[error("baseRef 必须是 HEAD、完整提交 OID 或完整本地 ref")]
    InvalidBaseRef,
    #[error("当前工作树没有 HEAD 提交")]
    UnbornHead,
    #[error("只读 Diff 观察期间检测到变化，需要重新查询")]
    Changed,
    #[error("只读 Diff 超过请求限制：{0}")]
    Limit(&'static str),
    #[error(transparent)]
    Git(#[from] GitError),
}

impl From<std::io::Error> for DiffError {
    fn from(error: std::io::Error) -> Self {
        Self::Git(error.into())
    }
}

struct Runner<'a> {
    git: &'a GitProbe,
    root: &'a Path,
    deadline: Instant,
    remaining: u64,
}

impl Runner<'_> {
    fn check_deadline(&self) -> Result<Duration, DiffError> {
        self.deadline
            .checked_duration_since(Instant::now())
            .ok_or(DiffError::Limit("请求耗时"))
    }

    async fn run(
        &mut self,
        operation: &'static str,
        arguments: impl IntoIterator<Item = impl AsRef<OsStr>>,
    ) -> Result<Output, DiffError> {
        let mut attributes = OsString::from("core.attributesFile=");
        attributes.push(null_device());
        let mut command = self.git.command(self.root);
        command
            .env("GIT_ATTR_NOSYSTEM", "1")
            .args([
                "--no-pager",
                "--literal-pathspecs",
                "--no-replace-objects",
                "-c",
                "diff.autoRefreshIndex=false",
                "-c",
                "core.safecrlf=false",
                "-c",
                "core.bigFileThreshold=16m",
                "-c",
            ])
            .arg(attributes)
            .arg(operation)
            .args(arguments);
        self.collect(&mut command).await
    }

    async fn config(&mut self, path: &Path) -> Result<Vec<u8>, DiffError> {
        let volume_root = self
            .root
            .ancestors()
            .last()
            .ok_or(DiffError::InvalidWorktree)?;
        let path = repository::git_metadata_arg(path)?;
        let mut command = self.git.command(volume_root);
        command
            .env("GIT_DIR", path.join("no-repository"))
            .args(["--no-pager", "config", "--file"])
            .arg(&path)
            .args(["--no-includes", "--null", "--list"]);
        readable(self.collect(&mut command).await?, "config")
    }

    async fn collect(&mut self, command: &mut Command) -> Result<Output, DiffError> {
        let timeout = self.check_deadline()?.min(COMMAND_TIMEOUT);
        let limit = self.remaining.min(OUTPUT_LIMIT);
        let output = run_bounded(command, limit, timeout)
            .await
            .map_err(|error| match error {
                GitError::OutputLimit if limit < OUTPUT_LIMIT => DiffError::Limit("请求累计输出"),
                GitError::Timeout if timeout < COMMAND_TIMEOUT => DiffError::Limit("请求耗时"),
                error => error.into(),
            })?;
        self.remaining = self
            .remaining
            .checked_sub((output.stdout.len() + output.stderr.len()) as u64)
            .ok_or(DiffError::Limit("请求累计输出"))?;
        Ok(output)
    }

    async fn read(
        &mut self,
        operation: &'static str,
        arguments: impl IntoIterator<Item = impl AsRef<OsStr>>,
    ) -> Result<Vec<u8>, DiffError> {
        readable(self.run(operation, arguments).await?, operation)
    }

    async fn diff(
        &mut self,
        endpoints: &Endpoints,
        options: &[&str],
        paths: impl IntoIterator<Item = impl AsRef<OsStr>>,
    ) -> Result<Vec<u8>, DiffError> {
        let flags = [
            "--no-ext-diff",
            "--no-textconv",
            "--ignore-submodules=all",
            "--no-color",
            "--no-renames",
            "--no-relative",
            "--default-prefix",
            "--diff-algorithm=myers",
            "--no-indent-heuristic",
            "--full-index",
            "--no-abbrev",
        ];
        let mut arguments: Vec<OsString> = flags.into_iter().map(OsString::from).collect();
        arguments.extend(options.iter().map(OsString::from));
        if endpoints.cached {
            arguments.push("--cached".into());
        }
        if let Some(oid) = &endpoints.commit {
            arguments.push(oid.into());
        }
        arguments.push("--".into());
        arguments.extend(paths.into_iter().map(|path| path.as_ref().to_owned()));
        self.read("diff", arguments).await
    }
}

fn readable(output: Output, operation: &'static str) -> Result<Vec<u8>, DiffError> {
    if output.status.success() && output.stderr.is_empty() {
        return Ok(output.stdout);
    }
    Err(GitError::CommandFailed {
        operation,
        code: output.status.code(),
        stderr: output.stderr,
    }
    .into())
}

struct Endpoints {
    cached: bool,
    commit: Option<String>,
}

impl GitProbe {
    /// 在受信任且由调用方串行化的工作树内审阅精确的已跟踪文件。
    pub async fn readonly_diff(
        &self,
        worktree_root: impl AsRef<Path>,
        request: DiffRequest<'_>,
    ) -> Result<DiffResult, DiffError> {
        if request.paths.is_empty() {
            return match request.output {
                DiffOutput::Metadata => Ok(DiffResult::default()),
                DiffOutput::Review(_) => Err(DiffError::UnauthorizedReview),
            };
        }
        let paths = selected_paths(request.paths)?;
        let review_path = match request.output {
            DiffOutput::Metadata => None,
            DiffOutput::Review(path) => Some(
                paths
                    .get_key_value(path)
                    .ok_or(DiffError::UnauthorizedReview)?,
            ),
        };
        let root = worktree_root.as_ref();
        if !root.is_absolute() {
            return Err(DiffError::InvalidWorktree);
        }
        let root = root.canonicalize()?;
        let observed_at = SystemTime::now();
        let mut runner = Runner {
            git: self,
            root: &root,
            deadline: Instant::now() + REQUEST_TIMEOUT,
            remaining: REQUEST_OUTPUT_LIMIT,
        };
        let layout = repository::Layout::read(&root)?;
        layout.preflight(&mut runner).await?;
        let endpoints = resolve_endpoints(&mut runner, &request.baseline).await?;
        let index = runner
            .read("ls-files", ["--stage", "--debug", "-z"])
            .await?;
        let index_files = format::index_paths(&index)?;
        let tree = if let Some(oid) = &endpoints.commit {
            let bytes = runner
                .read("ls-tree", ["-r", "-z", "--full-tree", oid])
                .await?;
            format::tree_paths(&bytes)?
        } else {
            Default::default()
        };
        let paths_before = repository::authorize(&root, &paths, &index_files, &tree)?;
        let raw = runner
            .diff(&endpoints, &["--raw", "--numstat", "-z"], paths.values())
            .await?;
        let changes = format::changes(&raw, &paths, !endpoints.cached)?;
        let review = if let Some((bytes, native)) = review_path {
            match changes.iter().find(|entry| entry.path == *bytes) {
                None => Some(DiffReview::Unchanged),
                Some(entry) if entry.binary => Some(DiffReview::Binary),
                Some(_) => {
                    let patch = runner
                        .diff(&endpoints, &["--patch", "--unified=3"], [native])
                        .await?;
                    if patch.is_empty() || patch.contains(&0) {
                        return Err(DiffError::Changed);
                    }
                    Some(DiffReview::Text(patch))
                }
            }
        } else {
            None
        };
        if repository::Layout::read(&root)? != layout
            || repository::authorize(&root, &paths, &index_files, &tree)? != paths_before
            || resolve_endpoints(&mut runner, &request.baseline)
                .await?
                .commit
                != endpoints.commit
        {
            return Err(DiffError::Changed);
        }
        runner.check_deadline()?;
        Ok(DiffResult {
            observation: Some(DiffObservation {
                worktree: layout.identity,
                requested_base_ref: match request.baseline {
                    DiffBaseline::BaseToWorktree(reference) => Some(reference.to_owned()),
                    _ => None,
                },
                resolved_commit_oid: endpoints.commit,
                observed_at,
            }),
            changes,
            review,
        })
    }
}

fn selected_paths<'a>(paths: &'a [&'a [u8]]) -> Result<BTreeMap<&'a [u8], PathBuf>, DiffError> {
    if paths.len() > MAX_PATHS {
        return Err(DiffError::Limit("授权文件数"));
    }
    if paths
        .iter()
        .map(|path| path.len())
        .try_fold(0usize, usize::checked_add)
        .is_none_or(|length| length > MAX_PATH_BYTES)
    {
        return Err(DiffError::Limit("授权路径字节数"));
    }
    let mut selected = BTreeMap::new();
    for &path in paths {
        if selected
            .insert(path, repository::native_file_path(path)?)
            .is_some()
        {
            return Err(DiffError::InvalidPath);
        }
    }
    Ok(selected)
}

async fn resolve_endpoints(
    runner: &mut Runner<'_>,
    baseline: &DiffBaseline<'_>,
) -> Result<Endpoints, DiffError> {
    let commit = match baseline {
        DiffBaseline::Unstaged => None,
        DiffBaseline::Staged | DiffBaseline::Uncommitted => {
            let output = runner
                .run(
                    "rev-parse",
                    ["--verify", "--end-of-options", "HEAD^{commit}"],
                )
                .await?;
            if output.status.success() {
                Some(format::commit_oid(&readable(output, "rev-parse")?)?)
            } else if unborn(runner).await? {
                if *baseline == DiffBaseline::Uncommitted {
                    return Err(DiffError::UnbornHead);
                }
                None
            } else {
                return Err(readable(output, "rev-parse").expect_err("HEAD 解析失败"));
            }
        }
        DiffBaseline::BaseToWorktree(reference) => {
            if reference.len() > 1024 {
                return Err(DiffError::InvalidBaseRef);
            }
            if reference != &"HEAD" && !format::is_oid(reference.as_bytes()) {
                if !["refs/heads/", "refs/tags/", "refs/remotes/"]
                    .iter()
                    .any(|prefix| reference.starts_with(prefix))
                    || reference.bytes().any(|byte| byte.is_ascii_control())
                {
                    return Err(DiffError::InvalidBaseRef);
                }
                let output = runner.run("check-ref-format", [*reference]).await?;
                if !output.status.success()
                    || !output.stdout.is_empty()
                    || !output.stderr.is_empty()
                {
                    return Err(DiffError::InvalidBaseRef);
                }
            }
            let commit = format!("{reference}^{{commit}}");
            let output = runner
                .read("rev-parse", ["--verify", "--end-of-options", &commit])
                .await?;
            Some(format::commit_oid(&output)?)
        }
    };
    Ok(Endpoints {
        cached: *baseline == DiffBaseline::Staged,
        commit,
    })
}

async fn unborn(runner: &mut Runner<'_>) -> Result<bool, DiffError> {
    let output = runner.run("symbolic-ref", ["-q", "HEAD"]).await?;
    if !output.status.success() {
        return Ok(false);
    }
    let name = readable(output, "symbolic-ref")?;
    let name = name
        .strip_suffix(b"\n")
        .filter(|name| name.starts_with(b"refs/heads/") && !name.contains(&0))
        .ok_or(GitError::InvalidMetadata("非法 HEAD 符号引用"))?;
    let name = crate::probe::native_path(name)?;
    let output = runner
        .run("show-ref", [OsStr::new("--exists"), name.as_os_str()])
        .await?;
    Ok(output.status.code() == Some(2))
}
