import { render } from "preact";
import { useEffect, useState } from "preact/hooks";
import { BottomSheet } from "./components/BottomSheet.tsx";
import {
  IconArrowUp,
  IconBolt,
  IconBranch,
  IconChat,
  IconCheck,
  IconCheckpoint,
  IconChevron,
  IconClock,
  IconFile,
  IconFolder,
  IconLayers,
  IconPlus,
  IconQr,
  IconRewind,
  IconSearch,
  IconShield,
  IconStop,
  IconSunMoon,
  IconTerminal,
} from "./icons.tsx";
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
} from "./mock-data.ts";
import type {
  ApprovalItem,
  AskQuestionState,
  MobileTab,
  PermissionLevel,
  ScheduleItem,
  ScriptItem,
  SheetKind,
  SizeClass,
  TurnBehavior,
  TurnMessage,
  WorkspaceBucket,
  WorkspaceItem,
} from "./types.ts";

function resolveSizeClass(width: number): SizeClass {
  if (width >= 1024) return "wide";
  if (width >= 640) return "regular";
  return "compact";
}

function App() {
  const [theme, setTheme] = useState<"dark" | "light">("dark");
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
  const [catchupToast, setCatchupToast] = useState<string | null>(null);
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
  const [sheet, setSheet] = useState<SheetKind>(null);
  const [pairCode, setPairCode] = useState<string>("");
  const [newBranchName, setNewBranchName] = useState<string>("feat/19-mobile-remote");

  useEffect(() => {
    const onResize = () => setSizeClass(resolveSizeClass(window.innerWidth));
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);

  useEffect(() => {
    document.documentElement.setAttribute("data-theme", theme);
    const meta = document.getElementById("soxu-theme-meta");
    if (meta) {
      meta.setAttribute("content", theme === "dark" ? "#000000" : "#ffffff");
    }
  }, [theme]);

  useEffect(() => {
    if (typeof window !== "undefined" && window.location.hash.includes("offer=")) {
      try {
        window.history.replaceState(null, "", window.location.pathname + window.location.search);
        setCatchupToast("已安全消费一次性配对令牌并清除地址栏 #offer= 片段");
      } catch {
        // ignore
      }
    }
    fetch("/api/health")
      .then((r) => (r.ok ? r.json() : null))
      .then((data) => {
        if (data && typeof data.magpie_mode === "string") {
          setDaemonOnline(true);
          setMagpieMode(data.magpie_mode === "standalone_native" ? "standalone_native" : "magpie_managed");
        }
      })
      .catch(() => setDaemonOnline(false));
  }, []);

  const pendingApprovals = approvals.filter((a) => !a.resolution);
  const resolvedApprovals = approvals.filter((a) => Boolean(a.resolution));
  const activeWorkspace = workspaces.find((w) => w.id === selectedWorkspaceId) ?? workspaces[0];
  const isTurnBusy = activeWorkspace.bucket === "running" || activeWorkspace.bucket === "needs_input";

  function flashToast(msg: string) {
    setCatchupToast(msg);
    setTimeout(() => {
      setCatchupToast((cur) => (cur === msg ? null : cur));
    }, 3600);
  }

  function simulateSeqCatchup() {
    const next = seqCursor + 8;
    setSeqCursor(next);
    flashToast(`已增量追平离线期间的 8 条更新 (seq #${seqCursor} → #${next})`);
  }

  function handleResolveApproval(id: string, resolution: "approved" | "session" | "rejected", note?: string) {
    setApprovals((prev) => prev.map((item) => (item.id === id ? { ...item, resolution, note } : item)));
    setSeqCursor((c) => c + 1);
    setRejectingId(null);
    setRejectReason("");
    flashToast(resolution === "rejected" ? "已拒绝并向 Agent 回传修改指导" : "已同步审批结果至桌面工作台");
  }

  function handleSelectModel(nextModelId: string) {
    setModel(nextModelId);
    const meta = MODEL_OPTIONS.find((m) => m.id === nextModelId);
    if (meta && !meta.thinkingOptions.includes(thinking)) {
      const old = thinking;
      const fallback = meta.thinkingOptions[0];
      setThinking(fallback);
      setLostControlNote(`已按 ${nextModelId} 自动调整思考档位 (${old} → ${fallback})`);
      setTimeout(() => setLostControlNote(null), 3500);
    }
    setSheet(null);
  }

  function handleSubmitPrompt() {
    const trimmed = draft.trim();
    if (!trimmed) return;
    if (isTurnBusy && behavior === "queue") {
      setQueuedPrompt(trimmed);
      setDraft("");
      setSeqCursor((c) => c + 1);
      return;
    }
    const badge = isTurnBusy
      ? behavior === "steer"
        ? "Steer 中途纠偏 · 下一步生效"
        : "Interrupt 已打断旧回合"
      : undefined;
    setTurns((prev) => [
      ...prev,
      { id: `t-${Date.now()}`, role: "user", author: "你 (移动遥控端)", text: trimmed, badge },
    ]);
    setDraft("");
    setSeqCursor((c) => c + 1);
  }

  function promoteQueuedToSteer() {
    if (!queuedPrompt) return;
    setTurns((prev) => [
      ...prev,
      { id: `t-${Date.now()}`, role: "user", author: "你 (提级纠偏)", text: queuedPrompt, badge: "由排队提级为 Steer" },
    ]);
    setQueuedPrompt(null);
    setSeqCursor((c) => c + 1);
  }

  function handleCreateWorktree() {
    const slug = newBranchName.trim().toLowerCase().replace(/[^a-z0-9/-]+/g, "-").replace(/^-+|-+$/g, "");
    if (!slug) return;
    const item: WorkspaceItem = {
      id: `wks-${Date.now()}`,
      project: "Soxu",
      branch: slug,
      isolation: "worktree",
      agent: cli,
      model,
      bucket: "done",
      summary: "新隔离 Worktree · 已为 direct MCP 补齐索引上下文",
      diffStat: "clean",
    };
    setWorkspaces((prev) => [item, ...prev]);
    setSelectedWorkspaceId(item.id);
    setSheet(null);
    setActiveTab("session");
    flashToast(`已创建 Worktree: ${slug}`);
  }

  const quickChips = isTurnBusy
    ? ["先跑 check-all 别直接提交", "删除前停下汇报", "完成后更新 PROGRESS.md"]
    : ["总结本轮改动与风险点", "运行全工作区单测", "生成 Git Emoji 提交预览"];

  const bucketMeta: Record<WorkspaceBucket, { label: string; color: string; bg: string }> = {
    needs_input: { label: "待审批", color: "var(--soxu-semantic-warning)", bg: "var(--soxu-semantic-warning-soft)" },
    failed: { label: "失败", color: "var(--soxu-semantic-danger)", bg: "var(--soxu-semantic-danger-soft)" },
    running: { label: "运行中", color: "var(--soxu-semantic-running)", bg: "var(--soxu-semantic-running-soft)" },
    attention: { label: "新完成", color: "var(--soxu-semantic-success)", bg: "var(--soxu-semantic-success-soft)" },
    done: { label: "空闲", color: "var(--soxu-text-tertiary)", bg: "var(--soxu-surface-control)" },
  };

  const filteredWorkspaces = bucketFilter === "all" ? workspaces : workspaces.filter((w) => w.bucket === bucketFilter);

  const WorkspaceRail = (
    <aside
      class="flex flex-col gap-3 p-3"
      style={{
        backgroundColor: "var(--soxu-surface-sidebar)",
        borderRight: sizeClass === "wide" ? "1px solid var(--soxu-border-hairline)" : "none",
        width: sizeClass === "wide" ? "320px" : "100%",
        flexShrink: 0,
      }}
    >
      <div class="flex items-center justify-between gap-2">
        <span class="text-xs font-semibold tracking-wider uppercase" style={{ color: "var(--soxu-text-tertiary)" }}>
          工作区 ({filteredWorkspaces.length})
        </span>
        <div class="flex items-center gap-1.5">
          <button
            type="button"
            onClick={() => setSheet("history")}
            class="soxu-touch px-2.5 py-1 rounded-full text-xs"
            style={{
              minHeight: "34px",
              backgroundColor: "var(--soxu-surface-control)",
              color: "var(--soxu-text-secondary)",
            }}
          >
            导入历史
          </button>
          <button
            type="button"
            onClick={() => setSheet("new-worktree")}
            class="soxu-touch px-2.5 py-1 rounded-full text-xs font-medium flex items-center gap-1"
            style={{
              minHeight: "34px",
              backgroundColor: "var(--soxu-action-inverse)",
              color: "var(--soxu-action-on-inverse)",
            }}
          >
            <IconPlus size={13} />
            <span>Worktree</span>
          </button>
        </div>
      </div>

      {/* 注意力状态桶筛选 */}
      <div class="flex items-center gap-1.5 overflow-x-auto soxu-no-scrollbar">
        {(
          [
            { id: "all", label: "全部" },
            { id: "needs_input", label: "待批" },
            { id: "failed", label: "失败" },
            { id: "running", label: "运行" },
            { id: "attention", label: "新完" },
          ] as const
        ).map((f) => {
          const active = bucketFilter === f.id;
          return (
            <button
              key={f.id}
              type="button"
              onClick={() => setBucketFilter(f.id)}
              class="soxu-touch px-2.5 py-1 rounded-full text-[11.5px] whitespace-nowrap"
              style={{
                minHeight: "30px",
                backgroundColor: active ? "var(--soxu-surface-raised)" : "var(--soxu-surface-card)",
                color: active ? "var(--soxu-text-primary)" : "var(--soxu-text-tertiary)",
                border: `1px solid ${active ? "var(--soxu-border-strong)" : "var(--soxu-border-subtle)"}`,
              }}
            >
              {f.label}
            </button>
          );
        })}
      </div>

      <div class="flex flex-col gap-2">
        {filteredWorkspaces.map((wks) => {
          const selected = wks.id === selectedWorkspaceId;
          const b = bucketMeta[wks.bucket];
          return (
            <button
              key={wks.id}
              type="button"
              onClick={() => {
                setSelectedWorkspaceId(wks.id);
                if (sizeClass !== "wide") setActiveTab("session");
              }}
              class="soxu-touch text-left rounded-xl p-3 flex flex-col gap-1.5"
              style={{
                backgroundColor: selected ? "var(--soxu-surface-raised)" : "var(--soxu-surface-card)",
                border: `1px solid ${selected ? "var(--soxu-border-strong)" : "var(--soxu-border-subtle)"}`,
              }}
            >
              <div class="flex items-center justify-between gap-2">
                <span class="flex items-center gap-1.5 font-medium text-sm min-w-0">
                  {wks.isolation === "worktree" ? <IconBranch size={15} /> : <IconFolder size={15} />}
                  <span class="soxu-mono truncate">{wks.branch}</span>
                </span>
                <span class="px-2 py-0.5 rounded-full text-[11px] font-medium shrink-0" style={{ backgroundColor: b.bg, color: b.color }}>
                  {b.label}
                </span>
              </div>
              <p class="text-xs truncate m-0" style={{ color: "var(--soxu-text-secondary)" }}>
                {wks.summary}
              </p>
              <div class="flex items-center justify-between text-[11px] soxu-mono soxu-tabular" style={{ color: "var(--soxu-text-tertiary)" }}>
                <span>
                  {wks.agent} · {wks.model}
                </span>
                <span class="flex items-center gap-1.5">
                  {wks.label && (
                    <span class="px-1.5 py-0.5 rounded" style={{ backgroundColor: "var(--soxu-surface-control)" }}>
                      {wks.label}
                    </span>
                  )}
                  <span>{wks.diffStat}</span>
                </span>
              </div>
            </button>
          );
        })}
      </div>
    </aside>
  );

  function renderApprovalCard(item: ApprovalItem) {
    if (item.resolution) {
      return (
        <div
          key={item.id}
          class="rounded-xl px-3.5 py-2.5 text-xs flex items-center justify-between gap-2"
          style={{
            backgroundColor: "var(--soxu-surface-card-muted)",
            border: "1px solid var(--soxu-border-subtle)",
            color: "var(--soxu-text-secondary)",
          }}
        >
          <span class="flex items-center gap-2 min-w-0">
            <IconCheck size={15} />
            <span class="truncate">
              {item.resolution === "approved"
                ? "已允许一次"
                : item.resolution === "session"
                  ? "本会话同类放行"
                  : `已拒绝：${item.note || "请修改方案"}`}
              {" · "}
              <code class="soxu-mono">{item.command}</code>
            </span>
          </span>
          <span class="soxu-tabular shrink-0" style={{ color: "var(--soxu-text-tertiary)" }}>
            已消解
          </span>
        </div>
      );
    }

    const expanded = Boolean(diffExpanded[item.id]);

    return (
      <article
        key={item.id}
        class="rounded-2xl p-4 flex flex-col gap-3"
        style={{
          backgroundColor: "var(--soxu-surface-card)",
          border: `1px solid ${item.highRisk ? "var(--soxu-semantic-danger-border)" : "var(--soxu-semantic-warning-border)"}`,
          borderLeftWidth: "3px",
          borderLeftColor: item.highRisk ? "var(--soxu-semantic-danger)" : "var(--soxu-semantic-warning)",
        }}
      >
        <div class="flex items-start justify-between gap-2">
          <div class="flex items-center gap-2 min-w-0">
            <span style={{ color: item.highRisk ? "var(--soxu-semantic-danger)" : "var(--soxu-semantic-warning)" }}>
              <IconShield size={18} />
            </span>
            <div class="min-w-0">
              <div class="text-sm font-semibold truncate">{item.toolKind}</div>
              <div class="text-xs soxu-mono truncate" style={{ color: "var(--soxu-text-tertiary)" }}>
                {item.branch} · {item.agent}
              </div>
            </div>
          </div>
          <span
            class="px-2 py-0.5 rounded-md text-[11px] font-medium shrink-0"
            style={{
              backgroundColor: item.highRisk ? "var(--soxu-semantic-danger-soft)" : "var(--soxu-semantic-warning-soft)",
              color: item.highRisk ? "var(--soxu-semantic-danger)" : "var(--soxu-semantic-warning)",
            }}
          >
            {item.highRisk ? "高危拦截" : "待确认"}
          </span>
        </div>

        <div
          class="rounded-xl p-3 soxu-mono text-xs overflow-x-auto"
          style={{
            backgroundColor: "var(--soxu-surface-terminal)",
            border: "1px solid var(--soxu-border-subtle)",
            color: "var(--soxu-text-code)",
          }}
        >
          ❯ {item.command}
        </div>

        <p
          class="text-xs m-0 leading-relaxed"
          style={{ color: item.highRisk ? "var(--soxu-semantic-danger)" : "var(--soxu-text-secondary)" }}
        >
          {item.riskNote}
        </p>

        {item.diffPreview.length > 0 && (
          <>
            <button
              type="button"
              onClick={() => setDiffExpanded((p) => ({ ...p, [item.id]: !expanded }))}
              class="soxu-touch self-start px-2.5 py-1 rounded-lg text-xs flex items-center gap-1.5"
              style={{
                minHeight: "34px",
                backgroundColor: "var(--soxu-surface-card-muted)",
                color: "var(--soxu-text-secondary)",
                border: "1px solid var(--soxu-border-subtle)",
              }}
              aria-expanded={expanded}
            >
              <IconChevron open={expanded} />
              <span>{expanded ? "收起关联 Diff" : `查看关联 Diff (${item.diffPreview.length} 行)`}</span>
            </button>
            {expanded && (
              <pre
                class="m-0 p-3 rounded-xl soxu-mono text-xs overflow-x-auto leading-relaxed"
                style={{
                  backgroundColor: "var(--soxu-surface-inset)",
                  border: "1px solid var(--soxu-border-subtle)",
                }}
              >
                {item.diffPreview.map((line, idx) => (
                  <div
                    key={idx}
                    style={{
                      color: line.startsWith("+")
                        ? "var(--soxu-semantic-success)"
                        : line.startsWith("-")
                          ? "var(--soxu-semantic-danger)"
                          : "var(--soxu-text-secondary)",
                    }}
                  >
                    {line}
                  </div>
                ))}
              </pre>
            )}
          </>
        )}

        {rejectingId === item.id ? (
          <div class="flex flex-col gap-2 pt-1">
            <label class="text-xs font-medium" htmlFor={`reject-${item.id}`}>
              向 Agent 附加拒绝与修改指导：
            </label>
            <input
              id={`reject-${item.id}`}
              type="text"
              value={rejectReason}
              onInput={(e) => setRejectReason((e.target as HTMLInputElement).value)}
              placeholder="例如：请改用 TempDir 并在用例退出后自动回收"
              class="w-full rounded-xl px-3 py-2.5"
              style={{
                fontSize: "var(--soxu-text-input-mobile)",
                backgroundColor: "var(--soxu-surface-composer)",
                border: "1px solid var(--soxu-border-strong)",
                color: "var(--soxu-text-primary)",
              }}
            />
            <div class="grid grid-cols-2 gap-2.5">
              <button
                type="button"
                onClick={() => setRejectingId(null)}
                class="soxu-touch rounded-xl text-sm font-medium"
                style={{ minHeight: "46px", backgroundColor: "var(--soxu-surface-control)", color: "var(--soxu-text-primary)" }}
              >
                取消
              </button>
              <button
                type="button"
                onClick={() => handleResolveApproval(item.id, "rejected", rejectReason)}
                class="soxu-touch rounded-xl text-sm font-semibold"
                style={{
                  minHeight: "46px",
                  backgroundColor: "var(--soxu-semantic-danger-soft)",
                  color: "var(--soxu-semantic-danger)",
                  border: "1px solid var(--soxu-semantic-danger-border)",
                }}
              >
                发送拒绝指导
              </button>
            </div>
          </div>
        ) : (
          <div class="grid grid-cols-2 gap-2.5 pt-1">
            <button
              type="button"
              onClick={() => setRejectingId(item.id)}
              class="soxu-touch rounded-xl text-sm font-medium"
              style={{
                minHeight: "48px",
                backgroundColor: "var(--soxu-surface-control)",
                color: "var(--soxu-text-primary)",
                border: "1px solid var(--soxu-border-subtle)",
              }}
            >
              拒绝 / 指导
            </button>
            <button
              type="button"
              onClick={() => handleResolveApproval(item.id, "approved")}
              class="soxu-touch rounded-xl text-sm font-semibold flex items-center justify-center gap-1.5"
              style={{
                minHeight: "48px",
                backgroundColor: item.highRisk ? "var(--soxu-semantic-danger-soft)" : "var(--soxu-action-inverse)",
                color: item.highRisk ? "var(--soxu-semantic-danger)" : "var(--soxu-action-on-inverse)",
                border: item.highRisk ? "1px solid var(--soxu-semantic-danger-border)" : "none",
              }}
            >
              <IconCheck size={17} />
              <span>{item.highRisk ? "确认只执行此一次" : "允许一次"}</span>
            </button>
          </div>
        )}
      </article>
    );
  }

  const ScriptsAndSchedulesView = (
    <section class="flex flex-col gap-4 py-1">
      <div
        class="rounded-2xl p-4 flex flex-col gap-2.5"
        style={{ backgroundColor: "var(--soxu-surface-card)", border: "1px solid var(--soxu-border-subtle)" }}
      >
        <div class="flex items-center justify-between gap-2">
          <span class="text-sm font-semibold">Magpie 与 CLI 拓扑</span>
          <button
            type="button"
            onClick={() => setMagpieMode((m) => (m === "magpie_managed" ? "standalone_native" : "magpie_managed"))}
            class="soxu-touch px-3 py-1 rounded-full text-xs font-medium"
            style={{
              minHeight: "32px",
              backgroundColor:
                magpieMode === "magpie_managed" ? "var(--soxu-semantic-accent-soft)" : "var(--soxu-semantic-warning-soft)",
              color: magpieMode === "magpie_managed" ? "var(--soxu-semantic-accent)" : "var(--soxu-semantic-warning)",
            }}
          >
            {magpieMode === "magpie_managed" ? "MagpieManaged 协同" : "StandaloneNative 独立"}
          </button>
        </div>
        <p class="text-xs m-0 leading-relaxed" style={{ color: "var(--soxu-text-secondary)" }}>
          {magpieMode === "magpie_managed"
            ? "网关 127.0.0.1:3425/v1 在线；新建 Worktree 自动为 direct MCP (aoci / codegraph) 补齐工作树上下文，全局配置只读无篡改。"
            : "未检测到 Magpie：已自动切换原生目录只读探测，各已装 CLI 独立完整可用。"}
        </p>
      </div>

      <div class="flex flex-col gap-2.5">
        <h2 class="text-xs font-semibold uppercase tracking-wider m-0 px-1" style={{ color: "var(--soxu-text-tertiary)" }}>
          Workspace 受管脚本 (paseo.json)
        </h2>
        {scripts.map((scr) => (
          <div
            key={scr.name}
            class="rounded-2xl p-3.5 flex items-center justify-between gap-3"
            style={{ backgroundColor: "var(--soxu-surface-card)", border: "1px solid var(--soxu-border-subtle)" }}
          >
            <button
              type="button"
              onClick={() => setSheet({ kind: "script-log", name: scr.name })}
              class="min-w-0 text-left bg-transparent border-0 p-0 flex-1"
            >
              <div class="flex items-center gap-2">
                <span
                  class="inline-block w-2 h-2 rounded-full"
                  style={{ backgroundColor: scr.running ? "var(--soxu-semantic-success)" : "var(--soxu-text-tertiary)" }}
                />
                <span class="font-medium text-sm soxu-mono">{scr.name}</span>
                {scr.port && (
                  <span class="px-1.5 py-0.5 rounded text-[11px] soxu-mono" style={{ backgroundColor: "var(--soxu-surface-control)" }}>
                    {scr.port}
                  </span>
                )}
              </div>
              <div class="text-xs soxu-mono truncate mt-1" style={{ color: "var(--soxu-text-tertiary)" }}>
                点击查看日志 · {scr.cmd}
              </div>
            </button>
            <button
              type="button"
              onClick={() => setScripts((prev) => prev.map((s) => (s.name === scr.name ? { ...s, running: !s.running } : s)))}
              class="soxu-touch px-3.5 rounded-xl text-xs font-semibold shrink-0"
              style={{
                backgroundColor: scr.running ? "var(--soxu-surface-control)" : "var(--soxu-action-inverse)",
                color: scr.running ? "var(--soxu-text-primary)" : "var(--soxu-action-on-inverse)",
              }}
            >
              {scr.running ? "停止" : "启动"}
            </button>
          </div>
        ))}
      </div>

      <div class="flex flex-col gap-2.5">
        <h2 class="text-xs font-semibold uppercase tracking-wider m-0 px-1" style={{ color: "var(--soxu-text-tertiary)" }}>
          Schedules &amp; Heartbeats 定时任务
        </h2>
        {schedules.map((sch) => (
          <div
            key={sch.id}
            class="rounded-2xl p-3.5 flex items-center justify-between gap-3"
            style={{ backgroundColor: "var(--soxu-surface-card)", border: "1px solid var(--soxu-border-subtle)" }}
          >
            <div class="min-w-0">
              <div class="flex items-center gap-2">
                <IconClock size={14} />
                <span class="text-sm font-medium truncate">{sch.name}</span>
                {sch.unreadFail && (
                  <span
                    class="px-1.5 py-0.5 rounded-full text-[10.5px] font-medium"
                    style={{ backgroundColor: "var(--soxu-semantic-danger-soft)", color: "var(--soxu-semantic-danger)" }}
                  >
                    未读失败
                  </span>
                )}
              </div>
              <div class="text-xs soxu-mono truncate mt-1" style={{ color: "var(--soxu-text-tertiary)" }}>
                {sch.cron} · {sch.human} · {sch.target}
              </div>
            </div>
            <button
              type="button"
              onClick={() =>
                setSchedules((prev) =>
                  prev.map((x) => (x.id === sch.id ? { ...x, active: !x.active, unreadFail: false } : x)),
                )
              }
              class="soxu-touch px-3 rounded-xl text-xs font-medium shrink-0"
              style={{
                backgroundColor: sch.active ? "var(--soxu-surface-control)" : "var(--soxu-semantic-accent-soft)",
                color: sch.active ? "var(--soxu-text-primary)" : "var(--soxu-semantic-accent)",
              }}
            >
              {sch.active ? "暂停" : "启用"}
            </button>
          </div>
        ))}
      </div>
    </section>
  );

  return (
    <div class="min-h-dvh flex flex-col" style={{ backgroundColor: "var(--soxu-surface-canvas)" }}>
      {/* 顶部浮动毛玻璃状态栏 */}
      <header
        class="sticky top-0 soxu-safe-top px-4 pb-2.5 backdrop-blur-md"
        style={{
          zIndex: "var(--soxu-z-header)",
          backgroundColor: "var(--soxu-surface-header-glass)",
          borderBottom: "1px solid var(--soxu-border-hairline)",
        }}
      >
        <div class="max-w-[1280px] mx-auto flex items-center justify-between gap-2">
          <div class="flex items-center gap-2.5 min-w-0">
            <button
              type="button"
              onClick={() => setSheet("pair")}
              class="soxu-touch rounded-full px-2.5 flex items-center gap-2"
              style={{
                minHeight: "36px",
                backgroundColor: "var(--soxu-surface-card)",
                border: "1px solid var(--soxu-border-subtle)",
              }}
              aria-label="管理配对设备与扫码连接"
            >
              <span class="w-2 h-2 rounded-full soxu-live-dot" style={{ backgroundColor: "var(--soxu-semantic-accent)" }} />
              <IconQr size={14} />
              <span class="text-xs font-semibold">Soxu</span>
            </button>

            <button
              type="button"
              onClick={simulateSeqCatchup}
              class="soxu-touch text-left min-w-0 bg-transparent border-0 p-0"
              title="点击模拟断线重连后游标增量追平"
            >
              <div class="text-[11.5px] soxu-mono soxu-tabular truncate" style={{ color: "var(--soxu-text-tertiary)" }}>
                {daemonOnline ? "Daemon 在线" : "遥控原型"} · seq #{seqCursor}
              </div>
              <div class="text-xs soxu-mono truncate" style={{ color: "var(--soxu-text-secondary)" }}>
                {activeWorkspace.branch}
              </div>
            </button>
          </div>

          <button
            type="button"
            onClick={() => setTheme((t) => (t === "dark" ? "light" : "dark"))}
            class="soxu-touch rounded-full flex items-center justify-center px-3 gap-1.5 text-xs font-medium shrink-0"
            style={{
              minHeight: "36px",
              backgroundColor: "var(--soxu-surface-card)",
              border: "1px solid var(--soxu-border-subtle)",
              color: "var(--soxu-text-secondary)",
            }}
            aria-label={theme === "dark" ? "切换日光主题" : "切换 Pure Black 主题"}
          >
            <IconSunMoon size={15} />
            <span>{theme === "dark" ? "Pure Black" : "Light"}</span>
          </button>
        </div>

        {catchupToast && (
          <div
            class="max-w-[760px] mx-auto mt-2 px-3 py-1.5 rounded-xl text-xs flex items-center justify-between gap-2"
            style={{
              backgroundColor: "var(--soxu-semantic-accent-soft)",
              border: "1px solid var(--soxu-border-subtle)",
              color: "var(--soxu-semantic-accent)",
            }}
          >
            <span class="flex items-center gap-1.5 truncate">
              <IconBolt size={13} />
              <span class="truncate">{catchupToast}</span>
            </span>
          </div>
        )}
      </header>

      {/* 主工作区 */}
      <div class="flex-1 flex max-w-[1280px] w-full mx-auto">
        {sizeClass === "wide" && WorkspaceRail}

        <main class="flex-1 flex flex-col justify-between min-w-0">
          <div
            class="w-full mx-auto px-4 py-4 flex flex-col gap-4"
            style={{ maxWidth: "var(--soxu-layout-transcript-max-width)" }}
          >
            {pendingApprovals.length > 0 && activeTab !== "approvals" && (
              <button
                type="button"
                onClick={() => setActiveTab("approvals")}
                class="soxu-touch w-full rounded-xl p-3 flex items-center justify-between text-left"
                style={{
                  backgroundColor: "var(--soxu-semantic-warning-soft)",
                  border: "1px solid var(--soxu-semantic-warning-border)",
                  color: "var(--soxu-semantic-warning)",
                }}
              >
                <span class="flex items-center gap-2 text-xs font-semibold">
                  <IconShield size={16} />
                  <span>{pendingApprovals.length} 项权限待确认（含高危命令拦截）</span>
                </span>
                <span class="text-xs underline shrink-0">去处理</span>
              </button>
            )}

            {sizeClass !== "wide" && activeTab === "workspaces" && WorkspaceRail}

            {activeTab === "approvals" && (
              <div class="flex flex-col gap-3">
                <div
                  class="grid grid-cols-2 p-1 rounded-xl"
                  style={{ backgroundColor: "var(--soxu-surface-inset)", border: "1px solid var(--soxu-border-hairline)" }}
                >
                  {(
                    [
                      { id: "pending", label: `待处理 (${pendingApprovals.length})` },
                      { id: "history", label: `已消解记录 (${resolvedApprovals.length})` },
                    ] as const
                  ).map((st) => (
                    <button
                      key={st.id}
                      type="button"
                      onClick={() => setApprovalSubTab(st.id)}
                      class="soxu-touch rounded-lg text-xs font-medium"
                      style={{
                        minHeight: "36px",
                        backgroundColor: approvalSubTab === st.id ? "var(--soxu-surface-raised)" : "transparent",
                        color: approvalSubTab === st.id ? "var(--soxu-text-primary)" : "var(--soxu-text-tertiary)",
                      }}
                    >
                      {st.label}
                    </button>
                  ))}
                </div>
                <div class="flex flex-col gap-3">
                  {(approvalSubTab === "pending" ? pendingApprovals : resolvedApprovals).length === 0 ? (
                    <div
                      class="rounded-2xl p-6 text-center text-xs"
                      style={{
                        backgroundColor: "var(--soxu-surface-card)",
                        border: "1px solid var(--soxu-border-subtle)",
                        color: "var(--soxu-text-tertiary)",
                      }}
                    >
                      当前分类暂无审批记录
                    </div>
                  ) : (
                    (approvalSubTab === "pending" ? pendingApprovals : resolvedApprovals).map((a) => renderApprovalCard(a))
                  )}
                </div>
              </div>
            )}

            {activeTab === "scripts" && ScriptsAndSchedulesView}

            {(activeTab === "session" || (sizeClass === "wide" && activeTab === "workspaces")) && (
              <div class="flex flex-col gap-5">
                {turns.map((turn) =>
                  turn.role === "user" ? (
                    <div key={turn.id} class="flex flex-col items-end gap-1">
                      {turn.badge && (
                        <span
                          class="text-[11px] px-2.5 py-0.5 rounded-full font-medium flex items-center gap-1"
                          style={{
                            backgroundColor: "var(--soxu-semantic-running-soft)",
                            color: "var(--soxu-semantic-running)",
                          }}
                        >
                          <IconBolt size={12} />
                          <span>{turn.badge}</span>
                        </span>
                      )}
                      <div
                        class="rounded-[18px] px-4 py-2.5 text-sm max-w-[84%] leading-relaxed"
                        style={{ backgroundColor: "var(--soxu-surface-raised)", color: "var(--soxu-text-primary)" }}
                      >
                        {turn.text}
                      </div>
                    </div>
                  ) : (
                    <div key={turn.id} class="flex flex-col gap-2.5">
                      <div class="flex items-center justify-between text-xs">
                        <span class="font-semibold">{turn.author}</span>
                        {turn.reasoningSec && (
                          <button
                            type="button"
                            onClick={() => setOpenReasons((p) => ({ ...p, [turn.id]: !p[turn.id] }))}
                            class="soxu-touch px-2 py-0.5 rounded-md flex items-center gap-1 bg-transparent border-0"
                            style={{ minHeight: "32px", color: "var(--soxu-text-tertiary)" }}
                          >
                            <IconChevron open={Boolean(openReasons[turn.id])} size={13} />
                            <span>已深度思考 {turn.reasoningSec}</span>
                          </button>
                        )}
                      </div>

                      {openReasons[turn.id] && turn.reasoningSteps && (
                        <div
                          class="pl-3 ml-1 flex flex-col gap-1 text-xs"
                          style={{ borderLeft: "2px solid var(--soxu-border-subtle)", color: "var(--soxu-text-secondary)" }}
                        >
                          {turn.reasoningSteps.map((st, idx) => (
                            <div key={idx}>{st}</div>
                          ))}
                        </div>
                      )}

                      {turn.tools?.map((tl) => {
                        const isOpen = Boolean(openTools[tl.id]);
                        return (
                          <div
                            key={tl.id}
                            class="rounded-xl overflow-hidden"
                            style={{ backgroundColor: "var(--soxu-surface-card)", border: "1px solid var(--soxu-border-subtle)" }}
                          >
                            <button
                              type="button"
                              onClick={() => setOpenTools((p) => ({ ...p, [tl.id]: !isOpen }))}
                              class="soxu-touch w-full px-3 py-2 flex items-center justify-between gap-2 text-left text-xs"
                              style={{ backgroundColor: "var(--soxu-surface-card-muted)", color: "var(--soxu-text-secondary)" }}
                              aria-expanded={isOpen}
                            >
                              <span class="flex items-center gap-2 min-w-0 soxu-mono">
                                {tl.kind === "read" ? (
                                  <IconSearch size={13} />
                                ) : tl.kind === "edit" ? (
                                  <IconFile size={13} />
                                ) : tl.kind === "bash" ? (
                                  <IconTerminal size={13} />
                                ) : (
                                  <IconLayers size={13} />
                                )}
                                <span class="truncate">{tl.title}</span>
                              </span>
                              <span class="flex items-center gap-1.5 shrink-0">
                                <span style={{ color: "var(--soxu-semantic-accent)" }}>{tl.status}</span>
                                <IconChevron open={isOpen} size={13} />
                              </span>
                            </button>
                            {isOpen && (
                              <div class="p-3 soxu-mono text-xs overflow-x-auto flex flex-col gap-0.5">
                                {tl.lines.map((ln, idx) => (
                                  <div
                                    key={idx}
                                    style={{
                                      color: ln.startsWith("+")
                                        ? "var(--soxu-semantic-success)"
                                        : ln.startsWith("$")
                                          ? "var(--soxu-text-tertiary)"
                                          : "var(--soxu-text-secondary)",
                                    }}
                                  >
                                    {ln}
                                  </div>
                                ))}
                              </div>
                            )}
                          </div>
                        );
                      })}

                      <p class="m-0 text-sm leading-relaxed" style={{ color: "var(--soxu-text-primary)" }}>
                        {turn.text}
                      </p>

                      {turn.checkpoint && (
                        <div class="flex items-center gap-2 pt-1">
                          <div class="h-px flex-1" style={{ backgroundColor: "var(--soxu-border-hairline)" }} />
                          <button
                            type="button"
                            onClick={() => setSheet("rewind")}
                            class="soxu-touch inline-flex items-center gap-1.5 px-3 py-1 rounded-full text-[11px] soxu-mono"
                            style={{
                              minHeight: "32px",
                              backgroundColor: "var(--soxu-semantic-handoff-soft)",
                              color: "var(--soxu-semantic-handoff)",
                              border: "1px solid var(--soxu-border-subtle)",
                            }}
                          >
                            <IconCheckpoint size={13} />
                            <span>
                              检查点 {turn.checkpoint.id} · {turn.checkpoint.sha}
                            </span>
                            <IconRewind size={12} />
                          </button>
                          <div class="h-px flex-1" style={{ backgroundColor: "var(--soxu-border-hairline)" }} />
                        </div>
                      )}
                    </div>
                  ),
                )}

                {/* PendingUserInput 结构化提问卡 */}
                <article
                  class="rounded-2xl p-4 flex flex-col gap-3"
                  style={{
                    backgroundColor: "var(--soxu-surface-card)",
                    border: "1px solid var(--soxu-semantic-running-border)",
                    borderLeftWidth: "3px",
                    borderLeftColor: "var(--soxu-semantic-accent)",
                  }}
                >
                  <div class="flex items-center justify-between gap-2">
                    <span class="text-sm font-semibold">需要确认架构策略</span>
                    <span
                      class="px-2 py-0.5 rounded-full text-[11px] font-medium"
                      style={{ backgroundColor: "var(--soxu-semantic-running-soft)", color: "var(--soxu-semantic-running)" }}
                    >
                      PendingUserInput
                    </span>
                  </div>
                  <p class="m-0 text-xs" style={{ color: "var(--soxu-text-secondary)" }}>
                    {askQuestion.prompt}
                  </p>
                  <div class="flex flex-col gap-2">
                    {askQuestion.options.map((opt) => {
                      const active = askQuestion.selectedId === opt.id;
                      return (
                        <button
                          key={opt.id}
                          type="button"
                          onClick={() => setAskQuestion((q) => ({ ...q, selectedId: opt.id }))}
                          class="soxu-touch rounded-xl px-3 py-2.5 text-left text-xs flex items-center justify-between gap-2"
                          style={{
                            backgroundColor: active ? "var(--soxu-surface-raised)" : "var(--soxu-surface-card-muted)",
                            border: `1px solid ${active ? "var(--soxu-border-strong)" : "var(--soxu-border-subtle)"}`,
                          }}
                        >
                          <span>{opt.label}</span>
                          {opt.recommended && (
                            <span
                              class="px-1.5 py-0.5 rounded text-[10.5px] shrink-0"
                              style={{ backgroundColor: "var(--soxu-semantic-accent-soft)", color: "var(--soxu-semantic-accent)" }}
                            >
                              推荐
                            </span>
                          )}
                        </button>
                      );
                    })}
                  </div>
                  <button
                    type="button"
                    onClick={() => {
                      setAskQuestion((q) => ({ ...q, submitted: true }));
                      flashToast("已提交结构化问题回答");
                    }}
                    class="soxu-touch rounded-xl text-sm font-semibold"
                    style={{
                      minHeight: "44px",
                      backgroundColor: askQuestion.submitted ? "var(--soxu-surface-control)" : "var(--soxu-action-inverse)",
                      color: askQuestion.submitted ? "var(--soxu-semantic-accent)" : "var(--soxu-action-on-inverse)",
                    }}
                  >
                    {askQuestion.submitted ? "已确认提交回答" : "确认提交选择"}
                  </button>
                </article>

                {pendingApprovals.map((item) => renderApprovalCard(item))}
              </div>
            )}
          </div>

          {/* 底部三态遥控输入舱 */}
          <div class="sticky bottom-0 px-3 pt-2 soxu-safe-bottom" style={{ zIndex: "var(--soxu-z-sticky)", backgroundColor: "var(--soxu-surface-canvas)" }}>
            <div class="w-full mx-auto flex flex-col gap-2" style={{ maxWidth: "var(--soxu-layout-composer-max-width)" }}>
              {lostControlNote && (
                <div class="px-3 text-[11.5px]" style={{ color: "var(--soxu-semantic-warning)" }}>
                  {lostControlNote}
                </div>
              )}

              <div class="flex items-center gap-2 overflow-x-auto soxu-no-scrollbar py-0.5">
                {quickChips.map((chip) => (
                  <button
                    key={chip}
                    type="button"
                    onClick={() => setDraft(chip)}
                    class="soxu-touch shrink-0 px-3 py-1.5 rounded-full text-xs whitespace-nowrap"
                    style={{
                      minHeight: "34px",
                      backgroundColor: "var(--soxu-surface-card)",
                      border: "1px solid var(--soxu-border-subtle)",
                      color: "var(--soxu-text-secondary)",
                    }}
                  >
                    {chip}
                  </button>
                ))}
              </div>

              {queuedPrompt && (
                <div
                  class="rounded-xl px-3 py-2 flex items-center justify-between gap-2 text-xs"
                  style={{ backgroundColor: "var(--soxu-surface-card)", border: "1px solid var(--soxu-semantic-warning-border)" }}
                >
                  <span class="truncate" style={{ color: "var(--soxu-text-secondary)" }}>
                    排队中：{queuedPrompt}
                  </span>
                  <div class="flex items-center gap-1.5 shrink-0">
                    <button
                      type="button"
                      onClick={promoteQueuedToSteer}
                      class="soxu-touch px-2.5 py-1 rounded-lg font-semibold flex items-center gap-1"
                      style={{
                        minHeight: "32px",
                        backgroundColor: "var(--soxu-semantic-running-soft)",
                        color: "var(--soxu-semantic-running)",
                      }}
                    >
                      <IconBolt size={12} />
                      <span>立即纠偏</span>
                    </button>
                    <button
                      type="button"
                      onClick={() => setQueuedPrompt(null)}
                      class="soxu-touch px-2 py-1 rounded-lg"
                      style={{ minHeight: "32px", color: "var(--soxu-text-tertiary)" }}
                    >
                      撤回
                    </button>
                  </div>
                </div>
              )}

              <div
                class="rounded-[22px] p-2.5 flex flex-col gap-2"
                style={{
                  backgroundColor: "var(--soxu-surface-composer)",
                  border: `1px solid ${
                    isTurnBusy && behavior === "steer"
                      ? "var(--soxu-semantic-running-border)"
                      : isTurnBusy && behavior === "interrupt"
                        ? "var(--soxu-semantic-danger-border)"
                        : "var(--soxu-border-subtle)"
                  }`,
                }}
              >
                <div class="flex items-center gap-1.5 overflow-x-auto soxu-no-scrollbar px-1">
                  <button
                    type="button"
                    onClick={() => setSheet("handoff")}
                    class="soxu-touch px-2.5 py-1 rounded-full text-[11.5px] soxu-mono whitespace-nowrap"
                    style={{
                      minHeight: "28px",
                      backgroundColor: "var(--soxu-semantic-handoff-soft)",
                      color: "var(--soxu-semantic-handoff)",
                    }}
                  >
                    @agent 移交
                  </button>
                  <button
                    type="button"
                    onClick={() => setSheet("cli")}
                    class="soxu-touch px-2.5 py-1 rounded-full text-[11.5px] soxu-mono whitespace-nowrap"
                    style={{ minHeight: "28px", backgroundColor: "var(--soxu-surface-control)", color: "var(--soxu-text-secondary)" }}
                  >
                    {cli}
                  </button>
                  <button
                    type="button"
                    onClick={() => setSheet("model")}
                    class="soxu-touch px-2.5 py-1 rounded-full text-[11.5px] soxu-mono whitespace-nowrap"
                    style={{ minHeight: "28px", backgroundColor: "var(--soxu-surface-control)", color: "var(--soxu-text-secondary)" }}
                  >
                    {model}
                  </button>
                  <button
                    type="button"
                    onClick={() => setSheet("thinking")}
                    class="soxu-touch px-2.5 py-1 rounded-full text-[11.5px] soxu-mono whitespace-nowrap"
                    style={{ minHeight: "28px", backgroundColor: "var(--soxu-surface-control)", color: "var(--soxu-text-secondary)" }}
                  >
                    {thinking}
                  </button>
                  <button
                    type="button"
                    onClick={() => setSheet("perm")}
                    class="soxu-touch px-2.5 py-1 rounded-full text-[11.5px] whitespace-nowrap"
                    style={{ minHeight: "28px", backgroundColor: "var(--soxu-surface-control)", color: "var(--soxu-text-secondary)" }}
                  >
                    {permLevel}
                  </button>
                </div>

                <label class="sr-only" htmlFor="soxu-remote-composer">
                  向当前工作区发送指令
                </label>
                <textarea
                  id="soxu-remote-composer"
                  rows={2}
                  value={draft}
                  onInput={(e) => setDraft((e.target as HTMLTextAreaElement).value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" && !e.shiftKey) {
                      e.preventDefault();
                      handleSubmitPrompt();
                    }
                  }}
                  placeholder={
                    isTurnBusy
                      ? behavior === "steer"
                        ? "输入中途纠偏指令（下一步注入）..."
                        : behavior === "queue"
                          ? "输入排队指令（本轮结束自动发送）..."
                          : "输入新指令（立即打断当前回合）..."
                      : "输入指令，或点上方 @agent 跨代理移交..."
                  }
                  class="w-full resize-none bg-transparent border-0 px-2 py-0.5 focus:outline-none"
                  style={{ fontSize: "var(--soxu-text-input-mobile)", color: "var(--soxu-text-primary)" }}
                />

                <div class="flex items-center justify-between gap-2 px-1">
                  <div class="flex items-center gap-1.5 overflow-x-auto soxu-no-scrollbar">
                    {(
                      [
                        { id: "steer", label: "纠偏 Steer" },
                        { id: "queue", label: "排队 Queue" },
                        { id: "interrupt", label: "打断重发" },
                      ] as const
                    ).map((mode) => {
                      const active = behavior === mode.id;
                      return (
                        <button
                          key={mode.id}
                          type="button"
                          onClick={() => setBehavior(mode.id)}
                          class="soxu-touch px-2.5 py-1 rounded-full text-xs font-medium whitespace-nowrap"
                          style={{
                            minHeight: "32px",
                            backgroundColor: active ? "var(--soxu-action-inverse)" : "var(--soxu-surface-control)",
                            color: active ? "var(--soxu-action-on-inverse)" : "var(--soxu-text-secondary)",
                          }}
                          aria-pressed={active}
                        >
                          {mode.label}
                        </button>
                      );
                    })}
                  </div>

                  {isTurnBusy && !draft.trim() ? (
                    <button
                      type="button"
                      onClick={() =>
                        setWorkspaces((prev) =>
                          prev.map((w) =>
                            w.id === activeWorkspace.id ? { ...w, bucket: "done", summary: "已由移动遥控端手动停止" } : w,
                          ),
                        )
                      }
                      class="soxu-touch rounded-full flex items-center justify-center shrink-0"
                      style={{
                        width: "40px",
                        height: "40px",
                        backgroundColor: "var(--soxu-action-inverse)",
                        color: "var(--soxu-action-on-inverse)",
                      }}
                      aria-label="停止当前活跃回合"
                    >
                      <IconStop size={15} />
                    </button>
                  ) : (
                    <button
                      type="button"
                      disabled={!draft.trim()}
                      onClick={handleSubmitPrompt}
                      class="soxu-touch rounded-full flex items-center justify-center shrink-0 disabled:cursor-not-allowed"
                      style={{
                        width: "40px",
                        height: "40px",
                        backgroundColor: draft.trim() ? "var(--soxu-action-inverse)" : "var(--soxu-action-disabled-bg)",
                        color: draft.trim() ? "var(--soxu-action-on-inverse)" : "var(--soxu-action-disabled-fg)",
                      }}
                      aria-label="发送指令"
                    >
                      <IconArrowUp size={18} />
                    </button>
                  )}
                </div>
              </div>

              {sizeClass !== "wide" && (
                <nav
                  class="grid grid-cols-4 gap-1 pt-1"
                  style={{ borderTop: "1px solid var(--soxu-border-hairline)" }}
                  aria-label="移动遥控主导航"
                >
                  {(
                    [
                      { id: "session", label: "当前会话", icon: <IconChat size={18} /> },
                      { id: "workspaces", label: "工作区", icon: <IconLayers size={18} /> },
                      {
                        id: "approvals",
                        label: pendingApprovals.length > 0 ? `待审批 (${pendingApprovals.length})` : "审批箱",
                        icon: <IconShield size={18} />,
                      },
                      { id: "scripts", label: "脚本/调度", icon: <IconTerminal size={18} /> },
                    ] as const
                  ).map((nav) => {
                    const active = activeTab === nav.id;
                    return (
                      <button
                        key={nav.id}
                        type="button"
                        onClick={() => setActiveTab(nav.id)}
                        class="soxu-touch rounded-xl flex flex-col items-center justify-center gap-0.5 py-1.5 text-[11px] font-medium"
                        style={{
                          color: active ? "var(--soxu-text-primary)" : "var(--soxu-text-tertiary)",
                          backgroundColor: active ? "var(--soxu-interaction-selected)" : "transparent",
                        }}
                        aria-current={active ? "page" : undefined}
                      >
                        {nav.icon}
                        <span>{nav.label}</span>
                      </button>
                    );
                  })}
                </nav>
              )}
            </div>
          </div>
        </main>
      </div>

      {/* ── Bottom Sheets ─────────────────────────────────────────────── */}
      {sheet === "cli" && (
        <BottomSheet title="切换原生 Agent CLI 驱动" subtitle="复用本机已登录认证与本地会话" onClose={() => setSheet(null)}>
          {CLI_OPTIONS.map((c) => (
            <button
              key={c.id}
              type="button"
              disabled={!c.available}
              onClick={() => {
                setCli(c.id);
                setSheet(null);
              }}
              class="soxu-touch rounded-xl px-3.5 py-3 flex items-center justify-between text-left"
              style={{
                backgroundColor: cli === c.id ? "var(--soxu-surface-raised)" : "var(--soxu-surface-card-muted)",
                border: `1px solid ${cli === c.id ? "var(--soxu-border-strong)" : "var(--soxu-border-subtle)"}`,
                opacity: c.available ? 1 : 0.48,
              }}
            >
              <span class="soxu-mono text-sm font-medium">
                {c.id} ({c.bin})
              </span>
              <span class="text-xs" style={{ color: "var(--soxu-text-tertiary)" }}>
                {c.available ? `v${c.ver}` : "未安装"}
              </span>
            </button>
          ))}
        </BottomSheet>
      )}

      {sheet === "model" && (
        <BottomSheet
          title={magpieMode === "magpie_managed" ? "Magpie 路由模型池" : "CLI 原生模型目录"}
          subtitle="切换模型时若不支持原思考档位将自动安全回退"
          tall
          onClose={() => setSheet(null)}
        >
          {MODEL_OPTIONS.map((m) => (
            <button
              key={m.id}
              type="button"
              onClick={() => handleSelectModel(m.id)}
              class="soxu-touch rounded-xl px-3.5 py-3 flex items-center justify-between gap-2 text-left"
              style={{
                backgroundColor: model === m.id ? "var(--soxu-surface-raised)" : "var(--soxu-surface-card-muted)",
                border: `1px solid ${model === m.id ? "var(--soxu-border-strong)" : "var(--soxu-border-subtle)"}`,
              }}
            >
              <div class="min-w-0">
                <div class="soxu-mono text-sm font-medium truncate">{m.id}</div>
                <div class="text-xs" style={{ color: "var(--soxu-text-tertiary)" }}>
                  {m.tier} · {m.note}
                </div>
              </div>
              <span class="text-[11px] soxu-mono shrink-0" style={{ color: "var(--soxu-semantic-accent)" }}>
                {m.thinkingOptions.join("/")}
              </span>
            </button>
          ))}
        </BottomSheet>
      )}

      {sheet === "thinking" && (
        <BottomSheet title="选择思考强度" subtitle={`当前模型：${model}`} onClose={() => setSheet(null)}>
          {(MODEL_OPTIONS.find((m) => m.id === model)?.thinkingOptions ?? ["low", "medium", "high"]).map((lv) => (
            <button
              key={lv}
              type="button"
              onClick={() => {
                setThinking(lv);
                setSheet(null);
              }}
              class="soxu-touch rounded-xl px-3.5 py-3 flex items-center justify-between text-left"
              style={{
                backgroundColor: thinking === lv ? "var(--soxu-surface-raised)" : "var(--soxu-surface-card-muted)",
                border: `1px solid ${thinking === lv ? "var(--soxu-border-strong)" : "var(--soxu-border-subtle)"}`,
              }}
            >
              <span class="soxu-mono text-sm">thinking: {lv}</span>
              {thinking === lv && <IconCheck size={16} />}
            </button>
          ))}
        </BottomSheet>
      )}

      {sheet === "perm" && (
        <BottomSheet title="执行权限策略档位" subtitle="高危破坏性命令无论何档位均强制人工确认" onClose={() => setSheet(null)}>
          {(
            [
              { id: "ask", label: "Ask（推荐）", desc: "编辑工作区外路径或运行关键命令均需人工确认" },
              { id: "auto-edit", label: "Auto-Edit", desc: "当前 Worktree 内代码修改自动放行，终端仍审批" },
              { id: "full-access", label: "Full-Access", desc: "受控隔离 Worktree 免交互推进（仍拦截删除红线）" },
            ] as const
          ).map((lv) => (
            <button
              key={lv.id}
              type="button"
              onClick={() => {
                setPermLevel(lv.id);
                setSheet(null);
              }}
              class="soxu-touch rounded-xl px-3.5 py-3 flex flex-col gap-0.5 text-left"
              style={{
                backgroundColor: permLevel === lv.id ? "var(--soxu-surface-raised)" : "var(--soxu-surface-card-muted)",
                border: `1px solid ${permLevel === lv.id ? "var(--soxu-border-strong)" : "var(--soxu-border-subtle)"}`,
              }}
            >
              <span class="text-sm font-semibold">{lv.label}</span>
              <span class="text-xs" style={{ color: "var(--soxu-text-tertiary)" }}>
                {lv.desc}
              </span>
            </button>
          ))}
        </BottomSheet>
      )}

      {sheet === "handoff" && (
        <BottomSheet title="@agent 跨代理上下文移交" subtitle="自动附带最近 3 轮摘要与未提交改动清单" onClose={() => setSheet(null)}>
          {["codex", "pi", "agy", "claude"].map((target) => (
            <button
              key={target}
              type="button"
              onClick={() => {
                setDraft((d) => `@${target} ` + d);
                setSheet(null);
              }}
              class="soxu-touch rounded-xl px-3.5 py-3 flex items-center justify-between text-left"
              style={{
                backgroundColor: "var(--soxu-surface-card-muted)",
                border: "1px solid var(--soxu-border-subtle)",
              }}
            >
              <span class="soxu-mono text-sm" style={{ color: "var(--soxu-semantic-handoff)" }}>
                @{target}
              </span>
              <span class="text-xs" style={{ color: "var(--soxu-text-tertiary)" }}>
                插入移交上下文胶囊
              </span>
            </button>
          ))}
        </BottomSheet>
      )}

      {sheet === "rewind" && (
        <BottomSheet title="检查点 #04 安全回退" subtitle="所有代码回退均会自动保存 Safety Stash 备份" tall onClose={() => setSheet(null)}>
          {[
            "1. 仅分叉对话（保留当前磁盘代码不变 · 默认安全）",
            "2. 仅回退工作树代码至 commit 7c4e91a",
            "3. 代码与对话同步完整回退至 #04",
          ].map((opt, i) => (
            <button
              key={i}
              type="button"
              onClick={() => {
                setSheet(null);
                flashToast(`已触发：${opt}`);
              }}
              class="soxu-touch rounded-xl px-3.5 py-3.5 text-left text-xs font-medium"
              style={{
                backgroundColor: i === 0 ? "var(--soxu-surface-raised)" : "var(--soxu-surface-card-muted)",
                border: `1px solid ${i === 2 ? "var(--soxu-semantic-danger-border)" : "var(--soxu-border-subtle)"}`,
                color: i === 2 ? "var(--soxu-semantic-danger)" : "var(--soxu-text-primary)",
              }}
            >
              {opt}
            </button>
          ))}
        </BottomSheet>
      )}

      {sheet === "pair" && (
        <BottomSheet title="扫码 / 配对码连接守护进程" subtitle="当前示例地址：127.0.0.1:7698" tall onClose={() => setSheet(null)}>
          <div
            class="rounded-xl p-3 text-xs"
            style={{ backgroundColor: "var(--soxu-surface-card-muted)", border: "1px solid var(--soxu-border-subtle)" }}
          >
            <div>已使用浏览器访问带 #offer= 的一次性二维码链接时，Soxu 会在自动握手后立刻抹除地址栏片段以防止截屏泄露。</div>
          </div>
          <label class="text-xs font-medium" htmlFor="pair-code-inp">
            手动输入 6 位一次性配对码：
          </label>
          <input
            id="pair-code-inp"
            type="text"
            inputMode="numeric"
            value={pairCode}
            onInput={(e) => setPairCode((e.target as HTMLInputElement).value)}
            placeholder="例如：849217"
            class="w-full rounded-xl px-3.5 py-2.5 soxu-mono"
            style={{
              fontSize: "var(--soxu-text-input-mobile)",
              backgroundColor: "var(--soxu-surface-composer)",
              border: "1px solid var(--soxu-border-strong)",
            }}
          />
          <button
            type="button"
            onClick={() => {
              setSheet(null);
              flashToast(`已通过配对码 ${pairCode || "849217"} 建立加密会话`);
            }}
            class="soxu-touch rounded-xl text-sm font-semibold"
            style={{
              minHeight: "46px",
              backgroundColor: "var(--soxu-action-inverse)",
              color: "var(--soxu-action-on-inverse)",
            }}
          >
            完成配对并同步状态
          </button>
        </BottomSheet>
      )}

      {sheet === "new-worktree" && (
        <BottomSheet title="新建隔离 Git Worktree" subtitle="自动按 branch-slug 归一化并注入直接 MCP 上下文" onClose={() => setSheet(null)}>
          <label class="text-xs font-medium" htmlFor="wks-branch-inp">
            新分支名称（基于 origin/main）：
          </label>
          <input
            id="wks-branch-inp"
            type="text"
            value={newBranchName}
            onInput={(e) => setNewBranchName((e.target as HTMLInputElement).value)}
            class="w-full rounded-xl px-3.5 py-2.5 soxu-mono"
            style={{
              fontSize: "var(--soxu-text-input-mobile)",
              backgroundColor: "var(--soxu-surface-composer)",
              border: "1px solid var(--soxu-border-strong)",
            }}
          />
          <button
            type="button"
            onClick={handleCreateWorktree}
            class="soxu-touch rounded-xl text-sm font-semibold"
            style={{
              minHeight: "46px",
              backgroundColor: "var(--soxu-action-inverse)",
              color: "var(--soxu-action-on-inverse)",
            }}
          >
            创建 Worktree 并切换进入
          </button>
        </BottomSheet>
      )}

      {sheet === "history" && (
        <BottomSheet title="导入本机 CLI 原生历史会话" subtitle="只读扫描，支持原位续接或分叉至新 Worktree" tall onClose={() => setSheet(null)}>
          {NATIVE_HISTORY.map((his) => (
            <button
              key={his.id}
              type="button"
              onClick={() => {
                setSheet(null);
                setActiveTab("session");
                flashToast(`已加载 ${his.cli} 原生历史：${his.title}`);
              }}
              class="soxu-touch rounded-xl p-3.5 text-left flex flex-col gap-1"
              style={{
                backgroundColor: "var(--soxu-surface-card-muted)",
                border: "1px solid var(--soxu-border-subtle)",
              }}
            >
              <div class="flex items-center justify-between text-xs">
                <span class="soxu-mono font-semibold">{his.cli}</span>
                <span style={{ color: "var(--soxu-text-tertiary)" }}>
                  {his.turns} 轮 · {his.updatedAt}
                </span>
              </div>
              <div class="text-sm font-medium">{his.title}</div>
            </button>
          ))}
        </BottomSheet>
      )}

      {sheet && typeof sheet === "object" && sheet.kind === "script-log" && (
        <BottomSheet title={`脚本日志 · ${sheet.name}`} subtitle="尾部实时输出预览" onClose={() => setSheet(null)}>
          <div
            class="rounded-xl p-3 soxu-mono text-xs flex flex-col gap-1 overflow-x-auto"
            style={{ backgroundColor: "var(--soxu-surface-terminal)", border: "1px solid var(--soxu-border-subtle)" }}
          >
            {(scripts.find((s) => s.name === sheet.name)?.logs ?? []).map((ln, idx) => (
              <div key={idx}>{ln}</div>
            ))}
          </div>
        </BottomSheet>
      )}
    </div>
  );
}

const root = document.getElementById("app");
if (root) {
  render(<App />, root);
}
