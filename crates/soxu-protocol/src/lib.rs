//! Soxu 跨端（GPUI 桌面端、移动端 Web/PWA 与 CLI）统一版本化通信协议。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 当前通信协议版本号，客户端连接 Daemon 时用于握手兼容性检查。
pub const PROTOCOL_VERSION: u32 = 1;

/// 当目标 Agent 正处于活跃推理轮次时，新消息的投递策略（对标 Paseo）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ActiveTurnBehavior {
    /// 默认排队：等待当前轮次自然结束后再消费。
    #[default]
    Queue,
    /// 中途纠偏：在下一步工具边界注入指令并继续工作。
    Steer,
    /// 立即中断：停止当前轮次并开启新轮次。
    Interrupt,
}

/// 带单调递增游标的事件信封，保障移动端切后台或弱网重连后可按 `seq` 增量追平。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventEnvelope<T> {
    pub protocol_version: u32,
    pub seq: u64,
    pub timestamp: DateTime<Utc>,
    pub payload: T,
}

impl<T> EventEnvelope<T> {
    #[must_use]
    pub fn new(seq: u64, payload: T) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            seq,
            timestamp: Utc::now(),
            payload,
        }
    }
}

/// 守护进程健康状态快照。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthSnapshot {
    pub status: String,
    pub daemon_version: String,
    pub protocol_version: u32,
    pub magpie_detected: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_event_envelope() {
        let env = EventEnvelope::new(42, ActiveTurnBehavior::Steer);
        let raw = serde_json::to_string(&env).expect("serialize envelope");
        let decoded: EventEnvelope<ActiveTurnBehavior> =
            serde_json::from_str(&raw).expect("deserialize envelope");
        assert_eq!(decoded.seq, 42);
        assert_eq!(decoded.protocol_version, PROTOCOL_VERSION);
        assert_eq!(decoded.payload, ActiveTurnBehavior::Steer);
    }
}
