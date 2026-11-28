# soxu-daemon

Soxu 核心守护进程：持久化状态机、WebSocket 游标事件流、移动配对鉴权与移动端 PWA 静态资产伺服。

## 运行

```powershell
cargo run -p soxu-daemon
```

- 默认监听 `127.0.0.1:7698`（`SOXU_LISTEN` 覆写）。
- 数据目录默认 `~/.soxu`（`SOXU_DATA_DIR` 覆写）；目录级文件锁防止双实例。
- 非回环监听地址自动要求全部请求携带设备 Bearer 令牌（`require_auth_for_all`）。

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

## 能力门控（M6-A）

`DaemonFeature::ManagedScripts` / `ControlledPty` **缺省关闭**。门控统一由
`scripts::FeatureGate` 判定：未声明时脚本域与 PTY 域整族命令一律返回
`ErrorCode::Unsupported` 的三要素错误，且不下发任何 `ScriptLifecycleChanged` / `PtyActivity` 事件。
`FeatureGate::from_csv("managed_scripts,controlled_pty")` 供宿主按配置装配。

> 传输层现状：`soxu-protocol` 的 `RpcCommand` 尚无脚本/PTY 请求变体（契约 PR #137 只交付了结果与事件变体，
> 见 `docs/ops/PROGRESS.md` §4.14 衔接红线）。本单交付的是执行器与宿主的完整实现、门控与命令处理入口
> （`ScriptService::handle` / `PtyHost::handle`），1:1 对应 `ScriptCommand` / `PtyCommand`；
> `RpcCommand` 转发与 `ClientHello` 能力字段需独立契约 PR 补齐后直接接线，本单未改动冻结的 `soxu-protocol`。

## 测试

```powershell
cargo test -p soxu-daemon
```

覆盖 HTTP/WS 端到端、`afterSeq` 增量追平、配对全生命周期、原生驱动只读会话、PWA 伺服 404/200/鉴权回归，
以及 M6-A 专项 `tests/scripts_pty.rs`（清单注入面、透传参数越权、高风险审批、输出预算截断、
超时后进程树有界回收、环境白名单隔离、幂等键去重、PTY 分片序号与终结片、关闭幂等、
会话上限与空闲回收、真实 ConPTY/openpty 冒烟）。
