//! 受控仓库创建使用的平台目录身份与目录句柄链。
//!
//! 该层只做一件事：证明某个绝对目录在检查期间是"受支持的普通目录"，并给出可比较的目录身份。
//! 它不提供跨重启的稳定 NTFS 文件 ID：Windows 纯 `std` 拿不到卷序列号与文件索引，
//! 因此 Windows 身份仅在"同一进程持有目录句柄链"期间用于检测替换，不宣称与 Linux 等价。

use std::ffi::OsStr;
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

    /// 从叶子目录句柄取元数据：属主与权限必须由固定句柄证明，不能只看路径。
    #[cfg(target_os = "linux")]
    pub(crate) fn leaf_metadata(&self) -> Result<std::fs::Metadata, RepositoryCreateError> {
        self.held
            .last()
            .ok_or(RepositoryCreateError::Unauthorized("授权目录缺少目录句柄"))?
            .metadata()
            .map_err(RepositoryCreateError::Io)
    }

    /// 写入用的句柄锚定路径。
    ///
    /// Linux 返回 `/proc/self/fd/<leaf>`：宿主自身的一切 fs 操作都绑定到已核对句柄指向的目录，
    /// 即使原路径被改名或替换也不会落到其他目录。该路径只能由宿主进程使用；
    /// 需要子进程访问时必须改用 `/proc/<host_pid>/fd/<leaf>`，不能用 `/proc/self`。
    /// Windows 返回原路径：句柄以 `share_mode(READ|WRITE)` 持有且不共享删除，
    /// 持有期内该路径不能被改名或换成链接。
    pub(crate) fn anchor(&self) -> Result<PathBuf, RepositoryCreateError> {
        #[cfg(target_os = "linux")]
        {
            use std::os::fd::AsRawFd;
            let leaf = self
                .held
                .last()
                .ok_or(RepositoryCreateError::Unauthorized("授权目录缺少目录句柄"))?;
            Ok(PathBuf::from(format!("/proc/self/fd/{}", leaf.as_raw_fd())))
        }
        #[cfg(windows)]
        {
            if self.held.is_empty() {
                return Err(RepositoryCreateError::Unauthorized("授权目录缺少目录句柄"));
            }
            Ok(self.path.clone())
        }
        #[cfg(not(any(target_os = "linux", windows)))]
        {
            Err(RepositoryCreateError::UnsupportedPlatform(
                "本平台没有受控目录句柄锚定实现",
            ))
        }
    }

    /// 逐段打开已存在子目录，并把新一级句柄并入持有的句柄链。
    pub(crate) fn child(&self, name: &OsStr) -> Result<Self, RepositoryCreateError> {
        let anchor = self.anchor()?;
        let file = open_handle(&anchor.join(name))?;
        ensure_plain_directory(&file)?;
        let identity = native_identity(&file)?;
        let mut held = Vec::with_capacity(self.held.len() + 1);
        for parent in &self.held {
            held.push(parent.try_clone().map_err(RepositoryCreateError::Io)?);
        }
        held.push(file.try_clone().map_err(RepositoryCreateError::Io)?);
        Ok(Self {
            path: self.path.join(name),
            identity,
            held,
        })
    }

    /// 在句柄锚定路径下独占创建子目录并立即转为受控句柄；已存在即拒绝，绝不覆盖。
    pub(crate) fn create_child(&self, name: &OsStr) -> Result<Self, RepositoryCreateError> {
        let anchor = self.anchor()?;
        match std::fs::create_dir(anchor.join(name)) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                return Err(RepositoryCreateError::TargetExists);
            }
            Err(error) => return Err(RepositoryCreateError::Io(error)),
        }
        self.child(name)
    }

    /// 供 Git 子进程访问的路径：Linux 用宿主 PID 的 `/proc/<pid>/fd/<leaf>`
    /// （子进程的 `proc/self` 指向自己，不能用）；Windows 返回真实路径，该平台写入入口已 fail-closed。
    pub(crate) fn child_process_path(&self) -> Result<PathBuf, RepositoryCreateError> {
        #[cfg(target_os = "linux")]
        {
            use std::os::fd::AsRawFd;
            let leaf = self
                .held
                .last()
                .ok_or(RepositoryCreateError::Unauthorized("授权目录缺少目录句柄"))?;
            Ok(PathBuf::from(format!(
                "/proc/{}/fd/{}",
                std::process::id(),
                leaf.as_raw_fd()
            )))
        }
        #[cfg(not(target_os = "linux"))]
        {
            Ok(self.path.clone())
        }
    }

    /// 克隆同一句柄链，供同一写入事务内多处持有。
    pub(crate) fn try_clone(&self) -> Result<Self, RepositoryCreateError> {
        let mut held = Vec::with_capacity(self.held.len());
        for file in &self.held {
            held.push(file.try_clone().map_err(RepositoryCreateError::Io)?);
        }
        Ok(Self {
            path: self.path.clone(),
            identity: self.identity,
            held,
        })
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

/// 逐级打开并核对绝对目录。
///
/// Linux：从 `/` 句柄开始，每一级都用上一级 fd 的 `/proc/self/fd/<fd>/<name>` 打开（`openat` 语义），
/// 不重开任何完整绝对路径。链上每一级都持有 fd，因此已核对的目录在本事务内不会被改名或替换。
/// Windows：逐段用绝对路径打开，但整条链以 `share_mode(READ|WRITE)`（不含 DELETE）持有，
/// Windows 共享检查是双向的，持有期间其他进程无法取得这些目录的删除权限。
#[cfg(target_os = "linux")]
fn open_chain(path: &Path) -> Result<OpenedChain, RepositoryCreateError> {
    use std::os::fd::AsRawFd;

    if !path.is_absolute() {
        return Err(RepositoryCreateError::Unauthorized("路径必须是绝对路径"));
    }
    let mut current = PathBuf::from("/");
    let root = open_handle(&current)?;
    ensure_plain_directory(&root)?;
    let mut held = vec![root];
    for component in path.components() {
        let name = match component {
            std::path::Component::RootDir | std::path::Component::CurDir => continue,
            std::path::Component::Normal(name) => name,
            _ => {
                return Err(RepositoryCreateError::Unauthorized(
                    "路径必须是规范的绝对路径",
                ));
            }
        };
        let parent = held
            .last()
            .ok_or(RepositoryCreateError::Unauthorized("路径缺少目录句柄"))?;
        let anchored = PathBuf::from(format!("/proc/self/fd/{}", parent.as_raw_fd())).join(name);
        let file = open_handle(&anchored)?;
        ensure_plain_directory(&file)?;
        current.push(name);
        held.push(file);
    }
    let identity = native_identity(
        held.last()
            .ok_or(RepositoryCreateError::Unauthorized("路径缺少目录身份"))?,
    )?;
    Ok(OpenedChain {
        path: current,
        identity,
        held,
    })
}

#[cfg(not(target_os = "linux"))]
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
/// 明确进程凭据事实：从 `/proc/self/status` 的 `Uid:` 行取有效 UID。
/// 目录属主不能代替该事实，`/proc/self` 的属主也不等同有效 UID。
#[cfg(target_os = "linux")]
fn effective_uid() -> Result<u32, RepositoryCreateError> {
    let text = std::fs::read_to_string("/proc/self/status").map_err(RepositoryCreateError::Io)?;
    for line in text.lines() {
        let Some(rest) = line.strip_prefix("Uid:") else {
            continue;
        };
        let mut fields = rest.split_whitespace();
        let _real = fields.next();
        let effective = fields
            .next()
            .ok_or(RepositoryCreateError::Unauthorized("无法解析有效 UID"))?;
        return effective
            .parse()
            .map_err(|_| RepositoryCreateError::Unauthorized("无法解析有效 UID"));
    }
    Err(RepositoryCreateError::Unauthorized("无法读取进程凭据"))
}

/// 绑定叶子目录句柄的挂载 ID，避免用路径重新推断挂载归属。
#[cfg(target_os = "linux")]
pub(crate) fn mount_id(root: &AuthorizedDirectory) -> Result<u64, RepositoryCreateError> {
    use std::os::fd::AsRawFd;
    let leaf = root
        .held
        .last()
        .ok_or(RepositoryCreateError::Unauthorized("授权目录缺少目录句柄"))?;
    let text = std::fs::read_to_string(format!("/proc/self/fdinfo/{}", leaf.as_raw_fd()))
        .map_err(RepositoryCreateError::Io)?;
    for line in text.lines() {
        let Some(value) = line.strip_prefix("mnt_id:") else {
            continue;
        };
        return value
            .trim()
            .parse()
            .map_err(|_| RepositoryCreateError::Unauthorized("无法解析目录句柄的挂载 ID"));
    }
    Err(RepositoryCreateError::Unauthorized(
        "无法从目录句柄取得挂载 ID",
    ))
}

/// 只接受可证明语义的本地文件系统；未知类型一律拒绝，而不是排除常见网络类型后放行其余。
#[cfg(target_os = "linux")]
pub(crate) fn require_supported_filesystem(
    root: &AuthorizedDirectory,
) -> Result<(), RepositoryCreateError> {
    const SUPPORTED: [&str; 11] = [
        "ext2", "ext3", "ext4", "xfs", "btrfs", "f2fs", "tmpfs", "ramfs", "zfs", "overlay", "drvfs",
    ];
    let target = mount_id(root)?;
    let text =
        std::fs::read_to_string("/proc/self/mountinfo").map_err(RepositoryCreateError::Io)?;
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let Some(id) = fields.next() else {
            continue;
        };
        if id.parse::<u64>().ok() != Some(target) {
            continue;
        }
        let Some(fstype) = line
            .split_whitespace()
            .skip_while(|item| *item != "-")
            .nth(1)
        else {
            continue;
        };
        if SUPPORTED.contains(&fstype) {
            return Ok(());
        }
        return Err(RepositoryCreateError::Unauthorized(
            "目录位于不受支持的文件系统",
        ));
    }
    Err(RepositoryCreateError::Unauthorized(
        "无法确认目录句柄所在的挂载",
    ))
}

/// 以固定目录句柄证明属主、权限与受支持文件系统。
#[cfg(target_os = "linux")]
pub(crate) fn require_private_owner(
    root: &AuthorizedDirectory,
) -> Result<(), RepositoryCreateError> {
    use std::os::unix::fs::MetadataExt;
    let metadata = root.leaf_metadata()?;
    if metadata.uid() != effective_uid()? {
        return Err(RepositoryCreateError::Unauthorized(
            "目录属主不是当前有效用户",
        ));
    }
    if metadata.mode() & 0o077 != 0 {
        return Err(RepositoryCreateError::Unauthorized(
            "目录必须是宿主私有目录",
        ));
    }
    require_supported_filesystem(root)
}

/// Windows 上该证据由调用方进程隔离与 DACL 提供；本入口不据此授予写能力。
#[cfg(not(target_os = "linux"))]
pub(crate) fn require_private_owner(
    _root: &AuthorizedDirectory,
) -> Result<(), RepositoryCreateError> {
    Ok(())
}
/// 在已持父目录句柄的锚定路径下受限只读打开普通文件（Linux `O_NOFOLLOW`）。
/// 该入口只用于源仓库的受限读取；其他平台没有等价原语，直接拒绝。
#[cfg(target_os = "linux")]
pub(crate) fn open_regular_anchored(
    parent: &AuthorizedDirectory,
    name: &OsStr,
) -> Result<File, RepositoryCreateError> {
    use std::os::unix::fs::OpenOptionsExt;
    let path = parent.anchor()?.join(name);
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(O_NOFOLLOW)
        .open(&path)
        .map_err(RepositoryCreateError::Io)?;
    let metadata = file.metadata().map_err(RepositoryCreateError::Io)?;
    if !metadata.is_file() || is_reparse(&metadata) {
        return Err(RepositoryCreateError::Unauthorized("锚定条目不是普通文件"));
    }
    Ok(file)
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn open_regular_anchored(
    _parent: &AuthorizedDirectory,
    _name: &OsStr,
) -> Result<File, RepositoryCreateError> {
    Err(RepositoryCreateError::UnsupportedPlatform(
        "本平台没有受限只读打开原语",
    ))
}
/// Windows 写入入口已 fail-closed，本入口不额外断言；其他平台按同一契约拒绝。
#[cfg(not(target_os = "linux"))]
pub(crate) fn require_supported_filesystem(
    _root: &AuthorizedDirectory,
) -> Result<(), RepositoryCreateError> {
    Ok(())
}
