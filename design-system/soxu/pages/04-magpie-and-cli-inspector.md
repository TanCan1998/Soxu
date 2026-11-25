# 04 · Magpie 双态路由诊断中心与原生 CLI 驱动检视器规范

> **继承主规范**：[`../MASTER.md`](../MASTER.md)  
> **目标模块**：`crates/soxu-magpie`、`crates/soxu-drivers`、`crates/soxu-desktop`、`apps/mobile-web`  
> **架构决策依据**：ADR-002（100% 原生 CLI 复用，不走 ACP） + ADR-003（Magpie 自动协同与零依赖原生降级）

---

## 一、设计目标：透明可观测、零配置心智负担、零全局污染

用户可能安装了 **Magpie** 统一接管本机所有 Agent 的模型网关、MCP 与 Skills，也可能在另一台干净机器上只装了 `claude` 或 `codex` 原生 CLI。本面板（可通过顶部状态胶囊 `[Magpie 协同]` / `[独立原生]` 或设置页打开）必须做到：
1. **一眼看清当前路由拓扑**：模型请求走的是本地 Magpie 网关还是原生直连？当前 Worktree 的 AOCI / CodeGraph 是怎么挂载进去的？
2. **降级态绝不报错焦虑**：未检测到 Magpie 时，界面展示沉稳的「独立原生模式（Standalone Native）」而非红色故障告警。
3. **严格只读守护声明**：在 UI 显著位置给用户吃定心丸——Soxu 只读解析 `~/.config/magpie` 与原生配置（自动跳过 `redact.key` 等敏感密钥文件），**绝不篡改任何全局 CLI 配置文件**。

---

## 二、顶部环境双态横幅（Dual-Mode Topology Banner）

### 状态 A：`MagpieManaged`（检测到 `~/.config/magpie` 协同模式）
- **容器**：背景 `#171717`，描边 `1px solid rgba(25, 195, 125, 0.32)`，圆角 `14px`，左侧配翠绿 `#19C37D` 在线脉冲点。
- **四宫格实时指标卡（底色 `#121212`）**：
  1. **本地模型网关**：`http://127.0.0.1:3425/v1` · 状态 `在线 (延迟 2ms)` · 已聚合路由组 `7 个`（如 `group/gpt-6.2-sol`、`group/claude-opus-5.5` 等）；若某模型在 Magpie Provider 中处于 `not picked`（如 `gemini-4-carbon` 仅在 `agy` 原生可用），在模型旁标注 `[仅 agy 原生直连]` 提示，避免跨 CLI 误调 404！
  2. **MCP Router 拓扑**：清晰拆分两栏标签：
     - `Shared 共享网关池`（全局长驻复用，零重复冷启动）；
     - `Direct 工作区直连`（目录敏感型服务，如 `aoci`、`codegraph`——面板明确展示：**「新建 Git Worktree 时，Soxu 已通过启动环境钩子自动补齐当前工作树索引上下文」**）。
  3. **Skill Router 拓扑**：展示 `Routed 按需路由技能` 与 `Direct 原生挂载技能` 数量及搜索入口。
  4. **零破坏安全锁**：显示锁形 SVG 图标 + `只读协同 · 零全局覆写`。

### 状态 B：`StandaloneNative`（未安装 Magpie 自动降级模式）
- **容器**：背景 `#171717`，描边 `1px solid #262626`，配柔和中性/琥珀信息徽标 `[独立原生模式 · Standalone Native]`。
- **说明文案**：「未在 `~/.config/magpie` 检测到 Magpie 配置。Soxu 已自动回退至各 Agent CLI 原生目录只读扫描，桌面多工作区编排与手机遥控功能 100% 完整可用。」
- **操作入口**：提供 `[重新探测环境]` 按钮（安装或启动 Magpie 后无需重启桌面端即可热切换至协同态）。

---

## 三、7 大原生 Agent CLI 驱动健康度矩阵（Native Drivers Grid）

以响应式双列卡片网格（宽屏 2 列，窄屏 1 列，卡片底 `#171717`，圆角 `12px`）呈现 `soxu-drivers::NativeCliKind` 支持的全部 7 类驱动：

| 驱动标识 (`NativeCliKind`) | 默认可执行文件名 | 卡片核心展示字段 | 能力徽章（Capability Pills） |
| :--- | :--- | :--- | :--- |
| `Claude` | `claude` | 可执行路径（`~` 脱敏）、版本号、认证状态 | `[Stream-JSON]` `[Session Resume]` `[Native History]` |
| `Codex` | `codex` | 可执行路径、版本号、`fast_mode` 支持态 | `[JSONL Events]` `[Worktree Ready]` `[Native History]` |
| `Pi` | `pi` | 可执行路径、版本号、已加载扩展摘要 | `[RPC / JSON Stream]` `[Steer Ready]` `[Native History]` |
| `OpenCode` | `opencode` | 可执行路径、版本号、默认 Mode | `[Non-Interactive & PTY]` `[Native History]` |
| `Antigravity` | `agy` | 可执行路径（映射 `agy`）、版本号、`--effort` 档位 | `[Stream-JSON]` `[Effort Control]` `[Conversation ID]` |
| `Copilot` | `copilot` | 可执行路径、GitHub 登录态 | `[CLI / PTY]` |
| `Qwen` | `qwen` | 可执行路径、版本号 | `[Stream / PTY]` |

- **未安装 CLI 的降级展示**：若某 CLI 在本机 `PATH` 中未安装，卡片自动沉入列表末尾，采用 `opacity: 0.55` 低调折叠，显示 `[未安装 · 自动从新建选择器隐藏]`，绝不阻断其他已安装 CLI 的使用。

---

## 四、本地原生历史会话发现与无损导入抽屉（Import Native Sessions）

用户在使用 Soxu 前，往往已经在终端里用 `claude`、`codex`、`pi` 或 `agy` 积累了大量本地会话记录：

1. **触发入口**：左栏底部 `[导入本机 CLI 历史会话]` 或驱动卡片上的 `[浏览本地历史 (N)]`。
2. **只读索引预览抽屉**：
   - 按项目工作目录自动匹配归组，展示每条原生会话的：**来源 CLI 徽标**、**首轮提示词提炼标题**（自动剥离系统路由帧与冗长引用前缀）、**最后活跃时间**、**轮次数**。
   - **双模式打开**：
     - **「原位续接 (Resume Native ID)」**：直接携带原 CLI 的 `--resume / --conversation <id>` 在对应工作目录拉起原生续接，保留 100% 原生上下文；
     - **「分叉至新 Worktree (Fork Context)」**：提取关键历史摘要注入全新隔离 Worktree 会话，保护原会话记录不被改动。

---

## 五、7 大 CLI 线性矢量徽标规范与 Worktree 启动环境透视器

### 5.1 CLI 官方原始品牌矢量徽标纪律
所有 CLI 驱动图标在会话标签、驱动卡片、移交选单与消息头中一律以 `14×14px`–`20×20px` **各自原本的官方品牌矢量 SVG 渲染**，**严禁使用本项目自绘替代线框、外链位图或 Emoji**：
- **Claude（`#D97757` 星芒）/ Codex（蓝紫渐变云朵终端）/ Pi（双阶像素几何标）/ Agy（Antigravity `#4388F0` 升空拱翼）/ OpenCode（双层嵌套方框）/ Copilot（GitHub Copilot 护目镜）/ Qwen（`#615CED` 六角几何编织标）** 均直接复用上游官方原版矢量路径，支持深浅主题高对比显示与 `monochrome` 单色跟随 `currentColor`。

### 5.2 Worktree 启动环境变量透视器（Launch Env Inspector）
为消除多 Worktree 并发或 Magpie `Direct` MCP 挂载黑盒感，驱动卡片详情提供 **「启动环境透视（只读）」** 折叠面板：
1. **自动展示项**：
   - `CARGO_TARGET_DIR = ../.cargo-target/soxu`（标记 `[共享构建缓存命中]`）；
   - `MAGPIE_GATEWAY = http://127.0.0.1:3425/v1`（`StandaloneNative` 下显示 `未启用，使用 CLI 原生凭据`）；
   - `SOXU_MCP_DIRECT = aoci, codegraph`（标示已按当前 Worktree 目录绑定索引根）；
   - `RuntimeExtensionHook` 槽位挂载列表（当前为空时显示 `NoopExtensionHost · 预留扩展点未激活`）。
2. **强制脱敏规则**：任何含 `KEY`、`TOKEN`、`SECRET`、`COOKIE`、`PASSWORD` 的环境变量名称仅展示键名并将值强制掩码为 `••••••••`，**严禁读取或打印 `redact.key` 等凭据文件内容**。
3. **无 Magpie 环境中性降级检查清单**：确认 7 项原生探测目录均采用 `~` 前缀展示且带有「当前仅扫描元数据，写入已锁定关闭」绿锁标识。

---

## 六、组件像素解剖与边界状态处理规则

| 界面区块 | 尺寸与间距 | 视觉表现令牌 | 异常与边界降级 |
| :--- | :--- | :--- | :--- |
| **Magpie 拓扑横幅** | `p(16)`, `rounded(14)` | `MagpieManaged` 用翠绿柔描边；`StandaloneNative` 用 `#262626` | 网关 `127.0.0.1:3425` 端口超时时，顶部状态点改为琥珀并给出 `[重启或检查 Magpie]` 操作链接 |
| **四宫格指标卡** | `min-h(78)`, `p(12)`, `rounded(10)` | `#121212` 内嵌底 + `1px solid #1A1A1A` | 当 `agi`/`pi` 等存在 `not picked` 模型时，展示模型列表微标签 `[原生直连]` |
| **CLI 驱动健康卡** | 双列 `gap(12)`, `p(14)`, `rounded(12)` | `#171717` 底 + `#262626` 边 | 执行 `claude --version` 超时 `> 2.5s` 时显示 `[探测超时 · 可重试]` 而不阻塞其余驱动渲染 |
| **历史会话导入抽屉** | 宽 `480px` (桌面) / `76dvh` (移动) | `#171717` 底 + 行高 `52px` 列表 | 解析坏损 JSONL 行自动跳过并计入尾部提示 `已忽略 1 条异常格式历史` |
