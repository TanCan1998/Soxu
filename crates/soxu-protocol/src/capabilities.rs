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

/// 守护进程能力位目录（M6 新增三组契约的协商入口）。
///
/// 能力位一律**缺省关闭**：未在握手中显式声明的能力视为不可用，
/// 服务端不得向未协商的客户端下发对应 RPC 结果与事件。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DaemonFeature {
    /// 受管脚本（`paseo.json` 清单与生命周期）。
    ManagedScripts,
    /// Schedules（cron / 固定周期调度）。
    Schedules,
    /// Heartbeats（空闲期主动心跳续跑）。
    Heartbeats,
    /// 受控 PTY（有界分片输出）。
    ControlledPty,
}

impl DaemonFeature {
    /// 全部已知能力位，用于生成声明快照与回归测试。
    pub const ALL: [Self; 4] = [
        Self::ManagedScripts,
        Self::Schedules,
        Self::Heartbeats,
        Self::ControlledPty,
    ];

    /// 稳定线缆令牌，跨版本不得改名。
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::ManagedScripts => "managed_scripts",
            Self::Schedules => "schedules",
            Self::Heartbeats => "heartbeats",
            Self::ControlledPty => "controlled_pty",
        }
    }

    /// 由线缆令牌解析能力位；未知令牌返回 `None` 而不报错，
    /// 从而保证旧客户端遇到新能力时只是忽略。
    #[must_use]
    pub fn from_token(token: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|feature| feature.token() == token)
    }
}

/// 能力声明集合：保留未知令牌原文，实现旧客户端向前兼容。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DaemonFeatureSet {
    /// 已声明的能力令牌；未知令牌原样保留，不参与能力判定，也不会被丢弃。
    #[serde(default)]
    pub declared: Vec<String>,
}

impl DaemonFeatureSet {
    /// 全关状态：三组 M6 契约的默认协商结果。
    #[must_use]
    pub const fn disabled() -> Self {
        Self {
            declared: Vec::new(),
        }
    }

    /// 按已知能力位构造声明集合。
    #[must_use]
    pub fn from_features(features: impl IntoIterator<Item = DaemonFeature>) -> Self {
        Self {
            declared: features.into_iter().map(|f| f.token().to_owned()).collect(),
        }
    }

    /// 判能力位是否已声明；未声明即视为关闭。
    #[must_use]
    pub fn is_enabled(&self, feature: DaemonFeature) -> bool {
        self.declared.iter().any(|token| token == feature.token())
    }

    /// 已识别能力位列表。
    #[must_use]
    pub fn enabled_features(&self) -> Vec<DaemonFeature> {
        Self::declared_features(&self.declared)
    }

    /// 未被当前协议识别的令牌（前向兼容诊断用）。
    #[must_use]
    pub fn unknown_tokens(&self) -> Vec<&str> {
        self.declared
            .iter()
            .map(String::as_str)
            .filter(|token| DaemonFeature::from_token(token).is_none())
            .collect()
    }

    fn declared_features(declared: &[String]) -> Vec<DaemonFeature> {
        DaemonFeature::ALL
            .into_iter()
            .filter(|feature| declared.iter().any(|token| token == feature.token()))
            .collect()
    }
}
