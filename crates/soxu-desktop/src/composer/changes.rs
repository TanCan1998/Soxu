//! 变更摘要浮药丸与输入舱内存动作回调（无 I/O、无 daemon 依赖）。
//!
//! 规范来源：`pages/02-composer-and-handoff.md` §2.1——
//! 工作区会话有变更时，在输入舱上方居中渲染简洁药丸 `[N 个文件已更改 +A -D]`，
//! 点击直达 Diff 审阅面板。
//!
//! 「点击直达」在本阶段实现为**纯内存动作回调**：点击把 [`ComposerAction`] 压入进程内
//! 动作槽（[`ActionSink`]），由持有焦点栈的视图消费并切换
//! [`crate::focus::FocusLayer::ReviewDiff`]。不触碰文件系统、Git 或子进程。

use std::sync::Mutex;

use crate::focus::FocusLayer;

/// 变更摘要快照（文件数 / 新增行 / 删除行）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChangeSummary {
    /// 变更文件数。
    pub files: u32,
    /// 新增行数。
    pub additions: u32,
    /// 删除行数。
    pub deletions: u32,
}

impl ChangeSummary {
    /// 由三项计数构造。
    pub const fn new(files: u32, additions: u32, deletions: u32) -> Self {
        Self {
            files,
            additions,
            deletions,
        }
    }

    /// 是否没有变更（无变更时不渲染药丸）。
    pub const fn is_empty(&self) -> bool {
        self.files == 0 && self.additions == 0 && self.deletions == 0
    }

    /// 药丸文案：`3 个文件已更改 +47 -4`。
    pub fn pill_label(&self) -> String {
        format!(
            "{} 个文件已更改 +{} -{}",
            self.files, self.additions, self.deletions
        )
    }

    /// 无障碍文案：`查看 3 个变更文件的 Diff`。
    pub fn aria_label(&self) -> String {
        format!("查看 {} 个变更文件的 Diff", self.files)
    }
}

/// 输入舱可发出的内存动作（点击回调的统一载荷）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComposerAction {
    /// 点击变更摘要药丸：聚焦 Diff 审阅栏。
    FocusDiff,
    /// `Alt+P` / 点击权限按钮：循环权限档位。
    CyclePermission,
    /// 打开权限档位浮层。
    OpenPermissionMenu,
    /// 点击 `@`：打开移交配置面板。
    OpenHandoffPanel,
    /// 翻转某枚移交范围微开关。
    ToggleHandoffScope(&'static str),
    /// 打开 `+` 更多操作菜单。
    OpenPlusMenu,
    /// 打开上下文用量面板。
    OpenContextUsage,
    /// 打开会话统计详单浮层。
    OpenSessionStats,
    /// 插入并执行某条斜杠命令。
    RunSlashCommand(&'static str),
    /// 移除某枚上下文芯片。
    RemoveChip(String),
}

impl ComposerAction {
    /// 该动作是否需要切换焦点层，以及切到哪一层。
    pub fn focus_target(&self) -> Option<FocusLayer> {
        match self {
            Self::FocusDiff => Some(FocusLayer::ReviewDiff),
            Self::OpenHandoffPanel => Some(FocusLayer::AutocompletePopup),
            Self::OpenPlusMenu => Some(FocusLayer::AutocompletePopup),
            Self::CyclePermission
            | Self::OpenPermissionMenu
            | Self::ToggleHandoffScope(_)
            | Self::OpenContextUsage
            | Self::OpenSessionStats
            | Self::RunSlashCommand(_)
            | Self::RemoveChip(_) => None,
        }
    }

    /// 动作的稳定键（供日志/审计与测试断言，不含任何用户数据）。
    pub fn kind(&self) -> &'static str {
        match self {
            Self::FocusDiff => "focus_diff",
            Self::CyclePermission => "cycle_permission",
            Self::OpenPermissionMenu => "open_permission_menu",
            Self::OpenHandoffPanel => "open_handoff_panel",
            Self::ToggleHandoffScope(_) => "toggle_handoff_scope",
            Self::OpenPlusMenu => "open_plus_menu",
            Self::OpenContextUsage => "open_context_usage",
            Self::OpenSessionStats => "open_session_stats",
            Self::RunSlashCommand(_) => "run_slash_command",
            Self::RemoveChip(_) => "remove_chip",
        }
    }
}

/// 进程内动作槽：`render` 期间零阻塞只读，点击回调只做一次 push。
///
/// 这里刻意不使用 `gpui::Context`：本阶段输入舱是纯视图模型，动作由外部视图在下一帧
/// 消费（M5 接线后改为 `cx.emit`）。槽位用 `Mutex` 保护，避免 `unsafe`（工作区禁用）。
static ACTIONS: Mutex<Vec<ComposerAction>> = Mutex::new(Vec::new());

/// 动作槽句柄（仅方法，不可实例化）。
#[derive(Clone, Copy, Debug)]
pub struct ActionSink;

impl ActionSink {
    /// 压入一个动作。
    pub fn push(action: ComposerAction) {
        if let Ok(mut queue) = ACTIONS.lock() {
            queue.push(action);
        }
    }

    /// 取走全部待消费动作（先进先出）。
    pub fn drain() -> Vec<ComposerAction> {
        match ACTIONS.lock() {
            Ok(mut queue) => std::mem::take(&mut *queue),
            Err(_) => Vec::new(),
        }
    }

    /// 窥视当前队列（不清空）；主要供测试断言与调试。
    pub fn snapshot() -> Vec<ComposerAction> {
        match ACTIONS.lock() {
            Ok(queue) => queue.clone(),
            Err(_) => Vec::new(),
        }
    }

    /// 清空队列。
    pub fn clear() {
        if let Ok(mut queue) = ACTIONS.lock() {
            queue.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::focus::FocusStack;

    /// 空变更不渲染药丸。
    #[test]
    fn empty_summary_is_hidden() {
        assert!(ChangeSummary::default().is_empty());
        assert!(!ChangeSummary::new(1, 0, 0).is_empty());
        assert!(!ChangeSummary::new(0, 3, 0).is_empty());
    }

    /// 药丸文案与 02 页规范一致：`3 个文件已更改 +47 -4`。
    #[test]
    fn pill_label_matches_spec() {
        let summary = ChangeSummary::new(3, 47, 4);
        assert_eq!(summary.pill_label(), "3 个文件已更改 +47 -4");
        assert_eq!(summary.aria_label(), "查看 3 个变更文件的 Diff");
    }

    /// 点击药丸 → 聚焦 Diff：动作映射到 `ReviewDiff` 焦点层，且焦点栈接受入栈。
    #[test]
    fn change_pill_focuses_review_diff() {
        let action = ComposerAction::FocusDiff;
        assert_eq!(action.focus_target(), Some(FocusLayer::ReviewDiff));
        assert_eq!(action.kind(), "focus_diff");

        let mut stack = FocusStack::new();
        stack.push(FocusLayer::ComposerInput);
        let target = action.focus_target().expect("聚焦动作应有目标层");
        assert_eq!(
            stack.push(target),
            crate::focus::FocusDecision::Allowed,
            "从输入舱切到审阅栏应被允许"
        );
        assert_eq!(stack.active(), Some(FocusLayer::ReviewDiff));
    }

    /// 动作槽先进先出，`drain` 后清空。
    #[test]
    fn action_sink_is_fifo_and_drains() {
        ActionSink::clear();
        ActionSink::push(ComposerAction::FocusDiff);
        ActionSink::push(ComposerAction::CyclePermission);
        assert_eq!(ActionSink::snapshot().len(), 2);

        let drained = ActionSink::drain();
        assert_eq!(
            drained,
            vec![ComposerAction::FocusDiff, ComposerAction::CyclePermission]
        );
        assert!(ActionSink::snapshot().is_empty());
        assert!(ActionSink::drain().is_empty());
    }

    /// 只弹面板类动作不改变焦点层；移交/更多菜单走补全浮层。
    #[test]
    fn focus_targets_only_for_layer_switching_actions() {
        assert_eq!(ComposerAction::CyclePermission.focus_target(), None);
        assert_eq!(ComposerAction::OpenSessionStats.focus_target(), None);
        assert_eq!(
            ComposerAction::OpenHandoffPanel.focus_target(),
            Some(FocusLayer::AutocompletePopup)
        );
        assert_eq!(
            ComposerAction::OpenPlusMenu.focus_target(),
            Some(FocusLayer::AutocompletePopup)
        );
        assert_eq!(
            ComposerAction::RemoveChip("chip-1".to_string()).focus_target(),
            None
        );
    }

    /// 动作稳定键覆盖全部变体且互不相同。
    #[test]
    fn action_kinds_are_unique() {
        let actions = vec![
            ComposerAction::FocusDiff,
            ComposerAction::CyclePermission,
            ComposerAction::OpenPermissionMenu,
            ComposerAction::OpenHandoffPanel,
            ComposerAction::ToggleHandoffScope("summary"),
            ComposerAction::OpenPlusMenu,
            ComposerAction::OpenContextUsage,
            ComposerAction::OpenSessionStats,
            ComposerAction::RunSlashCommand("/plan"),
            ComposerAction::RemoveChip("chip-1".to_string()),
        ];
        let mut kinds: Vec<&str> = actions.iter().map(ComposerAction::kind).collect();
        kinds.sort_unstable();
        let before = kinds.len();
        kinds.dedup();
        assert_eq!(kinds.len(), before);
    }
}
