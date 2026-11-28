//! 轮次检查点锚点、回退三选一与 `LiveSteps` 实时步骤焦点条（纯逻辑）。
//!
//! 设计基准：`pages/03-timeline-tools-and-checkpoints.md` §4（轮次检查点与安全回退）
//! 与 §7.2（`LiveSteps` 默认折叠单行）。
//!
//! 两条硬约束：
//! 1. **绝不静默覆写工作树**：回退必须先经过三选一确认，且「仅回退代码」与
//!    「代码对话同步回退」都必须先创建保底备份引用（Safety Stash Ref），
//!    因此两者都被标注为「需要备份」；
//! 2. `LiveSteps` **默认折叠成单行**微条，展开后才按 `1 → N` 单列垂直呈现。

use crate::theme::tokens::color::dark;
use crate::theme::tokens::color::light;
use crate::timeline::model::{
    DETAIL_LINE_HEIGHT_PX as MODEL_DETAIL_LINE_HEIGHT_PX,
    LIVE_STEPS_HEIGHT_PX as MODEL_LIVE_STEPS_HEIGHT_PX, RowId, RowTone, TimelineRow,
    TimelineRowKind,
};
use crate::timeline::text;

/// 检查点锚点横条高度（px）：设计指定高 `26px` 的 `#121212` 胶囊（03 §4）。
///
/// 令牌里没有独立的锚点高度，故与 [`crate::timeline::model::CHECKPOINT_ANCHOR_HEIGHT_PX`]
/// 同源取值，避免两处写死不同数字。
pub const ANCHOR_HEIGHT_PX: f32 = crate::timeline::model::CHECKPOINT_ANCHOR_HEIGHT_PX;

/// 轮次检查点摘要（与 `soxu-git` 的 `CheckpointSummary` 同构的最小视图）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckpointSummary {
    /// 检查点序号（1 基）。
    pub ordinal: u32,
    /// 提交 OID（完整 40/64 位，渲染时截断为 7 位）。
    pub commit: String,
    /// 变更文件数。
    pub files: u32,
    /// 新增行数。
    pub additions: u32,
    /// 删除行数。
    pub deletions: u32,
}

impl CheckpointSummary {
    /// 构造检查点摘要。
    pub fn new(
        ordinal: u32,
        commit: impl Into<String>,
        files: u32,
        additions: u32,
        deletions: u32,
    ) -> Self {
        Self {
            ordinal,
            commit: commit.into(),
            files,
            additions,
            deletions,
        }
    }

    /// 短提交号（前 7 位；不足 7 位原样返回）。
    pub fn short_commit(&self) -> &str {
        let end = self.commit.len().min(7);
        self.commit.get(..end).unwrap_or("")
    }

    /// 锚点横条文案：`检查点 #04 · commit 7c4e91a · 变更 4 个文件 (+86 -14)`。
    pub fn label(&self) -> String {
        text::checkpoint_label(
            self.ordinal,
            self.short_commit(),
            self.files,
            self.additions,
            self.deletions,
        )
    }

    /// 锚点图标语义色令牌（`--soxu-semantic-handoff` 紫罗兰，深色模式）。
    pub fn dark_accent_token() -> u32 {
        dark::SEMANTIC_HANDOFF
    }

    /// 锚点图标语义色令牌（浅色模式）。
    pub fn light_accent_token() -> u32 {
        light::SEMANTIC_HANDOFF
    }

    /// 转为对话流行（紫罗兰语义色 + 居中锚点横条）。
    pub fn row(&self, id: RowId) -> TimelineRow {
        TimelineRow::new(id, TimelineRowKind::Checkpoint, self.label(), 0)
            .with_tone(RowTone::Handoff)
    }
}

/// 回退三选一确认框的选项。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RewindChoice {
    /// 仅分叉对话（保留当前磁盘代码不变）。
    ForkOnly,
    /// 仅回退工作树代码至该快照（保留后续对话记录）。
    FilesOnly,
    /// 代码与对话同步回退。
    Both,
}

impl RewindChoice {
    /// 三选一全部选项（顺序即呈现顺序）。
    pub const ALL: [Self; 3] = [Self::ForkOnly, Self::FilesOnly, Self::Both];

    /// 选项文案。
    pub const fn label(self) -> &'static str {
        match self {
            Self::ForkOnly => "仅分叉对话（保留当前磁盘代码不变）",
            Self::FilesOnly => "仅回退工作树代码至该快照（保留后续对话记录）",
            Self::Both => "代码与对话同步回退",
        }
    }

    /// 是否必须先创建保底备份引用（Safety Stash Ref）。
    ///
    /// 「仅分叉」不动磁盘，因此不需要备份；另外两项都会改写工作树。
    pub const fn requires_safety_backup(self) -> bool {
        matches!(self, Self::FilesOnly | Self::Both)
    }

    /// 是否改写工作树磁盘内容。
    pub const fn mutates_worktree(self) -> bool {
        self.requires_safety_backup()
    }
}

/// 回退三选一确认框标题。
pub const REWIND_CONFIRM_TITLE: &str = text::REWIND_CONFIRM_TITLE;

/// 步骤状态。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepState {
    /// 尚未开始。
    Pending,
    /// 进行中。
    Active,
    /// 已完成。
    Done,
    /// 被门禁阻塞（例如等待人工确认）。
    Blocked,
}

impl StepState {
    /// 该步骤是否计入已完成数。
    pub const fn is_done(self) -> bool {
        matches!(self, Self::Done)
    }
}

/// 单个实时步骤。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveStep {
    /// 步骤标题。
    pub title: String,
    /// 步骤状态。
    pub state: StepState,
}

impl LiveStep {
    /// 构造步骤。
    pub fn new(title: impl Into<String>, state: StepState) -> Self {
        Self {
            title: title.into(),
            state,
        }
    }
}

/// 实时步骤焦点条（`LiveSteps`）。
///
/// 默认**折叠成单行**（30px 脉冲条）；展开后按 `1 → N` 单列垂直呈现。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LiveSteps {
    /// 步骤列表（顺序即执行顺序）。
    pub steps: Vec<LiveStep>,
    /// 是否展开。
    expanded: bool,
}

impl LiveSteps {
    /// 构造步骤条（默认折叠）。
    pub fn new(steps: Vec<LiveStep>) -> Self {
        Self {
            steps,
            expanded: false,
        }
    }

    /// 是否展开（新建时恒为 `false`，即默认折叠单行）。
    pub fn expanded(&self) -> bool {
        self.expanded
    }

    /// 切换展开/收起。
    pub fn toggle(&mut self) {
        self.expanded = !self.expanded;
    }

    /// 步骤总数。
    pub fn total(&self) -> usize {
        self.steps.len()
    }

    /// 已完成步骤数。
    pub fn done(&self) -> usize {
        self.steps
            .iter()
            .filter(|step| step.state.is_done())
            .count()
    }

    /// 当前焦点步骤：优先进行中，其次是阻塞，最后是下一个待开始步骤。
    pub fn current(&self) -> Option<&LiveStep> {
        self.steps
            .iter()
            .find(|step| step.state == StepState::Active)
            .or_else(|| {
                self.steps
                    .iter()
                    .find(|step| step.state == StepState::Blocked)
            })
            .or_else(|| {
                self.steps
                    .iter()
                    .find(|step| step.state == StepState::Pending)
            })
    }

    /// 是否存在阻塞步骤（界面需要琥珀/绯红提示）。
    pub fn has_blocked(&self) -> bool {
        self.steps
            .iter()
            .any(|step| step.state == StepState::Blocked)
    }

    /// 折叠态单行文案：`实时步骤焦点 · 2/4 · 等待人工确认安全门禁与目录清理边界`。
    pub fn collapsed_label(&self) -> String {
        text::live_steps_label(
            self.done(),
            self.total(),
            self.current().map(|step| step.title.as_str()).unwrap_or(""),
        )
    }

    /// 展开态的有序条目：`(1 基序号, 步骤)`。
    pub fn ordered(&self) -> impl Iterator<Item = (usize, &LiveStep)> {
        self.steps
            .iter()
            .enumerate()
            .map(|(index, step)| (index + 1, step))
    }

    /// 折叠态/展开态占位高度（px）。
    pub fn height_px(&self) -> f32 {
        if self.expanded {
            MODEL_LIVE_STEPS_HEIGHT_PX + self.total() as f32 * MODEL_DETAIL_LINE_HEIGHT_PX
        } else {
            MODEL_LIVE_STEPS_HEIGHT_PX
        }
    }

    /// 转为对话流行（进行中 → 电光蓝，有阻塞 → 琥珀，否则中性）。
    pub fn row(&self, id: RowId) -> TimelineRow {
        let tone = if self.has_blocked() {
            RowTone::NeedsInput
        } else if self
            .steps
            .iter()
            .any(|step| step.state == StepState::Active)
        {
            RowTone::Running
        } else {
            RowTone::Neutral
        };
        TimelineRow::new(
            id,
            TimelineRowKind::LiveSteps,
            self.collapsed_label(),
            self.total(),
        )
        .expanded(self.expanded)
        .with_tone(tone)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkpoint_label_truncates_commit_to_seven() {
        let summary =
            CheckpointSummary::new(4, "7c4e91a8b2f9c0d1e2f3a4b5c6d7e8f901234567", 4, 86, 14);
        assert_eq!(summary.short_commit(), "7c4e91a");
        assert_eq!(
            summary.label(),
            "检查点 #04 · commit 7c4e91a · 变更 4 个文件 (+86 -14)"
        );
        // 短 OID 原样返回，不越界。
        assert_eq!(
            CheckpointSummary::new(1, "abc", 0, 0, 0).short_commit(),
            "abc"
        );
        assert_eq!(
            CheckpointSummary::new(1, "abc", 0, 0, 0).label(),
            "检查点 #01 · commit abc · 变更 0 个文件 (+0 -0)"
        );
        assert_eq!(
            CheckpointSummary::dark_accent_token(),
            dark::SEMANTIC_HANDOFF
        );
        assert_eq!(
            CheckpointSummary::light_accent_token(),
            light::SEMANTIC_HANDOFF
        );
    }

    #[test]
    fn rewind_choices_never_overwrite_silently() {
        assert_eq!(RewindChoice::ALL.len(), 3);
        assert!(!REWIND_CONFIRM_TITLE.is_empty());
        // 仅分叉不改盘；另两项改盘，因此必须先备份。
        assert!(!RewindChoice::ForkOnly.mutates_worktree());
        assert!(!RewindChoice::ForkOnly.requires_safety_backup());
        for choice in [RewindChoice::FilesOnly, RewindChoice::Both] {
            assert!(choice.mutates_worktree(), "{choice:?} 应改写工作树");
            assert!(
                choice.requires_safety_backup(),
                "{choice:?} 必须先创建保底备份引用"
            );
            assert!(!choice.label().is_empty());
        }
    }

    #[test]
    fn live_steps_defaults_to_collapsed_single_line() {
        let mut steps = LiveSteps::new(vec![
            LiveStep::new("解析 Issue 与设计条款", StepState::Done),
            LiveStep::new("实现虚拟时间线", StepState::Active),
            LiveStep::new("等待人工确认安全门禁与目录清理边界", StepState::Blocked),
            LiveStep::new("收尾 AOCI 与门禁", StepState::Pending),
        ]);
        // 默认折叠（单行）。
        assert!(!steps.expanded());
        assert_eq!(steps.total(), 4);
        assert_eq!(steps.done(), 1);
        assert!(steps.has_blocked());
        // 当前焦点：进行中优先于阻塞。
        assert_eq!(
            steps.current().map(|step| step.title.as_str()),
            Some("实现虚拟时间线")
        );
        assert_eq!(
            steps.collapsed_label(),
            "实时步骤焦点 · 1/4 · 实现虚拟时间线"
        );
        assert_eq!(
            steps.height_px(),
            crate::timeline::model::LIVE_STEPS_HEIGHT_PX
        );

        // 展开：严格 1 → N 单列。
        steps.toggle();
        assert!(steps.expanded());
        let ordered: Vec<(usize, &str)> = steps
            .ordered()
            .map(|(index, step)| (index, step.title.as_str()))
            .collect();
        assert_eq!(ordered.len(), 4);
        assert_eq!(ordered[0].0, 1);
        assert_eq!(ordered[3].0, 4);
        assert!(steps.height_px() > crate::timeline::model::LIVE_STEPS_HEIGHT_PX);
        steps.toggle();
        assert!(!steps.expanded());
    }

    #[test]
    fn live_steps_current_falls_back_to_blocked_then_pending() {
        let blocked = LiveSteps::new(vec![
            LiveStep::new("已完成", StepState::Done),
            LiveStep::new("等待人工确认", StepState::Blocked),
        ]);
        assert_eq!(
            blocked.current().map(|step| step.title.as_str()),
            Some("等待人工确认")
        );
        assert!(blocked.collapsed_label().contains("1/2"));

        let pending = LiveSteps::new(vec![
            LiveStep::new("已完成", StepState::Done),
            LiveStep::new("下一步", StepState::Pending),
        ]);
        assert_eq!(
            pending.current().map(|step| step.title.as_str()),
            Some("下一步")
        );

        // 全部完成：无当前步骤，标签不带尾段。
        let finished = LiveSteps::new(vec![LiveStep::new("收尾", StepState::Done)]);
        assert!(!finished.has_blocked());
        assert_eq!(finished.current(), None);
        assert_eq!(finished.collapsed_label(), "实时步骤焦点 · 1/1");

        // 空步骤条安全。
        let empty = LiveSteps::default();
        assert_eq!(empty.total(), 0);
        assert_eq!(empty.collapsed_label(), "实时步骤焦点 · 0/0 · 暂无步骤");
        assert_eq!(
            empty.height_px(),
            crate::timeline::model::LIVE_STEPS_HEIGHT_PX
        );
    }

    #[test]
    fn checkpoint_and_live_steps_project_into_rows() {
        let checkpoint = CheckpointSummary::new(4, "7c4e91a8b2f9", 4, 86, 14);
        let row = checkpoint.row(1);
        assert_eq!(row.kind, TimelineRowKind::Checkpoint);
        assert_eq!(row.tone, RowTone::Handoff);
        assert!(!row.expanded);

        // 进行中 → 电光蓝；有阻塞 → 琥珀；全部完成 → 中性。
        let running = LiveSteps::new(vec![LiveStep::new("实现", StepState::Active)]);
        assert_eq!(running.row(2).tone, RowTone::Running);
        let blocked = LiveSteps::new(vec![LiveStep::new("等待门禁", StepState::Blocked)]);
        assert_eq!(blocked.row(3).tone, RowTone::NeedsInput);
        let done = LiveSteps::new(vec![LiveStep::new("收尾", StepState::Done)]);
        let row = done.row(4);
        assert_eq!(row.tone, RowTone::Neutral);
        assert_eq!(row.kind, TimelineRowKind::LiveSteps);
        assert_eq!(row.body_lines, 1);
    }

    #[test]
    fn anchor_height_never_exceeds_26px() {
        // 设计：高 26px 的胶囊。
        assert_eq!(ANCHOR_HEIGHT_PX, 26.0);
        assert_eq!(
            crate::timeline::model::CHECKPOINT_ANCHOR_HEIGHT_PX,
            ANCHOR_HEIGHT_PX
        );
    }
}
