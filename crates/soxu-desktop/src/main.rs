//! Soxu GPUI 桌面工作台入口（将在 M0.5 设计系统落盘后，于 M4 接入 gpui-component v0.11+）。

use soxu_protocol::PROTOCOL_VERSION;

fn main() {
    println!(
        "Soxu Desktop v{} (protocol v{}) — GPUI 工作台将在 M0.5 设计系统完成后挂载",
        env!("CARGO_PKG_VERSION"),
        PROTOCOL_VERSION
    );
}
