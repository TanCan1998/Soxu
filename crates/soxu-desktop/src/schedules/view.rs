//! 调度中心的轻量视图占位：标题栏入口与列表占位卡，不执行 RPC 或 I/O。

use gpui::{Div, Hsla, ParentElement, Styled, div, px};

use crate::schedules::SCHEDULE_HIT_TARGET_MIN_PX;
use crate::theme::gpui_colors::Palette;
use crate::tokens::{radius, spacing, typography};

/// 标题栏调度中心入口占位（44px 最小热区，后续接入真实面板交互）。
pub fn render_trigger(palette: &Palette) -> Div {
    div()
        .flex()
        .items_center()
        .justify_center()
        .h(px(SCHEDULE_HIT_TARGET_MIN_PX))
        .min_w(px(SCHEDULE_HIT_TARGET_MIN_PX))
        .px(px(spacing::SPACE_2_PX))
        .rounded(px(radius::RADIUS_SM_PX))
        .text_size(px(typography::TEXT_CAPTION_PX))
        .text_color(palette.text_secondary)
        .child("调度".to_owned())
}

/// 列表占位条：展示名称、下次触发与最近结果概述。
pub fn render_row(name: &str, next_run: &str, latest_result: &str, palette: &Palette) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(spacing::SPACE_1_PX))
        .min_h(px(SCHEDULE_HIT_TARGET_MIN_PX))
        .px(px(spacing::SPACE_3_PX))
        .py(px(spacing::SPACE_2_PX))
        .border_b_1()
        .border_color(palette.border_hairline)
        .text_color(palette.text_primary)
        .child(
            div()
                .text_size(px(typography::TEXT_BODY_PX))
                .child(name.to_owned()),
        )
        .child(
            div()
                .text_size(px(typography::TEXT_MICRO_PX))
                .text_color(palette.text_tertiary)
                .child(format!("下次：{next_run} · 最近：{latest_result}")),
        )
}

/// 最近失败原因占位文本颜色。
pub fn error_color(palette: &Palette) -> Hsla {
    palette.semantic_danger
}
