//! 四栏自适应工作台骨架视图（占位面板，不做 M5 业务视图）。
//!
//! 结构（`pages/01-desktop-workbench.md` §1）：
//!
//! ```text
//! ┌ Titlebar (48px, surface.sidebar) ─────────────────────────────┐
//! ├ 左栏 (sidebar) ┬ 中栏 (canvas #000000) ┬ 右栏 (inspector) ────┤
//! ├ Statusbar (30px, surface.sidebar + 1px hairline 顶边) ────────┤
//! └───────────────────────────────────────────────────────────────┘
//! ```
//!
//! 渲染线程零阻塞铁律：`render` 只读内存快照（断点/主题/i18n 均为纯计算），
//! 无任何文件系统、SQLite、Git 或子进程调用。
//!
//! 面板内容全部为占位组件——M5 落地对话流、输入舱与审阅栏。

use gpui::{
    AppContext, Context, Div, Entity, Hsla, IntoElement, ParentElement, Pixels, Render, Styled,
    Window, div, px,
};

use crate::composer::render_for_window;
use crate::focus::FocusStack;
use crate::i18n::I18n;
use crate::layout::{LayoutPlan, PanelVisibility};
use crate::right_panel;
use crate::theme::theme_state;
use crate::timeline::TimelineView;
use crate::tokens::{layout, sizing};

/// 工作台壳层根视图。
pub struct WorkbenchShell {
    /// 焦点栈（当前骨架只维护状态；M5 接键位分发）。
    focus_stack: FocusStack,
    /// i18n 骨架（语言切换入口占位）。
    i18n: I18n,
    /// 中栏主对话时间线（M5-A）。
    timeline: Entity<TimelineView>,
}

impl WorkbenchShell {
    /// 创建壳层状态。
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_stack: FocusStack::new(),
            i18n: I18n::new(),
            timeline: cx.new(TimelineView::new),
        }
    }

    /// 纯逻辑切片：按当前焦点栈取有效 key context（供测试与 M5 键位接线）。
    pub fn effective_focus_context(&self) -> &'static str {
        self.focus_stack.effective_context()
    }
}

/// 把 `PanelVisibility` 收敛为「是否渲染该面板」。
fn rendered(vis: PanelVisibility) -> bool {
    matches!(vis, PanelVisibility::Docked)
}

/// 侧栏占位面板：sidebar 底色 + hairline 分隔，内容文字居中。
fn placeholder_panel(label: &'static str, background: Hsla, text: Hsla, width: Pixels) -> Div {
    div()
        .flex()
        .flex_col()
        .justify_center()
        .items_center()
        .w(width)
        .h_full()
        .bg(background)
        .text_color(text)
        .text_size(px(12.0))
        .child(label.to_string())
}

impl Render for WorkbenchShell {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = theme_state::current(cx).palette();
        let plan = LayoutPlan::for_width(_window.viewport_size().width.as_f32());
        let i18n = &self.i18n;

        // 三段纵向：Titlebar / 主体四栏 / Statusbar。
        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(palette.surface_canvas)
            .text_color(palette.text_primary)
            .font_family(crate::font::UiFontPreset::InterNoto.primary_family())
            .child(
                // Titlebar：48px，sidebar 底色，底部 1px hairline（MASTER §6.1）。
                div()
                    .flex()
                    .items_center()
                    .px(px(12.0))
                    .h(px(sizing::TITLEBAR_HEIGHT_PX))
                    .bg(palette.surface_sidebar)
                    .border_b_1()
                    .border_color(palette.border_hairline)
                    .child(
                        div()
                            .text_size(px(13.5))
                            .text_color(palette.text_primary)
                            .child(i18n.t("app.name").to_string()),
                    ),
            )
            .child(
                // 主体：左栏 + 中栏 + 右栏，横向排布；中栏 flex-1 永远保留。
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .min_h_0()
                    .when(rendered(plan.sidebar), |row| {
                        row.child(placeholder_panel(
                            "◀",
                            palette.surface_sidebar,
                            palette.text_tertiary,
                            px(layout::SIDEBAR_DEFAULT_PX),
                        ))
                    })
                    .child(
                        // 中栏：Pure Black 画布——上方 M5-A 虚拟时间线（对话流/工具卡/导航轨），
                        // 下方 M5-B 760px 同步宽输入舱（时间线占剩余高度，输入舱保留自身高度）。
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .bg(palette.surface_canvas)
                            .child(self.timeline.clone())
                            .child(render_for_window(
                                &plan,
                                &palette,
                                _window.viewport_size().width.as_f32(),
                            )),
                    )
                    .when(rendered(plan.inspector), |row| {
                        row.child(right_panel::render_default(&palette))
                    }),
            )
            .child(
                // Statusbar：30px，sidebar 底色，顶部 1px hairline（MASTER §6.1）。
                div()
                    .flex()
                    .items_center()
                    .px(px(12.0))
                    .h(px(sizing::STATUSBAR_HEIGHT_PX))
                    .bg(palette.surface_sidebar)
                    .border_t_1()
                    .border_color(palette.border_hairline)
                    .text_size(px(11.0))
                    .text_color(palette.text_tertiary)
                    .child(i18n.t("shell.statusbar.daemon_offline").to_string()),
            )
    }
}

/// 视图需要的小工具 trait：`Styled::when`（条件链式修饰），与 gpui `Styled` 组合。
///
/// gpui-pre 0.3.14 的 `Styled` 未内置 `when`；这里以扩展 trait 补齐，
/// 语义与 gpui-component 的 `FluentBuilder::when` 一致。
trait StyledExt: Styled + Sized {
    /// 条件应用修饰。
    fn when(self, condition: bool, then: impl FnOnce(Self) -> Self) -> Self {
        if condition { then(self) } else { self }
    }
}

impl<E: Styled> StyledExt for E {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Breakpoint;

    /// 壳层的纯逻辑切片：默认焦点上下文为 WorkbenchGlobal。
    #[test]
    fn shell_focus_slice_defaults_to_global() {
        let shell_state = FocusStack::new();
        assert_eq!(shell_state.effective_context(), "WorkbenchGlobal");
    }

    /// 断点收敛：Docked 渲染、Drawer/Hidden 不渲染（与 render 分支同语义）。
    #[test]
    fn panel_rendering_matches_plan() {
        let wide = LayoutPlan::for_breakpoint(Breakpoint::Wide);
        assert!(rendered(wide.sidebar));
        assert!(rendered(wide.inspector));

        let compact = LayoutPlan::for_breakpoint(Breakpoint::Compact);
        assert!(!rendered(compact.sidebar));
        assert!(!rendered(compact.inspector));

        let ultra = LayoutPlan::for_breakpoint(Breakpoint::UltraCompact);
        assert!(!rendered(ultra.sidebar));
        assert!(!rendered(ultra.inspector));
    }
}
