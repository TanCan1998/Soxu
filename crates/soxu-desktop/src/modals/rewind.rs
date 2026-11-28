//! Checkpoint 回退窗（`pages/06-modals-onboarding-and-automation.md` §5 边界表）。
//!
//! 规范要点：
//! - 几何 `w = 480px`、`p(20)`、`rounded(18)`（见 [`crate::modals::ModalKind`]）；
//! - 首帧焦点默认落在**安全项**「仅分叉对话」，绝不默认落在改写工作树的选项；
//! - **三选一互斥**：仅分叉对话 / 对话与工作树 / 取消，任一时刻至多一个被选中；
//! - **禁止静默覆写**：改写工作树必须先显式确认，且存在冲突文件时拒绝执行并给出可执行的
//!   `git status` 恢复指引；
//! - **Safety Stash 绝不删除**：无论成功、失败或被取消，已建好的安全快照句柄都保留。
//!
//! 阶段边界（Issue #133）：纯内存状态机，不执行任何 Git 操作；真实回退经
//! `soxu-protocol` 的 `RewindPlanPreview` / `RewindActionScope` 在 M6 由 daemon 承担。

/// 回退三选一的互斥选项。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RewindChoice {
    /// 仅分叉对话（最安全，不动工作树）。
    ForkConversationOnly,
    /// 对话与工作树一并回退（破坏性，需显式确认）。
    ConversationAndWorktree,
    /// 取消。
    Cancel,
}

impl RewindChoice {
    /// 全部选项（Dashboard 顺序：安全 → 破坏性 → 取消）。
    pub const ALL: [Self; 3] = [
        Self::ForkConversationOnly,
        Self::ConversationAndWorktree,
        Self::Cancel,
    ];

    /// 首帧默认焦点项（安全项）。
    pub const SAFE_DEFAULT: Self = Self::ForkConversationOnly;

    /// 选项文案。
    pub const fn label(self) -> &'static str {
        match self {
            Self::ForkConversationOnly => "仅分叉对话",
            Self::ConversationAndWorktree => "对话与工作树一并回退",
            Self::Cancel => "取消",
        }
    }

    /// 是否破坏性（会改写工作树内容）。
    pub const fn is_destructive(self) -> bool {
        matches!(self, Self::ConversationAndWorktree)
    }

    /// 是否需要二次显式确认（破坏性选项）。
    pub const fn requires_confirmation(self) -> bool {
        self.is_destructive()
    }

    /// 循环移动焦点（`delta` 为 `±1`，越界回绕）。
    pub fn cycled(self, delta: i32) -> Self {
        let len = i32::try_from(Self::ALL.len()).unwrap_or(3);
        let index = Self::ALL
            .iter()
            .position(|choice| *choice == self)
            .and_then(|index| i32::try_from(index).ok())
            .unwrap_or(0);
        let next = (index + delta).rem_euclid(len);
        Self::ALL[usize::try_from(next).unwrap_or(0)]
    }
}

/// 回退执行错误（全部为「不做任何事」的拒绝态，不包含静默覆写路径）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RewindError {
    /// 尚未选择任何选项（三选一未决，禁止默默沿用上一步）。
    NoChoiceSelected,
    /// 用户选择取消。
    Cancelled,
    /// 破坏性选项尚未确认。
    NeedsConfirmation,
    /// 存在冲突文件，工作树回退被拒绝。
    ConflictsPresent,
}

impl RewindError {
    /// 面向用户的拒绝文案。
    pub const fn message(self) -> &'static str {
        match self {
            Self::NoChoiceSelected => "请先选择一种回退方式",
            Self::Cancelled => "已取消回退，未改动任何文件",
            Self::NeedsConfirmation => "工作树回退需要勾选确认后才能执行",
            Self::ConflictsPresent => "存在冲突文件，已阻止工作树回退",
        }
    }
}

/// 校验通过的回退请求（交由上层/daemon 执行）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RewindRequest {
    /// 目标检查点标识（如 `#04`）。
    pub target: String,
    /// 已确认的回退方式（不会是 [`RewindChoice::Cancel`]）。
    pub choice: RewindChoice,
    /// 是否保留工作树未提交改动（仅分叉对话时为 `true`）。
    pub keeps_worktree: bool,
}

impl RewindRequest {
    /// 是否触及工作树。
    pub const fn touches_worktree(&self) -> bool {
        !self.keeps_worktree
    }
}

/// 回退窗状态。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RewindModal {
    /// 目标检查点标识。
    pub target: String,
    /// 当前获得键盘焦点的选项。
    pub focused: RewindChoice,
    /// 已选中的选项（`None` 表示未决）。
    pub selected: Option<RewindChoice>,
    /// 破坏性选项的显式确认开关。
    pub confirmed: bool,
    /// 冲突文件列表（非空即阻止工作树回退）。
    pub conflicts: Vec<String>,
    /// 已建立的安全快照句柄（只读保留，绝不删除）。
    pub safety_stash: Option<String>,
    /// 最近一次拒绝原因。
    pub last_error: Option<RewindError>,
}

impl RewindModal {
    /// 新建回退窗：焦点在安全项、无选中、无确认。
    pub fn new(target: impl Into<String>) -> Self {
        Self {
            target: target.into(),
            focused: RewindChoice::SAFE_DEFAULT,
            selected: None,
            confirmed: false,
            conflicts: Vec::new(),
            safety_stash: None,
            last_error: None,
        }
    }

    /// 记录已建立的安全快照句柄。
    pub fn attach_safety_stash(&mut self, handle: impl Into<String>) {
        self.safety_stash = Some(handle.into());
    }

    /// 安全快照是否仍然保留（本模型没有任何删除入口，恒为保留语义）。
    pub fn safety_stash_preserved(&self) -> bool {
        self.safety_stash.is_some()
    }

    /// 移动焦点（↑↓/Tab）。
    pub fn move_focus(&mut self, delta: i32) {
        self.focused = self.focused.cycled(delta);
    }

    /// 选中某选项（互斥：覆盖旧的选中项，并重置确认开关）。
    pub fn select(&mut self, choice: RewindChoice) {
        self.selected = Some(choice);
        self.focused = choice;
        self.confirmed = false;
        self.last_error = None;
    }

    /// 当前是否已选中破坏性选项。
    pub fn destructive_selected(&self) -> bool {
        self.selected.is_some_and(RewindChoice::is_destructive)
    }

    /// 冲突存在时给出可执行的恢复指引（§5 边界表）。
    pub fn conflict_guidance(&self) -> Option<String> {
        if self.conflicts.is_empty() {
            return None;
        }
        let preview = self
            .conflicts
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join(" ");
        Some(format!(
            "先运行 `git status` 处理冲突文件（{preview}），安全快照已保留，可随时恢复"
        ))
    }

    /// 显式确认破坏性回退（勾选确认条）。
    pub fn confirm_destructive(&mut self) -> bool {
        if self.destructive_selected() {
            self.confirmed = true;
            true
        } else {
            false
        }
    }

    /// 取消（Esc / 取消按钮）：不产生任何请求。
    pub fn cancel(&mut self) {
        self.selected = Some(RewindChoice::Cancel);
        self.confirmed = false;
        self.last_error = Some(RewindError::Cancelled);
    }

    /// 提交回退：只有在「已选 + 已确认（若破坏性）+ 无冲突」时才产出请求。
    pub fn apply(&mut self) -> Result<RewindRequest, RewindError> {
        let Some(choice) = self.selected else {
            self.last_error = Some(RewindError::NoChoiceSelected);
            return Err(RewindError::NoChoiceSelected);
        };
        match choice {
            RewindChoice::Cancel => {
                self.last_error = Some(RewindError::Cancelled);
                Err(RewindError::Cancelled)
            }
            RewindChoice::ForkConversationOnly => {
                self.last_error = None;
                Ok(RewindRequest {
                    target: self.target.clone(),
                    choice,
                    keeps_worktree: true,
                })
            }
            RewindChoice::ConversationAndWorktree => {
                if !self.conflicts.is_empty() {
                    self.last_error = Some(RewindError::ConflictsPresent);
                    return Err(RewindError::ConflictsPresent);
                }
                if !self.confirmed {
                    self.last_error = Some(RewindError::NeedsConfirmation);
                    return Err(RewindError::NeedsConfirmation);
                }
                self.last_error = None;
                Ok(RewindRequest {
                    target: self.target.clone(),
                    choice,
                    keeps_worktree: false,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 三选一互斥：每次选择只保留一个选项，焦点同步跟随。
    #[test]
    fn choices_are_mutually_exclusive() {
        let mut modal = RewindModal::new("#04");
        assert_eq!(
            modal.selected, None,
            "初始必须未决，禁止默认选中任何回退方式"
        );
        assert_eq!(modal.focused, RewindChoice::SAFE_DEFAULT);

        for choice in RewindChoice::ALL {
            modal.select(choice);
            assert_eq!(modal.selected, Some(choice));
            assert_eq!(modal.focused, choice);
            // 任何时刻只可能有一个选中项（单值枚举天然互斥）。
            assert_eq!(RewindChoice::ALL.len(), 3);
        }
        assert_eq!(RewindChoice::ALL[0], RewindChoice::ForkConversationOnly);
        assert_eq!(RewindChoice::ALL[1], RewindChoice::ConversationAndWorktree);
        assert_eq!(RewindChoice::ALL[2], RewindChoice::Cancel);
    }

    /// 首帧焦点在安全项，↑↓ 循环移动不改变已选中项之外的状态。
    #[test]
    fn focus_starts_safe_and_cycles() {
        let mut modal = RewindModal::new("#04");
        assert_eq!(modal.focused.label(), "仅分叉对话");
        modal.move_focus(1);
        assert_eq!(modal.focused, RewindChoice::ConversationAndWorktree);
        modal.move_focus(1);
        assert_eq!(modal.focused, RewindChoice::Cancel);
        modal.move_focus(1);
        assert_eq!(modal.focused, RewindChoice::ForkConversationOnly);
        modal.move_focus(-1);
        assert_eq!(modal.focused, RewindChoice::Cancel);
        // 仅移动焦点不算选择。
        assert_eq!(modal.selected, None);
    }

    /// 未选择与取消都返回拒绝态，绝不静默回退。
    #[test]
    fn unselected_and_cancel_are_rejected() {
        let mut modal = RewindModal::new("#04");
        assert_eq!(modal.apply(), Err(RewindError::NoChoiceSelected));
        assert_eq!(modal.last_error, Some(RewindError::NoChoiceSelected));

        modal.cancel();
        assert_eq!(modal.selected, Some(RewindChoice::Cancel));
        assert_eq!(modal.apply(), Err(RewindError::Cancelled));
        assert_eq!(
            RewindError::Cancelled.message(),
            "已取消回退，未改动任何文件"
        );
    }

    /// 破坏性回退必须显式确认；确认后才产出请求。
    #[test]
    fn destructive_choice_requires_confirmation() {
        let mut modal = RewindModal::new("#04");
        modal.select(RewindChoice::ConversationAndWorktree);
        assert!(modal.destructive_selected());
        assert_eq!(modal.apply(), Err(RewindError::NeedsConfirmation));
        assert_eq!(
            RewindError::NeedsConfirmation.message(),
            "工作树回退需要勾选确认后才能执行"
        );

        assert!(modal.confirm_destructive());
        let request = modal.apply().expect("确认后应产出请求");
        assert_eq!(request.target, "#04");
        assert!(request.touches_worktree());
        assert!(!request.keeps_worktree);
    }

    /// 切换选中项会重置确认开关（禁止跨选项继承确认）。
    #[test]
    fn confirmation_resets_when_choice_changes() {
        let mut modal = RewindModal::new("#04");
        modal.select(RewindChoice::ConversationAndWorktree);
        assert!(modal.confirm_destructive());
        assert!(modal.confirmed);

        modal.select(RewindChoice::ForkConversationOnly);
        assert!(!modal.confirmed, "换选项后必须重新确认");
        assert!(!modal.confirm_destructive(), "安全项不接受破坏性确认");
        let request = modal.apply().expect("安全项无需确认即可回退");
        assert!(request.keeps_worktree);
        assert!(!request.touches_worktree());
    }

    /// 冲突文件阻止工作树回退，并给出 `git status` 恢复指引；安全快照保留。
    #[test]
    fn conflicts_block_worktree_rewind_and_keep_stash() {
        let mut modal = RewindModal::new("#04");
        modal.attach_safety_stash("stash@{0}");
        modal.conflicts = vec!["crates/soxu-desktop/src/shell.rs".to_string()];
        modal.select(RewindChoice::ConversationAndWorktree);
        assert!(modal.confirm_destructive());

        assert_eq!(modal.apply(), Err(RewindError::ConflictsPresent));
        assert_eq!(
            RewindError::ConflictsPresent.message(),
            "存在冲突文件，已阻止工作树回退"
        );
        let guidance = modal.conflict_guidance().expect("冲突时应给出恢复指引");
        assert!(guidance.contains("git status"));
        assert!(modal.safety_stash_preserved(), "安全快照绝不能被删除");

        // 仅分叉对话不受冲突影响。
        modal.select(RewindChoice::ForkConversationOnly);
        assert!(modal.apply().is_ok());
        assert!(modal.safety_stash_preserved());
    }

    /// 无冲突时不会产出指引文案。
    #[test]
    fn no_guidance_without_conflicts() {
        let modal = RewindModal::new("#04");
        assert!(modal.conflict_guidance().is_none());
        assert!(!modal.safety_stash_preserved(), "未建立快照时无保留语义");
    }
}
