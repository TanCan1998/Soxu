//! Soxu 守护进程库导出，供可嵌入运行及端到端集成测试使用。

pub mod antigravity;
pub mod native_assets;
pub mod pairing;
pub mod pty;
pub mod schedules;
pub mod scripts;
pub mod server;
pub mod state;
pub mod store;
pub mod web_assets;

pub use native_assets::{NativeAssetError, NativeAssetGuard};
pub use pty::{
    NullPtyEventSink, PortablePtyBackend, PtyBackend, PtyChild, PtyControl, PtyEventSink, PtyHost,
    PtySpawnSpec,
};
pub use schedules::{
    Clock as ScheduleClock, FinishOutcome, ManualClock, NullScheduleEventSink, ScheduleEventSink,
    ScheduleService, ScheduleStore, StoredSchedule, SystemClock, TickReport,
};
pub use scripts::{
    FeatureGate, NullScriptEventSink, ReapOutcome, RunOutput, RunScriptOptions, ScriptEventSink,
    ScriptManifest, ScriptRun, ScriptRunner, ScriptService,
};
pub use server::build_router;
pub use state::DaemonState;
pub use store::SqliteStore;
pub use web_assets::build_web_assets_router;
