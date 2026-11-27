//! Soxu 桌面端主题层。
//!
//! 当前只包含 [`tokens`]：由 `design-system/soxu/tokens.json` 编译出的零漂移令牌常量
//! （颜色、间距、尺寸、断点、层叠、动效、排版）。主题结构体与 GPUI 组件皮肤将在 M4
//! 基于这些常量组装，本模块不引入任何第三方依赖。

pub mod tokens;
