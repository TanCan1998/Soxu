//! Magpie 自动发现、Skills/MCP 路由感知与未安装环境原生降级适配层。

use serde::{Deserialize, Serialize};
use std::{env, ffi::OsString, path::Path};

mod magpie;
mod native;

pub use magpie::{MagpieDiscovery, RouteSummary};
pub use native::{NativeAgent, NativeCli};

/// 环境发现运行模式。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentMode {
    /// 检测到已安装 Magpie，协同感知模型网关、Skill Router 与 MCP Router。
    MagpieManaged,
    /// 未检测到 Magpie，回退为直接扫描各原生 CLI 标准配置目录。
    StandaloneNative,
}

/// Magpie MCP 路由暴露方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpRouteMode {
    /// 经由统一 `magpie` MCP 聚合服务共享路由。
    Shared,
    /// 直接写入对应 Agent 或依赖项目启动工作目录（如 AOCI、CodeGraph）。
    Direct,
}

/// Magpie Skill 路由暴露方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillRouteMode {
    /// 经由 `magpie-skills` 索引按需加载。
    Routed,
    /// 直接链接或拷贝至目标 Agent 技能目录。
    Direct,
}

/// 根据指定配置目录是否存在 `library.json` 或 `gateway.json` 判断是否启用 Magpie 协同模式。
#[must_use]
pub fn detect_mode_from_config_dir(magpie_config_dir: &Path) -> EnvironmentMode {
    if magpie_config_dir.join("library.json").is_file()
        || magpie_config_dir.join("gateway.json").is_file()
    {
        EnvironmentMode::MagpieManaged
    } else {
        EnvironmentMode::StandaloneNative
    }
}

/// 单项探测结果。错误只保留类别，避免携带路径或原始错误文本。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeStatus {
    Available,
    Missing,
    Unreadable,
    Invalid,
    NotChecked,
}

impl ProbeStatus {
    fn from_io(error: std::io::Error) -> Self {
        match error.kind() {
            std::io::ErrorKind::NotFound => Self::Missing,
            std::io::ErrorKind::NotADirectory => Self::Invalid,
            _ => Self::Unreadable,
        }
    }
}

/// 显式列出的启动环境变量。布尔值表示变量已设置，包括空值。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvironmentPresence {
    pub path: bool,
    pub home: bool,
    pub userprofile: bool,
    pub xdg_config_home: bool,
    pub claude_config_dir: bool,
    pub codex_home: bool,
    pub pi_coding_agent_dir: bool,
    pub opencode_config_dir: bool,
    pub copilot_home: bool,
    pub qwen_home: bool,
    pub magpie_addr: bool,
}

impl EnvironmentPresence {
    fn read(get_env: &impl Fn(&str) -> Option<OsString>) -> Self {
        Self {
            path: get_env("PATH").is_some(),
            home: get_env("HOME").is_some(),
            userprofile: get_env("USERPROFILE").is_some(),
            xdg_config_home: get_env("XDG_CONFIG_HOME").is_some(),
            claude_config_dir: get_env("CLAUDE_CONFIG_DIR").is_some(),
            codex_home: get_env("CODEX_HOME").is_some(),
            pi_coding_agent_dir: get_env("PI_CODING_AGENT_DIR").is_some(),
            opencode_config_dir: get_env("OPENCODE_CONFIG_DIR").is_some(),
            copilot_home: get_env("COPILOT_HOME").is_some(),
            qwen_home: get_env("QWEN_HOME").is_some(),
            magpie_addr: get_env("MAGPIE_ADDR").is_some(),
        }
    }
}

/// 可直接序列化的启动环境摘要。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvironmentDiscovery {
    pub mode: EnvironmentMode,
    pub magpie: MagpieDiscovery,
    pub native_clis: [NativeCli; 7],
    pub environment: EnvironmentPresence,
}

/// 只读探测当前进程所在的启动环境。
#[must_use]
pub fn discover_environment() -> EnvironmentDiscovery {
    discover_environment_with(None, |name| env::var_os(name))
}

/// 使用指定的 Magpie 目录和环境变量读取器探测，可用于隔离夹具。
/// 目录为 `None` 时，从配置根目录推导 Magpie 目录。
#[must_use]
pub fn discover_environment_with(
    magpie_config_dir: Option<&Path>,
    get_env: impl Fn(&str) -> Option<OsString>,
) -> EnvironmentDiscovery {
    let paths = native::SearchPaths::read(&get_env);
    let default_magpie_dir = paths.config_home.as_ref().map(|root| root.join("magpie"));
    let config_dir = magpie_config_dir.or(default_magpie_dir.as_deref());
    let magpie = MagpieDiscovery::inspect(config_dir, paths.executable("magpie"));
    let mode = if magpie.executable == ProbeStatus::Available
        || config_dir
            .is_some_and(|dir| detect_mode_from_config_dir(dir) == EnvironmentMode::MagpieManaged)
    {
        EnvironmentMode::MagpieManaged
    } else {
        EnvironmentMode::StandaloneNative
    };

    EnvironmentDiscovery {
        mode,
        magpie,
        native_clis: paths.native_clis(&get_env),
        environment: EnvironmentPresence::read(&get_env),
    }
}

#[cfg(test)]
mod tests;
