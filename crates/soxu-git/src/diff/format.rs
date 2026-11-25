use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use super::{DiffChange, DiffError, DiffFileMode, DiffKind, GitError};

pub(super) type Tracked = BTreeMap<Vec<u8>, Option<DiffFileMode>>;

const MALFORMED: GitError = GitError::InvalidMetadata("畸形的 Git Diff 元数据");

pub(super) fn is_oid(bytes: &[u8]) -> bool {
    bytes.len() == 40 && bytes.iter().all(u8::is_ascii_hexdigit)
}

pub(super) fn commit_oid(bytes: &[u8]) -> Result<String, DiffError> {
    oid(bytes.strip_suffix(b"\n").ok_or(MALFORMED)?)?.ok_or_else(|| MALFORMED.into())
}

fn oid(bytes: &[u8]) -> Result<Option<String>, DiffError> {
    if !is_oid(bytes) {
        return Err(MALFORMED.into());
    }
    if bytes.iter().all(|byte| *byte == b'0') {
        return Ok(None);
    }
    Ok(Some(
        std::str::from_utf8(bytes)
            .map_err(|_| MALFORMED)?
            .to_ascii_lowercase(),
    ))
}

fn mode(bytes: &[u8]) -> Result<Option<DiffFileMode>, DiffError> {
    match bytes {
        b"000000" | b"120000" => Ok(None),
        b"100644" => Ok(Some(DiffFileMode::Regular)),
        b"100755" => Ok(Some(DiffFileMode::Executable)),
        b"160000" => Err(DiffError::Unsupported("子模块")),
        b"040000" => Err(DiffError::Unsupported("稀疏索引")),
        _ => Err(MALFORMED.into()),
    }
}

fn field<'a>(input: &mut &'a [u8], separator: u8) -> Result<&'a [u8], DiffError> {
    let offset = input
        .iter()
        .position(|byte| *byte == separator)
        .ok_or(MALFORMED)?;
    let (value, rest) = input.split_at(offset);
    *input = &rest[1..];
    Ok(value)
}

fn words<const COUNT: usize>(mut bytes: &[u8]) -> Result<[&[u8]; COUNT], DiffError> {
    let mut result = [b"".as_slice(); COUNT];
    let Some((last, earlier)) = result.split_last_mut() else {
        return Err(MALFORMED.into());
    };
    for item in earlier {
        *item = field(&mut bytes, b' ')?;
    }
    *last = bytes;
    Ok(result)
}

fn insert_path(paths: &mut Tracked, path: &[u8], mode_bytes: &[u8]) -> Result<(), DiffError> {
    if path.is_empty()
        || mode_bytes == b"000000"
        || paths.insert(path.to_vec(), mode(mode_bytes)?).is_some()
    {
        return Err(MALFORMED.into());
    }
    Ok(())
}

pub(super) fn index_paths(mut bytes: &[u8]) -> Result<Tracked, DiffError> {
    let mut paths = BTreeMap::new();
    while !bytes.is_empty() {
        let mut path = field(&mut bytes, 0)?;
        let metadata = field(&mut path, b'\t')?;
        let [mode_bytes, object, stage] = words(metadata)?;
        if matches!(stage, b"1" | b"2" | b"3") {
            return Err(DiffError::Unsupported("冲突索引"));
        }
        if stage != b"0" || oid(object)?.is_none() {
            return Err(MALFORMED.into());
        }
        insert_path(&mut paths, path, mode_bytes)?;
        for prefix in [
            b"  ctime: ".as_slice(),
            b"  mtime: ",
            b"  dev: ",
            b"  uid: ",
        ] {
            if !field(&mut bytes, b'\n')?.starts_with(prefix) {
                return Err(MALFORMED.into());
            }
        }
        let mut flags = field(&mut bytes, b'\n')?;
        if !field(&mut flags, b'\t')?.starts_with(b"  size: ") {
            return Err(MALFORMED.into());
        }
        let flags = flags.strip_prefix(b"flags: ").ok_or(MALFORMED)?;
        let text = std::str::from_utf8(flags).map_err(|_| MALFORMED)?;
        match u32::from_str_radix(text, 16).map_err(|_| MALFORMED)? {
            0 => {}
            flags if flags & 0x2000_0000 != 0 => {
                return Err(DiffError::Unsupported("intent-to-add"));
            }
            _ => return Err(DiffError::Unsupported("特殊索引标志")),
        }
    }
    Ok(paths)
}

pub(super) fn tree_paths(mut bytes: &[u8]) -> Result<Tracked, DiffError> {
    let mut paths = BTreeMap::new();
    while !bytes.is_empty() {
        let mut path = field(&mut bytes, 0)?;
        let metadata = field(&mut path, b'\t')?;
        let [mode_bytes, kind, object] = words(metadata)?;
        if oid(object)?.is_none() || !matches!(kind, b"blob" | b"commit") {
            return Err(MALFORMED.into());
        }
        insert_path(&mut paths, path, mode_bytes)?;
    }
    Ok(paths)
}

pub(super) fn changes(
    mut bytes: &[u8],
    authorized: &BTreeMap<&[u8], PathBuf>,
    compares_worktree: bool,
) -> Result<Vec<DiffChange>, DiffError> {
    let mut changes = BTreeMap::new();
    while bytes.starts_with(b":") {
        let raw = field(&mut bytes, 0)?;
        let path = field(&mut bytes, 0)?;
        let [old_mode, new_mode, old_oid, new_oid, status] = words(&raw[1..])?;
        let (old_mode, new_mode) = (mode(old_mode)?, mode(new_mode)?);
        let (old_oid, new_oid) = (oid(old_oid)?, oid(new_oid)?);
        let kind = match (status, old_mode, new_mode) {
            (b"A", None, Some(_)) => DiffKind::Added,
            (b"D", Some(_), None) => DiffKind::Deleted,
            (b"M", Some(_), Some(_)) => DiffKind::Modified,
            _ => return Err(MALFORMED.into()),
        };
        let change = DiffChange {
            path: path.to_vec(),
            kind,
            old_mode,
            new_mode,
            old_oid,
            new_oid,
            binary: false,
        };
        if !authorized.contains_key(path) || changes.insert(path, change).is_some() {
            return Err(MALFORMED.into());
        }
    }
    let mut counted = BTreeSet::new();
    while !bytes.is_empty() {
        let mut stat = field(&mut bytes, 0)?;
        let added = field(&mut stat, b'\t')?;
        let deleted = field(&mut stat, b'\t')?;
        let change = changes.get_mut(stat).ok_or(MALFORMED)?;
        if !counted.insert(stat) {
            return Err(MALFORMED.into());
        }
        change.binary = match (added, deleted) {
            (b"-", b"-") => true,
            (added, deleted) if is_decimal(added) && is_decimal(deleted) => false,
            _ => return Err(MALFORMED.into()),
        };
    }
    let mut result = Vec::with_capacity(counted.len());
    for (path, change) in changes {
        if counted.contains(path) {
            result.push(change);
        } else if !compares_worktree
            || change.kind != DiffKind::Modified
            || change.old_mode != change.new_mode
            || change.old_oid.is_none()
            || change.new_oid.is_some()
        {
            return Err(MALFORMED.into());
        }
    }
    Ok(result)
}

fn is_decimal(bytes: &[u8]) -> bool {
    !bytes.is_empty() && bytes.iter().all(u8::is_ascii_digit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_malformed_and_unauthorized_git_records() {
        let raw = b":100644 100644 1111111111111111111111111111111111111111 0000000000000000000000000000000000000000 M\0file\0";
        let authorized = BTreeMap::from([(b"file".as_slice(), PathBuf::from("file"))]);
        assert!(changes(raw, &authorized, false).is_err());
        assert!(
            changes(raw, &authorized, true)
                .expect("stat-only 候选")
                .is_empty()
        );
        assert!(changes(&[raw.as_slice(), b"1\t1\tfile"].concat(), &authorized, true).is_err());
        assert!(
            changes(
                &[raw.as_slice(), b"1\t-\tfile\0"].concat(),
                &authorized,
                true
            )
            .is_err()
        );
        assert!(
            changes(
                &[raw.as_slice(), b"1\t1\tfile\0"].concat(),
                &BTreeMap::new(),
                true,
            )
            .is_err()
        );
        assert!(index_paths(b"100644 1111111111111111111111111111111111111111 0\tfile\0").is_err());
    }
}
