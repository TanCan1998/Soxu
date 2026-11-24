import {
  ArrowUp,
  AtSign,
  Bot,
  Check,
  ChevronRight,
  Clock,
  Copy,
  Cpu,
  ExternalLink,
  FileCode2,
  Folder,
  GitBranch,
  GitCommit,
  History,
  LayoutGrid,
  Network,
  PanelLeft,
  PanelRight,
  Plus,
  QrCode,
  RotateCcw,
  Search,
  Settings,
  Shield,
  SlidersHorizontal,
  Square,
  Terminal,
  X,
} from "lucide-react";
import { useEffect, useState } from "react";
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
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip";
import {
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
  INITIAL_SCHEDULES,
  INITIAL_SCRIPTS,
  INITIAL_WORKSPACES,
  MODEL_OPTIONS,
} from "@/mock-data";
import type { ApprovalItem, PermissionLevel, TurnBehavior } from "@/types";

const DIFF_ROWS = [
  { oldN: "22", newN: "22", kind: "ctx", code: "pub enum NativeCliKind { Claude, Codex, Pi, OpenCode, Antigravity, Copilot, Qwen }" },
  { oldN: "23", newN: "", kind: "del", code: "- // 旧版仅在运行时返回可执行文件名" },
  { oldN: "", newN: "23", kind: "add", code: "+ /// 返回驱动是否支持活跃回合中途热纠偏 (Steer)" },
  { oldN: "", newN: "24", kind: "add", code: "+ #[inline]" },
  { oldN: "", newN: "25", kind: "add", code: "+ pub const fn supports_live_steer(self) -> bool {" },
  { oldN: "", newN: "26", kind: "add", code: "+   matches!(self, Self::Pi | Self::Codex | Self::Claude)" },
  { oldN: "", newN: "27", kind: "add", code: "+ }" },
];

const RAIL_TURNS = [
  { idx: 1, title: "Turn #1 · 为 soxu-drivers 补齐热纠偏与 TempDir 单测", kind: "normal" as const },
  { idx: 2, title: "Turn #2 · codex 修改 lib.rs (+28 -3) · 检查点 #04", kind: "handoff" as const },
  { idx: 3, title: "Turn #3 · 提问 1 项 · 待审批 2 项", kind: "warning" as const },
];

export function DesktopWorkbench() {
  const [prefs, setPrefs] = useState<SoxuPreferences>(loadPreferences);
  const [resolvedTheme, setResolvedTheme] = useState<"dark" | "light">(() => applyPreferencesToDom(prefs));

  const [leftOpen, setLeftOpen] = useState(true);
  const [rightOpen, setRightOpen] = useState(true);
  const [rightTab, setRightTab] = useState<"diff" | "files" | "scripts" | "terminal">("diff");
  const [diffLayout, setDiffLayout] = useState<"unified" | "split">("unified");
  const [selectedFile, setSelectedFile] = useState("crates/soxu-drivers/src/lib.rs");

  const [selectedWorkspaceId, setSelectedWorkspaceId] = useState("wks-m2");
  const [magpieMode, setMagpieMode] = useState<"magpie_managed" | "standalone_native">("magpie_managed");
  const [activeTabId, setActiveTabId] = useState("tab-codex");
  const [tabs, setTabs] = useState([
    { id: "tab-codex", cli: "codex", title: "M2 驱动流式热纠偏", draft: false },
    { id: "tab-pi", cli: "pi", title: "seq 游标断线追平", draft: false },
    { id: "tab-draft", cli: "agy", title: "新建草稿会话", draft: true },
  ]);
  const [subagents, setSubagents] = useState([
    { id: "sub-research", cli: "pi", role: "参考库索引巡检", duration: "12s", detached: false },
    { id: "sub-test", cli: "codex", role: "TempDir 夹具测试", duration: "00:18", detached: false },
  ]);

  const [reasoningOpen, setReasoningOpen] = useState(false);
  const [toolOpen, setToolOpen] = useState<Record<string, boolean>>({ read: false, edit: true, bash: true, mcp: false });
  const [ask, setAsk] = useState(INITIAL_ASK);
  const [approvals, setApprovals] = useState<ApprovalItem[]>(INITIAL_APPROVALS);
  const [rejectingId, setRejectingId] = useState<string | null>(null);
  const [rejectNote, setRejectNote] = useState("");
  const [addedTurns, setAddedTurns] = useState<{ id: string; text: string; badge?: string }[]>([]);
  const [activeRail, setActiveRail] = useState(2);

  const [cli, setCli] = useState("codex");
  const [model, setModel] = useState("group/gpt-6.2-sol");
  const [thinking, setThinking] = useState("medium");
  const [permLevel, setPermLevel] = useState<PermissionLevel>("ask");
  const [composerMode, setComposerMode] = useState<TurnBehavior>("steer");
  const [composerText, setComposerText] = useState("");
  const [queuedPrompt, setQueuedPrompt] = useState<string | null>(
    "测试跑通后请同步更新 docs/ops/PROGRESS.md 与 AOCI 认知索引",
  );
  const [lostControlToast, setLostControlToast] = useState<string | null>(null);
  const [composerDockLocked, setComposerDockLocked] = useState(false);

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

  function handlePickModel(nextId: string) {
    setModel(nextId);
    const meta = MODEL_OPTIONS.find((m) => m.id === nextId);
    if (meta && !meta.thinkingOptions.includes(thinking)) {
      const old = thinking;
      const fallback = meta.thinkingOptions[0];
      setThinking(fallback);
      setLostControlToast(`已按 ${nextId} 自动调整思考档位 (${old} → ${fallback})`);
      window.setTimeout(() => setLostControlToast(null), 3500);
    } else {
      setLostControlToast(null);
    }
  }

  function sendPrompt() {
    const t = composerText.trim();
    if (!t) {
      flashToast("已发送 SIGINT 停止当前活跃回合");
      return;
    }
    if (composerMode === "queue") {
      setQueuedPrompt(t);
      setComposerText("");
      flashToast("已加入本轮末尾待发队列");
      return;
    }
    setAddedTurns((prev) => [
      ...prev,
      {
        id: `u-${Date.now()}`,
        text: t,
        badge: composerMode === "steer" ? "Steer 中途纠偏 · 下一步生效" : "Interrupt 已打断重发",
      },
    ]);
    setComposerText("");
  }

  const activeWorkspace = INITIAL_WORKSPACES.find((w) => w.id === selectedWorkspaceId) ?? INITIAL_WORKSPACES[0];
  const rowHeightClass = prefs.density === "compact" ? "min-h-[32px]" : "min-h-[38px]";

  const bucketBadge: Record<string, { label: string; variant: "warning" | "danger" | "running" | "accent" | "neutral" }> = {
    needs_input: { label: "待批 2", variant: "warning" },
    failed: { label: "失败", variant: "danger" },
    running: { label: "01:38", variant: "running" },
    attention: { label: "新完成", variant: "accent" },
    done: { label: "空闲", variant: "neutral" },
  };

  return (
    <TooltipProvider delayDuration={220}>
      <div className="h-screen w-screen flex flex-col bg-[var(--soxu-surface-canvas)] text-[var(--soxu-text-primary)] overflow-hidden">
        {/* 极简纯净 38px 顶部标题栏（无任何界面视角切换条） */}
        <header className="h-[var(--soxu-size-titlebar-height)] min-h-[var(--soxu-size-titlebar-height)] bg-[var(--soxu-surface-sidebar)] border-b border-[var(--soxu-border-hairline)] px-3 flex items-center justify-between gap-3 select-none z-[var(--soxu-z-header)]">
          <div className="flex items-center gap-2 min-w-0">
            <div className="flex items-center gap-2 px-1">
              <span className="h-2 w-2 rounded-full bg-[var(--soxu-semantic-accent)]" aria-hidden="true" />
              <span className="text-[13px] font-semibold tracking-tight">Soxu</span>
            </div>

            <Tooltip>
              <TooltipTrigger asChild>
                <Button variant={leftOpen ? "secondary" : "ghost"} size="icon-sm" onClick={() => setLeftOpen((v) => !v)} aria-label="折叠左侧栏">
                  <PanelLeft className="h-4 w-4" strokeWidth={1.75} />
                </Button>
              </TooltipTrigger>
              <TooltipContent>折叠/展开工作区栏 (Ctrl+B)</TooltipContent>
            </Tooltip>

            <span className="hidden sm:inline-flex items-center gap-1.5 text-xs text-[var(--soxu-text-tertiary)] truncate">
              <GitBranch className="h-3.5 w-3.5 shrink-0" strokeWidth={1.75} />
              <span className="soxu-mono truncate">{activeWorkspace.branch}</span>
            </span>
          </div>

          {/* 居中 Cmd+K 命令搜索触发器 */}
          <button
            type="button"
            onClick={() => setModal("cmdk")}
            className="h-[26px] flex-1 max-w-[340px] rounded-lg border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card)] px-2.5 flex items-center justify-between gap-2 text-xs text-[var(--soxu-text-tertiary)] hover:border-[var(--soxu-border-strong)] hover:text-[var(--soxu-text-secondary)] transition-colors cursor-pointer"
          >
            <span className="flex items-center gap-1.5 truncate">
              <Search className="h-3.5 w-3.5 shrink-0" strokeWidth={1.75} />
              <span className="truncate">搜索会话、文件或命令...</span>
            </span>
            <kbd className="soxu-mono text-[10px] px-1.5 py-0.5 rounded bg-[var(--soxu-surface-control)] text-[var(--soxu-text-secondary)]">
              Ctrl+K
            </kbd>
          </button>

          {/* 右侧极简工具区：悬停展开收纳坞 + 纯图标主题切换 + 设置齿轮 + 右栏开关 */}
          <div className="flex items-center gap-1">
            <HoverExpandDock
              triggerIcon={<LayoutGrid className="h-4 w-4" strokeWidth={1.75} />}
              triggerLabel="更多工作台工具（悬停自动展开）"
              dotColor={magpieMode === "magpie_managed" ? "var(--soxu-semantic-accent)" : "var(--soxu-semantic-warning)"}
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
                    aria-label="Magpie 路由诊断"
                  >
                    <Network className="h-3.5 w-3.5" strokeWidth={1.75} />
                  </Button>
                </TooltipTrigger>
                <TooltipContent>Magpie 路由与 CLI 诊断</TooltipContent>
              </Tooltip>

              <Tooltip>
                <TooltipTrigger asChild>
                  <Button variant="ghost" size="icon-sm" onClick={() => setModal("schedules")} aria-label="自动化调度中心">
                    <Clock className="h-3.5 w-3.5" strokeWidth={1.75} />
                  </Button>
                </TooltipTrigger>
                <TooltipContent>Schedules &amp; Heartbeats</TooltipContent>
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
                  <Button variant="ghost" size="icon-sm" onClick={() => setModal("history")} aria-label="导入原生 CLI 历史">
                    <History className="h-3.5 w-3.5" strokeWidth={1.75} />
                  </Button>
                </TooltipTrigger>
                <TooltipContent>导入本机 CLI 历史会话</TooltipContent>
              </Tooltip>

              <Tooltip>
                <TooltipTrigger asChild>
                  <Button variant="ghost" size="icon-sm" onClick={() => setModal("commit")} aria-label="Git Emoji AI 提交">
                    <GitCommit className="h-3.5 w-3.5" strokeWidth={1.75} />
                  </Button>
                </TooltipTrigger>
                <TooltipContent>Git Emoji AI 提交</TooltipContent>
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
                  aria-label="打开设置界面"
                >
                  <Settings className="h-4 w-4" strokeWidth={1.75} />
                </Button>
              </TooltipTrigger>
              <TooltipContent>设置 (Ctrl+,)</TooltipContent>
            </Tooltip>

            <Tooltip>
              <TooltipTrigger asChild>
                <Button variant={rightOpen ? "secondary" : "ghost"} size="icon-sm" onClick={() => setRightOpen((v) => !v)} aria-label="折叠右侧栏">
                  <PanelRight className="h-4 w-4" strokeWidth={1.75} />
                </Button>
              </TooltipTrigger>
              <TooltipContent>折叠/展开右侧审阅栏 (Ctrl+J)</TooltipContent>
            </Tooltip>
          </div>
        </header>

        {/* 主四区网格（平滑宽度过渡，GPUI panel_width * delta 等价） */}
        <div className="flex-1 min-h-0 flex overflow-hidden">
          {/* 左侧项目与工作区栏 */}
          <aside
            aria-label="项目与工作区树"
            className={cn(
              "hidden md:flex flex-col bg-[var(--soxu-surface-sidebar)] border-r border-[var(--soxu-border-hairline)] overflow-hidden transition-all duration-[var(--soxu-motion-panel)] ease-[var(--soxu-ease-out)] shrink-0",
              leftOpen ? "w-[260px] opacity-100" : "w-0 opacity-0 border-r-0",
            )}
          >
            <div className="w-[260px] flex-1 overflow-y-auto p-2.5 flex flex-col gap-3 soxu-scroll">
              <div className="flex items-center justify-between px-1.5">
                <span className="text-[11px] font-semibold uppercase tracking-wider text-[var(--soxu-text-tertiary)]">
                  Soxu ({INITIAL_WORKSPACES.length})
                </span>
                <Tooltip>
                  <TooltipTrigger asChild>
                    <button
                      type="button"
                      onClick={() => flashToast("新建 Worktree（Branch-off / Checkout PR）")}
                      className="h-5 w-5 rounded flex items-center justify-center text-[var(--soxu-text-tertiary)] hover:bg-[var(--soxu-interaction-hover)] hover:text-[var(--soxu-text-primary)] cursor-pointer"
                      aria-label="新建 Worktree"
                    >
                      <Plus className="h-3.5 w-3.5" strokeWidth={1.75} />
                    </button>
                  </TooltipTrigger>
                  <TooltipContent>新建隔离 Worktree</TooltipContent>
                </Tooltip>
              </div>

              <div className="flex flex-col gap-0.5">
                {INITIAL_WORKSPACES.map((w) => {
                  const selected = selectedWorkspaceId === w.id;
                  const b = bucketBadge[w.bucket];
                  return (
                    <button
                      key={w.id}
                      type="button"
                      onClick={() => setSelectedWorkspaceId(w.id)}
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
                          {w.isolation === "worktree" ? (
                            <GitBranch className="h-3.5 w-3.5 shrink-0 text-[var(--soxu-text-tertiary)]" strokeWidth={1.75} />
                          ) : (
                            <Folder className="h-3.5 w-3.5 shrink-0 text-[var(--soxu-text-tertiary)]" strokeWidth={1.75} />
                          )}
                          <span className="soxu-mono font-medium truncate">{w.branch}</span>
                        </div>
                        <div className="pl-5 text-[11px] soxu-tabular text-[var(--soxu-text-tertiary)] flex items-center gap-1.5">
                          <span>{w.diffStat}</span>
                          {w.label && (
                            <span className="px-1.5 rounded-full bg-[var(--soxu-surface-card)] border border-[var(--soxu-border-subtle)] text-[10px]">
                              {w.label}
                            </span>
                          )}
                        </div>
                      </div>
                      <Badge variant={b.variant} className={w.bucket === "running" ? "soxu-live-dot" : undefined}>
                        {b.label}
                      </Badge>
                    </button>
                  );
                })}
              </div>
            </div>

            <div className="w-[260px] border-t border-[var(--soxu-border-hairline)] p-2 flex items-center justify-between">
              <HoverExpandDock
                triggerIcon={<Cpu className="h-3.5 w-3.5" strokeWidth={1.75} />}
                triggerLabel="工作区工具（历史 / 调度 / 引导）"
                expandDirection="right"
              >
                <Button variant="ghost" size="sm" className="h-6 px-2 text-[11px]" onClick={() => setModal("history")}>
                  历史
                </Button>
                <Button variant="ghost" size="sm" className="h-6 px-2 text-[11px]" onClick={() => setModal("schedules")}>
                  调度
                </Button>
                <Button variant="ghost" size="sm" className="h-6 px-2 text-[11px]" onClick={() => setModal("onboarding")}>
                  引导
                </Button>
              </HoverExpandDock>
              <span className="text-[11px] soxu-mono text-[var(--soxu-text-tertiary)] px-1.5">
                {magpieMode === "magpie_managed" ? "Magpie" : "Native"}
              </span>
            </div>
          </aside>

          {/* 中央多 Agent 会话画布 */}
          <main className="flex-1 min-w-[320px] flex flex-col bg-[var(--soxu-surface-canvas)] overflow-hidden relative">
            {/* 会话页签条 */}
            <div className="h-[var(--soxu-size-tabbar-height)] min-h-[var(--soxu-size-tabbar-height)] bg-[var(--soxu-surface-sidebar)] border-b border-[var(--soxu-border-hairline)] px-2 flex items-end gap-0.5 overflow-x-auto soxu-no-scrollbar">
              {tabs.map((t) => {
                const active = activeTabId === t.id;
                return (
                  <button
                    key={t.id}
                    type="button"
                    onClick={() => setActiveTabId(t.id)}
                    className={cn(
                      "group relative h-[31px] max-w-[220px] px-3 rounded-t-lg text-xs flex items-center gap-2 whitespace-nowrap cursor-pointer transition-colors duration-150 border border-b-0",
                      active
                        ? "bg-[var(--soxu-surface-canvas)] text-[var(--soxu-text-primary)] border-[var(--soxu-border-hairline)]"
                        : "bg-transparent text-[var(--soxu-text-tertiary)] border-transparent hover:bg-[var(--soxu-interaction-hover)] hover:text-[var(--soxu-text-primary)]",
                    )}
                  >
                    {active && (
                      <span className="absolute top-0 left-2 right-2 h-0.5 rounded-b bg-[var(--soxu-semantic-accent)] transition-all duration-150" />
                    )}
                    <span className="soxu-mono text-[10.5px] text-[var(--soxu-text-tertiary)]">{t.cli}</span>
                    <span className={cn("truncate", t.draft && "italic")}>{t.title}</span>
                  </button>
                );
              })}
              <Button
                variant="ghost"
                size="icon-sm"
                className="mb-0.5 ml-1"
                onClick={() => {
                  const id = `tab-${Date.now()}`;
                  setTabs((prev) => [...prev, { id, cli, title: "新建草稿会话", draft: true }]);
                  setActiveTabId(id);
                }}
                aria-label="新建会话标签"
              >
                <Plus className="h-3.5 w-3.5" strokeWidth={1.75} />
              </Button>
            </div>

            {/* Subagents 子代理轨 */}
            <div className="h-[var(--soxu-size-subagent-bar-height)] min-h-[var(--soxu-size-subagent-bar-height)] bg-[var(--soxu-surface-sidebar)] border-b border-[var(--soxu-border-hairline)] px-3 flex items-center gap-2 text-[11.5px] text-[var(--soxu-text-tertiary)] overflow-x-auto soxu-no-scrollbar">
              <span className="shrink-0">子代理轨:</span>
              {subagents.map((sa) => (
                <span
                  key={sa.id}
                  className="inline-flex items-center gap-1.5 h-5 px-2 rounded-full bg-[var(--soxu-surface-card)] border border-[var(--soxu-border-subtle)] text-[var(--soxu-text-secondary)] soxu-mono shrink-0"
                >
                  <Bot className="h-3 w-3 text-[var(--soxu-semantic-accent)]" strokeWidth={1.75} />
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
                        setTabs((prev) => [...prev, { id: tid, cli: sa.cli, title: `子代理: ${sa.role}`, draft: false }]);
                        setActiveTabId(tid);
                        flashToast(`已独立子代理页签：${sa.role}`);
                      }}
                      className="inline-flex items-center gap-0.5 px-1.5 py-0.2 rounded-full bg-[var(--soxu-surface-control)] text-[10px] text-[var(--soxu-text-primary)] hover:bg-[var(--soxu-action-inverse)] hover:text-[var(--soxu-action-on-inverse)] transition-colors cursor-pointer"
                    >
                      <span>独立页签</span>
                      <ExternalLink className="h-2.5 w-2.5" strokeWidth={1.75} />
                    </button>
                  )}
                </span>
              ))}
            </div>

            {/* 对话转录区 + 右侧轮次迷你刻度轨 */}
            <div className="flex-1 min-h-0 flex overflow-hidden relative">
              <div className="flex-1 overflow-y-auto px-5 py-5 soxu-scroll">
                <div className="w-full max-w-[var(--soxu-layout-transcript-max-width)] mx-auto flex flex-col gap-5">
                  <div id="turn-1" className="flex justify-end">
                    <div className="max-w-[78%] rounded-[18px] bg-[var(--soxu-surface-raised)] px-4 py-2.5 leading-relaxed">
                      为 <code className="px-1 py-0.5 rounded bg-[var(--soxu-semantic-code-wash)]">soxu-drivers</code> 补齐原生
                      CLI 流式热纠偏能力探测，并用 <code className="px-1 py-0.5 rounded bg-[var(--soxu-semantic-code-wash)]">TempDir</code> 完成单测。
                    </div>
                  </div>

                  <div id="turn-2" className="group/msg flex flex-col gap-2.5">
                    <div className="flex items-center justify-between gap-2 text-xs">
                      <div className="flex items-center gap-2 font-semibold">
                        <Cpu className="h-3.5 w-3.5 text-[var(--soxu-semantic-accent)]" strokeWidth={1.75} />
                        <span>codex</span>
                        <span className="soxu-mono font-normal text-[var(--soxu-text-tertiary)]">
                          {model} · {thinking}
                        </span>
                      </div>
                      <div className="flex items-center gap-1 opacity-0 group-hover/msg:opacity-100 focus-within:opacity-100 transition-opacity duration-150">
                        <Button
                          variant="ghost"
                          size="sm"
                          className="h-6 px-2 text-[11px]"
                          onClick={() => flashToast("已复制当前回合回复")}
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

                    {/* Collapsible 深度思考链 */}
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
                          <span>已深度思考 6.4 秒 · 查看推演步骤</span>
                        </button>
                      </CollapsibleTrigger>
                      <CollapsibleContent className="ml-2 pl-3 border-l-2 border-[var(--soxu-border-subtle)] text-xs text-[var(--soxu-text-secondary)] flex flex-col gap-1 py-1">
                        <div>1. 对照 ADR-002：Pi / Codex / Claude 支持运行中热注入；OpenCode / Agy 走柔和中断续接。</div>
                        <div>2. 遵循安全红线，全部测试文件隔离在 TempDir 临时目录。</div>
                      </CollapsibleContent>
                    </Collapsible>

                    {/* 工具卡 1：只读探索 */}
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
                            <span className="truncate">已检索并读取 6 个文件 · 0.4s</span>
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

                    {/* 工具卡 2：内联 Rust 语法高亮 Diff（实时响应代码字体设置） */}
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
                            <span className="truncate">修改 crates/soxu-drivers/src/lib.rs (+28 -3)</span>
                          </span>
                          <Badge variant="accent">已应用</Badge>
                        </button>
                      </CollapsibleTrigger>
                      <CollapsibleContent className="p-2 border-t border-[var(--soxu-border-hairline)]">
                        <div className="rounded-lg border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-inset)] overflow-hidden soxu-code-block">
                          {DIFF_ROWS.map((r, idx) => (
                            <div
                              key={idx}
                              className="flex leading-relaxed whitespace-pre"
                              style={{
                                backgroundColor:
                                  r.kind === "add"
                                    ? "var(--soxu-syntax-diff-add-bg)"
                                    : r.kind === "del"
                                      ? "var(--soxu-syntax-diff-del-bg)"
                                      : undefined,
                              }}
                            >
                              <span
                                className="w-[var(--soxu-size-diff-gutter-width)] shrink-0 px-1.5 text-right text-[var(--soxu-text-tertiary)] border-r border-[var(--soxu-border-hairline)] select-none soxu-tabular"
                                style={{
                                  backgroundColor:
                                    r.kind === "add"
                                      ? "var(--soxu-syntax-diff-add-gutter)"
                                      : r.kind === "del"
                                        ? "var(--soxu-syntax-diff-del-gutter)"
                                        : undefined,
                                }}
                              >
                                {r.oldN} {r.newN}
                              </span>
                              <span
                                className={cn(
                                  "px-2.5 overflow-x-auto flex-1",
                                  r.kind === "add"
                                    ? "text-[var(--soxu-semantic-success)]"
                                    : r.kind === "del"
                                      ? "text-[var(--soxu-semantic-danger)]"
                                      : "text-[var(--soxu-syntax-plain)]",
                                )}
                              >
                                {r.code}
                              </span>
                            </div>
                          ))}
                        </div>
                      </CollapsibleContent>
                    </Collapsible>

                    {/* 工具卡 3：终端输出 */}
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
                            <span className="truncate">cargo test -p soxu-drivers · 退出码 0 · 1.8s</span>
                          </span>
                          <Badge variant="accent">6 passed</Badge>
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

                    {/* Checkpoint 锚点（悬停显现回退操作） */}
                    <div className="group/cp flex items-center gap-2.5 my-1">
                      <div className="flex-1 h-px bg-[var(--soxu-border-hairline)]" />
                      <button
                        type="button"
                        onClick={() => setModal("rewind")}
                        className="h-[26px] px-3 rounded-full bg-[var(--soxu-surface-card-muted)] border border-[var(--soxu-border-subtle)] flex items-center gap-2 text-[11.5px] soxu-mono text-[var(--soxu-text-secondary)] hover:border-[var(--soxu-border-strong)] cursor-pointer transition-colors"
                      >
                        <GitCommit className="h-3.5 w-3.5 text-[var(--soxu-semantic-handoff)]" strokeWidth={1.75} />
                        <span>检查点 #04 · 7c4e91a · 2 files (+28 -3)</span>
                        <span className="inline-flex items-center gap-1 opacity-65 group-hover/cp:opacity-100 text-[var(--soxu-semantic-handoff)]">
                          <RotateCcw className="h-3 w-3" strokeWidth={1.75} />
                          <span>回退</span>
                        </span>
                      </button>
                      <div className="flex-1 h-px bg-[var(--soxu-border-hairline)]" />
                    </div>
                  </div>

                  {/* Turn 3：提问卡 + 双级权限审批卡 */}
                  <div id="turn-3" className="flex flex-col gap-3">
                    <div className="rounded-xl border border-[var(--soxu-semantic-running-border)] border-l-[3px] border-l-[var(--soxu-semantic-accent)] bg-[var(--soxu-surface-card)] p-3.5 flex flex-col gap-2.5">
                      <div className="flex items-center justify-between">
                        <strong className="text-[13px]">结构化提问 (PendingUserInput)</strong>
                        <Badge variant="running">需你确认</Badge>
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
                            className="rounded-xl border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card-muted)] px-3.5 py-2 text-xs text-[var(--soxu-text-secondary)] flex items-center justify-between animate-in fade-in-0 duration-150"
                          >
                            <span className="flex items-center gap-2">
                              <Check className="h-3.5 w-3.5 text-[var(--soxu-semantic-accent)]" strokeWidth={2} />
                              <span>
                                {apr.toolKind} 已消解 ({apr.resolution}
                                {apr.note ? ` · 指导：${apr.note}` : ""})
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
                            "rounded-xl border bg-[var(--soxu-surface-card)] p-3.5 flex flex-col gap-2.5 border-l-[3px]",
                            apr.highRisk
                              ? "border-[var(--soxu-semantic-danger-border)] border-l-[var(--soxu-semantic-danger)]"
                              : "border-[var(--soxu-semantic-warning-border)] border-l-[var(--soxu-semantic-warning)]",
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
                              {apr.highRisk ? "禁用始终允许" : "常规审批"}
                            </Badge>
                          </div>
                          <div className="rounded-lg border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-terminal)] px-3 py-2 soxu-code-block">
                            ❯ {apr.command}
                          </div>
                          <div className="text-xs text-[var(--soxu-text-secondary)]">{apr.riskNote}</div>

                          {rejectingId === apr.id ? (
                            <div className="flex gap-2">
                              <input
                                autoFocus
                                value={rejectNote}
                                onChange={(e) => setRejectNote(e.target.value)}
                                placeholder="输入修改指导（如：改用 TempDir 自动回收）"
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
                                  flashToast("已发送拒绝指导并记录审计");
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

                    {addedTurns.map((t) => (
                      <div key={t.id} className="flex flex-col items-end gap-1 animate-in fade-in-0 slide-in-from-bottom-2 duration-200">
                        {t.badge && <Badge variant="running">{t.badge}</Badge>}
                        <div className="max-w-[78%] rounded-[18px] bg-[var(--soxu-surface-raised)] px-4 py-2.5">
                          {t.text}
                        </div>
                      </div>
                    ))}
                  </div>
                </div>
              </div>

              {/* 对话右侧 20px 轮次迷你导航轨 */}
              <nav aria-label="轮次导航轨" className="w-[20px] flex flex-col items-end justify-center gap-1.5 px-1 select-none">
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
                          activeRail === rt.idx ? "w-4" : "w-2.5 hover:w-4",
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
            </div>

            {/* 极简三态悬浮输入舱 */}
            <div className="px-5 pb-3.5 pt-1 bg-gradient-to-t from-[var(--soxu-surface-canvas)] via-[var(--soxu-surface-canvas)] to-transparent">
              <div className="w-full max-w-[var(--soxu-layout-composer-max-width)] mx-auto flex flex-col gap-1.5">
                {lostControlToast && <div className="text-[11.5px] text-[var(--soxu-semantic-warning)] px-2">{lostControlToast}</div>}

                {queuedPrompt && (
                  <div className="rounded-xl border border-[var(--soxu-semantic-warning-border)] bg-[var(--soxu-surface-card)] px-3 py-1.5 flex items-center justify-between gap-2 text-xs">
                    <span className="soxu-mono truncate text-[var(--soxu-text-secondary)]">排队中：{queuedPrompt}</span>
                    <div className="flex items-center gap-1 shrink-0">
                      <Button
                        size="sm"
                        variant="ghost"
                        className="h-6 px-2 text-[var(--soxu-semantic-running)]"
                        onClick={() => {
                          setAddedTurns((p) => [...p, { id: `q-${Date.now()}`, text: queuedPrompt, badge: "由排队提级为 Steer" }]);
                          setQueuedPrompt(null);
                        }}
                      >
                        提级 Steer
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
                  <div className="flex items-center gap-1.5 px-1">
                    <span className="inline-flex items-center gap-1 rounded-md border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card)] px-2 py-0.5 text-[11px] soxu-mono text-[var(--soxu-text-secondary)]">
                      <FileCode2 className="h-3 w-3" strokeWidth={1.75} />
                      <span>crates/soxu-drivers/src/lib.rs:22-27</span>
                    </span>
                  </div>

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
                    placeholder="输入指令或键入 @ 移交至其他 CLI..."
                    className="w-full resize-none bg-transparent border-0 px-2 py-0.5 text-sm outline-none placeholder:text-[var(--soxu-text-ghost)]"
                  />

                  <div className="flex items-center justify-between gap-2 px-1">
                    {/* 左侧常驻精简模型名 + 悬停自动展开全部参数胶囊 */}
                    <div className="flex items-center gap-1.5 min-w-0">
                      <span className="text-[11.5px] soxu-mono text-[var(--soxu-text-secondary)] px-2 py-0.5 rounded-full bg-[var(--soxu-surface-control)] truncate max-w-[175px]">
                        {cli} · {model}
                      </span>

                      <HoverExpandDock
                        triggerIcon={<SlidersHorizontal className="h-3.5 w-3.5" strokeWidth={1.75} />}
                        triggerLabel="调节 CLI、模型、思考强度与 @agent 移交"
                        locked={composerDockLocked}
                        expandDirection="right"
                      >
                        <DropdownMenu onOpenChange={setComposerDockLocked}>
                          <DropdownMenuTrigger asChild>
                            <Button variant="ghost" size="sm" className="h-6 px-2 text-[11px] soxu-mono">
                              CLI: {cli}
                            </Button>
                          </DropdownMenuTrigger>
                          <DropdownMenuContent>
                            <DropdownMenuLabel>原生 CLI 驱动</DropdownMenuLabel>
                            <DropdownMenuSeparator />
                            {CLI_OPTIONS.filter((c) => c.available).map((c) => (
                              <DropdownMenuItem key={c.id} onClick={() => setCli(c.id)}>
                                <span className="soxu-mono">{c.id}</span>
                                <Badge variant="neutral">v{c.ver}</Badge>
                              </DropdownMenuItem>
                            ))}
                          </DropdownMenuContent>
                        </DropdownMenu>

                        <DropdownMenu onOpenChange={setComposerDockLocked}>
                          <DropdownMenuTrigger asChild>
                            <Button variant="ghost" size="sm" className="h-6 px-2 text-[11px] soxu-mono">
                              模型
                            </Button>
                          </DropdownMenuTrigger>
                          <DropdownMenuContent className="w-64">
                            <DropdownMenuLabel>Magpie 模型池</DropdownMenuLabel>
                            <DropdownMenuSeparator />
                            {MODEL_OPTIONS.map((m) => (
                              <DropdownMenuItem key={m.id} onClick={() => handlePickModel(m.id)}>
                                <span className="soxu-mono truncate">{m.id}</span>
                                <span className="text-[10.5px] text-[var(--soxu-text-tertiary)]">{m.tier}</span>
                              </DropdownMenuItem>
                            ))}
                          </DropdownMenuContent>
                        </DropdownMenu>

                        <DropdownMenu onOpenChange={setComposerDockLocked}>
                          <DropdownMenuTrigger asChild>
                            <Button variant="ghost" size="sm" className="h-6 px-2 text-[11px] soxu-mono">
                              {thinking}
                            </Button>
                          </DropdownMenuTrigger>
                          <DropdownMenuContent>
                            {(MODEL_OPTIONS.find((m) => m.id === model)?.thinkingOptions ?? ["medium", "high"]).map((lv) => (
                              <DropdownMenuItem key={lv} onClick={() => setThinking(lv)}>
                                <span className="soxu-mono">thinking: {lv}</span>
                              </DropdownMenuItem>
                            ))}
                          </DropdownMenuContent>
                        </DropdownMenu>

                        <Button
                          variant="ghost"
                          size="sm"
                          className="h-6 px-2 text-[11px] text-[var(--soxu-semantic-handoff)]"
                          onClick={() => setComposerText((t) => `@codex ${t}`)}
                        >
                          <AtSign className="h-3 w-3" strokeWidth={1.75} />
                          <span>移交</span>
                        </Button>

                        <Button
                          variant="ghost"
                          size="sm"
                          className="h-6 px-2 text-[11px]"
                          onClick={() => {
                            const order: PermissionLevel[] = ["ask", "auto-edit", "full-access"];
                            setPermLevel(order[(order.indexOf(permLevel) + 1) % order.length]);
                          }}
                        >
                          {permLevel}
                        </Button>
                      </HoverExpandDock>
                    </div>

                    <div className="flex items-center gap-1.5 shrink-0">
                      <Button
                        size="sm"
                        variant="outline"
                        className="h-7 rounded-full text-[11.5px] px-2.5"
                        onClick={() => {
                          const order: TurnBehavior[] = ["steer", "queue", "interrupt"];
                          setComposerMode(order[(order.indexOf(composerMode) + 1) % order.length]);
                        }}
                      >
                        {composerMode === "steer" ? "纠偏 Steer" : composerMode === "queue" ? "排队 Queue" : "打断重发"}
                      </Button>

                      <Button
                        size="icon"
                        variant="default"
                        onClick={sendPrompt}
                        aria-label={composerText.trim() ? "发送指令" : "停止当前活跃回合"}
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
          </main>

          {/* 右侧审阅栏 */}
          <aside
            aria-label="上下文与代码审阅栏"
            className={cn(
              "hidden lg:flex flex-col bg-[var(--soxu-surface-sidebar)] border-l border-[var(--soxu-border-hairline)] overflow-hidden transition-all duration-[var(--soxu-motion-panel)] ease-[var(--soxu-ease-out)] shrink-0",
              rightOpen ? "w-[380px] opacity-100" : "w-0 opacity-0 border-l-0",
            )}
          >
            <div className="w-[380px] flex-1 flex flex-col overflow-hidden">
              <div className="h-[36px] px-2.5 border-b border-[var(--soxu-border-hairline)] flex items-center justify-between">
                <Tabs value={rightTab} onValueChange={(v) => setRightTab(v as typeof rightTab)}>
                  <TabsList>
                    <TabsTrigger value="diff">Diff</TabsTrigger>
                    <TabsTrigger value="files">Files</TabsTrigger>
                    <TabsTrigger value="scripts">Scripts</TabsTrigger>
                    <TabsTrigger value="terminal">Terminal</TabsTrigger>
                  </TabsList>
                </Tabs>
              </div>

              <div className="flex-1 overflow-y-auto p-3 flex flex-col gap-3 soxu-scroll">
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
                            {m === "unified" ? "Unified" : "Split"}
                          </button>
                        ))}
                      </div>
                      <Button size="sm" variant="default" onClick={() => setModal("commit")}>
                        <GitCommit className="h-3.5 w-3.5" strokeWidth={1.75} />
                        <span>提交变更</span>
                      </Button>
                    </div>

                    <div className="text-[11px] soxu-mono text-[var(--soxu-text-tertiary)]">
                      crates/soxu-drivers/src/lib.rs · vs {prefs.defaultBranch}
                    </div>

                    {diffLayout === "unified" ? (
                      <div className="rounded-lg border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-inset)] overflow-hidden soxu-code-block">
                        {DIFF_ROWS.map((r, i) => (
                          <div
                            key={i}
                            className="px-2 py-0.5 whitespace-pre overflow-x-auto"
                            style={{
                              backgroundColor:
                                r.kind === "add"
                                  ? "var(--soxu-syntax-diff-add-bg)"
                                  : r.kind === "del"
                                    ? "var(--soxu-syntax-diff-del-bg)"
                                    : undefined,
                            }}
                          >
                            {r.code}
                          </div>
                        ))}
                      </div>
                    ) : (
                      <div className="grid grid-cols-2 rounded-lg border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-inset)] soxu-code-block text-[11px]">
                        <div className="p-2 border-r border-[var(--soxu-border-hairline)] text-[var(--soxu-semantic-danger)]">
                          23 - // 旧版仅返回路径
                        </div>
                        <div className="p-2 text-[var(--soxu-semantic-success)]">25 + pub const fn supports_live_steer()</div>
                      </div>
                    )}
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
                    <pre className="m-0 rounded-lg border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-terminal)] p-3 soxu-code-block overflow-x-auto">
                      <code>
                        <span className="syn-com">// {selectedFile}</span>
                        {"\n"}
                        <span className="syn-kw">pub enum</span> <span className="syn-ty">EnvironmentMode</span> {"{\n"}
                        {"  "}MagpieManaged,{"\n"}
                        {"  "}StandaloneNative,{"\n"}
                        {"}"}
                      </code>
                    </pre>
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

                {rightTab === "terminal" && (
                  <pre className="m-0 rounded-lg border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-terminal)] p-3 soxu-code-block leading-relaxed">
                    <div>
                      <span style={{ color: "var(--soxu-ansi-bright-green)" }}>dev@soxu</span>{" "}
                      <span style={{ color: "var(--soxu-ansi-blue)" }}>~/projects/Soxu</span>
                    </div>
                    <div>
                      <span style={{ color: "var(--soxu-ansi-green)" }}>OK</span> check-all gates passed
                    </div>
                  </pre>
                )}
              </div>
            </div>
          </aside>
        </div>

        {/* 底部状态横梁（26px，极简收敛） */}
        <footer className="h-[var(--soxu-size-statusbar-height)] min-h-[var(--soxu-size-statusbar-height)] bg-[var(--soxu-surface-sidebar)] border-t border-[var(--soxu-border-hairline)] px-3 flex items-center justify-between text-[11px] text-[var(--soxu-text-tertiary)] soxu-tabular">
          <div className="flex items-center gap-4">
            <span className="inline-flex items-center gap-1.5 soxu-mono">
              <span className="h-1.5 w-1.5 rounded-full bg-[var(--soxu-semantic-accent)]" />
              Daemon 127.0.0.1:7698 (seq #1056)
            </span>
            <button
              type="button"
              onClick={() => {
                setSettingsSection("magpie");
                setSettingsOpen(true);
              }}
              className="hover:text-[var(--soxu-text-primary)] cursor-pointer transition-colors soxu-mono"
            >
              {magpieMode === "magpie_managed" ? "Magpie :3425 · 2ms" : "StandaloneNative"}
            </button>
          </div>
          <div className="flex items-center gap-3">
            {toast && <span className="text-[var(--soxu-semantic-accent)] animate-in fade-in-0 duration-150">{toast}</span>}
            <button
              type="button"
              onClick={() => setModal("pair")}
              className="hover:text-[var(--soxu-text-primary)] cursor-pointer transition-colors"
            >
              2 台设备在线
            </button>
          </div>
        </footer>

        <UtilityModals
          modal={modal}
          onClose={() => setModal(null)}
          onOpenModal={(m) => setModal(m)}
          onOpenSettings={() => setSettingsOpen(true)}
          onToggleTheme={() => handleToggleTheme(null)}
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
