//! 20px 轮次迷你导航轨：几何、刻度样式、命中与跳转（纯逻辑）。
//!
//! 设计基准：`pages/01-desktop-workbench.md` §7.1——
//! 对话超过 2 轮时在对话滚动容器最右侧浮动一条宽 **20px** 的导航轨
//! （`zIndex.rail = 15`，不挤占 760px 阅读列）；每个轮次一条
//! 高 `3px`、宽 `10~14px` 的水平微缩刻度线，当前可视轮次加宽到 `16px` 且纯白，
//! 含待审批的轮次琥珀、含检查点的轮次紫罗兰，点击平滑滚动定位到该轮顶部。
//!
//! 参考 Padu `apps/desktop/src/app/transcript.rs` 的
//! `navigation_rail_height / scale / active_navigation_turn_index` 结构：
//! 高度按视口比例封顶、压缩时等比例收窄槽位、活跃轮次用二分定位。
//! 几何常量全部改由本仓令牌（`sizing::TURN_NAV_RAIL_WIDTH_PX = 20`）推导。

use crate::theme::tokens::color::dark;
use crate::theme::tokens::color::light;
use crate::theme::tokens::sizing;

/// 导航轨宽度（px）：令牌 `sizing.turnNavRailWidth` = 20。
pub const RAIL_WIDTH_PX: f32 = sizing::TURN_NAV_RAIL_WIDTH_PX;

/// 刻度线高度（px）。
pub const TICK_HEIGHT_PX: f32 = 3.0;

/// 普通刻度线宽度（px）。
pub const TICK_WIDTH_PX: f32 = 10.0;

/// 悬停刻度线宽度（px）。
pub const TICK_WIDTH_HOVER_PX: f32 = 14.0;

/// 当前活跃轮次刻度线宽度（px）。
pub const TICK_WIDTH_ACTIVE_PX: f32 = 16.0;

/// 刻度线之间的间距（px）：槽高 = 刻度高 + 间距。
pub const TICK_GAP_PX: f32 = 6.0;

/// 单个轮次占用的槽高（px）。
pub const SLOT_HEIGHT_PX: f32 = TICK_HEIGHT_PX + TICK_GAP_PX;

/// 导航轨最大高度占视口的比例（超出后等比例压缩槽位，参考 Padu 0.80）。
pub const VIEWPORT_HEIGHT_RATIO: f32 = 0.8;

/// 至少超过该轮次数才显示导航轨（「超过 2 轮」→ 3 轮起）。
pub const MIN_TURNS: usize = 2;

/// 单个轮次的语义标记。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TurnFlags {
    /// 该轮含待人工审批（`needs_input`）。
    pub needs_input: bool,
    /// 该轮含检查点快照。
    pub checkpoint: bool,
}

impl TurnFlags {
    /// 按轮次快照推导标记。
    pub fn of(turn: &crate::timeline::model::TurnSnapshot) -> Self {
        Self {
            needs_input: turn.needs_input,
            checkpoint: turn.checkpoint.is_some(),
        }
    }
}

/// 刻度线配色语义（对应 `tokens.json` 的语义色）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TickTone {
    /// 普通已读轮次 `#383838`。
    Read,
    /// 当前可视活跃轮次 `#FFFFFF`。
    Active,
    /// 含待审批轮次 `#F59E0B`。
    NeedsInput,
    /// 含检查点轮次 `#A855F7`。
    Checkpoint,
}

impl TickTone {
    /// 该语义色在深色模式下的令牌值。
    pub fn dark_token(self) -> u32 {
        match self {
            Self::Read => dark::BORDER_STRONG,
            Self::Active => dark::BORDER_FOCUS_RING,
            Self::NeedsInput => dark::SEMANTIC_WARNING,
            Self::Checkpoint => dark::SEMANTIC_HANDOFF,
        }
    }

    /// 该语义色在浅色模式下的令牌值。
    pub fn light_token(self) -> u32 {
        match self {
            Self::Read => light::BORDER_STRONG,
            Self::Active => light::BORDER_FOCUS_RING,
            Self::NeedsInput => light::SEMANTIC_WARNING,
            Self::Checkpoint => light::SEMANTIC_HANDOFF,
        }
    }
}

/// 导航轨布局快照。
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RailLayout {
    /// 是否渲染导航轨。
    pub visible: bool,
    /// 导航轨总高度（px）。
    pub height: f32,
    /// 导航轨顶部相对滚动容器的偏移（px，垂直居中）。
    pub top: f32,
    /// 单个轮次槽高（px）；内容超高时会被压缩。
    pub slot: f32,
    /// 压缩后的刻度线高度（px），不超过 [`TICK_HEIGHT_PX`]。
    pub tick_height: f32,
}

impl RailLayout {
    /// 该布局下第 `index` 个刻度的顶部偏移（相对导航轨顶部）。
    pub fn tick_top(&self, index: usize) -> f32 {
        index as f32 * self.slot
    }

    /// 不可见布局（供提前返回复用）。
    pub fn hidden() -> Self {
        Self {
            visible: false,
            height: 0.0,
            top: 0.0,
            slot: SLOT_HEIGHT_PX,
            tick_height: TICK_HEIGHT_PX,
        }
    }
}

/// 计算导航轨布局。
///
/// - 轮次数 `<= MIN_TURNS`（不超过 2 轮）时不显示；
/// - 自然高度 `turn_count * SLOT_HEIGHT`，超过 `viewport * 0.8` 时等比例压缩槽位；
/// - 垂直居中（`top = (viewport - height) / 2`，不为负）。
pub fn layout(turn_count: usize, viewport_height: f32) -> RailLayout {
    if turn_count <= MIN_TURNS || viewport_height <= 0.0 {
        return RailLayout::hidden();
    }
    let natural = turn_count as f32 * SLOT_HEIGHT_PX;
    let ceiling = viewport_height * VIEWPORT_HEIGHT_RATIO;
    let height = natural.min(ceiling);
    let slot = height / turn_count as f32;
    RailLayout {
        visible: true,
        height,
        top: ((viewport_height - height) / 2.0).max(0.0),
        slot,
        tick_height: TICK_HEIGHT_PX.min(slot),
    }
}

/// 刻度线样式：返回配色语义与宽度。
///
/// 配色优先级：待审批（安全信息最高）＞ 检查点 ＞ 当前活跃 ＞ 普通已读。
/// 宽度优先级：当前活跃（16px）＞ 悬停（14px）＞ 普通（10px）。
pub fn tick_style(
    index: usize,
    active: Option<usize>,
    hovered: Option<usize>,
    flags: TurnFlags,
) -> (TickTone, f32) {
    let tone = if flags.needs_input {
        TickTone::NeedsInput
    } else if flags.checkpoint {
        TickTone::Checkpoint
    } else if active == Some(index) {
        TickTone::Active
    } else {
        TickTone::Read
    };
    let width = if active == Some(index) {
        TICK_WIDTH_ACTIVE_PX
    } else if hovered == Some(index) {
        TICK_WIDTH_HOVER_PX
    } else {
        TICK_WIDTH_PX
    };
    (tone, width)
}

/// 命中测试：把导航轨内的局部 y 坐标换算为轮次下标。
///
/// 越界（`y < 0` 或落在末刻度之后）返回 `None`，绝不返回越界下标——
/// 点击空白区域不产生跳转，避免误定位。
pub fn hit_test(y: f32, layout: &RailLayout, turn_count: usize) -> Option<usize> {
    if !layout.visible || turn_count == 0 || y < 0.0 {
        return None;
    }
    let index = (y / layout.slot.max(f32::EPSILON)).floor();
    if !index.is_finite() || index < 0.0 {
        return None;
    }
    let index = index as usize;
    (index < turn_count).then_some(index)
}

/// 导航轨活跃轮次定位的浮点容差（px），吸收滚动偏移累计误差。
pub const ACTIVE_INDEX_TOLERANCE_PX: f32 = 0.5;

/// 由滚动位置推导当前活跃轮次下标。
///
/// `turn_tops` 单调不减，用二分（`partition_point`）定位「顶部不超过当前滚动位置」
/// 的最后一个轮次；`at_end` 表示已贴底，此时强制末轮活跃。
pub fn active_turn_index(turn_tops: &[f32], scroll_top: f32, at_end: bool) -> Option<usize> {
    if turn_tops.is_empty() {
        return None;
    }
    if at_end {
        return Some(turn_tops.len() - 1);
    }
    Some(
        turn_tops
            .partition_point(|top| *top <= scroll_top + ACTIVE_INDEX_TOLERANCE_PX)
            .saturating_sub(1),
    )
}

/// 点击刻度后的滚动目标偏移（钳制到 `[0, total - viewport]`）。
///
/// 轮次越界返回 `None`，调用方据此保持原滚动位置。
pub fn jump_offset(
    turn_tops: &[f32],
    index: usize,
    viewport_height: f32,
    total_height: f32,
) -> Option<f32> {
    let top = turn_tops.get(index).copied()?;
    let max_offset = (total_height - viewport_height).max(0.0);
    Some(top.clamp(0.0, max_offset))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rail_width_comes_from_token() {
        assert_eq!(RAIL_WIDTH_PX, 20.0);
        assert_eq!(sizing::TURN_NAV_RAIL_WIDTH_PX, RAIL_WIDTH_PX);
    }

    #[test]
    fn layout_hidden_until_more_than_two_turns() {
        assert!(!layout(0, 800.0).visible);
        assert!(!layout(1, 800.0).visible);
        assert!(
            !layout(2, 800.0).visible,
            "恰好 2 轮不显示（规范：超过 2 轮）"
        );
        let three = layout(3, 800.0);
        assert!(three.visible);
        assert_eq!(three.height, 3.0 * SLOT_HEIGHT_PX);
        assert_eq!(three.slot, SLOT_HEIGHT_PX);
        assert_eq!(three.tick_height, TICK_HEIGHT_PX);
        // 垂直居中。
        assert_eq!(three.top, (800.0 - three.height) / 2.0);
        // 视口为 0 时不显示。
        assert!(!layout(10, 0.0).visible);
    }

    #[test]
    fn layout_compresses_when_exceeding_viewport_ratio() {
        // 200 轮 × 9px = 1800px > 800 * 0.8 = 640 → 压缩。
        let compressed = layout(200, 800.0);
        assert!(compressed.visible);
        assert_eq!(compressed.height, 640.0);
        assert_eq!(compressed.top, 80.0);
        assert!((compressed.slot - 3.2).abs() < 1e-4);
        // 槽位小于刻度高度时刻度同步收窄，不重叠。
        assert!(compressed.tick_height <= compressed.slot);
        assert!(compressed.tick_height <= TICK_HEIGHT_PX);
        // 末刻度仍落在导航轨内部。
        assert!(compressed.tick_top(199) + compressed.tick_height <= compressed.height + 1e-3);
    }

    #[test]
    fn tick_style_priority_and_widths() {
        let plain = TurnFlags::default();
        // 普通已读。
        assert_eq!(
            tick_style(0, Some(3), None, plain),
            (TickTone::Read, TICK_WIDTH_PX)
        );
        // 当前活跃：纯白 + 加宽 16px。
        assert_eq!(
            tick_style(3, Some(3), None, plain),
            (TickTone::Active, TICK_WIDTH_ACTIVE_PX)
        );
        // 悬停：14px，仍是普通配色。
        assert_eq!(
            tick_style(1, Some(3), Some(1), plain),
            (TickTone::Read, TICK_WIDTH_HOVER_PX)
        );
        // 检查点优先于活跃。
        let checkpoint = TurnFlags {
            needs_input: false,
            checkpoint: true,
        };
        assert_eq!(
            tick_style(3, Some(3), None, checkpoint),
            (TickTone::Checkpoint, TICK_WIDTH_ACTIVE_PX)
        );
        // 待审批优先于检查点（安全信息最高）。
        let both = TurnFlags {
            needs_input: true,
            checkpoint: true,
        };
        assert_eq!(tick_style(2, None, None, both).0, TickTone::NeedsInput);
        // 语义色令牌与设计一致（深/浅双模式）。
        assert_eq!(TickTone::NeedsInput.dark_token(), dark::SEMANTIC_WARNING);
        assert_eq!(TickTone::Checkpoint.light_token(), light::SEMANTIC_HANDOFF);
        assert_eq!(TickTone::Active.dark_token(), dark::BORDER_FOCUS_RING);
        assert_eq!(TickTone::Read.dark_token(), dark::BORDER_STRONG);
    }

    #[test]
    fn hit_test_respects_bounds() {
        let rail = layout(5, 400.0);
        assert_eq!(rail.slot, SLOT_HEIGHT_PX);
        assert_eq!(hit_test(0.0, &rail, 5), Some(0));
        assert_eq!(hit_test(8.9, &rail, 5), Some(0));
        assert_eq!(hit_test(9.0, &rail, 5), Some(1));
        assert_eq!(hit_test(44.5, &rail, 5), Some(4));
        // 末刻度之后 → None。
        assert_eq!(hit_test(45.0, &rail, 5), None);
        // 负坐标 → None。
        assert_eq!(hit_test(-1.0, &rail, 5), None);
        // 不可见轨（2 轮）→ None。
        let hidden = layout(2, 400.0);
        assert_eq!(hit_test(0.0, &hidden, 2), None);
        // 压缩轨仍可用。
        let compressed = layout(200, 800.0);
        assert_eq!(hit_test(0.0, &compressed, 200), Some(0));
        assert_eq!(hit_test(3.3, &compressed, 200), Some(1));
        assert_eq!(hit_test(63_999.0, &compressed, 200), None);
    }

    #[test]
    fn active_index_uses_partition_point_and_end_override() {
        let tops = [0.0, 420.0, 900.0];
        assert_eq!(active_turn_index(&tops, 0.0, false), Some(0));
        assert_eq!(active_turn_index(&tops, 419.0, false), Some(0));
        assert_eq!(active_turn_index(&tops, 421.0, false), Some(1));
        assert_eq!(active_turn_index(&tops, 5000.0, false), Some(2));
        // 贴底强制末轮。
        assert_eq!(active_turn_index(&tops, 0.0, true), Some(2));
        // 空列表。
        assert_eq!(active_turn_index(&[], 100.0, false), None);
        assert_eq!(active_turn_index(&[], 0.0, true), None);
    }

    #[test]
    fn jump_offset_clamps_and_rejects_out_of_range() {
        let tops = [0.0, 420.0, 900.0];
        // 中间轮次直接对齐顶部。
        assert_eq!(jump_offset(&tops, 1, 300.0, 1500.0), Some(420.0));
        // 末轮被钳制到 max_offset = 1500 - 300 = 1200。
        assert_eq!(jump_offset(&tops, 2, 300.0, 1500.0), Some(900.0));
        assert_eq!(jump_offset(&[0.0, 1400.0], 1, 300.0, 1500.0), Some(1200.0));
        // 越界轮次返回 None。
        assert_eq!(jump_offset(&tops, 3, 300.0, 1500.0), None);
        assert_eq!(jump_offset(&[], 0, 300.0, 1500.0), None);
    }
}
