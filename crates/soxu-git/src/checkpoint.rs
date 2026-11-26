//! 轮次 Checkpoint：私有 index、覆盖清单及可恢复的私有 ref 发布。

mod capture;
mod git;
mod platform;
mod record;

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::{CheckpointSummary, GitError, GitProbe};

const MAX_ENUMERATED_PATHS: usize = 40_000;

/// 每次会话由应用生成，调用者不能提供完整 ref 或任意 ref 段。
#[derive(Debug, Clone)]
pub struct CheckpointScope {
    workspace: String,
    session: String,
}

impl Default for CheckpointScope {
    fn default() -> Self {
        Self::new()
    }
}

impl CheckpointScope {
    #[must_use]
    pub fn new() -> Self {
        Self {
            workspace: git::id(),
            session: git::id(),
        }
    }
    #[must_use]
    pub fn new_session(&self) -> Self {
        Self {
            workspace: self.workspace.clone(),
            session: git::id(),
        }
    }
    #[must_use]
    pub fn workspace_id(&self) -> &str {
        &self.workspace
    }
    #[must_use]
    pub fn session_id(&self) -> &str {
        &self.session
    }
}

/// 必须指向工作树及 Git 元数据之外、由应用独占管理的既有目录。
/// Windows 的 ACL 所有权由宿主保障；模块持有无 FILE_SHARE_DELETE 的目录链。
#[derive(Debug, Clone)]
pub struct CheckpointOptions {
    pub state_directory: PathBuf,
    pub scope: CheckpointScope,
    pub turn_index: u32,
    pub policy: CheckpointPolicy,
}

#[derive(Debug, Clone)]
pub struct CheckpointPolicy {
    pub warning_bytes: u64,
    pub max_text_bytes: u64,
    pub max_binary_bytes: u64,
    pub max_total_bytes: u64,
    pub max_paths: usize,
    pub max_output_bytes: u64,
    pub command_timeout: Duration,
    pub total_timeout: Duration,
    pub allow_binary: bool,
}

impl Default for CheckpointPolicy {
    fn default() -> Self {
        Self {
            warning_bytes: 1024 * 1024,
            max_text_bytes: 8 * 1024 * 1024,
            max_binary_bytes: 32 * 1024 * 1024,
            max_total_bytes: 64 * 1024 * 1024,
            max_paths: 10_000,
            max_output_bytes: 16 * 1024 * 1024,
            command_timeout: Duration::from_secs(15),
            total_timeout: Duration::from_secs(120),
            allow_binary: false,
        }
    }
}

/// 路径始终为 Git 原始字节，不以有损 UTF-8 替代 Unix 文件名。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoveredPath {
    pub path: Vec<u8>,
    pub blob_oid: Option<String>,
    pub bytes: u64,
    pub warning: bool,
    /// raw-blob / deleted；不声称执行了 Git 的编码或 clean 转换。
    pub representation: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OmissionReason {
    Ignored,
    SensitivePath,
    SensitiveContent,
    TextBudget,
    BinaryNotAuthorized,
    BinaryBudget,
    TotalBudget,
    PathBudget,
    SpecialFile,
    NestedRepository,
    UnsupportedFilter,
    UnsupportedEncoding,
    InheritedSpecial,
    IndividualBudget,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Omission {
    pub path: Vec<u8>,
    pub reason: OmissionReason,
    pub retains_base_entry: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoverageManifest {
    pub complete: bool,
    pub included: Vec<CoveredPath>,
    pub omissions: Vec<Omission>,
    pub captured_bytes: u64,
    pub policy_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointObservation {
    pub worktree_root: PathBuf,
    pub git_dir: PathBuf,
    pub common_dir: PathBuf,
    pub directory_identity: Vec<String>,
    pub head: Vec<u8>,
    pub index_digest: Option<String>,
    pub configuration_digest: String,
    pub platform: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheckpointLifecycle {
    Intent,
    Published,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointRecord {
    pub schema_version: u32,
    pub workspace_id: String,
    pub session_id: String,
    pub checkpoint_id: String,
    pub turn_index: u32,
    pub parent_oid: Option<String>,
    pub base_tree_oid: String,
    pub worktree_tree_oid: String,
    pub commit_oid: String,
    pub changed_files: u32,
    pub before: CheckpointObservation,
    pub after: CheckpointObservation,
    pub coverage: CoverageManifest,
    pub created_at: String,
    pub lifecycle: CheckpointLifecycle,
}

impl CheckpointRecord {
    #[must_use]
    pub fn summary(&self) -> CheckpointSummary {
        CheckpointSummary {
            checkpoint_id: self.checkpoint_id.clone(),
            turn_index: self.turn_index,
            commit_oid: self.commit_oid.clone(),
            changed_files: self.changed_files,
        }
    }
    #[must_use]
    pub fn reference(&self) -> String {
        format!(
            "refs/soxu/checkpoints/{}/{}/{}",
            self.workspace_id, self.session_id, self.checkpoint_id
        )
    }
}

/// 观测钩子仅用于宿主同步/测试；不得在这里修改应用自己的发布目录。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckpointStage {
    Captured,
    Revalidated,
    IntentDurable,
}

#[derive(Debug, thiserror::Error)]
pub enum CheckpointError {
    #[error("平台缺少稳定句柄锚定能力，Checkpoint 写能力已关闭")]
    UnsupportedPlatform,
    #[error("工作树或捕获输入发生漂移，请重试")]
    Drift,
    #[error("无法安全捕获：{0}")]
    Invalid(&'static str),
    #[error("资源预算耗尽：{0}")]
    Budget(&'static str),
    #[error("Checkpoint 命令或整体操作超时")]
    Timeout,
    #[error("工作树已有 Soxu 写操作")]
    Busy,
    #[error("私有 ref 的旧值 CAS 冲突")]
    CasConflict,
    #[error("Git 退出码 {code:?}: {stderr:?}")]
    Git { code: Option<i32>, stderr: Vec<u8> },
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Probe(#[from] GitError),
}

impl GitProbe {
    pub async fn capture_checkpoint(
        &self,
        directory: impl AsRef<std::path::Path>,
        options: &CheckpointOptions,
    ) -> Result<CheckpointRecord, CheckpointError> {
        self.capture_checkpoint_observed(directory, options, |_| {})
            .await
    }

    pub async fn capture_checkpoint_observed(
        &self,
        directory: impl AsRef<std::path::Path>,
        options: &CheckpointOptions,
        mut observe: impl FnMut(CheckpointStage),
    ) -> Result<CheckpointRecord, CheckpointError> {
        capture::capture(self, directory.as_ref(), options, &mut observe).await
    }

    /// 只读核验持久 Intent/记录；不因发现缺口而移动 HEAD 或覆盖已有 ref。
    pub async fn inspect_checkpoint(
        &self,
        directory: impl AsRef<std::path::Path>,
        operation_directory: impl AsRef<std::path::Path>,
    ) -> Result<CheckpointRecord, CheckpointError> {
        record::inspect(self, directory.as_ref(), operation_directory.as_ref()).await
    }
}
