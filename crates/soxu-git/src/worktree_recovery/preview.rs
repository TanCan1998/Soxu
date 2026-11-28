//! 恢复预览契约的观察状态判定与五类操作影响证明。
//!
//! 判定只消费三类只读证据：
//! 1. 原生清单（`worktree list --porcelain -z`）的 locked / prunable 注解与 HEAD 状态；
//! 2. 协调域内的持久意图与检出留痕（只读回查，不写状态根）；
//! 3. 显式授权后的单目标目录状态（与 lifecycle 同款分类，不枚举内容）。
//!
//! 证据不足时输出证据缺口并拒绝，不推断磁盘状态。

use std::path::{Path, PathBuf};

use crate::worktree_lifecycle::identity::checked_directory;
use crate::worktree_lifecycle::{
    ExecutionResult, ExecutionStage, PersistentIntent, TargetState, read_intents,
};
use crate::{
    GitError, GitProbe, ProjectOpenError, WorktreeEntry, WorktreeHead, WorktreeInventory,
    WorktreeInventoryError, WorktreeKind, WorktreeLifecycleError,
};

use super::{RecoveryOperation, validate_source_root};

/// 预览错误：区分输入、仓库身份与底层诊断失败。
///
/// 拒绝路径（范围无法证明、证据缺口）属于预览结果而非错误；
/// 错误表示连只读观察都无法可靠完成。
#[derive(Debug, thiserror::Error)]
pub enum RecoveryPreviewError {
    #[error("恢复预览请求字段无效：{0}")]
    InvalidInput(&'static str),
    #[error("源仓库或协调域诊断失败：{0}")]
    Diagnostic(#[from] WorktreeLifecycleError),
    #[error("源仓库打开或 Git 诊断失败：{0}")]
    Git(#[from] GitError),
    #[error("源仓库身份不可用：{0}")]
    Project(#[from] ProjectOpenError),
    #[error(transparent)]
    Inventory(#[from] WorktreeInventoryError),
}

/// 单次恢复预览请求。
///
/// 源仓库根必须是已打开的工作树根；协调域可选——提供时才回查意图留痕。
/// `target_authorization` 只能来自宿主持有的注册意图或授权路径，
/// 预览不打开未授权的任意位置。
#[derive(Debug, Clone)]
pub struct RecoveryPreviewRequest<'a> {
    pub source_root: &'a Path,
    pub coordinator_state_root: Option<&'a Path>,
    /// 显式授权核对的单个目标路径。
    pub target_authorization: Option<&'a Path>,
    /// 待预览的操作；一次请求只预览一个操作。
    pub operation: RecoveryOperation,
    /// 清理部分落盘时绑定的注册请求 ID；其他操作忽略。
    pub partial_cleanup_request_id: Option<&'a str>,
}

/// 预览快照：可确认事实 + 证据缺口。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewObservation {
    /// 清单条目数（含主工作树）。
    pub inventory_entry_count: usize,
    /// 全部观察状态的逐条判定，顺序与清单一致。
    pub states: Vec<ObservedWorktreeState>,
    /// 未能确认的事实；宿主必须在授权摘要中如实展示。
    pub evidence_gaps: Vec<EvidenceGap>,
    /// 显式授权目标的目录状态；未授权时为 `NotExamined`。
    pub target_state: TargetState,
}

/// 设计文档第 5 节表格的观察状态判定。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedWorktreeState {
    /// 清单中的原始路径字节身份。
    pub path: crate::WorktreePath,
    pub kind: WorktreeKind,
    pub head: WorktreeHead,
    pub state: WorktreeObservationKind,
    /// locked / prunable 原始理由字节（如有）。
    pub annotation: Option<Vec<u8>>,
}

/// 观察状态分类（设计文档第 5 节表格行）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorktreeObservationKind {
    /// `locked` 且协调域意图可回查到同一注册。
    ValidProtectedLock,
    /// `locked` 但无任务租约证据可回查（协调域缺席或无匹配意图）。
    SuspectedOrphanLock,
    /// 清单报告 prunable，或授权目标观察显示路径不可达。
    PrunableOrUnreachable,
    /// 注册在卷离线或身份漂移下无法核对（目标授权存在但不可达/类型异常）。
    VolumeOfflineOrIdentityDrift,
    /// `Unavailable`、bare 或 HEAD/对象异常。
    HeadOrRepositoryAnomaly,
    /// `Prepared`/`Started` 执行结果未定。
    RegistrationIndeterminate,
    /// `Pending` 或 `Disabled` 检出。
    CheckoutPendingOrDisabled,
    /// 清单健康、无注解且无授权目标异常。
    Healthy,
}

/// 证据缺口分类，逐项展示给授权人。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceGap {
    /// 清单不探访注册路径，路径存在性未核实。
    PathPresenceUnverified { path: crate::WorktreePath },
    /// 协调域未提供或无匹配意图，锁租约无法确认。
    LockLeaseUnconfirmed { path: crate::WorktreePath },
    /// 意图处于 Prepared/Started 或结果未知，注册执行结局未知。
    RegistrationOutcomeUnknown { request_id: String },
    /// 检出留痕 Pending 或 Disabled，检出写入历史未知。
    CheckoutHistoryUnknown { request_id: String },
    /// 目标授权未提供，目标目录状态未核实。
    TargetNotExamined { path: crate::WorktreePath },
}

/// 五类操作的预览结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewOutcome {
    Unlock(UnlockPreview),
    Repair(RepairPreview),
    Prune(PrunePreview),
    RemoveRegistration(RemoveRegistrationPreview),
    PartialCleanup(PartialCleanupPreview),
}

/// `unlock` 预览：唯一 linked 注册 + 锁理由 + 失去锁保护的影响说明。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnlockPreview {
    pub observation: PreviewObservation,
    pub candidate: UnlockPreviewCandidate,
    /// 拒绝原因；非空表示本预览不可授权执行。
    pub rejection: Option<UnlockRejection>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnlockPreviewCandidate {
    pub path: crate::WorktreePath,
    /// 原始锁理由字节（如有）。
    pub lock_reason: Option<Vec<u8>>,
    /// 失去锁保护后的影响说明（固定文案，不推断租约状态）。
    pub protection_loss_notice: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnlockRejection {
    /// 没有可解锁的 linked 注册。
    NoLinkedRegistration,
    /// 该注册当前没有锁。
    NotLocked,
    /// 未知所有者、卷离线或记录不一致时保留锁（设计文档第 5 节）。
    LockEvidenceInsufficient,
}

/// `repair` 预览：已知移动前后身份的候选集合。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepairPreview {
    pub observation: PreviewObservation,
    pub candidates: Vec<RepairPreviewCandidate>,
    /// 拒绝原因；非空表示本预览不可授权执行。
    pub rejection: Option<RepairRejection>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepairPreviewCandidate {
    /// 清单中登记的原路径字节。
    pub registered_path: crate::WorktreePath,
    /// 显式授权并核对过的新路径。
    pub moved_to: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepairRejection {
    /// 原生 repair 按仓库执行，无法把触及范围界定到单个工作树。
    ScopeUnbounded,
    /// 无任何已证实的移动候选。
    NoProvenCandidate,
}

/// `prune` 预览：固定过期策略 dry-run 加前后清单。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrunePreview {
    pub observation: PreviewObservation,
    /// dry-run --verbose 输出的候选记录（原始字节，逐条保留）。
    pub candidates: Vec<PrunePreviewCandidate>,
    /// 预览后的完整清单，供执行前漂移比对。
    pub inventory_after: WorktreeInventory,
    /// 拒绝原因；非空表示本预览不可授权执行。
    pub rejection: Option<PruneRejection>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrunePreviewCandidate {
    /// 原生输出中的管理目录相对标识（如 `worktrees/wt1`）。
    pub admin_dir: Vec<u8>,
    /// 原生理由字节（如 `gitdir file points to non-existent location`）。
    pub reason: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PruneRejection {
    /// 存在无法确认在线状态的锁定注册：锁定的失联注册不出现在 prune 候选中，
    /// 授权人可能误以为它受保护且健康；须先裁决锁归属再授权整仓 prune。
    LockedRegistrationUnadjudicated,
}

/// 移除注册预览：区分在线干净 linked 与失联注册。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoveRegistrationPreview {
    pub observation: PreviewObservation,
    pub scope: RemoveRegistrationScope,
    /// 拒绝原因；非空表示本预览不可授权执行。
    pub rejection: Option<RemoveRegistrationRejection>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoveRegistrationScope {
    /// 在线 linked 工作树；删除涉及工作目录与管理数据。
    OnlineClean {
        path: crate::WorktreePath,
        /// 源仓库 status 无变更记录时为 true；false 时不授权普通 remove。
        clean: bool,
        locked: bool,
    },
    /// 失联注册；只允许走受控 prune，禁止手工删除管理目录。
    Disconnected {
        path: crate::WorktreePath,
        prunable_reason: Option<Vec<u8>>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoveRegistrationRejection {
    NoLinkedRegistration,
    /// 检查失败或分支/对象风险不明时拒绝（设计文档第 5 节）。
    CleanlinessUnproven,
    /// 锁保护的注册先走 unlock 审定。
    Locked,
    /// HEAD 异常等健康信号不明。
    HealthUncertain,
}

/// 清理部分落盘预览：对照既有请求的足迹身份。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartialCleanupPreview {
    pub observation: PreviewObservation,
    pub request_id: String,
    pub scope: PartialCleanupScope,
    /// 拒绝原因；非空表示本预览不可授权执行。
    pub rejection: Option<PartialCleanupRejection>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PartialCleanupScope {
    /// 意图留痕显示目标仍缺失：无宿主资产需要清理。
    TargetAbsent,
    /// 目标含非预期内容，归属无法由只读预览证明。
    UnprovableContent { target: PathBuf },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartialCleanupRejection {
    /// 协调域无该请求的意图留痕。
    IntentNotFound,
    /// 意图记录缺少目录身份（旧版状态根）。
    IntentIdentityMissing,
    /// 注册执行结果未知时保留预约，禁止清理。
    ExecutionUnknown,
    /// 目标状态未授权核对或含他人内容。
    TargetUnprovable,
}

/// 构建一次预览。
///
/// 流程（全程只读）：
/// 1. 校验源根并打开身份（复用 `open_local_project`）；
/// 2. 取 `worktree list --porcelain -z` 清单并逐条分类观察状态；
/// 3. 提供协调域时只读回查意图，补充证据缺口；
/// 4. 提供目标授权时核对一次目标目录状态；
/// 5. 按操作类型组装影响证明与拒绝原因。
pub(super) async fn build(
    git: &GitProbe,
    request: RecoveryPreviewRequest<'_>,
) -> Result<PreviewOutcome, RecoveryPreviewError> {
    validate_source_root(request.source_root)?;
    validate_request(&request)?;
    let identity = match git.open_local_project(request.source_root).await? {
        crate::LocalProject::GitWorktree(identity) => identity,
        crate::LocalProject::Directory { .. } => {
            return Err(RecoveryPreviewError::InvalidInput("源根不是 Git 工作树"));
        }
    };
    let inventory = git.list_worktrees(&identity.root).await?;
    let intents = read_intents_readonly(request.coordinator_state_root)?;
    let checkout_status = read_checkout_status(
        request.coordinator_state_root,
        request.partial_cleanup_request_id,
    )?;
    let checkout_status = &checkout_status;
    let target_state = observe_target(request.target_authorization)?;

    let mut states = Vec::new();
    let mut gaps = Vec::new();
    for entry in &inventory.entries {
        let state = classify_entry(
            entry,
            &intents,
            checkout_status.as_ref(),
            target_state,
            &mut gaps,
        );
        states.push(ObservedWorktreeState {
            path: entry.path.clone(),
            kind: entry.kind,
            head: entry.head.clone(),
            state,
            annotation: annotation_bytes(entry),
        });
    }

    let observation = PreviewObservation {
        inventory_entry_count: inventory.entries.len(),
        states,
        evidence_gaps: gaps,
        target_state: target_state.unwrap_or(TargetState::NotExamined),
    };

    let outcome = match request.operation {
        RecoveryOperation::Unlock => {
            PreviewOutcome::Unlock(unlock_preview(&inventory, observation))
        }
        RecoveryOperation::Repair => {
            PreviewOutcome::Repair(repair_preview(&inventory, observation, target_state))
        }
        RecoveryOperation::Prune => {
            PreviewOutcome::Prune(prune_preview(git, &identity.root, observation).await?)
        }
        RecoveryOperation::RemoveRegistration => PreviewOutcome::RemoveRegistration(
            remove_registration_preview(
                git,
                &identity.root,
                &inventory,
                observation,
                request.target_authorization,
                target_state,
            )
            .await?,
        ),
        RecoveryOperation::PartialCleanup => {
            PreviewOutcome::PartialCleanup(partial_cleanup_preview(
                &intents,
                request
                    .partial_cleanup_request_id
                    .expect("validate_request 已保证非空"),
                observation,
                target_state,
            ))
        }
    };
    Ok(outcome)
}

fn validate_request(request: &RecoveryPreviewRequest<'_>) -> Result<(), RecoveryPreviewError> {
    if request.operation == RecoveryOperation::PartialCleanup {
        let Some(request_id) = request.partial_cleanup_request_id else {
            return Err(RecoveryPreviewError::InvalidInput(
                "清理部分落盘必须绑定请求 ID",
            ));
        };
        crate::worktree_lifecycle::validate_request_id(request_id)
            .map_err(RecoveryPreviewError::Diagnostic)?;
    }
    Ok(())
}

/// 只读读取协调域内全部意图留痕。
///
/// 复用 lifecycle 的解码、上限与校验；状态根缺失（尚未建立协调域）视为无记录，
/// 其余读取失败向上传播——预览必须 fail-closed，不降级为「假定无意图」。
fn read_intents_readonly(
    state_root: Option<&Path>,
) -> Result<Vec<PersistentIntent>, RecoveryPreviewError> {
    let Some(state_root) = state_root else {
        return Ok(Vec::new());
    };
    let state_root = match checked_directory(state_root) {
        Ok(directory) => directory,
        Err(WorktreeLifecycleError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Vec::new());
        }
        Err(error) => return Err(error.into()),
    };
    Ok(read_intents(&state_root)?)
}

/// 只读读取检出留痕；仅清理部分落盘需要。
fn read_checkout_status(
    state_root: Option<&Path>,
    request_id: Option<&str>,
) -> Result<Option<crate::worktree_checkout::CheckoutStatus>, RecoveryPreviewError> {
    let (Some(state_root), Some(request_id)) = (state_root, request_id) else {
        return Ok(None);
    };
    let state_root = match checked_directory(state_root) {
        Ok(directory) => directory,
        Err(WorktreeLifecycleError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(None);
        }
        Err(error) => return Err(error.into()),
    };
    match crate::worktree_checkout::read_checkout_journal(state_root.path(), request_id) {
        Ok(status) => Ok(Some(status)),
        // 尚无检出记录（Pending 语义）不是错误。
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(RecoveryPreviewError::Diagnostic(
            WorktreeLifecycleError::Io(error),
        )),
    }
}

/// 显式授权目标的单次目录观察。
fn observe_target(target: Option<&Path>) -> Result<Option<TargetState>, RecoveryPreviewError> {
    let Some(target) = target else {
        return Ok(None);
    };
    if !target.is_absolute() {
        return Err(RecoveryPreviewError::InvalidInput("目标授权必须是绝对路径"));
    }
    Ok(Some(target_state_of(target)))
}

pub(super) fn target_state_of(target: &Path) -> TargetState {
    match target.symlink_metadata() {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => TargetState::Absent,
        Err(_) => TargetState::Inaccessible,
        Ok(metadata) if metadata.is_symlink() || is_reparse(&metadata) => {
            TargetState::LinkOrReparse
        }
        Ok(metadata) if metadata.is_dir() => TargetState::Directory,
        Ok(_) => TargetState::File,
    }
}

fn is_reparse(metadata: &std::fs::Metadata) -> bool {
    crate::worktree_lifecycle::identity::is_reparse(metadata)
}

fn annotation_bytes(entry: &WorktreeEntry) -> Option<Vec<u8>> {
    let reason = entry
        .locked
        .as_ref()
        .and_then(|annotation| annotation.reason.as_deref())
        .or_else(|| {
            entry
                .prunable
                .as_ref()
                .and_then(|annotation| annotation.reason.as_deref())
        })?;
    Some(reason.to_vec())
}

/// 单条清单判定的核心分类。
///
/// 优先级：意图结果未定 > prunable > 目标不可达/异常 > 锁 > HEAD 异常 > 健康。
/// 检出留痕 Pending/Disabled 作为独立状态报告（设计文档第 5 节最后一行）。
fn classify_entry(
    entry: &WorktreeEntry,
    intents: &[PersistentIntent],
    checkout_status: Option<&crate::worktree_checkout::CheckoutStatus>,
    target_state: Option<TargetState>,
    gaps: &mut Vec<EvidenceGap>,
) -> WorktreeObservationKind {
    // 意图匹配：目标路径与清单路径组件级等价视为同一注册。
    let intent = intents.iter().find(|intent| {
        entry
            .path
            .to_path_buf()
            .is_ok_and(|path| paths_equivalent(&path, &intent.target))
    });
    if let Some(intent) = intent {
        let indeterminate = intent.execution.stage == ExecutionStage::Prepared
            || intent.execution.stage == ExecutionStage::Started
            || matches!(
                &intent.execution.result,
                ExecutionResult::Unknown { .. } | ExecutionResult::SpawnFailed
            );
        if indeterminate {
            gaps.push(EvidenceGap::RegistrationOutcomeUnknown {
                request_id: intent.request_id.clone(),
            });
            return WorktreeObservationKind::RegistrationIndeterminate;
        }
    }

    if entry.prunable.is_some() {
        return WorktreeObservationKind::PrunableOrUnreachable;
    }

    if let Some(state) = target_state {
        match state {
            TargetState::Absent | TargetState::Inaccessible | TargetState::LinkOrReparse => {
                return WorktreeObservationKind::VolumeOfflineOrIdentityDrift;
            }
            TargetState::File => {
                return WorktreeObservationKind::HeadOrRepositoryAnomaly;
            }
            TargetState::Directory | TargetState::NotExamined => {}
        }
    } else if !matches!(entry.kind, WorktreeKind::Main) && intent.is_none() {
        gaps.push(EvidenceGap::PathPresenceUnverified {
            path: entry.path.clone(),
        });
    }

    if let Some(intent) = intent {
        // 检出留痕：Pending（含缺失）或 Disabled 都不声称检出完成。
        match checkout_status {
            Some(crate::worktree_checkout::CheckoutStatus::Pending) => {
                gaps.push(EvidenceGap::CheckoutHistoryUnknown {
                    request_id: intent.request_id.clone(),
                });
            }
            Some(crate::worktree_checkout::CheckoutStatus::Disabled { .. }) => {
                gaps.push(EvidenceGap::CheckoutHistoryUnknown {
                    request_id: intent.request_id.clone(),
                });
                return WorktreeObservationKind::CheckoutPendingOrDisabled;
            }
            Some(crate::worktree_checkout::CheckoutStatus::Verified) | None => {}
        }
    }

    if entry.locked.is_some() {
        // 锁租约证据分级：目标授权确认同一注册目录可达时，卷与工作树身份已核对
        // （设计表第 1 行「确认卷及同一工作树身份」）；仅凭清单无法确认持有人，
        // 按疑似孤立锁处理并保留缺口。
        if target_state == Some(TargetState::Directory) {
            return WorktreeObservationKind::ValidProtectedLock;
        }
        gaps.push(EvidenceGap::LockLeaseUnconfirmed {
            path: entry.path.clone(),
        });
        return WorktreeObservationKind::SuspectedOrphanLock;
    }

    match &entry.head {
        WorktreeHead::Bare | WorktreeHead::Unavailable { .. } => {
            WorktreeObservationKind::HeadOrRepositoryAnomaly
        }
        _ => WorktreeObservationKind::Healthy,
    }
}

fn paths_equivalent(left: &Path, right: &Path) -> bool {
    // 与 lifecycle 一致的组件级 ASCII 大小写不敏感比较。
    let left = left.components().collect::<Vec<_>>();
    let right = right.components().collect::<Vec<_>>();
    left.len() == right.len()
        && left
            .iter()
            .zip(&right)
            .all(|(a, b)| match (component_bytes(*a), component_bytes(*b)) {
                (Some(a), Some(b)) if a.is_ascii() && b.is_ascii() => a.eq_ignore_ascii_case(b),
                (Some(a), Some(b)) => a == b,
                (None, None) => a == b,
                _ => false,
            })
}

fn component_bytes(component: std::path::Component<'_>) -> Option<&[u8]> {
    match component {
        std::path::Component::Normal(path) => Some(path.as_encoded_bytes()),
        _ => None,
    }
}

const UNLOCK_PROTECTION_LOSS_NOTICE: &str =
    "解锁后该注册不再受锁保护，可能进入 prune 候选或被并发修改；重新加锁需要另行操作。";

fn unlock_preview(inventory: &WorktreeInventory, observation: PreviewObservation) -> UnlockPreview {
    let linked = inventory.entries.iter().find(|entry| {
        matches!(entry.kind, WorktreeKind::Linked) && !matches!(entry.head, WorktreeHead::Bare)
    });
    let Some(entry) = linked else {
        let fallback = inventory
            .entries
            .first()
            .map(|entry| entry.path.clone())
            .expect("清单必有主条目");
        return UnlockPreview {
            observation,
            candidate: UnlockPreviewCandidate {
                path: fallback,
                lock_reason: None,
                protection_loss_notice: UNLOCK_PROTECTION_LOSS_NOTICE,
            },
            rejection: Some(UnlockRejection::NoLinkedRegistration),
        };
    };
    let Some(annotation) = &entry.locked else {
        return UnlockPreview {
            observation,
            candidate: UnlockPreviewCandidate {
                path: entry.path.clone(),
                lock_reason: None,
                protection_loss_notice: UNLOCK_PROTECTION_LOSS_NOTICE,
            },
            rejection: Some(UnlockRejection::NotLocked),
        };
    };
    let confirmed = observation.states.iter().any(|state| {
        state.path == entry.path && state.state == WorktreeObservationKind::ValidProtectedLock
    });
    UnlockPreview {
        observation,
        candidate: UnlockPreviewCandidate {
            path: entry.path.clone(),
            lock_reason: annotation.reason.clone(),
            protection_loss_notice: UNLOCK_PROTECTION_LOSS_NOTICE,
        },
        rejection: if confirmed {
            None
        } else {
            Some(UnlockRejection::LockEvidenceInsufficient)
        },
    }
}

fn repair_preview(
    inventory: &WorktreeInventory,
    observation: PreviewObservation,
    target_state: Option<TargetState>,
) -> RepairPreview {
    // 原生 `worktree repair` 不支持按候选过滤，也不提供 dry-run；
    // 只有「全部 prunable 候选都能绑定授权目标且无未知候选」时范围才可证明。
    // 预览阶段只给出候选身份与目标绑定；具体移动前后身份证明属于执行工作包。
    let mut candidates = Vec::new();
    for state in &observation.states {
        if !matches!(
            state.state,
            WorktreeObservationKind::PrunableOrUnreachable
                | WorktreeObservationKind::VolumeOfflineOrIdentityDrift
        ) {
            continue;
        }
        let Some(authorized) = target_state else {
            continue;
        };
        // 目标必须可达且为目录，才可能证明「已移动到授权路径」。
        if authorized != TargetState::Directory {
            continue;
        }
        candidates.push(RepairPreviewCandidate {
            registered_path: state.path.clone(),
            moved_to: PathBuf::new(),
        });
    }
    let unbounded_candidates = candidates.len();
    // 授权目标缺席时无法证明任何候选。
    if candidates.is_empty() {
        return RepairPreview {
            observation,
            candidates,
            rejection: Some(RepairRejection::NoProvenCandidate),
        };
    }
    // 多候选（或仓库内同时存在其他 prunable 注册而未授权）时，
    // 原生命令按仓库执行无法把触及范围界定到单个工作树，明确拒绝。
    let other_prunable = inventory
        .entries
        .iter()
        .filter(|entry| entry.prunable.is_some())
        .count();
    let rejection = if unbounded_candidates > 1 || other_prunable > unbounded_candidates {
        Some(RepairRejection::ScopeUnbounded)
    } else {
        None
    };
    RepairPreview {
        observation,
        candidates,
        rejection,
    }
}

async fn prune_preview(
    git: &GitProbe,
    root: &Path,
    observation: PreviewObservation,
) -> Result<PrunePreview, RecoveryPreviewError> {
    // 固定过期策略：不传 --expire，让 Git 使用默认 3 天宽限；
    // 禁止 --expire=now 作为默认策略（设计文档第 5 节）。
    // 实测（Git for Windows 2.56）：候选清单输出在 stderr，退出码 0。
    let output = git
        .run_command(root, "worktree", &["prune", "--dry-run", "--verbose"])
        .await?;
    if !output.status.success() {
        return Err(crate::GitError::CommandFailed {
            operation: "worktree",
            code: output.status.code(),
            stderr: output.stderr,
        }
        .into());
    }
    let candidates = parse_prune_candidates(&output.stderr);
    // 锁定注册的丢失裁决（设计表第 1、3 行）：目标未经授权核对时，无法确认
    // 锁保护的注册是在线还是失联——锁定的失联注册不出现在 prune 候选中，
    // 授权人可能误以为它受保护且健康。此类裁决缺口阻断整仓 prune 授权。
    // 目标已授权确认为在线目录的锁定注册不阻断。
    let rejection = if observation.states.iter().any(|state| {
        state.annotation.is_some() && state.state == WorktreeObservationKind::SuspectedOrphanLock
    }) {
        Some(PruneRejection::LockedRegistrationUnadjudicated)
    } else {
        None
    };
    let inventory_after = git.list_worktrees(root).await?;
    Ok(PrunePreview {
        observation,
        candidates,
        inventory_after,
        rejection,
    })
}

/// 解析 `worktree prune --dry-run --verbose` 的行输出。
///
/// 实测格式（Git for Windows 2.56）：`Removing worktrees/<name>: <reason>`，
/// 无候选时 stdout 为空。原生输出按行切分；无法解析前缀的行整体保留为理由原文，
/// 避免静默丢失候选。
fn parse_prune_candidates(stdout: &[u8]) -> Vec<PrunePreviewCandidate> {
    let mut candidates = Vec::new();
    for line in stdout.split(|byte| *byte == b'\n') {
        if line.is_empty() {
            continue;
        }
        let (admin_dir, reason) = match line.strip_prefix(b"Removing ").and_then(|rest| {
            rest.iter()
                .position(|byte| *byte == b':')
                .map(|index| (&rest[..index], &rest[index + 1..]))
        }) {
            Some((admin_dir, reason)) => (admin_dir.to_vec(), reason.to_vec()),
            None => (Vec::new(), line.to_vec()),
        };
        candidates.push(PrunePreviewCandidate { admin_dir, reason });
    }
    candidates
}

async fn remove_registration_preview(
    git: &GitProbe,
    root: &Path,
    inventory: &WorktreeInventory,
    observation: PreviewObservation,
    target_authorization: Option<&Path>,
    target_state: Option<TargetState>,
) -> Result<RemoveRegistrationPreview, RecoveryPreviewError> {
    // 优先选择与授权目标路径等价的注册；未授权目标时回退到第一个 linked 条目。
    // 目标绑定使多注册仓库中的预览不因清单顺序而错选。
    let linked = inventory
        .entries
        .iter()
        .find(|entry| {
            matches!(entry.kind, WorktreeKind::Linked)
                && !matches!(entry.head, WorktreeHead::Bare)
                && target_authorization.is_some_and(|target| {
                    entry
                        .path
                        .to_path_buf()
                        .is_ok_and(|path| paths_equivalent(&path, target))
                })
        })
        .or_else(|| {
            inventory.entries.iter().find(|entry| {
                matches!(entry.kind, WorktreeKind::Linked)
                    && !matches!(entry.head, WorktreeHead::Bare)
            })
        });
    let Some(entry) = linked else {
        let fallback = inventory
            .entries
            .first()
            .map(|entry| entry.path.clone())
            .expect("清单必有主条目");
        return Ok(RemoveRegistrationPreview {
            observation,
            scope: RemoveRegistrationScope::Disconnected {
                path: fallback,
                prunable_reason: None,
            },
            rejection: Some(RemoveRegistrationRejection::NoLinkedRegistration),
        });
    };
    let path = entry.path.clone();
    if let Some(annotation) = &entry.prunable {
        // 失联注册只允许走受控 prune。
        return Ok(RemoveRegistrationPreview {
            observation,
            scope: RemoveRegistrationScope::Disconnected {
                path,
                prunable_reason: annotation.reason.clone(),
            },
            rejection: None,
        });
    }
    // 在线判定：授权目标观察必须覆盖该注册路径。
    let Some(TargetState::Directory) = target_state else {
        return Ok(RemoveRegistrationPreview {
            observation,
            scope: RemoveRegistrationScope::Disconnected {
                path,
                prunable_reason: None,
            },
            rejection: Some(RemoveRegistrationRejection::CleanlinessUnproven),
        });
    };
    if entry.locked.is_some() {
        return Ok(RemoveRegistrationPreview {
            observation,
            scope: RemoveRegistrationScope::OnlineClean {
                path,
                clean: false,
                locked: true,
            },
            rejection: Some(RemoveRegistrationRejection::Locked),
        });
    }
    if matches!(entry.head, WorktreeHead::Unavailable { .. }) {
        return Ok(RemoveRegistrationPreview {
            observation,
            scope: RemoveRegistrationScope::OnlineClean {
                path,
                clean: false,
                locked: false,
            },
            rejection: Some(RemoveRegistrationRejection::HealthUncertain),
        });
    }
    // 干净判定复用 status：源仓库工作区必须无任何变更记录。
    let clean = match git.probe(root).await {
        Ok(snapshot) => snapshot.status.entries.is_empty(),
        Err(_) => false,
    };
    Ok(RemoveRegistrationPreview {
        observation,
        scope: RemoveRegistrationScope::OnlineClean {
            path,
            clean,
            locked: false,
        },
        rejection: if clean {
            None
        } else {
            Some(RemoveRegistrationRejection::CleanlinessUnproven)
        },
    })
}

fn partial_cleanup_preview(
    intents: &[PersistentIntent],
    request_id: &str,
    observation: PreviewObservation,
    target_state: Option<TargetState>,
) -> PartialCleanupPreview {
    let Some(intent) = intents
        .iter()
        .find(|intent| intent.request_id == request_id)
    else {
        return PartialCleanupPreview {
            observation,
            request_id: request_id.to_owned(),
            scope: PartialCleanupScope::TargetAbsent,
            rejection: Some(PartialCleanupRejection::IntentNotFound),
        };
    };
    if matches!(
        intent.execution.stage,
        ExecutionStage::Started | ExecutionStage::Prepared
    ) || matches!(
        &intent.execution.result,
        ExecutionResult::Unknown { .. } | ExecutionResult::SpawnFailed
    ) {
        return PartialCleanupPreview {
            observation,
            request_id: request_id.to_owned(),
            scope: PartialCleanupScope::TargetAbsent,
            rejection: Some(PartialCleanupRejection::ExecutionUnknown),
        };
    }
    match target_state {
        None => PartialCleanupPreview {
            observation,
            request_id: request_id.to_owned(),
            scope: PartialCleanupScope::TargetAbsent,
            rejection: Some(PartialCleanupRejection::TargetUnprovable),
        },
        Some(TargetState::Absent) => PartialCleanupPreview {
            observation,
            request_id: request_id.to_owned(),
            scope: PartialCleanupScope::TargetAbsent,
            rejection: None,
        },
        Some(_) => PartialCleanupPreview {
            observation,
            request_id: request_id.to_owned(),
            scope: PartialCleanupScope::UnprovableContent {
                target: intent.target.clone(),
            },
            rejection: Some(PartialCleanupRejection::TargetUnprovable),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(value: &str) -> PathBuf {
        PathBuf::from(value)
    }

    #[test]
    fn paths_equivalent_compares_components_case_insensitively_for_ascii() {
        assert!(paths_equivalent(&path("C:/a/Beta"), &path("c:/A/beta")));
        assert!(!paths_equivalent(&path("C:/a/beta"), &path("C:/a/beta2")));
        assert!(!paths_equivalent(&path("C:/a/beta"), &path("C:/a/beta/子")));
    }

    #[test]
    fn prune_candidates_parse_native_lines_and_keep_unknown_prefixes() {
        let parsed = parse_prune_candidates(
            b"Removing worktrees/wt1: gitdir file points to non-existent location\n\n",
        );
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].admin_dir, b"worktrees/wt1".to_vec());
        assert_eq!(
            parsed[0].reason,
            b" gitdir file points to non-existent location".to_vec()
        );
        let unknown = parse_prune_candidates(b"warning: unexpected line\n");
        assert_eq!(unknown.len(), 1);
        assert!(unknown[0].admin_dir.is_empty());
        assert_eq!(unknown[0].reason, b"warning: unexpected line".to_vec());
        assert!(parse_prune_candidates(b"").is_empty());
    }
}
