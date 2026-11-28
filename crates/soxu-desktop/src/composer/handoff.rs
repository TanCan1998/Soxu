//! `@agent` 移交胶囊与「移交范围」微开关（纯逻辑）。
//!
//! 规范来源：`pages/02-composer-and-handoff.md` §3.6 与 §4.1——
//! - 选择移交目标后**仅切换按钮自身选中态样式**（紫罗兰柔底 + 边框 + 角标），按钮内部
//!   的 `@` 图标与「移交」文字保持不变；
//! - 选中 `@codex` 后在输入框生成紫罗兰胶囊，并在右侧浮现三枚移交范围微开关：
//!   `[✓ 附带最近 3 轮结论摘要]`、`[✓ 附带当前工作树未提交 Diff 路径 (N files)]`、
//!   `[○ 在新建独立 Worktree 中启动]`。
//!
//! 契约类型全部复用 [`soxu_protocol`]（`HandoffSelection` / `HandoffScopeOptions` /
//! `ComposerChip`），本模块只补状态切换、标签文案与呈现行。

use soxu_protocol::{
    ComposerChip, ComposerChipKind, HandoffScopeOptions, HandoffSelection, ThinkingEffort,
};

/// 移交范围微开关的三枚开关键。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandoffScopeToggle {
    /// 附带最近 N 轮结论摘要。
    SummaryTurns,
    /// 附带当前工作树未提交 Diff 路径。
    UncommittedDiffPaths,
    /// 在新建独立 Worktree 中启动。
    SpawnWorktree,
}

impl HandoffScopeToggle {
    /// 三枚开关（渲染顺序即规范顺序）。
    pub const ALL: [Self; 3] = [
        Self::SummaryTurns,
        Self::UncommittedDiffPaths,
        Self::SpawnWorktree,
    ];

    /// 开关的中文标签（不含勾选符号与动态计数）。
    pub const fn label(self) -> &'static str {
        match self {
            Self::SummaryTurns => "附带最近 3 轮结论摘要",
            Self::UncommittedDiffPaths => "附带当前工作树未提交 Diff 路径",
            Self::SpawnWorktree => "在新建独立 Worktree 中启动",
        }
    }
}

/// 单枚微开关的呈现行。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandoffToggleRow {
    /// 开关键。
    pub toggle: HandoffScopeToggle,
    /// 是否处于开启态。
    pub enabled: bool,
    /// 勾选字形：开启 `✓`，关闭 `○`（非 Emoji）。
    pub mark: &'static str,
    /// 中文标签。
    pub label: &'static str,
    /// 动态后缀（如 `(3 files)`），无则为空串。
    pub suffix: String,
}

/// `@agent` 移交状态机。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandoffState {
    selection: Option<HandoffSelection>,
    scope: HandoffScopeOptions,
    /// 当前工作树未提交 Diff 的文件数（用于微开关后缀）。
    diff_file_count: u32,
}

impl HandoffState {
    /// 未选中任何移交目标；范围取协议默认值（3 轮摘要 + Diff 路径，不新建 Worktree）。
    pub fn new() -> Self {
        Self {
            selection: None,
            scope: HandoffScopeOptions::default(),
            diff_file_count: 0,
        }
    }

    /// 是否已选定移交目标（决定胶囊与`@`按钮的选中态样式）。
    pub fn is_active(&self) -> bool {
        self.selection.is_some()
    }

    /// 当前移交选型。
    pub fn selection(&self) -> Option<&HandoffSelection> {
        self.selection.as_ref()
    }

    /// 当前移交范围。
    pub fn scope(&self) -> HandoffScopeOptions {
        self.scope
    }

    /// 未提交 Diff 文件数。
    pub fn diff_file_count(&self) -> u32 {
        self.diff_file_count
    }

    /// 选定移交目标（`@codex` → 目标 Agent / 模型 / 思考强度）。
    pub fn select(&mut self, selection: HandoffSelection) {
        self.selection = Some(selection);
    }

    /// 取消移交（`@` 按钮回到常态样式，胶囊消失）。
    pub fn clear(&mut self) {
        self.selection = None;
    }

    /// 刷新未提交 Diff 文件数（纯内存计数，不含任何 Git 调用）。
    pub fn set_diff_file_count(&mut self, count: u32) {
        self.diff_file_count = count;
    }

    /// 移交胶囊文案：`@codex`。
    pub fn capsule_label(&self) -> Option<String> {
        self.selection
            .as_ref()
            .map(|selection| format!("@{}", selection.target_agent))
    }

    /// 移交胶囊的模型/思考摘要：`gpt-6.2-sol 中`（供胶囊悬浮说明）。
    pub fn capsule_detail(&self) -> Option<String> {
        let selection = self.selection.as_ref()?;
        let thinking = selection
            .target_thinking
            .map(thinking_label)
            .unwrap_or("默认");
        Some(format!("{} {}", selection.target_model, thinking))
    }

    /// 翻转某枚微开关。
    pub fn toggle(&mut self, toggle: HandoffScopeToggle) {
        match toggle {
            HandoffScopeToggle::SummaryTurns => {
                // 3 轮 ⇄ 0 轮（关闭即不附带结论摘要）。
                self.scope.summary_turns = if self.scope.summary_turns > 0 { 0 } else { 3 };
            }
            HandoffScopeToggle::UncommittedDiffPaths => {
                self.scope.include_uncommitted_diff_paths =
                    !self.scope.include_uncommitted_diff_paths;
            }
            HandoffScopeToggle::SpawnWorktree => {
                self.scope.spawn_worktree = !self.scope.spawn_worktree;
            }
        }
    }

    /// 某枚微开关当前是否开启。
    pub fn is_enabled(&self, toggle: HandoffScopeToggle) -> bool {
        match toggle {
            HandoffScopeToggle::SummaryTurns => self.scope.summary_turns > 0,
            HandoffScopeToggle::UncommittedDiffPaths => self.scope.include_uncommitted_diff_paths,
            HandoffScopeToggle::SpawnWorktree => self.scope.spawn_worktree,
        }
    }

    /// 三枚微开关的呈现行（顺序固定，供渲染层直接消费）。
    pub fn toggle_rows(&self) -> Vec<HandoffToggleRow> {
        HandoffScopeToggle::ALL
            .into_iter()
            .map(|toggle| HandoffToggleRow {
                toggle,
                enabled: self.is_enabled(toggle),
                mark: if self.is_enabled(toggle) {
                    "✓"
                } else {
                    "○"
                },
                label: toggle.label(),
                suffix: self.suffix(toggle),
            })
            .collect()
    }

    fn suffix(&self, toggle: HandoffScopeToggle) -> String {
        match (toggle, self.diff_file_count) {
            (HandoffScopeToggle::UncommittedDiffPaths, 0) => String::new(),
            (HandoffScopeToggle::UncommittedDiffPaths, 1) => "(1 file)".to_string(),
            (HandoffScopeToggle::UncommittedDiffPaths, n) => format!("({n} files)"),
            _ => String::new(),
        }
    }

    /// 生成移交胶囊芯片（`kind = Handoff`，携带目标 Agent/模型/思考）。
    ///
    /// 未选定目标时返回 `None`，调用方不应渲染胶囊。
    pub fn chip(&self, id: &str) -> Option<ComposerChip> {
        let selection = self.selection.as_ref()?;
        Some(ComposerChip {
            id: id.to_string(),
            kind: ComposerChipKind::Handoff,
            label: self.capsule_label().unwrap_or_default(),
            path_or_ref: None,
            target_agent: Some(selection.target_agent.clone()),
            target_model: Some(selection.target_model.clone()),
            target_thinking: selection.target_thinking,
        })
    }
}

impl Default for HandoffState {
    fn default() -> Self {
        Self::new()
    }
}

/// 思考强度的中文单字标签（`低 / 中 / 高 / 极高 / 最高`）。
pub fn thinking_label(effort: ThinkingEffort) -> &'static str {
    match effort {
        ThinkingEffort::Low => "低",
        ThinkingEffort::Medium => "中",
        ThinkingEffort::High => "高",
        ThinkingEffort::Xhigh => "极高",
        ThinkingEffort::Max => "最高",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codex_selection() -> HandoffSelection {
        HandoffSelection {
            target_agent: "codex".to_string(),
            target_model: "gpt-6.2-sol".to_string(),
            target_thinking: Some(ThinkingEffort::Medium),
        }
    }

    /// 默认态：未选定目标，范围取协议默认（3 轮摘要 + Diff 路径，不新建 Worktree）。
    #[test]
    fn defaults_match_protocol_contract() {
        let state = HandoffState::new();
        assert!(!state.is_active());
        assert_eq!(state.scope().summary_turns, 3);
        assert!(state.scope().include_uncommitted_diff_paths);
        assert!(!state.scope().spawn_worktree);
        assert!(state.chip("chip-1").is_none());
        assert!(state.capsule_label().is_none());
    }

    /// 选定目标后生成 `@codex` 胶囊芯片，且携带模型与思考强度。
    #[test]
    fn selecting_target_builds_handoff_chip() {
        let mut state = HandoffState::new();
        state.select(codex_selection());
        assert!(state.is_active());
        assert_eq!(state.capsule_label().as_deref(), Some("@codex"));
        assert_eq!(state.capsule_detail().as_deref(), Some("gpt-6.2-sol 中"));

        let chip = state.chip("chip-1").expect("已选定目标应产出芯片");
        assert_eq!(chip.kind, ComposerChipKind::Handoff);
        assert_eq!(chip.label, "@codex");
        assert_eq!(chip.target_agent.as_deref(), Some("codex"));
        assert_eq!(chip.target_model.as_deref(), Some("gpt-6.2-sol"));
        assert_eq!(chip.target_thinking, Some(ThinkingEffort::Medium));
    }

    /// 三枚微开关独立翻转，互不影响。
    #[test]
    fn scope_toggles_are_independent() {
        let mut state = HandoffState::new();

        state.toggle(HandoffScopeToggle::SummaryTurns);
        assert_eq!(state.scope().summary_turns, 0);
        assert!(!state.is_enabled(HandoffScopeToggle::SummaryTurns));
        assert!(state.scope().include_uncommitted_diff_paths);
        assert!(!state.scope().spawn_worktree);

        state.toggle(HandoffScopeToggle::UncommittedDiffPaths);
        assert!(!state.scope().include_uncommitted_diff_paths);

        state.toggle(HandoffScopeToggle::SpawnWorktree);
        assert!(state.scope().spawn_worktree);

        // 再翻转一次复原。
        state.toggle(HandoffScopeToggle::SummaryTurns);
        assert_eq!(state.scope().summary_turns, 3);
        state.toggle(HandoffScopeToggle::UncommittedDiffPaths);
        assert!(state.scope().include_uncommitted_diff_paths);
        state.toggle(HandoffScopeToggle::SpawnWorktree);
        assert!(!state.scope().spawn_worktree);
    }

    /// 呈现行：三行顺序固定，勾选字形随开关状态变化。
    #[test]
    fn toggle_rows_render_marks() {
        let mut state = HandoffState::new();
        state.set_diff_file_count(3);

        let rows = state.toggle_rows();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].toggle, HandoffScopeToggle::SummaryTurns);
        assert_eq!(rows[0].mark, "✓");
        assert_eq!(rows[1].mark, "✓");
        // 未启用交付的 Worktree 开关显示空心圈。
        assert_eq!(rows[2].mark, "○");
        assert_eq!(rows[1].label, "附带当前工作树未提交 Diff 路径");
        assert_eq!(rows[1].suffix, "(3 files)");

        state.toggle(HandoffScopeToggle::SpawnWorktree);
        assert_eq!(state.toggle_rows()[2].mark, "✓");

        // 单文件时后缀用单数。
        state.set_diff_file_count(1);
        assert_eq!(state.toggle_rows()[1].suffix, "(1 file)");
        // 0 文件时不显示括号。
        state.set_diff_file_count(0);
        assert!(state.toggle_rows()[1].suffix.is_empty());
    }

    /// 取消移交后回到常态：胶囊消失、微开关保留用户上次选择。
    #[test]
    fn clearing_keeps_scope_but_drops_chip() {
        let mut state = HandoffState::new();
        state.select(codex_selection());
        state.toggle(HandoffScopeToggle::SpawnWorktree);
        state.clear();

        assert!(!state.is_active());
        assert!(state.chip("chip-1").is_none());
        assert!(state.scope().spawn_worktree);
    }

    /// 思考强度标签覆盖全部 5 档且互不相同。
    #[test]
    fn thinking_labels_cover_all_efforts() {
        let labels: Vec<&str> = [
            ThinkingEffort::Low,
            ThinkingEffort::Medium,
            ThinkingEffort::High,
            ThinkingEffort::Xhigh,
            ThinkingEffort::Max,
        ]
        .into_iter()
        .map(thinking_label)
        .collect();
        assert_eq!(labels, vec!["低", "中", "高", "极高", "最高"]);
    }

    /// 未设置思考强度时胶囊说明回落「默认」。
    #[test]
    fn capsule_detail_defaults_thinking() {
        let mut state = HandoffState::new();
        state.select(HandoffSelection {
            target_agent: "pi".to_string(),
            target_model: "deepseek-v4.1-flash".to_string(),
            target_thinking: None,
        });
        assert_eq!(
            state.capsule_detail().as_deref(),
            Some("deepseek-v4.1-flash 默认")
        );
    }
}
