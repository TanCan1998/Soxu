//! soxu-desktop 库目标。
//!
//! 目录结构（M4-W2B 壳层底座）：
//! - [`theme`]：由 `design-system/soxu/tokens.json` 编译生成的设计令牌常量、
//!   GPUI 颜色映射消费层与全局主题状态（明暗切换/跟随系统）。
//! - [`font`]：双开源字体预设与 `effective_code_family()` 语义。
//! - [`i18n`]：最小可扩展 i18n 骨架（key→文案 + 语言注册表，中文默认）。
//! - [`focus`]：7 层焦点上下文栈枚举与切换规则（纯逻辑）。
//! - [`layout`]：400/640/1024 窗口断点与四栏布局策略（纯逻辑）。
//! - [`shell`]：四栏自适应工作台骨架视图（占位面板）。
//!
//! 二进制入口见 `main.rs`。

pub mod focus;
pub mod font;
pub mod i18n;
pub mod layout;
pub mod shell;
pub mod theme;

/// 设计令牌直通再导出：视图代码统一从 crate 根取布局/尺寸常量。
pub use theme::tokens;
