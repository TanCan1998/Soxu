use crate::ProbeStatus;
use serde::{Deserialize, Serialize};
use std::{
    env,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

/// Soxu M2 支持的原生 CLI。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeAgent {
    Claude,
    Codex,
    Pi,
    Opencode,
    Agy,
    Copilot,
    Qwen,
}

impl NativeAgent {
    const ALL: [Self; 7] = [
        Self::Claude,
        Self::Codex,
        Self::Pi,
        Self::Opencode,
        Self::Agy,
        Self::Copilot,
        Self::Qwen,
    ];

    /// PATH 中搜索的命令名。
    #[must_use]
    pub const fn command(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Pi => "pi",
            Self::Opencode => "opencode",
            Self::Agy => "agy",
            Self::Copilot => "copilot",
            Self::Qwen => "qwen",
        }
    }

    fn config_dirs(
        self,
        paths: &SearchPaths,
        get_env: &impl Fn(&str) -> Option<OsString>,
    ) -> [Option<PathBuf>; 2] {
        let (override_var, home_subdir) = match self {
            Self::Claude => (Some("CLAUDE_CONFIG_DIR"), ".claude"),
            Self::Codex => (Some("CODEX_HOME"), ".codex"),
            Self::Pi => (Some("PI_CODING_AGENT_DIR"), ".pi/agent"),
            Self::Opencode => (Some("OPENCODE_CONFIG_DIR"), ""),
            Self::Agy => (None, ".gemini/antigravity"),
            Self::Copilot => (Some("COPILOT_HOME"), ".copilot"),
            Self::Qwen => (Some("QWEN_HOME"), ".qwen"),
        };
        let override_dir = override_var.and_then(|key| env_path(get_env, key));
        if let Some(directory) = override_dir {
            return [Some(directory), None];
        }
        if self == Self::Opencode {
            return [
                paths.config_home.as_ref().map(|root| root.join("opencode")),
                None,
            ];
        }
        [
            paths.home.as_ref().map(|home| home.join(home_subdir)),
            (self == Self::Agy)
                .then(|| paths.home.as_ref().map(|home| home.join(".antigravity")))
                .flatten(),
        ]
    }
}

/// 命令与配置目录分别探测。仅通过目录可读性判断配置目录状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeCli {
    pub agent: NativeAgent,
    pub executable: ProbeStatus,
    pub config_dir: ProbeStatus,
}

pub(crate) struct SearchPaths {
    home: Option<PathBuf>,
    pub(crate) config_home: Option<PathBuf>,
    path: Option<OsString>,
}

impl SearchPaths {
    pub(crate) fn read(get_env: &impl Fn(&str) -> Option<OsString>) -> Self {
        let home = if cfg!(windows) {
            env_path(get_env, "USERPROFILE").or_else(|| env_path(get_env, "HOME"))
        } else {
            env_path(get_env, "HOME")
        };
        let config_home = env_path(get_env, "XDG_CONFIG_HOME")
            .filter(|path| path.is_absolute())
            .or_else(|| home.as_ref().map(|home| home.join(".config")));
        Self {
            path: get_env("PATH"),
            home,
            config_home,
        }
    }

    pub(crate) fn native_clis(
        &self,
        get_env: &impl Fn(&str) -> Option<OsString>,
    ) -> [NativeCli; 7] {
        NativeAgent::ALL.map(|agent| NativeCli {
            agent,
            executable: self.executable(agent.command()),
            config_dir: first_available(
                agent
                    .config_dirs(self, get_env)
                    .into_iter()
                    .flatten()
                    .map(|directory| directory_status(&directory)),
            ),
        })
    }

    pub(crate) fn executable(&self, command: &str) -> ProbeStatus {
        #[cfg(windows)]
        const SUFFIXES: &[&str] = &[".com", ".exe", ".bat", ".cmd", ".ps1"];
        #[cfg(not(windows))]
        const SUFFIXES: &[&str] = &[""];

        let Some(path) = &self.path else {
            return ProbeStatus::NotChecked;
        };
        let names: Vec<_> = SUFFIXES
            .iter()
            .map(|suffix| format!("{command}{suffix}"))
            .collect();
        first_available(
            env::split_paths(path)
                .filter(|directory| !directory.as_os_str().is_empty())
                .flat_map(|directory| {
                    names
                        .iter()
                        .map(move |name| executable_status(&directory.join(name)))
                })
                .chain([ProbeStatus::Missing]),
        )
    }
}

fn env_path(get_env: &impl Fn(&str) -> Option<OsString>, name: &str) -> Option<PathBuf> {
    get_env(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn first_available(statuses: impl IntoIterator<Item = ProbeStatus>) -> ProbeStatus {
    let mut best = ProbeStatus::NotChecked;
    for status in statuses {
        match (status, best) {
            (ProbeStatus::Available, _) => return status,
            (ProbeStatus::Unreadable, _)
            | (ProbeStatus::Invalid, ProbeStatus::Missing | ProbeStatus::NotChecked)
            | (ProbeStatus::Missing, ProbeStatus::NotChecked) => best = status,
            _ => {}
        }
    }
    best
}

fn directory_status(path: &Path) -> ProbeStatus {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_dir() => {
            fs::read_dir(path).map_or_else(ProbeStatus::from_io, |_| ProbeStatus::Available)
        }
        Ok(_) => ProbeStatus::Invalid,
        Err(error) => ProbeStatus::from_io(error),
    }
}

fn executable_status(path: &Path) -> ProbeStatus {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if metadata.permissions().mode() & 0o111 == 0 {
                    return ProbeStatus::Invalid;
                }
            }
            ProbeStatus::Available
        }
        Ok(_) => ProbeStatus::Invalid,
        Err(error) => ProbeStatus::from_io(error),
    }
}
