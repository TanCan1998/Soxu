use std::path::PathBuf;

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("非法 porcelain v2 输入：记录 {record}，{reason}")]
pub struct ParseError {
    pub record: usize,
    pub reason: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GitPath(Vec<u8>);

impl GitPath {
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn to_path_buf(&self) -> Result<PathBuf, &'static str> {
        #[cfg(unix)]
        {
            use std::ffi::OsString;
            use std::os::unix::ffi::OsStringExt;
            Ok(PathBuf::from(OsString::from_vec(self.0.clone())))
        }
        #[cfg(not(unix))]
        {
            let text = std::str::from_utf8(&self.0).map_err(|_| "本平台不支持非 UTF-8 Git 路径")?;
            if text
                .chars()
                .any(|character| character.is_control() || "\\:<>\"|?*".contains(character))
            {
                return Err("本平台不支持该 Git 路径字符，原始字节仍保留");
            }
            for component in text.split('/') {
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
            Ok(PathBuf::from(text))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ChangeStatus {
    pub index: char,
    pub worktree: char,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SubmoduleStatus {
    pub is_submodule: bool,
    pub commit_changed: bool,
    pub tracked_changes: bool,
    pub untracked_changes: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OrdinaryChange {
    pub status: ChangeStatus,
    pub submodule: SubmoduleStatus,
    pub modes: [String; 3],
    pub oids: [String; 2],
    pub path: GitPath,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenameChange {
    pub change: OrdinaryChange,
    pub kind: char,
    pub similarity: u8,
    pub original_path: GitPath,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UnmergedChange {
    pub status: ChangeStatus,
    pub submodule: SubmoduleStatus,
    pub modes: [String; 4],
    pub oids: [String; 3],
    pub path: GitPath,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum StatusEntry {
    Ordinary(OrdinaryChange),
    Renamed(RenameChange),
    Unmerged(UnmergedChange),
    Untracked(GitPath),
    Ignored(GitPath),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatusHeader {
    pub name: Vec<u8>,
    pub value: Vec<u8>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct StatusSnapshot {
    pub headers: Vec<StatusHeader>,
    pub entries: Vec<StatusEntry>,
}

impl StatusSnapshot {
    pub fn header(&self, name: &[u8]) -> Option<&[u8]> {
        self.headers
            .iter()
            .find(|header| header.name == name)
            .map(|header| header.value.as_slice())
    }
}

pub fn parse_status(input: &[u8]) -> Result<StatusSnapshot, ParseError> {
    if input.is_empty() {
        return Ok(StatusSnapshot::default());
    }
    if input.last() != Some(&0) {
        return Err(ParseError {
            record: 0,
            reason: "缺少末尾 NUL",
        });
    }
    let mut records = input[..input.len() - 1].split(|byte| *byte == 0);
    let mut snapshot = StatusSnapshot::default();
    let mut record_index = 0;
    let mut oid_length = None;
    while let Some(record) = records.next() {
        record_index += 1;
        let error = |reason| ParseError {
            record: record_index,
            reason,
        };
        let entry = match record.first() {
            Some(b'#') => {
                let header = record
                    .strip_prefix(b"# ")
                    .ok_or_else(|| error("非法头部前缀"))?;
                let separator = header
                    .iter()
                    .position(|byte| *byte == b' ')
                    .ok_or_else(|| error("头部缺少值"))?;
                let (name, value) = (&header[..separator], &header[separator + 1..]);
                if name.is_empty() || value.is_empty() || snapshot.header(name).is_some() {
                    return Err(error("头部为空或重复"));
                }
                validate_header(name, value).map_err(error)?;
                if name == b"branch.oid" && value != b"(initial)" {
                    validate_oid_length(&mut oid_length, value.len()).map_err(error)?;
                }
                snapshot.headers.push(StatusHeader {
                    name: name.to_vec(),
                    value: value.to_vec(),
                });
                continue;
            }
            Some(b'1') => {
                let fields = fields(record, 9).map_err(error)?;
                if fields[0] != b"1" {
                    return Err(error("非法普通记录前缀"));
                }
                let change = ordinary(&fields, 8).map_err(error)?;
                if [change.status.index, change.status.worktree]
                    .iter()
                    .any(|status| ['U', 'R', 'C'].contains(status))
                {
                    return Err(error("冲突或重命名必须使用对应记录类型"));
                }
                StatusEntry::Ordinary(change)
            }
            Some(b'2') => {
                let fields = fields(record, 10).map_err(error)?;
                if fields[0] != b"2" {
                    return Err(error("非法重命名记录前缀"));
                }
                let change = ordinary(&fields, 9).map_err(error)?;
                let score = ascii(fields[8]).map_err(error)?;
                let kind = score.as_bytes().first().copied().map(char::from);
                if !matches!(kind, Some('R' | 'C'))
                    || score.len() < 2
                    || !score.as_bytes()[1..].iter().all(u8::is_ascii_digit)
                {
                    return Err(error("非法重命名或复制评分"));
                }
                let similarity = score[1..].parse::<u8>().map_err(|_| error("非法相似度"))?;
                let kind = kind.ok_or_else(|| error("缺少重命名类型"))?;
                if similarity > 100
                    || ![change.status.index, change.status.worktree].contains(&kind)
                    || [change.status.index, change.status.worktree].contains(&'U')
                {
                    return Err(error("重命名类型或相似度不匹配"));
                }
                let original = records.next().ok_or_else(|| error("缺少重命名源路径"))?;
                let original_path = path(original).map_err(error)?;
                record_index += 1;
                StatusEntry::Renamed(RenameChange {
                    change,
                    kind,
                    similarity,
                    original_path,
                })
            }
            Some(b'u') => {
                let fields = fields(record, 11).map_err(error)?;
                if fields[0] != b"u"
                    || !matches!(
                        fields[1],
                        b"DD" | b"AU" | b"UD" | b"UA" | b"DU" | b"AA" | b"UU"
                    )
                {
                    return Err(error("非法冲突状态"));
                }
                StatusEntry::Unmerged(UnmergedChange {
                    status: status(fields[1]).map_err(error)?,
                    submodule: submodule(fields[2]).map_err(error)?,
                    modes: [
                        mode(fields[3]).map_err(error)?,
                        mode(fields[4]).map_err(error)?,
                        mode(fields[5]).map_err(error)?,
                        mode(fields[6]).map_err(error)?,
                    ],
                    oids: [
                        oid(fields[7]).map_err(error)?,
                        oid(fields[8]).map_err(error)?,
                        oid(fields[9]).map_err(error)?,
                    ],
                    path: path(fields[10]).map_err(error)?,
                })
            }
            Some(b'?' | b'!') if record.get(1) == Some(&b' ') => {
                let path = path(&record[2..]).map_err(error)?;
                if record[0] == b'?' {
                    StatusEntry::Untracked(path)
                } else {
                    StatusEntry::Ignored(path)
                }
            }
            _ => return Err(error("未知或空记录")),
        };
        let oids: &[String] = match &entry {
            StatusEntry::Ordinary(change) => &change.oids,
            StatusEntry::Renamed(rename) => &rename.change.oids,
            StatusEntry::Unmerged(change) => &change.oids,
            StatusEntry::Untracked(_) | StatusEntry::Ignored(_) => &[],
        };
        for oid in oids {
            validate_oid_length(&mut oid_length, oid.len()).map_err(|reason| ParseError {
                record: record_index,
                reason,
            })?;
        }
        snapshot.entries.push(entry);
    }
    Ok(snapshot)
}

fn validate_oid_length(expected: &mut Option<usize>, length: usize) -> Result<(), &'static str> {
    if expected.is_some_and(|expected| expected != length) {
        return Err("同一状态流中对象标识长度不一致");
    }
    *expected = Some(length);
    Ok(())
}

fn fields(record: &[u8], count: usize) -> Result<Vec<&[u8]>, &'static str> {
    let fields: Vec<_> = record.splitn(count, |byte| *byte == b' ').collect();
    if fields.len() != count || fields.iter().any(|field| field.is_empty()) {
        return Err("字段数量错误或字段为空");
    }
    Ok(fields)
}

fn ordinary(fields: &[&[u8]], path_index: usize) -> Result<OrdinaryChange, &'static str> {
    Ok(OrdinaryChange {
        status: status(fields[1])?,
        submodule: submodule(fields[2])?,
        modes: [mode(fields[3])?, mode(fields[4])?, mode(fields[5])?],
        oids: [oid(fields[6])?, oid(fields[7])?],
        path: path(fields[path_index])?,
    })
}

fn ascii(value: &[u8]) -> Result<&str, &'static str> {
    if !value.is_ascii() {
        return Err("元数据不是 ASCII");
    }
    std::str::from_utf8(value).map_err(|_| "元数据编码错误")
}

pub(crate) fn oid(value: &[u8]) -> Result<String, &'static str> {
    if !matches!(value.len(), 40 | 64) || !value.iter().all(u8::is_ascii_hexdigit) {
        return Err("对象标识必须是完整 SHA-1 或 SHA-256");
    }
    Ok(ascii(value)?.to_owned())
}

fn mode(value: &[u8]) -> Result<String, &'static str> {
    if value.len() != 6 || !value.iter().all(|byte| (b'0'..=b'7').contains(byte)) {
        return Err("文件模式必须是六位八进制");
    }
    Ok(ascii(value)?.to_owned())
}

fn status(value: &[u8]) -> Result<ChangeStatus, &'static str> {
    if value.len() != 2 || !value.iter().all(|byte| b".MADRCUT".contains(byte)) || value == b".." {
        return Err("非法 XY 状态");
    }
    Ok(ChangeStatus {
        index: char::from(value[0]),
        worktree: char::from(value[1]),
    })
}

fn submodule(value: &[u8]) -> Result<SubmoduleStatus, &'static str> {
    if value != b"N..."
        && !(value.len() == 4
            && value[0] == b'S'
            && b".C".contains(&value[1])
            && b".M".contains(&value[2])
            && b".U".contains(&value[3]))
    {
        return Err("非法子模块状态");
    }
    Ok(SubmoduleStatus {
        is_submodule: value[0] == b'S',
        commit_changed: value[1] == b'C',
        tracked_changes: value[2] == b'M',
        untracked_changes: value[3] == b'U',
    })
}

fn path(value: &[u8]) -> Result<GitPath, &'static str> {
    if value.is_empty()
        || value.contains(&0)
        || value.starts_with(b"/")
        || value
            .split(|byte| *byte == b'/')
            .any(|component| component.is_empty() || component == b"." || component == b"..")
    {
        return Err("路径为空、绝对路径或含目录逃逸");
    }
    Ok(GitPath(value.to_vec()))
}

fn validate_header(name: &[u8], value: &[u8]) -> Result<(), &'static str> {
    match name {
        b"branch.oid" if value != b"(initial)" => oid(value).map(|_| ()),
        b"branch.ab" => {
            let counts: Vec<_> = value.split(|byte| *byte == b' ').collect();
            if counts.len() != 2
                || counts[0].first() != Some(&b'+')
                || counts[1].first() != Some(&b'-')
                || counts
                    .iter()
                    .any(|count| count.len() < 2 || !count[1..].iter().all(u8::is_ascii_digit))
            {
                return Err("非法 ahead/behind 计数");
            }
            Ok(())
        }
        _ => Ok(()),
    }
}
