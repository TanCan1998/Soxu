//! 带单调游标信封广播的实时事件流定义。

use crate::PROTOCOL_VERSION;
use crate::capabilities::LostControlNotice;
use crate::domain::{
    ApprovalAuditRecord, ApprovalItemSnapshot, DraftSnapshot, PendingUserInputAnswer,
    PendingUserInputRequest, ProjectSummary, QueuedMessageItem, QuickChatSummary, SessionSummary,
    SubagentSummary, TurnMessageSnapshot, WorkspaceSummary,
};
use crate::error::ActionableError;
use crate::pairing::PairedDeviceSummary;
use crate::preferences::SoxuPreferences;
use crate::rpc::StateSnapshot;
use crate::telemetry::SessionTelemetryStats;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 带单调递增游标的事件信封，保障移动端切后台或弱网重连后可按 `seq` 增量追平。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventEnvelope<T> {
    pub protocol_version: u32,
    pub seq: u64,
    pub timestamp: DateTime<Utc>,
    pub payload: T,
}

impl<T> EventEnvelope<T> {
    #[must_use]
    pub fn new(seq: u64, payload: T) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            seq,
            timestamp: Utc::now(),
            payload,
        }
    }
}

/// 守护进程向桌面端、移动 PWA 与 CLI 广播的实时增量事件枚举。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", content = "data", rename_all = "snake_case")]
pub enum DaemonEvent {
    /// 断线重连后按 `after_seq` 追平完成摘要事件。
    CatchUpCompleted {
        from_seq: u64,
        to_seq: u64,
        count: u64,
    },
    SnapshotSynced(StateSnapshot),
    ProjectUpdated(ProjectSummary),
    WorkspaceUpdated(WorkspaceSummary),
    QuickChatUpdated(QuickChatSummary),
    SessionUpdated(SessionSummary),
    SessionTelemetryUpdated {
        session_id: String,
        stats: SessionTelemetryStats,
    },
    TurnMessageUpserted(TurnMessageSnapshot),
    TurnStreamDelta {
        session_id: String,
        message_id: String,
        text_delta: String,
        reasoning_delta: Option<String>,
    },
    QueueUpdated {
        session_id: String,
        queued: Vec<QueuedMessageItem>,
    },
    DraftSaved(DraftSnapshot),
    CapabilitiesLostControls {
        session_id: String,
        notices: Vec<LostControlNotice>,
    },
    ApprovalRequested(ApprovalItemSnapshot),
    ApprovalResolved {
        approval_id: String,
        session_id: String,
        audit: ApprovalAuditRecord,
    },
    UserInputRequested(PendingUserInputRequest),
    UserInputSubmitted(PendingUserInputAnswer),
    SubagentUpdated(SubagentSummary),
    PreferencesChanged(SoxuPreferences),
    PairingDeviceUpdated(PairedDeviceSummary),
    ErrorRaised {
        session_id: Option<String>,
        error: ActionableError,
    },
}
