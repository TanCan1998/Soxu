//! M6-A 受管脚本域：能力门控、`paseo.json` 只读清单、受管执行器与命令处理入口。

pub mod gate;
pub mod manifest;
pub mod runner;
pub mod service;

pub use gate::{FeatureGate, feature_label};
pub use manifest::{
    PASEO_MANIFEST_FILE, PASEO_MANIFEST_MAX_BYTES, SCRIPT_MANIFEST_MAX_ENTRIES, ScriptManifest,
    child_facing_path, resolve_workdir,
};
pub use runner::{
    NullScriptEventSink, ReapOutcome, RunOutput, RunScriptOptions, SCRIPT_MAX_CONCURRENT_RUNS,
    SCRIPT_MAX_RETAINED_RUNS, SCRIPT_OUTPUT_LIMIT_BYTES, SCRIPT_REAP_WINDOW, ScriptEventSink,
    ScriptRun, ScriptRunner,
};
pub use service::ScriptService;
