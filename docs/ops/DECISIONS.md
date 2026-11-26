# Soxu 架构与工程决策日志 (ADR / DECISIONS)

本文件记录所有已拍板的关键技术与产品决策，避免后续会话或子 Agent 重复讨论或偏离既定方向。

---

## ADR-001：采用 GPUI 桌面端 + Rust 独立 Daemon + 内嵌响应式移动 Web/PWA（方案 1）
- **状态**：已采纳并修订（2026-11-25）
- **背景**：桌面端需要承载多项目、多 Agent 高频并发流式输出、长上下文虚拟列表与实时代码 Diff；移动端主要用于远程查看状态、权限审批与中途发令（Steer/Queue），同时需要一套标准组件库确保高保真交互原型与真实客户端无障碍对齐。
- **决策**：核心逻辑下沉至纯 Rust `soxu-daemon` 与版本化 `soxu-protocol`；桌面端使用 **GPUI + `gpui-component`（shadcn 风格同源）** 获取极致原生性能；移动端与桌面交互设计原型采用 **TypeScript + React 19 + `shadcn/ui`（`radix-ui`）+ Tailwind CSS v4 + `lucide-react`** 构建响应式 PWA，经 `rust-embed` 内嵌进 `soxu-daemon` 单一可执行文件直接提供服务。

## ADR-002：100% 复用本机已装原生 Agent CLI，暂不引入 ACP
- **状态**：已采纳（2026-11-24）
- **决策**：`soxu-drivers` 直接驱动用户本机已安装并登录认证的原生 CLI（`claude`、`codex`、`pi`、`opencode`、`agy`、`copilot`、`qwen` 等）的原生流式/非交互接口与受控 PTY，并支持导入各 CLI 本地历史会话；当前阶段不开发 ACP 协议层。

## ADR-003：Magpie 深度自动发现与未安装环境零依赖降级
- **状态**：已采纳（2026-11-24）
- **决策**：`soxu-magpie` 启动时自动检测 `~/.config/magpie` 与 `magpie` CLI。若已安装，无损读取其模型网关路由、`shared`/`direct` MCP Router 及 `routed`/`direct` Skill Router 状态，并在新建 Git Worktree 时自动为 `direct` 类目录敏感型 MCP（如 AOCI、CodeGraph）补齐工作区上下文，严禁暴力覆写全局 CLI 配置文件；若未安装 Magpie，自动降级为 `StandaloneNative` 原生目录扫描，全部功能正常可用。

## ADR-004：插件系统前期仅预留扩展槽位
- **状态**：已采纳（2026-11-24）
- **决策**：前期聚焦核心多 Agent 编排、Git Worktree 隔离、桌面与移动协同体验，不建设外部插件运行时或市场；在 `soxu-core` 与协议层预留 `RuntimeExtensionHook` 等零开销扩展接口，确保后续演进无需重构底座。

## ADR-005：UI/UX 设计系统严格前置（M0.5）、对标 ChatGPT 纯黑主题与双端令牌同源
- **状态**：已采纳并深度升级（2026-11-25）
- **决策**：在实际功能编码开始前基于 `Paseo`、`codeg`、`padu` 参考项目完成全端 UI/UX 设计系统：**深色主题明确对标 ChatGPT 纯黑色（Pure Black `#000000` 画布 + `#0D0D0D` / `#171717` / `#212121` 无色偏中性灰阶分层）**，映射为统一 CSS 变量（含 `shadcn/ui` 语义映射与 GPUI 常量）；前端全量使用 **`shadcn/ui` + Tailwind CSS v4 + `lucide-react` 开源图标**；实行极简收敛与悬停自动展开设计，提供独立 7 分区设置界面与开源免费双字体（界面字体 / 代码块字体独立设置或一键跟随），所有动效均提供 GPUI 1:1 可移植规范。

## ADR-006：全线采用最新稳定版工具链与 Git Emoji + 隐私拦截双重门禁
- **状态**：已采纳（2026-11-24）
- **决策**：默认采用 Rust `1.99+`（`edition = "2024"`, `resolver = "3"`）、Node `26`、`pnpm 12`、`gpui-component v0.11+` 与 `actions/checkout@v7` 等最新稳定版；所有提交强制执行 Git Emoji 规范与 `gitleaks` + 个人路径隐私扫描（本地 `.githooks` + 云端 Actions 双重拦截）。

## ADR-007：主控只编排审查、Antigravity 延期与 Pi/Git 正交并行
- **状态**：已采纳（2026-11-25，治理 Issue `#34`）
- **背景**：用户明确要求主控只分配、监督和审查，编码通过 Paseo 独立 Worktree 完成；Antigravity（`#26`）只读边界尚未闭合，用户同意暂缓该线，不再阻塞其他阶段，同时参考 PiDeck 优化 Pi（`#32`）并启动 M3 只读底座（`#33`）。
- **角色与隔离**：执行代理在自己的隔离工作树中完成实现、测试、提交推送与创建 PR，严格遵守 Issue Owned Paths；不得改主检出、其他分支或真实用户全局配置，不自行合并 PR、回收工作区。主控分配和监督任务、独立审查证据并负责批准后的串行合入，不直接编码。
- **本轮边界**：
  - `#32` 仅接通 Pi 原生 RPC 的 `Queue` / `Steer` / `Stop`，正确验证 active/settled 状态和控制应答，纠偏成功不重复排队。PiDeck 只参考原生 Pi 控制、事件和队列设计，继续遵守 ADR-002：不引入 ACP、不扩大权限、不复制凭据或用户配置。
  - `#33` 仅通过本机 Git 提供仓库身份、分支/HEAD 与结构化 `status --porcelain=v2 -z` 只读探测，不自动 stash/reset/clean，不一次实现 Worktree 池、Diff、Checkpoint 或提交引擎；保持 `CheckpointSummary` 兼容，公共依赖或协议变更先报主控。
  - `#34` 仅维护 `docs/ops/PROGRESS.md`、本决策日志及自身分支 AOCI 托管资产，记录实时 GitHub/Paseo 身份、并行白名单和收口顺序，不写业务代码或根依赖。三线身份与精确 Owned Paths 见 [`PROGRESS.md`](PROGRESS.md#31-当前-pi--git-并行波次2026-11-25-北京时间盘点)。
- **延期不等于验收**：`#26` 保持开放，保留已有驱动、防护和探针，Antigravity Daemon 路由继续关闭。恢复该线时必须另行闭合真实入口、配置来源、校验到启动竞态、运行期保护及取消边界；不得把七种流式格式或六类已接线 CLI 写成 M2 七代理全验收，也不得把 M3a 派发或只读底座写成 M3 全完成。
- **收口顺序与证据**：治理 Issue `#34` 对应的 PR `#35` 先合流，再按 Issue `#32` → Issue `#33` 的业务 PR 串行审查合入，业务开发本身可并行。每个执行代理须提供 Owned Paths 审计、完整 `scripts/check-all.ps1`、AOCI 对齐及剩余风险；模拟夹具和真实原生 CLI 证据分别报告。后一个 PR 由其执行代理吸收最新 `main`，处理测试及 AOCI 冲突后复跑门禁，不得用整卷覆盖丢弃前一 PR 的认知。公共契约若必须调整，先派独立契约 PR 合入，再继续受影响分支。主控在每次合入收口时安排更新看板；本决策不授权执行代理合并或回收。

## ADR-008：主控自主经验收集与流程自优化闭环（Self-Evolving Orchestration）
- **状态**：已采纳（2026-11-27，治理 Issue `#74`）
- **背景**：多波次多 Worktree 并行研发与双平台真机验证持续暴露跨工具链、环境隔离、AOCI 协议及缓存容量等高频陷阱；仅靠人工回顾容易重复踩坑，且单靠上下文总结无法跨 AGY/Codex 会话无损传承。
- **决策**：建立三层随仓库 Git 版本化的自优化闭环：在 `AGENTS.md`（第三章第 5 条、第五章）确立制度与安全红线；在 `.agents/skills/paseo-orchestrator/SKILL.md`（第五章）定义 4 类自动捕获时机、L1/L2/L3 分级晋升漏斗、派发前 10 秒预检与容量剪枝机制；在 `docs/ops/LESSONS.md` 维护脱敏工程经验台账与 <= 25 条活跃速查表。
- **约束与边界**：任何流程自主晋升均不得削弱 `AGENTS.md` 第一章安全红线，不得改写 `AGENTS.md` 中 `<!-- aoci:begin -->`~`<!-- aoci:end -->` 托管区块，不得修改本机用户全局配置，也不得将真实用户名路径或凭据写入版本化文件；已被脚本或自动化测试完全机器防呆的条目须及时归档剪枝。

## ADR-009：双仓解耦联动发版（soxu-releases）与分级抗压 CI 架构
- **状态**：已采纳（2026-11-27，治理 Issue `#76` / 对标 `yetone/magpie`）
- **背景**：私有主仓无法直接向公网分发公开包管理器（Scoop / Homebrew / Winget）与下载直链，且单仓 CI 此前缺乏修改域分级、`main` 并发保护与超时护栏。
- **决策**：
  1. 主仓 CI（`.github/workflows/ci.yml` + `scripts/detect-check-scope.ps1`）实行按路径域与 Cargo 反向依赖闭包的精确分级检查，同时对齐 Magpie 抗压规则：`main` 永不因新推送取消在途 run、全 Job 显式 `timeout-minutes`、`CARGO_INCREMENTAL=0`、设置汇总门禁 `ci-gate` 兼容 skipped job；
  2. 新建公开联动发版仓库 `TanCan1998/soxu-releases` 与主仓 `.github/workflows/release.yml`：主仓以只读 Deploy Key（配于联动仓 `SOXU_DEPLOY_KEY`）配合限流退避 `repository_dispatch` 触发联动仓执行多平台 release 编译、实机 `--version`/`--help` 冒烟验证、`SHA256SUMS` + SLSA 构建溯源及包管理器清单自动更新，随后主仓镜像产物并用双重 `sort -V` 保护最高语义版本 `Latest` 标记；
  3. 新增 `docs/ops/CODE_STANDARDS.md` 并在 `docs/ops/LESSONS.md` 引入 `Seen N×` 复现自动晋升规则与 `--match-head-commit` 锁 SHA 合入纪律。
