# 07 · 极简悬停交互、独立设置界面、开源双字体与 GPUI 动效规范

> **继承主规范**：[`../MASTER.md`](../MASTER.md)  
> **目标模块**：`crates/soxu-desktop`（GPUI + `gpui-component`） & `apps/mobile-web`（React 19 + `shadcn/ui` + Tailwind CSS v4 + `lucide-react`）  
> **核心原则**：克制收敛（默认隐藏低频噪音）、按需展开（Hover/Focus 流畅揭示）、动效有源（每帧 CSS 动效均提供 1:1 GPUI Rust 对照算法）

---

## 一、极简美学与悬停自动展开（Hover-Expand / Hover-Reveal）规范

为了保持主工作区近似 ChatGPT / Linear 的极净呼吸感，**真实界面顶栏严禁出现任何多页面/多视角演示切换栏**；所有非每分钟点击的次级控制统一遵循两类极简收纳模式：

### 1.1 收纳 Dock（`HoverExpandDock`）防抖状态机
用于顶栏次级工具（Magpie 诊断、Schedules、扫码配对、CLI 历史导入）以及输入舱底部参数槽（CLI 驱动、思考档位、权限模式、`@agent` 移交）：
- **静息态（Collapsed）**：只展示 1 枚线性图标（`16px` Lucide 图标 + 可选 `6px` 语义状态点，例如 `LayoutGrid` 或 `SlidersHorizontal`），总宽 ≤ `32px`。
- **触发条件（Expand Triggers）**：
  1. 鼠标指针划入触发区域并停留超过 **`40ms`**（防止鼠标快速掠过顶部误开闪烁）；
  2. 键盘 `Tab` 焦点落入容器内部任意子按钮（`:focus-within` 等价保障无障碍可用性）；
  3. 触控屏幕或移动端轻触唤醒并由外部点击收起。
- **退场防误触锁定（Collapse Guard）**：鼠标移出后延迟 **`160ms`** 才平滑收起；若容器内已打开 `<Select>`、`<DropdownMenu>` 或子 `<Tooltip>` 浮层，强制锁定保持展开态直至浮层关闭。
- **物理动画曲线**：横向宽度/最大宽度与子元素 `opacity`/`translateX(-4px → 0)` 在 `220ms cubic-bezier(0.16, 1, 0.3, 1)` 内同步展开。

### 1.2 行级渐进揭示（Row Hover-Reveal）
- 左栏 Worktree 行的 `[新建会话]` / `[打开右侧目录]` 操作图标、对话消息右上角的 `[复制全文]` / `[分叉对话]` 操作簇、以及 Checkpoint 锚点右侧 `[查看 Diff]` 按钮，默认保持 `opacity: 0`（或极淡提示），仅当行处于 `:hover` 或 `:focus-within` 时在 `150ms` 内淡入 `opacity: 1`。

---

## 二、7 分区独立设置界面规范（`SettingsDialog` · `Ctrl+,` / `Cmd+,`）

### 2.1 容器几何与触发入口
- **唯一显性主入口**：主窗口右上角与移动端顶栏的齿轮图标 button（`Settings` 16px Lucide 图标，无多余文字，配 `<Tooltip>` 提示 `设置 (Ctrl+,)`）。
- **桌面端形态**：`820px × 580px` 居中 `shadcn/ui` `<Dialog>`，左侧 `200px` 分区导航轨（`#0D0D0D`） + 右侧平滑滚动内容区（`#171717`）；
- **移动端形态**：宽 `< 640px` 自动降级为全屏舒适双段滚动抽屉（顶部分组胶囊条 + 下方分组开关列表），所有选项点击高度 ≥ `44px`。

### 2.2 7 大设置分区详情

| 分区序号 | 分区名称 | Lucide 图标 | 核心配置项清单 |
| :---: | :--- | :--- | :--- |
| **1** | **外观与主题** | `Palette` | 主题模式（ChatGPT Pure Black / Clean Light / 跟随 OS）、界面密度（紧凑 `32px` 行 / 舒适 `38px` 行）、高可见聚焦轮廓与减弱动效开关 |
| **2** | **字体与排版** | `Type` | 开源界面字体、UI 基准字号（`13~16px`）、**「代码块字体跟随界面字体」一键开关**、开源代码块字体、代码字号（`12~16px`）、连字（`liga/calt`）开关 + **实时双语混排 & Rust 代码预览框** |
| **3** | **Agent 与 Magpie** | `Cpu` | `MagpieManaged` / `StandaloneNative` 实时检测、MCP `Shared` vs `Direct` 路由清单、7 大原生 CLI 健康矩阵、Worktree 启动环境变量脱敏透视 |
| **4** | **工作区与 Git** | `GitBranch` | 默认主基准分支（`origin/main`）、共享构建缓存路径提示、对话轮次自动生成 Checkpoint 开关、Git Emoji AI 提交生成模型选择 |
| **5** | **安全与审批红线** | `ShieldAlert` | 默认权限档位（`Ask` / `Auto-Edit` / `Full-Access`）、高危破坏性命令（递归删除/强推/全局配置覆写）只读锁定提示、防隐私泄露检查状态 |
| **6** | **移动遥控与配对** | `Smartphone` | 局域网/加密中继开关、高对比度纯白留白扫码区、6 位动态配对码刷新与一键复制、已配对手机列表及单台吊销 |
| **7** | **快捷键与关于** | `Keyboard` | 全局键盘快捷表、版本号与协议版本 `v1`、重开 3 步 Onboarding 向导、折叠式「设计令牌系统校验详情」 |

---

## 三、开源免费双字体联动与「代码块跟随界面字体」规范

全部字体选用 **SIL Open Font License 1.1** 开源免费字体，支持离线系统字体回退与无缝热切换：

### 3.1 界面无衬线 / 通用中文栈（`uiPresets`）
1. **`inter-noto`（默认）**：`'Inter', 'Noto Sans SC', 'HarmonyOS Sans SC', system-ui, sans-serif`
2. **`ibm-plex-noto`**：`'IBM Plex Sans', 'Noto Sans SC', 'HarmonyOS Sans SC', system-ui, sans-serif`
3. **`jakarta-noto`**：`'Plus Jakarta Sans', 'Noto Sans SC', 'HarmonyOS Sans SC', system-ui, sans-serif`
4. **`lxgw-wenkai`（霞鹜文楷）**：`'Inter', 'LXGW WenKai Screen', 'LXGW WenKai', 'Noto Sans SC', sans-serif`
5. **`system-sans`**：操作系统原生默认无衬线字体回退

### 3.2 代码块 & 终端等宽栈（`codePresets`）
1. **`jetbrains-mono`（默认）**：`'JetBrains Mono', 'Noto Sans SC', monospace`
2. **`fira-code`**：`'Fira Code', 'Noto Sans SC', monospace`
3. **`maple-mono`（中英文严格 2:1 对齐）**：`'Maple Mono NF CN', 'Maple Mono', 'JetBrains Mono', 'Noto Sans SC', monospace`
4. **`ibm-plex-mono`**：`'IBM Plex Mono', 'Noto Sans SC', monospace`
5. **`cascadia-code`**：`'Cascadia Code', 'JetBrains Mono', 'Noto Sans SC', monospace`

### 3.3 独立 / 跟随联动契约（CSS & GPUI 双端同构）
- 根样式维护三枚动态变量：`--soxu-font-ui`（当前界面字体）、`--soxu-font-code`（独立选择的代码字体）以及实际生效变量 `--soxu-font-code-effective`。
- 当设置项 **`codeFollowsUi = true`** 时：
  - CSS 侧立即令 `--soxu-font-code-effective: var(--soxu-font-ui)`，并在设置页面禁用（视觉置淡至 `opacity: 0.55`）独立代码字体下拉框，标注「当前已跟随界面字体」；
  - GPUI 桌面端同步在 `Theme` / `FontConfig` 解析函数 `effective_code_family()` 中直接返回 `self.ui_font_family`（保留 `tabular-nums` 等宽数字特性），确保所有内联代码、Diff 视图、工具卡片与终端输出瞬间切换。

---

## 四、纯图标主题切换按钮与径向圆扩过渡

1. **视觉纯化**：顶栏主题按钮固定尺寸 `30×30px`（移动端 `36×36px` 伴 `44px` 命中热区），仅包含居中叠加的 Lucide **`Sun`**（`#ECECEC`）与 **`Moon`**（`#0D0D0D`）矢量图标，**不显示文字标签**。
2. **双图标旋变动画**：进入深色时 `Moon` 从 `rotate(-90deg) scale(0.55) opacity(0)` 转正为 `0deg scale(1) opacity(1)`（`280ms cubic-bezier(0.16, 1, 0.3, 1)`），同时 `Sun` 逆向缩隐。
3. **整屏过渡过渡机制**：
   - Web / PWA 优先使用 `document.startViewTransition` 获取主题按键圆心 `(cx, cy)` 与覆盖半径，执行 `clip-path: circle(0 → R)` `340ms` 纯滑扩展过渡；
   - 不支持 View Transitions 时自动执行 `220ms` 全表面 `background-color` / `border-color` / `color` 同步插值；
   - **GPUI 桌面端对应方案**：通过窗口级 `Animation::new(Duration::from_millis(220)).with_easing(ease_out_expo)` 驱动 `from_theme.blend(to_theme, delta)`，将 `Hsla` 背景、卡片与文本颜色按 `delta` 平滑插值重绘窗口，无需截图拷贝即可实现丝滑冷暖切换！

---

## 五、GPUI `with_animation` ↔ CSS 1:1 可移植动效对照矩阵

为保证 M4 阶段 Rust GPUI 开发能毫无阻力地复现原型动效，原型禁用任何依赖 CSS 复杂 `backdrop`/`box-shadow` 魔法、或难以在 GPUI 布局引擎落地的动画，统一锁定如下对照矩阵：

| 交互场景 | Web / `shadcn/ui` 实现 | 耗时 & 缓动曲线 | GPUI (`gpui-component` / `gpui::Animation`) 1:1 映射 |
| :--- | :--- | :--- | :--- |
| **主题径向 / 全局色阶过渡** | View Transition 扩圆 / `220ms` 颜色过渡 | `340ms` / `220ms` `ease_out_expo` | `Theme::blend(old, new, delta)` + `with_animation(220ms)` |
| **Sun ↔ Moon 图标切换** | `rotate` + `scale` + `opacity` | `280ms` `ease_out_expo` | `svg().with_transformation(Transformation::rotate(...))` |
| **HoverExpandDock 展开** | `40ms` 入延 → `max-width` & `opacity` | `220ms` `ease_out_expo` | 状态变量 + `cx.background_executor` 延迟防抖 + `width` 插值 |
| **深度思考 / 工具卡手风琴** | Radix `Collapsible` `grid-template-rows` / height | `200ms` `ease_out_expo` | `overflow_hidden` + 动态 `max_h(full_h * delta)` |
| **左右侧栏推拉折叠** | `width` + `opacity` 过渡 | `220ms` `ease_out_expo` | `panel_width * delta`，拖拽期间暂停动画防拖滞 |
| **会话 Tab 激活指示线** | 顶部 `2px` 指标平移/宽度渐变 | `180ms` `ease_fast` | Tab `x` 与 `w` 插值渲染顶部高亮条 |
| **Cmd+K / Settings Dialog** | Radix `Dialog` `scale(0.96→1)` + 遮罩淡入 | 入 `220ms` / 出 `150ms` | Scrim `opacity * delta` + 弹窗中心缩放位移 |
| **审批卡片消解坍缩** | 待确认态 → 单行审计态高度收拢 | `200ms` `ease_in_out` | 行容器高度线性插值至 `34px` |
| **减弱动效降级** | `@media (prefers-reduced-motion)` + 设置开关 | `0ms` 即时完成 | `if reduce_motion { delta = 1.0 }` 跳过插值帧 |

---

## 六、开源图标统一纪律（`lucide-react` ↔ `gpui-component` SVG）

1. **Web / 原型侧**：全量使用 **`lucide-react`**，统一 `strokeWidth={1.75}`，三档规范尺寸：**`14px`**（内联芯片/树微标）、**`16px`**（标题栏/工具栏/按钮图标）、**`20px`**（移动底栏与空状态图标）；严禁混用手搓 `<svg>`、位图 PNG 或 Emoji。
2. **Rust GPUI 侧**：从同源 **Lucide ISC SVG 图标集**打包内嵌至桌面 `Assets`（按 `icons/<kebab-name>.svg` 映射，保持与 Web 端 **100% 相同的图标语义命名与 `1.75px` 线性风格**）。

---

## 七、GPUI Rust 动效实现契约与设置持久化 Schema

### 7.1 GPUI `with_animation` 参考缓动曲线（Rust 同构）
在 `crates/soxu-desktop` 中，所有过渡动画不得使用重排高开销滤镜，直接基于以下与 `tokens.json -> motion` 数值完全等价的缓动函数插值：

```rust
/// 对应 CSS: cubic-bezier(0.16, 1.0, 0.3, 1.0) · 220ms
pub fn ease_out_expo(t: f32) -> f32 {
    if t >= 1.0 { 1.0 } else { 1.0 - 2.0f32.powf(-10.0 * t) }
}

/// 对应 CSS: cubic-bezier(0.2, 0.0, 0.0, 1.0) · 150ms
pub fn ease_fast_out(t: f32) -> f32 {
    let inv = 1.0 - t.clamp(0.0, 1.0);
    1.0 - inv * inv * inv
}
```

- **主题切换色阶平滑过渡**：在根视图维护 `theme_transition: Option<(ThemeSnapshot, ThemeSnapshot, f32)>`，每次切换调用 `ThemeSnapshot::lerp(&from, &to, ease_out_expo(delta))` 直接混合背景、描边、卡片与前景色，零额外 GPU 纹理分配。
- **拖拽防冲突原则**：当用户按住左栏/右栏分隔条拖拽宽度时，瞬时切换至实时宽度绑定并暂停 `220ms` 缓动动画，松手或点击折叠按钮时才恢复动画，消除拖动迟滞感。

### 7.2 双端统一用户偏好 Schema（`preferences.json` / `localStorage`）
桌面端与移动遥控端共用一致的偏好键名与默认值（支持跨端同步）：
- `themeMode`: `"dark" | "light" | "system"`（默认 `"dark"` 即 ChatGPT Pure Black）；
- `density`: `"compact" | "comfortable"`（决定工作区树行高 `32px` vs `38px`）；
- `uiFontPreset`: `"inter-noto" | "ibm-plex-noto" | "jakarta-noto" | "lxgw-wenkai" | "system-sans"`；
- `codeFollowsUi`: `boolean`（默认 `false`；设为 `true` 时代码块与终端强行继承界面字体族）；
- `codeFontPreset`: `"jetbrains-mono" | "fira-code" | "maple-mono" | "ibm-plex-mono" | "cascadia-code"`；
- `uiFontSize`: `13 | 14 | 15 | 16`（默认 `14`）；
- `codeFontSize`: `12 | 13 | 14 | 15 | 16`（默认 `13`）；
- `codeLigatures`: `boolean`（默认 `true`）；
- `reduceMotion`: `boolean`（默认跟随操作系统媒体查询 `prefers-reduced-motion`）。
