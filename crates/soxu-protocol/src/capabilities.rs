//! Agent 能力矩阵、思考强度与切换降级回退契约。

use crate::domain::PermissionLevel;
use serde::{Deserialize, Serialize};

/// Agent 推理思考深度分级（对应界面「低 / 中 / 高 / 极高 / 最高」）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ThinkingEffort {
    Low,
    #[default]
    Medium,
    High,
    Xhigh,
    Max,
}

/// 执行权限档位语义色调（用于驱动桌面与移动控件图标着色）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PermissionTone {
    Neutral,
    #[default]
    Accent,
    Warning,
    Danger,
}

/// 权限档位中文元数据描述。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionOptionMeta {
    pub id: PermissionLevel,
    pub label: String,
    pub description: String,
    pub tone: PermissionTone,
}

/// 模型能力与路由元信息。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelDescriptor {
    pub model_id: String,
    pub display_name: String,
    pub tier: String,
    pub note: String,
    pub context_window_limit: u64,
    pub thinking_options: Vec<ThinkingEffort>,
    /// 当模型仅在特定 CLI 原生直连可用时标注目标代理标识（如 `agy`）。
    pub native_direct_only_agent: Option<String>,
}

/// 原生 Agent CLI 驱动能力快照。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentCapabilityDescriptor {
    pub agent_id: String,
    pub display_name: String,
    pub binary_name: String,
    pub installed: bool,
    pub version: Option<String>,
    pub supported_models: Vec<String>,
    pub permissions: Vec<PermissionOptionMeta>,
    pub supports_thinking: bool,
    pub supports_live_steer: bool,
}

/// 切换代理或模型时失效参数自动回退提示（`lost-controls` Recovery）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LostControlNotice {
    pub field: String,
    pub previous_value: String,
    pub fallback_value: String,
    pub reason_zh: String,
}
