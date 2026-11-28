//! soxu-desktop 库目标。
//!
//! 目录结构（M4-W2B 壳层底座）：
//! - [`theme`]：由 `design-system/soxu/tokens.json` 编译生成的设计令牌常量、
//!   GPUI 颜色映射消费层与全局主题状态（明暗切换/跟随系统）。
//! - [`font`]：双开源字体预设与 `effective_code_family()` 语义。
//! - [`i18n`]：最小可扩展 i18n 骨架（key→文案 + 语言注册表，中文默认）。
//! - [`focus`]：7 层焦点上下文栈枚举与切换规则（纯逻辑）。
//! - [`layout`]：400/640/1024 窗口断点与四栏布局策略（纯逻辑）。
//! - [`timeline`]：M5-A 虚拟对话流、工具/审批/提问卡、检查点锚点与轮次导航轨
//!   （`<=8.3Hz` 合批、虚拟窗口数学、20px 导航轨；纯逻辑 + 薄渲染）。
//! - [`composer`]：760px 同步宽输入舱（权限/移交/圆环遥测/芯片补全，纯内存视图模型）。
//! - [`right_panel`]：右栏上下文与审阅检视器（`Diff`/`Files` 页签、行号槽与 400 行截断，纯内存视图模型）。
//! - [`settings`]：7 分区设置弹窗（分区导航、几何降级与焦点捕获/还原）。
//! - [`modals`]：核心业务模态宿主（提交/配对/回退/透视，含焦点锁定与脱敏契约）。
//! - [`search`]：M6-B `Cmd/Ctrl+K` 五分区检索（固定顺序 + `90ms` 防抖 + `24` 条 LRU +
//!   前缀优先 + 每区 8 行）、内存符号补全与标题栏触发条（纯内存视图模型）。
//! - [`onboarding`]：M6-B 按需三步首次引导（Magpie/CLI 自检、项目绑定、遥控与快捷键；
//!   可跳过可完成，完成后不再自动弹出）。
//! - [`shell`]：四栏自适应工作台骨架视图（中栏：时间线在上、输入舱在下；右栏：审阅检视器）。
//!
//! 二进制入口见 `main.rs`。

pub mod composer;
pub mod focus;
pub mod font;
pub mod i18n;
pub mod layout;
pub mod modals;
pub mod onboarding;
pub mod right_panel;
pub mod search;
pub mod settings;
pub mod shell;
pub mod theme;
pub mod timeline;

/// 设计令牌直通再导出：视图代码统一从 crate 根取布局/尺寸常量。
pub use theme::tokens;
