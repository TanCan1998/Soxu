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
- **CI 抗压波次：`#83`（CI-A）、`#86`（CI-B）、`#90`（原生矩阵热修复）、`#94`（Darwin Daemon 测试导入热修复）、`#96`（Magpie 测试导入热修复）、`#98`（原生资产集测临时根）、`#100`（非法字节路径夹具）、`#103`（仓库创建夹具后台维护）、`#105`（夹具 Git 构造点同源守卫）已合入 `main`，`#88`（CI-C）已合入 `50fb201f`。** `#83` / PR `#85`（`a86024e`）交付 `ci-gate-plan/v1` 固定 job 契约、`Plan` / `Verify` 同源推导对账与发布资产白名单，已通过远端 CI 合入 `main` 并归档工作区。审核候选提交 `2216417` 的 run `45906220407` 是审核快照，合入后主线 run `45906614204` 核验的正是 CI-B 的派发基线 `a86024e`，两者是分开记录的两条证据。`#86`（CI-B）已锁候选 `62c346f` 由 PR `#87` 合入 `main`（`8c01a533`），工作区 `wks_8bf43dd1b64bca90` 已归档；PR run `45909853439` 在 Ubuntu 26.04 实测 56 项检测器回归、202 项门禁断言与 POSIX 字面反斜杠用例，Windows Rust 门禁 156 通过、5 忽略。
- **原生矩阵冻结源码热修复（`#90` / PR `#91`）已 squash 合入 `main`（`354fbd7b`）。** 修复 run `45914578287`（attempt 1，合并候选 `260333d6d397d61f57d8a11e0084489dcbf3ae94`）的两处真实失败：Darwin 上 `crates/soxu-git/src/repository_create/platform.rs` 的 `open_handle` 未使用参数与 `needless_return`（保留 `UnsupportedPlatform` 零写入语义，未加 `allow`、未把 Linux 原语放宽到 Darwin）；Linux 上 `crates/soxu-git/tests/diff.rs` 的整树快照与夹具自身派生的后台 `git maintenance` 竞态（夹具提交改为与生产只读入口同源的 `-c maintenance.auto=false -c gc.auto=0` 守卫，新增 trace2 `child_start`/`child_exit` 确定性回归）。主线 run `45926704994` attempt 1 completed/success，CI Gate success。
- **`#90` 第二轮（有界修订）：加固回归用例里可独立证实的无界等待风险。** PR `#91` 原 head `b0fc54b6` 的 GitHub run `45917865092` / Windows `rust-check`（job `140241392408`）在 `Run scoped unit & integration tests` 触到 30 分钟 job 级上限后 cancelled，job 与 run 日志均报 `log not found`（`BlobNotFound`），远端无法定位到具体二进制。同范围本地运行（`-p soxu-daemon -p soxu-git`）**未复现**该超时：原候选 `b0fc54b6` 与最终源码 `93b5125` 分别耗时 `111s` 与 `94s`，因此原 30 分钟超时**仍属未定因**。可独立证实的是对照用例自身的无界等待风险：旧实现的阳性对照刻意重新打开后台维护，并用 `Command::output()` 无界收集 stdout/stderr，只要继承写端的子/孙进程存活就会永久阻塞——该风险在旧候选上已独立确认。修订后对照仍真实走同一条 `commit_command` 提交路径，参数为 `maintenance.auto=true` + `gc.auto=0` + 显式 `*autoDetach=false`，改为 `stdin`/`stdout`/`stderr` 全部 `Stdio::null()` 的有界直接进程等待（`spawn` + `try_wait` + `60s` 截止 + `kill` + `10s` 回收窗口），五态（成功 / 非零退出 / 超时 / 观察异常 / 回收未确认）可区分，`try_wait` 自身失败也要走 kill + 有界回收，超时只承诺回收直接子进程；失败证据仅输出布尔摘要，不转储原始 trace2。三项源证据（子进程启动、无 detach、同 `sid` + 同 `child_id` 的 `child_exit`）全部保留：`child_id` 只是发出进程内的序号，必须与该进程 `sid` 一起绑定，并已用“同号不同 `sid`”的负例回归钉住。原远端超时仍按未定因记录，本修订不声称已定位。
- **Magpie 测试导入热修复（`#96` / PR `#97`）已合入 `main`（`e6ea5ba8`）。** 仅按同平台条件门控 `crates/soxu-magpie/tests/worktree_context.rs` 两项导入（源码 SHA-256 `90e51cba…`），未加 `allow`、未删用例、未改断言。锁候选 `04c6283` 经 PR `#97` 合入，审核 run `45937478475` 通过、Runner 实际检出 `39aac98a`，受审/CI 检出/合入三棵树均为 `c71a74f9`；Issue 已关闭、远端修复分支已删，主线 run `45937841659` success。归档后 `wks_892f5dddf5b05a63` 不在活跃列表、代理 closed、Git Worktree 注册与目录均已移除；squash 后本地分支非祖先，`git branch -d` 拒绝并保留原候选。主控独立复核：Windows 范围门禁 `62.63s`、Rust `41/0/3`、专项 5；WSL `20.94s`、专项 6；Darwin 以同范围非 Daemon 交叉 Clippy 复现并验证（修复前 `exit 101` 15.58s → 修复后 `exit 0` 8.80s），全仓 Darwin 原生仍待新矩阵。
- **`#88` / PR `#89` 已合入 `main`（squash `50fb201f`；PR run `46001187539` success）。** 合流前由 `#107` / PR `#108`（`4173853`）热修复 Darwin 夹具误期望。工作区 `wks_7296ea2a4568da18` / 分支 `feat/88-ci-c-platform-matrix`。已依次合入 `#90`（`354fbd7b`）、`#94`（`32e0109`）、`#96`（`e6ea5ba8`）、`#98`（`7cccd0d6`）、`#100`（`83388ec3`）、`#103`（`0950c342`）与 `#105`（`061c4f25`）各轮新 `main`，候选冻结后的三平台矩阵与独立 Intel 入口按既定裁决执行。首轮 run `45914578287`（attempt 1）失败于 `macos-15` ARM64 的 Clippy 与 `ubuntu-24.04` 的 `soxu-git` Diff 集测，两处根因已由 `#90` 热修复分派并合入。`#88` 随后按主控裁决修复矩阵接线：Gate 改为从原始 jobs 派生逐实例证据、内部脚本调用改哈希表 splatting、PR head 与 `checkoutSha` 分离、artifact 名与下载 pattern 带 attempt；并多轮补审采集器计数语义。【嵌套计数】`native-90-control-linux-final.log`（WSL 真机 `cargo test -p soxu-git --locked` 成功日志，SHA-256 `23cc23f2…`）暴露采集器缺陷：日志含 12 个套件边界（11 个测试二进制 + `Doc-tests soxu_git`），顶层汇总合计 138 项，但 `worktree_lifecycle` 夹具派生的 `coordinator_process_worker` 子进程额外输出 9 条 `1 passed; 16 filtered out`，旧采集器会把子进程汇总一并累加而高估顶层数量。已按裁决 A 改为：仅在 Cargo 套件边界内可靠归属时计数；嵌套、交错、边界外、不完整或无足够边界的多行一律 `countsKnown=false` + `counts=null` + 稳定脱敏原因（`nested_test_result_lines` / `unattributed_test_result_lines` / `result_outside_suite_boundary` / `incomplete_suite_summary` / `result_count_mismatch`），并新增 `countsReason` / `testResultLines` / `suiteCount` / `nestedResultLines` 审计字段。汇总行必须行首独立（普通文字中嵌入的 `test result:` 不算）；单一开始与单条汇总还须满足 `running N` == `passed + failed + ignored`（`filtered`/`measured` 不参与）。未知计数不掩盖 `failed` / `build_failed` 分型。 【治理夹具环境】run `45929401718` attempt 1 暴露真实缺陷：Ubuntu governance job 未安装 pnpm，而真实 run 块往返夹具在隔离子进程里执行 `$(pnpm --version)` 与 `$(node --version)`，导致 `Finalize sanitized mobile web evidence` 失败；该块另有 4 条产出断言因失败被跳过，因此远端为 2/426 而非本地 430，二者不矛盾。修复：夹具在子进程内提供 rustc/cargo/node/pnpm 的受控版本桩，并把子进程 PATH 限制为「桩目录 + pwsh 目录 + 平台必需系统目录」；POSIX 桩的 PATH 项经 `cygpath -u` 做 MSYS 规范化（Windows 的 `C:/…` 会被 bash 的 PATH 冒号拆成两项而定位不到桩），无 cygpath 时回退 `/c/…` 变换；桩每次被调用都写日志并可断言，证据里的 toolchain 必须等于桩声明值；另以「无桩目录时不得出现桩值」的负向断言证明桩值只来自桩目录。桩仅用于回归夹具——生产 `collect-ci-evidence.ps1 -Mode Finalize` 仍调用真实工具链，工具缺失时保持失败闭合。修复理由记录于本看板，源码内不保留新增解释性注释。【本轮原生结果】run `45934505956`（attempt 1，merge checkout `fc5d9034`，树 `50a9fbe`）：Detect、Ubuntu 治理、格式、PWA、Windows（`cargo test` `147.960s`，`158/0/5`）与 Linux（`38.130s`，exit 0，`40` 汇总 / `31` 套件 / `9` 子进程汇总，计数保持未知）**实际通过**；Darwin job `140294571285` 仅在 `soxu-magpie` 集成测试两项平台导入处报错（Clippy `101`），后续测试**按失败跳过**，CI Gate 拒绝。PWA 实际 `pnpm 12.17.0` 对齐留给 `#92`。【本轮矩阵失败面】run `45976349619`（attempt 1）：Windows `158/0/5`、Linux 与 `macos-15` ARM64 Clippy 均通过；但 `probe` 两项完整快照各**仅差 `.git/objects/maintenance.lock`**，测试 `exit 101`，CI Gate 正确失败。这是 **Darwin 平台、该 run** 的独立观察，与 `#103` / PR `#104` 处理过的**历史 Linux 快照**是两次不同观察，不得合并叙述；历史锁差不等于已追踪派生链，**禁止过滤锁文件、缩断言、跳平台、`sleep` 或重试掩盖**。
- **夹具 Git 构造点同源守卫（`#105` / PR `#106`）已合入 `main`（`061c4f25`）。** 锁候选 `96b127ae`，合入树 `d68758d6` 与审查树一致；Issue `#105` CLOSED，PR run `45983235804` success，主线 run `45983857661` success（主控独立核实）。生产实现、工作流、依赖与协议**未改**；测试四处（`probe` / `fixed_diff` / `project` / `worktrees`）把唯一的夹具 Git 构造点集中并加与生产只读入口同源的 `-c maintenance.auto=false -c gc.auto=0` 守卫，`probe` 另加有界 trace2 受测/阳性同构与 `sid` + `child_id` 退出配对回归。回退实验：重建守卫后 `exit 101`、恢复后重建通过；**Windows 回退未再现原 Darwin 快照差异，不据此推断竞态不存在**。主控独立 Windows `117.88s`、WSL 全仓 `45.46s`、Darwin 排 Daemon 交叉 `6.69s` 均 `exit 0`，184 项执行摘要稳定，最终仅文档/AOCI 修订后摘要全同。
- **CI 波次后续顺序**：`#90`、`#94` 与 `#96` 已合入并构成本分支基线；CI-C 承接三平台动态矩阵的实际 job 执行与证据聚合；后续正式任务 `#92`（CI-D）与 `#93`（CI-E/F）均已合入。未经核验的原生测试与正式发布仍列为待办。
- **`#93` / PR `#112` 已合入 `main`（squash `bb10aca`）。** 交付四层 GPUI 测试路线与 Desktop 发布蓝图（`docs/dev/ci-and-testing-architecture.md`），并同步本分支 AOCI 认知。
- **`#92`（CI-D）已合入 `main`（squash `bdb500d6`，PR `#113`，主线 run `46031412971` 8/8 success）。** 联动发布仓 PR `#1` 已 squash 合入 `a610fcdf`。工作区 `F:\AI\paseo\worktrees\19r49pwc\feat-92-ci-d-toolchain-cache-concurrency`、分支 `feat/92-ci-d-toolchain-cache-concurrency`、PR [\#113](https://github.com/TanCan1998/Soxu/pull/113)，已普通 merge `origin/main`（`bb10aca`）。联动仓分支 `ci/92-toolchain-source`（HEAD `e664221`）与 PR [\#1](https://github.com/TanCan1998/soxu-releases/pull/1)。候选内容：`mise.toml` 作为工具链唯一真相源（`scripts/read-mise-versions.ps1` + 81 项断言）、缓存键七维绑定、并发按事件分组、`-Mode Summary` 脱敏摘要、双仓版本来源统一与 darwin-amd64 原生 Intel Runner。**实测证据（全部见 ADR-011）**：同一 SHA `cd32db4` 冷/热对照 run `46024301919`（attempt 1/2，均 success），三平台实测 `rustc/cargo 1.99.1`、PWA 实测 `node v26.13.1` / `pnpm 12.21.0`，缓存包 linux `209,487,214 B` / macos-arm64 `178,139,667 B` / windows `187,068,052 B`；PR 取消：`46026390870` 被 `46026504778` 取消（已完作业保留 success，矩阵实例 cancelled，`CI Gate` 正确 failure）；dispatch 独立分组：`46026504778` 与 `46026938724` 同时 in_progress；非取消排队：`46026961283` 未取消在途 run，排在约 `302s` 后 FIFO 执行。首选版 `queue: max` + 动态 `cancel-in-progress` 被 GitHub 运行时判非法（run `46023242089` 零作业失败），已按主控裁决改用 `queue: single` 并写入静态契约。
- **M4 Wave 4.1 设计令牌编译器（`#115`，分支 `feat/115-gpui-theme-tokens`）已合入 `main`（squash `01fcdaf`）。** `scripts/sync-design-tokens.mjs` 从唯一真相源 `tokens.json` 同批编译三份产物（`design-system/soxu/tokens.css`、`apps/mobile-web/src/tokens.generated.css`、`crates/soxu-desktop/src/theme/tokens.rs`）；`--check` 对三者逐字节比对，漂移时点名文件并非零退出。`scripts/verify-design-system.mjs` 把 `tokens.rs` 列为必需文件并校验三产物与 `--check` 接线（只做存在性与接线，不用正则重算语义）。新增 `crates/soxu-desktop/src/lib.rs` 与 `theme/{mod.rs,tokens.rs}`，通过库目标公开常量消除 `dead_code`（未用 `allow` 压制）；本单**未引入** gpui/gpui-component 或任何第三方依赖，未改根 `Cargo.toml`、未实现桌面视图、未动移动端。
- **`wave-20261128` 双路已合入 `main` 并收口。** `#118` / PR `#120`（`0d1165e`，M4-W1b gpui 依赖接入与令牌消费层，主线 run `46058243076` 三平台全绿）与 `#119` / PR `#121`（`1cc9c3f`，M3 `#57-2` Worktree 只读恢复预览契约，PR run `46065749309` 全绿）两路 Paseo Worktree 已归档、本地与远端分支已回收。两路子代理统一使用 `pi` + `magpie/group/jev-adaptive`（`high`），独立 `CARGO_TARGET_DIR`（`soxu-118-gpui-w1b` / `soxu-119-recovery-preview`）零互踩。`#118` 的依赖冲突按 ADR-012 裁决：crates.io 原名 `gpui` 最新稳定版仅 0.2.2，而 `gpui-component 0.11.0` 硬绑定 Zed gpui 0.3.x 线改名包 `gpui-pre`，最终锁定 `gpui-pre =0.3.14 + gpui-component 0.11.0` 单实现组合（`cargo tree` 证据）；`#119` 交付 `GitProbe::recovery_preview` 五类恢复操作（unlock / repair / prune / 移除注册 / 清理部分落盘）的只读预览契约与观察状态分类，Darwin `unused import` 热修一次（L-013 Seen 4×）。两处冻结文件最小可见性扩围（`read_intents` 与 `read_checkout_journal` 转 `pub(crate)`）经主控按 L-020 裁决放行并在 PR 单列说明。
- **M3 两份设计已合入 `main`。** `#57` 的显式 Init/Clone 与损坏恢复二次授权见 [仓库初始化、克隆与工作树恢复](../dev/git-init-clone-and-recovery.md)。`#58` 的轮次 Checkpoint 快照、先建 Safety Snapshot 再 Rewind 和文件级结构化提交见 [检查点、回退与结构化提交](../dev/git-checkpoint-rewind-commit.md)。
- **M6 前置契约（`#136` / `wave-m6-contract-20261128`）已交付契约 PR（分支 `feat/136-m6-contract`，基线 `13e784d`）。** `crates/soxu-protocol` 新增三组域契约：受管脚本（`scripts.rs`：`ManagedScriptEntry` 清单、`ScriptProgram`/`ScriptArgument` 受控程序与参数、`ScriptArgumentPolicy`、`ScriptWorkdirPolicy`、`ScriptEnvPolicy`、`ScriptCommand`/`ScriptCommandResult` 与 `ScriptLifecycleEvent`）、Schedules & Heartbeats（`schedules.rs`：cron/interval/heartbeat 节奏、有效期窗口、重试上限、幂等键、停用/恢复/删除/列出/立即触发，`ScheduleCommand` 与触发/结果事件）、受控 PTY（`pty.rs`：尺寸/工作目录/环境白名单策略，输出有界 16 KiB 分片 + 从 0 起严格递增序号 + `end_of_stream`，`PtyEvent`）；新增 `DaemonFeature::{ManagedScripts,Schedules,Heartbeats,ControlledPty}` 与缺省全关、未知令牌原样保留的 `DaemonFeatureSet`。**兼容边界**：`PROTOCOL_VERSION` 保持 `1`，只新增模块/类型/枚举变体；`soxu-daemon::dispatch_command` 对 `RpcCommand` 穷尽匹配且本单冻结该 crate，故全局 `RpcCommand` 与握手结构体不新增字段或变体，三组契约先以 `ScriptCommand`/`ScheduleCommand`/`PtyCommand` 域命令枚举成型，由后续三路实现 PR 1:1 转发到 `RpcCommand` 并接上能力协商入口。**验证**：`cargo test -p soxu-protocol --locked` 全绿（13 内联单测 + 7 Git 线缆 + 9 M6 契约）、`cargo clippy -p soxu-protocol --all-targets --locked -- -D warnings` 零告警、`cargo check --workspace --all-targets --locked` 通过（5m00s，含 gpui 依赖树）、增量 `scripts/check-all.ps1` 全绿。
- **`wave-m4-w2b-20261128` 轨道 B 已合入 `main`。** `#124` / PR `#125`（`365201b`）交付 GPUI 应用壳层底座；`#130` / PR `#131`（`1684e66`）交付 M5-B 760px 同步宽输入舱与权限/移交/统计圆环。
- **M5 Wave 进行中：`#129`（M5-A 时间线核心视图）已在独立 Worktree 落地（分支 `feat/129-m5-timeline`，基线 `80a9185`）。** 新增 `crates/soxu-desktop/src/timeline/**`（10 个子模块）：`<=8.3Hz` 合批节拍（120ms 量化、`StreamCadence`）、虚拟窗口/行高数学（`RowMetrics` 前缀和 + 上下占位 + 滚动锚定）、5 类工具卡（含 `400` 行 Diff 截断与「构建一次」快照）、审批卡（高危红线禁用「本会话允许」+ 默认焦点在拒绝 + 跨设备幂等消解）、提问卡（1-9 数字快选/多选/`44px` 热区）、检查点锚点与回退三选一、`LiveSteps` 默认折叠单行、`20px` 轮次导航轨（含刻度优先级与命中边界）、卡片内焦点不逃逸（复用冻结 `focus.rs` 的 `FocusLayer`/`FocusStack`）。`shell.rs` 仅新增 1 个字段、1 行构造与 1 行挂载（与 M5-B 共享，合流冲突由主控解决），`lib.rs` 仅新增模块声明。`cargo clippy -p soxu-desktop --all-targets --locked -- -D warnings` 与 `cargo test -p soxu-desktop --locked`（**112 项**）通过，`scripts/check-all.ps1` 增量门禁全绿。合流 M5-B（`#131`，`1684e663`）后中栏为「时间线在上、输入舱在下」，`lib.rs` 同时声明 `timeline` 与 `composer`，合流后测试增至 **177 项** 全绿。
- **`wave-m5c-20261128` 执行中。** `#133`（M5-C）在 `feat/133-m5-rightpanel` 落地右栏 Diff/Files、7 分区设置弹窗与提交/配对/回退/透视四模态（纯内存视图模型，渲染零 I/O）。已普通 merge `origin/main`（`dd26757`，M5-A 合入后的当前基线），与 `#129` 共用 `shell.rs` / `lib.rs` / 本看板的合流冲突已由本分支解析。
- 仓库已启用合并自动删支（`delete_branch_on_merge = true`）与 Paseo Worktree 回收。
- **参考项目与索引进度（已全部完成）**：
  - [x] 拉取参考项目至 `.refs/`：`paseo`（7,818 文件）、`codeg`（3,324 文件）、`padu`（1,157 文件）、`t3code`（上游 `pingdotgg/t3code`，快照 `f3e36337`，经 `gh-proxy.org` 镜像 FluxDown 下载）
  - [x] **t3code 认知边界（2026-11-28 用户决策）**：仓库过大，**不做全量 AOCI**；仅建 CodeGraph 本地图（541,919 节点 / 1,596,159 边，`--local-only`）+ 源码级参考，任务书按「无 AOCI、非 Git 根」声明，索引缺失时回退 `rg` + 源码阅读
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
| **M4** | Live Mobile PWA & GPUI Workbench Foundation | ⏳ 待启动 | **双轨 100% 正交并行**：`apps/mobile-web` 去 Mock 接通真实 Daemon WS/`rust-embed`/扫码配对与 13 组视口验收；`soxu-desktop` GPUI 令牌编译器、双开源字体联动、可扩展 `i18n`、7 层焦点栈与四栏自适应壳层。**设计基准（PR `#101` 已合入）**：多形态全自适应布局（`useAdaptiveViewport`/`VisualViewport`、`320px` 零溢出）、三段悬浮毛玻璃胶囊顶栏、`SinkLayout` 下沉侧栏与 44px 热区纪律 |
| **M5** | GPUI Core Views: Transcript, Composer & Inspector | ⏳ 待启动 | `soxu-desktop` 三路并行落地：`<=8.3Hz` 虚拟对话流 + 5 类工具卡/审批卡/提问卡 + 20px 轮次导航轨、760px 同步宽输入舱 + `@agent` 移交 + 三色圆环统计、右栏 Diff/Files、7 分区设置与核心业务模态框。**设计基准（PR `#101` 已合入）**：收件箱一触分诊台、`ToolCallCard` 意图卡、`Artifact` 审批卡、`LiveSteps` 默认折叠单行焦点、底部 `border-t` 遥测微脚条与零页签居中微点阵引导页 |
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
   - **Wave 4.1（单流准备 PR，`#118` 进行中）**：在根 `Cargo.toml` 引入 `gpui` / `gpui-component` 并配置开发编译优化，扩展 `scripts/sync-design-tokens.mjs` 生成零漂移 Rust 主题常量并接入 `--check`。版本裁决：crates.io `gpui` 最新稳定版为 0.2.2，而 `gpui-component` 0.11.0 硬绑定 Zed gpui 0.3.x 线（以 `gpui-pre` 之名发行），经主控裁决采用 `gpui = { package = "gpui-pre", version = "=0.3.14" } + gpui-component 0.11.0`，避免同树双实现；`theme/gpui_colors.rs` 把 `tokens.rs` 颜色常量映射为 GPUI `rgb`/`rgba`/`Hsla` 并配 6 项单元测试（含负向漂移验证）。
   - **Wave 4.2（双轨正交并行）**：轨道 A（`apps/mobile-web` + `soxu-daemon` Web 伺服）去 Mock 接通真实 Daemon WS/`rust-embed`/扫码配对并完成 13 组视口验收；轨道 B（`soxu-desktop`，`#124` 进行中）搭建 GPUI 应用壳层底座：`main.rs` 经 `gpui-pre-platform =0.3.14`（同 ADR-012 快照线）挂载真实 GPUI App 与主窗口（标题/尺寸/最小尺寸按 MASTER，GUI 冒烟实测窗口标题 `Soxu`）、全局主题状态 `ThemeState`（明暗切换/跟随系统 + `observe_window_appearance`）、双开源字体预设与 `effective_code_family()`、key→文案 i18n 骨架（zh-CN 默认/热注册回落）、7 层焦点栈纯逻辑与四栏自适应断点策略（400/640/1024）及 `WorkbenchShell` 占位骨架，40 项单元测试全绿。
5. **M5 — GPUI 桌面核心视图 3 路并行**：
   - 按 `crates/soxu-desktop/src/` 独立子目录 3 路并行：A 路（虚拟对话流 `<=8.3Hz` 合批 + 5 类工具卡/审批卡/提问卡 + 20px 轮次导航轨）、B 路（760px 同步宽输入舱 + 纯图标权限与投递 + 可配模型移交 + 三色圆环与 11 项统计）、C 路（右栏 Diff/Files + 7 分区设置弹窗 + 提交/配对/回退/透视等核心模态框）。
6. **M6 — 受管脚本、受控 PTY、全局检索与自动化收口 3 路并行**：
   - 单流补充 PTY/Scripts/Schedules 小契约后 3 路并行：A 路（`paseo.json` 受管脚本 + `#050505` 多窗格受控 PTY 终端）、B 路（`Cmd+K` 五分区检索 + CodeGraph 符号补全 + 按需三步 Onboarding）、C 路（Schedules & Heartbeats 定时调度中心 + 50,000 行 Diff 压测与跨平台打包）。

---

## 3. 活跃工作区与智能调度策略

<a name="31-当前-pi--git-并行波次2026-11-25-北京时间盘点"></a>

### 3.1 Pi / Git 波次收口与工作区盘点

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

#### M5 Wave（进行中）

| Issue / 任务 | 分支 / Paseo 工作区 | Owned Paths |
| :--- | :--- | :--- |
| `#129` M5-A 虚拟时间线、工具/审批卡与轮次导航轨 | `feat/129-m5-timeline` / `F:\AI\paseo\worktrees\19r49pwc\feat-129-m5-timeline` | `crates/soxu-desktop/src/timeline/**`（新增）、`shell.rs`（仅 import + 单行挂载）、`lib.rs`（仅 module 声明）、`docs/ops/PROGRESS.md`、本分支 AOCI |

与 `#130`（M5-B 输入舱）并行：两路共用 `shell.rs` 挂载点，冲突由主控在合流时解决；各自使用检出内 `target/` 与 `CARGO_BUILD_JOBS=4`，不共用构建目录。

**参考与索引边界**：本轮复用 Padu（`F:/workplace/Paseo/work/Soxu/.refs/padu`，`fbe97a3`）既有 AOCI（`aoci.code.txt` 直读 `transcript_view.rs` / `cadence-invariants.md` / `scrollbar.rs` 条目）与 CodeGraph（`codegraph explore --path <Padu 根>`），实际采用「虚拟列表 + 导航轨 + 120ms 提交节拍 + 尾行重测」结构；**未**移植 `bezel-gpui` 依赖、未用 `#[allow(dead_code)]`，导航轨宽度/刻度几何全部改由本仓令牌推导。参考索引未复制、未 init/scan/重建/维护；目标工作树无 `.codegraph`，按任务书回退到完整源码阅读。

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

#### 已收口波次：CI 抗压（`#83` / PR `#85`、`#86` / PR `#87`、`#90` / PR `#91`、`#94` / PR `#95` 已合入，`#88` 已合入 `50fb201f`）

| Issue / 任务 | 分支 / Paseo 工作区 | 状态 | Owned Paths |
| :--- | :--- | :--- | :--- |
| `#83` / PR `#85` CI-A 变更范围门禁与发布契约 | `feat/83-ci-a-strict-gate` / `wks_87a6a683f132b176` | 已合入 `a86024e`，工作区已归档 | `.github/workflows/ci.yml`、`scripts/verify-ci-gate.ps1`、`scripts/test-ci-gate.ps1`、AOCI |
| `#86` / PR `#87` CI-B 范围检测、Cargo Metadata 与测试计划 | `feat/86-ci-b-change-scope` / `wks_8bf43dd1b64bca90` | 已锁候选 `62c346f` 合入 `8c01a533`，工作区已归档 | `scripts/detect-check-scope.ps1`、`scripts/test-check-scope.ps1`、`scripts/check-all.ps1`、`scripts/verify-ci-gate.ps1`、`scripts/test-ci-gate.ps1`、`ci.yml` 检测器接线、本看板、AOCI |
| `#88` / PR `#89` CI-C 三平台动态矩阵与逐平台验收证据 | `feat/88-ci-c-platform-matrix` / `wks_7296ea2a4568da18` | 多轮补审完成，已依次合入 `#90`/`#94`/`#96`/`#98`/`#100`/`#103`/`#105` 各轮新 `main`（至 `061c4f25`）；本地固定候选待主控独立验收，三平台与独立 Intel run 待推送后补齐（run `45914578287`、治理 run `45929401718`、run `45934505956`、run `45976349619` 失败证据均保留） | `.github/workflows/ci.yml` 矩阵与证据接线、`scripts/detect-check-scope.ps1` 平台/Intel 判据、`scripts/verify-ci-gate.ps1` 计划契约与证据对账、`scripts/collect-ci-evidence.ps1`、`scripts/test-check-scope.ps1`、`scripts/test-ci-gate.ps1`、`scripts/check-all.ps1` 接线、本看板、AOCI |

#### 已合入热修复：原生平台矩阵冻结源码（`#90`）

| Issue / 任务 | 分支 / Paseo 工作区 | 状态 | Owned Paths |
| :--- | :--- | :--- | :--- |
| `#90` / PR `#91` 原生矩阵冻结源码热修复 | `fix/native-matrix-blockers` / `wks_4e0ff3d28c954665` | 已 squash 合入 `main`（`354fbd7b`），主线 run `45926704994` attempt 1 completed/success；工作区已 archive，本地分支因 squash 后非祖先而保留（未强删），残余目录保留 | `soxu-git/src/repository_create/platform.rs`（最小条件编译/告警修正）、`soxu-git/tests/diff.rs`（夹具隔离、有界维护回归）、本看板、`LESSONS.md`、本分支 AOCI |

#### 已合入热修复：Darwin Daemon 测试导入（`#94`）

| Issue / 任务 | 分支 / Paseo 工作区 | 状态 | Owned Paths |
| :--- | :--- | :--- | :--- |
| `#94` / PR `#95` Darwin Daemon 测试导入热修复 | `fix/94-daemon-darwin-test-imports` / `wks_fa74a88079bc342e` | 已锁 `fb0da0fb` 由 PR `#95` 合入 `main`（`32e01097`），合入树 `4a21ba34` 与受审树一致；PR run `45932340468` success，main run `45932680368` 为放行依据；Darwin 原生回退证据仍待 `#88` 合流后的真实 `macos-15` Runner 补齐 | `crates/soxu-daemon/tests/worktree_lifecycle.rs`（最小平台导入）、本看板、`LESSONS.md` 的 `L-013`、本分支 AOCI |

`#89` head `3db8969` 的 run `45929401718`（attempt 1，Darwin job `140278345298`）在 `Run Clippy` 报 `crates/soxu-daemon/tests/worktree_lifecycle.rs` 三处 `unused import`：`state::WorktreeAdmissionRequest`、`soxu_git::GitProbe`、`WORKTREE_CONTEXT_FILE`，测试步骤 skipped（证据记录为 `build_failed`，并非测试失败或原生通过）。三个符号的**全部**使用点只落在 `#[cfg(windows)]` 的系统拒绝用例与 `#[cfg(target_os = "linux")]` 的 `linux` 模块内，macOS 编译面一处也没有。修复保持最小：把三者从顶层无条件 `use` 移出，改为与用例同口径的 `#[cfg(any(windows, target_os = "linux"))]` 单条导入，测试体、断言、`Unsupported`/`UnsupportedFilesystem` 与权限边界一字未改，未加 `#[allow]`、未删用例、未屏蔽任何测试目标。主控独立复核：Windows 范围门禁 `62.63s`、Rust `41/0/3`、专项 5；WSL `20.94s`、专项 6；Darwin 以同范围非 Daemon 交叉 Clippy 复现并验证（修复前 `exit 101` 15.58s → 修复后 `exit 0` 8.80s），全仓 Darwin 原生待新矩阵。主控独立复核：Windows 范围门禁 `75.57s`、`worktree_lifecycle` `27/0/3`（工作树专项 `3` 通过）；WSL 全 `workspace --all-targets` Clippy + 专项 `8` 通过 `67.78s`；测试源码 SHA-256 `47e4235afb4c34269794b734c5e33951feb2416b4996017ea61c61c32dbcf9c1`；覆盖口径为**两个无条件 + 六个 Linux 用例**（不是三 + 五）。

#### 已合入热修复：Magpie 测试导入（`#96`）

| Issue / 任务 | 分支 / Paseo 工作区 | 状态 | Owned Paths |
| :--- | :--- | :--- | :--- |
| `#96` / PR `#97` Magpie 测试导入热修复 | `wks_892f5dddf5b05a63`（已归档） | 已锁 `04c6283` 由 PR `#97` 合入 `main`（`e6ea5ba8`），受审/CI 检出/合入三棵树均为 `c71a74f9`；审核 run `45937478475` 通过，main run `45937841659` success；归档后不在活跃列表、代理 closed、Git Worktree 注册与目录已移除，本地 squash 分支保留 | `crates/soxu-magpie/tests/worktree_context.rs`（最小平台导入）、本看板、`LESSONS.md`、本分支 AOCI |

`#89` head `df2849aa` 的 run `45934505956`（attempt 1，检出 `fc5d9034`，Darwin job `140294571285`）在 `macos-15` ARM64 的 `Run Clippy` 报 `crates/soxu-magpie/tests/worktree_context.rs` 两项 `unused import`：`WORKTREE_CONTEXT_FILE` 与 `WorktreeContextServer`，测试步骤 skipped（证据为构建失败，既不计为测试失败，也不构成 Darwin 原生通过）。两者使用点全部落在 `#[cfg(any(windows, target_os = "linux"))]` 的三个用例内（`WORKTREE_CONTEXT_FILE` 三处、`WorktreeContextServer` 三处），macOS 编译面零使用点。修复保持最小：从顶层无条件 `use` 移出，改为同口径 `#[cfg(any(windows, target_os = "linux"))]` 单条导入，测试体与断言一字未改，可在 Darwin 编译的 `native_and_shared_routes_do_not_create_assets` 与 `rejects_a_link_in_the_authorized_directory_chain` 保持无条件，未加 `#[allow]`、未删用例、未屏蔽任何测试目标。

#### 已合入热修复：Darwin 原生资产集测临时根（`#98`）

| Issue / 任务 | 分支 / Paseo 工作区 | 状态 | Owned Paths |
| :--- | :--- | :--- | :--- |
| `#98` 原生资产集测临时根修正 | `fix/98-native-asset-fixture-root` / `wks_c7a695bacda8d8e3` | 已合入 `main`（`7cccd0d6`），main run `45948364426` success；不改生产 Guard、CI 与公共契约 | 测试自建夹具根：`soxu-daemon/tests/{native_assets,rpc_ws_e2e}.rs` 与新增 `soxu-daemon/tests/common/mod.rs`、`soxu-daemon/src/state/worktrees.rs` 的 `#[cfg(test)]` 用例、`soxu-drivers/tests/native.rs::Scratch`、`soxu-git/tests/repository_create.rs::Fixture`、`soxu-git/src/repository_create/{checkout,publish}.rs` 的 `#[cfg(test)]` 子模块、`soxu-magpie/tests/worktree_context.rs::Fixture`、本看板、`LESSONS.md` 的 `L-018`、本分支 AOCI |

`#89` run `45940068623`（attempt 1，`macos-15` ARM64，job `140312147432`）在 `soxu-daemon --test native_assets` 出现 1 通过 / 5 失败，测试退出码 `101`，失败点全在 `NativeAssetGuard::new` → `checked_directory` 报「目录或祖先不是普通目录」。根因为**测试自建夹具根取自未规范化的系统临时目录**：守卫按设计拒绝任何含符号链接的祖先，于是正向构造先被拒绝，负向断言根本没有到达被测分支（同套件唯一“通过”的 `rejects_relative_escape_duplicate_empty_and_oversized_declarations` 属此类未到分支的伪通过）。修复只把测试自己创建的夹具根、以及 `src/*/#[cfg(test)]` 内的临时根改为取自规范化真实路径，再由同一真实根派生资产、启动目录与每次重验目标；有意构造的资产链接、祖先链接、junction、目录替换、路径逃逸一律原样交给被测函数。

同一根因在 `TMPDIR` 指向受控 symlink 时于 Linux 全量复现，落到 7 个测试目标：`soxu-daemon --lib`（1 失败）、`soxu-daemon --test native_assets`（5 失败）、`soxu-daemon --test rpc_ws_e2e`（2 失败）、`soxu-drivers --test native`（1 失败）、`soxu-git --lib`（5 失败）、`soxu-git --test repository_create`（27 失败）、`soxu-magpie --test worktree_context`（5 失败）。`soxu-daemon --test rpc_ws_e2e` 的两项只是 Codex 守卫用例（`spawn_guarded_daemon` 与两个 `native_asset_guard_rechecks_*`）：AGY 用例在别名条件下本来就通过，因为私有 guard 分支经 `antigravity::quick_chat_directory(guard, chat_id)` 直接取 guard 根下的 `quick-chats`，不经过 `runtime.quick_chat_root`，故 `spawn_antigravity_daemon` 保持原样零改动。

#### 已收口热修复：Darwin 非法字节路径夹具（`#100`，同因三处合并处理）

| Issue / 任务 | 分支 / Paseo 工作区 | 状态 | Owned Paths |
| :--- | :--- | :--- | :--- |
| `#100` Darwin 非法字节路径夹具门控 | `fix/100-darwin-raw-path-test` / `wks_6756d9ae8127d9ec` | 已按锁 SHA `8565fb8` 合入 `main` `83388ec3a85efc6cd06b18831276ceaad923aaef`（PR `#102`，run `45961069623` `success`），Issue `#100` 已 CLOSED；主控最终独立复验 Windows `106.14s`、Darwin 交叉排除 `soxu-daemon` `0.73s`、WSL 全仓 `22.38s` 均 `exit 0` 且复验前后哈希稳定 | `crates/soxu-git/tests/{diff,probe,worktrees}.rs`（仅最小平台门控与用例改名）、本看板、`LESSONS.md` 的 `L-019`/`L-020`、本分支 AOCI |

`#89` head `b8ca5fcd` 的 run `45951887087`（`macos-15` ARM64）在 `crates/soxu-git/tests/diff.rs::unix_preserves_raw_linebreak_name_and_refuses_a_replaced_symlink` 的 `fs::write` 报 `EILSEQ`（Illegal byte sequence）：夹具文件名 `line\nbyte\xff` 含非法 UTF-8 字节。已证明的只是**当前 `macos-15` Runner 所用文件系统对该夹具返回 `EILSEQ`**，不推断 Darwin 或其他文件系统的整体能力；Linux 是**本项目明确验证支持**原始字节路径语义的运行面。原 `#[cfg(unix)]` 把 Darwin 误纳入编译面。

修复保持最小：三处用例统一改为 `#[cfg(target_os = "linux")]`，沿用仓库具体目标门控惯例（`checkpoint.rs::linux_rejects_links_fifos_nested_repositories_and_preserves_raw_path_bytes`、`repository_create.rs` 的 `#[cfg(target_os = "linux")]`），并同步改名以免误称 Unix 通用：`diff.rs::linux_preserves_raw_linebreak_name_and_refuses_a_replaced_symlink`、`probe.rs::linux_preserves_non_utf8_and_newline_repository_paths`、`worktrees.rs::native_linux_newline_and_non_utf8_worktree_paths_remain_lossless`。测试体、断言、`Fixture` 夹具与生产实现一字未改，未加 `#[allow]`、未删用例、未屏蔽文件或测试目标；Linux 仍真实执行三处原始字节断言，Windows 仍不编译这三处。

**同因定向扫描（仅报告未越权修改）**：全仓 Rust 测试内 `from_bytes`/`from_vec` 与非法字节名命中分两类。① 建盘夹具：三处已纳入本 PR；另 `tests/checkpoint.rs::linux_rejects_links_fifos_nested_repositories_and_preserves_raw_path_bytes`（`fs::write` 写 `raw-\xff`）本就在 `#[cfg(target_os = "linux")]` 内，无需改动。② 纯内存字节解析、不创建非法字节文件名：`tests/status.rs` 的状态输出解析、`tests/worktrees.rs` 的 worktree 清单解析、`tests/diff.rs` 的 `old\xff` 文件**内容**与 `#[cfg(windows)]` 用例的 `b"invalid\xff"` 内存路径入参，均保持跨平台编译与执行。`crates/**` 内再无其他建盘非法字节夹具。

**CI 影响与后续**：`cargo test --workspace --locked` 不带 `--no-fail-fast`，目标顺序为 `checkpoint` → `diff` → `fixed_diff` → `probe` → … → `worktrees`（本机 Linux 全量日志实测），故 `diff` 先行失败会掩盖后两处。三处建盘夹具已一并改为 Linux 白名单；其中 `probe.rs` / `worktrees.rs` 在 Darwin 上的 `EILSEQ` 属**同因推断**（同一非法字节文件名建盘前提），本轮未在 macOS 原生复现，真实结论待 `#88` 原生重跑。`#100` 已按锁 SHA 合入 `main` `83388ec`（run `45961069623` `success`）。同次 Linux 快照的 `maintenance.lock` 线索由主控拆为独立 `#103`，并已用 trace2 正负对照取得实际派生证据（见下节），已随 PR `#104` 合入 `main` `0950c34`（run `45970384111` `success`）；`#100` / PR `#102` 与 `#103` / PR `#104` 均已合入，由 `#88` 在合流后重跑三平台及 Intel 入口，仍不把该快照当成可被忽略的抖动。

#### 已合入热修复：仓库创建夹具后台维护（`#103`）

| Issue / 任务 | 分支 / Paseo 工作区 | 状态 | Owned Paths |
| :--- | :--- | :--- | :--- |
| `#103` / PR `#104` 仓库创建夹具后台维护 | `fix/repository-fixture-maintenance` / `wks_2dd2dfe8b5781ee4` | 已锁 `eb39be3` 由 PR `#104` 合入 `main` `0950c342`，合入树与已审树同为 `0d444d92`；PR run `45967378514` 通过，main run `45970384111` success | `crates/soxu-git/tests/repository_create.rs`（仅夹具命令构造与直接回归）、本看板、`LESSONS.md` 的 `L-014`、本分支 AOCI（禁 `.aoci/config.json`） |

`#89` head `b8ca5fcd` 的 run `45951887087`（Linux job `140349512352`）在 `crates/soxu-git/tests/repository_create.rs::clone_records_authorized_source_without_temporary_path` 失败，主控解析日志得 left/after 与 right/before 唯一差异为 `.git/objects/maintenance.lock`（before 有、after 无）。该差异只是已观察证据；本轮改用 trace2 正负对照取实际派生链，不凭锁文件名宣称完整定因，也不把该文件当成可过滤的快照噪音。

**缺口与修法**：该文件的 `git()` / `git_stdout()` 原先只经一个命令构造点，但没有生产只读入口（`probe.rs`、`repository_create/sandbox.rs`、`checkpoint/git.rs`）已有的维护守卫。现把构造点收敛为 `git_command(directory, extra_args)`，在用户配置之后追加 `-c maintenance.auto=false -c gc.auto=0`；`git()` 与全部辅助调用都经它，所有调用点一并受保护，`extra_args` 追加在守卫之后只供回归显式反向启用。生产源码、CI、公共依赖、协议与其它测试文件零改动；现有测试体、源快照断言与安全断言一字未改，不新增过滤/忽略或 `#[allow]`。

**回归**：`fixture_commits_never_start_background_maintenance` 用 `GIT_TRACE2_EVENT` 直接断言行为——受测提交必须写出 trace2 且 `child_start` 中无维护子进程；正向对照只把 `maintenance.auto` 翻成 `true`（`gc.auto` 仍为 `0`）并显式 `maintenance.autoDetach=false` / `gc.autoDetach=false`，必须同时观察到子进程启动、无 `--detach`、以及同 `sid` + 同 `child_id` 的 `child_exit`。观察路径 `stdin`/`stdout`/`stderr` 全 `Stdio::null()`，等待为 `spawn` + `try_wait` + `60s` 截止 + `kill` + `10s` 回收窗口，成功/非零退出/超时/观察异常/回收未确认五态可区分，失败信息只输出脱敏布尔摘要，不转储原始 trace2，超时只承诺回收直接子进程。trace 文件写在夹具自建的真实根内，不复用系统临时目录的裸文件；另有同号不同 `sid` 的合成负例钉住绑定判据。

#### 已收口热修复：只读探测夹具后台维护（`#105` / PR `#106`，已合入 `061c4f25`）

| Issue / 任务 | 分支 / Paseo 工作区 | 状态 | Owned Paths |
| :--- | :--- | :--- | :--- |
| `#105` 只读探测夹具后台维护 | `fix/105-probe-fixture-maintenance` / Paseo 工作树 `19r49pwc` | 本地冻结候选待主控独立验收（基线 `0950c34`，即 `0950c342c6c773efaf5a5937945b895e89a3794d`，对应 run `45970384111` `success`）；不 push、不建 PR、不合入，`#88` 保持冻结 | `crates/soxu-git/tests/{probe,fixed_diff,project,worktrees}.rs`（仅夹具 Git 构造点守卫 + `probe.rs` 有界 trace2 回归）、本看板、`LESSONS.md` 的 `L-014`、本分支 AOCI（禁 `.aoci/config.json`） |

`PR #89` run `45976349619` attempt 1（head `32969a0`、实际检出 `b724235`、`macos-15` ARM64 job `140429021160`、实际 Git `2.56.1`）`cargo test --workspace --locked` `exit 101`，`soxu-git --test probe` 为 `6/2/0`，失败为 `accepts_sha256_repositories`（`probe.rs:444`）与 `detects_unborn_attached_and_detached_without_writing`（`probe.rs:163`）。两份快照分别 28 比 27 个文件，`left_only` 均为 `.git/objects/maintenance.lock`，`right_only` 与共同文件内容差异为空。`probe.rs` 的 `file_bytes` 只比较**文件集合与字节**、不含目录 mtime，所以该快照只能证明文件集合差异；派生链改用本轮 trace2 对照取证，**不把该历史 run 说成已追踪到后台进程**。

**缺口与修法**：`probe.rs::Fixture::run` 只有配置隔离，缺生产只读入口的 `-c maintenance.auto=false -c gc.auto=0`。现收敛为唯一构造点 `Fixture::command(directory, guard_overrides)`：守卫追加在既有 `-c` 之后，`guard_overrides` 再追加于守卫之后，`run` 与提交流程 `commit_command` 都经它，`commit` 与回归共用同一提交命令形状。同因扩围经主控裁决限于「自身先提交、又对整根（含 `.git`）做零写入断言、构造点确实缺守卫」的四处（含本文件）：`fixed_diff.rs::command`、`project.rs::Fixture::run`、`worktrees.rs::Fixture::run` 各只加两行守卫，不改夹具体、断言与用例名。盘查未改：`checkpoint.rs` 仅定点观测 `.git/HEAD`、`index`、`logs/*`、`note.txt`（非全树），`worktree_checkout.rs`、`worktree_lifecycle.rs`、`status.rs` 无整树零写入断言，`diff.rs`（`#103`）与 `repository_create.rs`（`#104`）已有同源守卫；`worktrees.rs::branch_validation_matches_native_git_check_ref_format` 的直连 `check-ref-format` 只校验 ref 字符串、不打开仓库不写对象，登记不改。生产实现、CI、依赖、`AGENTS.md`、`.aoci/config.json` 与其它测试文件零改动。

**回归**：`probe.rs::fixture_commits_never_start_background_maintenance` 用 `GIT_TRACE2_EVENT` 直接断言受测提交写出 trace2 且不派生维护子进程；正向对照走同一条提交路径，只把 `maintenance.auto` 翻成 `true`（`gc.auto` 仍为 `0`）并加 `maintenance.autoDetach=false` / `gc.autoDetach=false`，必须同时看到子进程启动、无 `--detach`、同 `sid` + 同 `child_id` 的 `child_exit`。观察进程三流全 `Stdio::null()`，等待为 `spawn` + `try_wait` + `60s` 截止 + `kill` + `10s` 回收窗口，五态可区分，失败只输出脱敏布尔摘要；trace 写在夹具自建真实根内（名字独占唯一，不预删），另有同号不同 `sid` 的合成负例钉住绑定判据。本轮实测坑：Windows 上 `Fixture::new` 的 `canonicalize` 根带 `\\?\` 前缀，Git 的 trace2 目标会静默不写该路径（表现为 `written=false`），若读成“未派生”就是空断言；去掉前缀时保留绝对路径（UNC 降为 `\\server\share`）后取证成功，并写入 `LESSONS.md` 的 `L-014`。

**参考复用与边界**：Padu `F:/workplace/Paseo/work/Soxu/.refs/padu`（固定 `fbe97a38e0a1fea8a9a386bda5f969f1f0bc6d02`）；AOCI 走 `aoci.exe --repo <Padu 根> mcp` 的 UTF-8 stdio（首包 `initialize` id 1、`notifications/initialized`、按实际 `tools/list` 调 `aoci_get_entries`），取到 `command_env.rs` 条目（`A:command, child_search_path, ensure_job_object_for_process_tree, capture_shell_environment`）；CodeGraph 用 `codegraph explore --path <Padu 根> "command_env output spawn"` 定位 `command`/`plain_command`/`spawn`/`output` 及依赖面。采用：子进程构造点集中（`plain_command` → `command` 分层、调用方覆盖追加在后）与 I/O 归属集中；不移植产品特定的 Windows Job Object、`CREATE_NO_WINDOW` 与登录/交互式 shell 环境探测。参考索引未复制、未 init/scan/重建/维护。

#### 已收口波次 `wave-m4-w1a-20261128`（M4 Wave 4.1 令牌编译器）

| Issue / 任务 | 分支 / Paseo 工作区 | Owned Paths |
| :--- | :--- | :--- |
| `#115` 设计令牌编译为 Rust 主题常量并纳入零漂移校验 | `feat/115-gpui-theme-tokens` / `wks_19r49pwc` | `scripts/sync-design-tokens.mjs`、`scripts/verify-design-system.mjs`、`crates/soxu-desktop/src/{lib.rs,theme/**}`（必要时 `main.rs`、crate `Cargo.toml`，不得新增依赖）、本看板、AOCI |

参考 Padu（`fbe97a38`）的 `scripts/generate-themes.ts` 三产物 `targets` + `--check` 逐文件比对组织方式；因其 `theme_palette.rs` 依赖 `bezel-gpui` 0.3.21 且用 `#[allow(dead_code)]`，本单不照搬 API/版本，改为零依赖常量 + 库目标公开导出。验收边界：`tokens.rs` 只是常量底座，GPUI 主题结构与 7 层焦点栈待 Wave 4.2 接入。

#### 进行中波次 `wave-m4-w2b-20261128`（M4 Wave 4.2 轨道 B：GPUI 应用壳层底座，`#124`）

| Issue / 任务 | 分支 / Paseo 工作区 | Owned Paths |
| :--- | :--- | :--- |
| `#124` GPUI 应用壳层与主题/字体/i18n/焦点栈底座 | `feat/124-desktop-shell` / Paseo 工作树 `19r49pwc` | `crates/soxu-desktop/**`（含其 `Cargo.toml`）、`docs/ops/PROGRESS.md`、本分支 AOCI；同因扩围（主控已裁决）：根 `Cargo.toml` 的 `[workspace.dependencies]` 追加 `gpui-pre-platform = "=0.3.14"` 单行 + `Cargo.lock` 同步 |

实现切片：`main.rs` 经 `gpui_platform::application()` 挂载真实 GPUI App（窗口 `Soxu` / `1120×720` / 最小 `360×480`，几何常量由编译期 `const` 断言锚定令牌断点）；`theme/theme_state.rs` 全局主题状态（Dark/Light/System + `observe_window_appearance` 跟随系统）；`font.rs` 双 OFL 字体预设 + `effective_code_family()` 联动契约；`i18n.rs` key→文案骨架（zh-CN 默认、热注册、zh-CN 不可覆盖、缺失回落）；`focus.rs` 7 层焦点栈纯逻辑（单一激活层/模态锁定/Esc 消费）；`layout.rs` 400/640/1024 断点与四栏策略；`shell.rs` `WorkbenchShell` 四栏占位骨架（零 I/O 渲染）。40 项单测全绿；依赖边界：新增 `gpui-pre-platform =0.3.14` 为 gpui-pre 快照线官方入口 crate（`Cargo.lock` 净新增其传递依赖闭包，`cargo verify-project` 与 `--locked` 全链验证通过）。

#### 进行中波次 `wave-m5b-20261128`（M5-B：760px 同步宽输入舱、权限/移交与统计圆环，`#130`）

| Issue / 任务 | 分支 / Paseo 工作树 | Owned Paths |
| :--- | :--- | :--- |
| `#130` M5-B 同步宽输入舱、纯图标权限与 `@agent` 移交、三色圆环与 11 项遥测 | `feat/130-m5-composer` / `19r49pwc` | `crates/soxu-desktop/src/composer/**`（新增）、`shell.rs`（仅 import + 单行组件挂载）、`lib.rs`（仅 module 声明与导出）、`docs/ops/PROGRESS.md`、本分支 AOCI（禁改 `.aoci/config.json`） |

设计权威为 `design-system/soxu/pages/02-composer-and-handoff.md` 与 `pages/07-settings-fonts-and-motion.md`；本单一字未改设计文件，未重新生成 `preview.html`（L-024）。

实现切片（`crates/soxu-desktop/src/composer/`，全部为纯内存视图模型，渲染零 I/O、零子进程）：

- `metrics.rs`：760px 同步宽几何。`synchronized_content_width` 是对话流与输入舱的**唯一取值入口**（`canvas_column_width − 2×16px` 后钳制到 `COMPOSER_MAX_WIDTH_PX`），只消费 `LayoutPlan` 的 `Docked/Drawer/Hidden` 形态与真实栏宽，不重算断点；编译期断言 `composerMaxWidth == transcriptMaxWidth == 760`。
- `permission.rs`：6 档纯图标权限状态机（`plan / read-only / ask / auto-edit / full-access / turbo`），按 Agent 能力子集循环（`Alt+P`）、不支持档位回退到 `ask` 并报告 `Fallback`，非 Emoji 几何占位图标 + 中文 `aria`/`hint` 文案。
- `handoff.rs`：`@agent` 移交胶囊与三枚范围微开关（3 轮摘要 / 未提交 Diff 路径计数 / 新建 Worktree），选型复用 `HandoffSelection` 与 `HandoffScopeOptions` 默认契约，产出 `ComposerChip{kind: Handoff}`。
- `keys.rs`：Enter / Shift+Enter / `Ctrl+Enter` / `Ctrl+Shift+Enter` / `Alt+Enter` 与 IME 组合门控。裁决顺序固定为「IME → 非 Enter → Shift+Enter → Preparing 拦截 → 修饰键 → 裸 Enter」，IME 组合期任何 Enter 都不提交也不换行。
- `stats.rs`：三色圆环（绿 `<65%` / 琥珀 `65–88%` / 绯红 `>=88%`，12 点钟顺时针弧线几何可单测）+ 11 项遥测（8 行主表 + 3 行「更多统计」）+ 零值空态；所有遥测文本禁止 `·` 分隔符。
- `chips.rs`：上下文芯片集合（同类型同引用去重、按 id 移除）、`+` 菜单 8 项（5 上下文引用 + 3 快捷指令）、`/` 命令纯内存过滤（前缀优先、最多 8 行、无 I/O）。
- `changes.rs`：变更摘要浮药丸 `[N 个文件已更改 +A -D]` 与进程内动作槽 `ActionSink`；点击映射到 `FocusLayer::ReviewDiff`。
- `mod.rs`：`ComposerState` 组合模型 + `render`/`render_for_window` 落图（`surface.composer` `#212121`、`22px` 圆角、`1px` 常态描边、`44px` 最小输入高度、页脚 `1px` hairline + 圆环/费用/速率读数）。

验证证据：`scripts/mise-doctor.ps1` 实测 rust `1.99.1` / node `26.13.1` / pnpm `12.21.0`、`CARGO_TARGET_DIR` 为工作树内 `target/`；`cargo check/clippy -p soxu-desktop --all-targets --locked`（`-D warnings`）通过；`cargo test -p soxu-desktop --all-targets --locked` `104 passed / 0 failed`（其中新增输入舱单测 33 项）；`cargo fmt --all -- --check` 通过；`pwsh -NoProfile -File scripts/check-all.ps1`（增量）7 步全部通过或按计划跳过。AOCI 完整认知链在本轮被宿主截断（chunk 1/6），按 `L-004` 停链后仅做 source-bound 工程与收尾维护。参考边界：本工作树未检出 `.refs/`，参考仅读取主检出可读根 `F:/workplace/Paseo/work/Soxu/.refs/padu`（`fbe97a38`）的输入舱/统计结构，未复制、未 `init`/`scan`/重建参考索引，也未照搬其依赖或 lint 压制。

#### 进行中波次 `wave-m5c-20261128`（M5-C：右栏 Diff/Files、7 分区设置与核心模态，`#133`）

| Issue / 任务 | 分支 / Paseo 工作树 | Owned Paths |
| :--- | :--- | :--- |
| `#133` M5-C 右栏 Diff/Files、7 分区设置与提交/配对/回退/透视模态 | `feat/133-m5-rightpanel` / `19r49pwc` | `crates/soxu-desktop/src/right_panel/**`、`settings/**`、`modals/**`（新增）、`shell.rs`（仅 import + 单行组件挂载）、`lib.rs`（仅 module 声明与导出）、本看板、本分支 AOCI（禁改 `.aoci/config.json`） |

设计权威为 `design-system/soxu/pages/01-desktop-workbench.md` §4/§6、`pages/04-magpie-and-cli-inspector.md` §5.2、`pages/06-modals-onboarding-and-automation.md` §3/§5、`pages/07-settings-fonts-and-motion.md` §二/§三/§七；本单一字未改设计文件，未重新生成 `preview.html`（L-024）。

实现切片（全部纯内存视图模型，渲染零 I/O、零子进程）：

- `right_panel/diff.rs`：归一化 unified diff 解析。文件头剔除、`@@ -a,b +c,d @@` 重置双侧游标、`+`/`-`/上下文分别只占新侧/旧侧/双侧行号；**行号槽右对齐**（`format_gutter`：`{:>4}` 定宽数字，空号位同宽空格）；**400 行截断**（`MAX_DIFF_LINES = 400`，超限置 `truncated` 并给「差异过长，仅显示前 400 行」提示）；结尾空上下文行回收。
- `right_panel/files.rs`：文件清单快照（归一化→去重→升序）与 `/` 大小写不敏感过滤；`path:line[:col]` 位置后缀剥离、反斜杠统一、`..` 越界一律拒绝（纯词法，不探测文件系统）。
- `right_panel/mod.rs`：四页签（变更对比/文件树/脚本/终端，页签头 `44px` 取 `TOUCH_HIT_MIN_PX`）、Diff 工具栏（基准 `未提交改动`/`对比 <baseRef>`/`对比检查点 <id>`、单栏/双栏且 `panel_width < 480px` 强制单栏、`/` 过滤、提交入口）、行号槽 + 增删行底色 + 截断提示、加载/错误/无变更/过滤无结果四种空态（`160px` 定高卡，错误态绯红描边）、Files 空快照态与过滤态。提交入口只产出 `ModalKind::Commit` 请求（`request_commit` → `commit_pending`），**不执行任何写入**。
- `settings/mod.rs`：7 分区（名称/序号/Lucide 图标名/细化配置项与 07 页表一致）、`820×580` + `200px` 导航轨几何、`< 640px` 降级抽屉且行高 `≥ 44px`、焦点捕获（打开压 `ModalDialog`、低层入栈 `RejectedModalLock`）与 Esc 关闭还原（回到打开前所在层，无先前层则回落 `WorkbenchGlobal`）、分区循环导航与 `/` 搜索、字号钳制（UI `12~17`、代码 `11~17`）、`2↔4` 制表符、`codeFollowsUi` 联动禁用下拉并切到界面字体族、偏好越界自检。
- `modals/mod.rs`：四模态几何（提交 `540px`+`max-h 80vh`、配对 `580px`、回退 `480px`，统一 `p(20)`/`rounded(18)`）、`ModalHost` 焦点锁定/还原与**互斥**打开。
- `modals/commit.rs`：`✨ feat` 等 8 类类型映射与仓库规范一致；范围预设 8 项 + 自定义校验（空或 `[a-z0-9_/-]` 且首尾为字母数字）；简述 `4~72` 字符且单行；首行合成 `<emoji> <type>(<scope>): <简述>`；门禁栏三态（首行格式/无个人路径/密钥扫描）任一失败或零勾选即禁用提交。
- `modals/pairing.rs`：`#offer=` 令牌与 6 位码互斥解析（允许 `123 456`/`123-456` 分隔）、`http://<ip>:7698/#offer=<token>` 词法解析、倒计时环（`300s` TTL、进度落 `0..=1`）、局域网不可达时提示切中继、设备在线计数与单台吊销；产出 `PairingRequest`（映射协议 `PairingCredentialInput`），不触网。
- `modals/rewind.rs`：三选一（仅分叉对话/对话与工作树/取消）单值互斥；初始**未决**、焦点在安全项；破坏性项 `apply` 先返 `NeedsConfirmation`，切换选项重置确认；冲突文件阻止工作树回退并给 `git status` 指引；Safety Stash 无删除入口，失败/取消后仍保留。
- `modals/inspector.rs`：启动环境透视。含 `KEY/TOKEN/SECRET/COOKIE/PASSWORD` 的键值强制掩码 `••••••••`、主目录折叠 `~`（偏好可关）、`CARGO_TARGET_DIR`/`SOXU_MCP_DIRECT` 徽标、降级态网关文案「未启用，使用 CLI 原生凭据」、`NoopExtensionHost · 预留扩展点未激活`。
- `shell.rs` / `lib.rs`：右栏挂载替换为 `right_panel::render_default(&palette)`（单行 + import），`lib.rs` 仅增 `modals`/`right_panel`/`settings` 声明与文档条目。

验证证据：`cargo check/clippy -p soxu-desktop --all-targets --locked`（`-D warnings`）通过；`cargo test -p soxu-desktop --locked` `169 passed / 0 failed`（本单新增 65 项：Diff 解析/行号/截断 9、文件清单与路径 5、右栏页签/空态/过滤/提交入口 9、设置 7 分区与焦点 11、模态几何与宿主 4、提交 6、配对 7、回退 7、透视 7）；`cargo fmt --all -- --check` 通过；`pwsh -NoProfile -File scripts/check-all.ps1`（增量）通过。工具链：rust `1.99.1`（`rustfmt 1.10.0`、`clippy 0.1.99`）/ node `26.13.1` / pnpm `12.21.0`，`CARGO_TARGET_DIR` 为工作树内 `target/`，`CARGO_BUILD_JOBS=4`。

参考复用与边界：Padu（`F:/workplace/Paseo/work/Soxu/.refs/padu`，`fbe97a3`）的 `apps/desktop/src/app/right_panel/{diff,files,mod,model,tabs}.rs`（工具栏/文件树/空错态三段式、`ReviewDiff`/`ReviewDiffTree` 双上下文、`old_line`/`new_line` 双侧行号模型）与 `apps/mobile/src/lib/diff-presentation.ts`（`MAX_DIFF_LINES = 400` 与文件头剔除、尾随空行回收）、`apps/desktop/src/app/right_panel/links.rs`（转录文件链接的纯词法路由）；Paseo（`F:/workplace/Paseo/work/Soxu/.refs/paseo`，`7bcd598`）的 AOCI 差分协议条目（`packages/protocol/src/binary-frames/diff.ts` 的差异块/行/样式分段，用于确认行号与分段语义边界）。CodeGraph 用 `codegraph explore --path <Padu 根> "right panel diff review layout gutter line numbers truncation"` 定位上述符号及其调用面；Paseo 的 `.codegraph` 有漂移提示，仅读 AOCI，未据图下结论。采用：三段式内容分发、双侧行号 + 空号位、400 行截断与尾行回收、词法路径归一化。不采用：Padu 的 `bezel-gpui`/`Pierre` 渲染依赖、`#[allow]` 压制、`/ 过滤` 与行内批注的完整交互链（M6 接 daemon 后再评估）。参考索引未复制、未 `init`/`scan`/重建/维护，目标工作树无 `.codegraph`。

### 3.2 已合入交付与验收证据边界

- `#105` 只读探测夹具后台维护的本地验收边界（基线 `0950c34`；仅改 `soxu-git` 四个测试文件的夹具构造点 + `probe.rs` 回归，生产实现 / CI / 公共契约零改动）：
  - **回退即失败（Windows，Git `2.56.0.windows.4`）**：仅删 `probe.rs::Fixture::command` 的两行守卫后重建（源 `B6138AC4…`、`probe` 二进制 `3BA45668…`），`fixture_commits_never_start_background_maintenance` 在 `0.33s` 内 `exit 101`，trace 为 `MaintenanceTrace { written: true, started: true, detached: true, exited: true }`；恢复后源回 `023F3878…`、重建二进制 `2C67A662…`，`10 passed / 0 failed`。恢复源码时曾因 `Copy-Item` 保留旧 mtime 而复用回退二进制造成一次假失败，已按 `L-015` 触达 mtime 后重建并留档。同一回退态下 `accepts_sha256_repositories` 与 `detects_unborn_attached_and_detached_without_writing` 本次在 Windows 仍 `ok`，**只说明本次 Windows 回退未再现历史快照差异**，不得据此说该竞态“在 Windows 不复现”；守卫依据是 trace2 派生事实。
  - **Windows 目标与门禁**（Rust/Cargo `1.99.1`，独立 `CARGO_TARGET_DIR=F:/workplace/Paseo/work/.cargo-target/soxu-105-probe-fixture`）：`cargo test -p soxu-git --test probe` 全绿（含新增 `fixture_commits_never_start_background_maintenance`、`maintenance_exit_must_come_from_the_same_parent_sid`）；`soxu-git` 其余 9 个目标全绿；`pwsh -NoProfile -File scripts/check-all.ps1 -BaseRef origin/main` `exit 0`（范围 `soxu-daemon`、`soxu-git`；`[1/7]` 提交历史与隐私、`[4/7]` fmt、`[5/7]` 增量 Clippy、`[6/7]` 增量测试实跑，`[2/7]`、`[3/7]`、`[7/7]` 按计划跳过）。
  - **WSL `Ubuntu 26.04`（x86_64，Git `2.53.0`，仅在头部记录版本，未在 WSL 内对宿主工作树执行 `git`）**（实体 rust `1.99.1` 与实体 node `26.13.1` 前置于 `PATH`，独立 `/root/.cache/soxu-105-probe-fixture`）：`cargo clippy --workspace --all-targets --locked -- -D warnings` 与 `cargo test --workspace --locked --no-fail-fast` 均 `exit 0`，全日志 `40` 条 `test result:` 行、`0 failed`（其中若干行来自测试内嵌套子进程汇总，不据此宣称独立测试总数）；`tests/probe.rs` 与 `tests/worktrees.rs` 均 `11 passed`（含 Linux 专属用例与新增 2 项）、`tests/fixed_diff.rs` `7 passed`、`tests/project.rs` `5 passed`。
  - **Darwin 交叉（编译面）**：`cargo clippy --target aarch64-apple-darwin --workspace --exclude soxu-daemon --all-targets --locked -- -D warnings` `exit 0`。排除 `soxu-daemon` 的唯一原因是本机缺 Darwin C 编译器/SDK（`libsqlite3-sys` 停在 `cc-rs: failed to find tool "cc"`）；该证据**不覆盖** `soxu-daemon`，也不等于 Darwin 原生运行，原生 Darwin 留待 `#88` 新矩阵。
  - **参考与索引边界**：本轮仅复用 Padu 既有 AOCI 与 CodeGraph（真实可读根 `F:/workplace/Paseo/work/Soxu/.refs/padu`，固定 `fbe97a38…`），查询入口与采用点见「已收口热修复：只读探测夹具后台维护」末段；未复制、初始化、scan、重建或维护参考索引，目标 Worktree 无 `.codegraph`，按任务书回退到完整源码阅读。
- `#103` 仓库创建夹具后台维护的本地验收边界（基线 `7cccd0d`，已随 PR `#104` 合入 `main` `0950c34`，run `45970384111` `success`；仅改 `soxu-git` 的一个测试文件，生产实现 / CI / 公共契约 / 其它测试文件零改动）：
  - **回退即失败（Linux，WSL `Ubuntu 26.04`）**：仅删除 `git_command` 的两行守卫后重建，`fixture_commits_never_start_background_maintenance` 确定性失败 `exit 101`，`MaintenanceTrace { written: true, started: true, detached: true, exited: true }`；恢复守卫后源文件 SHA-256 回到删除前同一值（`9d1ebb3d…`），重建后受测二进制 SHA-256（`7a9ba0dc…`、mtime 随重建更新）与删除前同一值，避免旧二进制充当当前证据。
  - **Linux 目标与全仓**（实体 rust `1.99.1` 与实体 node `26.13.1` 前置于 `PATH`，独立 `/root/.cache/soxu-103-repository-fixture`，未执行 `mise trust`；正式构建/测试命令不调用 `git`，前置只读 `rev-parse` 探测曾 `exit 128`，未写入，后续未重复）：`cargo test -p soxu-git --test repository_create` `30 passed / 0 failed`（含新增 2 项 `fixture_commits_never_start_background_maintenance`、`maintenance_exit_must_come_from_the_same_parent_sid`，以及原失败用例）；`cargo clippy --workspace --all-targets --locked --offline -- -D warnings` `exit 0`；`cargo test --workspace --locked --no-fail-fast --offline` `exit 0`，`40` 条 `test result:` 行全部 `0 failed`，`repository_create` 单列 `30 passed`；其中部分 `test result:` 行来自测试内嵌套子进程汇总，不求和为独立测试总数。
  - **Windows**（Rust `1.99.1`，独立 `CARGO_TARGET_DIR=F:/workplace/Paseo/work/.cargo-target/soxu-103-repository-fixture`）：`pwsh -NoProfile -File scripts/check-all.ps1 -BaseRef origin/main` `exit 0`（`100.32s`，范围 `soxu-daemon`、`soxu-git`；`[1/7]` 提交历史与隐私、`[4/7]` fmt、`[5/7]` 增量 Clippy、`[6/7]` 增量测试实跑，`[2/7]`、`[3/7]`、`[7/7]` 按计划跳过）；`repository_create` 在 Windows 编译面为 `2 passed`（新增 2 项为 `#[cfg(target_os = "linux")]`，不在 Windows 编译面）。
  - **主控独立复验（冻结 `e08c78b`）**：Windows `147.35s`、Darwin 交叉排除 `soxu-daemon` `19.81s`、Linux 全仓 `71.65s` 均 `exit 0`，复验前后全部执行字节哈希稳定且工作树干净；`30` 个 `repository_create` 目标（含原失败用例与新增 2 项回归）全部通过；主控另从执行代理原始 `tool_call` 输出取回回退/恢复捕获（`truncated=false`），与本文记录一致。
  - **Darwin 交叉（编译面）**：`cargo clippy --workspace --exclude soxu-daemon --all-targets --target aarch64-apple-darwin --locked --offline -- -D warnings` `exit 0`。排除 `soxu-daemon` 的唯一原因是本机缺 Darwin C 编译器/SDK；新增代码与用例全部落在 `#[cfg(target_os = "linux")]`，该证据**不覆盖** `soxu-daemon`，也不等于 Darwin 原生运行。
  - **参考与索引边界**：本轮仅复用 Padu 既有 AOCI 与 CodeGraph（真实可读根 `F:/workplace/Paseo/work/Soxu/.refs/padu`，HEAD `fbe97a3`），未复制、初始化、scan、重建或维护参考索引；目标 Worktree 无 `.codegraph`，按任务书回退到完整源码阅读，未借主仓索引冒充。
- `#100` Darwin 非法字节路径夹具门控（同因三处）的本地验收边界（基线 `7cccd0d`，与 `#88` 并行；仅改三处测试用例的 `cfg` 与用例名，生产实现 / Fixture / CI / 公共契约零改动）：
  - **Windows**（Rust `1.99.1`，独立 `CARGO_TARGET_DIR`）：`cargo fmt --all -- --check` `exit 0`；`cargo test -p soxu-git --test diff/probe/worktrees --locked`（逐个目标执行）分别 `15 / 8 / 10 passed`、`0 failed`、`exit 0`（三处 `#[cfg(target_os = "linux")]` 用例不在 Windows 编译面）；`pwsh -NoProfile -File scripts/check-all.ps1 -BaseRef origin/main` `exit 0`（`100s`，范围 `soxu-daemon`、`soxu-git`；`[1/7]` 提交历史与隐私、`[4/7]` fmt、`[5/7]` 增量 Clippy、`[6/7]` 增量测试实跑，`[2/7]`、`[3/7]`、`[7/7]` 按计划跳过）。
  - **WSL `Ubuntu 26.04`（x86_64）**（实体 rust `1.99.1` 与实体 node `26.13.1` 前置于 `PATH`，独立 `/root/.cache/soxu-100-darwin-raw-path-test`，未执行 `mise trust`，未在 WSL 内对宿主 `.git` 跑 `git`）：`cargo clippy --workspace --all-targets --locked --offline -- -D warnings` `exit 0`（`14.35s`，`0` 条 warning/error）；`cargo test --workspace --locked --no-fail-fast --offline` `exit 0`（仅记退出码，不称套件数）；逐目标计数：`tests/diff.rs` `14 passed`、`tests/probe.rs` `9 passed`、`tests/worktrees.rs` `11 passed`。三处原始字节用例在 Linux **实际执行并 ok**：`linux_preserves_raw_linebreak_name_and_refuses_a_replaced_symlink`、`linux_preserves_non_utf8_and_newline_repository_paths`、`native_linux_newline_and_non_utf8_worktree_paths_remain_lossless`。全仓日志共 `40` 条 `test result:` 行，其中 `9` 条来自测试内嵌套子进程汇总，实际目标套件为 `31` 个；未逐目标去重前不宣称全仓独立测试总数。
  - **Darwin 交叉（编译面）**：`cargo clippy --workspace --exclude soxu-daemon --all-targets --target aarch64-apple-darwin --locked --offline -- -D warnings` `exit 0`（`5s`）。排除 `soxu-daemon` 的唯一原因是本机缺 Darwin C 编译器/SDK（其 `libsqlite3-sys` 会停在 `cc-rs: failed to find tool "cc"`），该证据**不覆盖** `soxu-daemon`，也不构成全仓或原生通过；本机无 Darwin 链接器，无法交叉链接或运行 Darwin 测试二进制。
  - **同因推断边界**：`diff.rs` 的 `EILSEQ` 有 `macos-15` Runner 原生日志实测；`probe.rs` / `worktrees.rs` 在 Darwin 上的 `EILSEQ` 是同因**推断**（同一非法字节文件名建盘前提），本轮**未**在 macOS 原生复现，不作已复现结论。
  - **三处证据互不等价**：Windows `15 / 8 / 10 passed` 与 Linux `14 / 9 / 11 passed` 只证明各自编译面与运行面；Darwin 交叉 `exit 0` 只证明排除 `soxu-daemon` 后的 `aarch64-apple-darwin` 编译面干净。三者都不等于 macOS ARM64 原生已运行这些集成测试。
- `#98` Darwin 原生资产集测临时根修正的本地验收边界（基线 `e6ea5ba8b79ca9a7ad40223bdeb1ac043c2e4141`，与 `#88` 并行，只动测试夹具根与 `#[cfg(test)]` 内的临时根，生产 Guard / 路径检查 / CI / 公共契约零改动）：
  - **Linux 对照口径**（WSL `Ubuntu 26.04`，实体 `/root/.rustup/toolchains/1.99.1-x86_64-unknown-linux-gnu/bin` 与实体 `node 26.13.1` `bin`，未执行 `mise trust`，未在 WSL 内对宿主 `.git` 跑 `git`）：唯一变量是隔离子进程 `TMPDIR`，`/tmp/<x>/alias` 为指向同一普通目录 `/tmp/<x>/real` 的受控 symlink；命令固定为 `cargo test --workspace --locked --no-fail-fast`。基线 `e6ea5ba`：普通目录 `27.41s` `exit 0`，别名 `45.42s` `exit 101`（7 目标失败）；修复后：普通目录 `25s` `exit 0`、40 条 `test result` 汇总全部通过，别名 `24s` `exit 0`、同样 40 条 `test result` 汇总全部通过、0 条失败汇总（重复一轮为 `27s` / `25s`，结论一致）。这批 40 条汇总含夹具派生子进程的输出，**不能据此称 40 个独立套件**；本段不据这 40 条汇总反推套件数。
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

1. **CI-C（`#88` / PR `#89`）已合入 `main` `50fb201`（run `46001187539`，三平台 success）。** 合流前由 `#107` / PR `#108`（`4173853`）热修复 Darwin 夹具误期望，最终三平台矩阵与证据聚合全部通过。CI-C 已把 CI-B 的结构化计划（`platforms` / `platformMatrix` / `requiredPlatforms` / `requiredJobs` / `testProfiles` 与 `baseSha` / `headSha` / `checkoutSha`）接到真实三平台动态矩阵执行与逐平台证据聚合；`CI Gate` 检查名、必跑只认 `success` 的严格证据与失败闭合保持不变。**CI-D 以此为共同前置并在 `bdb500d6` 完成收口。**
2. 后续正式任务已建立：`#92`（CI-D）与 `#93`（CI-E/F）。两者**写入面正交、可并行排期**，只共同依赖 CI-C 合入后的主线绿灯；**CI-D 与 CI-E/F 之间没有先后依赖**，不必等其一完成再开另一个；并行时不得共用同一工作树，各自严守 Issue 声明的 Owned Paths，涉及公共契约的变更仍先走独立契约 PR。**进展：`#93` / PR `#112` 已合入 `main`（`bb10aca`）；`#92` / PR `#113` 已合入 `main`（`bdb500d6`，主线 run `46031412971` 全绿），联动发布仓 PR `#1` 已合入 `a610fcdf`。CI-C 与 CI-D 均已收口，下一步转入 M4/M5。**
3. UI 精修 PR `#101`（`089a3e8`）已合入 `main`。M4/M5 的 GitHub 里程碑描述与本看板设计基准已同步 PR `#101` 交付的多形态全自适应布局、`SinkLayout`、`ToolCallCard`、`Artifact` 审批卡、`LiveSteps` 折叠焦点与零页签引导页。
4. Git 后续候选继续按契约与共享写入范围排序：`#57-2` 只读恢复预览已由 PR `#121` 合入 `main`（`1cc9c3f`）；恢复执行及重启复核（`execute.rs` / `journal.rs`）、`#58-D` 先建 Safety Snapshot 再执行 Rewind、`#58-E` 文件级结构化提交的预检与执行按设计文档工作包顺序续排；`direct` MCP 运行时挂载与 Worktree 内写权限另排期，涉及 `soxu-protocol` 的变更先走独立契约 PR。
5. 未经核验的跨平台原生测试与正式发版仍列为待办。Intel 只保留两条入口：显式 `full-with-intel` 手动入口与正式发版（tag）入口；普通 `-Full` 不含 Intel，也不从文件路径或 `release.yml` 修改推导。`repository_create` 的 `/proc/self/fd`、Windows reparse 加固与 Darwin 写入拒绝属 OS 契约，由常规 Windows/Linux/macOS ARM64 三平台覆盖。
6. 首轮 `run 45914578287`（attempt 1）暴露的两处冻结源码缺陷均属 `#90` / PR `#91`，已修复并合入 `354fbd7b`：Linux `crates/soxu-git/tests/diff.rs:483` 的整树快照与夹具后台 `git maintenance` 竞态（夹具提交改为与生产只读入口同源的 `-c maintenance.auto=false -c gc.auto=0` 守卫，并加 trace2 `child_start`/`child_exit` 回归）；Darwin `crates/soxu-git/src/repository_create/platform.rs:336` 未使用参数与 `:353` `needless_return`。两者均未以删除断言、放宽元数据比对或 `allow` 压告警处理。另，`run 45934505956` 在 `macos-15` 报的 `soxu-magpie` 两项导入是一条**独立诊断**，由 `#96` / PR `#97` 处理并合入 `e6ea5ba8`，不属首轮同一缺陷链。首轮 `run 45914578287`（attempt 1）失败仍作为真实证据保留，不以刷新覆盖。
7. 在非 Windows 宿主补测 `#49`、`#51`、`#56`、`#69` 的真实注册、固定提交检出、宿主接入、路由重试与 Worktree 原生会话。`#69` 的 Linux 专属 `mod linux` 用例未在真实 Linux 宿主编译或运行（本机 WSL 启动返回 ERROR_BAD_EXE_FORMAT 且缺少 x86_64-linux-gnu-gcc），需留待 Linux 宿主补测。Windows 原生目录身份与 DACL 持续授权另行决策。
8. `#26` 隔离只读快聊已合入。Daemon 托管快聊的项目目录入口继续关闭，后续另行验收项目配置来源、启动竞态和运行期隔离。Copilot 项目钩子安全、遥测、历史导入、附件、移交、更高权限及移动端投递另排验收。M2 和 M3 均仍有未交付范围。
9. **M4 Wave 4.1 令牌底座（`#115`）已合入 `main`（`01fcdaf`，主线 run `46038032799` 全绿）。** 已交付 `tokens.json → {tokens.css, tokens.generated.css, theme/tokens.rs}` 三产物编译与逐字节零漂移门禁，并做真实负向漂移实验（篡改 `theme/tokens.rs` 的 `z_index::BASE` 与 `tokens.css` 的 `--soxu-space-2` 均 `exit 1` 且点名文件，恢复后 `exit 0`）。Wave 4.1 剩余部分（根 `Cargo.toml` 引入 `gpui`/`gpui-component` 与构建优化配置、GPUI 侧消费层）已由 `#118` / PR `#120`（`0d1165e`）完成，Wave 4.1 全部收口。
10. **M4 Wave 4.2 双轨已全部合入 `main` 并收口（2026-11-28）。** 轨道 A `#123` / PR `#126`（`80a9185`）：`apps/mobile-web` 去 Mock 接真实 Daemon WS（`afterSeq` 追平 + 指数退避重连 + Bearer/query 双通道）、`#offer=` 扫码与 6 位码双路配对、`soxu-daemon` 经 `rust-embed` 伺服 PWA（未知带扩展资产 404、`/api` JSON 404、dist 缺失 500 不 panic）、真实 daemon 端到端冒烟 12 项、Edge CDP 13 组视口真实渲染断言 13/13（并抓到并修复空快照白屏与 JSON 请求被 SPA 吞没两处真实缺陷）。轨道 B `#124` / PR `#125`（`365201b`）：`gpui-pre-platform =0.3.14` 挂载真实 GPUI App、主题状态（Dark/Light/System）、双字体与 `effective_code_family()`、i18n 骨架、7 层焦点栈、400/640/1024 断点四栏壳层，39 项单测 + 真实窗口冒烟通过。两路工作树已归档、本地与远端分支已回收。
12. **M5（GPUI 核心视图）三路已全部合入 `main`（2026-11-28）。** M5-B `#130` / PR `#131`（`1684e66`，760px 同步宽输入舱、6 档纯图标权限、`@agent` 移交与范围开关、三色圆环 + 11 项遥测、IME 门控）；M5-A `#129` / PR `#132`（`dd26757`，`<=8.3Hz` 合批虚拟时间线、5 类工具卡/审批卡/提问卡、400 行 Diff 截断、20px 轮次导航轨、检查点锚点与 `LiveSteps`）；M5-C `#133` / PR `#134`（`13e784d`，右栏 Diff/Files 四页签、7 分区设置、提交/配对/回退/透视模态）。三路各自独立 Worktree 与工作树内 `target/`，并发控制在同时最多两路编译、`CARGO_BUILD_JOBS=4`，全程可用内存未低于 8.9 GB；工作树均已归档、分支已回收。
13. **M6 前置契约已合入 `main`（`#136` / PR `#137`，`2debe53`）。** `soxu-protocol` 新增三组域契约与 `DaemonFeature` 能力位（缺省关闭、未知令牌前向兼容）：受管脚本（`paseo.json` 清单 + 生命周期，无裸 shell 透传，参数按不透明单实参、程序拒绝 shell 与元字符）、Schedules & Heartbeats（cron/interval/heartbeat、幂等键、重试上限）、受控 PTY（有界分片 + 序号 + 打开/输入/尺寸/关闭与输出/退出事件）；`tests/m6_contracts.rs` 覆盖 serde 往返、未知字段容错、非法参数拒绝、分片序号与能力默认关闭。其他 crate、根清单、workflow 与设计系统零改动。
14. **M6 三路实现队列（契约已就绪，可按并发上限放行）**：A 路（`soxu-daemon` 脚本执行器 + 受控 PTY 宿主 + `soxu-desktop` 终端/脚本面板）；B 路（`soxu-desktop` `Cmd+K` 五分区检索 + CodeGraph 符号补全 + 三步 Onboarding）；C 路（Schedules & Heartbeats 调度中心 + 50,000 行 Diff 压测与跨平台打包）。三路 Owned Paths 分别落在 daemon/desktop 不同子模块；同波最多两路并行编译，另一路待其一合入后放行。
11. **构建目录策略调整（2026-11-28 用户决策）**：`mise.toml` 的 `CARGO_TARGET_DIR` 由共享 `../.cargo-target/soxu` 改为检出内 `target/`（`.gitignore` 已注册 `/target/`），工作树归档时构建缓存随目录一并回收；并行隔离规则（L-001）保持「禁 `mise run` + 进程内注入」。旧共享 target 目录（`F:\AI\paseo\worktrees\19r49pwc\.cargo-target\**`）在活动任务结束后统一清理。
13. **M5-A（`#129` / PR `#132`）已与 M5-B（`#131`，`1684e663`）完成合流**：本分支 `git merge origin/main` 后 `shell.rs` 中栏结构为「时间线在上（`flex_1`）、输入舱在下（`composer::render_for_window`）」，两侧组件均未删除；`lib.rs` 同时声明 `timeline` 与 `composer`；合流后 `cargo check/clippy/test -p soxu-desktop`（**177 项**）与增量 `check-all` 全绿，AOCI 重新对齐。
14. **M6 三路实现的前置契约已由 `#136` 契约 PR 冻结（见上）。** 后续 `scripts` / `schedules` / `pty` 三路实现 PR 必须遵守两条衔接红线：①在 `RpcCommand` 中 1:1 转发 `ScriptCommand`/`ScheduleCommand`/`PtyCommand` 并补齐 `dispatch_command` 分支，禁止另起一套请求类型；②客户端未声明 `DaemonFeature` 对应能力前，不得下发 `RpcResultData::{ManagedScript,Schedule,ControlledPty}` 与 `DaemonEvent::{ScriptLifecycleChanged,ScheduleActivity,PtyActivity}`。移动端 `apps/mobile-web/src/lib/daemon-protocol.ts` 与桌面端消费层随实现 PR 同步接入，本契约 PR 未触碰前端。
