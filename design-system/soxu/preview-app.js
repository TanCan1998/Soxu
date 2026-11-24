"use strict";

/* ── SVG Vector Icon Factory (Zero Emoji Policy) ───────────────────────── */
function icon(name, size = 15) {
  const s = `width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"`;
  const map = {
    branch: `<svg ${s}><line x1="6" y1="3" x2="6" y2="15"/><circle cx="18" cy="6" r="3"/><circle cx="6" cy="18" r="3"/><path d="M18 9a9 9 0 0 1-9 9"/></svg>`,
    folder: `<svg ${s}><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/></svg>`,
    search: `<svg ${s}><circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/></svg>`,
    shield: `<svg ${s}><path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/><path d="M12 8v4"/><path d="M12 16h.01"/></svg>`,
    bolt: `<svg ${s}><polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2"/></svg>`,
    arrowUp: `<svg ${s}><line x1="12" y1="19" x2="12" y2="5"/><polyline points="5 12 12 5 19 12"/></svg>`,
    stop: `<svg width="${size}" height="${size}" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><rect x="6" y="6" width="12" height="12" rx="2.5"/></svg>`,
    check: `<svg ${s}><polyline points="20 6 9 17 4 12"/></svg>`,
    x: `<svg ${s}><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>`,
    plus: `<svg ${s}><line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/></svg>`,
    terminal: `<svg ${s}><polyline points="4 17 10 11 4 5"/><line x1="12" y1="19" x2="20" y2="19"/></svg>`,
    file: `<svg ${s}><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/></svg>`,
    cpu: `<svg ${s}><rect x="4" y="4" width="16" height="16" rx="2"/><rect x="9" y="9" width="6" height="6"/><line x1="9" y1="1" x2="9" y2="4"/><line x1="15" y1="1" x2="15" y2="4"/><line x1="9" y1="20" x2="9" y2="23"/><line x1="15" y1="20" x2="15" y2="23"/></svg>`,
    phone: `<svg ${s}><rect x="5" y="2" width="14" height="20" rx="2"/><line x1="12" y1="18" x2="12.01" y2="18"/></svg>`,
    clock: `<svg ${s}><circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/></svg>`,
    layers: `<svg ${s}><polygon points="12 2 2 7 12 12 22 7 12 2"/><polyline points="2 17 12 22 22 17"/><polyline points="2 12 12 17 22 12"/></svg>`,
    checkpoint: `<svg ${s}><circle cx="12" cy="12" r="3"/><line x1="3" y1="12" x2="9" y2="12"/><line x1="15" y1="12" x2="21" y2="12"/></svg>`,
    rewind: `<svg ${s}><polyline points="1 4 1 10 7 10"/><path d="M3.51 15a9 9 0 1 0 2.13-9.36L1 10"/></svg>`,
    sunMoon: `<svg ${s}><path d="M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z"/></svg>`,
    sidebarL: `<svg ${s}><rect x="3" y="3" width="18" height="18" rx="2"/><line x1="9" y1="3" x2="9" y2="21"/></svg>`,
    sidebarR: `<svg ${s}><rect x="3" y="3" width="18" height="18" rx="2"/><line x1="15" y1="3" x2="15" y2="21"/></svg>`,
    chevron: `<svg ${s}><polyline points="9 18 15 12 9 6"/></svg>`,
    chevronDown: `<svg ${s}><polyline points="6 9 12 15 18 9"/></svg>`,
    detach: `<svg ${s}><path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/><polyline points="15 3 21 3 21 9"/><line x1="10" y1="14" x2="21" y2="3"/></svg>`,
    lock: `<svg ${s}><rect x="3" y="11" width="18" height="11" rx="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/></svg>`,
    gitCommit: `<svg ${s}><circle cx="12" cy="12" r="3"/><line x1="12" y1="3" x2="12" y2="9"/><line x1="12" y1="15" x2="12" y2="21"/></svg>`,
    sparkle: `<svg ${s}><path d="M12 3l1.8 5.4L19 10l-5.2 1.6L12 17l-1.8-5.4L5 10l5.2-1.6z"/></svg>`,
  };
  return map[name] || map.cpu;
}

/* ── Application State ─────────────────────────────────────────────────── */
const state = {
  theme: "dark",
  view: "workbench", // workbench | magpie | schedules | mobile | tokens
  leftOpen: true,
  rightOpen: true,
  rightTab: "diff", // diff | files | scripts | terminal
  diffLayout: "unified", // unified | split
  diffFilter: "",
  selectedFile: "crates/soxu-drivers/src/lib.rs",
  magpieMode: "magpie_managed", // magpie_managed | standalone_native
  selectedWorkspace: "wks-drivers",
  activeTabId: "tab-codex-main",
  composerMode: "steer", // queue | steer | interrupt
  cli: "codex",
  model: "group/gpt-6.2-sol",
  thinking: "medium",
  permLevel: "ask",
  fastMode: true,
  composerText: "",
  queuedPrompt: "测试跑通后请顺便刷新 docs/ops/PROGRESS.md 与 AOCI 认知索引",
  composerPopover: null, // null | 'handoff' | 'cli' | 'model' | 'thinking' | 'perm'
  lostControlToast: null,
  activeModal: null, // null | 'cmdk' | 'pair' | 'rewind' | 'commit' | 'onboarding'
  onboardingStep: 1,
  cmdkQuery: "",
  activeRailTurn: 2,
  mobileViewport: "393", // 344 | 393 | 820
  reasoningOpen: { "t-2": false },
  toolsOpen: { "tool-read": false, "tool-edit": true, "tool-bash": true, "tool-mcp": false },
  approvals: {
    "apr-safe": { status: "pending", note: "" },
    "apr-danger": { status: "pending", note: "", rejecting: false },
  },
  askQuestion: {
    selected: "opt-1",
    custom: "",
    submitted: false,
  },
  tabs: [
    { id: "tab-codex-main", cli: "codex", title: "M2 驱动流式热纠偏", status: "needs_input", draft: false },
    { id: "tab-pi-protocol", cli: "pi", title: "seq 游标断线追平单测", status: "running", draft: false },
    { id: "tab-agy-draft", cli: "agy", title: "新建草稿会话", status: "done", draft: true },
  ],
  subagents: [
    { id: "sub-research", cli: "pi", role: "索引与参考巡检", status: "done", duration: "12s", detached: false },
    { id: "sub-test", cli: "codex", role: "TempDir 夹具单测", status: "running", duration: "00:18", detached: false },
  ],
  scripts: [
    { name: "daemon-dev", port: ":7698", running: true, cmd: "cargo run -p soxu-daemon" },
    { name: "mobile-dev", port: ":5173", running: true, cmd: "pnpm --dir apps/mobile-web dev" },
    { name: "check-all", port: "", running: false, cmd: "pwsh -NoProfile -File scripts/check-all.ps1" },
  ],
  schedules: [
    { id: "sch-1", name: "每日凌晨全仓巡检", kind: "Schedule", cron: "0 2 * * *", human: "每天 02:00 (Asia/Shanghai)", target: "pi · deepseek-v4.1-flash", isolation: "worktree", active: true, lastStatus: "ok", unreadFail: false },
    { id: "sch-2", name: "PR #4 CI 与门禁守候", kind: "Heartbeat", cron: "*/15 * * * *", human: "每 15 分钟唤醒一次", target: "当前主控会话", isolation: "self", active: true, lastStatus: "ok", unreadFail: false },
    { id: "sch-3", name: "周末 AOCI 索引漂移检查", kind: "Schedule", cron: "0 10 * * 0", human: "每周日 10:00", target: "agy · gemini-4-hydrogen", isolation: "worktree", active: false, lastStatus: "failed", unreadFail: true },
  ],
  addedTurns: [],
  toast: null,
};

const WORKSPACES = [
  { id: "wks-drivers", project: "Soxu", branch: "feat/12-native-drivers", isolation: "worktree", bucket: "needs_input", bucketLabel: "待批 2", stat: "+142 -18", label: "M2 驱动" },
  { id: "wks-ws", project: "Soxu", branch: "feat/8-ws-cursor-stream", isolation: "worktree", bucket: "running", bucketLabel: "01:38", stat: "+86 -9", label: "M1 协议" },
  { id: "wks-git", project: "Soxu", branch: "feat/15-checkpoint-gc", isolation: "worktree", bucket: "failed", bucketLabel: "失败", stat: "+24 -3", label: "" },
  { id: "wks-ui", project: "Soxu", branch: "ui/3-m0.5-pure-black-design-system", isolation: "local", bucket: "attention", bucketLabel: "新完成", stat: "+2569", label: "设计审阅" },
  { id: "wks-novel", project: "ai-novel-writer", branch: "main", isolation: "local", bucket: "done", bucketLabel: "空闲", stat: "clean", label: "" },
];

const CLIS = [
  { id: "claude", bin: "claude", ver: "2.1.4", path: "~/.local/bin/claude", ok: true, caps: ["Stream-JSON", "Session Resume", "Native History"] },
  { id: "codex", bin: "codex", ver: "0.49.0", path: "~/scoop/shims/codex", ok: true, caps: ["JSONL Events", "fast_mode", "Worktree Ready"] },
  { id: "pi", bin: "pi", ver: "0.9.8", path: "~/npm-global/pi", ok: true, caps: ["RPC Stream", "Live Steer", "Extensions"] },
  { id: "opencode", bin: "opencode", ver: "1.3.2", path: "~/.opencode/bin/opencode", ok: true, caps: ["Non-Interactive", "PTY Fallback"] },
  { id: "agy", bin: "agy", ver: "1.107.0", path: "~/scoop/shims/agy", ok: true, caps: ["Stream-JSON", "--effort", "Conversation ID"] },
  { id: "copilot", bin: "copilot", ver: "1.12.0", path: "~/.gh/copilot", ok: true, caps: ["CLI / PTY"] },
  { id: "qwen", bin: "qwen", ver: "未安装", path: "PATH 未命中", ok: false, caps: ["Stream / PTY"] },
];

const MODELS = [
  { id: "group/gpt-6.2-astra", tier: "L3 架构攻坚", star: true, thinking: ["medium", "high", "xhigh"], note: "Magpie 路由" },
  { id: "group/claude-opus-5.5", tier: "L3 架构攻坚", star: true, thinking: ["medium", "high", "max"], note: "Magpie 路由" },
  { id: "gemini-4-carbon", tier: "L3 架构攻坚", star: true, thinking: ["high", "max"], note: "仅 agy 原生直连" },
  { id: "group/gpt-6.2-sol", tier: "L2 主力实现", star: true, thinking: ["low", "medium", "high"], note: "Magpie 路由" },
  { id: "group/claude-sonnet-5.5", tier: "L2 主力实现", star: false, thinking: ["low", "medium", "high"], note: "Magpie 路由" },
  { id: "auto/deepseek-v4.1-flash-1017", tier: "L1 极速巡检", star: false, thinking: ["low", "medium"], note: "Magpie 路由" },
  { id: "auto/gemini-4-hydrogen", tier: "L1 极速巡检", star: false, thinking: ["low", "medium"], note: "Magpie 路由" },
];

const DIFF_ROWS = [
  { oldN: "22", newN: "22", kind: "ctx", html: `<span class="syn-kw">pub enum</span> <span class="syn-ty">NativeCliKind</span> { Claude, Codex, Pi, OpenCode, Antigravity, Copilot, Qwen }` },
  { oldN: "23", newN: "", kind: "del", html: `- <span class="syn-com">// 仅在运行时返回可执行文件名</span>` },
  { oldN: "", newN: "23", kind: "add", html: `+ <span class="syn-com">/// 返回驱动是否支持活跃回合中途热纠偏 (Steer)</span>` },
  { oldN: "", newN: "24", kind: "add", html: `+ <span class="syn-mac">#[inline]</span>` },
  { oldN: "", newN: "25", kind: "add", html: `+ <span class="syn-kw">pub const fn</span> <span class="syn-fn">supports_live_steer</span>(<span class="syn-kw">self</span>) -&gt; <span class="syn-ty">bool</span> {` },
  { oldN: "", newN: "26", kind: "add", html: `+   <span class="syn-mac">matches!</span>(<span class="syn-kw">self</span>, <span class="syn-ty">Self</span>::Pi | <span class="syn-ty">Self</span>::Codex | <span class="syn-ty">Self</span>::Claude)` },
  { oldN: "", newN: "27", kind: "add", html: `+ }` },
];

const RAIL_TURNS = [
  { idx: 1, title: "Turn #1 · 为 soxu-drivers 补齐热纠偏与 TempDir 单测", kind: "normal" },
  { idx: 2, title: "Turn #2 · codex 修改 lib.rs (+28 -3) · 检查点 #04", kind: "handoff" },
  { idx: 3, title: "Turn #3 · 待审批 2 项 · 结构化提问 1 项", kind: "warning" },
];

function esc(s) {
  return String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}

function showToast(msg) {
  state.toast = msg;
  render();
  setTimeout(() => {
    if (state.toast === msg) {
      state.toast = null;
      render();
    }
  }, 3200);
}

/* ── View 1: GPUI Desktop Workbench ────────────────────────────────────── */
function renderLeftSidebar() {
  if (!state.leftOpen) return "";
  const groups = { Soxu: [], "ai-novel-writer": [] };
  WORKSPACES.forEach((w) => groups[w.project].push(w));

  const badgeCls = {
    needs_input: "warn",
    running: "run",
    failed: "danger",
    attention: "accent",
    done: "neutral",
  };

  return `
  <aside class="pane-left" aria-label="项目与工作区栏">
    <div class="sidebar-scroll">
      ${Object.entries(groups)
        .map(
          ([proj, items]) => `
        <div>
          <div class="sec-hdr">
            <span>${esc(proj)} (${items.length})</span>
            <button class="icon-btn" style="height:20px;padding:0 5px" data-action="toast-worktree" title="新建隔离 Worktree">${icon("plus", 13)}</button>
          </div>
          <div style="display:flex;flex-direction:column;gap:2px">
            ${items
              .map(
                (w) => `
              <button class="wks-row ${state.selectedWorkspace === w.id ? "selected" : ""}" data-action="select-wks" data-id="${w.id}">
                <div class="wks-meta">
                  <div class="wks-name-line">
                    ${icon(w.isolation === "worktree" ? "branch" : "folder", 14)}
                    <span class="wks-branch mono">${esc(w.branch)}</span>
                  </div>
                  <div class="wks-subline tabular">
                    <span>${esc(w.stat)}</span>
                    ${w.label ? `<span class="label-chip">${esc(w.label)}</span>` : ""}
                  </div>
                </div>
                <span class="badge ${badgeCls[w.bucket]} tabular ${w.bucket === "running" ? "live-pulse" : ""}">${esc(w.bucketLabel)}</span>
              </button>`,
              )
              .join("")}
          </div>
        </div>`,
        )
        .join("")}
      <div>
        <div class="sec-hdr"><span>后台自动化工作区 (2)</span></div>
        <button class="wks-row" data-action="go-view" data-view="schedules">
          <span class="wks-name-line">${icon("clock", 14)}<span>Schedules &amp; Heartbeats 调度中心</span></span>
          <span class="badge danger">1</span>
        </button>
      </div>
    </div>
    <div class="sidebar-footer">
      <button class="wks-row" data-action="go-view" data-view="magpie">
        <span class="wks-name-line">${icon("cpu", 14)}<span>导入本机 CLI 历史会话</span></span>
        <span class="badge neutral">14</span>
      </button>
      <button class="wks-row" data-action="open-modal" data-modal="onboarding">
        <span class="wks-name-line">${icon("sparkle", 14)}<span>首次 3 步引导向导</span></span>
        ${icon("chevron", 13)}
      </button>
    </div>
  </aside>`;
}

function renderDiffBox() {
  return `
  <div class="code-box">
    ${DIFF_ROWS.map(
      (r) => `
      <div class="diff-line ${r.kind}">
        <span class="diff-gutter tabular">${esc(r.oldN)} ${esc(r.newN)}</span>
        <span class="diff-code">${r.html}</span>
      </div>`,
    ).join("")}
  </div>`;
}

function renderTranscript() {
  const aprSafe = state.approvals["apr-safe"];
  const aprDanger = state.approvals["apr-danger"];
  return `
  <div class="transcript-wrap">
    <div class="transcript-scroll" id="transcript-scroll">
      <div class="transcript-col">
        <!-- User Turn 1 -->
        <div class="user-turn" id="turn-1">
          <div class="user-bubble">
            为 <code>soxu-drivers</code> 补齐原生 CLI 的流式热纠偏能力探测，并用 <code>TempDir</code> 夹具完成单测，严禁触碰真实本机全局配置目录。
          </div>
        </div>

        <!-- Assistant Turn 2 with Reasoning, 4 Tool Cards & Checkpoint -->
        <div class="assistant-turn" id="turn-2">
          <div class="assistant-hdr">
            <span class="assistant-id">${icon("cpu", 15)} <span>codex</span> <span style="color:var(--soxu-text-tertiary);font-weight:400" class="mono">group/gpt-6.2-sol · high</span></span>
            <button class="icon-btn" data-action="open-modal" data-modal="commit" title="生成 Emoji 提交预览">${icon("gitCommit", 13)}<span>提交此轮</span></button>
          </div>

          <button class="reasoning-toggle" data-action="toggle-reason" data-id="t-2">
            ${icon(state.reasoningOpen["t-2"] ? "chevronDown" : "chevron", 14)}
            <span>已深度思考 6.4 秒 · 查看推演步骤</span>
          </button>
          ${
            state.reasoningOpen["t-2"]
              ? `<div class="reasoning-body">1. 核对 ADR-002：Pi / Codex / Claude 原生支持运行中热注入；OpenCode / Agy 默认按安全中断带上下文续接。<br/>2. 遵循安全红线，测试目录统一通过 tempfile::TempDir 构建。</div>`
              : ""
          }

          <!-- Tool Card 1: Read Group -->
          <div class="tool-card ${state.toolsOpen["tool-read"] ? "open" : ""}">
            <button class="tool-card-hdr" data-action="toggle-tool" data-id="tool-read">
              <span style="display:flex;align-items:center;gap:7px">${icon("search", 14)} <span>已检索并读取 6 个文件 (ACTIVITY_PREVIEW_LIMIT = 4 · 0.4s)</span></span>
              ${icon(state.toolsOpen["tool-read"] ? "chevronDown" : "chevron", 14)}
            </button>
            ${
              state.toolsOpen["tool-read"]
                ? `<div class="tool-card-body mono" style="color:var(--soxu-text-secondary)">crates/soxu-drivers/src/lib.rs · crates/soxu-protocol/src/lib.rs · crates/soxu-magpie/src/lib.rs · AGENTS.md <span style="color:var(--soxu-text-tertiary)">(+2 项已折叠)</span></div>`
                : ""
            }
          </div>

          <!-- Tool Card 2: File Edit Inline Diff -->
          <div class="tool-card ${state.toolsOpen["tool-edit"] ? "open" : ""}">
            <button class="tool-card-hdr" data-action="toggle-tool" data-id="tool-edit">
              <span style="display:flex;align-items:center;gap:7px">${icon("file", 14)} <span class="mono">修改 crates/soxu-drivers/src/lib.rs (+28 -3)</span></span>
              <span class="badge accent">已应用</span>
            </button>
            ${state.toolsOpen["tool-edit"] ? `<div class="tool-card-body" style="padding:8px">${renderDiffBox()}</div>` : ""}
          </div>

          <!-- Tool Card 3: Terminal ANSI Output -->
          <div class="tool-card ${state.toolsOpen["tool-bash"] ? "open" : ""}">
            <button class="tool-card-hdr" data-action="toggle-tool" data-id="tool-bash">
              <span style="display:flex;align-items:center;gap:7px">${icon("terminal", 14)} <span class="mono">cargo test -p soxu-drivers · 退出码 0 · 1.8s</span></span>
              <span class="badge accent">✓ 6 passed</span>
            </button>
            ${
              state.toolsOpen["tool-bash"]
                ? `<div class="tool-card-body"><div class="term-box"><span style="color:var(--soxu-ansi-bright-black)">$ cargo test -p soxu-drivers</span>\n<span style="color:var(--soxu-ansi-green)">   Compiling</span> soxu-drivers v0.1.0\n<span style="color:var(--soxu-ansi-cyan)">test</span> tests::steer_capability_matrix ... <span style="color:var(--soxu-ansi-bright-green)">ok</span>\n<span style="color:var(--soxu-ansi-bright-green)">test result: ok. 6 passed; 0 failed</span></div></div>`
                : ""
            }
          </div>

          <!-- Tool Card 4: MCP Tool Card -->
          <div class="tool-card ${state.toolsOpen["tool-mcp"] ? "open" : ""}">
            <button class="tool-card-hdr" data-action="toggle-tool" data-id="tool-mcp">
              <span style="display:flex;align-items:center;gap:7px">${icon("layers", 14)} <span class="mono">aoci · aoci_maintain [Direct 工作区直连] · ✓ 0.3s</span></span>
              <span class="badge neutral">五净对齐</span>
            </button>
            ${
              state.toolsOpen["tool-mcp"]
                ? `<div class="tool-card-body mono">route: Direct Worktree | composite_identity: 921fdc04 | remaining: 0</div>`
                : ""
            }
          </div>

          <!-- Checkpoint Strip -->
          <div class="checkpoint-strip">
            <div class="checkpoint-line"></div>
            <div class="checkpoint-pill mono">
              <span class="cp-accent">${icon("checkpoint", 13)} 检查点 #04 · 7c4e91a · 2 files (+28 -3)</span>
              <button class="icon-btn" style="height:20px;padding:0 6px" data-action="open-modal" data-modal="rewind">${icon("rewind", 12)}<span>回退至此轮</span></button>
            </div>
            <div class="checkpoint-line"></div>
          </div>
        </div>

        <!-- Turn 3: PendingUserInput + Standard Approval + High-Risk Redline Approval -->
        <div class="assistant-turn" id="turn-3">
          <!-- PendingUserInput Structured Ask Card -->
          <div class="perm-card ask">
            <div style="display:flex;justify-content:space-between;align-items:center">
              <strong style="font-size:13px">结构化提问 (PendingUserInput) · 问题 1 / 1</strong>
              <span class="badge run">需你拍板</span>
            </div>
            <div style="font-size:13px;color:var(--soxu-text-secondary)">当原生 CLI（如 agy / opencode）暂不支持运行中热注入时，Steer 指令应默认采用哪种降级策略？</div>
            <div style="display:flex;flex-direction:column;gap:6px">
              ${[
                { id: "opt-1", label: "1. 柔和中断当前回合，自动携带新指令重开续航轮次（推荐）", rec: true },
                { id: "opt-2", label: "2. 自动降级为 Queue 排队，待本轮自然结束后再发出" },
              ]
                .map(
                  (o) => `
                <button class="wks-row ${state.askQuestion.selected === o.id ? "selected" : ""}" style="border-color:var(--soxu-border-subtle)" data-action="select-ask" data-id="${o.id}">
                  <span>${esc(o.label)}</span>
                  ${o.rec ? `<span class="badge accent">推荐</span>` : `<kbd>2</kbd>`}
                </button>`,
                )
                .join("")}
            </div>
            <div class="perm-actions">
              <span style="font-size:12px;color:var(--soxu-text-tertiary)">${state.askQuestion.submitted ? "✓ 已提交选择并继续编排" : "支持键盘数字键 1-2 快选"}</span>
              <button class="btn primary" data-action="submit-ask">${state.askQuestion.submitted ? "已确认" : "确认提交回答"}</button>
            </div>
          </div>

          <!-- Standard Approval Card -->
          ${
            aprSafe.status === "pending"
              ? `<div class="perm-card">
                <div style="display:flex;justify-content:space-between;align-items:center">
                  <span style="display:flex;align-items:center;gap:7px;font-weight:600;color:var(--soxu-semantic-warning)">${icon("shield", 16)} <span>需要人工授权 · Workspace 脚本验证</span></span>
                  <span class="badge warn">常规审批</span>
                </div>
                <div class="perm-detail">❯ pwsh -NoProfile -File scripts/check-all.ps1</div>
                <div class="perm-actions">
                  <div style="display:flex;gap:6px">
                    <button class="btn" data-action="approve" data-id="apr-safe" data-res="rejected">拒绝并指导</button>
                    <button class="btn" data-action="approve" data-id="apr-safe" data-res="session">本会话允许</button>
                  </div>
                  <button class="btn primary" data-action="approve" data-id="apr-safe" data-res="once">${icon("check", 14)}<span>允许一次 ↵</span></button>
                </div>
              </div>`
              : `<div class="tool-card"><div class="tool-card-hdr"><span>✓ 常规脚本验证审批已消解 (${esc(aprSafe.status)} · 同步至移动遥控端)</span></div></div>`
          }

          <!-- High-Risk Redline Approval Card -->
          ${
            aprDanger.status === "pending"
              ? `<div class="perm-card danger">
                <div style="display:flex;justify-content:space-between;align-items:center">
                  <span style="display:flex;align-items:center;gap:7px;font-weight:600;color:var(--soxu-semantic-danger)">${icon("shield", 16)} <span>高危红线拦截 · 递归删除命令</span></span>
                  <span class="badge danger">禁用始终允许</span>
                </div>
                <div class="perm-detail" style="border-color:var(--soxu-semantic-danger-border)">❯ Remove-Item -Recurse -Force ./target-fixtures</div>
                <div style="font-size:12px;color:var(--soxu-semantic-danger)">命中安全红线 I：默认焦点强制停留在「拒绝并指导修改」，禁止本会话自动放行破坏性命令。</div>
                ${
                  aprDanger.rejecting
                    ? `<div style="display:flex;gap:8px"><input id="danger-reject-input" class="cmdk-input" style="background:var(--soxu-surface-composer);padding:6px 10px;border-radius:8px;border:1px solid var(--soxu-border-strong)" placeholder="输入指导建议（例如：改用 TempDir 自动回收）" value="${esc(aprDanger.note)}"/><button class="btn danger" data-action="submit-danger-reject">发送指导</button></div>`
                    : `<div class="perm-actions">
                      <button class="btn danger" data-action="open-danger-reject">拒绝并指导修改（默认焦点）</button>
                      <button class="btn" disabled title="高危红线命令禁止本会话自动允许">本会话允许 (已锁定)</button>
                      <button class="btn" style="border-color:var(--soxu-semantic-danger-border);color:var(--soxu-semantic-danger)" data-action="approve" data-id="apr-danger" data-res="once">确认仅执行本次</button>
                    </div>`
                }
              </div>`
              : `<div class="tool-card"><div class="tool-card-hdr"><span>✓ 高危命令拦截已人工处理 (${esc(aprDanger.status)} ${aprDanger.note ? "· 指导：" + esc(aprDanger.note) : ""})</span></div></div>`
          }

          ${state.addedTurns
            .map(
              (t) => `<div class="user-turn">
                ${t.badge ? `<span class="steer-badge">${icon("bolt", 11)} ${esc(t.badge)}</span>` : ""}
                <div class="user-bubble">${esc(t.text)}</div>
              </div>`,
            )
            .join("")}
        </div>
      </div>
    </div>

    <!-- ConversationNavigationRail (20px) -->
    <nav class="turn-nav-rail" aria-label="对话轮次迷你导航轨">
      ${RAIL_TURNS.map(
        (rt) => `
        <button class="rail-tick ${rt.kind === "warning" ? "warning" : rt.kind === "handoff" ? "handoff" : ""} ${state.activeRailTurn === rt.idx ? "active" : ""}" data-action="jump-turn" data-turn="${rt.idx}">
          <span class="rail-tick-tooltip">${esc(rt.title)}</span>
        </button>`,
      ).join("")}
    </nav>
  </div>`;
}

function renderComposer() {
  const modeMeta = {
    queue: { label: "排队 Queue", cls: "" },
    steer: { label: "纠偏 Steer", cls: "steer" },
    interrupt: { label: "打断重发", cls: "interrupt" },
  }[state.composerMode];

  let popHtml = "";
  if (state.composerPopover === "handoff") {
    popHtml = `<div class="composer-popover">
      <div class="sec-hdr"><span>@agent 跨代理上下文移交 (Handoff)</span></div>
      ${["codex", "pi", "agy", "claude"]
        .map(
          (c) => `<button class="pop-item" data-action="pick-handoff" data-val="${c}">
            <span class="mono" style="color:var(--soxu-semantic-handoff)">@${c}</span>
            <span style="color:var(--soxu-text-tertiary);font-size:11.5px">附带最近 3 轮摘要与未提交 Diff</span>
          </button>`,
        )
        .join("")}
    </div>`;
  } else if (state.composerPopover === "cli") {
    popHtml = `<div class="composer-popover">
      <div class="sec-hdr"><span>选择原生 CLI 驱动</span></div>
      ${CLIS.filter((c) => c.ok)
        .map(
          (c) => `<button class="pop-item ${state.cli === c.id ? "active" : ""}" data-action="pick-cli" data-val="${c.id}">
            <span class="mono">${c.id} (${c.bin})</span><span class="badge neutral">v${c.ver}</span>
          </button>`,
        )
        .join("")}
    </div>`;
  } else if (state.composerPopover === "model") {
    popHtml = `<div class="composer-popover">
      <div class="sec-hdr"><span>${state.magpieMode === "magpie_managed" ? "Magpie 路由模型池" : "原生 CLI 直连模型"}</span></div>
      ${MODELS.map(
        (m) => `<button class="pop-item ${state.model === m.id ? "active" : ""}" data-action="pick-model" data-val="${m.id}">
          <div><div class="mono">${m.id}</div><div style="font-size:11px;color:var(--soxu-text-tertiary)">${m.tier} · ${m.note}</div></div>
          <span class="badge ${m.star ? "accent" : "neutral"}">${m.thinking.join("/")}</span>
        </button>`,
      ).join("")}
    </div>`;
  } else if (state.composerPopover === "thinking") {
    const curModel = MODELS.find((m) => m.id === state.model) || MODELS[0];
    popHtml = `<div class="composer-popover">
      <div class="sec-hdr"><span>思考强度与运行时特性</span></div>
      ${curModel.thinking
        .map(
          (t) => `<button class="pop-item ${state.thinking === t ? "active" : ""}" data-action="pick-thinking" data-val="${t}">
            <span class="mono">thinking: ${t}</span>${state.thinking === t ? icon("check", 13) : ""}
          </button>`,
        )
        .join("")}
      <button class="pop-item" data-action="toggle-fast">
        <span>Codex fast_mode 快速响应</span>
        <span class="badge ${state.fastMode ? "accent" : "neutral"}">${state.fastMode ? "已开启" : "关闭"}</span>
      </button>
    </div>`;
  }

  return `
  <div class="composer-dock">
    <div class="composer-inner">
      ${popHtml}
      ${
        state.lostControlToast
          ? `<div style="font-size:11.5px;color:var(--soxu-semantic-warning);padding:0 8px">${esc(state.lostControlToast)}</div>`
          : ""
      }
      ${
        state.queuedPrompt
          ? `<div class="queue-strip">
              <span class="mono" style="overflow:hidden;text-overflow:ellipsis;white-space:nowrap;color:var(--soxu-text-secondary)">排队等待本轮结束：${esc(state.queuedPrompt)}</span>
              <span style="display:flex;gap:6px;flex-shrink:0">
                <button class="btn" style="height:24px;font-size:11.5px;color:var(--soxu-semantic-running)" data-action="promote-queue">${icon("bolt", 12)}<span>提级为 Steer</span></button>
                <button class="icon-btn" style="height:24px" data-action="drop-queue">${icon("x", 12)}</button>
              </span>
            </div>`
          : ""
      }
      <div class="composer-card ${modeMeta.cls}">
        <div class="context-chips">
          <span class="ctx-chip mono">${icon("file", 12)} crates/soxu-drivers/src/lib.rs:22-27</span>
          <span class="ctx-chip handoff mono" data-action="toggle-pop" data-pop="handoff" style="cursor:pointer">@agent 移交</span>
        </div>
        <textarea id="pg-composer" class="composer-textarea" rows="2" placeholder="输入指令或键入 @ 跨代理移交；运行中输入自动按「${modeMeta.label}」投递...">${esc(state.composerText)}</textarea>
        <div class="composer-toolbar">
          <div class="pill-cluster">
            <button class="runtime-pill mono" data-action="toggle-pop" data-pop="cli">${state.cli} ${icon("chevronDown", 12)}</button>
            <button class="runtime-pill mono" data-action="toggle-pop" data-pop="model">${state.model} ${icon("chevronDown", 12)}</button>
            <button class="runtime-pill mono" data-action="toggle-pop" data-pop="thinking">${state.thinking}${state.fastMode ? " · fast" : ""} ${icon("chevronDown", 12)}</button>
            <button class="runtime-pill" data-action="cycle-perm">${icon("shield", 12)} <span>${state.permLevel}</span></button>
          </div>
          <div style="display:flex;align-items:center;gap:6px">
            <button class="runtime-pill" data-action="cycle-mode" style="border-color:var(--soxu-semantic-running-border);color:var(--soxu-semantic-running)">${modeMeta.label}</button>
            <button class="send-circle" data-action="send-prompt" title="${state.composerText.trim() ? "发送指令" : "停止当前活跃回合"}">
              ${state.composerText.trim() ? icon("arrowUp", 16) : icon("stop", 14)}
            </button>
          </div>
        </div>
      </div>
    </div>
  </div>`;
}

function renderRightInspector() {
  if (!state.rightOpen) return "";
  let body = "";
  if (state.rightTab === "diff") {
    body = `
      <div style="display:flex;justify-content:space-between;align-items:center;gap:6px">
        <div class="view-tabs">
          <button class="view-tab ${state.diffLayout === "unified" ? "active" : ""}" data-action="set-diff-layout" data-val="unified">Unified</button>
          <button class="view-tab ${state.diffLayout === "split" ? "active" : ""}" data-action="set-diff-layout" data-val="split">Split 双栏</button>
        </div>
        <button class="btn primary" style="height:26px;font-size:11.5px" data-action="open-modal" data-modal="commit">${icon("gitCommit", 13)}<span>Emoji 提交</span></button>
      </div>
      <div class="mono" style="font-size:11.5px;color:var(--soxu-text-tertiary)">crates/soxu-drivers/src/lib.rs · vs origin/main</div>
      ${
        state.diffLayout === "unified"
          ? renderDiffBox()
          : `<div class="code-box" style="display:grid;grid-template-columns:1fr 1fr">
              <div style="border-right:1px solid var(--soxu-border-hairline);padding:6px;font-size:11.5px;color:var(--soxu-semantic-danger)">23 - // 旧版仅返回路径</div>
              <div style="padding:6px;font-size:11.5px;color:var(--soxu-semantic-success)">25 + pub const fn supports_live_steer(self) -&gt; bool</div>
            </div>`
      }`;
  } else if (state.rightTab === "files") {
    const files = ["AGENTS.md", "design-system/soxu/tokens.json", "crates/soxu-drivers/src/lib.rs", "crates/soxu-magpie/src/lib.rs", "apps/mobile-web/src/main.tsx"];
    body = `
      <div class="sec-hdr"><span>工作区文件树 (后台无阻塞快照)</span></div>
      ${files
        .map(
          (f) => `<button class="wks-row ${state.selectedFile === f ? "selected" : ""}" data-action="pick-file" data-val="${f}">
            <span class="wks-name-line mono">${icon("file", 13)} <span class="wks-branch">${f}</span></span>
          </button>`,
        )
        .join("")}
      <div class="term-box" style="font-size:11.5px"><span class="syn-com">// 预览：${esc(state.selectedFile)}</span>
<span class="syn-kw">pub enum</span> <span class="syn-ty">EnvironmentMode</span> {
  MagpieManaged,
  StandaloneNative,
}</div>`;
  } else if (state.rightTab === "scripts") {
    body = `
      <div class="sec-hdr"><span>paseo.json 受管生命周期脚本</span></div>
      ${state.scripts
        .map(
          (s) => `<div class="tool-card" style="padding:10px;display:flex;justify-content:space-between;align-items:center;gap:8px">
            <div style="min-width:0">
              <div class="mono" style="font-size:12.5px;font-weight:600">${s.name} ${s.port ? `<span class="badge accent">${s.port}</span>` : ""}</div>
              <div class="mono" style="font-size:11px;color:var(--soxu-text-tertiary);overflow:hidden;text-overflow:ellipsis;white-space:nowrap">${s.cmd}</div>
            </div>
            <button class="btn ${s.running ? "" : "primary"}" style="height:26px" data-action="toggle-script" data-val="${s.name}">${s.running ? "停止" : "启动"}</button>
          </div>`,
        )
        .join("")}`;
  } else {
    body = `
      <div class="sec-hdr"><span>受控分屏 PTY 终端 · ANSI 16 色校准</span></div>
      <div class="term-box">
        <div><span style="color:var(--soxu-ansi-bright-green)">dev@soxu</span> <span style="color:var(--soxu-ansi-blue)">~/projects/Soxu</span></div>
        <div><span style="color:var(--soxu-ansi-yellow)">WARN</span> CARGO_TARGET_DIR = ../target-cache</div>
        <div><span style="color:var(--soxu-ansi-green)">OK</span> check-all 6/6 gates passed</div>
      </div>`;
  }

  return `
  <aside class="pane-right" aria-label="右侧上下文审阅栏">
    <div class="inspector-tabs">
      ${[
        { id: "diff", label: "Diff (4)" },
        { id: "files", label: "Files" },
        { id: "scripts", label: "Scripts" },
        { id: "terminal", label: "Terminal" },
      ]
        .map((t) => `<button class="inspector-tab ${state.rightTab === t.id ? "active" : ""}" data-action="set-rtab" data-val="${t.id}">${t.label}</button>`)
        .join("")}
    </div>
    <div class="inspector-body">${body}</div>
  </aside>`;
}

/* ── View 2..5 Labs ────────────────────────────────────────────────────── */
function renderMagpieLab() {
  const managed = state.magpieMode === "magpie_managed";
  return `<div class="lab-scroll"><div class="lab-container">
    <div class="tool-card" style="padding:16px;border-color:${managed ? "rgba(25,195,125,.35)" : "var(--soxu-border-subtle)"}">
      <div style="display:flex;justify-content:space-between;align-items:center;gap:12px;flex-wrap:wrap">
        <div>
          <h2 style="margin:0;font-size:17px">${managed ? "MagpieManaged 协同拓扑（已识别 ~/.config/magpie）" : "StandaloneNative 独立原生模式（未安装 Magpie 自动降级）"}</h2>
          <p style="margin:4px 0 0;font-size:12.5px;color:var(--soxu-text-secondary)">${icon("lock", 13)} 只读探测外部配置，严禁覆写 ~/.claude / ~/.codex / ~/.pi / ~/.config/magpie，自动跳过密钥文件。</p>
        </div>
        <button class="btn primary" data-action="toggle-magpie">切换模拟为 ${managed ? "StandaloneNative 降级态" : "MagpieManaged 协同态"}</button>
      </div>
      <div class="grid-4" style="margin-top:14px">
        <div class="tool-card" style="padding:12px;background:var(--soxu-surface-card-muted)"><div style="font-size:11px;color:var(--soxu-text-tertiary)">本地模型网关</div><div class="mono" style="margin-top:4px;font-weight:600">${managed ? "127.0.0.1:3425/v1 · 2ms" : "原生 CLI 直连"}</div></div>
        <div class="tool-card" style="padding:12px;background:var(--soxu-surface-card-muted)"><div style="font-size:11px;color:var(--soxu-text-tertiary)">Shared MCP 池</div><div class="mono" style="margin-top:4px;font-weight:600">${managed ? "github / fetch 全局复用" : "按单 CLI 配置读取"}</div></div>
        <div class="tool-card" style="padding:12px;background:var(--soxu-surface-card-muted)"><div style="font-size:11px;color:var(--soxu-text-tertiary)">Direct MCP（Worktree 自动补齐）</div><div class="mono" style="margin-top:4px;font-weight:600">aoci · codegraph</div></div>
        <div class="tool-card" style="padding:12px;background:var(--soxu-surface-card-muted)"><div style="font-size:11px;color:var(--soxu-text-tertiary)">RuntimeExtensionHook</div><div class="mono" style="margin-top:4px;font-weight:600">NoopExtensionHost (槽位就绪)</div></div>
      </div>
    </div>

    <div class="grid-2">
      ${CLIS.map(
        (c) => `<div class="tool-card" style="padding:14px;opacity:${c.ok ? 1 : 0.52}">
          <div style="display:flex;justify-content:space-between;align-items:center">
            <strong class="mono">${c.id} (${c.bin})</strong>
            <span class="badge ${c.ok ? "accent" : "neutral"}">${c.ok ? "v" + c.ver : "未安装 · 已隐藏"}</span>
          </div>
          <div class="mono" style="font-size:11.5px;color:var(--soxu-text-tertiary);margin:6px 0">${c.path}</div>
          <div style="display:flex;gap:5px;flex-wrap:wrap">${c.caps.map((cap) => `<span class="label-chip mono">${cap}</span>`).join("")}</div>
        </div>`,
      ).join("")}
    </div>
  </div></div>`;
}

function renderSchedulesLab() {
  return `<div class="lab-scroll"><div class="lab-container">
    <div style="display:flex;justify-content:space-between;align-items:center">
      <div><h2 style="margin:0;font-size:17px">Schedules &amp; Heartbeats 自动化调度中心</h2><p style="margin:4px 0 0;font-size:12.5px;color:var(--soxu-text-secondary)">Cron 定时拉起隔离 Worktree 代理或周期唤醒当前会话</p></div>
      <button class="btn primary" data-action="toast-schedule">+ 新建 Cron 定时任务</button>
    </div>
    ${state.schedules
      .map(
        (s) => `<div class="tool-card" style="padding:14px;display:flex;justify-content:space-between;align-items:center;gap:12px;flex-wrap:wrap">
          <div>
            <div style="display:flex;align-items:center;gap:8px">
              <strong>${esc(s.name)}</strong>
              <span class="badge ${s.kind === "Heartbeat" ? "handoff" : "run"}">${s.kind}</span>
              ${s.unreadFail ? `<span class="badge danger">未读失败</span>` : ""}
            </div>
            <div class="mono" style="font-size:12px;color:var(--soxu-text-secondary);margin-top:4px">${s.cron} · ${s.human} · ${s.target}</div>
          </div>
          <div style="display:flex;gap:8px">
            <button class="btn" data-action="toggle-schedule" data-id="${s.id}">${s.active ? "暂停" : "恢复"}</button>
            <button class="btn" data-action="toast-run-once" data-id="${s.id}">立即运行一次</button>
          </div>
        </div>`,
      )
      .join("")}
  </div></div>`;
}

function renderMobileLab() {
  const w = state.mobileViewport;
  return `<div class="lab-scroll"><div class="lab-container" style="align-items:center">
    <div style="display:flex;gap:8px;align-items:center">
      <span style="font-size:13px;color:var(--soxu-text-secondary)">切换窗口宽度矩阵：</span>
      ${[
        { id: "344", label: "小折叠外屏 344×680 (compact)" },
        { id: "393", label: "iPhone 15 Pro 393×720 (compact)" },
        { id: "820", label: "iPad 竖屏 820×660 (regular)" },
      ]
        .map((v) => `<button class="btn ${w === v.id ? "primary" : ""}" data-action="set-mvw" data-val="${v.id}">${v.label}</button>`)
        .join("")}
    </div>
    <div class="phone-frame" style="width:${w}px;height:660px">
      <div style="padding:12px 14px;border-bottom:1px solid var(--soxu-border-hairline);display:flex;justify-content:space-between;align-items:center;background:var(--soxu-surface-sidebar)">
        <span style="font-weight:600;font-size:13px">Soxu Remote · ${w}px</span>
        <span class="badge accent">seq #1056</span>
      </div>
      <div style="flex:1;overflow:auto;padding:14px;display:flex;flex-direction:column;gap:12px;max-width:760px;width:100%;margin:0 auto">
        <div class="user-turn"><div class="user-bubble" style="font-size:13.5px">只跑 soxu-drivers 夹具单测，结果回传。</div></div>
        <div class="perm-card"><strong style="font-size:13px;color:var(--soxu-semantic-warning)">待大拇指审批 · 1 项</strong><div class="perm-detail">❯ cargo test -p soxu-drivers</div><div style="display:grid;grid-template-columns:1fr 1fr;gap:8px"><button class="btn" style="height:44px;justify-content:center">拒绝/指导</button><button class="btn primary" style="height:44px;justify-content:center">允许一次 ✓</button></div></div>
      </div>
      <div style="padding:10px 12px;border-top:1px solid var(--soxu-border-hairline)">
        <div style="background:var(--soxu-surface-composer);border:1px solid var(--soxu-border-subtle);border-radius:18px;padding:8px 12px;display:flex;justify-content:space-between;align-items:center">
          <span style="font-size:16px;color:var(--soxu-text-ghost)">输入 16px Steer 指令...</span>
          <span class="send-circle">${icon("arrowUp", 14)}</span>
        </div>
      </div>
    </div>
  </div></div>`;
}

function renderTokensLab() {
  const surfaces = [
    ["--soxu-surface-canvas", "L1 Canvas (#000000)", "17.8:1"],
    ["--soxu-surface-sidebar", "L3 Sidebar (#0D0D0D)", "15.9:1"],
    ["--soxu-surface-card", "L4 Card (#171717)", "14.9:1"],
    ["--soxu-surface-raised", "L5 Raised / Composer (#212121)", "13.2:1"],
    ["--soxu-surface-floating", "L6 Floating (#262626)", "12.4:1"],
    ["--soxu-surface-control", "L6+ Control (#2F2F2F)", "11.1:1"],
  ];
  return `<div class="lab-scroll"><div class="lab-container">
    <h2 style="margin:0;font-size:17px">Pure Black 7 级中性灰阶海拔 &amp; WCAG AA/AAA 对比度验证</h2>
    <div class="grid-2">
      ${surfaces
        .map(
          ([cssVar, title, ratio]) => `<div class="tool-card" style="background:var(${cssVar});padding:16px;display:flex;justify-content:space-between;align-items:center">
            <div><strong class="mono">${title}</strong><div class="mono" style="font-size:11.5px;color:var(--soxu-text-tertiary)">${cssVar}</div></div>
            <span class="badge accent">${ratio} AAA</span>
          </div>`,
        )
        .join("")}
    </div>
    <h3 style="margin:8px 0 0;font-size:14px">ANSI 16 色纯黑校准调色板</h3>
    <div style="display:grid;grid-template-columns:repeat(8,1fr);gap:8px">
      ${["black", "red", "green", "yellow", "blue", "magenta", "cyan", "white", "bright-black", "bright-red", "bright-green", "bright-yellow", "bright-blue", "bright-magenta", "bright-cyan", "bright-white"]
        .map((c) => `<div class="tool-card" style="padding:8px;text-align:center"><div style="height:24px;border-radius:6px;background:var(--soxu-ansi-${c});margin-bottom:4px"></div><div class="mono" style="font-size:10px">${c}</div></div>`)
        .join("")}
    </div>
  </div></div>`;
}

/* ── Global Modals (5 Interactive Flows) ───────────────────────────────── */
function renderModal() {
  if (!state.activeModal) return "";
  let inner = "";
  if (state.activeModal === "cmdk") {
    const allItems = [
      { sec: "Sessions", title: "feat/12-native-drivers · codex", action: "close-modal" },
      { sec: "Files", title: "crates/soxu-drivers/src/lib.rs", action: "close-modal" },
      { sec: "Commands", title: "打开手机扫码遥控配对浮层 (Ctrl+Shift+M)", action: "open-pair" },
      { sec: "Commands", title: "打开 Git Emoji AI 提交预览弹窗", action: "open-commit" },
      { sec: "Commands", title: "切换 Pure Black / Clean Light 主题", action: "toggle-theme" },
    ].filter((x) => !state.cmdkQuery.trim() || x.title.toLowerCase().includes(state.cmdkQuery.toLowerCase()));

    inner = `<div class="cmdk-card" onclick="event.stopPropagation()">
      <div class="cmdk-input-row">${icon("search", 16)}<input id="cmdk-input" class="cmdk-input" placeholder="输入搜索会话、文件、CLI 历史或命令..." value="${esc(state.cmdkQuery)}"/><kbd>Esc</kbd></div>
      <div class="cmdk-results">
        ${
          allItems.length
            ? allItems
                .map(
                  (it) => `<button class="pop-item" data-action="${it.action}"><span>${esc(it.title)}</span><span class="badge neutral">${it.sec}</span></button>`,
                )
                .join("")
            : `<div style="padding:36px;text-align:center;color:var(--soxu-text-tertiary)">未匹配 "${esc(state.cmdkQuery)}"</div>`
        }
      </div>
      <div class="cmdk-footer"><span>↑↓ 选择</span><span>↵ 执行</span><span>Esc 清空/关闭</span></div>
    </div>`;
  } else if (state.activeModal === "pair") {
    inner = `<div class="modal-card" onclick="event.stopPropagation()">
      <div style="display:flex;justify-content:space-between;align-items:center"><strong>手机遥控扫码配对 (#offer= 一次性令牌)</strong><button class="icon-btn" data-action="close-modal">${icon("x", 15)}</button></div>
      <div style="display:flex;gap:16px;align-items:center;flex-wrap:wrap">
        <div style="background:#ffffff;padding:16px;border-radius:12px;width:136px;height:136px;display:grid;grid-template-columns:repeat(5,1fr);gap:4px">
          ${Array.from({ length: 25 }, (_, i) => `<span style="background:${i % 2 === 0 || i < 5 || i % 5 === 0 ? "#000000" : "transparent"};border-radius:2px"></span>`).join("")}
        </div>
        <div style="flex:1;min-width:200px">
          <div style="font-size:12px;color:var(--soxu-text-secondary)">6 位一次性数字配对码（5 分钟失效）：</div>
          <div class="mono" style="font-size:26px;font-weight:700;letter-spacing:.18em;margin:6px 0">849 217</div>
          <div class="mono" style="font-size:11.5px;color:var(--soxu-text-tertiary)">http://192.168.1.42:7698/#offer=eyJ2IjoyfQ</div>
        </div>
      </div>
      <div style="display:flex;justify-content:flex-end;gap:8px"><button class="btn" data-action="close-modal">关闭</button></div>
    </div>`;
  } else if (state.activeModal === "rewind") {
    inner = `<div class="modal-card" onclick="event.stopPropagation()">
      <div style="display:flex;justify-content:space-between;align-items:center"><strong>检查点 #04 安全回退 (已自动创建备份 Stash)</strong><button class="icon-btn" data-action="close-modal">${icon("x", 15)}</button></div>
      <button class="wks-row selected" style="border-color:var(--soxu-border-subtle)" data-action="confirm-rewind" data-val="仅分叉对话（保留当前磁盘代码）"><span>1. 仅分叉对话，保留磁盘代码不变（默认安全项）</span></button>
      <button class="wks-row" style="border-color:var(--soxu-border-subtle)" data-action="confirm-rewind" data-val="仅回退代码至 #04（保留后续对话）"><span>2. 仅回退工作树代码至 commit 7c4e91a</span></button>
      <button class="wks-row" style="border-color:var(--soxu-semantic-danger-border);color:var(--soxu-semantic-danger)" data-action="confirm-rewind" data-val="代码与对话同步回退至 #04"><span>3. 代码与对话同步完整回退至 #04</span></button>
    </div>`;
  } else if (state.activeModal === "commit") {
    inner = `<div class="modal-card" onclick="event.stopPropagation()">
      <div style="display:flex;justify-content:space-between;align-items:center"><strong>Git Emoji AI 提交与隐私门禁预检</strong><button class="icon-btn" data-action="close-modal">${icon("x", 15)}</button></div>
      <div class="term-box">${"\u2728"} feat(drivers): 补齐原生 CLI 热纠偏能力探测与 TempDir 单测</div>
      <div style="display:flex;gap:8px;flex-wrap:wrap"><span class="badge accent">✓ Git Emoji 格式合规</span><span class="badge accent">✓ 无个人目录泄露</span><span class="badge accent">✓ Gitleaks 通过</span></div>
      <div style="display:flex;justify-content:flex-end;gap:8px"><button class="btn" data-action="close-modal">取消</button><button class="btn primary" data-action="confirm-commit">提交当前暂存</button></div>
    </div>`;
  } else if (state.activeModal === "onboarding") {
    const step = state.onboardingStep;
    const titles = ["Step 1/3 · 本机 Magpie 与原生 CLI 只读探测", "Step 2/3 · 绑定首个 Git 项目与共享缓存", "Step 3/3 · 手机扫码遥控与快捷键速览"];
    inner = `<div class="modal-card" onclick="event.stopPropagation()">
      <div style="display:flex;justify-content:space-between;align-items:center"><strong>${titles[step - 1]}</strong><button class="btn" style="height:26px" data-action="close-modal">跳过引导</button></div>
      <div style="font-size:13px;color:var(--soxu-text-secondary)">${step === 1 ? "已探测 6 个可用原生 CLI 与 Magpie 本地网关，绝不篡改全局配置。" : step === 2 ? "已识别 Git 仓库并开启多 Worktree 共享构建缓存 (CARGO_TARGET_DIR)。" : "使用 Ctrl+K 全局搜索、Ctrl+Shift+M 生成手机一次性扫码配对码。"}</div>
      <div style="display:flex;justify-content:space-between">
        <button class="btn" ${step === 1 ? "disabled" : ""} data-action="onb-prev">上一步</button>
        <button class="btn primary" data-action="onb-next">${step === 3 ? "开始使用 Soxu" : "下一步"}</button>
      </div>
    </div>`;
  }
  return `<div class="modal-scrim" data-action="close-modal">${inner}</div>`;
}

/* ── Root Render ───────────────────────────────────────────────────────── */
function render() {
  document.documentElement.dataset.theme = state.theme;
  const root = document.getElementById("soxu-playground");
  if (!root) return;

  let mainHtml = "";
  if (state.view === "workbench") {
    mainHtml = `
    <div class="workbench-body">
      ${renderLeftSidebar()}
      ${state.leftOpen ? `<div class="resize-gutter"></div>` : ""}
      <main class="pane-center">
        <div class="session-tabs">
          ${state.tabs
            .map(
              (t) => `<button class="session-tab ${state.activeTabId === t.id ? "active" : ""} ${t.draft ? "draft" : ""}" data-action="pick-tab" data-id="${t.id}">
                <span class="mono" style="font-size:11px;color:var(--soxu-text-tertiary)">${t.cli}</span>
                <span class="tab-title">${esc(t.title)}</span>
              </button>`,
            )
            .join("")}
          <button class="icon-btn" data-action="add-tab" title="新建空白草稿页签">${icon("plus", 14)}</button>
        </div>
        <div class="subagent-bar">
          <span>子代理轨:</span>
          ${state.subagents
            .map(
              (sa) => `<span class="subagent-pill mono">
                <span>${sa.cli} · ${esc(sa.role)} (${sa.duration})</span>
                ${sa.detached ? `<span style="color:var(--soxu-semantic-accent)">已独立</span>` : `<button class="subagent-detach" data-action="detach-sub" data-id="${sa.id}" style="display:inline-flex;align-items:center;gap:3px">独立成页签 ${icon("detach", 10)}</button>`}
              </span>`,
            )
            .join("")}
        </div>
        ${renderTranscript()}
        ${renderComposer()}
      </main>
      ${state.rightOpen ? `<div class="resize-gutter"></div>` : ""}
      ${renderRightInspector()}
    </div>`;
  } else if (state.view === "magpie") {
    mainHtml = renderMagpieLab();
  } else if (state.view === "schedules") {
    mainHtml = renderSchedulesLab();
  } else if (state.view === "mobile") {
    mainHtml = renderMobileLab();
  } else {
    mainHtml = renderTokensLab();
  }

  root.innerHTML = `
    <header class="titlebar">
      <div class="titlebar-left">
        <span class="brand-mark"><span class="brand-dot"></span><span>Soxu · 索序</span></span>
        ${state.view === "workbench" ? `<button class="icon-btn ${state.leftOpen ? "active" : ""}" data-action="toggle-left" title="Ctrl+B">${icon("sidebarL", 14)}</button>` : ""}
        <div class="view-tabs">
          ${[
            { id: "workbench", label: "桌面四栏工作台" },
            { id: "magpie", label: "Magpie & CLI 诊断" },
            { id: "schedules", label: "Schedules 调度" },
            { id: "mobile", label: "移动端视口矩阵" },
            { id: "tokens", label: "Pure Black 令牌" },
          ]
            .map((v) => `<button class="view-tab ${state.view === v.id ? "active" : ""}" data-action="go-view" data-view="${v.id}">${v.label}</button>`)
            .join("")}
        </div>
      </div>
      <button class="cmdk-trigger" data-action="open-modal" data-modal="cmdk">
        <span style="display:flex;align-items:center;gap:6px">${icon("search", 13)}<span>搜索会话 / 文件 / 命令 / @agent...</span></span>
        <kbd>Ctrl+K</kbd>
      </button>
      <div class="titlebar-right">
        <button class="icon-btn active" data-action="toggle-magpie">
          <span class="brand-dot" style="background:${state.magpieMode === "magpie_managed" ? "var(--soxu-semantic-accent)" : "var(--soxu-semantic-warning)"}"></span>
          <span>${state.magpieMode === "magpie_managed" ? "Magpie 协同" : "独立原生"}</span>
        </button>
        <button class="icon-btn" data-action="open-modal" data-modal="pair">${icon("phone", 14)}<span>扫码配对</span></button>
        <button class="icon-btn" data-action="toggle-theme">${icon("sunMoon", 14)}<span>${state.theme === "dark" ? "Pure Black" : "Light"}</span></button>
        ${state.view === "workbench" ? `<button class="icon-btn ${state.rightOpen ? "active" : ""}" data-action="toggle-right" title="Ctrl+J">${icon("sidebarR", 14)}</button>` : ""}
      </div>
    </header>
    ${mainHtml}
    <footer class="statusbar tabular">
      <div class="statusbar-group">
        <span class="statusbar-item mono"><span class="brand-dot"></span>Daemon 127.0.0.1:7698 (seq #1056)</span>
        <button class="statusbar-item mono" data-action="go-view" data-view="magpie">Magpie :3425 · 2ms</button>
        <span class="statusbar-item mono">${icon("branch", 12)} ui/3-m0.5*</span>
      </div>
      <div class="statusbar-group">
        ${state.toast ? `<span style="color:var(--soxu-semantic-accent)">${esc(state.toast)}</span>` : ""}
        <button class="statusbar-item" data-action="open-modal" data-modal="pair">${icon("phone", 12)}<span>2 台移动设备在线</span></button>
      </div>
    </footer>
    ${renderModal()}
  `;

  const inp = document.getElementById("pg-composer");
  if (inp) {
    inp.addEventListener("input", (e) => {
      state.composerText = e.target.value;
      if (state.composerText.endsWith("@") && state.composerPopover !== "handoff") {
        state.composerPopover = "handoff";
        render();
        const restored = document.getElementById("pg-composer");
        if (restored) {
          restored.focus();
          restored.setSelectionRange(restored.value.length, restored.value.length);
        }
      }
    });
    inp.addEventListener("keydown", (e) => {
      if (e.key === "Enter" && !e.shiftKey) {
        e.preventDefault();
        sendPromptFromComposer();
      }
    });
  }
  const cmdk = document.getElementById("cmdk-input");
  if (cmdk) {
    cmdk.focus();
    cmdk.setSelectionRange(cmdk.value.length, cmdk.value.length);
    cmdk.addEventListener("input", (e) => {
      state.cmdkQuery = e.target.value;
      render();
    });
  }
  const dangerInp = document.getElementById("danger-reject-input");
  if (dangerInp) {
    dangerInp.focus();
    dangerInp.addEventListener("input", (e) => {
      state.approvals["apr-danger"].note = e.target.value;
    });
  }
}

function sendPromptFromComposer() {
  const txt = state.composerText.trim();
  if (!txt) {
    showToast("已发送 SIGINT 停止当前活跃回合");
    return;
  }
  if (state.composerMode === "queue") {
    state.queuedPrompt = txt;
    state.composerText = "";
    render();
    showToast("已加入本轮末尾待发队列");
    return;
  }
  state.addedTurns.push({
    text: txt,
    badge: state.composerMode === "steer" ? "Steer 中途纠偏 · 下一步生效" : "Interrupt 已打断重发",
  });
  state.composerText = "";
  render();
}

document.addEventListener("click", (e) => {
  const el = e.target.closest("[data-action]");
  if (!el) return;
  const act = el.dataset.action;
  if (act === "toggle-theme") state.theme = state.theme === "dark" ? "light" : "dark";
  else if (act === "toggle-left") state.leftOpen = !state.leftOpen;
  else if (act === "toggle-right") state.rightOpen = !state.rightOpen;
  else if (act === "go-view") state.view = el.dataset.view;
  else if (act === "select-wks") {
    state.selectedWorkspace = el.dataset.id;
    showToast(`已切换工作区：${el.dataset.id}`);
  } else if (act === "set-rtab") state.rightTab = el.dataset.val;
  else if (act === "set-diff-layout") state.diffLayout = el.dataset.val;
  else if (act === "pick-file") state.selectedFile = el.dataset.val;
  else if (act === "toggle-script") {
    state.scripts = state.scripts.map((s) => (s.name === el.dataset.val ? { ...s, running: !s.running } : s));
  } else if (act === "toggle-reason") state.reasoningOpen[el.dataset.id] = !state.reasoningOpen[el.dataset.id];
  else if (act === "toggle-tool") state.toolsOpen[el.dataset.id] = !state.toolsOpen[el.dataset.id];
  else if (act === "select-ask") state.askQuestion.selected = el.dataset.id;
  else if (act === "submit-ask") {
    state.askQuestion.submitted = true;
    showToast("已提交结构化问题选型回答");
  } else if (act === "approve") {
    state.approvals[el.dataset.id].status = el.dataset.res;
    showToast(`审批已消解 (${el.dataset.res})`);
  } else if (act === "open-danger-reject") state.approvals["apr-danger"].rejecting = true;
  else if (act === "submit-danger-reject") {
    state.approvals["apr-danger"].status = "rejected";
    state.approvals["apr-danger"].rejecting = false;
    showToast("已拦截高危命令并附带修改指导");
  } else if (act === "jump-turn") {
    state.activeRailTurn = Number(el.dataset.turn);
    const target = document.getElementById(`turn-${state.activeRailTurn}`);
    if (target) target.scrollIntoView({ behavior: "smooth" });
  } else if (act === "pick-tab") state.activeTabId = el.dataset.id;
  else if (act === "add-tab") {
    const id = "tab-new-" + Date.now();
    state.tabs.push({ id, cli: state.cli, title: "新建草稿会话", status: "done", draft: true });
    state.activeTabId = id;
  } else if (act === "detach-sub") {
    const sa = state.subagents.find((x) => x.id === el.dataset.id);
    if (sa && !sa.detached) {
      sa.detached = true;
      const tid = "tab-sub-" + sa.id;
      state.tabs.push({ id: tid, cli: sa.cli, title: `子代理: ${sa.role}`, status: sa.status, draft: false });
      state.activeTabId = tid;
      showToast(`已将子代理 ${sa.role} 独立为工作区页签`);
    }
  } else if (act === "toggle-pop") {
    state.composerPopover = state.composerPopover === el.dataset.pop ? null : el.dataset.pop;
  } else if (act === "pick-handoff") {
    state.composerText = `@${el.dataset.val} ` + state.composerText;
    state.composerPopover = null;
  } else if (act === "pick-cli") {
    state.cli = el.dataset.val;
    state.composerPopover = null;
  } else if (act === "pick-model") {
    state.model = el.dataset.val;
    const m = MODELS.find((x) => x.id === state.model);
    if (m && !m.thinking.includes(state.thinking)) {
      const old = state.thinking;
      state.thinking = m.thinking[0];
      state.lostControlToast = `已因切换至 ${state.model} 自动调整不支持的思考档位 (${old} → ${state.thinking})`;
    } else {
      state.lostControlToast = null;
    }
    state.composerPopover = null;
  } else if (act === "pick-thinking") {
    state.thinking = el.dataset.val;
    state.composerPopover = null;
  } else if (act === "toggle-fast") {
    state.fastMode = !state.fastMode;
  } else if (act === "cycle-perm") {
    const order = ["ask", "auto-edit", "full-access"];
    state.permLevel = order[(order.indexOf(state.permLevel) + 1) % order.length];
  } else if (act === "cycle-mode") {
    const order = ["steer", "queue", "interrupt"];
    state.composerMode = order[(order.indexOf(state.composerMode) + 1) % order.length];
  } else if (act === "promote-queue") {
    if (state.queuedPrompt) {
      state.addedTurns.push({ text: state.queuedPrompt, badge: "由排队提级为 Steer" });
      state.queuedPrompt = null;
    }
  } else if (act === "drop-queue") state.queuedPrompt = null;
  else if (act === "send-prompt") {
    sendPromptFromComposer();
    return;
  } else if (act === "toggle-magpie") {
    state.magpieMode = state.magpieMode === "magpie_managed" ? "standalone_native" : "magpie_managed";
  } else if (act === "toggle-schedule") {
    state.schedules = state.schedules.map((s) => (s.id === el.dataset.id ? { ...s, active: !s.active, unreadFail: false } : s));
  } else if (act === "set-mvw") state.mobileViewport = el.dataset.val;
  else if (act === "open-modal") state.activeModal = el.dataset.modal;
  else if (act === "open-pair") state.activeModal = "pair";
  else if (act === "open-commit") state.activeModal = "commit";
  else if (act === "close-modal") state.activeModal = null;
  else if (act === "confirm-rewind") {
    state.activeModal = null;
    showToast(`已完成：${el.dataset.val}`);
  } else if (act === "confirm-commit") {
    state.activeModal = null;
    showToast("✓ Git Emoji 提交预演完成");
  } else if (act === "onb-prev" && state.onboardingStep > 1) state.onboardingStep--;
  else if (act === "onb-next") {
    if (state.onboardingStep < 3) state.onboardingStep++;
    else {
      state.onboardingStep = 1;
      state.activeModal = null;
      showToast("Onboarding 引导已完成");
    }
  } else if (act === "toast-worktree") showToast("已打开新建 Worktree 向导（Branch-off / PR Checkout）");
  else if (act === "toast-schedule") showToast("已打开 5 段 Cron 新建调度表（默认隔离 Worktree）");
  else if (act === "toast-run-once") showToast("已派发隔离 Worktree 单次巡检测试");

  render();
});

document.addEventListener("keydown", (e) => {
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
    e.preventDefault();
    state.activeModal = state.activeModal === "cmdk" ? null : "cmdk";
    render();
  } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "b" && state.view === "workbench") {
    e.preventDefault();
    state.leftOpen = !state.leftOpen;
    render();
  } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "j" && state.view === "workbench") {
    e.preventDefault();
    state.rightOpen = !state.rightOpen;
    render();
  } else if (e.key === "Escape") {
    if (state.activeModal === "cmdk" && state.cmdkQuery.trim()) {
      state.cmdkQuery = "";
      render();
    } else if (state.activeModal) {
      state.activeModal = null;
      render();
    } else if (state.composerPopover) {
      state.composerPopover = null;
      render();
    }
  }
});

render();
