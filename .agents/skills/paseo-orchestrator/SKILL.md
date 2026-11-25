---
name: paseo-orchestrator
description: 跨 AGY 桌面端与 Codex 桌面端共享的 Soxu Paseo 多工作树与代理编排规范。在任一桌面端接手 Soxu 开发、盘点 Paseo 工作区、创建隔离 Worktree、启动/调控子代理或在 AGY 与 Codex 之间切换控制权时使用。
---

# Soxu 跨桌面端 Paseo 协同编排指南

本技能随仓库共享于 `.agents/skills/paseo-orchestrator/SKILL.md`，**Google Antigravity（AGY 桌面端）** 与 **OpenAI Codex 桌面端** 均会原生自动发现。

---

## 一、统一控制入口（严禁裸调 `paseo`）

由于本机通过 Scoop 安装的 Paseo CLI 未在全局 PATH 暴露 shim，且桌面端未打开时本地守护进程可能处于休眠状态，**双端一律通过仓库包装脚本调用 Paseo**：

```powershell
pwsh -NoProfile -File scripts/paseo-ctl.ps1 <Paseo 参数...>
```

脚本特性：
1. 自动探测 `$env:PASEO_CLI`、PATH、Scoop 默认路径及常见安装目录中的 `paseo.cmd`；
2. 对 `project`、`workspace`、`ls`、`run`、`send`、`wait`、`logs`、`stop`、`rm`、`script`、`host` 等需要本地服务的命令，自动检查 `status --json` 并在后台休眠时自动执行 `start`；
3. 状态提示仅输出至 stderr，**stdout 始终保持纯净**，可直接管道连接 `| ConvertFrom-Json`。

---

## 二、AGY 桌面端与 Codex 桌面端「隔离与共享」矩阵

| 维度 | 类别 | 具体载体与边界约束 |
| :--- | :--- | :--- |
| **工程规范与进度认知** | 共享 | `AGENTS.md`、`docs/ops/PROGRESS.md`、`docs/ops/DECISIONS.md`、`aoci.txt` + `.aoci/`、`.agents/skills/`（全仓 Git 版本化，双端读取同一份事实来源） |
| **工具链与构建缓存** | 共享 | `mise.toml`、`paseo.json`、`Cargo.toml` 与上级共享缓存 `../.cargo-target/soxu`（双端均启用工作区 `scripts/check-all.ps1` 门禁） |
| **Paseo 运行状态池** | 共享 | 本机 Paseo 守护进程（`127.0.0.1:6767`）、项目 `Soxu`（`prj_24ffa03410264684`）、主控工作台（`wks_21203a9ef0f5e2e5`）、全部隔离 Worktree 与活跃子代理会话 |
| **宿主项目注册与全局配置** | 隔离 | **AGY**：项目注册在 `~/.gemini/config/projects/`，MCP 在 `~/.gemini/config/mcp_config.json`；**Codex**：项目注册在 `~/.codex/state_5.sqlite`，仓库级覆盖在已忽略的 `.codex/config.toml`。双端严禁相互篡改对方宿主配置或 `~/.claude` / `~/.pi` / `~/.config/magpie` |
| **工作树写操作（并发边界）** | 隔离 | 主检出仓库目录（项目根路径）仅作**主控编排、审查与工程治理台（Control Plane）**。**严禁** AGY 与 Codex 同时在主检出目录直接并发修改业务源码；功能开发一律下沉至独立的 `--isolation worktree` 分支并严守 Issue `Owned Paths` 白名单 |

> 说明：新建的 Paseo 隔离 Worktree 会通过 `paseo.json` 中的 `worktree.setup` 自动运行 `scripts/setup-hooks.ps1`，确保每个隔离工作树继承 `.githooks` 隐私与 Git Emoji 提交检查。

---

## 三、核心 Paseo 编排命令速查

### 1. 盘点全局状态、项目、工作区与子代理
```powershell
pwsh -NoProfile -File scripts/paseo-ctl.ps1 status --json | ConvertFrom-Json
pwsh -NoProfile -File scripts/paseo-ctl.ps1 project ls --json | ConvertFrom-Json
pwsh -NoProfile -File scripts/paseo-ctl.ps1 workspace ls --json | ConvertFrom-Json
pwsh -NoProfile -File scripts/paseo-ctl.ps1 ls --json | ConvertFrom-Json
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

### 3. 在目标 Worktree 中启动编码代理
```powershell
pwsh -NoProfile -File scripts/paseo-ctl.ps1 run `
  --no-wait `
  --workspace <wks_id> `
  --provider codex `
  "请阅读 AGENTS.md 与对应 Issue 的 Owned Paths，完成实现并通过 scripts/check-all.ps1"
```
（当前本机 Paseo 已识别的原生驱动含 `codex`、`claude`、`pi`、`antigravity`、`copilot`。）

### 4. 实时干预（热纠偏 / 排队 / 打断）与日志观察
```powershell
pwsh -NoProfile -File scripts/paseo-ctl.ps1 send <agent_id> "请补充边界测试后重跑 check-all" --behavior steer
pwsh -NoProfile -File scripts/paseo-ctl.ps1 send <agent_id> "完成后检查 git status" --behavior queue
pwsh -NoProfile -File scripts/paseo-ctl.ps1 send <agent_id> "暂停当前方案并改用增量游标" --behavior interrupt
pwsh -NoProfile -File scripts/paseo-ctl.ps1 logs <agent_id> --tail 30
pwsh -NoProfile -File scripts/paseo-ctl.ps1 wait <agent_id>
```

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
   2. 阅读 `docs/ops/PROGRESS.md` 与 `docs/ops/DECISIONS.md`；
   3. 执行只读盘点与接管：
      ```powershell
      git status -sb
      gh issue list --limit 20 --json number,title,milestone,assignees
      gh pr list --json number,title,headRefName,state
      pwsh -NoProfile -File scripts/paseo-ctl.ps1 workspace ls --json
      pwsh -NoProfile -File scripts/paseo-ctl.ps1 ls --json
      ```
   4. 若有正在运行的 Paseo Agent，直接通过 `logs <agent_id> --tail 30` 检查进度，并用 `send <agent_id> ...` 继续调控，**无需推倒重来**。
