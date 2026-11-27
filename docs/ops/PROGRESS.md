# Soxu 研发进度与跨会话交接看板 (PROGRESS)

> **新对话接手指引**：请先阅读根目录 [`AGENTS.md`](../../AGENTS.md)，再结合下方「当前里程碑状态」与 `gh issue list` / `gh pr list` 即可在 30 秒内无损恢复上下文。

---

## 1. 当前总体状态

- `M0`、`M0.5`（PR `#4`）与 **`M1: Protocol v1, Domain Core & Daemon Spine`（`#5` / `#6` / `#7`）已全部完成并合入 `main`**。**`M2: Magpie Discovery & 7 Native CLI Drivers`** 已交付双态发现（`#11`）、原生驱动（`#12`）、Codex、Pi、Claude Code、Qwen Code 本机只读对话（PR `#16`、`#18`、`#20`、`#22`）及 Copilot、OpenCode 本机只读快聊（PR `#24`、`#27`）。**Antigravity（`#26`）在 PR `#55` 接入 Daemon 托管的本机隔离只读 `QuickChat`，项目目录入口继续关闭。M2 仍有未交付范围。**
- **Pi / Git 波次已收口**（`#34` → `#32` → `#33`）。M2j 的 Pi 原生纠偏和 M3a 的 Git 仓库身份与状态只读探测均已合入 `main`。M3b（`#39` / PR `#41`）已合入 `main`（`7cea1c4`），交付 `GitProbe::open_local_project` 本地项目只读接入：区分普通目录与 `GitWorktree`，拒绝 bare、损坏仓库、Git 元数据目录及权限错误；`soxu-git` 25 项专项测试通过。
- **M3e 注册与安全检出已合入 `main`。** `#49` / PR `#53` 提供受控注册和只读诊断，`#51` / PR `#54` 提供固定 OID 的受控检出和失败禁用。
- **M3e 第三阶段（`#56` / PR `#61`）已合入 `main`，交付宿主接入控制面。** `DaemonState` 核验 `CheckoutStatus::Verified` 后，重核注册与目录身份、index 和固定提交树。路由失败单独记录，重验通过后可重试。`soxu-magpie` 在 `MagpieManaged` 为工作区准备 `direct` MCP 目录级上下文，`StandaloneNative` 可直接使用通过复核的工作区。`crates/soxu-daemon/src/state/native.rs` 与 `soxu-protocol` 保持冻结，真实 Agent 启动与 Worktree 会话联调已由 `#69` 接续（只读原生会话），运行时挂载仍待后续接入 PR。Linux 真实注册和检出链路待非 Windows 宿主补测。
- **本轮复验已收口。** `#26` / PR `#55`、`#56` / PR `#61` 与 Diff 夹具修复 PR `#62` 均已完成主控复验、通过远端 CI 并合入 `main`，对应 Worktree 已归档。PR `#62` 合入提交为 `810b670`，当前基线为 `0b0317b`。
- **`wave-m3-next-20261126` 三路已全部合入 `main`。** `#63` / PR `#66`（`02daacf`，子代理遇阻上报与隔离构建规范）、`#64` / PR `#67`（`5225eca`，M3 Git 操作公共契约）、`#65` / PR `#68`（`21cf871`，轮次 Checkpoint 私有索引与覆盖清单）均通过远端 CI 并合入，三路工作区已归档、本地与远端分支已回收，当前基线为 `21cf871`。
- **`wave-m3-next2-20261126` 三路已合入 `main` 并归档。** `#69` / PR `#73`（`88417b3`）、`#70` / PR `#72`（`d3afda4`）、`#71` / PR `#84`（`2f47c82`）均通过远端 CI 合入，三个工作区已归档、本地与远端分支已回收。本波所有子代理统一使用 DeepSeek（`magpie/auto/deepseek-v4.1-flash-1017`，用户简称 `ds4.1f`）。`#69` 交付 `soxu-daemon` 已复核 Worktree 的只读原生会话联调、启动前目录与清单身份重核及看板刷新；`#70` 交付固定端点 Diff；`#71` 交付显式受控 Init/Clone。
- **`main` 已再合入 `#74` / PR `#75`（`22f94af`）。** 升级主控提示词，建立主控自主经验收集与流程自优化闭环（`docs/ops/LESSONS.md` 的 Seen 复现计数晋升机制）。
- **`main` 已合入 `#76` / PR `#77`（`ea9f345`）。** 完成按改动域与 Cargo 反向依赖分级的抗压 CI、`scripts/detect-check-scope.ps1` 门禁分流、`.github/workflows/release.yml` 与公开联动发版仓库 `TanCan1998/soxu-releases`。
- **`main` 已合入 `#78` / PR `#79`（`481ed70`）与 PR `#81`（`0e969b6`）。** 项目级 `.pi/settings.json` 以 `autoload: false` 关闭 `context-mode`，派发脚本放行 `python 3.13` 并注入 `PYTHONUTF8=1`，同时修复 `detect-check-scope.ps1` 双括号 `[[bin]]` 误解析导致 `soxu-cli` 包名漏检的缺陷。
- **`main` 已合入 `#80` / PR `#82`（`e9504d6`）。** 收窄 Linux 真机已检出 Worktree 诊断的 `pending` 放行条件，为 `project.rs` 补跨挂载点 stderr 严格白名单，并修复两处历史 Linux Clippy 告警。
- **CI 抗压波次：`#83`（CI-A）与 `#86`（CI-B）均已合入 `main`。** `#83` / PR `#85`（`a86024e`）交付 `ci-gate-plan/v1` 固定 job 契约、`Plan` / `Verify` 同源推导对账与发布资产白名单，已通过远端 CI 合入 `main` 并归档工作区。审核候选提交 `2216417` 的 run `45906220407` 是审核快照，合入后主线 run `45906614204` 核验的正是 CI-B 的派发基线 `a86024e`，两者是分开记录的两条证据。`#86` / PR `#87`（`8c01a533`）交付 `schemaVersion: 1` 结构化变更计划、Cargo 元数据反向闭包、Git NUL 差异与基准语义、平台/必跑 job/测试配置推导，保留 CI-A 的严格门禁对账与路径契约保真，已通过远端 CI 合入 `main` 并归档工作区。
- **原生矩阵冻结源码热修复（`#90`，PR `#91`，分支 `fix/native-matrix-blockers`，工作区 `wks_4e0ff3d28c954665`，基线 `8c01a53`）已合入 `main` `354fbd7b52f2a27db111541ef3da522d3a770b09`。** 目标是在不改动 CI 工作流与脚本的前提下，修复 run `45914578287`（attempt 1，合并候选 `260333d6d397d61f57d8a11e0084489dcbf3ae94`）在 `macos-15` ARM64 与 `ubuntu-24.04` 上的两处真实失败：Darwin 上 `crates/soxu-git/src/repository_create/platform.rs` 的 `open_handle` 未使用参数与 `needless_return`（保留 `UnsupportedPlatform` 零写入语义，未加 `allow`、未把 Linux 原语放宽到 Darwin）；Linux 上 `crates/soxu-git/tests/diff.rs` 的整树快照与夹具自身派生的后台 `git maintenance` 竞态（夹具提交改为与生产只读入口同源的 `-c maintenance.auto=false -c gc.auto=0` 守卫，新增 trace2 `child_start`/`child_exit` 确定性回归）。
- **`#90` 第二轮（有界修订）：加固回归用例里可独立证实的不等待风险。** PR `#91` 原 head `b0fc54b6` 的 GitHub run `45917865092` / Windows `rust-check`（job `140241392408`）在 `Run scoped unit & integration tests` 触到 30 分钟 job 级上限后 cancelled，job 与 run 日志均报 `log not found`（`BlobNotFound`），远端无法定位到具体二进制。同范围本地运行（`-p soxu-daemon -p soxu-git`）**未复现**该超时：原候选 `b0fc54b6` 与最终源码 `93b5125` 分别耗时 `111s` 与 `94s`，因此原 30 分钟超时**仍属未定因**。可独立证实的是对照用例自身的无界等待风险：旧实现的阳性对照刻意重新打开后台维护，并用 `Command::output()` 无界收集 stdout/stderr，只要继承写端的子/孙进程存活就会永久阻塞——该风险在旧候选上已独立确认。修订后对照仍真实走同一条 `commit_command` 提交路径，参数为 `maintenance.auto=true` + `gc.auto=0` + 显式 `*autoDetach=false`，改为 `stdin`/`stdout`/`stderr` 全部 `Stdio::null()` 的有界直接进程等待（`spawn` + `try_wait` + `60s` 截止 + `kill` + `10s` 回收窗口），五态（成功 / 非零退出 / 超时 / 观察异常 / 回收未确认）可区分，`try_wait` 自身失败也要走 kill + 有界回收，超时只承诺回收直接子进程；失败证据仅输出布尔摘要，不转储原始 trace2。三项源证据（子进程启动、无 detach、同 `sid` + 同 `child_id` 的 `child_exit`）全部保留：`child_id` 只是发出进程内的序号，必须与该进程 `sid` 一起绑定，并已用“同号不同 `sid`”的负例回归钉住。原远端超时仍按未定因记录，本修订不声称已定位。
- **`#90` / PR `#91` 与 `#94` / PR `#95` 均已合入 `main`，`main` run `45932680368` 通过（`push` / `success`，head `32e0109`）。**
- **`#88` / PR `#89` 在办（CI-C 三平台动态矩阵）。** 首轮 run `45914578287`（attempt 1）失败于 `macos-15` ARM64 的 Clippy 与 `ubuntu-24.04` 的 `soxu-git` Diff 集测，两处根因分别由 `#90` / PR `#91`（`354fbd7`）修复。`#94` / PR `#95`（`32e0109`）闭合宿主接入侧的 Darwin 测试导入后，`#88` 重跑 run `45934505956`（attempt 1，检出 `fc5d9034`，Darwin job `140294571285`）仍失败于 `crates/soxu-magpie/tests/worktree_context.rs` 的两项 `unused import`，同 run 的 Ubuntu 治理、Linux 与 PWA 已通过，`CI Gate` 因 Darwin 失败与缺 test 阶段正确拒绝；该面由 `#96` 热修复承接。本看板由主控协调各分支合流，不互相覆盖。
- **CI 波次后续顺序**：CI-C 承接三平台动态矩阵的实际 job 执行与证据聚合，CI-D 后接；CI-E/F 与 CI-D 写入面正交。未经核验的原生测试与正式发布仍列为待办。
- **M3 两份设计已合入 `main`。** `#57` 的显式 Init/Clone 与损坏恢复二次授权见 [仓库初始化、克隆与工作树恢复](../dev/git-init-clone-and-recovery.md)。`#58` 的轮次 Checkpoint 快照、先建 Safety Snapshot 再 Rewind 和文件级结构化提交见 [检查点、回退与结构化提交](../dev/git-checkpoint-rewind-commit.md)。
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
| **M2** | Magpie Discovery & 7 Native CLI Drivers | M2h、M2j 已完成，M2i 隔离只读快聊已合入 | 六种原生只读 CLI 已合入，Pi 原生 `Queue` / `Steer` / `Stop` 已验证。Antigravity 的 Daemon 托管快聊已由 PR `#55` 合入，项目目录入口及七代理全验收仍待完成。 |
| **M3** | Git Worktree Pool, Checkpoints & Commit Engine | M3a–M3d、M3e 注册和检出、宿主接入与两份设计已合入 | `#49` 受控注册与只读诊断、`#51` 固定提交检出与失败禁用已合入。`#56` 交付宿主登记、会话就绪门禁与 `direct` MCP 上下文准备。`#57`、`#58` 已完成设计审定。`#63`–`#65`（PR `#66`–`#68`）补齐流程治理、M3 Git 公共契约与 Checkpoint 底座。`#69` / PR `#73` 已合入 Worktree 只读原生会话联调；`#71` / PR `#84` 已合入显式受控 Init/Clone；恢复预览、Checkpoint、Rewind、结构化提交执行与 `direct` MCP 运行时挂载仍待实现。Linux 真机端到端链路待实测。 |
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
   - **Pi 原生控制（`#32`，已合入）**：通过原生 RPC 实现 Queue、Steer 和 Stop，并保持本机只读对话权限。Antigravity（`#26`）隔离只读 `QuickChat` 已提交 PR `#55` 审查。
3. **M3 — Git 隔离池、轮次检查点与结构化提交**：
   - **M3a（`#33`，已合入）**：通过本机 Git 只读获取工作树根、独立与共享 Git 目录、HEAD/分支及 `status --porcelain=v2 -z`。
   - **M3b（`#39` / PR `#41`，已合入 `7cea1c4`）**：只读项目身份已交付。
   - **M3c（`#42` / PR `#45`，已合入 `4c5d706`）**：提供 Worktree 只读注册清单，保留原始路径、locked/prunable 原因及 `Unavailable` HEAD 状态。
   - **M3d（`#46` / PR `#50`，已合入 `0d0b4d9`）**：交付 `soxu-git` 内四基准只读 Diff、精确已跟踪路径授权、有界元数据及单文件审阅。
   - **M3e（`#49` → `#51` → `#52`，设计见 `#47` / PR `#48`）**：`#49` / PR `#53` 已合入，终态为「已注册，待检出」。`#51` 在同一协调域核验固定提交，受控检出并保留失败禁用记录，已由 PR `#54` 合入。第三阶段由 `#56` / PR `#61` 实现宿主登记与 `direct` MCP 上下文准备；Worktree 只读原生会话联调已由 `#69` / PR `#73` 合入，显式受控 Init/Clone 已由 `#71` / PR `#84` 合入；`direct` MCP 运行时挂载待后续交付。详见 [Worktree 生命周期设计](../dev/git-worktree-lifecycle.md)。
   - **后续依赖**：受控创建和诊断恢复承接 Worktree 清单及身份模型。Checkpoint 基准 Diff 依赖快照模型，回退必须先留可恢复快照，提交执行必须经过结构化预检。公共契约单独排期，先冻结再接入跨模块实现。
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

<a name="31-当前-pi--git-并行波次2026-11-25-北京时间盘点"></a>

### 3.1 Pi / Git 波次收口与在办工作区

| Issue / 交付 | 已合入 PR | 主线提交 / CI | Paseo 工作区 |
| :--- | :--- | :--- | :--- |
| [#34](https://github.com/TanCan1998/Soxu/issues/34) 并行治理与 AGY 延期 | [#35](https://github.com/TanCan1998/Soxu/pull/35) | [`01f1b99`](https://github.com/TanCan1998/Soxu/actions/runs/45610093472) | `wks_540110c51aea6f11` |
| [#32](https://github.com/TanCan1998/Soxu/issues/32) M2j Pi 原生纠偏 | [#37](https://github.com/TanCan1998/Soxu/pull/37) | [`a1ab5e4`](https://github.com/TanCan1998/Soxu/actions/runs/45611231881) | `wks_b7f3663147388206` |
| [#33](https://github.com/TanCan1998/Soxu/issues/33) M3a Git 只读探测 | [#36](https://github.com/TanCan1998/Soxu/pull/36) | [`404c307`](https://github.com/TanCan1998/Soxu/actions/runs/45613472814) | `wks_8f142f393281bfad` |

三项 Issue 均已关闭，表中三个工作区均已归档，对应的本地和远端分支均已回收，三次合入后的 `main` CI 均通过。

`#38` 交接已由 PR `#40` 合入。`#42` 的 Worktree 清单已由 PR `#45` 合入 `main`（`4c5d706`）。`#43` 的只读 Diff 设计已由 PR `#44` 合入（`63acf15`）。`#47` 的 Worktree 受控创建与只读诊断设计已由 PR `#48` 合入（`12ea672`）。

| Issue | 工作区 / Agent | 写入范围与合入顺序 |
| :--- | :--- | :--- |
| [#51](https://github.com/TanCan1998/Soxu/issues/51) M3e 已注册 Worktree 的安全检出 | `feat/51-worktree-checkout` | `soxu-git` 检出接口与测试、crate README、本看板及 AOCI。Windows 保留目录身份拒绝，Linux 负责受控发布。已由 PR `#54` 合入 `main`。 |
| [#26](https://github.com/TanCan1998/Soxu/issues/26) Antigravity 隔离只读快聊 | `feat/26-agy-quickchat` | [PR #55](https://github.com/TanCan1998/Soxu/pull/55) 复验与远端 CI 通过，合入 `63882d1`，工作区已归档。 |
| [#56](https://github.com/TanCan1998/Soxu/issues/56) M3e 宿主 Worktree 接入 | `feat/56-m3e-host-worktree-wiring` | [PR #61](https://github.com/TanCan1998/Soxu/pull/61) 基于先前合入的 PR `#62` Diff 测试修复，复验与远端 CI 通过，合入 `0b0317b`，工作区已归档。 |
| Diff 同秒 OID 夹具修复 | `fix/diff-same-second-oid` | [PR #62](https://github.com/TanCan1998/Soxu/pull/62) 复验与远端 CI 通过，合入 `810b670`，工作区已归档。 |

Soxu Paseo 项目为 `prj_24ffa03410264684`，主控工作区为 `wks_21203a9ef0f5e2e5`（`main`）。执行代理完成各自验证、推送和建 PR，主控负责审查、合入与归档。

#### 已收口波次 `wave-m3-next-20261126`（三路已全部合入 `main` 并归档）

| Issue / 任务 | 分支 / Paseo 工作区 | Owned Paths |
| :--- | :--- | :--- |
| `#63` 流程治理与看板刷新 | `docs/63-subagent-escalation-protocol` / `wks_97829a725294c410` | `AGENTS.md` 第三节第 4 条、`.agents/skills/paseo-orchestrator/SKILL.md`、`docs/ops/PROGRESS.md`、AOCI |
| `#64` M3 公共契约前置 PR | `feat/64-m3-git-ops-protocol` / `wks_525e64aa4ead7ae8` | `soxu-protocol` 新增 `git_ops.rs`、错误处理、RPC、Event 与测试，`soxu-daemon/src/state.rs` 的 `Unsupported` 占位、AOCI |
| `#65` M3 轮次 Checkpoint 私有索引快照与覆盖清单底座 | `feat/65-git-checkpoint-foundation` / `wks_99d046b24b5bcc95` | `soxu-git/src/checkpoint*`、`soxu-git/src/lib.rs` 仅新增导出、`soxu-git/tests/checkpoint.rs`、`soxu-git/README.md`、AOCI |

三路已按 **`#63` → `#64` → `#65`** 顺序合入：`#63` / PR `#66`（`02daacf`）、`#64` / PR `#67`（`5225eca`）、`#65` / PR `#68`（`21cf871`），三次合入的远端 CI 均通过，三个工作区均已归档、本地与远端分支已回收。三路业务写入范围正交，AOCI 按分支维护并由主控在合流后重新对齐。

#### 已收口波次 `wave-m3-next2-20261126`（三路已全部合入 `main` 并归档）

| Issue / 任务 | 分支 / Paseo 工作区 | 合入 PR / 提交 | Owned Paths |
| :--- | :--- | :--- | :--- |
| `#69` Worktree 原生只读会话联调与看板刷新 | `feat/69-worktree-native-session-wiring` / `wks_42f8f89cfbb8c36d` | PR `#73` / `88417b3` | `soxu-daemon/src/state/native.rs`、`soxu-daemon/src/state/worktrees.rs`、`soxu-daemon/tests/worktree_lifecycle.rs`、本看板、AOCI |
| `#70` M3 固定端点 Diff | `feat/70-git-fixed-endpoint-diff` / `wks_1f218f3956c3cdab` | PR `#72` / `d3afda4` | `soxu-git` 固定端点 Diff 实现与测试、crate README、AOCI |
| `#71` M3 显式仓库创建 | `feat/71-git-repository-create` / `wks_19a9523a8249b8ac` | PR `#84` / `2f47c82` | `soxu-git` 受控 Init/Clone 与测试、crate README、AOCI |

三路均通过远端 CI 合入 `main`，工作区已归档、本地与远端分支已回收。`#69` 的 WSL `Ubuntu-26.04` 真机收尾验证（Rust `1.99.1`、独立 `CARGO_TARGET_DIR`）记录在 3.2 节；其原生二进制与相关原生测试未在本轮门禁中复跑，仍列为待办。

#### 在办波次：CI 抗压（`#83` / PR `#85`、`#86` / PR `#87` 已合入）

| Issue / 任务 | 分支 / Paseo 工作区 | 状态 | Owned Paths |
| :--- | :--- | :--- | :--- |
| `#83` / PR `#85` CI-A 变更范围门禁与发布契约 | `feat/83-ci-a-strict-gate` / `wks_87a6a683f132b176` | 已合入 `a86024e`，工作区已归档 | `.github/workflows/ci.yml`、`scripts/verify-ci-gate.ps1`、`scripts/test-ci-gate.ps1`、AOCI |
| `#86` / PR `#87` CI-B 范围检测、Cargo Metadata 与测试计划 | `feat/86-ci-b-change-scope` / `wks_8bf43dd1b64bca90` | 已合入 `8c01a533`，工作区已归档 | `scripts/detect-check-scope.ps1`、`scripts/test-check-scope.ps1`、`scripts/check-all.ps1`、`scripts/verify-ci-gate.ps1`、`scripts/test-ci-gate.ps1`、`ci.yml` 检测器接线、本看板、AOCI |
| `#88` / PR `#89` CI-C 三平台动态矩阵与证据聚合 | `feat/88-ci-c-platform-matrix` | 在办；首轮 run `45914578287`（attempt 1）失败后由 `#90` / PR `#91` 与 `#94` / PR `#95` 两次热修复闭合，重跑 run `45934505956`（attempt 1）仍失败于 `soxu-magpie` 测试导入，等 `#96` 合入后再次重跑 | `.github/workflows/ci.yml`、CI 证据聚合脚本、`scripts/check-all.ps1`、本看板、AOCI |

#### 已收口热修复：原生平台矩阵冻结源码（`#90`）

| Issue / 任务 | 分支 / Paseo 工作区 | 状态 | Owned Paths |
| :--- | :--- | :--- | :--- |
| `#90` 原生矩阵冻结源码热修复 | `fix/native-matrix-blockers` / `wks_4e0ff3d28c954665` | 已合入 `main` `354fbd7b52f2a27db111541ef3da522d3a770b09`（PR `#91`，`2026-11-27T02:09:51Z`），工作区待回收 | `soxu-git/src/repository_create/platform.rs`（最小条件编译/告警修正）、`soxu-git/tests/diff.rs`（夹具隔离、有界维护回归）、本看板、`LESSONS.md`、本分支 AOCI |

#### 已收口热修复：Darwin Daemon 测试导入（`#94`）

| Issue / 任务 | 分支 / Paseo 工作区 | 状态 | Owned Paths |
| :--- | :--- | :--- | :--- |
| `#94` Darwin Daemon 测试导入热修复 | `fix/94-daemon-darwin-test-imports` / `wks_fa74a88079bc342e` | 已合入 `main` `32e0109737d35a2a0ac82815897e2358c9d467cc`（PR `#95`，`2026-11-27T03:11:12Z`），`main` run `45932680368` 通过；Darwin 原生回退证据仍待真实 `macos-15` Runner 补齐 | `crates/soxu-daemon/tests/worktree_lifecycle.rs`（最小平台导入）、本看板、`LESSONS.md` 的 `L-013`、本分支 AOCI |

`#89` head `3db8969` 的 run `45929401718`（attempt 1，Darwin job `140278345298`）在 `Run Clippy` 报 `crates/soxu-daemon/tests/worktree_lifecycle.rs` 三处 `unused import`：`state::WorktreeAdmissionRequest`、`soxu_git::GitProbe`、`WORKTREE_CONTEXT_FILE`，测试步骤 skipped（证据记录为 `build_failed`，并非测试失败或原生通过）。三个符号的**全部**使用点只落在 `#[cfg(windows)]` 的系统拒绝用例与 `#[cfg(target_os = "linux")]` 的 `linux` 模块内，macOS 编译面一处也没有。修复保持最小：把三者从顶层无条件 `use` 移出，改为与用例同口径的 `#[cfg(any(windows, target_os = "linux"))]` 单条导入，测试体、断言、`Unsupported`/`UnsupportedFilesystem` 与权限边界一字未改，未加 `#[allow]`、未删用例、未屏蔽任何测试目标。

#### 在办热修复：Darwin Magpie 测试导入（`#96`）

| Issue / 任务 | 分支 / Paseo 工作区 | 状态 | Owned Paths |
| :--- | :--- | :--- | :--- |
| `#96` Darwin Magpie 测试导入热修复 | `fix/96-magpie-darwin-test-imports` / `wks_892f5dddf5b05a63` | 本地候选已就绪，待主控独立复验与锁 SHA 合入（与 `#88` 并行，仅动测试文件导入面） | `crates/soxu-magpie/tests/worktree_context.rs`（最小平台导入）、本看板、`LESSONS.md` 的 `L-013`、本分支 AOCI |

`#89` head `df2849aa` 的 run `45934505956`（attempt 1，检出 `fc5d9034`，Darwin job `140294571285`）在 `macos-15` ARM64 的 `Run Clippy` 报 `crates/soxu-magpie/tests/worktree_context.rs` 两项 `unused import`：`WORKTREE_CONTEXT_FILE` 与 `WorktreeContextServer`，测试步骤 skipped（证据为构建失败，既不计为测试失败，也不构成 Darwin 原生通过）。两者使用点全部落在 `#[cfg(any(windows, target_os = "linux"))]` 的三个用例内（`WORKTREE_CONTEXT_FILE` 三处、`WorktreeContextServer` 三处），macOS 编译面零使用点。修复保持最小：从顶层无条件 `use` 移出，改为同口径 `#[cfg(any(windows, target_os = "linux"))]` 单条导入，测试体与断言一字未改，可在 Darwin 编译的 `native_and_shared_routes_do_not_create_assets` 与 `rejects_a_link_in_the_authorized_directory_chain` 保持无条件，未加 `#[allow]`、未删用例、未屏蔽任何测试目标。

#### 在办热修复：Darwin 原生资产集测临时根（`#98`）

| Issue / 任务 | 分支 / Paseo 工作区 | 状态 | Owned Paths |
| :--- | :--- | :--- | :--- |
| `#98` 原生资产集测临时根修正 | `fix/98-native-asset-fixture-root` / `wks_c7a695bacda8d8e3` | 本地候选就绪，待主控独立复验与锁 SHA 合入（与 `#88` 并行；不改生产 Guard、CI 与公共契约） | 测试自建夹具根：`soxu-daemon/tests/{native_assets,rpc_ws_e2e}.rs` 与新增 `soxu-daemon/tests/common/mod.rs`、`soxu-daemon/src/state/worktrees.rs` 的 `#[cfg(test)]` 用例、`soxu-drivers/tests/native.rs::Scratch`、`soxu-git/tests/repository_create.rs::Fixture`、`soxu-git/src/repository_create/{checkout,publish}.rs` 的 `#[cfg(test)]` 子模块、`soxu-magpie/tests/worktree_context.rs::Fixture`、本看板、`LESSONS.md` 的 `L-018`、本分支 AOCI |

`#89` run `45940068623`（attempt 1，`macos-15` ARM64，job `140312147432`）在 `soxu-daemon --test native_assets` 出现 1 通过 / 5 失败，测试退出码 `101`，失败点全在 `NativeAssetGuard::new` → `checked_directory` 报「目录或祖先不是普通目录」。根因为**测试自建夹具根取自未规范化的系统临时目录**：守卫按设计拒绝任何含符号链接的祖先，于是正向构造先被拒绝，负向断言根本没有到达被测分支（同套件唯一“通过”的 `rejects_relative_escape_duplicate_empty_and_oversized_declarations` 属此类未到分支的伪通过）。修复只把测试自己创建的夹具根、以及 `src/*/#[cfg(test)]` 内的临时根改为取自规范化真实路径，再由同一真实根派生资产、启动目录与每次重验目标；有意构造的资产链接、祖先链接、junction、目录替换、路径逃逸一律原样交给被测函数。

同一根因在 `TMPDIR` 指向受控 symlink 时于 Linux 全量复现，落到 7 个测试目标：`soxu-daemon --lib`（1 失败）、`soxu-daemon --test native_assets`（5 失败）、`soxu-daemon --test rpc_ws_e2e`（2 失败）、`soxu-drivers --test native`（1 失败）、`soxu-git --lib`（5 失败）、`soxu-git --test repository_create`（27 失败）、`soxu-magpie --test worktree_context`（5 失败）。`soxu-daemon --test rpc_ws_e2e` 的两项只是 Codex 守卫用例（`spawn_guarded_daemon` 与两个 `native_asset_guard_rechecks_*`）：AGY 用例在别名条件下本来就通过，因为私有 guard 分支经 `antigravity::quick_chat_directory(guard, chat_id)` 直接取 guard 根下的 `quick-chats`，不经过 `runtime.quick_chat_root`，故 `spawn_antigravity_daemon` 保持原样零改动。

#### 在办热修复：Darwin 非法字节路径夹具（`#100`）

| Issue / 任务 | 分支 / Paseo 工作区 | 状态 | Owned Paths |
| :--- | :--- | :--- | :--- |
| `#100` Darwin 非法字节路径夹具门控 | `fix/100-darwin-raw-path-test` / `wks_6756d9ae8127d9ec` | 本地候选就绪，待主控独立复验与锁 SHA 合入（与 `#88` 并行，只动 `soxu-git` 的 Diff 测试夹具门控） | `crates/soxu-git/tests/diff.rs`（最小平台门控）、本看板、`LESSONS.md` 的 `L-019`、本分支 AOCI |

`#89` head `b8ca5fcd` 的 run `45951887087`（`macos-15` ARM64）在 `crates/soxu-git/tests/diff.rs::unix_preserves_raw_linebreak_name_and_refuses_a_replaced_symlink` 的 `fs::write` 报 `EILSEQ`（Illegal byte sequence）：夹具文件名 `line\nbyte\xff` 含非法 UTF-8 字节，Darwin 内核文件系统 API 只接受合法 UTF-8 名字，而该用例语义（原始字节路径无损 + 被替换符号链接被拒绝）只在 Linux 成立；原 `#[cfg(unix)]` 把 Darwin 误纳入编译面。

修复保持最小：仅把该用例改为 `#[cfg(target_os = "linux")]`，沿用仓库具体目标门控惯例（`checkpoint.rs::linux_rejects_links_fifos_nested_repositories_and_preserves_raw_path_bytes`、`repository_create.rs` 的 `#[cfg(target_os = "linux")]`），同步改名为 `linux_preserves_raw_linebreak_name_and_refuses_a_replaced_symlink` 并补平台能力注释；测试体、断言、`Fixture` 夹具与生产实现一字未改，未加 `#[allow]`、未删用例、未屏蔽测试目标或整个文件。Linux 仍编译执行并保留原始字节路径与替换链接拒绝断言，Windows 仍不编译该用例。

**同因未覆盖风险（已上报主控裁决，本轮零改动）**：同一 `soxu-git` 集测里另有两处 `#[cfg(unix)]` 用例同样在磁盘上创建非法 UTF-8 名字——`tests/probe.rs::preserves_non_utf8_and_newline_repository_paths`（目录 `repo\n\xff`、文件 `file\n\xfe`）与 `tests/worktrees.rs::native_unix_newline_and_non_utf8_worktree_paths_remain_lossless`（目录 `raw-\xff`）。CI 的 `cargo test --workspace --locked` 不带 `--no-fail-fast`，`diff` 目标失败会中止其后的目标（本机 Linux 实测目标顺序为 `checkpoint` → `diff` → `fixed_diff` → `probe` → `project` → `repository_create` → `status` → `worktree_checkout` → `worktree_lifecycle` → `worktrees`），因此这两处是 `#88` 下一次 Darwin 原生运行的同因失败点。二者不在 `#100` 的 Owned Paths 内，是否并入同一 PR 由主控裁决。

### 3.2 已合入交付与验收证据边界

- `#100` Darwin 非法字节路径夹具门控的本地验收边界（基线 `7cccd0d`，与 `#88` 并行，只动 `soxu-git` 的 Diff 测试夹具门控，生产实现 / CI / 公共契约零改动）：
  - **Windows**（Rust `1.99.1`，独立 `CARGO_TARGET_DIR=F:/workplace/Paseo/work/.cargo-target/soxu-100-darwin-raw-path-test`）：`cargo fmt --all -- --check` `exit 0`；`cargo test -p soxu-git --test diff --locked` `15 passed / 0 failed` `exit 0`（13 项无条件 + `#[cfg(windows)]` 的 `rejects_windows_ambiguous_names_before_git` 与 `an_unselected_file_can_be_exclusive_locked_without_blocking_diff`；`#[cfg(target_os = "linux")]` 用例不在 Windows 编译面）；`pwsh -NoProfile -File scripts/check-all.ps1 -BaseRef origin/main` `exit 0`（`141s`）。
  - **WSL `Ubuntu 26.04`（x86_64）**（独立 `CARGO_TARGET_DIR=/root/.cache/soxu-100-darwin-raw-path-test`，实体 `/root/.rustup/toolchains/1.99.1-x86_64-unknown-linux-gnu/bin` 与实体 `node 26.13.1` `bin` 前置于 `PATH`，未执行 `mise trust`，未在 WSL 内对 Windows `.git` 工作树跑 `git`）：`cargo test --workspace --locked --no-fail-fast --offline` `exit 0`，`40` 个套件全通过、`225 passed / 0 failed`；`tests/diff.rs` 目标 `14 passed`，其中 `linux_preserves_raw_linebreak_name_and_refuses_a_replaced_symlink ... ok`，证明 Linux 仍真实执行原始字节路径与替换符号链接拒绝断言。
  - **Darwin 交叉（编译面）**：`cargo clippy -p soxu-git --all-targets --target aarch64-apple-darwin --locked -- -D warnings` `exit 0`（`17.11s`，0 error / 0 warning）。该交叉只证明 `soxu-git` 在 `aarch64-apple-darwin` 编译面（含测试目标）零告警，且该用例因 `#[cfg(target_os = "linux")]` 不进入 Darwin 编译面；本机无 Darwin 链接器/SDK，无法交叉链接或运行 Darwin 测试二进制，**不构成** `macos-15` ARM64 原生通过。真实 Darwin 原生回退证据（原 `EILSEQ` + 修复后同目标原生测试继续执行）待 `#88` 合流后的真实 Runner 补齐。
  - **三处证据互不等价**：Windows `15 passed` 与 Linux `14 passed` 只证明各自编译面与运行面；Darwin 交叉 `exit 0` 只证明编译面干净。三者都不等于 macOS ARM64 原生已运行该集成测试。


- `#98` Darwin 原生资产集测临时根修正的本地验收边界（基线 `e6ea5ba8b79ca9a7ad40223bdeb1ac043c2e4141`，与 `#88` 并行，只动测试夹具根与 `#[cfg(test)]` 内的临时根，生产 Guard / 路径检查 / CI / 公共契约零改动）：
  - **Linux 对照口径**（WSL `Ubuntu 26.04`，实体 `/root/.rustup/toolchains/1.99.1-x86_64-unknown-linux-gnu/bin` 与实体 `node 26.13.1` `bin`，未执行 `mise trust`，未在 WSL 内对宿主 `.git` 跑 `git`）：唯一变量是隔离子进程 `TMPDIR`，`/tmp/<x>/alias` 为指向同一普通目录 `/tmp/<x>/real` 的受控 symlink；命令固定为 `cargo test --workspace --locked --no-fail-fast`。基线 `e6ea5ba`：普通目录 `27.41s` `exit 0`，别名 `45.42s` `exit 101`（7 目标失败）；修复后：普通目录 `25s` `exit 0`、40 个套件全通过，别名 `24s` `exit 0`、同样 40 套件全通过、0 失败套件（重复一轮为 `27s` / `25s`，结论一致）。
  - **逐目标计数（别名条件，基线 → 修复后）**：`soxu-daemon --lib` `1 passed / 1 failed` → `2 passed / 0 failed`；`--test native_assets` `1 passed / 5 failed` → `6 passed / 0 failed`；`--test rpc_ws_e2e` `14 passed / 2 failed / 3 ignored` → `16 passed / 0 failed / 3 ignored`；`soxu-drivers --test native` `8 passed / 1 failed / 2 ignored` → `9 passed / 0 failed / 2 ignored`（Linux 编译面；Windows 为 `10 passed`，多一个 `#[cfg(windows)]` 用例）；`soxu-git --lib` `22 passed / 5 failed`（共 27 项） → `27 passed / 0 failed`；`soxu-git --test repository_create` `1 passed / 27 failed`（共 28 项） → `28 passed / 0 failed`；`soxu-magpie --test worktree_context` `1 passed / 5 failed`（共 6 项） → `6 passed / 0 failed`。
  - **Windows 专项计数**：`native_assets` `6 passed`（含 `#[cfg(windows)]` 的 junction 用例）、`rpc_ws_e2e` `16 passed / 3 ignored`、`soxu-daemon --lib` `2 passed`、`soxu-git --lib` `22 passed`、`soxu-git --test repository_create` `2 passed`（其余用例为 Linux 门控）、`soxu-magpie --test worktree_context` `5 passed`、`soxu-drivers --test native` `10 passed / 2 ignored`。
  - **Windows**（Rust `1.99.1`，Node `26.13.1`，独立 `CARGO_TARGET_DIR=F:/workplace/Paseo/work/.cargo-target/soxu-98-native-asset-fixture-root`）：`cargo fmt --all -- --check` `exit 0`；专项测试计数见下条；`pwsh -NoProfile -File scripts/check-all.ps1 -BaseRef origin/main` `exit 0`，范围 `soxu-cli,soxu-daemon,soxu-drivers,soxu-git,soxu-magpie`，`[1/7]`、`[4/7]` fmt、`[5/7]` Clippy、`[6/7]` 测试实跑，`[2/7]`、`[3/7]`、`[7/7]` 按计划跳过（未触及前端与设计系统）。
  - **Windows 平台约束（实测）**：`canonicalize` 在 Windows 附加 `\\?\` 扩展路径前缀，实测会让 `soxu-drivers --test native` 由 `10 passed` 退化为 `4 passed / 6 failed`（Node 夹具 `process.cwd()` 的物理路径与 `SOXU_FIXTURE_CWD` 字符串不再相等，子进程断言退出）。因此规范化只在 Unix 生效，Windows 一律保留 `TempDir`/临时目录的既有路径，`cmd mklink /J` 的 junction 用例与 Node cwd 契约均按原样运行。
  - **Darwin 证据边界**：`macos-15` ARM64 的失败侧有远端原生日志与工件（失败集合、`checked_directory` 拒绝原因、退出码 `101`），但 Runner 上系统临时目录的具体别名形态**没有直接留证**（日志只记录拒绝原因，不含被抓取路径）。本修正的可泛化结论仅为「守卫要求夹具根取自真实路径」；真实 Darwin 修复后完整验证仍归主控合流 `#88` 后的原生 Runner，不以 Linux 别名复现替代。

- `#94` Darwin Daemon 测试导入热修复的本地验收边界（基线 `354fbd7`，已含 `#90` 的原生矩阵热修复；与 `#88` 并行，仅动测试文件导入面）：
  - **Windows**（Rust `1.99.1`，独立 `CARGO_TARGET_DIR=F:/workplace/Paseo/work/.cargo-target/soxu-94-daemon-darwin`）：`cargo fmt --all -- --check` 与 `cargo clippy -p soxu-daemon --all-targets --locked -- -D warnings` 均 `exit 0`；`cargo test -p soxu-daemon --test worktree_lifecycle --locked` `3 passed`（`2` 无条件 + `1` 个 `#[cfg(windows)]` 用例）；`pwsh -NoProfile -File scripts/check-all.ps1 -BaseRef origin/main` `exit 0`，按所选范围各步均通过：`[1/7]` 提交历史与隐私、`[4/7]` fmt、`[5/7]` 增量 Clippy、`[6/7]` 增量测试实际执行，`[2/7]` 设计令牌同源、`[3/7]` 设计规范与 Paseo 编排回归、`[7/7]` 移动端 Web/PWA 构建均**按计划跳过**（未触及对应前端、设计系统与脚本）。
  - **WSL `Ubuntu 26.04.1`**（x86_64，独立 `CARGO_TARGET_DIR=/root/.cache/soxu-94-daemon-darwin`）：因 WSL 侧 `mise.toml` 未 trust，改用实体工具链 `/root/.rustup/toolchains/1.99.1-x86_64-unknown-linux-gnu/bin` 与实体 `node 26.13.1` `bin`（`rustc`/`cargo` 均为 `1.99.1`），未执行 `mise trust`；`cargo clippy -p soxu-daemon --all-targets --locked -- -D warnings` `exit 0`（`19s`），`worktree_lifecycle` `8 passed`（`2` 无条件 + `6` 个 `linux` 模块用例），均以 `${PIPESTATUS[0]}` 取真实退出码；未在 WSL 内对 Windows `.git` 工作树执行 `git`。该轮范围为主控指定的 `soxu-daemon` 包，主控的全 workspace 验证另记。
  - **Darwin：交叉未完成，真机未验。** `cargo clippy --target aarch64-apple-darwin -p soxu-daemon --all-targets --locked -- -D warnings` `exit 101`，停在 `libsqlite3-sys` 的 `cc-rs`：`failed to find tool "cc": program not found`。这是 C 工具链/SDK 边界，**不是**测试目标 Clippy 已通过的证据；本轮未装 SDK 或交叉编译器、未改依赖，亦不描述为原生通过。真实 Darwin 回退证据（原错误 + 修复后同一目标 Clippy）待 `#88` 合流后的 `macos-15` ARM64 Runner 补齐。
  - **双平台通过不等于 Darwin 通过**：macOS 编译面保留的是无条件顶层代码，不含 `#[cfg(windows)]` 用例与 `#[cfg(target_os = "linux")]` 模块。Windows 与 Linux 的 `--all-targets` Clippy 全绿只证明各自编译面干净，**不**是 Darwin lint 证据；对三符号逐处比对使用点（确认其使用点只落在已门控的用例与模块内）只是定位手段，不能替代 Darwin 原生验证。

- `#96` Darwin Magpie 测试导入热修复的本地验收边界（基线 `32e0109`，已含 `#90` / PR `#91` 与 `#94` / PR `#95` 两次合入；与 `#88` 并行，仅动测试文件导入面）：
  - **Windows**（Rust `1.99.1`，独立 `CARGO_TARGET_DIR=F:/workplace/Paseo/work/.cargo-target/soxu-96-magpie-darwin`）：`cargo fmt --all --check` `exit 0`；`cargo test -p soxu-magpie --test worktree_context --locked` `5 passed`（`1` 个无条件用例 + `3` 个 `#[cfg(any(windows, target_os = "linux"))]` 用例 + `1` 个 `#[cfg(any(unix, windows))]` 用例；`#[cfg(target_os = "linux")]` 的 `a_replaced_root_cannot_receive_a_manifest` 不在 Windows 编译面）；`pwsh -NoProfile -File scripts/check-all.ps1 -BaseRef origin/main` `exit 0`（执行代理本轮 `20.66s`；主控独立复跑 `62.63s` `exit 0`，覆盖 `soxu-cli`、`soxu-daemon`、`soxu-magpie`，`41 passed / 0 failed / 3 ignored`，`worktree_context` `5` 项），门禁步骤 `[1/7]`、`[4/7]`、`[5/7]`、`[6/7]` 实跑，`[2/7]`、`[3/7]`、`[7/7]` 按计划跳过（未触及前端与设计系统）。
  - **Darwin 交叉**（复用本机已装 `aarch64-apple-darwin` target，未装 SDK、未改依赖）：修复前 `cargo clippy --workspace --exclude soxu-daemon --all-targets --target aarch64-apple-darwin --locked -- -D warnings` `exit 101`（`15.47s`；一条 `unused imports` 诊断覆盖 `WORKTREE_CONTEXT_FILE` 与 `WorktreeContextServer` 两个符号，位于 `worktree_context.rs:2:22`，随后 `cargo` 报编译失败摘要）；修复后同命令 `exit 0`（`0.86s`，`0` error / `0` warning）。排除 `soxu-daemon` 的原因是本机缺 Darwin C 编译器/SDK，其 `libsqlite3-sys` 会停在 `cc-rs: failed to find tool "cc"`（`#94` 已实测），本次交叉因此**不覆盖** `soxu-daemon`；全仓原生 Clippy 与 Darwin 冒烟仍归 `#88` 真机 Runner。
  - **WSL `Ubuntu 26.04`（x86_64）**（独立 `CARGO_TARGET_DIR=/root/.cache/soxu-96-magpie-darwin`，实体 `/root/.cargo/bin` 工具链，未执行 `mise trust`）：`cargo clippy -p soxu-magpie --all-targets --locked -- -D warnings` `exit 0`（`8s`）；`cargo test -p soxu-magpie --test worktree_context --locked` `6 passed`（含 `#[cfg(target_os = "linux")]` 用例），均以 `${PIPESTATUS[0]}` 取真实退出码；未在 WSL 内对 Windows `.git` 工作树执行 `git`。
  - **三处证据互不等价**：Windows `5 passed` 与 Linux `6 passed` 只证明各自编译面与运行面，Darwin 交叉 `exit 0` 只证明 `--workspace --exclude soxu-daemon --all-targets` 在 `aarch64-apple-darwin` 编译面零告警；三者都**不**等于 `macos-15` ARM64 原生 Runner 已运行该集成测试。

- M3b（`#39` / PR `#41`）已合入 `7cea1c4`，交付 `GitProbe::open_local_project`，规范化显式绝对目录，返回 `LocalProject::Directory` 或 `LocalProject::GitWorktree(WorktreeIdentity)`。根与子目录归于同一工作树；linked worktree 保留独立工作树根、Git 目录及共享 common dir。bare、损坏仓库、Git 元数据目录和权限错误不得降级普通目录。打开仅查身份，不扫描完整 status，不初始化、克隆、创建工作树或写入。`cargo test -p soxu-git` 25 项专项通过（其中本地项目夹具 5 项），根依赖、协议和 M3a 公开契约保持不变。

- M3c（`#42` / PR `#45`，已合入 `4c5d706`）清单全批解析 NUL 分隔记录，校验 ref 和路径，保留注解原始字节。缺失 HEAD 时返回零 OID 与 `Unavailable`。Git 自身检查注册路径以判断 prunable。该波 Git 专项 35 项及执行端、主控各自的七项门禁均通过。

- M3d（`#46`）在 Windows Git `2.56.0.windows.4` 上专项 `cargo test -p soxu-git --locked` 通过 49 项，含 13 项 Diff 集成夹具，主控独立复跑结果一致。独立 `target/soxu-46-independent` 的七项门禁通过，冻结预览 SHA-256 一致。配置发现对照、linked 独立 index、基准树独有删除、未授权文件独占锁和 50,000 行输出限额均有夹具证据。配置、index、对象及夹具文件字节和 mtime 复核无变化。Unix 专属真实路径与链接用例由跨平台门禁验证。

- M3e（`#49`，`pi-takeover-20261126-01`）：已吸收 PR `#52`（main `de65f12`），闭合冻结范围内的启动前 `Started` 留痕、spawn 失败分型、`SOXUWT3` 身份持久化与旧状态待复核控制流；采用宿主唯一持久协调域。Windows 专项与隔离 target 七项门禁通过；另有原生测试二进制验证 spawn/退出码/输出限额/超时回收，不依赖 Windows 目录后端。Windows 注册仍为 `UnsupportedFilesystem`，原生句柄身份和持续 DACL 授权需独立决策。Unix 专项本轮未运行：WSL 启动返回 `Wsl/Service/CreateInstance/CreateVm/HCS/ERROR_BAD_EXE_FORMAT`，不以源码覆盖代替实测。AOCI 收尾与 PR 交主控审查，执行代理不合并或回收；`#51` 继续等待 `#49` 合入。

- M2c 已通过 PR `#16` 合入 `main`，隔离工作区已归档。夹具覆盖 HTTP/WS 增量、SQLite 回读、FIFO、取消、原生失败、重启恢复，以及只读和浏览器来源门禁。
- `#11` 的七项测试及 Windows 只读烟测通过。`#12` 的七种流式格式通过夹具验证，Codex 无项目和本地目录的真实对话通过登录烟测，原生 Resume/Fork 通过回环测试。各驱动的验证范围见 [`soxu-drivers/README.md`](../../crates/soxu-drivers/README.md)。
- `#15` 使用真实 Codex `0.172.0`、隔离 `CODEX_HOME` 和本机 Responses 夹具，验证两类目录均能经 Daemon 收到固定回复并落盘。在线登录推理沿用 `#12` 的验证记录。
- `#17` 已经由 PR `#18` 合入 `main`。真实 Pi `1.5.2` 经 Daemon 连接本机模型夹具，两轮流式回复沿用同一原生 UUID，第二轮带回首轮上下文。HTTP/WS 夹具覆盖无效标识、失败、取消、FIFO 与 SQLite 恢复。Pi 启动参数及数据目录见[驱动说明](../../crates/soxu-drivers/README.md#daemon-本机-pi-只读对话)。
- `#19` 已由 PR `#20` 合入 `main`，隔离工作区已归档。真实 Claude Code `2.1.331` 经 Daemon 连接本机模型夹具，两轮回复沿用同一 UUID，模型请求未携带工具，会话 JSONL 落在 Daemon 的 `claude` 目录。
- `#21` 已由 PR `#22` 合入 `main`，隔离工作区已归档。真实 Qwen Code `0.28.0` 经 Daemon 完成两轮回环续接，模型请求未携带工具，额外返回的工具调用触发失败且未执行。配置、运行数据与历史落在 Daemon 的 `qwen` 目录。
- `#23` 已由 PR `#24` 合入 `main`，隔离工作区已归档。真实 Copilot `1.0.114` 经 Daemon 完成两轮回环续接，两个请求均无工具，第二轮带回首轮提示与回复。两轮复用同一 UUID，历史与缓存分别写入隔离状态和缓存目录。本地及云端门禁通过。
- Copilot 项目钩子夹具中，默认启动跳过 `sessionStart`，启用目录信任后触发。可信项目把 `disableAllHooks` 设为 `false` 可以覆盖用户级 `true`，项目绑定继续关闭。
- `#25` 已由 PR `#27` 合入 `main`（`043fc66`），Paseo 工作区已归档、特性分支已回收。真实 OpenCode `1.19.6` 经 Daemon 两轮同一 `ses_` ID、上下文续接、零工具及停止后模型连接断开通过；数据库与缓存隔离。本地七项及云端三项门禁通过。版本门禁暂不放开派生独立服务的新版。
- `#26` 的严格权限防护与基础探针已经由 PR `#28` 合入 `main`（`ab96da7`），原工作区已归档。该探针使用 Antigravity `1.5.17`，探针只复制 EXE 并使用临时用户目录。两轮同一 UUID、前轮上下文及取消断开模型连接通过；有效文件写入被 deny 规则拦截，未生成目标文件，但回合返回成功。
- `#26` 的工具过滤及项目覆盖探针已由 PR `#29` 合入 `main`（`ba90fc4`），原工作区已归档。空工具自定义代理将实际模型声明从 15 项降至仅 `manage_task`，未声明写入被注册层拒绝；全匹配全局钩子能够在执行前拒绝残留工具。
- 项目同名代理可重新声明写入工具，关闭斜杠展开也没有阻止真实项目 `SessionStart` 钩子执行；项目同名禁用和自动批准钩子未削弱全局工具拒绝。Daemon 路由保持关闭，后续探针补充了启动前的可信资产校验。可复跑探针与完整限制见[驱动文档](../../crates/soxu-drivers/README.md#空工具代理与钩子实测)。
- `#26` 的钩子故障与资产缺失回归已由 PR `#30` 合入（`e19a523`），工作区已归档。钩子非零退出、非法 JSON、超时和脚本缺失均阻断残留工具；但配置为空、配置缺失或自动批准篡改可使任务列表工具真实执行。
- `#26` 的可信资产与实际回合启动门禁已由 PR `#31` 合入（`160e009`），工作区已归档、特性分支已回收，本地七项与云端三项门禁全部通过。Daemon 库新增 `NativeAssetGuard`：以调用方可信内容验证大小、SHA-256、普通文件和祖先目录，拒绝链接/reparse 替换和根内 `.agents`，启动目录不得越界。宿主可绑定已配置驱动，首轮、续接和自动 FIFO 在实际启动前重验；拒绝不写新回合消息、不消费队列，恢复后可继续。Windows 路径表示差异已处理。尚未挂入 Antigravity 路由，校验与启动竞态、其他配置来源和运行期保护仍待验证；该项延期，已合入的防护作为恢复验收的基础。
- `#26` / PR `#55` 接入 Daemon 托管的 `agy` / `antigravity`。入口仅接受未归档的本机只读 `QuickChat`，以私有 home、配置、临时目录和工作目录启动，项目目录入口继续关闭。真实 Windows `1.5.18` 的 HTTP/WS 回环验证同一 UUID 两轮续聊、`manage_task` 被钩子拒绝及等待模型时停止。首轮、续聊和 FIFO 出队前复核可信资产，拒绝缺失、篡改、链接及项目覆盖。Windows 运行期夹具验证资产可读且拒绝外部写入和删除，取消及丢弃均覆盖钩子进程树回收。超时钩子夹具在 1.5 秒自行退出。
- `#26` 当前依赖进程配置、可信资产校验和工具钩子实现隔离，尚无操作系统沙箱。项目自定义代理可重新声明工具，项目 `SessionStart` 钩子可执行，因此项目入口仍关闭。同权限进程、管理员及原生 CLI 重开路径仍受系统权限与上游实现约束。Windows 进程树快照存在父进程提前退出或并发派生的竞态，Unix 主动脱离进程组的后代超出回收范围。历史扫描和 Fork 尚不可用。
- `#32`（PR `#37`）使用真实 Pi `1.5.2` 和 localhost 模型夹具验证原生 RPC。续接保持同一原生 UUID，活跃回合中的 `Steer` 只进入模型上下文一次，普通 `Queue` 保持 FIFO，`Stop` 结束活跃回合。模型请求携带的工具数为零。PiDeck 参考提交为 [`b9a095e063845c80574c5517994c6fcf2be60f25`](https://github.com/ayuayue/PiDeck/commit/b9a095e063845c80574c5517994c6fcf2be60f25)，仅参考其原生 RPC。
- `#33`（PR `#36`）的 M3a Git 专项 20 项全通过，覆盖真实目录后缀、linked worktree、detached/unborn HEAD 和特殊路径。交付范围限于只读仓库身份与结构化状态。
- 本机使用 `soxu doctor --json` 查看脱敏发现结果，通过 `soxu chat new --read-only` 创建快聊，再用 `soxu chat send <对话或会话 ID> <文本>` 发送，`soxu events` 查看增量，`soxu chat stop <对话或会话 ID>` 停止。Windows 的 Scoop 包装器需用 `SOXU_CODEX_EXE` 指定真实 EXE 的绝对路径。失败或停止后暂停剩余队列，下一次投递从队首继续。队列驻留内存。

- `#69`（PR `#73`）在 `#56` 宿主接入之上打通已复核 Worktree 的原生只读会话：首轮、续接与 FIFO 出队前统一经 `resolve_ready_worktree_dir` 复核目录身份与 `.soxu-worktree-mcp.json` 清单（符号链接/重解析点拒绝），复核失败以 `WorkspaceDamaged` 上报并保留队首待撤回。WSL 真机专项 8 项全通过；同轮修正在测试桩驱动 `turn.completed` 缺少 `usage` 被 `soxu-drivers` 判为 `MalformedOutput` 导致的回合失败，以及用例在 `CreateSession` 之前订阅事件、被会话创建 `Idle` 事件提前满足首轮等待的竞态。GitHub CI 的 Rust 门禁仅跑 `windows-latest`，Linux 专属用例由本轮 WSL 真机补测。

- **并发规则**：各阶段公共契约先串行冻结，正交模块按独立 Owned Paths 采用 `2~3` 路 Paseo Worktree 并行；AOCI 托管资产由各分支独立维护，合流后重新对齐。
- **双桌面端主控协同（AGY ⇄ Codex 随时切换）**：本机 AGY 桌面端与 Codex 桌面端已同时导入 `Soxu` 项目，宿主配置与工作树写操作相互隔离，仓库认知（`AGENTS.md` / `aoci.txt` / `.agents/skills/paseo-orchestrator`）与 Paseo 守护进程状态池全面共享；任意时刻均可使用 `pwsh -NoProfile -File scripts/paseo-ctl.ps1` 在两端之间无损切换控制 Paseo。
- **主力 Agent 与模型路由池**（通过 Paseo 调度，经 Magpie 网关路由；**后续派发仅使用 `pi` agent**，并须通过 `scripts/paseo-run-pi.ps1` 绑定项目 `mise.toml` 锁定环境）：
  - **L3 架构/并发攻坚**：`pi` | `magpie/group/gpt-6.2-astra` / `magpie/group/claude-opus-5.5` / `magpie/group/glm-5.4` (`high`)
  - **L2 核心模块/UI 实现**：`pi` | `magpie/group/gpt-6.2-sol` / `magpie/group/claude-sonnet-5.5` / `magpie/auto/gemini-4-hydrogen` (`high` / `medium`)
  - **L1 脚手架/文档巡检**：`pi` | `magpie/auto/deepseek-v4.1-flash-1017` / `magpie/auto/gemini-4-hydrogen` (`low`)
  - **统一子代理模型（自 `wave-m3-next2-20261126` 起生效）**：所有 Soxu 子代理统一使用 DeepSeek（`magpie/auto/deepseek-v4.1-flash-1017`，用户简称 `ds4.1f`）。

---

## 4. 下一步紧接行动队列（Next Actions）

1. `#90` / PR `#91`（`354fbd7`）与 `#94` / PR `#95`（`32e0109`）已合入 `main`（run `45932680368` 通过），`#88`（CI-C）重跑 run `45934505956` 仍失败于 `soxu-magpie` 测试导入，由 `#96` 承接；`#96` 由主控独立复验并锁 SHA 合入后再重跑 `#88`，随后按 **CI-C → CI-D** 顺序推进；CI-E/F 与 CI-D 写入面正交，可并行排期但不能共用同一工作树。
2. CI-C 要把 CI-B 的结构化计划（`requiredPlatforms` / `requiredJobs` / `testProfiles` 与 `baseSha` / `headSha` / `checkoutSha`）接到真实三平台动态矩阵执行与证据聚合；动态矩阵可以按需扩展 job 契约，但必须保留 `CI Gate` 检查名、必跑只认 `success` 的严格证据与失败闭合，不得为接入矩阵而削弱对账。
3. Git 后续候选继续按契约与共享写入范围排序：`#57-2` 只读恢复预览（恢复执行须另经二次授权）、`#58-D` 先建 Safety Snapshot 再执行 Rewind、`#58-E` 文件级结构化提交的预检与执行；`direct` MCP 运行时挂载与 Worktree 内写权限另排期，涉及 `soxu-protocol` 的变更先走独立契约 PR。
4. 未经核验的原生测试、正式发布与 Intel 平台发版入口仍列为待办；Intel 只能由架构敏感、手动完整或发版入口显式请求，不从文件变更推导。
5. 在非 Windows 宿主补测 `#49`、`#51`、`#56`、`#69` 的真实注册、固定提交检出、宿主接入、路由重试与 Worktree 原生会话。`#69` 的 Linux 专属 `mod linux` 用例未在真实 Linux 宿主编译或运行（本机 WSL 启动返回 ERROR_BAD_EXE_FORMAT 且缺少 x86_64-linux-gnu-gcc），需留待 Linux 宿主补测。Windows 原生目录身份与 DACL 持续授权另行决策。
6. `#26` 隔离只读快聊已合入。Daemon 托管快聊的项目目录入口继续关闭，后续另行验收项目配置来源、启动竞态和运行期隔离。Copilot 项目钩子安全、遥测、历史导入、附件、移交、更高权限及移动端投递另排验收。M2 和 M3 均仍有未交付范围。

