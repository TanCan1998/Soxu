use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use super::git::{self, Git};
use super::platform::{self, Directory, Entry};
use super::*;

mod encoding;

const MAX_RECORD: u64 = 48 * 1024 * 1024;

pub(super) struct Operation {
    pub(super) id: String,
    state: Directory,
    dir: Directory,
    index: PathBuf,
    hooks: PathBuf,
}

impl Operation {
    pub(super) fn new(state: Directory) -> Result<Self, CheckpointError> {
        let id = git::id();
        private_dir(&state.anchored().join(&id))?;
        let dir = state.child(id.as_ref())?;
        private_dir(&dir.anchored().join("hooks"))?;
        let index = dir.path().join("index");
        let hooks = dir.path().join("hooks");
        state.sync()?;
        dir.sync()?;
        Ok(Self {
            id,
            state,
            dir,
            index,
            hooks,
        })
    }

    pub(super) fn git<'a>(
        &'a self,
        probe: &'a GitProbe,
        root: &'a Path,
        policy: &CheckpointPolicy,
    ) -> Git<'a> {
        Git {
            probe,
            root,
            index: &self.index,
            hooks: &self.hooks,
            deadline: Instant::now() + policy.total_timeout,
            max_output: policy.max_output_bytes,
            timeout: policy.command_timeout,
        }
    }

    pub(super) fn lock(&self, digest: &str) -> Result<File, CheckpointError> {
        // 系统锁随进程退出释放，保留文件以避免两个进程锁住不同 inode。
        let path = self
            .state
            .anchored()
            .join(format!("worktree-{digest}.lock"));
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600).custom_flags(0o400000 | 0o4000);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(3).custom_flags(0x0020_0000);
        }
        let lock = options.open(path)?;
        if !platform::is_regular(&lock.metadata()?) {
            return Err(CheckpointError::Invalid("无效的工作树锁"));
        }
        lock.try_lock().map_err(|error| match error {
            std::fs::TryLockError::WouldBlock => CheckpointError::Busy,
            std::fs::TryLockError::Error(error) => error.into(),
        })?;
        Ok(lock)
    }

    pub(super) fn intent(&self, record: &CheckpointRecord) -> Result<(), CheckpointError> {
        self.write("intent", record)
    }
    pub(super) fn publish(&self, record: &CheckpointRecord) -> Result<(), CheckpointError> {
        self.write("record", record)
    }

    fn write(&self, label: &str, record: &CheckpointRecord) -> Result<(), CheckpointError> {
        self.state.verify()?;
        self.dir.verify()?;
        let temporary = self.dir.anchored().join(format!("{label}.pending"));
        let destination = self.dir.anchored().join(label);
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        let payload = encoding::encode(record)?;
        if payload.len() as u64 > MAX_RECORD {
            return Err(CheckpointError::Budget("Checkpoint 记录大小"));
        }
        file.write_all(&payload)?;
        file.sync_all()?;
        drop(file);
        fs::rename(temporary, destination)?;
        self.dir.sync()?;
        Ok(())
    }
}

pub(super) fn state_directory(path: &Path) -> Result<Directory, CheckpointError> {
    if !path.is_absolute() {
        return Err(CheckpointError::Invalid("应用状态目录必须为绝对路径"));
    }
    let state = Directory::open(path)?;
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::MetadataExt;
        if state.anchored().metadata()?.mode() & 0o077 != 0 {
            return Err(CheckpointError::Invalid("应用状态目录必须为独占权限"));
        }
    }
    Ok(state)
}

fn private_dir(path: &Path) -> io::Result<()> {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new().mode(0o700).create(path)
    }
    #[cfg(not(target_os = "linux"))]
    fs::create_dir(path)
}

pub(super) async fn inspect(
    probe: &GitProbe,
    repository: &Path,
    operation: &Path,
) -> Result<CheckpointRecord, CheckpointError> {
    platform::platform_observation()?;
    let dir = Directory::open(operation)?;
    let path_id = dir
        .path()
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or(CheckpointError::Invalid("操作目录 ID"))?;
    let mut intent = read(&dir, "intent")?.ok_or(CheckpointError::Invalid("缺少持久 Intent"))?;
    if intent.lifecycle != CheckpointLifecycle::Intent || path_id != intent.checkpoint_id {
        return Err(CheckpointError::Invalid("Intent 绑定不一致"));
    }
    let root = Directory::open(repository)?;

    let index = dir.path().join("index");
    let hooks = dir.path().join("hooks");
    let _hooks = Directory::open(&hooks)?;
    let p = CheckpointPolicy::default();
    let git = Git {
        probe,
        root: root.path(),
        index: &index,
        hooks: &hooks,
        deadline: Instant::now() + p.total_timeout,
        max_output: p.max_output_bytes,
        timeout: p.command_timeout,
    };
    let (root, admin, common, identity) = super::capture::repository_paths(&git, &root).await?;
    if root != intent.before.worktree_root
        || admin != intent.before.git_dir
        || common != intent.before.common_dir
        || identity != intent.before.directory_identity
    {
        return Err(CheckpointError::Invalid("Intent 属于不同的工作树"));
    }
    let reference = intent.reference();
    git.text(&["check-ref-format", &reference]).await?;
    if !git
        .accepted(
            &git::strings(&["symbolic-ref", "-q", &reference]),
            None,
            &[1],
        )
        .await?
        .is_empty()
    {
        return Err(CheckpointError::CasConflict);
    }
    let actual = git
        .accepted(
            &git::strings(&["rev-parse", "--verify", "--quiet", &reference]),
            None,
            &[1],
        )
        .await?;
    if !actual.is_empty() && git::oid(&actual)? != intent.commit_oid {
        return Err(CheckpointError::CasConflict);
    }
    let commit = git
        .text(&["cat-file", "commit", &intent.commit_oid])
        .await?;
    let mut headers = commit.split(|b| *b == b'\n');
    if headers.next() != Some(format!("tree {}", intent.worktree_tree_oid).as_bytes()) {
        return Err(CheckpointError::Invalid("Intent 的树对象不匹配"));
    }
    match intent.parent_oid.as_ref() {
        Some(parent) if headers.next() != Some(format!("parent {parent}").as_bytes()) => {
            return Err(CheckpointError::Invalid("Intent 的父对象不匹配"));
        }
        None if headers
            .next()
            .is_some_and(|line| line.starts_with(b"parent ")) =>
        {
            return Err(CheckpointError::Invalid("Intent 的根提交不匹配"));
        }
        _ => {}
    }
    if let Some(published) = read(&dir, "record")? {
        intent.lifecycle = CheckpointLifecycle::Published;
        if actual.is_empty() || published != intent {
            return Err(CheckpointError::Invalid("发布记录与 Intent 或 ref 不一致"));
        }
    } else if !actual.is_empty() {
        intent.lifecycle = CheckpointLifecycle::Published;
    }
    Ok(intent)
}

fn read(dir: &Directory, label: &str) -> Result<Option<CheckpointRecord>, CheckpointError> {
    match dir.entry(Path::new(label))? {
        Entry::Absent => Ok(None),
        Entry::Special => Err(CheckpointError::Invalid("记录必须为普通文件")),
        Entry::Regular(mut file) => {
            let (bytes, _) = platform::read_bounded(&mut file, MAX_RECORD)?
                .ok_or(CheckpointError::Budget("Checkpoint 记录大小"))?;
            Ok(Some(encoding::decode(&bytes)?))
        }
    }
}
