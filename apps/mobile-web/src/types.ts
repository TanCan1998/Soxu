import type { DiffRowItem } from "@/components/CodeBlockView";

export type TurnBehavior = "queue" | "steer" | "interrupt";
export type MobileTab = "session" | "workspaces" | "approvals" | "scripts" | "diff" | "files" | "terminal" | "inbox" | "settings";
export type SizeClass = "compact" | "regular" | "wide";
export type WorkspaceBucket = "needs_input" | "failed" | "running" | "attention" | "done";
export type PermissionLevel = "plan" | "read-only" | "ask" | "auto-edit" | "full-access" | "turbo";
export type ComposerChipKind = "file" | "folder" | "diff" | "terminal" | "image" | "skill" | "handoff" | "quote" | "review";

export interface HandoffSelection {
  targetAgent: string;
  targetModel: string;
  targetThinking?: string;
}

export interface ComposerChip {
  id: string;
  kind: ComposerChipKind;
  label: string;
  targetAgent?: string;
  targetModel?: string;
  targetThinking?: string;
  detail?: string;
}

export interface QueuedPromptItem {
  id: string;
  text: string;
  createdAt: string;
  mode?: TurnBehavior;
}

export interface DiffFileItem {
  path: string;
  added: number;
  deleted: number;
  status: "M" | "A" | "D";
  staged?: boolean;
  rows: DiffRowItem[];
}

export interface ArtifactItem {
  id: string;
  title: string;
  kind: "plan" | "review" | "report";
  path: string;
  summary: string;
  status: "pending_approval" | "approved" | "draft";
  content: string[];
}

export interface LiveStepItem {
  id: string;
  title: string;
  status: "done" | "in_progress" | "pending";
}

export interface SessionTelemetryStats {
  turns: number;
  steps: number;
  totalTokens: number;
  apiCostUsd: number;
  avgSpeed: number;
  peakSpeed: number;
  recentSpeed: number;
  cacheHitPct: number;
  cacheNote: string;
  modelDurationSec: number;
  toolDurationSec: number;
  ttftSec: number;
}

export type SheetKind =
  | null
  | "search"
  | "pair"
  | "cli"
  | "model"
  | "thinking"
  | "perm"
  | "handoff"
  | "context"
  | "stats"
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
  activeIntent?: string;
  diffStat: string;
  label?: string;
  pinned?: boolean;
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
  intent?: string;
  metric?: string;
  running?: boolean;
  diffAdd?: number;
  diffDel?: number;
  pseudoLines?: string[];
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
  chips?: ComposerChip[];
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
