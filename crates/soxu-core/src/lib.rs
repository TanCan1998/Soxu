//! Soxu 核心领域模型、编排状态机与插件扩展预留槽位。

pub mod buckets;
pub mod capabilities;
pub mod drafts;
pub mod orchestrator;
pub mod queue;
pub mod redline;

pub use buckets::{
    aggregate_bucket, project_aggregated_bucket, sort_quick_chats_by_attention,
    sort_workspaces_by_attention,
};
pub use capabilities::{
    ReconcileOutcome, ReconcileRequest, default_agent_catalog, default_model_catalog,
    permission_label_zh, reconcile_capabilities, thinking_label_zh,
};
pub use drafts::{DraftSaveOutcome, DraftStore};
pub use orchestrator::CoreOrchestrator;
pub use queue::{QueuePromotion, TurnQueueManager};
pub use redline::{RedlineAssessment, assess_command_safety, validate_approval_resolution};
pub use soxu_protocol::WorkspaceIsolation;

use std::collections::BTreeMap;

/// 插件系统预留运行时钩子接口（前期不实现插件市场，使用空实现零开销运行）。
pub trait RuntimeExtensionHook: Send + Sync {
    /// 在启动原生 Agent CLI 进程前允许注入或转换环境变量。
    fn transform_launch_env(&self, _env: &mut BTreeMap<String, String>) {}
}

/// 默认空扩展宿主。
#[derive(Debug, Default)]
pub struct NoopExtensionHost;

impl RuntimeExtensionHook for NoopExtensionHost {}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use soxu_protocol::{
        ActiveTurnBehavior, ApprovalAuditRecord, ApprovalItemSnapshot, ApprovalResolutionKind,
        DraftSnapshot, ErrorCode, PermissionLevel, QueuedMessageItem, SessionTarget,
        ThinkingEffort, WorkspaceBucket, WorkspaceSummary,
    };

    fn make_workspace(
        id: &str,
        project_id: &str,
        bucket: WorkspaceBucket,
        archived: bool,
        offset_secs: i64,
    ) -> WorkspaceSummary {
        WorkspaceSummary {
            workspace_id: id.to_owned(),
            project_id: project_id.to_owned(),
            project_name: "Soxu".to_owned(),
            display_path: "~/projects/Soxu".to_owned(),
            branch: id.to_owned(),
            isolation: WorkspaceIsolation::Local,
            agent_id: "codex".to_owned(),
            model_id: "group/gpt-6.2-sol".to_owned(),
            bucket,
            summary: String::new(),
            diff_stat: "+0 -0".to_owned(),
            label: None,
            archived,
            needs_recovery: false,
            recovery_reason: None,
            updated_at: Utc::now() + Duration::seconds(offset_secs),
        }
    }

    #[test]
    fn noop_extension_preserves_env() {
        let host = NoopExtensionHost;
        let mut env = BTreeMap::from([("RUST_LOG".to_owned(), "info".to_owned())]);
        host.transform_launch_env(&mut env);
        assert_eq!(env.get("RUST_LOG").map(String::as_str), Some("info"));
    }

    #[test]
    fn sorts_workspaces_by_archive_bucket_and_recency() {
        let mut list = vec![
            make_workspace("w-done", "p1", WorkspaceBucket::Done, false, 100),
            make_workspace("w-arch", "p1", WorkspaceBucket::NeedsInput, true, 999),
            make_workspace("w-run-old", "p1", WorkspaceBucket::Running, false, 10),
            make_workspace("w-run-new", "p1", WorkspaceBucket::Running, false, 50),
            make_workspace("w-fail", "p1", WorkspaceBucket::Failed, false, 5),
            make_workspace("w-need", "p1", WorkspaceBucket::NeedsInput, false, 1),
        ];

        let orch = CoreOrchestrator::new();
        orch.sort_workspaces(&mut list);

        let ids: Vec<&str> = list.iter().map(|w| w.workspace_id.as_str()).collect();
        assert_eq!(
            ids,
            vec![
                "w-need",
                "w-fail",
                "w-run-new",
                "w-run-old",
                "w-done",
                "w-arch"
            ]
        );
        assert_eq!(
            orch.project_bucket(&list, "p1"),
            WorkspaceBucket::NeedsInput
        );
    }

    #[test]
    fn reconciles_lost_controls_across_agents_and_models() {
        let orch = CoreOrchestrator::new();
        let outcome = orch.reconcile(ReconcileRequest {
            agent_id: "copilot".to_owned(),
            current_model: Some("gemini-4-carbon".to_owned()),
            current_perm: Some(PermissionLevel::Turbo),
            current_thinking: Some(ThinkingEffort::Xhigh),
            current_behavior: ActiveTurnBehavior::Steer,
        });

        assert_eq!(outcome.agent_id, "copilot");
        assert_eq!(outcome.model, "group/gpt-6.2-sol");
        assert_eq!(outcome.permission, PermissionLevel::Ask);
        assert_eq!(outcome.thinking, None);
        assert_eq!(outcome.behavior, ActiveTurnBehavior::Queue);
        assert_eq!(outcome.lost_controls.len(), 4);
    }

    #[test]
    fn assesses_safety_redlines_and_blocks_session_allow() {
        let orch = CoreOrchestrator::new();
        let danger = orch.assess_safety("Bash", "Remove-Item -Recurse -Force ./target-fixtures");
        assert!(danger.high_risk);
        assert!(!danger.allow_session_permitted);
        assert!(
            danger
                .matched_rules
                .contains(&"destructive-recursive-delete".to_owned())
        );

        let cfg = orch.assess_safety("Write", "~/.config/magpie/applied.json");
        assert!(cfg.high_risk);
        assert!(!cfg.allow_session_permitted);

        let safe = orch.assess_safety("Bash", "cargo test --workspace");
        assert!(!safe.high_risk);
        assert!(safe.allow_session_permitted);

        let approval = ApprovalItemSnapshot {
            approval_id: "apr_1".to_owned(),
            session_id: "sess_1".to_owned(),
            target: SessionTarget::QuickChat {
                chat_id: "qc_1".to_owned(),
            },
            branch_label: "快速对话".to_owned(),
            agent_id: "codex".to_owned(),
            tool_kind: "Bash".to_owned(),
            command_or_target: "rm -rf /tmp/demo".to_owned(),
            risk_note: danger.risk_note_zh,
            high_risk: true,
            allow_session_permitted: false,
            diff_preview: Vec::new(),
            audit: None,
            created_at: Utc::now(),
        };

        let blocked = orch
            .validate_approval(&approval, ApprovalResolutionKind::AllowSession)
            .expect_err("high risk must block allow-session");
        assert_eq!(blocked.code, ErrorCode::SafetyRedlineBlocked);

        let mut approved = approval;
        approved.audit = Some(ApprovalAuditRecord {
            resolution: ApprovalResolutionKind::ApproveOnce,
            resolved_by: "mobile".to_owned(),
            guidance_note: None,
            resolved_at: Utc::now(),
        });
        let conflict = orch
            .validate_approval(&approved, ApprovalResolutionKind::ApproveOnce)
            .expect_err("double approval must fail with conflict");
        assert_eq!(conflict.code, ErrorCode::Conflict);
    }

    #[test]
    fn draft_store_rejects_stale_generations() {
        let mut orch = CoreOrchestrator::new();
        let target = SessionTarget::QuickChat {
            chat_id: "qc_1".to_owned(),
        };

        let r2 = orch.save_draft(DraftSnapshot {
            target: target.clone(),
            session_id: "sess_1".to_owned(),
            text: "第二版".to_owned(),
            chips: Vec::new(),
            generation: 2,
            updated_at: Utc::now(),
        });
        assert!(matches!(r2, DraftSaveOutcome::Accepted(_)));

        let r1 = orch.save_draft(DraftSnapshot {
            target,
            session_id: "sess_1".to_owned(),
            text: "过期第一版".to_owned(),
            chips: Vec::new(),
            generation: 1,
            updated_at: Utc::now(),
        });
        assert_eq!(
            r1,
            DraftSaveOutcome::RejectedStale {
                current_generation: 2
            }
        );
        assert_eq!(
            orch.draft_for("sess_1").map(|d| d.text.as_str()),
            Some("第二版")
        );
    }

    #[test]
    fn queue_manager_promotes_atomically_and_prevents_double_consume() {
        let mut orch = CoreOrchestrator::new();
        orch.enqueue_turn(QueuedMessageItem {
            queue_id: "q_1".to_owned(),
            session_id: "sess_1".to_owned(),
            text: "先跑测试".to_owned(),
            chips: Vec::new(),
            behavior: ActiveTurnBehavior::Queue,
            created_at: Utc::now(),
        });

        let invalid = orch
            .promote_queued_turn("sess_1", "q_1", ActiveTurnBehavior::Queue)
            .expect_err("cannot promote to queue");
        assert_eq!(invalid.code, ErrorCode::ValidationFailed);

        let promo = orch
            .promote_queued_turn("sess_1", "q_1", ActiveTurnBehavior::Steer)
            .expect("promote queued turn");
        assert_eq!(promo.target_behavior, ActiveTurnBehavior::Steer);
        assert!(orch.queued_turns("sess_1").is_empty());

        let second = orch
            .promote_queued_turn("sess_1", "q_1", ActiveTurnBehavior::Interrupt)
            .expect_err("already promoted item must be gone");
        assert_eq!(second.code, ErrorCode::NotFound);
    }
}
