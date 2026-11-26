use super::*;

const MAGIC: &[u8] = b"SOXU-CKPT\0";
const REASONS: &[OmissionReason] = &[
    OmissionReason::Ignored,
    OmissionReason::SensitivePath,
    OmissionReason::SensitiveContent,
    OmissionReason::TextBudget,
    OmissionReason::BinaryNotAuthorized,
    OmissionReason::BinaryBudget,
    OmissionReason::TotalBudget,
    OmissionReason::PathBudget,
    OmissionReason::SpecialFile,
    OmissionReason::NestedRepository,
    OmissionReason::UnsupportedFilter,
    OmissionReason::UnsupportedEncoding,
    OmissionReason::InheritedSpecial,
    OmissionReason::IndividualBudget,
];

// 固定版本、长度前缀，路径与 Git 输出保留原始字节。
pub(super) fn encode(record: &CheckpointRecord) -> Result<Vec<u8>, CheckpointError> {
    let mut w = Writer(MAGIC.to_vec());
    w.number(record.schema_version.into());
    for value in [
        &record.workspace_id,
        &record.session_id,
        &record.checkpoint_id,
    ] {
        w.text(value);
    }
    w.number(record.turn_index.into());
    w.optional(record.parent_oid.as_deref());
    for value in [
        &record.base_tree_oid,
        &record.worktree_tree_oid,
        &record.commit_oid,
    ] {
        w.text(value);
    }
    w.number(record.changed_files.into());
    w.observation(&record.before)?;
    w.observation(&record.after)?;
    let coverage = &record.coverage;
    w.flag(coverage.complete);
    w.number(coverage.captured_bytes);
    w.text(&coverage.policy_version);
    w.number(coverage.included.len() as u64);
    for item in &coverage.included {
        w.bytes(&item.path);
        w.optional(item.blob_oid.as_deref());
        w.number(item.bytes);
        w.flag(item.warning);
        w.text(&item.representation);
    }
    w.number(coverage.omissions.len() as u64);
    for item in &coverage.omissions {
        w.bytes(&item.path);
        let reason = REASONS
            .iter()
            .position(|v| *v == item.reason)
            .ok_or_else(corrupt)?;
        w.number(reason as u64);
        w.flag(item.retains_base_entry);
    }
    w.text(&record.created_at);
    w.flag(record.lifecycle == CheckpointLifecycle::Published);
    Ok(w.0)
}

pub(super) fn decode(bytes: &[u8]) -> Result<CheckpointRecord, CheckpointError> {
    let mut r = Reader(bytes.strip_prefix(MAGIC).ok_or_else(corrupt)?);
    let schema_version = r.u32()?;
    if schema_version != 1 {
        return Err(corrupt());
    }
    let workspace_id = r.id()?;
    let session_id = r.id()?;
    let checkpoint_id = r.id()?;
    let turn_index = r.u32()?;
    let parent_oid = r.optional_oid()?;
    let base_tree_oid = r.oid()?;
    let worktree_tree_oid = r.oid()?;
    let commit_oid = r.oid()?;
    let changed_files = r.u32()?;
    let before = r.observation()?;
    let after = r.observation()?;
    let complete = r.flag()?;
    let captured_bytes = r.number()?;
    let policy_version = r.text()?;
    let mut included = Vec::new();
    for _ in 0..r.count()? {
        let path = r.git_path()?;
        included.push(CoveredPath {
            path,
            blob_oid: r.optional_oid()?,
            bytes: r.number()?,
            warning: r.flag()?,
            representation: r.text()?,
        });
    }
    let mut omissions = Vec::new();
    for _ in 0..r.count()? {
        let path = r.git_path()?;
        let code = usize::try_from(r.number()?).map_err(|_| corrupt())?;
        omissions.push(Omission {
            path,
            reason: *REASONS.get(code).ok_or_else(corrupt)?,
            retains_base_entry: r.flag()?,
        });
    }
    let created_at = r.text()?;
    let lifecycle = if r.flag()? {
        CheckpointLifecycle::Published
    } else {
        CheckpointLifecycle::Intent
    };
    if !r.0.is_empty() {
        return Err(corrupt());
    }
    Ok(CheckpointRecord {
        schema_version,
        workspace_id,
        session_id,
        checkpoint_id,
        turn_index,
        parent_oid,
        base_tree_oid,
        worktree_tree_oid,
        commit_oid,
        changed_files,
        before,
        after,
        coverage: CoverageManifest {
            complete,
            included,
            omissions,
            captured_bytes,
            policy_version,
        },
        created_at,
        lifecycle,
    })
}

struct Writer(Vec<u8>);
impl Writer {
    fn number(&mut self, n: u64) {
        self.0.extend_from_slice(&n.to_le_bytes());
    }
    fn flag(&mut self, flag: bool) {
        self.0.push(u8::from(flag));
    }
    fn bytes(&mut self, bytes: &[u8]) {
        self.number(bytes.len() as u64);
        self.0.extend_from_slice(bytes);
    }
    fn text(&mut self, text: &str) {
        self.bytes(text.as_bytes());
    }
    fn optional(&mut self, value: Option<&str>) {
        self.flag(value.is_some());
        if let Some(value) = value {
            self.text(value);
        }
    }
    fn observation(&mut self, o: &CheckpointObservation) -> Result<(), CheckpointError> {
        for path in [&o.worktree_root, &o.git_dir, &o.common_dir] {
            self.bytes(&platform::git_name(path.as_os_str()).ok_or_else(corrupt)?);
        }
        self.number(o.directory_identity.len() as u64);
        for v in &o.directory_identity {
            self.text(v);
        }
        self.bytes(&o.head);
        self.optional(o.index_digest.as_deref());
        self.text(&o.configuration_digest);
        self.text(&o.platform);
        Ok(())
    }
}

struct Reader<'a>(&'a [u8]);
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], CheckpointError> {
        let (front, rest) = self.0.split_at_checked(n).ok_or_else(corrupt)?;
        self.0 = rest;
        Ok(front)
    }
    fn number(&mut self) -> Result<u64, CheckpointError> {
        Ok(u64::from_le_bytes(
            self.take(8)?.try_into().map_err(|_| corrupt())?,
        ))
    }
    fn u32(&mut self) -> Result<u32, CheckpointError> {
        self.number()?.try_into().map_err(|_| corrupt())
    }
    fn count(&mut self) -> Result<u64, CheckpointError> {
        let n = self.number()?;
        if n > MAX_ENUMERATED_PATHS as u64 {
            return Err(corrupt());
        }
        Ok(n)
    }
    fn flag(&mut self) -> Result<bool, CheckpointError> {
        match self.take(1)?[0] {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(corrupt()),
        }
    }
    fn bytes(&mut self) -> Result<&'a [u8], CheckpointError> {
        let n = self.number()?.try_into().map_err(|_| corrupt())?;
        self.take(n)
    }
    fn text(&mut self) -> Result<String, CheckpointError> {
        String::from_utf8(self.bytes()?.to_vec()).map_err(|_| corrupt())
    }
    fn oid(&mut self) -> Result<String, CheckpointError> {
        git::oid(self.bytes()?)
    }
    fn optional_oid(&mut self) -> Result<Option<String>, CheckpointError> {
        if self.flag()? {
            Ok(Some(self.oid()?))
        } else {
            Ok(None)
        }
    }
    fn id(&mut self) -> Result<String, CheckpointError> {
        let value = self.text()?;
        if value.len() != 32 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(corrupt());
        }
        Ok(value)
    }
    fn path(&mut self) -> Result<PathBuf, CheckpointError> {
        let path = PathBuf::from(platform::native_component(self.bytes()?).ok_or_else(corrupt)?);
        if !path.is_absolute() {
            return Err(corrupt());
        }
        Ok(path)
    }
    fn git_path(&mut self) -> Result<Vec<u8>, CheckpointError> {
        let path = self.bytes()?.to_vec();
        super::super::capture::safe_path(&path)?;
        Ok(path)
    }
    fn observation(&mut self) -> Result<CheckpointObservation, CheckpointError> {
        let worktree_root = self.path()?;
        let git_dir = self.path()?;
        let common_dir = self.path()?;
        let mut directory_identity = Vec::new();
        for _ in 0..self.count()? {
            directory_identity.push(self.text()?);
        }
        Ok(CheckpointObservation {
            worktree_root,
            git_dir,
            common_dir,
            directory_identity,
            head: self.bytes()?.to_vec(),
            index_digest: self.optional_oid()?,
            configuration_digest: self.oid()?,
            platform: self.text()?,
        })
    }
}

fn corrupt() -> CheckpointError {
    CheckpointError::Invalid("Checkpoint 记录格式或版本无效")
}
