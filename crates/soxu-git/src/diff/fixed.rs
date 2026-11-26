//! 固定端点只读 Diff：先校验授权，再固定完整 OID；遗漏绝不等同于零差异。
//!
//! 本模块不扩展既有四个 Diff 基准，也不接受任意 ref 字符串：端点只能来自服务端
//! 已发布的 Checkpoint 目录记录，或完整的 40/64 位本地公开历史提交。

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::format::Tracked;
use super::repository::{self, Layout};
use super::{
    DiffError, DiffFileMode, DiffOutput, DiffReview, MAX_PATH_BYTES, MAX_PATHS,
    REQUEST_OUTPUT_LIMIT, REQUEST_TIMEOUT, Runner,
};
use crate::{
    CheckpointError, CheckpointLifecycle, CheckpointObservation, CheckpointRecord, CheckpointScope,
    CoverageManifest, GitProbe, WorktreeIdentity,
};

mod parse;

/// 单次文本审阅上限：超过即记录预算命中，不返回半份补丁。
const MAX_REVIEW_BYTES: usize = 1024 * 1024;
/// Checkpoint 标识只能落在状态目录的单一层级内。
const MAX_CHECKPOINT_ID: usize = 128;
/// Git NUL 记录中的换行字节；显式写成十进制以免与转义序列混淆。
const NEWLINE: u8 = 10;
/// 与统一 Runner 的累计输出预算错误保持同一标识。
const OUTPUT_LIMIT: &str = "请求累计输出";

/// 仅服务端授权的目录记录或完整的本地公开历史提交可作为固定端点。
#[derive(Debug, Clone)]
pub enum FixedDiffEndpoint<'a> {
    /// 记录、私有 ref、对象与工作树归属必须逐项与服务端已发布的 Checkpoint 一致。
    AuthorizedCheckpoint {
        state_directory: &'a Path,
        scope: &'a CheckpointScope,
        checkpoint_id: &'a str,
    },
    /// 完整 40/64 位十六进制提交 OID；缩写、ref 名称与私有 refs/soxu 提交一律拒绝。
    AuthorizedBaseCommit(&'a str),
}

/// paths 为空时在两端已跟踪集合内枚举；非空时精确限定本次审阅范围。
#[derive(Debug, Clone)]
pub struct FixedDiffRequest<'a> {
    pub base: FixedDiffEndpoint<'a>,
    pub head: FixedDiffEndpoint<'a>,
    pub paths: &'a [&'a [u8]],
    pub output: DiffOutput<'a>,
}

/// 端点解析身份；工作树端没有固定提交，不得伪造历史身份。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixedDiffIdentity {
    pub worktree: WorktreeIdentity,
    pub workspace_id: Option<String>,
    pub session_id: Option<String>,
    pub checkpoint_id: Option<String>,
    pub commit_oid: Option<String>,
    pub tree_oid: Option<String>,
    /// true 表示该端是可变观察，内容仍可能在查询之外变化。
    pub mutable: bool,
    pub observed_at: SystemTime,
}

/// 单端覆盖汇总：Checkpoint 保留原始覆盖清单，工作树端记录未纳入比较的路径。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixedDiffCoverage {
    pub checkpoint: Option<CoverageManifest>,
    /// 未跟踪与被忽略路径，不读取内容也不进入比较。
    pub untracked_paths: Vec<Vec<u8>>,
}

/// 结果未覆盖全仓库的具体原因；任一命中都禁止标注为完整零差异。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixedDiffIncomplete {
    /// 基准端 Checkpoint 捕获时就存在遗漏。
    BaseOmissions,
    /// 对端 Checkpoint 捕获时存在遗漏。
    HeadOmissions,
    /// 调用方只授权了部分路径，结果不代表全仓库。
    PathSelection,
    /// 路径数或路径字节数触及本次请求预算。
    PathBudget,
    /// Git 输出触及请求累计预算，已放弃半成品记录。
    OutputBudget,
    /// 文本审阅触及单次补丁预算。
    ReviewBudget,
    /// 工作树端存在未跟踪或被忽略路径。
    UntrackedWorktreePaths,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixedDiffKind {
    Added,
    Deleted,
    Modified,
    Renamed,
}

/// 单文件变更；行统计缺失表示该侧为二进制内容。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixedDiffChange {
    pub path: Vec<u8>,
    pub old_path: Option<Vec<u8>>,
    pub kind: FixedDiffKind,
    pub old_mode: Option<DiffFileMode>,
    pub new_mode: Option<DiffFileMode>,
    pub old_oid: Option<String>,
    pub new_oid: Option<String>,
    pub binary: bool,
    pub additions: Option<u64>,
    pub deletions: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixedDiffResult {
    pub base: FixedDiffIdentity,
    pub head: FixedDiffIdentity,
    pub base_coverage: FixedDiffCoverage,
    pub head_coverage: FixedDiffCoverage,
    pub selected_paths: Vec<Vec<u8>>,
    pub changes: Vec<FixedDiffChange>,
    pub review: Option<DiffReview>,
    /// 空 changes 不能覆盖遗漏或预算命中这些事实。
    pub incomplete_reason: Vec<FixedDiffIncomplete>,
}

impl FixedDiffResult {
    /// 仅当两端都无遗漏且未命中任何预算时才为 true。
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.incomplete_reason.is_empty()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum FixedDiffError {
    #[error("固定端点只接受完整的 40/64 位提交 OID")]
    InvalidCommit,
    #[error("提交不在本地分支、标签、远端跟踪分支或 HEAD 可达历史中")]
    UnreachableCommit,
    #[error("Checkpoint 未发布，或工作树、会话、目录记录/私有 ref 不匹配")]
    UnauthorizedCheckpoint,
    #[error(transparent)]
    Checkpoint(#[from] CheckpointError),
    #[error(transparent)]
    Diff(#[from] DiffError),
}

impl From<std::io::Error> for FixedDiffError {
    fn from(error: std::io::Error) -> Self {
        Self::Diff(error.into())
    }
}

/// 已解析、固定并将在比较后二次核验的端点。
struct Resolved {
    identity: FixedDiffIdentity,
    coverage: FixedDiffCoverage,
    record: Option<CheckpointRecord>,
}

impl Resolved {
    fn worktree(identity: &WorktreeIdentity) -> Self {
        Self {
            identity: FixedDiffIdentity {
                worktree: identity.clone(),
                workspace_id: None,
                session_id: None,
                checkpoint_id: None,
                commit_oid: None,
                tree_oid: None,
                mutable: true,
                observed_at: SystemTime::now(),
            },
            coverage: FixedDiffCoverage {
                checkpoint: None,
                untracked_paths: Vec::new(),
            },
            record: None,
        }
    }
}

impl GitProbe {
    /// 比较两个固定提交树，不依赖当前 index 与工作树内容。
    ///
    /// # Errors
    /// 端点未获授权、对象不可达、工作树身份不符或请求预算耗尽时返回错误。
    pub async fn readonly_fixed_diff(
        &self,
        worktree_root: impl AsRef<Path>,
        request: FixedDiffRequest<'_>,
    ) -> Result<FixedDiffResult, FixedDiffError> {
        self.fixed_compare(
            worktree_root.as_ref(),
            &request.base,
            Some(&request.head),
            request.paths,
            request.output,
        )
        .await
    }

    /// 已授权 Checkpoint 到当前工作树的只读比较。
    ///
    /// 结果中 head 只有可变观察身份；工作树端的未跟踪路径会作为遗漏记录返回。
    ///
    /// # Errors
    /// 端点不是已授权 Checkpoint、工作树身份不符或请求预算耗尽时返回错误。
    pub async fn readonly_checkpoint_diff(
        &self,
        worktree_root: impl AsRef<Path>,
        checkpoint: FixedDiffEndpoint<'_>,
        paths: &[&[u8]],
        output: DiffOutput<'_>,
    ) -> Result<FixedDiffResult, FixedDiffError> {
        if !matches!(checkpoint, FixedDiffEndpoint::AuthorizedCheckpoint { .. }) {
            return Err(FixedDiffError::UnauthorizedCheckpoint);
        }
        self.fixed_compare(worktree_root.as_ref(), &checkpoint, None, paths, output)
            .await
    }

    async fn fixed_compare(
        &self,
        root: &Path,
        base_endpoint: &FixedDiffEndpoint<'_>,
        head_endpoint: Option<&FixedDiffEndpoint<'_>>,
        requested: &[&[u8]],
        output: DiffOutput<'_>,
    ) -> Result<FixedDiffResult, FixedDiffError> {
        if !root.is_absolute() {
            return Err(DiffError::InvalidWorktree.into());
        }
        let root = root.canonicalize()?;
        let mut runner = Runner {
            git: self,
            root: &root,
            deadline: tokio::time::Instant::now() + REQUEST_TIMEOUT,
            remaining: REQUEST_OUTPUT_LIMIT,
        };
        let layout = Layout::read(&root)?;
        layout.preflight_format(&mut runner, true).await?;
        let base_resolved = resolve(&mut runner, base_endpoint, &layout.identity).await?;
        let head_resolved = match head_endpoint {
            Some(endpoint) => resolve(&mut runner, endpoint, &layout.identity).await?,
            None => Resolved::worktree(&layout.identity),
        };
        let mut result = FixedDiffResult {
            base: base_resolved.identity.clone(),
            head: head_resolved.identity.clone(),
            base_coverage: base_resolved.coverage.clone(),
            head_coverage: head_resolved.coverage.clone(),
            selected_paths: Vec::new(),
            changes: Vec::new(),
            review: None,
            incomplete_reason: Vec::new(),
        };
        for (coverage, reason) in [
            (&base_resolved.coverage, FixedDiffIncomplete::BaseOmissions),
            (&head_resolved.coverage, FixedDiffIncomplete::HeadOmissions),
        ] {
            if coverage
                .checkpoint
                .as_ref()
                .is_some_and(|manifest| !manifest.complete || !manifest.omissions.is_empty())
            {
                push_once(&mut result.incomplete_reason, reason);
            }
        }
        match compare(&mut runner, &mut result, requested, output).await {
            Err(DiffError::Limit(reason)) if reason == OUTPUT_LIMIT => {
                // 预算耗尽时保留已固定的身份与覆盖事实，绝不返回半条 Git 记录。
                result.changes.clear();
                result.review = None;
                push_once(
                    &mut result.incomplete_reason,
                    FixedDiffIncomplete::OutputBudget,
                );
            }
            Err(error) => return Err(error.into()),
            Ok(()) => {}
        }
        if Layout::read(&root)? != layout {
            return Err(DiffError::Changed.into());
        }
        revalidate(&mut runner, base_endpoint, &base_resolved, &layout.identity).await?;
        if let Some(endpoint) = head_endpoint {
            revalidate(&mut runner, endpoint, &head_resolved, &layout.identity).await?;
        }
        runner.check_deadline()?;
        Ok(result)
    }
}

fn push_once(reasons: &mut Vec<FixedDiffIncomplete>, reason: FixedDiffIncomplete) {
    if !reasons.contains(&reason) {
        reasons.push(reason);
    }
}

async fn revalidate(
    runner: &mut Runner<'_>,
    endpoint: &FixedDiffEndpoint<'_>,
    before: &Resolved,
    identity: &WorktreeIdentity,
) -> Result<(), FixedDiffError> {
    let after = resolve(runner, endpoint, identity).await?;
    if before.record != after.record
        || before.identity.commit_oid != after.identity.commit_oid
        || before.identity.tree_oid != after.identity.tree_oid
    {
        return Err(DiffError::Changed.into());
    }
    Ok(())
}

async fn resolve(
    runner: &mut Runner<'_>,
    endpoint: &FixedDiffEndpoint<'_>,
    identity: &WorktreeIdentity,
) -> Result<Resolved, FixedDiffError> {
    let (commit, record) = match endpoint {
        FixedDiffEndpoint::AuthorizedBaseCommit(oid) => (public_commit(runner, oid).await?, None),
        FixedDiffEndpoint::AuthorizedCheckpoint {
            state_directory,
            scope,
            checkpoint_id,
        } => {
            let record =
                published_record(runner, state_directory, scope, checkpoint_id, identity).await?;
            (record.commit_oid.clone(), Some(record))
        }
    };
    let tree_query = format!("{commit}^{{tree}}");
    let tree = runner
        .read(
            "rev-parse",
            ["--verify", "--end-of-options", tree_query.as_str()],
        )
        .await?;
    let tree = parse::oid_line(&tree)?;
    if record
        .as_ref()
        .is_some_and(|record| record.worktree_tree_oid != tree)
    {
        return Err(FixedDiffError::UnauthorizedCheckpoint);
    }
    // 校验对象真实存在；统一 Runner 已禁止懒拉取与 replace 对象。
    let object_query = format!("{tree}^{{tree}}");
    runner
        .read("cat-file", ["-e", object_query.as_str()])
        .await?;
    Ok(Resolved {
        identity: FixedDiffIdentity {
            worktree: identity.clone(),
            workspace_id: record.as_ref().map(|record| record.workspace_id.clone()),
            session_id: record.as_ref().map(|record| record.session_id.clone()),
            checkpoint_id: record.as_ref().map(|record| record.checkpoint_id.clone()),
            commit_oid: Some(commit),
            tree_oid: Some(tree),
            mutable: false,
            observed_at: SystemTime::now(),
        },
        coverage: FixedDiffCoverage {
            checkpoint: record.as_ref().map(|record| record.coverage.clone()),
            untracked_paths: Vec::new(),
        },
        record,
    })
}

/// 只接受完整的 40/64 位 OID，且必须可从本地公开 ref 或 HEAD 到达。
async fn public_commit(runner: &mut Runner<'_>, oid: &str) -> Result<String, FixedDiffError> {
    let commit = commit_object(runner, oid).await?;
    let mut tips = vec!["--branches", "--tags", "--remotes"];
    let head = runner
        .run("rev-parse", ["--verify", "--quiet", "HEAD^{commit}"])
        .await?;
    if head.status.success() {
        tips.push("HEAD");
    }
    let reachable = runner.read("rev-list", tips).await?;
    if !reachable
        .split(|byte| *byte == NEWLINE)
        .any(|line| line == commit.as_bytes())
    {
        return Err(FixedDiffError::UnreachableCommit);
    }
    Ok(commit)
}

async fn commit_object(runner: &mut Runner<'_>, oid: &str) -> Result<String, FixedDiffError> {
    if !parse::is_oid(oid.as_bytes()) {
        return Err(FixedDiffError::InvalidCommit);
    }
    let query = format!("{oid}^{{commit}}");
    let resolved = runner
        .read(
            "rev-parse",
            ["--verify", "--end-of-options", query.as_str()],
        )
        .await?;
    let resolved = parse::oid_line(&resolved)?;
    if resolved != oid.to_ascii_lowercase() {
        return Err(FixedDiffError::InvalidCommit);
    }
    let object_query = format!("{resolved}^{{commit}}");
    runner
        .read("cat-file", ["-e", object_query.as_str()])
        .await?;
    Ok(resolved)
}

async fn published_record(
    runner: &mut Runner<'_>,
    state_directory: &Path,
    scope: &CheckpointScope,
    checkpoint_id: &str,
    identity: &WorktreeIdentity,
) -> Result<CheckpointRecord, FixedDiffError> {
    if checkpoint_id.is_empty()
        || checkpoint_id.len() > MAX_CHECKPOINT_ID
        || !checkpoint_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        || !state_directory.is_absolute()
    {
        return Err(FixedDiffError::UnauthorizedCheckpoint);
    }
    let state = state_directory.canonicalize()?;
    let operation = state.join(checkpoint_id);
    let Ok(operation) = operation.canonicalize() else {
        return Err(FixedDiffError::UnauthorizedCheckpoint);
    };
    if operation.parent() != Some(state.as_path()) {
        return Err(FixedDiffError::UnauthorizedCheckpoint);
    }
    let timeout = runner.check_deadline()?;
    let record = tokio::time::timeout(
        timeout,
        runner.git.inspect_checkpoint(runner.root, &operation),
    )
    .await
    .map_err(|_| DiffError::Limit("请求耗时"))??;
    if record.lifecycle != CheckpointLifecycle::Published
        || record.workspace_id != scope.workspace_id()
        || record.session_id != scope.session_id()
        || record.checkpoint_id != checkpoint_id
        || !same_worktree(&record.before, identity)
        || !same_worktree(&record.after, identity)
        || record.before.directory_identity != record.after.directory_identity
    {
        return Err(FixedDiffError::UnauthorizedCheckpoint);
    }
    let reference = record.reference();
    let symbolic = runner
        .run("symbolic-ref", ["-q", reference.as_str()])
        .await?;
    if symbolic.status.code() != Some(1) {
        return Err(FixedDiffError::UnauthorizedCheckpoint);
    }
    let actual = runner
        .read(
            "rev-parse",
            ["--verify", "--end-of-options", reference.as_str()],
        )
        .await?;
    if !parse::oid_line(&actual)?.eq_ignore_ascii_case(&record.commit_oid) {
        return Err(FixedDiffError::UnauthorizedCheckpoint);
    }
    let commit = commit_object(runner, &record.commit_oid).await?;
    if !commit.eq_ignore_ascii_case(&record.commit_oid) {
        return Err(FixedDiffError::UnauthorizedCheckpoint);
    }
    Ok(record)
}

fn same_worktree(observation: &CheckpointObservation, identity: &WorktreeIdentity) -> bool {
    observation.worktree_root == identity.root
        && observation.git_dir == identity.git_dir
        && observation.common_dir == identity.common_dir
}

async fn compare(
    runner: &mut Runner<'_>,
    result: &mut FixedDiffResult,
    requested: &[&[u8]],
    output: DiffOutput<'_>,
) -> Result<(), DiffError> {
    let base = result
        .base
        .commit_oid
        .clone()
        .ok_or(DiffError::InvalidBaseRef)?;
    let head = result.head.commit_oid.clone();
    let base_bytes = runner
        .read("ls-tree", ["-r", "-z", "--full-tree", base.as_str()])
        .await?;
    let base_paths = parse::tree_paths(&base_bytes)?;
    let head_paths = match &head {
        Some(head) => {
            let bytes = runner
                .read("ls-tree", ["-r", "-z", "--full-tree", head.as_str()])
                .await?;
            parse::tree_paths(&bytes)?
        }
        None => {
            let bytes = runner.read("ls-files", ["--stage", "-z"]).await?;
            parse::index_paths(&bytes)?
        }
    };
    let mut available: BTreeSet<Vec<u8>> = base_paths
        .keys()
        .chain(head_paths.keys())
        .cloned()
        .collect();
    if head.is_none() {
        available = worktree_omissions(runner, available, result).await?;
    }
    let candidates = selection(requested, &available, result)?;
    let mut selected: BTreeMap<&[u8], PathBuf> = BTreeMap::new();
    let mut bytes = 0usize;
    for path in &candidates {
        if selected.len() >= MAX_PATHS || bytes.saturating_add(path.len()) > MAX_PATH_BYTES {
            push_once(
                &mut result.incomplete_reason,
                FixedDiffIncomplete::PathBudget,
            );
            break;
        }
        let native = repository::native_file_path(path)?;
        if head.is_some() {
            check_transition(&base_paths, &head_paths, path)?;
        }
        bytes += path.len();
        selected.insert(path.as_slice(), native);
    }
    result.selected_paths = selected.keys().map(|path| (*path).to_vec()).collect();
    let review_path = match output {
        DiffOutput::Metadata => None,
        DiffOutput::Review(path) => Some(path),
    };
    if let Some(path) = review_path
        && !selected.contains_key(path)
    {
        return Err(DiffError::UnauthorizedReview);
    }
    if selected.is_empty() {
        return Ok(());
    }
    let stamps = match head {
        Some(_) => None,
        None => Some(repository::authorize(
            runner.root,
            &selected,
            &head_paths,
            &base_paths,
        )?),
    };
    let options = ["--raw", "--numstat", "-z"];
    let raw = runner
        .read(
            "diff",
            diff_arguments(&base, head.as_deref(), &options, selected.values()),
        )
        .await?;
    result.changes = parse::changes(&raw, &selected, base.len(), head.is_none())?;
    if let Some(path) = review_path {
        result.review =
            review_change(runner, &base, head.as_deref(), &result.changes, path).await?;
        if result.review.is_none() {
            push_once(
                &mut result.incomplete_reason,
                FixedDiffIncomplete::ReviewBudget,
            );
        }
    }
    if let Some(stamps) = stamps
        && repository::authorize(runner.root, &selected, &head_paths, &base_paths)? != stamps
    {
        return Err(DiffError::Changed);
    }
    Ok(())
}

/// 工作树端未跟踪/被忽略路径显式记为遗漏，且不参与比较。
async fn worktree_omissions(
    runner: &mut Runner<'_>,
    mut available: BTreeSet<Vec<u8>>,
    result: &mut FixedDiffResult,
) -> Result<BTreeSet<Vec<u8>>, DiffError> {
    let mut listed = runner
        .read("ls-files", ["--others", "--exclude-standard", "-z"])
        .await?;
    listed.extend(
        runner
            .read(
                "ls-files",
                ["--others", "--ignored", "--exclude-standard", "-z"],
            )
            .await?,
    );
    let omissions = parse::paths(&listed)?;
    if !omissions.is_empty() {
        for path in &omissions {
            available.remove(path);
        }
        result.head_coverage.untracked_paths = omissions;
        push_once(
            &mut result.incomplete_reason,
            FixedDiffIncomplete::UntrackedWorktreePaths,
        );
    }
    Ok(available)
}

fn selection(
    requested: &[&[u8]],
    available: &BTreeSet<Vec<u8>>,
    result: &mut FixedDiffResult,
) -> Result<BTreeSet<Vec<u8>>, DiffError> {
    if requested.is_empty() {
        return Ok(available.clone());
    }
    let mut candidates = BTreeSet::new();
    for path in requested {
        repository::native_file_path(path)?;
        if !candidates.insert(path.to_vec()) {
            return Err(DiffError::InvalidPath);
        }
        if !available.contains(*path) {
            return Err(DiffError::UntrackedPath);
        }
    }
    if candidates != *available {
        push_once(
            &mut result.incomplete_reason,
            FixedDiffIncomplete::PathSelection,
        );
    }
    Ok(candidates)
}

/// 固定端点之间不需要工作树戳记，但仍需拒绝符号链接与文件/目录类型变化。
fn check_transition(base: &Tracked, head: &Tracked, path: &[u8]) -> Result<(), DiffError> {
    let mut prefix = path.to_vec();
    prefix.push(b'/');
    for tracked in [base, head] {
        if tracked.get(path).is_some_and(Option::is_none) {
            return Err(DiffError::Unsupported("Git 符号链接条目"));
        }
        if tracked
            .range(prefix.clone()..)
            .next()
            .is_some_and(|(name, _)| name.starts_with(&prefix))
            || path
                .iter()
                .enumerate()
                .any(|(offset, byte)| *byte == b'/' && tracked.contains_key(&path[..offset]))
        {
            return Err(DiffError::Unsupported("端点间文件与目录类型变化"));
        }
    }
    Ok(())
}

fn diff_arguments<'a>(
    base: &str,
    head: Option<&str>,
    options: &[&str],
    paths: impl IntoIterator<Item = &'a PathBuf>,
) -> Vec<OsString> {
    let mut arguments: Vec<OsString> = [
        "--no-ext-diff",
        "--no-textconv",
        "--ignore-submodules=all",
        "--no-color",
        "--find-renames=50%",
        "--no-relative",
        "--default-prefix",
        "--diff-algorithm=myers",
        "--no-indent-heuristic",
        "--full-index",
        "--no-abbrev",
    ]
    .into_iter()
    .map(OsString::from)
    .collect();
    arguments.extend(options.iter().map(OsString::from));
    arguments.push(base.into());
    if let Some(head) = head {
        arguments.push(head.into());
    }
    arguments.push("--".into());
    arguments.extend(paths.into_iter().map(|path| path.as_os_str().to_owned()));
    arguments
}

async fn review_change(
    runner: &mut Runner<'_>,
    base: &str,
    head: Option<&str>,
    changes: &[FixedDiffChange],
    path: &[u8],
) -> Result<Option<DiffReview>, DiffError> {
    let Some(change) = changes
        .iter()
        .find(|change| change.path == path || change.old_path.as_deref() == Some(path))
    else {
        return Ok(Some(DiffReview::Unchanged));
    };
    if change.binary {
        return Ok(Some(DiffReview::Binary));
    }
    let mut paths = Vec::new();
    if let Some(old) = &change.old_path {
        paths.push(repository::native_file_path(old)?);
    }
    paths.push(repository::native_file_path(&change.path)?);
    let options = ["--patch", "--unified=3"];
    let patch = runner
        .read("diff", diff_arguments(base, head, &options, paths.iter()))
        .await?;
    if patch.is_empty() || patch.contains(&0) {
        return Err(DiffError::Changed);
    }
    if patch.len() > MAX_REVIEW_BYTES {
        return Ok(None);
    }
    Ok(Some(DiffReview::Text(patch)))
}
