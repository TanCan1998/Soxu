/**
 * 桌面单文件预览（desktop.html → design-system/soxu/preview.html）专用演示数据。
 *
 * 仅被 `DesktopWorkbench` 设计预览消费；移动端真实客户端（main.tsx）已全面
 * 切换至 Daemon WS 真实链路（`lib/daemon-*`），严禁从本文件导入数据。
 */

import type {
  ApprovalItem,
  ArtifactItem,
  AskQuestionState,
  CliOption,
  DiffFileItem,
  LiveStepItem,
  ModelOption,
  NativeSessionHistory,
  QuickChatItem,
  ScheduleItem,
  ScriptItem,
  TurnMessage,
  WorkspaceItem,
} from "@/types";

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
    pinned: true,
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
    activeIntent: "等待确认门禁指令与临时夹具清理边界",
    diffStat: "+142 -18",
    label: "驱动层",
    pinned: true,
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
    activeIntent: "cargo test -p soxu-daemon --test rpc_ws_e2e",
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
        title: "已检索并读取 6 个核心源码文件",
        intent: "读取驱动能力探测与协议边界源码",
        metric: "6 个文件 · 0.4s",
        status: "已完成",
        lines: ["crates/soxu-drivers/src/lib.rs", "crates/soxu-magpie/src/lib.rs", "crates/soxu-protocol/src/lib.rs"],
      },
      {
        id: "tl-edit",
        kind: "edit",
        title: "修改 crates/soxu-drivers/src/lib.rs",
        intent: "为 NativeCliKind 补齐运行中热纠偏能力探测",
        diffAdd: 5,
        diffDel: 1,
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
        title: "cargo test -p soxu-drivers",
        intent: "在临时沙箱目录中验证热纠偏与降级矩阵",
        metric: "6 项通过 · 1.8s",
        pseudoLines: [
          "with TempFixtureDir() as dir:",
          "  run('cargo test -p soxu-drivers --locked')",
          "> assert steer_capability_matrix.passed == 6  # 1.8s",
          "  assert global_user_configs.untouched()",
        ],
        status: "6 项通过",
        lines: ["$ cargo test -p soxu-drivers", "test tests::steer_capability_matrix ... ok", "test result: ok. 6 passed; 0 failed"],
      },
      {
        id: "tl-mcp",
        kind: "mcp",
        title: "aoci · 索引维护 [当前工作树]",
        intent: "检查并保持分支认知条目与代码摘要同步",
        metric: "零漂移 · 0.3s",
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

export const INITIAL_DIFF_FILES: DiffFileItem[] = [
  {
    path: "crates/soxu-drivers/src/lib.rs",
    added: 5,
    deleted: 1,
    status: "M",
    staged: true,
    rows: [
      { lineNum: 22, kind: "ctx", code: "pub enum NativeCliKind { Claude, Codex, Pi, OpenCode, Antigravity, Copilot, Qwen }" },
      { kind: "del", code: "// 旧版仅在运行时返回可执行文件名" },
      { lineNum: 23, kind: "add", code: "/// 返回驱动是否支持活跃回合中途热纠偏" },
      { lineNum: 24, kind: "add", code: "#[inline]" },
      { lineNum: 25, kind: "add", code: "pub const fn supports_live_steer(self) -> bool {" },
      { lineNum: 26, kind: "add", code: "  matches!(self, Self::Pi | Self::Codex | Self::Claude)" },
      { lineNum: 27, kind: "add", code: "}" },
    ],
  },
  {
    path: "crates/soxu-drivers/src/steer_fallback.rs",
    added: 18,
    deleted: 3,
    status: "M",
    staged: true,
    rows: [
      { lineNum: 41, kind: "ctx", code: "pub fn dispatch_active_turn(kind: NativeCliKind, msg: &str) -> DeliveryOutcome {" },
      { kind: "del", code: "  return DeliveryOutcome::RejectedBusy;" },
      { lineNum: 42, kind: "add", code: "  if kind.supports_live_steer() {" },
      { lineNum: 43, kind: "add", code: "    DeliveryOutcome::InjectedLive" },
      { lineNum: 44, kind: "add", code: "  } else {" },
      { lineNum: 45, kind: "add", code: "    DeliveryOutcome::SoftInterruptResume" },
      { lineNum: 46, kind: "add", code: "  }" },
      { lineNum: 47, kind: "ctx", code: "}" },
    ],
  },
  {
    path: "crates/soxu-drivers/tests/steer_matrix.rs",
    added: 24,
    deleted: 0,
    status: "A",
    staged: false,
    rows: [
      { lineNum: 1, kind: "add", code: "#[test]" },
      { lineNum: 2, kind: "add", code: "fn live_steer_and_soft_fallback_stay_isolated() {" },
      { lineNum: 3, kind: "add", code: "  let fixture = tempfile::tempdir().expect(\"temp fixture\");" },
      { lineNum: 4, kind: "add", code: "  assert!(NativeCliKind::Pi.supports_live_steer());" },
      { lineNum: 5, kind: "add", code: "}" },
    ],
  },
];

export const INITIAL_LIVE_STEPS: LiveStepItem[] = [
  { id: "st-1", title: "探测 7 大原生 CLI 热纠偏能力矩阵", status: "done" },
  { id: "st-2", title: "在隔离临时夹具运行驱动层单测 (6/6)", status: "done" },
  { id: "st-3", title: "等待人工确认安全门禁与目录清理边界", status: "in_progress" },
  { id: "st-4", title: "生成 Git Emoji 提交预览并更新认知索引", status: "pending" },
];

export const INITIAL_ARTIFACT: ArtifactItem = {
  id: "art-plan-steer",
  title: "原生驱动热纠偏降级与沙箱隔离实施方案",
  kind: "plan",
  path: "docs/plans/native-driver-steer-fallback.md",
  summary: "已覆盖 3 个支持热注入驱动与 4 个柔和中断续接驱动，全部夹具隔离在 TempDir。",
  status: "pending_approval",
  content: [
    "# 原生驱动热纠偏与降级边界方案",
    "",
    "1. 热注入优先路径：Pi (RPC) / Codex / Claude 在下一工具边界无损注入 steer。",
    "2. 降级续接路径：Agy / OpenCode / Copilot / Qwen 触发柔和中断并保留未消费草稿。",
    "3. 安全红线：严禁写入 ~/.config/magpie 等用户真实全局配置，夹具统一用 tempfile。",
    "4. 验收命令：pwsh -NoProfile -File scripts/check-all.ps1。",
  ],
};

export const MOBILE_QUICK_CHIPS = {
  running: [
    { id: "qc-check", label: "先跑 check-all 别急着提交", mode: "steer" as const },
    { id: "qc-stop-risk", label: "遇高危改动先暂停汇报", mode: "steer" as const },
    { id: "qc-aoci", label: "收尾对齐本地认知索引", mode: "queue" as const },
  ],
  idle: [
    { id: "qc-summary", label: "总结本轮改动与潜在风险", mode: "steer" as const },
    { id: "qc-tests", label: "运行全工作区增量门禁", mode: "steer" as const },
    { id: "qc-commit", label: "生成规范提交标题预览", mode: "steer" as const },
  ],
};
