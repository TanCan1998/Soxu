# Soxu 主控工程经验库与流程演进台账 (LESSONS)

> 本文件支撑 `AGENTS.md` 第三章第 5 条「自主经验收集与流程自优化闭环」。所有条目须经实战验证并严格脱敏（严禁写入个人用户名路径或运行凭据）。对照规则参考 `docs/ops/CODE_STANDARDS.md`。

---

## 一、使用约定与晋升剪枝规则

1. **派发前 10 秒预检**：主控起草新波次任务书前扫读下方「活跃规则速查表」，将适用条目直接内嵌任务书的「规定环境」「工具边界」或「验收门禁」。
2. **复现计数与三级晋升漏斗（对标 Magpie 实践）**：
   - `L1`：单次偶发观察，仅留在 PR 说明，不进入本台账；
   - `L2`：可复用避坑经验，写入本台账并标注 `Seen N× (YYYY-MM-DD)`；
   - `L3`：高危阻断、跨平台陷阱或 **在不同日期/波次复现 `Seen >= 3×`**：主控自主晋升写入 `docs/ops/CODE_STANDARDS.md`、`SKILL.md` / `AGENTS.md` 或防呆脚本/测试 PR。
3. **容量控制（上限 25 条）**：活跃规则速查表长期控制在 **<= 25 条**；已被脚本或测试完全机器防呆且连续两波无复发的条目移入「四、已自动化防呆归档区」。

---

## 二、活跃规则速查表（派发前 10 秒必扫）

| ID | 适用环节 | 强制动作 / 禁忌 | 级别 / 计数 | 当前落点 |
| :--- | :--- | :--- | :--- | :--- |
| `L-001` | 并行构建 | 禁用 `mise run`，须注入 `mise env --json`（`MISE_DISABLE_TOOLS=java,maven`）再覆写独立 `$env:CARGO_TARGET_DIR` | `L3 · Seen 3×` | `AGENTS.md` / `SKILL.md` / `CODE_STANDARDS.md` |
| `L-002` | WSL/Linux 复验 | 前置真实 Rust/Node `bin` 绕开 `mise` shims 并独立 cache；不在 WSL 内跑 `git` | `L3 · Seen 2×` | `SKILL.md` / 本文 |
| `L-003` | Pi 子代理选型 | 先用 `pi --list-models` 核实，优先高稳定自动路由组 `magpie/auto/deepseek-v4.1-flash-1017` | `L2 · Seen 2×` | 派发默认预设 / 本文 |
| `L-004` | AOCI stdio MCP | `$env:TEMP` 独立 `.mjs` 首包发 `id: 1` `initialize`；Overview 截断按合同停链走定向检索；不动 `.aoci/config.json` | `L3 · Seen 3×` | `SKILL.md` / 本文 |
| `L-005` | AOCI 审查署名 | `scope acknowledge --reviewed-by` 统一用 `soxu-agent-<issue>`，严禁个人用户名 | `L2 · Seen 2×` | `SKILL.md` / 本文 |
| `L-006` | 双平台真机门禁 | 含条件编译/文件系统/工作树/进程语义的变更，合入前必跑 WSL 全仓 Clippy + 目标集成测试 | `L3 · Seen 2×` | `CODE_STANDARDS.md` / 本文 |
| `L-007` | 阻塞即时上报 | 遇冻结模块缺陷或两轮排查未果立刻发 `[BLOCKED / NEED-DECISION]`，主控拆单或 `steer` | `L3 · Seen 3×` | `AGENTS.md` / `SKILL.md` / `CODE_STANDARDS.md` |
| `L-008` | 系统盘与缓存治理 | 禁止把大型 target/长留产物堆在系统盘 `%TEMP%`；禁止给 `%TEMP%` 根建 Junction；波次收口巡检残留 | `L3 · Seen 1×` | 波次微复盘检查表 / 本文 |
| `L-009` | `.ps1` 测试退出码 | PowerShell 测试脚本调完负向外部子进程后末尾必写 `exit 0`，防 CI `shell: pwsh` 继承非零 `$LASTEXITCODE` | `L3 · Seen 1×` | `test-*.ps1` / 本文 |
| `L-010` | PR 关联与防竞态合入 | PR 正文裸写 `Closes #<num>` 不套反引号；合入强制带 `--match-head-commit <reviewed_sha>` | `L3 · Seen 1×` | `CODE_STANDARDS.md` / `SKILL.md` / 本文 |
| `L-011` | `.ps1` UTF-8 防乱码 | 中文 Windows 无终端宿主默认 CP936；读写 `git` 输出的门禁脚本开头必设 `Console/OutputEncoding = UTF8` | `L3 · Seen 1×` | `check-all.ps1` / `verify-commits-and-privacy.ps1` |
| `L-012` | Pi 上下文与缓存 | 项目级 `.pi/settings.json` 必设 `autoload: false` + `["!**"]` 禁 `context-mode` 防 `.pi/npm/` 漂移，且 `trust.json` 覆 worktrees | `L3 · Seen 1×` | `.pi/settings.json` / `.gitignore` / `SKILL.md` |

---

## 三、经验明细（L2 / L3）

### `L-001` 并行隔离禁止 `mise run`，必须进程内注入环境并覆写独立 `CARGO_TARGET_DIR`
- **场景与根因**（`Seen 3× (2026-11-25, 2026-11-26)`）：`mise run check-all` 会按 `mise.toml` 重新加载默认 `CARGO_TARGET_DIR = "../.cargo-target/soxu"`，多条 Worktree 并行时抢占同一 Cargo 文件锁。
- **预防动作**：先注入 `mise env --json`（`MISE_DISABLE_TOOLS=java,maven`、`PYTHONUTF8=1`）到当前进程，再显式覆写独立 `$env:CARGO_TARGET_DIR`，随后直接执行 `mise-doctor.ps1`、`cargo` 或 `check-all.ps1`。
- **验证方式**：多路并行无 `Blocking waiting for file lock` 且产物落入各自缓存目录；**状态**：活跃（已晋升入 `CODE_STANDARDS.md`）。

### `L-002` WSL Linux 真机构建绕开 `mise` shims 与 Windows `.git` 隔离约定
- **场景与根因**（`Seen 2× (2026-11-26)`）：在 WSL 调用 `mise` shims 会覆盖 `CARGO_TARGET_DIR` 指向 `/mnt/...` 导致慢且污染共享缓存；宿主 Worktree `.git` 含 Windows 路径导致 WSL `git` 报错。
- **预防动作**：WSL 内运行 `cargo` 前将真实 Rust/Node toolchain `bin` 目录前置到 `PATH` 并设 Linux 本地 `CARGO_TARGET_DIR=/root/.cache/<isolated-dir>`；所有 `git`/`gh` 留在本机执行。
- **验证方式**：WSL 复验零污染宿主机共享 target；**状态**：活跃。

### `L-003` Pi 子代理模型路由选型以 `pi --list-models` 实测为准且优先自动路由组
- **场景与根因**（`Seen 2× (2026-11-26)`）：单点上游长上下文可能发生 SSE JSON 畸形或连接中断，且不得凭记忆拼写模型名。
- **预防动作**：派发前先以 `pi --list-models` 核对真实 ID，当前默认采用 `magpie/auto/deepseek-v4.1-flash-1017`（思考强度 `high`）。
- **验证方式**：长程 AOCI 与双平台构建连续稳定完成；**状态**：活跃。

### `L-004` AOCI stdio MCP 稳定初始化握手与宿主截断降级协议
- **场景与根因**（`Seen 3× (2026-11-26)`）：内联 `node -e` 易引文冲突且缺少首包 `id: 1` `initialize` 会挂起；宿主插件下 `aoci_overview` 可能被截断。
- **预防动作**：`$env:TEMP` 独立 `.mjs`/Python 脚本严格发 `id: 1` `initialize` → `notifications/initialized` → `tools/call`；Overview 截断时按合同中止并改走定向 `aoci_search` + 源码核对，严禁改 `.aoci/config.json`。
- **验证方式**：`aoci_maintain`、`aoci_update_entry` 与 `aoci verify`/`check` 可重复通过；**状态**：活跃。

### `L-005` AOCI `scope acknowledge` 审查署名脱敏规范
- **场景与根因**（`Seen 2× (2026-11-26)`）：白名单文档/测试触发 `observe` 确认时填写个人用户名既不合规又易泄露隐私。
- **预防动作**：经授权的白名单 `observe` 确认统一使用 `aoci scope acknowledge --reviewed-by "soxu-agent-<issue>" --json`。
- **验证方式**：`aoci verify`/`check` 通过且全仓隐私扫描零告警；**状态**：活跃。

### `L-006` Windows 单平台 CI 会掩盖 Linux 真机边界与 Clippy 告警
- **场景与根因**（`Seen 2× (2026-11-26)`）：Windows CI 不编译 `#[cfg(target_os = "linux")]` 与 Linux Clippy（如 `blocks_in_conditions`、`non_octal_unix_permissions`），容易把历史缺陷带到集成期。
- **预防动作**：涉平台条件编译、文件系统、目录身份与 Git 工作树语义的改动，合入前须在 WSL 实测全仓 Clippy 与目标集成测试。
- **验证方式**：同时留存 Windows 门禁 + 远端 CI + WSL 实测通过证据；**状态**：活跃（已关联 `CODE_STANDARDS.md`）。

### `L-007` 子代理遇冻结缺陷或两轮未果须即时上报，由主控拆单热修复
- **场景与根因**（`Seen 3× (2026-11-25, 2026-11-26)`）：在受限 `Owned Paths` 内围绕冻结 crate 历史缺陷盲目重试既越界又浪费上下文。
- **预防动作**：严格执行 Fail-Fast：遇冻结缺陷或两轮排查未果立刻发 `[BLOCKED / NEED-DECISION]` 并暂停，由主控拆独立热修复 PR 先行治本。
- **验证方式**：多路并行零互踩且热修复清晰解耦；**状态**：活跃（已晋升入 `CODE_STANDARDS.md`）。

### `L-008` 系统盘 `%TEMP%` 防爆规约与高速非系统盘联接边界
- **场景与根因**（`Seen 1× (2026-11-27)`）：多路测试夹具与工具下载会在系统盘 `%TEMP%` 堆积数十 GB；直接把 `%TEMP%` 根目录设为 Junction 会触发祖先 `ReparsePoint` 安全拦截。
- **预防动作**：禁止把大型 `CARGO_TARGET_DIR` 放系统盘；禁止把 `%TEMP%` 根目录建为 Junction，仅可将高增长纯缓存子目录（如 `%TEMP%\mise`、`npm-cache`、`pnpm-cache`）联接至独立高速 NVMe 盘；波次收口巡检系统盘空间。
- **验证方式**：`%TEMP%` 根目录无 `ReparsePoint` 且系统盘可用空间充足；**状态**：活跃。

### `L-009` PowerShell 回归脚本末尾必写显式 `exit 0` 防 CI 继承负向退出码
- **场景与根因**（`Seen 1× (2026-11-27)`）：在 `test-paseo-orchestration.ps1` 中调用负向测试用例会留下 `$LASTEXITCODE = 1`，随后末尾 `Write-Host` 不清零该状态；在 GitHub Actions `shell: pwsh`（以 `-Command ". './script.ps1'"` 执行）下会被直接判定步骤失败。
- **预防动作**：凡调用外部子进程的 `.ps1` 检查或回归脚本，在确认所有断言通过后末尾必须显式写 `exit 0`。
- **验证方式**：使用 `pwsh -NoProfile -Command ". './scripts/<name>.ps1'"` 在本地与 `ubuntu-latest` CI 均返回退出码 `0`；**状态**：活跃。

### `L-010` PR 自动关单格式与合入锁死已审 SHA 纪律
- **场景与根因**（`Seen 1× (2026-11-27)`）：PR 正文中写成 `` `Closes #74` ``（带反引号）时 GitHub 不识别自动关单；审查与合入之间若有新提交推送可能发生未审先合。
- **预防动作**：PR 正文始终裸写 `Closes #<issue>`；主控合并一律使用 `gh pr merge <pr> --squash --delete-branch --match-head-commit <reviewed_sha>`。
- **验证方式**：合入后对应 Issue 自动变为 `CLOSED` 且合入 SHA 与已审 SHA 完全一致；**状态**：活跃（已晋升入 `CODE_STANDARDS.md`）。

### `L-011` Windows CP936 无终端下 `.ps1` 读取 Git 输出防乱码
- **场景与根因**（`Seen 1× (2026-11-27)`）：无 TTY 宿主在中文 Windows 下默认代码页为 CP936，直接从管道读 `git log` / `mise env --json` 的 UTF-8 中文内容会截断成 `` 或破坏反斜杠转义导致 `ConvertFrom-Json` 与提交规范校验误报失败。
- **预防动作**：在读取外部工具 UTF-8 输出的 `.ps1` 门禁脚本开头显式设置 `[Console]::OutputEncoding = [Text.Encoding]::UTF8` 与 `$OutputEncoding = [Text.Encoding]::UTF8`，并加 `git -c i18n.logOutputEncoding=utf-8 -c core.quotepath=false`。
- **验证方式**：在 CP936 Python `subprocess` 宿主中直接运行 `scripts/check-all.ps1` 与 `verify-commits-and-privacy.ps1` 零乱码通过；**状态**：活跃。

### `L-012` Pi 项目级隔离 `context-mode` 与 Worktree 祖先信任兜底
- **场景与根因**（`Seen 1× (2026-11-27)`）：`npm:context-mode@2.2.1` 对超过 16 KB 未分段 `read` 直接拒读、>4 KB 结果截断入 FTS5，且在 `before_agent_start` 拼接输出损伤 Prompt Cache；同时默认 `ask` 信任模式下若 worktrees 父目录未列入全局 `trust.json`，非交互启动会跳过 `.pi/settings.json` 与 `aoci` MCP。
- **预防动作**：在仓内 `.pi/settings.json` 设 `"autoload": false` 与 `"extensions": ["!**"]` / `"skills": ["!**"]` 复用全局包禁用 `npm:context-mode`（缺 `autoload: false` 会触发 Pi 在工作树安装 `.pi/npm/` 并引发 AOCI `missing` 漂移），配合 `.gitignore` 忽略 `/.pi/npm/`，改由 AOCI + CodeGraph + 作用域门禁与独立探索会话协作；同步确认本机 Pi `trust.json` 已信任仓库与 worktrees 祖先目录。
- **验证方式**：AOCI `verify`/`check` 对齐、新工作树 `setup-worktree.ps1` 就绪且 Pi 启动不触发 `ctx_*` 拒读循环；**状态**：活跃（已同步写入 `ADR-010`）。

---

## 四、已自动化防呆归档区

> 当「活跃规则速查表」中的经验已被仓库脚本或自动化测试完全机器防呆且连续两个波次零复发后，将其迁入本区归档，以保持主控日常上下文精干。

- *暂无已迁出归档项（当前 `L-001` ~ `L-012` 处于活跃巡检期）。*
