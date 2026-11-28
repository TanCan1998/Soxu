//! `DaemonFeature` 能力门控（M6-A）。
//!
//! 能力位一律**缺省关闭**：未显式声明即视为不可用，请求返回既有三要素错误模型，
//! 事件一条都不下发。门控结论只由 [`FeatureGate`] 唯一给出，脚本域与 PTY 域共用，
//! 避免两域各写一套判定而漂移。

use soxu_protocol::{ActionableError, DaemonFeature, DaemonFeatureSet, ErrorCode};

/// 守护进程能力协商结果。
///
/// 当前阶段由宿主配置（启动参数 / 环境）注入；线缆级 `ClientHello` 能力字段属
/// `soxu-protocol` 冻结范围，由后续契约 PR 补齐后直接灌入本门控，判定逻辑不变。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FeatureGate {
    features: DaemonFeatureSet,
}

impl FeatureGate {
    /// 全关状态：M6 三组能力的默认协商结果。
    #[must_use]
    pub const fn disabled() -> Self {
        Self {
            features: DaemonFeatureSet::disabled(),
        }
    }

    /// 按已知能力位构造门控。
    #[must_use]
    pub fn from_features(features: impl IntoIterator<Item = DaemonFeature>) -> Self {
        Self {
            features: DaemonFeatureSet::from_features(features),
        }
    }

    /// 按线缆令牌列表构造门控；未知令牌原样保留但不参与判定。
    #[must_use]
    pub fn from_tokens(tokens: impl IntoIterator<Item = String>) -> Self {
        Self {
            features: DaemonFeatureSet {
                declared: tokens.into_iter().collect(),
            },
        }
    }

    /// 解析逗号分隔的能力令牌串（宿主配置形态，如 `managed_scripts,controlled_pty`）。
    ///
    /// 空串与纯空白解析为全关，保证缺省配置不缺省开启任何能力。
    #[must_use]
    pub fn from_csv(raw: &str) -> Self {
        Self::from_tokens(
            raw.split(',')
                .map(str::trim)
                .filter(|token| !token.is_empty())
                .map(ToOwned::to_owned),
        )
    }

    /// 判能力位是否已声明；未声明即关闭。
    #[must_use]
    pub fn is_enabled(&self, feature: DaemonFeature) -> bool {
        self.features.is_enabled(feature)
    }

    /// 已声明的能力集合（含未知令牌原文）。
    #[must_use]
    pub fn declared(&self) -> &DaemonFeatureSet {
        &self.features
    }

    /// 能力门：未声明时返回既有三要素错误模型，声明时放行。
    pub fn require(&self, feature: DaemonFeature) -> Result<(), ActionableError> {
        if self.is_enabled(feature) {
            return Ok(());
        }
        Err(ActionableError::new(
            ErrorCode::Unsupported,
            format!(
                "守护进程未协商 {} 能力（{}）",
                feature_label(feature),
                feature.token()
            ),
            "该请求未执行，未派生子进程、未创建会话，也没有下发任何事件",
            format!(
                "请先声明 `{}` 能力位后重试；能力位缺省关闭是 M6 契约红线",
                feature.token()
            ),
        )
        .with_diagnostic(format!("declared=[{}]", self.features.declared.join(","))))
    }
}

/// 能力位的中文展示名（错误文案用）。
#[must_use]
pub const fn feature_label(feature: DaemonFeature) -> &'static str {
    match feature {
        DaemonFeature::ManagedScripts => "受管脚本",
        DaemonFeature::Schedules => "定时调度",
        DaemonFeature::Heartbeats => "心跳续跑",
        DaemonFeature::ControlledPty => "受控终端",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_gate_keeps_every_feature_off() {
        let gate = FeatureGate::disabled();
        assert_eq!(gate, FeatureGate::default());
        for feature in DaemonFeature::ALL {
            assert!(!gate.is_enabled(feature), "{feature:?} 默认必须关闭");
            assert!(gate.require(feature).is_err());
        }
    }

    #[test]
    fn empty_configuration_never_enables_a_feature() {
        for raw in ["", "   ", ",", " , "] {
            let gate = FeatureGate::from_csv(raw);
            for feature in DaemonFeature::ALL {
                assert!(!gate.is_enabled(feature), "{raw:?} 不得开启 {feature:?}");
            }
        }
    }

    #[test]
    fn declared_tokens_enable_only_matching_features() {
        let gate = FeatureGate::from_csv("managed_scripts, controlled_pty");
        assert!(gate.is_enabled(DaemonFeature::ManagedScripts));
        assert!(gate.is_enabled(DaemonFeature::ControlledPty));
        assert!(!gate.is_enabled(DaemonFeature::Schedules));
        assert!(!gate.is_enabled(DaemonFeature::Heartbeats));
        assert!(gate.require(DaemonFeature::ManagedScripts).is_ok());
        assert!(gate.require(DaemonFeature::Schedules).is_err());
    }

    #[test]
    fn unknown_tokens_are_retained_but_never_grant_access() {
        let gate = FeatureGate::from_csv("future_capability");
        assert_eq!(gate.declared().unknown_tokens(), vec!["future_capability"]);
        for feature in DaemonFeature::ALL {
            assert!(!gate.is_enabled(feature));
        }
    }

    #[test]
    fn rejection_uses_three_part_error_model() {
        let err = FeatureGate::disabled()
            .require(DaemonFeature::ControlledPty)
            .expect_err("未声明必须拒绝");
        assert_eq!(err.code, ErrorCode::Unsupported);
        assert!(err.summary_zh.contains("受控终端"));
        assert!(err.summary_zh.contains("controlled_pty"));
        assert!(!err.impact_scope_zh.is_empty());
        assert!(!err.remediation_zh.is_empty());
        assert_eq!(
            err.diagnostic_detail.as_deref(),
            Some("declared=[]"),
            "诊断只暴露能力令牌，不泄漏其他信息"
        );
    }
}
