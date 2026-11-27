//! 最小可扩展 i18n 骨架：key→文案表 + 语言注册表，中文（zh-CN）默认。
//!
//! 设计约束（任务书 #124 / `pages/07-settings-fonts-and-motion.md` §2.2 分区 1）：
//! - **不引入 i18n 框架依赖**：静态表 + 运行时注册足够 M4 骨架使用；
//! - **可扩展**：新 locale 通过 [`register_locale`] 热注册（对齐偏好 Schema 的
//!   `registerLocalePack` 语义），key 缺失时逐级回落 zh-CN；
//! - 默认语言 `zh-CN`（偏好 Schema `locale` 默认值）。
//!
//! 后续接 `preferences.json` 持久化时，仅需把 [`LOCALE_DEFAULT`] 换成用户偏好读取。

use std::collections::HashMap;

use gpui::SharedString;

/// 默认语言（中文）。
pub const LOCALE_DEFAULT: &str = "zh-CN";

/// 单语言文案表：key → 文案。
pub type Messages = HashMap<&'static str, &'static str>;

/// 内置 zh-CN 文案表（默认语言，也是回落兜底）。
///
/// key 命名空间约定：`shell.*`（壳层骨架）、`settings.theme.*`（主题模式标签）。
fn zh_cn() -> Messages {
    Messages::from([
        ("app.name", "Soxu"),
        (
            "shell.titlebar.search_placeholder",
            "搜索会话 / 文件 / 命令...",
        ),
        ("shell.panel.sidebar", "快速对话与项目树"),
        ("shell.panel.canvas_empty", "要做点什么？"),
        ("shell.panel.inspector", "变更对比"),
        ("shell.statusbar.daemon_offline", "Daemon 未连接"),
        ("settings.theme.dark", "纯黑深色"),
        ("settings.theme.light", "纯净浅色"),
        ("settings.theme.system", "跟随系统"),
        ("settings.language", "界面语言"),
    ])
}

/// 内置 en-US 文案表（展示可扩展性；非默认语言，不参与回落链）。
fn en_us() -> Messages {
    Messages::from([
        ("app.name", "Soxu"),
        (
            "shell.titlebar.search_placeholder",
            "Search sessions / files / commands...",
        ),
        ("shell.panel.sidebar", "Quick Chats & Projects"),
        ("shell.panel.canvas_empty", "What do you want to do?"),
        ("shell.panel.inspector", "Changes"),
        ("shell.statusbar.daemon_offline", "Daemon offline"),
        ("settings.theme.dark", "Pure Black Dark"),
        ("settings.theme.light", "Clean Light"),
        ("settings.theme.system", "Follow System"),
        ("settings.language", "Interface language"),
    ])
}

/// 语言注册表：locale code → 文案表。
///
/// zh-CN 恒在且不可被覆盖注册（默认语言稳定性红线）。
#[derive(Clone, Debug)]
pub struct I18n {
    locales: HashMap<String, Messages>,
    active: String,
}

impl I18n {
    /// 以内置 zh-CN（默认）与 en-US 初始化，激活 zh-CN。
    pub fn new() -> Self {
        Self {
            locales: HashMap::from([
                (LOCALE_DEFAULT.to_string(), zh_cn()),
                ("en-US".to_string(), en_us()),
            ]),
            active: LOCALE_DEFAULT.to_string(),
        }
    }

    /// 当前激活语言。
    pub fn active_locale(&self) -> &str {
        &self.active
    }

    /// 已注册语言列表（排序后返回，供设置页语言切换入口）。
    pub fn available_locales(&self) -> Vec<&str> {
        let mut codes: Vec<&str> = self.locales.keys().map(String::as_str).collect();
        codes.sort_unstable();
        codes
    }

    /// 热注册一个语言包；返回是否注册成功。
    ///
    /// 拒绝覆盖 zh-CN（默认语言不可被运行时注册篡改）。
    pub fn register_locale(&mut self, code: &str, messages: Messages) -> bool {
        if code == LOCALE_DEFAULT {
            return false;
        }
        self.locales.insert(code.to_string(), messages);
        true
    }

    /// 切换激活语言；未注册的语言返回 `false` 且不改变当前语言。
    pub fn set_locale(&mut self, code: &str) -> bool {
        if self.locales.contains_key(code) {
            self.active = code.to_string();
            true
        } else {
            false
        }
    }

    /// 翻译：激活语言 → （缺失时回落 zh-CN）→（仍缺失时返回 key 本身，保证 UI 不出现空文案）。
    pub fn t(&self, key: &str) -> SharedString {
        self.locales
            .get(&self.active)
            .and_then(|m| m.get(key))
            .or_else(|| self.locales.get(LOCALE_DEFAULT).and_then(|m| m.get(key)))
            .map_or_else(
                || SharedString::from(key.to_string()),
                |s| SharedString::from(*s),
            )
    }
}

impl Default for I18n {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_locale_is_zh_cn() {
        let i18n = I18n::new();
        assert_eq!(i18n.active_locale(), "zh-CN");
        assert_eq!(i18n.t("app.name"), "Soxu");
        assert_eq!(i18n.t("shell.panel.canvas_empty"), "要做点什么？");
    }

    #[test]
    fn locale_switch_changes_text() {
        let mut i18n = I18n::new();
        assert!(i18n.set_locale("en-US"));
        assert_eq!(i18n.active_locale(), "en-US");
        assert_eq!(
            i18n.t("shell.panel.canvas_empty"),
            "What do you want to do?"
        );
    }

    #[test]
    fn unknown_key_falls_back_to_key_itself() {
        let i18n = I18n::new();
        assert_eq!(i18n.t("nonexistent.key"), "nonexistent.key");
    }

    #[test]
    fn missing_key_in_active_falls_back_to_zh_cn() {
        let mut i18n = I18n::new();
        // 注册一个只含部分 key 的语言并激活。
        let partial = Messages::from([("custom.only", "仅此一条")]);
        assert!(i18n.register_locale("xx-XX", partial));
        assert!(i18n.set_locale("xx-XX"));

        // 激活语言缺失的 key 回落 zh-CN。
        assert_eq!(i18n.t("app.name"), "Soxu");
        assert_eq!(i18n.t("custom.only"), "仅此一条");
    }

    #[test]
    fn switch_to_unknown_locale_is_rejected() {
        let mut i18n = I18n::new();
        assert!(!i18n.set_locale("fr-FR"));
        assert_eq!(i18n.active_locale(), "zh-CN");
    }

    #[test]
    fn zh_cn_cannot_be_overridden() {
        let mut i18n = I18n::new();
        let hostile = Messages::from([("app.name", "篡改")]);
        assert!(!i18n.register_locale("zh-CN", hostile));
        assert_eq!(i18n.t("app.name"), "Soxu");
    }

    #[test]
    fn hot_registration_is_listed_and_switchable() {
        let mut i18n = I18n::new();
        let ja = Messages::from([
            ("app.name", "Soxu"),
            ("shell.panel.canvas_empty", "何をする？"),
        ]);
        assert!(i18n.register_locale("ja-JP", ja));

        let codes = i18n.available_locales();
        assert!(codes.contains(&"ja-JP"));
        // 排序确定性。
        assert_eq!(codes.first(), Some(&"en-US"));

        assert!(i18n.set_locale("ja-JP"));
        assert_eq!(i18n.t("shell.panel.canvas_empty"), "何をする？");
    }

    /// 主题模式标签 key 与 `ThemeMode::label_key()` 零漂移守卫。
    #[test]
    fn theme_mode_label_keys_exist() {
        let i18n = I18n::new();
        for mode in crate::theme::theme_state::ThemeMode::ALL {
            let key = mode.label_key();
            // 翻译结果不得等于 key 本身（即 key 必须存在于文案表）。
            assert_ne!(i18n.t(key), key, "key {key} 缺少 zh-CN 文案");
        }
    }
}
