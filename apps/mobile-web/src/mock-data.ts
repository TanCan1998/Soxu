import type {
  AskQuestionState,
  ApprovalItem,
  CliOption,
  ModelOption,
  NativeSessionHistory,
  ScheduleItem,
  ScriptItem,
  TurnMessage,
  WorkspaceItem,
} from "./types.ts";

export const INITIAL_WORKSPACES: WorkspaceItem[] = [
  {
    id: "wks-m2",
    project: "Soxu",
    branch: "feat/12-native-drivers",
    isolation: "worktree",
    agent: "codex",
    model: "group/gpt-6.2-sol",
    bucket: "needs_input",
    summary: "2 项权限待确认（含 1 项高危清理拦截）",
    diffStat: "+142 -18",
    label: "M2 驱动",
  },
  {
    id: "wks-m1",
    project: "Soxu",
    branch: "feat/8-ws-cursor-stream",
    isolation: "worktree",
    agent: "pi",
    model: "group/claude-sonnet-5.5",
    bucket: "running",
    summary: "正在运行 seq 游标增量追平测试 (01:38)",
    diffStat: "+86 -9",
    label: "M1 协议",
  },
  {
    id: "wks-git",
    project: "Soxu",
    branch: "feat/15-checkpoint-gc",
    isolation: "worktree",
    agent: "claude",
    model: "group/claude-opus-5.5",
    bucket: "failed",
    summary: "未读错误：TempDir 引用计数清理断言失败",
    diffStat: "+24 -3",
  },
  {
    id: "wks-main",
    project: "Soxu",
    branch: "ui/3-m0.5-pure-black-design-system",
    isolation: "local",
    agent: "agy",
    model: "gemini-4-carbon",
    bucket: "attention",
    summary: "新完成：M0.5 纯黑全端设计与令牌门禁已就绪",
    diffStat: "+2569",
    label: "设计审阅",
  },
];

export const INITIAL_APPROVALS: ApprovalItem[] = [
  {
    id: "apr-safe",
    workspaceId: "wks-m2",
    branch: "feat/12-native-drivers",
    agent: "codex · gpt-6.2-sol",
    toolKind: "PowerShell 门禁验证",
    command: "pwsh -NoProfile -File scripts/check-all.ps1",
    riskNote: "只读检查与工作区测试，不触碰 ~/.codex 或 ~/.config/magpie 全局配置。",
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
  {
    id: "apr-danger",
    workspaceId: "wks-m2",
    branch: "feat/12-native-drivers",
    agent: "codex · gpt-6.2-sol",
    toolKind: "高危删除红线拦截",
    command: "Remove-Item -Recurse -Force ./target-fixtures",
    riskNote: "命中递归删除安全红线：禁止本会话自动放行，请优先点击「拒绝并指导」改用 TempDir 自动回收。",
    highRisk: true,
    diffPreview: [],
  },
];

export const INITIAL_ASK: AskQuestionState = {
  id: "ask-1",
  prompt: "当 CLI 暂不支持热注入时，Steer 指令应默认采用哪种降级行为？",
  options: [
    { id: "opt-1", label: "柔和中断当前回合并携上下文续接", recommended: true },
    { id: "opt-2", label: "自动退回 Queue 排队等待本轮结束" },
  ],
  selectedId: "opt-1",
  customNote: "",
  submitted: false,
};

export const INITIAL_TURNS: TurnMessage[] = [
  {
    id: "t-1",
    role: "user",
    author: "你",
    text: "为 soxu-drivers 补齐热纠偏能力探测并用 TempDir 完成单测，禁止触碰真实全局目录。",
  },
  {
    id: "t-2",
    role: "assistant",
    author: "codex · group/gpt-6.2-sol",
    reasoningSec: "6.4s",
    reasoningSteps: [
      "1. 对照 ADR-002：Pi / Codex / Claude 原生支持运行中热注入；Agy / OpenCode 需走柔和中断续航。",
      "2. 确保所有测试隔离在 TempDir，避免污染本机 ~/.config/magpie。",
    ],
    tools: [
      {
        id: "tl-read",
        kind: "read",
        title: "已检索并读取 6 个核心文件 · 0.4s",
        status: "完成",
        lines: ["crates/soxu-drivers/src/lib.rs", "crates/soxu-magpie/src/lib.rs", "crates/soxu-protocol/src/lib.rs"],
      },
      {
        id: "tl-edit",
        kind: "edit",
        title: "修改 crates/soxu-drivers/src/lib.rs (+28 -3)",
        status: "已应用",
        lines: [
          "+ pub const fn supports_live_steer(self) -> bool {",
          "+   matches!(self, Self::Pi | Self::Codex | Self::Claude)",
          "+ }",
        ],
      },
      {
        id: "tl-bash",
        kind: "bash",
        title: "cargo test -p soxu-drivers · 退出码 0 · 1.8s",
        status: "6 passed",
        lines: ["$ cargo test -p soxu-drivers", "test tests::steer_capability_matrix ... ok", "test result: ok. 6 passed; 0 failed"],
      },
      {
        id: "tl-mcp",
        kind: "mcp",
        title: "aoci · aoci_maintain [Direct 工作区] · 0.3s",
        status: "五净对齐",
        lines: ["route: Direct Worktree", "composite_identity: 921fdc04", "remaining: 0"],
      },
    ],
    text: "已完成 `supports_live_steer` 能力探测矩阵，并针对 TempDir 沙箱通过了 6 项单测；现待确认全量门禁执行与清理策略。",
    checkpoint: { id: "#04", sha: "7c4e91a", stat: "2 files (+28 -3)" },
  },
];

export const CLI_OPTIONS: CliOption[] = [
  { id: "codex", bin: "codex", ver: "0.49.0", available: true },
  { id: "pi", bin: "pi", ver: "0.9.8", available: true },
  { id: "claude", bin: "claude", ver: "2.1.4", available: true },
  { id: "agy", bin: "agy", ver: "1.107.0", available: true },
  { id: "opencode", bin: "opencode", ver: "1.3.2", available: true },
  { id: "copilot", bin: "copilot", ver: "1.12.0", available: true },
  { id: "qwen", bin: "qwen", ver: "未安装", available: false },
];

export const MODEL_OPTIONS: ModelOption[] = [
  { id: "group/gpt-6.2-astra", tier: "L3 架构", note: "Magpie 路由", thinkingOptions: ["medium", "high", "xhigh"] },
  { id: "group/claude-opus-5.5", tier: "L3 架构", note: "Magpie 路由", thinkingOptions: ["medium", "high", "max"] },
  { id: "gemini-4-carbon", tier: "L3 架构", note: "仅 agy 原生", thinkingOptions: ["high", "max"] },
  { id: "group/gpt-6.2-sol", tier: "L2 主力", note: "Magpie 路由", thinkingOptions: ["low", "medium", "high"] },
  { id: "group/claude-sonnet-5.5", tier: "L2 主力", note: "Magpie 路由", thinkingOptions: ["low", "medium", "high"] },
  { id: "auto/deepseek-v4.1-flash-1017", tier: "L1 巡检", note: "Magpie 路由", thinkingOptions: ["low", "medium"] },
  { id: "auto/gemini-4-hydrogen", tier: "L1 巡检", note: "Magpie 路由", thinkingOptions: ["low", "medium"] },
];

export const INITIAL_SCRIPTS: ScriptItem[] = [
  {
    name: "daemon-dev",
    port: ":7698",
    running: true,
    lastExit: 0,
    cmd: "cargo run -p soxu-daemon",
    logs: ["Listening on 127.0.0.1:7698 (protocol v1)", "Magpie mode: magpie_managed", "Embedded mobile-web served at /"],
  },
  {
    name: "mobile-dev",
    port: ":5173",
    running: true,
    lastExit: 0,
    cmd: "pnpm --dir apps/mobile-web dev",
    logs: ["VITE v8.3.8 ready in 118 ms", "Local: http://localhost:5173/"],
  },
  {
    name: "check-all",
    running: false,
    lastExit: 0,
    cmd: "pwsh -NoProfile -File scripts/check-all.ps1",
    logs: ["[1/7] Git Emoji & Privacy check passed", "[7/7] Mobile build succeeded"],
  },
];

export const INITIAL_SCHEDULES: ScheduleItem[] = [
  {
    id: "sch-1",
    name: "每日凌晨全仓巡检",
    kind: "Schedule",
    cron: "0 2 * * *",
    human: "每天 02:00",
    target: "pi · deepseek-v4.1-flash",
    active: true,
    unreadFail: false,
  },
  {
    id: "sch-2",
    name: "PR #4 CI 与门禁守候",
    kind: "Heartbeat",
    cron: "*/15 * * * *",
    human: "每 15 分钟唤醒",
    target: "当前主控会话",
    active: true,
    unreadFail: false,
  },
  {
    id: "sch-3",
    name: "周末 AOCI 认知校准",
    kind: "Schedule",
    cron: "0 10 * * 0",
    human: "每周日 10:00",
    target: "agy · gemini-4-hydrogen",
    active: false,
    unreadFail: true,
  },
];

export const NATIVE_HISTORY: NativeSessionHistory[] = [
  { id: "his-1", cli: "codex", title: "梳理 Pure Black 无色偏设计令牌", project: "Soxu", turns: 12, updatedAt: "今天 21:40" },
  { id: "his-2", cli: "claude", title: "设计 WorkspaceIsolation 与钩子签名", project: "Soxu", turns: 8, updatedAt: "昨天 19:12" },
  { id: "his-3", cli: "pi", title: "排查 pnpm 24h release age 校验", project: "Soxu", turns: 5, updatedAt: "昨天 14:05" },
];
