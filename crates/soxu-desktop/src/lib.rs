//! soxu-desktop 库目标。
//!
//! 目录结构：
//! - [`theme`]：由 `design-system/soxu/tokens.json` 编译生成的设计令牌常量，
//!   通过库目标公开给二进制入口与后续 GPUI 视图复用。
//!
//! 二进制入口见 `main.rs`。本阶段只落设计令牌底座，不引入 gpui / gpui-component，
//! 也不实现任何桌面视图。

pub mod theme;
