# Soxu 研发进度与跨会话交接看板 (PROGRESS)

> **新对话接手指引**：请先阅读根目录 [`AGENTS.md`](../../AGENTS.md)，再结合下方「当前里程碑状态」与 `gh issue list` / `gh pr list` 即可在 30 秒内无损恢复上下文。

---

## 1. 当前总体状态

- **当前所处阶段**：`M0: Bootstrap & Engineering Governance` 已完成；**`M0.5: UI/UX Design System & Prototypes` 已完成 shadcn/ui + Tailwind CSS v4 深度架构重构并提交 PR `#4` 待评审**（严格遵循 ChatGPT 纯黑无色偏主题、`lucide-react` 统一开源图标、`HoverExpandDock` 极简悬停收纳、纯图标径向主题切换、7 分区独立设置界面 `Ctrl+,`、SIL OFL 1.1 双开源字体独立/跟随联动，以及 GPUI 1:1 可移植动效规范）。
- **参考项目与索引进度（已全部完成）**：
  - [x] 拉取参考项目至 `.refs/`：`paseo`（7,818 文件）、`codeg`（3,324 文件）、`padu`（1,157 文件）
  - [x] **AOCI 认知索引已全部原子入库**：`padu`（方案 A 全量 **1,086** 条）、`paseo`（方案 B 宏观核心 **202** 条）、`codeg`（方案 B 宏观核心 **177** 条）、`Soxu` 主仓（全量对齐）
  - [x] **CodeGraph 符号图已全部构建（`--local-only`）**：`padu`（17,171 节点 / 61,175 边）、`paseo`（148,292 节点 / 427,706 边）、`codeg`（84,541 节点 / 336,448 边）、`Soxu`（87 节点 / 129 边）

---

## 2. 里程碑全景路线图

| 里程碑 | 名称 | 状态 | 核心交付物 |
| :--- | :--- | :--- | :--- |
| **M0** | Bootstrap & Engineering Governance | ✅ 已完成 | 仓库骨架（Rust 2024 + PWA）、`.githooks` + `gitleaks` 隐私与 Emoji 门禁、最新 Actions CI、三层文档、Paseo `git` 项目绑定 |
| **M0.5** | UI/UX Design System & Prototypes | 🚧 评审中（PR #4 已升级为 shadcn/ui） | 纯黑双态 tokens.json + shadcn/ui 变量、MASTER + 7 份像素规范、React 19 + shadcn/ui + Tailwind v4 双端组件与 7 分区独立设置（开源双字体联动 + GPUI 动效）、单文件桌面预览 `preview.html`、移动遥控 PWA |
| **M1** | Protocol, Core State & Daemon Skeleton | ⏳ 待启动 | `soxu-protocol` 完整 RPC/WS 契约、`soxu-core` 编排模型、`soxu-daemon` 事件总线与 `soxu-cli` 控制闭环 |
| **M2** | Native CLI Drivers & Magpie Discovery | ⏳ 待启动 | `soxu-drivers`（Claude/Codex/Pi/OpenCode/Agy 原生流式复用与本地历史会话导入）+ `soxu-magpie`（双态自动发现与降级） |
| **M3** | Git Worktree, Checkpoints & Supervision | ⏳ 待启动 | `soxu-git`（Worktree 池、对话轮次 Checkpoint & Rewind、实时 Diff）+ Workspace Scripts & 审批队列 |
| **M4** | GPUI Desktop Workbench | ⏳ 待启动 | `soxu-desktop`（基于 `gpui-component v0.11+` 实现多栏工作台、结构化工具卡片、`@agent` 移交、扫码配对） |
| **M5** | Mobile Web/PWA Remote & Automation | ⏳ 待启动 | `apps/mobile-web` 遥控审批与 Steer 完整实现、`rust-embed` 内嵌伺服、Schedules & Heartbeats 定时任务 |

---

## 3. 活跃工作区与智能调度策略

- **当前并发上限**：参考库索引阶段采用 `3~4` 路独立仓库并行初始化；后续进入 M0.5 顶层规范时回归 `1` 路串行，M1~M5 接口冻结后的正交模块实现采用 `2~3` 路 Paseo Worktree 并行。
- **主力 Agent 与模型路由池**（通过 Paseo 调度，经 Magpie 网关路由）：
  - **L3 架构/并发攻坚**：`codex` / `pi` / `agy` · `group/gpt-6.2-astra` / `group/claude-opus-5.5` / `gemini-4-carbon` (`high`)
  - **L2 核心模块/UI 实现**：`codex` / `pi` / `agy` · `group/gpt-6.2-sol` / `group/claude-sonnet-5.5` / `auto/gemini-4-hydrogen` (`medium`)
  - **L1 脚手架/文档巡检**：`pi` / `agy` · `auto/deepseek-v4.1-flash-1017` / `auto/gemini-4-hydrogen` (`low`)

---

## 4. 下一步紧接行动队列（Next Actions）

1. 等待用户评审 `M0.5`（PR `#4`：`design-system/soxu/preview.html` 与 `apps/mobile-web/`），确认后合并 PR `#4` 至 `main` 并关闭 Issue `#3`。
2. 在 `main` 分支按「计划先行 -> Issue -> 隔离 Worktree」流程正式启动 **M1**（`soxu-protocol` RPC/WS 契约、`soxu-core` 状态机、`soxu-daemon` 与 `soxu-cli` 控制闭环）。

