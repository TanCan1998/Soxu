//! Worktree 恢复契约：只读预览（M3 #57-2）与二次确认后的执行（M3 #57-3）。
//!
//! 预览只产出候选身份、影响对象、证据缺口与不确定性，供宿主生成二次授权摘要；
//! 执行按单次确认绑定预览摘要与有效期，只调用原生 Git（`unlock`/`repair`/`prune`/
//! `worktree remove`），并把意图、阶段与回读结果持久化到协调域 journal。
//! 两者都不做推断性自愈：失败保留足迹并禁用后续接入，重启后只能只读诊断。

use std::path::Path;

mod execute;
mod journal;
mod preview;

pub use execute::{
    RecoveryCancellation, RecoveryConfirmation, RecoveryExecuteError, RecoveryExecutionOutcome,
    RecoveryObservation, RecoveryPreviewDigest, recovery_preview_digest,
};
pub use journal::{RecoveryExecutionRecord, RecoveryNote};
pub use preview::{
    EvidenceGap, ObservedWorktreeState, PartialCleanupPreview, PartialCleanupRejection,
    PartialCleanupScope, PreviewObservation, PreviewOutcome, PrunePreview, PrunePreviewCandidate,
    PruneRejection, RecoveryPreviewError, RecoveryPreviewRequest, RemoveRegistrationPreview,
    RemoveRegistrationRejection, RemoveRegistrationScope, RepairPreview, RepairPreviewCandidate,
    RepairRejection, UnlockPreview, UnlockPreviewCandidate, UnlockRejection,
    WorktreeObservationKind,
};

/// 恢复预览的操作类型。
///
/// 每个操作独立预览、独立授权；任何会话级允许或前一次批准都不能复用。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryOperation {
    /// 解除单个 linked 注册的保护锁。
    Unlock,
    /// 修正已移动 linked 注册的双向路径映射。
    Repair,
    /// 按固定过期策略剪除仓库内全部失联注册。
    Prune,
    /// 移除一个在线干净或失联的 linked 注册。
    RemoveRegistration,
    /// 清理既有请求的部分落盘足迹。
    PartialCleanup,
}

/// 复用现有诊断入口的恢复预览总入口。
///
/// 全程只读：清单来自 `GitProbe::list_worktrees`，意图与检出留痕只读回查，
/// 目标路径只在显式授权时核对一次。Git 报告的 prunable 原因逐字节保留，
/// 不推断磁盘状态，不做任何自愈。
impl crate::GitProbe {
    pub async fn recovery_preview(
        &self,
        request: RecoveryPreviewRequest<'_>,
    ) -> Result<PreviewOutcome, RecoveryPreviewError> {
        preview::build(self, request).await
    }
}

/// 恢复预览的源仓库根必须已通过宿主授权并保持可打开。
///
/// 设计文档第 5 节要求快照分别记录观察时刻、可确认事实和证据缺口；
/// 这里集中校验输入边界，具体观察由 `preview` 子模块完成。
pub(crate) fn validate_source_root(source_root: &Path) -> Result<(), RecoveryPreviewError> {
    if !source_root.is_absolute() {
        return Err(RecoveryPreviewError::InvalidInput("源仓库根必须是绝对路径"));
    }
    Ok(())
}
