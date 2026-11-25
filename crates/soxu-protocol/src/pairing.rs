//! 移动端与远程设备配对、一次性 `#offer=` 令牌与设备吊销契约。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 远程连接通道模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PairingTransportMode {
    #[default]
    LanDirect,
    EncryptedRelay,
}

/// 桌面端生成的一次性扫码配对载荷（对应 `#offer=` URL 参数）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairingOfferPayload {
    pub offer_id: String,
    pub daemon_fingerprint: String,
    pub endpoint_url: String,
    pub transport: PairingTransportMode,
    pub one_time_token: String,
    pub expires_at: DateTime<Utc>,
}

/// 6 位动态配对码快照。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairingCodeSnapshot {
    pub code_id: String,
    pub six_digit_code: String,
    pub offer_url: String,
    pub expires_at: DateTime<Utc>,
    pub ttl_seconds: u64,
}

/// 移动端发起配对认领时携带的凭据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum PairingCredentialInput {
    OfferToken(String),
    SixDigitCode(String),
}

/// 移动端认领配对请求。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimPairingRequest {
    pub device_name: String,
    pub platform: String,
    pub credential: PairingCredentialInput,
}

/// 认领成功后下发给移动端的持久访问令牌与 Daemon 指纹。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimPairingResponse {
    pub device_id: String,
    pub bearer_token: String,
    pub daemon_fingerprint: String,
    pub protocol_version: u32,
    pub paired_at: DateTime<Utc>,
}

/// 已配对远程设备摘要（支持在线状态监控与单台吊销）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairedDeviceSummary {
    pub device_id: String,
    pub device_name: String,
    pub platform: String,
    pub last_seen_at: DateTime<Utc>,
    pub online: bool,
    pub revoked: bool,
}
