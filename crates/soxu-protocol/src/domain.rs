//! Soxu 核心领域对象、工作区与快速对话契约、权限与工具审批结构。

use crate::capabilities::ThinkingEffort;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 工作区文件系统隔离模式。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkspaceIsolation {
    /// 直接绑定本地目录。
    Local,
    /// 基于 Git Worktree 的独立分支隔离。
    Worktree { branch: String, base_ref: String },
}

/// 左栏与会话列表注意力优先级状态桶（`needs_input > failed > running > attention > done`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceBucket {
    NeedsInput,
    Failed,
    Running,
    Attention,
    #[default]
    Done,
}

impl WorkspaceBucket {
    /// 返回状态桶排序权重，数值越小越靠前置顶。
    #[must_use]
    pub const fn priority_rank(self) -> u8 {
        match self {
            Self::NeedsInput => 0,
            Self::Failed => 1,
            Self::Running => 2,
            Self::Attention => 3,
            Self::Done => 4,
        }
    }
}

/// 会话归属目标：绑定 Git/目录工作区，或不绑定项目的无项目快速对话。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SessionTarget {
    Workspace { workspace_id: String },
    QuickChat { chat_id: String },
}

/// 一级项目文件夹摘要。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectSummary {
    pub project_id: String,
    pub name: String,
    /// 经过 `~` 脱敏后的展示路径，严禁直接拼接真实系统用户名。
    pub display_path: String,
    pub root_path: String,
    pub is_git_repo: bool,
    pub default_branch: Option<String>,
    pub aggregated_bucket: WorkspaceBucket,
    pub workspace_count: u32,
}

/// 二级工作区/Worktree 节点摘要。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceSummary {
    pub workspace_id: String,
    pub project_id: String,
    pub project_name: String,
    pub display_path: String,
    pub branch: String,
    pub isolation: WorkspaceIsolation,
    pub agent_id: String,
    pub model_id: String,
    pub bucket: WorkspaceBucket,
    pub summary: String,
    pub diff_stat: String,
    pub label: Option<String>,
    pub archived: bool,
    pub needs_recovery: bool,
    pub recovery_reason: Option<String>,
    pub updated_at: DateTime<Utc>,
}

/// 无项目快速对话（Quick Chat）摘要。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuickChatSummary {
    pub chat_id: String,
    pub title: String,
    pub agent_id: String,
    pub model_id: String,
    pub bucket: WorkspaceBucket,
    pub archived: bool,
    pub updated_at: DateTime<Utc>,
}

/// 会话当前回合生命周期状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TurnLifecycleState {
    #[default]
    Idle,
    Preparing,
    Running,
}

/// 当目标 Agent 正处于活跃推理轮次时，新消息的投递策略（对标 Paseo）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ActiveTurnBehavior {
    /// 默认排队：等待当前轮次自然结束后再消费。
    #[default]
    Queue,
    /// 中途纠偏：在下一步工具边界注入指令并继续工作。
    Steer,
    /// 立即中断：停止当前轮次并开启新轮次。
    Interrupt,
}

/// 输入舱常驻纯图标执行权限档位（与前端 `types.ts` 保持 kebab-case 对齐）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum PermissionLevel {
    Plan,
    ReadOnly,
    #[default]
    Ask,
    AutoEdit,
    FullAccess,
    Turbo,
}

/// 输入舱上下文附件与移交标签类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComposerChipKind {
    File,
    Folder,
    Diff,
    Terminal,
    Image,
    Skill,
    Handoff,
}

/// 输入舱与消息气泡附带的结构化上下文标签芯片。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComposerChip {
    pub id: String,
    pub kind: ComposerChipKind,
    pub label: String,
    pub path_or_ref: Option<String>,
    pub target_agent: Option<String>,
    pub target_model: Option<String>,
    pub target_thinking: Option<ThinkingEffort>,
}

/// 活跃回合待发队列条目。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueuedMessageItem {
    pub queue_id: String,
    pub session_id: String,
    pub text: String,
    pub chips: Vec<ComposerChip>,
    pub behavior: ActiveTurnBehavior,
    pub created_at: DateTime<Utc>,
}

/// `@agent` 跨代理移交选型配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandoffSelection {
    pub target_agent: String,
    pub target_model: String,
    pub target_thinking: Option<ThinkingEffort>,
}

/// `@agent` 移交附带的上下文范围选项。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandoffScopeOptions {
    pub summary_turns: u32,
    pub include_uncommitted_diff_paths: bool,
    pub spawn_worktree: bool,
}

impl Default for HandoffScopeOptions {
    fn default() -> Self {
        Self {
            summary_turns: 3,
            include_uncommitted_diff_paths: true,
            spawn_worktree: false,
        }
    }
}

/// 跨 CLI 移交双向溯源锚点卡。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandoffAnchorCard {
    pub handoff_id: String,
    pub source_session_id: String,
    pub source_agent: String,
    pub source_model: String,
    pub target_session_id: String,
    pub target_agent: String,
    pub target_model: String,
    pub target_thinking: Option<ThinkingEffort>,
    pub summary_preview: String,
    pub attached_files_count: u32,
    pub created_at: DateTime<Utc>,
}

/// 子代理运行状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubagentStatus {
    Running,
    NeedsInput,
    Completed,
    Failed,
    Cancelled,
}

/// 子代理委派轨道节点摘要。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubagentSummary {
    pub subagent_id: String,
    pub parent_session_id: String,
    pub agent_id: String,
    pub model_id: String,
    pub task_title: String,
    pub status: SubagentStatus,
    pub elapsed_sec: u64,
    pub recent_activities: Vec<String>,
    /// 若用户点击「独立成页签 (Detach)」，记录提升后的会话 ID。
    pub detached_session_id: Option<String>,
}

/// 5 类结构化工具调用卡片分类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCallCategory {
    ReadGroup,
    FileEdit,
    TerminalExec,
    McpCall,
    SubagentDelegate,
}

/// MCP 工具路由来源徽章。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpRouteBadge {
    MagpieShared,
    DirectWorkspace,
    NativeDirect,
}

/// 结构化工具调用运行态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCallStatus {
    Running,
    Succeeded,
    Failed,
    PendingApproval,
    Cancelled,
}

/// 对话流中的结构化工具调用卡片快照。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCardSnapshot {
    pub tool_call_id: String,
    pub category: ToolCallCategory,
    pub title: String,
    pub summary: String,
    pub status: ToolCallStatus,
    pub duration_ms: Option<u64>,
    pub mcp_route: Option<McpRouteBadge>,
    pub preview_lines: Vec<String>,
    pub hidden_lines_count: u32,
    pub exit_code: Option<i32>,
    pub linked_subagent_id: Option<String>,
}

/// 权限审批决议动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalResolutionKind {
    ApproveOnce,
    AllowSession,
    RejectWithGuidance,
}

/// 权限审批消解审计记录（跨端幂等）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalAuditRecord {
    pub resolution: ApprovalResolutionKind,
    pub resolved_by: String,
    pub guidance_note: Option<String>,
    pub resolved_at: DateTime<Utc>,
}

/// 双端实时同步权限审批卡片快照。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalItemSnapshot {
    pub approval_id: String,
    pub session_id: String,
    pub target: SessionTarget,
    pub branch_label: String,
    pub agent_id: String,
    pub tool_kind: String,
    pub command_or_target: String,
    pub risk_note: String,
    pub high_risk: bool,
    /// 命中高危红线时必须强制为 `false`，禁止会话级自动放行。
    pub allow_session_permitted: bool,
    pub diff_preview: Vec<String>,
    pub audit: Option<ApprovalAuditRecord>,
    pub created_at: DateTime<Utc>,
}

/// 结构化提问选项。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AskQuestionOption {
    pub option_id: String,
    pub label: String,
    pub description: Option<String>,
    pub recommended: bool,
}

/// 单道待确认问题。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingUserInputQuestion {
    pub question_id: String,
    pub prompt: String,
    pub multi_select: bool,
    pub options: Vec<AskQuestionOption>,
}

/// `PendingUserInput` 结构化提问请求卡。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingUserInputRequest {
    pub request_id: String,
    pub session_id: String,
    pub title: String,
    pub questions: Vec<PendingUserInputQuestion>,
    pub created_at: DateTime<Utc>,
}

/// 单题回答负载。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingUserInputAnswerItem {
    pub question_id: String,
    pub selected_option_ids: Vec<String>,
    pub custom_note: Option<String>,
}

/// 结构化提问卡提交答案。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingUserInputAnswer {
    pub request_id: String,
    pub answers: Vec<PendingUserInputAnswerItem>,
    pub submitted_at: DateTime<Utc>,
}

/// 对话轮次末尾轻量 Git 检查点锚点。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointAnchor {
    pub checkpoint_id: String,
    pub turn_index: u32,
    pub commit_oid: String,
    pub changed_files: u32,
    pub diff_stat: String,
}

/// 对话消息角色。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnMessageRole {
    User,
    Assistant,
    System,
}

/// 对话消息快照。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TurnMessageSnapshot {
    pub message_id: String,
    pub session_id: String,
    pub turn_index: u32,
    pub role: TurnMessageRole,
    pub author: String,
    pub model_id: Option<String>,
    pub thinking: Option<ThinkingEffort>,
    pub text: String,
    pub chips: Vec<ComposerChip>,
    pub steer_badge: Option<String>,
    pub reasoning_duration_sec: Option<f64>,
    pub reasoning_steps: Vec<String>,
    pub tools: Vec<ToolCardSnapshot>,
    pub handoff_anchor: Option<HandoffAnchorCard>,
    pub checkpoint: Option<CheckpointAnchor>,
    pub created_at: DateTime<Utc>,
}

/// 防抖落盘草稿快照（附带单调 `generation` 版本号防止旧写覆盖新写）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftSnapshot {
    pub target: SessionTarget,
    pub session_id: String,
    pub text: String,
    pub chips: Vec<ComposerChip>,
    pub generation: u64,
    pub updated_at: DateTime<Utc>,
}

/// 会话完整摘要（用于页签、左栏绑定与恢复）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSummary {
    pub session_id: String,
    pub target: SessionTarget,
    pub title: String,
    pub agent_id: String,
    pub model_id: String,
    pub thinking: Option<ThinkingEffort>,
    pub permission: PermissionLevel,
    pub lifecycle: TurnLifecycleState,
    pub bucket: WorkspaceBucket,
    pub archived: bool,
    pub native_session_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
