//! 全局主题状态：明暗模式解析、切换与跟随系统。
//!
//! 颜色本体一律来自 [`crate::theme::gpui_colors::Palette`]（唯一真相源
//! `design-system/soxu/tokens.json`），本模块只持有「当前处于哪种模式」这一
//! 可变状态，不复制任何色值。
//!
//! - [`ThemeMode::Dark`] / [`ThemeMode::Light`]：用户显式选择；
//! - [`ThemeMode::System`]：跟随操作系统外观（由入口在窗口 appearance 变化时刷新）。
//!
//! gpui-component 的全局 `Theme`（组件皮肤层）与本状态在 M5 接入组件时对齐；
//! 本阶段自绘骨架直接读 `Palette::dark()/light()`。

use gpui::{App, BorrowAppContext, Global, WindowAppearance};

use crate::theme::gpui_colors::Palette;

/// 用户可选择的主题模式。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThemeMode {
    /// 纯黑深色（ChatGPT Pure Black 对标，`tokens.json` defaultMode）。
    Dark,
    /// 纯净浅色。
    Light,
    /// 跟随操作系统外观变化。
    #[default]
    System,
}

impl ThemeMode {
    /// 全部模式，供设置页枚举（顺序即 UI 呈现顺序）。
    pub const ALL: [Self; 3] = [Self::Dark, Self::Light, Self::System];

    /// 模式的 i18n key（`i18n` 模块负责翻译）。
    pub const fn label_key(self) -> &'static str {
        match self {
            Self::Dark => "settings.theme.dark",
            Self::Light => "settings.theme.light",
            Self::System => "settings.theme.system",
        }
    }

    /// 结合系统外观解析出实际明暗。
    pub fn resolves_to_dark(self, system_appearance: WindowAppearance) -> bool {
        match self {
            Self::Dark => true,
            Self::Light => false,
            Self::System => matches!(
                system_appearance,
                WindowAppearance::Dark | WindowAppearance::VibrantDark
            ),
        }
    }
}

/// 全局主题状态（gpui `Global`）。
#[derive(Clone, Copy, Debug)]
pub struct ThemeState {
    /// 用户偏好模式（含 System）。
    pub preference: ThemeMode,
    /// 最近一次已解析的系统外观（System 模式刷新来源）。
    system_appearance: WindowAppearance,
}

impl Global for ThemeState {}

impl ThemeState {
    /// 以解析后的初始模式创建全局状态；偏好默认跟随系统。
    pub fn new(initial_mode: ThemeMode) -> Self {
        let preference = ThemeMode::System;
        let system_appearance = match initial_mode {
            ThemeMode::Dark => WindowAppearance::Dark,
            _ => WindowAppearance::Light,
        };
        Self {
            preference,
            system_appearance,
        }
    }

    /// 直接以偏好 + 系统外观创建（测试与入口共用）。
    pub fn with_preference(preference: ThemeMode, system_appearance: WindowAppearance) -> Self {
        Self {
            preference,
            system_appearance,
        }
    }

    /// 系统外观变化回调（窗口 appearance observer 调用）。
    pub fn set_system_appearance(&mut self, appearance: WindowAppearance) {
        self.system_appearance = appearance;
    }

    /// 当前解析出的实际明暗。
    pub fn is_dark(&self) -> bool {
        self.preference.resolves_to_dark(self.system_appearance)
    }

    /// 当前生效色板。
    pub fn palette(&self) -> Palette {
        if self.is_dark() {
            Palette::dark()
        } else {
            Palette::light()
        }
    }
}

/// 读取当前全局主题状态；未初始化时按纯黑深色兜底（设计默认模式）。
pub fn current(cx: &App) -> ThemeState {
    if cx.has_global::<ThemeState>() {
        *cx.global::<ThemeState>()
    } else {
        ThemeState::with_preference(ThemeMode::Dark, WindowAppearance::Dark)
    }
}

/// 切换用户偏好模式。
pub fn set_preference(preference: ThemeMode, cx: &mut App) {
    cx.update_global::<ThemeState, _>(|state, _| state.preference = preference);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_mode_always_resolves_dark() {
        for appearance in [
            WindowAppearance::Light,
            WindowAppearance::Dark,
            WindowAppearance::VibrantLight,
            WindowAppearance::VibrantDark,
        ] {
            assert!(ThemeMode::Dark.resolves_to_dark(appearance));
        }
    }

    #[test]
    fn light_mode_always_resolves_light() {
        for appearance in [
            WindowAppearance::Light,
            WindowAppearance::Dark,
            WindowAppearance::VibrantLight,
            WindowAppearance::VibrantDark,
        ] {
            assert!(!ThemeMode::Light.resolves_to_dark(appearance));
        }
    }

    #[test]
    fn system_mode_follows_appearance() {
        assert!(ThemeMode::System.resolves_to_dark(WindowAppearance::Dark));
        assert!(ThemeMode::System.resolves_to_dark(WindowAppearance::VibrantDark));
        assert!(!ThemeMode::System.resolves_to_dark(WindowAppearance::Light));
        assert!(!ThemeMode::System.resolves_to_dark(WindowAppearance::VibrantLight));
    }

    #[test]
    fn theme_state_palette_matches_resolution() {
        let dark_state = ThemeState::with_preference(ThemeMode::System, WindowAppearance::Dark);
        assert!(dark_state.is_dark());
        assert_eq!(dark_state.palette(), Palette::dark());

        let light_state = ThemeState::with_preference(ThemeMode::System, WindowAppearance::Light);
        assert!(!light_state.is_dark());
        assert_eq!(light_state.palette(), Palette::light());

        // 系统外观刷新后解析随动。
        let mut state = dark_state;
        state.set_system_appearance(WindowAppearance::Light);
        assert!(!state.is_dark());
    }

    #[test]
    fn default_preference_is_system() {
        let state = ThemeState::new(ThemeMode::Dark);
        assert_eq!(state.preference, ThemeMode::System);
    }
}
