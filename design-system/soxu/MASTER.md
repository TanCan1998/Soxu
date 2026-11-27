# Soxu 全端设计系统主规范 · MASTER.md

> **唯一真相源状态**：M0.5 正式版 · 默认深色主题对标 **ChatGPT Pure Black（纯黑中性灰阶体系）**  
> **适用范围**：`crates/soxu-desktop`（GPUI 原生桌面工作台）与 `apps/mobile-web`（`soxu-daemon` 内嵌响应式移动遥控 PWA）  
> **品牌纪律**：`Soxu` 为专有品牌名称，全端一律保持原文 `Soxu` 不作中文翻译，且严禁在 `Soxu` 文字前加任何状态圆点或徽标。  
> **机器令牌源**：[`tokens.json`](tokens.json) → 由 `node scripts/sync-design-tokens.mjs` 编译产出 [`tokens.css`](tokens.css) 与 `apps/mobile-web/src/tokens.generated.css`

---

## 一、设计哲学与三大参考库精髓融合

Soxu 服务于高频并行操控多个本地编码 Agent（`claude`、`codex`、`pi`、`opencode`、`agy`、`copilot`、`qwen`）的开发者。界面必须在每天 8–12 小时的密集代码阅读、实时流式监控与手机跨屏审批中保持**零视觉疲劳、零信息迷失、零延迟卡顿**：

1. **Pure Black 光学克制（对标 ChatGPT 纯黑主题）**：
   - 摒弃传统开发者工具常见的「藏青/蓝紫偏色深灰（如 `#0F172A` / `#1E293B`）」，全系表面采用 **`chroma = 0` 的零色偏纯正中性灰阶**。
   - 主对话画布与移动端背景直取 **OLED 纯黑 `#000000`**，侧栏取 `#0D0D0D`，卡片取 `#171717`，用户消息气泡与输入舱取 ChatGPT 标志性的 `#212121`。
   - **纯黑环境下禁止依赖外阴影表达层级**：所有抬升卡片与浮动弹层必须通过 **表面亮度微阶差 + `1px` 内聚描边（`#262626` 常态 / `#383838` 浮层高光）** 塑造锐利边界。
2. **色彩仅为语义保留（Color Reserved for Meaning，吸收自 `padu`）**：
   - 结构框架、导航栏、按钮底色全部保持无彩色（纯黑/中性灰/纯白反色主按钮）。
   - 彩色仅在传达明确系统状态时出现：翠绿 `#19C37D`（Magpie 协同在线）、电光蓝 `#3B82F6`（Agent 运行中 / Steer 注入）、琥珀金 `#F59E0B`（待人工审批 `needs_input` / 原生降级提示）、绯红 `#EF4444`（破坏性高危命令拦截 / Interrupt 打断 / 未读错误）、紫罗兰 `#A855F7`（`@agent` 跨代理移交 / Git 轮次检查点）。
   - **铁律**：任何语义色出现时，**必须同时配对 SVG 语义图标或明确文字标签**，严禁仅靠颜色区分状态。
3. **多 Agent 监管与遥控同构（吸收自 `paseo` 与 `codeg`）**：
   - 工作区与会话列表严格遵循注意力优先级自动聚簇：**`needs_input`（待审批） > `failed`（未读失败） > `running`（运行中） > `attention`（新完成未读） > `done`（空闲）**。
   - 桌面端与移动端共享完全相同的语义令牌角色、三态投递模型（`Queue` / `Steer` / `Interrupt`）、权限审批动作与单调递增游标（`seq`）断线追平心智。

---

## 二、7 级表面海拔与 WCAG 无障碍对比度矩阵

### 2.1 表面海拔阶梯（Surface Elevation Stack）

| 层级序号 | 语义令牌角色 | CSS 变量 | Pure Black (Dark 默认) | Clean Neutral (Light) | 核心承载组件 |
| :---: | :--- | :--- | :---: | :---: | :--- |
| **L0** | `surface.inset` | `--soxu-surface-inset` | `#000000` (+ `#1A1A1A` 内边) | `#EBEBEB` | 代码行号槽、分段控制器底槽、凹陷输入井 |
| **L1** | `surface.canvas` | `--soxu-surface-canvas` | **`#000000`** | `#FFFFFF` | 主对话流画布、移动端主背景（Agent 消息直接排布于此，无气泡包裹） |
| **L2** | `surface.terminal` | `--soxu-surface-terminal` | `#050505` | `#F7F7F8` | 受控 PTY 终端面板、Bash 工具执行输出黑匣子 |
| **L3** | `surface.sidebar` | `--soxu-surface-sidebar` | **`#0D0D0D`** | `#F9F9F9` | 左侧项目/Worktree 导航树、右侧 Diff/脚本审阅栏、顶底 `28px` 状态横梁 |
| **L4** | `surface.card` | `--soxu-surface-card` | **`#171717`** | `#FFFFFF` | 工具调用折叠卡、权限审批卡、Magpie 路由诊断卡、设置分组面板 |
| **L5** | `surface.raised` / `composer` | `--soxu-surface-raised` | **`#212121`** | `#F4F4F4` | 用户消息右对齐胶囊、三态悬浮输入舱、活跃选中卡片 |
| **L6** | `surface.floating` | `--soxu-surface-floating` | **`#262626`** (+ `#383838` 边) | `#FFFFFF` | `Cmd+K` 命令面板、`@agent` / `/skill` 补全弹层、模型切换器、模态对话框 |
| **L6+** | `surface.control` | `--soxu-surface-control` | **`#2F2F2F`** | `#E8E8E8` | 输入舱内部模型/思考等级胶囊按钮、次级实心操作键 |

### 2.2 文本对比度实测验证（WCAG 2.1 AA ≥ 4.5:1 强制达标）

| 文本角色 | Dark 色值 | 对 `#000000` 对比度 | 对 `#171717` 对比度 | 对 `#212121` 对比度 | Light 色值 | 允许用途与禁令 |
| :--- | :---: | :---: | :---: | :---: | :---: | :--- |
| `text.primary` | `#ECECEC` | **17.8 : 1** (AAA) | **14.9 : 1** (AAA) | **13.2 : 1** (AAA) | `#0D0D0D` | 对话正文、标题、代码正文、按钮主文案 |
| `text.secondary` | `#B4B4B4` | **10.1 : 1** (AAA) | **8.5 : 1** (AAA) | **7.5 : 1** (AAA) | `#5D5D5D` | 工具调用摘要、路径前缀、辅助说明、未激活页签 |
| `text.tertiary` | `#8E8E8E` | **6.3 : 1** (AA) | **5.3 : 1** (AA) | **4.7 : 1** (AA) | `#757575` | 时间戳、Token 统计、快捷键提示、分组大写微标 |
| `text.ghost` | `#5E5E5E` | 3.2 : 1 | 2.7 : 1 | 2.4 : 1 | `#9B9B9B` | **严禁用于承载业务可读信息**；仅限输入框 Placeholder、已禁用控件与纯装饰斜杠 |
| `action.inverse` + `onInverse` | `#FFFFFF` 底 / `#000000` 字 | **21.0 : 1** (AAA) | — | — | `#0D0D0D` 底 / `#FFFFFF` 字 | 发送按钮、「允许一次」主审批按钮、确认配对主 CTA |

---

## 三、排版、间距、圆角与图标纪律

### 3.1 双尺度排版与开源双字体体系（GPUI `sp()` vs 定宽 `px()`）
- **全开源免费字体策略（SIL OFL 1.1）**：禁止依赖专有闭源付费字体。提供 5 套界面无衬线/中文字体预设（默认 `Inter + Noto Sans SC`，支持 `IBM Plex Sans`、`Plus Jakarta Sans`、`霞鹜文楷 LXGW WenKai Screen` 等）与 5 套代码等宽预设（默认 `JetBrains Mono`，支持 `Fira Code`、`Maple Mono`、`IBM Plex Mono`、`Cascadia Code`）。
- **独立 / 跟随双重模式**：设置界面支持单独设定「界面字体」与「代码块字体」，并提供 **「代码块字体跟随界面字体」** 开关（`--soxu-font-code-effective` 与 GPUI `effective_code_family()` 自动联动）。数字统计与计时器一律开启 `font-variant-numeric: tabular-nums`。
- **GPUI 缩放隔离铁律**：
  - 界面铬层（侧栏、页签、按钮、胶囊微标）统一使用 `sp(px_val)`（以 `14px = 1rem` 随用户 UI 缩放设置等比缩放）；
  - 内容表面（对话 Markdown、代码编辑器、Diff 行、终端网格）保持独立字号配置并以 `px()` 渲染，**严禁二次叠加 `sp()` 缩放**。
- **移动端输入防缩放铁律**：`apps/mobile-web` 中所有 `<textarea>` 与 `<input>` 字号强制 **`≥ 16px`（`--soxu-text-input-mobile`）**，防止 iOS Safari 聚焦时自动放大破坏视口布局；同时严禁设置 `user-scalable=no` 或禁锢系统 200% 无障碍字号缩放。

### 3.2 4/8px 空间韵律与单调嵌套圆角
- **唯一间距刻度**：仅允许使用 `2 / 4 / 6 / 8 / 12 / 16 / 20 / 24 / 32 / 48px`（禁止出现 `13px`、`15px`、`19px` 等魔术数值）。
- **阅读行宽上限（Reading Measure Ceiling）**：
  - 对话消息流（Transcript）：最大宽度 **`760px`** 并水平居中；
  - 悬浮三态输入舱（Composer）：最大宽度 **`720px`** 并水平居中；
  - 任何平板或超宽屏上单栏散文绝不超过 **`800px`**，严禁文字贴满宽屏左右边缘。
- **圆角单调递减嵌套法则**：外层容器圆角必须大于内层子元素（`R_outer = R_inner + padding` 的近似视觉收敛）：
  - 输入舱外框 `22px`（`composer`） → 内部快捷胶囊 `9999px`（`pill`）或附件缩略卡 `12px`（`md`）；
  - 审批/模态大卡 `18px`（`lg`） → 内部命令代码块 `12px`（`md`） → 徽章标签 `6px`（`xs`）。

### 3.3 矢量图标与状态微动效规范
- **严禁使用 Emoji 充当 UI 结构图标**（Git Commit 规范中的 Emoji 仅存在于 Git 历史，绝不用于界面导航、设置或工具栏图标）。
- 全端统一采用 **Lucide / 1.75px 线性描边 SVG 图标**，尺寸锁定三档：`14px`（胶囊内联/微标）、`16px`（桌面常规工具栏/树节点）、`20px`（移动端导航与主操作）。
- **无障碍语义**：伴随可见文字的装饰图标必须标记 `aria-hidden="true"`；纯图标按钮必须具备动态准确的 `aria-label`（如 `"停止当前回合 (Esc)"`）及 `≥ 3:1` 的邻接对比度。
- **交互反馈零位移**：悬停（`150ms`）与按压（`100ms`）仅改变背景/描边透明度（`interaction.hover` / `pressed`），**禁止使用会引起周围布局重排的尺寸改变**；全局遵守 `prefers-reduced-motion: reduce`。

---

## 四、双端工程与性能红线（M1–M6 编码必遵）

1. **GPUI 渲染线程零阻塞铁律（`soxu-desktop`）**：
   - `Render::render`、补全弹窗渲染、`Cmd+K` 面板渲染中**严禁执行任何文件系统 `stat`/遍历、SQLite 查询、Git 调用或子进程派生**；只允许读取内存不可变快照（优先通过 `Rc::ptr_eq` / 代次号 `generation` 判重）。
   - 流式增量文本与思考链向 UI 刷新的频率上限锁定 **`≤ 8.3 Hz`（约 120ms 合批一次，轮次结束瞬间立即 flush）**；运行态脉冲指示器时钟上限 **`≤ 30 Hz`**。
   - 对话消息区与大型 Diff（上限 `50,000` 行，单活动卡折叠预览上限 `400` 行并显示剩余隐藏行数）必须采用虚拟列表渲染。
   - Cargo 构建配置：根 `Cargo.toml` 在 M4 引入 GPUI 时须设置 `[profile.dev] opt-level = 1` 与 `[profile.dev.package."*"] opt-level = 2`，且 `gpui` 的 `test-support` 特性只能放入 `[dev-dependencies]`。
2. **跨端自适应唯一窗口宽度准则（`apps/mobile-web` & `soxu-desktop`）**：
   - **只对当前窗口实时宽度（Window Width）分支，绝不按设备型号、`isPad` 或横竖屏方向分支**：
     - `compact (< 640px)`：单栏流式布局，`16px` 页边距，选择器与详情一律走底部抽屉（Bottom Sheet，高度按视口比例 `0.52 / 0.78` 自适应，兼容折叠屏半折短视口），支持窄至 `320px`（iPad 分屏 1/3）与 `344px`（小折叠外屏）零水平溢出；
     - `regular (640px – 1023px)`：单栏居中限宽（消息 `760px` / 输入舱 `720px`），涵盖手机横屏、大折叠内屏与 iPad 竖屏（`~820px`）；
     - `wide (≥ 1024px)`：启用左会话/工作区轨（`280–320px`）+ 右主会话画布双栏并排（平板横屏与桌面 Web 预览）。
3. **隐私脱敏与生态安全展示规则**：
   - 界面展示本地路径时，自动将当前工作区根目录折叠为相对路径（如 `crates/soxu-core/src/lib.rs`），将用户主目录显示为 `~`（如 `~/.config/magpie`），避免截屏或录屏分享时暴露操作系统真实用户名。
   - 任何涉及覆盖/回退工作树未提交修改的操作（如 `Checkpoint Rewind`、`Discard Changes`）必须采用二次确认弹层并默认聚焦在「取消」或安全选项上。

---

## 五、代码语法高亮与纯黑终端 ANSI 16 色校准规范

### 5.1 14 角色代码高亮板（`syntax.*`）
传统暗色代码主题往往在 `#0F172A` 蓝黑背景上调校；放在 `#000000` / `#0D0D0D` 纯黑画布下容易出现过饱和刺眼或蓝紫暗沉。Soxu 从 `tokens.json -> color.{dark,light}.syntax` 统一下发 14 个语法角色：

| 语法角色 | Token 键名 | Pure Black (`#000000`/`#0D0D0D`) | Clean Light (`#FFFFFF`) | 说明与边界 |
| :--- | :--- | :---: | :---: | :--- |
| 普通文本 / 标识符 | `syntax.plain` | `#ECECEC` | `#1F2328` | 代码槽默认前景色（对比度 ≥ 14.9:1） |
| 关键字 / 控制流 | `syntax.keyword` | `#FF7B72` | `#CF222E` | `fn`、`pub`、`match`、`async`、`await` |
| 函数 / 方法定义 | `syntax.function` | `#D2A8FF` | `#8250DF` | 与 `syntax.type` 在明度上明显区隔 |
| 类型 / Trait / 枚举 | `syntax.type` | `#FFA657` | `#953800` | 结构名、泛型约束、枚举变体 |
| 字符串 / 正则 | `syntax.string` | `#A5D6FF` | `#0A3069` | 字面量字符串与格式化模板 |
| 数字 / 常量字面 | `syntax.number` | `#79C0FF` | `#0550AE` | 整浮点字面量与布尔常量 |
| 注释 / 文档说明 | `syntax.comment` | `#8B949E` | `#6E7781` | **对 `#0D0D0D` 对比度 ≥ 5.2:1，严禁暗灰不可读** |
| 属性 / 字段 / 宏 | `syntax.property` / `macro` | `#7EE787` / `#56D4DD` | `#116329` / `#0969DA` | Struct 字段、`#[derive(...)]` 与宏调用 |
| Diff 新增 / 删除背景 | `syntax.diffAddBg` / `diffDelBg` | `14%` 翠绿 / `14%` 绯红 | `12%` 翠绿 / `11%` 绯红 | 行号槽 `Gutter` 采用双倍浓度（`26%`/`22%`）形成清晰视觉边缘 |

- **代码块与 Diff 滚动纪律**：代码片段与差异对比视图（Unified / Split）仅在整块最底部渲染唯一一条整体横向滚动条，严禁为逐行单独出现横向滚动条；开启滚动时左侧行号槽采用固定粘性定位（`sticky left-0`）配合不透明底色防止代码文字透叠。
- **全端统一细滚动条规范**：全局纵向与横向滚动条统一 `6px` 宽/高、透明轨道与 `9999px` 胶囊圆角滑块，深浅色模式通过 `--soxu-interaction-scrollbar-thumb` 自适应调节对比度；会话标签条采用 `.soxu-auto-scrollbar`（`4px` 高），平时隐藏、悬停或滚动时浮现。
- **纯黑 PTY 终端 ANSI 16 色规范（`ansi.*`）**：
- 终端面板底色锁定 `--soxu-surface-terminal`（Dark `#050505` / Light `#F7F7F8`），保留细微 `1px` `--soxu-border-subtle` 外框；
- 重点校准传统终端最容易失明的 **ANSI Black (`#262626`)**、**BrightBlack (`#737373`)** 与 **ANSI Blue (`#60A5FA`)**，确保 Git diff 输出、Cargo 编译进度条与 Test Summary 在纯黑屏上字字清晰。

---

## 六、全局组件像素几何常量与 Z-Index 层级表

### 6.1 `sizing` 核心几何常量（GPUI `px()/sp()` 与 Web 1:1 对应）

| 令牌键 | 像素值 | 用途说明 |
| :--- | :---: | :--- |
| `sizing.titlebarHeight` | `38px` | 桌面主窗口顶部标题栏与 `Cmd+K` 触发条固定高度 |
| `sizing.tabbarHeight` | `36px` | 主会话标签条高度（单标签左右内边距 `12px`，关闭键 `18×18px`） |
| `sizing.subagentBarHeight` | `28px` | 委派子代理横向细轨高度 |
| `sizing.statusbarHeight` | `26px` | 桌面底栏全局 Daemon / Magpie / Git 状态横梁 |
| `sizing.sidebarRowHeight` | `32px` | 左侧项目与 Worktree 紧凑树行高度 |
| `sizing.toolCardHeaderHeight` | `34px` | 对话流中结构化工具调用折叠卡头部高度 |
| `sizing.diffGutterWidth` | `52px` | 活动 Diff 与右侧审阅器双列行号槽宽度 |
| `sizing.activityDiffMaxHeight` | `400px` | 对话流单张展开编辑 Diff 卡最大高度（超出原地滚动） |
| `sizing.permissionDetailMaxHeight` | `96px` | 权限审批卡片命令详情区最大滚动高度 |
| `sizing.cmdPaletteWidth` / `MaxHeight` | `640px` / `440px` | 全局 `Cmd+K` 命令面板尺寸（搜索输入区 `52px`） |
| `sizing.turnNavRailWidth` | `20px` | 对话区右侧边缘轮次迷你刻线导航轨宽度 |
| `sizing.compactRailWidth` | `52px` | 手机横屏 / 半折悬停态左侧紧凑功能轨宽度 |
| `sizing.dockPillHeight` | `40px` | 移动端上滑收起输入舱为胶囊的高度 |
| `sizing.sinkSidebarMaxWidth` | `316px` | 移动端 `SinkLayout` 底层抽屉最大展开宽度 |
| `sizing.touchHitMin` / `Primary` | `44px` / `48px` | 粗指针触屏最小扩展热区与主要提交/审批动作高度 |
| `layout.breakpointUltraCompact` | `400px` | 超窄手机与 iPad 1/3 分屏适配阈值 |
| `layout.shortViewportMax` | `500px` | 横屏/矮视口切换左侧紧凑轨与单行输入栏阈值 |
| `layout.macosTrafficLightsInset` | `78px` | macOS 桌面窗口左上角红绿灯安全留白宽度 |

### 6.2 Z-Index 层叠秩序（`zIndex`）
`base(0)` → `sticky(10)`（移动顶/底浮动舱） → `rail(15)`（轮次导航轨） → `header(20)`（顶栏） → `popover(40)`（内联工具坞浮层） → `drawerScrim(50)`（抽屉模糊遮罩） → `drawer(55)`（移动 Bottom Sheet 与左右侧滑抽屉） → `modalScrim(60)` → `modal(70)`（扫码配对/Checkpoint/提交弹窗） → `palette(80)`（`Cmd+K` 命令面板） → `floating(95)`（弹窗/抽屉内穿透下拉与选择器） → `toast(100)`（全局轻提示）。

---

## 七、全端通用边界态与跨端交互守则（Boundary & Input Rules）

1. **Empty State（首次空画布 / 无改动工作树）**：居中卡片限宽 `440px`，配 `36px` 线性矢量徽标、一句话明确说明及 **最多 1 个主 CTA + 3 个可点击 Starter Prompt 示例芯片**；严禁堆砌空白说明墙。
2. **Streaming & Skeleton（首次冷启动 / 会话详情回源）**：采用中性灰阶 `#171717` → `#212121` 微弱脉冲占位条（动画遵从 `prefers-reduced-motion`），占位行数严格匹配真实列表行高（`32px` 侧栏、`72px` 消息块），避免加载完成后发生剧烈布局跳变（CLS ≈ 0）。
3. **Offline / Reconnecting（Daemon 断线与增量游标追平）**：保留桌面与移动端屏幕上**所有已渲染的对话 DOM 与未发送草稿**（绝不整页白屏跳转报错），仅在顶栏显示非阻塞状态微条：`正在重连 Daemon (127.0.0.1:7698)...`，重连后自动携 `afterSeq` 拉取缺口并发出轻提示。
4. **Actionable Error（可执行错误提示）**：绝不把裸 JSON / `0x80070005` 甩给用户；每个错误框（`dangerSoft` 底 + `dangerBorder` 边）必须包含三要素：**发生了什么（中文） + 影响范围 + 一键修复或拷贝脱敏诊断详情按钮**。
5. **中文输入法组合护栏（IME Composition Safety）**：所有支持 `Enter` 提交的输入组件（主输入舱、Diff 内联批注、选区旁路追问、命令面板搜索）必须在 `onKeyDown` 首位检查 `isImeComposing(e)`（`nativeEvent.isComposing || keyCode === 229`），杜绝选词上屏误发。
6. **指针与触屏热区分化（`.soxu-hit-44` 与 `.soxu-reveal-action`）**：小尺寸辅助按键通过 `.soxu-hit-44` 在 `(pointer: coarse)` 下以透明伪元素将触控命中框扩至至少 `44×44px`；行内显隐操作按钮采用 `.soxu-reveal-action`（细指针悬停/聚焦/选中时显现，触屏固定 `opacity: 0.88` 永不形成交互死角）。
7. **跨平台快捷键呈现（`formatShortcut`）**：依据当前操作系统或外观窗口预览自动映射 `Mod` / `Alt` / `Shift` 提示为 `Cmd` 组合（`⌘` / `⇧` / `⌥`）或 `Ctrl+` 组合；用户 Chat 气泡统一绑定 `--soxu-surface-raised`（严禁引用未定义变量导致透明气泡）。

---

## 八、分页面与核心子系统规范索引

在进行具体模块开发前，Agent 必须同时阅读本文件与对应的分页面规格书：

1. [`pages/01-desktop-workbench.md`](pages/01-desktop-workbench.md) —— GPUI 桌面四栏工作台布局、**收件箱一触分诊台与置顶管理**、**多文件可批注 Diff 与右栏全宽审阅模式**、**轮次迷你导航轨**、分隔条键盘无障碍与快捷键表
2. [`pages/02-composer-and-handoff.md`](pages/02-composer-and-handoff.md) —— 三态输入舱、**`MultiQueueBar` 多条可视化队列**、4 段构成彩色窗口分析、IME 保护、`lost-controls` 回退、窄屏 Bottom Sheet 降级与 `@agent` 移交
3. [`pages/03-timeline-tools-and-checkpoints.md`](pages/03-timeline-tools-and-checkpoints.md) —— 对话流角色排版、**`LiveSteps` 实时步骤条**、**Oryn 风格 `ToolCallCard`（22px 图标块 + 意图读数）**、**Artifact 产物卡**、**选区引用与旁路解释卡**、**分叉会话**、结构化提问与审批对比
4. [`pages/04-magpie-and-cli-inspector.md`](pages/04-magpie-and-cli-inspector.md) —— Magpie 双态拓扑、7 大原生 CLI 徽标与能力矩阵、Worktree 启动环境变量预览器与本地历史会话无损导入流
5. [`pages/05-mobile-remote-pwa.md`](pages/05-mobile-remote-pwa.md) —— Daemon 内嵌移动遥控 PWA：**13 组真机视口 + 折叠屏双段布局**、**`SinkLayout` 下沉侧栏与 52px 底层胶囊**、**40px 上滑 `DockPill` 输入胶囊**、手势下滑关闭 Bottom Sheet 与大拇指审批流
6. [`pages/06-modals-onboarding-and-automation.md`](pages/06-modals-onboarding-and-automation.md) —— 首次 3 步 Onboarding 向导、`Cmd+K` 全局命令面板（16px 防缩放、触屏隐藏多余键位）、添加项目弹窗、Git Emoji 提交预览与调度中心
7. [`pages/07-settings-fonts-and-motion.md`](pages/07-settings-fonts-and-motion.md) —— `HoverExpandDock` 细/粗指针分化、7 分区设置中心、**macOS 红绿灯与折叠双屏预览切换**、双字体体系与 GPUI 动效对照矩阵
