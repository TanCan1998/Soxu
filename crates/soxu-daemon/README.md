# soxu-daemon

Soxu 核心守护进程：持久化状态机、WebSocket 游标事件流、移动配对鉴权与移动端 PWA 静态资产伺服。

## 运行

```powershell
cargo run -p soxu-daemon
```

- 默认监听 `127.0.0.1:7698`（`SOXU_LISTEN` 覆写）。
- 数据目录默认 `~/.soxu`（`SOXU_DATA_DIR` 覆写）；目录级文件锁防止双实例。
- 非回环监听地址自动要求全部请求携带设备 Bearer 令牌（`require_auth_for_all`）。
- M6 能力位（受管脚本 / 调度 / 心跳 / 受控终端）**默认全关**，**只能由 daemon 宿主进程环境**
  `SOXU_DAEMON_FEATURES`（逗号分隔令牌）开启；客户端无法通过 RPC 开启任何能力位，
  安全边界与过渡期说明见下文「M6 端到端接线」。

## 移动端 PWA 伺服（rust-embed）

守护进程把 `apps/mobile-web/dist` 的构建产物内嵌进二进制：

- 构建前先在 `apps/mobile-web` 下执行 `pnpm run build`；
- `dist` 缺失时 `#[allow_missing = true]` 让守护进程照常编译，访问 `/` 返回 500 与明确的中文构建指引，不 panic；
- 路由语义：`/` 与无扩展路径回退 SPA `index.html`；带扩展名的路径必须真实命中嵌入资产，未知资产返回 404；`/api/*` 与 `/ws` 未注册路径返回协议错误 JSON 404，不被 SPA 回退吞掉；
- 缓存策略：`assets/*`（内容哈希文件名）`immutable` 长缓存，入口与 manifest `must-revalidate`；
- 安全边界：静态资产不含任何守护进程凭据；配对令牌仅经 `/api/rpc` 的 `ClaimPairing` 下发，快照响应不回显令牌。

## 移动端配对

- 桌面端 `CreatePairingOffer` 生成 `#offer=` 一次性扫码链接与 6 位码（5 分钟有效，认领即作废防重放）；
- 移动端凭 `PairingCredentialInput`（扫码令牌或 6 位码）认领后获得持久 Bearer 令牌（仅 SHA-256 哈希落盘）；
- WS 支持经 `Authorization: Bearer` 头或 `?token=` query 携带令牌（移动端浏览器无法自定义 WS header 的降级通道）；
- 设备支持吊销（`RevokePairedDevice`）与空闲自动锁定（`remote_idle_lock_min`）。

## 受管脚本执行器（M6-A）

`crates/soxu-daemon/src/scripts/**`，消费 `soxu-protocol::scripts` 契约：

- **清单**：只读解析工作区根的 `paseo.json`，`scripts` 必须是 `ManagedScriptEntry` 数组；
  历史形态 `scripts: { 名称: { command: "..." } }`（shell 字符串）一律整份拒绝，
  不做「按空白切分再执行」的降级。文件 >256 KiB、非 UTF-8、条目 >64 条、`script_id` 重复均拒绝。
- **派生**：`program + 参数数组` 直接 `spawn`，全程不经 shell；带路径分隔符的程序必须是已存在的绝对路径，
  裸名按 `PATH` 查找。工作目录解析到真实路径后必须仍落在工作区根内（含符号链接指向区外）。
- **环境**：默认 `env_clear()`，只按 `env_policy.allow_keys` 从守护进程环境注入同名值；
  `inherits_parent_env = true` 才继承全部父进程环境。
- **预算**：单次运行 stdout/stderr 合计上限 256 KiB，超出丢弃并置截断标记，绝不无界缓冲。
- **终止**：`timeout_seconds` 到期或客户端 `stop` 后走「有界等待 → 进程树终止 → 回收窗口」：
  Unix 让子进程自成进程组后用 `kill -TERM/-KILL -- -<pgid>`，Windows 用 `taskkill /PID <pid> /T [/F]`；
  回收结论四态可判别（`NotRequired` / `ExitedAfterStop` / `ReapUnconfirmed` / `SpawnFailed`），
  回收窗口内未确认即如实报 `ReapUnconfirmed`，不宣称已回收干净。
- **审批**：`risk` 为 `high` / `critical` 的条目必须携带 `approval_id`，否则返回 `PolicyDenied` 且不派生任何子进程。
- **幂等**：同一 `idempotency_key` 收敛到同一 `run_id`，不重复执行副作用。

## 调度与心跳执行器（M6-C）

`crates/soxu-daemon/src/schedules/**`，消费 `soxu-protocol::schedules` 契约：

- **求值**：`cron.rs` 是 cron 语义的唯一求值点（5 段式、UTC、周字段 `0/7` 等价、
  范围步进从下界起算、日与周同时受限时取并集）；按「天 → 时 → 分」推进，
  闰日与月末组合在 8 年窗口内有界收敛，无命中即拒绝创建（不做忙轮询降级）。
- **时钟**：全部时间来自 `schedules::Clock`；生产用 `SystemClock`，测试用 `ManualClock` 推进，
  `tick` 不依赖真实等待，也不持有任何 `sleep`。
- **持久化**：`ScheduleStore` 单文件 JSON + `rename` 原子替换，体积上限 4 MiB，
  损坏 / 版本未知 / 标识重复一律 fail-closed 拒绝加载；恢复后仍逐条复核节奏与有效期。
- **幂等**：创建按 `idempotency_key` 收敛到同一定义，手动触发按 `idempotency_key`
  收敛到同一 `Triggered` 事件（每调度的键表上限 256 条，先进先出淘汰）。
- **重试**：失败 / 超时沿用同一 `run_id` 与同一 `scheduled_for`，退避上限 3600 秒，
  次数达到 `retry.max_attempts` 才下发一次 `RunFinished` 收敛结论。
- **不补发历史**：错过的触发点直接前进到 `now` 之后的最新一次，绝不按积压次数连发；
  心跳 `only_when_idle = true` 在 Agent 忙碌时跳过并前移周期（`tick_with_idle`）。
- **停用 / 删除收敛**：停用丢弃在途重试并把 `next_run_at` 置空；删除同时清理创建幂等索引、
  触发幂等键与在途登记，删除后到达的运行结果只丢弃，绝不复活定义、也不下发事件。
- **能力门控**：`Schedules` 管理 cron / 固定周期，`Heartbeats` 单独管理心跳；
  未声明时命令返回 `ErrorCode::Unsupported`，`tick` 也不为对应调度下发任何事件
  （`TickReport::gated` 如实记录被压制的条数）。
- **事件出口**：`ScheduleEventSink` 由宿主注入，事件只在持久化成功之后下发；
  `NullScheduleEventSink` 供只读宿主与回归使用。
- **传输层**：`RpcCommand` 的 6 个调度变体已由守护进程接线（见下节「M6 端到端接线」）。

## 受控 PTY 宿主（M6-A）

`crates/soxu-daemon/src/pty/**`，消费 `soxu-protocol::pty` 契约：

- 打开 / 输入 / 尺寸 / 关闭四类命令 1:1 对应；同时存活会话上限 8 个，空闲 15 分钟自动回收（`IdleTimeout`）。
- 输出按 `PTY_OUTPUT_CHUNK_MAX_BYTES`（16 KiB）切片，`sequence` 从 0 起严格递增；
  流结束时补一个空 `end_of_stream` 终结片并下发一次 `Exited`（携带 `last_sequence`）。
  增量 UTF-8 解码保留尾部残字节，多字节字符被读窗口截断时不会出现替换字符。
- 关闭幂等：重复关闭返回当前快照，不报错、不重复下发 `Closed`；已关闭会话拒绝写入（`Conflict`）。
- 真实后端经 `portable-pty`（Windows ConPTY / Unix `openpty`）。本工作区 `unsafe_code = "forbid"`，
  无法自行封装系统调用，因此把系统调用面收敛到一个受审计的跨平台 crate。
- 受控 shell 由 `SOXU_PTY_SHELL`（绝对路径）或平台默认值确定，**不接受客户端指定**：
  Windows 优先 `%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe`（应用包 `WindowsApps` 别名目录
  不可按路径启动，已排除），Unix 用 `$SHELL` 或 `/bin/sh`。终端必须派生交互 shell，与脚本域「严禁解释器」的红线互不冲突。
- 环境白名单之外，Windows 固定补 `SystemRoot`/`WINDIR`（机器级、不涉凭据）：ConPTY 的环境块为空时
  `CreateProcessW` 返回 `os error 87`，该基线是启动前提而非能力放宽。
- 宿主装配不会因为本机没有可用 shell 而启动失败：`pty::discover_or_unavailable` 在发现失败时
  返回 `UnavailablePtyBackend`，只在真正 `Open` 时回 `DriverUnavailable` 三要素错误。
- **传输层**：`RpcCommand` 的 4 个 PTY 变体已由守护进程接线（见下节）。

## 能力门控（M6-A）

`DaemonFeature::ManagedScripts` / `ControlledPty` / `Schedules` / `Heartbeats` **缺省关闭**。门控统一由
`scripts::FeatureGate` 判定：未声明时对应域整族命令一律返回
`ErrorCode::Unsupported` 的三要素错误，且不下发任何 `ScriptLifecycleChanged` / `PtyActivity` / `ScheduleActivity` 事件。
`FeatureGate::from_csv("managed_scripts,controlled_pty,schedules,heartbeats")` 供宿主按配置装配。

## M6 端到端接线（#150）

`crates/soxu-daemon/src/state.rs` 把 `soxu-protocol` 的 14 个 M6 `RpcCommand` 变体从占位 `Unsupported`
移出，经 `RpcCommand::into_domain_command` 折叠为 `RpcDomainCommand` 后 1:1 转发到三域服务：

| 变体 | 域服务 | 前置条件 |
|---|---|---|
| `ListScripts` / `RunScript` / `StopScript` / `InspectScript` | `ScriptService::handle` | `managed_scripts` 已声明 |
| `ListSchedules` / `CreateSchedule` / `UpdateSchedule` / `DisableSchedule` / `DeleteSchedule` / `TriggerSchedule` | `ScheduleService::handle` | `schedules` 或 `heartbeats` 已声明 |
| `OpenPty` / `WritePty` / `ResizePty` / `ClosePty` | `PtyHost::handle` | `controlled_pty` 已声明 |

- **能力位来源：只有 daemon 宿主进程能开启**。令牌来自 `SOXU_DAEMON_FEATURES`（逗号分隔；
  未设置、空串与纯空白一律解析为全关）或宿主在装配阶段调用 `DaemonState::with_feature_gate`。
  **RPC 面没有任何开启入口**：`RpcCommand` 不提供能力声明/灰度命令，客户端既不能自报能力、
  也不能修改门控——`DaemonFeatureSet` 只用于**判定**请求是否放行，绝不接受客户端主张。
  改变门控的唯一路径是重启 daemon 并给它不同的宿主环境（或宿主代码改用 builder），
  因此“声明后执行”永远不会变成客户端可自行提权的开关。
- **安全边界（过渡期门控）**：`ClientHello` 尚无能力字段（`soxu-protocol` 冻结），因此门控是
  **守护进程级**而非按连接级——一次开启对连到该 daemon 的所有客户端生效，M6 事件也经同一订阅
  通道下发给所有订阅者。这是协议缺能力字段期间的过渡方案：将来握手携带能力集合时只需灌入同一个
  `FeatureGate`，即可收敛为按连接级而判定逻辑不变。部署方应按最小权限原则，只给确实需要
  脚本/终端/调度能力的 daemon 实例开启对应令牌（默认配置不需要也不应该开启）。
- **顺序红线**：先判能力位、再解析工作区根、最后才调域服务。能力位关闭时即便是未知
  `workspace_id` / `schedule_id` 也返回 `Unsupported`（而非 `NotFound`），且零事件下发。
- **工作区根解析**：`Local` 工作区取项目登记的 `root_path`（绝不使用 `~` 脱敏展示路径），
  `Worktree` 复用宿主接入复核（`resolve_ready_worktree_dir`）；归档或待恢复的工作区一律拒绝。
- **停得动优先**：`StopScript` / `InspectScript` / `WritePty` / `ResizePty` / `ClosePty` 以域内标识定位，
  **不**解析工作区根——停掉一个已经失去工作区的失控进程树或关闭失效终端会话不能被路径复核挡住。
  为此 `ScriptService::handle` 与 `PtyHost::handle` 的工作区根参数为 `Option<&Path>`。
- **事件广播**：三域事件经 `DaemonEvent::ScriptLifecycleChanged` / `ScheduleActivity` / `PtyActivity`
  走与其余事件完全相同的落盘 + 广播管线（同一把顺序锁、同一 `seq` 序列），因此 `seq` 严格单调、
  断线可用 `after_seq` 追平；事件只经订阅通道下发，未订阅的客户端不收到、也不回放历史。
- **审批与注入面**：RPC 层原样透传 `approval_id` 与全部参数，不重写、不预校验；高风险审批门、
  清单注入面校验、输出上限、进程树回收、会话上限全部沿用域服务，接线层不新增旁路。
- **运行归属**：`RunScriptRequest.workspace_id` 原样写入运行快照与生命周期事件（不再由工作区根末段猜标识），
  客户端可直接把事件回挂到具体工作区。
- **调度持久化**：有数据目录时 `ScheduleService` 绑定 `<data_dir>/schedules.json`（fail-closed 加载）。
- **宿主驱动口**：`DaemonState::tick_schedules(agent_idle)` 推进一次调度（cron / 周期 / 心跳），
  `DaemonState::finish_schedule_run(result)` 收敛宿主执行完的触发结果。二者由宿主按自身节拍调用
  （独立可执行入口的节拍驱动不在本 PR 的 Owned Paths 内）。

> 传输层现状：`server.rs` 无需改动——HTTP `/api/rpc` 与 WS 上行都经 `handle_transport_rpc` →
> `dispatch_command` 落到本接线，事件沿用既有广播信封自动下发。

## 测试

```powershell
cargo test -p soxu-daemon
```

覆盖 HTTP/WS 端到端、`afterSeq` 增量追平、配对全生命周期、原生驱动只读会话、PWA 伺服 404/200/鉴权回归，
以及 M6-A 专项 `tests/scripts_pty.rs`（清单注入面、透传参数越权、高风险审批、输出预算截断、
超时后进程树有界回收、环境白名单隔离、幂等键去重、PTY 分片序号与终结片、关闭幂等、
会话上限与空闲回收、真实 ConPTY/openpty 冒烟），以及 M6-C 专项 `tests/schedules.rs`
（门控缺省关闭与事件零下发、创建/手动触发幂等、注入时钟下的周期推进与跳过补发、
失败重试沿用同一次运行、停用/删除收敛、有效期过期、定义持久化恢复、非 UTC 时区拒绝），
以及 M6 端到端接线专项 `tests/domain_wiring.rs`（能力关闭时 14 个变体拒绝且零事件、
部分声明只放行自身域、声明后 14 个变体全部真实路由、脚本生命周期事件与 `seq` 严格单调、
PTY 分片序号与终结片、调度触发/结果事件、WS 订阅者收到事件而后连客户端不回放）。
