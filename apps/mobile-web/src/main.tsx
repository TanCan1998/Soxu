import {
  Archive,
  ArchiveRestore,
  ArrowDown,
  ArrowUp,
  Bolt,
  Check,
  ChevronRight,
  ChevronUp,
  Clock,
  Eye,
  FileText,
  Folder,
  FolderOpen,
  GitBranch,
  History,
  Inbox,
  Layers,
  LayoutGrid,
  MessageSquare,
  Moon,
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
  Star,
  Sun,
  X,
} from "lucide-react";
import { StrictMode, useEffect, useMemo, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { AgentIcon } from "@/components/AgentIcon";
import { CodeDiffView, CodeSnippetView } from "@/components/CodeBlockView";
import {
  AgentModelConfigPanel,
  ComposerAgentModelPicker,
  ComposerChipBar,
  ComposerHandoffMenu,
  ComposerPermissionButton,
  ComposerPlusMenu,
  ComposerTurnBehaviorGroup,
  HandoffConfigPanel,
  PermissionIcon,
} from "@/components/ComposerActionMenus";
import { ContextUsagePanel, ContextUsageRing } from "@/components/ContextUsageRing";
import { HoverExpandDock } from "@/components/HoverExpandDock";
import { InboxTriageView } from "@/components/InboxTriageView";
import { MultiQueueBar } from "@/components/MultiQueueBar";
import { QuoteSelectionPopover } from "@/components/QuoteSelectionPopover";
import { SessionStatsPanel, SessionStatsPopover } from "@/components/SessionStatsPopover";
import { SettingsDialog, type SettingsSectionId } from "@/components/SettingsDialog";
import { SinkLayout } from "@/components/SinkLayout";
import { ThemeToggle } from "@/components/ThemeToggle";
import { ToolCallCard } from "@/components/ToolCallCard";
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
import { TooltipProvider } from "@/components/ui/tooltip";
import {
  THINKING_LABELS,
  STATIC_AGENT_PROFILES,
  getAgentModels,
  getAgentProfile,
  getPermissionMeta,
  reconcileAgentCapabilities,
} from "@/lib/agent-capabilities";
import {
  DaemonClient,
  claimPairing,
  clearStoredPairing,
  consumeOfferFromLocation,
  loadStoredPairing,
  saveStoredPairing,
  type ConnectionPhase,
  type StoredPairing,
} from "@/lib/daemon-client";
import {
  eventToViewUpdaters,
  mapSnapshot,
  pickDefaultSession,
  upsertMappedTurn,
} from "@/lib/daemon-store";
import { type EventEnvelope, type ServerHello } from "@/lib/daemon-protocol";
import { I18nProvider, useI18n } from "@/lib/i18n";
import {
  applyPreferencesToDom,
  loadPreferences,
  savePreferences,
  toggleThemeWithTransition,
  type SoxuPreferences,
} from "@/lib/preferences";
import { formatShortcut, isImeComposing, useAdaptiveViewport } from "@/lib/useAdaptiveViewport";
import { cn } from "@/lib/utils";
import "./index.css";

/** 真实链路下未由 Daemon 供给的模块级占位：等对应协议能力接入后自然替换。 */
const EMPTY_DIFF_FILES: import("./types").DiffFileItem[] = [];
const EMPTY_LIVE_STEPS: LiveStepItem[] = [];
const EMPTY_SCRIPTS: ScriptItem[] = [];
const EMPTY_SCHEDULES: ScheduleItem[] = [];
const EMPTY_ARTIFACT: ArtifactItem = {
  id: "empty",
  title: "暂无活跃产物",
  kind: "plan",
  path: "",
  summary: "当前会话尚未产出可审阅的结构化产物。",
  status: "draft",
  content: [],
};
const EMPTY_ASK: AskQuestionState = {
  id: "",
  prompt: "当前会话暂无待确认的结构化提问",
  options: [],
  selectedId: "",
  customNote: "",
  submitted: false,
};
const MOBILE_QUICK_CHIPS = {
  running: [
    { id: "qc-check", label: "先跑 check-all 别急着提交", mode: "steer" as const },
    { id: "qc-stop-risk", label: "遇高危改动先暂停汇报", mode: "steer" as const },
    { id: "qc-aoci", label: "收尾对齐本地认知索引", mode: "queue" as const },
  ],
  idle: [
    { id: "qc-summary", label: "总结本轮改动与潜在风险", mode: "steer" as const },
    { id: "qc-tests", label: "运行全工作区增量门禁", mode: "steer" as const },
    { id: "qc-commit", label: "生成 Git Emoji 提交预览", mode: "steer" as const },
  ],
};

const CLI_OPTIONS: import("./types").CliOption[] = Object.values(STATIC_AGENT_PROFILES).map((p) => ({
  id: p.id,
  bin: p.id,
  ver: "",
  available: true,
}));
import type {
  ApprovalItem,
  ArtifactItem,
  AskQuestionState,
  ComposerChip,
  HandoffSelection,
  LiveStepItem,
  MobileTab,
  PermissionLevel,
  QueuedPromptItem,
  QuickChatItem,
  ScheduleItem,
  ScriptItem,
  SessionTelemetryStats,
  SheetKind,
  TurnBehavior,
  TurnMessage,
  WorkspaceBucket,
  WorkspaceItem,
} from "./types";

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

  const vp = useAdaptiveViewport(prefs);
  const sizeClass = vp.sizeClass;
  const isFoldDualH = vp.foldPosture === "dual-horizontal";
  const isFoldDualV = vp.foldPosture === "dual-vertical" && !isFoldDualH;
  const isCompactSink = sizeClass === "compact" && !vp.isShortViewport && !isFoldDualH && !isFoldDualV;
  const showCompactRail = (vp.isShortViewport || isFoldDualV) && !isFoldDualH;

  const leftTriggerRef = useRef<HTMLButtonElement>(null);
  const transcriptScrollRef = useRef<HTMLDivElement>(null);
  const transcriptContentRef = useRef<HTMLDivElement>(null);
  const composerInputRef = useRef<HTMLTextAreaElement>(null);
  const composerSwipeStartYRef = useRef<number | null>(null);

  const [activeTab, setActiveTab] = useState<MobileTab>("session");
  const [inboxSubMode, setInboxSubMode] = useState<"triage" | "approvals">("triage");
  const [workspaces, setWorkspaces] = useState<WorkspaceItem[]>([]);
  const [quickChats, setQuickChats] = useState<QuickChatItem[]>([]);
  const [targetKind, setTargetKind] = useState<"workspace" | "quick">("workspace");
  const [selectedWorkspaceId, setSelectedWorkspaceId] = useState<string>("");
  const [selectedQuickChatId, setSelectedQuickChatId] = useState<string>("");
  const [bucketFilter, setBucketFilter] = useState<"all" | WorkspaceBucket>("all");
  const [openProjects, setOpenProjects] = useState<Record<string, boolean>>({ Soxu: true, Magpie: true });
  const [archivedOpen, setArchivedOpen] = useState(false);

  const [approvals, setApprovals] = useState<ApprovalItem[]>([]);
  const [approvalSubTab, setApprovalSubTab] = useState<"pending" | "history">("pending");
  const [askQuestion, setAskQuestion] = useState<AskQuestionState>(EMPTY_ASK);
  const [turns, setTurns] = useState<TurnMessage[]>([]);
  const [liveSteps, setLiveSteps] = useState<LiveStepItem[]>(EMPTY_LIVE_STEPS);
  const [stepsOpen, setStepsOpen] = useState(false);
  const [activeArtifact, setActiveArtifact] = useState<ArtifactItem>(EMPTY_ARTIFACT);
  const [scripts, setScripts] = useState<ScriptItem[]>(EMPTY_SCRIPTS);
  const [schedules, setSchedules] = useState<ScheduleItem[]>(EMPTY_SCHEDULES);
  const [magpieMode, setMagpieMode] = useState<"magpie_managed" | "standalone_native">("magpie_managed");
  const [daemonOnline, setDaemonOnline] = useState<boolean>(false);
  const [seqCursor, setSeqCursor] = useState<number>(0);
  const [toastMsg, setToastMsg] = useState<string | null>(null);
  const [lostControlNote, setLostControlNote] = useState<string | null>(null);

  const [cli, setCli] = useState<string>("codex");
  const [model, setModel] = useState<string>("group/gpt-6.2-sol");
  const [thinking, setThinking] = useState<string>("medium");
  const [permLevel, setPermLevel] = useState<PermissionLevel>(prefs.defaultPermMode);
  const [behavior, setBehavior] = useState<TurnBehavior>("steer");
  const [queuedPrompts, setQueuedPrompts] = useState<QueuedPromptItem[]>([]);
  const [queuePaused, setQueuePaused] = useState<boolean>(false);
  const [composerCollapsed, setComposerCollapsed] = useState<boolean>(false);
  const [showScrollBottom, setShowScrollBottom] = useState<boolean>(false);
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
  const [rightInspectorTab, setRightInspectorTab] = useState<"diff" | "artifact" | "scripts">("diff");
  const [selectedDiffFilePath, setSelectedDiffFilePath] = useState<string>("");
  const [diffFilter, setDiffFilter] = useState<string>("");
  const [diffBase, setDiffBase] = useState<"uncommitted" | "main" | "checkpoint">("uncommitted");
  const [searchQuery, setSearchQuery] = useState("");
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [settingsSection, setSettingsSection] = useState<SettingsSectionId>("appearance");
  const [pairCode, setPairCode] = useState<string>("");
  const [newBranchName, setNewBranchName] = useState<string>("feat/19-mobile-remote");
  const [newQuickTitle, setNewQuickTitle] = useState<string>("");

  // ---------- 真实 Daemon 链路状态 ----------
  // pairing 为 null 表示未认证（展示引导页）；非 null 后由 client effect 建立 WS。
  const [pairing, setPairing] = useState<StoredPairing | null>(() => loadStoredPairing());
  const [connPhase, setConnPhase] = useState<ConnectionPhase>("offline");
  const [connDetail, setConnDetail] = useState<string | null>(null);
  const [offerHint, setOfferHint] = useState<string | null>(null);
  const [pairError, setPairError] = useState<string | null>(null);
  const [pairingBusy, setPairingBusy] = useState(false);
  const clientRef = useRef<DaemonClient | null>(null);
  const [sessionsState, setSessionsState] = useState<import("@/lib/daemon-protocol").SessionSummary[]>([]);
  const sessionsStateRef = useRef<import("@/lib/daemon-protocol").SessionSummary[]>([]);
  sessionsStateRef.current = sessionsState;
  // 当前选中的目标（工作区/快速对话）：WS 回调只创建一次，必须经 ref 读取最新选择
  const selectedTargetRef = useRef<{ kind: "workspace" | "quick"; id: string }>({
    kind: "workspace",
    id: "",
  });
  selectedTargetRef.current =
    targetKind === "workspace"
      ? { kind: "workspace", id: selectedWorkspaceId }
      : { kind: "quick", id: selectedQuickChatId };

  /**
   * 把单条事件应用到视图：只更新事件真正影响的切片，未受影响的列表（如待审批
   * 卡、提问卡、其它工作区）保持原样；游标在事件应用之后才推进。
   */
  function applyIncomingEvent(env: EventEnvelope) {
    const updaters = eventToViewUpdaters(env);
    if (updaters.workspaces) setWorkspaces(updaters.workspaces);
    if (updaters.quickChats) setQuickChats(updaters.quickChats);
    if (updaters.approvals) setApprovals(updaters.approvals);
    if (updaters.askQuestion) setAskQuestion(updaters.askQuestion);
    if (updaters.sessionMessage) {
      const { session_id, message } = updaters.sessionMessage;
      const target = selectedTargetRef.current;
      const session = sessionsStateRef.current.find((s) => s.session_id === session_id);
      const isCurrent = session
        ? session.target.kind === "workspace"
          ? target.kind === "workspace" && session.target.workspace_id === target.id
          : target.kind === "quick" && session.target.chat_id === target.id
        : false;
      if (isCurrent) setTurns((cur) => upsertMappedTurn(cur, message));
    }
    setSeqCursor((prev) => Math.max(prev, env.seq));
  }
  // 6 位码配对目标端点：默认当前页面来源（经 rust-embed 伺服时即 daemon 本身），可手动改写
  const [daemonEndpoint, setDaemonEndpoint] = useState<string>(() => window.location.origin);

  useEffect(() => {
    const resolved = applyPreferencesToDom(prefs);
    setResolvedTheme(resolved);
    savePreferences(prefs);
  }, [prefs]);

  useEffect(() => {
    if (sizeClass === "wide" || isFoldDualH) {
      setLeftDrawerOpen(false);
    }
  }, [sizeClass, isFoldDualH]);

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === ",") {
        e.preventDefault();
        setSettingsSection("appearance");
        setSettingsOpen((v) => !v);
      } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setSheet((prev) => (prev === "search" ? null : "search"));
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
    };
  }, []);

  // ---------- 真实 Daemon 链路：#offer= 自动配对 / WS 连接 / 增量事件应用 ----------

  // 1. 首次挂载时检测 #offer= 片段（扫码进入），自动消费一次性令牌完成配对
  useEffect(() => {
    const offer = consumeOfferFromLocation();
    if (!offer) return;
    setOfferHint(`检测到配对链接（守护进程指纹 ${offer.daemon_fingerprint.slice(0, 8)}…），正在自动配对…`);
    setPairingBusy(true);
    claimPairing(offer.endpoint_url, { kind: "offer_token", value: offer.one_time_token })
      .then((claimed) => {
        const next: StoredPairing = {
          endpoint_url: offer.endpoint_url,
          bearer_token: claimed.bearer_token,
          device_id: claimed.device_id,
          daemon_fingerprint: claimed.daemon_fingerprint,
        };
        saveStoredPairing(next);
        setPairing(next);
        setOfferHint(null);
        flashToast("已通过扫码链接完成配对");
      })
      .catch((err: unknown) => {
        setOfferHint(null);
        setPairError(err instanceof Error ? err.message : "扫码配对失败");
      })
      .finally(() => setPairingBusy(false));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // 2. 已配对时建立 WS 连接：握手快照 → 实时事件应用（含增量追平由 Daemon 按 after_seq 完成）
  useEffect(() => {
    if (!pairing) return;
    const client = new DaemonClient(
      pairing,
      {
        onPhase: (phase, detail) => {
          setConnPhase(phase);
          setConnDetail(detail ?? null);
          setDaemonOnline(phase === "online");
        },
        onServerHello: (hello: ServerHello) => {
          const mapped = mapSnapshot(hello.snapshot);
          setWorkspaces(mapped.workspaces);
          setQuickChats(mapped.quickChats);
          setApprovals(mapped.approvals);
          setAskQuestion(mapped.askQuestion);
          setSessionsState(hello.snapshot.sessions);
          setMagpieMode(hello.magpie_detected ? "magpie_managed" : "standalone_native");
          // 以守护进程下发的快照头部为基线；旧版守护进程缺字段时回退 latest_seq
          setSeqCursor(hello.snapshot_seq ?? hello.latest_seq);
          const first = pickDefaultSession(hello.snapshot.sessions);
          if (first) {
            setTargetKind(first.target.kind === "workspace" ? "workspace" : "quick");
            if (first.target.kind === "workspace") setSelectedWorkspaceId(first.target.workspace_id);
            else setSelectedQuickChatId(first.target.chat_id);
          }
        },
        onCatchUpEvents: () => {},
        onLiveEvent: (env: EventEnvelope) => {
          applyIncomingEvent(env);
        },
      },
      0,
    );
    clientRef.current = client;
    client.connect();
    return () => {
      client.close();
      clientRef.current = null;
    };
    // pairing 变化（新配对/解除）时重建连接；选中会话变化不重建
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pairing]);

  useEffect(() => {
    const onAltPerm = (e: KeyboardEvent) => {
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
    window.addEventListener("keydown", onAltPerm);
    return () => window.removeEventListener("keydown", onAltPerm);
  }, [cli]);

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
  const isRunning = targetKind === "workspace" && activeWorkspace?.bucket === "running";
  // 真实快照可能为空（新 daemon 无任何项目/对话）：渲染零会话引导态而非崩溃
  const hasNoTargets = !activeWorkspace && !activeQuickChat;

  const activeWorkspaces = workspaces.filter((w) => !w.archived);
  const pinnedWorkspaces = activeWorkspaces.filter((w) => w.pinned);
  const activeQuickChats = quickChats.filter((q) => !q.archived);
  const filteredWorkspaces =
    bucketFilter === "all" ? activeWorkspaces : activeWorkspaces.filter((w) => w.bucket === bucketFilter);
  const inboxAttentionCount =
    pendingApprovals.length +
    activeWorkspaces.filter((w) => w.bucket === "needs_input" || w.bucket === "failed" || w.bucket === "attention").length;

  const filteredDiffFiles = useMemo(() => {
    const q = diffFilter.trim().toLowerCase();
    if (!q) return EMPTY_DIFF_FILES;
    return EMPTY_DIFF_FILES.filter((f) => f.path.toLowerCase().includes(q));
  }, [diffFilter]);

  const activeDiffFile =
    filteredDiffFiles.find((f) => f.path === selectedDiffFilePath) ?? filteredDiffFiles[0] ?? EMPTY_DIFF_FILES[0];

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

  const totalDiffAdded = useMemo(() => EMPTY_DIFF_FILES.reduce((acc, f) => acc + f.added, 0), []);
  const totalDiffDeleted = useMemo(() => EMPTY_DIFF_FILES.reduce((acc, f) => acc + f.deleted, 0), []);
  const doneLiveSteps = liveSteps.filter((s) => s.status === "done").length;
  const activeLiveStep = liveSteps.find((s) => s.status === "in_progress") ?? liveSteps[0];

  const activeProfile = getAgentProfile(cli);
  const availableModels = getAgentModels(cli);
  const currentModelMeta = availableModels.find((m) => m.id === model) ?? availableModels[0];
  const handoffChip = composerChips.find((c) => c.kind === "handoff" && c.targetAgent);
  const selectedHandoff: HandoffSelection | undefined = handoffChip?.targetAgent
    ? {
        targetAgent: handoffChip.targetAgent,
        targetModel: handoffChip.targetModel ?? getAgentModels(handoffChip.targetAgent)[0].id,
        targetThinking: handoffChip.targetThinking,
      }
    : undefined;

  function simulateSeqCatchup() {
    // 真实链路：向 Daemon 请求 after_seq 增量事件追平（不再伪造游标跳动）
    const client = clientRef.current;
    if (!client || connPhase !== "online") {
      flashToast("当前未连接守护服务，无法追平事件流");
      return;
    }
    const from = client.currentSeq;
    client
      .rpc({ method: "catch_up_events", params: { after_seq: from, limit: 500 } })
      .then((resp) => {
        if (!resp.ok || resp.result?.kind !== "events") {
          flashToast("追平请求失败，请稍后重试");
          return;
        }
        const { events, latest_seq } = resp.result.data;
        for (const env of events) {
          applyIncomingEvent(env);
        }
        setSeqCursor((prev) => Math.max(prev, latest_seq));
        flashToast(`事件流增量追平完成：#${from} → #${latest_seq}（${events.length} 条）`);
      })
      .catch(() => flashToast("追平请求失败，请稍后重试"));
  }

  function handleSelectCli(nextCliId: string, override?: { model?: string; thinking?: string }, keepSheetOpen = false) {
    const nextCli = CLI_OPTIONS.find((c) => c.id === nextCliId);
    if (!nextCli || !nextCli.available) return null;
    setCli(nextCli.id);
    const reconciled = reconcileAgentCapabilities({
      agentId: nextCli.id,
      currentModel: override?.model ?? model,
      currentPerm: permLevel,
      currentThinking: override?.thinking ?? thinking,
      currentBehavior: behavior,
    });
    setModel(reconciled.model);
    setPermLevel(reconciled.perm);
    setThinking(reconciled.thinking);
    setBehavior(reconciled.behavior);
    setLostControlNote(reconciled.notes.length > 0 ? reconciled.notes.join("；") : null);
    if (!keepSheetOpen) setSheet(null);
    flashToast(`活跃驱动已切换至 ${nextCli.id} v${nextCli.ver}`);
    return reconciled;
  }

  function handleSelectModel(nextModelId: string, keepSheetOpen = false) {
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
    if (!keepSheetOpen) setSheet(null);
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

  function scrollToBottom(smooth = true) {
    const el = transcriptScrollRef.current;
    if (!el) return;
    el.scrollTo({
      top: el.scrollHeight,
      behavior: smooth && !prefs.reduceMotion ? "smooth" : "auto",
    });
  }

  function handleScrollTranscript(e: React.UIEvent<HTMLDivElement>) {
    const el = e.currentTarget;
    const distanceFromBottom = el.scrollHeight - el.scrollTop - el.clientHeight;
    setShowScrollBottom(distanceFromBottom > 80);
  }

  function handleTogglePinWorkspace(workspaceId: string) {
    setWorkspaces((prev) =>
      prev.map((w) => {
        if (w.id !== workspaceId) return w;
        const nextPinned = !w.pinned;
        flashToast(nextPinned ? `已置顶 ${w.branch}` : `已取消置顶 ${w.branch}`);
        return { ...w, pinned: nextPinned };
      }),
    );
  }

  function handleMarkAllAttentionRead() {
    setWorkspaces((prev) => prev.map((w) => (w.bucket === "attention" ? { ...w, bucket: "done" } : w)));
    flashToast("已将已完成未读工作区全部标记为已读");
  }

  function handleBranchFromTurn(turn: TurnMessage) {
    if (!activeWorkspace) return;
    const id = `wks-fork-${Date.now().toString().slice(-4)}`;
    const branch = `feat/fork-${turn.id.replace(/[^a-z0-9]/gi, "").slice(-6)}`;
    const forked: WorkspaceItem = {
      id,
      project: activeWorkspace.project,
      projectPath: activeWorkspace.projectPath,
      branch,
      isolation: "worktree",
      agent: cli,
      model,
      bucket: "attention",
      summary: `自回合「${turn.text.slice(0, 24)}...」分叉的新隔离工作树`,
      activeIntent: "延续分叉前上下文",
      diffStat: "+0 -0",
    };
    setWorkspaces((prev) => [forked, ...prev]);
    setSelectedWorkspaceId(id);
    setTargetKind("workspace");
    flashToast(`已从该回合分叉新会话：${branch}`);
  }

  function handleSubmitDiffLineComment(filePath: string, lineNum: number, codeLine: string, comment: string) {
    addComposerChip({
      kind: "review",
      label: `${filePath}:${lineNum} ${comment}`,
      detail: `${codeLine}\n评审意见：${comment}`,
    });
    setComposerCollapsed(false);
    setRightDrawerOpen(false);
    flashToast(`已附着第 ${lineNum} 行评审批注至输入舱`);
    window.setTimeout(() => composerInputRef.current?.focus(), 60);
  }

  function handlePromoteSteerItem(item: QueuedPromptItem) {
    const nextSeq = seqCursor + 2;
    setSeqCursor(nextSeq);
    setTurns((prev) => [
      ...prev,
      { id: `turn-q-${nextSeq}`, role: "user", author: "你", text: item.text, badge: "提级为热纠偏" },
      {
        id: `turn-qa-${nextSeq + 1}`,
        role: "assistant",
        author: `${cli} · ${model}`,
        text: `已提级为热纠偏，优先处理排队指令：「${item.text}」。`,
      },
    ]);
    setQueuedPrompts((prev) => prev.filter((q) => q.id !== item.id));
    flashToast("已将排队指令提级为热纠偏注入当前回合");
  }

  function handleEditQueueItem(item: QueuedPromptItem) {
    setDraft(item.text);
    setQueuedPrompts((prev) => prev.filter((q) => q.id !== item.id));
    setComposerCollapsed(false);
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

  function handleSubmitPrompt(overrideBehavior?: TurnBehavior, overrideText?: string) {
    const activeBehavior = overrideBehavior ?? behavior;
    const isDirectChip = typeof overrideText === "string";
    const rawText = isDirectChip ? overrideText.trim() : draft.trim();
    const attachedChips = isDirectChip ? [] : composerChips;
    if (!rawText && attachedChips.length === 0) return;

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
      if (nextCfg) {
        effectiveCli = activeHandoff.targetAgent;
        effectiveModel = nextCfg.model;
        effectiveThinking = nextCfg.thinking;
        effectivePerm = nextCfg.perm;
      }
    }

    const permLabel = getPermissionMeta(effectiveCli, effectivePerm).label;
    const targetProfile = getAgentProfile(effectiveCli);
    const thinkDesc = targetProfile.supportsThinking
      ? `思考：${THINKING_LABELS[effectiveThinking] ?? effectiveThinking}，`
      : "";
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
        author: `${effectiveCli} · ${effectiveModel}`,
        badge: activeHandoff ? `@${effectiveCli} 已接管` : undefined,
        text: activeHandoff
          ? `已在无项目快速对话中移交至 @${effectiveCli}（${effectiveModel}，${thinkDesc}${permLabel}）并继续响应。`
          : `已在无项目快速对话下响应（${permLabel}）：收到你的指令「${text}」。`,
      };
      setQuickChats((prev) =>
        prev.map((q) =>
          q.id === activeQuickChat.id ? { ...q, updatedAt: "刚刚", messages: [...q.messages, uMsg, aMsg] } : q,
        ),
      );
      setContextTokens((cur) => cur + Math.max(1200, text.length * 40));
      if (!isDirectChip) {
        setComposerChips([]);
        setDraft("");
      }
      window.setTimeout(() => scrollToBottom(true), 40);
      return;
    }

    if (!activeHandoff) {
      if (activeBehavior === "queue") {
        setQueuedPrompts((prev) => [
          ...prev,
          {
            id: `mob-q-${Date.now()}-${Math.random().toString(36).slice(2, 5)}`,
            text,
            createdAt: "刚刚",
            mode: "queue",
          },
        ]);
        if (!isDirectChip) {
          setComposerChips([]);
          setDraft("");
        }
        flashToast("已加入指令队列，当前回合结束时顺延发送");
        return;
      }
      if (isRunning && activeBehavior === "interrupt" && activeWorkspace) {
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
      badge: activeHandoff ? `移交 -> @${effectiveCli}` : undefined,
    };
    const replyTurn: TurnMessage = {
      id: `turn-a-${nextSeq + 1}`,
      role: "assistant",
      author: `${effectiveCli} · ${effectiveModel}`,
      badge: activeHandoff ? `@${effectiveCli} 接管` : undefined,
      reasoningSec: "1.1 秒",
      reasoningSteps: [
        "1. 读取工作区检查点确保修改严格落在目标范围。",
        "2. 在沙箱隔离边界内验证测试结果，绝不修改全局外部配置。",
      ],
      text: activeHandoff
        ? `已接收跨代理移交上下文与 ${attachedChips.length} 项标签，切换至 @${effectiveCli}（${effectiveModel}，${thinkDesc}${permLabel}）继续执行。`
        : activeBehavior === "steer"
          ? `已在不中断当前进程前提下热纠偏：收到指令「${text}」，正在应用至活跃上下文。`
          : `已按「${permLabel}」策略接收新指令，正在执行检验。`,
    };
    setTurns((prev) => [...prev, userTurn, replyTurn]);
    setContextTokens((cur) => cur + Math.max(1800, text.length * 45));
    if (!isDirectChip) {
      setComposerChips([]);
      setDraft("");
    }
    window.setTimeout(() => scrollToBottom(true), 40);
  }

  function handleAnswerAsk(optionId: string) {
    const picked = askQuestion.options.find((o) => o.id === optionId);
    if (!picked) return;
    setAskQuestion((prev) => ({ ...prev, selectedId: optionId, submitted: true }));
    setSeqCursor((s) => s + 1);
    flashToast(`已回传确认结果：「${picked.label}」`);
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
        "flex flex-col gap-3.5 p-3.5 shrink-0",
        sizeClass === "wide"
          ? "w-[300px] border-r border-[var(--soxu-border-hairline)] bg-[var(--soxu-surface-sidebar)]"
          : "w-full",
      )}
    >
      {/* 0. 置顶收件箱分诊台入口 */}
      <button
        type="button"
        onClick={() => {
          setActiveTab("inbox");
          setLeftDrawerOpen(false);
        }}
        className={cn(
          "w-full min-h-[38px] rounded-xl px-3 py-2 flex items-center justify-between gap-2 text-xs font-medium border cursor-pointer transition-colors",
          activeTab === "inbox" || activeTab === "approvals"
            ? "bg-[var(--soxu-interaction-selected)] border-[var(--soxu-border-strong)] text-[var(--soxu-text-primary)]"
            : "bg-[var(--soxu-surface-card)] border-[var(--soxu-border-subtle)] text-[var(--soxu-text-secondary)] hover:text-[var(--soxu-text-primary)]",
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

      {/* 0.5 已置顶重要工作区 */}
      {pinnedWorkspaces.length > 0 && (
        <div className="flex flex-col gap-1.5">
          <div className="flex items-center justify-between px-0.5">
            <span className="text-[11px] font-semibold text-[var(--soxu-text-tertiary)] flex items-center gap-1">
              <Star className="h-3 w-3 text-[var(--soxu-semantic-warning)] fill-current" strokeWidth={1.75} />
              <span>{t("sidebar.pinned")}</span>
            </span>
          </div>
          {pinnedWorkspaces.map((w) => {
            const active = targetKind === "workspace" && w.id === selectedWorkspaceId && activeTab === "session";
            const meta = bucketMeta(w.bucket);
            return (
              <div
                key={`pin-${w.id}`}
                role="button"
                tabIndex={0}
                data-group-active={active ? "true" : undefined}
                onClick={() => {
                  setTargetKind("workspace");
                  setSelectedWorkspaceId(w.id);
                  setLeftDrawerOpen(false);
                  setActiveTab("session");
                }}
                onKeyDown={(e) => {
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    setTargetKind("workspace");
                    setSelectedWorkspaceId(w.id);
                    setActiveTab("session");
                  }
                }}
                className={cn(
                  "group rounded-xl px-2.5 py-2 flex items-center justify-between gap-2 border cursor-pointer transition-colors",
                  active
                    ? "bg-[var(--soxu-interaction-selected)] border-[var(--soxu-border-strong)]"
                    : "bg-[var(--soxu-surface-card)] border-[var(--soxu-border-subtle)]",
                )}
              >
                <div className="min-w-0 flex items-center gap-2">
                  <span className={cn("h-2 w-2 rounded-full shrink-0", meta.dotClass)} />
                  <div className="min-w-0">
                    <div className="soxu-mono text-[11.5px] font-semibold truncate">{w.branch}</div>
                    {w.activeIntent && (
                      <div className="text-[10.5px] text-[var(--soxu-semantic-running)] truncate">{w.activeIntent}</div>
                    )}
                  </div>
                </div>
                <button
                  type="button"
                  aria-label={t("sidebar.unpinAction")}
                  onClick={(e) => {
                    e.stopPropagation();
                    handleTogglePinWorkspace(w.id);
                  }}
                  className="soxu-hit-44 soxu-reveal-action p-1 text-[var(--soxu-semantic-warning)] bg-transparent border-0 cursor-pointer shrink-0"
                >
                  <Star className="h-3 w-3 fill-current" strokeWidth={1.75} />
                </button>
              </div>
            );
          })}
        </div>
      )}

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
        <div className="flex flex-col gap-2">
          {activeQuickChats.map((qc) => {
            const active = targetKind === "quick" && selectedQuickChatId === qc.id && activeTab === "session";
            const meta = bucketMeta(qc.bucket);
            return (
              <div
                key={qc.id}
                data-group-active={active ? "true" : undefined}
                onClick={() => {
                  setTargetKind("quick");
                  setSelectedQuickChatId(qc.id);
                  setLeftDrawerOpen(false);
                  setActiveTab("session");
                }}
                role="button"
                tabIndex={0}
                onKeyDown={(e) => {
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    setTargetKind("quick");
                    setSelectedQuickChatId(qc.id);
                    setActiveTab("session");
                  }
                }}
                className={cn(
                  "group rounded-xl px-3 py-2.5 flex items-center justify-between gap-2 border cursor-pointer transition-colors",
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
                  className="soxu-hit-44 soxu-reveal-action h-7 w-7 rounded-lg inline-flex items-center justify-center text-[var(--soxu-text-tertiary)] hover:text-[var(--soxu-text-primary)] hover:bg-[var(--soxu-surface-control)] bg-transparent border-0 cursor-pointer shrink-0"
                >
                  <Archive className="h-3.5 w-3.5" strokeWidth={1.75} />
                </button>
              </div>
            );
          })}
        </div>
      </div>

      {/* 2. 项目文件夹 -> 工作树层级 */}
      <div className="flex flex-col gap-2.5">
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
                "px-3 py-1.5 rounded-full text-xs font-medium whitespace-nowrap border cursor-pointer transition-colors",
                bucketFilter === b.id
                  ? "bg-[var(--soxu-action-inverse)] text-[var(--soxu-action-on-inverse)] border-transparent font-semibold"
                  : "bg-[var(--soxu-surface-card)] text-[var(--soxu-text-secondary)] border-[var(--soxu-border-subtle)]",
              )}
            >
              {b.label}
            </button>
          ))}
        </div>

        <div className="flex flex-col gap-2.5">
          {projectGroups.map(([projName, group]) => {
            const expanded = openProjects[projName] ?? true;
            return (
              <div key={projName} className="flex flex-col gap-1.5">
                <button
                  type="button"
                  onClick={() => setOpenProjects((p) => ({ ...p, [projName]: !expanded }))}
                  className="flex items-center justify-between px-1.5 py-1.5 text-xs font-semibold text-[var(--soxu-text-primary)] bg-transparent border-0 cursor-pointer"
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
                  <div className="ml-3 pl-2.5 border-l border-[var(--soxu-border-hairline)] flex flex-col gap-2">
                    {group.items.map((w) => {
                      const active = targetKind === "workspace" && w.id === selectedWorkspaceId && activeTab === "session";
                      const meta = bucketMeta(w.bucket);
                      return (
                        <div
                          key={w.id}
                          role="button"
                          tabIndex={0}
                          data-group-active={active ? "true" : undefined}
                          onClick={() => {
                            setTargetKind("workspace");
                            setSelectedWorkspaceId(w.id);
                            setLeftDrawerOpen(false);
                            setActiveTab("session");
                          }}
                          onKeyDown={(e) => {
                            if (e.key === "Enter" || e.key === " ") {
                              e.preventDefault();
                              setTargetKind("workspace");
                              setSelectedWorkspaceId(w.id);
                              setActiveTab("session");
                            }
                          }}
                          className={cn(
                            "group w-full text-left rounded-xl p-3 flex flex-col gap-1.5 border cursor-pointer transition-colors",
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
                                aria-label={w.pinned ? t("sidebar.unpinAction") : t("sidebar.pinAction")}
                                onClick={(e) => {
                                  e.stopPropagation();
                                  handleTogglePinWorkspace(w.id);
                                }}
                                className={cn(
                                  "soxu-hit-44 soxu-reveal-action h-7 w-7 rounded-lg inline-flex items-center justify-center hover:bg-[var(--soxu-surface-control)] bg-transparent border-0 cursor-pointer",
                                  w.pinned ? "opacity-100 text-[var(--soxu-semantic-warning)]" : "text-[var(--soxu-text-tertiary)] hover:text-[var(--soxu-text-primary)]",
                                )}
                              >
                                <Star className={cn("h-3.5 w-3.5", w.pinned && "fill-current")} strokeWidth={1.75} />
                              </button>
                              <button
                                type="button"
                                aria-label={`归档 ${w.branch}`}
                                onClick={(e) => {
                                  e.stopPropagation();
                                  archiveItem("workspace", w.id, `${w.project} / ${w.branch}`);
                                }}
                                className="soxu-hit-44 soxu-reveal-action h-7 w-7 rounded-lg inline-flex items-center justify-center text-[var(--soxu-text-tertiary)] hover:text-[var(--soxu-text-primary)] hover:bg-[var(--soxu-surface-control)] bg-transparent border-0 cursor-pointer"
                              >
                                <Archive className="h-3.5 w-3.5" strokeWidth={1.75} />
                              </button>
                            </div>
                          </div>
                          {w.activeIntent ? (
                            <div className="text-[11px] text-[var(--soxu-semantic-running)] truncate">{w.activeIntent}</div>
                          ) : (
                            <div className="text-[11.5px] text-[var(--soxu-text-secondary)] truncate">{w.summary}</div>
                          )}
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
            className="w-full px-1 py-1 flex items-center justify-between text-xs text-[var(--soxu-text-tertiary)] bg-transparent border-0 cursor-pointer"
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
                className="text-[11px] text-[var(--soxu-semantic-accent)] inline-flex items-center gap-1 shrink-0 bg-transparent border-0 cursor-pointer"
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

  function renderRightInspectorBody(showCloseButton = false) {
    return (
      <div className="flex-1 min-h-0 flex flex-col bg-[var(--soxu-surface-sidebar)]">
        <div className="px-3.5 py-2.5 border-b border-[var(--soxu-border-hairline)] flex items-center justify-between gap-2 shrink-0">
          <div className="inline-flex rounded-lg bg-[var(--soxu-surface-inset)] p-0.5 border border-[var(--soxu-border-hairline)]">
            {(
              [
                { id: "diff", label: t("inspector.diff") },
                { id: "artifact", label: t("inspector.artifact") },
                { id: "scripts", label: t("inspector.scripts") },
              ] as const
            ).map((item) => (
              <button
                key={item.id}
                type="button"
                onClick={() => setRightInspectorTab(item.id)}
                className={cn(
                  "px-2.5 py-1 rounded-md text-xs border-0 bg-transparent cursor-pointer transition-colors",
                  rightInspectorTab === item.id
                    ? "bg-[var(--soxu-surface-raised)] text-[var(--soxu-text-primary)] font-medium"
                    : "text-[var(--soxu-text-tertiary)]",
                )}
              >
                {item.label}
              </button>
            ))}
          </div>
          {showCloseButton && (
            <Button
              variant="ghost"
              size="icon-sm"
              onClick={() => setRightDrawerOpen(false)}
              aria-label={t("header.toggleRight")}
            >
              <X className="h-4 w-4" strokeWidth={1.75} />
            </Button>
          )}
        </div>

        <div className="flex-1 min-h-0 overflow-y-auto p-3.5 flex flex-col gap-3 soxu-scroll">
          {rightInspectorTab === "diff" && (
            <>
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
                    className="w-full rounded-xl pl-8 pr-3 py-1.5 text-base sm:text-xs bg-[var(--soxu-surface-composer)] border border-[var(--soxu-border-subtle)] text-[var(--soxu-text-primary)] placeholder:text-[var(--soxu-text-ghost)] outline-none"
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
                        "px-2 py-1 rounded-lg text-[11px] whitespace-nowrap border cursor-pointer transition-colors",
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
                    const selected = file.path === activeDiffFile?.path;
                    return (
                      <button
                        key={file.path}
                        type="button"
                        onClick={() => setSelectedDiffFilePath(file.path)}
                        className={cn(
                          "w-full rounded-lg px-2.5 py-1.5 text-left text-[11.5px] flex items-center justify-between gap-2 border cursor-pointer transition-colors",
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
                {activeDiffFile ? (
                  <>
                    {activeDiffFile.path} · {t("inspector.baseBranchLabel", { branch: diffBase === "main" ? "origin/main" : diffBase === "checkpoint" ? "#04" : prefs.defaultBranch })}
                  </>
                ) : (
                  <span>协议 v1 尚未提供会话 Diff 事件，暂无变更可展示</span>
                )}
              </div>
              {activeDiffFile && (
                <CodeDiffView
                  filePath={activeDiffFile.path}
                  rows={activeDiffFile.rows}
                  layout="unified"
                  showLineNumbers={prefs.showLineNumbers}
                  wrap={prefs.codeWordWrap}
                  onSubmitLineComment={handleSubmitDiffLineComment}
                />
              )}
            </>
          )}

          {rightInspectorTab === "artifact" && (
            <div className="rounded-2xl p-4 bg-[var(--soxu-surface-card)] border border-[var(--soxu-border-subtle)] flex flex-col gap-3">
              <div className="flex items-start justify-between gap-2">
                <div className="min-w-0">
                  <div className="text-sm font-semibold leading-snug">{activeArtifact.title}</div>
                  <div className="text-[11px] soxu-mono text-[var(--soxu-text-tertiary)] truncate mt-0.5">
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
                  variant="default"
                  size="lg"
                  onClick={() => {
                    setActiveArtifact((prev) => ({ ...prev, status: "approved" }));
                    setLiveSteps((prev) =>
                      prev.map((st, idx) => (idx <= 2 ? { ...st, status: "done" } : idx === 3 ? { ...st, status: "in_progress" } : st)),
                    );
                    flashToast("已批准实施产物文档并推进后续步骤");
                  }}
                >
                  <Check className="h-4 w-4" strokeWidth={2} />
                  <span>{t("artifact.approve")}</span>
                </Button>
              )}
            </div>
          )}

          {rightInspectorTab === "scripts" && ScriptsAndSchedulesView}
        </div>
      </div>
    );
  }

  const activeScopeTitle = targetKind === "quick"
    ? activeQuickChat?.title ?? t("tabs.newQuick")
    : activeWorkspace
      ? `${activeWorkspace.project} / ${activeWorkspace.branch}`
      : t("tabs.emptyTitle");

  const isFloatingHeader = sizeClass !== "wide";
  const isWideComposerRow = !isFoldDualH && vp.width >= 680;

  const HeaderBar = isFloatingHeader ? (
    <header
      className={cn(
        "absolute top-0 inset-x-0 pointer-events-none soxu-safe-top px-3 pb-2 bg-gradient-to-b from-[var(--soxu-surface-canvas)]/90 via-[var(--soxu-surface-canvas)]/45 to-transparent",
        vp.isShortViewport && "pt-2 pb-1.5",
      )}
      style={{ zIndex: "var(--soxu-z-header)" }}
    >
      <div className="max-w-[1280px] mx-auto flex items-center justify-between gap-2.5">
        {!showCompactRail ? (
          <button
            ref={leftTriggerRef}
            type="button"
            onClick={() => setLeftDrawerOpen((v) => !v)}
            aria-label={t("header.openLeftDrawer")}
            className="pointer-events-auto h-10 w-10 shrink-0 rounded-full bg-[var(--soxu-surface-floating)]/85 backdrop-blur-xl border border-[var(--soxu-border-subtle)] shadow-md inline-flex items-center justify-center text-[var(--soxu-text-primary)] hover:border-[var(--soxu-border-strong)] cursor-pointer transition-colors"
          >
            <PanelLeft className="h-4 w-4" strokeWidth={1.75} />
          </button>
        ) : (
          <span className="w-1 shrink-0" aria-hidden="true" />
        )}

        {/* 居中悬浮毛玻璃双行药丸：上行当前分支/会话标题，下行驱动+品牌+状态+游标（点击检索切换） */}
        <button
          type="button"
          onClick={() => setSheet("search")}
          aria-label={t("header.searchCompact")}
          className="pointer-events-auto flex-1 min-w-0 max-w-[320px] mx-auto h-10 px-3.5 rounded-full bg-[var(--soxu-surface-floating)]/85 backdrop-blur-xl border border-[var(--soxu-border-subtle)] shadow-md flex flex-col items-center justify-center text-center cursor-pointer hover:border-[var(--soxu-border-strong)] transition-colors"
        >
          <div className="text-[12px] font-semibold text-[var(--soxu-text-primary)] truncate max-w-full leading-tight">
            {activeScopeTitle}
          </div>
          <div className="text-[10px] text-[var(--soxu-text-tertiary)] flex items-center justify-center gap-1.5 leading-tight mt-0.5 max-w-full">
            <AgentIcon agent={cli} className="h-3 w-3 shrink-0" />
            <span className="shrink-0">{t("brand.name")}</span>
            <span
              className={cn(
                "h-1.5 w-1.5 rounded-full shrink-0",
                isRunning ? "bg-[var(--soxu-semantic-running)] soxu-live-dot" : "bg-[var(--soxu-semantic-success)]",
              )}
            />
            <span className="shrink-0">{isRunning ? t("status.running") : "就绪"}</span>
            <span className="soxu-mono soxu-tabular shrink-0">#{seqCursor}</span>
          </div>
        </button>

        {/* 右圆悬浮毛玻璃四格宫格菜单（收敛次级操作，保持零拥挤大热区） */}
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <button
              type="button"
              aria-label={t("header.moreTools")}
              className="pointer-events-auto relative h-10 w-10 shrink-0 rounded-full bg-[var(--soxu-surface-floating)]/85 backdrop-blur-xl border border-[var(--soxu-border-subtle)] shadow-md inline-flex items-center justify-center text-[var(--soxu-text-primary)] hover:border-[var(--soxu-border-strong)] cursor-pointer transition-colors"
            >
              <LayoutGrid className="h-4 w-4" strokeWidth={1.75} />
              {inboxAttentionCount > 0 && (
                <span className="absolute top-0.5 right-0.5 min-w-[14px] h-[14px] px-1 rounded-full text-[9px] leading-[14px] text-center bg-[var(--soxu-semantic-warning)] text-black font-bold">
                  {inboxAttentionCount}
                </span>
              )}
            </button>
          </DropdownMenuTrigger>
          <DropdownMenuContent side="bottom" align="end" className="w-56">
            <DropdownMenuLabel className="flex items-center justify-between">
              <span>{t("brand.name")} 快捷菜单</span>
              <span className="soxu-mono text-[10px] text-[var(--soxu-text-tertiary)]">#{seqCursor}</span>
            </DropdownMenuLabel>
            <DropdownMenuSeparator />
            <DropdownMenuItem onClick={() => setActiveTab((prev) => (prev === "inbox" ? "session" : "inbox"))}>
              <span className="inline-flex items-center gap-2">
                <Inbox className="h-3.5 w-3.5 text-[var(--soxu-semantic-accent)]" strokeWidth={1.75} />
                <span>{t("mobile.navInbox")}</span>
              </span>
              {inboxAttentionCount > 0 && <Badge variant="warning">{inboxAttentionCount}</Badge>}
            </DropdownMenuItem>
            {!isFoldDualH && (
              <DropdownMenuItem onClick={() => setRightDrawerOpen((v) => !v)}>
                <span className="inline-flex items-center gap-2">
                  <PanelRight className="h-3.5 w-3.5" strokeWidth={1.75} />
                  <span>{t("inspector.title")}</span>
                </span>
              </DropdownMenuItem>
            )}
            <DropdownMenuItem onClick={simulateSeqCatchup}>
              <span className="inline-flex items-center gap-2">
                <RotateCcw className="h-3.5 w-3.5" strokeWidth={1.75} />
                <span>游标增量追平 (#{seqCursor})</span>
              </span>
            </DropdownMenuItem>
            <DropdownMenuItem onClick={(e) => handleToggleTheme(e.currentTarget)}>
              <span className="inline-flex items-center gap-2">
                {resolvedTheme === "dark" ? (
                  <Sun className="h-3.5 w-3.5" strokeWidth={1.75} />
                ) : (
                  <Moon className="h-3.5 w-3.5" strokeWidth={1.75} />
                )}
                <span>{resolvedTheme === "dark" ? "切换至浅色主题" : "切换至纯黑主题"}</span>
              </span>
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem onClick={() => setSheet("pair")}>
              <span className="inline-flex items-center gap-2">
                <QrCode className="h-3.5 w-3.5" strokeWidth={1.75} />
                <span>{t("header.pair")}</span>
              </span>
            </DropdownMenuItem>
            <DropdownMenuItem onClick={() => setSheet("history")}>
              <span className="inline-flex items-center gap-2">
                <History className="h-3.5 w-3.5" strokeWidth={1.75} />
                <span>{t("sidebar.importHistory")}</span>
              </span>
            </DropdownMenuItem>
            <DropdownMenuItem onClick={() => setSheet("new-worktree")}>
              <span className="inline-flex items-center gap-2">
                <GitBranch className="h-3.5 w-3.5" strokeWidth={1.75} />
                <span>新建隔离工作树</span>
              </span>
            </DropdownMenuItem>
            <DropdownMenuItem onClick={() => setSheet("new-quick-chat")}>
              <span className="inline-flex items-center gap-2">
                <MessageSquare className="h-3.5 w-3.5" strokeWidth={1.75} />
                <span>{t("sidebar.newQuickChat")}</span>
              </span>
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem
              onClick={() => {
                setSettingsSection("appearance");
                setSettingsOpen(true);
              }}
            >
              <span className="inline-flex items-center gap-2">
                <Settings className="h-3.5 w-3.5" strokeWidth={1.75} />
                <span>{t("header.settings")}</span>
              </span>
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>

      {toastMsg && (
        <div className="pointer-events-auto max-w-[420px] mx-auto mt-2 px-3 py-1.5 rounded-full text-xs flex items-center justify-center gap-1.5 bg-[var(--soxu-surface-floating)]/95 backdrop-blur-md border border-[var(--soxu-border-strong)] text-[var(--soxu-semantic-accent)] shadow-lg">
          <Bolt className="h-3.5 w-3.5 shrink-0" strokeWidth={1.75} />
          <span className="truncate">{toastMsg}</span>
        </div>
      )}
    </header>
  ) : (
    <header
      className="sticky top-0 shrink-0 min-h-[48px] soxu-safe-top soxu-safe-x pb-2 backdrop-blur-md border-b border-[var(--soxu-border-hairline)] bg-[var(--soxu-surface-header-glass)] overflow-hidden"
      style={{ zIndex: "var(--soxu-z-header)" }}
    >
      <div className="max-w-[1280px] mx-auto grid items-center gap-2.5 grid-cols-[minmax(max-content,1fr)_minmax(160px,360px)_minmax(max-content,1fr)]">
        <div className="flex items-center gap-2 min-w-0 justify-self-start">
          <span className="px-1 text-[13.5px] font-semibold tracking-tight text-[var(--soxu-text-primary)] shrink-0">
            {t("brand.name")}
          </span>

          <button
            type="button"
            onClick={simulateSeqCatchup}
            className="hidden md:flex flex-col text-left min-w-0 bg-transparent border-0 p-0 cursor-pointer"
            title="点击模拟断线重连后游标增量追平"
          >
            <div className="text-[11px] soxu-mono soxu-tabular text-[var(--soxu-text-tertiary)] truncate">
              {daemonOnline ? t("mobile.daemonOnline") : connDetail ? `${t("mobile.remoteClient")} · ${connDetail}` : t("mobile.remoteClient")} · {t("mobile.cursorPrefix", { seq: seqCursor })}
            </div>
            <div className="text-[11.5px] soxu-mono text-[var(--soxu-text-secondary)] truncate max-w-[200px]">
              {activeScopeTitle}
            </div>
          </button>
        </div>

        <button
          type="button"
          onClick={() => setSheet("search")}
          className="h-8 w-full max-w-[360px] min-w-0 justify-self-center rounded-xl border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card)] px-3 flex items-center gap-2 text-xs text-[var(--soxu-text-tertiary)] hover:border-[var(--soxu-border-strong)] hover:text-[var(--soxu-text-secondary)] transition-colors cursor-pointer"
          aria-label={t("header.searchCompact")}
        >
          <Search className="h-3.5 w-3.5 shrink-0" strokeWidth={1.75} />
          <span className="truncate">{t("header.searchCompact")}</span>
        </button>

        <div className="flex items-center gap-1.5 shrink-0 justify-self-end">
          <Button
            variant={activeTab === "inbox" || activeTab === "approvals" ? "secondary" : "ghost"}
            size="icon-sm"
            onClick={() => setActiveTab((prev) => (prev === "inbox" ? "session" : "inbox"))}
            aria-label={t("mobile.navInbox")}
            title={t("mobile.navInbox")}
            className="rounded-full relative"
          >
            <Inbox className="h-4 w-4" strokeWidth={1.75} />
            {inboxAttentionCount > 0 && (
              <span className="absolute top-0 right-0 min-w-[14px] h-[14px] px-1 rounded-full text-[9px] leading-[14px] text-center bg-[var(--soxu-semantic-warning)] text-black font-bold">
                {inboxAttentionCount}
              </span>
            )}
          </Button>

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
            title={`${t("header.settings")} (${formatShortcut("Mod+,", vp.isMac)})`}
            className="rounded-full"
          >
            <Settings className="h-4 w-4" strokeWidth={1.75} />
          </Button>

          <Button
            variant={rightDrawerOpen ? "secondary" : "ghost"}
            size="icon-sm"
            onClick={() => setRightDrawerOpen((v) => !v)}
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
  );

  const MainBodyView = (
    <div
      ref={transcriptScrollRef}
      onScroll={handleScrollTranscript}
      className="flex-1 min-h-0 overflow-y-auto soxu-scroll relative"
    >
      <div
        ref={transcriptContentRef}
        className={cn(
          "w-full mx-auto px-3.5 sm:px-4 flex flex-col gap-3.5",
          isFloatingHeader ? (vp.isShortViewport ? "pt-[58px] pb-3.5" : "pt-[68px] pb-4") : "py-3.5 sm:py-4",
        )}
        style={{ maxWidth: "var(--soxu-layout-transcript-max-width)" }}
      >
        {pendingApprovals.length > 0 && activeTab !== "inbox" && activeTab !== "approvals" && (
          <button
            type="button"
            onClick={() => {
              setInboxSubMode("triage");
              setActiveTab("inbox");
            }}
            className="w-full rounded-xl px-3.5 py-2.5 text-left flex items-center justify-between gap-2 bg-[var(--soxu-semantic-warning-soft)] border border-[var(--soxu-semantic-warning-border)] text-[var(--soxu-semantic-warning)] cursor-pointer"
          >
            <span className="text-xs font-medium flex items-center gap-2 min-w-0">
              <Shield className="h-4 w-4 shrink-0" strokeWidth={1.75} />
              <span className="truncate">有 {pendingApprovals.length} 项操作待远程批准</span>
            </span>
            <span className="text-xs font-semibold shrink-0">去处理</span>
          </button>
        )}

        {sizeClass !== "wide" && activeTab === "workspaces" && WorkspaceRail}

        {(activeTab === "inbox" || activeTab === "approvals") && (
          <section className="flex flex-col gap-3">
            <div className="flex items-center justify-between gap-2 flex-wrap">
              <div className="inline-flex rounded-xl p-0.5 bg-[var(--soxu-surface-card-muted)] border border-[var(--soxu-border-subtle)]">
                <button
                  type="button"
                  onClick={() => {
                    setInboxSubMode("triage");
                    setActiveTab("inbox");
                  }}
                  className={cn(
                    "px-3 py-1.5 rounded-lg text-xs border-0 cursor-pointer transition-colors",
                    inboxSubMode === "triage" && activeTab === "inbox"
                      ? "bg-[var(--soxu-surface-raised)] text-[var(--soxu-text-primary)] font-semibold"
                      : "bg-transparent text-[var(--soxu-text-tertiary)]",
                  )}
                >
                  {t("inbox.title")}
                </button>
                <button
                  type="button"
                  onClick={() => {
                    setInboxSubMode("approvals");
                    setActiveTab("approvals");
                  }}
                  className={cn(
                    "px-3 py-1.5 rounded-lg text-xs border-0 cursor-pointer transition-colors",
                    inboxSubMode === "approvals" || activeTab === "approvals"
                      ? "bg-[var(--soxu-surface-raised)] text-[var(--soxu-text-primary)] font-semibold"
                      : "bg-transparent text-[var(--soxu-text-tertiary)]",
                  )}
                >
                  审批流明细 ({pendingApprovals.length})
                </button>
              </div>
              <Button variant="outline" size="sm" onClick={() => setActiveTab("session")}>
                返回当前会话
              </Button>
            </div>

            {inboxSubMode === "triage" && activeTab === "inbox" ? (
              <InboxTriageView
                workspaces={activeWorkspaces}
                approvals={approvals}
                askQuestion={askQuestion}
                onResolveApproval={handleResolveApproval}
                onAnswerAsk={handleAnswerAsk}
                onOpenWorkspace={(wsId) => {
                  setTargetKind("workspace");
                  setSelectedWorkspaceId(wsId);
                  setActiveTab("session");
                }}
                onMarkAllAttentionRead={handleMarkAllAttentionRead}
              />
            ) : (
              <>
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
              </>
            )}
          </section>
        )}

        {activeTab === "scripts" && ScriptsAndSchedulesView}

        {(activeTab === "session" || (sizeClass === "wide" && activeTab === "workspaces")) && (
          <>
            {targetKind === "quick" && activeQuickChat ? (
              <div className="flex flex-col gap-4">
                <div className="rounded-xl px-3.5 py-2.5 text-xs bg-[var(--soxu-surface-card-muted)] border border-[var(--soxu-border-subtle)] text-[var(--soxu-text-secondary)] flex items-center justify-between gap-2">
                  <span className="truncate">无项目自由对话 · {activeQuickChat.title}</span>
                  <Badge variant="neutral">轻量模式</Badge>
                </div>
                {activeQuickChat.messages.length === 0 ? (
                  <div className="relative py-16 px-4 rounded-2xl overflow-hidden flex flex-col items-center justify-center gap-3 text-center">
                    <div aria-hidden="true" className="pointer-events-none absolute inset-0 soxu-dot-grid opacity-80" />
                    <div className="relative h-11 w-11 rounded-2xl bg-[var(--soxu-surface-card)] border border-[var(--soxu-border-subtle)] flex items-center justify-center shadow-md">
                      <svg
                        width="22"
                        height="22"
                        viewBox="0 0 24 24"
                        fill="none"
                        className="text-[var(--soxu-text-primary)]"
                        aria-hidden="true"
                      >
                        <path
                          d="M9.5 4.5C6.5 6 5 8.8 5 12c0 3.2 1.5 6 4.5 7.5"
                          stroke="currentColor"
                          strokeWidth="2.2"
                          strokeLinecap="round"
                        />
                        <rect x="12" y="5" width="6.5" height="14" rx="2.5" fill="currentColor" />
                      </svg>
                    </div>
                    <div className="relative text-base font-semibold text-[var(--soxu-text-primary)]">要做点什么？</div>
                    <div className="relative text-xs text-[var(--soxu-text-tertiary)]">
                      在下方输入内容立即开启无项目自由对话
                    </div>
                  </div>
                ) : (
                  activeQuickChat.messages.map((m) =>
                    m.role === "user" ? (
                      <div key={m.id} className="flex flex-col items-end gap-1.5">
                        {m.chips && m.chips.length > 0 && <ComposerChipBar chips={m.chips} className="justify-end" />}
                        <div className="max-w-[85%] rounded-[18px] px-4 py-3 text-sm leading-relaxed break-words bg-[var(--soxu-surface-raised)] border border-[var(--soxu-border-subtle)] text-[var(--soxu-text-primary)]">
                          <p className="m-0">{m.text}</p>
                        </div>
                      </div>
                    ) : (
                      <article key={m.id} className="group flex flex-col gap-2">
                        <div className="flex items-center justify-between gap-2 text-xs font-semibold text-[var(--soxu-text-secondary)]">
                          <span className="inline-flex items-center gap-1.5 min-w-0">
                            <AgentIcon agent={m.author} className="h-3.5 w-3.5 shrink-0" />
                            <span className="truncate">{m.author}</span>
                            {m.badge && <Badge variant="handoff">{m.badge}</Badge>}
                          </span>
                          <button
                            type="button"
                            onClick={() => handleBranchFromTurn(m)}
                            className="soxu-reveal-action soxu-hit-44 inline-flex items-center gap-1 text-[11px] font-normal text-[var(--soxu-text-tertiary)] hover:text-[var(--soxu-text-primary)] bg-transparent border-0 cursor-pointer shrink-0"
                          >
                            <GitBranch className="h-3 w-3" strokeWidth={1.75} />
                            <span>{t("turn.branchSession")}</span>
                          </button>
                        </div>
                        <p className="text-sm m-0 leading-relaxed text-[var(--soxu-text-primary)]">{m.text}</p>
                      </article>
                    ),
                  )
                )}
              </div>
            ) : activeWorkspace ? (
              <>
                {/* 会话信息微条 */}
                <div className="flex items-center justify-between gap-2 text-xs text-[var(--soxu-text-tertiary)] pb-1 border-b border-[var(--soxu-border-hairline)]">
                  <span className="truncate">
                    {activeWorkspace.project} · {activeWorkspace.summary}
                  </span>
                  <span className="soxu-mono soxu-tabular shrink-0">{activeWorkspace.diffStat}</span>
                </div>

                {/* 实时步骤焦点（默认单行折叠展示当前进行中步骤，点击单列展开） */}
                <Collapsible
                  open={stepsOpen}
                  onOpenChange={setStepsOpen}
                  className="rounded-xl bg-[var(--soxu-surface-card-muted)] border border-[var(--soxu-border-hairline)]"
                >
                  <CollapsibleTrigger asChild>
                    <button
                      type="button"
                      className="w-full px-3.5 py-2.5 flex items-center justify-between gap-2 text-xs bg-transparent border-0 cursor-pointer"
                    >
                      <span className="inline-flex items-center gap-2 min-w-0 text-[var(--soxu-text-secondary)] font-medium">
                        <span className="h-2 w-2 rounded-full bg-[var(--soxu-semantic-running)] soxu-live-dot shrink-0" />
                        <span className="shrink-0">
                          {t("focus.title")} · {doneLiveSteps}/{liveSteps.length}
                        </span>
                        {activeLiveStep && (
                          <span className="truncate text-[var(--soxu-text-primary)] font-normal">
                            · {activeLiveStep.title}
                          </span>
                        )}
                      </span>
                      <ChevronRight
                        className={cn("h-3.5 w-3.5 text-[var(--soxu-text-tertiary)] transition-transform shrink-0", stepsOpen && "rotate-90")}
                        strokeWidth={1.75}
                      />
                    </button>
                  </CollapsibleTrigger>
                  <CollapsibleContent className="soxu-collapsible overflow-hidden px-3.5 pb-3 pt-1.5 border-t border-[var(--soxu-border-hairline)] flex flex-col gap-2">
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
                          <div className="max-w-[85%] rounded-[18px] px-4 py-3 text-sm leading-relaxed break-words bg-[var(--soxu-surface-raised)] border border-[var(--soxu-border-subtle)] text-[var(--soxu-text-primary)]">
                            <p className="m-0">{turn.text}</p>
                          </div>
                        </div>
                      );
                    }

                    const reasonOpen = Boolean(openReasons[turn.id]);
                    return (
                      <article key={turn.id} className="group flex flex-col gap-2.5">
                        <div className="flex items-center justify-between gap-2 text-xs text-[var(--soxu-text-tertiary)]">
                          <span className="inline-flex items-center gap-2 min-w-0">
                            <AgentIcon agent={turn.author} className="h-4 w-4 shrink-0" />
                            <span className="font-semibold text-[var(--soxu-text-secondary)] truncate">{turn.author}</span>
                            {turn.badge && <Badge variant="handoff">{turn.badge}</Badge>}
                          </span>
                          <button
                            type="button"
                            onClick={() => handleBranchFromTurn(turn)}
                            className="soxu-reveal-action soxu-hit-44 inline-flex items-center gap-1 text-[11px] text-[var(--soxu-text-tertiary)] hover:text-[var(--soxu-text-primary)] bg-transparent border-0 cursor-pointer shrink-0"
                          >
                            <GitBranch className="h-3 w-3" strokeWidth={1.75} />
                            <span>{t("turn.branchSession")}</span>
                          </button>
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

                        {turn.tools?.map((tl) => (
                          <ToolCallCard
                            key={tl.id}
                            tool={tl}
                            open={Boolean(openTools[tl.id])}
                            onOpenChange={(o) => setOpenTools((p) => ({ ...p, [tl.id]: o }))}
                            showLineNumbers={prefs.showLineNumbers}
                            wrap={prefs.codeWordWrap}
                            onSubmitLineComment={handleSubmitDiffLineComment}
                          />
                        ))}

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

                  {/* 结构化 Artifact 产出预览卡 */}
                  <div className="rounded-2xl p-3.5 flex flex-col gap-2.5 bg-[var(--soxu-surface-card)] border border-[var(--soxu-border-subtle)]">
                    <div className="flex items-start justify-between gap-2">
                      <div className="flex items-center gap-2 min-w-0">
                        <FileText className="h-4 w-4 text-[var(--soxu-semantic-accent)] shrink-0" strokeWidth={1.75} />
                        <div className="min-w-0">
                          <div className="text-xs font-semibold truncate">{activeArtifact.title}</div>
                          <div className="text-[11px] soxu-mono text-[var(--soxu-text-tertiary)] truncate">{activeArtifact.path}</div>
                        </div>
                      </div>
                      <Badge variant={activeArtifact.status === "approved" ? "running" : "warning"}>
                        {activeArtifact.status === "approved" ? t("artifact.approved") : "待审阅"}
                      </Badge>
                    </div>
                    <p className="text-xs text-[var(--soxu-text-secondary)] m-0 leading-relaxed">{activeArtifact.summary}</p>
                    <div className="flex items-center gap-2 flex-wrap">
                      <Button
                        variant="outline"
                        size="sm"
                        onClick={() => {
                          setRightInspectorTab("artifact");
                          if (!isFoldDualH) setRightDrawerOpen(true);
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
                              prev.map((st, idx) => (idx <= 2 ? { ...st, status: "done" } : idx === 3 ? { ...st, status: "in_progress" } : st)),
                            );
                            flashToast("已批准实施产物并推进下一步骤");
                          }}
                        >
                          <Check className="h-3.5 w-3.5" strokeWidth={2} />
                          <span>{t("artifact.approve")}</span>
                        </Button>
                      )}
                    </div>
                  </div>

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

                <QuoteSelectionPopover
                  containerRef={transcriptContentRef}
                  isCoarsePointer={vp.isCoarsePointer}
                  onQuote={(snippet) => {
                    addComposerChip({
                      kind: "quote",
                      label: snippet.slice(0, 42) + (snippet.length > 42 ? "..." : ""),
                      detail: snippet,
                    });
                    setComposerCollapsed(false);
                    window.setTimeout(() => composerInputRef.current?.focus(), 40);
                  }}
                  onCopyText={() => flashToast("已复制选中内容")}
                />
              </>
            ) : (
              /* workspace 会话但 activeWorkspace 暂缺（会话列表尚未同步）：中性空态 */
              <div className="py-16 px-4 flex flex-col items-center justify-center gap-2 text-center">
                <div className="text-xs text-[var(--soxu-text-tertiary)]">会话列表同步中，请稍候…</div>
              </div>
            )}
          </>
        )}
      </div>
    </div>
  );

  const quickChipsList = isRunning ? MOBILE_QUICK_CHIPS.running : MOBILE_QUICK_CHIPS.idle;

  const ComposerSection = activeTab === "session" && (
    <div className="shrink-0 relative px-3 sm:px-4 pt-1.5 soxu-kb-pad bg-gradient-to-t from-[var(--soxu-surface-canvas)] via-[var(--soxu-surface-canvas)] to-transparent">
      {showScrollBottom && (
        <div className="w-full mx-auto mb-1.5 flex justify-end" style={{ maxWidth: "var(--soxu-layout-transcript-max-width)" }}>
          <button
            type="button"
            onClick={() => scrollToBottom(true)}
            className="h-8 px-3 rounded-full bg-[var(--soxu-surface-floating)] border border-[var(--soxu-border-strong)] text-xs text-[var(--soxu-text-secondary)] hover:text-[var(--soxu-text-primary)] inline-flex items-center gap-1.5 shadow-md cursor-pointer"
          >
            <ArrowDown className="h-3.5 w-3.5" strokeWidth={1.75} />
            <span>{t("mobile.scrollToBottom")}</span>
          </button>
        </div>
      )}

      {composerCollapsed ? (
        <div
          style={{ maxWidth: "var(--soxu-layout-transcript-max-width)" }}
          className="w-full mx-auto flex justify-center"
        >
          <div
            role="button"
            tabIndex={0}
            onPointerDown={(e) => {
              composerSwipeStartYRef.current = e.clientY;
            }}
            onPointerUp={(e) => {
              const startY = composerSwipeStartYRef.current;
              composerSwipeStartYRef.current = null;
              if (startY !== null && startY - e.clientY > 24) {
                setComposerCollapsed(false);
                window.setTimeout(() => composerInputRef.current?.focus(), 40);
              }
            }}
            onClick={() => {
              setComposerCollapsed(false);
              window.setTimeout(() => composerInputRef.current?.focus(), 40);
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter" || e.key === " ") {
                e.preventDefault();
                setComposerCollapsed(false);
              }
            }}
            className="h-10 w-full rounded-full px-3.5 border border-[var(--soxu-border-strong)] bg-[var(--soxu-surface-composer)] flex items-center justify-between gap-2 cursor-pointer"
          >
            <span className="inline-flex items-center gap-2 min-w-0 text-xs text-[var(--soxu-text-secondary)]">
              <AgentIcon agent={cli} className="h-3.5 w-3.5 shrink-0" />
              <span className={cn("h-2 w-2 rounded-full shrink-0", isRunning ? "bg-[var(--soxu-semantic-running)] soxu-live-dot" : "bg-[var(--soxu-text-tertiary)]")} />
              <span className="truncate">{t("mobile.dockExpand")}</span>
              {queuedPrompts.length > 0 && (
                <Badge variant="warning" className="soxu-tabular">
                  {queuedPrompts.length}
                </Badge>
              )}
            </span>
            <ChevronUp className="h-4 w-4 text-[var(--soxu-text-tertiary)] shrink-0" strokeWidth={1.75} />
          </div>
        </div>
      ) : (
        <div
          style={{ maxWidth: "var(--soxu-layout-transcript-max-width)" }}
          className="w-full mx-auto flex flex-col"
        >
          {lostControlNote && (
            <div className="px-2.5 py-1 mb-1.5 rounded-lg text-[11px] bg-[var(--soxu-semantic-warning-soft)] text-[var(--soxu-semantic-warning)]">
              {lostControlNote}
            </div>
          )}

          {/* 顶部外置轻量条：工作区变更摘要药丸 + 窄屏常用指令快捷语（释放输入舱内部空间，杜绝拥挤） */}
          {targetKind === "workspace" && !vp.isShortViewport && (
            <div className="flex items-center gap-1.5 overflow-x-auto soxu-no-scrollbar mb-1.5 px-0.5">
              {!isFoldDualH && (
                <button
                  type="button"
                  onClick={() => {
                    setRightInspectorTab("diff");
                    setRightDrawerOpen(true);
                  }}
                  className="inline-flex items-center gap-1.5 h-7 px-2.5 rounded-full text-[11px] whitespace-nowrap border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card)] text-[var(--soxu-text-secondary)] hover:text-[var(--soxu-text-primary)] hover:border-[var(--soxu-border-strong)] cursor-pointer transition-colors shrink-0"
                >
                  <span>{filteredDiffFiles.length} 个文件已更改</span>
                  <span className="soxu-mono soxu-tabular text-[var(--soxu-semantic-success)]">+{totalDiffAdded}</span>
                  <span className="soxu-mono soxu-tabular text-[var(--soxu-semantic-danger)]">-{totalDiffDeleted}</span>
                </button>
              )}
              {sizeClass !== "wide" &&
                quickChipsList.map((chip) => (
                  <button
                    key={chip.id}
                    type="button"
                    onClick={() => handleSubmitPrompt(chip.mode, chip.label)}
                    className="h-7 px-2.5 rounded-full text-[11px] whitespace-nowrap border border-[var(--soxu-border-subtle)] bg-[var(--soxu-surface-card-muted)] text-[var(--soxu-text-secondary)] hover:text-[var(--soxu-text-primary)] hover:border-[var(--soxu-border-strong)] cursor-pointer transition-colors shrink-0"
                  >
                    {chip.label}
                  </button>
                ))}
            </div>
          )}

          {targetKind === "workspace" && (
            <MultiQueueBar
              docked
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
              "w-full border border-[var(--soxu-border-strong)] bg-[var(--soxu-surface-composer)] flex flex-col overflow-hidden",
              vp.isShortViewport ? "rounded-[18px]" : "rounded-[22px]",
            )}
          >
            {/* 顶部轻质滑动折叠手把 + 文本输入 + 主操控栏 */}
            <div className={cn("flex flex-col", vp.isShortViewport ? "px-2.5 pt-1 pb-1.5 gap-1" : "px-3 pt-1.5 pb-2 gap-1.5")}>
              <div
                className="flex items-center justify-center cursor-pointer select-none py-0.5"
                onPointerDown={(e) => {
                  composerSwipeStartYRef.current = e.clientY;
                }}
                onPointerUp={(e) => {
                  const startY = composerSwipeStartYRef.current;
                  composerSwipeStartYRef.current = null;
                  if (startY !== null && e.clientY - startY > 28) {
                    setComposerCollapsed(true);
                  }
                }}
                onClick={() => setComposerCollapsed(true)}
                title="点击或下滑收起输入舱为 40px 悬浮胶囊"
                aria-label="收起输入舱"
              >
                <span className="h-1 w-9 rounded-full bg-[var(--soxu-border-strong)]" />
              </div>

              <ComposerChipBar chips={composerChips} onRemove={removeComposerChip} />

              <textarea
                ref={composerInputRef}
                rows={vp.isShortViewport ? 1 : 2}
                value={draft}
                onChange={(e) => setDraft(e.target.value)}
                onKeyDown={(e) => {
                  if (isImeComposing(e)) return;
                  if (e.key === "Escape" && queuedPrompts.length > 0) {
                    e.preventDefault();
                    setQueuedPrompts((prev) => prev.slice(0, -1));
                  } else if (e.key === "Enter" && (e.ctrlKey || e.metaKey) && e.shiftKey) {
                    e.preventDefault();
                    setBehavior("queue");
                    handleSubmitPrompt("queue");
                  } else if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
                    e.preventDefault();
                    const targetMode: TurnBehavior = activeProfile.supportsLiveSteer ? "steer" : "queue";
                    setBehavior(targetMode);
                    handleSubmitPrompt(targetMode);
                  } else if (e.key === "Enter" && e.altKey) {
                    e.preventDefault();
                    setBehavior("interrupt");
                    handleSubmitPrompt("interrupt");
                  } else if (e.key === "Enter" && !e.shiftKey) {
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
                className="w-full resize-none bg-transparent border-0 outline-none px-1 py-0.5 text-base text-[var(--soxu-text-primary)] placeholder:text-[var(--soxu-text-ghost)]"
              />

              {/* 主操控行（减框减字不减功能：左 + | 权限 @，右 一体模型·思考 + 投递模式[宽行] + 发送键） */}
              <div className="flex items-center justify-between gap-1.5">
                <div className="flex items-center gap-1 sm:gap-1.5 shrink-0">
                  <ComposerPlusMenu
                    hasWorkspace={targetKind === "workspace"}
                    onAddChip={addComposerChip}
                  />
                  <span className="h-4 w-px bg-[var(--soxu-border-hairline)] shrink-0" />
                  <ComposerPermissionButton
                    agentId={cli}
                    permLevel={permLevel}
                    isMac={vp.isMac}
                    showLabel={isWideComposerRow && vp.width >= 820}
                    onSelectPerm={(nextId, label) => {
                      setPermLevel(nextId);
                      flashToast(`操作权限已调整为「${label}」`);
                    }}
                    onOpenSheet={sizeClass !== "wide" ? () => setSheet("perm") : undefined}
                  />
                  <ComposerHandoffMenu
                    currentCli={cli}
                    currentModel={model}
                    currentThinking={thinking}
                    selectedHandoff={selectedHandoff}
                    iconOnly
                    onSelectHandoff={(chipDraft) => {
                      addComposerChip(chipDraft);
                      flashToast(`已配置移交至 ${chipDraft.label}`);
                    }}
                    onClearHandoff={() => {
                      if (handoffChip) {
                        removeComposerChip(handoffChip.id);
                        flashToast("已取消移交配置");
                      }
                    }}
                    onOpenSheet={sizeClass !== "wide" ? () => setSheet("handoff") : undefined}
                  />
                </div>

                <div className="flex items-center gap-1.5 sm:gap-2 min-w-0">
                  <ComposerAgentModelPicker
                    cli={cli}
                    model={model}
                    thinking={thinking}
                    onSelectCli={(nextId) => handleSelectCli(nextId)}
                    onSelectModel={(nextId) => handleSelectModel(nextId)}
                    onSelectThinking={setThinking}
                    onOpenSheet={sizeClass !== "wide" ? () => setSheet("model-config") : undefined}
                  />

                  {isWideComposerRow && targetKind === "workspace" && (
                    <ComposerTurnBehaviorGroup
                      mode={behavior}
                      supportsLiveSteer={activeProfile.supportsLiveSteer}
                      isMac={vp.isMac}
                      onChange={setBehavior}
                    />
                  )}

                  {isRunning && !draft.trim() && composerChips.length === 0 ? (
                    <Button
                      variant="destructive"
                      size="icon"
                      onClick={() => {
                        if (!activeWorkspace) return;
                        setWorkspaces((prev) =>
                          prev.map((w) => (w.id === activeWorkspace.id ? { ...w, bucket: "done" } : w)),
                        );
                        flashToast("已发送停止信号");
                      }}
                      aria-label="停止生成"
                      className="shrink-0"
                    >
                      <Square className="h-3.5 w-3.5 fill-current" strokeWidth={1.75} />
                    </Button>
                  ) : (
                    <Button
                      variant="default"
                      size="icon"
                      disabled={!draft.trim() && composerChips.length === 0}
                      onClick={() => handleSubmitPrompt()}
                      aria-label="发送指令"
                      className="shrink-0"
                    >
                      <ArrowUp className="h-4 w-4" strokeWidth={2} />
                    </Button>
                  )}
                </div>
              </div>
            </div>

            {/* Oryn 底部遥测微脚条：柔和分隔线 + 窄屏投递模式/状态 + 彩色小圆环百分比 + 参考费用 + 无外框速度 */}
            <div className="border-t border-[var(--soxu-border-hairline)] bg-[var(--soxu-surface-card-muted)]/45 px-3 py-1.5 flex items-center justify-between gap-1.5">
              <div className="flex items-center gap-1.5 min-w-0">
                {!isWideComposerRow && targetKind === "workspace" ? (
                  <ComposerTurnBehaviorGroup
                    mode={behavior}
                    supportsLiveSteer={activeProfile.supportsLiveSteer}
                    isMac={vp.isMac}
                    onChange={setBehavior}
                  />
                ) : (
                  <span className="text-[11px] text-[var(--soxu-text-ghost)] truncate">
                    {targetKind === "workspace" ? (isRunning ? "运行中 · 可随时介入纠偏" : "就绪 · 点击右侧遥测查看明细") : "无项目自由对话"}
                  </span>
                )}
              </div>

              <div className="flex items-center gap-1 sm:gap-1.5 shrink-0">
                <ContextUsageRing
                  model={model}
                  usedTokens={contextTokens}
                  compact={false}
                  onCompact={() => {
                    setContextTokens((c) => Math.round(c * 0.42));
                    flashToast("已完成上下文摘要压缩");
                  }}
                  onOpenSheet={sizeClass !== "wide" ? () => setSheet("context") : undefined}
                />
                <SessionStatsPopover
                  stats={sessionStats}
                  showCost={!vp.isUltraNarrow}
                  compact={vp.isUltraNarrow}
                  onOpenSheet={sizeClass !== "wide" ? () => setSheet("stats") : undefined}
                />
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );

  const SinkCapsuleNav = (
    <nav
      aria-label="侧栏分区切换"
      className="h-[52px] rounded-[26px] px-1.5 bg-[var(--soxu-surface-card)] border border-[var(--soxu-border-subtle)] grid grid-cols-4 items-center gap-1"
    >
      {(
        [
          { id: "session", label: t("mobile.navSession"), icon: MessageSquare, action: () => { setActiveTab("session"); setLeftDrawerOpen(false); } },
          { id: "inbox", label: t("mobile.navInbox"), icon: Inbox, count: inboxAttentionCount, action: () => { setInboxSubMode("triage"); setActiveTab("inbox"); setLeftDrawerOpen(false); } },
          { id: "scripts", label: t("mobile.navScripts"), icon: Layers, action: () => { setActiveTab("scripts"); setLeftDrawerOpen(false); } },
          { id: "settings", label: t("mobile.navSettings"), icon: Settings, action: () => { setLeftDrawerOpen(false); setSettingsSection("appearance"); setSettingsOpen(true); } },
        ] as const
      ).map((nav) => {
        const Icon = nav.icon;
        const active = activeTab === nav.id || (nav.id === "inbox" && activeTab === "approvals");
        return (
          <button
            key={nav.id}
            type="button"
            onClick={nav.action}
            className={cn(
              "h-10 rounded-[20px] flex flex-col items-center justify-center gap-0.5 text-[10.5px] border-0 cursor-pointer transition-colors relative",
              active
                ? "bg-[var(--soxu-interaction-selected)] text-[var(--soxu-text-primary)] font-semibold"
                : "bg-transparent text-[var(--soxu-text-tertiary)]",
            )}
          >
            <div className="relative">
              <Icon className="h-3.5 w-3.5" strokeWidth={1.75} />
              {"count" in nav && nav.count > 0 && (
                <span className="absolute -top-1.5 -right-2.5 min-w-[14px] h-[14px] px-1 rounded-full text-[9px] leading-[14px] text-center bg-[var(--soxu-semantic-warning)] text-black font-bold">
                  {nav.count}
                </span>
              )}
            </div>
            <span>{nav.label}</span>
          </button>
        );
      })}
    </nav>
  );

  const CompactSideRail = (
    <aside
      aria-label="紧凑功能轨"
      className="w-[52px] shrink-0 soxu-safe-left py-2 border-r border-[var(--soxu-border-hairline)] bg-[var(--soxu-surface-sidebar)] flex flex-col items-center justify-between gap-2"
    >
      <div className="flex flex-col items-center gap-1.5">
        <Button
          variant="ghost"
          size="icon-sm"
          onClick={() => setLeftDrawerOpen(true)}
          aria-label={t("header.openLeftDrawer")}
          title={t("sidebar.title")}
        >
          <PanelLeft className="h-4 w-4" strokeWidth={1.75} />
        </Button>
        <Button
          variant={activeTab === "session" ? "secondary" : "ghost"}
          size="icon-sm"
          onClick={() => setActiveTab("session")}
          aria-label={t("mobile.navSession")}
          title={t("mobile.navSession")}
        >
          <MessageSquare className="h-4 w-4" strokeWidth={1.75} />
        </Button>
        <Button
          variant={activeTab === "inbox" || activeTab === "approvals" ? "secondary" : "ghost"}
          size="icon-sm"
          onClick={() => setActiveTab("inbox")}
          aria-label={t("mobile.navInbox")}
          title={t("mobile.navInbox")}
          className="relative"
        >
          <Inbox className="h-4 w-4" strokeWidth={1.75} />
          {inboxAttentionCount > 0 && (
            <span className="absolute -top-0.5 -right-0.5 min-w-[14px] h-[14px] px-1 rounded-full text-[9px] leading-[14px] text-center bg-[var(--soxu-semantic-warning)] text-black font-bold">
              {inboxAttentionCount}
            </span>
          )}
        </Button>
        <Button
          variant={activeTab === "scripts" ? "secondary" : "ghost"}
          size="icon-sm"
          onClick={() => setActiveTab("scripts")}
          aria-label={t("mobile.navScripts")}
          title={t("mobile.navScripts")}
        >
          <Layers className="h-4 w-4" strokeWidth={1.75} />
        </Button>
      </div>
      <div className="flex flex-col items-center gap-1.5">
        <Button
          variant={rightDrawerOpen ? "secondary" : "ghost"}
          size="icon-sm"
          onClick={() => setRightDrawerOpen(true)}
          aria-label={t("header.openRightDrawer")}
          title={t("header.openRightDrawer")}
        >
          <PanelRight className="h-4 w-4" strokeWidth={1.75} />
        </Button>
        <Button
          variant="ghost"
          size="icon-sm"
          onClick={() => {
            setSettingsSection("appearance");
            setSettingsOpen(true);
          }}
          aria-label={t("header.settings")}
          title={t("header.settings")}
        >
          <Settings className="h-4 w-4" strokeWidth={1.75} />
        </Button>
      </div>
    </aside>
  );

  // 未认证：全屏配对引导页（设计规范 05 · 3.1：扫码降级 + 6 位码表单 + 引导添加主屏幕）
  if (!pairing) {
    return (
      <div
        className="h-dvh flex flex-col items-center justify-center gap-6 bg-[var(--soxu-surface-canvas)] text-[var(--soxu-text-primary)] px-6 soxu-safe-top soxu-safe-bottom"
        style={{ "--soxu-kb-inset": "0px" } as React.CSSProperties}
      >
        <div className="flex flex-col items-center gap-2 text-center">
          <span className="text-2xl font-bold tracking-tight">{t("brand.name")}</span>
          <span className="text-sm text-[var(--soxu-text-secondary)]">移动遥控尚未与守护服务配对</span>
          <span className="text-xs text-[var(--soxu-text-tertiary)] max-w-[300px] leading-relaxed">
            在桌面端打开「扫码配对」后，用系统相机扫描二维码即可一键完成配对；或直接输入 6 位一次性配对码。
          </span>
        </div>

        {offerHint && (
          <div className="w-full max-w-[340px] rounded-xl p-3 text-xs bg-[var(--soxu-semantic-accent-soft)] text-[var(--soxu-semantic-accent)] text-center">
            {offerHint}
          </div>
        )}
        {pairingBusy && <div className="text-xs text-[var(--soxu-text-tertiary)]">正在与守护服务握手…</div>}
        {pairError && (
          <div className="w-full max-w-[340px] rounded-xl p-3 text-xs bg-[var(--soxu-semantic-danger-soft)] text-[var(--soxu-semantic-danger)]">{pairError}</div>
        )}

        <div className="w-full max-w-[340px] flex flex-col gap-2.5">
          <input
            type="text"
            inputMode="url"
            value={daemonEndpoint}
            onChange={(e) => setDaemonEndpoint(e.target.value.trim())}
            placeholder="守护服务地址（例如 http://192.168.1.42:7698）"
            className="w-full rounded-xl px-3 py-2.5 text-sm soxu-mono bg-[var(--soxu-surface-composer)] border border-[var(--soxu-border-subtle)] text-[var(--soxu-text-primary)] outline-none"
          />
          <input
            type="text"
            inputMode="numeric"
            value={pairCode}
            onChange={(e) => setPairCode(e.target.value.replace(/[^0-9]/g, "").slice(0, 6))}
            placeholder="输入 6 位一次性配对码（例如 849217）"
            className="w-full rounded-xl px-3 py-2.5 text-base soxu-mono tracking-[0.2em] bg-[var(--soxu-surface-composer)] border border-[var(--soxu-border-strong)] text-[var(--soxu-text-primary)] outline-none"
          />
          <Button
            variant="default"
            size="lg"
            disabled={pairCode.length !== 6 || pairingBusy}
            onClick={() => {
              setPairingBusy(true);
              setPairError(null);
              claimPairing(daemonEndpoint, { kind: "six_digit_code", value: pairCode })
                .then((claimed) => {
                  const next: StoredPairing = {
                    endpoint_url: daemonEndpoint,
                    bearer_token: claimed.bearer_token,
                    device_id: claimed.device_id,
                    daemon_fingerprint: claimed.daemon_fingerprint,
                  };
                  saveStoredPairing(next);
                  setPairing(next);
                  setPairCode("");
                  flashToast("已校验指纹并建立安全连接");
                })
                .catch((err: unknown) => setPairError(err instanceof Error ? err.message : "配对失败"))
                .finally(() => setPairingBusy(false));
            }}
          >
            {pairingBusy ? "正在配对…" : "验证指纹并上线"}
          </Button>
        </div>

        <div className="text-[11px] text-[var(--soxu-text-tertiary)] text-center max-w-[300px] leading-relaxed">
          配对成功后建议将本页添加到主屏幕，以独立全屏 PWA 运行；访问凭据仅保存在本机。
        </div>
      </div>
    );
  }

  return (
    <TooltipProvider delayDuration={220}>
      <div
        className="h-dvh flex flex-col bg-[var(--soxu-surface-canvas)] text-[var(--soxu-text-primary)] overflow-hidden"
        style={{ "--soxu-kb-inset": `${vp.keyboardInset}px` } as React.CSSProperties}
      >
        {hasNoTargets ? (
          /* 零会话引导态：daemon 快照尚无任何项目工作树或快速对话（不伪造数据） */
          <div className="flex-1 flex flex-col items-center justify-center gap-3 px-6 text-center soxu-safe-top soxu-safe-bottom">
            <div className="h-12 w-12 rounded-2xl bg-[var(--soxu-surface-card)] border border-[var(--soxu-border-subtle)] flex items-center justify-center">
              <Inbox className="h-5 w-5 text-[var(--soxu-text-tertiary)]" strokeWidth={1.5} />
            </div>
            <div className="text-sm font-semibold">守护服务已连接，暂无会话</div>
            <div className="text-xs text-[var(--soxu-text-tertiary)] max-w-[300px] leading-relaxed">
              当前守护进程还没有项目工作树或快速对话。在桌面端创建后，这里会实时同步出现。
            </div>
            <div className="text-[11px] soxu-mono text-[var(--soxu-text-tertiary)] mt-1">
              {t("mobile.cursorPrefix", { seq: seqCursor })}
            </div>
          </div>
        ) : isFoldDualH ? (
          /* 折叠屏展开 / 左右双屏分栏（跨铰链零控件跨缝） */
          <div className="flex-1 min-h-0 flex w-full overflow-hidden">
            <section
              className="relative flex flex-col min-w-0 min-h-0 overflow-hidden bg-[var(--soxu-surface-canvas)]"
              style={
                vp.leftSegmentWidth > 0
                  ? { width: `${vp.leftSegmentWidth}px`, flexShrink: 0 }
                  : { flex: "1 1 56%" }
              }
            >
              {HeaderBar}
              {MainBodyView}
              {ComposerSection}
            </section>

            {/* 铰链安全隔离带 */}
            <div
              aria-hidden="true"
              style={{ width: `${Math.max(12, vp.hingeWidth)}px` }}
              className="shrink-0 bg-[var(--soxu-surface-inset)] border-x border-[var(--soxu-border-hairline)] flex flex-col items-center justify-center"
              title={t("mobile.foldDualHint")}
            >
              <span className="h-12 w-1 rounded-full bg-[var(--soxu-border-strong)]" />
            </div>

            <aside
              aria-label={t("inspector.title")}
              className="flex flex-col min-w-[280px] min-h-0 border-l border-[var(--soxu-border-hairline)] bg-[var(--soxu-surface-sidebar)]"
              style={
                vp.rightSegmentWidth > 0
                  ? { width: `${vp.rightSegmentWidth}px`, flexShrink: 0 }
                  : { width: "42%" }
              }
            >
              {renderRightInspectorBody(false)}
            </aside>
          </div>
        ) : isCompactSink ? (
          /* 常规手机竖屏：下沉式侧栏 + 平时隐没、拉出可见的 52px 底层分区胶囊 */
          <SinkLayout
            open={leftDrawerOpen}
            onOpenChange={setLeftDrawerOpen}
            reduceMotion={prefs.reduceMotion}
            triggerRef={leftTriggerRef}
            sidebar={WorkspaceRail}
            footer={SinkCapsuleNav}
          >
            {HeaderBar}
            {MainBodyView}
            {ComposerSection}
          </SinkLayout>
        ) : isFoldDualV ? (
          /* 上下双段半折悬停态：上屏浏览，下屏控制 */
          <div className="flex-1 min-h-0 flex overflow-hidden">
            {CompactSideRail}
            <div className="flex-1 min-w-0 flex flex-col min-h-0 overflow-hidden">
              <div
                className="relative flex flex-col min-h-0 overflow-hidden"
                style={{
                  maxHeight: vp.topSegmentHeight > 0 ? `${vp.topSegmentHeight}px` : "52dvh",
                  flex: "1 1 auto",
                }}
              >
                {HeaderBar}
                {MainBodyView}
              </div>
              <div
                aria-hidden="true"
                style={{ height: `${Math.max(12, vp.hingeHeight)}px` }}
                className="shrink-0 bg-[var(--soxu-surface-inset)] border-y border-[var(--soxu-border-hairline)] flex items-center justify-center"
              >
                <span className="w-12 h-1 rounded-full bg-[var(--soxu-border-strong)]" />
              </div>
              <div
                className="shrink-0 flex flex-col justify-end bg-[var(--soxu-surface-canvas)]"
                style={{ minHeight: vp.bottomSegmentHeight > 0 ? `${vp.bottomSegmentHeight}px` : undefined }}
              >
                {ComposerSection}
              </div>
            </div>
          </div>
        ) : (
          /* 横屏紧凑左轨 / 平板 regular / 桌面 wide 通用结构 */
          <div className="flex-1 min-h-0 flex flex-col overflow-hidden">
            {!isFloatingHeader && HeaderBar}
            <div className="flex-1 min-h-0 flex max-w-[1280px] w-full mx-auto overflow-hidden">
              {showCompactRail && CompactSideRail}
              {sizeClass === "wide" && (
                <div className="h-full overflow-y-auto soxu-scroll shrink-0 border-r border-[var(--soxu-border-hairline)] bg-[var(--soxu-surface-sidebar)]">
                  {WorkspaceRail}
                </div>
              )}
              <main className="relative flex-1 min-w-0 min-h-0 flex flex-col justify-between overflow-hidden">
                {isFloatingHeader && HeaderBar}
                {MainBodyView}
                {ComposerSection}
              </main>
            </div>
          </div>
        )}

        {/* 非 SinkLayout 形态使用的左侧工作区抽屉 */}
        <Sheet open={!isCompactSink && sizeClass !== "wide" && leftDrawerOpen} onOpenChange={setLeftDrawerOpen}>
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

        {/* 右侧变更对比、产物预览与受管脚本抽屉 */}
        <Sheet open={!isFoldDualH && rightDrawerOpen} onOpenChange={setRightDrawerOpen}>
          <SheetContent side="right" rawBody srTitle={t("inspector.title")}>
            {renderRightInspectorBody(true)}
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
          {sheet === "model-config" && (
            <SheetContent title="代理、模型与思考强度" subtitle="一站式切换底层原生驱动、模型版本与思考深度" tall>
              <AgentModelConfigPanel
                cli={cli}
                model={model}
                thinking={thinking}
                onSelectCli={(nextId) => handleSelectCli(nextId, undefined, true)}
                onSelectModel={(nextId) => handleSelectModel(nextId, true)}
                onSelectThinking={setThinking}
                onDone={() => setSheet(null)}
              />
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
                  <div className="flex items-start gap-2.5 min-w-0">
                    <PermissionIcon level={p.id} className="mt-0.5 shrink-0" />
                    <div>
                      <div className="text-sm font-semibold">{p.label}</div>
                      <div className="text-xs text-[var(--soxu-text-tertiary)]">{p.desc}</div>
                    </div>
                  </div>
                  {permLevel === p.id && <Check className="h-4 w-4 text-[var(--soxu-semantic-accent)]" strokeWidth={2} />}
                </button>
              ))}
            </SheetContent>
          )}

          {sheet === "handoff" && (
            <SheetContent title={t("composer.handoff")} subtitle={t("composer.handoffChoose")} tall>
              <HandoffConfigPanel
                currentCli={cli}
                currentModel={model}
                currentThinking={thinking}
                selectedHandoff={selectedHandoff}
                onSelectHandoff={(chipDraft) => {
                  addComposerChip(chipDraft);
                  flashToast(`已配置移交至 ${chipDraft.label}`);
                }}
                onClearHandoff={() => {
                  if (handoffChip) {
                    removeComposerChip(handoffChip.id);
                    flashToast("已取消移交配置");
                  }
                }}
                onDone={() => setSheet(null)}
              />
            </SheetContent>
          )}

          {sheet === "context" && (
            <SheetContent title="上下文窗口分析" subtitle={`当前模型：${model}`}>
              <ContextUsagePanel
                model={model}
                usedTokens={contextTokens}
                onCompact={() => {
                  setContextTokens((c) => Math.round(c * 0.42));
                  flashToast("已完成上下文摘要压缩");
                }}
                onClose={() => setSheet(null)}
              />
            </SheetContent>
          )}

          {sheet === "stats" && (
            <SheetContent title={t("stats.title")} subtitle="实时会话速率、缓存命中率与参考费用统计">
              <SessionStatsPanel stats={sessionStats} />
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
            <SheetContent title="扫码与配对码连接守护服务" subtitle={pairing ? `已配对：${pairing.endpoint_url}` : "输入桌面端展示的 6 位一次性配对码"} tall>
              <div className="rounded-xl p-3 bg-[var(--soxu-surface-card-muted)] border border-[var(--soxu-border-subtle)] text-xs space-y-2">
                <div className="font-semibold">一次性安全凭据配对</div>
                <p className="text-[var(--soxu-text-tertiary)] m-0">
                  配对握手参数仅停留于本地链接片段中，不会上传或暴露在服务端日志中。
                </p>
              </div>
              {pairError && (
                <div className="rounded-xl p-3 text-xs bg-[var(--soxu-semantic-danger-soft)] text-[var(--soxu-semantic-danger)]">{pairError}</div>
              )}
              {pairing ? (
                <>
                  <div className="text-xs text-[var(--soxu-text-secondary)]">
                    设备 ID：<code className="soxu-mono">{pairing.device_id}</code>
                  </div>
                  <Button
                    variant="destructive"
                    size="lg"
                    onClick={() => {
                      // 解除配对：吊销 daemon 侧设备 + 清除本机凭据
                      const client = clientRef.current;
                      const revoke = client
                        ? client.rpc({ method: "revoke_paired_device", params: { device_id: pairing.device_id } }).catch(() => undefined)
                        : Promise.resolve(undefined);
                      revoke.finally(() => {
                        clearStoredPairing();
                        setPairing(null);
                        setConnPhase("offline");
                        setDaemonOnline(false);
                        setSheet(null);
                        flashToast("已解除配对并清除本机凭据");
                      });
                    }}
                  >
                    解除配对并清除本机凭据
                  </Button>
                </>
              ) : (
                <>
                  <input
                    type="text"
                    inputMode="url"
                    value={daemonEndpoint}
                    onChange={(e) => setDaemonEndpoint(e.target.value.trim())}
                    placeholder="守护服务地址（例如 http://192.168.1.42:7698）"
                    className="w-full rounded-xl px-3 py-2.5 text-sm soxu-mono bg-[var(--soxu-surface-composer)] border border-[var(--soxu-border-subtle)] text-[var(--soxu-text-primary)] outline-none"
                  />
                  <input
                    type="text"
                    inputMode="numeric"
                    value={pairCode}
                    onChange={(e) => setPairCode(e.target.value.replace(/[^0-9]/g, "").slice(0, 6))}
                    placeholder="输入 6 位一次性配对码（例如 849217）"
                    className="w-full rounded-xl px-3 py-2.5 text-base soxu-mono tracking-[0.2em] bg-[var(--soxu-surface-composer)] border border-[var(--soxu-border-strong)] text-[var(--soxu-text-primary)] outline-none"
                  />
                  <Button
                    variant="default"
                    size="lg"
                    disabled={pairCode.length !== 6 || pairingBusy || !daemonEndpoint}
                    onClick={() => {
                      setPairingBusy(true);
                      setPairError(null);
                      claimPairing(daemonEndpoint, { kind: "six_digit_code", value: pairCode })
                        .then((claimed) => {
                          const next: StoredPairing = {
                            endpoint_url: daemonEndpoint,
                            bearer_token: claimed.bearer_token,
                            device_id: claimed.device_id,
                            daemon_fingerprint: claimed.daemon_fingerprint,
                          };
                          saveStoredPairing(next);
                          setPairing(next);
                          setPairCode("");
                          setSheet(null);
                          flashToast("已校验指纹并建立安全连接");
                        })
                        .catch((err: unknown) => setPairError(err instanceof Error ? err.message : "配对失败"))
                        .finally(() => setPairingBusy(false));
                    }}
                  >
                    {pairingBusy ? "正在配对…" : "验证指纹并上线"}
                  </Button>
                </>
              )}
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
              <div className="rounded-xl p-6 text-center text-xs text-[var(--soxu-text-tertiary)] bg-[var(--soxu-surface-card-muted)] border border-[var(--soxu-border-subtle)]">
                原生会话历史扫描尚未接入：待守护进程历史导入能力交付后在此展示。
              </div>
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
