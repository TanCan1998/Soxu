//! M3 的 Checkpoint、固定端点 Diff、安全 Rewind 与结构化提交契约。
//!
//! 路径字段只携带仓库相对路径。摘要、风险和错误说明须由服务端脱敏，
//! 不得包含本机绝对路径、原始凭据或原始文件内容。ID 只供服务端目录查询。

use crate::error::ErrorCode;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 从授权会话的目录解析的不透明 Checkpoint ID。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CheckpointId(pub String);

/// 由服务端签发的不透明操作 ID。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GitOperationId(pub String);

/// 从授权恢复目录解析的不透明 Safety ID。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SafetyId(pub String);

/// 完整 Git 对象名。反序列化拒绝缩写、引用及修订表达式，
/// 服务端仍须按仓库对象格式与授权目录复核。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String")]
pub struct GitObjectId(String);

impl GitObjectId {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for GitObjectId {
    type Error = &'static str;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            Ok(Self(value))
        } else {
            Err("需要完整的 Git 对象 ID")
        }
    }
}

/// Checkpoint 按其声明的覆盖域评估完整性。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointState {
    Complete,
    Incomplete,
    Unavailable,
}

/// 覆盖清单的脱敏计数，省略明细仍计入 omitted_paths。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointCoverageSummary {
    pub tracked_paths: u32,
    pub untracked_paths: u32,
    pub omitted_paths: u32,
    pub manifest_digest: Option<String>,
}

/// 记录覆盖域中未捕获或未展示的路径及稳定原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointOmissionReason {
    Ignored,
    PolicyDenied,
    SizeLimit,
    UnsupportedEntry,
    PathEncoding,
    ReadFailed,
    BudgetExceeded,
}

/// 不能安全展示路径时，path 留空，仍保留遗漏原因。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointOmissionRecord {
    pub path: Option<String>,
    pub reason: CheckpointOmissionReason,
}

/// 会话 Checkpoint 的目录记录，与旧轮次消息中的轻量锚点并存。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointRecordSummary {
    pub workspace_id: String,
    pub session_id: String,
    pub checkpoint_id: CheckpointId,
    pub parent_oid: Option<String>,
    pub worktree_tree_oid: String,
    pub commit_oid: String,
    pub turn_index: u32,
    pub changed_files: u32,
    pub coverage: CheckpointCoverageSummary,
    pub omissions: Vec<CheckpointOmissionRecord>,
    pub state: CheckpointState,
    pub created_at: DateTime<Utc>,
}

/// 从当前工作树与会话的授权目录解析固定树。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CheckpointBaseline {
    AuthorizedBaseTree {
        /// OID 仅标识对象，服务端仍须验证归属、可达性和授权。
        tree_oid: GitObjectId,
    },
    Checkpoint {
        checkpoint_id: CheckpointId,
    },
}

/// 服务端已解析并固定的历史端点。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FixedDiffEndpoint {
    pub baseline: CheckpointBaseline,
    pub tree_oid: String,
    pub commit_oid: Option<String>,
    pub coverage: CheckpointCoverageSummary,
    pub omissions: Vec<CheckpointOmissionRecord>,
    pub state: CheckpointState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListCheckpointsRequest {
    pub workspace_id: String,
    pub session_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListCheckpointsResult {
    pub workspace_id: String,
    pub session_id: String,
    pub checkpoints: Vec<CheckpointRecordSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointDiffCompareRequest {
    pub workspace_id: String,
    pub session_id: String,
    pub from: CheckpointBaseline,
    pub to: CheckpointBaseline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GitPathChangeKind {
    Added,
    Deleted,
    Modified,
    Renamed,
    Unchanged,
}

/// 重命名时 previous_path 和 path 分别描述源和目标。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitPathChange {
    pub kind: GitPathChangeKind,
    pub path: String,
    pub previous_path: Option<String>,
}

/// Diff 区段的双侧行范围，只包含位置和计数。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointDiffHunkSummary {
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
}

/// 文件 Diff 摘要仅携带行计数和区段范围，不携带行文本。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointDiffFileSummary {
    pub change: GitPathChange,
    pub binary: bool,
    pub added_lines: Option<u32>,
    pub deleted_lines: Option<u32>,
    pub hunks: Vec<CheckpointDiffHunkSummary>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointDiffIncompleteReason {
    EndpointCoverage,
    DiffBudget,
    EndpointCoverageAndDiffBudget,
}

/// incomplete_reason 有值时，files 只代表已覆盖和成功展示的范围。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointDiffCompareResult {
    pub workspace_id: String,
    pub session_id: String,
    pub from: FixedDiffEndpoint,
    pub to: FixedDiffEndpoint,
    pub coverage: CheckpointCoverageSummary,
    pub omissions: Vec<CheckpointOmissionRecord>,
    pub incomplete_reason: Option<CheckpointDiffIncompleteReason>,
    pub files: Vec<CheckpointDiffFileSummary>,
}

/// 恢复 Safety 的工作树也须创建新计划及新的独立授权。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RewindWorktreeTarget {
    Checkpoint { checkpoint_id: CheckpointId },
    Safety { safety_id: SafetyId },
}

/// 分别指定两个授权域的恢复来源。组合申请时每种动作至多出现一次。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum RewindTargetAction {
    /// 恢复指定目标的完整覆盖域，保留当前 index。
    RestoreWorktree { target: RewindWorktreeTarget },
    /// 恢复先前 Safety 的完整原暂存状态。
    RestoreIndex { safety_id: SafetyId },
}

/// 服务端预检得出的完整影响范围，客户端批准时须原样回传。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RewindActionScope {
    #[serde(flatten)]
    pub action: RewindTargetAction,
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SafetyProtectionState {
    Pending,
    Durable,
    Incomplete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SafetyVerificationState {
    Pending,
    Verified,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SafetyRetentionPolicy {
    UntilReleased,
    Until { retain_until: DateTime<Utc> },
}

/// 恢复目录中的句柄，验证状态涵盖本次要求的保护产物和记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SafetySnapshotHandle {
    pub safety_id: SafetyId,
    pub worktree: SafetyProtectionState,
    pub index: SafetyProtectionState,
    pub retention: SafetyRetentionPolicy,
    pub verification: SafetyVerificationState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitPlanBlocker {
    pub code: ErrorCode,
    pub path: Option<String>,
    pub summary: String,
}

/// 工作树和 index 各自预览，便于分别展示 staged 和 unstaged 影响。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RewindPathPreview {
    pub change: GitPathChange,
    pub current_content_digest: Option<String>,
    pub target_blob_oid: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RewindPreviewRequest {
    pub workspace_id: String,
    pub session_id: String,
    pub actions: Vec<RewindTargetAction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RewindPlanPreview {
    pub operation_id: GitOperationId,
    pub workspace_id: String,
    pub session_id: String,
    pub actions: Vec<RewindActionScope>,
    pub worktree_changes: Vec<RewindPathPreview>,
    pub index_changes: Vec<RewindPathPreview>,
    pub blockers: Vec<GitPlanBlocker>,
    pub estimated_safety_bytes: u64,
    pub plan_digest: String,
    pub expires_at: DateTime<Utc>,
}

/// 仅批准显式列出的动作及其范围，服务端执行前重新核对计划和授权。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RewindApprovalTicket {
    pub operation_id: GitOperationId,
    pub workspace_id: String,
    pub session_id: String,
    pub approved_actions: Vec<RewindActionScope>,
    pub plan_digest: String,
    pub idempotency_key: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RewindStage {
    Planned,
    AwaitingApproval,
    Revalidating,
    CapturingSafety,
    SafetyDurable,
    Applying,
    Verifying,
    Completed,
    Rejected,
    Invalidated,
    FailedBeforeApply,
    PartialNeedsRecovery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommitContentSource {
    Worktree,
    Index,
    Deletion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommitPathRisk {
    SizeLimit,
    Binary,
    SensitiveContent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitPathPreview {
    pub change: GitPathChange,
    pub source: CommitContentSource,
    pub content_digest: Option<String>,
    pub size_bytes: Option<u64>,
    pub risks: Vec<CommitPathRisk>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommitHook {
    PreCommit,
    PrepareCommitMsg,
    CommitMsg,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitVerificationRequirements {
    pub hooks: Vec<CommitHook>,
    pub hook_adapter_available: bool,
    pub signature_required: bool,
}

/// 发布成功后同步选中文件的 index entry，并保留其余暂存内容。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitIndexSyncPlan {
    pub install_paths: Vec<String>,
    pub preserve_staged_paths: Vec<String>,
}

/// 以下路径是完整文件集合，包含每条选中重命名的源与目标。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitPreviewRequest {
    pub workspace_id: String,
    pub selected_paths: Vec<String>,
    pub message: String,
}

/// 首版只处理完整文件，所有路径和内容来源都由计划摘要绑定。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitPlanPreview {
    pub operation_id: GitOperationId,
    pub workspace_id: String,
    pub parent_oid: String,
    pub target_branch_ref: String,
    pub selected_paths: Vec<String>,
    pub files: Vec<CommitPathPreview>,
    pub unselected_staged_paths: Vec<GitPathChange>,
    pub replaced_staged_paths: Vec<GitPathChange>,
    pub requirements: CommitVerificationRequirements,
    pub index_sync: CommitIndexSyncPlan,
    pub blockers: Vec<GitPlanBlocker>,
    pub plan_digest: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitApprovalTicket {
    pub operation_id: GitOperationId,
    pub workspace_id: String,
    pub selected_paths: Vec<String>,
    pub plan_digest: String,
    pub idempotency_key: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommitStage {
    Planned,
    AwaitingApproval,
    Revalidating,
    StagingPrivateTree,
    PublishReady,
    RefPublished,
    IndexInstalled,
    Completed,
    Rejected,
    Invalidated,
    FailedBeforePublish,
    PartialNeedsRecovery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GitOperationKind {
    Checkpoint,
    Rewind,
    StructuredCommit,
}

/// 阶段携带所属类别，避免跨操作混用状态机。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "stage", rename_all = "snake_case")]
pub enum GitOperationStage {
    Checkpoint(CheckpointState),
    Rewind(RewindStage),
    StructuredCommit(CommitStage),
}

impl GitOperationStage {
    #[must_use]
    pub const fn kind(self) -> GitOperationKind {
        match self {
            Self::Checkpoint(_) => GitOperationKind::Checkpoint,
            Self::Rewind(_) => GitOperationKind::Rewind,
            Self::StructuredCommit(_) => GitOperationKind::StructuredCommit,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GitOperationLookup {
    OperationId { operation_id: GitOperationId },
    IdempotencyKey { idempotency_key: String },
}

/// 幂等键在授权工作树、会话和操作类别内查找。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitOperationStatusQuery {
    pub workspace_id: String,
    pub session_id: Option<String>,
    pub kind: GitOperationKind,
    pub lookup: GitOperationLookup,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitPublishedCommit {
    pub branch_ref: String,
    pub commit_oid: String,
    pub paths: Vec<String>,
}

/// 分开记录工作树与 index 的作用范围，以报告部分成功和待核验状态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitOperationPathScope {
    pub worktree_paths: Vec<String>,
    pub index_paths: Vec<String>,
}

/// 查询与进度事件共享的脱敏快照，可在进程重启后用于引导恢复。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitOperationRecord {
    pub operation_id: GitOperationId,
    pub workspace_id: String,
    pub session_id: Option<String>,
    pub stage: GitOperationStage,
    pub error_code: Option<ErrorCode>,
    pub published_commit: Option<GitPublishedCommit>,
    pub applied: GitOperationPathScope,
    pub pending_recovery: GitOperationPathScope,
    pub unknown: GitOperationPathScope,
    pub safety: Option<SafetySnapshotHandle>,
    pub ref_published_index_pending: bool,
    pub summary: String,
}
