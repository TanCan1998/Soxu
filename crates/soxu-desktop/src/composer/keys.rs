//! 输入舱按键门控：Enter / Shift+Enter 与 IME 组合期不误提交（纯逻辑）。
//!
//! 规范来源：
//! - `pages/02-composer-and-handoff.md` §2 四态投递状态机与 §6.2「输入法组合选词护栏」；
//! - `MASTER.md` §7.5：所有支持 `Enter` 提交的输入组件必须在 `onKeyDown` 首位检查
//!   `isImeComposing(e)`（`nativeEvent.isComposing || keyCode === 229`），杜绝选词上屏误发。
//!
//! 本模块只做裁决，不接触 GPUI 事件对象、不派生子进程、不写任何状态。
//! 渲染层把真实按键归一化为 [`KeyStroke`] 后调用 [`resolve`]，按 [`KeyOutcome`] 执行。

use soxu_protocol::{ActiveTurnBehavior, TurnLifecycleState};

/// 输入舱关心的按键（其余按键交给文本编辑本身）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComposerKey {
    /// 提交/换行键。
    Enter,
    /// 取消键（准备期取消、运行期中断）。
    Escape,
    /// 其他按键（不参与投递裁决）。
    Other,
}

/// 归一化后的按键快照（把平台事件压成可测的纯数据）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyStroke {
    /// 按键本体。
    pub key: ComposerKey,
    /// 是否按下 `Shift`。
    pub shift: bool,
    /// 是否按下 `Ctrl`（Windows/Linux）或 `Cmd`（macOS）。
    pub ctrl_or_cmd: bool,
    /// 是否按下 `Alt`。
    pub alt: bool,
    /// 输入法是否处于组合选词期（`isComposing` / `keyCode === 229`）。
    pub ime_composing: bool,
}

impl KeyStroke {
    /// 裸 `Enter`。
    pub const fn enter() -> Self {
        Self {
            key: ComposerKey::Enter,
            shift: false,
            ctrl_or_cmd: false,
            alt: false,
            ime_composing: false,
        }
    }

    /// `Shift+Enter`（换行）。
    pub const fn shift_enter() -> Self {
        Self {
            shift: true,
            ..Self::enter()
        }
    }

    /// `Ctrl/Cmd+Enter`（热纠偏）。
    pub const fn ctrl_enter() -> Self {
        Self {
            ctrl_or_cmd: true,
            ..Self::enter()
        }
    }

    /// `Ctrl/Cmd+Shift+Enter`（排队）。
    pub const fn ctrl_shift_enter() -> Self {
        Self {
            shift: true,
            ..Self::ctrl_enter()
        }
    }

    /// `Alt+Enter`（打断重发）。
    pub const fn alt_enter() -> Self {
        Self {
            alt: true,
            ..Self::enter()
        }
    }

    /// 组合选词期的裸 `Enter`（上屏，绝不可提交）。
    pub const fn ime_enter() -> Self {
        Self {
            ime_composing: true,
            ..Self::enter()
        }
    }

    /// 组合选词期的 `Shift+Enter`（仍属输入法上屏，不换行）。
    pub const fn ime_shift_enter() -> Self {
        Self {
            ime_composing: true,
            ..Self::shift_enter()
        }
    }

    /// 是否为裸 `Enter`（无任何修饰键且不处于输入法组合期）。
    pub const fn is_plain_enter(&self) -> bool {
        matches!(self.key, ComposerKey::Enter)
            && !self.shift
            && !self.ctrl_or_cmd
            && !self.alt
            && !self.ime_composing
    }
}

/// 一次投递的语义意图。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubmitIntent {
    /// 空闲态：开启新回合（`turn_index + 1`）。
    NewTurn,
    /// 运行中：入待发队列，当前回合结束后自动发出。
    Queue,
    /// 运行中：在下一个工具边界注入纠偏指令。
    Steer,
    /// 运行中：立即终止当前回合并用新指令开启新回合。
    Interrupt,
}

impl SubmitIntent {
    /// 映射到协议的活跃轮次投递策略（`NewTurn` 无活跃轮次，返回 `None`）。
    pub const fn behavior(self) -> Option<ActiveTurnBehavior> {
        match self {
            Self::NewTurn => None,
            Self::Queue => Some(ActiveTurnBehavior::Queue),
            Self::Steer => Some(ActiveTurnBehavior::Steer),
            Self::Interrupt => Some(ActiveTurnBehavior::Interrupt),
        }
    }
}

/// 按键裁决结果。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyOutcome {
    /// 插入换行（`Shift+Enter`）。
    InsertNewline,
    /// 提交，按 [`SubmitIntent`] 决定投递策略。
    Submit(SubmitIntent),
    /// 输入法组合期消费掉该键，既不提交也不换行（交给 IME 上屏）。
    ConsumedByIme,
    /// 不处理（`Esc`、空草稿提交、准备期重复派发等）。
    Ignored,
}

/// 裁决一次按键。
///
/// 优先级（严格按此顺序，顺序本身即红线）：
/// 1. **IME 组合期**：任何 `Enter` 一律 [`KeyOutcome::ConsumedByIme`]，绝不提交也不换行；
/// 2. 非 `Enter`：`Ignored`（`Esc` 的取消/中断由焦点栈裁决，不在投递路径上）；
/// 3. `Shift+Enter`：换行（运行中也一样，不打断当前回合）；
/// 4. `Preparing` 期：任何 `Enter` 组合一律忽略（防重复派发）；
/// 5. `Ctrl/Cmd+Shift+Enter`：运行中排队，空闲开新回合；
/// 6. `Ctrl/Cmd+Enter`：运行中纠偏，空闲开新回合；
/// 7. `Alt+Enter`：运行中打断重发，空闲开新回合；
/// 8. 裸 `Enter`：空草稿忽略、运行中排队、空闲开新回合。
pub fn resolve(stroke: &KeyStroke, phase: TurnLifecycleState, draft_empty: bool) -> KeyOutcome {
    if stroke.ime_composing {
        return match stroke.key {
            ComposerKey::Enter => KeyOutcome::ConsumedByIme,
            ComposerKey::Escape | ComposerKey::Other => KeyOutcome::Ignored,
        };
    }

    if !matches!(stroke.key, ComposerKey::Enter) {
        return KeyOutcome::Ignored;
    }

    // Shift+Enter 在任何运行态都只换行。
    if stroke.shift && !stroke.ctrl_or_cmd && !stroke.alt {
        return KeyOutcome::InsertNewline;
    }

    // 准备期（挂载 Worktree / 拉起 CLI 子进程）不得重复派发：
    // 任何修饰键组合的 Enter 都不开新回合，取消准备走 `Esc`（由焦点栈裁决）。
    if matches!(phase, TurnLifecycleState::Preparing) {
        return KeyOutcome::Ignored;
    }

    let running = matches!(phase, TurnLifecycleState::Running);

    // 组合修饰键：Ctrl/Cmd+Shift > Ctrl/Cmd > Alt。
    if stroke.ctrl_or_cmd && stroke.shift {
        return KeyOutcome::Submit(if running {
            SubmitIntent::Queue
        } else {
            SubmitIntent::NewTurn
        });
    }
    if stroke.ctrl_or_cmd {
        return KeyOutcome::Submit(if running {
            SubmitIntent::Steer
        } else {
            SubmitIntent::NewTurn
        });
    }
    if stroke.alt {
        return KeyOutcome::Submit(if running {
            SubmitIntent::Interrupt
        } else {
            SubmitIntent::NewTurn
        });
    }

    // 以下为裸 Enter。
    if draft_empty {
        // 空闲态空输入不可提交；运行中空输入的停止语义在发送键上，不在 Enter 上。
        return KeyOutcome::Ignored;
    }
    KeyOutcome::Submit(if running {
        SubmitIntent::Queue
    } else {
        SubmitIntent::NewTurn
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 空闲态：裸 Enter 开新回合，Shift+Enter 换行，空草稿不可提交。
    #[test]
    fn idle_phase_enter_matrix() {
        assert_eq!(
            resolve(&KeyStroke::enter(), TurnLifecycleState::Idle, false),
            KeyOutcome::Submit(SubmitIntent::NewTurn)
        );
        assert_eq!(
            resolve(&KeyStroke::shift_enter(), TurnLifecycleState::Idle, false),
            KeyOutcome::InsertNewline
        );
        assert_eq!(
            resolve(&KeyStroke::enter(), TurnLifecycleState::Idle, true),
            KeyOutcome::Ignored
        );
    }

    /// 运行态：Enter 排队、Ctrl+Enter 纠偏、Ctrl+Shift+Enter 排队、Alt+Enter 打断重发。
    #[test]
    fn running_phase_modifier_matrix() {
        let running = TurnLifecycleState::Running;
        assert_eq!(
            resolve(&KeyStroke::enter(), running, false),
            KeyOutcome::Submit(SubmitIntent::Queue)
        );
        assert_eq!(
            resolve(&KeyStroke::ctrl_enter(), running, false),
            KeyOutcome::Submit(SubmitIntent::Steer)
        );
        assert_eq!(
            resolve(&KeyStroke::ctrl_shift_enter(), running, false),
            KeyOutcome::Submit(SubmitIntent::Queue)
        );
        assert_eq!(
            resolve(&KeyStroke::alt_enter(), running, false),
            KeyOutcome::Submit(SubmitIntent::Interrupt)
        );
        // Shift+Enter 在运行态也只换行，不打断推理。
        assert_eq!(
            resolve(&KeyStroke::shift_enter(), running, false),
            KeyOutcome::InsertNewline
        );
        // 运行中空输入按 Enter 不投递（停止语义在发送键上）。
        assert_eq!(
            resolve(&KeyStroke::enter(), running, true),
            KeyOutcome::Ignored
        );
    }

    /// 准备期：所有组合键都不派发新回合；但换行仍可用。
    #[test]
    fn preparing_phase_blocks_dispatch() {
        let preparing = TurnLifecycleState::Preparing;
        for stroke in [
            KeyStroke::enter(),
            KeyStroke::ctrl_enter(),
            KeyStroke::ctrl_shift_enter(),
            KeyStroke::alt_enter(),
        ] {
            assert_eq!(
                resolve(&stroke, preparing, false),
                KeyOutcome::Ignored,
                "{stroke:?} 在准备期应被忽略"
            );
        }
        assert_eq!(
            resolve(&KeyStroke::shift_enter(), preparing, false),
            KeyOutcome::InsertNewline
        );
    }

    /// IME 组合期：任何 Enter 组合都不提交、不换行，交给输入法上屏。
    #[test]
    fn ime_composition_never_submits() {
        let phases = [
            TurnLifecycleState::Idle,
            TurnLifecycleState::Preparing,
            TurnLifecycleState::Running,
        ];
        for phase in phases {
            assert_eq!(
                resolve(&KeyStroke::ime_enter(), phase, false),
                KeyOutcome::ConsumedByIme,
                "{phase:?} 组合期裸 Enter 不得提交"
            );
            assert_eq!(
                resolve(&KeyStroke::ime_shift_enter(), phase, false),
                KeyOutcome::ConsumedByIme,
                "{phase:?} 组合期 Shift+Enter 不得换行"
            );
            // 组合期即使按下修饰键也不得逸出提交。
            for stroke in [
                KeyStroke {
                    ime_composing: true,
                    ..KeyStroke::ctrl_enter()
                },
                KeyStroke {
                    ime_composing: true,
                    ..KeyStroke::ctrl_shift_enter()
                },
                KeyStroke {
                    ime_composing: true,
                    ..KeyStroke::alt_enter()
                },
            ] {
                assert_eq!(
                    resolve(&stroke, phase, false),
                    KeyOutcome::ConsumedByIme,
                    "{stroke:?} 组合期不得提交"
                );
            }
        }
    }

    /// 非 Enter 键不参与投递裁决（`Esc` 由焦点栈处理）。
    #[test]
    fn escape_and_other_keys_are_ignored() {
        let escape = KeyStroke {
            key: ComposerKey::Escape,
            ..KeyStroke::enter()
        };
        assert_eq!(
            resolve(&escape, TurnLifecycleState::Running, false),
            KeyOutcome::Ignored
        );
        let other = KeyStroke {
            key: ComposerKey::Other,
            ..KeyStroke::enter()
        };
        assert_eq!(
            resolve(&other, TurnLifecycleState::Idle, false),
            KeyOutcome::Ignored
        );
        // 组合选词期的 Esc 仍交给编辑器/焦点栈，不在本模块消费。
        let ime_escape = KeyStroke {
            ime_composing: true,
            ..escape
        };
        assert_eq!(
            resolve(&ime_escape, TurnLifecycleState::Running, false),
            KeyOutcome::Ignored
        );
    }

    /// 投递意图映射到协议投递策略（`NewTurn` 不携带活跃轮次策略）。
    #[test]
    fn intent_maps_to_protocol_behavior() {
        assert_eq!(SubmitIntent::NewTurn.behavior(), None);
        assert_eq!(
            SubmitIntent::Queue.behavior(),
            Some(ActiveTurnBehavior::Queue)
        );
        assert_eq!(
            SubmitIntent::Steer.behavior(),
            Some(ActiveTurnBehavior::Steer)
        );
        assert_eq!(
            SubmitIntent::Interrupt.behavior(),
            Some(ActiveTurnBehavior::Interrupt)
        );
    }

    /// 裸 Enter 判定与修饰键互斥。
    #[test]
    fn plain_enter_detection() {
        assert!(KeyStroke::enter().is_plain_enter());
        assert!(!KeyStroke::shift_enter().is_plain_enter());
        assert!(!KeyStroke::ctrl_enter().is_plain_enter());
        assert!(!KeyStroke::alt_enter().is_plain_enter());
        assert!(!KeyStroke::ime_enter().is_plain_enter());
    }
}
