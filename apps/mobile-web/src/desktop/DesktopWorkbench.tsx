import {
  Archive,
  ArchiveRestore,
  ArrowUp,
  AtSign,
  Check,
  ChevronDown,
  ChevronRight,
  Clock,
  Copy,
  Cpu,
  ExternalLink,
  FileCode2,
  Folder,
  FolderOpen,
  GitBranch,
  GitCommit,
  History,
  LayoutGrid,
  MessageSquare,
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
  Terminal,
  X,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { AgentIcon } from "@/components/AgentIcon";
import { CodeDiffView, CodeSnippetView, type DiffRowItem } from "@/components/CodeBlockView";
import { ContextUsageRing } from "@/components/ContextUsageRing";
import { HoverExpandDock } from "@/components/HoverExpandDock";
import { SettingsDialog, type SettingsSectionId } from "@/components/SettingsDialog";
import { ThemeToggle } from "@/components/ThemeToggle";
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
import { cn } from "@/lib/utils";
import {
  CLI_OPTIONS,
  INITIAL_APPROVALS,
  INITIAL_ASK,
  INITIAL_QUICK_CHATS,
  INITIAL_SCHEDULES,
  INITIAL_SCRIPTS,
  INITIAL_TURNS,
  INITIAL_WORKSPACES,
} from "@/mock-data";
import type {
  ApprovalItem,
  PermissionLevel,
  QuickChatItem,
  TurnBehavior,
  TurnMessage,
  WorkspaceBucket,
  WorkspaceItem,
} from "@/types";

const DIFF_ROWS: DiffRowItem[] = [
  { lineNum: 22, kind: "ctx", code: "pub enum NativeCliKind { Claude, Codex, Pi, OpenCode, Antigravity, Copilot, Qwen }" },
  { kind: "del", code: "// 旧版仅在运行时返回可执行文件名" },
  { lineNum: 23, kind: "add", code: "/// 返回驱动是否支持活跃回合中途热纠偏" },
  { lineNum: 24, kind: "add", code: "#[inline]" },
  { lineNum: 25, kind: "add", code: "pub const fn supports_live_steer(self) -> bool {" },
  { lineNum: 26, kind: "add", code: "  matches!(self, Self::Pi | Self::Codex | Self::Claude)" },
  { lineNum: 27, kind: "add", code: "}" },
];

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
  const [viewportWidth, setViewportWidth] = useState<number>(() =>
    typeof window !== "undefined" ? window.innerWidth : 1280,
  );
  const isLeftDrawer = viewportWidth < 900;
  const isRightDrawer = viewportWidth < 1120;

  const [leftOpen, setLeftOpen] = useState(() => (typeof window !== "undefined" ? window.innerWidth >= 900 : true));
  const [rightOpen, setRightOpen] = useState(() => (typeof window !== "undefined" ? window.innerWidth >= 1120 : true));
  const [leftWidth, setLeftWidth] = useState(prefs.leftSidebarWidth);
  const [rightWidth, setRightWidth] = useState(prefs.rightInspectorWidth);
  const [resizingSide, setResizingSide] = useState<"left" | "right" | null>(null);
  const [tabsScrolling, setTabsScrolling] = useState(false);
  const tabStripRef = useRef<HTMLDivElement>(null);
  const tabScrollTimer = useRef<number | null>(null);

  const [rightTab, setRightTab] = useState<"diff" | "files" | "scripts" | "terminal">("diff");
  const [diffLayout, setDiffLayout] = useState<"unified" | "split">("unified");
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

  const [reasoningOpen, setReasoningOpen] = useState(false);
  const [toolOpen, setToolOpen] = useState<Record<string, boolean>>({ read: false, edit: true, bash: true, mcp: false });
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
  const [queuedPrompt, setQueuedPrompt] = useState<string | null>(
    "测试全部通过后同步更新研发看板与认知索引",
  );
  const [lostControlToast, setLostControlToast] = useState<string | null>(null);
  const [contextTokens, setContextTokens] = useState<Record<string, number>>({
    "tab-wks-m2": 76400,
    "tab-chat-arch": 24800,
    "tab-wks-m1": 112000,
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
    const prevWidthRef = { leftWasDrawer: viewportWidth < 900, rightWasDrawer: viewportWidth < 1120 };
    const onResize = () => {
      const w = window.innerWidth;
      setViewportWidth(w);
      const nextLeftDrawer = w < 900;
      const nextRightDrawer = w < 1120;
      if (!prevWidthRef.leftWasDrawer && nextLeftDrawer) {
        setLeftOpen(false);
      }
      if (!prevWidthRef.rightWasDrawer && nextRightDrawer) {
        setRightOpen(false);
      }
      prevWidthRef.leftWasDrawer = nextLeftDrawer;
      prevWidthRef.rightWasDrawer = nextRightDrawer;
    };
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);

  useEffect(() => {
    return () => {
      if (tabScrollTimer.current !== null) {
        window.clearTimeout(tabScrollTimer.current);
      }
    };
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
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
  }, []);

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

  function handleSelectCli(nextCliId: string) {
    setCli(nextCliId);
    const reconciled = reconcileAgentCapabilities({
      agentId: nextCliId,
      currentModel: model,
      currentPerm: permLevel,
      currentThinking: thinking,
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

  function sendPrompt() {
    const t = composerText.trim();
    if (!t) {
      flashToast("已发送优雅中断信号停止当前输出");
      return;
    }

    if (!activeTab) {
      createQuickChat(t);
      setComposerText("");
      return;
    }

    if (activeTab.targetKind === "quick" && activeQuickChat) {
      const nextMsgUser: TurnMessage = { id: `qu-${Date.now()}`, role: "user", author: "你", text: t };
      const nextMsgAi: TurnMessage = {
        id: `qa-${Date.now() + 1}`,
        role: "assistant",
        author: `${cli} · ${model}`,
        text: `已在无项目对话中响应（${getPermissionMeta(cli, permLevel).label}）：收到你的指令「${t}」。`,
      };
      setQuickChats((prev) =>
        prev.map((q) =>
          q.id === activeQuickChat.id
            ? {
                ...q,
                title: q.messages.length === 0 ? t.slice(0, 18) : q.title,
                updatedAt: "刚刚",
                messages: [...q.messages, nextMsgUser, nextMsgAi],
              }
            : q,
        ),
      );
      setTabs((prev) =>
        prev.map((tb) =>
          tb.id === activeTab.id && tb.draft ? { ...tb, draft: false, title: t.slice(0, 18) } : tb,
        ),
      );
      setComposerText("");
      setContextTokens((prev) => ({
        ...prev,
        [activeTab.id]: (prev[activeTab.id] ?? 18000) + Math.max(1200, t.length * 40),
      }));
      return;
    }

    if (composerMode === "queue") {
      setQueuedPrompt(t);
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
        text: t,
        badge: composerMode === "steer" ? "热纠偏 · 下一步生效" : "打断重发",
      },
      {
        id: `a-${Date.now() + 1}`,
        role: "assistant",
        author: `${cli} · ${model}`,
        text: `已按「${getPermissionMeta(cli, permLevel).label}」策略执行当前指令，仅调整工作树内受控改动。`,
      },
    ]);
    setContextTokens((prev) => ({
      ...prev,
      [activeTab.id]: (prev[activeTab.id] ?? 52000) + Math.max(1800, t.length * 45),
    }));
    setComposerText("");
  }

  const rowHeightClass = prefs.density === "compact" ? "min-h-[32px]" : "min-h-[38px]";
  const activeProfile = getAgentProfile(cli);
  const availableModels = getAgentModels(cli);
  const currentModelMeta = availableModels.find((m) => m.id === model) ?? availableModels[0];
  const activePermMeta = getPermissionMeta(cli, permLevel);
  const currentContextUsed = activeTab ? (contextTokens[activeTab.id] ?? 42800) : 6400;

  const activeQuickChats = quickChats.filter((q) => !q.archived);
  const activeWorkspaces = workspaces.filter((w) => !w.archived);
  const archivedItems = [
    ...quickChats.filter((q) => q.archived).map((q) => ({ kind: "quick" as const, id: q.id, title: q.title, sub: q.updatedAt })),
    ...workspaces
      .filter((w) => w.archived)
      .map((w) => ({ kind: "workspace" as const, id: w.id, title: `${w.project} / ${w.branch}`, sub: w.diffStat })),
  ];

  // 按项目文件夹聚合工作树
  const projectGroups = Array.from(
    activeWorkspaces.reduce((map, w) => {
      const list = map.get(w.project) ?? { path: w.projectPath, items: [] as WorkspaceItem[] };
      list.items.push(w);
      map.set(w.project, list);
      return map;
    }, new Map<string, { path: string; items: WorkspaceItem[] }>()),
  );

  // 左侧导航正文（宽屏平铺，窄屏自动作为左滑出抽屉内容复用）
  const leftSidebarBody = (
    <div className="flex-1 min-h-0 flex flex-col bg-[var(--soxu-surface-sidebar)] overflow-hidden">
      <div className="flex-1 overflow-y-auto p-2.5 flex flex-col gap-4 soxu-scroll">
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
                  className="h-5 w-5 rounded flex items-center justify-center text-[var(--soxu-text-tertiary)] hover:bg-[var(--soxu-interaction-hover)] hover:text-[var(--soxu-text-primary)] cursor-pointer"
                  aria-label={t("sidebar.newQuickChat")}
                >
                  <Plus className="h-3.5 w-3.5" strokeWidth={1.75} />
                </button>
              </TooltipTrigger>
              <TooltipContent>{t("sidebar.newQuickChat")}</TooltipContent>
            </Tooltip>
          </div>

          {activeQuickChats.map((qc) => {
            const selected = activeTab?.targetKind === "quick" && activeTab.targetId === qc.id;
            return (
              <div
                key={qc.id}
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
                  className="opacity-0 group-hover:opacity-100 h-5 w-5 rounded flex items-center justify-center text-[var(--soxu-text-tertiary)] hover:text-[var(--soxu-text-primary)] hover:bg-[var(--soxu-surface-control)] shrink-0 cursor-pointer transition-opacity"
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

          {projectGroups.map(([projName, group]) => {
            const isExpanded = openProjects[projName] ?? true;
            return (
              <div key={projName} className="flex flex-col gap-0.5">
                <div className="group flex items-center justify-between rounded-lg px-1.5 py-1 hover:bg-[var(--soxu-interaction-hover)] transition-colors">
                  <button
                    type="button"
                    onClick={() => setOpenProjects((p) => ({ ...p, [projName]: !isExpanded }))}
                    className="flex-1 min-w-0 flex items-center gap-1.5 text-left text-xs font-semibold text-[var(--soxu-text-primary)] cursor-pointer"
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
                        className="opacity-0 group-hover:opacity-100 h-5 w-5 rounded flex items-center justify-center text-[var(--soxu-text-tertiary)] hover:text-[var(--soxu-text-primary)] hover:bg-[var(--soxu-surface-control)] cursor-pointer transition-opacity shrink-0"
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
                      const selected = activeTab?.targetKind === "workspace" && activeTab.targetId === w.id;
                      const b = bucketLabel(w.bucket);
                      return (
                        <div
                          key={w.id}
                          role="button"
                          tabIndex={0}
                          onClick={() => openWorkspaceTab(w)}
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
                              {w.isolation === "worktree" ? (
                                <GitBranch className="h-3 w-3 shrink-0 text-[var(--soxu-text-tertiary)]" strokeWidth={1.75} />
                              ) : (
                                <Folder className="h-3 w-3 shrink-0 text-[var(--soxu-text-tertiary)]" strokeWidth={1.75} />
                              )}
                              <span className="soxu-mono font-medium truncate">{w.branch}</span>
                            </div>
                            <div className="pl-3.5 text-[10.5px] soxu-tabular text-[var(--soxu-text-tertiary)] flex items-center gap-1.5">
                              <span>{w.isolation === "worktree" ? t("sidebar.worktree") : t("sidebar.localRoot")}</span>
                              <span className="inline-flex items-center gap-1 soxu-mono">
                                <AgentIcon agent={w.agent} className="h-3 w-3 shrink-0" />
                                <span>{w.agent}</span>
                              </span>
                              <span>{w.diffStat}</span>
                            </div>
                          </div>
                          <div className="flex items-center gap-1 shrink-0">
                            {w.bucket !== "done" && <Badge variant={b.variant}>{t(b.labelKey)}</Badge>}
                            <button
                              type="button"
                              title={t("sidebar.archiveAction")}
                              aria-label={`${t("sidebar.archiveAction")} ${w.branch}`}
                              onClick={(e) => {
                                e.stopPropagation();
                                archiveTarget("workspace", w.id, `${w.project} / ${w.branch}`);
                              }}
                              className="opacity-0 group-hover:opacity-100 h-5 w-5 rounded flex items-center justify-center text-[var(--soxu-text-tertiary)] hover:text-[var(--soxu-text-primary)] hover:bg-[var(--soxu-surface-control)] cursor-pointer transition-opacity"
                            >
                              <Archive className="h-3 w-3" strokeWidth={1.75} />
                            </button>
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

  // 右侧上下文与代码审阅正文（宽屏平铺，窄屏自动作为右滑出抽屉内容复用）
  const rightInspectorBody = (
    <div className="flex-1 min-h-0 flex flex-col bg-[var(--soxu-surface-sidebar)] overflow-hidden">
      <div className="h-[38px] px-2.5 border-b border-[var(--soxu-border-hairline)] flex items-center justify-between gap-2 shrink-0">
        <Tabs value={rightTab} onValueChange={(v) => setRightTab(v as typeof rightTab)}>
          <TabsList>
            <TabsTrigger value="diff">{t("inspector.diff")}</TabsTrigger>
            <TabsTrigger value="files">{t("inspector.files")}</TabsTrigger>
            <TabsTrigger value="scripts">{t("inspector.scripts")}</TabsTrigger>
            <TabsTrigger value="terminal">{t("inspector.terminal")}</TabsTrigger>
          </TabsList>
        </Tabs>
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

      <div className="flex-1 overflow-y-auto p-3 flex flex-col gap-3 soxu-scroll">
        {activeTab?.targetKind !== "workspace" && rightTab !== "terminal" ? (
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
                <div className="flex items-center justify-between gap-2">
                  <div className="inline-flex rounded-lg bg-[var(--soxu-surface-inset)] p-0.5 border border-[var(--soxu-border-hairline)]">
                    {(["unified", "split"] as const).map((m) => (
                      <button
                        key={m}
                        type="button"
                        onClick={() => setDiffLayout(m)}
                        className={cn(
                          "px-2.5 py-1 rounded-md text-[11px] cursor-pointer",
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

                <div className="text-[11px] soxu-mono text-[var(--soxu-text-tertiary)] truncate">
                  crates/soxu-drivers/src/lib.rs · {t("inspector.baseBranchLabel", { branch: prefs.defaultBranch })}
                </div>

                <CodeDiffView
                  rows={DIFF_ROWS}
                  layout={diffLayout}
                  showLineNumbers={prefs.showLineNumbers}
                  wrap={prefs.codeWordWrap}
                />
              </>
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
        {/* 顶部标题栏：窄屏分轨防重叠；宽屏三栏对称 Grid 保证搜索框恒定水平居中 */}
        <header className="h-[var(--soxu-size-titlebar-height)] min-h-[var(--soxu-size-titlebar-height)] bg-[var(--soxu-surface-sidebar)] border-b border-[var(--soxu-border-hairline)] px-2 sm:px-3 grid grid-cols-[auto_minmax(0,1fr)_auto] sm:grid-cols-[minmax(max-content,1fr)_minmax(150px,360px)_minmax(max-content,1fr)] items-center gap-1.5 sm:gap-3 select-none z-[var(--soxu-z-header)]">
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
              <TooltipContent>{isLeftDrawer ? t("header.openLeftDrawer") : `${t("header.toggleLeft")} (Ctrl+B)`}</TooltipContent>
            </Tooltip>

            <span className="hidden lg:inline-flex items-center gap-1.5 text-xs text-[var(--soxu-text-tertiary)] truncate min-w-0">
              {activeTab?.targetKind === "quick" ? (
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
              Ctrl+K
            </kbd>
          </button>

          {/* 右侧极简工具区：无角标悬停收纳（窄屏悬浮弹出）+ 纯图标主题切换 + 设置齿轮 + 右栏/抽屉开关 */}
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
              <TooltipContent>{t("header.settings")} (Ctrl+,)</TooltipContent>
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
              <TooltipContent>{isRightDrawer ? t("header.openRightDrawer") : `${t("header.toggleRight")} (Ctrl+J)`}</TooltipContent>
            </Tooltip>
          </div>
        </header>

        {/* 主四区网格：宽屏左右平铺支持拖拽调宽；窄屏左右栏自动收进抽屉 */}
        <div className="flex-1 min-h-0 flex overflow-hidden">
          {!isLeftDrawer && (
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
                  aria-orientation="vertical"
                  aria-label="调整左侧栏宽度"
                  onPointerDown={(e) => {
                    e.preventDefault();
                    setResizingSide("left");
                  }}
                  onDoubleClick={() => updatePrefs({ leftSidebarWidth: DEFAULT_PREFERENCES.leftSidebarWidth })}
                  title="左右拖动调整宽度，双击恢复默认宽度"
                  className="flex w-[9px] -mx-[4px] z-20 cursor-col-resize items-center justify-center group select-none shrink-0"
                >
                  <div
                    className={cn(
                      "h-full w-px transition-colors duration-150",
                      resizingSide === "left"
                        ? "w-[2px] bg-[var(--soxu-semantic-running)]"
                        : "bg-[var(--soxu-border-hairline)] group-hover:w-[2px] group-hover:bg-[var(--soxu-semantic-running)]",
                    )}
                  />
                </div>
              )}
            </>
          )}

          {/* 中央多代理会话画布 */}
          <main className="flex-1 min-w-0 flex flex-col bg-[var(--soxu-surface-canvas)] overflow-hidden relative">
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
                  const active = activeTabId === tb.id;
                  return (
                    <div
                      key={tb.id}
                      role="tab"
                      tabIndex={0}
                      aria-selected={active}
                      onClick={() => setActiveTabId(tb.id)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter" || e.key === " ") {
                          e.preventDefault();
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
                          className="opacity-0 group-hover:opacity-100 h-4 w-4 rounded flex items-center justify-center text-[var(--soxu-text-tertiary)] hover:text-[var(--soxu-text-primary)] hover:bg-[var(--soxu-surface-control)] cursor-pointer transition-opacity"
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
                          className="opacity-0 group-hover:opacity-100 group-aria-selected:opacity-75 hover:!opacity-100 h-4 w-4 rounded flex items-center justify-center text-[var(--soxu-text-tertiary)] hover:text-[var(--soxu-text-primary)] hover:bg-[var(--soxu-surface-control)] cursor-pointer transition-opacity"
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
            {activeTab?.targetKind === "workspace" && (
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
              <div className="flex-1 overflow-y-auto soxu-scroll flex flex-col justify-between">
                <div className="px-3 sm:px-6 pt-5 pb-6 flex-1">
                  <div className="w-full max-w-[var(--soxu-layout-transcript-max-width)] mx-auto flex flex-col gap-5">
                    {!activeTab ? (
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
                              <div key={m.id} className="flex justify-end">
                                <div className="max-w-[78%] rounded-[18px] bg-[var(--soxu-surface-raised)] px-4 py-2.5 leading-relaxed">
                                  {m.text}
                                </div>
                              </div>
                            ) : (
                              <div key={m.id} className="flex flex-col gap-2">
                                <div className="flex items-center gap-2 text-xs font-semibold">
                                  <AgentIcon agent={m.author} className="h-4 w-4 shrink-0" />
                                  <span>{m.author}</span>
                                </div>
                                <p className="m-0 leading-relaxed text-[var(--soxu-text-primary)]">{m.text}</p>
                              </div>
                            ),
                          )
                        )}
                      </>
                    ) : (
                      <>
                        {/* 工作区绑定回合流 */}
                        <div id="turn-1" className="flex justify-end">
                          <div className="max-w-[78%] rounded-[18px] bg-[var(--soxu-surface-raised)] px-4 py-2.5 leading-relaxed">
                            {INITIAL_TURNS[0].text}
                          </div>
                        </div>

                        <div id="turn-2" className="group/msg flex flex-col gap-2.5">
                          <div className="flex items-center justify-between gap-2 text-xs">
                            <div className="flex items-center gap-2 font-semibold">
                              <AgentIcon agent={cli} className="h-4 w-4 shrink-0" />
                              <span>{cli}</span>
                              <span className="soxu-mono font-normal text-[var(--soxu-text-tertiary)]">
                                {model} · {THINKING_LABELS[thinking] ?? thinking}
                              </span>
                            </div>
                            <div className="flex items-center gap-1 opacity-0 group-hover/msg:opacity-100 focus-within:opacity-100 transition-opacity duration-150">
                              <Button
                                variant="ghost"
                                size="sm"
                                className="h-6 px-2 text-[11px]"
                                onClick={() => flashToast("已复制当前回合答复内容")}
                              >
                                <Copy className="h-3 w-3" strokeWidth={1.75} />
                                <span>复制</span>
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

                          {/* 工具卡 1：文件探索 */}
                          <Collapsible
                            open={toolOpen.read}
                            onOpenChange={(o) => setToolOpen((p) => ({ ...p, read: o }))}
                            className="rounded-xl border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card)] overflow-hidden"
                          >
                            <CollapsibleTrigger asChild>
                              <button
                                type="button"
                                className="w-full h-[34px] px-3 bg-[var(--soxu-surface-card-muted)] flex items-center justify-between text-xs text-[var(--soxu-text-secondary)] hover:text-[var(--soxu-text-primary)] cursor-pointer"
                              >
                                <span className="flex items-center gap-2 truncate">
                                  <Search className="h-3.5 w-3.5 shrink-0" strokeWidth={1.75} />
                                  <span className="truncate">已检索并读取 6 个核心源码文件 · 0.4 秒</span>
                                </span>
                                <ChevronRight
                                  className={cn("h-3.5 w-3.5 transition-transform duration-150", toolOpen.read && "rotate-90")}
                                  strokeWidth={1.75}
                                />
                              </button>
                            </CollapsibleTrigger>
                            <CollapsibleContent className="px-3 py-2 border-t border-[var(--soxu-border-hairline)] soxu-code-block text-[var(--soxu-text-secondary)]">
                              crates/soxu-drivers/src/lib.rs · crates/soxu-protocol/src/lib.rs · AGENTS.md
                            </CollapsibleContent>
                          </Collapsible>

                          {/* 工具卡 2：内联代码差异（右对齐唯一连续行号，无重复） */}
                          <Collapsible
                            open={toolOpen.edit}
                            onOpenChange={(o) => setToolOpen((p) => ({ ...p, edit: o }))}
                            className="rounded-xl border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card)] overflow-hidden"
                          >
                            <CollapsibleTrigger asChild>
                              <button
                                type="button"
                                className="w-full h-[34px] px-3 bg-[var(--soxu-surface-card-muted)] flex items-center justify-between text-xs text-[var(--soxu-text-secondary)] hover:text-[var(--soxu-text-primary)] cursor-pointer"
                              >
                                <span className="flex items-center gap-2 soxu-mono truncate">
                                  <FileCode2 className="h-3.5 w-3.5 shrink-0" strokeWidth={1.75} />
                                  <span className="truncate">修改 crates/soxu-drivers/src/lib.rs (+5 -1)</span>
                                </span>
                                <Badge variant="accent">已应用</Badge>
                              </button>
                            </CollapsibleTrigger>
                            <CollapsibleContent className="p-2 border-t border-[var(--soxu-border-hairline)]">
                              <CodeDiffView
                                rows={DIFF_ROWS}
                                layout="unified"
                                showLineNumbers={prefs.showLineNumbers}
                                wrap={prefs.codeWordWrap}
                              />
                            </CollapsibleContent>
                          </Collapsible>

                          {/* 工具卡 3：受控终端输出 */}
                          <Collapsible
                            open={toolOpen.bash}
                            onOpenChange={(o) => setToolOpen((p) => ({ ...p, bash: o }))}
                            className="rounded-xl border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card)] overflow-hidden"
                          >
                            <CollapsibleTrigger asChild>
                              <button
                                type="button"
                                className="w-full h-[34px] px-3 bg-[var(--soxu-surface-card-muted)] flex items-center justify-between text-xs text-[var(--soxu-text-secondary)] hover:text-[var(--soxu-text-primary)] cursor-pointer"
                              >
                                <span className="flex items-center gap-2 soxu-mono truncate">
                                  <Terminal className="h-3.5 w-3.5 shrink-0" strokeWidth={1.75} />
                                  <span className="truncate">cargo test -p soxu-drivers · 退出码 0 · 1.8 秒</span>
                                </span>
                                <Badge variant="accent">6 项通过</Badge>
                              </button>
                            </CollapsibleTrigger>
                            <CollapsibleContent className="p-2 border-t border-[var(--soxu-border-hairline)]">
                              <div className="rounded-lg border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-terminal)] p-2.5 soxu-code-block leading-relaxed">
                                <div style={{ color: "var(--soxu-ansi-bright-black)" }}>$ cargo test -p soxu-drivers</div>
                                <div>
                                  <span style={{ color: "var(--soxu-ansi-green)" }}>Compiling</span> soxu-drivers v0.1.0
                                </div>
                                <div>
                                  <span style={{ color: "var(--soxu-ansi-cyan)" }}>test</span> tests::steer_capability_matrix ...{" "}
                                  <span style={{ color: "var(--soxu-ansi-bright-green)" }}>ok</span>
                                </div>
                              </div>
                            </CollapsibleContent>
                          </Collapsible>

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

                          {workspaceTurns.map((t) =>
                            t.role === "user" ? (
                              <div key={t.id} className="flex flex-col items-end gap-1 animate-in fade-in-0 slide-in-from-bottom-2 duration-200">
                                {t.badge && <Badge variant="running">{t.badge}</Badge>}
                                <div className="max-w-[78%] rounded-[18px] bg-[var(--soxu-surface-raised)] px-4 py-2.5">
                                  {t.text}
                                </div>
                              </div>
                            ) : (
                              <div key={t.id} className="flex flex-col gap-1.5 animate-in fade-in-0 duration-200">
                                <div className="flex items-center gap-1.5 text-xs font-semibold text-[var(--soxu-text-secondary)]">
                                  <AgentIcon agent={t.author} className="h-3.5 w-3.5 shrink-0" />
                                  <span>{t.author}</span>
                                </div>
                                <p className="m-0 leading-relaxed">{t.text}</p>
                              </div>
                            ),
                          )}
                        </div>
                      </>
                    )}
                  </div>
                </div>

                {/* 底部悬浮三态输入舱：与上方内容处于同一 px-3 sm:px-6 居中容器，宽度绝对对齐并随侧栏拖拽实时伸缩 */}
                <div className="sticky bottom-0 z-10 px-3 sm:px-6 pb-3.5 pt-2 bg-gradient-to-t from-[var(--soxu-surface-canvas)] via-[var(--soxu-surface-canvas)] to-transparent">
                  <div className="w-full max-w-[var(--soxu-layout-composer-max-width)] mx-auto flex flex-col gap-1.5">
                    {lostControlToast && <div className="text-[11.5px] text-[var(--soxu-semantic-warning)] px-2">{lostControlToast}</div>}

                    {queuedPrompt && activeTab?.targetKind === "workspace" && (
                      <div className="rounded-xl border border-[var(--soxu-semantic-warning-border)] bg-[var(--soxu-surface-card)] px-3 py-1.5 flex items-center justify-between gap-2 text-xs">
                        <span className="soxu-mono truncate text-[var(--soxu-text-secondary)]">排队中：{queuedPrompt}</span>
                        <div className="flex items-center gap-1 shrink-0">
                          <Button
                            size="sm"
                            variant="ghost"
                            className="h-6 px-2 text-[var(--soxu-semantic-running)]"
                            onClick={() => {
                              setWorkspaceTurns((p) => [
                                ...p,
                                { id: `q-${Date.now()}`, role: "user", author: "你", text: queuedPrompt, badge: "提级为热纠偏" },
                              ]);
                              setQueuedPrompt(null);
                            }}
                          >
                            提级纠偏
                          </Button>
                          <Button size="icon-sm" variant="ghost" className="h-6 w-6" onClick={() => setQueuedPrompt(null)} aria-label="撤回排队">
                            <X className="h-3 w-3" strokeWidth={1.75} />
                          </Button>
                        </div>
                      </div>
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
                      {activeTab?.targetKind === "workspace" && (
                        <div className="flex items-center gap-1.5 px-1">
                          <span className="inline-flex items-center gap-1 rounded-md border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card)] px-2 py-0.5 text-[11px] soxu-mono text-[var(--soxu-text-secondary)]">
                            <FileCode2 className="h-3 w-3" strokeWidth={1.75} />
                            <span>crates/soxu-drivers/src/lib.rs:22-27</span>
                          </span>
                        </div>
                      )}

                      <textarea
                        rows={2}
                        value={composerText}
                        onChange={(e) => setComposerText(e.target.value)}
                        onKeyDown={(e) => {
                          if (e.key === "Enter" && !e.shiftKey) {
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
                        {/* 左侧常驻：代理 + 模型 + 思考强度(支持时) + 常驻操作权限（绝不折叠，随代理动态适配） */}
                        <div className="flex flex-wrap items-center gap-1 min-w-0">
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

                          {/* 常驻操作权限选择器（参考 Agy 桌面端，绝不折叠；选项随当前指定 Agent 自动调整） */}
                          <DropdownMenu>
                            <DropdownMenuTrigger asChild>
                              <button
                                type="button"
                                className="inline-flex items-center gap-1.5 h-7 px-2.5 rounded-full border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card)] hover:border-[var(--soxu-border-strong)] text-[11.5px] font-medium text-[var(--soxu-text-primary)] cursor-pointer transition-colors"
                                title={activePermMeta.desc}
                              >
                                <Shield className="h-3 w-3 text-[var(--soxu-semantic-accent)]" strokeWidth={1.75} />
                                <span>{activePermMeta.label}</span>
                                <ChevronDown className="h-3 w-3 text-[var(--soxu-text-tertiary)]" strokeWidth={1.75} />
                              </button>
                            </DropdownMenuTrigger>
                            <DropdownMenuContent align="start" className="w-72">
                              <DropdownMenuLabel>{activeProfile.displayName} 支持的操作权限档位</DropdownMenuLabel>
                              <DropdownMenuSeparator />
                              {activeProfile.permissions.map((pm) => (
                                <DropdownMenuItem
                                  key={pm.id}
                                  onClick={() => {
                                    setPermLevel(pm.id);
                                    flashToast(`操作权限已调整为「${pm.label}」`);
                                  }}
                                  className="flex-col items-start gap-0.5 py-2"
                                >
                                  <div className="w-full flex items-center justify-between">
                                    <span className="font-semibold text-xs">{pm.label}</span>
                                    {permLevel === pm.id && <Check className="h-3.5 w-3.5 text-[var(--soxu-semantic-accent)]" strokeWidth={2} />}
                                  </div>
                                  <span className="text-[11px] text-[var(--soxu-text-tertiary)]">{pm.desc}</span>
                                </DropdownMenuItem>
                              ))}
                            </DropdownMenuContent>
                          </DropdownMenu>

                          <Button
                            variant="ghost"
                            size="sm"
                            className="h-7 px-2 text-[11px] text-[var(--soxu-semantic-handoff)] rounded-full"
                            onClick={() => setComposerText((cur) => `@codex ${cur}`)}
                            title="移交上下文至其他原生代理"
                          >
                            <AtSign className="h-3 w-3" strokeWidth={1.75} />
                            <span>{t("composer.handoff")}</span>
                          </Button>
                        </div>

                        {/* 右侧：上下文百分比小圆环 + 投递模式 + 发送/停止按钮 */}
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

                          {activeTab?.targetKind === "workspace" && (
                            <Button
                              size="sm"
                              variant="outline"
                              className="h-7 rounded-full text-[11.5px] px-2.5"
                              onClick={() => {
                                const order: TurnBehavior[] = activeProfile.supportsLiveSteer
                                  ? ["steer", "queue", "interrupt"]
                                  : ["queue", "interrupt"];
                                const cur = order.includes(composerMode) ? order.indexOf(composerMode) : 0;
                                setComposerMode(order[(cur + 1) % order.length]);
                              }}
                            >
                              {composerMode === "steer"
                                ? t("composer.modeSteer")
                                : composerMode === "queue"
                                  ? t("composer.modeQueue")
                                  : t("composer.modeInterrupt")}
                            </Button>
                          )}

                          <Button
                            size="icon"
                            variant="default"
                            onClick={sendPrompt}
                            aria-label={composerText.trim() ? t("composer.send") : t("composer.stop")}
                            className="transition-transform duration-150 active:scale-90"
                          >
                            {composerText.trim() ? (
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
              </div>

              {/* 贴靠滚动区最右侧悬浮的轮次迷你导航轨，不占用水平对齐宽度 */}
              {activeTab?.targetKind === "workspace" && (
                <nav
                  aria-label="轮次导航轨"
                  className="absolute right-1.5 top-1/2 -translate-y-1/2 z-20 w-[18px] flex flex-col items-end gap-1.5 px-0.5 select-none"
                >
                  {RAIL_TURNS.map((rt) => (
                    <Tooltip key={rt.idx}>
                      <TooltipTrigger asChild>
                        <button
                          type="button"
                          onClick={() => {
                            setActiveRail(rt.idx);
                            document.getElementById(`turn-${rt.idx}`)?.scrollIntoView({ behavior: "smooth" });
                          }}
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

          {/* 宽屏右侧拖拽调宽分隔条与审阅栏 */}
          {!isRightDrawer && (
            <>
              {rightOpen && (
                <div
                  role="separator"
                  aria-orientation="vertical"
                  aria-label="调整右侧栏宽度"
                  onPointerDown={(e) => {
                    e.preventDefault();
                    setResizingSide("right");
                  }}
                  onDoubleClick={() => updatePrefs({ rightInspectorWidth: DEFAULT_PREFERENCES.rightInspectorWidth })}
                  title="左右拖动调整宽度，双击恢复默认宽度"
                  className="flex w-[9px] -mx-[4px] z-20 cursor-col-resize items-center justify-center group select-none shrink-0"
                >
                  <div
                    className={cn(
                      "h-full w-px transition-colors duration-150",
                      resizingSide === "right"
                        ? "w-[2px] bg-[var(--soxu-semantic-running)]"
                        : "bg-[var(--soxu-border-hairline)] group-hover:w-[2px] group-hover:bg-[var(--soxu-semantic-running)]",
                    )}
                  />
                </div>
              )}

              <aside
                aria-label={t("inspector.title")}
                style={{ width: rightOpen ? `${rightWidth}px` : "0px" }}
                className={cn(
                  "flex flex-col bg-[var(--soxu-surface-sidebar)] overflow-hidden shrink-0",
                  !resizingSide && "transition-[width,opacity] duration-[var(--soxu-motion-panel)] ease-[var(--soxu-ease-out)]",
                  rightOpen ? "opacity-100" : "opacity-0",
                )}
              >
                <div style={{ width: `${rightWidth}px` }} className="flex-1 min-h-0 flex flex-col">
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
