//! 命令面板与标题栏触发条的薄渲染层（`pages/06` §二 + `pages/01` §1.1）。
//!
//! 渲染只能读内存状态（[`PaletteState`]）；本文件不做任何检索、不读时钟、不碰 I/O。
//! 尺寸全部来自 [`crate::search`] 的几何常量（源值为 06 页字面量或设计令牌），
//! 颜色全部来自 [`Palette`]（令牌编译产物），不新增任何色值。
//!
//! 三段骨架（06 页 §二）：
//! 1. 搜索输入区 `52px`（恒定持焦的输入条）；
//! 2. 分区结果列表：分区头 `26px` 大写 `text.tertiary`，行高 `40 / 54 / 48`；
//! 3. 底部键位提示栏 `36px`（`#171717` 即 `surface.card`）。

use gpui::{Div, ParentElement, Styled, div, px};

use crate::search::keymap::{FOOTER_HINTS, TRIGGER_PLACEHOLDER, open_chord_label};
use crate::search::model::SearchItem;
use crate::search::rank::SectionResults;
use crate::search::{
    EMPTY_STATE_HEIGHT_PX, FOOTER_HEIGHT_PX, PALETTE_INPUT_HEIGHT_PX, PALETTE_MAX_HEIGHT_PX,
    PALETTE_RADIUS_PX, PALETTE_WIDTH_PX, PaletteState, SECTION_HEADER_HEIGHT_PX, TRIGGER_HEIGHT_PX,
    row_height,
};
use crate::theme::gpui_colors::Palette;
use crate::tokens::{radius, spacing, typography};

/// 标题栏中央搜索触发条（`pages/01` §1.1：高 `32px`，恒定水平居中）。
///
/// 返回的容器自带 `flex_1 + justify_center`，因此壳层只需一行 `.child(...)`
/// 即可把它插进 48px 标题栏，居中由本模块负责，不改壳层布局代码。
pub fn render_trigger_bar(palette: &Palette, mac_like: bool) -> Div {
    div()
        .flex_1()
        .flex()
        .flex_row()
        .justify_center()
        .items_center()
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .gap(px(spacing::SPACE_2_PX))
                .h(px(TRIGGER_HEIGHT_PX))
                .w(px(rail_width()))
                .px(px(spacing::SPACE_3_PX))
                .rounded(px(radius::RADIUS_PILL_PX))
                .bg(palette.surface_raised)
                .border_1()
                .border_color(palette.border_subtle)
                .text_size(px(typography::TEXT_CAPTION_PX))
                .text_color(palette.text_tertiary)
                .child(TRIGGER_PLACEHOLDER.to_string())
                .child(
                    div()
                        .px(px(spacing::SPACE_1_PX))
                        .rounded(px(radius::RADIUS_XS_PX))
                        .bg(palette.surface_control)
                        .text_size(px(typography::TEXT_MICRO_PX))
                        .text_color(palette.text_secondary)
                        .child(open_chord_label(mac_like).to_string()),
                ),
        )
}

/// 触发条在标题栏里的宽度上限：不抢占左侧分支名与右侧工具坞。
fn rail_width() -> f32 {
    PALETTE_WIDTH_PX * 0.5
}

/// 完整命令面板（`640 × ≤440`，`rounded(16)`，`surface.floating` + `border.strong`）。
pub fn render_palette(state: &PaletteState, palette: &Palette) -> Div {
    let mut list = div().flex().flex_col().flex_1().min_h_0().overflow_hidden();
    for section in state.results().sections() {
        list = list.child(render_section(section, state.active_id(), palette));
    }
    if state.results().is_empty() {
        list = list.child(render_empty_state(palette));
    }

    div()
        .flex()
        .flex_col()
        .w(px(PALETTE_WIDTH_PX))
        .max_h(px(PALETTE_MAX_HEIGHT_PX))
        .rounded(px(PALETTE_RADIUS_PX))
        .bg(palette.surface_floating)
        .border_1()
        .border_color(palette.border_strong)
        .overflow_hidden()
        .child(render_input_area(state, palette))
        .child(list)
        .child(render_footer(palette))
}

/// 搜索输入区：`52px`，输入框恒定持焦（选中光标移动不夺焦）。
fn render_input_area(state: &PaletteState, palette: &Palette) -> Div {
    let has_text = state.has_query();
    let body = if has_text {
        state.query().to_string()
    } else {
        TRIGGER_PLACEHOLDER.to_string()
    };
    let color = if has_text {
        palette.text_primary
    } else {
        palette.text_ghost
    };

    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(spacing::SPACE_2_PX))
        .h(px(PALETTE_INPUT_HEIGHT_PX))
        .px(px(spacing::SPACE_4_PX))
        .border_b_1()
        .border_color(palette.border_hairline)
        .text_size(px(typography::TEXT_BODY_PX))
        .text_color(color)
        .child(body)
}

/// 单个分区：`26px` 大写分区头 + 若干行。
fn render_section(section: &SectionResults, active: Option<&str>, palette: &Palette) -> Div {
    let mut column = div().flex().flex_col().child(
        div()
            .flex()
            .flex_row()
            .items_center()
            .h(px(SECTION_HEADER_HEIGHT_PX))
            .px(px(spacing::SPACE_4_PX))
            .text_size(px(typography::TEXT_MICRO_PX))
            .text_color(palette.text_tertiary)
            .child(section.section().header().to_string()),
    );
    for item in section.items() {
        let selected = active == Some(item.id());
        column = column.child(render_row(item, selected, palette));
    }
    column
}

/// 单行：主标题 +（可选）摘要 + 右侧分量（文件行给路径）。
fn render_row(item: &SearchItem, selected: bool, palette: &Palette) -> Div {
    let background = if selected {
        palette.interaction_selected
    } else {
        palette.surface_floating
    };
    let title_color = if selected {
        palette.text_primary
    } else {
        palette.text_secondary
    };

    let mut text = div().flex().flex_col().flex_1().min_w_0().child(
        div()
            .text_size(px(typography::TEXT_BODY_PX))
            .text_color(title_color)
            .child(item.title().to_string()),
    );
    if let Some(subtitle) = item.subtitle() {
        text = text.child(
            div()
                .text_size(px(typography::TEXT_CAPTION_PX))
                .text_color(palette.text_tertiary)
                .child(subtitle.to_string()),
        );
    }

    div()
        .flex()
        .flex_row()
        .items_center()
        .h(px(row_height(item)))
        .px(px(spacing::SPACE_4_PX))
        .bg(background)
        .text_color(title_color)
        .child(text)
}

/// 搜索无结果空态卡（06 页 §五边界表：`160px`，提示修正关键词或新建会话）。
fn render_empty_state(palette: &Palette) -> Div {
    div()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(spacing::SPACE_1_PX))
        .h(px(EMPTY_STATE_HEIGHT_PX))
        .px(px(spacing::SPACE_6_PX))
        .text_size(px(typography::TEXT_CAPTION_PX))
        .text_color(palette.text_tertiary)
        .child("没有匹配结果".to_string())
        .child(
            div()
                .text_color(palette.text_ghost)
                .child("修正关键词，或 ↵ 新建会话".to_string()),
        )
}

/// 底部键位提示栏：`36px`，`surface.card` 底色，四组提示横向排布。
fn render_footer(palette: &Palette) -> Div {
    let mut footer = div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(spacing::SPACE_4_PX))
        .h(px(FOOTER_HEIGHT_PX))
        .px(px(spacing::SPACE_4_PX))
        .bg(palette.surface_card)
        .border_t_1()
        .border_color(palette.border_hairline)
        .text_size(px(typography::TEXT_MICRO_PX))
        .text_color(palette.text_tertiary);
    for hint in FOOTER_HINTS {
        footer = footer.child(hint.to_string());
    }
    footer
}
