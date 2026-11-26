//! 目标、名称与路径边界判定。
//!
//! 该层不做任何写入，只回答"这个目标能不能被本请求独占创建"。

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use super::RepositoryCreateError;
use super::platform::{self, AuthorizedDirectory};

/// 受控创建目标在检查时刻的状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositoryTargetState {
    /// 目标不存在。
    Missing,
    /// 目标存在且为受支持的普通目录。
    Directory,
    /// 目标存在且为普通文件。
    File,
    /// 目标存在但为链接、junction 或其他 reparse point。
    LinkOrReparse,
    /// 目标存在但无法读取状态。
    Inaccessible,
}

/// 枚举目标位置，区分不存在、链接与权限错误。
pub(super) fn inspect(target: &Path) -> Result<RepositoryTargetState, RepositoryCreateError> {
    match fs::symlink_metadata(target) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(RepositoryTargetState::Missing),
        Err(_) => Ok(RepositoryTargetState::Inaccessible),
        Ok(metadata) => {
            if metadata.is_symlink() || platform::is_reparse(&metadata) {
                Ok(RepositoryTargetState::LinkOrReparse)
            } else if metadata.is_dir() {
                Ok(RepositoryTargetState::Directory)
            } else {
                Ok(RepositoryTargetState::File)
            }
        }
    }
}

/// 全量枚举目录项（含隐藏项），返回是否为空且可独占读取。
pub(super) fn is_empty_directory(target: &Path) -> Result<bool, RepositoryCreateError> {
    let mut entries = fs::read_dir(target).map_err(RepositoryCreateError::Io)?;
    Ok(entries
        .next()
        .transpose()
        .map_err(RepositoryCreateError::Io)?
        .is_none())
}

/// 已存在目录的受控身份视图：必须是普通目录、可枚举且与持有句柄身份一致。
pub(super) fn authorized_existing(
    target: &Path,
) -> Result<Option<AuthorizedDirectory>, RepositoryCreateError> {
    match inspect(target)? {
        RepositoryTargetState::Missing => Ok(None),
        RepositoryTargetState::Directory => Ok(Some(AuthorizedDirectory::open(target)?)),
        RepositoryTargetState::File => Err(RepositoryCreateError::TargetExists),
        RepositoryTargetState::LinkOrReparse => Err(RepositoryCreateError::Unauthorized(
            "目标位置是链接或 reparse point",
        )),
        RepositoryTargetState::Inaccessible => {
            Err(RepositoryCreateError::Unauthorized("无法读取目标位置状态"))
        }
    }
}

/// 目标目录名必须是无歧义的可移植单段名称。
pub(super) fn portable_name(name: &str) -> bool {
    if name.is_empty()
        || name.len() > 64
        || !name.is_ascii()
        || matches!(name, "." | "..")
        || name.starts_with(['-', '.'])
        || name.ends_with(['.', ' ', '-'])
        || name.starts_with(' ')
        || name.bytes().any(|byte| byte == b' ')
    {
        return false;
    }
    portable_component(name)
}

/// 提交树中的单个路径分量必须是无歧义的可移植分量。
pub(super) fn portable_component(name: &str) -> bool {
    if name.is_empty()
        || name.len() > 255
        || !name.is_ascii()
        || matches!(name, "." | "..")
        || name.ends_with(['.', ' '])
        || name.starts_with(' ')
        || name.bytes().any(|byte| {
            byte.is_ascii_control()
                || matches!(byte, b'<' | b'>' | b':' | b'"' | b'\\' | b'|' | b'?' | b'*')
        })
    {
        return false;
    }
    !is_reserved(name)
}

fn is_reserved(name: &str) -> bool {
    let folded = name.to_ascii_lowercase();
    let stem = folded.split('.').next().unwrap_or_default();
    matches!(stem, "con" | "prn" | "aux" | "nul" | "com0" | "lpt0")
        || ((stem.starts_with("com") || stem.starts_with("lpt"))
            && stem.len() == 4
            && stem.as_bytes()[3].is_ascii_digit())
        || folded == ".git"
}

/// 路径的每一段都必须是无歧义分量且不含链接语义。
pub(super) fn portable_relative(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 4096
        && path.split('/').all(portable_component)
        && !path.starts_with('/')
        && !path.contains("//")
}

/// 拒绝 UNC、卷设备与交错前缀，只接受本地盘符或 POSIX 绝对路径。
pub(super) fn local_absolute(path: &Path) -> bool {
    if !path.is_absolute() {
        return false;
    }
    let text = path.to_string_lossy();
    if text.starts_with("\\\\") || text.starts_with("//") {
        return false;
    }
    !text.starts_with("\\\\.\\") && !text.starts_with("/dev/")
}

/// 祖先链上不得已经存在 Git 元数据，避免把目标嵌套进既有仓库。
pub(super) fn repository_at_or_above(
    path: &Path,
) -> Result<Option<PathBuf>, RepositoryCreateError> {
    for ancestor in path.ancestors() {
        match fs::symlink_metadata(ancestor.join(".git")) {
            Ok(metadata) => {
                if metadata.is_symlink() || platform::is_reparse(&metadata) {
                    return Err(RepositoryCreateError::Unauthorized(
                        "祖先目录的 .git 是链接或 reparse point",
                    ));
                }
                return Ok(Some(ancestor.to_owned()));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(_) => {
                return Err(RepositoryCreateError::Unauthorized(
                    "无法读取祖先目录的 .git",
                ));
            }
        }
    }
    Ok(None)
}

/// 任一方向的前缀重叠都视为源与目标重叠。
pub(super) fn overlaps(first: &Path, second: &Path) -> bool {
    let first_in_second =
        first.components().count() <= second.components().count() && second.starts_with(first);
    let second_in_first =
        second.components().count() <= first.components().count() && first.starts_with(second);
    first_in_second || second_in_first
}

/// 拼接受控目标路径，并复核结果仍在受控父根之下。
pub(super) fn child_of(root: &Path, name: &str) -> Result<PathBuf, RepositoryCreateError> {
    if !portable_name(name) {
        return Err(RepositoryCreateError::InvalidInput("目标名称"));
    }
    let target = root.join(name);
    if target.parent() != Some(root) {
        return Err(RepositoryCreateError::Unauthorized("目标不在受控父根之下"));
    }
    if target.components().any(|component| {
        !matches!(
            component,
            Component::Normal(_) | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(RepositoryCreateError::Unauthorized(
            "目标路径包含非普通分量",
        ));
    }
    Ok(target)
}
