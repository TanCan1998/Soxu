//! 三步引导卡片的薄渲染层（`pages/06` §一 + §5 边界表）。
//!
//! 只读内存状态与调用方传入的环境快照，不做探测、不读写文件、不请求 daemon。
//! 几何全部取自 [`crate::onboarding`] 常量（`560px` / `p(20)` / `rounded(18)`），
//! 颜色全部来自 [`Palette`] 令牌编译产物。

use gpui::{Div, ParentElement, Styled, div, px};

use crate::onboarding::{
    CARD_PADDING_PX, CARD_RADIUS_PX, CARD_WIDTH_PX, EcosystemSnapshot, OnboardingState,
    OnboardingStep, PAIRING_URL_PREFIX, PROJECT_CHOICES, PROMISE_BANNER, SHARED_TARGET_HINT,
    SHORTCUT_CARDS, STEP_COUNT,
};
use crate::theme::gpui_colors::Palette;
use crate::tokens::{radius, spacing, typography};

/// 进度条单段高度（`3px` 细条）。
const PROGRESS_BAR_HEIGHT_PX: f32 = 3.0;

/// 三步引导卡片。
pub fn render_card(
    state: &OnboardingState,
    ecosystem: &EcosystemSnapshot,
    palette: &Palette,
) -> Div {
    let step = state.step();
    div()
        .flex()
        .flex_col()
        .gap(px(spacing::SPACE_4_PX))
        .w(px(CARD_WIDTH_PX))
        .p(px(CARD_PADDING_PX))
        .rounded(px(CARD_RADIUS_PX))
        .bg(palette.surface_card)
        .border_1()
        .border_color(palette.border_strong)
        .text_color(palette.text_primary)
        .child(render_progress(state, palette))
        .child(render_header(state, palette))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(spacing::SPACE_2_PX))
                .child(
                    div()
                        .text_size(px(typography::TEXT_TITLE_PX))
                        .child(step.title().to_string()),
                )
                .child(
                    div()
                        .text_size(px(typography::TEXT_CAPTION_PX))
                        .text_color(palette.text_secondary)
                        .child(step.subtitle().to_string()),
                ),
        )
        .child(render_body(step, ecosystem, palette))
        .child(render_footer(step, palette))
}

/// 顶部 3 段进度条：已完成的段用强调色，未到达的段用弱描边色。
fn render_progress(state: &OnboardingState, palette: &Palette) -> Div {
    let filled = state.progress_filled();
    let mut row = div()
        .flex()
        .flex_row()
        .gap(px(spacing::SPACE_1_PX))
        .h(px(PROGRESS_BAR_HEIGHT_PX));
    for index in 0..STEP_COUNT {
        let color = if index < filled {
            palette.semantic_accent
        } else {
            palette.surface_control
        };
        row = row.child(
            div()
                .flex_1()
                .h(px(PROGRESS_BAR_HEIGHT_PX))
                .rounded(px(radius::RADIUS_PILL_PX))
                .bg(color),
        );
    }
    row
}

/// 左上「Step n / 3」+ 右上常驻「跳过引导」。
fn render_header(state: &OnboardingState, palette: &Palette) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .text_size(px(typography::TEXT_MICRO_PX))
        .text_color(palette.text_tertiary)
        .child(format!("Step {}", state.step_counter()))
        .child(
            div()
                .px(px(spacing::SPACE_2_PX))
                .py(px(spacing::SPACE_HALF_PX))
                .rounded(px(radius::RADIUS_XS_PX))
                .text_color(palette.text_secondary)
                .child("跳过引导".to_string()),
        )
}

/// 分步正文。
fn render_body(step: OnboardingStep, ecosystem: &EcosystemSnapshot, palette: &Palette) -> Div {
    match step {
        OnboardingStep::Ecosystem => render_ecosystem(ecosystem, palette),
        OnboardingStep::Project => render_project(palette),
        OnboardingStep::Pairing => render_pairing(palette),
    }
}

/// Step 1：Magpie 状态卡 + 已装 CLI 绿灯 + 只读承诺横幅。
fn render_ecosystem(ecosystem: &EcosystemSnapshot, palette: &Palette) -> Div {
    let magpie = ecosystem.magpie();
    let magpie_color = match magpie {
        crate::onboarding::MagpieStatus::Managed => palette.semantic_accent,
        crate::onboarding::MagpieStatus::StandaloneNative => palette.text_secondary,
    };

    let mut column = div().flex().flex_col().gap(px(spacing::SPACE_2_PX)).child(
        div()
            .px(px(spacing::SPACE_3_PX))
            .py(px(spacing::SPACE_2_PX))
            .rounded(px(radius::RADIUS_SM_PX))
            .bg(palette.surface_card_muted)
            .border_1()
            .border_color(palette.border_subtle)
            .text_size(px(typography::TEXT_CAPTION_PX))
            .text_color(magpie_color)
            .child(magpie.label().to_string()),
    );

    let clis = ecosystem.clis_in_order();
    if clis.is_empty() {
        column = column.child(
            div()
                .text_size(px(typography::TEXT_CAPTION_PX))
                .text_color(palette.text_ghost)
                .child("未探测到已安装的原生 CLI（可稍后在设置中重新探测）".to_string()),
        );
    } else {
        let mut row = div()
            .flex()
            .flex_row()
            .gap(px(spacing::SPACE_1_PX))
            .text_size(px(typography::TEXT_MICRO_PX))
            .text_color(palette.semantic_accent);
        for cli in clis {
            row = row.child(
                div()
                    .px(px(spacing::SPACE_1_5_PX))
                    .rounded(px(radius::RADIUS_PILL_PX))
                    .bg(palette.surface_control)
                    .child(cli.to_string()),
            );
        }
        column = column.child(row);
    }

    column.child(
        div()
            .p(px(spacing::SPACE_3_PX))
            .rounded(px(radius::RADIUS_SM_PX))
            .bg(palette.interaction_selection)
            .text_size(px(typography::TEXT_CAPTION_PX))
            .text_color(palette.text_primary)
            .child(PROMISE_BANNER.to_string()),
    )
}

/// Step 2：三选一项目入口 + 共享编译缓存提示。
fn render_project(palette: &Palette) -> Div {
    let mut column = div().flex().flex_col().gap(px(spacing::SPACE_2_PX));
    for choice in PROJECT_CHOICES {
        column = column.child(
            div()
                .px(px(spacing::SPACE_3_PX))
                .py(px(spacing::SPACE_2_PX))
                .rounded(px(radius::RADIUS_SM_PX))
                .bg(palette.surface_card_muted)
                .border_1()
                .border_color(palette.border_subtle)
                .text_size(px(typography::TEXT_CAPTION_PX))
                .text_color(palette.text_secondary)
                .child(choice.to_string()),
        );
    }
    column.child(
        div()
            .text_size(px(typography::TEXT_MICRO_PX))
            .text_color(palette.text_tertiary)
            .child(SHARED_TARGET_HINT.to_string()),
    )
}

/// Step 3：配对地址预览 + 5 张快捷键卡。
fn render_pairing(palette: &Palette) -> Div {
    let column = div().flex().flex_col().gap(px(spacing::SPACE_2_PX)).child(
        div()
            .px(px(spacing::SPACE_3_PX))
            .py(px(spacing::SPACE_2_PX))
            .rounded(px(radius::RADIUS_SM_PX))
            .bg(palette.surface_terminal)
            .text_size(px(typography::TEXT_MICRO_PX))
            .text_color(palette.text_code)
            .child(PAIRING_URL_PREFIX.to_string()),
    );

    let mut row = div().flex().flex_row().gap(px(spacing::SPACE_1_PX));
    for (chord, label) in SHORTCUT_CARDS {
        row = row.child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap(px(spacing::SPACE_HALF_PX))
                .px(px(spacing::SPACE_2_PX))
                .py(px(spacing::SPACE_1_PX))
                .rounded(px(radius::RADIUS_XS_PX))
                .bg(palette.surface_card_muted)
                .child(
                    div()
                        .text_size(px(typography::TEXT_MICRO_PX))
                        .text_color(palette.text_primary)
                        .child(chord.to_string()),
                )
                .child(
                    div()
                        .text_size(px(typography::TEXT_MICRO_PX))
                        .text_color(palette.text_tertiary)
                        .child(label.to_string()),
                ),
        );
    }
    column.child(row)
}

/// 底部动作区：主按钮在右（打开即聚焦），上一步在左。
fn render_footer(step: OnboardingStep, palette: &Palette) -> Div {
    let mut footer = div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap(px(spacing::SPACE_2_PX));
    if step.previous().is_some() {
        footer = footer.child(
            div()
                .px(px(spacing::SPACE_3_PX))
                .py(px(spacing::SPACE_1_5_PX))
                .rounded(px(radius::RADIUS_SM_PX))
                .text_size(px(typography::TEXT_CAPTION_PX))
                .text_color(palette.text_secondary)
                .child("上一步".to_string()),
        );
    } else {
        footer = footer.child(div());
    }
    footer.child(
        div()
            .px(px(spacing::SPACE_4_PX))
            .py(px(spacing::SPACE_1_5_PX))
            .rounded(px(radius::RADIUS_SM_PX))
            .bg(palette.action_inverse)
            .text_size(px(typography::TEXT_CAPTION_PX))
            .text_color(palette.action_on_inverse)
            .child(step.primary_action().to_string()),
    )
}
