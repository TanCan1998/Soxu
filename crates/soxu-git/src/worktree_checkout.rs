//! 对持久注册请求执行一次受控检出。

use std::io;

use serde::{Deserialize, Serialize};

#[cfg(target_os = "linux")]
use crate::worktree_lifecycle::state;
use crate::worktree_lifecycle::validate_request_id;
use crate::{
    GitError, GitProbe, VerifiedRegistration, WorktreeLifecycleError,
    WorktreeRegistrationCoordinator,
};

mod journal;
#[cfg(target_os = "linux")]
mod publication;
#[cfg(any(target_os = "linux", test))]
mod sandbox;

/// 最后一个已持久化的阶段。进入阶段后，对应范围可能已经发生写入。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckoutStage {
    /// 仅检查注册并准备私有暂存目录。
    Preflight,
    /// Git 只向私有暂存目录写入。
    Materialize,
    /// 目标管理目录的 index 可能已发布。
    PublishIndex,
    /// index 与部分目标文件可能已发布。
    PublishFiles,
    /// index 与目标文件已发布，等待最终复核。
    Verify,
}

/// 持久检出结果。Verified 仅表示本次检出通过复核，宿主接入另行验证。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckoutStatus {
    Pending,
    Disabled {
        stage: Option<CheckoutStage>,
        reason: Option<String>,
    },
    Verified,
}

#[derive(Debug, thiserror::Error)]
pub enum CheckoutError {
    #[error(transparent)]
    Registration(#[from] WorktreeLifecycleError),
    #[error("检出文件系统操作失败：{0}")]
    Io(#[from] io::Error),
    #[error(transparent)]
    Git(#[from] GitError),
    #[error("拒绝受控检出：{0}")]
    Unsafe(&'static str),
    #[error("该请求已留下检出记录：{0:?}")]
    AlreadyAttempted(CheckoutStatus),
    #[error("检出调用已取消")]
    Cancelled,
    #[error("检出子进程可能仍在运行，停止原因：{0}")]
    StopUnconfirmed(&'static str),
    #[error("检出执行所有者退出，保留最后持久阶段")]
    OwnerLost,
    #[error("检出失败（{original}），已保留禁止接入记录，写入进一步诊断失败：{record}")]
    RecordFailure {
        original: Box<Self>,
        record: io::Error,
    },
}

impl GitProbe {
    /// 查询协调域中的检出留痕。调用者仍需核实当前工作树和授权。
    pub fn checkout_status(
        &self,
        coordinator: &WorktreeRegistrationCoordinator,
        request_id: &str,
    ) -> Result<CheckoutStatus, CheckoutError> {
        validate_request_id(request_id)?;
        coordinator.state_root.refresh()?;
        let status = journal::read(coordinator.state_root.path(), request_id)?;
        coordinator.state_root.refresh()?;
        Ok(status)
    }

    /// 重新核验持久注册并检出固定提交。每个授权请求有一次执行机会。
    pub async fn checkout_registered_worktree(
        &self,
        coordinator: &WorktreeRegistrationCoordinator,
        registration: &VerifiedRegistration,
    ) -> Result<CheckoutStatus, CheckoutError> {
        validate_request_id(&registration.intent.request_id)?;
        coordinator.state_root.refresh()?;
        // #49 当前在 Windows 上因缺少稳定的目录身份而拒绝注册。
        // Linux 以目录句柄为写入锚点，其它平台缺少该实现时同样拒绝。
        if !cfg!(target_os = "linux") {
            return Err(WorktreeLifecycleError::UnsupportedFilesystem(
                "缺少受控检出所需的目录句柄路径",
            )
            .into());
        }
        #[cfg(target_os = "linux")]
        {
            use crate::worktree_lifecycle::CheckoutRegistration;

            let lock = state::acquire_state_lock(&coordinator.state_root).await?;

            let intent = CheckoutRegistration::authorize(coordinator, registration)?;
            lock.refresh(&coordinator.state_root)?;
            let state_root = coordinator.state_root.clone();
            let coordinator = WorktreeRegistrationCoordinator {
                state_root: state_root.clone(),
            };
            let git = self.clone();
            let (cancel, cancelled) = tokio::sync::oneshot::channel();
            let task = tokio::spawn(async move {
                let mut journal = journal::Journal::begin(state_root.path(), &intent.request_id)?;
                let result = publication::checkout(
                    &git,
                    &coordinator,
                    &intent,
                    &lock,
                    &mut journal,
                    cancelled,
                )
                .await;
                match result {
                    Ok(()) => {
                        lock.refresh(&state_root)?;
                        journal.verified()?;
                        Ok(CheckoutStatus::Verified)
                    }
                    Err(error) => {
                        if let Err(record) = journal.failure(&error) {
                            return Err(CheckoutError::RecordFailure {
                                original: Box::new(error),
                                record,
                            });
                        }
                        Err(error)
                    }
                }
            });
            let result = task.await;
            drop(cancel);
            result.map_err(|_| CheckoutError::OwnerLost)?
        }
        #[cfg(not(target_os = "linux"))]
        {
            Err(
                WorktreeLifecycleError::UnsupportedFilesystem("缺少受控检出所需的目录句柄路径")
                    .into(),
            )
        }
    }
}
