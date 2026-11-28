//! Worktree 启动环境透视器（`pages/04-magpie-and-cli-inspector.md` §5.2）。
//!
//! 规范要点：
//! - **只读**展示 `CARGO_TARGET_DIR` / `MAGPIE_GATEWAY` / `SOXU_MCP_DIRECT` 与
//!   RuntimeExtensionHook 槽位，消除多 Worktree 并发与 Magpie `Direct` 挂载的黑盒感；
//! - **强制脱敏**：变量名含 `KEY` / `TOKEN` / `SECRET` / `COOKIE` / `PASSWORD` 时，
//!   只展示键名并把值强制掩码为 `••••••••`；绝不读取或打印 `redact.key` 等凭据文件；
//! - **主目录脱敏折叠**：偏好 `redact_home_path` 打开时把用户主目录段折叠为 `~`；
//! - 无 Magpie 时按中性降级文案展示，不制造红色故障告警（§1 降级态不焦虑）。

/// 敏感变量值的统一掩码（§5.2 强制脱敏规则）。
pub const MASK: &str = "••••••••";

/// 敏感变量名的判定片段（大小写不敏感子串）。
pub const SENSITIVE_KEY_MARKERS: [&str; 5] = ["key", "token", "secret", "cookie", "password"];

/// 主目录前缀（用于折叠为 `~`；仅作字符串比较，不访问文件系统）。
const HOME_PREFIXES: [&str; 4] = ["c:/users/", "c:\\users\\", "/users/", "/home/"];

/// 启动环境路由双态（`pages/04` §2）+ 无 Magpie 时语义等价的降级态。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RouteMode {
    /// 检测到 Magpie 协同配置。
    MagpieManaged,
    /// 未安装 Magpie，回退原生目录扫描。
    #[default]
    StandaloneNative,
}

impl RouteMode {
    /// 顶部横幅文案。
    pub const fn label(self) -> &'static str {
        match self {
            Self::MagpieManaged => "Magpie 协同",
            Self::StandaloneNative => "独立原生模式 · Standalone Native",
        }
    }

    /// 是否为降级态（中性展示，不用绯红告警）。
    pub const fn is_standalone(self) -> bool {
        matches!(self, Self::StandaloneNative)
    }
}

/// 单条透视行：键、已脱敏的值、是否被掩码、可选徽标。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvRow {
    /// 变量名。
    pub key: String,
    /// 已脱敏的展示值。
    pub value: String,
    /// 值是否被掩码。
    pub masked: bool,
    /// 右侧微标签（如 `共享构建缓存命中`）。
    pub badge: Option<&'static str>,
}

impl EnvRow {
    /// 该行是否可被点击复制（掩码行禁止复制明文语义）。
    pub const fn copyable(&self) -> bool {
        !self.masked
    }
}

/// 变量名是否敏感（含 KEY / TOKEN / SECRET / COOKIE / PASSWORD）。
pub fn is_sensitive_key(key: &str) -> bool {
    let lowered = key.to_lowercase();
    SENSITIVE_KEY_MARKERS
        .iter()
        .any(|marker| lowered.contains(marker))
}

/// 键值脱敏：敏感键返回统一掩码，否则原样返回。
pub fn redact_value(key: &str, value: &str) -> String {
    if is_sensitive_key(key) {
        MASK.to_string()
    } else {
        value.to_string()
    }
}

/// 主目录折叠：`c:/users/<user>/proj` → `~/proj`，`/home/<user>` → `~`。
///
/// 纯词法处理，不探测文件系统；未命中主目录前缀时原样返回。
pub fn redact_home_in_paths(value: &str) -> String {
    let lowered = value.to_lowercase();
    for marker in HOME_PREFIXES {
        if let Some(position) = lowered.find(marker) {
            let rest = &value[position + marker.len()..];
            let segments = rest
                .split(['/', '\\'])
                .filter(|segment| !segment.is_empty())
                .skip(1)
                .collect::<Vec<_>>();
            return if segments.is_empty() {
                "~".to_string()
            } else {
                format!("~/{}", segments.join("/"))
            };
        }
    }
    value.to_string()
}

/// 启动环境透视器状态（只读）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchEnvInspector {
    /// 路由双态。
    pub mode: RouteMode,
    /// 是否折叠主目录路径（偏好 `redact_home_path`，默认 `true`）。
    pub redact_home: bool,
    /// 待展示的键值行。
    pub rows: Vec<EnvRow>,
}

impl Default for LaunchEnvInspector {
    fn default() -> Self {
        Self {
            mode: RouteMode::default(),
            redact_home: true,
            rows: Vec::new(),
        }
    }
}

impl LaunchEnvInspector {
    /// 由原始键值对构建：先按键掩码，再按偏好折叠主目录。
    pub fn from_pairs<I, K, V>(pairs: I, mode: RouteMode, redact_home: bool) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        let rows = pairs
            .into_iter()
            .map(|(key, value)| {
                let key = key.into();
                let raw = value.into();
                let masked = is_sensitive_key(&key);
                let value = if masked {
                    redact_value(&key, &raw)
                } else if redact_home {
                    redact_home_in_paths(&raw)
                } else {
                    raw
                };
                let badge = if masked {
                    Some("已脱敏")
                } else if key == "CARGO_TARGET_DIR" && !value.is_empty() {
                    Some("共享构建缓存命中")
                } else if key == "SOXU_MCP_DIRECT" && !value.is_empty() {
                    Some("按当前工作树绑定索引根")
                } else {
                    None
                };
                EnvRow {
                    key,
                    value,
                    masked,
                    badge,
                }
            })
            .collect();
        Self {
            mode,
            redact_home,
            rows,
        }
    }

    /// 按键取行。
    pub fn row(&self, key: &str) -> Option<&EnvRow> {
        self.rows.iter().find(|row| row.key == key)
    }

    /// 是否所有敏感键都已被掩码（强制脱敏不变量）。
    pub fn all_sensitive_masked(&self) -> bool {
        self.rows
            .iter()
            .filter(|row| is_sensitive_key(&row.key))
            .all(|row| row.masked && row.value == MASK)
    }

    /// `MAGPIE_GATEWAY` 展示值：降级态下明确「未启用，使用 CLI 原生凭据」。
    pub fn gateway_display(&self) -> String {
        match self.row("MAGPIE_GATEWAY") {
            Some(row) => row.value.clone(),
            None if self.mode.is_standalone() => "未启用，使用 CLI 原生凭据".to_string(),
            None => "-".to_string(),
        }
    }

    /// 空扩展槽位提示（§5.2 `NoopExtensionHost`）。
    pub fn empty_slot_note(&self) -> &'static str {
        "NoopExtensionHost · 预留扩展点未激活"
    }

    /// 追加/覆写一行（M6 由环境钩子上报时使用）。
    pub fn upsert(&mut self, key: impl Into<String>, value: impl Into<String>) {
        let key = key.into();
        let raw = value.into();
        let masked = is_sensitive_key(&key);
        let value = if masked {
            redact_value(&key, &raw)
        } else if self.redact_home {
            redact_home_in_paths(&raw)
        } else {
            raw
        };
        let badge = masked.then_some("已脱敏");
        let row = EnvRow {
            key: key.clone(),
            value,
            masked,
            badge,
        };
        match self.rows.iter_mut().find(|existing| existing.key == key) {
            Some(existing) => *existing = row,
            None => self.rows.push(row),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 敏感键判定覆盖五种片段且大小写不敏感。
    #[test]
    fn sensitive_keys_are_detected() {
        for key in [
            "OPENAI_API_KEY",
            "GITHUB_TOKEN",
            "app_secret",
            "session_cookie",
            "DB_PASSWORD",
            "apiKey",
        ] {
            assert!(is_sensitive_key(key), "{key} 应判定为敏感");
        }
        for key in [
            "CARGO_TARGET_DIR",
            "MAGPIE_GATEWAY",
            "SOXU_MCP_DIRECT",
            "PATH",
        ] {
            assert!(!is_sensitive_key(key), "{key} 不应判定为敏感");
        }
    }

    /// 敏感键值强制掩码，普通键保留原值。
    #[test]
    fn redacts_sensitive_values_only() {
        assert_eq!(redact_value("GITHUB_TOKEN", "ghp_placeholder"), MASK);
        assert_eq!(
            redact_value("CARGO_TARGET_DIR", "../.cargo-target/soxu"),
            "../.cargo-target/soxu"
        );
        assert_eq!(MASK.chars().count(), 8);
    }

    /// 主目录折叠：Windows / POSIX 两种前缀都收敛为 `~`。
    #[test]
    fn folds_home_prefix_lexically() {
        assert_eq!(redact_home_in_paths("c:/users/example/proj"), "~/proj");
        assert_eq!(redact_home_in_paths("/home/example/x/y"), "~/x/y");
        assert_eq!(redact_home_in_paths("/home/example"), "~");
        assert_eq!(redact_home_in_paths("/tmp/build"), "/tmp/build");
        assert_eq!(redact_home_in_paths(""), "");
    }

    /// 透视器构建：掩码、徽标与主目录折叠同时生效。
    #[test]
    fn builds_rows_with_badges_and_masking() {
        let inspector = LaunchEnvInspector::from_pairs(
            [
                ("CARGO_TARGET_DIR", "../.cargo-target/soxu"),
                ("MAGPIE_GATEWAY", "http://127.0.0.1:3425/v1"),
                ("SOXU_MCP_DIRECT", "aoci, codegraph"),
                ("GITHUB_TOKEN", "ghp_placeholder"),
                ("HOME_PROJECT", "c:/users/example/soxu"),
            ],
            RouteMode::MagpieManaged,
            true,
        );

        assert_eq!(inspector.rows.len(), 5);
        assert!(inspector.all_sensitive_masked());
        assert_eq!(
            inspector.row("GITHUB_TOKEN").map(|row| (
                row.value.as_str(),
                row.masked,
                row.copyable()
            )),
            Some((MASK, true, false))
        );
        assert_eq!(
            inspector.row("CARGO_TARGET_DIR").and_then(|row| row.badge),
            Some("共享构建缓存命中")
        );
        assert_eq!(
            inspector.row("SOXU_MCP_DIRECT").and_then(|row| row.badge),
            Some("按当前工作树绑定索引根")
        );
        assert_eq!(
            inspector.row("HOME_PROJECT").map(|row| row.value.as_str()),
            Some("~/soxu")
        );
    }

    /// 关闭主目录折叠后保留原值。
    #[test]
    fn home_redaction_can_be_disabled() {
        let inspector = LaunchEnvInspector::from_pairs(
            [("HOME_PROJECT", "c:/users/example/soxu")],
            RouteMode::StandaloneNative,
            false,
        );
        assert_eq!(
            inspector.row("HOME_PROJECT").map(|row| row.value.as_str()),
            Some("c:/users/example/soxu")
        );
    }

    /// 降级态：无网关行时给出中性说明，不报错。
    #[test]
    fn standalone_mode_has_neutral_gateway_copy() {
        let inspector = LaunchEnvInspector::from_pairs(
            [("CARGO_TARGET_DIR", "../.cargo-target/soxu")],
            RouteMode::StandaloneNative,
            true,
        );
        assert!(inspector.mode.is_standalone());
        assert_eq!(inspector.mode.label(), "独立原生模式 · Standalone Native");
        assert_eq!(inspector.gateway_display(), "未启用，使用 CLI 原生凭据");
        assert_eq!(
            inspector.empty_slot_note(),
            "NoopExtensionHost · 预留扩展点未激活"
        );

        // 协同态且缺行时回落占位符。
        let managed = LaunchEnvInspector::from_pairs(
            [("SOXU_MCP_DIRECT", "aoci")],
            RouteMode::MagpieManaged,
            true,
        );
        assert_eq!(managed.gateway_display(), "-");
    }

    /// upsert：已有键覆写，新键追加，敏感键始终掩码。
    #[test]
    fn upsert_overwrites_and_masks() {
        let mut inspector = LaunchEnvInspector::default();
        inspector.upsert("MAGPIE_GATEWAY", "http://127.0.0.1:3425/v1");
        inspector.upsert("MAGPIE_GATEWAY", "http://127.0.0.1:3426/v1");
        assert_eq!(inspector.rows.len(), 1);
        assert_eq!(inspector.gateway_display(), "http://127.0.0.1:3426/v1");

        inspector.upsert("APP_SECRET", "plain-text");
        assert!(inspector.all_sensitive_masked());
        assert_eq!(
            inspector.row("APP_SECRET").map(|row| row.value.as_str()),
            Some(MASK)
        );
    }
}
