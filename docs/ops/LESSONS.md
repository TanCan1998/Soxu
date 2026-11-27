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
| `L-004` | AOCI stdio MCP | `$env:TEMP` 独立 `.mjs` 首包发 `id: 1` `initialize`；**Overview 截断即停该链**（禁读落地溢出/Search/Entry/scope/源码补 Challenge），source-bound 续做与完整认知验证严格区分 | `L3 · Seen 3×` | `SKILL.md` / 本文 |
| `L-005` | AOCI 审查署名 | `scope acknowledge --reviewed-by` 统一用 `soxu-agent-<issue>`，严禁个人用户名 | `L2 · Seen 2×` | `SKILL.md` / 本文 |
| `L-006` | 双平台真机门禁 | 含条件编译/文件系统/工作树/进程语义的变更，合入前必跑 WSL 全仓 Clippy + 目标集成测试 | `L3 · Seen 2×` | `CODE_STANDARDS.md` / 本文 |
| `L-007` | 阻塞即时上报 | 遇冻结模块缺陷或两轮排查未果立刻发 `[BLOCKED / NEED-DECISION]`，主控拆单或 `steer` | `L3 · Seen 3×` | `AGENTS.md` / `SKILL.md` / `CODE_STANDARDS.md` |
| `L-008` | 系统盘与缓存治理 | 禁止把大型 target/长留产物堆在系统盘 `%TEMP%`；禁止给 `%TEMP%` 根建 Junction；波次收口巡检残留 | `L3 · Seen 1×` | 波次微复盘检查表 / 本文 |
| `L-009` | `.ps1` 测试退出码 | PowerShell 测试脚本调完负向外部子进程后末尾必写 `exit 0`，防 CI `shell: pwsh` 继承非零 `$LASTEXITCODE` | `L3 · Seen 1×` | `test-*.ps1` / 本文 |
| `L-010` | PR 关联与防竞态合入 | PR 正文裸写 `Closes #<num>` 不套反引号；合入强制带 `--match-head-commit <reviewed_sha>` | `L3 · Seen 1×` | `CODE_STANDARDS.md` / `SKILL.md` / 本文 |
| `L-011` | `.ps1` UTF-8 防乱码 | 中文 Windows 无终端宿主默认 CP936；读写 `git` 输出的门禁脚本开头必设 `Console/OutputEncoding = UTF8` | `L3 · Seen 1×` | `check-all.ps1` / `verify-commits-and-privacy.ps1` |
| `L-012` | Pi 上下文与缓存 | 项目级 `.pi/settings.json` 必设 `autoload: false` + `["!**"]` 禁 `context-mode` 防 `.pi/npm/` 漂移，且 `trust.json` 覆 worktrees | `L3 · Seen 1×` | `.pi/settings.json` / `.gitignore` / `SKILL.md` |
| `L-013` | 平台矩阵告警 | `-D warnings` 只译当前平台分支，且覆盖范围必须是全 workspace `--all-targets`（含集成测试目标）；平台专用符号的 `use` 必须与用例同 `cfg` 门控；缺 Darwin C 工具链时可用已装 Darwin ARM64 target 跑 `--workspace --exclude <C 依赖 crate> --all-targets` 交叉，并**单列排除范围与原生待办**；不加 `#[allow]`、不移栽 Linux 原语 | `L2 · Seen 3×` | 本文 / 波次验收 |
| `L-014` | 测试夹具后台维护 | 夹具 Git 命令必须与生产只读入口同源加 `-c maintenance.auto=false -c gc.auto=0`，且集中到唯一构造点；回归用 trace2 `child_start`/`child_exit`，启动与退出须同 `sid` + 同 `child_id`；正向对照加 `*autoDetach=false`；trace 目标须去掉 Windows `canonicalize` 的 `\\?\` 前缀（Git 会静默不写，UNC 需降为 `\\server\share`），`written=false` 一律判观察无效；trace 名在独占夹具根内唯一，不预删 | `L3 · Seen 3×` | 本文 / 波次验收（`SKILL.md`、防呆脚本均待主控落地） |
| `L-015` | 管道退出码 | `cargo ... \| tail` 拿到的是 `tail` 的 0；WSL/Linux 验证脚本必 `set -euo pipefail` 或查 `${PIPESTATUS[0]}`，重跑前 `--no-run` 重建并核对二进制 SHA/mtime | `L2 · Seen 1×` | 本文 / 波次验收 |
| `L-016` | 测试内子进程等待 | 观察“某子进程是否被派生”时，受测进程 stdin/stdout/stderr 全 `Stdio::null()`，证据只从 trace2 读；等待用 `spawn`+`try_wait`+截止+`kill`+回收窗口，五态可区分（成功/非零退出/超时/观察异常/回收未确认）；禁 `output()` 无界收集 | `L2 · Seen 1×` | 本文 / 波次验收 |
| `L-017` | 夹具环境隔离 | 真实 run 块在隔离子进程执行时不得依赖宿主预装工具：注入受控版本桩（rustc/cargo/node/pnpm）并把子进程 PATH 限为「桩目录 + pwsh 目录 + 系统目录」；桩每次调用写日志、证据字段须等于桩值，并加「无桩目录时不得出现桩值」负例；POSIX 桩的 PATH 项须做 MSYS 规范化（Windows 的 `C:/…` 会被 bash 的 PATH 冒号拆成两项，须 `cygpath -u` 得 `/c/…`）；桩只用于夹具，生产 Finalize 仍走真实工具链且工具缺失时失败闭合 | `L2 · Seen 1×` | `test-ci-gate.ps1` / 本文 |
| `L-018` | 测试夹具根与路径校验 | 断言“拒绝链接/重解析点”的夹具，其合法根必须先取自真实路径（`canonicalize`），再从该真实根派生资产、启动目录与重验目标；未规范化的系统临时目录会让正向构造先被拒绝、负向断言到不了被测分支。规范化**仅限 Unix**：Windows 的 `\\?\` 前缀会破坏 `cmd mklink /J` 与 Node `cwd` 契约。有意构造的链接/junction/逃逸路径一律原样交给被测函数 | `L2 · Seen 1×` | 本文 / 波次验收 |
| `L-019` | 平台能力夹具 | 文件名含非法 UTF-8 字节的夹具只在 Linux 建立盘语义：`macos-15` Runner 对 `diff.rs` 同类夹具实测 `fs::write` 返回 `EILSEQ`，`probe.rs`/`worktrees.rs` 的 `create_dir`/`write` 属同因推断且未原生复现，不得借此推断 Darwin 或其他文件系统整体能力。此类用例一律 `#[cfg(target_os = "linux")]`，不得用 `cfg(unix)` 广域桶；测试名与 `cfg` 同口径，并一次性搜完同 crate 兄弟集测 | `L2 · Seen 1×` | 本文 / 波次验收 |
| `L-020` | 同因扩围裁决 | 发现不在 Owned Paths 的同因缺陷时，先按 `[BLOCKED / NEED-DECISION]` 上报复现证据与目标执行顺序，由主控裁决把同因点并入同一 PR 扩白名单；不得自行越权修改，也不另开碎片化热修复 PR | `L2 · Seen 1×` | 本文 / `SKILL.md` |

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
- **预防动作**：`$env:TEMP` 独立 `.mjs`/Python 脚本严格发 `id: 1` `initialize` → `notifications/initialized` → `tools/call`；`aoci_overview` 出现 `continuation_required=false` 以外的截断信号（Host 截断、缺块、乱序、cursor 失败、Index/chunk 变化）时，**立即停止本次认知链**：禁止读取落地溢出文件，禁止用 `aoci_search` / `aoci_get_entries` / 改 `scope` / 读源码回补 Challenge，也禁止自行调小 `overview_delivery.chunk_tokens` 后自动重开（只提示用户）。截断后可继续 source-bound 调查与工程，但不得声称已具备完整系统认知；完整认知验证必须由一次干净的完整传输重新完成。严禁改 `.aoci/config.json`。
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

### `L-013` 平台矩阵告警只在目标平台编译时可见，必须交叉 Clippy 取证
- **场景与根因**（`Seen 3× (2026-11-27)`）：`cargo clippy ... -- -D warnings` 只编译当前主机的 `cfg` 分支。`soxu-git` 的 `open_handle` 在 `#[cfg(not(any(target_os = "linux", windows)))]` 分支上留下未使用参数与 `needless_return`，Windows/Linux 本地与远端都不报警，直到 `macos-15` ARM64 原生 Runner 才暴露；反之 Linux 专属告警在 Windows 同样隐形。第二次同因不同面：`crates/soxu-daemon/tests/worktree_lifecycle.rs` 把 `state::WorktreeAdmissionRequest`、`soxu_git::GitProbe`、`WORKTREE_CONTEXT_FILE` 写成顶层无条件 `use`，而使用点全在 `#[cfg(windows)]` 用例与 `#[cfg(target_os = "linux")]` 模块内，macOS 编译面零使用点，遂在集成测试目标上报三条 `unused import`（run `45929401718` / job `140278345298`，测试步骤 skipped，证据为 `build_failed`）。第三次同因不同面：`crates/soxu-magpie/tests/worktree_context.rs` 把 `WORKTREE_CONTEXT_FILE` 与 `WorktreeContextServer` 写成顶层无条件 `use`，两者使用点全部落在 `#[cfg(any(windows, target_os = "linux"))]` 的三个用例内，macOS 编译面零使用点，遂在集成测试目标上报两项 `unused import`（run `45934505956` attempt 1 / Darwin job `140294571285`，检出 `fc5d9034`，测试步骤 skipped）。三次都落在“平台专用符号的导入面/分支面”上，且都是 Darwin 面先于本机暴露。
- **预防动作**：涉及平台条件编译的 crate 合入前必查 `rustup target list --installed`，并用 `cargo clippy -p <crate> --all-targets --target aarch64-apple-darwin --locked -- -D warnings` 补交叉编译证据。修法必须按 `cfg` 拆函数、保持 `UnsupportedPlatform` 等 fail-closed 语义，禁止 `#[allow]` 掩盖、禁止把 Linux 原语（`O_NOFOLLOW`/`O_DIRECTORY`）放宽到 Darwin。由本次新增三点：①覆盖范围口径写“全 workspace `--all-targets`”，不得用某个 crate（如 `soxu-git`）的交叉 Clippy 代表其它 crate 的集成测试目标已验证；②写测试时，平台专用符号的 `use` 必须与使用它的用例同 `cfg` 门控（形如 `#[cfg(any(windows, target_os = "linux"))] use ...;`），不要先写顶层无条件导入、等 CI 报错再拆；③没有 Darwin C SDK 时交叉仍**不是不可得**：复用已装的 Darwin ARM64 target，跑 `cargo clippy --workspace --exclude soxu-daemon --all-targets --target aarch64-apple-darwin --locked -- -D warnings`，并在同一份证据里**单列排除范围**（`soxu-daemon` 因 `libsqlite3-sys` 需要本机未提供的 Darwin 编译器/SDK）与**被排除范围的原生待办**，不得把 `--exclude` 后的全绿写成“全仓 Darwin 已验证”。
- **验证方式**：交叉 Clippy 零告警 + 远端对应平台 Runner 原生门禁；报告中严格区分「交叉编译证据」与「真机执行」，不得把前者写成后者。若交叉本身被环境阻断，必须如实公示并标未验：本次 `--target aarch64-apple-darwin` `exit 101`，停在 `libsqlite3-sys` 的 `cc-rs: failed to find tool "cc"`（无 Darwin C 工具链/SDK），属 C 构建边界而非“测试目标 Clippy 已通过”；不得为此装 SDK、反复重试交叉或修改依赖，改用“macOS 编译面只保留无条件顶层代码”逐处比对使用点（人工使用点审查只用于定位，双平台全绿不证明 Darwin lint 通过），并交由 `#88` 合流后的真实 `macos-15` Runner 用“原错误 + 修复后同目标 Clippy”组成回退证据。`#96` 已用③的口径取得本机交叉回退证据：同一 `--workspace --exclude soxu-daemon --all-targets --target aarch64-apple-darwin` 命令修复前 `exit 101`（`15.47s`，2 条 error，`worktree_context.rs:2:22`），修复后 `exit 0`（`0.86s`，0 error / 0 warning）；该 `exit 0` 仅覆盖排除 `soxu-daemon` 后的目标面，不能冒称全仓或 Darwin 原生通过。**状态**：活跃。

### `L-014` 测试夹具的 Git 命令必须与生产只读入口同源禁用后台维护
- **场景与根因**（`Seen 3× (2026-11-27)`）：run `45914578287` 的 Ubuntu 24.04 任务上，`binary_and_non_utf8_text_keep_distinct_output_contracts` 的整树快照只差 `.git/objects/maintenance.lock` 的增删与 `.git/objects` 的 mtime。根因是**夹具自己的 `git commit`** 派生了 detached `git maintenance run --auto`（trace2 可见 `child_start` 带 `--detach`），它持有 `objects/maintenance.lock` 并推进目录 mtime，与快照断言并发写。生产只读入口（`probe.rs`、`checkpoint/git.rs`、`repository_create/sandbox.rs`）都已用 `-c maintenance.auto=false -c gc.auto=0` 关闭，夹具漏了这套同源守卫。第二次同因不同面：`crates/soxu-git/tests/repository_create.rs::git` 同样缺少这套守卫，`#89` head `b8ca5fcd` 的 run `45951887087`（Linux job `140349512352`）在 `clone_records_authorized_source_without_temporary_path` 失败，源快照唯一差异也是 `.git/objects/maintenance.lock`（before 有、after 无）；`#103` 以 trace2 正负对照取得实际派生证据后才定因，**不把锁文件名字当作完整定因**。第三次同因更广：`#105` 指向 `probe.rs` 的 `Fixture::run`，同 crate 的 `fixed_diff.rs::command`、`project.rs::Fixture::run`、`worktrees.rs::Fixture::run` 三处属同一构造点缺口（各自夹具先自行提交、再对整根含 `.git` 做零写入断言）。`probe.rs` 的 `file_bytes` 只比较**文件集合与字节**（不含目录 mtime），`PR #89` run `45976349619` attempt 1（head `32969a0`、实际检出 `b724235`、`macos-15` ARM64 job `140429021160`、Git `2.56.1`）的两份 map 为 28 比 27 个文件、`left_only` 仅 `.git/objects/maintenance.lock`，该快照只能证明文件集合差异；**派生链由本轮夹具 trace2 对照取得，不把该历史 run 说成已追踪到后台进程**。
- **预防动作**：把守卫集中到夹具 Git 命令的唯一构造点（单一 `command_with` helper；`repository_create.rs` 为 `git_command`，`extra_args` 追加在守卫之后）。回归用 `GIT_TRACE2_EVENT` 的 `child_start`/`child_exit` 直接断言行为，不用 `sleep`、不删断言、不靠反复重跑；正向对照必须追加 `-c maintenance.autoDetach=false -c gc.autoDetach=false`（已验证 Git 2.53/2.56 均支持），让提交同步等待维护结束，并以**同 `sid` + 同 `child_id`** 的 `child_exit` 核实对照确实等到了该维护子进程退出；只表述对照实际观察到的退出证据，不认定其后代进程已全部回收。对照必须用**有界等待 + 无管道收集**（见 `L-016`），不得用 `Command::output()` 等可能派生子进程的命令。注意 `gc.auto=0` 单独并不足以阻止派生（Linux Git 2.53 实测仍派生），`maintenance.auto=false` 才是关键；对照只翻 `maintenance.auto=true`，`gc.auto` 保持 `0`，不要为了“让维护真跑一下”去开 `gc.auto`。观察用 trace 文件写在夹具自建的真实根内（名字在该 fixture 私有根内唯一），不复用系统临时目录的独立裸文件，也不预删可能碰撞的路径；**trace 目标必须去掉 Windows `canonicalize` 产生的 `\\?\` 前缀**——`#105` 实测该前缀会让 Git 静默不写 trace 文件，表现为 `written=false`，若把它读成“未派生”就是空断言；去掉前缀时必须仍得绝对路径（`\\?\UNC\s\share` 降为 `\\s\share`），不做无条件 `trim`。
- **验证方式**：删掉夹具守卫后回归用例如期失败（`started: true, detached: true`），恢复后受测字节 SHA-256 不变并通过；修复前旧实现重跑 40 次全部通过，证明重跑不能作为修复证据。`#103` 以同一判据在 `repository_create.rs` 上复现：移除 `git_command` 的两行守卫后 `fixture_commits_never_start_background_maintenance` 确定性失败（`MaintenanceTrace { written: true, started: true, detached: true, exited: true }`，`exit 101`），恢复后源文件 SHA-256 与重建的受测二进制 SHA-256 均回到移除前同一值。`#105` 在 `probe.rs` 复现同一判据：仅删 `Fixture::command` 的两行守卫后重建，`probe.rs` 源 `5601BE90…`、`probe` 二进制 `D044B3A2…`，同一回归 `0.34s` 内 `exit 101` 且 trace 为 `written/started/detached/exited` 全 `true`；恢复后源回到 `B4ED49E2…`、重建二进制 `F2E4E91F…`，`10 passed / 0 failed`。**同一回退态下 Windows 上原两个快照用例本次仍通过**，只能说明本次 Windows 回退未再现历史快照差异，不得据此说该竞态“在 Windows 不复现”；守卫的依据是 trace2 派生事实，而不是本地复现 Darwin 快照差异。**状态**：活跃（`L3`，第三次同类实测已触发晋升；`SKILL.md` / `CODE_STANDARDS.md` 的落地与「扫描 `crates/*/tests/**` 夹具构造点、凡有整树零写入断言却缺 `maintenance.auto=false` 即失败」的防呆脚本均由主控另行排期，本轮未改白名单以外文件）。

### `L-015` 管道会吞掉子进程退出码，重跑前必须重建二进制
- **场景与根因**（`Seen 1× (2026-11-27)`）：`cargo test ... | tail` 的退出码是 `tail` 的 `0`，编译或测试失败会被静默当作成功；另一次竞态重复实验中，回退实验后恢复源码却没有重新构建，直接跑到了回退实验遗留的旧二进制，得出「全仓套件 8/8 失败」的错误结论。后者是操作疏漏，不是工具缺陷：`cargo test` 已按 mtime 判断，只是当时没有触发重建。
- **预防动作**：WSL/Linux 验证脚本统一 `set -euo pipefail` 或显式检查 `${PIPESTATUS[0]}`；先重定向到日志文件、单独检查退出码、再 `tail` 日志。做过任何源码回退/恢复实验后，重复实验前必显式显示 `cargo test --no-run` 输出并打印二进制 `mtime` 与 SHA-256，确认跑的是当前源码。
- **验证方式**：每一步都留下 `exit=0` 逐条痕迹，且旧二进制误判不再出现。**状态**：活跃。

### `L-016` 测试内子进程必须无管道、有界等待，失败证据必须有界脱敏
- **场景与根因**（`Seen 1× (2026-11-27)`）：PR `#91` 原 head 的 Windows `rust-check`（run `45917865092` job `140241392408`）在 `Run scoped unit & integration tests` 触到 30 分钟 job 级上限后 cancelled，且 job/run 日志均 `log not found`，远端无任何 per-binary 粒度。同范围本地运行（`-p soxu-daemon -p soxu-git`）**未复现**该超时（原候选 `111s`、最终源码 `94s`），因此远端超时仍属未定因。本条针对的是本用例阳性对照自身的、可独立证实的风险：旧实现刻意重新打开后台维护，并用 `Command::output()` 无界收集 stdout/stderr；`output()` 要读到管道所有写端关闭，一旦有继承写端的子/孙进程存活就会永久阻塞，表现为 step 永不结束、日志无后续行。
- **预防动作**：凡“观察某子进程是否被派生”的用例，受测进程一律 `stdin(Stdio::null())` 与 `stdout/stderr` 不接管道，证据只从 trace2 文件读；等待改成 `spawn` + `try_wait` 轮询 + 截止时间 + 超时 `kill` + 回收窗口，并让“成功 / 非零退出 / 超时 / 观察异常 / 回收未确认”五种结局可分别判定（`try_wait` 自身失败也必须走 kill + 有界回收，不得直接丢弃可能存活的直接子进程）。作为证据的事件必须**按发出进程 `sid` 绑定**：`child_id` 只是该进程内部的子进程序号，启动与退出必须同 `sid` + 同 `child_id` 才算闭环，否则会把后一层 Git 子进程的同号事件误当成顶层维护进程已结束（依据 Git 官方 `api-trace2.html` 的 Event 语义）。超时只允许声称回收**直接子进程**，不得写成对后代进程的证明。断言失败信息只输出布尔与计数摘要，**禁止**在 CI 转储原始 trace2 或整份环境。
- **验证方式**：移除夹具守卫后用例确定性失败且 `0.3s` 内结束；把期限临时改成 `0` 得到 `timed_out` 且 `0.14s` 结束；把 `sid` 绑定退回只看 `child_id` 时负例用例失败；恢复字节后 SHA-256 一致。**状态**：活跃。

### `L-018` 断言路径校验的测试夹具根必须先取真实路径，且规范化只在 Unix 生效
- **场景与根因**（`Seen 1× (2026-11-27)`）：`NativeAssetGuard` 按设计拒绝任何含符号链接的祖先目录，但夹具根直接取自未规范化的系统临时目录。`macos-15` ARM64 CI（run `45940068623`）的 `soxu-daemon --test native_assets` 因此 1 通过 / 5 失败、退码 `101`，失败点全在正向构造（`NativeAssetGuard::new`）而非被测的拒绝分支；同套件唯一“通过”的用例全以 `is_err()` 断言，属于负向断言没走到分支的**伪通过**。用受控 symlink 作 `TMPDIR` 在 Linux 可确定性复现同一根因：全仓 7 个测试目标失败（`soxu-daemon` lib / `native_assets` / 集成 WS、`soxu-drivers` native、`soxu-git` lib / `repository_create`、`soxu-magpie` `worktree_context`）。
- **预防动作**：凡“拒绝链接/重解析点/路径逃逸”的夹具，先创建自己的临时目录，再以 `canonicalize` 取真实路径为根，**全部资产、启动目录、日志与重验目标都从这一个真实根派生**，让正向构造真正成功、负向断言真正落到被测分支。**有意构造**的资产链接、祖先链接、junction、目录替换与 `..` 路径仍原样传给被测函数，不得提前规范化。规范化必须限定在 `#[cfg(unix)]`：Windows 上 `canonicalize` 会附加 `\\?\` 扩展路径前缀，实测会让 `soxu-drivers --test native` 由 `10 passed` 退化为 `4 passed / 6 failed`（Node 夹具 `process.cwd()` 的物理路径与 `SOXU_FIXTURE_CWD` 不再字符串相等，子进程断言退出），并使 `cmd mklink /J` 失效。
- **验证方式**：`TMPDIR` 指向受控 symlink 的 Linux 全仓 `cargo test --workspace --locked --no-fail-fast` 修复前 `exit 101` / 修复后 `exit 0`，普通目录对照 `exit 0`，两者均 40 条 `test result` 汇总全部通过（含夹具派生子进程输出，不能据此称 40 个独立套件）；逐目标计数分别回到 `2 / 6 / 16 / 9 / 27 / 28 / 6` 通过，且负向用例仍在。**状态**：活跃（未晋升，待复现计数或脚本防呆）。

### `L-019` 非法 UTF-8 文件名的夹具只在 Linux 建立盘语义，用具体目标而非 `unix` 桶
- **场景与根因**（`Seen 1× (2026-11-27)`）：`#89` head `b8ca5fcd` 的 run `45951887087`（`macos-15` ARM64）在 `crates/soxu-git/tests/diff.rs::unix_preserves_raw_linebreak_name_and_refuses_a_replaced_symlink` 的 `fs::write` 报 `EILSEQ`。夹具文件名 `line\nbyte\xff` 含非法 UTF-8 字节，而**当前 `macos-15` Runner 所用文件系统对该夹具返回 `EILSEQ`**；Linux 是本项目明确验证支持原始字节路径语义的运行面。属“平台能力明确却写成 `cfg(unix)` 广域桶”的类型错误，与 `L-013` 的导入面/分支面问题同族但根因不同（这次是测试**建盘语义**的平台边界，不是 lint 面）。
- **预防动作**：依赖“文件系统可表示非法字节路径”的用例一律 `#[cfg(target_os = "linux")]`，与仓库既有 `checkpoint.rs::linux_rejects_links_fifos_...`、`repository_create.rs` 同口径；测试名同步改成 `linux_...` 并在注释里写清实测 Runner 的 `EILSEQ` 边界，**不往“某种文件系统只接受合法 UTF-8”这类泛化结论写**。不得为让它“能跑”而删用例、加 `#[allow]`、改被测代码或把夹具改成 lossy。改完必须定向搜全仓同因：`rg -n -g '*.rs' 'from_bytes|from_vec' crates` 配合 `\xff`/`\xfe` 字面量（PS7 不展开 `crates/**/*.rs` 这类 glob，必须交给 `rg` 的 `-g`），并逐个区分“建盘夹具”与“纯内存字节解析”——本次即据此发现 `tests/probe.rs`、`tests/worktrees.rs` 各一处同因建盘夹具（受 `diff` 先行失败掩盖），而 `status.rs`、`worktrees.rs` 的清单解析与 `diff.rs` 的 `\xff` 文件**内容**属跨平台安全，一并保留。
- **验证方式**：Linux 全仓 `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` 与 `cargo test --workspace --locked --no-fail-fast --offline` 均 `exit 0`，三处原始字节用例在 Linux **实际执行**并通过；Windows 同目标分别 `15 / 8 / 10 passed`（三处 Linux 用例不编译）；Darwin 交叉 `cargo clippy --workspace --exclude soxu-daemon --all-targets --target aarch64-apple-darwin` `exit 0`；三者均不得陈述为 macOS 原生通过。**状态**：活跃。

### `L-020` 同因扩围由主控裁决并入同一 PR，不越权也不碎片化
- **场景与根因**（`Seen 1× (2026-11-27)`）：`#100` 首轮只修 `diff.rs` 一处，定向扫描发现 `probe.rs`/`worktrees.rs` 两处同因用例不在 Owned Paths 内。若子代理自行扩面即越权；若各自另开 Issue/PR，则需三轮独立验证、三轮 Darwin 重跑才能全部闭合，且每轮都会被前一处先行失败掩盖。
- **预防动作**：先按 `[BLOCKED / NEED-DECISION]` 上报精确复现、目标执行顺序与掩盖机制，不动手修改；主控裁决后由主控把 Owned Paths 正式扩至同因文件，执行代理在同一工作树/分支续做并保留原环境、参考索引复用与安全边界，不重启代理或另开分支。
- **验证方式**：三处用例在同一 PR 内同口径门控与改名，一次 WSL 全仓复验三处均实际执行通过；PR 正文与看板同步记录裁决与扩围原因。**状态**：活跃。

---

## 四、已自动化防呆归档区

> 当「活跃规则速查表」中的经验已被仓库脚本或自动化测试完全机器防呆且连续两个波次零复发后，将其迁入本区归档，以保持主控日常上下文精干。

- *暂无已迁出归档项（当前 `L-001` ~ `L-016`、`L-018` ~ `L-020` 处于活跃巡检期；`L-017` 由 `#88` 冻结候选占用，合流后补齐）。*
