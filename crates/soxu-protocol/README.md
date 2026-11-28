# soxu-protocol

Soxu 跨端（GPUI 桌面端、移动端 PWA 与 CLI）统一版本化通信协议。

- HTTP/WebSocket RPC 信封：`RpcRequest` / `RpcResponse` / `RpcCommand` / `RpcResultData`；
- 实时事件流：`EventEnvelope<DaemonEvent>`，单调 `seq` 支持熄屏/断网后按 `after_seq` 追平；
- 领域契约：会话与轮次、审批与提问、配对与偏好、M3 Git 操作（`git_ops`）、
  以及 M6 前置三组契约（`scripts` / `schedules` / `pty`）；
- `PROTOCOL_VERSION` 为握手主版本，客户端与守护进程启动时必须核对。

## 命名与校验约定

- 结构体字段统一 `snake_case`，枚举变体统一 `snake_case`；
- 信封式枚举使用相邻标签：命令 `{"method": …, "params": …}`，结果 `{"kind": …, "data": …}`，
  事件 `{"event": …, "data": …}`；
- **未知字段一律容忍**（不使用 `deny_unknown_fields`），后续版本新增字段不得让旧端解析失败；
- 安全相关标量（幂等键、环境变量名、工作区相对路径、尺寸、分片长度、cron 表达式、
  重试上限、有效期窗口）在**反序列化阶段**校验，非法负载直接拒绝，不做宽松兜底。

## M6 前置三组契约

本目录只交付**协议类型、域命令/结果/事件枚举、能力位与协议测试**；
脚本执行、调度定时器、PTY 派生与帧编解码由后续 daemon 实现 PR 落地。

### 受管脚本（`scripts.rs`）

| 类型 | 作用 |
| :--- | :--- |
| `ManagedScriptEntry` | `paseo.json` 单条脚本声明：标识、展示名、风险等级、服务型标记、一次性超时 |
| `ScriptProgram` | 受控程序字段：拒绝空白/控制字符/shell 元字符与 `sh`、`pwsh`、`cmd` 等解释器 |
| `ScriptArgument` / `ScriptFixedArguments` / `ScriptUserArguments` | 固定参数（≤32）与透传参数（≤16），单条 ≤4096 字节且禁止 NUL 与换行 |
| `ScriptArgumentPolicy` | 固定参数 + 是否接受透传 + 透传条数/字节上限（不接受时上限必须为 0） |
| `ScriptWorkdirPolicy` | `workspace_root` 或 `subdirectory{relative_path}`（拒绝绝对路径与 `..`） |
| `ScriptEnvPolicy` | 环境变量**名**白名单（≤64、去重）+ 是否继承父进程环境（默认否） |
| `ScriptRiskLevel` | `low` / `medium` / `high` / `critical`；`high` 及以上必须逐次审批 |
| `ListScriptsRequest` / `RunScriptRequest` / `StopScriptRequest` / `InspectScriptRequest` | 清单、启动、停止、状态查询请求 |
| `ScriptManifestListing` / `ScriptRunSnapshot` | 清单结果与运行快照（状态、退出码、起止时间、幂等键） |
| `ScriptLifecycleEvent` | 生命周期相位 `queued` / `started` / `exited` / `failed` / `stopped` + 完整运行快照 |
| `ScriptCommand` / `ScriptCommandResult` | 脚本域命令与结果枚举 |

**注入面红线**：协议不提供裸 shell 字符串字段，参数只按数组直传，任何消费方都不得把
`program` 与参数重新拼接成命令行。

### Schedules & Heartbeats（`schedules.rs`）

| 类型 | 作用 |
| :--- | :--- |
| `ScheduleCadence` | `cron{expression,timezone}` / `interval{every_seconds}` / `heartbeat{every_seconds,only_when_idle}` |
| `CronExpression` | 标准 5 段式，只允许数字与 `* , - /`；语义求值留给调度器 |
| `ScheduleTimezone` | `UTC` 或 `Area/City` |
| `ScheduleValidityWindow` | 可选起止时刻，两者同时存在时必须单调（终点晚于起点），提供 `is_active_at` |
| `ScheduleRetryPolicy` | `max_attempts` 1..=10（含首次执行）与 `backoff_seconds` ≤3600 |
| `ScheduleTrigger` | 触发正文（≤16384 字节）、`agent_id`、可选 `model_id` 与权限档 |
| `CreateScheduleRequest` / `UpdateScheduleRequest` / `PauseScheduleRequest` / `DeleteScheduleRequest` / `ListSchedulesRequest` / `TriggerScheduleRequest` | 创建、更新、停用/恢复、删除、列出、立即触发 |
| `ScheduleSummary` / `ScheduleListResult` | 调度摘要（含 `next_run_at` / `last_run_at` / 状态） |
| `ScheduleTriggeredEvent` / `ScheduleRunResult` | 触发事件（含 `scheduled_for` 与 `attempt`）与运行结果（成功/失败/跳过/超时） |
| `ScheduleCommand` / `ScheduleCommandResult` / `ScheduleEvent` | 调度域命令、结果与事件枚举 |

周期硬边界：固定周期 60..=2592000 秒，心跳 30..=2592000 秒；
幂等键复用 `IdempotencyKey`，同一键重复下发必须收敛到同一结果。

### 受控 PTY（`pty.rs`）

| 类型 | 作用 |
| :--- | :--- |
| `PtySize` | 列 1..=1000、行 1..=500 |
| `PtyWorkdirPolicy` | `workspace_root` 或工作区内子目录 |
| `PtyEnvPolicy` | 环境变量**名**白名单（≤64、去重）+ 是否继承父进程环境（默认否） |
| `OpenPtyRequest` / `WritePtyRequest` / `ResizePtyRequest` / `ClosePtyRequest` | 打开、输入、调整尺寸、关闭请求 |
| `OpenPtyResponse` / `WritePtyResponse` / `ResizePtyResponse` / `ClosePtyResponse` | 对应响应 |
| `PtySessionSnapshot` / `PtySessionState` / `PtyExitStatus` | 会话快照与退出信息 |
| `PtyOutputData` / `PtyInputData` | 输出分片 ≤16384 字节、单次输入 1..=8192 字节 |
| `PtyOutputChunk` / `PtyEvent` | 有界输出分片（`sequence` 从 0 起严格递增 + `end_of_stream`）与事件枚举 |

`PtyOutputChunk::split_output` 提供 UTF-8 安全的切片实现：每片不超过 16 KiB、
序号连续、最后一片标记 `end_of_stream`，重组后与原始输出逐字节一致；
禁止把无界输出塞进单个事件。

### 能力协商（`capabilities.rs`）

- `DaemonFeature`：`managed_scripts` / `schedules` / `heartbeats` / `controlled_pty`，
  令牌跨版本稳定；
- `DaemonFeatureSet`：以 `declared: Vec<String>` 保存**原始令牌**，缺省为空即全部关闭；
- 未知令牌既不报错也不丢弃（`unknown_tokens()` 可诊断），旧客户端遇到新能力只忽略；
- 服务端只有在客户端声明对应能力后，才可下发
  `RpcResultData::{ManagedScript, Schedule, ControlledPty}` 与
  `DaemonEvent::{ScriptLifecycleChanged, ScheduleActivity, PtyActivity}`。

## 兼容策略（M6 前置）

- `PROTOCOL_VERSION` 保持 `1`：三组契约全部通过**新增模块、新增枚举变体与新类型**接入，
  不修改任何既有字段与既有变体语义；
- 全局 `RpcCommand` 未新增变体：`soxu-daemon` 的 `dispatch_command` 对 `RpcCommand`
  是穷尽匹配，本单冻结 `soxu-daemon`，新增变体会直接打断工作区编译。
  因此三组契约先以 `ScriptCommand` / `ScheduleCommand` / `PtyCommand` 域命令枚举独立成型，
  由后续实现 PR 在 `RpcCommand` 中 1:1 转发 `ManagedScript(_)` / `Schedule(_)` / `ControlledPty(_)`
  并补齐守护进程分支；
- 握手结构体（`ClientHello` / `ServerHello` / `StateSnapshot`）同样未新增字段，理由相同；
  能力位在握手入口的接线随实现 PR 一并落地。

## 测试

```powershell
cargo test -p soxu-protocol --locked
cargo clippy -p soxu-protocol --all-targets --locked -- -D warnings
```

- `tests/m6_contracts.rs`：三组契约的 serde 往返、未知字段容错、非法参数拒绝、
  分片边界与序号、能力位缺省关闭、增量扩展不改主版本；
- `tests/git_ops_wire.rs`：M3 Git 契约线缆形状回归；
- `src/**` 内联单测覆盖标量校验（路径越级、环境变量名、幂等键）与分片切分/尺寸边界。
