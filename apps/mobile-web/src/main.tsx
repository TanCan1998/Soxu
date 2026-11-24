import { render } from "preact";
import { useEffect, useState } from "preact/hooks";
import {
  IconArrowUp,
  IconBolt,
  IconBranch,
  IconChat,
  IconCheck,
  IconCheckpoint,
  IconChevron,
  IconLayers,
  IconShield,
  IconStop,
  IconSunMoon,
  IconTerminal,
} from "./icons.tsx";
import "./index.css";

type TurnBehavior = "queue" | "steer" | "interrupt";
type MobileTab = "session" | "workspaces" | "approvals" | "scripts";
type SizeClass = "compact" | "regular" | "wide";

interface WorkspaceItem {
  id: string;
  branch: string;
  isolation: "local" | "worktree";
  agent: string;
  model: string;
  bucket: "needs_input" | "running" | "attention" | "done";
  summary: string;
  diffStat: string;
}

interface ApprovalItem {
  id: string;
  workspaceId: string;
  branch: string;
  agent: string;
  toolKind: string;
  command: string;
  riskNote: string;
  highRisk: boolean;
  diffPreview: string[];
  resolution?: "approved" | "rejected";
  note?: string;
}

interface TurnMessage {
  id: string;
  role: "user" | "assistant";
  author: string;
  text: string;
  badge?: string;
  reasoningSec?: string;
  toolSummary?: string;
  checkpoint?: { id: string; sha: string; stat: string };
}

interface ScriptItem {
  name: string;
  port?: string;
  running: boolean;
  lastExit: number;
  cmd: string;
}

const INITIAL_WORKSPACES: WorkspaceItem[] = [
  {
    id: "wks-m2",
    branch: "feat/12-native-drivers",
    isolation: "worktree",
    agent: "codex",
    model: "group/gpt-6.2-sol",
    bucket: "needs_input",
    summary: "请求执行临时夹具清理测试命令",
    diffStat: "+142 -18",
  },
  {
    id: "wks-m1",
    branch: "feat/8-ws-cursor-stream",
    isolation: "worktree",
    agent: "pi",
    model: "group/claude-sonnet-5.5",
    bucket: "running",
    summary: "正在编写断线 seq 游标重连单测 (01:38)",
    diffStat: "+86 -9",
  },
  {
    id: "wks-main",
    branch: "main",
    isolation: "local",
    agent: "agy",
    model: "gemini-4-carbon",
    bucket: "done",
    summary: "M0.5 纯黑设计系统与 AOCI 索引已对齐",
    diffStat: "clean",
  },
];

const INITIAL_APPROVALS: ApprovalItem[] = [
  {
    id: "apr-104",
    workspaceId: "wks-m2",
    branch: "feat/12-native-drivers",
    agent: "codex · gpt-6.2-sol",
    toolKind: "PowerShell 命令授权",
    command: "cargo test -p soxu-drivers -- --nocapture",
    riskNote: "在共享 target 缓存目录编译并执行驱动层临时夹具测试，不触碰 ~/.codex 或 ~/.config/magpie。",
    highRisk: false,
    diffPreview: [
      "--- a/crates/soxu-drivers/src/lib.rs",
      "+++ b/crates/soxu-drivers/src/lib.rs",
      "@@ -24,3 +24,9 @@ impl NativeCliKind {",
      "+  pub fn supports_live_steer(self) -> bool {",
      "+    matches!(self, Self::Pi | Self::Codex | Self::Claude)",
      "+  }",
    ],
  },
];

const INITIAL_TURNS: TurnMessage[] = [
  {
    id: "t-1",
    role: "user",
    author: "你",
    text: "为 soxu-drivers 补齐原生 CLI 的流式热纠偏能力探测，并用 Fixture TempDir 完成单测，严禁读写本机真实用户配置目录。",
  },
  {
    id: "t-2",
    role: "assistant",
    author: "codex · group/gpt-6.2-sol",
    reasoningSec: "5.4s",
    toolSummary: "已读取 4 个文件 · 修改 crates/soxu-drivers/src/lib.rs (+28 -3)",
    text: "已按 ADR-002 与安全红线补充 `supports_live_steer` 能力矩阵：对不支持热注入的单次 `--print` 驱动自动声明安全中断续接回退。单测全部基于 `tempfile::TempDir` 构造沙箱目录，现请求运行包级测试验证。",
    checkpoint: { id: "#04", sha: "7c4e91a", stat: "2 files (+28 -3)" },
  },
];

const INITIAL_SCRIPTS: ScriptItem[] = [
  { name: "daemon-dev", port: ":7698", running: true, lastExit: 0, cmd: "cargo run -p soxu-daemon" },
  { name: "mobile-dev", port: ":5173", running: true, lastExit: 0, cmd: "pnpm --dir apps/mobile-web dev" },
  { name: "check-all", running: false, lastExit: 0, cmd: "pwsh -NoProfile -File scripts/check-all.ps1" },
];

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
  const [selectedWorkspaceId, setSelectedWorkspaceId] = useState<string>("wks-m2");
  const [workspaces, setWorkspaces] = useState<WorkspaceItem[]>(INITIAL_WORKSPACES);
  const [approvals, setApprovals] = useState<ApprovalItem[]>(INITIAL_APPROVALS);
  const [turns, setTurns] = useState<TurnMessage[]>(INITIAL_TURNS);
  const [scripts, setScripts] = useState<ScriptItem[]>(INITIAL_SCRIPTS);
  const [magpieMode, setMagpieMode] = useState<"magpie_managed" | "standalone_native">("magpie_managed");
  const [daemonOnline, setDaemonOnline] = useState<boolean>(false);
  const [seqCursor, setSeqCursor] = useState<number>(1056);
  const [behavior, setBehavior] = useState<TurnBehavior>("steer");
  const [queuedPrompt, setQueuedPrompt] = useState<string | null>(null);
  const [draft, setDraft] = useState<string>("");
  const [diffExpanded, setDiffExpanded] = useState<boolean>(false);
  const [rejectingId, setRejectingId] = useState<string | null>(null);
  const [rejectReason, setRejectReason] = useState<string>("");

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
    fetch("/api/health")
      .then((r) => (r.ok ? r.json() : null))
      .then((data) => {
        if (data && typeof data.magpie_mode === "string") {
          setDaemonOnline(true);
          setMagpieMode(data.magpie_mode === "standalone_native" ? "standalone_native" : "magpie_managed");
        }
      })
      .catch(() => {
        setDaemonOnline(false);
      });
  }, []);

  const pendingApprovals = approvals.filter((a) => !a.resolution);
  const activeWorkspace = workspaces.find((w) => w.id === selectedWorkspaceId) ?? workspaces[0];
  const isTurnBusy = activeWorkspace.bucket === "running" || activeWorkspace.bucket === "needs_input";

  function handleResolveApproval(id: string, resolution: "approved" | "rejected", note?: string) {
    setApprovals((prev) => prev.map((item) => (item.id === id ? { ...item, resolution, note } : item)));
    setWorkspaces((prev) =>
      prev.map((w) =>
        w.id === "wks-m2"
          ? {
              ...w,
              bucket: "running",
              summary: resolution === "approved" ? "已批准 · 正在执行驱动单测..." : "已附带修改指导 · 正在自修正...",
            }
          : w,
      ),
    );
    setSeqCursor((c) => c + 1);
    setRejectingId(null);
    setRejectReason("");
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
        ? "中途纠偏 · 下一步生效"
        : "已打断旧回合并重开轮次"
      : undefined;

    setTurns((prev) => [
      ...prev,
      {
        id: `t-${Date.now()}`,
        role: "user",
        author: "你 (移动遥控端)",
        text: trimmed,
        badge,
      },
    ]);
    setDraft("");
    setSeqCursor((c) => c + 1);
  }

  function promoteQueuedToSteer() {
    if (!queuedPrompt) return;
    setTurns((prev) => [
      ...prev,
      {
        id: `t-${Date.now()}`,
        role: "user",
        author: "你 (排队提级纠偏)",
        text: queuedPrompt,
        badge: "由排队提级为 Steer",
      },
    ]);
    setQueuedPrompt(null);
    setSeqCursor((c) => c + 1);
  }

  function toggleScript(name: string) {
    setScripts((prev) => prev.map((s) => (s.name === name ? { ...s, running: !s.running } : s)));
    setSeqCursor((c) => c + 1);
  }

  const quickChips = isTurnBusy
    ? ["先跑 check-all 别直接提交", "遇到删除操作先停下汇报", "完成后更新 PROGRESS.md"]
    : ["总结本轮改动与风险点", "运行全工作区单测", "生成 Git Emoji 提交预览"];

  const bucketMeta = {
    needs_input: { label: "待审批", color: "var(--soxu-semantic-warning)", bg: "var(--soxu-semantic-warning-soft)" },
    running: { label: "运行中", color: "var(--soxu-semantic-running)", bg: "var(--soxu-semantic-running-soft)" },
    attention: { label: "新完成", color: "var(--soxu-semantic-success)", bg: "var(--soxu-semantic-success-soft)" },
    done: { label: "空闲", color: "var(--soxu-text-tertiary)", bg: "var(--soxu-surface-control)" },
  } as const;

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
      <div class="flex items-center justify-between px-1">
        <span class="text-xs font-semibold tracking-wider uppercase" style={{ color: "var(--soxu-text-tertiary)" }}>
          Soxu 工作区 ({workspaces.length})
        </span>
        <button
          type="button"
          onClick={() => setMagpieMode((m) => (m === "magpie_managed" ? "standalone_native" : "magpie_managed"))}
          class="soxu-touch px-2.5 py-1 rounded-full text-xs font-medium flex items-center gap-1.5"
          style={{
            minHeight: "32px",
            backgroundColor:
              magpieMode === "magpie_managed" ? "var(--soxu-semantic-accent-soft)" : "var(--soxu-surface-control)",
            color: magpieMode === "magpie_managed" ? "var(--soxu-semantic-accent)" : "var(--soxu-text-secondary)",
            border: "1px solid var(--soxu-border-subtle)",
          }}
          title="点击模拟切换 Magpie 协同态与独立原生降级态"
        >
          <span
            class="inline-block w-2 h-2 rounded-full"
            style={{
              backgroundColor:
                magpieMode === "magpie_managed" ? "var(--soxu-semantic-accent)" : "var(--soxu-semantic-warning)",
            }}
          />
          {magpieMode === "magpie_managed" ? "Magpie 协同" : "独立原生"}
        </button>
      </div>

      <div class="flex flex-col gap-2">
        {workspaces.map((wks) => {
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
                <span class="flex items-center gap-1.5 font-medium text-sm truncate">
                  <IconBranch size={15} />
                  <span class="soxu-mono truncate">{wks.branch}</span>
                </span>
                <span
                  class="px-2 py-0.5 rounded-full text-[11px] font-medium shrink-0"
                  style={{ backgroundColor: b.bg, color: b.color }}
                >
                  {b.label}
                </span>
              </div>
              <p class="text-xs truncate m-0" style={{ color: "var(--soxu-text-secondary)" }}>
                {wks.summary}
              </p>
              <div
                class="flex items-center justify-between text-[11px] soxu-mono soxu-tabular"
                style={{ color: "var(--soxu-text-tertiary)" }}
              >
                <span>
                  {wks.agent} · {wks.model}
                </span>
                <span>{wks.diffStat}</span>
              </div>
            </button>
          );
        })}
      </div>
    </aside>
  );

  const ApprovalCardsSection = (
    <section class="flex flex-col gap-3" aria-label="权限审批队列">
      {approvals.map((item) => {
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
              <span class="flex items-center gap-2">
                <IconCheck size={15} />
                <span>
                  {item.resolution === "approved" ? "已批准执行一次" : `已拒绝并指导：${item.note || "请调整方案"}`} ·{" "}
                  <code class="soxu-mono">{item.command}</code>
                </span>
              </span>
              <span class="soxu-tabular shrink-0" style={{ color: "var(--soxu-text-tertiary)" }}>
                移动遥控端
              </span>
            </div>
          );
        }

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
              <div class="flex items-center gap-2">
                <span style={{ color: "var(--soxu-semantic-warning)" }}>
                  <IconShield size={18} />
                </span>
                <div>
                  <div class="text-sm font-semibold">{item.toolKind}</div>
                  <div class="text-xs soxu-mono" style={{ color: "var(--soxu-text-tertiary)" }}>
                    {item.branch} · {item.agent}
                  </div>
                </div>
              </div>
              <span
                class="px-2 py-0.5 rounded-md text-[11px] font-medium"
                style={{
                  backgroundColor: "var(--soxu-semantic-warning-soft)",
                  color: "var(--soxu-semantic-warning)",
                }}
              >
                待确认
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

            <p class="text-xs m-0 leading-relaxed" style={{ color: "var(--soxu-text-secondary)" }}>
              {item.riskNote}
            </p>

            <button
              type="button"
              onClick={() => setDiffExpanded((v) => !v)}
              class="soxu-touch self-start px-2.5 py-1 rounded-lg text-xs flex items-center gap-1.5"
              style={{
                minHeight: "34px",
                backgroundColor: "var(--soxu-surface-card-muted)",
                color: "var(--soxu-text-secondary)",
                border: "1px solid var(--soxu-border-subtle)",
              }}
              aria-expanded={diffExpanded}
            >
              <IconChevron open={diffExpanded} />
              <span>{diffExpanded ? "收起本轮关联代码 Diff" : "展开查看关联代码 Diff (6 行)"}</span>
            </button>

            {diffExpanded && (
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

            {rejectingId === item.id ? (
              <div class="flex flex-col gap-2 pt-1">
                <label class="text-xs font-medium" htmlFor="reject-note-input">
                  向 Agent 附加拒绝与修改指导：
                </label>
                <input
                  id="reject-note-input"
                  type="text"
                  value={rejectReason}
                  onInput={(e) => setRejectReason((e.target as HTMLInputElement).value)}
                  placeholder="例如：仅运行单元测试，跳过慢速集成测试"
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
                    style={{
                      minHeight: "46px",
                      backgroundColor: "var(--soxu-surface-control)",
                      color: "var(--soxu-text-primary)",
                    }}
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
                  拒绝 / 留言
                </button>
                <button
                  type="button"
                  onClick={() => handleResolveApproval(item.id, "approved")}
                  class="soxu-touch rounded-xl text-sm font-semibold flex items-center justify-center gap-1.5"
                  style={{
                    minHeight: "48px",
                    backgroundColor: "var(--soxu-action-inverse)",
                    color: "var(--soxu-action-on-inverse)",
                  }}
                >
                  <IconCheck size={17} />
                  <span>允许一次</span>
                </button>
              </div>
            )}
          </article>
        );
      })}
    </section>
  );

  const ScriptsAndSystemView = (
    <section class="flex flex-col gap-4 py-2">
      <div
        class="rounded-2xl p-4 flex flex-col gap-2.5"
        style={{
          backgroundColor: "var(--soxu-surface-card)",
          border: "1px solid var(--soxu-border-subtle)",
        }}
      >
        <div class="flex items-center justify-between">
          <span class="text-sm font-semibold">环境与路由拓扑诊断</span>
          <span
            class="px-2.5 py-0.5 rounded-full text-xs font-medium"
            style={{
              backgroundColor:
                magpieMode === "magpie_managed" ? "var(--soxu-semantic-accent-soft)" : "var(--soxu-semantic-warning-soft)",
              color: magpieMode === "magpie_managed" ? "var(--soxu-semantic-accent)" : "var(--soxu-semantic-warning)",
            }}
          >
            {magpieMode === "magpie_managed" ? "MagpieManaged 协同" : "StandaloneNative 独立"}
          </span>
        </div>
        <p class="text-xs m-0 leading-relaxed" style={{ color: "var(--soxu-text-secondary)" }}>
          {magpieMode === "magpie_managed"
            ? "已只读识别 ~/.config/magpie：模型经本地网关 127.0.0.1:3425/v1 路由，新建 Worktree 自动为 direct MCP (aoci / codegraph) 注入工作树上下文，零全局配置覆写。"
            : "未检测到 Magpie：已自动降级为原生 CLI 目录只读扫描，claude / codex / pi / agy 等本地驱动独立可用。"}
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
            style={{
              backgroundColor: "var(--soxu-surface-card)",
              border: "1px solid var(--soxu-border-subtle)",
            }}
          >
            <div class="min-w-0">
              <div class="flex items-center gap-2">
                <span
                  class="inline-block w-2 h-2 rounded-full"
                  style={{
                    backgroundColor: scr.running ? "var(--soxu-semantic-success)" : "var(--soxu-text-tertiary)",
                  }}
                />
                <span class="font-medium text-sm soxu-mono">{scr.name}</span>
                {scr.port && (
                  <span
                    class="px-1.5 py-0.5 rounded text-[11px] soxu-mono"
                    style={{
                      backgroundColor: "var(--soxu-surface-control)",
                      color: "var(--soxu-text-secondary)",
                    }}
                  >
                    {scr.port}
                  </span>
                )}
              </div>
              <div class="text-xs soxu-mono truncate mt-1" style={{ color: "var(--soxu-text-tertiary)" }}>
                {scr.cmd}
              </div>
            </div>
            <button
              type="button"
              onClick={() => toggleScript(scr.name)}
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
    </section>
  );

  return (
    <div class="min-h-dvh flex flex-col" style={{ backgroundColor: "var(--soxu-surface-canvas)" }}>
      {/* 顶部浮动毛玻璃状态栏 */}
      <header
        class="sticky top-0 z-20 soxu-safe-top px-4 pb-2.5 backdrop-blur-md"
        style={{
          backgroundColor: "var(--soxu-surface-header-glass)",
          borderBottom: "1px solid var(--soxu-border-hairline)",
        }}
      >
        <div class="max-w-[1280px] mx-auto flex items-center justify-between gap-2">
          <div class="flex items-center gap-2.5 min-w-0">
            <span
              class="w-2.5 h-2.5 rounded-full shrink-0 soxu-live-dot"
              style={{ backgroundColor: "var(--soxu-semantic-accent)" }}
            />
            <div class="min-w-0">
              <div class="flex items-center gap-2">
                <span class="font-semibold text-sm truncate">Soxu Remote</span>
                <span
                  class="text-[11px] soxu-mono soxu-tabular px-1.5 py-0.5 rounded"
                  style={{
                    backgroundColor: "var(--soxu-surface-card)",
                    color: "var(--soxu-text-tertiary)",
                    border: "1px solid var(--soxu-border-subtle)",
                  }}
                >
                  {daemonOnline ? "Daemon 在线" : "M0.5 原型"} · seq #{seqCursor}
                </span>
              </div>
              <div class="text-xs soxu-mono truncate" style={{ color: "var(--soxu-text-secondary)" }}>
                {activeWorkspace.branch} · {activeWorkspace.agent}
              </div>
            </div>
          </div>

          <button
            type="button"
            onClick={() => setTheme((t) => (t === "dark" ? "light" : "dark"))}
            class="soxu-touch rounded-full flex items-center justify-center px-3 gap-1.5 text-xs font-medium"
            style={{
              backgroundColor: "var(--soxu-surface-card)",
              border: "1px solid var(--soxu-border-subtle)",
              color: "var(--soxu-text-secondary)",
            }}
            aria-label={theme === "dark" ? "切换为日光浅色主题" : "切换为 ChatGPT 纯黑主题"}
          >
            <IconSunMoon size={15} />
            <span>{theme === "dark" ? "Pure Black" : "Light"}</span>
          </button>
        </div>
      </header>

      {/* 主体自适应工作区：wide (>=1024px) 双栏并排；compact/regular 单栏居中限宽 */}
      <div class="flex-1 flex max-w-[1280px] w-full mx-auto">
        {sizeClass === "wide" && WorkspaceRail}

        <main class="flex-1 flex flex-col justify-between min-w-0">
          <div
            class="w-full mx-auto px-4 py-4 flex flex-col gap-4"
            style={{ maxWidth: "var(--soxu-layout-transcript-max-width)" }}
          >
            {/* 置顶高优待审批横幅（非审批专页且有待批项时提示） */}
            {pendingApprovals.length > 0 && activeTab !== "approvals" && activeTab !== "session" && (
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
                  <span>有 {pendingApprovals.length} 项权限请求等待确认</span>
                </span>
                <span class="text-xs underline">立即处理</span>
              </button>
            )}

            {sizeClass !== "wide" && activeTab === "workspaces" && WorkspaceRail}
            {activeTab === "approvals" && ApprovalCardsSection}
            {activeTab === "scripts" && ScriptsAndSystemView}

            {(activeTab === "session" || (sizeClass === "wide" && activeTab === "workspaces")) && (
              <div class="flex flex-col gap-5">
                {turns.map((turn) =>
                  turn.role === "user" ? (
                    <div key={turn.id} class="flex flex-col items-end gap-1">
                      {turn.badge && (
                        <span
                          class="text-[11px] px-2 py-0.5 rounded-full font-medium flex items-center gap-1"
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
                        style={{
                          backgroundColor: "var(--soxu-surface-raised)",
                          color: "var(--soxu-text-primary)",
                        }}
                      >
                        {turn.text}
                      </div>
                    </div>
                  ) : (
                    <div key={turn.id} class="flex flex-col gap-2.5">
                      <div class="flex items-center justify-between text-xs">
                        <span class="font-semibold">{turn.author}</span>
                        {turn.reasoningSec && (
                          <span class="soxu-tabular" style={{ color: "var(--soxu-text-tertiary)" }}>
                            已深度思考 {turn.reasoningSec}
                          </span>
                        )}
                      </div>

                      {turn.toolSummary && (
                        <div
                          class="rounded-xl px-3 py-2 text-xs soxu-mono flex items-center gap-2"
                          style={{
                            backgroundColor: "var(--soxu-surface-card)",
                            border: "1px solid var(--soxu-border-subtle)",
                            color: "var(--soxu-text-secondary)",
                          }}
                        >
                          <IconTerminal size={14} />
                          <span class="truncate">{turn.toolSummary}</span>
                        </div>
                      )}

                      <p class="m-0 text-sm leading-relaxed" style={{ color: "var(--soxu-text-primary)" }}>
                        {turn.text}
                      </p>

                      {turn.checkpoint && (
                        <div class="flex items-center gap-2 pt-1">
                          <div class="h-px flex-1" style={{ backgroundColor: "var(--soxu-border-hairline)" }} />
                          <span
                            class="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-[11px] soxu-mono"
                            style={{
                              backgroundColor: "var(--soxu-semantic-handoff-soft)",
                              color: "var(--soxu-semantic-handoff)",
                              border: "1px solid var(--soxu-border-subtle)",
                            }}
                          >
                            <IconCheckpoint size={13} />
                            <span>
                              检查点 {turn.checkpoint.id} · {turn.checkpoint.sha} · {turn.checkpoint.stat}
                            </span>
                          </span>
                          <div class="h-px flex-1" style={{ backgroundColor: "var(--soxu-border-hairline)" }} />
                        </div>
                      )}
                    </div>
                  ),
                )}

                {/* 会话内联待审批卡片 */}
                {ApprovalCardsSection}
              </div>
            )}
          </div>

          {/* 底部悬浮三态遥控输入舱 (Composer) */}
          <div class="sticky bottom-0 z-10 px-3 pt-2 soxu-safe-bottom" style={{ backgroundColor: "var(--soxu-surface-canvas)" }}>
            <div class="w-full mx-auto flex flex-col gap-2" style={{ maxWidth: "var(--soxu-layout-composer-max-width)" }}>
              {/* 场景化快捷指令片 */}
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

              {/* 待发排队条 (Queue Strip) */}
              {queuedPrompt && (
                <div
                  class="rounded-xl px-3 py-2 flex items-center justify-between gap-2 text-xs"
                  style={{
                    backgroundColor: "var(--soxu-surface-card)",
                    border: "1px solid var(--soxu-semantic-warning-border)",
                  }}
                >
                  <span class="truncate" style={{ color: "var(--soxu-text-secondary)" }}>
                    排队等待本轮结束：{queuedPrompt}
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
                      aria-label="取消排队消息"
                    >
                      撤回
                    </button>
                  </div>
                </div>
              )}

              {/* ChatGPT Pure Black #212121 大圆角三态输入舱 */}
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
                <label class="sr-only" htmlFor="soxu-remote-composer">
                  向当前工作区 Agent 发送指令
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
                        ? "输入中途纠偏指令（将在下一步立即注入）..."
                        : behavior === "queue"
                          ? "输入排队指令（本轮结束后自动发送）..."
                          : "输入新指令（将立即打断当前回合）..."
                      : "向当前工作区发送指令，或键入 @ 跨 Agent 移交..."
                  }
                  class="w-full resize-none bg-transparent border-0 px-2 py-1 focus:outline-none"
                  style={{
                    fontSize: "var(--soxu-text-input-mobile)",
                    color: "var(--soxu-text-primary)",
                  }}
                />

                <div class="flex items-center justify-between gap-2 px-1">
                  {/* 运行中三态投递策略切换胶囊 */}
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

                  {/* 主发送 / 停止圆形行动键 */}
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
                        backgroundColor: draft.trim()
                          ? "var(--soxu-action-inverse)"
                          : "var(--soxu-action-disabled-bg)",
                        color: draft.trim() ? "var(--soxu-action-on-inverse)" : "var(--soxu-action-disabled-fg)",
                      }}
                      aria-label="发送指令"
                    >
                      <IconArrowUp size={18} />
                    </button>
                  )}
                </div>
              </div>

              {/* 窄屏/常规屏底部 4 栏主导航 (<=4 项，符合 Bottom Nav 规范) */}
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
                      { id: "scripts", label: "脚本/路由", icon: <IconTerminal size={18} /> },
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
    </div>
  );
}

const root = document.getElementById("app");
if (root) {
  render(<App />, root);
}
