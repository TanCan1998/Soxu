//! 协调域锁与确定性文本留痕（`SOXURC1`）。
//!
//! 记录只用于两类事：拒绝同 ID 变参、拒绝在结果未知时重放，以及重启后只读回查。它不是命令回放日志。

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::platform::{AuthorizedDirectory, DirectoryIdentity};
use super::{
    InitMode, RepositoryCreateError, RepositoryCreateResult, RepositoryCreateStage,
    RepositoryOperation, RepositoryOperationRecord, RepositoryTerminalState,
};

const LOCK_WAIT: Duration = Duration::from_secs(5);
const LOCK_POLL: Duration = Duration::from_millis(100);
const LOCK_NAME: &str = "repository-create.lock";
const RECORDS_DIR: &str = "records";
const RECORD_SUFFIX: &str = ".record";

pub(crate) const RECORD_HEADER: &str = "SOXURC1";
pub(crate) const MAX_RECORD_BYTES: u64 = 64 * 1024;
pub(crate) const MAX_RECORDS: usize = 256;

/// 协调域互斥锁。持有期间同一协调域内不会出现第二个创建写者。
#[derive(Debug)]
pub(crate) struct StateLock(File);

impl StateLock {
    pub(crate) fn refresh(
        &self,
        state_root: &AuthorizedDirectory,
    ) -> Result<(), RepositoryCreateError> {
        require_private(state_root)?;
        let held = self.0.metadata().map_err(RepositoryCreateError::Io)?;
        let named = state_root
            .path()
            .join(LOCK_NAME)
            .symlink_metadata()
            .map_err(RepositoryCreateError::Io)?;
        if !named.is_file() || named.is_symlink() || super::platform::is_reparse(&named) {
            return Err(RepositoryCreateError::IdentityChanged);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if (held.dev(), held.ino()) != (named.dev(), named.ino()) {
                return Err(RepositoryCreateError::IdentityChanged);
            }
        }
        #[cfg(not(unix))]
        let _ = held;
        state_root.refresh()
    }
}

/// 打开宿主私有协调域，要求已存在的普通目录。
pub(crate) fn open_root(state_root: &Path) -> Result<AuthorizedDirectory, RepositoryCreateError> {
    if !state_root.is_absolute() {
        return Err(RepositoryCreateError::Unauthorized("协调域必须是绝对路径"));
    }
    let root = AuthorizedDirectory::open(state_root)?;
    require_private(&root)?;
    Ok(root)
}

fn require_private(root: &AuthorizedDirectory) -> Result<(), RepositoryCreateError> {
    root.refresh()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = root
            .path()
            .symlink_metadata()
            .map_err(RepositoryCreateError::Io)?
            .permissions()
            .mode();
        if mode & 0o077 != 0 {
            return Err(RepositoryCreateError::Unauthorized(
                "协调域必须是宿主私有目录",
            ));
        }
    }
    Ok(())
}

pub(crate) async fn acquire(
    state_root: &AuthorizedDirectory,
) -> Result<StateLock, RepositoryCreateError> {
    let deadline = tokio::time::Instant::now() + LOCK_WAIT;
    loop {
        require_private(state_root)?;
        let path = state_root.path().join(LOCK_NAME);
        let file = open_regular_file(&path, true)?;
        let metadata = file.metadata().map_err(RepositoryCreateError::Io)?;
        if !metadata.is_file() || metadata.is_symlink() || super::platform::is_reparse(&metadata) {
            return Err(RepositoryCreateError::Unauthorized("协调锁必须是普通文件"));
        }
        match file.try_lock() {
            Ok(()) => {
                let lock = StateLock(file);
                lock.refresh(state_root)?;
                return Ok(lock);
            }
            Err(_) if tokio::time::Instant::now() < deadline => {
                tokio::time::sleep(LOCK_POLL).await;
            }
            Err(_) => return Err(RepositoryCreateError::StateBusy),
        }
    }
}

fn ensure_records(
    state_root: &AuthorizedDirectory,
) -> Result<AuthorizedDirectory, RepositoryCreateError> {
    state_root.refresh()?;
    let path = state_root.path().join(RECORDS_DIR);
    match std::fs::symlink_metadata(&path) {
        Ok(metadata) => {
            if !super::platform::plain_directory(&metadata) {
                return Err(RepositoryCreateError::Unauthorized("留痕目录不是普通目录"));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir(&path).map_err(RepositoryCreateError::Io)?;
        }
        Err(error) => return Err(RepositoryCreateError::Io(error)),
    }
    let records = AuthorizedDirectory::open(&path)?;
    if records.path() != path {
        return Err(RepositoryCreateError::Unauthorized("留痕目录路径不规范"));
    }
    Ok(records)
}

pub(crate) fn record_path(state_root: &AuthorizedDirectory, request_id: &str) -> PathBuf {
    state_root
        .path()
        .join(RECORDS_DIR)
        .join(format!("{request_id}{RECORD_SUFFIX}"))
}

/// 只读枚举协调域内的全部留痕，条目数与体积均有上限。
pub(crate) fn read_records(
    state_root: &AuthorizedDirectory,
) -> Result<Vec<RepositoryOperationRecord>, RepositoryCreateError> {
    state_root.refresh()?;
    let directory = state_root.path().join(RECORDS_DIR);
    if !directory.exists() {
        return Ok(Vec::new());
    }
    let entries = std::fs::read_dir(&directory).map_err(RepositoryCreateError::Io)?;
    let mut records = Vec::new();
    for entry in entries {
        let entry = entry.map_err(RepositoryCreateError::Io)?;
        let path = entry.path();
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            return Err(RepositoryCreateError::Unauthorized("留痕文件名不可表示"));
        };
        if !name.ends_with(RECORD_SUFFIX) {
            continue;
        }
        let metadata = path.symlink_metadata().map_err(RepositoryCreateError::Io)?;
        if !metadata.is_file() || metadata.is_symlink() || super::platform::is_reparse(&metadata) {
            return Err(RepositoryCreateError::Unauthorized("留痕条目不是普通文件"));
        }
        if records.len() >= MAX_RECORDS {
            return Err(RepositoryCreateError::InvalidInput("协调域留痕条目过多"));
        }
        records.push(decode(&read_regular_file(&path)?)?);
    }
    Ok(records)
}

pub(crate) fn read_record(
    state_root: &AuthorizedDirectory,
    request_id: &str,
) -> Result<Option<RepositoryOperationRecord>, RepositoryCreateError> {
    let path = record_path(state_root, request_id);
    match path.symlink_metadata() {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(RepositoryCreateError::Io(error)),
        Ok(metadata) => {
            if !metadata.is_file()
                || metadata.is_symlink()
                || super::platform::is_reparse(&metadata)
            {
                return Err(RepositoryCreateError::Unauthorized("留痕条目不是普通文件"));
            }
            Ok(Some(decode(&read_regular_file(&path)?)?))
        }
    }
}

fn read_regular_file(path: &Path) -> Result<Vec<u8>, RepositoryCreateError> {
    let file = open_regular_file(path, false)?;
    let metadata = file.metadata().map_err(RepositoryCreateError::Io)?;
    if metadata.len() > MAX_RECORD_BYTES {
        return Err(RepositoryCreateError::InvalidInput("留痕记录超过体积上限"));
    }
    let mut bytes = Vec::new();
    file.take(MAX_RECORD_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(RepositoryCreateError::Io)?;
    if bytes.len() as u64 > MAX_RECORD_BYTES {
        return Err(RepositoryCreateError::InvalidInput("留痕记录超过体积上限"));
    }
    Ok(bytes)
}

fn open_regular_file(path: &Path, create: bool) -> Result<File, RepositoryCreateError> {
    let mut options = OpenOptions::new();
    options.read(true);
    if create {
        options.write(true).create(true).truncate(false);
    }
    options.open(path).map_err(RepositoryCreateError::Io)
}

/// 首次写入：目标必须尚不存在，避免覆盖旧留痕。
pub(crate) fn write_record(
    state_root: &AuthorizedDirectory,
    record: &RepositoryOperationRecord,
) -> Result<(), RepositoryCreateError> {
    let bytes = encode(record)?;
    let records = ensure_records(state_root)?;
    let final_path = record_path(state_root, &record.request_id);
    match final_path.symlink_metadata() {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(RepositoryCreateError::Io(error)),
        Ok(_) => return Err(RepositoryCreateError::RequestConflict),
    }
    let temporary = records.path().join(format!("{}.tmp", record.request_id));
    write_atomic(&records, &temporary, &final_path, &bytes)?;
    state_root.refresh()
}

/// 覆盖写入既有留痕，仅用于推进同一请求的阶段。
pub(crate) fn rewrite_record(
    state_root: &AuthorizedDirectory,
    record: &RepositoryOperationRecord,
) -> Result<(), RepositoryCreateError> {
    let bytes = encode(record)?;
    let records = ensure_records(state_root)?;
    let final_path = record_path(state_root, &record.request_id);
    let metadata = final_path
        .symlink_metadata()
        .map_err(RepositoryCreateError::Io)?;
    if !metadata.is_file() || metadata.is_symlink() || super::platform::is_reparse(&metadata) {
        return Err(RepositoryCreateError::InvalidInput("留痕条目类型"));
    }
    let temporary = records.path().join(format!("{}.next", record.request_id));
    write_atomic(&records, &temporary, &final_path, &bytes)?;
    state_root.refresh()
}

fn write_atomic(
    records: &AuthorizedDirectory,
    temporary: &Path,
    final_path: &Path,
    bytes: &[u8],
) -> Result<(), RepositoryCreateError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temporary)
        .map_err(RepositoryCreateError::Io)?;
    let metadata = file.metadata().map_err(RepositoryCreateError::Io)?;
    if !metadata.is_file() || metadata.is_symlink() || super::platform::is_reparse(&metadata) {
        return Err(RepositoryCreateError::InvalidInput("留痕临时文件类型"));
    }
    file.write_all(bytes).map_err(RepositoryCreateError::Io)?;
    file.sync_all().map_err(RepositoryCreateError::Io)?;
    std::fs::rename(temporary, final_path).map_err(RepositoryCreateError::Io)?;
    sync_directory(records)?;
    Ok(())
}

fn sync_directory(directory: &AuthorizedDirectory) -> Result<(), RepositoryCreateError> {
    #[cfg(unix)]
    {
        let file = File::open(directory.path()).map_err(RepositoryCreateError::Io)?;
        file.sync_all().map_err(RepositoryCreateError::Io)?;
    }
    #[cfg(not(unix))]
    let _ = directory;
    Ok(())
}

fn escape(text: &str) -> Result<String, RepositoryCreateError> {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '%' => out.push_str("%25"),
            '\n' => out.push_str("%0A"),
            '\r' => out.push_str("%0D"),
            other if other.is_control() => {
                return Err(RepositoryCreateError::InvalidInput("留痕字段含控制字符"));
            }
            other => out.push(other),
        }
    }
    Ok(out)
}

fn unescape(text: &str) -> Option<String> {
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            let rest = std::str::from_utf8(&bytes[index..]).ok()?;
            let character = rest.chars().next()?;
            out.push(character);
            index += character.len_utf8();
            continue;
        }
        let code = text.get(index + 1..index + 3)?;
        out.push(match code {
            "25" => '%',
            "0A" => '\n',
            "0D" => '\r',
            _ => return None,
        });
        index += 3;
    }
    Some(out)
}

fn nanos(time: Option<SystemTime>) -> Option<u128> {
    time.and_then(|time| {
        time.duration_since(UNIX_EPOCH)
            .ok()
            .map(|delta| delta.as_nanos())
    })
}

fn time_of(text: &str) -> Option<Option<SystemTime>> {
    if text == "-" {
        return Some(None);
    }
    let value: u128 = text.parse().ok()?;
    Some(Some(
        UNIX_EPOCH + Duration::from_nanos(u64::try_from(value).ok()?),
    ))
}

fn flag(value: bool) -> &'static str {
    if value { "1" } else { "0" }
}

pub(crate) fn encode(record: &RepositoryOperationRecord) -> Result<Vec<u8>, RepositoryCreateError> {
    let mut text = String::new();
    text.push_str(RECORD_HEADER);
    text.push('\n');
    let mut push = |key: &str, value: &str| {
        text.push_str(key);
        text.push('=');
        text.push_str(value);
        text.push('\n');
    };
    push("request_id", &escape(&record.request_id)?);
    push("operation", record.operation.as_str());
    push("mode", record.mode.map_or("-", InitMode::as_str));
    push(
        "target_root",
        &escape(
            record
                .target_root
                .to_str()
                .ok_or(RepositoryCreateError::InvalidInput(
                    "留痕目标父根不可无损表示",
                ))?,
        )?,
    );
    push(
        "target",
        &escape(
            record
                .target
                .to_str()
                .ok_or(RepositoryCreateError::InvalidInput("留痕目标不可无损表示"))?,
        )?,
    );
    match &record.source {
        Some(source) => push(
            "source",
            &escape(
                source
                    .to_str()
                    .ok_or(RepositoryCreateError::InvalidInput("留痕来源不可无损表示"))?,
            )?,
        ),
        None => push("source", "-"),
    }
    push("stage", record.stage.as_str());
    push("result", record.result.as_str());
    push(
        "exit_code",
        &match &record.result {
            RepositoryCreateResult::Failed {
                exit_code: Some(code),
            } => code.to_string(),
            _ => "-".to_owned(),
        },
    );
    push(
        "stop_confirmed",
        match &record.result {
            RepositoryCreateResult::Unknown { stop_confirmed, .. } => flag(*stop_confirmed),
            _ => "-",
        },
    );
    push(
        "result_reason",
        &match &record.result {
            RepositoryCreateResult::Unknown { reason, .. } => escape(reason)?,
            _ => "-".to_owned(),
        },
    );
    push(
        "terminal",
        record.terminal.map_or("-", RepositoryTerminalState::as_str),
    );
    push("target_absent_before", flag(record.target_absent_before));
    push("target_empty_before", flag(record.target_empty_before));
    push(
        "recorded_at",
        &nanos(Some(record.recorded_at)).map_or("-".to_owned(), |value| value.to_string()),
    );
    push(
        "started_at",
        &nanos(record.started_at).map_or("-".to_owned(), |value| value.to_string()),
    );
    push(
        "observed_at",
        &nanos(record.observed_at).map_or("-".to_owned(), |value| value.to_string()),
    );
    push(
        "target_root_identity",
        &identity_text(record.target_root_identity),
    );
    push("target_identity", &identity_text(record.target_identity));
    push("git_dir_identity", &identity_text(record.git_dir_identity));
    for note in &record.notes {
        push("note", &escape(note)?);
    }
    let bytes = text.into_bytes();
    if bytes.len() as u64 > MAX_RECORD_BYTES {
        return Err(RepositoryCreateError::InvalidInput("留痕记录超过体积上限"));
    }
    Ok(bytes)
}

fn identity_text(identity: Option<DirectoryIdentity>) -> String {
    identity.map_or_else(|| "-".to_owned(), |identity| identity.to_hex())
}

pub(crate) fn decode(bytes: &[u8]) -> Result<RepositoryOperationRecord, RepositoryCreateError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| RepositoryCreateError::InvalidInput("留痕记录不是合法 UTF-8"))?;
    let mut lines = text.split('\n');
    if lines.next() != Some(RECORD_HEADER) {
        return Err(RepositoryCreateError::InvalidInput("留痕记录版本头"));
    }
    let mut request_id = None;
    let mut operation = None;
    let mut mode = None;
    let mut target_root = None;
    let mut target = None;
    let mut source = None;
    let mut stage = None;
    let mut result = None;
    let mut exit_code = None;
    let mut stop_confirmed = None;
    let mut result_reason = None;
    let mut terminal = None;
    let mut absent = None;
    let mut empty = None;
    let mut recorded_at = None;
    let mut started_at = None;
    let mut observed_at = None;
    let mut root_identity = None;
    let mut target_identity = None;
    let mut git_dir_identity = None;
    let mut notes = Vec::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(RepositoryCreateError::InvalidInput("留痕记录字段格式"));
        };
        match key {
            "request_id" => request_id = Some(unescape(value).ok_or(invalid())?),
            "operation" => operation = Some(RepositoryOperation::parse(value).ok_or(invalid())?),
            "mode" => mode = Some(InitMode::parse(value).ok_or(invalid())?),
            "target_root" => target_root = Some(PathBuf::from(unescape(value).ok_or(invalid())?)),
            "target" => target = Some(PathBuf::from(unescape(value).ok_or(invalid())?)),
            "source" => {
                source = Some(if value == "-" {
                    None
                } else {
                    Some(PathBuf::from(unescape(value).ok_or(invalid())?))
                });
            }
            "stage" => stage = Some(RepositoryCreateStage::parse(value).ok_or(invalid())?),
            "result" => result = Some(value.to_owned()),
            "exit_code" => exit_code = Some(value.to_owned()),
            "stop_confirmed" => stop_confirmed = Some(value.to_owned()),
            "result_reason" => result_reason = Some(value.to_owned()),
            "terminal" => terminal = Some(RepositoryTerminalState::parse(value)),
            "target_absent_before" => absent = Some(value == "1"),
            "target_empty_before" => empty = Some(value == "1"),
            "recorded_at" => recorded_at = time_of(value),
            "started_at" => started_at = time_of(value),
            "observed_at" => observed_at = time_of(value),
            "target_root_identity" => root_identity = Some(identity_from(value)?),
            "target_identity" => target_identity = Some(identity_from(value)?),
            "git_dir_identity" => git_dir_identity = Some(identity_from(value)?),
            "note" => notes.push(unescape(value).ok_or(invalid())?),
            _ => return Err(RepositoryCreateError::InvalidInput("留痕记录未知字段")),
        }
    }
    let result = match result.ok_or(invalid())?.as_str() {
        "not-started" => RepositoryCreateResult::NotStarted,
        "spawn-failed" => RepositoryCreateResult::SpawnFailed,
        "succeeded" => RepositoryCreateResult::Succeeded,
        "failed" => RepositoryCreateResult::Failed {
            exit_code: match exit_code.as_deref() {
                Some("-") | None => None,
                Some(text) => Some(text.parse().map_err(|_| invalid())?),
            },
        },
        "unknown" => RepositoryCreateResult::Unknown {
            stop_confirmed: stop_confirmed.as_deref() == Some("1"),
            reason: match result_reason.as_deref() {
                Some("-") | None => return Err(invalid()),
                Some(text) => unescape(text).ok_or(invalid())?,
            },
        },
        _ => return Err(invalid()),
    };
    Ok(RepositoryOperationRecord {
        request_id: request_id.ok_or(invalid())?,
        operation: operation.ok_or(invalid())?,
        mode: mode.ok_or(invalid())?,
        target_root: target_root.ok_or(invalid())?,
        target: target.ok_or(invalid())?,
        source: source.ok_or(invalid())?,
        stage: stage.ok_or(invalid())?,
        result,
        terminal: terminal.ok_or(invalid())?,
        target_absent_before: absent.ok_or(invalid())?,
        target_empty_before: empty.ok_or(invalid())?,
        recorded_at: recorded_at.ok_or(invalid())?.ok_or(invalid())?,
        started_at: started_at.ok_or(invalid())?,
        observed_at: observed_at.ok_or(invalid())?,
        notes,
        target_root_identity: root_identity.ok_or(invalid())?,
        target_identity: target_identity.ok_or(invalid())?,
        git_dir_identity: git_dir_identity.ok_or(invalid())?,
    })
}

fn invalid() -> RepositoryCreateError {
    RepositoryCreateError::InvalidInput("留痕记录字段无效")
}

fn identity_from(text: &str) -> Result<Option<DirectoryIdentity>, RepositoryCreateError> {
    if text == "-" {
        return Ok(None);
    }
    DirectoryIdentity::from_hex(text)
        .map(Some)
        .ok_or_else(invalid)
}
