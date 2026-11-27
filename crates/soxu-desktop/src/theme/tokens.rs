//! 由 scripts/sync-design-tokens.mjs 生成，禁止手改。
//!
//! 唯一真相源：`design-system/soxu/tokens.json`。本文件与 `design-system/soxu/tokens.css`、
//! `apps/mobile-web/src/tokens.generated.css` 同批编译，并由同一 `--check` 做逐字节零漂移校验。
//!
//! 重新生成：`node scripts/sync-design-tokens.mjs --write`
//! 漂移校验：`node scripts/sync-design-tokens.mjs --check`
//!
//! 命名规则：令牌键 → `SCREAMING_SNAKE_CASE`；尺寸加 `_PX`（`f32`），动效时长加 `_MS`
//! （`u64`），颜色为 `0xRRGGBB`（含 alpha 时 `0xRRGGBBAA`，`u32`）。

/// 令牌来源的文档元信息。
pub mod meta {
    /// 源令牌 `meta.name`。
    pub const NAME: &str = "Soxu Pure Black Design System";
    /// 源令牌 `meta.version`。
    pub const VERSION: &str = "1.0.0";
    /// 源令牌 `meta.defaultMode`。
    pub const DEFAULT_MODE: &str = "dark";
}

/// 排版令牌：字体栈、基准字号、字重开关与字号阶梯。
///
/// 字号一律以 CSS px 表达（`_PX`），GPUI 侧按需换算为 rems。
pub mod typography {
    /// 源令牌 `typography.family.sans`（字体栈）。
    pub const FONT_SANS: &str = "'Inter', 'Noto Sans SC', 'HarmonyOS Sans SC', 'PingFang SC', 'Microsoft YaHei UI', system-ui, sans-serif";
    /// 源令牌 `typography.family.mono`（字体栈）。
    pub const FONT_MONO: &str =
        "'JetBrains Mono', 'Cascadia Mono', 'Fira Code', 'Noto Sans SC', monospace";
    /// 源令牌 `typography.defaults.uiFontSizePx`。
    pub const FONT_UI_SIZE_PX: f32 = 14.0;
    /// 源令牌 `typography.defaults.codeFontSizePx`。
    pub const FONT_CODE_SIZE_PX: f32 = 13.0;
    /// 源令牌 `typography.defaults.codeLigatures`。
    pub const CODE_LIGATURES: bool = true;
    /// 源令牌 `typography.scale.desktopBasePx`。
    pub const TEXT_DESKTOP_BASE_PX: f32 = 14.0;
    /// 源令牌 `typography.scale.mobileBasePx`。
    pub const TEXT_MOBILE_BASE_PX: f32 = 15.0;
    /// 源令牌 `typography.scale.micro`。
    pub const TEXT_MICRO_PX: f32 = 11.0;
    /// 源令牌 `typography.scale.caption`。
    pub const TEXT_CAPTION_PX: f32 = 12.0;
    /// 源令牌 `typography.scale.code`。
    pub const TEXT_CODE_PX: f32 = 13.0;
    /// 源令牌 `typography.scale.body`。
    pub const TEXT_BODY_PX: f32 = 14.0;
    /// 源令牌 `typography.scale.bodyMobile`。
    pub const TEXT_BODY_MOBILE_PX: f32 = 15.0;
    /// 源令牌 `typography.scale.inputMobile`。
    pub const TEXT_INPUT_MOBILE_PX: f32 = 16.0;
    /// 源令牌 `typography.scale.title`。
    pub const TEXT_TITLE_PX: f32 = 17.0;
    /// 源令牌 `typography.scale.display`。
    pub const TEXT_DISPLAY_PX: f32 = 22.0;
}

/// 双模式颜色令牌。
///
/// 不透明令牌为 `0xRRGGBB`；带透明度的令牌为 `0xRRGGBBAA`（同一 `u32`，
/// 逐条在文档注释中标注），避免把 alpha 丢掉或另建第二份色板。
pub mod color {
    /// 深色模式（默认）颜色令牌。
    pub mod dark {
        /// 源令牌 `color.dark.surface.inset`。
        pub const SURFACE_INSET: u32 = 0x000000;
        /// 源令牌 `color.dark.surface.canvas`。
        pub const SURFACE_CANVAS: u32 = 0x000000;
        /// 源令牌 `color.dark.surface.sidebar`。
        pub const SURFACE_SIDEBAR: u32 = 0x0d0d0d;
        /// 源令牌 `color.dark.surface.card`。
        pub const SURFACE_CARD: u32 = 0x171717;
        /// 源令牌 `color.dark.surface.cardMuted`。
        pub const SURFACE_CARD_MUTED: u32 = 0x121212;
        /// 源令牌 `color.dark.surface.raised`。
        pub const SURFACE_RAISED: u32 = 0x212121;
        /// 源令牌 `color.dark.surface.composer`。
        pub const SURFACE_COMPOSER: u32 = 0x212121;
        /// 源令牌 `color.dark.surface.floating`。
        pub const SURFACE_FLOATING: u32 = 0x262626;
        /// 源令牌 `color.dark.surface.control`。
        pub const SURFACE_CONTROL: u32 = 0x2f2f2f;
        /// 源令牌 `color.dark.surface.terminal`。
        pub const SURFACE_TERMINAL: u32 = 0x050505;
        /// 源令牌 `color.dark.surface.scrim`（含 alpha，格式 `0xRRGGBBAA`）。
        pub const SURFACE_SCRIM: u32 = 0x000000c2;
        /// 源令牌 `color.dark.surface.headerGlass`（含 alpha，格式 `0xRRGGBBAA`）。
        pub const SURFACE_HEADER_GLASS: u32 = 0x000000d1;
        /// 源令牌 `color.dark.interaction.hover`（含 alpha，格式 `0xRRGGBBAA`）。
        pub const INTERACTION_HOVER: u32 = 0xffffff0d;
        /// 源令牌 `color.dark.interaction.selected`（含 alpha，格式 `0xRRGGBBAA`）。
        pub const INTERACTION_SELECTED: u32 = 0xffffff17;
        /// 源令牌 `color.dark.interaction.pressed`（含 alpha，格式 `0xRRGGBBAA`）。
        pub const INTERACTION_PRESSED: u32 = 0xffffff21;
        /// 源令牌 `color.dark.interaction.selection`（含 alpha，格式 `0xRRGGBBAA`）。
        pub const INTERACTION_SELECTION: u32 = 0x19c37d4d;
        /// 源令牌 `color.dark.interaction.scrollbarThumb`（含 alpha，格式 `0xRRGGBBAA`）。
        pub const INTERACTION_SCROLLBAR_THUMB: u32 = 0xffffff29;
        /// 源令牌 `color.dark.interaction.scrollbarThumbHover`（含 alpha，格式 `0xRRGGBBAA`）。
        pub const INTERACTION_SCROLLBAR_THUMB_HOVER: u32 = 0xffffff4d;
        /// 源令牌 `color.dark.text.primary`。
        pub const TEXT_PRIMARY: u32 = 0xececec;
        /// 源令牌 `color.dark.text.secondary`。
        pub const TEXT_SECONDARY: u32 = 0xb4b4b4;
        /// 源令牌 `color.dark.text.tertiary`。
        pub const TEXT_TERTIARY: u32 = 0x8e8e8e;
        /// 源令牌 `color.dark.text.ghost`。
        pub const TEXT_GHOST: u32 = 0x5e5e5e;
        /// 源令牌 `color.dark.text.code`。
        pub const TEXT_CODE: u32 = 0xf4f4f5;
        /// 源令牌 `color.dark.border.hairline`。
        pub const BORDER_HAIRLINE: u32 = 0x1a1a1a;
        /// 源令牌 `color.dark.border.subtle`。
        pub const BORDER_SUBTLE: u32 = 0x262626;
        /// 源令牌 `color.dark.border.strong`。
        pub const BORDER_STRONG: u32 = 0x383838;
        /// 源令牌 `color.dark.border.focusRing`。
        pub const BORDER_FOCUS_RING: u32 = 0xffffff;
        /// 源令牌 `color.dark.action.inverse`。
        pub const ACTION_INVERSE: u32 = 0xffffff;
        /// 源令牌 `color.dark.action.inverseHover`。
        pub const ACTION_INVERSE_HOVER: u32 = 0xe5e5e5;
        /// 源令牌 `color.dark.action.onInverse`。
        pub const ACTION_ON_INVERSE: u32 = 0x000000;
        /// 源令牌 `color.dark.action.disabledBg`。
        pub const ACTION_DISABLED_BG: u32 = 0x2a2a2a;
        /// 源令牌 `color.dark.action.disabledFg`。
        pub const ACTION_DISABLED_FG: u32 = 0x666666;
        /// 源令牌 `color.dark.semantic.accent`。
        pub const SEMANTIC_ACCENT: u32 = 0x19c37d;
        /// 源令牌 `color.dark.semantic.accentSoft`（含 alpha，格式 `0xRRGGBBAA`）。
        pub const SEMANTIC_ACCENT_SOFT: u32 = 0x19c37d26;
        /// 源令牌 `color.dark.semantic.accentBorder`（含 alpha，格式 `0xRRGGBBAA`）。
        pub const SEMANTIC_ACCENT_BORDER: u32 = 0x19c37d61;
        /// 源令牌 `color.dark.semantic.running`。
        pub const SEMANTIC_RUNNING: u32 = 0x3b82f6;
        /// 源令牌 `color.dark.semantic.runningSoft`（含 alpha，格式 `0xRRGGBBAA`）。
        pub const SEMANTIC_RUNNING_SOFT: u32 = 0x3b82f624;
        /// 源令牌 `color.dark.semantic.runningBorder`（含 alpha，格式 `0xRRGGBBAA`）。
        pub const SEMANTIC_RUNNING_BORDER: u32 = 0x3b82f65c;
        /// 源令牌 `color.dark.semantic.success`。
        pub const SEMANTIC_SUCCESS: u32 = 0x22c55e;
        /// 源令牌 `color.dark.semantic.successSoft`（含 alpha，格式 `0xRRGGBBAA`）。
        pub const SEMANTIC_SUCCESS_SOFT: u32 = 0x22c55e21;
        /// 源令牌 `color.dark.semantic.warning`。
        pub const SEMANTIC_WARNING: u32 = 0xf59e0b;
        /// 源令牌 `color.dark.semantic.warningSoft`（含 alpha，格式 `0xRRGGBBAA`）。
        pub const SEMANTIC_WARNING_SOFT: u32 = 0xf59e0b24;
        /// 源令牌 `color.dark.semantic.warningBorder`（含 alpha，格式 `0xRRGGBBAA`）。
        pub const SEMANTIC_WARNING_BORDER: u32 = 0xf59e0b61;
        /// 源令牌 `color.dark.semantic.danger`。
        pub const SEMANTIC_DANGER: u32 = 0xef4444;
        /// 源令牌 `color.dark.semantic.dangerSoft`（含 alpha，格式 `0xRRGGBBAA`）。
        pub const SEMANTIC_DANGER_SOFT: u32 = 0xef444424;
        /// 源令牌 `color.dark.semantic.dangerBorder`（含 alpha，格式 `0xRRGGBBAA`）。
        pub const SEMANTIC_DANGER_BORDER: u32 = 0xef444466;
        /// 源令牌 `color.dark.semantic.handoff`。
        pub const SEMANTIC_HANDOFF: u32 = 0xa855f7;
        /// 源令牌 `color.dark.semantic.handoffSoft`（含 alpha，格式 `0xRRGGBBAA`）。
        pub const SEMANTIC_HANDOFF_SOFT: u32 = 0xa855f724;
        /// 源令牌 `color.dark.semantic.codeWash`（含 alpha，格式 `0xRRGGBBAA`）。
        pub const SEMANTIC_CODE_WASH: u32 = 0xffffff14;
        /// 源令牌 `color.dark.syntax.plain`。
        pub const SYNTAX_PLAIN: u32 = 0xececec;
        /// 源令牌 `color.dark.syntax.keyword`。
        pub const SYNTAX_KEYWORD: u32 = 0xff7b72;
        /// 源令牌 `color.dark.syntax.function`。
        pub const SYNTAX_FUNCTION: u32 = 0xd2a8ff;
        /// 源令牌 `color.dark.syntax.type`。
        pub const SYNTAX_TYPE: u32 = 0xffa657;
        /// 源令牌 `color.dark.syntax.string`。
        pub const SYNTAX_STRING: u32 = 0xa5d6ff;
        /// 源令牌 `color.dark.syntax.number`。
        pub const SYNTAX_NUMBER: u32 = 0x79c0ff;
        /// 源令牌 `color.dark.syntax.comment`。
        pub const SYNTAX_COMMENT: u32 = 0x8b949e;
        /// 源令牌 `color.dark.syntax.operator`。
        pub const SYNTAX_OPERATOR: u32 = 0xff7b72;
        /// 源令牌 `color.dark.syntax.property`。
        pub const SYNTAX_PROPERTY: u32 = 0x7ee787;
        /// 源令牌 `color.dark.syntax.macro`。
        pub const SYNTAX_MACRO: u32 = 0x56d4dd;
        /// 源令牌 `color.dark.syntax.diffAddBg`（含 alpha，格式 `0xRRGGBBAA`）。
        pub const SYNTAX_DIFF_ADD_BG: u32 = 0x22c55e24;
        /// 源令牌 `color.dark.syntax.diffAddGutter`（含 alpha，格式 `0xRRGGBBAA`）。
        pub const SYNTAX_DIFF_ADD_GUTTER: u32 = 0x22c55e42;
        /// 源令牌 `color.dark.syntax.diffDelBg`（含 alpha，格式 `0xRRGGBBAA`）。
        pub const SYNTAX_DIFF_DEL_BG: u32 = 0xef444424;
        /// 源令牌 `color.dark.syntax.diffDelGutter`（含 alpha，格式 `0xRRGGBBAA`）。
        pub const SYNTAX_DIFF_DEL_GUTTER: u32 = 0xef444442;
        /// 源令牌 `color.dark.ansi.black`。
        pub const ANSI_BLACK: u32 = 0x262626;
        /// 源令牌 `color.dark.ansi.red`。
        pub const ANSI_RED: u32 = 0xf87171;
        /// 源令牌 `color.dark.ansi.green`。
        pub const ANSI_GREEN: u32 = 0x4ade80;
        /// 源令牌 `color.dark.ansi.yellow`。
        pub const ANSI_YELLOW: u32 = 0xfacc15;
        /// 源令牌 `color.dark.ansi.blue`。
        pub const ANSI_BLUE: u32 = 0x60a5fa;
        /// 源令牌 `color.dark.ansi.magenta`。
        pub const ANSI_MAGENTA: u32 = 0xc084fc;
        /// 源令牌 `color.dark.ansi.cyan`。
        pub const ANSI_CYAN: u32 = 0x22d3ee;
        /// 源令牌 `color.dark.ansi.white`。
        pub const ANSI_WHITE: u32 = 0xe5e5e5;
        /// 源令牌 `color.dark.ansi.brightBlack`。
        pub const ANSI_BRIGHT_BLACK: u32 = 0x737373;
        /// 源令牌 `color.dark.ansi.brightRed`。
        pub const ANSI_BRIGHT_RED: u32 = 0xfca5a5;
        /// 源令牌 `color.dark.ansi.brightGreen`。
        pub const ANSI_BRIGHT_GREEN: u32 = 0x86efac;
        /// 源令牌 `color.dark.ansi.brightYellow`。
        pub const ANSI_BRIGHT_YELLOW: u32 = 0xfde047;
        /// 源令牌 `color.dark.ansi.brightBlue`。
        pub const ANSI_BRIGHT_BLUE: u32 = 0x93c5fd;
        /// 源令牌 `color.dark.ansi.brightMagenta`。
        pub const ANSI_BRIGHT_MAGENTA: u32 = 0xd8b4fe;
        /// 源令牌 `color.dark.ansi.brightCyan`。
        pub const ANSI_BRIGHT_CYAN: u32 = 0x67e8f9;
        /// 源令牌 `color.dark.ansi.brightWhite`。
        pub const ANSI_BRIGHT_WHITE: u32 = 0xffffff;
    }

    /// 浅色模式颜色令牌。
    pub mod light {
        /// 源令牌 `color.light.surface.inset`。
        pub const SURFACE_INSET: u32 = 0xebebeb;
        /// 源令牌 `color.light.surface.canvas`。
        pub const SURFACE_CANVAS: u32 = 0xffffff;
        /// 源令牌 `color.light.surface.sidebar`。
        pub const SURFACE_SIDEBAR: u32 = 0xf9f9f9;
        /// 源令牌 `color.light.surface.card`。
        pub const SURFACE_CARD: u32 = 0xffffff;
        /// 源令牌 `color.light.surface.cardMuted`。
        pub const SURFACE_CARD_MUTED: u32 = 0xf4f4f4;
        /// 源令牌 `color.light.surface.raised`。
        pub const SURFACE_RAISED: u32 = 0xf4f4f4;
        /// 源令牌 `color.light.surface.composer`。
        pub const SURFACE_COMPOSER: u32 = 0xf4f4f4;
        /// 源令牌 `color.light.surface.floating`。
        pub const SURFACE_FLOATING: u32 = 0xffffff;
        /// 源令牌 `color.light.surface.control`。
        pub const SURFACE_CONTROL: u32 = 0xe8e8e8;
        /// 源令牌 `color.light.surface.terminal`。
        pub const SURFACE_TERMINAL: u32 = 0xf7f7f8;
        /// 源令牌 `color.light.surface.scrim`（含 alpha，格式 `0xRRGGBBAA`）。
        pub const SURFACE_SCRIM: u32 = 0x00000073;
        /// 源令牌 `color.light.surface.headerGlass`（含 alpha，格式 `0xRRGGBBAA`）。
        pub const SURFACE_HEADER_GLASS: u32 = 0xffffffdb;
        /// 源令牌 `color.light.interaction.hover`（含 alpha，格式 `0xRRGGBBAA`）。
        pub const INTERACTION_HOVER: u32 = 0x0000000a;
        /// 源令牌 `color.light.interaction.selected`（含 alpha，格式 `0xRRGGBBAA`）。
        pub const INTERACTION_SELECTED: u32 = 0x00000012;
        /// 源令牌 `color.light.interaction.pressed`（含 alpha，格式 `0xRRGGBBAA`）。
        pub const INTERACTION_PRESSED: u32 = 0x0000001c;
        /// 源令牌 `color.light.interaction.selection`（含 alpha，格式 `0xRRGGBBAA`）。
        pub const INTERACTION_SELECTION: u32 = 0x10a37f38;
        /// 源令牌 `color.light.interaction.scrollbarThumb`（含 alpha，格式 `0xRRGGBBAA`）。
        pub const INTERACTION_SCROLLBAR_THUMB: u32 = 0x0000002e;
        /// 源令牌 `color.light.interaction.scrollbarThumbHover`（含 alpha，格式 `0xRRGGBBAA`）。
        pub const INTERACTION_SCROLLBAR_THUMB_HOVER: u32 = 0x00000052;
        /// 源令牌 `color.light.text.primary`。
        pub const TEXT_PRIMARY: u32 = 0x0d0d0d;
        /// 源令牌 `color.light.text.secondary`。
        pub const TEXT_SECONDARY: u32 = 0x5d5d5d;
        /// 源令牌 `color.light.text.tertiary`。
        pub const TEXT_TERTIARY: u32 = 0x757575;
        /// 源令牌 `color.light.text.ghost`。
        pub const TEXT_GHOST: u32 = 0x9b9b9b;
        /// 源令牌 `color.light.text.code`。
        pub const TEXT_CODE: u32 = 0x18181b;
        /// 源令牌 `color.light.border.hairline`。
        pub const BORDER_HAIRLINE: u32 = 0xededed;
        /// 源令牌 `color.light.border.subtle`。
        pub const BORDER_SUBTLE: u32 = 0xe3e3e3;
        /// 源令牌 `color.light.border.strong`。
        pub const BORDER_STRONG: u32 = 0xc7c7c7;
        /// 源令牌 `color.light.border.focusRing`。
        pub const BORDER_FOCUS_RING: u32 = 0x0d0d0d;
        /// 源令牌 `color.light.action.inverse`。
        pub const ACTION_INVERSE: u32 = 0x0d0d0d;
        /// 源令牌 `color.light.action.inverseHover`。
        pub const ACTION_INVERSE_HOVER: u32 = 0x262626;
        /// 源令牌 `color.light.action.onInverse`。
        pub const ACTION_ON_INVERSE: u32 = 0xffffff;
        /// 源令牌 `color.light.action.disabledBg`。
        pub const ACTION_DISABLED_BG: u32 = 0xe5e5e5;
        /// 源令牌 `color.light.action.disabledFg`。
        pub const ACTION_DISABLED_FG: u32 = 0x9e9e9e;
        /// 源令牌 `color.light.semantic.accent`。
        pub const SEMANTIC_ACCENT: u32 = 0x10a37f;
        /// 源令牌 `color.light.semantic.accentSoft`（含 alpha，格式 `0xRRGGBBAA`）。
        pub const SEMANTIC_ACCENT_SOFT: u32 = 0x10a37f1f;
        /// 源令牌 `color.light.semantic.accentBorder`（含 alpha，格式 `0xRRGGBBAA`）。
        pub const SEMANTIC_ACCENT_BORDER: u32 = 0x10a37f57;
        /// 源令牌 `color.light.semantic.running`。
        pub const SEMANTIC_RUNNING: u32 = 0x2563eb;
        /// 源令牌 `color.light.semantic.runningSoft`（含 alpha，格式 `0xRRGGBBAA`）。
        pub const SEMANTIC_RUNNING_SOFT: u32 = 0x2563eb1a;
        /// 源令牌 `color.light.semantic.runningBorder`（含 alpha，格式 `0xRRGGBBAA`）。
        pub const SEMANTIC_RUNNING_BORDER: u32 = 0x2563eb4d;
        /// 源令牌 `color.light.semantic.success`。
        pub const SEMANTIC_SUCCESS: u32 = 0x16a34a;
        /// 源令牌 `color.light.semantic.successSoft`（含 alpha，格式 `0xRRGGBBAA`）。
        pub const SEMANTIC_SUCCESS_SOFT: u32 = 0x16a34a1c;
        /// 源令牌 `color.light.semantic.warning`。
        pub const SEMANTIC_WARNING: u32 = 0xd97706;
        /// 源令牌 `color.light.semantic.warningSoft`（含 alpha，格式 `0xRRGGBBAA`）。
        pub const SEMANTIC_WARNING_SOFT: u32 = 0xd977061c;
        /// 源令牌 `color.light.semantic.warningBorder`（含 alpha，格式 `0xRRGGBBAA`）。
        pub const SEMANTIC_WARNING_BORDER: u32 = 0xd9770652;
        /// 源令牌 `color.light.semantic.danger`。
        pub const SEMANTIC_DANGER: u32 = 0xdc2626;
        /// 源令牌 `color.light.semantic.dangerSoft`（含 alpha，格式 `0xRRGGBBAA`）。
        pub const SEMANTIC_DANGER_SOFT: u32 = 0xdc26261a;
        /// 源令牌 `color.light.semantic.dangerBorder`（含 alpha，格式 `0xRRGGBBAA`）。
        pub const SEMANTIC_DANGER_BORDER: u32 = 0xdc262652;
        /// 源令牌 `color.light.semantic.handoff`。
        pub const SEMANTIC_HANDOFF: u32 = 0x7c3aed;
        /// 源令牌 `color.light.semantic.handoffSoft`（含 alpha，格式 `0xRRGGBBAA`）。
        pub const SEMANTIC_HANDOFF_SOFT: u32 = 0x7c3aed1a;
        /// 源令牌 `color.light.semantic.codeWash`（含 alpha，格式 `0xRRGGBBAA`）。
        pub const SEMANTIC_CODE_WASH: u32 = 0x0000000f;
        /// 源令牌 `color.light.syntax.plain`。
        pub const SYNTAX_PLAIN: u32 = 0x1f2328;
        /// 源令牌 `color.light.syntax.keyword`。
        pub const SYNTAX_KEYWORD: u32 = 0xcf222e;
        /// 源令牌 `color.light.syntax.function`。
        pub const SYNTAX_FUNCTION: u32 = 0x8250df;
        /// 源令牌 `color.light.syntax.type`。
        pub const SYNTAX_TYPE: u32 = 0x953800;
        /// 源令牌 `color.light.syntax.string`。
        pub const SYNTAX_STRING: u32 = 0x0a3069;
        /// 源令牌 `color.light.syntax.number`。
        pub const SYNTAX_NUMBER: u32 = 0x0550ae;
        /// 源令牌 `color.light.syntax.comment`。
        pub const SYNTAX_COMMENT: u32 = 0x6e7781;
        /// 源令牌 `color.light.syntax.operator`。
        pub const SYNTAX_OPERATOR: u32 = 0xcf222e;
        /// 源令牌 `color.light.syntax.property`。
        pub const SYNTAX_PROPERTY: u32 = 0x116329;
        /// 源令牌 `color.light.syntax.macro`。
        pub const SYNTAX_MACRO: u32 = 0x0969da;
        /// 源令牌 `color.light.syntax.diffAddBg`（含 alpha，格式 `0xRRGGBBAA`）。
        pub const SYNTAX_DIFF_ADD_BG: u32 = 0x16a34a1f;
        /// 源令牌 `color.light.syntax.diffAddGutter`（含 alpha，格式 `0xRRGGBBAA`）。
        pub const SYNTAX_DIFF_ADD_GUTTER: u32 = 0x16a34a3d;
        /// 源令牌 `color.light.syntax.diffDelBg`（含 alpha，格式 `0xRRGGBBAA`）。
        pub const SYNTAX_DIFF_DEL_BG: u32 = 0xdc26261c;
        /// 源令牌 `color.light.syntax.diffDelGutter`（含 alpha，格式 `0xRRGGBBAA`）。
        pub const SYNTAX_DIFF_DEL_GUTTER: u32 = 0xdc262638;
        /// 源令牌 `color.light.ansi.black`。
        pub const ANSI_BLACK: u32 = 0x24292f;
        /// 源令牌 `color.light.ansi.red`。
        pub const ANSI_RED: u32 = 0xcf222e;
        /// 源令牌 `color.light.ansi.green`。
        pub const ANSI_GREEN: u32 = 0x116329;
        /// 源令牌 `color.light.ansi.yellow`。
        pub const ANSI_YELLOW: u32 = 0x9a6700;
        /// 源令牌 `color.light.ansi.blue`。
        pub const ANSI_BLUE: u32 = 0x0969da;
        /// 源令牌 `color.light.ansi.magenta`。
        pub const ANSI_MAGENTA: u32 = 0x8250df;
        /// 源令牌 `color.light.ansi.cyan`。
        pub const ANSI_CYAN: u32 = 0x0e7490;
        /// 源令牌 `color.light.ansi.white`。
        pub const ANSI_WHITE: u32 = 0x57606a;
        /// 源令牌 `color.light.ansi.brightBlack`。
        pub const ANSI_BRIGHT_BLACK: u32 = 0x6e7781;
        /// 源令牌 `color.light.ansi.brightRed`。
        pub const ANSI_BRIGHT_RED: u32 = 0xa40e26;
        /// 源令牌 `color.light.ansi.brightGreen`。
        pub const ANSI_BRIGHT_GREEN: u32 = 0x1a7f37;
        /// 源令牌 `color.light.ansi.brightYellow`。
        pub const ANSI_BRIGHT_YELLOW: u32 = 0x7d4e00;
        /// 源令牌 `color.light.ansi.brightBlue`。
        pub const ANSI_BRIGHT_BLUE: u32 = 0x0550ae;
        /// 源令牌 `color.light.ansi.brightMagenta`。
        pub const ANSI_BRIGHT_MAGENTA: u32 = 0x6639ba;
        /// 源令牌 `color.light.ansi.brightCyan`。
        pub const ANSI_BRIGHT_CYAN: u32 = 0x155e75;
        /// 源令牌 `color.light.ansi.brightWhite`。
        pub const ANSI_BRIGHT_WHITE: u32 = 0x0d0d0d;
    }
}

/// 间距刻度（4px 基准）。
pub mod spacing {
    /// 源令牌 `spacing.1`。
    pub const SPACE_1_PX: f32 = 4.0;
    /// 源令牌 `spacing.2`。
    pub const SPACE_2_PX: f32 = 8.0;
    /// 源令牌 `spacing.3`。
    pub const SPACE_3_PX: f32 = 12.0;
    /// 源令牌 `spacing.4`。
    pub const SPACE_4_PX: f32 = 16.0;
    /// 源令牌 `spacing.5`。
    pub const SPACE_5_PX: f32 = 20.0;
    /// 源令牌 `spacing.6`。
    pub const SPACE_6_PX: f32 = 24.0;
    /// 源令牌 `spacing.8`。
    pub const SPACE_8_PX: f32 = 32.0;
    /// 源令牌 `spacing.12`。
    pub const SPACE_12_PX: f32 = 48.0;
    /// 源令牌 `spacing.half`。
    pub const SPACE_HALF_PX: f32 = 2.0;
    /// 源令牌 `spacing.1_5`。
    pub const SPACE_1_5_PX: f32 = 6.0;
}

/// 圆角刻度。
pub mod radius {
    /// 源令牌 `radius.xs`。
    pub const RADIUS_XS_PX: f32 = 6.0;
    /// 源令牌 `radius.sm`。
    pub const RADIUS_SM_PX: f32 = 8.0;
    /// 源令牌 `radius.md`。
    pub const RADIUS_MD_PX: f32 = 12.0;
    /// 源令牌 `radius.lg`。
    pub const RADIUS_LG_PX: f32 = 18.0;
    /// 源令牌 `radius.composer`。
    pub const RADIUS_COMPOSER_PX: f32 = 22.0;
    /// 源令牌 `radius.pill`。
    pub const RADIUS_PILL_PX: f32 = 9999.0;
}

/// 布局栏宽、内容宽度、触控目标与响应式断点（含 400 / 500 / 640 / 1024）。
pub mod layout {
    /// 源令牌 `layout.sidebarDefault`。
    pub const SIDEBAR_DEFAULT_PX: f32 = 260.0;
    /// 源令牌 `layout.sidebarMin`。
    pub const SIDEBAR_MIN_PX: f32 = 224.0;
    /// 源令牌 `layout.sidebarMax`。
    pub const SIDEBAR_MAX_PX: f32 = 420.0;
    /// 源令牌 `layout.inspectorDefault`。
    pub const INSPECTOR_DEFAULT_PX: f32 = 380.0;
    /// 源令牌 `layout.inspectorMin`。
    pub const INSPECTOR_MIN_PX: f32 = 280.0;
    /// 源令牌 `layout.inspectorMax`。
    pub const INSPECTOR_MAX_PX: f32 = 720.0;
    /// 源令牌 `layout.transcriptMaxWidth`。
    pub const TRANSCRIPT_MAX_WIDTH_PX: f32 = 760.0;
    /// 源令牌 `layout.composerMaxWidth`。
    pub const COMPOSER_MAX_WIDTH_PX: f32 = 760.0;
    /// 源令牌 `layout.maxContentWidth`。
    pub const MAX_CONTENT_WIDTH_PX: f32 = 800.0;
    /// 源令牌 `layout.touchTargetMin`。
    pub const TOUCH_TARGET_MIN_PX: f32 = 44.0;
    /// 源令牌 `layout.touchTargetAndroid`。
    pub const TOUCH_TARGET_ANDROID_PX: f32 = 48.0;
    /// 源令牌 `layout.breakpointUltraCompact`。
    pub const BREAKPOINT_ULTRA_COMPACT_PX: f32 = 400.0;
    /// 源令牌 `layout.breakpointRegular`。
    pub const BREAKPOINT_REGULAR_PX: f32 = 640.0;
    /// 源令牌 `layout.breakpointWide`。
    pub const BREAKPOINT_WIDE_PX: f32 = 1024.0;
    /// 源令牌 `layout.shortViewportMax`。
    pub const SHORT_VIEWPORT_MAX_PX: f32 = 500.0;
    /// 源令牌 `layout.macosTrafficLightsInset`。
    pub const MACOS_TRAFFIC_LIGHTS_INSET_PX: f32 = 78.0;
}

/// 组件几何尺寸（含 44 / 48 触控热区）。
pub mod sizing {
    /// 源令牌 `sizing.titlebarHeight`。
    pub const TITLEBAR_HEIGHT_PX: f32 = 48.0;
    /// 源令牌 `sizing.tabbarHeight`。
    pub const TABBAR_HEIGHT_PX: f32 = 40.0;
    /// 源令牌 `sizing.subagentBarHeight`。
    pub const SUBAGENT_BAR_HEIGHT_PX: f32 = 32.0;
    /// 源令牌 `sizing.statusbarHeight`。
    pub const STATUSBAR_HEIGHT_PX: f32 = 30.0;
    /// 源令牌 `sizing.sidebarRowHeight`。
    pub const SIDEBAR_ROW_HEIGHT_PX: f32 = 36.0;
    /// 源令牌 `sizing.toolCardHeaderHeight`。
    pub const TOOL_CARD_HEADER_HEIGHT_PX: f32 = 38.0;
    /// 源令牌 `sizing.diffGutterWidth`。
    pub const DIFF_GUTTER_WIDTH_PX: f32 = 52.0;
    /// 源令牌 `sizing.activityDiffMaxHeight`。
    pub const ACTIVITY_DIFF_MAX_HEIGHT_PX: f32 = 400.0;
    /// 源令牌 `sizing.permissionDetailMaxHeight`。
    pub const PERMISSION_DETAIL_MAX_HEIGHT_PX: f32 = 96.0;
    /// 源令牌 `sizing.cmdPaletteWidth`。
    pub const CMD_PALETTE_WIDTH_PX: f32 = 640.0;
    /// 源令牌 `sizing.cmdPaletteMaxHeight`。
    pub const CMD_PALETTE_MAX_HEIGHT_PX: f32 = 440.0;
    /// 源令牌 `sizing.cmdPaletteInputHeight`。
    pub const CMD_PALETTE_INPUT_HEIGHT_PX: f32 = 52.0;
    /// 源令牌 `sizing.turnNavRailWidth`。
    pub const TURN_NAV_RAIL_WIDTH_PX: f32 = 20.0;
    /// 源令牌 `sizing.compactRailWidth`。
    pub const COMPACT_RAIL_WIDTH_PX: f32 = 52.0;
    /// 源令牌 `sizing.dockPillHeight`。
    pub const DOCK_PILL_HEIGHT_PX: f32 = 40.0;
    /// 源令牌 `sizing.sinkSidebarMaxWidth`。
    pub const SINK_SIDEBAR_MAX_WIDTH_PX: f32 = 316.0;
    /// 源令牌 `sizing.touchHitMin`。
    pub const TOUCH_HIT_MIN_PX: f32 = 44.0;
    /// 源令牌 `sizing.touchHitPrimary`。
    pub const TOUCH_HIT_PRIMARY_PX: f32 = 48.0;
    /// 源令牌 `sizing.iconSm`。
    pub const ICON_SM_PX: f32 = 14.0;
    /// 源令牌 `sizing.iconMd`。
    pub const ICON_MD_PX: f32 = 16.0;
    /// 源令牌 `sizing.iconLg`。
    pub const ICON_LG_PX: f32 = 20.0;
}

/// 层叠顺序。
pub mod z_index {
    /// 源令牌 `zIndex.base`。
    pub const BASE: u32 = 0;
    /// 源令牌 `zIndex.sticky`。
    pub const STICKY: u32 = 10;
    /// 源令牌 `zIndex.rail`。
    pub const RAIL: u32 = 15;
    /// 源令牌 `zIndex.header`。
    pub const HEADER: u32 = 20;
    /// 源令牌 `zIndex.popover`。
    pub const POPOVER: u32 = 40;
    /// 源令牌 `zIndex.drawerScrim`。
    pub const DRAWER_SCRIM: u32 = 50;
    /// 源令牌 `zIndex.drawer`。
    pub const DRAWER: u32 = 55;
    /// 源令牌 `zIndex.modalScrim`。
    pub const MODAL_SCRIM: u32 = 60;
    /// 源令牌 `zIndex.modal`。
    pub const MODAL: u32 = 70;
    /// 源令牌 `zIndex.palette`。
    pub const PALETTE: u32 = 80;
    /// 源令牌 `zIndex.floating`。
    pub const FLOATING: u32 = 95;
    /// 源令牌 `zIndex.toast`。
    pub const TOAST: u32 = 100;
}

/// 动效令牌：时长（`u64` 毫秒）、缓动曲线与刷新频率上限。
pub mod motion {
    /// 源令牌 `motion.pressDuration`。
    pub const PRESS_DURATION_MS: u64 = 100;
    /// 源令牌 `motion.hoverDuration`。
    pub const HOVER_DURATION_MS: u64 = 150;
    /// 源令牌 `motion.panelDuration`。
    pub const PANEL_DURATION_MS: u64 = 220;
    /// 源令牌 `motion.sheetDuration`。
    pub const SHEET_DURATION_MS: u64 = 260;
    /// 源令牌 `motion.enterDuration`。
    pub const ENTER_DURATION_MS: u64 = 220;
    /// 源令牌 `motion.exitDuration`。
    pub const EXIT_DURATION_MS: u64 = 150;
    /// 源令牌 `motion.hoverExpandDelay`。
    pub const HOVER_EXPAND_DELAY_MS: u64 = 40;
    /// 源令牌 `motion.hoverCollapseDelay`。
    pub const HOVER_COLLAPSE_DELAY_MS: u64 = 160;
    /// 源令牌 `motion.themeTransitionDuration`。
    pub const THEME_TRANSITION_DURATION_MS: u64 = 340;
    /// 源令牌 `motion.themeIconDuration`。
    pub const THEME_ICON_DURATION_MS: u64 = 280;
    /// 源令牌 `motion.easeOut`。
    pub const EASE_OUT: [f32; 4] = [0.16, 1.0, 0.3, 1.0];
    /// 源令牌 `motion.easeOutExpo`。
    pub const EASE_OUT_EXPO: [f32; 4] = [0.16, 1.0, 0.3, 1.0];
    /// 源令牌 `motion.easeInOut`。
    pub const EASE_IN_OUT: [f32; 4] = [0.4, 0.0, 0.2, 1.0];
    /// 源令牌 `motion.easeInOutCubic`。
    pub const EASE_IN_OUT_CUBIC: [f32; 4] = [0.65, 0.0, 0.35, 1.0];
    /// 源令牌 `motion.easeFast`。
    pub const EASE_FAST: [f32; 4] = [0.2, 0.0, 0.0, 1.0];
    /// 源令牌 `motion.streamCommitMaxHz`。
    pub const STREAM_COMMIT_MAX_HZ: f32 = 8.3;
    /// 源令牌 `motion.pulseClockMaxHz`。
    pub const PULSE_CLOCK_MAX_HZ: f32 = 30.0;
}

/// 强调色预设（深/浅双模式各一枚不透明色）。
pub mod accent_presets {
    /// 预设 `accentPresets.emerald`：翠玉绿 (默认)。
    pub mod emerald {
        /// 源令牌 `accentPresets.emerald.dark`。
        pub const DARK: u32 = 0x19c37d;
        /// 源令牌 `accentPresets.emerald.light`。
        pub const LIGHT: u32 = 0x10a37f;
    }

    /// 预设 `accentPresets.ocean`：极海蓝。
    pub mod ocean {
        /// 源令牌 `accentPresets.ocean.dark`。
        pub const DARK: u32 = 0x3b82f6;
        /// 源令牌 `accentPresets.ocean.light`。
        pub const LIGHT: u32 = 0x2563eb;
    }

    /// 预设 `accentPresets.violet`：星云紫。
    pub mod violet {
        /// 源令牌 `accentPresets.violet.dark`。
        pub const DARK: u32 = 0x8b5cf6;
        /// 源令牌 `accentPresets.violet.light`。
        pub const LIGHT: u32 = 0x7c3aed;
    }

    /// 预设 `accentPresets.amber`：琥珀橙。
    pub mod amber {
        /// 源令牌 `accentPresets.amber.dark`。
        pub const DARK: u32 = 0xf59e0b;
        /// 源令牌 `accentPresets.amber.light`。
        pub const LIGHT: u32 = 0xd97706;
    }

    /// 预设 `accentPresets.rose`：绯霞红。
    pub mod rose {
        /// 源令牌 `accentPresets.rose.dark`。
        pub const DARK: u32 = 0xf43f5e;
        /// 源令牌 `accentPresets.rose.light`。
        pub const LIGHT: u32 = 0xe11d48;
    }

    /// 预设 `accentPresets.cyan`：晴空青。
    pub mod cyan {
        /// 源令牌 `accentPresets.cyan.dark`。
        pub const DARK: u32 = 0x06b6d4;
        /// 源令牌 `accentPresets.cyan.light`。
        pub const LIGHT: u32 = 0x0891b2;
    }
}
