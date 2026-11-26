//! 受控仓库创建使用的平台目录身份与目录句柄链。
//!
//! 该层只做一件事：证明某个绝对目录在检查期间是"受支持的普通目录"，并给出可比较的目录身份。
//! 它不提供跨重启的稳定 NTFS 文件 ID：Windows 纯 `std` 拿不到卷序列号与文件索引，
//! 因此 Windows 身份仅在"同一进程持有目录句柄链"期间用于检测替换，不宣称与 Linux 等价。

use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

use super::RepositoryCreateError;

/// 目录身份编码长度（32 字节：volume u64 + file_id u64 + birth u128）。
pub(crate) const ENCODED_IDENTITY_LEN: usize = 32;

/// 平台目录身份。字段口径见模块文档：Linux 为 `dev:ino:ctime`，Windows 为 `creation_time + attributes`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DirectoryIdentity {
    volume: u64,
    file_id: u64,
    birth: u128,
}

impl DirectoryIdentity {
    pub(crate) fn to_bytes(self) -> [u8; ENCODED_IDENTITY_LEN] {
        let mut bytes = [0_u8; ENCODED_IDENTITY_LEN];
        bytes[..8].copy_from_slice(&self.volume.to_le_bytes());
        bytes[8..16].copy_from_slice(&self.file_id.to_le_bytes());
        bytes[16..].copy_from_slice(&self.birth.to_le_bytes());
        bytes
    }

    pub(crate) fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != ENCODED_IDENTITY_LEN {
            return None;
        }
        let (volume, rest) = bytes.split_first_chunk::<8>()?;
        let (file_id, birth) = rest.split_first_chunk::<8>()?;
        Some(Self {
            volume: u64::from_le_bytes(*volume),
            file_id: u64::from_le_bytes(*file_id),
            birth: u128::from_le_bytes(*birth.first_chunk::<16>()?),
        })
    }

    pub(crate) fn to_hex(self) -> String {
        let mut text = String::with_capacity(ENCODED_IDENTITY_LEN * 2);
        for byte in self.to_bytes() {
            text.push(nibble(byte >> 4));
            text.push(nibble(byte & 0x0f));
        }
        text
    }

    pub(crate) fn from_hex(text: &str) -> Option<Self> {
        if text.len() != ENCODED_IDENTITY_LEN * 2 {
            return None;
        }
        let raw = text.as_bytes();
        let mut bytes = [0_u8; ENCODED_IDENTITY_LEN];
        for (index, slot) in bytes.iter_mut().enumerate() {
            let high = hex_nibble(raw[index * 2])?;
            let low = hex_nibble(raw[index * 2 + 1])?;
            *slot = (high << 4) | low;
        }
        Self::from_bytes(&bytes)
    }
}

fn nibble(value: u8) -> char {
    match value {
        0..=9 => char::from(b'0' + value),
        _ => char::from(b'a' + (value - 10)),
    }
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

/// 已核对并通过句柄链保持的普通目录。
///
/// 句柄在整个操作期间保持打开：Windows 上句柄不共享删除权限，因此本条链上的各级目录
/// 在操作期间不会被改名或换成链接；Linux 上每级都以 `O_NOFOLLOW | O_DIRECTORY` 打开并绑定 `/proc/self/fd`。
#[derive(Debug)]
pub(crate) struct AuthorizedDirectory {
    path: PathBuf,
    identity: DirectoryIdentity,
    held: Vec<File>,
}

impl AuthorizedDirectory {
    /// 逐段打开并核对绝对目录，失败一律关闭整个操作。
    pub(crate) fn open(path: &Path) -> Result<Self, RepositoryCreateError> {
        if !path.is_absolute() {
            return Err(RepositoryCreateError::Unauthorized("路径必须是绝对路径"));
        }
        let leaf = open_chain(path)?;
        let canonical = path.canonicalize().map_err(RepositoryCreateError::Io)?;
        let canonical_identity = leaf.identity;
        if canonical != leaf.path {
            let other = open_leaf(&canonical)?;
            if other != canonical_identity {
                return Err(RepositoryCreateError::Unauthorized(
                    "原始路径与规范路径的目录身份不一致",
                ));
            }
        }
        Ok(Self {
            path: leaf.path,
            identity: canonical_identity,
            held: leaf.held,
        })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn identity(&self) -> DirectoryIdentity {
        self.identity
    }

    /// 重新核对本目录身份：先核对仍持有的句柄，再逐段重开路径，用于持锁前后与终态复核。
    pub(crate) fn refresh(&self) -> Result<(), RepositoryCreateError> {
        let leaf = self
            .held
            .last()
            .ok_or(RepositoryCreateError::Unauthorized("授权目录缺少目录句柄"))?;
        ensure_plain_directory(leaf)?;
        if native_identity(leaf)? != self.identity {
            return Err(RepositoryCreateError::IdentityChanged);
        }
        let current = open_chain(&self.path)?;
        if current.identity != self.identity {
            return Err(RepositoryCreateError::IdentityChanged);
        }
        Ok(())
    }
}

#[derive(Debug)]
struct OpenedChain {
    path: PathBuf,
    identity: DirectoryIdentity,
    held: Vec<File>,
}

fn open_chain(path: &Path) -> Result<OpenedChain, RepositoryCreateError> {
    let mut ancestors: Vec<&Path> = path.ancestors().collect();
    ancestors.reverse();
    let mut held = Vec::with_capacity(ancestors.len());
    let mut identity = None;
    for ancestor in ancestors {
        let file = open_handle(ancestor)?;
        ensure_plain_directory(&file)?;
        let current = native_identity(&file)?;
        identity = Some(current);
        held.push(file);
    }
    let identity = identity.ok_or(RepositoryCreateError::Unauthorized("路径缺少目录身份"))?;
    Ok(OpenedChain {
        path: path.to_owned(),
        identity,
        held,
    })
}

fn open_leaf(path: &Path) -> Result<DirectoryIdentity, RepositoryCreateError> {
    let file = open_handle(path)?;
    ensure_plain_directory(&file)?;
    native_identity(&file)
}

fn open_handle(path: &Path) -> Result<File, RepositoryCreateError> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(O_DIRECTORY | O_NOFOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT);
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        return Err(RepositoryCreateError::UnsupportedPlatform(
            "本平台没有受控目录句柄实现",
        ));
    }
    #[cfg(any(target_os = "linux", windows))]
    options.open(path).map_err(RepositoryCreateError::Io)
}

/// 拒绝符号链接、junction 与其他 reparse point。
pub(crate) fn is_reparse(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        let _ = metadata;
        false
    }
}

fn ensure_plain_directory(file: &File) -> Result<(), RepositoryCreateError> {
    let metadata = file.metadata().map_err(RepositoryCreateError::Io)?;
    if !metadata.is_dir() || metadata.is_symlink() || is_reparse(&metadata) {
        return Err(RepositoryCreateError::Unauthorized(
            "路径链存在链接、reparse point 或非目录对象",
        ));
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::fd::AsRawFd;
        use std::os::unix::fs::MetadataExt;
        let anchored = PathBuf::from(format!("/proc/self/fd/{}", file.as_raw_fd()));
        let anchored = std::fs::metadata(&anchored).map_err(RepositoryCreateError::Io)?;
        if anchored.dev() != metadata.dev() || anchored.ino() != metadata.ino() {
            return Err(RepositoryCreateError::Unauthorized(
                "句柄与 /proc/self/fd 绑定的目录身份不一致",
            ));
        }
    }
    Ok(())
}

#[cfg(windows)]
fn native_identity(file: &File) -> Result<DirectoryIdentity, RepositoryCreateError> {
    use std::os::windows::fs::MetadataExt;
    let metadata = file.metadata().map_err(RepositoryCreateError::Io)?;
    Ok(DirectoryIdentity {
        volume: 0,
        file_id: u64::from(metadata.file_attributes()),
        birth: u128::from(metadata.creation_time()),
    })
}

#[cfg(target_os = "linux")]
fn native_identity(file: &File) -> Result<DirectoryIdentity, RepositoryCreateError> {
    use std::os::unix::fs::MetadataExt;
    let metadata = file.metadata().map_err(RepositoryCreateError::Io)?;
    // 目录内容变化会推进 ctime，因此身份只能用创建时间；缺失时 fail-closed。
    let created = metadata.created().map_err(|error| {
        if error.kind() == io::ErrorKind::Unsupported {
            RepositoryCreateError::UnsupportedFilesystem("目录缺少原生创建时间")
        } else {
            RepositoryCreateError::Io(error)
        }
    })?;
    let birth = created
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| RepositoryCreateError::UnsupportedFilesystem("目录创建时间异常"))?
        .as_nanos();
    Ok(DirectoryIdentity {
        volume: metadata.dev(),
        file_id: metadata.ino(),
        birth,
    })
}

#[cfg(not(any(target_os = "linux", windows)))]
fn native_identity(_file: &File) -> Result<DirectoryIdentity, RepositoryCreateError> {
    Err(RepositoryCreateError::UnsupportedPlatform(
        "本平台没有受控目录身份实现",
    ))
}

#[cfg(target_os = "linux")]
const O_DIRECTORY: i32 = 0o200_000;
#[cfg(target_os = "linux")]
const O_NOFOLLOW: i32 = 0o400_000;
#[cfg(windows)]
const FILE_SHARE_READ: u32 = 0x0000_0001;
#[cfg(windows)]
const FILE_SHARE_WRITE: u32 = 0x0000_0002;
#[cfg(windows)]
const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
#[cfg(windows)]
const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
#[cfg(windows)]
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;

/// 目录项是否为"普通目录"，供目标枚举复用。
pub(crate) fn plain_directory(metadata: &std::fs::Metadata) -> bool {
    metadata.is_dir() && !metadata.is_symlink() && !is_reparse(metadata)
}

/// 归一化为可直接传给 Git 的原生路径文本，去掉 Windows 的 `\?\` 前缀。
pub(crate) fn native_path(path: &Path) -> Result<PathBuf, RepositoryCreateError> {
    crate::worktree_lifecycle::native_command_path(path).map_err(|error| match error {
        crate::WorktreeLifecycleError::Unauthorized(reason) => {
            RepositoryCreateError::Unauthorized(reason)
        }
        other => RepositoryCreateError::Io(io::Error::other(other.to_string())),
    })
}
