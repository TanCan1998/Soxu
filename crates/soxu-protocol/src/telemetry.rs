//! 上下文占用三色分段与 11 项会话遥测统计契约。

use serde::{Deserialize, Serialize};

/// 输入舱上下文占用三段语义色档位（绿 < 65%，黄 65%–88%，红 >= 88%）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextUsageBand {
    SafeGreen,
    WarningAmber,
    DangerRed,
}

impl ContextUsageBand {
    /// 根据上下文占用百分比判定三段语义色档位。
    #[must_use]
    pub fn from_percentage(pct: f64) -> Self {
        if pct >= 88.0 {
            Self::DangerRed
        } else if pct >= 65.0 {
            Self::WarningAmber
        } else {
            Self::SafeGreen
        }
    }
}

/// 输入舱右侧统计胶囊与详单浮层完整遥测快照（涵盖 11+ 项核心指标）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SessionTelemetryStats {
    pub turns: u32,
    pub steps: u32,
    pub used_context_tokens: u64,
    pub context_window_limit: u64,
    pub total_tokens: u64,
    pub api_cost_usd: f64,
    pub avg_speed_tps: f64,
    pub peak_speed_tps: f64,
    pub recent_speed_tps: f64,
    pub cache_hit_pct: f64,
    pub cache_note: String,
    pub model_duration_sec: f64,
    pub tool_duration_sec: f64,
    pub ttft_sec: f64,
}

impl SessionTelemetryStats {
    /// 计算当前上下文占用百分比（`0.0..=100.0`）。
    #[must_use]
    pub fn context_usage_pct(&self) -> f64 {
        if self.context_window_limit == 0 {
            return 0.0;
        }
        ((self.used_context_tokens as f64 / self.context_window_limit as f64) * 100.0)
            .clamp(0.0, 100.0)
    }

    /// 返回当前上下文占用对应的三段语义色档位。
    #[must_use]
    pub fn context_band(&self) -> ContextUsageBand {
        ContextUsageBand::from_percentage(self.context_usage_pct())
    }
}
