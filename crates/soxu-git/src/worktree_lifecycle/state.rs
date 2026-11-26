use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use super::WorktreeLifecycleError;
use super::identity::{AuthorizedDirectory, checked_directory, is_reparse};

pub(crate) const MAX_INTENT_FILES: usize = 256;
pub(crate) const MAX_TOTAL_INTENT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug)]
pub(crate) struct StateLock(File);

impl StateLock {
    pub(crate) fn refresh(
        &self,
        state_root: &AuthorizedDirectory,
    ) -> Result<(), WorktreeLifecycleError> {
        require_private_root(state_root)?;
        let held = self.0.metadata()?;
        let named = state_root
            .path()
            .join("registration.lock")
            .symlink_metadata()?;
        if !named.is_file() || named.is_symlink() || is_reparse(&named) {
            return Err(WorktreeLifecycleError::IdentityChanged);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if (held.dev(), held.ino()) != (named.dev(), named.ino()) {
                return Err(WorktreeLifecycleError::IdentityChanged);
            }
        }
        #[cfg(not(unix))]
        let _ = held;
        state_root.refresh()
    }
}

pub(crate) fn require_private_root(
    state_root: &AuthorizedDirectory,
) -> Result<(), WorktreeLifecycleError> {
    state_root.refresh()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = state_root.path().symlink_metadata()?.permissions().mode();
        if mode & 0o077 != 0 {
            return Err(WorktreeLifecycleError::Unauthorized(
                "协调域必须是宿主私有目录",
            ));
        }
    }
    Ok(())
}

fn ensure_intents(
    state_root: &AuthorizedDirectory,
) -> Result<AuthorizedDirectory, WorktreeLifecycleError> {
    state_root.refresh()?;
    let path = state_root.path().join("intents");
    match path.symlink_metadata() {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir(&path)?;
        }
        Err(error) => return Err(error.into()),
    }
    let directory = checked_directory(&path)?;
    for entry in std::fs::read_dir(directory.path())? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        if metadata.is_symlink() || is_reparse(&metadata) {
            return Err(WorktreeLifecycleError::Unauthorized(
                "状态意图目录包含链接或 reparse point",
            ));
        }
    }
    sync_directory(state_root)?;
    state_root.refresh()?;
    Ok(directory)
}

pub(crate) fn read_intents_directory(
    state_root: &AuthorizedDirectory,
) -> Result<Option<AuthorizedDirectory>, WorktreeLifecycleError> {
    let path = state_root.path().join("intents");
    match path.symlink_metadata() {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
        Ok(_) => {
            let directory = checked_directory(&path)?;
            for entry in std::fs::read_dir(directory.path())? {
                let entry = entry?;
                let metadata = entry.metadata()?;
                if metadata.is_symlink() || is_reparse(&metadata) {
                    return Err(WorktreeLifecycleError::Unauthorized(
                        "状态意图目录包含链接或 reparse point",
                    ));
                }
            }
            state_root.refresh()?;
            Ok(Some(directory))
        }
    }
}

pub(crate) async fn acquire_state_lock(
    state_root: &AuthorizedDirectory,
) -> Result<StateLock, WorktreeLifecycleError> {
    let deadline = tokio::time::Instant::now() + super::LOCK_WAIT;
    loop {
        require_private_root(state_root)?;
        let path = state_root.path().join("registration.lock");
        let file = open_regular_file(&path, true)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() || metadata.is_symlink() || is_reparse(&metadata) {
            return Err(WorktreeLifecycleError::Unauthorized("协调锁必须是普通文件"));
        }
        match file.try_lock() {
            Ok(()) => {
                let lock = StateLock(file);
                lock.refresh(state_root)?;
                return Ok(lock);
            }
            Err(_) if tokio::time::Instant::now() < deadline => {
                tokio::time::sleep(super::LOCK_POLL).await;
            }
            Err(_) => return Err(WorktreeLifecycleError::StateBusy),
        }
    }
}

pub(crate) fn intent_path(state_root: &AuthorizedDirectory, request_id: &str) -> PathBuf {
    state_root
        .path()
        .join("intents")
        .join(format!("{request_id}.intent"))
}

pub(crate) fn write_intent(
    state_root: &AuthorizedDirectory,
    bytes: &[u8],
    request_id: &str,
) -> Result<(), WorktreeLifecycleError> {
    let intents = ensure_intents(state_root)?;
    let final_path = intent_path(state_root, request_id);
    let temporary_path = intents.path().join(format!("{request_id}.tmp"));
    match final_path.symlink_metadata() {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
        Ok(_) => return Err(WorktreeLifecycleError::RequestConflict),
    }
    write_atomic(&intents, &temporary_path, &final_path, bytes)?;
    state_root.refresh()
}

pub(crate) fn rewrite_intent(
    state_root: &AuthorizedDirectory,
    bytes: &[u8],
    request_id: &str,
) -> Result<(), WorktreeLifecycleError> {
    let intents = ensure_intents(state_root)?;
    let final_path = intent_path(state_root, request_id);
    let metadata = final_path.symlink_metadata()?;
    if !metadata.is_file() || metadata.is_symlink() || is_reparse(&metadata) {
        return Err(WorktreeLifecycleError::InvalidInput("意图记录类型"));
    }
    let temporary_path = intents.path().join(format!("{request_id}.next"));
    write_atomic(&intents, &temporary_path, &final_path, bytes)?;
    state_root.refresh()
}

fn write_atomic(
    intents: &AuthorizedDirectory,
    temporary_path: &Path,
    final_path: &Path,
    bytes: &[u8],
) -> Result<(), WorktreeLifecycleError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temporary_path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.is_symlink() || is_reparse(&metadata) {
        return Err(WorktreeLifecycleError::InvalidInput("意图临时文件类型"));
    }
    file.write_all(bytes)?;
    file.sync_all()?;
    std::fs::rename(temporary_path, final_path)?;
    sync_directory(intents)?;
    Ok(())
}

fn sync_directory(directory: &AuthorizedDirectory) -> Result<(), WorktreeLifecycleError> {
    #[cfg(unix)]
    {
        let file = File::open(directory.path())?;
        file.sync_all()?;
    }
    #[cfg(not(unix))]
    let _ = directory;
    Ok(())
}

pub(crate) fn read_regular_file(path: &Path) -> Result<Vec<u8>, WorktreeLifecycleError> {
    let file = open_regular_file(path, false)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.is_symlink() || is_reparse(&metadata) {
        return Err(WorktreeLifecycleError::InvalidInput(
            "意图记录不是有界普通文件",
        ));
    }
    let mut bytes = Vec::new();
    file.take(super::MAX_INTENT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > super::MAX_INTENT_BYTES {
        return Err(WorktreeLifecycleError::InvalidInput("意图记录超过大小限制"));
    }
    Ok(bytes)
}

fn open_regular_file(path: &Path, create: bool) -> Result<File, WorktreeLifecycleError> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .write(create)
        .create(create)
        .truncate(false);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x0020_0000);
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(0o400_000);
    }
    Ok(options.open(path)?)
}
