use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DirectoryIdentity {
    volume: u64,
    file_id: u64,
    created_nanos: u128,
}

impl DirectoryIdentity {
    pub(crate) const ENCODED_LEN: usize = 32;

    pub(crate) fn to_bytes(self) -> [u8; Self::ENCODED_LEN] {
        let mut bytes = [0; Self::ENCODED_LEN];
        bytes[..8].copy_from_slice(&self.volume.to_le_bytes());
        bytes[8..16].copy_from_slice(&self.file_id.to_le_bytes());
        bytes[16..].copy_from_slice(&self.created_nanos.to_le_bytes());
        bytes
    }

    pub(crate) fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let (volume, rest) = bytes.split_first_chunk::<8>()?;
        let (file_id, created) = rest.split_first_chunk::<8>()?;
        Some(Self {
            volume: u64::from_le_bytes(*volume),
            file_id: u64::from_le_bytes(*file_id),
            created_nanos: u128::from_le_bytes(created.try_into().ok()?),
        })
    }
}

#[derive(Debug, Clone)]
pub(crate) struct AuthorizedDirectory {
    path: PathBuf,
    identity: DirectoryIdentity,
}

impl AuthorizedDirectory {
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn identity(&self) -> DirectoryIdentity {
        self.identity
    }

    pub(crate) fn refresh(&self) -> Result<(), super::WorktreeLifecycleError> {
        let current = checked_directory(&self.path)?;
        if current.identity != self.identity {
            return Err(super::WorktreeLifecycleError::IdentityChanged);
        }
        Ok(())
    }
}

pub(crate) fn checked_directory(
    path: &Path,
) -> Result<AuthorizedDirectory, super::WorktreeLifecycleError> {
    if !path.is_absolute() {
        return Err(super::WorktreeLifecycleError::Unauthorized(
            "路径必须是绝对路径",
        ));
    }
    for ancestor in path.ancestors() {
        let metadata = ancestor
            .symlink_metadata()
            .map_err(super::WorktreeLifecycleError::Io)?;
        if !metadata.is_dir() || metadata.is_symlink() || is_reparse(&metadata) {
            return Err(super::WorktreeLifecycleError::Unauthorized(
                "路径链存在链接、reparse point 或非目录对象",
            ));
        }
    }

    let canonical = path
        .canonicalize()
        .map_err(super::WorktreeLifecycleError::Io)?;
    ensure_no_reparse(&canonical)?;
    let path_identity = directory_identity(path)?;
    let canonical_identity = directory_identity(&canonical)?;
    if path_identity != canonical_identity {
        return Err(super::WorktreeLifecycleError::Unauthorized(
            "原始路径与规范路径的目录身份不一致",
        ));
    }

    Ok(AuthorizedDirectory {
        path: canonical,
        identity: canonical_identity,
    })
}

pub(crate) fn is_reparse(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        let _ = metadata;
        false
    }
}

fn ensure_no_reparse(path: &Path) -> Result<(), super::WorktreeLifecycleError> {
    let metadata = path
        .symlink_metadata()
        .map_err(super::WorktreeLifecycleError::Io)?;
    if !metadata.is_dir() || metadata.is_symlink() || is_reparse(&metadata) {
        return Err(super::WorktreeLifecycleError::Unauthorized(
            "规范路径不是受支持的普通目录",
        ));
    }
    Ok(())
}

fn directory_identity(path: &Path) -> Result<DirectoryIdentity, super::WorktreeLifecycleError> {
    let file = open_directory(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_dir() || metadata.is_symlink() || is_reparse(&metadata) {
        return Err(super::WorktreeLifecycleError::Unauthorized(
            "对象身份检查失败：不是受支持的目录",
        ));
    }
    native_identity(&file, &metadata)
}

fn open_directory(path: &Path) -> Result<File, io::Error> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        OpenOptions::new()
            .read(true)
            .custom_flags(0x0200_0000 | 0x0020_0000)
            .open(path)
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::OpenOptionsExt;
        OpenOptions::new()
            .read(true)
            .custom_flags(0o400_000)
            .open(path)
    }
}

#[cfg(windows)]
fn native_identity(
    _file: &File,
    metadata: &std::fs::Metadata,
) -> Result<DirectoryIdentity, super::WorktreeLifecycleError> {
    let _ = (_file, metadata);
    Err(super::WorktreeLifecycleError::UnsupportedFilesystem(
        "Windows 稳定 std 不提供目录身份",
    ))
}

#[cfg(not(windows))]
fn native_identity(
    _file: &File,
    metadata: &std::fs::Metadata,
) -> Result<DirectoryIdentity, super::WorktreeLifecycleError> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    use std::time::SystemTime;

    let permissions = metadata.permissions();
    let mode = permissions.mode();
    if mode & 0o022 != 0 || mode & 0o300 != 0o300 || mode & 0o100 == 0 {
        return Err(super::WorktreeLifecycleError::Unauthorized(
            "授权根权限过宽或不可用",
        ));
    }
    let created = metadata.created().map_err(|error| {
        if error.kind() == io::ErrorKind::Unsupported {
            super::WorktreeLifecycleError::UnsupportedFilesystem("目录缺少原生创建时间")
        } else {
            error.into()
        }
    })?;
    let created_nanos = created
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_err(|_| super::WorktreeLifecycleError::UnsupportedFilesystem("目录创建时间异常"))?
        .as_nanos();
    Ok(DirectoryIdentity {
        volume: metadata.dev(),
        file_id: metadata.ino(),
        created_nanos,
    })
}
