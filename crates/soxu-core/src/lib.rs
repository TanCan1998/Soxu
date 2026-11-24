//! Soxu 核心领域模型、编排状态与插件扩展预留槽位。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// 工作区文件系统隔离模式。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkspaceIsolation {
    /// 直接绑定本地目录。
    Local,
    /// 基于 Git Worktree 的独立分支/PR 隔离。
    Worktree { branch: String, base_ref: String },
}

/// 插件系统预留运行时钩子接口（前期不实现插件市场，使用空实现零开销运行）。
pub trait RuntimeExtensionHook: Send + Sync {
    /// 在启动原生 Agent CLI 进程前允许注入或转换环境变量。
    fn transform_launch_env(&self, _env: &mut BTreeMap<String, String>) {}
}

/// 默认空扩展宿主。
#[derive(Debug, Default)]
pub struct NoopExtensionHost;

impl RuntimeExtensionHook for NoopExtensionHost {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noop_extension_preserves_env() {
        let host = NoopExtensionHost;
        let mut env = BTreeMap::from([("RUST_LOG".to_owned(), "info".to_owned())]);
        host.transform_launch_env(&mut env);
        assert_eq!(env.get("RUST_LOG").map(String::as_str), Some("info"));
    }
}
