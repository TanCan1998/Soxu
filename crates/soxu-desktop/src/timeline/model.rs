//! 时间线视图模型快照（内存视图输入，不做任何 I/O）。
//!
//! 数据来源是 Issue #129 约定的「内存视图模型/快照」：本模块只描述一条已经渲染
//! 就绪的对话流快照，不含 daemon、SQLite、Git 或子进程调用（渲染线程零阻塞铁律）。
//!
//! **逐帧性能不变量**（`pages/01-desktop-workbench.md` §7.1）：导航轨快照只允许
//! `Rc::ptr_eq` 做 `O(1)` 指针相等性比较，禁止每帧深度遍历消息切片。因此
//! [`TimelineSnapshot`] 的 `PartialEq` 对 `turns` / `rows` 走指针比较，
//! 行高度量与轮次顶部偏移在快照构建时**一次性**算好，渲染循环不再重算。

use std::rc::Rc;

use crate::timeline::window::RowMetrics;

/// 轮次标识（单调递增；同一会话内唯一）。
pub type TurnId = u64;

/// 行标识（单调递增；用于展开态缓存与虚拟列表 key）。
pub type RowId = u64;

/// 单行最小高度：用户消息胶囊（`padding 12px 16px` + 正文）。
pub const USER_TURN_HEIGHT_PX: f32 = 64.0;
/// Agent 回复的身份头高度（03 §1 第 2 条：紧凑身份头 22px）。
pub const ASSISTANT_HEADER_HEIGHT_PX: f32 = 22.0;
/// 工具卡折叠态单行高度（03 §2：折叠态单行展示 34px）。
pub const TOOL_CARD_FOLDED_HEIGHT_PX: f32 = 34.0;
/// `LiveSteps` 默认折叠的单行焦点微条高度（03 §7.2：30px 紧凑脉冲条）。
pub const LIVE_STEPS_HEIGHT_PX: f32 = 30.0;
/// 检查点锚点横条高度（03 §4：高 26px 的 `#121212` 胶囊）。
pub const CHECKPOINT_ANCHOR_HEIGHT_PX: f32 = 26.0;
/// 轮次间垂直间距（03 §1：`--soxu-space-6` = 24px）。
pub const TURN_GAP_PX: f32 = 24.0;
/// 正文行高估算（估算值，仅用于虚拟窗口，不参与排版真值）。
pub const BODY_LINE_HEIGHT_PX: f32 = 20.0;
/// 展开态详情行高估算。
pub const DETAIL_LINE_HEIGHT_PX: f32 = 18.0;
/// 审批卡折叠态高度估算（头部 + 96px 详情区 + 按钮行）。
pub const APPROVAL_CARD_HEIGHT_PX: f32 = 132.0;
/// 提问卡单选项行高（03 §5：桌面 `min-height: 36px`）。
pub const ASK_OPTION_HEIGHT_PX: f32 = 36.0;

/// 对话流行类别。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TimelineRowKind {
    /// 用户消息（右对齐胶囊）。
    UserMessage,
    /// Agent 回复（无气泡全宽）。
    AssistantMessage,
    /// 5 类结构化工具卡中的一张。
    ToolCard,
    /// 权限审批卡（`PermissionGateCard`）。
    ApprovalGate,
    /// 结构化提问卡（`PendingUserInput`）。
    AskQuestion,
    /// 轮次检查点锚点横条。
    Checkpoint,
    /// 实时步骤焦点条（`LiveSteps`）。
    LiveSteps,
}

impl TimelineRowKind {
    /// 全部行类别。
    pub const ALL: [Self; 7] = [
        Self::UserMessage,
        Self::AssistantMessage,
        Self::ToolCard,
        Self::ApprovalGate,
        Self::AskQuestion,
        Self::Checkpoint,
        Self::LiveSteps,
    ];

    /// 是否属于「人类必须决策」的行（审批 / 提问）。
    pub fn needs_human(&self) -> bool {
        matches!(self, Self::ApprovalGate | Self::AskQuestion)
    }
}

/// 行高度估算：输入类别、展开态与正文行数，输出 px。
///
/// 这是**估算**（虚拟列表用），不是排版真值；`overscan` 与钳制在
/// [`crate::timeline::window`] 里兜住估算误差。
pub fn estimate_height_px(kind: TimelineRowKind, expanded: bool, body_lines: usize) -> f32 {
    let body = body_lines as f32;
    match kind {
        TimelineRowKind::UserMessage => USER_TURN_HEIGHT_PX + body * BODY_LINE_HEIGHT_PX,
        TimelineRowKind::AssistantMessage => {
            ASSISTANT_HEADER_HEIGHT_PX + body * BODY_LINE_HEIGHT_PX
        }
        TimelineRowKind::ToolCard => {
            if expanded {
                TOOL_CARD_FOLDED_HEIGHT_PX + body * DETAIL_LINE_HEIGHT_PX + 12.0
            } else {
                TOOL_CARD_FOLDED_HEIGHT_PX
            }
        }
        TimelineRowKind::ApprovalGate => APPROVAL_CARD_HEIGHT_PX,
        TimelineRowKind::AskQuestion => 56.0 + body * ASK_OPTION_HEIGHT_PX + 48.0,
        TimelineRowKind::Checkpoint => CHECKPOINT_ANCHOR_HEIGHT_PX,
        TimelineRowKind::LiveSteps => {
            if expanded {
                LIVE_STEPS_HEIGHT_PX + body * DETAIL_LINE_HEIGHT_PX
            } else {
                LIVE_STEPS_HEIGHT_PX
            }
        }
    }
}

/// 行视觉语义（只决定描边/微标颜色，不承载文字；MASTER 铁律：语义色必须配文字标签）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowTone {
    /// 中性灰：普通卡片与消息。
    Neutral,
    /// 电光蓝：正在运行（工具/子代理）。
    Running,
    /// 琥珀：待人工审批。
    NeedsInput,
    /// 绯红：高危红线或失败。
    Danger,
    /// 紫罗兰：检查点与跨代理移交。
    Handoff,
}

impl RowTone {
    /// 深色模式描边令牌。
    pub fn dark_token(self) -> u32 {
        use crate::tokens::color::dark;
        match self {
            Self::Neutral => dark::BORDER_SUBTLE,
            Self::Running => dark::SEMANTIC_RUNNING,
            Self::NeedsInput => dark::SEMANTIC_WARNING,
            Self::Danger => dark::SEMANTIC_DANGER,
            Self::Handoff => dark::SEMANTIC_HANDOFF,
        }
    }

    /// 浅色模式描边令牌。
    pub fn light_token(self) -> u32 {
        use crate::tokens::color::light;
        match self {
            Self::Neutral => light::BORDER_SUBTLE,
            Self::Running => light::SEMANTIC_RUNNING,
            Self::NeedsInput => light::SEMANTIC_WARNING,
            Self::Danger => light::SEMANTIC_DANGER,
            Self::Handoff => light::SEMANTIC_HANDOFF,
        }
    }
}

/// 一行对话流（视图渲染所需的全部只读字段）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimelineRow {
    /// 行标识。
    pub id: RowId,
    /// 行类别。
    pub kind: TimelineRowKind,
    /// 是否展开。
    pub expanded: bool,
    /// 折叠态标题（已本地化，视图直接渲染）。
    pub title: String,
    /// 可选副标题（如路径前缀、MCP 路由来源）。
    pub subtitle: Option<String>,
    /// 正文/详情行数（用于高度估算与截断提示）。
    pub body_lines: usize,
    /// 视觉语义（描边/微标颜色）。
    pub tone: RowTone,
}

impl TimelineRow {
    /// 构造一行。
    pub fn new(
        id: RowId,
        kind: TimelineRowKind,
        title: impl Into<String>,
        body_lines: usize,
    ) -> Self {
        Self {
            id,
            kind,
            expanded: false,
            title: title.into(),
            subtitle: None,
            body_lines,
            tone: RowTone::Neutral,
        }
    }

    /// 设置副标题。
    pub fn with_subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    /// 设置展开态。
    pub fn expanded(mut self, expanded: bool) -> Self {
        self.expanded = expanded;
        self
    }

    /// 设置视觉语义。
    pub fn with_tone(mut self, tone: RowTone) -> Self {
        self.tone = tone;
        self
    }

    /// 该行估算高度。
    pub fn estimated_height_px(&self) -> f32 {
        estimate_height_px(self.kind, self.expanded, self.body_lines)
    }
}

/// 单个轮次（用户提问 + Agent 回复 + 工具活动）的导航摘要。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TurnSnapshot {
    /// 轮次标识。
    pub id: TurnId,
    /// 轮次序号（0 基）。
    pub index: usize,
    /// 用户提问摘要（导航轨悬停气泡，前 48 字）。
    pub prompt_preview: String,
    /// 该轮在 `rows` 中的首个行下标。
    pub first_row: usize,
    /// 该轮是否含待人工审批（琥珀刻度）。
    pub needs_input: bool,
    /// 该轮是否含检查点（紫罗兰点缀），值为检查点序号。
    pub checkpoint: Option<u32>,
}

impl TurnSnapshot {
    /// 构造轮次摘要。
    pub fn new(
        id: TurnId,
        index: usize,
        prompt_preview: impl Into<String>,
        first_row: usize,
    ) -> Self {
        Self {
            id,
            index,
            prompt_preview: prompt_preview.into(),
            first_row,
            needs_input: false,
            checkpoint: None,
        }
    }

    /// 标记该轮含待审批。
    pub fn with_needs_input(mut self, needs_input: bool) -> Self {
        self.needs_input = needs_input;
        self
    }

    /// 标记该轮含检查点。
    pub fn with_checkpoint(mut self, ordinal: Option<u32>) -> Self {
        self.checkpoint = ordinal;
        self
    }
}

/// 一次渲染就绪的对话流快照。
///
/// 构建时一次性算好行高度量 [`RowMetrics`] 与轮次顶部偏移，渲染循环只做只读访问。
#[derive(Clone, Debug)]
pub struct TimelineSnapshot {
    turns: Rc<Vec<TurnSnapshot>>,
    rows: Rc<Vec<TimelineRow>>,
    metrics: Rc<RowMetrics>,
    turn_tops: Rc<Vec<f32>>,
    live_steps: Option<String>,
    reset_generation: u64,
}

impl PartialEq for TimelineSnapshot {
    /// 逐帧热路径：只比较指针与重置代次（`pages/01-desktop-workbench.md` §7.1）。
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.turns, &other.turns)
            && Rc::ptr_eq(&self.rows, &other.rows)
            && self.reset_generation == other.reset_generation
            && self.live_steps == other.live_steps
    }
}

impl Eq for TimelineSnapshot {}

impl Default for TimelineSnapshot {
    fn default() -> Self {
        Self::new(Vec::new(), Vec::new(), None)
    }
}

impl TimelineSnapshot {
    /// 由轮次、行与可选的 `LiveSteps` 单行文案构建快照。
    pub fn new(
        turns: Vec<TurnSnapshot>,
        rows: Vec<TimelineRow>,
        live_steps: Option<String>,
    ) -> Self {
        let metrics =
            RowMetrics::from_heights(rows.iter().map(TimelineRow::estimated_height_px).collect());
        let turn_tops = turns
            .iter()
            .map(|turn| metrics.top_of(turn.first_row))
            .collect();
        Self {
            turns: Rc::new(turns),
            rows: Rc::new(rows),
            metrics: Rc::new(metrics),
            turn_tops: Rc::new(turn_tops),
            live_steps,
            reset_generation: 0,
        }
    }

    /// 重置代次（会话切换时递增，导航轨据此清空悬停/焦点缓存）。
    pub fn with_reset_generation(mut self, generation: u64) -> Self {
        self.reset_generation = generation;
        self
    }

    /// 重置代次。
    pub fn reset_generation(&self) -> u64 {
        self.reset_generation
    }

    /// 轮次列表。
    pub fn turns(&self) -> &[TurnSnapshot] {
        &self.turns
    }

    /// 行列表。
    pub fn rows(&self) -> &[TimelineRow] {
        &self.rows
    }

    /// 行高度量（构建时算好）。
    pub fn metrics(&self) -> &RowMetrics {
        &self.metrics
    }

    /// 各轮次顶部偏移（px）。
    pub fn turn_tops(&self) -> &[f32] {
        &self.turn_tops
    }

    /// `LiveSteps` 折叠态单行文案（`None` 表示本次没有实时步骤）。
    pub fn live_steps(&self) -> Option<&str> {
        self.live_steps.as_deref()
    }

    /// 与另一份快照共享同一 `turns` / `rows` 指针（导航轨 `O(1)` 复用判定）。
    pub fn shares_storage_with(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.turns, &other.turns) && Rc::ptr_eq(&self.rows, &other.rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: RowId, kind: TimelineRowKind) -> TimelineRow {
        TimelineRow::new(id, kind, "行", 1)
    }

    fn snapshot() -> TimelineSnapshot {
        let rows = vec![
            row(0, TimelineRowKind::UserMessage),
            row(1, TimelineRowKind::AssistantMessage),
            row(2, TimelineRowKind::ToolCard),
            row(3, TimelineRowKind::Checkpoint),
            row(4, TimelineRowKind::UserMessage),
            row(5, TimelineRowKind::AssistantMessage),
        ];
        let turns = vec![
            TurnSnapshot::new(1, 0, "第一轮", 0),
            TurnSnapshot::new(2, 1, "第二轮", 4),
        ];
        TimelineSnapshot::new(turns, rows, None)
    }

    #[test]
    fn snapshot_computes_metrics_once() {
        let snapshot = snapshot();
        assert_eq!(snapshot.rows().len(), 6);
        assert_eq!(snapshot.metrics().len(), 6);
        // 总高度 == 各行估算高度之和。
        let sum: f32 = snapshot
            .rows()
            .iter()
            .map(|r| r.estimated_height_px())
            .sum();
        assert_eq!(snapshot.metrics().total_height(), sum);
        assert_eq!(
            snapshot.metrics().height(0),
            USER_TURN_HEIGHT_PX + BODY_LINE_HEIGHT_PX
        );
    }

    #[test]
    fn turn_tops_follow_first_row_offsets() {
        let snapshot = snapshot();
        let tops = snapshot.turn_tops();
        assert_eq!(tops.len(), 2);
        assert_eq!(tops[0], 0.0);
        // 第二轮从第 4 行开始 → 等于前 4 行高度之和。
        assert_eq!(tops[1], snapshot.metrics().top_of(4));
        assert!(tops[1] > tops[0]);
    }

    #[test]
    fn pointer_equality_is_used_for_perf_invariant() {
        let a = snapshot();
        // 克隆快照共享同一份 Rc 存储 → 相等（O(1) 指针比较）。
        let b = a.clone();
        assert_eq!(a, b);
        assert!(a.shares_storage_with(&b));

        // 内容完全相同但重新构建 → 不相等（指针不同），这正是导航轨避免逐帧深比较的语义。
        let c = snapshot();
        assert_ne!(a, c);
        assert!(!a.shares_storage_with(&c));

        // 重置代次变更必须立即被观测到。
        let d = a.clone().with_reset_generation(7);
        assert_ne!(a, d);
        assert_eq!(d.reset_generation(), 7);
    }

    #[test]
    fn live_steps_participates_in_equality() {
        let base = TimelineSnapshot::default();
        let with_steps = TimelineSnapshot::new(
            Vec::new(),
            Vec::new(),
            Some("实时步骤焦点 · 0/0 · 暂无步骤".to_string()),
        );
        assert_ne!(base, with_steps);
        assert_eq!(
            with_steps.live_steps(),
            Some("实时步骤焦点 · 0/0 · 暂无步骤")
        );
        assert_eq!(base.live_steps(), None);
    }

    #[test]
    fn height_estimate_is_monotonic_and_expansion_costs_more() {
        for kind in TimelineRowKind::ALL {
            let collapsed = estimate_height_px(kind, false, 3);
            let expanded = estimate_height_px(kind, true, 3);
            assert!(expanded >= collapsed, "{kind:?} 展开态不应比折叠态矮");
            let longer = estimate_height_px(kind, true, 30);
            assert!(longer >= expanded, "{kind:?} 行数增加高度不应下降");
        }
        assert_eq!(
            estimate_height_px(TimelineRowKind::ToolCard, false, 99),
            TOOL_CARD_FOLDED_HEIGHT_PX
        );
        assert_eq!(
            estimate_height_px(TimelineRowKind::Checkpoint, true, 99),
            CHECKPOINT_ANCHOR_HEIGHT_PX
        );
        assert!(TimelineRowKind::ApprovalGate.needs_human());
        assert!(!TimelineRowKind::ToolCard.needs_human());
    }

    #[test]
    fn row_tone_maps_to_semantic_tokens() {
        use crate::tokens::color::{dark, light};
        assert_eq!(RowTone::Neutral.dark_token(), dark::BORDER_SUBTLE);
        assert_eq!(RowTone::Running.dark_token(), dark::SEMANTIC_RUNNING);
        assert_eq!(RowTone::NeedsInput.dark_token(), dark::SEMANTIC_WARNING);
        assert_eq!(RowTone::Danger.dark_token(), dark::SEMANTIC_DANGER);
        assert_eq!(RowTone::Handoff.dark_token(), dark::SEMANTIC_HANDOFF);
        assert_eq!(RowTone::Danger.light_token(), light::SEMANTIC_DANGER);
        // 行默认中性色，构造器可覆盖。
        let plain = TimelineRow::new(7, TimelineRowKind::ToolCard, "修改 a.rs (+1 -0)", 1);
        assert_eq!(plain.tone, RowTone::Neutral);
        assert_eq!(plain.with_tone(RowTone::Danger).tone, RowTone::Danger);
    }

    #[test]
    fn empty_snapshot_is_safe() {
        let empty = TimelineSnapshot::default();
        assert!(empty.metrics().is_empty());
        assert_eq!(empty.metrics().total_height(), 0.0);
        assert!(empty.turn_tops().is_empty());
        assert_eq!(empty.reset_generation(), 0);
        // 越界 first_row 不 panic，钳到总高度。
        let snapshot = TimelineSnapshot::new(
            vec![TurnSnapshot::new(1, 0, "越界", 999)],
            vec![row(0, TimelineRowKind::UserMessage)],
            None,
        );
        let tops = snapshot.turn_tops();
        assert_eq!(tops.len(), 1);
        assert_eq!(tops[0], snapshot.metrics().total_height());
    }
}
