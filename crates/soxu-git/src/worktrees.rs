use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::{GitError, GitProbe};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreePath(Vec<u8>);

impl WorktreePath {
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn to_path_buf(&self) -> Result<PathBuf, &'static str> {
        #[cfg(unix)]
        let path = {
            use std::ffi::OsString;
            use std::os::unix::ffi::OsStringExt;
            PathBuf::from(OsString::from_vec(self.0.clone()))
        };
        #[cfg(not(unix))]
        let path = {
            let text = std::str::from_utf8(&self.0)
                .map_err(|_| "本平台不支持非 UTF-8 工作树路径，原始字节仍保留")?;
            let drive =
                text.as_bytes().get(1) == Some(&b':') && text.as_bytes()[0].is_ascii_alphabetic();
            let rest = if drive { &text[2..] } else { text };
            if rest
                .chars()
                .any(|character| character.is_control() || "\\:<>\"|?*".contains(character))
            {
                return Err("本平台不支持该工作树路径字符，原始字节仍保留");
            }
            for component in rest.split('/') {
                let stem = component
                    .split('.')
                    .next()
                    .unwrap_or_default()
                    .to_ascii_uppercase();
                if component.ends_with(['.', ' '])
                    || ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str())
                    || ((stem.starts_with("COM") || stem.starts_with("LPT"))
                        && stem.len() == 4
                        && matches!(stem.as_bytes()[3], b'1'..=b'9'))
                {
                    return Err("本平台路径身份可能归一化，原始字节仍保留");
                }
            }
            PathBuf::from(text)
        };
        if !path.is_absolute() {
            return Err("本平台无法表示该绝对工作树路径，原始字节仍保留");
        }
        Ok(path)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorktreeKind {
    Main,
    Linked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorktreeHead {
    Bare,
    Unavailable { oid: String, detached: bool },
    Attached { oid: String, branch: Vec<u8> },
    Detached { oid: String },
    Unborn { oid: String, branch: Vec<u8> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeAnnotation {
    pub reason: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeEntry {
    pub kind: WorktreeKind,
    pub path: WorktreePath,
    pub head: WorktreeHead,
    pub locked: Option<WorktreeAnnotation>,
    pub prunable: Option<WorktreeAnnotation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeInventory {
    pub entries: Vec<WorktreeEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("非法 worktree porcelain -z 输入：记录 {record}，{reason}")]
pub struct WorktreeParseError {
    pub record: usize,
    pub reason: &'static str,
}

#[derive(Debug, thiserror::Error)]
pub enum WorktreeInventoryError {
    #[error(transparent)]
    Git(#[from] GitError),
    #[error(transparent)]
    Parse(#[from] WorktreeParseError),
}

impl GitProbe {
    pub async fn list_worktrees(
        &self,
        directory: impl AsRef<Path>,
    ) -> Result<WorktreeInventory, WorktreeInventoryError> {
        let directory = directory.as_ref();
        if !directory.is_absolute() {
            return Err(GitError::RelativePath.into());
        }
        let directory = directory.canonicalize().map_err(GitError::Io)?;
        if !directory.is_dir() {
            return Err(GitError::NotDirectory.into());
        }
        let output = self
            .run_command(&directory, "worktree", &["list", "--porcelain", "-z"])
            .await?;
        if !output.status.success() {
            return Err(GitError::CommandFailed {
                operation: "worktree",
                code: output.status.code(),
                stderr: output.stderr,
            }
            .into());
        }
        Ok(parse_worktrees(&output.stdout)?)
    }
}

pub fn parse_worktrees(input: &[u8]) -> Result<WorktreeInventory, WorktreeParseError> {
    let mut entries = Vec::new();
    let mut paths = HashSet::new();
    let mut width = None;
    let mut offset = 0;
    while offset < input.len() {
        let record = entries.len() + 1;
        let error = |reason| WorktreeParseError { record, reason };
        let mut fields = Vec::new();
        loop {
            let end = input[offset..]
                .iter()
                .position(|byte| *byte == 0)
                .ok_or_else(|| error("属性或记录分隔符截断"))?;
            let field = &input[offset..offset + end];
            offset += end + 1;
            if field.is_empty() {
                break;
            }
            fields.push(field);
            if offset == input.len() {
                return Err(error("缺少空记录终止符"));
            }
        }
        let first = fields.first().ok_or_else(|| error("空工作树记录"))?;
        let path = first
            .strip_prefix(b"worktree ")
            .filter(|path_bytes| absolute_git_path(path_bytes))
            .ok_or_else(|| error("首属性必须是非空绝对 worktree 路径"))?;
        if !paths.insert(path.to_vec()) {
            return Err(error("重复工作树路径"));
        }
        let mut seen = HashSet::new();
        let mut oid = None;
        let mut branch = None;
        let mut bare = false;
        let mut detached = false;
        let mut locked = None;
        let mut prunable = None;
        for field in &fields[1..] {
            let (key, value) = match field.iter().position(|byte| *byte == b' ') {
                Some(separator_index) => (
                    &field[..separator_index],
                    Some(&field[separator_index + 1..]),
                ),
                None => (*field, None),
            };
            if !seen.insert(key) {
                return Err(error("重复属性"));
            }
            match (key, value) {
                (b"HEAD", Some(bytes)) => {
                    if !matches!(bytes.len(), 40 | 64) || !bytes.iter().all(u8::is_ascii_hexdigit) {
                        return Err(error("HEAD 必须是完整 SHA-1 或 SHA-256"));
                    }
                    if width.is_some_and(|oid_width| oid_width != bytes.len()) {
                        return Err(error("同批 OID 算法宽度不一致"));
                    }
                    width = Some(bytes.len());
                    oid = Some(String::from_utf8(bytes.to_vec()).expect("ASCII OID"));
                }
                (b"branch", Some(bytes)) if valid_branch_ref(bytes) => {
                    branch = Some(bytes.to_vec());
                }
                (b"bare", None) => bare = true,
                (b"detached", None) => detached = true,
                (b"locked", reason) => {
                    locked = Some(WorktreeAnnotation {
                        reason: reason.map(<[u8]>::to_vec),
                    })
                }
                (b"prunable", reason) => {
                    prunable = Some(WorktreeAnnotation {
                        reason: reason.map(<[u8]>::to_vec),
                    })
                }
                (b"worktree", _) => return Err(error("记录内部再次出现 worktree")),
                (b"HEAD" | b"branch" | b"bare" | b"detached", _) => {
                    return Err(error("属性值非法"));
                }
                _ => return Err(error("未知属性，拒绝误认未来状态")),
            }
        }
        let head = match (bare, detached, oid, branch) {
            (true, false, None, None) => WorktreeHead::Bare,
            (false, detached, Some(oid), None) if oid.bytes().all(|byte| byte == b'0') => {
                WorktreeHead::Unavailable { oid, detached }
            }
            (false, true, Some(oid), None) => WorktreeHead::Detached { oid },
            (false, false, Some(oid), Some(branch)) => {
                if oid.bytes().all(|byte| byte == b'0') {
                    WorktreeHead::Unborn { oid, branch }
                } else {
                    WorktreeHead::Attached { oid, branch }
                }
            }
            _ => return Err(error("bare、HEAD 与 branch/detached 状态缺失或冲突")),
        };
        entries.push(WorktreeEntry {
            kind: if entries.is_empty() {
                WorktreeKind::Main
            } else {
                WorktreeKind::Linked
            },
            path: WorktreePath(path.to_vec()),
            head,
            locked,
            prunable,
        });
    }
    if entries.is_empty() {
        return Err(WorktreeParseError {
            record: 1,
            reason: "缺少主工作树记录",
        });
    }
    Ok(WorktreeInventory { entries })
}

fn absolute_git_path(path: &[u8]) -> bool {
    let remainder = if let Some(remainder) = path.strip_prefix(b"//") {
        let components: Vec<_> = remainder.split(|byte| *byte == b'/').collect();
        if components.len() < 2 {
            return false;
        }
        remainder
    } else if let Some(remainder) = path.strip_prefix(b"/") {
        if remainder.is_empty() {
            return true;
        }
        remainder
    } else if path.len() >= 3 && path[0].is_ascii_alphabetic() && &path[1..3] == b":/" {
        if path.len() == 3 {
            return true;
        }
        &path[3..]
    } else {
        return false;
    };
    remainder
        .split(|byte| *byte == b'/')
        .all(|component| !component.is_empty() && component != b"." && component != b"..")
}

fn valid_branch_ref(branch: &[u8]) -> bool {
    branch.starts_with(b"refs/heads/")
        && branch.len() > 11
        && !branch
            .iter()
            .any(|byte| *byte <= b' ' || *byte == 0x7f || b"~^:?*[\\".contains(byte))
        && !branch.windows(2).any(|pair| pair == b".." || pair == b"@{")
        && !branch.ends_with(b".")
        && branch.split(|byte| *byte == b'/').all(|component| {
            !component.is_empty() && !component.starts_with(b".") && !component.ends_with(b".lock")
        })
}
