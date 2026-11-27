//! 7 层焦点上下文栈与切换规则（`pages/01-desktop-workbench.md` 第五节）。
//!
//! 从内到外的作用域隔离层级（内层处理后必须 `stop_propagation`，焦点不逃逸）：
//!
//! ```text
//! ModalDialog ＞ CommandPalette ＞ AutocompletePopup ＞ InlineRename
//!   ＞ ReviewDiff ＞ ComposerInput ＞ WorkbenchGlobal
//! ```
//!
//! 本模块只承载**纯逻辑**（枚举、栈序、切换裁决），不持有 gpui `FocusHandle`；
//! 真实按键分发与 `cx.stop_propagation()` 在 M5 接入视图时基于 [`FocusLayer`]
//! 与 [`FocusStack`] 接线。纯逻辑单测见文件底部。

/// 焦点上下文层级（内层优先级最高）。
///
/// 层级语义见 `pages/01-desktop-workbench.md` §5.1；`COUNT` 由编译期断言锁定为 7。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FocusLayer {
    /// 模态对话框（延迟 2 帧安全聚焦）。
    ModalDialog,
    /// 全局命令面板（`Ctrl/Cmd+K`；输入非空时 Esc 先清空再关闭）。
    CommandPalette,
    /// `@agent` / `/skill` / `#file` 自动补全弹层。
    AutocompletePopup,
    /// 会话内联重命名（Esc 仅取消重命名）。
    InlineRename,
    /// 右栏 Diff/文件树审阅（`/` 聚焦过滤）。
    ReviewDiff,
    /// 三态输入舱（Enter / Shift+Enter 组合投递）。
    ComposerInput,
    /// 工作台全局快捷键层（面板开关、会话切换）。
    WorkbenchGlobal,
}

impl FocusLayer {
    /// 层级总数（编译期断言锁定）。
    pub const COUNT: usize = 7;

    /// 全部层级，按从内（最高优先）到外排列。
    pub const ALL: [Self; Self::COUNT] = [
        Self::ModalDialog,
        Self::CommandPalette,
        Self::AutocompletePopup,
        Self::InlineRename,
        Self::ReviewDiff,
        Self::ComposerInput,
        Self::WorkbenchGlobal,
    ];

    /// 该层的 key context 字符串（GPUI 键位绑定的作用域名）。
    pub const fn key_context(self) -> &'static str {
        match self {
            Self::ModalDialog => "ModalDialog",
            Self::CommandPalette => "CommandPalette",
            Self::AutocompletePopup => "AutocompletePopup",
            Self::InlineRename => "InlineRename",
            Self::ReviewDiff => "ReviewDiff",
            Self::ComposerInput => "ComposerInput",
            Self::WorkbenchGlobal => "WorkbenchGlobal",
        }
    }
}

/// 焦点栈运行状态：保存当前激活层，裁决层间切换是否合法。
///
/// 不变量：
/// 1. **单一激活层**：任意时刻至多一层持有焦点上下文；
/// 2. **焦点不逃逸**：只有显式 [`FocusStack::pop_to`] / [`FocusStack::close_active`]
///    或更高层入栈才能改变激活层；
/// 3. **模态锁定**：[`FocusLayer::ModalDialog`] 激活时，除其自身关闭外任何
///    低层切换都被拒绝（模态锁定按规范）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FocusStack {
    /// 从内到外的激活层栈；栈顶即当前激活层。
    stack: Vec<FocusLayer>,
}

/// 层切换裁决结果。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusDecision {
    /// 允许切换。
    Allowed,
    /// 拒绝：模态锁定生效（ModalDialog 激活时低层请求）。
    RejectedModalLock,
    /// 拒绝：目标层与当前层相同且不是合法的重复激活。
    RejectedNoop,
}

impl FocusStack {
    /// 创建空栈（无任何焦点上下文，全局快捷键兜底生效）。
    pub fn new() -> Self {
        Self { stack: Vec::new() }
    }

    /// 当前激活层；空栈时为 `None`（等价于 WorkbenchGlobal 兜底）。
    pub fn active(&self) -> Option<FocusLayer> {
        self.stack.last().copied()
    }

    /// 有效 key context：空栈时回落到 WorkbenchGlobal。
    pub fn effective_context(&self) -> &'static str {
        self.active()
            .map_or(FocusLayer::WorkbenchGlobal.key_context(), |l| {
                l.key_context()
            })
    }

    /// 请求激活某层（入栈）。
    ///
    /// 规则：
    /// - 相同层重复激活视为幂等（`RejectedNoop`，状态不变）；
    /// - ModalDialog 激活时，其他层入栈一律拒绝（模态锁定）；
    /// - 其余情况允许，目标层压栈。
    pub fn push(&mut self, layer: FocusLayer) -> FocusDecision {
        if self.active() == Some(layer) {
            return FocusDecision::RejectedNoop;
        }
        if self.active() == Some(FocusLayer::ModalDialog) {
            return FocusDecision::RejectedModalLock;
        }
        self.stack.push(layer);
        FocusDecision::Allowed
    }

    /// 关闭当前激活层（出栈，回到其外层）。
    ///
    /// 空栈时返回 `None`；正常时返回被关闭的层。
    pub fn close_active(&mut self) -> Option<FocusLayer> {
        self.stack.pop()
    }

    /// 回退到指定层（把栈截断到该层为止，含该层）。
    ///
    /// 用于 ModalDialog 关闭后一次性清理中间层（如补全弹层）。
    /// 目标层不在栈中时状态不变并返回 `false`。
    pub fn pop_to(&mut self, layer: FocusLayer) -> bool {
        match self.stack.iter().rposition(|l| *l == layer) {
            Some(pos) => {
                let new_len = pos + 1;
                if self.stack.len() != new_len {
                    self.stack.truncate(new_len);
                }
                true
            }
            None => false,
        }
    }

    /// Esc 键裁决：把「输入非空先清空」这类前置行为留给视图层，
    /// 此处只裁决 Esc 是否被当前层消费（消费即 `stop_propagation`，焦点不逃逸）。
    pub fn consumes_escape(&self) -> bool {
        self.active().is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layer_count_is_seven() {
        assert_eq!(FocusLayer::ALL.len(), 7);
        assert_eq!(FocusLayer::COUNT, 7);
    }

    #[test]
    fn empty_stack_falls_back_to_global() {
        let stack = FocusStack::new();
        assert_eq!(stack.active(), None);
        assert_eq!(stack.effective_context(), "WorkbenchGlobal");
        assert!(!stack.consumes_escape());
    }

    #[test]
    fn push_and_close_roundtrip() {
        let mut stack = FocusStack::new();
        assert_eq!(
            stack.push(FocusLayer::CommandPalette),
            FocusDecision::Allowed
        );
        assert_eq!(stack.active(), Some(FocusLayer::CommandPalette));
        assert_eq!(stack.effective_context(), "CommandPalette");
        assert!(stack.consumes_escape());

        assert_eq!(stack.close_active(), Some(FocusLayer::CommandPalette));
        assert_eq!(stack.active(), None);
        // 空栈再关闭返回 None。
        assert_eq!(stack.close_active(), None);
    }

    #[test]
    fn nested_push_keeps_innermost_active() {
        let mut stack = FocusStack::new();
        stack.push(FocusLayer::ComposerInput);
        stack.push(FocusLayer::AutocompletePopup);
        assert_eq!(stack.active(), Some(FocusLayer::AutocompletePopup));

        // 关闭补全层后回落到输入舱。
        assert_eq!(stack.close_active(), Some(FocusLayer::AutocompletePopup));
        assert_eq!(stack.active(), Some(FocusLayer::ComposerInput));
    }

    #[test]
    fn modal_lock_rejects_lower_layers() {
        let mut stack = FocusStack::new();
        assert_eq!(stack.push(FocusLayer::ModalDialog), FocusDecision::Allowed);

        // 模态锁定：任何其他层都无法入栈。
        for layer in [
            FocusLayer::CommandPalette,
            FocusLayer::AutocompletePopup,
            FocusLayer::InlineRename,
            FocusLayer::ReviewDiff,
            FocusLayer::ComposerInput,
            FocusLayer::WorkbenchGlobal,
        ] {
            assert_eq!(
                stack.push(layer),
                FocusDecision::RejectedModalLock,
                "{layer:?} 在模态锁定下应被拒绝"
            );
            assert_eq!(stack.active(), Some(FocusLayer::ModalDialog));
        }

        // 仅关闭模态自身可解锁。
        assert_eq!(stack.close_active(), Some(FocusLayer::ModalDialog));
        assert_eq!(stack.active(), None);
        // 解锁后恢复正常。
        assert_eq!(
            stack.push(FocusLayer::ComposerInput),
            FocusDecision::Allowed
        );
    }

    #[test]
    fn duplicate_push_is_noop() {
        let mut stack = FocusStack::new();
        stack.push(FocusLayer::ReviewDiff);
        assert_eq!(
            stack.push(FocusLayer::ReviewDiff),
            FocusDecision::RejectedNoop
        );
        assert_eq!(stack.active(), Some(FocusLayer::ReviewDiff));
    }

    #[test]
    fn pop_to_truncates_intermediate_layers() {
        let mut stack = FocusStack::new();
        stack.push(FocusLayer::ComposerInput);
        stack.push(FocusLayer::AutocompletePopup);
        stack.push(FocusLayer::CommandPalette);

        assert!(stack.pop_to(FocusLayer::ComposerInput));
        assert_eq!(stack.active(), Some(FocusLayer::ComposerInput));

        // 目标层不在栈中：状态不变。
        assert!(!stack.pop_to(FocusLayer::ModalDialog));
        assert_eq!(stack.active(), Some(FocusLayer::ComposerInput));
    }

    #[test]
    fn key_context_names_match_spec() {
        assert_eq!(FocusLayer::ModalDialog.key_context(), "ModalDialog");
        assert_eq!(FocusLayer::WorkbenchGlobal.key_context(), "WorkbenchGlobal");
        assert_eq!(FocusLayer::ComposerInput.key_context(), "ComposerInput");
    }
}
