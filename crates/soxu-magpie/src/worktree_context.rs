use crate::{EnvironmentMode, ProbeStatus, magpie};
use serde::Serialize;
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// 交由宿主后续消费的工作区级上下文。文件不包含启动命令或凭据。
pub const WORKTREE_CONTEXT_FILE: &str = ".soxu-worktree-mcp.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorktreeContextServer {
    Aoci,
    CodeGraph,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorktreeContext {
    StandaloneNative,
    NoDirectServers,
    Prepared {
        manifest: PathBuf,
        servers: Vec<WorktreeContextServer>,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum WorktreeContextError {
    #[error("无法确认工作区目录身份")]
    UnsafeDirectory,
    #[error("工作区上下文与现有文件冲突")]
    Conflict,
    #[error("无法读取 Magpie MCP 路由声明（{0:?}）")]
    MagpieConfig(ProbeStatus),
    #[error("工作区上下文文件操作失败（{0:?}）")]
    Io(io::ErrorKind),
}

impl From<io::Error> for WorktreeContextError {
    fn from(error: io::Error) -> Self {
        Self::Io(error.kind())
    }
}

/// 保留从卷根至授权根的目录句柄，拒绝链接、重解析点和目录替换。
#[derive(Debug)]
pub struct WorktreeDirectory {
    path: PathBuf,
    directories: Vec<(PathBuf, File)>,
}

impl WorktreeDirectory {
    pub fn open(path: &Path) -> Result<Self, WorktreeContextError> {
        let mut current = PathBuf::new();
        let mut directories: Vec<(PathBuf, File)> = Vec::new();
        if !path.is_absolute() {
            return Err(WorktreeContextError::UnsafeDirectory);
        }
        for component in path.components() {
            match component {
                Component::Prefix(_) => {
                    #[cfg(windows)]
                    if let Component::Prefix(prefix) = component
                        && !matches!(
                            prefix.kind(),
                            std::path::Prefix::Disk(_) | std::path::Prefix::VerbatimDisk(_)
                        )
                    {
                        return Err(WorktreeContextError::UnsafeDirectory);
                    }
                    current.push(component);
                    continue;
                }
                Component::RootDir | Component::Normal(_) => current.push(component),
                _ => return Err(WorktreeContextError::UnsafeDirectory),
            }
            let parent = directories.last();
            let open_path = match (parent, component) {
                (Some((parent, handle)), Component::Normal(name)) => {
                    directory_anchor(handle, parent, Some(name))
                }
                _ => current.clone(),
            };
            let before = fs::symlink_metadata(&open_path)?;
            if !is_plain_directory(&before) {
                return Err(WorktreeContextError::UnsafeDirectory);
            }
            let handle = open_directory(&open_path)?;
            if !matches_directory(&handle.metadata()?, &before) {
                return Err(WorktreeContextError::UnsafeDirectory);
            }
            directories.push((current.clone(), handle));
        }
        if directories.len() < 2 {
            return Err(WorktreeContextError::UnsafeDirectory);
        }
        let directory = Self {
            path: current,
            directories,
        };
        directory.recheck()?;
        Ok(directory)
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn recheck(&self) -> Result<(), WorktreeContextError> {
        for (path, handle) in &self.directories {
            if !matches_directory(&handle.metadata()?, &fs::symlink_metadata(path)?) {
                return Err(WorktreeContextError::UnsafeDirectory);
            }
        }
        Ok(())
    }

    /// Linux 通过打开的目录访问子项。Windows 句柄的共享模式排除目录替换。
    #[must_use]
    pub fn anchored_path(&self) -> PathBuf {
        let (path, handle) = self.directories.last().expect("已验证目录链");
        directory_anchor(handle, path, None)
    }
}

fn is_plain_directory(metadata: &Metadata) -> bool {
    metadata.is_dir() && !is_link(metadata)
}

fn is_link(metadata: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    metadata.file_type().is_symlink()
}

fn matches_directory(open: &Metadata, named: &Metadata) -> bool {
    if !is_plain_directory(open) || !is_plain_directory(named) {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        open.dev() == named.dev() && open.ino() == named.ino()
    }
    #[cfg(not(unix))]
    true // Windows 的目录句柄排除了重命名及删除。
}

fn open_directory(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_SHARE_READ: u32 = 1;
        const FILE_SHARE_WRITE: u32 = 2;
        const BACKUP_SEMANTICS: u32 = 0x0200_0000;
        const OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        options
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(BACKUP_SEMANTICS | OPEN_REPARSE_POINT);
    }
    options.open(path)
}

fn directory_anchor(handle: &File, path: &Path, child: Option<&std::ffi::OsStr>) -> PathBuf {
    #[cfg(target_os = "linux")]
    let _ = path;
    #[cfg(target_os = "linux")]
    let mut anchor = {
        use std::os::fd::AsRawFd;
        PathBuf::from(format!("/proc/self/fd/{}", handle.as_raw_fd()))
    };
    #[cfg(not(target_os = "linux"))]
    let _ = handle;
    #[cfg(not(target_os = "linux"))]
    let mut anchor = path.to_path_buf();
    if let Some(child) = child {
        anchor.push(child);
    }
    anchor
}

#[derive(Serialize)]
struct ContextManifest<'a> {
    version: u8,
    worktree_root: &'static str,
    servers: &'a [WorktreeContextServer],
}

/// 仅准备授权根下的 direct 上下文声明。此入口由宿主在检出重验后调用。
pub fn prepare_worktree_context(
    root: &WorktreeDirectory,
    mode: EnvironmentMode,
    magpie_config_dir: Option<&Path>,
) -> Result<WorktreeContext, WorktreeContextError> {
    root.recheck()?;
    if mode == EnvironmentMode::StandaloneNative {
        return Ok(WorktreeContext::StandaloneNative);
    }
    let servers = magpie::direct_worktree_servers(magpie_config_dir)
        .map_err(WorktreeContextError::MagpieConfig)?;
    if servers.is_empty() {
        return Ok(WorktreeContext::NoDirectServers);
    }
    if !cfg!(any(windows, target_os = "linux")) {
        return Err(WorktreeContextError::UnsafeDirectory);
    }
    let mut bytes = serde_json::to_vec_pretty(&ContextManifest {
        version: 1,
        worktree_root: ".",
        servers: &servers,
    })
    .expect("上下文清单仅含固定的可序列化字段");
    bytes.push(b'\n');
    #[cfg(windows)]
    let _git_file = lock_windows_git_file(root)?;
    publish_manifest(root, &bytes)?;
    Ok(WorktreeContext::Prepared {
        manifest: root.path().join(WORKTREE_CONTEXT_FILE),
        servers,
    })
}

#[cfg(windows)]
fn lock_windows_git_file(root: &WorktreeDirectory) -> Result<File, WorktreeContextError> {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_SHARE_READ: u32 = 1;
    const OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    // 祖先由下一层打开的目录保持非空，目标则由受保护的 .git 文件保持非空。
    // 在允许对子项进行写入时，这仍阻止将 NTFS 目录变更为联接或符号链接。
    let file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(OPEN_REPARSE_POINT)
        .open(root.path().join(".git"))?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || is_link(&metadata) {
        return Err(WorktreeContextError::UnsafeDirectory);
    }
    root.recheck()?;
    Ok(file)
}

fn publish_manifest(root: &WorktreeDirectory, bytes: &[u8]) -> Result<(), WorktreeContextError> {
    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);
    let anchor = root.anchored_path();
    let destination = anchor.join(WORKTREE_CONTEXT_FILE);
    root.recheck()?;
    if manifest_matches(&destination, bytes)? {
        return root.recheck();
    }
    let candidate = anchor.join(format!(
        "{WORKTREE_CONTEXT_FILE}.{}.{}",
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&candidate)?;
    let published = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        root.recheck()?;
        match fs::hard_link(&candidate, &destination) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
        if !manifest_matches(&destination, bytes)? {
            return Err(WorktreeContextError::Conflict);
        }
        root.recheck()
    })();
    let cleanup = fs::remove_file(candidate);
    published?;
    cleanup?;
    Ok(())
}

fn manifest_matches(path: &Path, bytes: &[u8]) -> Result<bool, WorktreeContextError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    if is_link(&metadata) || !metadata.is_file() || metadata.len() != bytes.len() as u64 {
        return Err(WorktreeContextError::Conflict);
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        options.custom_flags(OPEN_REPARSE_POINT);
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        const O_NONBLOCK: i32 = 0o4000;
        options.custom_flags(O_NONBLOCK);
    }
    let file = options.open(path)?;
    let open = file.metadata()?;
    if !open.is_file() || is_link(&open) || is_link(&fs::symlink_metadata(path)?) {
        return Err(WorktreeContextError::Conflict);
    }
    #[cfg(unix)]
    if {
        use std::os::unix::fs::MetadataExt;
        open.dev() != metadata.dev() || open.ino() != metadata.ino()
    } {
        return Err(WorktreeContextError::Conflict);
    }
    let mut existing = Vec::new();
    file.take(bytes.len() as u64 + 1)
        .read_to_end(&mut existing)?;
    if existing != bytes {
        return Err(WorktreeContextError::Conflict);
    }
    Ok(true)
}
