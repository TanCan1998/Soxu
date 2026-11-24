//! Magpie 自动发现、Skills/MCP 路由感知与未安装环境原生降级适配层。

use serde::{Deserialize, Serialize};
use std::path::Path;

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn falls_back_to_standalone_when_missing() {
        let missing = PathBuf::from("__non_existent_magpie_dir_for_test__");
        assert_eq!(
            detect_mode_from_config_dir(&missing),
            EnvironmentMode::StandaloneNative
        );
    }
}
