//! 虚拟窗口数学：行高估算、可视区间、上下占位与滚动锚定（纯逻辑）。
//!
//! 设计基准：`pages/03-timeline-tools-and-checkpoints.md` 与
//! `pages/01-desktop-workbench.md` §7.1。虚拟列表只渲染可视行，其余以固定高度的
//! 上/下占位（spacer）撑开滚动高度，因此**不需要**逐帧布局整条对话流。
//!
//! 全部函数是纯计算：输入行高与滚动偏移，输出渲染所需区间，供视图与单测共用。
//! 参考 Padu `apps/desktop/src/app/transcript_view.rs` 的「有界可视行 + 尾行重测」
//! 结构，但几何常量与语义全部取自本仓 `design-system/soxu/**`。

/// 上/下额外预渲染的行数（滚动时不出现空白跳变）。
pub const OVERSCAN_ROWS: usize = 1;

/// 行高度量：一次构建、多次查询。
///
/// 由 [`crate::timeline::model::TimelineSnapshot`] 在快照构建时计算一次，
/// 渲染循环只做 `O(log n)` 查询，**禁止**每帧重新遍历行计算高度。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RowMetrics {
    /// 每行估算高度（px）。
    heights: Vec<f32>,
    /// 前缀和：`prefix[i]` = 前 `i` 行高度之和，`prefix[0] == 0.0`。
    prefix: Vec<f32>,
}

impl RowMetrics {
    /// 由行高序列构建前缀和（`O(n)`，只应在快照变更时调用）。
    pub fn from_heights(heights: Vec<f32>) -> Self {
        let mut prefix = Vec::with_capacity(heights.len() + 1);
        prefix.push(0.0);
        let mut acc = 0.0_f32;
        for h in &heights {
            // 负高度视为 0，避免上游估算缺陷破坏前缀和单调性。
            acc += h.max(0.0);
            prefix.push(acc);
        }
        Self { heights, prefix }
    }

    /// 行数。
    pub fn len(&self) -> usize {
        self.heights.len()
    }

    /// 是否没有行（clippy `len_without_is_empty`）。
    pub fn is_empty(&self) -> bool {
        self.heights.is_empty()
    }

    /// 总高度（px）。
    pub fn total_height(&self) -> f32 {
        self.prefix.last().copied().unwrap_or(0.0)
    }

    /// 第 `index` 行的高度；越界返回 0。
    pub fn height(&self, index: usize) -> f32 {
        self.heights.get(index).copied().unwrap_or(0.0)
    }

    /// 第 `index` 行的顶部偏移；越界返回总高度。
    pub fn top_of(&self, index: usize) -> f32 {
        self.prefix
            .get(index)
            .copied()
            .unwrap_or_else(|| self.total_height())
    }
}

/// 一次渲染所需的可视窗口切片。
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VirtualWindow {
    /// 首个需要渲染的行下标。
    pub first_row: usize,
    /// 需要渲染的行数（含 overscan）。
    pub visible_rows: usize,
    /// 顶部占位高度（px）。
    pub top_spacer: f32,
    /// 底部占位高度（px）。
    pub bottom_spacer: f32,
    /// 钳制后的滚动偏移（px）。
    pub offset: f32,
    /// 内容总高度（px）。
    pub total_height: f32,
    /// 最大滚动偏移（px）：`max(0, total - viewport)`。
    pub max_offset: f32,
}

impl VirtualWindow {
    /// 可视区间的半开末下标 `first_row + visible_rows`。
    pub fn end_row(&self) -> usize {
        self.first_row + self.visible_rows
    }

    /// 该行是否落在本次渲染区间内。
    pub fn contains(&self, index: usize) -> bool {
        index >= self.first_row && index < self.end_row()
    }
}

/// 把滚动偏移钳制到 `[0, total - viewport]`。
pub fn clamped_offset(offset: f32, total_height: f32, viewport_height: f32) -> f32 {
    let max_offset = (total_height - viewport_height).max(0.0);
    offset.clamp(0.0, max_offset)
}

/// 按行高度量计算可视窗口。
///
/// 规则：
/// - 偏移先钳制到合法区间；
/// - `first_row` = 包含 `offset` 的行（偏移为 0 时为 0），并向前吃掉 overscan；
/// - `visible_rows` = 与 `[offset, offset + viewport)` 相交的行数 + overscan（向后）；
/// - 上下占位保证 `top + Σ可见行高 + bottom == total_height`，滚动条不抖动。
pub fn for_metrics(metrics: &RowMetrics, viewport_height: f32, offset: f32) -> VirtualWindow {
    let total_height = metrics.total_height();
    let offset = clamped_offset(offset, total_height, viewport_height);
    let max_offset = (total_height - viewport_height).max(0.0);

    if metrics.is_empty() || viewport_height <= 0.0 {
        return VirtualWindow {
            first_row: 0,
            visible_rows: 0,
            top_spacer: 0.0,
            bottom_spacer: total_height,
            offset,
            total_height,
            max_offset,
        };
    }

    // 包含 offset 的行：prefix[i] <= offset 的最大 i（至少为 0）。
    let containing = metrics
        .prefix
        .partition_point(|p| *p <= offset)
        .saturating_sub(1)
        .min(metrics.len() - 1);
    let first_row = containing.saturating_sub(OVERSCAN_ROWS);

    let bottom = offset + viewport_height;
    let last_exclusive = metrics
        .prefix
        .partition_point(|p| *p < bottom)
        .max(first_row + 1)
        .min(metrics.len());
    let visible_rows = (last_exclusive - first_row + OVERSCAN_ROWS).min(metrics.len() - first_row);

    let top_spacer = metrics.top_of(first_row);
    let bottom_spacer = (total_height - metrics.top_of(first_row + visible_rows)).max(0.0);

    VirtualWindow {
        first_row,
        visible_rows,
        top_spacer,
        bottom_spacer,
        offset,
        total_height,
        max_offset,
    }
}

/// 滚动锚定：当锚点上方的内容高度变化（例如展开/收起工具卡）时，
/// 同步平移滚动偏移，使锚点保持在同一视觉位置。
///
/// 传入的 `height_delta_above` 为正表示上方增高，偏移同量增加；结果仍钳制在合法区间。
pub fn anchor_offset(
    prev_offset: f32,
    height_delta_above: f32,
    total_height: f32,
    viewport_height: f32,
) -> f32 {
    clamped_offset(
        prev_offset + height_delta_above,
        total_height,
        viewport_height,
    )
}

/// 流式追加行时的偏移策略：保持当前偏移不变（尾部增长不推动视口），
/// 除非原本贴底（`follow_tail`），此时吸附到新的底部。
pub fn offset_after_append(
    prev_offset: f32,
    follow_tail: bool,
    total_height: f32,
    viewport_height: f32,
) -> f32 {
    if follow_tail {
        clamped_offset(total_height, total_height, viewport_height)
    } else {
        clamped_offset(prev_offset, total_height, viewport_height)
    }
}

/// 判断当前是否贴底（尾部跟随）。容差 0.5px，避免浮点累计误差导致永不跟随。
pub fn is_at_bottom(offset: f32, total_height: f32, viewport_height: f32) -> bool {
    let max_offset = (total_height - viewport_height).max(0.0);
    max_offset - clamped_offset(offset, total_height, viewport_height) <= 0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uniform(count: usize, height: f32) -> RowMetrics {
        RowMetrics::from_heights(vec![height; count])
    }

    #[test]
    fn prefix_sums_are_monotonic_and_total_matches() {
        let metrics = RowMetrics::from_heights(vec![10.0, -5.0, 30.0]);
        assert_eq!(metrics.len(), 3);
        assert!(!metrics.is_empty());
        // 负高度被钳到 0，前缀和单调不减。
        assert_eq!(metrics.total_height(), 40.0);
        assert_eq!(metrics.top_of(0), 0.0);
        assert_eq!(metrics.top_of(1), 10.0);
        assert_eq!(metrics.top_of(2), 10.0);
        assert_eq!(metrics.top_of(3), 40.0);
        assert_eq!(metrics.height(3), 0.0);
    }

    #[test]
    fn uniform_window_at_top() {
        // 100 行 × 38px，视口 600px：0..=15 共 16 行与视口相交，再加 1 行 overscan。
        let metrics = uniform(100, 38.0);
        let win = for_metrics(&metrics, 600.0, 0.0);
        assert_eq!(win.first_row, 0);
        assert_eq!(win.visible_rows, 17);
        assert_eq!(win.top_spacer, 0.0);
        assert_eq!(win.total_height, 3800.0);
        assert_eq!(win.max_offset, 3200.0);
        // 三段高度守恒：视口内 + 底部占位 == 总高度。
        assert_eq!(win.bottom_spacer, 3800.0 - 17.0 * 38.0);
    }

    #[test]
    fn uniform_window_clamps_overscroll_and_bottom() {
        let metrics = uniform(100, 38.0);
        // 负偏移钳到 0。
        let top = for_metrics(&metrics, 600.0, -200.0);
        assert_eq!(top.offset, 0.0);
        assert_eq!(top.first_row, 0);

        // 超量偏移钳到 max_offset，且不越出末行。
        let bottom = for_metrics(&metrics, 600.0, 99_999.0);
        assert_eq!(bottom.offset, 3200.0);
        assert!(bottom.end_row() <= 100);
        assert_eq!(bottom.bottom_spacer, 0.0);
        assert!(bottom.first_row > 0);
    }

    #[test]
    fn window_mid_scroll_matches_row_math() {
        let metrics = uniform(100, 20.0);
        // 偏移 1000 → 第 50 行；上占位 = 50*20 - overscan*20 = 980。
        let win = for_metrics(&metrics, 300.0, 1000.0);
        assert_eq!(win.first_row, 49); // 50 - OVERSCAN_ROWS
        assert_eq!(win.top_spacer, 980.0);
        assert!(win.contains(49));
        assert!(win.contains(50));
        assert!(!win.contains(48));
        assert_eq!(
            win.top_spacer + (win.visible_rows as f32) * 20.0 + win.bottom_spacer,
            win.total_height
        );
    }

    #[test]
    fn variable_heights_keep_spacers_consistent() {
        let metrics = RowMetrics::from_heights(vec![30.0, 100.0, 30.0, 30.0, 30.0, 200.0]);
        let win = for_metrics(&metrics, 60.0, 0.0);
        assert_eq!(win.first_row, 0);
        assert_eq!(win.total_height, 420.0);
        assert_eq!(
            win.top_spacer
                + (win.first_row..win.end_row())
                    .map(|i| metrics.height(i))
                    .sum::<f32>()
                + win.bottom_spacer,
            win.total_height
        );

        // 视口落在超高行内部时，该行必须包含在窗口内。
        let deep = for_metrics(&metrics, 60.0, 150.0);
        assert!(deep.first_row <= 1 && deep.end_row() > 1);
    }

    #[test]
    fn empty_and_zero_viewport_are_safe() {
        let empty = for_metrics(&RowMetrics::default(), 600.0, 0.0);
        assert_eq!(empty.visible_rows, 0);
        assert_eq!(empty.bottom_spacer, 0.0);
        assert_eq!(empty.end_row(), 0);

        let metrics = uniform(10, 20.0);
        let zero = for_metrics(&metrics, 0.0, 0.0);
        assert_eq!(zero.visible_rows, 0);
        assert_eq!(zero.bottom_spacer, 200.0);
    }

    #[test]
    fn anchor_offset_tracks_expansion_above() {
        // 上方展开增高 120px：偏移同步 +120，锚点不动。
        assert_eq!(anchor_offset(400.0, 120.0, 2000.0, 600.0), 520.0);
        // 上方收起 -120px 同样吸附。
        assert_eq!(anchor_offset(520.0, -120.0, 2000.0, 600.0), 400.0);
        // 越过底部时钳制，不产生回弹。
        assert_eq!(anchor_offset(1900.0, 500.0, 2000.0, 600.0), 1400.0);
    }

    #[test]
    fn append_keeps_offset_unless_following_tail() {
        assert_eq!(offset_after_append(200.0, false, 2000.0, 600.0), 200.0);
        assert_eq!(offset_after_append(200.0, true, 2600.0, 600.0), 2000.0);
        assert!(is_at_bottom(1400.0, 2000.0, 600.0));
        assert!(!is_at_bottom(1399.0, 2000.0, 600.0));
        // 内容不足一屏时恒为贴底。
        assert!(is_at_bottom(0.0, 300.0, 600.0));
    }
}
