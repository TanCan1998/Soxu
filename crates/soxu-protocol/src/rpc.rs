//! 守护进程 HTTP/WebSocket RPC 握手、全量快照与指令契约。

use crate::capabilities::{
    AgentCapabilityDescriptor, LostControlNotice, ModelDescriptor, ThinkingEffort,
};
use crate::domain::{
    ActiveTurnBehavior, ApprovalAuditRecord, ApprovalItemSnapshot, ApprovalResolutionKind,
    ComposerChip, DraftSnapshot, HandoffScopeOptions, HandoffSelection, PendingUserInputAnswer,
    PendingUserInputRequest, PermissionLevel, ProjectSummary, QuickChatSummary, SessionSummary,
    SessionTarget, SubagentSummary, TurnLifecycleState, WorkspaceSummary,
};
use crate::error::ActionableError;
use crate::events::{DaemonEvent, EventEnvelope};
use crate::pairing::{
    ClaimPairingRequest, ClaimPairingResponse, PairedDeviceSummary, PairingCodeSnapshot,
    PairingOfferPayload, PairingTransportMode,
};
use crate::preferences::SoxuPreferences;
use serde::{Deserialize, Serialize};

/// 守护进程健康状态快照。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthSnapshot {
    pub status: String,
    pub daemon_version: String,
    pub protocol_version: u32,
    pub magpie_detected: bool,
}

/// 接入守护进程的客户端类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientKind {
    Desktop,
    MobilePwa,
    Cli,
}

/// WebSocket 建连握手请求（携带断线追平游标 `after_seq`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientHello {
    pub client_kind: ClientKind,
    pub client_version: String,
    pub protocol_version: u32,
    pub after_seq: Option<u64>,
    pub device_id: Option<String>,
}

/// 守护进程建连响应及当前全局快照。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerHello {
    pub daemon_version: String,
    pub daemon_fingerprint: String,
    pub protocol_version: u32,
    pub magpie_detected: bool,
    pub latest_seq: u64,
    pub snapshot: StateSnapshot,
}

/// 桌面与移动端首屏渲染所需的无阻塞内存全量快照。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct StateSnapshot {
    pub projects: Vec<ProjectSummary>,
    pub workspaces: Vec<WorkspaceSummary>,
    pub quick_chats: Vec<QuickChatSummary>,
    pub sessions: Vec<SessionSummary>,
    pub pending_approvals: Vec<ApprovalItemSnapshot>,
    pub pending_inputs: Vec<PendingUserInputRequest>,
    pub subagents: Vec<SubagentSummary>,
    pub agents: Vec<AgentCapabilityDescriptor>,
    pub models: Vec<ModelDescriptor>,
    pub preferences: SoxuPreferences,
    pub paired_devices: Vec<PairedDeviceSummary>,
}

/// 客户端发往 Daemon 的 RPC 命令。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
pub enum RpcCommand {
    GetStateSnapshot,
    CatchUpEvents {
        after_seq: u64,
        limit: Option<u32>,
    },
    CreateQuickChat {
        title: Option<String>,
        agent_id: String,
        model_id: Option<String>,
        permission: Option<PermissionLevel>,
        thinking: Option<ThinkingEffort>,
    },
    CreateSession {
        target: SessionTarget,
        title: Option<String>,
        agent_id: String,
        model_id: Option<String>,
        permission: Option<PermissionLevel>,
        thinking: Option<ThinkingEffort>,
    },
    ArchiveTarget {
        target: SessionTarget,
        archived: bool,
    },
    SubmitTurn {
        session_id: String,
        text: String,
        chips: Vec<ComposerChip>,
        behavior: ActiveTurnBehavior,
        handoff: Option<HandoffSelection>,
        handoff_scope: Option<HandoffScopeOptions>,
    },
    PromoteQueuedTurn {
        session_id: String,
        queue_id: String,
        target_behavior: ActiveTurnBehavior,
    },
    CancelQueuedTurn {
        session_id: String,
        queue_id: String,
    },
    StopTurn {
        session_id: String,
    },
    SwitchSessionCapabilities {
        session_id: String,
        agent_id: String,
        model_id: Option<String>,
        permission: Option<PermissionLevel>,
        thinking: Option<ThinkingEffort>,
        behavior: ActiveTurnBehavior,
    },
    SaveDraft(DraftSnapshot),
    ResolveApproval {
        approval_id: String,
        resolution: ApprovalResolutionKind,
        guidance_note: Option<String>,
        resolved_by: String,
    },
    SubmitUserInput(PendingUserInputAnswer),
    DetachSubagent {
        subagent_id: String,
    },
    UpdatePreferences(SoxuPreferences),
    CreatePairingOffer {
        transport: PairingTransportMode,
    },
    ClaimPairing(ClaimPairingRequest),
    RevokePairedDevice {
        device_id: String,
    },
}

/// 带请求关联 ID 的 RPC 请求信封。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RpcRequest {
    pub request_id: String,
    pub command: RpcCommand,
}

/// RPC 成功返回值变体。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum RpcResultData {
    Ack,
    StateSnapshot(StateSnapshot),
    Events {
        events: Vec<EventEnvelope<DaemonEvent>>,
        latest_seq: u64,
    },
    QuickChatCreated(QuickChatSummary),
    SessionCreated(SessionSummary),
    CapabilitiesSwitched {
        session: SessionSummary,
        lost_controls: Vec<LostControlNotice>,
    },
    TurnSubmitted {
        session_id: String,
        lifecycle: TurnLifecycleState,
        queued_id: Option<String>,
    },
    ApprovalResolved(ApprovalAuditRecord),
    SubagentDetached(SessionSummary),
    PreferencesUpdated(SoxuPreferences),
    PairingOfferCreated {
        offer: PairingOfferPayload,
        code: PairingCodeSnapshot,
    },
    PairingClaimed(ClaimPairingResponse),
}

/// RPC 响应信封。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RpcResponse {
    pub request_id: String,
    pub ok: bool,
    pub result: Option<RpcResultData>,
    pub error: Option<ActionableError>,
}

impl RpcResponse {
    #[must_use]
    pub fn ok(request_id: impl Into<String>, result: RpcResultData) -> Self {
        Self {
            request_id: request_id.into(),
            ok: true,
            result: Some(result),
            error: None,
        }
    }

    #[must_use]
    pub fn err(request_id: impl Into<String>, error: ActionableError) -> Self {
        Self {
            request_id: request_id.into(),
            ok: false,
            result: None,
            error: Some(error),
        }
    }
}
