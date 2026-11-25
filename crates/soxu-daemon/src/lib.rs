//! Soxu 守护进程库导出，供可嵌入运行及端到端集成测试使用。

pub mod pairing;
pub mod server;
pub mod state;
pub mod store;

pub use server::build_router;
pub use state::DaemonState;
pub use store::SqliteStore;
