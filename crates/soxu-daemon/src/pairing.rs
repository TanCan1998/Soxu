//! 移动遥控端一次性扫码 `#offer=` 令牌、6 位动态配对码及设备 Bearer 鉴权管理。

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Duration, Utc};
use rand::RngExt as _;
use sha2::{Digest, Sha256};
use soxu_protocol::{
    ActionableError, ClaimPairingRequest, ClaimPairingResponse, ErrorCode, PROTOCOL_VERSION,
    PairedDeviceSummary, PairingCodeSnapshot, PairingCredentialInput, PairingOfferPayload,
    PairingTransportMode,
};
use uuid::Uuid;

const DEFAULT_OFFER_TTL_SECS: i64 = 300;

#[derive(Debug, Clone)]
struct PendingOfferRecord {
    offer: PairingOfferPayload,
    code: PairingCodeSnapshot,
    used: bool,
}

/// 一次性配对会话与凭证管理器。
#[derive(Debug)]
pub struct PairingManager {
    daemon_fingerprint: String,
    default_endpoint_url: String,
    active_offers: Vec<PendingOfferRecord>,
}

impl PairingManager {
    #[must_use]
    pub fn new(daemon_fingerprint: String, default_endpoint_url: String) -> Self {
        Self {
            daemon_fingerprint,
            default_endpoint_url,
            active_offers: Vec::new(),
        }
    }

    /// 对明文令牌计算不可逆 SHA-256 十六进制摘要（仅将哈希落盘，不存明文）。
    #[must_use]
    pub fn hash_token(raw_token: &str) -> String {
        let digest = Sha256::digest(raw_token.as_bytes());
        let mut hex = String::with_capacity(digest.len() * 2);
        for byte in digest {
            hex.push_str(&format!("{byte:02x}"));
        }
        hex
    }

    /// 检查已配对设备是否已超过空闲自动锁定阈值。
    #[must_use]
    pub fn is_idle_locked(
        last_seen_at: DateTime<Utc>,
        idle_lock_min: u16,
        now: DateTime<Utc>,
    ) -> bool {
        if idle_lock_min == 0 {
            return false;
        }
        let max_idle = Duration::minutes(i64::from(idle_lock_min));
        now.signed_duration_since(last_seen_at) > max_idle
    }

    /// 生成新的一次性扫码 `#offer=` 载荷与 6 位数字配对码（默认 5 分钟过期）。
    pub fn create_offer(
        &mut self,
        transport: PairingTransportMode,
    ) -> (PairingOfferPayload, PairingCodeSnapshot) {
        let now = Utc::now();
        self.active_offers
            .retain(|rec| !rec.used && rec.offer.expires_at > now);

        let offer_id = format!("off_{}", Uuid::new_v4().simple());
        let code_id = format!("code_{}", Uuid::new_v4().simple());
        let one_time_token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        let pin_num: u32 = rand::rng().random_range(0..1_000_000);
        let six_digit_code = format!("{pin_num:06}");
        let expires_at = now + Duration::seconds(DEFAULT_OFFER_TTL_SECS);

        let offer = PairingOfferPayload {
            offer_id,
            daemon_fingerprint: self.daemon_fingerprint.clone(),
            endpoint_url: self.default_endpoint_url.clone(),
            transport,
            one_time_token,
            expires_at,
        };

        let offer_json = serde_json::to_vec(&offer).unwrap_or_default();
        let encoded_offer = URL_SAFE_NO_PAD.encode(offer_json);
        let offer_url = format!("{}/#offer={encoded_offer}", self.default_endpoint_url);

        let code = PairingCodeSnapshot {
            code_id,
            six_digit_code,
            offer_url,
            expires_at,
            ttl_seconds: u64::try_from(DEFAULT_OFFER_TTL_SECS).unwrap_or(300),
        };

        self.active_offers.push(PendingOfferRecord {
            offer: offer.clone(),
            code: code.clone(),
            used: false,
        });

        (offer, code)
    }

    /// 核验一次性扫码令牌或 6 位配对码；校验通过后立即消费作废防重放。
    pub fn claim(
        &mut self,
        req: &ClaimPairingRequest,
    ) -> Result<(ClaimPairingResponse, PairedDeviceSummary, String), ActionableError> {
        let now = Utc::now();
        let matched_idx = self
            .active_offers
            .iter()
            .position(|rec| match &req.credential {
                PairingCredentialInput::OfferToken(tok) => rec.offer.one_time_token == *tok,
                PairingCredentialInput::SixDigitCode(pin) => rec.code.six_digit_code == *pin,
            });

        let Some(idx) = matched_idx else {
            return Err(ActionableError::new(
                ErrorCode::Unauthorized,
                "配对码或扫码令牌无效",
                "移动设备未能接入当前守护进程",
                "请在桌面端重新生成配对二维码或 6 位配对码后重试",
            ));
        };

        let rec = &mut self.active_offers[idx];
        if rec.used {
            return Err(ActionableError::new(
                ErrorCode::PairingExpired,
                "该配对令牌已被使用",
                "为防止令牌重放泄露，一次性配对码不可重复认领",
                "请在桌面端点击刷新配对码后再扫一次",
            ));
        }
        if rec.offer.expires_at <= now {
            return Err(ActionableError::new(
                ErrorCode::PairingExpired,
                "配对码已超过 5 分钟有效期",
                "移动设备认证失败",
                "请在桌面端刷新配对码后重新发起连接",
            ));
        }

        rec.used = true;
        let device_id = format!("dev_{}", Uuid::new_v4().simple());
        let bearer_token = format!(
            "soxu_tok_{}{}",
            Uuid::new_v4().simple(),
            Uuid::new_v4().simple()
        );
        let token_sha256 = Self::hash_token(&bearer_token);

        let device = PairedDeviceSummary {
            device_id: device_id.clone(),
            device_name: req.device_name.clone(),
            platform: req.platform.clone(),
            last_seen_at: now,
            online: true,
            revoked: false,
        };

        let resp = ClaimPairingResponse {
            device_id,
            bearer_token,
            daemon_fingerprint: self.daemon_fingerprint.clone(),
            protocol_version: PROTOCOL_VERSION,
            paired_at: now,
        };

        Ok((resp, device, token_sha256))
    }
}
