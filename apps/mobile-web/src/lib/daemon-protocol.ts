/**
 * Soxu 协议 v1 的 TypeScript 同构类型与编解码。
 *
 * 字段与 `crates/soxu-protocol` v1 契约一一对应（serde 默认命名策略：
 * enum 变体 snake_case tagged，结构体字段 snake_case；SoxuPreferences 走 camelCase）。
 * 严禁在本文件新增任何协议字段；确需扩展先走独立契约 PR。
 */

/** 与 `soxu_protocol::PROTOCOL_VERSION` 对齐。 */
export const PROTOCOL_VERSION = 1;

// ---------- domain.rs ----------

export type TurnBehavior = "queue" | "steer" | "interrupt";
export type WorkspaceBucket = "needs_input" | "failed" | "running" | "attention" | "done";
export type PermissionLevel = "plan" | "read-only" | "ask" | "auto-edit" | "full-access" | "turbo";
export type ThinkingEffort = "low" | "medium" | "high" | "xhigh" | "max";
export type TurnLifecycleState = "idle" | "preparing" | "running";
export type TurnMessageRole = "user" | "assistant" | "system";
export type SubagentStatus = "running" | "needs_input" | "completed" | "failed" | "cancelled";
export type ComposerChipKind = "file" | "folder" | "diff" | "terminal" | "image" | "skill" | "handoff";

export interface SessionTargetWorkspace {
  kind: "workspace";
  workspace_id: string;
}
export interface SessionTargetQuickChat {
  kind: "quick_chat";
  chat_id: string;
}
export type SessionTarget = SessionTargetWorkspace | SessionTargetQuickChat;

export interface WorkspaceIsolationLocal {
  kind: "local";
}
export interface WorkspaceIsolationWorktree {
  kind: "worktree";
  branch: string;
  base_ref: string;
}
export type WorkspaceIsolation = WorkspaceIsolationLocal | WorkspaceIsolationWorktree;

export interface ProjectSummary {
  project_id: string;
  name: string;
  display_path: string;
  root_path: string;
  is_git_repo: boolean;
  default_branch: string | null;
  aggregated_bucket: WorkspaceBucket;
  workspace_count: number;
}

export interface WorkspaceSummary {
  workspace_id: string;
  project_id: string;
  project_name: string;
  display_path: string;
  branch: string;
  isolation: WorkspaceIsolation;
  agent_id: string;
  model_id: string;
  bucket: WorkspaceBucket;
  summary: string;
  diff_stat: string;
  label: string | null;
  archived: boolean;
  needs_recovery: boolean;
  recovery_reason: string | null;
  updated_at: string;
}

export interface QuickChatSummary {
  chat_id: string;
  title: string;
  agent_id: string;
  model_id: string;
  bucket: WorkspaceBucket;
  archived: boolean;
  updated_at: string;
}

export interface SessionSummary {
  session_id: string;
  target: SessionTarget;
  title: string;
  agent_id: string;
  model_id: string;
  thinking: ThinkingEffort | null;
  permission: PermissionLevel;
  lifecycle: TurnLifecycleState;
  bucket: WorkspaceBucket;
  archived: boolean;
  native_session_id: string | null;
  created_at: string;
  updated_at: string;
}

export interface PermissionOptionMeta {
  id: PermissionLevel;
  label: string;
  description: string;
  tone: "neutral" | "accent" | "warning" | "danger";
}

export interface ModelDescriptor {
  model_id: string;
  display_name: string;
  tier: string;
  note: string;
  context_window_limit: number;
  thinking_options: ThinkingEffort[];
  native_direct_only_agent: string | null;
}

export interface AgentCapabilityDescriptor {
  agent_id: string;
  display_name: string;
  binary_name: string;
  installed: boolean;
  version: string | null;
  supported_models: string[];
  permissions: PermissionOptionMeta[];
  supports_thinking: boolean;
  supports_live_steer: boolean;
}

export interface ApprovalAuditRecord {
  resolution: "approve_once" | "allow_session" | "reject_with_guidance";
  resolved_by: string;
  guidance_note: string | null;
  resolved_at: string;
}

export interface ApprovalItemSnapshot {
  approval_id: string;
  session_id: string;
  target: SessionTarget;
  branch_label: string;
  agent_id: string;
  tool_kind: string;
  command_or_target: string;
  risk_note: string;
  high_risk: boolean;
  allow_session_permitted: boolean;
  diff_preview: string[];
  audit: ApprovalAuditRecord | null;
  created_at: string;
}

export interface QueuedMessageItem {
  queue_id: string;
  session_id: string;
  text: string;
  chips: ComposerChip[];
  behavior: TurnBehavior;
  created_at: string;
}

export interface ComposerChip {
  id: string;
  kind: ComposerChipKind;
  label: string;
  path_or_ref: string | null;
  target_agent: string | null;
  target_model: string | null;
  target_thinking: ThinkingEffort | null;
}

export interface LostControlNotice {
  field: string;
  previous_value: string;
  fallback_value: string;
  reason_zh: string;
}

export interface PairedDeviceSummary {
  device_id: string;
  device_name: string;
  platform: string;
  last_seen_at: string;
  online: boolean;
  revoked: boolean;
}

// ---------- preferences.rs（camelCase，与前端既有 SoxuPreferences 同构） ----------

export interface ProtocolPreferences {
  themeMode: "dark" | "light" | "system";
  locale: string;
  accentPreset: string;
  customAccentHex: string;
  density: "compact" | "comfortable";
  uiLineHeight: "compact" | "normal" | "relaxed";
  timestampFormat: "relative" | "absolute";
  leftSidebarWidth: number;
  rightInspectorWidth: number;
  uiFontPreset: string;
  codeFollowsUi: boolean;
  codeFontPreset: string;
  uiFontSize: number;
  codeFontSize: number;
  tabSize: number;
  showLineNumbers: boolean;
  codeWordWrap: boolean;
  codeLigatures: boolean;
  reduceMotion: boolean;
  probeIntervalSec: number;
  autoCheckpoint: boolean;
  sharedCargoTarget: boolean;
  warnUncommittedOnClose: boolean;
  commitEmojiPrefix: boolean;
  worktreeLocation: "sibling" | "central";
  defaultBranch: string;
  defaultPermMode: PermissionLevel;
  commandTimeoutSec: number;
  redactHomePath: boolean;
  lanRemoteEnabled: boolean;
  remoteIdleLockMin: number;
}

// ---------- rpc.rs ----------

export interface HealthSnapshot {
  status: string;
  daemon_version: string;
  protocol_version: number;
  magpie_detected: boolean;
}

export type ClientKind = "desktop" | "mobile_pwa" | "cli";

export interface ClientHello {
  client_kind: ClientKind;
  client_version: string;
  protocol_version: number;
  after_seq: number | null;
  device_id: string | null;
}

export interface StateSnapshot {
  projects: ProjectSummary[];
  workspaces: WorkspaceSummary[];
  quick_chats: QuickChatSummary[];
  sessions: SessionSummary[];
  pending_approvals: ApprovalItemSnapshot[];
  pending_inputs: PendingUserInputRequest[];
  subagents: SubagentSummary[];
  agents: AgentCapabilityDescriptor[];
  models: ModelDescriptor[];
  preferences: ProtocolPreferences;
  paired_devices: PairedDeviceSummary[];
}

export interface ServerHello {
  daemon_version: string;
  daemon_fingerprint: string;
  protocol_version: number;
  magpie_detected: boolean;
  /** 本次快照对应的固定事件头部 `H`；旧守护进程不下发（缺省 → 回退 `latest_seq`）。 */
  snapshot_seq?: number | null;
  latest_seq: number;
  snapshot: StateSnapshot;
}

// ---------- pairing.rs ----------

export type PairingTransportMode = "lan_direct" | "encrypted_relay";

export interface PairingOfferPayload {
  offer_id: string;
  daemon_fingerprint: string;
  endpoint_url: string;
  transport: PairingTransportMode;
  one_time_token: string;
  expires_at: string;
}

export interface PairingCodeSnapshot {
  code_id: string;
  six_digit_code: string;
  offer_url: string;
  expires_at: string;
  ttl_seconds: number;
}

export type PairingCredentialInput =
  | { kind: "offer_token"; value: string }
  | { kind: "six_digit_code"; value: string };

export interface ClaimPairingRequest {
  device_name: string;
  platform: string;
  credential: PairingCredentialInput;
}

export interface ClaimPairingResponse {
  device_id: string;
  bearer_token: string;
  daemon_fingerprint: string;
  protocol_version: number;
  paired_at: string;
}

// ---------- error.rs ----------

export type ErrorCode =
  | "protocol_mismatch"
  | "unauthorized"
  | "pairing_expired"
  | "not_found"
  | "validation_failed"
  | "safety_redline_blocked"
  | "conflict"
  | "driver_unavailable"
  | "workspace_damaged"
  | "internal_error"
  | "unsupported"
  | "identity_changed"
  | "policy_denied"
  | "safety_incomplete"
  | "partial_needs_recovery"
  | "ref_published_index_pending"
  | "catch_up_failed";

export interface ActionableError {
  code: ErrorCode;
  summary_zh: string;
  impact_scope_zh: string;
  remediation_zh: string;
  diagnostic_detail: string | null;
}

// ---------- RPC 命令与响应（本客户端实际使用的子集，字段与协议 v1 完全一致） ----------

export type RpcCommand =
  | { method: "get_state_snapshot"; params: null }
  | { method: "catch_up_events"; params: { after_seq: number; limit: number | null } }
  | {
      method: "create_quick_chat";
      params: {
        title: string | null;
        agent_id: string;
        model_id: string | null;
        permission: PermissionLevel | null;
        thinking: ThinkingEffort | null;
      };
    }
  | { method: "submit_turn"; params: SubmitTurnParams }
  | { method: "resolve_approval"; params: ResolveApprovalParams }
  | { method: "submit_user_input"; params: PendingUserInputAnswer }
  | { method: "stop_turn"; params: { session_id: string } }
  | { method: "cancel_queued_turn"; params: { session_id: string; queue_id: string } }
  | { method: "update_preferences"; params: ProtocolPreferences }
  | { method: "create_pairing_offer"; params: { transport: PairingTransportMode } }
  | { method: "claim_pairing"; params: ClaimPairingRequest }
  | { method: "revoke_paired_device"; params: { device_id: string } };

export interface SubmitTurnParams {
  session_id: string;
  text: string;
  chips: ComposerChip[];
  behavior: TurnBehavior;
  handoff: null;
  handoff_scope: null;
}

export interface ResolveApprovalParams {
  approval_id: string;
  resolution: "approve_once" | "allow_session" | "reject_with_guidance";
  guidance_note: string | null;
  resolved_by: string;
}

export interface PendingUserInputAnswerItem {
  question_id: string;
  selected_option_ids: string[];
  custom_note: string | null;
}

export interface PendingUserInputAnswer {
  request_id: string;
  answers: PendingUserInputAnswerItem[];
  submitted_at: string;
}

export interface AskQuestionOption {
  option_id: string;
  label: string;
  description: string | null;
  recommended: boolean;
}

export interface PendingUserInputQuestion {
  question_id: string;
  prompt: string;
  multi_select: boolean;
  options: AskQuestionOption[];
}

export interface PendingUserInputRequest {
  request_id: string;
  session_id: string;
  title: string;
  questions: PendingUserInputQuestion[];
  created_at: string;
}

export interface SubagentSummary {
  subagent_id: string;
  parent_session_id: string;
  agent_id: string;
  model_id: string;
  task_title: string;
  status: SubagentStatus;
  elapsed_sec: number;
  recent_activities: string[];
  detached_session_id: string | null;
}

export type RpcResultData =
  | { kind: "ack"; data: null }
  | { kind: "state_snapshot"; data: StateSnapshot }
  | { kind: "events"; data: { events: EventEnvelope[]; latest_seq: number } }
  | { kind: "pairing_claimed"; data: ClaimPairingResponse }
  | { kind: "preferences_updated"; data: ProtocolPreferences }
  | { kind: "session_created"; data: SessionSummary }
  | { kind: "quick_chat_created"; data: QuickChatSummary }
  | {
      kind: "turn_submitted";
      data: { session_id: string; lifecycle: TurnLifecycleState; queued_id: string | null };
    }
  | { kind: "pairing_offer_created"; data: { offer: PairingOfferPayload; code: PairingCodeSnapshot } };

export interface RpcRequest {
  request_id: string;
  command: RpcCommand;
}

export interface RpcResponse {
  request_id: string;
  ok: boolean;
  result: RpcResultData | null;
  error: ActionableError | null;
}

// ---------- events.rs ----------

export interface EventEnvelope<T = DaemonEvent> {
  protocol_version: number;
  seq: number;
  timestamp: string;
  payload: T;
}

export interface CatchUpCompletedPayload {
  event: "catch_up_completed";
  data: { from_seq: number; to_seq: number; count: number };
}

export interface SnapshotSyncedPayload {
  event: "snapshot_synced";
  data: StateSnapshot;
}

export interface SessionUpdatedPayload {
  event: "session_updated";
  data: SessionSummary;
}

export interface WorkspaceUpdatedPayload {
  event: "workspace_updated";
  data: WorkspaceSummary;
}

export interface QuickChatUpdatedPayload {
  event: "quick_chat_updated";
  data: QuickChatSummary;
}

export interface ApprovalRequestedPayload {
  event: "approval_requested";
  data: ApprovalItemSnapshot;
}

export interface ApprovalResolvedPayload {
  event: "approval_resolved";
  data: { approval_id: string; session_id: string; audit: ApprovalAuditRecord };
}

export interface QueueUpdatedPayload {
  event: "queue_updated";
  data: { session_id: string; queued: QueuedMessageItem[] };
}

export interface CapabilitiesLostControlsPayload {
  event: "capabilities_lost_controls";
  data: { session_id: string; notices: LostControlNotice[] };
}

export interface PreferencesChangedPayload {
  event: "preferences_changed";
  data: ProtocolPreferences;
}

export interface PairingDeviceUpdatedPayload {
  event: "pairing_device_updated";
  data: PairedDeviceSummary;
}

export interface ErrorRaisedPayload {
  event: "error_raised";
  data: { session_id: string | null; error: ActionableError };
}

export interface UserInputRequestedPayload {
  event: "user_input_requested";
  data: PendingUserInputRequest;
}

export interface UserInputSubmittedPayload {
  event: "user_input_submitted";
  data: PendingUserInputAnswer;
}

export interface TurnMessageUpsertedPayload {
  event: "turn_message_upserted";
  data: TurnMessageSnapshot;
}

export interface TurnMessageSnapshot {
  message_id: string;
  session_id: string;
  turn_index: number;
  role: TurnMessageRole;
  author: string;
  model_id: string | null;
  thinking: ThinkingEffort | null;
  text: string;
  chips: ComposerChip[];
  steer_badge: string | null;
  reasoning_duration_sec: number | null;
  reasoning_steps: string[];
  tools: unknown[];
  handoff_anchor: unknown;
  checkpoint: unknown;
  created_at: string;
}

export type DaemonEvent =
  | CatchUpCompletedPayload
  | SnapshotSyncedPayload
  | SessionUpdatedPayload
  | WorkspaceUpdatedPayload
  | QuickChatUpdatedPayload
  | ApprovalRequestedPayload
  | ApprovalResolvedPayload
  | QueueUpdatedPayload
  | CapabilitiesLostControlsPayload
  | PreferencesChangedPayload
  | PairingDeviceUpdatedPayload
  | ErrorRaisedPayload
  | UserInputRequestedPayload
  | UserInputSubmittedPayload
  | TurnMessageUpsertedPayload;

/** 解析守护进程下发的未知事件负载；无法识别时返回 null（容错不崩溃）。 */
export function decodeEventEnvelope(raw: string): EventEnvelope | null {
  try {
    const env = JSON.parse(raw) as EventEnvelope;
    if (typeof env.seq !== "number" || typeof env.payload?.event !== "string") return null;
    return env;
  } catch {
    return null;
  }
}
