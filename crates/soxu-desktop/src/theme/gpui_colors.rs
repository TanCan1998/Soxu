//! 设计令牌 → GPUI 颜色类型映射（M4-W1b 最小消费层）。
//!
//! 唯一真相源仍是 `design-system/soxu/tokens.json`：本模块**不复制任何色值**，只把
//! [`crate::theme::tokens`] 中由 `scripts/sync-design-tokens.mjs` 编译出的 `u32` 常量
//! 转换为 GPUI 颜色类型，供后续视图直接消费。
//!
//! 令牌编码约定（与 `tokens.rs` 文档一致）：
//! - 不透明令牌为 `0xRRGGBB`，经 [`gpui::rgb`] 转换；
//! - 带透明度的令牌为 `0xRRGGBBAA`，经 [`gpui::rgba`] 转换。
//!
//! 本模块只做颜色映射，不绘制任何界面。

use gpui::{Hsla, Rgba, rgb, rgba};

use crate::theme::tokens;
use crate::theme::tokens::color::{dark, light};

/// 不透明令牌（`0xRRGGBB`）→ GPUI [`Rgba`]。
pub fn solid(token: u32) -> Rgba {
    rgb(token)
}

/// 带透明度令牌（`0xRRGGBBAA`，alpha 在低 8 位）→ GPUI [`Rgba`]。
pub fn translucent(token: u32) -> Rgba {
    rgba(token)
}

/// 不透明令牌（`0xRRGGBB`）→ GPUI [`Hsla`]（alpha = 1.0）。
pub fn solid_hsla(token: u32) -> Hsla {
    rgb(token).into()
}

/// 带透明度令牌（`0xRRGGBBAA`，alpha 在低 8 位）→ GPUI [`Hsla`]。
pub fn translucent_hsla(token: u32) -> Hsla {
    rgba(token).into()
}

/// 语义色板：把一组语义令牌映射为 GPUI [`Hsla`]，供视图直接着色。
///
/// 每个字段都在文档注释中标明其对应的源令牌路径，保证与 `tokens.json` 的映射可审计。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    /// 是否为深色模式。
    pub is_dark: bool,

    /// `color.*.surface.inset`
    pub surface_inset: Hsla,
    /// `color.*.surface.canvas`
    pub surface_canvas: Hsla,
    /// `color.*.surface.sidebar`
    pub surface_sidebar: Hsla,
    /// `color.*.surface.card`
    pub surface_card: Hsla,
    /// `color.*.surface.cardMuted`
    pub surface_card_muted: Hsla,
    /// `color.*.surface.raised`
    pub surface_raised: Hsla,
    /// `color.*.surface.composer`
    pub surface_composer: Hsla,
    /// `color.*.surface.floating`
    pub surface_floating: Hsla,
    /// `color.*.surface.control`
    pub surface_control: Hsla,
    /// `color.*.surface.terminal`
    pub surface_terminal: Hsla,
    /// `color.*.surface.scrim`（含 alpha）
    pub surface_scrim: Hsla,
    /// `color.*.surface.headerGlass`（含 alpha）
    pub surface_header_glass: Hsla,

    /// `color.*.interaction.hover`（含 alpha）
    pub interaction_hover: Hsla,
    /// `color.*.interaction.selected`（含 alpha）
    pub interaction_selected: Hsla,
    /// `color.*.interaction.pressed`（含 alpha）
    pub interaction_pressed: Hsla,
    /// `color.*.interaction.selection`（含 alpha）
    pub interaction_selection: Hsla,
    /// `color.*.interaction.scrollbarThumb`（含 alpha）
    pub interaction_scrollbar_thumb: Hsla,
    /// `color.*.interaction.scrollbarThumbHover`（含 alpha）
    pub interaction_scrollbar_thumb_hover: Hsla,

    /// `color.*.text.primary`
    pub text_primary: Hsla,
    /// `color.*.text.secondary`
    pub text_secondary: Hsla,
    /// `color.*.text.tertiary`
    pub text_tertiary: Hsla,
    /// `color.*.text.ghost`
    pub text_ghost: Hsla,
    /// `color.*.text.code`
    pub text_code: Hsla,

    /// `color.*.border.hairline`
    pub border_hairline: Hsla,
    /// `color.*.border.subtle`
    pub border_subtle: Hsla,
    /// `color.*.border.strong`
    pub border_strong: Hsla,
    /// `color.*.border.focusRing`
    pub border_focus_ring: Hsla,

    /// `color.*.action.inverse`
    pub action_inverse: Hsla,
    /// `color.*.action.inverseHover`
    pub action_inverse_hover: Hsla,
    /// `color.*.action.onInverse`
    pub action_on_inverse: Hsla,
    /// `color.*.action.disabledBg`
    pub action_disabled_bg: Hsla,
    /// `color.*.action.disabledFg`
    pub action_disabled_fg: Hsla,

    /// `color.*.semantic.accent`
    pub semantic_accent: Hsla,
    /// `color.*.semantic.accentSoft`（含 alpha）
    pub semantic_accent_soft: Hsla,
    /// `color.*.semantic.accentBorder`（含 alpha）
    pub semantic_accent_border: Hsla,
    /// `color.*.semantic.running`
    pub semantic_running: Hsla,
    /// `color.*.semantic.success`
    pub semantic_success: Hsla,
    /// `color.*.semantic.warning`
    pub semantic_warning: Hsla,
    /// `color.*.semantic.danger`
    pub semantic_danger: Hsla,
    /// `color.*.semantic.handoff`
    pub semantic_handoff: Hsla,
    /// `color.*.semantic.codeWash`（含 alpha）
    pub semantic_code_wash: Hsla,
}

impl Palette {
    /// 深色模式语义色板（默认模式）。
    pub fn dark() -> Self {
        Self {
            is_dark: true,
            surface_inset: solid_hsla(dark::SURFACE_INSET),
            surface_canvas: solid_hsla(dark::SURFACE_CANVAS),
            surface_sidebar: solid_hsla(dark::SURFACE_SIDEBAR),
            surface_card: solid_hsla(dark::SURFACE_CARD),
            surface_card_muted: solid_hsla(dark::SURFACE_CARD_MUTED),
            surface_raised: solid_hsla(dark::SURFACE_RAISED),
            surface_composer: solid_hsla(dark::SURFACE_COMPOSER),
            surface_floating: solid_hsla(dark::SURFACE_FLOATING),
            surface_control: solid_hsla(dark::SURFACE_CONTROL),
            surface_terminal: solid_hsla(dark::SURFACE_TERMINAL),
            surface_scrim: translucent_hsla(dark::SURFACE_SCRIM),
            surface_header_glass: translucent_hsla(dark::SURFACE_HEADER_GLASS),
            interaction_hover: translucent_hsla(dark::INTERACTION_HOVER),
            interaction_selected: translucent_hsla(dark::INTERACTION_SELECTED),
            interaction_pressed: translucent_hsla(dark::INTERACTION_PRESSED),
            interaction_selection: translucent_hsla(dark::INTERACTION_SELECTION),
            interaction_scrollbar_thumb: translucent_hsla(dark::INTERACTION_SCROLLBAR_THUMB),
            interaction_scrollbar_thumb_hover: translucent_hsla(
                dark::INTERACTION_SCROLLBAR_THUMB_HOVER,
            ),
            text_primary: solid_hsla(dark::TEXT_PRIMARY),
            text_secondary: solid_hsla(dark::TEXT_SECONDARY),
            text_tertiary: solid_hsla(dark::TEXT_TERTIARY),
            text_ghost: solid_hsla(dark::TEXT_GHOST),
            text_code: solid_hsla(dark::TEXT_CODE),
            border_hairline: solid_hsla(dark::BORDER_HAIRLINE),
            border_subtle: solid_hsla(dark::BORDER_SUBTLE),
            border_strong: solid_hsla(dark::BORDER_STRONG),
            border_focus_ring: solid_hsla(dark::BORDER_FOCUS_RING),
            action_inverse: solid_hsla(dark::ACTION_INVERSE),
            action_inverse_hover: solid_hsla(dark::ACTION_INVERSE_HOVER),
            action_on_inverse: solid_hsla(dark::ACTION_ON_INVERSE),
            action_disabled_bg: solid_hsla(dark::ACTION_DISABLED_BG),
            action_disabled_fg: solid_hsla(dark::ACTION_DISABLED_FG),
            semantic_accent: solid_hsla(dark::SEMANTIC_ACCENT),
            semantic_accent_soft: translucent_hsla(dark::SEMANTIC_ACCENT_SOFT),
            semantic_accent_border: translucent_hsla(dark::SEMANTIC_ACCENT_BORDER),
            semantic_running: solid_hsla(dark::SEMANTIC_RUNNING),
            semantic_success: solid_hsla(dark::SEMANTIC_SUCCESS),
            semantic_warning: solid_hsla(dark::SEMANTIC_WARNING),
            semantic_danger: solid_hsla(dark::SEMANTIC_DANGER),
            semantic_handoff: solid_hsla(dark::SEMANTIC_HANDOFF),
            semantic_code_wash: translucent_hsla(dark::SEMANTIC_CODE_WASH),
        }
    }

    /// 浅色模式语义色板。
    pub fn light() -> Self {
        Self {
            is_dark: false,
            surface_inset: solid_hsla(light::SURFACE_INSET),
            surface_canvas: solid_hsla(light::SURFACE_CANVAS),
            surface_sidebar: solid_hsla(light::SURFACE_SIDEBAR),
            surface_card: solid_hsla(light::SURFACE_CARD),
            surface_card_muted: solid_hsla(light::SURFACE_CARD_MUTED),
            surface_raised: solid_hsla(light::SURFACE_RAISED),
            surface_composer: solid_hsla(light::SURFACE_COMPOSER),
            surface_floating: solid_hsla(light::SURFACE_FLOATING),
            surface_control: solid_hsla(light::SURFACE_CONTROL),
            surface_terminal: solid_hsla(light::SURFACE_TERMINAL),
            surface_scrim: translucent_hsla(light::SURFACE_SCRIM),
            surface_header_glass: translucent_hsla(light::SURFACE_HEADER_GLASS),
            interaction_hover: translucent_hsla(light::INTERACTION_HOVER),
            interaction_selected: translucent_hsla(light::INTERACTION_SELECTED),
            interaction_pressed: translucent_hsla(light::INTERACTION_PRESSED),
            interaction_selection: translucent_hsla(light::INTERACTION_SELECTION),
            interaction_scrollbar_thumb: translucent_hsla(light::INTERACTION_SCROLLBAR_THUMB),
            interaction_scrollbar_thumb_hover: translucent_hsla(
                light::INTERACTION_SCROLLBAR_THUMB_HOVER,
            ),
            text_primary: solid_hsla(light::TEXT_PRIMARY),
            text_secondary: solid_hsla(light::TEXT_SECONDARY),
            text_tertiary: solid_hsla(light::TEXT_TERTIARY),
            text_ghost: solid_hsla(light::TEXT_GHOST),
            text_code: solid_hsla(light::TEXT_CODE),
            border_hairline: solid_hsla(light::BORDER_HAIRLINE),
            border_subtle: solid_hsla(light::BORDER_SUBTLE),
            border_strong: solid_hsla(light::BORDER_STRONG),
            border_focus_ring: solid_hsla(light::BORDER_FOCUS_RING),
            action_inverse: solid_hsla(light::ACTION_INVERSE),
            action_inverse_hover: solid_hsla(light::ACTION_INVERSE_HOVER),
            action_on_inverse: solid_hsla(light::ACTION_ON_INVERSE),
            action_disabled_bg: solid_hsla(light::ACTION_DISABLED_BG),
            action_disabled_fg: solid_hsla(light::ACTION_DISABLED_FG),
            semantic_accent: solid_hsla(light::SEMANTIC_ACCENT),
            semantic_accent_soft: translucent_hsla(light::SEMANTIC_ACCENT_SOFT),
            semantic_accent_border: translucent_hsla(light::SEMANTIC_ACCENT_BORDER),
            semantic_running: solid_hsla(light::SEMANTIC_RUNNING),
            semantic_success: solid_hsla(light::SEMANTIC_SUCCESS),
            semantic_warning: solid_hsla(light::SEMANTIC_WARNING),
            semantic_danger: solid_hsla(light::SEMANTIC_DANGER),
            semantic_handoff: solid_hsla(light::SEMANTIC_HANDOFF),
            semantic_code_wash: translucent_hsla(light::SEMANTIC_CODE_WASH),
        }
    }
}

/// 强调色预设（`accentPresets.*`），每枚预设提供深/浅两种不透明色。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccentPreset {
    /// 翠玉绿（默认）。
    Emerald,
    /// 极海蓝。
    Ocean,
    /// 星云紫。
    Violet,
    /// 琥珀橙。
    Amber,
    /// 绯霞红。
    Rose,
    /// 晴空青。
    Cyan,
}

impl AccentPreset {
    /// 全部预设，按 `tokens.json` 中的声明顺序。
    pub const ALL: [Self; 6] = [
        Self::Emerald,
        Self::Ocean,
        Self::Violet,
        Self::Amber,
        Self::Rose,
        Self::Cyan,
    ];

    /// 该预设在深色模式下的强调色。
    pub fn dark(self) -> Hsla {
        solid_hsla(match self {
            Self::Emerald => tokens::accent_presets::emerald::DARK,
            Self::Ocean => tokens::accent_presets::ocean::DARK,
            Self::Violet => tokens::accent_presets::violet::DARK,
            Self::Amber => tokens::accent_presets::amber::DARK,
            Self::Rose => tokens::accent_presets::rose::DARK,
            Self::Cyan => tokens::accent_presets::cyan::DARK,
        })
    }

    /// 该预设在浅色模式下的强调色。
    pub fn light(self) -> Hsla {
        solid_hsla(match self {
            Self::Emerald => tokens::accent_presets::emerald::LIGHT,
            Self::Ocean => tokens::accent_presets::ocean::LIGHT,
            Self::Violet => tokens::accent_presets::violet::LIGHT,
            Self::Amber => tokens::accent_presets::amber::LIGHT,
            Self::Rose => tokens::accent_presets::rose::LIGHT,
            Self::Cyan => tokens::accent_presets::cyan::LIGHT,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::tokens;

    /// 浮点比较容差：GPUI 的通道值来自 f32 除法，留半个 8 位量化步长。
    const EPS: f32 = 1.0 / 512.0;

    /// 按令牌编码拆分期望的 RGBA 通道；`has_alpha` 由调用方依据 tokens.rs
    /// 文档注释显式给出（黑色+alpha 令牌无法从数值本身识别）。
    /// 含 alpha 令牌为 `0xRRGGBBAA`：alpha 在低 8 位（与 gpui `rgba()` 的大端
    /// 字节序一致，已核对 gpui-pre 0.3.14 `src/color.rs`）。
    fn expected_channels(token: u32, has_alpha: bool) -> (f32, f32, f32, f32) {
        let channel = |shift: u32| ((token >> shift) & 0xff) as f32 / 255.0;
        if has_alpha {
            (channel(24), channel(16), channel(8), channel(0))
        } else {
            (channel(16), channel(8), channel(0), 1.0)
        }
    }

    fn assert_approx(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= EPS,
            "通道值不符：actual={actual}, expected={expected}"
        );
    }

    fn assert_rgba(actual: Rgba, token: u32, has_alpha: bool) {
        let (r, g, b, a) = expected_channels(token, has_alpha);
        assert_approx(actual.r, r);
        assert_approx(actual.g, g);
        assert_approx(actual.b, b);
        assert_approx(actual.a, a);
    }

    #[test]
    fn solid_keeps_rgb_and_forces_opaque_alpha() {
        // 每类各取一枚：表面、文字、边框、语义强调色。
        for token in [
            tokens::color::dark::SURFACE_CARD,
            tokens::color::dark::TEXT_PRIMARY,
            tokens::color::dark::BORDER_STRONG,
            tokens::color::dark::SEMANTIC_ACCENT,
            tokens::color::light::SURFACE_CANVAS,
        ] {
            let color = solid(token);
            assert_rgba(color, token, false);
            assert_approx(color.a, 1.0);
        }
    }

    #[test]
    fn translucent_keeps_token_alpha() {
        // 黑底 + alpha 令牌（`0x000000c2` = R0 G0 B0 A=c2）：像素全黑、alpha 必须保留。
        let scrim = translucent(tokens::color::dark::SURFACE_SCRIM);
        assert_rgba(scrim, tokens::color::dark::SURFACE_SCRIM, true);
        assert_approx(scrim.a, 0xc2 as f32 / 255.0);
        assert_approx(scrim.r, 0.0);
        assert_approx(scrim.b, 0.0);

        // 非零 RGB 的含 alpha 令牌：像素色与 alpha 同时保留。
        let selection = translucent(tokens::color::dark::INTERACTION_SELECTION);
        assert_rgba(selection, tokens::color::dark::INTERACTION_SELECTION, true);
        assert!(selection.a < 1.0);

        // 近白像素 + alpha：玻璃材质令牌。
        let glass = translucent(tokens::color::light::SURFACE_HEADER_GLASS);
        assert_rgba(glass, tokens::color::light::SURFACE_HEADER_GLASS, true);
    }

    #[test]
    fn hsla_mapping_preserves_alpha() {
        assert_approx(solid_hsla(tokens::color::dark::TEXT_PRIMARY).a, 1.0);
        assert_approx(
            translucent_hsla(tokens::color::dark::SURFACE_SCRIM).a,
            0xc2 as f32 / 255.0,
        );
    }

    /// 逐一核对 Palette 字段与源令牌的对应关系；`has_alpha` 与 tokens.rs
    /// 文档注释中的「含 alpha」标记一致。
    macro_rules! palette_cases {
        ($mode:ident) => {{
            let p = if stringify!($mode) == "dark" {
                Palette::dark()
            } else {
                Palette::light()
            };
            vec![
                (p.surface_inset, tokens::color::$mode::SURFACE_INSET, false),
                (
                    p.surface_canvas,
                    tokens::color::$mode::SURFACE_CANVAS,
                    false,
                ),
                (
                    p.surface_sidebar,
                    tokens::color::$mode::SURFACE_SIDEBAR,
                    false,
                ),
                (p.surface_card, tokens::color::$mode::SURFACE_CARD, false),
                (
                    p.surface_card_muted,
                    tokens::color::$mode::SURFACE_CARD_MUTED,
                    false,
                ),
                (
                    p.surface_raised,
                    tokens::color::$mode::SURFACE_RAISED,
                    false,
                ),
                (
                    p.surface_composer,
                    tokens::color::$mode::SURFACE_COMPOSER,
                    false,
                ),
                (
                    p.surface_floating,
                    tokens::color::$mode::SURFACE_FLOATING,
                    false,
                ),
                (
                    p.surface_control,
                    tokens::color::$mode::SURFACE_CONTROL,
                    false,
                ),
                (
                    p.surface_terminal,
                    tokens::color::$mode::SURFACE_TERMINAL,
                    false,
                ),
                (p.surface_scrim, tokens::color::$mode::SURFACE_SCRIM, true),
                (
                    p.surface_header_glass,
                    tokens::color::$mode::SURFACE_HEADER_GLASS,
                    true,
                ),
                (
                    p.interaction_hover,
                    tokens::color::$mode::INTERACTION_HOVER,
                    true,
                ),
                (
                    p.interaction_selected,
                    tokens::color::$mode::INTERACTION_SELECTED,
                    true,
                ),
                (
                    p.interaction_pressed,
                    tokens::color::$mode::INTERACTION_PRESSED,
                    true,
                ),
                (
                    p.interaction_selection,
                    tokens::color::$mode::INTERACTION_SELECTION,
                    true,
                ),
                (
                    p.interaction_scrollbar_thumb,
                    tokens::color::$mode::INTERACTION_SCROLLBAR_THUMB,
                    true,
                ),
                (
                    p.interaction_scrollbar_thumb_hover,
                    tokens::color::$mode::INTERACTION_SCROLLBAR_THUMB_HOVER,
                    true,
                ),
                (p.text_primary, tokens::color::$mode::TEXT_PRIMARY, false),
                (
                    p.text_secondary,
                    tokens::color::$mode::TEXT_SECONDARY,
                    false,
                ),
                (p.text_tertiary, tokens::color::$mode::TEXT_TERTIARY, false),
                (p.text_ghost, tokens::color::$mode::TEXT_GHOST, false),
                (p.text_code, tokens::color::$mode::TEXT_CODE, false),
                (
                    p.border_hairline,
                    tokens::color::$mode::BORDER_HAIRLINE,
                    false,
                ),
                (p.border_subtle, tokens::color::$mode::BORDER_SUBTLE, false),
                (p.border_strong, tokens::color::$mode::BORDER_STRONG, false),
                (
                    p.border_focus_ring,
                    tokens::color::$mode::BORDER_FOCUS_RING,
                    false,
                ),
                (
                    p.action_inverse,
                    tokens::color::$mode::ACTION_INVERSE,
                    false,
                ),
                (
                    p.action_inverse_hover,
                    tokens::color::$mode::ACTION_INVERSE_HOVER,
                    false,
                ),
                (
                    p.action_on_inverse,
                    tokens::color::$mode::ACTION_ON_INVERSE,
                    false,
                ),
                (
                    p.action_disabled_bg,
                    tokens::color::$mode::ACTION_DISABLED_BG,
                    false,
                ),
                (
                    p.action_disabled_fg,
                    tokens::color::$mode::ACTION_DISABLED_FG,
                    false,
                ),
                (
                    p.semantic_accent,
                    tokens::color::$mode::SEMANTIC_ACCENT,
                    false,
                ),
                (
                    p.semantic_accent_soft,
                    tokens::color::$mode::SEMANTIC_ACCENT_SOFT,
                    true,
                ),
                (
                    p.semantic_accent_border,
                    tokens::color::$mode::SEMANTIC_ACCENT_BORDER,
                    true,
                ),
                (
                    p.semantic_running,
                    tokens::color::$mode::SEMANTIC_RUNNING,
                    false,
                ),
                (
                    p.semantic_success,
                    tokens::color::$mode::SEMANTIC_SUCCESS,
                    false,
                ),
                (
                    p.semantic_warning,
                    tokens::color::$mode::SEMANTIC_WARNING,
                    false,
                ),
                (
                    p.semantic_danger,
                    tokens::color::$mode::SEMANTIC_DANGER,
                    false,
                ),
                (
                    p.semantic_handoff,
                    tokens::color::$mode::SEMANTIC_HANDOFF,
                    false,
                ),
                (
                    p.semantic_code_wash,
                    tokens::color::$mode::SEMANTIC_CODE_WASH,
                    true,
                ),
            ]
        }};
    }

    #[test]
    fn palette_dark_matches_tokens() {
        let p = Palette::dark();
        assert!(p.is_dark);
        for (field, token, has_alpha) in palette_cases!(dark) {
            let expected = if has_alpha {
                translucent_hsla(token)
            } else {
                solid_hsla(token)
            };
            assert_eq!(field, expected, "字段与令牌 0x{token:08x} 不匹配");
        }
    }

    #[test]
    fn palette_light_matches_tokens() {
        let p = Palette::light();
        assert!(!p.is_dark);
        for (field, token, has_alpha) in palette_cases!(light) {
            let expected = if has_alpha {
                translucent_hsla(token)
            } else {
                solid_hsla(token)
            };
            assert_eq!(field, expected, "字段与令牌 0x{token:08x} 不匹配");
        }
    }

    #[test]
    fn accent_presets_match_tokens() {
        for preset in AccentPreset::ALL {
            let expected_dark = match preset {
                AccentPreset::Emerald => tokens::accent_presets::emerald::DARK,
                AccentPreset::Ocean => tokens::accent_presets::ocean::DARK,
                AccentPreset::Violet => tokens::accent_presets::violet::DARK,
                AccentPreset::Amber => tokens::accent_presets::amber::DARK,
                AccentPreset::Rose => tokens::accent_presets::rose::DARK,
                AccentPreset::Cyan => tokens::accent_presets::cyan::DARK,
            };
            let expected_light = match preset {
                AccentPreset::Emerald => tokens::accent_presets::emerald::LIGHT,
                AccentPreset::Ocean => tokens::accent_presets::ocean::LIGHT,
                AccentPreset::Violet => tokens::accent_presets::violet::LIGHT,
                AccentPreset::Amber => tokens::accent_presets::amber::LIGHT,
                AccentPreset::Rose => tokens::accent_presets::rose::LIGHT,
                AccentPreset::Cyan => tokens::accent_presets::cyan::LIGHT,
            };
            assert_eq!(
                preset.dark(),
                solid_hsla(expected_dark),
                "{preset:?} 深色预设不符"
            );
            assert_eq!(
                preset.light(),
                solid_hsla(expected_light),
                "{preset:?} 浅色预设不符"
            );
            // 预设色为不透明色。
            assert_approx(preset.dark().a, 1.0);
            assert_approx(preset.light().a, 1.0);
        }
    }
}
