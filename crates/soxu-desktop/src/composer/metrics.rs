//! 输入舱与对话流共用的 760px 同步宽几何（纯逻辑，无 I/O）。
//!
//! 规范来源：`pages/02-composer-and-handoff.md` §1「外框容器与同步宽度对齐铁律」——
//! 输入舱必须与上方对话消息流同置于同一居中容器内，共同遵循 760px 上限与完全相等的
//! 左右内边距；拖拽左栏或右栏时两者按完全相同的像素宽度实时同步伸缩。
//!
//! 实现纪律（Issue #130 第 1 条）：
//! - 断点与栏宽令牌只从 [`crate::layout::LayoutPlan`] 与 [`crate::tokens::layout`] 读取，
//!   本模块**不重算、不新造**任何断点阈值或宽度常量；
//! - 中栏实际宽度 = 窗口宽度 − 并排（`Docked`）侧栏占位；抽屉/隐藏面板不占像素；
//! - 对话流与输入舱**共用同一个函数取值**，因此两者宽度恒等（含拖拽联动）。

use crate::layout::{LayoutPlan, PanelVisibility};
use crate::tokens::layout as layout_tokens;

/// 中栏内容单侧页边距（px）：两侧合计 32px，与
/// [`LayoutPlan::canvas_max_width`] 在窄区间的收缩口径一致。
pub const CANVAS_GUTTER_PX: f32 = 16.0;

/// 同步宽内容上限（px）：对话流与输入舱共同遵守的 760px。
///
/// 直接取令牌 `layout.composerMaxWidth`；编译期断言它与 `transcriptMaxWidth` 相等，
/// 避免两栏宽度契约被单侧改动打破。
pub const SYNC_CONTENT_MAX_WIDTH_PX: f32 = layout_tokens::COMPOSER_MAX_WIDTH_PX;

/// 编译期守卫：对话流与输入舱上限必须是同一个 760px。
const _: () = assert!(
    layout_tokens::COMPOSER_MAX_WIDTH_PX == layout_tokens::TRANSCRIPT_MAX_WIDTH_PX,
    "输入舱与对话流的最大宽度令牌必须保持同值"
);

/// 面板在给定断点下实际占用的像素宽度：只有 `Docked` 才占位。
fn docked_width(visibility: PanelVisibility, panel_width: f32) -> f32 {
    match visibility {
        PanelVisibility::Docked => panel_width.max(0.0),
        PanelVisibility::Drawer | PanelVisibility::Hidden => 0.0,
    }
}

/// 中栏（画布列）在给定窗口宽度与栏宽下的可用像素宽度。
///
/// `sidebar_width` / `inspector_width` 由持有分栏拖拽状态的视图传入，
/// 本函数只按 [`LayoutPlan`] 的并排形态决定是否扣除。
pub fn canvas_column_width(
    window_width: f32,
    plan: &LayoutPlan,
    sidebar_width: f32,
    inspector_width: f32,
) -> f32 {
    (window_width
        - docked_width(plan.sidebar, sidebar_width)
        - docked_width(plan.inspector, inspector_width))
    .max(0.0)
}

/// 同步宽内容宽度：对话流与输入舱的唯一取值入口（px，已扣页边距并钳制到 760px）。
pub fn synchronized_content_width(
    window_width: f32,
    plan: &LayoutPlan,
    sidebar_width: f32,
    inspector_width: f32,
) -> f32 {
    let available = (canvas_column_width(window_width, plan, sidebar_width, inspector_width)
        - 2.0 * CANVAS_GUTTER_PX)
        .max(0.0);
    available.min(SYNC_CONTENT_MAX_WIDTH_PX)
}

/// 输入舱落图几何快照（渲染层只读，不在 `render` 中重算断点）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ComposerGeometry {
    /// 输入舱与对话流共用的内容宽度（px）。
    pub content_width: f32,
    /// 单侧页边距（px）。
    pub gutter: f32,
    /// 是否展示权限按钮的中文标签（超窄区间退化为纯图标）。
    pub show_permission_label: bool,
}

impl ComposerGeometry {
    /// 由窗口宽度、布局计划与实际栏宽推导几何。
    pub fn new(
        window_width: f32,
        plan: &LayoutPlan,
        sidebar_width: f32,
        inspector_width: f32,
    ) -> Self {
        let content_width =
            synchronized_content_width(window_width, plan, sidebar_width, inspector_width);
        Self {
            content_width,
            gutter: CANVAS_GUTTER_PX,
            // 超窄断点以下主操控行会折行：按 02 页 §3.2 退化为 32×32 纯图标钮。
            show_permission_label: content_width >= layout_tokens::BREAKPOINT_ULTRA_COMPACT_PX,
        }
    }

    /// 侧栏/审阅栏取令牌默认宽度时的几何。
    ///
    /// 壳层（[`crate::shell`]）当前只持有 [`LayoutPlan`]，真实拖拽宽度在 M5 分栏接线下沉后
    /// 改由持有栏宽状态的视图调用 [`ComposerGeometry::new`] 注入。
    pub fn for_default_panels(window_width: f32, plan: &LayoutPlan) -> Self {
        Self::new(
            window_width,
            plan,
            layout_tokens::SIDEBAR_DEFAULT_PX,
            layout_tokens::INSPECTOR_DEFAULT_PX,
        )
    }

    /// 内容宽度是否为 0（极小窗口，渲染时不出负值也不会撑破布局）。
    pub fn is_collapsed(&self) -> bool {
        self.content_width <= 0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Breakpoint;

    /// 令牌零漂移：对话流与输入舱上限同为 760px。
    #[test]
    fn sync_width_matches_tokens() {
        assert_eq!(SYNC_CONTENT_MAX_WIDTH_PX, 760.0);
        assert_eq!(
            layout_tokens::TRANSCRIPT_MAX_WIDTH_PX,
            layout_tokens::COMPOSER_MAX_WIDTH_PX
        );
        assert_eq!(2.0 * CANVAS_GUTTER_PX, 32.0);
    }

    /// 宽屏四栏并排：窗口足够宽时固定钳制在 760px。
    #[test]
    fn wide_window_clamps_to_760() {
        let plan = LayoutPlan::for_breakpoint(Breakpoint::Wide);
        assert_eq!(
            synchronized_content_width(1600.0, &plan, 260.0, 380.0),
            760.0
        );
        // 窗口再宽也不会超过阅读行宽上限。
        assert_eq!(
            synchronized_content_width(2560.0, &plan, 224.0, 280.0),
            760.0
        );
    }

    /// 拖拽联动：左栏变宽 100px，中栏同步内容宽度精确减少 100px（上限内线性联动）。
    #[test]
    fn dragging_sidebar_links_width_one_to_one() {
        let plan = LayoutPlan::for_breakpoint(Breakpoint::Wide);
        let before = synchronized_content_width(1120.0, &plan, 260.0, 380.0);
        let after = synchronized_content_width(1120.0, &plan, 360.0, 380.0);
        assert_eq!(before, 448.0); // 1120 − 260 − 380 − 32
        assert_eq!(after, 348.0);
        assert_eq!(before - after, 100.0);
    }

    /// 拖拽联动：右栏（审阅栏）同理，且与左栏参数对称。
    #[test]
    fn dragging_inspector_links_width_one_to_one() {
        let plan = LayoutPlan::for_breakpoint(Breakpoint::Wide);
        let before = synchronized_content_width(1120.0, &plan, 260.0, 280.0);
        let after = synchronized_content_width(1120.0, &plan, 260.0, 480.0);
        assert_eq!(before, 548.0); // 1120 − 260 − 280 − 32
        assert_eq!(after, 348.0);
        assert_eq!(before - after, 200.0);
    }

    /// 抽屉/隐藏面板不占像素：紧凑与超窄区间只按窗口收缩（并保留 32px 页边距）。
    #[test]
    fn drawer_and_hidden_panels_take_no_pixels() {
        let compact = LayoutPlan::for_breakpoint(Breakpoint::Compact);
        assert_eq!(
            synchronized_content_width(640.0, &compact, 260.0, 380.0),
            608.0
        );

        let ultra = LayoutPlan::for_breakpoint(Breakpoint::UltraCompact);
        assert_eq!(
            synchronized_content_width(320.0, &ultra, 260.0, 380.0),
            288.0
        );

        // 极小窗口不出负值。
        assert_eq!(synchronized_content_width(10.0, &ultra, 260.0, 380.0), 0.0);
        assert!(ComposerGeometry::for_default_panels(10.0, &ultra).is_collapsed());
    }

    /// 中栏列宽始终 ≥ 同步内容宽度（页边距只内缩，不反向溢出）。
    #[test]
    fn column_width_never_smaller_than_content() {
        let plan = LayoutPlan::for_breakpoint(Breakpoint::Wide);
        for window in [1024.0_f32, 1120.0, 1440.0, 2560.0] {
            let column = canvas_column_width(window, &plan, 260.0, 380.0);
            let content = synchronized_content_width(window, &plan, 260.0, 380.0);
            assert!(column >= content, "window={window} column<content");
        }
    }

    /// 纯图标退化阈值：内容宽度跨过 400px 断点时切换「图标 + 中文标签」。
    #[test]
    fn permission_label_degrades_below_ultra_compact() {
        let ultra = LayoutPlan::for_breakpoint(Breakpoint::UltraCompact);
        // 320 窗口 → 288px 内容宽度 < 400px：纯图标。
        assert!(!ComposerGeometry::for_default_panels(320.0, &ultra).show_permission_label);

        let wide = LayoutPlan::for_breakpoint(Breakpoint::Wide);
        // 1280 − 260 − 380 − 32 = 608 ≥ 400：图标 + 标签。
        assert!(ComposerGeometry::for_default_panels(1280.0, &wide).show_permission_label);
    }
}
