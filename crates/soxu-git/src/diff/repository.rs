use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs::{self, File, Metadata};
use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::format::Tracked;
use super::{DiffError, GitError, OUTPUT_LIMIT, Runner};
use crate::WorktreeIdentity;
use crate::probe::native_path;

const SMALL_METADATA_LIMIT: u64 = 256 * 1024;

#[derive(PartialEq, Eq)]
pub(super) struct Layout {
    pub(super) identity: WorktreeIdentity,
    files: Vec<(PathBuf, Option<MetadataFile>)>,
}

#[derive(PartialEq, Eq)]
struct MetadataFile {
    stamp: Stamp,
    bytes: Vec<u8>,
}

#[derive(PartialEq, Eq)]
pub(super) struct Stamp {
    length: u64,
    modified: SystemTime,
}

impl Stamp {
    fn read(metadata: &Metadata) -> Result<Self, DiffError> {
        Ok(Self {
            length: metadata.len(),
            modified: metadata.modified()?,
        })
    }
}

impl Layout {
    pub(super) fn read(root: &Path) -> Result<Self, DiffError> {
        let marker = root.join(".git");
        let marker_metadata = metadata(&marker)?.ok_or(DiffError::InvalidWorktree)?;
        let mut files = Vec::new();
        let git_dir = if marker_metadata.is_dir() {
            marker
        } else {
            let descriptor = required_file(&marker, SMALL_METADATA_LIMIT)?;
            let git_dir = path_line(&descriptor.bytes, b"gitdir: ")?;
            files.push((marker, Some(descriptor)));
            root.join(git_dir).canonicalize()?
        };
        directory(&git_dir)?;
        let common_marker = git_dir.join("commondir");
        let common_dir =
            if let Some(descriptor) = metadata_file(&common_marker, SMALL_METADATA_LIMIT)? {
                let common_dir = git_dir
                    .join(path_line(&descriptor.bytes, b"")?)
                    .canonicalize()?;
                if !git_dir
                    .parent()
                    .is_some_and(|parent| parent == common_dir.join("worktrees"))
                {
                    return Err(DiffError::Unsupported("非标准 linked worktree 元数据"));
                }
                directory(&common_dir.join("worktrees"))?;
                let backlink = git_dir.join("gitdir");
                let target = required_file(&backlink, SMALL_METADATA_LIMIT)?;
                if git_dir
                    .join(path_line(&target.bytes, b"")?)
                    .canonicalize()?
                    != root.join(".git")
                {
                    return Err(DiffError::InvalidWorktree);
                }
                files.push((backlink, Some(target)));
                files.push((common_marker, Some(descriptor)));
                common_dir
            } else {
                if git_dir != root.join(".git") {
                    return Err(DiffError::Unsupported("分离 Git 目录"));
                }
                files.push((common_marker, None));
                git_dir.clone()
            };
        let objects = common_dir.join("objects");
        directory(&objects)?;
        for name in ["info", "pack"] {
            let path = objects.join(name);
            if metadata(&path)?.is_some_and(|entry| !entry.is_dir()) {
                return Err(DiffError::Unsupported("对象库目录类型"));
            }
        }
        for name in ["alternates", "http-alternates"] {
            if metadata(&objects.join("info").join(name))?.is_some() {
                return Err(DiffError::Unsupported("外部对象库"));
            }
        }
        check_packs(&objects.join("pack"))?;
        let info = common_dir.join("info");
        if metadata(&info)?.is_some_and(|entry| !entry.is_dir()) {
            return Err(DiffError::Unsupported("Git info 目录类型"));
        }
        for (path, limit) in [
            (common_dir.join("config"), SMALL_METADATA_LIMIT),
            (git_dir.join("config.worktree"), SMALL_METADATA_LIMIT),
            (git_dir.join("HEAD"), SMALL_METADATA_LIMIT),
            (info.join("attributes"), SMALL_METADATA_LIMIT),
            (git_dir.join("index"), OUTPUT_LIMIT),
        ] {
            let data = metadata_file(&path, limit)?;
            if data.is_none() && path == git_dir.join("HEAD") {
                return Err(DiffError::InvalidWorktree);
            }
            files.push((path, data));
        }
        Ok(Self {
            identity: WorktreeIdentity {
                root: root.to_owned(),
                git_dir,
                common_dir,
            },
            files,
        })
    }

    pub(super) async fn preflight(&self, runner: &mut Runner<'_>) -> Result<(), DiffError> {
        self.preflight_format(runner, false).await
    }

    // 固定端点单独允许 SHA-256，不扩大既有四基准的接受范围。
    pub(super) async fn preflight_format(
        &self,
        runner: &mut Runner<'_>,
        fixed: bool,
    ) -> Result<(), DiffError> {
        for (path, data) in &self.files {
            if data.is_some()
                && path
                    .file_name()
                    .is_some_and(|name| name == "config" || name == "config.worktree")
            {
                let config = runner.config(path).await?;
                check_config(&config, fixed)?;
            }
        }
        let kind = runner
            .read(
                "rev-parse",
                [
                    "--is-inside-work-tree",
                    "--is-bare-repository",
                    "--show-object-format",
                ],
            )
            .await?;
        match kind.as_slice() {
            b"true\nfalse\nsha1\n" => {}
            b"true\nfalse\nsha256\n" if fixed => {}
            b"true\nfalse\nsha256\n" => return Err(DiffError::Unsupported("SHA-256 对象格式")),
            _ => return Err(DiffError::InvalidWorktree),
        }
        for (option, expected) in [
            ("--show-toplevel", &self.identity.root),
            ("--absolute-git-dir", &self.identity.git_dir),
            ("--git-common-dir", &self.identity.common_dir),
        ] {
            let bytes = runner
                .read("rev-parse", ["--path-format=absolute", option])
                .await?;
            let actual = path_line(&bytes, b"")?;
            if !actual.is_absolute() || actual.canonicalize()? != *expected {
                return Err(DiffError::InvalidWorktree);
            }
        }
        let shared_index = runner.read("rev-parse", ["--shared-index-path"]).await?;
        if !shared_index.is_empty() && shared_index != b"\n" {
            return Err(DiffError::Unsupported("split index"));
        }
        Ok(())
    }
}

fn path_line(bytes: &[u8], prefix: &[u8]) -> Result<PathBuf, DiffError> {
    let value = bytes
        .strip_prefix(prefix)
        .and_then(|bytes| bytes.strip_suffix(b"\n"))
        .ok_or(DiffError::InvalidWorktree)?;
    let value = value.strip_suffix(b"\r").unwrap_or(value);
    if value.is_empty() || value.contains(&0) {
        return Err(DiffError::InvalidWorktree);
    }
    Ok(native_path(value)?)
}

fn check_config(bytes: &[u8], fixed: bool) -> Result<(), DiffError> {
    if bytes.is_empty() {
        return Ok(());
    }
    let body = bytes
        .strip_suffix(b"\0")
        .ok_or(GitError::InvalidMetadata("Git 配置记录缺少终止符"))?;
    for record in body.split(|byte| *byte == 0) {
        let mut fields = record.splitn(2, |byte| *byte == b'\n');
        let key = fields
            .next()
            .filter(|key| !key.is_empty())
            .ok_or(GitError::InvalidMetadata("Git 配置缺少键名"))?;
        let value = fields.next().unwrap_or_default();
        if key == b"include.path" || key.starts_with(b"includeif.") {
            return Err(DiffError::Unsupported("配置 include"));
        }
        if key.starts_with(b"filter.") && (key.ends_with(b".clean") || key.ends_with(b".process")) {
            return Err(GitError::ExternalFilter.into());
        }
        if key.starts_with(b"extensions.") {
            match key {
                b"extensions.worktreeconfig" => {}
                b"extensions.objectformat"
                    if value.eq_ignore_ascii_case(b"sha1")
                        || (fixed && value.eq_ignore_ascii_case(b"sha256")) => {}
                b"extensions.objectformat" => {
                    return Err(DiffError::Unsupported("非 SHA-1 对象格式"));
                }
                _ => return Err(DiffError::Unsupported("Git 仓库扩展")),
            }
        }
        if matches!(
            key,
            b"core.sparsecheckout" | b"index.sparse" | b"core.splitindex"
        ) && ![b"false".as_slice(), b"no", b"off", b"0"]
            .iter()
            .any(|off| value.eq_ignore_ascii_case(off))
        {
            return Err(DiffError::Unsupported(
                "sparse checkout、sparse index 或 split index",
            ));
        }
        if matches!(
            key,
            b"core.worktree" | b"diff.orderfile" | b"core.alternaterefscommand"
        ) || (key.starts_with(b"remote.")
            && (key.ends_with(b".promisor") || key.ends_with(b".partialclonefilter")))
        {
            return Err(DiffError::Unsupported("外部元数据来源或部分克隆配置"));
        }
    }
    Ok(())
}

fn check_packs(directory: &Path) -> Result<(), DiffError> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    for (offset, entry) in entries.enumerate() {
        if offset >= 4096 {
            return Err(DiffError::Limit("对象包目录项"));
        }
        let path = entry?.path();
        if path.extension() == Some(OsStr::new("promisor")) {
            return Err(DiffError::Unsupported("promisor 对象包"));
        }
        metadata(&path)?;
    }
    Ok(())
}

fn required_file(path: &Path, limit: u64) -> Result<MetadataFile, DiffError> {
    metadata_file(path, limit)?.ok_or(DiffError::InvalidWorktree)
}

fn metadata_file(path: &Path, limit: u64) -> Result<Option<MetadataFile>, DiffError> {
    let Some(metadata) = metadata(path)? else {
        return Ok(None);
    };
    if !metadata.is_file() {
        return Err(DiffError::Unsupported("Git 元数据文件类型"));
    }
    if metadata.len() > limit {
        return Err(DiffError::Limit("Git 元数据文件字节数"));
    }
    let mut bytes = Vec::new();
    File::open(path)?.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(DiffError::Limit("Git 元数据文件字节数"));
    }
    if bytes.len() as u64 != metadata.len() {
        return Err(DiffError::Changed);
    }
    Ok(Some(MetadataFile {
        stamp: Stamp::read(&metadata)?,
        bytes,
    }))
}

fn metadata(path: &Path) -> Result<Option<Metadata>, DiffError> {
    match path.symlink_metadata() {
        Ok(metadata) => {
            #[cfg(windows)]
            let reparse = {
                use std::os::windows::fs::MetadataExt;
                metadata.file_attributes() & 0x400 != 0
            };
            #[cfg(not(windows))]
            let reparse = false;
            if metadata.is_symlink() || reparse {
                return Err(DiffError::Unsupported("符号链接或 reparse point"));
            }
            Ok(Some(metadata))
        }
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn directory(path: &Path) -> Result<(), DiffError> {
    if !metadata(path)?.is_some_and(|entry| entry.is_dir()) {
        return Err(DiffError::InvalidWorktree);
    }
    Ok(())
}

pub(super) fn git_metadata_arg(path: &Path) -> Result<PathBuf, DiffError> {
    #[cfg(windows)]
    {
        use std::path::{Component, Prefix};

        if let Some(Component::Prefix(prefix)) = path.components().next() {
            match prefix.kind() {
                Prefix::VerbatimDisk(letter) => {
                    let root = PathBuf::from(format!("{}:\\", char::from(letter)));
                    return Ok(root.join(path.components().skip(2).collect::<PathBuf>()));
                }
                Prefix::Disk(_) => {}
                _ => return Err(DiffError::Unsupported("非本地 Windows 卷")),
            }
        }
    }
    Ok(path.to_owned())
}

pub(super) fn native_file_path(bytes: &[u8]) -> Result<PathBuf, DiffError> {
    if bytes.contains(&0)
        || bytes.contains(&b'\\')
        || bytes.contains(&b':')
        || bytes
            .split(|byte| *byte == b'/')
            .any(|part| matches!(part, b"" | b"." | b"..") || part.eq_ignore_ascii_case(b".git"))
    {
        return Err(DiffError::InvalidPath);
    }
    #[cfg(windows)]
    {
        let text = std::str::from_utf8(bytes).map_err(|_| DiffError::InvalidPath)?;
        if text
            .chars()
            .any(|value| value.is_control() || "<>\"|?*".contains(value))
        {
            return Err(DiffError::InvalidPath);
        }
        for path in text.split('/') {
            let stem = path
                .split('.')
                .next()
                .unwrap_or_default()
                .trim_end()
                .to_ascii_uppercase();
            if path.ends_with(['.', ' '])
                || matches!(
                    stem.as_str(),
                    "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
                )
                || stem
                    .strip_prefix("COM")
                    .or_else(|| stem.strip_prefix("LPT"))
                    .is_some_and(|suffix| {
                        matches!(
                            suffix,
                            "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                        )
                    })
            {
                return Err(DiffError::InvalidPath);
            }
        }
    }
    Ok(native_path(bytes)?)
}

pub(super) fn authorize(
    root: &Path,
    selected: &BTreeMap<&[u8], PathBuf>,
    index: &Tracked,
    tree: &Tracked,
) -> Result<Vec<Option<Stamp>>, DiffError> {
    let mut stamps = Vec::new();
    stamps.push(attribute_stamp(root)?);
    for (&bytes, native) in selected {
        if !index.contains_key(bytes) && !tree.contains_key(bytes) {
            return Err(DiffError::UntrackedPath);
        }
        let prefix = [bytes, b"/"].concat();
        for tracked in [index, tree] {
            if tracked.get(bytes).is_some_and(Option::is_none) {
                return Err(DiffError::Unsupported("Git 符号链接条目"));
            }
            if tracked
                .range(prefix.clone()..)
                .next()
                .is_some_and(|(name, _)| name.starts_with(&prefix))
                || bytes
                    .iter()
                    .enumerate()
                    .any(|(offset, byte)| *byte == b'/' && tracked.contains_key(&bytes[..offset]))
            {
                return Err(DiffError::Unsupported("端点间文件与目录类型变化"));
            }
        }
        let mut components = native.components().peekable();
        let mut current = root.to_owned();
        while let Some(component) = components.next() {
            current.push(component);
            let leaf = components.peek().is_none();
            let Some(entry) = metadata(&current)? else {
                stamps.push(None);
                break;
            };
            if !(if leaf {
                entry.is_file()
            } else {
                entry.is_dir()
            }) {
                return Err(DiffError::Unsupported("工作树路径类型变化"));
            }
            stamps.push(Some(Stamp::read(&entry)?));
            if !leaf {
                if metadata(&current.join(".git"))?.is_some() {
                    return Err(DiffError::Unsupported("内嵌 Git 仓库"));
                }
                stamps.push(attribute_stamp(&current)?);
            }
        }
    }
    Ok(stamps)
}

fn attribute_stamp(directory: &Path) -> Result<Option<Stamp>, DiffError> {
    let Some(entry) = metadata(&directory.join(".gitattributes"))? else {
        return Ok(None);
    };
    if !entry.is_file() {
        return Err(DiffError::Unsupported("工作树 attributes 文件类型"));
    }
    if entry.len() > SMALL_METADATA_LIMIT {
        return Err(DiffError::Limit("attributes 文件字节数"));
    }
    Ok(Some(Stamp::read(&entry)?))
}
