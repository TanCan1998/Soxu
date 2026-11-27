//! soxu-desktop 库目标。
//!
//! 目录结构：
//! - [`theme`]：由 `design-system/soxu/tokens.json` 编译生成的设计令牌常量，
//!   以及把颜色令牌映射为 GPUI 颜色类型的最小消费层，
//!   通过库目标公开给二进制入口与后续 GPUI 视图复用。
//!
//! 二进制入口见 `main.rs`。本阶段只落设计令牌底座与 GPUI 颜色映射，不实现任何桌面视图。

pub mod theme;
