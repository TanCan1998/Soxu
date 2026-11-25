//! Soxu 领域编排器聚合入口，统一协调能力协商、安全红线、排队提级与草稿代次。

use crate::buckets::{
    project_aggregated_bucket, sort_quick_chats_by_attention, sort_workspaces_by_attention,
};
use crate::capabilities::{
    ReconcileOutcome, ReconcileRequest, default_agent_catalog, default_model_catalog,
    reconcile_capabilities,
};
use crate::drafts::{DraftSaveOutcome, DraftStore};
use crate::queue::{QueuePromotion, TurnQueueManager};
use crate::redline::{RedlineAssessment, assess_command_safety, validate_approval_resolution};
use soxu_protocol::{
    ActionableError, ActiveTurnBehavior, AgentCapabilityDescriptor, ApprovalItemSnapshot,
    ApprovalResolutionKind, DraftSnapshot, ModelDescriptor, QueuedMessageItem, QuickChatSummary,
    WorkspaceBucket, WorkspaceSummary,
};

/// 核心领域编排器（纯内存状态机，零磁盘与终端 I/O）。
#[derive(Debug, Clone)]
pub struct CoreOrchestrator {
    agents: Vec<AgentCapabilityDescriptor>,
    models: Vec<ModelDescriptor>,
    drafts: DraftStore,
    queues: TurnQueueManager,
}

impl Default for CoreOrchestrator {
    fn default() -> Self {
        Self::new()
    }
}

impl CoreOrchestrator {
    #[must_use]
    pub fn new() -> Self {
        Self {
            agents: default_agent_catalog(),
            models: default_model_catalog(),
            drafts: DraftStore::new(),
            queues: TurnQueueManager::new(),
        }
    }

    #[must_use]
    pub fn agents(&self) -> &[AgentCapabilityDescriptor] {
        &self.agents
    }

    #[must_use]
    pub fn models(&self) -> &[ModelDescriptor] {
        &self.models
    }

    pub fn set_agent_catalog(&mut self, agents: Vec<AgentCapabilityDescriptor>) {
        self.agents = agents;
    }

    pub fn set_model_catalog(&mut self, models: Vec<ModelDescriptor>) {
        self.models = models;
    }

    #[must_use]
    pub fn reconcile(&self, req: ReconcileRequest) -> ReconcileOutcome {
        reconcile_capabilities(&self.agents, &self.models, req)
    }

    #[must_use]
    pub fn assess_safety(&self, tool_kind: &str, command_or_target: &str) -> RedlineAssessment {
        assess_command_safety(tool_kind, command_or_target)
    }

    pub fn validate_approval(
        &self,
        approval: &ApprovalItemSnapshot,
        resolution: ApprovalResolutionKind,
    ) -> Result<(), ActionableError> {
        validate_approval_resolution(approval, resolution)
    }

    pub fn save_draft(&mut self, draft: DraftSnapshot) -> DraftSaveOutcome {
        self.drafts.save(draft)
    }

    #[must_use]
    pub fn draft_for(&self, session_id: &str) -> Option<&DraftSnapshot> {
        self.drafts.get(session_id)
    }

    pub fn clear_draft(&mut self, session_id: &str) -> Option<DraftSnapshot> {
        self.drafts.clear(session_id)
    }

    #[must_use]
    pub fn all_drafts(&self) -> Vec<DraftSnapshot> {
        self.drafts.all()
    }

    pub fn enqueue_turn(&mut self, item: QueuedMessageItem) -> Vec<QueuedMessageItem> {
        self.queues.enqueue(item)
    }

    pub fn promote_queued_turn(
        &mut self,
        session_id: &str,
        queue_id: &str,
        target_behavior: ActiveTurnBehavior,
    ) -> Result<QueuePromotion, ActionableError> {
        self.queues.promote(session_id, queue_id, target_behavior)
    }

    pub fn cancel_queued_turn(
        &mut self,
        session_id: &str,
        queue_id: &str,
    ) -> Result<Vec<QueuedMessageItem>, ActionableError> {
        self.queues.cancel(session_id, queue_id)
    }

    pub fn pop_queued_turn(&mut self, session_id: &str) -> Option<QueuedMessageItem> {
        self.queues.pop_next(session_id)
    }

    #[must_use]
    pub fn queued_turns(&self, session_id: &str) -> Vec<QueuedMessageItem> {
        self.queues.snapshot(session_id)
    }

    pub fn sort_workspaces(&self, workspaces: &mut [WorkspaceSummary]) {
        sort_workspaces_by_attention(workspaces);
    }

    pub fn sort_quick_chats(&self, chats: &mut [QuickChatSummary]) {
        sort_quick_chats_by_attention(chats);
    }

    #[must_use]
    pub fn project_bucket(
        &self,
        workspaces: &[WorkspaceSummary],
        project_id: &str,
    ) -> WorkspaceBucket {
        project_aggregated_bucket(workspaces, project_id)
    }
}
