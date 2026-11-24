# Soxu 架构与工程决策日志 (ADR / DECISIONS)

本文件记录所有已拍板的关键技术与产品决策，避免后续会话或子 Agent 重复讨论或偏离既定方向。

---

## ADR-001：采用 GPUI 桌面端 + Rust 独立 Daemon + 内嵌响应式移动 Web/PWA（方案 1）
- **状态**：已采纳（2026-11-24）
- **背景**：桌面端需要承载多项目、多 Agent 高频并发流式输出、长上下文虚拟列表与实时代码 Diff；移动端主要用于远程查看状态、权限审批与中途发令（Steer/Queue）。
- **决策**：核心逻辑下沉至纯 Rust `soxu-daemon` 与版本化 `soxu-protocol`；桌面端使用 **GPUI + `gpui-component`** 获取极致原生性能；移动端采用 **TypeScript + Preact + Tailwind CSS v4** 构建超轻量响应式 PWA，经 `rust-embed` 内嵌进 `soxu-daemon` 单一可执行文件直接提供服务。

## ADR-002：100% 复用本机已装原生 Agent CLI，暂不引入 ACP
- **状态**：已采纳（2026-11-24）
- **决策**：`soxu-drivers` 直接驱动用户本机已安装并登录认证的原生 CLI（`claude`、`codex`、`pi`、`opencode`、`agy`、`copilot`、`qwen` 等）的原生流式/非交互接口与受控 PTY，并支持导入各 CLI 本地历史会话；当前阶段不开发 ACP 协议层。

## ADR-003：Magpie 深度自动发现与未安装环境零依赖降级
- **状态**：已采纳（2026-11-24）
- **决策**：`soxu-magpie` 启动时自动检测 `~/.config/magpie` 与 `magpie` CLI。若已安装，无损读取其模型网关路由、`shared`/`direct` MCP Router 及 `routed`/`direct` Skill Router 状态，并在新建 Git Worktree 时自动为 `direct` 类目录敏感型 MCP（如 AOCI、CodeGraph）补齐工作区上下文，严禁暴力覆写全局 CLI 配置文件；若未安装 Magpie，自动降级为 `StandaloneNative` 原生目录扫描，全部功能正常可用。

## ADR-004：插件系统前期仅预留扩展槽位
- **状态**：已采纳（2026-11-24）
- **决策**：前期聚焦核心多 Agent 编排、Git Worktree 隔离、桌面与移动协同体验，不建设外部插件运行时或市场；在 `soxu-core` 与协议层预留 `RuntimeExtensionHook` 等零开销扩展接口，确保后续演进无需重构底座。

## ADR-005：UI/UX 设计系统严格前置（M0.5）与双端令牌同源
- **状态**：已采纳（2026-11-24）
- **决策**：在实际功能编码开始前，先行完成 `design-system/soxu/MASTER.md`、DTCG `tokens.json`、`tokens.css` 与分页面交互规范；GPUI 桌面主题与移动端 CSS 变量同源消费同一套设计令牌。

## ADR-006：全线采用最新稳定版工具链与 Git Emoji + 隐私拦截双重门禁
- **状态**：已采纳（2026-11-24）
- **决策**：默认采用 Rust `1.99+`（`edition = "2024"`, `resolver = "3"`）、Node `26`、`pnpm 12`、`gpui-component v0.11+` 与 `actions/checkout@v7` 等最新稳定版；所有提交强制执行 Git Emoji 规范与 `gitleaks` + 个人路径隐私扫描（本地 `.githooks` + 云端 Actions 双重拦截）。
