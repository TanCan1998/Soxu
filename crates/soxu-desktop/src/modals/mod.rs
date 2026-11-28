//! 全局模态框宿主与几何契约（`pages/06-modals-onboarding-and-automation.md`）。
//!
//! 规范要点（§5 模态框焦点捕获、几何常量与边界处理表）：
//! - **焦点捕获**：模态打开即入栈 [`FocusLayer::ModalDialog`]，激活期间任何低层入栈
//!   都被拒绝（模态锁定），杜绝「按 Esc 关弹窗却误停后台 Agent」；
//! - **焦点还原**：Esc 关闭后回到打开前所在层，不把焦点丢给全局兜底层；
//! - **几何**：提交 `540px`/`max-h 80vh`、配对 `580px`、回退 `480px`，统一 `p(20)` 与
//!   `rounded(18)`（令牌 [`spacing::SPACE_5_PX`] / [`radius::RADIUS_LG_PX`]）；
//! - **互斥**：同一时刻只有一个模态，重复打开被拒绝。
//!
//! 阶段边界（Issue #133）：纯内存状态机 + 渲染骨架，不接 daemon I/O，
//! 模态内容各自见 [`commit`] / [`pairing`] / [`rewind`] / [`inspector`]。

pub mod commit;
pub mod inspector;
pub mod pairing;
pub mod rewind;

use gpui::{Div, ParentElement, Styled, div, px};

use crate::focus::{FocusDecision, FocusLayer, FocusStack};
use crate::theme::gpui_colors::Palette;
use crate::tokens::{radius, spacing, typography};

pub use commit::{CommitDraft, CommitFile, CommitGate, CommitKind};
pub use inspector::{EnvRow, LaunchEnvInspector, is_sensitive_key, redact_value};
pub use pairing::{PairingError, PairingInput, PairingModal};
pub use rewind::{RewindChoice, RewindModal, RewindRequest};

/// 模态种类（`pages/06` §3 三个业务模态 + `pages/04` §5.2 透视器）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ModalKind {
    /// 结构化 Git 提交弹窗。
    Commit,
    /// 手机扫码配对弹窗。
    Pairing,
    /// Checkpoint 回退窗。
    Rewind,
    /// Worktree 启动环境透视器（只读）。
    Inspector,
}

impl ModalKind {
    /// 全部模态。
    pub const ALL: [Self; 4] = [Self::Commit, Self::Pairing, Self::Rewind, Self::Inspector];

    /// 模态标题。
    pub const fn label(self) -> &'static str {
        match self {
            Self::Commit => "结构化提交",
            Self::Pairing => "手机遥控配对",
            Self::Rewind => "回退到检查点",
            Self::Inspector => "启动环境透视",
        }
    }

    /// 卡片宽度（px）。前三个来自 §5 边界表；`Inspector` 按同族收敛（§04 §5.2 折叠面板）。
    pub const fn width_px(self) -> f32 {
        match self {
            Self::Commit => 540.0,
            Self::Pairing => 580.0,
            Self::Rewind => 480.0,
            Self::Inspector => 520.0,
        }
    }

    /// 可选最大高度（视口百分比）；`None` 表示内容自适应。
    pub const fn max_height_vh(self) -> Option<f32> {
        match self {
            Self::Commit => Some(80.0),
            Self::Pairing | Self::Rewind | Self::Inspector => None,
        }
    }

    /// 内边距（`p(20)`，§5 边界表统一值）。
    pub const fn padding_px(self) -> f32 {
        spacing::SPACE_5_PX
    }

    /// 圆角（`rounded(18)`，§5 边界表统一值）。
    pub const fn radius_px(self) -> f32 {
        radius::RADIUS_LG_PX
    }

    /// 模态恒用 [`FocusLayer::ModalDialog`] 上下文。
    pub const fn focus_layer(self) -> FocusLayer {
        FocusLayer::ModalDialog
    }
}

/// 模态宿主：持有焦点栈与当前打开模态的纯状态机。
#[derive(Clone, Debug, Default)]
pub struct ModalHost {
    stack: FocusStack,
    open: Option<ModalKind>,
    /// 打开前的激活层（Esc 关闭后还原到这里）。
    captured: Option<FocusLayer>,
}

impl ModalHost {
    /// 空宿主（无模态、焦点栈为空 → 全局兜底）。
    pub fn new() -> Self {
        Self::default()
    }

    /// 焦点栈只读视图（供上层键位分发与测试）。
    pub fn stack(&self) -> &FocusStack {
        &self.stack
    }

    /// 显式入栈一个低层焦点上下文（打开模态前的当前层）。
    pub fn push_layer(&mut self, layer: FocusLayer) -> FocusDecision {
        self.stack.push(layer)
    }

    /// 当前打开模态。
    pub fn kind(&self) -> Option<ModalKind> {
        self.open
    }

    /// 是否有模态打开。
    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    /// 模态锁定是否生效（激活层为 `ModalDialog`）。
    pub fn focus_locked(&self) -> bool {
        self.stack.active() == Some(FocusLayer::ModalDialog)
    }

    /// 打开模态：捕获当前焦点层并压入 `ModalDialog`。
    ///
    /// 返回 `false` 表示已有模态打开（互斥）或模态层入栈被拒绝。
    pub fn open_modal(&mut self, kind: ModalKind) -> bool {
        if self.open.is_some() {
            return false;
        }
        let captured = self.stack.active();
        if self.stack.push(kind.focus_layer()) != FocusDecision::Allowed {
            return false;
        }
        self.captured = captured;
        self.open = Some(kind);
        true
    }

    /// Esc 裁决：模态打开时关闭并还原焦点，返回 `true`（消费即 `stop_propagation`）。
    pub fn handle_escape(&mut self) -> bool {
        if self.open.take().is_none() {
            return false;
        }
        self.stack.close_active();
        // 关闭后若上层没有任何焦点上下文，显式还原到打开前的层（如 `WorkbenchGlobal`）。
        if self.stack.active() != self.captured
            && let Some(layer) = self.captured
            && self.stack.active().is_none()
        {
            self.stack.push(layer);
        }
        self.captured = None;
        true
    }

    /// 当前生效的 key context 字符串。
    pub fn effective_context(&self) -> &'static str {
        self.stack.effective_context()
    }
}

/// 模态卡片骨架：定宽 + `p(20)` + `rounded(18)` + 浮起表面海拔。
///
/// 遮罩、居中定位与 `max-h 80vh` 折算由 M6 的叠加层宿主承担
/// （本阶段不改壳层结构，卡片只负责 §5 边界表的宽度、内边距与圆角）。
pub fn render_modal_card(kind: ModalKind, body: Div, palette: &Palette) -> Div {
    let card = div()
        .flex()
        .flex_col()
        .gap(px(spacing::SPACE_4_PX))
        .w(px(kind.width_px()))
        .p(px(kind.padding_px()))
        .rounded(px(kind.radius_px()))
        .bg(palette.surface_floating)
        .border_1()
        .border_color(palette.border_strong)
        .text_color(palette.text_primary)
        .child(
            div()
                .text_size(px(typography::TEXT_TITLE_PX))
                .child(kind.label()),
        );
    card.child(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// §5 边界表几何：三张表内模态的宽高与统一内边距/圆角。
    #[test]
    fn modal_geometry_matches_spec_table() {
        assert_eq!(ModalKind::Commit.width_px(), 540.0);
        assert_eq!(ModalKind::Commit.max_height_vh(), Some(80.0));
        assert_eq!(ModalKind::Pairing.width_px(), 580.0);
        assert_eq!(ModalKind::Rewind.width_px(), 480.0);
        for kind in ModalKind::ALL {
            assert_eq!(kind.padding_px(), 20.0);
            assert_eq!(kind.radius_px(), radius::RADIUS_LG_PX);
            assert_eq!(kind.focus_layer(), FocusLayer::ModalDialog);
        }
    }

    /// 打开即捕获焦点：模态锁定生效，Esc 关闭后还原到打开前所在层。
    #[test]
    fn open_captures_and_escape_restores_focus() {
        let mut host = ModalHost::new();
        assert!(!host.is_open());
        assert_eq!(host.effective_context(), "WorkbenchGlobal");

        // 从输入舱工具栏（全局层）打开提交模态。
        assert_eq!(
            host.push_layer(FocusLayer::WorkbenchGlobal),
            FocusDecision::Allowed
        );
        assert!(host.open_modal(ModalKind::Commit));
        assert!(host.focus_locked());
        assert_eq!(host.effective_context(), "ModalDialog");

        // 模态锁定：低层无法夺焦。
        assert_eq!(
            host.push_layer(FocusLayer::ComposerInput),
            FocusDecision::RejectedModalLock
        );
        assert_eq!(host.effective_context(), "ModalDialog");

        assert!(host.handle_escape(), "Esc 应被模态消费");
        assert!(!host.is_open());
        assert!(!host.focus_locked());
        assert_eq!(host.effective_context(), "WorkbenchGlobal");

        // 关闭后低层恢复正常。
        assert_eq!(
            host.push_layer(FocusLayer::ComposerInput),
            FocusDecision::Allowed
        );
    }

    /// Esc 打开前无任何焦点上下文时，关闭后不残留模态层。
    #[test]
    fn escape_without_previous_layer_clears_stack() {
        let mut host = ModalHost::new();
        assert!(host.open_modal(ModalKind::Pairing));
        assert!(host.handle_escape());
        assert_eq!(host.effective_context(), "WorkbenchGlobal");
        assert!(!host.handle_escape(), "已关闭时 Esc 不再被模态消费");
    }

    /// 模态互斥：已有模态时再打开被拒绝，且不改变当前焦点层。
    #[test]
    fn modals_are_mutually_exclusive() {
        let mut host = ModalHost::new();
        assert!(host.open_modal(ModalKind::Rewind));
        assert!(!host.open_modal(ModalKind::Commit));
        assert_eq!(host.kind(), Some(ModalKind::Rewind));
        assert!(host.handle_escape());
        assert!(host.open_modal(ModalKind::Commit));
        assert_eq!(host.kind(), Some(ModalKind::Commit));
    }
}
