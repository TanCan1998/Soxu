//! M6 三组域契约共用的受校验标量：幂等键、环境变量名与工作区相对路径。
//!
//! 这些标量一律在反序列化阶段完成校验，非法负载直接拒绝；
//! 服务端仍需按授权目录与环境白名单复核，类型校验不授予执行权限。

use serde::{Deserialize, Serialize};

/// 幂等键最大长度（字节）。
pub const IDEMPOTENCY_KEY_MAX_LEN: usize = 128;
/// 环境变量名最大长度（字节）。
pub const ENV_KEY_MAX_LEN: usize = 128;
/// 工作区相对路径最大长度（字节）。
pub const WORKSPACE_RELATIVE_PATH_MAX_LEN: usize = 1024;

/// 客户端幂等键：同一键重复提交必须收敛到同一结果，不得重复执行副作用。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct IdempotencyKey(String);

impl IdempotencyKey {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for IdempotencyKey {
    type Error = &'static str;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.is_empty() || value.len() > IDEMPOTENCY_KEY_MAX_LEN {
            return Err("幂等键长度必须在 1..=128 字节之间");
        }
        if !value.bytes().all(|byte| byte.is_ascii_graphic()) {
            return Err("幂等键只允许可见 ASCII 字符，禁止空白与控制字符");
        }
        Ok(Self(value))
    }
}

impl From<IdempotencyKey> for String {
    fn from(value: IdempotencyKey) -> Self {
        value.0
    }
}

/// 受控环境变量名（`^[A-Za-z_][A-Za-z0-9_]*$`）。值与原始环境快照一律不进协议。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct EnvKey(String);

impl EnvKey {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for EnvKey {
    type Error = &'static str;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.is_empty() || value.len() > ENV_KEY_MAX_LEN {
            return Err("环境变量名长度必须在 1..=128 字节之间");
        }
        let mut bytes = value.bytes();
        let first_ok = bytes
            .next()
            .is_some_and(|byte| byte == b'_' || byte.is_ascii_alphabetic());
        if !first_ok || !bytes.all(|byte| byte == b'_' || byte.is_ascii_alphanumeric()) {
            return Err("环境变量名必须以字母或下划线开头，且只包含字母、数字与下划线");
        }
        Ok(Self(value))
    }
}

impl From<EnvKey> for String {
    fn from(value: EnvKey) -> Self {
        value.0
    }
}

/// 严格限制在工作区内的相对路径。
///
/// 反序列化拒绝绝对路径、盘符路径、`..` 越级与空段，
/// 防止脚本工作目录或 PTY 起点逃出授权工作区。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct WorkspaceRelativePath(String);

impl WorkspaceRelativePath {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for WorkspaceRelativePath {
    type Error = &'static str;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.is_empty() || value.len() > WORKSPACE_RELATIVE_PATH_MAX_LEN {
            return Err("工作区相对路径长度必须在 1..=1024 字节之间");
        }
        if value.bytes().any(|byte| matches!(byte, 0 | b'\n' | b'\r')) {
            return Err("工作区相对路径禁止控制字符");
        }
        if value.starts_with('/') || value.starts_with('\\') {
            return Err("必须使用工作区内相对路径，禁止绝对路径");
        }
        if value.contains(':') {
            return Err("工作区相对路径禁止盘符与盘符相对写法");
        }
        if value
            .split(['/', '\\'])
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
        {
            return Err("工作区相对路径禁止空段、`.` 与 `..`");
        }
        Ok(Self(value))
    }
}

impl From<WorkspaceRelativePath> for String {
    fn from(value: WorkspaceRelativePath) -> Self {
        value.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_escaping_workspace_paths() {
        for bad in [
            "/etc/passwd",
            "\\\\server\\share",
            "C:\\Users",
            "..",
            "src/../../etc",
            "a//b",
            "./a",
            "",
        ] {
            assert!(
                WorkspaceRelativePath::try_from(bad.to_owned()).is_err(),
                "{bad} 应被拒绝"
            );
        }
        for good in ["src", "crates/soxu-protocol", "a\\b", "设计/草案"] {
            assert!(
                WorkspaceRelativePath::try_from(good.to_owned()).is_ok(),
                "{good} 应被接受"
            );
        }
    }

    #[test]
    fn env_key_follows_posix_identifier_shape() {
        assert!(EnvKey::try_from("PATH".to_owned()).is_ok());
        assert!(EnvKey::try_from("_MISE_X1".to_owned()).is_ok());
        assert!(EnvKey::try_from("1PATH".to_owned()).is_err());
        assert!(EnvKey::try_from("PATH=X".to_owned()).is_err());
        assert!(EnvKey::try_from(String::new()).is_err());
    }

    #[test]
    fn idempotency_key_rejects_whitespace_and_control_bytes() {
        assert!(IdempotencyKey::try_from("run-2026-11-28".to_owned()).is_ok());
        assert!(IdempotencyKey::try_from("a b".to_owned()).is_err());
        assert!(IdempotencyKey::try_from("a\nb".to_owned()).is_err());
        assert!(IdempotencyKey::try_from("x".repeat(129)).is_err());
    }
}
