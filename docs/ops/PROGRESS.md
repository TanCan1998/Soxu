# Soxu 研发进度与跨会话交接看板 (PROGRESS)

> **30 秒接手**：先读 [`AGENTS.md`](../../AGENTS.md) → 本文件 §1 里程碑总览与 §2 当前阶段 → 运行 `pwsh -NoProfile -File scripts/paseo-ctl.ps1 workspace ls --json` 与 `ls --json` 盘点实况。
> **历史明细**（逐波次交付、验收证据、旧行动队列）已归档至 [`PROGRESS-ARCHIVE.md`](PROGRESS-ARCHIVE.md)，本文件只保留当前状态与下一步。

---

## 1. 里程碑总览

| 里程碑 | 名称 | 状态 | 核心交付物 |
| :--- | :--- | :--- | :--- |
| **M0** | Bootstrap & Engineering Governance | ✅ 已完成 | 仓库骨架（Rust 2024 + PWA）、`.githooks` + `gitleaks` 隐私与 Emoji 门禁、Actions CI、三层文档、Paseo `git` 项目绑定 |
| **M0.5** | Pure Black Design System & Dual Prototypes | ✅ 已完成 | 纯黑 7 级海拔 `tokens.json` + `shadcn/ui` 变量、MASTER + 7 份分页面规范、双端原型与 7 分区设置、`preview.html`、移动遥控 PWA 原型（PR `#4`） |
| **M1** | Protocol v1, Domain Core & Daemon Spine | ✅ 已完成 | `soxu-protocol` RPC/WS `seq` 契约、`soxu-core` 状态桶/能力协商/红线/队列、`soxu-daemon` SQLite WAL + `afterSeq` 追平 + `#offer=`/6 位码鉴权、`soxu-cli` 闭环（`#5`/`#6`/`#7`） |
| **M2** | Magpie Discovery & 7 Native CLI Drivers | 🟡 主体完成 | 双态发现、7 原生驱动、Codex/Pi/Claude/Qwen/Copilot/OpenCode 只读对话、Pi 原生 Queue/Steer/Stop、Antigravity 隔离快聊（PR `#55`）；**剩余**：Antigravity 项目目录入口、七代理全验收、Copilot 钩子/历史导入/遥测/移动投递 |
| **M3** | Git Worktree Pool, Checkpoints & Commit Engine | 🟡 主体完成 | 只读身份/清单/Diff、受控注册/检出、宿主接入与 Worktree 原生只读会话、显式 Init/Clone、公开 Git 操作契约、Checkpoint 底座、只读恢复预览（`#121`）；**剩余**：恢复执行与 journal、Rewind、结构化提交、`direct` MCP 运行时挂载、Linux 真机端到端 |
| **M4** | Live Mobile PWA & GPUI Workbench Foundation | ✅ 已完成 | PWA 去 Mock 接真实 Daemon WS、`rust-embed` 伺服、`#offer=`/6 位码配对、13 组视口真实渲染验收（PR `#126`）；GPUI App/主题/字体/i18n/焦点栈/四栏壳层（PR `#125`）；令牌三产物零漂移（PR `#120`） |
| **M5** | GPUI Core Views: Transcript, Composer & Inspector | ✅ 已完成 | 时间线（`<=8.3Hz` 合批、5 类工具卡/审批卡/提问卡、20px 导航轨，PR `#132`）；输入舱（760px 同步宽、6 档权限、移交、三色圆环 + 11 项统计，PR `#131`）；右栏 Diff/Files、7 分区设置、核心模态（PR `#134`） |
| **M6** | Scripts, PTY Terminal, Search & Automation | 🟡 进行中 | 契约（PR `#137`）；脚本执行器 + 受控 PTY 宿主（PR `#142`）；`Cmd+K` 五分区检索 + Onboarding（PR `#141`）；调度执行器 + 桌面调度中心 + 50k Diff 压测（PR `#146`）；RpcCommand 传输契约（PR `#149`）；**进行中**：daemon 端到端接线（`#150`）；**剩余**：M6-D 发版级打包 dry-run |

---

## 2. 当前阶段（2026-11-28）

- **主线基线**：`main` = `08bd29f`（M6 传输契约合入）。
- **最近合入**：M6 传输契约 `#149`（`08bd29f`，14 个 `RpcCommand` 变体 + `Unsupported` 占位扩围）、M6-C `#146`（`0ddd041`）、看板重构 `#145`（`f360bec`）、M6-A `#142`（`8097f12`）、M6-B `#141`（`edbe47b`）、M6 契约 `#137`（`2debe53`）、M5-C/B/A、M4-W2A/B、构建目录治理 `#127`。
- **进行中**：`#150` M6 daemon 端到端接线（三域 handler 替换 `Unsupported` + 能力门控 + 事件广播），工作树 `feat-150-m6-daemon-wiring`（`wks_c98ded19f8b827eb`，agent `4ed1e02`）。
- **已知缺口（真实）**：daemon 传输层尚未把 14 个变体接到域服务；`#150` 完成后才能端到端调用脚本/PTY/调度。
- **门禁**：三平台 Rust（windows/linux/macos-arm64）+ PWA + 格式 + 隐私 + `CI Gate`；合入统一 `--match-head-commit`，合并后归档工作树与分支。
- **尚未交付**：Git 恢复执行/回退/结构化提交、`direct` MCP 运行时挂载、非 Windows 宿主真机补测、Antigravity 项目入口与全代理验收、正式发布（未核验原生测试）。

---

## 3. 并发与构建策略

- **派发引擎**：仅 `pi` agent + 经 `pi --list-models` 核实的 `magpie/group/jev-adaptive`（`high`）；严禁 `codex`/`claude`/`antigravity` 派发。
- **并发上限**：同波最多两路编译（GPUI 依赖树较重）；第三路待其一合入后放行。`CARGO_BUILD_JOBS=4`，实测可用内存下限约 6.4 GB，未触发 OOM。
- **构建缓存**：`CARGO_TARGET_DIR` 已改为各检出自带 `target/`（`mise.toml` + `.gitignore` `/target/`），工作树归档即回收；不再使用共享 `.cargo-target`。
- **工作树纪律**：每路独立 Worktree + 分支 + Owned Paths；共享接线点（如 `shell.rs`/`lib.rs`）仅允许单行挂载，冲突由主控合流解决；合入后立即 `workspace archive` 并回收本地分支。

---

## 4. 下一步行动队列（Next Actions）

1. **M6 端到端接线**：`#150` 审查/合入（三域 handler 替换 `Unsupported` + 能力门控 + 事件广播），然后做真实 daemon 冒烟（脚本运行、PTY 输出、调度触发各一条）。
2. **M6 daemon 接线**：消费 `into_domain_command()`，把域命令转给 `scripts::ScriptService` / `schedules::ScheduleExecutor` / `pty::PtyHub`，按能力位判定广播事件，再做一次真实端到端冒烟（脚本运行、PTY 输出、调度触发）。
3. **M6-D 打包与发布核验**：`soxu-releases` 四平台构建已在 `cargo build` 前执行 `pnpm build`（Linux/Windows/macOS 均确认），并含活二进制校验与 `soxu`+`soxu-daemon` 双产物；正式发版前补一次 tag 级 dry-run。
4. **M3 剩余**：恢复执行与 journal（`execute.rs`/`journal.rs`）、Safety Snapshot 先行 Rewind、文件级结构化提交预检；按设计文档顺序，涉及协议变更先走独立契约 PR。
5. **`direct` MCP 运行时挂载**与 Worktree 内写权限：需先冻结协议面，再接入 daemon。
6. **非 Windows 宿主补测**：`#49`/`#51`/`#56`/`#69` 的真实注册、固定提交检出、宿主接入、路由重试与 Worktree 原生会话。
7. **M2 收尾**：Antigravity 项目目录入口、七代理全验收；Copilot 钩子安全、历史导入、附件/移交/更高权限与移动端投递。
8. **发布**：未核验的跨平台原生测试与正式发版仍为待办；Intel 仅保留 `full-with-intel` 手动入口与 tag 入口。

---

## 5. 参考项目与索引

| 参考 | 版本 | AOCI | CodeGraph | 说明 |
| :--- | :--- | :--- | :--- | :--- |
| `paseo` | `7bcd598` | 202 条 | 148,292 节点（有漂移提示） | 工作区/Worktree/远控边界 |
| `codeg` | `028d30e` | 177 条 | 84,541 节点 | 会话标签页/文件夹命令 |
| `padu` | `fbe97a38` | 1,086 条 | 17,171 节点 | GPUI 主题/焦点/布局结构 |
| `t3code` | `f3e36337` | **无（用户决策：仓库过大不全量建）** | 541,919 节点 | 控制平面/contracts/client-runtime 源码级参考 |

> `.refs/` 不随仓库版本化；复用既有索引，禁止复制/重建/同步到目标工作树；索引不可用时回退 `rg` + 源码。

---

## 6. 历史归档

- [`PROGRESS-ARCHIVE.md`](PROGRESS-ARCHIVE.md)：2026-11-28 前的完整波次记录、验收证据边界与旧行动队列（M0–M3 CI 攻坚、Wave 3.x/4.x、热修复链）。
- [`LESSONS.md`](LESSONS.md)：活跃规则速查表（L-001…L-024）。
- [`DECISIONS.md`](DECISIONS.md)：ADR-001…ADR-012。


