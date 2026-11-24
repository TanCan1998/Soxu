# Soxu 系统架构设计

## 1. 分层拓扑

Soxu 遵循**“瘦客户端 + 强守护进程 + 纯原生生态复用”**的三层解耦架构：

1. **客户端表现层（Stateless Presentation）**：
   - `soxu-desktop`：基于 GPUI 与 `gpui-component` 构建的高帧率桌面主工作台，负责多栏工作区布局、结构化工具调用卡片渲染、内联/并排代码 Diff、快捷键编排与移动端配对引导。
   - `apps/mobile-web`：编译为极小体积静态资源并通过 `rust-embed` 打包进 `soxu-daemon` 的响应式 PWA，面向外出移动场景提供会话状态流、高危命令/工具权限一键审批、中途 `Steer` / `Queue` 指令注入与变更概览。
   - `soxu-cli`：命令行运维与脚本接入端。
2. **协议契约层（`soxu-protocol`）**：
   - 桌面端、移动端与 CLI 共用同一套带 `PROTOCOL_VERSION` 握手的 HTTP + WebSocket RPC 契约；
   - 所有会话与审批事件携带单调递增游标 `seq: u64`，支持移动设备熄屏/切网后按最后已读 `seq` 增量追平。
3. **守护进程与领域核心层（`soxu-daemon` / `soxu-core` / `soxu-git` / `soxu-drivers` / `soxu-magpie`）**：
   - `soxu-core`：维护 Host、Project、Workspace（`Local` / `Worktree`）、Session、Turn 投递策略（`Queue` / `Steer` / `Interrupt`）、`@agent` 委派图及 `RuntimeExtensionHook` 预留扩展点；
   - `soxu-drivers`：直接以子进程流式 JSONL / RPC / 受控 PTY 方式管理本机已装原生 CLI，并解析磁盘既有历史会话；
   - `soxu-magpie`：自适应感知本机 Magpie 配置与路由状态，未安装时无感切换至原生目录扫描；
   - `soxu-git`：管理独立 Git Worktree 生命周期、轮次级 Checkpoint 快照与一键 Rewind。

---

## 2. Magpie 协同与路由感知边界

`soxu-magpie` 严格遵守**只读感知 + 官方 CLI 协同 + 零破坏降级**：

- **已安装 Magpie（`MagpieManaged`）**：
  - 读取 `~/.config/magpie/` 下的非敏感结构元数据（`library.json`、`mcp-hub-servers.json`、`gateway.json`、`applied.json`），绝不触碰 `redact.key` 或原始密钥；
  - 区分 MCP 的 `shared`（经统一 `magpie` 聚合服务暴露）与 `direct`（直挂或依赖工作目录，如 `.aoci/config.json`），在新建 Git Worktree 时自动为 `direct` 工具补齐工作区软链或索引上下文；
  - 区分 Skill 的 `routed`（经 `magpie-skills` 按需检索）与 `direct`（直挂技能目录）。
- **未安装 Magpie（`StandaloneNative`）**：
  - 直接扫描系统 `PATH` 与 `~/.claude`、`~/.codex`、`~/.pi`、`~/.config/opencode`、`~/.agents/skills` 等原生路径，核心体验完全一致。
