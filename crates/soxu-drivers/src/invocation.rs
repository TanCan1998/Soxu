use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::{DriverError, NativeCliKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlSupport {
    Native,
    Process,
    DeferredToCaller,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    pub stream: bool,
    pub ephemeral: bool,
    pub queue: ControlSupport,
    pub steer: ControlSupport,
    pub interrupt: ControlSupport,
    pub discover_history: bool,
    pub resume: bool,
    pub fork: bool,
}

impl NativeCliKind {
    /// 表示适配器实现的接口。CLI 是否已安装、其版本及登录状态由部署环境决定。
    pub const fn capabilities(self) -> Capabilities {
        use ControlSupport::{DeferredToCaller, Native, Process, Unavailable};

        let pi = matches!(self, Self::Pi);
        Capabilities {
            stream: true,
            ephemeral: matches!(
                self,
                Self::Codex | Self::Pi | Self::ClaudeCode | Self::QwenCode
            ),
            queue: if pi { Native } else { DeferredToCaller },
            steer: if pi { Native } else { Unavailable },
            interrupt: if pi { Native } else { Process },
            discover_history: !matches!(self, Self::OpenCode | Self::Antigravity),
            resume: true,
            fork: matches!(
                self,
                Self::ClaudeCode | Self::Codex | Self::Pi | Self::OpenCode | Self::QwenCode
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Persistence {
    Native,
    Ephemeral,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunTarget {
    /// Daemon 提供其创建并管理的独立目录。
    QuickChat(PathBuf),
    Directory(PathBuf),
}

impl RunTarget {
    pub fn directory(&self) -> &Path {
        match self {
            Self::QuickChat(path) | Self::Directory(path) => path,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionMode {
    New,
    /// Pi 接受绝对会话文件路径或原生 ID，其他 CLI 接受原生 ID。
    Resume(OsString),
    Fork(OsString),
}

pub struct RunRequest {
    pub target: RunTarget,
    pub prompt: String,
    pub model: Option<String>,
    pub persistence: Persistence,
    pub session: SessionMode,
}

impl RunRequest {
    pub fn new(target: RunTarget, prompt: impl Into<String>) -> Self {
        Self {
            target,
            prompt: prompt.into(),
            model: None,
            persistence: Persistence::Native,
            session: SessionMode::New,
        }
    }
}

pub(crate) enum Input {
    Text,
    Rpc,
    Argument,
}

pub(crate) fn invocation(
    kind: NativeCliKind,
    request: &RunRequest,
) -> Result<(Vec<OsString>, Input), DriverError> {
    use NativeCliKind::{Antigravity, ClaudeCode, Codex, Copilot, OpenCode, Pi, QwenCode};

    let capabilities = kind.capabilities();
    let ephemeral = request.persistence == Persistence::Ephemeral;
    let session = match &request.session {
        SessionMode::New => None,
        SessionMode::Resume(id) => Some((false, id)),
        SessionMode::Fork(id) if capabilities.fork => Some((true, id)),
        SessionMode::Fork(_) => return Err(DriverError::Unsupported("fork")),
    };
    if ephemeral && !capabilities.ephemeral {
        return Err(DriverError::Unsupported("ephemeral"));
    }
    if ephemeral && session.is_some() {
        return Err(DriverError::InvalidRequest("ephemeral resume/fork"));
    }
    if request.prompt.trim().is_empty() || request.prompt.contains('\0') {
        return Err(DriverError::InvalidRequest("prompt"));
    }
    if let Some((_, id)) = session {
        let valid = id.to_str().is_some_and(|id| {
            (!id.is_empty()
                && !id.starts_with('-')
                && id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte)))
                || (kind == Pi && Path::new(id).is_absolute())
        });
        if !valid {
            return Err(DriverError::InvalidRequest("session id"));
        }
    }

    let (base, input): (&[&str], _) = match kind {
        Codex => (
            &[
                "--ask-for-approval",
                "never",
                "-c",
                "sandbox_mode=\"read-only\"",
                "exec",
            ],
            Input::Text,
        ),
        ClaudeCode => (
            &[
                "--print",
                "--verbose",
                "--output-format",
                "stream-json",
                "--include-partial-messages",
                "--tools",
                "",
                "--strict-mcp-config",
                "--mcp-config",
                "{\"mcpServers\":{}}",
                "--safe-mode",
                "--restricted",
                "--disable-slash-commands",
            ],
            Input::Text,
        ),
        Pi => (
            &[
                "--mode",
                "rpc",
                "--no-tools",
                "--no-extensions",
                "--no-mcp",
                "--no-skills",
                "--no-prompt-templates",
                "--no-context-files",
                "--no-approve",
                "--offline",
            ],
            Input::Rpc,
        ),
        OpenCode => (&["run", "--format", "json"], Input::Text),
        Antigravity => (
            &[
                "--output-format",
                "stream-json",
                "--mode",
                "plan",
                "--sandbox",
                "--disable-slash-commands",
            ],
            Input::Argument,
        ),
        Copilot => (
            &[
                "--output-format",
                "json",
                "--no-ask-user",
                "--disable-builtin-mcps",
                "--no-auto-update",
                "--no-custom-instructions",
                "--disable-client-telemetry",
                "--available-tools=soxu_no_native_tools",
                "--stream",
                "on",
            ],
            Input::Text,
        ),
        QwenCode => (
            &[
                "--output-format",
                "stream-json",
                "--include-partial-messages",
                "--bare",
                "--safe-mode",
                "--approval-mode",
                "plan",
                "--max-tool-calls",
                "0",
                "--exclude-tools",
                "edit",
                "get_goal",
                "notebook_edit",
                "read_file",
                "run_shell_command",
                "update_goal",
            ],
            Input::Text,
        ),
    };
    let mut args: Vec<OsString> = base.iter().map(OsString::from).collect();
    if kind == Codex {
        args.extend(session.map(|(fork, _)| OsString::from(if fork { "fork" } else { "resume" })));
        args.extend(["--json", "--skip-git-repo-check"].map(OsString::from));
        if session.is_none() {
            args.extend(["--sandbox", "read-only"].map(OsString::from));
        }
    }
    if ephemeral {
        args.extend(
            match kind {
                Codex => &["--ephemeral"][..],
                ClaudeCode => &["--no-session-persistence"],
                Pi => &["--no-session"],
                QwenCode => &["--chat-recording=false"],
                _ => return Err(DriverError::Unsupported("ephemeral")),
            }
            .iter()
            .map(OsString::from),
        );
    }
    if let Some(model) = &request.model {
        if model.is_empty() || model.contains('\0') {
            return Err(DriverError::InvalidRequest("model"));
        }
        args.push(format!("--model={model}").into());
    }
    if let Some((fork, id)) = session {
        let flag = match kind {
            Codex => None,
            ClaudeCode | QwenCode => Some("--resume"),
            Pi if fork => Some("--fork"),
            Pi | OpenCode => Some("--session"),
            Antigravity => Some("--conversation"),
            Copilot => None,
        };
        if fork && matches!(kind, ClaudeCode | QwenCode | OpenCode) {
            args.push(
                if kind == OpenCode {
                    "--fork"
                } else {
                    "--fork-session"
                }
                .into(),
            );
        }
        if kind == Copilot {
            let mut argument = OsString::from("--resume=");
            argument.push(id);
            args.push(argument);
        } else {
            args.extend(flag.map(OsString::from));
            args.push(id.clone());
        }
    }
    match kind {
        Codex => args.push("-".into()),
        Antigravity => args.push(format!("--print={}", request.prompt).into()),
        _ => {}
    }
    Ok((args, input))
}
