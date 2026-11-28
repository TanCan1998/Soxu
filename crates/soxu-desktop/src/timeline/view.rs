//! 时间线视图（GPUI 渲染层）：虚拟滚动对话流 + 结构化卡片行 + 20px 轮次导航轨。
//!
//! 渲染线程零阻塞铁律：`render` 只读内存快照（[`TimelineSnapshot`]），
//! 无文件系统、SQLite、Git 或子进程调用；行高度量与轮次偏移在快照构建时算好，
//! 每帧只做 `O(log n)` 的可视区间查询（[`crate::timeline::window`]）。
//!
//! 交互全部落在纯逻辑层并有单测覆盖：滚动（[`TimelineView::scroll_by`]）、
//! 轮次跳转（[`TimelineView::jump_to_turn`]）、行展开（[`TimelineView::toggle_row`]）、
//! 合批提交（[`TimelineView::commit_due`]）。视图层只负责把状态画出来。

use std::collections::HashMap;

use gpui::prelude::*;
use gpui::{ClickEvent, Context, Div, Hsla, Render, ScrollDelta, ScrollEvent, Window, div, px};

use crate::layout::LayoutPlan;
use crate::theme::gpui_colors::{self, Palette};
use crate::theme::theme_state;
use crate::theme::tokens::{radius, sizing, spacing};
use crate::timeline::cadence::{DeltaKind, StreamCadence};
use crate::timeline::focus_scope::CardFocusScope;
use crate::timeline::model::{
    BODY_LINE_HEIGHT_PX, LIVE_STEPS_HEIGHT_PX, RowId, RowTone, TURN_GAP_PX, TimelineRow,
    TimelineRowKind, TimelineSnapshot,
};
use crate::timeline::{cards, rail, text, window};

/// 中栏可用高度 = 窗口高度 − Titlebar − Statusbar。
fn canvas_viewport_height(window_height: f32) -> f32 {
    (window_height - sizing::TITLEBAR_HEIGHT_PX - sizing::STATUSBAR_HEIGHT_PX).max(0.0)
}

/// 行语义色 → GPUI 颜色。
fn tone_color(palette: &Palette, tone: RowTone) -> Hsla {
    gpui_colors::solid_hsla(if palette.is_dark {
        tone.dark_token()
    } else {
        tone.light_token()
    })
}

/// 导航轨刻度色 → GPUI 颜色。
fn tick_color(palette: &Palette, tone: rail::TickTone) -> Hsla {
    gpui_colors::solid_hsla(if palette.is_dark {
        tone.dark_token()
    } else {
        tone.light_token()
    })
}

/// 主对话画布时间线视图。
///
/// 由 `shell` 以 `cx.new` 创建并挂载；M5-B 共享 `shell.rs` 时只需换挂载点，不需要
/// 触碰本模块内部状态。
pub struct TimelineView {
    /// 渲染就绪的对话流快照。
    snapshot: TimelineSnapshot,
    /// 流式合批节拍器（`<= 8.3Hz`）。
    cadence: StreamCadence,
    /// 滚动偏移（px）。
    scroll_offset: f32,
    /// 最近一次渲染时的中栏可用高度（px）。
    viewport_height: f32,
    /// 逐行展开态（只存与快照默认值不同的行）。
    expanded: HashMap<RowId, bool>,
    /// 导航轨悬停轮次。
    hovered_turn: Option<usize>,
    /// 当前可视活跃轮次。
    active_turn: Option<usize>,
    /// 导航轨焦点作用域（卡片内焦点不逃逸）。
    rail_focus: CardFocusScope,
}

impl TimelineView {
    /// 创建空时间线视图（壳层挂载用）。
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Self {
            snapshot: TimelineSnapshot::default(),
            cadence: StreamCadence::new(),
            scroll_offset: 0.0,
            viewport_height: 0.0,
            expanded: HashMap::new(),
            hovered_turn: None,
            active_turn: None,
            rail_focus: CardFocusScope::composer_level(0),
        }
    }

    /// 当前快照（只读）。
    pub fn snapshot(&self) -> &TimelineSnapshot {
        &self.snapshot
    }

    /// 当前滚动偏移（px）。
    pub fn scroll_offset(&self) -> f32 {
        self.scroll_offset
    }

    /// 最近一次渲染记录的中栏可用高度（px）。
    pub fn viewport_height(&self) -> f32 {
        self.viewport_height
    }

    /// 当前可视活跃轮次下标。
    pub fn active_turn(&self) -> Option<usize> {
        self.active_turn
    }

    /// 合批节拍器（只读）。
    pub fn cadence(&self) -> &StreamCadence {
        &self.cadence
    }

    /// 写入新快照。
    ///
    /// - 与当前快照指针相同（`Rc::ptr_eq`）时直接返回 `false`，**不触发重绘**；
    /// - 原本贴底时保持尾部跟随，否则保持当前滚动位置（流式追加不推动视口）；
    /// - 会话重置（`reset_generation` 变化）时清空行展开态缓存。
    pub fn apply_snapshot(&mut self, snapshot: TimelineSnapshot, cx: &mut Context<Self>) -> bool {
        if self.snapshot == snapshot {
            return false;
        }
        let viewport = self.viewport_height;
        let follow_tail = window::is_at_bottom(
            self.scroll_offset,
            self.snapshot.metrics().total_height(),
            viewport,
        );
        let reset = self.snapshot.reset_generation() != snapshot.reset_generation();
        let total = snapshot.metrics().total_height();
        self.snapshot = snapshot;
        self.scroll_offset =
            window::offset_after_append(self.scroll_offset, follow_tail, total, viewport);
        if reset {
            self.expanded.clear();
            self.hovered_turn = None;
        }
        self.rail_focus
            .set_target_count(self.snapshot.turns().len());
        cx.notify();
        true
    }

    /// 记录一条流式增量（不重绘）。
    pub fn observe_delta(&mut self, at_ms: u64, kind: DeltaKind, bytes: usize) {
        self.cadence.observe(at_ms, kind, bytes);
    }

    /// 到达量化窗口边界时提交一次：返回是否真的产生了一次提交（调用方据此 `notify`）。
    pub fn commit_due(&mut self, now_ms: u64) -> bool {
        self.cadence.try_commit(now_ms).is_some()
    }

    /// 记录中栏可用高度（渲染时更新）。
    pub fn set_viewport_height(&mut self, height: f32) {
        self.viewport_height = height.max(0.0);
    }

    /// 按像素滚动；返回是否发生变化。
    pub fn scroll_by(&mut self, delta: f32) -> bool {
        let total = self.snapshot.metrics().total_height();
        let next = window::clamped_offset(self.scroll_offset + delta, total, self.viewport_height);
        if (next - self.scroll_offset).abs() <= f32::EPSILON {
            return false;
        }
        self.scroll_offset = next;
        true
    }

    /// 跳转到底部。
    pub fn jump_to_bottom(&mut self) -> bool {
        let total = self.snapshot.metrics().total_height();
        let next = window::offset_after_append(0.0, true, total, self.viewport_height);
        if (next - self.scroll_offset).abs() <= f32::EPSILON {
            return false;
        }
        self.scroll_offset = next;
        true
    }

    /// 跳转到第 `index` 轮顶部；越界返回 `false` 且保持原位置。
    pub fn jump_to_turn(&mut self, index: usize) -> bool {
        let total = self.snapshot.metrics().total_height();
        let Some(next) = rail::jump_offset(
            self.snapshot.turn_tops(),
            index,
            self.viewport_height,
            total,
        ) else {
            return false;
        };
        let changed = (next - self.scroll_offset).abs() > f32::EPSILON;
        self.scroll_offset = next;
        self.active_turn = Some(index);
        changed
    }

    /// 切换某行的展开态；返回新的展开态。
    pub fn toggle_row(&mut self, id: RowId) -> bool {
        let current = self
            .snapshot
            .rows()
            .iter()
            .find(|row| row.id == id)
            .map(|row| self.is_row_expanded(row))
            .unwrap_or(false);
        let next = !current;
        self.expanded.insert(id, next);
        next
    }

    /// 某行当前是否展开（行自身默认值 + 视图覆盖）。
    pub fn is_row_expanded(&self, row: &TimelineRow) -> bool {
        self.expanded.get(&row.id).copied().unwrap_or(row.expanded)
    }

    /// 设置导航轨悬停轮次。
    pub fn set_hovered_turn(&mut self, index: Option<usize>) {
        self.hovered_turn = index;
    }

    /// 导航轨焦点作用域（只读）。
    pub fn rail_focus(&self) -> &CardFocusScope {
        &self.rail_focus
    }

    /// 点击导航轨第 `index` 条刻度：跳转并重绘。返回是否发生跳转。
    pub fn on_rail_tick_clicked(&mut self, index: usize, cx: &mut Context<Self>) -> bool {
        if self.jump_to_turn(index) {
            self.hovered_turn = Some(index);
            cx.notify();
            return true;
        }
        false
    }

    /// 滚轮事件：换算为像素偏移并滚动。
    fn on_scroll(&mut self, event: &ScrollEvent, cx: &mut Context<Self>) {
        let delta = match event.delta {
            ScrollDelta::Pixels(point) => -point.y.as_f32(),
            ScrollDelta::Lines(point) => -point.y * BODY_LINE_HEIGHT_PX,
        };
        if self.scroll_by(delta) {
            cx.notify();
        }
    }

    /// 单行渲染：按行类别套用 03 页规定的卡片骨架。
    fn row_element(
        &self,
        palette: &Palette,
        row: &TimelineRow,
        measure: f32,
        cx: &mut Context<Self>,
    ) -> Div {
        let expanded = self.is_row_expanded(row);
        let accent = tone_color(palette, row.tone);
        let base = div().w_full().flex().flex_col();

        match row.kind {
            TimelineRowKind::UserMessage => base.items_end().child(
                div()
                    .max_w(px(measure * 0.78))
                    .px(px(spacing::SPACE_4_PX))
                    .py(px(spacing::SPACE_3_PX))
                    .rounded(px(radius::RADIUS_LG_PX))
                    .bg(palette.surface_raised)
                    .border_1()
                    .border_color(palette.border_subtle)
                    .text_size(px(14.0))
                    .text_color(palette.text_primary)
                    .child(row.title.clone()),
            ),
            TimelineRowKind::AssistantMessage => base
                .child(
                    div()
                        .flex()
                        .items_center()
                        .h(px(22.0))
                        .gap(px(spacing::SPACE_2_PX))
                        .text_size(px(12.0))
                        .text_color(palette.text_tertiary)
                        .child(row.title.clone()),
                )
                .when_some(row.subtitle.clone(), |column, subtitle| {
                    column.child(
                        div()
                            .text_size(px(14.0))
                            .text_color(palette.text_primary)
                            .child(subtitle),
                    )
                }),
            TimelineRowKind::Checkpoint => base.items_center().child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(spacing::SPACE_2_PX))
                    .px(px(spacing::SPACE_3_PX))
                    .h(px(crate::timeline::checkpoint::ANCHOR_HEIGHT_PX))
                    .rounded(px(radius::RADIUS_PILL_PX))
                    .bg(palette.surface_card_muted)
                    .border_1()
                    .border_color(palette.border_subtle)
                    .text_size(px(11.5))
                    .text_color(palette.text_secondary)
                    .child("⊶")
                    .child(row.title.clone()),
            ),
            TimelineRowKind::LiveSteps => base.child(
                div()
                    .flex()
                    .items_center()
                    .px(px(spacing::SPACE_3_PX))
                    .h(px(LIVE_STEPS_HEIGHT_PX))
                    .rounded(px(radius::RADIUS_SM_PX))
                    .bg(palette.surface_card_muted)
                    .text_size(px(12.0))
                    .text_color(palette.text_secondary)
                    .child(row.title.clone()),
            ),
            TimelineRowKind::ToolCard
            | TimelineRowKind::ApprovalGate
            | TimelineRowKind::AskQuestion => {
                let row_id = row.id;
                let clickable = div()
                    .id(("timeline-card", row_id as usize))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _event: &ClickEvent, _window, cx| {
                        this.toggle_row(row_id);
                        cx.notify();
                    }));
                let mut card = div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .bg(palette.surface_card)
                    .rounded(px(radius::RADIUS_MD_PX))
                    .border_1()
                    .border_color(accent)
                    .px(px(spacing::SPACE_3_PX))
                    .py(px(spacing::SPACE_2_PX));
                card = card.child(
                    clickable
                        .flex()
                        .items_center()
                        .gap(px(spacing::SPACE_2_PX))
                        .h(px(cards::CARD_FOLDED_HEIGHT_PX))
                        .text_size(px(12.5))
                        .text_color(palette.text_primary)
                        .child(if expanded { "▾" } else { "▸" })
                        .child(row.title.clone()),
                );
                if expanded && row.body_lines > 0 {
                    card = card.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(spacing::SPACE_1_PX))
                            .pb(px(spacing::SPACE_2_PX))
                            .text_size(px(12.0))
                            .text_color(palette.text_secondary)
                            .child(
                                row.subtitle
                                    .clone()
                                    .unwrap_or_else(|| format!("{} 行详情", row.body_lines)),
                            ),
                    );
                }
                // 400 行 Diff 上限：展开后仍超限时给出「展开剩余 N 行」提示。
                if expanded && row.body_lines > cards::DIFF_PREVIEW_MAX_LINES {
                    card = card.child(
                        div()
                            .text_size(px(11.5))
                            .text_color(palette.text_tertiary)
                            .child(text::diff_expand_label(
                                row.body_lines - cards::DIFF_PREVIEW_MAX_LINES,
                            )),
                    );
                }
                base.items_start().child(card)
            }
        }
    }
}

impl Render for TimelineView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = theme_state::current(cx).palette();
        let size = window.viewport_size();
        let width = size.width.as_f32();
        let available = canvas_viewport_height(size.height.as_f32());
        self.set_viewport_height(available);
        let plan = LayoutPlan::for_width(width);
        let measure = plan.canvas_max_width(width).max(0.0);

        let (virtual_window, at_bottom) = {
            let metrics = self.snapshot.metrics();
            let total = metrics.total_height();
            let math = window::for_metrics(metrics, available, self.scroll_offset);
            (
                math,
                window::is_at_bottom(self.scroll_offset, total, available),
            )
        };
        self.scroll_offset = virtual_window.offset;
        self.active_turn =
            rail::active_turn_index(self.snapshot.turn_tops(), self.scroll_offset, at_bottom);
        let rail_layout = rail::layout(self.snapshot.turns().len(), available);

        // 空态：与 M4 壳层保持一致的中栏引导页。
        if self.snapshot.rows().is_empty() {
            return div()
                .flex()
                .flex_row()
                .flex_1()
                .min_h_0()
                .min_w_0()
                .items_center()
                .justify_center()
                .bg(palette.surface_canvas)
                .child(
                    div()
                        .text_size(px(17.0))
                        .text_color(palette.text_secondary)
                        .child(text::CANVAS_EMPTY),
                );
        }

        // 对话列：限宽 760px，上下占位撑开滚动高度（虚拟列表）。
        let mut column = div().flex().flex_col().w(px(measure)).gap(px(TURN_GAP_PX));
        if let Some(label) = self.snapshot.live_steps() {
            column = column.child(
                div()
                    .flex()
                    .items_center()
                    .px(px(spacing::SPACE_3_PX))
                    .h(px(LIVE_STEPS_HEIGHT_PX))
                    .rounded(px(radius::RADIUS_SM_PX))
                    .bg(palette.surface_card_muted)
                    .text_size(px(12.0))
                    .text_color(palette.text_secondary)
                    .child(format!("● {label}")),
            );
        }
        let mut body = div().flex().flex_col().w_full().gap(px(TURN_GAP_PX));
        if virtual_window.top_spacer > 0.0 {
            body = body.child(div().h(px(virtual_window.top_spacer)));
        }
        let rows = self.snapshot.rows();
        let start = virtual_window.first_row.min(rows.len());
        let end = virtual_window.end_row().min(rows.len());
        if let Some(window_rows) = rows.get(start..end) {
            for row in window_rows {
                body = body.child(self.row_element(&palette, row, measure, cx));
            }
        }
        if virtual_window.bottom_spacer > 0.0 {
            body = body.child(div().h(px(virtual_window.bottom_spacer)));
        }
        column = column.child(body);

        // 轮次导航轨：20px，右侧贴边，不挤占阅读列。
        let mut rail_ticks = div()
            .flex()
            .flex_col()
            .items_center()
            .w_full()
            .h(px(rail_layout.height));
        for (index, turn) in self.snapshot.turns().iter().enumerate() {
            let flags = rail::TurnFlags::of(turn);
            let (tone, tick_width) =
                rail::tick_style(index, self.active_turn, self.hovered_turn, flags);
            let color = tick_color(&palette, tone);
            rail_ticks = rail_ticks.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_end()
                    .w_full()
                    .h(px(rail_layout.slot))
                    .child(
                        div()
                            .id(("timeline-rail-tick", index))
                            .cursor_pointer()
                            .w(px(tick_width))
                            .h(px(rail_layout.tick_height))
                            .rounded(px(1.5))
                            .bg(color)
                            .on_click(cx.listener(
                                move |this, _event: &ClickEvent, _window, cx| {
                                    this.on_rail_tick_clicked(index, cx);
                                },
                            )),
                    ),
            );
        }

        div()
            .flex()
            .flex_row()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .bg(palette.surface_canvas)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .items_center()
                    .overflow_hidden()
                    .on_scroll(cx.listener(|this, event: &ScrollEvent, _window, cx| {
                        this.on_scroll(event, cx);
                    }))
                    .child(column),
            )
            .when(rail_layout.visible, |root| {
                root.child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .w(px(rail::RAIL_WIDTH_PX))
                        .min_w(px(rail::RAIL_WIDTH_PX))
                        .h_full()
                        .flex_shrink_0()
                        .child(rail_ticks),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeline::model::TurnSnapshot;

    fn view_with(rows: Vec<TimelineRow>, turns: Vec<TurnSnapshot>) -> TimelineView {
        // 测试不需要 GPUI 上下文：直接构造结构体字段（同模块可见）。
        let mut view = TimelineView {
            snapshot: TimelineSnapshot::default(),
            cadence: StreamCadence::new(),
            scroll_offset: 0.0,
            viewport_height: 300.0,
            expanded: HashMap::new(),
            hovered_turn: None,
            active_turn: None,
            rail_focus: CardFocusScope::composer_level(0),
        };
        view.snapshot = TimelineSnapshot::new(turns, rows, None);
        view.rail_focus
            .set_target_count(view.snapshot.turns().len());
        view
    }

    fn row(id: RowId, kind: TimelineRowKind, lines: usize) -> TimelineRow {
        TimelineRow::new(id, kind, format!("第 {id} 行"), lines)
    }

    #[test]
    fn viewport_height_subtracts_chrome() {
        assert_eq!(canvas_viewport_height(800.0), 800.0 - 48.0 - 30.0);
        // 极小窗口不出负值。
        assert_eq!(canvas_viewport_height(10.0), 0.0);
    }

    #[test]
    fn scrolling_clamps_and_reports_change() {
        let rows = (0..40)
            .map(|i| row(i as RowId, TimelineRowKind::ToolCard, 1))
            .collect();
        let mut view = view_with(rows, Vec::new());
        let total = view.snapshot().metrics().total_height();
        assert!(total > 300.0);

        // 向上滚动无变化（已在顶部）。
        assert!(!view.scroll_by(-50.0));
        assert_eq!(view.scroll_offset(), 0.0);
        // 向下滚动生效。
        assert!(view.scroll_by(120.0));
        assert_eq!(view.scroll_offset(), 120.0);
        // 超大滚动被钳制到 max_offset。
        assert!(view.scroll_by(99_999.0));
        let max_offset = total - view.viewport_height();
        assert!((view.scroll_offset() - max_offset).abs() < 1e-3);
        // 再向下滚动无变化。
        assert!(!view.scroll_by(10.0));
        // 已在底部：再跳底部无变化。
        assert!(!view.jump_to_bottom());
        // 回到顶部后跳底部生效。
        assert!(view.scroll_by(-99_999.0));
        assert!(view.jump_to_bottom());
    }

    #[test]
    fn jump_to_turn_uses_rail_geometry_and_rejects_out_of_range() {
        // 30 行 × 64px = 1920px；6 轮分别从第 0/5/10/15/20/29 行开始。
        let rows: Vec<TimelineRow> = (0..30)
            .map(|i| row(i as RowId, TimelineRowKind::UserMessage, 0))
            .collect();
        let first_rows = [0_usize, 5, 10, 15, 20, 29];
        let turns: Vec<TurnSnapshot> = first_rows
            .iter()
            .enumerate()
            .map(|(index, first_row)| {
                TurnSnapshot::new(
                    index as u64 + 1,
                    index,
                    format!("第 {index} 轮"),
                    *first_row,
                )
            })
            .collect();
        let mut view = view_with(rows, turns);

        let total = view.snapshot().metrics().total_height();
        assert_eq!(total, 30.0 * 64.0);
        let expected = rail::jump_offset(
            view.snapshot().turn_tops(),
            2,
            view.viewport_height(),
            total,
        );
        assert_eq!(expected, Some(10.0 * 64.0));
        assert!(view.jump_to_turn(2));
        assert_eq!(view.scroll_offset(), 640.0);
        assert_eq!(view.active_turn(), Some(2));

        // 末轮顶部超出 max_offset（1920 - 300 = 1620）→ 钳制。
        assert!(view.jump_to_turn(5));
        assert_eq!(view.scroll_offset(), 1620.0);
        assert_eq!(view.active_turn(), Some(5));

        // 越界轮次：保持原位置。
        let before = view.scroll_offset();
        assert!(!view.jump_to_turn(99));
        assert_eq!(view.scroll_offset(), before);
    }

    #[test]
    fn toggle_row_overrides_snapshot_default() {
        let base = row(1, TimelineRowKind::ToolCard, 528);
        let mut view = view_with(vec![base.clone()], Vec::new());
        assert!(!view.is_row_expanded(&base));
        assert!(view.toggle_row(1));
        assert!(view.is_row_expanded(&base));
        // 再切换回折叠。
        assert!(!view.toggle_row(1));
        assert!(!view.is_row_expanded(&base));
        // 未知行按折叠态处理，且可展开。
        let unknown = row(999, TimelineRowKind::ToolCard, 1);
        assert!(!view.is_row_expanded(&unknown));
        assert!(view.toggle_row(999));
    }

    #[test]
    fn stream_deltas_commit_at_most_once_per_window() {
        let mut view = view_with(Vec::new(), Vec::new());
        for at in 0..4_u64 {
            view.observe_delta(at, DeltaKind::Text, 16);
        }
        assert_eq!(view.cadence().pending().deltas, 4);
        assert!(view.commit_due(10));
        // 同一窗口内不重复提交，且没有新增量时为空提交（不重绘）。
        assert!(!view.commit_due(20));
        assert!(!view.commit_due(120));
        // 新窗口有新增量 → 允许提交。
        view.observe_delta(130, DeltaKind::ToolOutput, 32);
        assert!(view.commit_due(130));
    }
}
