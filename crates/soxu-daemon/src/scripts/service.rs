//! 脚本域命令处理入口（`ScriptCommand` → `ScriptCommandResult`，1:1 对应协议变体）。
//!
//! 门控在最前面：`managed_scripts` 未声明时整族命令一律拒绝，且不下发任何事件。
//! 传输层（`RpcCommand` 转发）属 `soxu-protocol` 冻结范围，待契约补齐后直接调用本入口。

use crate::scripts::gate::FeatureGate;
use crate::scripts::manifest::{PASEO_MANIFEST_FILE, ScriptManifest};
use crate::scripts::runner::{RunScriptOptions, ScriptRunner};
use soxu_protocol::{
    ActionableError, DaemonFeature, ErrorCode, ScriptCommand, ScriptCommandResult,
    ScriptManifestListing,
};
use std::path::Path;
use std::sync::Arc;

/// 受管脚本域服务。
pub struct ScriptService {
    runner: ScriptRunner,
}

impl std::fmt::Debug for ScriptService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScriptService").finish_non_exhaustive()
    }
}

impl ScriptService {
    /// 构造脚本域服务；门控缺省全关。
    #[must_use]
    pub fn new(gate: FeatureGate, sink: Arc<dyn crate::scripts::ScriptEventSink>) -> Self {
        Self {
            runner: ScriptRunner::new(gate, sink),
        }
    }

    /// 当前门控。
    #[must_use]
    pub fn gate(&self) -> &FeatureGate {
        self.runner.gate()
    }

    /// 执行器（供宿主查询输出与回收结论）。
    #[must_use]
    pub fn runner(&self) -> &ScriptRunner {
        &self.runner
    }

    /// 处理一条脚本域命令。
    ///
    /// `workspace_root` 由宿主按 `workspace_id` 解析并授权，服务层只做**真实路径**
    /// 复核与执行前校验；解析失败一律拒绝，不做猜测式降级。
    pub fn handle(
        &self,
        workspace_root: &Path,
        command: ScriptCommand,
    ) -> Result<ScriptCommandResult, ActionableError> {
        self.runner.gate().require(DaemonFeature::ManagedScripts)?;
        match command {
            ScriptCommand::List(request) => {
                let manifest = ScriptManifest::load(workspace_root)?;
                Ok(ScriptCommandResult::Listed(ScriptManifestListing {
                    workspace_id: request.workspace_id,
                    manifest_file: PASEO_MANIFEST_FILE.to_owned(),
                    scripts: manifest.scripts().to_vec(),
                }))
            }
            ScriptCommand::Run(request) => {
                let manifest = ScriptManifest::load(workspace_root)?;
                let Some(entry) = manifest.get(&request.script_id) else {
                    return Err(ActionableError::new(
                        ErrorCode::NotFound,
                        format!("工作区清单中没有脚本 `{}`", request.script_id.as_str()),
                        "该请求未执行，未派生子进程",
                        "请先调用脚本清单查询，确认 script_id 后再启动",
                    ));
                };
                let snapshot = self.runner.run(
                    workspace_root,
                    entry,
                    RunScriptOptions {
                        user_arguments: request.user_arguments,
                        idempotency_key: request.idempotency_key,
                        approval_id: request.approval_id,
                    },
                )?;
                Ok(ScriptCommandResult::Accepted(snapshot))
            }
            ScriptCommand::Stop(request) => {
                let snapshot = self.runner.stop(&request.run_id, request.force)?;
                Ok(ScriptCommandResult::Snapshot(snapshot))
            }
            ScriptCommand::Inspect(request) => {
                let snapshot = self.runner.inspect(&request.run_id)?;
                Ok(ScriptCommandResult::Snapshot(snapshot))
            }
        }
    }
}
