---
name: paseo-orchestrator
description: 跨 AGY 桌面端与 Codex 桌面端编排 Soxu 工作树和子代理，核验任务书送达，并让子代理复用参考项目既有 AOCI 与 CodeGraph。用于接手、派发、调控及主控切换。
---

# Soxu 跨桌面端 Paseo 协同编排指南

本技能随仓库共享于 `.agents/skills/paseo-orchestrator/SKILL.md`，**Google Antigravity（AGY 桌面端）** 与 **OpenAI Codex 桌面端** 均会原生自动发现。

---

## 一、统一控制入口（严禁裸调 `paseo`）

由于本机通过 Scoop 安装的 Paseo CLI 未在全局 PATH 暴露 shim，且桌面端未打开时本地守护进程可能处于休眠状态，**双端一律通过仓库包装脚本调用 Paseo**：

```powershell
pwsh -NoProfile -File scripts/paseo-ctl.ps1 <Paseo 参数...>
```

参考入口盘点用 `scripts/paseo-reference-context.ps1`，`pi` 专用带环境启动用 `scripts/paseo-run-pi.ps1`，文件投递及全文核验用 `scripts/paseo-dispatch.ps1`；盘点与核验脚本不创建代理、不改全局配置，也不初始化或重建索引。

脚本特性：
1. 自动探测 `$env:PASEO_CLI`、PATH、Scoop 默认路径及常见安装目录中的 `paseo.cmd`；
2. 对 `project`、`workspace`、`ls`、`run`、`send`、`wait`、`logs`、`stop`、`rm`、`script`、`host` 等需要本地服务的命令，自动检查 `status --json` 并在后台休眠时自动执行 `start`；
3. 状态提示仅输出至 stderr，**stdout 始终保持纯净**，可直接管道连接 `| ConvertFrom-Json`。

---

## 二、AGY 桌面端与 Codex 桌面端「隔离与共享」矩阵

| 维度 | 类别 | 具体载体与边界约束 |
| :--- | :--- | :--- |
| **工程规范与进度认知** | 共享 | `AGENTS.md`、`docs/ops/PROGRESS.md`、`docs/ops/DECISIONS.md`、`docs/ops/LESSONS.md`、`aoci.txt` + `.aoci/`、`.agents/skills/`（全仓 Git 版本化，双端读取同一份事实来源） |
| **工具链与构建缓存** | 共享 | `mise.toml`、`paseo.json`、`Cargo.toml` 与上级共享缓存 `../.cargo-target/soxu`（双端均启用工作区 `scripts/check-all.ps1` 门禁） |
| **Paseo 运行状态池** | 共享 | 本机 Paseo 守护进程（`127.0.0.1:6767`）、项目 `Soxu`（`prj_24ffa03410264684`）、主控工作台（`wks_21203a9ef0f5e2e5`）、全部隔离 Worktree 与活跃子代理会话 |
| **宿主项目注册与全局配置** | 隔离 | **AGY**：项目注册在 `~/.gemini/config/projects/`，MCP 在 `~/.gemini/config/mcp_config.json`；**Codex**：项目注册在 `~/.codex/state_5.sqlite`，仓库级覆盖在已忽略的 `.codex/config.toml`。双端严禁相互篡改对方宿主配置或 `~/.claude` / `~/.pi` / `~/.config/magpie` |
| **工作树写操作（并发边界）** | 隔离 | 主检出仓库目录（项目根路径）仅作**主控编排、审查与工程治理台（Control Plane）**。**严禁** AGY 与 Codex 同时在主检出目录直接并发修改业务源码；功能开发一律下沉至独立的 `--isolation worktree` 分支并严守 Issue `Owned Paths` 白名单 |

> 说明：新建的 Paseo 隔离 Worktree 会通过 `paseo.json` 中的 `worktree.setup` 自动运行 `scripts/setup-worktree.ps1`，完成 `.githooks` 隐私与 Git Emoji 提交检查配置并自动执行 `mise trust` 激活项目锁定环境。

---

## 三、核心 Paseo 编排命令速查

### 1. 盘点全局状态、项目、工作区与子代理
```powershell
pwsh -NoProfile -File scripts/paseo-ctl.ps1 status --json | ConvertFrom-Json
pwsh -NoProfile -File scripts/paseo-ctl.ps1 project ls --json | ConvertFrom-Json
pwsh -NoProfile -File scripts/paseo-ctl.ps1 workspace ls --json | ConvertFrom-Json
pwsh -NoProfile -File scripts/paseo-ctl.ps1 ls --all --global --json | ConvertFrom-Json
```

### 2. 创建隔离 Git Worktree 工作区
```powershell
pwsh -NoProfile -File scripts/paseo-ctl.ps1 workspace create `
  --project prj_24ffa03410264684 `
  --title "feat-12-示例任务" `
  --isolation worktree `
  --mode branch-off `
  --new-branch "feat/12-example-task" `
  --base origin/main `
  --json
```

### 3. 派发任务并核验全文送达

**准备任务书与规定环境。** 后续所有子代理派发**仅使用 `pi` agent**（`--provider pi` + 经 `pi --list-models` 核实的 `magpie/...` 模型与思考等级），禁止新派发或续派 `codex` / `claude` / `antigravity` 等其他驱动。用 UTF-8 文件保存正文，第一行放本次唯一派发标记（例如 `Soxu #49 / <派发批次>`）。任务书放系统临时目录或已忽略的 `.agents/local/`，不提交本机路径或运行凭据。先盘点全局代理与 Worktree 环境，按本次用户指定的 `pi` 模型和思考强度选型；不改全局偏好、不静默换型或重启整个服务。

- **规定环境（Mise Toolchain）**。写明目标 Worktree 须先执行 `scripts/setup-worktree.ps1`（完成 `mise trust` 与 hooks 安装），锁定使用项目 `mise.toml` 工具链（`rust 1.99.1` 含 `rustfmt`/`clippy`、`node 26.13.1`、`pnpm 12.21.0`，`disable_tools = ["python", "java", "maven"]`，`PNPM_CONFIG_MANAGE_PACKAGE_MANAGER_VERSIONS=false`）与构建缓存目录。并行任务书必须指定每路独立的 `CARGO_TARGET_DIR`，按下方示例加载锁定环境并上报 doctor 实测结果。
- **任务和范围**。写明 Issue、目标、工作树和分支、`Owned Paths`、依赖和禁止碰触的范围。
- **参考项目**。AOCI 和 CodeGraph 在派发中的主要用途是帮助子代理理解参考项目，而不是让其在 Soxu 工作树重新建图。逐项给出参考根、版本、既有索引及查询入口，再标明源码文件、符号、采用点和边界。`.refs/` 不随 Worktree 检出；必须提供目标宿主可读的真实入口，远程代理不能直接沿用主控本机路径。
- **参考 AOCI**。复用参考根已有 `aoci.txt`（Legacy）或 Root 所声明的 Volumes，绑定该根的 MCP 先 `aoci_rules`、再 `aoci_overview`，遵守各自实时合同、分块和 Attestation。当前宿主 MCP 若绑定 Soxu，不能拿它查询参考仓库；改用已核实的 `aoci --repo <参考根> mcp` stdio 会话，按 `initialize`、`notifications/initialized`、`tools/list`、`tools/call` 调用。读取索引文件只能称为读取参考索引，不能冒充完整认知校验。不同参考仓库的收据和 Challenge 不混用。
- **参考 CodeGraph**。复用同一参考根的 `.codegraph`；Magpie `codegraph__codegraph_explore` 参数按实际 Schema 显式绑定参考根。MCP 无法切根时，用 `codegraph explore --path <参考根> <查询>` / `codegraph node --path <参考根> <符号>`。禁止指向目标工作树或把参考数据库复制进去。索引缺失、过期、解析缺口或入口不可访问时报告，按任务允许范围回退 `rg` 与源码；不得自动 `init`、`index`、`sync` 或升级参考索引。
- **目标工程治理另列**。Soxu 工作树仍遵守其 AGENTS 与 AOCI 认知/收尾维护要求，但这不替代参考索引复用。若授权收尾维护，在任务书的 Owned Paths 中写明「本分支 AOCI 托管资产（`aoci*.txt`、`.aoci/**`）」，且白名单 `scope acknowledge --reviewed-by` 统一使用脱敏代理标识（如 `soxu-agent-<issue>`），严禁填个人用户名，严禁修改 `.aoci/config.json`。不得因参考调查修改参考源码、AOCI 正式资产或 CodeGraph 图；查询工具可能有自身本地运行审计，不把普通查询声称为严格零写入。
- **跨平台 / WSL 真机与 AOCI MCP 调用防坑**。在 WSL/Linux 下运行 `cargo` 前，须将真实 Rust/Node toolchain `bin` 目录前置到 `PATH`（绕开会重读 `mise.toml` 覆写 `CARGO_TARGET_DIR` 的 `mise` shims）并指定 Linux 本地独立缓存目录（如 `/root/.cache/...`），同时严禁在 WSL 内对带 Windows 绝对路径 `.git` 的宿主机 Worktree 运行 `git` 命令；通过 `aoci --repo <路径> mcp` 调用 stdio MCP 时，将独立 UTF-8 `.mjs` 脚本写入 `$env:TEMP` 执行并严格首包发送 `id: 1` 的 `initialize`。
- **验收与交付**。写明必须通过的检查、PR 要报告的参考采用及工具使用证据，以及需要立即回报的阻塞。正文要求代理等待主控核验后发出的开工指令。

- **即时上报与停点协议**。遇环境、工具链或文件锁冲突，AOCI / CodeGraph 或脚本异常、截断，Owned Paths 或契约冲突、冻结代码缺陷，设计歧义或平台原语缺失，或同一问题连续排查两轮未果时，立即停止后续修改。按下方模板上报并结束当前回合，等待主控 `steer`。禁止越权试改或推迟到交稿才报告。

```text
[BLOCKED / NEED-DECISION]
- 阶段：<当前步骤>
- 阻塞点：<一句话说明>
- 复现命令与摘要：<精确命令及关键报错原文，去除凭据和隐私路径>
- 已尝试动作：<至多两轮只读排查>
- 可选方案与影响：<选项及 Owned Paths、依赖、契约影响>
- 需要主控裁决：<明确需要决定的问题>
```

**并行构建环境示例。** 严禁用 `mise run ...` 启动门禁或 doctor，子任务会重读 `mise.toml` 并覆盖独立 target。先注入 `mise env --json`，再显式覆写 `$env:CARGO_TARGET_DIR` 为各自独立目录，随后直接运行脚本或 `cargo`。

```powershell
$env:MISE_DISABLE_TOOLS = "python,java,maven"
$env:PNPM_CONFIG_MANAGE_PACKAGE_MANAGER_VERSIONS = "false"
$miseEnvJson = mise env --json
if ($LASTEXITCODE -ne 0) { throw "mise env --json 执行失败" }
$miseEnv = ($miseEnvJson | Out-String) | ConvertFrom-Json
foreach ($entry in $miseEnv.PSObject.Properties) {
  [Environment]::SetEnvironmentVariable($entry.Name, [string]$entry.Value, "Process")
}
$env:CARGO_TARGET_DIR = Join-Path $PWD.Path "../.cargo-target/soxu-示例任务" # 替换为任务书指定目录
pwsh -NoProfile -File scripts/mise-doctor.ps1
cargo --version
pwsh -NoProfile -File scripts/check-all.ps1
```

生成确定性参考清单（只查资产、版本和工具状态，不生成语义、不建索引）：

```powershell
pwsh -NoProfile -File scripts/paseo-reference-context.ps1 `
  -Names paseo,codeg,padu | Set-Content -Encoding utf8 .agents/local/reference-context.json
```

检查清单的 `issues`、`aoci.statusChecked` 与 `codegraph.statusChecked`，把选中参考的根、资产摘要、查询参数及明确回退完整写进任务书；清单本身不证明子代理已经读懂参考。资产缺失或状态异常不能隐藏，也不自动重建。若需开工前强制所有参考可查询，给盘点脚本加 `-RequireReady`。子代理开工后应回报参考根与 HEAD、AOCI 实际读取/校验状态、CodeGraph 实际查询与源码定位，PR 保留证据及取舍。

**启动（仅限 `pi` agent + 绑定 `mise` 环境）。** 从主控仓库根执行。以下变量使用本次已核实的实际值（`$model` 须为 `pi --list-models` 验证过的 `magpie/...` 模型，如 `magpie/group/gpt-6.2-sol` / `magpie/group/gpt-6.2-astra`）。首条可见消息用短单行写明 Issue、参考项目名称、AOCI 和 CodeGraph、规定 `mise` 环境，并说明随后还有完整任务书。当前 `run --help` 只提供位置参数 `<prompt>`，禁止经 `run` 的位置参数传递多行任务书。统一通过 `scripts/paseo-run-pi.ps1` 启动，自动注入工作区 `mise env --json` 工具链前置 `PATH`、`MISE_DISABLE_TOOLS=python,java,maven` 与锁定变量：

```powershell
pwsh -NoProfile -File scripts/paseo-run-pi.ps1 `
  -WorkspaceId $workspaceId -WorkspacePath $workspacePath `
  -Title "Soxu #$issueNumber" `
  -Model $model -Thinking $thinking `
  -Prompt "Soxu #$issueNumber。参考 $referenceNames，复用这些参考项目已有的 AOCI 和 CodeGraph；按项目 mise.toml（rust 1.99.1 / node 26.13.1 / pnpm 12.21.0）规定环境运行。请先回执，等待完整任务书与开工确认。"
```

**投递与核验。** 记录实际代理 ID。首轮回执结束后，用 `send --prompt-file` 发送 UTF-8 文件。对仍在执行的代理补发正文时使用 `--behavior steer`。人工主控直接发送时避免使用需要发送方 Agent ID 的 `queue`。

```powershell
pwsh -NoProfile -File scripts/paseo-dispatch.ps1 `
  -AgentId $agentId -PromptFile $promptFile
```

脚本通过 `paseo-ctl.ps1` 发文件并有限等待日志，只统一 CRLF/LF 和末尾换行，核对完整 `user_message.text`；返回代理 ID、消息 ID、正文摘要和长度。它不自动确认开工、不重发、不创建代理。仅复查已发送正文时加 `-VerifyOnly`。日志缺失或截断时先诊断，再按同一标记向同一代理补发并复核；全文核验通过且参考入口与回退已确认后，主控才发含标记的开工指令。首句预览、启动成功和资产存在均不能替代全文或参考实际使用证据。

### 4. 实时干预（热纠偏 / 排队 / 打断）与日志观察
```powershell
pwsh -NoProfile -File scripts/paseo-ctl.ps1 send <agent_id> "请补充边界测试后重跑 check-all" --behavior steer
pwsh -NoProfile -File scripts/paseo-ctl.ps1 send <agent_id> "暂停当前方案并改用增量游标" --behavior interrupt
pwsh -NoProfile -File scripts/paseo-ctl.ps1 logs <agent_id> --tail 30
pwsh -NoProfile -File scripts/paseo-ctl.ps1 wait <agent_id>
```

**阻塞协同闭环。** 主控派发后持续巡检活跃代理。发现 `[BLOCKED / NEED-DECISION]` 时，先用 `logs <agent_id> --tail 30` 查看阻塞原文与上下文，核实复现、实际影响及 Owned Paths 和契约边界，再裁决或就重大决策报请用户。裁决后用 `send <agent_id> "<裁决、允许范围与恢复步骤>" --behavior steer` 放行，并复查日志确认收到裁决和恢复状态。

### 5. 工作区后台服务与脚本控制
```powershell
pwsh -NoProfile -File scripts/paseo-ctl.ps1 script ls --workspace <wks_id>
pwsh -NoProfile -File scripts/paseo-ctl.ps1 script start check-all --workspace <wks_id>
pwsh -NoProfile -File scripts/paseo-ctl.ps1 script stop check-all --workspace <wks_id>
```

---

## 四、AGY ⇄ Codex 30 秒随时无损切换 SOP

当用户从 AGY 桌面端切换至 Codex 桌面端（或反向切换）继续控制 Paseo 开发时：

1. **切出方（移交前）**：
   - 将长耗时编码任务托管在独立 Paseo Worktree / Agent 会话中，确保主检出目录无未规划的半成品脏改动；
   - 若完成了里程碑、PR 状态变更或重要决策，同步更新 `docs/ops/PROGRESS.md` 与 `aoci` 索引。
2. **切入方（接手前 30 秒恢复）**：
   1. 阅读根目录 `AGENTS.md`；
   2. 阅读 `docs/ops/PROGRESS.md`、`docs/ops/DECISIONS.md` 与 `docs/ops/LESSONS.md`「活跃规则速查表」；
   3. 执行只读盘点与接管：
      ```powershell
      git status -sb
      gh issue list --limit 20 --json number,title,milestone,assignees
      gh pr list --json number,title,headRefName,state
      pwsh -NoProfile -File scripts/paseo-ctl.ps1 workspace ls --json
      pwsh -NoProfile -File scripts/paseo-ctl.ps1 ls --json
      ```
   4. 若有正在运行的 Paseo Agent，直接通过 `logs <agent_id> --tail 30` 检查进度，并用 `send <agent_id> ...` 继续调控，**无需推倒重来**。

---

## 五、主控自主经验收集与流程自优化闭环（Retro & Self-Evolution）

主控不仅调度开发与合入，还负责把多 Agent 实战中暴露出的工具链陷阱、阻塞模式与跨平台故障转化为持久可复用的工程资产，形成**「捕获 → 沉淀 → 晋升 → 防呆」**四步闭环：

### 1. 四类自动触发采集时机
1. **阻塞裁决闭环后**：每当处理完一次子代理 `[BLOCKED / NEED-DECISION]` 并完成 `steer` 恢复或拆单热修复；
2. **双平台 / CI 捕获真机缺陷后**：Windows CI 通过但 WSL/Linux 暴露编译/Clippy/语义缺陷，或远端 CI/Hooks 拦截出新根因；
3. **连续两轮重复摩擦后**：同一操作步骤（如环境注入、AOCI 初始化、Worktree 合并、缓存占用）在连续两轮中重复踩坑或耗时异常；
4. **波次（Wave）收口结束时**：每完成一轮并行波次的串行合入，执行一次极简波次微复盘（不额外铺张，只提炼可行动条目）。

### 2. 三级沉淀与自主晋升漏斗
- **L1（偶发观察）**：仅影响单次孤立变更，留在对应 PR 说明或单波汇报，不写入持久经验账本。
- **L2（可复用经验）**：提炼为严格脱敏的结构化条目写入 `docs/ops/LESSONS.md`（包含 `ID`、`适用环节`、`根因`、`预防动作`、`验证方式`、`晋升落点`、`状态`；单条 <= 8 行，严禁写入真实用户名或个人目录路径）。
- **L3（流程自主晋升）**：满足以下任一条件时，主控无需用户再次提醒即可自主发起治理升级：
  - 高危安全边界或数据/磁盘空间风险（如系统盘 `%TEMP%` 防爆与联接禁忌）；
  - 跨平台（Windows vs Linux）真机必然复现的构建/测试陷阱；
  - 同类 L2 经验在实战中命中 **>= 2 次**。
  晋升去向按收益递增依次为：① 下轮任务书「规定环境 / Owned Paths / 验收清单」默认强制条款 → ② 更新 `.agents/skills/paseo-orchestrator/SKILL.md` 与 `AGENTS.md` → ③ 单开防呆脚本或门禁测试治理 Issue/PR（彻底变成机器检查）。

### 3. 派发前 10 秒预检与容量剪枝机制
- **派发前必扫**：起草任何新 Issue 任务书前，快速扫读 `docs/ops/LESSONS.md`「活跃规则速查表」，将命中本次任务特征的规则（如 WSL shims 绕过、AOCI stdio `.mjs`、双平台实测命令）直接内嵌到任务书对应小节。
- **定期剪枝（控噪防膨胀）**：`docs/ops/LESSONS.md` 的活跃速查表严格保持 **<= 25 条**。一旦某条规则已被脚本（如 `setup-worktree.ps1`、`paseo-run-pi.ps1`、`verify-commits-and-privacy.ps1`）或自动化测试完全机器防呆，即将其从活跃表移至「已自动化防呆归档区」，保证主控上下文长期精干高信噪比。

### 4. 自主演化安全护栏
- 所有自优化修改一律经由独立 Worktree 与 PR 落地，跑通 `scripts/check-all.ps1` 与 AOCI 校验后合入；
- 严禁以“优化效率”为由削弱 `AGENTS.md` 第一章安全红线（如禁止跳过 `--no-verify` 校验、禁止静默改写本机全局配置、禁止修改 `AGENTS.md` 内 `<!-- aoci:begin -->`~`<!-- aoci:end -->` 托管区块）。
