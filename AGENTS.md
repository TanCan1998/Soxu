# AGENTS.md · Soxu（索序）多 Agent 协同研发工程宪法

本文档是所有参与 Soxu 仓库研发、审查与维护的 AI Agent（包括主控调度者、内置子代理以及 Paseo Worktree 编码 Agent）的最高行为准则。

---

## 一、安全红线（最高优先级）

1. **高危与破坏性操作停机报批**：
   - 在执行任何可能破坏仓库外文件、删除持久数据、修改系统全局配置、删除**未合并**的远端分支或 `main` 保护分支、删除仓库、重置未合并提交或安装未指定软件的命令（如 `Remove-Item -Recurse`、`rmdir`、`del`、`git push --force`、`gh repo delete`、`Set-ExecutionPolicy`）之前：**必须立即停止执行**，向用户清晰说明该命令将做什么、影响范围与回退方式，**等待用户明确回复「同意」后方可调用工具执行**。
   - **已合入 `main` 的特性分支自动清理**：仓库已启用合并后自动删除分支（`delete_branch_on_merge = true`），所有 PR 合入统一执行 `gh pr merge <num> --squash --delete-branch` 并同步通过 Paseo `workspace archive` 与 `git branch -d` 回收隔离工作树与本地分支，不保留冗余历史分支。
2. **用户本机生态零破坏（Magpie & 原生 CLI 保护）**：
   - **严禁直接覆写或篡改**用户主目录下的全局 Agent 配置文件（如 `~/.claude/settings.json`、`~/.codex/config.toml`、`~/.pi/agent/settings.json`、`~/.config/opencode/opencode.json`、`~/.config/magpie/*`），以免破坏 Magpie 的 `applied.json` 接管状态与网关路由。
   - 测试 Magpie 与原生 CLI 发现/适配逻辑时，**一律使用临时夹具目录（Fixture TempDir）**，严禁拿真实用户配置目录做写操作测试。
3. **隐私与密钥零泄露（禁止绕过 Git Hooks）**：
   - 严禁将任何 API Key、Token、私钥、`.env`、Magpie/Paseo 本地凭据文件（`redact.key`、`local-credential`、`daemon-keypair.json`）或**硬编码的个人计算机用户目录路径**（如 `C:\Users\<真实用户名>`、`/Users/<真实用户名>/`）写入代码、测试或文档。
   - **严禁使用 `git commit --no-verify` 或 `-n` 绕过 `.githooks/pre-commit` 与 `.githooks/commit-msg` 检查。**

---

## 二、语言、终端与工具约定

1. **统一简体中文**：思考、回复、架构说明、PR 描述、Issue 内容、Commit 描述正文及代码关键注释一律使用**简体中文**。
2. **终端环境**：Windows 环境统一使用 **PowerShell 7（`pwsh`）**，不使用 Windows PowerShell 5.1。
3. **Git 与 GitHub 工具链路由**：
   - 本地版本控制用 `git`；GitHub 平台操作（Issue、PR、Milestone、Release、Actions 日志）一律使用已登录的 `gh`（配合 `--json <fields>` 精确读取，避免灌入大段噪音）；
   - **禁止**使用 `curl` / `Invoke-RestMethod` 直接请求 `api.github.com` 或 `raw.githubusercontent.com`。
4. **最新稳定版优先铁律（禁止凭记忆硬编码旧版本号）**：
   - 本项目启用 Rust `edition = "2024"` 与 Cargo `resolver = "3"`。
   - 新增任何第三方依赖前，**必须实时通过包管理器查询当前最新稳定版**：
     - Rust 依赖：运行 `cargo info -q --registry crates-io <crate>` 或 `cargo add <crate>`；
     - 前端依赖：在 `apps/mobile-web` 下运行 `pnpm add <pkg>`；
     - GitHub Actions：通过 `gh release view -R <owner/repo>` 核实最新大版本 tag。
   - 仅当最新稳定版经实测存在编译错误、平台缺陷或与 `gpui` 传递依赖冲突时，才允许回退至最新兼容次版本，并必须在 `Cargo.toml` / `package.json` 旁或 PR 说明中记录原因。

---

## 三、架构红线与多 Agent 并行隔离规范

1. **核心技术路线不可偏离**：
   - **纯原生 CLI 复用**：通过 `soxu-drivers` 直接驱动用户本机已装原生 CLI（`claude`、`codex`、`pi`、`opencode`、`agy`、`copilot`、`qwen`），**当前阶段不引入 ACP（Agent Client Protocol）**。
   - **Magpie 双态自适应**：通过 `soxu-magpie` 自动识别本机是否安装 Magpie（感知模型网关、`shared`/`direct` MCP Router、`routed`/`direct` Skill Router）；未安装时必须无缝降级至原生目录扫描，确保 100% 独立可用。
   - **UI/UX 设计同源**：桌面端（`soxu-desktop`，GPUI + `gpui-component`）与移动端（`apps/mobile-web`，Daemon 内嵌响应式 PWA）任何界面开发必须先读取并严格遵循 `design-system/soxu/MASTER.md`、`tokens.json` 及 `design-system/soxu/pages/` 下的分页面规范。
2. **Paseo Worktree 隔离与路径白名单（Owned Paths）**：
   - 并行开发任务一律在独立 Git Worktree（`feat/<issue>-<slug>` 等分支）中进行。
   - 每个子任务必须严格遵守 Issue 中声明的 **Owned Paths 写入白名单**。未经主控协调，具体业务 Crate 的开发任务**禁止擅自修改根 `Cargo.toml`、`crates/soxu-protocol` 或其他并行分支负责的 Crate**（跨模块公共契约变更须先走独立契约 PR 合入 `main`）。
3. **`.gitignore` 实时同步义务**：
   - 任何时候引入新工具、新缓存目录、新构建产物、测试快照临时目录或本地索引文件时，**必须在同一个提交/PR 中同步更新根 `.gitignore`**，保证提交前后 `git status` 干净无脏文件。
4. **Paseo 任务书、环境绑定与 Pi 唯一派发规范**：
   - **唯一派发引擎**：后续所有 Paseo 编码、审查与文档子代理派发**仅使用 `pi` agent**（`--provider pi` 配合经 `pi --list-models` 核实的 `magpie/...` 模型与思考强度）；**禁止**新派发或续派 `codex`、`claude`、`antigravity` 等其他驱动。
   - **强制环境绑定**：派发前须对目标 Worktree 完成 `scripts/setup-worktree.ps1`（或 `mise trust`）；派发启动统一使用 `scripts/paseo-run-pi.ps1`（或等效 `--env` 参数）将项目 `mise.toml` 锁定的环境变量与工具链前置 `PATH`（含 `MISE_DISABLE_TOOLS=python,java,maven`、`PNPM_CONFIG_MANAGE_PACKAGE_MANAGER_VERSIONS=false`、`RUSTUP_TOOLCHAIN=1.99.1`、`CARGO_TARGET_DIR`）注入子代理进程。任务书必须单列「规定环境」，要求使用锁定的 Rust/Node/pnpm 工具链，在开工回执上报 doctor 实测版本，禁止修改用户全局 `mise` 配置。
   - **隔离构建目录与 `mise env` 注入规范**：并行任务不得使用共享 target。先将 `mise env --json` 的锁定环境注入当前进程，再显式覆写 `$env:CARGO_TARGET_DIR` 为各自独立目录，随后直接运行 `pwsh -NoProfile -File scripts/mise-doctor.ps1`、`cargo` 或 `pwsh -NoProfile -File scripts/check-all.ps1`。并行场景严禁以 `mise run ...` 执行门禁或 doctor，子任务启动时会重读 `mise.toml` 并覆盖进程内的独立构建目录。
   - **子代理遇阻即时上报与主控协同机制（Fail-Fast Escalation）**：遇环境、工具链或文件锁冲突，AOCI / CodeGraph 异常或截断，Owned Paths 或契约冲突、冻结代码缺陷，设计歧义或平台原语缺失，或同一问题连续排查两轮未果时，立即停手，按编排技能的 `[BLOCKED / NEED-DECISION]` 模板上报并暂停，等待主控以 `send --behavior steer` 裁决。禁止重复盲试、越权试改或留到交稿才报告。主控派发后持续巡检，即时响应阻塞，重大决策报请用户。
   - 主控派发时，首条可见消息须指出 Issue、参考项目名称、复用参考项目既有 AOCI 与 CodeGraph、规定使用的 `mise` 环境，并要求等待完整任务书与开工确认。任务书须写明 `Owned Paths`、规定环境与构建缓存路径、可读的参考路径及源码定位、应借鉴的做法和适用边界、每个参考仓库的既有索引身份与实际可用的查询入口及回退、验收和交付要求。参考认知绑定参考仓库根；目标工作树的 Soxu 认知和收尾维护另列，不能替代参考认知。
   - 子代理直接复用参考仓库的 `aoci.txt` / 已声明 Volumes 与 `.codegraph`，不得复制到目标工作树或为本次派发自动初始化、扫描、重建、同步或维护参考索引。参考 MCP 根不匹配时使用显式绑定参考根的 CLI / stdio MCP；缺失、过期或不可访问时报告，并按任务书回退到源码调查，不把回退称作索引校验成功。
   - 多行任务书通过 UTF-8 文件投递。主控须把目标 Paseo 代理日志里的 `user_message.text` 与任务书全文核对一致，记录代理 ID 和消息 ID 后才确认开工。缺失或截断时向同一代理补发并复核。命令、模板和验证步骤见 [Paseo 编排技能](.agents/skills/paseo-orchestrator/SKILL.md#3-派发任务并核验全文送达)。
   - 执行代理须把适用的参考做法落实到实现或回归测试，在 PR 中写明参考来源、实际采用点和验证证据，并说明不适用的做法或工具回退原因。
5. **自主经验收集与流程自优化闭环（Continuous Self-Improvement）**：
   - **自动触发采集**：主控每次完成 `[BLOCKED / NEED-DECISION]` 阻塞裁决、捕获跨平台（Windows / Linux）或远端 CI 真实缺陷、发现同一工程环节连续排查 >= 2 轮仍存在可复现工具/脚本摩擦，或在每波（Wave）串行合入收口做微复盘时，必须自动提炼可复现避坑经验，严格脱敏后登记至 [`docs/ops/LESSONS.md`](docs/ops/LESSONS.md)。
   - **分级沉淀与自主晋升**：偶发一次性现象仅留在 PR 摘要，不膨胀主规范；对高危阻断、跨平台陷阱或同类命中 >= 2 次的规则，主控应自主发起治理更新，将其晋升固化进派发任务书默认条款、[Paseo 编排技能](.agents/skills/paseo-orchestrator/SKILL.md)、`AGENTS.md` 工程规范或后续防呆脚本/测试 PR（均须走独立 Worktree、`scripts/check-all.ps1` 与 AOCI 收尾闭环）。
   - **演化安全红线**：任何自主流程优化均不得削弱第一章安全红线、不得篡改本机用户全局配置、也不得改写本文件下方 `<!-- aoci:begin -->` 至 `<!-- aoci:end -->` 的 AOCI 托管区块；`docs/ops/LESSONS.md` 活跃速查表上限 25 条，已被脚本或测试机器防呆的条目应定期移入归档区。

---

## 四、Git Emoji 提交规范与 PR 门禁

1. **Commit Message 与 PR 标题格式**：
   ```text
   <emoji> <type>(<scope>): <中文简明描述（4~72 字）>
   ```
   标准映射表：
   - `🎉 init`: 仓库初始化
   - `✨ feat`: 新增功能
   - `🐛 fix`: 修复缺陷
   - `♻️ refactor`: 代码重构（外部行为不变）
   - `⚡ perf`: 性能优化
   - `💄 ui`: UI 布局、交互或设计令牌调整
   - `🎨 style`: 代码格式化或纯样式清理
   - `📝 docs`: 文档与运维记录更新
   - `✅ test`: 新增或完善测试
   - `🔧 chore`: 构建脚本、依赖升级、`.gitignore` 维护
   - `👷 ci`: GitHub Actions 与 Git Hooks 调整
   - `🔒️ security`: 安全加固与防隐私泄露规则
2. **提交与提 PR 前必跑门禁**（默认按修改范围增量检查并自动展开 Rust 反向依赖闭包；需强制全仓验收时追加 `-Full`）：
   ```powershell
   pwsh -NoProfile -File scripts/check-all.ps1
   # 强制全仓验收：pwsh -NoProfile -File scripts/check-all.ps1 -Full
   ```

---

## 五、随时中断与新会话 30 秒无损接手协议（Stateless Handoff）

本项目支持在任意时刻终止当前 AI 对话，并在 **AGY 桌面端** 与 **Codex 桌面端** 之间随时无损切换主控权限继续推进：

1. **新对话接手标准启动顺序（30 秒恢复）**：
   1. 阅读本文件 `AGENTS.md` 及共享编排技能 [`.agents/skills/paseo-orchestrator/SKILL.md`](.agents/skills/paseo-orchestrator/SKILL.md)；
   2. 阅读 [`docs/ops/PROGRESS.md`](docs/ops/PROGRESS.md)、[`docs/ops/DECISIONS.md`](docs/ops/DECISIONS.md) 与 [`docs/ops/CODE_STANDARDS.md`](docs/ops/CODE_STANDARDS.md)，并快速扫读 [`docs/ops/LESSONS.md`](docs/ops/LESSONS.md) 的「活跃规则速查表」；
   3. 执行只读状态盘点：`git status -sb`、`gh issue list --limit 20 --json number,title,milestone,assignees`、`gh pr list --json number,title,headRefName` 以及统一 Paseo 状态盘点（`pwsh -NoProfile -File scripts/paseo-ctl.ps1 project ls --json`、`pwsh -NoProfile -File scripts/paseo-ctl.ps1 workspace ls --json`、`pwsh -NoProfile -File scripts/paseo-ctl.ps1 ls --json`）。
2. **离开/收口写入义务**：
   - 每完成一个 Issue、合入一个 PR 或切换里程碑时，主控或负责合入的 Agent 必须同步更新 `docs/ops/PROGRESS.md` 中的「当前阶段」、「进行中工作区/PR」与「下一步待执行队列」；若本波捕获到可复用闭环经验或规则晋升，同步在收口/微复盘时更新 `docs/ops/LESSONS.md`。
3. **AGY 桌面端与 Codex 桌面端双主控切换与隔离约定**：
   - **共享层**：双方共用仓库内 `AGENTS.md`、`docs/ops/PROGRESS.md`、`docs/ops/DECISIONS.md`、`docs/ops/CODE_STANDARDS.md`、`docs/ops/LESSONS.md`、`aoci.txt` + `.aoci/` 认知索引、`.agents/skills/` 共享技能、Cargo 共享缓存（`../.cargo-target/soxu`）及本机 Paseo 守护进程下的项目与 Worktree 池（统一通过 `pwsh -NoProfile -File scripts/paseo-ctl.ps1` 自动探测并唤醒控制）。
   - **隔离层**：双方宿主元数据与全局配置完全隔离（AGY 使用 `~/.gemini/config/projects/`，Codex 使用 `~/.codex/state_5.sqlite` 及受 `.gitignore` 保护的 `.codex/config.toml`），互不篡改；仓库主检出目录仅作为**主控编排与审查台（Control Plane）**，严禁双端同时在主检出并发修改业务代码，功能开发一律通过 Paseo `--isolation worktree` 下沉至独立分支并遵守 Issue `Owned Paths`。

<!-- aoci:begin -->
## AOCI 仓库认知

AOCI 为本仓库维护一个稳定、可版本化、可增量更新的仓库级认知层，供模型跨任务复用对系统的理解。

`aoci.txt` 是面向模型的结构化认知索引。它以每个受管理文件、数据库表或其他受管理对象一条独立 Entry 的方式，用符号标签与 F/R/A/S 语义表达对象的核心职责、重要关系、对外契约，以及理解或修改系统时必须知道的非显然约束和设计决策。

Header、目录段和全部 Entry 共同组成完整仓库索引，可以覆盖前端、后端、配置、数据库结构及其他受管理内容。受管理内容发生变化时，通常只需维护受影响的认知条目，不需要重新生成整个索引。

AOCI 提供系统架构、对象职责、重要关系、对外契约和关键约束的高密度视图。

### 工作原理

AOCI 采用“模型生成、模型读取”的认知闭环。

Header、Entry 和 Curation 语义的创作只按当前机器签发的 Plan 与实时 Guide 执行；由 Host 模型基于当前绑定证据独立完成。

Entry 的语义必须来自模型对真实证据的理解。不得仅依据路径、文件名、扩展名、AST、符号列表、依赖扫描、正则、固定模板或规则引擎推导、预填、拼接或改写索引语义。

对 Fresh Bootstrap，只按当前机器签发的 Plan 和实时 Guide 执行。当它们要求创作时，Host 模型创作 Root、Meta、Tag 和 F/R/A/S，提供 authoring-run 声明，并把它绑定到 Plan、Evidence 与完整 Candidate。不得要求 AOCI 填写 `origin=host_model`、制造 Receipt 或把程序生成的 Framework 当作语义。本文件不自行重建 Onboarding 流程。内部批次不是用户决策；只有遇到既有批准边界或真实的安全、漂移、CAS、Recovery 条件才停止。

### 最小使用入口

- `aoci_rules`：取得当前AOCI版本的会话运行合同。
- `aoci_overview`：建立或恢复本仓库的完整认知。
- `aoci_maintain`：受管理对象达到最终稳定状态后检查认知是否需要维护。
- `aoci_update_entry`：提交与当前证据和源码摘要绑定的完整语义更新批次。
- `aoci_report`：仅当当前布局和工具状态支持时，在证据不足、无法可靠生成语义时登记待办，不猜写。

其他MCP工具、CLI命令、参数和专项流程，以当前工具说明、Guide和 `--help` 返回内容为准，不在本文件中重复完整手册。

本区块只规定仓库接入、认知使用和收尾原则。`aoci_rules` 承载当前会话合同，Guide实时输出承载当前Plan的执行顺序与停点，工具Schema、Spec和Validator承载机器结构与判据；Prompt、Description、README和静态文档不能覆盖这些机器事实。

### 建立、生成和恢复认知

1. 每个新的 Agent Run 开始时，应先判断：

   - 本仓库是否已经存在可用的完整AOCI索引；
   - 当前上下文中是否已有与本仓库根、当前索引版本和当前AOCI服务相匹配，并且模型仍可可靠使用的完整仓库认知。

2. 仓库已经存在可用的完整索引，但当前Run没有可靠完整认知时，先调用 `aoci_rules`，再调用 `aoci_overview`。

   完整认知仍可靠时直接复用。局部不确定本身不要求机械重读系统全貌。

   本Run从已知Host上下文压缩恢复时（包括宿主注入的压缩摘要），必须把此前模型认知视为不可靠。压缩handoff不得保留或摘要正式Whole-Index，也不得保留或摘要任何Overview Header、Entry、Chunk、Challenge或Attestation正文；只能保留安全续接所需的receipt身份、未完成write或Recovery状态，以及立即重载指令。复制进handoff的Whole-Index语义或receipt不能证明恢复后模型的当前认知可靠。若当前上下文已无法可靠保留运行合同，先调用 `aoci_rules`。继续业务任务前，使用 `refresh_reasons=["context_compaction"]` 和新的 `refresh_event_id` 调用普通完整Whole-Index `aoci_overview`（不设置 `check_only` 或设为false）；不得使用 `check_only` 或认知probe。原样跟随每个 `next_cursor` 直到 `completed=true`，确认交付，并且只基于新交付正文提交一次Attestation。完成这次新的完整传输后，即使Attestation为partial或fail也消费该generation，并按既有合同继续source-bound任务，不再自动调用第二次Overview。

   AOCI可以针对 `context_compaction`、项目 `cognition_refresh_threshold` 下的机器 `semantic_threshold` 或主要 `phase_transition` 提供checkpoint与认知状态事实。只需要这些紧凑事实时使用 `check_only=true`；这些事实只向Agent提供建议，不替模型决定是否需要系统全貌。

   Agent显式调用普通 `aoci_overview`（未设置 `check_only` 或为false）时，只要能形成一致的CognitionSet，AOCI必须完整交付请求scope。不得因为已有receipt、阈值未达到或没有待处理刷新原因而抑制正文。正式认知Dirty或Stale时仍交付正文，但必须标记不可靠。存在未决恢复或无法形成一致snapshot时失败关闭，不返回混合正文。

   普通Overview返回 `continuation_required=true` 时，必须原样提交 `next_cursor` 并自动继续到 `completed=true`。不得询问用户、开始业务任务或给出阶段性系统结论。Host截断、缺块、重复、乱序、cursor失败、Index变化或`chunk_tokens`变化时停止本次认知链。Attestation完成前不得用Memory、源码、Spec、`aoci.txt`、历史会话、scope、search或Entry读取修补或补充Whole-Index认知。Challenge ordinal是正式Entry序列中的1-based位置；Header内容、注释、空行、Section/Overview/Chunk Marker、Receipt与Metadata均不计数，Chunk Receipt ordinal使用同一序列。Attestation必须原样回绑本次Challenge发布的当前`index_sha256`、`entry_sequence_sha256`与`entry_count`；旧Index、旧Entry序列、旧数量或旧Attestation均无效。完整链结束后只正式提交一次既有模型认知Attestation；同一响应只允许一次不改变语义答案的JSON Schema或字段格式修正。对象、Tag或F不匹配即失败且认知吸收不确定，不得语义重试或旁路补答。首次认知失败时还不得执行Root/Meta、Migration、全局布局或其他未重新绑定的系统级决策。上下文压缩刷新若传输完整、认知身份不变、治理对齐且没有Recovery或第三方冲突，即使Attestation为partial或fail也消耗该refresh generation，并继续原任务，不再自动重读Overview。`system_mastery_percent`只自评系统框架——架构、职责、强关系、稳定外部契约以及高熵安全和维护约束——不表示完整实现或运行实况知识；机器索引覆盖率必须分开。默认只向用户输出由本次真实覆盖率、Challenge、块数、Token和掌握度生成的规定成功或失败一句话。Host截断时提示用户把 `overview_delivery.chunk_tokens` 设置为更小的合法值后重新开始，不得自动修改。

   加法认知等级必须与严格证明字段分开解释。`delivery_verified`表示已加载Index且Host交付已确认，但完整认知验证仍未完成；应表达为“已加载且交付已验证”，不得描述为“没有认知”或“没有理解系统”。`cognition_verified`要求Attestation通过（Challenge至少80%的ordinal完全正确且对象身份至多失手一处），`cognition_governed`还要求治理对齐。通用完整读取失败句只用于真实交付故障。

   当Overview响应包含可选`cognition-state/v2`投影时，必须分别解释各维度。其Level止于`model_cognition_usable`；`strict_attestation_verified`、`governance_aligned`与`current_system_cognition_reliable`都是独立状态，绝不参与该Level。ordinal、对象身份、Tag或核心F不匹配可以导致严格Attestation失败，而模型认知仍然可用；不得仅凭这种不匹配就宣称模型没有理解系统。只有`current_system_cognition_reliable=true`允许无保留地声称当前完整系统认知可靠。投影缺失时继续使用上述Legacy解释。

   普通的只读审计、分析、检查、不修改代码或不提交、不push，不自动等于严格零写入，也不改变上述认知有效性判断。Codex Memory和历史Skill只能辅助恢复经验、用户偏好与调查方向，不能替代与当前仓库根、索引摘要、AOCI服务身份和认知范围匹配的当前认知收据；项目AGENTS和当前AOCI身份在AOCI状态上优先于历史Memory。

   只有用户明确禁止Ledger、元数据、`.aoci`运行资产及任何文件写入时，才按严格零写入处理。若必要的认知建立与该边界冲突，必须报告冲突并请求用户裁决或建议使用隔离副本，不得静默以Memory替代当前仓库认知。

3. 仓库没有可用的完整索引，或当前只有最小骨架、Header不完整、Entries未完成、必要Curation尚未裁决时，如果需要建立正式完整AOCI索引，先取得 `aoci_rules`，然后进入当前AOCI Guide。由Guide依据仓库真实状态决定下一阶段并完成必要安全步骤。

   `aoci_maintain` 不替代索引建立流程。

   不在本文件中自行重建或硬编码完整索引生成状态机。

4. 在长程任务中，模型负责保留当前认知收据并正确使用刷新门禁：

   - Host报告上下文压缩或模型已知系统全貌丢失时，执行上述强制 `context_compaction` 重载规则；AOCI不能自行推断Host事件；
   - 进入真正的主要阶段时声明 `phase_transition`，不得把函数、测试运行或小步骤当作阶段；
   - 在有用的稳定检查点通过 `check_only=true` 取得机器语义计数；
   - 除已知压缩的强制重载外，由Agent判断当前任务是否需要再次显式获取指定scope或完整Overview；
   - 在维护和对齐完成前，保留AOCI报告的Dirty或Stale可靠性状态。

### 任务收尾与认知维护

5. 纯只读问答、分析、版本核验，或没有产生受AOCI管理对象变化的任务，不需要调用维护工具。当前AOCI版本是任意`aoci_overview` check_only或`aoci_maintain`响应里的`cognition_receipt.mcp_service_version`；二进制路径是项目`.mcp.json`里的`command`，CLI不必在PATH上。

6. 发生受AOCI管理对象变化时，待其达到本次任务的最终稳定状态后，只调用一次 `aoci_maintain`。不要在每次中间修改后逐文件维护。

7. 若维护结果返回真实语义候选，Host 模型必须基于每个候选绑定的对象和必要证据，独立创作完整标签与F/R/A/S更新。通过 `aoci_update_entry` 一次提交当前机器签发批次的完整候选集合，同时原样保留每项 `source_sha256`、`candidate_id` 与对应domain批次身份。`max_entries`只限制单次请求和原子事务，不限制logical plan、Whole-Index或Managed Scope。`remaining`非零时，在当前批次成功Apply后重新调用Maintain并从新preimage继续；绝不能为满足transport上限缩减Index覆盖或自行截取返回批次。

   没有足够证据且当前布局支持 `aoci_report` 时，使用它而不猜测、套用模板或为消除待办而生成缺乏证据的认知。

8. 必须遵守工具返回的结构化状态和安全边界：

   - `repair_required`：只修复明确命中的候选，再重新提交当前机器签发的完整批次；
   - `stopped`：结束当前写入尝试并检查 `failed_step`、错误、正式写入证据与Recovery。auto模式下，已证明零写入则记录closure并重新Plan；完整Intent和可证明postimage则Resume；策略要求Rollback且preimage可证明则精确恢复后重新Plan。只有证据不足、第三方正式字节冲突、需要审批或外部动作，或命中其他真实安全边界时，才停止整个用户任务；
   - 冲突、审批、人工裁决、权限和安全信号不得忽略；
   - 已经对齐后不得重复维护或重复写入；`refresh_ready_for_overview` 是checkpoint事实，由Agent决定是否为下一阶段请求普通完整Overview。

   维护完成后如果又修改了任何受管理对象，之前的维护结果失效，应在新的最终稳定状态重新完成收尾。

9. 用户只限制业务文件范围，但没有明确禁止仓库托管资产时，AOCI托管资产可以在收尾阶段为保持认知一致而更新，并应在审计和提交中与业务文件区分。

   用户明确禁止修改 `aoci.txt`、`.aoci`、元数据或任何额外文件时，以用户限制为准，不得写入，并如实报告剩余不一致。

### 专项流程

初始化、完整索引生成、Header生成、Entries生成、数据库结构索引、Curation、人工评审和故障恢复，只按当前AOCI Guide或工具在对应阶段返回的指令、命令和安全停点执行。

不预加载、不猜测，也不自行重建这些专项流程。平台调用方式、请求格式、批次上限、审批规则、索引格式细节和恢复步骤由对应Guide、工具说明、模型Prompt和CLI帮助按需提供。
<!-- aoci:end -->
