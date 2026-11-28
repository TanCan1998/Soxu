//! 恢复执行的持久 journal：写入意图、阶段、结果与重启后的只读复核。
//!
//! 记录只含固定分类码、仓库与目标路径以及时间，不保存 Git 原始输出或凭据。
//! 每次阶段推进都是「独占创建临时文件 → fsync → 原子重命名 → 同步目录」，
//! 因此崩溃只会留下前一阶段的完整记录；残留临时文件一律失败闭合，不覆盖、不清扫。
//! 读取失败向上传播，绝不降级为「假定无记录」。

#[cfg(unix)]
use std::fs::File;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::worktree_lifecycle::identity::{
    AuthorizedDirectory, DirectoryIdentity, checked_directory, is_reparse,
};
use crate::worktree_lifecycle::{ExecutionResult, ExecutionStage, WorktreeLifecycleError};
use crate::{RecoveryOperation, WorktreeIdentity};

const HEADER: &[u8] = b"SOXURV1\0";
const MAX_JOURNAL_BYTES: usize = 64 * 1024;
const MAX_REASON_BYTES: usize = 128;
const MAX_TEXT_BYTES: usize = 512;
pub(crate) const MAX_JOURNAL_FILES: usize = 256;
pub(crate) const MAX_TOTAL_JOURNAL_BYTES: usize = 8 * 1024 * 1024;

/// 固定结果分类。宿主据此判断是否允许接入，不解析自由文本。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryNote {
    /// 确认已持久化，命令尚未启动（含启动前取消）。
    NotStarted,
    /// 命令成功且回读与预期一致。
    Verified,
    /// 命令成功，但回读显示预期之外的残留或漂移。
    PartialFootprint,
    /// 原生命令非零退出，足迹与预约保留。
    CommandFailed,
    /// 无法确认结局：启动失败、超时、输出超限、进程树未收敛或回读失败。
    UnknownOutcome,
    /// 持久化意图后、启动 Git 前取消，记录仍占用该执行 ID。
    Cancelled,
    /// 清理部分落盘：没有可证明的宿主资产需要处理。
    NothingToClean,
}

impl RecoveryNote {
    fn tag(self) -> u8 {
        match self {
            RecoveryNote::NotStarted => 0,
            RecoveryNote::Verified => 1,
            RecoveryNote::PartialFootprint => 2,
            RecoveryNote::CommandFailed => 3,
            RecoveryNote::UnknownOutcome => 4,
            RecoveryNote::Cancelled => 5,
            RecoveryNote::NothingToClean => 6,
        }
    }

    fn from_tag(tag: u8) -> Option<Self> {
        Some(match tag {
            0 => RecoveryNote::NotStarted,
            1 => RecoveryNote::Verified,
            2 => RecoveryNote::PartialFootprint,
            3 => RecoveryNote::CommandFailed,
            4 => RecoveryNote::UnknownOutcome,
            5 => RecoveryNote::Cancelled,
            6 => RecoveryNote::NothingToClean,
            _ => return None,
        })
    }
}

/// 一次恢复执行的可诊断记录（重启复核的唯一事实来源）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryExecutionRecord {
    pub execution_id: String,
    pub operation: RecoveryOperation,
    pub source: WorktreeIdentity,
    pub target: Option<PathBuf>,
    pub stage: ExecutionStage,
    pub result: ExecutionResult,
    pub recorded_at: SystemTime,
    pub started_at: Option<SystemTime>,
    pub observed_at: Option<SystemTime>,
    pub note: RecoveryNote,
    /// 结果未定或回读不符时置位：宿主在只读裁决前不得接入该工作树。
    pub blocked: bool,
}

impl RecoveryExecutionRecord {
    /// 未完成复核或进程结局未知时保留预约。旧记录的 `stop_confirmed`
    /// 只涉及直接子进程，不能据此解除预约。
    pub(crate) fn reserves_scope(&self) -> bool {
        self.stage != ExecutionStage::Observed
            || matches!(self.result, ExecutionResult::Unknown { .. })
    }
}

/// 持久记录 + 预览摘要绑定。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct JournalRecord {
    pub(crate) record: RecoveryExecutionRecord,
    pub(crate) preview_digest: [u8; 32],
    pub(crate) common_identity: Option<DirectoryIdentity>,
}

fn directory_path(state_root: &AuthorizedDirectory) -> PathBuf {
    state_root.path().join("recovery")
}

fn journal_path(directory: &AuthorizedDirectory, execution_id: &str) -> PathBuf {
    directory.path().join(format!("{execution_id}.journal"))
}

/// 建立并核验 journal 目录；已存在时逐项拒绝链接与 reparse point。
fn ensure_directory(
    state_root: &AuthorizedDirectory,
) -> Result<AuthorizedDirectory, WorktreeLifecycleError> {
    state_root.refresh()?;
    let path = directory_path(state_root);
    match path.symlink_metadata() {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            create_private_directory(&path)?;
            sync_directory(state_root)?;
        }
        Err(error) => return Err(error.into()),
    }
    existing_directory(state_root)?.ok_or(WorktreeLifecycleError::IdentityChanged)
}

fn existing_directory(
    state_root: &AuthorizedDirectory,
) -> Result<Option<AuthorizedDirectory>, WorktreeLifecycleError> {
    state_root.refresh()?;
    let directory = match checked_directory(&directory_path(state_root)) {
        Err(WorktreeLifecycleError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(None);
        }
        Err(error) => return Err(error),
        Ok(directory) => directory,
    };
    state_root.refresh()?;
    Ok(Some(directory))
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

/// journal 目录必须与协调域同样私有；显式 0700 不依赖调用方 umask。
fn create_private_directory(path: &Path) -> Result<(), WorktreeLifecycleError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new().mode(0o700).create(path)?;
    }
    #[cfg(not(unix))]
    std::fs::create_dir(path)?;
    Ok(())
}

fn write_atomic(
    directory: &AuthorizedDirectory,
    temporary: &Path,
    final_path: &Path,
    bytes: &[u8],
) -> Result<(), WorktreeLifecycleError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temporary)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.is_symlink() || is_reparse(&metadata) {
        return Err(WorktreeLifecycleError::InvalidInput("journal 临时文件类型"));
    }
    file.write_all(bytes)?;
    file.sync_all()?;
    std::fs::rename(temporary, final_path)?;
    sync_directory(directory)
}

/// 首次持久化一次执行意图。
pub(crate) fn begin(
    state_root: &AuthorizedDirectory,
    record: &JournalRecord,
) -> Result<(), WorktreeLifecycleError> {
    let directory = ensure_directory(state_root)?;
    let execution_id = valid_execution_id(&record.record.execution_id)?;
    let final_path = journal_path(&directory, execution_id);
    match final_path.symlink_metadata() {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
        Ok(_) => return Err(WorktreeLifecycleError::RequestConflict),
    }
    let bytes = encode(record)?;
    write_atomic(
        &directory,
        &directory.path().join(format!("{execution_id}.tmp")),
        &final_path,
        &bytes,
    )?;
    state_root.refresh()
}

/// 推进到下一阶段。
///
/// 上一次写入若在重命名前崩溃，会留下 `.next` 残留；此时拒绝覆盖，
/// 由宿主按 journal 中最后完整的阶段做只读裁决。
pub(crate) fn rewrite(
    state_root: &AuthorizedDirectory,
    record: &JournalRecord,
) -> Result<(), WorktreeLifecycleError> {
    let directory = ensure_directory(state_root)?;
    let execution_id = valid_execution_id(&record.record.execution_id)?;
    let final_path = journal_path(&directory, execution_id);
    let metadata = final_path.symlink_metadata()?;
    if !metadata.is_file() || metadata.is_symlink() || is_reparse(&metadata) {
        return Err(WorktreeLifecycleError::InvalidInput("journal 记录类型"));
    }
    let bytes = encode(record)?;
    write_atomic(
        &directory,
        &directory.path().join(format!("{execution_id}.next")),
        &final_path,
        &bytes,
    )?;
    state_root.refresh()
}

/// 读取全部记录；目录缺失（尚未建立协调域）视为无记录。
pub(crate) fn read_all(
    state_root: &AuthorizedDirectory,
) -> Result<Vec<JournalRecord>, WorktreeLifecycleError> {
    let Some(directory) = existing_directory(state_root)? else {
        return Ok(Vec::new());
    };
    let mut records = Vec::new();
    let mut total_bytes = 0usize;
    for (index, entry) in std::fs::read_dir(directory.path())?.enumerate() {
        if index >= MAX_JOURNAL_FILES {
            return Err(WorktreeLifecycleError::InvalidInput("journal 文件总量超限"));
        }
        let path = entry?.path();
        if path
            .extension()
            .is_none_or(|extension| extension != "journal")
        {
            continue;
        }
        let bytes = crate::worktree_lifecycle::state::read_regular_file(&path)?;
        total_bytes = total_bytes.saturating_add(bytes.len());
        if total_bytes > MAX_TOTAL_JOURNAL_BYTES {
            return Err(WorktreeLifecycleError::InvalidInput("journal 字节总量超限"));
        }
        let record = decode(&bytes)?;
        if path.file_stem().and_then(|stem| stem.to_str())
            != Some(record.record.execution_id.as_str())
        {
            return Err(WorktreeLifecycleError::InvalidInput(
                "journal 文件名与执行 ID 不一致",
            ));
        }
        records.push(record);
    }
    records.sort_by(|left, right| left.record.execution_id.cmp(&right.record.execution_id));
    state_root.refresh()?;
    Ok(records)
}

/// 读取单条记录；未知执行 ID 返回 `None`。
pub(crate) fn read(
    state_root: &AuthorizedDirectory,
    execution_id: &str,
) -> Result<Option<JournalRecord>, WorktreeLifecycleError> {
    let execution_id = valid_execution_id(execution_id)?;
    let Some(directory) = existing_directory(state_root)? else {
        return Ok(None);
    };
    let path = journal_path(&directory, execution_id);
    match path.symlink_metadata() {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
        Ok(metadata) if !metadata.is_file() || metadata.is_symlink() || is_reparse(&metadata) => {
            return Err(WorktreeLifecycleError::InvalidInput("journal 记录类型"));
        }
        Ok(_) => {}
    }
    let bytes = crate::worktree_lifecycle::state::read_regular_file(&path)?;
    let record = decode(&bytes)?;
    if record.record.execution_id != execution_id {
        return Err(WorktreeLifecycleError::InvalidInput(
            "journal 文件名与执行 ID 不一致",
        ));
    }
    state_root.refresh()?;
    Ok(Some(record))
}

fn valid_execution_id(execution_id: &str) -> Result<&str, WorktreeLifecycleError> {
    crate::worktree_lifecycle::validate_request_id(execution_id)?;
    Ok(execution_id)
}

fn encode(record: &JournalRecord) -> Result<Vec<u8>, WorktreeLifecycleError> {
    let stored = &record.record;
    let mut output = HEADER.to_vec();
    output.push(operation_tag(stored.operation));
    output.extend_from_slice(&record.preview_digest);
    push_field(&mut output, stored.execution_id.as_bytes())?;
    push_field(&mut output, path_bytes(&stored.source.root)?)?;
    push_field(&mut output, path_bytes(&stored.source.git_dir)?)?;
    push_field(&mut output, path_bytes(&stored.source.common_dir)?)?;
    match &stored.target {
        None => output.push(0),
        Some(target) => {
            output.push(1);
            push_field(&mut output, path_bytes(target)?)?;
        }
    }
    output.push(stage_tag(stored.stage));
    encode_result(&mut output, &stored.result)?;
    output.extend_from_slice(&nanos(stored.recorded_at)?.to_le_bytes());
    encode_time(&mut output, stored.started_at)?;
    encode_time(&mut output, stored.observed_at)?;
    output.push(u8::from(stored.blocked));
    output.push(stored.note.tag());
    if let Some(identity) = record.common_identity {
        output.extend_from_slice(&identity.to_bytes());
    }
    if output.len() > MAX_JOURNAL_BYTES {
        return Err(WorktreeLifecycleError::InvalidInput(
            "journal 记录超过大小限制",
        ));
    }
    Ok(output)
}

fn path_bytes(path: &Path) -> Result<&[u8], WorktreeLifecycleError> {
    std::str::from_utf8(path.as_os_str().as_encoded_bytes())
        .map(str::as_bytes)
        .map_err(|_| WorktreeLifecycleError::Unauthorized("持久路径必须为 UTF-8"))
}

fn push_field(output: &mut Vec<u8>, bytes: &[u8]) -> Result<(), WorktreeLifecycleError> {
    let length = u32::try_from(bytes.len())
        .map_err(|_| WorktreeLifecycleError::InvalidInput("journal 字段长度"))?;
    output.extend_from_slice(&length.to_le_bytes());
    output.extend_from_slice(bytes);
    Ok(())
}

fn encode_result(
    output: &mut Vec<u8>,
    result: &ExecutionResult,
) -> Result<(), WorktreeLifecycleError> {
    match result {
        ExecutionResult::NotStarted => output.push(0),
        ExecutionResult::SpawnFailed => output.push(1),
        ExecutionResult::Succeeded => output.push(2),
        ExecutionResult::Failed { exit_code } => {
            output.push(3);
            match exit_code {
                None => output.push(0),
                Some(code) => {
                    output.push(1);
                    output.extend_from_slice(&code.to_le_bytes());
                }
            }
        }
        ExecutionResult::Unknown {
            stop_confirmed,
            reason,
        } => {
            output.push(4);
            output.push(u8::from(*stop_confirmed));
            push_field(output, reason.as_bytes())?;
        }
    }
    Ok(())
}

fn encode_time(
    output: &mut Vec<u8>,
    time: Option<SystemTime>,
) -> Result<(), WorktreeLifecycleError> {
    match time {
        None => output.push(0),
        Some(time) => {
            output.push(1);
            output.extend_from_slice(&nanos(time)?.to_le_bytes());
        }
    }
    Ok(())
}

fn nanos(time: SystemTime) -> Result<u64, WorktreeLifecycleError> {
    let duration = time
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_err(|_| WorktreeLifecycleError::InvalidInput("系统时间早于纪元"))?;
    u64::try_from(duration.as_nanos())
        .map_err(|_| WorktreeLifecycleError::InvalidInput("系统时间超出可持久范围"))
}

fn operation_tag(operation: RecoveryOperation) -> u8 {
    match operation {
        RecoveryOperation::Unlock => 1,
        RecoveryOperation::Repair => 2,
        RecoveryOperation::Prune => 3,
        RecoveryOperation::RemoveRegistration => 4,
        RecoveryOperation::PartialCleanup => 5,
    }
}

fn operation_from_tag(tag: u8) -> Option<RecoveryOperation> {
    Some(match tag {
        1 => RecoveryOperation::Unlock,
        2 => RecoveryOperation::Repair,
        3 => RecoveryOperation::Prune,
        4 => RecoveryOperation::RemoveRegistration,
        5 => RecoveryOperation::PartialCleanup,
        _ => return None,
    })
}

fn stage_tag(stage: ExecutionStage) -> u8 {
    match stage {
        ExecutionStage::Prepared => 0,
        ExecutionStage::Started => 1,
        ExecutionStage::Completed => 2,
        ExecutionStage::Observed => 3,
    }
}

fn stage_from_tag(tag: u8) -> Option<ExecutionStage> {
    Some(match tag {
        0 => ExecutionStage::Prepared,
        1 => ExecutionStage::Started,
        2 => ExecutionStage::Completed,
        3 => ExecutionStage::Observed,
        _ => return None,
    })
}

struct Cursor<'a> {
    bytes: &'a [u8],
}

impl<'a> Cursor<'a> {
    fn array<const N: usize>(&mut self) -> Result<[u8; N], WorktreeLifecycleError> {
        let (head, rest) = self
            .bytes
            .split_first_chunk::<N>()
            .ok_or(WorktreeLifecycleError::InvalidInput("journal 记录截断"))?;
        self.bytes = rest;
        Ok(*head)
    }

    fn u8(&mut self) -> Result<u8, WorktreeLifecycleError> {
        Ok(self.array::<1>()?[0])
    }

    fn u64(&mut self) -> Result<u64, WorktreeLifecycleError> {
        Ok(u64::from_le_bytes(self.array::<8>()?))
    }

    fn i32(&mut self) -> Result<i32, WorktreeLifecycleError> {
        Ok(i32::from_le_bytes(self.array::<4>()?))
    }

    fn field(&mut self) -> Result<&'a [u8], WorktreeLifecycleError> {
        let length = u32::from_le_bytes(self.array::<4>()?) as usize;
        if length > self.bytes.len() {
            return Err(WorktreeLifecycleError::InvalidInput("journal 字段截断"));
        }
        let (field, rest) = self.bytes.split_at(length);
        self.bytes = rest;
        Ok(field)
    }

    fn text(&mut self, limit: usize) -> Result<String, WorktreeLifecycleError> {
        let field = self.field()?;
        if field.len() > limit || !field.iter().all(|byte| (0x20..0x7f).contains(byte)) {
            return Err(WorktreeLifecycleError::InvalidInput("journal 文本字段"));
        }
        String::from_utf8(field.to_vec())
            .map_err(|_| WorktreeLifecycleError::InvalidInput("journal 文本编码"))
    }

    fn path(&mut self) -> Result<PathBuf, WorktreeLifecycleError> {
        let field = self.field()?;
        let text = std::str::from_utf8(field)
            .map_err(|_| WorktreeLifecycleError::InvalidInput("journal 路径编码"))?;
        Ok(PathBuf::from(text))
    }

    fn finish(self) -> Result<(), WorktreeLifecycleError> {
        if self.bytes.is_empty() {
            Ok(())
        } else {
            Err(WorktreeLifecycleError::InvalidInput("journal 记录尾随数据"))
        }
    }
}

fn decode(bytes: &[u8]) -> Result<JournalRecord, WorktreeLifecycleError> {
    let mut cursor = Cursor {
        bytes: bytes
            .strip_prefix(HEADER)
            .ok_or(WorktreeLifecycleError::InvalidInput("journal 记录头"))?,
    };
    let operation = operation_from_tag(cursor.u8()?)
        .ok_or(WorktreeLifecycleError::InvalidInput("journal 操作类型"))?;
    let preview_digest = cursor.array::<32>()?;
    let execution_id = cursor.text(MAX_TEXT_BYTES)?;
    let root = cursor.path()?;
    let git_dir = cursor.path()?;
    let common_dir = cursor.path()?;
    let target = match cursor.u8()? {
        0 => None,
        1 => Some(cursor.path()?),
        _ => return Err(WorktreeLifecycleError::InvalidInput("journal 目标标记")),
    };
    let stage = stage_from_tag(cursor.u8()?)
        .ok_or(WorktreeLifecycleError::InvalidInput("journal 执行阶段"))?;
    let result = decode_result(&mut cursor)?;
    let recorded_at = time_from_nanos(cursor.u64()?)?;
    let started_at = decode_time(&mut cursor)?;
    let observed_at = decode_time(&mut cursor)?;
    let stored_blocked = match cursor.u8()? {
        0 => false,
        1 => true,
        _ => return Err(WorktreeLifecycleError::InvalidInput("journal 阻断标记")),
    };
    let note = RecoveryNote::from_tag(cursor.u8()?)
        .ok_or(WorktreeLifecycleError::InvalidInput("journal 结果分类"))?;
    // 早期 SOXURV1 记录没有共享 Git 目录身份，仍可只读诊断。
    let common_identity = if cursor.bytes.is_empty() {
        None
    } else {
        Some(
            DirectoryIdentity::from_bytes(&cursor.array::<{ DirectoryIdentity::ENCODED_LEN }>()?)
                .ok_or(WorktreeLifecycleError::InvalidInput("journal 仓库身份"))?,
        )
    };
    cursor.finish()?;
    let blocked = stored_blocked
        || stage != ExecutionStage::Observed
        || matches!(result, ExecutionResult::Unknown { .. });
    Ok(JournalRecord {
        record: RecoveryExecutionRecord {
            execution_id,
            operation,
            source: WorktreeIdentity {
                root,
                git_dir,
                common_dir,
            },
            target,
            stage,
            result,
            recorded_at,
            started_at,
            observed_at,
            note,
            blocked,
        },
        preview_digest,
        common_identity,
    })
}

fn decode_result(cursor: &mut Cursor<'_>) -> Result<ExecutionResult, WorktreeLifecycleError> {
    Ok(match cursor.u8()? {
        0 => ExecutionResult::NotStarted,
        1 => ExecutionResult::SpawnFailed,
        2 => ExecutionResult::Succeeded,
        3 => match cursor.u8()? {
            0 => ExecutionResult::Failed { exit_code: None },
            1 => ExecutionResult::Failed {
                exit_code: Some(cursor.i32()?),
            },
            _ => return Err(WorktreeLifecycleError::InvalidInput("journal 退出码标记")),
        },
        4 => {
            let stop_confirmed = match cursor.u8()? {
                0 => false,
                1 => true,
                _ => return Err(WorktreeLifecycleError::InvalidInput("journal 停止确认标记")),
            };
            let reason = cursor.text(MAX_REASON_BYTES)?;
            if reason.is_empty() {
                return Err(WorktreeLifecycleError::InvalidInput("journal 未知原因"));
            }
            ExecutionResult::Unknown {
                stop_confirmed,
                reason,
            }
        }
        _ => return Err(WorktreeLifecycleError::InvalidInput("journal 执行结果")),
    })
}

fn decode_time(cursor: &mut Cursor<'_>) -> Result<Option<SystemTime>, WorktreeLifecycleError> {
    match cursor.u8()? {
        0 => Ok(None),
        1 => Ok(Some(time_from_nanos(cursor.u64()?)?)),
        _ => Err(WorktreeLifecycleError::InvalidInput("journal 时间标记")),
    }
}

fn time_from_nanos(nanos: u64) -> Result<SystemTime, WorktreeLifecycleError> {
    SystemTime::UNIX_EPOCH
        .checked_add(Duration::from_nanos(nanos))
        .ok_or(WorktreeLifecycleError::InvalidInput("journal 时间"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // 协调域要求真实目录身份（dev/ino/birth），只有 Linux 建立该盘语义（L-013 / L-019）。
    #[cfg(target_os = "linux")]
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[cfg(target_os = "linux")]
    fn fixture_root() -> PathBuf {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "soxu-recovery-journal-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).expect("创建夹具目录");
        root.canonicalize().expect("夹具绝对路径")
    }

    #[cfg(target_os = "linux")]
    fn private(path: &Path) {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        if path.symlink_metadata().is_err() {
            std::fs::DirBuilder::new()
                .mode(0o700)
                .create(path)
                .expect("创建私有目录");
        }
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
            .expect("收紧目录权限");
    }

    fn sample(execution_id: &str) -> JournalRecord {
        JournalRecord {
            record: RecoveryExecutionRecord {
                execution_id: execution_id.to_owned(),
                operation: RecoveryOperation::Prune,
                source: WorktreeIdentity {
                    root: PathBuf::from("/fixture/repo"),
                    git_dir: PathBuf::from("/fixture/repo/.git"),
                    common_dir: PathBuf::from("/fixture/repo/.git"),
                },
                target: None,
                stage: ExecutionStage::Prepared,
                result: ExecutionResult::NotStarted,
                recorded_at: SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000),
                started_at: None,
                observed_at: None,
                note: RecoveryNote::NotStarted,
                blocked: true,
            },
            preview_digest: [7; 32],
            common_identity: None,
        }
    }

    #[test]
    fn round_trips_every_stage_and_result_shape() {
        for (stage, result, note, blocked) in [
            (
                ExecutionStage::Prepared,
                ExecutionResult::NotStarted,
                RecoveryNote::NotStarted,
                true,
            ),
            (
                ExecutionStage::Started,
                ExecutionResult::Unknown {
                    stop_confirmed: false,
                    reason: "running".to_owned(),
                },
                RecoveryNote::NotStarted,
                true,
            ),
            (
                ExecutionStage::Completed,
                ExecutionResult::Failed {
                    exit_code: Some(128),
                },
                RecoveryNote::CommandFailed,
                true,
            ),
            (
                ExecutionStage::Completed,
                ExecutionResult::SpawnFailed,
                RecoveryNote::UnknownOutcome,
                true,
            ),
            (
                ExecutionStage::Observed,
                ExecutionResult::Succeeded,
                RecoveryNote::Verified,
                false,
            ),
        ] {
            let mut record = sample("exec-roundtrip");
            record.record.stage = stage;
            record.record.result = result;
            record.record.note = note;
            record.record.blocked = blocked;
            record.record.target = Some(PathBuf::from("/fixture/target"));
            record.record.started_at =
                Some(SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_001));
            let bytes = encode(&record).expect("编码 journal");
            assert_eq!(decode(&bytes).expect("解码 journal"), record);
        }
    }

    #[test]
    fn legacy_unknown_remains_blocked_even_if_direct_child_was_reaped() {
        let mut old = sample("legacy-timeout");
        old.record.stage = ExecutionStage::Observed;
        old.record.blocked = false;
        old.record.result = ExecutionResult::Unknown {
            stop_confirmed: true,
            reason: "timeout".to_owned(),
        };
        let stored = decode(&encode(&old).expect("编码旧记录")).expect("解码旧记录");
        assert!(stored.record.blocked);
        assert!(stored.record.reserves_scope());
        assert_eq!(stored.record.result, old.record.result);
    }

    #[test]
    fn rejects_truncation_trailing_bytes_and_unknown_tags() {
        let bytes = encode(&sample("exec-corrupt")).expect("编码 journal");
        assert!(decode(&bytes[..bytes.len() - 1]).is_err());
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(decode(&trailing).is_err());
        let mut unknown_note = bytes.clone();
        let last = unknown_note.len() - 1;
        unknown_note[last] = 9;
        assert!(decode(&unknown_note).is_err());
        let mut bad_header = bytes;
        bad_header[0] ^= 0xff;
        assert!(decode(&bad_header).is_err());
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn crash_residue_keeps_last_stage_and_refuses_overwrite() {
        // 协调域目录身份与目录同步只在 Linux 有原生实现；Windows 由上层 fail-closed。
        let root = fixture_root();
        private(&root);
        let authorized = checked_directory(&root).expect("授权协调域");
        let mut record = sample("exec-residue");
        begin(&authorized, &record).expect("持久化 Prepared");
        record.record.stage = ExecutionStage::Started;
        record.record.result = ExecutionResult::Unknown {
            stop_confirmed: false,
            reason: "running".to_owned(),
        };
        rewrite(&authorized, &record).expect("推进 Started");
        assert_eq!(
            read(&authorized, "exec-residue")
                .expect("读取 journal")
                .expect("记录存在")
                .record
                .stage,
            ExecutionStage::Started
        );
        // 模拟重命名前崩溃：残留 .next 必须阻止继续写入，且旧阶段仍可读。
        std::fs::write(root.join("recovery/exec-residue.next"), b"partial").expect("写入残留");
        let mut next = record.clone();
        next.record.stage = ExecutionStage::Completed;
        assert!(matches!(
            rewrite(&authorized, &next),
            Err(WorktreeLifecycleError::Io(_))
        ));
        assert_eq!(
            read(&authorized, "exec-residue")
                .expect("读取 journal")
                .expect("记录存在")
                .record
                .stage,
            ExecutionStage::Started
        );
        // 未见记录时只读诊断返回 None，不创建任何文件。
        assert!(
            read(&authorized, "exec-absent")
                .expect("读取 journal")
                .is_none()
        );
        std::fs::remove_dir_all(root).expect("清理夹具");
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn rejects_reused_execution_id_and_surfaces_existing_record() {
        let root = fixture_root();
        private(&root);
        let authorized = checked_directory(&root).expect("授权协调域");
        let record = sample("exec-replay");
        begin(&authorized, &record).expect("持久化 Prepared");
        assert!(matches!(
            begin(&authorized, &record),
            Err(WorktreeLifecycleError::RequestConflict)
        ));
        let stored = read(&authorized, "exec-replay")
            .expect("读取 journal")
            .expect("记录存在");
        assert_eq!(stored.record, record.record);
        let all = read_all(&authorized).expect("读取全部记录");
        assert_eq!(all.len(), 1);
        std::fs::remove_dir_all(root).expect("清理夹具");
    }
}
