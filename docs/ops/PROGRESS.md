# Soxu 研发进度与跨会话交接看板 (PROGRESS)

> **新对话接手指引**：请先阅读根目录 [`AGENTS.md`](../../AGENTS.md)，再结合下方「当前里程碑状态」与 `gh issue list` / `gh pr list` 即可在 30 秒内无损恢复上下文。

---

## 1. 当前总体状态

- `M0`、`M0.5`（PR `#4`）与 **`M1: Protocol v1, Domain Core & Daemon Spine`（`#5` / `#6` / `#7`）已全部完成并合入 `main`**。**`M2: Magpie Discovery & 7 Native CLI Drivers`** 已交付双态发现（`#11`）、原生驱动（`#12`）、Codex、Pi、Claude Code、Qwen Code 本机只读对话（PR `#16`、`#18`、`#20`、`#22`）及 Copilot、OpenCode 本机只读快聊（PR `#24`、`#27`）。**Antigravity（`#26`）延期验收，Issue 保持开放，Daemon 路由保持关闭。延期不阻塞其他阶段，M2 仍有未交付范围。**
- **Pi / Git 波次已收口**（`#34` → `#32` → `#33`）。M2j 的 Pi 原生纠偏和 M3a 的 Git 仓库身份与状态只读探测均已合入 `main`。M3b（`#39` / PR `#41`）已合入 `main`（`7cea1c4`），交付 `GitProbe::open_local_project` 本地项目只读接入：区分普通目录与 `GitWorktree`，拒绝 bare、损坏仓库、Git 元数据目录及权限错误；`soxu-git` 25 项专项测试通过。
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
| **M2** | Magpie Discovery & 7 Native CLI Drivers | M2h、M2j 已完成，M2i 延期 | 已接通六种原生只读 CLI，Pi 原生 `Queue` / `Steer` / `Stop` 已验证。Antigravity 路由保持关闭，七代理全验收仍待完成。 |
| **M3** | Git Worktree Pool, Checkpoints & Commit Engine | M3a–M3d 与 M3e 注册已合入，安全检出在审 | 仓库身份与结构化状态只读探测、本地项目身份、Worktree 只读注册清单和四基准只读 Diff 已合入。`#49` / PR `#53` 已合入，实现显式新分支受控注册与只读诊断。`#51` 交付固定 OID 的受控检出与失败禁用。显式 Init/Clone、Worktree 池、`direct` MCP 挂载、诊断恢复、Checkpoint Diff、安全回退与提交执行待交付。 |
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
   - **Pi 原生控制（`#32`，已合入）**：通过原生 RPC 实现 Queue、Steer 和 Stop，并保持本机只读对话权限。Antigravity（`#26`）保留为开放的延期验收项。
3. **M3 — Git 隔离池、轮次检查点与结构化提交**：
   - **M3a（`#33`，已合入）**：通过本机 Git 只读获取工作树根、独立与共享 Git 目录、HEAD/分支及 `status --porcelain=v2 -z`。
   - **M3b（`#39` / PR `#41`，已合入 `7cea1c4`）**：只读项目身份已交付。
   - **M3c（`#42` / PR `#45`，已合入 `4c5d706`）**：提供 Worktree 只读注册清单，保留原始路径、locked/prunable 原因及 `Unavailable` HEAD 状态。
   - **M3d（`#46` / PR `#50`，已合入 `0d0b4d9`）**：交付 `soxu-git` 内四基准只读 Diff、精确已跟踪路径授权、有界元数据及单文件审阅。
   - **M3e（`#49` → `#51` → `#52`，设计见 `#47` / PR `#48`）**：`#49` / PR `#53` 已合入，终态为「已注册，待检出」。`#51` 在同一协调域核验固定提交，受控检出并保留失败禁用记录，待主控复审。第三阶段 `#52` 核验就绪后由宿主登记、开放 Agent 并挂载 `direct` MCP。详见 [Worktree 生命周期设计](../dev/git-worktree-lifecycle.md)。
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
| [#51](https://github.com/TanCan1998/Soxu/issues/51) M3e 已注册 Worktree 的安全检出 | `feat/51-worktree-checkout` | `soxu-git` 检出接口与测试、crate README、本看板及 AOCI。Windows 保留目录身份拒绝，Linux 负责受控发布。交主控复审与合流。 |

Soxu Paseo 项目为 `prj_24ffa03410264684`，主控工作区为 `wks_21203a9ef0f5e2e5`（`main`）。执行代理完成各自验证、推送和建 PR，主控负责审查、合入与归档。

### 3.2 已合入交付与本 PR 验收证据边界

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
- `#32`（PR `#37`）使用真实 Pi `1.5.2` 和 localhost 模型夹具验证原生 RPC。续接保持同一原生 UUID，活跃回合中的 `Steer` 只进入模型上下文一次，普通 `Queue` 保持 FIFO，`Stop` 结束活跃回合。模型请求携带的工具数为零。PiDeck 参考提交为 [`b9a095e063845c80574c5517994c6fcf2be60f25`](https://github.com/ayuayue/PiDeck/commit/b9a095e063845c80574c5517994c6fcf2be60f25)，仅参考其原生 RPC。
- `#33`（PR `#36`）的 M3a Git 专项 20 项全通过，覆盖真实目录后缀、linked worktree、detached/unborn HEAD 和特殊路径。交付范围限于只读仓库身份与结构化状态。
- 本机使用 `soxu doctor --json` 查看脱敏发现结果，通过 `soxu chat new --read-only` 创建快聊，再用 `soxu chat send <对话或会话 ID> <文本>` 发送，`soxu events` 查看增量，`soxu chat stop <对话或会话 ID>` 停止。Windows 的 Scoop 包装器需用 `SOXU_CODEX_EXE` 指定真实 EXE 的绝对路径。失败或停止后暂停剩余队列，下一次投递从队首继续。队列驻留内存。

- **并发规则**：各阶段公共契约先串行冻结，正交模块按独立 Owned Paths 采用 `2~3` 路 Paseo Worktree 并行；AOCI 托管资产由各分支独立维护，合流后重新对齐。
- **双桌面端主控协同（AGY ⇄ Codex 随时切换）**：本机 AGY 桌面端与 Codex 桌面端已同时导入 `Soxu` 项目，宿主配置与工作树写操作相互隔离，仓库认知（`AGENTS.md` / `aoci.txt` / `.agents/skills/paseo-orchestrator`）与 Paseo 守护进程状态池全面共享；任意时刻均可使用 `pwsh -NoProfile -File scripts/paseo-ctl.ps1` 在两端之间无损切换控制 Paseo。
- **主力 Agent 与模型路由池**（通过 Paseo 调度，经 Magpie 网关路由；**后续派发仅使用 `pi` agent**，并须通过 `scripts/paseo-run-pi.ps1` 绑定项目 `mise.toml` 锁定环境）：
  - **L3 架构/并发攻坚**：`pi` | `magpie/group/gpt-6.2-astra` / `magpie/group/claude-opus-5.5` / `magpie/group/glm-5.4` (`high`)
  - **L2 核心模块/UI 实现**：`pi` | `magpie/group/gpt-6.2-sol` / `magpie/group/claude-sonnet-5.5` / `magpie/auto/gemini-4-hydrogen` (`high` / `medium`)
  - **L1 脚手架/文档巡检**：`pi` | `magpie/auto/deepseek-v4.1-flash-1017` / `magpie/auto/gemini-4-hydrogen` (`low`)

---

## 4. 下一步紧接行动队列（Next Actions）

1. 主控复审 `#49` 的受控注册/只读诊断 PR，补齐可用 Unix 宿主的专项实测；Windows 原生身份与 DACL 依赖另行决策。通过后按 `#47` 分别验收安全检出和宿主接入，显式 Init/Clone 按独立 Owned Paths 排期。
2. Checkpoint 快照模型稳定后接入 Checkpoint 基准 Diff。执行回退前先保存可恢复快照。
3. 结构化提交先实现预检，再接入提交执行。根依赖、协议或其他跨模块公共契约如需变更，单独排契约 PR，合入后再联调。
4. `#26` 继续开放并延期，Daemon 路由保持关闭。恢复验收时覆盖原生入口、配置来源、启动竞态、运行期保护和取消边界。Copilot 项目钩子安全、遥测、历史导入、附件、移交、更高权限及移动端投递另排验收。M2 和 M3 均仍有未交付范围。

