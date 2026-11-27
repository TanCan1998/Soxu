//! 窗口宽度断点与四栏工作台布局策略（纯逻辑，供视图与单测复用）。
//!
//! 断点唯一真相源为 `tokens.json -> layout.breakpoint*`（经 [`crate::tokens`] 直通）：
//!
//! - `ultra_compact (< 400px)`：超窄视口，全部侧栏收起，仅保留中栏；
//! - `compact ([400, 640))`：单栏流式，侧栏转为抽屉；
//! - `regular ([640, 1024))`：单栏居中限宽（消息 ≤760px / 输入舱 ≤720px）；
//! - `wide (≥ 1024px)`：四栏并排（Titlebar + 左栏 + 中栏 + 右栏 + Statusbar）。
//!
//! 准则（MASTER 第四节）：**只对当前窗口实时宽度分支**，绝不按设备型号或方向分支。
//! 本模块不绘制任何界面；几何落图在 [`crate::shell`]。

use crate::tokens::layout;

/// 窗口宽度断点区间。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Breakpoint {
    /// `< 400px`：超窄（iPad 1/3 分屏、小折叠外屏）。
    UltraCompact,
    /// `[400, 640)`：紧凑单栏。
    Compact,
    /// `[640, 1024)`：常规（手机横屏、iPad 竖屏）。
    Regular,
    /// `≥ 1024px`：宽屏四栏。
    Wide,
}

impl Breakpoint {
    /// 全部断点，按从窄到宽排列（供测试与设置页枚举）。
    pub const ALL: [Self; 4] = [Self::UltraCompact, Self::Compact, Self::Regular, Self::Wide];

    /// 由实时窗口宽度解析断点（px，逻辑像素）。
    pub fn from_width(width: f32) -> Self {
        if width < layout::BREAKPOINT_ULTRA_COMPACT_PX {
            Self::UltraCompact
        } else if width < layout::BREAKPOINT_REGULAR_PX {
            Self::Compact
        } else if width < layout::BREAKPOINT_WIDE_PX {
            Self::Regular
        } else {
            Self::Wide
        }
    }
}

/// 单个面板在某断点下的呈现形态。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelVisibility {
    /// 并排固定栏（四栏形态）。
    Docked,
    /// 收起为滑出抽屉（占位阶段表现为隐藏）。
    Drawer,
    /// 完全隐藏（超窄视口）。
    Hidden,
}

/// 某断点下的四栏布局策略快照。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayoutPlan {
    /// 断点本身。
    pub breakpoint: Breakpoint,
    /// 左栏（项目/Worktree 树）形态。
    pub sidebar: PanelVisibility,
    /// 中栏（主会话画布）形态——任何断点下都保留。
    pub canvas: PanelVisibility,
    /// 右栏（上下文审阅）形态。
    pub inspector: PanelVisibility,
    /// 底部状态横梁形态。
    pub statusbar: PanelVisibility,
}

impl LayoutPlan {
    /// 按断点推导四栏策略（`pages/01-desktop-workbench.md` §1 渐进收缩）。
    pub fn for_breakpoint(breakpoint: Breakpoint) -> Self {
        match breakpoint {
            Breakpoint::UltraCompact => Self {
                breakpoint,
                sidebar: PanelVisibility::Hidden,
                canvas: PanelVisibility::Docked,
                inspector: PanelVisibility::Hidden,
                statusbar: PanelVisibility::Docked,
            },
            Breakpoint::Compact => Self {
                breakpoint,
                sidebar: PanelVisibility::Drawer,
                canvas: PanelVisibility::Docked,
                inspector: PanelVisibility::Drawer,
                statusbar: PanelVisibility::Docked,
            },
            Breakpoint::Regular => Self {
                breakpoint,
                sidebar: PanelVisibility::Drawer,
                canvas: PanelVisibility::Docked,
                inspector: PanelVisibility::Drawer,
                statusbar: PanelVisibility::Docked,
            },
            Breakpoint::Wide => Self {
                breakpoint,
                sidebar: PanelVisibility::Docked,
                canvas: PanelVisibility::Docked,
                inspector: PanelVisibility::Docked,
                statusbar: PanelVisibility::Docked,
            },
        }
    }

    /// 由实时宽度直接推导（`from_width` + `for_breakpoint` 组合糖）。
    pub fn for_width(width: f32) -> Self {
        Self::for_breakpoint(Breakpoint::from_width(width))
    }

    /// 中栏内容最大宽度（阅读行宽上限，px）：
    /// 宽屏常规 = 760（transcript 上限）；更窄区间跟随窗口收缩。
    pub fn canvas_max_width(&self, window_width: f32) -> f32 {
        let ceiling = layout::TRANSCRIPT_MAX_WIDTH_PX;
        match self.breakpoint {
            Breakpoint::Wide => ceiling,
            // 窄区间：内容随窗口收缩，但保留 16px 双侧页边距（compact 页边距纪律）。
            _ => (window_width - 32.0).max(0.0).min(ceiling),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 断点边界矩阵：400/640/1024 三个阈值两侧各取一点 + 极端值。
    #[test]
    fn breakpoint_boundaries() {
        assert_eq!(Breakpoint::from_width(0.0), Breakpoint::UltraCompact);
        assert_eq!(Breakpoint::from_width(399.9), Breakpoint::UltraCompact);
        assert_eq!(Breakpoint::from_width(400.0), Breakpoint::Compact);
        assert_eq!(Breakpoint::from_width(639.9), Breakpoint::Compact);
        assert_eq!(Breakpoint::from_width(640.0), Breakpoint::Regular);
        assert_eq!(Breakpoint::from_width(1023.9), Breakpoint::Regular);
        assert_eq!(Breakpoint::from_width(1024.0), Breakpoint::Wide);
        assert_eq!(Breakpoint::from_width(2560.0), Breakpoint::Wide);
    }

    /// 断点常量与令牌零漂移（防止手改本模块阈值）。
    #[test]
    fn thresholds_match_tokens() {
        assert_eq!(layout::BREAKPOINT_ULTRA_COMPACT_PX, 400.0);
        assert_eq!(layout::BREAKPOINT_REGULAR_PX, 640.0);
        assert_eq!(layout::BREAKPOINT_WIDE_PX, 1024.0);
    }

    /// 跨断点扫描：中栏永远保留；超窄全隐侧栏；宽屏四栏全并排。
    #[test]
    fn panel_visibility_across_breakpoints() {
        let ultra = LayoutPlan::for_width(320.0);
        assert_eq!(ultra.sidebar, PanelVisibility::Hidden);
        assert_eq!(ultra.inspector, PanelVisibility::Hidden);
        assert_eq!(ultra.canvas, PanelVisibility::Docked);

        let compact = LayoutPlan::for_width(500.0);
        assert_eq!(compact.sidebar, PanelVisibility::Drawer);
        assert_eq!(compact.inspector, PanelVisibility::Drawer);

        let regular = LayoutPlan::for_width(820.0);
        assert_eq!(regular.sidebar, PanelVisibility::Drawer);
        assert_eq!(regular.canvas, PanelVisibility::Docked);

        let wide = LayoutPlan::for_width(1280.0);
        assert_eq!(wide.sidebar, PanelVisibility::Docked);
        assert_eq!(wide.inspector, PanelVisibility::Docked);
        assert_eq!(wide.canvas, PanelVisibility::Docked);
    }

    /// 阅读行宽上限：宽屏钳制在 760px；窄区间随窗口收缩且不为负。
    #[test]
    fn canvas_max_width_clamps_to_reading_measure() {
        let wide = LayoutPlan::for_width(1280.0);
        assert_eq!(wide.canvas_max_width(1280.0), 760.0);

        let regular = LayoutPlan::for_width(640.0);
        assert_eq!(regular.canvas_max_width(640.0), 608.0); // 640 - 32

        let ultra = LayoutPlan::for_width(320.0);
        assert_eq!(ultra.canvas_max_width(320.0), 288.0); // 320 - 32

        // 极小窗口不出负值。
        assert_eq!(LayoutPlan::for_width(10.0).canvas_max_width(10.0), 0.0);
    }
}
