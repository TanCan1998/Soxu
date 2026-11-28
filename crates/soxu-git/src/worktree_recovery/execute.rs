//! 恢复执行：单次确认绑定、持久 journal、原生 Git 写入与重启只读复核（M3 #57-3）。
//!
//! 原生目录身份随审批预览绑定，在协调域持锁后及启动前复验。
//! 当前只有解锁可执行 Git 写入。范围或数据保全证据不足的操作仅交付只读预览。

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use sha2::{Digest, Sha256};
use tokio::sync::watch;

use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;

use crate::probe::{COMMAND_TIMEOUT, OUTPUT_LIMIT, native_path};
use crate::worktree_lifecycle::identity::{AuthorizedDirectory, checked_directory};
use crate::worktree_lifecycle::{
    ExecutionResult, ExecutionStage, TargetState, WorktreeLifecycleError, read_intents, state,
};
use crate::{
    GitError, GitProbe, LocalProject, ProjectOpenError, RecoveryOperation, WorktreeHead,
    WorktreeIdentity, WorktreeInventory, WorktreeInventoryError, WorktreeKind,
    WorktreeRegistrationCoordinator,
};

use super::journal::{self, JournalRecord, RecoveryExecutionRecord, RecoveryNote};
use super::preview::{self, target_state_of};
use super::{
    EvidenceGap, ObservedWorktreeState, PartialCleanupRejection, PartialCleanupScope,
    PreviewObservation, PreviewOutcome, PruneRejection, RecoveryPreviewError,
    RecoveryPreviewRequest, RemoveRegistrationRejection, RemoveRegistrationScope, RepairRejection,
    UnlockRejection, WorktreeObservationKind,
};

/// 单次确认的最长有效期；宿主必须另取一次本机用户二次确认。
const MAX_CONFIRMATION_WINDOW: Duration = Duration::from_secs(600);

/// 预览摘要：把一次预览的可确认事实压成固定 32 字节，供单次确认绑定。
///
/// 摘要覆盖操作类型、显式授权目标、清单与观察状态、证据缺口以及逐操作的候选集合；
/// 执行前重算不一致即视为漂移，确认作废。
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct RecoveryPreviewDigest([u8; 32]);

impl RecoveryPreviewDigest {
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn to_hex(&self) -> String {
        let mut text = String::with_capacity(64);
        for byte in self.0 {
            text.push(char::from_digit(u32::from(byte >> 4), 16).expect("十六进制高位"));
            text.push(char::from_digit(u32::from(byte & 0x0f), 16).expect("十六进制低位"));
        }
        text
    }
}

impl fmt::Debug for RecoveryPreviewDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "RecoveryPreviewDigest({})", self.to_hex())
    }
}

impl fmt::Display for RecoveryPreviewDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_hex())
    }
}

/// 计算只读预览内容的摘要。写入授权使用 `RecoveryExecutionPreview::digest`。
pub fn recovery_preview_digest(
    outcome: &PreviewOutcome,
    target_authorization: Option<&Path>,
) -> RecoveryPreviewDigest {
    let mut bytes = Vec::with_capacity(1024);
    bytes.extend_from_slice(b"SOXURP1\0");
    match target_authorization {
        None => bytes.push(0),
        Some(target) => {
            bytes.push(1);
            push_field(&mut bytes, target.as_os_str().as_encoded_bytes());
        }
    }
    encode_preview(outcome, &mut bytes);
    RecoveryPreviewDigest(Sha256::digest(bytes).into())
}

/// 供宿主展示后取得单次确认的预览，含无法由路径字符串重建的原生身份。
#[derive(Debug)]
pub struct RecoveryExecutionPreview {
    outcome: PreviewOutcome,
    binding: RecoveryBinding,
    digest: RecoveryPreviewDigest,
}

impl RecoveryExecutionPreview {
    pub fn outcome(&self) -> &PreviewOutcome {
        &self.outcome
    }

    pub fn digest(&self) -> RecoveryPreviewDigest {
        self.digest
    }

    /// 宿主应展示拒绝原因，并在有拒绝时保留只读候选以供复核。
    pub fn execution_blocker(&self) -> Option<&'static str> {
        execution_blocker(self.binding.operation).or_else(|| rejection_code(&self.outcome))
    }

    /// 仅在本机用户确认已展示的预览后调用，保留同一执行 ID。
    pub fn confirm(self, expires_at: SystemTime) -> RecoveryConfirmation {
        RecoveryConfirmation {
            binding: self.binding,
            preview_digest: self.digest,
            issued_at: SystemTime::now(),
            expires_at,
        }
    }
}

/// 单次确认，绑定审批预览生成时的仓库、目标和协调域。
#[derive(Debug, Clone)]
pub struct RecoveryConfirmation {
    binding: RecoveryBinding,
    preview_digest: RecoveryPreviewDigest,
    issued_at: SystemTime,
    expires_at: SystemTime,
}

#[derive(Debug, Clone)]
struct RecoveryBinding {
    execution_id: String,
    operation: RecoveryOperation,
    repository: WorktreeIdentity,
    state_root: AuthorizedDirectory,
    common_dir: AuthorizedDirectory,
    target: Option<PathBuf>,
    partial_cleanup_request_id: Option<String>,
    directories: Vec<AuthorizedDirectory>,
    target_admin: Option<AuthorizedDirectory>,
    git_program: PathBuf,
    git_version: Vec<u8>,
    observed_at: SystemTime,
}

impl RecoveryBinding {
    fn request(&self) -> RecoveryPreviewRequest<'_> {
        RecoveryPreviewRequest {
            source_root: &self.repository.root,
            coordinator_state_root: Some(self.state_root.path()),
            target_authorization: self.target.as_deref(),
            operation: self.operation,
            partial_cleanup_request_id: self.partial_cleanup_request_id.as_deref(),
        }
    }

    fn digest(&self, outcome: &PreviewOutcome) -> RecoveryPreviewDigest {
        let mut bytes = b"SOXURP2\0".to_vec();
        push_field(&mut bytes, self.execution_id.as_bytes());
        push_field(&mut bytes, self.git_program.as_os_str().as_encoded_bytes());
        push_field(&mut bytes, &self.git_version);
        for directory in [&self.state_root, &self.common_dir]
            .into_iter()
            .chain(&self.directories)
            .chain(&self.target_admin)
        {
            push_field(&mut bytes, directory.path().as_os_str().as_encoded_bytes());
            bytes.extend_from_slice(&directory.identity().to_bytes());
        }
        bytes
            .extend_from_slice(recovery_preview_digest(outcome, self.target.as_deref()).as_bytes());
        RecoveryPreviewDigest(Sha256::digest(bytes).into())
    }

    fn refresh(&self) -> Result<(), RecoveryExecuteError> {
        self.state_root.refresh()?;
        for directory in std::iter::once(&self.common_dir).chain(&self.directories) {
            directory
                .refresh()
                .map_err(|_| RecoveryExecuteError::IdentityChanged)?;
        }
        if let Some(admin) = &self.target_admin {
            let (observed, marker) = linked_admin(&self.repository, required_target(self)?)?;
            if observed.identity() != admin.identity() || observed.path() != admin.path() {
                return Err(RecoveryExecuteError::IdentityChanged);
            }
            verify_backlink(admin, &marker)?;
        }
        Ok(())
    }
}

/// 协作式取消。命令已启动时尝试回收直接进程，并持久化未知结局。
#[derive(Debug, Clone)]
pub struct RecoveryCancellation(watch::Sender<bool>);

impl Default for RecoveryCancellation {
    fn default() -> Self {
        Self(watch::channel(false).0)
    }
}

impl RecoveryCancellation {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.send_replace(true);
    }

    pub fn is_cancelled(&self) -> bool {
        *self.0.borrow()
    }
}

/// 命令结束后的只读回读事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryObservation {
    pub observed_at: SystemTime,
    pub inventory: WorktreeInventory,
    /// 绑定目标是否仍在清单中。
    pub target_registered: bool,
    /// 通过原生目录身份复核后的目标状态。
    pub target_state: TargetState,
}

/// 一次执行的结果分类。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryExecutionOutcome {
    /// 命令成功且回读与预期一致。
    Completed {
        record: RecoveryExecutionRecord,
        observation: RecoveryObservation,
    },
    /// 命令成功但回读显示残留或漂移；`record.blocked` 为真。
    PartialFootprint {
        record: RecoveryExecutionRecord,
        observation: RecoveryObservation,
        command: ExecutionResult,
    },
    /// 原生命令非零退出，足迹与预约保留。
    Failed {
        record: RecoveryExecutionRecord,
        observation: Option<RecoveryObservation>,
        command: ExecutionResult,
    },
    /// 结局无法确认（启动失败、超时、输出超限、进程树未收敛或回读失败）。
    UnknownOutcome {
        record: RecoveryExecutionRecord,
        observation: Option<RecoveryObservation>,
        command: ExecutionResult,
        reason: &'static str,
    },
    /// 清理部分落盘：没有可证明的宿主资产需要处理，零写入。
    NothingToClean { record: RecoveryExecutionRecord },
}

impl RecoveryExecutionOutcome {
    /// 本结论对应的持久记录。
    pub fn record(&self) -> &RecoveryExecutionRecord {
        match self {
            RecoveryExecutionOutcome::Completed { record, .. }
            | RecoveryExecutionOutcome::PartialFootprint { record, .. }
            | RecoveryExecutionOutcome::Failed { record, .. }
            | RecoveryExecutionOutcome::UnknownOutcome { record, .. }
            | RecoveryExecutionOutcome::NothingToClean { record } => record,
        }
    }
}

/// 恢复执行的拒绝与失败。
#[derive(Debug, thiserror::Error)]
pub enum RecoveryExecuteError {
    #[error("恢复执行请求字段无效：{0}")]
    InvalidInput(&'static str),
    #[error("确认已超过有效期，必须重新预览并重新确认")]
    ConfirmationExpired,
    #[error("确认绑定的预览证据已漂移，本次确认作废")]
    PreviewDrift,
    #[error("预览当前拒绝该操作：{0}")]
    PreviewRejected(&'static str),
    #[error("该确认已被消费：{0:?}")]
    AlreadyAttempted(Box<RecoveryExecutionRecord>),
    #[error("同一仓库或目标仍有未裁决的执行：{0}")]
    ExecutionStillUnknown(String),
    #[error("当前执行入口缺少所需证据：{0}")]
    EvidenceUnavailable(&'static str),
    #[error("执行任务未返回结论，请检查持久记录")]
    OwnerLost,
    #[error("清理部分落盘的范围无法证明，必须保留并报告")]
    UnprovableFootprint,
    #[error("恢复执行在启动 Git 前已取消")]
    Cancelled,
    #[error("仓库身份在执行与预览之间发生变化")]
    IdentityChanged,
    #[error(transparent)]
    Preview(#[from] RecoveryPreviewError),
    #[error(transparent)]
    Diagnostic(#[from] WorktreeLifecycleError),
    #[error(transparent)]
    Git(#[from] GitError),
    #[error(transparent)]
    Inventory(#[from] WorktreeInventoryError),
    #[error(transparent)]
    Project(#[from] ProjectOpenError),
}

impl GitProbe {
    /// 只读生成供本机用户审批的预览。调用方持续提供宿主独占的写入边界，
    /// 并排除域外 Git、任务租约及其他进程对授权目录和路径链的并发变更。
    /// `registration.lock` 只串行化共享协调域的 Soxu 进程。
    pub async fn prepare_recovery(
        &self,
        coordinator: &WorktreeRegistrationCoordinator,
        execution_id: &str,
        request: RecoveryPreviewRequest<'_>,
    ) -> Result<RecoveryExecutionPreview, RecoveryExecuteError> {
        crate::worktree_lifecycle::validate_request_id(execution_id)?;
        let binding = bind_identity(self, coordinator, execution_id, request).await?;
        let outcome = preview::build(self, binding.request()).await?;
        verify_repository_identity(self, &binding).await?;
        let digest = binding.digest(&outcome);
        Ok(RecoveryExecutionPreview {
            outcome,
            binding,
            digest,
        })
    }

    /// 执行已确认的恢复操作。命令与持久化任务持锁至结果落盘，即使调用方先返回。
    pub async fn execute_recovery(
        &self,
        coordinator: &WorktreeRegistrationCoordinator,
        confirmation: &RecoveryConfirmation,
        cancellation: Option<&RecoveryCancellation>,
    ) -> Result<RecoveryExecutionOutcome, RecoveryExecuteError> {
        execute(self, coordinator, confirmation, cancellation).await
    }

    /// 重启后只读诊断：只读 journal，不重放、不清理、不访问仓库。
    pub fn diagnose_recovery(
        &self,
        coordinator: &WorktreeRegistrationCoordinator,
        execution_id: &str,
    ) -> Result<Option<RecoveryExecutionRecord>, RecoveryExecuteError> {
        let record = journal::read(&coordinator.state_root, execution_id)?;
        Ok(record.map(|record| record.record))
    }

    /// 列出协调域内全部执行记录（按执行 ID 排序），供重启复核。
    pub fn recovery_execution_records(
        &self,
        coordinator: &WorktreeRegistrationCoordinator,
    ) -> Result<Vec<RecoveryExecutionRecord>, RecoveryExecuteError> {
        let records = journal::read_all(&coordinator.state_root)?;
        Ok(records.into_iter().map(|record| record.record).collect())
    }
}

async fn execute(
    git: &GitProbe,
    coordinator: &WorktreeRegistrationCoordinator,
    confirmation: &RecoveryConfirmation,
    cancellation: Option<&RecoveryCancellation>,
) -> Result<RecoveryExecutionOutcome, RecoveryExecuteError> {
    let binding = &confirmation.binding;
    if let Some(reason) = execution_blocker(binding.operation) {
        return Err(RecoveryExecuteError::EvidenceUnavailable(reason));
    }
    check_start(confirmation, cancellation)?;
    let state_root = &coordinator.state_root;
    if binding.state_root.path() != state_root.path()
        || binding.state_root.identity() != state_root.identity()
    {
        return Err(RecoveryExecuteError::IdentityChanged);
    }
    let lock = state::acquire_state_lock(state_root).await?;
    check_start(confirmation, cancellation)?;
    if let Some(existing) = journal::read(state_root, &binding.execution_id)? {
        return Err(RecoveryExecuteError::AlreadyAttempted(Box::new(
            existing.record,
        )));
    }
    for record in journal::read_all(state_root)? {
        if record.record.reserves_scope()
            && (same_scope(&record.record, binding)
                || record
                    .common_identity
                    .is_none_or(|identity| identity == binding.common_dir.identity()))
        {
            return Err(RecoveryExecuteError::ExecutionStillUnknown(
                record.record.execution_id,
            ));
        }
    }
    verify_repository_identity(git, binding).await?;
    let preview = preview::build(git, binding.request()).await?;
    if binding.digest(&preview) != confirmation.preview_digest {
        return Err(RecoveryExecuteError::PreviewDrift);
    }
    if let Some(reason) = rejection_code(&preview) {
        return Err(RecoveryExecuteError::PreviewRejected(reason));
    }
    let command = match preview {
        PreviewOutcome::Unlock(preview) => {
            let target = required_target(binding)?;
            if preview.candidate.path.to_path_buf().as_deref() != Ok(target) {
                return Err(RecoveryExecuteError::PreviewDrift);
            }
            let target = crate::worktree_lifecycle::native_command_path(target)?;
            let mut command = git.command(&binding.repository.root);
            command.args(["worktree", "unlock", "--"]).arg(target);
            Some(command)
        }
        PreviewOutcome::PartialCleanup(preview)
            if preview.scope == PartialCleanupScope::TargetAbsent =>
        {
            let intent = read_intents(state_root)?
                .into_iter()
                .find(|intent| {
                    Some(intent.request_id.as_str())
                        == binding.partial_cleanup_request_id.as_deref()
                })
                .ok_or(RecoveryExecuteError::UnprovableFootprint)?;
            let target = required_target(binding)?;
            if intent.source != binding.repository || intent.target != target {
                return Err(RecoveryExecuteError::UnprovableFootprint);
            }
            let observation = git
                .diagnose_registration_with_target(
                    state_root.path(),
                    &intent.request_id,
                    &binding.repository.root,
                    target,
                )
                .await?
                .ok_or(RecoveryExecuteError::UnprovableFootprint)?;
            if observation.target_state != TargetState::Absent
                || observation.target_registered
                || observation.ref_present
            {
                return Err(RecoveryExecuteError::UnprovableFootprint);
            }
            None
        }
        _ => return Err(RecoveryExecuteError::UnprovableFootprint),
    };
    lock.refresh(state_root)?;
    binding.refresh()?;
    check_start(confirmation, cancellation)?;
    let record = JournalRecord {
        record: RecoveryExecutionRecord {
            execution_id: binding.execution_id.clone(),
            operation: binding.operation,
            source: binding.repository.clone(),
            target: binding.target.clone(),
            stage: ExecutionStage::Prepared,
            result: ExecutionResult::NotStarted,
            recorded_at: SystemTime::now(),
            started_at: None,
            observed_at: None,
            note: RecoveryNote::NotStarted,
            blocked: true,
        },
        preview_digest: confirmation.preview_digest.0,
        common_identity: Some(binding.common_dir.identity()),
    };
    journal::begin(state_root, &record)?;
    let git = git.clone();
    let confirmation = confirmation.clone();
    let cancellation = cancellation.cloned();
    tokio::spawn(async move {
        finish(
            &git,
            &confirmation,
            cancellation.as_ref(),
            lock,
            record,
            command,
        )
        .await
    })
    .await
    .map_err(|_| RecoveryExecuteError::OwnerLost)?
}

async fn finish(
    git: &GitProbe,
    confirmation: &RecoveryConfirmation,
    cancellation: Option<&RecoveryCancellation>,
    lock: state::StateLock,
    mut entry: JournalRecord,
    command: Option<Command>,
) -> Result<RecoveryExecutionOutcome, RecoveryExecuteError> {
    let binding = &confirmation.binding;
    let state_root = &binding.state_root;
    if command.is_some() {
        entry.record.stage = ExecutionStage::Started;
        entry.record.started_at = Some(SystemTime::now());
        entry.record.result = ExecutionResult::Unknown {
            stop_confirmed: false,
            reason: "running".to_owned(),
        };
        lock.refresh(state_root)?;
        journal::rewrite(state_root, &entry)?;
    }
    // 持久化和目录复核都可能耗时。在任何原生写进程启动前最后核验时限与取消。
    lock.refresh(state_root)?;
    let final_check = binding.refresh().and_then(|()| {
        if binding.operation == RecoveryOperation::PartialCleanup
            && target_state_of(required_target(binding)?) != TargetState::Absent
        {
            return Err(RecoveryExecuteError::PreviewDrift);
        }
        check_start(confirmation, cancellation)
    });
    if let Err(error) = final_check {
        entry.record.stage = ExecutionStage::Observed;
        entry.record.result = ExecutionResult::NotStarted;
        entry.record.started_at = None;
        entry.record.observed_at = Some(SystemTime::now());
        entry.record.note = if matches!(error, RecoveryExecuteError::Cancelled) {
            RecoveryNote::Cancelled
        } else {
            RecoveryNote::NotStarted
        };
        entry.record.blocked = false;
        lock.refresh(state_root)?;
        journal::rewrite(state_root, &entry)?;
        return Err(error);
    }
    let Some(mut command) = command else {
        entry.record.stage = ExecutionStage::Observed;
        entry.record.result = ExecutionResult::Succeeded;
        entry.record.observed_at = Some(SystemTime::now());
        entry.record.note = RecoveryNote::NothingToClean;
        entry.record.blocked = false;
        journal::rewrite(state_root, &entry)?;
        return Ok(RecoveryExecutionOutcome::NothingToClean {
            record: entry.record,
        });
    };

    let result = run_native(&mut command, OUTPUT_LIMIT, COMMAND_TIMEOUT, cancellation).await;
    entry.record.stage = ExecutionStage::Completed;
    entry.record.result = result.clone();
    if result == ExecutionResult::SpawnFailed {
        entry.record.started_at = None;
    }
    lock.refresh(state_root)?;
    journal::rewrite(state_root, &entry)?;
    // 进程树结局不确定时，当前清单无法证明最终状态，保留预约待裁决。
    let (observation, satisfied) = if matches!(result, ExecutionResult::Unknown { .. }) {
        (None, false)
    } else {
        observe_unlock(git, binding)
            .await
            .map_or((None, false), |(value, satisfied)| (Some(value), satisfied))
    };
    let (note, observation) = classify(&result, observation, satisfied);
    entry.record.stage = ExecutionStage::Observed;
    entry.record.observed_at = Some(SystemTime::now());
    entry.record.note = note;
    entry.record.blocked = note != RecoveryNote::Verified;
    lock.refresh(state_root)?;
    journal::rewrite(state_root, &entry)?;
    let record = entry.record;
    Ok(match (note, observation) {
        (RecoveryNote::Verified, Some(observation)) => RecoveryExecutionOutcome::Completed {
            record,
            observation,
        },
        (RecoveryNote::PartialFootprint, Some(observation)) => {
            RecoveryExecutionOutcome::PartialFootprint {
                record,
                observation,
                command: result,
            }
        }
        (RecoveryNote::CommandFailed, observation) => RecoveryExecutionOutcome::Failed {
            record,
            observation,
            command: result,
        },
        (_, observation) => RecoveryExecutionOutcome::UnknownOutcome {
            record,
            observation,
            reason: unknown_reason(&result),
            command: result,
        },
    })
}

fn check_start(
    confirmation: &RecoveryConfirmation,
    cancellation: Option<&RecoveryCancellation>,
) -> Result<(), RecoveryExecuteError> {
    let now = SystemTime::now();
    if now >= confirmation.expires_at
        || now < confirmation.issued_at
        || now < confirmation.binding.observed_at
        || now
            .duration_since(confirmation.binding.observed_at)
            .is_ok_and(|age| age >= MAX_CONFIRMATION_WINDOW)
    {
        return Err(RecoveryExecuteError::ConfirmationExpired);
    }
    if !confirmation
        .expires_at
        .duration_since(confirmation.issued_at)
        .is_ok_and(|window| window <= MAX_CONFIRMATION_WINDOW)
    {
        return Err(RecoveryExecuteError::InvalidInput("确认有效期超过上限"));
    }
    if cancellation.is_some_and(RecoveryCancellation::is_cancelled) {
        return Err(RecoveryExecuteError::Cancelled);
    }
    Ok(())
}

fn execution_blocker(operation: RecoveryOperation) -> Option<&'static str> {
    Some(match operation {
        RecoveryOperation::Prune => "prune_candidate_evidence_and_external_exclusion_required",
        RecoveryOperation::Repair => "repair_bidirectional_identity_and_full_scope_required",
        RecoveryOperation::RemoveRegistration => {
            "remove_target_cleanliness_and_recovery_evidence_required"
        }
        RecoveryOperation::Unlock | RecoveryOperation::PartialCleanup => return None,
    })
}

async fn bind_identity(
    git: &GitProbe,
    coordinator: &WorktreeRegistrationCoordinator,
    execution_id: &str,
    request: RecoveryPreviewRequest<'_>,
) -> Result<RecoveryBinding, RecoveryExecuteError> {
    let root = checked_directory(request.source_root)?;
    let state_root = coordinator.state_root.clone();
    if request
        .coordinator_state_root
        .is_some_and(|path| path != state_root.path())
    {
        return Err(RecoveryExecuteError::IdentityChanged);
    }
    state::require_private_root(&state_root)?;
    let LocalProject::GitWorktree(repository) = git.open_local_project(root.path()).await? else {
        return Err(RecoveryExecuteError::InvalidInput("源根不是 Git 工作树"));
    };
    if repository.root != root.path() {
        return Err(RecoveryExecuteError::IdentityChanged);
    }
    let common_dir = checked_directory(&repository.common_dir)?;
    let mut directories = vec![root, checked_directory(&repository.git_dir)?];
    let mut target_admin = None;
    if let Some(target) = request.target_authorization {
        let parent = target
            .parent()
            .ok_or(RecoveryExecuteError::InvalidInput("目标缺少父目录"))?;
        let parent = checked_directory(parent)?;
        if target.parent() != Some(parent.path()) {
            return Err(RecoveryExecuteError::IdentityChanged);
        }
        directories.push(parent);
        if request.operation == RecoveryOperation::Unlock {
            let (admin, marker) = linked_admin(&repository, target)?;
            verify_backlink(&admin, &marker)?;
            target_admin = Some(admin);
            directories.push(checked_directory(target)?);
        }
    } else if matches!(
        request.operation,
        RecoveryOperation::Unlock | RecoveryOperation::PartialCleanup
    ) {
        return Err(RecoveryExecuteError::InvalidInput("操作需要授权单一目标"));
    }
    for directory in std::iter::once(&common_dir)
        .chain(&directories)
        .chain(&target_admin)
    {
        if directory.identity() == state_root.identity()
            || state_root.path().starts_with(directory.path())
            || directory.path().starts_with(state_root.path())
        {
            return Err(RecoveryExecuteError::InvalidInput(
                "协调域须与源仓库和目标隔离",
            ));
        }
    }
    if request.operation == RecoveryOperation::Prune && request.target_authorization.is_some()
        || request.operation != RecoveryOperation::PartialCleanup
            && request.partial_cleanup_request_id.is_some()
    {
        return Err(RecoveryExecuteError::InvalidInput(
            "确认参数与恢复操作不匹配",
        ));
    }
    let git_version = read_git_version(git, &repository.root).await?;
    Ok(RecoveryBinding {
        execution_id: execution_id.to_owned(),
        operation: request.operation,
        repository,
        state_root,
        common_dir,
        target: request.target_authorization.map(Path::to_owned),
        partial_cleanup_request_id: request.partial_cleanup_request_id.map(str::to_owned),
        directories,
        target_admin,
        git_program: git.program().to_owned(),
        git_version,
        observed_at: SystemTime::now(),
    })
}

async fn verify_repository_identity(
    git: &GitProbe,
    binding: &RecoveryBinding,
) -> Result<(), RecoveryExecuteError> {
    binding.refresh()?;
    if git.program() != binding.git_program
        || git.open_local_project(&binding.repository.root).await?
            != LocalProject::GitWorktree(binding.repository.clone())
        || read_git_version(git, &binding.repository.root).await? != binding.git_version
    {
        return Err(RecoveryExecuteError::IdentityChanged);
    }
    binding.refresh()
}

async fn read_git_version(git: &GitProbe, root: &Path) -> Result<Vec<u8>, RecoveryExecuteError> {
    let output = git.run_command(root, "--version", &[]).await?;
    if !output.status.success()
        || !output.stderr.is_empty()
        || !output.stdout.starts_with(b"git version ")
    {
        return Err(RecoveryExecuteError::EvidenceUnavailable(
            "git_version_unverified",
        ));
    }
    Ok(output.stdout)
}

fn required_target(binding: &RecoveryBinding) -> Result<&Path, RecoveryExecuteError> {
    binding
        .target
        .as_deref()
        .ok_or(RecoveryExecuteError::InvalidInput("操作需要授权单一目标"))
}

fn linked_admin(
    repository: &WorktreeIdentity,
    target: &Path,
) -> Result<(AuthorizedDirectory, PathBuf), RecoveryExecuteError> {
    let target = checked_directory(target)?;
    let marker = target.path().join(".git");
    let bytes = state::read_regular_file(&marker)?;
    let path = bytes
        .strip_prefix(b"gitdir: ")
        .and_then(|bytes| bytes.strip_suffix(b"\n"))
        .ok_or(RecoveryExecuteError::IdentityChanged)?;
    let admin = native_path(path)?;
    if admin.parent() != Some(repository.common_dir.join("worktrees").as_path())
        || admin
            .components()
            .any(|part| part == std::path::Component::ParentDir)
    {
        return Err(RecoveryExecuteError::IdentityChanged);
    }
    let directory = checked_directory(&admin)?;
    if directory.path() != admin {
        return Err(RecoveryExecuteError::IdentityChanged);
    }
    Ok((directory, marker))
}

fn verify_backlink(admin: &AuthorizedDirectory, marker: &Path) -> Result<(), RecoveryExecuteError> {
    let gitdir = state::read_regular_file(&admin.path().join("gitdir"))?;
    let commondir = state::read_regular_file(&admin.path().join("commondir"))?;
    if gitdir.strip_suffix(b"\n") != Some(marker.as_os_str().as_encoded_bytes())
        || commondir != b"../..\n"
    {
        return Err(RecoveryExecuteError::IdentityChanged);
    }
    Ok(())
}

fn rejection_code(outcome: &PreviewOutcome) -> Option<&'static str> {
    match outcome {
        PreviewOutcome::Unlock(preview) => preview.rejection.map(|rejection| match rejection {
            UnlockRejection::NoLinkedRegistration => "unlock_no_linked_registration",
            UnlockRejection::NotLocked => "unlock_not_locked",
            UnlockRejection::LockEvidenceInsufficient => "unlock_lock_evidence_insufficient",
        }),
        PreviewOutcome::Repair(preview) => preview.rejection.map(|rejection| match rejection {
            RepairRejection::ScopeUnbounded => "repair_scope_unbounded",
            RepairRejection::NoProvenCandidate => "repair_no_proven_candidate",
        }),
        PreviewOutcome::Prune(preview) => preview.rejection.map(|rejection| match rejection {
            PruneRejection::LockedRegistrationUnadjudicated => "prune_locked_unadjudicated",
        }),
        PreviewOutcome::RemoveRegistration(preview) => {
            preview.rejection.map(|rejection| match rejection {
                RemoveRegistrationRejection::NoLinkedRegistration => {
                    "remove_no_linked_registration"
                }
                RemoveRegistrationRejection::CleanlinessUnproven => "remove_cleanliness_unproven",
                RemoveRegistrationRejection::Locked => "remove_locked",
                RemoveRegistrationRejection::HealthUncertain => "remove_health_uncertain",
            })
        }
        PreviewOutcome::PartialCleanup(preview) => {
            preview.rejection.map(|rejection| match rejection {
                PartialCleanupRejection::IntentNotFound => "cleanup_intent_not_found",
                PartialCleanupRejection::IntentIdentityMissing => "cleanup_intent_identity_missing",
                PartialCleanupRejection::ExecutionUnknown => "cleanup_execution_unknown",
                PartialCleanupRejection::TargetUnprovable => "cleanup_target_unprovable",
            })
        }
    }
}

fn same_scope(record: &RecoveryExecutionRecord, binding: &RecoveryBinding) -> bool {
    record.source.root == binding.repository.root
        || record.source.common_dir == binding.repository.common_dir
        || record
            .target
            .as_ref()
            .zip(binding.target.as_ref())
            .is_some_and(|(old, new)| old == new)
}

async fn observe_unlock(
    git: &GitProbe,
    binding: &RecoveryBinding,
) -> Result<(RecoveryObservation, bool), RecoveryExecuteError> {
    verify_repository_identity(git, binding).await?;
    let inventory = git.list_worktrees(&binding.repository.root).await?;
    binding.refresh()?;
    let target = required_target(binding)?;
    let entry = inventory.entries.iter().find(|entry| {
        entry.kind == WorktreeKind::Linked && entry.path.to_path_buf().as_deref() == Ok(target)
    });
    let satisfied = entry.is_some_and(|entry| entry.locked.is_none() && entry.prunable.is_none());
    Ok((
        RecoveryObservation {
            observed_at: SystemTime::now(),
            target_registered: entry.is_some(),
            target_state: TargetState::Directory,
            inventory,
        },
        satisfied,
    ))
}

fn classify(
    command: &ExecutionResult,
    observation: Option<RecoveryObservation>,
    satisfied: bool,
) -> (RecoveryNote, Option<RecoveryObservation>) {
    match command {
        // 命令成功但回读失败时无法判定结局，按未知处理而不是假定成功。
        ExecutionResult::Succeeded => match observation {
            Some(observation) if satisfied => (RecoveryNote::Verified, Some(observation)),
            Some(observation) => (RecoveryNote::PartialFootprint, Some(observation)),
            None => (RecoveryNote::UnknownOutcome, None),
        },
        ExecutionResult::Failed { .. } => (RecoveryNote::CommandFailed, observation),
        ExecutionResult::SpawnFailed
        | ExecutionResult::Unknown { .. }
        | ExecutionResult::NotStarted => (RecoveryNote::UnknownOutcome, observation),
    }
}

fn unknown_reason(result: &ExecutionResult) -> &'static str {
    match result {
        ExecutionResult::SpawnFailed => "spawn_failed",
        ExecutionResult::Unknown { reason, .. } if reason == "timeout" => "timeout",
        ExecutionResult::Unknown { reason, .. } if reason == "output_limit" => "output_limit",
        ExecutionResult::Unknown { reason, .. } if reason == "cancelled" => "cancelled",
        ExecutionResult::Unknown {
            stop_confirmed: false,
            ..
        } => "stop_unconfirmed",
        ExecutionResult::Unknown { .. } => "unknown",
        _ => "readback_failed",
    }
}

/// 有界运行原生 Git：读取独占管道，超时或读取失败先释放管道再停止子进程。
async fn run_native(
    command: &mut Command,
    limit: u64,
    timeout: Duration,
    cancellation: Option<&RecoveryCancellation>,
) -> ExecutionResult {
    let Ok(mut child) = command.spawn() else {
        return ExecutionResult::SpawnFailed;
    };
    let result = {
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let child = &mut child;
        let collect = async move {
            let stdout = stdout.ok_or("missing_stdout")?;
            let stderr = stderr.ok_or("missing_stderr")?;
            let (_, _, status) = tokio::try_join!(
                count_output(stdout, limit),
                count_output(stderr, limit),
                async { child.wait().await.map_err(|_| "wait_io") },
            )?;
            Ok(status)
        };
        tokio::time::timeout(timeout, async {
            if let Some(cancellation) = cancellation {
                let mut receiver = cancellation.0.subscribe();
                tokio::select! {
                    biased;
                    result = collect => result,
                    _ = receiver.wait_for(|cancelled| *cancelled) => Err("cancelled"),
                }
            } else {
                collect.await
            }
        })
        .await
    };
    let reason = match result {
        Ok(Ok(status)) if status.success() => return ExecutionResult::Succeeded,
        Ok(Ok(status)) => {
            return ExecutionResult::Failed {
                exit_code: status.code(),
            };
        }
        Ok(Err(reason)) => reason,
        Err(_) => "timeout",
    };
    if !matches!(child.try_wait(), Ok(Some(_))) {
        let _ = child.start_kill();
        let _ = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
    }
    // 直接子进程回收没有提供后代进程退出的证据。
    ExecutionResult::Unknown {
        stop_confirmed: false,
        reason: reason.to_owned(),
    }
}

async fn count_output(reader: impl AsyncRead + Unpin, limit: u64) -> Result<(), &'static str> {
    let bytes = tokio::io::copy(
        &mut reader.take(limit.saturating_add(1)),
        &mut tokio::io::sink(),
    )
    .await
    .map_err(|_| "output_io")?;
    if bytes > limit {
        return Err("output_limit");
    }
    Ok(())
}

// ---------------------------------------------------------------- 预览摘要编码

fn push_field(output: &mut Vec<u8>, bytes: &[u8]) {
    output.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
    output.extend_from_slice(bytes);
}

fn push_tag(output: &mut Vec<u8>, tag: u8) {
    output.push(tag);
}

fn encode_preview(outcome: &PreviewOutcome, output: &mut Vec<u8>) {
    match outcome {
        PreviewOutcome::Unlock(preview) => {
            push_tag(output, 1);
            encode_observation(&preview.observation, output);
            push_field(output, preview.candidate.path.as_bytes());
            match &preview.candidate.lock_reason {
                None => push_tag(output, 0),
                Some(reason) => {
                    push_tag(output, 1);
                    push_field(output, reason);
                }
            }
            push_field(output, preview.candidate.protection_loss_notice.as_bytes());
            match preview.rejection {
                None => push_tag(output, 0),
                Some(UnlockRejection::NoLinkedRegistration) => push_tag(output, 1),
                Some(UnlockRejection::NotLocked) => push_tag(output, 2),
                Some(UnlockRejection::LockEvidenceInsufficient) => push_tag(output, 3),
            }
        }
        PreviewOutcome::Repair(preview) => {
            push_tag(output, 2);
            encode_observation(&preview.observation, output);
            push_field(output, &(preview.candidates.len() as u64).to_le_bytes());
            for candidate in &preview.candidates {
                push_field(output, candidate.registered_path.as_bytes());
                push_field(output, candidate.moved_to.as_os_str().as_encoded_bytes());
            }
            match preview.rejection {
                None => push_tag(output, 0),
                Some(RepairRejection::ScopeUnbounded) => push_tag(output, 1),
                Some(RepairRejection::NoProvenCandidate) => push_tag(output, 2),
            }
        }
        PreviewOutcome::Prune(preview) => {
            push_tag(output, 3);
            encode_observation(&preview.observation, output);
            push_field(output, &(preview.candidates.len() as u64).to_le_bytes());
            for candidate in &preview.candidates {
                push_field(output, &candidate.admin_dir);
                push_field(output, &candidate.reason);
            }
            encode_inventory(&preview.inventory_after, output);
            match preview.rejection {
                None => push_tag(output, 0),
                Some(PruneRejection::LockedRegistrationUnadjudicated) => push_tag(output, 1),
            }
        }
        PreviewOutcome::RemoveRegistration(preview) => {
            push_tag(output, 4);
            encode_observation(&preview.observation, output);
            match &preview.scope {
                RemoveRegistrationScope::OnlineClean {
                    path,
                    clean,
                    locked,
                } => {
                    push_tag(output, 1);
                    push_field(output, path.as_bytes());
                    push_tag(output, u8::from(*clean));
                    push_tag(output, u8::from(*locked));
                }
                RemoveRegistrationScope::Disconnected {
                    path,
                    prunable_reason,
                } => {
                    push_tag(output, 2);
                    push_field(output, path.as_bytes());
                    match prunable_reason {
                        None => push_tag(output, 0),
                        Some(reason) => {
                            push_tag(output, 1);
                            push_field(output, reason);
                        }
                    }
                }
            }
            match preview.rejection {
                None => push_tag(output, 0),
                Some(RemoveRegistrationRejection::NoLinkedRegistration) => push_tag(output, 1),
                Some(RemoveRegistrationRejection::CleanlinessUnproven) => push_tag(output, 2),
                Some(RemoveRegistrationRejection::Locked) => push_tag(output, 3),
                Some(RemoveRegistrationRejection::HealthUncertain) => push_tag(output, 4),
            }
        }
        PreviewOutcome::PartialCleanup(preview) => {
            push_tag(output, 5);
            encode_observation(&preview.observation, output);
            push_field(output, preview.request_id.as_bytes());
            match &preview.scope {
                PartialCleanupScope::TargetAbsent => push_tag(output, 1),
                PartialCleanupScope::UnprovableContent { target } => {
                    push_tag(output, 2);
                    push_field(output, target.as_os_str().as_encoded_bytes());
                }
            }
            match preview.rejection {
                None => push_tag(output, 0),
                Some(PartialCleanupRejection::IntentNotFound) => push_tag(output, 1),
                Some(PartialCleanupRejection::IntentIdentityMissing) => push_tag(output, 2),
                Some(PartialCleanupRejection::ExecutionUnknown) => push_tag(output, 3),
                Some(PartialCleanupRejection::TargetUnprovable) => push_tag(output, 4),
            }
        }
    }
}

fn encode_observation(observation: &PreviewObservation, output: &mut Vec<u8>) {
    push_field(
        output,
        &(observation.inventory_entry_count as u64).to_le_bytes(),
    );
    push_field(output, &(observation.states.len() as u64).to_le_bytes());
    for state in &observation.states {
        encode_state(state, output);
    }
    push_field(
        output,
        &(observation.evidence_gaps.len() as u64).to_le_bytes(),
    );
    for gap in &observation.evidence_gaps {
        encode_gap(gap, output);
    }
    push_tag(output, target_state_tag(observation.target_state));
}

fn encode_state(state: &ObservedWorktreeState, output: &mut Vec<u8>) {
    push_field(output, state.path.as_bytes());
    push_tag(
        output,
        match state.kind {
            WorktreeKind::Main => 1,
            WorktreeKind::Linked => 2,
        },
    );
    encode_head(&state.head, output);
    push_tag(output, observation_tag(state.state));
    match &state.annotation {
        None => push_tag(output, 0),
        Some(annotation) => {
            push_tag(output, 1);
            push_field(output, annotation);
        }
    }
}

fn encode_head(head: &WorktreeHead, output: &mut Vec<u8>) {
    match head {
        WorktreeHead::Bare => push_tag(output, 1),
        WorktreeHead::Unavailable { oid, detached } => {
            push_tag(output, 2);
            push_field(output, oid.as_bytes());
            push_tag(output, u8::from(*detached));
        }
        WorktreeHead::Attached { oid, branch } => {
            push_tag(output, 3);
            push_field(output, oid.as_bytes());
            push_field(output, branch);
        }
        WorktreeHead::Detached { oid } => {
            push_tag(output, 4);
            push_field(output, oid.as_bytes());
        }
        WorktreeHead::Unborn { oid, branch } => {
            push_tag(output, 5);
            push_field(output, oid.as_bytes());
            push_field(output, branch);
        }
    }
}

fn encode_gap(gap: &EvidenceGap, output: &mut Vec<u8>) {
    match gap {
        EvidenceGap::PathPresenceUnverified { path } => {
            push_tag(output, 1);
            push_field(output, path.as_bytes());
        }
        EvidenceGap::LockLeaseUnconfirmed { path } => {
            push_tag(output, 2);
            push_field(output, path.as_bytes());
        }
        EvidenceGap::RegistrationOutcomeUnknown { request_id } => {
            push_tag(output, 3);
            push_field(output, request_id.as_bytes());
        }
        EvidenceGap::CheckoutHistoryUnknown { request_id } => {
            push_tag(output, 4);
            push_field(output, request_id.as_bytes());
        }
        EvidenceGap::TargetNotExamined { path } => {
            push_tag(output, 5);
            push_field(output, path.as_bytes());
        }
    }
}

fn encode_inventory(inventory: &WorktreeInventory, output: &mut Vec<u8>) {
    push_field(output, &(inventory.entries.len() as u64).to_le_bytes());
    for entry in &inventory.entries {
        push_tag(
            output,
            match entry.kind {
                WorktreeKind::Main => 1,
                WorktreeKind::Linked => 2,
            },
        );
        push_field(output, entry.path.as_bytes());
        encode_head(&entry.head, output);
        for annotation in [&entry.locked, &entry.prunable] {
            match annotation {
                None => push_tag(output, 0),
                Some(annotation) => {
                    push_tag(output, 1);
                    match &annotation.reason {
                        None => push_tag(output, 0),
                        Some(reason) => {
                            push_tag(output, 1);
                            push_field(output, reason);
                        }
                    }
                }
            }
        }
    }
}

fn observation_tag(kind: WorktreeObservationKind) -> u8 {
    match kind {
        WorktreeObservationKind::ValidProtectedLock => 1,
        WorktreeObservationKind::SuspectedOrphanLock => 2,
        WorktreeObservationKind::PrunableOrUnreachable => 3,
        WorktreeObservationKind::VolumeOfflineOrIdentityDrift => 4,
        WorktreeObservationKind::HeadOrRepositoryAnomaly => 5,
        WorktreeObservationKind::RegistrationIndeterminate => 6,
        WorktreeObservationKind::CheckoutPendingOrDisabled => 7,
        WorktreeObservationKind::Healthy => 8,
    }
}

fn target_state_tag(state: TargetState) -> u8 {
    match state {
        TargetState::NotExamined => 0,
        TargetState::Absent => 1,
        TargetState::Directory => 2,
        TargetState::File => 3,
        TargetState::LinkOrReparse => 4,
        TargetState::Inaccessible => 5,
    }
}

#[cfg(test)]
mod tests {
    use std::process::Stdio;
    use std::time::Duration;

    use super::*;

    fn sample_observation() -> PreviewObservation {
        PreviewObservation {
            inventory_entry_count: 1,
            states: Vec::new(),
            evidence_gaps: Vec::new(),
            target_state: TargetState::NotExamined,
        }
    }

    fn prune_preview_sample() -> PreviewOutcome {
        PreviewOutcome::Prune(super::super::PrunePreview {
            observation: sample_observation(),
            candidates: Vec::new(),
            inventory_after: WorktreeInventory {
                entries: Vec::new(),
            },
            rejection: None,
        })
    }

    #[test]
    fn digest_binds_operation_target_and_evidence() {
        let preview = prune_preview_sample();
        let base = recovery_preview_digest(&preview, None);
        assert_eq!(base, recovery_preview_digest(&preview, None));
        assert_ne!(
            base,
            recovery_preview_digest(&preview, Some(Path::new("/fixture/target")))
        );
        // 观察状态变化必须改变摘要（同一预览对象的克隆）。
        let mut changed = prune_preview_sample();
        if let PreviewOutcome::Prune(preview) = &mut changed {
            preview.observation.inventory_entry_count = 2;
        }
        assert_ne!(base, recovery_preview_digest(&changed, None));
        // 拒绝原因变化同样改变摘要。
        let mut rejected = prune_preview_sample();
        if let PreviewOutcome::Prune(preview) = &mut rejected {
            preview.rejection = Some(PruneRejection::LockedRegistrationUnadjudicated);
        }
        assert_ne!(base, recovery_preview_digest(&rejected, None));
    }

    #[test]
    fn classification_keeps_partial_footprint_and_unknown_distinct() {
        let observation = RecoveryObservation {
            observed_at: SystemTime::UNIX_EPOCH,
            inventory: WorktreeInventory {
                entries: Vec::new(),
            },
            target_registered: false,
            target_state: TargetState::Absent,
        };
        // 命令成功但回读不符：保留足迹并标记阻断。
        assert_eq!(
            classify(
                &ExecutionResult::Succeeded,
                Some(observation.clone()),
                false
            ),
            (RecoveryNote::PartialFootprint, Some(observation.clone()))
        );
        // 命令成功但回读失败：结局不可判定，不得假定成功。
        assert_eq!(
            classify(&ExecutionResult::Succeeded, None, false),
            (RecoveryNote::UnknownOutcome, None)
        );
        assert_eq!(
            classify(&ExecutionResult::Succeeded, Some(observation.clone()), true).0,
            RecoveryNote::Verified
        );
        assert_eq!(
            classify(
                &ExecutionResult::Failed {
                    exit_code: Some(128)
                },
                Some(observation.clone()),
                false
            )
            .0,
            RecoveryNote::CommandFailed
        );
        assert_eq!(
            classify(
                &ExecutionResult::Unknown {
                    stop_confirmed: false,
                    reason: "timeout".to_owned()
                },
                None,
                false
            )
            .0,
            RecoveryNote::UnknownOutcome
        );
        assert_eq!(
            unknown_reason(&ExecutionResult::Unknown {
                stop_confirmed: false,
                reason: "running".to_owned()
            }),
            "stop_unconfirmed"
        );
        assert_eq!(
            unknown_reason(&ExecutionResult::SpawnFailed),
            "spawn_failed"
        );
    }

    // 直接复用当前原生测试二进制，覆盖超时、非零退出、输出上限与回收。
    #[test]
    fn execution_child_fixture() {
        let Ok(mode) = std::env::var("SOXU_RECOVERY_EXECUTION_FIXTURE") else {
            return;
        };
        match mode.as_str() {
            "failed" => std::process::exit(37),
            "output" => {
                use std::io::Write;
                let mut stdout = std::io::stdout().lock();
                stdout.write_all(&[b'x'; 4096]).expect("输出夹具字节");
                stdout.flush().expect("刷新夹具输出");
            }
            "timeout" => std::thread::sleep(Duration::from_secs(5)),
            "cancel" => {
                let marker = std::env::var_os("SOXU_RECOVERY_STARTED_MARKER").expect("启动标记");
                std::fs::write(marker, b"started").expect("记录原生子进程已启动");
                std::thread::sleep(Duration::from_secs(10));
            }
            "success" => {}
            _ => panic!("未知执行夹具模式"),
        }
    }

    #[tokio::test]
    async fn cancellation_after_native_start_preserves_unknown() {
        let root = (0..64)
            .find_map(|index| {
                let path = std::env::temp_dir().join(format!(
                    "soxu-recovery-cancel-{}-{index}",
                    std::process::id()
                ));
                match std::fs::create_dir(&path) {
                    Ok(()) => Some(path),
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => None,
                    Err(error) => panic!("建立取消夹具失败：{error}"),
                }
            })
            .expect("创建独占夹具根");
        let marker = root.join("started");
        let cancellation = RecoveryCancellation::new();
        let mut command = Command::new(std::env::current_exe().expect("原生夹具程序"));
        command
            .args([
                "--exact",
                "worktree_recovery::execute::tests::execution_child_fixture",
                "--nocapture",
            ])
            .env("SOXU_RECOVERY_EXECUTION_FIXTURE", "cancel")
            .env("SOXU_RECOVERY_STARTED_MARKER", &marker)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let (result, started) = tokio::join!(
            run_native(
                &mut command,
                8192,
                Duration::from_secs(5),
                Some(&cancellation)
            ),
            tokio::time::timeout(Duration::from_secs(4), async {
                while !marker.is_file() {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                cancellation.cancel();
            })
        );
        std::fs::remove_dir_all(root).expect("清理取消夹具");
        started.expect("仅在证实启动后发送取消");
        assert_eq!(
            result,
            ExecutionResult::Unknown {
                stop_confirmed: false,
                reason: "cancelled".to_owned()
            }
        );
    }

    #[tokio::test]
    async fn native_runner_preserves_unknown_on_timeout_and_output_limit() {
        let executable = std::env::current_exe().expect("取得原生测试入口");
        let mut missing = Command::new(executable.join("missing-program"));
        assert_eq!(
            run_native(&mut missing, 1024, Duration::from_secs(5), None).await,
            ExecutionResult::SpawnFailed
        );
        for (mode, limit, deadline, expected) in [
            (
                "success",
                8192,
                Duration::from_secs(5),
                ExecutionResult::Succeeded,
            ),
            (
                "failed",
                8192,
                Duration::from_secs(5),
                ExecutionResult::Failed {
                    exit_code: Some(37),
                },
            ),
            (
                "output",
                1024,
                Duration::from_secs(5),
                ExecutionResult::Unknown {
                    stop_confirmed: false,
                    reason: "output_limit".to_owned(),
                },
            ),
            (
                "timeout",
                8192,
                Duration::from_millis(400),
                ExecutionResult::Unknown {
                    stop_confirmed: false,
                    reason: "timeout".to_owned(),
                },
            ),
        ] {
            let mut command = Command::new(std::env::current_exe().expect("取得原生测试入口"));
            command
                .args([
                    "--exact",
                    "worktree_recovery::execute::tests::execution_child_fixture",
                    "--nocapture",
                ])
                .env("SOXU_RECOVERY_EXECUTION_FIXTURE", mode)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true);
            assert_eq!(
                run_native(&mut command, limit, deadline, None).await,
                expected,
                "执行模式 {mode}"
            );
        }
    }
}
