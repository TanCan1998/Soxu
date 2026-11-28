//! 7 分区独立设置弹窗（`pages/07-settings-fonts-and-motion.md` §二 + §七）。
//!
//! 规范要点：
//! - **几何**：桌面端 `820px × 580px` 居中对话框，左侧 `200px` 分区导航轨，
//!   右侧平滑滚动内容区；宽 `< 640px` 自动降级为全屏双段滚动抽屉，行高 `≥ 44px`；
//! - **7 大分区**：外观与主题 / 字体与排版 / 代理与路由器 / 工作区与 Git /
//!   安全与审批红线 / 移动遥控与配对 / 快捷键与关于，顺序与 Lucide 图标严格同源；
//! - **焦点捕获与还原**：打开即进入 [`FocusLayer::ModalDialog`]，激活期间任何低层
//!   入栈被拒绝；`Esc` 关闭并把焦点还原到打开前所在层；
//! - **各分区表单为纯状态占位**：值取自 `soxu-protocol::SoxuPreferences`（双端同构
//!   Schema），令牌只读取，不新增任何色值或尺寸常量。
//!
//! 阶段边界（Issue #133）：不读写 `preferences.json`、不触发任何 I/O；
//! 持久化在 M6 由 daemon 承担。

use gpui::{Div, ParentElement, Styled, div, px};
use soxu_protocol::{
    AccentPresetId, CodeFontPresetId, DensityMode, LineHeightMode, SoxuPreferences,
    TimestampFormatMode, UiFontPresetId, WorktreeLocationMode,
};

use crate::focus::{FocusDecision, FocusLayer, FocusStack};
use crate::font::{CodeFontPreset, FontConfig, UiFontPreset};
use crate::theme::gpui_colors::Palette;
use crate::tokens::{radius, sizing, spacing, typography};

/// 7 大设置分区数量。
pub const SECTION_COUNT: usize = 7;

/// 对话框桌面端宽度（§2.1 `820px × 580px`）。
pub const DIALOG_WIDTH_PX: f32 = 820.0;

/// 对话框桌面端高度。
pub const DIALOG_HEIGHT_PX: f32 = 580.0;

/// 左侧分区导航轨宽度（§2.1 `200px`）。
pub const NAV_WIDTH_PX: f32 = 200.0;

/// 移动端降级断点：`< 640px` 转为全屏抽屉（§2.1）。
pub const MOBILE_DRAWER_BREAKPOINT_PX: f32 = 640.0;

/// 移动端行高下限（无障碍 `≥ 44px` 热区）。
pub const MOBILE_ROW_HEIGHT_PX: f32 = sizing::TOUCH_HIT_MIN_PX;

/// UI 字号可选区间（§2.2 分区 2：`12~17px`）。
pub const UI_FONT_SIZE_RANGE: (u8, u8) = (12, 17);

/// 代码字号可选区间（§2.2 分区 2：`11~17px`）。
pub const CODE_FONT_SIZE_RANGE: (u8, u8) = (11, 17);

/// 制表符宽度可选值。
pub const TAB_SIZE_CHOICES: [u8; 2] = [2, 4];

/// 后台健康探测间隔可选值（§2.2 分区 3：`15s / 30s / 60s`）。
pub const PROBE_INTERVAL_CHOICES_SECS: [u16; 3] = [15, 30, 60];

/// 命令执行超时可选值（§2.2 分区 5：`30s / 120s / 300s / 无限制`，`0` 表示无限制）。
pub const COMMAND_TIMEOUT_CHOICES_SECS: [u32; 4] = [30, 120, 300, 0];

/// 移动端空闲自动锁定可选值（§2.2 分区 6：`5m / 15m / 30m`）。
pub const REMOTE_IDLE_LOCK_CHOICES_MIN: [u16; 3] = [5, 15, 30];

/// 可扩展语言注册表预设（§2.2 分区 1：`zh-CN / zh-TW / en-US`）。
pub const LOCALE_PRESETS: [&str; 3] = ["zh-CN", "zh-TW", "en-US"];

/// 主题强调色预设数量（§2.2 分区 1：6 款预设 + 自定义拾色器）。
pub const ACCENT_PRESET_COUNT: usize = 6;

/// 7 大设置分区。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SettingsSection {
    /// 外观与主题。
    #[default]
    Appearance,
    /// 字体与排版。
    Fonts,
    /// 代理与路由器。
    Agents,
    /// 工作区与 Git。
    Workspace,
    /// 安全与审批红线。
    Security,
    /// 移动遥控与配对。
    Remote,
    /// 快捷键与关于。
    Shortcuts,
}

impl SettingsSection {
    /// 全部分区，按设计表序号顺序。
    pub const ALL: [Self; SECTION_COUNT] = [
        Self::Appearance,
        Self::Fonts,
        Self::Agents,
        Self::Workspace,
        Self::Security,
        Self::Remote,
        Self::Shortcuts,
    ];

    /// 分区名（§2.2 表格「分区名称」列）。
    pub const fn label(self) -> &'static str {
        match self {
            Self::Appearance => "外观与主题",
            Self::Fonts => "字体与排版",
            Self::Agents => "代理与路由器",
            Self::Workspace => "工作区与 Git",
            Self::Security => "安全与审批红线",
            Self::Remote => "移动遥控与配对",
            Self::Shortcuts => "快捷键与关于",
        }
    }

    /// Lucide 图标名（kebab-case，与 Web 端 100% 同名同义，§六）。
    pub const fn icon(self) -> &'static str {
        match self {
            Self::Appearance => "palette",
            Self::Fonts => "type",
            Self::Agents => "cpu",
            Self::Workspace => "git-branch",
            Self::Security => "shield-alert",
            Self::Remote => "smartphone",
            Self::Shortcuts => "keyboard",
        }
    }

    /// 分区细化配置项清单（§2.2 表格「细化配置项清单」列，供表单占位渲染）。
    pub const fn fields(self) -> &'static [&'static str] {
        match self {
            Self::Appearance => &[
                "主题模式",
                "界面语言",
                "主题强调色",
                "界面密度",
                "正文行距",
                "时间戳显示风格",
                "侧栏宽度重置",
                "减弱动效",
            ],
            Self::Fonts => &[
                "界面字体",
                "UI 字号",
                "代码字体跟随界面字体",
                "代码字体",
                "代码字号",
                "制表符宽度",
                "显示代码行号",
                "自动换行",
                "连字",
            ],
            Self::Agents => &[
                "协同与原生直连检测",
                "后台健康探测间隔",
                "上下文压缩建议阈值",
                "原生代理健康清单",
            ],
            Self::Workspace => &[
                "默认基准分支",
                "隔离工作树创建目录规则",
                "跨工作树共享 Rust 编译缓存",
                "关闭前未提交提醒",
                "自动保存检查点",
                "提交表情前缀",
            ],
            Self::Security => &[
                "默认权限档位",
                "命令执行超时限制",
                "界面主目录脱敏折叠",
                "高危破坏性命令只读锁",
            ],
            Self::Remote => &[
                "局域网直连服务",
                "空闲自动锁定超时",
                "二维码与 6 位动态配对码",
                "已授权手机在线列表",
            ],
            Self::Shortcuts => &[
                "分类快捷键对照表",
                "版本与协议信息",
                "重开新手三步引导",
                "7 级色彩令牌自检表",
                "恢复全部默认设置",
            ],
        }
    }

    /// 分区序号（`1..=7`，与设计表「分区序号」列一致）。
    pub const fn ordinal(self) -> usize {
        match self {
            Self::Appearance => 1,
            Self::Fonts => 2,
            Self::Agents => 3,
            Self::Workspace => 4,
            Self::Security => 5,
            Self::Remote => 6,
            Self::Shortcuts => 7,
        }
    }

    /// 按 `delta` 循环切换分区（越界回绕）。
    pub fn cycled(self, delta: i32) -> Self {
        let len = i32::try_from(SECTION_COUNT).unwrap_or(7);
        let index = i32::try_from(self.ordinal() - 1).unwrap_or(0);
        let next = (index + delta).rem_euclid(len);
        Self::ALL[usize::try_from(next).unwrap_or(0)]
    }
}

/// 对话框呈现形态（§2.1 桌面 / 移动降级）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SettingsLayout {
    /// 桌面端居中对话框。
    #[default]
    Desktop,
    /// 移动端全屏双段滚动抽屉。
    MobileDrawer,
}

/// 按窗口宽度推导的对话框几何。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SettingsGeometry {
    /// 卡片宽度。
    pub width: f32,
    /// 卡片高度（移动端撑满视口高度由上层换算）。
    pub height: f32,
    /// 左侧导航轨宽度（移动端为分组胶囊条高度语义沿用宽度字段）。
    pub nav_width: f32,
    /// 呈现形态。
    pub layout: SettingsLayout,
}

impl SettingsGeometry {
    /// 按实时窗口宽度推导（`< 640px` 降级为抽屉）。
    pub fn for_window(window_width: f32) -> Self {
        if window_width < MOBILE_DRAWER_BREAKPOINT_PX {
            Self {
                width: window_width.max(0.0),
                height: window_width.max(0.0),
                nav_width: 0.0,
                layout: SettingsLayout::MobileDrawer,
            }
        } else {
            Self {
                width: DIALOG_WIDTH_PX,
                height: DIALOG_HEIGHT_PX,
                nav_width: NAV_WIDTH_PX,
                layout: SettingsLayout::Desktop,
            }
        }
    }

    /// 单个设置行的高度：移动端强制 `≥ 44px` 热区。
    pub const fn row_height(&self) -> f32 {
        match self.layout {
            SettingsLayout::Desktop => sizing::SIDEBAR_ROW_HEIGHT_PX,
            SettingsLayout::MobileDrawer => MOBILE_ROW_HEIGHT_PX,
        }
    }
}

/// 设置弹窗状态（纯内存）。
#[derive(Clone, Debug)]
pub struct SettingsDialog {
    open: bool,
    active: SettingsSection,
    query: String,
    preferences: SoxuPreferences,
    stack: FocusStack,
    captured: Option<FocusLayer>,
}

impl Default for SettingsDialog {
    fn default() -> Self {
        Self {
            open: false,
            active: SettingsSection::default(),
            query: String::new(),
            preferences: SoxuPreferences::default(),
            stack: FocusStack::new(),
            captured: None,
        }
    }
}

impl SettingsDialog {
    /// 关闭态、默认偏好（`SoxuPreferences::default()`，双端同构）的设置弹窗。
    pub fn new() -> Self {
        Self::default()
    }

    /// 是否已打开。
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// 当前分区。
    pub fn active_section(&self) -> SettingsSection {
        self.active
    }

    /// 只读偏好快照。
    pub fn preferences(&self) -> &SoxuPreferences {
        &self.preferences
    }

    /// `/` 搜索词。
    pub fn query(&self) -> &str {
        &self.query
    }

    /// 覆写搜索词。
    pub fn set_query(&mut self, query: impl Into<String>) {
        self.query = query.into();
    }

    /// 焦点栈只读视图（测试与键位分发）。
    pub fn stack(&self) -> &FocusStack {
        &self.stack
    }

    /// 显式入栈低层焦点上下文（打开设置前的当前层）。
    pub fn push_layer(&mut self, layer: FocusLayer) -> FocusDecision {
        self.stack.push(layer)
    }

    /// 打开设置：捕获当前焦点层并压入 `ModalDialog`。
    ///
    /// 返回 `false` 表示已打开或模态层被拒绝。
    pub fn open_dialog(&mut self) -> bool {
        if self.open {
            return false;
        }
        let captured = self.stack.active();
        if self.stack.push(FocusLayer::ModalDialog) != FocusDecision::Allowed {
            return false;
        }
        self.captured = captured;
        self.open = true;
        true
    }

    /// 关闭设置并还原焦点到打开前所在层。
    pub fn close_dialog(&mut self) -> Option<FocusLayer> {
        if !self.open {
            return None;
        }
        let closed = self.stack.close_active();
        if self.stack.active() != self.captured
            && let Some(layer) = self.captured
            && self.stack.active().is_none()
        {
            self.stack.push(layer);
        }
        self.captured = None;
        self.open = false;
        closed
    }

    /// `Esc` 裁决：打开时关闭并还原焦点，返回 `true`（消费即 `stop_propagation`）。
    pub fn handle_escape(&mut self) -> bool {
        self.close_dialog().is_some()
    }

    /// 当前生效 key context。
    pub fn effective_context(&self) -> &'static str {
        self.stack.effective_context()
    }

    /// 激活某分区。
    pub fn activate(&mut self, section: SettingsSection) {
        self.active = section;
    }

    /// 按 `delta` 切换分区（`↑`/`↓`、`Tab` 顺序遍历）。
    pub fn navigate(&mut self, delta: i32) {
        self.active = self.active.cycled(delta);
    }

    /// 搜索过滤：命中分区名或任一配置项名（大小写不敏感）。
    pub fn filtered_sections(&self) -> Vec<SettingsSection> {
        let needle = self.query.trim().to_lowercase();
        SettingsSection::ALL
            .iter()
            .copied()
            .filter(|section| {
                needle.is_empty()
                    || section.label().to_lowercase().contains(&needle)
                    || section
                        .fields()
                        .iter()
                        .any(|field| field.to_lowercase().contains(&needle))
            })
            .collect()
    }

    /// UI 字号（`12~17px` 区间钳制）。
    pub fn set_ui_font_size(&mut self, size: u8) -> u8 {
        self.preferences.ui_font_size = clamp_to_range(size, UI_FONT_SIZE_RANGE);
        self.preferences.ui_font_size
    }

    /// 代码字号（`11~17px` 区间钳制）。
    pub fn set_code_font_size(&mut self, size: u8) -> u8 {
        self.preferences.code_font_size = clamp_to_range(size, CODE_FONT_SIZE_RANGE);
        self.preferences.code_font_size
    }

    /// 切换制表符宽度（`2 ↔ 4`）。
    pub fn cycle_tab_size(&mut self) -> u8 {
        self.preferences.tab_size = match self.preferences.tab_size {
            2 => 4,
            _ => 2,
        };
        self.preferences.tab_size
    }

    /// 代码字体下拉框是否可用：`codeFollowsUi = true` 时置淡禁用（§3.3）。
    pub fn code_font_picker_enabled(&self) -> bool {
        !self.preferences.code_follows_ui
    }

    /// 实际生效的代码字体族（跟随开关联动 `FontConfig::effective_code_family`）。
    pub fn effective_code_family(&self) -> &'static str {
        self.font_config().effective_code_family()
    }

    /// 由偏好 Schema 派生字体配置（协议枚举 → 视图枚举的唯一映射点）。
    pub fn font_config(&self) -> FontConfig {
        FontConfig {
            ui_preset: ui_preset(self.preferences.ui_font_preset),
            code_preset: code_preset(self.preferences.code_font_preset),
            code_follows_ui: self.preferences.code_follows_ui,
        }
    }

    /// 偏好自检：枚举取值必须落在设计允许集合内（防止持久化脏值）。
    pub fn validate(&self) -> Vec<&'static str> {
        let mut issues = Vec::new();
        if !in_range(self.preferences.ui_font_size, UI_FONT_SIZE_RANGE) {
            issues.push("ui_font_size");
        }
        if !in_range(self.preferences.code_font_size, CODE_FONT_SIZE_RANGE) {
            issues.push("code_font_size");
        }
        if !TAB_SIZE_CHOICES.contains(&self.preferences.tab_size) {
            issues.push("tab_size");
        }
        if !PROBE_INTERVAL_CHOICES_SECS.contains(&self.preferences.probe_interval_sec) {
            issues.push("probe_interval_sec");
        }
        if !COMMAND_TIMEOUT_CHOICES_SECS.contains(&self.preferences.command_timeout_sec) {
            issues.push("command_timeout_sec");
        }
        if !REMOTE_IDLE_LOCK_CHOICES_MIN.contains(&self.preferences.remote_idle_lock_min) {
            issues.push("remote_idle_lock_min");
        }
        if self.preferences.locale.is_empty() {
            issues.push("locale");
        }
        issues
    }

    /// TOKEN：主题模式 / 语言 / 密度 / 行距 / 时间戳 / 工作树位置的可读摘要行。
    pub fn appearance_summary(&self) -> Vec<(&'static str, String)> {
        vec![
            (
                "主题模式",
                theme_mode_label(self.preferences.theme_mode).to_string(),
            ),
            ("界面语言", self.preferences.locale.clone()),
            (
                "主题强调色",
                accent_label(self.preferences.accent_preset).to_string(),
            ),
            (
                "界面密度",
                density_label(self.preferences.density).to_string(),
            ),
            (
                "正文行距",
                line_height_label(self.preferences.ui_line_height).to_string(),
            ),
            (
                "时间戳显示风格",
                timestamp_label(self.preferences.timestamp_format).to_string(),
            ),
            (
                "工作树位置",
                worktree_label(self.preferences.worktree_location).to_string(),
            ),
        ]
    }
}

/// 闭区间钳制。
fn clamp_to_range(value: u8, range: (u8, u8)) -> u8 {
    value.clamp(range.0, range.1)
}

/// 是否落在闭区间内。
fn in_range(value: u8, range: (u8, u8)) -> bool {
    (range.0..=range.1).contains(&value)
}

/// 协议 UI 字体预设 → 视图预设。
fn ui_preset(id: UiFontPresetId) -> UiFontPreset {
    match id {
        UiFontPresetId::InterNoto => UiFontPreset::InterNoto,
        UiFontPresetId::IbmPlexNoto => UiFontPreset::IbmPlexNoto,
        UiFontPresetId::JakartaNoto => UiFontPreset::JakartaNoto,
        UiFontPresetId::LxgwWenkai => UiFontPreset::LxgwWenkai,
        UiFontPresetId::SystemSans => UiFontPreset::SystemSans,
    }
}

/// 协议代码字体预设 → 视图预设。
fn code_preset(id: CodeFontPresetId) -> CodeFontPreset {
    match id {
        CodeFontPresetId::JetbrainsMono => CodeFontPreset::JetBrainsMono,
        CodeFontPresetId::FiraCode => CodeFontPreset::FiraCode,
        CodeFontPresetId::MapleMono => CodeFontPreset::MapleMono,
        CodeFontPresetId::IbmPlexMono => CodeFontPreset::IbmPlexMono,
        CodeFontPresetId::CascadiaCode => CodeFontPreset::CascadiaCode,
    }
}

/// 主题模式可读文案（偏好 Schema 的 `themeMode`，语义与 `settings.theme.*` 一致）。
pub fn theme_mode_label(mode: soxu_protocol::ThemeMode) -> &'static str {
    match mode {
        soxu_protocol::ThemeMode::Dark => "纯黑深色",
        soxu_protocol::ThemeMode::Light => "纯净浅色",
        soxu_protocol::ThemeMode::System => "跟随系统",
    }
}

/// 强调色预设文案（`accentPreset`）。
fn accent_label(preset: AccentPresetId) -> &'static str {
    match preset {
        AccentPresetId::Emerald => "翡翠",
        AccentPresetId::Ocean => "海洋",
        AccentPresetId::Violet => "紫罗兰",
        AccentPresetId::Amber => "琥珀",
        AccentPresetId::Rose => "玫瑰",
        AccentPresetId::Cyan => "青蓝",
        AccentPresetId::Custom => "自定义",
    }
}

/// 密度文案（`density`）。
fn density_label(density: DensityMode) -> &'static str {
    match density {
        DensityMode::Compact => "紧凑",
        DensityMode::Comfortable => "舒适",
    }
}

/// 行距文案（`uiLineHeight`）。
fn line_height_label(mode: LineHeightMode) -> &'static str {
    match mode {
        LineHeightMode::Compact => "紧凑",
        LineHeightMode::Normal => "标准",
        LineHeightMode::Relaxed => "宽松",
    }
}

/// 时间戳文案（`timestampFormat`）。
fn timestamp_label(mode: TimestampFormatMode) -> &'static str {
    match mode {
        TimestampFormatMode::Relative => "相对时间",
        TimestampFormatMode::Absolute => "绝对时间",
    }
}

/// 工作树位置文案（`worktreeLocation`）。
fn worktree_label(mode: WorktreeLocationMode) -> &'static str {
    match mode {
        WorktreeLocationMode::Sibling => "同级目录",
        WorktreeLocationMode::Central => "集中管理",
    }
}

/// 分区导航轨（`200px`，`#0D0D0D` 底）。
fn nav_rail(dialog: &SettingsDialog, palette: &Palette) -> Div {
    let mut rail = div()
        .flex()
        .flex_col()
        .flex_none()
        .w(px(NAV_WIDTH_PX))
        .h_full()
        .gap(px(spacing::SPACE_HALF_PX))
        .p(px(spacing::SPACE_2_PX))
        .bg(palette.surface_sidebar)
        .border_r_1()
        .border_color(palette.border_hairline);
    for section in SettingsSection::ALL {
        let selected = section == dialog.active;
        let mut row = div()
            .flex()
            .items_center()
            .gap(px(spacing::SPACE_2_PX))
            .h(px(sizing::SIDEBAR_ROW_HEIGHT_PX))
            .px(px(spacing::SPACE_2_PX))
            .rounded(px(radius::RADIUS_SM_PX))
            .text_size(px(typography::TEXT_CAPTION_PX))
            .child(format!("{} · {}", section.ordinal(), section.label()));
        row = if selected {
            row.bg(palette.interaction_selected)
                .text_color(palette.text_primary)
        } else {
            row.text_color(palette.text_tertiary)
        };
        rail = rail.child(row);
    }
    rail
}

/// 当前分区表单占位区（内容区 `#171717`）。
fn section_panel(dialog: &SettingsDialog, palette: &Palette) -> Div {
    let section = dialog.active;
    let mut panel = div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .min_h_0()
        .gap(px(spacing::SPACE_3_PX))
        .p(px(spacing::SPACE_5_PX))
        .bg(palette.surface_card)
        .child(
            div()
                .text_size(px(typography::TEXT_TITLE_PX))
                .text_color(palette.text_primary)
                .child(section.label()),
        );
    for field in section.fields() {
        panel = panel.child(
            div()
                .flex()
                .items_center()
                .h(px(MOBILE_ROW_HEIGHT_PX))
                .px(px(spacing::SPACE_3_PX))
                .rounded(px(radius::RADIUS_SM_PX))
                .bg(palette.surface_raised)
                .border_1()
                .border_color(palette.border_subtle)
                .text_size(px(typography::TEXT_CAPTION_PX))
                .text_color(palette.text_secondary)
                .child(field.to_string()),
        );
    }
    // 代码字体跟随联动：跟随开启时独立字体选择置淡禁用（§3.3）。
    if section == SettingsSection::Fonts && !dialog.code_font_picker_enabled() {
        panel = panel.child(
            div()
                .text_size(px(typography::TEXT_MICRO_PX))
                .text_color(palette.text_tertiary)
                .child("当前已跟随界面字体"),
        );
    }
    panel
}

/// 渲染设置弹窗卡片（桌面端 `820×580`；移动端尺寸由叠加层按视口换算）。
pub fn render(dialog: &SettingsDialog, palette: &Palette) -> Div {
    div()
        .flex()
        .flex_col()
        .w(px(DIALOG_WIDTH_PX))
        .h(px(DIALOG_HEIGHT_PX))
        .rounded(px(radius::RADIUS_LG_PX))
        .overflow_hidden()
        .bg(palette.surface_card)
        .border_1()
        .border_color(palette.border_strong)
        .child(
            div()
                .flex()
                .items_center()
                .h(px(sizing::TITLEBAR_HEIGHT_PX))
                .px(px(spacing::SPACE_4_PX))
                .border_b_1()
                .border_color(palette.border_hairline)
                .text_size(px(typography::TEXT_BODY_PX))
                .text_color(palette.text_primary)
                .child("设置"),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .flex_1()
                .min_h_0()
                .child(nav_rail(dialog, palette))
                .child(section_panel(dialog, palette)),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 7 分区顺序、序号、图标名与设计表逐一对应。
    #[test]
    fn sections_match_design_table() {
        assert_eq!(SettingsSection::ALL.len(), SECTION_COUNT);
        let expected = [
            ("外观与主题", "palette"),
            ("字体与排版", "type"),
            ("代理与路由器", "cpu"),
            ("工作区与 Git", "git-branch"),
            ("安全与审批红线", "shield-alert"),
            ("移动遥控与配对", "smartphone"),
            ("快捷键与关于", "keyboard"),
        ];
        for (index, (label, icon)) in expected.iter().enumerate() {
            let section = SettingsSection::ALL[index];
            assert_eq!(section.label(), *label);
            assert_eq!(section.icon(), *icon);
            assert_eq!(section.ordinal(), index + 1);
            assert!(!section.fields().is_empty(), "{label} 应有细化配置项");
        }
    }

    /// 分区导航循环切换与直接激活。
    #[test]
    fn navigation_cycles_and_activates() {
        let mut dialog = SettingsDialog::new();
        assert_eq!(dialog.active_section(), SettingsSection::Appearance);
        dialog.navigate(1);
        assert_eq!(dialog.active_section(), SettingsSection::Fonts);
        dialog.navigate(-1);
        assert_eq!(dialog.active_section(), SettingsSection::Appearance);
        dialog.navigate(-1);
        assert_eq!(
            dialog.active_section(),
            SettingsSection::Shortcuts,
            "越界回绕到末项"
        );
        dialog.activate(SettingsSection::Security);
        assert_eq!(dialog.active_section(), SettingsSection::Security);
    }

    /// 搜索过滤命中分区名与配置项名。
    #[test]
    fn search_filters_sections_and_fields() {
        let mut dialog = SettingsDialog::new();
        assert_eq!(dialog.filtered_sections().len(), SECTION_COUNT);

        dialog.set_query("字体");
        let hits = dialog.filtered_sections();
        assert_eq!(hits, vec![SettingsSection::Fonts]);

        dialog.set_query("探测间隔");
        assert_eq!(dialog.filtered_sections(), vec![SettingsSection::Agents]);

        dialog.set_query("zzz");
        assert!(dialog.filtered_sections().is_empty());
    }

    /// 几何：桌面 `820×580` + `200px` 导航；`< 640px` 降级抽屉且行高 ≥ 44px。
    #[test]
    fn geometry_switches_at_640() {
        let desktop = SettingsGeometry::for_window(1280.0);
        assert_eq!(desktop.layout, SettingsLayout::Desktop);
        assert_eq!(desktop.width, DIALOG_WIDTH_PX);
        assert_eq!(desktop.height, DIALOG_HEIGHT_PX);
        assert_eq!(desktop.nav_width, NAV_WIDTH_PX);
        assert_eq!(desktop.row_height(), sizing::SIDEBAR_ROW_HEIGHT_PX);

        let mobile = SettingsGeometry::for_window(639.0);
        assert_eq!(mobile.layout, SettingsLayout::MobileDrawer);
        assert_eq!(mobile.width, 639.0);
        assert!(mobile.row_height() >= 44.0);
        assert_eq!(MOBILE_ROW_HEIGHT_PX, sizing::TOUCH_HIT_MIN_PX);

        let edge = SettingsGeometry::for_window(640.0);
        assert_eq!(edge.layout, SettingsLayout::Desktop);
    }

    /// 焦点捕获与还原：打开即模态锁定，Esc 关闭回到打开前所在层。
    #[test]
    fn open_captures_and_escape_restores_focus() {
        let mut dialog = SettingsDialog::new();
        assert!(!dialog.is_open());
        assert_eq!(
            dialog.push_layer(FocusLayer::ComposerInput),
            FocusDecision::Allowed
        );

        assert!(dialog.open_dialog());
        assert!(dialog.is_open());
        assert_eq!(dialog.effective_context(), "ModalDialog");
        assert!(!dialog.open_dialog(), "重复打开应被拒绝");

        // 模态锁定：低层无法夺焦。
        assert_eq!(
            dialog.push_layer(FocusLayer::ReviewDiff),
            FocusDecision::RejectedModalLock
        );
        assert_eq!(dialog.effective_context(), "ModalDialog");

        assert!(dialog.handle_escape());
        assert!(!dialog.is_open());
        assert_eq!(
            dialog.effective_context(),
            "ComposerInput",
            "Esc 后焦点必须还原到打开前的层"
        );
        assert!(!dialog.handle_escape(), "关闭后 Esc 不再被设置窗消费");
    }

    /// 打开前无焦点上下文时关闭后回落到全局层。
    #[test]
    fn escape_without_previous_layer_falls_back_to_global() {
        let mut dialog = SettingsDialog::new();
        assert!(dialog.open_dialog());
        assert!(dialog.handle_escape());
        assert_eq!(dialog.effective_context(), "WorkbenchGlobal");
    }

    /// 字号钳制与制表符循环。
    #[test]
    fn font_size_clamping_and_tab_cycle() {
        let mut dialog = SettingsDialog::new();
        assert_eq!(dialog.set_ui_font_size(9), UI_FONT_SIZE_RANGE.0);
        assert_eq!(dialog.set_ui_font_size(99), UI_FONT_SIZE_RANGE.1);
        assert_eq!(dialog.set_ui_font_size(15), 15);

        assert_eq!(dialog.set_code_font_size(4), CODE_FONT_SIZE_RANGE.0);
        assert_eq!(dialog.set_code_font_size(42), CODE_FONT_SIZE_RANGE.1);

        assert_eq!(dialog.cycle_tab_size(), 4);
        assert_eq!(dialog.cycle_tab_size(), 2);
    }

    /// 代码字体跟随联动：跟随开启时下拉框禁用且生效字体切换到界面字体。
    #[test]
    fn code_font_follows_ui_toggle() {
        let mut dialog = SettingsDialog::new();
        assert!(dialog.code_font_picker_enabled());
        let independent = dialog.effective_code_family();
        assert_eq!(independent, CodeFontPreset::JetBrainsMono.primary_family());

        dialog.preferences.code_follows_ui = true;
        assert!(!dialog.code_font_picker_enabled());
        assert_eq!(
            dialog.effective_code_family(),
            dialog.font_config().ui_family()
        );
        assert_eq!(dialog.effective_code_family(), "Inter");
    }

    /// 偏好默认值全部落在设计允许集合内，且可读摘要覆盖 7 项外观项。
    #[test]
    fn defaults_validate_and_summary_is_complete() {
        let dialog = SettingsDialog::new();
        assert!(dialog.validate().is_empty(), "默认偏好不应有任何越界项");
        let summary = dialog.appearance_summary();
        assert_eq!(summary.len(), 7);
        assert_eq!(summary[0], ("主题模式", "纯黑深色".to_string()));
        assert_eq!(summary[1], ("界面语言", "zh-CN".to_string()));
        assert_eq!(summary[2], ("主题强调色", "翡翠".to_string()));
        assert_eq!(ACCENT_PRESET_COUNT, 6);
        assert_eq!(LOCALE_PRESETS.len(), 3);
        assert_eq!(PROBE_INTERVAL_CHOICES_SECS, [15, 30, 60]);
        assert_eq!(COMMAND_TIMEOUT_CHOICES_SECS, [30, 120, 300, 0]);
        assert_eq!(REMOTE_IDLE_LOCK_CHOICES_MIN, [5, 15, 30]);
    }

    /// 脏值会被 validate 逐项报出（防止持久化越界值静默生效）。
    #[test]
    fn validate_reports_out_of_range_values() {
        let mut dialog = SettingsDialog::new();
        dialog.preferences.ui_font_size = 30;
        dialog.preferences.tab_size = 3;
        dialog.preferences.probe_interval_sec = 45;
        let issues = dialog.validate();
        assert!(issues.contains(&"ui_font_size"));
        assert!(issues.contains(&"tab_size"));
        assert!(issues.contains(&"probe_interval_sec"));
        assert!(!issues.contains(&"code_font_size"));
    }

    /// 主题模式文案与 `settings.theme.*` i18n 语义一致（不新增 key）。
    #[test]
    fn theme_mode_labels_match_i18n_text() {
        assert_eq!(theme_mode_label(soxu_protocol::ThemeMode::Dark), "纯黑深色");
        assert_eq!(
            theme_mode_label(soxu_protocol::ThemeMode::Light),
            "纯净浅色"
        );
        assert_eq!(
            theme_mode_label(soxu_protocol::ThemeMode::System),
            "跟随系统"
        );
    }
}
