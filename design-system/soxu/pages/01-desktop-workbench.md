# 01 · GPUI 桌面四栏工作台与键盘导航规范 (`soxu-desktop`)

> **继承主规范**：[`../MASTER.md`](../MASTER.md)  
> **目标模块**：`crates/soxu-desktop`（M4 里程碑实现）  
> **参考对标**：`padu`（GPUI 焦点栈、面板极值与零 I/O 渲染） + `paseo`（项目/Worktree 树与状态桶排序） + `codeg`（会话标签页与右侧审阅抽屉）

---

## 一、四区自适应骨架（Workbench Spatial Grid）

```text
┌──────────────────────────────────────────────────────────────────────────────────────────┐
│ [● ○ ○]  Soxu · 索序      [⌘K 搜索会话 / 文件 / 命令 / @agent...]       [Magpie 协同] [📱] │ 38px Titlebar
├───────────────────┬────────────────────────────────────────────┬─────────────────────────┤
│ 项目与工作区树     │ [π M1 协议 ●] [cx 驱动 ⏳1] [+ 新建会话]     │ [Diff 3] [文件] [脚本]   │ 36px Tab Header
│ (#0D0D0D)         ├────────────────────────────────────────────┤ (#0D0D0D)               │
│                   │ ↳ 子代理轨: [agy 索引校验 ✓] [pi 单测 ⟳]    │                         │ 28px Subagent Bar
│ ▾ Soxu (git)      ├────────────────────────────────────────────┤ crates/soxu-core/lib.rs │
│   ● main (local)  │                                            │ @@ -12,4 +12,9 @@       │
│   ⏳ feat/m1-rpc   │        主对话画布 (#000000 Pure Black)      │ + pub struct SessionId  │
│   ✓ ui/m0.5       │          居中阅读列 max-width: 760px        │                         │
│                   │                                            │ ─────────────────────── │
│ ▾ Paseo Scripts   │        ┌──────────────────────────┐        │ ▸ Workspace Scripts     │
│   ▶ daemon :7698  │        │ 三态悬浮输入舱 (#212121)  │        │   ● mobile-dev :5173    │
│   ■ check-all     │        │     max-width: 720px     │        │   ○ check-all           │
├───────────────────┴────────┴──────────────────────────┴────────┴─────────────────────────┤
│ ● Daemon 127.0.0.1:7698 (seq #1056) │ Magpie :3425 (7 models) │ git: ui/3-m0.5* │ 2 台设备 │ 26px Statusbar
└──────────────────────────────────────────────────────────────────────────────────────────┘
```

### 1.1 面板尺寸、拖拽极值与渐进收缩策略（Progressive Shrink）

| 区域 | 默认宽度/高度 | 最小极值 | 最大极值 | 颜色令牌 | 窗口收缩响应策略 |
| :--- | :---: | :---: | :---: | :--- | :--- |
| **顶部自定义标题栏** | 高 `38px` | 固定 | 固定 | `--soxu-surface-sidebar` (`#0D0D0D`) | Windows 保留右侧原生最小化/最大化/关闭三键命中区（宽 `138px`）；macOS 左侧预留 `76px` 红黄绿交通灯内边距 |
| **左栏：项目与工作区轨** | `260px` | `224px` | `420px` | `--soxu-surface-sidebar` (`#0D0D0D`) | `Ctrl/Cmd + B` 切换折叠；窗口总宽 `< 900px` 时自动折叠为浮动抽屉 |
| **中栏：多 Agent 会话画布** | 自适应 `flex: 1` | **`380px`** | 无上限（正文限宽 `760px`） | `--soxu-surface-canvas` (`#000000`) | 最高保留优先级；拖拽左右分隔条时不得挤压中栏低于 `380px` |
| **右栏：上下文与审阅栏** | `380px` | `280px` | `720px` | `--soxu-surface-sidebar` (`#0D0D0D`) | `Ctrl/Cmd + J` 切换显隐；窗口总宽 `< 1160px` 时优先自动收起右栏以保卫中栏阅读宽度 |
| **底部全局状态横梁** | 高 `26px` | 固定 | 固定 | `--soxu-surface-sidebar` (`#0D0D0D`) | 顶边 `1px` `--soxu-border-hairline` (`#1A1A1A`)，字号 `sp(11.5)` + `tabular-nums` |

- **分隔条（Resize Handle）交互**：静态为 `1px` `#1A1A1A` 细线，但鼠标命中热区向两侧各扩展 `4px`（总计 **`9px` 热区**）；悬停超过 `120ms` 或拖拽中渐变为 `2px` `--soxu-semantic-running` (`#3B82F6`)。

---

## 二、左栏：项目、Git Worktree 与注意力优先级状态桶

### 2.1 状态桶聚合与排序规则（Workspace State Buckets）
每个工作区（`local` 主干或 `worktree` 隔离分支）根据其名下所有活跃 Agent 与受管脚本实时计算单一最高优先级状态桶，并在项目节点折叠时向上冒泡：

| 优先级 | 状态桶枚举 | 视觉指示器（图标 + 色彩双编码） | 触发条件 |
| :---: | :--- | :--- | :--- |
| **P0 (最高)** | `needs_input` | 琥珀实心呼吸圆点 `#F59E0B` + 右侧胶囊数字徽章 `[待批 2]` | 存在待人工确认的权限审批卡或 `ask_question` 提问卡 |
| **P1** | `failed` | 绯红感叹号徽章 `#EF4444` + `[失败]` | 存在**未读**的回合异常退出或脚本非零崩溃（用户点击查看后自动降级退出 `failed`，即使历史状态仍记录错误） |
| **P2** | `running` | 电光蓝旋转脉冲环 `#3B82F6`（`≤ 30 Hz`）+ 耗时秒表 `01:24` | 主代理或委派子代理正在执行流式回合，或后台任务 `runningCount > 0` |
| **P3** | `attention` | 翠绿实心小圆点 `#22C55E` | 后台回合刚刚顺利结束但用户尚未切换到该会话阅读结果 |
| **P4 (最低)** | `done` / `idle` | 中性灰分支图标 `#8E8E8E`（无额外徽章） | 所有会话空闲且已读 |

### 2.2 Worktree 节点行信息密度与右键操作
- **单行高度**：`32px`（紧凑高密度模式），圆角 `8px`（`--soxu-radius-sm`），左右内边距 `10px`。
- **行结构**：`[隔离类型图标: 文件夹(local) / 分叉树(worktree)]` + `分支或工作区名（超出单行省略）` + `[变更统计微标 +18 -4]` + `[状态桶徽章]`。
- **新建 Worktree 抽屉**：点击项目名右侧 `+` 号，支持三种模式（对齐 `paseo` 契约）：
  1. `Branch-off`（默认）：基于 `origin/main` 或本地 `main` 创建新分支 `feat/<issue>-<slug>`，分支名输入实时按 `branch-slug` 规则归一化预览；
  2. `Checkout Branch`：检出已有本地/远端分支；
  3. `Checkout PR`：输入 PR 编号（如 `#12`）直接拉取审阅工作树。
- **后台工作区折叠区**：`background: true` 的自动化工作区默认归入列表底部的「后台运行工作区 (N)」折叠抽屉，不污染主视线。

---

## 三、中栏顶区：多 Agent 会话页签与子代理轨道（Subagents Track）

1. **主会话标签栏（Session Tab Strip，高 `36px`）**：
   - 每个标签显示：`[原生 CLI 专属矢量徽标: Claude / Codex / Pi / OpenCode / Agy / Copilot / Qwen]` + `会话标题` + `[状态点]` + `关闭键 ×（悬停显现）`。
   - **草稿保留不变量**：每个工作区最多保留 1 个未发送的本地草稿页签（斜体标题），断网或切换工作区绝不丢失未发送文本（`250ms` 防抖 + `generation` 代次号防乱序回写）。
2. **子代理委派轨道（Subagents Track，高 `28px`，仅当存在子代理时展开）**：
   - 当主 Agent 派生子代理时，在标签栏下方平滑展开一条 `#0D0D0D` 细轨，横向排列子代理胶囊：`[↳ research · 索引扫描 ✓ 12s]`、`[↳ codex · 协议单测 ⟳ 00:18]`。
   - 点击子代理胶囊可内联抽屉预览其流式日志；点击胶囊右侧 **「独立成页签 (Detach)」** 图标，可将该子代理提升为当前工作区的独立标签页继续追加人工提示，同时保留其父子溯源关系标签。

---

## 四、右栏：四合一上下文与审阅检视器（Context Inspector）

右栏顶部提供 4 个固定语义页签（支持拖拽排序，复用同一类型页签避免重复堆积）：

1. **`Diff`（实时工作树差异审阅）**：
   - 顶部工具栏：基准切换（`Uncommitted` vs `vs baseRef(origin/main)` vs `指定 Checkpoint`）、视图切换（Unified 单栏 / Split 双栏，宽 `< 480px` 时强制 Unified）、`/` 键快速过滤文件路径。
   - 支持一键打开 **Git 提交模态窗**：自动调用选定的轻量模型（如 `deepseek-v4.1-flash`）按仓库 `<emoji> <type>(<scope>): <中文描述>` 规范生成 Commit Message 草稿，并在提交前实时显示 `.githooks` 门禁预检状态。
2. **`Files`（零阻塞工作区文件树）**：
   - 目录遍历严格在后台线程执行，按 `token + workspace_path` 双重校验回填快照；点击转录区中的文件链接（如 `crates/soxu-protocol/src/lib.rs:42`）采用纯词法路径归一化（防止越界逃逸），自动展开右栏并高亮目标行。
3. **`Scripts`（`paseo.json` / 工作区受管脚本面板）**：
   - 列出 `daemon-dev`、`desktop-dev`、`mobile-dev`、`check-all` 等配置脚本，展示生命周期状态（`Running` / `Stopped` / `Exited(1)`）、监听端口徽章（如 `:7698`、`:5173` 一键打开预览）及启停控制键。
4. **`Terminal`（受控多窗格 PTY 终端）**：
   - 底色 `#050505`，支持水平/垂直分屏；Windows/Linux 下 `Ctrl+Shift+C` 复制、`Ctrl+C` 保留给进程 `SIGINT` 中断信号；支持断线后 `live / visible-snapshot` 恢复。

---

## 五、GPUI 键盘焦点上下文栈与全局快捷键矩阵

### 5.1 焦点上下文优先级栈（Focus Context Hierarchy）
为彻底消灭「按 `Esc` 关弹窗却误停后台 Agent」或「弹窗刚打开打字落入底层输入框」的经典桌面端 Bug，GPUI 键位绑定必须严格按以下从内到外的作用域隔离（内层处理后调用 `cx.stop_propagation()`）：

```text
ModalDialog (延迟 2 帧安全聚焦)
  ＞ CommandPalette ("Cmd+K", 输入非空时 Esc 先清空输入，再按关闭面板)
    ＞ AutocompletePopup ("@agent" / "/skill" / "#file"，必须在 Input 之后注册以接管上下键与 Enter)
      ＞ InlineRename ("SessionRename > TextInput"，Esc 仅取消重命名，绝不触发窗口级 Stop)
        ＞ ReviewDiff / ReviewDiffTree ("/" 聚焦过滤，方向键按树深度展开折叠)
          ＞ ComposerInput (Enter / Shift+Enter / Ctrl+Enter / Alt+Enter)
            ＞ WorkbenchGlobal (全局面板开关与会话切换)
```

### 5.2 核心快捷键标准表

| 功能动作 | Windows / Linux | macOS | 焦点作用域 |
| :--- | :--- | :--- | :--- |
| 打开全局命令与会话搜索面板 | `Ctrl + K` | `Cmd + K` | `WorkbenchGlobal` |
| 新建当前工作区 Agent 会话 | `Ctrl + N` | `Cmd + N` | `WorkbenchGlobal` |
| 新建隔离 Git Worktree 会话 | `Ctrl + Shift + N` | `Cmd + Shift + N` | `WorkbenchGlobal` |
| 折叠 / 展开左侧项目工作区栏 | `Ctrl + B` | `Cmd + B` | `WorkbenchGlobal` |
| 折叠 / 展开右侧上下文审阅栏 | `Ctrl + J` | `Cmd + J` | `WorkbenchGlobal` |
| 切换上/下一个活跃会话标签 | `Ctrl + PageUp / PageDown` | `Cmd + Shift + [ / ]` | `WorkbenchGlobal` |
| 快速跳至首个 `needs_input` 待审批项 | `Ctrl + Shift + A` | `Cmd + Shift + A` | `WorkbenchGlobal` |
| 打开手机扫码遥控配对浮层 | `Ctrl + Shift + M` | `Cmd + Shift + M` | `WorkbenchGlobal` |
| 批准当前聚焦权限审批卡（允许一次） | `Alt + Y` | `Cmd + Enter` (审批卡聚焦时) | `PermissionCard` |
| 拒绝当前聚焦权限审批卡并聚焦反馈框 | `Alt + N` | `Cmd + Backspace` | `PermissionCard` |
