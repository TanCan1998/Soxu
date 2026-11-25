# Soxu（索序）

**原生高性能多 Agent 协同桌面工作台与移动远程控制中心** —— 基于 **Rust + GPUI** 构建，零侵入复用本机已装 AI 编码 CLI，深度协同 **Magpie** 模型与技能/MCP 路由。

---

## 核心特性定位

- **极致轻量与流畅（Rust + GPUI）**：告别臃肿套壳，桌面端采用 GPUI 与 `gpui-component` 原生 GPU 渲染，轻松承载多项目、多 Agent 高频流式输出与实时 Diff 审阅。
- **纯原生 CLI 零侵入复用**：直接聚合调度本机已安装认证的 `claude`、`codex`、`pi`、`opencode`、`agy`、`copilot`、`qwen` 等原生 CLI（含本地历史会话检索导入，无需 ACP 转换层）。
- **Magpie 智能感知 + 独立可用**：
  - **已装 Magpie**：自动发现模型网关路由组、Skill Router（`routed`/`direct`）与 MCP Router（`shared`/`direct`），新建 Worktree 自动承接目录敏感型 MCP；
  - **未装 Magpie**：自动降级扫描各 CLI 原生配置目录，开箱即用。
- **守护进程分离与移动端随时控场**：独立常驻 `soxu-daemon` 通过带游标的 WebSocket RPC 协议同步状态，内置超轻量响应式 Web/PWA 遥控台，手机扫码或经 Tailscale 即可在外出时看进度、批权限、中途纠偏（`Queue` / `Steer` / `Interrupt`）。
- **安全级 Git Worktree 与对话检查点**：任务级 Git Worktree 物理隔离，支持与对话轮次绑定的 Checkpoint 快照及一键 Rewind 回退。

---

## 仓库目录结构

```text
Soxu/
├── AGENTS.md                 # AI Agent 协同研发宪法与安全红线（最高优先级）
├── Cargo.toml                # Rust 2024 Workspace 根配置
├── mise.toml                 # 工具链版本锁定与跨 Worktree 共享构建缓存
├── paseo.json                # Paseo 托管工作区脚本定义
├── design-system/soxu/       # 全端统一 UI/UX 设计系统与 DTCG 令牌 (M0.5)
├── crates/
│   ├── soxu-protocol/        # 版本化 RPC 与增量事件流契约
│   ├── soxu-core/            # 领域状态机、编排原语与插件扩展预留槽位
│   ├── soxu-magpie/          # Magpie 自动发现与未安装环境原生降级引擎
│   ├── soxu-drivers/         # 本机原生 Agent CLI 流式进程驱动层
│   ├── soxu-git/             # Git Worktree 池、Checkpoint/Rewind 与实时 Diff
│   ├── soxu-daemon/          # 独立守护进程（含内嵌移动端 PWA 静态伺服）
│   ├── soxu-desktop/         # GPUI + gpui-component 桌面端工作台
│   └── soxu-cli/             # soxu 命令行控制与环境诊断工具
├── apps/
│   └── mobile-web/           # 响应式移动端 Web/PWA 遥控端 (React 19 + shadcn/ui + TS + Tailwind v4)
├── scripts/                  # 本地 Git Hooks、隐私防泄露扫描与全量门禁脚本
└── docs/
    ├── ops/                  # 实时研发进度看板 (PROGRESS.md) 与架构决策 (DECISIONS.md)
    ├── dev/                  # 开发者架构说明与多 Agent 并行贡献指南
    └── user/                 # 最终用户快速上手与移动端遥控指南
```

---

## 文档导航

- **研发与 AI 协作入口**：[AGENTS.md](./AGENTS.md) · [研发进度看板](./docs/ops/PROGRESS.md) · [架构决策记录](./docs/ops/DECISIONS.md)
- **开发者文档**：[系统架构设计](./docs/dev/architecture.md) · [本地构建与并行贡献指南](./docs/dev/contributing.md)
- **用户文档**：[快速上手与 Magpie 协同指南](./docs/user/getting-started.md) · [移动端远程查看与控制指南](./docs/user/mobile-remote.md)
