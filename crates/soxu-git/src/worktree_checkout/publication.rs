use std::collections::BTreeMap;
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{self, Read};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use tokio::sync::oneshot;

use super::journal::Journal;
use super::sandbox::{
    MAX_BYTES, Sandbox, Tree, check_cancel, open_regular, read_regular, require_absent,
};
use super::{CheckoutError, CheckoutStage};
use crate::worktree_lifecycle::{
    CheckoutRegistration, identity::AuthorizedDirectory, state::StateLock,
};
use crate::{GitProbe, PersistentIntent, WorktreeRegistrationCoordinator};

pub(super) async fn checkout(
    git: &GitProbe,
    coordinator: &WorktreeRegistrationCoordinator,
    intent: &PersistentIntent,
    lock: &StateLock,
    journal: &mut Journal,
    mut cancel: oneshot::Receiver<()>,
) -> Result<(), CheckoutError> {
    check_cancel(&mut cancel)?;
    let registration = CheckoutRegistration::open(coordinator, intent)?;
    let target = PinnedDirectory::authorized(&registration.target)?;
    let admin = PinnedDirectory::authorized(&registration.git_dir)?;
    let common = PinnedDirectory::authorized(&registration.common_dir)?;
    let state = PinnedDirectory::authorized(&coordinator.state_root)?;
    if state.file.metadata()?.dev() != admin.file.metadata()?.dev() {
        return Err(CheckoutError::Unsafe("持久协调目录与 Git 管理目录跨卷"));
    }
    recheck(&registration, intent, &admin, &common, true)?;
    // 子进程通过父进程公开的目录句柄访问私有暂存。路径前缀被替换时仍绑定原协调域。
    let scratch = state
        .child_process_path()
        .join(format!("checkout-{}.stage", intent.request_id));
    let sandbox = Sandbox::create(&scratch, &intent.base_oid)?;
    sandbox.snapshot(common.path(), admin.path())?;
    let tree = sandbox
        .preflight(git, &intent.base_oid, &mut cancel)
        .await?;
    sandbox.recheck_config(common.path(), admin.path())?;
    recheck(&registration, intent, &admin, &common, true)?;
    lock.refresh(&coordinator.state_root)?;
    journal.enter(CheckoutStage::Materialize)?;
    sandbox
        .materialize(git, &intent.base_oid, &tree, &mut cancel)
        .await?;

    let staged = PinnedDirectory::open(&sandbox.work)?;
    let staged_index = sandbox.admin.join("index");
    let index = read_regular(&staged_index, 16 * 1024 * 1024)?;
    verify_tree(&staged, &staged, &tree, false, &mut cancel)?;
    sandbox.recheck_config(common.path(), admin.path())?;
    recheck(&registration, intent, &admin, &common, true)?;
    lock.refresh(&coordinator.state_root)?;
    check_cancel(&mut cancel)?;
    journal.enter(CheckoutStage::PublishIndex)?;
    open_regular(&staged_index)?.sync_all()?;
    fs::hard_link(&staged_index, admin.path().join("index"))?;
    admin.file.sync_all()?;

    journal.enter(CheckoutStage::PublishFiles)?;
    publish_files(&target, &staged, &tree, &mut cancel, || {
        registration.target.refresh().map_err(Into::into)
    })?;
    journal.enter(CheckoutStage::Verify)?;
    recheck(&registration, intent, &admin, &common, false)?;
    verify_tree(&target, &staged, &tree, true, &mut cancel)?;
    if read_regular(&admin.path().join("index"), 16 * 1024 * 1024)? != index {
        return Err(CheckoutError::Unsafe("目标 index 已变化"));
    }
    sandbox.recheck_config(common.path(), admin.path())?;
    recheck(&registration, intent, &admin, &common, false)?;
    check_cancel(&mut cancel)
}

fn recheck(
    registration: &CheckoutRegistration,
    intent: &PersistentIntent,
    admin: &PinnedDirectory,
    common: &PinnedDirectory,
    pending: bool,
) -> Result<(), CheckoutError> {
    registration.recheck(intent, pending)?;
    for name in ["locked", "index.lock", "HEAD.lock", "config.worktree"] {
        require_absent(&admin.path().join(name), "管理目录存在锁或专属配置")?;
    }
    if pending {
        require_absent(&admin.path().join("index"), "初始 index 已存在")?;
    }
    if read_regular(&admin.path().join("HEAD"), 512)?
        != format!("ref: {}\n", intent.branch_ref).as_bytes()
    {
        return Err(CheckoutError::Unsafe("目标 HEAD 已变化"));
    }
    // 新注册的分支使用 loose ref。遇到 pack-refs 或其他不可直接核验的写入，保留待诊断记录。
    let path = Path::new(&intent.branch_ref);
    let (directory, name) = split_path(&intent.branch_ref);
    let reference = common.descendant(directory)?;
    if read_regular(&reference.path().join(name), 128)?
        != format!("{}\n", intent.base_oid).as_bytes()
    {
        return Err(CheckoutError::Unsafe("固定分支 OID 已变化"));
    }
    let lock_name = format!(
        "{}.lock",
        path.file_name()
            .and_then(|name| name.to_str())
            .ok_or(CheckoutError::Unsafe("无效分支路径"))?
    );
    require_absent(&reference.path().join(lock_name), "分支正在被修改")?;
    Ok(())
}

pub(super) struct PinnedDirectory {
    file: File,
    path: PathBuf,
}

impl PinnedDirectory {
    pub(super) fn open(path: &Path) -> Result<Self, CheckoutError> {
        // Linux O_DIRECTORY | O_NOFOLLOW。句柄保留到完成本目录的最后一次操作。
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(0o200000 | 0o400000)
            .open(path)?;
        let descriptor = PathBuf::from(format!("/proc/self/fd/{}", file.as_raw_fd()));
        if identity(&file.metadata()?) != identity(&descriptor.metadata()?) {
            return Err(CheckoutError::Unsafe("无法绑定目录句柄"));
        }
        Ok(Self {
            file,
            path: descriptor,
        })
    }

    fn authorized(directory: &AuthorizedDirectory) -> Result<Self, CheckoutError> {
        directory.refresh()?;
        let pinned = Self::open(directory.path())?;
        let metadata = pinned.file.metadata()?;
        let created = metadata
            .created()?
            .duration_since(UNIX_EPOCH)
            .map_err(|_| CheckoutError::Unsafe("目录创建时间异常"))?;
        let encoded = directory.identity().to_bytes();
        if encoded[..8] != metadata.dev().to_le_bytes()
            || encoded[8..16] != metadata.ino().to_le_bytes()
            || encoded[16..] != created.as_nanos().to_le_bytes()
        {
            return Err(CheckoutError::Unsafe("目录句柄不属于授权对象"));
        }
        directory.refresh()?;
        Ok(pinned)
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    fn child_process_path(&self) -> PathBuf {
        PathBuf::from(format!(
            "/proc/{}/fd/{}",
            std::process::id(),
            self.file.as_raw_fd()
        ))
    }

    fn descendant(&self, path: &str) -> Result<Self, CheckoutError> {
        let mut handle = Self {
            file: self.file.try_clone()?,
            path: PathBuf::new(),
        };
        handle.path = PathBuf::from(format!("/proc/self/fd/{}", handle.file.as_raw_fd()));
        for component in path.split('/').filter(|part| !part.is_empty()) {
            handle = Self::open(&handle.path.join(component))?;
        }
        Ok(handle)
    }
}

fn identity(metadata: &Metadata) -> (u64, u64) {
    (metadata.dev(), metadata.ino())
}

fn split_path(path: &str) -> (&str, &str) {
    path.rsplit_once('/').unwrap_or(("", path))
}

fn open_published_parent(
    root: &PinnedDirectory,
    path: &str,
    published: &BTreeMap<String, (u64, u64)>,
) -> Result<PinnedDirectory, CheckoutError> {
    let mut handle = root.descendant("")?;
    let mut prefix = String::new();
    for component in path.split('/').filter(|part| !part.is_empty()) {
        if !prefix.is_empty() {
            prefix.push('/');
        }
        prefix.push_str(component);
        handle = PinnedDirectory::open(&handle.path.join(component))?;
        if published.get(&prefix) != Some(&identity(&handle.file.metadata()?)) {
            return Err(CheckoutError::Unsafe("已发布的工作目录被替换"));
        }
    }
    Ok(handle)
}

fn publish_files(
    target: &PinnedDirectory,
    staged: &PinnedDirectory,
    tree: &Tree,
    cancel: &mut oneshot::Receiver<()>,
    mut refresh: impl FnMut() -> Result<(), CheckoutError>,
) -> Result<(), CheckoutError> {
    let mut published = BTreeMap::new();
    let device = target.file.metadata()?.dev();
    for directory in &tree.directories {
        check_cancel(cancel)?;
        refresh()?;
        let (parent, name) = split_path(directory);
        let parent = open_published_parent(target, parent, &published)?;
        let destination = parent.path().join(name);
        fs::DirBuilder::new().mode(0o700).create(&destination)?;
        let created = PinnedDirectory::open(&destination)?;
        let metadata = created.file.metadata()?;
        if metadata.dev() != device || fs::read_dir(created.path())?.next().transpose()?.is_some() {
            return Err(CheckoutError::Unsafe("新建工作目录边界已变化"));
        }
        published.insert(directory.clone(), identity(&metadata));
        parent.file.sync_all()?;
    }
    for (path, executable) in &tree.files {
        check_cancel(cancel)?;
        refresh()?;
        let (parent, name) = split_path(path);
        let parent = open_published_parent(target, parent, &published)?;
        let mut source = open_regular(&staged.path().join(path))?;
        let mode = if *executable { 0o700 } else { 0o600 };
        let mut destination = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(mode)
            .open(parent.path().join(name))?;
        io::copy(&mut source, &mut destination)?;
        destination.sync_all()?;
        parent.file.sync_all()?;
    }
    target.file.sync_all()?;
    Ok(())
}

fn verify_tree(
    root: &PinnedDirectory,
    staged: &PinnedDirectory,
    tree: &Tree,
    linked: bool,
    cancel: &mut oneshot::Receiver<()>,
) -> Result<(), CheckoutError> {
    let mut remaining = MAX_BYTES;
    let mut total_files = 0;
    for directory in std::iter::once("").chain(tree.directories.iter().map(String::as_str)) {
        check_cancel(cancel)?;
        let pinned = root.descendant(directory)?;
        for entry in fs::read_dir(pinned.path())? {
            check_cancel(cancel)?;
            let entry = entry?;
            let name = entry.file_name();
            let name = name
                .to_str()
                .ok_or(CheckoutError::Unsafe("工作目录出现不可表示的文件名"))?;
            if linked && directory.is_empty() && name == ".git" {
                continue;
            }
            let relative = if directory.is_empty() {
                name.to_owned()
            } else {
                format!("{directory}/{name}")
            };
            let kind = entry.file_type()?;
            if kind.is_dir() && tree.directories.contains(&relative) {
                continue;
            }
            let Some(executable) = tree.files.get(&relative).filter(|_| kind.is_file()) else {
                return Err(CheckoutError::Unsafe("工作目录出现非预期条目"));
            };
            let file = open_regular(&entry.path())?;
            let metadata = file.metadata()?;
            remaining = remaining
                .checked_sub(metadata.len())
                .ok_or(CheckoutError::Unsafe("检出内容大小超限"))?;
            if (metadata.mode() & 0o100 != 0) != *executable {
                return Err(CheckoutError::Unsafe("检出文件模式不一致"));
            }
            if linked
                && (!same_contents(
                    file,
                    open_regular(&staged.path().join(&relative))?,
                    metadata.len(),
                )?)
            {
                return Err(CheckoutError::Unsafe("检出工作文件内容已变化"));
            }
            total_files += 1;
        }
    }
    if total_files != tree.files.len() {
        return Err(CheckoutError::Unsafe("检出文件数量不一致"));
    }
    Ok(())
}

fn same_contents(mut target: File, mut staged: File, length: u64) -> io::Result<bool> {
    if staged.metadata()?.len() != length {
        return Ok(false);
    }
    let mut remaining = length;
    let mut first = [0; 8192];
    let mut second = [0; 8192];
    while remaining > 0 {
        let size = usize::try_from(remaining.min(first.len() as u64)).unwrap_or(first.len());
        target.read_exact(&mut first[..size])?;
        staged.read_exact(&mut second[..size])?;
        if first[..size] != second[..size] {
            return Ok(false);
        }
        remaining -= size as u64;
    }
    Ok(target.read(&mut first[..1])? == 0 && staged.read(&mut second[..1])? == 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worktree_checkout::sandbox::{parse_tree, tests::Fixture};
    use std::os::unix::fs::symlink;

    fn prepare(root: &Path) -> (PinnedDirectory, PinnedDirectory, Tree) {
        let stage = root.join("stage");
        let target = root.join("target");
        fs::create_dir_all(stage.join("nested")).expect("创建夹具路径");
        fs::create_dir(&target).expect("创建夹具路径");
        fs::write(stage.join("first.txt"), b"first").expect("写入夹具文件");
        fs::write(stage.join("nested/second.txt"), b"second").expect("写入夹具文件");
        let oid = "1234567890123456789012345678901234567890";
        let listing = format!(
            "100644 blob {oid} 6\tfirst.txt\0{}",
            format_args!("100644 blob {oid} 6\tnested/second.txt\0")
        );
        (
            PinnedDirectory::open(&target).expect("核对夹具操作结果"),
            PinnedDirectory::open(&stage).expect("核对夹具操作结果"),
            parse_tree(listing.as_bytes(), 40).expect("解析夹具提交树"),
        )
    }

    #[test]
    fn mid_publication_user_file_blocks_creation_and_partial_files_remain() {
        let fixture = Fixture::new();
        let (target, staged, tree) = prepare(fixture.path());
        let mut journal = Journal::begin(fixture.path(), "request").expect("持久化夹具检出记录");
        for phase in [
            CheckoutStage::Materialize,
            CheckoutStage::PublishIndex,
            CheckoutStage::PublishFiles,
        ] {
            journal.enter(phase).expect("持久化夹具检出记录");
        }
        let (_guard, mut cancel) = oneshot::channel();
        let mut refreshes = 0;
        let error = publish_files(&target, &staged, &tree, &mut cancel, || {
            refreshes += 1;
            if refreshes == 3 {
                fs::write(target.path().join("nested/second.txt"), b"user")?;
            }
            Ok(())
        })
        .expect_err("夹具操作应被拒绝");
        journal.failure(&error).expect("持久化夹具检出记录");
        assert!(
            matches!(error, CheckoutError::Io(error) if error.kind() == io::ErrorKind::AlreadyExists)
        );
        assert_eq!(
            fs::read(target.path().join("first.txt")).expect("读取夹具文件"),
            b"first"
        );
        assert_eq!(
            fs::read(target.path().join("nested/second.txt")).expect("读取夹具文件"),
            b"user"
        );
        assert_eq!(
            super::super::journal::read(fixture.path(), "request").expect("持久化夹具检出记录"),
            super::super::CheckoutStatus::Disabled {
                stage: Some(CheckoutStage::PublishFiles),
                reason: Some("existing_entry".to_owned()),
            }
        );
    }

    #[test]
    fn rejects_symlink_replacement_during_publication_and_extra_files_afterwards() {
        let fixture = Fixture::new();
        let (target, staged, tree) = prepare(fixture.path());
        let outside = fixture.path().join("outside");
        fs::create_dir(&outside).expect("创建夹具路径");
        let (_guard, mut cancel) = oneshot::channel();
        let mut refreshes = 0;
        assert!(
            publish_files(&target, &staged, &tree, &mut cancel, || {
                refreshes += 1;
                if refreshes == 3 {
                    fs::rename(
                        target.path().join("nested"),
                        target.path().join("old-nested"),
                    )?;
                    symlink(&outside, target.path().join("nested"))?;
                }
                Ok(())
            })
            .is_err()
        );
        assert!(
            fs::read_dir(&outside)
                .expect("枚举夹具目录")
                .next()
                .is_none()
        );

        let other = Fixture::new();
        let (target, staged, tree) = prepare(other.path());
        let (_guard, mut cancel) = oneshot::channel();
        publish_files(&target, &staged, &tree, &mut cancel, || Ok(())).expect("填充夹具工作目录");
        fs::write(target.path().join("user.txt"), b"user").expect("写入夹具文件");
        assert!(verify_tree(&target, &staged, &tree, true, &mut cancel).is_err());
        assert_eq!(
            fs::read(target.path().join("user.txt")).expect("读取夹具文件"),
            b"user"
        );
    }
}
