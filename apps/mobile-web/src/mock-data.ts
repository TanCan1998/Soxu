import type {
  ApprovalItem,
  AskQuestionState,
  CliOption,
  ModelOption,
  NativeSessionHistory,
  QuickChatItem,
  ScheduleItem,
  ScriptItem,
  TurnMessage,
  WorkspaceItem,
} from "./types.ts";

export const INITIAL_WORKSPACES: WorkspaceItem[] = [
  {
    id: "wks-main",
    project: "Soxu",
    projectPath: "~/projects/Soxu",
    branch: "main",
    isolation: "local",
    agent: "agy",
    model: "gemini-4-carbon",
    bucket: "attention",
    summary: "主工作目录：纯黑设计系统与令牌门禁已对齐",
    diffStat: "+2569",
    label: "设计审阅",
  },
  {
    id: "wks-m2",
    project: "Soxu",
    projectPath: "~/projects/Soxu",
    branch: "feat/12-native-drivers",
    isolation: "worktree",
    agent: "codex",
    model: "group/gpt-6.2-sol",
    bucket: "needs_input",
    summary: "2 项权限待确认（含 1 项高危清理拦截）",
    diffStat: "+142 -18",
    label: "驱动层",
  },
  {
    id: "wks-m1",
    project: "Soxu",
    projectPath: "~/projects/Soxu",
    branch: "feat/8-ws-cursor-stream",
    isolation: "worktree",
    agent: "pi",
    model: "group/claude-sonnet-5.5",
    bucket: "running",
    summary: "正在运行增量游标追平验证 (01:38)",
    diffStat: "+86 -9",
    label: "协议层",
  },
  {
    id: "wks-git",
    project: "Soxu",
    projectPath: "~/projects/Soxu",
    branch: "feat/15-checkpoint-gc",
    isolation: "worktree",
    agent: "claude",
    model: "group/claude-opus-5.5",
    bucket: "failed",
    summary: "未读错误：临时目录引用清理断言未通过",
    diffStat: "+24 -3",
  },
  {
    id: "wks-magpie-main",
    project: "Magpie",
    projectPath: "~/projects/Magpie",
    branch: "main",
    isolation: "local",
    agent: "codex",
    model: "group/gpt-6.2-astra",
    bucket: "done",
    summary: "模型路由器网关稳定运行中",
    diffStat: "+0 -0",
  },
  {
    id: "wks-magpie-fix",
    project: "Magpie",
    projectPath: "~/projects/Magpie",
    branch: "fix/router-timeout",
    isolation: "worktree",
    agent: "claude",
    model: "group/claude-sonnet-5.5",
    bucket: "attention",
    summary: "已修复上游网关超时重试退避系数",
    diffStat: "+36 -11",
    label: "路由器",
  },
];

export const INITIAL_QUICK_CHATS: QuickChatItem[] = [
  {
    id: "chat-arch",
    title: "极简界面排版与多端组件对齐方案",
    agent: "agy",
    model: "gemini-4-carbon",
    bucket: "attention",
    updatedAt: "今天 07:35",
    messages: [
      {
        id: "qc-1",
        role: "user",
        author: "你",
        text: "请对比目前多代理终端工作台的界面结构，如何做到无绑定项目也能随时发起对话与技术方案推演？",
      },
      {
        id: "qc-2",
        role: "assistant",
        author: "agy · gemini-4-carbon",
        reasoningSec: "2.4s",
        reasoningSteps: [
          "1. 区分无项目快速对话与绑定本地目录的工作树会话，避免强制用户先建仓库。",
          "2. 无项目会话保留完整的模型、思考与权限配置，并在归档时独立收纳。",
        ],
        text: "无项目快速对话可直接复用当前配置的代理与路由器上下文。你随时可以将思考成熟的方案一键绑定至工作树执行代码落地。",
      },
    ],
  },
  {
    id: "chat-rust",
    title: "Rust 异步状态机并发安全排查思路",
    agent: "claude",
    model: "group/claude-opus-5.5",
    bucket: "done",
    updatedAt: "昨天 22:18",
    messages: [
      {
        id: "qc-3",
        role: "user",
        author: "你",
        text: "在零锁异步事件循环中，如何确保序列游标增量单调不回退？",
      },
      {
        id: "qc-4",
        role: "assistant",
        author: "claude · group/claude-opus-5.5",
        text: "建议使用单调递增序列号配合比较交换原子提交，断线重连后只接受严格大于本地游标的增量快照。",
      },
    ],
  },
  {
    id: "chat-archived-demo",
    title: "早期原型色彩对比度验算记录",
    agent: "pi",
    model: "group/gpt-6.2-sol",
    bucket: "done",
    updatedAt: "3 天前",
    archived: true,
    messages: [
      {
        id: "qc-5",
        role: "user",
        author: "你",
        text: "已确认纯黑画布七级灰阶满足文本高对比度要求，归档备查。",
      },
    ],
  },
];

export const INITIAL_APPROVALS: ApprovalItem[] = [
  {
    id: "apr-safe",
    workspaceId: "wks-m2",
    branch: "feat/12-native-drivers",
    agent: "codex · group/gpt-6.2-sol",
    toolKind: "全仓门禁检查指令",
    command: "pwsh -NoProfile -File scripts/check-all.ps1",
    riskNote: "仅执行只读规范校验与隔离测试，不修改全局用户配置目录。",
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
    agent: "codex · group/gpt-6.2-sol",
    toolKind: "高危删除红线拦截",
    command: "Remove-Item -Recurse -Force ./target-fixtures",
    riskNote: "命中递归删除安全红线：禁止本会话自动放行，请优先选择拒绝并要求改用临时隔离目录自动回收。",
    highRisk: true,
    diffPreview: [],
  },
];

export const INITIAL_ASK: AskQuestionState = {
  id: "ask-1",
  prompt: "当代理驱动暂不支持运行中热注入时，纠偏指令应默认采用哪种降级策略？",
  options: [
    { id: "opt-1", label: "柔和中断当前回合并携上下文自动续接", recommended: true },
    { id: "opt-2", label: "自动退回排队模式等待本轮输出结束" },
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
    text: "为驱动适配模块补齐热纠偏能力探测并使用临时沙箱完成单元测试，禁止触碰真实全局目录。",
  },
  {
    id: "t-2",
    role: "assistant",
    author: "codex · group/gpt-6.2-sol",
    reasoningSec: "6.4 秒",
    reasoningSteps: [
      "1. 对照架构决策：Pi / Codex / Claude 原生支持运行中热注入；Agy 与 OpenCode 采用柔和中断续接。",
      "2. 确保全部测试夹具隔离在临时目录内，严禁改动本机真实全局配置文件。",
    ],
    tools: [
      {
        id: "tl-read",
        kind: "read",
        title: "已检索并读取 6 个核心源码文件 · 0.4 秒",
        status: "已完成",
        lines: ["crates/soxu-drivers/src/lib.rs", "crates/soxu-magpie/src/lib.rs", "crates/soxu-protocol/src/lib.rs"],
      },
      {
        id: "tl-edit",
        kind: "edit",
        title: "修改 crates/soxu-drivers/src/lib.rs (+5 -1)",
        status: "已应用",
        lines: [
          "  pub enum NativeCliKind { Claude, Codex, Pi, OpenCode, Antigravity, Copilot, Qwen }",
          "- // 旧版仅在运行时返回可执行文件名",
          "+ /// 返回驱动是否支持活跃回合中途热纠偏",
          "+ #[inline]",
          "+ pub const fn supports_live_steer(self) -> bool {",
          "+   matches!(self, Self::Pi | Self::Codex | Self::Claude)",
          "+ }",
        ],
      },
      {
        id: "tl-bash",
        kind: "bash",
        title: "cargo test -p soxu-drivers · 退出码 0 · 1.8 秒",
        status: "6 项通过",
        lines: ["$ cargo test -p soxu-drivers", "test tests::steer_capability_matrix ... ok", "test result: ok. 6 passed; 0 failed"],
      },
      {
        id: "tl-mcp",
        kind: "mcp",
        title: "aoci · 索引维护 [当前工作树] · 0.3 秒",
        status: "零漂移对齐",
        lines: ["工作区模式: Direct Worktree", "索引指纹: 921fdc04", "待处理条目: 0"],
      },
    ],
    text: "已完成热纠偏能力探测矩阵重构，并在临时沙箱环境下通过全部 6 项单元测试；当前正在等待全套门禁命令与清理策略审批。",
    checkpoint: { id: "#04", sha: "7c4e91a", stat: "2 个文件 (+28 -3)" },
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
  { id: "group/gpt-6.2-astra", tier: "复杂架构", note: "协同路由", thinkingOptions: ["medium", "high", "xhigh"] },
  { id: "group/claude-opus-5.5", tier: "复杂架构", note: "协同路由", thinkingOptions: ["medium", "high", "max"] },
  { id: "gemini-4-carbon", tier: "复杂架构", note: "原生直连", thinkingOptions: ["high", "max"] },
  { id: "group/gpt-6.2-sol", tier: "主力开发", note: "协同路由", thinkingOptions: ["low", "medium", "high"] },
  { id: "group/claude-sonnet-5.5", tier: "主力开发", note: "协同路由", thinkingOptions: ["low", "medium", "high"] },
  { id: "auto/deepseek-v4.1-flash-1017", tier: "轻量巡检", note: "协同路由", thinkingOptions: ["low", "medium"] },
  { id: "auto/gemini-4-hydrogen", tier: "轻量巡检", note: "协同路由", thinkingOptions: ["low", "medium"] },
];

export const INITIAL_SCRIPTS: ScriptItem[] = [
  {
    name: "daemon-dev",
    port: ":7698",
    running: true,
    lastExit: 0,
    cmd: "cargo run -p soxu-daemon",
    logs: ["正在监听本地端口 127.0.0.1:7698 (协议版本 1)", "路由器状态: 协同托管模式", "移动遥控客户端已就绪"],
  },
  {
    name: "mobile-dev",
    port: ":5173",
    running: true,
    lastExit: 0,
    cmd: "pnpm --dir apps/mobile-web dev",
    logs: ["本地开发服务已就绪 (118 毫秒)", "访问地址: http://localhost:5173/"],
  },
  {
    name: "check-all",
    running: false,
    lastExit: 0,
    cmd: "pwsh -NoProfile -File scripts/check-all.ps1",
    logs: ["[1/7] 提交规范与隐私门禁通过", "[7/7] 移动与预览产物构建完成"],
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
    name: "合并请求持续守候",
    kind: "Heartbeat",
    cron: "*/15 * * * *",
    human: "每 15 分钟唤醒",
    target: "当前主控会话",
    active: true,
    unreadFail: false,
  },
  {
    id: "sch-3",
    name: "周末认知索引校准",
    kind: "Schedule",
    cron: "0 10 * * 0",
    human: "每周日 10:00",
    target: "agy · gemini-4-hydrogen",
    active: false,
    unreadFail: true,
  },
];

export const NATIVE_HISTORY: NativeSessionHistory[] = [
  { id: "his-1", cli: "codex", title: "梳理纯黑无色偏设计令牌", project: "Soxu", turns: 12, updatedAt: "今天 21:40" },
  { id: "his-2", cli: "claude", title: "设计工作树隔离与钩子签名", project: "Soxu", turns: 8, updatedAt: "昨天 19:12" },
  { id: "his-3", cli: "pi", title: "排查依赖最小发布期校验", project: "Soxu", turns: 5, updatedAt: "昨天 14:05" },
];
