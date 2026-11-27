//! 双开源字体体系：界面/代码块独立配置 + `effective_code_family()` 语义。
//!
//! 规范来源（`MASTER.md` §3.1、`pages/07-settings-fonts-and-motion.md` §三）：
//! - 全开源 SIL OFL 1.1 字体，5 套界面预设 + 5 套代码预设；
//! - 独立/跟随双模式：`code_follows_ui = true` 时 [`FontConfig::effective_code_family`]
//!   直接返回界面字体族（保留 tabular-nums 等宽数字特性）；
//! - **本阶段不提交任何字体二进制**：预设只声明字体族名，字体文件引入方式与
//!   许可记录在 PR（M5 接入真实文本渲染时经 `Assets` 打包并核对 OFL 许可文本）。
//!
//! GPUI 缩放隔离铁律：界面铬层走 `sp()`（rems 随 UI 缩放），内容表面保持
//! `px()` 独立字号——本模块只管「用哪个字体族」，字号决策在视图层。

/// 界面字体预设（`uiPresets`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiFontPreset {
    /// `inter-noto`（默认）：Inter + Noto Sans SC。
    InterNoto,
    /// `ibm-plex-noto`。
    IbmPlexNoto,
    /// `jakarta-noto`。
    JakartaNoto,
    /// `lxgw-wenkai`：霞鹜文楷。
    LxgwWenkai,
    /// 系统原生无衬线回退。
    SystemSans,
}

/// 代码等宽字体预设（`codePresets`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodeFontPreset {
    /// `jetbrains-mono`（默认）。
    JetBrainsMono,
    /// `fira-code`。
    FiraCode,
    /// `maple-mono`（中英文严格 2:1 对齐）。
    MapleMono,
    /// `ibm-plex-mono`。
    IbmPlexMono,
    /// `cascadia-code`。
    CascadiaCode,
}

impl UiFontPreset {
    /// 全部预设，按规范声明顺序（供设置页枚举）。
    pub const ALL: [Self; 5] = [
        Self::InterNoto,
        Self::IbmPlexNoto,
        Self::JakartaNoto,
        Self::LxgwWenkai,
        Self::SystemSans,
    ];

    /// 偏好 Schema 序列化值。
    pub const fn id(self) -> &'static str {
        match self {
            Self::InterNoto => "inter-noto",
            Self::IbmPlexNoto => "ibm-plex-noto",
            Self::JakartaNoto => "jakarta-noto",
            Self::LxgwWenkai => "lxgw-wenkai",
            Self::SystemSans => "system-sans",
        }
    }

    /// GPUI 主字体族名（首个可解析的族；缺失时 GPUI 走内置回退栈）。
    pub const fn primary_family(self) -> &'static str {
        match self {
            // Windows 上 `.SystemUIFont` 解析为 Segoe UI；规范默认栈以 Inter 开头，
            // 未安装 Inter 时由 GPUI 回退栈承接（font.rs 不做运行时探测，保持零 I/O）。
            Self::InterNoto => "Inter",
            Self::IbmPlexNoto => "IBM Plex Sans",
            Self::JakartaNoto => "Plus Jakarta Sans",
            Self::LxgwWenkai => "Inter",
            Self::SystemSans => ".SystemUIFont",
        }
    }
}

impl CodeFontPreset {
    /// 全部预设，按规范声明顺序。
    pub const ALL: [Self; 5] = [
        Self::JetBrainsMono,
        Self::FiraCode,
        Self::MapleMono,
        Self::IbmPlexMono,
        Self::CascadiaCode,
    ];

    /// 偏好 Schema 序列化值。
    pub const fn id(self) -> &'static str {
        match self {
            Self::JetBrainsMono => "jetbrains-mono",
            Self::FiraCode => "fira-code",
            Self::MapleMono => "maple-mono",
            Self::IbmPlexMono => "ibm-plex-mono",
            Self::CascadiaCode => "cascadia-code",
        }
    }

    /// GPUI 等宽字体族名。
    pub const fn primary_family(self) -> &'static str {
        match self {
            Self::JetBrainsMono => "JetBrains Mono",
            Self::FiraCode => "Fira Code",
            Self::MapleMono => "Maple Mono NF CN",
            Self::IbmPlexMono => "IBM Plex Mono",
            Self::CascadiaCode => "Cascadia Code",
        }
    }
}

/// 字体配置：界面/代码独立预设 + 跟随开关。
///
/// 对齐偏好 Schema：`uiFontPreset` / `codeFontPreset` / `codeFollowsUi`。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FontConfig {
    /// 界面字体预设。
    pub ui_preset: UiFontPreset,
    /// 代码字体预设（独立选择）。
    pub code_preset: CodeFontPreset,
    /// 代码块字体跟随界面字体开关（默认 false）。
    pub code_follows_ui: bool,
}

impl Default for FontConfig {
    fn default() -> Self {
        Self {
            ui_preset: UiFontPreset::InterNoto,
            code_preset: CodeFontPreset::JetBrainsMono,
            code_follows_ui: false,
        }
    }
}

impl FontConfig {
    /// 界面字体族。
    pub fn ui_family(&self) -> &'static str {
        self.ui_preset.primary_family()
    }

    /// **实际生效**的代码字体族（联动契约核心）：
    /// `code_follows_ui = true` 时返回界面字体族（保留 tabular-nums 特性），
    /// 否则返回独立选择的代码字体族。
    pub fn effective_code_family(&self) -> &'static str {
        if self.code_follows_ui {
            self.ui_preset.primary_family()
        } else {
            self.code_preset.primary_family()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 默认配置：界面 Inter + 代码 JetBrains Mono，独立模式。
    #[test]
    fn defaults_match_spec() {
        let config = FontConfig::default();
        assert_eq!(config.ui_family(), "Inter");
        assert_eq!(config.effective_code_family(), "JetBrains Mono");
        assert!(!config.code_follows_ui);
    }

    /// 跟随开关：开启后代码族立即切到界面族（联动契约）。
    #[test]
    fn effective_code_family_follows_ui_when_enabled() {
        let config = FontConfig {
            ui_preset: UiFontPreset::IbmPlexNoto,
            code_preset: CodeFontPreset::FiraCode,
            code_follows_ui: true,
        };
        assert_eq!(config.ui_family(), "IBM Plex Sans");
        assert_eq!(config.effective_code_family(), "IBM Plex Sans");
    }

    /// 关闭跟随：独立代码族生效。
    #[test]
    fn effective_code_family_independent_when_disabled() {
        let config = FontConfig {
            ui_preset: UiFontPreset::IbmPlexNoto,
            code_preset: CodeFontPreset::FiraCode,
            code_follows_ui: false,
        };
        assert_eq!(config.effective_code_family(), "Fira Code");
    }

    /// 系统回退预设使用 `.SystemUIFont`（GPUI 虚拟族名，Windows → Segoe UI）。
    #[test]
    fn system_sans_uses_virtual_family() {
        let config = FontConfig {
            ui_preset: UiFontPreset::SystemSans,
            code_preset: CodeFontPreset::CascadiaCode,
            code_follows_ui: false,
        };
        assert_eq!(config.ui_family(), ".SystemUIFont");
        assert_eq!(config.effective_code_family(), "Cascadia Code");
    }

    /// 预设 id 与偏好 Schema 零漂移。
    #[test]
    fn preset_ids_match_preference_schema() {
        assert_eq!(UiFontPreset::ALL[0].id(), "inter-noto");
        assert_eq!(UiFontPreset::ALL[4].id(), "system-sans");
        assert_eq!(CodeFontPreset::ALL[0].id(), "jetbrains-mono");
        assert_eq!(CodeFontPreset::ALL[4].id(), "cascadia-code");
    }

    /// 开源许可纪律：预设族名全部来自 OFL 字体或系统虚拟族，不含专有付费字体。
    #[test]
    fn presets_are_open_source_families() {
        // OFL 体系内的已知族名集合（系统虚拟族与 Windows 自带 Cascadia 亦为 OFL）。
        for preset in UiFontPreset::ALL {
            assert!(
                matches!(
                    preset.primary_family(),
                    "Inter" | "IBM Plex Sans" | "Plus Jakarta Sans" | ".SystemUIFont"
                ),
                "{} 引入了规范外字体族",
                preset.id()
            );
        }
        for preset in CodeFontPreset::ALL {
            assert!(
                matches!(
                    preset.primary_family(),
                    "JetBrains Mono"
                        | "Fira Code"
                        | "Maple Mono NF CN"
                        | "IBM Plex Mono"
                        | "Cascadia Code"
                ),
                "{} 引入了规范外字体族",
                preset.id()
            );
        }
    }
}
