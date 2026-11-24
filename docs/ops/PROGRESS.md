# Soxu 研发进度与跨会话交接看板 (PROGRESS)

> **新对话接手指引**：请先阅读根目录 [`AGENTS.md`](../../AGENTS.md)，再结合下方「当前里程碑状态」与 `gh issue list` / `gh pr list` 即可在 30 秒内无损恢复上下文。

---

## 1. 当前总体状态

- **当前所处阶段**：`M0: Bootstrap & Engineering Governance` 已完成 → 正在推进 **`M0.5: UI/UX Design System & Prototypes`（编码前强制前置阶段）**
- **受控挂起项（严禁未获口令擅自启动）**：
  - [ ] **参考项目拉取与索引**：将 `getpaseo/paseo`、`spacering-net/codeg`、`wisnuwiry/padu` 拉取至 `.refs/` 并执行 `AOCI` + `CodeGraph` 初始化——**状态：等待用户明确下达口令后再执行**。

---

## 2. 里程碑全景路线图

| 里程碑 | 名称 | 状态 | 核心交付物 |
| :--- | :--- | :--- | :--- |
| **M0** | Bootstrap & Engineering Governance | ✅ 已完成 | 仓库骨架（Rust 2024 + PWA）、`.githooks` + `gitleaks` 隐私与 Emoji 门禁、最新 Actions CI、三层文档、Paseo `git` 项目绑定 |
| **M0.5** | UI/UX Design System & Prototypes | 🔄 进行中 | `design-system/soxu/MASTER.md`、DTCG `tokens.json`、`tokens.css`、`preview.html` 及桌面/移动四大核心页面交互规范 |
| **M1** | Protocol, Core State & Daemon Skeleton | ⏳ 待启动 | `soxu-protocol` 完整 RPC/WS 契约、`soxu-core` 编排模型、`soxu-daemon` 事件总线与 `soxu-cli` 控制闭环 |
| **M2** | Native CLI Drivers & Magpie Discovery | ⏳ 待启动 | `soxu-drivers`（Claude/Codex/Pi/OpenCode/Agy 原生流式复用与本地历史会话导入）+ `soxu-magpie`（双态自动发现与降级） |
| **M3** | Git Worktree, Checkpoints & Supervision | ⏳ 待启动 | `soxu-git`（Worktree 池、对话轮次 Checkpoint & Rewind、实时 Diff）+ Workspace Scripts & 审批队列 |
| **M4** | GPUI Desktop Workbench | ⏳ 待启动 | `soxu-desktop`（基于 `gpui-component v0.11+` 实现多栏工作台、结构化工具卡片、`@agent` 移交、扫码配对） |
| **M5** | Mobile Web/PWA Remote & Automation | ⏳ 待启动 | `apps/mobile-web` 遥控审批与 Steer 完整实现、`rust-embed` 内嵌伺服、Schedules & Heartbeats 定时任务 |

---

## 3. 活跃工作区与智能调度策略

- **当前并发上限**：`1`（处于 M0 / M0.5 顶层规范制定阶段，串行保证零冲突；进入 M1~M5 接口冻结后的正交模块实现时上调至 `2~3`）。
- **主力 Agent 与模型路由池**（通过 Paseo 调度，经 Magpie 网关路由）：
  - **L3 架构/并发攻坚**：`codex` / `pi` / `agy` · `group/gpt-6.2-astra` / `group/claude-opus-5.5` / `gemini-4-carbon` (`high`)
  - **L2 核心模块/UI 实现**：`codex` / `pi` / `agy` · `group/gpt-6.2-sol` / `group/claude-sonnet-5.5` / `auto/gemini-4-hydrogen` (`medium`)
  - **L1 脚手架/文档巡检**：`pi` / `agy` · `auto/deepseek-v4.1-flash-1017` / `auto/gemini-4-hydrogen` (`low`)

---

## 4. 下一步紧接行动队列（Next Actions）

1. 完成 `M0.5` 全端 UI/UX 设计系统（`design-system/soxu/`）并通过标准 PR 流程合入 `main`，交由用户审阅视觉令牌与布局原型。
2. 等待用户审阅 UI/UX 设计，并在收到用户口令后执行 `.refs/{paseo,codeg,padu}` 参考仓库拉取及 `AOCI` + `CodeGraph` 本地索引初始化。
3. 按 GitHub Milestones 拆分 M1 精细粒度任务 Issue（单任务 10~25 分钟可闭环），启动 M1 研发。
