//! Soxu 桌面端主题层。
//!
//! - [`tokens`]：由 `design-system/soxu/tokens.json` 编译出的零漂移令牌常量
//!   （颜色、间距、尺寸、断点、层叠、动效、排版）。
//! - [`gpui_colors`]：把上述 `u32` 颜色令牌转换为 GPUI 颜色类型的最小消费层。
//! - [`theme_state`]：全局主题状态（明暗切换/跟随系统），色板仍由 `gpui_colors` 提供。
//!
//! 完整 GPUI 组件皮肤将在 M5 基于这三层组装。

pub mod gpui_colors;
pub mod theme_state;
pub mod tokens;
