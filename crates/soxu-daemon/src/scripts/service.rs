//! 脚本域命令处理入口（`ScriptCommand` → `ScriptCommandResult`，1:1 对应协议变体）。
//!
//! 门控在最前面：`managed_scripts` 未声明时整族命令一律拒绝，且不下发任何事件。
//! 全局 `RpcCommand` 的 14 个 M6 变体由守护进程折叠成 `RpcDomainCommand` 后
//! 1:1 转发到本入口（见 `crate::state::DaemonState::dispatch_domain_command`）。

use crate::scripts::gate::FeatureGate;
use crate::scripts::manifest::{PASEO_MANIFEST_FILE, ScriptManifest};
use crate::scripts::runner::{RunScriptOptions, ScriptRunner};
use soxu_protocol::{
    ActionableError, DaemonFeature, ErrorCode, ScriptCommand, ScriptCommandResult,
    ScriptManifestListing,
};
use std::path::Path;
use std::sync::Arc;

/// 需要工作区根的命令在缺根时给出可执行拒绝，绝不以当前目录或空路径顶替。
fn require_workspace_root(workspace_root: Option<&Path>) -> Result<&Path, ActionableError> {
    workspace_root.ok_or_else(|| {
        ActionableError::new(
            ErrorCode::ValidationFailed,
            "该脚本命令必须绑定工作区根目录",
            "该请求未执行，未读取清单也未派生子进程",
            "请由守护进程按 workspace_id 解析受控目录后重试",
        )
    })
}

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
    ///
    /// 只有 `List` / `Run` 需要工作区根；`Stop` / `Inspect` 以 `run_id` 定位运行，
    /// 传 `None` 即可——停掉一个已经失去工作区的失控进程树不能被路径复核挡住。
    pub fn handle(
        &self,
        workspace_root: Option<&Path>,
        command: ScriptCommand,
    ) -> Result<ScriptCommandResult, ActionableError> {
        self.runner.gate().require(DaemonFeature::ManagedScripts)?;
        match command {
            ScriptCommand::List(request) => {
                let workspace_root = require_workspace_root(workspace_root)?;
                let manifest = ScriptManifest::load(workspace_root)?;
                Ok(ScriptCommandResult::Listed(ScriptManifestListing {
                    workspace_id: request.workspace_id,
                    manifest_file: PASEO_MANIFEST_FILE.to_owned(),
                    scripts: manifest.scripts().to_vec(),
                }))
            }
            ScriptCommand::Run(request) => {
                let workspace_root = require_workspace_root(workspace_root)?;
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
                    &request.workspace_id,
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
