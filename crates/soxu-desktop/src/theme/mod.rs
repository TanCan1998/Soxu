//! Soxu 桌面端主题层。
//!
//! - [`tokens`]：由 `design-system/soxu/tokens.json` 编译出的零漂移令牌常量
//!   （颜色、间距、尺寸、断点、层叠、动效、排版）。
//! - [`gpui_colors`]：把上述 `u32` 颜色令牌转换为 GPUI 颜色类型的最小消费层。
//!
//! 主题结构体与完整 GPUI 组件皮肤将在后续 M4 任务基于这两层组装。

pub mod gpui_colors;
pub mod tokens;
