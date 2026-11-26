use std::fs::File;
#[cfg(any(target_os = "linux", test))]
use std::fs::OpenOptions;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

#[cfg(any(target_os = "linux", test))]
use super::CheckoutError;
use super::{CheckoutStage, CheckoutStatus};

const LIMIT: usize = 4096;

fn path(root: &Path, request: &str) -> PathBuf {
    root.join(format!("checkout-{request}.log"))
}

/// 读取持久检出记录。
///
/// 只把文件系统读取失败作为错误上报，调用方据此选择自己的错误类型并继续 fail-closed。
pub(super) fn read(root: &Path, request: &str) -> Result<CheckoutStatus, io::Error> {
    let path = path(root, request);
    let invalid = CheckoutStatus::Disabled {
        stage: None,
        reason: Some("invalid_record".to_owned()),
    };
    match path.symlink_metadata() {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(CheckoutStatus::Pending);
        }
        Err(error) => return Err(error),
        Ok(metadata)
            if !metadata.is_file()
                || crate::worktree_lifecycle::identity::is_reparse(&metadata) =>
        {
            return Ok(invalid);
        }
        Ok(_) => {}
    }
    let mut bytes = Vec::new();
    File::open(path)?
        .take((LIMIT + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > LIMIT {
        return Ok(invalid);
    }
    Ok(parse(&bytes))
}

enum Entry {
    Enter(CheckoutStage),
    Failure(String),
    Verified,
}

const STAGES: &[CheckoutStage] = &[
    CheckoutStage::Preflight,
    CheckoutStage::Materialize,
    CheckoutStage::PublishIndex,
    CheckoutStage::PublishFiles,
    CheckoutStage::Verify,
];

fn parse(bytes: &[u8]) -> CheckoutStatus {
    let mut stages = STAGES.iter();
    let mut stage = None;
    let mut reason = None;
    let mut terminated = false;
    let mut verified = false;
    for record in bytes.split_inclusive(|byte| *byte == b'\n') {
        if terminated || !record.ends_with(b"\n") {
            reason = Some("invalid_record".to_owned());
            verified = false;
            break;
        }
        match parse_entry(record) {
            Some(Entry::Enter(next)) if stages.next() == Some(&next) => stage = Some(next),
            Some(Entry::Failure(error)) if stage.is_some() => {
                reason = Some(error);
                terminated = true;
            }
            Some(Entry::Verified) if stage == Some(CheckoutStage::Verify) => {
                verified = true;
                terminated = true;
            }
            _ => {
                reason = Some("invalid_record".to_owned());
                break;
            }
        }
    }
    if verified {
        CheckoutStatus::Verified
    } else {
        CheckoutStatus::Disabled { stage, reason }
    }
}

// 持久格式只含固定阶段和内部原因代码，不保存 Git 输出或路径。
fn parse_entry(record: &[u8]) -> Option<Entry> {
    let text = std::str::from_utf8(record.strip_suffix(b"\n")?).ok()?;
    let entry = match text {
        "preflight" => Entry::Enter(CheckoutStage::Preflight),
        "materialize" => Entry::Enter(CheckoutStage::Materialize),
        "publish_index" => Entry::Enter(CheckoutStage::PublishIndex),
        "publish_files" => Entry::Enter(CheckoutStage::PublishFiles),
        "verify" => Entry::Enter(CheckoutStage::Verify),
        "verified" => Entry::Verified,
        _ => Entry::Failure(text.strip_prefix("failure ")?.to_owned()),
    };
    Some(entry)
}

#[cfg(any(target_os = "linux", test))]
pub(super) struct Journal(File);

#[cfg(any(target_os = "linux", test))]
impl Journal {
    pub(super) fn begin(root: &Path, request: &str) -> Result<Self, CheckoutError> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut journal = match options.open(path(root, request)) {
            Ok(file) => Self(file),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                return Err(CheckoutError::AlreadyAttempted(read(root, request)?));
            }
            Err(error) => return Err(error.into()),
        };
        journal.enter(CheckoutStage::Preflight)?;
        #[cfg(unix)]
        File::open(root)?.sync_all()?;
        Ok(journal)
    }

    pub(super) fn enter(&mut self, stage: CheckoutStage) -> io::Result<()> {
        self.write(match stage {
            CheckoutStage::Preflight => "preflight",
            CheckoutStage::Materialize => "materialize",
            CheckoutStage::PublishIndex => "publish_index",
            CheckoutStage::PublishFiles => "publish_files",
            CheckoutStage::Verify => "verify",
        })
    }

    pub(super) fn verified(&mut self) -> io::Result<()> {
        self.write("verified")
    }

    pub(super) fn failure(&mut self, error: &CheckoutError) -> io::Result<()> {
        let reason = match error {
            CheckoutError::Registration(_) => "registration_changed",
            CheckoutError::Io(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                "existing_entry"
            }
            CheckoutError::Io(_) => "filesystem",
            CheckoutError::Git(crate::GitError::Timeout) => "timeout",
            CheckoutError::Git(_) => "git_failed",
            CheckoutError::Unsafe(reason) => reason,
            CheckoutError::Cancelled => "cancelled",
            CheckoutError::StopUnconfirmed(reason) => {
                return self.write(&format!("failure stop_unconfirmed_{reason}"));
            }
            _ => "unexpected_error",
        };
        self.write(&format!("failure {reason}"))
    }

    fn write(&mut self, line: &str) -> io::Result<()> {
        use std::io::Write;
        self.0.write_all(format!("{line}\n").as_bytes())?;
        self.0.sync_all()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worktree_checkout::sandbox::tests::Fixture;

    #[test]
    fn records_stay_disabled_after_interruption_and_refuse_replay() {
        let fixture = Fixture::new();
        let root = fixture.path();
        assert_eq!(
            read(root, "request").expect("读取持久检出状态"),
            CheckoutStatus::Pending
        );
        let mut journal = Journal::begin(root, "request").expect("持久化夹具检出记录");
        journal
            .enter(CheckoutStage::Materialize)
            .expect("持久化夹具检出记录");
        journal
            .enter(CheckoutStage::PublishIndex)
            .expect("持久化夹具检出记录");
        journal
            .enter(CheckoutStage::PublishFiles)
            .expect("持久化夹具检出记录");
        drop(journal);
        let disabled = CheckoutStatus::Disabled {
            stage: Some(CheckoutStage::PublishFiles),
            reason: None,
        };
        assert_eq!(read(root, "request").expect("读取持久检出状态"), disabled);
        assert!(
            matches!(Journal::begin(root, "request"), Err(CheckoutError::AlreadyAttempted(status)) if status == disabled)
        );
    }

    #[test]
    fn preserves_cancellation_and_rejects_truncated_or_out_of_order_success() {
        let fixture = Fixture::new();
        let mut journal = Journal::begin(fixture.path(), "request").expect("持久化夹具检出记录");
        journal
            .failure(&CheckoutError::Cancelled)
            .expect("持久化夹具检出记录");
        assert_eq!(
            read(fixture.path(), "request").expect("读取持久检出状态"),
            CheckoutStatus::Disabled {
                stage: Some(CheckoutStage::Preflight),
                reason: Some("cancelled".to_owned()),
            }
        );
        for bytes in [
            b"preflight\nverified\n".as_slice(),
            b"preflight\nmaterialize\npublish_index\npublish_files\nverify\nverified",
        ] {
            assert!(matches!(parse(bytes), CheckoutStatus::Disabled { .. }));
        }
        let mut successful = Journal::begin(fixture.path(), "success").expect("持久化夹具检出记录");
        for stage in &STAGES[1..] {
            successful.enter(*stage).expect("持久化夹具检出记录");
        }
        successful.verified().expect("持久化夹具检出记录");
        assert_eq!(
            read(fixture.path(), "success").expect("读取持久检出状态"),
            CheckoutStatus::Verified
        );
    }
}
