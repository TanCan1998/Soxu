//! `paseo.json` 受管脚本清单的**只读**解析与严格校验。
//!
//! 红线：清单只描述「程序 + 固定参数 + 受控透传参数」的结构化条目，
//! 不接受任何 shell 字符串命令。历史形态 `"scripts": { "x": { "command": "..." } }`
//! 一律以可执行错误模型拒绝，绝不尝试按空白切分后当作参数数组执行。

use serde::Deserialize;
use soxu_protocol::{
    ActionableError, ErrorCode, ManagedScriptEntry, ScriptId, ScriptWorkdirPolicy,
};
use std::path::Path;

/// 清单文件名（在工作区根内固定）。
pub const PASEO_MANIFEST_FILE: &str = "paseo.json";
/// 清单文件字节上限：超限直接拒绝，避免读入不可控大文件。
pub const PASEO_MANIFEST_MAX_BYTES: u64 = 256 * 1024;
/// 单个工作区允许声明的脚本条数上限。
pub const SCRIPT_MANIFEST_MAX_ENTRIES: usize = 64;

/// 解析后的受管脚本清单。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScriptManifest {
    scripts: Vec<ManagedScriptEntry>,
}

#[derive(Deserialize)]
struct ManifestWire {
    #[serde(default)]
    scripts: Option<serde_json::Value>,
}

impl ScriptManifest {
    /// 只读加载工作区清单。
    ///
    /// 文件缺失视为「没有受管脚本」返回空清单；存在但不可读、超限、
    /// 非 UTF-8 或结构非法时返回可执行错误，绝不降级为部分执行。
    pub fn load(workspace_root: &Path) -> Result<Self, ActionableError> {
        let path = workspace_root.join(PASEO_MANIFEST_FILE);
        let metadata = match std::fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(err) => {
                return Err(ActionableError::new(
                    ErrorCode::ValidationFailed,
                    "无法读取工作区脚本清单",
                    "该工作区的受管脚本本次全部不可用",
                    "请确认 paseo.json 存在且当前用户可读",
                )
                .with_diagnostic(format!("metadata={:?}", err.kind())));
            }
        };
        if !metadata.is_file() {
            return Err(invalid_manifest(
                "paseo.json 必须是普通文件",
                "请把受管脚本清单放在工作区根的 paseo.json 普通文件中",
            ));
        }
        if metadata.len() > PASEO_MANIFEST_MAX_BYTES {
            return Err(invalid_manifest(
                &format!("paseo.json 超过 {PASEO_MANIFEST_MAX_BYTES} 字节上限"),
                "请精简清单条目后重试；本单不读取超大清单",
            ));
        }
        let raw = std::fs::read(&path).map_err(|err| {
            ActionableError::new(
                ErrorCode::ValidationFailed,
                "无法读取工作区脚本清单",
                "该工作区的受管脚本本次全部不可用",
                "请确认当前用户对 paseo.json 有读权限",
            )
            .with_diagnostic(format!("read={:?}", err.kind()))
        })?;
        let text = String::from_utf8(raw).map_err(|_| {
            invalid_manifest(
                "paseo.json 不是合法 UTF-8",
                "请以 UTF-8 重新保存清单，本单不接受其他编码",
            )
        })?;
        Self::parse(&text)
    }

    /// 解析清单文本（纯函数，便于注入面回归）。
    pub fn parse(raw: &str) -> Result<Self, ActionableError> {
        if raw.len() as u64 > PASEO_MANIFEST_MAX_BYTES {
            return Err(invalid_manifest(
                &format!("paseo.json 超过 {PASEO_MANIFEST_MAX_BYTES} 字节上限"),
                "请精简清单条目后重试",
            ));
        }
        let wire: ManifestWire = serde_json::from_str(raw).map_err(|err| {
            invalid_manifest(
                "paseo.json 结构非法",
                "请按受管脚本条目结构声明；本单拒绝任何无法结构化解析的清单",
            )
            .with_diagnostic(err.to_string())
        })?;
        let Some(scripts) = wire.scripts else {
            return Ok(Self::default());
        };
        if scripts.is_object() {
            return Err(invalid_manifest(
                "paseo.json 的 `scripts` 必须是数组形式的受管条目",
                "本单不接受 `scripts: { 名称: { command: \"...\" } }` 形态的 shell 字符串命令；\
                 请改用 `program` + `argument_policy.fixed_arguments` 的结构化声明",
            ));
        }
        let entries: Vec<ManagedScriptEntry> = serde_json::from_value(scripts).map_err(|err| {
            invalid_manifest(
                "paseo.json 中存在不合法的受管脚本条目",
                "请逐条核对 program/argument_policy/workdir/env_policy/risk 字段；\
                 非法条目会导致整份清单拒绝，不会只跳过其中一条",
            )
            .with_diagnostic(err.to_string())
        })?;
        Self::from_entries(entries)
    }

    /// 按已校验条目构造清单，并做跨条目的唯一性与条数校验。
    pub fn from_entries(entries: Vec<ManagedScriptEntry>) -> Result<Self, ActionableError> {
        if entries.len() > SCRIPT_MANIFEST_MAX_ENTRIES {
            return Err(invalid_manifest(
                &format!("受管脚本条数超过 {SCRIPT_MANIFEST_MAX_ENTRIES} 上限"),
                "请拆分工作区或删减条目后重试",
            ));
        }
        let mut seen = std::collections::BTreeSet::new();
        for entry in &entries {
            if !seen.insert(entry.script_id.as_str().to_owned()) {
                return Err(invalid_manifest(
                    &format!("受管脚本标识重复：{}", entry.script_id.as_str()),
                    "请保证每个 script_id 在工作区内唯一",
                ));
            }
        }
        Ok(Self { scripts: entries })
    }

    /// 清单条目（保持文件声明顺序）。
    #[must_use]
    pub fn scripts(&self) -> &[ManagedScriptEntry] {
        &self.scripts
    }

    /// 按标识取条目。
    #[must_use]
    pub fn get(&self, script_id: &ScriptId) -> Option<&ManagedScriptEntry> {
        self.scripts
            .iter()
            .find(|entry| entry.script_id == *script_id)
    }

    /// 是否没有任何受管脚本。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.scripts.is_empty()
    }
}

/// 解析脚本工作目录策略到受控绝对路径，并复核仍在工作区根内。
///
/// 词法层由协议 `WorkspaceRelativePath` 拒绝 `..` 与绝对路径；本函数再做
/// **真实路径**复核（符号链接/reparse 指向区外也不放行），失败一律拒绝执行。
pub fn resolve_workdir(
    workspace_root: &Path,
    policy: &ScriptWorkdirPolicy,
) -> Result<std::path::PathBuf, ActionableError> {
    if !workspace_root.is_absolute() {
        return Err(ActionableError::new(
            ErrorCode::ValidationFailed,
            "工作区根必须是绝对路径",
            "该脚本未执行",
            "请以绝对路径挂载工作区后重试",
        ));
    }
    let root = std::fs::canonicalize(workspace_root).map_err(|err| {
        ActionableError::new(
            ErrorCode::WorkspaceDamaged,
            "无法解析工作区根真实路径",
            "该脚本未执行",
            "请确认工作区目录仍然存在且可访问",
        )
        .with_diagnostic(format!("canonicalize={:?}", err.kind()))
    })?;
    if !root.is_dir() {
        return Err(ActionableError::new(
            ErrorCode::WorkspaceDamaged,
            "工作区根不是目录",
            "该脚本未执行",
            "请重新登记工作区目录后重试",
        ));
    }
    let candidate = match policy {
        ScriptWorkdirPolicy::WorkspaceRoot => root.clone(),
        ScriptWorkdirPolicy::Subdirectory { relative_path } => root.join(relative_path.as_str()),
    };
    let resolved = std::fs::canonicalize(&candidate).map_err(|err| {
        ActionableError::new(
            ErrorCode::ValidationFailed,
            "脚本工作目录不存在或不可访问",
            "该脚本未执行",
            "请在 paseo.json 中改用已存在的工作区内子目录",
        )
        .with_diagnostic(format!("canonicalize={:?}", err.kind()))
    })?;
    if !resolved.is_dir() {
        return Err(ActionableError::new(
            ErrorCode::ValidationFailed,
            "脚本工作目录不是目录",
            "该脚本未执行",
            "请修正 paseo.json 的 workdir 声明",
        ));
    }
    if !resolved.starts_with(&root) {
        return Err(ActionableError::new(
            ErrorCode::ValidationFailed,
            "脚本工作目录越出工作区",
            "该脚本未执行，未派生子进程",
            "请把 workdir 限定在工作区内；越界（含符号链接指向区外）一律拒绝",
        ));
    }
    Ok(resolved)
}

/// 把 `canonicalize` 结果还原为适合传给子进程的路径。
///
/// Windows 的 `canonicalize` 会附加 `\\?\` 扩展长度前缀；直接把它交给
/// `Command::current_dir` / ConPTY 会让子进程 cwd 契约与 `process.cwd()` 等
/// 按原样路径比较的消费方失配（见 `docs/ops/LESSONS.md` 的 L-013 记录）。
/// 这里只做前缀还原，不改变路径语义；越界判定仍用未还原的规范路径。
#[must_use]
pub fn child_facing_path(path: &Path) -> std::path::PathBuf {
    #[cfg(windows)]
    {
        let raw = path.as_os_str().to_string_lossy();
        if let Some(rest) = raw.strip_prefix(r"\\?\UNC\") {
            return std::path::PathBuf::from(format!(r"\\{rest}"));
        }
        if let Some(rest) = raw.strip_prefix(r"\\?\") {
            return std::path::PathBuf::from(rest);
        }
        path.to_path_buf()
    }
    #[cfg(not(windows))]
    {
        path.to_path_buf()
    }
}

fn invalid_manifest(summary: &str, remediation: &str) -> ActionableError {
    ActionableError::new(
        ErrorCode::ValidationFailed,
        summary,
        "该工作区的受管脚本本次全部不可用，没有任何脚本被执行",
        remediation,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn entry_json(script_id: &str) -> serde_json::Value {
        json!({
            "script_id": script_id,
            "display_name": "运行检查",
            "program": "cargo",
            "argument_policy": {
                "fixed_arguments": ["check"],
                "accepts_user_arguments": false,
                "max_user_arguments": 0,
                "max_user_argument_bytes": 64
            },
            "workdir": { "kind": "workspace_root" },
            "risk": "low"
        })
    }

    #[test]
    fn parses_array_manifest_in_declaration_order() {
        let raw = json!({ "scripts": [entry_json("check"), entry_json("lint")] }).to_string();
        let manifest = ScriptManifest::parse(&raw).expect("清单应可解析");
        assert_eq!(
            manifest
                .scripts()
                .iter()
                .map(|entry| entry.script_id.as_str())
                .collect::<Vec<_>>(),
            ["check", "lint"]
        );
    }

    #[test]
    fn missing_scripts_key_yields_empty_manifest() {
        let manifest = ScriptManifest::parse("{}").expect("空对象应可解析");
        assert!(manifest.is_empty());
    }

    #[test]
    fn rejects_legacy_shell_string_object_form() {
        let raw = json!({
            "scripts": { "dev": { "command": "pwsh -NoProfile -Command \"cargo run\"" } }
        })
        .to_string();
        let err = ScriptManifest::parse(&raw).expect_err("shell 字符串形态必须拒绝");
        assert_eq!(err.code, ErrorCode::ValidationFailed);
        assert!(err.summary_zh.contains("数组"));
        assert!(err.remediation_zh.contains("program"));
    }

    #[test]
    fn rejects_duplicate_script_identifiers() {
        let raw = json!({ "scripts": [entry_json("check"), entry_json("check")] }).to_string();
        let err = ScriptManifest::parse(&raw).expect_err("重复标识必须拒绝");
        assert!(err.summary_zh.contains("重复"));
    }

    #[test]
    fn rejects_shell_interpreter_program_and_injection_arguments() {
        let mut shell = entry_json("check");
        shell["program"] = json!("pwsh");
        let raw = json!({ "scripts": [shell] }).to_string();
        assert!(ScriptManifest::parse(&raw).is_err(), "解释器必须拒绝");

        let mut injected = entry_json("check");
        injected["argument_policy"]["fixed_arguments"] = json!(["--flag\nrm -rf /"]);
        let raw = json!({ "scripts": [injected] }).to_string();
        assert!(ScriptManifest::parse(&raw).is_err(), "含换行参数必须拒绝");
    }

    #[test]
    fn rejects_escaping_subdirectory_workdir() {
        let mut entry = entry_json("check");
        entry["workdir"] = json!({ "kind": "subdirectory", "relative_path": "../.." });
        let raw = json!({ "scripts": [entry] }).to_string();
        assert!(
            ScriptManifest::parse(&raw).is_err(),
            "越界 workdir 必须拒绝"
        );
    }

    #[test]
    fn rejects_oversized_manifest_text() {
        let padding = "x".repeat(PASEO_MANIFEST_MAX_BYTES as usize + 1);
        let raw = json!({ "scripts": [], "padding": padding }).to_string();
        let err = ScriptManifest::parse(&raw).expect_err("超限清单必须拒绝");
        assert!(err.summary_zh.contains("上限"));
    }

    #[test]
    fn resolves_workspace_root_and_subdirectory_with_containment() {
        let root = tempfile::tempdir().expect("临时工作区");
        let nested = root.path().join("crates");
        std::fs::create_dir_all(&nested).expect("建子目录");

        let root_dir =
            resolve_workdir(root.path(), &ScriptWorkdirPolicy::WorkspaceRoot).expect("根目录");
        assert_eq!(
            root_dir,
            std::fs::canonicalize(root.path()).expect("规范根")
        );

        let sub = soxu_protocol::WorkspaceRelativePath::try_from("crates".to_owned())
            .expect("合法相对路径");
        let sub_dir = resolve_workdir(
            root.path(),
            &ScriptWorkdirPolicy::Subdirectory { relative_path: sub },
        )
        .expect("子目录");
        assert_eq!(sub_dir, std::fs::canonicalize(&nested).expect("规范子目录"));

        let missing = soxu_protocol::WorkspaceRelativePath::try_from("nope".to_owned())
            .expect("合法相对路径");
        assert!(
            resolve_workdir(
                root.path(),
                &ScriptWorkdirPolicy::Subdirectory {
                    relative_path: missing
                }
            )
            .is_err(),
            "不存在的子目录必须拒绝"
        );
    }

    #[test]
    fn missing_manifest_file_is_empty_not_error() {
        let root = tempfile::tempdir().expect("临时工作区");
        let manifest = ScriptManifest::load(root.path()).expect("缺失清单视为空");
        assert!(manifest.is_empty());
    }
}
