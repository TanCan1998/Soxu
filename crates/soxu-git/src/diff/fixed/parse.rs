//! Git NUL 记录的严格解析：不使用有损 UTF-8 解码路径，并兼容 SHA-1 与 SHA-256。
//!
//! 与 [`super::super::format`] 分开维护：固定端点需要 64 位对象格式、重命名记录与
//! 行统计，而既有四基准的解析器必须保持原样，不能被扩大接受范围。

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use super::super::format::Tracked;
use super::{DiffError, DiffFileMode, FixedDiffChange, FixedDiffKind};
use crate::GitError;

const NUL: u8 = 0;
const TAB: u8 = 9;
const NEWLINE: u8 = 10;

fn malformed() -> DiffError {
    GitError::InvalidMetadata("畸形的固定端点 Diff 记录").into()
}

/// 完整 40/64 位十六进制 OID；全零不代表任何真实对象。
pub(super) fn is_oid(bytes: &[u8]) -> bool {
    matches!(bytes.len(), 40 | 64)
        && bytes.iter().all(u8::is_ascii_hexdigit)
        && bytes.iter().any(|byte| *byte != b'0')
}

/// 解析 rev-parse 输出的单行 OID。
pub(super) fn oid_line(bytes: &[u8]) -> Result<String, DiffError> {
    let bytes = bytes.strip_suffix(&[NEWLINE]).ok_or_else(malformed)?;
    if !is_oid(bytes) {
        return Err(malformed());
    }
    Ok(std::str::from_utf8(bytes)
        .map_err(|_| malformed())?
        .to_ascii_lowercase())
}

fn field<'a>(input: &mut &'a [u8], separator: u8) -> Result<&'a [u8], DiffError> {
    let offset = input
        .iter()
        .position(|byte| *byte == separator)
        .ok_or_else(malformed)?;
    let (value, rest) = input.split_at(offset);
    *input = &rest[1..];
    Ok(value)
}

fn mode(bytes: &[u8]) -> Result<Option<DiffFileMode>, DiffError> {
    match bytes {
        b"000000" => Ok(None),
        b"100644" => Ok(Some(DiffFileMode::Regular)),
        b"100755" => Ok(Some(DiffFileMode::Executable)),
        b"120000" => Err(DiffError::Unsupported("Git 符号链接条目")),
        b"160000" => Err(DiffError::Unsupported("子模块")),
        _ => Err(malformed()),
    }
}

fn object(bytes: &[u8], expected: Option<usize>) -> Result<Option<String>, DiffError> {
    if !matches!(bytes.len(), 40 | 64)
        || expected.is_some_and(|width| width != bytes.len())
        || !bytes.iter().all(u8::is_ascii_hexdigit)
    {
        return Err(malformed());
    }
    if bytes.iter().all(|byte| *byte == b'0') {
        return Ok(None);
    }
    Ok(Some(
        std::str::from_utf8(bytes)
            .map_err(|_| malformed())?
            .to_ascii_lowercase(),
    ))
}

fn track(paths: &mut Tracked, path: &[u8], mode_bytes: &[u8]) -> Result<(), DiffError> {
    if path.is_empty() || paths.contains_key(path) {
        return Err(malformed());
    }
    paths.insert(path.to_vec(), mode(mode_bytes)?);
    Ok(())
}

/// ls-tree -r -z --full-tree：<mode> SP <type> SP <oid> TAB <path> NUL
pub(super) fn tree_paths(mut bytes: &[u8]) -> Result<Tracked, DiffError> {
    let mut paths = Tracked::new();
    let mut width = None;
    while !bytes.is_empty() {
        let mut record = field(&mut bytes, NUL)?;
        let metadata = field(&mut record, TAB)?;
        let mut parts = metadata.split(|byte| *byte == b' ');
        let (Some(mode_bytes), Some(kind), Some(object_bytes), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(malformed());
        };
        if kind != b"blob" {
            return Err(DiffError::Unsupported("子模块"));
        }
        if object(object_bytes, width)?.is_none() {
            return Err(malformed());
        }
        width = Some(object_bytes.len());
        track(&mut paths, record, mode_bytes)?;
    }
    Ok(paths)
}

/// ls-files --stage -z：<mode> SP <oid> SP <stage> TAB <path> NUL
pub(super) fn index_paths(mut bytes: &[u8]) -> Result<Tracked, DiffError> {
    let mut paths = Tracked::new();
    let mut width = None;
    while !bytes.is_empty() {
        let mut record = field(&mut bytes, NUL)?;
        let metadata = field(&mut record, TAB)?;
        let mut parts = metadata.split(|byte| *byte == b' ');
        let (Some(mode_bytes), Some(object_bytes), Some(stage), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(malformed());
        };
        if stage != b"0" {
            return Err(DiffError::Unsupported("冲突索引"));
        }
        if object(object_bytes, width)?.is_none() {
            return Err(DiffError::Unsupported("intent-to-add"));
        }
        width = Some(object_bytes.len());
        track(&mut paths, record, mode_bytes)?;
    }
    Ok(paths)
}

/// ls-files --others 的 -z 列表：去重后的裸路径。
pub(super) fn paths(mut bytes: &[u8]) -> Result<Vec<Vec<u8>>, DiffError> {
    let mut result = BTreeSet::new();
    while !bytes.is_empty() {
        let path = field(&mut bytes, NUL)?;
        if path.is_empty() {
            return Err(malformed());
        }
        result.insert(path.to_vec());
    }
    Ok(result.into_iter().collect())
}

fn decimal(bytes: &[u8]) -> Result<u64, DiffError> {
    if bytes.is_empty() || !bytes.iter().all(u8::is_ascii_digit) {
        return Err(malformed());
    }
    std::str::from_utf8(bytes)
        .map_err(|_| malformed())?
        .parse()
        .map_err(|_| malformed())
}

/// diff --raw --numstat -z：先全部 raw 记录，再全部 numstat 记录。
pub(super) fn changes(
    mut bytes: &[u8],
    authorized: &BTreeMap<&[u8], PathBuf>,
    width: usize,
    worktree: bool,
) -> Result<Vec<FixedDiffChange>, DiffError> {
    let mut changes = BTreeMap::new();
    while bytes.starts_with(b":") {
        let header = field(&mut bytes, NUL)?;
        let fields: Vec<&[u8]> = header[1..].split(|byte| *byte == b' ').collect();
        if fields.len() != 5 {
            return Err(malformed());
        }
        let (old_mode, new_mode) = (mode(fields[0])?, mode(fields[1])?);
        let (old_oid, new_oid) = (
            object(fields[2], Some(width))?,
            object(fields[3], Some(width))?,
        );
        let source = field(&mut bytes, NUL)?;
        let kind = match fields[4] {
            b"A" if old_mode.is_none() && new_mode.is_some() && old_oid.is_none() => {
                FixedDiffKind::Added
            }
            b"D" if old_mode.is_some() && new_mode.is_none() && new_oid.is_none() => {
                FixedDiffKind::Deleted
            }
            b"M" if old_mode.is_some() && new_mode.is_some() => FixedDiffKind::Modified,
            status if status.starts_with(b"R") => {
                if old_mode.is_none() || new_mode.is_none() || decimal(&status[1..])? > 100 {
                    return Err(malformed());
                }
                FixedDiffKind::Renamed
            }
            b"T" => return Err(DiffError::Unsupported("端点间文件类型变化")),
            _ => return Err(malformed()),
        };
        let (path, old_path) = if kind == FixedDiffKind::Renamed {
            (field(&mut bytes, NUL)?, Some(source.to_vec()))
        } else {
            (source, None)
        };
        if !authorized.contains_key(path) || !authorized.contains_key(source) {
            return Err(malformed());
        }
        let change = FixedDiffChange {
            path: path.to_vec(),
            old_path,
            kind,
            old_mode,
            new_mode,
            old_oid,
            new_oid,
            binary: false,
            additions: None,
            deletions: None,
        };
        if changes.insert(path.to_vec(), change).is_some() {
            return Err(malformed());
        }
    }
    let mut counted = BTreeSet::new();
    while !bytes.is_empty() {
        let mut stats = field(&mut bytes, NUL)?;
        let additions = field(&mut stats, TAB)?;
        let deletions = field(&mut stats, TAB)?;
        let (path, source) = if stats.is_empty() {
            let old = field(&mut bytes, NUL)?;
            (field(&mut bytes, NUL)?, Some(old))
        } else {
            (stats, None)
        };
        let change = changes.get_mut(path).ok_or_else(malformed)?;
        if !counted.insert(path.to_vec()) || change.old_path.as_deref() != source {
            return Err(malformed());
        }
        match (additions, deletions) {
            (b"-", b"-") => change.binary = true,
            (added, deleted) => {
                change.additions = Some(decimal(added)?);
                change.deletions = Some(decimal(deleted)?);
            }
        }
    }
    let mut result = Vec::with_capacity(counted.len());
    for (path, change) in changes {
        if counted.contains(&path) {
            result.push(change);
        } else {
            // 工作树比较允许仅 raw 记录的修改条目：两侧内容相同而元数据已刷新。
            let tolerated = worktree
                && change.kind == FixedDiffKind::Modified
                && change.old_mode == change.new_mode
                && change.old_oid.is_some()
                && change.new_oid.is_none();
            if !tolerated {
                return Err(malformed());
            }
        }
    }
    Ok(result)
}
