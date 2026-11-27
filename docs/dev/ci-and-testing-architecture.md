# GPUI 测试路线与 Desktop 发布蓝图（CI-E/F）

> 本文为 M4（GPUI 工作台）与 M6（Desktop 发布）的验收方案基线，对应 Issue [#93](https://github.com/TanCan1998/Soxu/issues/93)。
> 本文是**纯文档交付**：不引入 GPUI 或其他产品依赖、不修改 CI 工作流、不修改产品 Crate 与发布契约。
> 所有 GPUI 能力结论均标注证据等级；参考项目的成功运行**不替代**本项目实测。

---

## 1. 范围与证据等级

### 1.1 本文回答什么

1. Soxu 的 GPUI 桌面端在**哪一层**用什么手段验证，每一层能证明什么、不能证明什么；
2. 视觉回归夹具必须固定哪些变量，失败样本落在哪里；
3. 真实 macOS Runner 的能力边界（架构、Metal、离屏绘制、WindowServer），以及受限时补真机入口；
4. Desktop 从「ad-hoc 内部验证」走到「Developer ID + 公证」的门槛，以及不满足条件时的失败关闭行为；
5. 纯文档 PR 在现有轻量 CI 下的实际跳过矩阵，以及 M4 引入 GPUI 后门禁需要如何演进（本 PR 不做）。

### 1.2 证据等级（全文与 PR 说明共用）

| 等级 | 含义 | 允许的表述 |
| :--- | :--- | :--- |
| `实测` | 本文作者在本机或真实 GitHub 托管 Runner 上执行并保留命令、输出、run/job URL | 可以下结论 |
| `源码核验` | 读取固定 SHA 的 crate 源码或上游仓库文件，命中具体符号/行 | 可以下「当前版本无此 API」类结论 |
| `参考推断` | 参考项目（Padu、Zed、gpui-shot 等）的做法与线上记录 | 只能作为做法依据，不能当本项目验收 |
| `未验证` | 未执行、无证据 | 必须显式列为待办，不得包装成通过 |

**红线**：`TestAppContext` 通过、像素快照通过、`cargo test --no-run` 通过、参考项目 CI 通过，**都不得**表述为「macOS 真机验收通过」。

---

## 2. 版本与参考绑定（固定 SHA）

### 2.1 目标 GPUI 版本（实测）

| 依赖 | 版本 | 证据 |
| :--- | :--- | :--- |
| `gpui` | `0.2.2` | `cargo info -q --registry crates-io gpui` 实测 crates.io 当前最新稳定版；`test-support` feature 存在（实测 crate manifest） |
| `gpui-component` | `0.11.0` | `cargo info -q --registry crates-io gpui-component` 实测最新稳定版，60+ 组件 |

> M4 落地时以**再次实测**的最新稳定版为准；本表记录的是本文成文时的绑定值。若届时版本前进，第 3 章的 API 结论必须重新核验，不得直接沿用。

### 2.2 参考项目与固定提交

| 参考 | 固定 SHA | 实际读取的文件 | 结论用途 |
| :--- | :--- | :--- | :--- |
| Zed（GPUI 上游） | `a0e666ae930a85ab38947e2d49222b0ea99f1c2f` | `crates/gpui/examples/testing.rs`、`crates/gpui/docs/contexts.md` | 测试宏、`TestAppContext` / `VisualTestContext` 语义、属性测试与多 App 上下文 |
| `AprilNEA/gpui-shot` | `a12f7efd1a7abdc66ff16c6bcf43572c868d0efd` | `.github/workflows/visual.yml`、`docs/gpuix-notes.md` | 视觉回归的矩阵形态与**上游捕获能力缺口** |
| `Ziinc/gpui-testing-library` | `eb3521f240319e5f2fa5d1edbfc7879b1bb8e1f9` | `Cargo.toml` | 第三方测试库的版本耦合现实 |
| Padu（本地既有索引） | `fbe97a38e0a1fea8a9a386bda5f969f1f0bc6d02` | `apps/desktop/src/input.rs`、`apps/desktop/src/browser.rs`、AOCI 条目、CodeGraph 符号图 | 测试组织、IME/焦点语义、夹具边界 |

### 2.3 已核实的版本落差（重要）

以下三条均为**源码核验**结论，直接决定本文的写法：

1. **Padu 用的不是 `gpui` 0.2.2**。`apps/desktop/Cargo.toml`（`fbe97a3`）把 `gpui` 重命名为 `bezel-gpui` 且版本为 `0.3.21`，同时依赖 `bezel-gpui-platform`。因此 Padu 的测试代码**不能逐行照搬**到 Soxu。
2. **`gpui` 0.2.2 没有内建截图/离屏捕获 API**。在 `gpui-0.2.2` crate 源码全量检索 `fn capture_screenshot` 与 `fn render_to_image` 均为**零命中**。这解释了 `gpui-shot` 为何需要 `patches/zed-v0.224.8-linux-blade-capture.patch`，以及 `gpuix` 为何使用打过补丁的 Zed fork。
3. **`gpui-testing-library` 绑定的是 gpui 快照**。其 `Cargo.toml` 依赖 `gpui = { package = "gpui-pre", version = "=0.3.9" }` 与 `gpui-kit = "=0.7.3"`，即该库与具体 GPUI 快照强耦合，**不适用于 `gpui` 0.2.2**。

### 2.4 Padu 测试模式的借鉴边界

采用（`参考推断` + 本项目实测校准）：

- **Harness 根视图**：测试里用最小的 `Render` 根视图包住被测实体（Padu 的 `InputHarness` / `ComposerHarness`），只声明宽度，不复制业务外壳；
- **统一装配函数**：`fn setup_input(cx, content, width) -> (Entity<T>, &mut VisualTestContext)` 一次性完成 `init`、建实体、**显式 `window.focus(...)`**、`run_until_parked()`，避免每个用例各写一遍焦点漂移点；
- **IME 直接驱动 `EntityInputHandler`**：Padu 的 `ime_composition_after_ascii_keeps_its_caret` 不依赖系统输入法，而是直接调 `replace_and_mark_text_in_range` / `select_range`，因此该层测试在未安装中文输入法的 CI 上同样可跑；
- **布局用选择器断言**：Padu 的 `an_unwrapped_field_gives_its_pane_a_horizontal_range` 断言 `ScrollHandle::max_offset()` 而不是比较像素图，与本项目 `debug_bounds` 路线同源；
- **语义化命名**：测试名直接描述被保护的不变量（`set_content_with_same_length_text_re_arms_caret_follow`），失败时无需读实现即可定位。

不采用：

- `bezel-gpui` / `bezel-gpui-platform` 的包名与平台 crate 划分（Soxu 目标是上游 `gpui`）；
- Padu 的产品级 IME 编辑器实现（UTF-16 光标、内联文件引用、软换行滚动）——Soxu 只借鉴其**测试驱动方式**；
- Padu 的 Windows Job Object、浏览器运行时等与本文无关的子系统。

---

## 3. 四层测试路线（A）

四层不是「覆盖率分层」，而是**能力分层**：每一层只能证明它自己范围内的东西，任何一层都不能顶替下一层。

### 3.1 L0 · 平台无关逻辑测试

| 项目 | 内容 |
| :--- | :--- |
| 证明什么 | 纯函数、状态机、协议编解码、令牌换算、Markdown 分块、Diff 行映射等不含窗口语义的行为 |
| 不证明什么 | 渲染、焦点、输入法、布局、平台差异 |
| 入口 | 普通 `#[test]`，随 `cargo test -p soxu-desktop` 进入现有 Rust 矩阵 |
| 失败关闭 | 任一断言失败即门禁红；不得 `#[ignore]` 掩盖 |
| 成本 | 秒级，跨平台稳定 |

要求：L0 用例必须能在**无真实显示器的 CI 环境**（`ubuntu-24.04` 无 GPU、`macos-15` 为虚拟 GPU 且无物理显示器）上通过。任何需要窗口的断言一律下沉到 L1/L2。

### 3.2 L1 · 隔离 `TestAppContext` 的 GPUI 交互测试

| 项目 | 内容 |
| :--- | :--- |
| 证明什么 | 实体状态流转、事件订阅、动作派发、焦点归属、布局几何、异步任务调度顺序、IME 标记区语义 |
| 不证明什么 | 真实 GPU 合成结果、真实系统输入法、真实窗口管理器行为、字体回退的最终像素形态 |
| 入口 | `#[gpui::test]`，`cargo test -p soxu-desktop --features test-support` |
| 失败关闭 | GPUI 测试执行器对「等待 GPUI 控制外事件」按设计 panic；需要跨线程时必须显式 `allow_parking()` 并说明理由 |
| 成本 | 毫秒到秒级；`gpui 0.2.2` 实测在 `macos-15`（ARM64）Runner 上 8 项用例全绿（见第 5 章） |

`gpui 0.2.2` 可用测试面（**源码核验**，`src/app/test_context.rs`）：

| API | 用途 | 本文实测状态 |
| :--- | :--- | :--- |
| `TestAppContext::update / read / update_entity / read_entity` | 无窗口访问实体 | 可用 |
| `App::new`（经 `AppContext` 引入） | 建实体；**`TestAppContext::new` 在 0.2.2 是私有方法** | 可用（必须 `cx.update(|cx| cx.new(..))`） |
| `add_window_view` | 建窗口 + 根视图，根视图类型必须 `impl Render` | 可用 |
| `VisualTestContext::dispatch_action` | 按焦点派发动作 | 可用 |
| `simulate_mouse_move / simulate_mouse_down / simulate_click / simulate_resize` | 指针与窗口几何 | 可用 |
| `simulate_keystrokes / simulate_input / dispatch_keystroke` | 键盘与字符输入（`simulate_input` 按字符逐个派发，**不是** IME 组合事件） | 可用 |
| `debug_selector(|| "id".into())` + `debug_bounds("id")` | 用选择器取布局边界，替代像素比较 | 可用 |
| `executor().allow_parking()` | 放行跨线程等待 | 可用 |
| `#[gpui::test(iterations = N)]` + `StdRng` | 属性测试 | 上游示例提供，未在本项目落地 |
| `capture_screenshot` / `render_to_image` | 离屏像素读取 | **0.2.2 不存在** |

> `focus` 的签名在 0.2.2 是 `FocusHandle::focus(&self, window: &mut Window)`（单参）；这是与 0.3.x 系列不一致的真实差异，迁移时必须重核。

### 3.3 L2 · 真实渲染的视觉回归

| 项目 | 内容 |
| :--- | :--- |
| 证明什么 | 在**像素层面**证明同一输入在固定平台/字体/DPI/主题下渲染一致；捕获主题令牌、字体回退、圆角/描边/毛玻璃等无法用几何断言表达的效果 |
| 不证明什么 | 真实系统输入法、真实窗口管理器、Finder/PTY/剪贴板等系统集成；跨平台一致性（不同后端像素不可比） |
| 入口 | 独立 workflow（**本 PR 不新增**），按平台固定后端：Linux/`lavapipe` 权威、macOS/`Metal` 参考 |
| 失败关闭 | 差异超阈值即红；更新基线必须走显式「接受新基线」提交并附 diff 图，不得静默覆盖 |
| 成本 | 分钟级，需要 GPU 后端或软件光栅 |

**前置事实**：`gpui 0.2.2` 无内建离屏捕获（§2.3），因此 L2 **不可能**仅靠 `cargo test` 完成。可行路线只有三条，按优先级：

1. **下游捕获补丁**：参照 `gpui-shot` 的 `patches/zed-v0.224.8-linux-blade-capture.patch` 做法，对 Soxu 实际使用的 GPUI 版本自带最小补丁（Linux Blade `render_to_image`）；
2. **分支/后端替换**：参照 `gpuix` 使用打过补丁的 GPUI fork（代价是脱离上游，必须记录 fork SHA 与 patch 内容）；
3. **平台外部截图**：macOS 走 `Metal` drawable 读回、Linux 走 `Xvfb` + 软件光栅外部抓屏（`参考推断`：`gpui-shot` 的 `visual.yml` 正是 `ubuntu-24.04` + lavapipe 权威、`macos-15` + Metal 参考的双实例形态）。

**在选定路线前，本项目的视觉回归一律标记为 `未验证`。**

#### 视觉夹具必须固定的变量

| 变量 | 固定方式 | 不固定的后果 |
| :--- | :--- | :--- |
| 字体 | **仓库内 vendored 字体文件**（UI：Inter + Noto Sans SC；代码：JetBrains Mono），测试内显式 `font` 加载，禁止回退到系统字体栈 | Runner 字体差异导致全局字形漂移 |
| 字号与行高 | 用 `design-system/soxu/tokens.json` 的 `uiFontSizePx` / `codeFontSizePx`，禁止用例内硬编码另一套值 | 同一设计令牌两处真相 |
| DPI / scale factor | 每个用例显式声明 `scale_factor`，并至少覆盖 `1.0` 与 `2.0` 两档；**不以「真实屏幕 DPI」为输入** | Retina 与非 Retina 结果不可比 |
| 窗口尺寸 | 固定 `Bounds`（例如四栏 `1440×900@1.0`、窄态 `320×800@2.0`），与设计规范断点对齐 | 断点邻域抖动 |
| 主题 | 固定 `Pure Black` 与 `Light` 两个显式主题实例，禁止读系统外观偏好 | 深色/浅色随宿主机翻转 |
| 静态数据 | 会话 Transcript、工具卡、Diff、设置项一律来自**仓库内 JSON fixture**，禁止实时 Daemon 数据 | 数据漂移使 diff 无意义 |
| 时钟 | 注入可控时钟（固定 `Instant`/`now`），禁止在渲染路径直接读系统时间 | 时间戳、相对时间文本抖动 |
| 动画与合批 | 固定动画 tick 或直接关闭动画；`Transcript` 的 `<=8.3Hz` 合批必须在夹具里显式推进到稳定帧 | 半帧捕获导致随机差异 |
| 渲染后端 | workflow 内固定后端（Linux/`lavapipe`、macOS/`Metal`），并把 `runner.os` + 后端写进证据 | 跨后端像素比较必然误报 |

#### 像素容差与失败样本

- **比较方式**：逐像素 RGBA 比较 + 差异计数，不用「整图哈希」作为唯一判据（哈希无法给出可读失败原因）。
- **阈值**：按用例声明 `max_diff_ratio` 与 `per_channel_tolerance`；默认建议 `per_channel_tolerance = 0`、`max_diff_ratio = 0`（严格模式）起步，只有在**同平台同后端**复现出确定性抗锯齿差异时才允许放宽，且必须在用例内写明放宽理由与来源 run。
- **失败样本可查看路径**：任一用例失败必须产出并上传三份文件——
  - `$RUNNER_TEMP/visual/<case>/expected.png`
  - `$RUNNER_TEMP/visual/<case>/actual.png`
  - `$RUNNER_TEMP/visual/<case>/diff.png`（差异高亮）
  
  作为 `visual-diff-<run_attempt>-<platform>` artifact 上传（`retention-days` 与现有证据一致，14 天），并在失败日志里打印三份文件的**绝对路径与 artifact 名**。
- **基线更新**：只能由显式提交（如 `GPUI_SHOT_UPDATE=1` 或等价语义）产生，PR 必须附 diff 图与「为何新基线正确」的说明；禁止在 CI 上自动更新基线。

### 3.4 L3 · macOS 真机验收

L1/L2 在托管 Runner 上能跑通**不代表**产品在真实 macOS 桌面上可用。以下能力只能在真机（人工或有屏幕的机器）验收，必须逐项留证：

| 能力 | 检查点 | 为什么 Runner 覆盖不了 |
| :--- | :--- | :--- |
| Metal / Retina | `MTLCreateSystemDefaultDevice()` 名称、`hasUnifiedMemory`、真实 drawable 合成、`scale_factor = 2.0` 的实际清晰度 | Runner 的 GPU 是 `Apple Paravirtual device`，且 `system_profiler SPDisplaysDataType` **输出为空**（无物理显示器）；见第 5 章实测 |
| WindowServer | 窗口出现、前置/失焦、多窗口与全屏切换、Dock/菜单栏交互 | Runner 有 `Aqua` 会话与 WindowServer 进程，但无显示器、无解锁交互式桌面、无真实键盘事件源 |
| Finder PATH | 从 Finder 双击 `.app` 启动时 `PATH` 不含 Homebrew 目录，CLI 定位必须显式解析 | 需要真实 LaunchServices 启动路径 |
| Homebrew CLI | `/opt/homebrew/bin`（ARM64）与 `/usr/local/bin`（Intel）下 `claude`/`codex`/`pi` 等被正确发现 | Runner 的 `PATH` 由 Actions 注入，不能代表 Finder 启动 |
| Command 快捷键 | `cmd-*` 组合键、`cmd+tab` 失焦恢复、输入法切走后的按键归属 | 需要真实事件源 |
| 菜单 | App 菜单、Edit 菜单的标准 `cut/copy/paste/selectAll` 路由到 GPUI 焦点元素 | 需要真实菜单栏 |
| 输入法 | 中文（拼音）与日文输入法的候选窗定位、`replace_and_mark_text_in_range` 的标记区、组合中提交/取消、光标不跳到行首 | 需要系统 IME；L1 只能直接驱动 handler |
| 剪贴板 | 文本/图片/文件路径三种粘贴与富文本降级 | 需要真实 `NSPasteboard` |
| 拖拽 | 从 Finder 拖文件到 Composer、内部拖拽、拖出到 Finder | 需要真实 dragging session |
| PTY | `#050505` 多窗格终端、窗口缩放时的重排、`ctrl-c` 与信号语义 | 需要真实伪终端与 TTY 信号链 |
| 权限 | 完全磁盘访问、辅助功能、通知权限的首次请求与拒绝后降级 | 需要真实 TCC 数据库 |
| Daemon 生命周期 | App 退出/崩溃时 Daemon 是否被正确回收、重开时重连、`MainActor` 退出清理 | 需要真实进程与登录会话 |

L3 的产出物固定为「检查项 → 环境（macOS 版本、机型、架构、Homebrew 前缀）→ 实际结果 → 证据（截图/日志路径）」四列，缺列即视为未验收。

---

## 4. 覆盖矩阵（B）

图例：`●` 必须覆盖、`○` 建议覆盖、`—` 不适用。**同一行的 `●` 只在其所在层有效**，不得跨层替代。

| 场景 | L0 逻辑 | L1 `TestAppContext` | L2 视觉回归 | L3 真机 |
| :--- | :--- | :--- | :--- | :--- |
| 四栏工作台自适应（宽/中/窄、`SinkLayout` 下沉侧栏、320px 零溢出） | ○ 断点计算 | ● `debug_bounds` 断言各栏宽与折叠 | ● 三档宽度快照 | ● 真实窗口缩放 |
| 流式 Transcript（`<=8.3Hz` 合批、轮次导航轨、虚拟滚动） | ● 合批时间窗与索引映射 | ● 注入事件流后推进时钟断言批次数 | ● 稳定帧快照 | ○ 长会话滚动流畅度 |
| Markdown / 代码块（标题层级、表格、行内代码、高亮） | ● 解析与令牌切分 | ○ 段落到元素的映射 | ● 高亮配色与字体 | — |
| Composer（760px 同步宽输入舱、`@agent` 移交、权限圆环） | ● 宽度与令牌计算 | ● 输入、提交、`secondary-Enter` 排队 | ● 聚焦/悬停/禁用态 | ● 真实 IME + `cmd-enter` |
| 工具卡（Intent / Artifact / LiveSteps 折叠） | ● 卡片状态机 | ● 展开/折叠与事件冒泡 | ● 三态快照 | ○ 触控板滚动 |
| 审批与提问（高危红线段、6 档权限、`PendingUserInput`） | ● 权限判定与红线 | ● 审批后状态流转与事件 | ● 高危卡描边与危险色 | ● 真实二次授权弹窗 |
| Diff / Files 右栏（行内/并排、超长行、二进制） | ● 行映射与截断 | ● 切换视图后几何 | ● 并排两列快照 | ● 真实字体渲染 |
| 设置（7 分区、主题切换、语言切换） | ● 偏好读写 | ● 切换后全局状态 | ● 两主题 × 两分区 | ● 系统外观联动 |
| 主题（Pure Black / Light、7 级海拔对比度） | ● 令牌换算 | ○ 主题注入后的背景色 | ● 全页快照 | ○ 高对比度辅助功能 |
| 窗口缩放（最小尺寸、跨断点、DPI 切换） | ○ 断点阈值 | ● `simulate_resize` 后重新布局 | ● 断点前后快照 | ● 拖拽边缘与 Retina/非 Retina |
| 多语言（zh-CN / en、可扩展 i18n） | ● 键查找与回退 | ● 切换后文本替换 | ○ 两种语言各一页 | ○ 输入法配合 |
| 中文长文本（UTF-16 光标、换行、IME 标记区） | ● UTF-16 ↔ 字节偏移 | ● 标记区与光标断言（本文第 5 章有实测用例） | ● 竖排/长行换行形态 | ● 真实拼音输入法 |

> 空白即缺陷：M4/M5 每个页面合入前，必须把该行在 L0/L1 的 `●` 全部落地；L2 的 `●` 只在视觉回归管线建立后生效；L3 必须在 M6 发布前完成一轮记录。

---

## 5. 真实 macOS Runner 实测（C）

### 5.1 实验设置

- 独立实验分支：`exp/93-gpui-macos-probe`（**不进入产品 PR，保持未合入**）
- 夹具：`exp/gpui-macos-probe`（自带 `[workspace]`，与 Soxu 产品 workspace 完全隔离）
- 依赖固定：`gpui = "=0.2.2"`，`features = ["test-support"]`
- Runner：`macos-15`（GitHub 托管标准 Runner，未使用 larger runner）
- 独立缓存：`CARGO_TARGET_DIR=..\.cargo-target\soxu-93-exp`（不污染产品缓存）
- 未使用任何 Secret、签名服务或产品凭据

### 5.2 夹具覆盖的测试能力

`exp/gpui-macos-probe/tests/gpui_context.rs` 共 8 项用例，逐条对应第 3 章的能力层：

| 用例 | 覆盖能力 |
| :--- | :--- |
| `utf16_offsets_round_trip_for_chinese_text` | L0：UTF-16 ↔ 字节偏移换算（中文长文本前置） |
| `entity_update_emits_without_a_window` | L1：无窗口实体更新 |
| `focus_and_action_dispatch_reach_the_render_tree` | L1：窗口焦点归属 + 动作经渲染树解析 |
| `layout_bounds_are_queryable_by_debug_selector` | L1：`debug_selector` + `debug_bounds` 几何断言 |
| `ime_marked_range_keeps_utf16_caret` | L1：直接驱动 `EntityInputHandler` 的中文 IME 标记区 |
| `click_and_resize_do_not_panic` | L1：指针与窗口几何事件 |
| `detached_tasks_run_only_when_parked` | L1：异步任务调度顺序 |
| `external_thread_requires_allow_parking` | L1：`allow_parking()` 与跨线程等待 |

### 5.3 实测结果

> 以下为真实命令与结果，`实测` 等级。失败项一律保留原文，不改写为通过。

**执行位置**：`exp/93-gpui-macos-probe` @ `5c12f04`（基线 `740b743`），GitHub 托管 `macos-15`。
**Run / Job**：run `46019734750` / job `140575960524`（作业名 `GPUI probe (macos-15 ARM64)`），结论 **`success`**。
**证据 artifact**：`exp-gpui-macos-probe-1`（`runner.txt`、`gpu.txt`、`toolchain.txt`、`cargo-test.log`、`displays.txt`）。

#### 5.3.1 Runner 身份（`runner.txt` 原文）

| 字段 | 实测值 |
| :--- | :--- |
| `runner_os` / `runner_arch` | `macOS` / `ARM64` |
| `uname_m` | `arm64` |
| `sw_vers` | `macOS 15.8.2`，`BuildVersion: 24H232` |
| 内核 | `Darwin 24.6.0 … RELEASE_ARM64_VMAPPLE arm64` |
| `cpu` | `Apple M1 (Virtual)` |
| `arm64_hint` | `1` |
| `hw_model` | `VirtualMac2,1` |
| `managername` | `Aqua` |
| `who` | `runner   console` |
| `windowserver_procs` | `1` |
| `xcodebuild` / `sdk` / `swift` | `Xcode 16.4 (16F6)` / `15.5` / `Swift 6.1.2` |

#### 5.3.2 Metal 与图形会话（`gpu.txt` 原文）

```text
cgsession_present=1
cgsession_kCGSSessionOnConsoleKey=1
cgsession_kCGSSessionLoginDoneKey=missing
cgsession_CGSSessionScreenIsLocked=missing
cgsession_kCGSSessionUserNameKey=runner
metal_device=Apple Paravirtual device
metal_unified_memory=true
metal_low_power=false
metal_max_threadgroup=MTLSize(width: 1024, height: 1024, depth: 1024)
offscreen_status=completed
offscreen_first_pixel_bgra=191,128,64,255
```

离屏绘制探针的做法：建 64×64 `bgra8Unorm` 纹理（`usage = [.renderTarget]`、`storageMode = .shared`）→ `loadAction = .clear`、`clearColor = (0.25, 0.5, 0.75, 1.0)` → 提交并 `waitUntilCompleted()` → `getBytes` 读回首像素。

- `offscreen_status=completed`：命令缓冲**完成**，Metal 渲染管线在虚拟 GPU 上可用；
- 首像素 `BGRA=191,128,64,255` 与 `clearColor` 完全对应（B=0.75×255=191、G=0.5×255=128、R=0.25×255=64、A=255），证明**读回字节序与内容正确**，不是空缓冲；
- `system_profiler SPDisplaysDataType` **输出为空**（`displays.txt` 为 0 字节）：该 Runner **没有物理显示器**。

#### 5.3.3 GPUI 测试结果（`cargo-test.log`）

```text
toolchain: rustc 1.99.1 (58cf8f9c3 2026-10-13) / cargo 1.99.1 (5f94df478 2026-08-27)
gpui pin: gpui = { version = "=0.2.2", features = ["test-support"] }
command : cargo test --locked -- --test-threads=1
build   : Finished `test` profile [unoptimized + debuginfo] target(s) in 3m 07s
lib     : test result: ok. 0 passed; 0 failed  (本夹具无 lib 单测，用例全在集成测试目标)
target  : tests/gpui_context.rs → test result: ok. 8 passed; 0 failed; 0 ignored; finished in 0.02s
  ok utf16_offsets_round_trip_for_chinese_text
  ok entity_update_emits_without_a_window
  ok focus_and_action_dispatch_reach_the_render_tree
  ok layout_bounds_are_queryable_by_debug_selector
  ok ime_marked_range_keeps_utf16_caret
  ok click_and_resize_do_not_panic
  ok detached_tasks_run_only_when_parked
  ok external_thread_requires_allow_parking
doc     : test result: ok. 0 passed; 0 failed
```

同一提交在推 Runner 前已在本机 Windows（同版本 toolchain、独立 `CARGO_TARGET_DIR`）跑过同一命令，`8 passed / 0 failed`，用来保证推上去的是**可编译且本地通过**的夹具，不把编译错误冒充「Runner 受限」。

#### 5.3.4 失败与修正留痕（不隐藏）

夹具在本地首次成型时曾出现 4 类真实编译/断言失败，修正后未再复现，保留在这里以防后人重踩：

| 现象 | 根因 | 处理 |
| :--- | :--- | :--- |
| `TestAppContext::new is private` | 0.2.2 的 `TestAppContext::new` 是私有继承方法，只供内部使用 | 改用 `cx.update(|cx| cx.new(..))`（`AppContext` trait 必须引入作用域） |
| `focus` 参数个数不符 | 0.2.2 为 `FocusHandle::focus(&self, window)` 单参 | 去掉多余 `cx` 实参 |
| `utf16` 断言错（11 vs 14、3 vs 6） | 断言用的是**字节**偏移而期望值按 UTF-16 写的 | 按实际 UTF-16 长度与字节长度分别断言 |
| IME 标记区 `4..6` vs `2..4` | 标记区起点是**插入后的光标位置**而非 0 | 断言改为绝对 UTF-16 位置 |

### 5.4 能力边界结论

#### 5.4.1 已证实（`实测`）

| 能力 | 结论 | 证据 |
| :--- | :--- | :--- |
| ARM64 原生 | `macos-15` 实例为 arm64 原生（`uname -m = arm64`、`arm64_hint = 1`） | `runner.txt` |
| GPUI 可在 CI 上跑 L1 | `gpui 0.2.2` + `test-support` 在 3m07s 内完成编译，8 项用例 0.02s 全绿 | `cargo-test.log` |
| Metal 设备存在 | `MTLCreateSystemDefaultDevice()` 返回 `Apple Paravirtual device`，`hasUnifiedMemory = true` | `gpu.txt` |
| 静默离屏渲染与读回 | 离屏 render pass 命令缓冲 `completed`，读回首像素与 `clearColor` 完全一致 | `gpu.txt` |
| 至少存在 GUI 会话 | `managername = Aqua`、`windowserver_procs = 1`、`CGSession` 存在且 `OnConsole = 1` | `runner.txt` / `gpu.txt` |

#### 5.4.2 仍未覆盖（`未验证`，不得写成通过）

| 能力 | 为什么 Runner 覆盖不了 | 补真机入口 |
| :--- | :--- | :--- |
| 真实 drawable 合成与 Retina | 无物理显示器（`system_profiler` 输出为空），GPU 为 Paravirtual，`scale_factor = 2.0` 只能测逻辑值，测不出真实清晰度 | §5.5 第 1–2 项 |
| 系统输入法（拼音/日文） | Runner 未启用中文输入法；本次只证明**直接驱动 `EntityInputHandler`** 可行 | §5.5 第 3 项 |
| 键盘事件源与 Command 快捷键 | `kCGSSessionLoginDoneKey`、`CGSSessionScreenIsLocked` 缺失，说明不是解锁的交互式桌面；无真实 HID 事件 | §5.5 第 1–2 项 |
| 窗口前置/失焦、菜单栏、Dock、全屏 | 有 WindowServer 进程，但无显示器、无用户交互 | §5.5 第 2 项 |
| Finder 启动路径与 PATH | Actions 启动而非 LaunchServices | §5.5 第 4 项 |
| TCC 权限与剪贴板/拖拽 | 需要真实用户会话与交互 | §5.5 第 5 项 |
| PTY 与 Daemon 生命周期 | 未在本次实验范围内，且需真实登录会话退出路径 | §5.5 第 6 项 |
| 视觉回归（L2） | `gpui 0.2.2` 无捕获 API；本次只证明 Metal 离屏通道在虚拟 GPU 上可用 | §3.3 三条路线 |

#### 5.4.3 结论

1. **L1 从「未验证」提升为「`macos-15` ARM64 通过」**：8 项用例覆盖实体、焦点、动作、几何、IME、事件、异步与跨线程，可在现有免费托管 Runner 上常态运行。
2. **L2 仍为 `未验证`**：本次只证明「底层 Metal 离屏可读回」，未证明「GPUI 能把帧交给这条通道」，后者需要 §3.3 的捕获补丁决策。
3. **L3 必须真机**：虚拟 GPU + 无显示器 + 非解锁会话，三者叠加已足以否定「Runner 等于真机验收」。

### 5.5 补真机入口

托管 Runner 受限的项目必须在真机补齐，且不得因为「Runner 已跑通 L1」而跳过：

1. 一台 Apple Silicon Mac（`macOS 15+`）与一台 Intel Mac（或 `macos-15-intel` Runner 补架构证据）；
2. 真实 GUI 会话（`Aqua`）：窗口合成、菜单栏、Dock、全屏、多显示器；
3. 系统输入法：拼音与日文各一轮，覆盖组合中提交、组合中取消、光标跟随；
4. Finder 启动路径：双击 `.app`、`open -a`、`PATH` 缺失 Homebrew 的场景；
5. 权限与 TCC：完全磁盘访问、辅助功能、通知的首次与拒绝后路径；
6. PTY 与 Daemon 生命周期：终端信号、App 退出回收、崩溃后重连。

---

## 6. Desktop 发布蓝图（D）

### 6.1 三类产物必须分开

| 产物 | 内容 | 现状 |
| :--- | :--- | :--- |
| CLI / Daemon | `soxu`、`soxu-daemon` 等无 GUI 二进制 | 已在四平台资产中发布 |
| GPUI Desktop（`.app`） | 含 `Info.plist`、资源、字体、图标的应用包 | **尚不存在**，本文只定蓝图 |
| 分发包（`.dmg`） | 承载 `.app` 的版本化镜像 + `SHA256SUMS` 成员 | **尚不存在** |

**当前发布的四平台资产集合是冻结契约**（`release.yml` 的 `EXPECTED_RELEASE_ASSETS`）：

```text
soxu-linux-amd64.tar.gz
soxu-windows-amd64.zip
soxu-darwin-arm64.tar.gz
soxu-darwin-amd64.tar.gz
SHA256SUMS
```

任何新增资产（`.app` / `.dmg`）都必须是**独立后续 PR + 显式契约变更 + 双仓联动验证**，本文与对应产品 PR 都不得改动该集合、不得改动 `release.yml`、不得创建 tag。

### 6.2 架构与执行方式

| 组合 | 执行方式 | 结论 |
| :--- | :--- | :--- |
| `darwin-arm64` | 在 `macos-15`（ARM64）**原生**构建与测试 | 必须 |
| `darwin-amd64` | 在 `macos-15-intel` 原生构建与测试（该 Runner id 已存在于本仓平台计划中，仅在显式入口请求） | 必须 |
| `darwin-amd64` 交叉编译 | 在 ARM64 上 `--target x86_64-apple-darwin` | 只作为**补充**，不得替代 Intel 原生验收 |
| Rosetta 运行 arm64 包 | 仅用于开发者本机临时验证 | **不构成** Intel 支持证据 |

失败关闭：Intel 原生 Runner 不可用或未跑时，发布说明必须写明「Intel 未原生验证」，且不得把该版本标为 Intel 支持版本。

### 6.3 版本化命名契约（提案）

```text
soxu-desktop-<version>-darwin-arm64.dmg
soxu-desktop-<version>-darwin-amd64.dmg
```

约束：

1. `<version>` 必须与 tag `v<version>`、`.app/Contents/Info.plist` 的 `CFBundleShortVersionString` 三者一致；`CFBundleVersion` 单调递增。
2. 每个 `.dmg` 必须能在 `SHA256SUMS` 里找到同名成员，且 `sha256sum -c` 严格通过。
3. `.dmg` 内 `.app` 必须**完整**（含 `Contents/MacOS/`、`Contents/Resources/`、字体与图标），不得依赖构建机路径。
4. 冒烟校验在**解包后的独立进程**里跑：`soxu-desktop --version`、`soxu --version`、`soxu-daemon --version` 三项都必须返回 0 并输出版本号。

### 6.4 签名与公证的接入门槛

| 阶段 | 做法 | 门槛 | 失败关闭 |
| :--- | :--- | :--- | :--- |
| S0 · 内部 ad-hoc | `codesign --force --deep --sign - Soxu.app` | 无证书；仅内部验证 | 未 ad-hoc 签名的包**不得**进入任何对外渠道 |
| S1 · Developer ID | `codesign --force --options runtime --timestamp --sign "Developer ID Application: ..." Soxu.app` + `entitlements` | 需 Developer ID 证书与 CI 凭据 | `codesign --verify --deep --strict` 失败即中止发布 |
| S2 · 公证 | `xcrun notarytool submit ... --wait` + `xcrun stapler staple Soxu.app` | 需 App Store Connect API Key 或 Apple ID 专用密码 | `notarytool` 非 `Accepted`、或 `stapler validate` 失败即中止 |
| S3 · 分发校验 | `spctl --assess --type execute --verbose=4 Soxu.app` | S1+S2 已完成 | `spctl` 拒绝即不得发布 |

**硬红线**：

1. 无证书、无凭据、无公证时，**只能**产出 S0 ad-hoc 包，并且必须在发布说明里显式标注「未签名/未公证，可能被 Gatekeeper 拦截」；
2. 严禁把「移除 quarantine 属性」（`xattr -d com.apple.quarantine`）写进安装脚本或发布说明来绕过 Gatekeeper；
3. 严禁在缺凭据时把 S1/S2 步骤写成 `continue-on-error` 或 `|| true` 静默跳过；
4. 文档与 PR 不得声称「已签名/已公证」，除非有对应 run/job URL 与 `spctl` 输出。

### 6.5 必须保留的既有发布契约

1. 双仓联动：主仓 `v*` tag → `repository_dispatch` → `TanCan1998/soxu-releases` 构建 → 精确资产清单回传镜像；
2. 精确资产集合校验（缺失、额外、重名、嵌套路径、散列不符一律拒绝）；
3. `SHA256SUMS` 严格校验与 SLSA 溯源（在联动仓产出）；
4. 包管理器清单：Scoop / Homebrew / Winget；
5. Latest 标记的双重 `sort -V` 防竞态校正。

### 6.6 Desktop 发布的最小后续任务（不在本 PR）

1. `soxu-desktop` 接入 `gpui` + `gpui-component`（版本再次实测）；
2. 产出可复现的 `.app` 打包脚本（`cargo bundle` 或等价）；
3. 契约 PR：把 `.dmg` 加入 `EXPECTED_RELEASE_ASSETS`、`SHA256SUMS` 成员与包管理器清单；
4. 联动仓新增 macOS 打包与 ad-hoc 签名作业，并保留 Intel 原生实例；
5. 拿到 Developer ID 与公证凭据后，另起 PR 把 S1/S2/S3 接入同一契约。

---

## 7. 轻量 CI 计划（E）

### 7.1 本 PR 的变更面

产品 PR 只包含：`docs/dev/ci-and-testing-architecture.md`（新增）+ 本分支 AOCI 托管资产。

### 7.2 现有门禁契约（`ci-gate-plan/v1`）

固定 job 契约（**源码核验**，`scripts/verify-ci-gate.ps1`）：

| Job | 触发条件 | 本 PR 期望 |
| :--- | :--- | :--- |
| `Detect Check Scope` | 无条件 | 实跑 `success` |
| `Git Emoji & Privacy Guard` | 无条件 | 实跑 `success`（含 gitleaks v8.30.1 扫描、提交历史 Emoji 校验） |
| `Rust Format Check` | `rust == true` | 按计划 `skipped` |
| `Rust (<platform>)` | `rust == true`，按平台矩阵实例化 | 按计划 `skipped`（0 实例） |
| `Mobile Web/PWA Typecheck & Build` | `mobile_web == true` | 按计划 `skipped` |
| `CI Gate` | 无条件 | 实跑 `success`，核对上表「实跑」与「按计划跳过」 |

要点：`CI Gate` 不认聚合字符串，它从原始 run jobs 派生逐实例结论，并要求「必跑任务只认 `success`、计划内跳过的任务必须显式 `skipped`」。因此纯文档 PR 的绿灯**必须**表现为 `Rust Format` 与 `Rust (*)` 与 `Mobile Web` 三个作业显式 `skipped`，而不是「没出现」。

### 7.3 实际回读

**PR 首次推送（head `419bdad`）的 `CI` run：`46021033688`（conclusion `success`）**

实测门禁计划（`Detect Check Scope` 日志原文，`checkoutSha = e7df525`）：

```json
{"version":"ci-gate-plan/v1","pathContract":"repo-relative/v1","detector":"detect-changes","gate":"ci-gate",
 "jobs":[{"id":"governance-and-privacy","required":true},
         {"id":"rust-format","required":false,"depends_on":"rust"},
         {"id":"rust-check","required":false,"depends_on":"rust"},
         {"id":"mobile-web-check","required":false,"depends_on":"mobile_web"}],
 "selfTest":false,"releaseContract":false,
 "scope":{"schemaVersion":1,"full":false,"rust":false,"rustScope":"none","mobileWeb":false,
          "paseoOrchestration":false,"checkScopeSelfTest":false,"platforms":[],
          "changedFiles":[".aoci/baseline.json","aoci.code.txt","docs/dev/ci-and-testing-architecture.md"]}}
```

实际作业结论（`gh pr checks`）：

| 作业 | 计划 | 实际 | 耗时 |
| :--- | :--- | :--- | :--- |
| `Detect Check Scope` | 必跑 | `pass` | 7s |
| `Git Emoji & Privacy Guard` | 必跑 | `pass` | 13s |
| `Rust Format Check` | `required=false` | `skipping` | 0 |
| `Rust (${{ matrix.platform.id }})` | 矩阵 0 实例（`platforms=[]`） | `skipping` | 0 |
| `Mobile Web/PWA Typecheck & Build` | `required=false` | `skipping` | 0 |
| `CI Gate` | 必跑 | `pass` | 17s |

结论：

1. 纯文档变更在现有 `ci-gate-plan/v1` 下**不需要**任何 Rust/PWA 算力，`CI Gate` 仍按「计划内跳过必须显式 `skipped`」逐实例核对，未被空跑放行。
2. 本地 `scripts/check-all.ps1` 的范围结论（`rust=跳过`、`mobileWeb=False`、`paseo=False`）与远端检测器完全一致：本地跳过的阶段在远端同样是 `skipping`，不存在「本地跳过、远端空跑」的偏差。
3. `CI Gate` 校验的不是聚合字符串，而是从原始 jobs 派生的逐实例状态；本次 `Rust` 矩阵实例数为 0，因此不会产生「矩阵漏跑但无人发现」的缺口。

### 7.4 M4 引入 GPUI 后需要演进的点（本 PR 不做）

1. `soxu-desktop` 进入 Rust 矩阵后，`rust` 范围自动包含该 crate；`gpui` 的构建时长会显著拉长 `rust-check`，需要评估 `timeout-minutes: 45` 是否仍够（必要时提高到 60 并记录依据）；
2. `macos-arm64` 实例必须实际编译 `soxu-desktop`，不得因为「Darwin 交叉不方便」而用 `--exclude` 长期屏蔽；
3. 需要新增**独立**视觉回归 workflow（第 3.3 节），且必须与 `ci-gate-plan/v1` 的 job 契约一起演进，不得把视觉回归塞进 `rust-check` 掩盖失败面；
4. `cargo test --workspace` 会因 `test-support` feature 分裂出额外构建配置，需要在 `test_profiles` 里显式表达，避免「本地过、CI 不过」。

---

## 8. 失败与回退边界（汇总）

| 场景 | 行为 | 禁止 |
| :--- | :--- | :--- |
| L1 用例在 macOS Runner 失败 | 保留完整日志、run/job URL，结论写「失败」，另行拆分修复 | 重试掩盖、`#[ignore]`、`--no-fail-fast` 跳过 |
| Runner 无 GUI 会话 / 无 Metal 设备 | 记录 `cgsession` 与 `metal_*` 实测输出，结论限定为「该 Runner 受限」 | 用参考项目的成功 run 替代本项目实测 |
| 视觉回归无捕获通道 | L2 标记 `未验证`，先落地捕获方案与基线目录结构 | 把 L1 通过包装成「视觉回归通过」 |
| 字体/DPI/主题不可固定 | 该用例不进入 L2，转为 L1 几何断言或 L3 人工检查 | 放宽像素阈值直到变绿 |
| Intel 原生 Runner 不可用 | 发布说明写「Intel 未原生验证」 | 用交叉编译或 Rosetta 宣称 Intel 支持 |
| 无签名/公证凭据 | 只产出 ad-hoc 包并显式标注 | 移除 quarantine、`continue-on-error`、声称已签名 |
| §6.4 任一校验失败 | 发布中止（失败关闭） | 手工补签后继续、跳过 `spctl` |
| 发布 workflow 或资产契约需变更 | 另起契约 PR + 双仓联动验证 | 在文档 PR 或功能 PR 里顺手改 `release.yml` |

---

## 9. 参考采用点与不适用原因（PR 证据）

| 参考 | 实际采用点 | 落点 | 不适用/回退原因 |
| :--- | :--- | :--- | :--- |
| Zed `a0e666a`（`examples/testing.rs`） | `#[gpui::test]` + `TestAppContext` / `VisualTestContext` 的双上下文分工；`run_until_parked` 的调度语义；`allow_parking()` 的跨线程边界 | §3.2 能力表、实验夹具 8 项用例 | 上游示例依赖 `gpui_platform` crate，该 crate **不在 crates.io**（实测 `cargo info` 报错），Soxu 只使用 `gpui` 本身 |
| Zed `docs/contexts.md` | `TestAppContext` 与异步上下文的差别（panic 而非 `Result`） | §3.2 | — |
| `gpui-shot` `a12f7ef` | 视觉回归的**双实例矩阵形态**（Linux/lavapipe 权威 + macOS/Metal 参考）、`GPUI_SHOT_UPDATE` 式显式基线更新语义 | §3.3 路线 1/3、§6.4 S2 | 其补丁与 Zed `v0.224.8` 绑定；Soxu 版本不同，不能直接套用补丁，且 run `32943744606` 只证明其自身流程有记录 |
| `gpuix` notes（经 `gpui-shot` 文档） | 捕获顺序：更新视图 → `window.refresh()` → `run_until_parked()` → 抓取 → 同步落盘；根实体必须活到捕获结束；macOS 需开 `font-kit` 供字形 | §3.3 固定变量与捕获顺序 | 基于打过补丁的 fork，不符合 Soxu「纯上游依赖」路线，仅作顺序与生命周期依据 |
| `gpui-testing-library` `eb3521f` | 「按用户可感知方式查询视图」的断言取向（对应本项目 `debug_selector` + 语义断言） | §3.2、§4 | 其 `gpui-pre =0.3.9` / `gpui-kit =0.7.3` 强耦合，不兼容 `gpui 0.2.2` |
| Padu `fbe97a3`（CodeGraph + AOCI） | Harness 根视图、统一 `setup_*` 装配、IME 直接驱动 `EntityInputHandler`、几何断言优先于像素断言、语义化测试命名 | §2.4、§3.2、§5.2 | Padu 使用 `bezel-gpui 0.3.21`（`gpui` 重命名包），API 与 0.2.2 有实测差异（`TestAppContext::new` 可见性、`FocusHandle::focus` 参数个数），只能借鉴模式 |

参考索引使用方式（**只读复用，未复制、未初始化、未扫描、未重建、未维护**）：

```powershell
# AOCI：显式绑定 Padu 根，读取 input.rs / browser.rs / theme.rs / app.rs 条目
D:/Programs/AOCI/aoci.exe --repo <padu-root> mcp

# CodeGraph：显式绑定 Padu 根，定位测试符号与其调用面
codegraph explore --path <padu-root> "set_content_with_same_length_text_re_arms_caret_follow ime_composition_after_ascii_keeps_its_caret"
codegraph explore --path <padu-root> "test_address_resolution"
```

固定上游文件的取数方式（不使用 `curl` / `Invoke-RestMethod` 直连）：

```powershell
gh api "repos/zed-industries/zed/contents/crates/gpui/examples/testing.rs?ref=a0e666ae930a85ab38947e2d49222b0ea99f1c2f" -H "Accept: application/vnd.github.raw"
gh api "repos/AprilNEA/gpui-shot/contents/docs/gpuix-notes.md?ref=a12f7efd1a7abdc66ff16c6bcf43572c868d0efd" -H "Accept: application/vnd.github.raw"
```

---

## 10. 官方与上游核验入口

| 目的 | 入口 |
| :--- | :--- |
| GPUI 测试宏与示例 | `zed-industries/zed@a0e666ae` → `crates/gpui/examples/testing.rs` |
| 上下文语义 | 同上 → `crates/gpui/docs/contexts.md` |
| GPUI 版本与 feature | `cargo info -q --registry crates-io gpui` |
| `gpui-component` 版本 | `cargo info -q --registry crates-io gpui-component` |
| 视觉回归矩阵参考 | `AprilNEA/gpui-shot@a12f7efd` → `.github/workflows/visual.yml` |
| 捕获实现约束 | 同上 → `docs/gpuix-notes.md` |
| 本仓 CI 门禁契约 | `scripts/verify-ci-gate.ps1`、`scripts/detect-check-scope.ps1`、`.github/workflows/ci.yml` |
| 本仓发布契约 | `.github/workflows/release.yml` |

---

## 11. 未完成事项（明确列为待办）

1. **视觉回归管线未建立**：`gpui 0.2.2` 无捕获 API，需选定第 3.3 节的路线并自带补丁/fork 决策；
2. **真机 L3 全部未做**：本章只定义检查项，尚无任何真机证据；
3. **Intel 原生 Desktop 构建未验证**：`soxu-desktop` 尚未接入 GPUI，无从构建；
4. **签名/公证未接入**：无证书与凭据，S1–S3 全部未执行；
5. **属性测试（`iterations`）未落地**：仅在 §3.2 记录能力，未见项目内用例；
6. **视觉基线目录与命名未落地**：需要在 M4 的第一个视觉 PR 中确定并写入本文件。
