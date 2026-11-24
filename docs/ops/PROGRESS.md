# Soxu 研发进度与跨会话交接看板 (PROGRESS)

> **新对话接手指引**：请先阅读根目录 [`AGENTS.md`](../../AGENTS.md)，再结合下方「当前里程碑状态」与 `gh issue list` / `gh pr list` 即可在 30 秒内无损恢复上下文。

---

## 1. 当前总体状态

- **当前所处阶段**：`M0: Bootstrap & Engineering Governance` 已完成；**`M0.5: UI/UX Design System & Prototypes` 已按指令暂停**（初版留存于 `draft/ui-v0-initial` 草稿分支，待参考项目完成 `AOCI` + `CodeGraph` 索引后，对标 **ChatGPT 纯黑色主题** 深化细节设计）。
- **参考项目与索引进度（已全部完成）**：
  - [x] 拉取参考项目至 `.refs/`：`paseo`（7,818 文件）、`codeg`（3,324 文件）、`padu`（1,157 文件）
  - [x] **AOCI 认知索引已全部原子入库**：`padu`（方案 A 全量 **1,086** 条）、`paseo`（方案 B 宏观核心 **202** 条）、`codeg`（方案 B 宏观核心 **177** 条）、`Soxu` 主仓（全量 **55+** 条）
  - [x] **CodeGraph 符号图已全部构建（`--local-only`）**：`padu`（17,171 节点 / 61,175 边）、`paseo`（148,292 节点 / 427,706 边）、`codeg`（84,541 节点 / 336,448 边）、`Soxu`（87 节点 / 129 边）

---

## 2. 里程碑全景路线图

| 里程碑 | 名称 | 状态 | 核心交付物 |
| :--- | :--- | :--- | :--- |
| **M0** | Bootstrap & Engineering Governance | ✅ 已完成 | 仓库骨架（Rust 2024 + PWA）、`.githooks` + `gitleaks` 隐私与 Emoji 门禁、最新 Actions CI、三层文档、Paseo `git` 项目绑定 |
| **M0.5** | UI/UX Design System & Prototypes | 🚧 评审中（PR #4 已深度强化） | 纯黑全令牌 tokens.json（含语法高亮/ANSI 16色/Sizing）、MASTER + 6 份像素规范、桌面全景游乐场（5视角+5模态）、模块化移动 PWA（25.6KB gzip） |
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

1. 完成远端旧仓库删除、绑定私有仓库 `TanCan1998/Soxu` 并升级 Paseo `git` 项目注册。
2. 按用户拍板的模型并行完成 `.refs/{paseo,codeg,padu}` 与 `Soxu` 的 `AOCI` 初始化，随后执行 `CodeGraph` 本地符号图构建。
3. 基于已索引的三大参考项目交互细节与 ChatGPT 纯黑色（Pure Black）视觉体系，重启并完成 `M0.5` 高细节 UI/UX 设计。

