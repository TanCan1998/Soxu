# 资源生命周期与事件留存治理方案

> 范围：Soxu daemon 进程内存/句柄、SQLite 事件日志与磁盘、受控 PTY/脚本子进程、桌面与 PWA 会话模型。
> 依据：2026-11-28 主控源码审计 + 两路只读调研（Paseo `jev-adaptive` 子代理）+ 参考项目既有索引 + 一手官方文档。
> 状态：方案已定，按 §5 分期实施；未实施部分不得声称已修复。

## 1. 结论摘要

当前不存在已证实的「内存无限泄漏」单点，但存在 **3 个 P0 正确性缺口**（会把容量/追平语义弄错）和 **4 个 P1 资源增长面**：

1. **P0 追平分页会漏事件却报完成**：WS 握手只补一页（1000 条）就发 `CatchUpCompleted`；移动端收到 `ServerHello` 即把游标推进到 `latest_seq`。离线期间 >1000 条事件时会静默缺口。
2. **P0 PTY 生命周期未闭环**：自然退出不置 `closed`、关闭/失败会话不从宿主表摘除、`reap_idle` 无生产调用者、daemon 无优雅关停与线程 join。容量会被永久占住，`sessions` 表随开关增长。
3. **P0 容量检查存在 check-then-act 竞态**：PTY `live_sessions()` 与脚本 `active` 都是先检查后递增，并发请求可越过上限。
4. **P1 事件日志只增不删**：`event_log` 无保留窗口、无 `P/H` 边界、无 checkpoint/VACUUM 维护；PTY 每个 16 KiB 分片都会成为一行。
5. **P1 PWA/桌面长会话无保留上限**：消息数组与时间线模型随历史线性增长（渲染虚拟化只限制绘制，不限制模型）。
6. **P1 调度节拍无宿主 tick**：`tick_schedules`/`finish_schedule_run` 可调用但无生产调用者（功能缺口）。
7. **P1 关停路径未证明**：脚本执行任务 `tokio::spawn` 未保存句柄，`kill_on_drop` 只覆盖直接子进程；daemon 退出时不保证子孙树收敛。

修复顺序必须是：**先修语义正确性（追平/快照/PTY 生命周期/并发上限），再做物理删行与磁盘回收，最后做 UI 保留上限**。顺序颠倒会把「裁剪导致的缺页」误判为客户端 bug。

## 2. 现状实测与口径

| 项目 | 本次值 | 口径 |
| :--- | :--- | :--- |
| 仓库 `.git` | 17.9 MB | 实测 |
| `.refs` 参考源码 | 3.79 GB | 实测（含 t3code CodeGraph 库 2.2 GB） |
| 单工作树 debug `target/` | 1.28 GB | 实测（部分构建闭包） |
| PWA `dist` | 628.3 KB | 实测（入口 gzip 约 192 KB，来自 #126） |
| 双路并行构建可用内存 | 最低 6.4–6.8 GB | 实测（`CARGO_BUILD_JOBS=4`） |
| daemon 空闲 RSS | 30–60 MB | **预估**（本次无可执行二进制，未实测） |
| 每 PTY 会话 | +5–15 MB | 预估（8 KiB 读缓冲 + ConPTY/openpty 原生侧） |
| 脚本单次运行 | +1–5 MB，上限 8 并发 | 预估（输出预算 256 KiB/运行、保留 64 条） |
| 桌面 GPUI 空闲 | 150–300 MB | 预估 |
| 桌面长会话 | 300–600 MB 或更高 | 预估；模型随历史增长 |
| 移动端 PWA 页签 | 80–150 MB | 预估 |

**内存读数必须分进程**：PTY shell/子孙进程、daemon、GPUI 桌面、PWA 浏览器是四个独立工作集。Rust 对象释放后 allocator 未必立即归还操作系统；Windows `WorkingSet64` 包含共享页且受系统修剪影响，不能当作活跃对象字节数。

已确认的有界点：PTY 上限 8、脚本 8 并发/64 保留/256 KiB 输出、搜索 24 条 LRU、广播容量 512（慢消费者丢弃）。

## 3. 风险分级与证据

### P0-1 追平分页与快照交接

- WS 握手固定补 1000 条（`crates/soxu-daemon/src/server.rs:279`），随后无条件发送 `CatchUpCompleted`；`Lagged` 补 500 条（`server.rs:349`），查询失败被 `if let Ok` 静默跳过。
- 移动端收到 `ServerHello` 先 `lastAppliedSeq = max(..., hello.latest_seq)`（`apps/mobile-web/src/lib/daemon-client.ts:189`），再处理补读帧。
- 快照与 `latest_seq` 分两次读取（`server.rs:256`/`259`），两值之间提交的更新可能被 `seq > last_sent_seq` 过滤。
- 反例：离线期间产生 1001 条事件，重连且无新写入 → 第 1001 条永不送达，客户端仍显示「已追平」。

### P0-2 PTY 生命周期

- `close()` 置 `closed` 并 `terminate`，但 `PtyHost.sessions` 无删除路径（`crates/soxu-daemon/src/pty/mod.rs:228` 插入，全文件无 `remove`）。
- 输出泵自然结束只 `finish_stream()`（`pty/session.rs:275`），不置 `closed`；`live_sessions()` 按 `!is_closed()` 计数 → 自然退出会话永久占容量；`reap_idle()` 只选 `Running`，不会回收它们。
- `reap_idle` 全仓无生产调用（仅定义与测试）。
- `main` 直接 `axum::serve(...).await`（`main.rs:333`），无 graceful shutdown；PTY 泵线程 `JoinHandle` 未保存，无 Drop 清理。

### P0-3 容量 check-then-act

- PTY：`live_sessions() >= 8` 检查后派生、再插入表（`pty/mod.rs:184`→`228`）。
- 脚本：`active.load() >= 8` 检查后，`tokio::spawn` 前才 `fetch_add`（`scripts/runner.rs:298`→`336`）。
- 并发下可越过上限；PTY 还会实际多派生 shell 进程。

### P1-1 事件日志与磁盘

- `event_log` 只有 `INSERT` 与 `SELECT`（`store.rs:588`/`616`），无 `DELETE`、保留窗口、`VACUUM`、`wal_checkpoint`。
- `seq` 为 `INTEGER PRIMARY KEY AUTOINCREMENT`，但 `latest_seq` 取 `MAX(seq)`（`store.rs:609`）；删空后会回退到 0，而下次插入仍继续大序号。
- PTY 每 16 KiB 输出写一行事件；一次长终端会话即可产生大量行与 WAL。

### P1-2 UI 模型增长

- 桌面 `TimelineSnapshot` 持有完整 turns/rows/metrics/tops（`crates/soxu-desktop/src/timeline/model.rs:265`）；虚拟窗口只限制绘制（`window.rs`）。
- PWA `turns` 按 ID upsert、无截断（`apps/mobile-web/src/main.tsx:430`）。

### P1-3 调度与关停

- `tick_schedules`（`state.rs:1212`）与 `finish_schedule_run` 无生产调用者。
- 脚本 `tokio::spawn` 句柄未保存（`runner.rs:358`）；`kill_on_drop(true)` 仅覆盖直接子进程。

### P2 语义缺陷（需一并修）

- 移动端 `applyEventToPatch(env, {})` 以空补丁调用，reducer 从传入列表合并，单条工作区更新可能把多项列表替换为单项（`apps/mobile-web/src/lib/daemon-store.ts:190`、`main.tsx:398`）。

## 4. 参考方案对照

| 来源 | 采用点 | 不采用/边界 |
| :--- | :--- | :--- |
| **t3code** `OrchestrationEventStore.ts:38`、`ProjectionMaintenance.ts:133/549`、`ShellStream.ts:54` | 双序号 `H`（日志头）/`P`（已裁剪到）；`prunedThroughSequence` 判定 `after < P` 必须重取快照；到期标记与投影检查点取小；先发布边界再分批删行；固定头分页、回放后复查边界 | 其 1 天留存与 shell 回放预算是产品参数；Soxu 还要覆盖内存域、审批审计与 PTY 历史，需自证恢复来源 |
| **Paseo** `packages/server/src/terminal/terminal-restore.ts` | 观察实际传输积压；快照成功才替代待发数据；按能力分块恢复 | 其终端可恢复屏幕，Soxu 的全局事件还含审批等不可合并操作，不能照搬丢帧规则 |
| **Padu** `apps/desktop/src/app/runtime.rs:2721` | idle provider runtime 守卫：无活动 turn、无后台工作、会话 Idle/Failed 才关闭 | 其 Web 转录仍从完整消息建项，虚拟化不等于历史上限 |
| **SQLite WAL** <https://sqlite.org/wal.html> · <https://sqlite.org/c3ref/wal_checkpoint_v2.html> | 读者固定快照；checkpoint 受长读者限制；PASSIVE/TRUNCATE 与帧计数区别 | 未给出 Soxu 负载下的合适周期，需夹具实测 |
| **SQLite Pragma/VACUUM** <https://www.sqlite.org/pragma.html> · <https://sqlite.org/lang_vacuum.html> | `page_count`/`freelist_count` 估算可复用空间；`INCREMENTAL` 前提；`VACUUM` 可能需约两倍库空间 | 已有 `auto_vacuum=NONE` 库切换需重建，必须备份与维护窗口 |
| **Tokio broadcast** <https://docs.rs/tokio/latest/tokio/sync/broadcast/index.html> | `Lagged(n)` 表示接收器移到环内最老项；应用层从持久日志补洞 | 广播层不负责游标、字节预算与完成条件 |
| **Tokio process** <https://docs.rs/tokio/latest/tokio/process/struct.Command.html> | `kill_on_drop` 仅请求终止直接子进程；关停需显式进程树策略 | 不能把 drop 当成子孙树收敛证明 |
| **Microsoft PeakWorkingSet64** <https://learn.microsoft.com/en-us/dotnet/api/system.diagnostics.process.peakworkingset64> | 进程启动以来物理工作集峰值口径 | 含共享/私有驻留页，不能归因到 SQLite/序列化/WS 缓冲 |

## 5. 分期实施计划

### 阶段 0：观测与基线（P0 前置，1 个小 PR）

- `scripts/collect-runtime-metrics.ps1`：对指定 PID 采集 `WorkingSet64`/`PrivateMemorySize64`/句柄数/线程数；对数据目录采集 `.db`、`-wal`、`-shm` 长度、`page_count`、`freelist_count`、事件行数（只读 SQL）。
- 固定负载脚本：空载、1 PTY、8 PTY、2 脚本、1000 事件、10k 事件各采样一次，结果入 `docs/ops/PROGRESS.md` 或独立报告。
- 验收：同一 PID 三次采样波动 <10%；不把一次删行差值换算成 RSS 收益。

### 阶段 1：追平与快照交接（P0）

- Owned Paths：`crates/soxu-daemon/src/server.rs`、`state.rs`、`crates/soxu-protocol/src/rpc.rs`（仅新增诊断字段/错误码，保持 v1 兼容）、`apps/mobile-web/src/lib/daemon-client.ts`、`daemon-store.ts`、`main.tsx`。
- 语义：`H` 为握手固定头；分页从本页最后成功应用序号续读；只有覆盖到 `H` 才发完成；`after=P` 可续读，`after<P` 返回「需重取快照」并带 `P/最早现存/H`；`Lagged` 走同一逻辑；每连接限制排队字节与单轮补读预算。
- 移动端：`ServerHello` 不得先把游标推到 `latest_seq`；以快照序号 `S` 建立基线，再接 `seq>S`；修正 `applyEventToPatch` 传入当前列表。
- 验收：离线 1001 条、手动补读 501 条、广播溢出 >500、分页中途裁剪、同序号完成帧、一快一慢读者；E2E 断言完整序列与最终视图。
- 回滚：保留旧握手字段；新版客户端可被旧 daemon 拒绝并明确提示升级。

### 阶段 2：PTY/脚本生命周期（P0）

- Owned Paths：`crates/soxu-daemon/src/pty/{mod,session}.rs`、`scripts/runner.rs`、`main.rs`、`tests/`。
- 修复：容量检查与插入在同一把锁内（预留位或 CAS）；自然退出置 `closed` 并退出容量计数；`close`/`Exited`/`Failed` 后从 `sessions` 摘除（保留最后快照给重复 close）；`reap_idle` 接入宿主周期任务；daemon 增加 `with_graceful_shutdown` 并保存/join 泵线程；脚本用 semaphore 替代 check-then-act，保存任务句柄并在关停时按进程树收敛。
- 验收：并发 open 不超过 8；自然退出后容量释放；开关 100 次 `sessions` 条目归零；daemon SIGINT 后无存活 PTY 子孙（Windows 用 Job Object/taskkill 证据）；脚本并发不超 8。
- 回滚：保留旧 API；新增周期任务可用环境变量关闭。

### 阶段 3：事件留存与磁盘（P1）

- Owned Paths：`crates/soxu-daemon/src/store.rs`、`state.rs`、`main.rs`、`tests/retention.rs`。
- 双序号：`meta` 持久化 `head_seq`（单调）与 `pruned_through`；删行只推进 `P`，`H` 不回退；`CatchUpEvents` 在 `after<P` 时返回可诊断错误（`ErrorCode::Conflict` 或新增专用码）并携带边界。
- 保留策略：默认「条数上限 + 最短保留时长」双条件，按固定 `H` 取连续前缀；每批限行数/字节/持锁时间；先发布 `P` 再删行；崩溃后幂等续删；维护窗口执行 `wal_checkpoint(PASSIVE)`，`TRUNCATE`/`VACUUM` 单独授权。
- 验收：老库升级、空库、删空后再追加、`P-1/P/H/H+1`、并发写+维护、备份恢复；删行前后 `.db`/`-wal`/`freelist` 数据；关闭开关后不再删除。
- 回滚：保留备份；关闭策略仅停止后续删除；不自动 `VACUUM`。

### 阶段 4：UI 模型保留上限（P1）

- 桌面：时间线按「最近 N 轮 + 字节预算」保留，超出折叠为摘要，当前状态/待审批永不裁剪；导航轨按摘要索引。
- PWA：`turns` 按会话设置上限与分页回填；切换会话释放旧模型。
- 验收：10k 轮夹具下模型字节上界、滚动/导航正确、待审批不丢。

## 6. 测量方法（必须区分口径）

| 对象 | 指标 |
| :--- | :--- |
| daemon | `WorkingSet64`、`PrivateMemorySize64`、句柄数、线程数、PTY 表条目（存活/终态）、脚本 active/retained |
| 子进程 | 每个 PTY/脚本的进程树数量与存活状态（daemon 快照不能替代） |
| SQLite 主库 | 文件长度、实际分配字节、`page_count*page_size`、`freelist_count*page_size`、事件行数 |
| WAL | `-wal`/`-shm` 长度；`wal_checkpoint(PASSIVE)` 的 busy/帧数/耗时（标记为主动维护） |
| 磁盘归还 | `auto_vacuum` 模式；`incremental_vacuum(N)` 或维护窗口 `VACUUM` 的实测差值 |
| 桌面/PWA | 各自进程工作集；不并入 daemon RSS |

## 7. 测试矩阵

- 追平：空库、1001 条积压、501 条手动补读、广播溢出 >500、分页中裁剪、`P-1/P/H/H+1`、同序号完成帧、快慢读者并存。
- PTY：并发 open×16、自然退出、显式 close、terminate 失败、线程创建失败、开关 100 次、daemon 关停、空闲 15 分钟（注入时钟）。
- 脚本：并发 run×16、Stop/超时、reap 未确认、runtime 取消、daemon 关停。
- 留存：老库升级、删空后追加、备份恢复、维护中断、关闭开关。
- UI：10k 轮、待审批保留、切换会话释放。

## 8. 非目标

- 本方案不引入新数据库或外部队列；不改变协议主版本；不做无备份的自动 `VACUUM`/删库。
- 不把「虚拟化渲染」当作「历史数据有上限」；不把广播层丢弃当作追平成功。
- 未完成阶段不得在 PR/看板声称已修复；每阶段独立 PR、独立回滚与实测证据。
