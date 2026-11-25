//! 本机已装原生 Agent CLI 驱动层（纯原生流式/进程复用，不引入 ACP 适配）。

use serde::{Deserialize, Serialize};

mod events;
mod history;
mod invocation;
mod process;

pub use events::{DriverEvent, Outcome, Telemetry, UsageScope};
pub use history::{HistoryEntry, HistoryListing, discover_history};
pub use invocation::{
    Capabilities, ControlSupport, Persistence, RunRequest, RunTarget, SessionMode,
};
pub use process::{Control, DriverError, NativeDriver, RunningTurn, TurnControl};

/// 支持复用的本机原生 Agent CLI 类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeCliKind {
    ClaudeCode,
    Codex,
    Pi,
    OpenCode,
    Antigravity,
    Copilot,
    QwenCode,
}

impl NativeCliKind {
    pub const ALL: [Self; 7] = [
        Self::ClaudeCode,
        Self::Codex,
        Self::Pi,
        Self::OpenCode,
        Self::Antigravity,
        Self::Copilot,
        Self::QwenCode,
    ];

    /// 返回对应 CLI 在系统 PATH 中的默认可执行程序名。
    #[must_use]
    pub const fn binary_name(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude",
            Self::Codex => "codex",
            Self::Pi => "pi",
            Self::OpenCode => "opencode",
            Self::Antigravity => "agy",
            Self::Copilot => "copilot",
            Self::QwenCode => "qwen",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_expected_binary_names() {
        assert_eq!(NativeCliKind::Pi.binary_name(), "pi");
        assert_eq!(NativeCliKind::Codex.binary_name(), "codex");
        assert_eq!(NativeCliKind::Antigravity.binary_name(), "agy");
    }
}
