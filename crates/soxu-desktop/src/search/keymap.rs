//! 命令面板键位语义（06 页 §二「双段 `Esc` 隔离逻辑」与底部键位提示栏）。
//!
//! 键位裁决是纯函数：[`resolve`] 只读写 [`PaletteState`]，不读定时器、不碰 GPU。
//! 三条硬规则：
//!
//! 1. **`Cmd/Ctrl + K` 打开**：面板关闭时打开并定位首行；已打开时幂等忽略
//!    （不做「再按一次关闭」，避免与 `Esc` 的两段语义混淆）。
//! 2. **双段 `Esc`**：输入框有文字 → 只清空搜索框并保留焦点（[`KeyOutcome::Cleared`]）；
//!    输入框为空 → 关闭面板并还原打开前的焦点层（[`KeyOutcome::Closed`]）。
//! 3. **上下键只移动光标**：`↑↓`（及 `Ctrl+P/Ctrl+N`）只在既有结果集上移动选中，
//!    绝不触发重算或重新检索，光标闪烁也不会引起全表抖动。
//!
//! `⇥ / ⇧⇥` 按分区跳转，落到相邻非空分区的首行（对照 t3code
//! `commandPaletteItems.ts` 的 `nextPaletteIndex` 取模回绕）。

use crate::search::model::SearchCatalog;
use crate::search::{KeyOutcome, PaletteState};

/// 命令面板可识别的按键。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaletteKey {
    /// `Cmd + K`（macOS）/ `Ctrl + K`（Windows/Linux）：打开面板。
    Toggle,
    /// `Esc`：双段语义（先清空，再关闭）。
    Escape,
    /// `↑` / `Ctrl + P`。
    Up,
    /// `↓` / `Ctrl + N`。
    Down,
    /// `⇥`：跳到下一个分区的首行。
    NextSection,
    /// `⇧⇥`：跳到上一个分区的首行。
    PrevSection,
    /// `↵`：执行当前选中行。
    Confirm,
}

impl PaletteKey {
    /// 全部按键（键位表自检用）。
    pub const ALL: [Self; 7] = [
        Self::Toggle,
        Self::Escape,
        Self::Up,
        Self::Down,
        Self::NextSection,
        Self::PrevSection,
        Self::Confirm,
    ];

    /// 该按键在底部提示栏的文案（`Enter` 之外的按键均有提示）。
    pub const fn hint(self) -> Option<&'static str> {
        match self {
            Self::Toggle => Some("⌘K 打开"),
            Self::Escape => Some("Esc 清空/关闭"),
            Self::Up | Self::Down => Some("↑↓ 选择"),
            Self::NextSection | Self::PrevSection => Some("⇥ 分区跳转"),
            Self::Confirm => Some("↵ 执行"),
        }
    }
}

/// 底部键位提示栏（06 页 §二第 3 段，`36px`，四组提示）。
pub const FOOTER_HINTS: [&str; 4] = ["↑↓ 选择", "↵ 执行", "⇥ 分区跳转", "Esc 清空/关闭"];

/// 打开面板的快捷键文案（macOS 用 `⌘K`，其余平台 `Ctrl+K`）。
pub const fn open_chord_label(mac_like: bool) -> &'static str {
    if mac_like { "⌘K" } else { "Ctrl+K" }
}

/// 标题栏触发条占位文案（`pages/01` §1 中央搜索条）。
pub const TRIGGER_PLACEHOLDER: &str = "搜索会话 / 文件 / 命令...";

/// 打开面板一次所需的上下文（时间戳与纯内存目录）。
#[derive(Clone, Copy, Debug)]
pub struct KeyContext<'a> {
    /// 当前时刻（毫秒，由宿主注入）。
    pub now_ms: u64,
    /// 纯内存检索目录（零 I/O 快照）。
    pub catalog: &'a SearchCatalog,
}

impl<'a> KeyContext<'a> {
    /// 构造上下文。
    pub const fn new(now_ms: u64, catalog: &'a SearchCatalog) -> Self {
        Self { now_ms, catalog }
    }
}

/// 键位裁决。
pub fn resolve(state: &mut PaletteState, key: PaletteKey, ctx: &KeyContext<'_>) -> KeyOutcome {
    match key {
        PaletteKey::Toggle => {
            if state.is_open() {
                KeyOutcome::Ignored
            } else {
                state.open(ctx.catalog);
                KeyOutcome::Opened
            }
        }
        PaletteKey::Escape => {
            if !state.is_open() {
                KeyOutcome::Ignored
            } else if state.has_query() {
                state.clear_query(ctx.catalog);
                KeyOutcome::Cleared
            } else {
                state.close();
                KeyOutcome::Closed
            }
        }
        PaletteKey::Up => state.move_selection(-1),
        PaletteKey::Down => state.move_selection(1),
        PaletteKey::NextSection => state.jump_section(1),
        PaletteKey::PrevSection => state.jump_section(-1),
        PaletteKey::Confirm => state.confirm(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 键位表自检：每个按键都有提示文案，打开文案随平台切换。
    #[test]
    fn key_table_is_complete() {
        for key in PaletteKey::ALL {
            assert!(key.hint().is_some(), "按键 {:?} 缺少提示文案", key);
        }
        assert_eq!(open_chord_label(true), "⌘K");
        assert_eq!(open_chord_label(false), "Ctrl+K");
        assert_eq!(FOOTER_HINTS.len(), 4);
        assert_eq!(FOOTER_HINTS[3], "Esc 清空/关闭");
        assert_ne!(TRIGGER_PLACEHOLDER, "");
    }
}
