//! 双端同构用户偏好配置 Schema（与前端 `apps/mobile-web/src/lib/preferences.ts` 1:1 映射）。

use crate::domain::PermissionLevel;
use serde::{Deserialize, Serialize};

/// 主题外观模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ThemeMode {
    #[default]
    Dark,
    Light,
    System,
}

/// 列表与树行视觉密度。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DensityMode {
    #[default]
    Compact,
    Comfortable,
}

/// 主题强调色预设枚举（含用户自定义 Hex 模式）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AccentPresetId {
    #[default]
    Emerald,
    Ocean,
    Violet,
    Amber,
    Rose,
    Cyan,
    Custom,
}

/// 正文行距档位。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LineHeightMode {
    Compact,
    #[default]
    Normal,
    Relaxed,
}

/// 时间戳显示格式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TimestampFormatMode {
    #[default]
    Relative,
    Absolute,
}

/// Worktree 默认创建位置策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WorktreeLocationMode {
    #[default]
    Sibling,
    Central,
}

/// SIL OFL 1.1 开源界面无衬线字体预设。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum UiFontPresetId {
    #[default]
    InterNoto,
    IbmPlexNoto,
    JakartaNoto,
    LxgwWenkai,
    SystemSans,
}

/// SIL OFL 1.1 开源等宽代码字体预设。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum CodeFontPresetId {
    #[default]
    JetbrainsMono,
    FiraCode,
    MapleMono,
    IbmPlexMono,
    CascadiaCode,
}

/// 桌面端与移动端共享持久化偏好设置（采用 camelCase 与 Web JSON 完全一致）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SoxuPreferences {
    pub theme_mode: ThemeMode,
    pub locale: String,
    pub accent_preset: AccentPresetId,
    pub custom_accent_hex: String,
    pub density: DensityMode,
    pub ui_line_height: LineHeightMode,
    pub timestamp_format: TimestampFormatMode,
    pub left_sidebar_width: u16,
    pub right_inspector_width: u16,
    pub ui_font_preset: UiFontPresetId,
    pub code_follows_ui: bool,
    pub code_font_preset: CodeFontPresetId,
    pub ui_font_size: u8,
    pub code_font_size: u8,
    pub tab_size: u8,
    pub show_line_numbers: bool,
    pub code_word_wrap: bool,
    pub code_ligatures: bool,
    pub reduce_motion: bool,
    pub probe_interval_sec: u16,
    pub auto_checkpoint: bool,
    pub shared_cargo_target: bool,
    pub warn_uncommitted_on_close: bool,
    pub commit_emoji_prefix: bool,
    pub worktree_location: WorktreeLocationMode,
    pub default_branch: String,
    pub default_perm_mode: PermissionLevel,
    pub command_timeout_sec: u32,
    pub redact_home_path: bool,
    pub lan_remote_enabled: bool,
    pub remote_idle_lock_min: u16,
}

impl Default for SoxuPreferences {
    fn default() -> Self {
        Self {
            theme_mode: ThemeMode::Dark,
            locale: "zh-CN".to_owned(),
            accent_preset: AccentPresetId::Emerald,
            custom_accent_hex: "#6366f1".to_owned(),
            density: DensityMode::Compact,
            ui_line_height: LineHeightMode::Normal,
            timestamp_format: TimestampFormatMode::Relative,
            left_sidebar_width: 264,
            right_inspector_width: 380,
            ui_font_preset: UiFontPresetId::InterNoto,
            code_follows_ui: false,
            code_font_preset: CodeFontPresetId::JetbrainsMono,
            ui_font_size: 14,
            code_font_size: 13,
            tab_size: 2,
            show_line_numbers: true,
            code_word_wrap: false,
            code_ligatures: true,
            reduce_motion: false,
            probe_interval_sec: 30,
            auto_checkpoint: true,
            shared_cargo_target: true,
            warn_uncommitted_on_close: true,
            commit_emoji_prefix: true,
            worktree_location: WorktreeLocationMode::Sibling,
            default_branch: "origin/main".to_owned(),
            default_perm_mode: PermissionLevel::Ask,
            command_timeout_sec: 120,
            redact_home_path: true,
            lan_remote_enabled: true,
            remote_idle_lock_min: 15,
        }
    }
}
