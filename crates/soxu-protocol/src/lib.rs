//! Soxu 跨端（GPUI 桌面端、移动端 Web/PWA 与 CLI）统一版本化通信协议。

pub mod capabilities;
pub mod domain;
pub mod error;
pub mod events;
pub mod git_ops;
pub mod pairing;
pub mod preferences;
pub mod primitives;
pub mod pty;
pub mod rpc;
pub mod schedules;
pub mod scripts;
pub mod telemetry;

/// 当前通信协议版本号，客户端连接 Daemon 时用于握手兼容性检查。
pub const PROTOCOL_VERSION: u32 = 1;

pub use capabilities::{
    AgentCapabilityDescriptor, DaemonFeature, DaemonFeatureSet, LostControlNotice, ModelDescriptor,
    PermissionOptionMeta, PermissionTone, ThinkingEffort,
};
pub use domain::{
    ActiveTurnBehavior, ApprovalAuditRecord, ApprovalItemSnapshot, ApprovalResolutionKind,
    AskQuestionOption, CheckpointAnchor, ComposerChip, ComposerChipKind, DraftSnapshot,
    HandoffAnchorCard, HandoffScopeOptions, HandoffSelection, McpRouteBadge,
    PendingUserInputAnswer, PendingUserInputAnswerItem, PendingUserInputQuestion,
    PendingUserInputRequest, PermissionLevel, ProjectSummary, QueuedMessageItem, QuickChatSummary,
    SessionSummary, SessionTarget, SubagentStatus, SubagentSummary, ToolCallCategory,
    ToolCallStatus, ToolCardSnapshot, TurnLifecycleState, TurnMessageRole, TurnMessageSnapshot,
    WorkspaceBucket, WorkspaceIsolation, WorkspaceSummary,
};
pub use error::{ActionableError, ErrorCode};
pub use events::{DaemonEvent, EventEnvelope};
pub use git_ops::{
    CheckpointBaseline, CheckpointCoverageSummary, CheckpointDiffCompareRequest,
    CheckpointDiffCompareResult, CheckpointDiffFileSummary, CheckpointDiffHunkSummary,
    CheckpointDiffIncompleteReason, CheckpointId, CheckpointOmissionReason,
    CheckpointOmissionRecord, CheckpointRecordSummary, CheckpointState, CommitApprovalTicket,
    CommitContentSource, CommitHook, CommitIndexSyncPlan, CommitPathPreview, CommitPathRisk,
    CommitPlanPreview, CommitPreviewRequest, CommitStage, CommitVerificationRequirements,
    FixedDiffEndpoint, GitObjectId, GitOperationId, GitOperationKind, GitOperationLookup,
    GitOperationPathScope, GitOperationRecord, GitOperationStage, GitOperationStatusQuery,
    GitPathChange, GitPathChangeKind, GitPlanBlocker, GitPublishedCommit, ListCheckpointsRequest,
    ListCheckpointsResult, RewindActionScope, RewindApprovalTicket, RewindPathPreview,
    RewindPlanPreview, RewindPreviewRequest, RewindStage, RewindTargetAction, RewindWorktreeTarget,
    SafetyId, SafetyProtectionState, SafetyRetentionPolicy, SafetySnapshotHandle,
    SafetyVerificationState,
};
pub use pairing::{
    ClaimPairingRequest, ClaimPairingResponse, PairedDeviceSummary, PairingCodeSnapshot,
    PairingCredentialInput, PairingOfferPayload, PairingTransportMode,
};
pub use preferences::{
    AccentPresetId, CodeFontPresetId, DensityMode, LineHeightMode, SoxuPreferences, ThemeMode,
    TimestampFormatMode, UiFontPresetId, WorktreeLocationMode,
};
pub use primitives::{EnvKey, IdempotencyKey, WorkspaceRelativePath};
pub use pty::{
    ClosePtyRequest, ClosePtyResponse, OpenPtyRequest, OpenPtyResponse, PTY_INPUT_CHUNK_MAX_BYTES,
    PTY_OUTPUT_CHUNK_MAX_BYTES, PtyCloseReason, PtyCommand, PtyCommandResult, PtyEnvPolicy,
    PtyEvent, PtyExitStatus, PtyInputData, PtyOutputChunk, PtyOutputData, PtySessionId,
    PtySessionSnapshot, PtySessionState, PtySize, PtyWorkdirPolicy, ResizePtyRequest,
    ResizePtyResponse, WritePtyRequest, WritePtyResponse,
};
pub use rpc::{
    ClientHello, ClientKind, HealthSnapshot, RpcCommand, RpcDomainCommand, RpcRequest, RpcResponse,
    RpcResultData, ServerHello, StateSnapshot,
};
pub use schedules::{
    CreateScheduleRequest, CronExpression, DeleteScheduleRequest, ListSchedulesRequest,
    PauseScheduleRequest, ScheduleCadence, ScheduleCommand, ScheduleCommandResult, ScheduleEvent,
    ScheduleId, ScheduleListResult, ScheduleName, ScheduleRetryPolicy, ScheduleRunOutcome,
    ScheduleRunResult, ScheduleRunTrigger, ScheduleState, ScheduleSummary, ScheduleTimezone,
    ScheduleTrigger, ScheduleTriggeredEvent, ScheduleValidityWindow, TriggerScheduleRequest,
    UpdateScheduleRequest,
};
pub use scripts::{
    InspectScriptRequest, ListScriptsRequest, ManagedScriptEntry, RunScriptRequest, ScriptArgument,
    ScriptArgumentPolicy, ScriptCommand, ScriptCommandResult, ScriptEnvPolicy,
    ScriptFixedArguments, ScriptId, ScriptLifecycleEvent, ScriptLifecyclePhase,
    ScriptManifestListing, ScriptProgram, ScriptRiskLevel, ScriptRunSnapshot, ScriptRunState,
    ScriptUserArguments, ScriptWorkdirPolicy, StopScriptRequest,
};
pub use telemetry::{ContextUsageBand, SessionTelemetryStats};

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn roundtrip_event_envelope_and_daemon_event() {
        let env = EventEnvelope::new(
            42,
            DaemonEvent::CatchUpCompleted {
                from_seq: 30,
                to_seq: 42,
                count: 12,
            },
        );
        let raw = serde_json::to_string(&env).expect("serialize envelope");
        let decoded: EventEnvelope<DaemonEvent> =
            serde_json::from_str(&raw).expect("deserialize envelope");
        assert_eq!(decoded.seq, 42);
        assert_eq!(decoded.protocol_version, PROTOCOL_VERSION);
        assert_eq!(
            decoded.payload,
            DaemonEvent::CatchUpCompleted {
                from_seq: 30,
                to_seq: 42,
                count: 12,
            }
        );
    }

    #[test]
    fn permission_level_uses_kebab_case() {
        let json = serde_json::to_string(&PermissionLevel::AutoEdit)
            .expect("serialize kebab-case permission");
        assert_eq!(json, "\"auto-edit\"");
        let ro = serde_json::to_string(&PermissionLevel::ReadOnly)
            .expect("serialize read-only permission");
        assert_eq!(ro, "\"read-only\"");
        let decoded: PermissionLevel =
            serde_json::from_str("\"full-access\"").expect("deserialize full-access");
        assert_eq!(decoded, PermissionLevel::FullAccess);
    }

    #[test]
    fn preferences_roundtrip_uses_camel_case_and_matches_frontend_defaults() {
        let prefs = SoxuPreferences::default();
        let val = serde_json::to_value(&prefs).expect("serialize preferences");
        assert_eq!(val["themeMode"], "dark");
        assert_eq!(val["locale"], "zh-CN");
        assert_eq!(val["accentPreset"], "emerald");
        assert_eq!(val["codeFollowsUi"], false);
        assert_eq!(val["uiFontPreset"], "inter-noto");
        assert_eq!(val["codeFontPreset"], "jetbrains-mono");
        assert_eq!(val["defaultPermMode"], "ask");
        assert_eq!(val["leftSidebarWidth"], 264);
        assert_eq!(val["rightInspectorWidth"], 380);

        let partial: SoxuPreferences =
            serde_json::from_str(r#"{"codeFollowsUi":true,"uiFontSize":16}"#)
                .expect("deserialize partial preferences with defaults");
        assert!(partial.code_follows_ui);
        assert_eq!(partial.ui_font_size, 16);
        assert_eq!(partial.locale, "zh-CN");
    }

    #[test]
    fn context_usage_band_threshold_boundaries() {
        assert_eq!(
            ContextUsageBand::from_percentage(0.0),
            ContextUsageBand::SafeGreen
        );
        assert_eq!(
            ContextUsageBand::from_percentage(64.99),
            ContextUsageBand::SafeGreen
        );
        assert_eq!(
            ContextUsageBand::from_percentage(65.0),
            ContextUsageBand::WarningAmber
        );
        assert_eq!(
            ContextUsageBand::from_percentage(87.99),
            ContextUsageBand::WarningAmber
        );
        assert_eq!(
            ContextUsageBand::from_percentage(88.0),
            ContextUsageBand::DangerRed
        );

        let stats = SessionTelemetryStats {
            used_context_tokens: 176_000,
            context_window_limit: 200_000,
            ..SessionTelemetryStats::default()
        };
        assert!((stats.context_usage_pct() - 88.0).abs() < 1e-6);
        assert_eq!(stats.context_band(), ContextUsageBand::DangerRed);

        let zero_limit = SessionTelemetryStats::default();
        assert_eq!(zero_limit.context_usage_pct(), 0.0);
        assert_eq!(zero_limit.context_band(), ContextUsageBand::SafeGreen);
    }

    #[test]
    fn workspace_bucket_priority_order() {
        let mut buckets = [
            WorkspaceBucket::Done,
            WorkspaceBucket::Running,
            WorkspaceBucket::Attention,
            WorkspaceBucket::NeedsInput,
            WorkspaceBucket::Failed,
        ];
        buckets.sort_by_key(|b| b.priority_rank());
        assert_eq!(
            buckets,
            [
                WorkspaceBucket::NeedsInput,
                WorkspaceBucket::Failed,
                WorkspaceBucket::Running,
                WorkspaceBucket::Attention,
                WorkspaceBucket::Done,
            ]
        );
    }

    #[test]
    fn roundtrip_rpc_request_and_response() {
        let req = RpcRequest {
            request_id: "req_01".to_owned(),
            command: RpcCommand::SubmitTurn {
                session_id: "sess_01".to_owned(),
                text: "检查降级逻辑".to_owned(),
                chips: vec![ComposerChip {
                    id: "chip_1".to_owned(),
                    kind: ComposerChipKind::Handoff,
                    label: "@codex".to_owned(),
                    path_or_ref: None,
                    target_agent: Some("codex".to_owned()),
                    target_model: Some("group/gpt-6.2-sol".to_owned()),
                    target_thinking: Some(ThinkingEffort::High),
                }],
                behavior: ActiveTurnBehavior::Steer,
                handoff: Some(HandoffSelection {
                    target_agent: "codex".to_owned(),
                    target_model: "group/gpt-6.2-sol".to_owned(),
                    target_thinking: Some(ThinkingEffort::High),
                }),
                handoff_scope: Some(HandoffScopeOptions::default()),
            },
        };
        let encoded = serde_json::to_string(&req).expect("serialize rpc request");
        let decoded: RpcRequest = serde_json::from_str(&encoded).expect("deserialize rpc request");
        assert_eq!(decoded, req);

        let resp = RpcResponse::ok(
            "req_01",
            RpcResultData::ApprovalResolved(ApprovalAuditRecord {
                resolution: ApprovalResolutionKind::ApproveOnce,
                resolved_by: "desktop".to_owned(),
                guidance_note: None,
                resolved_at: Utc::now(),
            }),
        );
        let resp_raw = serde_json::to_string(&resp).expect("serialize rpc response");
        let resp_decoded: RpcResponse =
            serde_json::from_str(&resp_raw).expect("deserialize rpc response");
        assert!(resp_decoded.ok);
        assert_eq!(resp_decoded.request_id, "req_01");
    }

    #[test]
    fn actionable_error_displays_three_elements() {
        let err = ActionableError::new(
            ErrorCode::SafetyRedlineBlocked,
            "拦截高危删除命令",
            "仅影响当前会话指令提交",
            "请移除递归删除目标或改为临时目录测试",
        )
        .with_diagnostic("rule=rm-rf");
        let text = err.to_string();
        assert!(text.contains("拦截高危删除命令"));
        assert!(text.contains("影响范围"));
        assert!(text.contains("建议处理"));
    }
}
