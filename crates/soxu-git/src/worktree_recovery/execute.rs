//! 恢复执行：单次确认绑定、持久 journal、原生 Git 写入与重启只读复核（M3 #57-3）。
//!
//! 本模块只调用原生 Git（`worktree unlock` / `repair` / `prune` / `remove`），
//! 不手改 Git 元数据、不做 best-effort 清理。每次执行都绑定一次预览摘要与短期有效期：
//! 协调域持锁后重算预览并与确认逐项比对，任何漂移都作废该确认且零写入。
//! 写入前先持久化意图与阶段，命令结束后回读结果；无法确认结局时保留足迹并
//! 阻止同仓新执行，直到宿主按 journal 做只读裁决。

use std::fmt;
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;

use crate::probe::{COMMAND_TIMEOUT, OUTPUT_LIMIT};
use crate::worktree_lifecycle::{
    ExecutionResult, ExecutionStage, TargetState, WorktreeLifecycleError, state,
};
use crate::{
    GitError, GitProbe, LocalProject, ProjectOpenError, RecoveryOperation, WorktreeEntry,
    WorktreeHead, WorktreeIdentity, WorktreeInventory, WorktreeInventoryError, WorktreeKind,
    WorktreeRegistrationCoordinator,
};

use super::journal::{self, JournalRecord, RecoveryExecutionRecord, RecoveryNote};
use super::preview::{self, parse_prune_candidates, target_state_of};
use super::{
    EvidenceGap, ObservedWorktreeState, PartialCleanupRejection, PartialCleanupScope,
    PreviewObservation, PreviewOutcome, PruneRejection, RecoveryPreviewError,
    RecoveryPreviewRequest, RemoveRegistrationRejection, RemoveRegistrationScope, RepairRejection,
    UnlockRejection, WorktreeObservationKind,
};

/// 单次确认的最长有效期；宿主必须另取一次本机用户二次确认。
const MAX_CONFIRMATION_WINDOW: Duration = Duration::from_secs(600);
/// 允许的签发时钟偏移，防止用未来时间戳无限延长有效期。
const MAX_ISSUED_SKEW: Duration = Duration::from_secs(60);

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

/// 计算一次预览的摘要。宿主应把该值与预览一起展示，并由用户二次确认。
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
    RecoveryPreviewDigest(sha256(&bytes))
}

/// 单次执行确认。
///
/// 字段全部来自宿主已展示并获用户二次确认的预览；执行时不接受额外参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryConfirmation {
    /// 单次执行的持久身份；同 ID 重放一律拒绝。
    pub execution_id: String,
    pub operation: RecoveryOperation,
    pub preview_digest: RecoveryPreviewDigest,
    /// 预览使用的源仓库根（绝对路径）。
    pub source_root: PathBuf,
    /// 预览时记录的仓库身份；执行前重新打开必须一致。
    pub repository: WorktreeIdentity,
    /// 显式授权并已核对的单一目标；`prune` 与 `PartialCleanup` 的语义见文档。
    pub target: Option<PathBuf>,
    pub issued_at: SystemTime,
    pub expires_at: SystemTime,
    /// 高风险操作的独立二次确认：`prune` 多候选与在线移除必须为 `true`。
    pub risk_acknowledged: bool,
    /// 清理部分落盘时绑定的注册请求 ID。
    pub partial_cleanup_request_id: Option<String>,
}

/// 启动前的协作式取消标记。
///
/// 取消只在命令启动前被观察：一旦原生 Git 写入开始，结局仍会持久化到 journal。
#[derive(Debug, Clone, Default)]
pub struct RecoveryCancellation(Arc<AtomicBool>);

impl RecoveryCancellation {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// 命令结束后的只读回读事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryObservation {
    pub observed_at: SystemTime,
    pub inventory: WorktreeInventory,
    /// 绑定目标是否仍在清单中。
    pub target_registered: bool,
    /// 绑定目标的目录状态；无目标操作（`prune`）为 `NotExamined`。
    pub target_state: TargetState,
    /// `prune` 后仍报告的候补管理目录；其他操作恒为空。
    pub remaining_candidates: Vec<Vec<u8>>,
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
    #[error("高风险操作缺少独立二次确认")]
    ConfirmationRequired,
    #[error("清理部分落盘的范围无法证明，必须保留并报告")]
    UnprovableFootprint,
    #[error("执行已取消，确认未被消费")]
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
    /// 按单次确认执行一次恢复写入。
    ///
    /// 流程：校验有效期与字段 → 取协调域锁 → 拒绝重放与未裁决预约 → 重算预览并比对摘要
    /// → 复核仓库身份 → 二次确认判定 → 持久化 `Prepared`/`Started` → 原生 Git
    /// → 持久化命令结果 → 回读清单并分类 → 持久化 `Observed`。
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
    validate_confirmation(confirmation)?;
    let state_root = &coordinator.state_root;
    let lock = state::acquire_state_lock(state_root).await?;

    if let Some(existing) = journal::read(state_root, &confirmation.execution_id)? {
        return Err(RecoveryExecuteError::AlreadyAttempted(Box::new(
            existing.record,
        )));
    }
    for record in journal::read_all(state_root)? {
        if record.record.reserves_scope() && same_scope(&record.record, confirmation) {
            return Err(RecoveryExecuteError::ExecutionStillUnknown(
                record.record.execution_id,
            ));
        }
    }

    // 在同一协调域内重算预览：外部改动、授权复用或换目标都会命中摘要漂移。
    lock.refresh(state_root)?;
    let preview = recompute_preview(git, coordinator, confirmation).await?;
    let digest = recovery_preview_digest(&preview, confirmation.target.as_deref());
    if digest != confirmation.preview_digest {
        return Err(RecoveryExecuteError::PreviewDrift);
    }
    verify_repository_identity(git, confirmation).await?;
    let binding = bind_evidence(&preview, confirmation)?;
    if let Some(rejection) = binding.rejection {
        return Err(RecoveryExecuteError::PreviewRejected(rejection));
    }
    if binding.requires_second_confirmation && !confirmation.risk_acknowledged {
        return Err(RecoveryExecuteError::ConfirmationRequired);
    }
    if confirmation.operation == RecoveryOperation::PartialCleanup && !binding.nothing_to_clean {
        return Err(RecoveryExecuteError::UnprovableFootprint);
    }
    if cancellation.is_some_and(RecoveryCancellation::is_cancelled) {
        return Err(RecoveryExecuteError::Cancelled);
    }

    let mut record = RecoveryExecutionRecord {
        execution_id: confirmation.execution_id.clone(),
        operation: confirmation.operation,
        source: confirmation.repository.clone(),
        target: confirmation.target.clone(),
        stage: ExecutionStage::Prepared,
        result: ExecutionResult::NotStarted,
        recorded_at: SystemTime::now(),
        started_at: None,
        observed_at: None,
        note: RecoveryNote::NotStarted,
        blocked: false,
    };
    lock.refresh(state_root)?;
    journal::begin(
        state_root,
        &JournalRecord {
            record: record.clone(),
            preview_digest: digest.0,
        },
    )?;

    if confirmation.operation == RecoveryOperation::PartialCleanup {
        // 预览已证明没有可清理的宿主资产：只持久化结论，不触碰任何路径。
        record.stage = ExecutionStage::Observed;
        record.result = ExecutionResult::Succeeded;
        record.observed_at = Some(SystemTime::now());
        record.note = RecoveryNote::NothingToClean;
        lock.refresh(state_root)?;
        rewrite_journal(state_root, &record, digest.0)?;
        return Ok(RecoveryExecutionOutcome::NothingToClean { record });
    }

    let started_at = SystemTime::now();
    let mut started = record.clone();
    started.stage = ExecutionStage::Started;
    started.started_at = Some(started_at);
    started.result = ExecutionResult::Unknown {
        stop_confirmed: false,
        reason: "running".to_owned(),
    };
    lock.refresh(state_root)?;
    rewrite_journal(state_root, &started, digest.0)?;

    let command = match native_command(git, confirmation) {
        Ok(command) => command,
        Err(error) => {
            // 参数在启动前就无法构造：命令未运行，留下可作为结论的记录。
            let mut failed = started.clone();
            failed.stage = ExecutionStage::Observed;
            failed.started_at = None;
            failed.result = ExecutionResult::SpawnFailed;
            failed.observed_at = Some(SystemTime::now());
            failed.note = RecoveryNote::UnknownOutcome;
            failed.blocked = true;
            lock.refresh(state_root)?;
            rewrite_journal(state_root, &failed, digest.0)?;
            return Err(error);
        }
    };
    let result = match command {
        Some(mut command) => run_native(&mut command, OUTPUT_LIMIT, COMMAND_TIMEOUT).await,
        None => ExecutionResult::Succeeded,
    };

    let mut completed = started;
    completed.stage = ExecutionStage::Completed;
    completed.result = result.clone();
    if result == ExecutionResult::SpawnFailed {
        completed.started_at = None;
    }
    lock.refresh(state_root)?;
    rewrite_journal(state_root, &completed, digest.0)?;

    // 回读：命令成功也要与预期逐项比对；命令失败同样记录足迹，供宿主裁决。
    let observation = observe_after(git, confirmation).await.ok();
    let satisfied = observation
        .as_ref()
        .is_some_and(|observation| expectation_satisfied(confirmation, observation));
    let (note, outcome_observation) = classify(&result, observation, satisfied);

    let mut observed = completed;
    observed.stage = ExecutionStage::Observed;
    observed.observed_at = Some(SystemTime::now());
    observed.note = note;
    observed.blocked = matches!(
        note,
        RecoveryNote::PartialFootprint | RecoveryNote::CommandFailed | RecoveryNote::UnknownOutcome
    );
    lock.refresh(state_root)?;
    rewrite_journal(state_root, &observed, digest.0)?;
    drop(lock);

    Ok(match (note, outcome_observation) {
        (RecoveryNote::Verified, Some(observation)) => RecoveryExecutionOutcome::Completed {
            record: observed,
            observation,
        },
        (RecoveryNote::PartialFootprint, Some(observation)) => {
            RecoveryExecutionOutcome::PartialFootprint {
                record: observed,
                observation,
                command: result,
            }
        }
        (RecoveryNote::CommandFailed, observation) => RecoveryExecutionOutcome::Failed {
            record: observed,
            observation,
            command: result,
        },
        (_, observation) => RecoveryExecutionOutcome::UnknownOutcome {
            record: observed,
            observation,
            reason: unknown_reason(&result),
            command: result,
        },
    })
}

fn rewrite_journal(
    state_root: &crate::worktree_lifecycle::identity::AuthorizedDirectory,
    record: &RecoveryExecutionRecord,
    preview_digest: [u8; 32],
) -> Result<(), RecoveryExecuteError> {
    journal::rewrite(
        state_root,
        &JournalRecord {
            record: record.clone(),
            preview_digest,
        },
    )?;
    Ok(())
}

fn validate_confirmation(confirmation: &RecoveryConfirmation) -> Result<(), RecoveryExecuteError> {
    crate::worktree_lifecycle::validate_request_id(&confirmation.execution_id)?;
    if !confirmation.source_root.is_absolute() {
        return Err(RecoveryExecuteError::InvalidInput("源仓库根必须是绝对路径"));
    }
    if confirmation
        .target
        .as_ref()
        .is_some_and(|target| !target.is_absolute())
    {
        return Err(RecoveryExecuteError::InvalidInput("目标必须是绝对路径"));
    }
    match confirmation.operation {
        RecoveryOperation::Prune => {
            if confirmation.target.is_some() || confirmation.partial_cleanup_request_id.is_some() {
                return Err(RecoveryExecuteError::InvalidInput(
                    "prune 不绑定单一目标或清理请求",
                ));
            }
        }
        RecoveryOperation::PartialCleanup => {
            if confirmation.target.is_none() {
                return Err(RecoveryExecuteError::InvalidInput(
                    "清理部分落盘必须绑定目标目录",
                ));
            }
            let Some(request_id) = confirmation.partial_cleanup_request_id.as_deref() else {
                return Err(RecoveryExecuteError::InvalidInput(
                    "清理部分落盘必须绑定请求 ID",
                ));
            };
            crate::worktree_lifecycle::validate_request_id(request_id)?;
        }
        RecoveryOperation::Unlock
        | RecoveryOperation::Repair
        | RecoveryOperation::RemoveRegistration => {
            if confirmation.target.is_none() {
                return Err(RecoveryExecuteError::InvalidInput("该操作必须绑定单一目标"));
            }
            if confirmation.partial_cleanup_request_id.is_some() {
                return Err(RecoveryExecuteError::InvalidInput(
                    "该操作不接受清理请求 ID",
                ));
            }
        }
    }
    let Some(window) = confirmation
        .expires_at
        .duration_since(confirmation.issued_at)
        .ok()
    else {
        return Err(RecoveryExecuteError::InvalidInput(
            "确认有效期必须晚于签发时间",
        ));
    };
    let now = SystemTime::now();
    if now > confirmation.expires_at {
        return Err(RecoveryExecuteError::ConfirmationExpired);
    }
    if window > MAX_CONFIRMATION_WINDOW {
        return Err(RecoveryExecuteError::InvalidInput("确认有效期超过上限"));
    }
    if confirmation
        .issued_at
        .duration_since(now)
        .is_ok_and(|ahead| ahead > MAX_ISSUED_SKEW)
    {
        return Err(RecoveryExecuteError::InvalidInput("确认签发时间超前"));
    }
    Ok(())
}

async fn recompute_preview(
    git: &GitProbe,
    coordinator: &WorktreeRegistrationCoordinator,
    confirmation: &RecoveryConfirmation,
) -> Result<PreviewOutcome, RecoveryExecuteError> {
    Ok(preview::build(
        git,
        RecoveryPreviewRequest {
            source_root: &confirmation.source_root,
            coordinator_state_root: Some(coordinator.state_root.path()),
            target_authorization: confirmation.target.as_deref(),
            operation: confirmation.operation,
            partial_cleanup_request_id: confirmation.partial_cleanup_request_id.as_deref(),
        },
    )
    .await?)
}

async fn verify_repository_identity(
    git: &GitProbe,
    confirmation: &RecoveryConfirmation,
) -> Result<(), RecoveryExecuteError> {
    let LocalProject::GitWorktree(identity) =
        git.open_local_project(&confirmation.source_root).await?
    else {
        return Err(RecoveryExecuteError::InvalidInput("源根不是 Git 工作树"));
    };
    let expected = &confirmation.repository;
    if !paths_equivalent(&identity.root, &confirmation.source_root)
        || !paths_equivalent(&identity.root, &expected.root)
        || !paths_equivalent(&identity.git_dir, &expected.git_dir)
        || !paths_equivalent(&identity.common_dir, &expected.common_dir)
    {
        return Err(RecoveryExecuteError::IdentityChanged);
    }
    Ok(())
}

/// 预览证据与本确认的显式绑定。
struct OperationBinding {
    rejection: Option<&'static str>,
    requires_second_confirmation: bool,
    nothing_to_clean: bool,
}

fn bind_evidence(
    preview: &PreviewOutcome,
    confirmation: &RecoveryConfirmation,
) -> Result<OperationBinding, RecoveryExecuteError> {
    let binding = OperationBinding {
        rejection: rejection_code(preview),
        requires_second_confirmation: false,
        nothing_to_clean: false,
    };
    // 预览已拒绝时不再做目标绑定：宿主必须先解决拒绝原因，执行入口保持零写入。
    if binding.rejection.is_some() {
        return Ok(binding);
    }
    Ok(match preview {
        PreviewOutcome::Unlock(preview) => {
            require_bound_target(
                confirmation,
                &preview
                    .candidate
                    .path
                    .to_path_buf()
                    .map_err(|_| RecoveryExecuteError::InvalidInput("目标路径无法在本平台表示"))?,
            )?;
            binding
        }
        PreviewOutcome::Repair(preview) => OperationBinding {
            // 原生命令按仓库执行，只有唯一候选才可能把范围界定到一个工作树。
            requires_second_confirmation: preview.candidates.len() > 1,
            ..binding
        },
        PreviewOutcome::Prune(preview) => OperationBinding {
            requires_second_confirmation: preview.candidates.len() > 1,
            ..binding
        },
        PreviewOutcome::RemoveRegistration(preview) => {
            let path = match &preview.scope {
                RemoveRegistrationScope::OnlineClean { path, .. }
                | RemoveRegistrationScope::Disconnected { path, .. } => path,
            };
            let path = path
                .to_path_buf()
                .map_err(|_| RecoveryExecuteError::InvalidInput("目标路径无法在本平台表示"))?;
            require_bound_target(confirmation, &path)?;
            OperationBinding {
                // 在线移除会同时删除工作目录与管理数据，必须独立二次确认。
                requires_second_confirmation: matches!(
                    preview.scope,
                    RemoveRegistrationScope::OnlineClean { .. }
                ),
                ..binding
            }
        }
        PreviewOutcome::PartialCleanup(preview) => OperationBinding {
            nothing_to_clean: matches!(preview.scope, PartialCleanupScope::TargetAbsent),
            ..binding
        },
    })
}

/// 确认携带的目标必须与预览候选完全一致；换目标即视为授权复用并拒绝。
fn require_bound_target(
    confirmation: &RecoveryConfirmation,
    expected: &Path,
) -> Result<(), RecoveryExecuteError> {
    let Some(target) = confirmation.target.as_ref() else {
        return Err(RecoveryExecuteError::InvalidInput("该操作必须绑定单一目标"));
    };
    if !paths_equivalent(target, expected) {
        return Err(RecoveryExecuteError::PreviewDrift);
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

fn same_scope(record: &RecoveryExecutionRecord, confirmation: &RecoveryConfirmation) -> bool {
    if paths_equivalent(&record.source.root, &confirmation.source_root)
        || paths_equivalent(
            &record.source.common_dir,
            &confirmation.repository.common_dir,
        )
    {
        return true;
    }
    record
        .target
        .as_ref()
        .zip(confirmation.target.as_ref())
        .is_some_and(|(left, right)| paths_equivalent(left, right))
}

fn native_command(
    git: &GitProbe,
    confirmation: &RecoveryConfirmation,
) -> Result<Option<Command>, RecoveryExecuteError> {
    if confirmation.operation == RecoveryOperation::PartialCleanup {
        return Ok(None);
    }
    let mut command = git.command(&confirmation.source_root);
    command.arg("worktree");
    match confirmation.operation {
        RecoveryOperation::Prune => {
            command.arg("prune").arg("--verbose");
        }
        RecoveryOperation::Unlock => {
            command.arg("unlock").arg(native_target(confirmation)?);
        }
        RecoveryOperation::Repair => {
            command.arg("repair").arg(native_target(confirmation)?);
        }
        RecoveryOperation::RemoveRegistration => {
            command.arg("remove").arg(native_target(confirmation)?);
        }
        RecoveryOperation::PartialCleanup => return Ok(None),
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    Ok(Some(command))
}

fn native_target(confirmation: &RecoveryConfirmation) -> Result<PathBuf, RecoveryExecuteError> {
    let target = confirmation
        .target
        .as_deref()
        .ok_or(RecoveryExecuteError::InvalidInput("该操作必须绑定单一目标"))?;
    Ok(crate::worktree_lifecycle::native_command_path(target)?)
}

async fn observe_after(
    git: &GitProbe,
    confirmation: &RecoveryConfirmation,
) -> Result<RecoveryObservation, RecoveryExecuteError> {
    let inventory = git.list_worktrees(&confirmation.source_root).await?;
    let target_state = confirmation
        .target
        .as_deref()
        .map_or(TargetState::NotExamined, target_state_of);
    let target_registered = confirmation
        .target
        .as_ref()
        .is_some_and(|target| inventory_entry(&inventory, target).is_some());
    let mut remaining_candidates = Vec::new();
    if confirmation.operation == RecoveryOperation::Prune {
        // 与预览同源的 dry-run：执行后仍报告的候补即部分足迹或并发新增。
        let output = git
            .run_command(
                &confirmation.source_root,
                "worktree",
                &["prune", "--dry-run", "--verbose"],
            )
            .await?;
        if !output.status.success() {
            return Err(GitError::CommandFailed {
                operation: "worktree",
                code: output.status.code(),
                stderr: output.stderr,
            }
            .into());
        }
        for candidate in parse_prune_candidates(&output.stdout)
            .into_iter()
            .chain(parse_prune_candidates(&output.stderr))
        {
            if !remaining_candidates.contains(&candidate.admin_dir) {
                remaining_candidates.push(candidate.admin_dir);
            }
        }
    }
    Ok(RecoveryObservation {
        observed_at: SystemTime::now(),
        inventory,
        target_registered,
        target_state,
        remaining_candidates,
    })
}

fn inventory_entry<'a>(
    inventory: &'a WorktreeInventory,
    target: &Path,
) -> Option<&'a WorktreeEntry> {
    inventory.entries.iter().find(|entry| {
        entry
            .path
            .to_path_buf()
            .is_ok_and(|path| paths_equivalent(&path, target))
    })
}

/// 每类操作的预期终态；回读不符即部分足迹。
fn expectation_satisfied(
    confirmation: &RecoveryConfirmation,
    observation: &RecoveryObservation,
) -> bool {
    let target = confirmation.target.as_deref();
    match confirmation.operation {
        RecoveryOperation::Unlock => target.is_some_and(|target| {
            inventory_entry(&observation.inventory, target)
                .is_some_and(|entry| entry.locked.is_none())
        }),
        RecoveryOperation::Repair => target.is_some_and(|target| {
            observation.target_state == TargetState::Directory
                && inventory_entry(&observation.inventory, target)
                    .is_some_and(|entry| entry.prunable.is_none())
                && !observation
                    .inventory
                    .entries
                    .iter()
                    .any(|entry| entry.prunable.is_some())
        }),
        RecoveryOperation::Prune => {
            observation.remaining_candidates.is_empty()
                && !observation
                    .inventory
                    .entries
                    .iter()
                    .any(|entry| entry.prunable.is_some())
        }
        // 删除后结果：注册消失且目标目录已不存在才算一致。
        RecoveryOperation::RemoveRegistration => {
            !observation.target_registered && observation.target_state == TargetState::Absent
        }
        RecoveryOperation::PartialCleanup => {
            confirmation.target.is_some() && observation.target_state == TargetState::Absent
        }
    }
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
        ExecutionResult::Unknown {
            stop_confirmed: false,
            ..
        } => "stop_unconfirmed",
        ExecutionResult::Unknown { .. } => "unknown",
        _ => "readback_failed",
    }
}

/// 有界运行原生 Git：读取独占管道，超时或读取失败先释放管道再停止子进程。
async fn run_native(command: &mut Command, limit: u64, timeout: Duration) -> ExecutionResult {
    let Ok(mut child) = command.spawn() else {
        return ExecutionResult::SpawnFailed;
    };
    let result = {
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let child = &mut child;
        tokio::time::timeout(timeout, async move {
            let stdout = stdout.ok_or("missing_stdout")?;
            let stderr = stderr.ok_or("missing_stderr")?;
            let (_, _, status) = tokio::try_join!(
                count_output(stdout, limit),
                count_output(stderr, limit),
                async { child.wait().await.map_err(|_| "wait_io") },
            )?;
            Ok(status)
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
    let stop_confirmed = match child.try_wait() {
        Ok(Some(_)) => true,
        _ => {
            let _ = child.start_kill();
            let reap_timeout = timeout.max(Duration::from_secs(2));
            matches!(
                tokio::time::timeout(reap_timeout, child.wait()).await,
                Ok(Ok(_))
            )
        }
    };
    ExecutionResult::Unknown {
        stop_confirmed,
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

fn paths_equivalent(left: &Path, right: &Path) -> bool {
    let left: Vec<_> = left.components().collect();
    let right: Vec<_> = right.components().collect();
    left.len() == right.len()
        && left.iter().zip(&right).all(|(left, right)| {
            match (component_bytes(*left), component_bytes(*right)) {
                (Some(left), Some(right)) if left.is_ascii() && right.is_ascii() => {
                    left.eq_ignore_ascii_case(right)
                }
                (Some(left), Some(right)) => left == right,
                (None, None) => left == right,
                _ => false,
            }
        })
}

fn component_bytes(component: Component<'_>) -> Option<&[u8]> {
    match component {
        Component::Normal(path) => Some(path.as_encoded_bytes()),
        _ => None,
    }
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

// ------------------------------------------------------------------- SHA-256

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

fn sha256(data: &[u8]) -> [u8; 32] {
    let mut state: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let bit_length = (data.len() as u64).wrapping_mul(8);
    let mut message = data.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_length.to_be_bytes());
    for chunk in message.as_chunks::<64>().0 {
        let mut words = [0u32; 64];
        for (index, bytes) in chunk.as_chunks::<4>().0.iter().enumerate() {
            words[index] = u32::from_be_bytes(*bytes);
        }
        for index in 16..64 {
            let s0 = words[index - 15].rotate_right(7)
                ^ words[index - 15].rotate_right(18)
                ^ (words[index - 15] >> 3);
            let s1 = words[index - 2].rotate_right(17)
                ^ words[index - 2].rotate_right(19)
                ^ (words[index - 2] >> 10);
            words[index] = words[index - 16]
                .wrapping_add(s0)
                .wrapping_add(words[index - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = state;
        for index in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choose = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(choose)
                .wrapping_add(K[index])
                .wrapping_add(words[index]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(majority);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        for (slot, value) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *slot = slot.wrapping_add(value);
        }
    }
    let mut digest = [0u8; 32];
    for (chunk, value) in digest.as_chunks_mut::<4>().0.iter_mut().zip(state) {
        *chunk = value.to_be_bytes();
    }
    digest
}

#[cfg(test)]
mod tests {
    use std::process::Stdio;
    use std::time::Duration;

    use super::*;

    #[test]
    fn sha256_matches_known_vectors() {
        assert_eq!(
            RecoveryPreviewDigest(sha256(b"")).to_hex(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            RecoveryPreviewDigest(sha256(b"abc")).to_hex(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            RecoveryPreviewDigest(sha256(
                b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
            ))
            .to_hex(),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

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
    fn confirmation_window_and_field_binding_fail_closed() {
        let now = SystemTime::now();
        // 绝对路径取自宿主临时目录，避免把 Unix 风格常量当成 Windows 绝对路径。
        let root = std::env::temp_dir().join("soxu-exec-confirmation");
        let target = root.join("wt");
        let repository = WorktreeIdentity {
            root: root.clone(),
            git_dir: root.join(".git"),
            common_dir: root.join(".git"),
        };
        let make = |operation, with_target: bool, issued, expires| RecoveryConfirmation {
            execution_id: "exec-window".to_owned(),
            operation,
            preview_digest: RecoveryPreviewDigest([0; 32]),
            source_root: root.clone(),
            repository: repository.clone(),
            target: with_target.then(|| target.clone()),
            issued_at: issued,
            expires_at: expires,
            risk_acknowledged: false,
            partial_cleanup_request_id: None,
        };
        assert!(
            validate_confirmation(&make(
                RecoveryOperation::Unlock,
                true,
                now,
                now + Duration::from_secs(60)
            ))
            .is_ok()
        );
        let expired = make(
            RecoveryOperation::Unlock,
            true,
            now - Duration::from_secs(3600),
            now - Duration::from_secs(1),
        );
        assert!(matches!(
            validate_confirmation(&expired),
            Err(RecoveryExecuteError::ConfirmationExpired)
        ));
        let too_long = make(
            RecoveryOperation::Unlock,
            true,
            now,
            now + MAX_CONFIRMATION_WINDOW + Duration::from_secs(1),
        );
        assert!(matches!(
            validate_confirmation(&too_long),
            Err(RecoveryExecuteError::InvalidInput(_))
        ));
        let future = make(
            RecoveryOperation::Unlock,
            true,
            now + Duration::from_secs(600),
            now + Duration::from_secs(900),
        );
        assert!(matches!(
            validate_confirmation(&future),
            Err(RecoveryExecuteError::InvalidInput(_))
        ));
        // 操作与目标/请求 ID 的绑定必须一致。
        let prune_with_target = make(
            RecoveryOperation::Prune,
            true,
            now,
            now + Duration::from_secs(60),
        );
        assert!(matches!(
            validate_confirmation(&prune_with_target),
            Err(RecoveryExecuteError::InvalidInput(_))
        ));
        let unlock_without_target = make(
            RecoveryOperation::Unlock,
            false,
            now,
            now + Duration::from_secs(60),
        );
        assert!(matches!(
            validate_confirmation(&unlock_without_target),
            Err(RecoveryExecuteError::InvalidInput(_))
        ));
        let cleanup_without_request = make(
            RecoveryOperation::PartialCleanup,
            true,
            now,
            now + Duration::from_secs(60),
        );
        assert!(matches!(
            validate_confirmation(&cleanup_without_request),
            Err(RecoveryExecuteError::InvalidInput(_))
        ));
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
            remaining_candidates: Vec::new(),
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
            "success" => {}
            _ => panic!("未知执行夹具模式"),
        }
    }

    #[tokio::test]
    async fn native_runner_separates_spawn_exit_limits_and_confirmed_stops() {
        let executable = std::env::current_exe().expect("取得原生测试入口");
        let mut missing = Command::new(executable.join("missing-program"));
        assert_eq!(
            run_native(&mut missing, 1024, Duration::from_secs(5)).await,
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
                    stop_confirmed: true,
                    reason: "output_limit".to_owned(),
                },
            ),
            (
                "timeout",
                8192,
                Duration::from_millis(400),
                ExecutionResult::Unknown {
                    stop_confirmed: true,
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
                run_native(&mut command, limit, deadline).await,
                expected,
                "执行模式 {mode}"
            );
        }
    }

    /// 进程树未收敛的持久记录必须阻断同仓新执行；重启诊断只读不重放。
    #[tokio::test]
    #[cfg(target_os = "linux")]
    async fn unconfirmed_stop_reserves_scope_and_is_diagnosed_without_replay() {
        use std::os::unix::fs::DirBuilderExt;

        let root = std::env::temp_dir().join(format!("soxu-exec-reserve-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("创建夹具根");
        let state = root.join("state");
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&state)
            .expect("创建私有状态根");
        let coordinator = WorktreeRegistrationCoordinator::open(&state, &[]).expect("打开协调域");
        let repository_root = root.join("repo");
        let started = RecoveryExecutionRecord {
            execution_id: "exec-unknown".to_owned(),
            operation: RecoveryOperation::Unlock,
            source: WorktreeIdentity {
                root: repository_root.clone(),
                git_dir: repository_root.join(".git"),
                common_dir: repository_root.join(".git"),
            },
            target: Some(root.join("wt")),
            stage: ExecutionStage::Started,
            result: ExecutionResult::Unknown {
                stop_confirmed: false,
                reason: "timeout".to_owned(),
            },
            recorded_at: SystemTime::now(),
            started_at: Some(SystemTime::now()),
            observed_at: None,
            note: RecoveryNote::NotStarted,
            blocked: false,
        };
        journal::begin(
            &coordinator.state_root,
            &JournalRecord {
                record: started.clone(),
                preview_digest: [0; 32],
            },
        )
        .expect("持久化 Started 记录");

        let confirmation = RecoveryConfirmation {
            execution_id: "exec-next".to_owned(),
            operation: RecoveryOperation::Unlock,
            preview_digest: RecoveryPreviewDigest([0; 32]),
            source_root: repository_root.clone(),
            repository: started.source.clone(),
            target: Some(root.join("wt")),
            issued_at: SystemTime::now(),
            expires_at: SystemTime::now() + Duration::from_secs(60),
            risk_acknowledged: false,
            partial_cleanup_request_id: None,
        };
        let git = GitProbe::discover().expect("测试需要原生 Git");
        assert!(matches!(
            git.execute_recovery(&coordinator, &confirmation, None).await,
            Err(RecoveryExecuteError::ExecutionStillUnknown(id)) if id == "exec-unknown"
        ));
        let records = journal::read_all(&coordinator.state_root).expect("读取全部记录");
        assert_eq!(records.len(), 1, "拒绝路径不得写入新记录");
        assert!(records[0].record.reserves_scope());
        assert_eq!(
            git.diagnose_recovery(&coordinator, "exec-unknown")
                .expect("只读诊断")
                .expect("记录必须存在"),
            started
        );

        // 明确的非零退出不预约范围：宿主重新预览并取得新确认后可重试。
        let mut failed = started.clone();
        failed.execution_id = "exec-failed".to_owned();
        failed.stage = ExecutionStage::Observed;
        failed.result = ExecutionResult::Failed {
            exit_code: Some(128),
        };
        failed.note = RecoveryNote::CommandFailed;
        failed.blocked = true;
        assert!(!failed.reserves_scope());
        std::fs::remove_dir_all(&root).expect("清理夹具");
    }
}
