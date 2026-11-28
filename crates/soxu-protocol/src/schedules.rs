//! Schedules & Heartbeats 契约：创建、更新、停用、删除、列出、立即触发，
//! 以及触发与运行结果事件（M6 前置契约）。
//!
//! 校验红线：cron 表达式必须是标准 5 段式，周期与重试上限有硬边界，
//! 有效期窗口必须单调，幂等键复用 [`IdempotencyKey`] 保证重复下发只执行一次。
//!
//! 本模块只定义契约：调度持久化、定时器与触发执行由后续 daemon 实现 PR 落地。

use crate::domain::PermissionLevel;
use crate::error::ActionableError;
use crate::primitives::IdempotencyKey;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// cron 表达式段数（分 时 日 月 周）。
pub const CRON_FIELD_COUNT: usize = 5;
/// cron 表达式最大长度（字节）。
pub const CRON_EXPRESSION_MAX_LEN: usize = 128;
/// 调度名称最大长度（字符）。
pub const SCHEDULE_NAME_MAX_LEN: usize = 120;
/// 时区标识最大长度（字节）。
pub const SCHEDULE_TIMEZONE_MAX_LEN: usize = 64;
/// 固定周期最小间隔（秒）：低于一分钟视为误配置的忙轮询。
pub const SCHEDULE_INTERVAL_MIN_SECONDS: u32 = 60;
/// 固定周期最大间隔（秒），对应 30 天。
pub const SCHEDULE_INTERVAL_MAX_SECONDS: u32 = 30 * 24 * 3_600;
/// 心跳最小间隔（秒）。
pub const HEARTBEAT_INTERVAL_MIN_SECONDS: u32 = 30;
/// 心跳最大间隔（秒），对应 30 天。
pub const HEARTBEAT_INTERVAL_MAX_SECONDS: u32 = 30 * 24 * 3_600;
/// 失败重试次数上限（含首次执行）。
pub const SCHEDULE_RETRY_MAX_ATTEMPTS: u8 = 10;
/// 重试退避上限（秒）。
pub const SCHEDULE_RETRY_BACKOFF_MAX_SECONDS: u32 = 3_600;
/// 触发正文最大长度（字节）。
pub const SCHEDULE_PROMPT_TEXT_MAX_LEN: usize = 16_384;

/// 调度标识：服务端签发的稳定 ID。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ScheduleId(String);

impl ScheduleId {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for ScheduleId {
    type Error = &'static str;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.is_empty() || value.len() > 64 {
            return Err("调度标识长度必须在 1..=64 字节之间");
        }
        if !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        {
            return Err("调度标识只允许 ASCII 字母、数字、`-`、`_` 与 `.`");
        }
        Ok(Self(value))
    }
}

impl From<ScheduleId> for String {
    fn from(value: ScheduleId) -> Self {
        value.0
    }
}

/// 调度名称：非空且不含控制字符。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ScheduleName(String);

impl ScheduleName {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for ScheduleName {
    type Error = &'static str;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let len = value.chars().count();
        if len == 0 || len > SCHEDULE_NAME_MAX_LEN {
            return Err("调度名称长度必须在 1..=120 个字符之间");
        }
        if value.trim() != value {
            return Err("调度名称首尾不得包含空白");
        }
        if value.chars().any(char::is_control) {
            return Err("调度名称禁止控制字符");
        }
        Ok(Self(value))
    }
}

impl From<ScheduleName> for String {
    fn from(value: ScheduleName) -> Self {
        value.0
    }
}

/// 标准 5 段式 cron 表达式（分 时 日 月 周），只允许数字与 `* , - /`。
///
/// 语义求值（步长范围、周字段 0/7 等价）由服务端调度器负责；
/// 协议层只拒绝结构性非法表达式，避免两套 cron 语义分叉。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct CronExpression(String);

impl CronExpression {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for CronExpression {
    type Error = &'static str;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.is_empty() || value.len() > CRON_EXPRESSION_MAX_LEN {
            return Err("cron 表达式长度必须在 1..=128 字节之间");
        }
        let fields: Vec<&str> = value.split_whitespace().collect();
        if fields.len() != CRON_FIELD_COUNT {
            return Err("cron 表达式必须是标准 5 段式（分 时 日 月 周）");
        }
        if fields.iter().any(|field| {
            field
                .bytes()
                .any(|byte| !(byte.is_ascii_digit() || matches!(byte, b'*' | b',' | b'-' | b'/')))
        }) {
            return Err("cron 表达式只允许数字、`*`、`,`、`-` 与 `/`");
        }
        Ok(Self(fields.join(" ")))
    }
}

impl From<CronExpression> for String {
    fn from(value: CronExpression) -> Self {
        value.0
    }
}

/// IANA 时区标识或 `UTC`；拒绝空串、空白与路径分隔符。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ScheduleTimezone(String);

impl ScheduleTimezone {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for ScheduleTimezone {
    type Error = &'static str;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.is_empty() || value.len() > SCHEDULE_TIMEZONE_MAX_LEN {
            return Err("时区标识长度必须在 1..=64 字节之间");
        }
        if value.trim() != value || value.chars().any(char::is_whitespace) {
            return Err("时区标识禁止空白");
        }
        if !value.contains('/') && value != "UTC" {
            return Err("时区必须是 `UTC` 或 `Area/City` 形式");
        }
        if value.bytes().any(|byte| {
            !(byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'_' | b'-' | b'+'))
        }) {
            return Err("时区标识只允许字母、数字、`/`、`_`、`-` 与 `+`");
        }
        Ok(Self(value))
    }
}

impl From<ScheduleTimezone> for String {
    fn from(value: ScheduleTimezone) -> Self {
        value.0
    }
}

/// 调度节奏：cron、固定周期或心跳。
///
/// 反序列化经 [`ScheduleCadenceWire`] 校验周期边界，序列化保持 `tag + inline` 形态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScheduleCadence {
    Cron {
        expression: CronExpression,
        timezone: Option<ScheduleTimezone>,
    },
    Interval {
        every_seconds: u32,
    },
    /// 心跳：Agent 空闲时也按固定间隔主动唤醒并续跑。
    Heartbeat {
        every_seconds: u32,
        only_when_idle: bool,
    },
}

impl ScheduleCadence {
    /// 校验周期边界；反序列化经 [`ScheduleCadenceWire`] 自动调用。
    fn validate(&self) -> Result<(), &'static str> {
        match self {
            Self::Cron { .. } => Ok(()),
            Self::Interval { every_seconds } => {
                if !(SCHEDULE_INTERVAL_MIN_SECONDS..=SCHEDULE_INTERVAL_MAX_SECONDS)
                    .contains(every_seconds)
                {
                    return Err("固定周期必须落在 60..=2592000 秒");
                }
                Ok(())
            }
            Self::Heartbeat { every_seconds, .. } => {
                if !(HEARTBEAT_INTERVAL_MIN_SECONDS..=HEARTBEAT_INTERVAL_MAX_SECONDS)
                    .contains(every_seconds)
                {
                    return Err("心跳间隔必须落在 30..=2592000 秒");
                }
                Ok(())
            }
        }
    }
}

/// 反序列化中间体：承载与 [`ScheduleCadence`] 同构的 `tag + content`（inline）形态。
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ScheduleCadenceWire {
    Cron {
        expression: CronExpression,
        #[serde(default)]
        timezone: Option<ScheduleTimezone>,
    },
    Interval {
        every_seconds: u32,
    },
    Heartbeat {
        every_seconds: u32,
        #[serde(default)]
        only_when_idle: bool,
    },
}

impl<'de> Deserialize<'de> for ScheduleCadence {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = ScheduleCadenceWire::deserialize(deserializer)?;
        let cadence = match wire {
            ScheduleCadenceWire::Cron {
                expression,
                timezone,
            } => Self::Cron {
                expression,
                timezone,
            },
            ScheduleCadenceWire::Interval { every_seconds } => Self::Interval { every_seconds },
            ScheduleCadenceWire::Heartbeat {
                every_seconds,
                only_when_idle,
            } => Self::Heartbeat {
                every_seconds,
                only_when_idle,
            },
        };
        cadence.validate().map_err(serde::de::Error::custom)?;
        Ok(cadence)
    }
}

/// 失败重试策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ScheduleRetryPolicyWire")]
pub struct ScheduleRetryPolicy {
    /// 含首次执行的最大尝试次数。
    pub max_attempts: u8,
    pub backoff_seconds: u32,
}

#[derive(Deserialize)]
struct ScheduleRetryPolicyWire {
    max_attempts: u8,
    #[serde(default)]
    backoff_seconds: u32,
}

impl TryFrom<ScheduleRetryPolicyWire> for ScheduleRetryPolicy {
    type Error = &'static str;

    fn try_from(wire: ScheduleRetryPolicyWire) -> Result<Self, Self::Error> {
        if !(1..=SCHEDULE_RETRY_MAX_ATTEMPTS).contains(&wire.max_attempts) {
            return Err("重试最大尝试次数必须落在 1..=10");
        }
        if wire.backoff_seconds > SCHEDULE_RETRY_BACKOFF_MAX_SECONDS {
            return Err("重试退避秒数不得超过 3600");
        }
        Ok(Self {
            max_attempts: wire.max_attempts,
            backoff_seconds: wire.backoff_seconds,
        })
    }
}

impl Default for ScheduleRetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 1,
            backoff_seconds: 0,
        }
    }
}

/// 有效期窗口：可省略起点与终点，两者同时存在时必须单调递增。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ScheduleValidityWindowWire")]
pub struct ScheduleValidityWindow {
    pub starts_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
struct ScheduleValidityWindowWire {
    #[serde(default)]
    starts_at: Option<DateTime<Utc>>,
    #[serde(default)]
    expires_at: Option<DateTime<Utc>>,
}

impl TryFrom<ScheduleValidityWindowWire> for ScheduleValidityWindow {
    type Error = &'static str;

    fn try_from(wire: ScheduleValidityWindowWire) -> Result<Self, Self::Error> {
        if let (Some(starts_at), Some(expires_at)) = (wire.starts_at, wire.expires_at)
            && expires_at <= starts_at
        {
            return Err("调度有效期终点必须晚于起点");
        }
        Ok(Self {
            starts_at: wire.starts_at,
            expires_at: wire.expires_at,
        })
    }
}

impl ScheduleValidityWindow {
    /// 判断给定时刻是否落在有效期内。
    #[must_use]
    pub fn is_active_at(&self, moment: DateTime<Utc>) -> bool {
        self.starts_at.is_none_or(|start| moment >= start)
            && self.expires_at.is_none_or(|end| moment < end)
    }
}

/// 调度触发写入会话的正文与能力参数。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ScheduleTriggerWire")]
pub struct ScheduleTrigger {
    pub agent_id: String,
    pub model_id: Option<String>,
    pub permission: Option<PermissionLevel>,
    pub text: String,
}

#[derive(Deserialize)]
struct ScheduleTriggerWire {
    agent_id: String,
    #[serde(default)]
    model_id: Option<String>,
    #[serde(default)]
    permission: Option<PermissionLevel>,
    text: String,
}

impl TryFrom<ScheduleTriggerWire> for ScheduleTrigger {
    type Error = &'static str;

    fn try_from(wire: ScheduleTriggerWire) -> Result<Self, Self::Error> {
        if wire.agent_id.trim().is_empty() {
            return Err("调度触发必须指定非空 agent_id");
        }
        if wire
            .model_id
            .as_deref()
            .is_some_and(|model| model.trim().is_empty())
        {
            return Err("调度触发的 model_id 不能是空白字符串");
        }
        if wire.text.is_empty() || wire.text.len() > SCHEDULE_PROMPT_TEXT_MAX_LEN {
            return Err("调度触发正文长度必须在 1..=16384 字节之间");
        }
        Ok(Self {
            agent_id: wire.agent_id,
            model_id: wire.model_id,
            permission: wire.permission,
            text: wire.text,
        })
    }
}

/// 调度状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleState {
    Active,
    Paused,
    Expired,
}

/// 创建调度请求。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateScheduleRequest {
    pub workspace_id: String,
    pub name: ScheduleName,
    pub cadence: ScheduleCadence,
    pub trigger: ScheduleTrigger,
    #[serde(default)]
    pub validity: ScheduleValidityWindow,
    #[serde(default)]
    pub retry: ScheduleRetryPolicy,
    pub idempotency_key: IdempotencyKey,
    #[serde(default)]
    pub paused: bool,
}

/// 更新调度请求：省略字段保持原值，显式 `null` 仅用于清空可空字段。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateScheduleRequest {
    pub schedule_id: ScheduleId,
    #[serde(default)]
    pub name: Option<ScheduleName>,
    #[serde(default)]
    pub cadence: Option<ScheduleCadence>,
    #[serde(default)]
    pub trigger: Option<ScheduleTrigger>,
    #[serde(default)]
    pub validity: Option<ScheduleValidityWindow>,
    #[serde(default)]
    pub retry: Option<ScheduleRetryPolicy>,
}

/// 停用/恢复调度请求（`paused = true` 为停用）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PauseScheduleRequest {
    pub schedule_id: ScheduleId,
    pub paused: bool,
}

/// 删除调度请求。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeleteScheduleRequest {
    pub schedule_id: ScheduleId,
}

/// 列出某工作区全部调度。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListSchedulesRequest {
    pub workspace_id: String,
}

/// 立即触发一次调度（不改变周期，仍走同一幂等与重试语义）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggerScheduleRequest {
    pub schedule_id: ScheduleId,
    pub idempotency_key: IdempotencyKey,
}

/// 调度摘要。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduleSummary {
    pub schedule_id: ScheduleId,
    pub workspace_id: String,
    pub name: ScheduleName,
    pub cadence: ScheduleCadence,
    pub trigger: ScheduleTrigger,
    pub validity: ScheduleValidityWindow,
    pub retry: ScheduleRetryPolicy,
    pub state: ScheduleState,
    pub next_run_at: Option<DateTime<Utc>>,
    pub last_run_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 调度列表结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduleListResult {
    pub workspace_id: String,
    pub schedules: Vec<ScheduleSummary>,
}

/// 触发来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleRunTrigger {
    Manual,
    Cron,
    Interval,
    Heartbeat,
}

/// 单次运行结果分类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleRunOutcome {
    Succeeded,
    Failed,
    Skipped,
    TimedOut,
}

/// 调度触发事件：调度器决定开跑时广播。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduleTriggeredEvent {
    pub schedule_id: ScheduleId,
    pub run_id: String,
    pub trigger: ScheduleRunTrigger,
    /// 本次应由哪个计划时刻负责（重试沿用同一 `scheduled_for`）。
    pub scheduled_for: DateTime<Utc>,
    pub attempt: u8,
}

/// 调度运行结果事件：收敛成功、失败、跳过与超时。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduleRunResult {
    pub schedule_id: ScheduleId,
    pub run_id: String,
    pub outcome: ScheduleRunOutcome,
    pub attempts: u8,
    pub session_id: Option<String>,
    pub error: Option<ActionableError>,
    pub finished_at: DateTime<Utc>,
}

/// 调度域事件（由全局 `DaemonEvent` 在实现 PR 中转发）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum ScheduleEvent {
    Updated(ScheduleSummary),
    Removed { schedule_id: ScheduleId },
    Triggered(ScheduleTriggeredEvent),
    RunFinished(ScheduleRunResult),
}

/// 调度域命令（由全局 `RpcCommand` 在实现 PR 中 1:1 转发）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
pub enum ScheduleCommand {
    Create(CreateScheduleRequest),
    Update(UpdateScheduleRequest),
    Pause(PauseScheduleRequest),
    Delete(DeleteScheduleRequest),
    List(ListSchedulesRequest),
    TriggerNow(TriggerScheduleRequest),
}

/// 调度域命令结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum ScheduleCommandResult {
    Created(ScheduleSummary),
    Updated(ScheduleSummary),
    Listed(ScheduleListResult),
    TriggerAccepted(ScheduleTriggeredEvent),
    Removed { schedule_id: ScheduleId },
}
