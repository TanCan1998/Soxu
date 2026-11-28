//! M5-B 同步宽输入舱（Composer）视图模型与落图。
//!
//! 规范来源（唯一语义来源 `design-system/soxu/**`）：
//! - `pages/02-composer-and-handoff.md`：三态输入舱几何、纯图标权限、`@agent` 移交与
//!   范围微开关、变更摘要浮药丸、Oryn 遥测微脚条、IME 组合护栏、`+`/`/` 补全；
//! - `pages/07-settings-fonts-and-motion.md`：安全与审批红线中的默认权限档位；
//! - `MASTER.md` §2 表面海拔、§3.2 圆角与阅读行宽、§3.3 图标纪律、§4.1 渲染零阻塞。
//!
//! 本阶段边界（Issue #130）：
//! - **纯内存视图模型**，不接 daemon I/O、不读文件系统、不起子进程；
//! - 协议契约全部复用 `soxu-protocol`（`PermissionLevel` / `HandoffScopeOptions` /
//!   `ComposerChip` / `SessionTelemetryStats` / `ContextUsageBand` / `TurnLifecycleState`）；
//! - 与对话流共用 760px 宽度契约，几何只消费 [`crate::layout::LayoutPlan`] 与设计令牌。
//!
//! 子模块：
//! - [`metrics`]：760px 同步宽几何（拖拽联动）；
//! - [`permission`]：6 档纯图标权限状态机；
//! - [`handoff`]：`@agent` 移交胶囊与范围微开关；
//! - [`keys`]：Enter / Shift+Enter 与 IME 组合门控；
//! - [`chips`]：上下文芯片、`+` 菜单与 `/` 命令过滤；
//! - [`stats`]：三色圆环与 11 项遥测统计；
//! - [`changes`]：变更摘要浮药丸与内存动作回调。

pub mod changes;
pub mod chips;
pub mod handoff;
pub mod keys;
pub mod metrics;
pub mod permission;
pub mod stats;

use gpui::{
    Div, ElementId, Hsla, InteractiveElement, IntoElement, ParentElement, PathBuilder,
    SharedString, StatefulInteractiveElement, Styled, canvas, div, point, px,
};
use soxu_protocol::{
    ContextUsageBand, PermissionLevel, PermissionTone, SessionTelemetryStats, TurnLifecycleState,
};

use crate::layout::LayoutPlan;
use crate::theme::gpui_colors::Palette;
use crate::tokens::{radius, sizing, spacing, typography};

pub use changes::{ActionSink, ChangeSummary, ComposerAction};
pub use chips::{
    AUTOCOMPLETE_MAX_ROWS, ChipSet, PlusMenuSection, SlashCommand, SlashKind, code_chip,
    filter_slash_commands,
};
pub use handoff::{HandoffScopeToggle, HandoffState};
pub use keys::{ComposerKey, KeyOutcome, KeyStroke, SubmitIntent, resolve as resolve_key};
pub use metrics::{CANVAS_GUTTER_PX, ComposerGeometry, SYNC_CONTENT_MAX_WIDTH_PX};
pub use permission::{
    CycleDirection, PERMISSION_CATALOG, PermissionMeta, PermissionSelection, PermissionState,
};
pub use stats::{
    EMPTY_TELEMETRY_LABEL, FooterReadout, RING_RADIUS_PX, RING_STROKE_PX, TelemetryGroup,
    TelemetryRow, TelemetryView, UsageRing, ring_arc, telemetry_view,
};

/// 输入舱占位文案（`pages/02` §1 示意文本）。
pub const PLACEHOLDER: &str = "输入指令、按需要选择运行中纠偏模式或移交代理...";

/// 页脚左侧快捷键提示（`pages/02` §1 示意文本）。
pub const FOOTER_HINT: &str = "Enter 发送 · Shift+Enter 换行";

/// 三种回合投递模式的纯图标字形（非 Emoji，`pages/02` §3.5）。
pub const BEHAVIOR_GLYPHS: [&str; 3] = ["↯", "≡", "⊗"];

/// 发送键字形（空闲/新回合）。
const SEND_GLYPH: &str = "↑";
/// 停止键字形（运行中空输入）。
const STOP_GLYPH: &str = "■";
/// 准备期字形。
const PREPARING_GLYPH: &str = "◌";

/// 输入舱视图模型：组合权限、移交、芯片、遥测与草稿的纯内存状态。
#[derive(Clone, Debug, PartialEq)]
pub struct ComposerState {
    /// 6 档执行权限。
    pub permission: PermissionState,
    /// `@agent` 移交状态。
    pub handoff: HandoffState,
    /// 已挂载的上下文芯片。
    pub chips: ChipSet,
    /// 会话遥测快照。
    pub stats: SessionTelemetryStats,
    /// 工作树变更摘要。
    pub changes: ChangeSummary,
    /// 当前回合生命周期。
    pub phase: TurnLifecycleState,
    draft: String,
}

impl ComposerState {
    /// 空态输入舱：默认权限档位、未选移交、无芯片、零遥测、空闲回合。
    pub fn placeholder() -> Self {
        Self {
            permission: PermissionState::new(),
            handoff: HandoffState::new(),
            chips: ChipSet::new(),
            stats: SessionTelemetryStats::default(),
            changes: ChangeSummary::default(),
            phase: TurnLifecycleState::Idle,
            draft: String::new(),
        }
    }

    /// 当前草稿文本。
    pub fn draft(&self) -> &str {
        &self.draft
    }

    /// 覆写草稿文本。
    pub fn set_draft(&mut self, draft: impl Into<String>) {
        self.draft = draft.into();
    }

    /// 草稿是否为空（仅空白字符也视为空）。
    pub fn is_draft_empty(&self) -> bool {
        self.draft.trim().is_empty()
    }

    /// 裁决一次按键（委托 [`keys::resolve`]，纯逻辑可测）。
    pub fn handle_key(&self, stroke: &KeyStroke) -> KeyOutcome {
        resolve_key(stroke, self.phase, self.is_draft_empty())
    }

    /// 点击变更摘要浮药丸：非空变更时记录聚焦 Diff 动作。
    ///
    /// 返回 `None` 表示没有变更、药丸不可点。
    pub fn activate_change_pill(&self) -> Option<ComposerAction> {
        if self.changes.is_empty() {
            return None;
        }
        let action = ComposerAction::FocusDiff;
        ActionSink::push(action.clone());
        Some(action)
    }

    /// 点击权限按钮 / `Alt+P`：循环到下一档并记录动作。
    pub fn cycle_permission(&mut self, direction: CycleDirection) -> PermissionLevel {
        let next = self.permission.cycle(direction);
        ActionSink::push(ComposerAction::CyclePermission);
        next
    }

    /// 翻转移交范围微开关并记录动作。
    pub fn toggle_handoff_scope(&mut self, toggle: HandoffScopeToggle) -> &'static str {
        self.handoff.toggle(toggle);
        let key = match toggle {
            HandoffScopeToggle::SummaryTurns => "summary_turns",
            HandoffScopeToggle::UncommittedDiffPaths => "uncommitted_diff_paths",
            HandoffScopeToggle::SpawnWorktree => "spawn_worktree",
        };
        ActionSink::push(ComposerAction::ToggleHandoffScope(key));
        key
    }

    /// 移除一枚上下文芯片并记录动作。
    pub fn remove_chip(&mut self, id: &str) -> bool {
        let removed = self.chips.remove(id);
        if removed {
            ActionSink::push(ComposerAction::RemoveChip(id.to_string()));
        }
        removed
    }

    /// 执行一条斜杠命令（本阶段仅记录内存动作，不触发任何 I/O）。
    pub fn run_slash_command(&mut self, name: &'static str) {
        ActionSink::push(ComposerAction::RunSlashCommand(name));
    }

    /// 上下文占用圆环快照。
    pub fn usage_ring(&self) -> UsageRing {
        UsageRing::from_stats(&self.stats)
    }

    /// 11 项遥测呈现状态（零值 → 空态）。
    pub fn telemetry(&self) -> TelemetryView {
        telemetry_view(&self.stats)
    }

    /// 按窗口宽度与布局计划推导几何。
    pub fn geometry(&self, window_width: f32, plan: &LayoutPlan) -> ComposerGeometry {
        ComposerGeometry::for_default_panels(window_width, plan)
    }
}

impl Default for ComposerState {
    fn default() -> Self {
        Self::placeholder()
    }
}

/// 携带 alpha 的颜色副本（用于柔和底色，避免新增令牌）。
fn with_alpha(color: Hsla, alpha: f32) -> Hsla {
    Hsla { a: alpha, ..color }
}

/// 权限档位语义色调 → GPUI 颜色。
fn tone_color(tone: PermissionTone, palette: &Palette) -> Hsla {
    match tone {
        PermissionTone::Neutral => palette.text_secondary,
        PermissionTone::Accent => palette.semantic_accent,
        PermissionTone::Warning => palette.semantic_warning,
        PermissionTone::Danger => palette.semantic_danger,
    }
}

/// 三色圆环档位 → GPUI 颜色（绿 / 琥珀 / 绯红）。
fn band_color(band: ContextUsageBand, palette: &Palette) -> Hsla {
    match band {
        ContextUsageBand::SafeGreen => palette.semantic_success,
        ContextUsageBand::WarningAmber => palette.semantic_warning,
        ContextUsageBand::DangerRed => palette.semantic_danger,
    }
}

/// 三段语义圆环：12 点钟起点顺时针弧线 + 低透明度轨道（`Canvas` 直接绘制）。
fn usage_ring_element(ring: UsageRing, palette: &Palette) -> impl IntoElement {
    let color = band_color(ring.band, palette);
    let track = with_alpha(color, 0.24);
    let arc = ring_arc(ring.percent, RING_RADIUS_PX);
    let is_full = ring.is_full;
    let diameter = 2.0 * (RING_RADIUS_PX + RING_STROKE_PX);

    canvas(
        move |_bounds, _window, _cx| (),
        move |bounds, (), window, _cx| {
            let center_x = bounds.origin.x.as_f32() + bounds.size.width.as_f32() / 2.0;
            let center_y = bounds.origin.y.as_f32() + bounds.size.height.as_f32() / 2.0;
            let radius = point(px(RING_RADIUS_PX), px(RING_RADIUS_PX));

            let full_circle = |builder: &mut PathBuilder| {
                builder.move_to(point(px(center_x), px(center_y - RING_RADIUS_PX)));
                builder.arc_to(
                    radius,
                    px(0.0),
                    false,
                    true,
                    point(px(center_x), px(center_y + RING_RADIUS_PX)),
                );
                builder.arc_to(
                    radius,
                    px(0.0),
                    false,
                    true,
                    point(px(center_x), px(center_y - RING_RADIUS_PX)),
                );
            };

            // 轨道：整圈低透明度衬底，保证纯黑背景上圆环轮廓恒可见。
            let mut track_builder = PathBuilder::stroke(px(RING_STROKE_PX));
            full_circle(&mut track_builder);
            if let Ok(path) = track_builder.build() {
                window.paint_path(path, track);
            }

            // 进度：满量程画整圈，其余画 12 点钟起点顺时针弧线。
            let mut builder = PathBuilder::stroke(px(RING_STROKE_PX));
            if is_full {
                full_circle(&mut builder);
            } else {
                builder.move_to(point(
                    px(center_x + arc.start.0),
                    px(center_y + arc.start.1),
                ));
                builder.arc_to(
                    radius,
                    px(0.0),
                    arc.large_arc,
                    true,
                    point(px(center_x + arc.end.0), px(center_y + arc.end.1)),
                );
            }
            if let Ok(path) = builder.build() {
                window.paint_path(path, color);
            }
        },
    )
    .w(px(diameter))
    .h(px(diameter))
}

/// 无框图钮：透明底、无描边，仅承载单字形与点击回调。
fn glyph_button(
    id: &'static str,
    glyph: String,
    color: Hsla,
    background: Option<Hsla>,
    action: ComposerAction,
) -> impl IntoElement {
    let mut button = div()
        .id(ElementId::Name(SharedString::from(id)))
        .flex()
        .items_center()
        .justify_center()
        .w(px(28.0))
        .h(px(28.0))
        .rounded(px(radius::RADIUS_SM_PX))
        .text_size(px(typography::TEXT_CAPTION_PX))
        .text_color(color)
        .hover(|style| style.bg(color))
        .on_click(move |_event, _window, _cx| ActionSink::push(action.clone()))
        .child(glyph);

    if let Some(background) = background {
        button = button.bg(background);
    } else {
        button = button.bg(hsla_transparent());
    }
    button
}

/// 完全透明背景（GPUI 无 `transparent` 常量，统一在此构造）。
fn hsla_transparent() -> Hsla {
    Hsla {
        h: 0.0,
        s: 0.0,
        l: 0.0,
        a: 0.0,
    }
}

/// 上下文芯片行（`pages/02` §5.1）。
fn chip_row(state: &ComposerState, palette: &Palette) -> Div {
    let mut row = div()
        .flex()
        .flex_row()
        .items_center()
        .flex_wrap()
        .gap(px(spacing::SPACE_1_5_PX))
        .px(px(spacing::SPACE_2_PX))
        .pt(px(spacing::SPACE_2_PX));

    for chip in state.chips.iter() {
        let is_handoff = matches!(chip.kind, soxu_protocol::ComposerChipKind::Handoff);
        let background = if is_handoff {
            with_alpha(palette.semantic_handoff, 0.14)
        } else {
            palette.surface_card
        };
        let text = if is_handoff {
            palette.semantic_handoff
        } else {
            palette.text_secondary
        };
        let mut pill = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(spacing::SPACE_1_PX))
            .px(px(spacing::SPACE_1_5_PX))
            .h(px(26.0))
            .rounded(px(radius::RADIUS_SM_PX))
            .bg(background)
            .border_1()
            .border_color(palette.border_subtle)
            .text_size(px(typography::TEXT_MICRO_PX))
            .text_color(text)
            .child(chip.label.clone());

        // 移交胶囊以紫罗兰色高亮，其余芯片带 `×` 移除键。
        if !is_handoff {
            let chip_id = chip.id.clone();
            pill = pill.child(
                div()
                    .id(ElementId::Name(SharedString::from(format!(
                        "chip-remove-{}",
                        chip.id
                    ))))
                    .w(px(sizing::ICON_SM_PX))
                    .h(px(sizing::ICON_SM_PX))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(palette.text_tertiary)
                    .child("×")
                    .on_click(move |_event, _window, _cx| {
                        ActionSink::push(ComposerAction::RemoveChip(chip_id.clone()));
                    }),
            );
        }
        row = row.child(pill);
    }
    row
}

/// 文本编辑区（最小 44px，桌面字号 14px / 行高 21px）。
fn input_area(state: &ComposerState, palette: &Palette) -> Div {
    let empty = state.is_draft_empty();
    let text = if empty {
        PLACEHOLDER.to_string()
    } else {
        state.draft().to_string()
    };
    div()
        .flex()
        .flex_col()
        .justify_center()
        .min_h(px(44.0))
        .px(px(spacing::SPACE_3_PX))
        .py(px(spacing::SPACE_2_PX))
        .text_size(px(typography::TEXT_BODY_PX))
        .line_height(px(21.0))
        .text_color(if empty {
            palette.text_ghost
        } else {
            palette.text_primary
        })
        .child(text)
}

/// 主操控行：`+` / 权限 / `@` 移交 / 三态模式组 / 发送键。
fn control_row(state: &ComposerState, geometry: ComposerGeometry, palette: &Palette) -> Div {
    let mut row = div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(spacing::SPACE_1_5_PX))
        .px(px(spacing::SPACE_2_PX))
        .pb(px(spacing::SPACE_2_PX));

    // 左侧 `+` 更多操作（无厚重灰底大热区透明圆钮）。
    row = row.child(glyph_button(
        "composer-plus",
        "+".to_string(),
        palette.text_secondary,
        None,
        ComposerAction::OpenPlusMenu,
    ));

    // 细竖线分隔（`1px` hairline）。
    row = row.child(
        div()
            .w(px(1.0))
            .h(px(sizing::ICON_MD_PX))
            .bg(palette.border_hairline),
    );

    // 常驻执行权限按钮：纯图标（宽版附中文标签），色调随风险档位变化。
    let permission = &state.permission;
    let permission_color = tone_color(permission.meta().tone, palette);
    let mut permission_button = div()
        .id(ElementId::Name(SharedString::from("composer-permission")))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(spacing::SPACE_1_PX))
        .h(px(28.0))
        .px(px(spacing::SPACE_1_5_PX))
        .rounded(px(radius::RADIUS_SM_PX))
        .text_size(px(typography::TEXT_CAPTION_PX))
        .text_color(permission_color)
        .on_click(move |_event, _window, _cx| {
            ActionSink::push(ComposerAction::CyclePermission);
        })
        .child(permission.glyph().to_string());

    if permission.show_label(&geometry) {
        permission_button = permission_button.child(permission.meta().label.to_string());
    }
    row = row.child(permission_button);

    // `@` 移交入口：选中后仅切换按钮自身选中态样式，图标与文字保持不变。
    let handoff_active = state.handoff.is_active();
    let handoff_color = if handoff_active {
        palette.semantic_handoff
    } else {
        palette.text_secondary
    };
    let mut handoff_button = div()
        .id(ElementId::Name(SharedString::from("composer-handoff")))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(spacing::SPACE_1_PX))
        .h(px(28.0))
        .px(px(spacing::SPACE_1_5_PX))
        .rounded(px(radius::RADIUS_SM_PX))
        .text_size(px(typography::TEXT_CAPTION_PX))
        .text_color(handoff_color)
        .on_click(move |_event, _window, _cx| {
            ActionSink::push(ComposerAction::OpenHandoffPanel);
        })
        .child("@".to_string());

    if handoff_active {
        handoff_button = handoff_button
            .bg(with_alpha(palette.semantic_handoff, 0.14))
            .border_1()
            .border_color(with_alpha(palette.semantic_handoff, 0.38))
            .child("移交".to_string());
        if let Some(capsule) = state.handoff.capsule_label() {
            handoff_button = handoff_button.child(capsule);
        }
    }
    row = row.child(handoff_button);

    // 投递模式纯图标分段控件（热纠偏 / 排队 / 打断重发），不附带按钮内文字。
    let mut behavior_group = div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(spacing::SPACE_HALF_PX))
        .px(px(spacing::SPACE_HALF_PX))
        .h(px(28.0))
        .rounded(px(radius::RADIUS_SM_PX))
        .bg(palette.surface_control);
    for (index, glyph) in BEHAVIOR_GLYPHS.iter().enumerate() {
        let color = match index {
            0 => palette.semantic_running,
            2 => palette.semantic_danger,
            _ => palette.text_secondary,
        };
        behavior_group = behavior_group.child(
            div()
                .w(px(22.0))
                .h(px(22.0))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(typography::TEXT_MICRO_PX))
                .text_color(color)
                .child((*glyph).to_string()),
        );
    }
    row = row.child(behavior_group);

    // 右侧发送键：白色实心圆 + 黑色字形，禁用态使用禁用令牌。
    let (glyph, background, foreground) = send_button_look(state, palette);
    row = row.child(
        div()
            .id(ElementId::Name(SharedString::from("composer-send")))
            .flex()
            .items_center()
            .justify_center()
            .w(px(28.0))
            .h(px(28.0))
            .rounded(px(radius::RADIUS_PILL_PX))
            .bg(background)
            .text_size(px(typography::TEXT_CAPTION_PX))
            .text_color(foreground)
            .child(glyph),
    );

    row
}

/// 发送键三态外观：返回（字形、底色、前景色）。
fn send_button_look(state: &ComposerState, palette: &Palette) -> (String, Hsla, Hsla) {
    match state.phase {
        TurnLifecycleState::Preparing => (
            PREPARING_GLYPH.to_string(),
            palette.action_disabled_bg,
            palette.semantic_running,
        ),
        TurnLifecycleState::Running if state.is_draft_empty() => (
            STOP_GLYPH.to_string(),
            palette.action_inverse,
            palette.action_on_inverse,
        ),
        _ if state.is_draft_empty() => (
            SEND_GLYPH.to_string(),
            palette.action_disabled_bg,
            palette.action_disabled_fg,
        ),
        _ => (
            SEND_GLYPH.to_string(),
            palette.action_inverse,
            palette.action_on_inverse,
        ),
    }
}

/// 底部遥测微脚条：左快捷键提示，右三色圆环 + 费用/速率读数。
fn footer_row(state: &ComposerState, geometry: ComposerGeometry, palette: &Palette) -> Div {
    let ring = state.usage_ring();
    let telemetry_empty = state.telemetry().is_empty();
    let readout = FooterReadout::from_stats(&state.stats, geometry.content_width < 420.0);
    let ring_color = band_color(ring.band, palette);

    let mut right = div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(spacing::SPACE_2_PX))
        .text_size(px(typography::TEXT_MICRO_PX));

    right = right.child(usage_ring_element(ring, palette)).child(
        div()
            .text_color(if telemetry_empty {
                palette.text_tertiary
            } else {
                ring_color
            })
            .child(if telemetry_empty {
                EMPTY_TELEMETRY_LABEL.to_string()
            } else {
                ring.label()
            }),
    );

    if !telemetry_empty {
        let mut readout_text = readout.text();
        if geometry.content_width < 640.0 {
            // 窄屏仅保留速率，避免脚条折行。
            readout_text = readout.speed.clone();
        }
        right = right.child(
            div()
                .text_color(palette.semantic_success)
                .child(readout_text),
        );
    }

    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap(px(spacing::SPACE_2_PX))
        .px(px(spacing::SPACE_3_PX))
        .py(px(spacing::SPACE_1_5_PX))
        .border_t_1()
        .border_color(palette.border_hairline)
        .text_size(px(typography::TEXT_MICRO_PX))
        .text_color(palette.text_tertiary)
        .child(FOOTER_HINT.to_string())
        .child(right)
}

/// 输入舱外框（`#212121` / 22px 圆角 / `1px` 常态描边）。
fn composer_frame(state: &ComposerState, geometry: ComposerGeometry, palette: &Palette) -> Div {
    let mut frame = div()
        .flex()
        .flex_col()
        .w(px(geometry.content_width))
        .bg(palette.surface_composer)
        .border_1()
        .border_color(palette.border_subtle)
        .rounded(px(radius::RADIUS_COMPOSER_PX));

    if !state.chips.is_empty() {
        frame = frame.child(chip_row(state, palette));
    }
    frame
        .child(input_area(state, palette))
        .child(control_row(state, geometry, palette))
        .child(footer_row(state, geometry, palette))
}

/// 变更摘要浮药丸（居中，点击聚焦 Diff）。
fn change_pill(state: &ComposerState, palette: &Palette) -> impl IntoElement {
    div()
        .id(ElementId::Name(SharedString::from("composer-change-pill")))
        .flex()
        .items_center()
        .h(px(28.0))
        .px(px(spacing::SPACE_3_PX))
        .rounded(px(radius::RADIUS_PILL_PX))
        .bg(palette.surface_floating)
        .border_1()
        .border_color(palette.border_strong)
        .text_size(px(typography::TEXT_MICRO_PX))
        .text_color(palette.text_secondary)
        .on_click(move |_event, _window, _cx| {
            ActionSink::push(ComposerAction::FocusDiff);
        })
        .child(state.changes.pill_label())
}

/// 渲染输入舱（纯读取快照，不做任何 I/O；见 `MASTER.md` §4.1）。
pub fn render(state: &ComposerState, geometry: ComposerGeometry, palette: &Palette) -> Div {
    if geometry.is_collapsed() {
        return div();
    }

    let mut column = div()
        .flex()
        .flex_col()
        .items_center()
        .w_full()
        .gap(px(spacing::SPACE_1_5_PX))
        .py(px(spacing::SPACE_2_PX));

    if !state.changes.is_empty() {
        column = column.child(change_pill(state, palette));
    }
    column.child(composer_frame(state, geometry, palette))
}

/// 壳层接线入口：按窗口宽度与布局计划落图空态输入舱。
///
/// 真实分栏拖拽宽度在 M5 分栏接线下沉后由持有栏宽状态的视图调用
/// [`render`] 注入 [`ComposerGeometry`]；本入口只服务当前壳层（极薄接线点）。
pub fn render_for_window(
    plan: &LayoutPlan,
    palette: &Palette,
    window_width: f32,
) -> impl IntoElement {
    let geometry = ComposerGeometry::for_default_panels(window_width, plan);
    render(&ComposerState::placeholder(), geometry, palette)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 空态：无变更时药丸不可点，权限为默认档位，遥测为空态。
    #[test]
    fn placeholder_state_is_empty() {
        let state = ComposerState::placeholder();
        assert_eq!(state.phase, TurnLifecycleState::Idle);
        assert_eq!(state.permission.current(), PermissionLevel::default());
        assert!(state.changes.is_empty());
        assert!(state.chips.is_empty());
        assert!(!state.handoff.is_active());
        assert!(state.telemetry().is_empty());
        assert!(state.is_draft_empty());
        assert!(state.activate_change_pill().is_none());
    }

    /// 有变更时点击药丸记录聚焦 Diff 动作，并映射到 `ReviewDiff` 焦点层。
    #[test]
    fn change_pill_records_focus_diff_action() {
        let _guard = crate::composer::changes::sink_test_lock();
        ActionSink::clear();
        let mut state = ComposerState::placeholder();
        state.changes = ChangeSummary::new(3, 47, 4);
        state.handoff.set_diff_file_count(3);

        let action = state.activate_change_pill().expect("有变更时药丸应可点");
        assert_eq!(action, ComposerAction::FocusDiff);
        assert_eq!(
            action.focus_target(),
            Some(crate::focus::FocusLayer::ReviewDiff)
        );
        assert_eq!(ActionSink::snapshot(), vec![ComposerAction::FocusDiff]);
        ActionSink::clear();
    }

    /// 权限按钮循环：6 档按顺序推进并逐次记录动作。
    #[test]
    fn permission_button_cycles_and_records() {
        let _guard = crate::composer::changes::sink_test_lock();
        ActionSink::clear();
        let mut state = ComposerState::placeholder();
        assert_eq!(
            state.cycle_permission(CycleDirection::Forward),
            PermissionLevel::AutoEdit
        );
        assert_eq!(
            state.cycle_permission(CycleDirection::Backward),
            PermissionLevel::Ask
        );
        assert_eq!(
            ActionSink::snapshot(),
            vec![
                ComposerAction::CyclePermission,
                ComposerAction::CyclePermission
            ]
        );
        ActionSink::clear();
    }

    /// 草稿非空但只有空白字符时仍视为空。
    #[test]
    fn whitespace_draft_is_empty() {
        let mut state = ComposerState::placeholder();
        state.set_draft("   \n\t ");
        assert!(state.is_draft_empty());
        state.set_draft("跑完单测后更新认知索引");
        assert!(!state.is_draft_empty());
        assert_eq!(
            state.handle_key(&KeyStroke::enter()),
            KeyOutcome::Submit(SubmitIntent::NewTurn)
        );
    }

    /// IME 组合期的 Enter 在状态层同样不提交。
    #[test]
    fn ime_enter_does_not_submit_from_state() {
        let mut state = ComposerState::placeholder();
        state.set_draft("中文");
        assert_eq!(
            state.handle_key(&KeyStroke::ime_enter()),
            KeyOutcome::ConsumedByIme
        );
        assert_eq!(
            state.handle_key(&KeyStroke::shift_enter()),
            KeyOutcome::InsertNewline
        );
    }

    /// 芯片移除记录动作；不存在时返回 false 且不记录。
    #[test]
    fn removing_chip_records_action() {
        let _guard = crate::composer::changes::sink_test_lock();
        ActionSink::clear();
        let mut state = ComposerState::placeholder();
        state
            .chips
            .push(code_chip("chip-1", "crates/soxu-core/src/lib.rs", 1, 9));

        assert!(state.remove_chip("chip-1"));
        assert!(!state.remove_chip("chip-1"));
        assert_eq!(
            ActionSink::snapshot(),
            vec![ComposerAction::RemoveChip("chip-1".to_string())]
        );
        ActionSink::clear();
    }

    /// 移交范围微开关翻转记录动作并改变范围契约。
    #[test]
    fn handoff_scope_toggle_records_action() {
        let _guard = crate::composer::changes::sink_test_lock();
        ActionSink::clear();
        let mut state = ComposerState::placeholder();
        state.handoff.select(soxu_protocol::HandoffSelection {
            target_agent: "codex".to_string(),
            target_model: "gpt-6.2-sol".to_string(),
            target_thinking: None,
        });

        assert_eq!(
            state.toggle_handoff_scope(HandoffScopeToggle::SpawnWorktree),
            "spawn_worktree"
        );
        assert!(state.handoff.scope().spawn_worktree);
        assert_eq!(
            ActionSink::snapshot(),
            vec![ComposerAction::ToggleHandoffScope("spawn_worktree")]
        );
        ActionSink::clear();
    }

    /// 几何联动：同一窗口宽度下输入舱与对话流取值完全一致，且钳制 760px。
    #[test]
    fn geometry_is_synchronized_with_transcript() {
        let wide = LayoutPlan::for_width(1600.0);
        let geometry = ComposerState::placeholder().geometry(1600.0, &wide);
        assert_eq!(geometry.content_width, SYNC_CONTENT_MAX_WIDTH_PX);
        assert_eq!(
            geometry.content_width,
            metrics::synchronized_content_width(
                1600.0,
                &wide,
                crate::tokens::layout::SIDEBAR_DEFAULT_PX,
                crate::tokens::layout::INSPECTOR_DEFAULT_PX,
            )
        );

        let narrow = LayoutPlan::for_width(820.0);
        let narrow_geometry = ComposerState::placeholder().geometry(820.0, &narrow);
        assert_eq!(narrow_geometry.content_width, 788.0_f32.min(760.0));
    }

    /// 遥测为空时圆环为绿档空态，不为空时按分档着色。
    #[test]
    fn ring_follows_telemetry() {
        let mut state = ComposerState::placeholder();
        assert_eq!(state.usage_ring().band, ContextUsageBand::SafeGreen);
        assert!(state.usage_ring().is_empty());

        state.stats.context_window_limit = 100_000;
        state.stats.used_context_tokens = 88_000;
        state.stats.turns = 1;
        assert_eq!(state.usage_ring().band, ContextUsageBand::DangerRed);
        assert_eq!(state.usage_ring().label(), "88.0%");
        assert_eq!(state.telemetry().len(), 11);
    }

    /// 斜杠命令过滤在状态层可用（纯内存，不触发 I/O 动作）。
    #[test]
    fn slash_command_filter_available_without_actions() {
        let _guard = crate::composer::changes::sink_test_lock();
        ActionSink::clear();
        let hits = filter_slash_commands("/pl");
        assert_eq!(hits.first().map(|c| c.name), Some("/plan"));
        assert!(ActionSink::snapshot().is_empty());

        let mut state = ComposerState::placeholder();
        state.run_slash_command("/plan");
        assert_eq!(
            ActionSink::snapshot(),
            vec![ComposerAction::RunSlashCommand("/plan")]
        );
        ActionSink::clear();
    }
}
