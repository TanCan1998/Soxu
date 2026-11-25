# Soxu 研发进度与跨会话交接看板 (PROGRESS)

> **新对话接手指引**：请先阅读根目录 [`AGENTS.md`](../../AGENTS.md)，再结合下方「当前里程碑状态」与 `gh issue list` / `gh pr list` 即可在 30 秒内无损恢复上下文。

---

## 1. 当前总体状态

- `M0`、`M0.5`（PR `#4`）与 **`M1: Protocol v1, Domain Core & Daemon Spine`（`#5` / `#6` / `#7`）已全部完成并合入 `main`**。**`M2: Magpie Discovery & 7 Native CLI Drivers`** 已交付双态发现（`#11`）、原生驱动（`#12`）、Codex、Pi、Claude Code、Qwen Code 本机只读对话（PR `#16`、`#18`、`#20`、`#22`）及 Copilot、OpenCode 本机只读快聊（PR `#24`、`#27`）。**用户于 2026-11-25 决定暂缓 Antigravity（`#26`）验收，Daemon 路由继续关闭，但延期不再阻塞其他阶段。M2 不据此宣布七代理全验收。**
- **当前主线为 Pi / Git 正交并行波次**：`#32`（M2j）参考 PiDeck 接通 Pi 原生纠偏控制；`#33`（M3a）启动 Git 仓库与状态只读探测；`#34` 独立维护治理文档。三项均已派发至 Paseo 隔离工作树，尚不能把派发、运行或提交 PR 当作功能交付；M3 只启动首个只读底座，不宣称整个里程碑完成。
- 仓库已启用合并自动删支（`delete_branch_on_merge = true`）与 Paseo Worktree 回收。
- **参考项目与索引进度（已全部完成）**：
  - [x] 拉取参考项目至 `.refs/`：`paseo`（7,818 文件）、`codeg`（3,324 文件）、`padu`（1,157 文件）
  - [x] **AOCI 认知索引已全部原子入库**：`padu`（方案 A 全量 **1,086** 条）、`paseo`（方案 B 宏观核心 **202** 条）、`codeg`（方案 B 宏观核心 **177** 条）、`Soxu` 主仓（全量对齐）
  - [x] **CodeGraph 符号图已全部构建（`--local-only`）**：`padu`（17,171 节点 / 61,175 边）、`paseo`（148,292 节点 / 427,706 边）、`codeg`（84,541 节点 / 336,448 边）、`Soxu`（87 节点 / 129 边）

---

## 2. 里程碑全景路线图

| 里程碑 | 名称 | 状态 | 核心交付物 |
| :--- | :--- | :--- | :--- |
| **M0** | Bootstrap & Engineering Governance | ✅ 已完成 | 仓库骨架（Rust 2024 + PWA）、`.githooks` + `gitleaks` 隐私与 Emoji 门禁、最新 Actions CI、三层文档、Paseo `git` 项目绑定 |
| **M0.5** | Pure Black Design System & Dual Prototypes | ✅ 已完成 | 纯黑 7 级海拔 `tokens.json` + `shadcn/ui` 变量、MASTER + 7 份分页面规范、React 19 + `shadcn/ui` + Tailwind v4 双端原型与 7 分区独立设置、单文件桌面预览 `preview.html`、移动遥控 PWA 原型（PR `#4`） |
| **M1** | Protocol v1, Domain Core & Daemon Spine | ✅ 已完成 | `soxu-protocol` 全量 RPC/WS `seq` 契约、`soxu-core` 状态桶/能力协商/安全红线/队列草稿、`soxu-daemon` SQLite WAL 持久化 + WS 增量追平 + `#offer=`/6 位码鉴权、`soxu-cli` 闭环（`#5` / `#6` / `#7`） |
| **M2** | Magpie Discovery & 7 Native CLI Drivers | M2h 已完成，M2i 暂缓，M2j 进行中 | 已接通六种原生只读 CLI；`#32` 正在完善 Pi 原生控制。七种流式格式适配不等于七代理全验收，Antigravity 路由保持关闭。 |
| **M3** | Git Worktree Pool, Checkpoints & Commit Engine | M3a 进行中，其余待启动 | `#33` 本轮仅交付仓库身份与结构化状态只读探测。三合一项目接入、Worktree 池、`direct` MCP 挂载、损坏恢复、三基准 Diff、Checkpoint 安全回退及结构化提交仍为后续目标。 |
| **M4** | Live Mobile PWA & GPUI Workbench Foundation | ⏳ 待启动 | **双轨 100% 正交并行**：`apps/mobile-web` 去 Mock 接通真实 Daemon WS/`rust-embed`/扫码配对与 13 组视口验收；`soxu-desktop` GPUI 令牌编译器、双开源字体联动、可扩展 `i18n`、7 层焦点栈与四栏自适应壳层 |
| **M5** | GPUI Core Views: Transcript, Composer & Inspector | ⏳ 待启动 | `soxu-desktop` 三路并行落地：`<=8.3Hz` 虚拟对话流 + 5 类工具卡/审批卡/提问卡 + 20px 轮次导航轨、760px 同步宽输入舱 + `@agent` 移交 + 三色圆环统计、右栏 Diff/Files、7 分区设置与核心业务模态框 |
| **M6** | Scripts, PTY Terminal, Search & Automation | ⏳ 待启动 | `paseo.json` 受管脚本遥控、`#050505` 多窗格受控 PTY 终端、`Cmd+K` 五分区检索 + CodeGraph 符号补全 + 按需三步 Onboarding、Schedules & Heartbeats 定时调度中心、50,000 行 Diff 压测与跨平台打包 |

### 2.1 分阶段串并行开发顺序（Paseo Worktree 编排主线）

1. **M1 — 全量契约与守护进程底座**：
   - **Wave 1.1（单流契约 PR）**：冻结 `Cargo.toml` + `soxu-protocol`（覆盖 `SessionTarget` 无项目 `QuickChat` 与项目二级树、5 级状态桶、6 档权限与高危红线审批、`PendingUserInput`、`HandoffSelection`、11 项遥测 `SessionTelemetryStats`、跨端 `SoxuPreferences`、`afterSeq` 游标信封）。
   - **Wave 1.2（双路 Worktree 并行）**：Worktree A（`soxu-core`）实现状态桶聚合、7 代理能力协商与 `lost-controls` 回退、高危红线判定、草稿代次与队列提级；Worktree B（`soxu-daemon` + `soxu-cli`）实现本地持久化、WS 双向 RPC 与 `afterSeq` 增量追平、`#offer=` 与 6 位码配对鉴权。
2. **M2 — Magpie 感知与 7 大原生 CLI 流式驱动（率先跑通真实对话）**：
   - **双路 Worktree 并行**：Worktree A（`soxu-magpie`）实现双态只读拓扑、原生降级扫描与启动环境脱敏透视；Worktree B（`soxu-drivers`）实现 7 大 CLI 流式驱动、`Queue`/`Steer`/`Interrupt`、11 项遥测采集与本地历史 Resume/Fork 导入。
   - **合流接入**：将 `soxu-magpie` 与 `soxu-drivers` 挂入 `soxu-daemon` 与 `soxu doctor`，打通无项目/本地目录真实对话。
   - **当前增量（`#32`）**：只完善 Pi 原生 RPC 的 `Queue` / `Steer` / `Stop`，参考 PiDeck 的原生 Pi 控制、事件与队列设计；不引入其 ACP 路线，不扩大权限。Antigravity（`#26`）作为开放的延期验收项保留。
3. **M3 — Git 隔离池、轮次检查点与结构化提交**：
   - **当前首阶段（`#33`）**：仅通过本机 Git 探测绝对路径对应的仓库身份、HEAD/分支及 `status --porcelain=v2 -z`；不自动写仓库，不实现 Worktree、Diff 或 Checkpoint 操作，与 `#32` 正交推进。
   - **后续双路推进 `soxu-git`**：A 路实现打开/克隆/初始化三合一接入、Worktree 池与损坏恢复、Checkpoint 三选一安全回退（含 Safety Stash）；B 路实现三基准 Diff、词法安全文件树、拆分式 Git Emoji 提交与门禁预检。
   - **合流联动**：新建 Worktree 时联动 `soxu-magpie` 自动补齐 `direct` MCP（AOCI / CodeGraph）工作区上下文。
4. **M4 — 移动遥控 PWA 真机闭环 + GPUI 桌面底座（双轨 100% 正交并行）**：
   - **Wave 4.1（单流准备 PR）**：在根 `Cargo.toml` 引入 `gpui` / `gpui-component` 并配置开发编译优化，扩展 `scripts/sync-design-tokens.mjs` 生成零漂移 Rust 主题常量并接入 `--check`。
   - **Wave 4.2（双轨正交并行）**：轨道 A（`apps/mobile-web` + `soxu-daemon` Web 伺服）去 Mock 接通真实 Daemon WS/`rust-embed`/扫码配对并完成 13 组视口验收；轨道 B（`soxu-desktop`）搭建 Pure Black/Light 主题插值、双开源字体与 `effective_code_family()`、可扩展 `i18n`、7 层焦点栈与四栏自适应壳层。
5. **M5 — GPUI 桌面核心视图 3 路并行**：
   - 按 `crates/soxu-desktop/src/` 独立子目录 3 路并行：A 路（虚拟对话流 `<=8.3Hz` 合批 + 5 类工具卡/审批卡/提问卡 + 20px 轮次导航轨）、B 路（760px 同步宽输入舱 + 纯图标权限与投递 + 可配模型移交 + 三色圆环与 11 项统计）、C 路（右栏 Diff/Files + 7 分区设置弹窗 + 提交/配对/回退/透视等核心模态框）。
6. **M6 — 受管脚本、受控 PTY、全局检索与自动化收口 3 路并行**：
   - 单流补充 PTY/Scripts/Schedules 小契约后 3 路并行：A 路（`paseo.json` 受管脚本 + `#050505` 多窗格受控 PTY 终端）、B 路（`Cmd+K` 五分区检索 + CodeGraph 符号补全 + 按需三步 Onboarding）、C 路（Schedules & Heartbeats 定时调度中心 + 50,000 行 Diff 压测与跨平台打包）。

---

## 3. 活跃工作区与智能调度策略

### 3.1 当前 Pi / Git 并行波次（2026-11-25 北京时间盘点）

以下身份与状态来自本轮 `gh issue view 26/32/33/34`、`gh pr list` 和 `paseo-ctl.ps1 project ls / workspace ls / ls --json`。盘点时三项 Issue 均开放、三个执行 Agent 均为 `running`，GitHub 无开放 PR；这是派发快照，不是验收结论。新 PR 及后续状态须实时复查，合入收口时由主控安排更新本看板。

- **主控边界**：Soxu 项目为 `prj_24ffa03410264684`，主控工作台为 `wks_21203a9ef0f5e2e5`（`main`）。主控只分配、监督与审查，不直接编码；执行代理自行完成实现、测试、提交推送与建 PR，不合并 PR、不归档工作区、不修改主检出。
- **PiDeck 参考基线**：`#32` 派发记录指定 `ayuayue/PiDeck@b9a095e063845c80574c5517994c6fcf2be60f25`；具体原生 Pi 文件、可借鉴决策及模拟/实机证据由该执行代理在 PR 中报告，不把参考库的 ACP 设计移植进 Soxu。

| Issue / 工作线 | 隔离分支 | Workspace | Agent | 派发配置 / 盘点状态 |
| :--- | :--- | :--- | :--- | :--- |
| `#34` 治理先行 | `docs/34-parallel-wave` | `wks_540110c51aea6f11` | `308c1ebb-ddc9-4055-8cdd-c213c4034e87` | Codex / Sol / medium；`running`，PR 待创建 |
| `#32` M2j Pi 原生纠偏 | `feat/32-pi-native-steer` | `wks_b7f3663147388206` | `74d002f4-7107-41e0-a290-872c33f56eee` | Codex / Astra / high；`running`，PR 待创建 |
| `#33` M3a Git 只读底座 | `feat/33-git-readonly-probe` | `wks_8f142f393281bfad` | `48cafea4-3d96-4950-83f8-79d5d5263e4e` | Codex / Sol / high；`running`，PR 待创建 |

三线仅可修改下列 Owned Paths 及**各自分支的 AOCI 托管资产**，不得写其他工作树或主检出；根依赖、`soxu-protocol`、`soxu-core` 及未列出的路径保持冻结。

| Issue | 业务 / 治理写入白名单 | 明确禁止 |
| :--- | :--- | :--- |
| `#34` | `docs/ops/PROGRESS.md`、`docs/ops/DECISIONS.md` | 业务代码、根依赖与其他文档路径 |
| `#32` | `crates/soxu-daemon/src/state/native.rs`、`crates/soxu-daemon/tests/**`、`crates/soxu-drivers/src/process.rs`（仅必要 Pi 控制修正）、`crates/soxu-drivers/tests/**`、`crates/soxu-drivers/README.md` | `soxu-git`、`docs/ops` 及其他 crate；公共契约变更须先报主控 |
| `#33` | `crates/soxu-git/**` | 其他 crate、根依赖、协议及 `docs/ops`；不得自动 stash/reset/clean |

**审查与串行合入顺序：`#34` → `#32` → `#33`。** 治理 PR 先合流，业务实现可同时继续；每个 PR 必须由主控核对 Owned Paths、独立复核证据、确认完整 `scripts/check-all.ps1` 与 AOCI 对齐后才合入。后一个 PR 由其执行代理吸收最新 `main`，处理交叉的测试或 AOCI 资产并复跑门禁，禁止用整卷覆盖抹掉前一个 PR 的认知。公共契约若确需变更，先派独立契约任务、串行合入，再恢复受影响分支；不得让两个业务 PR 同时改共享契约。

主控只在审查批准后执行合入与已合并分支/工作区回收。`#26` 保持开放、路由关闭，只有另行恢复验证并闭合原生入口、配置来源、启动竞态及运行期边界后才可重新评估验收。各线的本机夹具必须隔离，禁止写真实用户全局配置或复制凭据。

### 3.2 已合入交付与证据边界

- M2c 已通过 PR `#16` 合入 `main`，隔离工作区已归档。夹具覆盖 HTTP/WS 增量、SQLite 回读、FIFO、取消、原生失败、重启恢复，以及只读和浏览器来源门禁。
- `#11` 的七项测试及 Windows 只读烟测通过。`#12` 的七种流式格式通过夹具验证，Codex 无项目和本地目录的真实对话通过登录烟测，原生 Resume/Fork 通过回环测试。各驱动的验证范围见 [`soxu-drivers/README.md`](../../crates/soxu-drivers/README.md)。
- `#15` 使用真实 Codex `0.172.0`、隔离 `CODEX_HOME` 和本机 Responses 夹具，验证两类目录均能经 Daemon 收到固定回复并落盘。在线登录推理沿用 `#12` 的验证记录。
- `#17` 已经由 PR `#18` 合入 `main`。真实 Pi `1.5.2` 经 Daemon 连接本机模型夹具，两轮流式回复沿用同一原生 UUID，第二轮带回首轮上下文。HTTP/WS 夹具覆盖无效标识、失败、取消、FIFO 与 SQLite 恢复。Pi 启动参数及数据目录见[驱动说明](../../crates/soxu-drivers/README.md#daemon-本机-pi-只读对话)。
- `#19` 已由 PR `#20` 合入 `main`，隔离工作区已归档。真实 Claude Code `2.1.331` 经 Daemon 连接本机模型夹具，两轮回复沿用同一 UUID，模型请求未携带工具，会话 JSONL 落在 Daemon 的 `claude` 目录。
- `#21` 已由 PR `#22` 合入 `main`，隔离工作区已归档。真实 Qwen Code `0.28.0` 经 Daemon 完成两轮回环续接，模型请求未携带工具，额外返回的工具调用触发失败且未执行。配置、运行数据与历史落在 Daemon 的 `qwen` 目录。
- `#23` 已由 PR `#24` 合入 `main`，隔离工作区已归档。真实 Copilot `1.0.114` 经 Daemon 完成两轮回环续接，两个请求均无工具，第二轮带回首轮提示与回复。两轮复用同一 UUID，历史与缓存分别写入隔离状态和缓存目录。本地及云端门禁通过。
- Copilot 项目钩子夹具中，默认启动跳过 `sessionStart`，启用目录信任后触发。可信项目把 `disableAllHooks` 设为 `false` 可以覆盖用户级 `true`，项目绑定继续关闭。
- `#25` 已由 PR `#27` 合入 `main`（`043fc66`），Paseo 工作区已归档、特性分支已回收。真实 OpenCode `1.19.6` 经 Daemon 两轮同一 `ses_` ID、上下文续接、零工具及停止后模型连接断开通过；数据库与缓存隔离。本地七项及云端三项门禁通过。版本门禁暂不放开派生独立服务的新版。
- `#26` 的严格权限防护与基础探针已经由 PR `#28` 合入 `main`（`ab96da7`），原工作区已归档。实机当前为 Antigravity `1.5.17`，探针只复制 EXE 并使用临时用户目录。两轮同一 UUID、前轮上下文及取消断开模型连接通过；有效文件写入被 deny 规则拦截，未生成目标文件，但回合返回成功。
- `#26` 的工具过滤及项目覆盖探针已由 PR `#29` 合入 `main`（`ba90fc4`），原工作区已归档。空工具自定义代理将实际模型声明从 15 项降至仅 `manage_task`，未声明写入被注册层拒绝；全匹配全局钩子能够在执行前拒绝残留工具。
- 项目同名代理可重新声明写入工具，关闭斜杠展开也没有阻止真实项目 `SessionStart` 钩子执行；项目同名禁用和自动批准钩子未削弱全局工具拒绝。Daemon 路由保持关闭，下一步需固定隔离快聊的可信资产并验证篡改和加载边界。可复跑探针与完整限制见[驱动文档](../../crates/soxu-drivers/README.md#空工具代理与钩子实测)。
- `#26` 的钩子故障与资产缺失回归已由 PR `#30` 合入（`e19a523`），工作区已归档。钩子非零退出、非法 JSON、超时和脚本缺失均阻断残留工具；但配置为空、配置缺失或自动批准篡改可使任务列表工具真实执行。
- `#26` 的可信资产与实际回合启动门禁已由 PR `#31` 合入（`160e009`），工作区已归档、特性分支已回收，本地七项与云端三项门禁全部通过。Daemon 库新增 `NativeAssetGuard`：以调用方可信内容验证大小、SHA-256、普通文件和祖先目录，拒绝链接/reparse 替换和根内 `.agents`，启动目录不得越界。宿主可绑定已配置驱动，首轮、续接和自动 FIFO 在实际启动前重验；拒绝不写新回合消息、不消费队列，恢复后可继续。Windows 路径表示差异已处理。尚未挂入 Antigravity 路由，校验与启动竞态、其他配置来源和运行期保护仍待验证；本轮已按用户决定延期，不作为 Pi/Git 双线的前置阻塞，不把已合入防护当作 AGY 适配完成。
- 本机使用 `soxu doctor --json` 查看脱敏发现结果，通过 `soxu chat new --read-only` 创建快聊，再用 `soxu chat send <对话或会话 ID> <文本>` 发送，`soxu events` 查看增量，`soxu chat stop <对话或会话 ID>` 停止。Windows 的 Scoop 包装器需用 `SOXU_CODEX_EXE` 指定真实 EXE 的绝对路径。失败或停止后暂停剩余队列，下一次投递从队首继续。队列驻留内存。

- **当前并发上限**：参考库索引阶段采用 `3~4` 路独立仓库并行初始化；后续进入 M0.5 顶层规范与各阶段契约冻结时回归 `1` 路串行，M1–M6 接口冻结后的正交模块实现采用 `2~3` 路 Paseo Worktree 并行。
- **双桌面端主控协同（AGY ⇄ Codex 随时切换）**：本机 AGY 桌面端与 Codex 桌面端已同时导入 `Soxu` 项目，宿主配置与工作树写操作相互隔离，仓库认知（`AGENTS.md` / `aoci.txt` / `.agents/skills/paseo-orchestrator`）与 Paseo 守护进程状态池全面共享；任意时刻均可使用 `pwsh -NoProfile -File scripts/paseo-ctl.ps1` 在两端之间无损切换控制 Paseo。
- **主力 Agent 与模型路由池**（通过 Paseo 调度，经 Magpie 网关路由）：
  - **L3 架构/并发攻坚**：`codex` / `pi` / `agy` | `group/gpt-6.2-astra` / `group/claude-opus-5.5` / `gemini-4-carbon` (`high`)
  - **L2 核心模块/UI 实现**：`codex` / `pi` / `agy` | `group/gpt-6.2-sol` / `group/claude-sonnet-5.5` / `auto/gemini-4-hydrogen` (`medium`)
  - **L1 脚手架/文档巡检**：`pi` / `agy` | `auto/deepseek-v4.1-flash-1017` / `auto/gemini-4-hydrogen` (`low`)

---

## 4. 下一步紧接行动队列（Next Actions）

1. 主控先审查并合入 `#34` 治理 PR，确认本波次角色、Owned Paths、AGY 延期与串行合入顺序；执行代理不自行合并或回收。
2. `#32` 独立完成 PiDeck 原生 Pi 参考报告、Pi active/settled 与 RPC ack 校验、纠偏成功不重复排队及拒绝/停止/持久化回归；完整门禁和 AOCI 对齐后提交 PR，由主控审查。
3. `#33` 同时完成 Git 仓库身份与 `porcelain=v2 -z` 只读探测，覆盖 linked worktree、detached/unborn、特殊文件名及错误输入；在 `#32` 之后吸收最新 `main`、复跑门禁并交主控串行审查合入。Worktree 池、Diff、Checkpoint 与提交引擎后续另行派发。
4. `#26` 暂缓且保持开放，保留全部已合入防护和探针，Daemon 路由保持关闭。待用户/主控恢复该线时再验证原生入口、配置来源、启动竞态、运行期保护及取消边界，不占用当前主线前置队列。
5. 后续另行定义 Copilot 项目钩子安全、遥测、历史导入、附件、移交、更高权限及移动端投递的验收范围；M2 七代理全验收与 M3 全交付均不可提前声明。

