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
`ControlAck` 表示 RPC 接受或拒绝该请求。Daemon 应在 `TurnStarted` 后发送控制。
其余驱动将 Queue 交回 Daemon，Steer 返回不可用，Interrupt 请求终止所启动的
直接子进程。`TurnControl::cancel` 对所有驱动可用。丢弃 `RunningTurn` 也会触发
进程回收。进程级取消的范围是直接子进程。

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
Daemon 使用自己的 FIFO 队列，运行中的纠偏请求会降级为排队。

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

## 逐驱动证据

下列本机版本是本次核对的版本。常规隔离测试验证七种事件格式。实际登录推理已
验证 Codex 两种目标。真实 Codex 接回环 Responses 夹具额外验证了临时会话、
本地历史发现、Resume 保留 ID 以及 Fork 分配新 ID。

| CLI | 流式接口与可采集用量 | 历史与本次验证范围 |
| --- | --- | --- |
| Codex 0.172.0 | `exec --json`；`thread.started`、`item.completed`、`turn.completed`；轮次、输入输出和缓存 token | `rollout-*.jsonl`，`exec resume/fork`；已登录烟测与回环续接通过 |
| Pi 1.5.2 | `--mode rpc`；`message_update`、`agent_settled`；模型步骤、token、成本、上下文 | Session JSONL，`--session` / `--fork`；真实 Pi 经 Daemon 接回环模型夹具，通过两轮流式续接并沿用同一会话 ID |
| Claude Code 2.1.331 | `--print --output-format stream-json --verbose`；部分文本、`result`、报告的轮次、token 与美元成本 | `projects/<cwd>/<id>.jsonl`，`--resume` / `--fork-session`；真实 Claude Code 经 Daemon 接本机模型夹具，两轮回复使用同一 UUID，记录落在隔离目录 |
| OpenCode | `run --standalone --format json`；`text`、`tool_use`、`step_finish`；步骤、token、缓存、成本和工具执行时长 | `--session` / `--fork`；上游 `packages/cli/src/run/{run,noninteractive}.ts` 与夹具。本机不在 PATH，SQLite 历史扫描不可用 |
| Antigravity 1.5.16 | `--print --output-format stream-json`；`init`、`step_update`、`result`；步骤、时长、轮次与 token | 已知会话 ID 可使用 `--conversation`；本机帮助、Google Antigravity *Headless mode* 与夹具。历史扫描和 Fork 不可用 |
| Copilot 1.0.114 | `--output-format json --stream on`；助手增量、轮次结束与有报告的用量、上下文和延迟 | `session-state/<id>/events.jsonl`，`--resume=<id>`；本机帮助、GitHub *Streaming session events*、配置目录参考与夹具。Fork 不可用 |
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
