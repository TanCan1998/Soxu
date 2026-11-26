use std::ffi::{OsStr, OsString};
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

#[cfg(target_os = "linux")]
use std::os::fd::AsRawFd;
#[cfg(target_os = "linux")]
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
#[cfg(windows)]
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};

use super::CheckpointError;

#[cfg(target_os = "linux")]
const NO_FOLLOW: i32 = 0o400000;
#[cfg(target_os = "linux")]
const DIRECTORY: i32 = 0o200000;
#[cfg(target_os = "linux")]
const NONBLOCK: i32 = 0o4000;
#[cfg(windows)]
const OPEN_REPARSE_POINT: u32 = 0x0020_0000;
#[cfg(windows)]
const BACKUP_SEMANTICS: u32 = 0x0200_0000;

pub(super) fn platform_observation() -> Result<&'static str, CheckpointError> {
    #[cfg(target_os = "linux")]
    return Ok("linux:procfd/dev/ino/ctime/nlink");
    #[cfg(windows)]
    return Ok("windows:share-delete-denied/reparse-checked;stable-file-id-and-nlink-unavailable");
    #[cfg(not(any(target_os = "linux", windows)))]
    Err(CheckpointError::UnsupportedPlatform)
}

pub(super) struct Directory {
    path: PathBuf,
    chain: Vec<File>,
}

impl Directory {
    pub(super) fn open(path: &Path) -> io::Result<Self> {
        let path = std::path::absolute(path)?;
        let mut current = PathBuf::new();
        let mut chain = Vec::new();
        for component in path.components() {
            match component {
                Component::Prefix(prefix) => {
                    #[cfg(windows)]
                    if !matches!(
                        prefix.kind(),
                        std::path::Prefix::Disk(_) | std::path::Prefix::VerbatimDisk(_)
                    ) {
                        return Err(unsafe_path());
                    }
                    current.push(prefix.as_os_str());
                }
                Component::RootDir => {
                    current.push(component.as_os_str());
                    chain.push(open_directory(&current)?);
                }
                Component::Normal(name) => {
                    let parent = chain.last().ok_or_else(unsafe_path)?;
                    let next = anchored(parent, &current).join(name);
                    chain.push(open_directory(&next)?);
                    current.push(name);
                }
                _ => return Err(unsafe_path()),
            }
        }
        if chain.is_empty() {
            return Err(unsafe_path());
        }
        Ok(Self {
            path: current,
            chain,
        })
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    pub(super) fn anchored(&self) -> PathBuf {
        anchored(&self.chain[self.chain.len() - 1], &self.path)
    }

    pub(super) fn child(&self, name: &OsStr) -> io::Result<Self> {
        let mut chain = self
            .chain
            .iter()
            .map(File::try_clone)
            .collect::<io::Result<Vec<_>>>()?;
        chain.push(open_directory(&self.anchored().join(name))?);
        Ok(Self {
            path: self.path.join(name),
            chain,
        })
    }

    pub(super) fn entry(&self, path: &Path) -> io::Result<Entry> {
        let mut entries = path.components().peekable();
        let mut children = Vec::new();
        let mut parent = self;
        while let Some(Component::Normal(name)) = entries.next() {
            if entries.peek().is_none() {
                return open_entry(&parent.anchored().join(name));
            }
            let child = match parent.child(name) {
                Ok(child) => child,
                Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Entry::Absent),
                Err(error) => return Err(error),
            };
            if metadata_if_present(&child.anchored().join(".git"))?.is_some() {
                return Ok(Entry::Special);
            }
            children.push(child);
            parent = children.last().ok_or_else(unsafe_path)?;
        }
        Err(unsafe_path())
    }

    pub(super) fn verify(&self) -> Result<(), CheckpointError> {
        let current = Self::open(&self.path).map_err(|_| CheckpointError::Drift)?;
        if self.identity()? != current.identity()? {
            return Err(CheckpointError::Drift);
        }
        Ok(())
    }

    pub(super) fn identity(&self) -> io::Result<Vec<String>> {
        self.chain
            .iter()
            .map(|f| f.metadata().map(|m| directory_identity(&m)))
            .collect()
    }

    pub(super) fn sync(&self) -> io::Result<()> {
        #[cfg(target_os = "linux")]
        return self.chain[self.chain.len() - 1].sync_all();
        #[cfg(windows)]
        {
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .share_mode(3)
                .custom_flags(BACKUP_SEMANTICS | OPEN_REPARSE_POINT)
                .open(&self.path)?;
            let metadata = file.metadata()?;
            if !is_plain_directory(&metadata)
                || directory_identity(&metadata)
                    != self.identity()?.pop().ok_or_else(unsafe_path)?
            {
                return Err(unsafe_path());
            }
            file.sync_all()
        }
        #[cfg(not(any(target_os = "linux", windows)))]
        Err(unsafe_path())
    }
}

pub(super) enum Entry {
    Absent,
    Regular(File),
    Special,
}

pub(super) fn open_entry(path: &Path) -> io::Result<Entry> {
    let Some(before) = metadata_if_present(path)? else {
        return Ok(Entry::Absent);
    };
    if !is_regular(&before) {
        return Ok(Entry::Special);
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(target_os = "linux")]
    options.custom_flags(NO_FOLLOW | NONBLOCK);
    #[cfg(windows)]
    options.share_mode(1).custom_flags(OPEN_REPARSE_POINT);
    let file = options.open(path)?;
    let after = file.metadata()?;
    if !is_regular(&after) || file_identity(&before)? != file_identity(&after)? {
        return Err(io::Error::other("打开前后文件身份不一致"));
    }
    Ok(Entry::Regular(file))
}

pub(super) fn read_bounded(
    file: &mut File,
    limit: u64,
) -> Result<Option<(Vec<u8>, String)>, CheckpointError> {
    let before = file.metadata()?;
    if !is_regular(&before) {
        return Err(CheckpointError::Drift);
    }
    if before.len() > limit {
        return Ok(None);
    }
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    let after = file.metadata()?;
    let identity = file_identity(&after)?;
    if !is_regular(&after)
        || file_identity(&before)? != identity
        || after.len() != bytes.len() as u64
    {
        return Err(CheckpointError::Drift);
    }
    if bytes.len() as u64 > limit {
        return Ok(None);
    }
    Ok(Some((bytes, identity)))
}

pub(super) fn metadata_if_present(path: &Path) -> io::Result<Option<Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(meta) => Ok(Some(meta)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

pub(super) fn is_plain_directory(meta: &Metadata) -> bool {
    meta.is_dir() && !redirected(meta)
}

pub(super) fn is_regular(meta: &Metadata) -> bool {
    let regular = meta.is_file() && !redirected(meta);
    #[cfg(target_os = "linux")]
    {
        regular && meta.nlink() == 1
    }
    #[cfg(not(target_os = "linux"))]
    {
        regular
    }
}

fn redirected(meta: &Metadata) -> bool {
    #[cfg(windows)]
    {
        meta.is_symlink() || (meta.file_attributes() & 0x400 != 0)
    }
    #[cfg(not(windows))]
    {
        meta.is_symlink()
    }
}

fn open_directory(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(target_os = "linux")]
    options.custom_flags(NO_FOLLOW | DIRECTORY);
    #[cfg(windows)]
    options
        .share_mode(1 | 2)
        .custom_flags(BACKUP_SEMANTICS | OPEN_REPARSE_POINT);
    let file = options.open(path)?;
    let meta = file.metadata()?;
    if !is_plain_directory(&meta)
        || directory_identity(&meta) != directory_identity(&anchored(&file, path).metadata()?)
    {
        return Err(unsafe_path());
    }
    Ok(file)
}

fn anchored(file: &File, path: &Path) -> PathBuf {
    #[cfg(target_os = "linux")]
    {
        let _ = path;
        PathBuf::from(format!("/proc/self/fd/{}", file.as_raw_fd()))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = file;
        path.to_owned()
    }
}

fn directory_identity(meta: &Metadata) -> String {
    #[cfg(target_os = "linux")]
    return format!("{}:{}", meta.dev(), meta.ino());
    #[cfg(windows)]
    return format!("{}:{}", meta.creation_time(), meta.file_attributes());
    #[cfg(not(any(target_os = "linux", windows)))]
    format!("{:?}", meta.created())
}

fn file_identity(meta: &Metadata) -> io::Result<String> {
    #[cfg(target_os = "linux")]
    return Ok(format!(
        "{}:{}:{}:{}:{}:{}:{}:{}:{}",
        meta.dev(),
        meta.ino(),
        meta.ctime(),
        meta.ctime_nsec(),
        meta.mtime(),
        meta.mtime_nsec(),
        meta.len(),
        meta.mode(),
        meta.nlink()
    ));
    #[cfg(windows)]
    return Ok(format!(
        "{}:{}:{}:{}",
        meta.creation_time(),
        meta.last_write_time(),
        meta.len(),
        meta.file_attributes()
    ));
    #[cfg(not(any(target_os = "linux", windows)))]
    Ok(format!("{:?}:{}", meta.modified()?, meta.len()))
}

pub(super) fn native_component(bytes: &[u8]) -> Option<OsString> {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::ffi::OsStringExt;
        Some(OsString::from_vec(bytes.to_vec()))
    }
    #[cfg(not(target_os = "linux"))]
    std::str::from_utf8(bytes).ok().map(OsString::from)
}

pub(super) fn git_name(name: &OsStr) -> Option<Vec<u8>> {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::ffi::OsStrExt;
        Some(name.as_bytes().to_vec())
    }
    #[cfg(not(target_os = "linux"))]
    name.to_str().map(|s| s.as_bytes().to_vec())
}

pub(super) fn executable(file: &File, inherited: bool) -> io::Result<bool> {
    #[cfg(target_os = "linux")]
    {
        let _ = inherited;
        Ok(file.metadata()?.mode() & 0o100 != 0)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = file;
        Ok(inherited)
    }
}

fn unsafe_path() -> io::Error {
    io::Error::other("无法安全固定路径组件")
}
