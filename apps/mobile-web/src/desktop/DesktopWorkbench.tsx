import {
  Archive,
  ArchiveRestore,
  ArrowUp,
  Check,
  ChevronDown,
  ChevronRight,
  Clock,
  Copy,
  Cpu,
  ExternalLink,
  Eye,
  FileCode2,
  FileText,
  Folder,
  FolderOpen,
  GitBranch,
  GitCommit,
  History,
  Inbox,
  LayoutGrid,
  Maximize2,
  MessageSquare,
  Minimize2,
  Network,
  PanelLeft,
  PanelRight,
  Plus,
  QrCode,
  RotateCcw,
  Search,
  Settings,
  Shield,
  Square,
  Star,
  X,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { AgentIcon } from "@/components/AgentIcon";
import { CodeDiffView, CodeSnippetView } from "@/components/CodeBlockView";
import {
  ComposerChipBar,
  ComposerHandoffMenu,
  ComposerPermissionButton,
  ComposerPlusMenu,
  ComposerTurnBehaviorGroup,
} from "@/components/ComposerActionMenus";
import { ContextUsageRing } from "@/components/ContextUsageRing";
import { HoverExpandDock } from "@/components/HoverExpandDock";
import { InboxTriageView } from "@/components/InboxTriageView";
import { MultiQueueBar } from "@/components/MultiQueueBar";
import { QuoteSelectionPopover } from "@/components/QuoteSelectionPopover";
import { SessionStatsPopover } from "@/components/SessionStatsPopover";
import { SettingsDialog, type SettingsSectionId } from "@/components/SettingsDialog";
import { ThemeToggle } from "@/components/ThemeToggle";
import { ToolCallCard } from "@/components/ToolCallCard";
import { UtilityModals, type WorkbenchModalKind } from "@/components/UtilityModals";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Sheet, SheetContent } from "@/components/ui/sheet";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip";
import {
  THINKING_LABELS,
  getAgentModels,
  getAgentProfile,
  getPermissionMeta,
  reconcileAgentCapabilities,
} from "@/lib/agent-capabilities";
import { I18nProvider, useI18n, type I18nKey } from "@/lib/i18n";
import {
  DEFAULT_PREFERENCES,
  applyPreferencesToDom,
  loadPreferences,
  savePreferences,
  toggleThemeWithTransition,
  type SoxuPreferences,
} from "@/lib/preferences";
import { formatShortcut, isImeComposing, useAdaptiveViewport } from "@/lib/useAdaptiveViewport";
import { cn } from "@/lib/utils";
import {
  CLI_OPTIONS,
  INITIAL_APPROVALS,
  INITIAL_ARTIFACT,
  INITIAL_ASK,
  INITIAL_DIFF_FILES,
  INITIAL_LIVE_STEPS,
  INITIAL_QUICK_CHATS,
  INITIAL_SCHEDULES,
  INITIAL_SCRIPTS,
  INITIAL_TURNS,
  INITIAL_WORKSPACES,
} from "@/mock-data";
import type {
  ApprovalItem,
  ArtifactItem,
  ComposerChip,
  HandoffSelection,
  LiveStepItem,
  PermissionLevel,
  QueuedPromptItem,
  QuickChatItem,
  SessionTelemetryStats,
  TurnBehavior,
  TurnMessage,
  WorkspaceBucket,
  WorkspaceItem,
} from "@/types";

const RAIL_TURNS = [
  { idx: 1, title: "第 1 轮 · 为驱动模块补齐热纠偏能力探测与临时沙箱测试", kind: "normal" as const },
  { idx: 2, title: "第 2 轮 · 修改 lib.rs (+5 -1) · 检查点 #04", kind: "handoff" as const },
  { idx: 3, title: "第 3 轮 · 结构化确认 1 项 · 待审批 2 项", kind: "warning" as const },
];

interface SessionTab {
  id: string;
  targetKind: "workspace" | "quick";
  targetId: string;
  cli: string;
  title: string;
  status: WorkspaceBucket;
  draft?: boolean;
}

function bucketDotClass(bucket: WorkspaceBucket): string {
  switch (bucket) {
    case "needs_input":
      return "bg-[var(--soxu-semantic-warning)]";
    case "failed":
      return "bg-[var(--soxu-semantic-danger)]";
    case "running":
      return "bg-[var(--soxu-semantic-running)] soxu-live-dot";
    case "attention":
      return "bg-[var(--soxu-semantic-accent)]";
    case "done":
      return "bg-[var(--soxu-text-tertiary)]";
  }
}

function bucketLabel(bucket: WorkspaceBucket): { labelKey: I18nKey; variant: "warning" | "danger" | "running" | "accent" | "neutral" } {
  switch (bucket) {
    case "needs_input":
      return { labelKey: "status.needsInput", variant: "warning" };
    case "failed":
      return { labelKey: "status.failed", variant: "danger" };
    case "running":
      return { labelKey: "status.running", variant: "running" };
    case "attention":
      return { labelKey: "status.attention", variant: "accent" };
    case "done":
      return { labelKey: "status.done", variant: "neutral" };
  }
}

export function DesktopWorkbench() {
  const [prefs, setPrefs] = useState<SoxuPreferences>(loadPreferences);
  return (
    <I18nProvider locale={prefs.locale}>
      <DesktopWorkbenchInner prefs={prefs} setPrefs={setPrefs} />
    </I18nProvider>
  );
}

function DesktopWorkbenchInner({
  prefs,
  setPrefs,
}: {
  prefs: SoxuPreferences;
  setPrefs: React.Dispatch<React.SetStateAction<SoxuPreferences>>;
}) {
  const { t } = useI18n();
  const [resolvedTheme, setResolvedTheme] = useState<"dark" | "light">(() => applyPreferencesToDom(prefs));
  const vp = useAdaptiveViewport(prefs);
  const isLeftDrawer = vp.width < 900;
  const isRightDrawer = vp.width < 1120;

  const [leftOpen, setLeftOpen] = useState(() => (typeof window !== "undefined" ? window.innerWidth >= 900 : true));
  const [rightOpen, setRightOpen] = useState(() => (typeof window !== "undefined" ? window.innerWidth >= 1120 : true));
  const [inspectorMaximized, setInspectorMaximized] = useState(false);
  const [leftWidth, setLeftWidth] = useState(prefs.leftSidebarWidth);
  const [rightWidth, setRightWidth] = useState(prefs.rightInspectorWidth);
  const [resizingSide, setResizingSide] = useState<"left" | "right" | null>(null);
  const [tabsScrolling, setTabsScrolling] = useState(false);
  const tabStripRef = useRef<HTMLDivElement>(null);
  const tabScrollTimer = useRef<number | null>(null);
  const prevDrawerFlagsRef = useRef({ leftWasDrawer: isLeftDrawer, rightWasDrawer: isRightDrawer });
  const transcriptScrollRef = useRef<HTMLDivElement>(null);
  const transcriptBodyRef = useRef<HTMLDivElement>(null);
  const composerInputRef = useRef<HTMLTextAreaElement>(null);

  const [mainView, setMainView] = useState<"session" | "inbox">("session");
  const [bucketFilter, setBucketFilter] = useState<"all" | WorkspaceBucket>("all");
  const [rightTab, setRightTab] = useState<"diff" | "artifact" | "files" | "scripts" | "terminal">("diff");
  const [diffLayout, setDiffLayout] = useState<"unified" | "split">("unified");
  const [selectedDiffPath, setSelectedDiffPath] = useState(INITIAL_DIFF_FILES[0].path);
  const [diffFilter, setDiffFilter] = useState("");
  const [diffBase, setDiffBase] = useState<"uncommitted" | "main" | "checkpoint">("uncommitted");
  const [selectedFile, setSelectedFile] = useState("crates/soxu-drivers/src/lib.rs");

  const [workspaces, setWorkspaces] = useState<WorkspaceItem[]>(INITIAL_WORKSPACES);
  const [quickChats, setQuickChats] = useState<QuickChatItem[]>(INITIAL_QUICK_CHATS);
  const [openProjects, setOpenProjects] = useState<Record<string, boolean>>({ Soxu: true, Magpie: true });
  const [archivedOpen, setArchivedOpen] = useState(false);

  const [magpieMode, setMagpieMode] = useState<"magpie_managed" | "standalone_native">("magpie_managed");
  const [tabs, setTabs] = useState<SessionTab[]>([
    {
      id: "tab-wks-m2",
      targetKind: "workspace",
      targetId: "wks-m2",
      cli: "codex",
      title: "原生驱动流式热纠偏",
      status: "needs_input",
    },
    {
      id: "tab-chat-arch",
      targetKind: "quick",
      targetId: "chat-arch",
      cli: "agy",
      title: "极简界面排版讨论",
      status: "attention",
    },
    {
      id: "tab-wks-m1",
      targetKind: "workspace",
      targetId: "wks-m1",
      cli: "pi",
      title: "游标增量追平验证",
      status: "running",
    },
  ]);
  const [activeTabId, setActiveTabId] = useState<string | null>("tab-wks-m2");

  const [subagents, setSubagents] = useState([
    { id: "sub-research", cli: "pi", role: "参考库索引巡检", duration: "12 秒", detached: false },
    { id: "sub-test", cli: "codex", role: "沙箱夹具单测", duration: "00:18", detached: false },
  ]);

  const [liveSteps, setLiveSteps] = useState<LiveStepItem[]>(INITIAL_LIVE_STEPS);
  const [liveStepsOpen, setLiveStepsOpen] = useState(true);
  const [activeArtifact, setActiveArtifact] = useState<ArtifactItem>(INITIAL_ARTIFACT);
  const [reasoningOpen, setReasoningOpen] = useState(false);
  const [toolOpen, setToolOpen] = useState<Record<string, boolean>>({
    "tl-read": false,
    "tl-edit": true,
    "tl-bash": true,
    "tl-mcp": false,
  });
  const [ask, setAsk] = useState(INITIAL_ASK);
  const [approvals, setApprovals] = useState<ApprovalItem[]>(INITIAL_APPROVALS);
  const [rejectingId, setRejectingId] = useState<string | null>(null);
  const [rejectNote, setRejectNote] = useState("");
  const [workspaceTurns, setWorkspaceTurns] = useState<TurnMessage[]>([]);
  const [activeRail, setActiveRail] = useState(2);

  const [cli, setCli] = useState("codex");
  const [model, setModel] = useState("group/gpt-6.2-sol");
  const [thinking, setThinking] = useState("medium");
  const [permLevel, setPermLevel] = useState<PermissionLevel>(prefs.defaultPermMode);
  const [composerMode, setComposerMode] = useState<TurnBehavior>("steer");
  const [composerText, setComposerText] = useState("");
  const [queuedPrompts, setQueuedPrompts] = useState<QueuedPromptItem[]>([
    {
      id: "desk-q-init",
      text: "测试全部通过后同步更新研发看板与认知索引",
      createdAt: "刚刚",
      mode: "queue",
    },
  ]);
  const [queuePaused, setQueuePaused] = useState(false);
  const [lostControlToast, setLostControlToast] = useState<string | null>(null);
  const [contextTokens, setContextTokens] = useState<Record<string, number>>({
    "tab-wks-m2": 172800,
    "tab-chat-arch": 24800,
    "tab-wks-m1": 182000,
  });
  const [composerChipsByTab, setComposerChipsByTab] = useState<Record<string, ComposerChip[]>>({
    "tab-wks-m2": [{ id: "chip-init-lib", kind: "file", label: "crates/soxu-drivers/src/lib.rs:22-27" }],
    "tab-wks-m1": [{ id: "chip-init-proto", kind: "file", label: "crates/soxu-protocol/src/lib.rs" }],
  });
  const [sessionStatsByTab, setSessionStatsByTab] = useState<Record<string, SessionTelemetryStats>>({
    "tab-wks-m2": {
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
    },
    "tab-chat-arch": {
      turns: 6,
      steps: 18,
      totalTokens: 38400,
      apiCostUsd: 0.42,
      avgSpeed: 88.6,
      peakSpeed: 142.0,
      recentSpeed: 96.2,
      cacheHitPct: 84.5,
      cacheNote: "稳定命中",
      modelDurationSec: 94,
      toolDurationSec: 18,
      ttftSec: 2,
    },
    "tab-wks-m1": {
      turns: 14,
      steps: 412,
      totalTokens: 19600000,
      apiCostUsd: 9.84,
      avgSpeed: 71.4,
      peakSpeed: 218.5,
      recentSpeed: 85.1,
      cacheHitPct: 88.2,
      cacheNote: "稳定命中",
      modelDurationSec: 46 * 60 + 12,
      toolDurationSec: 29 * 60 + 48,
      ttftSec: 6,
    },
  });

  const [scripts, setScripts] = useState(INITIAL_SCRIPTS);
  const [schedules, setSchedules] = useState(INITIAL_SCHEDULES);
  const [modal, setModal] = useState<WorkbenchModalKind>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [settingsSection, setSettingsSection] = useState<SettingsSectionId>("appearance");
  const [toast, setToast] = useState<string | null>(null);

  useEffect(() => {
    setResolvedTheme(applyPreferencesToDom(prefs));
    savePreferences(prefs);
  }, [prefs]);

  useEffect(() => {
    setLeftWidth(prefs.leftSidebarWidth);
    setRightWidth(prefs.rightInspectorWidth);
  }, [prefs.leftSidebarWidth, prefs.rightInspectorWidth]);

  useEffect(() => {
    if (!prevDrawerFlagsRef.current.leftWasDrawer && isLeftDrawer) {
      setLeftOpen(false);
    }
    if (!prevDrawerFlagsRef.current.rightWasDrawer && isRightDrawer) {
      setRightOpen(false);
      setInspectorMaximized(false);
    }
    prevDrawerFlagsRef.current = { leftWasDrawer: isLeftDrawer, rightWasDrawer: isRightDrawer };
  }, [isLeftDrawer, isRightDrawer]);

  useEffect(() => {
    return () => {
      if (tabScrollTimer.current !== null) {
        window.clearTimeout(tabScrollTimer.current);
      }
    };
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && inspectorMaximized) {
        setInspectorMaximized(false);
      } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setModal((m) => (m === "cmdk" ? null : "cmdk"));
      } else if ((e.ctrlKey || e.metaKey) && e.key === ",") {
        e.preventDefault();
        setSettingsOpen((v) => !v);
      } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "b") {
        e.preventDefault();
        setLeftOpen((v) => !v);
      } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "j") {
        e.preventDefault();
        setRightOpen((v) => !v);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [inspectorMaximized]);

  useEffect(() => {
    const onAltShortcut = (e: KeyboardEvent) => {
      if (e.altKey && e.key.toLowerCase() === "p") {
        e.preventDefault();
        const perms = getAgentProfile(cli).permissions;
        if (perms.length === 0) return;
        setPermLevel((prev) => {
          const idx = perms.findIndex((p) => p.id === prev);
          const next = perms[(idx + 1) % perms.length];
          flashToast(`操作权限已调整为「${next.label}」`);
          return next.id;
        });
      }
    };
    window.addEventListener("keydown", onAltShortcut);
    return () => window.removeEventListener("keydown", onAltShortcut);
  }, [cli]);

  // 左右侧栏自由拖拽调宽
  useEffect(() => {
    if (!resizingSide) return;
    let latestLeft = leftWidth;
    let latestRight = rightWidth;

    const onMove = (ev: PointerEvent) => {
      const vw = window.innerWidth;
      if (resizingSide === "left") {
        const maxAllowed = Math.max(220, Math.min(420, vw - (rightOpen ? rightWidth : 0) - 380));
        latestLeft = Math.round(Math.min(maxAllowed, Math.max(220, ev.clientX)));
        setLeftWidth(latestLeft);
      } else {
        const maxAllowed = Math.max(280, Math.min(620, vw - (leftOpen ? leftWidth : 0) - 380));
        latestRight = Math.round(Math.min(maxAllowed, Math.max(280, vw - ev.clientX)));
        setRightWidth(latestRight);
      }
    };

    const onUp = () => {
      setResizingSide(null);
      setPrefs((p) => ({
        ...p,
        leftSidebarWidth: latestLeft,
        rightInspectorWidth: latestRight,
      }));
    };

    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    return () => {
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
    };
  }, [resizingSide, leftOpen, rightOpen, leftWidth, rightWidth]);

  function markTabStripScrolling() {
    setTabsScrolling(true);
    if (tabScrollTimer.current !== null) {
      window.clearTimeout(tabScrollTimer.current);
    }
    tabScrollTimer.current = window.setTimeout(() => {
      setTabsScrolling(false);
    }, 900);
  }

  function flashToast(msg: string) {
    setToast(msg);
    window.setTimeout(() => setToast((cur) => (cur === msg ? null : cur)), 3000);
  }

  function updatePrefs(patch: Partial<SoxuPreferences>, originEl?: HTMLElement | null) {
    const nextPrefs: SoxuPreferences = { ...prefs, ...patch };
    if (patch.themeMode !== undefined && patch.themeMode !== prefs.themeMode) {
      toggleThemeWithTransition(originEl ?? null, prefs.reduceMotion, () => {
        setResolvedTheme(applyPreferencesToDom(nextPrefs));
        setPrefs(nextPrefs);
      });
    } else {
      setPrefs(nextPrefs);
    }
  }

  function handleToggleTheme(originEl: HTMLElement | null) {
    const next = resolvedTheme === "dark" ? "light" : "dark";
    updatePrefs({ themeMode: next }, originEl);
  }

  function showAdapterNotes(notes: string[]) {
    if (notes.length === 0) {
      setLostControlToast(null);
      return;
    }
    const text = notes.join("；");
    setLostControlToast(text);
    window.setTimeout(() => setLostControlToast((cur) => (cur === text ? null : cur)), 4000);
  }

  function handleSelectCli(nextCliId: string, override?: { model?: string; thinking?: string }) {
    setCli(nextCliId);
    const reconciled = reconcileAgentCapabilities({
      agentId: nextCliId,
      currentModel: override?.model ?? model,
      currentPerm: permLevel,
      currentThinking: override?.thinking ?? thinking,
      currentBehavior: composerMode,
    });
    setModel(reconciled.model);
    setPermLevel(reconciled.perm);
    setThinking(reconciled.thinking);
    setComposerMode(reconciled.behavior);
    showAdapterNotes(reconciled.notes);
    if (activeTabId) {
      setTabs((prev) => prev.map((t) => (t.id === activeTabId ? { ...t, cli: nextCliId } : t)));
    }
    return reconciled;
  }

  function handleSelectModel(nextModelId: string) {
    setModel(nextModelId);
    const reconciled = reconcileAgentCapabilities({
      agentId: cli,
      currentModel: nextModelId,
      currentPerm: permLevel,
      currentThinking: thinking,
      currentBehavior: composerMode,
    });
    setThinking(reconciled.thinking);
    showAdapterNotes(reconciled.notes);
  }

  const activeTab = tabs.find((t) => t.id === activeTabId) ?? null;
  const activeWorkspace =
    activeTab?.targetKind === "workspace"
      ? workspaces.find((w) => w.id === activeTab.targetId) ?? null
      : null;
  const activeQuickChat =
    activeTab?.targetKind === "quick"
      ? quickChats.find((q) => q.id === activeTab.targetId) ?? null
      : null;

  function openWorkspaceTab(w: WorkspaceItem) {
    setMainView("session");
    handleSelectCli(w.agent);
    setModel(w.model);
    if (isLeftDrawer) setLeftOpen(false);
    const existing = tabs.find((t) => t.targetKind === "workspace" && t.targetId === w.id);
    if (existing) {
      setActiveTabId(existing.id);
      return;
    }
    const newTab: SessionTab = {
      id: `tab-${w.id}`,
      targetKind: "workspace",
      targetId: w.id,
      cli: w.agent,
      title: `${w.project} / ${w.branch}`,
      status: w.bucket,
    };
    setTabs((prev) => [...prev, newTab]);
    setActiveTabId(newTab.id);
  }

  function openQuickChatTab(q: QuickChatItem) {
    setMainView("session");
    handleSelectCli(q.agent);
    setModel(q.model);
    if (isLeftDrawer) setLeftOpen(false);
    const existing = tabs.find((t) => t.targetKind === "quick" && t.targetId === q.id);
    if (existing) {
      setActiveTabId(existing.id);
      return;
    }
    const newTab: SessionTab = {
      id: `tab-${q.id}`,
      targetKind: "quick",
      targetId: q.id,
      cli: q.agent,
      title: q.title,
      status: q.bucket,
    };
    setTabs((prev) => [...prev, newTab]);
    setActiveTabId(newTab.id);
  }

  function createQuickChat(initialPrompt?: string) {
    setMainView("session");
    const id = `chat-${Date.now().toString().slice(-4)}`;
    const newChat: QuickChatItem = {
      id,
      title: initialPrompt ? initialPrompt.slice(0, 18) : "新建无项目自由对话",
      agent: cli,
      model,
      bucket: "done",
      updatedAt: "刚刚",
      messages: initialPrompt
        ? [
            { id: `m-u-${Date.now()}`, role: "user", author: "你", text: initialPrompt },
            {
              id: `m-a-${Date.now() + 1}`,
              role: "assistant",
              author: `${cli} · ${model}`,
              text: `已进入无项目自由对话模式，当前以「${getPermissionMeta(cli, permLevel).label}」权限为你分析与推演。`,
            },
          ]
        : [],
    };
    setQuickChats((prev) => [newChat, ...prev]);
    const tabId = `tab-${id}`;
    setTabs((prev) => [
      ...prev,
      { id: tabId, targetKind: "quick", targetId: id, cli, title: newChat.title, status: "done", draft: !initialPrompt },
    ]);
    setActiveTabId(tabId);
    setContextTokens((p) => ({ ...p, [tabId]: 6200 }));
  }

  function closeTab(tabId: string) {
    const idx = tabs.findIndex((t) => t.id === tabId);
    const nextTabs = tabs.filter((t) => t.id !== tabId);
    setTabs(nextTabs);
    if (activeTabId === tabId) {
      const fallback = nextTabs[Math.min(idx, nextTabs.length - 1)] ?? null;
      setActiveTabId(fallback ? fallback.id : null);
    }
  }

  function archiveTarget(kind: "workspace" | "quick", id: string, title: string) {
    if (kind === "quick") {
      setQuickChats((prev) => prev.map((q) => (q.id === id ? { ...q, archived: true } : q)));
    } else {
      setWorkspaces((prev) => prev.map((w) => (w.id === id ? { ...w, archived: true } : w)));
    }
    const closingTab = tabs.find((t) => t.targetKind === kind && t.targetId === id);
    if (closingTab) closeTab(closingTab.id);
    flashToast(`已归档对话：${title}`);
  }

  function restoreArchivedTarget(kind: "workspace" | "quick", id: string) {
    if (kind === "quick") {
      const target = quickChats.find((q) => q.id === id);
      setQuickChats((prev) => prev.map((q) => (q.id === id ? { ...q, archived: false } : q)));
      if (target) openQuickChatTab({ ...target, archived: false });
    } else {
      const target = workspaces.find((w) => w.id === id);
      setWorkspaces((prev) => prev.map((w) => (w.id === id ? { ...w, archived: false } : w)));
      if (target) openWorkspaceTab({ ...target, archived: false });
    }
    flashToast("已从归档恢复会话");
  }

  function addChipToActive(draftChip: Omit<ComposerChip, "id">) {
    const key = activeTabId ?? "__draft__";
    setComposerChipsByTab((prev) => {
      const list = prev[key] ?? [];
      if (draftChip.kind === "handoff") {
        const rest = list.filter((c) => c.kind !== "handoff");
        return { ...prev, [key]: [{ ...draftChip, id: `chip-handoff-${Date.now()}` }, ...rest] };
      }
      if (list.some((c) => c.kind === draftChip.kind && c.label === draftChip.label)) {
        return prev;
      }
      return { ...prev, [key]: [...list, { ...draftChip, id: `chip-${Date.now()}-${Math.random().toString(36).slice(2, 6)}` }] };
    });
  }

  function removeChipFromActive(chipId: string) {
    const key = activeTabId ?? "__draft__";
    setComposerChipsByTab((prev) => ({
      ...prev,
      [key]: (prev[key] ?? []).filter((c) => c.id !== chipId),
    }));
  }

  function bumpTabTelemetry(tabId: string, promptLen: number) {
    setContextTokens((prev) => ({
      ...prev,
      [tabId]: (prev[tabId] ?? 52000) + Math.max(1800, promptLen * 45),
    }));
    setSessionStatsByTab((prev) => {
      const base = prev[tabId] ?? {
        turns: 1,
        steps: 3,
        totalTokens: 12400,
        apiCostUsd: 0.14,
        avgSpeed: 82.0,
        peakSpeed: 128.4,
        recentSpeed: 91.8,
        cacheHitPct: 86.0,
        cacheNote: "正常",
        modelDurationSec: 32,
        toolDurationSec: 11,
        ttftSec: 3,
      };
      const deltaTokens = Math.max(2400, promptLen * 60);
      return {
        ...prev,
        [tabId]: {
          ...base,
          turns: base.turns + 1,
          steps: base.steps + 4,
          totalTokens: base.totalTokens + deltaTokens,
          apiCostUsd: Number((base.apiCostUsd + 0.08).toFixed(2)),
          recentSpeed: Number((88 + (promptLen % 15)).toFixed(1)),
          modelDurationSec: base.modelDurationSec + 14,
          toolDurationSec: base.toolDurationSec + 6,
        },
      };
    });
  }

  function sendPrompt(overrideMode?: TurnBehavior) {
    const activeMode = overrideMode ?? composerMode;
    const tText = composerText.trim();
    const key = activeTabId ?? "__draft__";
    const attachedChips = composerChipsByTab[key] ?? [];
    if (!tText && attachedChips.length === 0) {
      flashToast("已发送优雅中断信号停止当前输出");
      return;
    }

    const activeHandoff = attachedChips.find((c) => c.kind === "handoff" && c.targetAgent);
    let effectiveCli = cli;
    let effectiveModel = model;
    let effectiveThinking = thinking;
    let effectivePerm = permLevel;
    if (activeHandoff?.targetAgent) {
      const nextCfg = handleSelectCli(activeHandoff.targetAgent, {
        model: activeHandoff.targetModel,
        thinking: activeHandoff.targetThinking,
      });
      effectiveCli = activeHandoff.targetAgent;
      effectiveModel = nextCfg.model;
      effectiveThinking = nextCfg.thinking;
      effectivePerm = nextCfg.perm;
    }

    const targetProfile = getAgentProfile(effectiveCli);
    const thinkDesc = targetProfile.supportsThinking
      ? `思考：${THINKING_LABELS[effectiveThinking] ?? effectiveThinking}，`
      : "";
    const finalPrompt = tText || `请处理已附加的 ${attachedChips.length} 项上下文标签`;

    if (!activeTab) {
      createQuickChat(finalPrompt);
      setComposerChipsByTab((p) => ({ ...p, [key]: [] }));
      setComposerText("");
      return;
    }

    if (activeTab.targetKind === "quick" && activeQuickChat) {
      const nextMsgUser: TurnMessage = {
        id: `qu-${Date.now()}`,
        role: "user",
        author: "你",
        text: finalPrompt,
        chips: attachedChips.length > 0 ? attachedChips : undefined,
      };
      const nextMsgAi: TurnMessage = {
        id: `qa-${Date.now() + 1}`,
        role: "assistant",
        author: `${effectiveCli} · ${effectiveModel}`,
        badge: activeHandoff ? `@${effectiveCli} 已接管` : undefined,
        text: activeHandoff
          ? `已将最近上下文与附加标签移交至 @${effectiveCli}（${effectiveModel}，${thinkDesc}${getPermissionMeta(effectiveCli, effectivePerm).label}），开始继续响应。`
          : `已在无项目对话中响应（${getPermissionMeta(effectiveCli, effectivePerm).label}）：收到你的指令「${finalPrompt}」。`,
      };
      setQuickChats((prev) =>
        prev.map((q) =>
          q.id === activeQuickChat.id
            ? {
                ...q,
                title: q.messages.length === 0 ? finalPrompt.slice(0, 18) : q.title,
                updatedAt: "刚刚",
                messages: [...q.messages, nextMsgUser, nextMsgAi],
              }
            : q,
        ),
      );
      setTabs((prev) =>
        prev.map((tb) =>
          tb.id === activeTab.id && tb.draft ? { ...tb, draft: false, title: finalPrompt.slice(0, 18) } : tb,
        ),
      );
      setComposerChipsByTab((p) => ({ ...p, [key]: [] }));
      setComposerText("");
      bumpTabTelemetry(activeTab.id, finalPrompt.length);
      return;
    }

    if (activeMode === "queue" && !activeHandoff) {
      setQueuedPrompts((prev) => [
        ...prev,
        {
          id: `desk-q-${Date.now()}-${Math.random().toString(36).slice(2, 5)}`,
          text: finalPrompt,
          createdAt: "刚刚",
          mode: "queue",
        },
      ]);
      setComposerChipsByTab((p) => ({ ...p, [key]: [] }));
      setComposerText("");
      flashToast("已加入本轮末尾待发队列");
      return;
    }

    setWorkspaceTurns((prev) => [
      ...prev,
      {
        id: `u-${Date.now()}`,
        role: "user",
        author: "你",
        text: finalPrompt,
        chips: attachedChips.length > 0 ? attachedChips : undefined,
        badge: activeHandoff
          ? `移交 -> @${effectiveCli}`
          : activeMode === "steer"
            ? "热纠偏 · 下一步生效"
            : "打断重发",
      },
      {
        id: `a-${Date.now() + 1}`,
        role: "assistant",
        author: `${effectiveCli} · ${effectiveModel}`,
        badge: activeHandoff ? `@${effectiveCli} 接管` : undefined,
        text: activeHandoff
          ? `已接收跨代理移交上下文与 ${attachedChips.length} 项附件标签，以 @${effectiveCli}（${effectiveModel}，${thinkDesc}${getPermissionMeta(effectiveCli, effectivePerm).label}）继续执行。`
          : `已按「${getPermissionMeta(effectiveCli, effectivePerm).label}」策略执行当前指令，仅调整工作树内受控改动。`,
      },
    ]);
    bumpTabTelemetry(activeTab.id, finalPrompt.length);
    setComposerChipsByTab((p) => ({ ...p, [key]: [] }));
    setComposerText("");
  }

  function handleTogglePinWorkspace(workspaceId: string) {
    setWorkspaces((prev) =>
      prev.map((w) => {
        if (w.id !== workspaceId) return w;
        const nextPinned = !w.pinned;
        flashToast(nextPinned ? `已置顶工作树 ${w.branch}` : `已取消置顶 ${w.branch}`);
        return { ...w, pinned: nextPinned };
      }),
    );
  }

  function handleResolveApproval(id: string, resolution: "approved" | "session" | "rejected", note?: string) {
    setApprovals((prev) => prev.map((a) => (a.id === id ? { ...a, resolution, note } : a)));
    setRejectingId(null);
    setRejectNote("");
    flashToast(
      resolution === "rejected"
        ? "已拒绝高危动作并回传修改指导"
        : resolution === "session"
          ? "已批准并在当前会话同类命令自动放行"
          : "已授权单次执行",
    );
  }

  function handleAnswerAsk(optionId: string) {
    const picked = ask.options.find((o) => o.id === optionId);
    if (!picked) return;
    setAsk((prev) => ({ ...prev, selectedId: optionId, submitted: true }));
    flashToast(`已提交确认选择：「${picked.label}」`);
  }

  function handleMarkAllAttentionRead() {
    setWorkspaces((prev) => prev.map((w) => (w.bucket === "attention" ? { ...w, bucket: "done" } : w)));
    flashToast("已将新完成未读工作区全部标记为已读");
  }

  function handleBranchFromTurn(summarySnippet: string) {
    const id = `wks-fork-${Date.now().toString().slice(-4)}`;
    const branch = `feat/fork-${Date.now().toString().slice(-4)}`;
    const forked: WorkspaceItem = {
      id,
      project: activeWorkspace?.project ?? "Soxu",
      projectPath: activeWorkspace?.projectPath ?? "~/projects/Soxu",
      branch,
      isolation: "worktree",
      agent: cli,
      model,
      bucket: "attention",
      summary: `自回合「${summarySnippet.slice(0, 22)}...」分叉的新隔离工作树`,
      activeIntent: "继承前置上下文并并行推进",
      diffStat: "+0 -0",
    };
    setWorkspaces((prev) => [forked, ...prev]);
    openWorkspaceTab(forked);
    flashToast(`已自该回合分叉新会话：${branch}`);
  }

  function handleSubmitDiffLineComment(filePath: string, lineNum: number, codeLine: string, comment: string) {
    addChipToActive({
      kind: "review",
      label: `${filePath}:${lineNum} ${comment}`,
      detail: `${codeLine}\n评审意见：${comment}`,
    });
    flashToast(`已将第 ${lineNum} 行审阅批注附着至输入舱`);
    window.setTimeout(() => composerInputRef.current?.focus(), 40);
  }

  function handlePromoteSteerItem(item: QueuedPromptItem) {
    setWorkspaceTurns((p) => [
      ...p,
      { id: `q-${Date.now()}`, role: "user", author: "你", text: item.text, badge: "提级为热纠偏" },
      {
        id: `qa-${Date.now() + 1}`,
        role: "assistant",
        author: `${cli} · ${model}`,
        text: `已将排队指令提级为热纠偏并在当前回合优先执行：「${item.text}」。`,
      },
    ]);
    setQueuedPrompts((prev) => prev.filter((q) => q.id !== item.id));
    flashToast("已提级为热纠偏注入当前回合");
  }

  function handleEditQueueItem(item: QueuedPromptItem) {
    setComposerText(item.text);
    setQueuedPrompts((prev) => prev.filter((q) => q.id !== item.id));
    window.setTimeout(() => composerInputRef.current?.focus(), 40);
  }

  function handleMoveQueueItem(id: string, dir: -1 | 1) {
    setQueuedPrompts((prev) => {
      const idx = prev.findIndex((q) => q.id === id);
      if (idx < 0) return prev;
      const target = idx + dir;
      if (target < 0 || target >= prev.length) return prev;
      const copy = [...prev];
      const [moved] = copy.splice(idx, 1);
      copy.splice(target, 0, moved);
      return copy;
    });
  }

  function handleRemoveQueueItem(id: string) {
    setQueuedPrompts((prev) => prev.filter((q) => q.id !== id));
  }

  function scrollToTurnAnchor(turnIdx: number) {
    setActiveRail(turnIdx);
    const scroller = transcriptScrollRef.current;
    const target = document.getElementById(`turn-${turnIdx}`);
    if (!scroller || !target) return;
    const scrollerRect = scroller.getBoundingClientRect();
    const targetRect = target.getBoundingClientRect();
    const offsetTop = scroller.scrollTop + (targetRect.top - scrollerRect.top) - 16;
    scroller.scrollTo({
      top: Math.max(0, offsetTop),
      behavior: prefs.reduceMotion ? "auto" : "smooth",
    });
  }

  const rowHeightClass = prefs.density === "compact" ? "min-h-[32px]" : "min-h-[38px]";
  const activeProfile = getAgentProfile(cli);
  const availableModels = getAgentModels(cli);
  const currentModelMeta = availableModels.find((m) => m.id === model) ?? availableModels[0];
  const activeChipKey = activeTabId ?? "__draft__";
  const currentComposerChips = composerChipsByTab[activeChipKey] ?? [];
  const handoffChip = currentComposerChips.find((c) => c.kind === "handoff" && c.targetAgent);
  const selectedHandoff: HandoffSelection | undefined = handoffChip?.targetAgent
    ? {
        targetAgent: handoffChip.targetAgent,
        targetModel: handoffChip.targetModel ?? getAgentModels(handoffChip.targetAgent)[0].id,
        targetThinking: handoffChip.targetThinking,
      }
    : undefined;
  const currentContextUsed = activeTab ? (contextTokens[activeTab.id] ?? 42800) : 6400;
  const currentSessionStats: SessionTelemetryStats = (activeTab && sessionStatsByTab[activeTab.id]) || {
    turns: 3,
    steps: 19,
    totalTokens: 28400,
    apiCostUsd: 0.28,
    avgSpeed: 84.5,
    peakSpeed: 162.3,
    recentSpeed: 92.4,
    cacheHitPct: 87.6,
    cacheNote: "正常",
    modelDurationSec: 54,
    toolDurationSec: 21,
    ttftSec: 4,
  };

  const activeQuickChats = quickChats.filter((q) => !q.archived);
  const activeWorkspaces = workspaces.filter((w) => !w.archived);
  const pinnedWorkspaces = activeWorkspaces.filter((w) => w.pinned);
  const filteredWorkspaces =
    bucketFilter === "all" ? activeWorkspaces : activeWorkspaces.filter((w) => w.bucket === bucketFilter);
  const pendingApprovals = approvals.filter((a) => !a.resolution);
  const inboxAttentionCount =
    pendingApprovals.length +
    activeWorkspaces.filter((w) => w.bucket === "needs_input" || w.bucket === "failed" || w.bucket === "attention").length;

  const filteredDiffFiles = useMemo(() => {
    const q = diffFilter.trim().toLowerCase();
    if (!q) return INITIAL_DIFF_FILES;
    return INITIAL_DIFF_FILES.filter((f) => f.path.toLowerCase().includes(q));
  }, [diffFilter]);

  const activeDiffFile =
    filteredDiffFiles.find((f) => f.path === selectedDiffPath) ?? filteredDiffFiles[0] ?? INITIAL_DIFF_FILES[0];

  const archivedItems = [
    ...quickChats.filter((q) => q.archived).map((q) => ({ kind: "quick" as const, id: q.id, title: q.title, sub: q.updatedAt })),
    ...workspaces
      .filter((w) => w.archived)
      .map((w) => ({ kind: "workspace" as const, id: w.id, title: `${w.project} / ${w.branch}`, sub: w.diffStat })),
  ];

  // 按项目文件夹聚合工作树
  const projectGroups = Array.from(
    filteredWorkspaces.reduce((map, w) => {
      const list = map.get(w.project) ?? { path: w.projectPath, items: [] as WorkspaceItem[] };
      list.items.push(w);
      map.set(w.project, list);
      return map;
    }, new Map<string, { path: string; items: WorkspaceItem[] }>()),
  );

  // 左侧导航正文（宽屏平铺，窄屏自动作为左滑出抽屉内容复用）
  const leftSidebarBody = (
    <div className="flex-1 min-h-0 flex flex-col bg-[var(--soxu-surface-sidebar)] overflow-hidden">
      <div className="flex-1 overflow-y-auto p-2.5 flex flex-col gap-3.5 soxu-scroll">
        {/* 0. 置顶收件箱一触分诊台入口 */}
        <button
          type="button"
          onClick={() => {
            setMainView((v) => (v === "inbox" ? "session" : "inbox"));
            if (isLeftDrawer) setLeftOpen(false);
          }}
          className={cn(
            "w-full rounded-xl px-2.5 py-2 flex items-center justify-between gap-2 text-xs font-medium border cursor-pointer transition-colors",
            mainView === "inbox"
              ? "bg-[var(--soxu-interaction-selected)] border-[var(--soxu-border-strong)] text-[var(--soxu-text-primary)]"
              : "bg-[var(--soxu-surface-card)] border-[var(--soxu-border-subtle)] text-[var(--soxu-text-secondary)] hover:text-[var(--soxu-text-primary)] hover:border-[var(--soxu-border-strong)]",
          )}
        >
          <span className="inline-flex items-center gap-2 min-w-0">
            <Inbox className="h-3.5 w-3.5 text-[var(--soxu-semantic-accent)] shrink-0" strokeWidth={1.75} />
            <span className="truncate">{t("sidebar.inbox")}</span>
          </span>
          {inboxAttentionCount > 0 && (
            <Badge variant="warning" className="soxu-tabular">
              {inboxAttentionCount}
            </Badge>
          )}
        </button>

        {/* 0.5 已置顶重要工作树 */}
        {pinnedWorkspaces.length > 0 && (
          <div className="flex flex-col gap-1">
            <div className="flex items-center justify-between px-1.5">
              <span className="text-[11px] font-semibold tracking-wider text-[var(--soxu-text-tertiary)] inline-flex items-center gap-1">
                <Star className="h-3 w-3 text-[var(--soxu-semantic-warning)] fill-current" strokeWidth={1.75} />
                <span>{t("sidebar.pinned")}</span>
              </span>
            </div>
            {pinnedWorkspaces.map((w) => {
              const selected = mainView === "session" && activeTab?.targetKind === "workspace" && activeTab.targetId === w.id;
              return (
                <div
                  key={`pin-${w.id}`}
                  data-group-active={selected ? "true" : undefined}
                  onClick={() => openWorkspaceTab(w)}
                  role="button"
                  tabIndex={0}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" || e.key === " ") {
                      e.preventDefault();
                      openWorkspaceTab(w);
                    }
                  }}
                  className={cn(
                    "group w-full rounded-lg px-2 py-1.5 flex items-center justify-between gap-2 text-left text-xs transition-colors duration-150 cursor-pointer border",
                    rowHeightClass,
                    selected
                      ? "bg-[var(--soxu-interaction-selected)] border-[var(--soxu-border-subtle)] text-[var(--soxu-text-primary)]"
                      : "border-transparent text-[var(--soxu-text-secondary)] hover:bg-[var(--soxu-interaction-hover)] hover:text-[var(--soxu-text-primary)]",
                  )}
                >
                  <div className="min-w-0 flex flex-col gap-0.5">
                    <div className="flex items-center gap-1.5 min-w-0">
                      <span className={cn("h-2 w-2 rounded-full shrink-0", bucketDotClass(w.bucket))} />
                      <span className="soxu-mono font-medium truncate">{w.branch}</span>
                    </div>
                    {w.activeIntent && (
                      <div className="pl-3.5 text-[10.5px] text-[var(--soxu-semantic-running)] truncate">{w.activeIntent}</div>
                    )}
                  </div>
                  <button
                    type="button"
                    title={t("sidebar.unpinAction")}
                    aria-label={`${t("sidebar.unpinAction")} ${w.branch}`}
                    onClick={(e) => {
                      e.stopPropagation();
                      handleTogglePinWorkspace(w.id);
                    }}
                    className="soxu-hit-44 soxu-reveal-action h-5 w-5 rounded flex items-center justify-center text-[var(--soxu-semantic-warning)] hover:bg-[var(--soxu-surface-control)] shrink-0 bg-transparent border-0 cursor-pointer"
                  >
                    <Star className="h-3 w-3 fill-current" strokeWidth={1.75} />
                  </button>
                </div>
              );
            })}
          </div>
        )}

        {/* 1. 无项目快速对话区 */}
        <div className="flex flex-col gap-1">
          <div className="flex items-center justify-between px-1.5 mb-0.5">
            <span className="text-[11px] font-semibold tracking-wider text-[var(--soxu-text-tertiary)]">
              {t("sidebar.quickChatsCount", { count: activeQuickChats.length })}
            </span>
            <Tooltip>
              <TooltipTrigger asChild>
                <button
                  type="button"
                  onClick={() => createQuickChat()}
                  className="soxu-hit-44 h-5 w-5 rounded flex items-center justify-center text-[var(--soxu-text-tertiary)] hover:bg-[var(--soxu-interaction-hover)] hover:text-[var(--soxu-text-primary)] cursor-pointer"
                  aria-label={t("sidebar.newQuickChat")}
                >
                  <Plus className="h-3.5 w-3.5" strokeWidth={1.75} />
                </button>
              </TooltipTrigger>
              <TooltipContent>{t("sidebar.newQuickChat")}</TooltipContent>
            </Tooltip>
          </div>

          {activeQuickChats.map((qc) => {
            const selected = mainView === "session" && activeTab?.targetKind === "quick" && activeTab.targetId === qc.id;
            return (
              <div
                key={qc.id}
                data-group-active={selected ? "true" : undefined}
                onClick={() => openQuickChatTab(qc)}
                role="button"
                tabIndex={0}
                onKeyDown={(e) => {
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    openQuickChatTab(qc);
                  }
                }}
                className={cn(
                  "group w-full rounded-lg px-2 py-1.5 flex items-center justify-between gap-2 text-left text-xs transition-colors duration-150 cursor-pointer border",
                  rowHeightClass,
                  selected
                    ? "bg-[var(--soxu-interaction-selected)] border-[var(--soxu-border-subtle)] text-[var(--soxu-text-primary)]"
                    : "border-transparent text-[var(--soxu-text-secondary)] hover:bg-[var(--soxu-interaction-hover)] hover:text-[var(--soxu-text-primary)]",
                )}
              >
                <div className="flex items-center gap-1.5 min-w-0">
                  <span className={cn("h-2 w-2 rounded-full shrink-0", bucketDotClass(qc.bucket))} />
                  <AgentIcon agent={qc.agent} className="h-3.5 w-3.5 shrink-0" />
                  <span className="truncate font-medium">{qc.title}</span>
                </div>
                <button
                  type="button"
                  title={t("sidebar.archiveAction")}
                  aria-label={`${t("sidebar.archiveAction")} ${qc.title}`}
                  onClick={(e) => {
                    e.stopPropagation();
                    archiveTarget("quick", qc.id, qc.title);
                  }}
                  className="soxu-reveal-action soxu-hit-44 h-5 w-5 rounded flex items-center justify-center text-[var(--soxu-text-tertiary)] hover:text-[var(--soxu-text-primary)] hover:bg-[var(--soxu-surface-control)] shrink-0 bg-transparent border-0 cursor-pointer"
                >
                  <Archive className="h-3 w-3" strokeWidth={1.75} />
                </button>
              </div>
            );
          })}
        </div>

        {/* 2. 项目文件夹 -> 工作树二级目录 */}
        <div className="flex flex-col gap-2">
          <div className="flex items-center justify-between px-1.5">
            <span className="text-[11px] font-semibold tracking-wider text-[var(--soxu-text-tertiary)]">
              {t("sidebar.projectsCount", { count: projectGroups.length })}
            </span>
          </div>

          <div className="flex items-center gap-1 overflow-x-auto soxu-no-scrollbar px-0.5">
            {(
              [
                { id: "all", label: t("inbox.filterAll") },
                { id: "needs_input", label: t("status.needsInput") },
                { id: "failed", label: t("status.failed") },
                { id: "running", label: t("status.running") },
                { id: "attention", label: t("status.attention") },
              ] as const
            ).map((f) => (
              <button
                key={f.id}
                type="button"
                onClick={() => setBucketFilter(f.id)}
                className={cn(
                  "px-2 py-0.5 rounded-full text-[10.5px] whitespace-nowrap border cursor-pointer transition-colors",
                  bucketFilter === f.id
                    ? "bg-[var(--soxu-action-inverse)] text-[var(--soxu-action-on-inverse)] border-transparent font-semibold"
                    : "bg-transparent border-[var(--soxu-border-hairline)] text-[var(--soxu-text-tertiary)] hover:text-[var(--soxu-text-primary)]",
                )}
              >
                {f.label}
              </button>
            ))}
          </div>

          {projectGroups.map(([projName, group]) => {
            const isExpanded = openProjects[projName] ?? true;
            return (
              <div key={projName} className="flex flex-col gap-0.5">
                <div className="group flex items-center justify-between rounded-lg px-1.5 py-1 hover:bg-[var(--soxu-interaction-hover)] transition-colors">
                  <button
                    type="button"
                    onClick={() => setOpenProjects((p) => ({ ...p, [projName]: !isExpanded }))}
                    className="flex-1 min-w-0 flex items-center gap-1.5 text-left text-xs font-semibold text-[var(--soxu-text-primary)] bg-transparent border-0 cursor-pointer"
                  >
                    <ChevronRight
                      className={cn("h-3.5 w-3.5 text-[var(--soxu-text-tertiary)] shrink-0 transition-transform duration-150", isExpanded && "rotate-90")}
                      strokeWidth={1.75}
                    />
                    {isExpanded ? (
                      <FolderOpen className="h-3.5 w-3.5 text-[var(--soxu-text-secondary)] shrink-0" strokeWidth={1.75} />
                    ) : (
                      <Folder className="h-3.5 w-3.5 text-[var(--soxu-text-secondary)] shrink-0" strokeWidth={1.75} />
                    )}
                    <span className="truncate">{projName}</span>
                    <span className="text-[10.5px] soxu-mono font-normal text-[var(--soxu-text-tertiary)] truncate">
                      {prefs.redactHomePath ? group.path : group.path.replace("~", "/home/dev")}
                    </span>
                  </button>

                  <Tooltip>
                    <TooltipTrigger asChild>
                      <button
                        type="button"
                        onClick={() => {
                          const newWks: WorkspaceItem = {
                            id: `wks-${Date.now().toString().slice(-4)}`,
                            project: projName,
                            projectPath: group.path,
                            branch: `feat/wt-${Date.now().toString().slice(-3)}`,
                            isolation: "worktree",
                            agent: cli,
                            model,
                            bucket: "done",
                            summary: "新建隔离工作树，已注入目录上下文",
                            diffStat: "+0 -0",
                          };
                          setWorkspaces((prev) => [newWks, ...prev]);
                          openWorkspaceTab(newWks);
                          flashToast(`已在 ${projName} 下新建工作树 ${newWks.branch}`);
                        }}
                        className="soxu-reveal-action soxu-hit-44 h-5 w-5 rounded flex items-center justify-center text-[var(--soxu-text-tertiary)] hover:text-[var(--soxu-text-primary)] hover:bg-[var(--soxu-surface-control)] cursor-pointer bg-transparent border-0 shrink-0"
                        aria-label={`为项目 ${projName} 新建工作树`}
                      >
                        <Plus className="h-3.5 w-3.5" strokeWidth={1.75} />
                      </button>
                    </TooltipTrigger>
                    <TooltipContent>在 {projName} 下新建隔离工作树</TooltipContent>
                  </Tooltip>
                </div>

                {isExpanded && (
                  <div className="ml-3 pl-2.5 border-l border-[var(--soxu-border-hairline)] flex flex-col gap-0.5 mt-0.5">
                    {group.items.map((w) => {
                      const selected = mainView === "session" && activeTab?.targetKind === "workspace" && activeTab.targetId === w.id;
                      const b = bucketLabel(w.bucket);
                      return (
                        <div
                          key={w.id}
                          role="button"
                          tabIndex={0}
                          data-group-active={selected ? "true" : undefined}
                          onClick={() => openWorkspaceTab(w)}
                          onKeyDown={(e) => {
                            if (e.key === "Enter" || e.key === " ") {
                              e.preventDefault();
                              openWorkspaceTab(w);
                            }
                          }}
                          className={cn(
                            "group w-full rounded-lg px-2 py-1.5 flex flex-col gap-0.5 text-left text-xs transition-colors duration-150 cursor-pointer border",
                            rowHeightClass,
                            selected
                              ? "bg-[var(--soxu-interaction-selected)] border-[var(--soxu-border-subtle)] text-[var(--soxu-text-primary)]"
                              : "border-transparent text-[var(--soxu-text-secondary)] hover:bg-[var(--soxu-interaction-hover)] hover:text-[var(--soxu-text-primary)]",
                          )}
                        >
                          <div className="flex items-center justify-between gap-1.5 min-w-0">
                            <div className="flex items-center gap-1.5 min-w-0 flex-1">
                              <span className={cn("h-2 w-2 rounded-full shrink-0", bucketDotClass(w.bucket))} />
                              {w.isolation === "worktree" ? (
                                <GitBranch className="h-3 w-3 shrink-0 text-[var(--soxu-text-tertiary)]" strokeWidth={1.75} />
                              ) : (
                                <Folder className="h-3 w-3 shrink-0 text-[var(--soxu-text-tertiary)]" strokeWidth={1.75} />
                              )}
                              <span className="soxu-mono font-medium truncate">{w.branch}</span>
                            </div>
                            <div className="flex items-center gap-0.5 shrink-0">
                              {w.bucket !== "done" && <Badge variant={b.variant}>{t(b.labelKey)}</Badge>}
                              <button
                                type="button"
                                title={w.pinned ? t("sidebar.unpinAction") : t("sidebar.pinAction")}
                                aria-label={w.pinned ? t("sidebar.unpinAction") : t("sidebar.pinAction")}
                                onClick={(e) => {
                                  e.stopPropagation();
                                  handleTogglePinWorkspace(w.id);
                                }}
                                className={cn(
                                  "soxu-reveal-action soxu-hit-44 h-5 w-5 rounded flex items-center justify-center hover:bg-[var(--soxu-surface-control)] cursor-pointer bg-transparent border-0",
                                  w.pinned ? "opacity-100 text-[var(--soxu-semantic-warning)]" : "text-[var(--soxu-text-tertiary)] hover:text-[var(--soxu-text-primary)]",
                                )}
                              >
                                <Star className={cn("h-3 w-3", w.pinned && "fill-current")} strokeWidth={1.75} />
                              </button>
                              <button
                                type="button"
                                title={t("sidebar.archiveAction")}
                                aria-label={`${t("sidebar.archiveAction")} ${w.branch}`}
                                onClick={(e) => {
                                  e.stopPropagation();
                                  archiveTarget("workspace", w.id, `${w.project} / ${w.branch}`);
                                }}
                                className="soxu-reveal-action soxu-hit-44 h-5 w-5 rounded flex items-center justify-center text-[var(--soxu-text-tertiary)] hover:text-[var(--soxu-text-primary)] hover:bg-[var(--soxu-surface-control)] cursor-pointer bg-transparent border-0"
                              >
                                <Archive className="h-3 w-3" strokeWidth={1.75} />
                              </button>
                            </div>
                          </div>
                          {w.activeIntent && (
                            <div className="pl-3.5 text-[10.5px] text-[var(--soxu-semantic-running)] truncate">
                              {w.activeIntent}
                            </div>
                          )}
                          <div className="pl-3.5 text-[10.5px] soxu-tabular text-[var(--soxu-text-tertiary)] flex items-center justify-between gap-2 whitespace-nowrap">
                            <span className="inline-flex items-center gap-1.5 min-w-0 truncate">
                              <span className="shrink-0">{w.isolation === "worktree" ? t("sidebar.worktree") : t("sidebar.localRoot")}</span>
                              <span className="inline-flex items-center gap-1 soxu-mono shrink-0">
                                <AgentIcon agent={w.agent} className="h-3 w-3 shrink-0" />
                                <span>{w.agent}</span>
                              </span>
                            </span>
                            <span className="soxu-mono shrink-0">{w.diffStat}</span>
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

        {/* 3. 已归档对话折叠区 */}
        <Collapsible open={archivedOpen} onOpenChange={setArchivedOpen}>
          <CollapsibleTrigger asChild>
            <button
              type="button"
              className="w-full px-1.5 py-1 flex items-center justify-between text-[11px] font-medium text-[var(--soxu-text-tertiary)] hover:text-[var(--soxu-text-secondary)] cursor-pointer"
            >
              <span className="flex items-center gap-1.5">
                <Archive className="h-3.5 w-3.5" strokeWidth={1.75} />
                <span>{t("sidebar.archivedCount", { count: archivedItems.length })}</span>
              </span>
              <ChevronRight
                className={cn("h-3.5 w-3.5 transition-transform duration-150", archivedOpen && "rotate-90")}
                strokeWidth={1.75}
              />
            </button>
          </CollapsibleTrigger>
          <CollapsibleContent className="flex flex-col gap-1 pt-1">
            {archivedItems.length === 0 ? (
              <div className="px-2 py-2 text-[11px] text-[var(--soxu-text-ghost)]">{t("sidebar.noArchived")}</div>
            ) : (
              archivedItems.map((arc) => (
                <div
                  key={arc.id}
                  className="rounded-lg px-2 py-1.5 bg-[var(--soxu-surface-card-muted)] border border-[var(--soxu-border-hairline)] flex items-center justify-between gap-2 text-xs"
                >
                  <div className="min-w-0">
                    <div className="truncate text-[var(--soxu-text-secondary)]">{arc.title}</div>
                    <div className="text-[10px] text-[var(--soxu-text-tertiary)]">{arc.sub}</div>
                  </div>
                  <button
                    type="button"
                    onClick={() => restoreArchivedTarget(arc.kind, arc.id)}
                    className="h-6 px-2 rounded text-[11px] text-[var(--soxu-semantic-accent)] hover:bg-[var(--soxu-interaction-hover)] cursor-pointer shrink-0 inline-flex items-center gap-1"
                  >
                    <ArchiveRestore className="h-3 w-3" strokeWidth={1.75} />
                    <span>{t("sidebar.restore")}</span>
                  </button>
                </div>
              ))
            )}
          </CollapsibleContent>
        </Collapsible>
      </div>

      {/* 左栏底部快捷工具 */}
      <div className="border-t border-[var(--soxu-border-hairline)] p-2 flex items-center justify-between gap-2 shrink-0">
        <div className="flex items-center gap-1 min-w-0">
          <Button
            variant="ghost"
            size="sm"
            className="h-6 px-2 text-[11px]"
            onClick={() => {
              if (isLeftDrawer) setLeftOpen(false);
              setModal("history");
            }}
          >
            <History className="h-3 w-3" strokeWidth={1.75} />
            <span>{t("sidebar.importHistory")}</span>
          </Button>
          <Button
            variant="ghost"
            size="sm"
            className="h-6 px-2 text-[11px]"
            onClick={() => {
              if (isLeftDrawer) setLeftOpen(false);
              setModal("schedules");
            }}
          >
            <Clock className="h-3 w-3" strokeWidth={1.75} />
            <span>{t("sidebar.schedules")}</span>
          </Button>
        </div>
        <span className="text-[11px] text-[var(--soxu-text-tertiary)] px-1.5 truncate">
          {magpieMode === "magpie_managed" ? t("sidebar.gatewayManaged") : t("sidebar.gatewayDirect")}
        </span>
      </div>
    </div>
  );

  // 右侧上下文与代码审阅正文（宽屏平铺，支持全宽最大化，窄屏自动作为右滑出抽屉内容复用）
  const rightInspectorBody = (
    <div className="flex-1 min-h-0 flex flex-col bg-[var(--soxu-surface-sidebar)] overflow-hidden">
      <div className="h-[38px] px-2.5 border-b border-[var(--soxu-border-hairline)] flex items-center justify-between gap-2 shrink-0">
        <div className="min-w-0 overflow-x-auto soxu-no-scrollbar">
          <Tabs value={rightTab} onValueChange={(v) => setRightTab(v as typeof rightTab)}>
            <TabsList>
              <TabsTrigger value="diff">{t("inspector.diff")}</TabsTrigger>
              <TabsTrigger value="artifact">{t("inspector.artifact")}</TabsTrigger>
              <TabsTrigger value="files">{t("inspector.files")}</TabsTrigger>
              <TabsTrigger value="scripts">{t("inspector.scripts")}</TabsTrigger>
              <TabsTrigger value="terminal">{t("inspector.terminal")}</TabsTrigger>
            </TabsList>
          </Tabs>
        </div>
        <div className="flex items-center gap-1 shrink-0">
          {!isRightDrawer && (
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  variant={inspectorMaximized ? "secondary" : "ghost"}
                  size="icon-sm"
                  onClick={() => setInspectorMaximized((v) => !v)}
                  aria-label={inspectorMaximized ? t("inspector.restoreWidth") : t("inspector.maximize")}
                >
                  {inspectorMaximized ? (
                    <Minimize2 className="h-3.5 w-3.5" strokeWidth={1.75} />
                  ) : (
                    <Maximize2 className="h-3.5 w-3.5" strokeWidth={1.75} />
                  )}
                </Button>
              </TooltipTrigger>
              <TooltipContent>
                {inspectorMaximized ? t("inspector.restoreWidth") : t("inspector.maximize")}
              </TooltipContent>
            </Tooltip>
          )}
          {isRightDrawer && (
            <Button
              variant="ghost"
              size="icon-sm"
              onClick={() => setRightOpen(false)}
              aria-label={t("header.toggleRight")}
              className="shrink-0"
            >
              <X className="h-4 w-4" strokeWidth={1.75} />
            </Button>
          )}
        </div>
      </div>

      <div className="flex-1 overflow-y-auto p-3 flex flex-col gap-3 soxu-scroll">
        {activeTab?.targetKind !== "workspace" && rightTab !== "terminal" && rightTab !== "artifact" ? (
          <div className="rounded-xl border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card-muted)] p-5 text-center text-xs text-[var(--soxu-text-tertiary)] flex flex-col items-center gap-2.5 mt-4">
            <Folder className="h-5 w-5 text-[var(--soxu-text-tertiary)]" strokeWidth={1.75} />
            <div>{t("inspector.emptyTitle")}</div>
            <Button
              size="sm"
              variant="outline"
              onClick={() => {
                const first = activeWorkspaces[0];
                if (first) openWorkspaceTab(first);
              }}
            >
              {t("tabs.newWorkspace")}
            </Button>
          </div>
        ) : (
          <>
            {rightTab === "diff" && (
              <>
                <div className="flex items-center justify-between gap-2 flex-wrap">
                  <div className="inline-flex rounded-lg bg-[var(--soxu-surface-inset)] p-0.5 border border-[var(--soxu-border-hairline)]">
                    {(["unified", "split"] as const).map((m) => (
                      <button
                        key={m}
                        type="button"
                        onClick={() => setDiffLayout(m)}
                        className={cn(
                          "px-2.5 py-1 rounded-md text-[11px] border-0 bg-transparent cursor-pointer",
                          diffLayout === m
                            ? "bg-[var(--soxu-surface-raised)] text-[var(--soxu-text-primary)]"
                            : "text-[var(--soxu-text-tertiary)]",
                        )}
                      >
                        {m === "unified" ? t("inspector.diffUnified") : t("inspector.diffSplit")}
                      </button>
                    ))}
                  </div>
                  <Button
                    size="sm"
                    variant="default"
                    onClick={() => {
                      if (isRightDrawer) setRightOpen(false);
                      setModal("commit");
                    }}
                  >
                    <GitCommit className="h-3.5 w-3.5" strokeWidth={1.75} />
                    <span>{t("inspector.commitChanges")}</span>
                  </Button>
                </div>

                {/* 多文件 Diff 过滤、基准切换与变动列表 */}
                <div className="flex flex-col gap-2">
                  <div className="relative">
                    <Search
                      className="h-3.5 w-3.5 text-[var(--soxu-text-tertiary)] absolute left-2.5 top-1/2 -translate-y-1/2 pointer-events-none"
                      strokeWidth={1.75}
                    />
                    <input
                      type="text"
                      value={diffFilter}
                      onChange={(e) => setDiffFilter(e.target.value)}
                      placeholder={t("inspector.filterPlaceholder")}
                      className="w-full rounded-lg pl-8 pr-2.5 py-1 text-xs bg-[var(--soxu-surface-composer)] border border-[var(--soxu-border-subtle)] text-[var(--soxu-text-primary)] placeholder:text-[var(--soxu-text-ghost)] outline-none"
                    />
                  </div>

                  <div className="flex items-center gap-1 overflow-x-auto soxu-no-scrollbar">
                    {(
                      [
                        { id: "uncommitted", label: t("inspector.baseUncommitted") },
                        { id: "main", label: t("inspector.baseMain") },
                        { id: "checkpoint", label: t("inspector.baseCheckpoint") },
                      ] as const
                    ).map((b) => (
                      <button
                        key={b.id}
                        type="button"
                        onClick={() => setDiffBase(b.id)}
                        className={cn(
                          "px-2 py-0.5 rounded-md text-[10.5px] whitespace-nowrap border cursor-pointer transition-colors",
                          diffBase === b.id
                            ? "bg-[var(--soxu-interaction-selected)] border-[var(--soxu-border-strong)] text-[var(--soxu-text-primary)] font-medium"
                            : "bg-transparent border-[var(--soxu-border-hairline)] text-[var(--soxu-text-tertiary)]",
                        )}
                      >
                        {b.label}
                      </button>
                    ))}
                  </div>

                  <div className="flex flex-col gap-1">
                    {filteredDiffFiles.map((file) => {
                      const selected = file.path === activeDiffFile.path;
                      return (
                        <button
                          key={file.path}
                          type="button"
                          onClick={() => setSelectedDiffPath(file.path)}
                          className={cn(
                            "w-full rounded-lg px-2 py-1 text-left text-[11px] flex items-center justify-between gap-2 border cursor-pointer transition-colors",
                            selected
                              ? "bg-[var(--soxu-interaction-selected)] border-[var(--soxu-border-strong)] text-[var(--soxu-text-primary)]"
                              : "bg-[var(--soxu-surface-card-muted)] border-[var(--soxu-border-hairline)] text-[var(--soxu-text-secondary)]",
                          )}
                        >
                          <span className="inline-flex items-center gap-1.5 min-w-0">
                            <Badge variant={file.status === "A" ? "running" : "neutral"}>{file.status}</Badge>
                            <span className="soxu-mono truncate">{file.path}</span>
                          </span>
                          <span className="soxu-mono soxu-tabular text-[10.5px] shrink-0">
                            <span className="text-[var(--soxu-semantic-success)]">+{file.added}</span>{" "}
                            <span className="text-[var(--soxu-semantic-danger)]">-{file.deleted}</span>
                          </span>
                        </button>
                      );
                    })}
                  </div>
                </div>

                <div className="text-[11px] soxu-mono text-[var(--soxu-text-tertiary)] truncate">
                  {activeDiffFile.path} · {t("inspector.baseBranchLabel", { branch: diffBase === "main" ? "origin/main" : diffBase === "checkpoint" ? "#04" : prefs.defaultBranch })}
                </div>

                <CodeDiffView
                  filePath={activeDiffFile.path}
                  rows={activeDiffFile.rows}
                  layout={diffLayout}
                  showLineNumbers={prefs.showLineNumbers}
                  wrap={prefs.codeWordWrap}
                  onSubmitLineComment={handleSubmitDiffLineComment}
                />
              </>
            )}

            {rightTab === "artifact" && (
              <div className="rounded-xl border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card)] p-3.5 flex flex-col gap-3">
                <div className="flex items-start justify-between gap-2">
                  <div className="min-w-0">
                    <div className="text-xs font-semibold leading-snug">{activeArtifact.title}</div>
                    <div className="text-[10.5px] soxu-mono text-[var(--soxu-text-tertiary)] truncate mt-0.5">
                      {activeArtifact.path}
                    </div>
                  </div>
                  <Badge variant={activeArtifact.status === "approved" ? "running" : "warning"}>
                    {activeArtifact.status === "approved" ? t("artifact.approved") : "待审阅"}
                  </Badge>
                </div>
                <p className="text-xs text-[var(--soxu-text-secondary)] m-0 leading-relaxed">{activeArtifact.summary}</p>
                <CodeSnippetView
                  lines={activeArtifact.content}
                  startLine={1}
                  showLineNumbers={prefs.showLineNumbers}
                  wrap
                />
                {activeArtifact.status !== "approved" && (
                  <Button
                    size="sm"
                    variant="default"
                    onClick={() => {
                      setActiveArtifact((prev) => ({ ...prev, status: "approved" }));
                      setLiveSteps((prev) =>
                        prev.map((st, idx) => (idx <= 2 ? { ...st, status: "done" } : idx === 3 ? { ...st, status: "in_progress" } : st)),
                      );
                      flashToast("已批准实施产物文档并推进后续步骤");
                    }}
                  >
                    <Check className="h-3.5 w-3.5" strokeWidth={2} />
                    <span>{t("artifact.approve")}</span>
                  </Button>
                )}
              </div>
            )}

            {rightTab === "files" && (
              <>
                <div className="flex flex-col gap-0.5">
                  {["AGENTS.md", "design-system/soxu/tokens.json", "crates/soxu-drivers/src/lib.rs", "crates/soxu-magpie/src/lib.rs"].map(
                    (f) => (
                      <button
                        key={f}
                        type="button"
                        onClick={() => setSelectedFile(f)}
                        className={cn(
                          "w-full rounded-lg px-2.5 py-1.5 text-left text-xs soxu-mono flex items-center gap-2 cursor-pointer",
                          selectedFile === f
                            ? "bg-[var(--soxu-interaction-selected)] text-[var(--soxu-text-primary)]"
                            : "text-[var(--soxu-text-secondary)] hover:bg-[var(--soxu-interaction-hover)]",
                        )}
                      >
                        <FileCode2 className="h-3.5 w-3.5 shrink-0" strokeWidth={1.75} />
                        <span className="truncate">{f}</span>
                      </button>
                    ),
                  )}
                </div>
                <CodeSnippetView
                  startLine={1}
                  showLineNumbers={prefs.showLineNumbers}
                  wrap={prefs.codeWordWrap}
                  lines={[
                    `// ${selectedFile}`,
                    "pub enum EnvironmentMode {",
                    "  MagpieManaged,",
                    "  StandaloneNative,",
                    "}",
                  ]}
                />
              </>
            )}

            {rightTab === "scripts" && (
              <div className="flex flex-col gap-2">
                {scripts.map((s) => (
                  <div
                    key={s.name}
                    className="rounded-xl border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card)] p-2.5 flex items-center justify-between gap-2"
                  >
                    <div className="min-w-0">
                      <div className="flex items-center gap-1.5 text-xs font-semibold soxu-mono">
                        <span>{s.name}</span>
                        {s.port && <Badge variant="accent">{s.port}</Badge>}
                      </div>
                      <div className="text-[11px] soxu-mono text-[var(--soxu-text-tertiary)] truncate">{s.cmd}</div>
                    </div>
                    <Button
                      size="sm"
                      variant={s.running ? "secondary" : "default"}
                      onClick={() =>
                        setScripts((p) => p.map((x) => (x.name === s.name ? { ...x, running: !x.running } : x)))
                      }
                    >
                      {s.running ? "停止" : "启动"}
                    </Button>
                  </div>
                ))}
              </div>
            )}
          </>
        )}

        {rightTab === "terminal" && (
          <pre className="m-0 rounded-lg border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-terminal)] p-3 soxu-code-block leading-relaxed overflow-x-auto">
            <div>
              <span style={{ color: "var(--soxu-ansi-bright-green)" }}>dev@soxu</span>{" "}
              <span style={{ color: "var(--soxu-ansi-blue)" }}>~/projects/Soxu</span>
            </div>
            <div>
              <span style={{ color: "var(--soxu-ansi-green)" }}>OK</span> 全仓门禁验证已就绪
            </div>
          </pre>
        )}
      </div>
    </div>
  );

  return (
    <TooltipProvider delayDuration={220}>
      <div className="h-screen w-screen flex flex-col bg-[var(--soxu-surface-canvas)] text-[var(--soxu-text-primary)] overflow-hidden">
        {/* 顶部标题栏：支持 macOS 红绿灯安全留白与跨平台快捷键提示，三栏对称 Grid 保证搜索框恒定居中 */}
        <header
          className={cn(
            "relative h-[var(--soxu-size-titlebar-height)] min-h-[var(--soxu-size-titlebar-height)] bg-[var(--soxu-surface-sidebar)] border-b border-[var(--soxu-border-hairline)] pr-2 sm:pr-3 grid grid-cols-[auto_minmax(0,1fr)_auto] sm:grid-cols-[minmax(max-content,1fr)_minmax(150px,360px)_minmax(max-content,1fr)] items-center gap-1.5 sm:gap-3 select-none z-[var(--soxu-z-header)]",
            vp.isMac ? "pl-[78px]" : "pl-2 sm:pl-3",
          )}
        >
          {prefs.windowChromeStyle === "macos" && (
            <div
              aria-hidden="true"
              className="absolute left-3.5 top-1/2 -translate-y-1/2 flex items-center gap-2 pointer-events-none"
            >
              <span className="h-2.5 w-2.5 rounded-full bg-[#ff5f57] border border-black/20" />
              <span className="h-2.5 w-2.5 rounded-full bg-[#febc2e] border border-black/20" />
              <span className="h-2.5 w-2.5 rounded-full bg-[#28c840] border border-black/20" />
            </div>
          )}

          <div className="flex items-center gap-1.5 sm:gap-2 min-w-0 justify-self-start">
            {/* Soxu 为品牌名，不做中文翻译，且前面严禁加任何状态圆点 */}
            <span className="px-1 text-[13px] font-semibold tracking-tight text-[var(--soxu-text-primary)] shrink-0">
              {t("brand.name")}
            </span>

            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  variant={leftOpen ? "secondary" : "ghost"}
                  size="icon-sm"
                  onClick={() => setLeftOpen((v) => !v)}
                  aria-label={isLeftDrawer ? t("header.openLeftDrawer") : t("header.toggleLeft")}
                  className="shrink-0"
                >
                  <PanelLeft className="h-4 w-4" strokeWidth={1.75} />
                </Button>
              </TooltipTrigger>
              <TooltipContent>
                {isLeftDrawer ? t("header.openLeftDrawer") : `${t("header.toggleLeft")} (${formatShortcut("Mod+B", vp.isMac)})`}
              </TooltipContent>
            </Tooltip>

            <span className="hidden lg:inline-flex items-center gap-1.5 text-xs text-[var(--soxu-text-tertiary)] truncate min-w-0">
              {mainView === "inbox" ? (
                <>
                  <Inbox className="h-3.5 w-3.5 text-[var(--soxu-semantic-accent)] shrink-0" strokeWidth={1.75} />
                  <span className="truncate">{t("inbox.title")}</span>
                </>
              ) : activeTab?.targetKind === "quick" ? (
                <>
                  <MessageSquare className="h-3.5 w-3.5 shrink-0" strokeWidth={1.75} />
                  <span className="truncate">{t("header.quickChat")}</span>
                </>
              ) : activeWorkspace ? (
                <>
                  <GitBranch className="h-3.5 w-3.5 shrink-0" strokeWidth={1.75} />
                  <span className="soxu-mono truncate">{activeWorkspace.project} / {activeWorkspace.branch}</span>
                </>
              ) : (
                <span className="truncate">{t("header.idleWorkbench")}</span>
              )}
            </span>
          </div>

          {/* 始终居中的命令搜索触发条 */}
          <button
            type="button"
            onClick={() => setModal("cmdk")}
            className="h-[26px] w-full max-w-[360px] min-w-0 justify-self-center rounded-lg border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card)] px-2 sm:px-2.5 flex items-center justify-between gap-1.5 text-xs text-[var(--soxu-text-tertiary)] hover:border-[var(--soxu-border-strong)] hover:text-[var(--soxu-text-secondary)] transition-colors cursor-pointer"
          >
            <span className="flex items-center gap-1.5 min-w-0">
              <Search className="h-3.5 w-3.5 shrink-0" strokeWidth={1.75} />
              <span className="truncate">{t("header.searchPlaceholder")}</span>
            </span>
            <kbd className="hidden sm:inline-flex soxu-mono text-[10px] px-1.5 py-0.5 rounded bg-[var(--soxu-surface-control)] text-[var(--soxu-text-secondary)] shrink-0">
              {formatShortcut("Mod+K", vp.isMac)}
            </kbd>
          </button>

          {/* 右侧极简工具区：无角标悬停收纳（窄屏悬浮弹出）+ 收件箱快捷入口 + 主题切换 + 设置齿轮 + 右栏/抽屉开关 */}
          <div className="flex items-center gap-1 justify-self-end shrink-0">
            <HoverExpandDock
              triggerIcon={<LayoutGrid className="h-4 w-4" strokeWidth={1.75} />}
              triggerLabel={t("header.moreTools")}
              expandDirection="left"
            >
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    onClick={() => {
                      setSettingsSection("magpie");
                      setSettingsOpen(true);
                    }}
                    aria-label="代理与路由诊断"
                  >
                    <Network className="h-3.5 w-3.5" strokeWidth={1.75} />
                  </Button>
                </TooltipTrigger>
                <TooltipContent>代理与路由器诊断</TooltipContent>
              </Tooltip>

              <Tooltip>
                <TooltipTrigger asChild>
                  <Button variant="ghost" size="icon-sm" onClick={() => setModal("schedules")} aria-label="定时任务调度中心">
                    <Clock className="h-3.5 w-3.5" strokeWidth={1.75} />
                  </Button>
                </TooltipTrigger>
                <TooltipContent>定时任务与周期巡检</TooltipContent>
              </Tooltip>

              <Tooltip>
                <TooltipTrigger asChild>
                  <Button variant="ghost" size="icon-sm" onClick={() => setModal("pair")} aria-label="手机扫码配对">
                    <QrCode className="h-3.5 w-3.5" strokeWidth={1.75} />
                  </Button>
                </TooltipTrigger>
                <TooltipContent>手机扫码遥控配对</TooltipContent>
              </Tooltip>

              <Tooltip>
                <TooltipTrigger asChild>
                  <Button variant="ghost" size="icon-sm" onClick={() => setModal("history")} aria-label="导入原生历史会话">
                    <History className="h-3.5 w-3.5" strokeWidth={1.75} />
                  </Button>
                </TooltipTrigger>
                <TooltipContent>导入本机历史会话</TooltipContent>
              </Tooltip>

              <Tooltip>
                <TooltipTrigger asChild>
                  <Button variant="ghost" size="icon-sm" onClick={() => setModal("commit")} aria-label="提交工作树变更">
                    <GitCommit className="h-3.5 w-3.5" strokeWidth={1.75} />
                  </Button>
                </TooltipTrigger>
                <TooltipContent>提交代码变更</TooltipContent>
              </Tooltip>
            </HoverExpandDock>

            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  variant={mainView === "inbox" ? "secondary" : "ghost"}
                  size="icon-sm"
                  onClick={() => setMainView((v) => (v === "inbox" ? "session" : "inbox"))}
                  aria-label={t("inbox.title")}
                  className="relative"
                >
                  <Inbox className="h-4 w-4" strokeWidth={1.75} />
                  {inboxAttentionCount > 0 && (
                    <span className="absolute -top-0.5 -right-0.5 min-w-[14px] h-[14px] px-1 rounded-full text-[9px] leading-[14px] text-center bg-[var(--soxu-semantic-warning)] text-black font-bold">
                      {inboxAttentionCount}
                    </span>
                  )}
                </Button>
              </TooltipTrigger>
              <TooltipContent>{t("inbox.title")}</TooltipContent>
            </Tooltip>

            <ThemeToggle resolvedTheme={resolvedTheme} onToggle={handleToggleTheme} />

            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  variant="ghost"
                  size="icon-sm"
                  onClick={() => {
                    setSettingsSection("appearance");
                    setSettingsOpen(true);
                  }}
                  aria-label={t("header.settings")}
                >
                  <Settings className="h-4 w-4" strokeWidth={1.75} />
                </Button>
              </TooltipTrigger>
              <TooltipContent>{t("header.settings")} ({formatShortcut("Mod+,", vp.isMac)})</TooltipContent>
            </Tooltip>

            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  variant={rightOpen ? "secondary" : "ghost"}
                  size="icon-sm"
                  onClick={() => setRightOpen((v) => !v)}
                  aria-label={isRightDrawer ? t("header.openRightDrawer") : t("header.toggleRight")}
                >
                  <PanelRight className="h-4 w-4" strokeWidth={1.75} />
                </Button>
              </TooltipTrigger>
              <TooltipContent>
                {isRightDrawer ? t("header.openRightDrawer") : `${t("header.toggleRight")} (${formatShortcut("Mod+J", vp.isMac)})`}
              </TooltipContent>
            </Tooltip>
          </div>
        </header>

        {/* 主四区网格：宽屏左右平铺支持拖拽与键盘调宽，支持右栏全宽审阅模式；窄屏左右栏自动收进抽屉 */}
        <div className="flex-1 min-h-0 flex overflow-hidden">
          {!isLeftDrawer && !(inspectorMaximized && rightOpen) && (
            <>
              <aside
                aria-label={t("sidebar.title")}
                style={{ width: leftOpen ? `${leftWidth}px` : "0px" }}
                className={cn(
                  "flex flex-col bg-[var(--soxu-surface-sidebar)] overflow-hidden shrink-0",
                  !resizingSide && "transition-[width,opacity] duration-[var(--soxu-motion-panel)] ease-[var(--soxu-ease-out)]",
                  leftOpen ? "opacity-100" : "opacity-0",
                )}
              >
                <div style={{ width: `${leftWidth}px` }} className="flex-1 min-h-0 flex flex-col">
                  {leftSidebarBody}
                </div>
              </aside>

              {leftOpen && (
                <div
                  role="separator"
                  tabIndex={0}
                  aria-orientation="vertical"
                  aria-label="调整左侧栏宽度"
                  aria-valuemin={220}
                  aria-valuemax={420}
                  aria-valuenow={leftWidth}
                  onPointerDown={(e) => {
                    e.preventDefault();
                    setResizingSide("left");
                  }}
                  onDoubleClick={() => updatePrefs({ leftSidebarWidth: DEFAULT_PREFERENCES.leftSidebarWidth })}
                  onKeyDown={(e) => {
                    if (e.key === "ArrowLeft") {
                      e.preventDefault();
                      const next = Math.max(220, leftWidth - 24);
                      setLeftWidth(next);
                      updatePrefs({ leftSidebarWidth: next });
                    } else if (e.key === "ArrowRight") {
                      e.preventDefault();
                      const next = Math.min(420, leftWidth + 24);
                      setLeftWidth(next);
                      updatePrefs({ leftSidebarWidth: next });
                    } else if (e.key === "Home") {
                      e.preventDefault();
                      setLeftWidth(DEFAULT_PREFERENCES.leftSidebarWidth);
                      updatePrefs({ leftSidebarWidth: DEFAULT_PREFERENCES.leftSidebarWidth });
                    }
                  }}
                  title="左右拖动或方向键调整宽度，双击或 Home 恢复默认"
                  className="flex w-[9px] -mx-[4px] z-20 cursor-col-resize items-center justify-center group select-none shrink-0 focus-visible:outline-none"
                >
                  <div
                    className={cn(
                      "h-full w-px transition-colors duration-150",
                      resizingSide === "left"
                        ? "w-[2px] bg-[var(--soxu-semantic-running)]"
                        : "bg-[var(--soxu-border-hairline)] group-hover:w-[2px] group-hover:bg-[var(--soxu-semantic-running)] group-focus-visible:w-[2px] group-focus-visible:bg-[var(--soxu-semantic-accent)]",
                    )}
                  />
                </div>
              )}
            </>
          )}

          {/* 中央多代理会话画布 */}
          <main
            className={cn(
              "flex-1 min-w-0 flex flex-col bg-[var(--soxu-surface-canvas)] overflow-hidden relative",
              inspectorMaximized && !isRightDrawer && rightOpen && "hidden",
            )}
          >
            {/* 会话页签条：先自适应缩窄宽度，超出最小容量后再显隐底部横向滚动条；+ 按钮固定右侧 */}
            <div className="h-[var(--soxu-size-tabbar-height)] min-h-[var(--soxu-size-tabbar-height)] bg-[var(--soxu-surface-sidebar)] border-b border-[var(--soxu-border-hairline)] px-2 flex items-end gap-1">
              <div
                ref={tabStripRef}
                role="tablist"
                data-scrolling={tabsScrolling ? "true" : undefined}
                onScroll={markTabStripScrolling}
                onWheel={(e) => {
                  const el = tabStripRef.current;
                  if (el && el.scrollWidth > el.clientWidth && Math.abs(e.deltaY) > Math.abs(e.deltaX)) {
                    el.scrollLeft += e.deltaY;
                    markTabStripScrolling();
                  }
                }}
                className="flex-1 min-w-0 h-full flex items-end gap-0.5 overflow-x-auto overflow-y-hidden soxu-auto-scrollbar"
              >
                {tabs.map((tb) => {
                  const active = mainView === "session" && activeTabId === tb.id;
                  return (
                    <div
                      key={tb.id}
                      role="tab"
                      tabIndex={0}
                      aria-selected={active}
                      data-group-active={active ? "true" : undefined}
                      onClick={() => {
                        setMainView("session");
                        setActiveTabId(tb.id);
                      }}
                      onKeyDown={(e) => {
                        if (e.key === "Enter" || e.key === " ") {
                          e.preventDefault();
                          setMainView("session");
                          setActiveTabId(tb.id);
                        }
                      }}
                      className={cn(
                        "group relative h-[31px] flex-1 basis-[196px] max-w-[220px] min-w-[116px] px-2.5 rounded-t-lg text-xs flex items-center gap-1.5 whitespace-nowrap cursor-pointer transition-colors duration-150 border border-b-0 select-none",
                        active
                          ? "bg-[var(--soxu-surface-canvas)] text-[var(--soxu-text-primary)] border-[var(--soxu-border-hairline)]"
                          : "bg-transparent text-[var(--soxu-text-tertiary)] border-transparent hover:bg-[var(--soxu-interaction-hover)] hover:text-[var(--soxu-text-primary)]",
                      )}
                    >
                      <span className={cn("h-2 w-2 rounded-full shrink-0", bucketDotClass(tb.status))} />
                      <AgentIcon agent={tb.cli} className="h-3.5 w-3.5 shrink-0" />
                      <span className="soxu-mono text-[10.5px] text-[var(--soxu-text-tertiary)] shrink-0">{tb.cli}</span>
                      <span className={cn("flex-1 min-w-0 truncate", tb.draft && "italic")}>{tb.title}</span>

                      <div className="flex items-center gap-0.5 shrink-0">
                        <button
                          type="button"
                          title={t("tabs.archiveTab")}
                          aria-label={`${t("tabs.archiveTab")} ${tb.title}`}
                          onClick={(e) => {
                            e.stopPropagation();
                            archiveTarget(tb.targetKind, tb.targetId, tb.title);
                          }}
                          className="soxu-reveal-action soxu-hit-44 h-4 w-4 rounded flex items-center justify-center text-[var(--soxu-text-tertiary)] hover:text-[var(--soxu-text-primary)] hover:bg-[var(--soxu-surface-control)] cursor-pointer bg-transparent border-0"
                        >
                          <Archive className="h-2.5 w-2.5" strokeWidth={1.75} />
                        </button>
                        <button
                          type="button"
                          title={t("tabs.closeTab")}
                          aria-label={`${t("tabs.closeTab")} ${tb.title}`}
                          onClick={(e) => {
                            e.stopPropagation();
                            closeTab(tb.id);
                          }}
                          className="soxu-reveal-action soxu-hit-44 h-4 w-4 rounded flex items-center justify-center text-[var(--soxu-text-tertiary)] hover:text-[var(--soxu-text-primary)] hover:bg-[var(--soxu-surface-control)] cursor-pointer bg-transparent border-0"
                        >
                          <X className="h-3 w-3" strokeWidth={1.75} />
                        </button>
                      </div>
                    </div>
                  );
                })}
              </div>

              <Tooltip>
                <TooltipTrigger asChild>
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    className="mb-0.5 shrink-0"
                    onClick={() => createQuickChat()}
                    aria-label={t("sidebar.newQuickChat")}
                  >
                    <Plus className="h-3.5 w-3.5" strokeWidth={1.75} />
                  </Button>
                </TooltipTrigger>
                <TooltipContent>{t("sidebar.newQuickChat")}</TooltipContent>
              </Tooltip>
            </div>

            {/* 子代理委派轨道（仅绑定工作区会话展示） */}
            {mainView === "session" && activeTab?.targetKind === "workspace" && (
              <div className="h-[var(--soxu-size-subagent-bar-height)] min-h-[var(--soxu-size-subagent-bar-height)] bg-[var(--soxu-surface-sidebar)] border-b border-[var(--soxu-border-hairline)] px-3 flex items-center gap-2 text-[11.5px] text-[var(--soxu-text-tertiary)] overflow-x-auto soxu-no-scrollbar">
                <span className="shrink-0">子代理轨：</span>
                {subagents.map((sa) => (
                  <span
                    key={sa.id}
                    className="inline-flex items-center gap-1.5 h-5 px-2 rounded-full bg-[var(--soxu-surface-card)] border border-[var(--soxu-border-subtle)] text-[var(--soxu-text-secondary)] soxu-mono shrink-0"
                  >
                    <AgentIcon agent={sa.cli} className="h-3.5 w-3.5 shrink-0" />
                    <span>
                      {sa.cli} · {sa.role} ({sa.duration})
                    </span>
                    {sa.detached ? (
                      <span className="text-[10.5px] text-[var(--soxu-semantic-accent)]">已独立</span>
                    ) : (
                      <button
                        type="button"
                        onClick={() => {
                          setSubagents((prev) => prev.map((x) => (x.id === sa.id ? { ...x, detached: true } : x)));
                          const tid = `tab-${sa.id}`;
                          setTabs((prev) => [
                            ...prev,
                            {
                              id: tid,
                              targetKind: "workspace",
                              targetId: activeWorkspace?.id ?? "wks-m2",
                              cli: sa.cli,
                              title: `子代理：${sa.role}`,
                              status: "running",
                            },
                          ]);
                          setActiveTabId(tid);
                          flashToast(`已独立子代理页签：${sa.role}`);
                        }}
                        className="inline-flex items-center gap-0.5 px-1.5 rounded-full bg-[var(--soxu-surface-control)] text-[10px] text-[var(--soxu-text-primary)] hover:bg-[var(--soxu-action-inverse)] hover:text-[var(--soxu-action-on-inverse)] transition-colors cursor-pointer"
                      >
                        <span>独立页签</span>
                        <ExternalLink className="h-2.5 w-2.5" strokeWidth={1.75} />
                      </button>
                    )}
                  </span>
                ))}
              </div>
            )}

            {/* 主滚动容器（对话正文与底部悬浮输入舱同在内部共用同一水平内边距与限宽，确保宽度 1:1 严格对齐并随侧栏拖拽同步缩放） */}
            <div className="flex-1 min-h-0 relative flex flex-col">
              <div ref={transcriptScrollRef} className="flex-1 overflow-y-auto soxu-scroll flex flex-col justify-between">
                <div className="px-3 sm:px-6 pt-5 pb-6 flex-1">
                  <div
                    ref={transcriptBodyRef}
                    className="w-full max-w-[var(--soxu-layout-transcript-max-width)] mx-auto flex flex-col gap-5"
                  >
                    {mainView === "inbox" ? (
                      <div className="flex flex-col gap-3">
                        <div className="flex items-center justify-between gap-2">
                          <span className="text-xs text-[var(--soxu-text-tertiary)]">
                            统一集中处理阻塞审批、等待确认与未读新完成工作树
                          </span>
                          <Button variant="outline" size="sm" onClick={() => setMainView("session")}>
                            返回会话视图
                          </Button>
                        </div>
                        <InboxTriageView
                          workspaces={activeWorkspaces}
                          approvals={approvals}
                          askQuestion={ask}
                          onResolveApproval={handleResolveApproval}
                          onAnswerAsk={handleAnswerAsk}
                          onOpenWorkspace={(wsId) => {
                            const found = workspaces.find((w) => w.id === wsId);
                            if (found) openWorkspaceTab(found);
                            else setMainView("session");
                          }}
                          onMarkAllAttentionRead={handleMarkAllAttentionRead}
                        />
                      </div>
                    ) : !activeTab ? (
                      <div className="my-12 rounded-2xl border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card)] p-8 text-center flex flex-col items-center gap-4">
                        <div className="h-11 w-11 rounded-full bg-[var(--soxu-surface-raised)] border border-[var(--soxu-border-strong)] flex items-center justify-center">
                          <MessageSquare className="h-5 w-5 text-[var(--soxu-semantic-accent)]" strokeWidth={1.75} />
                        </div>
                        <div>
                          <h2 className="text-base font-semibold">{t("tabs.emptyTitle")}</h2>
                          <p className="text-xs text-[var(--soxu-text-secondary)] mt-1">{t("tabs.emptyDesc")}</p>
                        </div>
                        <div className="flex flex-wrap items-center justify-center gap-2">
                          <Button variant="default" onClick={() => createQuickChat()}>
                            <Plus className="h-4 w-4" strokeWidth={1.75} />
                            <span>{t("tabs.newQuick")}</span>
                          </Button>
                          <Button
                            variant="secondary"
                            onClick={() => {
                              const first = activeWorkspaces[0];
                              if (first) openWorkspaceTab(first);
                            }}
                          >
                            <FolderOpen className="h-4 w-4" strokeWidth={1.75} />
                            <span>{t("tabs.newWorkspace")}</span>
                          </Button>
                        </div>
                      </div>
                    ) : activeTab.targetKind === "quick" && activeQuickChat ? (
                      <>
                        <div className="rounded-xl border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card-muted)] px-3.5 py-2.5 flex items-center justify-between gap-3 text-xs">
                          <span className="flex items-center gap-2 text-[var(--soxu-text-secondary)] truncate">
                            <MessageSquare className="h-3.5 w-3.5 text-[var(--soxu-semantic-accent)] shrink-0" strokeWidth={1.75} />
                            <span className="truncate">{t("tabs.quickBanner")}</span>
                          </span>
                          <Badge variant="neutral">{t("tabs.quickModeBadge")}</Badge>
                        </div>

                        {activeQuickChat.messages.length === 0 ? (
                          <div className="py-10 text-center text-xs text-[var(--soxu-text-tertiary)]">
                            在下方输入问题，立即开启无项目自由对话
                          </div>
                        ) : (
                          activeQuickChat.messages.map((m) =>
                            m.role === "user" ? (
                              <div key={m.id} className="flex flex-col items-end gap-1.5">
                                {m.chips && m.chips.length > 0 && <ComposerChipBar chips={m.chips} className="justify-end" />}
                                <div className="max-w-[78%] rounded-[18px] bg-[var(--soxu-surface-raised)] border border-[var(--soxu-border-subtle)] px-4 py-2.5 leading-relaxed">
                                  {m.text}
                                </div>
                              </div>
                            ) : (
                              <div key={m.id} className="group flex flex-col gap-2">
                                <div className="flex items-center justify-between gap-2 text-xs font-semibold">
                                  <span className="inline-flex items-center gap-2 min-w-0">
                                    <AgentIcon agent={m.author} className="h-4 w-4 shrink-0" />
                                    <span className="truncate">{m.author}</span>
                                    {m.badge && <Badge variant="handoff">{m.badge}</Badge>}
                                  </span>
                                  <Button
                                    variant="ghost"
                                    size="sm"
                                    className="soxu-reveal-action h-6 px-2 text-[11px] font-normal"
                                    onClick={() => handleBranchFromTurn(m.text)}
                                  >
                                    <GitBranch className="h-3 w-3" strokeWidth={1.75} />
                                    <span>{t("turn.branchSession")}</span>
                                  </Button>
                                </div>
                                <p className="m-0 leading-relaxed text-[var(--soxu-text-primary)]">{m.text}</p>
                              </div>
                            ),
                          )
                        )}
                      </>
                    ) : (
                      <>
                        {/* 实时步骤焦点（吸收 AGY Live Steps） */}
                        <Collapsible
                          open={liveStepsOpen}
                          onOpenChange={setLiveStepsOpen}
                          className="rounded-xl border border-[var(--soxu-border-hairline)] bg-[var(--soxu-surface-card-muted)]"
                        >
                          <CollapsibleTrigger asChild>
                            <button
                              type="button"
                              className="w-full px-3.5 py-2 flex items-center justify-between gap-2 text-xs bg-transparent border-0 cursor-pointer"
                            >
                              <span className="inline-flex items-center gap-2 text-[var(--soxu-text-secondary)] font-medium min-w-0">
                                <span className="h-2 w-2 rounded-full bg-[var(--soxu-semantic-running)] soxu-live-dot shrink-0" />
                                <span className="truncate">
                                  {t("focus.title")} · {liveSteps.filter((s) => s.status === "done").length}/{liveSteps.length}
                                </span>
                              </span>
                              <ChevronRight
                                className={cn("h-3.5 w-3.5 text-[var(--soxu-text-tertiary)] transition-transform", liveStepsOpen && "rotate-90")}
                                strokeWidth={1.75}
                              />
                            </button>
                          </CollapsibleTrigger>
                          <CollapsibleContent className="soxu-collapsible overflow-hidden px-3.5 pb-2.5 grid grid-cols-1 sm:grid-cols-2 gap-1.5">
                            {liveSteps.map((st) => (
                              <div key={st.id} className="flex items-center gap-2 text-xs">
                                {st.status === "done" ? (
                                  <Check className="h-3.5 w-3.5 text-[var(--soxu-semantic-success)] shrink-0" strokeWidth={2} />
                                ) : st.status === "in_progress" ? (
                                  <span className="h-2 w-2 mx-0.5 rounded-full bg-[var(--soxu-semantic-running)] soxu-live-dot shrink-0" />
                                ) : (
                                  <span className="h-2 w-2 mx-0.5 rounded-full bg-[var(--soxu-border-strong)] shrink-0" />
                                )}
                                <span
                                  className={cn(
                                    "truncate",
                                    st.status === "done"
                                      ? "text-[var(--soxu-text-tertiary)] line-through"
                                      : st.status === "in_progress"
                                        ? "text-[var(--soxu-text-primary)] font-medium"
                                        : "text-[var(--soxu-text-tertiary)]",
                                  )}
                                >
                                  {st.title}
                                </span>
                              </div>
                            ))}
                          </CollapsibleContent>
                        </Collapsible>

                        {/* 工作区绑定回合流 */}
                        <div id="turn-1" className="flex justify-end">
                          <div className="max-w-[78%] rounded-[18px] bg-[var(--soxu-surface-raised)] border border-[var(--soxu-border-subtle)] px-4 py-2.5 leading-relaxed">
                            {INITIAL_TURNS[0].text}
                          </div>
                        </div>

                        <div id="turn-2" className="group group/msg flex flex-col gap-2.5">
                          <div className="flex items-center justify-between gap-2 text-xs">
                            <div className="flex items-center gap-2 font-semibold min-w-0">
                              <AgentIcon agent={cli} className="h-4 w-4 shrink-0" />
                              <span>{cli}</span>
                              <span className="soxu-mono font-normal text-[var(--soxu-text-tertiary)] truncate">
                                {model} · {THINKING_LABELS[thinking] ?? thinking}
                              </span>
                            </div>
                            <div className="soxu-reveal-action flex items-center gap-1 shrink-0">
                              <Button
                                variant="ghost"
                                size="sm"
                                className="h-6 px-2 text-[11px]"
                                onClick={() => flashToast("已复制当前回合答复内容")}
                              >
                                <Copy className="h-3 w-3" strokeWidth={1.75} />
                                <span>复制</span>
                              </Button>
                              <Button
                                variant="ghost"
                                size="sm"
                                className="h-6 px-2 text-[11px]"
                                onClick={() => handleBranchFromTurn(INITIAL_TURNS[1].text)}
                              >
                                <GitBranch className="h-3 w-3" strokeWidth={1.75} />
                                <span>{t("turn.branchSession")}</span>
                              </Button>
                              <Button variant="ghost" size="sm" className="h-6 px-2 text-[11px]" onClick={() => setModal("commit")}>
                                <GitCommit className="h-3 w-3" strokeWidth={1.75} />
                                <span>提交</span>
                              </Button>
                            </div>
                          </div>

                          {/* 深度思考折叠卡 */}
                          <Collapsible open={reasoningOpen} onOpenChange={setReasoningOpen}>
                            <CollapsibleTrigger asChild>
                              <button
                                type="button"
                                className="inline-flex items-center gap-1.5 rounded-md px-1.5 py-1 text-xs text-[var(--soxu-text-tertiary)] hover:bg-[var(--soxu-interaction-hover)] hover:text-[var(--soxu-text-secondary)] cursor-pointer transition-colors"
                              >
                                <ChevronRight
                                  className={cn("h-3.5 w-3.5 transition-transform duration-150", reasoningOpen && "rotate-90")}
                                  strokeWidth={1.75}
                                />
                                <span>已深度思考 6.4 秒，点击查看推演步骤</span>
                              </button>
                            </CollapsibleTrigger>
                            <CollapsibleContent className="ml-2 pl-3 border-l border-[var(--soxu-border-subtle)] text-xs text-[var(--soxu-text-secondary)] flex flex-col gap-1 py-1">
                              <div>1. 对照决策约束：仅部分驱动原生支持运行中热注入；其余自动转为柔和中断续接。</div>
                              <div>2. 遵循安全红线，全部测试夹具隔离于临时目录，严禁写入全局配置文件。</div>
                            </CollapsibleContent>
                          </Collapsible>

                          {/* 统一结构化工具调用卡列表（Oryn 22px 图标块 + 意图读数 + 可行级批注 Diff） */}
                          <div className="flex flex-col gap-2">
                            {INITIAL_TURNS[1].tools?.map((tl) => (
                              <ToolCallCard
                                key={tl.id}
                                tool={tl}
                                open={Boolean(toolOpen[tl.id])}
                                onOpenChange={(o) => setToolOpen((p) => ({ ...p, [tl.id]: o }))}
                                showLineNumbers={prefs.showLineNumbers}
                                wrap={prefs.codeWordWrap}
                                onSubmitLineComment={handleSubmitDiffLineComment}
                              />
                            ))}
                          </div>

                          {/* 结构化 Artifact 产出卡 */}
                          <div className="rounded-xl border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card)] p-3.5 flex items-center justify-between gap-3 flex-wrap">
                            <div className="flex items-center gap-2.5 min-w-0">
                              <div className="h-8 w-8 rounded-lg bg-[var(--soxu-semantic-accent-soft)] text-[var(--soxu-semantic-accent)] flex items-center justify-center shrink-0">
                                <FileText className="h-4 w-4" strokeWidth={1.75} />
                              </div>
                              <div className="min-w-0">
                                <div className="flex items-center gap-2">
                                  <span className="text-xs font-semibold truncate">{activeArtifact.title}</span>
                                  <Badge variant={activeArtifact.status === "approved" ? "running" : "warning"}>
                                    {activeArtifact.status === "approved" ? t("artifact.approved") : "待审阅"}
                                  </Badge>
                                </div>
                                <div className="text-[11px] soxu-mono text-[var(--soxu-text-tertiary)] truncate">
                                  {activeArtifact.path}
                                </div>
                              </div>
                            </div>
                            <div className="flex items-center gap-1.5 shrink-0">
                              <Button
                                variant="outline"
                                size="sm"
                                onClick={() => {
                                  setRightTab("artifact");
                                  setRightOpen(true);
                                }}
                              >
                                <Eye className="h-3.5 w-3.5" strokeWidth={1.75} />
                                <span>{t("artifact.preview")}</span>
                              </Button>
                              {activeArtifact.status !== "approved" && (
                                <Button
                                  variant="default"
                                  size="sm"
                                  onClick={() => {
                                    setActiveArtifact((prev) => ({ ...prev, status: "approved" }));
                                    setLiveSteps((prev) =>
                                      prev.map((st, idx) =>
                                        idx <= 2 ? { ...st, status: "done" } : idx === 3 ? { ...st, status: "in_progress" } : st,
                                      ),
                                    );
                                    flashToast("已批准实施产物文档并推进后续步骤");
                                  }}
                                >
                                  <Check className="h-3.5 w-3.5" strokeWidth={2} />
                                  <span>{t("artifact.approve")}</span>
                                </Button>
                              )}
                            </div>
                          </div>

                          {/* 检查点锚点 */}
                          <div className="group/cp flex items-center gap-2.5 my-1">
                            <div className="flex-1 h-px bg-[var(--soxu-border-hairline)]" />
                            <button
                              type="button"
                              onClick={() => setModal("rewind")}
                              className="h-[26px] px-3 rounded-full bg-[var(--soxu-surface-card-muted)] border border-[var(--soxu-border-subtle)] flex items-center gap-2 text-[11.5px] soxu-mono text-[var(--soxu-text-secondary)] hover:border-[var(--soxu-border-strong)] cursor-pointer transition-colors"
                            >
                              <GitCommit className="h-3.5 w-3.5 text-[var(--soxu-semantic-handoff)]" strokeWidth={1.75} />
                              <span>检查点 #04 · 7c4e91a · 2 个文件 (+28 -3)</span>
                              <span className="inline-flex items-center gap-1 opacity-65 group-hover/cp:opacity-100 text-[var(--soxu-semantic-handoff)]">
                                <RotateCcw className="h-3 w-3" strokeWidth={1.75} />
                                <span>回退</span>
                              </span>
                            </button>
                            <div className="flex-1 h-px bg-[var(--soxu-border-hairline)]" />
                          </div>
                        </div>

                        {/* 第 3 轮：结构化提问卡与权限审批卡（全部移除左侧加宽颜色条，改用四边等宽细描边） */}
                        <div id="turn-3" className="flex flex-col gap-3">
                          <div className="rounded-xl border border-[var(--soxu-semantic-running-border)] bg-[var(--soxu-surface-card)] p-4 flex flex-col gap-2.5">
                            <div className="flex items-center justify-between">
                              <strong className="text-[13px]">结构化确认提问</strong>
                              <Badge variant="running">等待确认</Badge>
                            </div>
                            <p className="text-xs text-[var(--soxu-text-secondary)] m-0">{ask.prompt}</p>
                            <div className="flex flex-col gap-1.5">
                              {ask.options.map((o) => {
                                const active = ask.selectedId === o.id;
                                return (
                                  <button
                                    key={o.id}
                                    type="button"
                                    onClick={() => setAsk((a) => ({ ...a, selectedId: o.id }))}
                                    className={cn(
                                      "w-full rounded-lg border px-3 py-2 text-left text-xs flex items-center justify-between cursor-pointer transition-colors",
                                      active
                                        ? "border-[var(--soxu-border-strong)] bg-[var(--soxu-interaction-selected)]"
                                        : "border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card-muted)] hover:bg-[var(--soxu-interaction-hover)]",
                                    )}
                                  >
                                    <span>{o.label}</span>
                                    {o.recommended && <Badge variant="accent">推荐</Badge>}
                                  </button>
                                );
                              })}
                            </div>
                            <div className="flex justify-end">
                              <Button
                                size="sm"
                                variant={ask.submitted ? "secondary" : "default"}
                                onClick={() => {
                                  setAsk((a) => ({ ...a, submitted: true }));
                                  flashToast("已提交结构化问题回答");
                                }}
                              >
                                {ask.submitted ? "已确认提交" : "确认提交回答"}
                              </Button>
                            </div>
                          </div>

                          {approvals.map((apr) => {
                            if (apr.resolution) {
                              return (
                                <div
                                  key={apr.id}
                                  className="rounded-xl border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card-muted)] px-3.5 py-2.5 text-xs text-[var(--soxu-text-secondary)] flex items-center justify-between animate-in fade-in-0 duration-150"
                                >
                                  <span className="flex items-center gap-2">
                                    <Check className="h-3.5 w-3.5 text-[var(--soxu-semantic-accent)]" strokeWidth={2} />
                                    <span>
                                      {apr.toolKind} 已消解（
                                      {apr.resolution === "approved"
                                        ? "允许一次"
                                        : apr.resolution === "session"
                                          ? "本会话允许"
                                          : `已拒绝：${apr.note || "按建议调整"}`}
                                      ）
                                    </span>
                                  </span>
                                  <span className="text-[11px] text-[var(--soxu-text-tertiary)]">多端已同步</span>
                                </div>
                              );
                            }

                            return (
                              <div
                                key={apr.id}
                                className={cn(
                                  "rounded-xl border bg-[var(--soxu-surface-card)] p-4 flex flex-col gap-2.5",
                                  apr.highRisk
                                    ? "border-[var(--soxu-semantic-danger-border)]"
                                    : "border-[var(--soxu-semantic-warning-border)]",
                                )}
                              >
                                <div className="flex items-center justify-between">
                                  <span
                                    className="flex items-center gap-1.5 text-xs font-semibold"
                                    style={{
                                      color: apr.highRisk ? "var(--soxu-semantic-danger)" : "var(--soxu-semantic-warning)",
                                    }}
                                  >
                                    <Shield className="h-4 w-4" strokeWidth={1.75} />
                                    <span>{apr.toolKind}</span>
                                  </span>
                                  <Badge variant={apr.highRisk ? "danger" : "warning"}>
                                    {apr.highRisk ? "禁用自动放行" : "普通审批"}
                                  </Badge>
                                </div>
                                <div className="rounded-lg border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-terminal)] px-3 py-2 soxu-code-block">
                                  $ {apr.command}
                                </div>
                                <div className="text-xs text-[var(--soxu-text-secondary)]">{apr.riskNote}</div>

                                {rejectingId === apr.id ? (
                                  <div className="flex gap-2">
                                    <input
                                      autoFocus
                                      value={rejectNote}
                                      onChange={(e) => setRejectNote(e.target.value)}
                                      placeholder="输入修改指导（例如改用临时隔离目录并自动回收）"
                                      className="flex-1 rounded-lg border border-[var(--soxu-border-strong)] bg-[var(--soxu-surface-composer)] px-3 py-1.5 text-xs outline-none"
                                    />
                                    <Button
                                      variant="destructive"
                                      onClick={() => {
                                        setApprovals((p) =>
                                          p.map((x) => (x.id === apr.id ? { ...x, resolution: "rejected", note: rejectNote } : x)),
                                        );
                                        setRejectingId(null);
                                        setRejectNote("");
                                        flashToast("已发送拒绝指导并记录审计日志");
                                      }}
                                    >
                                      发送指导
                                    </Button>
                                  </div>
                                ) : (
                                  <div className="flex flex-wrap items-center justify-between gap-2">
                                    <div className="flex gap-1.5">
                                      <Button
                                        variant={apr.highRisk ? "destructive" : "secondary"}
                                        onClick={() => setRejectingId(apr.id)}
                                      >
                                        拒绝并指导
                                      </Button>
                                      <Button
                                        variant="secondary"
                                        disabled={apr.highRisk}
                                        onClick={() =>
                                          setApprovals((p) =>
                                            p.map((x) => (x.id === apr.id ? { ...x, resolution: "session" } : x)),
                                          )
                                        }
                                      >
                                        本会话允许
                                      </Button>
                                    </div>
                                    <Button
                                      variant={apr.highRisk ? "outline" : "default"}
                                      onClick={() =>
                                        setApprovals((p) =>
                                          p.map((x) => (x.id === apr.id ? { ...x, resolution: "approved" } : x)),
                                        )
                                      }
                                    >
                                      <Check className="h-3.5 w-3.5" strokeWidth={2} />
                                      <span>{apr.highRisk ? "确认仅执行本次" : "允许一次"}</span>
                                    </Button>
                                  </div>
                                )}
                              </div>
                            );
                          })}

                          {workspaceTurns.map((turnItem) =>
                            turnItem.role === "user" ? (
                              <div key={turnItem.id} className="flex flex-col items-end gap-1.5 animate-in fade-in-0 slide-in-from-bottom-2 duration-200">
                                {turnItem.badge && <Badge variant="running">{turnItem.badge}</Badge>}
                                {turnItem.chips && turnItem.chips.length > 0 && (
                                  <ComposerChipBar chips={turnItem.chips} className="justify-end" />
                                )}
                                <div className="max-w-[78%] rounded-[18px] bg-[var(--soxu-surface-raised)] px-4 py-2.5">
                                  {turnItem.text}
                                </div>
                              </div>
                            ) : (
                              <div key={turnItem.id} className="flex flex-col gap-1.5 animate-in fade-in-0 duration-200">
                                <div className="flex items-center gap-1.5 text-xs font-semibold text-[var(--soxu-text-secondary)]">
                                  <AgentIcon agent={turnItem.author} className="h-3.5 w-3.5 shrink-0" />
                                  <span>{turnItem.author}</span>
                                  {turnItem.badge && <Badge variant="handoff">{turnItem.badge}</Badge>}
                                </div>
                                <p className="m-0 leading-relaxed">{turnItem.text}</p>
                              </div>
                            ),
                          )}
                        </div>
                      </>
                    )}

                    {mainView === "session" && (
                      <QuoteSelectionPopover
                        containerRef={transcriptBodyRef}
                        isCoarsePointer={vp.isCoarsePointer}
                        onQuote={(snippet) => {
                          addChipToActive({
                            kind: "quote",
                            label: snippet.slice(0, 42) + (snippet.length > 42 ? "..." : ""),
                            detail: snippet,
                          });
                          window.setTimeout(() => composerInputRef.current?.focus(), 40);
                        }}
                        onCopyText={() => flashToast("已复制选区内容")}
                      />
                    )}
                  </div>
                </div>

                {/* 底部悬浮三态输入舱：与上方内容处于同一 px-3 sm:px-6 居中容器，宽度绝对对齐并随侧栏拖拽实时伸缩 */}
                {mainView === "session" && (
                  <div className="sticky bottom-0 z-10 px-3 sm:px-6 pb-3.5 pt-2 bg-gradient-to-t from-[var(--soxu-surface-canvas)] via-[var(--soxu-surface-canvas)] to-transparent">
                    <div className="w-full max-w-[var(--soxu-layout-composer-max-width)] mx-auto flex flex-col gap-1.5">
                      {lostControlToast && <div className="text-[11.5px] text-[var(--soxu-semantic-warning)] px-2">{lostControlToast}</div>}

                      {activeTab?.targetKind === "workspace" && (
                        <MultiQueueBar
                          items={queuedPrompts}
                          paused={queuePaused}
                          supportsLiveSteer={activeProfile.supportsLiveSteer}
                          isMac={vp.isMac}
                          onTogglePause={() => setQueuePaused((v) => !v)}
                          onPromoteSteer={handlePromoteSteerItem}
                          onEditItem={handleEditQueueItem}
                          onMoveItem={handleMoveQueueItem}
                          onRemoveItem={handleRemoveQueueItem}
                        />
                      )}

                      <div
                        className={cn(
                          "rounded-[22px] bg-[var(--soxu-surface-composer)] border p-2.5 flex flex-col gap-2 transition-colors duration-150",
                          composerMode === "steer"
                            ? "border-[var(--soxu-semantic-running-border)]"
                            : composerMode === "interrupt"
                              ? "border-[var(--soxu-semantic-danger-border)]"
                              : "border-[var(--soxu-border-subtle)] focus-within:border-[var(--soxu-border-strong)]",
                        )}
                      >
                        <ComposerChipBar chips={currentComposerChips} onRemove={removeChipFromActive} />

                        <textarea
                          ref={composerInputRef}
                          rows={2}
                          value={composerText}
                          onChange={(e) => setComposerText(e.target.value)}
                          onKeyDown={(e) => {
                            if (isImeComposing(e)) return;
                            if (e.key === "Escape" && queuedPrompts.length > 0) {
                              e.preventDefault();
                              setQueuedPrompts((prev) => prev.slice(0, -1));
                            } else if (e.key === "Enter" && (e.ctrlKey || e.metaKey) && e.shiftKey) {
                              e.preventDefault();
                              setComposerMode("queue");
                              sendPrompt("queue");
                            } else if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
                              e.preventDefault();
                              const targetMode: TurnBehavior = activeProfile.supportsLiveSteer ? "steer" : "queue";
                              setComposerMode(targetMode);
                              sendPrompt(targetMode);
                            } else if (e.key === "Enter" && e.altKey) {
                              e.preventDefault();
                              setComposerMode("interrupt");
                              sendPrompt("interrupt");
                            } else if (e.key === "Enter" && !e.shiftKey) {
                              e.preventDefault();
                              sendPrompt();
                            }
                          }}
                          placeholder={
                            activeTab?.targetKind === "quick"
                              ? t("composer.placeholderQuick")
                              : t("composer.placeholderWork")
                          }
                          className="w-full resize-none bg-transparent border-0 px-2 py-0.5 text-sm outline-none placeholder:text-[var(--soxu-text-ghost)]"
                        />

                        <div className="flex flex-wrap items-center justify-between gap-2 px-1">
                          {/* 左侧常驻：+ 更多操作 + 代理 + 模型 + 思考强度(支持时) + 常驻操作权限 + 可选代理移交 */}
                          <div className="flex flex-wrap items-center gap-1 min-w-0">
                            <ComposerPlusMenu
                              hasWorkspace={activeTab?.targetKind === "workspace"}
                              onAddChip={addChipToActive}
                            />

                            <DropdownMenu>
                              <DropdownMenuTrigger asChild>
                                <button
                                  type="button"
                                  className="inline-flex items-center gap-1.5 h-7 px-2.5 rounded-full bg-[var(--soxu-surface-control)] hover:bg-[var(--soxu-interaction-selected)] text-[11.5px] soxu-mono text-[var(--soxu-text-primary)] cursor-pointer transition-colors"
                                >
                                  <AgentIcon agent={cli} className="h-3.5 w-3.5 shrink-0" />
                                  <span>{cli}</span>
                                  <ChevronDown className="h-3 w-3 text-[var(--soxu-text-tertiary)]" strokeWidth={1.75} />
                                </button>
                              </DropdownMenuTrigger>
                              <DropdownMenuContent align="start">
                                <DropdownMenuLabel>原生代理驱动</DropdownMenuLabel>
                                <DropdownMenuSeparator />
                                {CLI_OPTIONS.filter((c) => c.available).map((c) => (
                                  <DropdownMenuItem key={c.id} onClick={() => handleSelectCli(c.id)}>
                                    <span className="inline-flex items-center gap-2">
                                      <AgentIcon agent={c.id} className="h-4 w-4 shrink-0" />
                                      <span className="soxu-mono">{c.id}</span>
                                    </span>
                                    <Badge variant="neutral">v{c.ver}</Badge>
                                  </DropdownMenuItem>
                                ))}
                              </DropdownMenuContent>
                            </DropdownMenu>

                            <DropdownMenu>
                              <DropdownMenuTrigger asChild>
                                <button
                                  type="button"
                                  className="inline-flex items-center gap-1 h-7 px-2.5 rounded-full bg-[var(--soxu-surface-control)] hover:bg-[var(--soxu-interaction-selected)] text-[11.5px] soxu-mono text-[var(--soxu-text-secondary)] max-w-[165px] cursor-pointer transition-colors"
                                >
                                  <span className="truncate">{model.replace("group/", "").replace("auto/", "")}</span>
                                  <ChevronDown className="h-3 w-3 shrink-0 text-[var(--soxu-text-tertiary)]" strokeWidth={1.75} />
                                </button>
                              </DropdownMenuTrigger>
                              <DropdownMenuContent align="start" className="w-64">
                                <DropdownMenuLabel>{activeProfile.displayName} 支持的模型</DropdownMenuLabel>
                                <DropdownMenuSeparator />
                                {availableModels.map((m) => (
                                  <DropdownMenuItem key={m.id} onClick={() => handleSelectModel(m.id)}>
                                    <span className="soxu-mono truncate">{m.id}</span>
                                    <span className="text-[10.5px] text-[var(--soxu-text-tertiary)]">{m.tier}</span>
                                  </DropdownMenuItem>
                                ))}
                              </DropdownMenuContent>
                            </DropdownMenu>

                            {activeProfile.supportsThinking && (
                              <DropdownMenu>
                                <DropdownMenuTrigger asChild>
                                  <button
                                    type="button"
                                    className="inline-flex items-center gap-1 h-7 px-2.5 rounded-full bg-[var(--soxu-surface-control)] hover:bg-[var(--soxu-interaction-selected)] text-[11.5px] text-[var(--soxu-text-secondary)] cursor-pointer transition-colors"
                                  >
                                    <Cpu className="h-3 w-3 text-[var(--soxu-text-tertiary)]" strokeWidth={1.75} />
                                    <span>{THINKING_LABELS[thinking] ?? thinking}</span>
                                    <ChevronDown className="h-3 w-3 text-[var(--soxu-text-tertiary)]" strokeWidth={1.75} />
                                  </button>
                                </DropdownMenuTrigger>
                                <DropdownMenuContent align="start">
                                  <DropdownMenuLabel>思考深度</DropdownMenuLabel>
                                  <DropdownMenuSeparator />
                                  {currentModelMeta.thinkingOptions.map((lv) => (
                                    <DropdownMenuItem key={lv} onClick={() => setThinking(lv)}>
                                      <span>{THINKING_LABELS[lv] ?? lv}</span>
                                      {thinking === lv && <Check className="h-3.5 w-3.5 text-[var(--soxu-semantic-accent)]" strokeWidth={2} />}
                                    </DropdownMenuItem>
                                  ))}
                                </DropdownMenuContent>
                              </DropdownMenu>
                            )}

                            {/* 常驻操作权限纯图标选择器（绝不折叠；图标与可选列表随当前指定 Agent 与权限档位区分） */}
                            <ComposerPermissionButton
                              agentId={cli}
                              permLevel={permLevel}
                              isMac={vp.isMac}
                              onSelectPerm={(nextId, label) => {
                                setPermLevel(nextId);
                                flashToast(`操作权限已调整为「${label}」`);
                              }}
                            />

                            <ComposerHandoffMenu
                              currentCli={cli}
                              currentModel={model}
                              currentThinking={thinking}
                              selectedHandoff={selectedHandoff}
                              onSelectHandoff={(chipDraft) => {
                                addChipToActive(chipDraft);
                                flashToast(`已配置移交至 ${chipDraft.label}`);
                              }}
                              onClearHandoff={() => {
                                if (handoffChip) {
                                  removeChipFromActive(handoffChip.id);
                                  flashToast("已取消移交配置");
                                }
                              }}
                            />
                          </div>

                          {/* 右侧：三色上下文圆环 + 会话统计胶囊 + 纯图标投递模式组 + 发送/停止按钮 */}
                          <div className="flex items-center gap-1.5 shrink-0 ml-auto">
                            <ContextUsageRing
                              model={model}
                              usedTokens={currentContextUsed}
                              onCompact={() => {
                                if (activeTab) {
                                  setContextTokens((p) => ({ ...p, [activeTab.id]: Math.round(currentContextUsed * 0.42) }));
                                }
                                flashToast("已完成上下文无损摘要压缩");
                              }}
                            />

                            <SessionStatsPopover stats={currentSessionStats} />

                            {activeTab?.targetKind === "workspace" && (
                              <ComposerTurnBehaviorGroup
                                mode={composerMode}
                                supportsLiveSteer={activeProfile.supportsLiveSteer}
                                isMac={vp.isMac}
                                onChange={setComposerMode}
                              />
                            )}

                            <Button
                              size="icon"
                              variant="default"
                              onClick={() => sendPrompt()}
                              aria-label={composerText.trim() || currentComposerChips.length > 0 ? t("composer.send") : t("composer.stop")}
                              className="transition-transform duration-150 active:scale-90"
                            >
                              {composerText.trim() || currentComposerChips.length > 0 ? (
                                <ArrowUp className="h-4 w-4 transition-transform duration-200" strokeWidth={2} />
                              ) : (
                                <Square className="h-3.5 w-3.5 fill-current" />
                              )}
                            </Button>
                          </div>
                        </div>
                      </div>
                    </div>
                  </div>
                )}
              </div>

              {/* 贴靠滚动区最右侧悬浮的轮次迷你导航轨，不占用水平对齐宽度 */}
              {mainView === "session" && activeTab?.targetKind === "workspace" && (
                <nav
                  aria-label="轮次导航轨"
                  className="absolute right-1.5 top-1/2 -translate-y-1/2 z-20 w-[18px] flex flex-col items-end gap-1.5 px-0.5 select-none"
                >
                  {RAIL_TURNS.map((rt) => (
                    <Tooltip key={rt.idx}>
                      <TooltipTrigger asChild>
                        <button
                          type="button"
                          onClick={() => scrollToTurnAnchor(rt.idx)}
                          aria-label={rt.title}
                          className={cn(
                            "h-[3px] rounded-full transition-all duration-150 cursor-pointer",
                            activeRail === rt.idx ? "w-3.5" : "w-2 hover:w-3.5",
                          )}
                          style={{
                            backgroundColor:
                              rt.kind === "warning"
                                ? "var(--soxu-semantic-warning)"
                                : rt.kind === "handoff"
                                  ? "var(--soxu-semantic-handoff)"
                                  : activeRail === rt.idx
                                    ? "var(--soxu-text-primary)"
                                    : "var(--soxu-border-strong)",
                          }}
                        />
                      </TooltipTrigger>
                      <TooltipContent side="left">{rt.title}</TooltipContent>
                    </Tooltip>
                  ))}
                </nav>
              )}
            </div>
          </main>

          {/* 宽屏右侧拖拽调宽分隔条与审阅栏（支持全宽最大化与键盘调整） */}
          {!isRightDrawer && (
            <>
              {rightOpen && !inspectorMaximized && (
                <div
                  role="separator"
                  tabIndex={0}
                  aria-orientation="vertical"
                  aria-label="调整右侧栏宽度"
                  aria-valuemin={280}
                  aria-valuemax={620}
                  aria-valuenow={rightWidth}
                  onPointerDown={(e) => {
                    e.preventDefault();
                    setResizingSide("right");
                  }}
                  onDoubleClick={() => updatePrefs({ rightInspectorWidth: DEFAULT_PREFERENCES.rightInspectorWidth })}
                  onKeyDown={(e) => {
                    if (e.key === "ArrowLeft") {
                      e.preventDefault();
                      const next = Math.min(620, rightWidth + 24);
                      setRightWidth(next);
                      updatePrefs({ rightInspectorWidth: next });
                    } else if (e.key === "ArrowRight") {
                      e.preventDefault();
                      const next = Math.max(280, rightWidth - 24);
                      setRightWidth(next);
                      updatePrefs({ rightInspectorWidth: next });
                    } else if (e.key === "Home") {
                      e.preventDefault();
                      setRightWidth(DEFAULT_PREFERENCES.rightInspectorWidth);
                      updatePrefs({ rightInspectorWidth: DEFAULT_PREFERENCES.rightInspectorWidth });
                    }
                  }}
                  title="左右拖动或方向键调整宽度，双击或 Home 恢复默认"
                  className="flex w-[9px] -mx-[4px] z-20 cursor-col-resize items-center justify-center group select-none shrink-0 focus-visible:outline-none"
                >
                  <div
                    className={cn(
                      "h-full w-px transition-colors duration-150",
                      resizingSide === "right"
                        ? "w-[2px] bg-[var(--soxu-semantic-running)]"
                        : "bg-[var(--soxu-border-hairline)] group-hover:w-[2px] group-hover:bg-[var(--soxu-semantic-running)] group-focus-visible:w-[2px] group-focus-visible:bg-[var(--soxu-semantic-accent)]",
                    )}
                  />
                </div>
              )}

              <aside
                aria-label={t("inspector.title")}
                style={{ width: inspectorMaximized && rightOpen ? undefined : rightOpen ? `${rightWidth}px` : "0px" }}
                className={cn(
                  "flex flex-col bg-[var(--soxu-surface-sidebar)] overflow-hidden",
                  inspectorMaximized && rightOpen ? "flex-1 min-w-0" : "shrink-0",
                  !resizingSide && "transition-[width,opacity] duration-[var(--soxu-motion-panel)] ease-[var(--soxu-ease-out)]",
                  rightOpen ? "opacity-100" : "opacity-0",
                )}
              >
                <div
                  style={{ width: inspectorMaximized && rightOpen ? undefined : `${rightWidth}px` }}
                  className="flex-1 min-h-0 flex flex-col w-full"
                >
                  {rightInspectorBody}
                </div>
              </aside>
            </>
          )}
        </div>

        {/* 窄屏左/右滑出抽屉（替代平铺分栏，保证不挤压中间对话流也不导致元素重叠） */}
        <Sheet open={isLeftDrawer && leftOpen} onOpenChange={setLeftOpen}>
          <SheetContent side="left" rawBody srTitle={t("sidebar.title")}>
            {leftSidebarBody}
          </SheetContent>
        </Sheet>

        <Sheet open={isRightDrawer && rightOpen} onOpenChange={setRightOpen}>
          <SheetContent side="right" rawBody srTitle={t("inspector.title")}>
            {rightInspectorBody}
          </SheetContent>
        </Sheet>

        {/* 底部状态栏 */}
        <footer className="h-[var(--soxu-size-statusbar-height)] min-h-[var(--soxu-size-statusbar-height)] bg-[var(--soxu-surface-sidebar)] border-t border-[var(--soxu-border-hairline)] px-2.5 sm:px-3 flex items-center justify-between gap-2 text-[11px] text-[var(--soxu-text-tertiary)] soxu-tabular">
          <div className="flex items-center gap-3 sm:gap-4 min-w-0">
            <span className="inline-flex items-center gap-1.5 soxu-mono truncate">
              <span className="h-1.5 w-1.5 rounded-full bg-[var(--soxu-semantic-accent)] shrink-0" />
              <span className="truncate">{t("footer.daemon", { addr: "127.0.0.1:7698", seq: 1056 })}</span>
            </span>
            <button
              type="button"
              onClick={() => {
                setSettingsSection("magpie");
                setSettingsOpen(true);
              }}
              className="hidden sm:inline-flex hover:text-[var(--soxu-text-primary)] cursor-pointer transition-colors shrink-0"
            >
              {magpieMode === "magpie_managed" ? "协同网关 127.0.0.1:3425 · 2ms" : t("sidebar.gatewayDirect")}
            </button>
          </div>
          <div className="flex items-center gap-3 shrink-0">
            {toast && <span className="hidden md:inline text-[var(--soxu-semantic-accent)] animate-in fade-in-0 duration-150">{toast}</span>}
            <button
              type="button"
              onClick={() => setModal("pair")}
              className="hover:text-[var(--soxu-text-primary)] cursor-pointer transition-colors"
            >
              {t("footer.devices", { count: 2 })}
            </button>
          </div>
        </footer>

        <UtilityModals
          modal={modal}
          onClose={() => setModal(null)}
          onOpenModal={(m) => setModal(m)}
          onOpenSettings={() => setSettingsOpen(true)}
          onToggleTheme={() => handleToggleTheme(null)}
          branch={activeWorkspace?.branch}
          isMac={vp.isMac}
          isCoarsePointer={vp.isCoarsePointer}
          schedules={schedules}
          onToggleSchedule={(id) =>
            setSchedules((prev) => prev.map((x) => (x.id === id ? { ...x, active: !x.active, unreadFail: false } : x)))
          }
          onFlashToast={flashToast}
        />

        <SettingsDialog
          open={settingsOpen}
          onOpenChange={setSettingsOpen}
          initialSection={settingsSection}
          prefs={prefs}
          onUpdatePrefs={updatePrefs}
          magpieMode={magpieMode}
          onToggleMagpie={() => setMagpieMode((m) => (m === "magpie_managed" ? "standalone_native" : "magpie_managed"))}
          onOpenOnboarding={() => setModal("onboarding")}
          onFlashToast={flashToast}
        />
      </div>
    </TooltipProvider>
  );
}
