//! 卡片内焦点不逃逸：复用冻结的 7 层焦点栈（`crate::focus`）。
//!
//! 设计基准：`pages/01-desktop-workbench.md` §5.1——「内层处理后必须
//! `cx.stop_propagation()`，焦点不逃逸」。审批卡、提问卡与工具卡内部都有多个可聚焦
//! 控件（三个审批按钮、选项网格、展开/收起），Tab / Shift+Tab 必须在卡片内循环，
//! Esc 只关闭当前卡片的作用域，绝不穿透到背景的输入舱或全局快捷键层。
//!
//! 本模块不新建焦点层级（`focus.rs` 为冻结文件，7 层枚举不可扩展），而是把卡片
//! **放在**既有层级里：由调用方给出所在层（默认
//! [`FocusLayer::ComposerInput`]——审批与提问卡与输入舱同级，位于审阅层内侧），
//! 卡片只在体内做焦点环绕，因此模态锁定、幂等入栈、`pop_to` 截断等规则全部继承。

use crate::focus::{FocusDecision, FocusLayer, FocusStack};

/// 卡片内的焦点作用域。
#[derive(Clone, Debug)]
pub struct CardFocusScope {
    /// 焦点栈（继承模态锁定与焦点不逃逸规则）。
    stack: FocusStack,
    /// 卡片所在的焦点层级。
    layer: FocusLayer,
    /// 卡片内可聚焦控件数量。
    target_count: usize,
    /// 当前聚焦的控件下标。
    index: usize,
}

impl CardFocusScope {
    /// 创建作用域（未激活；默认聚焦第 1 个控件）。
    pub fn new(layer: FocusLayer, target_count: usize) -> Self {
        Self {
            stack: FocusStack::new(),
            layer,
            target_count,
            index: 0,
        }
    }

    /// 以输入舱同级层创建（默认层：审批/提问卡与输入舱同级）。
    pub fn composer_level(target_count: usize) -> Self {
        Self::new(FocusLayer::ComposerInput, target_count)
    }

    /// 所在层级。
    pub fn layer(&self) -> FocusLayer {
        self.layer
    }

    /// 卡片内可聚焦控件数量。
    pub fn target_count(&self) -> usize {
        self.target_count
    }

    /// 更新可聚焦控件数量（行/轮次变化后调用）；当前焦点下标被钳回合法区间。
    pub fn set_target_count(&mut self, count: usize) {
        self.target_count = count;
        if count == 0 {
            self.index = 0;
        } else if self.index >= count {
            self.index = count - 1;
        }
    }

    /// 是否已接管焦点。
    pub fn is_open(&self) -> bool {
        self.stack.active() == Some(self.layer)
    }

    /// 进入卡片作用域（入栈）。
    ///
    /// 继承模态锁定：`ModalDialog` 激活时返回 [`FocusDecision::RejectedModalLock`]，
    /// 作用域保持未激活。
    pub fn open(&mut self) -> FocusDecision {
        self.stack.push(self.layer)
    }

    /// 退出卡片作用域（出栈）。
    pub fn close(&mut self) -> Option<FocusLayer> {
        self.stack.close_active()
    }

    /// 当前聚焦的控件下标。
    pub fn focus(&self) -> usize {
        self.index
    }

    /// 聚焦下一个控件（到末尾回卷到首个，永不逃出卡片）。
    pub fn focus_next(&mut self) -> usize {
        if self.target_count == 0 {
            self.index = 0;
            return 0;
        }
        self.index = (self.index + 1) % self.target_count;
        self.index
    }

    /// 聚焦上一个控件（到首个回卷到末尾，永不逃出卡片）。
    pub fn focus_prev(&mut self) -> usize {
        if self.target_count == 0 {
            self.index = 0;
            return 0;
        }
        self.index = (self.index + self.target_count - 1) % self.target_count;
        self.index
    }

    /// 直接聚焦指定控件；越界返回 `false` 且状态不变。
    pub fn focus_index(&mut self, index: usize) -> bool {
        if index >= self.target_count {
            return false;
        }
        self.index = index;
        true
    }

    /// 焦点栈是否消费 Esc（内层已接管即不冒泡）。
    pub fn consumes_escape(&self) -> bool {
        self.stack.consumes_escape()
    }

    /// Esc 处理：作用域内消费并关闭自身，焦点回到外层。返回是否消费。
    ///
    /// 未接管焦点时返回 `false`，让外层正常处理 Esc。
    pub fn escape(&mut self) -> bool {
        if self.is_open() {
            self.stack.close_active();
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_cycles_within_card_and_never_escapes() {
        // 审批卡三个按钮：拒绝 / 本会话允许 / 允许一次。
        let mut scope = CardFocusScope::composer_level(3);
        assert_eq!(scope.focus(), 0);
        assert_eq!(scope.focus_next(), 1);
        assert_eq!(scope.focus_next(), 2);
        // 到末尾回卷一，绝不越界。
        assert_eq!(scope.focus_next(), 0);
        assert_eq!(scope.focus_prev(), 2);
        assert_eq!(scope.focus_prev(), 1);
        assert_eq!(scope.focus_prev(), 0);

        // 100 次连续 Tab 后仍在卡片内。
        for _ in 0..100 {
            let index = scope.focus_next();
            assert!(index < 3);
        }
        assert_eq!(scope.target_count(), 3);
        assert_eq!(scope.layer(), FocusLayer::ComposerInput);
    }

    #[test]
    fn empty_card_is_safe() {
        let mut scope = CardFocusScope::composer_level(0);
        assert_eq!(scope.focus_next(), 0);
        assert_eq!(scope.focus_prev(), 0);
        assert!(!scope.focus_index(0));
        assert_eq!(scope.focus(), 0);
    }

    #[test]
    fn direct_focus_index_rejects_out_of_range() {
        let mut scope = CardFocusScope::composer_level(2);
        assert!(scope.focus_index(1));
        assert_eq!(scope.focus(), 1);
        assert!(!scope.focus_index(2));
        assert_eq!(scope.focus(), 1, "越界聚焦不得改变状态");
    }

    #[test]
    fn escape_is_consumed_only_while_scope_holds_focus() {
        let mut scope = CardFocusScope::composer_level(3);
        // 未接管焦点：Esc 冒泡给外层。
        assert!(!scope.consumes_escape());
        assert!(!scope.escape());

        assert_eq!(scope.open(), FocusDecision::Allowed);
        assert!(scope.is_open());
        // 接管后 Esc 被消费，并只关闭卡片自身。
        assert!(scope.consumes_escape());
        assert!(scope.escape());
        assert!(!scope.is_open());
        // 关闭后再按 Esc 不再被消费（回到外层正常处理）。
        assert!(!scope.escape());
    }

    #[test]
    fn target_count_update_clamps_focus() {
        let mut scope = CardFocusScope::composer_level(4);
        assert!(scope.focus_index(3));
        // 轮次减少：焦点钳回新的末位。
        scope.set_target_count(2);
        assert_eq!(scope.focus(), 1);
        assert_eq!(scope.target_count(), 2);
        assert_eq!(scope.focus_next(), 0);
        // 清空：焦点归零且不 panic。
        scope.set_target_count(0);
        assert_eq!(scope.focus(), 0);
        assert_eq!(scope.focus_next(), 0);
    }

    #[test]
    fn modal_lock_prevents_card_from_stealing_focus() {
        // 同一层重复入栈幂等（继承 `FocusStack` 的 `RejectedNoop`）。
        let mut scope = CardFocusScope::composer_level(1);
        assert_eq!(scope.open(), FocusDecision::Allowed);
        assert_eq!(scope.open(), FocusDecision::RejectedNoop);
        assert!(scope.is_open());

        // 模态层占位后，卡片再入栈被模态锁定拒绝。
        let mut locked = CardFocusScope::composer_level(2);
        assert_eq!(
            locked.stack.push(FocusLayer::ModalDialog),
            FocusDecision::Allowed
        );
        assert_eq!(locked.open(), FocusDecision::RejectedModalLock);
        assert!(!locked.is_open());

        // 关闭后重新可入栈。
        assert_eq!(scope.close(), Some(FocusLayer::ComposerInput));
        assert!(!scope.is_open());
        assert_eq!(scope.open(), FocusDecision::Allowed);
    }
}
