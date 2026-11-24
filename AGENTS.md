# AGENTS.md · Soxu（索序）多 Agent 协同研发工程宪法

本文档是所有参与 Soxu 仓库研发、审查与维护的 AI Agent（包括主控调度者、内置子代理以及 Paseo Worktree 编码 Agent）的最高行为准则。

---

## 一、安全红线（最高优先级）

1. **高危与破坏性操作停机报批**：
   - 在执行任何可能破坏仓库外文件、删除持久数据、修改系统全局配置、删除远端分支/仓库、重置未合并提交或安装未指定软件的命令（如 `Remove-Item -Recurse`、`rmdir`、`del`、`git push --force`、`gh repo delete`、`Set-ExecutionPolicy`）之前：**必须立即停止执行**，向用户清晰说明该命令将做什么、影响范围与回退方式，**等待用户明确回复「同意」后方可调用工具执行**。
2. **用户本机生态零破坏（Magpie & 原生 CLI 保护）**：
   - **严禁直接覆写或篡改**用户主目录下的全局 Agent 配置文件（如 `~/.claude/settings.json`、`~/.codex/config.toml`、`~/.pi/agent/settings.json`、`~/.config/opencode/opencode.json`、`~/.config/magpie/*`），以免破坏 Magpie 的 `applied.json` 接管状态与网关路由。
   - 测试 Magpie 与原生 CLI 发现/适配逻辑时，**一律使用临时夹具目录（Fixture TempDir）**，严禁拿真实用户配置目录做写操作测试。
3. **隐私与密钥零泄露（禁止绕过 Git Hooks）**：
   - 严禁将任何 API Key、Token、私钥、`.env`、Magpie/Paseo 本地凭据文件（`redact.key`、`local-credential`、`daemon-keypair.json`）或**硬编码的个人计算机用户目录路径**（如 `C:\Users\<真实用户名>`、`/Users/<真实用户名>/`）写入代码、测试或文档。
   - **严禁使用 `git commit --no-verify` 或 `-n` 绕过 `.githooks/pre-commit` 与 `.githooks/commit-msg` 检查。**

---

## 二、语言、终端与工具约定

1. **统一简体中文**：思考、回复、架构说明、PR 描述、Issue 内容、Commit 描述正文及代码关键注释一律使用**简体中文**。
2. **终端环境**：Windows 环境统一使用 **PowerShell 7（`pwsh`）**，不使用 Windows PowerShell 5.1。
3. **Git 与 GitHub 工具链路由**：
   - 本地版本控制用 `git`；GitHub 平台操作（Issue、PR、Milestone、Release、Actions 日志）一律使用已登录的 `gh`（配合 `--json <fields>` 精确读取，避免灌入大段噪音）；
   - **禁止**使用 `curl` / `Invoke-RestMethod` 直接请求 `api.github.com` 或 `raw.githubusercontent.com`。
4. **最新稳定版优先铁律（禁止凭记忆硬编码旧版本号）**：
   - 本项目启用 Rust `edition = "2024"` 与 Cargo `resolver = "3"`。
   - 新增任何第三方依赖前，**必须实时通过包管理器查询当前最新稳定版**：
     - Rust 依赖：运行 `cargo info -q --registry crates-io <crate>` 或 `cargo add <crate>`；
     - 前端依赖：在 `apps/mobile-web` 下运行 `pnpm add <pkg>`；
     - GitHub Actions：通过 `gh release view -R <owner/repo>` 核实最新大版本 tag。
   - 仅当最新稳定版经实测存在编译错误、平台缺陷或与 `gpui` 传递依赖冲突时，才允许回退至最新兼容次版本，并必须在 `Cargo.toml` / `package.json` 旁或 PR 说明中记录原因。

---

## 三、架构红线与多 Agent 并行隔离规范

1. **核心技术路线不可偏离**：
   - **纯原生 CLI 复用**：通过 `soxu-drivers` 直接驱动用户本机已装原生 CLI（`claude`、`codex`、`pi`、`opencode`、`agy`、`copilot`、`qwen`），**当前阶段不引入 ACP（Agent Client Protocol）**。
   - **Magpie 双态自适应**：通过 `soxu-magpie` 自动识别本机是否安装 Magpie（感知模型网关、`shared`/`direct` MCP Router、`routed`/`direct` Skill Router）；未安装时必须无缝降级至原生目录扫描，确保 100% 独立可用。
   - **UI/UX 设计同源**：桌面端（`soxu-desktop`，GPUI + `gpui-component`）与移动端（`apps/mobile-web`，Daemon 内嵌响应式 PWA）任何界面开发必须先读取并严格遵循 `design-system/soxu/MASTER.md`、`tokens.json` 及 `design-system/soxu/pages/` 下的分页面规范。
2. **Paseo Worktree 隔离与路径白名单（Owned Paths）**：
   - 并行开发任务一律在独立 Git Worktree（`feat/<issue>-<slug>` 等分支）中进行。
   - 每个子任务必须严格遵守 Issue 中声明的 **Owned Paths 写入白名单**。未经主控协调，具体业务 Crate 的开发任务**禁止擅自修改根 `Cargo.toml`、`crates/soxu-protocol` 或其他并行分支负责的 Crate**（跨模块公共契约变更须先走独立契约 PR 合入 `main`）。
3. **`.gitignore` 实时同步义务**：
   - 任何时候引入新工具、新缓存目录、新构建产物、测试快照临时目录或本地索引文件时，**必须在同一个提交/PR 中同步更新根 `.gitignore`**，保证提交前后 `git status` 干净无脏文件。

---

## 四、Git Emoji 提交规范与 PR 门禁

1. **Commit Message 与 PR 标题格式**：
   ```text
   <emoji> <type>(<scope>): <中文简明描述（4~72 字）>
   ```
   标准映射表：
   - `🎉 init`: 仓库初始化
   - `✨ feat`: 新增功能
   - `🐛 fix`: 修复缺陷
   - `♻️ refactor`: 代码重构（外部行为不变）
   - `⚡ perf`: 性能优化
   - `💄 ui`: UI 布局、交互或设计令牌调整
   - `🎨 style`: 代码格式化或纯样式清理
   - `📝 docs`: 文档与运维记录更新
   - `✅ test`: 新增或完善测试
   - `🔧 chore`: 构建脚本、依赖升级、`.gitignore` 维护
   - `👷 ci`: GitHub Actions 与 Git Hooks 调整
   - `🔒️ security`: 安全加固与防隐私泄露规则
2. **提交与提 PR 前必跑门禁**：
   ```powershell
   pwsh -NoProfile -File scripts/check-all.ps1
   ```

---

## 五、随时中断与新会话 30 秒无损接手协议（Stateless Handoff）

本项目支持在任意时刻终止当前 AI 对话，并由新对话无损接手继续推进：

1. **新对话接手标准启动顺序（30 秒恢复）**：
   1. 阅读本文件 `AGENTS.md`；
   2. 阅读 [`docs/ops/PROGRESS.md`](docs/ops/PROGRESS.md) 与 [`docs/ops/DECISIONS.md`](docs/ops/DECISIONS.md)；
   3. 执行只读状态盘点：`git status -sb`、`gh issue list --limit 20 --json number,title,milestone,assignees`、`gh pr list --json number,title,headRefName` 以及 Paseo 工作区列表（`paseo project ls` / `paseo workspace ls`）。
2. **离开/收口写入义务**：
   - 每完成一个 Issue、合入一个 PR 或切换里程碑时，主控或负责合入的 Agent 必须同步更新 `docs/ops/PROGRESS.md` 中的「当前阶段」、「进行中工作区/PR」与「下一步待执行队列」。
