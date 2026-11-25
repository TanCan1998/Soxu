import {
  Archive,
  ArchiveRestore,
  ArrowUp,
  Bolt,
  Check,
  ChevronDown,
  ChevronRight,
  Clock,
  Cpu,
  FileCode2,
  Folder,
  FolderOpen,
  GitBranch,
  History,
  Layers,
  MessageSquare,
  PanelLeft,
  PanelRight,
  Plus,
  QrCode,
  RotateCcw,
  Search,
  Settings,
  Shield,
  SlidersHorizontal,
  Sparkles,
  Square,
  Terminal,
  X,
} from "lucide-react";
import { StrictMode, useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { AgentIcon } from "@/components/AgentIcon";
import { CodeDiffView, CodeSnippetView, type DiffRowItem } from "@/components/CodeBlockView";
import { ComposerChipBar, ComposerHandoffMenu, ComposerPlusMenu } from "@/components/ComposerActionMenus";
import { ContextUsageRing } from "@/components/ContextUsageRing";
import { HoverExpandDock } from "@/components/HoverExpandDock";
import { SessionStatsPopover } from "@/components/SessionStatsPopover";
import { SettingsDialog, type SettingsSectionId } from "@/components/SettingsDialog";
import { ThemeToggle } from "@/components/ThemeToggle";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Sheet, SheetContent } from "@/components/ui/sheet";
import { TooltipProvider } from "@/components/ui/tooltip";
import {
  THINKING_LABELS,
  getAgentModels,
  getAgentProfile,
  getPermissionMeta,
  reconcileAgentCapabilities,
} from "@/lib/agent-capabilities";
import { I18nProvider, useI18n } from "@/lib/i18n";
import {
  applyPreferencesToDom,
  loadPreferences,
  savePreferences,
  toggleThemeWithTransition,
  type SoxuPreferences,
} from "@/lib/preferences";
import { cn } from "@/lib/utils";
import "./index.css";
import {
  CLI_OPTIONS,
  INITIAL_APPROVALS,
  INITIAL_ASK,
  INITIAL_QUICK_CHATS,
  INITIAL_SCHEDULES,
  INITIAL_SCRIPTS,
  INITIAL_TURNS,
  INITIAL_WORKSPACES,
  NATIVE_HISTORY,
} from "./mock-data";
import type {
  ApprovalItem,
  AskQuestionState,
  ComposerChip,
  MobileTab,
  PermissionLevel,
  QuickChatItem,
  ScheduleItem,
  ScriptItem,
  SessionTelemetryStats,
  SheetKind,
  SizeClass,
  ToolCardItem,
  TurnBehavior,
  TurnMessage,
  WorkspaceBucket,
  WorkspaceItem,
} from "./types";

function resolveSizeClass(width: number): SizeClass {
  if (width >= 1024) return "wide";
  if (width >= 640) return "regular";
  return "compact";
}

function toolIcon(kind: ToolCardItem["kind"]) {
  switch (kind) {
    case "read":
      return <Search className="h-3.5 w-3.5" strokeWidth={1.75} />;
    case "edit":
      return <FileCode2 className="h-3.5 w-3.5" strokeWidth={1.75} />;
    case "bash":
      return <Terminal className="h-3.5 w-3.5" strokeWidth={1.75} />;
    case "mcp":
      return <Cpu className="h-3.5 w-3.5" strokeWidth={1.75} />;
  }
}

function bucketMeta(bucket: WorkspaceBucket): {
  label: string;
  dotClass: string;
  variant: "warning" | "danger" | "running" | "handoff" | "neutral";
} {
  switch (bucket) {
    case "needs_input":
      return { label: "需输入", dotClass: "bg-[var(--soxu-semantic-warning)]", variant: "warning" };
    case "failed":
      return { label: "失败", dotClass: "bg-[var(--soxu-semantic-danger)]", variant: "danger" };
    case "running":
      return { label: "运行中", dotClass: "bg-[var(--soxu-semantic-running)] soxu-live-dot", variant: "running" };
    case "attention":
      return { label: "新完成", dotClass: "bg-[var(--soxu-semantic-accent)]", variant: "handoff" };
    case "done":
      return { label: "空闲", dotClass: "bg-[var(--soxu-text-tertiary)]", variant: "neutral" };
  }
}

function parseLinesToDiffRows(lines: string[]) {
  let num = 22;
  return lines.map((line) => {
    if (line.startsWith("+")) {
      return { lineNum: num++, kind: "add" as const, code: line };
    }
    if (line.startsWith("-")) {
      return { kind: "del" as const, code: line };
    }
    return { lineNum: num++, kind: "ctx" as const, code: line };
  });
}

const INSPECTOR_DIFF_ROWS: DiffRowItem[] = [
  { lineNum: 22, kind: "ctx", code: "pub enum NativeCliKind { Claude, Codex, Pi, OpenCode, Antigravity, Copilot, Qwen }" },
  { kind: "del", code: "// 旧版仅在运行时返回可执行文件名" },
  { lineNum: 23, kind: "add", code: "/// 返回驱动是否支持活跃回合中途热纠偏" },
  { lineNum: 24, kind: "add", code: "#[inline]" },
  { lineNum: 25, kind: "add", code: "pub const fn supports_live_steer(self) -> bool {" },
  { lineNum: 26, kind: "add", code: "  matches!(self, Self::Pi | Self::Codex | Self::Claude)" },
  { lineNum: 27, kind: "add", code: "}" },
];

function MobileApp() {
  const [prefs, setPrefs] = useState<SoxuPreferences>(loadPreferences);
  return (
    <I18nProvider locale={prefs.locale}>
      <MobileAppInner prefs={prefs} setPrefs={setPrefs} />
    </I18nProvider>
  );
}

function MobileAppInner({
  prefs,
  setPrefs,
}: {
  prefs: SoxuPreferences;
  setPrefs: React.Dispatch<React.SetStateAction<SoxuPreferences>>;
}) {
  const { t } = useI18n();
  const [resolvedTheme, setResolvedTheme] = useState<"dark" | "light">(() => applyPreferencesToDom(prefs));

  const [sizeClass, setSizeClass] = useState<SizeClass>(() =>
    typeof window !== "undefined" ? resolveSizeClass(window.innerWidth) : "compact",
  );
  const [activeTab, setActiveTab] = useState<MobileTab>("session");
  const [workspaces, setWorkspaces] = useState<WorkspaceItem[]>(INITIAL_WORKSPACES);
  const [quickChats, setQuickChats] = useState<QuickChatItem[]>(INITIAL_QUICK_CHATS);
  const [targetKind, setTargetKind] = useState<"workspace" | "quick">("workspace");
  const [selectedWorkspaceId, setSelectedWorkspaceId] = useState<string>("wks-m2");
  const [selectedQuickChatId, setSelectedQuickChatId] = useState<string>("chat-arch");
  const [bucketFilter, setBucketFilter] = useState<"all" | WorkspaceBucket>("all");
  const [openProjects, setOpenProjects] = useState<Record<string, boolean>>({ Soxu: true, Magpie: true });
  const [archivedOpen, setArchivedOpen] = useState(false);

  const [approvals, setApprovals] = useState<ApprovalItem[]>(INITIAL_APPROVALS);
  const [approvalSubTab, setApprovalSubTab] = useState<"pending" | "history">("pending");
  const [askQuestion, setAskQuestion] = useState<AskQuestionState>(INITIAL_ASK);
  const [turns, setTurns] = useState<TurnMessage[]>(INITIAL_TURNS);
  const [scripts, setScripts] = useState<ScriptItem[]>(INITIAL_SCRIPTS);
  const [schedules, setSchedules] = useState<ScheduleItem[]>(INITIAL_SCHEDULES);
  const [magpieMode, setMagpieMode] = useState<"magpie_managed" | "standalone_native">("magpie_managed");
  const [daemonOnline, setDaemonOnline] = useState<boolean>(false);
  const [seqCursor, setSeqCursor] = useState<number>(1056);
  const [toastMsg, setToastMsg] = useState<string | null>(null);
  const [lostControlNote, setLostControlNote] = useState<string | null>(null);

  const [cli, setCli] = useState<string>("codex");
  const [model, setModel] = useState<string>("group/gpt-6.2-sol");
  const [thinking, setThinking] = useState<string>("medium");
  const [permLevel, setPermLevel] = useState<PermissionLevel>(prefs.defaultPermMode);
  const [behavior, setBehavior] = useState<TurnBehavior>("steer");
  const [queuedPrompt, setQueuedPrompt] = useState<string | null>(null);
  const [draft, setDraft] = useState<string>("");
  const [composerChips, setComposerChips] = useState<ComposerChip[]>([
    { id: "mob-init-chip", kind: "file", label: "crates/soxu-drivers/src/lib.rs:22-27" },
  ]);
  const [contextTokens, setContextTokens] = useState<number>(172800);
  const [sessionStats, setSessionStats] = useState<SessionTelemetryStats>({
    turns: 24,
    steps: 1439,
    totalTokens: 104800000,
    apiCostUsd: 31.49,
    avgSpeed: 77.2,
    peakSpeed: 313.9,
    recentSpeed: 94.4,
    cacheHitPct: 91.9,
    cacheNote: "待确认",
    modelDurationSec: 182 * 60 + 13,
    toolDurationSec: 143 * 60 + 34,
    ttftSec: 11,
  });

  const [openReasons, setOpenReasons] = useState<Record<string, boolean>>({});
  const [openTools, setOpenTools] = useState<Record<string, boolean>>({ "tl-edit": true });
  const [diffExpanded, setDiffExpanded] = useState<Record<string, boolean>>({});
  const [rejectingId, setRejectingId] = useState<string | null>(null);
  const [rejectReason, setRejectReason] = useState<string>("");
  const [highRiskUnlocked, setHighRiskUnlocked] = useState<Record<string, boolean>>({});

  const [sheet, setSheet] = useState<SheetKind>(null);
  const [leftDrawerOpen, setLeftDrawerOpen] = useState(false);
  const [rightDrawerOpen, setRightDrawerOpen] = useState(false);
  const [rightInspectorTab, setRightInspectorTab] = useState<"diff" | "scripts">("diff");
  const [searchQuery, setSearchQuery] = useState("");
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [settingsSection, setSettingsSection] = useState<SettingsSectionId>("appearance");
  const [pairCode, setPairCode] = useState<string>("");
  const [newBranchName, setNewBranchName] = useState<string>("feat/19-mobile-remote");
  const [newQuickTitle, setNewQuickTitle] = useState<string>("");

  useEffect(() => {
    const resolved = applyPreferencesToDom(prefs);
    setResolvedTheme(resolved);
    savePreferences(prefs);
  }, [prefs]);

  useEffect(() => {
    const onResize = () => {
      const next = resolveSizeClass(window.innerWidth);
      setSizeClass(next);
      if (next === "wide") {
        setLeftDrawerOpen(false);
      }
    };
    const onKeyDown = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === ",") {
        e.preventDefault();
        setSettingsSection("appearance");
        setSettingsOpen((v) => !v);
      }
    };
    window.addEventListener("resize", onResize);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("resize", onResize);
      window.removeEventListener("keydown", onKeyDown);
    };
  }, []);

  function flashToast(msg: string) {
    setToastMsg(msg);
    window.setTimeout(() => {
      setToastMsg((cur) => (cur === msg ? null : cur));
    }, 2800);
  }

  function updatePrefs(patch: Partial<SoxuPreferences>, originEl?: HTMLElement | null) {
    const nextPrefs: SoxuPreferences = { ...prefs, ...patch };
    if (patch.themeMode && patch.themeMode !== prefs.themeMode && originEl) {
      toggleThemeWithTransition(originEl, prefs.reduceMotion, () => {
        setResolvedTheme(applyPreferencesToDom(nextPrefs));
        setPrefs(nextPrefs);
      });
      return;
    }
    setPrefs(nextPrefs);
  }

  function handleToggleTheme(originEl: HTMLElement | null) {
    const nextMode = resolvedTheme === "dark" ? "light" : "dark";
    const nextPrefs: SoxuPreferences = { ...prefs, themeMode: nextMode };
    toggleThemeWithTransition(originEl, prefs.reduceMotion, () => {
      setResolvedTheme(applyPreferencesToDom(nextPrefs));
      setPrefs(nextPrefs);
    });
  }

  const activeWorkspace = workspaces.find((w) => w.id === selectedWorkspaceId) ?? workspaces[0];
  const activeQuickChat = quickChats.find((q) => q.id === selectedQuickChatId) ?? quickChats[0];
  const pendingApprovals = approvals.filter((a) => !a.resolution);
  const resolvedApprovals = approvals.filter((a) => Boolean(a.resolution));
  const isRunning = targetKind === "workspace" && activeWorkspace.bucket === "running";

  const activeWorkspaces = workspaces.filter((w) => !w.archived);
  const activeQuickChats = quickChats.filter((q) => !q.archived);
  const filteredWorkspaces =
    bucketFilter === "all" ? activeWorkspaces : activeWorkspaces.filter((w) => w.bucket === bucketFilter);

  const projectGroups = Array.from(
    filteredWorkspaces.reduce((map, w) => {
      const entry = map.get(w.project) ?? { path: w.projectPath, items: [] as WorkspaceItem[] };
      entry.items.push(w);
      map.set(w.project, entry);
      return map;
    }, new Map<string, { path: string; items: WorkspaceItem[] }>()),
  );

  const archivedEntries = [
    ...quickChats.filter((q) => q.archived).map((q) => ({ kind: "quick" as const, id: q.id, title: q.title, sub: q.updatedAt })),
    ...workspaces.filter((w) => w.archived).map((w) => ({ kind: "workspace" as const, id: w.id, title: `${w.project} / ${w.branch}`, sub: w.diffStat })),
  ];

  const activeProfile = getAgentProfile(cli);
  const availableModels = getAgentModels(cli);
  const currentModelMeta = availableModels.find((m) => m.id === model) ?? availableModels[0];
  const activePermMeta = getPermissionMeta(cli, permLevel);

  function simulateSeqCatchup() {
    setSeqCursor((prev) => {
      const next = prev + 17;
      flashToast(`事件流增量追平完成：游标 #${prev} 至 #${next}`);
      return next;
    });
  }

  function handleSelectCli(nextCliId: string) {
    const nextCli = CLI_OPTIONS.find((c) => c.id === nextCliId);
    if (!nextCli || !nextCli.available) return;
    setCli(nextCli.id);
    const reconciled = reconcileAgentCapabilities({
      agentId: nextCli.id,
      currentModel: model,
      currentPerm: permLevel,
      currentThinking: thinking,
      currentBehavior: behavior,
    });
    setModel(reconciled.model);
    setPermLevel(reconciled.perm);
    setThinking(reconciled.thinking);
    setBehavior(reconciled.behavior);
    setLostControlNote(reconciled.notes.length > 0 ? reconciled.notes.join("；") : null);
    setSheet(null);
    flashToast(`活跃驱动已切换至 ${nextCli.id} v${nextCli.ver}`);
  }

  function handleSelectModel(nextModelId: string) {
    setModel(nextModelId);
    const reconciled = reconcileAgentCapabilities({
      agentId: cli,
      currentModel: nextModelId,
      currentPerm: permLevel,
      currentThinking: thinking,
      currentBehavior: behavior,
    });
    setThinking(reconciled.thinking);
    setLostControlNote(reconciled.notes.length > 0 ? reconciled.notes.join("；") : null);
    setSheet(null);
    flashToast(`模型已切换为 ${nextModelId}`);
  }

  function handleResolveApproval(id: string, resolution: "approved" | "session" | "rejected", note?: string) {
    setApprovals((prev) => prev.map((a) => (a.id === id ? { ...a, resolution, note } : a)));
    setRejectingId(null);
    setRejectReason("");
    setHighRiskUnlocked((prev) => ({ ...prev, [id]: false }));
    setSeqCursor((s) => s + 1);
    flashToast(
      resolution === "rejected"
        ? "已拒绝高危动作并将修改指导回传至代理"
        : resolution === "session"
          ? "已批准并允许本会话同类安全命令自动放行"
          : "已授权单次执行",
    );
  }

  function addComposerChip(draftChip: Omit<ComposerChip, "id">) {
    setComposerChips((prev) => {
      if (draftChip.kind === "handoff") {
        const rest = prev.filter((c) => c.kind !== "handoff");
        return [{ ...draftChip, id: `mob-handoff-${Date.now()}` }, ...rest];
      }
      if (prev.some((c) => c.kind === draftChip.kind && c.label === draftChip.label)) {
        return prev;
      }
      return [...prev, { ...draftChip, id: `mob-chip-${Date.now()}-${Math.random().toString(36).slice(2, 6)}` }];
    });
  }

  function removeComposerChip(chipId: string) {
    setComposerChips((prev) => prev.filter((c) => c.id !== chipId));
  }

  function handleSubmitPrompt() {
    const rawText = draft.trim();
    const attachedChips = composerChips;
    if (!rawText && attachedChips.length === 0) return;

    const handoffChip = attachedChips.find((c) => c.kind === "handoff" && c.targetAgent);
    const effectiveCli = handoffChip?.targetAgent ?? cli;
    if (handoffChip?.targetAgent && handoffChip.targetAgent !== cli) {
      handleSelectCli(handoffChip.targetAgent);
    }

    const text = rawText || `请处理已附加的 ${attachedChips.length} 项上下文标签`;

    setSessionStats((prev) => ({
      ...prev,
      turns: prev.turns + 1,
      steps: prev.steps + 4,
      totalTokens: prev.totalTokens + Math.max(2400, text.length * 60),
      apiCostUsd: Number((prev.apiCostUsd + 0.08).toFixed(2)),
      recentSpeed: Number((88 + (text.length % 15)).toFixed(1)),
      modelDurationSec: prev.modelDurationSec + 14,
      toolDurationSec: prev.toolDurationSec + 6,
    }));

    if (targetKind === "quick" && activeQuickChat) {
      const nextSeq = seqCursor + 2;
      setSeqCursor(nextSeq);
      const uMsg: TurnMessage = {
        id: `qu-${nextSeq}`,
        role: "user",
        author: "你",
        text,
        chips: attachedChips.length > 0 ? attachedChips : undefined,
      };
      const aMsg: TurnMessage = {
        id: `qa-${nextSeq + 1}`,
        role: "assistant",
        author: `${effectiveCli} · ${model}`,
        badge: handoffChip ? `@${effectiveCli} 已接管` : undefined,
        text: handoffChip
          ? `已在无项目快速对话中移交至 @${effectiveCli}（${activePermMeta.label}）并继续响应。`
          : `已在无项目快速对话下响应（${activePermMeta.label}）：收到你的指令「${text}」。`,
      };
      setQuickChats((prev) =>
        prev.map((q) =>
          q.id === activeQuickChat.id ? { ...q, updatedAt: "刚刚", messages: [...q.messages, uMsg, aMsg] } : q,
        ),
      );
      setContextTokens((cur) => cur + Math.max(1200, text.length * 40));
      setComposerChips([]);
      setDraft("");
      return;
    }

    if (isRunning && !handoffChip) {
      if (behavior === "queue") {
        setQueuedPrompt(text);
        setComposerChips([]);
        setDraft("");
        flashToast("已加入指令队列，当前回合结束时顺延发送");
        return;
      }
      if (behavior === "interrupt") {
        setWorkspaces((prev) => prev.map((w) => (w.id === activeWorkspace.id ? { ...w, bucket: "done" } : w)));
      }
    }

    const nextSeq = seqCursor + 2;
    setSeqCursor(nextSeq);
    const userTurn: TurnMessage = {
      id: `turn-u-${nextSeq}`,
      role: "user",
      author: "你",
      text,
      chips: attachedChips.length > 0 ? attachedChips : undefined,
      badge: handoffChip ? `移交 -> @${effectiveCli}` : undefined,
    };
    const replyTurn: TurnMessage = {
      id: `turn-a-${nextSeq + 1}`,
      role: "assistant",
      author: `${effectiveCli} · ${model}`,
      badge: handoffChip ? `@${effectiveCli} 接管` : undefined,
      reasoningSec: "1.1 秒",
      reasoningSteps: [
        "1. 读取工作区检查点确保修改严格落在目标范围。",
        "2. 在沙箱隔离边界内验证测试结果，绝不修改全局外部配置。",
      ],
      text: handoffChip
        ? `已接收跨代理移交上下文与 ${attachedChips.length} 项标签，切换至 @${effectiveCli}（${activePermMeta.label}）继续执行。`
        : behavior === "steer"
          ? `已在不中断当前进程前提下热纠偏：收到指令「${text}」，正在应用至活跃上下文。`
          : `已按「${activePermMeta.label}」策略接收新指令，正在执行检验。`,
    };
    setTurns((prev) => [...prev, userTurn, replyTurn]);
    setContextTokens((cur) => cur + Math.max(1800, text.length * 45));
    setComposerChips([]);
    setDraft("");
  }

  function handleAnswerAsk(optionId: string) {
    const picked = askQuestion.options.find((o) => o.id === optionId);
    if (!picked) return;
    setAskQuestion((prev) => ({ ...prev, selectedId: optionId, submitted: true }));
    setSeqCursor((s) => s + 1);
    flashToast(`已回传确认结果：「${picked.label}」`);
  }

  function handleHandoff(targetCli: string) {
    setSheet(null);
    handleSelectCli(targetCli);
    const nextSeq = seqCursor + 2;
    setSeqCursor(nextSeq);
    setTurns((prev) => [
      ...prev,
      {
        id: `turn-handoff-${nextSeq}`,
        role: "assistant",
        author: `${targetCli} · 上下文移交`,
        badge: `@${targetCli} 已接管`,
        text: `已将最近 3 轮摘要、未提交变更文件列表与游标 #${nextSeq} 移交至 @${targetCli} 继续处理。`,
      },
    ]);
    flashToast(`上下文已移交至 @${targetCli}`);
  }

  function handleApplyRewind(label: string) {
    setSheet(null);
    setSeqCursor((s) => s + 1);
    flashToast(`已回退至检查点 ${label}（未提交改动已自动保存安全暂存备份）`);
  }

  function handleCreateWorktree() {
    const raw = newBranchName.trim();
    if (!raw) return;
    const slug = raw
      .toLowerCase()
      .replace(/[^a-z0-9/_-]+/g, "-")
      .replace(/^-+|-+$/g, "");
    const id = `wks-${Date.now().toString().slice(-4)}`;
    const item: WorkspaceItem = {
      id,
      project: "Soxu",
      projectPath: "~/projects/Soxu",
      branch: slug || "feat/new-worktree",
      isolation: "worktree",
      agent: cli,
      model,
      bucket: "attention",
      summary: "新建隔离工作树，已注入目录上下文",
      diffStat: "+0 -0",
    };
    setWorkspaces((prev) => [item, ...prev]);
    setSelectedWorkspaceId(id);
    setTargetKind("workspace");
    setSheet(null);
    setActiveTab("session");
    flashToast(`已创建并切入工作树：${item.branch}`);
  }

  function handleCreateQuickChat() {
    const title = newQuickTitle.trim() || "新建无项目自由对话";
    const id = `chat-${Date.now().toString().slice(-4)}`;
    const qc: QuickChatItem = {
      id,
      title,
      agent: cli,
      model,
      bucket: "done",
      updatedAt: "刚刚",
      messages: [],
    };
    setQuickChats((prev) => [qc, ...prev]);
    setSelectedQuickChatId(id);
    setTargetKind("quick");
    setNewQuickTitle("");
    setSheet(null);
    setActiveTab("session");
    flashToast(`已打开快速对话：${title}`);
  }

  function archiveItem(kind: "workspace" | "quick", id: string, title: string) {
    if (kind === "quick") {
      setQuickChats((prev) => prev.map((q) => (q.id === id ? { ...q, archived: true } : q)));
    } else {
      setWorkspaces((prev) => prev.map((w) => (w.id === id ? { ...w, archived: true } : w)));
    }
    flashToast(`已归档对话：${title}`);
  }

  function restoreItem(kind: "workspace" | "quick", id: string) {
    if (kind === "quick") {
      setQuickChats((prev) => prev.map((q) => (q.id === id ? { ...q, archived: false } : q)));
      setSelectedQuickChatId(id);
      setTargetKind("quick");
    } else {
      setWorkspaces((prev) => prev.map((w) => (w.id === id ? { ...w, archived: false } : w)));
      setSelectedWorkspaceId(id);
      setTargetKind("workspace");
    }
    setActiveTab("session");
    flashToast("已从归档恢复对话");
  }

  function renderApprovalCard(item: ApprovalItem) {
    if (item.resolution) {
      return (
        <div
          key={item.id}
          className="rounded-xl px-3.5 py-2.5 text-xs flex items-center justify-between gap-2 bg-[var(--soxu-surface-card-muted)] border border-[var(--soxu-border-subtle)] text-[var(--soxu-text-secondary)]"
        >
          <span className="flex items-center gap-2 min-w-0">
            <Check className="h-3.5 w-3.5 text-[var(--soxu-semantic-success)] shrink-0" strokeWidth={1.75} />
            <span className="truncate">
              {item.resolution === "approved"
                ? "已允许一次"
                : item.resolution === "session"
                  ? "本会话同类放行"
                  : `已拒绝：${item.note || "按建议修改"}`}
              {" · "}
              <code className="soxu-mono">{item.command}</code>
            </span>
          </span>
          <span className="soxu-tabular text-[var(--soxu-text-tertiary)] shrink-0">已处理</span>
        </div>
      );
    }

    const expanded = Boolean(diffExpanded[item.id]);
    const unlocked = Boolean(highRiskUnlocked[item.id]);

    return (
      <article
        key={item.id}
        className={cn(
          "rounded-2xl p-4 flex flex-col gap-3 bg-[var(--soxu-surface-card)] border",
          item.highRisk
            ? "border-[var(--soxu-semantic-danger-border)]"
            : "border-[var(--soxu-semantic-warning-border)]",
        )}
      >
        <div className="flex items-start justify-between gap-2">
          <div className="flex items-center gap-2 min-w-0">
            <Shield
              className={cn(
                "h-4 w-4 shrink-0",
                item.highRisk ? "text-[var(--soxu-semantic-danger)]" : "text-[var(--soxu-semantic-warning)]",
              )}
              strokeWidth={1.75}
            />
            <div className="min-w-0">
              <div className="text-sm font-semibold truncate">{item.toolKind}</div>
              <div className="flex items-center gap-1.5 text-xs soxu-mono text-[var(--soxu-text-tertiary)] truncate">
                <span className="truncate">{item.branch}</span>
                <span>·</span>
                <AgentIcon agent={item.agent} className="h-3.5 w-3.5 shrink-0" />
                <span className="truncate">{item.agent}</span>
              </div>
            </div>
          </div>
          <Badge variant={item.highRisk ? "danger" : "warning"}>{item.highRisk ? "高危拦截" : "等待审批"}</Badge>
        </div>

        <div className="rounded-xl p-3 soxu-code-block overflow-x-auto bg-[var(--soxu-surface-terminal)] border border-[var(--soxu-border-subtle)] text-[var(--soxu-text-code)]">
          $ {item.command}
        </div>

        <p
          className={cn(
            "text-xs m-0 leading-relaxed",
            item.highRisk ? "text-[var(--soxu-semantic-danger)]" : "text-[var(--soxu-text-secondary)]",
          )}
        >
          {item.riskNote}
        </p>

        {item.diffPreview.length > 0 && (
          <Collapsible
            open={expanded}
            onOpenChange={(open) => setDiffExpanded((p) => ({ ...p, [item.id]: open }))}
          >
            <CollapsibleTrigger asChild>
              <Button variant="secondary" size="sm" className="self-start">
                <ChevronRight
                  className={cn("h-3.5 w-3.5 transition-transform duration-150", expanded && "rotate-90")}
                  strokeWidth={1.75}
                />
                <span>{expanded ? "收起关联差异" : `查看关联差异 (${item.diffPreview.length} 行)`}</span>
              </Button>
            </CollapsibleTrigger>
            <CollapsibleContent className="soxu-collapsible overflow-hidden pt-2">
              <CodeDiffView
                rows={parseLinesToDiffRows(item.diffPreview)}
                layout="unified"
                showLineNumbers={prefs.showLineNumbers}
                wrap={prefs.codeWordWrap}
              />
            </CollapsibleContent>
          </Collapsible>
        )}

        {rejectingId === item.id ? (
          <div className="flex flex-col gap-2 pt-1">
            <label className="text-xs font-medium" htmlFor={`reject-${item.id}`}>
              附加拒绝原因与修改建议：
            </label>
            <input
              id={`reject-${item.id}`}
              type="text"
              value={rejectReason}
              onChange={(e) => setRejectReason(e.target.value)}
              placeholder="例如请改用临时目录并在测试后自动清理"
              className="w-full rounded-xl px-3 py-2.5 text-base bg-[var(--soxu-surface-composer)] border border-[var(--soxu-border-strong)] text-[var(--soxu-text-primary)] outline-none"
            />
            <div className="grid grid-cols-2 gap-2.5">
              <Button variant="secondary" size="lg" onClick={() => setRejectingId(null)}>
                取消
              </Button>
              <Button
                variant="destructive"
                size="lg"
                onClick={() => handleResolveApproval(item.id, "rejected", rejectReason)}
              >
                确认拒绝并回传
              </Button>
            </div>
          </div>
        ) : item.highRisk ? (
          <div className="flex flex-col gap-2">
            {!unlocked ? (
              <div className="grid grid-cols-2 gap-2.5">
                <Button
                  variant="destructive"
                  size="lg"
                  onClick={() => {
                    setRejectingId(item.id);
                    setRejectReason("");
                  }}
                >
                  拒绝高危命令
                </Button>
                <Button
                  variant="secondary"
                  size="lg"
                  className="border-[var(--soxu-semantic-danger-border)] text-[var(--soxu-semantic-danger)]"
                  onClick={() => setHighRiskUnlocked((prev) => ({ ...prev, [item.id]: true }))}
                >
                  解锁二次确认
                </Button>
              </div>
            ) : (
              <div className="flex flex-col gap-2">
                <div className="text-[11.5px] text-[var(--soxu-semantic-danger)] font-medium">
                  已解锁高危执行开关，请再次确认命令副作用：
                </div>
                <div className="grid grid-cols-2 gap-2.5">
                  <Button
                    variant="secondary"
                    size="lg"
                    onClick={() => setHighRiskUnlocked((prev) => ({ ...prev, [item.id]: false }))}
                  >
                    重新锁定
                  </Button>
                  <Button
                    variant="destructive"
                    size="lg"
                    onClick={() => handleResolveApproval(item.id, "approved")}
                  >
                    确认单次放行
                  </Button>
                </div>
              </div>
            )}
          </div>
        ) : (
          <div className="flex flex-col gap-2">
            <div className="grid grid-cols-2 gap-2.5">
              <Button
                variant="secondary"
                size="lg"
                onClick={() => {
                  setRejectingId(item.id);
                  setRejectReason("");
                }}
              >
                拒绝并指导
              </Button>
              <Button variant="default" size="lg" onClick={() => handleResolveApproval(item.id, "approved")}>
                允许一次
              </Button>
            </div>
            <Button variant="outline" size="sm" onClick={() => handleResolveApproval(item.id, "session")}>
              本会话同类命令自动放行
            </Button>
          </div>
        )}
      </article>
    );
  }

  const WorkspaceRail = (
    <aside
      className={cn(
        "flex flex-col gap-4 p-3.5 shrink-0",
        sizeClass === "wide"
          ? "w-[320px] border-r border-[var(--soxu-border-hairline)] bg-[var(--soxu-surface-sidebar)]"
          : "w-full",
      )}
    >
      {/* 1. 无项目快速对话区 */}
      <div className="flex flex-col gap-2">
        <div className="flex items-center justify-between gap-2">
          <span className="text-xs font-semibold text-[var(--soxu-text-secondary)]">快速对话 (无项目)</span>
          <Button
            variant="outline"
            size="sm"
            onClick={() => {
              setLeftDrawerOpen(false);
              setSheet("new-quick-chat");
            }}
          >
            <Plus className="h-3.5 w-3.5" strokeWidth={1.75} />
            <span>新对话</span>
          </Button>
        </div>
        <div className="flex flex-col gap-1.5">
          {activeQuickChats.map((qc) => {
            const active = targetKind === "quick" && selectedQuickChatId === qc.id;
            const meta = bucketMeta(qc.bucket);
            return (
              <div
                key={qc.id}
                onClick={() => {
                  setTargetKind("quick");
                  setSelectedQuickChatId(qc.id);
                  setLeftDrawerOpen(false);
                  if (sizeClass !== "wide") setActiveTab("session");
                }}
                role="button"
                tabIndex={0}
                onKeyDown={(e) => {
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    setTargetKind("quick");
                    setSelectedQuickChatId(qc.id);
                  }
                }}
                className={cn(
                  "rounded-xl px-3 py-2.5 flex items-center justify-between gap-2 border cursor-pointer transition-colors",
                  active
                    ? "bg-[var(--soxu-interaction-selected)] border-[var(--soxu-border-strong)]"
                    : "bg-[var(--soxu-surface-card)] border-[var(--soxu-border-subtle)]",
                )}
              >
                <div className="flex items-center gap-2 min-w-0">
                  <span className={cn("h-2 w-2 rounded-full shrink-0", meta.dotClass)} />
                  <AgentIcon agent={qc.agent} className="h-3.5 w-3.5 shrink-0" />
                  <span className="text-xs font-medium truncate">{qc.title}</span>
                </div>
                <button
                  type="button"
                  title="归档对话"
                  aria-label={`归档 ${qc.title}`}
                  onClick={(e) => {
                    e.stopPropagation();
                    archiveItem("quick", qc.id, qc.title);
                  }}
                  className="p-1 text-[var(--soxu-text-tertiary)] hover:text-[var(--soxu-text-primary)] cursor-pointer shrink-0"
                >
                  <Archive className="h-3.5 w-3.5" strokeWidth={1.75} />
                </button>
              </div>
            );
          })}
        </div>
      </div>

      {/* 2. 项目文件夹 -> 工作树层级 */}
      <div className="flex flex-col gap-2">
        <div className="flex items-center justify-between gap-2">
          <span className="text-xs font-semibold text-[var(--soxu-text-secondary)]">项目与工作树</span>
          <div className="flex items-center gap-1.5">
            <Button
              variant="outline"
              size="sm"
              onClick={() => {
                setLeftDrawerOpen(false);
                setSheet("history");
              }}
            >
              <History className="h-3.5 w-3.5" strokeWidth={1.75} />
              <span>历史</span>
            </Button>
            <Button
              variant="secondary"
              size="sm"
              onClick={() => {
                setLeftDrawerOpen(false);
                setSheet("new-worktree");
              }}
            >
              <Plus className="h-3.5 w-3.5" strokeWidth={1.75} />
              <span>工作树</span>
            </Button>
          </div>
        </div>

        <div className="flex items-center gap-1.5 overflow-x-auto soxu-no-scrollbar pb-0.5">
          {(
            [
              { id: "all", label: "全部" },
              { id: "needs_input", label: "需输入" },
              { id: "failed", label: "失败" },
              { id: "running", label: "运行中" },
              { id: "attention", label: "新完成" },
              { id: "done", label: "空闲" },
            ] as const
          ).map((b) => (
            <button
              key={b.id}
              type="button"
              onClick={() => setBucketFilter(b.id)}
              className={cn(
                "px-2.5 py-1 rounded-full text-xs font-medium whitespace-nowrap border cursor-pointer transition-colors",
                bucketFilter === b.id
                  ? "bg-[var(--soxu-action-inverse)] text-[var(--soxu-action-on-inverse)] border-transparent font-semibold"
                  : "bg-[var(--soxu-surface-card)] text-[var(--soxu-text-secondary)] border-[var(--soxu-border-subtle)]",
              )}
            >
              {b.label}
            </button>
          ))}
        </div>

        <div className="flex flex-col gap-2">
          {projectGroups.map(([projName, group]) => {
            const expanded = openProjects[projName] ?? true;
            return (
              <div key={projName} className="flex flex-col gap-1">
                <button
                  type="button"
                  onClick={() => setOpenProjects((p) => ({ ...p, [projName]: !expanded }))}
                  className="flex items-center justify-between px-1.5 py-1 text-xs font-semibold text-[var(--soxu-text-primary)] cursor-pointer"
                >
                  <span className="flex items-center gap-1.5 truncate">
                    <ChevronRight
                      className={cn("h-3.5 w-3.5 text-[var(--soxu-text-tertiary)] transition-transform", expanded && "rotate-90")}
                      strokeWidth={1.75}
                    />
                    {expanded ? <FolderOpen className="h-3.5 w-3.5" strokeWidth={1.75} /> : <Folder className="h-3.5 w-3.5" strokeWidth={1.75} />}
                    <span>{projName}</span>
                    <span className="text-[11px] font-normal soxu-mono text-[var(--soxu-text-tertiary)] truncate">
                      {prefs.redactHomePath ? group.path : group.path.replace("~", "/home/dev")}
                    </span>
                  </span>
                </button>

                {expanded && (
                  <div className="ml-3 pl-2.5 border-l border-[var(--soxu-border-hairline)] flex flex-col gap-1.5">
                    {group.items.map((w) => {
                      const active = targetKind === "workspace" && w.id === selectedWorkspaceId;
                      const meta = bucketMeta(w.bucket);
                      return (
                        <div
                          key={w.id}
                          role="button"
                          tabIndex={0}
                          onClick={() => {
                            setTargetKind("workspace");
                            setSelectedWorkspaceId(w.id);
                            setLeftDrawerOpen(false);
                            if (sizeClass !== "wide") setActiveTab("session");
                          }}
                          onKeyDown={(e) => {
                            if (e.key === "Enter" || e.key === " ") {
                              e.preventDefault();
                              setTargetKind("workspace");
                              setSelectedWorkspaceId(w.id);
                            }
                          }}
                          className={cn(
                            "w-full text-left rounded-xl p-2.5 flex flex-col gap-1 border cursor-pointer transition-colors",
                            active
                              ? "bg-[var(--soxu-interaction-selected)] border-[var(--soxu-border-strong)]"
                              : "bg-[var(--soxu-surface-card)] border-[var(--soxu-border-subtle)]",
                          )}
                        >
                          <div className="flex items-center justify-between gap-2">
                            <span className="flex items-center gap-1.5 min-w-0">
                              <span className={cn("h-2 w-2 rounded-full shrink-0", meta.dotClass)} />
                              <span className="soxu-mono text-xs font-semibold truncate">{w.branch}</span>
                            </span>
                            <div className="flex items-center gap-1 shrink-0">
                              {w.bucket !== "done" && <Badge variant={meta.variant}>{meta.label}</Badge>}
                              <button
                                type="button"
                                aria-label={`归档 ${w.branch}`}
                                onClick={(e) => {
                                  e.stopPropagation();
                                  archiveItem("workspace", w.id, `${w.project} / ${w.branch}`);
                                }}
                                className="p-1 text-[var(--soxu-text-tertiary)] hover:text-[var(--soxu-text-primary)] cursor-pointer"
                              >
                                <Archive className="h-3 w-3" strokeWidth={1.75} />
                              </button>
                            </div>
                          </div>
                          <div className="text-[11.5px] text-[var(--soxu-text-secondary)] truncate">{w.summary}</div>
                          <div className="flex items-center justify-between text-[11px] text-[var(--soxu-text-tertiary)] soxu-mono">
                            <span className="inline-flex items-center gap-1.5">
                              <span>{w.isolation === "worktree" ? "工作树" : "主目录"}</span>
                              <span>·</span>
                              <AgentIcon agent={w.agent} className="h-3 w-3 shrink-0" />
                              <span>{w.agent}</span>
                            </span>
                            <span className="soxu-tabular">{w.diffStat}</span>
                          </div>
                        </div>
                      );
                    })}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      </div>

      {/* 3. 已归档对话折叠区 */}
      <Collapsible open={archivedOpen} onOpenChange={setArchivedOpen}>
        <CollapsibleTrigger asChild>
          <button
            type="button"
            className="w-full px-1 py-1 flex items-center justify-between text-xs text-[var(--soxu-text-tertiary)] cursor-pointer"
          >
            <span className="flex items-center gap-1.5">
              <Archive className="h-3.5 w-3.5" strokeWidth={1.75} />
              <span>已归档对话 ({archivedEntries.length})</span>
            </span>
            <ChevronRight className={cn("h-3.5 w-3.5 transition-transform", archivedOpen && "rotate-90")} strokeWidth={1.75} />
          </button>
        </CollapsibleTrigger>
        <CollapsibleContent className="pt-1.5 flex flex-col gap-1.5">
          {archivedEntries.map((ae) => (
            <div
              key={ae.id}
              className="rounded-xl px-3 py-2 flex items-center justify-between gap-2 bg-[var(--soxu-surface-card-muted)] border border-[var(--soxu-border-hairline)] text-xs"
            >
              <span className="truncate text-[var(--soxu-text-secondary)]">{ae.title}</span>
              <button
                type="button"
                onClick={() => restoreItem(ae.kind, ae.id)}
                className="text-[11px] text-[var(--soxu-semantic-accent)] inline-flex items-center gap-1 shrink-0 cursor-pointer"
              >
                <ArchiveRestore className="h-3 w-3" strokeWidth={1.75} />
                <span>恢复</span>
              </button>
            </div>
          ))}
        </CollapsibleContent>
      </Collapsible>
    </aside>
  );

  const ScriptsAndSchedulesView = (
    <section className="flex flex-col gap-4">
      <div className="flex flex-col gap-2">
        <h2 className="text-xs font-semibold text-[var(--soxu-text-tertiary)] tracking-wider m-0">
          工作区受管脚本
        </h2>
        {scripts.map((scr) => (
          <div
            key={scr.name}
            className="rounded-2xl p-3.5 flex items-center justify-between gap-3 bg-[var(--soxu-surface-card)] border border-[var(--soxu-border-subtle)]"
          >
            <div className="min-w-0">
              <div className="flex items-center gap-2">
                <span className="text-sm font-semibold truncate">{scr.name}</span>
                {scr.port && <span className="soxu-mono text-[11px] text-[var(--soxu-text-tertiary)]">{scr.port}</span>}
                <Badge variant={scr.running ? "running" : "neutral"}>{scr.running ? "运行中" : "空闲"}</Badge>
              </div>
              <div className="text-xs soxu-mono text-[var(--soxu-text-tertiary)] truncate mt-0.5">{scr.cmd}</div>
            </div>
            <div className="flex items-center gap-1.5 shrink-0">
              <Button
                variant="outline"
                size="sm"
                onClick={() => {
                  setRightDrawerOpen(false);
                  setSheet({ kind: "script-log", name: scr.name });
                }}
              >
                日志
              </Button>
              <Button
                variant={scr.running ? "destructive" : "default"}
                size="sm"
                onClick={() => setScripts((prev) => prev.map((s) => (s.name === scr.name ? { ...s, running: !s.running } : s)))}
              >
                {scr.running ? "停止" : "启动"}
              </Button>
            </div>
          </div>
        ))}
      </div>

      <div className="flex flex-col gap-2">
        <h2 className="text-xs font-semibold text-[var(--soxu-text-tertiary)] tracking-wider m-0">
          定时任务与周期巡检
        </h2>
        {schedules.map((sch) => (
          <div
            key={sch.id}
            className="rounded-2xl p-3.5 flex items-center justify-between gap-3 bg-[var(--soxu-surface-card)] border border-[var(--soxu-border-subtle)]"
          >
            <div className="min-w-0">
              <div className="flex items-center gap-2">
                <Clock className="h-3.5 w-3.5 text-[var(--soxu-text-tertiary)] shrink-0" strokeWidth={1.75} />
                <span className="text-sm font-semibold truncate">{sch.name}</span>
                <Badge variant={sch.unreadFail ? "danger" : sch.active ? "running" : "neutral"}>
                  {sch.kind === "Heartbeat" ? "周期巡检" : "定时任务"}
                </Badge>
              </div>
              <div className="text-xs soxu-mono text-[var(--soxu-text-tertiary)] truncate mt-0.5">
                {sch.human} ({sch.cron}) · {sch.target}
              </div>
            </div>
            <Button
              variant={sch.active ? "secondary" : "outline"}
              size="sm"
              onClick={() => setSchedules((prev) => prev.map((x) => (x.id === sch.id ? { ...x, active: !x.active } : x)))}
            >
              {sch.active ? "暂停" : "启用"}
            </Button>
          </div>
        ))}
      </div>
    </section>
  );

  const activeScriptForLog =
    sheet !== null && typeof sheet === "object" && sheet.kind === "script-log"
      ? scripts.find((s) => s.name === sheet.name)
      : null;

  return (
    <TooltipProvider delayDuration={220}>
      <div className="min-h-dvh flex flex-col bg-[var(--soxu-surface-canvas)] text-[var(--soxu-text-primary)]">
        {/* 顶部极简毛玻璃栏：三轨分栏确保搜索框恒定居中、按钮不重叠；Soxu 前方绝无圆点 */}
        <header
          className="sticky top-0 soxu-safe-top px-2.5 sm:px-4 pb-2 backdrop-blur-md border-b border-[var(--soxu-border-hairline)] bg-[var(--soxu-surface-header-glass)]"
          style={{ zIndex: "var(--soxu-z-header)" }}
        >
          <div className="max-w-[1280px] mx-auto grid grid-cols-[auto_minmax(0,1fr)_auto] sm:grid-cols-[minmax(max-content,1fr)_minmax(140px,320px)_minmax(max-content,1fr)] items-center gap-1.5 sm:gap-2.5">
            <div className="flex items-center gap-1 sm:gap-2 min-w-0 justify-self-start">
              {sizeClass !== "wide" && (
                <Button
                  variant="ghost"
                  size="icon-sm"
                  onClick={() => setLeftDrawerOpen(true)}
                  aria-label={t("header.openLeftDrawer")}
                  className="shrink-0"
                >
                  <PanelLeft className="h-4 w-4" strokeWidth={1.75} />
                </Button>
              )}

              <span className="px-1 text-xs sm:text-[13px] font-semibold tracking-tight text-[var(--soxu-text-primary)] shrink-0">
                {t("brand.name")}
              </span>

              <button
                type="button"
                onClick={simulateSeqCatchup}
                className="hidden md:flex flex-col text-left min-w-0 bg-transparent border-0 p-0 cursor-pointer"
                title="点击模拟断线重连后游标增量追平"
              >
                <div className="text-[11px] soxu-mono soxu-tabular text-[var(--soxu-text-tertiary)] truncate">
                  {daemonOnline ? t("mobile.daemonOnline") : t("mobile.remoteClient")} · {t("mobile.cursorPrefix", { seq: seqCursor })}
                </div>
                <div className="text-[11.5px] soxu-mono text-[var(--soxu-text-secondary)] truncate max-w-[200px]">
                  {targetKind === "quick" ? `${t("header.quickChat")} / ${activeQuickChat.title}` : `${activeWorkspace.project} / ${activeWorkspace.branch}`}
                </div>
              </button>
            </div>

            {/* 始终水平居中且防重叠的快速检索触发条 */}
            <button
              type="button"
              onClick={() => setSheet("search")}
              className="h-8 w-full max-w-[320px] min-w-0 justify-self-center rounded-full border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card)] px-2.5 flex items-center gap-1.5 text-xs text-[var(--soxu-text-tertiary)] hover:border-[var(--soxu-border-strong)] hover:text-[var(--soxu-text-secondary)] transition-colors cursor-pointer"
              aria-label={t("header.searchCompact")}
            >
              <Search className="h-3.5 w-3.5 shrink-0" strokeWidth={1.75} />
              <span className="truncate">{t("header.searchCompact")}</span>
            </button>

            <div className="flex items-center gap-0.5 sm:gap-1 shrink-0 justify-self-end">
              <Button
                variant="ghost"
                size="icon-sm"
                onClick={() => setSheet("pair")}
                aria-label={t("header.pair")}
                title={t("header.pair")}
                className="rounded-full"
              >
                <QrCode className="h-4 w-4" strokeWidth={1.75} />
              </Button>

              <HoverExpandDock
                triggerIcon={<SlidersHorizontal className="h-4 w-4" strokeWidth={1.75} />}
                triggerLabel={t("header.moreTools")}
                expandDirection="left"
              >
                <Button variant="ghost" size="icon-sm" onClick={() => setSheet("history")} title={t("sidebar.importHistory")}>
                  <History className="h-3.5 w-3.5" strokeWidth={1.75} />
                </Button>
                <Button variant="ghost" size="icon-sm" onClick={() => setSheet("new-worktree")} title={t("sidebar.worktree")}>
                  <GitBranch className="h-3.5 w-3.5" strokeWidth={1.75} />
                </Button>
                <Button variant="ghost" size="icon-sm" onClick={() => setSheet("new-quick-chat")} title={t("sidebar.newQuickChat")}>
                  <MessageSquare className="h-3.5 w-3.5" strokeWidth={1.75} />
                </Button>
              </HoverExpandDock>

              <ThemeToggle resolvedTheme={resolvedTheme} onToggle={handleToggleTheme} />

              <Button
                variant="ghost"
                size="icon-sm"
                onClick={() => {
                  setSettingsSection("appearance");
                  setSettingsOpen(true);
                }}
                aria-label={t("header.settings")}
                title={`${t("header.settings")} (Ctrl+,)`}
                className="rounded-full"
              >
                <Settings className="h-4 w-4" strokeWidth={1.75} />
              </Button>

              <Button
                variant={rightDrawerOpen ? "secondary" : "ghost"}
                size="icon-sm"
                onClick={() => setRightDrawerOpen(true)}
                aria-label={t("header.openRightDrawer")}
                title={t("header.openRightDrawer")}
                className="rounded-full"
              >
                <PanelRight className="h-4 w-4" strokeWidth={1.75} />
              </Button>
            </div>
          </div>

          {toastMsg && (
            <div className="max-w-[760px] mx-auto mt-2 px-3 py-1.5 rounded-xl text-xs flex items-center gap-1.5 bg-[var(--soxu-semantic-accent-soft)] border border-[var(--soxu-border-subtle)] text-[var(--soxu-semantic-accent)]">
              <Bolt className="h-3.5 w-3.5 shrink-0" strokeWidth={1.75} />
              <span className="truncate">{toastMsg}</span>
            </div>
          )}
        </header>

        {/* 主工作区 */}
        <div className="flex-1 flex max-w-[1280px] w-full mx-auto">
          {sizeClass === "wide" && WorkspaceRail}

          <main className="flex-1 flex flex-col justify-between min-w-0">
            <div
              className="w-full mx-auto px-4 py-4 flex flex-col gap-4"
              style={{ maxWidth: "var(--soxu-layout-transcript-max-width)" }}
            >
              {pendingApprovals.length > 0 && activeTab !== "approvals" && (
                <button
                  type="button"
                  onClick={() => setActiveTab("approvals")}
                  className="w-full rounded-xl px-3.5 py-2.5 text-left flex items-center justify-between gap-2 bg-[var(--soxu-semantic-warning-soft)] border border-[var(--soxu-semantic-warning-border)] text-[var(--soxu-semantic-warning)] cursor-pointer"
                >
                  <span className="text-xs font-medium flex items-center gap-2 min-w-0">
                    <Shield className="h-4 w-4 shrink-0" strokeWidth={1.75} />
                    <span className="truncate">有 {pendingApprovals.length} 项高风险或工具操作等待远程批准</span>
                  </span>
                  <span className="text-xs font-semibold shrink-0">去处理</span>
                </button>
              )}

              {sizeClass !== "wide" && activeTab === "workspaces" && WorkspaceRail}

              {activeTab === "approvals" && (
                <section className="flex flex-col gap-3">
                  <div className="flex items-center gap-2">
                    <Button
                      variant={approvalSubTab === "pending" ? "default" : "secondary"}
                      size="sm"
                      onClick={() => setApprovalSubTab("pending")}
                    >
                      待人工确认 ({pendingApprovals.length})
                    </Button>
                    <Button
                      variant={approvalSubTab === "history" ? "default" : "secondary"}
                      size="sm"
                      onClick={() => setApprovalSubTab("history")}
                    >
                      已处理记录 ({resolvedApprovals.length})
                    </Button>
                  </div>
                  {(approvalSubTab === "pending" ? pendingApprovals : resolvedApprovals).length === 0 ? (
                    <div className="rounded-2xl p-6 text-center text-xs text-[var(--soxu-text-tertiary)] bg-[var(--soxu-surface-card)] border border-[var(--soxu-border-subtle)]">
                      当前分类暂无审批记录
                    </div>
                  ) : (
                    (approvalSubTab === "pending" ? pendingApprovals : resolvedApprovals).map((a) => renderApprovalCard(a))
                  )}
                </section>
              )}

              {activeTab === "scripts" && ScriptsAndSchedulesView}

              {(activeTab === "session" || (sizeClass === "wide" && activeTab === "workspaces")) && (
                <>
                  {targetKind === "quick" && activeQuickChat ? (
                    <div className="flex flex-col gap-4">
                      <div className="rounded-xl px-3.5 py-2.5 text-xs bg-[var(--soxu-surface-card-muted)] border border-[var(--soxu-border-subtle)] text-[var(--soxu-text-secondary)] flex items-center justify-between">
                        <span>无项目自由对话 · {activeQuickChat.title}</span>
                        <Badge variant="neutral">轻量模式</Badge>
                      </div>
                      {activeQuickChat.messages.length === 0 ? (
                        <div className="py-10 text-center text-xs text-[var(--soxu-text-tertiary)]">
                          在下方输入内容开始无项目自由对话
                        </div>
                      ) : (
                        activeQuickChat.messages.map((m) =>
                          m.role === "user" ? (
                            <div key={m.id} className="flex flex-col items-end gap-1.5">
                              {m.chips && m.chips.length > 0 && <ComposerChipBar chips={m.chips} className="justify-end" />}
                              <div className="max-w-[85%] rounded-[18px] px-4 py-2.5 text-sm leading-relaxed bg-[var(--soxu-surface-user-bubble)] text-[var(--soxu-text-primary)]">
                                {m.text}
                              </div>
                            </div>
                          ) : (
                            <article key={m.id} className="flex flex-col gap-2">
                              <div className="flex items-center gap-1.5 text-xs font-semibold text-[var(--soxu-text-secondary)]">
                                <AgentIcon agent={m.author} className="h-3.5 w-3.5 shrink-0" />
                                <span>{m.author}</span>
                                {m.badge && <Badge variant="handoff">{m.badge}</Badge>}
                              </div>
                              <p className="text-sm m-0 leading-relaxed text-[var(--soxu-text-primary)]">{m.text}</p>
                            </article>
                          ),
                        )
                      )}
                    </div>
                  ) : (
                    <>
                      {/* 会话信息微条 */}
                      <div className="flex items-center justify-between gap-2 text-xs text-[var(--soxu-text-tertiary)] pb-1 border-b border-[var(--soxu-border-hairline)]">
                        <span className="truncate">
                          {activeWorkspace.project} · {activeWorkspace.summary}
                        </span>
                        <span className="soxu-mono soxu-tabular shrink-0">{activeWorkspace.diffStat}</span>
                      </div>

                      {/* 对话回合流 */}
                      <div className="flex flex-col gap-4">
                        {turns.map((turn) => {
                          if (turn.role === "user") {
                            return (
                              <div key={turn.id} className="flex flex-col items-end gap-1.5">
                                {turn.badge && <Badge variant="handoff">{turn.badge}</Badge>}
                                {turn.chips && turn.chips.length > 0 && (
                                  <ComposerChipBar chips={turn.chips} className="justify-end" />
                                )}
                                <div className="max-w-[85%] rounded-[18px] px-4 py-2.5 text-sm leading-relaxed bg-[var(--soxu-surface-user-bubble)] text-[var(--soxu-text-primary)]">
                                  {turn.text}
                                </div>
                              </div>
                            );
                          }

                          const reasonOpen = Boolean(openReasons[turn.id]);
                          return (
                            <article key={turn.id} className="flex flex-col gap-2.5">
                              <div className="flex items-center gap-2 text-xs text-[var(--soxu-text-tertiary)]">
                                <AgentIcon agent={turn.author} className="h-4 w-4 shrink-0" />
                                <span className="font-semibold text-[var(--soxu-text-secondary)]">{turn.author}</span>
                                {turn.badge && <Badge variant="handoff">{turn.badge}</Badge>}
                              </div>

                              {turn.reasoningSteps && turn.reasoningSteps.length > 0 && (
                                <Collapsible
                                  open={reasonOpen}
                                  onOpenChange={(o) => setOpenReasons((p) => ({ ...p, [turn.id]: o }))}
                                  className="rounded-xl bg-[var(--soxu-surface-card-muted)] border border-[var(--soxu-border-hairline)]"
                                >
                                  <CollapsibleTrigger asChild>
                                    <button
                                      type="button"
                                      className="w-full px-3 py-2 flex items-center justify-between gap-2 text-xs text-[var(--soxu-text-secondary)] bg-transparent border-0 cursor-pointer"
                                    >
                                      <span className="flex items-center gap-1.5 truncate">
                                        <Sparkles className="h-3.5 w-3.5 text-[var(--soxu-semantic-handoff)] shrink-0" strokeWidth={1.75} />
                                        <span className="truncate">思考过程 · {turn.reasoningSec ?? "已完成"}</span>
                                      </span>
                                      <ChevronRight
                                        className={cn("h-3.5 w-3.5 shrink-0 transition-transform duration-150", reasonOpen && "rotate-90")}
                                        strokeWidth={1.75}
                                      />
                                    </button>
                                  </CollapsibleTrigger>
                                  <CollapsibleContent className="soxu-collapsible overflow-hidden">
                                    <div className="px-3 pb-2.5 text-xs text-[var(--soxu-text-tertiary)] leading-relaxed flex flex-col gap-1">
                                      {turn.reasoningSteps.map((step) => (
                                        <div key={step}>{step}</div>
                                      ))}
                                    </div>
                                  </CollapsibleContent>
                                </Collapsible>
                              )}

                              <p className="text-sm m-0 leading-relaxed text-[var(--soxu-text-primary)]">{turn.text}</p>

                              {turn.tools?.map((tl) => {
                                const isOpen = Boolean(openTools[tl.id]);
                                return (
                                  <Collapsible
                                    key={tl.id}
                                    open={isOpen}
                                    onOpenChange={(o) => setOpenTools((p) => ({ ...p, [tl.id]: o }))}
                                    className="rounded-xl overflow-hidden bg-[var(--soxu-surface-card)] border border-[var(--soxu-border-subtle)]"
                                  >
                                    <CollapsibleTrigger asChild>
                                      <button
                                        type="button"
                                        className="w-full px-3 py-2 flex items-center justify-between gap-2 text-xs bg-transparent border-0 cursor-pointer"
                                      >
                                        <span className="flex items-center gap-2 min-w-0 text-[var(--soxu-text-secondary)]">
                                          {toolIcon(tl.kind)}
                                          <span className="soxu-mono truncate">{tl.title}</span>
                                        </span>
                                        <span className="flex items-center gap-2 shrink-0 text-[var(--soxu-text-tertiary)]">
                                          <span>{tl.status}</span>
                                          <ChevronRight
                                            className={cn("h-3.5 w-3.5 transition-transform duration-150", isOpen && "rotate-90")}
                                            strokeWidth={1.75}
                                          />
                                        </span>
                                      </button>
                                    </CollapsibleTrigger>
                                    <CollapsibleContent className="soxu-collapsible overflow-hidden p-2 border-t border-[var(--soxu-border-hairline)]">
                                      {tl.kind === "edit" ? (
                                        <CodeDiffView
                                          rows={parseLinesToDiffRows(tl.lines)}
                                          layout="unified"
                                          showLineNumbers={prefs.showLineNumbers}
                                          wrap={prefs.codeWordWrap}
                                        />
                                      ) : (
                                        <CodeSnippetView
                                          lines={tl.lines}
                                          startLine={1}
                                          showLineNumbers={prefs.showLineNumbers}
                                          wrap={prefs.codeWordWrap}
                                        />
                                      )}
                                    </CollapsibleContent>
                                  </Collapsible>
                                );
                              })}

                              {turn.checkpoint && (
                                <div className="rounded-xl px-3 py-2 flex items-center justify-between gap-2 text-xs bg-[var(--soxu-surface-card-muted)] border border-[var(--soxu-border-subtle)]">
                                  <span className="flex items-center gap-2 min-w-0 text-[var(--soxu-text-secondary)]">
                                    <RotateCcw className="h-3.5 w-3.5 shrink-0" strokeWidth={1.75} />
                                    <span className="truncate">
                                      检查点 {turn.checkpoint.id} ({turn.checkpoint.sha}) · {turn.checkpoint.stat}
                                    </span>
                                  </span>
                                  <Button variant="outline" size="sm" onClick={() => setSheet("rewind")}>
                                    安全回退
                                  </Button>
                                </div>
                              )}
                            </article>
                          );
                        })}

                        {/* 结构化确认提问卡 */}
                        <div className="rounded-2xl p-4 flex flex-col gap-3 bg-[var(--soxu-surface-card)] border border-[var(--soxu-border-strong)]">
                          <div className="flex items-center justify-between gap-2">
                            <Badge variant="accent">结构化确认提问</Badge>
                            <span className="text-[11px] text-[var(--soxu-text-tertiary)]">
                              {askQuestion.submitted ? "已回传决策" : "等待选择确认"}
                            </span>
                          </div>
                          <div className="text-sm font-medium leading-relaxed">{askQuestion.prompt}</div>
                          <div className="flex flex-col gap-2">
                            {askQuestion.options.map((opt) => {
                              const picked = askQuestion.selectedId === opt.id;
                              return (
                                <button
                                  key={opt.id}
                                  type="button"
                                  onClick={() => handleAnswerAsk(opt.id)}
                                  className={cn(
                                    "w-full rounded-xl px-3.5 py-2.5 text-left text-xs flex items-center justify-between gap-2 border cursor-pointer transition-colors",
                                    picked
                                      ? "bg-[var(--soxu-interaction-selected)] border-[var(--soxu-border-strong)] text-[var(--soxu-text-primary)] font-semibold"
                                      : "bg-[var(--soxu-surface-card-muted)] border-[var(--soxu-border-subtle)] text-[var(--soxu-text-secondary)]",
                                  )}
                                >
                                  <span className="flex items-center gap-2 min-w-0">
                                    <span className="truncate">{opt.label}</span>
                                    {opt.recommended && <Badge variant="accent">推荐</Badge>}
                                  </span>
                                  {picked && <Check className="h-3.5 w-3.5 text-[var(--soxu-semantic-accent)] shrink-0" strokeWidth={2} />}
                                </button>
                              );
                            })}
                          </div>
                        </div>

                        {pendingApprovals.map((item) => renderApprovalCard(item))}
                      </div>
                    </>
                  )}
                </>
              )}
            </div>

            {/* 底部悬浮输入舱：外层同为 px-4 与相同 maxWidth，与上方内容宽度完全一致 */}
            {activeTab === "session" && (
              <div className="sticky bottom-0 px-4 pb-2.5 pt-2 bg-gradient-to-t from-[var(--soxu-surface-canvas)] via-[var(--soxu-surface-canvas)] to-transparent">
                <div
                  className="w-full mx-auto rounded-[22px] p-2.5 border border-[var(--soxu-border-strong)] bg-[var(--soxu-surface-composer)] flex flex-col gap-2"
                  style={{ maxWidth: "var(--soxu-layout-transcript-max-width)" }}
                >
                  {lostControlNote && (
                    <div className="px-2.5 py-1 rounded-lg text-[11px] bg-[var(--soxu-semantic-warning-soft)] text-[var(--soxu-semantic-warning)]">
                      {lostControlNote}
                    </div>
                  )}

                  {queuedPrompt && (
                    <div className="px-2.5 py-1 rounded-lg text-[11px] flex items-center justify-between gap-2 bg-[var(--soxu-surface-card)] text-[var(--soxu-text-secondary)]">
                      <span className="truncate">排队中：{queuedPrompt}</span>
                      <button
                        type="button"
                        className="text-[11px] text-[var(--soxu-semantic-danger)] bg-transparent border-0 cursor-pointer"
                        onClick={() => setQueuedPrompt(null)}
                      >
                        撤回
                      </button>
                    </div>
                  )}

                  <ComposerChipBar chips={composerChips} onRemove={removeComposerChip} />

                  <textarea
                    rows={2}
                    value={draft}
                    onChange={(e) => setDraft(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter" && !e.shiftKey) {
                        e.preventDefault();
                        handleSubmitPrompt();
                      }
                    }}
                    placeholder={
                      targetKind === "quick"
                        ? "输入问题随时开展无项目对话..."
                        : isRunning
                          ? `运行中（${behavior === "steer" ? "热纠偏" : behavior === "queue" ? "排队" : "打断重发"}）...`
                          : "输入指令、@代理 移交上下文，或粘贴任务要求..."
                    }
                    className="w-full resize-none bg-transparent border-0 outline-none px-1.5 py-1 text-base text-[var(--soxu-text-primary)] placeholder:text-[var(--soxu-text-ghost)]"
                  />

                  <div className="flex flex-wrap items-center justify-between gap-1.5">
                    {/* 操作权限与代理/模型完全常驻不折叠，左侧保留 + 更多操作及可选代理移交 */}
                    <div className="flex flex-wrap items-center gap-1 min-w-0">
                      <ComposerPlusMenu
                        hasWorkspace={targetKind === "workspace"}
                        onAddChip={addComposerChip}
                      />

                      <Button variant="ghost" size="sm" onClick={() => setSheet("cli")} className="h-7 px-2 text-[11.5px] soxu-mono">
                        <AgentIcon agent={cli} className="h-3.5 w-3.5 shrink-0" />
                        <span>{cli}</span>
                        <ChevronDown className="h-3 w-3" strokeWidth={1.75} />
                      </Button>

                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => setSheet("model")}
                        className="h-7 px-2 text-[11.5px] soxu-mono truncate max-w-[120px]"
                      >
                        {model.replace("group/", "").replace("auto/", "")}
                      </Button>

                      {activeProfile.supportsThinking && (
                        <Button variant="ghost" size="sm" onClick={() => setSheet("thinking")} className="h-7 px-2 text-[11.5px]">
                          <span>{THINKING_LABELS[thinking] ?? thinking}</span>
                        </Button>
                      )}

                      {/* 常驻操作权限切换按钮（绝不折叠） */}
                      <Button
                        variant="outline"
                        size="sm"
                        onClick={() => setSheet("perm")}
                        className="h-7 px-2 text-[11.5px] rounded-full"
                      >
                        <Shield className="h-3 w-3 text-[var(--soxu-semantic-accent)]" strokeWidth={1.75} />
                        <span>{activePermMeta.label}</span>
                      </Button>

                      <ComposerHandoffMenu
                        currentCli={cli}
                        selectedTargetAgent={composerChips.find((c) => c.kind === "handoff")?.targetAgent}
                        onSelectTarget={(targetCli) => {
                          addComposerChip({
                            kind: "handoff",
                            label: `@${targetCli}`,
                            targetAgent: targetCli,
                          });
                          flashToast(`已选定移交目标代理 @${targetCli}`);
                        }}
                      />
                    </div>

                    <div className="flex items-center gap-1 shrink-0 ml-auto">
                      <ContextUsageRing
                        model={model}
                        usedTokens={contextTokens}
                        onCompact={() => {
                          setContextTokens((c) => Math.round(c * 0.42));
                          flashToast("已完成上下文摘要压缩");
                        }}
                      />

                      <SessionStatsPopover stats={sessionStats} />

                      {isRunning && (
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={() => {
                            const order: TurnBehavior[] = activeProfile.supportsLiveSteer
                              ? ["steer", "queue", "interrupt"]
                              : ["queue", "interrupt"];
                            const cur = order.includes(behavior) ? order.indexOf(behavior) : 0;
                            setBehavior(order[(cur + 1) % order.length]);
                          }}
                          className="h-7 px-2 text-[11px]"
                        >
                          {behavior === "steer" ? "热纠偏" : behavior === "queue" ? "排队" : "打断"}
                        </Button>
                      )}
                      {isRunning && !draft.trim() && composerChips.length === 0 ? (
                        <Button
                          variant="destructive"
                          size="icon"
                          onClick={() => {
                            setWorkspaces((prev) =>
                              prev.map((w) => (w.id === activeWorkspace.id ? { ...w, bucket: "done" } : w)),
                            );
                            flashToast("已发送停止信号");
                          }}
                          aria-label="停止生成"
                        >
                          <Square className="h-3.5 w-3.5 fill-current" strokeWidth={1.75} />
                        </Button>
                      ) : (
                        <Button
                          variant="default"
                          size="icon"
                          disabled={!draft.trim() && composerChips.length === 0}
                          onClick={handleSubmitPrompt}
                          aria-label="发送指令"
                        >
                          <ArrowUp className="h-4 w-4" strokeWidth={2} />
                        </Button>
                      )}
                    </div>
                  </div>
                </div>
              </div>
            )}
          </main>
        </div>

        {/* 移动端底部四页签导航 */}
        {sizeClass !== "wide" && (
          <nav
            aria-label="主导航"
            className="sticky bottom-0 soxu-safe-bottom border-t border-[var(--soxu-border-hairline)] bg-[var(--soxu-surface-header-glass)] backdrop-blur-md grid grid-cols-4"
            style={{ zIndex: "var(--soxu-z-header)" }}
          >
            {(
              [
                { id: "session", label: t("mobile.navSession"), icon: MessageSquare },
                { id: "workspaces", label: t("mobile.navWorkspaces"), icon: Folder },
                { id: "approvals", label: t("mobile.navApprovals"), icon: Shield, count: pendingApprovals.length },
                { id: "scripts", label: t("mobile.navScripts"), icon: Layers },
              ] as const
            ).map((nav) => {
              const Icon = nav.icon;
              const active = activeTab === nav.id;
              return (
                <button
                  key={nav.id}
                  type="button"
                  onClick={() => setActiveTab(nav.id)}
                  className={cn(
                    "py-2 flex flex-col items-center justify-center gap-0.5 text-[11px] bg-transparent border-0 cursor-pointer relative transition-colors",
                    active ? "text-[var(--soxu-text-primary)] font-semibold" : "text-[var(--soxu-text-tertiary)]",
                  )}
                >
                  <div className="relative">
                    <Icon className="h-4 w-4" strokeWidth={1.75} />
                    {"count" in nav && nav.count > 0 && (
                      <span className="absolute -top-1 -right-2 min-w-[14px] h-[14px] px-1 rounded-full text-[9.5px] leading-[14px] text-center bg-[var(--soxu-semantic-warning)] text-black font-bold">
                        {nav.count}
                      </span>
                    )}
                  </div>
                  <span>{nav.label}</span>
                </button>
              );
            })}
          </nav>
        )}

        {/* 窄屏左侧工作区与快速对话抽屉 */}
        <Sheet open={sizeClass !== "wide" && leftDrawerOpen} onOpenChange={setLeftDrawerOpen}>
          <SheetContent side="left" rawBody srTitle={t("sidebar.title")}>
            <div className="px-3.5 py-2.5 border-b border-[var(--soxu-border-hairline)] flex items-center justify-between gap-2 shrink-0 bg-[var(--soxu-surface-sidebar)]">
              <span className="text-xs font-semibold truncate">{t("sidebar.title")}</span>
              <Button
                variant="ghost"
                size="icon-sm"
                onClick={() => setLeftDrawerOpen(false)}
                aria-label={t("header.toggleLeft")}
              >
                <X className="h-4 w-4" strokeWidth={1.75} />
              </Button>
            </div>
            <div className="flex-1 overflow-y-auto soxu-scroll bg-[var(--soxu-surface-sidebar)]">
              {WorkspaceRail}
            </div>
          </SheetContent>
        </Sheet>

        {/* 右侧变更对比与受管脚本抽屉 */}
        <Sheet open={rightDrawerOpen} onOpenChange={setRightDrawerOpen}>
          <SheetContent side="right" rawBody srTitle={t("inspector.title")}>
            <div className="px-3.5 py-2.5 border-b border-[var(--soxu-border-hairline)] flex items-center justify-between gap-2 shrink-0 bg-[var(--soxu-surface-sidebar)]">
              <div className="inline-flex rounded-lg bg-[var(--soxu-surface-inset)] p-0.5 border border-[var(--soxu-border-hairline)]">
                {(
                  [
                    { id: "diff", label: t("inspector.diff") },
                    { id: "scripts", label: t("inspector.scripts") },
                  ] as const
                ).map((item) => (
                  <button
                    key={item.id}
                    type="button"
                    onClick={() => setRightInspectorTab(item.id)}
                    className={cn(
                      "px-2.5 py-1 rounded-md text-xs cursor-pointer transition-colors",
                      rightInspectorTab === item.id
                        ? "bg-[var(--soxu-surface-raised)] text-[var(--soxu-text-primary)] font-medium"
                        : "text-[var(--soxu-text-tertiary)]",
                    )}
                  >
                    {item.label}
                  </button>
                ))}
              </div>
              <Button
                variant="ghost"
                size="icon-sm"
                onClick={() => setRightDrawerOpen(false)}
                aria-label={t("header.toggleRight")}
              >
                <X className="h-4 w-4" strokeWidth={1.75} />
              </Button>
            </div>
            <div className="flex-1 overflow-y-auto p-3.5 flex flex-col gap-3 soxu-scroll bg-[var(--soxu-surface-sidebar)]">
              {rightInspectorTab === "diff" ? (
                <>
                  <div className="text-[11px] soxu-mono text-[var(--soxu-text-tertiary)] truncate">
                    crates/soxu-drivers/src/lib.rs · {t("inspector.baseBranchLabel", { branch: prefs.defaultBranch })}
                  </div>
                  <CodeDiffView
                    rows={INSPECTOR_DIFF_ROWS}
                    layout="unified"
                    showLineNumbers={prefs.showLineNumbers}
                    wrap={prefs.codeWordWrap}
                  />
                </>
              ) : (
                ScriptsAndSchedulesView
              )}
            </div>
          </SheetContent>
        </Sheet>

        {/* 底部抽屉集合 */}
        <Sheet open={sheet !== null} onOpenChange={(o) => !o && setSheet(null)}>
          {sheet === "search" && (
            <SheetContent title={t("header.searchCompact")} subtitle="快速定位工作树、自由对话或跳转设置" tall>
              <input
                autoFocus
                type="text"
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                placeholder={t("header.searchPlaceholder")}
                className="w-full rounded-xl px-3 py-2.5 text-base bg-[var(--soxu-surface-composer)] border border-[var(--soxu-border-strong)] text-[var(--soxu-text-primary)] outline-none"
              />
              <div className="flex flex-col gap-1.5">
                {[
                  ...activeWorkspaces.map((w) => ({
                    key: w.id,
                    title: `${w.project} / ${w.branch}`,
                    badge: w.agent,
                    run: () => {
                      setTargetKind("workspace");
                      setSelectedWorkspaceId(w.id);
                      setActiveTab("session");
                      setSheet(null);
                    },
                  })),
                  ...activeQuickChats.map((q) => ({
                    key: q.id,
                    title: q.title,
                    badge: t("header.quickChat"),
                    run: () => {
                      setTargetKind("quick");
                      setSelectedQuickChatId(q.id);
                      setActiveTab("session");
                      setSheet(null);
                    },
                  })),
                ]
                  .filter((item) => !searchQuery.trim() || item.title.toLowerCase().includes(searchQuery.toLowerCase()))
                  .map((item) => (
                    <button
                      key={item.key}
                      type="button"
                      onClick={item.run}
                      className="w-full rounded-xl px-3 py-2.5 text-left text-xs flex items-center justify-between gap-2 bg-[var(--soxu-surface-card-muted)] border border-[var(--soxu-border-subtle)] cursor-pointer hover:border-[var(--soxu-border-strong)]"
                    >
                      <span className="flex items-center gap-2 min-w-0">
                        <AgentIcon agent={item.badge === t("header.quickChat") ? cli : item.badge} className="h-3.5 w-3.5 shrink-0" />
                        <span className="truncate font-medium">{item.title}</span>
                      </span>
                      <Badge variant="neutral">{item.badge}</Badge>
                    </button>
                  ))}
              </div>
            </SheetContent>
          )}
          {sheet === "cli" && (
            <SheetContent title="切换原生代理驱动" subtitle="复用本机已登录认证与本地会话">
              {CLI_OPTIONS.map((c) => (
                <button
                  key={c.id}
                  type="button"
                  disabled={!c.available}
                  onClick={() => handleSelectCli(c.id)}
                  className={cn(
                    "w-full rounded-xl p-3 text-left flex items-center justify-between gap-2 border transition-colors",
                    c.available ? "cursor-pointer" : "opacity-50 cursor-not-allowed",
                    cli === c.id
                      ? "bg-[var(--soxu-interaction-selected)] border-[var(--soxu-border-strong)]"
                      : "bg-[var(--soxu-surface-card-muted)] border-[var(--soxu-border-subtle)]",
                  )}
                >
                  <div className="flex items-center gap-2.5">
                    <AgentIcon agent={c.id} className="h-5 w-5 shrink-0" />
                    <div>
                      <div className="text-sm font-semibold">{c.id}</div>
                      <div className="text-xs soxu-mono text-[var(--soxu-text-tertiary)]">
                        {c.bin} · v{c.ver}
                      </div>
                    </div>
                  </div>
                  {cli === c.id ? (
                    <Check className="h-4 w-4 text-[var(--soxu-semantic-accent)]" strokeWidth={2} />
                  ) : (
                    <Badge variant={c.available ? "running" : "neutral"}>{c.available ? "已就绪" : "未安装"}</Badge>
                  )}
                </button>
              ))}
            </SheetContent>
          )}

          {sheet === "model" && (
            <SheetContent title={`选择 ${activeProfile.displayName} 支持的模型`} subtitle="按能力范围自动过滤可选模型">
              {availableModels.map((m) => (
                <button
                  key={m.id}
                  type="button"
                  onClick={() => handleSelectModel(m.id)}
                  className={cn(
                    "w-full rounded-xl p-3 text-left flex items-center justify-between gap-2 border cursor-pointer",
                    model === m.id
                      ? "bg-[var(--soxu-interaction-selected)] border-[var(--soxu-border-strong)]"
                      : "bg-[var(--soxu-surface-card-muted)] border-[var(--soxu-border-subtle)]",
                  )}
                >
                  <div className="min-w-0">
                    <div className="text-sm font-semibold soxu-mono truncate">{m.id}</div>
                    <div className="text-xs text-[var(--soxu-text-tertiary)] truncate">
                      {m.tier} · {m.note}
                    </div>
                  </div>
                  {model === m.id && <Check className="h-4 w-4 text-[var(--soxu-semantic-accent)] shrink-0" strokeWidth={2} />}
                </button>
              ))}
            </SheetContent>
          )}

          {sheet === "thinking" && (
            <SheetContent title="选择思考强度" subtitle={`当前模型：${model}`}>
              {currentModelMeta.thinkingOptions.map((lv) => (
                <button
                  key={lv}
                  type="button"
                  onClick={() => {
                    setThinking(lv);
                    setSheet(null);
                  }}
                  className={cn(
                    "w-full rounded-xl p-3 text-left flex items-center justify-between border cursor-pointer",
                    thinking === lv
                      ? "bg-[var(--soxu-interaction-selected)] border-[var(--soxu-border-strong)]"
                      : "bg-[var(--soxu-surface-card-muted)] border-[var(--soxu-border-subtle)]",
                  )}
                >
                  <span className="text-sm font-medium">{THINKING_LABELS[lv] ?? lv}</span>
                  {thinking === lv && <Check className="h-4 w-4 text-[var(--soxu-semantic-accent)]" strokeWidth={2} />}
                </button>
              ))}
            </SheetContent>
          )}

          {sheet === "perm" && (
            <SheetContent
              title={`${activeProfile.displayName} 支持的操作权限档位`}
              subtitle="高危破坏性命令无论何档位均强制人工确认"
            >
              {activeProfile.permissions.map((p) => (
                <button
                  key={p.id}
                  type="button"
                  onClick={() => {
                    setPermLevel(p.id);
                    setSheet(null);
                  }}
                  className={cn(
                    "w-full rounded-xl p-3 text-left flex items-center justify-between gap-2 border cursor-pointer",
                    permLevel === p.id
                      ? "bg-[var(--soxu-interaction-selected)] border-[var(--soxu-border-strong)]"
                      : "bg-[var(--soxu-surface-card-muted)] border-[var(--soxu-border-subtle)]",
                  )}
                >
                  <div>
                    <div className="text-sm font-semibold">{p.label}</div>
                    <div className="text-xs text-[var(--soxu-text-tertiary)]">{p.desc}</div>
                  </div>
                  {permLevel === p.id && <Check className="h-4 w-4 text-[var(--soxu-semantic-accent)]" strokeWidth={2} />}
                </button>
              ))}
            </SheetContent>
          )}

          {sheet === "handoff" && (
            <SheetContent title="跨代理上下文移交" subtitle="自动附带最近 3 轮摘要与未提交变更文件列表">
              {CLI_OPTIONS.filter((c) => c.available).map((c) => (
                <button
                  key={c.id}
                  type="button"
                  onClick={() => handleHandoff(c.id)}
                  className="w-full rounded-xl p-3 text-left flex items-center justify-between gap-2 bg-[var(--soxu-surface-card-muted)] border border-[var(--soxu-border-subtle)] cursor-pointer"
                >
                  <div className="flex items-center gap-2.5">
                    <AgentIcon agent={c.id} className="h-5 w-5 shrink-0" />
                    <div>
                      <div className="text-sm font-semibold">@{c.id}</div>
                      <div className="text-xs soxu-mono text-[var(--soxu-text-tertiary)]">
                        {c.bin} · v{c.ver}
                      </div>
                    </div>
                  </div>
                  <Badge variant="handoff">移交</Badge>
                </button>
              ))}
            </SheetContent>
          )}

          {sheet === "rewind" && (
            <SheetContent title="检查点 #04 安全回退" subtitle="所有代码回退均会自动保存安全暂存备份" tall>
              {[
                { label: "#04 (当前)", desc: "编辑 crates/soxu-drivers/src/lib.rs (+28 -3)" },
                { label: "#03", desc: "编辑 crates/soxu-core/src/event.rs (+12 -0)" },
              ].map((cp) => (
                <button
                  key={cp.label}
                  type="button"
                  onClick={() => handleApplyRewind(cp.label)}
                  className="w-full rounded-xl p-3 text-left flex items-center justify-between gap-2 bg-[var(--soxu-surface-card-muted)] border border-[var(--soxu-border-subtle)] cursor-pointer"
                >
                  <div>
                    <div className="text-sm font-semibold">检查点 {cp.label}</div>
                    <div className="text-xs text-[var(--soxu-text-tertiary)]">{cp.desc}</div>
                  </div>
                  <Badge variant="warning">回退至此</Badge>
                </button>
              ))}
            </SheetContent>
          )}

          {sheet === "pair" && (
            <SheetContent title="扫码与配对码连接守护服务" subtitle="当前示例地址：127.0.0.1:7698" tall>
              <div className="rounded-xl p-3 bg-[var(--soxu-surface-card-muted)] border border-[var(--soxu-border-subtle)] text-xs space-y-2">
                <div className="font-semibold">一次性安全凭据配对</div>
                <p className="text-[var(--soxu-text-tertiary)] m-0">
                  配对握手参数仅停留于本地链接片段中，不会上传或暴露在服务端日志中。
                </p>
              </div>
              <input
                type="text"
                value={pairCode}
                onChange={(e) => setPairCode(e.target.value)}
                placeholder="输入 6 位一次性配对码（例如 849 217）"
                className="w-full rounded-xl px-3 py-2.5 text-base bg-[var(--soxu-surface-composer)] border border-[var(--soxu-border-strong)] text-[var(--soxu-text-primary)] outline-none"
              />
              <Button
                variant="default"
                size="lg"
                onClick={() => {
                  setDaemonOnline(true);
                  setSheet(null);
                  flashToast("已校验指纹并建立安全连接");
                }}
              >
                验证指纹并上线
              </Button>
            </SheetContent>
          )}

          {sheet === "new-worktree" && (
            <SheetContent title="新建隔离工作树" subtitle="自动归一化分支名称并注入项目上下文">
              <input
                type="text"
                value={newBranchName}
                onChange={(e) => setNewBranchName(e.target.value)}
                placeholder="例如 feat/19-mobile-remote"
                className="w-full rounded-xl px-3 py-2.5 text-base soxu-mono bg-[var(--soxu-surface-composer)] border border-[var(--soxu-border-strong)] text-[var(--soxu-text-primary)] outline-none"
              />
              <Button variant="default" size="lg" onClick={handleCreateWorktree}>
                创建并切入工作树会话
              </Button>
            </SheetContent>
          )}

          {sheet === "new-quick-chat" && (
            <SheetContent title="新建无项目自由对话" subtitle="无需绑定代码仓库，随时开展技术讨论或问答">
              <input
                type="text"
                value={newQuickTitle}
                onChange={(e) => setNewQuickTitle(e.target.value)}
                placeholder="输入对话主题（留空使用默认名称）"
                className="w-full rounded-xl px-3 py-2.5 text-base bg-[var(--soxu-surface-composer)] border border-[var(--soxu-border-strong)] text-[var(--soxu-text-primary)] outline-none"
              />
              <Button variant="default" size="lg" onClick={handleCreateQuickChat}>
                立即进入自由对话
              </Button>
            </SheetContent>
          )}

          {sheet === "history" && (
            <SheetContent title="导入本机原生代理历史" subtitle="只读扫描本地会话记录，支持原位续接或分叉至新工作树" tall>
              {NATIVE_HISTORY.map((item) => (
                <div
                  key={item.id}
                  className="rounded-xl p-3 flex items-center justify-between gap-2 bg-[var(--soxu-surface-card-muted)] border border-[var(--soxu-border-subtle)]"
                >
                  <div className="min-w-0">
                    <div className="text-sm font-semibold truncate">{item.title}</div>
                    <div className="flex items-center gap-1.5 text-xs soxu-mono text-[var(--soxu-text-tertiary)] truncate">
                      <AgentIcon agent={item.cli} className="h-3.5 w-3.5 shrink-0" />
                      <span className="truncate">
                        {item.cli} · {item.project} · {item.updatedAt} · {item.turns} 轮
                      </span>
                    </div>
                  </div>
                  <Button
                    variant="secondary"
                    size="sm"
                    onClick={() => {
                      setSheet(null);
                      flashToast(`已只读导入原生历史：${item.title}`);
                    }}
                  >
                    导入
                  </Button>
                </div>
              ))}
            </SheetContent>
          )}

          {activeScriptForLog && (
            <SheetContent title={`脚本日志 · ${activeScriptForLog.name}`} subtitle={activeScriptForLog.cmd} tall>
              <CodeSnippetView
                lines={activeScriptForLog.logs}
                startLine={1}
                showLineNumbers={prefs.showLineNumbers}
                wrap={prefs.codeWordWrap}
              />
            </SheetContent>
          )}
        </Sheet>

        {/* 独立系统设置中心 */}
        <SettingsDialog
          open={settingsOpen}
          onOpenChange={setSettingsOpen}
          initialSection={settingsSection}
          prefs={prefs}
          onUpdatePrefs={updatePrefs}
          magpieMode={magpieMode}
          onToggleMagpie={() => {
            const next = magpieMode === "magpie_managed" ? "standalone_native" : "magpie_managed";
            setMagpieMode(next);
            flashToast(`路由模式已切换为 ${next === "magpie_managed" ? "协同托管" : "原生直连"}`);
          }}
          onFlashToast={flashToast}
        />
      </div>
    </TooltipProvider>
  );
}

const rootEl = document.getElementById("app");
if (rootEl) {
  createRoot(rootEl).render(
    <StrictMode>
      <MobileApp />
    </StrictMode>,
  );
}
