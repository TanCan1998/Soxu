# Soxu 原生 CLI 驱动

Daemon 通过 `NativeDriver::discover` 从绝对 PATH 目录查找程序，也可以用
`NativeDriver::with_program` 指定受信任的绝对可执行路径和固定前缀参数。
Windows 仅启动 `.exe`。若 PATH 只提供 `.cmd`、`.bat` 或 `.ps1`，发现过程返回
`DriverError::ScriptShim`。Pi 和 Qwen 可使用 Node EXE 加安装包的绝对 JS 入口。
Copilot 的 npm loader 会再启动一个进程，建议直接使用平台包的 Copilot EXE。

在 Tokio 运行时调用 `start(RunRequest)`。`RunTarget::QuickChat` 接收 Daemon
管理的临时目录，`RunTarget::Directory` 接收已选本地目录。两者都通过子进程
工作目录设置。`Persistence::Ephemeral` 需要原生支持，和 Resume/Fork 互斥。
`RunningTurn::next_event` 依次交付会话 ID、文本、工具状态、遥测及 `Finished`。
文本须按接收顺序拼接。工具参数、工具结果、原始诊断和 stderr 不进入事件流。
调用方应继续读取事件直到 `Finished`，并对运行设定期限。

`capabilities` 描述各适配器实现的能力。它不探测版本或登录状态。
Pi 通过 `TurnControl::send` 发送 `follow_up`、`steer` 或 `abort`，
`ControlAck` 表示 RPC 接受或拒绝该请求，不保证消息已被模型消费。`is_active()`
反映解码器最近确认的 Pi `agent_start` / `agent_settled` 状态，在取消或控制通道关闭后
返回 false。Daemon 应在 `TurnStarted` 后发送控制；发送时仍可能撞上 settled，须核对 ack。
已经写入原生通道但退出前未获得应答的请求不合成拒绝 ack，调用方应报告结果未知。
其余驱动将 Queue 交回 Daemon，Steer 返回不可用。`TurnControl::cancel`、Interrupt 和丢弃
`RunningTurn` 会触发进程回收。Antigravity 回收其钩子进程树，其余驱动回收直接子进程。

`discover_history` 只读扫描显式指定的绝对目录，无需启动 CLI。列表返回原生 ID、
源路径和可取得的工作目录、起始时间与修改时间。`HistoryEntry::resume/fork`
生成原生启动参数。Daemon 应从 `spawn_blocking` 调用扫描，并使用原生会话所属
的工作目录。历史入口以元数据为限，续接时由 CLI 自行载入对话。
`Telemetry` 的缺失值为 `None`。`UsageScope` 标记上下文、模型步骤、工具调用、
轮次或运行汇总，聚合时应避免重复计入。平均和峰值吞吐保留未知。

## Daemon 本机 Pi 只读对话

启动 Daemon 前，将 `SOXU_PI_EXE` 设置为可信 Pi 可执行文件的绝对路径。
使用 Node 安装的 Pi 时，将该变量指向真实 Node 可执行文件，并将 `SOXU_PI_ENTRY`
设置为 Pi 安装包声明的 JS 入口绝对路径。Windows 上应使用安装包的入口，
避免使用会继续派生 Pi 进程的包装器。

在回环监听的 Daemon 上，用 `soxu chat new --agent pi --read-only` 创建快聊，
再用 `soxu chat send` 和 `soxu chat stop` 控制轮次。Pi 使用本机模型设置，
以纯文本模式启动，关闭工具、扩展、MCP、项目配置授权、技能、模板和项目上下文文件。
Pi 原生记录保存在 Daemon 数据目录的 `pi-sessions`，继续发消息时沿用原生会话 ID。
Daemon 使用自己的 FIFO 队列。Pi 确认 active 时，`SubmitTurn.behavior=steer`
发送原生 `steer`，不会再写入 FIFO；尚未 active 或已经 settled 但进程仍收尾时，
沿用明确的 Queue 降级提示。`StopTurn` 仍采用直接进程取消，不伪造 abort 应答。

纠偏消息与当前轮次绑定并持久化，通过 `steer_badge` 和 `TurnMessageUpserted`
事件标记“纠偏待确认”“纠偏已接收”“纠偏被拒绝”或“纠偏结果未知”。
`TurnSubmitted` 无 `queued_id` 只表示已提交控制，不代表原生确认；原生 ack 才更新结果。
5 秒未确认、退出或取消时报告可恢复错误，不自动排队或重发；应核对回复及历史后决定。
同一回合最多等待一条纠偏确认，防止重复点击堆积。停止中不接受新消息，权限、纯文本
及 128 KiB 长度门禁保持不变。FIFO 仍仅驻留内存，重启不会自动重放。

### PiDeck 参考与验证（Issue #32）

参考 `ayuayue/PiDeck` commit `b9a095e063845c80574c5517994c6fcf2be60f25`：

- `src/main/pi/PiRpcClient.ts`：按请求 ID 关联应答，关闭和超时结清 pending；借鉴明确确认边界。
- `src/main/pi/AgentManager.ts` 的 `agent_settled` 与 `abort`：settled 才表示自动后续工作已结束，
  abort 先设本地取消标记，不能把迟到事件重新当作运行；Soxu 保留进程级 Stop 与停止门禁。
- `src/main/pi/promptDispatch.ts`：忙碌投递需显式 `streamingBehavior`，不可因 UI 状态失准丢失投递语义。
  Soxu 使用现有原生 `steer` RPC，而非复制其 prompt 重试路线。
- `src/shared/busySendDelivery.ts`：区分当前回合纠偏与下一轮 follow-up；Soxu 的下一轮仍由自身 FIFO 管理。

不复用 ACP、凭据配置、扩展加载或权限放宽。参考代码只读取，没有复制运行配置。

```powershell
mise exec -- pwsh -NoProfile
$env:CARGO_TARGET_DIR = Join-Path (Get-Location) 'target'
cargo test -p soxu-daemon --test rpc_ws_e2e native_pi
cargo test -p soxu-drivers --test native pi_
$env:SOXU_TEST_PI_ENTRY = '<已安装 Pi 包的绝对 JS 入口>'
cargo test -p soxu-daemon --test rpc_ws_e2e native_pi_real_loopback -- --ignored --nocapture
pwsh -NoProfile -File scripts/check-all.ps1
```

隔离子 shell 启动后执行 `cargo test` 与 `pwsh -NoProfile -File scripts/check-all.ps1`；
不要再次嵌套 `mise exec`，以免 mise.toml 重新覆盖 `CARGO_TARGET_DIR`。
最终门禁使用当前工作树已被忽略的 `target`，不使用多分支共享构建产物。

隔离 Node RPC 夹具覆盖 HTTP/WS、成功/拒绝/超时/断管、settled 后迟到 ack、FIFO、Stop、
SQLite 回读与续接。真实 Pi `1.5.2` + localhost OpenAI 兼容模型夹具实测通过：同一 UUID
续接，纠偏出现在下一次模型请求中且仅记录一次，Queue 等原回合结束后单独执行，
5 次请求均无工具，Stop 后模型连接关闭。真实测试用动态临时 key 和 `PI_CODING_AGENT_DIR`
隔离配置及会话，不修改用户全局配置；该证据不是公网模型推理或其他 Pi 版本验证。

## Daemon 本机 Claude Code 只读对话

启动 Daemon 前，将 `SOXU_CLAUDE_EXE` 指向真实 Claude Code 可执行文件的绝对路径。
Daemon 把 `CLAUDE_CONFIG_DIR` 设置为自身数据目录下的 `claude`，并从进程环境继承
模型服务配置。可通过 Daemon 的启动环境提供 `ANTHROPIC_API_KEY` 和可选的
`ANTHROPIC_BASE_URL`。会话与该运行环境的配置均留在独立目录。

使用 `soxu chat new --agent claude --read-only` 创建快聊，再用 `soxu chat send`
和 `soxu chat stop` 投递或停止。Daemon 使用 Claude Code 的安全和受限模式，
关闭工具、MCP、斜杠命令与项目自定义，续接成功的会话时沿用原生 UUID。

## Daemon 本机 Qwen Code 只读对话

将 `SOXU_QWEN_NODE` 指向真实 Node 可执行文件，并将 `SOXU_QWEN_ENTRY` 指向
Qwen 安装包内处理请求的 `cli.js`，两者均使用绝对路径。Windows 上的 `cli-entry.js`
会派生另一个进程。直接启动 `cli.js` 能让停止操作回收处理当前轮次的进程。
Daemon 将 Qwen 的配置、运行数据与历史隔离到自身数据目录的 `qwen` 子目录。

此接线使用 OpenAI 兼容认证，从 Daemon 启动环境读取 `OPENAI_API_KEY`、
`OPENAI_BASE_URL` 和 `OPENAI_MODEL`。使用 `soxu chat new --agent qwen --read-only`
创建快聊。Daemon 使用 Qwen 的最小与安全模式，按名称排除已实测的内置工具，并将
本轮工具调用预算设为零。若模型仍返回工具调用，回合失败并暂停队列。

## Daemon 本机 Copilot 只读快聊

将 `SOXU_COPILOT_EXE` 指向真实 Copilot 平台可执行文件的绝对路径。Daemon
将 `COPILOT_HOME` 与 `COPILOT_CACHE_HOME` 分别设置为自身数据目录下的
`copilot/state` 与 `copilot/cache`。自定义模型可通过 Daemon 的启动环境设置
`COPILOT_PROVIDER_BASE_URL`、`COPILOT_MODEL` 及相应的 provider 认证参数。
使用 `soxu chat new --agent copilot --read-only` 创建本机快聊。

Daemon 通过 stdin 投递用户文本，关闭内置 MCP、交互提问、项目指令和自动更新，
并使用空工具白名单。Copilot 续接成功后沿用同一原生 UUID。当前只读接线限于快聊，
项目目录会话需要先验证原生项目钩子的隔离行为。

## Daemon 本机 OpenCode 只读快聊

将 `SOXU_OPENCODE_EXE` 指向已验证的 OpenCode `1.19.6` 平台可执行文件。
在 Daemon 启动环境的 `OPENCODE_CONFIG_CONTENT` 中提供可信的 `model` 和
`provider` 配置。Daemon 只采用这两项顶层配置，固定全拒绝工具权限，并隔离
OpenCode 配置、数据库、缓存与状态到 Daemon 的 `opencode` 子目录。

使用 `soxu chat new --agent opencode --read-only` 创建快聊。Daemon 以
`--pure run --format json` 启动真实进程并通过 stdin 投递文本，禁用项目配置、
外部插件和自动更新。轮次成功后保存 `ses_` 原生 ID，后续使用 `--session`
续接。当前开放本机快聊。

## 逐驱动证据

下列本机版本是本次核对的版本。常规隔离测试验证七种事件格式。实际登录推理已
验证 Codex 两种目标。真实 Codex 接回环 Responses 夹具额外验证了临时会话、
本地历史发现、Resume 保留 ID 以及 Fork 分配新 ID。

| CLI | 流式接口与可采集用量 | 历史与本次验证范围 |
| --- | --- | --- |
| Codex 0.172.0 | `exec --json`；`thread.started`、`item.completed`、`turn.completed`；轮次、输入输出和缓存 token | `rollout-*.jsonl`，`exec resume/fork`；已登录烟测与回环续接通过 |
| Pi 1.5.2 | `--mode rpc`；`message_update`、`agent_settled`；模型步骤、token、成本、上下文 | Session JSONL，`--session` / `--fork`；真实 Pi 经 Daemon 接回环模型夹具，通过两轮流式续接并沿用同一会话 ID |
| Claude Code 2.1.331 | `--print --output-format stream-json --verbose`；部分文本、`result`、报告的轮次、token 与美元成本 | `projects/<cwd>/<id>.jsonl`，`--resume` / `--fork-session`；真实 Claude Code 经 Daemon 接本机模型夹具，两轮回复使用同一 UUID，记录落在隔离目录 |
| OpenCode 1.19.6 | `run --format json`、stdin；`text`、`tool_use`、`step_finish`；步骤、token、缓存、成本和工具执行时长 | `--session` / `--fork`；真实 OpenCode 经 Daemon 验证两轮回环、零工具、同一原生 ID 和停止后模型连接断开。SQLite 历史扫描不可用 |
| Antigravity 1.5.18 | `--print --output-format stream-json`；`init`、`step_update`、`result`；步骤、时长、轮次与 token | Daemon 仅开放隔离只读快聊。真实回环覆盖 `--conversation` 同 ID 续接、残留工具拒绝和取消。历史扫描和 Fork 不可用 |
| Copilot 1.0.114 | `--output-format json --stream on`、stdin；助手增量、最终 `result`、有报告的用量、上下文和延迟 | `session-state/<id>/events.jsonl`，`--resume=<id>`；真实 Copilot 经 Daemon 通过两轮回环续接，沿用同一 UUID，且模型请求均无工具。Fork 不可用 |
| Qwen Code 0.28.0 | `--output-format stream-json --include-partial-messages`；助手增量与 `result` 用量 | 项目 `chats/<id>.jsonl`，`--resume` / `--fork-session`；真实 Qwen 经 Daemon 通过两轮回环续接，模型请求不带工具，额外的工具请求被拦截 |

## 验证命令

```powershell
cargo fmt -p soxu-drivers -- --check
cargo test -p soxu-drivers --locked
cargo clippy -p soxu-drivers --all-targets --locked -- -D warnings
cargo test -p soxu-drivers --test native real_codex_quick_chat_directory_resume_and_fork -- --ignored --nocapture
cargo test -p soxu-drivers --test native signed_in_codex_streams_both_targets -- --ignored
```

进程夹具使用 Node，可通过 `SOXU_TEST_NODE` 指定 Node 的绝对路径。
回环测试可通过 `SOXU_TEST_CODEX` 明确指定 Codex EXE。它使用临时 `CODEX_HOME`
与 loopback 服务，分别记录四次请求的阶段，不打印请求体或原生诊断。
逐字节输入测试独立覆盖引号、Unicode、换行和 shell 元字符。已登录烟测继承
现有 Codex 环境和默认模型，以临时工作目录和只读、临时会话模式运行。

## Antigravity 隔离只读快聊

Daemon 接受 `agy` 和 `antigravity` 两个标识，启动范围为未归档的本机 QuickChat + ReadOnly。
宿主通过 `SOXU_AGY_EXE` 指定可信原生程序的绝对路径。Windows 使用真实 EXE，拒绝脚本与
已知 Scoop shim。钩子使用 PATH 中可信的原生 Node，也可通过 `SOXU_AGY_NODE_EXE` 固定入口。
Gemini API 密钥可由宿主显式通过 `SOXU_AGY_GEMINI_API_KEY` 注入。AGY 子进程使用 Daemon
私有 home、配置及临时目录，继承环境清空，系统变量按平台白名单设置。

### 真实 CLI 边界探针

真实 Windows Antigravity `1.5.17` 的隔离回环验证已覆盖两轮流式、同一 UUID、
续接携带前轮模型回复，以及直接进程停止后模型连接断开。有效 `write_file`
调用被全局 `write_file(*)` deny 规则拒绝，目标文件未产生；回合仍以成功状态结束，
所以成功终态不能证明工具已执行或整轮未触发工具。

驱动移除与 `--disable-slash-commands` 同时使用时无效的 `--mode plan`，改用
会话级 `--settings` 固定严格权限、关闭非工作区访问，并拒绝文件、命令、网页和
MCP 的权限命名空间。仅应用这组设置时，模型请求仍声明 15 种工具，其中包括调度、
工作流及子代理。Daemon 另外固定空工具代理和全匹配 `PreToolUse` 钩子。

可复跑探针仅复制可信 EXE 到临时目录，生成独立用户目录和随机本机模型夹具凭据，
不读取或写入用户真实配置，不安装软件，并保留脱敏的 JSON 证据：

```powershell
$env:SOXU_TEST_AGY = '<可信 Antigravity EXE 的绝对路径>'
mise exec -- node crates/soxu-drivers/tests/fixtures/antigravity-boundary.cjs
mise exec -- cargo test -p soxu-daemon --test rpc_ws_e2e antigravity_real_quick_chat_http_ws_loopback -- --ignored
```

权限语义依据上游 [CLI Headless](https://antigravity.google/docs/cli/headless/)、
[Settings](https://antigravity.google/docs/settings/) 与
[Permissions](https://antigravity.google/docs/permissions/)；实际生效结论以实机探针为准。

### 空工具代理与钩子实测

在隔离全局目录创建 `tools: []` 自定义代理并通过 `--agent` 选择后，真实 `1.5.17`
的模型工具声明从 15 项降到仅 `manage_task`。模型额外返回有效 `write_file` 调用时，
工具注册层以 `unknown tool` 拒绝，而非依靠参数校验或文件权限；过滤代理两轮续接
仍沿用同一 UUID 并带回前轮模型回复。`init.tools` 仍列出完整内置目录，不能用它
替代真实模型请求的工具列表。

隔离全局 `hooks.json` 的全匹配 `PreToolUse` 钩子能拒绝残留的 `manage_task`。
项目级同名禁用钩子与另一个自动批准钩子未削弱该拒绝。但项目级同名自定义代理
可以重新声明写入工具；即使关闭斜杠展开，项目 `SessionStart` 钩子仍真实执行。
全局工具拒绝不是项目启动钩子隔离，也不是操作系统沙箱。

边界探针覆盖工具过滤、续接、未声明写入、全局钩子拒绝及项目覆盖。另在 Windows
使用真实 Antigravity `1.5.18` 经 Daemon HTTP/WS 回环验证同 UUID 两轮续聊、`manage_task`
返回钩子拒绝和等待模型期间停止。父目录夹具中的项目钩子未加载，快聊工作目录中的项目
自定义资产会使生产路由拒绝启动。

### 钩子故障与资产完整性

同一隔离探针还覆盖钩子脚本非零退出、非法 JSON、超时与脚本缺失：这四类运行
故障均使残留工具返回 `TOOL_ERROR`，没有回退为工具成功。但全局钩子配置为空、
配置文件缺失或被替换为自动批准配置时，`manage_task` 的合法 `list` 调用实际
执行成功。测试只列举当前隔离进程的后台任务，不停止或控制用户任务。

Daemon 在首次、续聊和 FIFO 启动前校验固定代理、钩子脚本、钩子配置及其他配置资产。
缺失、篡改、链接和意外配置文件均拒绝启动。

Daemon 库提供独立的 `NativeAssetGuard` 校验底座：调用方提供可信内容，校验器不从
磁盘学习并接受当前内容；每次校验都复核大小、SHA-256、普通文件与祖先目录，并
拒绝符号链接及 Windows reparse point。启动目录必须位于声明根内，根到工作目录
之间出现 `.agents` 即拒绝。先拒绝链接，再规范化根和工作目录，以兼容 Windows
绝对路径的普通及 verbatim 表示。Windows 使用私有目录 ACL，并以仅共享读取的方式持有
已校验资产，直到回收原生进程。Unix 校验所有者和私有权限，从打开的文件句柄计算摘要。
目录身份和原生程序由宿主维护。同权限进程、管理员及上游对路径的重新打开仍受操作系统
权限与上游实现约束。应用层检查无法消除所有路径竞态。

宿主在 `with_native_driver_set` 后可用 `with_native_asset_guard(kind, guard)` 将声明
绑定已配置的可信驱动。绑定时校验一次，实际 `start_native` 在调用 `driver.start`
前再次校验，覆盖首轮、续接、停止后的下一轮和自动 FIFO 出队；拒绝时不生成新轮次
消息、不消费排队条目。HTTP/WS 测试以 Node 夹具验证两轮启动、等长篡改、缺失与
项目覆盖拒绝，以及出队前篡改、恢复后原队列继续。Antigravity 路由要求绑定由私有配置
生成的守卫。Windows 夹具还验证了回合中资产可读且拒绝外部写入和删除。

### 取消与平台边界

Windows 在回收直接子进程前通过系统目录中的 `taskkill /T /F` 终止 AGY 及其钩子子进程。
Unix 启动时创建独立进程组，通过系统 `kill` 向整组发信号并等待组首退出。Windows 夹具
分别验证显式取消和丢弃回合后父子 PID 退出。Windows 的进程树快照存在父进程提前退出或
后代同时派生的竞态，Unix 主动脱离进程组的后代超出回收范围。系统回收命令失败或超时会
回退终止直接子进程，并报告 I/O 失败。
