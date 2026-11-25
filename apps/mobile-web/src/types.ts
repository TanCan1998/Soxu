export type TurnBehavior = "queue" | "steer" | "interrupt";
export type MobileTab = "session" | "workspaces" | "approvals" | "scripts";
export type SizeClass = "compact" | "regular" | "wide";
export type WorkspaceBucket = "needs_input" | "failed" | "running" | "attention" | "done";
export type PermissionLevel = "plan" | "read-only" | "ask" | "auto-edit" | "full-access" | "turbo";

export type SheetKind =
  | null
  | "search"
  | "pair"
  | "cli"
  | "model"
  | "thinking"
  | "perm"
  | "handoff"
  | "rewind"
  | "history"
  | "new-worktree"
  | "new-quick-chat"
  | { kind: "script-log"; name: string };

export interface WorkspaceItem {
  id: string;
  project: string;
  projectPath: string;
  branch: string;
  isolation: "local" | "worktree";
  agent: string;
  model: string;
  bucket: WorkspaceBucket;
  summary: string;
  diffStat: string;
  label?: string;
  archived?: boolean;
}

export interface QuickChatItem {
  id: string;
  title: string;
  agent: string;
  model: string;
  bucket: WorkspaceBucket;
  updatedAt: string;
  archived?: boolean;
  messages: TurnMessage[];
}

export interface ToolCardItem {
  id: string;
  kind: "read" | "edit" | "bash" | "mcp";
  title: string;
  status: string;
  lines: string[];
}

export interface ApprovalItem {
  id: string;
  workspaceId: string;
  branch: string;
  agent: string;
  toolKind: string;
  command: string;
  riskNote: string;
  highRisk: boolean;
  diffPreview: string[];
  resolution?: "approved" | "session" | "rejected";
  note?: string;
}

export interface AskQuestionState {
  id: string;
  prompt: string;
  options: { id: string; label: string; recommended?: boolean }[];
  selectedId: string;
  customNote: string;
  submitted: boolean;
}

export interface TurnMessage {
  id: string;
  role: "user" | "assistant";
  author: string;
  text: string;
  badge?: string;
  reasoningSec?: string;
  reasoningSteps?: string[];
  tools?: ToolCardItem[];
  checkpoint?: { id: string; sha: string; stat: string };
}

export interface CliOption {
  id: string;
  bin: string;
  ver: string;
  available: boolean;
}

export interface ModelOption {
  id: string;
  tier: string;
  note: string;
  thinkingOptions: string[];
}

export interface ScriptItem {
  name: string;
  port?: string;
  running: boolean;
  lastExit: number;
  cmd: string;
  logs: string[];
}

export interface ScheduleItem {
  id: string;
  name: string;
  kind: "Schedule" | "Heartbeat";
  cron: string;
  human: string;
  target: string;
  active: boolean;
  unreadFail: boolean;
}

export interface NativeSessionHistory {
  id: string;
  cli: string;
  title: string;
  project: string;
  turns: number;
  updatedAt: string;
}
