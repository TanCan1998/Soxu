//! 从宿主私有暂存向受控目标发布：逐段句柄锚定、独占创建、绝不覆盖。
//!
//! 发布只做两件事：把暂存仓库的 `.git` 目录树与（可选）工作树文件复制到目标。
//! 每一级目录都先在上一级句柄的锚定路径下独占创建，然后立刻打开该级句柄并并入句柄链，
//! 因此后续写入永远发生在已核对身份的那一级目录内，而不是"单一根句柄 + 多段字符串拼接"。
//!
//! 能力边界：复制使用独占创建 + 按实际流量限长写入 + `sync_all`，不依赖硬链接或同卷语义，
//! 因此同卷与跨卷走同一条代码路径；跨卷是否可用仍取决于目标文件系统的实际行为，
//! 本轮未在跨卷布局上实测，不声称已验证。

use std::ffi::OsStr;
use std::fs::{File, OpenOptions};
use std::io::{self, Read};
use std::path::Path;

use super::RepositoryCreateError;
use super::platform::{self, AuthorizedDirectory};

/// 单次发布允许创建的条目总数（目录与文件合计）。
const MAX_TOTAL_ENTRIES: usize = 200_000;
/// 单个发布文件体积上限。
const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
/// 单次发布总体积上限。
const MAX_TOTAL_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// 一次发布的实际写入量。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct Published {
    pub entries: usize,
    pub files: usize,
    pub bytes: u64,
    /// 其中属于工作树（非 `.git`）的文件数。
    pub worktree_files: usize,
}

/// 把暂存仓库发布到目标目录：`with_worktree` 为真时同时发布工作树文件。
pub(super) fn repository(
    staged: &Path,
    target: &AuthorizedDirectory,
    with_worktree: bool,
) -> Result<Published, RepositoryCreateError> {
    require_empty(target)?;
    let mut budget = Budget::default();

    let staged_git = staged.join(".git");
    let metadata = staged_git
        .symlink_metadata()
        .map_err(RepositoryCreateError::Io)?;
    if !platform::plain_directory(&metadata) {
        return Err(RepositoryCreateError::UnsafeContent(
            "暂存仓库缺少受控 .git 目录",
        ));
    }
    let git_root = target.create_child(OsStr::new(".git"))?;
    budget.count_entry()?;
    copy_tree(&staged_git, &git_root, false, false, &mut budget)?;

    if with_worktree {
        copy_tree(staged, target, true, true, &mut budget)?;
    }

    target.refresh()?;
    Ok(budget.published)
}

fn require_empty(target: &AuthorizedDirectory) -> Result<(), RepositoryCreateError> {
    let anchor = target.anchor()?;
    let mut entries = std::fs::read_dir(&anchor).map_err(RepositoryCreateError::Io)?;
    if entries
        .next()
        .transpose()
        .map_err(RepositoryCreateError::Io)?
        .is_some()
    {
        return Err(RepositoryCreateError::UnsafeContent(
            "发布目标必须为空目录，不覆盖任何既有内容",
        ));
    }
    Ok(())
}

fn copy_tree(
    source: &Path,
    destination: &AuthorizedDirectory,
    skip_git: bool,
    worktree: bool,
    budget: &mut Budget,
) -> Result<(), RepositoryCreateError> {
    let mut reader = std::fs::read_dir(source).map_err(RepositoryCreateError::Io)?;
    while let Some(entry) = reader
        .next()
        .transpose()
        .map_err(RepositoryCreateError::Io)?
    {
        let name = entry.file_name();
        if skip_git && name == OsStr::new(".git") {
            continue;
        }
        let path = entry.path();
        let metadata = path.symlink_metadata().map_err(RepositoryCreateError::Io)?;
        if metadata.is_symlink() || platform::is_reparse(&metadata) {
            return Err(RepositoryCreateError::UnsafeContent(
                "暂存产物含链接或 reparse point",
            ));
        }
        if metadata.is_dir() {
            budget.count_entry()?;
            let child = destination.create_child(&name)?;
            copy_tree(&path, &child, false, worktree, budget)?;
        } else if metadata.is_file() {
            budget.copy_file(&path, destination, &name, worktree)?;
        } else {
            return Err(RepositoryCreateError::UnsafeContent("暂存产物含特殊文件"));
        }
    }
    Ok(())
}

#[derive(Debug, Default)]
struct Budget {
    published: Published,
}

impl Budget {
    fn count_entry(&mut self) -> Result<(), RepositoryCreateError> {
        if self.published.entries >= MAX_TOTAL_ENTRIES {
            return Err(RepositoryCreateError::UnsafeContent("发布条目超过上限"));
        }
        self.published.entries += 1;
        Ok(())
    }

    fn copy_file(
        &mut self,
        source: &Path,
        parent: &AuthorizedDirectory,
        name: &OsStr,
        worktree: bool,
    ) -> Result<(), RepositoryCreateError> {
        let mut input = File::open(source).map_err(RepositoryCreateError::Io)?;
        let metadata = input.metadata().map_err(RepositoryCreateError::Io)?;
        if !metadata.is_file() || platform::is_reparse(&metadata) {
            return Err(RepositoryCreateError::UnsafeContent("暂存条目不是普通文件"));
        }
        self.count_entry()?;
        let remaining = MAX_TOTAL_BYTES.saturating_sub(self.published.bytes);
        let limit = MAX_FILE_BYTES.min(remaining);
        let anchor = parent.anchor()?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(anchor.join(name))
            .map_err(|error| {
                if error.kind() == io::ErrorKind::AlreadyExists {
                    RepositoryCreateError::UnsafeContent("发布目标已存在同名文件")
                } else {
                    RepositoryCreateError::Io(error)
                }
            })?;
        let copied = io::copy(&mut Read::by_ref(&mut input).take(limit), &mut output)
            .map_err(RepositoryCreateError::Io)?;
        // 超限探测只读：额外字节不会被写入目标。
        let mut probe = [0_u8; 1];
        if input.read(&mut probe).map_err(RepositoryCreateError::Io)? > 0 {
            return Err(RepositoryCreateError::UnsafeContent("发布文件超过体积上限"));
        }
        output.sync_all().map_err(RepositoryCreateError::Io)?;
        copy_executable_bit(&input, &output)?;
        self.published.bytes += copied;
        self.published.files += 1;
        if worktree {
            self.published.worktree_files += 1;
        }
        Ok(())
    }
}

/// Unix 上保留可执行位；Windows 没有对应语义，按普通文件发布。
#[cfg(unix)]
fn copy_executable_bit(source: &File, destination: &File) -> Result<(), RepositoryCreateError> {
    use std::os::unix::fs::PermissionsExt;
    let mode = source
        .metadata()
        .map_err(RepositoryCreateError::Io)?
        .permissions()
        .mode();
    if mode & 0o111 == 0 {
        return Ok(());
    }
    let mut permissions = destination
        .metadata()
        .map_err(RepositoryCreateError::Io)?
        .permissions();
    permissions.set_mode(0o755);
    destination
        .set_permissions(permissions)
        .map_err(RepositoryCreateError::Io)
}

#[cfg(not(unix))]
fn copy_executable_bit(_source: &File, _destination: &File) -> Result<(), RepositoryCreateError> {
    Ok(())
}

#[cfg(all(test, any(target_os = "linux", windows)))]
mod tests {
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    fn fixture(tag: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let root =
            std::env::temp_dir().join(format!("soxu-rc-publish-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir(&root).expect("临时根");
        // 生产路径校验拒绝含链接的祖先，Unix 上夹具根须取自规范化后的真实路径。
        #[cfg(unix)]
        let root = root.canonicalize().expect("规范化临时根");
        let staged = root.join("staged");
        let target = root.join("target");
        std::fs::create_dir_all(staged.join(".git").join("objects")).expect("staged .git");
        std::fs::create_dir_all(staged.join("nested").join("deeper")).expect("staged nested");
        std::fs::create_dir_all(&target).expect("target");
        std::fs::write(staged.join(".git").join("HEAD"), b"ref: refs/heads/main\n").expect("HEAD");
        std::fs::write(staged.join(".git").join("objects").join("blob"), b"payload")
            .expect("object");
        std::fs::write(
            staged.join("nested").join("deeper").join("deep.txt"),
            b"deep\n",
        )
        .expect("deep file");
        let script = staged.join("run.sh");
        std::fs::write(&script, b"#!/bin/sh\necho ok\n").expect("script");
        #[cfg(unix)]
        {
            let mut permissions = std::fs::metadata(&script).expect("mode").permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(&script, permissions).expect("chmod");
        }
        (staged, target)
    }

    #[test]
    fn publishes_nested_tree_and_keeps_executable_bit() {
        let (staged, target) = fixture("nested");
        let directory = AuthorizedDirectory::open(&target).expect("目标句柄");
        let published = repository(&staged, &directory, true).expect("发布");
        assert!(published.entries >= 5, "目录与文件都要计入预算");
        assert_eq!(published.worktree_files, 2, "工作树文件单独计数");
        assert!(target.join(".git").join("HEAD").is_file());
        assert!(target.join(".git").join("objects").join("blob").is_file());
        assert!(
            target
                .join("nested")
                .join("deeper")
                .join("deep.txt")
                .is_file()
        );
        #[cfg(target_os = "linux")]
        {
            let mode = std::fs::metadata(target.join("run.sh"))
                .expect("脚本")
                .permissions()
                .mode();
            assert_eq!(mode & 0o111, 0o111, "必须保留可执行位");
        }
        drop(directory);
        let _ = std::fs::remove_dir_all(target.parent().expect("临时根"));
    }

    #[test]
    fn second_publish_is_rejected_without_overwrite() {
        let (staged, target) = fixture("second");
        let directory = AuthorizedDirectory::open(&target).expect("目标句柄");
        repository(&staged, &directory, true).expect("首次发布");
        let error = repository(&staged, &directory, true).expect_err("重复发布必须拒绝");
        assert!(matches!(error, RepositoryCreateError::UnsafeContent(_)));
        assert!(target.join(".git").join("HEAD").is_file());
        drop(directory);
        let _ = std::fs::remove_dir_all(target.parent().expect("临时根"));
    }

    #[test]
    fn non_empty_target_is_rejected() {
        let (staged, target) = fixture("occupied");
        std::fs::write(target.join("existing"), b"keep").expect("既有文件");
        let directory = AuthorizedDirectory::open(&target).expect("目标句柄");
        let error = repository(&staged, &directory, false).expect_err("非空目标必须拒绝");
        assert!(matches!(error, RepositoryCreateError::UnsafeContent(_)));
        assert_eq!(
            std::fs::read(target.join("existing")).expect("保留内容"),
            b"keep"
        );
        drop(directory);
        let _ = std::fs::remove_dir_all(target.parent().expect("临时根"));
    }
}
#[cfg(all(test, not(any(target_os = "linux", windows))))]
mod unsupported {
    use super::*;

    #[test]
    fn directory_handle_is_refused_without_platform_primitives() {
        let root = std::env::temp_dir().join(format!("soxu-rc-unsupported-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("临时目录");
        let error = AuthorizedDirectory::open(&root).expect_err("缺少平台原语必须拒绝");
        assert!(matches!(
            error,
            RepositoryCreateError::UnsupportedPlatform(_)
        ));
        let _ = std::fs::remove_dir_all(&root);
    }
}
