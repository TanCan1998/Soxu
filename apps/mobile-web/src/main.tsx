import {
  ArrowUp,
  AtSign,
  Bolt,
  Check,
  ChevronRight,
  Clock,
  Cpu,
  FileCode2,
  Folder,
  GitBranch,
  History,
  Layers,
  MessageSquare,
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
} from "lucide-react";
import { StrictMode, useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { HoverExpandDock } from "@/components/HoverExpandDock";
import { SettingsDialog, type SettingsSectionId } from "@/components/SettingsDialog";
import { ThemeToggle } from "@/components/ThemeToggle";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Sheet, SheetContent } from "@/components/ui/sheet";
import { TooltipProvider } from "@/components/ui/tooltip";
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
  INITIAL_SCHEDULES,
  INITIAL_SCRIPTS,
  INITIAL_TURNS,
  INITIAL_WORKSPACES,
  MODEL_OPTIONS,
  NATIVE_HISTORY,
} from "./mock-data";
import type {
  ApprovalItem,
  AskQuestionState,
  MobileTab,
  PermissionLevel,
  ScheduleItem,
  ScriptItem,
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
  variant: "warning" | "danger" | "running" | "handoff" | "neutral";
} {
  switch (bucket) {
    case "needs_input":
      return { label: "需输入", variant: "warning" };
    case "failed":
      return { label: "失败", variant: "danger" };
    case "running":
      return { label: "运行中", variant: "running" };
    case "attention":
      return { label: "待关注", variant: "handoff" };
    case "done":
      return { label: "已完成", variant: "neutral" };
  }
}

function permLabel(level: PermissionLevel): string {
  switch (level) {
    case "ask":
      return "询问确认";
    case "auto-edit":
      return "自动编辑";
    case "full-access":
      return "完全放行";
  }
}

function MobileApp() {
  const [prefs, setPrefs] = useState<SoxuPreferences>(loadPreferences);
  const [resolvedTheme, setResolvedTheme] = useState<"dark" | "light">(() => applyPreferencesToDom(prefs));

  const [sizeClass, setSizeClass] = useState<SizeClass>(() =>
    typeof window !== "undefined" ? resolveSizeClass(window.innerWidth) : "compact",
  );
  const [activeTab, setActiveTab] = useState<MobileTab>("session");
  const [workspaces, setWorkspaces] = useState<WorkspaceItem[]>(INITIAL_WORKSPACES);
  const [selectedWorkspaceId, setSelectedWorkspaceId] = useState<string>("wks-m2");
  const [bucketFilter, setBucketFilter] = useState<"all" | WorkspaceBucket>("all");
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
  const [permLevel, setPermLevel] = useState<PermissionLevel>("ask");
  const [behavior, setBehavior] = useState<TurnBehavior>("steer");
  const [queuedPrompt, setQueuedPrompt] = useState<string | null>(null);
  const [draft, setDraft] = useState<string>("");

  const [openReasons, setOpenReasons] = useState<Record<string, boolean>>({});
  const [openTools, setOpenTools] = useState<Record<string, boolean>>({ "tl-edit": true });
  const [diffExpanded, setDiffExpanded] = useState<Record<string, boolean>>({});
  const [rejectingId, setRejectingId] = useState<string | null>(null);
  const [rejectReason, setRejectReason] = useState<string>("");
  const [highRiskUnlocked, setHighRiskUnlocked] = useState<Record<string, boolean>>({});

  const [sheet, setSheet] = useState<SheetKind>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [settingsSection, setSettingsSection] = useState<SettingsSectionId>("appearance");
  const [pairCode, setPairCode] = useState<string>("");
  const [newBranchName, setNewBranchName] = useState<string>("feat/19-mobile-remote");

  useEffect(() => {
    const resolved = applyPreferencesToDom(prefs);
    setResolvedTheme(resolved);
    savePreferences(prefs);
  }, [prefs]);

  useEffect(() => {
    const onResize = () => setSizeClass(resolveSizeClass(window.innerWidth));
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
  const pendingApprovals = approvals.filter((a) => !a.resolution);
  const resolvedApprovals = approvals.filter((a) => Boolean(a.resolution));
  const isRunning = activeWorkspace.bucket === "running";
  const filteredWorkspaces =
    bucketFilter === "all" ? workspaces : workspaces.filter((w) => w.bucket === bucketFilter);
  const currentModelMeta = MODEL_OPTIONS.find((m) => m.id === model) ?? MODEL_OPTIONS[0];

  function simulateSeqCatchup() {
    setSeqCursor((prev) => {
      const next = prev + 17;
      flashToast(`WS 增量追平完成：seq #${prev} -> #${next}（无状态漂移）`);
      return next;
    });
  }

  function handleSelectCli(nextCliId: string) {
    const nextCli = CLI_OPTIONS.find((c) => c.id === nextCliId);
    if (!nextCli || !nextCli.available) return;
    setCli(nextCli.id);
    if (nextCli.id === "agy") {
      setModel("gemini-4-carbon");
      setThinking("high");
      setLostControlNote("已切换为 agy 原生会话：默认模型已对齐 gemini-4-carbon。");
    } else if (model === "gemini-4-carbon") {
      setModel("group/gpt-6.2-sol");
      setThinking("medium");
      setLostControlNote(`${nextCli.id} 通过 Magpie 统一路由运行，已自动回退至 group/gpt-6.2-sol。`);
    } else {
      setLostControlNote(null);
    }
    setSheet(null);
    flashToast(`活跃驱动已切换至 ${nextCli.id} v${nextCli.ver}`);
  }

  function handleSelectModel(nextModelId: string) {
    const nextMeta = MODEL_OPTIONS.find((m) => m.id === nextModelId);
    if (!nextMeta) return;
    setModel(nextMeta.id);
    if (!nextMeta.thinkingOptions.includes(thinking)) {
      const fallback = nextMeta.thinkingOptions[0] ?? "medium";
      setThinking(fallback);
      setLostControlNote(`${nextMeta.id} 不支持当前思考强度，已平滑降级为 ${fallback}。`);
    } else {
      setLostControlNote(null);
    }
    setSheet(null);
    flashToast(`模型已切换为 ${nextMeta.id}`);
  }

  function handleResolveApproval(id: string, resolution: "approved" | "session" | "rejected", note?: string) {
    setApprovals((prev) =>
      prev.map((a) => (a.id === id ? { ...a, resolution, note } : a)),
    );
    setRejectingId(null);
    setRejectReason("");
    setHighRiskUnlocked((prev) => ({ ...prev, [id]: false }));
    setSeqCursor((s) => s + 1);
    flashToast(
      resolution === "rejected"
        ? "已拒绝高危动作并将修改意见回传 Agent"
        : resolution === "session"
          ? "已批准，并允许本会话同类只读/安全命令自动放行"
          : "已授权单次执行",
    );
  }

  function handleSubmitPrompt() {
    const text = draft.trim();
    if (!text) return;

    if (isRunning) {
      if (behavior === "queue") {
        setQueuedPrompt(text);
        setDraft("");
        flashToast("已加入 Turn 队列，将在当前回合结束时顺延发送");
        return;
      }
      if (behavior === "interrupt") {
        setWorkspaces((prev) =>
          prev.map((w) => (w.id === activeWorkspace.id ? { ...w, bucket: "done" } : w)),
        );
      }
    }

    const nextSeq = seqCursor + 2;
    setSeqCursor(nextSeq);
    const userTurn: TurnMessage = {
      id: `turn-u-${nextSeq}`,
      role: "user",
      author: "你",
      text,
    };
    const replyTurn: TurnMessage = {
      id: `turn-a-${nextSeq + 1}`,
      role: "assistant",
      author: `${cli} · ${model}`,
      reasoningSec: "1.1s",
      reasoningSteps: [
        "1. 通过本地 IPC 读取工作区检查点，确保修改范围受限在目标路径。",
        "2. 在 TempDir 沙箱验证变更边界，不触碰全局配置目录。",
      ],
      text:
        behavior === "steer"
          ? `已在不中断当前进程的前提下热纠偏 (${cli})：收到指令「${text}」，正应用至活跃上下文。`
          : `已接收新指令（${permLabel(permLevel)}），正在通过 ${cli} (${model}) 执行校验。`,
    };
    setTurns((prev) => [...prev, userTurn, replyTurn]);
    setDraft("");
  }

  function handleAnswerAsk(optionId: string) {
    const picked = askQuestion.options.find((o) => o.id === optionId);
    if (!picked) return;
    setAskQuestion((prev) => ({ ...prev, selectedId: optionId, submitted: true }));
    setSeqCursor((s) => s + 1);
    flashToast(`已回传人工决策：「${picked.label}」`);
  }

  function handleHandoff(targetCli: string) {
    setSheet(null);
    setCli(targetCli);
    const nextSeq = seqCursor + 2;
    setSeqCursor(nextSeq);
    setTurns((prev) => [
      ...prev,
      {
        id: `turn-handoff-${nextSeq}`,
        role: "assistant",
        author: `${targetCli} · handoff`,
        badge: `@${targetCli} 移交完成`,
        text: `已通过 @${targetCli} 完成上下文移交：完整继承最近 3 轮摘要、未提交 Diff 列表与 seq #${nextSeq} 游标。`,
      },
    ]);
    flashToast(`上下文已平滑移交至 @${targetCli}`);
  }

  function handleApplyRewind(label: string) {
    setSheet(null);
    setSeqCursor((s) => s + 1);
    flashToast(`已安全回退至检查点 ${label}（工作区未提交更改已保留至 Safety Stash）`);
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
      branch: slug || "feat/new-worktree",
      isolation: "worktree",
      agent: cli,
      model,
      bucket: "attention",
      summary: "新建独立 Worktree · 已注入本地 AOCI / MCP 上下文",
      diffStat: "+0 -0",
      label: "New",
    };
    setWorkspaces((prev) => [item, ...prev]);
    setSelectedWorkspaceId(id);
    setSheet(null);
    setActiveTab("session");
    flashToast(`已创建并切换至隔离 Worktree：${item.branch}`);
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
                  : `已拒绝：${item.note || "请修改方案"}`}
              {" · "}
              <code className="soxu-mono">{item.command}</code>
            </span>
          </span>
          <span className="soxu-tabular text-[var(--soxu-text-tertiary)] shrink-0">已消解</span>
        </div>
      );
    }

    const expanded = Boolean(diffExpanded[item.id]);
    const unlocked = Boolean(highRiskUnlocked[item.id]);

    return (
      <article
        key={item.id}
        className={cn(
          "rounded-2xl p-4 flex flex-col gap-3 bg-[var(--soxu-surface-card)] border border-l-[3px]",
          item.highRisk
            ? "border-[var(--soxu-semantic-danger-border)] border-l-[var(--soxu-semantic-danger)]"
            : "border-[var(--soxu-semantic-warning-border)] border-l-[var(--soxu-semantic-warning)]",
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
              <div className="text-xs soxu-mono text-[var(--soxu-text-tertiary)] truncate">
                {item.branch} · {item.agent}
              </div>
            </div>
          </div>
          <Badge variant={item.highRisk ? "danger" : "warning"}>{item.highRisk ? "高危拦截" : "待确认"}</Badge>
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
                <span>{expanded ? "收起关联 Diff" : `查看关联 Diff (${item.diffPreview.length} 行)`}</span>
              </Button>
            </CollapsibleTrigger>
            <CollapsibleContent className="soxu-collapsible overflow-hidden pt-2">
              <pre className="m-0 p-3 rounded-xl soxu-code-block overflow-x-auto leading-relaxed bg-[var(--soxu-surface-inset)] border border-[var(--soxu-border-subtle)]">
                {item.diffPreview.map((line, idx) => (
                  <div
                    key={idx}
                    className={cn(
                      line.startsWith("+")
                        ? "text-[var(--soxu-semantic-success)]"
                        : line.startsWith("-")
                          ? "text-[var(--soxu-semantic-danger)]"
                          : "text-[var(--soxu-text-secondary)]",
                    )}
                  >
                    {line}
                  </div>
                ))}
              </pre>
            </CollapsibleContent>
          </Collapsible>
        )}

        {rejectingId === item.id ? (
          <div className="flex flex-col gap-2 pt-1">
            <label className="text-xs font-medium" htmlFor={`reject-${item.id}`}>
              向 Agent 附加拒绝与修改指导：
            </label>
            <input
              id={`reject-${item.id}`}
              type="text"
              value={rejectReason}
              onChange={(e) => setRejectReason(e.target.value)}
              placeholder="例如：请改用 TempDir 并在用例退出后自动回收"
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
                拒绝并附指导
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
        "flex flex-col gap-3 p-3.5 shrink-0",
        sizeClass === "wide"
          ? "w-[320px] border-r border-[var(--soxu-border-hairline)] bg-[var(--soxu-surface-sidebar)]"
          : "w-full",
      )}
    >
      <div className="flex items-center justify-between gap-2">
        <span className="text-xs font-semibold text-[var(--soxu-text-secondary)]">工作区与会话分桶</span>
        <div className="flex items-center gap-1.5">
          <Button variant="outline" size="sm" onClick={() => setSheet("history")}>
            <History className="h-3.5 w-3.5" strokeWidth={1.75} />
            <span>原生历史</span>
          </Button>
          <Button variant="secondary" size="sm" onClick={() => setSheet("new-worktree")}>
            <Plus className="h-3.5 w-3.5" strokeWidth={1.75} />
            <span>Worktree</span>
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
            { id: "attention", label: "待关注" },
            { id: "done", label: "已完成" },
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
                : "bg-[var(--soxu-surface-card)] text-[var(--soxu-text-secondary)] border-[var(--soxu-border-subtle)] hover:text-[var(--soxu-text-primary)]",
            )}
          >
            {b.label}
          </button>
        ))}
      </div>

      <div className="flex flex-col gap-2">
        {filteredWorkspaces.map((w) => {
          const active = w.id === selectedWorkspaceId;
          const meta = bucketMeta(w.bucket);
          return (
            <button
              key={w.id}
              type="button"
              onClick={() => {
                setSelectedWorkspaceId(w.id);
                if (sizeClass !== "wide") setActiveTab("session");
              }}
              className={cn(
                "w-full text-left rounded-2xl p-3 flex flex-col gap-1.5 border cursor-pointer transition-colors",
                active
                  ? "bg-[var(--soxu-interaction-selected)] border-[var(--soxu-border-strong)]"
                  : "bg-[var(--soxu-surface-card)] border-[var(--soxu-border-subtle)] hover:bg-[var(--soxu-interaction-hover)]",
              )}
            >
              <div className="flex items-center justify-between gap-2">
                <span className="soxu-mono text-xs font-semibold truncate">{w.branch}</span>
                <Badge variant={meta.variant}>{meta.label}</Badge>
              </div>
              <div className="text-xs text-[var(--soxu-text-secondary)] truncate">{w.summary}</div>
              <div className="flex items-center justify-between text-[11px] text-[var(--soxu-text-tertiary)] soxu-mono">
                <span>
                  {w.isolation === "worktree" ? "Worktree" : "Local"} · {w.agent}
                </span>
                <span className="soxu-tabular">{w.diffStat}</span>
              </div>
            </button>
          );
        })}
      </div>
    </aside>
  );

  const ScriptsAndSchedulesView = (
    <section className="flex flex-col gap-4">
      <div className="flex flex-col gap-2">
        <h2 className="text-xs font-semibold text-[var(--soxu-text-tertiary)] uppercase tracking-wider m-0">
          工作区脚本 (paseo.json / soxu.json)
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
                <Badge variant={scr.running ? "running" : "neutral"}>
                  {scr.running ? "运行中" : "空闲"}
                </Badge>
              </div>
              <div className="text-xs soxu-mono text-[var(--soxu-text-tertiary)] truncate mt-0.5">{scr.cmd}</div>
            </div>
            <div className="flex items-center gap-1.5 shrink-0">
              <Button variant="outline" size="sm" onClick={() => setSheet({ kind: "script-log", name: scr.name })}>
                日志
              </Button>
              <Button
                variant={scr.running ? "destructive" : "default"}
                size="sm"
                onClick={() =>
                  setScripts((prev) =>
                    prev.map((s) => (s.name === scr.name ? { ...s, running: !s.running } : s)),
                  )
                }
              >
                {scr.running ? "停止" : "启动"}
              </Button>
            </div>
          </div>
        ))}
      </div>

      <div className="flex flex-col gap-2">
        <h2 className="text-xs font-semibold text-[var(--soxu-text-tertiary)] uppercase tracking-wider m-0">
          定时调度与心跳巡检 (Cron)
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
                <Badge variant={sch.unreadFail ? "danger" : sch.active ? "running" : "neutral"}>{sch.kind}</Badge>
              </div>
              <div className="text-xs soxu-mono text-[var(--soxu-text-tertiary)] truncate mt-0.5">
                {sch.human} ({sch.cron}) · {sch.target}
              </div>
            </div>
            <Button
              variant={sch.active ? "secondary" : "outline"}
              size="sm"
              onClick={() =>
                setSchedules((prev) => prev.map((x) => (x.id === sch.id ? { ...x, active: !x.active } : x)))
              }
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
        {/* 顶部极简毛玻璃栏：无视角切换 Tabs，次级工具收纳至悬停 Dock */}
        <header
          className="sticky top-0 soxu-safe-top px-4 pb-2.5 backdrop-blur-md border-b border-[var(--soxu-border-hairline)] bg-[var(--soxu-surface-header-glass)]"
          style={{ zIndex: "var(--soxu-z-header)" }}
        >
          <div className="max-w-[1280px] mx-auto flex items-center justify-between gap-2">
            <div className="flex items-center gap-2.5 min-w-0">
              <button
                type="button"
                onClick={() => setSheet("pair")}
                className="soxu-touch rounded-full px-2.5 flex items-center gap-2 bg-[var(--soxu-surface-card)] border border-[var(--soxu-border-subtle)] cursor-pointer hover:border-[var(--soxu-border-strong)] transition-colors"
                style={{ minHeight: "36px" }}
                aria-label="管理配对设备与扫码连接"
              >
                <span
                  className={cn(
                    "w-2 h-2 rounded-full soxu-live-dot",
                    daemonOnline ? "bg-[var(--soxu-semantic-running)]" : "bg-[var(--soxu-semantic-accent)]",
                  )}
                />
                <QrCode className="h-3.5 w-3.5" strokeWidth={1.75} />
                <span className="text-xs font-semibold">Soxu</span>
              </button>

              <button
                type="button"
                onClick={simulateSeqCatchup}
                className="text-left min-w-0 bg-transparent border-0 p-0 cursor-pointer"
                title="点击模拟断线重连后游标增量追平"
              >
                <div className="text-[11.5px] soxu-mono soxu-tabular text-[var(--soxu-text-tertiary)] truncate">
                  {daemonOnline ? "Daemon 在线" : "遥控客户端"} · seq #{seqCursor}
                </div>
                <div className="text-xs soxu-mono text-[var(--soxu-text-secondary)] truncate">
                  {activeWorkspace.branch}
                </div>
              </button>
            </div>

            <div className="flex items-center gap-1 shrink-0">
              <HoverExpandDock
                triggerIcon={<SlidersHorizontal className="h-4 w-4" strokeWidth={1.75} />}
                triggerLabel="展开快捷动作"
                expandDirection="left"
              >
                <Button variant="ghost" size="icon-sm" onClick={() => setSheet("pair")} title="扫码配对">
                  <QrCode className="h-3.5 w-3.5" strokeWidth={1.75} />
                </Button>
                <Button variant="ghost" size="icon-sm" onClick={() => setSheet("history")} title="导入原生历史">
                  <History className="h-3.5 w-3.5" strokeWidth={1.75} />
                </Button>
                <Button variant="ghost" size="icon-sm" onClick={() => setSheet("new-worktree")} title="新建 Worktree">
                  <GitBranch className="h-3.5 w-3.5" strokeWidth={1.75} />
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
                aria-label="打开设置 (Ctrl+,)"
                title="设置 (Ctrl+,)"
                className="rounded-full"
              >
                <Settings className="h-4 w-4" strokeWidth={1.75} />
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
                    <span className="truncate">有 {pendingApprovals.length} 项高风险或工具操作等待你远程批准</span>
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
                      已消解记录 ({resolvedApprovals.length})
                    </Button>
                  </div>
                  {(approvalSubTab === "pending" ? pendingApprovals : resolvedApprovals).length === 0 ? (
                    <div className="rounded-2xl p-6 text-center text-xs text-[var(--soxu-text-tertiary)] bg-[var(--soxu-surface-card)] border border-[var(--soxu-border-subtle)]">
                      当前分类暂无审批记录
                    </div>
                  ) : (
                    (approvalSubTab === "pending" ? pendingApprovals : resolvedApprovals).map((a) =>
                      renderApprovalCard(a),
                    )
                  )}
                </section>
              )}

              {activeTab === "scripts" && ScriptsAndSchedulesView}

              {(activeTab === "session" || (sizeClass === "wide" && activeTab === "workspaces")) && (
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
                          <div key={turn.id} className="flex justify-end">
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
                                <CollapsibleContent className="soxu-collapsible overflow-hidden">
                                  <pre className="m-0 px-3 py-2.5 soxu-code-block overflow-x-auto leading-relaxed border-t border-[var(--soxu-border-hairline)] bg-[var(--soxu-surface-terminal)] text-[var(--soxu-text-code)]">
                                    {tl.lines.map((line, idx) => (
                                      <div
                                        key={idx}
                                        className={cn(
                                          line.startsWith("+")
                                            ? "text-[var(--soxu-semantic-success)]"
                                            : line.startsWith("-")
                                              ? "text-[var(--soxu-semantic-danger)]"
                                              : undefined,
                                        )}
                                      >
                                        {line}
                                      </div>
                                    ))}
                                  </pre>
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

                    {/* PendingUserInput 结构化提问卡 */}
                    <div className="rounded-2xl p-4 flex flex-col gap-3 bg-[var(--soxu-surface-card)] border border-[var(--soxu-border-strong)]">
                      <div className="flex items-center justify-between gap-2">
                        <Badge variant="accent">PendingUserInput</Badge>
                        <span className="text-[11px] text-[var(--soxu-text-tertiary)]">
                          {askQuestion.submitted ? "已回传决策" : "阻塞等待中"}
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
                                  : "bg-[var(--soxu-surface-card-muted)] border-[var(--soxu-border-subtle)] text-[var(--soxu-text-secondary)] hover:bg-[var(--soxu-interaction-hover)]",
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

                    {/* 内联未决审批置顶 */}
                    {pendingApprovals.map((item) => renderApprovalCard(item))}
                  </div>
                </>
              )}
            </div>

            {/* 底部悬浮极简输入框 */}
            {activeTab === "session" && (
              <div className="sticky bottom-0 px-3 pb-2 pt-2 bg-gradient-to-t from-[var(--soxu-surface-canvas)] via-[var(--soxu-surface-canvas)] to-transparent">
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
                      isRunning
                        ? `运行中（${behavior === "steer" ? "热纠偏" : behavior === "queue" ? "排队" : "打断"}模式）...`
                        : "输入指令、@agent 移交上下文，或粘贴任务描述..."
                    }
                    className="w-full resize-none bg-transparent border-0 outline-none px-1.5 py-1 text-base text-[var(--soxu-text-primary)] placeholder:text-[var(--soxu-text-ghost)]"
                  />

                  <div className="flex items-center justify-between gap-1.5">
                    <div className="flex items-center gap-1 min-w-0">
                      {/* 低频控制折叠至极简悬停 Dock */}
                      <HoverExpandDock
                        triggerIcon={<SlidersHorizontal className="h-3.5 w-3.5" strokeWidth={1.75} />}
                        triggerLabel="展开 CLI、思考与权限调节"
                        expandDirection="right"
                      >
                        <Button variant="ghost" size="sm" onClick={() => setSheet("cli")} className="h-7 px-2 text-[11.5px]">
                          <Terminal className="h-3 w-3" strokeWidth={1.75} />
                          <span>{cli}</span>
                        </Button>
                        <Button variant="ghost" size="sm" onClick={() => setSheet("thinking")} className="h-7 px-2 text-[11.5px]">
                          <Cpu className="h-3 w-3" strokeWidth={1.75} />
                          <span>{thinking}</span>
                        </Button>
                        <Button variant="ghost" size="sm" onClick={() => setSheet("perm")} className="h-7 px-2 text-[11.5px]">
                          <Shield className="h-3 w-3" strokeWidth={1.75} />
                          <span>{permLabel(permLevel)}</span>
                        </Button>
                        <Button variant="ghost" size="sm" onClick={() => setSheet("handoff")} className="h-7 px-2 text-[11.5px]">
                          <AtSign className="h-3 w-3" strokeWidth={1.75} />
                          <span>移交</span>
                        </Button>
                      </HoverExpandDock>

                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => setSheet("model")}
                        className="h-7 px-2 text-[11.5px] soxu-mono truncate max-w-[148px]"
                      >
                        {model.replace("group/", "")}
                      </Button>
                    </div>

                    <div className="flex items-center gap-1 shrink-0">
                      {isRunning && (
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={() =>
                            setBehavior((b) => (b === "steer" ? "queue" : b === "queue" ? "interrupt" : "steer"))
                          }
                          className="h-7 px-2 text-[11px]"
                        >
                          {behavior === "steer" ? "热纠偏" : behavior === "queue" ? "排队" : "打断"}
                        </Button>
                      )}
                      {isRunning && !draft.trim() ? (
                        <Button
                          variant="destructive"
                          size="icon"
                          onClick={() => {
                            setWorkspaces((prev) =>
                              prev.map((w) => (w.id === activeWorkspace.id ? { ...w, bucket: "done" } : w)),
                            );
                            flashToast("已发送 SIGINT 停止信号");
                          }}
                          aria-label="停止生成"
                        >
                          <Square className="h-3.5 w-3.5 fill-current" strokeWidth={1.75} />
                        </Button>
                      ) : (
                        <Button
                          variant="default"
                          size="icon"
                          disabled={!draft.trim()}
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
                { id: "session", label: "会话", icon: MessageSquare },
                { id: "workspaces", label: "工作区", icon: Folder },
                { id: "approvals", label: "审批", icon: Shield, count: pendingApprovals.length },
                { id: "scripts", label: "脚本", icon: Layers },
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

        {/* shadcn/ui 底部抽屉集合 */}
        <Sheet open={sheet !== null} onOpenChange={(o) => !o && setSheet(null)}>
          {sheet === "cli" && (
            <SheetContent title="切换原生 Agent CLI 驱动" subtitle="复用本机已登录认证与本地会话">
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
                  <div>
                    <div className="text-sm font-semibold">{c.id}</div>
                    <div className="text-xs soxu-mono text-[var(--soxu-text-tertiary)]">
                      {c.bin} · v{c.ver}
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
            <SheetContent title="选择执行模型" subtitle="支持供应商分组与上下文路由">
              {MODEL_OPTIONS.map((m) => (
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
                  <span className="text-sm font-medium soxu-mono">{lv}</span>
                  {thinking === lv && <Check className="h-4 w-4 text-[var(--soxu-semantic-accent)]" strokeWidth={2} />}
                </button>
              ))}
            </SheetContent>
          )}

          {sheet === "perm" && (
            <SheetContent title="执行权限策略档位" subtitle="高危破坏性命令无论何档位均强制人工确认">
              {(
                [
                  { id: "ask", title: "询问确认 (Ask)", desc: "每次写入或执行命令前请求确认" },
                  { id: "auto-edit", title: "自动编辑 (Auto Edit)", desc: "工作区内代码写入自动通过，终端命令需确认" },
                  { id: "full-access", title: "完全放行 (Full Access)", desc: "除高危黑名单外在 Worktree 内自主运行" },
                ] as const
              ).map((p) => (
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
                    <div className="text-sm font-semibold">{p.title}</div>
                    <div className="text-xs text-[var(--soxu-text-tertiary)]">{p.desc}</div>
                  </div>
                  {permLevel === p.id && <Check className="h-4 w-4 text-[var(--soxu-semantic-accent)]" strokeWidth={2} />}
                </button>
              ))}
            </SheetContent>
          )}

          {sheet === "handoff" && (
            <SheetContent title="@agent 跨代理上下文移交" subtitle="自动附带最近 3 轮摘要与未提交改动清单">
              {CLI_OPTIONS.filter((c) => c.available).map((c) => (
                <button
                  key={c.id}
                  type="button"
                  onClick={() => handleHandoff(c.id)}
                  className="w-full rounded-xl p-3 text-left flex items-center justify-between gap-2 bg-[var(--soxu-surface-card-muted)] border border-[var(--soxu-border-subtle)] cursor-pointer hover:bg-[var(--soxu-interaction-hover)]"
                >
                  <div>
                    <div className="text-sm font-semibold">@{c.id}</div>
                    <div className="text-xs soxu-mono text-[var(--soxu-text-tertiary)]">
                      {c.bin} · v{c.ver}
                    </div>
                  </div>
                  <Badge variant="handoff">移交</Badge>
                </button>
              ))}
            </SheetContent>
          )}

          {sheet === "rewind" && (
            <SheetContent title="检查点 #04 安全回退" subtitle="所有代码回退均会自动保存 Safety Stash 备份" tall>
              {[
                { label: "#04 (当前)", desc: "编辑 crates/soxu-drivers/src/lib.rs (+28 -3)" },
                { label: "#03", desc: "编辑 crates/soxu-core/src/event.rs (+12 -0)" },
              ].map((cp) => (
                <button
                  key={cp.label}
                  type="button"
                  onClick={() => handleApplyRewind(cp.label)}
                  className="w-full rounded-xl p-3 text-left flex items-center justify-between gap-2 bg-[var(--soxu-surface-card-muted)] border border-[var(--soxu-border-subtle)] cursor-pointer hover:bg-[var(--soxu-interaction-hover)]"
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
            <SheetContent title="扫码 / 配对码连接守护进程" subtitle="当前示例地址：127.0.0.1:7698" tall>
              <div className="rounded-xl p-3 bg-[var(--soxu-surface-card-muted)] border border-[var(--soxu-border-subtle)] text-xs space-y-2">
                <div className="font-semibold">URL Fragment 配对 (#offer=)</div>
                <p className="text-[var(--soxu-text-tertiary)] m-0">
                  配对握手参数仅停留在本地 URI Fragment 中，不会上传 HTTP Referer。
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
                  flashToast("已验证 Ed25519 指纹并建立本地安全连接");
                }}
              >
                验证指纹并上线
              </Button>
            </SheetContent>
          )}

          {sheet === "new-worktree" && (
            <SheetContent title="新建隔离 Git Worktree" subtitle="自动按 branch-slug 归一化并注入本地 AOCI / MCP 上下文">
              <input
                type="text"
                value={newBranchName}
                onChange={(e) => setNewBranchName(e.target.value)}
                placeholder="例如 feat/19-mobile-remote"
                className="w-full rounded-xl px-3 py-2.5 text-base soxu-mono bg-[var(--soxu-surface-composer)] border border-[var(--soxu-border-strong)] text-[var(--soxu-text-primary)] outline-none"
              />
              <Button variant="default" size="lg" onClick={handleCreateWorktree}>
                创建并切入 Worktree 会话
              </Button>
            </SheetContent>
          )}

          {sheet === "history" && (
            <SheetContent title="导入本机 CLI 原生历史会话" subtitle="只读扫描，支持原位续接或分叉至新 Worktree" tall>
              {NATIVE_HISTORY.map((item) => (
                <div
                  key={item.id}
                  className="rounded-xl p-3 flex items-center justify-between gap-2 bg-[var(--soxu-surface-card-muted)] border border-[var(--soxu-border-subtle)]"
                >
                  <div className="min-w-0">
                    <div className="text-sm font-semibold truncate">{item.title}</div>
                    <div className="text-xs soxu-mono text-[var(--soxu-text-tertiary)] truncate">
                      {item.cli} · {item.project} · {item.updatedAt} · {item.turns} 轮
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
              <pre className="m-0 p-3 rounded-xl soxu-code-block overflow-x-auto leading-relaxed bg-[var(--soxu-surface-terminal)] border border-[var(--soxu-border-subtle)] text-[var(--soxu-text-code)]">
                {activeScriptForLog.logs.join("\n")}
              </pre>
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
            flashToast(`工具与 MCP 路由已切换为 ${next}`);
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
