//! 手机端扫码配对弹窗（`pages/06-modals-onboarding-and-automation.md` §3.1）。
//!
//! 规范要点：
//! - 左半区高对比纯白底二维码矩阵 + 上方 `[局域网直连]` / `[加密中继通道]` 切换；
//! - 右半区 6 位一次性数字配对码（等宽 `24px`、`letter-spacing 0.16em`）+ 倒计时刷新环
//!   （`5 分钟`失效防泄露）+ 已配对设备在线列表与单台吊销；
//! - `#offer=` 一次性令牌与 6 位码是**两种互斥凭据**，产出只落到回调载荷
//!   [`PairingRequest`]，**绝不触网**（本阶段无网络 I/O）。
//!
//! 凭据枚举直接复用 `soxu-protocol` 的 [`PairingCredentialInput`]，保证客户端与
//! 移动端认领语义同源；传输模式复用 [`PairingTransportMode`]。

use soxu_protocol::{PairingCredentialInput, PairingTransportMode};

/// 6 位一次性配对码长度。
pub const PAIRING_CODE_LEN: usize = 6;

/// 配对码有效期（`5 分钟`失效防泄露）。
pub const PAIRING_CODE_TTL_SECS: u64 = 300;

/// 桌面端局域网直连默认端口（`pages/06` §3.1 `192.168.x.x:7698`）。
pub const PAIRING_DEFAULT_PORT: u16 = 7698;

/// `#offer=` 一次性令牌在 URL 中的标记。
pub const OFFER_MARKER: &str = "#offer=";

/// 配对输入校验错误。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PairingError {
    /// 输入为空。
    Empty,
    /// `#offer=` 链接缺少有效令牌。
    MalformedOffer,
    /// 既不是有效 `#offer=` 链接，也不是 6 位数字码。
    InvalidCode,
}

impl PairingError {
    /// 面向用户的错误文案。
    pub const fn message(self) -> &'static str {
        match self {
            Self::Empty => "请输入配对码或扫码链接",
            Self::MalformedOffer => "扫码链接缺少有效的一次性令牌",
            Self::InvalidCode => "配对码应为 6 位数字",
        }
    }
}

/// 校验通过的配对输入。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PairingInput {
    /// `#offer=` 一次性令牌。
    OfferToken(String),
    /// 6 位数字配对码。
    SixDigitCode(String),
}

impl PairingInput {
    /// 转为协议层凭据（认领请求载荷）。
    pub fn credential(&self) -> PairingCredentialInput {
        match self {
            Self::OfferToken(token) => PairingCredentialInput::OfferToken(token.clone()),
            Self::SixDigitCode(code) => PairingCredentialInput::SixDigitCode(code.clone()),
        }
    }
}

/// 该输入是否为 6 位一次性配对码（`#offer=` 令牌优先判定）。
pub fn parse_pairing_input(raw: &str) -> Result<PairingInput, PairingError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(PairingError::Empty);
    }
    if let Some(position) = trimmed.find(OFFER_MARKER) {
        let token = &trimmed[position + OFFER_MARKER.len()..];
        return match sanitize_offer_token(token) {
            Some(token) => Ok(PairingInput::OfferToken(token)),
            None => Err(PairingError::MalformedOffer),
        };
    }
    match sanitize_code(trimmed) {
        Some(code) => Ok(PairingInput::SixDigitCode(code)),
        None => Err(PairingError::InvalidCode),
    }
}

/// 一次性令牌字符集收敛（`[A-Za-z0-9_-]`，长度 4..=128）。
fn sanitize_offer_token(raw: &str) -> Option<String> {
    let token = raw.trim();
    let valid = (4..=128).contains(&token.len())
        && token
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'));
    valid.then(|| token.to_string())
}

/// 6 位数字码收敛：允许用户名心中/粘贴常见的空格与连字符分隔。
fn sanitize_code(raw: &str) -> Option<String> {
    if !raw
        .chars()
        .all(|ch| ch.is_ascii_digit() || matches!(ch, ' ' | '-' | '\t'))
    {
        return None;
    }
    let digits: String = raw.chars().filter(char::is_ascii_digit).collect();
    (digits.len() == PAIRING_CODE_LEN).then_some(digits)
}

/// 扫码链接的词法解析结果（只做字符串解析，不发起任何请求）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OfferLink {
    /// 主机（IP 或域名）。
    pub host: String,
    /// 端口（缺省取 [`PAIRING_DEFAULT_PORT`]）。
    pub port: u16,
    /// 一次性令牌。
    pub token: String,
}

/// 解析 `http://<ip>:7698/#offer=<token>` 形式的扫码链接。
///
/// 失败返回 `None`：本阶段不猜测用户意图，也不回落到网络探测。
pub fn parse_offer_link(raw: &str) -> Option<OfferLink> {
    let trimmed = raw.trim();
    let marker = trimmed.find(OFFER_MARKER)?;
    let token = sanitize_offer_token(&trimmed[marker + OFFER_MARKER.len()..])?;
    let before = &trimmed[..marker];
    let authority = before
        .split_once("://")
        .map_or(before, |(_, rest)| rest)
        .trim_end_matches('/');
    if authority.is_empty() {
        return None;
    }
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => (host.to_string(), port.parse::<u16>().ok()?),
        None => (authority.to_string(), PAIRING_DEFAULT_PORT),
    };
    if host.is_empty() {
        return None;
    }
    Some(OfferLink { host, port, token })
}

/// 一次性配对码快照 + 倒计时环。
///
/// 倒计时以「已流逝秒数」为输入，避免在视图模型里引入时钟依赖
/// （真实计时由 M6 的定时器驱动并重新渲染）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PairingCodeView {
    /// 6 位数字码。
    pub six_digit_code: String,
    /// 扫码链接（含 `#offer=`）。
    pub offer_url: String,
    /// 有效期总长（秒）。
    pub ttl_seconds: u64,
}

impl PairingCodeView {
    /// 新建快照；超长码按 6 位截断，空码返回 `None`。
    pub fn new(
        six_digit_code: &str,
        offer_url: impl Into<String>,
        ttl_seconds: u64,
    ) -> Option<Self> {
        let digits: String = six_digit_code
            .chars()
            .filter(char::is_ascii_digit)
            .collect();
        if digits.len() != PAIRING_CODE_LEN {
            return None;
        }
        Some(Self {
            six_digit_code: digits,
            offer_url: offer_url.into(),
            ttl_seconds,
        })
    }

    /// 剩余有效秒数（已流逝超出有效期时为 `0`）。
    pub fn remaining_secs(&self, elapsed_secs: u64) -> u64 {
        self.ttl_seconds.saturating_sub(elapsed_secs)
    }

    /// 是否已失效。
    pub fn is_expired(&self, elapsed_secs: u64) -> bool {
        elapsed_secs >= self.ttl_seconds
    }

    /// 倒计时环进度（`1.0` 满环 → `0.0` 空环）。
    pub fn ring_progress(&self, elapsed_secs: u64) -> f32 {
        if self.ttl_seconds == 0 {
            return 0.0;
        }
        let remaining = self.remaining_secs(elapsed_secs) as f32;
        (remaining / self.ttl_seconds as f32).clamp(0.0, 1.0)
    }

    /// 码位展示文案（等宽 + `letter-spacing 0.16em` 由渲染层负责间距）。
    pub fn spaced_code(&self) -> String {
        self.six_digit_code
            .chars()
            .map(String::from)
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// 已配对设备行（在线管理 + 单台吊销）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PairedDevice {
    /// 设备标识。
    pub device_id: String,
    /// 设备名。
    pub device_name: String,
    /// 平台描述（如 `iOS 19`）。
    pub platform: String,
    /// 是否在线。
    pub online: bool,
}

impl PairedDevice {
    /// 在线状态文案。
    pub const fn status_label(&self) -> &'static str {
        if self.online { "在线" } else { "离线" }
    }
}

/// 认领回调载荷（不触网：由上层决定何时发出）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PairingRequest {
    /// 凭据（一次性令牌或 6 位码）。
    pub credential: PairingInput,
    /// 当前选定传输模式。
    pub transport: PairingTransportMode,
}

impl PairingRequest {
    /// 协议层凭据视图。
    pub fn protocol_credential(&self) -> PairingCredentialInput {
        self.credential.credential()
    }
}

/// 配对弹窗状态。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PairingModal {
    /// 传输模式（局域网直连 / 加密中继）。
    pub mode: PairingTransportMode,
    /// 手动输入框内容（`#offer=` 链接或 6 位码）。
    pub input: String,
    /// 最近一次校验错误。
    pub error: Option<PairingError>,
    /// 当前配对码快照（含倒计时）。
    pub code: Option<PairingCodeView>,
    /// 已配对设备列表。
    pub devices: Vec<PairedDevice>,
    /// 局域网是否存在可达的非回环地址（`false` 时高亮提示切中继）。
    pub lan_available: bool,
}

impl Default for PairingModal {
    fn default() -> Self {
        Self {
            mode: PairingTransportMode::default(),
            input: String::new(),
            error: None,
            code: None,
            devices: Vec::new(),
            lan_available: true,
        }
    }
}

impl PairingModal {
    /// 空态配对弹窗。
    pub fn new() -> Self {
        Self::default()
    }

    /// 切换传输模式并清空旧错误（切换是用户纠错动作的一半）。
    pub fn set_mode(&mut self, mode: PairingTransportMode) {
        self.mode = mode;
        self.error = None;
    }

    /// 覆写输入框内容，并清空旧错误。
    pub fn set_input(&mut self, input: impl Into<String>) {
        self.input = input.into();
        self.error = None;
    }

    /// 模式切换提示：局域网不可达时明确引导用户改走中继。
    pub fn mode_hint(&self) -> Option<&'static str> {
        (!self.lan_available && self.mode == PairingTransportMode::LanDirect)
            .then_some("未找到可达的局域网地址，请切换到加密中继通道")
    }

    /// 模式文案。
    pub const fn mode_label(&self) -> &'static str {
        match self.mode {
            PairingTransportMode::LanDirect => "局域网直连",
            PairingTransportMode::EncryptedRelay => "加密中继通道",
        }
    }

    /// 提交当前输入：成功返回回调载荷，失败记录错误。
    pub fn submit(&mut self) -> Result<PairingRequest, PairingError> {
        match parse_pairing_input(&self.input) {
            Ok(credential) => {
                self.error = None;
                Ok(PairingRequest {
                    credential,
                    transport: self.mode,
                })
            }
            Err(error) => {
                self.error = Some(error);
                Err(error)
            }
        }
    }

    /// 刷新配对码（空码或非 6 位数字视为失效，返回 `false`）。
    pub fn refresh_code(&mut self, six_digit_code: &str, offer_url: &str) -> bool {
        self.code = PairingCodeView::new(six_digit_code, offer_url, PAIRING_CODE_TTL_SECS);
        self.code.is_some()
    }

    /// 吊销某台已配对设备；未知设备返回 `false`。
    pub fn revoke_device(&mut self, device_id: &str) -> bool {
        let before = self.devices.len();
        self.devices.retain(|device| device.device_id != device_id);
        self.devices.len() != before
    }

    /// 在线设备数（状态栏「N 台设备在线」来源）。
    pub fn online_device_count(&self) -> usize {
        self.devices.iter().filter(|device| device.online).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 扫码链接：解析主机、端口与令牌。
    #[test]
    fn parses_offer_link_lexically() {
        let link = parse_offer_link("http://192.168.1.20:7698/#offer=abc123XY")
            .expect("标准扫码链接应解析成功");
        assert_eq!(link.host, "192.168.1.20");
        assert_eq!(link.port, PAIRING_DEFAULT_PORT);
        assert_eq!(link.token, "abc123XY");

        // 缺省端口回落默认端口。
        let no_port = parse_offer_link("http://soxu.lan/#offer=token-1").expect("无端口也应解析");
        assert_eq!(no_port.port, PAIRING_DEFAULT_PORT);
        assert_eq!(no_port.host, "soxu.lan");

        assert!(parse_offer_link("http://192.168.1.20:7698/").is_none());
        assert!(parse_offer_link("http://:7698/#offer=abcd").is_none());
        assert!(parse_offer_link("http://192.168.1.20:abc/#offer=abcd").is_none());
    }

    /// 输入校验矩阵：令牌、6 位码（含分隔符）、非法输入。
    #[test]
    fn validates_pairing_input() {
        assert_eq!(parse_pairing_input("   "), Err(PairingError::Empty));
        assert_eq!(
            parse_pairing_input("#offer="),
            Err(PairingError::MalformedOffer)
        );
        assert_eq!(
            parse_pairing_input("#offer=ab"),
            Err(PairingError::MalformedOffer),
            "令牌过短应判定为缺失而非无效码"
        );

        assert_eq!(
            parse_pairing_input("http://192.168.1.20:7698/#offer=abc123XY"),
            Ok(PairingInput::OfferToken("abc123XY".to_string()))
        );

        assert_eq!(
            parse_pairing_input("123456"),
            Ok(PairingInput::SixDigitCode("123456".to_string()))
        );
        assert_eq!(
            parse_pairing_input(" 123 456 "),
            Ok(PairingInput::SixDigitCode("123456".to_string())),
            "允许粘贴带分隔空格的码"
        );
        assert_eq!(
            parse_pairing_input("123-456"),
            Ok(PairingInput::SixDigitCode("123456".to_string()))
        );

        for invalid in ["12345", "1234567", "abcdef", "12a456", "1234567890"] {
            assert_eq!(
                parse_pairing_input(invalid),
                Err(PairingError::InvalidCode),
                "{invalid} 应判定为非法配对码"
            );
        }
    }

    /// 提交成功/失败的状态迁移与回调载荷（不触网）。
    #[test]
    fn submit_returns_callback_payload_only() {
        let mut modal = PairingModal::new();
        assert_eq!(modal.mode, PairingTransportMode::LanDirect);
        modal.set_input("123");
        let error = modal.submit().expect_err("非法输入应失败");
        assert_eq!(error, PairingError::InvalidCode);
        assert_eq!(modal.error, Some(PairingError::InvalidCode));
        assert_eq!(error.message(), "配对码应为 6 位数字");

        // 继续编辑即清空旧错误。
        modal.set_input("123456");
        assert!(modal.error.is_none());
        let request = modal.submit().expect("合法码应产出回调");
        assert_eq!(
            request.protocol_credential(),
            PairingCredentialInput::SixDigitCode("123456".to_string())
        );

        modal.set_mode(PairingTransportMode::EncryptedRelay);
        assert_eq!(modal.mode_label(), "加密中继通道");
        let relayed = modal.submit().expect("切换模式后仍可提交");
        assert_eq!(relayed.transport, PairingTransportMode::EncryptedRelay);
    }

    /// 倒计时环：剩余秒数递减、过期为 0、进度落在 0..=1。
    #[test]
    fn countdown_ring_and_expiry() {
        let code = PairingCodeView::new(
            "654321",
            "http://192.168.1.20:7698/#offer=x1",
            PAIRING_CODE_TTL_SECS,
        )
        .expect("6 位码应构建成功");
        assert_eq!(code.ttl_seconds, 300);
        assert_eq!(code.remaining_secs(0), 300);
        assert_eq!(code.remaining_secs(299), 1);
        assert_eq!(code.remaining_secs(300), 0);
        assert!(code.is_expired(300));
        assert!(!code.is_expired(299));
        assert_eq!(code.ring_progress(0), 1.0);
        assert_eq!(code.ring_progress(300), 0.0);
        let mid = code.ring_progress(150);
        assert!((mid - 0.5).abs() < 1e-6);
        assert_eq!(code.spaced_code(), "6 5 4 3 2 1");

        // 非 6 位数字码无法构建快照。
        assert!(PairingCodeView::new("12345", "u", 300).is_none());
        assert!(PairingCodeView::new("abcdef", "u", 300).is_none());
    }

    /// 局域网不可达时提示切中继；切换到中继后提示消失。
    #[test]
    fn hints_relay_when_lan_unavailable() {
        let mut modal = PairingModal::new();
        modal.lan_available = false;
        assert_eq!(
            modal.mode_hint(),
            Some("未找到可达的局域网地址，请切换到加密中继通道")
        );
        modal.set_mode(PairingTransportMode::EncryptedRelay);
        assert!(modal.mode_hint().is_none());
    }

    /// 设备列表：在线计数与单台吊销。
    #[test]
    fn device_list_revoke() {
        let mut modal = PairingModal::new();
        modal.devices = vec![
            PairedDevice {
                device_id: "dev-1".to_string(),
                device_name: "Pixel".to_string(),
                platform: "Android 16".to_string(),
                online: true,
            },
            PairedDevice {
                device_id: "dev-2".to_string(),
                device_name: "iPad".to_string(),
                platform: "iPadOS 20".to_string(),
                online: false,
            },
        ];
        assert_eq!(modal.online_device_count(), 1);
        assert_eq!(modal.devices[0].status_label(), "在线");
        assert_eq!(modal.devices[1].status_label(), "离线");

        assert!(modal.revoke_device("dev-1"));
        assert_eq!(modal.devices.len(), 1);
        assert!(!modal.revoke_device("dev-1"), "重复吊销应返回 false");
    }

    /// 刷新配对码：有效码写入快照，非法码清空。
    #[test]
    fn refresh_code_writes_snapshot() {
        let mut modal = PairingModal::new();
        assert!(modal.refresh_code("112233", "http://192.168.1.20:7698/#offer=z9"));
        assert_eq!(
            modal.code.as_ref().map(|code| code.six_digit_code.as_str()),
            Some("112233")
        );
        assert!(!modal.refresh_code("1122", "http://192.168.1.20:7698/#offer=z9"));
        assert!(modal.code.is_none());
    }
}
