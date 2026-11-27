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

## ADR-010：Soxu 项目级关闭 `context-mode` 并采用语义索引 + 作用域节流治理上下文
- **状态**：已采纳（2026-11-27，治理 Issue `#78`）
- **背景**：实测全局 `npm:context-mode@2.2.1` 会对超过 16 KB 且未传分段的 `read` 直接拒读、将 >4 KB 输出截断入本地 FTS5 索引，并在 `before_agent_start` 拼接动态状态破坏 Prompt Cache 命中率；在中大型 Rust/TypeScript 跨模块改造中易诱发残缺上下文与返工死循环。同时，若本机 Pi `trust.json` 未将主仓与 worktrees 父目录加入信任列表，非交互派发会跳过项目级 `.pi/settings.json` 与 `aoci` MCP。
- **决策**：
  1. 在仓库根目录配置 `.pi/settings.json`，在项目作用域内将 `npm:context-mode` 的 `extensions` 与 `skills` 置空，保障长链路编码时完整 `read` 源码与 API Prompt Cache 稳定性；不修改全局面向外部重 I/O 场景的通用设置。
  2. 以 **AOCI 认知索引 + CodeGraph 符号关系图 + Git 版本化 `docs/ops/` 事实文档**替代 FTS5 压缩记忆；命令输出侧通过 `scripts/detect-check-scope.ps1` 精准收敛范围与日志摘要替代黑盒过滤；重型编译日志排障与外部网页调研交由独立探索会话完成。
  3. 本机使用前须确保 Pi `trust.json` 覆盖主仓与对应的 Paseo worktrees 祖先目录，以激活项目级 `.pi/settings.json` 与自动 AOCI MCP 挂载。

## ADR-011：双仓工具链单一真相源、可区分缓存边界与按事件分组的并发治理（CI-D）
- **状态**：已采纳（2026-11-27，Issue `#92`，对标 `paseo` / `padu`）
- **背景**：`mise.toml` 已锁定 Rust `1.99.1`、Node `26.13.1`、pnpm `12.21.0`，但工作流里仍有三类硬编码：`toolchain: 1.99.1`（格式与矩阵各一处）、`version: 12`、`node-version: '26'`；联动发布仓另用 `toolchain: stable` + `version: 12` + `node-version: '26'`。CI-C 的审核 run 已实测 PWA 作业装到 `pnpm 12.17.0`，与锁定的 `12.21.0` 不一致——即"声明一套、实跑一套"，且不会报错，逐平台证据因此失真。缓存键 `key: ${{ matrix.platform.id }}` 也没有工具链、锁文件、profile 与 feature 维度。
- **决策**：
  1. **版本单一真相源**：新增 `scripts/read-mise-versions.ps1`，从受信任来源（工作树或 `-Ref` 提交）严格提取三个精确版本。缺项、范围/浮动/复合/标签版本、重复声明、未知工具或字段、畸形 TOML 一律非零退出，**不回退内置默认值**；输出只含版本、`mise.toml` 摘要与 ref 名，不含绝对路径或令牌。
  2. **只读一次、按输出传递**：版本只在 `detect-changes` 解析并写成 `rust_version` / `node_version` / `pnpm_version` / `mise_toml_sha256`，其余作业一律消费输出；安装后各跑一次 `-AssertInstalled` 把实测 `rustc` / `node` / `pnpm` 与声明比对，让"参数写对但没真装上"也失败闭合。
  3. **缓存键七维绑定**：`platform.id` / `runner.os` / `runner.arch` / `rust_version` / `profile-dev` / `features-default` / `hashFiles('Cargo.lock')`。实测缓存键由 `v0-rust-rust-check-<os>-<arch>-<hash>` 变为 `v0-rust-<platform>-<os>-<arch>-rust-<version>-profile-dev-features-default-lock-<sha>-…`，新旧键不交叉，不会跨工具链或跨依赖图复用。
  4. **并发按事件分组**：组名 `ci-<event>-<ref>`，`cancel-in-progress` 仅对 `pull_request` 为真。**实测约束**：GitHub 在**运行时**求值 `concurrency`，`queue: max` 与解析为 `true` 的 `cancel-in-progress`（动态表达式同样算）共存会让整个 workflow 判非法并**零作业**失败。因此在"同一 workflow 必须同时服务 PR 取消与 main 排队"的前提下只能使用 `queue: single`，并把该组合写进静态契约（`concurrency_queue_max_with_dynamic_cancel`）拦住后续回退。
  5. **发布链路同源且不缓存**：发布仓从已核验 `commit_sha` 的 `mise.toml` 读版本（复用源仓解析器，不复制实现、不写常量），三作业消费输出并实测比对；发布链**不挂任何持久依赖缓存**，并把 `rust-cache` / `actions/cache` / `sccache` / pnpm cache 纳入禁止清单——主仓 CI 与发布仓共用 Actions 缓存服务，同键构建可能命中不同源码树的指纹产物。
  6. **darwin-amd64 原生 Intel**：正式发布的 amd64 改由 `macos-15-intel` 原生构建并执行二进制 + daemon 冒烟，不再以 arm64 宿主交叉编译后标注"未原生验证"；四平台资产名、SHA256SUMS 严格校验、SLSA、逐作业 SHA 校验与只读权限保持不变。
- **实测证据**（同一 SHA `cd32db4`、同一 Runner 标签与工具链、`Cargo.lock` 未变，run `46024301919` 的 attempt 1 冷 / attempt 2 热）：

  | 平台 | 阶段耗时 冷→热（ms） | 缓存包 | 实测工具链 |
  | :--- | :--- | :--- | :--- |
  | linux | fetch 818→402 · clippy 32407→4842 · test 49883→36192 | 209,487,214 B | rustc/cargo `1.99.1` |
  | macos-arm64 | fetch 2027→4004 · clippy 24540→13005 · test 51732→77171 | 178,139,667 B | rustc/cargo `1.99.1` |
  | windows | fetch 13409→16758 · clippy 31951→9485 · test 130850→126677 | 187,068,052 B | rustc/cargo `1.99.1` |
  | PWA 作业 | — | — | node `v26.13.1` / pnpm `12.21.0` |

  **不宣称整体提速**：热缓存主要改善 Clippy（linux 32.4s→4.8s、windows 31.9s→9.5s、macos 24.5s→13.0s），而 `cargo-test` 在 macos 与 windows 上并未变快（macos 51.7s→77.2s，windows 130.9s→126.7s），`cargo-fetch` 在 macos/windows 上反而变慢。这些是真实观测值，受 Runner 方差影响，不构成"缓存使整体构建更快"的结论。
- **并发实测**（真实 run，禁止 push `main`，故 main 自身 ref 未直接实测）：
  - **PR 取消**：run `46026390870`（`ee8d54c`，创建 15:36:21，已 in_progress）被随后创建的 run `46026504778`（`686b405`，创建 15:37:08）取消；已完成的 `Detect` / `Rust Format` / `PWA` 保留 success，`Governance` 与三个 `Rust` 矩阵实例为 cancelled，`CI Gate` 按契约判 failure——取消不计为通过。
  - **手动触发独立分组**：run `46026504778`（pull_request，首个 job 15:37:47 起）与 run `46026938724`（workflow_dispatch，首个 job 15:40:10 起）**同时 in_progress**（重叠 15:40:10–15:43:08），证明 dispatch 组与 PR 组互不占位。
  - **非取消语义排队**：run `46026961283`（dispatch，创建 15:40:16）**未取消**在途的 `46026938724`，其首个 job 直到 15:45:18 才开始（对方 15:45:15 结束），排队约 302s 后 FIFO 执行。这与 `main` push 的配置路径完全同一（`cancel-in-progress` 解析为 false）。
  - **口径纪律**：`run_started_at` 在排队 run 上等于创建时间，排队等待只能从**首个 job 的 `started_at`** 读出；墙钟只能用 job 起止计算，不能用 run 的 API 时间戳凑。
- **校验与契约**：`verify-ci-gate.ps1` 新增硬编码版本、版本来源接线、缓存键维度、并发组/取消/queue 合法组合、摘要接线断言；`test-ci-gate.ps1` 扩到 514 项断言（含 8 个负向变体与 Summary 脱敏用例）；`test-read-mise-versions.ps1` 81 项断言；`check-all.ps1` 在 `mise.toml`/读取脚本/`ci.yml` 变更时自动跑版本自测。
- **未验证边界**（不得当作已完成）：① 联动仓未实际运行 `release.yml`（无 `v*` 正式 tag 授权，也未确认 `SOXU_RELEASES_TOKEN`/`SOXU_DEPLOY_KEY`），`macos-15-intel` 原生 Runner 与 daemon 冒烟**未在真实发布 run 中执行**，只做了 workflow 语法与安全自检；② `main` 自身 ref 的排队未直接实测（禁止 push `main`），以同配置的非取消分组实测替代；③ `queue: max` 的多 pending 能力在本仓不可用（见决策 4），本 PR 不声称取得 `queue: max` 通过证据。
