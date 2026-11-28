//! 受管脚本（`paseo.json`）清单、生命周期请求与事件契约（M6 前置契约）。
//!
//! 设计红线：脚本命令只允许「程序 + 固定参数 + 受控透传参数」的结构化描述。
//! 协议层不提供裸 shell 字符串字段，也不接受 shell 解释器作为程序，
//! 更拒绝参数中的换行与 NUL，从契约层消灭命令拼接注入面。
//!
//! 本模块只定义契约：脚本发现、执行、回收全部由后续 daemon 实现 PR 落地。

use crate::primitives::{EnvKey, IdempotencyKey, WorkspaceRelativePath};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// 脚本标识最大长度（字节）。
pub const SCRIPT_ID_MAX_LEN: usize = 64;
/// 程序路径最大长度（字节）。
pub const SCRIPT_PROGRAM_MAX_LEN: usize = 260;
/// 单个参数最大长度（字节）。
pub const SCRIPT_ARGUMENT_MAX_LEN: usize = 4096;
/// 固定参数最大条数。
pub const SCRIPT_FIXED_ARGUMENT_MAX_COUNT: usize = 32;
/// 透传参数最大条数。
pub const SCRIPT_USER_ARGUMENT_MAX_COUNT: usize = 16;
/// 环境白名单最大条数。
pub const SCRIPT_ENV_ALLOWLIST_MAX_COUNT: usize = 64;
/// 脚本快照名称最大长度（字符）。
pub const SCRIPT_DISPLAY_NAME_MAX_LEN: usize = 120;
/// 脚本超时上限（秒），对应 24 小时。
pub const SCRIPT_TIMEOUT_MAX_SECONDS: u32 = 86_400;

/// 禁止作为脚本程序的 shell 解释器名单：它们会把参数重新拼接成命令行。
const SHELL_INTERPRETERS: [&str; 10] = [
    "sh",
    "bash",
    "zsh",
    "dash",
    "ksh",
    "cmd",
    "powershell",
    "pwsh",
    "fish",
    "csh",
];

/// 程序路径禁止出现的字符：shell 元字符与引号，避免后续任何拼接消费方被注入。
const FORBIDDEN_PROGRAM_CHARS: &[u8] = b"&|;<>$`\"'(){}[]*?!^";

/// 脚本标识：`[a-z0-9]` 开头，后续只允许小写字母、数字、`.`、`-`、`_`。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ScriptId(String);

impl ScriptId {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for ScriptId {
    type Error = &'static str;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.is_empty() || value.len() > SCRIPT_ID_MAX_LEN {
            return Err("脚本标识长度必须在 1..=64 字节之间");
        }
        let mut bytes = value.bytes();
        let first_ok = bytes
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit());
        let rest_ok = bytes.all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-' | b'_')
        });
        if !first_ok || !rest_ok {
            return Err("脚本标识必须小写字母或数字开头，且只含小写字母、数字、`.`、`-`、`_`");
        }
        Ok(Self(value))
    }
}

impl From<ScriptId> for String {
    fn from(value: ScriptId) -> Self {
        value.0
    }
}

/// 受管脚本程序（可执行文件名或工作区内可执行路径）。
///
/// 拒绝空白、控制字符、shell 元字符，以及 `sh`/`pwsh`/`cmd` 一类解释器，
/// 保证参数始终按数组直传而不经任何 shell 重新解析。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ScriptProgram(String);

impl ScriptProgram {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 取程序最后一段并去掉 Windows 可执行扩展名，用于识别 shell 解释器。
fn program_stem(program: &str) -> String {
    let basename = program.rsplit(['/', '\\']).next().unwrap_or(program);
    let lowered = basename.to_ascii_lowercase();
    lowered
        .strip_suffix(".exe")
        .map_or_else(|| lowered.clone(), str::to_owned)
}

impl TryFrom<String> for ScriptProgram {
    type Error = &'static str;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.is_empty() || value.len() > SCRIPT_PROGRAM_MAX_LEN {
            return Err("脚本程序长度必须在 1..=260 字节之间");
        }
        if value.bytes().any(|byte| {
            byte.is_ascii_whitespace() || byte == 0 || FORBIDDEN_PROGRAM_CHARS.contains(&byte)
        }) {
            return Err("脚本程序禁止空白、控制字符与 shell 元字符");
        }
        let stem = program_stem(&value);
        if SHELL_INTERPRETERS.contains(&stem.as_str()) {
            return Err("脚本程序禁止使用 shell 解释器，请直接给出可执行程序");
        }
        Ok(Self(value))
    }
}

impl From<ScriptProgram> for String {
    fn from(value: ScriptProgram) -> Self {
        value.0
    }
}

/// 单个脚本参数：不透明字符串，禁止 NUL、回车与换行。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ScriptArgument(String);

impl ScriptArgument {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for ScriptArgument {
    type Error = &'static str;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.len() > SCRIPT_ARGUMENT_MAX_LEN {
            return Err("单个脚本参数长度不得超过 4096 字节");
        }
        if value.bytes().any(|byte| matches!(byte, 0 | b'\n' | b'\r')) {
            return Err("脚本参数禁止 NUL 与换行，防止日志与控制流注入");
        }
        Ok(Self(value))
    }
}

impl From<ScriptArgument> for String {
    fn from(value: ScriptArgument) -> Self {
        value.0
    }
}

/// 固定参数列表（`paseo.json` 中声明的参数），条数上限 32。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Vec<ScriptArgument>", into = "Vec<ScriptArgument>")]
pub struct ScriptFixedArguments(Vec<ScriptArgument>);

impl ScriptFixedArguments {
    #[must_use]
    pub fn as_slice(&self) -> &[ScriptArgument] {
        &self.0
    }
}

impl TryFrom<Vec<ScriptArgument>> for ScriptFixedArguments {
    type Error = &'static str;

    fn try_from(value: Vec<ScriptArgument>) -> Result<Self, Self::Error> {
        if value.len() > SCRIPT_FIXED_ARGUMENT_MAX_COUNT {
            return Err("固定参数最多 32 条");
        }
        Ok(Self(value))
    }
}

impl From<ScriptFixedArguments> for Vec<ScriptArgument> {
    fn from(value: ScriptFixedArguments) -> Self {
        value.0
    }
}

/// 透传参数列表（客户端随启动请求提交），条数上限 16。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Vec<ScriptArgument>", into = "Vec<ScriptArgument>")]
pub struct ScriptUserArguments(Vec<ScriptArgument>);

impl ScriptUserArguments {
    #[must_use]
    pub fn as_slice(&self) -> &[ScriptArgument] {
        &self.0
    }
}

impl TryFrom<Vec<ScriptArgument>> for ScriptUserArguments {
    type Error = &'static str;

    fn try_from(value: Vec<ScriptArgument>) -> Result<Self, Self::Error> {
        if value.len() > SCRIPT_USER_ARGUMENT_MAX_COUNT {
            return Err("透传参数最多 16 条");
        }
        Ok(Self(value))
    }
}

impl From<ScriptUserArguments> for Vec<ScriptArgument> {
    fn from(value: ScriptUserArguments) -> Self {
        value.0
    }
}

/// 脚本参数策略：声明固定参数与是否接受客户端透传参数。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ScriptArgumentPolicyWire")]
pub struct ScriptArgumentPolicy {
    pub fixed_arguments: ScriptFixedArguments,
    pub accepts_user_arguments: bool,
    /// 接受透传参数时允许的最大条数；不接受时必须为 0。
    pub max_user_arguments: u16,
    /// 单条透传参数最大字节数；必须落在 1..=4096。
    pub max_user_argument_bytes: u16,
}

#[derive(Deserialize)]
struct ScriptArgumentPolicyWire {
    fixed_arguments: ScriptFixedArguments,
    #[serde(default)]
    accepts_user_arguments: bool,
    #[serde(default)]
    max_user_arguments: u16,
    max_user_argument_bytes: u16,
}

impl TryFrom<ScriptArgumentPolicyWire> for ScriptArgumentPolicy {
    type Error = &'static str;

    fn try_from(wire: ScriptArgumentPolicyWire) -> Result<Self, Self::Error> {
        if !(1..=SCRIPT_ARGUMENT_MAX_LEN as u16).contains(&wire.max_user_argument_bytes) {
            return Err("透传参数单条字节上限必须落在 1..=4096");
        }
        let limit_ok = if wire.accepts_user_arguments {
            (1..=SCRIPT_USER_ARGUMENT_MAX_COUNT as u16).contains(&wire.max_user_arguments)
        } else {
            wire.max_user_arguments == 0
        };
        if !limit_ok {
            return Err("透传参数条数上限必须与是否接受透传参数一致");
        }
        Ok(Self {
            fixed_arguments: wire.fixed_arguments,
            accepts_user_arguments: wire.accepts_user_arguments,
            max_user_arguments: wire.max_user_arguments,
            max_user_argument_bytes: wire.max_user_argument_bytes,
        })
    }
}

/// 脚本工作目录策略：工作区根或工作区内子目录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScriptWorkdirPolicy {
    WorkspaceRoot,
    Subdirectory {
        relative_path: WorkspaceRelativePath,
    },
}

/// 脚本环境策略：显式白名单 + 是否继承父进程环境（默认不继承）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ScriptEnvPolicyWire")]
pub struct ScriptEnvPolicy {
    pub allow_keys: Vec<EnvKey>,
    pub inherits_parent_env: bool,
}

#[derive(Deserialize)]
struct ScriptEnvPolicyWire {
    #[serde(default)]
    allow_keys: Vec<EnvKey>,
    #[serde(default)]
    inherits_parent_env: bool,
}

impl TryFrom<ScriptEnvPolicyWire> for ScriptEnvPolicy {
    type Error = &'static str;

    fn try_from(wire: ScriptEnvPolicyWire) -> Result<Self, Self::Error> {
        if wire.allow_keys.len() > SCRIPT_ENV_ALLOWLIST_MAX_COUNT {
            return Err("环境变量白名单最多 64 条");
        }
        let mut seen = HashSet::with_capacity(wire.allow_keys.len());
        if wire.allow_keys.iter().any(|key| !seen.insert(key.as_str())) {
            return Err("环境变量白名单禁止重复项");
        }
        Ok(Self {
            allow_keys: wire.allow_keys,
            inherits_parent_env: wire.inherits_parent_env,
        })
    }
}

/// 脚本风险等级：与审批卡的 `high_risk` 判定直接对齐。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScriptRiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

impl ScriptRiskLevel {
    /// 高风险及以上必须逐次人工审批，禁止会话级自动放行。
    #[must_use]
    pub const fn requires_approval(self) -> bool {
        matches!(self, Self::High | Self::Critical)
    }
}

/// `paseo.json` 中的单条受管脚本声明。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ManagedScriptEntryWire")]
pub struct ManagedScriptEntry {
    pub script_id: ScriptId,
    pub display_name: String,
    pub description: Option<String>,
    pub program: ScriptProgram,
    pub argument_policy: ScriptArgumentPolicy,
    pub workdir: ScriptWorkdirPolicy,
    pub env_policy: ScriptEnvPolicy,
    pub risk: ScriptRiskLevel,
    /// 常驻服务型脚本（对标 `paseo.json` 的 `type: "service"`）。
    pub service: bool,
    /// 一次性脚本的超时；常驻服务型必须为 `None`。
    pub timeout_seconds: Option<u32>,
}

#[derive(Deserialize)]
struct ManagedScriptEntryWire {
    script_id: ScriptId,
    display_name: String,
    #[serde(default)]
    description: Option<String>,
    program: ScriptProgram,
    argument_policy: ScriptArgumentPolicy,
    workdir: ScriptWorkdirPolicy,
    #[serde(default)]
    env_policy: ScriptEnvPolicy,
    risk: ScriptRiskLevel,
    #[serde(default)]
    service: bool,
    #[serde(default)]
    timeout_seconds: Option<u32>,
}

impl TryFrom<ManagedScriptEntryWire> for ManagedScriptEntry {
    type Error = &'static str;

    fn try_from(wire: ManagedScriptEntryWire) -> Result<Self, Self::Error> {
        let name_len = wire.display_name.chars().count();
        if name_len == 0 || name_len > SCRIPT_DISPLAY_NAME_MAX_LEN {
            return Err("脚本展示名长度必须在 1..=120 个字符之间");
        }
        if wire.display_name.chars().any(char::is_control) {
            return Err("脚本展示名禁止控制字符");
        }
        if let Some(timeout) = wire.timeout_seconds
            && !(1..=SCRIPT_TIMEOUT_MAX_SECONDS).contains(&timeout)
        {
            return Err("脚本超时必须落在 1..=86400 秒");
        }
        if wire.service && wire.timeout_seconds.is_some() {
            return Err("常驻服务型脚本不得声明一次性超时");
        }
        Ok(Self {
            script_id: wire.script_id,
            display_name: wire.display_name,
            description: wire.description,
            program: wire.program,
            argument_policy: wire.argument_policy,
            workdir: wire.workdir,
            env_policy: wire.env_policy,
            risk: wire.risk,
            service: wire.service,
            timeout_seconds: wire.timeout_seconds,
        })
    }
}

/// 脚本清单查询请求。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListScriptsRequest {
    pub workspace_id: String,
}

/// 脚本清单查询结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptManifestListing {
    pub workspace_id: String,
    /// 展示用相对文件名，固定为 `paseo.json`，不含本机绝对路径。
    pub manifest_file: String,
    pub scripts: Vec<ManagedScriptEntry>,
}

/// 脚本启动请求。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunScriptRequest {
    pub workspace_id: String,
    pub script_id: ScriptId,
    #[serde(default)]
    pub user_arguments: ScriptUserArguments,
    pub idempotency_key: Option<IdempotencyKey>,
    /// 高风险脚本必须携带审批卡 ID，服务端据此复核一次性授权。
    pub approval_id: Option<String>,
}

/// 脚本停止请求。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StopScriptRequest {
    pub run_id: String,
    /// `true` 表示立即强制终止进程树，`false` 表示先发送温和停止信号。
    pub force: bool,
}

/// 脚本运行状态查询请求。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InspectScriptRequest {
    pub run_id: String,
}

/// 脚本运行态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScriptRunState {
    Queued,
    Running,
    Stopping,
    Exited,
    Failed,
    Cancelled,
}

/// 脚本运行快照（清单 + 状态 + 退出信息）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptRunSnapshot {
    pub run_id: String,
    pub workspace_id: String,
    pub script_id: ScriptId,
    pub state: ScriptRunState,
    pub exit_code: Option<i32>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub idempotency_key: Option<IdempotencyKey>,
}

/// 脚本生命周期相位（事件流专用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScriptLifecyclePhase {
    Queued,
    Started,
    Exited,
    Failed,
    Stopped,
}

/// 脚本生命周期事件：每次相位变化携带完整运行快照，客户端无需回查。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptLifecycleEvent {
    pub phase: ScriptLifecyclePhase,
    pub run: ScriptRunSnapshot,
    pub occurred_at: DateTime<Utc>,
}

/// 脚本域命令（由全局 `RpcCommand` 在实现 PR 中 1:1 转发）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
pub enum ScriptCommand {
    List(ListScriptsRequest),
    Run(RunScriptRequest),
    Stop(StopScriptRequest),
    Inspect(InspectScriptRequest),
}

/// 脚本域命令结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum ScriptCommandResult {
    Listed(ScriptManifestListing),
    Accepted(ScriptRunSnapshot),
    Snapshot(ScriptRunSnapshot),
}
