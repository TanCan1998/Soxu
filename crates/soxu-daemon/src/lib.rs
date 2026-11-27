//! Soxu 守护进程库导出，供可嵌入运行及端到端集成测试使用。

pub mod antigravity;
pub mod native_assets;
pub mod pairing;
pub mod server;
pub mod state;
pub mod store;
pub mod web_assets;

pub use native_assets::{NativeAssetError, NativeAssetGuard};
pub use server::build_router;
pub use state::DaemonState;
pub use store::SqliteStore;
pub use web_assets::build_web_assets_router;
