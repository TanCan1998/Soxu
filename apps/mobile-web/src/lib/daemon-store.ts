/**
 * Daemon 快照与事件流到既有前端视图模型（`types.ts`）的映射层。
 *
 * 设计边界：真实数据优先；daemon 未提供的字段（如受管脚本、定时任务）
 * 保留空集占位而非伪造数据，等对应协议能力接入后自然补齐。
 */

import type {
  ApprovalItemSnapshot,
  DaemonEvent,
  EventEnvelope,
  PendingUserInputRequest,
  QuickChatSummary,
  SessionSummary,
  StateSnapshot,
  TurnMessageSnapshot,
  WorkspaceSummary,
} from "./daemon-protocol";
import type {
  ApprovalItem,
  AskQuestionState,
  QuickChatItem,
  TurnMessage,
  WorkspaceBucket,
  WorkspaceItem,
} from "@/types";

const BUCKET_ORDER: Record<WorkspaceBucket, number> = {
  needs_input: 0,
  failed: 1,
  running: 2,
  attention: 3,
  done: 4,
};

/** 顺序参考 soxu-core sort_workspaces_by_attention（注意力优先置顶）。 */
export function bucketRank(bucket: WorkspaceBucket): number {
  return BUCKET_ORDER[bucket] ?? 4;
}

function relativeTime(iso: string): string {
  const then = Date.parse(iso);
  if (Number.isNaN(then)) return "";
  const diffMs = Date.now() - then;
  const min = Math.floor(diffMs / 60_000);
  if (min < 1) return "刚刚";
  if (min < 60) return `${min} 分钟前`;
  const hour = Math.floor(min / 60);
  if (hour < 24) return `${hour} 小时前`;
  return `${Math.floor(hour / 24)} 天前`;
}

export function mapWorkspace(w: WorkspaceSummary): WorkspaceItem {
  return {
    id: w.workspace_id,
    project: w.project_name,
    projectPath: w.display_path,
    branch: w.branch,
    isolation: w.isolation.kind === "worktree" ? "worktree" : "local",
    agent: w.agent_id,
    model: w.model_id,
    bucket: w.bucket,
    summary: w.summary,
    diffStat: w.diff_stat,
    label: w.label ?? undefined,
    archived: w.archived,
  };
}

export function mapQuickChat(q: QuickChatSummary): QuickChatItem {
  return {
    id: q.chat_id,
    title: q.title,
    agent: q.agent_id,
    model: q.model_id,
    bucket: q.bucket,
    updatedAt: relativeTime(q.updated_at),
    archived: q.archived,
    messages: [],
  };
}

export function mapApproval(a: ApprovalItemSnapshot): ApprovalItem {
  return {
    id: a.approval_id,
    workspaceId: sessionWorkspaceId(a),
    branch: a.branch_label,
    agent: a.agent_id,
    toolKind: a.tool_kind,
    command: a.command_or_target,
    riskNote: a.risk_note,
    highRisk: a.high_risk,
    diffPreview: a.diff_preview,
    resolution:
      a.audit?.resolution === "approve_once"
        ? "approved"
        : a.audit?.resolution === "allow_session"
          ? "session"
          : a.audit?.resolution === "reject_with_guidance"
            ? "rejected"
            : undefined,
    note: a.audit?.guidance_note ?? undefined,
  };
}

function sessionWorkspaceId(a: ApprovalItemSnapshot): string {
  return a.target.kind === "workspace" ? a.target.workspace_id : "";
}

/** Daemon 待确认提问卡 → 前端结构化提问状态（单卡展示，多题时取第一题）。 */
export function mapPendingInput(req: PendingUserInputRequest | null): AskQuestionState {
  if (!req) {
    return {
      id: "",
      prompt: "当前会话暂无待确认的结构化提问",
      options: [],
      selectedId: "",
      customNote: "",
      submitted: false,
    };
  }
  const q = req.questions[0];
  return {
    id: req.request_id,
    prompt: req.questions.length > 1 ? `${req.title}（共 ${req.questions.length} 题，先答第一题）` : (q?.prompt ?? req.title),
    options: q?.options.map((o) => ({ id: o.option_id, label: o.label, recommended: o.recommended })) ?? [],
    selectedId: "",
    customNote: "",
    submitted: false,
  };
}

/** 快照 → 全套视图初始状态。 */
export function mapSnapshot(snap: StateSnapshot): {
  workspaces: WorkspaceItem[];
  quickChats: QuickChatItem[];
  approvals: ApprovalItem[];
  askQuestion: AskQuestionState;
} {
  const workspaces = [...snap.workspaces]
    .sort((a, b) => bucketRank(a.bucket) - bucketRank(b.bucket))
    .map(mapWorkspace);
  return {
    workspaces,
    quickChats: snap.quick_chats.map(mapQuickChat),
    approvals: snap.pending_approvals.map(mapApproval),
    askQuestion: mapPendingInput(snap.pending_inputs[0] ?? null),
  };
}

export function mapTurnMessage(m: TurnMessageSnapshot): TurnMessage {
  return {
    id: m.message_id,
    role: m.role === "assistant" ? "assistant" : "user",
    author: m.author,
    text: m.text,
    badge: m.steer_badge ?? undefined,
    reasoningSteps: m.reasoning_steps.length > 0 ? m.reasoning_steps : undefined,
    reasoningSec: m.reasoning_duration_sec !== null ? `${m.reasoning_duration_sec.toFixed(1)} 秒` : undefined,
  };
}

/** 会话消息聚合：把 upsert 事件合并进会话的回合列表。 */
export function mergeTurnMessage(
  turns: TurnMessage[],
  message: TurnMessageSnapshot,
): TurnMessage[] {
  const mapped = mapTurnMessage(message);
  const idx = turns.findIndex((t) => t.id === mapped.id);
  if (idx >= 0) {
    const next = [...turns];
    next[idx] = mapped;
    return next;
  }
  return [...turns, mapped];
}

export interface ViewPatch {
  workspaces?: WorkspaceItem[];
  quickChats?: QuickChatItem[];
  approvals?: ApprovalItem[];
  askQuestion?: AskQuestionState;
  /** 已提交答案的提问卡 request_id（供调用方把对应卡片标记为 submitted）。 */
  askQuestionSubmitted?: string;
  /** 新消息归属的会话 id（供调用方定位当前会话是否受影响）。 */
  sessionMessage?: { session_id: string; message: TurnMessage };
}

/** 把单条事件增量应用到视图补丁；未知事件返回空补丁。 */
export function applyEventToPatch(env: EventEnvelope, patch: ViewPatch): ViewPatch {
  const payload: DaemonEvent = env.payload;
  switch (payload.event) {
    case "workspace_updated": {
      const list = (patch.workspaces ?? []).filter((w) => w.id !== payload.data.workspace_id);
      list.unshift(mapWorkspace(payload.data));
      return { ...patch, workspaces: list };
    }
    case "quick_chat_updated": {
      const list = (patch.quickChats ?? []).filter((q) => q.id !== payload.data.chat_id);
      list.unshift(mapQuickChat(payload.data));
      return { ...patch, quickChats: list };
    }
    case "approval_requested": {
      const list = (patch.approvals ?? []).filter((a) => a.id !== payload.data.approval_id);
      list.push(mapApproval(payload.data));
      return { ...patch, approvals: list };
    }
    case "approval_resolved": {
      const list = (patch.approvals ?? []).map((a) =>
        a.id === payload.data.approval_id
          ? {
              ...a,
              resolution:
                payload.data.audit.resolution === "approve_once"
                  ? ("approved" as const)
                  : payload.data.audit.resolution === "allow_session"
                    ? ("session" as const)
                    : ("rejected" as const),
              note: payload.data.audit.guidance_note ?? undefined,
            }
          : a,
      );
      return { ...patch, approvals: list };
    }
    case "user_input_requested":
      return { ...patch, askQuestion: mapPendingInput(payload.data) };
    case "user_input_submitted":
      // 提交后保持本地已答状态（真实 selectedId 由发起提交的本地交互已知）
      return { ...patch, askQuestionSubmitted: payload.data.request_id };
    case "turn_message_upserted":
      return {
        ...patch,
        sessionMessage: {
          session_id: payload.data.session_id,
          message: mapTurnMessage(payload.data),
        },
      };
    default:
      return patch;
  }
}

/** 会话消息聚合：把已映射的回合消息按 id upsert 进回合列表。 */
export function upsertMappedTurn(turns: TurnMessage[], message: TurnMessage): TurnMessage[] {
  const idx = turns.findIndex((t) => t.id === message.id);
  if (idx >= 0) {
    const next = [...turns];
    next[idx] = message;
    return next;
  }
  return [...turns, message];
}

/** 从快照里挑默认会话：优先绑定工作区、按注意力排序的第一个活跃会话。 */
export function pickDefaultSession(sessions: SessionSummary[]): SessionSummary | null {
  const active = sessions
    .filter((s) => !s.archived)
    .sort((a, b) => bucketRank(a.bucket) - bucketRank(b.bucket));
  return active[0] ?? null;
}

/** 从快照里按目标定位会话（工作区 id 或快速对话 id）。 */
export function findSessionForTarget(
  sessions: SessionSummary[],
  target: { kind: string; id: string },
): SessionSummary | null {
  return (
    sessions.find((s) => {
      if (s.archived) return false;
      if (s.target.kind === "workspace") return s.target.workspace_id === target.id && target.kind === "workspace";
      if (s.target.kind === "quick_chat") return s.target.chat_id === target.id && target.kind === "quick";
      return false;
    }) ?? null
  );
}
