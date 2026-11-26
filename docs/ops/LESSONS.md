# Soxu 主控工程经验库与流程演进台账 (LESSONS)

> 本文件用于支撑 `AGENTS.md` 第三章第 5 条「自主经验收集与流程自优化闭环」。所有条目须经实战验证并严格脱敏（严禁写入个人用户名路径或运行凭据）。

---

## 一、使用约定与剪枝阈值

1. **派发前 10 秒预检**：主控起草新波次任务书前扫读下方「活跃规则速查表」，将适用条目直接内嵌任务书的「规定环境」「工具边界」或「验收门禁」。
2. **分级晋升规则**：
   - `L1`：单次偶发观察，仅留在 PR 说明，不进入本台账；
   - `L2`：可复用避坑经验，写入本台账「经验明细」并加入速查表；
   - `L3`：高危阻断、跨平台陷阱或命中 >= 2 次的规则，自主晋升写入 `SKILL.md` / `AGENTS.md` 或沉淀为脚本/测试机器防呆 PR。
3. **容量控制（上限 25 条）**：活跃规则速查表长期控制在 **<= 25 条**；已被脚本或测试完全机器防呆的条目应移入「四、已自动化防呆归档区」。

---

## 二、活跃规则速查表（派发前 10 秒必扫）

| ID | 适用环节 | 强制动作 / 禁忌 | 级别 | 当前落点 |
| :--- | :--- | :--- | :--- | :--- |
| `L-001` | 并行构建 | 禁用 `mise run`，须注入 `mise env --json` 再覆写独立 `$env:CARGO_TARGET_DIR` | `L3` | `AGENTS.md` / `SKILL.md` / 本文 |
| `L-002` | WSL/Linux 复验 | 前置真实 Rust/Node `bin` 绕开 `mise` shims 并独立 cache；不在 WSL 内跑 `git` | `L3` | `SKILL.md` / 本文 |
| `L-003` | Pi 子代理选型 | 先用 `pi --list-models` 核实，优先选用高稳定自动路由组 `magpie/auto/deepseek-v4.1-flash-1017` | `L2` | 派发默认预设 / 本文 |
| `L-004` | AOCI stdio MCP | `$env:TEMP` 独立 `.mjs` 首包发 `id: 1` `initialize`；Overview 截断按合同停链走定向检索；不动 `.aoci/config.json` | `L3` | `SKILL.md` / 本文 |
| `L-005` | AOCI 审查署名 | `scope acknowledge --reviewed-by` 统一用 `soxu-agent-<issue>`，严禁个人用户名 | `L2` | `SKILL.md` / 本文 |
| `L-006` | 双平台真机门禁 | 含条件编译/文件系统/工作树/进程语义的变更，合入前必跑 WSL 全仓 Clippy + 目标集成测试 | `L3` | 任务书验收清单 / 本文 |
| `L-007` | 阻塞即时上报 | 遇冻结模块缺陷或两轮排查未果立刻发 `[BLOCKED / NEED-DECISION]`，主控拆单或 `steer` | `L3` | `AGENTS.md` / `SKILL.md` / 本文 |
| `L-008` | 系统盘与缓存治理 | 禁止把大型 target/长留产物堆在系统盘 `%TEMP%`；禁止给 `%TEMP%` 根建 Junction；波次收口巡检残留 | `L3` | 波次微复盘检查表 / 本文 |

---

## 三、经验明细（L2 / L3）

### `L-001` 并行隔离禁止 `mise run`，必须进程内注入环境并覆写独立 `CARGO_TARGET_DIR`
- **场景与根因**：`mise run check-all` 等任务会按 `mise.toml` 重新加载默认 `CARGO_TARGET_DIR = "../.cargo-target/soxu"`，多条 Worktree 并行构建时会在共享 target 上互相抢 Cargo 文件锁或覆盖产物。
- **预防动作**：先用 `mise env --json`（设 `MISE_DISABLE_TOOLS=python,java,maven`）将锁定环境注入当前进程，再显式覆写 `$env:CARGO_TARGET_DIR` 为每路独立目录，随后直接运行 `pwsh -NoProfile -File scripts/mise-doctor.ps1`、`cargo` 或 `scripts/check-all.ps1`。
- **验证方式**：多路并行运行测试无 `Blocking waiting for file lock` 且各自缓存目录独立生成；**状态**：活跃（已固化入 `AGENTS.md` 与 `SKILL.md`）。

### `L-002` WSL Linux 真机构建绕开 `mise` shims 与 Windows `.git` 隔离约定
- **场景与根因**：在 WSL 中调用 `~/.local/share/mise/shims/cargo` 会重读仓库 `mise.toml`，把 `CARGO_TARGET_DIR` 覆盖为指向 Windows `/mnt/...` 的路径，极慢且污染共享缓存；此外宿主机创建的 Worktree `.git` 含 Windows 路径，在 WSL 中直接执行 `git` 会报错。
- **预防动作**：WSL 内运行 `cargo` 前将真实 Rust/Node toolchain `bin` 目录前置到 `PATH`，并显式指定 Linux 本地独立 `CARGO_TARGET_DIR=/root/.cache/<isolated-dir>`；所有 `git`/`gh` 命令留在 Windows 宿主机执行，WSL 仅跑编译与测试。
- **验证方式**：WSL 复验零误写宿主机共享 target，Clippy 与集成测试跑在原生 ext4 缓存目录；**状态**：活跃（已固化入 `SKILL.md`）。

### `L-003` Pi 子代理模型路由选型以 `pi --list-models` 实测为准且优先自动路由组
- **场景与根因**：部分未确认或单点上游模型路由在长上下文或高并发回合可能出现 SSE JSON 畸形、连接重置或突发限额中断，且不得凭记忆拼写不存在的模型名。
- **预防动作**：派发前先以 `pi --list-models` 核对真实 ID，当前优先采用高稳定多成员自动路由组 `magpie/auto/deepseek-v4.1-flash-1017`（思考强度 `high`）；切换模型须在任务边界交接时显式通过 `--model` 重新唤醒或经 `paseo` 启动，不修改全局配置。
- **验证方式**：多路并发展开与长程 AOCI/WSL 调试连续多轮零断流完成；**状态**：活跃。

### `L-004` AOCI stdio MCP 稳定初始化握手与宿主截断降级协议
- **场景与根因**：通过命令行内联 `-e` 脚本或缺少首包 `id: 1` `initialize` 时极易触发引文解析冲突或协议挂起；此外部分宿主插件环境下 `aoci_overview` 单次响应可能被宿主截断。
- **预防动作**：通过 `$env:TEMP` 下独立 UTF-8 `.mjs`（或 Python）脚本连接 `aoci --repo <路径> mcp`，严格发送 `id: 1` `initialize` → `notifications/initialized` → `tools/call` 并在收到完整响应后退出；遇到 `aoci_overview` 被宿主截断时严格按 AOCI 合同中止本条 Overview 链，改走定向 `aoci_search` + 源码阅读核对，严禁修改 `.aoci/config.json`。
- **验证方式**：AOCI 维护脚本可重复成功跑通 `aoci_maintain`、`aoci_update_entry` 及 `aoci verify`/`check`；**状态**：活跃（已固化入 `SKILL.md`）。

### `L-005` AOCI `scope acknowledge` 审查署名脱敏规范
- **场景与根因**：当仅白名单内文档或测试变动触发 `observe_change_policy = review_required` 需做范围确认时，直接填个人用户名或空署名不规范，且有暴露个人身份风险。
- **预防动作**：经过主控预授权的白名单 `observe` 确认，统一执行 `aoci scope acknowledge --reviewed-by "soxu-agent-<issue>" --json`，严禁填入真实个人用户名或擅改 `managed_scope` 策略。
- **验证方式**：`aoci verify --json` 与 `aoci check --json` 通过且全仓隐私扫描零告警；**状态**：活跃（已固化入 `SKILL.md`）。

### `L-006` Windows 单平台 CI 会掩盖 Linux 真机边界与 Clippy 告警
- **场景与根因**：仅通过 Windows CI 无法触发 `#[cfg(target_os = "linux")]` 模块编译、Unix 权限八进制 lint（`non_octal_unix_permissions`）、`blocks_in_conditions` 告警及 Linux 目录句柄工作树真实行为，容易把历史缺陷带到后期集成波次。
- **预防动作**：凡涉及跨平台条件编译、文件系统/权限、目录身份安全边界、Git 工作树生命周期或进程生命周期的变更，任务书与主控复验清单必须强制包含真实 Linux/WSL 的 `cargo clippy --workspace --all-targets --locked -- -D warnings` 与对应集成测试实测。
- **验证方式**：PR 合入前同时留存 Windows 门禁 + 远端 CI + WSL Linux 实测全绿证据；**状态**：活跃。

### `L-007` 子代理遇冻结缺陷或两轮未果须即时上报，由主控拆单热修复
- **场景与根因**：若子代理被禁止碰触的冻结模块缺陷阻断（如跨模块历史 bug），继续试错既易突破 `Owned Paths` 造成多分支冲突，又浪费上下文和算力。
- **预防动作**：严格执行 Fail-Fast 机制：遇冻结模块缺陷或连续两轮只读排查无果立即输出 `[BLOCKED / NEED-DECISION]` 并暂停，由主控视影响决定快速 `steer` 或单开独立热修复 PR 先行治本合入 `main` 后再通知下游分支 `merge origin/main`。
- **验证方式**：多路零互相踩踏，冻结契约边界清晰；**状态**：活跃（已固化入 `AGENTS.md` 与 `SKILL.md`）。

### `L-008` 系统盘 `%TEMP%` 防爆规约与高速非系统盘联接边界
- **场景与根因**：高频多 Worktree 集成测试与工具下载（如 `mise` 临时下载缓存、包管理器缓存及失败保留的测试夹具目录）会在系统盘 `%TEMP%` 迅速堆积数十 GB 残留；若图省事直接把 `%TEMP%` 根目录做成 NTFS Junction，又会导致依赖祖先目录 `ReparsePoint` 检测的安全路径逻辑误判拦截。
- **预防动作**：严禁把大型 `CARGO_TARGET_DIR` 落在系统盘；禁止把 `%TEMP%` 根目录设为 Junction，仅允许将独立的高增长工具纯下载缓存子目录（如 `%TEMP%\mise`、`npm-cache`、`pnpm-cache`）安全联接至独立高速非系统盘 NVMe；波次收口微复盘时顺带巡检系统盘剩余空间与遗留临时测试目录。
- **验证方式**：系统盘 `%TEMP%` 保持普通物理目录无 `ReparsePoint` 标记，系统盘可用空间稳定充足；**状态**：活跃。

---

## 四、已自动化防呆归档区

> 当「活跃规则速查表」中的经验已被仓库脚本（如 `scripts/setup-worktree.ps1`、`scripts/paseo-run-pi.ps1`、`scripts/paseo-dispatch.ps1`、`scripts/verify-commits-and-privacy.ps1`）或自动化测试完全机器防呆且连续两个波次零人工失误后，将其从活跃表迁入本区归档，以保持主控日常上下文精干。

- *暂无已迁出归档项（首批 `L-001` ~ `L-008` 处于活跃巡检期）。*
