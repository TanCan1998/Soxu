//! 三要素可执行错误契约（中文描述 + 影响范围 + 一键修复建议）。

use serde::{Deserialize, Serialize};
use std::fmt;

/// 协议级稳定错误码。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    ProtocolMismatch,
    Unauthorized,
    PairingExpired,
    NotFound,
    ValidationFailed,
    SafetyRedlineBlocked,
    Conflict,
    DriverUnavailable,
    WorkspaceDamaged,
    InternalError,
    Unsupported,
    IdentityChanged,
    PolicyDenied,
    SafetyIncomplete,
    PartialNeedsRecovery,
    RefPublishedIndexPending,
    /// 断线追平过程中读取事件日志失败：本次追平已中止，缺口必须重新握手重取快照。
    CatchUpFailed,
}

/// 面向客户端渲染的三要素可执行错误负载。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionableError {
    pub code: ErrorCode,
    pub summary_zh: String,
    pub impact_scope_zh: String,
    pub remediation_zh: String,
    pub diagnostic_detail: Option<String>,
}

impl ActionableError {
    #[must_use]
    pub fn new(
        code: ErrorCode,
        summary_zh: impl Into<String>,
        impact_scope_zh: impl Into<String>,
        remediation_zh: impl Into<String>,
    ) -> Self {
        Self {
            code,
            summary_zh: summary_zh.into(),
            impact_scope_zh: impact_scope_zh.into(),
            remediation_zh: remediation_zh.into(),
            diagnostic_detail: None,
        }
    }

    #[must_use]
    pub fn with_diagnostic(mut self, detail: impl Into<String>) -> Self {
        self.diagnostic_detail = Some(detail.into());
        self
    }
}

impl fmt::Display for ActionableError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}（影响范围：{}；建议处理：{}）",
            self.summary_zh, self.impact_scope_zh, self.remediation_zh
        )
    }
}

impl std::error::Error for ActionableError {}
